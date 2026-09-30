use cucumber::{gherkin::Step, given, then, when, World};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthStr;
use varde::editor::{span_text, Buffer};
use varde::format;
use varde::keys::{self, Drafts};
use varde::lsp::{self, About, Ask, Candidate, Candidates, Gone};
use varde::mouse::Encoding;
use varde::review::{self, GitFile, GitStatus};
use varde::risk::{self, Figures, Function, Kind, Metrics, Scope, Space};
use varde::startup::{
    self, Config, Formatter, PathStatus, Server, Startup, StartupError, Unanswerable,
};
use varde::story;
use varde::tools;
use varde::tree::{self, Entry};
use varde::tree_actions::{Action, Target};
use varde::{
    layout, mouse, reading, update, DiffLine, Direction, Effect, Event, Modal, Pane, Place,
    ReplaceFailed, Selection, State, Tap, View, PALETTE,
};

/// Where the pointer is when a scenario clicks or scrolls, in the pane's own
/// grid. A scenario never cares which cell it is, only that the child is told
/// about the one that was clicked.
const POINTER: Place = Place { line: 3, column: 7 };

/// Per-scenario state. Cucumber builds a fresh one for every scenario, so
/// scenarios cannot leak into each other.
///
/// `terminal_input` and `executed` model the terminal, which lives at the edge:
/// the World applies the effects `update` returns, exactly as the binary will.
#[derive(Debug, Default, World)]
pub struct VardeWorld {
    state: State,
    /// The edge's own pointer, kept across steps rather than made fresh per
    /// gesture: a drag held against a pane's edge is remembered there, and the
    /// steps that hold it and release it are steps of their own.
    pointer: mouse::Pointer,
    terminal_input: String,
    executed: Vec<String>,
    target: Option<Target>,
    rendered: Vec<View>,
    startup: Startup,
    config: Option<Config>,
    /// Exactly what starting asked the edge to do. The Rule that a server is
    /// only named — never started — is an assertion about this list.
    startup_effects: Vec<Effect>,
    error: Option<StartupError>,
    dirs: BTreeSet<PathBuf>,
    files: BTreeMap<PathBuf, String>,
    /// Every path a write reached, in order. What "unchanged" is a statement
    /// about: a file a scenario never registered on the disk model above has no
    /// contents to compare, and a buffer holding unsaved work is exactly that
    /// case.
    wrote: Vec<PathBuf>,
    /// Every path a scenario has ever named via "held:"/"holds:"/"now
    /// holds:"/"is gone", kept even after "is gone" empties `files` — a file
    /// that vanished still needs a `FileHunks` entry so staleness can tell
    /// "gone" from "never diffed at all".
    known_files: BTreeSet<PathBuf>,
    opened: Vec<PathBuf>,
    disk: BTreeMap<PathBuf, Vec<Entry>>,
    ai_spawned: Vec<String>,
    /// Whether the edge holds an AI pane. `main` derives `State::ai_running`
    /// from the pane it holds rather than the core remembering it, so the world
    /// does too.
    ai_pane: bool,
    /// Whether the edge holds a synthesizer child, whether it found the
    /// configured player, and whether the file the voice names is on disk.
    /// Told to the core rather than set by it, exactly as `ai_pane` above is
    /// and for the same reason.
    voice_child: bool,
    player_on_path: bool,
    voice_on_disk: bool,
    /// What the edge is playing, and the stream it built to play it — the
    /// words `Effect::Speak` handed over, cleared by `Effect::StopSpeaking`.
    /// The stream file itself lives outside every workspace and is the edge's
    /// (ADR 0014), so what a scenario can see of it is that it exists while a
    /// player does and is gone with it.
    speaking: Option<String>,
    /// Where each Utterance starts in the stream the stand-in edge built, and
    /// how many players it holds. The offsets are told to the core, so the
    /// World tells them; the count is what "exactly one reading is in flight"
    /// is a statement about, and it goes to two the moment a Reading starts
    /// over one already playing without stopping it first.
    stream: Vec<u32>,
    players: usize,
    /// The speed the Reading being played was built at. Set by `Effect::Speak`
    /// and by nothing else, which is what makes "the Reading in flight is
    /// still at the old speed" an assertion a re-pacing would break.
    spoken_at: Option<f32>,
    /// The offset a resume, a next or a previous asked the edge to play from.
    resumed_from: Option<u32>,
    /// Where the sound was when a scenario paused it, so "resumes from where
    /// it was paused" compares two values.
    paused_at: Option<u32>,
    /// The tree as it stood when the Reading was asked for, so "unchanged"
    /// compares two values rather than asserting an absence nothing could
    /// have filled.
    tree_before: Vec<PathBuf>,
    /// Set by a scenario where the CLI cannot be launched at all, so a spawn
    /// leaves the edge holding no pane — the path that used to leave the core
    /// believing a session was running.
    ai_spawn_fails: bool,
    notices: Vec<String>,
    scrolled: Vec<(Pane, Direction)>,
    clipboard: Option<String>,
    browser: Vec<String>,
    keys_sent: Vec<(Pane, Vec<u8>)>,
    /// Which split each `:split` asked for a shell beside, 0-based.
    splits: Vec<usize>,
    /// The tokens a `… is highlighted` step last produced, by line — the shape
    /// the highlighter answers in.
    highlighted: Vec<Vec<varde::highlight::Token>>,
    highlighted_source: String,
    /// The diff's two sides, each highlighted whole, exactly as the edge parses
    /// them when it reads a diff. Empty where a side could not be read.
    diff_sides: (
        Vec<Vec<varde::highlight::Token>>,
        Vec<Vec<varde::highlight::Token>>,
    ),
    exited: bool,
    relaunched: bool,
    /// Every `ReplaceBinary` asked for, which only the step that says the
    /// replacement finished answers.
    replacing: Vec<Effect>,
    terminal_clipboard: Option<String>,
    ai_stopped: bool,
    project: Vec<(String, String)>,
    /// The project's files as they stand on disk, which `Effect::IndexProject`
    /// walks. Kept apart from the index the core holds: a filter walks afresh,
    /// so a file added here after one has run must still turn up in the next.
    indexable: Vec<String>,
    diffs_read: Vec<PathBuf>,
    /// What the terminal's grid holds, as the edge would read it off a pty.
    screen: Vec<String>,
    /// What the AI session's grid holds. Separate from `screen` because they are
    /// separate ptys: a drag that read the wrong one is the bug behind this.
    ai_screen: Vec<String>,
    /// Every argv the edge was asked to run in the Debug group's own terminal.
    program: Vec<Vec<String>>,
    /// The size of the pty the edge holds for it, `None` when it holds none.
    /// Kept as a size rather than a flag because a hidden pane that was
    /// resized to nothing is the failure the scenarios pin.
    output_pty: Option<(u16, u16)>,
    /// What the edge is half-way through collecting — the key router needs it.
    drafts: Drafts,
    /// The content last shown or held for a diffed file, so a `Then` can
    /// recompute the blob oid it expects rather than hard-coding one.
    diff_contents: BTreeMap<String, String>,
    /// Revisions a scenario says git cannot resolve — `Effect::ReadStories`
    /// asks this instead of a real repository.
    unresolvable: BTreeSet<String>,
    /// The review list as it stood the moment authoring began, so a later
    /// `Then` can tell it was left untouched.
    repo_before: Option<Vec<String>>,
    /// The old side of a change — what "held:" records, kept separate from
    /// `files` (the current side) so "now holds:" overwriting the latter
    /// still leaves both sides of the subtraction available.
    held: BTreeMap<PathBuf, String>,
    /// What `refs/remotes/origin/HEAD` resolves to, per the scenario —
    /// `Effect::ResolveStory` asks this instead of a real repository.
    origin_head: Option<String>,
    upstream_branch: Option<String>,
    default_branch_config: Option<String>,
    /// Branch names a probe would find, for the `main`/`master` fallback.
    probes: BTreeSet<String>,
    /// What a range spelling the offline ladder (or an explicit command)
    /// produced resolves to on disk — the base and head a real repository
    /// would give `revparse`. Only a spelling declared here can be seen as
    /// already authored or as a valid explicit range, the same way
    /// `unresolvable` stands in for git elsewhere.
    resolved_oids: BTreeMap<String, (String, String)>,
    /// Every Scope an analysis was asked for, in order. The job itself is the
    /// edge's: the world records the request and answers only when a scenario
    /// says the figures arrived, so "computing" is a state a scenario can stand
    /// in.
    analyses: Vec<Analysis>,
    /// What `HEAD` resolves to, per the scenario — the edge tells the core the
    /// commit, so the world does too.
    head: Option<String>,
    /// The base and head most recently registered by "a story set exists for
    /// the range", so "that range is what … resolves to" has something to
    /// point a spelling at.
    last_authored_range: Option<(String, String)>,
    /// The repository's branch refs, as a scenario declared them — what
    /// `Effect::ReadBranches` answers with, since only the edge can read refs.
    branch_refs: Vec<story::BranchRef>,
    /// The branch `HEAD` is on, and every branch a checkout moved it to. Both,
    /// because "no branch was checked out" is a statement about the second.
    on_branch: String,
    /// Each one with the repository it was made in: a Guest repo's branch is
    /// checked out in the clone, and a checkout in the workspace instead is
    /// exactly the plausible-looking miss.
    checkouts: Vec<(PathBuf, String)>,
    /// Every test command the Gate ran, in order. The run itself is the edge's;
    /// the world records it and answers only when a scenario says the tests
    /// finished, so "waiting for the tests" is a state a scenario can stand in.
    tests_run: Vec<String>,
    /// What each Iteration's snapshot holds, taken off the project's files the
    /// moment the Iteration began — the edge copies files aside, so the world
    /// does too.
    snapshots: BTreeMap<u32, BTreeMap<String, String>>,
    /// Which Iterations were restored, and which files each restore put back.
    /// Both, because "only the files it touched" is a statement about the
    /// second, not the first.
    restores: Vec<u32>,
    restored: Vec<String>,
    /// Every path a delete reached — what "the sentinel was deleted" is a
    /// statement about, since a file that was never there leaves `files`
    /// looking the same either way.
    deleted: Vec<PathBuf>,
    /// Every directory a delete reached, kept apart from the files above
    /// because "no directory was deleted" is an absence about a whole tree.
    dirs_deleted: Vec<PathBuf>,
    /// What a file held at the last commit, and what the user's own uncommitted
    /// edits left in it. Kept apart so a revert can be held to reaching
    /// neither: not the commit, and not over the user's work.
    committed: BTreeMap<String, String>,
    mine: BTreeMap<String, String>,
    /// Every language server the edge holds, and the command it was started
    /// with. The world plays the edge: `State::lsp_running` is derived from
    /// this in `tell_core`, exactly as `main.rs` derives it from the servers it
    /// holds, so the core never remembers that a process exists.
    lsp_running: BTreeMap<String, String>,
    /// What a probe of this machine's `PATH` would find, which is the edge's
    /// answer and never the core's memory (R31.23). The world plays the edge
    /// here exactly as it does for `lsp_running` above.
    on_path: BTreeSet<String>,
    /// Whether a `git` binary is on this machine, which is the edge's answer
    /// and never the core's memory — the world plays the edge here exactly as
    /// it does for `on_path` above. `false` until a scenario says otherwise,
    /// because a capability nobody has checked is not one.
    git_on_path: bool,
    /// What the edge found in this workspace, by the name the library gave the
    /// fact (R31.27). The world plays the edge here for the same reason it does
    /// for `on_path`: a path on the machine is nothing the core may look up.
    workspace_facts: BTreeMap<String, String>,
    /// Folders `Effect::ReadFolder` asked for, until the batch they came in is
    /// finished — the edge queues the answer as an event rather than deciding
    /// it mid-batch (`drain` in main.rs), because the rest of the batch has
    /// facts still to tell the core.
    folders_to_read: Vec<PathBuf>,
    /// Every spawn the edge was asked for, in order — including one that
    /// produced no process, which is what "1 language server was started"
    /// counts.
    lsp_started: Vec<(String, String)>,
    /// The arguments each spawn was asked for, which is where an interpolated
    /// name either arrived or did not.
    lsp_args: BTreeMap<String, Vec<String>>,
    /// Every message sent to each language's server, in order. No scenario runs
    /// a server, so this list and the canned replies are the whole of the
    /// conversation.
    lsp_sent: Vec<(String, Value)>,
    /// Languages whose canned server answers its handshake as soon as it is
    /// asked — what "a language server for X is ready" stands for.
    lsp_answers: BTreeSet<String>,
    /// Languages whose configured command does not exist, so the edge ends up
    /// holding nothing.
    lsp_fails: BTreeSet<String>,
    /// Every formatter the edge was asked to run, as it was asked — the
    /// command, its arguments and the text that would have gone in on stdin.
    /// Kept rather than answered here: what the command made of it is the fact
    /// only the edge can observe, so a Scenario says it in a step of its own,
    /// which is what makes an install that worked and one that has not
    /// different Scenarios.
    formatter_runs: Vec<Effect>,
    /// Whether the edge is holding a debounce for the candidate list. One flag
    /// and not a queue, because the edge holds one timer that every keystroke
    /// restarts: "the debounce window passes" fires once however many times the
    /// core armed it, which is what makes the request count in
    /// `features/language_intelligence.feature` a statement about debouncing
    /// rather than about arithmetic.
    candidates_armed: bool,
    /// The window the edge is holding before it asks what the pointer is
    /// resting on, as the core asked for it. The number and not a flag: a
    /// Scenario says how long a rest is, so the step that fires it can only
    /// answer for a window the core actually armed.
    dwell_armed: Option<u64>,
    /// Every notice reported, kept even after one is withdrawn: "reported once"
    /// is a statement about the session, not about what the footer happens to
    /// be saying now.
    reported: Vec<String>,
    /// Whether the edge is holding back its answer about what the arriving
    /// artifact's Sites hold — one flag, because the fill is one round trip
    /// per artifact. Set by the Scenario that watches a set stay unwalkable
    /// while the read is outstanding; every other Scenario answers at once,
    /// the way a real edge does inside the batch that read the artifact.
    hold_site_texts: bool,
    /// What each notice that names something has named. The slug alone cannot say
    /// where a refused jump would have gone, and "naming where it went" is what
    /// makes that refusal a decision rather than a key that did nothing.
    named: Vec<String>,
    /// The scripted Debug adapter. No scenario runs one: the world plays the
    /// edge and the adapter both, answering requests with canned replies and
    /// sending the events a scenario names.
    dap: FakeAdapter,
}

/// What the scripted Debug adapter has been asked and holds ready to say.
#[derive(Debug, Default)]
struct FakeAdapter {
    /// Every command the edge was asked to spawn, including one that
    /// produced no process.
    spawned: Vec<String>,
    /// Every port the edge was asked to connect to an adapter a language
    /// server hosts on, which is spawned by nobody.
    dialed: Vec<u16>,
    /// Whether the edge holds the adapter.
    held: bool,
    /// Whether it answers `initialize` the moment it is asked — "is ready".
    ready: bool,
    /// Every message sent to it, in order.
    sent: Vec<Value>,
    /// The call stack each thread answers `stackTrace` with: name, file, line
    /// and presentation hint.
    stacks: BTreeMap<i64, Vec<(String, PathBuf, usize, String)>>,
    /// The scopes the chosen Frame answers `scopes` with, by name, and the
    /// reference each is asked for its members by.
    scopes: Vec<(String, i64)>,
    /// What each reference answers `variables` with. A scenario plants
    /// members here; the adapter hands back whichever page was asked for.
    members: BTreeMap<i64, Vec<Value>>,
    /// How many requests had been sent when the last pause brought the
    /// Variables up, which is what "since the pause" counts from.
    since_pause: usize,
    /// The `stopped` event that brought the last pause, kept so a scenario
    /// that plants a member can drive the same pause again and have the
    /// Variables hold it.
    stopped: Option<Value>,
    /// The Exception filters it lists in its `initialize` answer, as the
    /// protocol shapes them.
    filters: Vec<Value>,
    /// What the next `stopped` event says paused the program.
    text: Option<String>,
    /// Every child session's connection the edge was asked to open, and what
    /// was sent over it, by the number it was opened under.
    children: BTreeMap<usize, Vec<Value>>,
    /// The one thread a child session names, which stops as soon as the child
    /// is configured, the way a worker started under a Breakpoint does.
    child_thread: String,
}

impl VardeWorld {
    /// One file's two sides and the hunks between them, at Varde's pinned
    /// options — for the poll and for an arriving artifact alike, so the two
    /// never describe the same file differently. Identical bytes on both
    /// sides diff to no hunks, so a bare "holds:" fixture with no "held:"
    /// counterpart never needs excluding.
    fn read_file(&self, repo: &Path, file: &str) -> story::FileHunks {
        let path = repo.join(file);
        let old = self.held.get(&path).cloned().unwrap_or_default();
        let new = self.files.get(&path).cloned().unwrap_or_default();
        story::FileHunks {
            file: file.to_string(),
            hunks: story::hunks(old.as_bytes(), new.as_bytes()),
            old_exists: self.held.contains_key(&path),
            old_text: old,
            // The world's ranges have no commit of their own: `held` is the
            // base and `files` is both the head and the working tree, so a
            // scenario about drift between them is one nothing here can play.
            head_exists: self.files.contains_key(&path),
            head_text: new.clone(),
            new_exists: self.files.contains_key(&path),
            new_text: new,
        }
    }

    /// Plays the edge's own poll and its arrival read alike: every file the
    /// range touches, the way `main.rs` reports them alongside `git_status`,
    /// plus any the Story names that the range did not touch — a `context`
    /// Site's file, or a citation pointing outside the change.
    fn read_files(&self, repo: &Path, named: &[String]) -> Vec<story::FileHunks> {
        let mut paths: BTreeSet<PathBuf> = self.held.keys().cloned().collect();
        paths.extend(self.files.keys().cloned());
        paths.extend(self.known_files.iter().cloned());
        let mut files: Vec<String> = paths
            .into_iter()
            .filter(|path| path.starts_with(repo))
            .map(|path| {
                path.strip_prefix(repo)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        for file in named {
            if !files.contains(file) {
                files.push(file.clone());
            }
        }
        files
            .iter()
            .map(|file| self.read_file(repo, file))
            .collect()
    }

    fn recompute_file_hunks(&mut self) {
        let repo = self.state.repo_root().to_path_buf();
        self.state.file_hunks = self.read_files(&repo, &[]).into();
    }

    /// Marks a path `Modified` in `state.repo` if it isn't listed already —
    /// what a real edge's next `git_status` poll would find once a file's
    /// bytes differ from HEAD, whether it changed or disappeared.
    fn mark_modified(&mut self, path: &str) {
        let mut repo = self.state.repo.clone().unwrap_or_default();
        if !repo.iter().any(|file| file.path == path) {
            repo.push(GitFile {
                path: path.to_string(),
                status: GitStatus::Modified,
            });
        }
        self.state.repo = Some(repo);
    }

    /// A click at a place in a pane's text, routed the way the edge routes it:
    /// press then release, because the child's click is decided on the release.
    fn click(&mut self, pane: Pane, at: (usize, usize), modifiers: terminput::KeyModifiers) {
        let panes = self.panes();
        let (column, row) = pointer_at(&self.state, &panes, pane, at);
        let mut pointer = mouse::Pointer::default();
        for kind in [mouse::Kind::LeftDown, mouse::Kind::LeftUp] {
            let outcome = mouse::on_mouse(
                &self.state,
                &panes,
                &mut pointer,
                mouse::Input {
                    kind,
                    column,
                    row,
                    modifiers,
                },
            );
            for event in outcome.events {
                self.send(event);
            }
            // The world plays the edge, as it does for a drag: the row the
            // link request names is read off the grid the scenario gave the pane.
            if let Some((pane, at)) = outcome.link {
                let row = self.pane_lines(pane)[at.line - 1].clone();
                self.send(Event::ClickLink {
                    row,
                    column: at.column,
                });
            }
        }
    }

    /// Two presses on the same cell, `apart` milliseconds apart, through one
    /// pointer: the second press is only a double-click to the pointer that saw
    /// the first, and the gap is what the double-tap window is measured against.
    fn click_twice(&mut self, pane: Pane, at: (usize, usize), apart: u64) {
        let panes = self.panes();
        let (column, row) = pointer_at(&self.state, &panes, pane, at);
        let mut pointer = mouse::Pointer::default();
        for stamp in [0, apart] {
            pointer.at_ms = stamp;
            for kind in [mouse::Kind::LeftDown, mouse::Kind::LeftUp] {
                let outcome = mouse::on_mouse(
                    &self.state,
                    &panes,
                    &mut pointer,
                    mouse::Input {
                        kind,
                        column,
                        row,
                        modifiers: terminput::KeyModifiers::NONE,
                    },
                );
                for event in outcome.events {
                    self.send(event);
                }
            }
        }
    }

    /// The pointer moving with nothing pressed, routed the way the edge routes
    /// it. `None` for the place is the pointer off the editor's text — over
    /// another pane, since a move is reported wherever it happens.
    fn point(
        &mut self,
        pane: Pane,
        at: Option<(usize, usize)>,
        modifiers: terminput::KeyModifiers,
    ) {
        let panes = self.panes();
        let (column, row) = match at {
            Some(place) => pointer_at(&self.state, &panes, pane, place),
            None => (panes.tree.x + 1, panes.tree.y + 1),
        };
        let outcome = mouse::on_mouse(
            &self.state,
            &panes,
            &mut mouse::Pointer::default(),
            mouse::Input {
                kind: mouse::Kind::Moved,
                column,
                row,
                modifiers,
            },
        );
        for event in outcome.events {
            self.send(event);
        }
    }

    /// The screen the drag and click steps hit-test against: what the scenario
    /// reported, or a plain window if it never said.
    fn screen(&self) -> (u16, u16) {
        (
            match self.state.screen_width {
                0 => 120,
                width => width,
            },
            match self.state.screen_height {
                0 => 26,
                height => height,
            },
        )
    }

    fn panes(&self) -> layout::Layout {
        let (width, height) = self.screen();
        layout::panes(
            width,
            height,
            self.state.tree_divider as u16,
            self.state.ai_width.map(|width| width as u16),
            story::band_height(&self.state),
            story::step_menu_width(&self.state),
            varde::shapes(&self.state),
        )
    }

    /// The world plays the edge for a drag: `mouse` says which pane and which
    /// span, and the world reads the characters — out of the buffer, or out of
    /// the grid the scenario gave the terminal.
    fn drag(&mut self, pane: Pane, from: (usize, usize), to: (usize, usize)) {
        self.pointer = mouse::Pointer::default();
        for place in [from, to] {
            let (column, row) = pointer_at(&self.state, &self.panes(), pane, place);
            self.report(mouse::Kind::LeftDrag, column, row);
        }
    }

    /// One mouse report, routed the way the edge routes it: the library says
    /// which pane and which span, and only a pty's drag comes back unfinished
    /// — the edge reads the grid of the pane the span names.
    fn report(&mut self, kind: mouse::Kind, column: u16, row: u16) {
        let panes = self.panes();
        let mut pointer = self.pointer;
        let outcome = mouse::on_mouse(
            &self.state,
            &panes,
            &mut pointer,
            mouse::Input {
                kind,
                column,
                row,
                modifiers: terminput::KeyModifiers::NONE,
            },
        );
        self.pointer = pointer;
        for event in outcome.events {
            self.send(event);
        }
        let Some(selection) = outcome.select else {
            return;
        };
        let lines = self.pane_lines(selection.pane);
        self.send(Event::SelectIn {
            pane: selection.pane,
            from: selection.from,
            to: selection.to,
            text: span_text(&lines, selection.from, selection.to),
        });
    }

    /// The lines a pane holds, as the edge would read them.
    fn pane_lines(&self, pane: Pane) -> Vec<String> {
        match pane {
            Pane::Editor => self
                .state
                .current_buffer
                .as_ref()
                .and_then(|path| self.state.buffers.get(path))
                .map(|buffer| buffer.shown().split('\n').map(str::to_string).collect())
                .unwrap_or_default(),
            Pane::Ai => self.ai_screen.clone(),
            Pane::Evaluator => self
                .state
                .evaluator
                .as_ref()
                .map(|evaluator| {
                    evaluator
                        .snippet
                        .shown()
                        .split('\n')
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            Pane::Tree
            | Pane::Risk
            | Pane::Buffers
            | Pane::History
            | Pane::Breakpoints
            | Pane::Frames
            | Pane::Diagnostics
            | Pane::Variables
            | Pane::Output
            | Pane::Terminal => self.screen.clone(),
        }
    }

    fn send(&mut self, event: Event) {
        let (state, effects) = update(&self.state, event);
        self.state = state;
        self.apply(effects);
        self.tell_core();
    }

    /// What `main` does after every event: whether a session exists is the
    /// edge's to know, so the core is told rather than remembering it.
    fn tell_core(&mut self) {
        self.state.ai_running = self.ai_pane;
        self.state.output_running = self.output_pty.is_some();
        // What `resize_panes` does before every frame: fit the pty to the
        // rectangle the layout gives it, and leave it alone when the layout
        // gives it none.
        let output = self.panes().output;
        if let (true, Some(size)) = (
            self.output_pty.is_some(),
            layout::pty_size(output.width, output.height),
        ) {
            self.output_pty = Some(size);
        }
        self.state.head = self.head.clone();
        // Which branch HEAD is on is the edge's to read, so the world reads it
        // too — the core never writes it.
        self.state.branch = (!self.on_branch.is_empty()).then(|| self.on_branch.clone());
        self.state.lsp_running = self.lsp_running.keys().cloned().collect();
        self.state.commands_on_path = self.on_path.clone();
        self.state.git_installed = self.git_on_path;
        self.state.workspace_facts = self.workspace_facts.clone();
        self.state.voice_running = self.voice_child;
        self.state.player_installed = self.player_on_path;
        // A blank voice names no file, so none is on disk.
        self.state.voice_installed = self.voice_on_disk && !self.state.speech.voice.is_empty();
        // What `tell_traced` tells the core before every frame: the trace of
        // the current revision.
        if let Some(path) = self.state.current_buffer.clone() {
            let committed = self.state.committed.get(&path).and_then(Option::as_deref);
            if let Some(buffer) = self.state.buffers.get(&path) {
                let lines = varde::authorship::traced(committed, buffer.shown());
                self.state.traced = Some((path, buffer.revision(), lines.into()));
            }
        }
    }

    /// What the edge does wherever it stops holding a pane: the core is told
    /// the session is gone, so what was queued for it is dropped.
    fn ai_is_gone(&mut self) {
        self.ai_pane = false;
        let (state, effects) = update(&self.state, Event::AiExited);
        self.state = state;
        self.apply(effects);
    }

    /// What the edge does wherever it *starts* holding one: the core is told,
    /// and the conversation begins against the process rather than against the
    /// asking. Whether the spawn was asked for in this scenario or the server
    /// was already up before it makes no difference here, which is the whole
    /// reason a Scenario can say a server is already running.
    fn lsp_is_running(&mut self, language: &str, command: &str) {
        self.lsp_running
            .insert(language.to_string(), command.to_string());
        self.tell_core();
        self.send_now(Event::LspStarted {
            language: language.to_string(),
        });
    }

    /// What the edge does wherever it stops holding a server: the core is told,
    /// so what that conversation was holding goes with it.
    fn lsp_is_gone(&mut self, language: &str, why: Gone) {
        self.lsp_running.remove(language);
        self.tell_core();
        self.send_now(Event::LspGone {
            language: language.to_string(),
            why,
        });
    }

    /// The scripted adapter's canned replies: `initialize` with the one
    /// capability Varde reads, a thread to pause, the stack a scenario gave a
    /// thread, and a `disconnect` answered. Anything else it is only told.
    fn adapter_answers(&mut self, message: &Value) {
        if message["type"] != "request" {
            return;
        }
        let body = match message["command"].as_str() {
            Some("initialize") => json!({
                "supportsConfigurationDoneRequest": true,
                "exceptionBreakpointFilters": self.dap.filters,
            }),
            Some("threads") => json!({ "threads": [{ "id": 1, "name": "main" }] }),
            Some("stackTrace") => {
                let thread = message["arguments"]["threadId"]
                    .as_i64()
                    .unwrap_or_default();
                let frames: Vec<Value> = self
                    .dap
                    .stacks
                    .get(&thread)
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(id, (name, file, line, hint))| {
                        let mut source = json!({ "path": file });
                        if !hint.is_empty() {
                            source["presentationHint"] = json!(hint);
                        }
                        json!({ "id": id, "name": name, "line": line, "source": source })
                    })
                    .collect();
                json!({ "stackFrames": frames })
            }
            Some("disconnect") => json!({}),
            // As an adapter that can run one thread alone answers: only what
            // was asked for ran on.
            Some("continue") => json!({
                "allThreadsContinued": message["arguments"]["singleThread"] != json!(true)
            }),
            Some("scopes") => {
                let scopes: Vec<Value> = self
                    .dap
                    .scopes
                    .iter()
                    .map(|(name, reference)| {
                        json!({ "name": name, "variablesReference": reference })
                    })
                    .collect();
                json!({ "scopes": scopes })
            }
            // The page asked for, or everything the reference holds when no
            // page was named — which is what a real adapter does with a
            // request carrying no `start`.
            Some("variables") => {
                let reference = message["arguments"]["variablesReference"]
                    .as_i64()
                    .unwrap_or_default();
                let held = self
                    .dap
                    .members
                    .get(&reference)
                    .cloned()
                    .unwrap_or_default();
                let start = message["arguments"]["start"].as_u64().unwrap_or_default() as usize;
                let count = message["arguments"]["count"]
                    .as_u64()
                    .map_or(held.len(), |count| count as usize);
                let page: Vec<Value> = held.into_iter().skip(start).take(count).collect();
                json!({ "variables": page })
            }
            _ => return,
        };
        self.adapter_says(json!({
            "type": "response",
            "request_seq": message["seq"],
            "success": true,
            "command": message["command"],
            "body": body,
        }));
    }

    /// One message from the adapter, driven straight in as the edge would.
    fn adapter_says(&mut self, message: Value) {
        self.send_now(Event::DapReceived {
            json: message.to_string(),
            from: 0,
        });
    }

    /// A child session's side of the protocol: it starts when asked, names
    /// its one thread, and that thread stops once it is configured. Its
    /// thread is numbered 1, as the session's own is, which is the point.
    fn child_answers(&mut self, child: usize, message: &Value) {
        if message["type"] != "request" {
            return;
        }
        let command = message["command"].as_str().unwrap_or_default();
        let body = match command {
            "threads" => json!({ "threads": [{ "id": 1, "name": self.dap.child_thread }] }),
            "stackTrace" => json!({ "stackFrames": [] }),
            _ => json!({}),
        };
        let says = |world: &mut Self, message: Value| {
            world.send_now(Event::DapReceived {
                json: message.to_string(),
                from: child,
            })
        };
        says(
            self,
            json!({
                "type": "response",
                "request_seq": message["seq"],
                "success": true,
                "command": command,
                "body": body,
            }),
        );
        match command {
            "launch" | "attach" => says(self, json!({ "type": "event", "event": "initialized" })),
            "configurationDone" => says(
                self,
                json!({
                    "type": "event",
                    "event": "stopped",
                    "body": { "threadId": 1, "reason": "breakpoint" },
                }),
            ),
            _ => {}
        }
    }

    /// What the canned server said, driven straight in as the edge would.
    fn lsp_replies(&mut self, language: &str, message: Value) {
        self.send_now(Event::LspReceived {
            language: language.to_string(),
            json: message.to_string(),
        });
    }

    /// One event through `update` from inside an effect the world is playing
    /// out — the three lines `apply` uses everywhere it feeds one back.
    fn send_now(&mut self, event: Event) {
        let (state, effects) = update(&self.state, event);
        self.state = state;
        self.apply(effects);
    }

    fn apply(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            let Some(effect) = self.applied_to_panes(effect) else {
                continue;
            };
            let Some(effect) = self.applied_to_files(effect) else {
                continue;
            };
            let Some(effect) = self.applied_to_lists(effect) else {
                continue;
            };
            self.applied_to_jobs(effect);
        }
        // What a folder read found, once the batch that asked for it is done:
        // a read answered mid-batch decides the next event against facts the
        // effects behind it have not told the core yet, which is how one
        // opening asked for two language servers.
        for path in std::mem::take(&mut self.folders_to_read) {
            let entries = self.disk.get(&path).cloned().unwrap_or_default();
            self.send_now(Event::Expand { path, entries });
        }
    }

    /// The shell, the AI pane, the clipboard and the status row.
    fn applied_to_panes(&mut self, effect: Effect) -> Option<Effect> {
        match effect {
            Effect::SetTerminalInput(text) => self.terminal_input = text,
            Effect::SplitTerminal { from } => self.splits.push(from),
            Effect::RunInTerminal(text) => {
                self.executed.push(text);
                self.terminal_input.clear();
            }
            Effect::RenderView(view) => self.rendered.push(view),
            Effect::EnsureDir(path) => {
                self.dirs.insert(path);
            }
            Effect::OpenBuffer(path) => {
                self.opened.push(path.clone());
                let (state, effects) = update(
                    &self.state,
                    Event::BufferOpened {
                        contents: "contents".to_string(),
                        path,
                        preview: false,
                        at: None,
                    },
                );
                self.state = state;
                self.apply(effects);
            }
            Effect::PreviewBuffer(path) => {
                self.opened.push(path.clone());
                let (state, effects) = update(
                    &self.state,
                    Event::BufferOpened {
                        contents: "contents".to_string(),
                        path,
                        preview: true,
                        at: None,
                    },
                );
                self.state = state;
                self.apply(effects);
            }
            Effect::WriteFile { path, contents } => {
                self.wrote.push(path.clone());
                self.files.insert(path, contents);
            }
            Effect::DeleteDir(path) => self.dirs_deleted.push(path),
            Effect::DeleteFile(path) => {
                self.deleted.push(path.clone());
                self.files.remove(&path);
            }
            Effect::SpawnAi { command } => {
                self.ai_spawned.push(command);
                if self.ai_spawn_fails {
                    self.ai_is_gone();
                } else {
                    self.ai_pane = true;
                }
            }
            Effect::StartLsp {
                language,
                command,
                args,
            } => {
                self.lsp_started.push((language.clone(), command.clone()));
                self.lsp_args.insert(language.clone(), args);
                // A command that does not exist leaves the edge holding
                // nothing, which is the failure `ai_running` was: a spawn that
                // was asked for is not a process that exists.
                if self.lsp_fails.contains(&language) {
                    self.lsp_is_gone(&language, Gone::FailedToStart);
                } else {
                    self.lsp_is_running(&language, &command);
                }
            }
            Effect::StartDap {
                reach: varde::debug::Reach::Port(port),
                ..
            } => {
                self.dap.dialed.push(port);
                if self.dap.ready {
                    self.dap.held = true;
                    self.send_now(Event::DapStarted { from: 0 });
                }
            }
            Effect::StartDap { command, .. } => {
                self.dap.spawned.push(command.clone());
                // A command that is not there is the spawn failing as the
                // edge sees it; one that is starts only once the scenario's
                // adapter is ready, so a Scenario can report the spawn itself.
                if !self.on_path.contains(&command) {
                    self.send_now(Event::DapGone {
                        why: varde::debug::Gone::Missing,
                        from: 0,
                    });
                } else if self.dap.ready {
                    self.dap.held = true;
                    self.send_now(Event::DapStarted { from: 0 });
                }
            }
            Effect::DapChild { child } => {
                self.dap.children.insert(child, Vec::new());
                self.send_now(Event::DapStarted { from: child });
            }
            // A child session's connection goes with the adapter's, so one
            // let go hears what it was sent and answers nothing.
            Effect::DapSend { to, json } if to != 0 => {
                let message: Value = serde_json::from_str(&json).expect("valid DAP");
                self.dap
                    .children
                    .get_mut(&to)
                    .expect("a child session the edge was asked to open")
                    .push(message.clone());
                if self.dap.held {
                    self.child_answers(to, &message);
                }
            }
            Effect::DapSend { json, .. } => {
                let message: Value = serde_json::from_str(&json).expect("valid DAP");
                self.dap.sent.push(message.clone());
                match self.dap.held {
                    true => self.adapter_answers(&message),
                    false => self.send_now(Event::DapGone {
                        why: varde::debug::Gone::Exited,
                        from: 0,
                    }),
                }
            }
            Effect::StopDap => {
                if std::mem::take(&mut self.dap.held) {
                    // The debugged program goes with the session it belonged
                    // to, which is what the edge does at every site that stops
                    // holding an adapter.
                    self.output_pty = None;
                    self.send_now(Event::DapGone {
                        why: varde::debug::Gone::Exited,
                        from: 0,
                    });
                }
            }
            Effect::LspSend { language, json } => {
                let message: Value = serde_json::from_str(&json).expect("valid JSON-RPC");
                let handshake = message["method"] == "initialize";
                let id = message["id"].clone();
                self.lsp_sent.push((language.clone(), message));
                match self.lsp_running.contains_key(&language) {
                    // The canned server answers its handshake at once, which is
                    // what "is ready" means. Everything else it is told, it is
                    // only told.
                    true => {
                        if handshake && self.lsp_answers.contains(&language) {
                            // Declaring what it can do, because a capability is
                            // the server's own answer about itself and Varde
                            // asks for nothing it was not offered. A server
                            // that declines one is configured by the Scenario
                            // that is about declining.
                            self.lsp_replies(
                                &language,
                                json!({"jsonrpc": "2.0", "id": id, "result": {"capabilities": {
                                    "hoverProvider": true,
                                    "definitionProvider": true,
                                    "completionProvider": {},
                                }}}),
                            );
                        }
                    }
                    // Writing to a server the edge does not hold is how the edge
                    // learns it is gone, and the core is told rather than left
                    // believing in it.
                    false => self.lsp_is_gone(&language, Gone::Exited),
                }
            }
            Effect::DebounceCandidates(_) => self.candidates_armed = true,
            Effect::DwellHover(after_ms) => self.dwell_armed = Some(after_ms),
            Effect::Notify(notice) => {
                self.notices.push(notice.to_string());
                self.reported.push(notice.to_string());
            }
            Effect::NotifyAbout { slug, about } => {
                self.notices.push(slug.to_string());
                self.reported.push(slug.to_string());
                self.named.push(about);
            }
            // The edge holds one status line, so a withdrawal leaves
            // nothing showing rather than the notice before it.
            Effect::ClearNotice => self.notices.clear(),
            // What the edge does: start the program in the Debug group's own
            // terminal, never in a shell. Its pty opens at the floor and is
            // fitted by `tell_core` above, exactly as the edge does.
            Effect::RunProgram { argv, .. } => {
                self.program.push(argv);
                self.output_pty = Some((2, 1));
            }
            // What the edge does: build the stream under `~/.varde/tmp` and
            // play it. No path crosses the seam, so none is modelled — what a
            // scenario can see is that words reached a voice.
            Effect::Speak { utterances, speed } => {
                self.speaking = Some(reading::words(&utterances));
                self.players += 1;
                self.stream = built(&utterances);
                self.spoken_at = Some(speed);
            }
            // A resume, a next or a previous: the same words out of the stream
            // that is already there, which is what "does not re-synthesize"
            // is a statement about — nothing here rebuilds `stream`.
            Effect::SpeakFrom { at_ms } => {
                self.speaking = self
                    .state
                    .reading
                    .as_ref()
                    .map(|reading| reading::words(&reading.utterances));
                self.players = 1;
                self.resumed_from = Some(at_ms);
            }
            // A pause keeps the stream and stops the sound; a stop takes both.
            Effect::PauseSpeaking => {
                self.speaking = None;
                self.players = 0;
            }
            Effect::StopSpeaking => {
                self.speaking = None;
                self.players = 0;
                self.stream.clear();
            }
            other => return Some(other),
        }
        None
    }

    /// Buffers and the files under them.
    fn applied_to_files(&mut self, effect: Effect) -> Option<Effect> {
        match effect {
            Effect::Scrolled(pane, direction) => self.scrolled.push((pane, direction)),
            Effect::SetClipboard(text) => self.clipboard = Some(text),
            Effect::OpenUrl(url) => self.browser.push(url),
            Effect::SendKeys { pane, bytes } => self.keys_sent.push((pane, bytes)),
            Effect::Exit => self.exited = true,
            Effect::Relaunch => self.relaunched = true,
            Effect::StopAi => {
                self.ai_stopped = true;
                self.ai_is_gone();
            }
            Effect::ClipboardViaTerminal(text) => self.terminal_clipboard = Some(text),
            // What the edge does: read the clipboard and hand the text back as
            // the one edit a paste is. With nothing on it there is nothing to
            // hand back, which is the same nothing a machine with no clipboard
            // at all answers with.
            Effect::ReadClipboard => {
                if let Some(text) = self.clipboard.clone().filter(|text| !text.is_empty()) {
                    let (state, effects) = update(&self.state, Event::EditorPaste(text));
                    self.state = state;
                    self.apply(effects);
                }
            }
            // What the edge does: load the diff, then hand it back.
            // What the edge does: walk the project and hand back the list.
            Effect::IndexProject => {
                let files = self.indexable.clone();
                let (state, effects) = update(&self.state, Event::Indexed(files));
                self.state = state;
                self.apply(effects);
            }
            // What the edge does: assemble the sources and scan them.
            Effect::RunSearch(query) => {
                let disk = self.project.clone();
                let sources = varde::search::sources(&self.state, disk);
                let results = varde::search::scan(&query, &sources);
                let (state, effects) = update(&self.state, Event::Searched(results));
                self.state = state;
                self.apply(effects);
            }
            Effect::OpenAt { path, at } => {
                self.opened.push(path.clone());
                let (state, effects) = update(
                    &self.state,
                    Event::BufferOpened {
                        contents: self
                            .project
                            .iter()
                            .find(|(name, _)| self.state.root.join(name) == path)
                            .map(|(_, contents)| contents.clone())
                            .or_else(|| self.files.get(&path).cloned())
                            .unwrap_or_default(),
                        path,
                        preview: false,
                        at: Some(at),
                    },
                );
                self.state = state;
                self.apply(effects);
            }
            other => return Some(other),
        }
        None
    }

    /// The effects that read something back and feed it in as an event.
    fn applied_to_lists(&mut self, effect: Effect) -> Option<Effect> {
        match effect {
            Effect::ReadBreakpointFile(path) => {
                let contents = self.on_disk(&path);
                let (state, effects) =
                    update(&self.state, Event::BreakpointFileRead { path, contents });
                self.state = state;
                self.apply(effects);
            }
            Effect::ReadForReview(path) => {
                let contents = self.on_disk(&path);
                let (state, effects) =
                    update(&self.state, Event::ReviewFileRead { path, contents });
                self.state = state;
                self.apply(effects);
            }
            Effect::ReadDiff(path) => {
                self.diffs_read.push(path.clone());
                let file = path
                    .strip_prefix(&self.state.root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                let lines = (1..=3)
                    .map(|number| varde::DiffLine {
                        new_line: Some(number),
                        old_line: None,
                        removed: false,
                        text: format!("line {number}"),
                    })
                    .collect();
                let (state, effects) = update(
                    &self.state,
                    Event::ShowDiff {
                        file,
                        lines,
                        revision: "mock-revision".to_string(),
                    },
                );
                self.state = state;
                self.apply(effects);
            }
            Effect::SaveState(contents) => self.startup.state_json = Some(contents),
            // Recorded, never answered: the analysis runs off the main
            // loop, so the figures arrive when a scenario says they do.
            Effect::AnalyseRisk {
                scope,
                generation,
                files,
                base,
            } => self.analyses.push(Analysis {
                scope,
                generation,
                files,
                base,
            }),
            Effect::ReadFolder(path) => self.folders_to_read.push(path),
            // The world plays the edge: the offline default-branch
            // ladder, or an explicit range, resolved against whatever
            // the scenario declared instead of a real repository.
            // What the edge does: copy the whole tree aside under the
            // Iteration's number — the same files for either Scope, because
            // the restore puts back only what the Iteration touched and a
            // snapshot narrower than the session's reach could not put back
            // a file it edited outside the Scope it was given.
            Effect::Snapshot { iteration } => {
                self.snapshots.insert(iteration, self.tree());
            }
            // What the edge does: put back only the files whose bytes
            // differ from the snapshot — which is exactly the files the
            // Iteration touched.
            Effect::RestoreSnapshot { iteration } => {
                self.restores.push(iteration);
                let snapshot = self.snapshots.get(&iteration).cloned().unwrap_or_default();
                for (file, before) in snapshot {
                    if self.tree().get(&file) != Some(&before) {
                        self.restored.push(file.clone());
                        self.write_tree(&file, &before);
                    }
                }
            }
            other => return Some(other),
        }
        None
    }

    /// Whatever the three groups above did not take. Last, so its catch-all is
    /// every effect they declined — which is none of them.
    fn applied_to_jobs(&mut self, effect: Effect) {
        match effect {
            Effect::RunTests { command } => self.tests_run.push(command),
            run @ Effect::RunFormatter { .. } => self.formatter_runs.push(run),
            // Asked for, and answered by the step that says what `PATH` holds:
            // the whole point of the re-check is that the answer arrives after
            // the asking, so a world that answered it here would make an
            // install that worked and one that did not the same scenario.
            Effect::ProbePath => {}
            // Answered by the step that says what the Release is, for the
            // reason the probe above is: the answer arrives after the asking.
            Effect::CheckRelease { .. } => {}
            fetch @ Effect::ReplaceBinary { .. } => self.replacing.push(fetch),
            // The world plays the edge: the core says which files the arriving
            // artifact names, and this diffs and reads each one — the old side
            // out of what the range's base holds, the new side off the working
            // tree, exactly the pair `story::staleness` compares against and
            // the hunks the arrival checks run over. The same read
            // `recompute_file_hunks` plays for the poll, narrowed to the files
            // asked for.
            Effect::ReadStoryFiles {
                repo,
                base: _,
                head: _,
                files,
            } => {
                if self.hold_site_texts {
                    return;
                }
                let read = self.read_files(&repo, &files);
                let (state, effects) = update(&self.state, Event::StoryFiles(read));
                self.state = state;
                self.apply(effects);
            }
            // The world plays the edge: what git would have answered about the
            // range is not the core's decision, and `story::context_file` has
            // its own unit tests. What a scenario can see from here is that the
            // hand-over was written, beside the set, before the prompt went out.
            Effect::WriteStoryContext {
                repo: _,
                spelling,
                path,
            } => {
                self.wrote.push(path.clone());
                self.files.insert(path, format!("the change in {spelling}"));
            }
            // The world plays git: the refs a scenario declared, and whether
            // it said the folder is a repository with a clean tree at all.
            Effect::ReadBranches => {
                let branching = match &self.state.repo {
                    None => story::Branching::NotARepository,
                    Some(files) if story::uncommitted(files) => story::Branching::Dirty,
                    Some(_) => story::Branching::Listed(self.branch_refs.clone()),
                };
                self.send_now(Event::Branches(branching));
            }
            // The world plays the edge reading its modelled disk: the file as
            // a step last left it, or as Varde was started on.
            Effect::ReadGlobalConfig {
                path,
                kind,
                name,
                write,
            } => {
                let text = self
                    .files
                    .get(&path)
                    .cloned()
                    .or_else(|| self.startup.global_config.clone())
                    .unwrap_or_default();
                self.send_now(Event::GlobalConfigRead {
                    kind,
                    name,
                    write,
                    text: Some(text),
                });
            }
            Effect::ReadInstallStatus(sentinel) => {
                let status = self.files.get(&sentinel).cloned();
                self.send_now(Event::InstallEnded(status));
            }
            // The world plays the edge (ADR 0015): the exit status is what the
            // sentinel holds, and a clone that worked is read like any other
            // repository — the refs the scenario declared.
            Effect::ReadGuestBranches { sentinel, how, .. } => {
                let status = self.files.get(&sentinel).cloned();
                self.send_now(Event::Branches(match status.as_deref().map(str::trim) {
                    Some("0") => story::Branching::Listed(self.branch_refs.clone()),
                    // A sentinel there is nothing to read is the failure with
                    // no status to name, the way the edge reports one. `how`
                    // is the core's, echoed back the way the edge echoes it.
                    failed => story::Branching::DownloadFailed {
                        how,
                        status: failed.map(str::to_string),
                    },
                }));
            }
            Effect::CheckoutBranch { repo, name } => {
                self.checkouts.push((repo, name.clone()));
                let left = std::mem::replace(&mut self.on_branch, name);
                self.send_now(Event::CheckedOut { left });
            }
            Effect::ResolveStory {
                repo: _,
                dir,
                explicit,
                force,
            } => {
                let outcome = self.resolution(&dir, explicit, force);
                let (state, effects) = update(&self.state, Event::StoryResolved(outcome));
                self.state = state;
                self.apply(effects);
            }
            // The world plays the edge: the folder's newest artifact, and
            // git's answer about the range its name is written for.
            Effect::ReadStories { dir, repo: _ } => {
                if let Some((path, contents)) = self
                    .files
                    .iter()
                    .find(|(candidate, _)| candidate.starts_with(&dir))
                {
                    let resolves = |revision: &str| !self.unresolvable.contains(revision);
                    let range = story::range_status(path, resolves);
                    let contents = contents.clone();
                    let (state, effects) =
                        update(&self.state, Event::StoryArtifact { contents, range });
                    self.state = state;
                    self.apply(effects);
                }
            }
            other => unreachable!("no group applies {other:?}"),
        }
    }

    /// The world plays git: which range a `:story` resolves to, and whether it
    /// has been authored already.
    fn resolution(&self, dir: &Path, explicit: Option<String>, force: bool) -> story::Resolution {
        let dirty = self
            .state
            .repo
            .as_ref()
            .is_some_and(|files| !files.is_empty());
        let is_explicit = explicit.is_some();
        let spelling = match explicit {
            Some(range) => self.resolved_oids.contains_key(&range).then_some(range),
            None if dirty => Some("HEAD..worktree".to_string()),
            None => story::default_branch(
                self.origin_head.as_deref(),
                self.upstream_branch.as_deref(),
                self.default_branch_config.as_deref(),
                |name| self.probes.contains(name),
            )
            .map(|branch| story::range(&branch, "HEAD")),
        };
        match spelling {
            None if is_explicit => story::Resolution::BadRange,
            None => story::Resolution::NoDefaultBranch,
            Some(spelling) => {
                let out = self
                    .resolved_oids
                    .get(&spelling)
                    .map(|(base, head)| story::artifact_path(dir, base, head))
                    .unwrap_or_default();
                let authored = self.files.contains_key(Path::new(&out));
                story::decide(force, authored, spelling, out)
            }
        }
    }

    /// The project's files as they stand, by relative name — the working tree
    /// a snapshot is taken of and a revert is measured against.
    fn tree(&self) -> BTreeMap<String, String> {
        self.project.iter().cloned().collect()
    }

    /// What the disk model holds for a path, whichever side of it a Scenario
    /// wrote — the tree it listed, or the contents it spelled out.
    fn on_disk(&self, path: &Path) -> String {
        let relative = path
            .strip_prefix(&self.state.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        // Never a silent empty file: a read the disk model cannot answer would
        // sync as nothing at all, and the assertions about what the server was
        // told would pass on it without anybody having written the file.
        self.files
            .get(path)
            .cloned()
            .or_else(|| {
                self.project
                    .iter()
                    .find(|(name, _)| *name == relative)
                    .map(|(_, held)| held.clone())
            })
            .unwrap_or_else(|| panic!("no scenario said what {relative} holds"))
    }

    fn write_tree(&mut self, file: &str, contents: &str) {
        match self.project.iter_mut().find(|(name, _)| name == file) {
            Some((_, held)) => *held = contents.to_string(),
            None => self.project.push((file.to_string(), contents.to_string())),
        }
    }

    fn trigger(&mut self, action: &str) {
        self.send(Event::Trigger(parse_action(action), self.target.clone()));
    }
}

fn parse_action(name: &str) -> Action {
    match name {
        "go here" => Action::GoHere,
        "new file" => Action::NewFile,
        "new directory" => Action::NewDirectory,
        "delete" => Action::Delete,
        "copy path" => Action::CopyPath,
        "back to project root" => Action::BackToRoot,
        other => panic!("unknown tree action {other:?}"),
    }
}

#[given(expr = "the workspace root is {string}")]
fn workspace_root(world: &mut VardeWorld, root: String) {
    world.state.root = PathBuf::from(&root);
    world.startup.root = PathBuf::from(root);
}

#[given(expr = "the project has no {string} folder")]
fn project_lacks_folder(world: &mut VardeWorld, name: String) {
    world.dirs.remove(&world.startup.root.join(name));
}

#[then(expr = "the project has a {string} folder")]
fn project_has_folder(world: &mut VardeWorld, name: String) {
    assert!(world.dirs.contains(&world.startup.root.join(name)));
}

#[given(expr = "the project has a {string}")]
fn project_has_file(world: &mut VardeWorld, name: String) {
    world
        .files
        .insert(world.startup.root.join(name), "untouched".to_string());
}

#[then(expr = "the project {string} is unchanged")]
fn project_file_unchanged(world: &mut VardeWorld, name: String) {
    let path = world.startup.root.join(&name);
    assert!(
        !world.wrote.contains(&path),
        "{name} was written to: {:?}",
        world.wrote
    );
}

#[given(expr = "the project {string} records the last view as {string}")]
fn state_records_view(world: &mut VardeWorld, path: String, view: String) {
    assert_eq!(path, ".varde/state.json");
    world.startup.state_json = Some(format!("{{\"last_view\": \"{view}\"}}"));
}

#[given("the global config is:")]
fn global_config(world: &mut VardeWorld, step: &Step) {
    world.startup.global_config = Some(step.docstring().expect("docstring").trim().to_string());
}

#[given("the project config is:")]
fn project_config(world: &mut VardeWorld, step: &Step) {
    world.startup.project_config = Some(step.docstring().expect("docstring").trim().to_string());
}

#[given(expr = "the global config is empty")]
fn global_config_empty(world: &mut VardeWorld) {
    world.startup.global_config = Some(String::new());
}

#[given(expr = "there is no global config")]
fn no_global_config(world: &mut VardeWorld) {
    world.startup.global_config = None;
}

#[given(expr = "the project has no config file")]
fn no_project_config(world: &mut VardeWorld) {
    world.startup.project_config = None;
}

// ---- Staying up to date: the checkout's Version against the Running version ----

#[given(expr = "Varde's checkout is at {string}")]
fn checkout_at(world: &mut VardeWorld, path: String) {
    world.startup.checkout = Some(PathBuf::from(path));
}

#[given(expr = "the Running version is {string}")]
fn running_version(world: &mut VardeWorld, version: String) {
    world.startup.running_version = version;
}

#[given("the checkout manifest is:")]
fn checkout_manifest(world: &mut VardeWorld, step: &Step) {
    world.startup.checkout_manifest = Some(step.docstring().expect("docstring").trim().to_string());
}

/// The edge still found a directory above the binary — there is just nothing in
/// it, which is the case the core has to rule out.
#[given(expr = "there is no checkout manifest")]
fn no_checkout_manifest(world: &mut VardeWorld) {
    world.startup.checkout_manifest = None;
}

#[then(expr = "an Update to {string} is available")]
fn update_offered_to(world: &mut VardeWorld, version: String) {
    assert_eq!(world.state.update, Some(version));
}

#[given(expr = "Varde was built for {string} on {string}")]
fn built_for_platform(world: &mut VardeWorld, os: String, arch: String) {
    world.startup.os = os;
    world.startup.arch = arch;
}

#[then(expr = "Varde asks for the latest Release")]
fn asks_for_release(world: &mut VardeWorld) {
    assert!(
        world.startup_effects.contains(&Effect::CheckRelease {
            url: startup::RELEASE_URL.to_string()
        }),
        "starting asked for: {:?}",
        world.startup_effects
    );
}

#[then(expr = "Varde does not ask for a Release")]
fn asks_for_no_release(world: &mut VardeWorld) {
    assert!(
        !world
            .startup_effects
            .iter()
            .any(|effect| matches!(effect, Effect::CheckRelease { .. })),
        "starting asked for: {:?}",
        world.startup_effects
    );
}

#[then(expr = "the story sets were read from {string}")]
fn story_sets_read_at_start(world: &mut VardeWorld, dir: String) {
    assert!(
        world.startup_effects.contains(&Effect::ReadStories {
            dir: world.startup.root.join(dir),
            repo: world.startup.root.clone(),
        }),
        "starting asked for: {:?}",
        world.startup_effects
    );
}

#[then(expr = "no story sets were read")]
fn no_story_sets_read_at_start(world: &mut VardeWorld) {
    assert!(
        !world
            .startup_effects
            .iter()
            .any(|effect| matches!(effect, Effect::ReadStories { .. })),
        "starting asked for: {:?}",
        world.startup_effects
    );
}

#[when("the latest Release answers:")]
fn release_answers(world: &mut VardeWorld, step: &Step) {
    let body = step.docstring().expect("docstring").to_string();
    world.send(Event::ReleaseAnswered(Some(body)));
}

/// Through the event the edge sends, so what is remembered is what the core
/// made of an answer rather than a Release the scenario wrote into the state.
#[given(expr = "a newer Release for this platform has been found")]
fn newer_release_found(world: &mut VardeWorld) {
    let asset = format!("varde-{}-{}", world.startup.os, world.startup.arch);
    world.send(Event::ReleaseAnswered(Some(format!(
        r#"{{"tag_name": "v9.0.0", "assets": [
            {{"name": "{asset}", "browser_download_url": "https://example.test/{asset}"}},
            {{"name": "SHA256SUMS", "browser_download_url": "https://example.test/SHA256SUMS"}}
        ]}}"#
    ))));
    assert!(
        world.state.release.is_some(),
        "the Release was not remembered"
    );
}

#[then(expr = "Varde fetches the remembered Release")]
fn fetches_release(world: &mut VardeWorld) {
    let release = world.state.release.clone().expect("a remembered Release");
    assert_eq!(
        world.replacing,
        vec![Effect::ReplaceBinary {
            asset: release.asset,
            checksums: release.checksums,
        }]
    );
}

#[then(expr = "Varde does not fetch a Release")]
fn fetches_no_release(world: &mut VardeWorld) {
    assert!(world.replacing.is_empty(), "fetched: {:?}", world.replacing);
}

#[given(expr = "the binary has been replaced")]
#[when(expr = "the binary has been replaced")]
fn binary_replaced(world: &mut VardeWorld) {
    world.send(Event::BinaryReplaced(Ok(())));
}

#[when(expr = "replacing the binary fails at the {word} step")]
fn replacing_fails(world: &mut VardeWorld, at: String) {
    let failed = match at.as_str() {
        "download" => ReplaceFailed::Download,
        "no-asset" => ReplaceFailed::NoAsset,
        "checksum" => ReplaceFailed::Checksum,
        "replace" => ReplaceFailed::Replace,
        other => panic!("unknown step {other}"),
    };
    world.send(Event::BinaryReplaced(Err(failed)));
}

#[then(expr = "the binary is known to be replaced")]
fn known_replaced(world: &mut VardeWorld) {
    assert!(world.state.replaced);
}

#[then(expr = "the binary is not known to be replaced")]
fn not_known_replaced(world: &mut VardeWorld) {
    assert!(!world.state.replaced);
}

#[then(expr = "Varde relaunches")]
fn relaunches(world: &mut VardeWorld) {
    assert!(world.relaunched);
}

#[then(expr = "Varde does not relaunch")]
fn does_not_relaunch(world: &mut VardeWorld) {
    assert!(!world.relaunched);
}

#[when(expr = "the request for the latest Release fails")]
fn release_request_fails(world: &mut VardeWorld) {
    world.send(Event::ReleaseAnswered(None));
}

#[then(
    expr = "the remembered Release is {string} with the Asset {string} and the checksums {string}"
)]
fn release_remembered(world: &mut VardeWorld, version: String, asset: String, checksums: String) {
    assert_eq!(
        world.state.release,
        Some(startup::Release {
            version,
            asset,
            checksums
        })
    );
}

#[then(expr = "no Release is remembered")]
fn no_release_remembered(world: &mut VardeWorld) {
    assert_eq!(world.state.release, None);
}

#[then(expr = "no Update is available")]
fn no_update_offered(world: &mut VardeWorld) {
    assert_eq!(world.state.update, None, "an Update was offered");
}

#[then(expr = "Varde's checkout is known to be {string}")]
fn checkout_known(world: &mut VardeWorld, path: String) {
    assert_eq!(world.state.checkout, Some(PathBuf::from(path)));
}

#[then(expr = "Varde's checkout is not known")]
fn checkout_not_known(world: &mut VardeWorld) {
    assert_eq!(world.state.checkout, None);
}

#[then(expr = "Varde started")]
fn varde_started(world: &mut VardeWorld) {
    assert!(world.error.is_none(), "Varde refused to start");
}

#[then(expr = "no notice was raised")]
fn no_notice(world: &mut VardeWorld) {
    assert!(world.notices.is_empty(), "notices: {:?}", world.notices);
}

#[when(expr = "I ask Varde to update from the command line")]
fn ask_to_update(world: &mut VardeWorld) {
    world.send(Event::Rebuild);
}

#[when(expr = "I ask Varde for help from the command line")]
fn ask_for_help(world: &mut VardeWorld) {
    world.send(Event::ToggleCheatsheet);
}

#[given(expr = "the key reminder is hidden")]
fn reminder_hidden(world: &mut VardeWorld) {
    world.state.cheatsheet = false;
}

#[given(expr = "the key reminder is shown")]
fn reminder_up(world: &mut VardeWorld) {
    world.state.cheatsheet = true;
}

#[given(expr = "the project {string} records the key reminder as shown")]
fn state_records_reminder(world: &mut VardeWorld, path: String) {
    assert_eq!(path, ".varde/state.json");
    world.startup.state_json = Some("{\"cheatsheet\": true}".to_string());
}

#[then(expr = "the key reminder is shown")]
fn reminder_shown(world: &mut VardeWorld) {
    assert!(world.state.cheatsheet);
}

#[then(expr = "the key reminder is not shown")]
fn reminder_not_shown(world: &mut VardeWorld) {
    assert!(!world.state.cheatsheet);
}

#[then(expr = "the saved project state does not mention the key reminder")]
fn reminder_not_remembered(world: &mut VardeWorld) {
    let saved = world
        .startup
        .state_json
        .as_deref()
        .expect("state was saved");
    assert!(!saved.contains("\"cheatsheet\""), "state was: {saved}");
}

#[when(expr = "I dim the editor from the command line")]
fn dim_editor(world: &mut VardeWorld) {
    world.send(Event::ToggleField);
}

#[given(expr = "the editor field is off")]
fn field_off(world: &mut VardeWorld) {
    world.state.editor_field = false;
}

#[given(expr = "the project {string} records the editor field as off")]
fn state_records_field(world: &mut VardeWorld, path: String) {
    assert_eq!(path, ".varde/state.json");
    world.startup.state_json = Some("{\"editor_field\": false}".to_string());
}

#[then(expr = "the editor field is shown")]
fn field_shown(world: &mut VardeWorld) {
    assert!(world.state.editor_field);
}

#[then(expr = "the editor field is not shown")]
fn field_hidden(world: &mut VardeWorld) {
    assert!(!world.state.editor_field);
}

#[then(expr = "the remembered editor field is {string}")]
fn field_remembered(world: &mut VardeWorld, state: String) {
    let saved = world
        .startup
        .state_json
        .as_deref()
        .expect("state was saved");
    let shown = match state.as_str() {
        "off" => "false",
        "on" => "true",
        other => panic!("unknown field state {other}"),
    };
    assert!(
        saved.contains(&format!("\"editor_field\":{shown}")),
        "state was: {saved}"
    );
}

#[then(expr = "the user is told there is nothing to update from")]
fn told_nothing_to_update(world: &mut VardeWorld) {
    assert_eq!(world.notices, vec!["nothing-to-update".to_string()]);
}

/// The write ledger, not the disk model, and minus the two writes no gesture
/// makes: starting seeds the project's config file (R9.7) and the global one,
/// so a scenario that starts carries writes it never asked for. Nothing but
/// starting ever writes those paths, so leaving them out costs the promise
/// nothing.
#[then(expr = "no file was written")]
fn nothing_written(world: &mut VardeWorld) {
    let seeds = [
        world.startup.root.join(".varde/config.toml"),
        world.startup.varde_home.join(startup::CONFIG_FILE),
    ];
    let wrote: Vec<&PathBuf> = world
        .wrote
        .iter()
        .filter(|path| !seeds.contains(path))
        .collect();
    assert!(wrote.is_empty(), "written: {wrote:?}");
}

#[given(expr = "Varde started in the project")]
#[given(expr = "Varde starts in the project")]
#[when(expr = "Varde starts in the project")]
fn varde_starts(world: &mut VardeWorld) {
    // A scenario that says nothing about the OS still needs one, since the
    // install command a row offers is looked up under it. A fixed value rather
    // than this machine's: a suite whose rows read differently on Linux is a
    // suite that fails somewhere nobody is looking. Scenarios that care say
    // "Varde was built for" themselves, which runs before this.
    if world.startup.os.is_empty() {
        world.startup.os = "macos".to_string();
    }
    world.startup.varde_home = Path::new(HOME).join(varde::VARDE_DIR);
    // The world plays git, and the edge asks it before starting.
    world.startup.repo = world.state.repo.clone();
    match startup::start(&world.startup) {
        Ok((state, config, effects)) => {
            world.state = state;
            world.config = Some(config);
            world.startup_effects = effects.clone();
            world.apply(effects);
        }
        Err(error) => world.error = Some(error),
    }
}

/// The seed is a write like any other, held to its contents as well as its
/// path: "a file appeared" would pass on an empty one, and the file's whole
/// job is the keys it names.
#[then(expr = "the project {string} was seeded")]
fn project_file_was_seeded(world: &mut VardeWorld, name: String) {
    let path = world.startup.root.join(&name);
    assert!(world.wrote.contains(&path), "written: {:?}", world.wrote);
    assert_eq!(
        world.files.get(&path).map(String::as_str),
        Some(startup::SEEDED_CONFIG)
    );
}

#[then(expr = "the global config was seeded from the template")]
fn global_file_was_seeded(world: &mut VardeWorld) {
    let path = world.startup.varde_home.join(startup::CONFIG_FILE);
    assert!(world.wrote.contains(&path), "written: {:?}", world.wrote);
    assert_eq!(world.files.get(&path), Some(&startup::template()));
}

#[then(expr = "the global config is unchanged")]
fn global_file_unchanged(world: &mut VardeWorld) {
    let path = world.startup.varde_home.join(startup::CONFIG_FILE);
    assert!(!world.wrote.contains(&path), "written: {:?}", world.wrote);
}

/// The seeded text itself, read back off the modelled disk and handed to a
/// second start as the project's layer. Feeding the constant in directly would
/// prove the constant harmless; this proves the file Varde actually laid down
/// is, which is the promise "every key is commented out" is really making.
#[when(expr = "Varde starts again with the config file it seeded")]
fn starts_again_with_the_seeded_config(world: &mut VardeWorld) {
    let path = world.startup.root.join(".varde/config.toml");
    let seeded = world.files.get(&path).cloned().expect("a seeded config");
    world.startup.project_config = Some(seeded);
    varde_starts(world);
}

#[then(expr = "the effective setting {string} is {string}")]
fn effective_setting(world: &mut VardeWorld, key: String, expected: String) {
    let config = world.config.as_ref().expect("Varde started");
    assert_eq!(config.get(&key).as_deref(), Some(expected.as_str()));
}

// ---- F31: which command serves which language ----

fn servers(world: &VardeWorld) -> BTreeMap<String, Server> {
    world.config.as_ref().expect("Varde started").servers()
}

#[then(expr = "a language server is configured for {string}")]
#[then(expr = "the configured languages include {string}")]
fn language_is_configured(world: &mut VardeWorld, language: String) {
    let configured = servers(world);
    assert!(
        configured.contains_key(&language),
        "no server for {language}; configured: {:?}",
        configured.keys().collect::<Vec<_>>()
    );
}

#[then(expr = "the configured languages do not include {string}")]
#[then(expr = "there is no language server configured for {string}")]
fn language_is_not_configured(world: &mut VardeWorld, language: String) {
    assert_eq!(
        servers(world).get(&language),
        None,
        "a server is configured for {language}"
    );
}

#[then(expr = "the configured server command for {string} is {string}")]
fn server_command_is(world: &mut VardeWorld, language: String, expected: String) {
    let server = servers(world)
        .remove(&language)
        .unwrap_or_else(|| panic!("no server for {language}"));
    assert_eq!(server.command, expected);
}

#[then(expr = "the configured server arguments for {string} are:")]
fn server_arguments_are(world: &mut VardeWorld, language: String, step: &Step) {
    let server = servers(world)
        .remove(&language)
        .unwrap_or_else(|| panic!("no server for {language}"));
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(server.args, expected);
}

/// Two promises in one step: no spawn was asked for anywhere, and naming a
/// server *configures nothing else* either — the allowlist is the two effects
/// starting already returns, and the restored view it arrives in.
#[then(expr = "no language server was started")]
fn no_language_server_started(world: &mut VardeWorld) {
    assert!(
        world.lsp_started.is_empty(),
        "a server was started: {:?}",
        world.lsp_started
    );
    let extra: Vec<&Effect> = world
        .startup_effects
        .iter()
        .filter(|effect| {
            !matches!(
                effect,
                Effect::EnsureDir(_)
                    | Effect::AnalyseRisk { .. }
                    | Effect::CheckRelease { .. }
                    | Effect::RenderView(_)
            ) && !matches!(effect, Effect::DeleteDir(path) if is_scratch(world, path))
                && !matches!(effect, Effect::WriteFile { contents, .. }
                    if *contents == startup::SEEDED_CONFIG || *contents == startup::template())
        })
        .collect();
    assert!(
        extra.is_empty(),
        "starting did more than it used to: {extra:?}"
    );
}

#[then(expr = "Varde refuses to start")]
fn refuses_to_start(world: &mut VardeWorld) {
    assert!(world.error.is_some(), "Varde started");
}

fn config_error(world: &VardeWorld) -> &varde::startup::ConfigError {
    match world.error.as_ref().expect("an error") {
        StartupError::Config(error) => error,
        other => panic!("expected a config error, got {other:?}"),
    }
}

#[then(expr = "the error names the file {string}")]
fn error_names_file(world: &mut VardeWorld, file: String) {
    assert_eq!(config_error(world).file, file);
}

#[then(expr = "the error names line {int}")]
fn error_names_line(world: &mut VardeWorld, line: usize) {
    assert_eq!(config_error(world).line, line);
}

/// Which of the three faults it was, as a name rather than as the sentence the
/// edge prints: the wording is pinned by a unit test beside `Display`, and a
/// suite that failed on rewording would teach people to ignore it.
#[then(expr = "the fault is {string}")]
fn fault_is(world: &mut VardeWorld, expected: String) {
    let actual = match &config_error(world).fault {
        startup::ConfigFault::NotToml => "not-toml",
        startup::ConfigFault::WrongType(_) => "wrong-type",
        startup::ConfigFault::Incomplete { .. } => "incomplete",
        startup::ConfigFault::ClaimedTwice { .. } => "extension-claimed-twice",
        startup::ConfigFault::Unreadable => "unreadable",
    };
    assert_eq!(actual, expected);
}

#[then(expr = "the error names the rows {string} and {string}")]
fn error_names_rows(world: &mut VardeWorld, first: String, second: String) {
    match &config_error(world).fault {
        startup::ConfigFault::ClaimedTwice { rows, .. } => assert_eq!(rows, &[first, second]),
        other => panic!("expected an extension claimed twice, got {other:?}"),
    }
}

#[then(expr = "the reason is {string}")]
fn reason_is(world: &mut VardeWorld, reason: String) {
    match world.error.as_ref().expect("an error") {
        StartupError::Path(actual) => assert_eq!(*actual, reason),
        other => panic!("expected a path error, got {other:?}"),
    }
}

// ---- F1: opening on a folder ----

#[given(expr = "the folder {string} exists")]
#[given(expr = "the folder {string} exists and is empty")]
fn folder_exists(world: &mut VardeWorld, path: String) {
    world.disk.insert(PathBuf::from(path), Vec::new());
}

#[given(expr = "{string} does not exist")]
fn path_missing(world: &mut VardeWorld, _path: String) {
    world.startup.path_status = PathStatus::Missing;
}

#[given(expr = "{string} is a file")]
fn path_is_file(world: &mut VardeWorld, _path: String) {
    world.startup.path_status = PathStatus::NotAFolder;
}

#[given(expr = "the folder {string} cannot be read")]
fn path_unreadable(world: &mut VardeWorld, _path: String) {
    world.startup.path_status = PathStatus::Unreadable;
}

#[when(expr = "Varde opens {string}")]
fn varde_opens(world: &mut VardeWorld, path: String) {
    world.startup.root = PathBuf::from(&path);
    let entries = world
        .disk
        .get(&world.startup.root)
        .cloned()
        .unwrap_or_default();
    varde_starts(world);
    if world.error.is_none() {
        world.state.contents.insert(PathBuf::from(path), entries);
    }
}

/// On disk before Varde starts, so what the tree shows is what the folder held
/// rather than what a step put there afterwards. The idiom `the workspace
/// folder contains:` already uses, one file at a time.
#[given(expr = "the workspace folder holds the file {string}")]
fn workspace_folder_holds_file(world: &mut VardeWorld, name: String) {
    let root = world.state.root.clone();
    put_on_disk(
        world,
        root,
        Entry {
            name,
            is_dir: false,
        },
    );
}

/// Where a Bare workspace's Sidecar is, as the edge derived it from the folder
/// and the process id. Handed in rather than built here: how the two parts are
/// spelled into one directory name is edge work with no scenario, and what a
/// scenario is about is that the library wrote *there* and not into the folder.
const SIDECAR: &str = "/home/me/.varde/paths/%home%me%projects%theirs-4242";

/// The user's own directory, which `~` in a scenario names and `varde_home`
/// below is under. The edge reads both; a scenario states them, so a path
/// outside every workspace is a path a step can spell.
const HOME: &str = "/home/me";

#[given(expr = "Varde started with no folder in {string}")]
#[when(expr = "Varde starts with no folder in {string}")]
fn varde_starts_bare(world: &mut VardeWorld, path: String) {
    world.startup.sidecar = Some(PathBuf::from(SIDECAR));
    varde_opens(world, path);
}

/// The load-bearing absence: a folder Varde was not given is a folder Varde
/// leaves alone. Every directory, not `.varde` by name — a site that missed the
/// accessor would create some other one and this would still catch it.
#[then(expr = "no directory was created in the folder")]
fn no_directory_in_folder(world: &mut VardeWorld) {
    let inside: Vec<&PathBuf> = world
        .dirs
        .iter()
        .filter(|dir| dir.starts_with(&world.startup.root))
        .collect();
    assert!(inside.is_empty(), "created: {inside:?}");
}

/// The Sidecar is deleted at exit, so a review written into it is a review
/// lost — and the folder is the one place a Bare workspace may not write at
/// all. Both absences are held against every write, wherever it went.
#[then(expr = "no file was written into the folder Varde was started in")]
fn nothing_written_into_folder(world: &mut VardeWorld) {
    let inside: Vec<&PathBuf> = world
        .wrote
        .iter()
        .filter(|path| path.starts_with(&world.startup.root))
        .collect();
    assert!(inside.is_empty(), "written: {inside:?}");
}

#[then(expr = "no file was written into the Sidecar")]
fn nothing_written_into_sidecar(world: &mut VardeWorld) {
    let inside: Vec<&PathBuf> = world
        .wrote
        .iter()
        .filter(|path| path.starts_with(SIDECAR))
        .collect();
    assert!(inside.is_empty(), "written: {inside:?}");
}

/// Held against the folder the scenario named, never against the workspace
/// root the library came up with: a Bare workspace that made its Sidecar the
/// workspace would move both sides of a root-relative assertion at once and
/// pass while editing a copy of the file nobody can find.
#[then(expr = "{string} was written into the folder Varde was started in")]
fn written_into_folder(world: &mut VardeWorld, name: String) {
    let path = world.startup.root.join(name);
    assert!(world.wrote.contains(&path), "written: {:?}", world.wrote);
}

/// The folder is the obvious place a seed could land, and the Sidecar is the
/// plausible-looking one: writing the file *somewhere* looks like the promise
/// kept while the key is still in a directory deleted at exit. Held against
/// every write of a config file but the global one, wherever it went.
#[then(expr = "no config file was seeded in the workspace")]
fn no_config_seeded_in_workspace(world: &mut VardeWorld) {
    let global = world.startup.varde_home.join(startup::CONFIG_FILE);
    let seeded: Vec<&PathBuf> = world
        .wrote
        .iter()
        .filter(|path| path.file_name() == Some(startup::CONFIG_FILE.as_ref()) && **path != global)
        .collect();
    assert!(seeded.is_empty(), "seeded: {seeded:?}");
}

/// Quitting a Bare workspace leaves nothing behind. Held against the Sidecar
/// the edge handed in, so an implementation that deleted some other directory
/// of its own choosing fails here.
#[then(expr = "the Sidecar was deleted")]
fn sidecar_was_deleted(world: &mut VardeWorld) {
    assert!(
        world.dirs_deleted.contains(&PathBuf::from(SIDECAR)),
        "deleted: {:?}",
        world.dirs_deleted
    );
}

/// The absence a project workspace is owed: the rule is that everything under
/// Varde's global directory can go, and a project's `.varde/` is not under it.
#[then(expr = "no directory was deleted")]
fn no_directory_deleted(world: &mut VardeWorld) {
    let deleted: Vec<&PathBuf> = world
        .dirs_deleted
        .iter()
        .filter(|path| !is_scratch(world, path))
        .collect();
    assert!(deleted.is_empty(), "deleted: {deleted:?}");
}

/// Varde's own scratch, which every start sweeps and remakes before anything
/// can write into it (ADR 0014). Neither a workspace directory nor a Sidecar,
/// so the absences those two promises are about are not about this one — and
/// naming it here rather than dropping the effect keeps the sweep a value the
/// suite can still see.
fn is_scratch(world: &VardeWorld, path: &Path) -> bool {
    path == varde::tmp_dir(&world.startup.varde_home)
}

/// A Bare workspace forgets everything, so the write on the way out is not
/// made rather than made into a directory that is about to go.
#[then(expr = "no state was saved")]
fn no_state_saved(world: &mut VardeWorld) {
    assert_eq!(world.startup.state_json, None);
}

#[then(expr = "Varde's own directory is the Sidecar")]
fn varde_dir_is_sidecar(world: &mut VardeWorld) {
    assert!(
        world.dirs.contains(&PathBuf::from(SIDECAR)),
        "created: {:?}",
        world.dirs
    );
}

#[then(expr = "the workspace root is {string}")]
fn workspace_root_should_be(world: &mut VardeWorld, path: String) {
    assert_eq!(world.state.root, PathBuf::from(path));
}

#[then(expr = "the workspace title is {string}")]
fn workspace_title_should_be(world: &mut VardeWorld, title: String) {
    assert_eq!(world.state.title(), title);
}

#[then(expr = "the file tree is empty")]
fn tree_is_empty(world: &mut VardeWorld) {
    assert!(tree::rows(&world.state).is_empty());
}

// ---- F2 / F5: the tree ----

/// A scenario's path, against the workspace root — unless it names the user's
/// own directory, which nothing in a workspace can reach. `~` is the only way
/// a scenario can say "outside every workspace" and still be read against the
/// same home the edge hands in.
fn abs(world: &VardeWorld, path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => Path::new(HOME).join(rest),
        None => world.state.root.join(path),
    }
}

/// Where the pointer sits when it is over a given line and column of a pane's
/// text — the inverse of what `mouse` does with a screen position.
fn pointer_at(
    state: &State,
    panes: &layout::Layout,
    pane: Pane,
    (line, column): (usize, usize),
) -> (u16, u16) {
    let (area, gutter, scroll) = match pane {
        Pane::Editor => (panes.editor, varde::gutter(state), state.editor_scroll),
        Pane::Tree => (panes.tree, 0, state.tree_scroll),
        Pane::Ai => (panes.ai, 0, 0),
        Pane::Output => (panes.output, 0, 0),
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics => (panes.corner, 0, 0),
        Pane::Terminal | Pane::Variables => (panes.terminal, 0, 0),
        // The window itself, not the Snippet's own rectangle: the `+ 1` below
        // is the border every other pane's rectangle carries.
        Pane::Evaluator => (panes.evaluator, 0, 0),
    };
    (
        area.x + 1 + gutter + (column - 1) as u16,
        area.y + 1 + (line - 1 - scroll) as u16,
    )
}

fn put_on_disk(world: &mut VardeWorld, folder: PathBuf, entry: Entry) {
    let entries = world.disk.entry(folder).or_default();
    if !entries.iter().any(|e| e.name == entry.name) {
        entries.push(entry);
    }
}

#[given("the workspace folder contains:")]
fn workspace_contains(world: &mut VardeWorld, step: &Step) {
    let root = world.state.root.clone();
    let entries: Vec<Entry> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| Entry {
            name: row[0].clone(),
            is_dir: row[1] == "directory",
        })
        .collect();
    world.disk.insert(root.clone(), entries.clone());
    world.state.contents.insert(root, entries);
}

#[given(expr = "{string} contains {string}")]
fn folder_contains(world: &mut VardeWorld, folder: String, name: String) {
    // Saying what a folder contains also says the folder is there.
    let root = world.state.root.clone();
    put_on_disk(
        world,
        root,
        Entry {
            name: folder.clone(),
            is_dir: true,
        },
    );
    let path = abs(world, &folder);
    put_on_disk(
        world,
        path,
        Entry {
            name,
            is_dir: false,
        },
    );
}

/// Every folder on the way is on disk as a folder, and none of them is
/// expanded: the tree as it stands before anyone has walked into it.
#[given(expr = "{string} is on disk under collapsed folders")]
fn on_disk_under_collapsed(world: &mut VardeWorld, path: String) {
    let root = world.state.root.clone();
    let parts: Vec<String> = path.split('/').map(str::to_string).collect();
    let mut folder = root.clone();
    for (index, part) in parts.iter().enumerate() {
        let is_dir = index + 1 < parts.len();
        put_on_disk(
            world,
            folder.clone(),
            Entry {
                name: part.clone(),
                is_dir,
            },
        );
        folder = folder.join(part);
    }
    let entries = world.disk.get(&root).cloned().unwrap_or_default();
    world.state.contents.insert(root, entries);
}

#[given(expr = "the project has no recorded tree state")]
fn no_tree_state(world: &mut VardeWorld) {
    world.state.expanded.clear();
}

#[given(expr = "the project state records {string} as expanded")]
fn state_records_expanded(world: &mut VardeWorld, folder: String) {
    world.startup.state_json = Some(format!("{{\"expanded\": [\"{folder}\"]}}"));
    let root = world.state.root.clone();
    let contents = world.state.contents.clone();
    varde_starts(world);
    world.state.root = root;
    world.state.contents = contents;
}

#[given(expr = "the file tree shows the collapsed folder {string}")]
#[given(expr = "the folder {string} is collapsed")]
fn tree_shows_collapsed(world: &mut VardeWorld, folder: String) {
    let root = world.state.root.clone();
    let path = abs(world, &folder);
    put_on_disk(
        world,
        root.clone(),
        Entry {
            name: folder,
            is_dir: true,
        },
    );
    let entries = world.disk.get(&root).cloned().unwrap_or_default();
    world.state.contents.insert(root, entries);
    world.state.expanded.remove(&path);
}

#[given(expr = "the folder {string} is expanded")]
fn folder_is_expanded(world: &mut VardeWorld, folder: String) {
    tree_shows_collapsed(world, folder.clone());
    expand(world, folder);
}

#[when(expr = "I expand {string}")]
fn expand(world: &mut VardeWorld, folder: String) {
    let path = abs(world, &folder);
    let entries = world.disk.get(&path).cloned().unwrap_or_default();
    world.send(Event::Expand { path, entries });
}

#[given(expr = "I collapse the tree")]
#[when(expr = "I collapse the tree")]
fn collapse_the_tree(world: &mut VardeWorld) {
    world.send(Event::CollapseTree);
}

#[when(expr = "the file tree is rendered")]
fn render_tree(world: &mut VardeWorld) {
    // What the edge does on startup: read the root, plus every folder that was
    // restored as expanded. Nothing else is read — the tree is lazy.
    let mut folders = vec![world.state.root.clone()];
    folders.extend(world.state.expanded.iter().cloned());
    for folder in folders {
        if let Some(entries) = world.disk.get(&folder).cloned() {
            world.state.contents.insert(folder, entries);
        }
    }
}

#[given(expr = "git ignores {string}")]
fn git_ignores(world: &mut VardeWorld, name: String) {
    let path = abs(world, &name);
    world.state.ignored.insert(path);
}

#[then("the file tree lists, in order:")]
fn tree_lists_in_order(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let actual: Vec<String> = tree::rows(&world.state)
        .iter()
        .map(|row| row.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(actual, expected);
}

fn row_for(world: &VardeWorld, path: &str) -> Option<tree::Row> {
    let wanted = world.state.root.join(path);
    tree::rows(&world.state)
        .into_iter()
        .find(|r| r.path == wanted)
}

#[given(expr = "the file tree shows {string}")]
fn tree_shows_given(world: &mut VardeWorld, path: String) {
    let (folder, name) = path.rsplit_once('/').expect("a nested path");
    let folder = folder.to_string();
    folder_contains(world, folder.clone(), name.to_string());
    folder_is_expanded(world, folder);
}

#[then(expr = "the file tree shows {string} as a folder")]
fn tree_shows_as_folder(world: &mut VardeWorld, path: String) {
    assert!(
        row_for(world, &path).expect("a row").is_dir,
        "{path} is listed as a file"
    );
}

#[then(expr = "the file tree shows {string}")]
fn tree_shows(world: &mut VardeWorld, path: String) {
    assert!(row_for(world, &path).is_some(), "missing {path}");
}

#[then(expr = "the file tree does not show {string}")]
fn tree_does_not_show(world: &mut VardeWorld, path: String) {
    assert!(
        row_for(world, &path).is_none(),
        "unexpectedly showing {path}"
    );
}

#[then(expr = "{string} is expanded")]
fn is_expanded(world: &mut VardeWorld, folder: String) {
    assert!(world.state.expanded.contains(&abs(world, &folder)));
}

#[then(expr = "{string} is collapsed")]
fn is_collapsed(world: &mut VardeWorld, folder: String) {
    assert!(!world.state.expanded.contains(&abs(world, &folder)));
}

#[then(expr = "{string} is dimmed")]
fn is_dimmed(world: &mut VardeWorld, path: String) {
    assert!(row_for(world, &path).expect("a row").dimmed);
}

#[then(expr = "{string} is not dimmed")]
fn is_not_dimmed(world: &mut VardeWorld, path: String) {
    assert!(!row_for(world, &path).expect("a row").dimmed);
}

#[given(expr = "{string} is created on disk")]
#[when(expr = "{string} is created on disk")]
fn created_on_disk(world: &mut VardeWorld, path: String) {
    appeared_on_disk(world, &path, tree::Kind::File);
}

#[given(expr = "the folder {string} is created on disk")]
#[when(expr = "the folder {string} is created on disk")]
fn folder_created_on_disk(world: &mut VardeWorld, path: String) {
    appeared_on_disk(world, &path, tree::Kind::Folder);
}

fn appeared_on_disk(world: &mut VardeWorld, path: &str, kind: tree::Kind) {
    let (folder, name) = path.rsplit_once('/').expect("a nested path");
    let folder = abs(world, folder);
    put_on_disk(
        world,
        folder,
        Entry {
            name: name.to_string(),
            is_dir: kind == tree::Kind::Folder,
        },
    );
    let absolute = abs(world, path);
    world.send(Event::FilesAppeared(vec![(absolute, kind)]));
}

#[when(expr = "{string} is deleted on disk")]
fn deleted_on_disk(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    world.send(Event::FilesRemoved(vec![absolute]));
}

// ---- F5: buffers ----

/// Opens a file the way the edge does — through the event, so whatever
/// `update` decides at open time (which shape a markdown file arrives in, for
/// one) is decided once and not a second time here. A step that built a
/// `Buffer` and inserted it was a step that could go green over a default
/// `main.rs` never applies.
fn open_buffer(world: &mut VardeWorld, path: &str, contents: &str) -> PathBuf {
    let absolute = abs(world, path);
    // What was opened is what is on disk: a jump reads the file again and the
    // buffer follows it, so a disk the scenario never described would read as
    // an empty file and blank the buffer the scenario set up.
    world
        .files
        .entry(absolute.clone())
        .or_insert_with(|| contents.to_string());
    world.send(Event::BufferOpened {
        path: absolute.clone(),
        contents: contents.to_string(),
        preview: false,
        at: None,
    });
    absolute
}

#[given(expr = "{string} is open in the editor with no unsaved edits")]
fn open_clean(world: &mut VardeWorld, path: String) {
    // Whatever the scenario put on disk, and a stand-in for a file it never
    // described — the same rule "I open" follows.
    let contents = world
        .files
        .get(&abs(world, &path))
        .cloned()
        .unwrap_or_else(|| "on disk".to_string());
    open_buffer(world, &path, &contents);
}

/// A file with nothing in it, for the Scenarios that assert on the whole of
/// what the buffer holds: a stand-in line would leave every one of them
/// asserting the stand-in as well as what was typed.
#[given(expr = "{string} is open in the editor holding nothing")]
fn open_empty(world: &mut VardeWorld, path: String) {
    open_buffer(world, &path, "");
}

#[given(expr = "{string} is open in the editor with unsaved edits")]
fn open_dirty(world: &mut VardeWorld, path: String) {
    let absolute = open_buffer(world, &path, "on disk");
    world
        .state
        .buffers
        .get_mut(&absolute)
        .expect("the buffer")
        .draft = Some("my edits".to_string());
}

/// Unsaved edits over whatever the project holds on disk: a clean buffer
/// holding text the disk does not is a stale one, and follows the disk the
/// moment anything reads the file again.
#[given(expr = "{string} is open in the editor with unsaved edits holding:")]
fn open_dirty_holding(world: &mut VardeWorld, path: String, step: &Step) {
    let draft = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let absolute = abs(world, &path);
    let disk = world
        .project
        .iter()
        .find(|(name, _)| world.state.root.join(name) == absolute)
        .map(|(_, contents)| contents.clone())
        .unwrap_or_default();
    open_buffer(world, &path, &disk);
    world
        .state
        .buffers
        .get_mut(&absolute)
        .expect("the buffer")
        .draft = Some(draft);
}

#[given(expr = "{string} is changed on disk")]
#[when(expr = "{string} is changed on disk")]
fn changed_on_disk(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    world.send(Event::FileChanged {
        path: absolute,
        contents: "changed on disk".to_string(),
    });
}

#[when(expr = "I reload {string}")]
fn reload(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    world.send(Event::Reload(absolute));
}

fn buffer<'a>(world: &'a VardeWorld, path: &str) -> &'a Buffer {
    world
        .state
        .buffers
        .get(&world.state.root.join(path))
        .expect("an open buffer")
}

#[then(expr = "the editor shows the version on disk")]
fn shows_disk_version(world: &mut VardeWorld) {
    let buffer = world.state.buffers.values().next().expect("a buffer");
    assert_eq!(buffer.shown(), buffer.disk);
}

#[then(expr = "the editor still shows the unsaved edits")]
fn shows_unsaved(world: &mut VardeWorld) {
    let buffer = world.state.buffers.values().next().expect("a buffer");
    assert_eq!(buffer.shown(), "my edits");
}

#[then(expr = "{string} is flagged as changed on disk")]
fn is_flagged(world: &mut VardeWorld, path: String) {
    assert!(buffer(world, &path).changed_on_disk);
}

#[then(expr = "{string} is not flagged as changed on disk")]
fn is_not_flagged(world: &mut VardeWorld, path: String) {
    assert!(!buffer(world, &path).changed_on_disk);
}

/// The contract between the core and the watcher at the edge: `main` adds and
/// drops watches to match `watched_folders`, so a file whose folder is not in
/// that set is a file nothing will ever report a change to. Folders, never
/// files — a watch on a file is lost when a tool replaces it by rename.
#[then(expr = "{string} is followed for changes")]
fn is_followed(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    let parent = absolute.parent().expect("a parent").to_path_buf();
    let watched = varde::watched_folders(&world.state);
    assert!(
        watched.contains(&parent),
        "{parent:?} is watched by nobody, so {path} cannot follow its file; watching {watched:?}"
    );
}

#[then(expr = "the notice is {string}")]
fn notice_is(world: &mut VardeWorld, expected: String) {
    assert!(
        world.notices.contains(&expected),
        "notices: {:?}",
        world.notices
    );
}

/// Through the real key router, so the picker's own bindings are what the
/// scenario exercises rather than the event they happen to send.
#[when(expr = "I resolve the divergence with {string}")]
fn resolve_divergence(world: &mut VardeWorld, key: String) {
    route_key(world, &key, 0);
}

#[then(expr = "the unsaved edits were written to {string}")]
fn unsaved_edits_written(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    assert_eq!(
        world.files.get(&absolute).map(String::as_str),
        Some("my edits")
    );
}

/// A path alone would not be enough: the buffer's version exists nowhere on
/// disk for the CLI to read, so a prompt that only names the file asks the AI
/// to merge against something it cannot see.
#[then(expr = "the prompt carries both versions")]
fn prompt_carries_both_versions(world: &mut VardeWorld) {
    let sent = ai_sends(world);
    let prompt = sent.first().expect("a prompt");
    assert!(prompt.contains("my edits"), "no buffer version:\n{prompt}");
    assert!(
        prompt.contains("changed on disk"),
        "no disk version:\n{prompt}"
    );
}

#[given(expr = "the terminal input is empty")]
#[then(expr = "the terminal input is empty")]
#[then(expr = "the terminal is offered nothing")]
fn terminal_input_empty(world: &mut VardeWorld) {
    assert_eq!(world.terminal_input, "");
}

#[given(expr = "the terminal input is {string}")]
fn terminal_input_is(world: &mut VardeWorld, text: String) {
    world.terminal_input = text;
}

#[given(expr = "the file tree shows the folder {string}")]
fn tree_shows_folder(world: &mut VardeWorld, path: String) {
    world.target = Some(Target::Folder(PathBuf::from(path)));
}

#[given(expr = "the file tree shows the file {string}")]
fn tree_shows_file(world: &mut VardeWorld, path: String) {
    world.target = Some(Target::File(PathBuf::from(path)));
}

#[given(expr = "I trigger {string} on that folder")]
#[when(expr = "I trigger {string} on that folder")]
#[when(expr = "I trigger {string} on that file")]
fn trigger_on_target(world: &mut VardeWorld, action: String) {
    world.trigger(&action);
}

#[when(expr = "I trigger {string}")]
fn trigger_bare(world: &mut VardeWorld, action: String) {
    world.trigger(&action);
}

#[given(expr = "I enter the name {string}")]
#[when(expr = "I enter the name {string}")]
fn enter_name(world: &mut VardeWorld, name: String) {
    world.send(Event::EnterName(name));
}

#[given(expr = "I press {string}")]
#[when(expr = "I press {string}")]
#[given(expr = "I pressed {string}")]
fn press(world: &mut VardeWorld, key: String) {
    // Through the router: what a tapped Space means is where the keyboard is
    // and what mode it is in, which no one event stands for — and in Stepping
    // mode that is true of every key, since the mode claims four letters and
    // hands the rest back to whatever they always were.
    if key == "Space"
        || world.state.stepping
        // The Snippet is a buffer being typed into, so what a key means there
        // is the router's answer too: Enter runs it in normal mode and opens a
        // line while inserting, which no one event stands for.
        || world.state.focus == Pane::Evaluator
        || named_key(&key).is_some_and(|event| matches!(event.code, terminput::KeyCode::F(_)))
    {
        return route_key(world, &key, 0);
    }
    world.send(match key.as_str() {
        "Escape" => Event::Cancel,
        "Ctrl+Space" => Event::FallbackBinding,
        "Alt+h" => Event::MoveFocus(Direction::Left),
        "Alt+l" => Event::MoveFocus(Direction::Right),
        "Alt+k" => Event::MoveFocus(Direction::Up),
        "Alt+j" => Event::MoveFocus(Direction::Down),
        "Down" => Event::MoveSelection(Direction::Down),
        "Up" => Event::MoveSelection(Direction::Up),
        "Right" => Event::MoveAction(Direction::Right),
        "Left" => Event::MoveAction(Direction::Left),
        "Enter" => Event::Activate,
        "Ctrl+c" => Event::Copy,
        other => Event::Key(parse_char(other)),
    });
}

/// A real keypress, routed the way the edge routes it: converted losslessly and
/// handed to the key router, which decides whether Varde claims it or the
/// child receives it. `I press` above sends the event a key stands for; this
/// sends the key.
#[given(expr = "I press the key {string}")]
#[when(expr = "I press the key {string}")]
fn press_the_key(world: &mut VardeWorld, key: String) {
    route_key(world, &key, 0);
}

#[when(expr = "I press the key {string} and press it again after {int} ms")]
fn press_the_key_twice(world: &mut VardeWorld, key: String, gap: u64) {
    route_key(world, &key, 0);
    route_key(world, &key, gap);
}

/// The key a Scenario names, for the names that are not a single character.
/// `None` is a key spelled as itself.
fn named_key(key: &str) -> Option<terminput::KeyEvent> {
    let alt = |code| terminput::KeyEvent::new(code).modifiers(terminput::KeyModifiers::ALT);
    let plain = terminput::KeyEvent::new;
    Some(match key {
        "Alt+Enter" => alt(terminput::KeyCode::Enter),
        "Alt+h" => alt(terminput::KeyCode::Char('h')),
        "Alt+Left" => alt(terminput::KeyCode::Left),
        // Ctrl+Option: the terminal reports both modifiers, which is the
        // shape the router reads for the jump alias.
        "Ctrl+Alt+Left" => plain(terminput::KeyCode::Left)
            .modifiers(terminput::KeyModifiers::ALT | terminput::KeyModifiers::CTRL),
        "Ctrl+Alt+Right" => plain(terminput::KeyCode::Right)
            .modifiers(terminput::KeyModifiers::ALT | terminput::KeyModifiers::CTRL),
        "Alt+Right" => alt(terminput::KeyCode::Right),
        "Left" => plain(terminput::KeyCode::Left),
        "Right" => plain(terminput::KeyCode::Right),
        "Ctrl+c" => plain(terminput::KeyCode::Char('c')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+Enter" => plain(terminput::KeyCode::Enter).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+v" => plain(terminput::KeyCode::Char('v')).modifiers(terminput::KeyModifiers::CTRL),
        // Command, which the host terminal reports as Super where it reports
        // it at all: the alias the copy and paste keys inspect and nothing
        // else does.
        "Cmd+c" => plain(terminput::KeyCode::Char('c')).modifiers(terminput::KeyModifiers::SUPER),
        "Cmd+v" => plain(terminput::KeyCode::Char('v')).modifiers(terminput::KeyModifiers::SUPER),
        "Ctrl+d" => plain(terminput::KeyCode::Char('d')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+n" => plain(terminput::KeyCode::Char('n')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+p" => plain(terminput::KeyCode::Char('p')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+s" => plain(terminput::KeyCode::Char('s')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+z" => plain(terminput::KeyCode::Char('z')).modifiers(terminput::KeyModifiers::CTRL),
        // The base key and Shift as separate facts, the way the Kitty protocol
        // reports it: the router applies the shift on the way in.
        "Ctrl+Shift+z" => plain(terminput::KeyCode::Char('z'))
            .modifiers(terminput::KeyModifiers::CTRL | terminput::KeyModifiers::SHIFT),
        "Escape" => plain(terminput::KeyCode::Esc),
        "Enter" => plain(terminput::KeyCode::Enter),
        "Backspace" => plain(terminput::KeyCode::Backspace),
        "Alt+Backspace" => alt(terminput::KeyCode::Backspace),
        "Tab" => plain(terminput::KeyCode::Tab),
        // What a terminal sends for a back-tab, converted: the code is Tab
        // and the shift is a modifier, which is the shape the router reads.
        "Shift+Tab" => plain(terminput::KeyCode::Tab).modifiers(terminput::KeyModifiers::SHIFT),
        "Up" => plain(terminput::KeyCode::Up),
        "Down" => plain(terminput::KeyCode::Down),
        "Ctrl+Space" => {
            plain(terminput::KeyCode::Char(' ')).modifiers(terminput::KeyModifiers::CTRL)
        }
        "Space" => plain(terminput::KeyCode::Char(' ')),
        // A function key, bare or with the one modifier a binding inspects.
        other => {
            let (modifiers, name) = match other.split_once('+') {
                Some(("Ctrl", name)) => (terminput::KeyModifiers::CTRL, name),
                Some(("Shift", name)) => (terminput::KeyModifiers::SHIFT, name),
                Some(_) => return None,
                None => (terminput::KeyModifiers::NONE, other),
            };
            let number = name.strip_prefix('F')?.parse().ok()?;
            plain(terminput::KeyCode::F(number)).modifiers(modifiers)
        }
    })
}

fn route_key(world: &mut VardeWorld, key: &str, at_ms: u64) {
    let event = named_key(key).unwrap_or_else(|| plain_key(parse_char(key)));
    let mut drafts = std::mem::take(&mut world.drafts);
    for event in keys::on_key_event(&world.state, &mut drafts, event, at_ms) {
        world.send(event);
    }
    world.drafts = drafts;
}

fn parse_char(key: &str) -> char {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c,
        _ => panic!("unhandled key {key:?}"),
    }
}

fn parse_view(name: &str) -> View {
    match name {
        "Edit" => View::Edit,
        "Review" => View::Review,
        "Story" => View::Story,
        other => panic!("unknown view {other:?}"),
    }
}

#[given(expr = "the terminal reports modifier key events")]
fn reports_modifiers(world: &mut VardeWorld) {
    world.state.reports_modifiers = true;
}

#[given(expr = "the terminal does not report modifier key events")]
fn no_modifiers(world: &mut VardeWorld) {
    world.state.reports_modifiers = false;
}

#[given(expr = "the double-tap window is {int} ms")]
fn double_tap_window(world: &mut VardeWorld, ms: u64) {
    world.state.double_tap_ms = ms;
}

#[given(expr = "the current view is {word}")]
fn current_view_is(world: &mut VardeWorld, view: String) {
    world.state.view = parse_view(&view);
}

#[then(expr = "the current view is {word}")]
fn current_view_should_be(world: &mut VardeWorld, view: String) {
    assert_eq!(world.state.view, parse_view(&view));
}

#[when(expr = "I press Ctrl and press Ctrl again after {int} ms")]
fn double_tap(world: &mut VardeWorld, gap: u64) {
    world.send(Event::Tapped {
        key: Tap::Ctrl,
        at_ms: 0,
    });
    world.send(Event::Tapped {
        key: Tap::Ctrl,
        at_ms: gap,
    });
}

#[when(expr = "I press Ctrl")]
fn single_ctrl(world: &mut VardeWorld) {
    world.send(Event::Tapped {
        key: Tap::Ctrl,
        at_ms: 0,
    });
}

#[given(expr = "the view palette is shown")]
fn open_palette(world: &mut VardeWorld) {
    world.state.modal = Modal::Palette;
}

#[then(expr = "the view palette is shown")]
fn palette_should_be_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.modal, Modal::Palette);
}

#[then(expr = "the view palette is not shown")]
fn palette_should_not_be_shown(world: &mut VardeWorld) {
    assert_ne!(world.state.modal, Modal::Palette);
}

#[then("the view palette offers:")]
fn palette_offers(_world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, char, String)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| (row[0].clone(), parse_char(&row[1]), row[2].clone()))
        .collect();
    let actual: Vec<(String, char, String)> = PALETTE
        .iter()
        .flat_map(|(group, entries)| {
            entries
                .iter()
                .map(|(key, entry)| (group.to_string(), *key, entry.trim().to_string()))
        })
        .collect();
    assert_eq!(actual, expected);
}

/// The row and the letter beside it are the same gesture, so the step names
/// the entry the palette draws and the click carries the key that row offers.
fn palette_key(entry: &str) -> char {
    PALETTE
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .find(|(_, label)| label.trim() == entry)
        .map(|(key, _)| *key)
        .unwrap_or_else(|| panic!("no palette entry {entry:?}"))
}

/// The palette is up for any click on one of its rows — the mouse hit-tests
/// them only while it is — so a step that reaches for an entry opens it first
/// rather than relying on the core to.
fn pick_palette_entry(world: &mut VardeWorld, entry: &str) {
    world.state.modal = Modal::Palette;
    world.send(Event::ClickPaletteEntry(palette_key(entry)));
}

#[when(expr = "I click the palette entry {string}")]
fn click_palette_entry(world: &mut VardeWorld, entry: String) {
    pick_palette_entry(world, &entry);
}

#[then(expr = "the view was not re-rendered")]
fn not_rerendered(world: &mut VardeWorld) {
    assert!(world.rendered.is_empty(), "rendered: {:?}", world.rendered);
}

#[given(expr = "the terminal is in {string}")]
fn terminal_is_in(_world: &mut VardeWorld, _dir: String) {
    // Intentionally inert: R3.3 makes injected paths absolute precisely so the
    // shell's current directory cannot change the outcome.
}

#[then(expr = "the terminal input is {string}")]
fn terminal_input_should_be(world: &mut VardeWorld, expected: String) {
    assert_eq!(world.terminal_input, expected);
}

#[then("the terminal input is:")]
fn terminal_input_should_be_docstring(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim();
    assert_eq!(world.terminal_input, expected);
}

#[then(expr = "no command has been executed")]
fn nothing_executed(world: &mut VardeWorld) {
    assert!(world.executed.is_empty(), "executed: {:?}", world.executed);
}

#[then(expr = "the terminal has executed {string}")]
fn terminal_executed(world: &mut VardeWorld, expected: String) {
    assert_eq!(world.executed, vec![expected]);
}

#[then(expr = "the name box is shown")]
fn name_box_shown(world: &mut VardeWorld) {
    assert!(matches!(world.state.modal, Modal::NameBox { .. }));
}

#[then(expr = "the name box is not shown")]
fn name_box_not_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.modal, Modal::None);
}

#[tokio::main]
async fn main() {
    VardeWorld::cucumber()
        // Undefined and skipped steps must fail the build. Without this, a
        // mistyped step name reads as a pass. See AGENTS.md.
        .fail_on_skipped()
        .run_and_exit("features")
        .await;
}

#[when(expr = "I change the terminal input to {string}")]
fn change_terminal_input(world: &mut VardeWorld, text: String) {
    world.terminal_input = text;
}

#[then(expr = "{string} is open in the editor")]
fn is_open(world: &mut VardeWorld, path: String) {
    assert_eq!(world.opened, vec![PathBuf::from(path)]);
}

#[then(expr = "no file was opened in the editor")]
fn nothing_opened(world: &mut VardeWorld) {
    assert!(world.opened.is_empty(), "opened: {:?}", world.opened);
}

#[when(expr = "the AI creates {string}")]
fn ai_creates(world: &mut VardeWorld, path: String) {
    world.send(Event::FilesAppeared(vec![(
        PathBuf::from(path),
        tree::Kind::File,
    )]));
}

#[when(expr = "{int} files appear on disk from a branch checkout")]
fn checkout_files(world: &mut VardeWorld, count: usize) {
    let paths = (0..count)
        .map(|n| {
            (
                world.startup.root.join(format!("checked-out-{n}.rs")),
                tree::Kind::File,
            )
        })
        .collect();
    world.send(Event::FilesAppeared(paths));
}

// ---- F7: review view ----

#[given(expr = "the project is a git repository")]
fn is_git_repo(world: &mut VardeWorld) {
    world.state.repo = Some(Vec::new());
}

#[given(expr = "the project is not a git repository")]
fn not_git_repo(world: &mut VardeWorld) {
    world.state.repo = None;
}

#[given(expr = "the working tree has no changes")]
fn no_changes(world: &mut VardeWorld) {
    world.state.repo = Some(Vec::new());
}

#[given("the working tree contains:")]
fn working_tree_contains(world: &mut VardeWorld, step: &Step) {
    let files: Vec<GitFile> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| GitFile {
            path: row[0].clone(),
            status: match row[1].as_str() {
                "modified" => GitStatus::Modified,
                "staged" => GitStatus::Staged,
                "untracked" => GitStatus::Untracked,
                "committed" => GitStatus::Committed,
                "ignored" => GitStatus::Ignored,
                other => panic!("unknown git status {other:?}"),
            },
        })
        .collect();
    // On disk as well as in git's answer: a file git reports a status for is a
    // file that exists, and a snapshot is taken off the tree. Without them a
    // revert has nothing to put back and "only the touched files were restored"
    // would pass for a tree that held none of them. Only where the scenario has
    // not said what the file holds.
    for file in &files {
        if !world.project.iter().any(|(name, _)| *name == file.path) {
            world.write_tree(&file.path, "the file as it stands\n");
        }
    }
    world.state.repo = Some(files);
}

#[given(expr = "I open Review view")]
#[given(expr = "I opened Review view")]
#[when(expr = "I open Review view")]
fn open_review_view(world: &mut VardeWorld) {
    world.send(Event::OpenReviewView);
}

#[given("the review list shows:")]
fn review_list_seed(world: &mut VardeWorld, step: &Step) {
    let files = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| GitFile {
            path: row[0].clone(),
            status: GitStatus::Modified,
        })
        .collect();
    world.state.repo = Some(files);
}

#[then("the review list shows:")]
fn review_list_shows(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let mut actual = review::list(&world.state);
    let mut expected_sorted = expected.clone();
    actual.sort();
    expected_sorted.sort();
    assert_eq!(actual, expected_sorted);
}

#[then(expr = "the review list is empty")]
fn review_list_empty(world: &mut VardeWorld) {
    assert!(review::list(&world.state).is_empty());
}

#[then(expr = "the review view state is {string}")]
fn review_view_state(world: &mut VardeWorld, expected: String) {
    assert_eq!(review::view_state(&world.state), expected);
}

// ---- F8: submitting ----

#[given(expr = "{string} is changed at revision {string}")]
fn changed_at_revision(world: &mut VardeWorld, path: String, _revision: String) {
    world.state.repo = Some(vec![GitFile {
        path,
        status: GitStatus::Modified,
    }]);
}

/// A blob oid, computed exactly as the edge computes one — over content bytes,
/// not a fixture string — so a scenario's expectation is real git hashing, not
/// a hard-coded stand-in for it.
fn blob_oid(content: &str) -> String {
    git2::Oid::hash_object(git2::ObjectType::Blob, content.as_bytes())
        .expect("hash content")
        .to_string()
}

#[given(expr = "{string} is shown in the diff holding:")]
#[when(expr = "{string} is shown in the diff holding:")]
fn shown_in_diff_holding(world: &mut VardeWorld, file: String, step: &Step) {
    let content = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let lines = content
        .lines()
        .enumerate()
        .map(|(index, text)| DiffLine {
            new_line: Some(index + 1),
            old_line: None,
            removed: false,
            text: text.to_string(),
        })
        .collect();
    let revision = blob_oid(&content);
    world.diff_contents.insert(file.clone(), content);
    world.send(Event::ShowDiff {
        file,
        lines,
        revision,
    });
}

#[given(expr = "{string} is shown in the diff as deleted, having held:")]
fn shown_in_diff_as_deleted(world: &mut VardeWorld, file: String, step: &Step) {
    let content = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let lines = content
        .lines()
        .enumerate()
        .map(|(index, text)| DiffLine {
            new_line: None,
            old_line: Some(index + 1),
            removed: true,
            text: text.to_string(),
        })
        .collect();
    let revision = blob_oid(&content);
    world.diff_contents.insert(file.clone(), content);
    world.send(Event::ShowDiff {
        file,
        lines,
        revision,
    });
}

#[given(expr = "an AI session is running in the AI pane")]
fn ai_running(world: &mut VardeWorld) {
    world.ai_pane = true;
    world.tell_core();
    // A session that is up has printed something; a just-started one has not,
    // which is what `I start the AI with` leaves behind.
    world.state.ai_spoken = true;
}

#[given(expr = "no AI session is running in the AI pane")]
fn ai_not_running(world: &mut VardeWorld) {
    world.ai_pane = false;
    world.tell_core();
}

#[given(expr = "the AI CLI cannot be started")]
#[when(expr = "the AI CLI cannot be started")]
fn ai_cannot_start(world: &mut VardeWorld) {
    world.ai_spawn_fails = true;
}

#[given(expr = "the AI CLI can be started")]
#[when(expr = "the AI CLI can be started")]
fn ai_can_start(world: &mut VardeWorld) {
    world.ai_spawn_fails = false;
}

#[given(expr = "the effective setting {string} is {string}")]
fn set_effective_setting(world: &mut VardeWorld, key: String, value: String) {
    assert_eq!(key, "ai.command");
    world.state.ai_command = value;
}

#[given(expr = "I drag the gutter of {string} from line {int} to line {int}")]
#[when(expr = "I drag the gutter of {string} from line {int} to line {int}")]
fn drag_gutter(world: &mut VardeWorld, file: String, from_line: u32, to_line: u32) {
    world.send(Event::DragGutter {
        file,
        from_line,
        to_line,
    });
}

/// The whole gesture the reviewer performs: the letter that picks the type,
/// the body typed into the box, and the key that files it. Routed as real
/// keypresses rather than sent as one event, because the box's own routing —
/// which letters pick a type, and which key files rather than starting a new
/// line — is what these scenarios are about.
#[when(expr = "I choose the type {word} and enter {string}")]
fn choose_type(world: &mut VardeWorld, kind: String, body: String) {
    pick_comment_type(world, kind);
    type_in_comment_body(world, body);
    file_comment(world);
}

#[given(expr = "I pick the comment type {word}")]
#[when(expr = "I pick the comment type {word}")]
fn pick_comment_type(world: &mut VardeWorld, kind: String) {
    let letter = match kind.as_str() {
        "ISSUE" => "i",
        "NOTE" => "n",
        "SUGGESTION" => "s",
        "COMMENT" => "c",
        other => panic!("no letter picks {other:?}"),
    };
    route_key(world, letter, 0);
}

#[given(expr = "I type {string} in the comment body")]
#[when(expr = "I type {string} in the comment body")]
fn type_in_comment_body(world: &mut VardeWorld, text: String) {
    for key in text.chars() {
        route_key(world, &key.to_string(), 0);
    }
}

#[given(expr = "I press {word} in the comment body")]
#[when(expr = "I press {word} in the comment body")]
fn press_in_comment_body(world: &mut VardeWorld, key: String) {
    route_key(world, &key, 0);
}

#[given(expr = "I press the {word} arrow in the comment body")]
#[when(expr = "I press the {word} arrow in the comment body")]
fn arrow_in_comment_body(world: &mut VardeWorld, direction: String) {
    route_key(world, &direction, 0);
}

#[given(expr = "I move a word left in the comment body")]
#[when(expr = "I move a word left in the comment body")]
fn word_left_in_comment_body(world: &mut VardeWorld) {
    route_key(world, "Alt+Left", 0);
}

#[given(expr = "I file the comment")]
#[when(expr = "I file the comment")]
fn file_comment(world: &mut VardeWorld) {
    route_key(world, "Ctrl+s", 0);
}

/// A paste the comment box takes, routed by `keys::on_paste` as `main` routes
/// one. Any other destination fails the step rather than being replayed as
/// keystrokes: a paste replayed as keys is the defect this scenario exists for,
/// since the first newline in it was read as the key that filed the comment.
#[given("I paste into the comment body:")]
#[when("I paste into the comment body:")]
fn paste_into_comment_body(world: &mut VardeWorld, step: &Step) {
    let text = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    match keys::on_paste(&world.state, &world.drafts, text) {
        keys::Pasted::ToBuffer(event) => world.send(event),
        _ => panic!("the comment body did not take the paste"),
    }
}

#[then("the comment body holds:")]
fn comment_body_holds(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    let body = world.state.comment.as_ref().expect("the box has a body");
    assert_eq!(body.shown(), expected);
}

#[then("the filed comment's body is:")]
fn filed_comment_body(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    let comment = world.state.comments.first().expect("a comment");
    assert_eq!(comment.body, expected);
}

#[given(expr = "I add an ISSUE on {string} lines {int} to {int} saying {string}")]
#[when(expr = "I add an ISSUE on {string} lines {int} to {int} saying {string}")]
fn add_issue(world: &mut VardeWorld, file: String, from: u32, to: u32, body: String) {
    add_comment(world, "ISSUE", file, from, to, body);
}

#[given(expr = "I add a {word} on {string} lines {int} to {int} saying {string}")]
#[when(expr = "I add a {word} on {string} lines {int} to {int} saying {string}")]
fn add_typed(world: &mut VardeWorld, kind: String, file: String, from: u32, to: u32, body: String) {
    add_comment(world, &kind, file, from, to, body);
}

fn add_comment(world: &mut VardeWorld, kind: &str, file: String, from: u32, to: u32, body: String) {
    world.send(Event::AddComment {
        file,
        from_line: from,
        to_line: to,
        kind: kind.to_string(),
        body,
    });
}

#[given(expr = "the review holds no comments")]
#[then(expr = "the review holds no comments")]
fn review_empty(world: &mut VardeWorld) {
    assert!(world.state.comments.is_empty());
}

#[then("the review holds a comment:")]
fn review_holds_comment(world: &mut VardeWorld, step: &Step) {
    let comment = world.state.comments.first().expect("a comment");
    let rows = &step.table().expect("table").rows;
    // Two shapes: a vertical list of (field, value) rows, or a header row of
    // field names followed by one row of values.
    let pairs: Vec<(&str, &str)> = if rows.first().is_some_and(|row| row.len() > 2) {
        rows[0]
            .iter()
            .map(String::as_str)
            .zip(rows[1].iter().map(String::as_str))
            .collect()
    } else {
        rows.iter()
            .map(|row| (row[0].as_str(), row[1].as_str()))
            .collect()
    };
    for (field, expected) in pairs {
        let actual = match field {
            "file" => comment.file.clone(),
            "from_line" | "from" => comment.from_line.to_string(),
            "to_line" | "to" => comment.to_line.to_string(),
            "type" => comment.kind.clone(),
            "body" => comment.body.clone(),
            "revision" => comment.revision.clone(),
            "story" => comment.story.clone().unwrap_or_default(),
            "step" => comment.step.map(|s| s.to_string()).unwrap_or_default(),
            other => panic!("unknown field {other:?}"),
        };
        assert_eq!(actual, expected, "field {field}");
    }
}

#[given(expr = "I commented {string} on the current step with {string}")]
#[when(expr = "I comment {string} on the current step with {string}")]
fn comment_on_current_step(world: &mut VardeWorld, kind: String, body: String) {
    world.send(Event::Key('c'));
    pick_comment_type(world, kind);
    type_in_comment_body(world, body);
    file_comment(world);
}

/// Story view's own code surface, not the review artifact: the artifact holds
/// the comment either way, so asserting on it would pass without a single row
/// being drawn. This asks where the row *sits* — under the line it covers.
#[then(expr = "the code shows a comment on line {int} of {string}")]
fn code_shows_comment_on_line(world: &mut VardeWorld, line: u32, file: String) {
    assert_eq!(
        story::shown_file(&world.state),
        file,
        "Story view is showing another file"
    );
    let rows: Vec<_> = story::rows(&world.state, line as usize + 1).collect();
    let under = rows
        .iter()
        .position(|row| *row == story::Row::Code(line))
        .expect("a row for that line")
        + 1;
    assert!(
        matches!(rows.get(under), Some(story::Row::Comment(_))),
        "no comment row under line {line} of {file}"
    );
}

#[then(expr = "the comment's revision is the blob oid of what {string} held")]
fn comment_revision_is_blob_oid(world: &mut VardeWorld, file: String) {
    let content = world
        .diff_contents
        .get(&file)
        .expect("content shown for this file");
    let expected = blob_oid(content);
    let comment = world.state.comments.last().expect("a comment");
    assert_eq!(comment.revision, expected);
}

#[then(expr = "the two comments record different revisions")]
fn two_comments_record_different_revisions(world: &mut VardeWorld) {
    assert_eq!(world.state.comments.len(), 2, "expected two comments");
    assert_ne!(
        world.state.comments[0].revision,
        world.state.comments[1].revision
    );
}

#[then(expr = "the comment records no story")]
fn comment_records_no_story(world: &mut VardeWorld) {
    let comment = world.state.comments.last().expect("a comment");
    assert!(comment.story.is_none());
}

#[given(expr = "I submit the review")]
#[when(expr = "I submit the review")]
fn submit_review(world: &mut VardeWorld) {
    world.send(Event::SubmitReview);
}

#[given(expr = "I confirm the submission")]
#[when(expr = "I confirm the submission")]
fn confirm_submission(world: &mut VardeWorld) {
    world.send(Event::ConfirmSubmit);
}

#[when(expr = "I decline the submission")]
fn decline_submission(world: &mut VardeWorld) {
    world.send(Event::Cancel);
}

#[then(expr = "the submission confirmation is shown")]
fn confirmation_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.modal, Modal::ConfirmSubmit);
}

#[then(expr = "the submission confirmation is not shown")]
fn confirmation_not_shown(world: &mut VardeWorld) {
    assert_ne!(world.state.modal, Modal::ConfirmSubmit);
}

#[when(expr = "the AI session is ready for input")]
fn ai_spoke(world: &mut VardeWorld) {
    world.send(Event::AiSpoke);
}

#[then(expr = "the prompt reached the AI as one paste, with its line breaks intact")]
fn prompt_as_one_paste(world: &mut VardeWorld) {
    let sent = ai_sends(world);
    let [prompt] = sent.as_slice() else {
        panic!("expected one send, got {sent:?}")
    };
    assert!(prompt.contains("\x1b[200~"), "not a paste: {prompt:?}");
    assert!(
        prompt.contains('\n'),
        "line breaks were flattened: {prompt:?}"
    );
}

#[then(expr = "the review verdict is {string}")]
fn review_verdict(world: &mut VardeWorld, expected: String) {
    assert_eq!(world.state.last_verdict.as_deref(), Some(expected.as_str()));
}

#[then(expr = "the file {string} exists")]
fn file_exists(world: &mut VardeWorld, path: String) {
    assert!(
        world.files.contains_key(&abs(world, &path)),
        "missing {path}"
    );
}

#[then(expr = "the file {string} does not exist")]
fn file_absent(world: &mut VardeWorld, path: String) {
    assert!(
        !world.files.contains_key(&abs(world, &path)),
        "still there: {path}"
    );
}

/// Whatever reached the AI's child, as text. The review injection is the only
/// thing these scenarios send it.
fn ai_sends(world: &VardeWorld) -> Vec<String> {
    world
        .keys_sent
        .iter()
        .filter(|(pane, _)| *pane == Pane::Ai)
        .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        .collect()
}

#[then(expr = "the AI pane was sent a prompt containing {string}")]
fn prompt_contains(world: &mut VardeWorld, needle: String) {
    let sent = ai_sends(world);
    let prompt = sent.first().expect("a prompt");
    assert!(prompt.contains(&needle), "prompt was:\n{prompt}");
}

#[then(expr = "the prompt was submitted to the AI")]
fn prompt_submitted(world: &mut VardeWorld) {
    let sent = ai_sends(world);
    let [prompt] = sent.as_slice() else {
        panic!("expected one send, got {sent:?}")
    };
    assert!(prompt.ends_with('\r'), "not submitted: {prompt:?}");
}

#[then(expr = "the prompt was not submitted to the AI")]
fn prompt_not_submitted(world: &mut VardeWorld) {
    let sent = ai_sends(world);
    assert!(!sent.is_empty(), "nothing reached the AI");
    assert!(
        sent.iter().all(|prompt| !prompt.contains('\r')),
        "submitted: {sent:?}"
    );
}

/// The one thing the AI's prompt was handed, with the paste markers taken off.
fn pasted(world: &VardeWorld) -> String {
    let sent = ai_sends(world);
    let [prompt] = sent.as_slice() else {
        panic!("expected one send, got {sent:?}")
    };
    prompt.replace("\x1b[200~", "").replace("\x1b[201~", "")
}

#[then(expr = "the AI pane's prompt holds a Pause snapshot")]
fn prompt_holds_snapshot(world: &mut VardeWorld) {
    let snapshot = varde::debug::snapshot(&world.state).expect("still Paused");
    assert_eq!(pasted(world), snapshot);
}

#[then(expr = "the AI program received the Pause snapshot as a bracketed paste")]
fn snapshot_bracketed(world: &mut VardeWorld) {
    let sent = ai_sends(world);
    let [prompt] = sent.as_slice() else {
        panic!("expected one send, got {sent:?}")
    };
    assert!(prompt.starts_with("\x1b[200~"), "{prompt:?}");
    assert!(prompt.ends_with("\x1b[201~"), "{prompt:?}");
}

/// The quoted source is every line carrying the gutter's bar, each ending in
/// the text the buffer holds on that line.
#[then(
    expr = "the Pause snapshot holds {string} lines {int} to {int} with line {int} marked as the Paused line"
)]
fn snapshot_holds_lines(
    world: &mut VardeWorld,
    file: String,
    from: usize,
    to: usize,
    marked: usize,
) {
    let text = pasted(world);
    let source = world.state.buffers[&abs(world, &file)].disk.clone();
    let source: Vec<&str> = source.lines().collect();
    let quoted: Vec<&str> = text.lines().filter(|line| line.contains(" | ")).collect();
    assert_eq!(quoted.len(), to + 1 - from, "{text}");
    for (line, number) in quoted.into_iter().zip(from..=to) {
        assert!(line.ends_with(source[number - 1]), "{number}: {line}");
        assert!(line.contains(&number.to_string()), "{number}: {line}");
        assert_eq!(line.starts_with('\u{2192}'), number == marked, "{line}");
    }
}

#[then(expr = "the Pause snapshot names the Frames {string} and {string}")]
fn snapshot_names_frames(world: &mut VardeWorld, inner: String, outer: String) {
    let text = pasted(world);
    let frames = &text[text.find("Frames:").expect("a Frames section")..];
    let at = |name: &str| {
        frames
            .find(name)
            .unwrap_or_else(|| panic!("{name}: {text}"))
    };
    assert!(at(&inner) < at(&outer), "{text}");
}

#[then(expr = "the Pause snapshot holds the Variable {string} with the value {string}")]
#[then(expr = "the AI pane's prompt holds the Variable {string} with the value {string}")]
fn snapshot_holds_variable(world: &mut VardeWorld, name: String, value: String) {
    let text = pasted(world);
    assert!(text.contains(&format!("{name} = {value}")), "{text}");
}

#[then(expr = "the Pause snapshot holds the exception {string}")]
fn snapshot_holds_exception(world: &mut VardeWorld, exception: String) {
    let text = pasted(world);
    assert!(text.contains(&exception), "{text}");
}

#[then(expr = "no prompt was sent to the AI")]
fn no_prompt(world: &mut VardeWorld) {
    assert!(ai_sends(world).is_empty(), "{:?}", world.keys_sent);
}

#[then(expr = "no new AI session was started")]
fn no_ai_started(world: &mut VardeWorld) {
    assert!(world.ai_spawned.is_empty());
}

#[then(expr = "an AI session was started with {string}")]
fn ai_started_with(world: &mut VardeWorld, command: String) {
    assert_eq!(world.ai_spawned, vec![command]);
}

#[then(expr = "the AI was started again with {string}")]
fn ai_started_again_with(world: &mut VardeWorld, command: String) {
    assert_eq!(
        world.ai_spawned.last().map(String::as_str),
        Some(command.as_str()),
        "spawned: {:?}",
        world.ai_spawned
    );
}

#[then(expr = "the review is not submitted")]
fn not_submitted(world: &mut VardeWorld) {
    assert!(world.files.is_empty() && ai_sends(world).is_empty());
}

#[then(expr = "the reviewer is told the review is empty")]
fn told_empty(world: &mut VardeWorld) {
    assert_eq!(world.notices, vec!["review-empty".to_string()]);
}

#[given(expr = "the retention limit is {int} reviews")]
fn retention_limit(world: &mut VardeWorld, limit: usize) {
    world.state.retention_limit = limit;
}

#[given(expr = "{string} holds {int} reviews numbered {word} to {word}")]
fn seed_reviews(world: &mut VardeWorld, dir: String, _count: usize, first: String, last: String) {
    let (first, last): (u32, u32) = (first.parse().unwrap(), last.parse().unwrap());
    for number in first..=last {
        // Both sides: a scenario that seeds before starting is stating what the
        // edge found on disk, and one that seeds after is stating what the
        // session already wrote.
        world.startup.reviews.insert(number);
        world.state.reviews.insert(number);
        let path = abs(world, &format!("{dir}/{number:04}.json"));
        world.files.insert(path, "{}".to_string());
    }
}

#[then(expr = "{string} holds {int} reviews")]
fn holds_reviews(world: &mut VardeWorld, dir: String, expected: usize) {
    let prefix = abs(world, &dir);
    let count = world
        .files
        .keys()
        .filter(|p| p.starts_with(&prefix))
        .count();
    assert_eq!(count, expected);
}

// ---- F10: mouse ----

fn parse_pane(name: &str) -> Pane {
    match name {
        "file tree" => Pane::Tree,
        "editor" => Pane::Editor,
        "terminal" => Pane::Terminal,
        "AI" => Pane::Ai,
        "risk" => Pane::Risk,
        "buffers" => Pane::Buffers,
        "history" => Pane::History,
        "diagnostics" => Pane::Diagnostics,
        other => panic!("unknown pane {other:?}"),
    }
}

#[given(expr = "the {word} pane has focus")]
fn pane_has_focus(world: &mut VardeWorld, pane: String) {
    world.state.focus = parse_pane(&pane);
}

#[then(expr = "the {word} pane has focus")]
fn pane_should_have_focus(world: &mut VardeWorld, pane: String) {
    assert_eq!(world.state.focus, parse_pane(&pane));
}

#[then(expr = "the {word} pane still has focus")]
fn pane_still_has_focus(world: &mut VardeWorld, pane: String) {
    assert_eq!(world.state.focus, parse_pane(&pane));
}

#[when(expr = "I click in the {word} pane")]
fn click_pane(world: &mut VardeWorld, pane: String) {
    // A click is a press and a release: the press is ours, and the release is
    // what a child that asked for mouse events gets.
    let pane = parse_pane(&pane);
    world.send(Event::ClickPane(pane));
    world.send(Event::ClickThrough { pane, at: POINTER });
}

#[when(expr = "I click at line {int} column {int} in the editor")]
fn click_text(world: &mut VardeWorld, line: usize, column: usize) {
    world.click(Pane::Editor, (line, column), terminput::KeyModifiers::NONE);
}

/// The jump gesture: Cmd where a terminal reports it, and Ctrl — which every
/// terminal reports — is what the scenarios drive, since the two are one
/// gesture to `mouse`.
const JUMP: terminput::KeyModifiers = terminput::KeyModifiers::CTRL;

/// The toggle sits in the gutter, which `pointer_at` measures *past* — so the
/// column is named here, against the same constant the renderer draws it at.
#[when(expr = "I click the fold toggle on row {int} in the editor")]
fn click_fold_toggle(world: &mut VardeWorld, row: usize) {
    let panes = world.panes();
    let column = panes.editor.x + 1 + layout::TOGGLE_COLUMN;
    let at = panes.editor.y + 1 + (row - 1 - world.state.editor_scroll) as u16;
    let mut pointer = mouse::Pointer::default();
    for kind in [mouse::Kind::LeftDown, mouse::Kind::LeftUp] {
        let outcome = mouse::on_mouse(
            &world.state,
            &panes,
            &mut pointer,
            mouse::Input {
                kind,
                column,
                row: at,
                modifiers: terminput::KeyModifiers::NONE,
            },
        );
        for event in outcome.events {
            world.send(event);
        }
    }
}

#[when(expr = "I click at line {int} column {int} in the editor with the jump modifier held")]
fn jump_click_text(world: &mut VardeWorld, line: usize, column: usize) {
    world.click(Pane::Editor, (line, column), JUMP);
}

#[when(expr = "I click on {string} in the {word} pane with the jump modifier held")]
fn jump_click_text_in_pane(world: &mut VardeWorld, text: String, pane: String) {
    let pane = parse_pane(&pane);
    let lines = world.pane_lines(pane);
    let index = lines
        .iter()
        .position(|line| line.contains(&text))
        .unwrap_or_else(|| panic!("{text:?} is not in that pane"));
    let at = lines[index].find(&text).expect("the column");
    let column = lines[index][..at].chars().count() + 1;
    world.click(pane, (index + 1, column), JUMP);
}

#[then(expr = "the browser opens {string}")]
fn browser_opens(world: &mut VardeWorld, url: String) {
    assert_eq!(world.browser, vec![url]);
}

#[then("the browser opens nothing")]
fn browser_opens_nothing(world: &mut VardeWorld) {
    assert!(world.browser.is_empty(), "opened: {:?}", world.browser);
}

#[given(expr = "I hold the jump modifier over line {int} column {int} in the editor")]
#[when(expr = "I hold the jump modifier over line {int} column {int} in the editor")]
fn hold_over_text(world: &mut VardeWorld, line: usize, column: usize) {
    world.point(Pane::Editor, Some((line, column)), JUMP);
}

#[when(expr = "I point at line {int} column {int} in the editor")]
fn point_at_text(world: &mut VardeWorld, line: usize, column: usize) {
    world.point(
        Pane::Editor,
        Some((line, column)),
        terminput::KeyModifiers::NONE,
    );
}

#[then(expr = "the editor underlines line {int} columns {int} to {int}")]
fn underlines(world: &mut VardeWorld, line: usize, from: usize, to: usize) {
    assert_eq!(varde::link(&world.state), Some((line, from, to)));
}

#[then("the editor underlines nothing")]
fn underlines_nothing(world: &mut VardeWorld) {
    assert_eq!(varde::link(&world.state), None);
}

#[when(expr = "I right-click in the {word} pane")]
fn right_click(world: &mut VardeWorld, pane: String) {
    world.send(Event::RightClick(parse_pane(&pane)));
}

#[then(expr = "nothing happened")]
fn nothing_happened(world: &mut VardeWorld) {
    assert!(world.opened.is_empty() && world.scrolled.is_empty() && world.clipboard.is_none());
}

#[when(expr = "I click the row {string}")]
fn click_row(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    world.send(Event::ClickRow(absolute));
}

#[given(expr = "the file tree shows the expanded folder {string}")]
fn tree_shows_expanded(world: &mut VardeWorld, folder: String) {
    folder_is_expanded(world, folder);
}

#[given(expr = "I scroll {word} with the pointer over the {word} pane")]
#[when(expr = "I scroll {word} with the pointer over the {word} pane")]
fn scroll_over(world: &mut VardeWorld, direction: String, pane: String) {
    world.send(Event::Scroll {
        pane: parse_pane(&pane),
        direction: parse_direction(&direction),
        at: POINTER,
    });
}

/// The same notch, several times over. A wheel notch is one row, so a
/// scenario about where the clamp stops needs more than a scenario about where
/// one notch lands — spelling each one out is the same step six times.
#[given(expr = "I scroll {word} {int} times with the pointer over the {word} pane")]
#[when(expr = "I scroll {word} {int} times with the pointer over the {word} pane")]
fn scroll_over_times(world: &mut VardeWorld, direction: String, times: usize, pane: String) {
    for _ in 0..times {
        scroll_over(world, direction.clone(), pane.clone());
    }
}

#[given(expr = "I scroll {word} with the pointer over the file tree pane")]
#[when(expr = "I scroll {word} with the pointer over the file tree pane")]
fn scroll_over_tree(world: &mut VardeWorld, direction: String) {
    world.send(Event::Scroll {
        pane: Pane::Tree,
        direction: parse_direction(&direction),
        at: POINTER,
    });
}

fn parse_direction(name: &str) -> Direction {
    match name {
        "up" => Direction::Up,
        "down" => Direction::Down,
        "left" => Direction::Left,
        "right" => Direction::Right,
        other => panic!("unknown direction {other:?}"),
    }
}

#[then(expr = "the {word} pane scrolled {word}")]
fn pane_scrolled(world: &mut VardeWorld, pane: String, direction: String) {
    assert!(world
        .scrolled
        .contains(&(parse_pane(&pane), parse_direction(&direction))));
}

#[then(expr = "the {word} pane did not scroll")]
fn pane_did_not_scroll(world: &mut VardeWorld, pane: String) {
    let pane = parse_pane(&pane);
    assert!(!world.scrolled.iter().any(|(which, _)| *which == pane));
    // The panes the core scrolls itself leave no effect behind, so the offset
    // is the only thing that can prove they stayed put.
    match pane {
        Pane::Tree => assert_eq!(world.state.tree_scroll, 0),
        Pane::Editor => assert_eq!(world.state.editor_scroll, 0),
        _ => {}
    }
}

#[given(expr = "the {word} program asked for bracketed paste")]
fn program_asked_for_bracketed_paste(world: &mut VardeWorld, pane: String) {
    match parse_pane(&pane) {
        Pane::Ai => {
            world.ai_pane = true;
            world.tell_core();
            world.state.ai_paste = keys::Paste::Bracketed;
        }
        _ => world.state.terminal_paste = keys::Paste::Bracketed,
    }
}

#[given(expr = "the {word} program asked for {string} mouse reporting")]
fn program_asked_for_mouse(world: &mut VardeWorld, pane: String, encoding: String) {
    let encoding = match encoding.as_str() {
        "no" => Encoding::None,
        "legacy" => Encoding::Legacy,
        "SGR" => Encoding::Sgr,
        other => panic!("unknown mouse encoding {other:?}"),
    };
    match parse_pane(&pane) {
        Pane::Ai => {
            world.ai_pane = true;
            world.tell_core();
            world.state.ai_mouse = encoding;
        }
        _ => world.state.terminal_mouse = encoding,
    }
}

// The encoding, not the exact report: which one the child reads is the whole
// point, and the bytes of each are pinned in the mouse router's unit tests.
#[then(expr = "the click reached the {word} program in {string} encoding")]
#[then(expr = "the scroll reached the {word} program in {string} encoding")]
fn reached_in_encoding(world: &mut VardeWorld, pane: String, encoding: String) {
    let pane = parse_pane(&pane);
    let prefix: &[u8] = match encoding.as_str() {
        "legacy" => b"\x1b[M",
        "SGR" => b"\x1b[<",
        other => panic!("unknown mouse encoding {other:?}"),
    };
    let sent: Vec<&Vec<u8>> = world
        .keys_sent
        .iter()
        .filter(|(which, _)| *which == pane)
        .map(|(_, bytes)| bytes)
        .collect();
    assert!(
        matches!(sent.as_slice(), [report] if report.starts_with(prefix)),
        "sent: {sent:?}"
    );
}

#[then(expr = "nothing reached the {word} program")]
fn nothing_reached(world: &mut VardeWorld, pane: String) {
    let pane = parse_pane(&pane);
    assert!(
        !world.keys_sent.iter().any(|(which, _)| *which == pane),
        "keys sent: {:?}",
        world.keys_sent
    );
}

#[given(expr = "the cursor is on line {int}")]
fn cursor_on_line(world: &mut VardeWorld, line: usize) {
    let path = world.state.current_buffer.clone().expect("an open buffer");
    world
        .state
        .buffers
        .get_mut(&path)
        .expect("an open buffer")
        .go_to_place(varde::Place { line, column: 1 });
}

/// Where the cursor ended up, for a jump that was asked for by a row rather
/// than typed: the buffer clamps to what it holds, so a file too short for the
/// line would answer line 1 and this would fail rather than pass quietly.
#[then(expr = "the cursor is on line {int}")]
fn cursor_should_be_on_line(world: &mut VardeWorld, line: usize) {
    let path = world.state.current_buffer.clone().expect("an open buffer");
    assert_eq!(
        world.state.buffers.get(&path).expect("an open buffer").line,
        line
    );
}

// ---- Folding a block away ----

#[then(expr = "the editor hides lines {int} to {int}")]
fn hides_lines(world: &mut VardeWorld, from: usize, to: usize) {
    let hidden = varde::fold::hidden(&world.state);
    for line in from..=to {
        assert!(hidden.contains(&line), "line {line} is drawn: {hidden:?}");
    }
}

#[then(expr = "the editor hides no lines")]
fn hides_no_lines(world: &mut VardeWorld) {
    let hidden = varde::fold::hidden(&world.state);
    assert!(hidden.is_empty(), "{hidden:?}");
}

#[then(expr = "the fold toggle on line {int} is {word}")]
fn fold_toggle_is(world: &mut VardeWorld, line: usize, state: String) {
    let toggle = varde::fold::toggles(&world.state, ..)
        .get(&line)
        .copied()
        .unwrap_or_else(|| panic!("line {line} carries no fold toggle"));
    let named = match toggle {
        varde::fold::Toggle::Open => "open",
        varde::fold::Toggle::Folded => "folded",
    };
    assert_eq!(named, state);
}

#[then(expr = "line {int} carries no fold toggle")]
fn no_fold_toggle(world: &mut VardeWorld, line: usize) {
    let toggles = varde::fold::toggles(&world.state, ..);
    assert!(!toggles.contains_key(&line), "{toggles:?}");
}

/// One offset, two words for it. They agree everywhere but Story view, where
/// a comment row sits between two lines: `editor_scroll` counts the rows the
/// surface draws, which is what a scenario about framing has to say.
#[then(expr = "the editor view starts at line {int}")]
#[then(expr = "the editor view starts at row {int}")]
fn editor_view_starts(world: &mut VardeWorld, first: usize) {
    assert_eq!(world.state.editor_scroll + 1, first);
}

#[then(expr = "the editor view starts at column {int}")]
fn editor_view_starts_at_column(world: &mut VardeWorld, column: usize) {
    assert_eq!(world.state.editor_hscroll + 1, column);
}

#[then(expr = "the file tree view starts at row {int}")]
fn tree_view_starts(world: &mut VardeWorld, row: usize) {
    assert_eq!(world.state.tree_scroll + 1, row);
}

#[given(expr = "the screen is {int} rows by {int} columns")]
fn screen_is(world: &mut VardeWorld, rows: u16, columns: u16) {
    world.send(Event::Resized {
        width: columns,
        height: rows,
    });
}

#[given(expr = "{string} is open in the editor with {int} lines")]
fn open_with_lines(world: &mut VardeWorld, path: String, lines: usize) {
    let contents: Vec<String> = (1..=lines).map(|line| format!("line {line}")).collect();
    open_buffer(world, &path, &contents.join("\n"));
}

#[given(expr = "{string} is open in the editor with {int} lines of {int} characters")]
fn open_with_wide_lines(world: &mut VardeWorld, path: String, lines: usize, width: usize) {
    let contents: Vec<String> = (0..lines).map(|_| "x".repeat(width)).collect();
    open_buffer(world, &path, &contents.join("\n"));
}

/// A file taller than the editor with one word on two lines of it, and filler
/// that never holds that word everywhere else — so where an occurrence is
/// found is decided by the scenario rather than by the filler.
#[given(
    expr = "{string} is open in the editor with {int} lines, {string} on lines {int} and {int}"
)]
fn open_with_word_on_two_lines(
    world: &mut VardeWorld,
    path: String,
    lines: usize,
    word: String,
    first: usize,
    second: usize,
) {
    let contents: Vec<String> = (1..=lines)
        .map(|line| {
            if line == first || line == second {
                word.clone()
            } else {
                format!("filler {line}")
            }
        })
        .collect();
    open_buffer(world, &path, &contents.join("\n"));
}

#[then(expr = "line {int} is on screen in the editor")]
fn line_on_screen(world: &mut VardeWorld, line: usize) {
    let fits = varde::fits_in(&world.state, &world.panes()).1;
    let first = world.state.editor_scroll + 1;
    assert!(
        (first..first + fits).contains(&line),
        "the editor shows lines {first} to {}",
        first + fits - 1
    );
}

#[given(expr = "{string} is open in the editor holding a {int}-character line above a short one")]
fn open_with_long_line(world: &mut VardeWorld, path: String, width: usize) {
    let contents = format!("{}\nend", "x".repeat(width));
    open_buffer(world, &path, &contents);
}

/// A markdown file whose one code fence holds a line wider than the pane —
/// the case the sideways gesture exists for, since a fence is laid out one row
/// per source line and never wrapped (ADR 0007).
#[given(expr = "{string} is open in the editor holding a code fence {int} characters wide")]
fn open_with_wide_fence(world: &mut VardeWorld, path: String, width: usize) {
    let contents = format!("```rust\n{}\n```\n", "x".repeat(width));
    open_buffer(world, &path, &contents);
}

#[given(expr = "the workspace folder holds {int} files")]
fn folder_holds(world: &mut VardeWorld, count: usize) {
    let entries = (1..=count)
        .map(|index| Entry {
            name: format!("file-{index:02}.js"),
            is_dir: false,
        })
        .collect();
    let root = world.state.root.clone();
    world.state.contents.insert(root, entries);
}

#[given(expr = "the divider between the file tree and the editor is at column {int}")]
fn divider_at(world: &mut VardeWorld, column: u32) {
    world.state.tree_divider = column;
}

#[given(expr = "I drag that divider to column {int}")]
#[when(expr = "I drag that divider to column {int}")]
fn drag_divider(world: &mut VardeWorld, column: u32) {
    world.send(Event::DragDivider(column));
}

#[given(expr = "I drag the AI pane's edge to column {int}")]
#[when(expr = "I drag the AI pane's edge to column {int}")]
fn drag_ai_edge(world: &mut VardeWorld, column: u16) {
    let panes = world.panes();
    let mut pointer = mouse::Pointer::default();
    for (kind, at) in [
        (mouse::Kind::LeftDown, panes.ai.x),
        (mouse::Kind::LeftDrag, column),
    ] {
        let outcome = mouse::on_mouse(
            &world.state,
            &panes,
            &mut pointer,
            mouse::Input {
                kind,
                column: at,
                row: 1,
                modifiers: terminput::KeyModifiers::NONE,
            },
        );
        for event in outcome.events {
            world.send(event);
        }
    }
}

// ---- The minimap ----

/// Driven through the hit-test rather than by sending the event, so what the
/// scenario presses is the strip's own columns: a travel that works only when
/// the event is posted by hand is a travel nobody can perform with a mouse.
///
/// The row is 1-based, the way a scenario counts rows of a pane.
#[given(expr = "I drag the minimap to row {int}")]
#[when(expr = "I drag the minimap to row {int}")]
fn drag_minimap(world: &mut VardeWorld, row: u16) {
    let panes = world.panes();
    let strip = varde::minimap::strip(&world.state, panes.editor);
    let mut pointer = mouse::Pointer::default();
    for kind in [mouse::Kind::LeftDown, mouse::Kind::LeftDrag] {
        let outcome = mouse::on_mouse(
            &world.state,
            &panes,
            &mut pointer,
            mouse::Input {
                kind,
                column: strip.x,
                row: strip.y + row - 1,
                modifiers: terminput::KeyModifiers::NONE,
            },
        );
        for event in outcome.events {
            world.send(event);
        }
    }
}

/// A move, not a press: what lights the slider is where the pointer is, and
/// the event only fires when that answer changes — so this drives the hit-test
/// rather than the field, the same way the drag step does.
#[given(expr = "I move the pointer onto the minimap")]
#[when(expr = "I move the pointer onto the minimap")]
fn pointer_onto_minimap(world: &mut VardeWorld) {
    let panes = world.panes();
    let strip = varde::minimap::strip(&world.state, panes.editor);
    move_pointer(world, strip.x, strip.y);
}

#[given(expr = "I move the pointer into the editor's text")]
#[when(expr = "I move the pointer into the editor's text")]
fn pointer_into_text(world: &mut VardeWorld) {
    let panes = world.panes();
    move_pointer(
        world,
        panes.editor.x + 1 + layout::GUTTER,
        panes.editor.y + 1,
    );
}

fn move_pointer(world: &mut VardeWorld, column: u16, row: u16) {
    let panes = world.panes();
    let outcome = mouse::on_mouse(
        &world.state,
        &panes,
        &mut mouse::Pointer::default(),
        mouse::Input {
            kind: mouse::Kind::Moved,
            column,
            row,
            modifiers: terminput::KeyModifiers::NONE,
        },
    );
    for event in outcome.events {
        world.send(event);
    }
}

#[then(expr = "the minimap slider is lit")]
fn minimap_slider_lit(world: &mut VardeWorld) {
    assert!(varde::minimap::lit(&world.state));
}

#[then(expr = "the minimap slider is quiet")]
fn minimap_slider_quiet(world: &mut VardeWorld) {
    assert!(!varde::minimap::lit(&world.state));
}

#[then(expr = "the minimap mirrors lines {int} through {int}")]
fn minimap_mirrors(world: &mut VardeWorld, first: usize, last: usize) {
    assert_eq!(varde::minimap::mirrored(&world.state), Some((first, last)));
}

/// A scenario about the editor's own geometry says this rather than carrying
/// the mirror's twelve columns through its arithmetic: what it pins is the
/// slide, the box or the caret clamp, and a number that moves when an
/// unrelated strip is widened is a number nobody can read.
///
/// Apart from the `Then` below on purpose — one step that set the flag and then
/// asserted it would be an assertion that cannot fail.
#[given(expr = "the minimap is turned off")]
fn minimap_turned_off(world: &mut VardeWorld) {
    world.state.minimap = false;
}

#[then(expr = "the minimap is hidden")]
fn minimap_hidden(world: &mut VardeWorld) {
    assert_eq!(varde::minimap::mirrored(&world.state), None);
    assert_eq!(varde::minimap::width(&world.state), 0);
}

/// The columns the clamp lets a line of text reach — what the mirror costs,
/// measured where it is paid.
#[then(expr = "the editor shows {int} columns of text")]
fn editor_text_columns(world: &mut VardeWorld, columns: usize) {
    assert_eq!(varde::fits(&world.state).2, columns);
}

#[then(expr = "the AI pane is {int} columns wide")]
fn ai_pane_width(world: &mut VardeWorld, columns: u16) {
    assert_eq!(world.panes().ai.width, columns);
}

#[given(expr = "I make the AI pane tall from the command line")]
#[when(expr = "I make the AI pane tall from the command line")]
fn make_ai_pane_tall(world: &mut VardeWorld) {
    world.send(Event::ToggleTallAi);
}

// ---- F45: the Strip ----

/// A `[dap.<language>]` row in the programs template's layer, which is where
/// ADR 0021 puts a Debug adapter.
#[given(expr = "a Debug adapter for {string} is configured")]
///
/// The template's row where it ships one — so a scenario's `codelldb` is the
/// command the rust row really names — and installed, since that is what
/// "configured" is short for here: `the command … is not on PATH` takes it
/// away. Into the running workspace as well as the file, for the reason
/// `load_debug_config` gives.
fn debug_adapter_configured(world: &mut VardeWorld, language: String) {
    let adapter = shipped()
        .adapters
        .get(&language)
        .cloned()
        .unwrap_or_else(|| startup::Adapter {
            command: format!("{language}-adapter"),
            args: Vec::new(),
            install: BTreeMap::new(),
            server: None,
            plugin: None,
        });
    let row = format!("[dap.{language}]\ncommand = \"{}\"", adapter.command);
    world.startup.global_config = Some(match world.startup.global_config.take() {
        Some(config) => format!("{config}\n{row}"),
        None => row,
    });
    // And what installs it, so a scenario that takes the command away is left
    // with a row an install would fix rather than one needing an installer.
    let installer = adapter
        .install
        .get("macos")
        .and_then(|install| tools::installer(install));
    for command in std::iter::once(adapter.command.clone()).chain(installer) {
        world.on_path.insert(command.clone());
        world.state.commands_on_path.insert(command);
    }
    // Spawned whatever the template says, since the scripted adapter is what
    // answers: the Rule about adapters a language server hosts configures its
    // own row.
    let adapter = startup::Adapter {
        server: None,
        plugin: None,
        ..adapter
    };
    world.state.adapters.insert(language, adapter);
}

/// A World starts with none, and nothing in these steps starts one.
#[given(expr = "no Debug session exists")]
fn no_debug_session(_world: &mut VardeWorld) {}

#[given(expr = "the screen is {int} columns by {int} rows")]
#[when(expr = "the screen is resized to {int} columns by {int} rows")]
fn screen_columns_by_rows(world: &mut VardeWorld, columns: u16, rows: u16) {
    world.send(Event::Resized {
        width: columns,
        height: rows,
    });
}

#[given(expr = "the Strip is {int} rows tall")]
fn strip_is(world: &mut VardeWorld, rows: u32) {
    world.state.strip_height = Some(rows);
}

/// Through the hit-test, from the Strip's own top border, so the handle is
/// one a pointer can reach.
#[when(expr = "I drag the border above the Strip {word} {int} rows")]
fn drag_strip(world: &mut VardeWorld, way: String, rows: u16) {
    let border = world.panes().terminal.y;
    let to = match way.as_str() {
        "up" => border.saturating_sub(rows),
        "down" => border + rows,
        other => panic!("no direction {other:?}"),
    };
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, 2, border);
    world.report(mouse::Kind::LeftDrag, 2, to);
    world.report(mouse::Kind::LeftUp, 2, to);
}

/// The state and the rectangle both: what is remembered has to be what is
/// drawn.
#[then(expr = "the Strip is {int} rows tall")]
fn strip_should_be(world: &mut VardeWorld, rows: u16) {
    assert_eq!(world.state.strip_height, Some(u32::from(rows)));
    assert_eq!(world.panes().terminal.height, rows);
}

#[then(expr = "the Strip is at its least height")]
fn strip_is_least(world: &mut VardeWorld) {
    strip_should_be(world, layout::STRIP_LEAST);
}

#[then(expr = "the area above the Strip is at its least height")]
fn top_is_least(world: &mut VardeWorld) {
    let (_, height) = world.screen();
    strip_should_be(world, height - layout::TOP_LEAST);
    assert_eq!(world.panes().terminal.y, layout::TOP_LEAST);
}

#[then(expr = "the project {string} records the Strip as {int} rows tall")]
fn records_strip(world: &mut VardeWorld, _file: String, rows: u64) {
    let saved: serde_json::Value =
        serde_json::from_str(world.startup.state_json.as_deref().expect("state saved"))
            .expect("json");
    assert_eq!(saved["strip_height"].as_u64(), Some(rows));
}

#[then(expr = "the Group tabs are:")]
fn group_tabs_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let tabs: Vec<String> = varde::group_tabs(&world.state)
        .iter()
        .map(|tab| format!("{:?}", tab.group))
        .collect();
    assert_eq!(tabs, expected);
}

#[then(expr = "the Strip shows the Shell group")]
fn strip_shows_shells(world: &mut VardeWorld) {
    assert_eq!(world.state.strip, layout::Group::Shells);
    let lit: Vec<layout::Group> = varde::group_tabs(&world.state)
        .into_iter()
        .filter_map(|tab| tab.lit.then_some(tab.group))
        .collect();
    assert_eq!(lit, vec![layout::Group::Shells]);
}

// ---- F45: Breakpoints ----

/// Through the hit-test, in the gutter's Breakpoint column, so the column is
/// one a pointer can reach.
#[given(expr = "I click the Breakpoint column on line {int}")]
#[when(expr = "I click the Breakpoint column on line {int}")]
fn click_breakpoint_column(world: &mut VardeWorld, line: usize) {
    let panes = world.panes();
    let (_, row) = pointer_at(&world.state, &panes, Pane::Editor, (line, 1));
    let column = panes.editor.x + 1 + layout::BREAKPOINT_COLUMN;
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, row);
    world.report(mouse::Kind::LeftUp, column, row);
}

// ---- F41: Run marks ----

#[then(expr = "line {int} carries a Run mark")]
fn carries_run_mark(world: &mut VardeWorld, line: usize) {
    load_debug_config(world);
    let marks = varde::run::marks(&world.state);
    assert!(marks.contains_key(&line), "Run marks: {marks:?}");
}

#[then(expr = "line {int} carries no Run mark")]
fn carries_no_run_mark(world: &mut VardeWorld, line: usize) {
    load_debug_config(world);
    let marks = varde::run::marks(&world.state);
    assert!(!marks.contains_key(&line), "Run marks: {marks:?}");
}

#[then("no line carries a Run mark")]
fn no_run_marks(world: &mut VardeWorld) {
    load_debug_config(world);
    assert_eq!(varde::run::marks(&world.state), BTreeMap::new());
}

/// Through the hit-test, on the Breakpoint column the ▶ is drawn in.
#[when(expr = "I click the Run mark on line {int}")]
fn click_run_mark(world: &mut VardeWorld, line: usize) {
    load_debug_config(world);
    click_breakpoint_column(world, line);
}

#[then("the Run mark offers:")]
fn run_mark_offers(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a table of choices")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let offered: Vec<&str> = varde::run::chips(&world.state)
        .iter()
        .map(|chip| chip.action)
        .collect();
    assert_eq!(offered, expected);
}

/// The click, then the key the chosen Chip names. The adapter answers, so a
/// Debug session gets as far as its launch request.
#[when(expr = "I choose {string} on the Run mark on line {int}")]
fn choose_on_run_mark(world: &mut VardeWorld, choice: String, line: usize) {
    click_run_mark(world, line);
    let chip = varde::run::chips(&world.state)
        .into_iter()
        .find(|chip| chip.action == choice)
        .unwrap_or_else(|| panic!("the Run mark does not offer {choice:?}"));
    world.dap.ready = true;
    route_key(world, chip.keys, 0);
}

#[then(expr = "the Debug adapter's launch arguments name {string}")]
fn launch_arguments_name(world: &mut VardeWorld, name: String) {
    let arguments = last_request(world, "launch")["arguments"].to_string();
    assert!(arguments.contains(&name), "launch arguments: {arguments}");
}

#[then(expr = "the Debug adapter's launch arguments do not name {string}")]
fn launch_arguments_do_not_name(world: &mut VardeWorld, name: String) {
    let arguments = last_request(world, "launch")["arguments"].to_string();
    assert!(!arguments.contains(&name), "launch arguments: {arguments}");
}

/// The column right of the Breakpoint column, where the number starts.
#[when(expr = "I click the line number of line {int}")]
fn click_line_number(world: &mut VardeWorld, line: usize) {
    let panes = world.panes();
    let (_, row) = pointer_at(&world.state, &panes, Pane::Editor, (line, 1));
    let column = panes.editor.x + 2 + layout::BREAKPOINT_COLUMN;
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, row);
    world.report(mouse::Kind::LeftUp, column, row);
}

/// Set as the core holds one, against what the file holds on the line. A file
/// the scenario never described is given lines enough to hold it, so going to
/// the Breakpoint lands on its line rather than clamping to an empty file's.
/// Recorded in the project's state too, as setting it would have: a Then about
/// what is remembered has to have something to find unchanged.
#[given(expr = "a Breakpoint on {string} line {int}")]
fn breakpoint_on(world: &mut VardeWorld, file: String, line: usize) {
    let relative = file.clone();
    let file = abs(world, &file);
    world.files.entry(file.clone()).or_insert_with(|| {
        (1..=line)
            .map(|at| format!("line {at}"))
            .collect::<Vec<_>>()
            .join("\n")
    });
    let text = world
        .files
        .get(&file)
        .and_then(|held| held.split('\n').nth(line - 1))
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut saved: serde_json::Value = world
        .startup
        .state_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let remembered = serde_json::json!({"file": relative, "line": line, "text": text});
    match saved["breakpoints"].as_array_mut() {
        Some(breakpoints) => breakpoints.push(remembered),
        None => saved["breakpoints"] = serde_json::json!([remembered]),
    }
    world.startup.state_json = Some(saved.to_string());
    world.state.breakpoints.push(varde::debug::Breakpoint {
        file,
        line,
        text,
        stale: false,
        properties: Default::default(),
    });
}

/// Written straight onto the Breakpoint the step before it set, the way a
/// project that remembered one with its properties would hold it.
fn with_property(
    world: &mut VardeWorld,
    file: String,
    line: usize,
    set: impl FnOnce(&mut varde::debug::Properties),
) {
    breakpoint_on(world, file, line);
    set(&mut world
        .state
        .breakpoints
        .last_mut()
        .expect("a Breakpoint")
        .properties);
}

#[given(expr = "a Breakpoint on {string} line {int} with the condition {string}")]
fn breakpoint_with_condition(world: &mut VardeWorld, file: String, line: usize, text: String) {
    with_property(world, file, line, |properties| properties.condition = text);
}

#[given(expr = "a Breakpoint on {string} line {int} with the hit count {string}")]
fn breakpoint_with_hit_count(world: &mut VardeWorld, file: String, line: usize, text: String) {
    with_property(world, file, line, |properties| properties.hit_count = text);
}

#[given(expr = "a Logpoint on {string} line {int} with the message {string}")]
fn logpoint_with_message(world: &mut VardeWorld, file: String, line: usize, text: String) {
    with_property(world, file, line, |properties| {
        properties.log_message = text
    });
}

/// The Breakpoint as the adapter was told of it, found by its line among
/// every `setBreakpoints` request sent.
fn sent_breakpoint(world: &VardeWorld, line: usize) -> Value {
    dap_requests(world, "setBreakpoints")
        .into_iter()
        .flat_map(|request| {
            request["arguments"]["breakpoints"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .find(|breakpoint| breakpoint["line"] == line)
        .unwrap_or_else(|| {
            panic!(
                "no Breakpoint on line {line} sent; sent {:?}",
                world.dap.sent
            )
        })
}

#[then(
    expr = "the Debug adapter was sent a \"setBreakpoints\" request for line {int} with the condition {string}"
)]
fn sent_condition(world: &mut VardeWorld, line: usize, text: String) {
    assert_eq!(sent_breakpoint(world, line)["condition"], text);
}

#[then(
    expr = "the Debug adapter was sent a \"setBreakpoints\" request for line {int} with the hit condition {string}"
)]
fn sent_hit_condition(world: &mut VardeWorld, line: usize, text: String) {
    assert_eq!(sent_breakpoint(world, line)["hitCondition"], text);
}

#[then(
    expr = "the Debug adapter was sent a \"setBreakpoints\" request for line {int} with the log message {string}"
)]
fn sent_log_message(world: &mut VardeWorld, line: usize, text: String) {
    assert_eq!(sent_breakpoint(world, line)["logMessage"], text);
}

#[then(expr = "the Breakpoint on {string} line {int} suspends {string}")]
fn breakpoint_suspends(world: &mut VardeWorld, file: String, line: usize, scope: String) {
    let file = abs(world, &file);
    let breakpoint = world
        .state
        .breakpoints
        .iter()
        .find(|breakpoint| breakpoint.file == file && breakpoint.line == line)
        .expect("a Breakpoint there");
    let suspends = match breakpoint.properties.suspend {
        varde::debug::Suspend::Thread => "thread",
        varde::debug::Suspend::All => "all",
    };
    assert_eq!(suspends, scope);
}

/// Through `␣B`'s event, on the buffer on screen: the box is only ever opened
/// on a Breakpoint somebody is looking at.
#[given(expr = "the Breakpoint box is open for {string} line {int}")]
fn breakpoint_box_opened(world: &mut VardeWorld, file: String, line: usize) {
    assert_eq!(world.state.current_buffer, Some(abs(world, &file)));
    world.send(Event::EditBreakpoint(line));
    breakpoint_box_is_open(world, file, line);
}

#[then(expr = "the Breakpoint box is open for {string} line {int}")]
fn breakpoint_box_is_open(world: &mut VardeWorld, file: String, line: usize) {
    let file = abs(world, &file);
    match &world.state.modal {
        Modal::Breakpoint {
            file: open,
            line: at,
            ..
        } => assert_eq!((open, *at), (&file, line)),
        other => panic!("expected the Breakpoint box, got {other:?}"),
    }
}

/// Tab to the switch, then Space: the keys, so the switch is one a keyboard
/// can reach.
#[when("I switch the Breakpoint's suspend scope")]
fn switch_suspend_scope(world: &mut VardeWorld) {
    while !matches!(
        world.state.modal,
        Modal::Breakpoint {
            field: varde::debug::Field::Suspend,
            ..
        }
    ) {
        press_key(world, terminput::KeyCode::Tab);
    }
    press_key(world, terminput::KeyCode::Char(' '));
}

/// Typed a key at a time into the row the box opens on, which is the
/// condition's.
#[given(expr = "I write the condition {string} in the Breakpoint box")]
#[when(expr = "I write the condition {string} in the Breakpoint box")]
fn write_condition(world: &mut VardeWorld, text: String) {
    assert!(matches!(
        world.state.modal,
        Modal::Breakpoint {
            field: varde::debug::Field::Condition,
            ..
        }
    ));
    for c in text.chars() {
        press_key(world, terminput::KeyCode::Char(c));
    }
}

#[when("I confirm the Breakpoint box")]
fn confirm_breakpoint_box(world: &mut VardeWorld) {
    press_key(world, terminput::KeyCode::Enter);
}

/// The `✎` on the line is drawn on the cursor's line while the editor has the
/// keyboard, so both are put there first; the click then lands on the cell
/// `debug::edit_chip` names, which `ui` draws it at.
#[when(expr = "I click the {string} Chip on line {int}'s Breakpoint")]
fn click_line_chip(world: &mut VardeWorld, chip: String, line: usize) {
    assert_eq!(chip, "edit", "unknown line Chip {chip:?}");
    world.state.focus = Pane::Editor;
    current_buffer_mut(world).go_to_place(Place { line, column: 1 });
    let (column, row) =
        varde::debug::edit_chip(&world.state, &world.panes()).expect("the Chip on screen");
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, row);
    world.report(mouse::Kind::LeftUp, column, row);
}

fn breakpoint_lines(world: &VardeWorld, file: &str) -> Vec<usize> {
    let file = abs(world, file);
    world
        .state
        .breakpoints
        .iter()
        .filter(|breakpoint| breakpoint.file == file)
        .map(|breakpoint| breakpoint.line)
        .collect()
}

#[then(expr = "{string} line {int} has a Breakpoint")]
fn has_breakpoint(world: &mut VardeWorld, file: String, line: usize) {
    assert!(
        breakpoint_lines(world, &file).contains(&line),
        "Breakpoints: {:?}",
        world.state.breakpoints
    );
}

#[then(expr = "{string} line {int} has no Breakpoint")]
fn has_no_breakpoint(world: &mut VardeWorld, file: String, line: usize) {
    assert!(
        !breakpoint_lines(world, &file).contains(&line),
        "Breakpoints: {:?}",
        world.state.breakpoints
    );
}

#[then(expr = "{string} has no Breakpoints")]
fn has_no_breakpoints(world: &mut VardeWorld, file: String) {
    assert_eq!(breakpoint_lines(world, &file), Vec::<usize>::new());
}

/// Added to whatever the project's state already records, the way a project
/// that set a Breakpoint last time also recorded everything else.
#[given(expr = "the project {string} records a Breakpoint on {string} line {int} holding {string}")]
fn state_records_breakpoint(
    world: &mut VardeWorld,
    path: String,
    file: String,
    line: u64,
    text: String,
) {
    assert_eq!(path, ".varde/state.json");
    let mut saved: serde_json::Value = world
        .startup
        .state_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    saved["breakpoints"] = serde_json::json!([{"file": file, "line": line, "text": text}]);
    world.startup.state_json = Some(saved.to_string());
}

#[then(expr = "the project {string} records a Breakpoint on {string} line {int} holding {string}")]
fn records_breakpoint(world: &mut VardeWorld, path: String, file: String, line: u64, text: String) {
    assert_eq!(path, ".varde/state.json");
    let saved: serde_json::Value =
        serde_json::from_str(world.startup.state_json.as_deref().expect("state saved"))
            .expect("json");
    assert_eq!(
        saved["breakpoints"],
        serde_json::json!([{"file": file, "line": line, "text": text}])
    );
}

/// The folder is the one the scenario already opened; what makes it Bare is
/// that its state goes to a Sidecar.
#[given(expr = "the workspace is a Bare workspace")]
fn workspace_is_bare(world: &mut VardeWorld) {
    world.startup.sidecar = Some(PathBuf::from(SIDECAR));
    world.state.sidecar = Some(PathBuf::from(SIDECAR));
}

/// What the edge does between two runs: a Bare workspace's state went to its
/// Sidecar, which is deleted at exit, and the next run is another process
/// with a Sidecar of its own — so what it reads is nothing.
#[when(expr = "Varde starts again in the same folder")]
fn starts_again_in_same_folder(world: &mut VardeWorld) {
    if world.startup.sidecar.is_some() {
        world.startup.sidecar = Some(PathBuf::from(format!("{SIDECAR}1")));
        world.startup.state_json = None;
    }
    varde_starts(world);
}

/// Read off the rows the Breakpoint list draws.
#[then(expr = "the Breakpoint list marks {string} line {int} as {string}")]
fn breakpoint_list_marks(world: &mut VardeWorld, file: String, line: usize, mark: String) {
    let file = abs(world, &file);
    let breakpoint = varde::debug::list(&world.state)
        .into_iter()
        .find(|breakpoint| breakpoint.file == file && breakpoint.line == line)
        .expect("a Breakpoint there");
    let marked = match breakpoint.stale {
        true => "stale",
        false => "current",
    };
    assert_eq!(marked, mark);
}

/// Through the palette's event rather than by poking the field, as every other
/// Corner occupant's Given is: the only way the list comes to be on screen is
/// being asked for.
#[given("the Corner shows the Breakpoint list")]
#[when("the Corner shows the Breakpoint list")]
fn corner_shows_breakpoint_list(world: &mut VardeWorld) {
    if world.state.corner != layout::Corner::Breakpoints {
        world.send(Event::ToggleBreakpointList);
    }
}

#[then("the Corner holds the Breakpoint list")]
fn corner_holds_breakpoint_list(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Breakpoints);
}

#[given("the Breakpoint list has focus")]
fn breakpoint_list_has_focus(world: &mut VardeWorld) {
    world.state.focus = Pane::Breakpoints;
}

/// Headerless: each row is a path relative to the root and a line, in the
/// order the list draws them.
#[then("the Breakpoint list rows are:")]
fn breakpoint_list_rows(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, usize)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| (row[0].clone(), row[1].parse().expect("a line")))
        .collect();
    let drawn: Vec<(String, usize)> = varde::debug::list(&world.state)
        .into_iter()
        .map(|breakpoint| {
            (
                varde::relative(&world.state, &breakpoint.file),
                breakpoint.line,
            )
        })
        .collect();
    assert_eq!(drawn, expected);
}

/// The row's icon is drawn on the row the keyboard is on, so the keyboard is
/// put there first; the click then goes through the hit-test at the icon's
/// columns, hard against the right-hand border.
#[when(expr = "I click the {string} Chip on the row for {string} line {int}")]
fn click_breakpoint_row_chip(world: &mut VardeWorld, chip: String, file: String, line: usize) {
    assert_eq!(chip, "remove", "unknown row Chip {chip:?}");
    let file = abs(world, &file);
    let index = varde::debug::list(&world.state)
        .iter()
        .position(|breakpoint| breakpoint.file == file && breakpoint.line == line)
        .expect("a row for that Breakpoint");
    world.state.focus = Pane::Breakpoints;
    world.state.breakpoints_selection = index;
    let column = world.panes().corner.width.saturating_sub(3) as usize;
    world.click(
        Pane::Breakpoints,
        (index + 1, column),
        terminput::KeyModifiers::NONE,
    );
}

/// A Chip on the Transport along the top border of the Corner's occupant or of
/// the Variables, found by the hit-test that answers a click there — never
/// driven as an event, so a Chip nobody could reach with a pointer fails here.
/// The focused Variables row's when no Transport offers the name: `ask-ai` is
/// on both, and the row's is asked for as "the row's".
#[given(expr = "I click the {string} Chip")]
#[when(expr = "I click the {string} Chip")]
fn click_chip(world: &mut VardeWorld, chip: String) {
    let on_transport = varde::debug::transport(&world.state)
        .into_iter()
        .chain(varde::debug::strip_transport(&world.state))
        .any(|offered| offered.name == chip);
    if !on_transport && row_chip(world, &chip).is_some() {
        return click_variables_row_chip(world, &chip);
    }
    let (area, chips) = match varde::debug::transport(&world.state)
        .iter()
        .any(|offered| offered.name == chip)
    {
        true => (world.panes().corner, varde::debug::transport(&world.state)),
        false => (
            varde::transport_area(&world.state, world.panes().strip()),
            varde::debug::strip_transport(&world.state),
        ),
    };
    let at = chips
        .iter()
        .position(|offered| offered.name == chip)
        .unwrap_or_else(|| panic!("no {chip:?} Chip"));
    let labels = layout::chip_labels(&chips, area.width, layout::CORNER_TITLE);
    let column = (area.x..area.right())
        .find(|&column| layout::strip_at(area, &labels, column) == Some(at))
        .expect("the Chip on screen");
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, area.y);
    world.report(mouse::Kind::LeftUp, column, area.y);
}

#[then("the workspace has no Breakpoints")]
fn workspace_has_no_breakpoints(world: &mut VardeWorld) {
    assert_eq!(world.state.breakpoints, vec![]);
}

#[then(expr = "the gutter draws line {int}'s Breakpoint as {string}")]
fn gutter_draws_breakpoint(world: &mut VardeWorld, line: usize, kind: String) {
    let drawn = match varde::debug::marks(&world.state).get(&line) {
        Some(varde::debug::Mark::Plain) => "plain",
        Some(varde::debug::Mark::Conditional) => "conditional",
        Some(varde::debug::Mark::Logpoint) => "logpoint",
        Some(varde::debug::Mark::Stale) => "stale",
        Some(varde::debug::Mark::Unverified) => "unverified",
        None => "none",
    };
    assert_eq!(drawn, kind);
}

#[then(expr = "the gutter draws a Breakpoint on line {int}")]
fn gutter_draws_a_breakpoint(world: &mut VardeWorld, line: usize) {
    let marks = varde::debug::marks(&world.state);
    assert!(marks.contains_key(&line), "gutter: {marks:?}");
}

#[then(expr = "the gutter draws no Breakpoint on line {int}")]
fn gutter_draws_no_breakpoint(world: &mut VardeWorld, line: usize) {
    let marks = varde::debug::marks(&world.state);
    assert!(!marks.contains_key(&line), "gutter: {marks:?}");
}

#[then(expr = "the Breakpoint list lists {string} line {int}")]
fn breakpoint_list_lists(world: &mut VardeWorld, file: String, line: usize) {
    let file = abs(world, &file);
    let rows: Vec<(PathBuf, usize)> = varde::debug::list(&world.state)
        .into_iter()
        .map(|breakpoint| (breakpoint.file.clone(), breakpoint.line))
        .collect();
    assert!(rows.contains(&(file, line)), "rows: {rows:?}");
}

/// The last `setBreakpoints` answered the way a real adapter answers it: one
/// entry per line asked for, in order, each bound where it was set except the
/// one the scenario names.
fn answer_set_breakpoints(world: &mut VardeWorld, line: usize, verdict: Value) {
    let request = last_request(world, "setBreakpoints").clone();
    let breakpoints: Vec<Value> = request["arguments"]["breakpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|asked| match asked["line"].as_u64() == Some(line as u64) {
            true => verdict.clone(),
            false => json!({ "verified": true, "line": asked["line"] }),
        })
        .collect();
    adapter_event(
        world,
        json!({ "type": "response", "request_seq": request["seq"], "success": true,
                "command": "setBreakpoints", "body": { "breakpoints": breakpoints } }),
    );
}

#[when(
    expr = "the Debug adapter answers \"setBreakpoints\" for line {int} with verified false and message {string}"
)]
fn adapter_leaves_unbound(world: &mut VardeWorld, line: usize, message: String) {
    answer_set_breakpoints(
        world,
        line,
        json!({ "verified": false, "message": message }),
    );
}

#[given(
    expr = "the Debug adapter answers \"setBreakpoints\" for line {int} with verified true at line {int}"
)]
#[when(
    expr = "the Debug adapter answers \"setBreakpoints\" for line {int} with verified true at line {int}"
)]
fn adapter_binds_elsewhere(world: &mut VardeWorld, line: usize, bound: usize) {
    answer_set_breakpoints(world, line, json!({ "verified": true, "line": bound }));
}

/// Through the hit-test, resting on the Breakpoint column the gutter draws it in.
#[then(expr = "line {int}'s Breakpoint explains {string} on hover")]
fn breakpoint_explains_on_hover(world: &mut VardeWorld, line: usize, reason: String) {
    let panes = world.panes();
    let (_, row) = pointer_at(&world.state, &panes, Pane::Editor, (line, 1));
    let column = panes.editor.x + 1 + layout::BREAKPOINT_COLUMN;
    world.report(mouse::Kind::Moved, column, row);
    assert_eq!(
        varde::debug::explained(&world.state),
        Some((line, reason.as_str()))
    );
}

// ---- F38: terminal splits ----

/// What the edge tells: how many shells it holds, after a spawn or an exit.
#[given(expr = "the terminal holds {int} shell(s)")]
#[when(expr = "the terminal holds {int} shell(s)")]
fn terminal_holds(world: &mut VardeWorld, shells: usize) {
    world.state.terminals.resize(shells, varde::Shell::Idle);
}

#[given(expr = "terminal {int} is running a process")]
fn terminal_busy(world: &mut VardeWorld, split: usize) {
    world.state.terminals[split - 1] = varde::Shell::Busy;
}

/// The same two as claims: what the Debug group hides, it does not stop.
#[then(expr = "the terminal holds {int} shell(s)")]
fn terminal_should_hold(world: &mut VardeWorld, shells: usize) {
    assert_eq!(world.state.terminals.len(), shells);
}

#[then(expr = "terminal {int} is running a process")]
fn terminal_should_be_busy(world: &mut VardeWorld, split: usize) {
    assert_eq!(world.state.terminals[split - 1], varde::Shell::Busy);
}

#[when(expr = "terminal {int} prints its prompt")]
fn terminal_spoke(world: &mut VardeWorld, split: usize) {
    world.send(Event::ShellSpoke(split - 1));
}

#[given(expr = "I split the terminal from the command line")]
#[when(expr = "I split the terminal from the command line")]
fn split_terminal(world: &mut VardeWorld) {
    world.send(Event::SplitTerminal);
}

#[given(expr = "terminal {int} has focus")]
fn focus_split(world: &mut VardeWorld, split: usize) {
    world.send(Event::FocusSplit(split - 1));
}

#[then(expr = "a shell is asked for beside terminal {int}")]
fn shell_asked_beside(world: &mut VardeWorld, split: usize) {
    assert_eq!(world.splits, vec![split - 1]);
}

#[then(expr = "terminal {int} has the keyboard")]
fn split_has_keyboard(world: &mut VardeWorld, split: usize) {
    assert_eq!(world.state.focus, Pane::Terminal);
    assert_eq!(world.state.split(), split - 1);
}

#[then(expr = "the AI pane spans the whole height")]
fn ai_pane_is_tall(world: &mut VardeWorld) {
    let panes = world.panes();
    assert_eq!(panes.ai.y, 0);
    assert_eq!(panes.ai.bottom(), panes.terminal.bottom());
}

#[then(expr = "the AI pane stops above the terminal")]
fn ai_pane_is_beside(world: &mut VardeWorld) {
    assert_eq!(world.panes().ai.bottom(), world.panes().terminal.y);
}

#[then(expr = "the terminal pane ends where the AI pane starts")]
fn terminal_ends_at_the_ai_pane(world: &mut VardeWorld) {
    let panes = world.panes();
    assert_eq!(panes.terminal.right(), panes.ai.x);
}

#[then(expr = "the terminal pane spans the whole width")]
fn terminal_spans_the_width(world: &mut VardeWorld) {
    let panes = world.panes();
    assert_eq!(panes.terminal.right(), panes.ai.right());
}

#[then(expr = "the divider between the file tree and the editor is at column {int}")]
fn divider_should_be_at(world: &mut VardeWorld, column: u32) {
    assert_eq!(world.state.tree_divider, column);
}

#[given(expr = "{string} is open in the editor")]
fn open_buffer_plain(world: &mut VardeWorld, path: String) {
    open_clean(world, path);
}

#[given(expr = "I copy the selection")]
#[when(expr = "I copy the selection")]
fn copy_selection(world: &mut VardeWorld) {
    world.send(Event::Copy);
}

/// Something else already put text there — another application, or an earlier
/// copy — which is what a paste reads.
#[given(expr = "the clipboard holds {string}")]
fn clipboard_seeded(world: &mut VardeWorld, text: String) {
    world.clipboard = Some(text);
}

#[then(expr = "the clipboard holds {string}")]
fn clipboard_holds(world: &mut VardeWorld, text: String) {
    assert_eq!(world.clipboard.as_deref(), Some(text.as_str()));
}

#[then(expr = "the file tree pane has focus")]
fn tree_pane_has_focus(world: &mut VardeWorld) {
    assert_eq!(world.state.focus, Pane::Tree);
}

#[given(expr = "the file tree pane has focus")]
fn give_tree_pane_focus(world: &mut VardeWorld) {
    world.state.focus = Pane::Tree;
}

// ---- F11 / F12: keyboard and row actions ----

#[given(expr = "I type {string}")]
#[when(expr = "I type {string}")]
fn type_text(world: &mut VardeWorld, text: String) {
    // Bytes are what a hosted pane's child receives. The Snippet is Varde's
    // own buffer, so the same typing reaches it as the editor's keys do.
    if world.state.focus == Pane::Evaluator {
        for key in text.chars() {
            world.send(Event::EditorKey(key));
        }
        return;
    }
    world.send(Event::Bytes(text.into_bytes()));
}

#[when(expr = "I paste {string}")]
fn paste_text(world: &mut VardeWorld, text: String) {
    world.send(Event::Pasted(
        text.replace("\\n", "\n").replace("\\e", "\x1b"),
    ));
}

/// A paste the buffer takes, routed by `keys::on_paste` as `main` routes one.
/// Any other destination fails the step rather than being replayed as
/// keystrokes: a paste that stopped being the buffer's is the defect these
/// scenarios exist for, and replaying it here would hide that behind whatever
/// the keys then did.
#[given(expr = "I paste {string} into the editor")]
#[when(expr = "I paste {string} into the editor")]
fn paste_into_editor(world: &mut VardeWorld, text: String) {
    match keys::on_paste(&world.state, &world.drafts, text.replace("\\n", "\n")) {
        keys::Pasted::ToBuffer(event) => world.send(event),
        _ => panic!("the buffer did not take the paste"),
    }
}

/// The copy-paste round trip, routed exactly as `main` routes a paste —
/// keystroke replay included, since a paste that reaches the buffer as keys is
/// what the scenario is about rather than a step failure.
#[when("I paste what was copied into the editor")]
fn paste_what_was_copied(world: &mut VardeWorld) {
    let text = world
        .clipboard
        .clone()
        .or_else(|| world.terminal_clipboard.clone())
        .expect("nothing was copied");
    let mut drafts = std::mem::take(&mut world.drafts);
    match keys::on_paste(&world.state, &drafts, text) {
        keys::Pasted::ToChild(event) | keys::Pasted::ToBuffer(event) => world.send(event),
        keys::Pasted::AsKeys(events) => {
            for key in events {
                for event in keys::on_key_event(&world.state, &mut drafts, key, 0) {
                    world.send(event);
                }
            }
        }
    }
    world.drafts = drafts;
}

#[then(expr = "the terminal received {string}")]
fn terminal_received(world: &mut VardeWorld, text: String) {
    assert_eq!(
        world.keys_sent,
        vec![(Pane::Terminal, text.into_bytes())],
        "keys sent: {:?}",
        world.keys_sent
    );
}

/// A key Varde did not claim, as the child received it: the bytes, pinned, so
/// a key that quietly stops reaching the shell fails here rather than reading
/// as a pass because something arrived.
#[then(expr = "the terminal program received the key {string}")]
fn terminal_received_key(world: &mut VardeWorld, key: String) {
    let expected: &[u8] = match key.as_str() {
        "F8" => b"\x1b[19~",
        other => panic!("no bytes are pinned for {other:?}"),
    };
    assert_eq!(
        world.keys_sent,
        vec![(Pane::Terminal, expected.to_vec())],
        "keys sent: {:?}",
        world.keys_sent
    );
}

/// The way out of a hosted pane is free only if the child still gets the key,
/// so the count and the pane are both the assertion.
#[then(expr = "the terminal received the escape twice")]
fn terminal_received_two_escapes(world: &mut VardeWorld) {
    assert_eq!(
        world.keys_sent,
        vec![(Pane::Terminal, vec![0x1b]), (Pane::Terminal, vec![0x1b])],
        "keys sent: {:?}",
        world.keys_sent
    );
}

#[then(expr = "the terminal received nothing")]
fn terminal_received_nothing(world: &mut VardeWorld) {
    assert!(
        world.keys_sent.is_empty(),
        "keys sent: {:?}",
        world.keys_sent
    );
}

#[given(expr = "no row is selected in the tree")]
fn nothing_selected(world: &mut VardeWorld) {
    world.state.tree_selection = None;
}

#[given(expr = "the tree selection is {string}")]
fn selection_is(world: &mut VardeWorld, path: String) {
    world.state.tree_selection = Some(abs(world, &path));
}

#[then(expr = "the tree selection is {string}")]
fn selection_should_be(world: &mut VardeWorld, path: String) {
    assert_eq!(world.state.tree_selection, Some(abs(world, &path)));
}

#[then("the row actions offered are:")]
fn row_actions_offered(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let selection = world.state.tree_selection.clone().expect("a selection");
    let actual: Vec<String> = tree::row_actions(&world.state, &selection)
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(actual, expected);
}

#[then(expr = "the row {string} offers no actions")]
fn row_offers_nothing(world: &mut VardeWorld, path: String) {
    let path = abs(world, &path);
    assert!(tree::row_actions(&world.state, &path).is_empty());
}

#[when(expr = "I click the {string} action on {string}")]
fn click_row_action(world: &mut VardeWorld, action: String, _row: String) {
    let action: &'static str = match action.as_str() {
        "new-file" => "new-file",
        "new-directory" => "new-directory",
        "go-here" => "go-here",
        "delete" => "delete",
        "copy-path" => "copy-path",
        "search-here" => "search-here",
        other => panic!("unknown action {other:?}"),
    };
    world.send(Event::RowAction(action));
}

// ---- F13: editing ----

/// What HEAD holds for the file, as the edge would have told the core.
#[given(expr = "the last commit holds {string} as:")]
fn commit_holds(world: &mut VardeWorld, path: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let absolute = abs(world, &path);
    world.state.committed.insert(absolute, Some(contents));
    world.tell_core();
}

#[then(expr = "lines {string} are marked as changed")]
fn lines_marked_changed(world: &mut VardeWorld, lines: String) {
    let expected: Vec<usize> = lines
        .split(", ")
        .map(|line| line.parse().expect("a line"))
        .collect();
    assert_eq!(varde::changed_lines(&world.state), expected);
}

#[then("no line is marked as changed")]
fn no_line_marked_changed(world: &mut VardeWorld) {
    assert!(varde::changed_lines(&world.state).is_empty());
}

/// The Authorship the edge read off the commit, one row per line of the file *as the
/// commit holds it* — the same shape and the same key as the commit's text
/// above, because the core is told both on the same poll and never reads git.
#[given(expr = "the last commit authored {string} as:")]
fn commit_authored(world: &mut VardeWorld, path: String, step: &Step) {
    let authors = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| varde::authorship::Authored {
            author: row[1].clone(),
            date: row[2].clone(),
        })
        .collect();
    let absolute = abs(world, &path);
    world.state.authorship.insert(absolute, authors);
}

#[then(expr = "the editor pane reports the line as authored by {string} on {string}")]
fn line_authored_by(world: &mut VardeWorld, author: String, date: String) {
    assert_eq!(
        varde::authorship::at_cursor(&world.state),
        Some(varde::authorship::Authorship::Committed(
            varde::authorship::Authored { author, date }
        ))
    );
}

#[then(expr = "the editor pane reports the line as {string}")]
fn line_authorship_is(world: &mut VardeWorld, state: String) {
    let reported = match varde::authorship::at_cursor(&world.state) {
        Some(varde::authorship::Authorship::NotCommittedYet) => "not-committed-yet",
        Some(varde::authorship::Authorship::Committed(_)) => "committed",
        None => "no-authorship",
    };
    assert_eq!(reported, state);
}

#[then(expr = "the editor pane reports no authorship")]
fn no_authorship(world: &mut VardeWorld) {
    assert_eq!(varde::authorship::at_cursor(&world.state), None);
}

#[given(expr = "{string} is open in the editor holding:")]
fn open_holding(world: &mut VardeWorld, path: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    open_buffer(world, &path, &contents);
}

fn current_buffer(world: &VardeWorld) -> &Buffer {
    let path = world
        .state
        .current_buffer
        .as_ref()
        .expect("a current buffer");
    world.state.buffers.get(path).expect("the buffer")
}

fn current_buffer_mut(world: &mut VardeWorld) -> &mut Buffer {
    let path = world
        .state
        .current_buffer
        .clone()
        .expect("a current buffer");
    world.state.buffers.get_mut(&path).expect("the buffer")
}

#[given(expr = "the editor mode is {word}")]
fn set_mode(world: &mut VardeWorld, mode: String) {
    match mode.as_str() {
        "insert" => world.send(Event::EditorKey('i')),
        "normal" => world.send(Event::EditorEscape),
        other => panic!("unknown mode {other:?}"),
    }
}

#[then(expr = "the editor mode is {word}")]
fn mode_should_be(world: &mut VardeWorld, mode: String) {
    assert_eq!(current_buffer(world).mode.as_str(), mode);
}

#[given(expr = "I press {string} in the editor")]
#[when(expr = "I press {string} in the editor")]
fn press_in_editor(world: &mut VardeWorld, keys: String) {
    // A key with a name rather than a spelling goes through the real router,
    // because what Escape, Enter and the arrows mean depends on what is on
    // screen: a candidate list claims them and the buffer answers them
    // otherwise, and a step that decided for itself would prove whichever it
    // picked instead of what the router does.
    if named_key(&keys).is_some() {
        return route_key(world, &keys, 0);
    }
    for key in keys.chars() {
        world.send(Event::EditorKey(key));
    }
}

/// The same key, held. Written as a count rather than as twelve steps because
/// what the Scenario is about is a selection running past the window, and
/// twelve identical lines would bury it.
#[given(expr = "I press {string} in the editor {int} times")]
#[when(expr = "I press {string} in the editor {int} times")]
fn press_in_editor_times(world: &mut VardeWorld, keys: String, times: usize) {
    for _ in 0..times {
        press_in_editor(world, keys.clone());
    }
}

#[given(expr = "I type {string} in the editor")]
#[when(expr = "I type {string} in the editor")]
fn type_in_editor(world: &mut VardeWorld, text: String) {
    for key in text.chars() {
        world.send(Event::EditorKey(key));
    }
}

#[then("the buffer holds:")]
fn buffer_holds(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    assert_eq!(current_buffer(world).shown(), expected);
}

/// `is_dirty` is the assertion that distinguishes refusing an edit from doing
/// it: a buffer a refused key left alone never grew a draft.
#[then(expr = "the buffer is unchanged")]
fn buffer_unchanged(world: &mut VardeWorld) {
    assert!(!current_buffer(world).is_dirty(), "buffer was edited");
}

#[then(expr = "the cursor is at line {int} column {int}")]
fn cursor_at(world: &mut VardeWorld, line: usize, column: usize) {
    let buffer = current_buffer(world);
    assert_eq!((buffer.line, buffer.column), (line, column));
}

#[given(expr = "the cursor is at line {int} column {int}")]
#[when(expr = "the cursor is at line {int} column {int}")]
fn place_cursor_at(world: &mut VardeWorld, line: usize, column: usize) {
    current_buffer_mut(world).go_to_place(Place { line, column });
}

#[then(expr = "the word under the cursor is marked at:")]
fn word_marked_at(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<Place> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| Place {
            line: row[0].parse().expect("a line"),
            column: row[1].parse().expect("a column"),
        })
        .collect();
    assert_eq!(varde::word_occurrences(&world.state, ..), expected);
}

#[then(expr = "no word is marked")]
fn no_word_marked(world: &mut VardeWorld) {
    assert_eq!(varde::word_occurrences(&world.state, ..), Vec::new());
}

#[then(expr = "{string} has unsaved edits")]
fn has_unsaved(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    assert!(world
        .state
        .buffers
        .get(&absolute)
        .expect("a buffer")
        .is_dirty());
}

#[then(expr = "{string} has no unsaved edits")]
fn has_no_unsaved(world: &mut VardeWorld, path: String) {
    // A file with no buffer open trivially has no unsaved edits, which is the
    // case when a read-only diff is what is on screen.
    let absolute = abs(world, &path);
    match world.state.buffers.get(&absolute) {
        Some(buffer) => assert!(!buffer.is_dirty()),
        None => assert!(world.state.current_buffer.is_none()),
    }
}

#[given(expr = "I write the buffer")]
#[when(expr = "I write the buffer")]
fn write_buffer(world: &mut VardeWorld) {
    world.send(Event::WriteBuffer);
}

#[when(expr = "I reload the buffer")]
fn reload_buffer(world: &mut VardeWorld) {
    world.send(Event::ReloadBuffer);
}

#[then(expr = "{string} was written with:")]
fn written_with(world: &mut VardeWorld, path: String, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    let written = world.files.get(&abs(world, &path)).expect("a written file");
    assert_eq!(written, expected);
}

// ---- F14: highlighting ----

#[given(expr = "{string} contains:")]
fn file_contains(world: &mut VardeWorld, path: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    world.files.insert(abs(world, &path), contents);
}

#[when(expr = "{string} is highlighted")]
fn highlight_file(world: &mut VardeWorld, path: String) {
    let source = world.files.get(&abs(world, &path)).expect("file").clone();
    world.highlighted = varde::highlight::highlight(&path, &source);
    world.highlighted_source = source;
}

fn kind_of(world: &VardeWorld, needle: &str) -> varde::highlight::Kind {
    world
        .highlighted
        .iter()
        .flatten()
        .find(|token| token.text.trim() == needle)
        .unwrap_or_else(|| panic!("no token {needle:?} in {:?}", world.highlighted))
        .kind
}

#[then(expr = "{string} is a keyword")]
fn is_keyword(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Keyword);
}

#[then(expr = "{string} is an operator")]
fn is_operator(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Operator);
}

#[then(expr = "{string} is a string")]
fn is_string(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::String);
}

#[then(expr = "{string} is a comment")]
fn is_comment(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Comment);
}

#[then(expr = "{string} is a function")]
fn is_function(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Function);
}

#[then(expr = "{string} is a type")]
fn is_type(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Type);
}

#[then(expr = "{string} is a number")]
fn is_number(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Number);
}

#[then(expr = "{string} is a constant")]
fn is_constant(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Constant);
}

#[then(expr = "{string} is a property")]
fn is_property(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Property);
}

#[then(expr = "{string} is an attribute")]
fn is_attribute(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Attribute);
}

#[then(expr = "{string} is punctuation")]
fn is_punctuation(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Punctuation);
}

#[then(expr = "{string} is markup")]
fn is_markup(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Markup);
}

#[then(expr = "{string} is invalid")]
fn is_invalid(world: &mut VardeWorld, text: String) {
    assert_eq!(kind_of(world, &text), varde::highlight::Kind::Invalid);
}

#[then(expr = "every token is plain text")]
fn all_plain(world: &mut VardeWorld) {
    assert!(world
        .highlighted
        .iter()
        .flatten()
        .all(|token| token.kind == varde::highlight::Kind::Plain));
}

/// A unified diff of two texts at Varde's pinned options, built the way the
/// edge builds one — `git2` over the two buffers, the same origins kept and
/// the same trailing whitespace trimmed. A scenario that hand-wrote its rows
/// would be asserting against its own idea of a diff.
fn unified(name: &str, old: &str, new: &str) -> Vec<DiffLine> {
    let mut options = git2::DiffOptions::new();
    options.context_lines(story::CONTEXT_LINES);
    let path = Path::new(name);
    let patch = git2::Patch::from_buffers(
        old.as_bytes(),
        Some(path),
        new.as_bytes(),
        Some(path),
        Some(&mut options),
    )
    .expect("a patch");
    let mut lines = Vec::new();
    for hunk in 0..patch.num_hunks() {
        let (_, count) = patch.hunk(hunk).expect("a hunk");
        for index in 0..count {
            let line = patch.line_in_hunk(hunk, index).expect("a line");
            if matches!(line.origin(), '+' | '-' | ' ') {
                lines.push(DiffLine {
                    new_line: line.new_lineno().map(|n| n as usize),
                    old_line: line.old_lineno().map(|n| n as usize),
                    removed: line.origin() == '-',
                    text: String::from_utf8_lossy(line.content())
                        .trim_end()
                        .to_string(),
                });
            }
        }
    }
    lines
}

/// The whole of what the edge does when it reads a diff: diff the two sides,
/// highlight each of them whole, and hand the rows to the core. A file the
/// scenario never said HEAD held has no old side at all, which is the case a
/// removed row cannot be coloured from.
#[given(expr = "the diff for {string} is shown against HEAD")]
fn show_diff_against_head(world: &mut VardeWorld, file: String) {
    let full = world.state.root.join(&file);
    let old = world.held.get(&full).cloned();
    let new = world.files.get(&full).cloned().unwrap_or_default();
    let name = full
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let lines = unified(&name, old.as_deref().unwrap_or_default(), &new);
    world.diff_sides = (
        varde::highlight::highlight(&name, &new),
        old.map(|text| varde::highlight::highlight(&name, &text))
            .unwrap_or_default(),
    );
    let revision = blob_oid(&new);
    world.diff_contents.insert(file.clone(), new);
    world.send(Event::ShowDiff {
        file,
        lines,
        revision,
    });
}

#[when(expr = "diff row {int} is highlighted")]
fn diff_row_highlighted(world: &mut VardeWorld, row: usize) {
    let diff = world.state.diff.clone().expect("a diff on screen");
    let tokens = review::diff_tokens(&diff, &world.diff_sides.0, &world.diff_sides.1);
    world.highlighted = tokens[row - 1]
        .map(|line| vec![line.to_vec()])
        .unwrap_or_default();
    world.highlighted_source = diff[row - 1].text.clone();
}

#[then(expr = "the diff row carries no tokens of its own")]
fn diff_row_uncoloured(world: &mut VardeWorld) {
    assert!(
        world.highlighted.is_empty(),
        "{:?} was coloured from somewhere",
        world.highlighted
    );
}

#[then(expr = "the highlighted tokens reassemble to the original line")]
fn tokens_reassemble(world: &mut VardeWorld) {
    let joined = world
        .highlighted
        .iter()
        .map(|line| line.iter().map(|token| token.text.as_str()).collect())
        .collect::<Vec<String>>()
        .join("\n");
    assert_eq!(joined, world.highlighted_source);
}

#[then(expr = "the selected lines are {int} to {int}")]
fn selected_lines(world: &mut VardeWorld, from: usize, to: usize) {
    assert_eq!(current_buffer(world).selected_lines(), Some((from, to)));
}

// ---- F15: quitting ----

#[given(expr = "I quit")]
#[when(expr = "I quit")]
fn quit(world: &mut VardeWorld) {
    world.send(Event::Quit);
}

#[when(expr = "I force quit")]
fn force_quit(world: &mut VardeWorld) {
    world.send(Event::QuitForce);
}

#[then(expr = "Varde exits")]
fn varde_exits(world: &mut VardeWorld) {
    assert!(world.exited);
}

#[then(expr = "Varde is still running")]
fn varde_still_running(world: &mut VardeWorld) {
    assert!(!world.exited);
}

#[then(expr = "the project state was saved")]
fn state_saved(world: &mut VardeWorld) {
    assert!(world.startup.state_json.is_some());
}

#[then(expr = "the reviewer is told there are unsaved changes")]
fn told_unsaved(world: &mut VardeWorld) {
    assert!(world.notices.contains(&"unsaved-changes".to_string()));
}

// ---- F16: selection and clipboard ----

#[given(expr = "a system clipboard is available")]
fn clipboard_available(world: &mut VardeWorld) {
    world.state.system_clipboard = true;
}

#[given(expr = "no system clipboard is available")]
fn clipboard_unavailable(world: &mut VardeWorld) {
    world.state.system_clipboard = false;
}

#[given("the terminal shows:")]
#[when("the terminal shows:")]
fn terminal_shows(world: &mut VardeWorld, step: &Step) {
    world.screen = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .split('\n')
        .map(str::to_string)
        .collect();
}

#[given("the AI session shows:")]
#[when("the AI session shows:")]
fn ai_session_shows(world: &mut VardeWorld, step: &Step) {
    world.ai_screen = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .split('\n')
        .map(str::to_string)
        .collect();
}

#[given(expr = "I drag across {string} in the {word} pane")]
#[when(expr = "I drag across {string} in the {word} pane")]
fn drag_in_pane(world: &mut VardeWorld, text: String, pane: String) {
    let pane = parse_pane(&pane);
    let lines = world.pane_lines(pane);
    let index = lines
        .iter()
        .position(|line| line.contains(&text))
        .unwrap_or_else(|| panic!("{text:?} is not in that pane"));
    let at = lines[index].find(&text).expect("the column");
    let column = lines[index][..at].chars().count() + 1;
    let last = column + text.chars().count() - 1;
    world.drag(pane, (index + 1, column), (index + 1, last));
}

#[given(expr = "I drag in the {word} pane from line {int} column {int} to line {int} column {int}")]
#[when(expr = "I drag in the {word} pane from line {int} column {int} to line {int} column {int}")]
fn drag_span(
    world: &mut VardeWorld,
    pane: String,
    from_line: usize,
    from_column: usize,
    to_line: usize,
    to_column: usize,
) {
    let pane = parse_pane(&pane);
    world.drag(pane, (from_line, from_column), (to_line, to_column));
}

/// The button going down where a scenario names a place, which is what starts
/// a drag: the pane the press lands in is the pane the drag belongs to for as
/// long as it is held.
// Spelled out because `{word}` is one word and the tree's name is two — the
// same reason "the file tree pane has focus" has a step of its own above.
#[when(expr = "I press at line {int} column {int} in the file tree pane")]
fn press_at_in_tree(world: &mut VardeWorld, line: usize, column: usize) {
    press_at(world, line, column, "file tree".to_string());
}

#[when(expr = "I drag past the {word} of the file tree pane")]
fn drag_past_tree(world: &mut VardeWorld, side: String) {
    drag_past(world, side, "file tree".to_string());
}

#[when(expr = "I press at line {int} column {int} in the {word} pane")]
fn press_at(world: &mut VardeWorld, line: usize, column: usize, pane: String) {
    let pane = parse_pane(&pane);
    world.pointer = mouse::Pointer::default();
    let (at_column, at_row) = pointer_at(&world.state, &world.panes(), pane, (line, column));
    world.report(mouse::Kind::LeftDown, at_column, at_row);
}

#[when(expr = "I drag to line {int} column {int} in the {word} pane")]
fn drag_to(world: &mut VardeWorld, line: usize, column: usize, pane: String) {
    let pane = parse_pane(&pane);
    let (at_column, at_row) = pointer_at(&world.state, &world.panes(), pane, (line, column));
    world.report(mouse::Kind::LeftDrag, at_column, at_row);
}

/// The pointer taken off the pane entirely, one cell past the border on the
/// side named — which is where a neighbouring pane begins, so this is also what
/// proves the drag stays with the pane it started in.
#[when(expr = "I drag past the {word} of the {word} pane")]
fn drag_past(world: &mut VardeWorld, side: String, pane: String) {
    let area = match parse_pane(&pane) {
        Pane::Tree => world.panes().tree,
        Pane::Editor => world.panes().editor,
        Pane::Ai => world.panes().ai,
        Pane::Output => world.panes().output,
        Pane::Terminal | Pane::Variables => world.panes().terminal,
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics => world.panes().corner,
        Pane::Evaluator => world.panes().evaluator,
    };
    // Straight out from where the button went down, which is the gesture a
    // person makes: aiming at the middle of the pane instead would move the
    // other axis too and hide which edge the scroll answered.
    let (at_column, at_row) = world.pointer.drag_from.expect("a button is down");
    let (column, row) = match side.as_str() {
        "bottom" => (at_column, area.bottom()),
        "top" => (at_column, area.y.saturating_sub(1)),
        "left" => (area.x.saturating_sub(1), at_row),
        "right" => (area.right(), at_row),
        "bottom-right" => (area.right(), area.bottom()),
        other => panic!("no {other:?} side of a pane"),
    };
    world.report(mouse::Kind::LeftDrag, column, row);
}

/// The cadence the edge replays a held drag on, fired by hand: no clock, and
/// nothing moves in between, which is the case the feature exists for.
#[when(expr = "I hold the drag still for {int} ticks")]
fn hold_drag(world: &mut VardeWorld, ticks: usize) {
    for _ in 0..ticks {
        let held = world.pointer.held.expect("a drag is held");
        world.report(held.kind, held.column, held.row);
    }
}

#[when("I release the mouse")]
fn release_mouse(world: &mut VardeWorld) {
    let held = world
        .pointer
        .held
        .or_else(|| {
            world.pointer.drag_from.map(|(column, row)| mouse::Input {
                kind: mouse::Kind::LeftUp,
                column,
                row,
                modifiers: terminput::KeyModifiers::NONE,
            })
        })
        .expect("a button is down");
    world.report(mouse::Kind::LeftUp, held.column, held.row);
}

#[then("no drag is held")]
fn no_drag_is_held(world: &mut VardeWorld) {
    assert_eq!(world.pointer.held, None);
}

#[when(expr = "I drag across the row {string} in the file tree pane")]
fn drag_row(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    let index = tree::visible_rows(&world.state)
        .iter()
        .position(|row| row.path == absolute)
        .unwrap_or_else(|| panic!("no row for {path:?}"));
    let line = index + 1 + world.state.tree_scroll;
    world.drag(Pane::Tree, (line, 1), (line, 3));
}

// `markdown_preview.feature`'s own wording for the same assertion — stacked
// rather than a duplicate function, the way `add_issue`/`add_typed` already do
// it in this file.
#[then(expr = "the selection holds {string}")]
#[then(expr = "the selection is {string}")]
fn selection_holds(world: &mut VardeWorld, text: String) {
    assert_eq!(world.state.selected_text(), Some(text));
}

/// Drags across whichever Preview row holds `text` — a row, never a source
/// line, since markup is consumed and a heading's row is not its line. The
/// drag resolves in-core: `mouse::dragged` never hands the editor pane back as
/// a `Selection` request, so nothing here plays the edge the way `drag` does
/// for a pty.
#[given(expr = "I drag across the row holding {string}")]
#[when(expr = "I drag across the row holding {string}")]
fn drag_preview_row(world: &mut VardeWorld, text: String) {
    let rows = preview_rows(world);
    let index = rows
        .iter()
        .position(|row| row.text().contains(&text))
        .unwrap_or_else(|| panic!("no row holds {text:?}"));
    let row_text = rows[index].text();
    let at = row_text.find(&text).expect("the column");
    let column = row_text[..at].chars().count() + 1;
    let last = column + text.chars().count() - 1;
    world.drag(Pane::Editor, (index + 1, column), (index + 1, last));
}

/// Where a word sits in the editor: a Preview's row while previewing, and a
/// source line otherwise — the same distinction the drag steps make, since a
/// heading's row is not its line.
fn word_at(world: &mut VardeWorld, text: &str) -> (usize, usize) {
    let lines: Vec<String> = match varde::previewing(&world.state) {
        true => preview_rows(world)
            .iter()
            .map(varde::preview::Row::text)
            .collect(),
        false => world.pane_lines(Pane::Editor),
    };
    let index = lines
        .iter()
        .position(|line| line.contains(text))
        .unwrap_or_else(|| panic!("{text:?} is not in the editor"));
    let at = lines[index].find(text).expect("the column");
    (index + 1, lines[index][..at].chars().count() + 1)
}

#[when(expr = "I double-click on {string} in the editor")]
fn double_click_word(world: &mut VardeWorld, text: String) {
    let at = word_at(world, &text);
    world.click_twice(Pane::Editor, at, 0);
}

#[when(expr = "I double-click at line {int} column {int} in the editor")]
fn double_click_at(world: &mut VardeWorld, line: usize, column: usize) {
    world.click_twice(Pane::Editor, (line, column), 0);
}

#[when(expr = "I click twice on {string} in the editor {int}ms apart")]
fn click_twice_apart(world: &mut VardeWorld, text: String, apart: u64) {
    let at = word_at(world, &text);
    world.click_twice(Pane::Editor, at, apart);
}

#[then("the selection holds:")]
fn selection_holds_lines(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    assert_eq!(world.state.selected_text().as_deref(), Some(expected));
}

#[then("the other occurrences picked are:")]
fn occurrences_picked(world: &mut VardeWorld, step: &Step) {
    let rows = &step.table().expect("table").rows;
    let expected: Vec<(usize, usize)> = rows[1..]
        .iter()
        .map(|row| {
            (
                row[0].parse().expect("line"),
                row[1].parse().expect("column"),
            )
        })
        .collect();
    let picked: Vec<(usize, usize)> = world
        .state
        .occurrences
        .iter()
        .map(|place| (place.line, place.column))
        .collect();
    assert_eq!(picked, expected);
}

#[then(expr = "no other occurrence is picked")]
fn no_occurrence_picked(world: &mut VardeWorld) {
    assert!(world.state.occurrences.is_empty());
}

#[then(expr = "the selection holds nothing")]
fn selection_empty(world: &mut VardeWorld) {
    assert!(world.state.selection.is_none());
}

#[then("the clipboard holds:")]
fn clipboard_holds_lines(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    assert_eq!(world.clipboard.as_deref(), Some(expected));
}

/// A linewise span's text ends with the break that ends its last line, which
/// the two docstring assertions above cannot express: cucumber trims the
/// newlines around a docstring, so an end that was quietly dropped would still
/// read as a pass. These add it back.
#[given("the selection holds the lines:")]
#[then("the selection holds the lines:")]
fn selection_holds_whole_lines(world: &mut VardeWorld, step: &Step) {
    let expected = format!(
        "{}\n",
        step.docstring().expect("docstring").trim_matches('\n')
    );
    assert_eq!(
        world.state.selected_text().as_deref(),
        Some(expected.as_str())
    );
}

#[then("the clipboard holds the lines:")]
fn clipboard_holds_whole_lines(world: &mut VardeWorld, step: &Step) {
    let expected = format!(
        "{}\n",
        step.docstring().expect("docstring").trim_matches('\n')
    );
    assert_eq!(world.clipboard.as_deref(), Some(expected.as_str()));
}

#[then(expr = "the clipboard holds nothing")]
fn clipboard_empty(world: &mut VardeWorld) {
    assert!(world.clipboard.is_none());
}

#[then(expr = "the terminal was asked to hold {string}")]
fn terminal_clipboard(world: &mut VardeWorld, text: String) {
    assert_eq!(world.terminal_clipboard.as_deref(), Some(text.as_str()));
}

// ---- F17: reaching the review flow ----

#[given(expr = "the diff for {string} is shown")]
#[when(expr = "the diff for {string} is shown")]
fn show_diff(world: &mut VardeWorld, file: String) {
    let lines = (1..=3)
        .map(|number| varde::DiffLine {
            new_line: Some(number),
            old_line: None,
            removed: false,
            text: format!("line {number}"),
        })
        .collect();
    world.send(Event::ShowDiff {
        file,
        lines,
        revision: "mock-revision".to_string(),
    });
}

/// A diff of one row, as wide as the scenario needs — the case the sideways
/// gesture exists for, since the mock diff above holds nothing that reaches the
/// pane's right edge.
#[given(expr = "the diff for {string} is shown holding a {int}-character line")]
fn show_wide_diff(world: &mut VardeWorld, file: String, width: usize) {
    world.send(Event::ShowDiff {
        file,
        lines: vec![varde::DiffLine {
            new_line: Some(1),
            old_line: None,
            removed: false,
            text: "x".repeat(width),
        }],
        revision: "mock-revision".to_string(),
    });
}

#[then(expr = "the diff for {string} is shown")]
fn diff_is_shown(world: &mut VardeWorld, file: String) {
    assert_eq!(world.state.diff_file.as_deref(), Some(file.as_str()));
}

#[then(expr = "the diff for {string} was re-read")]
fn diff_re_read(world: &mut VardeWorld, file: String) {
    assert_eq!(world.diffs_read, vec![abs(world, &file)]);
}

#[then(expr = "no diff was re-read")]
fn no_diff_re_read(world: &mut VardeWorld) {
    assert!(world.diffs_read.is_empty());
}

#[then(expr = "the diff cursor is on line {int}")]
fn diff_cursor(world: &mut VardeWorld, line: usize) {
    assert_eq!(world.state.diff_line, line);
}

#[then(expr = "the comment picker is shown")]
fn picker_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.modal, Modal::Comment);
}

#[then(expr = "the comment picker is not shown")]
fn picker_not_shown(world: &mut VardeWorld) {
    assert_ne!(world.state.modal, Modal::Comment);
}

#[given(expr = "I submit the review from the command line")]
#[when(expr = "I submit the review from the command line")]
fn submit_from_command_line(world: &mut VardeWorld) {
    world.send(Event::SubmitReview);
}

#[given(expr = "I press the {word} arrow in the editor")]
#[when(expr = "I press the {word} arrow in the editor")]
fn press_arrow(world: &mut VardeWorld, direction: String) {
    world.send(Event::EditorArrow(arrow(&direction)));
}

fn arrow(direction: &str) -> Direction {
    match direction {
        "Up" => Direction::Up,
        "Down" => Direction::Down,
        "Left" => Direction::Left,
        "Right" => Direction::Right,
        other => panic!("unknown arrow {other:?}"),
    }
}

#[given(expr = "I hold shift and press the {word} arrow in the editor")]
#[when(expr = "I hold shift and press the {word} arrow in the editor")]
fn extend_selection(world: &mut VardeWorld, direction: String) {
    world.send(Event::EditorExtend(arrow(&direction)));
}

#[given(expr = "I hold alt and press the {word} arrow in the editor")]
#[when(expr = "I hold alt and press the {word} arrow in the editor")]
fn word_motion(world: &mut VardeWorld, direction: String) {
    world.send(Event::EditorWord(arrow(&direction)));
}

#[given(expr = "I hold shift and alt and press the {word} arrow in the editor")]
#[when(expr = "I hold shift and alt and press the {word} arrow in the editor")]
fn extend_selection_by_word(world: &mut VardeWorld, direction: String) {
    world.send(Event::EditorExtendWord(arrow(&direction)));
}

#[given(expr = "I hold shift and press the {word} arrow in the editor {int} times")]
#[when(expr = "I hold shift and press the {word} arrow in the editor {int} times")]
fn extend_selection_times(world: &mut VardeWorld, direction: String, times: usize) {
    for _ in 0..times {
        world.send(Event::EditorExtend(arrow(&direction)));
    }
}

#[when(expr = "I press Backspace in the editor")]
fn press_backspace(world: &mut VardeWorld) {
    world.send(Event::EditorBackspace);
}

#[then(expr = "the pending command shows {string}")]
fn pending_command_shows(world: &mut VardeWorld, expected: String) {
    assert_eq!(current_buffer(world).pending_command(), expected);
}

#[when(expr = "I switch to Edit view")]
#[given(expr = "I open Edit view")]
#[when(expr = "I open Edit view")]
fn switch_to_edit(world: &mut VardeWorld) {
    pick_palette_entry(world, "Edit");
}

#[then(expr = "no diff is shown")]
fn no_diff_shown(world: &mut VardeWorld) {
    assert!(world.state.diff.is_none() && world.state.diff_file.is_none());
}

#[given(expr = "I start the AI")]
#[when(expr = "I start the AI")]
fn start_ai(world: &mut VardeWorld) {
    world.send(Event::StartAi {
        command: None,
        force: false,
    });
}

/// The one scenario that names exact bytes, because the reported symptom is
/// exactly that a modifier went missing: the old path sent a bare carriage
/// return for Option+Enter, which is why the AI submitted the prompt instead of
/// growing a line. Non-empty bytes would not have caught that. `\e` is the
/// escape byte the Alt prefix is made of.
#[then(expr = "the AI received the bytes {string}")]
fn ai_received_the_bytes(world: &mut VardeWorld, bytes: String) {
    let expected: Vec<u8> = bytes
        .replace("\\e", "\x1b")
        .replace("\\r", "\r")
        .replace("\\n", "\n")
        .into_bytes();
    assert_eq!(world.keys_sent, vec![(Pane::Ai, expected)]);
}

#[then(expr = "the AI received nothing")]
fn ai_received_nothing(world: &mut VardeWorld) {
    assert!(
        !world.keys_sent.iter().any(|(pane, _)| *pane == Pane::Ai),
        "keys sent: {:?}",
        world.keys_sent
    );
}

#[then(expr = "the AI received {string}")]
fn ai_received(world: &mut VardeWorld, text: String) {
    assert_eq!(world.keys_sent, vec![(Pane::Ai, text.into_bytes())]);
}

#[given(expr = "I start the AI with {string}")]
#[when(expr = "I start the AI with {string}")]
fn start_named_ai(world: &mut VardeWorld, command: String) {
    world.send(Event::StartAi {
        command: Some(command),
        force: false,
    });
}

#[given(expr = "I force the AI to {string}")]
#[when(expr = "I force the AI to {string}")]
fn force_ai(world: &mut VardeWorld, command: String) {
    world.send(Event::StartAi {
        command: Some(command),
        force: true,
    });
}

#[then(expr = "the AI session was stopped")]
fn ai_stopped(world: &mut VardeWorld) {
    assert!(world.ai_stopped);
}

/// The absence: authoring a Story set for a repository the session knows
/// nothing about must not cost the reviewer the session they already had.
#[then(expr = "no AI session was stopped")]
fn ai_not_stopped(world: &mut VardeWorld) {
    assert!(!world.ai_stopped, "the AI session was stopped");
}

#[given(expr = "the remembered AI command is {string}")]
fn remember_ai_command(world: &mut VardeWorld, command: String) {
    world.state.ai_command = command;
}

#[then(expr = "the remembered AI command is {string}")]
fn ai_command_remembered(world: &mut VardeWorld, command: String) {
    let saved = world
        .startup
        .state_json
        .as_deref()
        .expect("state was saved");
    assert!(
        saved.contains(&format!("\"ai_command\":\"{command}\"")),
        "state was: {saved}"
    );
}

#[then(expr = "the reviewer is told the AI is already running")]
fn told_ai_running(world: &mut VardeWorld) {
    assert!(world.notices.contains(&"ai-already-running".to_string()));
}

#[then(expr = "the reviewer is not told the AI is already running")]
fn not_told_ai_running(world: &mut VardeWorld) {
    assert!(!world.notices.contains(&"ai-already-running".to_string()));
}

#[then(expr = "only one AI session was started")]
fn one_ai_session(world: &mut VardeWorld) {
    assert_eq!(world.ai_spawned.len(), 1, "spawned: {:?}", world.ai_spawned);
}

#[then(expr = "the AI pane is asking which CLI to start")]
fn ai_pane_asking(world: &mut VardeWorld) {
    assert!(!world.state.ai_running);
}

#[then(expr = "the AI pane is not asking which CLI to start")]
fn ai_pane_not_asking(world: &mut VardeWorld) {
    assert!(world.state.ai_running);
}

#[given(expr = "the AI session exits")]
#[when(expr = "the AI session exits")]
fn ai_exits(world: &mut VardeWorld) {
    world.ai_is_gone();
    world.tell_core();
}

#[then(expr = "the reviewer is told the comment was added")]
fn told_comment_added(world: &mut VardeWorld) {
    assert!(world.notices.contains(&"comment-added".to_string()));
}

#[then(expr = "the diff shows the comment {string} against line {int}")]
fn diff_shows_comment(world: &mut VardeWorld, body: String, line: u32) {
    let file = world.state.diff_file.clone().expect("a diff");
    let found = review::comments_at(&world.state, &file, line);
    assert!(
        found.iter().any(|comment| comment.body == body),
        "comments at line {line}: {found:?}"
    );
}

#[then(expr = "the diff shows no comment against line {int}")]
fn diff_shows_no_comment(world: &mut VardeWorld, line: u32) {
    let file = world.state.diff_file.clone().expect("a diff");
    assert!(review::comments_at(&world.state, &file, line).is_empty());
}

#[given(expr = "I close the buffer")]
#[when(expr = "I close the buffer")]
fn close_buffer(world: &mut VardeWorld) {
    world.send(Event::CloseBuffer { force: false });
}

#[when(expr = "I force close the buffer")]
fn force_close_buffer(world: &mut VardeWorld) {
    world.send(Event::CloseBuffer { force: true });
}

/// Through the `:` line rather than the event behind it, so the command that
/// spells the gesture is what these Scenarios exercise: an event sent by hand
/// goes green over a command nobody can type.
#[when(expr = "I close every clean buffer")]
fn close_clean_buffers(world: &mut VardeWorld) {
    run_story_command(world, ":qa".to_string());
}

#[when(expr = "I force close every buffer")]
fn force_close_every_buffer(world: &mut VardeWorld) {
    run_story_command(world, ":qa!".to_string());
}

#[then(expr = "the editor has no file open")]
fn no_file_open(world: &mut VardeWorld) {
    assert!(world.state.current_buffer.is_none() && world.state.buffers.is_empty());
}

#[then(expr = "the editor shows no buffer")]
fn no_buffer_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.current_buffer, None);
}

/// The command line is a draft the edge holds, not core state, so a scenario
/// asks the drafts the router was handed.
#[then(expr = "the command line is open")]
fn command_line_open(world: &mut VardeWorld) {
    assert_eq!(world.drafts.command.as_deref(), Some(""));
}

#[then(expr = "the command line is not open")]
fn command_line_closed(world: &mut VardeWorld) {
    assert_eq!(world.drafts.command, None);
}

// ---- F19: several buffers ----

#[given(expr = "I open {string}")]
#[when(expr = "I open {string}")]
fn open_named(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    // What the edge reads off disk: whatever the scenario put there, and a
    // stand-in for a file it never described.
    let contents = world
        .files
        .get(&absolute)
        .cloned()
        .unwrap_or_else(|| "contents".to_string());
    world.send(Event::BufferOpened {
        contents,
        path: absolute,
        preview: false,
        at: None,
    });
}

#[given(expr = "{string} is previewed")]
#[when(expr = "{string} is previewed")]
fn preview_named(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
    world.send(Event::BufferOpened {
        contents: "contents".to_string(),
        path: absolute,
        preview: true,
        at: None,
    });
}

#[then("the open buffers are:")]
fn open_buffers_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<PathBuf> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| world.state.root.join(&row[0]))
        .collect();
    let actual: Vec<PathBuf> = world.state.buffers.keys().cloned().collect();
    assert_eq!(actual, expected);
}

#[given(expr = "the current buffer is {string}")]
#[then(expr = "the current buffer is {string}")]
fn current_buffer_is(world: &mut VardeWorld, path: String) {
    assert_eq!(world.state.current_buffer, Some(abs(world, &path)));
}

#[given(expr = "I click buffer dot {int}")]
#[when(expr = "I click buffer dot {int}")]
fn click_dot(world: &mut VardeWorld, index: usize) {
    let path = world
        .state
        .buffers
        .keys()
        .nth(index - 1)
        .cloned()
        .expect("a buffer");
    world.send(Event::ShowBuffer(path));
}

#[then(expr = "the buffer mark for {string} is {string}")]
fn buffer_mark_is(world: &mut VardeWorld, path: String, expected: String) {
    let actual = match varde::mark(&world.state, &abs(world, &path)) {
        varde::Mark::None => "none",
        varde::Mark::Open => "open",
        varde::Mark::Dirty => "dirty",
        varde::Mark::Current => "current",
        varde::Mark::CurrentDirty => "current-dirty",
    };
    assert_eq!(actual, expected);
}

// ---- F20: filtering the tree ----

#[given("the project contains:")]
fn project_contains(world: &mut VardeWorld, step: &Step) {
    let files: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    world.indexable = files.clone();
    world.send(Event::Indexed(files));
}

#[given(expr = "the project gains {string}")]
fn project_gains(world: &mut VardeWorld, path: String) {
    // Only on disk: whether the core comes to know about it is the behaviour
    // under test.
    world.indexable.push(path);
}

#[given(expr = "I filter by {string}")]
#[when(expr = "I filter by {string}")]
fn filter_by(world: &mut VardeWorld, text: String) {
    world.send(Event::Filter(text));
}

#[then("the filtered files are:")]
fn filtered_files_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let mut actual = varde::filter::matches(&world.state);
    let mut sorted = expected.clone();
    actual.sort();
    sorted.sort();
    assert_eq!(actual, sorted);
}

#[then(expr = "the filtered files include {string}")]
fn filtered_include(world: &mut VardeWorld, path: String) {
    assert!(varde::filter::matches(&world.state).contains(&path));
}

#[then(expr = "the filtered files are empty")]
fn filtered_empty(world: &mut VardeWorld) {
    assert!(varde::filter::matches(&world.state).is_empty());
}

#[then(expr = "the best match is {string}")]
#[then(expr = "the completion is {string}")]
fn best_match_is(world: &mut VardeWorld, path: String) {
    assert_eq!(
        varde::filter::best(&world.state).as_deref(),
        Some(path.as_str())
    );
}

/// What the pane draws, not what the tree remembers. A filtered row is shown
/// open so its match is visible without the filter expanding the tree behind
/// it — conflating the two is what left every folder a match sat under
/// standing open once the filter was cleared.
#[then(expr = "the row {string} is expanded")]
fn row_is_expanded(world: &mut VardeWorld, path: String) {
    let wanted = abs(world, &path);
    let row = tree::visible_rows(&world.state)
        .into_iter()
        .find(|row| row.path == wanted)
        .expect("a row");
    assert!(row.expanded);
}

#[then(expr = "the tree is not filtered")]
fn tree_not_filtered(world: &mut VardeWorld) {
    assert!(world.state.filter.is_empty());
}

#[when(expr = "I accept the filter")]
fn accept_filter(world: &mut VardeWorld) {
    world.send(Event::AcceptFilter);
}

#[then(expr = "the tree filter state is {string}")]
fn tree_filter_state(world: &mut VardeWorld, expected: String) {
    assert_eq!(varde::filter::view_state(&world.state), expected);
}

// ---- F21: content search ----

#[given("the project holds:")]
fn project_holds(world: &mut VardeWorld, step: &Step) {
    world.project = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| (row[0].clone(), row[1].replace("\\n", "\n")))
        .collect();
    let names = world.project.iter().map(|(name, _)| name.clone()).collect();
    world.send(Event::Indexed(names));
}

#[given(expr = "I open search")]
#[when(expr = "I open search")]
fn open_search(world: &mut VardeWorld) {
    world.send(Event::OpenSearch);
}

#[given(expr = "I open search in {string}")]
fn open_search_in(world: &mut VardeWorld, folder: String) {
    world.send(Event::Trigger(
        varde::tree_actions::Action::SearchHere,
        Some(varde::tree_actions::Target::Folder(folder.into())),
    ));
}

#[then(expr = "the search is scoped to {string}")]
fn search_is_scoped_to(world: &mut VardeWorld, folder: String) {
    assert_eq!(search(world).scope.as_deref(), Some(Path::new(&folder)));
}

#[then("the search is scoped to the whole project")]
fn search_is_unscoped(world: &mut VardeWorld) {
    assert_eq!(search(world).scope, None);
}

#[when(expr = "I close search")]
fn close_search(world: &mut VardeWorld) {
    world.send(Event::CloseSearch);
}

#[then(expr = "the search is open")]
fn search_is_open(world: &mut VardeWorld) {
    assert!(world.state.search.is_some());
}

#[then(expr = "the search is not open")]
fn search_is_closed(world: &mut VardeWorld) {
    assert!(world.state.search.is_none());
}

#[given(expr = "I search for {string}")]
#[when(expr = "I search for {string}")]
fn search_for(world: &mut VardeWorld, query: String) {
    world.send(Event::SearchQuery(query));
}

fn search(world: &VardeWorld) -> &varde::Search {
    world.state.search.as_ref().expect("search is open")
}

#[then(expr = "the search query is {string}")]
fn search_query_is(world: &mut VardeWorld, expected: String) {
    assert_eq!(search(world).query.shown(), expected);
}

/// Key by key through the router, as the box receives them: a whole query sent
/// at once could not land in the middle of one already typed.
#[given(expr = "I type {string} into the search")]
#[when(expr = "I type {string} into the search")]
fn type_into_search(world: &mut VardeWorld, query: String) {
    for key in query.chars() {
        route_key(world, &key.to_string(), 0);
    }
}

#[then(expr = "there are no hits")]
fn no_hits(world: &mut VardeWorld) {
    assert!(search(world).results.hits.is_empty());
}

#[then(expr = "{int} hits were found")]
fn hit_count(world: &mut VardeWorld, expected: usize) {
    let hits = &search(world).results.hits;
    assert_eq!(hits.len(), expected, "hits: {hits:?}");
}

#[then("the hits are:")]
fn hits_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, u32)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| (row[0].clone(), row[1].parse().expect("a line number")))
        .collect();
    let actual: Vec<(String, u32)> = search(world)
        .results
        .hits
        .iter()
        .map(|hit| (hit.file.clone(), hit.line))
        .collect();
    assert_eq!(actual, expected);
}

#[then("the hit files in order are:")]
fn hit_files_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(varde::search::files(&search(world).results), expected);
}

#[given(expr = "I move down in the search")]
#[when(expr = "I move down in the search")]
fn move_down_in_search(world: &mut VardeWorld) {
    world.send(Event::MoveHit(Direction::Down));
}

#[given(expr = "I move up in the search")]
#[when(expr = "I move up in the search")]
fn move_up_in_search(world: &mut VardeWorld) {
    world.send(Event::MoveHit(Direction::Up));
}

#[given(expr = "I jump to the next file in the search")]
#[when(expr = "I jump to the next file in the search")]
fn next_file_in_search(world: &mut VardeWorld) {
    world.send(Event::MoveHitFile(Direction::Down));
}

#[when(expr = "I jump to the previous file in the search")]
fn previous_file_in_search(world: &mut VardeWorld) {
    world.send(Event::MoveHitFile(Direction::Up));
}

/// A click on a row of the box as it is drawn, 1-based, routed through the
/// mouse the way the edge routes one: the box is not a pane, so the row is
/// turned into a screen position off the box's own rectangle, and `mouse`
/// resolves it back into whatever the row holds. Row 6 of a box that draws 5 is
/// its bottom border — the chrome scenario relies on that.
#[given(expr = "I click result row {int}")]
#[when(expr = "I click result row {int}")]
fn click_result_row(world: &mut VardeWorld, row: u16) {
    let (width, height) = world.screen();
    let area = layout::search_box(width, height);
    let panes = world.panes();
    let mut pointer = mouse::Pointer::default();
    for kind in [mouse::Kind::LeftDown, mouse::Kind::LeftUp] {
        let outcome = mouse::on_mouse(
            &world.state,
            &panes,
            &mut pointer,
            mouse::Input {
                kind,
                column: area.x + 3,
                row: area.y + 1 + layout::SEARCH_HEADER + row - 1,
                modifiers: terminput::KeyModifiers::NONE,
            },
        );
        for event in outcome.events {
            world.send(event);
        }
    }
}

/// The first row of the list the box shows, 1-based — rows and not hits,
/// because the files are headings between them.
#[then(expr = "the results start at row {int}")]
fn results_start_at_row(world: &mut VardeWorld, row: usize) {
    assert_eq!(search(world).scroll + 1, row);
}

#[then(expr = "the selected hit is {string} line {int}")]
fn selected_hit_is(world: &mut VardeWorld, file: String, line: u32) {
    let search = search(world);
    let hit = search.results.hits.get(search.selected).expect("a hit");
    assert_eq!((hit.file.as_str(), hit.line), (file.as_str(), line));
}

#[when(expr = "I open the selected hit")]
fn open_selected_hit(world: &mut VardeWorld) {
    world.send(Event::OpenHit);
}

#[when(expr = "I open every hit")]
fn open_every_hit(world: &mut VardeWorld) {
    world.send(Event::OpenEveryHit);
}

#[when(expr = "I complete the search")]
fn complete_search(world: &mut VardeWorld) {
    world.send(Event::CompleteSearch);
}

// ---- F21: finding inside the buffer ----

#[then(expr = "the in-file search is open")]
fn find_is_open(world: &mut VardeWorld) {
    assert!(world.state.find.is_some());
}

#[then(expr = "the in-file search is not open")]
fn find_is_closed(world: &mut VardeWorld) {
    assert!(world.state.find.is_none());
}

#[then(expr = "there are no matches")]
fn there_are_no_matches(world: &mut VardeWorld) {
    assert!(varde::matches(&world.state, ..).is_empty());
}

/// Key by key through the router, because that is what the editor sees: the
/// cursor moves as the query grows, so a whole query sent at once would prove
/// nothing — and a key lands wherever the query's caret is.
#[given(expr = "I type {string} into the in-file search")]
#[when(expr = "I type {string} into the in-file search")]
fn type_into_find(world: &mut VardeWorld, query: String) {
    for key in query.chars() {
        route_key(world, &key.to_string(), 0);
    }
}

#[then(expr = "the in-file search query is {string}")]
fn find_query_is(world: &mut VardeWorld, expected: String) {
    assert_eq!(world.state.find_query.shown(), expected);
}

#[when(expr = "I press Escape during the in-file search")]
fn abandon_find(world: &mut VardeWorld) {
    world.send(Event::CloseFind);
}

#[given(expr = "I press Enter during the in-file search")]
#[when(expr = "I press Enter during the in-file search")]
fn accept_find(world: &mut VardeWorld) {
    world.send(Event::AcceptFind);
}

#[then(expr = "nothing is highlighted")]
fn nothing_highlighted(world: &mut VardeWorld) {
    assert_eq!(varde::matches(&world.state, ..), Vec::new());
}

/// Every match, not only the one the cursor is on — walking the file is not the
/// only way to know how many there are.
#[then(expr = "the highlighted matches are:")]
fn highlighted_matches(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(usize, usize)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| {
            (
                row[0].parse().expect("a line"),
                row[1].parse().expect("a column"),
            )
        })
        .collect();
    let found: Vec<(usize, usize)> = varde::matches(&world.state, ..)
        .iter()
        .map(|at| (at.line, at.column))
        .collect();
    assert_eq!(found, expected);
}

#[then(expr = "nothing is echoed")]
fn nothing_echoed(world: &mut VardeWorld) {
    assert_eq!(varde::echoes(&world.state, ..), Vec::new());
}

#[then(expr = "the echoed occurrences are:")]
fn echoed_occurrences(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(usize, usize)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| {
            (
                row[0].parse().expect("a line"),
                row[1].parse().expect("a column"),
            )
        })
        .collect();
    let found: Vec<(usize, usize)> = varde::echoes(&world.state, ..)
        .iter()
        .map(|at| (at.line, at.column))
        .collect();
    assert_eq!(found, expected);
}

#[then(expr = "the selected action is {string}")]
fn selected_action_is(world: &mut VardeWorld, expected: String) {
    let index = world.state.selected_action.expect("an action is selected");
    let path = world.state.tree_selection.clone().expect("a row");
    let actions = tree::row_actions(&world.state, &path);
    assert_eq!(actions.get(index).copied(), Some(expected.as_str()));
}

#[then(expr = "no action is selected")]
fn no_action_selected(world: &mut VardeWorld) {
    assert!(world.state.selected_action.is_none());
}

#[given(expr = "the file tree pane does not have focus")]
fn tree_not_focused(world: &mut VardeWorld) {
    world.state.focus = Pane::Editor;
    world.state.tree_selection = None;
}

// ---- F22: the story artifact, and the spine it becomes ----

/// A story artifact, serialised straight from a `serde_json::Value` tree
/// rather than through the library's own (`Deserialize`-only) types — the
/// world plays the CLI that writes one, and the CLI has no reason to link
/// against `varde`.
fn build_story(
    base: &str,
    head: &str,
    spelling: &str,
    stories: Vec<(String, Vec<Value>)>,
) -> String {
    json!({
        "protocolVersion": 2,
        "title": "Title",
        "range": { "base": base, "head": head, "spelling": spelling },
        "stories": stories
            .into_iter()
            .enumerate()
            .map(|(index, (name, steps))| json!({
                "id": format!("s{}", index + 1),
                "name": name,
                "premise": "premise",
                "steps": steps,
            }))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

fn build_step(
    id: &str,
    file: &str,
    side: &str,
    kind: &str,
    from: u32,
    to: u32,
    text: &str,
) -> Value {
    // An empty text is no `text` field at all: the AI stopped writing one, so
    // the artifact a Scenario hands over should not carry an empty string the
    // real thing would never write.
    let mut site = json!({ "file": file, "side": side, "kind": kind, "from": from, "to": to });
    if !text.is_empty() {
        site["text"] = json!(text);
    }
    json!({
        "id": id,
        "name": id,
        "claim": "claim",
        "why": "why",
        "site": site,
    })
}

/// The already-loaded artifact, back into the JSON a re-authoring run would
/// write — needed only so "the story set is re-authored" can resend whatever
/// a `Given` step (e.g. one that attaches a Prediction) left the in-memory
/// artifact holding. The library's own types stay `Deserialize`-only, same
/// reason `build_story`/`build_step` above go through `serde_json::Value`
/// rather than the library's types: nothing in `src/` ever writes an
/// artifact back out, so nothing there should know how.
fn artifact_to_json(artifact: &story::Artifact) -> Value {
    json!({
        "protocolVersion": artifact.protocol_version,
        "title": artifact.title,
        "range": {
            "base": artifact.range.base,
            "head": artifact.range.head,
            "spelling": artifact.range.spelling,
        },
        "stories": artifact.stories.iter().map(|story| json!({
            "id": story.id,
            "name": story.name,
            "premise": story.premise,
            "steps": story.steps.iter().map(step_to_json).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

fn step_to_json(step: &story::Step) -> Value {
    let mut value = json!({
        "id": step.id,
        "name": step.name,
        "claim": step.claim,
        "why": step.why,
        "site": {
            "file": step.site.file,
            "side": match step.site.side {
                story::Side::Old => "old",
                story::Side::New => "new",
            },
            "kind": match step.site.kind {
                story::Kind::Changed => "changed",
                story::Kind::Context => "context",
            },
            "from": step.site.from,
            "to": step.site.to,
            "text": step.site.text,
        },
        "values": step.values.iter().map(|value| {
            let mut json_value = json!({
                "name": value.name,
                "value": value.value,
                "provenance": value.provenance.as_str(),
            });
            if let Some(cite) = &value.cite {
                json_value["cite"] = json!({ "file": cite.file, "line": cite.line });
            }
            json_value
        }).collect::<Vec<_>>(),
    });
    if let Some(flow) = &step.flow {
        value["flow"] = json!({ "in": flow.flow_in, "out": flow.flow_out });
    }
    if let Some(nudge) = &step.nudge {
        value["nudge"] = json!(nudge);
    }
    if let Some(prediction) = &step.prediction {
        value["prediction"] = json!({
            "question": prediction.question,
            "choices": prediction.choices.iter().map(|choice| json!({
                "text": choice.text,
                "correct": choice.correct,
                "feedback": choice.feedback,
            })).collect::<Vec<_>>(),
        });
    }
    value
}

/// A table cell by header name rather than position, since `story` and `text`
/// are optional columns a scenario may leave out.
fn column<'a>(headers: &[String], row: &'a [String], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .position(|header| header == name)
        .map(|index| row[index].as_str())
}

// "held:" is the old side of a change, so it also seeds `held` for the
// Remainder's subtraction; "holds:" is a file's current content unrelated to
// any change, so it only ever touches `files`.
#[given(expr = "{string} held:")]
fn file_held(world: &mut VardeWorld, path: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    world.held.insert(full.clone(), contents.clone());
    world.files.insert(full, contents);
    world.recompute_file_hunks();
}

#[given(expr = "{string} holds:")]
fn file_holds(world: &mut VardeWorld, path: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    world.files.insert(full, contents);
}

/// A file long enough to be scrolled, without a forty-line docstring in the
/// feature saying nothing but its own length.
#[given(expr = "{string} holds {int} numbered lines")]
fn file_holds_numbered_lines(world: &mut VardeWorld, path: String, lines: usize) {
    let contents: Vec<String> = (1..=lines).map(|line| format!("line {line}")).collect();
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    world.files.insert(full, contents.join("\n"));
}

#[given(expr = "{string} now holds:")]
#[when(expr = "{string} now holds:")]
fn file_now_holds(world: &mut VardeWorld, path: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    world.files.insert(full, contents);
    world.mark_modified(&path);
    world.recompute_file_hunks();
}

#[given(expr = "{string} is gone")]
fn file_is_gone(world: &mut VardeWorld, path: String) {
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    world.files.remove(&full);
    world.mark_modified(&path);
    world.recompute_file_hunks();
}

/// Seeds a file with a real hunk no Story's Site names, so the Remainder has
/// something bare to walk — real subtraction over real hunks, never a
/// fictional count.
#[given(expr = "{string} has an unclaimed hunk")]
fn file_has_an_unclaimed_hunk(world: &mut VardeWorld, path: String) {
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    world.held.insert(full.clone(), "a\nb\nc\n".to_string());
    world.files.insert(full, "a\nX\nc\n".to_string());
    world.mark_modified(&path);
    world.recompute_file_hunks();
}

/// The same bare seeding, shaped so the hunk git finds spans exactly `from`
/// to `to`: the file ends at `to` and differs from `from + CONTEXT_LINES`
/// onward, so the leading context opens the hunk at `from` and the end of
/// the file closes it at `to`. A span too short to hold that much context
/// leaves the two sides identical and git finds no hunk at all, which would
/// fail at the scenario's `Then` with nothing pointing back here — so it
/// refuses out loud instead.
#[given(expr = "{string} has an unclaimed hunk covering lines {int} to {int}")]
fn file_has_an_unclaimed_hunk_covering(world: &mut VardeWorld, path: String, from: u32, to: u32) {
    assert!(
        to >= from + story::CONTEXT_LINES,
        "lines {from} to {to} cannot be one hunk: a hunk needs {} lines of \
         leading context before the first line that differs",
        story::CONTEXT_LINES
    );
    let full = world.state.root.join(&path);
    let changed_from = from + story::CONTEXT_LINES;
    let held: String = (1..=to).map(|n| format!("line {n}\n")).collect();
    let now: String = (1..=to)
        .map(|n| {
            if n < changed_from {
                format!("line {n}\n")
            } else {
                format!("changed {n}\n")
            }
        })
        .collect();
    world.known_files.insert(full.clone());
    world.held.insert(full.clone(), held);
    world.files.insert(full, now);
    world.mark_modified(&path);
    world.recompute_file_hunks();
}

#[given(expr = "{string} holds only {int} lines")]
fn file_holds_only_lines(world: &mut VardeWorld, path: String, lines: usize) {
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    let current = world.files.get(&full).cloned().unwrap_or_default();
    let truncated = current
        .split('\n')
        .take(lines)
        .collect::<Vec<_>>()
        .join("\n");
    world.files.insert(full, truncated);
    world.mark_modified(&path);
    world.recompute_file_hunks();
}

#[given(expr = "{string} now holds different text at line {int}")]
fn file_now_holds_different_text_at_line(world: &mut VardeWorld, path: String, line: usize) {
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    let mut lines: Vec<String> = world
        .files
        .get(&full)
        .cloned()
        .unwrap_or_default()
        .split('\n')
        .map(str::to_string)
        .collect();
    if let Some(existing) = lines.get_mut(line - 1) {
        *existing = "THIS LINE NO LONGER MATCHES THE STEP".to_string();
    }
    world.files.insert(full, lines.join("\n"));
    world.mark_modified(&path);
    world.recompute_file_hunks();
}

#[given(expr = "a story {string} holds the steps:")]
fn story_holds_the_steps(world: &mut VardeWorld, name: String, step: &Step) {
    let table = step.table().expect("table");
    let headers = &table.rows[0];
    let steps: Vec<Value> = table
        .rows
        .iter()
        .skip(1)
        .enumerate()
        .map(|(index, row)| {
            let claim = column(headers, row, "claim").expect("claim column");
            let file = column(headers, row, "file").expect("file column");
            let side = column(headers, row, "side").expect("side column");
            let from: u32 = column(headers, row, "from")
                .expect("from column")
                .parse()
                .expect("a number");
            let to: u32 = column(headers, row, "to")
                .expect("to column")
                .parse()
                .expect("a number");
            let text = column(headers, row, "text").unwrap_or("");
            json!({
                "id": format!("s{index}"),
                "name": claim,
                "claim": claim,
                "why": "why",
                "site": {
                    "file": file, "side": side, "kind": "changed",
                    "from": from, "to": to, "text": text,
                },
            })
        })
        .collect();
    let contents = build_story(
        "aaaaaaaaaaaa",
        "bbbbbbbbbbbb",
        "main..HEAD",
        vec![(name, steps)],
    );
    world.send(Event::StoryArtifact {
        contents,
        range: story::RangeStatus::Resolves,
    });
}

/// The already-loaded story set's own steps, found by claim rather than by
/// index — a scenario names a Step the way a reviewer would.
fn step_named<'a>(artifact: &'a mut story::Artifact, claim: &str) -> &'a mut story::Step {
    artifact
        .stories
        .iter_mut()
        .flat_map(|story| story.steps.iter_mut())
        .find(|step| step.claim == claim)
        .expect("a step with that claim")
}

#[given(expr = "the step {string} carries the values:")]
fn step_carries_values(world: &mut VardeWorld, claim: String, step: &Step) {
    let table = step.table().expect("table");
    let headers = &table.rows[0];
    let values: Vec<story::Value> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let name = column(headers, row, "name")
                .expect("name column")
                .to_string();
            let value = column(headers, row, "value")
                .expect("value column")
                .to_string();
            let provenance = match column(headers, row, "provenance").expect("provenance column") {
                "literal" => story::Provenance::Literal,
                "fixture" => story::Provenance::Fixture,
                "invented" => story::Provenance::Invented,
                other => panic!("unknown provenance {other}"),
            };
            let cite = match (
                column(headers, row, "cite file"),
                column(headers, row, "cite line"),
            ) {
                (Some(file), Some(line)) if !file.is_empty() => Some(story::Cite {
                    file: file.to_string(),
                    line: line.parse().expect("a number"),
                }),
                _ => None,
            };
            story::Value {
                name,
                value,
                provenance,
                cite,
            }
        })
        .collect();
    let story::Set::Loaded(artifact) = &mut world.state.story_set else {
        panic!("no story set loaded");
    };
    step_named(artifact, &claim).values = values;
}

#[given(expr = "the step {string} holds:")]
fn step_holds_detail(world: &mut VardeWorld, claim: String, step: &Step) {
    let table = step.table().expect("table");
    let headers = &table.rows[0];
    let row = &table.rows[1];
    let why = column(headers, row, "why").expect("why column").to_string();
    let flow_in = column(headers, row, "flow in")
        .expect("flow in column")
        .to_string();
    let flow_out = column(headers, row, "flow out")
        .expect("flow out column")
        .to_string();
    let nudge = column(headers, row, "nudge")
        .map(str::to_string)
        .filter(|text| !text.is_empty());
    let story::Set::Loaded(artifact) = &mut world.state.story_set else {
        panic!("no story set loaded");
    };
    let target = step_named(artifact, &claim);
    target.why = why;
    target.flow = Some(story::Flow { flow_in, flow_out });
    target.nudge = nudge;
}

#[given(expr = "the step {string} asks {string}:")]
fn step_asks(world: &mut VardeWorld, claim: String, question: String, step: &Step) {
    let table = step.table().expect("table");
    let headers = &table.rows[0];
    let choices: Vec<story::Choice> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let text = column(headers, row, "choice")
                .expect("choice column")
                .to_string();
            let correct = match column(headers, row, "correct").expect("correct column") {
                "yes" => true,
                "no" => false,
                other => panic!("unknown correct {other:?}"),
            };
            let feedback = column(headers, row, "feedback")
                .expect("feedback column")
                .to_string();
            story::Choice {
                text,
                correct,
                feedback,
            }
        })
        .collect();
    let story::Set::Loaded(artifact) = &mut world.state.story_set else {
        panic!("no story set loaded");
    };
    step_named(artifact, &claim).prediction = Some(story::Prediction { question, choices });
}

#[given("a story set for this change claims:")]
#[when("a story set for this change claims:")]
fn story_set_claims(world: &mut VardeWorld, step: &Step) {
    let table = step.table().expect("table");
    let headers = &table.rows[0];
    let mut stories: Vec<(String, Vec<Value>)> = Vec::new();
    for (index, row) in table.rows.iter().enumerate().skip(1) {
        let name = column(headers, row, "story").unwrap_or("Story").to_string();
        let file = column(headers, row, "file").expect("file column");
        let side = column(headers, row, "side").expect("side column");
        let kind = column(headers, row, "kind").expect("kind column");
        let from: u32 = column(headers, row, "from")
            .expect("from column")
            .parse()
            .expect("a number");
        let to: u32 = column(headers, row, "to")
            .expect("to column")
            .parse()
            .expect("a number");
        let text = column(headers, row, "text").unwrap_or("");
        let mut claimed = build_step(&format!("s{index}"), file, side, kind, from, to, text);
        // A cited value, written as the value and the `file:line` it is
        // claimed to come from — the pair the arrival check compares.
        if let (Some(value), Some(cite)) =
            (column(headers, row, "value"), column(headers, row, "cite"))
        {
            let (file, line) = cite.split_once(':').expect("a file:line citation");
            claimed["values"] = json!([{
                "name": "the value",
                "value": value,
                "provenance": "literal",
                "cite": { "file": file, "line": line.parse::<u32>().expect("a line number") },
            }]);
        }
        match stories.iter_mut().find(|(existing, _)| *existing == name) {
            Some((_, steps)) => steps.push(claimed),
            None => stories.push((name, vec![claimed])),
        }
    }
    let contents = build_story("aaaaaaaaaaaa", "bbbbbbbbbbbb", "main..HEAD", stories);
    world.send(Event::StoryArtifact {
        contents,
        range: story::RangeStatus::Resolves,
    });
}

#[given("a story set for this change holds:")]
fn story_set_holds(world: &mut VardeWorld, step: &Step) {
    let table = step.table().expect("table");
    let headers = &table.rows[0];
    let stories: Vec<(String, Vec<Value>)> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let name = column(headers, row, "story")
                .expect("story column")
                .to_string();
            let count: usize = column(headers, row, "steps")
                .expect("steps column")
                .parse()
                .expect("a number");
            let steps = (1..=count)
                .map(|n| {
                    build_step(
                        &format!("s{n}"),
                        "src/keys.rs",
                        "new",
                        "changed",
                        n as u32,
                        n as u32,
                        "",
                    )
                })
                .collect();
            (name, steps)
        })
        .collect();
    let contents = build_story("aaaaaaaaaaaa", "bbbbbbbbbbbb", "main..HEAD", stories);
    world.send(Event::StoryArtifact {
        contents,
        range: story::RangeStatus::Resolves,
    });
}

/// Back to the editing view, so the next "I open Story view" is a real
/// switch: `switch_view` returns early for the view already on screen, so
/// re-opening without leaving reads nothing off disk.
#[given(expr = "I leave Story view")]
#[when(expr = "I leave Story view")]
fn leave_story_view(world: &mut VardeWorld) {
    pick_palette_entry(world, "Edit");
}

#[given(expr = "I opened Story view")]
#[given(expr = "I open Story view")]
#[when(expr = "I open Story view")]
fn open_story_view(world: &mut VardeWorld) {
    pick_palette_entry(world, "Story");
}

/// The Story's index in `story::spine`'s order — the same order
/// `Event::EnterStory` reads by, so a scenario can name a Story rather than
/// its position.
fn story_index(world: &VardeWorld, name: &str) -> usize {
    let story::Set::Loaded(artifact) = &world.state.story_set else {
        panic!("no story set loaded");
    };
    artifact
        .stories
        .iter()
        .position(|story| story.name == name)
        .expect("a story with that name")
}

#[given(expr = "I enter the story {string}")]
#[when(expr = "I enter the story {string}")]
fn enter_story(world: &mut VardeWorld, name: String) {
    if world.state.view != View::Story {
        pick_palette_entry(world, "Story");
    }
    let index = story_index(world, &name);
    world.send(Event::EnterStory(index));
}

#[given(expr = "I enter the remainder")]
#[when(expr = "I enter the remainder")]
fn enter_remainder(world: &mut VardeWorld) {
    if world.state.view != View::Story {
        pick_palette_entry(world, "Story");
    }
    world.send(Event::EnterRemainder);
}

#[then(expr = "the cursor is in {string}")]
fn cursor_is_in(world: &mut VardeWorld, path: String) {
    let expected = world.state.root.join(&path);
    assert_eq!(world.state.current_buffer, Some(expected));
}

/// Walking the Remainder is never a Story: it has no Step, so the band has
/// nothing to claim.
#[then(expr = "the band has no claim")]
fn band_has_no_claim(world: &mut VardeWorld) {
    assert!(story::current_step(&world.state).is_none());
}

#[then(expr = "no prediction is offered")]
fn no_prediction_is_offered(world: &mut VardeWorld) {
    let prediction = story::current_step(&world.state).and_then(|step| step.prediction.as_ref());
    assert!(prediction.is_none());
}

/// Walking a Story, set up directly rather than through `Event::EnterStory`:
/// this is scenario setup, not the thing under test, and going through the
/// real effect would record an open the later "no file was opened" scenarios
/// must not see.
#[given(expr = "I am walking {string}")]
fn set_walking(world: &mut VardeWorld, name: String) {
    world.state.view = View::Story;
    world.state.focus = Pane::Editor;
    let index = story_index(world, &name);
    let (file, from) = {
        let story::Set::Loaded(artifact) = &world.state.story_set else {
            unreachable!("checked by story_index");
        };
        let site = &artifact.stories[index].steps[0].site;
        (site.file.clone(), site.from as usize)
    };
    let path = world.state.root.join(&file);
    let contents = world.files.get(&path).cloned().unwrap_or_default();
    world.send(Event::BufferOpened {
        path: path.clone(),
        contents,
        preview: false,
        at: None,
    });
    if let Some(buffer) = world.state.buffers.get_mut(&path) {
        buffer.go_to_place(Place {
            line: from,
            column: 1,
        });
    }
    world.state.walking = Some(story::Walking::Story {
        story: index,
        step: 0,
        diff: story::Diff::Hidden,
    });
}

#[then(expr = "I am walking {string}")]
fn still_walking(world: &mut VardeWorld, name: String) {
    let index = story_index(world, &name);
    assert_eq!(world.state.view, View::Story);
    let walking_story = match world.state.walking {
        Some(story::Walking::Story { story, .. }) => Some(story),
        _ => None,
    };
    assert_eq!(walking_story, Some(index));
}

/// Enters the story, then steps forward until the given (1-based) Step.
#[given(expr = "I walked to step {int} of {string}")]
#[when(expr = "I walk to step {int} of {string}")]
fn walk_to_step(world: &mut VardeWorld, target: usize, name: String) {
    enter_story(world, name);
    for _ in 1..target {
        world.send(Event::StepStory(Direction::Right));
    }
}

/// The current Story's Step by its 1-based position — the same numbering a
/// scenario reads the spine or the walkthrough by.
fn nth_step(world: &VardeWorld, index: usize) -> &story::Step {
    let Some(story::Walking::Story { story, .. }) = world.state.walking else {
        panic!("walking a story");
    };
    let story::Set::Loaded(artifact) = &world.state.story_set else {
        panic!("no story set loaded");
    };
    &artifact.stories[story].steps[index - 1]
}

/// Reads the loaded set straight through rather than through `nth_step`: the
/// text Varde filled in is a fact about the set, and a Scenario should not
/// have to start walking a Story to see it.
#[then(expr = "the site text of step {int} is {string}")]
fn site_text_of_step_is(world: &mut VardeWorld, index: usize, expected: String) {
    let story::Set::Loaded(artifact) = &world.state.story_set else {
        panic!("no story set loaded");
    };
    let sites: Vec<&story::Site> = artifact
        .stories
        .iter()
        .flat_map(|story| story.steps.iter().map(|step| &step.site))
        .collect();
    assert_eq!(sites[index - 1].text, expected);
}

/// Holds back the edge's answer about what the Sites hold, so a Scenario can
/// watch a parsed set stay unwalkable while the read is outstanding.
#[given(expr = "Varde has not yet read what the sites hold")]
fn sites_not_yet_read(world: &mut VardeWorld) {
    world.hold_site_texts = true;
}

/// By position, not by name: a set that is not loaded has no spine to look a
/// name up in, which is exactly the Scenario this exists for.
#[when(expr = "I choose the first story from the spine")]
fn choose_first_story(world: &mut VardeWorld) {
    world.send(Event::EnterStory(0));
}

#[then(expr = "step {int} is stale as {string}")]
fn step_is_stale_as(world: &mut VardeWorld, index: usize, kind: String) {
    let step = nth_step(world, index);
    assert_eq!(
        story::staleness(&world.state, step).kind(),
        Some(kind.as_str())
    );
}

#[given(expr = "step {int} is not stale")]
#[then(expr = "step {int} is not stale")]
fn step_is_not_stale(world: &mut VardeWorld, index: usize) {
    let step = nth_step(world, index);
    assert!(!story::staleness(&world.state, step).is_stale());
}

#[then(expr = "the band warns that the step is stale")]
fn band_warns_stale(world: &mut VardeWorld) {
    let step = story::current_step(&world.state).expect("a step");
    assert!(story::staleness(&world.state, step).is_stale());
}

#[then(expr = "the overlay shows the site's stored text")]
fn overlay_shows_stored_text(world: &mut VardeWorld) {
    let step = story::current_step(&world.state).expect("a step");
    assert!(!step.site.text.is_empty());
}

#[then(expr = "the overlay shows what the site holds now")]
fn overlay_shows_current_text(world: &mut VardeWorld) {
    let step = story::current_step(&world.state).expect("a step");
    match story::staleness(&world.state, step) {
        story::Staleness::Changed { now } => assert_ne!(now, step.site.text),
        other => panic!("expected a text-changed staleness, got {other:?}"),
    }
}

#[when(expr = "I change line {int} of {string} in the editor")]
fn change_line_in_editor(world: &mut VardeWorld, line: usize, path: String) {
    edit_line(world, &path, line, "totally different text now");
}

#[when(expr = "I re-indent line {int} of {string} in the editor")]
fn reindent_line_in_editor(world: &mut VardeWorld, line: usize, path: String) {
    let full = world.state.root.join(&path);
    let buffer = world.state.buffers.get_mut(&full).expect("buffer open");
    buffer.go_to_place(Place { line, column: 1 });
    buffer.key('i');
    for key in "    ".chars() {
        buffer.key(key);
    }
    buffer.escape();
}

/// Clears a whole line, then types replacement text into it — used to make an
/// in-editor edit that genuinely changes a Site's text, as opposed to a
/// reindent that only shifts its whitespace.
fn edit_line(world: &mut VardeWorld, path: &str, line: usize, text: &str) {
    let full = world.state.root.join(path);
    let buffer = world.state.buffers.get_mut(&full).expect("buffer open");
    buffer.go_to_place(Place { line, column: 1 });
    buffer.delete_in(Place { line, column: 1 }, Place { line, column: 9999 });
    buffer.key('i');
    for key in text.chars() {
        buffer.key(key);
    }
    buffer.escape();
}

// ---- The Site mark ----

/// The mark names a file the way a Site spells it; the file on screen has to
/// be spelt the same way or the bar lands on nothing. A Guest repo's file was
/// spelt from the workspace root and never matched.
#[then(expr = "the site mark is drawn on the file on screen")]
fn site_mark_is_on_screen(world: &mut VardeWorld) {
    let marked = story::mark(&world.state);
    let story::SiteMark::Site { from, .. } = &marked else {
        panic!("expected a site mark, got {marked:?}");
    };
    let shown = story::shown_file(&world.state);
    assert!(
        marked.covers(&shown, *from),
        "the mark {marked:?} does not cover {shown:?}"
    );
}

#[then(expr = "the site mark covers lines {int} to {int} of {string}")]
fn site_mark_covers(world: &mut VardeWorld, from: u32, to: u32, path: String) {
    let marked = story::mark(&world.state);
    let story::SiteMark::Site {
        file,
        from: marked_from,
        to: marked_to,
        ..
    } = &marked
    else {
        panic!("expected a site mark, got {marked:?}");
    };
    assert_eq!(
        (file.as_str(), *marked_from, *marked_to),
        (path.as_str(), from, to)
    );
}

/// The promise framing makes, asked of the rows the pane actually shows: a
/// Site is a range, so "in view" is a claim about both its ends. In rows, via
/// `story::row_of`, and against `varde::fits` rather than a count of its own —
/// a scenario recomputing the pane's size would be asserting its own
/// arithmetic.
#[then(expr = "the whole site is in view")]
fn whole_site_is_in_view(world: &mut VardeWorld) {
    let marked = story::mark(&world.state);
    let story::SiteMark::Site { from, to, .. } = &marked else {
        panic!("expected a site mark, got {marked:?}");
    };
    let (first, last) = (
        story::row_of(&world.state, *from),
        story::row_of(&world.state, *to),
    );
    let top = world.state.editor_scroll + 1;
    let bottom = top + varde::fits(&world.state).1 - 1;
    assert!(
        first >= top && last <= bottom,
        "the site is drawn on rows {first} to {last}, and the pane shows rows {top} to {bottom}"
    );
}

#[then(expr = "the site mark is {string}")]
fn site_mark_is(world: &mut VardeWorld, kind: String) {
    let marked = story::mark(&world.state);
    let story::SiteMark::Site {
        kind: marked_kind, ..
    } = &marked
    else {
        panic!("expected a site mark, got {marked:?}");
    };
    assert_eq!(marked_kind.as_str(), kind);
}

#[then(expr = "line {int} of {string} is marked")]
fn line_is_marked(world: &mut VardeWorld, line: u32, path: String) {
    assert!(story::mark(&world.state).covers(&path, line));
}

#[then(expr = "no site mark is drawn")]
fn no_site_mark(world: &mut VardeWorld) {
    let marked = story::mark(&world.state);
    assert!(
        !matches!(marked, story::SiteMark::Site { .. }),
        "expected no site mark, got {marked:?}"
    );
}

// ---- The Site's diff ----

#[then(expr = "line {int} of {string} is marked as added")]
fn line_is_marked_as_added(world: &mut VardeWorld, line: u32, path: String) {
    assert_eq!(story::shown_file(&world.state), path);
    let diff = story::site_diff(&world.state);
    assert!(
        diff.as_ref().is_some_and(|diff| diff.added.contains(&line)),
        "line {line} is not marked as added: {diff:?}"
    );
}

#[then(expr = "line {int} of {string} is not marked as added")]
fn line_is_not_marked_as_added(world: &mut VardeWorld, line: u32, path: String) {
    assert_eq!(story::shown_file(&world.state), path);
    let diff = story::site_diff(&world.state);
    assert!(
        !diff.as_ref().is_some_and(|diff| diff.added.contains(&line)),
        "line {line} is marked as added: {diff:?}"
    );
}

/// Every removed row the code surface draws, with the line it sits under —
/// read off `story::rows`, the map the renderer, the caret and the scroll
/// clamp all read.
fn removed_rows(world: &VardeWorld) -> Vec<(u32, String)> {
    let lines = current_buffer(world).shown().split('\n').count();
    let mut under = 0;
    let mut removed = Vec::new();
    for row in story::rows(&world.state, lines) {
        match row {
            story::Row::Code(number) => under = number,
            story::Row::Removed(text) => removed.push((under, text.to_string())),
            story::Row::Comment(_) => {}
        }
    }
    removed
}

#[then(expr = "the code shows the removed rows:")]
fn code_shows_removed_rows(world: &mut VardeWorld, step: &Step) {
    let table = step.table().expect("table");
    let expected: Vec<(u32, String)> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| (row[0].parse().expect("a line number"), row[1].clone()))
        .collect();
    let trimmed = |rows: Vec<(u32, String)>| -> Vec<(u32, String)> {
        rows.into_iter()
            .map(|(under, text)| (under, text.trim().to_string()))
            .collect()
    };
    assert_eq!(trimmed(removed_rows(world)), trimmed(expected));
}

#[then(expr = "the code shows no removed rows")]
fn code_shows_no_removed_rows(world: &mut VardeWorld) {
    assert_eq!(removed_rows(world), vec![]);
}

/// What the pane title says keys will do — the buffer's own mode everywhere but
/// a walk, which claims every editor key before the buffer sees one.
#[then(expr = "the editor mode label is {string}")]
fn editor_mode_label_is(world: &mut VardeWorld, expected: String) {
    let label = varde::mode_label(&world.state, current_buffer(world));
    assert_eq!(label, expected);
}

#[then(expr = "the step view state is {string}")]
fn step_view_state_is(world: &mut VardeWorld, expected: String) {
    assert_eq!(story::mark(&world.state).as_str(), expected);
}

/// Dimming is not a query of its own: a line is dimmed exactly when it is
/// outside the mark, so this asserts the same answer the renderer derives.
#[then(expr = "line {int} of {string} is dimmed")]
fn line_is_dimmed(world: &mut VardeWorld, line: u32, path: String) {
    assert!(!story::mark(&world.state).covers(&path, line));
}

/// Widens a Step's Site, carrying its stored text with it — a range moved
/// without its text would read as stale, which is a different scenario.
#[given(expr = "the step {string} covers lines {int} to {int}")]
fn step_covers_lines(world: &mut VardeWorld, claim: String, from: u32, to: u32) {
    let file = {
        let story::Set::Loaded(artifact) = &world.state.story_set else {
            panic!("no story set loaded");
        };
        artifact
            .stories
            .iter()
            .flat_map(|story| story.steps.iter())
            .find(|step| step.claim == claim)
            .expect("a step with that claim")
            .site
            .file
            .clone()
    };
    let contents = world
        .files
        .get(&world.state.root.join(&file))
        .cloned()
        .unwrap_or_default();
    let text = contents
        .lines()
        .skip(from as usize - 1)
        .take((to - from + 1) as usize)
        .collect::<Vec<_>>()
        .join("\n");
    let story::Set::Loaded(artifact) = &mut world.state.story_set else {
        panic!("no story set loaded");
    };
    let site = &mut step_named(artifact, &claim).site;
    site.from = from;
    site.to = to;
    site.text = text;
}

#[given(expr = "the step {string} points at context")]
fn step_points_at_context(world: &mut VardeWorld, claim: String) {
    let story::Set::Loaded(artifact) = &mut world.state.story_set else {
        panic!("no story set loaded");
    };
    step_named(artifact, &claim).site.kind = story::Kind::Context;
}

/// A Prediction with no scenario-visible content: these scenarios care that
/// the overlay is up at all, not what it offers.
#[given(expr = "the step {string} carries a prediction")]
fn step_carries_a_prediction(world: &mut VardeWorld, claim: String) {
    let choices = ["a", "b", "c"]
        .into_iter()
        .enumerate()
        .map(|(index, text)| story::Choice {
            text: text.to_string(),
            correct: index == 0,
            feedback: "because".to_string(),
        })
        .collect();
    let story::Set::Loaded(artifact) = &mut world.state.story_set else {
        panic!("no story set loaded");
    };
    step_named(artifact, &claim).prediction = Some(story::Prediction {
        question: "why?".to_string(),
        choices,
    });
}

#[given(expr = "the step {string} points at the old side")]
fn step_points_at_old_side(world: &mut VardeWorld, claim: String) {
    let story::Set::Loaded(artifact) = &mut world.state.story_set else {
        panic!("no story set loaded");
    };
    step_named(artifact, &claim).site.side = story::Side::Old;
}

#[given(expr = "the story set's range is {string}")]
fn story_set_range_is(world: &mut VardeWorld, spelling: String) {
    let (base, head, _) = story::revisions(&spelling).expect("a base..head spelling");
    let story::Set::Loaded(artifact) = &mut world.state.story_set else {
        panic!("no story set loaded");
    };
    artifact.range = story::Range {
        base: base.to_string(),
        head: head.to_string(),
        spelling,
    };
}

/// What committing does to an old-side Site: `HEAD` moves to whatever the
/// working tree currently holds, which is what makes an old-side Site over
/// an uncommitted range vulnerable to staleness in the first place.
#[given(expr = "the working tree is committed")]
#[when(expr = "the working tree is committed")]
fn working_tree_is_committed(world: &mut VardeWorld) {
    for (path, contents) in world.files.clone() {
        world.held.insert(path, contents);
    }
    world.recompute_file_hunks();
}

#[given(expr = "authoring has begun for {string}")]
fn authoring_begun(world: &mut VardeWorld, spelling: String) {
    // The snapshot "the review list is unchanged" compares against, taken
    // before the real confirm flow runs.
    world.repo_before = Some(review::list(&world.state));
    world.state.modal = Modal::ConfirmStory {
        spelling,
        out: String::new(),
    };
    world.send(Event::ConfirmStory);
}

#[when(expr = "the story artifact arrives:")]
fn story_artifact_arrives(world: &mut VardeWorld, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    world.send(Event::StoryArtifact {
        contents,
        range: story::RangeStatus::Resolves,
    });
}

#[when(expr = "a story artifact arrives whose prediction offers {int} choices")]
fn artifact_with_choice_count(world: &mut VardeWorld, count: u32) {
    let choices: Vec<Value> = (0..count)
        .map(|index| {
            json!({
                "text": format!("choice {index}"),
                "correct": index == 0,
                "feedback": "because",
            })
        })
        .collect();
    let mut claimed = build_step("s1e1", "src/keys.rs", "new", "changed", 1, 1, "x");
    claimed["prediction"] = json!({ "question": "why?", "choices": choices });
    let contents = build_story(
        "aaaaaaaaaaaa",
        "bbbbbbbbbbbb",
        "main..HEAD",
        vec![("Story".to_string(), vec![claimed])],
    );
    world.send(Event::StoryArtifact {
        contents,
        range: story::RangeStatus::Resolves,
    });
}

#[given(expr = "a story set exists for the range {string}")]
fn story_set_exists_for_range(world: &mut VardeWorld, spelling: String) {
    let (base, head, _) = story::revisions(&spelling).expect("a range");
    let claimed = build_step("s1e1", "src/keys.rs", "new", "changed", 1, 1, "x");
    // The Step has to point at real, changed code, or the arrival checks
    // refuse the set before any of this scenario's question is reached.
    let file = world.state.root.join("src/keys.rs");
    world.known_files.insert(file.clone());
    world.files.insert(file, "x".to_string());
    let contents = build_story(
        base,
        head,
        &spelling,
        vec![("Story".to_string(), vec![claimed])],
    );
    let dir = world.state.root.join(".varde/stories");
    world
        .files
        .insert(dir.join(format!("{base}-{head}.json")), contents);
    world.last_authored_range = Some((base.to_string(), head.to_string()));
}

/// A story set already in the folder, writing no `site.text` — the shape the
/// AI leaves behind now that Varde fills the text in. Written into the folder
/// rather than sent as an event, so opening Story view really re-reads it.
#[given(expr = "a story set on disk claims lines {int} to {int} of {string}")]
fn story_set_on_disk_claims(world: &mut VardeWorld, from: u32, to: u32, path: String) {
    let claimed = build_step("s1e1", &path, "new", "changed", from, to, "");
    let contents = build_story(
        "aaaaaaaaaaaa",
        "bbbbbbbbbbbb",
        "main..HEAD",
        vec![("The keys".to_string(), vec![claimed])],
    );
    let dir = world.state.root.join(".varde/stories");
    world
        .files
        .insert(dir.join("aaaaaaaaaaaa-bbbbbbbbbbbb.json"), contents);
}

#[given(expr = "that range is what {string} resolves to")]
fn range_resolves_to(world: &mut VardeWorld, spelling: String) {
    let range = world
        .last_authored_range
        .clone()
        .expect("a story set range to point the spelling at");
    world.resolved_oids.insert(spelling, range);
}

#[given(expr = "{string} is not a commit in this repository")]
fn revision_unresolvable(world: &mut VardeWorld, revision: String) {
    world.unresolvable.insert(revision);
}

#[then(expr = "the tree pane shows the spine")]
fn tree_shows_spine(world: &mut VardeWorld) {
    assert_eq!(world.state.story_listing, story::Listing::Spine);
}

#[then(expr = "the tree pane shows the review list")]
fn tree_shows_review_list(world: &mut VardeWorld) {
    assert_eq!(world.state.story_listing, story::Listing::Files);
    let mut visible: Vec<PathBuf> = tree::visible_rows(&world.state)
        .into_iter()
        .map(|row| row.path)
        .collect();
    let mut expected: Vec<PathBuf> = review::list(&world.state)
        .into_iter()
        .map(|file| world.state.root.join(file))
        .collect();
    visible.sort();
    expected.sort();
    assert_eq!(visible, expected);
}

#[then("the spine lists:")]
fn spine_lists(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, usize)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| (row[0].clone(), row[1].parse().expect("a number")))
        .collect();
    let actual: Vec<(String, usize)> = story::spine(&world.state)
        .into_iter()
        .map(|row| (row.name, row.steps))
        .collect();
    assert_eq!(actual, expected);
}

#[then(expr = "the tree pane has no filter box")]
fn tree_has_no_filter_box(world: &mut VardeWorld) {
    assert_eq!(tree::filter_rows(world.state.view), 0);
}

#[then(expr = "the spine's title is {string}")]
fn spine_title_is(world: &mut VardeWorld, expected: String) {
    assert_eq!(story::title(&world.state), Some(expected.as_str()));
}

#[then(expr = "the spine has no title")]
fn spine_has_no_title(world: &mut VardeWorld) {
    assert_eq!(story::title(&world.state), None);
}

#[then(expr = "the spine reserves a row for the title")]
fn spine_reserves_a_row_for_the_title(world: &mut VardeWorld) {
    assert_eq!(story::title_rows(&world.state), 1);
}

#[then(expr = "the spine reserves no row for the title")]
fn spine_reserves_no_row_for_the_title(world: &mut VardeWorld) {
    assert_eq!(story::title_rows(&world.state), 0);
}

#[then(expr = "the spine view starts at row {int}")]
fn spine_view_starts(world: &mut VardeWorld, row: usize) {
    assert_eq!(world.state.spine_scroll + 1, row);
}

#[then(expr = "the story view state is {string}")]
fn story_view_state(world: &mut VardeWorld, expected: String) {
    assert_eq!(story::view_state(&world.state), expected);
}

/// A refused set has to say which Step failed and how, or the reviewer is
/// left rereading a 70KB artifact. The Step's id and the fault's own word,
/// never the sentence around them.
#[then(expr = "the refusal names step {string} as {string}")]
fn refusal_names_step(world: &mut VardeWorld, id: String, fault: String) {
    let story::Set::Refused { because } = &world.state.story_set else {
        panic!("the set was not refused: {:?}", world.state.story_set);
    };
    assert!(because.contains(&format!("{id}: {fault}")), "{because}");
}

/// The fix request is the send after the authoring prompt, so it is read off
/// the end rather than the front. The Step's id and the fault's own word,
/// never the sentence around them — the same pair a refusal names, because the
/// two must not drift apart.
#[then(expr = "the fix request names step {string} as {string}")]
fn fix_request_names_step(world: &mut VardeWorld, id: String, fault: String) {
    assert!(
        matches!(world.state.story_set, story::Set::Fixing { .. }),
        "the set is not being fixed: {:?}",
        world.state.story_set
    );
    let request = fix_request(world);
    assert!(request.contains(&format!("`{id}`: {fault}")), "{request}");
}

/// A fix request that named a passing Step would have the AI rewriting the
/// whole set, which is the ten minutes this exists to save.
#[then(expr = "the fix request does not name step {string}")]
fn fix_request_does_not_name_step(world: &mut VardeWorld, id: String) {
    let request = fix_request(world);
    assert!(!request.contains(&format!("`{id}`:")), "{request}");
}

#[then(expr = "the fix request names the file {string}")]
fn fix_request_names_the_file(world: &mut VardeWorld, path: String) {
    let request = fix_request(world);
    assert!(request.contains(&path), "{request}");
}

fn fix_request(world: &VardeWorld) -> String {
    let sent = ai_sends(world);
    assert!(sent.len() > 1, "no fix request was sent: {sent:?}");
    sent.last().expect("a send").clone()
}

#[then(expr = "the cursor is on line {int} of {string}")]
fn cursor_on_line_of(world: &mut VardeWorld, line: usize, path: String) {
    let expected = world.state.root.join(&path);
    assert_eq!(world.state.current_buffer, Some(expected.clone()));
    let buffer = world
        .state
        .buffers
        .get(&expected)
        .expect("the file to be open");
    assert_eq!(buffer.line, line);
}

#[then(expr = "the band claim is {string}")]
fn band_claim_is(world: &mut VardeWorld, expected: String) {
    let claim = story::current_step(&world.state).map(|step| step.claim.clone());
    assert_eq!(claim, Some(expected));
}

#[then(expr = "the step menu is empty")]
fn step_menu_is_empty(world: &mut VardeWorld) {
    assert!(story::step_menu(&world.state).is_empty());
}

#[then("the step menu lists:")]
fn step_menu_lists(world: &mut VardeWorld, step: &Step) {
    let table = step.table().expect("table");
    let headers = &table.rows[0];
    let expected: Vec<(String, bool)> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let name = column(headers, row, "name")
                .expect("name column")
                .to_string();
            let current = column(headers, row, "current")
                .expect("current column")
                .parse()
                .expect("a bool");
            (name, current)
        })
        .collect();
    let actual: Vec<(String, bool)> = story::step_menu(&world.state)
        .into_iter()
        .map(|row| (row.name, row.current))
        .collect();
    assert_eq!(actual, expected);
}

#[then(expr = "the band values are:")]
fn band_values_are(world: &mut VardeWorld, step: &Step) {
    let table = step.table().expect("table");
    let headers = &table.rows[0];
    let expected: Vec<(String, String, String)> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            (
                column(headers, row, "name")
                    .expect("name column")
                    .to_string(),
                column(headers, row, "value")
                    .expect("value column")
                    .to_string(),
                column(headers, row, "provenance")
                    .expect("provenance column")
                    .to_string(),
            )
        })
        .collect();
    let actual: Vec<(String, String, String)> = story::current_step(&world.state)
        .map(|step| {
            step.values
                .iter()
                .map(|value| {
                    (
                        value.name.clone(),
                        value.value.clone(),
                        value.displayed_provenance().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(actual, expected);
}

#[then(expr = "the band has no values row")]
fn band_has_no_values_row(world: &mut VardeWorld) {
    let empty = story::current_step(&world.state)
        .map(|step| step.values.is_empty())
        .unwrap_or(true);
    assert!(empty);
}

#[then(expr = "the overlay sections are:")]
fn overlay_sections_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let actual: Vec<String> = story::current_step(&world.state)
        .map(|step| story::detail_sections(&world.state, step))
        .unwrap_or_default()
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(actual, expected);
}

#[then(expr = "the view is {string}")]
fn the_view_is(world: &mut VardeWorld, name: String) {
    let expected = match name.as_str() {
        "edit" => View::Edit,
        "review" => View::Review,
        "story" => View::Story,
        other => panic!("unknown view {other:?}"),
    };
    assert_eq!(world.state.view, expected);
}

#[then(expr = "{string} was opened in the editor")]
fn was_opened_in_the_editor(world: &mut VardeWorld, path: String) {
    let expected = world.state.root.join(&path);
    assert!(
        world.opened.contains(&expected),
        "opened: {:?}",
        world.opened
    );
}

#[then(expr = "the editor scrolled down")]
fn editor_scrolled_down(world: &mut VardeWorld) {
    assert!(world.state.editor_scroll > 0);
}

#[then(expr = "the spine is empty")]
fn spine_is_empty(world: &mut VardeWorld) {
    assert!(story::spine(&world.state).is_empty());
}

// ---- F22: resolving a story range ----

#[given(expr = "I run {string} in the editor")]
#[when(expr = "I run {string} in the editor")]
#[given(expr = "I run {string}")]
#[when(expr = "I run {string}")]
fn run_story_command(world: &mut VardeWorld, line: String) {
    assert!(line.starts_with(':'), "not a : command: {line:?}");
    let mut drafts = std::mem::take(&mut world.drafts);
    for c in line.chars() {
        for event in keys::on_key_event(&world.state, &mut drafts, plain_key(c), 0) {
            world.send(event);
        }
    }
    for event in keys::on_key_event(
        &world.state,
        &mut drafts,
        terminput::KeyEvent::new(terminput::KeyCode::Enter),
        0,
    ) {
        world.send(event);
    }
    world.drafts = drafts;
}

fn plain_key(c: char) -> terminput::KeyEvent {
    terminput::KeyEvent::new(terminput::KeyCode::Char(c))
}

#[given(expr = "{string} resolves to {string}")]
fn origin_head_resolves_to(world: &mut VardeWorld, source: String, revision: String) {
    match source.as_str() {
        "origin/HEAD" => world.origin_head = Some(revision),
        "HEAD" => {
            world.head = Some(revision.clone());
            world.startup.head = Some(revision);
            world.tell_core();
        }
        other => panic!("unexpected source {other:?}"),
    }
}

#[given(regex = r#"^the repository resolves (.+) to "(.+)"$"#)]
fn ladder_candidate_resolves_to(world: &mut VardeWorld, source: String, branch: String) {
    match source.as_str() {
        "\"origin/HEAD\"" => world.origin_head = Some(branch),
        "the upstream branch" => world.upstream_branch = Some(branch),
        "\"init.defaultBranch\"" => world.default_branch_config = Some(branch),
        "a probe for \"main\"" => {
            world.probes.insert("main".to_string());
        }
        "a probe for \"master\"" => {
            world.probes.insert("master".to_string());
        }
        other => panic!("unknown default-branch source {other:?}"),
    }
}

// Narrative only: the offline ladder resolves by existence, never by
// ancestry — a candidate that resolves is used as-is, and where the two have
// parted is git's question, asked by the three dots the spelling carries.
#[given(expr = "{string} has commits {string} does not")]
fn has_commits_the_other_does_not(_world: &mut VardeWorld, _branch: String, _of: String) {}

#[given(expr = "the repository resolves no default branch")]
fn no_default_branch_resolves(world: &mut VardeWorld) {
    world.origin_head = None;
    world.upstream_branch = None;
    world.default_branch_config = None;
    world.probes.clear();
}

/// The refs the repository states about itself. `seconds` is the commit date,
/// which is what the list is ordered by — a bare number, because a scenario
/// about ordering is about which is newer and nothing else.
#[given(expr = "the repository has branches:")]
fn repository_has_branches(world: &mut VardeWorld, step: &Step) {
    let rows = step.table().expect("a table of branches").rows.clone();
    world.branch_refs = rows
        .into_iter()
        .skip(1)
        .map(|row| story::BranchRef {
            name: row[0].trim().to_string(),
            remote: match row[1].trim() {
                "local" => false,
                "remote" => true,
                other => panic!("unexpected branch kind {other:?}"),
            },
            when: row[2].trim().parse().expect("a commit date in seconds"),
        })
        .collect();
}

/// A branch pushed to the remote after the clone — the one thing only a fetch
/// puts in the picker. Newer than every branch the scenario declared, because
/// a branch pushed after the clone is the newest thing there is to list.
#[when(expr = "the branch {string} appears on the remote")]
fn branch_appears_on_the_remote(world: &mut VardeWorld, name: String) {
    let when = world
        .branch_refs
        .iter()
        .map(|row| row.when)
        .max()
        .unwrap_or(0)
        + 100;
    world.branch_refs.push(story::BranchRef {
        name,
        remote: true,
        when,
    });
}

#[given(expr = "git is installed")]
fn git_is_installed(world: &mut VardeWorld) {
    world.git_on_path = true;
    world.tell_core();
}

/// The one runtime dependency a Guest repo adds. Stated rather than assumed,
/// because the core is told what is on the machine and never remembers it.
#[given(expr = "git is not installed")]
fn git_is_not_installed(world: &mut VardeWorld) {
    world.git_on_path = false;
    world.tell_core();
}

/// The command, held to the three things that make it the user's own git in
/// the shell pane (ADR 0015): the URL as pasted, a destination inside the
/// Sidecar, and the exit status written to the sentinel last. Where the
/// Sidecar is spelled is not a scenario's business, so the step derives it
/// the way the step for a review written outside every workspace does.
#[then(expr = "the terminal has cloned {string} into the Sidecar")]
fn terminal_has_cloned(world: &mut VardeWorld, url: String) {
    // The last command, not the only one: a session that has already cloned
    // one repository can clone a second. Cloning this URL *twice* is still
    // held against, since that is the whole of what a fetch exists to avoid.
    let command = world.executed.last().expect("a command");
    assert_eq!(
        world
            .executed
            .iter()
            .filter(|command| command.contains(&format!("git clone {url}")))
            .count(),
        1,
        "cloned twice: {:?}",
        world.executed
    );
    let guest = Path::new(SIDECAR).join(story::guest_name(&url));
    let sentinel = Path::new(SIDECAR).join(story::DOWNLOAD_SENTINEL);
    assert!(
        command.contains(&format!("git clone {url}")),
        "not a clone of the URL: {command}"
    );
    assert!(
        command.contains(&guest.display().to_string()),
        "not a clone into the Sidecar: {command}"
    );
    // Never `&& touch`: a clone that failed has to leave a file that says so.
    // How the line is spelled is `story::clone_command`'s own unit test; what
    // a scenario is held to is that the status is what lands in the sentinel.
    assert!(
        command.contains("echo $? >") && command.contains(&sentinel.display().to_string()),
        "the sentinel carries no exit status: {command}"
    );
}

/// The second `:story?` on a URL already downloaded: one clone for the
/// session, and a fetch aimed at the copy the clone left. `origin` and not the
/// URL, because a fetch given a URL updates no remote-tracking ref — how the
/// line is spelled is `story::download_command`'s own unit test.
#[then(expr = "the terminal has fetched {string}")]
fn terminal_has_fetched(world: &mut VardeWorld, url: String) {
    let guest = Path::new(SIDECAR).join(story::guest_name(&url));
    let sentinel = Path::new(SIDECAR).join(story::DOWNLOAD_SENTINEL);
    let fetch = world.executed.last().expect("a command");
    assert!(
        fetch.contains("git -C ")
            && fetch.contains(&guest.display().to_string())
            && fetch.contains(" fetch"),
        "not a fetch of the Guest repo: {fetch}"
    );
    assert!(
        fetch.contains("echo $? >") && fetch.contains(&sentinel.display().to_string()),
        "the sentinel carries no exit status: {fetch}"
    );
    assert_eq!(
        world
            .executed
            .iter()
            .filter(|command| command.contains(&format!("git clone {url}")))
            .count(),
        1,
        "cloned as well as fetched: {:?}",
        world.executed
    );
}

/// A fetch that failed is a fetch: the copy on disk is still the repository
/// under review, and nothing about it is thrown away because a download of
/// something newer did not arrive.
#[then(expr = "the Guest repo is still there")]
fn guest_repo_is_still_there(world: &mut VardeWorld) {
    let guest = PathBuf::from(SIDECAR).join(story::guest_name("git@github.com:them/theirs.git"));
    assert_eq!(world.state.guest.as_ref(), Some(&guest));
    assert!(
        !world.dirs_deleted.contains(&guest)
            && !world.deleted.iter().any(|p| p.starts_with(&guest)),
        "deleted: {:?} {:?}",
        world.dirs_deleted,
        world.deleted
    );
}

/// Absolute and in the Sidecar, both of them: the prompt goes to a session
/// whose working directory Varde did not set and does not move, so a relative
/// path names a file somewhere nobody agreed on — and in the workspace it
/// would be a file in a folder Varde promises to write nothing into.
#[then(expr = "the authoring prompt names the story set in the Sidecar")]
fn prompt_names_the_set_in_the_sidecar(world: &mut VardeWorld) {
    let prompt = ai_sends(world).first().cloned().expect("a prompt");
    let named: Vec<&str> = prompt
        .split_whitespace()
        .filter(|word| word.contains("/stories/"))
        .collect();
    assert!(
        !named.is_empty(),
        "the prompt names no story set:\n{prompt}"
    );
    for path in named {
        let path = path.trim_matches(|c: char| !c.is_ascii_graphic() || c == '`');
        assert!(
            path.starts_with(SIDECAR),
            "not an absolute path in the Sidecar: {path}"
        );
    }
}

/// Written beside the set, which for a Guest repo means beside it in the
/// Sidecar: the prompt has already pointed the AI at this exact path.
#[then(expr = "the story context file was written into the Sidecar")]
fn context_written_into_the_sidecar(world: &mut VardeWorld) {
    let written: Vec<&PathBuf> = world
        .wrote
        .iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    let [context] = written.as_slice() else {
        panic!("wrote: {:?}", world.wrote);
    };
    assert!(
        context.starts_with(SIDECAR),
        "not written into the Sidecar: {}",
        context.display()
    );
}

/// The clone and nothing else: the authoring session's working directory is
/// nothing Varde moves, so no `cd` and no second command follow it.
#[then(expr = "the terminal has run nothing but the clone")]
fn terminal_ran_only_the_clone(world: &mut VardeWorld) {
    let [clone] = world.executed.as_slice() else {
        panic!("executed: {:?}", world.executed);
    };
    assert!(clone.contains("git clone"), "not a clone: {clone}");
}

/// A file in the clone rather than in the workspace — the Sidecar path the
/// core derived, so a scenario never spells the Sidecar itself.
fn guest_path(world: &VardeWorld, file: &str) -> PathBuf {
    world
        .state
        .guest
        .clone()
        .expect("a Guest repo under review")
        .join(file)
}

#[given(expr = "the Guest repo's file {string} holds:")]
#[when(expr = "the Guest repo's file {string} holds:")]
fn guest_file_holds(world: &mut VardeWorld, file: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let path = guest_path(world, &file);
    world.known_files.insert(path.clone());
    world.files.insert(path, contents);
    // The poll the edge runs against the repository under review, which for a
    // Guest repo is the clone: a Site's staleness is judged against what its
    // lines hold there.
    world.recompute_file_hunks();
}

/// A Guest repo's file as its range's base held it — the "held:" of the clone.
#[given(expr = "the Guest repo's file {string} held:")]
#[when(expr = "the Guest repo's file {string} held:")]
fn guest_file_held(world: &mut VardeWorld, file: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let path = guest_path(world, &file);
    world.known_files.insert(path.clone());
    world.held.insert(path.clone(), contents.clone());
    world.files.insert(path, contents);
    world.recompute_file_hunks();
}

#[when(expr = "the Guest repo's file {string} is opened in the editor")]
fn guest_file_is_opened(world: &mut VardeWorld, file: String) {
    let path = guest_path(world, &file);
    let contents = world.files.get(&path).cloned().unwrap_or_default();
    world.send(Event::BufferOpened {
        path,
        contents,
        preview: false,
        at: None,
    });
}

#[then(expr = "the Guest repo's file {string} was opened in the editor")]
fn guest_file_was_opened(world: &mut VardeWorld, file: String) {
    let path = guest_path(world, &file);
    assert!(
        world.opened.contains(&path),
        "opened: {:?}, wanted {}",
        world.opened,
        path.display()
    );
}

/// The watcher that already exists is what sees a clone end: the directory
/// git made in the Sidecar and the sentinel it wrote there, arriving as the
/// same event a file landing in any other watched folder does. The status is
/// the sentinel's contents, so a clone that failed is a file that says so
/// rather than a file that never comes.
#[when(expr = "the clone finishes with exit status {string}")]
#[when(expr = "the fetch finishes with exit status {string}")]
fn clone_finishes(world: &mut VardeWorld, status: String) {
    let story::Set::Downloading { url, .. } = world.state.story_set.clone() else {
        panic!("no download in flight: {:?}", world.state.story_set);
    };
    let sidecar = Path::new(SIDECAR);
    let sentinel = sidecar.join(story::DOWNLOAD_SENTINEL);
    world.files.insert(sentinel.clone(), status);
    world.send(Event::FilesAppeared(vec![
        (sidecar.join(story::guest_name(&url)), tree::Kind::Folder),
        (sentinel, tree::Kind::File),
    ]));
}

#[given(expr = "I was on the branch {string}")]
fn on_the_branch(world: &mut VardeWorld, name: String) {
    world.on_branch = name;
}

/// Walked to with the arrows and picked with Enter, never reached into: what
/// the picker answers to is the router's answer, and walking is what holds the
/// selection to being reachable with no modifier (R31.11).
#[when(expr = "I pick the branch {string}")]
fn pick_the_branch(world: &mut VardeWorld, name: String) {
    let row = |world: &VardeWorld| match &world.state.modal {
        Modal::Branches { row, .. } => *row,
        other => panic!("expected a branch picker, got {other:?}"),
    };
    let names = shown_branches(world);
    let wanted = names
        .iter()
        .position(|candidate| *candidate == name)
        .unwrap_or_else(|| panic!("no row for {name:?} in {names:?}"));
    while row(world) < wanted {
        press_key(world, terminput::KeyCode::Down);
    }
    while row(world) > wanted {
        press_key(world, terminput::KeyCode::Up);
    }
    press_key(world, terminput::KeyCode::Enter);
}

/// Typed a character at a time through the router, for the reason picking a
/// branch is walked to rather than reached into: what narrows the list is the
/// keys a reviewer presses.
#[when(expr = "I type {string} in the picker")]
fn type_in_the_picker(world: &mut VardeWorld, text: String) {
    for character in text.chars() {
        press_key(world, terminput::KeyCode::Char(character));
    }
}

#[then("the picker lists nothing")]
fn picker_lists_nothing(world: &mut VardeWorld) {
    assert_eq!(shown_branches(world), Vec::<String>::new());
}

#[then(expr = "the picker lists:")]
fn picker_lists(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a table of branch names")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(shown_branches(world), expected);
}

fn shown_branches(world: &VardeWorld) -> Vec<String> {
    match &world.state.modal {
        Modal::Branches { refs, filter, .. } => story::branches(refs, filter),
        other => panic!("expected a branch picker, got {other:?}"),
    }
}

fn press_key(world: &mut VardeWorld, code: terminput::KeyCode) {
    let mut drafts = std::mem::take(&mut world.drafts);
    for event in keys::on_key_event(&world.state, &mut drafts, terminput::KeyEvent::new(code), 0) {
        world.send(event);
    }
    world.drafts = drafts;
}

#[then(expr = "the branch {string} was checked out")]
fn branch_was_checked_out(world: &mut VardeWorld, name: String) {
    assert_eq!(world.checkouts, [(world.state.root.clone(), name)]);
}

/// In the clone, not in the workspace: the branch belongs to a repository the
/// folder Varde was opened on knows nothing about.
#[then(expr = "the branch {string} was checked out in the Guest repo")]
fn branch_was_checked_out_in_the_guest(world: &mut VardeWorld, name: String) {
    let guest = world
        .state
        .guest
        .clone()
        .expect("a Guest repo under review");
    assert!(
        guest.starts_with(SIDECAR),
        "the Guest repo is not in the Sidecar: {}",
        guest.display()
    );
    assert_eq!(world.checkouts, [(guest, name)]);
}

#[then(expr = "no branch was checked out")]
fn no_branch_was_checked_out(world: &mut VardeWorld) {
    assert!(
        world.checkouts.is_empty(),
        "checked out: {:?}",
        world.checkouts
    );
}

/// Both branches, because Varde does not check the original one back out: the
/// promise is that Story view says where the reviewer is *and* where they were.
#[then(expr = "the story view is on the branch {string} and left the branch {string}")]
fn story_view_names_the_branches(world: &mut VardeWorld, onto: String, left: String) {
    assert_eq!(world.state.branch, Some(onto));
    assert_eq!(world.state.left_branch, Some(left));
}

#[then(expr = "the story range is {string}")]
fn story_range_is(world: &mut VardeWorld, expected: String) {
    match &world.state.modal {
        Modal::ConfirmStory { spelling, .. } => assert_eq!(spelling, &expected),
        other => panic!("expected a ConfirmStory modal, got {other:?}"),
    }
}

#[then(expr = "the modal is {string}")]
fn modal_is(world: &mut VardeWorld, expected: String) {
    let actual = match &world.state.modal {
        Modal::None => "none",
        Modal::ConfirmStory { .. } => "confirm-story",
        Modal::ConfirmSubmit => "confirm-submit",
        Modal::Palette => "palette",
        Modal::Chord => "chord",
        Modal::Tools { .. } => "tools",
        Modal::Launches { .. } => "launches",
        Modal::Branches { .. } => "branches",
        Modal::Comment => "comment",
        Modal::NameBox { .. } => "name-box",
        Modal::StepDetail => "step-detail",
        Modal::Prediction { .. } => "prediction",
        Modal::Diverged => "diverged",
        Modal::Candidates(_) => "candidates",
        Modal::Stops { .. } => "stops",
        Modal::Restart => "restart",
        Modal::SetValue => "set-value",
        Modal::NewWatch => "new-watch",
        Modal::ExceptionClass => "exception-class",
        Modal::Breakpoint { .. } => "breakpoint",
        Modal::RunMark { .. } => "run-mark",
    };
    assert_eq!(actual, expected);
}

#[then(expr = "the overlay is {string}")]
fn overlay_is(world: &mut VardeWorld, expected: String) {
    modal_is(world, expected);
}

#[then(expr = "the overlay offers {int} choices")]
fn overlay_offers_choices(world: &mut VardeWorld, count: usize) {
    assert_eq!(story::prediction_choices(&world.state).len(), count);
}

#[then(expr = "the overlay offers no choices")]
fn overlay_offers_no_choices(world: &mut VardeWorld) {
    overlay_offers_choices(world, 0);
}

#[then(expr = "the overlay shows the feedback for choice {int}")]
fn overlay_shows_feedback_for_choice(world: &mut VardeWorld, choice: usize) {
    let step = story::current_step(&world.state).expect("a step");
    let prediction = step.prediction.as_ref().expect("a prediction");
    let expected = prediction.choices[choice - 1].feedback.as_str();
    assert_eq!(story::prediction_feedback(&world.state), Some(expected));
}

/// A wrong pick's feedback is its own choice's, never the correct choice's —
/// the one thing revealing it would remove any reason to think afterwards.
#[then(expr = "the overlay does not show the correct choice")]
fn overlay_does_not_show_correct_choice(world: &mut VardeWorld) {
    let step = story::current_step(&world.state).expect("a step");
    let prediction = step.prediction.as_ref().expect("a prediction");
    let correct = prediction
        .choices
        .iter()
        .find(|choice| choice.correct)
        .expect("a correct choice");
    assert_ne!(
        story::prediction_feedback(&world.state),
        Some(correct.feedback.as_str())
    );
}

#[then(expr = "the walkthrough records step {int} as put")]
fn walkthrough_records_step_as_put(world: &mut VardeWorld, step_number: usize) {
    let Some(story::Walking::Story { story, .. }) = world.state.walking else {
        panic!("walking a story");
    };
    assert!(world
        .state
        .predictions_put
        .contains(&(story, step_number - 1)));
}

/// `predictions_put` is a set of `(story, step)` pairs — there is no field a
/// choice could live in, so this reuses the same check
/// `walkthrough_records_step_as_put` makes rather than inventing a second one
/// that could tell a different story.
#[then(expr = "the walkthrough holds no choice")]
fn walkthrough_holds_no_choice(world: &mut VardeWorld) {
    let Some(story::Walking::Story { step, .. }) = world.state.walking else {
        panic!("walking a story");
    };
    walkthrough_records_step_as_put(world, step + 1);
}

#[when(expr = "the story set is re-authored")]
fn story_set_is_re_authored(world: &mut VardeWorld) {
    let story::Set::Loaded(artifact) = &world.state.story_set else {
        panic!("no story set loaded");
    };
    let contents = artifact_to_json(artifact).to_string();
    world.send(Event::StoryArtifact {
        contents,
        range: story::RangeStatus::Resolves,
    });
}

#[given(expr = "an AI session is running")]
fn ai_running_bare(world: &mut VardeWorld) {
    ai_running(world);
}

#[given(expr = "no AI session is running")]
fn ai_not_running_bare(world: &mut VardeWorld) {
    ai_not_running(world);
}

// "And I ran ..." continues whatever concrete keyword came before it, so this
// needs registering as a Given as well as the When `run_story_command` already is.
#[given(expr = "I ran {string}")]
fn ran_story_command(world: &mut VardeWorld, line: String) {
    run_story_command(world, line);
}

#[when(expr = "I confirm the story range")]
fn confirm_story_range(world: &mut VardeWorld) {
    world.send(Event::ConfirmStory);
}

#[when(expr = "I decline the story range")]
fn decline_story_range(world: &mut VardeWorld) {
    world.send(Event::Cancel);
}

#[then(expr = "the AI pane was sent a prompt naming the range {string}")]
fn prompt_names_the_range(world: &mut VardeWorld, range: String) {
    prompt_contains(world, range);
}

#[then(expr = "the AI session {string} was started")]
fn the_ai_session_was_started(world: &mut VardeWorld, command: String) {
    ai_started_with(world, command);
}

#[then(expr = "the AI pane was sent a prompt naming the file {string}")]
fn prompt_names_the_file(world: &mut VardeWorld, path: String) {
    prompt_contains(world, path);
}

/// The edge's own answer for what a spelling resolves to (ADR 0005): the
/// `Effect::ResolveStory` fake reads this to compute the exact `{OUT}` path a
/// confirmed range is authored to, the same map "that range is what … resolves
/// to" already populates for an already-authored range.
#[given(expr = "{string} resolves to base {string} and head {string}")]
fn spelling_resolves_to_oids(world: &mut VardeWorld, spelling: String, base: String, head: String) {
    world.resolved_oids.insert(spelling, (base, head));
}

#[given(expr = "the project holds {int} story sets")]
fn seed_story_sets(world: &mut VardeWorld, count: usize) {
    let dir = world.state.root.join(".varde/stories");
    for index in 0..count {
        let name = format!("story-{index:02}.json");
        world.state.story_sets.push(name.clone());
        world
            .files
            .insert(dir.join(story::context_path(&name)), "diff".to_string());
        world.files.insert(dir.join(name), "{}".to_string());
    }
}

#[when(expr = "a new story set is written")]
fn a_new_story_set_is_written(world: &mut VardeWorld) {
    let dir = world.state.root.join(".varde/stories");
    let name = "story-new.json".to_string();
    world.files.insert(dir.join(&name), "{}".to_string());
    world.send(Event::StoryFileWritten(name));
}

#[then(expr = "{int} story sets remain")]
fn story_sets_remain(world: &mut VardeWorld, expected: usize) {
    let dir = world.state.root.join(".varde/stories");
    let count = world
        .files
        .keys()
        .filter(|path| path.starts_with(&dir))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .count();
    assert_eq!(count, expected);
}

#[then(expr = "the oldest story set was pruned")]
fn oldest_story_set_was_pruned(world: &mut VardeWorld) {
    let dir = world.state.root.join(".varde/stories");
    assert!(!world.files.contains_key(&dir.join("story-00.json")));
}

#[then(expr = "the oldest story set's companion file was pruned")]
fn oldest_companion_file_was_pruned(world: &mut VardeWorld) {
    let dir = world.state.root.join(".varde/stories");
    assert!(!world.files.contains_key(&dir.join("story-00.context.md")));
    assert!(world.files.contains_key(&dir.join("story-01.context.md")));
}

#[then(expr = "the file {string} was written")]
fn the_file_was_written(world: &mut VardeWorld, path: String) {
    let full = world.state.root.join(&path);
    assert!(world.wrote.contains(&full), "{:?}", world.wrote);
}

#[then(expr = "no keys were sent to the AI pane")]
fn no_keys_to_ai_pane(world: &mut VardeWorld) {
    assert!(ai_sends(world).is_empty(), "{:?}", world.keys_sent);
}

#[then(expr = "no AI session was started")]
fn no_ai_session_started(world: &mut VardeWorld) {
    assert!(world.ai_spawned.is_empty());
}

#[then(expr = "the remainder holds {int} unclaimed hunk")]
#[then(expr = "the remainder holds {int} unclaimed hunks")]
fn remainder_holds_unclaimed(world: &mut VardeWorld, count: usize) {
    assert_eq!(story::remainder(&world.state).unclaimed, count);
}

#[then("the remainder lists:")]
fn remainder_lists(world: &mut VardeWorld, step: &Step) {
    let mut expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let mut actual = story::remainder(&world.state).locations;
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected);
}

#[then(expr = "the remainder reports {int} unwalked deletion")]
#[then(expr = "the remainder reports {int} unwalked deletions")]
fn remainder_reports_unwalked_deletions(world: &mut VardeWorld, count: usize) {
    assert_eq!(
        story::remainder(&world.state).unwalked_deletions,
        Some(count)
    );
}

#[then(expr = "the remainder reports no deletions line")]
fn remainder_reports_no_deletions_line(world: &mut VardeWorld) {
    assert_eq!(story::remainder(&world.state).unwalked_deletions, None);
}

#[then(expr = "the spine reports {int} stale step")]
#[then(expr = "the spine reports {int} stale steps")]
fn spine_reports_stale_steps(world: &mut VardeWorld, count: usize) {
    let total: usize = story::spine(&world.state).iter().map(|row| row.stale).sum();
    assert_eq!(total, count);
}

#[then(expr = "the review list is unchanged")]
fn review_list_unchanged(world: &mut VardeWorld) {
    assert_eq!(Some(review::list(&world.state)), world.repo_before);
}

// ---- F26: markdown preview ----

fn preview_rows(world: &VardeWorld) -> Vec<varde::preview::Row> {
    varde::preview_rows(&world.state)
}

fn row_at(world: &VardeWorld, number: usize) -> varde::preview::Row {
    let rows = preview_rows(world);
    rows.get(number - 1)
        .cloned()
        .unwrap_or_else(|| panic!("row {number} of {rows:?}"))
}

#[when(expr = "{string} is opened in the editor")]
fn open_file_in_editor(world: &mut VardeWorld, path: String) {
    let contents = world
        .files
        .get(&abs(world, &path))
        .cloned()
        .unwrap_or_default();
    open_buffer(world, &path, &contents);
}

#[given(expr = "no file is open in the editor")]
fn nothing_is_open(world: &mut VardeWorld) {
    world.state.current_buffer = None;
}

/// The pane's width is the layout's answer rather than a field, so the screen
/// it takes to make the pane that wide is solved for here — a magic terminal
/// size in the feature file would say nothing about what the scenario is for.
#[given(expr = "the editor pane is {int} columns wide")]
fn editor_pane_is_columns_wide(world: &mut VardeWorld, columns: u16) {
    let height = 40;
    let width = (columns..600)
        .find(|width| {
            layout::panes(
                *width,
                height,
                world.state.tree_divider as u16,
                world.state.ai_width.map(|width| width as u16),
                0,
                0,
                varde::shapes(&world.state),
            )
            .editor
            .width
                == columns
        })
        .unwrap_or_else(|| panic!("no screen width gives the editor pane {columns} columns"));
    world.send(Event::Resized { width, height });
}

/// A `Given` as well, because a Preview is the state a scenario about what
/// `:format` does to one has to start in, and a precondition nothing asserts
/// is a scenario that stops covering anything the day markdown stops opening
/// as one.
#[given(expr = "the editor is showing preview")]
#[then(expr = "the editor is showing preview")]
fn showing_preview(world: &mut VardeWorld) {
    assert!(varde::previewing(&world.state), "not previewing");
}

#[then(expr = "the editor is not showing preview")]
fn not_showing_preview(world: &mut VardeWorld) {
    assert!(!varde::previewing(&world.state), "previewing");
}

#[then(expr = "the editor is showing source")]
fn showing_source(world: &mut VardeWorld) {
    assert!(world.state.current_buffer.is_some(), "no file open");
    assert!(!varde::previewing(&world.state), "previewing");
}

#[then(expr = "the editor is showing a diff")]
fn showing_a_diff(world: &mut VardeWorld) {
    assert!(world.state.diff.is_some(), "no diff");
}

#[then(expr = "the editor is showing a story step")]
fn showing_a_story_step(world: &mut VardeWorld) {
    assert!(story::current_step(&world.state).is_some(), "no step");
}

#[then(expr = "the editor refuses with {string}")]
fn editor_refuses_with(world: &mut VardeWorld, reason: String) {
    assert_eq!(
        world
            .state
            .refusal
            .as_ref()
            .map(varde::preview::Refusal::as_str),
        Some(reason.as_str())
    );
}

/// Review view substitutes the editor's whole rectangle, so a diff has to be on
/// screen for the scenario to be about anything.
#[when(expr = "I switch to review view")]
fn switch_to_review_view(world: &mut VardeWorld) {
    let path = world
        .state
        .current_buffer
        .clone()
        .expect("a current buffer");
    world.send(Event::OpenReviewView);
    let file = path
        .strip_prefix(&world.state.root)
        .expect("inside the workspace")
        .to_string_lossy()
        .into_owned();
    let content = world
        .state
        .buffers
        .get(&path)
        .expect("the buffer")
        .shown()
        .to_string();
    let lines = content
        .lines()
        .enumerate()
        .map(|(index, text)| DiffLine {
            new_line: Some(index + 1),
            old_line: None,
            removed: false,
            text: text.to_string(),
        })
        .collect();
    let revision = blob_oid(&content);
    world.send(Event::ShowDiff {
        file,
        lines,
        revision,
    });
}

/// Walking is set directly for the reason "I am walking" sets it directly: this
/// is scenario setup, not the thing under test.
#[when(expr = "I walk a story step pointing at {string}")]
fn walk_a_step_pointing_at(world: &mut VardeWorld, file: String) {
    let full = world.state.root.join(&file);
    let held = world
        .state
        .buffers
        .get(&full)
        .map(|buffer| buffer.shown().to_string())
        .unwrap_or_else(|| "a line".to_string());
    world.known_files.insert(full.clone());
    world.files.insert(full, held);
    let steps = vec![build_step("s1.1", &file, "new", "changed", 1, 1, "")];
    let contents = build_story(
        "aaaaaaaaaaaa",
        "bbbbbbbbbbbb",
        "main..HEAD",
        vec![("Story".to_string(), steps)],
    );
    world.send(Event::StoryArtifact {
        contents,
        range: story::RangeStatus::Resolves,
    });
    world.state.view = View::Story;
    world.state.focus = Pane::Editor;
    world.state.walking = Some(story::Walking::Story {
        story: 0,
        step: 0,
        diff: story::Diff::Hidden,
    });
}

#[then(expr = "row {int} is a heading")]
fn row_is_a_heading(world: &mut VardeWorld, number: usize) {
    assert!(matches!(
        row_at(world, number).kind,
        varde::preview::RowKind::Heading(_)
    ));
}

#[then(expr = "row {int} is metadata")]
fn row_is_metadata(world: &mut VardeWorld, number: usize) {
    assert_eq!(
        row_at(world, number).kind,
        varde::preview::RowKind::Metadata
    );
}

#[then(expr = "the last row is a paragraph")]
fn last_row_is_a_paragraph(world: &mut VardeWorld) {
    let rows = preview_rows(world);
    let last = rows.last().expect("no rows");
    assert_eq!(last.kind, varde::preview::RowKind::Paragraph, "{rows:?}");
}

#[then(expr = "row {int} holds {string}")]
fn row_holds(world: &mut VardeWorld, number: usize, text: String) {
    let row = row_at(world, number);
    assert!(
        row.text().contains(&text),
        "row {number} holds {:?}",
        row.text()
    );
}

#[then(expr = "no row holds {string}")]
fn no_row_holds(world: &mut VardeWorld, text: String) {
    let rows = preview_rows(world);
    assert!(
        !rows.iter().any(|row| row.text().contains(&text)),
        "{rows:?}"
    );
}

#[then(expr = "there is more than {int} row")]
fn more_than_rows(world: &mut VardeWorld, count: usize) {
    let rows = preview_rows(world);
    assert!(rows.len() > count, "{rows:?}");
}

#[then(expr = "every row is a paragraph")]
fn every_row_is_a_paragraph(world: &mut VardeWorld) {
    let rows = preview_rows(world);
    assert!(!rows.is_empty(), "no rows");
    assert!(
        rows.iter()
            .all(|row| row.kind == varde::preview::RowKind::Paragraph),
        "{rows:?}"
    );
}

#[then(expr = "every row comes from source line {int}")]
fn every_row_comes_from_line(world: &mut VardeWorld, line: usize) {
    let rows = preview_rows(world);
    assert!(!rows.is_empty(), "no rows");
    assert!(rows.iter().all(|row| row.line == line), "{rows:?}");
}

#[then(expr = "every row is a diagram")]
fn every_row_is_a_diagram(world: &mut VardeWorld) {
    let rows = preview_rows(world);
    assert!(!rows.is_empty(), "no rows");
    assert!(
        rows.iter()
            .all(|row| row.kind == varde::preview::RowKind::Diagram),
        "{rows:?}"
    );
}

#[then(expr = "some row holds {string}")]
fn some_row_holds(world: &mut VardeWorld, text: String) {
    let rows = preview_rows(world);
    assert!(
        rows.iter().any(|row| row.text().contains(&text)),
        "{rows:?}"
    );
}

#[then(expr = "the diagram was refused with {string}")]
fn diagram_was_refused_with(world: &mut VardeWorld, reason: String) {
    let rows = preview_rows(world);
    assert!(
        rows.iter().any(
            |row| row.refused.map(varde::preview::DiagramRefusal::as_str) == Some(reason.as_str())
        ),
        "{rows:?}"
    );
}

#[then(expr = "there is {int} code row")]
fn there_is_n_code_rows(world: &mut VardeWorld, count: usize) {
    let rows = preview_rows(world);
    let code = rows
        .iter()
        .filter(|row| row.kind == varde::preview::RowKind::Code)
        .count();
    assert_eq!(code, count, "{rows:?}");
}

fn the_code_row(world: &VardeWorld) -> varde::preview::Row {
    preview_rows(world)
        .into_iter()
        .find(|row| row.kind == varde::preview::RowKind::Code)
        .unwrap_or_else(|| panic!("no code row"))
}

#[then(expr = "the code row holds {string}")]
fn the_code_row_holds(world: &mut VardeWorld, text: String) {
    let row = the_code_row(world);
    assert!(row.text().contains(&text), "{:?}", row.text());
}

fn code_row_kind(world: &VardeWorld, text: &str) -> Option<varde::highlight::Kind> {
    let row = the_code_row(world);
    row.pieces
        .iter()
        .find(|piece| piece.text == text)
        .unwrap_or_else(|| panic!("no piece {text:?} in {row:?}"))
        .token
}

#[then(expr = "{string} in the code row is a keyword")]
fn token_in_code_row_is_a_keyword(world: &mut VardeWorld, text: String) {
    assert_eq!(
        code_row_kind(world, &text),
        Some(varde::highlight::Kind::Keyword)
    );
}

#[then(expr = "{string} in the code row is a type")]
fn token_in_code_row_is_a_type(world: &mut VardeWorld, text: String) {
    assert_eq!(
        code_row_kind(world, &text),
        Some(varde::highlight::Kind::Type)
    );
}

#[then(expr = "{string} in the code row is a function")]
fn token_in_code_row_is_a_function(world: &mut VardeWorld, text: String) {
    assert_eq!(
        code_row_kind(world, &text),
        Some(varde::highlight::Kind::Function)
    );
}

#[then(expr = "every token in the code row is plain text")]
fn every_token_in_code_row_is_plain(world: &mut VardeWorld) {
    let row = the_code_row(world);
    assert!(
        row.pieces
            .iter()
            .all(|piece| piece.token == Some(varde::highlight::Kind::Plain)),
        "{row:?}"
    );
}

#[then(expr = "the cursor is on row {int}")]
fn cursor_is_on_row(world: &mut VardeWorld, number: usize) {
    assert_eq!(current_buffer(world).row, number);
}

/// The Preview cursor's own pair, which is *not* the buffer's line and column:
/// a rendered column indexes what is drawn, so a scenario that asserted the
/// source pair here would pass on a cursor nobody can see.
#[then(expr = "the cursor is on row {int} column {int}")]
fn cursor_is_on_row_column(world: &mut VardeWorld, row: usize, column: usize) {
    let buffer = current_buffer(world);
    assert_eq!((buffer.row, buffer.row_column), (row, column));
}

fn row_holding(world: &VardeWorld, text: &str) -> usize {
    let rows = preview_rows(world);
    rows.iter()
        .position(|row| row.text().contains(text))
        .unwrap_or_else(|| panic!("no row holds {text:?}: {rows:?}"))
        + 1
}

#[given(expr = "the cursor is on the row holding {string}")]
fn place_cursor_on_row_holding(world: &mut VardeWorld, text: String) {
    let row = row_holding(world, &text);
    current_buffer_mut(world).row = row;
}

#[then(expr = "the cursor is on the row holding {string}")]
fn cursor_is_on_row_holding(world: &mut VardeWorld, text: String) {
    let row = row_holding(world, &text);
    assert_eq!(current_buffer(world).row, row);
}

#[then(expr = "the cursor is on source line {int}")]
fn cursor_is_on_source_line(world: &mut VardeWorld, line: usize) {
    let number = current_buffer(world).row;
    assert_eq!(row_at(world, number).line, line);
}

#[then(expr = "the editor has no line-number gutter")]
fn no_line_number_gutter(world: &mut VardeWorld) {
    assert_eq!(varde::gutter(&world.state), 0);
}

#[then(expr = "the editor has a line-number gutter")]
fn a_line_number_gutter(world: &mut VardeWorld) {
    assert_ne!(varde::gutter(&world.state), 0);
}

/// The mode clause is the part of the title the core owns; the file name beside
/// it is `ui`'s to compose, and a `ui` unit test holds the two together.
#[then(expr = "the editor title says {string}")]
fn editor_title_says(world: &mut VardeWorld, word: String) {
    assert_eq!(varde::mode_label(&world.state, current_buffer(world)), word);
}

#[then(expr = "the editor title does not say {string}")]
fn editor_title_does_not_say(world: &mut VardeWorld, word: String) {
    assert_ne!(varde::mode_label(&world.state, current_buffer(world)), word);
}

#[given(expr = "I type {string} into the buffer as its only line")]
#[when(expr = "I type {string} into the buffer as its only line")]
fn type_as_only_line(world: &mut VardeWorld, text: String) {
    for key in ['d', 'd', 'i'] {
        world.send(Event::EditorKey(key));
    }
    for key in text.chars() {
        world.send(Event::EditorKey(key));
    }
    world.send(Event::EditorEscape);
}

// ---- F27: Risk ----

fn scope(name: &str) -> Scope {
    match name {
        "workspace" => Scope::Workspace,
        "review" => Scope::Review,
        other => panic!("unknown scope {other:?}"),
    }
}

/// One `Effect::AnalyseRisk`, as the world saw it: what it covers, which
/// request it is, the files it names and the revision it measures them from as
/// well. Recorded whole, so a scenario can hold the delta's base against the
/// diff's rather than against a copy of the same decision.
#[derive(Debug)]
struct Analysis {
    scope: Scope,
    generation: u64,
    files: Option<Vec<String>>,
    base: Option<String>,
}

/// The table the analyser's answer is written as: one row per space it found,
/// each under its own file, plus one unreadable file per Unparsed the scenario
/// declared — exactly the shape the edge hands `risk::figures`, so the count of
/// what could not be read is the library's answer rather than the glue's.
fn figures(step: &Step, unparsed: usize) -> Figures {
    figures_from(step, unparsed, "")
}

/// The same table read for the other side of a delta: the `was …` columns, which
/// say what each Function measured at the base revision.
fn figures_from(step: &Step, unparsed: usize, prefix: &str) -> Figures {
    let table = step.table().expect("a table");
    let header = &table.rows[0];
    let column = |row: &Vec<String>, name: &str| {
        header
            .iter()
            .position(|heading| heading == name)
            .and_then(|at| row.get(at))
            .cloned()
            .unwrap_or_default()
    };
    let mut analysed: Vec<(String, Option<Space>)> = (0..unparsed)
        .map(|which| (format!("unreadable-{which}"), None))
        .collect();
    for row in &table.rows[1..] {
        let file = column(row, "file");
        let name = column(row, "function");
        let figure = |name: &str| {
            column(row, &format!("{prefix}{name}"))
                .parse()
                .unwrap_or_default()
        };
        let space = Space {
            name: (!name.is_empty()).then(|| name.clone()),
            line: column(row, "line").parse().unwrap_or_default(),
            kind: Kind::Function,
            metrics: Metrics {
                cyclomatic: figure("cyclomatic"),
                cognitive: figure("cognitive"),
                maintainability: figure("maintainability"),
                lines: figure("lines"),
            },
            children: Vec::new(),
        };
        analysed.push((
            file.clone(),
            Some(Space {
                name: Some(file),
                line: 1,
                kind: Kind::Unit,
                metrics: Metrics::default(),
                children: vec![space],
            }),
        ));
    }
    risk::figures(analysed)
}

#[given(expr = "the risk threshold is {int}")]
fn risk_threshold(world: &mut VardeWorld, threshold: u32) {
    world.state.risk_threshold = threshold;
    world.startup.project_config = Some(format!("[risk]\nthreshold = {threshold}\n"));
}

/// No coverage report is read anywhere yet, so this is the only situation
/// there is — the step exists to make the scenario say which one it is.
#[given(expr = "no test coverage was read")]
fn no_test_coverage(_world: &mut VardeWorld) {}

#[given(expr = "every file in the workspace is in a language the analyser does not handle")]
fn nothing_analysable(_world: &mut VardeWorld) {}

/// The edge's own sequence: an answer only exists because a request was made,
/// so a scenario that says the figures arrived without having opened Varde gets
/// the request too — the core takes an answer only while one is waiting.
fn deliver(world: &mut VardeWorld, scope: Scope, figures: Figures, before: Option<Figures>) {
    if !world.state.risk.in_flight() {
        let asked = risk::analyse(&mut world.state, scope);
        world.apply(vec![asked]);
    }
    world.send(Event::RiskFigures {
        generation: world.state.risk.asked,
        figures,
        before,
    });
}

#[when(expr = "the analysis finishes")]
fn analysis_finishes(world: &mut VardeWorld) {
    deliver(world, Scope::Workspace, Figures::default(), None);
}

/// The figure, and the files it describes: a Function named at line 88 implies a
/// file with 88 lines in it, so the files go on disk too. Without them a jump to
/// a Function's line lands on line 1 — the buffer clamps to what it holds — and
/// "the cursor is on line 17" would pass for a file that was never there.
#[given(expr = "the figures were computed for the scope {string}:")]
fn figures_were_computed(world: &mut VardeWorld, name: String, step: &Step) {
    let computed = figures(step, 0);
    for function in &computed.functions {
        let lines = function.line.max(1);
        let contents: String = (1..=lines).map(|line| format!("line {line}\n")).collect();
        match world
            .project
            .iter_mut()
            .find(|(file, _)| *file == function.file)
        {
            Some((_, held)) if held.lines().count() < lines => *held = contents,
            Some(_) => {}
            None => world.project.push((function.file.clone(), contents)),
        }
    }
    world.state.risk.figure = risk::Figure::Current(computed);
    assert_eq!(scope(&name), Scope::Workspace);
}

#[when(expr = "the figures arrive for the scope {string}:")]
fn figures_arrive(world: &mut VardeWorld, name: String, step: &Step) {
    deliver(world, scope(&name), figures(step, 0), None);
}

#[when(expr = "the figure goes stale")]
fn figure_goes_stale(world: &mut VardeWorld) {
    risk::went_stale(&mut world.state.risk);
}

#[given(expr = "the figure has gone stale")]
fn figure_has_gone_stale(world: &mut VardeWorld) {
    risk::went_stale(&mut world.state.risk);
}

/// The other half of the pair, so the explicit recompute is specified against
/// both: a request that only worked on a stale figure would pass one of them.
#[given(expr = "the figure has not gone stale")]
fn figure_has_not_gone_stale(world: &mut VardeWorld) {
    assert_eq!(risk::view_state(&world.state), "computed");
}

#[when(expr = "I ask for the figures to be recomputed")]
fn ask_for_recompute(world: &mut VardeWorld) {
    world.send(Event::RecomputeRisk);
}

/// The cache as a previous run left it: one Function, so the figure it restores
/// is a figure rather than a workspace with nothing analysed.
#[given(expr = "the figures were recorded at the commit {string}")]
fn figures_recorded_at_commit(world: &mut VardeWorld, commit: String) {
    let figures = Figures {
        functions: vec![Function {
            file: "src/keys.rs".to_string(),
            name: "route".to_string(),
            line: 88,
            metrics: Metrics {
                cyclomatic: 31,
                cognitive: 24,
                maintainability: 41,
                lines: 96,
            },
        }],
        unparsed: 0,
    };
    world.startup.risk_json = Some(risk::persist(&figures, &commit));
}

fn written_figures(world: &VardeWorld) -> risk::Persisted {
    let path =
        varde::varde_dir(&world.startup.root, world.startup.sidecar.as_deref()).join(risk::FILE);
    let json = world
        .files
        .get(&path)
        .unwrap_or_else(|| panic!("nothing at {path:?}: {:?}", world.files.keys()));
    serde_json::from_str(json).expect("Varde's own shape")
}

#[then(expr = "the figures were written to {string}")]
fn figures_were_written(world: &mut VardeWorld, path: String) {
    assert_eq!(path, format!("{}/{}", varde::VARDE_DIR, risk::FILE));
    written_figures(world);
}

#[then(expr = "the written figures record the commit {string}")]
fn written_figures_record_commit(world: &mut VardeWorld, commit: String) {
    assert_eq!(written_figures(world).commit, commit);
}

#[then(expr = "the written figures record the metric {string}")]
fn written_figures_record_metric(world: &mut VardeWorld, metric: String) {
    assert_eq!(written_figures(world).metric, metric);
}

/// Field for field against the table, so the file's shape is pinned rather than
/// merely present — and the analyser's types cannot appear in it, because the
/// step deserializes Varde's own.
#[then(expr = "the written figures list:")]
fn written_figures_list(world: &mut VardeWorld, step: &Step) {
    assert_eq!(written_figures(world).functions, figures(step, 0).functions);
}

#[when(expr = "the figures arrive for the scope {string} with {int} files Unparsed:")]
fn figures_arrive_with_unparsed(
    world: &mut VardeWorld,
    name: String,
    unparsed: usize,
    step: &Step,
) {
    deliver(world, scope(&name), figures(step, unparsed), None);
}

/// The request the world records but never answers, exactly as the edge's
/// thread would leave it: `asked` is ahead of `answered`, so "computing" is a
/// state a scenario can stand in.
#[given(expr = "an analysis is in flight over the scope {string}")]
fn analysis_in_flight(world: &mut VardeWorld, name: String) {
    let asked = risk::analyse(&mut world.state, scope(&name));
    world.apply(vec![asked]);
}

#[then(expr = "an analysis was asked for over the scope {string}")]
fn analysis_asked_for(world: &mut VardeWorld, name: String) {
    assert!(
        world
            .analyses
            .iter()
            .any(|asked| asked.scope == scope(&name)),
        "asked for {:?}",
        world.analyses
    );
}

#[then(expr = "no analysis was asked for over the scope {string}")]
fn no_analysis_asked_for_scope(world: &mut VardeWorld, name: String) {
    assert!(
        !world
            .analyses
            .iter()
            .any(|asked| asked.scope == scope(&name)),
        "asked for {:?}",
        world.analyses
    );
}

/// At most one, by construction — the count is asserted anyway, because the
/// promise is that a recompute supersedes rather than queues and a queue would
/// be the plausible-looking implementation.
#[then(expr = "{int} analysis is in flight")]
fn analyses_in_flight(world: &mut VardeWorld, expected: usize) {
    assert_eq!(usize::from(world.state.risk.in_flight()), expected);
}

/// Falsifiable rather than incidental: the earlier job's answer is delivered,
/// and the figure it carries must not appear. A core that took the last answer
/// to arrive would show it.
#[then(expr = "the earlier analysis was superseded")]
fn earlier_analysis_superseded(world: &mut VardeWorld) {
    let generation = world
        .analyses
        .first()
        .expect("an analysis was asked for")
        .generation;
    assert!(
        generation < world.state.risk.asked,
        "nothing superseded it: {:?}",
        world.analyses
    );
    world.send(Event::RiskFigures {
        generation,
        figures: Figures {
            // A figure nothing else in the suite produces, so a count of one
            // here could only have come from the superseded answer.
            functions: vec![Function {
                file: "src/superseded.rs".to_string(),
                name: "gone".to_string(),
                line: 1,
                metrics: Metrics {
                    cyclomatic: 99,
                    ..Metrics::default()
                },
            }],
            unparsed: 0,
        },
        before: None,
    });
    assert_eq!(
        risk::risk_count(&world.state),
        None,
        "the superseded figure was taken"
    );
    assert!(world.state.risk.in_flight(), "the fresh job was answered");
}

#[then(expr = "no analysis was asked for")]
fn no_analysis_asked_for(world: &mut VardeWorld) {
    assert!(world.analyses.is_empty(), "asked for {:?}", world.analyses);
}

/// The name and the caption together: a job with no caption is the hang the
/// spinner exists to rule out, so a scenario that names the job also holds the
/// border to saying what is being worked on.
#[then(expr = "the job in flight is {string}")]
fn the_job_in_flight(world: &mut VardeWorld, expected: String) {
    let (name, caption) = risk::job(&world.state).expect("no job in flight");
    assert_eq!(name, expected);
    assert!(!caption.is_empty(), "a spinner with no caption");
}

#[then(expr = "the tree border risk state is {string}")]
fn tree_border_risk_state(world: &mut VardeWorld, expected: String) {
    assert_eq!(risk::view_state(&world.state), expected);
}

#[then(expr = "the risk count is {int}")]
fn the_risk_count(world: &mut VardeWorld, expected: usize) {
    assert_eq!(risk::risk_count(&world.state), Some(expected));
}

#[then(expr = "no risk count is shown")]
fn no_risk_count(world: &mut VardeWorld) {
    assert_eq!(risk::risk_count(&world.state), None);
}

#[then(expr = "the risk metric is {string}")]
fn the_risk_metric(world: &mut VardeWorld, expected: String) {
    assert_eq!(risk::METRIC, expected);
    assert!(world.state.risk.figures().is_some(), "no figures to label");
}

#[then(expr = "the unparsed count is {int}")]
fn the_unparsed_count(world: &mut VardeWorld, expected: usize) {
    match world.state.risk.figures() {
        Some(figures) => assert_eq!(figures.unparsed, expected),
        None => panic!("no figures: {:?}", world.state.risk),
    }
}

// ---- F28: the Risk list pane ----

#[then(expr = "the view palette offers {string} in the group {string} under the key {string}")]
fn palette_offers_entry(world: &mut VardeWorld, entry: String, group: String, key: String) {
    assert_eq!(world.state.modal, Modal::Palette);
    let offered: Vec<(String, char, String)> = PALETTE
        .iter()
        .flat_map(|(heading, entries)| {
            entries
                .iter()
                .map(|(key, label)| (heading.to_string(), *key, label.trim().to_string()))
        })
        .collect();
    assert!(
        offered.contains(&(group.clone(), parse_char(&key), entry.clone())),
        "{group}/{key}/{entry} is not offered: {offered:?}"
    );
    // Un-indented, because this is a pane rather than another pane's shape —
    // the indent is part of the label the palette draws.
    let drawn = PALETTE
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .find(|(_, label)| label.trim() == entry)
        .map(|(_, label)| *label)
        .expect("the entry");
    assert_eq!(drawn, entry, "the entry is indented under another");
}

/// Through the toggle, not by poking the field: the only way the pane comes to
/// be on screen is being asked for, and asking for it puts focus in it (R28.8).
/// A Given that set the field alone would describe a state no gesture produces,
/// and every scenario about the pane's own keys would be pressing them at
/// whatever pane the default focus is on.
#[given(expr = "the Risk list is shown")]
fn risk_list_is_shown(world: &mut VardeWorld) {
    if world.state.corner != layout::Corner::Risk {
        world.send(Event::ToggleRiskList);
    }
    assert_eq!(world.state.focus, Pane::Risk);
}

#[given(expr = "the Risk list is hidden")]
fn risk_list_is_hidden(world: &mut VardeWorld) {
    world.state.corner = layout::Corner::Hidden;
}

#[when(expr = "I show the Risk list")]
fn show_risk_list(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Hidden);
    world.send(Event::ToggleRiskList);
}

#[then(expr = "the Risk list is shown")]
fn risk_list_should_be_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Risk);
}

#[then(expr = "the Risk list is hidden")]
fn risk_list_should_be_hidden(world: &mut VardeWorld) {
    assert_ne!(world.state.corner, layout::Corner::Risk);
}

#[given(expr = "the Risk list pane has focus")]
fn risk_pane_has_focus(world: &mut VardeWorld) {
    world.state.focus = Pane::Risk;
}

#[then(expr = "the Risk list pane has focus")]
fn risk_pane_should_have_focus(world: &mut VardeWorld) {
    assert_eq!(world.state.focus, Pane::Risk);
}

#[given(expr = "the project {string} records the Risk list as {word}")]
fn state_records_risk_list(world: &mut VardeWorld, path: String, shown: String) {
    assert_eq!(path, ".varde/state.json");
    let recorded = match shown.as_str() {
        "shown" => "Shown",
        "hidden" => "Hidden",
        other => panic!("unknown state {other}"),
    };
    world.startup.state_json = Some(format!("{{\"risk_list\": \"{recorded}\"}}"));
}

#[when(expr = "I show every Function in the Risk list")]
fn show_every_function(world: &mut VardeWorld) {
    world.send(Event::ToggleRiskAll);
}

/// The rows as the pane draws them: the Function, its figure, and — where the
/// table asks for one — how far the change moved it.
#[then("the Risk list shows:")]
fn risk_list_shows(world: &mut VardeWorld, step: &Step) {
    let table = step.table().expect("a table");
    let deltas = table.rows[0].iter().any(|heading| heading == "delta");
    let expected: Vec<(String, u32, Option<i64>)> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            (
                row[0].clone(),
                row[1].parse().expect("a figure"),
                deltas.then(|| row[2].parse().expect("a delta")),
            )
        })
        .collect();
    let shown: Vec<(String, u32, Option<i64>)> = risk::list(&world.state)
        .iter()
        .map(|function| {
            (
                function.name.clone(),
                function.metrics.cyclomatic,
                deltas.then(|| {
                    risk::row_delta(&world.state, function).expect("the row says nothing about it")
                }),
            )
        })
        .collect();
    assert_eq!(shown, expected);
}

#[then(expr = "the Risk list is empty")]
fn risk_list_is_empty(world: &mut VardeWorld) {
    assert!(
        risk::list(&world.state).is_empty(),
        "{:?}",
        risk::list(&world.state)
    );
}

#[then(expr = "the Risk list state is {string}")]
fn risk_list_state(world: &mut VardeWorld, expected: String) {
    assert_eq!(risk::view_state(&world.state), expected);
}

/// On top of the figure the Background computed, the way the edge would have
/// counted them alongside it.
#[given(expr = "{int} files were Unparsed")]
fn files_were_unparsed(world: &mut VardeWorld, count: usize) {
    match &mut world.state.risk.figure {
        risk::Figure::Current(figures) | risk::Figure::Stale(figures) => figures.unparsed = count,
        risk::Figure::None => panic!("no figure to count them against"),
    }
}

#[given(expr = "no figures have been computed")]
fn no_figures_computed(world: &mut VardeWorld) {
    world.state.risk.figure = risk::Figure::None;
}

#[then(expr = "the Risk list unparsed count is {int}")]
fn risk_list_unparsed(world: &mut VardeWorld, expected: usize) {
    assert_eq!(risk::unparsed(&world.state), expected);
}

// ---- F28: navigating the list and opening a Function ----

/// The selection by the name of the Function it is on: an index is what the
/// core holds, but a scenario about a worklist names the row.
fn risk_row(world: &VardeWorld, function: &str) -> usize {
    risk::list(&world.state)
        .iter()
        .position(|row| row.name == function)
        .unwrap_or_else(|| {
            panic!(
                "no row named {function}: {:?}",
                risk::list(&world.state)
                    .iter()
                    .map(|row| &row.name)
                    .collect::<Vec<_>>()
            )
        })
}

#[given(expr = "the Risk list selection is {string}")]
fn risk_selection_is(world: &mut VardeWorld, function: String) {
    world.state.risk_selection = risk_row(world, &function);
}

#[then(expr = "the Risk list selection is {string}")]
fn risk_selection_should_be(world: &mut VardeWorld, function: String) {
    assert_eq!(
        risk::selected(&world.state).map(|row| row.name.clone()),
        Some(function)
    );
}

#[given(expr = "the Risk list selection is the first row")]
#[then(expr = "the Risk list selection is the first row")]
fn risk_selection_is_the_first_row(world: &mut VardeWorld) {
    assert_eq!(world.state.risk_selection, 0);
}

/// Focus in the pane, then the motion that walks off the end of the list —
/// driven rather than assigned, because "one slot past the last row" is the
/// behaviour under test and a Given that set the index would still hold if the
/// motion stopped reaching it.
#[given(expr = "the keyboard is on the Risk list pane actions")]
fn keyboard_is_on_the_pane_actions(world: &mut VardeWorld) {
    world.state.focus = Pane::Risk;
    for _ in 0..=risk::list(&world.state).len() {
        world.send(Event::MoveSelection(varde::Direction::Down));
    }
    assert!(risk::on_actions(&world.state));
}

#[then(expr = "the keyboard is on the Risk list pane actions")]
fn keyboard_should_be_on_the_pane_actions(world: &mut VardeWorld) {
    assert!(
        risk::on_actions(&world.state),
        "on row {}",
        world.state.risk_selection
    );
}

/// By name out of the list the pane draws, never by index: the icon that is lit
/// and the action Enter runs are read from one list, so the step cannot agree
/// with a renderer that has drifted from it.
#[then(expr = "the armed Risk list action is {string}")]
fn armed_risk_action_is(world: &mut VardeWorld, expected: String) {
    let actions = match risk::on_actions(&world.state) {
        true => risk::pane_actions(&world.state),
        false => risk::row_actions(&world.state),
    };
    let armed = world
        .state
        .selected_action
        .and_then(|at| actions.get(at).copied());
    assert_eq!(armed, Some(expected.as_str()));
}

#[then(expr = "no Risk list action is armed")]
fn no_risk_action_is_armed(world: &mut VardeWorld) {
    assert_eq!(world.state.selected_action, None);
}

#[then(expr = "the Risk list border shows the file {string}")]
fn risk_border_shows_file(world: &mut VardeWorld, file: String) {
    assert_eq!(
        risk::selected(&world.state).map(|row| row.file.clone()),
        Some(file)
    );
}

/// The click the mouse would report, by the row it lands on — `mouse` resolves a
/// screen position into this index, and its own test pins that arithmetic.
#[when(expr = "I click the Risk list row {string}")]
fn click_risk_row(world: &mut VardeWorld, function: String) {
    let index = risk_row(world, &function);
    world.send(Event::ClickRiskRow(index));
}

/// More Functions than the pane has rows, so scrolling is a thing that can
/// happen at all. Descending figures, all above the threshold, so the list holds
/// every one of them in the order they are made.
#[given(expr = "the Risk list holds {int} Functions")]
fn risk_list_holds(world: &mut VardeWorld, count: u32) {
    let functions = (0..count)
        .map(|which| Function {
            file: format!("src/{which}.rs"),
            name: format!("f{which}"),
            line: 1,
            metrics: Metrics {
                cyclomatic: 1000 - which,
                ..Metrics::default()
            },
        })
        .collect();
    world.state.risk.figure = risk::Figure::Current(Figures {
        functions,
        unparsed: 0,
    });
    assert_eq!(risk::list(&world.state).len() as u32, count);
}

#[then(expr = "the Risk list first visible row is row {int}")]
fn risk_first_visible_row(world: &mut VardeWorld, row: usize) {
    assert_eq!(world.state.risk_scroll, row);
}

#[then(expr = "the Risk list selection is in view")]
fn risk_selection_in_view(world: &mut VardeWorld) {
    let first = world.state.risk_scroll;
    let last = first + varde::corner_rows(&world.state).max(1);
    assert!(
        (first..last).contains(&world.state.risk_selection),
        "row {} is not among the rows {first}..{last} on screen",
        world.state.risk_selection
    );
}

// ---- F29: a single Function's refactor ----

#[then("the Risk list row actions offered are:")]
fn risk_row_actions_offered(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let actual: Vec<String> = risk::row_actions(&world.state)
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(actual, expected);
}

/// The keyboard path and the click path are the same gesture, so both steps
/// raise the one event the row's icon and the armed Enter both raise.
#[given(expr = "I ask for a refactor of the Function {string}")]
#[when(expr = "I ask for a refactor of the Function {string}")]
fn ask_for_a_refactor(world: &mut VardeWorld, function: String) {
    world.state.risk_selection = risk_row(world, &function);
    world.send(Event::RowAction(risk::REFACTOR));
}

#[when(expr = "I click the {string} action on the Risk list row {string}")]
fn click_risk_row_action(world: &mut VardeWorld, action: String, row: String) {
    assert_eq!(action, risk::REFACTOR, "unknown action {action:?}");
    ask_for_a_refactor(world, row);
}

#[then(expr = "the AI pane was sent a prompt naming the Function {string}")]
fn prompt_names_the_function(world: &mut VardeWorld, function: String) {
    prompt_contains(world, function);
}

#[then(expr = "the AI pane was sent a prompt naming the figure {int}")]
fn prompt_names_the_figure(world: &mut VardeWorld, figure: u32) {
    prompt_contains(world, figure.to_string());
}

#[then(expr = "exactly {int} prompt was sent to the AI")]
#[then(expr = "exactly {int} prompts were sent to the AI")]
fn exactly_this_many_prompts(world: &mut VardeWorld, expected: usize) {
    assert_eq!(ai_sends(world).len(), expected, "{:?}", world.keys_sent);
}

/// No branch anywhere may test which CLI is running, and neither may a prompt
/// address one: the same prompt drives the providers nobody has tried. The
/// names are the ones a plausible prompt would reach for, the configured
/// command among them — a scenario sets `ai.command` to one of them.
#[then(expr = "the prompt names no AI provider")]
fn prompt_names_no_provider(world: &mut VardeWorld) {
    let sent = ai_sends(world).join("\n").to_lowercase();
    for provider in [
        "claude",
        "anthropic",
        "codex",
        "openai",
        "chatgpt",
        "gpt",
        "copilot",
        "cursor",
        "gemini",
        "aider",
        "llama",
    ] {
        assert!(
            !sent.contains(provider),
            "the prompt names {provider:?}:\n{sent}"
        );
    }
}

// ---- F29: one gated Iteration ----

/// Adds a key under `[risk]`, keeping what an earlier step put there: the
/// Refactor loop's Background configures three of them, and a step that
/// replaced the file would leave the last one standing alone.
fn risk_config(world: &mut VardeWorld, line: &str) {
    let mut config = world
        .startup
        .project_config
        .clone()
        .unwrap_or_else(|| "[risk]\n".to_string());
    if !config.contains("[risk]") {
        config.push_str("[risk]\n");
    }
    config.push_str(line);
    config.push('\n');
    world.startup.project_config = Some(config);
}

#[given(expr = "the configured test command is {string}")]
fn configured_test_command(world: &mut VardeWorld, command: String) {
    world.state.test_command = Some(command.clone());
    risk_config(world, &format!("test_command = {command:?}"));
}

#[given(expr = "no {string} is configured")]
fn nothing_configured(world: &mut VardeWorld, key: String) {
    assert_eq!(key, "risk.test_command", "unknown key {key:?}");
    world.state.test_command = None;
}

#[given(expr = "the iteration cap is {int}")]
fn iteration_cap(world: &mut VardeWorld, cap: u32) {
    world.state.max_iterations = cap;
    risk_config(world, &format!("max_iterations = {cap}"));
}

/// A file in the project's root, which is what the shape is read off: the
/// marker the project holds is what says which command its tests are behind.
#[given(expr = "the project holds {string}")]
fn project_holds_file(world: &mut VardeWorld, name: String) {
    let root = world.state.root.clone();
    world.state.contents.entry(root).or_default().push(Entry {
        name,
        is_dir: false,
    });
}

#[given(expr = "the project's shape names no test command")]
fn shape_names_no_test_command(world: &mut VardeWorld) {
    let root = world.state.root.clone();
    world.state.contents.insert(root, Vec::new());
}

#[then(expr = "the loop's test command is {string}")]
fn loop_test_command(world: &mut VardeWorld, expected: String) {
    assert_eq!(
        world
            .state
            .refactor
            .running
            .as_ref()
            .map(|iteration| iteration.test_command.clone()),
        Some(expected)
    );
}

#[given(expr = "I start the Refactor loop over the scope {string}")]
#[when(expr = "I start the Refactor loop over the scope {string}")]
fn start_the_loop(world: &mut VardeWorld, name: String) {
    world.send(Event::StartRefactorLoop(scope(&name)));
}

/// A loop already on that Iteration is left where it is — restarting it would
/// throw away the pass a scenario declared — and a loop behind it is driven
/// forward through the Gate, because passing it is the only way an Iteration
/// after the first exists.
#[given(expr = "the Refactor loop is on Iteration {int} over the scope {string}")]
fn loop_is_on_iteration(world: &mut VardeWorld, number: u32, name: String) {
    let on = |world: &VardeWorld| {
        world
            .state
            .refactor
            .running
            .as_ref()
            .map(|iteration| iteration.number)
    };
    if on(world).is_none() {
        // A Scope with nothing measured and nothing on the way refuses the
        // loop, so a scenario declaring one already running gets the analysis
        // behind it too: a workspace measured and found empty is the emptiest
        // baseline the Gate can still judge. A review-scoped loop needs none
        // here — the view asked for its analysis on the way in, and that answer
        // is the baseline.
        if scope(&name) == Scope::Workspace
            && world.state.risk.figures().is_none()
            && !world.state.risk.in_flight()
        {
            deliver(world, Scope::Workspace, Figures::default(), None);
        }
        start_the_loop(world, name.clone());
    }
    while on(world).is_some_and(|running| running < number) {
        pass_the_gate(world, scope(&name));
    }
    assert_eq!(
        on(world),
        Some(number),
        "the loop is not on Iteration {number}"
    );
}

#[then(expr = "the Refactor loop is running")]
fn loop_is_running(world: &mut VardeWorld) {
    assert!(world.state.refactor.running.is_some());
}

#[then(expr = "{int} Refactor loop is running")]
fn this_many_loops_running(world: &mut VardeWorld, count: usize) {
    assert_eq!(world.state.refactor.running.iter().count(), count);
}

#[then(expr = "no Refactor loop is running")]
#[then(expr = "the Refactor loop is not running")]
fn loop_is_not_running(world: &mut VardeWorld) {
    assert_eq!(world.state.refactor.running, None);
}

#[then(expr = "the Refactor loop refusal is {string}")]
fn loop_refusal(world: &mut VardeWorld, expected: String) {
    assert_eq!(
        world.state.refactor.refusal,
        Some(expected.as_str()),
        "notices: {:?}",
        world.notices
    );
}

#[then(expr = "the Refactor loop wait state is {string}")]
fn loop_wait_state(world: &mut VardeWorld, expected: String) {
    assert_eq!(
        world
            .state
            .refactor
            .running
            .as_ref()
            .map(|iteration| iteration.wait.as_str()),
        Some(expected.as_str())
    );
}

/// The numbers, not the wording: `risk`'s own unit test pins the exact caption
/// the border draws. What this holds is that the pane says both of them at all
/// — an Iteration without its cap says nothing about how much of the run is
/// left, and a pane that says neither is the hang a caption exists to rule out.
#[then(expr = "the Refactor loop status shows Iteration {int} of {int}")]
fn loop_status_shows(world: &mut VardeWorld, number: u32, cap: u32) {
    assert_eq!(
        world
            .state
            .refactor
            .running
            .as_ref()
            .map(|iteration| iteration.number),
        Some(number)
    );
    assert_eq!(world.state.max_iterations, cap);
    let status = risk::status(&world.state).expect("the pane says nothing about the loop");
    assert!(
        status.contains(&number.to_string()) && status.contains(&cap.to_string()),
        "{status}"
    );
}

/// The figure the pane draws is the one this Iteration is judged against: the
/// list reads `risk.figures()`, and the Iteration carries the same figures as
/// its baseline. Drifting apart is a pane showing the previous pass's numbers
/// while the Gate measures against these.
#[then(expr = "the Risk list shows the figure for Iteration {int}")]
fn risk_list_shows_iteration_figure(world: &mut VardeWorld, number: u32) {
    let iteration = world
        .state
        .refactor
        .running
        .clone()
        .expect("no Refactor loop is running");
    assert_eq!(iteration.number, number);
    assert_eq!(world.state.risk.figures(), iteration.before.as_ref());
}

#[then("the Risk list pane actions offered are:")]
fn risk_pane_actions_offered(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].trim().to_string())
        .collect();
    let actual: Vec<String> = risk::pane_actions(&world.state)
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(actual, expected);
}

/// The click and the pane's own key are the one gesture, so this raises the
/// event both reach — the action is looked up in the list the pane draws
/// rather than spelled again here.
#[when(expr = "I click the {string} action on the Risk list pane")]
fn click_risk_pane_action(world: &mut VardeWorld, action: String) {
    let named = risk::pane_actions(&world.state)
        .into_iter()
        .find(|offered| *offered == action)
        .unwrap_or_else(|| panic!("the pane does not offer {action:?}"));
    world.send(Event::PaneAction(named));
}

#[when(expr = "I stop the Refactor loop")]
fn stop_the_loop(world: &mut VardeWorld) {
    world.send(Event::StopRefactorLoop);
}

/// A Given as well as a Then: the recompute's scenario has to say that there was
/// a verdict there to clear, and a precondition nothing checks is what makes the
/// assertion after it pass vacuously.
#[given(expr = "the Refactor loop stopped because {string}")]
#[then(expr = "the Refactor loop stopped because {string}")]
fn loop_stopped_because(world: &mut VardeWorld, expected: String) {
    assert_eq!(world.state.refactor.stopped, Some(expected.as_str()));
}

#[then(expr = "the Refactor loop last test result is {string}")]
fn loop_last_test_result(world: &mut VardeWorld, expected: String) {
    assert_eq!(world.state.refactor.last_test(), Some(expected.as_str()));
}

#[then(expr = "no Refactor loop last test result is reported")]
fn no_loop_last_test_result(world: &mut VardeWorld) {
    assert_eq!(world.state.refactor.last_test(), None);
}

/// What the border trails after the figure, which is what a finished run leaves
/// behind: `None` is the border saying nothing rather than saying "idle".
#[then(expr = "the Risk list border says nothing about the loop")]
fn border_says_nothing_about_the_loop(world: &mut VardeWorld) {
    assert_eq!(risk::status(&world.state), None);
}

#[then("the Refactor loop reports the tests' output:")]
fn loop_reports_output(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim();
    let reported = world
        .state
        .refactor
        .tests
        .as_ref()
        .map(|(_, output)| output.trim().to_string());
    assert_eq!(reported.as_deref(), Some(expected));
}

#[then(expr = "the sentinel {string} was deleted")]
fn sentinel_was_deleted(world: &mut VardeWorld, path: String) {
    let path = abs(world, &path);
    assert!(
        world.deleted.contains(&path),
        "deleted: {:?}",
        world.deleted
    );
}

#[given(expr = "the sentinel {string} already exists")]
fn sentinel_already_exists(world: &mut VardeWorld, path: String) {
    let path = abs(world, &path);
    world.files.insert(path, String::new());
}

/// The watcher that already exists is what sees it — the same event a file
/// landing anywhere else in the workspace arrives as.
#[when(expr = "the sentinel {string} appears")]
fn sentinel_appears(world: &mut VardeWorld, path: String) {
    let path = abs(world, &path);
    world.files.insert(path.clone(), String::new());
    world.send(Event::FilesAppeared(vec![(path, tree::Kind::File)]));
}

/// Time passing, and nothing else: the wait has no timeout, so a scenario about
/// silence hands the core exactly what the edge hands it while a job runs — the
/// ticks a spinner turns on and no event at all besides.
#[when(expr = "the clock advances {int} minutes")]
fn clock_advances(world: &mut VardeWorld, minutes: u64) {
    for _ in 0..minutes {
        world.send(Event::Tick);
    }
}

#[then(expr = "the test command {string} was run")]
fn test_command_was_run(world: &mut VardeWorld, expected: String) {
    assert_eq!(world.tests_run, vec![expected]);
}

#[then(expr = "no test command was run")]
fn no_test_command_was_run(world: &mut VardeWorld) {
    assert!(world.tests_run.is_empty(), "{:?}", world.tests_run);
}

#[then(expr = "a snapshot was taken for Iteration {int}")]
fn snapshot_was_taken(world: &mut VardeWorld, iteration: u32) {
    assert!(
        world.snapshots.contains_key(&iteration),
        "snapshots: {:?}",
        world.snapshots.keys().collect::<Vec<_>>()
    );
}

/// What the session did with the pass: the files it edited, which is what a
/// revert is measured against. Their contents change; nothing else does.
#[given("the Iteration touched:")]
fn iteration_touched(world: &mut VardeWorld, step: &Step) {
    for row in &step.table().expect("table").rows {
        let file = row[0].clone();
        let edited = format!(
            "{}\nthe session's edit\n",
            world.tree().get(&file).cloned().unwrap_or_default()
        );
        world.write_tree(&file, &edited);
    }
}

/// A file the user had already edited when the loop started: what the last
/// commit holds, and what their own uncommitted work left in it.
#[given(expr = "{string} had uncommitted changes before the loop started")]
fn had_uncommitted_changes(world: &mut VardeWorld, file: String) {
    let committed = world.tree().get(&file).cloned().unwrap_or_default();
    let mine = format!("{committed}my own uncommitted edit\n");
    world.committed.insert(file.clone(), committed);
    world.mine.insert(file.clone(), mine.clone());
    world.write_tree(&file, &mine);
    // Declared as of *before* the loop, so any snapshot already taken holds it
    // too — otherwise the world would show the loop having changed a file the
    // user changed, and every revert would restore it.
    for snapshot in world.snapshots.values_mut() {
        if let Some(held) = snapshot.get_mut(&file) {
            *held = mine.clone();
        }
    }
}

#[given("the tests finish failing:")]
#[when("the tests finish failing:")]
fn tests_finish_failing(world: &mut VardeWorld, step: &Step) {
    let output = step.docstring().expect("docstring").trim().to_string();
    world.send(Event::TestsFinished {
        passed: false,
        output,
    });
}

#[given("the tests finished passing")]
#[when("the tests finish passing")]
fn tests_finish_passing(world: &mut VardeWorld) {
    world.send(Event::TestsFinished {
        passed: true,
        output: "ok".to_string(),
    });
}

#[then(expr = "Iteration {int} was restored from its snapshot")]
fn iteration_was_restored(world: &mut VardeWorld, iteration: u32) {
    assert!(
        world.restores.contains(&iteration),
        "restores: {:?}",
        world.restores
    );
}

#[then(expr = "Iteration {int} was not restored from its snapshot")]
fn iteration_was_not_restored(world: &mut VardeWorld, iteration: u32) {
    assert!(
        !world.restores.contains(&iteration),
        "restores: {:?}",
        world.restores
    );
}

#[then("the files restored are:")]
fn files_restored_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(world.restored, expected);
}

#[then(expr = "{string} was not restored")]
fn was_not_restored(world: &mut VardeWorld, file: String) {
    assert!(!world.restored.contains(&file), "{:?}", world.restored);
}

/// What the loop's whole run changed, measured against the tree as it stood
/// when the first Iteration was snapshotted — files the run created included, so
/// a session that wrote outside its Scope is caught rather than missed.
#[then("the files the loop changed are:")]
fn files_the_loop_changed(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(loop_changed(world), expected);
}

/// The absence the review-scoped loop is worth having for: a file nobody is
/// reviewing is a file the loop may not touch, and the user's own uncommitted
/// work in it is still there afterwards.
#[then(expr = "{string} is unchanged by the loop")]
fn unchanged_by_the_loop(world: &mut VardeWorld, file: String) {
    assert!(
        !loop_changed(world).contains(&file),
        "the loop changed {file}"
    );
}

fn loop_changed(world: &VardeWorld) -> Vec<String> {
    let before = world.snapshots.get(&1).expect("a snapshot for Iteration 1");
    world
        .tree()
        .into_iter()
        .filter(|(file, now)| before.get(file) != Some(now))
        .map(|(file, _)| file)
        .collect()
}

#[then(expr = "{string} holds the uncommitted changes it held before the loop started")]
fn holds_my_uncommitted_changes(world: &mut VardeWorld, file: String) {
    let mine = world.mine.get(&file).expect("a file the scenario edited");
    assert_eq!(world.tree().get(&file), Some(mine));
}

#[then(expr = "{string} was not restored from the last commit")]
fn not_restored_from_the_commit(world: &mut VardeWorld, file: String) {
    let committed = world.committed.get(&file).expect("a committed side");
    assert_ne!(world.tree().get(&file), Some(committed));
}

/// The loop's whole output is a dirty working tree: nothing it does may reach
/// history, per Iteration or at the end. Both channels a commit could come
/// down, since neither is allowed to carry one.
#[then(expr = "nothing was committed")]
fn nothing_was_committed(world: &mut VardeWorld) {
    for command in world.executed.iter().chain(world.tests_run.iter()) {
        assert!(!command.contains("commit"), "committed: {command:?}");
    }
}

/// Any of the prompts, not only the first: an Iteration's own prompt went
/// before whatever the Gate has to explain afterwards.
fn any_prompt_contains(world: &VardeWorld, needle: &str) {
    let sent = ai_sends(world);
    assert!(
        sent.iter().any(|prompt| prompt.contains(needle)),
        "no prompt holds {needle:?}:\n{}",
        sent.join("\n---\n")
    );
}

#[then(expr = "the AI pane was sent a prompt naming the scope {string}")]
fn prompt_names_the_scope(world: &mut VardeWorld, name: String) {
    any_prompt_contains(world, scope(&name).as_str());
}

#[then(expr = "the AI pane was sent a prompt naming the target threshold {string}")]
fn prompt_names_the_target(world: &mut VardeWorld, threshold: String) {
    any_prompt_contains(world, &threshold);
}

#[then(
    expr = "the AI pane was sent a prompt instructing the session to follow the repo's convention files"
)]
fn prompt_names_convention_files(world: &mut VardeWorld) {
    any_prompt_contains(world, "convention files this repository holds");
}

/// The figure is the symptom, and a pass that only moved it is a pass R29.11's
/// third condition reverts — but a revert costs an Iteration, and the row
/// action has no Gate behind it at all, so the wording has to say it up front.
#[then(expr = "the AI pane was sent a prompt asking for splits that stand on their own")]
fn prompt_asks_for_meaningful_splits(world: &mut VardeWorld) {
    any_prompt_contains(world, "the symptom, not the goal");
}

/// The other direction: naming what a good split is only constrains a session
/// if the prompt also names the shape that fails, so both halves are pinned.
#[then(expr = "the AI pane was sent a prompt naming structural scattering as a failed pass")]
fn prompt_names_scattering(world: &mut VardeWorld) {
    any_prompt_contains(world, "structural scattering");
}

#[then(expr = "the AI pane was sent a prompt naming the Gate condition {string}")]
fn prompt_names_the_condition(world: &mut VardeWorld, condition: String) {
    any_prompt_contains(world, &condition);
}

#[then("the AI pane was sent a prompt containing the tests' output")]
fn prompt_contains_the_output(world: &mut VardeWorld) {
    let output = world
        .state
        .refactor
        .tests
        .as_ref()
        .map(|(_, output)| output.clone())
        .expect("a test run");
    any_prompt_contains(world, output.trim());
}

/// The most project-specific string in the system, kept out of a prompt that
/// has to work on any workspace — because Varde runs the tests itself.
#[then(expr = "no prompt sent to the AI contains {string}")]
fn no_prompt_contains(world: &mut VardeWorld, needle: String) {
    for prompt in ai_sends(world) {
        assert!(
            !prompt.contains(&needle),
            "prompt holds {needle:?}:\n{prompt}"
        );
    }
}

// ---- F29: the Gate on the metrics ----

/// Passing is not a flag the loop keeps: it is the Iteration standing — never
/// restored — and the loop having moved on from it, either to the next
/// Iteration or to a stop at the cap. Nothing else produces that pair.
#[then(expr = "Iteration {int} passed the Gate")]
fn iteration_passed_the_gate(world: &mut VardeWorld, iteration: u32) {
    iteration_was_not_restored(world, iteration);
    let moved_on = match world.state.refactor.running.as_ref() {
        Some(running) => running.number == iteration + 1,
        None => world.state.refactor.stopped == Some(risk::CAP_REACHED),
    };
    assert!(
        moved_on,
        "the loop did not move on from Iteration {iteration}: {:?}, stopped {:?}",
        world.state.refactor.running, world.state.refactor.stopped
    );
}

/// One whole Iteration through the Gate — the sentinel, the tests, figures the
/// Gate accepts — so a scenario standing on a passed Gate stands on the path
/// the Gate itself takes rather than on a state nobody could reach.
///
/// The figure it delivers halves the worst Function, which is what a real pass
/// looks like: the count falls by one and every other metric with it, so the
/// Gate's three conditions are satisfied by the same answer.
fn pass_the_gate(world: &mut VardeWorld, scope: Scope) {
    measured_baseline(world);
    session_edits(world);
    sentinel_appears(world, format!("{}/{}", varde::VARDE_DIR, risk::SENTINEL));
    tests_finish_passing(world);
    let mut functions = world
        .state
        .refactor
        .running
        .as_ref()
        .map(|iteration| {
            iteration
                .before
                .clone()
                .expect("the Iteration was judged against nothing")
                .functions
        })
        .expect("an Iteration in flight");
    if let Some(worst) = functions
        .iter_mut()
        .max_by_key(|function| function.metrics.cyclomatic)
    {
        worst.metrics = Metrics {
            cyclomatic: worst.metrics.cyclomatic / 2,
            cognitive: worst.metrics.cognitive / 2,
            maintainability: worst.metrics.maintainability / 2,
            lines: worst.metrics.lines / 2,
        };
    }
    deliver(
        world,
        scope,
        Figures {
            functions,
            unparsed: 0,
        },
        // The base revision the last answer measured, carried through: a
        // review-scoped answer has both sides in it, and a workspace one has
        // neither.
        world.state.risk.before.clone(),
    );
}

/// The figure the Gate will judge the pass against, where the loop began before
/// one had been measured — which is every review-scoped loop, since the view
/// asks for its analysis on the way in and the loop starts while it runs. This
/// is that analysis answering: one Function per reviewed file, above the
/// threshold, both sides alike because the change itself is not what the loop is
/// being judged on.
///
/// Invented by the world and never asserted on: every figure a scenario reads is
/// one a step named. What it stands in for is the answer the edge would deliver.
fn measured_baseline(world: &mut VardeWorld) {
    let waiting = world
        .state
        .refactor
        .running
        .as_ref()
        .is_some_and(|iteration| iteration.before.is_none());
    if !waiting {
        return;
    }
    let figures = Figures {
        functions: review::list(&world.state)
            .iter()
            .map(|file| Function {
                file: file.clone(),
                name: "under_review".to_string(),
                line: 1,
                metrics: Metrics {
                    // Above whatever threshold the scenario set, and low enough
                    // that halving it lands under: a fixture pinned to a number
                    // would stop being a Risk the moment a scenario moved the
                    // threshold, and the Gate would read the pass as a plateau.
                    cyclomatic: world.state.risk_threshold + 10,
                    cognitive: world.state.risk_threshold + 4,
                    ..Metrics::default()
                },
            })
            .collect(),
        unparsed: 0,
    };
    deliver(world, Scope::Review, figures.clone(), Some(figures));
}

/// The session's own pass, as a session obeying the prompt it was handed would
/// make it: every file under review the prompt named is edited, and nothing
/// else. Varde cannot stop a session touching a file it was not given, so the
/// prompt is the whole of what keeps a review-scoped loop inside its Scope —
/// which is what makes "the loop changed these files and no others" an
/// assertion about the prompt rather than about this glue.
fn session_edits(world: &mut VardeWorld) {
    let prompt = ai_sends(world).last().cloned().unwrap_or_default();
    let named: Vec<String> = review::list(&world.state)
        .into_iter()
        .filter(|file| prompt.contains(file))
        .collect();
    for file in named {
        let edited = format!(
            "{}\nthe session's edit\n",
            world.tree().get(&file).cloned().unwrap_or_default()
        );
        world.write_tree(&file, &edited);
    }
}

#[given(expr = "{int} Iteration has passed the Gate over the scope {string}")]
#[given(expr = "Iteration {int} passed the Gate over the scope {string}")]
#[when(expr = "Iteration {int} passes the Gate over the scope {string}")]
fn an_iteration_has_passed_the_gate(world: &mut VardeWorld, number: u32, name: String) {
    loop_is_on_iteration(world, number, name.clone());
    pass_the_gate(world, scope(&name));
    iteration_passed_the_gate(world, number);
}

#[then(expr = "the Refactor loop is on Iteration {int}")]
fn loop_is_now_on_iteration(world: &mut VardeWorld, number: u32) {
    assert_eq!(
        world
            .state
            .refactor
            .running
            .as_ref()
            .map(|iteration| iteration.number),
        Some(number)
    );
}

/// The figures arrived and nothing has been decided on them yet: no verdict, no
/// revert, and no Iteration counted as passed.
#[then("the Gate has not been evaluated")]
fn the_gate_has_not_been_evaluated(world: &mut VardeWorld) {
    assert_eq!(world.state.refactor.stopped, None);
    assert!(world.restores.is_empty(), "restores: {:?}", world.restores);
    // Still on the Iteration whose figures are being measured: an accepted one
    // would have moved the loop on, and a reverted one would have ended it.
    assert_eq!(
        world
            .state
            .refactor
            .running
            .as_ref()
            .map(|iteration| iteration.wait.as_str()),
        Some("waiting-for-figures")
    );
}

/// Every file, not only the ones the snapshot holds: "leaves nothing behind"
/// is a statement about the whole tree, so a file the Iteration created would
/// fail this even though the restore walks the snapshot.
#[then(expr = "the working tree holds no change from Iteration {int}")]
fn tree_holds_no_change(world: &mut VardeWorld, iteration: u32) {
    let snapshot = world
        .snapshots
        .get(&iteration)
        .cloned()
        .expect("a snapshot for the Iteration");
    assert_eq!(world.tree(), snapshot);
}

/// The row action is one prompt and no Iteration: nothing to snapshot, because
/// there is no Gate behind it and so nothing that could revert.
#[then(expr = "no snapshot was taken")]
fn no_snapshot_was_taken(world: &mut VardeWorld) {
    assert!(
        world.snapshots.is_empty(),
        "snapshots: {:?}",
        world.snapshots.keys().collect::<Vec<_>>()
    );
}

// ---- F30: Risk in Review view ----

/// The revision the diff on screen is measured from: `HEAD`, which the edge
/// tells the core on the same poll as the git status. The step sets it the way
/// the edge does, so nothing about the delta's base is arranged behind the
/// diff's back.
#[given(expr = "the review diff is measured from {string}")]
fn review_diff_measured_from(world: &mut VardeWorld, revision: String) {
    world.head = Some(revision.clone());
    world.startup.head = Some(revision);
    world.tell_core();
}

#[given(expr = "{string} is modified")]
fn file_is_modified(world: &mut VardeWorld, path: String) {
    let mut files = world.state.repo.clone().unwrap_or_default();
    files.push(GitFile {
        path,
        status: GitStatus::Modified,
    });
    world.state.repo = Some(files);
}

fn last_analysis(world: &VardeWorld) -> &Analysis {
    world.analyses.last().expect("an analysis was asked for")
}

/// Exactly those files and no others: a figure over a mix of the reviewed files
/// and the workspace's is a figure nobody can act on, so the request is held to
/// naming the Scope and nothing beside it.
#[then(expr = "the analysis covers exactly:")]
fn analysis_covers_exactly(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let covered = last_analysis(world)
        .files
        .clone()
        .expect("the request names the workspace, not a file set");
    assert_eq!(covered, expected);
}

#[then(expr = "the review risk base revision is {string}")]
fn review_risk_base_is(world: &mut VardeWorld, expected: String) {
    assert_eq!(
        last_analysis(world).base.as_deref(),
        Some(expected.as_str())
    );
}

/// The delta reuses the diff's base rather than choosing one of its own: a
/// figure that disagrees with the diff beside it is worse than no figure. Held
/// against `review::base` — what Review view measures its diff from — so the two
/// cannot come to be measured from two different places.
#[then(expr = "the review risk base revision is the revision the review diff is measured from")]
fn review_risk_base_is_the_diffs(world: &mut VardeWorld) {
    assert_eq!(
        last_analysis(world).base.as_deref(),
        review::base(&world.state),
        "the delta chose a base of its own"
    );
}

/// The answer for a review-scoped analysis: both sides measured in one job, the
/// `was …` columns being the same Functions at the base revision.
#[when(expr = "the review figures arrive:")]
fn review_figures_arrive(world: &mut VardeWorld, step: &Step) {
    let before = figures_from(step, 0, "was ");
    deliver(world, Scope::Review, figures(step, 0), Some(before));
}

fn review_delta(world: &VardeWorld) -> (i64, bool) {
    match risk::shown(&world.state) {
        Some(risk::Shown::Delta { delta, worse }) => (delta, worse),
        other => panic!("the border is not showing a delta: {other:?}"),
    }
}

#[then(expr = "the tree border shows the review delta")]
fn border_shows_review_delta(world: &mut VardeWorld) {
    review_delta(world);
}

#[then(expr = "the tree border shows the workspace risk count")]
fn border_shows_workspace_count(world: &mut VardeWorld) {
    match risk::shown(&world.state) {
        Some(risk::Shown::Count(_)) => {}
        other => panic!("the border is not showing the workspace count: {other:?}"),
    }
}

#[then(expr = "the review risk delta is {int}")]
fn review_risk_delta_is(world: &mut VardeWorld, expected: i64) {
    assert_eq!(review_delta(world).0, expected);
}

#[then(expr = "the review risk is marked worse")]
fn review_risk_marked_worse(world: &mut VardeWorld) {
    assert!(review_delta(world).1, "the change is not marked worse");
    let drawn = risk::border(&world.state).expect("a border");
    assert!(
        drawn.contains("worse"),
        "the border does not say so: {drawn}"
    );
}

#[then(expr = "the review risk is not marked worse")]
fn review_risk_not_marked_worse(world: &mut VardeWorld) {
    assert!(!review_delta(world).1, "the change is marked worse");
    let drawn = risk::border(&world.state).expect("a border");
    assert!(
        !drawn.contains("worse"),
        "the border says so anyway: {drawn}"
    );
}

/// A file in no language the analyser handles contributes nothing — and nothing
/// is not an improvement: a fabricated zero for it would read as a clean bill of
/// health nobody was given.
#[then(expr = "{string} contributes no figure")]
fn contributes_no_figure(world: &mut VardeWorld, path: String) {
    let named: Vec<&String> = world
        .state
        .risk
        .figures()
        .expect("a figure")
        .functions
        .iter()
        .map(|function| &function.file)
        .filter(|file| **file == path)
        .collect();
    assert!(named.is_empty(), "{path} is in the figure");
}

// ---- F31: the transport — a server starts, is initialized, and is told about
// the buffer. No scenario runs one: the world plays the edge, and everything a
// server says arrives as canned JSON.

/// Varde started on the template alone, whose rows say which files each
/// language claims: a server or formatter a scenario configures "for rust"
/// serves what the shipped rust row serves, and a language nothing ships
/// claims nothing.
fn shipped() -> State {
    startup::start(&Startup::default())
        .expect("the defaults start")
        .0
}

#[given(expr = "a language server {string} is configured for {string}")]
fn server_configured(world: &mut VardeWorld, command: String, language: String) {
    world.state.servers.insert(
        language.clone(),
        Server {
            command,
            args: Vec::new(),
            also_served_by: Vec::new(),
            extensions: shipped()
                .servers
                .get(&language)
                .map(|server| server.extensions.clone())
                .unwrap_or_default(),
            install: BTreeMap::new(),
            initialization_options: None,
            partial: None,
            unanswerable: None,
        },
    );
}

/// A server that answers its handshake the moment it is asked. The command is
/// arbitrary — what the Scenarios below turn on is that the conversation
/// reaches ready, not what is on `PATH`.
#[given(expr = "a language server for {string} is ready")]
fn server_is_ready(world: &mut VardeWorld, language: String) {
    world.lsp_answers.insert(language.clone());
    world
        .state
        .servers
        .entry(language.clone())
        .or_insert(Server {
            command: format!("{language}-language-server"),
            args: Vec::new(),
            also_served_by: Vec::new(),
            extensions: shipped()
                .servers
                .get(&language)
                .map(|server| server.extensions.clone())
                .unwrap_or_default(),
            install: BTreeMap::new(),
            initialization_options: None,
            partial: None,
            unanswerable: None,
        });
}

/// A server the edge was already holding when the Scenario began — started for
/// some buffer in a session that is not what this Scenario is about. Distinct
/// from "is ready" above, which arms a canned server and leaves the spawn to
/// whatever opens a file: a Scenario that opens no file needs the process to
/// exist without one, and needs "no language server was started" to stay a
/// statement about the whole run.
#[given(expr = "a language server for {string} is already running")]
fn server_is_already_running(world: &mut VardeWorld, language: String) {
    let command = format!("{language}-language-server");
    world.lsp_answers.insert(language.clone());
    world
        .state
        .servers
        .entry(language.clone())
        .or_insert(Server {
            command: command.clone(),
            args: Vec::new(),
            also_served_by: Vec::new(),
            extensions: shipped()
                .servers
                .get(&language)
                .map(|server| server.extensions.clone())
                .unwrap_or_default(),
            install: BTreeMap::new(),
            initialization_options: None,
            partial: None,
            unanswerable: None,
        });
    world.lsp_is_running(&language, &command);
}

/// Which other servers also serve this language's files, as configuration says
/// it. Data rather than a branch: what makes a `.vue` file two servers' business
/// is a row in a table, which is what lets a Scenario name two of them at all
/// (R31.1, ADR 0011).
#[given(expr = "{string} files are also served by the language server for {string}")]
fn files_also_served_by(world: &mut VardeWorld, language: String, also: String) {
    world
        .state
        .servers
        .get_mut(&language)
        .unwrap_or_else(|| panic!("no server configured for {language}"))
        .also_served_by
        .push(also);
}

/// A server whose configuration names a request it puts to its client and Varde
/// will not answer — so it has, by construction, questions it cannot answer on
/// its own. What the Scenario turns on is the *notice*: reporting that server as
/// knowing nothing is Varde blaming somebody else for its own refusal.
#[given(
    expr = "the language server for {string} relays {string} to a companion Varde does not run"
)]
fn server_relays_to_a_companion(world: &mut VardeWorld, language: String, request: String) {
    world
        .state
        .servers
        .get_mut(&language)
        .unwrap_or_else(|| panic!("no server configured for {language}"))
        .unanswerable = Some(Unanswerable {
        request,
        // A don't-care for every Scenario that names this step: what the notice
        // turns on is that the question was put and refused, never the method
        // the refusal went back on. The refusal's own shape is pinned by the
        // Scenarios under the Rule that owns it.
        response: "tsserver/response".to_string(),
    });
}

/// Nothing holds a server for that language — true of every language until
/// something starts one, and said out loud because it is the precondition the
/// Scenario turns on rather than an accident of the Background.
#[given(expr = "there is no language server running for {string}")]
fn no_server_running_for(world: &mut VardeWorld, language: String) {
    world.lsp_running.remove(&language);
    world.tell_core();
}

#[given(expr = "there is no language server configured for {string}")]
fn no_server_for(world: &mut VardeWorld, language: String) {
    world.state.servers.remove(&language);
}

#[given(expr = "the language server for {string} fails to start")]
#[when(expr = "the language server for {string} fails to start")]
fn server_fails_to_start(world: &mut VardeWorld, language: String) {
    world.lsp_fails.insert(language.clone());
    world.lsp_is_gone(&language, Gone::FailedToStart);
}

#[given(expr = "the language server for {string} exits")]
#[when(expr = "the language server for {string} exits")]
fn server_exits(world: &mut VardeWorld, language: String) {
    world.lsp_is_gone(&language, Gone::Exited);
}

#[given(expr = "the language server for {string} replies to {string} with:")]
#[when(expr = "the language server for {string} replies to {string} with:")]
fn server_replies(world: &mut VardeWorld, language: String, method: String, step: &Step) {
    let result: Value = serde_json::from_str(step.docstring().expect("docstring")).expect("json");
    let id = sent(world, &language)
        .iter()
        .find(|message| message["method"] == method)
        .unwrap_or_else(|| panic!("{language} was never sent a {method} request"))["id"]
        .clone();
    world.lsp_replies(
        &language,
        json!({"jsonrpc": "2.0", "id": id, "result": result}),
    );
}

#[when(expr = "the language server for {string} answers {string} with the error {string}")]
fn server_answers_error(world: &mut VardeWorld, language: String, method: String, why: String) {
    let id = sent(world, &language)
        .iter()
        .rfind(|message| message["method"] == method)
        .unwrap_or_else(|| panic!("{language} was never sent a {method} request"))["id"]
        .clone();
    world.lsp_replies(
        &language,
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": why}}),
    );
}

#[then(expr = "the language server for {string} was sent {int} {string} requests")]
fn server_sent_count(world: &mut VardeWorld, language: String, count: usize, method: String) {
    let messages = sent(world, &language);
    assert_eq!(
        messages
            .iter()
            .filter(|message| message["method"] == method)
            .count(),
        count
    );
}

/// A question the server puts to Varde on a method of its own — a notification,
/// so the protocol has no reply for it and the server is left waiting unless
/// Varde says something back on the method its configuration names.
#[given(expr = "the language server for {string} asks {string} with:")]
#[when(expr = "the language server for {string} asks {string} with:")]
fn server_asks(world: &mut VardeWorld, language: String, method: String, step: &Step) {
    let params: Value = serde_json::from_str(step.docstring().expect("docstring")).expect("json");
    world.lsp_replies(
        &language,
        json!({"jsonrpc": "2.0", "method": method, "params": params}),
    );
}

#[then(expr = "the language server for {string} was sent {string} with:")]
fn server_was_sent_with(world: &mut VardeWorld, language: String, method: String, step: &Step) {
    let expected: Value = serde_json::from_str(step.docstring().expect("docstring")).expect("json");
    let messages = sent(world, &language);
    let said = messages
        .iter()
        .find(|message| message["method"] == method)
        .unwrap_or_else(|| {
            panic!(
                "{language} was never sent {method}; it was sent: {:?}",
                methods(&messages)
            )
        });
    assert_eq!(said["params"], expected);
}

/// Every message that language's server was sent, in order.
fn sent(world: &VardeWorld, language: &str) -> Vec<Value> {
    world
        .lsp_sent
        .iter()
        .filter(|(named, _)| named == language)
        .map(|(_, message)| message.clone())
        .collect()
}

/// The documents that language's server was told about: the method, which file,
/// what version and what the file held. One shape for `didOpen` and `didChange`
/// because every assertion below is about the pair, not about which notification
/// carried it.
fn documents(world: &VardeWorld, language: &str) -> Vec<(String, PathBuf, i64, String)> {
    sent(world, language)
        .iter()
        .filter_map(|message| {
            let method = message["method"].as_str()?.to_string();
            let document = &message["params"]["textDocument"];
            let uri = document["uri"].as_str()?;
            let text = match method.as_str() {
                "textDocument/didOpen" => document["text"].as_str()?.to_string(),
                "textDocument/didChange" => message["params"]["contentChanges"][0]["text"]
                    .as_str()?
                    .to_string(),
                _ => return None,
            };
            Some((
                method,
                PathBuf::from(uri.trim_start_matches("file://")),
                document["version"].as_i64()?,
                text,
            ))
        })
        .collect()
}

/// The documents a server was told about for one file, in the order it was told.
fn told_about(world: &VardeWorld, language: &str, path: &str) -> Vec<(String, i64, String)> {
    let absolute = abs(world, path);
    documents(world, language)
        .into_iter()
        .filter(|(_, told, _, _)| *told == absolute)
        .map(|(method, _, version, text)| (method, version, text))
        .collect()
}

#[then(expr = "a language server was started with {string}")]
fn server_was_started_with(world: &mut VardeWorld, command: String) {
    assert!(
        world
            .lsp_started
            .iter()
            .any(|(_, started)| *started == command),
        "started: {:?}",
        world.lsp_started
    );
}

#[then(expr = "the language server for {string} was started with arguments:")]
fn server_started_with_arguments(world: &mut VardeWorld, language: String, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(
        world.lsp_args.get(&language),
        Some(&expected),
        "spawns: {:?}",
        world.lsp_args
    );
}

#[then(expr = "{int} language server was started for {string}")]
fn servers_started_for(world: &mut VardeWorld, count: usize, language: String) {
    let started: Vec<&(String, String)> = world
        .lsp_started
        .iter()
        .filter(|(named, _)| *named == language)
        .collect();
    assert_eq!(started.len(), count, "started: {started:?}");
}

/// Sent, and — for `initialized` — sent *before anything about a document*,
/// which is what the Scenario's name promises. A membership check alone would
/// pass with the handshake closed out after the first `didOpen`.
#[then(expr = "the language server for {string} was sent a {string} request")]
#[then(expr = "the language server for {string} was sent an {string} request")]
#[then(expr = "the language server for {string} was sent an {string} notification")]
fn server_was_sent(world: &mut VardeWorld, language: String, method: String) {
    let messages = sent(world, &language);
    let at = messages
        .iter()
        .position(|message| message["method"] == method)
        .unwrap_or_else(|| {
            panic!(
                "{language} was never sent {method}; it was sent: {:?}",
                methods(&messages)
            )
        });
    let documents = messages.iter().position(|message| {
        message["method"]
            .as_str()
            .is_some_and(|method| method.starts_with("textDocument/"))
    });
    if method == "initialized" {
        assert!(
            documents.is_none_or(|first| at < first),
            "the handshake was closed out after the documents went: {:?}",
            methods(&messages)
        );
    }
}

/// What the handshake carried in `initializationOptions`, which is the whole of
/// what a Scenario can say about a key Varde never reads: it went out verbatim.
fn initialization_options(world: &VardeWorld, language: &str) -> Value {
    let messages = sent(world, language);
    let handshake = messages
        .iter()
        .find(|message| message["method"] == "initialize")
        .unwrap_or_else(|| {
            panic!(
                "{language} was never sent initialize; it was sent: {:?}",
                methods(&messages)
            )
        });
    handshake["params"]["initializationOptions"].clone()
}

#[then(expr = "the initialize request for {string} carried initialization options:")]
fn initialize_carried_options(world: &mut VardeWorld, language: String, step: &Step) {
    let expected: Value =
        serde_json::from_str(step.docstring().expect("docstring")).expect("valid JSON");
    assert_eq!(initialization_options(world, &language), expected);
}

/// The one capability Varde claims. A server may hold its diagnostics back
/// from a client that never said it could receive them, which is silence a
/// reader cannot tell from a clean file.
#[then(expr = "the initialize request for {string} said Varde can be told diagnostics")]
fn initialize_declared_diagnostics(world: &mut VardeWorld, language: String) {
    let messages = sent(world, &language);
    let handshake = messages
        .iter()
        .find(|message| message["method"] == "initialize")
        .expect("initialize");
    assert!(
        handshake["params"]["capabilities"]["textDocument"]["publishDiagnostics"].is_object(),
        "{language} was told: {}",
        handshake["params"]["capabilities"]
    );
}

/// The other capability, and the one that is a promise: a server may only send
/// `${1:…}` to a client that said it could resolve it.
#[then(expr = "the initialize request for {string} said Varde can receive snippets")]
fn initialize_declared_snippets(world: &mut VardeWorld, language: String) {
    let messages = sent(world, &language);
    let handshake = messages
        .iter()
        .find(|message| message["method"] == "initialize")
        .expect("initialize");
    assert_eq!(
        handshake["params"]["capabilities"]["textDocument"]["completion"]["completionItem"]
            ["snippetSupport"],
        json!(true),
        "{language} was told: {}",
        handshake["params"]["capabilities"]
    );
}

/// The third, and the one this box could not honour until it rendered what it
/// was sent: a server may answer in either format, and most pick markdown.
#[then(expr = "the initialize request for {string} said Varde can read markdown")]
fn initialize_declared_markdown(world: &mut VardeWorld, language: String) {
    let messages = sent(world, &language);
    let handshake = messages
        .iter()
        .find(|message| message["method"] == "initialize")
        .expect("initialize");
    let formats = &handshake["params"]["capabilities"]["textDocument"]["hover"]["contentFormat"];
    assert!(
        formats
            .as_array()
            .is_some_and(|formats| formats.contains(&json!("markdown"))),
        "{language} was told: {}",
        handshake["params"]["capabilities"]
    );
}

/// Null, which is what the protocol has for a client that says nothing — and
/// what an absent field reads as here, since either spelling is the same
/// absence to the server.
#[then(expr = "the initialize request for {string} carried no initialization options")]
fn initialize_carried_no_options(world: &mut VardeWorld, language: String) {
    assert_eq!(initialization_options(world, &language), Value::Null);
}

#[then(expr = "the language server for {string} was sent no {string} request")]
#[then(expr = "the language server for {string} was sent no {string} notification")]
fn server_was_sent_nothing(world: &mut VardeWorld, language: String, method: String) {
    let messages = sent(world, &language);
    assert!(
        !messages.iter().any(|message| message["method"] == method),
        "{language} was sent: {:?}",
        methods(&messages)
    );
}

fn methods(messages: &[Value]) -> Vec<String> {
    messages
        .iter()
        .map(|message| message["method"].to_string())
        .collect()
}

#[then(expr = "the language server for {string} is ready")]
fn server_is_ready_now(world: &mut VardeWorld, language: String) {
    assert!(
        lsp::ready(&world.state, &language),
        "the handshake has not completed: {:?}",
        world.state.lsp.get(&language)
    );
}

#[then(expr = "the language server for {string} is not ready")]
fn server_is_not_ready(world: &mut VardeWorld, language: String) {
    assert!(
        !lsp::ready(&world.state, &language),
        "the handshake completed anyway"
    );
}

/// A reply to a request the Scenario names by id rather than by method — the
/// one shape that can express a reply nobody asked for.
#[when(expr = "the language server for {string} replies to request {int} with:")]
fn server_replies_to_id(world: &mut VardeWorld, language: String, id: i64, step: &Step) {
    let result: Value = serde_json::from_str(step.docstring().expect("docstring")).expect("json");
    world.lsp_replies(
        &language,
        json!({"jsonrpc": "2.0", "id": id, "result": result}),
    );
}

/// The reply to the hover Varde last asked for, under that request's own id —
/// which is what makes the correlation the thing under test rather than
/// something the step arranges around.
#[given(expr = "the language server for {string} answers the hover with:")]
#[when(expr = "the language server for {string} answers the hover with:")]
fn server_answers_the_hover(world: &mut VardeWorld, language: String, step: &Step) {
    let result: Value = serde_json::from_str(step.docstring().expect("docstring")).expect("json");
    world.lsp_replies(
        &language,
        json!({"jsonrpc": "2.0", "id": asked(world, &language, "textDocument/hover"), "result": result}),
    );
}

/// The reply to the definition Varde last asked for. The table is in the
/// editor's own 1-based coordinates, so the step converts to the protocol's
/// zero-based line and UTF-16 column rather than pinning the protocol's numbers
/// in the Gherkin — the same direction the hover request assertion converts in.
#[given(expr = "the language server for {string} answers the definition with:")]
#[when(expr = "the language server for {string} answers the definition with:")]
fn server_answers_the_definition(world: &mut VardeWorld, language: String, step: &Step) {
    let locations: Vec<Value> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let line: u32 = row[1].parse().expect("a line number");
            let column: u32 = row[2].parse().expect("a column");
            let path = match row[0].starts_with('/') {
                true => PathBuf::from(&row[0]),
                false => abs(world, &row[0]),
            };
            let at = json!({"line": line - 1, "character": column - 1});
            json!({
                "uri": format!("file://{}", path.display()),
                "range": {"start": at, "end": at},
            })
        })
        .collect();
    answer_definition(world, &language, Value::Array(locations));
}

/// The server saying it knows of nowhere: `null`, which is what the protocol
/// has for a symbol with no definition.
#[given(expr = "the language server for {string} answers the definition with nothing")]
#[when(expr = "the language server for {string} answers the definition with nothing")]
fn server_answers_the_definition_with_nothing(world: &mut VardeWorld, language: String) {
    answer_definition(world, &language, Value::Null);
}

fn answer_definition(world: &mut VardeWorld, language: &str, result: Value) {
    world.lsp_replies(
        language,
        json!({"jsonrpc": "2.0", "id": asked(world, language, "textDocument/definition"), "result": result}),
    );
}

/// The id of the last request of that method sent to that language's server.
fn asked(world: &VardeWorld, language: &str, method: &str) -> Value {
    sent(world, language)
        .iter()
        .rev()
        .find(|message| message["method"] == method)
        .unwrap_or_else(|| panic!("{language} was never sent a {method} request"))["id"]
        .clone()
}

/// Where a request asked about, in the editor's own 1-based coordinates. The
/// protocol counts lines from zero and columns in UTF-16 units, so the step
/// converts rather than pinning the protocol's numbers in the Gherkin.
#[then(
    expr = "the language server for {string} was sent a {string} request for {string} line {int} column {int}"
)]
fn was_sent_request_for(
    world: &mut VardeWorld,
    language: String,
    method: String,
    path: String,
    line: u64,
    column: u64,
) {
    let absolute = abs(world, &path);
    let asked: Vec<Value> = sent(world, &language)
        .into_iter()
        .filter(|message| message["method"] == method)
        .map(|message| message["params"].clone())
        .collect();
    let wanted = json!({
        "textDocument": {"uri": format!("file://{}", absolute.display())},
        "position": {"line": line - 1, "character": column - 1},
    });
    assert!(
        asked.iter().any(|params| params == &wanted),
        "{language} was asked {method}: {asked:?}"
    );
}

/// The absence a plain click promises: the question was never put at all.
#[then(expr = "the language server for {string} was never sent a {string} request")]
fn was_never_sent(world: &mut VardeWorld, language: String, method: String) {
    let messages = sent(world, &language);
    assert!(
        !messages.iter().any(|message| message["method"] == method),
        "{language} was asked {method}: {:?}",
        methods(&messages)
    );
}

#[then(expr = "no language server request is outstanding for {string}")]
fn nothing_outstanding(world: &mut VardeWorld, language: String) {
    assert_eq!(lsp::outstanding(&world.state, &language), 0);
}

/// A hover already on screen, arranged the way one arrives: the binding asks,
/// and the server answers the request it made.
#[given(expr = "a hover is shown")]
fn a_hover_is_shown(world: &mut VardeWorld) {
    press_in_editor(world, "K".to_string());
    let language = "rust".to_string();
    world.lsp_replies(
        &language,
        json!({
            "jsonrpc": "2.0",
            "id": asked(world, &language, "textDocument/hover"),
            "result": {"contents": {"kind": "plaintext", "value": "fn main()"}},
        }),
    );
    assert!(world.state.hover.is_some(), "no hover was shown");
}

/// What the box says, whether or not a wrap fell inside the words asked
/// about: the rows are wrapped before they are measured, so a signature can
/// straddle two of them and still be what the reader sees.
#[then(expr = "the hover shows {string}")]
fn hover_shows(world: &mut VardeWorld, text: String) {
    assert!(
        words(&hover_rows(world).join(" ")).contains(&words(&text)),
        "the hover shows: {:?}",
        hover_rows(world)
    );
}

/// What is drawn, row by row — never the markdown the server sent. A row is a
/// rendered row, so a marker that survived into one is a marker the reader is
/// looking at.
fn hover_rows(world: &VardeWorld) -> Vec<String> {
    world
        .state
        .hover
        .as_ref()
        .expect("a hover")
        .lines
        .iter()
        .map(varde::preview::Row::text)
        .collect()
}

#[then(expr = "no hover row holds {string}")]
fn no_hover_row_holds(world: &mut VardeWorld, text: String) {
    let rows = hover_rows(world);
    assert!(
        !rows.iter().any(|row| row.contains(&text)),
        "{text:?} is still on screen: {rows:?}"
    );
}

fn hover_code_row(world: &VardeWorld) -> varde::preview::Row {
    world
        .state
        .hover
        .as_ref()
        .expect("a hover")
        .lines
        .iter()
        .find(|row| row.kind == varde::preview::RowKind::Code)
        .unwrap_or_else(|| panic!("no code row in {:?}", hover_rows(world)))
        .clone()
}

#[then(expr = "the hover has {int} code row")]
fn hover_has_code_rows(world: &mut VardeWorld, count: usize) {
    let hover = world.state.hover.as_ref().expect("a hover");
    let code = hover
        .lines
        .iter()
        .filter(|row| row.kind == varde::preview::RowKind::Code)
        .count();
    assert_eq!(code, count, "{:?}", hover.lines);
}

#[then(expr = "{string} in the hover\'s code row is a keyword")]
fn hover_code_token_is_a_keyword(world: &mut VardeWorld, text: String) {
    let row = hover_code_row(world);
    let piece = row
        .pieces
        .iter()
        .find(|piece| piece.text == text)
        .unwrap_or_else(|| panic!("no piece {text:?} in {row:?}"));
    assert_eq!(piece.token, Some(varde::highlight::Kind::Keyword));
}

/// Its rows plus the two its border sits on — the number the core placed the
/// box by, so a cap the renderer would have to clip is a cap that did not
/// happen.
#[then(expr = "the hover is no taller than {int} rows")]
fn hover_is_no_taller_than(world: &mut VardeWorld, rows: usize) {
    world.state.hover.as_ref().expect("a hover");
    let count = varde::lsp::rows(&world.state);
    assert!(count <= rows, "the box is {count} rows");
}

#[then(expr = "the last hover row says it was cut short")]
fn last_hover_row_is_cut_short(world: &mut VardeWorld) {
    let rows = hover_rows(world);
    assert_eq!(
        rows.last().map(String::as_str),
        Some("\u{2026}"),
        "{rows:?}"
    );
}

/// The words of a string, one space apart, so a line break inside them is not
/// a difference.
fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[then(expr = "no hover is shown")]
fn no_hover(world: &mut VardeWorld) {
    assert!(
        world.state.hover.is_none(),
        "a hover is shown: {:?}",
        world.state.hover
    );
}

#[then(expr = "no hover row is wider than {int} columns")]
fn hover_fits(world: &mut VardeWorld, columns: usize) {
    for row in hover_rows(world) {
        assert!(
            UnicodeWidthStr::width(row.as_str()) <= columns,
            "{row:?} is wider than {columns} columns"
        );
    }
}

#[then(expr = "the hover does not cover line {int}")]
fn hover_does_not_cover(world: &mut VardeWorld, line: usize) {
    let hover = world.state.hover.as_ref().expect("a hover");
    assert!(
        !varde::lsp::covers(&world.state, line),
        "the hover covers {} lines from line {}",
        hover.lines.len(),
        hover.from
    );
}

#[then(expr = "the language server for {string} was sent {int} {string} request")]
fn was_sent_how_many(world: &mut VardeWorld, language: String, count: usize, method: String) {
    let messages = sent(world, &language);
    let asked = messages
        .iter()
        .filter(|message| message["method"] == method)
        .count();
    assert_eq!(
        asked,
        count,
        "{language} was sent: {:?}",
        methods(&messages)
    );
}

/// The edge's one timer, fired. Only if the core actually armed it: a step that
/// asked regardless would make "one request for a burst of typing" a statement
/// about this step rather than about the debounce.
#[when(expr = "the debounce window passes")]
#[given(expr = "the debounce window passes")]
fn debounce_passes(world: &mut VardeWorld) {
    if std::mem::take(&mut world.candidates_armed) {
        world.send(Event::CandidatesDue);
    }
}

#[given(expr = "the pointer moves to line {int} column {int} in the editor")]
#[when(expr = "the pointer moves to line {int} column {int} in the editor")]
fn pointer_moves_to(world: &mut VardeWorld, line: usize, column: usize) {
    world.point(
        Pane::Editor,
        Some((line, column)),
        terminput::KeyModifiers::NONE,
    );
}

#[when("the pointer moves out of the editor")]
fn pointer_leaves_the_editor(world: &mut VardeWorld) {
    world.point(Pane::Editor, None, terminput::KeyModifiers::NONE);
}

/// The pointer put somewhere and left there: the move, and then the edge's
/// timer fired. Only if the core armed it, for the reason the debounce step is
/// guarded — a step that fired regardless would make the dwell a statement
/// about this step.
#[given(expr = "the pointer rests on line {int} column {int} in the editor")]
#[when(expr = "the pointer rests on line {int} column {int} in the editor")]
fn pointer_rests_on(world: &mut VardeWorld, line: usize, column: usize) {
    world.point(
        Pane::Editor,
        Some((line, column)),
        terminput::KeyModifiers::NONE,
    );
    if world.dwell_armed.take().is_some() {
        world.send(Event::HoverDue);
    }
}

/// Where the box is drawn, asked of the same rectangle the renderer and the
/// mouse both read — a step that worked it out for itself would prove its own
/// arithmetic rather than the hit-test's. Its first row of text, inside the
/// border.
fn on_the_hover(world: &VardeWorld) -> (u16, u16) {
    let panes = world.panes();
    let spot = varde::lsp::placement(&world.state)
        .expect("a hover")
        .spot(&world.state, &panes);
    (spot.x + 1, spot.y + 1)
}

#[given("the pointer moves onto the hover")]
#[when("the pointer moves onto the hover")]
fn pointer_onto_the_hover(world: &mut VardeWorld) {
    let (column, row) = on_the_hover(world);
    move_pointer(world, column, row);
}

#[when(expr = "I scroll {word} with the pointer over the hover")]
fn scroll_over_the_hover(world: &mut VardeWorld, direction: String) {
    let (column, row) = on_the_hover(world);
    let kind = match parse_direction(&direction) {
        Direction::Down => mouse::Kind::ScrollDown,
        _ => mouse::Kind::ScrollUp,
    };
    world.report(kind, column, row);
}

#[then(expr = "the hover starts at its row {int}")]
fn hover_starts_at(world: &mut VardeWorld, row: usize) {
    assert_eq!(world.state.hover.as_ref().expect("a hover").first + 1, row);
}

#[given(expr = "the language server for {string} answers the hover with a {int}-line reply")]
fn server_answers_a_long_hover(world: &mut VardeWorld, language: String, lines: usize) {
    let value: Vec<String> = (1..=lines).map(|line| format!("row {line}")).collect();
    world.lsp_replies(
        &language,
        json!({
            "jsonrpc": "2.0",
            "id": asked(world, &language, "textDocument/hover"),
            "result": {"contents": {"kind": "plaintext", "value": value.join("\n")}},
        }),
    );
}

/// Arrived at the way a reader arrives at it: `K`, the reply, and `K` again.
#[given("the hover has focus")]
fn hover_focused(world: &mut VardeWorld) {
    world.send(Event::EditorKey('K'));
    world.lsp_replies(
        "rust",
        json!({
            "jsonrpc": "2.0",
            "id": asked(world, "rust", "textDocument/hover"),
            "result": {"contents": {"kind": "plaintext", "value": "fn main()"}},
        }),
    );
    world.send(Event::EditorKey('K'));
    hover_has_focus(world);
}

#[then("the hover has focus")]
fn hover_has_focus(world: &mut VardeWorld) {
    let hover = world.state.hover.as_ref().expect("a hover");
    assert!(hover.focused, "the keyboard is not in the hover");
}

#[then(expr = "the pointer must rest {int} ms before the server is asked")]
fn dwell_window_is(world: &mut VardeWorld, window: u64) {
    assert_eq!(world.dwell_armed, Some(window));
}

/// The reply to the completion Varde last asked for, under that request's own
/// id — the same correlation the hover and definition answers go through.
#[given(expr = "the language server for {string} answers with candidates:")]
#[when(expr = "the language server for {string} answers with candidates:")]
fn answers_with_candidates(world: &mut VardeWorld, language: String, step: &Step) {
    let rows = &step.table().expect("table").rows;
    let header = &rows[0];
    let items: Vec<Value> = rows
        .iter()
        .skip(1)
        .map(|row| {
            let mut item = json!({});
            for (column, value) in header.iter().zip(row) {
                // The one field the protocol counts rather than spells. The
                // table names the two formats, because a Scenario saying `2`
                // would be a Scenario about the wire rather than about a
                // snippet.
                if column == "format" {
                    item["insertTextFormat"] = json!(match value.as_str() {
                        "snippet" => 2,
                        "plain" => 1,
                        other => panic!("no insert text format is called {other}"),
                    });
                    continue;
                }
                let field = match column.as_str() {
                    "label" => "label",
                    "insert" => "insertText",
                    "filter" => "filterText",
                    "sort" => "sortText",
                    other => panic!("no completion field is called {other}"),
                };
                item[field] = Value::String(value.clone());
            }
            item
        })
        .collect();
    answer_completion(world, &language, Value::Array(items));
}

/// A reply the size a real server's is. Named by its count rather than spelled
/// out, because the Scenario is about the *number* — a box as tall as the reply
/// is what covered the line being typed.
#[when(expr = "the language server for {string} answers with {int} candidates")]
fn answers_with_many_candidates(world: &mut VardeWorld, language: String, many: usize) {
    let items: Vec<Value> = (1..=many)
        .map(|at| {
            let label = format!("worklist_{at:03}");
            json!({"label": label, "insertText": label})
        })
        .collect();
    answer_completion(world, &language, Value::Array(items));
}

/// The server saying it knows of nothing that starts like this: an empty list,
/// which is what a prefix nothing matches answers with.
#[when(expr = "the language server for {string} answers with no candidates")]
fn answers_with_no_candidates(world: &mut VardeWorld, language: String) {
    answer_completion(world, &language, json!([]));
}

fn answer_completion(world: &mut VardeWorld, language: &str, result: Value) {
    world.lsp_replies(
        language,
        json!({"jsonrpc": "2.0", "id": asked(world, language, "textDocument/completion"), "result": result}),
    );
}

/// What the formatting request asked, in the editor's own coordinates: which
/// character asked for it, and where the cursor was when it did. Converted
/// here rather than pinned as the protocol's zero-based line in the Gherkin —
/// the same direction the hover request assertion converts in.
#[then(
    expr = "the language server for {string} was asked to format after {string} at line {int} column {int}"
)]
fn was_asked_to_format(
    world: &mut VardeWorld,
    language: String,
    typed: String,
    line: u64,
    column: u64,
) {
    let asked: Vec<Value> = sent(world, &language)
        .into_iter()
        .filter(|message| message["method"] == "textDocument/onTypeFormatting")
        .map(|message| message["params"].clone())
        .collect();
    assert!(
        asked.iter().any(|params| params["ch"] == typed
            && params["position"] == json!({"line": line - 1, "character": column - 1})),
        "{language} was asked to format: {asked:?}"
    );
}

/// The reply to the formatting Varde last asked for, under that request's own
/// id — the same correlation the hover, definition and completion answers go
/// through.
///
/// The edits are in the editor's own 1-based columns, with `to` the last
/// column the edit covers, so a Scenario says what a reader would say rather
/// than what the wire holds. A protocol range ends at the first place it does
/// not cover, which is that column's number exactly.
#[given(expr = "the language server for {string} answers the formatting with:")]
#[when(expr = "the language server for {string} answers the formatting with:")]
fn answers_the_formatting(world: &mut VardeWorld, language: String, step: &Step) {
    let edits: Vec<Value> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let line: u32 = row[0].parse().expect("a line number");
            let from: u32 = row[1].parse().expect("a column");
            let to: u32 = row[2].parse().expect("a column");
            json!({
                "range": {
                    "start": {"line": line - 1, "character": from - 1},
                    "end": {"line": line - 1, "character": to},
                },
                "newText": row[3],
            })
        })
        .collect();
    world.lsp_replies(
        &language,
        json!({"jsonrpc": "2.0", "id": asked(world, &language, "textDocument/onTypeFormatting"), "result": edits}),
    );
}

/// A list already on screen. Written into the state rather than driven through
/// a reply because the Scenarios that use it are about the keys, not about how
/// the list got there — the Scenarios above cover that, through the reply.
/// Placed the way a reply places it: below the line the cursor is on, which is
/// the one being typed.
#[given(expr = "a candidate list is open in the editor with:")]
fn a_candidate_list_is_open(world: &mut VardeWorld, step: &Step) {
    let items = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| Candidate {
            label: row[0].clone(),
            insert: row[0].clone(),
            filter: None,
            sort: None,
            snippet: false,
        })
        .collect();
    // Placed the way a reply places one, from where the cursor is — and from
    // line 1 column 1 when the Scenario opened no file at all: the Scenarios
    // that arrange a list rather than earning one are about the keys, and a
    // key does not care where the box is.
    let place = world
        .state
        .current_buffer
        .as_ref()
        .and_then(|path| world.state.buffers.get(path))
        .map_or(Place { line: 1, column: 1 }, |buffer| Place {
            line: buffer.line,
            column: buffer.column,
        });
    arrange_candidates(world, items, place);
}

/// A list written into the state, placed the way a reply places one. One
/// function for both arranging steps: the placement is the thing under test in
/// the Scenarios above, and a second copy of it here would be a second author
/// for it.
fn arrange_candidates(world: &mut VardeWorld, items: Vec<Candidate>, place: Place) {
    let path = world
        .state
        .current_buffer
        .clone()
        .unwrap_or_else(|| abs(world, "src/lib.rs"));
    let asked = Ask {
        revision: world
            .state
            .buffers
            .get(&path)
            .map_or(1, |buffer| buffer.revision()),
        path,
        place,
        about: About::Candidates,
    };
    // The list belongs to the file on screen, so the arranged state says so
    // even where the Scenario opened nothing: a list naming another document is
    // taken down by the core before a key can reach it, and a fixture that
    // arranged one would be arranging a state the core cannot be in.
    world.state.current_buffer = Some(asked.path.clone());
    world.state.modal = Modal::Candidates(Candidates::offering(&world.state, items, asked));
}

#[given(expr = "a candidate list is open in the editor with {int} candidates")]
fn a_long_candidate_list_is_open(world: &mut VardeWorld, many: usize) {
    let items = (1..=many)
        .map(|at| Candidate {
            label: format!("worklist_{at:03}"),
            insert: format!("worklist_{at:03}"),
            filter: None,
            sort: None,
            snippet: false,
        })
        .collect();
    arrange_candidates(world, items, Place { line: 1, column: 1 });
}

fn candidate_list(world: &VardeWorld) -> &Candidates {
    match &world.state.modal {
        Modal::Candidates(list) => list,
        modal => panic!("no candidate list is open: {modal:?}"),
    }
}

#[then(expr = "the candidate list is open")]
fn candidate_list_is_open(world: &mut VardeWorld) {
    assert!(!candidate_list(world).items.is_empty());
}

#[then(expr = "the candidate list is not open")]
fn candidate_list_is_not_open(world: &mut VardeWorld) {
    assert!(
        !matches!(world.state.modal, Modal::Candidates(_)),
        "a candidate list is open: {:?}",
        world.state.modal
    );
}

/// Nothing left to Tab to, which is the whole of what "the sequence is over"
/// means: the stops are the ones that have not been visited, so none pending is
/// none left.
#[then(expr = "no tab stops are pending")]
fn no_tab_stops(world: &mut VardeWorld) {
    assert!(
        !matches!(world.state.modal, Modal::Stops { .. }),
        "tab stops are pending: {:?}",
        world.state.modal
    );
}

#[then(expr = "the candidates are:")]
fn candidates_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let shown: Vec<String> = candidate_list(world)
        .shown()
        .iter()
        .map(|candidate| candidate.label.clone())
        .collect();
    assert_eq!(shown, expected);
}

/// The box's height, which is the whole of defect one: a box as tall as the
/// reply is a box the renderer clamps to the pane and draws over the line
/// being typed.
#[then(expr = "the candidate list is {int} rows tall")]
fn candidate_list_rows(world: &mut VardeWorld, rows: usize) {
    assert_eq!(candidate_list(world).rows(), rows);
}

/// Which candidate the window starts at — the selection scrolling inside the
/// box rather than the box growing to hold it.
#[then(expr = "the first candidate shown is {string}")]
fn first_candidate_shown(world: &mut VardeWorld, label: String) {
    let list = candidate_list(world);
    assert_eq!(
        list.shown()[list.first].label,
        label,
        "the window starts at {}",
        list.first
    );
}

#[then(expr = "the candidate list starts at column {int}")]
fn candidate_list_column(world: &mut VardeWorld, column: usize) {
    assert_eq!(candidate_list(world).column, column);
}

#[then(expr = "the selected candidate is {string}")]
fn selected_candidate_is(world: &mut VardeWorld, label: String) {
    assert_eq!(candidate_list(world).chosen().label, label);
}

#[then(expr = "the candidate list does not cover line {int}")]
fn candidates_do_not_cover(world: &mut VardeWorld, line: usize) {
    let list = candidate_list(world);
    assert!(
        !list.covers(line),
        "the list covers {} rows from line {}",
        list.rows(),
        list.from
    );
}

#[then(expr = "the language server for {string} was told {string} is open")]
fn told_open(world: &mut VardeWorld, language: String, path: String) {
    let told = told_about(world, &language, &path);
    assert!(
        told.iter()
            .any(|(method, _, _)| method == "textDocument/didOpen"),
        "{language} was told about {path}: {told:?}"
    );
}

#[then(expr = "the language server for {string} was told {string} is a {string} document")]
fn told_language_id(world: &mut VardeWorld, language: String, path: String, id: String) {
    let uri = format!("file://{}", abs(world, &path).display());
    let ids: Vec<Value> = sent(world, &language)
        .iter()
        .filter(|message| message["method"] == "textDocument/didOpen")
        .map(|message| &message["params"]["textDocument"])
        .filter(|document| document["uri"] == uri.as_str())
        .map(|document| document["languageId"].clone())
        .collect();
    assert_eq!(ids, vec![Value::from(id)]);
}

#[then(expr = "the language server for {string} was told {string} is open with:")]
fn told_open_with(world: &mut VardeWorld, language: String, path: String, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    let opened: Vec<String> = told_about(world, &language, &path)
        .into_iter()
        .filter(|(method, _, _)| method == "textDocument/didOpen")
        .map(|(_, _, text)| text)
        .collect();
    assert_eq!(opened, vec![expected.to_string()]);
}

#[then(expr = "the document version sent for {string} is {int}")]
fn version_sent(world: &mut VardeWorld, path: String, expected: i64) {
    let told: Vec<(String, i64, String)> = world
        .state
        .servers
        .keys()
        .cloned()
        .collect::<Vec<String>>()
        .iter()
        .flat_map(|language| told_about(world, language, &path))
        .collect();
    let (_, version, _) = told
        .last()
        .unwrap_or_else(|| panic!("no server was told about {path}"));
    assert_eq!(*version, expected, "told: {told:?}");
}

#[then(expr = "the language server for {string} was told {string} changed")]
fn told_changed(world: &mut VardeWorld, language: String, path: String) {
    let told = told_about(world, &language, &path);
    assert!(
        told.iter()
            .any(|(method, _, _)| method == "textDocument/didChange"),
        "{language} was told about {path}: {told:?}"
    );
}

#[then(expr = "the language server for {string} was told {string} changed {int} times")]
fn told_changed_times(world: &mut VardeWorld, language: String, path: String, count: usize) {
    let changes: Vec<(String, i64, String)> = told_about(world, &language, &path)
        .into_iter()
        .filter(|(method, _, _)| method == "textDocument/didChange")
        .collect();
    assert_eq!(changes.len(), count, "changes: {changes:?}");
}

#[then(expr = "the language server for {string} was last told {string} holds:")]
fn last_told_holds(world: &mut VardeWorld, language: String, path: String, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    let told = told_about(world, &language, &path);
    let (_, _, text) = told
        .last()
        .unwrap_or_else(|| panic!("{language} was told nothing about {path}"));
    assert_eq!(text, expected);
}

#[then(expr = "the editor says {string}")]
fn editor_says(world: &mut VardeWorld, notice: String) {
    assert!(
        world.notices.contains(&notice),
        "the editor says: {:?}",
        world.notices
    );
}

/// What the editor did *not* say. Load-bearing: the notice the arbitration
/// exists to withhold is the one a plausible implementation gives on the first
/// empty reply, and a Scenario that only checks the right thing happened would
/// pass with the wrong sentence on screen beside it.
#[then(expr = "the editor never said {string}")]
fn editor_never_said(world: &mut VardeWorld, notice: String) {
    assert!(
        !world.notices.contains(&notice),
        "the editor says: {:?}",
        world.notices
    );
}

#[then(expr = "the message names {string}")]
fn message_names(world: &mut VardeWorld, name: String) {
    assert!(
        world.named.contains(&name),
        "the messages named: {:?}",
        world.named
    );
}

#[then(expr = "{string} was reported {int} time")]
fn reported_times(world: &mut VardeWorld, notice: String, count: usize) {
    let reported = world
        .reported
        .iter()
        .filter(|reported| **reported == notice)
        .count();
    assert_eq!(reported, count, "reported: {:?}", world.reported);
}

/// A stand-in file with enough lines for a scenario to name one. The stand-in
/// a file nobody described gets is one line long, so a cursor sent to line 12
/// clamps to line 1 and the scenario passes without the jump ever having
/// happened. Lines wide enough to hold a column, for the same reason.
#[given(expr = "{string} on disk is {int} lines long")]
fn is_lines_long(world: &mut VardeWorld, path: String, lines: usize) {
    let contents = (1..=lines)
        .map(|line| format!("    // line {line} of a stand-in file"))
        .collect::<Vec<String>>()
        .join("\n");
    let full = abs(world, &path);
    world.known_files.insert(full.clone());
    world.files.insert(full, contents);
}

#[given(expr = "{string} on disk holds:")]
fn on_disk_holds(world: &mut VardeWorld, path: String, step: &Step) {
    let contents = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    let full = abs(world, &path);
    world.known_files.insert(full.clone());
    world.files.insert(full, contents);
}

/// The absence that says the server was told the Buffer rather than the file:
/// nothing was written, so what a server heard about cannot have come from disk.
#[then(expr = "{string} on disk still holds:")]
fn on_disk_still_holds(world: &mut VardeWorld, path: String, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    assert_eq!(
        world.files.get(&abs(world, &path)).map(String::as_str),
        Some(expected)
    );
}

/// A Buffer whose Document version has moved: opened, then edited until its
/// revision is the one the Scenario names. Driven through the editor rather
/// than assigned, because `revision` is bumped by content changes and nothing
/// else — which is the whole reason it can be a version at all.
#[given(expr = "{string} is open in the editor at revision {int}")]
fn open_at_revision(world: &mut VardeWorld, path: String, revision: u64) {
    open_clean(world, path.clone());
    world.send(Event::EditorKey('i'));
    for _ in 1..revision {
        world.send(Event::EditorKey(' '));
    }
    world.send(Event::EditorEscape);
    let buffer = world
        .state
        .buffers
        .get(&abs(world, &path))
        .expect("the buffer");
    assert_eq!(
        buffer.revision(),
        revision,
        "the buffer is not at the revision the scenario named"
    );
}

// ---- F31: diagnostics. A server push, so nothing correlates it to a request:
// the world builds the notification and hands it to the core, and no scenario
// runs a server.

/// One `publishDiagnostics` notification, from a table of lines and severities.
/// The version is the one the Scenario names, or absent — the protocol's own
/// optional field, and the difference the stale-version drop turns on.
fn publish(world: &mut VardeWorld, language: &str, path: &str, version: Option<i64>, step: &Step) {
    let diagnostics: Vec<Value> = step
        .table()
        .map(|table| table.rows.iter().skip(1).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            let line: u32 = row[0].trim().parse::<u32>().expect("a line number") - 1;
            json!({
                "range": {
                    "start": {"line": line, "character": 0},
                    "end": {"line": line, "character": 1},
                },
                "severity": severity_number(row[1].trim()),
                "message": row[2].trim(),
            })
        })
        .collect();
    let mut params = json!({
        "uri": format!("file://{}", abs(world, path).display()),
        "diagnostics": diagnostics,
    });
    if let Some(version) = version {
        params["version"] = json!(version);
    }
    world.lsp_replies(
        language,
        json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": params}),
    );
}

/// The protocol's numbering, which the Scenarios never spell out — they name
/// the severity, and this is the one place the wire's integer appears.
fn severity_number(severity: &str) -> u8 {
    match severity {
        "error" => 1,
        "warning" => 2,
        "information" => 3,
        "hint" => 4,
        other => panic!("no such severity: {other}"),
    }
}

#[given(expr = "the language server for {string} publishes diagnostics for {string}:")]
#[when(expr = "the language server for {string} publishes diagnostics for {string}:")]
fn publishes(world: &mut VardeWorld, language: String, path: String, step: &Step) {
    publish(world, &language, &path, None, step);
}

#[given(
    expr = "the language server for {string} publishes diagnostics for {string} at version {int}:"
)]
#[when(
    expr = "the language server for {string} publishes diagnostics for {string} at version {int}:"
)]
fn publishes_at_version(
    world: &mut VardeWorld,
    language: String,
    path: String,
    version: i64,
    step: &Step,
) {
    publish(world, &language, &path, Some(version), step);
}

#[given(
    expr = "the language server for {string} publishes diagnostics for {string} with no version:"
)]
#[when(
    expr = "the language server for {string} publishes diagnostics for {string} with no version:"
)]
fn publishes_without_version(world: &mut VardeWorld, language: String, path: String, step: &Step) {
    publish(world, &language, &path, None, step);
}

/// A push whose ranges name columns as well as lines — the table's `from` and
/// `to` are 1-based and inclusive, as the Scenario reads them, and this is the
/// one place they become the protocol's zero-based start and exclusive end.
#[given(expr = "the language server for {string} publishes diagnostics for {string} with spans:")]
#[when(expr = "the language server for {string} publishes diagnostics for {string} with spans:")]
fn publishes_with_spans(world: &mut VardeWorld, language: String, path: String, step: &Step) {
    let diagnostics: Vec<Value> = step
        .table()
        .map(|table| table.rows.iter().skip(1).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            let number = |at: usize| row[at].trim().parse::<u32>().expect("a number");
            let line = number(0) - 1;
            json!({
                "range": {
                    "start": {"line": line, "character": number(1) - 1},
                    "end": {"line": line, "character": number(2)},
                },
                "severity": severity_number(row[3].trim()),
                "message": row[4].trim(),
            })
        })
        .collect();
    world.lsp_replies(
        language.as_str(),
        json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": {
            "uri": format!("file://{}", abs(world, &path).display()),
            "diagnostics": diagnostics,
        }}),
    );
}

/// What is underlined on a line, as the core answers it — the span already
/// clamped to the characters the line holds.
#[then(expr = "the underline on line {int} of {string} covers columns {int} through {int}")]
fn underline_covers(world: &mut VardeWorld, line: usize, path: String, from: usize, to: usize) {
    let full = abs(world, &path);
    let spans: Vec<(usize, usize)> = lsp::underlines(&world.state, &full, line)
        .into_iter()
        .map(|(from, to, _)| (from, to))
        .collect();
    assert!(
        spans.contains(&(from, to)),
        "line {line} is underlined at {spans:?}"
    );
}

#[then(expr = "line {int} of {string} has no underline")]
fn no_underline(world: &mut VardeWorld, line: usize, path: String) {
    let full = abs(world, &path);
    assert_eq!(
        lsp::underlines(&world.state, &full, line),
        Vec::new(),
        "line {line} is underlined"
    );
}

#[given(expr = "the pointer rests on line {int} column {int} of the editor")]
#[when(expr = "the pointer rests on line {int} column {int} of the editor")]
fn pointer_rests_in_the_editor(world: &mut VardeWorld, line: usize, column: usize) {
    world.point(
        Pane::Editor,
        Some((line, column)),
        terminput::KeyModifiers::NONE,
    );
}

#[when(expr = "the pointer rests on row {int} column {int} of the file tree")]
fn pointer_rests_in_the_tree(world: &mut VardeWorld, row: usize, column: usize) {
    world.point(
        Pane::Tree,
        Some((row, column)),
        terminput::KeyModifiers::NONE,
    );
}

/// What the box beside the line says, wrapped as the reader sees it.
#[then(expr = "the diagnostic box says {string}")]
fn diagnostic_box_says(world: &mut VardeWorld, message: String) {
    let (lines, _) = lsp::pointed(&world.state).expect("no diagnostic box is shown");
    assert!(
        words(&lines.join(" ")).contains(&words(&message)),
        "the box says: {lines:?}"
    );
}

/// Beside the line and never on it: a box over the underline hides the
/// characters it is about.
#[then(expr = "the diagnostic box sits beside line {int}")]
fn diagnostic_box_beside(world: &mut VardeWorld, line: usize) {
    let (_, placement) = lsp::pointed(&world.state).expect("no diagnostic box is shown");
    assert_eq!(placement.from, line + 1);
}

#[then("no diagnostic box is shown")]
fn no_diagnostic_box(world: &mut VardeWorld) {
    assert_eq!(lsp::pointed(&world.state).map(|(lines, _)| lines), None);
}

/// The other half of that distinction: the server has said nothing about the
/// file at all. True until something is pushed, and stated rather than assumed
/// because it is the precondition the Scenario turns on — a Background that
/// came to seed diagnostics would make it pass for the wrong reason.
#[given(expr = "the language server for {string} has published no diagnostics for {string}")]
fn has_published_nothing(world: &mut VardeWorld, _language: String, path: String) {
    let full = abs(world, &path);
    assert!(
        !world.state.diagnostics.contains_key(&full),
        "{path} already carries: {:?}",
        world.state.diagnostics.get(&full)
    );
}

/// The empty push — the server saying a file is clean, which is not the same
/// fact as its never having spoken about it.
#[given(expr = "the language server for {string} publishes no diagnostics for {string}")]
#[when(expr = "the language server for {string} publishes no diagnostics for {string}")]
fn publishes_nothing(world: &mut VardeWorld, language: String, path: String, step: &Step) {
    publish(world, &language, &path, None, step);
}

/// Every publisher's, flattened. A Scenario that names a count names what the
/// file carries, not what one of the servers serving it said — which is the
/// whole of what two publishers on one path changed.
fn diagnostics(world: &VardeWorld, path: &str) -> Vec<lsp::Diagnostic> {
    world
        .state
        .diagnostics
        .get(&abs(world, path))
        .into_iter()
        .flat_map(|by_language| by_language.values().flatten().cloned())
        .collect()
}

#[then(expr = "{string} has no diagnostics")]
fn no_diagnostics(world: &mut VardeWorld, path: String) {
    let held = diagnostics(world, &path);
    assert!(held.is_empty(), "{path} carries: {held:?}");
}

#[then(expr = "{string} has {int} diagnostic")]
#[then(expr = "{string} has {int} diagnostics")]
fn diagnostic_count(world: &mut VardeWorld, path: String, count: usize) {
    let held = diagnostics(world, &path);
    assert_eq!(held.len(), count, "{path} carries: {held:?}");
}

// ---- F31: what Review view says about the change under review. Every step
// below reads `review::diagnostics`, which is the whole of what the view draws:
// a row per file under review, and nothing at all where nobody has spoken.

/// One row of the review's counts, by the file it names.
fn review_row(world: &VardeWorld, path: &str) -> Option<review::Counts> {
    let rows = review::diagnostics(&world.state);
    let (_, counts) = rows
        .iter()
        .find(|(file, _)| file == path)
        .unwrap_or_else(|| panic!("{path} is not under review: {rows:?}"));
    *counts
}

#[then(expr = "{string} is not measured in the review")]
fn not_measured(world: &mut VardeWorld, path: String) {
    assert_eq!(
        review_row(world, &path),
        None,
        "{path} carries a count rather than reading as not measured"
    );
}

#[then(expr = "{string} is measured in the review")]
fn is_measured(world: &mut VardeWorld, path: String) {
    assert!(
        review_row(world, &path).is_some(),
        "{path} reads as not measured"
    );
}

/// The distinction the whole Rule turns on, asserted from the other side: not
/// measured is not zero, and a step that only checked the number would pass on
/// a view that conflated them.
#[then(expr = "the review diagnostic count for {string} is not {int}")]
fn review_count_is_not(world: &mut VardeWorld, path: String, count: usize) {
    let row = review_row(world, &path);
    assert_ne!(
        row.map(|counts| counts.errors + counts.warnings),
        Some(count),
        "{path} counts {row:?}"
    );
}

#[then(expr = "the review diagnostic count for {string} is {int}")]
fn review_count_is(world: &mut VardeWorld, path: String, count: usize) {
    let row = review_row(world, &path);
    assert_eq!(
        row.map(|counts| counts.errors + counts.warnings),
        Some(count),
        "{path} counts {row:?}"
    );
}

/// The counts there are, file by file. Only the measured rows: a file nobody
/// has spoken about has no count to put in a column, which is exactly what
/// `is not measured` above says about it.
#[then("the review diagnostic counts are:")]
fn review_counts_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, usize, usize)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            (
                row[0].trim().to_string(),
                row[1].trim().parse().expect("errors"),
                row[2].trim().parse().expect("warnings"),
            )
        })
        .collect();
    let measured: Vec<(String, usize, usize)> = review::diagnostics(&world.state)
        .into_iter()
        .filter_map(|(file, counts)| counts.map(|counts| (file, counts.errors, counts.warnings)))
        .collect();
    assert_eq!(measured, expected);
}

/// Which files the review reports on at all, measured or not.
#[then("the review diagnostic counts name exactly:")]
fn review_counts_name(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].trim().to_string())
        .collect();
    let named: Vec<String> = review::diagnostics(&world.state)
        .into_iter()
        .map(|(file, _)| file)
        .collect();
    assert_eq!(named, expected);
}

/// The figures, whether or not every file under review is measured — that
/// distinction is the border's to draw, and `Unmeasured` is the absence the
/// steps above assert per file.
fn review_total(world: &VardeWorld) -> review::Counts {
    match review::diagnostic_total(&world.state) {
        review::Total::Whole(total) | review::Total::Partial(total) => total,
        review::Total::Unmeasured => panic!("nothing under review was measured"),
    }
}

#[then(expr = "the review error total is {int}")]
fn review_error_total(world: &mut VardeWorld, expected: usize) {
    assert_eq!(review_total(world).errors, expected);
}

#[then(expr = "the review warning total is {int}")]
fn review_warning_total(world: &mut VardeWorld, expected: usize) {
    assert_eq!(review_total(world).warnings, expected);
}

/// The reviewer put a file back, so git stops reporting it and the review stops
/// listing it. Written the way the edge writes it — `state.repo` is git's own
/// answer, polled — and followed by a pass, because a file leaving the review
/// is a document its server must be told is closed.
#[when(expr = "{string} is restored to what HEAD holds")]
fn restored_to_head(world: &mut VardeWorld, path: String) {
    let kept: Vec<GitFile> = world
        .state
        .repo
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|file| file.path != path)
        .collect();
    world.state.repo = Some(kept);
    world.recompute_file_hunks();
    world.send(Event::Tick);
}

#[then(expr = "the gutter mark on line {int} of {string} is {string}")]
fn gutter_mark_is(world: &mut VardeWorld, line: usize, path: String, severity: String) {
    let full = abs(world, &path);
    assert_eq!(
        lsp::mark(&world.state, &full, line).map(lsp::Severity::as_str),
        Some(severity.as_str()),
        "{path} carries: {:?}",
        diagnostics(world, &path)
    );
}

#[then(expr = "the gutter has no mark on line {int} of {string}")]
fn gutter_has_no_mark(world: &mut VardeWorld, line: usize, path: String) {
    let full = abs(world, &path);
    assert_eq!(
        lsp::mark(&world.state, &full, line).map(lsp::Severity::as_str),
        None,
        "line {line} of {path} is marked"
    );
}

#[then(expr = "the diagnostic message shown is {string}")]
fn diagnostic_message_shown(world: &mut VardeWorld, expected: String) {
    assert_eq!(
        lsp::message_at_cursor(&world.state),
        Some(expected.as_str())
    );
}

#[then(expr = "no diagnostic message is shown")]
fn no_diagnostic_message(world: &mut VardeWorld) {
    assert_eq!(lsp::message_at_cursor(&world.state), None);
}

/// The one screen fact the diagnostics carry: the gutter is the layout's
/// answer, so a mark takes the column the line number was already padded with
/// rather than a column of its own.
#[given(expr = "the editor gutter width is {int}")]
#[then(expr = "the editor gutter width is {int}")]
fn gutter_width_is(world: &mut VardeWorld, expected: u16) {
    assert_eq!(
        varde::gutter(&world.state),
        expected,
        "the gutter changed width"
    );
}

#[given(expr = "the cursor is moved to line {int} column {int}")]
#[when(expr = "the cursor is moved to line {int} column {int}")]
fn cursor_moved_to(world: &mut VardeWorld, line: usize, column: usize) {
    world.send(Event::JumpTo(Place { line, column }));
}

// ---- F31: the palette's second face — what could serve this workspace ----

/// The list is `State::servers`, which starting fills from the merged
/// configuration — so a scenario that only names configuration has to have
/// started before it can open the list. Guarded on nothing having started yet,
/// because starting replaces the whole `State`: a scenario that has already
/// built one says "Varde started in the project" itself, first.
fn started(world: &mut VardeWorld) {
    if world.config.is_none() {
        varde_starts(world);
        world.tell_core();
    }
}

#[given(expr = "the command {string} is on PATH")]
#[when(expr = "the command {string} is on PATH")]
fn command_is_on_path(world: &mut VardeWorld, command: String) {
    world.on_path.insert(command);
    probe_lands(world);
}

/// A fact about this workspace as the edge found it — or did not. Stated the
/// way `PATH` is, and for the same reason: the core is told and never looks.
#[given(expr = "the edge resolved {string} to {string}")]
fn edge_resolved(world: &mut VardeWorld, name: String, value: String) {
    world.workspace_facts.insert(name, value);
    started(world);
    world.tell_core();
}

/// The same fact appearing while Varde runs — what an `npm install` finishing
/// looks like from the core's side. The pass comes round the way the edge
/// brings it round after a re-probe, which is what makes "and no restart" an
/// assertion rather than a hope (R31.24).
#[when(expr = "the edge resolves {string} to {string}")]
fn edge_resolves(world: &mut VardeWorld, name: String, value: String) {
    world.workspace_facts.insert(name, value);
    world.tell_core();
    world.send(Event::PathProbed);
}

#[given(expr = "the edge resolved no {string}")]
fn edge_resolved_nothing(world: &mut VardeWorld, name: String) {
    world.workspace_facts.remove(&name);
    started(world);
    world.tell_core();
}

#[given(expr = "the command {string} is not on PATH")]
#[when(expr = "the command {string} is not on PATH")]
fn command_is_not_on_path(world: &mut VardeWorld, command: String) {
    world.on_path.remove(&command);
    probe_lands(world);
}

/// What `PATH` holds is a fact about the machine, so the step writes the field
/// and then says a fresh probe has landed — which is exactly what the edge
/// does, in that order. It is the *landing* that Varde acts on: a command that
/// has appeared is a reason to forget it was missing, and a re-check that still
/// finds nothing is what offers the restart. Before Varde has started there is
/// nothing to tell — starting replaces the whole `State` — so the Scenarios
/// that state `PATH` as a precondition set the field and stop there.
fn probe_lands(world: &mut VardeWorld) {
    world.tell_core();
    if world.config.is_some() {
        world.send(Event::PathProbed);
    }
}

#[given(expr = "I open the palette")]
#[when(expr = "I open the palette")]
fn open_the_palette(world: &mut VardeWorld) {
    started(world);
    world.send(Event::FallbackBinding);
}

/// A key pressed at the open palette, routed the way the edge routes it: what a
/// letter means with a modal up is the router's answer, not the step's.
#[given(expr = "I press {string} in the palette")]
#[when(expr = "I press {string} in the palette")]
fn press_in_palette(world: &mut VardeWorld, key: String) {
    route_key(world, &key, 0);
}

#[given(expr = "I open Tools")]
#[when(expr = "I open Tools")]
fn open_tools(world: &mut VardeWorld) {
    open_the_palette(world);
    press_in_palette(world, palette_key("Tools").to_string());
}

#[then(expr = "the palette is listing tools")]
fn listing_tools(world: &mut VardeWorld) {
    assert!(
        matches!(world.state.modal, Modal::Tools { .. }),
        "not listing tools: {:?}",
        world.state.modal
    );
}

#[then(expr = "the palette is closed")]
fn palette_is_closed(world: &mut VardeWorld) {
    assert_eq!(world.state.modal, Modal::None);
}

/// A row by its group and name: `rust` is a language server and a formatter,
/// so a name alone does not say which row is meant.
fn tool_row(world: &VardeWorld, kind: tools::Kind, name: &str) -> tools::ToolRow {
    tools::rows(&world.state)
        .into_iter()
        .find(|row| row.kind == kind && row.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no {} row for {name}; rows: {:?}",
                kind.as_str(),
                tools::rows(&world.state)
                    .iter()
                    .map(|row| (row.kind.as_str(), row.name.clone()))
                    .collect::<Vec<_>>()
            )
        })
}

/// The group a step names, spelled the way a reader says it.
fn kind(word: &str) -> tools::Kind {
    match word {
        "server" => tools::Kind::Server,
        "formatter" => tools::Kind::Formatter,
        "requirement" => tools::Kind::Requirement,
        "speech" => tools::Kind::Speech,
        other => panic!("no group {other:?}"),
    }
}

/// The language server scenarios' own spelling: a row named by language alone is
/// the language server's.
fn server_row(world: &VardeWorld, language: &str) -> tools::ToolRow {
    tool_row(world, tools::Kind::Server, language)
}

#[then(expr = "the {word} row for {string} is {string}")]
fn kind_row_reads(world: &mut VardeWorld, group: String, name: String, expected: String) {
    assert_eq!(
        tool_row(world, kind(&group), &name).availability.as_str(),
        expected
    );
}

/// The package manager the row's install needs and this machine lacks, named
/// on the row because `needs-installer` alone does not say which to install.
#[then(expr = "the {word} row for {string} needs the installer {string}")]
fn row_needs_installer(world: &mut VardeWorld, group: String, name: String, expected: String) {
    match tool_row(world, kind(&group), &name).availability {
        tools::Availability::NeedsInstaller { installer } => assert_eq!(installer, expected),
        other => panic!("{name} reads {}", other.as_str()),
    }
}

/// The `[facts.*]` row a server's command is here without, named on the row
/// because `missing-requirement` alone does not say what to install.
#[then(expr = "the {word} row for {string} needs the requirement {string}")]
fn row_needs_requirement(world: &mut VardeWorld, group: String, name: String, expected: String) {
    match tool_row(world, kind(&group), &name).availability {
        tools::Availability::Unmet { needs } => assert_eq!(needs, expected),
        other => panic!("{name} reads {}", other.as_str()),
    }
}

#[then(expr = "the {word} row for {string} differs from its template")]
fn row_differs(world: &mut VardeWorld, group: String, name: String) {
    assert_eq!(
        tool_row(world, kind(&group), &name).origin,
        tools::Origin::Differs
    );
}

#[then(expr = "the {word} row for {string} is the template's")]
fn row_is_the_templates(world: &mut VardeWorld, group: String, name: String) {
    assert_eq!(
        tool_row(world, kind(&group), &name).origin,
        tools::Origin::Template
    );
}

/// Each group once, in the order the list draws them — which is the order
/// the rows come in, since the renderer heads a group where its kind changes.
#[then("the tools list is grouped as:")]
fn tools_grouped_as(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let mut actual: Vec<String> = tools::rows(&world.state)
        .iter()
        .map(|row| row.kind.as_str().to_string())
        .collect();
    actual.dedup();
    assert_eq!(actual, expected);
}

#[then(expr = "the list offers a row for {string}")]
fn list_offers_row(world: &mut VardeWorld, language: String) {
    server_row(world, &language);
}

#[then(expr = "the row for {string} names the command {string}")]
fn row_names_command(world: &mut VardeWorld, language: String, expected: String) {
    assert_eq!(server_row(world, &language).command, expected);
}

#[then(expr = "the row for {string} is {string}")]
fn row_reads(world: &mut VardeWorld, language: String, expected: String) {
    assert_eq!(server_row(world, &language).availability.as_str(), expected);
}

/// The row's own words for what it cannot do, which is the whole of why the
/// state exists: `partly-working` on its own tells a reader to go looking.
#[then(expr = "the row for {string} says it cannot do {string}")]
fn row_says_it_cannot(world: &mut VardeWorld, language: String, expected: String) {
    match server_row(world, &language).availability {
        tools::Availability::Partial { without } => assert_eq!(without, expected),
        other => panic!("{language} reads {}", other.as_str()),
    }
}

/// Which OS the binary was built for, handed in as `main.rs` hands it in — the
/// step that makes a Linux row specifiable on a Mac (R31.22).
#[given(expr = "Varde was built for {string}")]
fn built_for(world: &mut VardeWorld, os: String) {
    // Both, because a Scenario that never starts still has to look up an
    // install command under it: starting copies `Startup::os` into the state,
    // and one that arranges its own world would otherwise be on no OS at all.
    world.state.os = os.clone();
    world.startup.os = os;
}

/// The install key, on the row the arrows were walked to. Both go through the
/// router, so what the list answers to is the router's answer and not the
/// step's — and walking rather than reaching into the modal is what holds the
/// selection to being reachable with no modifier.
#[when(expr = "I install the row for {string}")]
#[given(expr = "I asked to install the row for {string}")]
fn install_the_row(world: &mut VardeWorld, language: String) {
    walk_to_row(world, tools::Kind::Server, &language);
    press_in_palette(world, "i".to_string());
}

#[when(expr = "I take the {word} row for {string}")]
#[given(expr = "I took the {word} row for {string}")]
fn take_the_row(world: &mut VardeWorld, group: String, name: String) {
    walk_to_row(world, kind(&group), &name);
    press_in_palette(world, "i".to_string());
}

fn global_config_path(world: &VardeWorld) -> PathBuf {
    world.startup.varde_home.join(startup::CONFIG_FILE)
}

/// Edited in another editor after Varde started, so what Varde started on
/// and what is on disk now are two different texts.
#[given("the global config has since been edited to:")]
fn global_config_edited(world: &mut VardeWorld, step: &Step) {
    started(world);
    let path = global_config_path(world);
    let text = step.docstring().expect("docstring").trim().to_string();
    world.files.insert(path, text);
}

#[then("the global config still holds everything it held")]
fn global_config_kept(world: &mut VardeWorld) {
    let held = world
        .startup
        .global_config
        .clone()
        .expect("a global config");
    let now = &world.files[&global_config_path(world)];
    assert!(now.starts_with(&held), "the file now reads:\n{now}");
}

#[then(expr = "the global config names the row {string}")]
fn global_config_names(world: &mut VardeWorld, dotted: String) {
    let now = &world.files[&global_config_path(world)];
    let table: toml::Table = now.parse().expect("the written file parses");
    let (section, name) = dotted.split_once('.').expect("section.name");
    assert!(
        table
            .get(section)
            .and_then(toml::Value::as_table)
            .is_some_and(|rows| rows.contains_key(name)),
        "no [{dotted}] in:\n{now}"
    );
}

#[then(expr = "the global config sets {string} to {string}")]
fn global_config_sets(world: &mut VardeWorld, dotted: String, expected: String) {
    let now = &world.files[&global_config_path(world)];
    let table: toml::Table = now.parse().expect("the written file parses");
    let (section, key) = dotted.split_once('.').expect("section.key");
    assert_eq!(
        table.get(section).and_then(|rows| rows.get(key)?.as_str()),
        Some(expected.as_str()),
        "in:\n{now}"
    );
}

fn install_sentinel(world: &VardeWorld) -> PathBuf {
    varde::varde_dir(&world.startup.root, world.startup.sidecar.as_deref()).join(tools::SENTINEL)
}

#[then(expr = "the shell pane runs {string} reporting its exit status")]
fn shell_pane_runs_install(world: &mut VardeWorld, install: String) {
    assert_eq!(
        world.executed,
        vec![tools::reported(&install, &install_sentinel(world))]
    );
}

/// The shipped default, asserted on the server it names rather than on the
/// package manager that installs it: which manager is right for a machine is
/// data `PROGRAMS` carries and a config file may replace, so a scenario pinning
/// the whole string would be a scenario about the data.
#[then(expr = "the shell pane runs a command mentioning {string}")]
fn shell_pane_runs_mentioning(world: &mut VardeWorld, fragment: String) {
    assert!(
        matches!(world.executed.as_slice(), [run] if run.contains(&fragment)),
        "executed {:?}, which does not mention {fragment:?}",
        world.executed
    );
}

/// The sentinel written and seen by the watcher, as the shell's `echo $?`
/// and the rename after it leave it.
#[when(expr = "the install reports the exit status {string}")]
fn install_reports(world: &mut VardeWorld, status: String) {
    let sentinel = install_sentinel(world);
    world.files.insert(sentinel.clone(), format!("{status}\n"));
    world.send(Event::FilesAppeared(vec![(sentinel, tree::Kind::File)]));
}

/// The list open with the selection on that row, walked to with the
/// arrows rather than reached into: what the list answers to is the router's
/// answer, and walking is what holds the selection to being reachable with no
/// modifier. Reopened when it is not up, because `i` closes it — the install
/// runs in the shell pane and the focus follows it — so a re-check
/// after an install has no list to walk. `r` leaves it standing.
fn walk_to_row(world: &mut VardeWorld, group: tools::Kind, name: &str) {
    if !matches!(world.state.modal, Modal::Tools { .. }) {
        open_tools(world);
    }
    let index = tools::rows(&world.state)
        .iter()
        .position(|row| row.kind == group && row.name == name)
        .unwrap_or_else(|| panic!("no {} row for {name}", group.as_str()));
    for _ in 0..index {
        press_in_palette(world, "Down".to_string());
    }
}

#[then(expr = "the focus is the terminal")]
fn focus_is_the_terminal(world: &mut VardeWorld) {
    assert_eq!(world.state.focus, Pane::Terminal);
}

/// A language written off earlier in the session, exactly as the edge writes
/// one off: the spawn produced no process, so nothing is held for it and the
/// core is told. Past tense on purpose — this is the state the Scenario starts
/// from, not the failure it is about. The command is the configured default's,
/// which is what the Scenario then names on `PATH`.
#[given(expr = "a language server for {string} failed to start")]
fn server_failed_to_start(world: &mut VardeWorld, language: String) {
    started(world);
    world.lsp_is_gone(&language, Gone::FailedToStart);
}

/// Started, and started *once*: forgetting a write-off drops the conversation,
/// so a pass that ran before the edge reported the new process would ask for a
/// second one — and a respawn loop is the one failure R31.24 has to avoid. The
/// counted step already spells that, so this is it with the count the Scenario
/// leaves implicit.
#[then(expr = "a language server is started for {string}")]
fn server_is_started_for(world: &mut VardeWorld, language: String) {
    servers_started_for(world, 1, language);
}

/// The write-off still holding, said as an absence: nothing was spawned at any
/// point in the run, so a probe that found nothing changed nothing. Narrower
/// than `no language server was started`, which also holds the *startup*
/// effects to an allowlist — this Scenario has started, opened a buffer and
/// written a language off before it gets here, so the whole run's effects are
/// not the promise being made.
#[then(expr = "no language server is started")]
fn no_language_server_is_started(world: &mut VardeWorld) {
    assert!(
        world.lsp_started.is_empty(),
        "a server was started: {:?}",
        world.lsp_started
    );
}

/// Installing closed the list, so re-checking reopens it and walks back to the
/// row — the whole gesture, through the router, because a re-check nobody can
/// reach is a re-check nobody makes.
#[when(expr = "I re-check the row for {string}")]
#[given(expr = "I re-check the row for {string}")]
fn recheck_the_row(world: &mut VardeWorld, language: String) {
    walk_to_row(world, tools::Kind::Server, &language);
    press_in_palette(world, "r".to_string());
}

#[then(expr = "Varde asks whether to restart")]
fn asks_whether_to_restart(world: &mut VardeWorld) {
    assert_eq!(world.state.modal, Modal::Restart);
}

#[then(expr = "Varde is not asking whether to restart")]
fn is_not_asking_whether_to_restart(world: &mut VardeWorld) {
    assert_ne!(world.state.modal, Modal::Restart);
}

/// The question, reached the only way it can be: a row installed and then
/// re-checked with the command still nowhere. Driven rather than set, so the
/// Scenario that asserts declining changes nothing is asserting it about the
/// state the gesture really produces. `zig` is the vehicle because the shipped
/// defaults give it both a command and a macOS install command and nothing puts
/// `zls` on the world's `PATH`; the question itself carries no language, so
/// which row asked it is not something a Scenario can observe.
#[given(expr = "Varde is asking whether to restart")]
fn is_asking_whether_to_restart(world: &mut VardeWorld) {
    recheck_the_row(world, "zig".to_string());
    command_is_not_on_path(world, "zls".to_string());
    assert_eq!(world.state.modal, Modal::Restart);
}

#[when(expr = "I decline the restart")]
fn decline_the_restart(world: &mut VardeWorld) {
    route_key(world, "n", 0);
}

// ---- F32: formatting. The Language server half drives replies exactly as the
// on-type half does; the command half records what the edge was asked to run
// and answers it in a step, because what a child made of the text is the fact
// only the edge can observe ----

/// The reply to the document formatting Varde last asked for, under that
/// request's own id. The edits are in the editor's own 1-based columns with
/// `to` the last column covered, exactly as the on-type step spells them, so a
/// Scenario says what a reader would say rather than what the wire holds.
#[given(expr = "the language server for {string} answers the document formatting with:")]
#[when(expr = "the language server for {string} answers the document formatting with:")]
fn answers_the_document_formatting(world: &mut VardeWorld, language: String, step: &Step) {
    let edits: Vec<Value> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let line: u32 = row[0].parse().expect("a line number");
            let from: u32 = row[1].parse().expect("a column");
            let to: u32 = row[2].parse().expect("a column");
            json!({
                "range": {
                    "start": {"line": line - 1, "character": from - 1},
                    "end": {"line": line - 1, "character": to},
                },
                "newText": row[3],
            })
        })
        .collect();
    document_formatting_reply(world, &language, json!(edits));
}

/// The server saying there is nothing to change, which the protocol spells as a
/// null result — the empty hand a key was pressed for, so it is the one that
/// speaks.
#[given(expr = "the language server for {string} answers the document formatting with nothing")]
#[when(expr = "the language server for {string} answers the document formatting with nothing")]
fn answers_the_document_formatting_with_nothing(world: &mut VardeWorld, language: String) {
    document_formatting_reply(world, &language, Value::Null);
}

fn document_formatting_reply(world: &mut VardeWorld, language: &str, result: Value) {
    let id = asked(world, language, "textDocument/formatting");
    world.lsp_replies(
        language,
        json!({"jsonrpc": "2.0", "id": id, "result": result}),
    );
}

#[given(expr = "a formatter {string} is configured for {string}")]
fn formatter_configured(world: &mut VardeWorld, command: String, language: String) {
    world.state.formatters.insert(
        language.clone(),
        Formatter {
            command,
            args: Vec::new(),
            install: BTreeMap::new(),
            extensions: shipped()
                .formatters
                .get(&language)
                .map(|formatter| formatter.extensions.clone())
                .unwrap_or_default(),
        },
    );
}

fn configured_formatter<'a>(world: &'a mut VardeWorld, language: &str) -> &'a mut Formatter {
    world
        .state
        .formatters
        .get_mut(language)
        .unwrap_or_else(|| panic!("no formatter is configured for {language}"))
}

#[given(expr = "the formatter for {string} takes the arguments {string}")]
fn formatter_takes_arguments(world: &mut VardeWorld, language: String, args: String) {
    configured_formatter(world, &language).args =
        args.split_whitespace().map(str::to_string).collect();
}

/// The same arrangement as the step below, in a docstring, which is the only
/// way a scenario can put a newline inside the install command — a `{string}`
/// carries the two characters `\n`, not the one the shell reads as Enter.
#[given(expr = "the formatter for {string} is installed on {string} with:")]
fn formatter_installed_with_docstring(
    world: &mut VardeWorld,
    language: String,
    os: String,
    step: &Step,
) {
    let command = step.docstring().expect("docstring").trim_matches('\n');
    configured_formatter(world, &language)
        .install
        .insert(os, command.to_string());
}

#[given(expr = "the formatter for {string} is installed with {string} on {string}")]
fn formatter_installed_with(world: &mut VardeWorld, language: String, command: String, os: String) {
    configured_formatter(world, &language)
        .install
        .insert(os, command);
}

#[given(expr = "the formatter for {string} claims the extension {string}")]
fn formatter_claims_extension(world: &mut VardeWorld, language: String, extension: String) {
    configured_formatter(world, &language)
        .extensions
        .push(extension);
}

#[then(expr = "a formatter is configured for {string}")]
fn a_formatter_is_configured(world: &mut VardeWorld, language: String) {
    assert!(
        world.state.formatters.contains_key(&language),
        "configured: {:?}",
        world.state.formatters.keys().collect::<Vec<_>>()
    );
}

#[then(expr = "there is no formatter configured for {string}")]
fn no_formatter_configured(world: &mut VardeWorld, language: String) {
    assert_eq!(
        world.state.formatters.get(&language),
        None,
        "a formatter is configured for {language}"
    );
}

#[then(expr = "the formatter for {string} is {string}")]
fn the_formatter_for_is(world: &mut VardeWorld, language: String, command: String) {
    assert_eq!(configured_formatter(world, &language).command, command);
}

/// What the edge was handed: the command, and the text that goes in on its
/// stdin. Both in one step, because a command run over text nobody named is
/// half an assertion.
#[then(expr = "the formatter {string} was run with:")]
fn formatter_was_run_with(world: &mut VardeWorld, command: String, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    let run = last_run(world);
    let Effect::RunFormatter {
        command: ran, text, ..
    } = run
    else {
        unreachable!("only RunFormatter is kept")
    };
    assert_eq!((ran.as_str(), text.as_str()), (command.as_str(), expected));
}

#[then(expr = "it was run with the arguments {string}")]
fn it_was_run_with_the_arguments(world: &mut VardeWorld, expected: String) {
    let Effect::RunFormatter { args, .. } = last_run(world) else {
        unreachable!("only RunFormatter is kept")
    };
    assert_eq!(args.join(" "), expected);
}

#[then(expr = "no formatter was run")]
fn no_formatter_was_run(world: &mut VardeWorld) {
    assert!(
        world.formatter_runs.is_empty(),
        "ran: {:?}",
        world.formatter_runs
    );
}

/// How many times, which is the only assertion that can tell a second `:format`
/// that ran from a second `:format` that remembered the command was missing:
/// the answer steps below reach for the *last* run, and a run that never
/// happened leaves the first one there, still current, still applicable.
#[then(expr = "the formatter was run {int} times")]
fn the_formatter_was_run_times(world: &mut VardeWorld, expected: usize) {
    assert_eq!(
        world.formatter_runs.len(),
        expected,
        "ran: {:?}",
        world.formatter_runs
    );
}

fn last_run(world: &VardeWorld) -> Effect {
    world
        .formatter_runs
        .last()
        .cloned()
        .expect("a formatter was run")
}

/// The edge answering, which is the only party that can: it held the child, or
/// it did not. Written as three `When`s rather than as an arrangement, because
/// whether the command is there is a fact about the machine at the moment it
/// ran — nothing is remembered between two `:format`s, which is what the
/// no-restart Scenario turns on.
#[given(expr = "the formatter answers with:")]
#[when(expr = "the formatter answers with:")]
fn formatter_answers_with(world: &mut VardeWorld, step: &Step) {
    let text = step.docstring().expect("docstring").trim_matches('\n');
    formatter_answered(world, format::Answer::Done(text.to_string()));
}

/// Nothing on stdout, which no docstring can express: a Gherkin docstring
/// always carries the newline the fences sit on, and an exit-0 command that
/// wrote nothing is the whole of what this covers.
#[given(expr = "the formatter answers with nothing at all")]
#[when(expr = "the formatter answers with nothing at all")]
fn formatter_answers_with_nothing(world: &mut VardeWorld) {
    formatter_answered(world, format::Answer::Done(String::new()));
}

#[given(expr = "the formatter reports that its command is not installed")]
#[when(expr = "the formatter reports that its command is not installed")]
fn formatter_is_not_installed(world: &mut VardeWorld) {
    formatter_answered(world, format::Answer::Missing);
}

#[given(expr = "the formatter fails with:")]
#[when(expr = "the formatter fails with:")]
fn formatter_fails_with(world: &mut VardeWorld, step: &Step) {
    let said = step.docstring().expect("docstring").trim_matches('\n');
    formatter_answered(world, format::Answer::Failed(said.to_string()));
}

fn formatter_answered(world: &mut VardeWorld, answer: format::Answer) {
    let Effect::RunFormatter {
        language,
        path,
        revision,
        ..
    } = last_run(world)
    else {
        unreachable!("only RunFormatter is kept")
    };
    world.send(Event::FormatterAnswered {
        language,
        path,
        revision,
        answer,
    });
}

// ---- F33: the Buffers pane ----

/// Through the toggle, not by poking the field, for the reason the Risk list's
/// own Given gives: the only way the pane comes to be on screen is being asked
/// for, and asking for it puts focus in it.
#[given(expr = "the Buffers pane is shown")]
fn buffers_pane_is_shown(world: &mut VardeWorld) {
    if world.state.corner != layout::Corner::Buffers {
        world.send(Event::ToggleBuffersList);
    }
    assert_eq!(world.state.focus, Pane::Buffers);
}

#[given(expr = "the Buffers pane is hidden")]
fn buffers_pane_is_hidden(world: &mut VardeWorld) {
    world.state.corner = layout::Corner::Hidden;
}

#[then(expr = "the Buffers pane is shown")]
fn buffers_pane_should_be_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Buffers);
}

#[then(expr = "the Buffers pane is hidden")]
fn buffers_pane_should_be_hidden(world: &mut VardeWorld) {
    assert_ne!(world.state.corner, layout::Corner::Buffers);
}

/// The Corner holding nothing at all, which is not what a per-pane step says: a
/// pane being hidden is "the Corner holds something else", and a restart that
/// opened the wrong occupant satisfies every one of those. The slot names its
/// occupant, so the scenarios about a Corner nobody opened ask about the slot.
#[then(expr = "the corner is empty")]
fn corner_should_be_empty(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Hidden);
}

/// The rows as the pane draws them, by path relative to the root — read off the
/// same list the renderer walks rather than off `buffers` directly, so a pane
/// listing something else fails here.
#[then("the Buffers pane lists:")]
fn buffers_pane_lists(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<PathBuf> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| abs(world, &row[0]))
        .collect();
    let actual: Vec<PathBuf> = varde::buffer_list(&world.state)
        .into_iter()
        .cloned()
        .collect();
    assert_eq!(actual, expected);
}

/// The same `Mark` the editor's dot strip draws, asked of a path the pane is
/// actually listing: a mark for a file with no row would be a claim about
/// nothing.
#[then(expr = "the Buffers pane marks {string} as {string}")]
fn buffers_pane_marks(world: &mut VardeWorld, path: String, expected: String) {
    let path = abs(world, &path);
    assert!(
        varde::buffer_list(&world.state).contains(&&path),
        "{path:?} has no row"
    );
    buffer_mark_is(world, path.to_string_lossy().into_owned(), expected);
}

#[then(expr = "the Buffers pane selection is {string}")]
fn buffers_selection_is(world: &mut VardeWorld, path: String) {
    assert_eq!(
        varde::buffer_selected(&world.state),
        Some(&abs(world, &path))
    );
}

#[given(expr = "the Buffers pane selection is the first row")]
#[then(expr = "the Buffers pane selection is the first row")]
fn buffers_selection_is_the_first_row(world: &mut VardeWorld) {
    assert_eq!(world.state.buffers_selection, 0);
}

#[then(expr = "the Buffers pane first visible row is row {int}")]
fn buffers_first_visible_row(world: &mut VardeWorld, row: usize) {
    assert_eq!(world.state.buffers_scroll, row);
}

#[then(expr = "the Buffers pane selection is in view")]
fn buffers_selection_in_view(world: &mut VardeWorld) {
    let first = world.state.buffers_scroll;
    let last = first + varde::corner_rows(&world.state).max(1);
    assert!(
        (first..last).contains(&world.state.buffers_selection),
        "row {} is not among the rows {first}..{last} on screen",
        world.state.buffers_selection
    );
}

#[when(expr = "I click Buffers pane row {int}")]
fn click_buffers_row(world: &mut VardeWorld, row: usize) {
    world.send(Event::ClickBufferRow(row - 1));
}

/// A real drag across the rows of whichever pane is in the corner — the press
/// the gesture starts with and then the move — fulfilled the way `main` fulfils
/// one: whatever span comes back is filled with characters off a grid and
/// handed to `Event::SelectIn`. A pane holding rows must hand back no span at
/// all, so this drives the press itself rather than `VardeWorld::drag`'s two
/// moves. Which pane it is is the layout's to answer, not the step's: asserting
/// only that Ctrl+C copies nothing would pass against a pane nobody ever
/// dragged in.
#[when("I drag from the corner pane's first row to its second")]
fn drag_across_corner_rows(world: &mut VardeWorld) {
    let panes = world.panes();
    let mut pointer = mouse::Pointer::default();
    for (kind, row) in [
        (mouse::Kind::LeftDown, panes.corner.y + 1),
        (mouse::Kind::LeftDrag, panes.corner.y + 2),
    ] {
        let outcome = mouse::on_mouse(
            &world.state,
            &panes,
            &mut pointer,
            mouse::Input {
                kind,
                column: panes.corner.x + 1,
                row,
                modifiers: terminput::KeyModifiers::NONE,
            },
        );
        for event in outcome.events {
            world.send(event);
        }
        if let Some(selection) = outcome.select {
            let lines = world.pane_lines(selection.pane);
            world.send(Event::SelectIn {
                pane: selection.pane,
                from: selection.from,
                to: selection.to,
                text: span_text(&lines, selection.from, selection.to),
            });
        }
    }
}

/// Enough rows to overflow the pane, opened the way any file is opened, so the
/// scrolling Scenarios are driving the list the pane actually draws.
#[given(expr = "{int} files are open in the editor")]
fn many_files_open(world: &mut VardeWorld, count: usize) {
    for index in 1..=count {
        open_named(world, format!("src/file{index:02}.js"));
    }
}

#[given(expr = "the project {string} records the corner pane as {string}")]
fn state_records_corner(world: &mut VardeWorld, path: String, occupant: String) {
    assert_eq!(path, ".varde/state.json");
    world.startup.state_json = Some(format!("{{\"corner\": \"{occupant}\"}}"));
}

// ---- F34: the Cursor history ----

/// A jump the way every long-distance jump reaches the edge: `Effect::OpenAt`,
/// fulfilled the way `main` fulfils one. Driven as the effect rather than as
/// the landing event it comes back as, because an `OpenAt` for the file already
/// on screen is exactly what the recording rule has to survive — a step that
/// moved the cursor itself would pass against an implementation that records
/// the place it arrived at rather than the place it left.
#[given(expr = "I jump to {string} line {int}")]
#[when(expr = "I jump to {string} line {int}")]
fn jump_to_place(world: &mut VardeWorld, path: String, line: usize) {
    let path = abs(world, &path);
    world.apply(vec![Effect::OpenAt {
        path,
        at: Place { line, column: 1 },
    }]);
}

#[then(expr = "the cursor history is empty")]
fn history_is_empty(world: &mut VardeWorld) {
    assert_eq!(
        varde::history::list(&world.state),
        &[] as &[varde::history::Visit],
    );
}

/// The rows by the file and line each names, read off the same list the pane
/// draws. The whole list, in order, so a place recorded that should not have
/// been fails here rather than passing as a superset.
#[then("the cursor history holds:")]
fn history_holds(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, usize)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| (row[0].clone(), row[1].parse().expect("a line")))
        .collect();
    let actual: Vec<(String, usize)> = varde::history::list(&world.state)
        .iter()
        .map(|visit| (visit.file.clone(), visit.line))
        .collect();
    assert_eq!(actual, expected);
}

/// A filler Visit, named so the Scenarios can tell one from another.
fn filler_visit(which: usize) -> varde::history::Visit {
    varde::history::Visit {
        file: match which {
            0 => "src/oldest.rs".to_string(),
            other => format!("src/f{other}.rs"),
        },
        line: which + 1,
        column: 1,
        text: format!("let filler = {which};"),
    }
}

/// The list, and the history's cursor left past the newest place — where
/// recording that many would have left it, and where somebody who has not
/// travelled anywhere stands. A fixture that left the cursor on the oldest row
/// instead would answer the first Ctrl+p from the middle of the list, which is
/// not the press this pane's boundary is about.
#[given(expr = "the cursor history holds {int} places")]
fn history_holds_places(world: &mut VardeWorld, count: usize) {
    world.state.visits = (0..count).map(filler_visit).collect();
    world.state.history_selection = count;
}

/// The cap itself, so the Scenario about dropping the oldest drives the
/// boundary rather than a number that happens to be near it.
#[given(expr = "the cursor history is full")]
fn history_fill_to_the_cap(world: &mut VardeWorld) {
    history_holds_places(world, varde::history::CAP);
}

#[then(expr = "the cursor history is full")]
fn history_should_be_full(world: &mut VardeWorld) {
    assert_eq!(
        varde::history::list(&world.state).len(),
        varde::history::CAP
    );
}

#[then(expr = "the cursor history holds no place in {string}")]
fn history_holds_no_place_in(world: &mut VardeWorld, file: String) {
    assert!(
        !varde::history::list(&world.state)
            .iter()
            .any(|visit| visit.file == file),
        "{file} is still listed"
    );
}

/// Through the toggle, not by poking the field, for the reason the Risk list's
/// and the Buffers pane's own Givens say so: the only way the pane comes to be
/// on screen is being asked for, and asking for it puts focus in it.
#[given(expr = "the Cursor history pane is shown")]
fn history_pane_is_shown(world: &mut VardeWorld) {
    if world.state.corner != layout::Corner::History {
        world.send(Event::ToggleCursorHistory);
    }
    assert_eq!(world.state.focus, Pane::History);
}

#[given(expr = "the Cursor history pane is hidden")]
fn history_pane_is_hidden(world: &mut VardeWorld) {
    world.state.corner = layout::Corner::Hidden;
}

#[when(expr = "I show the Cursor history pane")]
fn show_history_pane(world: &mut VardeWorld) {
    assert_ne!(world.state.corner, layout::Corner::History);
    world.send(Event::ToggleCursorHistory);
}

#[then(expr = "the Cursor history pane is shown")]
fn history_pane_should_be_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::History);
}

#[then(expr = "the Cursor history pane is hidden")]
fn history_pane_should_be_hidden(world: &mut VardeWorld) {
    assert_ne!(world.state.corner, layout::Corner::History);
}

fn history_row(world: &VardeWorld, row: usize) -> varde::history::Visit {
    varde::history::list(&world.state)
        .get(row - 1)
        .unwrap_or_else(|| {
            panic!(
                "no row {row}: {:?}",
                varde::history::list(&world.state)
                    .iter()
                    .map(|visit| &visit.file)
                    .collect::<Vec<_>>()
            )
        })
        .clone()
}

/// The name, not the path: the pane is the tree's width, and the selected row's
/// whole path is on the border instead.
#[then(expr = "Cursor history row {int} names the file {string} on line {int}")]
fn history_row_names(world: &mut VardeWorld, row: usize, name: String, line: usize) {
    let visit = history_row(world, row);
    assert_eq!(
        std::path::Path::new(&visit.file)
            .file_name()
            .and_then(|name| name.to_str()),
        Some(name.as_str())
    );
    assert_eq!(visit.line, line);
}

/// Through `history::excerpt`, which is where the rule lives — the renderer
/// only draws it, so a step reading the renderer would be asserting a colour.
#[then(expr = "Cursor history row {int} shows the excerpt {string}")]
fn history_row_excerpt(world: &mut VardeWorld, row: usize, expected: String) {
    let visit = history_row(world, row);
    assert_eq!(
        varde::history::excerpt(&visit.text, visit.column, varde::history::WORDS),
        expected
    );
}

/// The file the row recorded, edited on the line the row recorded — through the
/// buffer's own keys, so the Scenario changes the text the way anything else
/// does rather than rewriting state behind the pane's back.
#[when(expr = "the line Cursor history row {int} recorded is edited")]
fn edit_the_recorded_line(world: &mut VardeWorld, row: usize) {
    let visit = history_row(world, row);
    let path = abs(world, &visit.file);
    world.send(Event::ShowBuffer(path));
    world.send(Event::JumpTo(Place {
        line: visit.line,
        column: 1,
    }));
    world.send(Event::EditorKey('x'));
}

#[then(expr = "Cursor history row {int} is stale")]
fn history_row_is_stale(world: &mut VardeWorld, row: usize) {
    let visit = history_row(world, row);
    assert!(
        varde::history::stale(&world.state, &visit),
        "{visit:?} still holds what it recorded"
    );
}

#[then(expr = "Cursor history row {int} is not stale")]
fn history_row_is_not_stale(world: &mut VardeWorld, row: usize) {
    let visit = history_row(world, row);
    assert!(!varde::history::stale(&world.state, &visit), "{visit:?}");
}

#[given(expr = "the Cursor history selection is row {int}")]
fn history_selection_is(world: &mut VardeWorld, row: usize) {
    world.state.history_selection = row - 1;
}

#[then(expr = "the Cursor history selection is row {int}")]
fn history_selection_should_be(world: &mut VardeWorld, row: usize) {
    assert_eq!(
        varde::history::selected(&world.state),
        Some(&history_row(world, row))
    );
}

/// The position past the newest Visit: where the history's cursor sits before
/// anything has been travelled to, which is a place newer than everything
/// recorded and therefore no row.
#[then(expr = "the Cursor history selection is nothing")]
fn history_selection_should_be_nothing(world: &mut VardeWorld) {
    assert_eq!(varde::history::selected(&world.state), None);
    assert_eq!(
        world.state.history_selection,
        varde::history::list(&world.state).len()
    );
}

#[then("the Cursor history row actions offered are:")]
fn history_row_actions_offered(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let actual: Vec<String> = varde::history::row_actions(&world.state)
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(actual, expected);
}

/// A real click, through the hit-test: the row's own columns, so the step
/// exercises the arithmetic that decides whether a click lands on the row or on
/// its icon rather than sending the event it hopes for.
#[when(expr = "I click Cursor history row {int}")]
fn click_history_row(world: &mut VardeWorld, row: usize) {
    world.click(Pane::History, (row, 1), terminput::KeyModifiers::NONE);
}

/// The icon's own columns — the last two the row draws, hard against the
/// right-hand border, which is where `ui` puts it and where `mouse` looks.
#[when(expr = "I click the {string} action on Cursor history row {int}")]
fn click_history_row_action(world: &mut VardeWorld, action: String, row: usize) {
    assert_eq!(action, varde::history::GO_TO, "unknown action {action:?}");
    let column = world.panes().corner.width.saturating_sub(3) as usize;
    world.click(Pane::History, (row, column), terminput::KeyModifiers::NONE);
}

#[then(expr = "the Cursor history first visible row is row {int}")]
fn history_first_visible_row(world: &mut VardeWorld, row: usize) {
    assert_eq!(world.state.history_scroll, row);
}

#[then(expr = "the Cursor history selection is in view")]
fn history_selection_in_view(world: &mut VardeWorld) {
    let first = world.state.history_scroll;
    let last = first + varde::corner_rows(&world.state).max(1);
    assert!(
        (first..last).contains(&world.state.history_selection),
        "row {} is not among the rows {first}..{last} on screen",
        world.state.history_selection
    );
}

// ---- F35: reading aloud ----

/// Everything a Reading needs, in place. The rows are `[speech]`'s own shape
/// with the per-OS tables already resolved, which is what `startup` hands the
/// core — no scenario names a synthesizer, so these are stand-ins with the
/// right *shape* rather than the commands `PROGRAMS` ships (ADR 0013).
#[given("a voice is configured")]
fn voice_configured(world: &mut VardeWorld) {
    world.state.speech = reading::Speech {
        command: "a-synthesizer".to_string(),
        args: vec!["${voice}".to_string()],
        voice: "/voices/a-voice".to_string(),
        speed: 1.00,
        player: "a-player".to_string(),
        install: "fetch-a-voice".to_string(),
    };
    world.voice_child = true;
    world.player_on_path = true;
    world.voice_on_disk = true;
    world.tell_core();
}

/// The three ways a Reading has nothing to speak with. Each undoes one of the
/// three the Background put in place, so what is asserted is which piece is
/// named and not merely that something refused.
#[given("no synthesizer is on the PATH")]
fn no_synthesizer(world: &mut VardeWorld) {
    world.voice_child = false;
    world.tell_core();
}

#[given("no voice is configured")]
fn no_voice(world: &mut VardeWorld) {
    world.state.speech.voice = String::new();
    world.tell_core();
}

/// Named, and deleted since — or never fetched to where the row says.
#[given("the voice file is not on disk")]
fn voice_not_on_disk(world: &mut VardeWorld) {
    world.voice_on_disk = false;
    world.tell_core();
}

#[given("no audio player is configured")]
fn no_player(world: &mut VardeWorld) {
    world.player_on_path = false;
    world.tell_core();
}

#[given(expr = "{string} is open in the editor holding {string}")]
fn open_holding_inline(world: &mut VardeWorld, path: String, contents: String) {
    open_buffer(world, &path, &contents);
}

/// A charwise span over the first line that holds the text. Set as a value
/// rather than driven through a drag: the Selection is pre-existing context
/// here, and which gesture produced it is `selection.feature`'s question.
#[given(expr = "the selection covers {string}")]
fn selection_covers(world: &mut VardeWorld, text: String) {
    let lines: Vec<String> = current_buffer(world)
        .shown()
        .lines()
        .map(str::to_string)
        .collect();
    // A passage none of the open text holds is a passage in another document,
    // which is what a reader who has finished one and moved on to the next
    // has: "the selection covers X" while a Reading of something else plays
    // needs an X to cover, and it is opened rather than typed into the buffer
    // already on screen.
    let lines = if lines.iter().any(|line| line.contains(&text)) {
        lines
    } else {
        open_buffer(world, "elsewhere.md", &text);
        vec![text.clone()]
    };
    let (index, line) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| line.contains(&text))
        .unwrap_or_else(|| panic!("no line holds {text:?}"));
    let at = line.find(&text).expect("the column");
    let column = line[..at].chars().count() + 1;
    world.state.selection = Some(Selection::Buffer {
        anchor: Place {
            line: index + 1,
            column,
        },
        cursor: Place {
            line: index + 1,
            column: column + text.chars().count() - 1,
        },
    });
}

#[given("the selection covers the whole buffer")]
fn selection_covers_all(world: &mut VardeWorld) {
    let lines: Vec<String> = current_buffer(world)
        .shown()
        .lines()
        .map(str::to_string)
        .collect();
    world.state.selection = Some(Selection::Buffer {
        anchor: Place { line: 1, column: 1 },
        cursor: Place {
            line: lines.len().max(1),
            column: lines.last().map_or(1, |line| line.chars().count().max(1)),
        },
    });
}

#[given("there is no selection")]
fn no_selection(world: &mut VardeWorld) {
    world.state.selection = None;
}

#[when("a reading is started")]
fn reading_started(world: &mut VardeWorld) {
    world.tree_before = tree::visible_rows(&world.state)
        .iter()
        .map(|row| row.path.clone())
        .collect();
    world.send(Event::StartReading);
}

/// A Reading already going, stated rather than performed: the passage is put
/// in a markdown buffer, selected whole and started, because that is the only
/// way there is one — R35.1 leaves no cursor or whole-file route to it.
#[given(expr = "a reading of {string} is in flight")]
#[when(expr = "a reading of {string} is started")]
fn a_reading_is_in_flight(world: &mut VardeWorld, text: String) {
    open_buffer(world, "guide.md", &text);
    selection_covers_all(world);
    world.send(Event::StartReading);
    assert!(
        world.state.reading.is_some(),
        "notices: {:?}",
        world.notices
    );
}

/// The passage already on screen, read whole — the two-step Given above is
/// what a scenario about a Reading's *text* needs, and this is what one about
/// where the mark sits needs: a buffer with lines of its own to mark.
#[given("a reading of the whole buffer is in flight")]
fn a_reading_of_the_buffer_is_in_flight(world: &mut VardeWorld) {
    selection_covers_all(world);
    world.send(Event::StartReading);
    assert!(
        world.state.reading.is_some(),
        "notices: {:?}",
        world.notices
    );
}

#[when("the reading is stopped")]
fn reading_stopped(world: &mut VardeWorld) {
    world.send(Event::StopReading);
}

#[then("the reading is in flight")]
fn reading_in_flight(world: &mut VardeWorld) {
    assert!(
        world.state.reading.is_some(),
        "notices: {:?}",
        world.notices
    );
}

#[then("no reading is in flight")]
fn reading_not_in_flight(world: &mut VardeWorld) {
    assert!(world.state.reading.is_none());
}

/// `reading.feature`'s Outline wording for the two above, which is one step
/// with the outcome substituted into it.
#[then(expr = "the reading is refused as {string}")]
fn reading_refused_as(world: &mut VardeWorld, slug: String) {
    reading_not_in_flight(world);
    reading_refuses(world, slug);
}

#[then(expr = "the reading refuses with {string}")]
fn reading_refuses(world: &mut VardeWorld, slug: String) {
    assert!(
        world.notices.contains(&slug),
        "notices: {:?}",
        world.notices
    );
}

/// R35.4. The count and two of the three, which is what the scenario asks: the
/// combinatorial boundary cases are unit tests beside `reading::utterances`,
/// because `AGENTS.md` forbids chasing them through behaviour tests.
#[then(expr = "the reading holds {int} utterances")]
fn reading_holds_utterances(world: &mut VardeWorld, count: usize) {
    let reading = world.state.reading.as_ref().expect("a reading in flight");
    assert_eq!(reading.utterances.len(), count, "{:?}", reading.utterances);
}

#[then(expr = "utterance {int} is {string}")]
fn utterance_is(world: &mut VardeWorld, at: usize, expected: String) {
    let reading = world.state.reading.as_ref().expect("a reading in flight");
    assert_eq!(
        reading.utterances.get(at - 1).map(|one| one.text.as_str()),
        Some(expected.as_str()),
        "{:?}",
        reading.utterances
    );
}

#[then(expr = "the spoken text is {string}")]
fn spoken_text_is(world: &mut VardeWorld, expected: String) {
    assert_eq!(world.speaking.as_deref(), Some(expected.as_str()));
    assert_eq!(
        world
            .state
            .reading
            .as_ref()
            .map(|reading| reading::words(&reading.utterances)),
        Some(expected),
        "what the core holds and what reached the voice are one text"
    );
}

#[then("nothing is waiting on the terminal's input line")]
fn nothing_is_waiting(world: &mut VardeWorld) {
    assert_eq!(world.terminal_input, "");
}

#[given("a reading was refused")]
fn reading_was_refused(world: &mut VardeWorld) {
    world.send(Event::StartReading);
    reading_not_in_flight(world);
}

/// Offered as Tools offers it: the list up, with the install key's row on it.
#[then(expr = "the {word} row for {string} is offered")]
fn row_is_offered(world: &mut VardeWorld, group: String, name: String) {
    let Modal::Tools { row } = world.state.modal else {
        panic!("not listing tools: {:?}", world.state.modal);
    };
    let offered = tools::rows(&world.state)
        .into_iter()
        .nth(row)
        .expect("the row is in the list");
    assert_eq!(
        (offered.kind, offered.name.as_str()),
        (kind(&group), name.as_str())
    );
}

/// R35.10. The stream is Varde's own scratch and lives outside every
/// workspace, so nothing a Reading does may name a path under the root.
#[then("no file was written inside the workspace root")]
fn nothing_written_in_workspace(world: &mut VardeWorld) {
    let root = world.state.root.clone();
    let inside: Vec<&PathBuf> = world
        .wrote
        .iter()
        .chain(world.dirs.iter())
        .filter(|path| path.starts_with(&root))
        .collect();
    assert!(
        inside.is_empty(),
        "written inside the workspace: {inside:?}"
    );
}

#[then("the file tree is unchanged")]
fn tree_is_unchanged(world: &mut VardeWorld) {
    let now: Vec<PathBuf> = tree::visible_rows(&world.state)
        .iter()
        .map(|row| row.path.clone())
        .collect();
    assert_eq!(now, world.tree_before);
}

/// How long the stand-in edge's voice takes over an Utterance, plus the
/// silence the Reading itself asked for. A scenario cannot derive this — a
/// voice's pace is in the audio it produced — so the World stands in for the
/// synthesizer with a round second each, exactly as it stands in for the pty
/// and the disk elsewhere.
const SAID_MS: u32 = 1_000;

/// Where each Utterance starts in the stream, which is what the edge tells the
/// core once the stream exists.
fn built(utterances: &[reading::Utterance]) -> Vec<u32> {
    let mut at_ms = 0;
    utterances
        .iter()
        .map(|one| {
            let start = at_ms;
            at_ms += SAID_MS + one.gap_ms;
            start
        })
        .collect()
}

/// The edge reporting where the sound has got to, which is the only way the
/// core learns it.
fn sound_reached(world: &mut VardeWorld, at_ms: u32) {
    let offsets = world.stream.clone();
    world.send(Event::Speaking { at_ms, offsets });
}

/// Far enough in that a resume from the start would be a different answer, and
/// still inside the first Utterance.
const PART_WAY: u32 = 300;

fn reading_now(world: &VardeWorld) -> &reading::Reading {
    world.state.reading.as_ref().expect("a reading in flight")
}

/// The sound sitting at that Utterance's first millisecond, told the way the
/// edge tells it.
#[given(expr = "the current utterance is {int}")]
fn current_utterance_starts(world: &mut VardeWorld, at: usize) {
    let at_ms = world.stream[at - 1];
    sound_reached(world, at_ms);
}

/// R35.12. Which lines the mark covers, asked of the core rather than the
/// renderer: `ui` only draws it, so a step reading the screen would be
/// asserting a colour the ticket says no scenario asserts.
#[then(expr = "the marked lines are {int} to {int}")]
fn marked_lines(world: &mut VardeWorld, from: usize, to: usize) {
    assert_eq!(
        reading::mark(&world.state),
        Some((from, to)),
        "at {}ms",
        reading_now(world).at_ms
    );
}

#[then("no lines are marked")]
fn no_lines_marked(world: &mut VardeWorld) {
    assert_eq!(reading::mark(&world.state), None);
}

#[then(expr = "the current utterance is {int}")]
fn current_utterance_is(world: &mut VardeWorld, at: usize) {
    let reading = reading_now(world);
    assert_eq!(
        reading.at() + 1,
        at,
        "at {}ms of {:?}",
        reading.at_ms,
        reading.offsets
    );
}

/// Two Utterances' worth of sound has come out, so the third is the one being
/// spoken. Driven from the offsets the edge built rather than from a duration
/// a scenario invented: the boundary is where the stream says it is.
#[when(expr = "the reading has been speaking for the length of {int} utterances")]
fn speaking_for(world: &mut VardeWorld, count: usize) {
    let at_ms = *world.stream.get(count).expect("that many utterances");
    sound_reached(world, at_ms);
}

#[when("the next utterance is asked for")]
fn next_utterance(world: &mut VardeWorld) {
    world.send(Event::NextUtterance);
}

#[when("the previous utterance is asked for")]
fn previous_utterance(world: &mut VardeWorld) {
    world.send(Event::PreviousUtterance);
}

/// A Reading in flight has had sound out of it for a moment, and the edge says
/// where the sound got to as it stops it — `main`'s `pause` reports the offset
/// unthrottled for exactly this reason, since the core's own position is a
/// report up to 80ms old. A pause taken at offset zero would let "resumes
/// where it stopped" pass for a restart.
#[when("the reading is paused")]
fn reading_paused(world: &mut VardeWorld) {
    sound_reached(world, PART_WAY);
    world.paused_at = Some(reading_now(world).at_ms);
    world.send(Event::PlayPause);
}

#[then("the reading is paused")]
fn reading_is_paused(world: &mut VardeWorld) {
    assert!(reading_now(world).paused);
}

#[when("the reading is resumed")]
fn reading_resumed(world: &mut VardeWorld) {
    world.send(Event::PlayPause);
}

/// Through the Transport's own action, not `Event::PlayPause`: what the
/// scenario is about is the control on the border, and the routing from the
/// glyph to the event is part of what it promises.
#[when("the play control is pressed")]
fn play_control_pressed(world: &mut VardeWorld) {
    world.send(Event::PaneAction(reading::PLAY_PAUSE));
}

/// R35.6. The offset the edge was asked to play from is the one the pause
/// recorded, and the Reading is still where it stopped rather than back at the
/// start — and nothing was synthesized again, which is what the stream being
/// untouched says.
#[then("the reading resumes from where it was paused")]
fn resumes_where_paused(world: &mut VardeWorld) {
    let paused_at = world.paused_at.expect("a pause to resume from");
    assert_eq!(world.resumed_from, Some(paused_at));
    assert_eq!(reading_now(world).at_ms, paused_at);
    assert!(!reading_now(world).paused);
    assert_eq!(
        world.stream,
        built(&reading_now(world).utterances),
        "the reading was synthesized again"
    );
}

/// R35.5. One player, because the one that was playing was stopped before the
/// next was started: a Reading supersedes rather than queueing, and two
/// streams over one another is what a queue would sound like.
#[then("exactly one reading is in flight")]
fn exactly_one_reading(world: &mut VardeWorld) {
    assert!(world.state.reading.is_some());
    assert_eq!(world.players, 1);
}

#[then("no sound is being played")]
fn nothing_is_playing(world: &mut VardeWorld) {
    assert_eq!(world.speaking, None);
}

/// The stream exists to be played and is deleted with the player, so what a
/// scenario can see of it is the edge holding neither (ADR 0014).
#[then("no stream file remains")]
fn no_stream_remains(world: &mut VardeWorld) {
    nothing_is_playing(world);
}

#[given(expr = "the reading speed is {float}")]
fn reading_speed_is(world: &mut VardeWorld, speed: f32) {
    world.state.speech.speed = speed;
}

#[given(expr = "a reading of {string} is in flight at speed {float}")]
fn a_reading_in_flight_at_speed(world: &mut VardeWorld, text: String, speed: f32) {
    reading_speed_is(world, speed);
    a_reading_is_in_flight(world, text);
}

/// What the voice was actually asked for, which is the reciprocal and never
/// the multiplier. Inverted here rather than through `reading::duration_scale`
/// on purpose: a scenario that called the function under test would stay green
/// if the core ever handed the effect a scale instead of a multiplier, which is
/// the one thing this step exists to catch. To two decimals because the table
/// is written the way a person reads it — `0.90` inverts to 1.111…, and pinning
/// every digit of a float would be pinning the format rather than the number.
#[then(expr = "the voice is asked for a duration scale of {float}")]
fn voice_asked_for_scale(world: &mut VardeWorld, scale: f32) {
    let asked = 1.0 / world.spoken_at.expect("the voice to have been asked");
    assert!(
        (asked - scale).abs() < 0.005,
        "the voice was asked for {asked}, not {scale}"
    );
}

#[when(expr = "the reading speed is changed to {float}")]
fn reading_speed_changed(world: &mut VardeWorld, speed: f32) {
    world.send(Event::SetSpeed(speed));
}

/// The pace of the stream that is playing, which is the speed it was *built*
/// at — so a speed change that re-synthesized would show up here as the new
/// one rather than the old.
#[then(expr = "the reading in flight is still at speed {float}")]
#[then(expr = "the reading is at speed {float}")]
fn reading_is_at_speed(world: &mut VardeWorld, speed: f32) {
    assert_eq!(world.spoken_at, Some(speed));
}

/// R35.8. Drawn or absent, never greyed — and `ui` draws the same list
/// `mouse` hit-tests, so what the bar offers is one value to assert.
#[then("the transport is on screen")]
fn transport_on_screen(world: &mut VardeWorld) {
    assert!(
        !reading::transport(&world.state).is_empty(),
        "no transport for {:?}",
        world.state.current_buffer
    );
}

#[then("the transport is not on screen")]
fn transport_not_on_screen(world: &mut VardeWorld) {
    assert_eq!(
        reading::transport(&world.state),
        vec![],
        "{:?}",
        world.state.current_buffer
    );
}

/// R44.7. The names say what pressing each does, so the first reads `play`
/// here and `pause` while a Reading plays.
#[then("the editor's Transport's Chips are:")]
fn editor_transport_chips_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table
        .as_ref()
        .expect("a table of Chips")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let drawn: Vec<&str> = reading::transport(&world.state)
        .iter()
        .map(|chip| chip.name)
        .collect();
    assert_eq!(drawn, expected);
}

/// R35.8. The bar is an affordance and a reminder, never the only way in: a
/// control reachable only by mouse is one the cheatsheet cannot promise.
///
/// Driven rather than listed: each control is pressed, its command is typed,
/// and the two are held to leaving the same state and asking for the same
/// effects — a command that merely exists could still do something else. The
/// table is checked against what the bar actually offers, so a sixth control
/// fails here rather than shipping without a key.
#[then("every transport action is reachable from the keyboard")]
fn every_transport_action_has_a_key(world: &mut VardeWorld) {
    open_buffer_plain(world, "guide.md".to_string());
    let commands = [
        (reading::PLAY_PAUSE, ":pause"),
        (reading::PREVIOUS, ":prev"),
        (reading::NEXT, ":next"),
        (reading::STOP, ":stop"),
        // The ladder's next rung, said out loud: a glyph on a border cannot be
        // typed a number into, so the click steps and the key names.
        (reading::SPEED, ":speed 1.25"),
    ];
    // Each Chip's keys are the command it is held to below, so a Chip cannot
    // teach a key that does something else.
    let offered: Vec<(&str, &str)> = reading::transport(&world.state)
        .iter()
        .map(|chip| (chip.action, chip.keys))
        .collect();
    let listed: Vec<(&str, &str)> = commands
        .iter()
        .map(|(action, line)| (*action, line.split(' ').next().unwrap_or(line)))
        .collect();
    assert_eq!(offered, listed, "a control with no key beside it");
    for (action, line) in commands {
        let clicked = update(&world.state, Event::PaneAction(action));
        let mut drafts = varde::keys::Drafts::default();
        let mut typed = world.state.clone();
        let mut effects = Vec::new();
        for key in line
            .chars()
            .map(plain_key)
            .chain([terminput::KeyEvent::new(terminput::KeyCode::Enter)])
        {
            for event in keys::on_key_event(&typed, &mut drafts, key, 0) {
                let (next, more) = update(&typed, event);
                typed = next;
                effects.extend(more);
            }
        }
        assert_eq!(
            (typed, effects),
            clicked,
            "{line} and the {action} control part ways"
        );
    }
    assert!(
        keys::CHEATSHEET
            .iter()
            .any(|(keys, _, _)| keys.contains(":read") && keys.contains(":speed")),
        "the transport's keys are not on the cheatsheet"
    );
}

// ---- Indent guides ----

fn guides_on(world: &VardeWorld, line: usize) -> Vec<varde::editor::Guide> {
    current_buffer(world)
        .guides([line])
        .pop()
        .expect("a line that far down")
}

#[then(expr = "the indent guides on line {int} are at columns {string}")]
fn guides_at(world: &mut VardeWorld, line: usize, columns: String) {
    let expected: Vec<usize> = columns
        .split(", ")
        .map(|column| column.parse().expect("a column"))
        .collect();
    let found: Vec<usize> = guides_on(world, line)
        .iter()
        .map(|guide| guide.column)
        .collect();
    assert_eq!(found, expected);
}

#[then(expr = "line {int} has no indent guides")]
fn no_guides(world: &mut VardeWorld, line: usize) {
    assert!(guides_on(world, line).is_empty());
}

#[then(expr = "the indent guide on line {int} at column {int} is {word}")]
fn guide_is(world: &mut VardeWorld, line: usize, column: usize, drawn: String) {
    let guide = guides_on(world, line)
        .into_iter()
        .find(|guide| guide.column == column)
        .expect("a guide in that column");
    assert_eq!(
        guide.active,
        match drawn.as_str() {
            "heavy" => true,
            "light" => false,
            other => panic!("unknown guide {other:?}"),
        }
    );
}

#[then(expr = "no indent guide on line {int} is heavy")]
fn no_heavy_guide(world: &mut VardeWorld, line: usize) {
    assert!(guides_on(world, line).iter().all(|guide| !guide.active));
}

#[then(expr = "the marked brackets are at:")]
fn marked_brackets(world: &mut VardeWorld, step: &Step) {
    let rows = &step.table.as_ref().expect("a table").rows;
    let expected: Vec<(usize, usize)> = rows[1..]
        .iter()
        .map(|row| {
            (
                row[0].parse().expect("a line"),
                row[1].parse().expect("a column"),
            )
        })
        .collect();
    let (from, to) = current_buffer(world).bracket_pair().expect("a marked pair");
    assert_eq!(
        vec![(from.line, from.column), (to.line, to.column)],
        expected
    );
}

#[then(expr = "no brackets are marked")]
fn no_marked_brackets(world: &mut VardeWorld) {
    assert_eq!(current_buffer(world).bracket_pair(), None);
}

// ---- F45: the Debug session. No scenario runs an adapter: `FakeAdapter`
// answers what a real one would, and the events a scenario names are driven in
// as the edge would drive them.

/// What a scenario's config files name, loaded into the running workspace
/// rather than by restarting it, which would take the Breakpoints and Buffers
/// the scenario set up with it. An adapter a step already configured is kept.
fn load_debug_config(world: &mut VardeWorld) {
    let (loaded, _, _) = startup::start(&world.startup).expect("the scenario's config starts");
    world.state.launches = loaded.launches;
    world.state.runs = loaded.runs;
    for (language, adapter) in loaded.adapters {
        world.state.adapters.entry(language).or_insert(adapter);
    }
}

/// A session over whichever adapter the Background configured, for the
/// features that are about being Paused rather than about how a session is
/// started: they name no Launch configuration, so one is made here.
fn session_running(world: &mut VardeWorld) {
    let adapter = world
        .state
        .adapters
        .keys()
        .next()
        .cloned()
        .expect("a Debug adapter is configured");
    world.state.launches.insert(
        "debug".to_string(),
        startup::Launch {
            adapter,
            request: "launch".to_string(),
            args: serde_json::Map::new(),
            reattach: true,
        },
    );
    world.dap.ready = true;
    world.send(Event::StartLaunch("debug".to_string()));
    adapter_event(world, json!({ "type": "event", "event": "initialized" }));
}

fn adapter_event(world: &mut VardeWorld, message: Value) {
    world.adapter_says(message);
    world.tell_core();
}

/// The stack a thread answers with, unless a scenario gave it one.
fn stopped_at(world: &mut VardeWorld, thread: i64, file: &str, line: usize, reason: &str) {
    plant_the_programs_locals(world);
    let path = abs(world, file);
    let stack = world
        .dap
        .stacks
        .entry(thread)
        .or_insert_with(|| vec![("main".to_string(), path.clone(), line, String::new())]);
    // Where the program stopped is the top of its stack, whatever the pause
    // before it left there: a second `stopped` event naming another line with
    // the stack still pointing at the first is a program that never moved.
    if let Some(top) = stack.first_mut() {
        top.1 = path;
        top.2 = line;
    }
    let mut body = json!({ "threadId": thread, "reason": reason });
    if let Some(text) = world.dap.text.clone() {
        body["text"] = json!(text);
    }
    let stopped = json!({ "type": "event", "event": "stopped", "body": body });
    world.dap.stopped = Some(stopped.clone());
    adapter_event(world, stopped);
    // What the pause itself asked for is the pause, so "since the pause"
    // counts from after it has brought the Variables up.
    world.dap.since_pause = world.dap.sent.len();
}

/// The requests the adapter was sent, by command, in order.
fn dap_requests<'a>(world: &'a VardeWorld, command: &str) -> Vec<&'a Value> {
    world
        .dap
        .sent
        .iter()
        .filter(|message| message["type"] == "request" && message["command"] == command)
        .collect()
}

fn last_request<'a>(world: &'a VardeWorld, command: &str) -> &'a Value {
    dap_requests(world, command)
        .pop()
        .unwrap_or_else(|| panic!("no {command:?} request; sent {:?}", world.dap.sent))
}

#[given(expr = "the Debug adapter for {string} is ready")]
fn debug_adapter_ready(world: &mut VardeWorld, _language: String) {
    world.dap.ready = true;
}

#[when(expr = "I open the launch palette")]
fn open_launch_palette(world: &mut VardeWorld) {
    load_debug_config(world);
    world.send(Event::FallbackBinding);
    route_key(world, &palette_key("Launch").to_string(), 0);
}

/// Through the palette the way a person goes: its Launch face, the arrows down
/// to the row, and Enter.
#[given(expr = "I start the Launch configuration {string} from the palette")]
#[when(expr = "I start the Launch configuration {string} from the palette")]
fn start_launch_from_palette(world: &mut VardeWorld, name: String) {
    open_launch_palette(world);
    let row = varde::debug::launches(&world.state)
        .iter()
        .position(|offered| *offered == name)
        .unwrap_or_else(|| panic!("the launch palette does not offer {name:?}"));
    for _ in 0..row {
        route_key(world, "Down", 0);
    }
    route_key(world, "Enter", 0);
}

#[then(expr = "the launch palette offers {string}")]
fn launch_palette_offers(world: &mut VardeWorld, name: String) {
    assert!(matches!(world.state.modal, Modal::Launches { .. }));
    assert!(
        varde::debug::launches(&world.state).contains(&name.as_str()),
        "offered: {:?}",
        varde::debug::launches(&world.state)
    );
}

#[given(expr = "a Debug session was started from the Launch configuration {string}")]
#[when(expr = "a Debug session was started from the Launch configuration {string}")]
fn session_was_started(world: &mut VardeWorld, name: String) {
    world.dap.ready = true;
    start_launch_from_palette(world, name);
    adapter_event(world, json!({ "type": "event", "event": "initialized" }));
}

#[given(expr = "a Debug session is Running")]
fn debug_session_is_running(world: &mut VardeWorld) {
    session_running(world);
}

#[given(expr = "a Debug session is Paused at {string} line {int}")]
#[when(expr = "a Debug session is Paused at {string} line {int}")]
fn debug_session_is_paused(world: &mut VardeWorld, file: String, line: usize) {
    session_running(world);
    stopped_at(world, 1, &file, line, "breakpoint");
}

#[given(expr = "a Debug session is Paused at {string} line {int} on thread {int}")]
fn debug_session_is_paused_on(world: &mut VardeWorld, file: String, line: usize, thread: i64) {
    session_running(world);
    stopped_at(world, thread, &file, line, "breakpoint");
}

#[given(
    expr = "a Debug session is Paused in {string} at {string} line {int} called from {string} at {string} line {int}"
)]
fn debug_session_is_paused_in(
    world: &mut VardeWorld,
    inner: String,
    inner_file: String,
    inner_line: usize,
    outer: String,
    outer_file: String,
    outer_line: usize,
) {
    let stack = vec![
        (inner, abs(world, &inner_file), inner_line, String::new()),
        (outer, abs(world, &outer_file), outer_line, String::new()),
    ];
    world.dap.stacks.insert(1, stack);
    session_running(world);
    stopped_at(world, 1, &inner_file, inner_line, "breakpoint");
}

#[given(expr = "the Debug adapter sends the event {string}")]
#[when(expr = "the Debug adapter sends the event {string}")]
fn adapter_sends_event(world: &mut VardeWorld, event: String) {
    adapter_event(world, json!({ "type": "event", "event": event }));
}

#[given(
    expr = "the Debug adapter sends the {string} event for thread {int} at {string} line {int} with reason {string}"
)]
#[when(
    expr = "the Debug adapter sends the {string} event for thread {int} at {string} line {int} with reason {string}"
)]
fn adapter_sends_stopped(
    world: &mut VardeWorld,
    event: String,
    thread: i64,
    file: String,
    line: usize,
    reason: String,
) {
    assert_eq!(event, "stopped");
    stopped_at(world, thread, &file, line, &reason);
}

#[given(expr = "the Debug adapter reports the program continued")]
#[when(expr = "the Debug adapter reports the program continued")]
fn adapter_reports_continued(world: &mut VardeWorld) {
    adapter_event(
        world,
        json!({ "type": "event", "event": "continued", "body": { "threadId": 1, "allThreadsContinued": true } }),
    );
}

#[when(expr = "the Debug adapter answers {string} with the error {string}")]
fn adapter_answers_with_error(world: &mut VardeWorld, command: String, error: String) {
    let seq = last_request(world, &command)["seq"].clone();
    adapter_event(
        world,
        json!({ "type": "response", "request_seq": seq, "success": false, "command": command, "message": error }),
    );
}

#[when(expr = "the edge reports the Debug adapter is gone")]
fn edge_reports_adapter_gone(world: &mut VardeWorld) {
    world.dap.held = false;
    world.send(Event::DapGone {
        why: varde::debug::Gone::Exited,
        from: 0,
    });
}

/// Only for the port the session is watching: an edge told to try another
/// would be trying the wrong one.
#[given(expr = "the edge reports the port {int} answers")]
#[when(expr = "the edge reports the port {int} answers")]
fn edge_reports_port_answers(world: &mut VardeWorld, port: u16) {
    let watched = varde::debug::waiting_on(&world.state).map(|(_, watched)| watched);
    assert_eq!(watched, Some(port));
    world.send(Event::DapPortAnswers);
}

#[when(expr = "the edge reports the Debug adapter could not be started")]
fn edge_reports_adapter_failed(world: &mut VardeWorld) {
    world.send(Event::DapGone {
        why: varde::debug::Gone::FailedToStart,
        from: 0,
    });
}

#[then(expr = "the Debug adapter's {word} arguments are:")]
fn request_arguments_are(world: &mut VardeWorld, request: String, step: &Step) {
    let expected: Value =
        serde_json::from_str(step.docstring().expect("docstring")).expect("JSON arguments");
    assert_eq!(last_request(world, &request)["arguments"], expected);
}

#[then("the Debug adapter was sent, in order:")]
fn adapter_sent_in_order(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<&str> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].as_str())
        .collect();
    let sent: Vec<&str> = world
        .dap
        .sent
        .iter()
        .filter(|message| message["type"] == "request")
        .filter_map(|message| message["command"].as_str())
        .collect();
    assert_eq!(sent, expected);
}

#[then(expr = "the Debug adapter was sent no {string} request")]
fn adapter_sent_no(world: &mut VardeWorld, command: String) {
    assert!(
        dap_requests(world, &command).is_empty(),
        "{:?}",
        world.dap.sent
    );
}

#[then(expr = "the Debug adapter was sent a {string} request")]
fn adapter_sent_a(world: &mut VardeWorld, command: String) {
    last_request(world, &command);
}

#[then(expr = "no Debug adapter was spawned")]
fn no_adapter_spawned(world: &mut VardeWorld) {
    assert_eq!(world.dap.spawned, Vec::<String>::new());
}

#[then(expr = "the Debug adapter is reached on port {int}")]
fn adapter_reached_on(world: &mut VardeWorld, port: u16) {
    assert_eq!(world.dap.dialed, vec![port]);
}

#[then(expr = "the Debug adapter was sent {int} {string} requests")]
fn adapter_sent_count(world: &mut VardeWorld, count: usize, command: String) {
    assert_eq!(dap_requests(world, &command).len(), count);
}

#[then(expr = "the Debug adapter was sent {int} {string} requests for {string}")]
fn adapter_sent_count_for(world: &mut VardeWorld, count: usize, command: String, file: String) {
    let path = json!(abs(world, &file));
    let sent = dap_requests(world, &command)
        .into_iter()
        .filter(|request| request["arguments"]["source"]["path"] == path)
        .count();
    assert_eq!(sent, count);
}

#[then(expr = "the Debug adapter was sent a {string} request for thread {int}")]
fn adapter_sent_for_thread(world: &mut VardeWorld, command: String, thread: i64) {
    assert_eq!(
        last_request(world, &command)["arguments"]["threadId"],
        thread
    );
}

#[then(expr = "the Debug adapter was sent a {string} request for thread {int} alone")]
fn adapter_sent_for_thread_alone(world: &mut VardeWorld, command: String, thread: i64) {
    let arguments = &last_request(world, &command)["arguments"];
    assert_eq!(arguments["threadId"], thread);
    assert_eq!(arguments["singleThread"], true);
}

/// The lines of the last request naming `file`: a file's list is replaced
/// whole by each one, so the last is what the adapter holds.
fn lines_sent(world: &VardeWorld, command: &str, file: &str) -> Vec<Value> {
    let path = abs(world, file);
    let request = dap_requests(world, command)
        .into_iter()
        .rfind(|request| request["arguments"]["source"]["path"] == json!(path))
        .unwrap_or_else(|| panic!("no {command:?} for {file:?}; sent {:?}", world.dap.sent));
    request["arguments"]["breakpoints"]
        .as_array()
        .expect("a list of breakpoints")
        .iter()
        .map(|breakpoint| breakpoint["line"].clone())
        .collect()
}

#[then(expr = "the Debug adapter was sent a {string} request for {string} line {int}")]
fn adapter_sent_for_line(world: &mut VardeWorld, command: String, file: String, line: usize) {
    assert_eq!(lines_sent(world, &command, &file), vec![json!(line)]);
}

#[then(expr = "the Debug adapter was sent a {string} request for {string} with no lines")]
fn adapter_sent_no_lines(world: &mut VardeWorld, command: String, file: String) {
    assert_eq!(lines_sent(world, &command, &file), Vec::<Value>::new());
}

#[then(expr = "the Debug adapter was sent a {string} request with {string} {word}")]
fn adapter_sent_with(world: &mut VardeWorld, command: String, key: String, value: String) {
    let expected: Value = serde_json::from_str(&value).expect("a JSON value");
    assert_eq!(
        last_request(world, &command)["arguments"][key.as_str()],
        expected
    );
}

/// The hint is a menu as well as a reminder: the mouse hit-tests the rows
/// `chord_rows` names, which the unit test beside it holds, so what this
/// drives is the key the row offers.
#[when(expr = "I click the Chord hint entry for {string}")]
fn click_chord_entry(world: &mut VardeWorld, key: String) {
    let key = key.chars().next().expect("a key");
    let rows = keys::chord_rows(&world.state);
    assert!(
        rows.iter().any(|(offered, _)| *offered == Some(key)),
        "the Chord hint does not offer {key:?}: {rows:?}"
    );
    world.send(Event::ClickPaletteEntry(key));
}

/// The cheatsheet as `ui` draws it for the view on screen: the rows
/// `keys::cheatsheet` yields, which is the one list the box and the sweep both
/// read.
fn cheatsheet_lists(world: &VardeWorld, key: &str) -> bool {
    keys::cheatsheet(&world.state)
        .filter(|(_, _, views)| keys::applies_to(views, world.state.view))
        .flat_map(|(keys, _, _)| keys.split_whitespace())
        .any(|token| token == key)
}

#[then(expr = "the cheatsheet lists {string}")]
fn cheatsheet_should_list(world: &mut VardeWorld, key: String) {
    assert!(cheatsheet_lists(world, &key), "{key:?} is not listed");
}

#[then(expr = "the cheatsheet does not list {string}")]
fn cheatsheet_should_not_list(world: &mut VardeWorld, key: String) {
    assert!(!cheatsheet_lists(world, &key), "{key:?} is listed");
}

/// The hint and the cheatsheet are drawn from one list, and this is what holds
/// them to it: a key offered in the hint that the box never names is a key
/// nobody can find once the hint is down.
#[then(expr = "every key the Chord hint lists is in the cheatsheet")]
fn every_chord_hint_key_is_in_the_cheatsheet(world: &mut VardeWorld) {
    for (key, _) in keys::chord_rows(&world.state) {
        let Some(key) = key else { continue };
        let chord = format!("␣{key}");
        assert!(
            cheatsheet_lists(world, &chord),
            "the hint offers {key:?} and the cheatsheet does not name {chord:?}"
        );
    }
}

#[then(expr = "the Chord hint is shown")]
fn chord_hint_is_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.modal, Modal::Chord);
}

#[then(expr = "no Chord hint is shown")]
fn no_chord_hint_is_shown(world: &mut VardeWorld) {
    assert_ne!(world.state.modal, Modal::Chord);
}

#[then(expr = "the Chord hint lists the keys:")]
fn chord_hint_lists(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<char> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].chars().next().expect("a key"))
        .collect();
    let offered: Vec<char> = keys::chord_rows(&world.state)
        .into_iter()
        .filter_map(|(key, _)| key)
        .collect();
    assert_eq!(offered, expected);
}

#[given(expr = "Stepping mode is on")]
fn stepping_mode_on(world: &mut VardeWorld) {
    world.state.stepping = true;
}

#[then(expr = "Stepping mode is on")]
fn stepping_mode_should_be_on(world: &mut VardeWorld) {
    assert!(world.state.stepping);
}

#[then(expr = "Stepping mode is off")]
fn stepping_mode_should_be_off(world: &mut VardeWorld) {
    assert!(!world.state.stepping);
}

#[then(expr = "the Debug session is {string}")]
fn debug_session_is(world: &mut VardeWorld, phase: String) {
    let actual = match world.state.debug.as_ref().map(|session| &session.phase) {
        Some(varde::debug::Phase::Running(_)) => "running",
        Some(varde::debug::Phase::Paused(_)) => "paused",
        Some(varde::debug::Phase::Waiting) => "waiting",
        other => panic!("the session is {other:?}"),
    };
    assert_eq!(actual, phase);
}

#[then(expr = "no Debug session exists")]
fn no_debug_session_exists(world: &mut VardeWorld) {
    assert_eq!(world.state.debug, None);
}

#[then(expr = "the Paused line is {string} line {int}")]
fn paused_line_is(world: &mut VardeWorld, file: String, line: usize) {
    let file = abs(world, &file);
    assert_eq!(
        varde::debug::paused_line(&world.state).map(|(file, line, _)| (file.to_path_buf(), line)),
        Some((file, line))
    );
}

/// The gutter and the full-width wash are drawn from the same answer, and it is
/// only drawn over the Buffer on screen — which is what these two ask.
#[then(expr = "the gutter marks line {int} as the Paused line")]
#[then(expr = "line {int} is highlighted across the editor's full width")]
fn paused_line_on_screen(world: &mut VardeWorld, line: usize) {
    let (file, at, _) = varde::debug::paused_line(&world.state).expect("a Paused line");
    assert_eq!(world.state.current_buffer.as_deref(), Some(file));
    assert_eq!(at, line);
}

#[then(expr = "the Paused line is drawn as {string}")]
fn paused_line_drawn_as(world: &mut VardeWorld, kind: String) {
    let (_, _, why) = varde::debug::paused_line(&world.state).expect("a Paused line");
    assert_eq!(why.as_str(), kind);
}

#[then(expr = "no Paused line is marked")]
fn no_paused_line(world: &mut VardeWorld) {
    assert_eq!(varde::debug::paused_line(&world.state), None);
}

#[given(expr = "the Corner shows the Buffers pane")]
fn corner_shows_buffers(world: &mut VardeWorld) {
    world.state.corner = layout::Corner::Buffers;
}

#[then(expr = "the Corner shows the Buffers pane")]
fn corner_should_show_buffers(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Buffers);
}

#[given(expr = "the corner is empty")]
fn corner_is_empty(world: &mut VardeWorld) {
    world.state.corner = layout::Corner::Hidden;
}

#[then(expr = "the Corner holds the Frames")]
fn corner_holds_frames(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Frames);
}

/// Through the hit-test, on the Frame's row in the Corner.
#[when(expr = "I choose the Frame {string}")]
#[given(expr = "I choose the Frame {string}")]
fn choose_frame(world: &mut VardeWorld, name: String) {
    let frames = varde::debug::frames(&world.state);
    let index = varde::debug::frame_rows(&world.state)
        .iter()
        .position(
            |row| matches!(row, varde::debug::FrameRow::Frame(at) if frames[*at].name == name),
        )
        .unwrap_or_else(|| panic!("no Frame {name:?}"));
    let corner = world.panes().corner;
    let row = corner.y + 1 + (index - world.state.frames_scroll) as u16;
    world.report(mouse::Kind::LeftDown, corner.x + 2, row);
    world.report(mouse::Kind::LeftUp, corner.x + 2, row);
}

#[then(expr = "the buffer has unsaved edits")]
fn the_buffer_has_unsaved_edits(world: &mut VardeWorld) {
    let path = world.state.current_buffer.clone().expect("a buffer");
    assert!(world.state.buffers[&path].is_dirty());
}

#[when(expr = "the tools list is shown")]
fn tools_list_is_shown(world: &mut VardeWorld) {
    // The OS a row's install is looked up under, as `Varde starts` fixes it.
    if world.state.os.is_empty() {
        world.state.os = "macos".to_string();
    }
    world.send(Event::FallbackBinding);
    route_key(world, &palette_key("Tools").to_string(), 0);
}

// ---- F45: the Strip's Debug group, and the Variables in it ----

/// The reference the world's one scope answers its members by. A number of its
/// own, so a scenario's own references cannot collide with it.
const LOCALS: i64 = 1000;

#[given(expr = "the Strip shows the {word} group")]
fn strip_shows(world: &mut VardeWorld, group: String) {
    world.state.strip = parse_group(&group);
}

#[then(expr = "the Strip shows the Debug group")]
fn strip_should_show_debug(world: &mut VardeWorld) {
    assert_eq!(world.state.strip, layout::Group::Debug);
}

fn parse_group(name: &str) -> layout::Group {
    match name {
        "Shell" => layout::Group::Shells,
        "Debug" => layout::Group::Debug,
        other => panic!("unknown group {other:?}"),
    }
}

/// Whether a tab is drawn as the one showing — the same pairs `ui` draws from.
fn tab(world: &VardeWorld, label: &str) -> varde::Tab {
    varde::group_tabs(&world.state)
        .into_iter()
        .find(|tab| tab.group.label() == label)
        .unwrap_or_else(|| panic!("no Group tab {label:?}"))
}

#[then(expr = "the Group tab {string} is lit")]
fn group_tab_is_lit(world: &mut VardeWorld, label: String) {
    assert!(tab(world, &label).lit);
}

#[then(expr = "the Group tab {string} is not lit")]
fn group_tab_is_not_lit(world: &mut VardeWorld, label: String) {
    assert!(!tab(world, &label).lit);
}

/// Through the hit-test, on the tab's own columns of the Strip's top border —
/// the columns `layout::strip_at` names and `ui` draws into.
#[when(expr = "I click the Group tab {string}")]
fn click_group_tab(world: &mut VardeWorld, label: String) {
    let strip = world.panes().strip();
    let labels = varde::group_labels(&world.state);
    let index = varde::group_tabs(&world.state)
        .iter()
        .position(|tab| tab.group.label() == label)
        .unwrap_or_else(|| panic!("no Group tab {label:?}"));
    let column = (strip.x + strip.width - 1 - varde::layout::strip_width(&labels)
        + labels[..index]
            .iter()
            .map(|label| label.chars().count() as u16 + 1)
            .sum::<u16>())
        + 1;
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, strip.y);
    world.report(mouse::Kind::LeftUp, column, strip.y);
}

#[then(expr = "the Variables have focus")]
fn variables_have_focus(world: &mut VardeWorld) {
    assert_eq!(world.state.focus, Pane::Variables);
}

/// The title is what the Transport's border says beside its Chips.
#[then(expr = "the Variables title says {string}")]
#[then(expr = "the Transport says {string}")]
fn variables_title_says(world: &mut VardeWorld, said: String) {
    assert_eq!(varde::debug::title(&world.state), said);
}

/// What the last pause left on screen is drawn dimmed — one answer for both
/// panes, because it is one fact about the session rather than two.
/// A header row names the file, line and, where the scenario gives one, the
/// adapter's presentation hint. The thread's name is what the canned
/// `threads` answer calls it.
#[given(expr = "the Debug adapter's stack for thread {int} {string} is:")]
fn adapter_stack_is(world: &mut VardeWorld, thread: i64, _name: String, step: &Step) {
    let table = step.table().expect("table");
    let hints = table.rows[0].iter().position(|column| column == "hint");
    let stack = table.rows[1..]
        .iter()
        .map(|row| {
            let hint = hints.map_or(String::new(), |at| row[at].clone());
            (
                row[0].clone(),
                abs(world, &row[1]),
                row[2].parse().expect("a line"),
                hint,
            )
        })
        .collect();
    world.dap.stacks.insert(thread, stack);
}

/// Library frames outside the workspace, named as the scenario that unfolds
/// them expects the run to read.
#[given(
    expr = "a Debug session is Paused with {int} Library frames folded between {string} and {string}"
)]
fn paused_with_library_frames(world: &mut VardeWorld, count: usize, inner: String, outer: String) {
    let mut stack = vec![(inner, abs(world, "src/lib.rs"), 8, String::new())];
    stack.extend(
        ["call_once", "poll", "run"]
            .iter()
            .cycle()
            .take(count)
            .map(|name| {
                let file = PathBuf::from(format!("/home/me/.cargo/registry/{name}.rs"));
                (name.to_string(), file, 1, String::new())
            }),
    );
    stack.push((outer, abs(world, "src/main.rs"), 4, String::new()));
    world.dap.stacks.insert(1, stack);
    session_running(world);
    stopped_at(world, 1, "src/lib.rs", 8, "breakpoint");
}

#[given(expr = "the Frames have focus")]
fn frames_have_focus(world: &mut VardeWorld) {
    world.state.focus = Pane::Frames;
}

#[given(expr = "the Frames selection is the folded row")]
fn frames_selection_is_folded(world: &mut VardeWorld) {
    world.state.frames_selection = varde::debug::frame_rows(&world.state)
        .iter()
        .position(|row| matches!(row, varde::debug::FrameRow::Library { .. }))
        .expect("a folded row");
}

/// Each Frame under the thread whose header it follows.
#[then(expr = "the Frames are:")]
fn frames_are(world: &mut VardeWorld, step: &Step) {
    let mut thread = String::new();
    let mut shown = Vec::new();
    let frames = varde::debug::frames(&world.state);
    for row in varde::debug::frame_rows(&world.state) {
        match row {
            varde::debug::FrameRow::Thread { name, .. } => thread = name,
            varde::debug::FrameRow::Frame(index) => {
                shown.push(vec![thread.clone(), frames[index].name.clone()])
            }
            varde::debug::FrameRow::Library { .. } => {}
        }
    }
    let expected: Vec<Vec<String>> = step.table().expect("table").rows[1..].to_vec();
    assert_eq!(shown, expected);
}

/// The inspected thread's rows under its header: a Frame by its name, a
/// folded run by how many it holds.
#[then(expr = "the Frames rows are:")]
fn frames_rows_are(world: &mut VardeWorld, step: &Step) {
    let frames = varde::debug::frames(&world.state);
    let shown: Vec<Vec<String>> = varde::debug::frame_rows(&world.state)
        .into_iter()
        .filter_map(|row| match row {
            varde::debug::FrameRow::Thread { .. } => None,
            varde::debug::FrameRow::Frame(index) => {
                Some(vec!["frame".to_string(), frames[index].name.clone()])
            }
            varde::debug::FrameRow::Library { count, .. } => {
                Some(vec!["library".to_string(), count.to_string()])
            }
        })
        .collect();
    let expected: Vec<Vec<String>> = step.table().expect("table").rows[1..].to_vec();
    assert_eq!(shown, expected);
}

/// `ui` draws every folded run dimmed, so the row being one is the fact.
#[then(expr = "the folded row is drawn dimmed")]
fn folded_row_is_dimmed(world: &mut VardeWorld) {
    assert!(varde::debug::frame_rows(&world.state)
        .iter()
        .any(|row| matches!(row, varde::debug::FrameRow::Library { .. })));
}

#[then(expr = "the Frames flag thread {int} as paused")]
fn frames_flag_thread(world: &mut VardeWorld, thread: i64) {
    assert!(
        varde::debug::frame_rows(&world.state)
            .iter()
            .any(|row| matches!(
                row,
                varde::debug::FrameRow::Thread { id, paused: true, .. } if *id == thread
            )),
        "{:?}",
        varde::debug::frame_rows(&world.state)
    );
}

#[then(expr = "thread {int} is still Paused")]
fn thread_still_paused(world: &mut VardeWorld, thread: i64) {
    frames_flag_thread(world, thread);
}

#[then(expr = "the {string} Chip counts {int}")]
fn chip_counts(world: &mut VardeWorld, name: String, count: usize) {
    let glyph = chip(world, &name).glyph;
    let digits: String = glyph.chars().filter(char::is_ascii_digit).collect();
    assert_eq!(digits, count.to_string(), "{glyph:?}");
}

#[then(expr = "the Variables are drawn dimmed")]
#[then(expr = "the Frames are drawn dimmed")]
fn debug_panes_are_dimmed(world: &mut VardeWorld) {
    assert!(varde::debug::stale(&world.state));
}

fn variable_row(world: &VardeWorld, name: &str) -> varde::debug::Row {
    varde::debug::variables(&world.state)
        .into_iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no Variables row {name:?}; rows are {:?}",
                varde::debug::variables(&world.state)
                    .iter()
                    .map(|row| row.name.clone())
                    .collect::<Vec<String>>()
            )
        })
}

/// The pause driven again, so the Variables hold what a scenario has just
/// planted in the adapter: what they show is what the adapter answered at the
/// pause, and a member planted after it was answered would be a member nobody
/// asked for. What "since the pause" counts from is reset here for the same
/// reason — the requests the pause itself makes are the pause, not something
/// done in it.
fn repause(world: &mut VardeWorld) {
    let stop = world.dap.stopped.clone().expect("a pause");
    adapter_event(world, stop);
    world.dap.since_pause = world.dap.sent.len();
}

/// A member the adapter answers with, in the one scope the world gives a
/// Frame that has none of its own.
fn plant_member(world: &mut VardeWorld, member: Value) {
    if world.dap.scopes.is_empty() {
        world.dap.scopes.push(("Locals".to_string(), LOCALS));
    }
    let held = world.dap.members.entry(LOCALS).or_default();
    // Planted over whatever the pause already had by that name, never beside
    // it: two rows with one name is a scenario acting on whichever the walk
    // reached first.
    held.retain(|already| already["name"] != member["name"]);
    held.push(member);
    repause(world);
}

/// The locals of the program every debug Scenario's Background opens on. A
/// pause has to show something, and a scenario that says "the row `count`"
/// without planting one means the program's own — the same two names its code
/// declares.
fn plant_the_programs_locals(world: &mut VardeWorld) {
    if !world.dap.scopes.is_empty() {
        return;
    }
    world.dap.scopes.push(("Locals".to_string(), LOCALS));
    world.dap.members.entry(LOCALS).or_default().extend([
        json!({ "name": "orders", "value": "[…]" }),
        json!({ "name": "count", "value": "3" }),
    ]);
}

#[given(expr = "the Variables show {string}")]
#[when(expr = "the Variables show {string}")]
fn variables_show(world: &mut VardeWorld, name: String) {
    plant_member(world, json!({ "name": name, "value": "3" }));
}

#[given(expr = "the Variables show {string} with the value {string}")]
fn variables_show_value(world: &mut VardeWorld, name: String, value: String) {
    plant_member(world, json!({ "name": name, "value": value }));
}

#[then(expr = "the Variables show {string}")]
fn variables_should_show(world: &mut VardeWorld, name: String) {
    variable_row(world, &name);
}

#[given(expr = "the Variables show {string} with reference {int}")]
#[when(expr = "the Variables show {string} with reference {int}")]
fn variables_show_reference(world: &mut VardeWorld, name: String, reference: i64) {
    plant_member(
        world,
        json!({ "name": name, "value": "[…]", "variablesReference": reference }),
    );
}

#[given(expr = "the Variables show {string} with reference {int} holding {int} indexed children")]
fn variables_show_collection(
    world: &mut VardeWorld,
    name: String,
    reference: i64,
    children: usize,
) {
    world.dap.members.insert(
        reference,
        (0..children)
            .map(|index| json!({ "name": format!("[{index}]"), "value": "0" }))
            .collect(),
    );
    plant_member(
        world,
        json!({
            "name": name,
            "value": "[…]",
            "variablesReference": reference,
            "indexedVariables": children,
        }),
    );
}

#[given(expr = "the Variables show {string} with the presentation hint {string}")]
#[when(expr = "the Variables show {string} with the presentation hint {string}")]
fn variables_show_hint(world: &mut VardeWorld, name: String, hint: String) {
    plant_member(world, hinted(&name, 0, &hint));
}

#[given(
    expr = "the Variables show {string} with reference {int} and the presentation hint {string}"
)]
fn variables_show_reference_hint(
    world: &mut VardeWorld,
    name: String,
    reference: i64,
    hint: String,
) {
    plant_member(world, hinted(&name, reference, &hint));
}

/// One member carrying the adapter's hint in the field the protocol puts it
/// in: visibility for who may see it, attributes for what may be done to it.
fn hinted(name: &str, reference: i64, hint: &str) -> Value {
    let presentation = match hint {
        "private" => json!({ "visibility": "private" }),
        attribute => json!({ "attributes": [attribute] }),
    };
    json!({
        "name": name,
        "value": "…",
        "variablesReference": reference,
        "presentationHint": presentation,
    })
}

#[when(expr = "the Debug adapter answers {string} with the scopes:")]
fn adapter_answers_scopes(world: &mut VardeWorld, command: String, step: &Step) {
    assert_eq!(command, "scopes");
    world.dap.scopes = step
        .table()
        .expect("table")
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row[0].clone(), LOCALS + index as i64))
        .collect();
    repause(world);
}

#[then(expr = "the Variables' top rows are:")]
fn variables_top_rows_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let tops: Vec<String> = varde::debug::variables(&world.state)
        .into_iter()
        .filter(|row| row.depth == 0)
        .map(|row| row.name)
        .collect();
    assert_eq!(tops, expected);
}

#[then(expr = "the Variables row {string} is drawn as {string}")]
fn variables_row_drawn_as(world: &mut VardeWorld, name: String, drawn: String) {
    assert_eq!(variable_row(world, &name).hint.as_str(), drawn);
}

/// Through the hit-test, on the row's own line of the Strip — the gesture
/// Enter is, which is why opening by mouse and by key are one arm.
fn open_variables_row(world: &mut VardeWorld, index: usize) {
    let strip = world.panes().terminal;
    let row = strip.y + 1 + (index - world.state.variables_scroll) as u16;
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, strip.x + 2, row);
    world.report(mouse::Kind::LeftUp, strip.x + 2, row);
}

#[given(expr = "I open the Variables row {string}")]
#[when(expr = "I open the Variables row {string}")]
fn open_row_named(world: &mut VardeWorld, name: String) {
    let index = varde::debug::variables(&world.state)
        .iter()
        .position(|row| row.name == name)
        .unwrap_or_else(|| panic!("no Variables row {name:?}"));
    open_variables_row(world, index);
}

/// The row that stands for the rest of `name`'s children: the one whose next
/// page is that member's reference.
fn next_page_row(world: &VardeWorld, name: &str) -> Option<usize> {
    let reference = match variable_row(world, name).opens {
        varde::debug::Opens::Children { reference, .. } => reference,
        other => panic!("the row {name:?} opens {other:?}"),
    };
    varde::debug::variables(&world.state).iter().position(|row| {
        matches!(row.opens, varde::debug::Opens::NextPage { reference: next, .. } if next == reference)
    })
}

#[then(expr = "the Variables show a row for the next page of {string}")]
fn variables_show_next_page(world: &mut VardeWorld, name: String) {
    assert!(
        next_page_row(world, &name).is_some(),
        "no next-page row for {name:?}"
    );
}

/// By keyboard rather than by pointer: the row that stands for the rest of a
/// ten thousand element collection is a hundred rows past the Strip's last
/// line, and a row nothing has scrolled to is a row no pointer can reach. The
/// arrows are what scroll it into view, and Enter is what every list in Varde
/// opens a row with.
#[when(expr = "I open the next page of {string}")]
fn open_next_page(world: &mut VardeWorld, name: String) {
    let index = next_page_row(world, &name).unwrap_or_else(|| panic!("no next page of {name:?}"));
    world.state.focus = Pane::Variables;
    while world.state.variables_selection < index {
        world.send(Event::MoveSelection(Direction::Down));
    }
    world.send(Event::Activate);
}

/// The `variables` requests that named a reference, in order.
fn for_reference<'a>(world: &'a VardeWorld, command: &str, reference: i64) -> Vec<&'a Value> {
    dap_requests(world, command)
        .into_iter()
        .filter(|message| message["arguments"]["variablesReference"] == reference)
        .collect()
}

#[then(expr = "the Debug adapter was sent a {string} request for reference {int}")]
fn adapter_sent_for_reference(world: &mut VardeWorld, command: String, reference: i64) {
    assert!(
        !for_reference(world, &command, reference).is_empty(),
        "{:?}",
        world.dap.sent
    );
}

#[then(expr = "the Debug adapter was sent no {string} request for reference {int}")]
fn adapter_sent_none_for_reference(world: &mut VardeWorld, command: String, reference: i64) {
    assert!(
        for_reference(world, &command, reference).is_empty(),
        "{:?}",
        world.dap.sent
    );
}

#[then(
    expr = "the Debug adapter was sent a {string} request for reference {int} starting at {int} counting {int}"
)]
fn adapter_sent_page(
    world: &mut VardeWorld,
    command: String,
    reference: i64,
    start: u64,
    count: u64,
) {
    let asked = for_reference(world, &command, reference);
    assert!(
        asked.iter().any(|message| {
            message["arguments"]["start"] == start && message["arguments"]["count"] == count
        }),
        "{asked:?}"
    );
}

#[then(expr = "the Debug adapter was sent exactly {int} {string} request(s) since the pause")]
fn adapter_sent_exactly_since(world: &mut VardeWorld, count: usize, command: String) {
    let sent: Vec<&Value> = world.dap.sent[world.dap.since_pause..]
        .iter()
        .filter(|message| message["type"] == "request" && message["command"] == command)
        .collect();
    assert_eq!(sent.len(), count, "{sent:?}");
}

#[then(expr = "the Debug adapter was sent a {string} request for the Frame {string}")]
fn adapter_sent_for_frame(world: &mut VardeWorld, command: String, name: String) {
    let frame = varde::debug::frames(&world.state)
        .iter()
        .find(|frame| frame.name == name)
        .unwrap_or_else(|| panic!("no Frame {name:?}"));
    assert_eq!(
        last_request(world, &command)["arguments"]["frameId"],
        frame.id
    );
}

#[given(
    expr = "the Debug adapter sends the {string} event for thread {int} at {string} line {int} with reason {string} and the text {string}"
)]
#[when(
    expr = "the Debug adapter sends the {string} event for thread {int} at {string} line {int} with reason {string} and the text {string}"
)]
fn adapter_sends_stopped_with_text(
    world: &mut VardeWorld,
    event: String,
    thread: i64,
    file: String,
    line: usize,
    reason: String,
    text: String,
) {
    assert_eq!(event, "stopped");
    world.dap.text = Some(text);
    stopped_at(world, thread, &file, line, &reason);
}

#[then(expr = "the first Variables row is the exception {string}")]
fn first_variables_row_is_the_exception(world: &mut VardeWorld, text: String) {
    let first = varde::debug::variables(&world.state)
        .into_iter()
        .next()
        .expect("a row");
    assert_eq!((first.name.as_str(), first.value), ("exception", text));
}

#[then(expr = "the Debug adapter row for {string} is {string}")]
fn debug_adapter_row_is(world: &mut VardeWorld, language: String, expected: String) {
    assert_eq!(
        tool_row(world, tools::Kind::Adapter, &language)
            .availability
            .as_str(),
        expected
    );
}

// ---- R41.7: child sessions are more threads ----

/// `startDebugging`, as js-debug sends it for a worker: a configuration of
/// its own, which the child session is started with.
#[given(expr = "the Debug adapter asks to start a child session whose thread is {string}")]
#[when(expr = "the Debug adapter asks to start a child session whose thread is {string}")]
fn adapter_asks_for_a_child(world: &mut VardeWorld, thread: String) {
    world.dap.child_thread = thread;
    let seq = 1000 + world.dap.sent.len();
    adapter_event(
        world,
        json!({
            "type": "request",
            "seq": seq,
            "command": "startDebugging",
            "arguments": {
                "request": "attach",
                "configuration": { "type": "pwa-node", "name": "worker.js", "__pendingTargetId": "7" },
            },
        }),
    );
}

#[then(expr = "the Frames list the thread {string}")]
fn frames_list_the_thread(world: &mut VardeWorld, thread: String) {
    let listed = varde::debug::frame_rows(&world.state)
        .into_iter()
        .any(|row| matches!(row, varde::debug::FrameRow::Thread { name, .. } if name == thread));
    assert!(listed, "{:?}", varde::debug::frame_rows(&world.state));
}

/// One session, which is the one started: a child opened as a session of its
/// own would have its adapter started again, or the session replaced by it.
#[then(expr = "exactly one Debug session is shown")]
fn exactly_one_session(world: &mut VardeWorld) {
    assert!(world.state.debug.is_some());
    assert_eq!(world.dap.spawned.len() + world.dap.dialed.len(), 1);
    assert_eq!(dap_requests(world, "launch").len(), 1);
}

#[then(expr = "no session picker is shown")]
fn no_session_picker(world: &mut VardeWorld) {
    assert_eq!(world.state.modal, Modal::None);
}

#[then(expr = "the Transport has one {string} Chip")]
fn transport_has_one_chip(world: &mut VardeWorld, name: String) {
    let chips = varde::debug::strip_transport(&world.state);
    assert_eq!(chips.iter().filter(|chip| chip.name == name).count(), 1);
}

/// In the id the child knows its thread by, which is the session's own
/// thread's too: only the connection tells them apart.
#[then(expr = "the child session was sent a {string} request for its thread")]
fn child_was_sent_for_its_thread(world: &mut VardeWorld, command: String) {
    let (child, sent) = world.dap.children.iter().next().expect("a child session");
    let asked = sent
        .iter()
        .rev()
        .find(|message| message["command"] == command)
        .unwrap_or_else(|| panic!("child {child} was sent {sent:?}"));
    assert_eq!(asked["arguments"]["threadId"], 1);
}

#[then(expr = "every child session was sent a {string} request")]
fn every_child_was_sent(world: &mut VardeWorld, command: String) {
    assert!(
        !world.dap.children.is_empty(),
        "no child session was opened"
    );
    for (child, sent) in &world.dap.children {
        assert!(
            sent.iter().any(|message| message["command"] == command),
            "child {child} was sent {sent:?}"
        );
    }
}

// ---- F45: the Program output in the Debug group ----

/// The adapter's reverse request, driven in exactly as the edge frames it.
#[given(expr = "the Debug adapter asks to run {string} in a terminal")]
#[when(expr = "the Debug adapter asks to run {string} in a terminal")]
fn adapter_asks_for_a_terminal(world: &mut VardeWorld, program: String) {
    // A `seq` of the adapter's own, past anything Varde has sent: a reverse
    // request is numbered in the adapter's sequence, not in Varde's.
    let seq = 1000 + world.dap.sent.len() as i64;
    adapter_event(
        world,
        json!({
            "type": "request",
            "seq": seq,
            "command": "runInTerminal",
            "arguments": { "kind": "integrated", "args": [program] },
        }),
    );
}

/// A session with the debugged program already running in the Debug group,
/// which is what every step about the Program output needs behind it.
fn program_running(world: &mut VardeWorld) {
    if world.output_pty.is_none() {
        adapter_asks_for_a_terminal(world, "target/debug/server".to_string());
    }
}

#[then(expr = "a Program output pty is asked for running {string}")]
fn program_pty_asked_for(world: &mut VardeWorld, program: String) {
    assert_eq!(world.program, vec![vec![program]]);
    assert!(
        world.output_pty.is_some(),
        "the edge holds no Program output"
    );
}

#[then(expr = "no shell is asked for")]
fn no_shell_asked_for(world: &mut VardeWorld) {
    assert_eq!(world.splits, Vec::<usize>::new());
    assert_eq!(world.state.terminals.len(), 1);
}

/// What is in the Debug group, whether or not it is the group on screen — so
/// the group is brought forward, which is where a reader looking would be.
#[then(expr = "the Debug group holds the Variables and the Program output")]
fn debug_group_holds_both(world: &mut VardeWorld) {
    program_running(world);
    world.state.strip = layout::Group::Debug;
    world.tell_core();
    let panes = world.panes();
    assert_eq!(
        layout::pane_at(&panes, panes.terminal.x + 1, panes.terminal.y + 1),
        Some(Pane::Variables)
    );
    assert_eq!(
        layout::pane_at(&panes, panes.output.x + 1, panes.output.y + 1),
        Some(Pane::Output)
    );
}

/// The border's column, as a width off the Strip's right-hand end — which is
/// what the layout is told, since the Strip's own left edge moves with the
/// Corner beside it.
#[given(expr = "the border between the Variables and the Program output is at column {int}")]
fn border_at_column(world: &mut VardeWorld, column: u16) {
    program_running(world);
    let right = debug_group_right(world);
    world.state.output_width = Some(u32::from(right.saturating_sub(column)));
    world.tell_core();
}

/// The Debug group's right-hand edge, which the border is a width back from.
/// Off the Strip when the Program output is away and off the output when it is
/// not: the group is the two rectangles together.
fn debug_group_right(world: &VardeWorld) -> u16 {
    let panes = world.panes();
    panes.terminal.right().max(panes.output.right())
}

/// Through the hit-test, from the border column itself: the handle has to be
/// one a pointer can reach.
#[when(expr = "I drag that border to column {int}")]
#[when(expr = "I drag the border between the Variables and the Program output to column {int}")]
fn drag_output_border(world: &mut VardeWorld, column: u16) {
    program_running(world);
    let panes = world.panes();
    let row = panes.output.y + 1;
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, panes.output.x, row);
    world.report(mouse::Kind::LeftDrag, column, row);
    world.report(mouse::Kind::LeftUp, column, row);
}

#[given(expr = "the Program output is {int} columns wide")]
fn output_is_wide(world: &mut VardeWorld, columns: u16) {
    program_running(world);
    let column = debug_group_right(world) - columns;
    border_at_column(world, column);
}

#[then(expr = "the Program output is {int} columns wide")]
fn output_should_be_wide(world: &mut VardeWorld, columns: u16) {
    assert_eq!(world.panes().output.width, columns);
}

#[then(expr = "the Variables are at their least width")]
fn variables_at_least_width(world: &mut VardeWorld) {
    assert_eq!(world.panes().terminal.width, layout::GROUP_LEAST);
}

#[then(expr = "the Variables have the Debug group's whole width")]
fn variables_have_the_whole_width(world: &mut VardeWorld) {
    let panes = world.panes();
    assert_eq!(panes.output.width, 0);
    assert_eq!(panes.terminal.right(), panes.ai.right());
}

#[given(expr = "the Program output is hidden")]
fn output_is_hidden(world: &mut VardeWorld) {
    program_running(world);
    if !world.state.output_hidden {
        world.send(Event::ToggleOutput);
    }
}

#[then(expr = "the Program output is hidden")]
fn output_should_be_hidden(world: &mut VardeWorld) {
    assert!(world.state.output_hidden);
    assert_eq!(world.panes().output.width, 0);
}

#[then(expr = "the Program output is shown")]
fn output_should_be_shown(world: &mut VardeWorld) {
    assert!(varde::showing_output(&world.state));
    assert!(world.panes().output.width > 0);
}

#[given(expr = "the Program output has focus")]
fn output_has_focus(world: &mut VardeWorld) {
    program_running(world);
    world.state.focus = Pane::Output;
}

#[given(expr = "the Variables have focus")]
fn variables_take_focus(world: &mut VardeWorld) {
    world.state.focus = Pane::Variables;
}

/// Everything the program prints reaches the core the same way: the edge says
/// it spoke, and the core decides whether the reader could see it.
#[when(expr = "the program prints {string}")]
fn program_prints(world: &mut VardeWorld, text: String) {
    program_running(world);
    world.send(Event::OutputSpoke);
    // The adapter's own copy of the same output, which is what a run in the
    // Evaluator collects: the pty prints it and the adapter repeats it.
    if world.state.debug.is_some() {
        adapter_event(
            world,
            json!({
                "type": "event",
                "event": "output",
                "body": { "category": "stdout", "output": format!("{text}\n") },
            }),
        );
    }
}

#[given(expr = "the program prints {string}")]
fn program_printed(world: &mut VardeWorld, text: String) {
    program_prints(world, text);
}

#[then(expr = "the Group tab {string} is marked as having unseen output")]
fn tab_is_marked(world: &mut VardeWorld, label: String) {
    assert!(tab(world, &label).unseen);
}

#[then(expr = "the Group tab {string} is not marked as having unseen output")]
fn tab_is_not_marked(world: &mut VardeWorld, label: String) {
    assert!(!tab(world, &label).unseen);
}

#[then(expr = "the {string} Chip is marked as having unseen output")]
fn chip_is_marked(world: &mut VardeWorld, chip: String) {
    let offered = varde::debug::strip_transport(&world.state)
        .into_iter()
        .find(|offered| offered.name == chip)
        .unwrap_or_else(|| panic!("no {chip:?} Chip"));
    assert_eq!(offered.tone, varde::Tone::Marked);
}

#[then(expr = "the Program output pty was never stopped")]
fn output_pty_never_stopped(world: &mut VardeWorld) {
    assert!(world.output_pty.is_some());
    assert_eq!(world.program.len(), 1, "the program was started again");
}

#[then(expr = "the Program output's pty is at least {int} rows by {int} column")]
fn output_pty_is_at_least(world: &mut VardeWorld, rows: u16, columns: u16) {
    let (held_rows, held_columns) = world.output_pty.expect("a Program output pty");
    assert!(
        held_rows >= rows && held_columns >= columns,
        "{held_rows}x{held_columns}"
    );
}

#[then(expr = "the Program output received the key {string}")]
fn output_received_key(world: &mut VardeWorld, key: String) {
    let expected: &[u8] = match key.as_str() {
        "Space" => b" ",
        other => panic!("no bytes are pinned for {other:?}"),
    };
    assert_eq!(
        world.keys_sent,
        vec![(Pane::Output, expected.to_vec())],
        "keys sent: {:?}",
        world.keys_sent
    );
}

#[then(expr = "nothing reached the Program output")]
fn nothing_reached_the_output(world: &mut VardeWorld) {
    assert!(
        !world
            .keys_sent
            .iter()
            .any(|(pane, _)| *pane == Pane::Output),
        "keys sent: {:?}",
        world.keys_sent
    );
}

// ---- #56: the debug Transport ----

/// The spawn `debug::start` asked the edge for, named by the command the
/// adapter row gives — which is what a scenario means by "asked for": whether
/// a process exists is the edge's to say.
#[then(expr = "the Debug adapter for {string} is asked for")]
#[then(expr = "a Debug adapter for {string} is asked for")]
fn adapter_is_asked_for(world: &mut VardeWorld, language: String) {
    let command = world
        .state
        .adapters
        .get(&language)
        .unwrap_or_else(|| panic!("no adapter configured for {language:?}"))
        .command
        .clone();
    assert!(
        world.dap.spawned.contains(&command),
        "spawned: {:?}",
        world.dap.spawned
    );
}

#[then("no Debug adapter is asked for")]
fn no_adapter_is_asked_for(world: &mut VardeWorld) {
    assert_eq!(world.dap.spawned, Vec::<String>::new());
}

/// The program ended, which is the state restart is reached for from.
#[given("the Debug session has ended")]
#[when("the Debug session has ended")]
fn session_has_ended(world: &mut VardeWorld) {
    adapter_sends_event(world, "terminated".to_string());
    assert!(world.state.debug.is_none(), "the session is still going");
    world.dap.spawned.clear();
}

/// What the named Launch configuration carries, as configuration gave it —
/// so a restart that reran a different one, or reached the adapter with
/// arguments of its own, fails here.
#[then(expr = "the Debug adapter's launch arguments are those of {string}")]
fn launch_arguments_are_those_of(world: &mut VardeWorld, name: String) {
    let expected = world
        .state
        .launches
        .get(&name)
        .unwrap_or_else(|| panic!("no Launch configuration {name:?}"))
        .args
        .clone();
    assert_eq!(
        last_request(world, "launch")["arguments"],
        Value::Object(expected)
    );
}

/// The Variables' Transport, by the names that say what pressing each Chip
/// does — never the glyphs or the colours, which are `ui`'s.
#[then("the Transport's Chips are:")]
fn transport_chips_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a table of Chips")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let drawn: Vec<&str> = varde::debug::strip_transport(&world.state)
        .iter()
        .map(|chip| chip.name)
        .collect();
    assert_eq!(drawn, expected);
}

#[then(expr = "the Transport's first Chip is {string}")]
fn transport_first_chip_is(world: &mut VardeWorld, name: String) {
    let chips = varde::debug::strip_transport(&world.state);
    assert_eq!(chips.first().expect("a Chip").name, name);
}

/// Every key the Chip teaches, in the spelling the cheatsheet uses: a Chip
/// that named one of its two routes would say the other is not there.
#[then(expr = "the {string} Chip names the keys:")]
fn chip_names_the_keys(world: &mut VardeWorld, name: String, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a table of keys")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let named: Vec<String> = chip(world, &name)
        .keys
        .split_whitespace()
        .map(str::to_string)
        .collect();
    assert_eq!(named, expected);
}

/// The Chip that says what pressing it does, found by that name — on the
/// Variables' Transport, the Breakpoint list's, or on the row the keyboard is
/// on, since all are Chips and a scenario names one by what pressing it does.
fn chip(world: &VardeWorld, name: &str) -> varde::Chip {
    varde::debug::strip_transport(&world.state)
        .into_iter()
        .chain(varde::debug::transport(&world.state))
        .find(|offered| offered.name == name)
        .or_else(|| row_chip(world, name))
        .unwrap_or_else(|| panic!("no {name:?} Chip"))
}

/// The named Chip on the Variables row the keyboard is on, if it carries one.
fn row_chip(world: &VardeWorld, name: &str) -> Option<varde::Chip> {
    varde::debug::row_chips(&world.state, world.state.variables_selection)
        .into_iter()
        .find(|offered| offered.name == name)
}

#[when(expr = "I click the row's {string} Chip")]
fn click_the_rows_chip(world: &mut VardeWorld, name: String) {
    click_variables_row_chip(world, &name);
}

/// Through the hit-test, on the row's own line and at the columns `ui` draws
/// the Chips into — never driven as an event, so a Chip no pointer could
/// reach fails here.
fn click_variables_row_chip(world: &mut VardeWorld, name: &str) {
    let index = world.state.variables_selection;
    let chips = varde::debug::row_chips(&world.state, index);
    let at = chips
        .iter()
        .position(|offered| offered.name == name)
        .unwrap_or_else(|| panic!("no {name:?} Chip on the row"));
    let strip = world.panes().terminal;
    let row = strip.y + 1 + (index - world.state.variables_scroll) as u16;
    let column = strip.x + strip.width - 1 - 2 * (chips.len() - at) as u16;
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, row);
    world.report(mouse::Kind::LeftUp, column, row);
}

#[then(expr = "the {string} Chip is dimmed")]
fn chip_is_dimmed(world: &mut VardeWorld, name: String) {
    assert_eq!(chip(world, &name).tone, varde::Tone::Dimmed);
}

#[then(expr = "the {string} Chip is not dimmed")]
fn chip_is_not_dimmed(world: &mut VardeWorld, name: String) {
    assert_ne!(chip(world, &name).tone, varde::Tone::Dimmed);
}

#[then(expr = "the {string} Chip is lit")]
fn chip_is_lit(world: &mut VardeWorld, name: String) {
    assert_eq!(chip(world, &name).tone, varde::Tone::Lit);
}

#[then(expr = "the {string} Chip is not lit")]
fn chip_is_not_lit(world: &mut VardeWorld, name: String) {
    assert_ne!(chip(world, &name).tone, varde::Tone::Lit);
}

/// Time passing is the ticks that would have arrived — the one event nobody
/// pressed, on `main`'s own 80ms cadence. Nothing else can pass here, which
/// is the assertion: a lit Chip that faded would need a timer, and ADR 0009
/// allows a Tick only while work is in flight.
#[when(expr = "{int} seconds pass")]
fn seconds_pass(world: &mut VardeWorld, seconds: u64) {
    for _ in 0..(seconds * 1_000 / 80) {
        world.send(Event::Tick);
    }
}

/// The Transport is drawn across the Strip's top border, so the Variables'
/// width is the room its Chips have. Driven by the screen rather than set: the
/// panes are the layout's to decide, and a width written into `State` would be
/// a width nothing draws.
#[when(expr = "the Variables are {int} columns wide")]
fn variables_are_wide(world: &mut VardeWorld, columns: u16) {
    let (width, height) = world.screen();
    let wanted = width + columns - world.panes().terminal.width;
    world.send(Event::Resized {
        width: wanted,
        height,
    });
    assert_eq!(world.panes().terminal.width, columns);
}

/// The labels `ui` draws and `mouse` hit-tests, at the width the Transport
/// really has.
fn transport_labels(world: &VardeWorld) -> Vec<String> {
    let chips = varde::debug::strip_transport(&world.state);
    let area = varde::transport_area(&world.state, world.panes().strip());
    layout::chip_labels(&chips, area.width, layout::CORNER_TITLE)
}

/// Each label pinned exactly, never "contains its keys": a Chip that names no
/// key yet would pass a containment test whichever shape it was drawn in, and
/// a `Then` that cannot fail is worse than no `Then`.
#[then("every Chip shows its keys")]
fn every_chip_shows_its_keys(world: &mut VardeWorld) {
    let chips = varde::debug::strip_transport(&world.state);
    for (chip, label) in chips.iter().zip(transport_labels(world)) {
        assert_eq!(
            label,
            format!(" {} {} ", chip.glyph, chip.keys),
            "{:?}",
            chip.name
        );
    }
}

#[then("no Chip shows its keys")]
fn no_chip_shows_its_keys(world: &mut VardeWorld) {
    let chips = varde::debug::strip_transport(&world.state);
    for (chip, label) in chips.iter().zip(transport_labels(world)) {
        assert_eq!(label, format!(" {} ", chip.glyph), "{:?}", chip.name);
    }
}

/// None dropped, none cut and none wrapped: every Chip still has a label, and
/// each one holds that Chip's whole glyph and nothing of another's.
#[then("every Chip is drawn whole")]
fn every_chip_is_drawn_whole(world: &mut VardeWorld) {
    let chips = varde::debug::strip_transport(&world.state);
    let labels = transport_labels(world);
    assert_eq!(labels.len(), chips.len());
    for (chip, label) in chips.iter().zip(&labels) {
        assert!(
            label.starts_with(&format!(" {} ", chip.glyph)),
            "{:?} is drawn {label:?}",
            chip.name
        );
    }
}

// ---- #57: the Variables' row Chips, and the Watches ----

/// The row a scenario names, by the expression that reaches it — `orders[0].id`
/// rather than the `id` the row is labelled with — falling back to the label,
/// which is how a scope and the exception row are named.
fn variables_index(world: &VardeWorld, named: &str) -> usize {
    let rows = varde::debug::variables(&world.state);
    rows.iter()
        .position(|row| row.expression == named)
        .or_else(|| rows.iter().position(|row| row.name == named))
        .unwrap_or_else(|| {
            panic!(
                "no Variables row {named:?}; rows are {:?}",
                rows.iter()
                    .map(|row| row.expression.clone())
                    .collect::<Vec<String>>()
            )
        })
}

/// By the arrows, from wherever the selection is — the gesture a reader makes,
/// so a row the motion cannot reach fails here rather than being set into
/// place behind the keyboard's back.
#[given(expr = "the Variables selection is the row {string}")]
#[given(expr = "the Variables selection is the Watch {string}")]
#[when(expr = "I move the Variables selection to the row {string}")]
fn variables_selection_is(world: &mut VardeWorld, named: String) {
    let index = variables_index(world, &named);
    world.state.focus = Pane::Variables;
    while world.state.variables_selection > index {
        world.send(Event::MoveSelection(Direction::Up));
    }
    while world.state.variables_selection < index {
        world.send(Event::MoveSelection(Direction::Down));
    }
    assert_eq!(world.state.variables_selection, index);
}

/// The row's Chips by the names that say what pressing each does, never the
/// glyphs or the colours, exactly as the Transport's are read.
#[then(expr = "the row {string} carries the Chips:")]
fn row_carries_chips(world: &mut VardeWorld, named: String, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a table of Chips")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let index = variables_index(world, &named);
    let drawn: Vec<&str> = varde::debug::row_chips(&world.state, index)
        .iter()
        .map(|chip| chip.name)
        .collect();
    assert_eq!(drawn, expected);
}

#[then(expr = "the row {string} carries no Chips")]
fn row_carries_no_chips(world: &mut VardeWorld, named: String) {
    let index = variables_index(world, &named);
    assert_eq!(varde::debug::row_chips(&world.state, index), Vec::new());
}

/// The adapter's own word about itself, as the `capabilities` event carries
/// it — never assumed, and never found out by trying the request.
#[given(expr = "the Debug adapter reported it can set variables")]
#[given(expr = "the Debug adapter reported it cannot set variables")]
fn adapter_reported_set_variables(world: &mut VardeWorld, step: &Step) {
    let can = !step.value.contains("cannot");
    adapter_event(
        world,
        json!({
            "type": "event",
            "event": "capabilities",
            "body": { "capabilities": { "supportsSetVariable": can } },
        }),
    );
}

/// Through the box the set-value Chip opens: the key, the characters, Enter —
/// the gesture a reader makes, so a box that never opened fails here.
#[given(expr = "I set the value of the row to {string}")]
#[when(expr = "I set the value of the row to {string}")]
fn set_the_value_of_the_row(world: &mut VardeWorld, value: String) {
    world.state.focus = Pane::Variables;
    route_key(world, "s", 0);
    assert_eq!(
        world.state.modal,
        Modal::SetValue,
        "the set-value box did not open"
    );
    for character in value.chars() {
        route_key(world, &character.to_string(), 0);
    }
    route_key(world, "Enter", 0);
}

#[then(expr = "the Debug adapter was sent a {string} request for {string} with the value {string}")]
fn adapter_sent_set_variable(world: &mut VardeWorld, command: String, name: String, value: String) {
    let request = last_request(world, &command);
    assert_eq!(request["arguments"]["name"], json!(name));
    assert_eq!(request["arguments"]["value"], json!(value));
}

#[when(expr = "the Debug adapter answers {string} with the value {string}")]
fn adapter_answers_with_value(world: &mut VardeWorld, command: String, value: String) {
    let seq = last_request(world, &command)["seq"].clone();
    adapter_event(
        world,
        json!({
            "type": "response",
            "request_seq": seq,
            "success": true,
            "command": command,
            "body": { "value": value },
        }),
    );
}

#[given(expr = "the Variables row {string} shows the value {string}")]
fn variables_row_shows_value(world: &mut VardeWorld, name: String, value: String) {
    plant_member(world, json!({ "name": name, "value": value }));
}

#[then(expr = "the Variables row {string} shows the value {string}")]
fn variables_row_should_show_value(world: &mut VardeWorld, named: String, value: String) {
    let index = variables_index(world, &named);
    assert_eq!(varde::debug::variables(&world.state)[index].value, value);
}

/// A nested row planted and walked open, a level at a time — the members
/// above it have to exist and be open for the row to be drawn at all.
#[given(expr = "the Variables row {string} is open")]
fn variables_row_is_open(world: &mut VardeWorld, path: String) {
    let names = expression_parts(&path);
    let mut reference = LOCALS;
    for (depth, name) in names.iter().enumerate() {
        let leaf = depth + 1 == names.len();
        let child = match leaf {
            true => 0,
            false => 7000 + depth as i64,
        };
        let member = json!({ "name": name, "value": "…", "variablesReference": child });
        match depth {
            0 => plant_member(world, member),
            _ => {
                world.dap.members.entry(reference).or_default().push(member);
                repause(world);
            }
        }
        reference = child;
    }
    for depth in 1..names.len() {
        let so_far = names[..depth].iter().fold(String::new(), |path, name| {
            match (path.is_empty(), name.starts_with('[')) {
                (true, _) => name.to_string(),
                (false, true) => format!("{path}{name}"),
                (false, false) => format!("{path}.{name}"),
            }
        });
        let index = variables_index(world, &so_far);
        open_variables_row(world, index);
    }
}

/// An expression split into the members it walks through: `orders[0].id` is
/// three of them, and an index is a member of its own.
fn expression_parts(path: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut held = String::new();
    for character in path.chars() {
        match character {
            '.' => {
                if !held.is_empty() {
                    parts.push(std::mem::take(&mut held));
                }
            }
            '[' => {
                if !held.is_empty() {
                    parts.push(std::mem::take(&mut held));
                }
                held.push('[');
            }
            other => held.push(other),
        }
    }
    if !held.is_empty() {
        parts.push(held);
    }
    parts
}

#[when(expr = "I copy the row as an expression")]
fn copy_the_row_as_an_expression(world: &mut VardeWorld) {
    world.state.focus = Pane::Variables;
    route_key(world, "Y", 0);
}

/// Through the box `a` opens on the Variables, typed and entered — the other
/// half of "added from a row or typed", and the gesture a reader makes, so a
/// box that never opened fails here.
#[given(expr = "the Watches are {string}")]
#[given(expr = "the Watches are {string} and {string}")]
fn the_watches_are(world: &mut VardeWorld, step: &Step) {
    world.state.focus = Pane::Variables;
    for expression in step
        .value
        .trim_start_matches("Given ")
        .trim_start_matches("the Watches are ")
        .split(" and ")
    {
        route_key(world, "a", 0);
        assert_eq!(
            world.state.modal,
            Modal::NewWatch,
            "the Watch box did not open"
        );
        for character in expression.trim_matches('"').chars() {
            route_key(world, &character.to_string(), 0);
        }
        route_key(world, "Enter", 0);
    }
}

#[then("the Watches are:")]
fn the_watches_should_be(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a table of Watches")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let held: Vec<String> = world
        .state
        .watches
        .iter()
        .map(|watch| watch.expression.clone())
        .collect();
    assert_eq!(held, expected);
}

#[then("the Variables' first rows are the Watches")]
fn variables_first_rows_are_the_watches(world: &mut VardeWorld) {
    let expected: Vec<String> = world
        .state
        .watches
        .iter()
        .map(|watch| watch.expression.clone())
        .collect();
    assert!(!expected.is_empty(), "no Watches to be the first rows");
    let rows = varde::debug::variables(&world.state);
    let first: Vec<String> = rows
        .iter()
        .take(expected.len())
        .map(|row| row.expression.clone())
        .collect();
    assert_eq!(first, expected);
    assert!(
        rows.iter()
            .take(expected.len())
            .all(|row| matches!(row.of, varde::debug::Of::Watch { .. })),
        "a first row is not a Watch"
    );
}

#[then(
    expr = "the Debug adapter was sent an {string} request for {string} in the {string} context"
)]
fn adapter_sent_evaluate(
    world: &mut VardeWorld,
    command: String,
    expression: String,
    context: String,
) {
    let asked: Vec<&Value> = dap_requests(world, &command)
        .into_iter()
        .filter(|message| {
            message["arguments"]["expression"] == json!(expression)
                && message["arguments"]["context"] == json!(context)
        })
        .collect();
    assert!(!asked.is_empty(), "sent: {:?}", world.dap.sent);
}

/// The Watch row, by the expression it holds.
fn watch_row(world: &VardeWorld, expression: &str) -> varde::debug::Row {
    varde::debug::variables(&world.state)
        .into_iter()
        .find(|row| {
            matches!(row.of, varde::debug::Of::Watch { .. }) && row.expression == expression
        })
        .unwrap_or_else(|| panic!("no Watch row {expression:?}"))
}

#[then(expr = "the Watch {string} is marked as calling")]
fn watch_is_marked_as_calling(world: &mut VardeWorld, expression: String) {
    assert!(matches!(
        watch_row(world, &expression).of,
        varde::debug::Of::Watch { calling: true, .. }
    ));
}

#[then(expr = "the Watch {string} is not marked as calling")]
fn watch_is_not_marked_as_calling(world: &mut VardeWorld, expression: String) {
    assert!(matches!(
        watch_row(world, &expression).of,
        varde::debug::Of::Watch { calling: false, .. }
    ));
}

#[when(expr = "the Debug adapter answers the {string} for {string} with the error {string}")]
fn adapter_answers_evaluate_with_error(
    world: &mut VardeWorld,
    command: String,
    expression: String,
    error: String,
) {
    let seq = dap_requests(world, &command)
        .into_iter()
        .rev()
        .find(|message| message["arguments"]["expression"] == json!(expression))
        .unwrap_or_else(|| panic!("no {command:?} for {expression:?}"))["seq"]
        .clone();
    adapter_event(
        world,
        json!({
            "type": "response",
            "request_seq": seq,
            "success": false,
            "command": command,
            "message": error,
        }),
    );
}

#[then(expr = "the Watch {string} shows an error")]
fn watch_shows_an_error(world: &mut VardeWorld, expression: String) {
    assert!(matches!(
        watch_row(world, &expression).of,
        varde::debug::Of::Watch { failed: true, .. }
    ));
}

// ---- #58: Inline values ----

/// The Inline values the editor draws, off the same tokens the edge caches for
/// it and hands `ui`.
fn inline_values(world: &VardeWorld) -> BTreeMap<usize, Vec<varde::debug::Inline>> {
    let (name, source) = match world.state.current_buffer.as_ref() {
        Some(path) => (
            path.file_name().unwrap_or_default().to_string_lossy(),
            current_buffer(world).shown().to_string(),
        ),
        None => return BTreeMap::new(),
    };
    // The columns the renderer drew, off the same rectangles the mouse is
    // hit-tested against — a scenario that never said how big the screen is
    // gets the plain window `screen` falls back to.
    let columns = varde::fits_in(&world.state, &world.panes()).2;
    varde::debug::inline(
        &world.state,
        &varde::highlight::highlight(&name, &source),
        columns,
    )
}

/// One Inline value wherever it is drawn, by the name it stands for.
fn inline_value(world: &VardeWorld, name: &str) -> varde::debug::Inline {
    inline_values(world)
        .into_values()
        .flatten()
        .find(|value| value.name == name)
        .unwrap_or_else(|| panic!("no Inline value for {name:?}"))
}

/// The Locals the scripted adapter answers with, replacing whatever a pause
/// would otherwise plant. One set for every Frame: the fake answers `scopes`
/// the same whichever Frame asked, so a scenario naming one is saying which
/// call's values it is describing, not scripting a second set.
fn plant_locals(world: &mut VardeWorld, values: &[(String, String)]) {
    world.dap.scopes = vec![("Locals".to_string(), LOCALS)];
    world.dap.members.insert(
        LOCALS,
        values
            .iter()
            .map(|(name, value)| json!({ "name": name, "value": value }))
            .collect(),
    );
}

#[given(expr = "the Debug adapter's Locals are {string} = {string}")]
fn locals_are(world: &mut VardeWorld, name: String, value: String) {
    plant_locals(world, &[(name, value)]);
}

#[given(expr = "the Debug adapter's Locals are {string} = {string} and {string} = {string}")]
fn locals_are_two(
    world: &mut VardeWorld,
    first: String,
    first_value: String,
    second: String,
    second_value: String,
) {
    plant_locals(world, &[(first, first_value), (second, second_value)]);
}

#[given(
    expr = "the Debug adapter's Locals for {string} are {string} = {string} and {string} = {string}"
)]
fn locals_for_frame(
    world: &mut VardeWorld,
    _frame: String,
    first: String,
    first_value: String,
    second: String,
    second_value: String,
) {
    plant_locals(world, &[(first, first_value), (second, second_value)]);
}

#[given(expr = "a Debug session is Paused at {string} line {int} with {string} = {string}")]
fn paused_with_local(
    world: &mut VardeWorld,
    file: String,
    line: usize,
    name: String,
    value: String,
) {
    plant_locals(world, &[(name, value)]);
    debug_session_is_paused(world, file, line);
}

#[given(
    expr = "the Debug adapter sends the {string} event for thread {int} at {string} line {int} with reason {string} and {string} = {string} and {string} = {string}"
)]
#[when(
    expr = "the Debug adapter sends the {string} event for thread {int} at {string} line {int} with reason {string} and {string} = {string} and {string} = {string}"
)]
// One parameter per value the step names, which is what the step text has:
// a struct to carry them would be a struct nothing else reads.
#[allow(clippy::too_many_arguments)]
fn adapter_sends_stopped_with_locals(
    world: &mut VardeWorld,
    event: String,
    thread: i64,
    file: String,
    line: usize,
    reason: String,
    first: String,
    first_value: String,
    second: String,
    second_value: String,
) {
    assert_eq!(event, "stopped");
    plant_locals(world, &[(first, first_value), (second, second_value)]);
    stopped_at(world, thread, &file, line, &reason);
}

/// The editor's **text** width rather than its rectangle: what a line has to
/// fit in is what is left once the borders, the gutter and the mirror have
/// taken theirs, and that is the width an Inline value is trimmed to. Not the
/// same question as "the editor **pane** is N columns wide", which sets the
/// rectangle — the two differ by the chrome. Solved for by screen size, for
/// the reason that step solves for one.
#[given(expr = "the editor is {int} columns wide")]
fn editor_is_columns_wide(world: &mut VardeWorld, columns: usize) {
    let (_, height) = world.screen();
    let width = (columns as u16..600)
        .find(|width| {
            let mut wider = world.state.clone();
            wider.screen_width = *width;
            wider.screen_height = height;
            varde::fits(&wider).2 == columns
        })
        .unwrap_or_else(|| panic!("no screen width leaves the editor {columns} columns of text"));
    world.send(Event::Resized { width, height });
}

#[then(expr = "line {int} carries the Inline value {string} = {string}")]
fn line_carries_inline_value(world: &mut VardeWorld, line: usize, name: String, value: String) {
    let drawn = inline_values(world);
    let row = drawn
        .get(&line)
        .unwrap_or_else(|| panic!("line {line} carries no Inline value; drawn: {drawn:?}"));
    let shown = row
        .iter()
        .find(|shown| shown.name == name)
        .unwrap_or_else(|| panic!("line {line} carries nothing for {name:?}; drawn: {row:?}"));
    assert_eq!(shown.text.trim(), format!("{name} = {value}"));
}

#[then(expr = "line {int} carries no Inline value")]
fn line_carries_no_inline_value(world: &mut VardeWorld, line: usize) {
    let drawn = inline_values(world);
    assert!(
        !drawn.contains_key(&line),
        "line {line} carries {:?}",
        drawn.get(&line)
    );
}

#[then(expr = "no line carries an Inline value")]
fn no_line_carries_an_inline_value(world: &mut VardeWorld) {
    assert!(inline_values(world).is_empty(), "values are still drawn");
}

/// A file other than the one the pause is in, looked at the only way there is
/// to look at one: brought on screen, since Inline values are derived for the
/// Buffer the editor is drawing.
#[then(expr = "{string} carries no Inline values")]
fn file_carries_no_inline_values(world: &mut VardeWorld, path: String) {
    let path = abs(world, &path);
    world.apply(vec![Effect::OpenAt {
        path,
        at: Place { line: 1, column: 1 },
    }]);
    assert!(inline_values(world).is_empty(), "values are still drawn");
}

/// The last pause's values, still drawn and nothing among them highlighted:
/// a highlight says *this pause* moved a value, and while the program runs
/// there is no such pause, so the whole run is the faint layer. Dimmed by the
/// same fact the Frames and the Variables are dimmed by, so there is one
/// author for it.
#[then(expr = "the Inline values are drawn dimmed")]
fn inline_values_are_dimmed(world: &mut VardeWorld) {
    let drawn = inline_values(world);
    assert!(!drawn.is_empty(), "no values are drawn");
    assert!(varde::debug::stale(&world.state), "not running");
    let marked: Vec<String> = drawn
        .into_values()
        .flatten()
        .filter(|value| value.changed)
        .map(|value| value.name)
        .collect();
    assert!(marked.is_empty(), "still highlighted: {marked:?}");
}

#[then(expr = "the Inline value {string} is highlighted")]
fn inline_value_is_highlighted(world: &mut VardeWorld, name: String) {
    assert!(inline_value(world, &name).changed, "{name} is not marked");
}

#[then(expr = "the Inline value {string} is not highlighted")]
fn inline_value_is_not_highlighted(world: &mut VardeWorld, name: String) {
    assert!(!inline_value(world, &name).changed, "{name} is marked");
}

#[then(expr = "line {int}'s Inline values end within the editor's width")]
fn inline_values_end_within_the_width(world: &mut VardeWorld, line: usize) {
    let columns = varde::fits_in(&world.state, &world.panes()).2;
    let drawn = inline_values(world);
    let values = drawn
        .get(&line)
        .unwrap_or_else(|| panic!("line {line} carries no Inline value"));
    // Display columns, which is what the pane has: the editor is measured in
    // screen cells and a wide glyph takes two of them.
    let text = unicode_width::UnicodeWidthStr::width(
        current_buffer(world)
            .shown()
            .split('\n')
            .nth(line - 1)
            .unwrap_or_default(),
    );
    let drawn: usize = values
        .iter()
        .map(|value| unicode_width::UnicodeWidthStr::width(value.text.as_str()))
        .sum();
    assert!(
        text + drawn <= columns,
        "{text} + {drawn} columns drawn in {columns}",
    );
}

// ---- #59: a Hover while Paused ----

#[then("the hover's first section is the value")]
fn hover_first_section_is_the_value(world: &mut VardeWorld) {
    assert!(
        matches!(
            varde::lsp::sections(&world.state).first(),
            Some(varde::lsp::Said::Value(_) | varde::lsp::Said::Needs)
        ),
        "the box opens with {:?}",
        varde::lsp::sections(&world.state).first()
    );
}

/// Under the value and never interleaved with it: the two sections are one
/// box, so "second" is a claim about every row above the first doc row.
#[then("the hover's second section is the type and docs")]
fn hover_second_section_is_the_type_and_docs(world: &mut VardeWorld) {
    let said = varde::lsp::sections(&world.state);
    let first = said
        .iter()
        .position(|row| matches!(row, varde::lsp::Said::Docs(_)))
        .expect("no type and docs in the box");
    assert!(first > 0, "the box has no value above the docs");
    assert!(
        said[first..]
            .iter()
            .all(|row| matches!(row, varde::lsp::Said::Docs(_))),
        "the two sections interleave"
    );
}

#[then(expr = "the hover's value section says {string}")]
fn hover_value_section_says(world: &mut VardeWorld, expected: String) {
    let says = match varde::lsp::sections(&world.state).first() {
        Some(varde::lsp::Said::Needs) => "needs-evaluate",
        Some(varde::lsp::Said::Value(_)) => "a-value",
        _ => "nothing",
    };
    assert_eq!(says, expected);
}

#[then(expr = "the hover carries the {string} Chip")]
fn hover_carries_the_chip(world: &mut VardeWorld, name: String) {
    assert!(
        varde::debug::hover_chips(&world.state)
            .iter()
            .any(|chip| chip.name == name),
        "the box carries {:?}",
        varde::debug::hover_chips(&world.state)
            .iter()
            .map(|chip| chip.name)
            .collect::<Vec<&str>>()
    );
}

/// Through the hit-test, on the box's own top border and at the columns `ui`
/// draws the Chips into — never driven as an event, so a Chip no pointer
/// could reach fails here.
#[when(expr = "I click the hover's {string} Chip")]
fn click_the_hovers_chip(world: &mut VardeWorld, name: String) {
    let panes = world.panes();
    let spot = varde::lsp::placement(&world.state)
        .expect("a hover")
        .spot(&world.state, &panes);
    let chips = varde::debug::hover_chips(&world.state);
    let at = chips
        .iter()
        .position(|chip| chip.name == name)
        .unwrap_or_else(|| panic!("no {name:?} Chip on the hover"));
    let labels = varde::debug::hover_labels(&world.state, spot.width);
    let column = (spot.x..spot.right())
        .find(|&column| layout::strip_at(spot, &labels, column) == Some(at))
        .expect("the Chip on screen");
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, spot.y);
    world.report(mouse::Kind::LeftUp, column, spot.y);
}

/// The box's first row of value, clicked where it is drawn.
#[when("I open the hover's value")]
fn open_the_hovers_value(world: &mut VardeWorld) {
    let panes = world.panes();
    let spot = varde::lsp::placement(&world.state)
        .expect("a hover")
        .spot(&world.state, &panes);
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, spot.x + 1, spot.y + 1);
    world.report(mouse::Kind::LeftUp, spot.x + 1, spot.y + 1);
}

/// The span the editor washes, read back out of the buffer it names: a step
/// that trusted the columns would pass on a span naming the wrong characters.
#[then(expr = "the editor highlights {string} on line {int}")]
fn editor_highlights_on_line(world: &mut VardeWorld, text: String, line: usize) {
    let (at, width) = varde::debug::hover_span(&world.state).expect("nothing is highlighted");
    assert_eq!(at.line, line, "highlighted on line {}", at.line);
    let path = world.state.current_buffer.clone().expect("a buffer");
    let source = world.state.buffers[&path].shown().to_string();
    let held: String = source
        .split('\n')
        .nth(line - 1)
        .expect("a line")
        .chars()
        .skip(at.column - 1)
        .take(width)
        .collect();
    assert_eq!(held, text);
}

#[when(expr = "the Debug adapter answers the {string} for {string} with the value {string}")]
fn adapter_answers_evaluate_with_value(
    world: &mut VardeWorld,
    command: String,
    expression: String,
    value: String,
) {
    answer_evaluate(world, &command, &expression, json!({ "result": value }));
}

#[given(expr = "the Debug adapter answers the {string} for {string} with reference {int}")]
#[when(expr = "the Debug adapter answers the {string} for {string} with reference {int}")]
fn adapter_answers_evaluate_with_reference(
    world: &mut VardeWorld,
    command: String,
    expression: String,
    reference: i64,
) {
    answer_evaluate(
        world,
        &command,
        &expression,
        json!({ "result": expression, "variablesReference": reference }),
    );
}

/// The reply to the last request for that expression, which is the one the
/// box is still waiting on.
fn answer_evaluate(world: &mut VardeWorld, command: &str, expression: &str, body: Value) {
    let seq = dap_requests(world, command)
        .into_iter()
        .rev()
        .find(|message| message["arguments"]["expression"] == json!(expression))
        .unwrap_or_else(|| panic!("no {command:?} for {expression:?}"))["seq"]
        .clone();
    adapter_event(
        world,
        json!({
            "type": "response",
            "request_seq": seq,
            "success": true,
            "command": command,
            "body": body,
        }),
    );
}

#[then(expr = "the Debug adapter was sent no {string} request for {string}")]
fn adapter_sent_no_request_for(world: &mut VardeWorld, command: String, expression: String) {
    let asked: Vec<&Value> = dap_requests(world, &command)
        .into_iter()
        .filter(|message| message["arguments"]["expression"] == json!(expression))
        .collect();
    assert!(asked.is_empty(), "sent: {asked:?}");
}

// The Evaluator: the floating window that runs a Snippet inside the Paused
// program. Its Snippet is a Buffer, so the editing steps above reach it once
// it has focus; these are the steps about the window, the run and its output.

fn evaluator(world: &VardeWorld) -> &varde::debug::Evaluator {
    world
        .state
        .evaluator
        .as_ref()
        .expect("the Evaluator is open")
}

fn snippet(world: &VardeWorld) -> String {
    evaluator(world).snippet.shown().to_string()
}

#[given(expr = "the Evaluator is open holding {string}")]
fn evaluator_is_open_holding(world: &mut VardeWorld, expression: String) {
    varde::debug::open_evaluator(&mut world.state, expression);
}

#[given("the Evaluator is open holding:")]
fn evaluator_is_open_holding_block(world: &mut VardeWorld, step: &Step) {
    let text = step
        .docstring()
        .expect("docstring")
        .trim_matches('\n')
        .to_string();
    varde::debug::open_evaluator(&mut world.state, text);
}

/// Where the window is. A scenario that starts from a place sets it straight
/// on the state, the way a rectangle recorded by the project arrives: the
/// dragging is what the scenarios below are *about*, so using a drag to set
/// one up would be a step that fails for the thing it is not testing.
fn window(world: &VardeWorld) -> layout::Area {
    world
        .state
        .evaluator_at
        .expect("the Evaluator has a place on the screen")
}

/// The screen row an editor line is drawn on, which is what a window covering
/// a line has to be placed over.
fn editor_row(world: &VardeWorld, line: u16) -> u16 {
    world.panes().editor.y + line
}

/// A drag of the window's chrome, as the edge reports one: press, one report
/// where the pointer has got to, release. The pointer is made fresh, because
/// a drag belongs to the pane its button went down in.
fn drag_window(world: &mut VardeWorld, from: (u16, u16), to: (u16, u16)) {
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, from.0, from.1);
    world.report(mouse::Kind::LeftDrag, to.0, to.1);
    world.report(mouse::Kind::LeftUp, to.0, to.1);
}

/// The title bar, two columns in: the Chips are flush right, and a press on
/// one of those is a Chip rather than a grab.
fn title_bar(world: &VardeWorld) -> (u16, u16) {
    let at = window(world);
    (at.x + 2, at.y)
}

#[given("the Evaluator is open")]
fn open_an_empty_evaluator(world: &mut VardeWorld) {
    varde::debug::open_evaluator(&mut world.state, String::new());
}

#[given(expr = "the Evaluator is open at column {int} row {int}")]
fn evaluator_open_at(world: &mut VardeWorld, column: u16, row: u16) {
    open_an_empty_evaluator(world);
    world.state.evaluator_at = Some(layout::Area {
        x: column,
        y: row,
        ..window(world)
    });
}

#[given(expr = "the Evaluator is open {int} columns by {int} rows")]
fn evaluator_open_sized(world: &mut VardeWorld, columns: u16, rows: u16) {
    open_an_empty_evaluator(world);
    world.state.evaluator_at = Some(layout::Area {
        width: columns,
        height: rows,
        ..window(world)
    });
}

#[given(expr = "the Evaluator is open with a Snippet {int} rows tall")]
fn evaluator_open_with_snippet_rows(world: &mut VardeWorld, rows: u16) {
    open_an_empty_evaluator(world);
    world
        .state
        .evaluator
        .as_mut()
        .expect("the Evaluator is open")
        .snippet_rows = Some(rows);
}

/// Over the row that line is drawn on, so the stopped event that follows has
/// something to shift the window off.
#[given(expr = "the Evaluator is open over editor line {int}")]
fn evaluator_open_over_line(world: &mut VardeWorld, line: u16) {
    open_an_empty_evaluator(world);
    let row = editor_row(world, line);
    world.state.evaluator_at = Some(layout::Area {
        y: row,
        ..window(world)
    });
    assert!(
        window(world).holds(window(world).x, row),
        "not over line {line}"
    );
}

/// Recorded by the project and read back through starting, so what is asserted
/// is the restore and not a field the step wrote. The session the Background
/// set up stays: only what a start recovers is taken from it.
fn restored(world: &mut VardeWorld, saved: serde_json::Value) -> State {
    world.startup.state_json = Some(saved.to_string());
    world.startup.repo = world.state.repo.clone();
    let (state, _, _) = startup::start(&world.startup).expect("Varde starts");
    state
}

#[given(
    expr = "the project {string} records the Evaluator at column {int} row {int} sized {int} by {int}"
)]
fn state_records_evaluator(
    world: &mut VardeWorld,
    path: String,
    column: u16,
    row: u16,
    width: u16,
    height: u16,
) {
    assert_eq!(path, ".varde/state.json");
    let saved = serde_json::json!({
        "evaluator": {
            "column": column,
            "row": row,
            "width": width,
            "height": height,
        }
    });
    world.state.evaluator_at = restored(world, saved).evaluator_at;
}

#[given(expr = "the project {string} records the Snippets:")]
fn state_records_snippets(world: &mut VardeWorld, path: String, step: &Step) {
    assert_eq!(path, ".varde/state.json");
    let snippets: Vec<String> = step
        .table()
        .expect("a table of Snippets")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let saved = serde_json::json!({ "snippets": snippets });
    world.state.snippets = restored(world, saved).snippets;
}

#[when("I open the Evaluator")]
fn i_open_the_evaluator(world: &mut VardeWorld) {
    world.send(Event::OpenEvaluator);
}

#[when(expr = "I drag the Evaluator's title bar {int} columns right")]
fn drag_title_bar_right(world: &mut VardeWorld, columns: u16) {
    let (column, row) = title_bar(world);
    drag_window(world, (column, row), (column + columns, row));
}

/// Onto the very row the Paused line is drawn on, which is the one place the
/// window may not come to rest.
#[when("I drag the Evaluator's title bar onto the Paused line")]
fn drag_title_bar_onto_paused_line(world: &mut VardeWorld) {
    let (column, row) = title_bar(world);
    let paused = varde::debug::paused_row(&world.state).expect("a Paused line on screen");
    drag_window(world, (column, row), (column, paused));
}

#[when(expr = "I drag the Evaluator's bottom-right corner {int} columns right and {int} rows down")]
fn drag_bottom_right(world: &mut VardeWorld, columns: u16, rows: u16) {
    let at = window(world);
    let corner = (at.right() - 1, at.bottom() - 1);
    drag_window(world, corner, (corner.0 + columns, corner.1 + rows));
}

#[when(expr = "I drag the Evaluator's right border {int} columns left")]
fn drag_right_border(world: &mut VardeWorld, columns: u16) {
    let at = window(world);
    let border = (at.right() - 1, at.y + 2);
    drag_window(world, border, (border.0 - columns, border.1));
}

#[when(expr = "I drag the border under the Snippet up {int} rows")]
fn drag_snippet_border(world: &mut VardeWorld, rows: u16) {
    let at = window(world);
    let (snippet, _) = layout::evaluator_split(at, snippet_rows(world));
    let rule = (at.x + 2, snippet.bottom());
    drag_window(world, rule, (rule.0, rule.1 - rows));
}

fn snippet_rows(world: &VardeWorld) -> Option<u16> {
    world
        .state
        .evaluator
        .as_ref()
        .and_then(|evaluator| evaluator.snippet_rows)
}

#[then("the Evaluator is centred on the screen")]
fn evaluator_is_centred(world: &mut VardeWorld) {
    let (width, height) = (world.state.screen_width, world.state.screen_height);
    let at = window(world);
    // The same room either side and above and below, rather than the number
    // the layout produced: what centred *means* is what the reader sees.
    assert_eq!(at.x, width - at.right(), "{at:?}");
    assert_eq!(at.y, height - at.bottom(), "{at:?}");
}

#[then(expr = "the Evaluator is at column {int} row {int}")]
fn evaluator_is_at(world: &mut VardeWorld, column: u16, row: u16) {
    let at = window(world);
    assert_eq!((at.x, at.y), (column, row), "{at:?}");
}

#[then(expr = "the Evaluator is {int} columns by {int} rows")]
fn evaluator_is_sized(world: &mut VardeWorld, columns: u16, rows: u16) {
    let at = window(world);
    assert_eq!((at.width, at.height), (columns, rows), "{at:?}");
}

#[then(expr = "the Snippet is {int} rows tall")]
fn snippet_is_rows_tall(world: &mut VardeWorld, rows: u16) {
    let (snippet, _) = layout::evaluator_split(window(world), snippet_rows(world));
    assert_eq!(snippet.height, rows);
}

#[then("the Evaluator lies wholly on the screen")]
fn evaluator_lies_on_the_screen(world: &mut VardeWorld) {
    let at = window(world);
    let (width, height) = (world.state.screen_width, world.state.screen_height);
    assert!(at.right() <= width && at.bottom() <= height, "{at:?}");
    assert!(at.width > 0 && at.height > 0, "{at:?}");
}

#[then("the Evaluator does not cover the Paused line")]
fn evaluator_does_not_cover_the_paused_line(world: &mut VardeWorld) {
    let row = varde::debug::paused_row(&world.state).expect("a Paused line on screen");
    let at = window(world);
    assert!(
        !(at.y..at.bottom()).contains(&row),
        "{at:?} covers row {row}"
    );
}

#[given("the Evaluator has focus")]
fn the_evaluator_has_focus(world: &mut VardeWorld) {
    world.state.focus = Pane::Evaluator;
}

#[then(expr = "the Evaluator is open holding {string}")]
fn evaluator_holds(world: &mut VardeWorld, expected: String) {
    assert_eq!(snippet(world), expected);
}

#[then("the Evaluator is open")]
fn evaluator_is_open(world: &mut VardeWorld) {
    assert!(world.state.evaluator.is_some(), "no Evaluator is open");
}

#[then("the Evaluator is not open")]
fn evaluator_is_not_open(world: &mut VardeWorld) {
    assert!(
        world.state.evaluator.is_none(),
        "the Evaluator is still open"
    );
}

#[then(expr = "the Snippet is {string}")]
fn the_snippet_is(world: &mut VardeWorld, expected: String) {
    assert_eq!(snippet(world), expected);
}

#[then("the Snippet is:")]
fn the_snippet_is_block(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    assert_eq!(snippet(world), expected);
}

/// The Selection inside the Snippet, which is what a run sends instead of the
/// whole block. Against the Snippet's own text, never the buffer behind the
/// window.
#[given(expr = "the selection in the Snippet covers {string}")]
fn selection_in_the_snippet(world: &mut VardeWorld, text: String) {
    let held = snippet(world);
    let (index, line) = held
        .split('\n')
        .enumerate()
        .find(|(_, line)| line.contains(&text))
        .unwrap_or_else(|| panic!("no line of the Snippet holds {text:?}"));
    let at = line.find(&text).expect("the column");
    let column = line[..at].chars().count() + 1;
    world.state.selection = Some(Selection::Buffer {
        anchor: Place {
            line: index + 1,
            column,
        },
        cursor: Place {
            line: index + 1,
            column: column + text.chars().count() - 1,
        },
    });
}

/// The same, against a named line of the buffer on screen: what `␣e` reads
/// when there is a Selection rather than a cursor.
#[given(expr = "the selection covers {string} on line {int}")]
fn selection_covers_on_line(world: &mut VardeWorld, text: String, line: usize) {
    let held = current_buffer(world).shown().to_string();
    let row = held
        .split('\n')
        .nth(line - 1)
        .expect("the line")
        .to_string();
    let at = row
        .find(&text)
        .unwrap_or_else(|| panic!("line {line} has no {text:?}"));
    let column = row[..at].chars().count() + 1;
    world.state.selection = Some(Selection::Buffer {
        anchor: Place { line, column },
        cursor: Place {
            line,
            column: column + text.chars().count() - 1,
        },
    });
}

#[when(expr = "I click line {int} column {int} in the editor")]
fn click_line_column_in_editor(world: &mut VardeWorld, line: usize, column: usize) {
    world.click(Pane::Editor, (line, column), terminput::KeyModifiers::NONE);
}

/// Through the columns the Chip is drawn at, so a Chip the renderer would not
/// show fails here — the Hover's Chip click, one window over.
#[when(expr = "I click the Evaluator's {string} Chip")]
fn click_evaluator_chip(world: &mut VardeWorld, name: String) {
    let window = world.panes().evaluator;
    let chips = varde::debug::evaluator_chips(&world.state);
    let at = chips
        .iter()
        .position(|chip| chip.name == name)
        .unwrap_or_else(|| panic!("no {name:?} Chip on the Evaluator"));
    let labels = varde::debug::evaluator_labels(&world.state, window.width);
    let column = (window.x..window.right())
        .find(|&column| layout::strip_at(window, &labels, column) == Some(at))
        .expect("the Chip on screen");
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, window.y);
    world.report(mouse::Kind::LeftUp, column, window.y);
}

#[then(expr = "the Evaluator's {string} Chip is dimmed")]
fn evaluator_chip_is_dimmed(world: &mut VardeWorld, name: String) {
    let chip = varde::debug::evaluator_chips(&world.state)
        .into_iter()
        .find(|chip| chip.name == name)
        .unwrap_or_else(|| panic!("no {name:?} Chip on the Evaluator"));
    assert_eq!(chip.tone, varde::Tone::Dimmed);
}

/// The requests a Snippet's run goes out as, told from a Watch's and a
/// Hover's by the context they carry.
fn repl_requests<'a>(world: &'a VardeWorld, context: &str) -> Vec<&'a Value> {
    dap_requests(world, "evaluate")
        .into_iter()
        .filter(|message| message["arguments"]["context"] == json!(context))
        .collect()
}

#[then(expr = "the Debug adapter was sent an {string} request in the {string} context")]
fn adapter_sent_evaluate_in_context(world: &mut VardeWorld, command: String, context: String) {
    assert_eq!(command, "evaluate");
    assert!(
        !repl_requests(world, &context).is_empty(),
        "sent: {:?}",
        world.dap.sent
    );
}

#[then(expr = "the Debug adapter was sent an {string} request in the {string} context holding:")]
fn adapter_sent_evaluate_holding(
    world: &mut VardeWorld,
    command: String,
    context: String,
    step: &Step,
) {
    assert_eq!(command, "evaluate");
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    let sent: Vec<String> = repl_requests(world, &context)
        .iter()
        .map(|message| {
            message["arguments"]["expression"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert!(sent.iter().any(|held| held == expected), "sent: {sent:?}");
}

#[then(expr = "that request names the Frame {string}")]
fn that_request_names_the_frame(world: &mut VardeWorld, name: String) {
    let asked = last_request(world, "evaluate")["arguments"]["frameId"].clone();
    let frames = varde::debug::frames(&world.state);
    let frame = frames
        .iter()
        .find(|frame| json!(frame.id) == asked)
        .unwrap_or_else(|| panic!("no Frame numbered {asked}"));
    assert_eq!(frame.name, name);
}

/// The reply to the run still in flight, which is the last `evaluate` sent in
/// the `repl` context.
fn answer_the_run(world: &mut VardeWorld, success: bool, body: Value, message: &str) {
    let seq = repl_requests(world, "repl")
        .last()
        .expect("a Snippet was run")["seq"]
        .clone();
    adapter_event(
        world,
        json!({
            "type": "response",
            "request_seq": seq,
            "success": success,
            "command": "evaluate",
            "message": message,
            "body": body,
        }),
    );
}

#[given(expr = "the Debug adapter answers the {string} with the value {string}")]
#[when(expr = "the Debug adapter answers the {string} with the value {string}")]
fn adapter_answers_the_run_with_value(world: &mut VardeWorld, command: String, value: String) {
    assert_eq!(command, "evaluate");
    answer_the_run(world, true, json!({ "result": value }), "");
}

#[given(expr = "the Debug adapter answers the {string} with reference {int}")]
#[when(expr = "the Debug adapter answers the {string} with reference {int}")]
fn adapter_answers_the_run_with_reference(world: &mut VardeWorld, command: String, reference: i64) {
    assert_eq!(command, "evaluate");
    answer_the_run(
        world,
        true,
        json!({ "result": "", "variablesReference": reference }),
        "",
    );
}

#[when(expr = "the Debug adapter answers the {string} with the error {string}")]
fn adapter_answers_the_run_with_error(world: &mut VardeWorld, command: String, why: String) {
    assert_eq!(command, "evaluate");
    answer_the_run(world, false, json!({}), &why);
}

#[given("the Debug adapter reported it supports cancelling")]
fn adapter_supports_cancelling(world: &mut VardeWorld) {
    adapter_event(
        world,
        json!({
            "type": "event",
            "event": "capabilities",
            "body": { "capabilities": { "supportsCancelRequest": true } },
        }),
    );
}

#[then(expr = "the Debug adapter was sent a {string} request for that evaluate")]
fn adapter_sent_cancel_for_that_evaluate(world: &mut VardeWorld, command: String) {
    assert_eq!(command, "cancel");
    let evaluate = repl_requests(world, "repl")
        .last()
        .expect("a Snippet was run")["seq"]
        .clone();
    let cancelled: Vec<&Value> = dap_requests(world, "cancel")
        .into_iter()
        .filter(|message| message["arguments"]["requestId"] == evaluate)
        .collect();
    assert!(!cancelled.is_empty(), "sent: {:?}", world.dap.sent);
}

/// The Evaluator output as it is drawn, one row a kind and its text: what the
/// program printed, then the value or the adapter's reason.
fn evaluator_output(world: &VardeWorld) -> Vec<(String, String)> {
    varde::debug::evaluator_output(&world.state)
        .into_iter()
        .map(|line| match line {
            varde::debug::Said::Printed(text) => ("printed".to_string(), text),
            varde::debug::Said::Failed(why) => ("error".to_string(), why),
            varde::debug::Said::Running => ("running".to_string(), String::new()),
            varde::debug::Said::Value(row) => ("value".to_string(), row.value),
        })
        .collect()
}

#[then("the Evaluator output is:")]
fn the_evaluator_output_is(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, String)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| (row[0].trim().to_string(), row[1].trim().to_string()))
        .collect();
    assert_eq!(evaluator_output(world), expected);
}

#[then(expr = "the Evaluator output holds the print {string}")]
fn the_evaluator_output_holds_the_print(world: &mut VardeWorld, text: String) {
    let held = evaluator_output(world);
    assert!(
        held.contains(&("printed".to_string(), text.clone())),
        "{held:?}"
    );
}

#[then("the Evaluator output is running")]
fn the_evaluator_output_is_running(world: &mut VardeWorld) {
    let held = evaluator_output(world);
    assert!(held.iter().any(|(kind, _)| kind == "running"), "{held:?}");
}

/// Through the row the value is drawn on, which is the row the mouse would
/// click: the prints come first, so the value is not row zero.
#[when("I open the Evaluator output's value")]
fn open_the_evaluator_value(world: &mut VardeWorld) {
    let at = evaluator_output(world)
        .iter()
        .position(|(kind, _)| kind == "value")
        .expect("a value in the Evaluator output");
    world.send(Event::OpenEvaluatedRow(at));
}

#[then(expr = "the project {string} records the Snippet {string}")]
fn state_json_records_the_snippet(world: &mut VardeWorld, _file: String, expected: String) {
    let saved: Value =
        serde_json::from_str(world.startup.state_json.as_deref().expect("state saved"))
            .expect("json");
    let held: Vec<String> = saved["snippets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|snippet| Some(snippet.as_str()?.to_string()))
        .collect();
    assert!(held.contains(&expected), "{held:?}");
}

/// The run gesture without naming which of the three makes it: normal-mode
/// Enter, the Run Chip and Ctrl+Enter are one event, so a scenario about what
/// a run carries says only that one was made.
#[when("I run the Snippet")]
fn run_the_snippet(world: &mut VardeWorld) {
    world.send(Event::RunSnippet);
}

#[then(expr = "the Debug adapter was sent an {string} request in the Frame {string}")]
fn adapter_sent_evaluate_in_frame(world: &mut VardeWorld, command: String, name: String) {
    assert_eq!(command, "evaluate");
    adapter_sent_evaluate_in_context(world, command.clone(), "repl".to_string());
    that_request_names_the_frame(world, name);
}

/// In the `initialize` answer for a session still to come, and in a
/// `capabilities` event for one already running — the two places an adapter
/// says what it can pause on.
#[given("the Debug adapter reported the Exception filters:")]
fn adapter_reported_exception_filters(world: &mut VardeWorld, step: &Step) {
    let table = step.table().expect("a table of filters");
    world.dap.filters = table.rows[1..]
        .iter()
        .map(|row| json!({ "filter": row[0], "label": row[1] }))
        .collect();
    if world.state.debug.is_some() {
        let filters = world.dap.filters.clone();
        adapter_event(
            world,
            json!({
                "type": "event",
                "event": "capabilities",
                "body": { "capabilities": { "exceptionBreakpointFilters": filters } },
            }),
        );
    }
}

#[then("the Breakpoint list switches are:")]
fn breakpoint_list_switches_are(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a list of filters")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    let shown: Vec<String> = varde::debug::switches(&world.state)
        .into_iter()
        .map(|(filter, _)| filter.id.clone())
        .collect();
    assert_eq!(shown, expected);
}

/// The keyboard's route: along the Breakpoint list to the switch, and Enter.
#[when(expr = "I switch the Exception filter {string} on")]
fn switch_exception_filter_on(world: &mut VardeWorld, id: String) {
    let at = varde::debug::switches(&world.state)
        .iter()
        .position(|(filter, _)| filter.id == id)
        .unwrap_or_else(|| panic!("no {id:?} switch"));
    assert!(
        !varde::debug::switches(&world.state)[at].1,
        "{id:?} is already on"
    );
    world.state.focus = Pane::Breakpoints;
    while world.state.breakpoints_selection > at {
        route_key(world, "k", 0);
    }
    while world.state.breakpoints_selection < at {
        route_key(world, "j", 0);
    }
    route_key(world, "Enter", 0);
}

#[then(expr = "the Debug adapter was sent a {string} request with the filters:")]
fn adapter_sent_with_the_filters(world: &mut VardeWorld, command: String, step: &Step) {
    let expected: Vec<String> = step
        .table()
        .expect("a list of filters")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(
        last_request(world, &command)["arguments"]["filters"],
        json!(expected)
    );
}

#[given(expr = "the project {string} records the Exception filter {string} on for {string}")]
fn state_records_exception_filter(
    world: &mut VardeWorld,
    path: String,
    id: String,
    adapter: String,
) {
    assert_eq!(path, ".varde/state.json");
    let mut saved: Value = world
        .startup
        .state_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_else(|| json!({}));
    saved["exception_filters"][adapter.as_str()] = json!([id]);
    world.startup.state_json = Some(saved.to_string());
}

#[then(expr = "the project {string} records the Exception filter {string} on for {string}")]
fn records_exception_filter(world: &mut VardeWorld, path: String, id: String, adapter: String) {
    assert_eq!(path, ".varde/state.json");
    let saved: Value =
        serde_json::from_str(world.startup.state_json.as_deref().expect("state saved"))
            .expect("json");
    assert_eq!(saved["exception_filters"][adapter.as_str()], json!([id]));
}

#[given("the Debug adapter reported it supports exception options")]
#[given("the Debug adapter reported it does not support exception options")]
fn adapter_reported_exception_options(world: &mut VardeWorld, step: &Step) {
    let supports = !step.value.contains("does not");
    adapter_event(
        world,
        json!({
            "type": "event",
            "event": "capabilities",
            "body": { "capabilities": { "supportsExceptionOptions": supports } },
        }),
    );
}

/// Through the box the exception-class Chip's key opens: the key, the name,
/// Enter — so a box that never opened fails here.
#[when(expr = "I pause on the exception class {string}")]
fn pause_on_exception_class(world: &mut VardeWorld, class: String) {
    corner_shows_breakpoint_list(world);
    world.state.focus = Pane::Breakpoints;
    route_key(world, "x", 0);
    assert_eq!(
        world.state.modal,
        Modal::ExceptionClass,
        "the exception-class box did not open"
    );
    for character in class.chars() {
        route_key(world, &character.to_string(), 0);
    }
    route_key(world, "Enter", 0);
}

#[then(expr = "the Debug adapter was sent a {string} request naming {string}")]
fn adapter_sent_naming(world: &mut VardeWorld, command: String, class: String) {
    assert_eq!(
        last_request(world, &command)["arguments"]["exceptionOptions"][0]["path"][0]["names"],
        json!([class])
    );
}

// ---- The Diagnostic list ----

/// A Severity as the Scenarios name it: the protocol's own word, which is
/// `lsp::Severity::as_str`, never a label's copy.
fn severity_named(name: &str) -> lsp::Severity {
    lsp::Severity::ALL
        .into_iter()
        .find(|severity| severity.as_str() == name)
        .unwrap_or_else(|| panic!("no such severity: {name}"))
}

/// Through the palette's event, as every other Corner occupant's is: the only
/// way the list comes to be on screen is being asked for.
#[given("the Diagnostic list is shown")]
fn diagnostic_list_is_shown(world: &mut VardeWorld) {
    if lsp::showing(&world.state).is_none() {
        world.send(Event::ToggleDiagnosticList);
    }
    assert_eq!(world.state.focus, Pane::Diagnostics);
}

#[when("I show the Diagnostic list")]
fn show_diagnostic_list(world: &mut VardeWorld) {
    assert_eq!(lsp::showing(&world.state), None);
    world.send(Event::ToggleDiagnosticList);
}

#[then("the Corner holds the Diagnostic list")]
fn corner_holds_diagnostic_list(world: &mut VardeWorld) {
    assert!(
        matches!(world.state.corner, layout::Corner::Diagnostics(_)),
        "{:?}",
        world.state.corner
    );
}

#[then(expr = "the Diagnostic list shows {string}")]
fn diagnostic_list_shows(world: &mut VardeWorld, severity: String) {
    assert_eq!(lsp::showing(&world.state), Some(severity_named(&severity)));
}

#[then("the Diagnostic list has no rows")]
fn diagnostic_list_is_empty(world: &mut VardeWorld) {
    assert!(lsp::listed(&world.state).is_empty());
}

/// With a header row: a file heading has neither line nor column.
#[then("the Diagnostic list rows are:")]
fn diagnostic_list_rows(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, Option<(usize, usize)>)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let at = match row[1].trim() {
                "" => None,
                line => Some((
                    line.parse().expect("a line"),
                    row[2].trim().parse().expect("a column"),
                )),
            };
            (row[0].clone(), at)
        })
        .collect();
    let listed: Vec<(String, Option<(usize, usize)>)> = lsp::listed(&world.state)
        .into_iter()
        .map(|(path, diagnostic)| {
            (
                varde::relative(&world.state, path),
                diagnostic.map(|diagnostic| (diagnostic.line, diagnostic.column)),
            )
        })
        .collect();
    assert_eq!(listed, expected);
}

/// The index of the row naming a file's Diagnostic on a line, or its heading.
fn diagnostic_row(world: &VardeWorld, file: &str, line: Option<usize>) -> usize {
    let path = abs(world, file);
    lsp::listed(&world.state)
        .iter()
        .position(|(at, diagnostic)| {
            *at == path.as_path() && diagnostic.map(|diagnostic| diagnostic.line) == line
        })
        .unwrap_or_else(|| panic!("no row for {file} {line:?}"))
}

/// Moved to with the keyboard, the way a reader gets there.
fn select_diagnostic_row(world: &mut VardeWorld, index: usize) {
    world.state.focus = Pane::Diagnostics;
    while world.state.diagnostics_selection > index {
        world.send(Event::Key('k'));
    }
    while world.state.diagnostics_selection < index {
        world.send(Event::Key('j'));
    }
}

#[given(expr = "the Diagnostic list selection is on {string} line {int}")]
fn diagnostic_selection_on(world: &mut VardeWorld, file: String, line: usize) {
    let index = diagnostic_row(world, &file, Some(line));
    select_diagnostic_row(world, index);
}

#[given(expr = "the Diagnostic list selection is on the heading for {string}")]
fn diagnostic_selection_on_heading(world: &mut VardeWorld, file: String) {
    let index = diagnostic_row(world, &file, None);
    select_diagnostic_row(world, index);
}

#[then(expr = "the Diagnostic list selection is on {string} line {int}")]
fn diagnostic_selection_should_be_on(world: &mut VardeWorld, file: String, line: usize) {
    assert_eq!(
        world.state.diagnostics_selection,
        diagnostic_row(world, &file, Some(line))
    );
}

/// Through the hit-test, a press and a release on the row's text.
#[when(expr = "I click the Diagnostic list row for {string} line {int}")]
fn click_diagnostic_row(world: &mut VardeWorld, file: String, line: usize) {
    let index = diagnostic_row(world, &file, Some(line));
    let corner = world.panes().corner;
    let row = corner.y + 1 + (index - world.state.diagnostics_scroll) as u16;
    world.report(mouse::Kind::LeftDown, corner.x + 2, row);
    world.report(mouse::Kind::LeftUp, corner.x + 2, row);
}

/// On the label's first column, found the way `ui` right-aligns them.
#[when(expr = "I click the Severity label for {string}")]
fn click_severity_label(world: &mut VardeWorld, severity: String) {
    let corner = world.panes().corner;
    let labels = lsp::severity_labels(&world.state, corner.width);
    let at = lsp::Severity::ALL
        .iter()
        .position(|each| *each == severity_named(&severity))
        .expect("a label");
    let column =
        corner.right() - 1 - layout::strip_width(&labels) + layout::strip_width(&labels[..at]) + 1;
    world.report(mouse::Kind::LeftDown, column, corner.y);
    world.report(mouse::Kind::LeftUp, column, corner.y);
}

/// Headerless: a Severity and its count, every one of the four.
#[then("the Severity labels count:")]
fn severity_labels_count(world: &mut VardeWorld, step: &Step) {
    for row in &step.table().expect("table").rows {
        assert_eq!(
            lsp::total(&world.state, severity_named(row[0].trim())),
            row[1].trim().parse::<usize>().expect("a count"),
            "{}",
            row[0]
        );
    }
}

/// Headerless: the Severities the tree's border counts, in the order drawn.
#[then("the tree's border counts:")]
fn tree_border_counts(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(lsp::Severity, usize)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| {
            (
                severity_named(row[0].trim()),
                row[1].trim().parse().expect("a count"),
            )
        })
        .collect();
    let drawn: Vec<(lsp::Severity, usize)> = lsp::nudge(&world.state)
        .into_iter()
        .map(|(severity, _)| (severity, lsp::total(&world.state, severity)))
        .collect();
    assert_eq!(drawn, expected);
}

#[then("the tree's border counts nothing")]
fn tree_border_counts_nothing(world: &mut VardeWorld) {
    assert_eq!(lsp::nudge(&world.state), vec![]);
}

/// On the count's first column: the title and every string before it, then
/// the gap its own string opens with.
#[when(expr = "I click the tree's {word} count")]
fn click_tree_count(world: &mut VardeWorld, severity: String) {
    let tree = world.panes().tree;
    let mut column = tree.x + 1 + tree::title(&world.state).width() as u16;
    for (each, label) in lsp::nudge(&world.state) {
        if each == severity_named(&severity) {
            column += (label.len() - label.trim_start().len()) as u16;
            break;
        }
        column += label.width() as u16;
    }
    world.report(mouse::Kind::LeftDown, column, tree.y);
    world.report(mouse::Kind::LeftUp, column, tree.y);
}
