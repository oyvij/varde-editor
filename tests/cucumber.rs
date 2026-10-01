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

const POINTER: Place = Place { line: 3, column: 7 };

#[derive(Debug, Default, World)]
pub struct VardeWorld {
    state: State,
    pointer: mouse::Pointer,
    terminal_input: String,
    executed: Vec<String>,
    target: Option<Target>,
    rendered: Vec<View>,
    startup: Startup,
    config: Option<Config>,
    startup_effects: Vec<Effect>,
    error: Option<StartupError>,
    config_effects: Vec<Effect>,
    dirs: BTreeSet<PathBuf>,
    files: BTreeMap<PathBuf, String>,
    wrote: Vec<PathBuf>,
    known_files: BTreeSet<PathBuf>,
    opened: Vec<PathBuf>,
    disk: BTreeMap<PathBuf, Vec<Entry>>,
    ai_spawned: Vec<String>,
    ai_pane: bool,
    voice_child: bool,
    player_on_path: bool,
    voice_on_disk: bool,
    speaking: Option<String>,
    stream: Vec<u32>,
    players: usize,
    spoken_at: Option<f32>,
    resumed_from: Option<u32>,
    paused_at: Option<u32>,
    tree_before: Vec<PathBuf>,
    ai_spawn_fails: bool,
    notices: Vec<String>,
    scrolled: Vec<(Pane, Direction)>,
    clipboard: Option<String>,
    browser: Vec<String>,
    keys_sent: Vec<(Pane, Vec<u8>)>,
    splits: Vec<usize>,
    highlighted: Vec<Vec<varde::highlight::Token>>,
    highlighted_source: String,
    diff_sides: (
        Vec<Vec<varde::highlight::Token>>,
        Vec<Vec<varde::highlight::Token>>,
    ),
    exited: bool,
    relaunched: bool,
    replacing: Vec<Effect>,
    terminal_clipboard: Option<String>,
    ai_stopped: bool,
    project: Vec<(String, String)>,
    indexable: Vec<String>,
    holding_searches: bool,
    held_search: Option<varde::search::Request>,
    diffs_read: Vec<PathBuf>,
    screen: Vec<String>,
    ai_screen: Vec<String>,
    program: Vec<Vec<String>>,
    output_pty: Option<(u16, u16)>,
    drafts: Drafts,
    diff_contents: BTreeMap<String, String>,
    unresolvable: BTreeSet<String>,
    repo_before: Option<Vec<String>>,
    held: BTreeMap<PathBuf, String>,
    origin_head: Option<String>,
    upstream_branch: Option<String>,
    default_branch_config: Option<String>,
    probes: BTreeSet<String>,
    resolved_oids: BTreeMap<String, (String, String)>,
    analyses: Vec<Analysis>,
    head: Option<String>,
    last_authored_range: Option<(String, String)>,
    branch_refs: Vec<story::BranchRef>,
    on_branch: String,
    checkouts: Vec<(PathBuf, String)>,
    tests_run: Vec<String>,
    snapshots: BTreeMap<u32, BTreeMap<String, String>>,
    restores: Vec<u32>,
    restored: Vec<String>,
    deleted: Vec<PathBuf>,
    dirs_deleted: Vec<PathBuf>,
    committed: BTreeMap<String, String>,
    mine: BTreeMap<String, String>,
    lsp_running: BTreeMap<String, String>,
    on_path: BTreeSet<String>,
    git_on_path: bool,
    workspace_facts: BTreeMap<String, String>,
    folders_to_read: Vec<PathBuf>,
    lsp_started: Vec<(String, String)>,
    lsp_args: BTreeMap<String, Vec<String>>,
    lsp_sent: Vec<(String, Value)>,
    lsp_answers: BTreeSet<String>,
    lsp_fails: BTreeSet<String>,
    formatter_runs: Vec<Effect>,
    candidates_armed: bool,
    dwell_armed: Option<u64>,
    reported: Vec<String>,
    hold_site_texts: bool,
    named: Vec<String>,
    dap: FakeAdapter,
}

#[derive(Debug, Default)]
struct FakeAdapter {
    spawned: Vec<String>,
    dialed: Vec<u16>,
    held: bool,
    ready: bool,
    sent: Vec<Value>,
    stacks: BTreeMap<i64, Vec<(String, PathBuf, usize, String)>>,
    scopes: Vec<(String, i64)>,
    members: BTreeMap<i64, Vec<Value>>,
    since_pause: usize,
    stopped: Option<Value>,
    filters: Vec<Value>,
    text: Option<String>,
    children: BTreeMap<usize, Vec<Value>>,
    child_thread: String,
}

impl VardeWorld {
    fn read_file(&self, repo: &Path, file: &str) -> story::FileHunks {
        let path = repo.join(file);
        let old = self.held.get(&path).cloned().unwrap_or_default();
        let new = self.files.get(&path).cloned().unwrap_or_default();
        story::FileHunks {
            file: file.to_string(),
            hunks: story::hunks(old.as_bytes(), new.as_bytes()),
            old_exists: self.held.contains_key(&path),
            old_text: old,
            head_exists: self.files.contains_key(&path),
            head_text: new.clone(),
            new_exists: self.files.contains_key(&path),
            new_text: new,
        }
    }

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
            if let Some((pane, at)) = outcome.link {
                let row = self.pane_lines(pane)[at.line - 1].clone();
                self.send(Event::ClickLink {
                    row,
                    column: at.column,
                });
            }
        }
    }

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

    fn drag(&mut self, pane: Pane, from: (usize, usize), to: (usize, usize)) {
        self.pointer = mouse::Pointer::default();
        for place in [from, to] {
            let (column, row) = pointer_at(&self.state, &self.panes(), pane, place);
            self.report(mouse::Kind::LeftDrag, column, row);
        }
    }

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
            | Pane::Conflicts
            | Pane::Variables
            | Pane::Output
            | Pane::Cheatsheet
            | Pane::Terminal => self.screen.clone(),
        }
    }

    fn send(&mut self, event: Event) {
        let (state, effects) = update(&self.state, event);
        self.state = state;
        self.apply(effects);
        self.tell_core();
    }

    fn tell_core(&mut self) {
        self.state.ai_running = self.ai_pane;
        self.state.output_running = self.output_pty.is_some();
        let output = self.panes().output;
        if let (true, Some(size)) = (
            self.output_pty.is_some(),
            layout::pty_size(output.width, output.height),
        ) {
            self.output_pty = Some(size);
        }
        self.state.head = self.head.clone();
        self.state.branch = (!self.on_branch.is_empty()).then(|| self.on_branch.clone());
        self.state.lsp_running = self.lsp_running.keys().cloned().collect();
        self.state.commands_on_path = self.on_path.clone();
        self.state.git_installed = self.git_on_path;
        self.state.workspace_facts = self.workspace_facts.clone();
        self.state.voice_running = self.voice_child;
        self.state.player_installed = self.player_on_path;
        self.state.voice_installed = self.voice_on_disk && !self.state.speech.voice.is_empty();
        if let Some(path) = self.state.current_buffer.clone() {
            let committed = self.state.committed.get(&path).and_then(Option::as_deref);
            if let Some(buffer) = self.state.buffers.get(&path) {
                let lines = varde::authorship::traced(committed, buffer.shown());
                self.state.traced = Some((path, buffer.revision(), lines.into()));
            }
        }
    }

    fn ai_is_gone(&mut self) {
        self.ai_pane = false;
        let (state, effects) = update(&self.state, Event::AiExited);
        self.state = state;
        self.apply(effects);
    }

    fn lsp_is_running(&mut self, language: &str, command: &str) {
        self.lsp_running
            .insert(language.to_string(), command.to_string());
        self.tell_core();
        self.send_now(Event::LspStarted {
            language: language.to_string(),
        });
    }

    fn lsp_is_gone(&mut self, language: &str, why: Gone) {
        self.lsp_running.remove(language);
        self.tell_core();
        self.send_now(Event::LspGone {
            language: language.to_string(),
            why,
        });
    }

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

    fn adapter_says(&mut self, message: Value) {
        self.send_now(Event::DapReceived {
            json: message.to_string(),
            from: 0,
        });
    }

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

    fn lsp_replies(&mut self, language: &str, message: Value) {
        self.send_now(Event::LspReceived {
            language: language.to_string(),
            json: message.to_string(),
        });
    }

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
        for path in std::mem::take(&mut self.folders_to_read) {
            let entries = self.disk.get(&path).cloned().unwrap_or_default();
            self.send_now(Event::Expand { path, entries });
        }
    }

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
                    true => {
                        if handshake && self.lsp_answers.contains(&language) {
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
            Effect::ClearNotice => self.notices.clear(),
            Effect::RunProgram { argv, .. } => {
                self.program.push(argv);
                self.output_pty = Some((2, 1));
            }
            Effect::Speak { utterances, speed } => {
                self.speaking = Some(reading::words(&utterances));
                self.players += 1;
                self.stream = built(&utterances);
                self.spoken_at = Some(speed);
            }
            Effect::SpeakFrom { at_ms } => {
                self.speaking = self
                    .state
                    .reading
                    .as_ref()
                    .map(|reading| reading::words(&reading.utterances));
                self.players = 1;
                self.resumed_from = Some(at_ms);
            }
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

    fn search_finishes(&mut self, request: varde::search::Request) {
        let disk: Vec<(String, String)> = self
            .project
            .iter()
            .filter(|(name, _)| request.covers(name))
            .cloned()
            .collect();
        let mut hits = varde::search::scan(&request.query, &request.buffers);
        hits.extend(varde::search::scan(&request.query, &disk));
        let (state, effects) = update(
            &self.state,
            Event::Searched {
                generation: request.generation,
                hits,
                done: true,
            },
        );
        self.state = state;
        self.apply(effects);
    }

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
            Effect::ReadClipboard => {
                if let Some(text) = self.clipboard.clone().filter(|text| !text.is_empty()) {
                    let (state, effects) = update(&self.state, Event::EditorPaste(text));
                    self.state = state;
                    self.apply(effects);
                }
            }
            Effect::IndexProject { walk } => {
                let files = self.indexable.clone();
                let (state, effects) = update(
                    &self.state,
                    Event::Indexed {
                        walk,
                        files,
                        done: true,
                    },
                );
                self.state = state;
                self.apply(effects);
            }
            Effect::RunSearch(request) => match self.holding_searches {
                true => self.held_search = Some(request),
                false => self.search_finishes(request),
            },
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
            Effect::Snapshot { iteration } => {
                self.snapshots.insert(iteration, self.tree());
            }
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

    fn applied_to_jobs(&mut self, effect: Effect) {
        match effect {
            Effect::RunTests { command } => self.tests_run.push(command),
            run @ Effect::RunFormatter { .. } => self.formatter_runs.push(run),
            Effect::ProbePath => {}
            Effect::CheckRelease { .. } => {}
            fetch @ Effect::ReplaceBinary { .. } => self.replacing.push(fetch),
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
            Effect::WriteStoryContext {
                repo: _,
                spelling,
                path,
            } => {
                self.wrote.push(path.clone());
                self.files.insert(path, format!("the change in {spelling}"));
            }
            Effect::ReadBranches => {
                let branching = match &self.state.repo {
                    None => story::Branching::NotARepository,
                    Some(files) if story::uncommitted(files) => story::Branching::Dirty,
                    Some(_) => story::Branching::Listed(self.branch_refs.clone()),
                };
                self.send_now(Event::Branches(branching));
            }
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
            Effect::ReadGuestBranches { sentinel, how, .. } => {
                let status = self.files.get(&sentinel).cloned();
                self.send_now(Event::Branches(match status.as_deref().map(str::trim) {
                    Some("0") => story::Branching::Listed(self.branch_refs.clone()),
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

    fn tree(&self) -> BTreeMap<String, String> {
        self.project.iter().cloned().collect()
    }

    fn on_disk(&self, path: &Path) -> String {
        let relative = path
            .strip_prefix(&self.state.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
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

fn config_edited(world: &mut VardeWorld, global: Option<startup::OnDisk>) {
    let layer = |text: &Option<String>| match text {
        Some(text) => startup::OnDisk::Text(text.clone()),
        None => startup::OnDisk::Missing,
    };
    let event = Event::ConfigEdited {
        global: global.unwrap_or_else(|| layer(&world.startup.global_config)),
        project: layer(&world.startup.project_config),
    };
    let (state, effects) = update(&world.state, event);
    world.state = state;
    world.config_effects = effects.clone();
    world.apply(effects);
    world.tell_core();
}

#[when("the global config is saved as:")]
fn global_config_saved(world: &mut VardeWorld, step: &Step) {
    global_config(world, step);
    config_edited(world, None);
}

#[when("the project config is saved as:")]
fn project_config_saved(world: &mut VardeWorld, step: &Step) {
    project_config(world, step);
    config_edited(world, None);
}

#[when("the global config can no longer be read")]
fn global_config_unreadable(world: &mut VardeWorld) {
    config_edited(world, Some(startup::OnDisk::Unreadable));
}

#[then(expr = "the test command in effect is {string}")]
fn test_command_in_effect(world: &mut VardeWorld, command: String) {
    assert_eq!(world.state.test_command, Some(command));
}

#[then(expr = "no test command is in effect")]
fn no_test_command_in_effect(world: &mut VardeWorld) {
    assert_eq!(world.state.test_command, None);
}

#[then(expr = "the language server for {string} is configured as {string}")]
fn server_configured_as(world: &mut VardeWorld, language: String, command: String) {
    let server = world.state.servers.get(&language);
    assert_eq!(
        server.map(|server| server.command.as_str()),
        Some(command.as_str())
    );
}

#[then(expr = "the save asked for nothing to be started or stopped")]
fn save_asked_for_nothing(world: &mut VardeWorld) {
    assert!(
        world.config_effects.is_empty(),
        "asked for: {:?}",
        world.config_effects
    );
}

#[then(expr = "a language server for {string} is still running")]
fn server_still_running(world: &mut VardeWorld, language: String) {
    assert!(
        world.state.lsp_running.contains(&language),
        "running: {:?}",
        world.state.lsp_running
    );
}

#[given(expr = "the project {string} records the minimap as hidden")]
fn state_records_minimap_hidden(world: &mut VardeWorld, path: String) {
    assert_eq!(path, ".varde/state.json");
    world.startup.state_json = Some(r#"{"minimap": false}"#.to_string());
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

#[given(expr = "I ask Varde for help from the command line")]
#[when(expr = "I ask Varde for help from the command line")]
fn ask_for_help(world: &mut VardeWorld) {
    world.send(Event::ToggleCheatsheet);
}

#[given(expr = "the Cheatsheet is hidden")]
fn reminder_hidden(world: &mut VardeWorld) {
    world.state.ai_slot = layout::Slot::Ai;
}

#[given(expr = "the Cheatsheet is shown")]
fn reminder_up(world: &mut VardeWorld) {
    world.state.ai_slot = layout::Slot::Cheatsheet;
}

#[given(expr = "the project {string} records the Cheatsheet as shown")]
fn state_records_reminder(world: &mut VardeWorld, path: String) {
    assert_eq!(path, ".varde/state.json");
    world.startup.state_json = Some("{\"cheatsheet\": true}".to_string());
}

#[then(expr = "the Cheatsheet is shown")]
fn reminder_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.ai_slot, layout::Slot::Cheatsheet);
}

#[then(expr = "the Cheatsheet is not shown")]
fn reminder_not_shown(world: &mut VardeWorld) {
    assert_eq!(world.state.ai_slot, layout::Slot::Ai);
}

#[when(expr = "I turn the wheel {word} over the AI pane's rectangle")]
fn wheel_over_ai_rectangle(world: &mut VardeWorld, direction: String) {
    let ai = world.panes().ai;
    let kind = match parse_direction(&direction) {
        Direction::Down => mouse::Kind::ScrollDown,
        _ => mouse::Kind::ScrollUp,
    };
    world.report(kind, ai.x + ai.width / 2, ai.y + ai.height / 2);
}

#[then(expr = "the Cheatsheet is scrolled {int} rows down")]
fn cheatsheet_scrolled(world: &mut VardeWorld, rows: usize) {
    assert_eq!(world.state.cheatsheet_scroll, rows);
}

#[then(expr = "the Cheatsheet's last row is on screen")]
fn cheatsheet_at_its_end(world: &mut VardeWorld) {
    let rows = keys::cheatsheet_rows(&world.state).len();
    let fits = varde::cheatsheet_fits(&world.state);
    assert!(rows > fits, "{rows} rows fit in {fits}");
    assert_eq!(world.state.cheatsheet_scroll, rows - fits);
}

#[then(expr = "the Cheatsheet lists the keys of the view on screen")]
fn cheatsheet_lists_the_view(world: &mut VardeWorld) {
    assert_eq!(world.state.ai_slot, layout::Slot::Cheatsheet);
    assert!(!keys::cheatsheet_rows(&world.state).is_empty());
}

#[then(expr = "nothing was asked of the AI session")]
fn nothing_asked_of_ai(world: &mut VardeWorld) {
    assert!(world.ai_spawned.is_empty(), "{:?}", world.ai_spawned);
    assert!(!world.ai_stopped);
    assert!(ai_sends(world).is_empty(), "{:?}", world.keys_sent);
    assert!(world.state.ai_running);
}

#[then(expr = "the saved project state does not mention the Cheatsheet")]
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
    if world.startup.os.is_empty() {
        world.startup.os = "macos".to_string();
    }
    world.startup.varde_home = Path::new(HOME).join(varde::VARDE_DIR);
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

const SIDECAR: &str = "/home/me/.varde/paths/%home%me%projects%theirs-4242";

const HOME: &str = "/home/me";

#[given(expr = "Varde started with no folder in {string}")]
#[when(expr = "Varde starts with no folder in {string}")]
fn varde_starts_bare(world: &mut VardeWorld, path: String) {
    world.startup.sidecar = Some(PathBuf::from(SIDECAR));
    varde_opens(world, path);
}

#[then(expr = "no directory was created in the folder")]
fn no_directory_in_folder(world: &mut VardeWorld) {
    let inside: Vec<&PathBuf> = world
        .dirs
        .iter()
        .filter(|dir| dir.starts_with(&world.startup.root))
        .collect();
    assert!(inside.is_empty(), "created: {inside:?}");
}

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

#[then(expr = "{string} was written into the folder Varde was started in")]
fn written_into_folder(world: &mut VardeWorld, name: String) {
    let path = world.startup.root.join(name);
    assert!(world.wrote.contains(&path), "written: {:?}", world.wrote);
}

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

#[then(expr = "the Sidecar was deleted")]
fn sidecar_was_deleted(world: &mut VardeWorld) {
    assert!(
        world.dirs_deleted.contains(&PathBuf::from(SIDECAR)),
        "deleted: {:?}",
        world.dirs_deleted
    );
}

#[then(expr = "no directory was deleted")]
fn no_directory_deleted(world: &mut VardeWorld) {
    let deleted: Vec<&PathBuf> = world
        .dirs_deleted
        .iter()
        .filter(|path| !is_scratch(world, path))
        .collect();
    assert!(deleted.is_empty(), "deleted: {deleted:?}");
}

fn is_scratch(world: &VardeWorld, path: &Path) -> bool {
    path == varde::tmp_dir(&world.startup.varde_home)
}

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

fn abs(world: &VardeWorld, path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => Path::new(HOME).join(rest),
        None => world.state.root.join(path),
    }
}

fn pointer_at(
    state: &State,
    panes: &layout::Layout,
    pane: Pane,
    (line, column): (usize, usize),
) -> (u16, u16) {
    let (area, gutter, scroll) = match pane {
        Pane::Editor => (panes.editor, varde::gutter(state), state.editor_scroll),
        Pane::Tree => (panes.tree, 0, state.tree_scroll),
        Pane::Ai | Pane::Cheatsheet => (panes.ai, 0, 0),
        Pane::Output => (panes.output, 0, 0),
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts => (panes.corner, 0, 0),
        Pane::Terminal | Pane::Variables => (panes.terminal, 0, 0),
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

fn open_buffer(world: &mut VardeWorld, path: &str, contents: &str) -> PathBuf {
    let absolute = abs(world, path);
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
    let contents = world
        .files
        .get(&abs(world, &path))
        .cloned()
        .unwrap_or_else(|| "on disk".to_string());
    open_buffer(world, &path, &contents);
}

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
    if key == "Space"
        || world.state.stepping
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

fn named_key(key: &str) -> Option<terminput::KeyEvent> {
    let alt = |code| terminput::KeyEvent::new(code).modifiers(terminput::KeyModifiers::ALT);
    let plain = terminput::KeyEvent::new;
    Some(match key {
        "Alt+Enter" => alt(terminput::KeyCode::Enter),
        "Alt+h" => alt(terminput::KeyCode::Char('h')),
        "Alt+Left" => alt(terminput::KeyCode::Left),
        "Ctrl+Alt+Left" => plain(terminput::KeyCode::Left)
            .modifiers(terminput::KeyModifiers::ALT | terminput::KeyModifiers::CTRL),
        "Ctrl+Alt+Right" => plain(terminput::KeyCode::Right)
            .modifiers(terminput::KeyModifiers::ALT | terminput::KeyModifiers::CTRL),
        "Alt+Right" => alt(terminput::KeyCode::Right),
        "Shift+Right" => plain(terminput::KeyCode::Right).modifiers(terminput::KeyModifiers::SHIFT),
        "Shift+Alt+Right" => plain(terminput::KeyCode::Right)
            .modifiers(terminput::KeyModifiers::SHIFT | terminput::KeyModifiers::ALT),
        "Left" => plain(terminput::KeyCode::Left),
        "Right" => plain(terminput::KeyCode::Right),
        "Ctrl+c" => plain(terminput::KeyCode::Char('c')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+Enter" => plain(terminput::KeyCode::Enter).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+v" => plain(terminput::KeyCode::Char('v')).modifiers(terminput::KeyModifiers::CTRL),
        "Cmd+c" => plain(terminput::KeyCode::Char('c')).modifiers(terminput::KeyModifiers::SUPER),
        "Cmd+v" => plain(terminput::KeyCode::Char('v')).modifiers(terminput::KeyModifiers::SUPER),
        "Ctrl+d" => plain(terminput::KeyCode::Char('d')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+f" => plain(terminput::KeyCode::Char('f')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+n" => plain(terminput::KeyCode::Char('n')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+p" => plain(terminput::KeyCode::Char('p')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+s" => plain(terminput::KeyCode::Char('s')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+z" => plain(terminput::KeyCode::Char('z')).modifiers(terminput::KeyModifiers::CTRL),
        "Ctrl+Shift+z" => plain(terminput::KeyCode::Char('z'))
            .modifiers(terminput::KeyModifiers::CTRL | terminput::KeyModifiers::SHIFT),
        "Escape" => plain(terminput::KeyCode::Esc),
        "Enter" => plain(terminput::KeyCode::Enter),
        "Backspace" => plain(terminput::KeyCode::Backspace),
        "Alt+Backspace" => alt(terminput::KeyCode::Backspace),
        "Tab" => plain(terminput::KeyCode::Tab),
        "Shift+Tab" => plain(terminput::KeyCode::Tab).modifiers(terminput::KeyModifiers::SHIFT),
        "Up" => plain(terminput::KeyCode::Up),
        "Down" => plain(terminput::KeyCode::Down),
        "PageUp" => plain(terminput::KeyCode::PageUp),
        "PageDown" => plain(terminput::KeyCode::PageDown),
        "Ctrl+Space" => {
            plain(terminput::KeyCode::Char(' ')).modifiers(terminput::KeyModifiers::CTRL)
        }
        "Space" => plain(terminput::KeyCode::Char(' ')),
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

fn palette_key(entry: &str) -> char {
    PALETTE
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .find(|(_, label)| label.trim() == entry)
        .map(|(key, _)| *key)
        .unwrap_or_else(|| panic!("no palette entry {entry:?}"))
}

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
fn terminal_is_in(_world: &mut VardeWorld, _dir: String) {}

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
        // Without this, cucumber passes undefined or skipped steps, so a mistyped step reads green.
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

#[given(expr = "{string} is changed at revision {string}")]
fn changed_at_revision(world: &mut VardeWorld, path: String, _revision: String) {
    world.state.repo = Some(vec![GitFile {
        path,
        status: GitStatus::Modified,
    }]);
}

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
        "conflicts" => Pane::Conflicts,
        "Cheatsheet" => Pane::Cheatsheet,
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

#[given(expr = "I click in the {word} pane")]
#[when(expr = "I click in the {word} pane")]
fn click_pane(world: &mut VardeWorld, pane: String) {
    let pane = parse_pane(&pane);
    world.send(Event::ClickPane(pane));
    world.send(Event::ClickThrough { pane, at: POINTER });
}

#[when(expr = "I click at line {int} column {int} in the editor")]
fn click_text(world: &mut VardeWorld, line: usize, column: usize) {
    world.click(Pane::Editor, (line, column), terminput::KeyModifiers::NONE);
}

const JUMP: terminput::KeyModifiers = terminput::KeyModifiers::CTRL;

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

#[then(expr = "the cursor is on line {int}")]
fn cursor_should_be_on_line(world: &mut VardeWorld, line: usize) {
    let path = world.state.current_buffer.clone().expect("an open buffer");
    assert_eq!(
        world.state.buffers.get(&path).expect("an open buffer").line,
        line
    );
}

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

#[given(expr = "the minimap is turned off")]
fn minimap_turned_off(world: &mut VardeWorld) {
    world.state.minimap = false;
}

#[then(expr = "the minimap is hidden")]
fn minimap_hidden(world: &mut VardeWorld) {
    assert_eq!(varde::minimap::mirrored(&world.state), None);
    assert_eq!(varde::minimap::width(&world.state), 0);
}

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

#[given(expr = "a Debug adapter for {string} is configured")]
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
    let installer = adapter
        .install
        .get("macos")
        .and_then(|install| tools::installer(install));
    for command in std::iter::once(adapter.command.clone()).chain(installer) {
        world.on_path.insert(command.clone());
        world.state.commands_on_path.insert(command);
    }
    let adapter = startup::Adapter {
        server: None,
        plugin: None,
        ..adapter
    };
    world.state.adapters.insert(language, adapter);
}

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

#[when(expr = "I click the line number of line {int}")]
fn click_line_number(world: &mut VardeWorld, line: usize) {
    let panes = world.panes();
    let (_, row) = pointer_at(&world.state, &panes, Pane::Editor, (line, 1));
    let column = panes.editor.x + 2 + layout::BREAKPOINT_COLUMN;
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, row);
    world.report(mouse::Kind::LeftUp, column, row);
}

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

#[given(expr = "the workspace is a Bare workspace")]
fn workspace_is_bare(world: &mut VardeWorld) {
    world.startup.sidecar = Some(PathBuf::from(SIDECAR));
    world.state.sidecar = Some(PathBuf::from(SIDECAR));
}

#[when(expr = "Varde starts again in the same folder")]
fn starts_again_in_same_folder(world: &mut VardeWorld) {
    if world.startup.sidecar.is_some() {
        world.startup.sidecar = Some(PathBuf::from(format!("{SIDECAR}1")));
        world.startup.state_json = None;
    }
    varde_starts(world);
}

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

#[given(expr = "the terminal holds {int} shell(s)")]
#[when(expr = "the terminal holds {int} shell(s)")]
fn terminal_holds(world: &mut VardeWorld, shells: usize) {
    world.state.terminals.resize(shells, varde::Shell::Idle);
}

#[given(expr = "terminal {int} is running a process")]
fn terminal_busy(world: &mut VardeWorld, split: usize) {
    world.state.terminals[split - 1] = varde::Shell::Busy;
}

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

#[given(expr = "I type {string}")]
#[when(expr = "I type {string}")]
fn type_text(world: &mut VardeWorld, text: String) {
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

#[given(expr = "I paste {string} into the editor")]
#[when(expr = "I paste {string} into the editor")]
fn paste_into_editor(world: &mut VardeWorld, text: String) {
    match keys::on_paste(&world.state, &world.drafts, text.replace("\\n", "\n")) {
        keys::Pasted::ToBuffer(event) => world.send(event),
        _ => panic!("the buffer did not take the paste"),
    }
}

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
    if named_key(&keys).is_some() {
        return route_key(world, &keys, 0);
    }
    for key in keys.chars() {
        world.send(Event::EditorKey(key));
    }
}

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

#[when(expr = "I drag past the {word} of the {word} pane")]
fn drag_past(world: &mut VardeWorld, side: String, pane: String) {
    let area = match parse_pane(&pane) {
        Pane::Tree => world.panes().tree,
        Pane::Editor => world.panes().editor,
        Pane::Ai | Pane::Cheatsheet => world.panes().ai,
        Pane::Output => world.panes().output,
        Pane::Terminal | Pane::Variables => world.panes().terminal,
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts => world.panes().corner,
        Pane::Evaluator => world.panes().evaluator,
    };
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

#[then(expr = "the selection holds {string}")]
#[then(expr = "the selection is {string}")]
fn selection_holds(world: &mut VardeWorld, text: String) {
    assert_eq!(world.state.selected_text(), Some(text));
}

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

/// cucumber trims the newlines around a docstring, so the trailing line break is added back here.
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

#[given(expr = "I press Backspace in the editor")]
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

#[then(expr = "the command line is open")]
fn command_line_open(world: &mut VardeWorld) {
    assert_eq!(world.drafts.command.as_deref(), Some(""));
}

#[then(expr = "the command line is not open")]
fn command_line_closed(world: &mut VardeWorld) {
    assert_eq!(world.drafts.command, None);
}

#[given(expr = "I open {string}")]
#[when(expr = "I open {string}")]
fn open_named(world: &mut VardeWorld, path: String) {
    let absolute = abs(world, &path);
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

#[given("the project contains:")]
fn project_contains(world: &mut VardeWorld, step: &Step) {
    let files: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    world.indexable = files;
}

#[given(expr = "the project gains {string}")]
fn project_gains(world: &mut VardeWorld, path: String) {
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
    let mut actual: Vec<&str> = varde::filter::matches(&world.state).collect();
    let mut sorted = expected.clone();
    actual.sort();
    sorted.sort();
    assert_eq!(actual, sorted);
}

#[then(expr = "the filtered files include {string}")]
fn filtered_include(world: &mut VardeWorld, path: String) {
    assert!(varde::filter::matches(&world.state).any(|found| found == path));
}

#[then(expr = "the filtered files are empty")]
fn filtered_empty(world: &mut VardeWorld) {
    assert_eq!(varde::filter::matches(&world.state).next(), None);
}

#[then(expr = "the best match is {string}")]
#[then(expr = "the completion is {string}")]
fn best_match_is(world: &mut VardeWorld, path: String) {
    assert_eq!(varde::filter::chosen(&world.state), Some(path.as_str()));
}

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

#[given(expr = "I step {word} through the matches")]
#[when(expr = "I step {word} through the matches")]
fn step_through_matches(world: &mut VardeWorld, way: String) {
    world.send(Event::StepFilter(match way.as_str() {
        "down" => Direction::Down,
        "up" => Direction::Up,
        other => panic!("no way {other:?}"),
    }));
}

#[then(expr = "no row is highlighted")]
fn no_row_highlighted(world: &mut VardeWorld) {
    assert_eq!(tree::highlighted(&world.state), None);
}

#[then(expr = "the highlighted row is {string}")]
fn highlighted_row_is(world: &mut VardeWorld, path: String) {
    assert_eq!(tree::highlighted(&world.state), Some(abs(world, &path)));
}

#[when(expr = "I accept the filter")]
fn accept_filter(world: &mut VardeWorld) {
    world.send(Event::AcceptFilter);
}

#[then(expr = "the tree filter state is {string}")]
fn tree_filter_state(world: &mut VardeWorld, expected: String) {
    assert_eq!(varde::filter::view_state(&world.state), expected);
}

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
}

#[given("searching does not finish yet")]
fn searching_is_held(world: &mut VardeWorld) {
    world.holding_searches = true;
}

#[when("the search finishes")]
fn search_finishes(world: &mut VardeWorld) {
    let request = world.held_search.take().expect("a search in flight");
    world.search_finishes(request);
}

#[then("the search spinner shows")]
fn search_spinner_shows(world: &mut VardeWorld) {
    assert!(varde::search::running(&world.state).is_some());
}

#[then("the search spinner is gone")]
fn search_spinner_is_gone(world: &mut VardeWorld) {
    assert_eq!(varde::search::running(&world.state), None);
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

#[given(expr = "I type {string} into the in-file search")]
#[when(expr = "I type {string} into the in-file search")]
#[given(expr = "I type {string} into the replace box")]
#[when(expr = "I type {string} into the replace box")]
fn type_into_find(world: &mut VardeWorld, query: String) {
    for key in query.chars() {
        route_key(world, &key.to_string(), 0);
    }
}

#[then(expr = "the in-file search query is {string}")]
fn find_query_is(world: &mut VardeWorld, expected: String) {
    let find = world.state.find.as_ref().expect("a search that is on");
    assert_eq!(find.query.shown(), expected);
}

fn find_keys(world: &VardeWorld) -> varde::FindKeys {
    world.state.find.as_ref().expect("a search that is on").keys
}

fn find_icon(name: &str) -> varde::FindIcon {
    match name {
        "case" => varde::FindIcon::Case,
        "word" => varde::FindIcon::Word,
        "replace" => varde::FindIcon::Replace,
        "replace all" => varde::FindIcon::ReplaceAll,
        other => panic!("no icon {other:?}"),
    }
}

#[then(expr = "the keyboard is in the in-file search query")]
fn keyboard_in_query(world: &mut VardeWorld) {
    assert_eq!(find_keys(world), varde::FindKeys::Query);
}

#[then(expr = "the keyboard is on the in-file search's {string} icon")]
fn keyboard_on_icon(world: &mut VardeWorld, icon: String) {
    assert_eq!(find_keys(world), varde::FindKeys::Icon(find_icon(&icon)));
}

#[then(expr = "the keyboard is not in the in-file search")]
fn keyboard_not_in_find(world: &mut VardeWorld) {
    assert_eq!(find_keys(world), varde::FindKeys::Away);
}

#[then(expr = "the {word} toggle is {word}")]
fn toggle_is(world: &mut VardeWorld, icon: String, shown: String) {
    let find = world.state.find.as_ref().expect("a search that is on");
    let lit = find.lit(find_icon(&icon));
    assert_eq!(lit, shown == "lit", "the toggle is lit: {lit}");
}

#[when(expr = "I click the word toggle in the replace box")]
fn click_replace_word(world: &mut VardeWorld) {
    let spot = layout::replace_box(world.panes().editor);
    let (_, _, at) = layout::replace_toggles(spot)
        .into_iter()
        .find(|(icon, _, _)| *icon == varde::FindIcon::Word)
        .expect("a word toggle");
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, at.x, at.y);
    world.report(mouse::Kind::LeftUp, at.x, at.y);
}

#[then(expr = "the replace box is open")]
fn replace_box_open(world: &mut VardeWorld) {
    assert!(matches!(find_keys(world), varde::FindKeys::Replace(_)));
}

#[then(expr = "the replace box is not open")]
fn replace_box_closed(world: &mut VardeWorld) {
    assert!(!matches!(find_keys(world), varde::FindKeys::Replace(_)));
}

#[given(expr = "I click the {string} icon on the in-file search line")]
#[when(expr = "I click the {string} icon on the in-file search line")]
fn click_find_icon(world: &mut VardeWorld, icon: String) {
    let icon = find_icon(&icon);
    let editor = world.panes().editor;
    let mut column = editor.x + 1;
    for (text, piece) in varde::find_line(&world.state) {
        if piece == Some(icon) {
            break;
        }
        column += UnicodeWidthStr::width(text.as_str()) as u16;
    }
    let row = editor.bottom() - 1;
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, column, row);
    world.report(mouse::Kind::LeftUp, column, row);
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

fn column<'a>(headers: &[String], row: &'a [String], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .position(|header| header == name)
        .map(|index| row[index].as_str())
}

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

#[given(expr = "{string} has an unclaimed hunk")]
fn file_has_an_unclaimed_hunk(world: &mut VardeWorld, path: String) {
    let full = world.state.root.join(&path);
    world.known_files.insert(full.clone());
    world.held.insert(full.clone(), "a\nb\nc\n".to_string());
    world.files.insert(full, "a\nX\nc\n".to_string());
    world.mark_modified(&path);
    world.recompute_file_hunks();
}

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

#[then(expr = "the band has no claim")]
fn band_has_no_claim(world: &mut VardeWorld) {
    assert!(story::current_step(&world.state).is_none());
}

#[then(expr = "no prediction is offered")]
fn no_prediction_is_offered(world: &mut VardeWorld) {
    let prediction = story::current_step(&world.state).and_then(|step| step.prediction.as_ref());
    assert!(prediction.is_none());
}

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

#[given(expr = "I walked to step {int} of {string}")]
#[when(expr = "I walk to step {int} of {string}")]
fn walk_to_step(world: &mut VardeWorld, target: usize, name: String) {
    enter_story(world, name);
    for _ in 1..target {
        world.send(Event::StepStory(Direction::Right));
    }
}

fn nth_step(world: &VardeWorld, index: usize) -> &story::Step {
    let Some(story::Walking::Story { story, .. }) = world.state.walking else {
        panic!("walking a story");
    };
    let story::Set::Loaded(artifact) = &world.state.story_set else {
        panic!("no story set loaded");
    };
    &artifact.stories[story].steps[index - 1]
}

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

#[given(expr = "Varde has not yet read what the sites hold")]
fn sites_not_yet_read(world: &mut VardeWorld) {
    world.hold_site_texts = true;
}

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

#[then(expr = "the editor mode label is {string}")]
fn editor_mode_label_is(world: &mut VardeWorld, expected: String) {
    let label = varde::mode_label(&world.state, current_buffer(world));
    assert_eq!(label, expected);
}

#[then(expr = "the step view state is {string}")]
fn step_view_state_is(world: &mut VardeWorld, expected: String) {
    assert_eq!(story::mark(&world.state).as_str(), expected);
}

#[then(expr = "line {int} of {string} is dimmed")]
fn line_is_dimmed(world: &mut VardeWorld, line: u32, path: String) {
    assert!(!story::mark(&world.state).covers(&path, line));
}

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

#[then(expr = "the refusal names step {string} as {string}")]
fn refusal_names_step(world: &mut VardeWorld, id: String, fault: String) {
    let story::Set::Refused { because } = &world.state.story_set else {
        panic!("the set was not refused: {:?}", world.state.story_set);
    };
    assert!(because.contains(&format!("{id}: {fault}")), "{because}");
}

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

#[given(expr = "{string} has commits {string} does not")]
fn has_commits_the_other_does_not(_world: &mut VardeWorld, _branch: String, _of: String) {}

#[given(expr = "the repository resolves no default branch")]
fn no_default_branch_resolves(world: &mut VardeWorld) {
    world.origin_head = None;
    world.upstream_branch = None;
    world.default_branch_config = None;
    world.probes.clear();
}

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

#[given(expr = "git is not installed")]
fn git_is_not_installed(world: &mut VardeWorld) {
    world.git_on_path = false;
    world.tell_core();
}

#[then(expr = "the terminal has cloned {string} into the Sidecar")]
fn terminal_has_cloned(world: &mut VardeWorld, url: String) {
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
    assert!(
        command.contains("echo $? >") && command.contains(&sentinel.display().to_string()),
        "the sentinel carries no exit status: {command}"
    );
}

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

#[then(expr = "the terminal has run nothing but the clone")]
fn terminal_ran_only_the_clone(world: &mut VardeWorld) {
    let [clone] = world.executed.as_slice() else {
        panic!("executed: {:?}", world.executed);
    };
    assert!(clone.contains("git clone"), "not a clone: {clone}");
}

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
    world.recompute_file_hunks();
}

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

// `And I ran` can follow a Given, so this step is registered as a Given as well as a When
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

fn scope(name: &str) -> Scope {
    match name {
        "workspace" => Scope::Workspace,
        "review" => Scope::Review,
        other => panic!("unknown scope {other:?}"),
    }
}

#[derive(Debug)]
struct Analysis {
    scope: Scope,
    generation: u64,
    files: Option<Vec<String>>,
    base: Option<String>,
}

fn figures(step: &Step, unparsed: usize) -> Figures {
    figures_from(step, unparsed, "")
}

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

#[given(expr = "no test coverage was read")]
fn no_test_coverage(_world: &mut VardeWorld) {}

#[given(expr = "every file in the workspace is in a language the analyser does not handle")]
fn nothing_analysable(_world: &mut VardeWorld) {}

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

#[given(expr = "the figure has not gone stale")]
fn figure_has_not_gone_stale(world: &mut VardeWorld) {
    assert_eq!(risk::view_state(&world.state), "computed");
}

#[when(expr = "I ask for the figures to be recomputed")]
fn ask_for_recompute(world: &mut VardeWorld) {
    world.send(Event::RecomputeRisk);
}

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

#[then(expr = "{int} analysis is in flight")]
fn analyses_in_flight(world: &mut VardeWorld, expected: usize) {
    assert_eq!(usize::from(world.state.risk.in_flight()), expected);
}

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
    let drawn = PALETTE
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .find(|(_, label)| label.trim() == entry)
        .map(|(_, label)| *label)
        .expect("the entry");
    assert_eq!(drawn, entry, "the entry is indented under another");
}

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

#[when(expr = "I click the Risk list row {string}")]
fn click_risk_row(world: &mut VardeWorld, function: String) {
    let index = risk_row(world, &function);
    world.send(Event::ClickRiskRow(index));
}

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

#[when(expr = "the sentinel {string} appears")]
fn sentinel_appears(world: &mut VardeWorld, path: String) {
    let path = abs(world, &path);
    world.files.insert(path.clone(), String::new());
    world.send(Event::FilesAppeared(vec![(path, tree::Kind::File)]));
}

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

#[given(expr = "{string} had uncommitted changes before the loop started")]
fn had_uncommitted_changes(world: &mut VardeWorld, file: String) {
    let committed = world.tree().get(&file).cloned().unwrap_or_default();
    let mine = format!("{committed}my own uncommitted edit\n");
    world.committed.insert(file.clone(), committed);
    world.mine.insert(file.clone(), mine.clone());
    world.write_tree(&file, &mine);
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

#[then(expr = "nothing was committed")]
fn nothing_was_committed(world: &mut VardeWorld) {
    for command in world.executed.iter().chain(world.tests_run.iter()) {
        assert!(!command.contains("commit"), "committed: {command:?}");
    }
}

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

#[then(expr = "the AI pane was sent a prompt asking for splits that stand on their own")]
fn prompt_asks_for_meaningful_splits(world: &mut VardeWorld) {
    any_prompt_contains(world, "the symptom, not the goal");
}

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

#[then(expr = "no prompt sent to the AI contains {string}")]
fn no_prompt_contains(world: &mut VardeWorld, needle: String) {
    for prompt in ai_sends(world) {
        assert!(
            !prompt.contains(&needle),
            "prompt holds {needle:?}:\n{prompt}"
        );
    }
}

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
        world.state.risk.before.clone(),
    );
}

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

#[then("the Gate has not been evaluated")]
fn the_gate_has_not_been_evaluated(world: &mut VardeWorld) {
    assert_eq!(world.state.refactor.stopped, None);
    assert!(world.restores.is_empty(), "restores: {:?}", world.restores);
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

#[then(expr = "the working tree holds no change from Iteration {int}")]
fn tree_holds_no_change(world: &mut VardeWorld, iteration: u32) {
    let snapshot = world
        .snapshots
        .get(&iteration)
        .cloned()
        .expect("a snapshot for the Iteration");
    assert_eq!(world.tree(), snapshot);
}

#[then(expr = "no snapshot was taken")]
fn no_snapshot_was_taken(world: &mut VardeWorld) {
    assert!(
        world.snapshots.is_empty(),
        "snapshots: {:?}",
        world.snapshots.keys().collect::<Vec<_>>()
    );
}

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

#[then(expr = "the review risk base revision is the revision the review diff is measured from")]
fn review_risk_base_is_the_diffs(world: &mut VardeWorld) {
    assert_eq!(
        last_analysis(world).base.as_deref(),
        review::base(&world.state),
        "the delta chose a base of its own"
    );
}

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
        response: "tsserver/response".to_string(),
    });
}

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

fn sent(world: &VardeWorld, language: &str) -> Vec<Value> {
    world
        .lsp_sent
        .iter()
        .filter(|(named, _)| named == language)
        .map(|(_, message)| message.clone())
        .collect()
}

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

#[when(expr = "the language server for {string} replies to request {int} with:")]
fn server_replies_to_id(world: &mut VardeWorld, language: String, id: i64, step: &Step) {
    let result: Value = serde_json::from_str(step.docstring().expect("docstring")).expect("json");
    world.lsp_replies(
        &language,
        json!({"jsonrpc": "2.0", "id": id, "result": result}),
    );
}

#[given(expr = "the language server for {string} answers the hover with:")]
#[when(expr = "the language server for {string} answers the hover with:")]
fn server_answers_the_hover(world: &mut VardeWorld, language: String, step: &Step) {
    let result: Value = serde_json::from_str(step.docstring().expect("docstring")).expect("json");
    world.lsp_replies(
        &language,
        json!({"jsonrpc": "2.0", "id": asked(world, &language, "textDocument/hover"), "result": result}),
    );
}

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

fn asked(world: &VardeWorld, language: &str, method: &str) -> Value {
    sent(world, language)
        .iter()
        .rev()
        .find(|message| message["method"] == method)
        .unwrap_or_else(|| panic!("{language} was never sent a {method} request"))["id"]
        .clone()
}

/// LSP counts lines from zero and columns in UTF-16 units
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

#[then(expr = "the hover shows {string}")]
fn hover_shows(world: &mut VardeWorld, text: String) {
    assert!(
        words(&hover_rows(world).join(" ")).contains(&words(&text)),
        "the hover shows: {:?}",
        hover_rows(world)
    );
}

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

#[then(expr = "the candidate list is {int} rows tall")]
fn candidate_list_rows(world: &mut VardeWorld, rows: usize) {
    assert_eq!(candidate_list(world).rows(), rows);
}

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

#[then(expr = "the editor never said {string}")]
fn editor_never_said(world: &mut VardeWorld, notice: String) {
    assert!(
        !world.notices.contains(&notice),
        "the editor says: {:?}",
        world.notices
    );
}

#[then(expr = "the message names the file {string} at line {int}")]
fn message_names_file_and_line(world: &mut VardeWorld, file: String, line: usize) {
    let prefix = format!("{file}:{line}: ");
    assert!(
        world.named.iter().any(|named| named.starts_with(&prefix)),
        "the messages named: {:?}",
        world.named
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

#[then(expr = "{string} on disk still holds:")]
fn on_disk_still_holds(world: &mut VardeWorld, path: String, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    assert_eq!(
        world.files.get(&abs(world, &path)).map(String::as_str),
        Some(expected)
    );
}

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

#[then(expr = "the diagnostic box says {string}")]
fn diagnostic_box_says(world: &mut VardeWorld, message: String) {
    let (lines, _) = lsp::pointed(&world.state).expect("no diagnostic box is shown");
    assert!(
        words(&lines.join(" ")).contains(&words(&message)),
        "the box says: {lines:?}"
    );
}

#[then(expr = "the diagnostic box sits beside line {int}")]
fn diagnostic_box_beside(world: &mut VardeWorld, line: usize) {
    let (_, placement) = lsp::pointed(&world.state).expect("no diagnostic box is shown");
    assert_eq!(placement.from, line + 1);
}

#[then("no diagnostic box is shown")]
fn no_diagnostic_box(world: &mut VardeWorld) {
    assert_eq!(lsp::pointed(&world.state).map(|(lines, _)| lines), None);
}

#[given(expr = "the language server for {string} has published no diagnostics for {string}")]
fn has_published_nothing(world: &mut VardeWorld, _language: String, path: String) {
    let full = abs(world, &path);
    assert!(
        !world.state.diagnostics.contains_key(&full),
        "{path} already carries: {:?}",
        world.state.diagnostics.get(&full)
    );
}

#[given(expr = "the language server for {string} publishes no diagnostics for {string}")]
#[when(expr = "the language server for {string} publishes no diagnostics for {string}")]
fn publishes_nothing(world: &mut VardeWorld, language: String, path: String, step: &Step) {
    publish(world, &language, &path, None, step);
}

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

#[given(expr = "the edge resolved {string} to {string}")]
fn edge_resolved(world: &mut VardeWorld, name: String, value: String) {
    world.workspace_facts.insert(name, value);
    started(world);
    world.tell_core();
}

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

fn kind(word: &str) -> tools::Kind {
    match word {
        "server" => tools::Kind::Server,
        "formatter" => tools::Kind::Formatter,
        "requirement" => tools::Kind::Requirement,
        "speech" => tools::Kind::Speech,
        other => panic!("no group {other:?}"),
    }
}

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

#[then(expr = "the {word} row for {string} needs the installer {string}")]
fn row_needs_installer(world: &mut VardeWorld, group: String, name: String, expected: String) {
    match tool_row(world, kind(&group), &name).availability {
        tools::Availability::NeedsInstaller { installer } => assert_eq!(installer, expected),
        other => panic!("{name} reads {}", other.as_str()),
    }
}

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

#[then(expr = "the row for {string} says it cannot do {string}")]
fn row_says_it_cannot(world: &mut VardeWorld, language: String, expected: String) {
    match server_row(world, &language).availability {
        tools::Availability::Partial { without } => assert_eq!(without, expected),
        other => panic!("{language} reads {}", other.as_str()),
    }
}

#[given(expr = "Varde was built for {string}")]
fn built_for(world: &mut VardeWorld, os: String) {
    world.state.os = os.clone();
    world.startup.os = os;
}

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

#[then(expr = "the shell pane runs a command mentioning {string}")]
fn shell_pane_runs_mentioning(world: &mut VardeWorld, fragment: String) {
    assert!(
        matches!(world.executed.as_slice(), [run] if run.contains(&fragment)),
        "executed {:?}, which does not mention {fragment:?}",
        world.executed
    );
}

#[when(expr = "the install reports the exit status {string}")]
fn install_reports(world: &mut VardeWorld, status: String) {
    let sentinel = install_sentinel(world);
    world.files.insert(sentinel.clone(), format!("{status}\n"));
    world.send(Event::FilesAppeared(vec![(sentinel, tree::Kind::File)]));
}

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

#[given(expr = "a language server for {string} failed to start")]
fn server_failed_to_start(world: &mut VardeWorld, language: String) {
    started(world);
    world.lsp_is_gone(&language, Gone::FailedToStart);
}

#[then(expr = "a language server is started for {string}")]
fn server_is_started_for(world: &mut VardeWorld, language: String) {
    servers_started_for(world, 1, language);
}

#[then(expr = "no language server is started")]
fn no_language_server_is_started(world: &mut VardeWorld) {
    assert!(
        world.lsp_started.is_empty(),
        "a server was started: {:?}",
        world.lsp_started
    );
}

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

/// A Gherkin {string} carries `\n` as two characters, so a real newline needs a docstring
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

#[given(expr = "the formatter answers with:")]
#[when(expr = "the formatter answers with:")]
fn formatter_answers_with(world: &mut VardeWorld, step: &Step) {
    let text = step.docstring().expect("docstring").trim_matches('\n');
    formatter_answered(world, format::Answer::Done(text.to_string()));
}

/// A Gherkin docstring always ends in a newline, so it cannot express empty stdout
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

#[then(expr = "the corner is empty")]
fn corner_should_be_empty(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Hidden);
}

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

#[given(expr = "the cursor history holds {int} places")]
fn history_holds_places(world: &mut VardeWorld, count: usize) {
    world.state.visits = (0..count).map(filler_visit).collect();
    world.state.history_selection = count;
}

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

#[then(expr = "Cursor history row {int} shows the excerpt {string}")]
fn history_row_excerpt(world: &mut VardeWorld, row: usize, expected: String) {
    let visit = history_row(world, row);
    assert_eq!(
        varde::history::excerpt(&visit.text, visit.column, varde::history::WORDS),
        expected
    );
}

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

#[when(expr = "I click Cursor history row {int}")]
fn click_history_row(world: &mut VardeWorld, row: usize) {
    world.click(Pane::History, (row, 1), terminput::KeyModifiers::NONE);
}

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

#[given(expr = "the selection covers {string}")]
fn selection_covers(world: &mut VardeWorld, text: String) {
    let lines: Vec<String> = current_buffer(world)
        .shown()
        .lines()
        .map(str::to_string)
        .collect();
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

const SAID_MS: u32 = 1_000;

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

fn sound_reached(world: &mut VardeWorld, at_ms: u32) {
    let offsets = world.stream.clone();
    world.send(Event::Speaking { at_ms, offsets });
}

const PART_WAY: u32 = 300;

fn reading_now(world: &VardeWorld) -> &reading::Reading {
    world.state.reading.as_ref().expect("a reading in flight")
}

#[given(expr = "the current utterance is {int}")]
fn current_utterance_starts(world: &mut VardeWorld, at: usize) {
    let at_ms = world.stream[at - 1];
    sound_reached(world, at_ms);
}

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

#[when("the play control is pressed")]
fn play_control_pressed(world: &mut VardeWorld) {
    world.send(Event::PaneAction(reading::PLAY_PAUSE));
}

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

#[then("exactly one reading is in flight")]
fn exactly_one_reading(world: &mut VardeWorld) {
    assert!(world.state.reading.is_some());
    assert_eq!(world.players, 1);
}

#[then("no sound is being played")]
fn nothing_is_playing(world: &mut VardeWorld) {
    assert_eq!(world.speaking, None);
}

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

/// Inverted by hand: calling reading::duration_scale would hide a core sending the multiplier
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

#[then(expr = "the reading in flight is still at speed {float}")]
#[then(expr = "the reading is at speed {float}")]
fn reading_is_at_speed(world: &mut VardeWorld, speed: f32) {
    assert_eq!(world.spoken_at, Some(speed));
}

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

#[then("every transport action is reachable from the keyboard")]
fn every_transport_action_has_a_key(world: &mut VardeWorld) {
    open_buffer_plain(world, "guide.md".to_string());
    let commands = [
        (reading::PLAY_PAUSE, ":pause"),
        (reading::PREVIOUS, ":prev"),
        (reading::NEXT, ":next"),
        (reading::STOP, ":stop"),
        (reading::SPEED, ":speed 1.25"),
    ];
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

fn load_debug_config(world: &mut VardeWorld) {
    let (loaded, _, _) = startup::start(&world.startup).expect("the scenario's config starts");
    world.state.launches = loaded.launches;
    world.state.runs = loaded.runs;
    for (language, adapter) in loaded.adapters {
        world.state.adapters.entry(language).or_insert(adapter);
    }
}

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

fn stopped_at(world: &mut VardeWorld, thread: i64, file: &str, line: usize, reason: &str) {
    plant_the_programs_locals(world);
    let path = abs(world, file);
    let stack = world
        .dap
        .stacks
        .entry(thread)
        .or_insert_with(|| vec![("main".to_string(), path.clone(), line, String::new())]);
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
    world.dap.since_pause = world.dap.sent.len();
}

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

#[then(expr = "the launch palette, opened without restarting, offers {string}")]
fn launch_palette_without_restarting_offers(world: &mut VardeWorld, name: String) {
    world.send(Event::FallbackBinding);
    route_key(world, &palette_key("Launch").to_string(), 0);
    launch_palette_offers(world, name);
}

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
    if world.state.os.is_empty() {
        world.state.os = "macos".to_string();
    }
    world.send(Event::FallbackBinding);
    route_key(world, &palette_key("Tools").to_string(), 0);
}

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

#[then(expr = "the Variables title says {string}")]
#[then(expr = "the Transport says {string}")]
fn variables_title_says(world: &mut VardeWorld, said: String) {
    assert_eq!(varde::debug::title(&world.state), said);
}

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

fn repause(world: &mut VardeWorld) {
    let stop = world.dap.stopped.clone().expect("a pause");
    adapter_event(world, stop);
    world.dap.since_pause = world.dap.sent.len();
}

fn plant_member(world: &mut VardeWorld, member: Value) {
    if world.dap.scopes.is_empty() {
        world.dap.scopes.push(("Locals".to_string(), LOCALS));
    }
    let held = world.dap.members.entry(LOCALS).or_default();
    held.retain(|already| already["name"] != member["name"]);
    held.push(member);
    repause(world);
}

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

#[when(expr = "I open the next page of {string}")]
fn open_next_page(world: &mut VardeWorld, name: String) {
    let index = next_page_row(world, &name).unwrap_or_else(|| panic!("no next page of {name:?}"));
    world.state.focus = Pane::Variables;
    while world.state.variables_selection < index {
        world.send(Event::MoveSelection(Direction::Down));
    }
    world.send(Event::Activate);
}

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

#[given(expr = "the Debug adapter asks to run {string} in a terminal")]
#[when(expr = "the Debug adapter asks to run {string} in a terminal")]
fn adapter_asks_for_a_terminal(world: &mut VardeWorld, program: String) {
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

#[given(expr = "the border between the Variables and the Program output is at column {int}")]
fn border_at_column(world: &mut VardeWorld, column: u16) {
    program_running(world);
    let right = debug_group_right(world);
    world.state.output_width = Some(u32::from(right.saturating_sub(column)));
    world.tell_core();
}

fn debug_group_right(world: &VardeWorld) -> u16 {
    let panes = world.panes();
    panes.terminal.right().max(panes.output.right())
}

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

#[when(expr = "the program prints {string}")]
fn program_prints(world: &mut VardeWorld, text: String) {
    program_running(world);
    world.send(Event::OutputSpoke);
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

#[given("the Debug session has ended")]
#[when("the Debug session has ended")]
fn session_has_ended(world: &mut VardeWorld) {
    adapter_sends_event(world, "terminated".to_string());
    assert!(world.state.debug.is_none(), "the session is still going");
    world.dap.spawned.clear();
}

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

fn chip(world: &VardeWorld, name: &str) -> varde::Chip {
    varde::debug::strip_transport(&world.state)
        .into_iter()
        .chain(varde::debug::transport(&world.state))
        .find(|offered| offered.name == name)
        .or_else(|| row_chip(world, name))
        .unwrap_or_else(|| panic!("no {name:?} Chip"))
}

fn row_chip(world: &VardeWorld, name: &str) -> Option<varde::Chip> {
    varde::debug::row_chips(&world.state, world.state.variables_selection)
        .into_iter()
        .find(|offered| offered.name == name)
}

#[when(expr = "I click the row's {string} Chip")]
fn click_the_rows_chip(world: &mut VardeWorld, name: String) {
    click_variables_row_chip(world, &name);
}

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

#[when(expr = "{int} seconds pass")]
fn seconds_pass(world: &mut VardeWorld, seconds: u64) {
    for _ in 0..(seconds * 1_000 / 80) {
        world.send(Event::Tick);
    }
}

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

fn transport_labels(world: &VardeWorld) -> Vec<String> {
    let chips = varde::debug::strip_transport(&world.state);
    let area = varde::transport_area(&world.state, world.panes().strip());
    layout::chip_labels(&chips, area.width, layout::CORNER_TITLE)
}

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

fn inline_values(world: &VardeWorld) -> BTreeMap<usize, Vec<varde::debug::Inline>> {
    let (name, source) = match world.state.current_buffer.as_ref() {
        Some(path) => (
            path.file_name().unwrap_or_default().to_string_lossy(),
            current_buffer(world).shown().to_string(),
        ),
        None => return BTreeMap::new(),
    };
    let columns = varde::fits_in(&world.state, &world.panes()).2;
    varde::debug::inline(
        &world.state,
        &varde::highlight::highlight(&name, &source),
        columns,
    )
}

fn inline_value(world: &VardeWorld, name: &str) -> varde::debug::Inline {
    inline_values(world)
        .into_values()
        .flatten()
        .find(|value| value.name == name)
        .unwrap_or_else(|| panic!("no Inline value for {name:?}"))
}

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

#[then(expr = "{string} carries no Inline values")]
fn file_carries_no_inline_values(world: &mut VardeWorld, path: String) {
    let path = abs(world, &path);
    world.apply(vec![Effect::OpenAt {
        path,
        at: Place { line: 1, column: 1 },
    }]);
    assert!(inline_values(world).is_empty(), "values are still drawn");
}

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

fn window(world: &VardeWorld) -> layout::Area {
    world
        .state
        .evaluator_at
        .expect("the Evaluator has a place on the screen")
}

fn editor_row(world: &VardeWorld, line: u16) -> u16 {
    world.panes().editor.y + line
}

fn drag_window(world: &mut VardeWorld, from: (u16, u16), to: (u16, u16)) {
    world.pointer = mouse::Pointer::default();
    world.report(mouse::Kind::LeftDown, from.0, from.1);
    world.report(mouse::Kind::LeftDrag, to.0, to.1);
    world.report(mouse::Kind::LeftUp, to.0, to.1);
}

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

#[then(expr = "the cursor in the Snippet is at line {int} column {int}")]
fn cursor_in_the_snippet(world: &mut VardeWorld, line: usize, column: usize) {
    let snippet = &evaluator(world).snippet;
    assert_eq!((snippet.line, snippet.column), (line, column));
}

#[then("the Snippet is:")]
fn the_snippet_is_block(world: &mut VardeWorld, step: &Step) {
    let expected = step.docstring().expect("docstring").trim_matches('\n');
    assert_eq!(snippet(world), expected);
}

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

fn severity_named(name: &str) -> lsp::Severity {
    lsp::Severity::ALL
        .into_iter()
        .find(|severity| severity.as_str() == name)
        .unwrap_or_else(|| panic!("no such severity: {name}"))
}

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

fn diagnostic_row(world: &VardeWorld, file: &str, line: Option<usize>) -> usize {
    let path = abs(world, file);
    lsp::listed(&world.state)
        .iter()
        .position(|(at, diagnostic)| {
            *at == path.as_path() && diagnostic.map(|diagnostic| diagnostic.line) == line
        })
        .unwrap_or_else(|| panic!("no row for {file} {line:?}"))
}

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

#[when(expr = "I click the Diagnostic list row for {string} line {int}")]
fn click_diagnostic_row(world: &mut VardeWorld, file: String, line: usize) {
    let index = diagnostic_row(world, &file, Some(line));
    let corner = world.panes().corner;
    let row = corner.y + 1 + (index - world.state.diagnostics_scroll) as u16;
    world.report(mouse::Kind::LeftDown, corner.x + 2, row);
    world.report(mouse::Kind::LeftUp, corner.x + 2, row);
}

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

#[given("git reports as unmerged:")]
#[when("git reports as unmerged:")]
fn git_reports_unmerged(world: &mut VardeWorld, step: &Step) {
    let files: Vec<String> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .map(|row| row[0].trim().to_string())
        .collect();
    world.state.repo = Some(
        files
            .iter()
            .map(|path| GitFile {
                path: path.clone(),
                status: GitStatus::Conflicted,
            })
            .collect(),
    );
    world.state.conflicts_on_disk = files
        .iter()
        .map(|file| {
            let text = world
                .project
                .iter()
                .find(|(name, _)| name == file)
                .map(|(_, contents)| contents.clone())
                .unwrap_or_default();
            (abs(world, file), varde::conflict::find(text.split('\n')))
        })
        .collect();
}

fn drawn_as(world: &VardeWorld, line: usize) -> &'static str {
    use varde::conflict::Drawn;
    match varde::conflict::drawn(&world.state, line) {
        None => "text",
        Some(Drawn::Current) => "current",
        Some(Drawn::Ancestor) => "ancestor",
        Some(Drawn::Incoming) => "incoming",
        Some(Drawn::Bar(pieces)) if pieces.is_empty() => "separator",
        Some(Drawn::Bar(pieces)) if pieces.iter().any(|(_, side)| side.is_some()) => "buttons",
        Some(Drawn::Bar(_)) => "bar",
    }
}

#[then("the editor draws the lines as:")]
fn editor_draws_lines(world: &mut VardeWorld, step: &Step) {
    for row in step.table().expect("table").rows.iter().skip(1) {
        let line: usize = row[0].trim().parse().expect("a line");
        assert_eq!(drawn_as(world, line), row[1].trim(), "line {line}");
    }
}

#[then(expr = "the editor draws line {int} as {string}")]
fn editor_draws_line(world: &mut VardeWorld, line: usize, drawn: String) {
    assert_eq!(drawn_as(world, line), drawn);
}

#[then(expr = "the Conflict at line {int} is between {string} and {string}")]
fn conflict_between(world: &mut VardeWorld, line: usize, current: String, incoming: String) {
    let conflict = current_buffer(world)
        .conflicts()
        .iter()
        .find(|conflict| conflict.start == line)
        .expect("a Conflict there")
        .clone();
    assert_eq!((conflict.current, conflict.incoming), (current, incoming));
}

#[then(expr = "the buffer holds {string}")]
fn buffer_holds_inline(world: &mut VardeWorld, text: String) {
    assert_eq!(current_buffer(world).shown(), text.replace("\\n", "\n"));
}

#[when(expr = "I click the {string} button on the Conflict's bar")]
fn click_conflict_button(world: &mut VardeWorld, side: String) {
    let side = match side.as_str() {
        "current" => varde::conflict::Side::Current,
        "incoming" => varde::conflict::Side::Incoming,
        "both" => varde::conflict::Side::Both,
        other => panic!("no such side: {other}"),
    };
    let line = current_buffer(world).conflicts()[0].start;
    let Some(varde::conflict::Drawn::Bar(pieces)) = varde::conflict::drawn(&world.state, line)
    else {
        panic!("line {line} is not drawn as a bar");
    };
    let column = 1 + pieces
        .iter()
        .take_while(|(_, button)| *button != Some(side))
        .map(|(piece, _)| piece.chars().count())
        .sum::<usize>();
    let (x, y) = pointer_at(&world.state, &world.panes(), Pane::Editor, (line, column));
    world.report(mouse::Kind::LeftDown, x, y);
    world.report(mouse::Kind::LeftUp, x, y);
}

#[given("the Conflict list is shown")]
#[when("I show the Conflict list")]
fn conflict_list_is_shown(world: &mut VardeWorld) {
    if world.state.corner != layout::Corner::Conflicts {
        world.send(Event::ToggleConflictList);
    }
    assert_eq!(world.state.focus, Pane::Conflicts);
}

#[then("the Corner holds the Conflict list")]
fn corner_holds_conflict_list(world: &mut VardeWorld) {
    assert_eq!(world.state.corner, layout::Corner::Conflicts);
}

#[then("the Conflict list rows are:")]
fn conflict_list_rows(world: &mut VardeWorld, step: &Step) {
    let expected: Vec<(String, Option<usize>)> = step
        .table()
        .expect("table")
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let line = match row[1].trim() {
                "" => None,
                line => Some(line.parse().expect("a line")),
            };
            (row[0].trim().to_string(), line)
        })
        .collect();
    let listed: Vec<(String, Option<usize>)> = varde::conflict::listed(&world.state)
        .into_iter()
        .map(|(path, conflict)| {
            (
                varde::relative(&world.state, &path),
                conflict.map(|conflict| conflict.start),
            )
        })
        .collect();
    assert_eq!(listed, expected);
}

fn conflict_row(world: &VardeWorld, file: &str, line: Option<usize>) -> usize {
    let path = abs(world, file);
    varde::conflict::listed(&world.state)
        .iter()
        .position(|(at, conflict)| *at == path && conflict.map(|conflict| conflict.start) == line)
        .unwrap_or_else(|| panic!("no row for {file} {line:?}"))
}

fn select_conflict_row(world: &mut VardeWorld, index: usize) {
    world.state.focus = Pane::Conflicts;
    while world.state.conflicts_selection > index {
        world.send(Event::Key('k'));
    }
    while world.state.conflicts_selection < index {
        world.send(Event::Key('j'));
    }
}

#[given(expr = "the Conflict list selection is on {string} line {int}")]
fn conflict_selection_on(world: &mut VardeWorld, file: String, line: usize) {
    let index = conflict_row(world, &file, Some(line));
    select_conflict_row(world, index);
}

#[given(expr = "the Conflict list selection is on the row for {string}")]
fn conflict_selection_on_file(world: &mut VardeWorld, file: String) {
    let index = conflict_row(world, &file, None);
    select_conflict_row(world, index);
}

#[then(expr = "the Conflict list selection is on {string} line {int}")]
fn conflict_selection_should_be_on(world: &mut VardeWorld, file: String, line: usize) {
    assert_eq!(
        world.state.conflicts_selection,
        conflict_row(world, &file, Some(line))
    );
}

#[when(expr = "I click the Conflict list row for {string} line {int}")]
fn click_conflict_row(world: &mut VardeWorld, file: String, line: usize) {
    let index = conflict_row(world, &file, Some(line));
    let corner = world.panes().corner;
    let row = corner.y + 1 + (index - world.state.conflicts_scroll) as u16;
    world.report(mouse::Kind::LeftDown, corner.x + 2, row);
    world.report(mouse::Kind::LeftUp, corner.x + 2, row);
}
