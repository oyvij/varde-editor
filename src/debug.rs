use crate::preview::Refusal;
use crate::{layout, Effect, Pane, Place, State};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakpoint {
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
    pub stale: bool,
    pub properties: Properties,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Properties {
    pub condition: String,
    pub hit_count: String,
    pub log_message: String,
    pub suspend: Suspend,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Suspend {
    #[default]
    Thread,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Condition,
    HitCount,
    LogMessage,
    Suspend,
}

pub const FIELDS: [Field; 4] = [
    Field::Condition,
    Field::HitCount,
    Field::LogMessage,
    Field::Suspend,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Plain,
    Conditional,
    Logpoint,
    Stale,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    Bound(usize),
    Unbound(String),
}

fn drawn<'a>(state: &'a State, breakpoint: &Breakpoint) -> (usize, Mark, Option<&'a str>) {
    let verdict = state
        .debug
        .as_ref()
        .filter(|session| session.phase != Phase::Stopping)
        .and_then(|session| {
            session
                .verdicts
                .get(&(breakpoint.file.clone(), breakpoint.line))
        });
    let properties = &breakpoint.properties;
    let kind = if !properties.log_message.is_empty() {
        Mark::Logpoint
    } else if !properties.condition.is_empty() || !properties.hit_count.is_empty() {
        Mark::Conditional
    } else {
        Mark::Plain
    };
    match (breakpoint.stale, verdict) {
        (true, _) => (breakpoint.line, Mark::Stale, None),
        (false, None) => (breakpoint.line, kind, None),
        (false, Some(Verdict::Bound(line))) => (*line, kind, None),
        (false, Some(Verdict::Unbound(why))) => (breakpoint.line, Mark::Unverified, Some(why)),
    }
}

pub fn marks(state: &State) -> BTreeMap<usize, Mark> {
    on_screen(state)
        .map(|(line, mark, _)| (line, mark))
        .collect()
}

pub fn explained(state: &State) -> Option<(usize, &str)> {
    let crate::Pointed::Breakpoint(line) = state.pointed_at else {
        return None;
    };
    on_screen(state)
        .find(|(drawn_on, _, _)| *drawn_on == line)
        .and_then(|(_, _, why)| why)
        .filter(|why| !why.is_empty())
        .map(|why| (line, why))
}

pub fn forget_changed(before: &[Breakpoint], next: &mut State) {
    let of = |breakpoints: &[Breakpoint], file: &Path| -> Vec<Breakpoint> {
        breakpoints
            .iter()
            .filter(|breakpoint| breakpoint.file == file)
            .cloned()
            .collect()
    };
    let after = &next.breakpoints;
    if let Some(session) = next.debug.as_mut() {
        session
            .verdicts
            .retain(|(file, _), _| of(before, file) == of(after, file));
    }
}

fn on_screen(state: &State) -> impl Iterator<Item = (usize, Mark, Option<&str>)> {
    state
        .breakpoints
        .iter()
        .filter(|breakpoint| Some(&breakpoint.file) == state.current_buffer.as_ref())
        .map(|breakpoint| drawn(state, breakpoint))
}

pub fn list(state: &crate::State) -> Vec<&Breakpoint> {
    let mut rows: Vec<&Breakpoint> = state.breakpoints.iter().collect();
    rows.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    rows
}

pub fn selected(state: &crate::State) -> Option<&Breakpoint> {
    let at = state
        .breakpoints_selection
        .checked_sub(switches(state).len())?;
    list(state).get(at).copied()
}

pub fn open_box(next: &mut State, file: PathBuf, line: usize) {
    let Some(breakpoint) = next
        .breakpoints
        .iter()
        .find(|breakpoint| breakpoint.file == file && breakpoint.line == line)
    else {
        return;
    };
    next.modal = crate::Modal::Breakpoint {
        draft: breakpoint.properties.clone(),
        file,
        line,
        field: Field::Condition,
    };
}

pub fn edit_chip(state: &State, panes: &layout::Layout) -> Option<(u16, u16)> {
    if state.focus != Pane::Editor
        || state.view != crate::View::Edit
        || state.diff.is_some()
        || state.walking.is_some()
        || crate::previewing(state)
    {
        return None;
    }
    let line = crate::current_buffer(state)?.line;
    state.breakpoints.iter().find(|breakpoint| {
        Some(&breakpoint.file) == state.current_buffer.as_ref() && breakpoint.line == line
    })?;
    let (_, rows, columns) = crate::fits_in(state, panes);
    let row = crate::story::row_of(state, line as u32)
        .checked_sub(1 + state.editor_scroll)
        .filter(|row| *row < rows)?;
    let column = (columns as u16).checked_sub(1)?;
    Some((
        panes.editor.x + 1 + crate::gutter(state) + column,
        panes.editor.y + 1 + row as u16,
    ))
}

pub fn field_text(draft: &Properties, field: Field) -> &str {
    match field {
        Field::Condition => &draft.condition,
        Field::HitCount => &draft.hit_count,
        Field::LogMessage => &draft.log_message,
        Field::Suspend => "",
    }
}

pub const REMOVE: &str = "remove-breakpoint";
pub const EDIT: &str = "edit-breakpoint";
pub const TOGGLE_OUTPUT: &str = "toggle-output";
pub const CLEAR_ALL: &str = "clear-all-breakpoints";
pub const EXCEPTION_CLASS: &str = "debug-exception-class";
pub const RESUME: &str = "debug-resume";
pub const STEP_OVER: &str = "debug-step-over";
pub const STEP_INTO: &str = "debug-step-into";
pub const STEP_OUT: &str = "debug-step-out";
pub const STOP: &str = "debug-stop";
pub const RESTART: &str = "debug-restart";
pub const ASK_AI: &str = "debug-ask-ai";
pub const SET_VALUE: &str = "debug-set-value";
pub const COPY_VALUE: &str = "debug-copy-value";
pub const COPY_EXPRESSION: &str = "debug-copy-expression";
pub const WATCH: &str = "debug-watch";
pub const REMOVE_WATCH: &str = "debug-remove-watch";
pub const EVALUATE: &str = "debug-evaluate";
pub const ROW_ASK_AI: &str = "debug-ask-ai-value";
pub const NEXT_THREAD: &str = "debug-next-thread";
pub const HOT_REPLACE: &str = "debug-hot-replace";

pub fn row_actions(state: &crate::State) -> Vec<&'static str> {
    match selected(state) {
        Some(_) => vec![EDIT, REMOVE],
        None => Vec::new(),
    }
}

pub fn transport(state: &crate::State) -> Vec<crate::Chip> {
    let can_name = state
        .debug
        .as_ref()
        .is_some_and(|session| session.can_name_class);
    vec![
        crate::Chip {
            action: EXCEPTION_CLASS,
            name: "exception-class",
            glyph: "\u{25c7}".to_string(),
            keys: "x",
            hue: crate::Hue::Hold,
            tone: match can_name {
                true => crate::Tone::Plain,
                false => crate::Tone::Dimmed,
            },
        },
        crate::Chip {
            action: CLEAR_ALL,
            name: "clear-all",
            glyph: "\u{2715}".to_string(),
            keys: "D",
            hue: crate::Hue::Halt,
            tone: match state.breakpoints.is_empty() {
                true => crate::Tone::Dimmed,
                false => crate::Tone::Plain,
            },
        },
    ]
}

pub fn strip_transport(state: &crate::State) -> Vec<crate::Chip> {
    use crate::{Chip, Hue, Tone};
    let chip = |action, name, glyph: &str, keys, hue, dimmed| Chip {
        action,
        name,
        glyph: glyph.to_string(),
        keys,
        hue,
        tone: match (state.transport_lit == Some(action), dimmed) {
            (true, _) => Tone::Lit,
            (false, true) => Tone::Dimmed,
            (false, false) => Tone::Plain,
        },
    };
    let restart = |dimmed| {
        chip(
            RESTART,
            "restart",
            "\u{21bb}",
            "C-F5 \u{2423}r",
            Hue::Go,
            dimmed,
        )
    };
    let mut chips = Vec::new();
    if let Some(session) = state.debug.as_ref() {
        let running = matches!(session.phase, Phase::Running(_));
        let stepping = !matches!(session.phase, Phase::Paused(_));
        let between = !matches!(session.phase, Phase::Paused(_) | Phase::Running(_));
        chips.extend([
            match running {
                true => chip(
                    RESUME,
                    "pause",
                    "\u{2016}",
                    "F9 \u{2423}c",
                    Hue::Hold,
                    between,
                ),
                false => chip(
                    RESUME,
                    "continue",
                    "\u{25ba}",
                    "F9 \u{2423}c",
                    Hue::Go,
                    between,
                ),
            },
            chip(
                STEP_OVER,
                "step-over",
                "\u{293c}",
                "F8 \u{2423}n",
                Hue::Step,
                stepping,
            ),
            chip(
                STEP_INTO,
                "step-into",
                "\u{2913}",
                "F7 \u{2423}i",
                Hue::Step,
                stepping,
            ),
            chip(
                STEP_OUT,
                "step-out",
                "\u{2912}",
                "S-F8 \u{2423}o",
                Hue::Step,
                stepping,
            ),
            chip(STOP, "stop", "\u{25a0}", "C-F2 \u{2423}q", Hue::Halt, false),
            restart(!session.offered),
            chip(
                ASK_AI,
                "ask-ai",
                "\u{2736}",
                "\u{2423}a",
                Hue::Plain,
                stepping,
            ),
            chip(
                NEXT_THREAD,
                "next-thread",
                &format!("\u{21c9}{}", session.others.len()),
                "",
                Hue::Plain,
                session.others.is_empty(),
            ),
        ]);
        if hot_replace(state).is_some() {
            chips.push(chip(
                HOT_REPLACE,
                "hot-replace",
                "\u{21c4}",
                "",
                Hue::Go,
                between,
            ));
        }
    }
    if state.debug.is_none() && state.last_launch.is_some() {
        chips.push(restart(false));
    }
    if state.output_running {
        chips.push(Chip {
            action: TOGGLE_OUTPUT,
            name: match state.output_hidden {
                true => "show-output",
                false => "hide-output",
            },
            glyph: match state.output_hidden {
                true => "\u{25a3}".to_string(),
                false => "\u{25a2}".to_string(),
            },
            keys: "\u{2423}h",
            hue: Hue::Plain,
            tone: match state.output_unseen {
                true => Tone::Marked,
                false => Tone::Plain,
            },
        });
    }
    chips
}

pub fn held(text: &str, line: usize) -> Option<&str> {
    Some(text.split('\n').nth(line.checked_sub(1)?)?.trim())
}

pub fn follow(breakpoints: &mut Vec<Breakpoint>, file: &std::path::Path, old: &str, new: &str) {
    let mut options = git2::DiffOptions::new();
    options.context_lines(0);
    let Ok(patch) = git2::Patch::from_buffers(
        old.as_bytes(),
        None,
        new.as_bytes(),
        None,
        Some(&mut options),
    ) else {
        return;
    };
    // A hunk side holding no lines names the line it sits after, not the first line it holds
    let hunks: Vec<(usize, usize, usize, usize)> = (0..patch.num_hunks())
        .filter_map(|index| patch.hunk(index).ok())
        .map(|(hunk, _)| {
            let (old_lines, new_lines) = (hunk.old_lines() as usize, hunk.new_lines() as usize);
            let start = |at: u32, lines: usize| at as usize + usize::from(lines == 0);
            (
                start(hunk.old_start(), old_lines),
                old_lines,
                start(hunk.new_start(), new_lines),
                new_lines,
            )
        })
        .collect();
    let carried = |line: usize| {
        let mut offset = 0isize;
        for &(old_start, old_lines, new_start, new_lines) in &hunks {
            if line < old_start {
                break;
            }
            if line >= old_start + old_lines {
                offset += new_lines as isize - old_lines as isize;
                continue;
            }
            let within = line - old_start;
            return (within < new_lines).then_some(new_start + within);
        }
        usize::try_from(line as isize + offset).ok()
    };
    breakpoints.retain_mut(|breakpoint| {
        if breakpoint.file != file {
            return true;
        }
        let Some(line) = carried(breakpoint.line) else {
            return false;
        };
        breakpoint.line = line;
        if !breakpoint.stale {
            breakpoint.text = held(new, line).unwrap_or_default().to_string();
        }
        true
    });
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    adapter: String,
    command: String,
    begin: Begin,
    pub phase: Phase,
    request: String,
    args: serde_json::Map<String, Value>,
    watch: Option<(String, u16)>,
    seq: i64,
    asked: BTreeMap<i64, Ask>,
    verdicts: BTreeMap<(PathBuf, usize), Verdict>,
    pausing: BTreeSet<i64>,
    threads: Vec<(i64, String)>,
    others: BTreeMap<i64, (Why, Option<String>)>,
    previous: Option<(String, BTreeMap<String, String>)>,
    can_set: bool,
    can_cancel: bool,
    filters: Vec<Filter>,
    can_name_class: bool,
    class: Option<String>,
    offered: bool,
    corner: layout::Corner,
    strip: layout::Group,
    children: BTreeMap<usize, Child>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Child {
    name: String,
    request: String,
    configuration: Value,
}

fn thread_on(link: usize, id: i64) -> i64 {
    ((link as i64) << 32) | (id & 0xFFFF_FFFF)
}

fn link_of(thread: i64) -> (usize, i64) {
    ((thread >> 32) as usize, thread & 0xFFFF_FFFF)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub id: String,
    pub label: String,
}

fn reported(filters: &Value) -> Option<Vec<Filter>> {
    let listed = filters.as_array()?;
    Some(
        listed
            .iter()
            .map(|filter| Filter {
                id: filter["filter"].as_str().unwrap_or_default().to_string(),
                label: printable(filter["label"].as_str().unwrap_or_default()),
            })
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Ask {
    command: String,
    arguments: Value,
    to: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Spawning,
    Initializing,
    Starting,
    Running(Option<Pause>),
    Paused(Pause),
    /// Dropping the adapter before disconnect answers can kill a launched program unannounced
    Stopping,
    Waiting,
}

const PORT: &str = "${port}";

pub fn on_port(args: &[String], port: u16) -> Vec<String> {
    args.iter()
        .map(|arg| arg.replace(PORT, &port.to_string()))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Stdio,
    Server,
    Port(u16),
}

#[derive(Debug, Clone, PartialEq)]
enum Begin {
    Spawn(Effect),
    Ask { server: String, id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pause {
    pub thread: i64,
    pub why: Why,
    exception: Option<String>,
    pub frames: Vec<Frame>,
    pub chosen: usize,
    scopes: Vec<Member>,
    children: BTreeMap<i64, Vec<Member>>,
    open: BTreeSet<i64>,
    fetched: BTreeMap<i64, usize>,
    unfolded: BTreeSet<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Member {
    name: String,
    value: String,
    reference: i64,
    hint: Hint,
    indexed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Plain,
    Private,
    ReadOnly,
    Lazy,
}

impl Hint {
    pub fn as_str(self) -> &'static str {
        match self {
            Hint::Plain => "plain",
            Hint::Private => "private",
            Hint::ReadOnly => "read-only",
            Hint::Lazy => "lazy",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub value: String,
    pub depth: usize,
    pub hint: Hint,
    pub open: bool,
    pub opens: Opens,
    pub expression: String,
    pub parent: i64,
    pub of: Of,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Of {
    Member,
    Watch {
        index: usize,
        calling: bool,
        failed: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    Nothing,
    Children { reference: i64, indexed: usize },
    NextPage { reference: i64, start: usize },
}

const PAGE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    pub expression: String,
    pub answer: Answer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Waiting,
    Value(String),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Paused,
    Exception,
}

impl Why {
    pub fn as_str(self) -> &'static str {
        match self {
            Why::Paused => "paused",
            Why::Exception => "exception",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Over,
    Into,
    Out,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub id: i64,
    pub name: String,
    pub file: Option<PathBuf>,
    pub line: usize,
    pub library: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    Missing,
    FailedToStart,
    Exited,
}

pub fn launches(state: &State) -> Vec<&str> {
    state.launches.keys().map(String::as_str).collect()
}

pub fn start(next: &mut State, name: &str) -> Vec<Effect> {
    match next.launches.get(name).cloned() {
        Some(launch) => launch_with(next, launch),
        None => Vec::new(),
    }
}

pub fn launch_with(next: &mut State, launch: crate::startup::Launch) -> Vec<Effect> {
    if next.debug.is_some() {
        next.refusal = Some(Refusal::SessionRunning);
        return Vec::new();
    }
    let Some(adapter) = next.adapters.get(&launch.adapter).cloned() else {
        next.refusal = Some(Refusal::NoDebugAdapter(launch.adapter.clone()));
        return Vec::new();
    };
    let (begin, effect) = match &adapter.server {
        Some(server) => match crate::lsp::execute(next, server, &adapter.command) {
            Some((id, effect)) => (
                Begin::Ask {
                    server: server.clone(),
                    id,
                },
                effect,
            ),
            None => {
                next.refusal = Some(Refusal::NoLanguageServer(server.clone()));
                return Vec::new();
            }
        },
        None => {
            let effect = Effect::StartDap {
                command: adapter.command.clone(),
                args: adapter.args.clone(),
                reach: match adapter.args.iter().any(|arg| arg.contains(PORT)) {
                    true => Reach::Server,
                    false => Reach::Stdio,
                },
            };
            (Begin::Spawn(effect.clone()), effect)
        }
    };
    let watch = match (launch.request.as_str(), launch.reattach) {
        ("attach", true) => launch
            .args
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
            .map(|port| {
                let host = launch.args.get("hostName").and_then(Value::as_str);
                (printable(host.unwrap_or("localhost")), port)
            }),
        _ => None,
    };
    next.last_launch = Some(launch.clone());
    next.debug = Some(Session {
        adapter: launch.adapter.clone(),
        command: adapter.command.clone(),
        begin,
        phase: Phase::Spawning,
        request: launch.request.clone(),
        args: launch.args.clone(),
        watch,
        seq: 0,
        asked: BTreeMap::new(),
        verdicts: BTreeMap::new(),
        pausing: BTreeSet::new(),
        threads: Vec::new(),
        others: BTreeMap::new(),
        children: BTreeMap::new(),
        previous: None,
        can_set: false,
        can_cancel: false,
        filters: Vec::new(),
        can_name_class: false,
        class: None,
        offered: false,
        corner: next.corner,
        strip: next.strip,
    });
    next.strip = layout::Group::Debug;
    vec![effect]
}

pub fn relaunch(next: &mut State) -> Vec<Effect> {
    match next.relaunch.take() {
        Some(launch) => launch_with(next, launch),
        None => Vec::new(),
    }
}

pub fn waiting_on(state: &State) -> Option<(&str, u16)> {
    let session = state.debug.as_ref()?;
    let (host, port) = session.watch.as_ref()?;
    (session.phase == Phase::Waiting).then_some((host.as_str(), *port))
}

pub fn reattach(next: &mut State) -> Vec<Effect> {
    if waiting_on(next).is_none() {
        return Vec::new();
    }
    let session = next.debug.take().expect("a Waiting session");
    let (begin, effect) = match &session.begin {
        Begin::Spawn(effect) => (session.begin.clone(), effect.clone()),
        Begin::Ask { server, .. } => match crate::lsp::execute(next, server, &session.command) {
            Some((id, effect)) => (
                Begin::Ask {
                    server: server.clone(),
                    id,
                },
                effect,
            ),
            None => {
                next.refusal = Some(Refusal::NoLanguageServer(server.clone()));
                next.debug = Some(session);
                return end(next);
            }
        },
    };
    next.debug = Some(Session {
        begin,
        phase: Phase::Spawning,
        asked: BTreeMap::new(),
        verdicts: BTreeMap::new(),
        pausing: BTreeSet::new(),
        threads: Vec::new(),
        others: BTreeMap::new(),
        children: BTreeMap::new(),
        previous: None,
        offered: false,
        ..session
    });
    next.strip = layout::Group::Debug;
    vec![effect]
}

pub fn ported(next: &mut State, server: &str, id: i64, message: &Value) -> Option<Vec<Effect>> {
    let session = next.debug.as_ref()?;
    let asked = Begin::Ask {
        server: server.to_string(),
        id,
    };
    if session.phase != Phase::Spawning || session.begin != asked {
        return None;
    }
    let port = message["result"]
        .as_u64()
        .and_then(|port| u16::try_from(port).ok());
    Some(match port {
        Some(port) => vec![Effect::StartDap {
            command: session.command.clone(),
            args: Vec::new(),
            reach: Reach::Port(port),
        }],
        None => {
            let why = printable(
                message["error"]["message"]
                    .as_str()
                    .unwrap_or(&session.command),
            );
            next.refusal = Some(Refusal::LaunchFailed(why.clone()));
            let mut effects = end(next);
            effects.push(Effect::notify_about("launch-failed", why));
            effects
        }
    })
}

pub fn unhosted(next: &mut State, server: &str) -> Vec<Effect> {
    let asking = next.debug.as_ref().is_some_and(|session| {
        session.phase == Phase::Spawning
            && matches!(&session.begin, Begin::Ask { server: asked, .. } if asked == server)
    });
    if !asking {
        return Vec::new();
    }
    next.refusal = Some(Refusal::NoLanguageServer(server.to_string()));
    end(next)
}

pub fn started(next: &mut State, from: usize) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    match from {
        0 if session.phase == Phase::Spawning => session.phase = Phase::Initializing,
        0 => return Vec::new(),
        child if !session.children.contains_key(&child) => return Vec::new(),
        _ => {}
    }
    let adapter = session.adapter.clone();
    vec![ask_on(
        session,
        from,
        "initialize",
        json!({
            "clientID": "varde",
            "clientName": "Varde",
            "adapterID": adapter,
            "pathFormat": "path",
            "linesStartAt1": true,
            "columnsStartAt1": true,
            "supportsStartDebuggingRequest": true,
        }),
    )]
}

pub fn gone(next: &mut State, why: Gone, from: usize) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    if from != 0 {
        forget(session, from);
        return Vec::new();
    }
    let command = session.command.clone();
    let (refusal, notice) = match (why, &session.phase) {
        (_, Phase::Stopping) => (None, None),
        (Gone::Missing, _) => (
            Some(Refusal::NoDebugAdapter(command.clone())),
            Some(Effect::notify_about("no-debug-adapter", command)),
        ),
        (Gone::FailedToStart, _) => (
            Some(Refusal::DebugAdapterFailed),
            Some(Effect::Notify("debug-adapter-failed")),
        ),
        (Gone::Exited, _) => (
            Some(Refusal::DebugAdapterExited),
            Some(Effect::Notify("debug-adapter-exited")),
        ),
    };
    next.refusal = refusal;
    let mut effects = end(next);
    effects.extend(notice);
    effects
}

pub fn received(next: &mut State, json: &str, from: usize) -> Vec<Effect> {
    let Ok(mut message) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let Some(session) = next.debug.as_ref() else {
        return Vec::new();
    };
    if from != 0 && !session.children.contains_key(&from) {
        return Vec::new();
    }
    if let Some(body) = message.get_mut("body") {
        if let Some(id) = body["threadId"].as_i64() {
            body["threadId"] = json!(thread_on(from, id));
        }
        for thread in body["threads"].as_array_mut().into_iter().flatten() {
            if let Some(id) = thread["id"].as_i64() {
                thread["id"] = json!(thread_on(from, id));
            }
        }
    }
    match message["type"].as_str() {
        Some("response") => answered(next, &message),
        Some("event") => told(next, &message, from),
        Some("request") => {
            let session = next.debug.as_mut().expect("a session");
            let command = message["command"].as_str().unwrap_or_default();
            if command == "runInTerminal" {
                let arguments = &message["arguments"];
                let argv: Vec<String> = arguments["args"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(printable)
                    .collect();
                if argv.is_empty() {
                    return vec![reply(
                        session,
                        from,
                        &message,
                        json!({
                            "success": false,
                            "command": command,
                            "message": "runInTerminal named no program",
                        }),
                    )];
                }
                let env = arguments["env"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter_map(|(name, value)| Some((printable(name), printable(value.as_str()?))))
                    .collect();
                let cwd = arguments["cwd"].as_str().map(printable).map(PathBuf::from);
                return vec![
                    reply(
                        session,
                        from,
                        &message,
                        json!({ "success": true, "command": command, "body": {} }),
                    ),
                    Effect::RunProgram { argv, cwd, env },
                ];
            }
            if command == "startDebugging" {
                let arguments = &message["arguments"];
                let child = session.seq as usize + 1;
                let configuration = arguments["configuration"].clone();
                let name = configuration["name"]
                    .as_str()
                    .map_or_else(|| format!("child {child}"), printable);
                let request = arguments["request"]
                    .as_str()
                    .unwrap_or("launch")
                    .to_string();
                session.children.insert(
                    child,
                    Child {
                        name,
                        request,
                        configuration,
                    },
                );
                return vec![
                    reply(
                        session,
                        from,
                        &message,
                        json!({ "success": true, "command": command, "body": {} }),
                    ),
                    Effect::DapChild { child },
                ];
            }
            // An adapter left waiting on a reverse-request reply hangs the session, so refuse out loud
            vec![reply(
                session,
                from,
                &message,
                json!({
                    "success": false,
                    "command": command,
                    "message": format!("Varde does not answer {command}"),
                }),
            )]
        }
        _ => Vec::new(),
    }
}

fn answered(next: &mut State, message: &Value) -> Vec<Effect> {
    let named = hot_replace(next).map(|named| named.request.clone());
    let session = next.debug.as_mut().expect("a session");
    let answering = message["request_seq"].as_i64().unwrap_or_default();
    let Some(Ask {
        command,
        arguments,
        to,
    }) = session.asked.remove(&answering)
    else {
        return Vec::new();
    };
    if message["success"] != Value::Bool(true) {
        return match command.as_str() {
            "initialize" | "launch" | "attach" | "disconnect" if to != 0 => {
                forget(session, to);
                let mut effects = vec![Effect::StopDapChild { child: to }];
                if command != "disconnect" {
                    effects.push(Effect::notify_about(
                        "launch-failed",
                        adapter_error(message, &command),
                    ));
                }
                effects
            }
            "initialize" | "launch" | "attach" => {
                let why = adapter_error(message, &command);
                let mut effects = match session.watch {
                    Some(_) => {
                        session.phase = Phase::Waiting;
                        Vec::new()
                    }
                    None => end(next),
                };
                next.refusal = Some(Refusal::LaunchFailed(why.clone()));
                effects.extend([Effect::notify_about("launch-failed", why), Effect::StopDap]);
                effects
            }
            "disconnect" => let_go(next),
            "threads" => {
                session.pausing.remove(&answering);
                Vec::new()
            }
            "evaluate" => {
                let why = adapter_error(message, &command);
                if let Some(ran) = ran_asked(next, &arguments, answering) {
                    ran.answer = Ran::Failed(why);
                    return Vec::new();
                }
                match hover_asked(next, &arguments) {
                    Some(hovered) => hovered.held = Held::Failed(why),
                    None => {
                        if let Some(watch) = watch_asked(next, &arguments) {
                            watch.answer = Answer::Failed(why);
                        }
                    }
                }
                Vec::new()
            }
            "setVariable" => {
                next.refusal = Some(Refusal::SetValueFailed(adapter_error(message, &command)));
                Vec::new()
            }
            replace if Some(replace) == named.as_deref() => {
                session.offered = true;
                vec![Effect::notify_about(
                    "hot-replace-failed",
                    adapter_error(message, &command),
                )]
            }
            _ => Vec::new(),
        };
    }
    match command.as_str() {
        "initialize" if to != 0 => {
            let Some(child) = session.children.get(&to) else {
                return Vec::new();
            };
            let (request, configuration) = (child.request.clone(), child.configuration.clone());
            vec![ask_on(session, to, &request, configuration)]
        }
        "disconnect" if to != 0 => {
            forget(session, to);
            vec![Effect::StopDapChild { child: to }]
        }
        "initialize" => {
            session.can_set = message["body"]["supportsSetVariable"] == Value::Bool(true);
            session.can_cancel = message["body"]["supportsCancelRequest"] == Value::Bool(true);
            session.can_name_class =
                message["body"]["supportsExceptionOptions"] == Value::Bool(true);
            session.filters =
                reported(&message["body"]["exceptionBreakpointFilters"]).unwrap_or_default();
            session.phase = Phase::Starting;
            let (request, args) = (session.request.clone(), session.args.clone());
            vec![ask_on(session, 0, &request, Value::Object(args))]
        }
        "stackTrace" => {
            let root = &next.root;
            let frames = message["body"]["stackFrames"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|frame| {
                    let file = frame["source"]["path"]
                        .as_str()
                        .map(printable)
                        .map(PathBuf::from);
                    Frame {
                        id: frame["id"].as_i64().unwrap_or_default(),
                        name: printable(frame["name"].as_str().unwrap_or_default()),
                        library: file.as_ref().is_none_or(|file| !file.starts_with(root))
                            || frame["presentationHint"] == "subtle"
                            || frame["source"]["presentationHint"] == "deemphasize",
                        file,
                        line: frame["line"].as_u64().unwrap_or_default() as usize,
                    }
                })
                .collect();
            let Phase::Paused(pause) = &mut session.phase else {
                return Vec::new();
            };
            if arguments["threadId"].as_i64() != Some(pause.thread) {
                return Vec::new();
            }
            pause.frames = frames;
            pause.chosen = 0;
            next.frames_selection = row_of(next, 0);
            inspect(next)
        }
        "variables" => {
            let members: Vec<Member> = message["body"]["variables"]
                .as_array()
                .into_iter()
                .flatten()
                .map(member)
                .collect();
            let Phase::Paused(pause) = &mut session.phase else {
                return Vec::new();
            };
            let reference = arguments["variablesReference"].as_i64().unwrap_or_default();
            let held = pause.children.entry(reference).or_default();
            held.extend(members);
            let fetched = held.len();
            pause.fetched.insert(reference, fetched);
            Vec::new()
        }
        "scopes" => {
            let scopes: Vec<(Member, bool)> = message["body"]["scopes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|scope| (member(scope), scope["expensive"] == Value::Bool(true)))
                .collect();
            let Phase::Paused(pause) = &mut session.phase else {
                return Vec::new();
            };
            pause.scopes = scopes.iter().map(|(scope, _)| scope.clone()).collect();
            pause.children.clear();
            pause.fetched.clear();
            let opened: Vec<(i64, usize)> = scopes
                .iter()
                .filter(|(_, expensive)| !expensive)
                .map(|(scope, _)| (scope.reference, scope.indexed))
                .collect();
            pause.open = opened.iter().map(|(reference, _)| *reference).collect();
            opened
                .into_iter()
                .map(|(reference, indexed)| fetch(session, reference, paged(indexed, 0)))
                .collect()
        }
        "evaluate" => {
            let value = printable(message["body"]["result"].as_str().unwrap_or_default());
            let reference = message["body"]["variablesReference"]
                .as_i64()
                .unwrap_or_default();
            let indexed = message["body"]["indexedVariables"]
                .as_u64()
                .unwrap_or_default() as usize;
            if let Some(ran) = ran_asked(next, &arguments, answering) {
                ran.answer = Ran::Value {
                    value,
                    reference,
                    indexed,
                };
                return Vec::new();
            }
            match hover_asked(next, &arguments) {
                Some(hovered) => {
                    hovered.held = Held::Value {
                        value,
                        reference,
                        indexed,
                    }
                }
                None => {
                    if let Some(watch) = watch_asked(next, &arguments) {
                        watch.answer = Answer::Value(value);
                    }
                }
            }
            Vec::new()
        }
        "setVariable" => {
            let value = printable(message["body"]["value"].as_str().unwrap_or_default());
            let reference = arguments["variablesReference"].as_i64().unwrap_or_default();
            let name = arguments["name"].as_str().unwrap_or_default();
            let Some(Phase::Paused(pause)) = next.debug.as_mut().map(|s| &mut s.phase) else {
                return Vec::new();
            };
            let held = match pause.children.get_mut(&reference) {
                Some(held) => held,
                None => &mut pause.scopes,
            };
            if let Some(member) = held.iter_mut().find(|member| member.name == name) {
                member.value = value;
            }
            Vec::new()
        }
        "setBreakpoints" => {
            let file = PathBuf::from(arguments["source"]["path"].as_str().unwrap_or_default());
            let asked = arguments["breakpoints"].as_array().into_iter().flatten();
            let answers = message["body"]["breakpoints"]
                .as_array()
                .into_iter()
                .flatten();
            for (asked, answer) in asked.zip(answers) {
                let set = asked["line"].as_u64().unwrap_or_default() as usize;
                let verdict = match answer["verified"] == Value::Bool(true) {
                    true => {
                        Verdict::Bound(answer["line"].as_u64().map_or(set, |line| line as usize))
                    }
                    false => {
                        Verdict::Unbound(printable(answer["message"].as_str().unwrap_or_default()))
                    }
                };
                session.verdicts.insert((file.clone(), set), verdict);
            }
            Vec::new()
        }
        "threads" => {
            session.threads.retain(|(id, _)| link_of(*id).0 != to);
            let named = message["body"]["threads"].as_array().into_iter().flatten();
            session.threads.extend(named.filter_map(|thread| {
                let name = printable(thread["name"].as_str().unwrap_or_default());
                Some((thread["id"].as_i64()?, name))
            }));
            session.threads.sort_by_key(|(id, _)| link_of(*id).0);
            // An adapter's parent session often has no threads, so it must not end the pause
            if !session.pausing.remove(&answering)
                || (session.threads.is_empty() && !session.pausing.is_empty())
            {
                return Vec::new();
            }
            session.pausing.clear();
            match session.threads.first() {
                Some(&(thread, _)) => vec![ask(session, "pause", json!({ "threadId": thread }))],
                None => Vec::new(),
            }
        }
        // DAP's default is that every thread continued; only allThreadsContinued: false says otherwise
        "continue" => {
            if message["body"]["allThreadsContinued"] != Value::Bool(false) {
                session.others.retain(|id, _| link_of(*id).0 != to);
            }
            Vec::new()
        }
        "disconnect" => let_go(next),
        replace if Some(replace) == named.as_deref() => {
            session.offered = false;
            vec![Effect::Notify("hot-replaced")]
        }
        _ => Vec::new(),
    }
}

fn let_go(next: &mut State) -> Vec<Effect> {
    let mut effects = match waiting_on(next) {
        Some(_) => Vec::new(),
        None => end(next),
    };
    effects.push(Effect::StopDap);
    effects
}

fn told(next: &mut State, message: &Value, from: usize) -> Vec<Effect> {
    let body = &message["body"];
    match message["event"].as_str() {
        Some("initialized") => configured(next, from),
        Some("stopped") => {
            let session = next.debug.as_mut().expect("a session");
            let thread = body["threadId"].as_i64().unwrap_or_default();
            let why = match body["reason"].as_str() {
                Some("exception") => Why::Exception,
                _ => Why::Paused,
            };
            let exception = body["text"].as_str().map(printable);
            match &session.phase {
                Phase::Running(_) => {}
                Phase::Paused(pause) if pause.thread == thread => {}
                Phase::Paused(_) => {
                    session.others.insert(thread, (why, exception));
                    return ask_all(session, "threads", json!({}));
                }
                _ => return Vec::new(),
            }
            let ending = match &session.phase {
                Phase::Paused(pause) => Some(pause),
                Phase::Running(last) => last.as_ref(),
                _ => None,
            };
            session.previous = ending.map(|pause| {
                let frame = pause
                    .frames
                    .get(pause.chosen)
                    .map(|frame| frame.name.clone())
                    .unwrap_or_default();
                (frame, locals(pause))
            });
            let effects = inspect_thread(session, thread, why, exception);
            next.corner = layout::Corner::Frames;
            next.strip = layout::Group::Debug;
            effects
        }
        Some("capabilities") => {
            let session = next.debug.as_mut().expect("a session");
            if let Some(can_set) = body["capabilities"]["supportsSetVariable"].as_bool() {
                session.can_set = can_set;
            }
            if let Some(can_cancel) = body["capabilities"]["supportsCancelRequest"].as_bool() {
                session.can_cancel = can_cancel;
            }
            if let Some(can_name) = body["capabilities"]["supportsExceptionOptions"].as_bool() {
                session.can_name_class = can_name;
            }
            if let Some(filters) = reported(&body["capabilities"]["exceptionBreakpointFilters"]) {
                session.filters = filters;
            }
            Vec::new()
        }
        Some("output") => {
            let text = body["output"].as_str().unwrap_or_default();
            if let Some(ran) = in_flight(next) {
                ran.printed.push(printable(text.trim_end_matches('\n')));
            }
            Vec::new()
        }
        Some("continued") => {
            let session = next.debug.as_mut().expect("a session");
            let every = body["allThreadsContinued"] == Value::Bool(true);
            let thread = body["threadId"].as_i64().unwrap_or_default();
            match every {
                true => session.others.retain(|id, _| link_of(*id).0 != from),
                false => {
                    session.others.remove(&thread);
                }
            }
            if let Phase::Paused(pause) = &session.phase {
                if (every && link_of(pause.thread).0 == from) || pause.thread == thread {
                    session.phase = Phase::Running(Some(pause.clone()));
                }
            }
            Vec::new()
        }
        Some("terminated") => {
            let session = next.debug.as_mut().expect("a session");
            if from != 0 {
                return vec![ask_on(session, from, "disconnect", json!({}))];
            }
            if matches!(session.phase, Phase::Stopping | Phase::Waiting) {
                return Vec::new();
            }
            session.phase = match session.watch {
                Some(_) => Phase::Waiting,
                None => Phase::Stopping,
            };
            let effects = ask_all(session, "disconnect", json!({}));
            next.stepping = false;
            effects
        }
        Some(named) if hot_replace(next).is_some_and(|row| row.event == named) => {
            let request = hot_replace(next).expect("a named request").request.clone();
            let session = next.debug.as_mut().expect("a session");
            vec![ask_on(session, from, &request, json!({}))]
        }
        _ => Vec::new(),
    }
}

fn configured(next: &mut State, from: usize) -> Vec<Effect> {
    let pause_on = pause_on(next);
    let session = next.debug.as_mut().expect("a session");
    match from {
        0 if session.phase != Phase::Starting => return Vec::new(),
        child if child != 0 && !session.children.contains_key(&child) => return Vec::new(),
        _ => {}
    }
    let files: BTreeSet<&Path> = next
        .breakpoints
        .iter()
        .map(|breakpoint| breakpoint.file.as_path())
        .collect();
    let mut effects: Vec<Effect> = files
        .into_iter()
        .map(|file| set_breakpoints(session, from, &next.breakpoints, file))
        .collect();
    effects.push(ask_on(session, from, "setExceptionBreakpoints", pause_on));
    effects.push(ask_on(session, from, "configurationDone", json!({})));
    if from == 0 {
        session.phase = Phase::Running(None);
    }
    effects
}

/// setExceptionBreakpoints replaces what the adapter held, so it is sent whole every time
fn pause_on(state: &State) -> Value {
    let on: Vec<&str> = switches(state)
        .into_iter()
        .filter(|(_, on)| *on)
        .map(|(filter, _)| filter.id.as_str())
        .collect();
    let mut arguments = json!({ "filters": on });
    if let Some(class) = state
        .debug
        .as_ref()
        .and_then(|session| session.class.as_ref())
    {
        arguments["exceptionOptions"] =
            json!([{ "path": [{ "names": [class] }], "breakMode": "always" }]);
    }
    arguments
}

pub fn switches(state: &State) -> Vec<(&Filter, bool)> {
    let Some(session) = &state.debug else {
        return Vec::new();
    };
    let on = state.exception_filters.get(&session.adapter);
    session
        .filters
        .iter()
        .map(|filter| (filter, on.is_some_and(|on| on.contains(&filter.id))))
        .collect()
}

pub fn rows(state: &State) -> usize {
    switches(state).len() + state.breakpoints.len()
}

pub fn switch(next: &mut State, at: usize) -> Vec<Effect> {
    let Some((id, on)) = switches(next)
        .get(at)
        .map(|(filter, on)| (filter.id.clone(), *on))
    else {
        return Vec::new();
    };
    let session = next.debug.as_ref().expect("a session with switches");
    let chosen = next
        .exception_filters
        .entry(session.adapter.clone())
        .or_default();
    match on {
        true => chosen.remove(&id),
        false => chosen.insert(id),
    };
    let arguments = pause_on(next);
    let session = next.debug.as_mut().expect("a session with switches");
    let mut effects = ask_all(session, "setExceptionBreakpoints", arguments);
    effects.push(Effect::SaveState(crate::state_json(next)));
    effects
}

pub fn name_class(next: &mut State, class: String) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    session.class = Some(class);
    let arguments = pause_on(next);
    let session = next.debug.as_mut().expect("a session");
    ask_all(session, "setExceptionBreakpoints", arguments)
}

/// A file left with no Breakpoints is sent an empty list, or the adapter keeps the last ones
fn set_breakpoints(
    session: &mut Session,
    to: usize,
    breakpoints: &[Breakpoint],
    file: &Path,
) -> Effect {
    let lines: Vec<Value> = breakpoints
        .iter()
        .filter(|breakpoint| breakpoint.file == file && !breakpoint.stale)
        .map(|breakpoint| {
            let properties = &breakpoint.properties;
            let mut sent = json!({ "line": breakpoint.line });
            for (key, text) in [
                ("condition", &properties.condition),
                ("hitCondition", &properties.hit_count),
                ("logMessage", &properties.log_message),
            ] {
                if !text.is_empty() {
                    sent[key] = json!(text);
                }
            }
            sent
        })
        .collect();
    ask_on(
        session,
        to,
        "setBreakpoints",
        json!({ "source": { "path": file }, "breakpoints": lines }),
    )
}

pub fn breakpoints_changed(next: &mut State, file: &Path) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    match session.phase {
        Phase::Running(_) | Phase::Paused(_) => links(session)
            .into_iter()
            .map(|link| set_breakpoints(session, link, &next.breakpoints, file))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn resume(next: &mut State) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    let effects = match &session.phase {
        Phase::Paused(pause) => {
            let (thread, last) = (pause.thread, pause.clone());
            session.phase = Phase::Running(Some(last));
            vec![ask(
                session,
                "continue",
                json!({ "threadId": thread, "singleThread": true }),
            )]
        }
        Phase::Running(_) if session.pausing.is_empty() => {
            let before = session.seq;
            let effects = ask_all(session, "threads", json!({}));
            session.pausing = (before + 1..=session.seq).collect();
            effects
        }
        _ => Vec::new(),
    };
    lit(next, RESUME, &effects);
    effects
}

fn lit(next: &mut State, action: &'static str, effects: &[Effect]) {
    if !effects.is_empty() {
        next.transport_lit = Some(action);
    }
}

pub fn step(next: &mut State, step: Step) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    let Phase::Paused(pause) = &session.phase else {
        return Vec::new();
    };
    let (thread, last) = (pause.thread, pause.clone());
    let request = match step {
        Step::Over => "next",
        Step::Into => "stepIn",
        Step::Out => "stepOut",
    };
    session.phase = Phase::Running(Some(last));
    let effects = vec![ask(session, request, json!({ "threadId": thread }))];
    lit(
        next,
        match step {
            Step::Over => STEP_OVER,
            Step::Into => STEP_INTO,
            Step::Out => STEP_OUT,
        },
        &effects,
    );
    effects
}

pub fn stop(next: &mut State) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    let effects = match session.phase {
        Phase::Stopping | Phase::Spawning | Phase::Waiting => {
            let mut effects = end(next);
            effects.push(Effect::StopDap);
            effects
        }
        _ => {
            let terminate = session.request == "launch";
            session.phase = Phase::Stopping;
            ask_all(
                session,
                "disconnect",
                json!({ "terminateDebuggee": terminate }),
            )
        }
    };
    lit(next, STOP, &effects);
    effects
}

pub fn restart(next: &mut State) -> Vec<Effect> {
    let Some(launch) = next.last_launch.clone() else {
        next.refusal = Some(Refusal::NoLastSession);
        return Vec::new();
    };
    let offered = next.debug.as_ref().is_some_and(|session| session.offered);
    let effects = match offered {
        true => {
            next.relaunch = Some(launch);
            stop(next)
        }
        false => launch_with(next, launch),
    };
    lit(next, RESTART, &effects);
    effects
}

fn hot_replace(state: &State) -> Option<&crate::startup::HotReplace> {
    let session = state.debug.as_ref()?;
    state.adapters.get(&session.adapter)?.hot_replace.as_ref()
}

pub fn replace_classes(next: &mut State) -> Vec<Effect> {
    let Some(request) = hot_replace(next).map(|named| named.request.clone()) else {
        return vec![Effect::Notify("no-hot-replace")];
    };
    let session = next.debug.as_mut().expect("a session");
    let effects = vec![ask_on(session, 0, &request, json!({}))];
    lit(next, HOT_REPLACE, &effects);
    effects
}

pub fn choose(next: &mut State, row: usize) -> Vec<Effect> {
    let chosen = frame_rows(next).get(row).cloned();
    if let Some(FrameRow::Thread {
        id, paused: true, ..
    }) = chosen
    {
        return jump(next, id);
    }
    let Some(Phase::Paused(pause)) = next.debug.as_mut().map(|s| &mut s.phase) else {
        return Vec::new();
    };
    match chosen {
        Some(FrameRow::Frame(index)) => {
            pause.chosen = index;
            next.frames_selection = row;
            inspect(next)
        }
        Some(FrameRow::Library { start, .. }) => {
            pause.unfolded.insert(start);
            Vec::new()
        }
        _ => Vec::new(),
    }
}

pub fn next_thread(next: &mut State) -> Vec<Effect> {
    let Some(session) = next.debug.as_ref() else {
        return Vec::new();
    };
    let after = match &session.phase {
        Phase::Paused(pause) => pause.thread,
        _ => i64::MIN,
    };
    let mut held = session.others.keys();
    let Some(&thread) = held.clone().find(|id| **id > after).or(held.next()) else {
        return Vec::new();
    };
    let effects = jump(next, thread);
    lit(next, NEXT_THREAD, &effects);
    effects
}

fn jump(next: &mut State, thread: i64) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    let leaving = match &session.phase {
        Phase::Paused(pause) => Some((pause.thread, (pause.why, pause.exception.clone()))),
        Phase::Running(_) => None,
        _ => return Vec::new(),
    };
    let Some((why, exception)) = session.others.remove(&thread) else {
        return Vec::new();
    };
    session.others.extend(leaving);
    session.previous = None;
    inspect_thread(session, thread, why, exception)
}

fn inspect_thread(
    session: &mut Session,
    thread: i64,
    why: Why,
    exception: Option<String>,
) -> Vec<Effect> {
    session.others.remove(&thread);
    session.phase = Phase::Paused(Pause {
        thread,
        why,
        exception,
        frames: Vec::new(),
        chosen: 0,
        scopes: Vec::new(),
        children: BTreeMap::new(),
        open: BTreeSet::new(),
        fetched: BTreeMap::new(),
        unfolded: BTreeSet::new(),
    });
    let mut effects = vec![ask(session, "stackTrace", json!({ "threadId": thread }))];
    effects.extend(ask_all(session, "threads", json!({})));
    effects
}

fn showing(state: &State) -> Option<&Pause> {
    match state.debug.as_ref().map(|session| &session.phase) {
        Some(Phase::Paused(pause)) => Some(pause),
        Some(Phase::Running(last)) => last.as_ref(),
        _ => None,
    }
}

pub fn stale(state: &State) -> bool {
    matches!(
        state.debug.as_ref().map(|session| &session.phase),
        Some(Phase::Running(Some(_)))
    )
}

pub fn title(state: &State) -> &'static str {
    if waiting_on(state).is_some() {
        return "waiting";
    }
    match (state.stepping, stale(state)) {
        (true, _) => "stepping",
        (false, true) => "running",
        (false, false) => "variables",
    }
}

pub fn frames(state: &State) -> &[Frame] {
    showing(state).map_or(&[], |pause| &pause.frames)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameRow {
    Thread {
        id: i64,
        name: String,
        paused: bool,
        child: Option<String>,
    },
    Frame(usize),
    Library {
        start: usize,
        count: usize,
    },
}

pub fn frame_rows(state: &State) -> Vec<FrameRow> {
    let (Some(session), Some(pause)) = (state.debug.as_ref(), showing(state)) else {
        return Vec::new();
    };
    let thread = |id: i64| FrameRow::Thread {
        id,
        name: session
            .threads
            .iter()
            .find(|(named, _)| *named == id)
            .map_or_else(
                || format!("thread {}", link_of(id).1),
                |(_, name)| name.clone(),
            ),
        paused: session.others.contains_key(&id),
        child: session
            .children
            .get(&link_of(id).0)
            .map(|child| child.name.clone()),
    };
    let mut rows = vec![thread(pause.thread)];
    let mut index = 0;
    while index < pause.frames.len() {
        let run = pause.frames[index..]
            .iter()
            .take_while(|frame| frame.library)
            .count();
        let folds = run > 0
            && !pause.unfolded.contains(&index)
            && !(index..index + run).contains(&pause.chosen);
        match folds {
            true => rows.push(FrameRow::Library {
                start: index,
                count: run,
            }),
            false => rows.extend((index..index + run.max(1)).map(FrameRow::Frame)),
        }
        index += run.max(1);
    }
    let mut others: Vec<i64> = session.threads.iter().map(|(id, _)| *id).collect();
    others.extend(
        session
            .others
            .keys()
            .filter(|id| !others.contains(id))
            .collect::<Vec<_>>(),
    );
    rows.extend(
        others
            .into_iter()
            .filter(|id| *id != pause.thread)
            .map(thread),
    );
    rows
}

fn row_of(state: &State, index: usize) -> usize {
    frame_rows(state)
        .iter()
        .position(|row| *row == FrameRow::Frame(index))
        .unwrap_or_default()
}

pub fn variables(state: &State) -> Vec<Row> {
    let mut rows: Vec<Row> = state
        .watches
        .iter()
        .enumerate()
        .map(|(index, watch)| Row {
            name: watch.expression.clone(),
            value: match &watch.answer {
                Answer::Waiting => String::new(),
                Answer::Value(value) | Answer::Failed(value) => value.clone(),
            },
            depth: 0,
            hint: Hint::Plain,
            open: false,
            opens: Opens::Nothing,
            expression: watch.expression.clone(),
            parent: 0,
            of: Of::Watch {
                index,
                calling: calls(&paused_in(state), &watch.expression),
                failed: matches!(watch.answer, Answer::Failed(_)),
            },
        })
        .collect();
    let Some(pause) = showing(state) else {
        return rows;
    };
    if let Some(text) = &pause.exception {
        rows.push(Row {
            name: "exception".to_string(),
            value: text.clone(),
            depth: 0,
            hint: Hint::Plain,
            open: false,
            opens: Opens::Nothing,
            expression: String::new(),
            parent: 0,
            of: Of::Member,
        });
    }
    for scope in &pause.scopes {
        draw(pause, scope, 0, "", &mut Vec::new(), &mut rows);
    }
    rows
}

pub fn paused_in(state: &State) -> String {
    file_named(paused_line(state).map(|(file, _, _)| file))
}

fn file_named(path: Option<&Path>) -> String {
    path.and_then(Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_string()
}

fn calls(named: &str, expression: &str) -> bool {
    crate::highlight::highlight(named, expression)
        .into_iter()
        .flatten()
        .any(|token| token.kind == crate::highlight::Kind::Function)
}

/// An adapter's reference can contain itself; one already on the walked path is not reopened
fn draw(
    pause: &Pause,
    member: &Member,
    depth: usize,
    path: &str,
    walked: &mut Vec<i64>,
    rows: &mut Vec<Row>,
) {
    let open = member.reference != 0
        && pause.open.contains(&member.reference)
        && !walked.contains(&member.reference);
    let expression = match depth {
        0 => String::new(),
        _ => joined(path, &member.name),
    };
    rows.push(Row {
        name: member.name.clone(),
        value: member.value.clone(),
        depth,
        hint: member.hint,
        open,
        opens: match member.reference {
            0 => Opens::Nothing,
            reference => Opens::Children {
                reference,
                indexed: member.indexed,
            },
        },
        expression: expression.clone(),
        parent: walked.last().copied().unwrap_or_default(),
        of: Of::Member,
    });
    if !open {
        return;
    }
    walked.push(member.reference);
    for child in pause.children.get(&member.reference).into_iter().flatten() {
        draw(pause, child, depth + 1, &expression, walked, rows);
    }
    walked.pop();
    let fetched = pause
        .fetched
        .get(&member.reference)
        .copied()
        .unwrap_or_default();
    if fetched < member.indexed {
        rows.push(Row {
            name: format!("{} more", member.indexed - fetched),
            value: String::new(),
            depth: depth + 1,
            hint: Hint::Plain,
            open: false,
            opens: Opens::NextPage {
                reference: member.reference,
                start: fetched,
            },
            expression: String::new(),
            parent: member.reference,
            of: Of::Member,
        });
    }
}

pub fn row(state: &State) -> Option<Row> {
    variables(state).into_iter().nth(state.variables_selection)
}

pub fn row_chips(state: &State, index: usize) -> Vec<crate::Chip> {
    use crate::{Chip, Hue, Tone};
    if index != state.variables_selection {
        return Vec::new();
    }
    let Some(row) = variables(state).into_iter().nth(index) else {
        return Vec::new();
    };
    let chip = |action, name, glyph: &str, keys, hue, dimmed| Chip {
        action,
        name,
        glyph: glyph.to_string(),
        keys,
        hue,
        tone: match dimmed {
            true => Tone::Dimmed,
            false => Tone::Plain,
        },
    };
    let nothing_to_write = row.parent == 0;
    let nothing_to_watch = row.expression.is_empty();
    let cannot_set =
        nothing_to_write || !state.debug.as_ref().is_some_and(|session| session.can_set);
    vec![
        chip(
            SET_VALUE,
            "set-value",
            "\u{270e}",
            "s",
            Hue::Step,
            cannot_set,
        ),
        chip(COPY_VALUE, "copy", "\u{29c9}", "y", Hue::Plain, false),
        match row.of {
            Of::Watch { .. } => chip(
                REMOVE_WATCH,
                "remove-watch",
                "\u{2715}",
                "d",
                Hue::Halt,
                false,
            ),
            Of::Member => chip(WATCH, "watch", "\u{25c9}", "w", Hue::Go, nothing_to_watch),
        },
        chip(
            EVALUATE,
            "evaluate",
            "\u{2261}",
            "",
            Hue::Plain,
            nothing_to_watch,
        ),
        chip(
            ROW_ASK_AI,
            "ask-ai",
            "\u{2736}",
            "",
            Hue::Plain,
            nothing_to_watch || paused_line(state).is_none(),
        ),
    ]
}

pub fn add_watch(next: &mut State, expression: String) -> Vec<Effect> {
    if expression.is_empty() || next.watches.iter().any(|w| w.expression == expression) {
        return Vec::new();
    }
    next.watches.push(Watch {
        expression: expression.clone(),
        answer: Answer::Waiting,
    });
    evaluate(next, &[expression], WATCH_CONTEXT)
}

pub fn remove_watch(next: &mut State, index: usize) {
    if index < next.watches.len() {
        next.watches.remove(index);
    }
}

fn evaluate_watches(next: &mut State) -> Vec<Effect> {
    let watches: Vec<String> = next
        .watches
        .iter()
        .map(|watch| watch.expression.clone())
        .collect();
    for watch in next.watches.iter_mut() {
        watch.answer = Answer::Waiting;
    }
    evaluate(next, &watches, WATCH_CONTEXT)
}

fn evaluate(next: &mut State, watches: &[String], context: &str) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    let Phase::Paused(pause) = &session.phase else {
        return Vec::new();
    };
    let Some(frame) = pause.frames.get(pause.chosen) else {
        return Vec::new();
    };
    let id = frame.id;
    watches
        .iter()
        .map(|expression| {
            ask(
                session,
                "evaluate",
                json!({ "expression": expression, "frameId": id, "context": context }),
            )
        })
        .collect()
}

pub fn set_value(next: &mut State, value: String) -> Vec<Effect> {
    let Some(row) = row(next) else {
        return Vec::new();
    };
    let name = row.name.clone();
    let parent = row.parent;
    let Some(session) = next.debug.as_mut().filter(|session| session.can_set) else {
        return Vec::new();
    };
    vec![ask(
        session,
        "setVariable",
        json!({ "variablesReference": parent, "name": name, "value": value }),
    )]
}

pub fn open(next: &mut State, index: usize) -> Vec<Effect> {
    match variables(next).get(index).cloned() {
        Some(row) => opened(next, row),
        None => Vec::new(),
    }
}

fn opened(next: &mut State, row: Row) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    let Phase::Paused(pause) = &mut session.phase else {
        return Vec::new();
    };
    match row.opens {
        Opens::Nothing => Vec::new(),
        Opens::Children { reference, indexed } => {
            if !pause.open.insert(reference) {
                pause.open.remove(&reference);
                return Vec::new();
            }
            match pause.children.contains_key(&reference) {
                true => Vec::new(),
                false => vec![fetch(session, reference, paged(indexed, 0))],
            }
        }
        Opens::NextPage { reference, start } => vec![fetch(session, reference, Some(start))],
    }
}

fn joined(path: &str, name: &str) -> String {
    match (path.is_empty(), name.starts_with('[')) {
        (true, _) => name.to_string(),
        (false, true) => format!("{path}{name}"),
        (false, false) => format!("{path}.{name}"),
    }
}

fn paged(indexed: usize, start: usize) -> Option<usize> {
    (indexed > PAGE).then_some(start)
}

fn fetch(session: &mut Session, reference: i64, page: Option<usize>) -> Effect {
    let arguments = match page {
        Some(start) => json!({ "variablesReference": reference, "start": start, "count": PAGE }),
        None => json!({ "variablesReference": reference }),
    };
    ask(session, "variables", arguments)
}

fn member(value: &Value) -> Member {
    let attributes = value["presentationHint"]["attributes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<&str>>();
    let hint = if value["presentationHint"]["visibility"] == "private" {
        Hint::Private
    } else if attributes.contains(&"lazy") {
        Hint::Lazy
    } else if attributes.contains(&"readOnly") {
        Hint::ReadOnly
    } else {
        Hint::Plain
    };
    Member {
        name: printable(value["name"].as_str().unwrap_or_default()),
        value: printable(value["value"].as_str().unwrap_or_default()),
        reference: value["variablesReference"].as_i64().unwrap_or_default(),
        hint,
        indexed: value["indexedVariables"].as_u64().unwrap_or_default() as usize,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hovered {
    pub expression: String,
    pub at: Place,
    pub held: Held,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    NeedsEvaluate,
    Waiting,
    Failed(String),
    Value {
        value: String,
        reference: i64,
        indexed: usize,
    },
}

pub fn hovered(next: &mut State, at: Place) -> (Option<Hovered>, Vec<Effect>) {
    if !matches!(
        next.debug.as_ref().map(|session| &session.phase),
        Some(Phase::Paused(_))
    ) {
        return (None, Vec::new());
    }
    let Some((expression, column)) = expression_at(next, at) else {
        return (None, Vec::new());
    };
    let named = file_named(next.current_buffer.as_deref());
    let at = Place {
        line: at.line,
        column,
    };
    if calls(&named, &expression) {
        return (
            Some(Hovered {
                expression,
                at,
                held: Held::NeedsEvaluate,
            }),
            Vec::new(),
        );
    }
    let effects = evaluate(next, std::slice::from_ref(&expression), HOVER);
    (
        Some(Hovered {
            expression,
            at,
            held: Held::Waiting,
        }),
        effects,
    )
}

const HOVER: &str = "hover";
const WATCH_CONTEXT: &str = "watch";

fn expression_at(state: &State, at: Place) -> Option<(String, usize)> {
    let line = line_chars(state, at.line)?;
    let on = at.column.checked_sub(1)?;
    if !line.get(on).is_some_and(|character| named(*character)) {
        return None;
    }
    let mut from = on;
    while from > 0 && named(line[from - 1]) {
        from -= 1;
    }
    let mut to = on + 1;
    while to < line.len() && named(line[to]) {
        to += 1;
    }
    if line[from].is_ascii_digit() {
        return None;
    }
    while from > 1 && line[from - 1] == '.' {
        let Some(receiver) = ends_at(&line, from - 2) else {
            break;
        };
        from = receiver;
    }
    if line.get(to) == Some(&'(') {
        let mut depth = 0;
        for (index, character) in line.iter().enumerate().skip(to) {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        to = index + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    Some((line[from..to].iter().collect(), from + 1))
}

fn line_chars(state: &State, line: usize) -> Option<Vec<char>> {
    let path = state.current_buffer.as_ref()?;
    Some(
        state
            .buffers
            .get(path)?
            .shown()
            .split('\n')
            .nth(line.checked_sub(1)?)?
            .chars()
            .collect(),
    )
}

pub fn cursor_expression(state: &State) -> String {
    if let Some(text) = state.selected_text().filter(|text| !text.is_empty()) {
        return text;
    }
    let Some(buffer) = crate::current_buffer(state) else {
        return String::new();
    };
    let at = Place {
        line: buffer.line,
        column: buffer.column,
    };
    let Some((expression, column)) = expression_at(state, at) else {
        return String::new();
    };
    let Some(line) = line_chars(state, at.line) else {
        return expression;
    };
    let from = column - 1;
    let to = chain_end(&line, from + expression.chars().count());
    line[from..to].iter().collect()
}

fn chain_end(line: &[char], mut to: usize) -> usize {
    loop {
        while let Some(open @ ('(' | '[')) = line.get(to).copied() {
            let close = match open {
                '(' => ')',
                _ => ']',
            };
            let mut depth = 0usize;
            let mut at = to;
            loop {
                match line.get(at) {
                    Some(character) if *character == open => depth += 1,
                    Some(character) if *character == close => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    Some(_) => {}
                    None => return to,
                }
                at += 1;
            }
            to = at + 1;
        }
        if line.get(to) != Some(&'.') {
            return to;
        }
        let mut after = to + 1;
        while after < line.len() && named(line[after]) {
            after += 1;
        }
        if after == to + 1 {
            return to;
        }
        to = after;
    }
}

fn named(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn ends_at(line: &[char], last: usize) -> Option<usize> {
    let mut at = last;
    loop {
        let opener = match line.get(at)? {
            ')' => '(',
            ']' => '[',
            character if named(*character) => break,
            _ => return None,
        };
        let closer = line[at];
        let mut depth = 0usize;
        loop {
            let character = *line.get(at)?;
            if character == closer {
                depth += 1;
            } else if character == opener {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            at = at.checked_sub(1)?;
        }
        at = at.checked_sub(1)?;
    }
    while at > 0 && named(line[at - 1]) {
        at -= 1;
    }
    Some(at)
}

pub fn hover_span(state: &State) -> Option<(Place, usize)> {
    let hovered = state.hover.as_ref()?.value.as_ref()?;
    Some((hovered.at, hovered.expression.chars().count()))
}

pub fn hovered_rows(state: &State) -> Vec<Row> {
    let Some(hovered) = state.hover.as_ref().and_then(|hover| hover.value.as_ref()) else {
        return Vec::new();
    };
    let Some(pause) = showing(state) else {
        return Vec::new();
    };
    let member = match &hovered.held {
        Held::NeedsEvaluate => return Vec::new(),
        Held::Waiting => Member {
            name: hovered.expression.clone(),
            value: String::new(),
            reference: 0,
            hint: Hint::Plain,
            indexed: 0,
        },
        Held::Failed(why) => Member {
            name: hovered.expression.clone(),
            value: why.clone(),
            reference: 0,
            hint: Hint::Plain,
            indexed: 0,
        },
        Held::Value {
            value,
            reference,
            indexed,
        } => Member {
            name: hovered.expression.clone(),
            value: value.clone(),
            reference: *reference,
            hint: Hint::Plain,
            indexed: *indexed,
        },
    };
    let mut rows = Vec::new();
    draw(pause, &member, 0, "", &mut Vec::new(), &mut rows);
    rows
}

pub fn hover_chips(state: &State) -> Vec<crate::Chip> {
    use crate::{Chip, Hue, Tone};
    if state
        .hover
        .as_ref()
        .and_then(|hover| hover.value.as_ref())
        .is_none()
    {
        return Vec::new();
    }
    let chip = |action, name, glyph: &str, hue, tone| Chip {
        action,
        name,
        glyph: glyph.to_string(),
        keys: "",
        hue,
        tone,
    };
    vec![
        chip(EVALUATE, "evaluate", "\u{2261}", Hue::Plain, Tone::Plain),
        chip(WATCH, "watch", "\u{25c9}", Hue::Go, Tone::Plain),
    ]
}

pub fn hover_labels(state: &State, width: u16) -> Vec<String> {
    crate::layout::chip_labels(&hover_chips(state), width, 0)
}

pub fn watch_hovered(next: &mut State) -> Vec<Effect> {
    let Some(expression) = next
        .hover
        .as_ref()
        .and_then(|hover| hover.value.as_ref())
        .map(|hovered| hovered.expression.clone())
    else {
        return Vec::new();
    };
    add_watch(next, expression)
}

pub fn open_hovered(next: &mut State, index: usize) -> Vec<Effect> {
    match hovered_rows(next).get(index).cloned() {
        Some(row) => opened(next, row),
        None => Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluator {
    pub snippet: crate::editor::Buffer,
    pub ran: Option<Run>,
    pub snippet_rows: Option<u16>,
    recalled: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrange {
    Moving,
    Sizing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    ran: String,
    printed: Vec<String>,
    answer: Ran,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ran {
    Running(i64),
    Value {
        value: String,
        reference: i64,
        indexed: usize,
    },
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Printed(String),
    Value(Row),
    Failed(String),
    Running,
}

const REPL: &str = "repl";

pub const RUN: &str = "evaluator-run";
pub const CANCEL: &str = "evaluator-cancel";
pub const CLOSE: &str = "evaluator-close";

pub fn open_evaluator(next: &mut State, expression: String) {
    if next.debug.is_none() {
        return;
    }
    next.evaluator = Some(Evaluator {
        snippet: crate::editor::Buffer::open(&expression, false, next.tab_width),
        ran: None,
        snippet_rows: None,
        recalled: None,
    });
    next.evaluator_at =
        Some(next.evaluator_at.unwrap_or_else(|| {
            crate::layout::centred_window(next.screen_width, next.screen_height)
        }));
    next.focus = Pane::Evaluator;
    next.selection = None;
}

pub fn run(next: &mut State) -> Vec<Effect> {
    if next.evaluator.is_none() {
        return Vec::new();
    }
    let text = running_text(next);
    let whole = snippet_text(next);
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    let Phase::Paused(pause) = &session.phase else {
        return Vec::new();
    };
    let Some(frame) = pause.frames.get(pause.chosen) else {
        return Vec::new();
    };
    let id = frame.id;
    let effect = ask(
        session,
        "evaluate",
        json!({ "expression": &text, "frameId": id, "context": REPL }),
    );
    let seq = session.seq;
    if let Some(evaluator) = next.evaluator.as_mut() {
        evaluator.ran = Some(Run {
            ran: text,
            printed: Vec::new(),
            answer: Ran::Running(seq),
        });
    }
    let mut effects = vec![effect];
    effects.extend(remember(next, whole));
    effects
}

pub fn cancel(next: &mut State) -> Vec<Effect> {
    let Some(Ran::Running(seq)) = next
        .evaluator
        .as_ref()
        .and_then(|evaluator| evaluator.ran.as_ref())
        .map(|ran| ran.answer.clone())
    else {
        return Vec::new();
    };
    let Some(session) = next.debug.as_mut().filter(|session| session.can_cancel) else {
        return Vec::new();
    };
    let to = session.asked.get(&seq).map_or(0, |asked| asked.to);
    vec![ask_on(session, to, "cancel", json!({ "requestId": seq }))]
}

pub fn place(next: &mut State, at: crate::layout::Area) -> Vec<Effect> {
    if next.evaluator.is_none() {
        return Vec::new();
    }
    next.evaluator_at = Some(crate::layout::placed_window(
        at,
        next.screen_width,
        next.screen_height,
        paused_row(next),
    ));
    vec![Effect::SaveState(crate::state_json(next))]
}

pub fn arrange(next: &mut State, direction: crate::Direction, how: Arrange) -> Vec<Effect> {
    use crate::Direction::{Down, Left, Right, Up};
    let Some(at) = next.evaluator_at else {
        return Vec::new();
    };
    let moved = match (how, direction) {
        (Arrange::Moving, Left) => crate::layout::Area {
            x: at.x.saturating_sub(1),
            ..at
        },
        (Arrange::Moving, Right) => crate::layout::Area { x: at.x + 1, ..at },
        (Arrange::Moving, Up) => crate::layout::Area {
            y: at.y.saturating_sub(1),
            ..at
        },
        (Arrange::Moving, Down) => crate::layout::Area { y: at.y + 1, ..at },
        (Arrange::Sizing, Left) => crate::layout::Area {
            width: at.width.saturating_sub(1),
            ..at
        },
        (Arrange::Sizing, Right) => crate::layout::Area {
            width: at.width + 1,
            ..at
        },
        (Arrange::Sizing, Up) => crate::layout::Area {
            height: at.height.saturating_sub(1),
            ..at
        },
        (Arrange::Sizing, Down) => crate::layout::Area {
            height: at.height + 1,
            ..at
        },
    };
    place(next, moved)
}

pub fn paused_row(state: &State) -> Option<u16> {
    let (file, line, _) = paused_line(state)?;
    if state.current_buffer.as_deref() != Some(file) {
        return None;
    }
    let editor = crate::panes_of(state).editor;
    let row = u16::try_from(line.checked_sub(1)?.checked_sub(state.editor_scroll)?).ok()?;
    let at = editor.y + 1 + row;
    (at < editor.bottom().saturating_sub(1)).then_some(at)
}

pub fn recall(next: &mut State, direction: crate::Direction) -> bool {
    use crate::Direction::{Down, Up};
    let newest = match next.snippets.len() {
        0 => return false,
        held => held - 1,
    };
    let tab_width = next.tab_width;
    let Some(evaluator) = next.evaluator.as_mut() else {
        return false;
    };
    if evaluator.recalled.is_none() && !evaluator.snippet.shown().is_empty() {
        return false;
    }
    let at = match (evaluator.recalled, direction) {
        (None, Up) => newest,
        (Some(at), Up) => at.saturating_sub(1),
        (Some(at), Down) => (at + 1).min(newest),
        (None, Down) => return false,
        (_, crate::Direction::Left | crate::Direction::Right) => return false,
    };
    evaluator.recalled = Some(at);
    let recalled = next.snippets[at].clone();
    if let Some(evaluator) = next.evaluator.as_mut() {
        evaluator.snippet = crate::editor::Buffer::open(&recalled, false, tab_width);
    }
    true
}

fn running_text(state: &State) -> String {
    let Some(evaluator) = state.evaluator.as_ref() else {
        return String::new();
    };
    match state
        .selection
        .as_ref()
        .and_then(crate::Selection::buffer_span)
    {
        Some((from, to)) if state.focus == Pane::Evaluator => evaluator.snippet.text_in(from, to),
        _ => evaluator.snippet.shown().to_string(),
    }
}

fn snippet_text(state: &State) -> String {
    state
        .evaluator
        .as_ref()
        .map(|evaluator| evaluator.snippet.shown().to_string())
        .unwrap_or_default()
}

fn remember(next: &mut State, snippet: String) -> Vec<Effect> {
    if snippet.is_empty() {
        return Vec::new();
    }
    next.snippets.retain(|held| held != &snippet);
    next.snippets.push(snippet);
    vec![Effect::SaveState(crate::state_json(next))]
}

pub fn evaluator_chips(state: &State) -> Vec<crate::Chip> {
    use crate::{Chip, Hue, Tone};
    let Some(evaluator) = state.evaluator.as_ref() else {
        return Vec::new();
    };
    let running = matches!(
        evaluator.ran.as_ref().map(|ran| &ran.answer),
        Some(Ran::Running(_))
    );
    let stopped = matches!(
        state.debug.as_ref().map(|session| &session.phase),
        Some(Phase::Paused(_))
    );
    let can_cancel = state
        .debug
        .as_ref()
        .is_some_and(|session| session.can_cancel);
    vec![
        Chip {
            action: RUN,
            name: "run",
            glyph: "\u{25b6}".to_string(),
            keys: "\u{21b5}",
            hue: Hue::Go,
            tone: match stopped {
                true => Tone::Plain,
                false => Tone::Dimmed,
            },
        },
        Chip {
            action: CANCEL,
            name: "cancel",
            glyph: "\u{25a0}".to_string(),
            keys: "",
            hue: Hue::Halt,
            tone: match running && can_cancel {
                true => Tone::Plain,
                false => Tone::Dimmed,
            },
        },
        Chip {
            action: CLOSE,
            name: "close",
            glyph: "\u{2715}".to_string(),
            keys: "Esc",
            hue: Hue::Halt,
            tone: Tone::Plain,
        },
    ]
}

pub fn evaluator_labels(state: &State, width: u16) -> Vec<String> {
    crate::layout::chip_labels(&evaluator_chips(state), width, 0)
}

pub fn evaluator_output(state: &State) -> Vec<Said> {
    let Some(ran) = state
        .evaluator
        .as_ref()
        .and_then(|evaluator| evaluator.ran.as_ref())
    else {
        return Vec::new();
    };
    let mut lines: Vec<Said> = ran.printed.iter().cloned().map(Said::Printed).collect();
    match &ran.answer {
        Ran::Running(_) => lines.push(Said::Running),
        Ran::Failed(why) => lines.push(Said::Failed(why.clone())),
        Ran::Value {
            value,
            reference,
            indexed,
        } => {
            let Some(pause) = showing(state) else {
                return lines;
            };
            let member = Member {
                name: ran.ran.clone(),
                value: value.clone(),
                reference: *reference,
                hint: Hint::Plain,
                indexed: *indexed,
            };
            let mut rows = Vec::new();
            draw(pause, &member, 0, "", &mut Vec::new(), &mut rows);
            lines.extend(rows.into_iter().map(Said::Value));
        }
    }
    lines
}

pub fn code(state: &State) -> Vec<String> {
    let values = evaluator_output(state)
        .into_iter()
        .filter_map(|said| match said {
            Said::Value(row) => Some(row),
            _ => None,
        });
    variables(state)
        .into_iter()
        .filter(|row| !matches!(row.of, Of::Watch { failed: true, .. }))
        .chain(values)
        .map(|row| row.value)
        .chain(
            state
                .evaluator
                .as_ref()
                .map(|open| open.snippet.shown().to_string()),
        )
        .filter(|text| !text.is_empty())
        .collect()
}

pub fn open_evaluated(next: &mut State, index: usize) -> Vec<Effect> {
    match evaluator_output(next).into_iter().nth(index) {
        Some(Said::Value(row)) => opened(next, row),
        _ => Vec::new(),
    }
}

fn in_flight(next: &mut State) -> Option<&mut Run> {
    next.evaluator
        .as_mut()?
        .ran
        .as_mut()
        .filter(|ran| matches!(ran.answer, Ran::Running(_)))
}

fn ran_asked<'a>(next: &'a mut State, arguments: &Value, seq: i64) -> Option<&'a mut Run> {
    if arguments["context"] != json!(REPL) {
        return None;
    }
    in_flight(next).filter(|ran| ran.answer == Ran::Running(seq))
}

pub fn snapshot(state: &State) -> Option<String> {
    let (file, line, _) = paused_line(state)?;
    let pause = showing(state)?;
    let mark = |here: bool| if here { '\u{2192}' } else { ' ' };
    let mut text = format!(
        "My program is Paused at {}:{line}.\n",
        crate::relative(state, file)
    );
    if let Some(buffer) = state.buffers.get(file) {
        let lines = buffer.lines();
        text.push('\n');
        for number in line.saturating_sub(2).max(1)..=(line + 2).min(lines.len()) {
            text.push_str(&format!(
                "{} {number:>4} | {}\n",
                mark(number == line),
                lines[number - 1]
            ));
        }
    }
    text.push_str("\nFrames:\n");
    for (index, frame) in pause.frames.iter().enumerate() {
        let at = frame.file.as_deref().map_or(String::new(), |file| {
            format!("  {}:{}", crate::relative(state, file), frame.line)
        });
        text.push_str(&format!(
            "{} {}{at}\n",
            mark(index == pause.chosen),
            frame.name
        ));
    }
    text.push_str("\nVariables:\n");
    for row in variables(state) {
        let indent = "  ".repeat(row.depth + 1);
        text.push_str(&format!("{indent}{} = {}\n", row.name, row.value));
    }
    Some(text)
}

pub fn paused_line(state: &State) -> Option<(&Path, usize, Why)> {
    let Some(Phase::Paused(pause)) = state.debug.as_ref().map(|session| &session.phase) else {
        return None;
    };
    let frame = pause.frames.get(pause.chosen)?;
    Some((frame.file.as_deref()?, frame.line, pause.why))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inline {
    pub name: String,
    pub text: String,
    pub changed: bool,
}

const GAP: usize = 2;

pub fn inline(
    state: &State,
    tokens: &[Vec<crate::highlight::Token>],
    columns: usize,
) -> BTreeMap<usize, Vec<Inline>> {
    let mut drawn = BTreeMap::new();
    let (Some(session), Some(pause)) = (state.debug.as_ref(), showing(state)) else {
        return drawn;
    };
    let Some(frame) = pause.frames.get(pause.chosen) else {
        return drawn;
    };
    let Some(buffer) = frame
        .file
        .as_ref()
        .filter(|file| state.current_buffer.as_ref() == Some(*file))
        .and_then(|file| state.buffers.get(file))
    else {
        return drawn;
    };
    let held = locals(pause);
    let was = session
        .previous
        .as_ref()
        .filter(|_| matches!(session.phase, Phase::Paused(_)))
        .filter(|(called, _)| *called == frame.name)
        .map(|(_, held)| held);
    let source = buffer.shown();
    let opens = call_start(source, frame.line);
    for (index, line) in source
        .split('\n')
        .enumerate()
        .skip(opens - 1)
        .take(frame.line.saturating_sub(opens))
    {
        let number = index + 1;
        let mut room = columns.saturating_sub(line.width());
        let mut values: Vec<Inline> = Vec::new();
        for name in mentioned(tokens.get(number - 1).map_or(&[], Vec::as_slice)) {
            let Some(value) = held.get(name) else {
                continue;
            };
            if values.iter().any(|shown| shown.name == name) {
                continue;
            }
            let head = format!("{}{name} = ", " ".repeat(GAP));
            let spare = room.saturating_sub(head.width());
            if spare == 0 {
                break;
            }
            let value = clipped(value, spare);
            room -= head.width() + value.width();
            values.push(Inline {
                name: name.to_string(),
                text: head + &value,
                changed: was.is_some_and(|was| was.get(name) != held.get(name)),
            });
        }
        if !values.is_empty() {
            drawn.insert(number, values);
        }
    }
    drawn
}

fn clipped(text: &str, columns: usize) -> String {
    let mut left = columns;
    let mut kept = String::new();
    for character in text.chars() {
        let width = character.width().unwrap_or(0);
        if width > left {
            break;
        }
        left -= width;
        kept.push(character);
    }
    kept
}

fn locals(pause: &Pause) -> BTreeMap<String, String> {
    pause
        .scopes
        .iter()
        .filter_map(|scope| pause.children.get(&scope.reference))
        .flatten()
        .map(|member| (member.name.clone(), member.value.clone()))
        .collect()
}

fn mentioned(tokens: &[crate::highlight::Token]) -> Vec<&str> {
    tokens
        .iter()
        .filter(|token| token.kind == crate::highlight::Kind::Plain)
        .flat_map(|token| token.text.split(|c: char| !c.is_alphanumeric() && c != '_'))
        .filter(|word| !word.is_empty())
        .collect()
}

fn call_start(source: &str, line: usize) -> usize {
    crate::fold::blocks(source)
        .into_iter()
        .filter(|block| block.from < line && line <= block.to)
        .map(|block| block.from)
        .max()
        .unwrap_or(1)
}

fn inspect(next: &mut State) -> Vec<Effect> {
    let mut effects = match paused_line(next) {
        Some((path, line, _)) if next.current_buffer.as_deref() != Some(path) => {
            vec![Effect::OpenAt {
                path: path.to_path_buf(),
                at: Place { line, column: 1 },
            }]
        }
        _ => Vec::new(),
    };
    let Some(session) = next.debug.as_mut() else {
        return effects;
    };
    let Phase::Paused(pause) = &session.phase else {
        return effects;
    };
    if let Some(frame) = pause.frames.get(pause.chosen) {
        let id = frame.id;
        effects.push(ask(session, "scopes", json!({ "frameId": id })));
    }
    effects.extend(evaluate_watches(next));
    effects
}

pub fn resting_corner(state: &State) -> layout::Corner {
    state
        .debug
        .as_ref()
        .map_or(state.corner, |session| session.corner)
}

fn end(next: &mut State) -> Vec<Effect> {
    next.corner = resting_corner(next);
    next.strip = next
        .debug
        .as_ref()
        .map_or(next.strip, |session| session.strip);
    next.debug = None;
    next.stepping = false;
    next.output_unseen = false;
    if matches!(next.focus, Pane::Frames | Pane::Variables) {
        next.focus = Pane::Editor;
    }
    close_evaluator(next)
}

pub fn close_evaluator(next: &mut State) -> Vec<Effect> {
    if next.focus == Pane::Evaluator {
        next.focus = Pane::Editor;
    }
    let snippet = snippet_text(next);
    next.evaluator = None;
    remember(next, snippet)
}

fn hover_asked<'a>(next: &'a mut State, arguments: &Value) -> Option<&'a mut Hovered> {
    if arguments["context"] != json!(HOVER) {
        return None;
    }
    let expression = arguments["expression"].as_str()?;
    next.hover
        .as_mut()?
        .value
        .as_mut()
        .filter(|hovered| hovered.expression == expression)
}

fn watch_asked<'a>(next: &'a mut State, arguments: &Value) -> Option<&'a mut Watch> {
    let expression = arguments["expression"].as_str()?;
    next.watches
        .iter_mut()
        .find(|watch| watch.expression == expression)
}

fn adapter_error(message: &Value, command: &str) -> String {
    printable(
        message["body"]["error"]["format"]
            .as_str()
            .or(message["message"].as_str())
            .unwrap_or(command),
    )
}

pub(crate) fn printable(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .collect()
}

fn ask(session: &mut Session, command: &str, arguments: Value) -> Effect {
    let to = match (arguments["threadId"].as_i64(), &session.phase) {
        (Some(thread), _) => link_of(thread).0,
        (None, Phase::Paused(pause) | Phase::Running(Some(pause))) => link_of(pause.thread).0,
        (None, _) => 0,
    };
    ask_on(session, to, command, arguments)
}

fn ask_on(session: &mut Session, to: usize, command: &str, arguments: Value) -> Effect {
    session.seq += 1;
    session.asked.insert(
        session.seq,
        Ask {
            command: command.to_string(),
            arguments: arguments.clone(),
            to,
        },
    );
    let mut sent = arguments;
    if let Some(thread) = sent["threadId"].as_i64() {
        sent["threadId"] = json!(link_of(thread).1);
    }
    Effect::DapSend {
        to,
        json: json!({
            "seq": session.seq,
            "type": "request",
            "command": command,
            "arguments": sent,
        })
        .to_string(),
    }
}

fn ask_all(session: &mut Session, command: &str, arguments: Value) -> Vec<Effect> {
    links(session)
        .into_iter()
        .map(|to| ask_on(session, to, command, arguments.clone()))
        .collect()
}

fn links(session: &Session) -> Vec<usize> {
    std::iter::once(0)
        .chain(session.children.keys().copied())
        .collect()
}

fn forget(session: &mut Session, child: usize) {
    session.children.remove(&child);
    session.threads.retain(|(id, _)| link_of(*id).0 != child);
    session.others.retain(|id, _| link_of(*id).0 != child);
    if let Phase::Paused(pause) | Phase::Running(Some(pause)) = &session.phase {
        if link_of(pause.thread).0 == child {
            session.phase = Phase::Running(None);
        }
    }
}

fn reply(session: &mut Session, to: usize, request: &Value, mut answer: Value) -> Effect {
    session.seq += 1;
    answer["seq"] = json!(session.seq);
    answer["type"] = json!("response");
    answer["request_seq"] = request["seq"].clone();
    Effect::DapSend {
        to,
        json: answer.to_string(),
    }
}

#[cfg(test)]
pub(crate) fn outstanding(state: &State, command: &str) -> i64 {
    let session = state.debug.as_ref().expect("a session");
    *session
        .asked
        .iter()
        .find(|(_, asked)| asked.command == command)
        .unwrap_or_else(|| panic!("nothing is waiting on {command:?}"))
        .0
}

#[cfg(test)]
pub(crate) fn paused(mut state: State) -> State {
    state.adapters.insert(
        "rust".to_string(),
        crate::startup::Adapter {
            command: "adapter".to_string(),
            args: Vec::new(),
            install: BTreeMap::new(),
            server: None,
            plugin: None,
            hot_replace: None,
        },
    );
    state.launches.insert(
        "app".to_string(),
        crate::startup::Launch {
            adapter: "rust".to_string(),
            request: "launch".to_string(),
            args: serde_json::Map::new(),
            reattach: true,
        },
    );
    start(&mut state, "app");
    started(&mut state, 0);
    let seq = outstanding(&state, "initialize");
    received(
        &mut state,
        &json!({"type": "response", "request_seq": seq, "success": true, "command": "initialize"})
            .to_string(),
        0,
    );
    received(&mut state, r#"{"type":"event","event":"initialized"}"#, 0);
    received(
        &mut state,
        r#"{"type":"event","event":"stopped","body":{"threadId":1,"reason":"breakpoint"}}"#,
        0,
    );
    let seq = outstanding(&state, "stackTrace");
    received(
        &mut state,
        &json!({"type": "response", "request_seq": seq, "success": true, "command": "stackTrace",
            "body": {"stackFrames": [{"id": 1, "name": "main", "line": 1, "source": {"path": "/w/one.rs"}}]}})
        .to_string(),
        0,
    );
    crate::settle(state, Vec::new(), false).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn received(next: &mut State, json: &str) -> Vec<Effect> {
        super::received(next, json, 0)
    }

    fn started(next: &mut State) -> Vec<Effect> {
        super::started(next, 0)
    }

    #[test]
    fn a_cursor_names_the_whole_chain_it_stands_in() {
        let named = |line: &str, column: usize| {
            let mut state = State::default();
            let path = PathBuf::from("/w/one.rs");
            state
                .buffers
                .insert(path.clone(), crate::editor::Buffer::open(line, false, 4));
            let buffer = state.buffers.get_mut(&path).expect("just inserted");
            buffer.line = 1;
            buffer.column = column;
            state.current_buffer = Some(path);
            cursor_expression(&state)
        };
        assert_eq!(named("    let n = orders.len();", 17), "orders.len()");
        assert_eq!(named("a.b().c[0].d", 1), "a.b().c[0].d");
        assert_eq!(named("a.b().c[0].d", 7), "a.b().c[0].d");
        assert_eq!(named("x.f(g(1)).y", 1), "x.f(g(1)).y");
        assert_eq!(named("total.", 1), "total");
        assert_eq!(named("n + 1.5", 1), "n");
        assert_eq!(named("a.b(1", 1), "a.b");
        assert_eq!(named("    let n = 7;", 13), "");
    }

    #[test]
    fn a_place_names_the_expression_the_syntax_around_it_makes() {
        let named = |line: &str, column: usize| {
            let mut state = State::default();
            let path = PathBuf::from("/w/one.rs");
            state
                .buffers
                .insert(path.clone(), crate::editor::Buffer::open(line, false, 4));
            state.current_buffer = Some(path);
            expression_at(&state, Place { line: 1, column })
        };
        assert_eq!(
            named("    let order = load(7);", 9),
            Some(("order".to_string(), 9))
        );
        assert_eq!(
            named("    one.two.three = 1", 13),
            Some(("one.two.three".to_string(), 5))
        );
        assert_eq!(
            named("    delete_order(order.id);", 5),
            Some(("delete_order(order.id)".to_string(), 5))
        );
        assert_eq!(
            named("    let n = a.count(b(c));", 15),
            Some(("a.count(b(c))".to_string(), 13))
        );
        assert_eq!(
            named("    let n = get().total;", 19),
            Some(("get().total".to_string(), 13))
        );
        assert_eq!(
            named("    let n = v[i].total;", 18),
            Some(("v[i].total".to_string(), 13))
        );
        assert_eq!(
            named("    let n = a.b()[0].c;", 22),
            Some(("a.b()[0].c".to_string(), 13))
        );
        assert_eq!(
            named("    let n = 1 + .total;", 18),
            Some(("total".to_string(), 18))
        );
        assert_eq!(named("    let order = load(7);", 22), None);
        assert_eq!(named("    let order = load(7);", 15), None);
        assert_eq!(named("    let order = load(7);", 90), None);
    }

    #[test]
    fn opening_on_a_selection_does_not_leave_it_to_be_read_against_the_snippet() {
        let mut state = paused(State::default());
        let path = PathBuf::from("/w/one.rs");
        state.buffers.insert(
            path.clone(),
            crate::editor::Buffer::open("    let count = orders.len();\n", false, 4),
        );
        state.current_buffer = Some(path);
        state.selection = Some(crate::Selection::Buffer {
            anchor: Place {
                line: 1,
                column: 17,
            },
            cursor: Place {
                line: 1,
                column: 28,
            },
        });
        open_evaluator(&mut state, "orders.len()".to_string());
        assert_eq!(running_text(&state), "orders.len()");
    }

    #[test]
    fn a_refused_hover_evaluate_says_why_in_the_box() {
        let mut state = paused(State::default());
        let path = PathBuf::from("/w/one.rs");
        state.buffers.insert(
            path.clone(),
            crate::editor::Buffer::open("    let order = load(7);\n", false, 4),
        );
        state.current_buffer = Some(path.clone());
        let at = Place { line: 1, column: 9 };
        crate::lsp::value_hover(&mut state, at);
        let seq = outstanding(&state, "evaluate");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": false,
                "command": "evaluate", "message": "not available"})
            .to_string(),
        );
        assert_eq!(
            state
                .hover
                .as_ref()
                .and_then(|hover| hover.value.as_ref())
                .map(|hovered| hovered.held.clone()),
            Some(Held::Failed("not available".to_string()))
        );
    }

    #[test]
    fn a_hover_asks_for_everything_but_a_call() {
        let mut state = paused(State::default());
        let path = PathBuf::from("/w/one.rs");
        state.buffers.insert(
            path.clone(),
            crate::editor::Buffer::open("    delete_order(order.id);\n", false, 4),
        );
        state.current_buffer = Some(path);
        let (over_call, effects) = hovered(&mut state, Place { line: 1, column: 5 });
        assert_eq!(over_call.map(|box_| box_.held), Some(Held::NeedsEvaluate));
        assert_eq!(effects, Vec::new(), "the adapter was asked to run a call");
        let (over_argument, effects) = hovered(
            &mut state,
            Place {
                line: 1,
                column: 24,
            },
        );
        assert_eq!(
            over_argument.map(|box_| box_.expression),
            Some("order.id".to_string())
        );
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn the_initialize_reply_is_where_the_set_value_capability_comes_from() {
        let plain = paused(State::default());
        assert!(!plain.debug.as_ref().expect("a session").can_set);
        let mut can = State::default();
        can.adapters.insert(
            "rust".to_string(),
            crate::startup::Adapter {
                command: "adapter".to_string(),
                args: Vec::new(),
                install: BTreeMap::new(),
                server: None,
                plugin: None,
                hot_replace: None,
            },
        );
        can.launches.insert(
            "app".to_string(),
            crate::startup::Launch {
                adapter: "rust".to_string(),
                request: "launch".to_string(),
                args: serde_json::Map::new(),
                reattach: true,
            },
        );
        start(&mut can, "app");
        started(&mut can);
        let seq = outstanding(&can, "initialize");
        received(
            &mut can,
            &json!({"type": "response", "request_seq": seq, "success": true,
                "command": "initialize", "body": {"supportsSetVariable": true}})
            .to_string(),
        );
        assert!(can.debug.as_ref().expect("a session").can_set);
        received(
            &mut can,
            r#"{"type":"event","event":"capabilities","body":{"capabilities":{"supportsSetVariable":false}}}"#,
        );
        assert!(!can.debug.as_ref().expect("a session").can_set);
    }

    #[test]
    fn exception_requests_carry_exactly_what_is_on() {
        let mut state = paused(State::default());
        state.exception_filters.insert(
            "rust".to_string(),
            ["gone".to_string()].into_iter().collect(),
        );
        received(
            &mut state,
            r#"{"type":"event","event":"capabilities","body":{"capabilities":{"exceptionBreakpointFilters":[{"filter":"caught","label":"Caught"}]}}}"#,
        );
        let sent = |effects: &[Effect]| {
            effects
                .iter()
                .find_map(|effect| match effect {
                    Effect::DapSend { json, .. } => serde_json::from_str::<Value>(json).ok(),
                    _ => None,
                })
                .expect("a request")["arguments"]
                .clone()
        };
        assert_eq!(sent(&switch(&mut state, 0))["filters"], json!(["caught"]));
        let named = sent(&name_class(&mut state, "Oops".to_string()));
        assert_eq!(named["filters"], json!(["caught"]));
        let off = sent(&switch(&mut state, 0));
        assert_eq!(off["filters"], json!([]));
        assert_eq!(off["exceptionOptions"], named["exceptionOptions"]);
    }

    #[test]
    fn a_typed_watch_joins_the_watches_once() {
        let mut state = State::default();
        add_watch(&mut state, "orders.len()".to_string());
        add_watch(&mut state, "orders.len()".to_string());
        add_watch(&mut state, String::new());
        assert_eq!(
            state
                .watches
                .iter()
                .map(|watch| watch.expression.as_str())
                .collect::<Vec<&str>>(),
            ["orders.len()"]
        );
        remove_watch(&mut state, 5);
        assert_eq!(state.watches.len(), 1);
        remove_watch(&mut state, 0);
        assert!(state.watches.is_empty());
    }

    #[test]
    fn a_set_value_is_filed_against_the_member_that_was_written() {
        let mut state = paused(State::default());
        state.debug.as_mut().expect("a session").can_set = true;
        let seq = outstanding(&state, "scopes");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true, "command": "scopes",
                "body": {"scopes": [{"name": "Locals", "variablesReference": 1}]}})
            .to_string(),
        );
        let seq = outstanding(&state, "variables");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true, "command": "variables",
                "body": {"variables": [
                    {"name": "first", "value": "…", "variablesReference": 2},
                    {"name": "second", "value": "…", "variablesReference": 3},
                    {"name": "other", "value": "7", "variablesReference": 0}]}})
            .to_string(),
        );
        for (row, reference) in [(1, 2), (3, 3)] {
            open(&mut state, row);
            let seq = outstanding(&state, "variables");
            received(
                &mut state,
                &json!({"type": "response", "request_seq": seq, "success": true,
                    "command": "variables", "body": {"variables":
                        [{"name": "count", "value": "0", "variablesReference": 0}]}})
                .to_string(),
            );
            assert_eq!(
                variables(&state)[row].opens,
                Opens::Children {
                    reference,
                    indexed: 0
                }
            );
        }
        state.variables_selection = 2;
        let effects = set_value(&mut state, "9".to_string());
        assert_eq!(effects.len(), 1, "one setVariable");
        state.variables_selection = 5;
        let seq = outstanding(&state, "setVariable");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true,
                "command": "setVariable", "body": {"value": "9"}})
            .to_string(),
        );
        let written: Vec<(String, String)> = variables(&state)
            .into_iter()
            .map(|row| (row.expression, row.value))
            .collect();
        assert_eq!(
            written,
            [
                (String::new(), String::new()),
                ("first".to_string(), "…".to_string()),
                ("first.count".to_string(), "9".to_string()),
                ("second".to_string(), "…".to_string()),
                ("second.count".to_string(), "0".to_string()),
                ("other".to_string(), "7".to_string()),
            ]
        );
    }

    #[test]
    fn an_expression_is_the_path_to_the_member() {
        assert_eq!(joined("", "orders"), "orders");
        assert_eq!(joined("orders", "[0]"), "orders[0]");
        assert_eq!(joined("orders[0]", "id"), "orders[0].id");
    }

    fn on(line: usize, text: &str) -> Breakpoint {
        Breakpoint {
            file: PathBuf::from("/w/a.rs"),
            line,
            text: text.to_string(),
            stale: false,
            properties: Default::default(),
        }
    }

    #[test]
    fn a_breakpoint_is_drawn_as_what_it_does_when_hit() {
        let drawn = |stale: bool, condition: &str, log_message: &str| {
            let state = State {
                current_buffer: Some(PathBuf::from("/w/a.rs")),
                breakpoints: vec![Breakpoint {
                    stale,
                    properties: Properties {
                        condition: condition.to_string(),
                        log_message: log_message.to_string(),
                        ..Properties::default()
                    },
                    ..on(1, "")
                }],
                ..State::default()
            };
            marks(&state)[&1]
        };
        assert_eq!(drawn(false, "", ""), Mark::Plain);
        assert_eq!(drawn(false, "x > 1", ""), Mark::Conditional);
        assert_eq!(drawn(false, "x > 1", "x is {x}"), Mark::Logpoint);
        assert_eq!(drawn(true, "x > 1", "x is {x}"), Mark::Stale);
    }

    fn followed(breakpoints: &[Breakpoint], old: &str, new: &str) -> Vec<(usize, String)> {
        let mut breakpoints = breakpoints.to_vec();
        follow(&mut breakpoints, Path::new("/w/a.rs"), old, new);
        breakpoints
            .into_iter()
            .map(|breakpoint| (breakpoint.line, breakpoint.text))
            .collect()
    }

    #[test]
    fn a_breakpoint_rides_the_lines_above_it_and_dies_with_its_own() {
        let old = "a\nb\nc\nd";
        let both = [on(1, "a"), on(3, "c")];
        assert_eq!(
            followed(&both, old, "a\nnew\nb\nc\nd"),
            [(1, "a".to_string()), (4, "c".to_string())]
        );
        assert_eq!(
            followed(&both, old, "a\nc\nd"),
            [(1, "a".to_string()), (2, "c".to_string())]
        );
        assert_eq!(followed(&both, old, "a\nb\nd"), [(1, "a".to_string())]);
        assert_eq!(
            followed(&both, old, "new\na\nb\nc\nd"),
            [(2, "a".to_string()), (4, "c".to_string())]
        );
    }

    #[test]
    fn a_line_edited_in_place_keeps_its_breakpoint_and_takes_its_text() {
        assert_eq!(
            followed(&[on(2, "b")], "a\nb\nc", "a\n    b2\nc"),
            [(2, "b2".to_string())]
        );
        assert_eq!(
            followed(&[on(2, "b"), on(3, "c")], "a\nb\nc\nd", "a\nx\nd"),
            [(2, "x".to_string())]
        );
    }

    #[test]
    fn a_stale_breakpoint_moves_but_keeps_its_remembered_text() {
        let stale = Breakpoint {
            stale: true,
            ..on(2, "let limit = 9;")
        };
        let mut breakpoints = vec![stale];
        follow(&mut breakpoints, Path::new("/w/a.rs"), "a\nb", "new\na\nb");
        assert_eq!(breakpoints[0].line, 3);
        assert_eq!(breakpoints[0].text, "let limit = 9;");
    }

    fn sent(effects: &[Effect]) -> Vec<Value> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::DapSend { json, .. } => serde_json::from_str(json).ok(),
                _ => None,
            })
            .collect()
    }

    fn sent_on(effects: &[Effect]) -> Vec<(usize, Value)> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::DapSend { to, json } => Some((*to, serde_json::from_str(json).ok()?)),
                _ => None,
            })
            .collect()
    }

    fn with_child() -> (State, usize) {
        let mut state = paused(State::default());
        let effects = received(
            &mut state,
            r#"{"seq":40,"type":"request","command":"startDebugging","arguments":
                {"request":"attach","configuration":{"name":"worker.js","port":9229}}}"#,
        );
        let child = match effects.last() {
            Some(Effect::DapChild { child }) => *child,
            other => panic!("no child opened: {other:?}"),
        };
        let (to, reply) = &sent_on(&effects)[0];
        assert_eq!(
            (*to, &reply["request_seq"], &reply["success"]),
            (0, &json!(40), &json!(true))
        );
        let initialize = sent_on(&super::started(&mut state, child));
        assert_eq!(initialize[0].0, child);
        let answer = |seq: &Value, command: &str, body: Value| {
            json!({"type": "response", "request_seq": seq, "success": true,
                "command": command, "body": body})
            .to_string()
        };
        let attach = sent_on(&super::received(
            &mut state,
            &answer(&initialize[0].1["seq"], "initialize", json!({})),
            child,
        ));
        assert_eq!(attach[0].0, child);
        assert_eq!(
            (&attach[0].1["command"], &attach[0].1["arguments"]["port"]),
            (&json!("attach"), &json!(9229))
        );
        super::received(
            &mut state,
            r#"{"type":"event","event":"initialized"}"#,
            child,
        );
        let asked = sent_on(&super::received(
            &mut state,
            r#"{"type":"event","event":"stopped","body":{"threadId":1,"reason":"breakpoint"}}"#,
            child,
        ));
        let (_, threads) = asked
            .iter()
            .find(|(to, message)| *to == child && message["command"] == "threads")
            .expect("the child asked for its threads");
        super::received(
            &mut state,
            &answer(
                &threads["seq"],
                "threads",
                json!({"threads": [{"id": 1, "name": "worker"}]}),
            ),
            child,
        );
        (state, child)
    }

    #[test]
    fn a_childs_threads_join_the_frames_under_its_name() {
        let (state, _) = with_child();
        let threads: Vec<(String, bool, Option<String>)> = frame_rows(&state)
            .into_iter()
            .filter_map(|row| match row {
                FrameRow::Thread {
                    name,
                    paused,
                    child,
                    ..
                } => Some((name, paused, child)),
                _ => None,
            })
            .collect();
        assert_eq!(
            threads,
            [
                ("thread 1".to_string(), false, None),
                ("worker".to_string(), true, Some("worker.js".to_string())),
            ]
        );
    }

    #[test]
    fn a_step_on_a_childs_thread_is_asked_of_the_child() {
        let (mut state, child) = with_child();
        next_thread(&mut state);
        let asked = sent_on(&step(&mut state, Step::Over));
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].0, child);
        assert_eq!(
            (&asked[0].1["command"], &asked[0].1["arguments"]["threadId"]),
            (&json!("next"), &json!(1))
        );
    }

    #[test]
    fn a_child_that_goes_takes_only_its_threads() {
        let (mut state, child) = with_child();
        assert_eq!(super::gone(&mut state, Gone::Exited, child), []);
        assert!(state.debug.is_some());
        assert_eq!(state.refusal, None);
        let rows = frame_rows(&state);
        assert!(!rows
            .iter()
            .any(|row| matches!(row, FrameRow::Thread { child: Some(_), .. })));
    }

    #[test]
    fn a_child_that_will_not_start_is_let_go_and_the_session_goes_on() {
        let mut state = paused(State::default());
        let effects = received(
            &mut state,
            r#"{"seq":40,"type":"request","command":"startDebugging","arguments":
                {"request":"launch","configuration":{"name":"worker.js"}}}"#,
        );
        let Some(&Effect::DapChild { child }) = effects.last() else {
            panic!("no child opened");
        };
        let initialize = sent_on(&super::started(&mut state, child));
        let effects = super::received(
            &mut state,
            &json!({"type": "response", "request_seq": initialize[0].1["seq"], "success": false,
                "command": "initialize", "message": "no such target"})
            .to_string(),
            child,
        );
        assert_eq!(effects[0], Effect::StopDapChild { child });
        assert!(matches!(
            &effects[1],
            Effect::NotifyAbout {
                slug: "launch-failed",
                ..
            }
        ));
        assert!(matches!(
            state.debug.as_ref().expect("a session").phase,
            Phase::Paused(_)
        ));
        assert_eq!(state.refusal, None);
    }

    #[test]
    fn a_child_whose_program_ends_is_let_go_alone() {
        let (mut state, child) = with_child();
        let asked = sent_on(&super::received(
            &mut state,
            r#"{"type":"event","event":"terminated"}"#,
            child,
        ));
        assert_eq!(asked.len(), 1);
        assert_eq!(
            (asked[0].0, &asked[0].1["command"]),
            (child, &json!("disconnect"))
        );
        let effects = super::received(
            &mut state,
            &json!({"type": "response", "request_seq": asked[0].1["seq"], "success": true,
                "command": "disconnect"})
            .to_string(),
            child,
        );
        assert_eq!(effects, [Effect::StopDapChild { child }]);
        assert!(state.debug.is_some());
        assert!(!frame_rows(&state)
            .iter()
            .any(|row| matches!(row, FrameRow::Thread { child: Some(_), .. })));
    }

    #[test]
    fn continuing_the_sessions_threads_leaves_a_childs_paused() {
        let (mut state, _) = with_child();
        let asked = sent_on(&resume(&mut state));
        assert_eq!(asked[0].0, 0);
        super::received(
            &mut state,
            &json!({"type": "response", "request_seq": asked[0].1["seq"], "success": true,
                "command": "continue", "body": {"allThreadsContinued": true}})
            .to_string(),
            0,
        );
        assert!(frame_rows(&state).iter().any(|row| matches!(
            row,
            FrameRow::Thread {
                child: Some(_),
                paused: true,
                ..
            }
        )));
    }

    #[test]
    fn a_pause_outlives_one_connection_failing_to_name_its_threads() {
        let (mut state, child) = with_child();
        resume(&mut state);
        let asked = sent_on(&resume(&mut state));
        let seq = |to: usize| {
            let (_, message) = asked
                .iter()
                .find(|(on, _)| *on == to)
                .expect("threads asked");
            message["seq"].clone()
        };
        super::received(
            &mut state,
            &json!({"type": "response", "request_seq": seq(0), "success": false, "command": "threads"})
                .to_string(),
            0,
        );
        let paused = sent_on(&super::received(
            &mut state,
            &json!({"type": "response", "request_seq": seq(child), "success": true,
                "command": "threads", "body": {"threads": [{"id": 1, "name": "worker"}]}})
            .to_string(),
            child,
        ));
        assert_eq!(paused.len(), 1);
        assert_eq!(
            (paused[0].0, &paused[0].1["command"]),
            (child, &json!("pause"))
        );
    }

    #[test]
    fn a_child_the_session_never_opened_is_not_heard() {
        let mut state = paused(State::default());
        let before = frame_rows(&state);
        let effects = super::received(
            &mut state,
            r#"{"type":"event","event":"stopped","body":{"threadId":1,"reason":"breakpoint"}}"#,
            7,
        );
        assert_eq!(effects, []);
        assert_eq!(frame_rows(&state), before);
    }

    #[test]
    fn a_breakpoint_changed_mid_session_reaches_every_child() {
        let (mut state, child) = with_child();
        let asked = sent_on(&breakpoints_changed(&mut state, Path::new("/w/one.rs")));
        let to: Vec<usize> = asked.iter().map(|(to, _)| *to).collect();
        assert_eq!(to, [0, child]);
    }

    #[test]
    fn a_request_from_the_adapter_is_refused_rather_than_ignored() {
        let mut state = paused(State::default());
        let effects = received(
            &mut state,
            r#"{"seq":40,"type":"request","command":"runInTerminal","arguments":{}}"#,
        );
        let reply = &sent(&effects)[0];
        assert_eq!(reply["type"], "response");
        assert_eq!(reply["request_seq"], 40);
        assert_eq!(reply["success"], false);
        assert_eq!(reply["command"], "runInTerminal");
    }

    #[test]
    fn a_response_to_no_request_changes_nothing() {
        let mut state = paused(State::default());
        let before = state.clone();
        let effects = received(
            &mut state,
            r#"{"type":"response","request_seq":999,"success":false,"command":"launch"}"#,
        );
        assert_eq!(effects, vec![]);
        assert_eq!(state, before);
    }

    #[test]
    fn a_refused_launch_says_why_without_the_escapes() {
        let mut state = State::default();
        state.adapters.insert(
            "rust".to_string(),
            crate::startup::Adapter {
                command: "adapter".to_string(),
                args: Vec::new(),
                install: BTreeMap::new(),
                server: None,
                plugin: None,
                hot_replace: None,
            },
        );
        state.launches.insert(
            "app".to_string(),
            crate::startup::Launch {
                adapter: "rust".to_string(),
                request: "launch".to_string(),
                args: serde_json::Map::new(),
                reattach: true,
            },
        );
        start(&mut state, "app");
        started(&mut state);
        received(
            &mut state,
            r#"{"type":"response","request_seq":1,"success":true,"command":"initialize"}"#,
        );
        let effects = received(
            &mut state,
            "{\"type\":\"response\",\"request_seq\":2,\"success\":false,\"command\":\"launch\",\"message\":\"no \\u001b[2Jprogram\"}",
        );
        assert_eq!(
            effects,
            vec![
                Effect::notify_about("launch-failed", "no [2Jprogram".to_string()),
                Effect::StopDap
            ]
        );
        assert_eq!(state.debug, None);
        assert_eq!(
            state.refusal,
            Some(Refusal::LaunchFailed("no [2Jprogram".to_string()))
        );
    }

    #[test]
    fn pausing_a_program_that_never_paused_asks_for_its_threads_first() {
        let mut state = paused(State::default());
        resume(&mut state);
        let asked = sent(&resume(&mut state));
        assert_eq!(asked[0]["command"], "threads");
        assert_eq!(resume(&mut state), vec![]);
        let seq = asked[0]["seq"].clone();
        let effects = received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true, "command": "threads",
                "body": {"threads": [{"id": 7, "name": "worker"}]}})
            .to_string(),
        );
        let pause = &sent(&effects)[0];
        assert_eq!(pause["command"], "pause");
        assert_eq!(pause["arguments"]["threadId"], 7);
    }

    #[test]
    fn a_second_stop_lets_the_adapter_go_at_once() {
        let mut state = paused(State::default());
        let asked = sent(&stop(&mut state));
        assert_eq!(asked[0]["command"], "disconnect");
        assert!(state.debug.is_some());
        assert_eq!(stop(&mut state), vec![Effect::StopDap]);
        assert_eq!(state.debug, None);
    }

    #[test]
    fn nothing_is_configured_before_the_program_is_launched() {
        let mut state = State::default();
        state.adapters.insert(
            "rust".to_string(),
            crate::startup::Adapter {
                command: "adapter".to_string(),
                args: Vec::new(),
                install: BTreeMap::new(),
                server: None,
                plugin: None,
                hot_replace: None,
            },
        );
        state.launches.insert(
            "app".to_string(),
            crate::startup::Launch {
                adapter: "rust".to_string(),
                request: "launch".to_string(),
                args: serde_json::Map::new(),
                reattach: true,
            },
        );
        start(&mut state, "app");
        started(&mut state);
        let effects = received(&mut state, r#"{"type":"event","event":"initialized"}"#);
        assert_eq!(effects, vec![]);
    }

    #[test]
    fn only_the_inspected_thread_or_a_running_program_moves_the_pause() {
        let mut state = paused(State::default());
        let other =
            r#"{"type":"event","event":"stopped","body":{"threadId":2,"reason":"breakpoint"}}"#;
        let asked = sent(&received(&mut state, other));
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0]["command"], "threads");
        assert_eq!(paused_line(&state).map(|(_, line, _)| line), Some(1));
        stop(&mut state);
        assert_eq!(received(&mut state, other), vec![]);
        assert_eq!(
            state.debug.map(|session| session.phase),
            Some(Phase::Stopping)
        );
    }

    fn two_paused() -> State {
        let mut state = paused(State::default());
        received(
            &mut state,
            r#"{"type":"event","event":"stopped","body":{"threadId":2,"reason":"exception","text":"boom"}}"#,
        );
        state
    }

    fn answer_stack(state: &mut State, frames: Value) {
        let seq = outstanding(state, "stackTrace");
        received(
            state,
            &json!({"type": "response", "request_seq": seq, "success": true,
                "command": "stackTrace", "body": {"stackFrames": frames}})
            .to_string(),
        );
    }

    #[test]
    fn library_runs_fold_except_the_one_holding_the_chosen_frame() {
        let mut state = paused(State {
            root: PathBuf::from("/w"),
            ..State::default()
        });
        received(
            &mut state,
            r#"{"type":"event","event":"stopped","body":{"threadId":1,"reason":"step"}}"#,
        );
        answer_stack(
            &mut state,
            json!([
                {"id": 1, "name": "panic", "line": 1, "source": {"path": "/rust/panic.rs"}},
                {"id": 2, "name": "mine", "line": 2, "source": {"path": "/w/one.rs"}},
                {"id": 3, "name": "shim", "line": 3, "source": {"path": "/w/shim.rs"}, "presentationHint": "subtle"},
                {"id": 4, "name": "start", "line": 0},
                {"id": 5, "name": "main", "line": 4, "source": {"path": "/w/main.rs"}},
            ]),
        );
        let rows = frame_rows(&state);
        assert_eq!(
            rows[1..],
            [
                FrameRow::Frame(0),
                FrameRow::Frame(1),
                FrameRow::Library { start: 2, count: 2 },
                FrameRow::Frame(4),
            ]
        );
        assert_eq!(state.frames_selection, 1);
    }

    #[test]
    fn a_continue_that_ran_every_thread_lets_the_others_go() {
        for (all, held) in [(json!(null), 0), (json!(true), 0), (json!(false), 1)] {
            let mut state = two_paused();
            resume(&mut state);
            let seq = outstanding(&state, "continue");
            received(
                &mut state,
                &json!({"type": "response", "request_seq": seq, "success": true,
                    "command": "continue", "body": {"allThreadsContinued": all}})
                .to_string(),
            );
            let session = state.debug.as_ref().expect("a session");
            assert_eq!(session.others.len(), held, "{all}");
        }
    }

    #[test]
    fn another_thread_continuing_leaves_the_inspected_one_paused() {
        let mut state = two_paused();
        received(
            &mut state,
            r#"{"type":"event","event":"continued","body":{"threadId":2}}"#,
        );
        assert!(!stale(&state));
        assert!(state.debug.as_ref().expect("a session").others.is_empty());
    }

    #[test]
    fn a_held_thread_stopping_again_is_inspected_and_no_longer_counted() {
        let mut state = two_paused();
        resume(&mut state);
        received(
            &mut state,
            r#"{"type":"event","event":"stopped","body":{"threadId":2,"reason":"step"}}"#,
        );
        let session = state.debug.as_ref().expect("a session");
        assert!(matches!(&session.phase, Phase::Paused(pause) if pause.thread == 2));
        assert!(session.others.is_empty());
    }

    #[test]
    fn the_next_thread_is_inspected_and_the_one_left_is_held() {
        let mut state = two_paused();
        let asked = sent(&next_thread(&mut state));
        assert_eq!(asked[0]["command"], "stackTrace");
        assert_eq!(asked[0]["arguments"]["threadId"], 2);
        let session = state.debug.as_ref().expect("a session");
        assert_eq!(session.previous, None);
        assert!(session.others.contains_key(&1));
        assert_eq!(state.transport_lit, Some(NEXT_THREAD));
        let Some(Phase::Paused(pause)) = state.debug.as_ref().map(|s| &s.phase) else {
            panic!("not paused");
        };
        assert_eq!(
            (pause.why, pause.exception.as_deref()),
            (Why::Exception, Some("boom"))
        );
        answer_stack(
            &mut state,
            json!([{"id": 9, "name": "work", "line": 7, "source": {"path": "/w/two.rs"}}]),
        );
        assert_eq!(frames(&state)[0].name, "work");
        let late = next_thread(&mut state);
        assert_eq!(sent(&late)[0]["arguments"]["threadId"], 1);
        next_thread(&mut state);
        let seq = outstanding(&state, "stackTrace");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true, "command": "stackTrace",
                "body": {"stackFrames": [{"id": 1, "name": "main", "line": 1}]}})
            .to_string(),
        );
        assert!(frames(&state).is_empty());
    }

    #[test]
    fn choosing_a_flagged_thread_jumps_to_it() {
        let mut state = two_paused();
        assert_eq!(choose(&mut state, 0), vec![]);
        let flagged = frame_rows(&state)
            .iter()
            .position(|row| matches!(row, FrameRow::Thread { paused: true, .. }))
            .expect("thread 2 is flagged");
        let asked = sent(&choose(&mut state, flagged));
        assert_eq!(asked[0]["arguments"]["threadId"], 2);
    }

    #[test]
    fn a_failed_threads_request_does_not_leave_pausing_stuck() {
        let mut state = paused(State::default());
        resume(&mut state);
        let seq = sent(&resume(&mut state))[0]["seq"].clone();
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": false, "command": "threads"})
                .to_string(),
        );
        assert_eq!(sent(&resume(&mut state))[0]["command"], "threads");
    }

    #[test]
    fn an_answer_is_drawn_unless_the_text_says_otherwise() {
        let file = PathBuf::from("/w/one.rs");
        let at = |line, stale| Breakpoint {
            file: file.clone(),
            line,
            text: String::new(),
            stale,
            properties: Default::default(),
        };
        let mut state = paused(State {
            breakpoints: vec![at(2, false), at(3, true), at(5, false)],
            ..State::default()
        });
        state.current_buffer = Some(file.clone());
        let seq = outstanding(&state, "setBreakpoints");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true,
            "command": "setBreakpoints", "body": {"breakpoints": [
                {"verified": true},
                {"verified": true, "line": 7},
            ]}})
            .to_string(),
        );
        assert_eq!(
            marks(&state),
            BTreeMap::from([(2, Mark::Plain), (3, Mark::Stale), (7, Mark::Plain)])
        );
    }

    #[test]
    fn an_answer_is_forgotten_once_its_file_s_breakpoints_change() {
        let file = PathBuf::from("/w/one.rs");
        let at = |line| Breakpoint {
            file: file.clone(),
            line,
            text: String::new(),
            stale: false,
            properties: Default::default(),
        };
        let mut state = paused(State {
            breakpoints: vec![at(2), at(5)],
            ..State::default()
        });
        state.current_buffer = Some(file.clone());
        state.buffers.insert(
            file.clone(),
            crate::editor::Buffer::open("a\nb\nc\nd\ne\nf", false, 4),
        );
        let seq = outstanding(&state, "setBreakpoints");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true,
            "command": "setBreakpoints", "body": {"breakpoints": [
                {"verified": false},
                {"verified": true, "line": 6},
            ]}})
            .to_string(),
        );
        state.pointed_at = crate::Pointed::Breakpoint(2);
        assert_eq!(marks(&state).get(&2), Some(&Mark::Unverified));
        assert_eq!(explained(&state), None);
        let (state, _) = crate::update(&state, crate::Event::ToggleBreakpoint(3));
        assert_eq!(
            marks(&state),
            BTreeMap::from([(2, Mark::Plain), (3, Mark::Plain), (5, Mark::Plain)])
        );
    }

    #[test]
    fn a_terminated_program_disconnects_before_the_adapter_goes() {
        let mut state = paused(State::default());
        let asked = sent(&received(
            &mut state,
            r#"{"type":"event","event":"terminated"}"#,
        ));
        assert_eq!(asked[0]["command"], "disconnect");
        let effects = received(
            &mut state,
            &json!({"type": "response", "request_seq": asked[0]["seq"], "success": true, "command": "disconnect"})
                .to_string(),
        );
        assert_eq!(effects, vec![Effect::StopDap]);
        assert_eq!(state.debug, None);
    }

    #[test]
    fn the_corner_at_rest_is_the_one_held_before_the_session() {
        let state = paused(State {
            corner: layout::Corner::Buffers,
            ..State::default()
        });
        assert_eq!(state.corner, layout::Corner::Frames);
        assert_eq!(resting_corner(&state), layout::Corner::Buffers);
    }

    #[test]
    fn a_reference_that_holds_itself_is_drawn_once() {
        let mut state = paused(State::default());
        let seq = outstanding(&state, "scopes");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true, "command": "scopes",
                "body": {"scopes": [{"name": "Locals", "variablesReference": 1}]}})
            .to_string(),
        );
        let seq = outstanding(&state, "variables");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true, "command": "variables",
                "body": {"variables": [{"name": "itself", "variablesReference": 1}]}})
            .to_string(),
        );
        let rows: Vec<String> = variables(&state).into_iter().map(|row| row.name).collect();
        assert_eq!(rows, ["Locals", "itself"]);
    }

    #[test]
    fn another_files_breakpoints_are_left_alone() {
        let mut breakpoints = vec![Breakpoint {
            file: PathBuf::from("/w/b.rs"),
            ..on(2, "b")
        }];
        follow(&mut breakpoints, Path::new("/w/a.rs"), "a\nb", "b");
        assert_eq!(breakpoints[0].line, 2);
    }

    #[test]
    fn only_the_names_the_grammar_left_plain_are_variables() {
        let tokens = crate::highlight::highlight("main.rs", "    let total = 0; // count");
        assert_eq!(mentioned(&tokens[0]), ["total"]);
        let tokens = crate::highlight::highlight("main.rs", "    let total = \"count\";");
        assert_eq!(mentioned(&tokens[0]), ["total"]);
    }

    #[test]
    fn the_call_opens_where_the_block_holding_the_paused_line_opens() {
        let source = "fn a() {\n    let x = 1;\n}\nfn b() {\n    let y = 2;\n}";
        assert_eq!(call_start(source, 5), 4);
    }

    #[test]
    fn a_value_is_cut_by_display_width_and_never_by_character_count() {
        assert_eq!(clipped("東京タワー", 5), "東京");
        assert_eq!(clipped("abc", 2), "ab");
    }

    #[test]
    fn a_call_in_a_language_without_braces_opens_the_same_way() {
        let source = "def main():\n    total = 0\n    print(total)";
        assert_eq!(call_start(source, 3), 1);
        assert_eq!(call_start("total = 0\nprint(total)", 2), 1);
    }

    #[test]
    fn only_the_answer_to_the_latest_asking_is_the_port() {
        let mut state = State::default();
        state.adapters.insert(
            "java".to_string(),
            crate::startup::Adapter {
                command: "start-debugging".to_string(),
                args: Vec::new(),
                install: BTreeMap::new(),
                server: Some("java".to_string()),
                plugin: None,
                hot_replace: None,
            },
        );
        state.launches.insert(
            "app".to_string(),
            crate::startup::Launch {
                adapter: "java".to_string(),
                request: "attach".to_string(),
                args: serde_json::Map::new(),
                reattach: true,
            },
        );
        state.lsp_running.insert("java".to_string());
        crate::lsp::sync(&mut state);
        crate::lsp::received(
            &mut state,
            "java",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#,
        );
        let asked = start(&mut state, "app");
        let [Effect::LspSend { json, .. }] = asked.as_slice() else {
            panic!("the command is put to the server");
        };
        let id = serde_json::from_str::<Value>(json).unwrap()["id"]
            .as_i64()
            .unwrap();
        let answer = |id: i64| json!({"jsonrpc": "2.0", "id": id, "result": 41234});
        assert_eq!(ported(&mut state, "java", id + 1, &answer(id + 1)), None);
        assert_eq!(ported(&mut state, "go", id, &answer(id)), None);
        assert_eq!(
            ported(&mut state, "java", id, &answer(id)),
            Some(vec![Effect::StartDap {
                command: "start-debugging".to_string(),
                args: Vec::new(),
                reach: Reach::Port(41234),
            }])
        );
        started(&mut state);
        assert_eq!(ported(&mut state, "java", id, &answer(id)), None);
    }

    fn started_with(adapter: &[&str], request: &str, args: Value) -> (State, Vec<Effect>) {
        let mut state = State::default();
        state.adapters.insert(
            "rust".to_string(),
            crate::startup::Adapter {
                command: "adapter".to_string(),
                args: adapter.iter().map(|arg| arg.to_string()).collect(),
                install: BTreeMap::new(),
                server: None,
                plugin: None,
                hot_replace: None,
            },
        );
        state.launches.insert(
            "app".to_string(),
            crate::startup::Launch {
                adapter: "rust".to_string(),
                request: request.to_string(),
                args: args.as_object().cloned().unwrap_or_default(),
                reattach: true,
            },
        );
        let effects = start(&mut state, "app");
        (state, effects)
    }

    #[test]
    fn a_row_naming_a_port_is_reached_as_a_server() {
        let reached = |args: &[&str]| match started_with(args, "launch", json!({})).1.as_slice() {
            [Effect::StartDap { reach, .. }] => *reach,
            other => panic!("{other:?}"),
        };
        assert_eq!(reached(&["--port", "${port}"]), Reach::Server);
        assert_eq!(reached(&["--listen=127.0.0.1:${port}"]), Reach::Server);
        assert_eq!(reached(&["--port", "5005"]), Reach::Stdio);
        assert_eq!(reached(&[]), Reach::Stdio);
        let args = ["--listen=127.0.0.1:${port}", "--port", "${port}", "--quiet"].map(String::from);
        assert_eq!(
            on_port(&args, 41234),
            ["--listen=127.0.0.1:41234", "--port", "41234", "--quiet"]
        );
    }

    #[test]
    fn an_attach_naming_no_port_ends_with_its_program() {
        let (mut state, _) = started_with(&[], "attach", json!({ "pid": 42 }));
        started(&mut state);
        let seq = outstanding(&state, "initialize");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true, "command": "initialize"})
                .to_string(),
        );
        received(&mut state, r#"{"type":"event","event":"terminated"}"#);
        assert_eq!(waiting_on(&state), None);
        assert_eq!(
            state.debug.map(|session| session.phase),
            Some(Phase::Stopping)
        );
    }

    #[test]
    fn a_snapshot_quotes_what_the_file_has_around_the_paused_line() {
        let mut state = paused(State {
            root: PathBuf::from("/w"),
            ..State::default()
        });
        let unread = snapshot(&state).expect("Paused");
        assert!(unread.contains("one.rs:1"), "{unread}");
        assert!(!unread.contains(" | "), "no source: {unread}");
        state.buffers.insert(
            PathBuf::from("/w/one.rs"),
            crate::editor::Buffer::open("first\nsecond\nthird\nfourth", false, 4),
        );
        let quoted = snapshot(&state).expect("Paused");
        let marked: Vec<&str> = quoted
            .lines()
            .filter(|line| line.contains('\u{2192}'))
            .collect();
        assert_eq!(marked.len(), 2, "{quoted}");
        assert!(marked[0].ends_with("first"), "{quoted}");
        assert!(marked[1].contains("main"), "the chosen Frame: {quoted}");
        assert!(quoted.contains("third"), "{quoted}");
        assert!(!quoted.contains("fourth"), "{quoted}");
    }

    #[test]
    fn the_code_the_debug_panes_colour_is_their_values_in_the_paused_language() {
        let mut state = paused(State::default());
        let value = "Order { name: \"Ann\", total: 42 }";
        state.watches = vec![
            Watch {
                expression: "order".to_string(),
                answer: Answer::Value(value.to_string()),
            },
            Watch {
                expression: "gone".to_string(),
                answer: Answer::Failed("not in scope".to_string()),
            },
        ];
        open_evaluator(&mut state, "order.total".to_string());
        run(&mut state);
        received(
            &mut state,
            r#"{"type":"event","event":"output","body":{"output":"printed 7\n"}}"#,
        );
        let seq = outstanding(&state, "evaluate");
        received(
            &mut state,
            &json!({"type": "response", "request_seq": seq, "success": true, "command": "evaluate",
                "body": {"result": "42", "variablesReference": 0}})
            .to_string(),
        );

        let code = code(&state);
        assert_eq!(paused_in(&state), "one.rs");
        assert!(code.contains(&value.to_string()), "{code:?}");
        assert!(code.contains(&"42".to_string()), "{code:?}");
        assert!(code.contains(&"order.total".to_string()), "{code:?}");
        for words in ["order", "gone", "not in scope", "printed 7"] {
            assert!(!code.contains(&words.to_string()), "{words:?} in {code:?}");
        }
        let kinds: Vec<_> = crate::highlight::highlight(&paused_in(&state), value)
            .into_iter()
            .flatten()
            .map(|token| token.kind)
            .collect();
        assert!(kinds.contains(&crate::highlight::Kind::String), "{kinds:?}");
        assert!(kinds.contains(&crate::highlight::Kind::Number), "{kinds:?}");
    }

    #[test]
    fn with_nothing_paused_the_code_is_plain() {
        let state = State {
            watches: vec![Watch {
                expression: "order".to_string(),
                answer: Answer::Value("\"Ann\" 42".to_string()),
            }],
            ..State::default()
        };
        assert_eq!(paused_in(&state), "");
        for text in code(&state) {
            assert!(crate::highlight::highlight(&paused_in(&state), &text)
                .into_iter()
                .flatten()
                .all(|token| token.kind == crate::highlight::Kind::Plain));
        }
        assert_eq!(code(&state), vec!["\"Ann\" 42".to_string()]);
    }
}
