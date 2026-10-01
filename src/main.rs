//! The edge. Reads the world, executes effects, draws. Every decision it makes
//! is delegated to `varde::update` — see AGENTS.md.

mod pty;
mod rpc;
mod ui;

use anyhow::Result;
use clap::Parser as ClapParser;
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyEventKind, KeyboardEnhancementFlags, MouseButton, MouseEventKind,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::{execute, terminal};
use notify::{RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use terminput_crossterm::{to_terminput_key, to_terminput_mouse};
use varde::authorship;
use varde::editor;
use varde::format;
use varde::keys::{self, Drafts};
use varde::layout;
use varde::mouse;
use varde::preview;
use varde::reading;
use varde::review::{GitFile, GitStatus};
use varde::risk::{self, Figures, Metrics, Space};
use varde::startup::{self, Fact, FactValue, PathStatus, Startup, StartupError};
use varde::story;
use varde::tree::Entry;
use varde::{
    tmp_dir, tree, update, varde_dir, Direction, Effect, Event, Modal, Pane, ReplaceFailed, State,
};

/// Shown whenever there is nothing more urgent to say.
const HINT: &str = " Ctrl+Space or Esc Esc commands (e/r view · f find · a AI · t terminal · w write · s submit · q quit) · ^F search · / filter · gt/gT buffers · :q close · :qa quit · Alt+hjkl focus · Enter open · → row actions · n/N/d shortcuts";

#[derive(ClapParser)]
#[command(
    name = "varde",
    about = "A terminal IDE that reviews, tests and ships your work"
)]
struct Args {
    /// Folder to open as the workspace. Omitted, the current folder is opened
    /// as a Bare workspace: nothing of Varde's is written into it.
    folder: Option<PathBuf>,
    /// Print the programs Varde can be configured to run and what installs
    /// each on this OS, one tab-separated line each, and exit.
    #[arg(long, conflicts_with = "folder")]
    deps: bool,
    /// Print the template a new `~/.varde/config.toml` starts as, and exit.
    #[arg(long, conflicts_with_all = ["folder", "deps"])]
    default_config: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let varde_home = home().join(varde::VARDE_DIR);
    let global_config = config_layer(&varde_home.join(startup::CONFIG_FILE));
    if args.deps {
        let deps = match startup::deps(global_config.as_deref(), std::env::consts::OS) {
            Ok(deps) => deps,
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        };
        for dep in deps {
            let install = dep.install.unwrap_or_default();
            println!("{}\t{}\t{}\t{install}", dep.kind, dep.name, dep.command);
        }
        return Ok(());
    }
    if args.default_config {
        print!("{}", startup::template());
        return Ok(());
    }
    // Optional rather than defaulted to ".", because `varde` and `varde .`
    // name the same folder and must not mean the same thing: which it was is
    // the library's decision, and a default here would have thrown the fact
    // away before it could be reported (ADR 0016).
    let folder = args.folder.as_deref().unwrap_or(Path::new("."));
    let root = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let sidecar = args.folder.is_none().then(|| sidecar(&root));
    // Before this session's own is created, and on every start rather than only
    // a bare one: the directory swept is Varde's own and a project workspace
    // has no Sidecar to lose, so there is one rule instead of a condition.
    sweep();

    // Where this binary came from, for the update check. `varde` is installed as
    // a symlink into its checkout, so the executable is
    // <checkout>/target/release/varde — canonicalize explicitly, because macOS
    // does not promise `current_exe` resolves the link it was invoked through.
    // Resolved once, here: it is also the file `:update` replaces and relaunches,
    // and on Linux asking again after the replacement names a deleted inode.
    let exe = std::env::current_exe().and_then(std::fs::canonicalize).ok();
    let checkout = exe
        .as_ref()
        .and_then(|exe| exe.ancestors().nth(3).map(Path::to_path_buf));

    let input = Startup {
        path_status: path_status(folder),
        global_config,
        project_config: config_layer(
            &varde_dir(&root, sidecar.as_deref()).join(startup::CONFIG_FILE),
        ),
        state_json: read(&varde_dir(&root, sidecar.as_deref()).join(STATE_FILE)),
        risk_json: read(&varde_dir(&root, sidecar.as_deref()).join(varde::risk::FILE)),
        head: head_commit(&root),
        repo: git_status(&root),
        reviews: numbered(&varde::reviews_dir(&root, sidecar.as_deref(), &varde_home)),
        root: root.clone(),
        sidecar,
        varde_home,
        checkout_manifest: checkout
            .as_ref()
            .and_then(|checkout| read(&checkout.join("Cargo.toml"))),
        checkout,
        running_version: env!("CARGO_PKG_VERSION").to_string(),
        // Which install command a Tools row offers. Read here rather than
        // in the library, exactly as the version above is (R31.22).
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    };

    let (state, _config, startup_effects) = match startup::start(&input) {
        Ok(started) => started,
        Err(error) => {
            eprintln!("{}", describe(&error, folder));
            std::process::exit(1);
        }
    };

    run(state, root, exe, startup_effects)
}

/// Where a Bare workspace keeps what a project keeps in `.varde`. Keyed by the
/// folder *and* the process id: two Vardes on one folder must not share a
/// Sidecar, since quitting one deletes it, and a sweep of dead Sidecars needs a
/// live one to be recognisable. Both parts are only observable here, which is
/// why the path is derived at the edge and handed in. Only `home` and the pid
/// are read; the name is otherwise a function of the path, and `flatten` below
/// is where anything can be got wrong, which is why it is tested.
fn sidecar(root: &Path) -> PathBuf {
    sidecars().join(format!("{}-{}", flatten(root), std::process::id()))
}

fn sidecars() -> PathBuf {
    home().join(varde::VARDE_DIR).join("paths")
}

/// The process a Sidecar's name was built from, which is what the sweep reads
/// it for. The pid is the part after the *last* `-`, since a folder name may
/// hold one and `flatten` leaves it there. Zero is not a pid: `kill(0, …)`
/// asks about this process's own group, so a name ending `-0` would read as
/// alive forever and never be swept.
fn pid_of(name: &str) -> Option<u32> {
    name.rsplit_once('-')?
        .1
        .parse()
        .ok()
        .filter(|pid| *pid != 0)
}

/// Quitting deletes a Sidecar; a crash does not, so on start every Sidecar
/// whose Varde is gone is deleted. Everything under `~/.varde` is Varde's, so
/// everything in it can go (ADR 0014) — the one thing that must not go is the
/// Sidecar of an instance still running, and the *only* way to tell that one
/// apart is the process id in its name. That is why the path is keyed by pid
/// rather than by folder alone: keyed by folder, a second Varde on the same
/// folder would have its state swept out from under it by this very loop, and
/// quitting the first would delete the second's Sidecar besides. A name with
/// no pid in it has no live process either, so it goes too — one rule, no
/// exceptions to get wrong. Liveness is only observable here, which is why no
/// scenario covers this and only the parse above is unit tested. The one thing
/// it cannot see is a recycled pid: a crashed session whose number now belongs
/// to something else reads as alive and its Sidecar stays until that process
/// ends. A leak of a directory, never a deletion of a live one, which is the
/// side to be wrong on.
fn sweep() {
    let Ok(entries) = std::fs::read_dir(sidecars()) else {
        return;
    };
    for entry in entries.flatten() {
        if !pid_of(&entry.file_name().to_string_lossy()).is_some_and(alive) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Signal 0 delivers nothing and only asks whether the process is there.
/// `EPERM` is an answer, not a failure: the process exists and belongs to
/// somebody else, and deleting the Sidecar of a running Varde is the one
/// mistake this check exists to avoid.
fn alive(pid: u32) -> bool {
    let answer = unsafe { libc::kill(pid as libc::pid_t, 0) };
    answer == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// An absolute path as one directory name. The separator becomes `%`: not the
/// separator itself, or the whole tree of a machine's folders would grow under
/// `paths/`, and not `-`, which a folder name may hold and which would then
/// make two folders one Sidecar. A `%` already in a name is doubled for exactly
/// that reason — `a/b` and `a%b` are two folders and must stay two Sidecars.
fn flatten(root: &Path) -> String {
    root.to_string_lossy()
        .replace('%', "%%")
        .replace(std::path::MAIN_SEPARATOR, "%")
}

fn path_status(path: &Path) -> PathStatus {
    match std::fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => PathStatus::Missing,
        Err(_) => PathStatus::Unreadable,
        Ok(meta) if !meta.is_dir() => PathStatus::NotAFolder,
        Ok(_) => match std::fs::read_dir(path) {
            Ok(_) => PathStatus::Folder,
            Err(_) => PathStatus::Unreadable,
        },
    }
}

fn describe(error: &StartupError, path: &Path) -> String {
    match error {
        StartupError::Path("no-such-folder") => format!("No such folder: {}", path.display()),
        StartupError::Path("not-a-folder") => format!("Not a folder: {}", path.display()),
        StartupError::Path(_) => format!("Cannot read folder: {}", path.display()),
        // The words are the library's: which of the three faults it was is a
        // distinction the error carries, and a sentence composed here is a
        // sentence no test can read.
        StartupError::Config(problem) => problem.to_string(),
    }
}

/// The reviews a directory already holds, by number. Numbering from what is
/// there rather than from nothing: `~/.varde/reviews/` is shared by every Bare
/// workspace, so a session that starts at `0001` writes over the review
/// submitted from another folder. A name that is not `NNNN.json` is not a
/// review and is left out of the count, and out of the retention sweep with it.
fn numbered(dir: &Path) -> std::collections::BTreeSet<u32> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_suffix(".json")?
                .parse()
                .ok()
        })
        .collect()
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Either config layer, both of whose absence starting acts on: `None` is what
/// `start` reads as "there is no file to lose" before it seeds one (R9.7). `read`
/// answers `None` for every failure, not only for a file that is not there, so a
/// `config.toml` that exists and cannot be read — not UTF-8, or
/// write-only — would be overwritten with the seed and the reader's settings
/// would be gone. An empty layer merges nothing, so the effective config is
/// what it was either way, and the file survives to be fixed in another editor.
fn config_layer(path: &Path) -> Option<String> {
    read(path).or_else(|| path.try_exists().unwrap_or(false).then(String::new))
}

fn entries(folder: &Path) -> Vec<Entry> {
    std::fs::read_dir(folder)
        .map(|dir| {
            dir.flatten()
                .map(|item| Entry {
                    name: item.file_name().to_string_lossy().into_owned(),
                    is_dir: item.file_type().map(|kind| kind.is_dir()).unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Which of the tree's *listed* paths git ignores, so `tree::rows` can dim
/// them. Only listed paths are asked about: an answer is needed per row, and
/// walking an ignored folder to pre-answer for rows nobody expanded is the
/// walk this exists to make unnecessary. Not a repository means nothing is
/// ignored — the same answer git gives.
fn ignored(root: &Path, contents: &BTreeMap<PathBuf, Vec<Entry>>) -> BTreeSet<PathBuf> {
    let Ok(repository) = git2::Repository::open(root) else {
        return BTreeSet::new();
    };
    contents
        .iter()
        .flat_map(|(folder, entries)| entries.iter().map(|entry| folder.join(&entry.name)))
        .filter(|path| repository.is_path_ignored(path).unwrap_or(false))
        .collect()
}

/// git's answer to F7's question, or None when this is not a repository.
fn git_status(root: &Path) -> Option<Vec<GitFile>> {
    let repository = git2::Repository::open(root).ok()?;
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).include_ignored(false);
    let statuses = repository.statuses(Some(&mut options)).ok()?;
    Some(
        statuses
            .iter()
            .filter_map(|entry| {
                let path = entry.path().ok()?.to_string();
                let flags = entry.status();
                // First: an unmerged file is also modified in the index and
                // the tree, and what it needs is resolving.
                let status = if flags.is_conflicted() {
                    GitStatus::Conflicted
                } else if flags.is_wt_new() {
                    GitStatus::Untracked
                } else if flags.is_index_new() || flags.is_index_modified() {
                    GitStatus::Staged
                } else if flags.is_wt_modified() || flags.is_wt_deleted() {
                    GitStatus::Modified
                } else {
                    GitStatus::Committed
                };
                Some(GitFile { path, status })
            })
            .collect(),
    )
}

/// The Conflicts in each unmerged file's text as it is on disk. A file that
/// cannot be read as text is left out rather than read as having none, which
/// would tick it resolved: a binary file's Conflict is not one Varde can draw.
fn unmerged(root: &Path, files: &[GitFile]) -> BTreeMap<PathBuf, Vec<varde::conflict::Conflict>> {
    files
        .iter()
        .filter(|file| file.status == GitStatus::Conflicted)
        .filter_map(|file| {
            let path = root.join(&file.path);
            let text = std::fs::read_to_string(&path).ok()?;
            Some((path, varde::conflict::find(text.split('\n'))))
        })
        .collect()
}

/// The hunks the range under the spine is made of, at Varde's pinned diff
/// options, so binary detection, untracked content and hunk boundaries all
/// come from git2 rather than a second, hand-rolled read of blobs and files.
/// Computed on the same poll as `git_status` so the Remainder rides that
/// cadence rather than a second one.
///
/// Which two revisions that is, is [`story::inventory`]'s decision, and its
/// doc is where the reasoning lives.
///
/// Every file a loaded Story's Sites name also gets an entry even when it
/// carries no delta at all, since a Site's staleness needs the text at its own
/// range and not only whether a hunk overlaps it. That text's new side is the
/// working tree even over a committed range, unlike the hunks — see
/// [`story::staleness`] for why the two read different things.
fn file_hunks(root: &Path, inventory: story::Inventory, named: &[String]) -> Vec<story::FileHunks> {
    let Ok(repository) = git2::Repository::open(root) else {
        return Vec::new();
    };
    let tree = |revision: &str| {
        repository
            .revparse_single(revision)
            .ok()
            .and_then(|object| object.peel_to_tree().ok())
    };
    let head_tree = repository
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok());
    // A range between two real commits reads its own recorded, immutable
    // revisions, so an unrelated later commit elsewhere never moves either
    // side. Both or neither: a range with one revision resolved is a range
    // git cannot answer for, and letting the head alone fall through would
    // diff a recorded base against the working tree — a third answer nobody
    // asked for, arrived at by silence. The spine reports it `RangeGone`.
    let (base_tree, range_head_tree) = match inventory {
        story::Inventory::Committed { base, head } => match (tree(base), tree(head)) {
            (Some(base), Some(head)) => (Some(base), Some(head)),
            _ => (head_tree.clone(), None),
        },
        story::Inventory::Worktree => (head_tree.clone(), None),
    };

    let mut entries: BTreeMap<String, story::FileHunks> = BTreeMap::new();
    diff_entries(
        &repository,
        base_tree.as_ref(),
        range_head_tree.as_ref(),
        root,
        &mut entries,
    );
    story_entries(
        &repository,
        base_tree.as_ref(),
        range_head_tree.as_ref(),
        root,
        named,
        &mut entries,
    );
    entries.into_values().collect()
}

/// One file's text at one revision, or nothing when that revision does not
/// hold the path at all — a file the range added has no base side, one it
/// deleted has no head side.
fn blob_at(repository: &git2::Repository, tree: &git2::Tree, path: &Path) -> Option<String> {
    let blob = tree
        .get_path(path)
        .ok()?
        .to_object(repository)
        .ok()?
        .into_blob()
        .ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// One file's entry: its hunks, and its text at every end that asks. The old
/// side comes out of the range's base tree and the new side off disk, so a
/// file the change added has no old side and one it deleted has no new one.
/// The range's own new side is the head commit's copy when it has one and the
/// working tree's otherwise — [`story::FileHunks::head_text`] argues why that
/// is a separate read from the new side and not the same one.
fn hunks_entry(
    repository: &git2::Repository,
    base_tree: Option<&git2::Tree>,
    range_head_tree: Option<&git2::Tree>,
    root: &Path,
    path: &Path,
    file: String,
    hunks: Vec<story::Hunk>,
) -> story::FileHunks {
    let old_blob = base_tree.and_then(|tree| blob_at(repository, tree, path));
    let full = root.join(path);
    let new_exists = full.exists();
    let new_text = new_exists
        .then(|| std::fs::read_to_string(&full).ok())
        .flatten()
        .unwrap_or_default();
    let head_blob = range_head_tree.map(|tree| blob_at(repository, tree, path));
    story::FileHunks {
        file,
        hunks,
        old_exists: old_blob.is_some(),
        old_text: old_blob.unwrap_or_default(),
        head_exists: match &head_blob {
            Some(blob) => blob.is_some(),
            None => new_exists,
        },
        head_text: match head_blob {
            Some(blob) => blob.unwrap_or_default(),
            None => new_text.clone(),
        },
        new_exists,
        new_text,
    }
}

/// Every file the range changes, as git reports it. `range_head_tree` is the
/// commit a committed range ends at, and its absence is what makes the range
/// the working tree's own. `file_hunks` resolves the two together, so a `None`
/// base with a head present cannot arrive here — the first arm folds it in
/// rather than growing a case for a state nothing can reach.
fn diff_entries(
    repository: &git2::Repository,
    base_tree: Option<&git2::Tree>,
    range_head_tree: Option<&git2::Tree>,
    root: &Path,
    entries: &mut BTreeMap<String, story::FileHunks>,
) {
    let mut options = git2::DiffOptions::new();
    // The untracked options are inert on the tree-to-tree path — a commit has
    // no untracked files — and are set once rather than per arm so both paths
    // are demonstrably at the same context width.
    options
        .context_lines(story::CONTEXT_LINES)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
    let diff = match (base_tree, range_head_tree) {
        (base, Some(head)) => repository.diff_tree_to_tree(base, Some(head), Some(&mut options)),
        (Some(base), None) => {
            repository.diff_tree_to_workdir_with_index(Some(base), Some(&mut options))
        }
        (None, None) => repository.diff_index_to_workdir(None, Some(&mut options)),
    };
    let Ok(diff) = diff else {
        return;
    };
    for index in 0..diff.deltas().len() {
        let Some(patch) = git2::Patch::from_diff(&diff, index).ok().flatten() else {
            continue;
        };
        let delta = patch.delta();
        let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) else {
            continue;
        };
        let file = path.to_string_lossy().into_owned();
        let hunks = (0..patch.num_hunks())
            .filter_map(|hunk_index| patch.hunk(hunk_index).ok())
            .map(|(hunk, _lines)| story::Hunk {
                old_start: hunk.old_start(),
                old_lines: hunk.old_lines(),
                new_start: hunk.new_start(),
                new_lines: hunk.new_lines(),
            })
            .collect();
        entries.insert(
            file.clone(),
            hunks_entry(
                repository,
                base_tree,
                range_head_tree,
                root,
                path,
                file,
                hunks,
            ),
        );
    }
}

/// Every file the Story names, whether or not it carries a delta — a Site of
/// `context` kind points at code the range never touched, and a cited value
/// can point anywhere at all.
fn story_entries(
    repository: &git2::Repository,
    base_tree: Option<&git2::Tree>,
    range_head_tree: Option<&git2::Tree>,
    root: &Path,
    named: &[String],
    entries: &mut BTreeMap<String, story::FileHunks>,
) {
    for file in named {
        if entries.contains_key(file) {
            continue;
        }
        entries.insert(
            file.clone(),
            hunks_entry(
                repository,
                base_tree,
                range_head_tree,
                root,
                Path::new(file),
                file.clone(),
                Vec::new(),
            ),
        );
    }
}

impl Edge {
    /// The shell the core means by "the terminal": the one it says has the
    /// keyboard, bounded here as well because the core's count of them is a
    /// pass behind what this holds.
    fn shell(&mut self, split: usize) -> &mut pty::Pane {
        let last = self.shells.len() - 1;
        &mut self.shells[split.min(last)]
    }
}

/// What the bottom row is saying and how loudly. One field rather than two:
/// words and tone set separately drift, and a warning drawn in the notice
/// colour is a warning that reads as routine.
struct Status {
    text: String,
    tone: ui::Tone,
}

/// Where a Debug adapter answers: the port it listens on, or the command that
/// starts another one over stdio, which is the only way to have a second
/// conversation with an adapter that has no port.
#[derive(Clone)]
enum Again {
    Dial(u16),
    Spawn { command: String, args: Vec<String> },
}

struct Edge {
    /// The terminal strip's shells, side by side, never empty while the loop
    /// runs: the last one exiting is how Varde ends.
    shells: Vec<pty::Pane>,
    ai: Option<pty::Pane>,
    /// The debugged program's own terminal, in the Debug group beside the
    /// Variables. Held here and nowhere else, so the core is told it exists
    /// rather than remembering it was asked for.
    output: Option<pty::Pane>,
    root: PathBuf,
    /// The Bare workspace's Sidecar, as `main` derived it — the edge's own copy
    /// of what it told the core, for the effects it executes against a path of
    /// Varde's rather than one the core named.
    sidecar: Option<PathBuf>,
    status: Status,
    drafts: Drafts,
    clipboard: Option<arboard::Clipboard>,
    area: ratatui::layout::Rect,
    pointer: mouse::Pointer,
    cursor_style: &'static str,
    /// Tokens for the current buffer's text, kept until it changes. Re-parsing
    /// a whole file every frame is what made a big file feel heavy. The text
    /// they are for is the key; they are a parse of it once one has landed, and
    /// the last parse carried onto it until then (`highlight::carried`).
    highlighted: (PathBuf, u64, Vec<Vec<varde::highlight::Token>>),
    /// Where every parse puts its tokens and Run marks, off the main loop:
    /// syntect over a ten-thousand-line file is most of a second, and a
    /// restored workspace asks for one before the first key is read (#83),
    /// and a keystroke in that file asks again (#101).
    highlit: Sender<Parsed>,
    /// The parse in flight, if one is — one at a time (ADR 0023): a key
    /// pressed while a big file parses asks for the next parse once this one
    /// lands, rather than a thread per keystroke each most of a second long.
    parsing: Option<std::thread::JoinHandle<()>>,
    /// The text the last parse was asked of.
    parse_asked: (PathBuf, u64),
    /// The lines a Run mark stands on, found with the tokens and for their
    /// reason — a syntax tree is a parse too — and carried with them.
    run_marks: Vec<usize>,
    /// What the core's trace (`State::traced`) was last worked out against:
    /// what the buffer's commit holds, since a commit that moved changes the
    /// answer as surely as an edit does.
    trace_committed: Option<String>,
    /// The new and old sides of the diff under review, each parsed whole when
    /// the diff was read. No key: a diff is only ever on screen because a
    /// `ReadDiff` put it there, and that is the one place either side changes.
    diff_sides: (
        Vec<Vec<varde::highlight::Token>>,
        Vec<Vec<varde::highlight::Token>>,
    ),
    /// The current buffer's Preview rows, kept until the buffer changes or the
    /// pane does. Width is half the key because rows reflow: the same file at
    /// two widths is two different answers. "Never parse per frame" is sharper
    /// here than for tokens — a diagram is routed, not merely scanned.
    previewed: (PathBuf, u64, usize, Vec<varde::preview::Row>),
    faint: ratatui::style::Style,
    /// A query waiting for typing to settle.
    pending_search: Option<(String, Instant)>,
    /// When to tell the core that typing has paused long enough to be worth
    /// asking what may follow it. One deadline, overwritten by every keystroke,
    /// which is what makes a burst of typing one request. How long to wait is
    /// the core's number, carried by the effect: a delay chosen here is a delay
    /// no scenario can see.
    candidates_due: Option<Instant>,
    /// When to tell the core that the pointer has rested long enough to be
    /// worth asking what is under it. Overwritten by every cell the pointer
    /// crosses, for the reason above: the window is the core's number and the
    /// clock is the edge's.
    hover_due: Option<Instant>,
    /// Where a finished analysis puts its answer. The job runs off the main
    /// loop, so its result arrives as an event like any other fact only the
    /// edge can observe.
    analysed: Sender<(u64, Figures, Option<Figures>)>,
    /// Where a finished test run puts its verdict and its output — off the main
    /// loop for the same reason, and off the shell pane because the shell is
    /// the user's.
    tested: Sender<(bool, String)>,
    /// Where a finished formatter puts what it made of the Buffer, off the main
    /// loop for the reason the two above are: a `prettier` over a large file is
    /// hundreds of milliseconds, and a TUI that stops answering keys for one is
    /// a TUI that froze. The Buffer it was asked about rides along so the core
    /// can measure the answer against what is on screen now.
    formatted: Sender<(String, PathBuf, u64, format::Answer)>,
    /// Where the latest-Release request puts its body, off the main loop so a
    /// slow network never holds the first frame.
    released: Sender<Option<String>>,
    /// Where replacing the binary with a Release puts how it ended, off the
    /// main loop because an Asset is megabytes over whatever network there is.
    replaced: Sender<Result<(), ReplaceFailed>>,
    /// This binary, resolved at startup: what `:update` replaces and relaunches.
    exe: Option<PathBuf>,
    /// Leaving is a relaunch, which happens once the terminal is restored.
    relaunch: bool,
    /// One language server per language, held for as long as it is alive. This
    /// map *is* `State::lsp_running`: the core reads what the edge holds and
    /// never remembers that a process exists — the failure `ai_running` was, in
    /// a second shape (`docs/adr/0011-a-language-server-is-a-second-hosted-child.md`).
    servers: BTreeMap<String, rpc::Server>,
    /// The Debug adapter, held for as long as it is alive — the one fact about
    /// a Debug session only the edge can observe, told to the core as
    /// `Event::DapStarted` and `Event::DapGone`.
    adapter: Option<rpc::Adapter>,
    /// A server-reached adapter spawned and not yet listening. Holding it is
    /// holding the adapter: nothing is told the core until it connects, and
    /// letting it go is a site that stops holding one.
    connecting: Option<Receiver<std::io::Result<rpc::Adapter>>>,
    /// How the adapter was reached, so a child session's connection to it is
    /// reached the same way.
    again: Option<Again>,
    /// The child sessions' connections, by the number the core gave each, and
    /// the ones still being made. The adapter's: let go wherever it is.
    children: BTreeMap<usize, rpc::Adapter>,
    joining: BTreeMap<usize, Receiver<std::io::Result<rpc::Adapter>>>,
    /// A Waiting session's port being tried, off the main loop because a
    /// remote host that drops the packets holds a connect for as long as its
    /// timeout, and when it was last tried.
    probe: Option<Receiver<bool>>,
    probed: Instant,
    /// Which of the configured commands a probe of this process's `PATH` found,
    /// and `None` for "not probed since Tools was last opened" —
    /// which is what makes reopening the list a fresh answer rather than a
    /// cached one (R31.23).
    on_path: Option<BTreeSet<String>>,
    /// Whether this machine has a `git` binary, asked once. `None` until it
    /// has been asked — the `PATH` walk is not repeated per event, and unlike
    /// the probe above there is nothing about a clone that suggests git is
    /// about to be installed.
    git: Option<bool>,
    /// What this workspace holds for each fact the library names, and `None`
    /// for "not looked for yet". Kept rather than re-derived per event because
    /// it is a directory listing and every event would pay for it; dropped
    /// wherever the `PATH` probe is dropped, so a re-check after an install
    /// looks again (R31.27).
    facts: Option<(BTreeSet<PathBuf>, BTreeMap<String, String>)>,
    /// `~/.varde`, so the stream a Reading builds can be put under
    /// [`tmp_dir`] — outside every workspace, which is the whole of why
    /// nothing flickers in the file tree when Varde speaks (ADR 0014).
    varde_home: PathBuf,
    /// The `[speech]` rows, the edge's own copy of what it told the core, the
    /// way `sidecar` above is. `perform` holds no `State`, and which binary
    /// speaks and which one plays is the edge's business anyway: the core
    /// decides words and a pace, and names neither command (ADR 0013).
    speech: reading::Speech,
    /// The synthesizer, held for as long as it is alive. This *is*
    /// `State::voice_running`: a spawn that failed and a child that died both
    /// leave nothing here, and a flag the core set instead would report a
    /// voice that is not there — `ai_running`'s failure, avoided by not
    /// repeating it.
    voice: Option<Voice>,
    /// The player, while one is running. Apart from the stream below because
    /// the two no longer end together: a pause stops the player and keeps the
    /// stream, which is the whole of what makes resuming free (R35.6).
    playing: Option<std::process::Child>,
    /// The stream the Reading in flight was built into. Dropped only where its
    /// files are deleted — a stream outliving the Reading is the disk leak
    /// ADR 0014 is about.
    stream: Option<Stream>,
    /// Whether `PATH` holds the configured player, asked once — a player is
    /// not installed mid-Reading, and the refusal that says it is missing
    /// carries the install line that fixes it for good.
    player: Option<bool>,
    /// Each open buffer's Authorship and the commit it was read at. Cached here
    /// rather than read on every git poll: reading it walks a file's history,
    /// and only a new commit can change what it answers — which is why
    /// `Buffer::revision` is nowhere in this key, and why typing starts no walk
    /// (F40).
    authored: BTreeMap<PathBuf, (String, Arc<[authorship::Authored]>)>,
    /// The buffers whose Authorship is being read, and where each answer goes.
    /// Off the main loop: a blame of a file with a long history is half a
    /// second, and every buffer a restored workspace reopens asks for one on
    /// the first poll (#83).
    blaming: BTreeSet<PathBuf>,
    blamed: Sender<(PathBuf, String, Vec<authorship::Authored>)>,
    /// The git poll that is out, if one is. Diffing the working tree is tens of
    /// milliseconds on a branch with large uncommitted files, so the poll runs
    /// on a thread (#98), and one that falls due while the last is still out
    /// waits for it rather than queueing behind it (ADR 0023). A handle rather
    /// than a flag: a poll that panicked sends nothing, and a flag would wait
    /// for it forever and never read git again.
    polling: Option<std::thread::JoinHandle<()>>,
    polled: Sender<Polled>,
    /// The buffers the last poll was asked about, so a buffer opened since is
    /// asked now rather than in two seconds. What was *asked* rather than what
    /// came back: outside a repository the answer names no buffer at all, and
    /// keying on it asked again on every pump.
    asked: BTreeSet<PathBuf>,
}

/// A git poll's answer, read on the poll's thread. Everything but `workdir` is
/// told to the core as it came; `workdir` is what [`authorship`] needs to start
/// a walk without opening the repository on the main loop.
struct Polled {
    repo: Option<Vec<GitFile>>,
    /// The Conflicts in each file `repo` names unmerged, read off the disk.
    conflicts: BTreeMap<PathBuf, Vec<varde::conflict::Conflict>>,
    file_hunks: Arc<[story::FileHunks]>,
    ignored: BTreeSet<PathBuf>,
    branch: Option<String>,
    committed: BTreeMap<PathBuf, Option<String>>,
    head: Option<String>,
    workdir: Option<PathBuf>,
}

/// A Reading as audio, and everything a position report needs about it. The
/// core is told these rather than working them out: a voice's pace is in the
/// samples it produced, not in the text it was handed, so how long an Utterance
/// takes to say is the edge's only to know (R35.6, R35.12).
struct Stream {
    /// The whole Reading, kept across a pause so a resume has something to be
    /// rewritten from.
    whole: PathBuf,
    /// Where a seek's rewritten tail goes. One name, fixed when the stream is
    /// built rather than taken from whatever the player last held: a play from
    /// the start hands over `whole`, and a tail nobody is holding any more is
    /// still a file on the disk to delete (ADR 0014).
    tail: PathBuf,
    /// Where each Utterance begins, in milliseconds.
    offsets: Vec<u32>,
    /// Where the sound now running began within the Reading, and when — the
    /// two together are where it has got to.
    from_ms: u32,
    since: Instant,
}

/// A resident synthesizer. Loading a voice costs about as long as the whole
/// gesture's budget, so a child started on the first press pays that load on
/// the press that wanted sound (R35.11) — it is started when the first
/// markdown buffer opens instead.
///
/// It is spoken to a line at a time and answers with the path it wrote, which
/// is the framing that makes one child serve every Reading. What the line
/// means and which flags it was spawned with are the `[speech]` rows' to say.
struct Voice {
    child: std::process::Child,
    answers: std::io::BufReader<std::io::PipeReader>,
    /// The speed its arguments were filled with. The pace is baked in at
    /// synthesis, so a different one is a different child — which is also why
    /// a speed change applies to the next Reading and not the one playing
    /// (R35.7).
    speed: f32,
}

/// A resident child outlives the struct that held it otherwise, and there are
/// two ways to stop holding one: quitting, and a speed change, which is a new
/// child because the pace is baked in at synthesis. A 238MB process left
/// behind by either is the same leak, so it is answered once here rather than
/// at both sites.
impl Drop for Voice {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The ratatui terminal the loop draws through, named once so the functions
/// the loop calls can take it without spelling the backend out each time.
type Screen = ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>;

fn run(
    mut state: State,
    root: PathBuf,
    exe: Option<PathBuf>,
    startup_effects: Vec<Effect>,
) -> Result<()> {
    let title = root.file_name().map_or_else(
        || root.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    // Asked before raw mode, which the query sets and restores for itself. A
    // terminal that does not answer is not a failure: `ui::faint` falls back
    // to the terminal's own dimming.
    let palette = terminal_colorsaurus::color_palette(Default::default())
        .ok()
        .map(|palette| {
            [palette.foreground, palette.background].map(|colour| {
                let (r, g, b) = colour.scale_to_8bit();
                [r, g, b]
            })
        });
    let (out, enhanced) = enter_terminal(&title)?;
    state.reports_modifiers = false;

    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(out))?;
    let size = terminal.size()?;

    let (analysed_tx, analysed_rx) = channel();
    let (tested_tx, tested_rx) = channel();
    let (formatted_tx, formatted_rx) = channel();
    let (released_tx, released_rx) = channel();
    let (replaced_tx, replaced_rx) = channel();
    let (highlit_tx, highlit_rx) = channel();
    let (blamed_tx, blamed_rx) = channel();
    let (polled_tx, polled_rx) = channel();
    let mut edge = Edge {
        output: None,
        shells: vec![pty::Pane::spawn(
            &[],
            &root,
            &BTreeMap::new(),
            size.height / 3,
            size.width,
        )?],
        ai: None,
        root: root.clone(),
        sidecar: state.sidecar.clone(),
        status: Status {
            text: String::new(),
            tone: ui::Tone::Notice,
        },
        drafts: Drafts {
            ai: state.ai_command.clone(),
            ..Drafts::default()
        },
        clipboard: arboard::Clipboard::new().ok(),
        pointer: mouse::Pointer::default(),
        cursor_style: "",
        highlighted: (PathBuf::new(), u64::MAX, Vec::new()),
        highlit: highlit_tx,
        parsing: None,
        parse_asked: (PathBuf::new(), u64::MAX),
        run_marks: Vec::new(),
        trace_committed: None,
        diff_sides: (Vec::new(), Vec::new()),
        previewed: (PathBuf::new(), u64::MAX, 0, Vec::new()),
        faint: ui::faint(palette),
        pending_search: None,
        candidates_due: None,
        hover_due: None,
        servers: BTreeMap::new(),
        adapter: None,
        on_path: None,
        git: None,
        facts: None,
        varde_home: state.varde_home.clone(),
        speech: state.speech.clone(),
        voice: None,
        playing: None,
        stream: None,
        player: None,
        authored: BTreeMap::new(),
        blaming: BTreeSet::new(),
        blamed: blamed_tx,
        polling: None,
        polled: polled_tx,
        asked: BTreeSet::new(),
        area: ratatui::layout::Rect::new(0, 0, size.width, size.height),
        analysed: analysed_tx,
        tested: tested_tx,
        formatted: formatted_tx,
        released: released_tx,
        replaced: replaced_tx,
        exe,
        relaunch: false,
        probe: None,
        probed: Instant::now(),
        connecting: None,
        again: None,
        children: BTreeMap::new(),
        joining: BTreeMap::new(),
    };

    let (watch_tx, watch_rx) = channel();
    let mut watcher = notify::recommended_watcher(watch_tx)?;
    let mut watched: BTreeSet<PathBuf> = BTreeSet::new();

    // Over SSH there is no system clipboard, and copying has to reach the
    // machine the user is actually sitting at.
    state.system_clipboard = edge.clipboard.is_some();
    state.branch = head_branch(&root);
    state.file_hunks = file_hunks(
        state.repo_root(),
        story::inventory(&state.story_set),
        &story::named_files(&state.story_set),
    )
    .into();
    let mut queue: VecDeque<Event> = VecDeque::new();
    // Creating `.varde`, and asking for the figures nobody had to request.
    // Both wait for the edge to exist rather than running before the TUI opens:
    // the directory is wanted before anything writes into it, which is not
    // until this loop runs, and the analysis answers down a channel the edge
    // holds.
    for effect in startup_effects {
        perform(effect, state.split(), &mut edge, &mut queue);
    }
    queue.push_back(Event::Expand {
        entries: entries(&root),
        path: root.clone(),
    });

    let started = Instant::now();
    let mut last_git = Instant::now();
    let mut last_tick = Instant::now();
    let mut last_drag = Instant::now();
    let mut last_position = Instant::now();
    let mut last_shape = (0usize, 0usize);
    // Polling stays at 16ms so input latency is unchanged; it is the *draw* —
    // and the parse behind it — that now happens only when something moved.
    let mut dirty = true;
    let result = loop {
        match pump(&mut state, &mut edge, &mut queue, started) {
            Ok(true) => dirty = true,
            Ok(false) => {}
            Err(error) => break Err(error),
        }
        if !edge.shells.iter().any(|shell| shell.alive) {
            break Ok(());
        }
        if edge.status.text == "quit" {
            break Ok(());
        }

        sync_watches(&state, &mut watcher, &mut watched);
        let before = queue.len();
        collect_watch_events(&watch_rx, &state, &mut queue);
        collect_job_events(
            &analysed_rx,
            &tested_rx,
            &formatted_rx,
            &released_rx,
            &replaced_rx,
            &mut queue,
        );
        dirty |= queue.len() != before;
        dirty |= resize_panes(&mut terminal, &state, &mut edge, &mut queue);

        queue_tick(&state, &mut last_tick, &mut queue);
        dirty |= queue_held_drag(&state, &mut edge, &mut last_drag, &mut queue);
        dirty |= queue_settled_search(&state, &root, &mut edge, &mut queue);
        dirty |= queue_due_windows(&mut edge, &mut queue);
        dirty |= drain_panes(&state, &mut edge, &mut queue);
        dirty |= drain_servers(&mut edge, &mut queue);
        dirty |= drain_adapter(&mut edge, &mut queue);
        dirty |= probe_waiting(&state, &mut edge, &mut queue);
        start_voice(&state, &mut edge);
        reap_player(&mut edge, &mut queue);
        queue_position(&edge, &mut last_position, &mut queue);
        tell_core(&mut state, &mut edge);
        dirty |= refresh_git(
            &root,
            &mut state,
            &mut edge,
            &blamed_rx,
            &polled_rx,
            &mut last_git,
            &mut last_shape,
        );
        set_cursor_style(&state, &mut edge);
        dirty |= cache_highlight(&state, &mut edge, &highlit_rx);
        dirty |= tell_traced(&mut state, &mut edge);
        cache_preview(&state, &mut edge);

        if !dirty {
            continue;
        }
        dirty = false;
        render(&mut terminal, &state, &mut edge)?;
    };

    // What leaving means is the core's answer, asked again here because two of
    // the three ways out of that loop never reached `Event::Quit`: the pump
    // erroring, and the shell exiting. This used to be a `std::fs::write` of
    // `state_json` spelled out at the edge, which is a second author for the
    // one decision — and in a Bare workspace it wrote the state file back into
    // the Sidecar the core had just asked to have deleted, saved from being a
    // defect only by `write` not creating a parent. Asking twice costs nothing:
    // saving state is idempotent and a directory is removed once.
    for effect in update(&state, Event::QuitForce).1 {
        perform(effect, state.split(), &mut edge, &mut queue);
    }
    leave_terminal(enhanced);
    if !edge.relaunch {
        return result;
    }
    let Some(exe) = edge.exe.clone() else {
        return Err(anyhow::anyhow!(
            "relaunching: the running binary could not be located"
        ));
    };
    // Dropped first, so the panes' children end here as they do on a quit
    // rather than outliving an `exec` that runs no destructors.
    drop(edge);
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new(&exe)
        .args(std::env::args_os().skip(1))
        .exec();
    Err(anyhow::anyhow!("relaunching {}: {error}", exe.display()))
}

/// The one deliberate exception to draw-only-when-something-changed, and it is
/// bounded by the `if`: a job in flight is a redraw source, and when none is in
/// flight nothing here fires, so idle CPU is 0% exactly as before
/// (`docs/adr/0009-a-spinner-is-bounded-by-its-job.md`). Queued rather than
/// drawn: the tick goes through `pump` like every other event, so a tick
/// landing with a hundred wheel events is one frame and not a hundred and one.
/// Nothing about the thread reaches the core — only that time passed.
fn queue_tick(state: &State, last_tick: &mut Instant, queue: &mut VecDeque<Event>) {
    if state.risk.in_flight() && last_tick.elapsed() >= SPIN {
        *last_tick = Instant::now();
        queue.push_back(Event::Tick);
    }
}

/// The drag the pointer is holding against a pane's edge, reported again. A
/// terminal sends nothing while nothing moves, so a pointer held just past the
/// border would otherwise scroll one step and stop — and the step is the
/// library's to decide, which is why this replays the report it already routed
/// rather than working a row out here. Bounded exactly as the spinner above is:
/// `mouse::dragged` clears `held` on every drag that is not against an edge and
/// on the release, so with nothing held this fires nothing and idle CPU is
/// unchanged.
fn queue_held_drag(
    state: &State,
    edge: &mut Edge,
    last_drag: &mut Instant,
    queue: &mut VecDeque<Event>,
) -> bool {
    let Some(input) = edge.pointer.held else {
        return false;
    };
    if last_drag.elapsed() < SPIN {
        return false;
    }
    *last_drag = Instant::now();
    let before = queue.len();
    route_mouse(state, edge, input, queue);
    queue.len() != before
}

/// A query whose typing has stopped long enough to be worth searching for.
fn queue_settled_search(
    state: &State,
    root: &Path,
    edge: &mut Edge,
    queue: &mut VecDeque<Event>,
) -> bool {
    let Some((query, at)) = edge.pending_search.clone() else {
        return false;
    };
    if Instant::now() < at {
        return false;
    }
    edge.pending_search = None;
    queue.push_back(Event::Searched(run_search(state, root, &query)));
    true
}

/// The core's two windows, fired: typing that has stopped, and a pointer that
/// has stopped. Each deadline goes as it fires — the core arms a fresh one on
/// the next keystroke or the next cell, and one left behind would ask again
/// about text nobody typed or a symbol nobody is pointing at.
fn queue_due_windows(edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    let now = Instant::now();
    let mut fired = false;
    if edge.candidates_due.is_some_and(|due| now >= due) {
        edge.candidates_due = None;
        queue.push_back(Event::CandidatesDue);
        fired = true;
    }
    if edge.hover_due.is_some_and(|due| now >= due) {
        edge.hover_due = None;
        queue.push_back(Event::HoverDue);
        fired = true;
    }
    fired
}

/// Raw mode, the alternate screen, and the input modes Varde needs from the
/// host terminal. Returns the handle the backend takes and whether the Kitty
/// keyboard protocol was accepted — which teardown has to pop again.
fn enter_terminal(title: &str) -> Result<(std::io::Stdout, bool)> {
    use std::io::Write;
    terminal::enable_raw_mode()?;
    let mut out = std::io::stdout();
    // Bracketed paste is what makes a paste one event rather than a burst of
    // keystrokes indistinguishable from fast typing — without it Varde has
    // nothing to mark as a paste, and a multi-line paste is submitted a line at
    // a time by whatever is reading it.
    execute!(
        out,
        terminal::EnterAlternateScreen,
        terminal::SetTitle(title),
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    // Any-event tracking, on top of what `EnableMouseCapture` asks for: it
    // asks for button events (1002), where the terminal reports motion only
    // while a button is down, and a pointer coming to rest presses nothing. No
    // crossterm command spells this one, so it is the bytes.
    write!(out, "\x1b[?1003h")?;
    out.flush()?;
    // Ask for the Kitty keyboard protocol, but never for
    // REPORT_ALL_KEYS_AS_ESCAPE_CODES. That flag stops the terminal sending
    // *text* at all: every key arrives as its base keycode plus modifiers, and
    // the character the OS composed is only sent alongside if
    // REPORT_ASSOCIATED_TEXT is also set — which crossterm cannot ask for (the
    // flag is commented out in its bitflags) and could not deliver anyway,
    // since its CSI-u parser reads the modifier field and drops the text one.
    // So asking for it threw away every character that needs a composing key:
    // on a Norwegian layout Option+8/9 and Shift+Option+8/9 are the only way to
    // type [], {} and they were unreachable in every pane, Varde's own and
    // hosted alike, because the bytes never arrived. Anything a layout composes
    // — AltGr, dead keys, an IME — was lost the same way and just as silently.
    //
    // Without it the terminal composes and sends the character, and no bare
    // Ctrl press is reported anywhere — so `reports_modifiers` is false however
    // rich the terminal is, and the Ctrl double-tap is a gesture nothing can
    // perform. What replaced it is Ctrl+Space, claimed in `keys` before the
    // hosted-pane split so it opens the palette from every pane rather than
    // only the ones Varde interprets. Escape stays armed as the second way out
    // of a hosted pane. A wrong answer here is a pane nobody can leave.
    //
    // The cost of dropping the flag is that Option is no longer reported as
    // Alt on macOS, because the OS composes with it instead: the `M-hjkl`
    // focus aliases need `macos-option-as-alt` set in the terminal. Every pane
    // is reachable from the palette without a modifier, which is why that is
    // an alias rather than a regression.
    let enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(
            out,
            // DISAMBIGUATE_ESCAPE_CODES separates a real Escape from the prefix
            // of a sequence, which the Escape double-tap now rests on.
            // REPORT_ALTERNATE_KEYS still carries the layout's shifted
            // character on the modified keys that do arrive as CSI-u.
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS,
            )
        )?;
    }
    Ok((out, enhanced))
}

fn leave_terminal(enhanced: bool) {
    use std::io::Write;
    let mut out = std::io::stdout();
    if enhanced {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    // Whatever asked for motion has to give it back: `DisableMouseCapture`
    // spells the modes crossterm turned on and not this one, and a terminal
    // left in any-event tracking reports every move to whatever runs next.
    let _ = write!(out, "\x1b[?1003l");
    let _ = execute!(
        out,
        DisableBracketedPaste,
        DisableMouseCapture,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

/// What a finished job answers with. Both run off the main loop, so their
/// results arrive as events like any other fact only the edge can observe.
fn collect_job_events(
    analysed: &Receiver<(u64, Figures, Option<Figures>)>,
    tested: &Receiver<(bool, String)>,
    formatted: &Receiver<(String, PathBuf, u64, format::Answer)>,
    released: &Receiver<Option<String>>,
    replaced: &Receiver<Result<(), ReplaceFailed>>,
    queue: &mut VecDeque<Event>,
) {
    while let Ok(body) = released.try_recv() {
        queue.push_back(Event::ReleaseAnswered(body));
    }
    while let Ok(outcome) = replaced.try_recv() {
        queue.push_back(Event::BinaryReplaced(outcome));
    }
    while let Ok((generation, figures, before)) = analysed.try_recv() {
        queue.push_back(Event::RiskFigures {
            generation,
            figures,
            before,
        });
    }
    while let Ok((passed, output)) = tested.try_recv() {
        queue.push_back(Event::TestsFinished { passed, output });
    }
    while let Ok((language, path, revision, answer)) = formatted.try_recv() {
        queue.push_back(Event::FormatterAnswered {
            language,
            path,
            revision,
            answer,
        });
    }
}

/// Resize before feeding the child's output in: vt100 can panic if the grid
/// shrinks underneath a cursor that is still where the old size put it.
fn resize_panes(
    terminal: &mut Screen,
    state: &State,
    edge: &mut Edge,
    queue: &mut VecDeque<Event>,
) -> bool {
    let Ok(size) = terminal.size() else {
        return false;
    };
    let mut dirty = false;
    if (size.width, size.height) != (state.screen_width, state.screen_height) {
        queue.push_back(Event::Resized {
            width: size.width,
            height: size.height,
        });
        dirty = true;
    }
    edge.area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    let areas = ui::areas(edge.area, state);
    // One rule for every hosted pane, off the rectangles the renderer draws:
    // `pty_size` clamps the grid vt100 would panic on and answers `None` for a
    // pane with no rectangle at all — the Program output while it is hidden,
    // which squeezed to the floor would reflow everything it printed.
    let fit = |pane: &mut pty::Pane, width, height| {
        if let Some((rows, cols)) = varde::layout::pty_size(width, height) {
            pane.resize(rows, cols);
        }
    };
    for (shell, area) in edge.shells.iter_mut().zip(&areas.splits) {
        fit(shell, area.width, area.height);
    }
    if let Some(ai) = edge.ai.as_mut() {
        fit(ai, areas.ai.width, areas.ai.height);
    }
    if let Some(output) = edge.output.as_mut() {
        let area = areas.panes.output;
        fit(output, area.width, area.height);
    }
    dirty
}

/// Whatever the children have said since the last pass, and what their saying
/// it means: a CLI that has printed something is reading its stdin, and one
/// that has exited leaves a pty nobody may write to.
fn drain_panes(state: &State, edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    let mut dirty = false;
    let mut spoke = Vec::new();
    for shell in &mut edge.shells {
        let was_silent = !shell.spoken;
        dirty |= shell.drain();
        spoke.push(was_silent && shell.spoken);
    }
    // A split whose shell exited closes. The last one is left where it is:
    // its exiting is the loop's signal to end, and a strip with no shell at
    // all is a pane nothing can draw or write to. The flags go with their
    // shells, so a first word is reported under the index the core will see.
    if edge.shells.iter().any(|shell| shell.alive) {
        let mut alive = edge.shells.iter().map(|shell| shell.alive);
        spoke.retain(|_| alive.next().unwrap_or(true));
        edge.shells.retain(|shell| shell.alive);
    }
    // A shell that has printed its prompt is reading its input — what a
    // command held for a fresh split waits for, as a review waits for the AI.
    for (split, _) in spoke.iter().enumerate().filter(|(_, spoke)| **spoke) {
        queue.push_back(Event::ShellSpoke(split));
    }
    if let Some(output) = edge.output.as_mut() {
        // Every arrival, not only the first: out of sight each one is what
        // marks the `Debug` Group tab, and the core is the one that decides
        // whether the reader can see it.
        if output.drain() {
            dirty = true;
            queue.push_back(Event::OutputSpoke);
        }
        // The program ended, so the pane goes with it — the Debug group is the
        // Variables alone again.
        if !output.alive {
            edge.output = None;
            dirty = true;
        }
    }
    let Some(ai) = edge.ai.as_mut() else {
        return dirty;
    };
    let was_silent = !ai.spoken;
    dirty |= ai.drain();
    // A CLI that has printed something is reading its stdin; before that it
    // drops what it is sent, which is what a queued review waits for.
    if was_silent && ai.spoken {
        queue.push_back(Event::AiSpoke);
    }
    // The CLI exited: drop the dead pty so the pane goes back to asking which
    // one to start.
    if !ai.alive {
        edge.ai = None;
        edge.drafts.ai = state.ai_command.clone();
        queue.push_back(Event::AiExited);
    }
    dirty
}

/// Whatever the language servers have said since the last pass, and the ones
/// that are no longer there. A message is handed to the core exactly as it
/// arrived; a server that has gone is told, because the diagnostics it published
/// and the requests it never answered go with it.
fn drain_servers(edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    let mut dirty = false;
    let mut gone = Vec::new();
    for (language, server) in edge.servers.iter_mut() {
        for json in server.drain() {
            dirty = true;
            queue.push_back(Event::LspReceived {
                language: language.clone(),
                json,
            });
        }
        if !server.alive {
            gone.push(language.clone());
        }
    }
    for language in gone {
        edge.servers.remove(&language);
        dirty = true;
        queue.push_back(Event::LspGone {
            language,
            why: varde::lsp::Gone::Exited,
        });
    }
    dirty
}

/// Whatever the Debug adapter has said since the last pass, and whether it is
/// still there — `drain_servers` for the one adapter a session holds.
fn drain_adapter(edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    if let Some(connected) = &edge.connecting {
        let why = match connected.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Ok(Ok(adapter)) => {
                edge.connecting = None;
                edge.adapter = Some(adapter);
                queue.push_back(Event::DapStarted { from: 0 });
                return true;
            }
            Ok(Err(error)) => format!("the Debug adapter never listened: {error}"),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                "the Debug adapter's connection was lost".to_string()
            }
        };
        // The notice says the adapter failed; the reason is the edge's alone,
        // so it goes to the status line with it.
        edge.status = Status {
            text: why,
            tone: ui::Tone::Warning,
        };
        adapter_is_gone(edge, queue, varde::debug::Gone::FailedToStart);
        return true;
    }
    let Some(adapter) = edge.adapter.as_mut() else {
        return false;
    };
    let arrived = adapter.drain();
    let mut dirty = !arrived.is_empty() || !adapter.alive;
    queue.extend(
        arrived
            .into_iter()
            .map(|json| Event::DapReceived { json, from: 0 }),
    );
    if !adapter.alive {
        adapter_is_gone(edge, queue, varde::debug::Gone::Exited);
        return dirty;
    }
    // Each child session's connection the same way, told apart by its number.
    let mut joined = Vec::new();
    for (&child, joining) in &edge.joining {
        match joining.try_recv() {
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Ok(Ok(adapter)) => joined.push((child, Ok(adapter))),
            Ok(Err(error)) => joined.push((child, Err(error.to_string()))),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                joined.push((child, Err("the connection was lost".to_string())))
            }
        }
    }
    for (child, adapter) in joined {
        edge.joining.remove(&child);
        dirty = true;
        match adapter {
            Ok(adapter) => {
                edge.children.insert(child, adapter);
                queue.push_back(Event::DapStarted { from: child });
            }
            Err(why) => child_never_joined(edge, queue, child, why),
        }
    }
    let mut ended = Vec::new();
    for (&child, adapter) in edge.children.iter_mut() {
        let arrived = adapter.drain();
        dirty |= !arrived.is_empty() || !adapter.alive;
        queue.extend(
            arrived
                .into_iter()
                .map(|json| Event::DapReceived { json, from: child }),
        );
        if !adapter.alive {
            ended.push(child);
        }
    }
    for child in ended {
        edge.children.remove(&child);
        queue.push_back(Event::DapGone {
            why: varde::debug::Gone::Exited,
            from: child,
        });
    }
    dirty
}

/// A child session's connection that could not be made: the core hears it
/// failed, and the reason, which only the edge has, goes to the status line.
fn child_never_joined(edge: &mut Edge, queue: &mut VecDeque<Event>, child: usize, why: String) {
    edge.status = Status {
        text: format!("a child Debug session could not reach its adapter: {why}"),
        tone: ui::Tone::Warning,
    };
    queue.push_back(Event::DapGone {
        why: varde::debug::Gone::FailedToStart,
        from: child,
    });
}

/// The port a Waiting session watches, tried about once a second while — and
/// only while — `debug::waiting_on` names one, so a session nobody is waiting
/// on costs nothing. What answering means is the core's: the event says only
/// that it did.
fn probe_waiting(state: &State, edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    if let Some(probe) = &edge.probe {
        match probe.try_recv() {
            Ok(true) => {
                edge.probe = None;
                queue.push_back(Event::DapPortAnswers);
                return true;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Ok(false) | Err(std::sync::mpsc::TryRecvError::Disconnected) => edge.probe = None,
        }
    }
    // The clock restarts while nothing is Waiting, so the first try is a whole
    // interval after the program went: one still shutting down can hold its
    // port long enough to be attached to again on its way out.
    let Some((host, port)) = varde::debug::waiting_on(state) else {
        edge.probed = Instant::now();
        return false;
    };
    if edge.probed.elapsed() < PROBE {
        return false;
    }
    edge.probed = Instant::now();
    let (answered, probe) = channel();
    let host = host.to_string();
    std::thread::spawn(move || {
        use std::net::ToSocketAddrs;
        let answers = (host.as_str(), port)
            .to_socket_addrs()
            .into_iter()
            .flatten()
            .any(|address| std::net::TcpStream::connect_timeout(&address, PROBE).is_ok());
        let _ = answered.send(answers);
    });
    edge.probe = Some(probe);
    false
}

/// What the edge does wherever it stops holding an adapter: the session is
/// gone, and the debugged program goes with it. Dropping its pty is what takes
/// the child down — a program left running in a group nobody can reach is a
/// pane with no way back, `ai_running`'s failure from the other side. One
/// function for the two sites, so neither can remember only half of it.
fn adapter_is_gone(edge: &mut Edge, queue: &mut VecDeque<Event>, why: varde::debug::Gone) {
    edge.adapter = None;
    edge.connecting = None;
    edge.again = None;
    edge.children.clear();
    edge.joining.clear();
    edge.output = None;
    queue.push_back(Event::DapGone { why, from: 0 });
}

/// Tells the core what the last git poll found, and asks the next one. Nothing
/// here opens the repository: the poll reads a snapshot on its own thread and
/// answers down `polled`, and a finished poll redraws only when its answer
/// differs, so idle CPU stays at 0% (ADR 0023).
///
/// An answer read before the buffers or the Story moved is applied as it is and
/// corrected by the next poll, never merged with a fresher one: at most one poll
/// is out, so answers arrive in the order they were asked and none can land on
/// top of a newer one.
fn refresh_git(
    root: &Path,
    state: &mut State,
    edge: &mut Edge,
    blamed: &Receiver<(PathBuf, String, Vec<authorship::Authored>)>,
    polled: &Receiver<Polled>,
    last_git: &mut Instant,
    last_shape: &mut (usize, usize),
) -> bool {
    // An Authorship that has arrived is a poll now rather than up to two
    // seconds from now, so its border does not wait on the cadence.
    let mut due = false;
    while let Ok((path, at, lines)) = blamed.try_recv() {
        edge.blaming.remove(&path);
        edge.authored.insert(path, (at, lines.into()));
        due = true;
    }
    let mut dirty = false;
    // Asked before the channel is read: the poll sends before it ends, so one
    // that has ended with nothing waiting died without answering.
    let ended = edge.polling.as_ref().is_some_and(|poll| poll.is_finished());
    if let Ok(answer) = polled.try_recv() {
        edge.polling = None;
        let fresh_authorship = authorship(
            answer.workdir.as_deref(),
            state.buffers.keys(),
            answer.head.as_deref(),
            &mut edge.authored,
            &mut edge.blaming,
            &edge.blamed,
        );
        dirty = answer.repo != state.repo
            || !Arc::ptr_eq(&answer.file_hunks, &state.file_hunks)
            || answer.ignored != state.ignored
            || answer.branch != state.branch
            || answer.committed != state.committed
            || answer.conflicts != state.conflicts_on_disk
            || fresh_authorship != state.authorship;
        state.repo = answer.repo;
        state.file_hunks = answer.file_hunks;
        state.ignored = answer.ignored;
        state.committed = answer.committed;
        state.conflicts_on_disk = answer.conflicts;
        state.authorship = fresh_authorship;
        // A commit that moved is what makes the figure worth recomputing, so the
        // core is told on the same poll rather than remembering the commit it
        // started at. The workspace's commit, not the repository under review's:
        // `head` is Review view's base and the Risk delta's revision, and both of
        // those are about the folder on screen.
        state.head = answer.head;
        // Which branch that commit is on, told on the same poll and for the same
        // reason: only the edge can read it, and a `git switch` in the terminal
        // pane is a branch change Varde did not make.
        state.branch = answer.branch;
    } else if ended {
        edge.polling = None;
    }
    // A buffer opened since the last poll is not made to wait two seconds for
    // its change marks, nor a folder just listed for its ignored rows to dim —
    // listing one, or a file appearing or going, moves one of these two numbers.
    // A .gitignore edited under a tree that kept its shape waits for the cadence.
    let shape = (
        state.contents.len(),
        state.contents.values().map(Vec::len).sum::<usize>(),
    );
    due |= shape != *last_shape
        || state.buffers.keys().any(|path| !edge.asked.contains(path))
        || last_git.elapsed() > Duration::from_secs(2);
    if !due || edge.polling.is_some() {
        return dirty;
    }
    *last_git = Instant::now();
    *last_shape = shape;
    edge.asked = state.buffers.keys().cloned().collect();
    // The repository under review, which is the Guest repo when there is one:
    // a Step's staleness is judged against what its Site's lines hold in the
    // repository the Story describes, and which branch is checked out there is
    // what Story view says it is on. `git_status` and `ignored` stay the
    // workspace's — the tree's marks and what it hides are about the folder
    // Varde was opened on, not about a clone that is not in it.
    let repo = state.repo_root().to_path_buf();
    let range = match story::inventory(&state.story_set) {
        story::Inventory::Committed { base, head } => Some((base.to_string(), head.to_string())),
        story::Inventory::Worktree => None,
    };
    let named = story::named_files(&state.story_set);
    let (root, contents, buffers, told) = (
        root.to_path_buf(),
        state.contents.clone(),
        edge.asked.clone(),
        state.file_hunks.clone(),
    );
    let answer = edge.polled.clone();
    edge.polling = Some(std::thread::spawn(move || {
        let inventory = match &range {
            Some((base, head)) => story::Inventory::Committed { base, head },
            None => story::Inventory::Worktree,
        };
        // The hunks carry whole files' text, megabytes on a large change, so
        // they are compared here and an unchanged read hands back the very
        // `Arc` the core holds: the loop tells the two apart by pointer.
        let fresh = file_hunks(&repo, inventory, &named);
        let status = git_status(&root);
        let _ = answer.send(Polled {
            conflicts: unmerged(&root, status.as_deref().unwrap_or_default()),
            repo: status,
            file_hunks: if *fresh == *told { told } else { fresh.into() },
            ignored: ignored(&root, &contents),
            branch: head_branch(&repo),
            committed: committed(&root, buffers.iter()),
            head: head_commit(&root),
            workdir: git2::Repository::discover(&root)
                .ok()
                .and_then(|repository| repository.workdir().map(Path::to_path_buf)),
        });
    }));
    dirty
}

/// What the last commit holds for each open buffer, keyed by the buffer's own
/// path so the core needs no second spelling of it. `None` for a file the
/// commit does not hold; nothing at all outside a repository, which the core
/// reads the same way.
fn committed<'a>(
    root: &Path,
    buffers: impl Iterator<Item = &'a PathBuf>,
) -> BTreeMap<PathBuf, Option<String>> {
    let Ok(repository) = git2::Repository::discover(root) else {
        return BTreeMap::new();
    };
    let tree = repository
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok());
    let Some(workdir) = repository.workdir() else {
        return BTreeMap::new();
    };
    buffers
        .map(|path| {
            let text = tree
                .as_ref()
                .and_then(|tree| blob_at(&repository, tree, path.strip_prefix(workdir).ok()?));
            (path.clone(), text)
        })
        .collect()
}

/// Who last committed each line of each open buffer, in the commit `HEAD`
/// names. Nothing at all outside a repository, and an empty list for a file the
/// commit has no copy of — the core reads both as nobody's, the way it reads
/// [`committed`]'s two answers.
///
/// Cached against that commit, so the walk happens once per file per commit: the
/// buffer's edits cannot change what the commit holds, which is what keeps a
/// keystroke off git's history. A buffer that has closed takes its entry with
/// it, and a commit that moved drops the lot. The walk itself runs off the main
/// loop and answers into [`refresh_git`]; a file whose walk is out reads as
/// nobody's until it lands.
fn authorship<'a>(
    workdir: Option<&Path>,
    buffers: impl Iterator<Item = &'a PathBuf>,
    head: Option<&str>,
    cached: &mut BTreeMap<PathBuf, (String, Arc<[authorship::Authored]>)>,
    blaming: &mut BTreeSet<PathBuf>,
    blamed: &Sender<(PathBuf, String, Vec<authorship::Authored>)>,
) -> BTreeMap<PathBuf, Arc<[authorship::Authored]>> {
    let open: BTreeSet<&PathBuf> = buffers.collect();
    cached.retain(|path, (at, _)| Some(at.as_str()) == head && open.contains(path));
    let Some(head) = head else {
        return BTreeMap::new();
    };
    let Some(workdir) = workdir else {
        return BTreeMap::new();
    };
    for path in open {
        if cached.contains_key(path) || blaming.contains(path) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(workdir) else {
            continue;
        };
        blaming.insert(path.clone());
        let (relative, path, head) = (relative.to_path_buf(), path.clone(), head.to_string());
        let (workdir, answer) = (workdir.to_path_buf(), blamed.clone());
        std::thread::spawn(move || {
            let lines = git2::Repository::open(&workdir)
                .map(|repository| authored_lines(&repository, &relative))
                .unwrap_or_default();
            let _ = answer.send((path, head, lines));
        });
    }
    cached
        .iter()
        .map(|(path, (_, lines))| (path.clone(), lines.clone()))
        .collect()
}

/// One file's Authorship, a row per line of the file as the commit holds it —
/// git's `blame`, which is the one place that word belongs. Every hunk
/// contributes exactly the lines it covers, so nothing can slide a line onto the
/// wrong hand.
fn authored_lines(repository: &git2::Repository, relative: &Path) -> Vec<authorship::Authored> {
    let Ok(blamed) = repository.blame_file(relative, None) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for hunk in blamed.iter() {
        // A commit the blame names and the repository cannot find leaves the
        // whole file with no rows, so the border reads "Not committed yet" —
        // the same thing it reads for a file no commit holds. A repository this
        // broken is not a state worth a third wording, and the alternative is
        // every line after the bad hunk credited to the wrong hand.
        let Ok(commit) = repository.find_commit(hunk.final_commit_id()) else {
            return Vec::new();
        };
        let who = authorship::Authored {
            author: commit.author().name().unwrap_or_default().to_string(),
            date: short_date(commit.author().when()),
        };
        lines.extend(std::iter::repeat_n(who, hunk.lines_in_hunk()));
    }
    lines
}

/// A commit's authored date as `YYYY-MM-DD`, in the offset its author was at —
/// git's own `--date=short`. The day they wrote it, not the day it was wherever
/// the reader happens to be.
fn short_date(when: git2::Time) -> String {
    let stamp = chrono::DateTime::from_timestamp(when.seconds(), 0);
    let zone = chrono::FixedOffset::east_opt(when.offset_minutes() * 60);
    match (stamp, zone) {
        (Some(stamp), Some(zone)) => stamp.with_timezone(&zone).format("%Y-%m-%d").to_string(),
        _ => String::new(),
    }
}

/// The cursor's shape is how vim tells you which mode you are in.
fn set_cursor_style(state: &State, edge: &mut Edge) {
    let wanted = match state
        .current_buffer
        .as_ref()
        .and_then(|p| state.buffers.get(p))
    {
        // Only the editor's caret carries a mode. In a pty pane the shape is
        // the shell's business, and a leftover bar reads as insert mode.
        Some(buffer) if state.diff.is_none() && state.focus == Pane::Editor => match buffer.mode {
            editor::Mode::Insert => "bar",
            editor::Mode::Visual => "underline",
            editor::Mode::Normal => "block",
        },
        _ => "block",
    };
    if wanted == edge.cursor_style {
        return;
    }
    edge.cursor_style = wanted;
    let mut out = std::io::stdout();
    let _ = match wanted {
        "bar" => execute!(out, SetCursorStyle::SteadyBar),
        "underline" => execute!(out, SetCursorStyle::SteadyUnderScore),
        _ => execute!(out, SetCursorStyle::SteadyBlock),
    };
}

/// What a parse answers with: the file, its tokens and the lines its Run marks
/// stand on.
type Parsed = (PathBuf, Vec<Vec<varde::highlight::Token>>, Vec<usize>);

/// Parse once per edit, not once per frame, and never on the main loop.
///
/// A file the editor has just switched to is drawn plain and an edit to the
/// one on screen is drawn with the last parse carried onto the new text, while
/// the parse runs on a thread — so opening a big file, or typing in one, never
/// holds a key (#83, #101). A parse that lands for a text since edited is
/// carried onto it too, and the text as it is now asked for next. The Run
/// marks go with the tokens both ways (#104).
fn cache_highlight(state: &State, edge: &mut Edge, highlit: &Receiver<Parsed>) -> bool {
    let current = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path).map(|buffer| (path, buffer)));
    // Asked before the channel is read, for the reason the git poll asks it: a
    // parse sends before it ends, so one that has ended with nothing waiting
    // died without answering.
    let ended = edge
        .parsing
        .as_ref()
        .is_some_and(|parse| parse.is_finished());
    let mut arrived = false;
    while let Ok((path, tokens, marks)) = highlit.try_recv() {
        edge.parsing = None;
        let Some((_, buffer)) = current.filter(|(current, _)| **current == path) else {
            continue;
        };
        let (tokens, marks) = varde::highlight::carried(tokens, marks, buffer.shown());
        edge.highlighted = (path, buffer.revision(), tokens);
        edge.run_marks = marks;
        arrived = true;
    }
    if ended {
        edge.parsing = None;
    }
    let Some((path, buffer)) = current else {
        if !edge.highlighted.2.is_empty() {
            edge.highlighted = (PathBuf::new(), u64::MAX, Vec::new());
            edge.run_marks.clear();
        }
        return arrived;
    };
    if (&edge.highlighted.0, edge.highlighted.1) != (path, buffer.revision()) {
        let (tokens, marks) = match edge.highlighted.0 == *path {
            true => varde::highlight::carried(
                std::mem::take(&mut edge.highlighted.2),
                std::mem::take(&mut edge.run_marks),
                buffer.shown(),
            ),
            false => (varde::highlight::plain(buffer.shown()), Vec::new()),
        };
        edge.highlighted = (path.clone(), buffer.revision(), tokens);
        edge.run_marks = marks;
    }
    let asked = (path.clone(), buffer.revision());
    if edge.parsing.is_none() && edge.parse_asked != asked {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let (text, runs, answer) = (
            buffer.shown().to_string(),
            state.runs.clone(),
            edge.highlit.clone(),
        );
        let path = path.clone();
        edge.parsing = Some(std::thread::spawn(move || {
            let tokens = varde::highlight::highlight(&name, &text);
            let marks = varde::run::marks_in(&runs, &path, &text)
                .into_keys()
                .collect();
            let _ = answer.send((path, tokens, marks));
        }));
        edge.parse_asked = asked;
    }
    arrived
}

/// Tell the core the current buffer traced through its commit
/// (`State::traced`), once per revision and once per commit, before the frame
/// that draws it. On the loop rather than a thread: a trace that landed a
/// keystroke late put every Change bar below an added line on the wrong line
/// (#104). About 7 ms on an edit to the 600 KB `src/lib.rs`.
fn tell_traced(state: &mut State, edge: &mut Edge) -> bool {
    let Some((path, buffer)) = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path).map(|buffer| (path, buffer)))
    else {
        return false;
    };
    let committed = state.committed.get(path).and_then(Option::as_deref);
    let told = state
        .traced
        .as_ref()
        .is_some_and(|(traced, revision, _)| (traced, *revision) == (path, buffer.revision()));
    if told && edge.trace_committed.as_deref() == committed {
        return false;
    }
    let lines = varde::authorship::traced(committed, buffer.shown());
    edge.trace_committed = committed.map(str::to_string);
    state.traced = Some((path.clone(), buffer.revision(), lines.into()));
    true
}

/// Lay the document out once per edit and once per resize, not once per frame.
/// Asked of the same `State` the renderer will read, so the rows on screen are
/// the rows the core would answer with.
fn cache_preview(state: &State, edge: &mut Edge) {
    let current = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path).map(|buffer| (path, buffer)))
        .filter(|_| varde::previewing(state));
    let Some((path, buffer)) = current else {
        if !edge.previewed.3.is_empty() {
            edge.previewed = (PathBuf::new(), u64::MAX, 0, Vec::new());
        }
        return;
    };
    let columns = varde::preview_columns(state);
    let key = (path.clone(), buffer.revision(), columns);
    if (&edge.previewed.0, edge.previewed.1, edge.previewed.2) != (&key.0, key.1, key.2) {
        edge.previewed = (key.0, key.1, key.2, varde::preview_rows(state));
    }
}

fn render(terminal: &mut Screen, state: &State, edge: &mut Edge) -> Result<()> {
    let rows = tree::visible_rows(state);
    terminal.draw(|frame| {
        edge.area = frame.area();
        // The command line renders inside the editor pane; the bottom row keeps
        // the status or, failing that, the way out.
        let (status, tone) = if edge.status.text.is_empty() {
            (HINT.to_string(), ui::Tone::Hint)
        } else {
            (edge.status.text.clone(), edge.status.tone)
        };
        ui::draw(
            frame,
            state,
            &rows,
            &edge.shells,
            edge.ai.as_ref(),
            edge.output.as_ref(),
            ui::Chrome {
                status: &status,
                tone,
                name_draft: &edge.drafts.name,
                command_draft: edge.drafts.command.as_deref(),
                ai_draft: &edge.drafts.ai,
                comment_kind: &edge.drafts.comment_kind,
                filter_draft: edge.drafts.filter.as_deref(),
                tokens: &edge.highlighted.2,
                run_marks: &edge.run_marks,
                diff_new: &edge.diff_sides.0,
                diff_old: &edge.diff_sides.1,
                preview: &edge.previewed.3,
                faint: edge.faint,
            },
        );
    })?;
    Ok(())
}

/// What survives a quit, in [`varde_dir`]: the buffers that were open, the
/// selection, the view. Read once at startup and written once on the way out.
const STATE_FILE: &str = "state.json";

/// How many input events one frame may absorb. A cap, so input that never
/// stops cannot starve the drawing.
const INPUT_BATCH: usize = 512;

/// How often the spinner turns while a job is in flight. Fast enough to read as
/// motion, slow enough that a frame's cost is a rounding error next to the
/// analysis it is reporting on.
const SPIN: Duration = Duration::from_millis(80);

/// How often a Waiting session's port is tried, and how long one try may take.
const PROBE: Duration = Duration::from_secs(1);

/// How far below the workspace root [`beneath`] looks for a fact's marker.
/// Three, because `apps/web/frontend` is as deep as a package sits and this is
/// the one search with no file to start from — the depth is what keeps it a few
/// dozen questions of the filesystem rather than a walk of the workspace.
const SPREAD: usize = 3;

/// One thing crossterm reported, as the events it means. No decision of its
/// own: which `Event` a key or a click is belongs to `keys` and `mouse`.
fn translate_input(
    state: &mut State,
    edge: &mut Edge,
    input: event::Event,
    started: Instant,
    queue: &mut VecDeque<Event>,
) {
    match input {
        event::Event::Key(key) if key.kind != KeyEventKind::Release => {
            translate_key(state, edge, key, started, queue);
        }
        event::Event::Mouse(mouse) => translate_mouse(state, edge, mouse, started, queue),
        event::Event::Paste(text) => translate_paste(state, edge, text, started, queue),
        _ => {}
    }
}

/// A paste is one event to a child that asked to be told about pastes, and a
/// run of keys to anything else — each interpreted against the state the key
/// before it left, which is why `drain` runs inside the loop.
fn translate_paste(
    state: &mut State,
    edge: &mut Edge,
    text: String,
    started: Instant,
    queue: &mut VecDeque<Event>,
) {
    match keys::on_paste(state, &edge.drafts, text) {
        // Two destinations, one arm: both are one event, and which one it is is
        // the library's decision to have made rather than this loop's to
        // repeat.
        keys::Pasted::ToChild(event) | keys::Pasted::ToBuffer(event) => queue.push_back(event),
        keys::Pasted::AsKeys(events) => {
            for key in events {
                queue.extend(keys::on_key_event(
                    state,
                    &mut edge.drafts,
                    key,
                    started.elapsed().as_millis() as u64,
                ));
                drain(state, edge, queue);
            }
        }
    }
}

/// Reads input, runs it through `update`, and executes what comes back.
fn pump(
    state: &mut State,
    edge: &mut Edge,
    queue: &mut VecDeque<Event>,
    started: Instant,
) -> Result<bool> {
    let mut read_input = false;
    let mut worked = false;
    if event::poll(Duration::from_millis(16))? {
        read_input = true;
        // Everything already waiting is read before anything is drawn. A
        // trackpad flick arrives as hundreds of wheel events and a frame costs
        // milliseconds, so drawing one per event is a backlog that takes
        // seconds to clear — which is what read as the TUI freezing.
        let mut budget = INPUT_BATCH;
        loop {
            translate_input(state, edge, event::read()?, started, queue);
            // One frame per batch, but one *interpretation* per key: what a key
            // means depends on the state the key before it left. Deciding a
            // whole batch against the state it started with is why `/` opened
            // an in-file search and the letters pasted behind it went to the
            // buffer instead of the query.
            worked |= drain(state, edge, queue);
            budget -= 1;
            if budget == 0 || !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }

    // Events also arrive from the watcher, the ptys and the effects themselves,
    // with no input to interleave them with.
    worked |= drain(state, edge, queue);
    Ok(worked || read_input)
}

/// Runs what is queued through `update` and executes what comes back.
fn drain(state: &mut State, edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    let mut worked = false;
    while let Some(next_event) = queue.pop_front() {
        worked = true;
        let (next, effects) = update(state, next_event);
        *state = next;
        // The one `[speech]` value the core writes is the voice an install
        // configured, and the synthesizer the edge starts has to load it.
        if edge.speech != state.speech {
            edge.speech = state.speech.clone();
        }
        for effect in effects {
            perform(effect, state.split(), edge, queue);
        }
        // An effect can start or end a session, so the core is told again
        // before the next event is decided against a pane it no longer has.
        tell_core(state, edge);
    }
    worked
}

/// Everything about the children only the edge can see, handed to the core,
/// which reads these fields and never writes them. `ai_running` was the
/// exception — core state, set when a spawn was *asked for* and cleared by an
/// event — and a spawn that failed cleared nothing, so the core routed keys to a
/// pane the edge did not hold and refused `:ai` because a session was "already
/// running". Derived from the pane, it cannot say a session exists when none
/// does. Losing one is still announced separately: every site below that stops
/// holding a pane queues `AiExited`, because a review queued for that session
/// must not be handed to the next one.
///
/// The `PATH` probe below runs from here rather than beside the two call sites
/// this function has, for that same reason turned around: two calls that must
/// stay together are two calls that can come apart, and the core would then
/// read a set the edge had not refreshed.
fn tell_core(state: &mut State, edge: &mut Edge) {
    probe_path(state, edge);
    state.commands_on_path = edge.on_path.clone().unwrap_or_default();
    // Which directories the search starts from: the file being served, for
    // every buffer that is open. A monorepo installs its toolchain per package,
    // so a fact resolved from the root alone is the wrong answer wherever the
    // root has none — and never above the root, which would let a toolchain
    // outside the workspace serve a file inside it.
    let from: BTreeSet<PathBuf> = state
        .buffers
        .keys()
        .filter_map(|path| path.parent())
        .filter(|dir| dir.starts_with(&edge.root))
        .map(Path::to_path_buf)
        .collect();
    // Kept against the directories it was searched from rather than
    // recomputed per event: it is a walk over directory listings, and every
    // event would pay for it. Opening a buffer in another package is a
    // different question, so it is asked again.
    if !matches!(&edge.facts, Some((searched, _)) if *searched == from) {
        let answers = state
            .facts
            .iter()
            .filter_map(|(name, fact)| Some((name.clone(), found(fact, &edge.root, &from)?)))
            .collect();
        edge.facts = Some((from, answers));
    }
    state.workspace_facts = edge
        .facts
        .as_ref()
        .map(|(_, answers)| answers.clone())
        .unwrap_or_default();
    state.ai_running = edge.ai.is_some();
    // Asked once and kept, unlike the Tools list's probe above: nobody
    // installs git mid-session, and the answer is a `PATH` walk that every
    // event would otherwise pay for.
    state.git_installed = *edge.git.get_or_insert_with(|| which::which("git").is_ok());
    state.lsp_running = edge.servers.keys().cloned().collect();
    // Both facts only the edge can observe: a child it holds, and a command on
    // this machine. The core reads them and writes neither.
    state.voice_running = edge.voice.is_some();
    state.player_installed = *edge
        .player
        .get_or_insert_with(|| which::which(&edge.speech.player).is_ok());
    // Asked every time, unlike the player: an install that exited 0 is not
    // proof the file is still there, and a stat is cheap.
    state.voice_installed =
        reading::voice_file(&state.speech.voice, &home()).is_some_and(|file| file.is_file());
    // R10.5: only forward clicks when the running program asked for them, and
    // only in the encoding it asked for.
    // The count first: `split()` is bounded by it, and the shell the two
    // facts below are read off is the one the keyboard is in.
    state.terminals = edge
        .shells
        .iter()
        .map(|shell| match shell.busy() {
            true => varde::Shell::Busy,
            false => varde::Shell::Idle,
        })
        .collect();
    state.terminal_mouse = edge.shells[state.split()].mouse_encoding();
    state.ai_mouse = edge
        .ai
        .as_ref()
        .map(pty::Pane::mouse_encoding)
        .unwrap_or_default();
    state.terminal_paste = edge.shells[state.split()].bracketed_paste();
    state.ai_paste = edge
        .ai
        .as_ref()
        .map(pty::Pane::bracketed_paste)
        .unwrap_or_default();
    // The Program output's three, the same way: whether the edge holds it, and
    // what its child asked for.
    state.output_running = edge.output.is_some();
    state.output_mouse = edge
        .output
        .as_ref()
        .map(pty::Pane::mouse_encoding)
        .unwrap_or_default();
    state.output_paste = edge
        .output
        .as_ref()
        .map(pty::Pane::bracketed_paste)
        .unwrap_or_default();
}

/// The synthesizer, started when the first markdown buffer opens rather than on
/// the press that wanted sound (R35.11). Nothing is spoken here: this is the
/// ~600ms voice load, paid while the reader is still reading.
///
/// No voice on disk means nothing to start, and that is not an error — it
/// is the state `reading::start` names out loud, with the speech row offered
/// beside it.
fn start_voice(state: &State, edge: &mut Edge) {
    if edge.voice.is_some() || !state.voice_installed {
        return;
    }
    if !state.buffers.keys().any(|path| preview::is_markdown(path)) {
        return;
    }
    // The configured speed, which is the one a Reading will ask for unless
    // `SetSpeed` has moved it since. A pre-warm at the wrong pace costs
    // nothing: `speak` below respawns when the pace it is handed differs.
    edge.voice = spawn_voice(edge, edge.speech.speed);
}

/// One synthesizer, with its arguments filled. `${voice}` is the model,
/// `${dir}` is where the stream goes and `${scale}` is the duration scale —
/// the synthesizer's own backwards convention, which is why it is taken from
/// the speed here, at the command, and never held anywhere a human writes a
/// number (R35.7). The reciprocal itself is `reading::duration_scale`, because
/// arithmetic in `main.rs` is arithmetic without a test.
///
/// A failure is reported and leaves nothing held, which is exactly what
/// `voice_running` then tells the core.
fn spawn_voice(edge: &mut Edge, speed: f32) -> Option<Voice> {
    let dir = tmp_dir(&edge.varde_home);
    let scale = reading::duration_scale(speed);
    let voice = reading::voice_file(&edge.speech.voice, &home()).unwrap_or_default();
    let args: Vec<String> = edge
        .speech
        .args
        .iter()
        .map(|arg| {
            arg.replace("${voice}", &voice.to_string_lossy())
                .replace("${dir}", &dir.to_string_lossy())
                .replace("${scale}", &format!("{scale:.4}"))
        })
        .collect();
    // One pipe for both channels, because which of the two a synthesizer
    // answers on is its own business and not something configuration should
    // have to say: a child that logs its answer rather than printing it puts
    // the path on stderr, and reading only stdout waits for a line that is
    // never coming — with the whole TUI behind that wait. `reading::wrote`
    // picks the answer out of whatever else the line carries.
    let Ok((answers, writer)) = std::io::pipe() else {
        return None;
    };
    let Ok(second) = writer.try_clone() else {
        return None;
    };
    let spawned = std::process::Command::new(&edge.speech.command)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(writer)
        .stderr(second)
        .spawn();
    match spawned {
        Ok(child) => Some(Voice {
            child,
            answers: std::io::BufReader::new(answers),
            speed,
        }),
        Err(error) => {
            // Never by silence: a Reading that says nothing is a Reading whose
            // success sounds the same as its failure (ADR 0013).
            edge.status = Status {
                text: format!("could not start {}: {error}", edge.speech.command),
                tone: ui::Tone::Warning,
            };
            None
        }
    }
}

/// Say it. Whatever was playing goes first — a Reading supersedes rather than
/// queues (R35.5) — and the pace being different from the resident child's is
/// a different child, because it is baked in at synthesis.
///
/// The line goes in and the path it wrote comes back, which is the framing that
/// lets one resident child serve every Reading. Blocking on that line is
/// deliberate and is the cost the spec names: building more than about sixty
/// seconds of audio exceeds the budget, and head-first streaming is the known
/// fix and is not built.
fn speak(
    edge: &mut Edge,
    utterances: &[reading::Utterance],
    speed: f32,
    queue: &mut VecDeque<Event>,
) {
    hush(edge);
    if edge
        .voice
        .as_ref()
        .is_some_and(|voice| voice.speed != speed)
    {
        edge.voice = None;
    }
    if edge.voice.is_none() {
        edge.voice = spawn_voice(edge, speed);
    }
    let Some(voice) = edge.voice.as_mut() else {
        return;
    };
    let said: Option<Vec<(PathBuf, u32)>> = utterances
        .iter()
        .map(|one| ask_voice(voice, &one.text).map(|wav| (wav, one.gap_ms)))
        .collect();
    let stream = said
        .filter(|said| !said.is_empty())
        .and_then(|said| stitch(&tmp_dir(&edge.varde_home), &said));
    let Some((whole, offsets)) = stream else {
        // The child is gone or would not answer. Dropping it is what makes the
        // next press start a fresh one, and what makes `voice_running` stop
        // claiming a session that is not there.
        edge.voice = None;
        edge.status = Status {
            text: format!("{} said nothing", edge.speech.command),
            tone: ui::Tone::Warning,
        };
        return;
    };
    edge.stream = Some(Stream {
        tail: whole.with_file_name("from.wav"),
        whole,
        offsets,
        from_ms: 0,
        since: Instant::now(),
    });
    play(edge, 0, queue);
}

/// Play the stream already built, from `at_ms` — a resume, a next, a previous.
/// The player is replaced rather than signalled and the stream is rewritten
/// from the offset rather than seeked in: freezing a process while its audio
/// device drains underruns the buffer and clicks audibly, and the rewrite cost
/// 7.5ms in the prototype, so this is both instant and sample-accurate (R35.6).
/// `docs/adr/0013-a-voice-is-an-installed-binary.md` names the reversal trigger
/// if a rewrite ever clicks where a linked library would not.
fn play(edge: &mut Edge, at_ms: u32, queue: &mut VecDeque<Event>) {
    stop_player(edge);
    let Some(stream) = edge.stream.as_ref() else {
        return;
    };
    let handing = if at_ms == 0 {
        Ok(stream.whole.clone())
    } else {
        rewrite(&stream.whole, &stream.tail, at_ms).map(|()| stream.tail.clone())
    };
    let played = handing.and_then(|path| {
        std::process::Command::new(&edge.speech.player)
            .arg(&path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|child| (child, path))
            .map_err(|error| format!("could not start {}: {error}", edge.speech.player))
    });
    match played {
        Ok((child, _)) => {
            edge.playing = Some(child);
            let stream = edge.stream.as_mut().expect("the stream borrowed above");
            stream.from_ms = at_ms;
            stream.since = Instant::now();
        }
        // Nothing is playing and nothing will, so the Reading is over and the
        // core is *told* that rather than left holding one: `:pause` and
        // `:next` aimed at a player nobody holds is `ai_running`'s failure
        // spelled with a different child, and refusing out loud is R35.9.
        Err(because) => {
            hush(edge);
            queue.push_back(Event::ReadingEnded);
            edge.status = Status {
                text: because,
                tone: ui::Tone::Warning,
            };
        }
    }
}

/// The stream from `at_ms` on, written to `tail` — overwritten by the next
/// seek and deleted with the stream it was cut from. The cause of a failure
/// comes back with it: a refusal that does not say what went wrong is
/// indistinguishable from silence, which is the one thing a Reading may not
/// fail as (R35.9).
fn rewrite(whole: &Path, tail: &Path, at_ms: u32) -> Result<(), String> {
    let built = (|| -> Result<(), hound::Error> {
        let mut reader = hound::WavReader::open(whole)?;
        let spec = reader.spec();
        let skip = samples(spec, at_ms);
        let mut writer = hound::WavWriter::create(tail, spec)?;
        for sample in reader.samples::<i16>().skip(skip as usize) {
            writer.write_sample(sample?)?;
        }
        writer.finalize()
    })();
    built.map_err(|error| {
        let _ = std::fs::remove_file(tail);
        format!("could not read {} from {at_ms}ms: {error}", whole.display())
    })
}

/// Where the sound has got to, told while a player is running — which is
/// also what keeps the mark following the voice rather than the last keypress
/// (R35.12). On the spinner's cadence and for its reason: a mark that moves
/// once a sentence does not need sixty frames a second to do it, and the
/// redraw is bounded by the sound the same way ADR 0009 bounds it by the job.
/// Queued rather than written into `State`: position is a
/// consequence of it and the core's own, and a field the edge wrote and the
/// core read would have two authors.
fn queue_position(edge: &Edge, last: &mut Instant, queue: &mut VecDeque<Event>) {
    if edge.playing.is_none() || last.elapsed() < SPIN {
        return;
    }
    *last = Instant::now();
    tell_position(edge, queue);
}

/// Where the sound is, to the millisecond. Called on the cadence above while a
/// player runs, and *unthrottled* by a pause — the core's position is a report
/// up to `SPIN` old, so a pause that trusted it would resume 80ms early, and
/// one taken in the first frame of a Reading would resume from the start,
/// which is the exact thing R35.6 rules out.
fn tell_position(edge: &Edge, queue: &mut VecDeque<Event>) {
    let Some(stream) = edge.stream.as_ref() else {
        return;
    };
    let elapsed = u32::try_from(stream.since.elapsed().as_millis()).unwrap_or(u32::MAX);
    queue.push_back(Event::Speaking {
        at_ms: stream.from_ms.saturating_add(elapsed),
        offsets: stream.offsets.clone(),
    });
}

/// A pause: where the sound got to, and then no sound.
fn pause(edge: &mut Edge, queue: &mut VecDeque<Event>) {
    tell_position(edge, queue);
    stop_player(edge);
}

/// One line in, one line back. A newline is the frame, so the words carry none
/// — [`reading::utterances`] joins a block's rows with spaces — and any that
/// survived would split one Utterance into two the caller only reads half of.
fn ask_voice(voice: &mut Voice, text: &str) -> Option<PathBuf> {
    use std::io::{BufRead, Write};
    let stdin = voice.child.stdin.as_mut()?;
    writeln!(stdin, "{}", text.replace(['\n', '\r'], " ")).ok()?;
    stdin.flush().ok()?;
    let mut answer = String::new();
    loop {
        answer.clear();
        // Zero is EOF: the child is gone, and the caller drops it and says so.
        // Anything that is not an answer is the child talking while it starts,
        // which is skipped rather than mistaken for silence.
        if voice.answers.read_line(&mut answer).ok()? == 0 {
            return None;
        }
        if let Some(wav) = reading::wrote(&answer) {
            return Some(PathBuf::from(wav));
        }
    }
}

/// The Utterances and the silence between them as **one** wav, which is the
/// whole of R35.4: a player per Utterance was built and rejected by ear,
/// because the ~150ms it takes to spawn one is a gap the machine chose sitting
/// where the listener needed one the writer chose.
///
/// One name, overwritten by the next Reading and deleted with the player, so
/// listening to a long document does not fill the disk. `hound` reads and
/// writes the container: the header carries lengths in two places and a
/// hand-patched one is wrong in a way nothing here would hear until a player
/// refused the file.
///
/// The pieces are the synthesizer's own output files and are gone by the time
/// this returns — the stream is what is played, and they were only ever the
/// parts of it.
///
/// The samples are read and written as 16-bit, which is what the shipped
/// `[speech]` command emits. A voice that answers in another format fails the
/// whole stream rather than half of it, and is reported as the command having
/// said nothing — out loud, per R35.9, and never as silence.
fn stitch(dir: &Path, said: &[(PathBuf, u32)]) -> Option<(PathBuf, Vec<u32>)> {
    let stream = dir.join("reading.wav");
    let first = &said.first()?.0;
    let mut offsets = Vec::with_capacity(said.len());
    let built = (|| -> Result<(), hound::Error> {
        let spec = hound::WavReader::open(first)?.spec();
        let mut writer = hound::WavWriter::create(&stream, spec)?;
        let mut at_ms = 0u32;
        for (wav, gap_ms) in said {
            offsets.push(at_ms);
            let mut reader = hound::WavReader::open(wav)?;
            // Frames, not samples: `duration` counts per channel, and a
            // stereo Utterance measured in samples would be reported as
            // taking half as long as it does — which is a seek landing in the
            // middle of a sentence.
            at_ms +=
                u32::try_from(u64::from(reader.duration()) * 1000 / u64::from(spec.sample_rate))
                    .unwrap_or(u32::MAX)
                    + gap_ms;
            for sample in reader.samples::<i16>() {
                writer.write_sample(sample?)?;
            }
            for _ in 0..samples(spec, *gap_ms) {
                writer.write_sample(0i16)?;
            }
        }
        writer.finalize()
    })();
    for (wav, _) in said {
        let _ = std::fs::remove_file(wav);
    }
    match built {
        Ok(()) => Some((stream, offsets)),
        Err(_) => {
            let _ = std::fs::remove_file(&stream);
            None
        }
    }
}

/// How many samples a duration is worth — an Utterance's gap to write, or the
/// head of a stream to skip past on a seek. Named rather than inlined because
/// it is arithmetic, and arithmetic without a test is what the edge is not
/// allowed to hold: a duration counted per frame rather than per sample is a
/// stereo stream whose silences are half as long as the ear was promised, and
/// a resume landing at half the offset it was given.
fn samples(spec: hound::WavSpec, ms: u32) -> u64 {
    u64::from(spec.sample_rate) * u64::from(ms) / 1000 * u64::from(spec.channels)
}

/// Stop the sound and keep the stream, which is the whole of what a pause is:
/// the file is what makes resuming cost 7.5ms instead of a second (R35.6).
fn stop_player(edge: &mut Edge) {
    let Some(mut child) = edge.playing.take() else {
        return;
    };
    let _ = child.kill();
    let _ = child.wait();
}

/// Stop the player and take the stream with it. The first of the two deletions
/// ADR 0014 asks for; the second is `leaving`'s `StopSpeaking`, and the third
/// is the sweep at startup that catches what a crash escaped into.
fn hush(edge: &mut Edge) {
    stop_player(edge);
    let Some(stream) = edge.stream.take() else {
        return;
    };
    let _ = std::fs::remove_file(&stream.whole);
    let _ = std::fs::remove_file(&stream.tail);
}

/// A player that finished on its own, which is a Reading that reached the end
/// of its Selection. Same deletion as a stop, and the core is *told* rather
/// than left holding a Reading nothing is playing — `ai_running`'s lesson, and
/// the reason a spawn that failed above hushes too.
fn reap_player(edge: &mut Edge, queue: &mut VecDeque<Event>) {
    let done = matches!(
        edge.playing.as_mut().map(std::process::Child::try_wait),
        Some(Ok(Some(_))) | Some(Err(_))
    );
    if done {
        hush(edge);
        queue.push_back(Event::ReadingEnded);
    }
}

/// Whether each configured command, and the program each install starts
/// with, is on this machine, asked while the list that shows it is open or a
/// re-check waits on the answer, and at no other time: the premise of the list
/// is that what it describes is about to change, so an answer kept from
/// startup would describe the machine as it was. `which` rather than a walk
/// over `PATH` — an executable bit, a `PATHEXT` on Windows and a command that
/// is already an absolute path are the edge cases nobody meets until they hit
/// one.
fn probe_path(state: &State, edge: &mut Edge) {
    if !matches!(state.modal, Modal::Tools { .. }) && state.recheck.is_none() {
        edge.on_path = None;
        return;
    }
    if edge.on_path.is_some() {
        return;
    }
    edge.on_path = Some(
        varde::tools::rows(state)
            .into_iter()
            .flat_map(|row| {
                let installer = row.install.as_deref().and_then(varde::tools::installer);
                [Some(row.command), installer]
            })
            .flatten()
            .filter(|command| which::which(command).is_ok())
            .collect(),
    );
}

/// The one search the edge runs for every fact configuration declares: the
/// nearest directory from the file being served up to the workspace root that
/// holds the marker, then the root itself, then anywhere [`beneath`] the root,
/// then a machine-wide install. Never above the root — a toolchain outside the
/// workspace must not serve a file inside it.
///
/// Nothing here names an ecosystem. `node_modules/typescript/lib/typescript.js`,
/// `.venv/bin/python` and `compile_commands.json` are the same question asked
/// with a different marker, and the marker is data (R31.1, R31.27): an arm per
/// ecosystem here is a server's name in an arm with a package manager in it.
///
/// Nearest-first is why the walk starts at the file. A pnpm monorepo installs
/// its dependencies per package, so a root with no `node_modules/typescript`
/// and a package with one is the ordinary shape rather than the exotic one, and
/// resolving from the root found nothing where every other editor finds the
/// SDK the package pinned.
///
/// With buffers open in two packages the first answer wins, in path order.
/// `State::workspace_facts` is one map for the workspace, so that is what one
/// map can say; keying it by language is the change a workspace whose packages
/// genuinely disagree would force, and nothing has yet.
fn found(fact: &Fact, root: &Path, from: &BTreeSet<PathBuf>) -> Option<String> {
    for start in from
        .iter()
        .map(PathBuf::as_path)
        .chain(std::iter::once(root))
    {
        let mut here = Some(start);
        while let Some(directory) = here {
            let marker = directory.join(&fact.marker);
            if marker.exists() {
                return handed_over(fact, &marker);
            }
            here = match directory == root {
                true => None,
                false => directory.parent(),
            };
        }
    }
    if let Some(marker) = beneath(fact, root) {
        return handed_over(fact, &marker);
    }
    // The machine-wide install, expressed the same way: a command on `PATH`,
    // and the marker relative to the directory holding it once symlinks are
    // resolved — `tsc` is a link into the package, so its `lib` is beside the
    // `bin` the link pointed into. Checked for the marker rather than assumed,
    // because the globally installed TypeScript 7.0 native preview is on
    // `PATH` and its `lib/` holds no `typescript.js` at all: a package being
    // installed says nothing about whether it can serve as an SDK, and no
    // version string reports the difference.
    let binary = std::fs::canonicalize(which::which(fact.command.as_ref()?).ok()?).ok()?;
    let beside =
        std::fs::canonicalize(binary.parent()?.join(fact.command_marker.as_ref()?)).ok()?;
    handed_over(fact, &beside)
}

/// The workspace's own answer where no open file's package holds one and the
/// root holds none either — down from the root rather than up from a file, and
/// the last place looked before the machine-wide install.
///
/// It is what makes a fact a question about the *workspace* rather than about
/// whichever files are open. A monorepo installs per package, so with nothing
/// open — the state Tools is read in — the walk above had only the
/// root to search and the row said `missing-requirement` about a workspace
/// holding the SDK all along, then said `installed` as soon as a file in the
/// package was opened. An answer that changes with the buffer list is the
/// "sometimes it works" a reader cannot act on.
///
/// Two bounds, because this is the one search with no file to start from.
/// `SPREAD` is how deep a package may sit, and a directory named as the
/// marker's own first component is never descended into: that is where the
/// answer lives and it holds no second copy of itself, which is what keeps an
/// installed dependency tree from being walked. Both read off the fact, so
/// nothing here names an ecosystem (R31.1). Breadth-first and in path order,
/// so the answer is the shallowest and the same one twice.
fn beneath(fact: &Fact, root: &Path) -> Option<PathBuf> {
    let own = fact.marker.split('/').next().unwrap_or_default();
    let mut here = vec![root.to_path_buf()];
    for _ in 0..SPREAD {
        let mut next = Vec::new();
        for directory in here {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(directory)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .filter(|path| {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    !name.starts_with('.') && name != own
                })
                .collect();
            inside.sort();
            for candidate in inside {
                let marker = candidate.join(&fact.marker);
                if marker.exists() {
                    return Some(marker);
                }
                next.push(candidate);
            }
        }
        here = next;
    }
    None
}

/// The marker itself, or the directory holding it, as the fact was declared.
fn handed_over(fact: &Fact, marker: &Path) -> Option<String> {
    let answer = match fact.value {
        FactValue::Directory => marker.parent()?,
        FactValue::Marker => marker,
    };
    Some(answer.to_string_lossy().to_string())
}

/// Converts whatever this terminal sent into the lossless event type, then lets
/// the library decide what it means. Everything here is conversion; nothing here
/// routes, and nothing here discards a modifier.
fn translate_key(
    state: &State,
    edge: &mut Edge,
    key: event::KeyEvent,
    started: Instant,
    queue: &mut VecDeque<Event>,
) {
    let Ok(event) = to_terminput_key(key) else {
        return;
    };
    let at_ms = started.elapsed().as_millis() as u64;
    queue.extend(keys::on_key_event(state, &mut edge.drafts, event, at_ms));
}

/// Decodes a mouse event and lets the library route it. The one thing it
/// cannot do is read characters off the terminal, so a text selection comes
/// back as a request to fulfil here.
fn translate_mouse(
    state: &State,
    edge: &mut Edge,
    mouse: event::MouseEvent,
    started: Instant,
    queue: &mut VecDeque<Event>,
) {
    edge.pointer.at_ms = started.elapsed().as_millis() as u64;
    let kind = match mouse.kind {
        MouseEventKind::Moved => mouse::Kind::Moved,
        MouseEventKind::Down(MouseButton::Left) => mouse::Kind::LeftDown,
        MouseEventKind::Drag(MouseButton::Left) => mouse::Kind::LeftDrag,
        MouseEventKind::Up(MouseButton::Left) => mouse::Kind::LeftUp,
        MouseEventKind::Down(MouseButton::Right) => mouse::Kind::RightDown,
        MouseEventKind::ScrollUp => mouse::Kind::ScrollUp,
        MouseEventKind::ScrollDown => mouse::Kind::ScrollDown,
        MouseEventKind::ScrollLeft => mouse::Kind::ScrollLeft,
        MouseEventKind::ScrollRight => mouse::Kind::ScrollRight,
        // Exhaustive over the buttons, and deliberately: the catch-all that
        // stood here swallowed every sideways wheel report for Varde's whole
        // life, so a sideways swipe did nothing anywhere and said nothing
        // either. A kind crossterm grows now fails the build instead.
        MouseEventKind::Down(MouseButton::Middle)
        | MouseEventKind::Up(MouseButton::Middle)
        | MouseEventKind::Drag(MouseButton::Middle)
        | MouseEventKind::Up(MouseButton::Right)
        | MouseEventKind::Drag(MouseButton::Right) => return,
    };
    route_mouse(
        state,
        edge,
        mouse::Input {
            kind,
            column: mouse.column,
            row: mouse.row,
            // Through `terminput`, for the reason a key goes through it: the
            // modifiers a mouse report carries are the terminal library's to
            // decode, and which of them means something is `mouse`'s to say.
            modifiers: to_terminput_mouse(mouse).modifiers,
        },
        queue,
    );
}

/// Hands one decoded report to the library and fulfils what comes back. Split
/// from the decoding above because a held drag is replayed through here on a
/// cadence with no crossterm event behind it.
fn route_mouse(state: &State, edge: &mut Edge, input: mouse::Input, queue: &mut VecDeque<Event>) {
    let panes = layout::panes(
        edge.area.width,
        edge.area.height,
        state.tree_divider as u16,
        state.ai_width.map(|width| width as u16),
        story::band_height(state),
        story::step_menu_width(state),
        varde::shapes(state),
    );
    let outcome = mouse::on_mouse(state, &panes, &mut edge.pointer, input);
    queue.extend(outcome.events);
    if let Some(selection) = outcome.select {
        if let Some(lines) = grid_lines(edge, selection.pane, state.split(), selection.to.line) {
            queue.push_back(Event::SelectIn {
                pane: selection.pane,
                from: selection.from,
                to: selection.to,
                text: editor::span_text(&lines, selection.from, selection.to),
            });
        }
    }
    if let Some((pane, at)) = outcome.link {
        if let Some(row) =
            grid_lines(edge, pane, state.split(), at.line).and_then(|mut lines| lines.pop())
        {
            queue.push_back(Event::ClickLink {
                row,
                column: at.column,
            });
        }
    }
}

/// The first `upto` rows of a pty's grid, as text — the only part of a
/// selection or a link the library cannot finish, because reading the screen
/// needs the pty. It said which pane and which cells; this reads them off that
/// pane's own child. None when there is no session to read, so nothing gets
/// picked from the wrong one.
fn grid_lines(edge: &Edge, pane: Pane, split: usize, upto: usize) -> Option<Vec<String>> {
    // Exhaustive on purpose, for the reason a pane's rectangle is chosen
    // exhaustively: a fall-through arm reads the shell's grid for whichever
    // pane nobody thought about, and a fourth pane in the corner is supposed to
    // be a compiler error here rather than a drag full of the terminal's text.
    let screen = match pane {
        Pane::Ai => edge.ai.as_ref()?.screen(),
        Pane::Terminal => edge.shells.get(split)?.screen(),
        Pane::Output => edge.output.as_ref()?.screen(),
        Pane::Tree
        | Pane::Editor
        | Pane::Evaluator
        | Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts
        | Pane::Variables
        | Pane::Cheatsheet => return None,
    };
    let (rows, columns) = screen.size();
    // Absolutely indexed from the grid's first row, because that is what the
    // span's line numbers count from. `grid_row` keeps each cell's column, which
    // is the whole difference between reading the row the pointer was on and
    // reading a collapsed version of it.
    Some(
        (0..rows.min(upto as u16))
            .map(|row| {
                editor::grid_row(
                    (0..columns).map(|column| screen.cell(row, column).map(vt100::Cell::contents)),
                )
            })
            .collect(),
    )
}

/// Executes one effect. Grouped rather than one match of thirty arms: each
/// group takes the effects it owns and hands the rest on.
fn perform(effect: Effect, split: usize, edge: &mut Edge, queue: &mut VecDeque<Event>) {
    let Some(effect) = perform_terminal(effect, split, edge) else {
        return;
    };
    let Some(effect) = perform_session(effect, edge, queue) else {
        return;
    };
    let Some(effect) = perform_files(effect, edge, queue) else {
        return;
    };
    perform_jobs(effect, edge, queue);
}

/// Everything written straight at a pty or at the host terminal. `split` is
/// the core's answer to which of the strip's shells has the keyboard: what the
/// core says about "the terminal" is said about that one.
fn perform_terminal(effect: Effect, split: usize, edge: &mut Edge) -> Option<Effect> {
    match effect {
        // Ctrl+U clears the line the user was typing, which is R3.4's "replace".
        Effect::SetTerminalInput(text) => {
            edge.shell(split).send(b"\x15");
            edge.shell(split).send(text.as_bytes());
        }
        Effect::RunInTerminal(text) => {
            edge.shell(split).send(b"\x15");
            edge.shell(split).send(text.as_bytes());
            edge.shell(split).send(b"\r");
        }
        // ponytail: the wheel scrolls the split with the keyboard, not the one under the pointer.
        Effect::Scrolled(pane, direction) => {
            let up = direction == Direction::Up;
            match (pane, edge.ai.as_mut()) {
                (Pane::Ai, Some(ai)) => ai.scroll(up),
                _ => edge.shell(split).scroll(up),
            }
        }
        Effect::SetClipboard(text) => {
            if let Some(clipboard) = edge.clipboard.as_mut() {
                let _ = clipboard.set_text(text);
            }
        }
        // As an argument, never interpolated: the URL is text a child printed.
        // ponytail: unix only, like `alive`'s kill(2) — a port picks its opener here.
        Effect::OpenUrl(url) => {
            let opener = match cfg!(target_os = "macos") {
                true => "open",
                false => "xdg-open",
            };
            match std::process::Command::new(opener).arg(&url).spawn() {
                // Reaped off the loop: an opener hands off and exits at once,
                // and an unwaited child is a zombie until Varde quits.
                Ok(mut child) => {
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                }
                Err(error) => {
                    edge.status = Status {
                        text: format!("could not open {url}: {error}"),
                        tone: ui::Tone::Warning,
                    };
                }
            }
        }
        // OSC 52: the terminal puts it on the clipboard of whatever machine you
        // are actually sitting at. That terminal is Varde's own stdout: sent to
        // the shell pane, the sequence was typed at its prompt and copied nothing.
        Effect::ClipboardViaTerminal(text) => {
            let _ = execute!(
                std::io::stdout(),
                crossterm::clipboard::CopyToClipboard::to_clipboard_from(text)
            );
        }
        // Exhaustive on purpose: a catch-all here sent the AI pane's bytes to
        // the shell whenever its child was gone, which would have run a review
        // as commands. Bytes for a pane with no child are dropped.
        Effect::SendKeys { pane, bytes } => match pane {
            Pane::Ai => {
                if let Some(ai) = edge.ai.as_mut() {
                    ai.send(&bytes);
                }
            }
            Pane::Terminal => edge.shell(split).send(&bytes),
            Pane::Output => {
                if let Some(output) = edge.output.as_mut() {
                    output.send(&bytes);
                }
            }
            Pane::Tree
            | Pane::Editor
            | Pane::Evaluator
            | Pane::Risk
            | Pane::Buffers
            | Pane::History
            | Pane::Breakpoints
            | Pane::Frames
            | Pane::Diagnostics
            | Pane::Conflicts
            | Pane::Variables
            | Pane::Cheatsheet => {}
        },
        other => return Some(other),
    }
    None
}

/// The AI session, the status row, and leaving.
fn perform_session(effect: Effect, edge: &mut Edge, queue: &mut VecDeque<Event>) -> Option<Effect> {
    match effect {
        Effect::RenderView(_) => {}
        Effect::StopAi => {
            edge.ai = None;
            // `:ai!` replaces the session in one breath, and the review queued
            // for the one being stopped was not written for its replacement.
            queue.push_back(Event::AiExited);
        }
        // Beside the shell that was split, in its folder — the root only when
        // the OS will not say where it has got to.
        Effect::SplitTerminal { from } => {
            let from = from.min(edge.shells.len() - 1);
            let cwd = edge.shells[from].cwd().unwrap_or_else(|| edge.root.clone());
            let (rows, cols) = edge.shells[from].screen().size();
            match pty::Pane::spawn(&[], &cwd, &BTreeMap::new(), rows, cols) {
                Ok(pane) => edge.shells.insert(from + 1, pane),
                Err(error) => {
                    edge.status = Status {
                        text: format!("could not split the terminal: {error}"),
                        tone: ui::Tone::Warning,
                    };
                }
            }
        }
        Effect::SpawnAi { command } => {
            let size = edge.shells[0].screen().size();
            match pty::Pane::spawn(
                std::slice::from_ref(&command),
                &edge.root,
                &BTreeMap::new(),
                size.0,
                size.1,
            ) {
                Ok(pane) => {
                    edge.ai = Some(pane);
                }
                Err(error) => {
                    edge.status = Status {
                        text: format!("could not start {command}: {error}"),
                        tone: ui::Tone::Warning,
                    };
                    edge.drafts.ai = command;
                    // The pane never came to exist, so there is no session —
                    // and a review queued for it is not owed to whichever CLI
                    // is started next.
                    queue.push_back(Event::AiExited);
                }
            }
        }
        // The adapter asked for a terminal to run the debugged program in. It
        // replaces whatever the last session left: one Debug group, one
        // program, and a pane holding a program nobody is debugging is the
        // dead pane `ai_running` was.
        Effect::RunProgram { argv, cwd, env } => {
            let cwd = cwd.unwrap_or_else(|| edge.root.clone());
            let size = edge.shells[0].screen().size();
            match pty::Pane::spawn(&argv, &cwd, &env, size.0, size.1) {
                Ok(pane) => edge.output = Some(pane),
                Err(error) => {
                    edge.status = Status {
                        text: format!("could not run {}: {error}", argv.join(" ")),
                        tone: ui::Tone::Warning,
                    };
                }
            }
        }
        Effect::Speak { utterances, speed } => speak(edge, &utterances, speed, queue),
        Effect::SpeakFrom { at_ms } => play(edge, at_ms, queue),
        Effect::PauseSpeaking => pause(edge, queue),
        Effect::StopSpeaking => hush(edge),
        Effect::StartLsp {
            language,
            command,
            args,
        } => {
            // Truncated per spawn: the last run is the diagnosis, and an
            // ever-growing file is a disk leak in a directory nobody asked
            // for. Nothing reads it back — a core that parsed a server's log
            // would be branching on which server it is (R31.1).
            let log =
                varde_dir(&edge.root, edge.sidecar.as_deref()).join(format!("lsp-{language}.log"));
            let log = match std::fs::File::create(&log) {
                Ok(file) => Some(file),
                Err(error) => {
                    edge.status = Status {
                        text: format!("could not open {}: {error}", log.display()),
                        tone: ui::Tone::Warning,
                    };
                    None
                }
            };
            match rpc::Server::spawn(&command, &args, &edge.root, log) {
                Ok(server) => {
                    edge.servers.insert(language.clone(), server);
                    // A process exists now, which is the fact the conversation
                    // begins against: the core reads which servers are held off
                    // `lsp_running`, and this is what brings a pass around to do
                    // it. Without it the handshake would wait for whatever the
                    // reader happened to press next.
                    queue.push_back(Event::LspStarted { language });
                }
                // The command is not there, so no process is — and the core is
                // told rather than left holding a conversation with nobody.
                Err(_) => queue.push_back(Event::LspGone {
                    language,
                    why: varde::lsp::Gone::FailedToStart,
                }),
            }
        }
        Effect::StartDap {
            command,
            args,
            reach,
        } => {
            // A new session numbers its children afresh, so none of the last
            // one's may still answer to a number.
            edge.children.clear();
            edge.joining.clear();
            // Truncated per spawn, for the reason a server's log is.
            let log = varde_dir(&edge.root, edge.sidecar.as_deref()).join("dap.log");
            let log = match std::fs::File::create(&log) {
                Ok(file) => Some(file),
                Err(error) => {
                    edge.status = Status {
                        text: format!("could not open {}: {error}", log.display()),
                        tone: ui::Tone::Warning,
                    };
                    None
                }
            };
            let spawned = match reach {
                varde::debug::Reach::Stdio => rpc::Adapter::spawn(&command, &args, &edge.root, log)
                    .map(|adapter| {
                        edge.adapter = Some(adapter);
                        edge.again = Some(Again::Spawn {
                            command: command.clone(),
                            args: args.clone(),
                        });
                        queue.push_back(Event::DapStarted { from: 0 });
                    }),
                // A port nothing holds, asked of the OS and let go for the
                // adapter to take: the only way to learn one that is free.
                varde::debug::Reach::Server => {
                    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                        .and_then(|listener| listener.local_addr())
                        .and_then(|address| {
                            let port = address.port();
                            let args = varde::debug::on_port(&args, port);
                            edge.again = Some(Again::Dial(port));
                            rpc::Adapter::connect(&command, &args, &edge.root, log, port)
                        })
                        .map(|connected| edge.connecting = Some(connected))
                }
                varde::debug::Reach::Port(port) => {
                    edge.connecting = Some(rpc::Adapter::dial(port, None));
                    edge.again = Some(Again::Dial(port));
                    Ok(())
                }
            };
            // What the spawn said, which only the edge can see: a command
            // that is not there is the one the reader fixes by installing.
            if let Err(error) = spawned {
                queue.push_back(Event::DapGone {
                    why: match error.kind() {
                        std::io::ErrorKind::NotFound => varde::debug::Gone::Missing,
                        _ => varde::debug::Gone::FailedToStart,
                    },
                    from: 0,
                });
            }
        }
        Effect::DapSend { to, json } => {
            let adapter = match to {
                0 => edge.adapter.as_mut(),
                child => edge.children.get_mut(&child),
            };
            let alive = adapter.is_some_and(|adapter| {
                adapter.send(json);
                adapter.alive
            });
            if !alive {
                match to {
                    0 => edge.adapter = None,
                    child => {
                        edge.children.remove(&child);
                    }
                }
                queue.push_back(Event::DapGone {
                    why: varde::debug::Gone::Exited,
                    from: to,
                });
            }
        }
        // Another conversation with the adapter the session holds, for a child
        // session: its port dialled again, or another one of it spawned.
        Effect::DapChild { child } => match edge.again.clone() {
            Some(Again::Dial(port)) => {
                edge.joining.insert(child, rpc::Adapter::dial(port, None));
            }
            Some(Again::Spawn { command, args }) => {
                match rpc::Adapter::spawn(&command, &args, &edge.root, None) {
                    Ok(adapter) => {
                        edge.children.insert(child, adapter);
                        queue.push_back(Event::DapStarted { from: child });
                    }
                    Err(error) => child_never_joined(edge, queue, child, error.to_string()),
                }
            }
            None => child_never_joined(edge, queue, child, "no adapter is held".to_string()),
        },
        Effect::StopDapChild { child } => {
            if edge.children.remove(&child).is_some() || edge.joining.remove(&child).is_some() {
                queue.push_back(Event::DapGone {
                    why: varde::debug::Gone::Exited,
                    from: child,
                });
            }
        }
        Effect::StopDap => {
            if edge.adapter.is_some() || edge.connecting.is_some() {
                adapter_is_gone(edge, queue, varde::debug::Gone::Exited);
            }
        }
        // Asked for, so the answer kept from the list opening is dropped and
        // `tell_core` — which runs before the next event is decided — probes
        // again. The event says only that a fresh answer has landed: the set
        // itself stays a field the core is told and never writes (R31.23).
        Effect::ProbePath => {
            edge.on_path = None;
            // And the workspace facts with it: what a re-check is asking after
            // is an install, and an `npm install` is exactly what puts a
            // TypeScript SDK in a workspace that had none.
            edge.facts = None;
            queue.push_back(Event::PathProbed);
        }
        Effect::LspSend { language, json } => match edge.servers.get_mut(&language) {
            Some(server) => {
                server.send(json);
                if !server.alive {
                    edge.servers.remove(&language);
                    queue.push_back(Event::LspGone {
                        language,
                        why: varde::lsp::Gone::Exited,
                    });
                }
            }
            // Nothing holds that language's server any more, which the core
            // learns here rather than by waiting for a reply that cannot come.
            None => queue.push_back(Event::LspGone {
                language,
                why: varde::lsp::Gone::Exited,
            }),
        },
        // One timer, restarted: the core sends this on every change while
        // inserting, so the last keystroke of a word is the one that counts.
        Effect::DebounceCandidates(after_ms) => {
            edge.candidates_due = Some(Instant::now() + Duration::from_millis(after_ms));
        }
        // One timer, restarted: the core sends this for every cell the pointer
        // crosses, so the cell it stops on is the one that counts.
        Effect::DwellHover(after_ms) => {
            edge.hover_due = Some(Instant::now() + Duration::from_millis(after_ms));
        }
        Effect::Notify(notice) => {
            let (text, tone) = notice_text(notice);
            edge.status = Status {
                text: text.to_string(),
                tone,
            };
        }
        Effect::NotifyAbout { slug, about } => {
            let (text, tone) = notice_text(slug);
            edge.status = Status {
                text: format!("{text}: {about}"),
                tone,
            };
        }
        // The words go when the fact does. Falling back to empty rather than to
        // the hint keeps one place deciding what an empty status shows.
        Effect::ClearNotice => edge.status.text.clear(),
        Effect::Exit => {
            edge.status = Status {
                text: "quit".to_string(),
                tone: ui::Tone::Notice,
            }
        }
        Effect::Relaunch => {
            edge.relaunch = true;
            edge.status = Status {
                text: "quit".to_string(),
                tone: ui::Tone::Notice,
            }
        }
        other => return Some(other),
    }
    None
}

/// Reading and writing the filesystem.
fn perform_files(effect: Effect, edge: &mut Edge, queue: &mut VecDeque<Event>) -> Option<Effect> {
    match effect {
        Effect::EnsureDir(path) => {
            let _ = std::fs::create_dir_all(path);
        }
        Effect::OpenBuffer(path) => queue.push_back(Event::BufferOpened {
            contents: std::fs::read_to_string(&path).unwrap_or_default(),
            path,
            preview: false,
            at: None,
        }),
        Effect::PreviewBuffer(path) => queue.push_back(Event::BufferOpened {
            contents: std::fs::read_to_string(&path).unwrap_or_default(),
            path,
            preview: true,
            at: None,
        }),
        Effect::WriteFile { path, contents } => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, contents);
        }
        Effect::DeleteFile(path) => {
            let _ = std::fs::remove_file(path);
        }
        Effect::DeleteDir(path) => {
            let _ = std::fs::remove_dir_all(path);
        }
        Effect::ReadFolder(path) => {
            let entries = entries(&path);
            queue.push_back(Event::Expand { path, entries });
        }
        // Read and hand back, the way a folder is: the core asked for a paste
        // and the clipboard's text is the only part of it the edge knows. An
        // empty clipboard is nothing to hand back, not an empty edit.
        Effect::ReadClipboard => {
            if let Some(text) = edge
                .clipboard
                .as_mut()
                .and_then(|clipboard| clipboard.get_text().ok())
                .filter(|text| !text.is_empty())
            {
                queue.push_back(Event::EditorPaste(text));
            }
        }
        Effect::SaveState(contents) => {
            let path = varde_dir(&edge.root, edge.sidecar.as_deref()).join(STATE_FILE);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, contents);
        }
        // One event, not the opening and then the place: the core cannot tell
        // whether a jump into the file already on screen went anywhere from
        // either half alone. See `Event::BufferOpened`.
        Effect::OpenAt { path, at } => {
            queue.push_back(Event::BufferOpened {
                contents: std::fs::read_to_string(&path).unwrap_or_default(),
                path,
                preview: false,
                at: Some(at),
            });
        }
        // The file, not the Buffer: nothing has this one open, which is the
        // whole point of reading it. A file that cannot be read is answered
        // with nothing, and stays *not measured* — the honest answer, and the
        // same one a file no server serves gets.
        Effect::ReadForReview(path) => {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                queue.push_back(Event::ReviewFileRead { path, contents });
            }
        }
        // A file that cannot be read is answered as empty, which makes every
        // Breakpoint in it Stale — the honest answer about a line nobody can
        // find — and says why.
        Effect::ReadBreakpointFile(path) => {
            let contents = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                eprintln!("varde: cannot read {}: {error}", path.display());
                String::new()
            });
            queue.push_back(Event::BreakpointFileRead { contents, path });
        }
        Effect::ReadDiff(path) => {
            let diff = diff_lines(&edge.root, &path);
            let file = path
                .strip_prefix(&edge.root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            edge.diff_sides = (diff.new_side, diff.old_side);
            queue.push_back(Event::ShowDiff {
                file,
                lines: diff.lines,
                revision: diff.revision,
            });
        }
        other => return Some(other),
    }
    None
}

/// Work that goes and finds something: a walk, a diff, a thread. The last
/// group, so its catch-all is every effect the three before it declined —
/// which is none of them. Chaining is what costs the compiler's exhaustiveness
/// check here, so the assertion stands in for it: a new `Effect` nobody
/// executes fails the suite rather than going quietly missing at runtime.
fn perform_jobs(effect: Effect, edge: &mut Edge, queue: &mut VecDeque<Event>) {
    match effect {
        // Debounced: the query is held until typing settles, so a repo is read
        // once per word rather than once per keystroke.
        Effect::RunSearch(query) => {
            edge.pending_search = Some((query, Instant::now() + Duration::from_millis(150)));
        }
        // One walk per filter, git-ignore aware. The tree stays lazy; only the
        // filter needs to know the whole project, and it needs to know it as it
        // is now — a walk kept for the session is a walk that stops finding
        // files as soon as one is created.
        Effect::IndexProject => {
            let root = edge.root.clone();
            let files = walk(&root);
            // Dotfiles are part of the project (R2.2), but .git is a
            // database — matching its objects is noise, not results.
            queue.push_back(Event::Indexed(files));
        }
        Effect::ReadStories { dir, repo } => {
            // Told in full, oldest first, every time the folder is opened —
            // not only on a live watcher event — so retention is enforced
            // against what is really on disk rather than only what a Create
            // happened to fire for during this session (ADR 0005).
            for name in story_set_names_oldest_first(&dir) {
                queue.push_back(Event::StoryFileWritten(name));
            }
            if let Some((path, contents)) = latest_story(&dir) {
                let range = story_range_status(&repo, &path);
                queue.push_back(Event::StoryArtifact { contents, range });
            }
        }
        Effect::AnalyseRisk {
            scope: _,
            generation,
            files,
            base,
        } => {
            // Off the main loop: a workspace's worth of parsing takes seconds,
            // and the TUI answers keys throughout. The thread reads files and
            // converts; every decision about what it found is the library's.
            let answer = edge.analysed.clone();
            let root = edge.root.clone();
            // The receiver outlives every job — it is dropped when Varde
            // exits — so a send that fails is a Varde already shutting down.
            std::thread::spawn(move || {
                answer_figures(generation, files, base, &root, &answer);
            });
        }
        Effect::ResolveStory {
            repo,
            dir,
            explicit,
            force,
        } => {
            let outcome = resolve_story(&repo, &dir, explicit.as_deref(), force);
            queue.push_back(Event::StoryResolved(outcome));
        }
        // One read answers all three of the picker's questions, so a refusal
        // arrives before the list is drawn rather than after a row is picked.
        Effect::ReadBranches => queue.push_back(Event::Branches(read_branches(&edge.root))),
        // The download's exit status, as the sentinel carries it (ADR 0015),
        // and the Guest repo read exactly as any other repository is — `git2`,
        // like every read in Varde: only the clone and the fetch shell out. A
        // sentinel that cannot be read is a failure too, with no status to
        // name: refused out loud rather than reported as a download that
        // worked.
        Effect::ReadGuestBranches {
            sentinel,
            repo,
            how,
        } => {
            let status = std::fs::read_to_string(&sentinel);
            let branching = match status.as_deref().map(str::trim) {
                Ok("0") => read_branches(&repo),
                Ok(status) => story::Branching::DownloadFailed {
                    how,
                    status: Some(status.to_string()),
                },
                // A sentinel this Varde asked for and cannot read is a failure
                // with no status to name, reported as one rather than as a
                // clone that worked. The reason is printed with the rest of
                // what the edge cannot show on screen.
                Err(error) => {
                    eprintln!("varde: cannot read {}: {error}", sentinel.display());
                    story::Branching::DownloadFailed { how, status: None }
                }
            };
            queue.push_back(Event::Branches(branching));
        }
        // A file that is not there is an empty one: appending to it loses
        // nothing. One that is there and cannot be read is `None`, which the
        // core refuses rather than writing over.
        Effect::ReadGlobalConfig {
            path,
            kind,
            name,
            write,
        } => {
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => Some(text),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
                Err(error) => {
                    eprintln!("varde: cannot read {}: {error}", path.display());
                    None
                }
            };
            queue.push_back(Event::GlobalConfigRead {
                kind,
                name,
                write,
                text,
            });
        }
        Effect::ReadInstallStatus(sentinel) => {
            let status = std::fs::read_to_string(&sentinel)
                .inspect_err(|error| {
                    eprintln!("varde: cannot read {}: {error}", sentinel.display())
                })
                .ok();
            queue.push_back(Event::InstallEnded(status));
        }
        Effect::CheckoutBranch { repo, name } => queue.push_back(match checkout(&repo, &name) {
            Ok(left) => Event::CheckedOut { left },
            Err(because) => Event::CheckoutFailed(because),
        }),
        // The fill and the arrival checks: the core says which files and which
        // two revisions, the edge diffs and reads them. Exactly the poll's own
        // read, aimed at the arriving range rather than the loaded one, so the
        // texts the fill lands are the texts the stale check will compare
        // against and the hunks the checks run over are the range's own.
        Effect::ReadStoryFiles {
            repo,
            base,
            head,
            files,
        } => {
            let inventory = match &head {
                Some(head) => story::Inventory::Committed { base: &base, head },
                None => story::Inventory::Worktree,
            };
            let read = file_hunks(&repo, inventory, &files);
            queue.push_back(Event::StoryFiles(read));
        }
        Effect::WriteStoryContext {
            repo,
            spelling,
            path,
        } => {
            // Always written, even when git had no answer: the prompt has
            // already pointed the AI at this path, so an absent file is a
            // refusal by silence. What it says instead is the library's.
            let contents = story_context(&repo, &spelling)
                .unwrap_or_else(|| story::CONTEXT_UNAVAILABLE.to_string());
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, contents);
        }
        Effect::Snapshot { iteration } => snapshot(&edge.root, edge.sidecar.as_deref(), iteration),
        Effect::RestoreSnapshot { iteration } => {
            restore(&edge.root, edge.sidecar.as_deref(), iteration)
        }
        Effect::CheckRelease { url } => {
            let answer = edge.released.clone();
            std::thread::spawn(move || {
                let _ = answer.send(latest_release(&url));
            });
        }
        Effect::ReplaceBinary { asset, checksums } => {
            let answer = edge.replaced.clone();
            let exe = edge.exe.clone();
            std::thread::spawn(move || {
                let outcome = match exe {
                    Some(exe) => replace_binary(&exe, &asset, &checksums),
                    None => {
                        eprintln!("varde: update failed: the running binary could not be located");
                        Err(ReplaceFailed::Replace)
                    }
                };
                let _ = answer.send(outcome);
            });
        }
        Effect::RunTests { command } => {
            // Off the main loop and out of the shell pane: the TUI answers keys
            // while a suite runs, and the shell stays the user's.
            //
            // Split and executed directly, never through `sh -c`: the command
            // comes from the *opened folder's* config, and AGENTS.md's security
            // rule is that nothing from the workspace is interpolated into a
            // shell command. `Pane::spawn` treats the configured AI command the
            // same way, and `shlex` — already the repo's answer for shell words
            // — is what splits it.
            let answer = edge.tested.clone();
            let root = edge.root.clone();
            std::thread::spawn(move || answer_tests(&command, &root, &answer));
        }
        Effect::RunFormatter {
            language,
            command,
            args,
            path,
            revision,
            text,
        } => {
            // Off the main loop and split rather than shelled, for both reasons
            // `RunTests` above is: the TUI answers keys while a formatter runs,
            // and a command out of the opened folder's config is never
            // interpolated into a shell (AGENTS.md's security rule).
            let answer = edge.formatted.clone();
            let root = edge.root.clone();
            std::thread::spawn(move || {
                let made = answer_formatter(&command, &args, &root, &text);
                let _ = answer.send((language, path, revision, made));
            });
        }
        other => debug_assert!(false, "no group executes {other:?}"),
    }
}

/// The body of the latest-Release request, or `None` with the reason printed
/// alongside the rest of what the edge cannot show on screen: a failed check is
/// never a notice (ADR 0017).
fn latest_release(url: &str) -> Option<String> {
    let outcome = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "20",
            "-H",
            "Accept: application/vnd.github+json",
            url,
        ])
        .output();
    match outcome {
        Ok(finished) if finished.status.success() => {
            Some(String::from_utf8_lossy(&finished.stdout).into_owned())
        }
        Ok(finished) => {
            eprintln!(
                "varde: release check failed: {}",
                String::from_utf8_lossy(&finished.stderr).trim()
            );
            None
        }
        Err(error) => {
            eprintln!("varde: release check failed: curl: {error}");
            None
        }
    }
}

/// The Asset verified and renamed over `exe`. Written beside it first, so the
/// rename is on one filesystem and atomic: a crash part-way leaves the old
/// binary where it was, and the running process keeps its open inode.
fn replace_binary(exe: &Path, asset: &str, checksums: &str) -> Result<(), ReplaceFailed> {
    let list = download(checksums).ok_or(ReplaceFailed::Download)?;
    let binary = download(asset).ok_or(ReplaceFailed::Download)?;
    startup::verify(&String::from_utf8_lossy(&list), asset, &binary).inspect_err(|failed| {
        eprintln!("varde: update failed: {failed:?} for {asset}");
    })?;
    // Named per thread, so a second `:update` while the first still downloads
    // writes its own file rather than into the one being renamed.
    let temp = exe.with_file_name(format!(
        ".varde-update-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let written = std::fs::write(&temp, &binary)
        .and_then(|()| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o755))
        })
        .and_then(|()| std::fs::rename(&temp, exe));
    written.map_err(|error| {
        eprintln!("varde: update failed: replacing {}: {error}", exe.display());
        let _ = std::fs::remove_file(&temp);
        ReplaceFailed::Replace
    })
}

/// A whole file over HTTP, or `None` with the reason logged.
fn download(url: &str) -> Option<Vec<u8>> {
    let outcome = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "300", url])
        .output();
    match outcome {
        Ok(finished) if finished.status.success() => Some(finished.stdout),
        Ok(finished) => {
            eprintln!(
                "varde: update failed: {url}: {}",
                String::from_utf8_lossy(&finished.stderr).trim()
            );
            None
        }
        Err(error) => {
            eprintln!("varde: update failed: curl: {error}");
            None
        }
    }
}

/// One test run's verdict, off the main loop. Split and executed directly,
/// never through `sh -c`: the command comes from the *opened folder's* config,
/// and AGENTS.md's security rule is that nothing from the workspace is
/// interpolated into a shell command. `Pane::spawn` treats the configured AI
/// command the same way, and `shlex` — already the repo's answer for shell
/// words — is what splits it.
fn answer_tests(command: &str, root: &Path, answer: &Sender<(bool, String)>) {
    let words = shlex::split(command).unwrap_or_default();
    let Some((program, arguments)) = words.split_first() else {
        // Said out loud rather than passed: reporting a Gate that ran nothing
        // as a pass is the one failure mode worse than no Gate at all.
        let _ = answer.send((false, format!("{command:?} is not a command")));
        return;
    };
    let outcome = std::process::Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output();
    let _ = answer.send(match outcome {
        Ok(finished) => {
            let mut output = String::from_utf8_lossy(&finished.stdout).into_owned();
            output.push_str(&String::from_utf8_lossy(&finished.stderr));
            (finished.status.success(), output)
        }
        Err(error) => (false, format!("{command}: {error}")),
    });
}

/// One formatter run, off the main loop: the Buffer goes in on the child's
/// stdin and what it writes on stdout comes back. The file is never named to
/// it except as an argument its configuration asked for, and never written —
/// the text on screen is what is formatted, which is the rule the language
/// server is already held to.
///
/// A command `PATH` does not hold is the one failure this reports as its own
/// kind, because the core can act on it: it is what puts an install command on
/// the terminal's input line. Asked of the operating system here, at the moment
/// it matters, rather than remembered from a probe — nothing kept is nothing
/// that can go stale, which is the whole of formatting the moment a formatter
/// is installed.
fn answer_formatter(command: &str, args: &[String], root: &Path, text: &str) -> format::Answer {
    let words = shlex::split(command).unwrap_or_default();
    let Some((program, configured)) = words.split_first() else {
        return format::Answer::Failed(format!("{command:?} is not a command"));
    };
    let child = std::process::Command::new(program)
        .args(configured)
        .args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return format::Answer::Missing
        }
        Err(error) => return format::Answer::Failed(format!("{command}: {error}")),
    };
    // Written from a thread of its own while the parent reads, which is the
    // only order that cannot deadlock: a formatter that prints as it goes fills
    // its stdout pipe — 64 KiB, and neither end may assume more — then stops
    // and reads no further stdin, so a parent that writes the whole Buffer
    // before it reads a byte waits for a child that is waiting for it. Nothing
    // says so when it happens: the thread holds its `Sender` forever and
    // `:format` simply never answers, which is the silence AGENTS.md's error
    // rule forbids. Every shipped row happens to buffer its input, so no
    // scenario can reach this and the shape is the whole of the argument.
    let writing = child.stdin.take().map(|mut stdin| {
        let text = text.to_string();
        std::thread::spawn(move || {
            let _ = std::io::Write::write_all(&mut stdin, text.as_bytes());
        })
    });
    let output = child.wait_with_output();
    if let Some(writing) = writing {
        let _ = writing.join();
    }
    match output {
        Ok(finished) if finished.status.success() => {
            format::Answer::Done(String::from_utf8_lossy(&finished.stdout).into_owned())
        }
        Ok(finished) => {
            format::Answer::Failed(String::from_utf8_lossy(&finished.stderr).into_owned())
        }
        Err(error) => format::Answer::Failed(format!("{command}: {error}")),
    }
}

/// One analysis's figures, off the main loop. The thread reads files and
/// converts; every decision about what it found is the library's.
fn answer_figures(
    generation: u64,
    files: Option<Vec<String>>,
    base: Option<String>,
    root: &Path,
    answer: &Sender<(u64, Figures, Option<Figures>)>,
) {
    let files = files.unwrap_or_else(|| walk(root));
    let figures = measure(root, &files);
    let before = base.map(|base| measured_at(root, &files, &base));
    let _ = answer.send((generation, figures, before));
}

/// Where an Iteration's files are kept while its session edits them. Per-user
/// derived data, ignored rather than committed.
fn snapshot_dir(root: &Path, sidecar: Option<&Path>, iteration: u32) -> PathBuf {
    varde_dir(root, sidecar).join(format!("snapshots/{iteration}"))
}

/// Copies the working tree aside, so a failed Gate has somewhere to go back to
/// that is not the last commit. The walk is the git-ignore-aware one the
/// analyser uses, so the snapshot covers what a Scope could cover and no build
/// output — the whole tree whichever Scope the loop is over, because the restore
/// puts back only the files the Iteration touched and a snapshot narrower than
/// the session's reach could not put back a file it edited outside its Scope.
/// On the main loop on purpose, unlike the test run: the copy has to be
/// complete before the prompt in the same batch of effects reaches the session,
/// or the Iteration is measured against a snapshot of its own edits.
fn snapshot(root: &Path, sidecar: Option<&Path>, iteration: u32) {
    let dir = snapshot_dir(root, sidecar, iteration);
    let _ = std::fs::remove_dir_all(&dir);
    for name in walk(root) {
        // Varde's own folder is never part of a Scope: a workspace that does
        // not ignore `.varde` would otherwise have each snapshot copy the one
        // before it, and the sentinel is deleted per Iteration anyway.
        if Path::new(&name).starts_with(varde::VARDE_DIR) {
            continue;
        }
        let to = dir.join(&name);
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(root.join(&name), to);
    }
}

/// Puts back only the files this Iteration changed. A file whose bytes still
/// match the snapshot is left alone — including one the user edited before the
/// loop started and the Iteration never touched, which is the whole reason the
/// revert is not `git checkout`.
fn restore(root: &Path, sidecar: Option<&Path>, iteration: u32) {
    let dir = snapshot_dir(root, sidecar, iteration);
    for name in walk(&dir) {
        let saved = dir.join(&name);
        let live = root.join(&name);
        let changed = match (std::fs::read(&saved), std::fs::read(&live)) {
            (Ok(before), Ok(now)) => before != now,
            (Ok(_), Err(_)) => true,
            (Err(_), _) => false,
        };
        if changed {
            if let Some(parent) = live.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&saved, &live);
        }
    }
}

/// `:story`'s whole offline decision: a dirty tree means uncommitted against
/// HEAD; an explicit range is checked directly; otherwise the default-branch
/// ladder is walked. The core cannot ask git any of this, so it is decided
/// here and handed back whole.
///
/// `repo` is the repository under review and `dir` is where its artifact
/// belongs — both named by the core, because a Guest repo's Story set is
/// resolved in the clone and written into the Sidecar beside it.
fn resolve_story(
    repo: &Path,
    dir: &Path,
    explicit: Option<&str>,
    force: bool,
) -> story::Resolution {
    let refused = || match explicit {
        Some(_) => story::Resolution::BadRange,
        None => story::Resolution::NoDefaultBranch,
    };
    let Ok(repository) = git2::Repository::open(repo) else {
        return refused();
    };
    let dirty = git_status(repo).is_some_and(|files| !files.is_empty());
    let spelling = match explicit {
        Some(range) => explicit_range(&repository, range),
        None if dirty => Some("HEAD..worktree".to_string()),
        None => bare_default_branch(&repository).map(|branch| story::range(&branch, "HEAD")),
    };
    let Some(spelling) = spelling else {
        return refused();
    };
    // A spelling that resolves but has no path is a range git cannot answer
    // for — two branches with no merge-base between them. Authoring into the
    // empty string an `unwrap_or_default` would hand over writes the story set
    // nowhere, silently; a range Varde cannot name is refused like any other.
    let Some(out) = story_artifact_path(dir, &repository, &spelling) else {
        return refused();
    };
    let authored = Path::new(&out).exists();
    story::decide(force, authored, spelling, out)
}

/// The picker's whole read: whether the folder is a repository, whether its
/// working tree is clean, and every branch ref there is. Which of those refs
/// become rows — the dedup and the order — is [`story::branches`]'s, so this
/// reads and decides nothing.
fn read_branches(root: &Path) -> story::Branching {
    let Ok(repository) = git2::Repository::open(root) else {
        return story::Branching::NotARepository;
    };
    // Refused rather than assumed clean. `git_status` answers `None` for a
    // status read git could not make as well as for a folder that is no
    // repository, and a read that failed is not a statement that there is
    // nothing to lose — which is what a truthy default would have made it.
    let Some(files) = git_status(root) else {
        return story::Branching::NotARepository;
    };
    if story::uncommitted(&files) {
        return story::Branching::Dirty;
    }
    // Refused for the same reason: a ref database git will not read is not a
    // repository with no branches, and "this repository has no branches" is
    // exactly the plausible message a silent failure would wear.
    let Ok(branches) = repository.branches(None) else {
        return story::Branching::NotARepository;
    };
    let refs = branches
        .flatten()
        .filter_map(|(branch, kind)| {
            let name = branch.name().ok().flatten()?.to_string();
            let when = branch.get().peel_to_commit().ok()?.time().seconds();
            Some(story::BranchRef {
                name,
                remote: kind == git2::BranchType::Remote,
                when,
            })
        })
        .collect();
    story::Branching::Listed(refs)
}

/// Checks the picked branch out, and says which branch was left. A
/// remote-tracking branch has no local ref to move `HEAD` to, so one is created
/// pointing at the same commit — which is what `git switch` does with a branch
/// that exists only on a remote, and what makes a branch pushed but never
/// checked out reachable at all. Nothing more than the ref: the upstream, the
/// config and the remote are the reviewer's repository's, and Varde is only
/// reading their branch.
///
/// The reason is git's own words on the way out: a checkout can fail (a file in
/// the way, a ref that vanished between the read and the pick), and a picker
/// that closed and did nothing is indistinguishable from a key that did
/// nothing.
fn checkout(root: &Path, name: &str) -> Result<String, String> {
    let repository = git2::Repository::open(root).map_err(|error| error.message().to_string())?;
    let left = head_branch(root).unwrap_or_else(|| "a detached HEAD".to_string());
    let reference = match repository.find_branch(name, git2::BranchType::Local) {
        Ok(local) => local.into_reference(),
        Err(_) => {
            let remote = repository
                .branches(Some(git2::BranchType::Remote))
                .map_err(|error| error.message().to_string())?
                .flatten()
                .find(|(branch, _)| {
                    branch.name().ok().flatten().is_some_and(|full| {
                        full.split_once('/').is_some_and(|(_, rest)| rest == name)
                    })
                })
                .map(|(branch, _)| branch)
                .ok_or_else(|| format!("no branch named {name}"))?;
            let commit = remote
                .get()
                .peel_to_commit()
                .map_err(|error| error.message().to_string())?;
            let local = repository
                .branch(name, &commit, false)
                .map_err(|error| error.message().to_string())?;
            local.into_reference()
        }
    };
    let full = reference
        .name()
        .map_err(|error| error.message().to_string())?
        .to_string();
    let object = reference
        .peel(git2::ObjectType::Any)
        .map_err(|error| error.message().to_string())?;
    repository
        .checkout_tree(&object, None)
        .map_err(|error| error.message().to_string())?;
    repository
        .set_head(&full)
        .map_err(|error| error.message().to_string())?;
    Ok(left)
}

/// Everything the confirmed range's companion file is made of, gathered from
/// git: the two oids the spelling resolves to, and the range's own two sides of
/// every file it changes. The format is [`story::context_file`]'s — this reads
/// and hands over, and decides nothing.
///
/// The sides come from [`story::inventory`]'s two revisions rather than from
/// the working tree, so a fully committed range hands over its own diff and not
/// a later commit's. `None` when git cannot resolve the spelling at all: a
/// hand-over Varde cannot fill in is left unwritten rather than written empty.
fn story_context(root: &Path, spelling: &str) -> Option<String> {
    let repository = git2::Repository::open(root).ok()?;
    let (base, head) = range_ends(&repository, spelling)?;
    let base_tree = repository
        .find_object(base, None)
        .ok()?
        .peel_to_tree()
        .ok()?;
    let (head_oid, head_tree) = match head {
        None => (story::WORKTREE.to_string(), None),
        Some(oid) => (
            oid.to_string(),
            Some(
                repository
                    .find_object(oid, None)
                    .ok()?
                    .peel_to_tree()
                    .ok()?,
            ),
        ),
    };
    let base_oid = base.to_string();
    let mut entries: BTreeMap<String, story::FileHunks> = BTreeMap::new();
    diff_entries(
        &repository,
        Some(&base_tree),
        head_tree.as_ref(),
        root,
        &mut entries,
    );
    // `diff_entries` reads every new side off disk, because that is what a
    // Site's staleness needs (see `file_hunks`). A hand-over is the opposite
    // question: a committed range's new side is the commit it ends at, so a
    // later commit in the working tree cannot leak into the diff the AI is
    // told is the range's.
    let files: Vec<story::FileHunks> = entries
        .into_values()
        .map(|mut entry| {
            if let Some(tree) = head_tree.as_ref() {
                let blob = blob_at(&repository, tree, Path::new(&entry.file));
                entry.new_exists = blob.is_some();
                entry.new_text = blob.unwrap_or_default();
            }
            entry
        })
        .collect();
    Some(story::context_file(
        &base_oid,
        &head_oid,
        spelling,
        &files,
        story::CONTEXT_CAP,
    ))
}

/// An explicit range is only ever the spelling itself — git either resolves
/// both sides or the command is refused, never guessed at.
fn explicit_range(repository: &git2::Repository, range: &str) -> Option<String> {
    let (base, head, _) = story::revisions(range)?;
    if repository.revparse_single(base).is_err() {
        return None;
    }
    if head != story::WORKTREE && repository.revparse_single(head).is_err() {
        return None;
    }
    Some(range.to_string())
}

/// The offline default-branch ladder (R22.2): the remote's own idea of its
/// default branch, the current branch's configured upstream, the layered
/// `init.defaultBranch` config, then a probe for `main` and then `master`.
/// Every candidate is a fact the repository states about itself — nothing
/// here asks a remote.
fn bare_default_branch(repository: &git2::Repository) -> Option<String> {
    let shorthand = |name: &str| name.rsplit('/').next().map(str::to_string);
    let origin_head = repository
        .find_reference("refs/remotes/origin/HEAD")
        .ok()
        .and_then(|reference| {
            reference
                .symbolic_target()
                .ok()
                .flatten()
                .and_then(shorthand)
        });
    let upstream = repository
        .head()
        .ok()
        .and_then(|head| head.shorthand().ok().map(str::to_string))
        .and_then(|name| repository.find_branch(&name, git2::BranchType::Local).ok())
        .and_then(|branch| branch.upstream().ok())
        .and_then(|upstream| upstream.name().ok().flatten().and_then(shorthand));
    let configured = repository
        .config()
        .ok()
        .and_then(|config| config.get_string("init.defaultBranch").ok());
    story::default_branch(
        origin_head.as_deref(),
        upstream.as_deref(),
        configured.as_deref(),
        |name| repository.revparse_single(name).is_ok(),
    )
}

/// The path a story set for this spelling belongs at, per ADR 0005 —
/// `<base12>-<head12|worktree>.json` — computed here rather than left for
/// the AI to derive: only the edge has git, and the whole point of naming by
/// revision is that Varde, not the CLI's own guess, decides where a
/// re-authored range lands.
fn story_artifact_path(
    dir: &Path,
    repository: &git2::Repository,
    spelling: &str,
) -> Option<String> {
    let (base, head) = range_ends(repository, spelling)?;
    let head_repr = match head {
        None => story::WORKTREE.to_string(),
        Some(oid) => oid.to_string()[..12].to_string(),
    };
    Some(story::artifact_path(
        dir,
        &base.to_string()[..12],
        &head_repr,
    ))
}

/// The two commits a spelling compares — the base being the merge-base where
/// [`story::revisions`] says the spelling asked for one, which is what makes a
/// Story set cover what the head introduced rather than the difference between
/// two tips. `None` for the head is the working tree, which has no oid and no
/// merge-base to take against.
fn range_ends(
    repository: &git2::Repository,
    spelling: &str,
) -> Option<(git2::Oid, Option<git2::Oid>)> {
    let (base, head, merge_base) = story::revisions(spelling)?;
    let base_oid = repository.revparse_single(base).ok()?.id();
    if head == story::WORKTREE {
        return Some((base_oid, None));
    }
    let head_oid = repository.revparse_single(head).ok()?.id();
    let base_oid = if merge_base {
        repository.merge_base(base_oid, head_oid).ok()?
    } else {
        base_oid
    };
    Some((base_oid, Some(head_oid)))
}

/// The most recently written story set in the folder — a single artifact for
/// now, since nothing yet keys them by range; a future ticket replaces "most
/// recent" with that lookup.
fn latest_story(dir: &Path) -> Option<(PathBuf, String)> {
    let newest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _)| *modified)?
        .1;
    let contents = std::fs::read_to_string(&newest).ok()?;
    Some((newest, contents))
}

/// Every story set's filename in the folder, oldest write first — what the
/// core needs to enforce the ten-set retention (ADR 0005) against the whole
/// folder, not only whatever a live watcher event happened to catch.
fn story_set_names_oldest_first(dir: &Path) -> Vec<String> {
    let mut entries: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.file_name().to_string_lossy().into_owned()))
        })
        .collect();
    entries.sort_by_key(|(modified, _)| *modified);
    entries.into_iter().map(|(_, name)| name).collect()
}

/// Whether git can still resolve the revisions a story file is named for. The
/// core cannot ask git; reading the name and deciding what the answer means
/// belong to the library.
fn story_range_status(root: &Path, file: &Path) -> story::RangeStatus {
    match git2::Repository::open(root) {
        Ok(repository) => story::range_status(file, |revision| {
            repository.revparse_single(revision).is_ok()
        }),
        Err(_) => story::range_status(file, |_| false),
    }
}

/// Every file in the project, git-ignore aware. Dotfiles are part of the
/// project (R2.2), but .git is a database — matching its objects is noise.
fn walk(root: &Path) -> Vec<String> {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            entry
                .path()
                .strip_prefix(root)
                .ok()
                .map(|rest| rest.to_string_lossy().into_owned())
        })
        .collect()
}

/// What `HEAD` resolves to — the commit a cached figure is checked against. A
/// folder that is no repository, and one with no commit yet, both have no
/// answer, and no answer is what stops a cache from ever being believed there.
fn head_commit(root: &Path) -> Option<String> {
    let repository = git2::Repository::discover(root).ok()?;
    let head = repository.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    Some(commit.id().to_string())
}

/// The branch `HEAD` is on, or `None` on a detached HEAD — which is a state
/// worth saying rather than a name to invent. Beside [`head_commit`] because
/// the same poll tells the core both.
fn head_branch(root: &Path) -> Option<String> {
    let repository = git2::Repository::discover(root).ok()?;
    let head = repository.head().ok()?;
    head.is_branch()
        .then(|| head.shorthand().ok().map(str::to_string))
        .flatten()
}

/// The analyser's answer for every walked file in a language it handles,
/// converted into the library's own shape. Six languages implement the metrics
/// — Python, Rust, C/C++, Java, JavaScript, TypeScript/TSX — and a file in any
/// other one does not appear at all, because a figure for code nobody measured
/// would be a clean bill of health nobody was given. A file in a supported
/// language that will not read or will not parse is Unparsed instead: carried,
/// so the count can say it describes less than the whole workspace.
fn measure(root: &Path, files: &[String]) -> Figures {
    let analysed = files
        .iter()
        .filter_map(|name| {
            let path = root.join(name);
            language(&path)?;
            let space = std::fs::read(&path)
                .ok()
                .and_then(|source| spaces(&path, source));
            Some((name.clone(), space))
        })
        .collect();
    risk::figures(analysed)
}

/// The same files as one revision has them, so a delta is measured from the
/// same base as the diff beside it. Read out of git rather than off disk: the
/// working tree is the other side of the comparison, and a file the change added
/// is not in this tree at all — it is dropped, so it counts as all its own
/// figure rather than as an improvement over something that was never there.
fn measured_at(root: &Path, files: &[String], base: &str) -> Figures {
    let Ok(repository) = git2::Repository::open(root) else {
        return Figures::default();
    };
    // No such revision is not the same as a revision holding nothing: neither
    // yields figures, and the delta is over what was read either way.
    let tree = git2::Oid::from_str(base)
        .ok()
        .and_then(|oid| repository.find_commit(oid).ok())
        .and_then(|commit| commit.tree().ok());
    let Some(tree) = tree else {
        return Figures::default();
    };
    let analysed = files
        .iter()
        .filter_map(|name| {
            let path = root.join(name);
            language(&path)?;
            let blob = tree
                .get_path(Path::new(name))
                .ok()
                .and_then(|entry| repository.find_blob(entry.id()).ok())?;
            Some((name.clone(), spaces(&path, blob.content().to_vec())))
        })
        .collect();
    risk::figures(analysed)
}

/// Which language the analyser handles this file as, if it handles it at all.
/// Which languages those are is the analyser's own answer, not a list here — and
/// a file in any other one is not part of the answer, so it is dropped rather
/// than reported as something Varde failed at.
fn language(path: &Path) -> Option<rust_code_analysis::LANG> {
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    rust_code_analysis::get_from_ext(&extension)
}

/// One file's space tree, from bytes rather than from a path, so the same
/// conversion measures the working tree and a blob at the base revision. `None`
/// where the analyser handles the language and could not parse it, which
/// `risk::figures` counts as Unparsed.
fn spaces(path: &Path, source: Vec<u8>) -> Option<Space> {
    let language = language(path)?;
    rust_code_analysis::get_function_spaces(&language, source, path, None)
        .map(|analysed| space(&analysed))
}

/// The analyser's tree as the library's own, field for field. The *sum*
/// getters, not the per-space ones: the parent's sum already carries every
/// child's contribution, which is what makes a closure's branching part of the
/// Function holding it (`the_metrics_of_a_parent_space_include_its_children`).
/// Whole numbers, per `risk::Metrics`.
fn space(analysed: &rust_code_analysis::FuncSpace) -> Space {
    Space {
        name: analysed.name.clone(),
        line: analysed.start_line,
        kind: match analysed.kind {
            rust_code_analysis::SpaceKind::Unknown => risk::Kind::Unknown,
            rust_code_analysis::SpaceKind::Function => risk::Kind::Function,
            rust_code_analysis::SpaceKind::Class => risk::Kind::Class,
            rust_code_analysis::SpaceKind::Struct => risk::Kind::Struct,
            rust_code_analysis::SpaceKind::Trait => risk::Kind::Trait,
            rust_code_analysis::SpaceKind::Impl => risk::Kind::Impl,
            rust_code_analysis::SpaceKind::Unit => risk::Kind::Unit,
            rust_code_analysis::SpaceKind::Namespace => risk::Kind::Namespace,
            rust_code_analysis::SpaceKind::Interface => risk::Kind::Interface,
        },
        metrics: Metrics {
            cyclomatic: analysed.metrics.cyclomatic.cyclomatic_sum() as u32,
            cognitive: analysed.metrics.cognitive.cognitive_sum() as u32,
            // The index is a formula over Halstead volume and line counts, so
            // a space it cannot be computed for comes back `NaN` — recorded as
            // 0, the same as the least maintainable code there is, because a
            // figure and a missing figure are both worth looking at and neither
            // may read as "fine". Surfacing the difference is `06`'s Unparsed.
            maintainability: analysed.metrics.mi.mi_visual_studio().max(0.0) as u32,
            lines: analysed.metrics.loc.sloc() as u32,
        },
        children: analysed.spaces.iter().map(space).collect(),
    }
}

/// Runs a settled query: reads the project, lets open buffers stand in for
/// their files, and scans.
fn run_search(state: &State, root: &Path, query: &str) -> varde::search::Results {
    let disk = walk(root)
        .into_iter()
        .filter_map(|name| {
            std::fs::read_to_string(root.join(&name))
                .ok()
                .map(|contents| (name, contents))
        })
        .collect();
    varde::search::scan(query, &varde::search::sources(state, disk))
}

/// One file's diff as the edge reads it: the rows, the blob oid of what is on
/// screen, and each side highlighted whole and indexed by line.
///
/// The sides are parsed here rather than at draw time for the reason the
/// buffer's tokens are, and they are parsed *whole* for the reason
/// [`varde::review::diff_tokens`] takes whole ones: a diff is two sources
/// interleaved, and a row highlighted on its own restarts every token that
/// spans lines. `old` is empty where there is nothing to read — no repository,
/// no commit, a file `HEAD` does not have.
#[derive(Default)]
struct Diff {
    lines: Vec<varde::DiffLine>,
    revision: String,
    new_side: Vec<Vec<varde::highlight::Token>>,
    old_side: Vec<Vec<varde::highlight::Token>>,
}

/// A unified diff of one file against HEAD, with the new-file line numbers a
/// comment anchors to, and the blob oid of what's on screen — computed here,
/// in the `Repository::open` this already does, rather than derived later
/// under a comment already made.
fn diff_lines(root: &Path, path: &Path) -> Diff {
    let Ok(repository) = git2::Repository::open(root) else {
        return Diff::default();
    };
    let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    let mut options = git2::DiffOptions::new();
    options
        .pathspec(&relative)
        .context_lines(story::CONTEXT_LINES)
        // A file the AI just created is untracked and has no index entry, so
        // without this it lists in the review and shows an empty diff.
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
    // R7.1 measures against HEAD, so staged changes show too. An empty repo has
    // no HEAD to compare with.
    let head = repository
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok());
    let diff = match head {
        Some(tree) => repository.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut options)),
        None => repository.diff_index_to_workdir(None, Some(&mut options)),
    };
    let Ok(diff) = diff else {
        return Diff::default();
    };
    let mut lines = Vec::new();
    let _ = diff.print(git2::DiffFormat::Patch, |_, _, line| {
        if matches!(line.origin(), '+' | '-' | ' ') {
            lines.push(varde::DiffLine {
                new_line: line.new_lineno().map(|n| n as usize),
                old_line: line.old_lineno().map(|n| n as usize),
                removed: line.origin() == '-',
                text: String::from_utf8_lossy(line.content())
                    .trim_end()
                    .to_string(),
            });
        }
        true
    });
    // A revision is the blob oid of the content on screen: the working-tree
    // file when there is one, or HEAD's blob for a file already deleted from
    // disk — the reviewer commented on the old side of the diff.
    let revision = match std::fs::read(path) {
        Ok(bytes) => git2::Oid::hash_object(git2::ObjectType::Blob, &bytes)
            .map(|oid| oid.to_string())
            .unwrap_or_default(),
        Err(_) => repository
            .head()
            .ok()
            .and_then(|head| head.peel_to_tree().ok())
            .and_then(|tree| tree.get_path(&relative).ok())
            .map(|entry| entry.id().to_string())
            .unwrap_or_default(),
    };
    // The name the grammar is chosen by, not the path: the same lookup the
    // buffer's tokens go through.
    let name = relative
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let old_text = repository
        .head()
        .ok()
        .and_then(|head| head.peel_to_tree().ok())
        .and_then(|tree| tree.get_path(&relative).ok())
        .and_then(|entry| repository.find_blob(entry.id()).ok())
        .map(|blob| String::from_utf8_lossy(blob.content()).into_owned());
    Diff {
        lines,
        revision,
        new_side: varde::highlight::highlight(
            &name,
            &std::fs::read_to_string(path).unwrap_or_default(),
        ),
        old_side: old_text
            .map(|text| varde::highlight::highlight(&name, &text))
            .unwrap_or_default(),
    }
}

/// Notices are slugs in the core so no scenario asserts on wording. The words
/// belong here. A table rather than a match: nothing is decided, so nothing
/// branches — the Gate's conditions are named by the library so the words and
/// the slug cannot drift apart, since each revert says which one closed the
/// Gate and "the loop gave up" is the report nobody can act on.
const NOTICES: &[(&str, &str, ui::Tone)] = &[
    (
        "broken-config",
        "Config not applied — still running on the last one that worked",
        ui::Tone::Warning,
    ),
    (
        "checkout-failed",
        "Could not check that branch out — you are still on the branch you were on",
        ui::Tone::Warning,
    ),
    (
        "review-empty",
        "Nothing to submit — add a comment first",
        ui::Tone::Warning,
    ),
    (
        "unsaved-changes",
        "Unsaved changes — :w to write, :e to discard, :q! to close anyway",
        ui::Tone::Warning,
    ),
    (
        "language-server-failed",
        "Could not start the language server — see .varde/lsp-<language>.log, and its command in .varde/config.toml",
        ui::Tone::Warning,
    ),
    (
        "language-server-stopped",
        "The language server stopped — see .varde/lsp-<language>.log; no diagnostics, hover or completion until it is restarted",
        ui::Tone::Warning,
    ),
    (
        "no-debug-adapter",
        "No Debug adapter — install it from Tools (Ctrl+Space v)",
        ui::Tone::Warning,
    ),
    (
        "debug-adapter-failed",
        "The Debug adapter could not be started — its log is .varde/dap.log",
        ui::Tone::Warning,
    ),
    (
        "debug-adapter-exited",
        "The Debug adapter stopped, and the Debug session with it — its log is .varde/dap.log",
        ui::Tone::Warning,
    ),
    (
        "launch-failed",
        "The Debug adapter would not start the program",
        ui::Tone::Warning,
    ),
    (
        "no-language-server",
        "No language server answering for this file — none is configured, or it has not started",
        ui::Tone::Warning,
    ),
    (
        "no-hover-support",
        "This language server does not answer hover",
        ui::Tone::Notice,
    ),
    (
        "no-definition-support",
        "This language server does not answer go-to-definition",
        ui::Tone::Notice,
    ),
    (
        "no-definition",
        "The language server knows of no definition for the symbol under the cursor",
        ui::Tone::Notice,
    ),
    (
        "definition-outside-workspace",
        "That definition is outside this workspace, so it was not opened",
        ui::Tone::Warning,
    ),
    (
        "nothing-known-here",
        "The language server knows nothing about the symbol under the cursor",
        ui::Tone::Notice,
    ),
    // The two sentences above are false when Varde is what declined to answer,
    // and they are the sentences that sent a reader looking at their own
    // install instead of at Varde. One slug for both keys: it is one refusal.
    (
        "needs-a-companion",
        "This language server relays that question to a second server Varde does not run — see .varde/config.toml",
        ui::Tone::Warning,
    ),
    (
        "language-server-error",
        "The language server refused the question — its reply was an error",
        ui::Tone::Warning,
    ),
    (
        "nothing-to-format",
        "Nothing to change in this file",
        ui::Tone::Notice,
    ),
    (
        "no-formatter-configured",
        "Nothing is configured to format this file; add a row for it",
        ui::Tone::Warning,
    ),
    (
        "formatter-missing",
        "That formatter is not installed",
        ui::Tone::Warning,
    ),
    (
        "formatter-failed",
        "The formatter refused the file",
        ui::Tone::Warning,
    ),
    (
        "ai-already-running",
        "An AI is already running — :ai! <command> to replace it",
        ui::Tone::Warning,
    ),
    (
        "comment-added",
        "Comment added — :submit to send the review to the AI",
        ui::Tone::Notice,
    ),
    (
        "nothing-to-comment",
        "Select an added or unchanged line — removed lines cannot be commented on",
        ui::Tone::Warning,
    ),
    (
        "buffer-diverged",
        "Changed on disk under your unsaved edits — D to resolve",
        ui::Tone::Warning,
    ),
    (
        "no-test-command",
        "No test command — set risk.test_command in .varde/config.toml",
        ui::Tone::Warning,
    ),
    (
        risk::TESTS_FAILED,
        "The tests failed — the pass was put back and the loop stopped",
        ui::Tone::Warning,
    ),
    (
        risk::NO_IMPROVEMENT,
        "The pass lowered nothing — it was put back and the loop stopped",
        ui::Tone::Warning,
    ),
    (
        risk::OTHER_METRIC_WORSENED,
        "The pass raised another metric — it was put back and the loop stopped",
        ui::Tone::Warning,
    ),
    (
        risk::NO_BASELINE,
        "The pass could not be judged — nothing measured it, so it was put back",
        ui::Tone::Warning,
    ),
    (
        risk::LOOP_ALREADY_RUNNING,
        "A Refactor loop is already running — stop it before starting another",
        ui::Tone::Warning,
    ),
    (
        risk::NO_FIGURE,
        "Risk has not been measured yet — the loop has no baseline to judge a pass against",
        ui::Tone::Warning,
    ),
    (
        risk::STOPPED,
        "The loop stopped — the passes that stood are in the working tree",
        ui::Tone::Notice,
    ),
    (
        risk::CAP_REACHED,
        "The Iteration cap was reached — the passes that stood are in the working tree",
        ui::Tone::Notice,
    ),
    (
        "buffers-closed",
        "Every buffer is closed",
        ui::Tone::Notice,
    ),
    // Named, not counted: which files are still open is what says whether the
    // work left unsaved is work worth going back to. The slug's words end where
    // the names begin, since the status line joins the two with a colon.
    (
        "buffers-kept",
        "Closed the clean buffers — still open, with unsaved edits in",
        ui::Tone::Notice,
    ),
    (
        "nothing-diverged",
        "Nothing to resolve — this buffer agrees with the file on disk",
        ui::Tone::Notice,
    ),
    (
        "no-earlier-place",
        "No earlier place — this is the oldest place the cursor has been",
        ui::Tone::Warning,
    ),
    (
        "no-later-place",
        "No later place — this is where the cursor was before it went back",
        ui::Tone::Warning,
    ),
    (
        "no-place-here",
        "The Cursor history is not standing on a place — arrow onto a row first",
        ui::Tone::Warning,
    ),
    // The words end where the chord begins, the way `buffers-kept`'s do: the
    // core sends the keys that were typed as the `about`, and the status line
    // joins the two with a colon.
    ("no-such-motion", "No such motion", ui::Tone::Warning),
    (
        "nothing-to-update",
        "Nothing to update from — no checkout of Varde, and no newer Release found",
        ui::Tone::Warning,
    ),
    // One per step of replacing the binary, so a network error, a Release with
    // nothing for this machine and a bad download read differently.
    (
        "update-download",
        "Update failed — the Release could not be downloaded",
        ui::Tone::Warning,
    ),
    (
        "update-no-asset",
        "Update failed — the Release's checksum list has nothing for this platform",
        ui::Tone::Warning,
    ),
    (
        "update-checksum",
        "Update refused — the download does not match its published checksum",
        ui::Tone::Warning,
    ),
    (
        "update-replace",
        "Update failed — the running binary could not be replaced",
        ui::Tone::Warning,
    ),
    // F35's five. Three of them are the only thing standing between a missing
    // piece and silence, and silence is also what a Reading sounds like before
    // its first word — which is the whole reason ADR 0013 makes refusing out
    // loud a requirement rather than a courtesy. Each names *which* piece,
    // because "reading did not work" sends nobody anywhere.
    (
        "nothing-selected",
        "Select the passage to read first — a Reading covers the selection and nothing else",
        ui::Tone::Warning,
    ),
    (
        "not-markdown",
        "Only a markdown buffer can be read aloud",
        ui::Tone::Warning,
    ),
    (
        "no-voice",
        "No voice on this machine — press i to install the speech row, which fetches one and names it in ~/.varde/config.toml",
        ui::Tone::Warning,
    ),
    (
        "no-synthesizer",
        "No speech synthesizer — the command speech.command names is not running; press i to install the speech row",
        ui::Tone::Warning,
    ),
    (
        "no-player",
        "No audio player — set speech.player in ~/.varde/config.toml to something on this machine",
        ui::Tone::Warning,
    ),
];

/// The words for a slug — and the slug itself for one nobody has worded yet.
/// An empty string was the fallback, which is how three notices this ticket's
/// core returns reached a status line that said nothing at all: the scenarios
/// assert the slug, by design, so nothing here was ever checked. A terse
/// message is a bug somebody reports; silence is a feature nobody knows fired.
fn notice_text(notice: &str) -> (&str, ui::Tone) {
    NOTICES
        .iter()
        .find(|(slug, _, _)| *slug == notice)
        .map_or((notice, ui::Tone::Warning), |(_, text, tone)| {
            (*text, *tone)
        })
}

/// Adds and drops watches to match the folders the core says it needs — never a
/// whole subtree, which is what keeps .git's objects and node_modules off the
/// watch list. Which folders those are is `varde::watched_folders`' decision:
/// the set used to be assembled here, and a set assembled in `main.rs` is a set
/// with no test, which is how a buffer in a collapsed folder came to be watched
/// by nobody.
fn sync_watches(
    state: &State,
    watcher: &mut notify::RecommendedWatcher,
    watched: &mut BTreeSet<PathBuf>,
) {
    let wanted = varde::watched_folders(state);
    for path in wanted.difference(watched).cloned().collect::<Vec<_>>() {
        if watcher.watch(&path, RecursiveMode::NonRecursive).is_ok() {
            watched.insert(path);
        }
    }
    for path in watched.difference(&wanted).cloned().collect::<Vec<_>>() {
        let _ = watcher.unwatch(&path);
        watched.remove(&path);
    }
}

fn collect_watch_events(
    receiver: &Receiver<notify::Result<notify::Event>>,
    state: &State,
    queue: &mut VecDeque<Event>,
) {
    let git_dir = state.root.join(".git");
    let stories_dir = varde_dir(&state.root, state.sidecar.as_deref()).join("stories");
    let global = state.varde_home.join(startup::CONFIG_FILE);
    let project = varde_dir(&state.root, state.sidecar.as_deref()).join(startup::CONFIG_FILE);
    let mut edited = false;
    while let Ok(Ok(event)) = receiver.try_recv() {
        let notify::Event { kind, paths, .. } = event;
        for path in paths {
            // Not `Access`: inotify reports an open, and reading the layers
            // below opens them — counting it would reload once per batch forever.
            edited |= !matches!(kind, notify::EventKind::Access(_))
                && (path == global || path == project);
            // `~/.varde` is watched for its config alone: nothing else in it
            // is the workspace's.
            if path.parent() == Some(state.varde_home.as_path()) && !path.starts_with(&state.root) {
                continue;
            }
            watched_path(&kind, path, state, &git_dir, &stories_dir, queue);
        }
    }
    // Both layers, once per batch however many events a save made: the merge
    // needs the one that did not change as much as the one that did.
    if edited {
        queue.push_back(Event::ConfigEdited {
            global: on_disk(&global),
            project: on_disk(&project),
        });
    }
}

fn on_disk(path: &Path) -> startup::OnDisk {
    match std::fs::read_to_string(path) {
        Ok(text) => startup::OnDisk::Text(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => startup::OnDisk::Missing,
        Err(error) => {
            eprintln!("varde: cannot read {}: {error}", path.display());
            startup::OnDisk::Unreadable
        }
    }
}

/// What one watched path's change means to the core.
fn watched_path(
    kind: &notify::EventKind,
    path: PathBuf,
    state: &State,
    git_dir: &Path,
    stories_dir: &Path,
    queue: &mut VecDeque<Event>,
) {
    // Git's own bookkeeping and the artifact channel are not folder contents:
    // the lock files git makes and unmakes on every command are churn, not a
    // tree changing.
    let ours = !path.starts_with(git_dir) && !path.starts_with(stories_dir);
    match kind {
        // The artifact channel (ADR 0006): a `.json` landing here is the AI's
        // whole report, never a tree entry — the core reads it as a story, not
        // as a file that appeared in a folder. `Modify` counts too:
        // re-authoring (`:story!`) writes the same name again (a story dies
        // with its range), which on most platforms is a modify of the existing
        // file, not a fresh create — missing that would leave a re-authored
        // range waiting forever.
        notify::EventKind::Create(_) | notify::EventKind::Modify(_)
            if path.starts_with(stories_dir)
                && path.extension().is_some_and(|ext| ext == "json") =>
        {
            queue_story_file(&path, state, queue);
        }
        // The same rule the create arm below follows, for the other direction:
        // ask the filesystem rather than trusting the event kind. A move is a
        // name-modification on the platforms that report it at all, and a
        // watcher that coalesced a burst reports whatever it settled on — so a
        // watched path that no longer exists is a removal, whatever the
        // platform called the change. One rule, rather than a case per platform
        // spelling. Before this, a move fell into the catch-all modify arm,
        // which tried to read the path as text, failed because it was gone, and
        // queued nothing: the tree went on describing the folder as it was.
        // `symlink_metadata`, not `exists`: a symlink whose target is gone is
        // still an entry in the folder.
        _ if ours && std::fs::symlink_metadata(&path).is_err() => {
            queue.push_back(Event::FilesRemoved(vec![path]));
        }
        // A rename's other half, which the rule above cannot reach: the name a
        // move lands on is a path that exists, so only what left was reported
        // and the tree never heard what arrived. Measured on macOS, `mv a b`
        // reports `Modify(Name(Any))` on both paths — the two halves differ by
        // nothing but whether the file is still there, which is the same
        // question the rule above asks. A name change is the one modify that
        // *makes* an entry rather than altering one, so the tree is told; the
        // core's appearance arm already refuses a name it knows, and that guard
        // is what makes reporting a rename as a removal and an appearance safe.
        // The contents follow because renaming a temp file over the target is
        // how `sed -i`, most editors and most AI CLIs save, and the buffer
        // following the file has to hear that too.
        notify::EventKind::Modify(notify::event::ModifyKind::Name(_)) if ours => {
            let entry = if std::fs::metadata(&path).is_ok_and(|meta| meta.is_dir()) {
                tree::Kind::Folder
            } else {
                tree::Kind::File
            };
            queue.push_back(Event::FilesAppeared(vec![(path.clone(), entry)]));
            if let Ok(contents) = std::fs::read_to_string(&path) {
                queue.push_back(Event::FileChanged { path, contents });
            }
        }
        notify::EventKind::Create(_) => {
            // notify's own create kind is `Any` on some platforms, so ask the
            // filesystem rather than trusting it.
            let kind = if std::fs::metadata(&path).is_ok_and(|meta| meta.is_dir()) {
                tree::Kind::Folder
            } else {
                tree::Kind::File
            };
            queue.push_back(Event::FilesAppeared(vec![(path, kind)]));
        }
        notify::EventKind::Remove(_) => {
            queue.push_back(Event::FilesRemoved(vec![path]));
        }
        // What a change means is update's decision — an open buffer, the diff
        // on screen, or nothing. The edge only keeps out what is not ours and a
        // file it cannot read as text: reporting that as empty would blank the
        // buffer it claims to follow. The file is still there — the arm above
        // has already answered for a path that is not.
        notify::EventKind::Modify(_) if ours => {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                queue.push_back(Event::FileChanged { path, contents });
            }
        }
        _ => {}
    }
}

/// Told together, in this order: a name the core cannot yet load into anything
/// is not a set it should count toward retention.
fn queue_story_file(path: &Path, state: &State, queue: &mut VecDeque<Event>) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    if let Some(name) = path.file_name() {
        queue.push_back(Event::StoryFileWritten(name.to_string_lossy().into_owned()));
    }
    let range = story_range_status(&state.root, path);
    queue.push_back(Event::StoryArtifact { contents, range });
}

#[cfg(test)]
mod tests {
    use super::{
        alive, authorship, checkout, committed, flatten, found, notice_text, pid_of, range_ends,
        read_branches, rewrite, samples, short_date, sidecar, space, stitch, watched_path,
    };
    use notify::event::{CreateKind, DataChange, ModifyKind, RenameMode};
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;
    use std::collections::VecDeque;
    use std::fs;
    use std::path::{Path, PathBuf};
    use varde::risk;
    use varde::startup::{Fact, FactValue};
    use varde::story;
    use varde::{tree, Event, State};

    /// R35.4's silence, in the unit the container counts in. A sample is one
    /// channel's, so a stereo second is twice a mono one — counting frames
    /// would halve every gap the ear was promised on a stereo voice.
    #[test]
    fn a_gap_is_counted_in_samples_and_not_in_frames() {
        let mono = hound::WavSpec {
            channels: 1,
            sample_rate: 22050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        assert_eq!(samples(mono, 1000), 22050);
        assert_eq!(samples(mono, 550), 12127);
        assert_eq!(samples(mono, 0), 0);
        let stereo = hound::WavSpec {
            channels: 2,
            ..mono
        };
        assert_eq!(samples(stereo, 1000), 44100);
    }

    /// R35.4, at the one seam no scenario can reach: the Utterances and the
    /// silence between them are **one** file by the time a player is spawned.
    /// Two wavs in, one out, and its length is both of them plus the gap — a
    /// stream missing the silence is the staccato the whole ticket exists to
    /// prevent, and it would otherwise be a deletion nothing turns red.
    ///
    /// Real files because the container is the thing under test. The pieces
    /// are the synthesizer's and are gone afterwards, which is the other half
    /// of what `stitch` promises.
    #[test]
    fn a_reading_is_one_stream_holding_its_utterances_and_their_silence() {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 22050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let dir = tempfile::tempdir().expect("a temporary directory");
        let piece = |name: &str, samples: u32| {
            let path = dir.path().join(name);
            let mut writer = hound::WavWriter::create(&path, spec).expect("a wav");
            for _ in 0..samples {
                writer.write_sample(1i16).expect("a sample");
            }
            writer.finalize().expect("a finished wav");
            path
        };
        let said = vec![
            (piece("one.wav", 22050), 1000),
            (piece("two.wav", 11025), 0),
        ];

        let (stream, offsets) = stitch(dir.path(), &said).expect("a stream");

        let built = hound::WavReader::open(&stream).expect("the stream");
        assert_eq!(built.duration(), 22050 + 22050 + 11025);
        // R35.6's seek targets: a second of speech, then a second of silence,
        // so the second Utterance starts two seconds in. Measured off the
        // audio rather than guessed from the text, which is the whole reason
        // the core is told them.
        assert_eq!(offsets, [0, 2000]);
        for (piece, _) in &said {
            assert!(!piece.exists(), "the pieces are gone: {piece:?}");
        }
    }

    /// R35.6's resume, at the seam no scenario can reach: the stream from an
    /// offset is the stream minus that much audio, and the offset is in
    /// milliseconds of *sound* — a rewrite that counted frames on a stereo
    /// voice would land at half the sentence it was asked for.
    #[test]
    fn a_rewrite_from_an_offset_drops_exactly_that_much_sound() {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 22050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let dir = tempfile::tempdir().expect("a temporary directory");
        let whole = dir.path().join("reading.wav");
        let mut writer = hound::WavWriter::create(&whole, spec).expect("a wav");
        for _ in 0..44100 {
            writer.write_sample(1i16).expect("a sample");
        }
        writer.finalize().expect("a finished wav");

        let tail = dir.path().join("from.wav");
        rewrite(&whole, &tail, 500).expect("a rewritten stream");

        assert_eq!(
            hound::WavReader::open(&tail).expect("the tail").duration(),
            44100 - 11025
        );
        assert!(whole.exists(), "the whole stream is what a seek cuts from");
    }

    /// The shipped fact, spelled here so these tests search for what Varde
    /// actually ships without reaching for the whole config merge. The
    /// `PROGRAMS` unit test in `startup.rs` holds the two spellings level.
    fn typescript_sdk() -> Fact {
        Fact {
            marker: "node_modules/typescript/lib/typescript.js".to_string(),
            value: FactValue::Directory,
            command: None,
            command_marker: None,
            optional: false,
            install: Default::default(),
        }
    }

    /// The two halves of the Sidecar's name, read back the way the sweep reads
    /// them. Two instances on one folder are two Sidecars because the pid is
    /// in the name, and the sweep can find the pid again even when the folder
    /// name holds a `-` of its own.
    #[test]
    fn a_sidecar_names_the_process_that_owns_it() {
        let mine = sidecar(Path::new("/home/me/my-projects/theirs"));
        let name = mine
            .file_name()
            .expect("a directory name")
            .to_string_lossy();
        assert_eq!(pid_of(&name), Some(std::process::id()));
        assert!(alive(std::process::id()));
    }

    /// Two folders are two Sidecars. The pid keeps two *instances* apart; this
    /// is what keeps two *folders* apart, and it is the whole reason the
    /// separator is not spelled with a character a folder name may hold.
    #[test]
    fn flattening_a_path_collides_with_no_other_path() {
        assert_eq!(
            flatten(Path::new("/home/me/projects/varde")),
            "%home%me%projects%varde"
        );
        assert_ne!(flatten(Path::new("/a/b")), flatten(Path::new("/a%b")));
    }

    fn from(dirs: &[&Path]) -> BTreeSet<PathBuf> {
        dirs.iter().map(|dir| dir.to_path_buf()).collect()
    }

    /// A workspace's own toolchain is what a server is pointed at, since it is
    /// the one the project's code was written against.
    #[test]
    fn a_workspace_marker_is_what_the_server_is_pointed_at() {
        let root = tempfile::tempdir().expect("a temp directory");
        let lib = root.path().join("node_modules/typescript/lib");
        fs::create_dir_all(&lib).expect("the fixture layout");
        fs::write(lib.join("typescript.js"), "").expect("the SDK file");

        assert_eq!(
            found(&typescript_sdk(), root.path(), &BTreeSet::new()).as_deref(),
            Some(lib.to_string_lossy().as_ref())
        );
    }

    /// The defect this ticket exists for. A pnpm monorepo installs its
    /// dependencies per package, so the root holds a `node_modules` with no
    /// TypeScript in it and the package holds the only usable SDK. Searching
    /// from the root found nothing and the Vue server was spawned without
    /// `--tsdk=`, fell back to `require('typescript')`, reached the unusable
    /// 7.0 global and died — while every other editor served the same file
    /// with no configuration at all.
    #[test]
    fn a_monorepo_resolves_from_the_file_rather_than_from_the_root() {
        let root = tempfile::tempdir().expect("a temp directory");
        fs::create_dir_all(root.path().join("node_modules/.bin")).expect("a root install");
        let package = root.path().join("components/frontend");
        let lib = package.join("node_modules/typescript/lib");
        fs::create_dir_all(&lib).expect("the fixture layout");
        fs::write(lib.join("typescript.js"), "").expect("the SDK file");
        let source = package.join("src/components");
        fs::create_dir_all(&source).expect("the source layout");

        assert_eq!(
            found(&typescript_sdk(), root.path(), &from(&[&source])).as_deref(),
            Some(lib.to_string_lossy().as_ref()),
            "the SDK the package installed is the one that serves the package's files"
        );
    }

    /// The same monorepo with nothing open, which is the state Tools
    /// is read in: the walk starts at the open files, so a workspace with no
    /// file of that language open had only the root to search and the row said
    /// `missing-requirement` about a workspace holding the SDK all along —
    /// then said `installed` as soon as a `.vue` file was opened, which is the
    /// "sometimes it works" this ticket reports.
    #[test]
    fn a_package_holds_the_answer_with_nothing_open_at_all() {
        let root = tempfile::tempdir().expect("a temp directory");
        fs::create_dir_all(root.path().join("node_modules/.bin")).expect("a root install");
        let lib = root
            .path()
            .join("components/frontend/node_modules/typescript/lib");
        fs::create_dir_all(&lib).expect("the fixture layout");
        fs::write(lib.join("typescript.js"), "").expect("the SDK file");

        assert_eq!(
            found(&typescript_sdk(), root.path(), &BTreeSet::new()).as_deref(),
            Some(lib.to_string_lossy().as_ref()),
            "a workspace answers for itself whether or not one of its files is open"
        );
    }

    /// And the workspace-wide search is the last resort rather than the first:
    /// the package serving the open file installed its own SDK, and the
    /// shallower one belonging to a package nobody has open must not take it.
    #[test]
    fn the_open_file_still_outranks_a_shallower_package() {
        let root = tempfile::tempdir().expect("a temp directory");
        let other = root.path().join("apps/node_modules/typescript/lib");
        fs::create_dir_all(&other).expect("the other package");
        fs::write(other.join("typescript.js"), "").expect("the other SDK file");
        let package = root.path().join("components/frontend");
        let lib = package.join("node_modules/typescript/lib");
        fs::create_dir_all(&lib).expect("the fixture layout");
        fs::write(lib.join("typescript.js"), "").expect("the SDK file");
        let source = package.join("src");
        fs::create_dir_all(&source).expect("the source layout");

        assert_eq!(
            found(&typescript_sdk(), root.path(), &from(&[&source])).as_deref(),
            Some(lib.to_string_lossy().as_ref())
        );
    }

    /// Never above the workspace root: a toolchain outside the workspace must
    /// not serve a file inside it. The fixture puts the marker one directory
    /// above the root, which the walk would reach if it did not stop.
    #[test]
    fn the_search_stops_at_the_workspace_root() {
        let outside = tempfile::tempdir().expect("a temp directory");
        let lib = outside.path().join("node_modules/typescript/lib");
        fs::create_dir_all(&lib).expect("the fixture layout");
        fs::write(lib.join("typescript.js"), "").expect("the SDK file");
        let root = outside.path().join("workspace");
        let source = root.join("src");
        fs::create_dir_all(&source).expect("the workspace layout");

        assert_eq!(found(&typescript_sdk(), &root, &from(&[&source])), None);
    }

    /// The measured failure the marker exists for: `typescript` 7.0, the
    /// native preview, installs a `lib/` holding `tsc.js` and no
    /// `typescript.js` at all. It is installed, it is on `PATH`, and a server
    /// pointed at it resolves an undefined module and dies on the first
    /// `didOpen` — so the package being there is not the test, and the marker
    /// file being there is (R31.27). A fixture rather than a comment because
    /// no version string reports the difference.
    #[test]
    fn a_package_that_is_not_an_sdk_is_not_offered_as_one() {
        let root = tempfile::tempdir().expect("a temp directory");
        let lib = root.path().join("node_modules/typescript/lib");
        fs::create_dir_all(&lib).expect("the fixture layout");
        fs::write(lib.join("tsc.js"), "").expect("the compiler file");

        assert_ne!(
            found(&typescript_sdk(), root.path(), &BTreeSet::new()).as_deref(),
            Some(lib.to_string_lossy().as_ref())
        );
    }

    /// A workspace with nothing installed anywhere in it names no directory of
    /// its own, and a fact with no global fallback declared says nothing at
    /// all.
    #[test]
    fn a_workspace_with_no_marker_names_no_directory_of_its_own() {
        let root = tempfile::tempdir().expect("a temp directory");
        let source = root.path().join("src");
        fs::create_dir_all(&source).expect("the workspace layout");

        assert_eq!(
            found(&typescript_sdk(), root.path(), &from(&[&source])),
            None
        );
    }

    /// The marker itself where the fact asks for it — pyright wants the
    /// interpreter, not the `bin` holding it — which is why what is handed
    /// over is declared rather than guessed from the marker's shape.
    #[test]
    fn a_fact_may_hand_over_the_marker_rather_than_its_directory() {
        let root = tempfile::tempdir().expect("a temp directory");
        let bin = root.path().join("services/api/.venv/bin");
        fs::create_dir_all(&bin).expect("the fixture layout");
        fs::write(bin.join("python"), "").expect("the interpreter");
        let source = root.path().join("services/api/src");
        fs::create_dir_all(&source).expect("the source layout");

        let interpreter = Fact {
            marker: ".venv/bin/python".to_string(),
            value: FactValue::Marker,
            command: None,
            command_marker: None,
            optional: false,
            install: Default::default(),
        };
        assert_eq!(
            found(&interpreter, root.path(), &from(&[&source])).as_deref(),
            Some(bin.join("python").to_string_lossy().as_ref())
        );
    }

    /// A Function holding a closure, both of them branching. The rule that a
    /// closure counts toward the Function holding it rests on the analyser's
    /// parent-space metrics *including* their children's contributions — an
    /// assumption, until this test. If a future release makes subspace metrics
    /// exclusive this goes red, and the Function rule under-reports until the
    /// conversion adds the children up itself. Settled: they are inclusive.
    const FIXTURE: &str = "fn outer(flag: bool) -> usize {\n    let inner = |n: usize| if n > 1 { n } else { 0 };\n    if flag { inner(2) } else { inner(0) }\n}\n";

    #[test]
    fn the_metrics_of_a_parent_space_include_its_children() {
        let analysed = rust_code_analysis::get_function_spaces(
            &rust_code_analysis::LANG::Rust,
            FIXTURE.as_bytes().to_vec(),
            std::path::Path::new("fixture.rs"),
            None,
        )
        .expect("the fixture parses");
        let (functions, unparsed) = risk::functions("fixture.rs", &space(&analysed));

        assert_eq!(unparsed, 0);
        assert_eq!(
            functions
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["outer"],
            "the closure is not a Function of its own"
        );
        // Hand-checked: `outer` branches once and the closure branches once,
        // and each space carries its own entry path, so 2 + 2 is the whole
        // figure the Function is charged. Cognitive charges nesting, so the
        // closure's 3 sits on top of `outer`'s own 2.
        assert_eq!(functions[0].metrics.cyclomatic, 4);
        assert_eq!(functions[0].metrics.cognitive, 5);
        // Recorded alongside the primary figure even though only the primary
        // one is displayed: the Gate reads every metric it was given.
        assert_eq!(functions[0].metrics.lines, 4);
        assert_eq!(functions[0].metrics.maintainability, 71);
        let closure = &analysed.spaces[0].spaces[0];
        assert!(
            closure.metrics.cyclomatic.cyclomatic_sum() > 0.0
                && functions[0].metrics.cyclomatic
                    > analysed.spaces[0].metrics.cyclomatic.cyclomatic() as u32,
            "the parent's figure is more than its own space's"
        );
    }
    /// Issue 10, and the reason it was silent: a move is a name-modification on
    /// the platforms that report it at all, and a watcher that coalesced a burst
    /// reports whatever kind it settled on. Trusting the kind sent all of these
    /// to the catch-all modify arm, which read a path that was gone, failed and
    /// queued nothing — so the tree went on describing the folder as it was.
    /// Every spelling is driven here because the fix is one rule, not a case
    /// per platform: if the path is gone, it left.
    #[test]
    fn a_path_that_is_gone_is_a_removal_whatever_the_kind_says() {
        let root = tempfile::tempdir().expect("a temp directory");
        let gone = root.path().join("moved.rs");

        for kind in [
            notify::EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            notify::EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            notify::EventKind::Modify(ModifyKind::Any),
            notify::EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            notify::EventKind::Create(CreateKind::Any),
            notify::EventKind::Any,
            notify::EventKind::Other,
        ] {
            let mut queue = VecDeque::new();
            watched_path(
                &kind,
                gone.clone(),
                &State::default(),
                &root.path().join(".git"),
                &root.path().join(".varde/stories"),
                &mut queue,
            );

            assert_eq!(
                Vec::from(queue),
                [Event::FilesRemoved(vec![gone.clone()])],
                "{kind:?}"
            );
        }
    }

    /// The one read the change marks depend on that no scenario can make: that
    /// the commit's copy of an open buffer is found under the buffer's own path,
    /// which is the canonical one `main` opens on, and that a file the commit
    /// does not hold is answered `None` rather than left unanswered.
    #[test]
    fn the_commit_is_read_under_the_buffers_own_path() {
        let dir = tempfile::tempdir().expect("a temp directory");
        let root = std::fs::canonicalize(dir.path()).expect("a canonical root");
        let repository = git2::Repository::init(&root).expect("a repository");
        let who = git2::Signature::now("t", "t@example.com").expect("a signature");
        std::fs::write(root.join("a.rs"), "fn main() {}\n").expect("the file");
        let mut index = repository.index().expect("the index");
        index.add_path(Path::new("a.rs")).expect("staged");
        let tree = repository
            .find_tree(index.write_tree().expect("a tree"))
            .expect("the tree");
        repository
            .commit(Some("HEAD"), &who, &who, "init", &tree, &[])
            .expect("a commit");

        let buffers = [root.join("a.rs"), root.join("new.rs")];
        let found = committed(&root, buffers.iter());
        assert_eq!(
            found.get(&buffers[0]),
            Some(&Some("fn main() {}\n".to_string()))
        );
        assert_eq!(found.get(&buffers[1]), Some(&None));
    }

    /// F40's two reads no scenario can make, in one repository: that the
    /// Authorship comes back as the hand and the day the commit names, line by
    /// line and under the buffer's own path, and that a file the commit has no
    /// copy of comes back empty rather than unanswered.
    ///
    /// And the cache, which is the whole point of it: the entry is poisoned and
    /// asked for again at the same commit, so a second walk would show. That is
    /// what "editing the buffer does not recompute it" comes to — the key holds
    /// a commit and nothing about the Buffer — and a commit that moved drops the
    /// lot.
    #[test]
    fn the_authorship_is_read_per_commit_and_not_again_until_it_moves() {
        let dir = tempfile::tempdir().expect("a temp directory");
        let root = std::fs::canonicalize(dir.path()).expect("a canonical root");
        let repository = git2::Repository::init(&root).expect("a repository");
        // A fixed instant, so the date is the commit's and never today's:
        // 2026-01-05 in UTC, which is the offset this signature is at.
        let who = git2::Signature::new(
            "Ada Lovelace",
            "ada@example.com",
            &git2::Time::new(1_767_571_200, 0),
        )
        .expect("a signature");
        std::fs::write(root.join("a.rs"), "fn main() {}\nfn run() {}\n").expect("the file");
        let mut index = repository.index().expect("the index");
        index.add_path(Path::new("a.rs")).expect("staged");
        let tree = repository
            .find_tree(index.write_tree().expect("a tree"))
            .expect("the tree");
        repository
            .commit(Some("HEAD"), &who, &who, "init", &tree, &[])
            .expect("a commit");

        let buffers = [root.join("a.rs"), root.join("new.rs")];
        let ada = varde::authorship::Authored {
            author: "Ada Lovelace".to_string(),
            date: "2026-01-05".to_string(),
        };
        let (blamed, answers) = std::sync::mpsc::channel();
        let (mut cached, mut blaming) = (BTreeMap::new(), BTreeSet::new());
        // What `refresh_git` does with each answer.
        let land = |cached: &mut BTreeMap<_, _>, blaming: &mut BTreeSet<PathBuf>| {
            while !blaming.is_empty() {
                let (path, at, lines): (PathBuf, String, Vec<varde::authorship::Authored>) =
                    answers
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .expect("the walk's answer");
                blaming.remove(&path);
                cached.insert(path, (at, lines.into()));
            }
        };

        // Off the calling thread (#83): the poll that asks is answered later,
        // and one that comes before the answer does not ask again.
        for _ in 0..2 {
            let asked = authorship(
                Some(&root),
                buffers.iter(),
                Some("head"),
                &mut cached,
                &mut blaming,
                &blamed,
            );
            assert_eq!(asked, BTreeMap::new());
            assert_eq!(blaming, BTreeSet::from(buffers.clone()));
        }
        land(&mut cached, &mut blaming);
        let found = authorship(
            Some(&root),
            buffers.iter(),
            Some("head"),
            &mut cached,
            &mut blaming,
            &blamed,
        );
        assert_eq!(
            found.get(&buffers[0]).map(|lines| &lines[..]),
            Some(&[ada.clone(), ada.clone()][..])
        );
        assert_eq!(found.get(&buffers[1]).map(|lines| lines.len()), Some(0));

        let poison = varde::authorship::Authored {
            author: "nobody walked this".to_string(),
            date: String::new(),
        };
        cached.insert(
            buffers[0].clone(),
            ("head".to_string(), vec![poison.clone()].into()),
        );
        let again = authorship(
            Some(&root),
            buffers.iter(),
            Some("head"),
            &mut cached,
            &mut blaming,
            &blamed,
        );
        assert_eq!(
            again.get(&buffers[0]).map(|lines| &lines[..]),
            Some(&[poison][..])
        );
        assert_eq!(blaming, BTreeSet::new(), "walked twice");

        let moved = authorship(
            Some(&root),
            buffers.iter(),
            Some("another"),
            &mut cached,
            &mut blaming,
            &blamed,
        );
        assert_eq!(moved, BTreeMap::new());
        land(&mut cached, &mut blaming);
        let moved = authorship(
            Some(&root),
            buffers.iter(),
            Some("another"),
            &mut cached,
            &mut blaming,
            &blamed,
        );
        assert_eq!(
            moved.get(&buffers[0]).map(|lines| &lines[..]),
            Some(&[ada.clone(), ada][..])
        );

        // Nothing at all outside a repository, which the poll answers as no
        // working directory, and which is what makes the border silent there
        // rather than reporting an absence on every file.
        assert_eq!(
            authorship(
                None,
                buffers.iter(),
                Some("head"),
                &mut cached,
                &mut blaming,
                &blamed,
            ),
            BTreeMap::new()
        );
        assert_eq!(blaming, BTreeSet::new());
    }

    /// The date is the day the author was on when they wrote it, not the day it
    /// was in UTC — git's own `--date=short`. An hour before midnight UTC, an
    /// hour east, is already tomorrow where the hand was.
    #[test]
    fn the_authored_date_is_read_in_the_offset_its_author_was_at() {
        assert_eq!(short_date(git2::Time::new(1_767_567_600, 0)), "2026-01-04");
        assert_eq!(short_date(git2::Time::new(1_767_567_600, 60)), "2026-01-05");
    }

    /// The swap `:update` makes on a binary install, through the same `curl`
    /// it uses on a Release, pointed at `file://` stand-ins: a verified Asset
    /// lands executable at the binary's path, and a bad one leaves the old
    /// binary and no stray temporary file.
    #[test]
    fn a_verified_asset_replaces_the_binary_and_a_bad_one_does_not() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("a temp directory");
        let exe = dir.path().join("varde");
        fs::write(&exe, "old").expect("the running binary");
        let asset = dir.path().join("varde-linux-x86_64");
        fs::write(&asset, "foo").expect("the Asset");
        let sums = dir.path().join("SHA256SUMS");
        let url = |path: &Path| format!("file://{}", path.display());

        fs::write(&sums, "0000000000000000000000000000000000000000000000000000000000000000  varde-linux-x86_64\n")
            .expect("a wrong list");
        assert_eq!(
            super::replace_binary(&exe, &url(&asset), &url(&sums)),
            Err(super::ReplaceFailed::Checksum)
        );
        assert_eq!(fs::read_to_string(&exe).expect("the binary"), "old");

        fs::write(&sums, "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae  varde-linux-x86_64\n")
            .expect("the list");
        assert_eq!(
            super::replace_binary(&exe, &url(&asset), &url(&sums)),
            Ok(())
        );
        assert_eq!(fs::read_to_string(&exe).expect("the binary"), "foo");
        let mode = fs::metadata(&exe).expect("the binary").permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
        assert_eq!(fs::read_dir(dir.path()).expect("the directory").count(), 3);
    }

    /// The two edge reads the branch picker is made of, against a repository
    /// where they can be wrong: that a branch pushed but never checked out is
    /// listed at all, and that picking it moves `HEAD` onto a local branch of
    /// its own. No scenario covers either — only git can answer them — and the
    /// remote-tracking case is the half that has no local ref to move `HEAD`
    /// to.
    #[test]
    fn a_branch_pushed_but_never_checked_out_is_listed_and_checks_out() {
        let dir = tempfile::tempdir().expect("a temp directory");
        let repository = git2::Repository::init(dir.path()).expect("a repository");
        let who = git2::Signature::now("t", "t@example.com").expect("a signature");
        let empty = repository
            .find_tree(
                repository
                    .treebuilder(None)
                    .expect("a tree builder")
                    .write()
                    .expect("an empty tree"),
            )
            .expect("the empty tree");
        let first = repository
            .commit(None, &who, &who, "the first", &empty, &[])
            .expect("a commit");
        let commit = repository.find_commit(first).expect("the commit");
        repository
            .branch("main", &commit, true)
            .expect("the local branch");
        repository
            .set_head("refs/heads/main")
            .expect("a checked out branch");
        repository
            .reference("refs/remotes/origin/pushed", first, true, "pushed")
            .expect("the remote-tracking ref");

        let story::Branching::Listed(refs) = read_branches(dir.path()) else {
            panic!("a repository with a clean tree lists its branches");
        };
        // The dedup and the order are `story::branches`'; what the edge owes it
        // is every ref, on both sides.
        assert_eq!(story::branches(&refs, ""), ["main", "pushed"]);

        // The branch it left, which is the half no later read can answer.
        assert_eq!(checkout(dir.path(), "pushed"), Ok("main".to_string()));
        assert_eq!(
            repository
                .head()
                .expect("a head")
                .shorthand()
                .expect("a name"),
            "pushed"
        );
    }

    /// The correction itself, at the one site that can get it wrong. The
    /// *choice* of the three-dot form is [`story::revisions`]'s own unit test;
    /// that the edge then asks git for the merge-base rather than the base's
    /// tip needs a repository where the two differ, so this builds one — three
    /// commits, no working tree, no terminal.
    #[test]
    fn a_three_dot_range_ends_where_the_two_branches_parted() {
        let dir = tempfile::tempdir().expect("a temp directory");
        let repository = git2::Repository::init(dir.path()).expect("a repository");
        let who = git2::Signature::now("t", "t@example.com").expect("a signature");
        let empty = repository
            .find_tree(
                repository
                    .treebuilder(None)
                    .expect("a tree builder")
                    .write()
                    .expect("an empty tree"),
            )
            .expect("the empty tree");
        let commit = |message: &str, parent: Option<git2::Oid>| {
            let parents: Vec<git2::Commit> = parent
                .map(|oid| repository.find_commit(oid).expect("the parent"))
                .into_iter()
                .collect();
            repository
                .commit(
                    None,
                    &who,
                    &who,
                    message,
                    &empty,
                    &parents.iter().collect::<Vec<_>>(),
                )
                .expect("a commit")
        };
        let fork = commit("where they parted", None);
        let moved_on = commit("what landed on the base meanwhile", Some(fork));
        let introduced = commit("what the head introduced", Some(fork));
        repository
            .branch(
                "main",
                &repository.find_commit(moved_on).expect("the base tip"),
                true,
            )
            .expect("the base branch");
        repository
            .branch(
                "topic",
                &repository.find_commit(introduced).expect("the head"),
                true,
            )
            .expect("the head branch");

        assert_eq!(
            range_ends(&repository, "main...topic"),
            Some((fork, Some(introduced)))
        );
        // The two-dot form is still tip against tip, which is what an explicit
        // range the reviewer typed asked for.
        assert_eq!(
            range_ends(&repository, "main..topic"),
            Some((moved_on, Some(introduced)))
        );
    }

    /// The other half of the same rule, and what stops it from swallowing the
    /// events the editor and the diff follow: a path that is still there is not
    /// a disappearance, whatever else happened to it.
    #[test]
    fn a_path_that_still_exists_is_not_mistaken_for_a_removal() {
        let root = tempfile::tempdir().expect("a temp directory");
        let file = root.path().join("kept.rs");
        fs::write(&file, "fn main() {}\n").expect("the fixture file");
        let folder = root.path().join("kept");
        fs::create_dir(&folder).expect("the fixture folder");
        // A symlink whose target is gone is still an entry in the folder, and
        // the rule asks `symlink_metadata` rather than `exists` for exactly
        // this: `exists` follows the link, finds nothing, and deletes a row the
        // folder still has. It queues nothing here — the contents arm cannot
        // read it either, and reporting that as empty would blank a buffer.
        let dangling = root.path().join("dangling.rs");
        std::os::unix::fs::symlink(root.path().join("no-such-target"), &dangling)
            .expect("the fixture symlink");
        let mut queue = VecDeque::new();

        for (kind, path) in [
            (
                notify::EventKind::Modify(ModifyKind::Data(DataChange::Any)),
                file.clone(),
            ),
            (notify::EventKind::Create(CreateKind::Any), folder.clone()),
            (
                notify::EventKind::Modify(ModifyKind::Data(DataChange::Any)),
                dangling,
            ),
        ] {
            watched_path(
                &kind,
                path,
                &State::default(),
                &root.path().join(".git"),
                &root.path().join(".varde/stories"),
                &mut queue,
            );
        }

        assert_eq!(
            Vec::from(queue),
            [
                Event::FileChanged {
                    path: file,
                    contents: "fn main() {}\n".to_string(),
                },
                Event::FilesAppeared(vec![(folder, tree::Kind::Folder)]),
            ]
        );
    }

    /// The other half of a rename, and the half the removal rule cannot reach:
    /// the name a move lands on exists, so the tree is only told what left
    /// unless somebody tells it what arrived. Measured on macOS: `mv a b`
    /// reports `Modify(Name(Any))` on both paths, so the old name and the new
    /// one differ by nothing but whether the file is there. The contents go
    /// with it because renaming a temp file over the target is how `sed -i`,
    /// most editors and most AI CLIs save, and a buffer following the file has
    /// to hear that.
    #[test]
    fn a_name_a_move_landed_on_appears_in_the_tree() {
        let root = tempfile::tempdir().expect("a temp directory");
        let file = root.path().join("renamed.rs");
        fs::write(&file, "fn renamed() {}\n").expect("the fixture file");
        let folder = root.path().join("renamed");
        fs::create_dir(&folder).expect("the fixture folder");
        let mut queue = VecDeque::new();

        for path in [file.clone(), folder.clone()] {
            watched_path(
                &notify::EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
                path,
                &State::default(),
                &root.path().join(".git"),
                &root.path().join(".varde/stories"),
                &mut queue,
            );
        }

        assert_eq!(
            Vec::from(queue),
            [
                Event::FilesAppeared(vec![(file.clone(), tree::Kind::File)]),
                Event::FileChanged {
                    path: file,
                    contents: "fn renamed() {}\n".to_string(),
                },
                Event::FilesAppeared(vec![(folder, tree::Kind::Folder)]),
            ]
        );
    }

    /// The rule above stops at what is not folder contents. Git makes and
    /// unmakes a lock file on every command, and a story artifact is the AI's
    /// whole report (ADR 0006) rather than a file that appeared in a folder —
    /// so a coalesced kind naming one of them must queue nothing, or every
    /// `git status` would delete a row. Only the spellings the vanish rule
    /// reaches are its business: an explicit `Remove` is the arm below it, and
    /// that arm is unchanged.
    #[test]
    fn what_is_not_folder_contents_is_no_removal_either() {
        let root = tempfile::tempdir().expect("a temp directory");
        let git = root.path().join(".git");
        let stories = root.path().join(".varde/stories");

        for path in [git.join("index.lock"), stories.join("gone.json")] {
            let mut queue = VecDeque::new();
            watched_path(
                &notify::EventKind::Modify(ModifyKind::Any),
                path.clone(),
                &State::default(),
                &git,
                &stories,
                &mut queue,
            );

            assert!(queue.is_empty(), "{path:?} left {queue:?}");
        }
    }

    /// Every slug the library builds a notice from has words in this table.
    /// The scenarios assert the slug and never the copy — deliberately, since
    /// wording changes — so a refusal that reaches the status line as its own
    /// slug is a defect the whole suite is blind to: three shipped that way and
    /// were found by eye, and two more survived that same sweep. The slugs are
    /// read off the library's own source rather than a list kept beside the
    /// table, because a list is a second place to forget one, which is the
    /// failure being guarded. The two spellings a notice is built with are
    /// `Effect::Notify` with a literal and the `slug` field of `NotifyAbout`; a
    /// third way of naming one would need a line here, and reading the whole of
    /// `src` rather than a file list is what keeps a new module from being a
    /// blind spot of its own.
    #[test]
    fn every_notice_slug_the_library_emits_has_words() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut unworded: Vec<String> = Vec::new();
        for entry in fs::read_dir(&source).expect("the source directory") {
            let path = entry.expect("a source file").path();
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs") {
                continue;
            }
            let text = fs::read_to_string(&path).expect("readable source");
            let names = ["Notify(\"", "slug: \""];
            for slug in names
                .iter()
                .flat_map(|marker| text.split(marker).skip(1))
                .filter_map(|rest| rest.split('"').next())
            {
                if notice_text(slug).0 == slug {
                    unworded.push(format!("{slug} in {:?}", path.file_name()));
                }
            }
        }
        assert!(unworded.is_empty(), "notices with no words: {unworded:?}");
    }
}
