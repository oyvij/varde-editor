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
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
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

const HINT: &str = " Ctrl+Space or Esc Esc commands (e/r view · f find · a AI · t terminal · w write · s submit · q quit) · ^F search · / filter · gt/gT buffers · :q close · :qa quit · Alt+hjkl focus · Enter open · → row actions · n/N/d shortcuts";

#[derive(ClapParser)]
#[command(
    name = "varde",
    about = "A terminal IDE that reviews, tests and ships your work"
)]
struct Args {
    folder: Option<PathBuf>,
    #[arg(long, conflicts_with = "folder")]
    deps: bool,
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
    let folder = args.folder.as_deref().unwrap_or(Path::new("."));
    let root = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let sidecar = args.folder.is_none().then(|| sidecar(&root));
    sweep();

    // Canonicalize: macOS current_exe may not resolve symlinks; resolved once since :update replaces it
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

fn sidecar(root: &Path) -> PathBuf {
    sidecars().join(format!("{}-{}", flatten(root), std::process::id()))
}

fn sidecars() -> PathBuf {
    home().join(varde::VARDE_DIR).join("paths")
}

/// pid 0 is rejected: kill(0, …) asks about our own process group, so it would always read alive
fn pid_of(name: &str) -> Option<u32> {
    name.rsplit_once('-')?
        .1
        .parse()
        .ok()
        .filter(|pid| *pid != 0)
}

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

/// EPERM means the process exists but belongs to someone else, so it counts as alive
fn alive(pid: u32) -> bool {
    let answer = unsafe { libc::kill(pid as libc::pid_t, 0) };
    answer == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

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
        StartupError::Config(problem) => problem.to_string(),
    }
}

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

fn config_layer(path: &Path) -> Option<String> {
    read(path).or_else(|| path.try_exists().unwrap_or(false).then(String::new))
}

fn config_text(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
        Err(error) => {
            eprintln!("varde: cannot read {}: {error}", path.display());
            None
        }
    }
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

fn diff_entries(
    repository: &git2::Repository,
    base_tree: Option<&git2::Tree>,
    range_head_tree: Option<&git2::Tree>,
    root: &Path,
    entries: &mut BTreeMap<String, story::FileHunks>,
) {
    let mut options = git2::DiffOptions::new();
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
    fn shell(&mut self, split: usize) -> &mut pty::Pane {
        let last = self.shells.len() - 1;
        &mut self.shells[split.min(last)]
    }
}

struct Status {
    text: String,
    tone: ui::Tone,
}

#[derive(Clone)]
enum Again {
    Dial(u16),
    Spawn { command: String, args: Vec<String> },
}

struct Edge {
    shells: Vec<pty::Pane>,
    ai: Option<pty::Pane>,
    output: Option<pty::Pane>,
    root: PathBuf,
    sidecar: Option<PathBuf>,
    status: Status,
    drafts: Drafts,
    clipboard: Option<arboard::Clipboard>,
    area: ratatui::layout::Rect,
    pointer: mouse::Pointer,
    cursor_style: &'static str,
    highlighted: (PathBuf, u64, Vec<Vec<varde::highlight::Token>>),
    highlit: Sender<Parsed>,
    parsing: Option<std::thread::JoinHandle<()>>,
    parse_asked: (PathBuf, u64),
    run_marks: Vec<usize>,
    trace_committed: Option<String>,
    diff_sides: (
        Vec<Vec<varde::highlight::Token>>,
        Vec<Vec<varde::highlight::Token>>,
    ),
    previewed: (PathBuf, u64, usize, Vec<varde::preview::Row>),
    code: (String, ui::Code),
    faint: ratatui::style::Style,
    pending_search: Option<(varde::search::Request, Instant)>,
    searching: Option<(u64, Receiver<Vec<varde::search::Hit>>, Arc<()>)>,
    indexing: Option<(u64, Receiver<String>)>,
    candidates_due: Option<Instant>,
    hover_due: Option<Instant>,
    analysed: Sender<(u64, Figures, Option<Figures>)>,
    tested: Sender<(bool, String)>,
    formatted: Sender<(String, PathBuf, u64, format::Answer)>,
    released: Sender<Option<String>>,
    replaced: Sender<Result<(), ReplaceFailed>>,
    exe: Option<PathBuf>,
    relaunch: bool,
    servers: BTreeMap<String, rpc::Server>,
    adapter: Option<rpc::Adapter>,
    connecting: Option<Receiver<std::io::Result<rpc::Adapter>>>,
    again: Option<Again>,
    children: BTreeMap<usize, rpc::Adapter>,
    joining: BTreeMap<usize, Receiver<std::io::Result<rpc::Adapter>>>,
    probe: Option<Receiver<bool>>,
    probed: Instant,
    on_path: Option<BTreeSet<String>>,
    git: Option<bool>,
    facts: Option<(BTreeSet<PathBuf>, BTreeMap<String, String>)>,
    varde_home: PathBuf,
    speech: reading::Speech,
    voice: Option<Voice>,
    playing: Option<std::process::Child>,
    stream: Option<Stream>,
    player: Option<bool>,
    authored: BTreeMap<PathBuf, (String, Arc<[authorship::Authored]>)>,
    blaming: BTreeSet<PathBuf>,
    blamed: Sender<(PathBuf, String, Vec<authorship::Authored>)>,
    polling: Option<std::thread::JoinHandle<()>>,
    polled: Sender<Polled>,
    asked: BTreeSet<PathBuf>,
}

struct Polled {
    repo: Option<Vec<GitFile>>,
    conflicts: BTreeMap<PathBuf, Vec<varde::conflict::Conflict>>,
    file_hunks: Arc<[story::FileHunks]>,
    ignored: BTreeSet<PathBuf>,
    branch: Option<String>,
    committed: BTreeMap<PathBuf, Option<String>>,
    head: Option<String>,
    workdir: Option<PathBuf>,
}

struct Stream {
    whole: PathBuf,
    tail: PathBuf,
    offsets: Vec<u32>,
    from_ms: u32,
    since: Instant,
}

struct Voice {
    child: std::process::Child,
    answers: std::io::BufReader<std::io::PipeReader>,
    speed: f32,
}

impl Drop for Voice {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

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
            None,
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
        code: (String::new(), ui::Code::new()),
        faint: ui::faint(palette),
        pending_search: None,
        searching: None,
        indexing: None,
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

    state.system_clipboard = edge.clipboard.is_some();
    state.branch = head_branch(&root);
    state.file_hunks = file_hunks(
        state.repo_root(),
        story::inventory(&state.story_set),
        &story::named_files(&state.story_set),
    )
    .into();
    let mut queue: VecDeque<Event> = VecDeque::new();
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
        dirty |= queue_search(&state, &root, &mut edge, &mut queue);
        dirty |= queue_index(&state, &mut edge, &mut queue);
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
        cache_code(&state, &mut edge);
        render(&mut terminal, &state, &mut edge)?;
    };

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
    drop(edge);
    use std::os::unix::process::CommandExt;
    let error = std::process::Command::new(&exe)
        .args(std::env::args_os().skip(1))
        .exec();
    Err(anyhow::anyhow!("relaunching {}: {error}", exe.display()))
}

fn queue_tick(state: &State, last_tick: &mut Instant, queue: &mut VecDeque<Event>) {
    let spinning =
        state.risk.in_flight() || state.index.walking || varde::search::running(state).is_some();
    if spinning && last_tick.elapsed() >= SPIN {
        *last_tick = Instant::now();
        queue.push_back(Event::Tick);
    }
}

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

fn queue_search(state: &State, root: &Path, edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    let wanted = varde::search::running(state);
    if edge
        .pending_search
        .as_ref()
        .is_some_and(|(request, _)| Some(request.generation) != wanted)
    {
        edge.pending_search = None;
    }
    if edge
        .searching
        .as_ref()
        .is_some_and(|(generation, ..)| Some(*generation) != wanted)
    {
        edge.searching = None;
    }
    if let Some((request, _)) = edge.pending_search.take_if(|(_, at)| Instant::now() >= *at) {
        let (hits, found) = channel();
        let root = root.to_path_buf();
        let wanted = Arc::new(());
        let still = Arc::downgrade(&wanted);
        edge.searching = Some((request.generation, found, wanted));
        std::thread::spawn(move || search_project(&root, &request, &hits, &still));
    }
    let Some((generation, found, _)) = edge.searching.as_ref() else {
        return false;
    };
    let generation = *generation;
    let (batches, done) = drained(found);
    if done {
        edge.searching = None;
    }
    if batches.is_empty() && !done {
        return false;
    }
    queue.push_back(Event::Searched {
        generation,
        hits: batches.into_iter().flatten().collect(),
        done,
    });
    true
}

fn queue_index(state: &State, edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    let Some((walk, files)) = edge.indexing.as_ref() else {
        return false;
    };
    let walk = *walk;
    if !state.index.walking || state.index.walk != walk {
        edge.indexing = None;
        return false;
    }
    let (files, done) = drained(files);
    if done {
        edge.indexing = None;
    }
    if files.is_empty() && !done {
        return false;
    }
    queue.push_back(Event::Indexed { walk, files, done });
    true
}

fn drained<T>(receiver: &Receiver<T>) -> (Vec<T>, bool) {
    let mut batch = Vec::new();
    loop {
        match receiver.try_recv() {
            Ok(item) => batch.push(item),
            Err(TryRecvError::Empty) => return (batch, false),
            Err(TryRecvError::Disconnected) => return (batch, true),
        }
    }
}

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

fn enter_terminal(title: &str) -> Result<(std::io::Stdout, bool)> {
    use std::io::Write;
    terminal::enable_raw_mode()?;
    let mut out = std::io::stdout();
    execute!(
        out,
        terminal::EnterAlternateScreen,
        terminal::SetTitle(title),
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    // Any-event tracking (1003): EnableMouseCapture only asks for 1002 and crossterm has no command for it
    write!(out, "\x1b[?1003h")?;
    out.flush()?;
    // Never REPORT_ALL_KEYS_AS_ESCAPE_CODES: crossterm then drops composed text (AltGr, Option, dead keys)
    let enhanced = terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(
            out,
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
    // DisableMouseCapture does not undo 1003, so it is reset by hand
    let _ = write!(out, "\x1b[?1003l");
    let _ = execute!(
        out,
        DisableBracketedPaste,
        DisableMouseCapture,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}

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

/// Resize before draining output: vt100 can panic if the grid shrinks under the cursor
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

fn drain_panes(state: &State, edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    let mut dirty = false;
    let mut spoke = Vec::new();
    for shell in &mut edge.shells {
        let was_silent = !shell.spoken;
        dirty |= shell.drain();
        spoke.push(was_silent && shell.spoken);
    }
    if edge.shells.iter().any(|shell| shell.alive) {
        let mut alive = edge.shells.iter().map(|shell| shell.alive);
        spoke.retain(|_| alive.next().unwrap_or(true));
        edge.shells.retain(|shell| shell.alive);
    }
    for (split, _) in spoke.iter().enumerate().filter(|(_, spoke)| **spoke) {
        queue.push_back(Event::ShellSpoke(split));
    }
    if let Some(output) = edge.output.as_mut() {
        if output.drain() {
            dirty = true;
            queue.push_back(Event::OutputSpoke);
        }
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
    if was_silent && ai.spoken {
        queue.push_back(Event::AiSpoke);
    }
    if !ai.alive {
        edge.ai = None;
        edge.drafts.ai = state.ai_command.clone();
        queue.push_back(Event::AiExited);
    }
    dirty
}

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

fn adapter_is_gone(edge: &mut Edge, queue: &mut VecDeque<Event>, why: varde::debug::Gone) {
    edge.adapter = None;
    edge.connecting = None;
    edge.again = None;
    edge.children.clear();
    edge.joining.clear();
    edge.output = None;
    queue.push_back(Event::DapGone { why, from: 0 });
}

fn refresh_git(
    root: &Path,
    state: &mut State,
    edge: &mut Edge,
    blamed: &Receiver<(PathBuf, String, Vec<authorship::Authored>)>,
    polled: &Receiver<Polled>,
    last_git: &mut Instant,
    last_shape: &mut (usize, usize),
) -> bool {
    let mut due = false;
    while let Ok((path, at, lines)) = blamed.try_recv() {
        edge.blaming.remove(&path);
        edge.authored.insert(path, (at, lines.into()));
        due = true;
    }
    let mut dirty = false;
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
        state.head = answer.head;
        state.branch = answer.branch;
    } else if ended {
        edge.polling = None;
    }
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

fn authored_lines(repository: &git2::Repository, relative: &Path) -> Vec<authorship::Authored> {
    let Ok(blamed) = repository.blame_file(relative, None) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for hunk in blamed.iter() {
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

fn short_date(when: git2::Time) -> String {
    let stamp = chrono::DateTime::from_timestamp(when.seconds(), 0);
    let zone = chrono::FixedOffset::east_opt(when.offset_minutes() * 60);
    match (stamp, zone) {
        (Some(stamp), Some(zone)) => stamp.with_timezone(&zone).format("%Y-%m-%d").to_string(),
        _ => String::new(),
    }
}

fn set_cursor_style(state: &State, edge: &mut Edge) {
    let wanted = match state
        .current_buffer
        .as_ref()
        .and_then(|p| state.buffers.get(p))
    {
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

type Parsed = (PathBuf, Vec<Vec<varde::highlight::Token>>, Vec<usize>);

fn cache_highlight(state: &State, edge: &mut Edge, highlit: &Receiver<Parsed>) -> bool {
    let current = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path).map(|buffer| (path, buffer)));
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

fn cache_code(state: &State, edge: &mut Edge) {
    let language = varde::debug::paused_in(state);
    if edge.code.0 != language {
        edge.code = (language, ui::Code::new());
    }
    let mut kept = ui::Code::new();
    for text in varde::debug::code(state) {
        if let std::collections::hash_map::Entry::Vacant(entry) = kept.entry(text) {
            let tokens = edge
                .code
                .1
                .remove(entry.key())
                .unwrap_or_else(|| varde::highlight::highlight(&edge.code.0, entry.key()));
            entry.insert(tokens);
        }
    }
    edge.code.1 = kept;
}

fn render(terminal: &mut Screen, state: &State, edge: &mut Edge) -> Result<()> {
    let rows = tree::visible_rows(state);
    terminal.draw(|frame| {
        edge.area = frame.area();
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
                command: edge.drafts.command.as_ref(),
                ai_draft: &edge.drafts.ai,
                comment_kind: &edge.drafts.comment_kind,
                filter_draft: edge.drafts.filter.as_deref(),
                tokens: &edge.highlighted.2,
                run_marks: &edge.run_marks,
                diff_new: &edge.diff_sides.0,
                diff_old: &edge.diff_sides.1,
                preview: &edge.previewed.3,
                code: &edge.code.1,
                faint: edge.faint,
            },
        );
    })?;
    Ok(())
}

const STATE_FILE: &str = "state.json";

const INPUT_BATCH: usize = 512;

const SPIN: Duration = Duration::from_millis(80);

const PROBE: Duration = Duration::from_secs(1);

const SPREAD: usize = 3;

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

fn translate_paste(
    state: &mut State,
    edge: &mut Edge,
    text: String,
    started: Instant,
    queue: &mut VecDeque<Event>,
) {
    match keys::on_paste(state, &edge.drafts, text) {
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
        let mut budget = INPUT_BATCH;
        loop {
            translate_input(state, edge, event::read()?, started, queue);
            worked |= drain(state, edge, queue);
            budget -= 1;
            if budget == 0 || !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }

    worked |= drain(state, edge, queue);
    Ok(worked || read_input)
}

fn drain(state: &mut State, edge: &mut Edge, queue: &mut VecDeque<Event>) -> bool {
    let mut worked = false;
    while let Some(next_event) = queue.pop_front() {
        worked = true;
        let (next, effects) = update(state, next_event);
        *state = next;
        if edge.speech != state.speech {
            edge.speech = state.speech.clone();
        }
        for effect in effects {
            perform(effect, state.split(), edge, queue);
        }
        tell_core(state, edge);
    }
    worked
}

fn tell_core(state: &mut State, edge: &mut Edge) {
    probe_path(state, edge);
    state.commands_on_path = edge.on_path.clone().unwrap_or_default();
    let from: BTreeSet<PathBuf> = state
        .buffers
        .keys()
        .filter_map(|path| path.parent())
        .filter(|dir| dir.starts_with(&edge.root))
        .map(Path::to_path_buf)
        .collect();
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
    state.git_installed = *edge.git.get_or_insert_with(|| which::which("git").is_ok());
    state.lsp_running = edge.servers.keys().cloned().collect();
    state.voice_running = edge.voice.is_some();
    state.player_installed = *edge
        .player
        .get_or_insert_with(|| which::which(&edge.speech.player).is_ok());
    state.voice_installed =
        reading::voice_file(&state.speech.voice, &home()).is_some_and(|file| file.is_file());
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

fn start_voice(state: &State, edge: &mut Edge) {
    if edge.voice.is_some() || !state.voice_installed {
        return;
    }
    if !state.buffers.keys().any(|path| preview::is_markdown(path)) {
        return;
    }
    edge.voice = spawn_voice(edge, edge.speech.speed);
}

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
    // stdout and stderr share one pipe: some synthesizers print their answer path on stderr
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
            edge.status = Status {
                text: format!("could not start {}: {error}", edge.speech.command),
                tone: ui::Tone::Warning,
            };
            None
        }
    }
}

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

fn queue_position(edge: &Edge, last: &mut Instant, queue: &mut VecDeque<Event>) {
    if edge.playing.is_none() || last.elapsed() < SPIN {
        return;
    }
    *last = Instant::now();
    tell_position(edge, queue);
}

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

fn pause(edge: &mut Edge, queue: &mut VecDeque<Event>) {
    tell_position(edge, queue);
    stop_player(edge);
}

fn ask_voice(voice: &mut Voice, text: &str) -> Option<PathBuf> {
    use std::io::{BufRead, Write};
    let stdin = voice.child.stdin.as_mut()?;
    writeln!(stdin, "{}", text.replace(['\n', '\r'], " ")).ok()?;
    stdin.flush().ok()?;
    let mut answer = String::new();
    loop {
        answer.clear();
        if voice.answers.read_line(&mut answer).ok()? == 0 {
            return None;
        }
        if let Some(wav) = reading::wrote(&answer) {
            return Some(PathBuf::from(wav));
        }
    }
}

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
            // hound's duration counts frames per channel, not samples
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

fn samples(spec: hound::WavSpec, ms: u32) -> u64 {
    u64::from(spec.sample_rate) * u64::from(ms) / 1000 * u64::from(spec.channels)
}

fn stop_player(edge: &mut Edge) {
    let Some(mut child) = edge.playing.take() else {
        return;
    };
    let _ = child.kill();
    let _ = child.wait();
}

fn hush(edge: &mut Edge) {
    stop_player(edge);
    let Some(stream) = edge.stream.take() else {
        return;
    };
    let _ = std::fs::remove_file(&stream.whole);
    let _ = std::fs::remove_file(&stream.tail);
}

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
    let binary = std::fs::canonicalize(which::which(fact.command.as_ref()?).ok()?).ok()?;
    let beside =
        std::fs::canonicalize(binary.parent()?.join(fact.command_marker.as_ref()?)).ok()?;
    handed_over(fact, &beside)
}

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

fn handed_over(fact: &Fact, marker: &Path) -> Option<String> {
    let answer = match fact.value {
        FactValue::Directory => marker.parent()?,
        FactValue::Marker => marker,
    };
    Some(answer.to_string_lossy().to_string())
}

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
            modifiers: to_terminput_mouse(mouse).modifiers,
        },
        queue,
    );
}

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

fn grid_lines(edge: &Edge, pane: Pane, split: usize, upto: usize) -> Option<Vec<String>> {
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

fn perform_terminal(effect: Effect, split: usize, edge: &mut Edge) -> Option<Effect> {
    match effect {
        Effect::SetTerminalInput(text) => {
            edge.shell(split).send(b"\x15");
            edge.shell(split).send(text.as_bytes());
        }
        Effect::RunInTerminal(text) => {
            edge.shell(split).send(b"\x15");
            edge.shell(split).send(text.as_bytes());
            edge.shell(split).send(b"\r");
        }
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
        Effect::OpenUrl(url) => {
            let opener = match cfg!(target_os = "macos") {
                true => "open",
                false => "xdg-open",
            };
            match std::process::Command::new(opener).arg(&url).spawn() {
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
        Effect::ClipboardViaTerminal(text) => {
            let _ = execute!(
                std::io::stdout(),
                crossterm::clipboard::CopyToClipboard::to_clipboard_from(text)
            );
        }
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

fn perform_session(effect: Effect, edge: &mut Edge, queue: &mut VecDeque<Event>) -> Option<Effect> {
    match effect {
        Effect::RenderView(_) => {}
        Effect::StopAi => {
            edge.ai = None;
            queue.push_back(Event::AiExited);
        }
        Effect::SplitTerminal { from } => {
            let from = from.min(edge.shells.len() - 1);
            let cwd = edge.shells[from].cwd().unwrap_or_else(|| edge.root.clone());
            let (rows, cols) = edge.shells[from].screen().size();
            match pty::Pane::spawn(&[], &cwd, &BTreeMap::new(), None, rows, cols) {
                Ok(pane) => edge.shells.insert(from + 1, pane),
                Err(error) => {
                    edge.status = Status {
                        text: format!("could not split the terminal: {error}"),
                        tone: ui::Tone::Warning,
                    };
                }
            }
        }
        Effect::SpawnAi { command, env } => {
            let size = edge.shells[0].screen().size();
            let argv = shlex::split(&command).filter(|argv| !argv.is_empty());
            let started = match &argv {
                Some(argv) => pty::Pane::spawn(
                    argv,
                    &edge.root,
                    &BTreeMap::new(),
                    Some(&env),
                    size.0,
                    size.1,
                )
                .map_err(|error| format!("could not start {command}: {error}")),
                None => Err(format!("{command:?} is not a command")),
            };
            match started {
                Ok(pane) => {
                    edge.ai = Some(pane);
                }
                Err(text) => {
                    edge.status = Status {
                        text,
                        tone: ui::Tone::Warning,
                    };
                    edge.drafts.ai = command;
                    queue.push_back(Event::AiExited);
                }
            }
        }
        Effect::RunProgram { argv, cwd, env } => {
            let cwd = cwd.unwrap_or_else(|| edge.root.clone());
            let size = edge.shells[0].screen().size();
            match pty::Pane::spawn(&argv, &cwd, &env, None, size.0, size.1) {
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
                    queue.push_back(Event::LspStarted { language });
                }
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
            edge.children.clear();
            edge.joining.clear();
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
        Effect::ProbePath => {
            edge.on_path = None;
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
            None => queue.push_back(Event::LspGone {
                language,
                why: varde::lsp::Gone::Exited,
            }),
        },
        Effect::DebounceCandidates(after_ms) => {
            edge.candidates_due = Some(Instant::now() + Duration::from_millis(after_ms));
        }
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
        Effect::OpenAt { path, at } => match std::fs::read_to_string(&path) {
            Ok(contents) => queue.push_back(Event::BufferOpened {
                contents,
                path,
                preview: false,
                at: Some(at),
            }),
            Err(error) => {
                edge.status = Status {
                    text: format!("could not open {}: {error}", path.display()),
                    tone: ui::Tone::Warning,
                };
            }
        },
        Effect::ReadForReview(path) => {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                queue.push_back(Event::ReviewFileRead { path, contents });
            }
        }
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

fn perform_jobs(effect: Effect, edge: &mut Edge, queue: &mut VecDeque<Event>) {
    match effect {
        Effect::RunSearch(request) => {
            edge.searching = None;
            edge.pending_search = Some((request, Instant::now() + Duration::from_millis(150)));
        }
        Effect::IndexProject { walk } => {
            let (files, walked) = channel();
            let root = edge.root.clone();
            edge.indexing = Some((walk, walked));
            std::thread::spawn(move || {
                for name in walking(&root) {
                    if files.send(name).is_err() {
                        return;
                    }
                }
            });
        }
        Effect::ReadStories { dir, repo } => {
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
            let answer = edge.analysed.clone();
            let root = edge.root.clone();
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
        Effect::ReadBranches => queue.push_back(Event::Branches(read_branches(&edge.root))),
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
                Err(error) => {
                    eprintln!("varde: cannot read {}: {error}", sentinel.display());
                    story::Branching::DownloadFailed { how, status: None }
                }
            };
            queue.push_back(Event::Branches(branching));
        }
        Effect::ReadLaunchTarget { path } => {
            queue.push_back(Event::LaunchTargetRead(config_text(&path)));
        }
        Effect::ReadGlobalConfig {
            path,
            kind,
            name,
            write,
        } => {
            let text = config_text(&path);
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

fn replace_binary(exe: &Path, asset: &str, checksums: &str) -> Result<(), ReplaceFailed> {
    let list = download(checksums).ok_or(ReplaceFailed::Download)?;
    let binary = download(asset).ok_or(ReplaceFailed::Download)?;
    startup::verify(&String::from_utf8_lossy(&list), asset, &binary).inspect_err(|failed| {
        eprintln!("varde: update failed: {failed:?} for {asset}");
    })?;
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

fn answer_tests(command: &str, root: &Path, answer: &Sender<(bool, String)>) {
    let words = shlex::split(command).unwrap_or_default();
    let Some((program, arguments)) = words.split_first() else {
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
    // Write stdin from its own thread: a formatter that fills its stdout pipe stops reading stdin
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

fn snapshot_dir(root: &Path, sidecar: Option<&Path>, iteration: u32) -> PathBuf {
    varde_dir(root, sidecar).join(format!("snapshots/{iteration}"))
}

fn snapshot(root: &Path, sidecar: Option<&Path>, iteration: u32) {
    let dir = snapshot_dir(root, sidecar, iteration);
    let _ = std::fs::remove_dir_all(&dir);
    for name in walk(root) {
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
    let Some(out) = story_artifact_path(dir, &repository, &spelling) else {
        return refused();
    };
    let authored = Path::new(&out).exists();
    story::decide(force, authored, spelling, out)
}

fn read_branches(root: &Path) -> story::Branching {
    let Ok(repository) = git2::Repository::open(root) else {
        return story::Branching::NotARepository;
    };
    let Some(files) = git_status(root) else {
        return story::Branching::NotARepository;
    };
    if story::uncommitted(&files) {
        return story::Branching::Dirty;
    }
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

fn story_range_status(root: &Path, file: &Path) -> story::RangeStatus {
    match git2::Repository::open(root) {
        Ok(repository) => story::range_status(file, |revision| {
            repository.revparse_single(revision).is_ok()
        }),
        Err(_) => story::range_status(file, |_| false),
    }
}

fn walk(root: &Path) -> Vec<String> {
    walking(root).collect()
}

fn project(base: &Path) -> ignore::WalkBuilder {
    let mut walker = ignore::WalkBuilder::new(base);
    walker
        .hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git");
    walker
}

fn walking(root: &Path) -> impl Iterator<Item = String> + '_ {
    project(root)
        .build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(move |entry| {
            entry
                .path()
                .strip_prefix(root)
                .ok()
                .map(|rest| rest.to_string_lossy().into_owned())
        })
}

fn head_commit(root: &Path) -> Option<String> {
    let repository = git2::Repository::discover(root).ok()?;
    let head = repository.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    Some(commit.id().to_string())
}

fn head_branch(root: &Path) -> Option<String> {
    let repository = git2::Repository::discover(root).ok()?;
    let head = repository.head().ok()?;
    head.is_branch()
        .then(|| head.shorthand().ok().map(str::to_string))
        .flatten()
}

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

fn measured_at(root: &Path, files: &[String], base: &str) -> Figures {
    let Ok(repository) = git2::Repository::open(root) else {
        return Figures::default();
    };
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

fn language(path: &Path) -> Option<rust_code_analysis::LANG> {
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    rust_code_analysis::get_from_ext(&extension)
}

fn spaces(path: &Path, source: Vec<u8>) -> Option<Space> {
    let language = language(path)?;
    rust_code_analysis::get_function_spaces(&language, source, path, None)
        .map(|analysed| space(&analysed))
}

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
            maintainability: analysed.metrics.mi.mi_visual_studio().max(0.0) as u32,
            lines: analysed.metrics.loc.sloc() as u32,
        },
        children: analysed.spaces.iter().map(space).collect(),
    }
}

fn search_project(
    root: &Path,
    request: &varde::search::Request,
    hits: &Sender<Vec<varde::search::Hit>>,
    wanted: &std::sync::Weak<()>,
) {
    use grep_searcher::{BinaryDetection, SearcherBuilder};
    if hits
        .send(varde::search::scan(&request.query, &request.buffers))
        .is_err()
    {
        return;
    }
    let exact = varde::search::Case::Smart.exact(&request.query);
    let matcher = match grep_regex::RegexMatcherBuilder::new()
        .fixed_strings(true)
        .case_insensitive(!exact)
        .build(&request.query)
    {
        Ok(matcher) => matcher,
        Err(error) => {
            eprintln!("varde: cannot search for {:?}: {error}", request.query);
            return;
        }
    };
    let searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(0))
        .line_number(true)
        .build();
    let search_file = |searcher: &mut grep_searcher::Searcher, path: &Path| -> bool {
        if wanted.strong_count() == 0 {
            return false;
        }
        let Some(name) = path
            .strip_prefix(root)
            .ok()
            .map(|rest| rest.to_string_lossy())
        else {
            return true;
        };
        if !request.covers(&name) {
            return true;
        }
        let mut found = Vec::new();
        let searched = searcher.search_path(
            &matcher,
            path,
            grep_searcher::sinks::Lossy(|number, line| {
                found.extend(varde::search::hit(&request.query, &name, number, line));
                Ok(found.len() <= varde::search::CAP)
            }),
        );
        if let Err(error) = searched {
            eprintln!("varde: cannot read {}: {error}", path.display());
        }
        found.is_empty() || hits.send(found).is_ok()
    };
    if let Some(only) = &request.only {
        let mut searcher = searcher;
        for name in only {
            if !search_file(&mut searcher, &root.join(name)) {
                return;
            }
        }
        return;
    }
    let base = match &request.under {
        Some(under) => root.join(under),
        None => root.to_path_buf(),
    };
    project(&base).build_parallel().run(|| {
        let mut searcher = searcher.clone();
        let search_file = &search_file;
        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return ignore::WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                return ignore::WalkState::Continue;
            }
            match search_file(&mut searcher, entry.path()) {
                true => ignore::WalkState::Continue,
                false => ignore::WalkState::Quit,
            }
        })
    });
}

#[derive(Default)]
struct Diff {
    lines: Vec<varde::DiffLine>,
    revision: String,
    new_side: Vec<Vec<varde::highlight::Token>>,
    old_side: Vec<Vec<varde::highlight::Token>>,
}

fn diff_lines(root: &Path, path: &Path) -> Diff {
    let Ok(repository) = git2::Repository::open(root) else {
        return Diff::default();
    };
    let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    let mut options = git2::DiffOptions::new();
    options
        .pathspec(&relative)
        .context_lines(story::CONTEXT_LINES)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
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
        "hot-replaced",
        "The running program has the new code",
        ui::Tone::Notice,
    ),
    (
        "hot-replace-failed",
        "The running program still has the old code — Space r restarts it",
        ui::Tone::Warning,
    ),
    (
        "no-hot-replace",
        "This Debug adapter's row names no hot replace request",
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
        "cannot-inject-lines",
        "This CLI cannot be told a paste from typing — inject one line at a time",
        ui::Tone::Warning,
    ),
    (
        "nothing-to-inject",
        "Nothing to inject — select some text or put the cursor on a line",
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
    ("no-such-motion", "No such motion", ui::Tone::Warning),
    (
        "nothing-to-update",
        "Nothing to update from — no checkout of Varde, and no newer Release found",
        ui::Tone::Warning,
    ),
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

fn notice_text(notice: &str) -> (&str, ui::Tone) {
    NOTICES
        .iter()
        .find(|(slug, _, _)| *slug == notice)
        .map_or((notice, ui::Tone::Warning), |(_, text, tone)| {
            (*text, *tone)
        })
}

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
            // Not Access: inotify reports opens, and reloading the config opens it, looping forever
            edited |= !matches!(kind, notify::EventKind::Access(_))
                && (path == global || path == project);
            if path.parent() == Some(state.varde_home.as_path()) && !path.starts_with(&state.root) {
                continue;
            }
            watched_path(&kind, path, state, &git_dir, &stories_dir, queue);
        }
    }
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

fn watched_path(
    kind: &notify::EventKind,
    path: PathBuf,
    state: &State,
    git_dir: &Path,
    stories_dir: &Path,
    queue: &mut VecDeque<Event>,
) {
    let ours = !path.starts_with(git_dir) && !path.starts_with(stories_dir);
    match kind {
        notify::EventKind::Create(_) | notify::EventKind::Modify(_)
            if path.starts_with(stories_dir)
                && path.extension().is_some_and(|ext| ext == "json") =>
        {
            queue_story_file(&path, state, queue);
        }
        // symlink_metadata, not exists: a dangling symlink is still an entry in the folder
        _ if ours && std::fs::symlink_metadata(&path).is_err() => {
            queue.push_back(Event::FilesRemoved(vec![path]));
        }
        // macOS reports `mv a b` as Modify(Name(Any)) on both paths
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
        notify::EventKind::Modify(_) if ours => {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                queue.push_back(Event::FileChanged { path, contents });
            }
        }
        _ => {}
    }
}

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
        assert_eq!(offsets, [0, 2000]);
        for (piece, _) in &said {
            assert!(!piece.exists(), "the pieces are gone: {piece:?}");
        }
    }

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
        assert_eq!(functions[0].metrics.cyclomatic, 4);
        assert_eq!(functions[0].metrics.cognitive, 5);
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

    #[test]
    fn the_authorship_is_read_per_commit_and_not_again_until_it_moves() {
        let dir = tempfile::tempdir().expect("a temp directory");
        let root = std::fs::canonicalize(dir.path()).expect("a canonical root");
        let repository = git2::Repository::init(&root).expect("a repository");
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

    #[test]
    fn the_authored_date_is_read_in_the_offset_its_author_was_at() {
        assert_eq!(short_date(git2::Time::new(1_767_567_600, 0)), "2026-01-04");
        assert_eq!(short_date(git2::Time::new(1_767_567_600, 60)), "2026-01-05");
    }

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
        assert_eq!(story::branches(&refs, ""), ["main", "pushed"]);

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
        assert_eq!(
            range_ends(&repository, "main..topic"),
            Some((moved_on, Some(introduced)))
        );
    }

    #[test]
    fn a_path_that_still_exists_is_not_mistaken_for_a_removal() {
        let root = tempfile::tempdir().expect("a temp directory");
        let file = root.path().join("kept.rs");
        fs::write(&file, "fn main() {}\n").expect("the fixture file");
        let folder = root.path().join("kept");
        fs::create_dir(&folder).expect("the fixture folder");
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
