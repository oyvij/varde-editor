//! The debugger's pure half. Breakpoints: core state that exists with or
//! without a Debug session, carried with their lines as a Buffer is edited.
//! And the Debug session: the Debug Adapter Protocol's requests built and its
//! replies and events read, as JSON the edge frames and moves without deciding
//! anything (`docs/adr/0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md`).
//!
//! By hand over `serde_json::Value` rather than a crate's types: `dap`, the one
//! maintained crate, is written for implementing an adapter, so its requests
//! only deserialize and its responses and events only serialize — the
//! opposite of what a client needs.

use crate::preview::Refusal;
use crate::{layout, Effect, Pane, Place, State};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// A line the program pauses at. `text` is what the line held, trimmed, so a
/// project that remembers it can tell at load whether the line still does —
/// and re-indenting a block does not make every Breakpoint in it Stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakpoint {
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
    /// Remembered against text its line no longer holds. It keeps the text it
    /// was remembered with and the line it was set on: never re-pointed at
    /// whatever moved into its place.
    pub stale: bool,
    pub properties: Properties,
}

/// What a Breakpoint does when it is hit, beyond pausing. The three texts are
/// the program's own language, handed to the adapter as written and never
/// read here; empty is unset, because that is what an empty field in the
/// Breakpoint box means.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Properties {
    pub condition: String,
    pub hit_count: String,
    pub log_message: String,
    pub suspend: Suspend,
}

/// Which threads a hit pauses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Suspend {
    #[default]
    Thread,
    All,
}

/// The Breakpoint box's rows, in the order Tab walks them.
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

/// How the gutter draws a Breakpoint.
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

/// Where the gutter draws a Breakpoint, how, and the adapter's reason for an
/// Unverified one. Stale wins over whatever the adapter said: it is a claim
/// about the text, which no answer changes. The adapter's word counts only
/// while there is a program for it to be about — a session stopping has none,
/// so a moved Breakpoint is back on its own line the moment the program ends
/// rather than when the adapter lets go.
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
    // A Logpoint with a condition is still a Logpoint: what it does when hit
    // is print, which is the thing worth seeing at a glance.
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

/// The Breakpoints of the buffer on screen, by line, as the gutter draws them.
pub fn marks(state: &State) -> BTreeMap<usize, Mark> {
    on_screen(state)
        .map(|(line, mark, _)| (line, mark))
        .collect()
}

/// The adapter's reason for not binding the Breakpoint the pointer rests on,
/// and the line it is drawn on. None where it gave no reason, which would be
/// an empty box.
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

/// An answer is about the list it was sent: once a file's Breakpoints change —
/// edited onto other lines, set, removed — what the adapter said of them is
/// keyed to lines that are no longer theirs, and would be read as another
/// Breakpoint's.
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

/// Every Breakpoint in the workspace as the Breakpoint list draws it: by path,
/// then line. Read by `ui` to draw the rows, by `mouse` to hit-test them and by
/// `update` to act on the one selected, so the three cannot disagree about
/// which Breakpoint a row is.
pub fn list(state: &crate::State) -> Vec<&Breakpoint> {
    let mut rows: Vec<&Breakpoint> = state.breakpoints.iter().collect();
    rows.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    rows
}

/// The row the keyboard is on in the Breakpoint list, if it names one.
pub fn selected(state: &crate::State) -> Option<&Breakpoint> {
    let at = state
        .breakpoints_selection
        .checked_sub(switches(state).len())?;
    list(state).get(at).copied()
}

/// Opens the Breakpoint box on the Breakpoint at `line` of `file`, drafted
/// from what it carries now; a line with no Breakpoint opens nothing.
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

/// The screen cell of the `✎` Chip on the cursor's line, while that line
/// holds a Breakpoint and the editor has the keyboard: hard against the text's
/// right edge, where the tree draws its focused row's icons. Read by `ui` to
/// draw it and by `mouse` to hit-test it, so the two cannot land a row apart.
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
    // The line it was set on rather than the one the adapter drew it at: the
    // box opens on the Breakpoint that line holds.
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

/// What a row of the Breakpoint box reads, for the keys to type onto and
/// `ui` to draw. The switch has no text of its own.
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
/// The row's own, not the Transport's: one asks the AI about the whole pause
/// and the other about one value, so they are two actions wearing one name.
pub const ROW_ASK_AI: &str = "debug-ask-ai-value";
pub const NEXT_THREAD: &str = "debug-next-thread";

/// What the focused row offers: editing the Breakpoint it names, and
/// removing it.
pub fn row_actions(state: &crate::State) -> Vec<&'static str> {
    match selected(state) {
        Some(_) => vec![EDIT, REMOVE],
        None => Vec::new(),
    }
}

/// The Chips on the Breakpoint list's top border. Clearing is dimmed with
/// nothing to clear, and never lit: once it has run there is nothing left for
/// it to say it did. Naming an exception class is dimmed wherever the adapter
/// has not said it can take one — dimmed, never hidden, so it is findable
/// before a session that could use it.
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

/// The Chips on the Variables' top border: every debug action, then the
/// Program output's. One control per action and never two — continue and
/// pause are one Chip named for what pressing it does, the way the Reading's
/// play is and for the same reason.
///
/// The Debug group's border carries the whole set while a session exists,
/// since that is when their keys are reserved. With the Shell group up — which
/// is where a session that ended leaves the Strip — one Chip is left: restart,
/// while there is a configuration to rerun. A control reachable only while
/// the thing it restarts is running is a control nobody can press, and the
/// keyboard's own `C-F5` has the same reach.
///
/// What the Transport holds, never where it is drawn: `crate::showing_transport`
/// is that, and `ui` and `mouse` read the one answer.
///
/// Glyphs are geometric and one cell wide in every font (ADR 0022): no emoji,
/// whose width terminals disagree about, and every column to the right of a
/// two-cell glyph is a click landing where nobody pointed.
pub fn strip_transport(state: &crate::State) -> Vec<crate::Chip> {
    use crate::{Chip, Hue, Tone};
    let chip = |action, name, glyph: &str, keys, hue, dimmed| Chip {
        action,
        name,
        glyph: glyph.to_string(),
        keys,
        hue,
        // Lit ahead of dimmed: a step taken leaves the program running, so the
        // Chip that was just pressed is dimmed the instant it acts and would
        // otherwise never be seen lit at all.
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
        // Nothing to continue or pause until the program is one of the two:
        // a session still spawning or already stopping answers neither.
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
            // Stepping is only ever asked of a stopped thread: an adapter sent
            // a step while the program runs answers with an error, so the
            // Chips say so rather than the reader finding out from the footer.
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
            // Dimmed while a session exists: restarting a live one is stop
            // and start again, which nothing specifies yet, so today it would
            // refuse with `debug-session-running` — and a Chip that refuses is
            // a Chip that lies. What it is for is the session that ended,
            // below.
            restart(true),
            chip(
                ASK_AI,
                "ask-ai",
                "\u{2736}",
                "\u{2423}a",
                Hue::Plain,
                stepping,
            ),
            // Counting the other Paused threads, dimmed at none. No key of
            // its own: Enter on a flagged thread in the Frames is the
            // keyboard's way there, and it reaches every one, not only the
            // next.
            chip(
                NEXT_THREAD,
                "next-thread",
                &format!("\u{21c9}{}", session.others.len()),
                "",
                Hue::Plain,
                session.others.is_empty(),
            ),
        ]);
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

/// What 1-based `line` of `text` holds, trimmed — the text a Breakpoint is
/// remembered against — or nothing for a line `text` does not have.
pub fn held(text: &str, line: usize) -> Option<&str> {
    Some(text.split('\n').nth(line.checked_sub(1)?)?.trim())
}

/// Carries `file`'s Breakpoints from `old` to `new`: down or up with the lines
/// inserted or deleted above them, off the list with their own line deleted.
/// A line edited in place keeps its Breakpoint, and one that holds its text
/// takes the line's new text with it.
///
/// Zero context, for the reason `format::spans` diffs with none: a hunk is then
/// exactly the lines that changed, so a line inside one was replaced — kept if
/// the replacement has a line in its place, gone if not — and a line past one
/// moves by what the hunk added less what it took.
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
    // A hunk that holds no lines on a side names the line it sits *after*
    // there, which is the one place a unified diff's arithmetic is not the
    // obvious one.
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

/// A Debug session, laid over Edit view. One field of `State`: a session
/// exists or it does not. Everything here is a claim about a conversation the
/// core is party to. Whether the adapter's process exists is the edge's to
/// say: a session asked for is `Spawning` until `Event::DapStarted`, and
/// gone at `Event::DapGone`, never assumed from the spawn having been asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// The adapter's row name, which `initialize` names it by, and the command
    /// that runs it, which a refusal names — both taken at the start, with the
    /// request below.
    adapter: String,
    command: String,
    /// How the session came to hold its adapter, done again when a Waiting
    /// session attaches anew.
    begin: Begin,
    pub phase: Phase,
    /// `launch` or `attach`, and what that request carries — taken when the
    /// session starts, so a config edited mid-session changes the next one.
    request: String,
    args: serde_json::Map<String, Value>,
    /// The host and port an attach session watches once its program has
    /// gone, and `None` for a session that ends with its program instead: a
    /// launch, an attach that opted out, and one that names no port, which
    /// would wait on nothing anybody could answer.
    watch: Option<(String, u16)>,
    /// The `seq` the last request went out with, and every one still
    /// unanswered, by `seq`: a response names its request only by number.
    seq: i64,
    asked: BTreeMap<i64, Ask>,
    /// What the adapter said of each Breakpoint, under the file and the line
    /// it was set on. Here rather than on the Breakpoint so it goes with the
    /// session and never reaches the line the user set or what is remembered.
    verdicts: BTreeMap<(PathBuf, usize), Verdict>,
    /// A pause asked for before any thread was known, so it waits on the
    /// `threads` answers that may name one: the requests it sent, one per
    /// connection, until one names a thread or every one has answered.
    pausing: BTreeSet<i64>,
    /// Every thread the adapter named, in its order, asked for again at every
    /// `stopped`: a worker started since the last pause is a thread nobody
    /// would otherwise find.
    threads: Vec<(i64, String)>,
    /// The Paused threads other than the one being inspected, and why each
    /// stopped — so the one jumped to shows its exception as it would have
    /// had it been first. The inspected thread is never in it: one thread in
    /// two places is a count that is off by one.
    others: BTreeMap<i64, (Why, Option<String>)>,
    /// What the locals held at the pause before this one, by name, under the
    /// name of the Frame they belong to — and `None` until a pause has been
    /// left behind. This is what an Inline value is marked as changed against,
    /// and the Frame's name travels with them because only the *inspected*
    /// Frame's scopes are ever fetched: without it, choosing an outer Frame
    /// would diff one call's locals against another's and mark names that
    /// never moved. On the Session rather than on the Pause, because the
    /// question is about the pause that is gone: a Pause carrying it would
    /// have to be handed its predecessor's copy to build itself.
    previous: Option<(String, BTreeMap<String, String>)>,
    /// Whether the adapter said it can write a member back. Its word, never
    /// a try: a set-value Chip that looked enabled and failed teaches the
    /// reader nothing, so the capability dims it instead.
    can_set: bool,
    /// Whether it said a request can be taken back, which is the only way a
    /// Snippet that has not returned can be stopped. Its word for the reason
    /// `can_set` is its word: a cancel Chip that looked enabled and did
    /// nothing is worse than one that says it cannot.
    can_cancel: bool,
    /// The Exception filters the adapter reported, in its order — the only
    /// ones a switch is drawn for or an id is sent under — and whether it
    /// said one exception class can be named, and the one that was.
    filters: Vec<Filter>,
    can_name_class: bool,
    class: Option<String>,
    /// What the Corner and the Strip held when the session began, given back
    /// when it ends.
    corner: layout::Corner,
    strip: layout::Group,
    /// The child sessions the adapter asked Varde to start, by the connection
    /// each is held on. Never a session of their own: their threads are this
    /// one's, and stopping it stops them.
    children: BTreeMap<usize, Child>,
}

/// A child session: what the adapter's `startDebugging` asked to be started,
/// and the name its threads are grouped under.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Child {
    name: String,
    request: String,
    configuration: Value,
}

/// A thread as the session numbers it: the connection it lives on above the
/// id the adapter gave it, since every child session numbers its threads from
/// the start. The session's own connection is 0, so its threads keep the
/// adapter's ids.
fn thread_on(link: usize, id: i64) -> i64 {
    ((link as i64) << 32) | (id & 0xFFFF_FFFF)
}

/// The connection a thread lives on and the id the adapter knows it by.
fn link_of(thread: i64) -> (usize, i64) {
    ((thread >> 32) as usize, thread & 0xFFFF_FFFF)
}

/// An Exception filter as the adapter listed it: the id it is switched by and
/// what it is called on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub id: String,
    pub label: String,
}

/// The adapter's list of Exception filters, or `None` where the message
/// carries none — a `capabilities` event that says nothing about them leaves
/// the ones already reported alone.
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

/// A request waiting for its answer: the command, which is how the response is
/// read, and the arguments it went out with. The whole arguments and not the
/// one field each arm wants, because a response carries almost nothing of its
/// question — a `variables` answer is a list of members belonging to nothing,
/// an `evaluate` answer is a string belonging to no expression, and a
/// `setVariable` answer names neither the member it wrote nor what held it.
/// One field per arm is three fields that travel together and a fourth on the
/// next command; the request already says all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ask {
    command: String,
    arguments: Value,
    to: usize,
}

/// Where a session has got to. An enum rather than flags, for the reason
/// `Modal` is one: Running and Paused at once has no answer for F9.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Asked of the edge, which has not yet said it holds an adapter.
    Spawning,
    /// `initialize` sent, and nothing else until it is answered.
    Initializing,
    /// Launched or attached, waiting for the adapter's `initialized` before
    /// the Breakpoints go: one that arrives any earlier would send them ahead
    /// of the program they are for.
    Starting,
    /// Running, holding what the last pause showed so it can stay on screen,
    /// dimmed, while the program runs — `None` until the first pause. The
    /// pause has one home either way: a second field holding it beside the
    /// phase is two authors for one fact.
    Running(Option<Pause>),
    Paused(Pause),
    /// `disconnect` sent, waiting for its answer before the adapter is let go:
    /// dropped at once, it could take a launched program down with it before
    /// it had heard it was being stopped. A second stop does not wait.
    Stopping,
    /// An attach session whose program went away, holding no adapter, until
    /// the edge reports its port answers. Only stopping ends it.
    Waiting,
}

/// What a server row's arguments name where the port goes.
const PORT: &str = "${port}";

/// A server row's arguments with the port the edge found free filled in.
pub fn on_port(args: &[String], port: u16) -> Vec<String> {
    args.iter()
        .map(|arg| arg.replace(PORT, &port.to_string()))
        .collect()
}

/// How the edge reaches a Debug adapter, which its row says as data (ADR
/// 0021): over its standard streams, over TCP on a port the edge fills in for
/// a row whose arguments name `${port}`, or over TCP on the port the language
/// server hosting it answered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Stdio,
    Server,
    Port(u16),
}

/// How a session comes to hold its adapter: a spawn asked of the edge, or the
/// row's command put to the language server that hosts the adapter, under the
/// id of the latest asking, whose answer is the port.
#[derive(Debug, Clone, PartialEq)]
enum Begin {
    Spawn(Effect),
    Ask { server: String, id: i64 },
}

/// One thread stopped, the call stack it stopped in, and which Frame is being
/// inspected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pause {
    pub thread: i64,
    pub why: Why,
    /// What the adapter said paused it, where it said anything: the first
    /// Variables row, since an exception is what the reader is looking for.
    exception: Option<String>,
    /// Empty until the adapter answers `stackTrace`, which the pause asks for.
    pub frames: Vec<Frame>,
    pub chosen: usize,
    /// The chosen Frame's scopes, as the adapter named them, and the children
    /// fetched for every reference that has been opened. A member is asked
    /// for when it is opened and never before: a tree walked whole at every
    /// pause is a debugger that stops for seconds on a deep structure.
    scopes: Vec<Member>,
    children: BTreeMap<i64, Vec<Member>>,
    open: BTreeSet<i64>,
    /// How many children of a reference have arrived, which is where its next
    /// page starts.
    fetched: BTreeMap<i64, usize>,
    /// The runs of Library frames the reader unfolded, by the index of the
    /// run's first Frame. Per pause, since the next stack is another stack.
    unfolded: BTreeSet<usize>,
}

/// One member of the Variables tree as the adapter named it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Member {
    name: String,
    value: String,
    /// What to ask for this member's children by; 0 for one that has none.
    reference: i64,
    hint: Hint,
    /// How many indexed children the adapter says it has, which is what
    /// decides whether it is read a page at a time. 0 for a member the
    /// adapter did not count, which is every member small enough not to need
    /// counting.
    indexed: usize,
}

/// How a member is drawn, as the adapter's presentation hints say — never as
/// Varde guesses from the language, which it does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Plain,
    Private,
    ReadOnly,
    /// A member the adapter will only compute when it is asked for, which is
    /// what opening it does.
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

/// One row of the Variables as it is drawn: the tree flattened to what is
/// open, which is what `ui` draws, what the mouse hit-tests and what Enter
/// acts on — the three reading one list, for the reason the Breakpoint list's
/// rows are one list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub value: String,
    pub depth: usize,
    pub hint: Hint,
    pub open: bool,
    pub opens: Opens,
    /// The path to this row in the program's own language — `orders[0].id`
    /// rather than `id` — which is what copying it as an expression puts on
    /// the clipboard and what watching it adds. Built as the tree is
    /// flattened, because only the walk knows what stands above a row; empty
    /// for a scope, which is a heading and not an expression.
    pub expression: String,
    /// The reference of whatever holds this row, which is how `setVariable`
    /// names a member: the protocol asks for the container and the member's
    /// name, never for the member's own reference.
    pub parent: i64,
    pub of: Of,
}

/// What a row stands for, which is what its Chips act on: a member is
/// watched where a Watch is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Of {
    /// A member of the adapter's tree, or the exception that paused the
    /// program: both are the adapter's word about the program.
    Member,
    /// A Watch, by its place in the Watches — and whether its expression
    /// calls something, since a Watch runs that call again at every pause,
    /// and whether the last pause could not evaluate it.
    Watch {
        index: usize,
        calling: bool,
        failed: bool,
    },
}

/// What opening a row asks the adapter for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    /// A member with no children to ask for.
    Nothing,
    Children {
        reference: i64,
        indexed: usize,
    },
    /// The row that stands for the rest of a collection too big to have come
    /// whole, and the index it carries on from.
    NextPage {
        reference: i64,
        start: usize,
    },
}

/// How many members of an indexed collection are asked for at a time. A
/// hundred is more rows than any pane shows and few enough that an adapter
/// answers at once; the alternative — asking for all of them — is a ten
/// thousand element vector serialized into a pane twenty rows tall.
const PAGE: usize = 100;

/// One expression kept at the top of the Variables and re-evaluated at every
/// pause. Core state rather than the session's: a Watch is a question the
/// reader is asking of the program, and the next session is asked the same
/// one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Watch {
    pub expression: String,
    pub answer: Answer,
}

/// What the last pause's `evaluate` said about a Watch. The adapter's reason
/// is kept apart from a value because a reason drawn as a value reads as the
/// program's own answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Asked and not yet answered, which is also where a Watch starts.
    Waiting,
    Value(String),
    Failed(String),
}

/// Why the program paused, as far as the Paused line is drawn.
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

/// How far one step goes: over a call, into it, or out of the one being
/// inspected. Named for the gesture rather than for the protocol, because it is
/// what a key, a Chip and a cheatsheet row all say; `step` below is the one
/// place the protocol's spelling for each of them lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Over,
    Into,
    Out,
}

/// One call on the stack. `file` is absent for a Frame with no source the
/// adapter can name — a call inside a library shipped without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The adapter's number for it, which is the only way to ask for its
    /// scopes: a Frame is named to the reader and numbered to the adapter.
    pub id: i64,
    pub name: String,
    pub file: Option<PathBuf>,
    pub line: usize,
    /// Outside the workspace, or hinted by the adapter as not worth showing.
    pub library: bool,
}

/// Why the edge stopped holding an adapter. Missing is its own case because it
/// is the one the reader fixes by installing something, so it is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    Missing,
    FailedToStart,
    Exited,
}

/// The names the launch list offers, in the order it draws them.
pub fn launches(state: &State) -> Vec<&str> {
    state.launches.keys().map(String::as_str).collect()
}

/// Starts the Launch configuration `name`.
pub fn start(next: &mut State, name: &str) -> Vec<Effect> {
    match next.launches.get(name).cloned() {
        Some(launch) => launch_with(next, launch),
        None => Vec::new(),
    }
}

/// Starts a session from `launch`: refused by name if it names no configured
/// adapter, and otherwise a spawn asked of the edge, whose answer is the
/// session's first fact.
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
    // Remembered before the session exists and kept after it ends: what
    // restart reruns is the configuration, not the session.
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
        corner: next.corner,
        strip: next.strip,
    });
    // After the session has remembered the group it found, which is what
    // ending it gives back: the Program output lives in the Debug group, and a
    // program that never pauses would otherwise print out of sight.
    next.strip = layout::Group::Debug;
    vec![effect]
}

/// The host and port a Waiting session is watching, which the edge probes
/// until it answers — and nothing while no session is Waiting.
pub fn waiting_on(state: &State) -> Option<(&str, u16)> {
    let session = state.debug.as_ref()?;
    let (host, port) = session.watch.as_ref()?;
    (session.phase == Phase::Waiting).then_some((host.as_str(), *port))
}

/// The port a Waiting session watches answers, so the adapter is spawned
/// again and the whole handshake runs anew — Breakpoints included, since the
/// program that answered holds none of them. Nothing of the conversation that
/// ended carries over but what the session was started with.
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
            // The server went away while the session waited, so there is
            // nobody to ask for the adapter.
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
        ..session
    });
    next.strip = layout::Group::Debug;
    vec![effect]
}

/// The answer of the language server hosting the adapter to the command that
/// starts it: the port to reach the adapter on, or the server's refusal, which
/// ends the session in its words. `None` for every other reply it sends.
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
        // Ended rather than sent back to Waiting as a refused attach is: the
        // program is not party to this, so there is nothing on its way up to
        // wait for, and a server that cannot host the adapter — its plugin
        // not loaded — says the same thing every time it is asked.
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

/// The language server `server` went away. A session still asking it for its
/// adapter would wait on an answer nothing is left to send, so it ends.
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

/// The edge holds the adapter now, so the conversation begins.
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

/// The edge stopped holding the adapter. Whatever the session was waiting on
/// went with it; a session that was being stopped has already ended.
pub fn gone(next: &mut State, why: Gone, from: usize) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    // A child session going is its threads going, never the session: the
    // program it belonged to is still the one being debugged.
    if from != 0 {
        forget(session, from);
        return Vec::new();
    }
    let command = session.command.clone();
    // Said twice: the refusal answers the event in the footer, and the notice
    // stays in the status line, because this arrives when the edge notices
    // rather than when anybody pressed anything, and the next event of any
    // kind takes a refusal down.
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

/// One message the adapter sent, exactly as the edge read it.
pub fn received(next: &mut State, json: &str, from: usize) -> Vec<Effect> {
    let Ok(mut message) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    // A child this session never opened, or has forgotten, speaks for nothing
    // on screen: its threads would be a session nobody started.
    let Some(session) = next.debug.as_ref() else {
        return Vec::new();
    };
    if from != 0 && !session.children.contains_key(&from) {
        return Vec::new();
    }
    // Every thread the message names, renumbered onto its connection here, so
    // nothing past this line can mistake one child's thread 1 for another's.
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
            // The adapter asking for a terminal to run the debugged program
            // in. It gets the Debug group's own, never a shell: the Strip's
            // shells are the reader's, and a program started in one would
            // print over whatever was running there and end with the next
            // `:split`.
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
                // The adapter's, which is untrusted input, so it is never
                // interpolated into a shell command: the argv goes to the pty
                // as it stands and the environment is a map of names to
                // values, both handed to the edge rather than spelled out as
                // a line something else would parse.
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
            // A child session the adapter wants started, for a worker or a
            // process the program spawned: another connection to the same
            // adapter, folded into this session rather than offered as one to
            // pick, since which sessions an adapter keeps is its bookkeeping.
            if command == "startDebugging" {
                let arguments = &message["arguments"];
                // Numbered off `seq`, which never goes back, so a child that
                // went is never confused with one opened after it.
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
            // A reverse request nothing here answers is refused out loud: an
            // adapter left waiting on a reply is a session that hangs.
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
            // A child session that would not start is one fewer set of
            // threads, said in the status line, and the session goes on.
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
                // A session that watches a port goes back to watching it:
                // a program on its way up can answer before it will take a
                // debugger, and only stopping ends a Waiting session.
                let mut effects = match session.watch {
                    Some(_) => {
                        session.phase = Phase::Waiting;
                        Vec::new()
                    }
                    None => end(next),
                };
                next.refusal = Some(Refusal::LaunchFailed(why.clone()));
                // And in the status line, for the reason `gone` says it twice.
                effects.extend([Effect::notify_about("launch-failed", why), Effect::StopDap]);
                effects
            }
            // The answer that lets the adapter go, whatever it says.
            "disconnect" => let_go(next),
            // A pause that could not learn a thread is not still waiting on
            // one, or F9 would never pause again.
            "threads" => {
                session.pausing.remove(&answering);
                Vec::new()
            }
            // The adapter's reason, kept against the Watch that asked: a
            // Watch that silently showed nothing is a Watch the reader reads
            // as false rather than as unanswerable.
            "evaluate" => {
                let why = adapter_error(message, &command);
                // The Evaluator first, and told apart by the context the
                // request went out in for the reason a Hover is: the reader
                // asked out loud, so the adapter's words are shown as they
                // came and never softened into a blank output.
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
            // Said out loud, for the reason a launch that failed is: a value
            // the program would not take and nothing on screen to say so is
            // a set that looks as though it worked.
            "setVariable" => {
                next.refusal = Some(Refusal::SetValueFailed(adapter_error(message, &command)));
                Vec::new()
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
                        // A Frame with no source is outside the workspace
                        // too: there is nothing of the reader's to open.
                        library: file.as_ref().is_none_or(|file| !file.starts_with(root))
                            || frame["presentationHint"] == "subtle"
                            || frame["source"]["presentationHint"] == "deemphasize",
                        file,
                        line: frame["line"].as_u64().unwrap_or_default() as usize,
                    }
                })
                .collect();
            // Filed only against the thread it was asked about: a jump to
            // another thread while one was in flight would otherwise show
            // this thread's calls under that one's name.
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
        // One level of one reference. Appended, never replacing: the only
        // second request for a reference is its next page, and a page that
        // overwrote the one before it is a collection that never grows.
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
        // The chosen Frame's scopes become the top rows, each open unless the
        // adapter called it expensive — a scope it says costs something to
        // read is one nobody asked to read.
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
        // A Watch's value, filed under the expression that asked for it — or
        // the Hover's, told apart by the context the request went out in and
        // never by the expression, since a Watch on what the pointer is
        // resting on is one expression with two places to be.
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
        // The adapter's new value replaces the row's, rather than Varde
        // assuming what it wrote: an adapter is free to coerce what it was
        // given, and the row has to show what the program now holds.
        // Filed against the member the request named, in the container it
        // named: matching on the name alone rewrites every `id` in the tree,
        // and reading the selection back would file the answer wherever the
        // keyboard has got to since.
        "setVariable" => {
            let value = printable(message["body"]["value"].as_str().unwrap_or_default());
            let reference = arguments["variablesReference"].as_i64().unwrap_or_default();
            let name = arguments["name"].as_str().unwrap_or_default();
            let Some(Phase::Paused(pause)) = next.debug.as_mut().map(|s| &mut s.phase) else {
                return Vec::new();
            };
            let held = match pause.children.get_mut(&reference) {
                Some(held) => held,
                // A member of a scope rather than of an opened row: the
                // scopes are the one list that is not under a reference.
                None => &mut pause.scopes,
            };
            if let Some(member) = held.iter_mut().find(|member| member.name == name) {
                member.value = value;
            }
            Vec::new()
        }
        // The file's whole list, in the order it was asked, naming neither the
        // file nor the lines it answers about: the request does.
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
        // One connection's threads, in place of the ones it named before: the
        // session's own first, then each child's in the order it started.
        "threads" => {
            session.threads.retain(|(id, _)| link_of(*id).0 != to);
            let named = message["body"]["threads"].as_array().into_iter().flatten();
            session.threads.extend(named.filter_map(|thread| {
                let name = printable(thread["name"].as_str().unwrap_or_default());
                Some((thread["id"].as_i64()?, name))
            }));
            session.threads.sort_by_key(|(id, _)| link_of(*id).0);
            // Every connection is asked, and one with no threads of its own —
            // an adapter's parent session often has none — does not end the
            // pause while another may yet name one.
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
        // The protocol's default is that every thread ran, whatever was
        // asked: only an adapter that says otherwise leaves the others Paused.
        "continue" => {
            if message["body"]["allThreadsContinued"] != Value::Bool(false) {
                session.others.retain(|id, _| link_of(*id).0 != to);
            }
            Vec::new()
        }
        "disconnect" => let_go(next),
        _ => Vec::new(),
    }
}

/// `disconnect` answered: the adapter goes, and the session with it unless it
/// is Waiting for its program to come back.
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
            // Only a running program, or the thread being inspected, moves the
            // inspection: a second thread pausing leaves the view where it is
            // and is counted instead, and a session being stopped is not
            // brought back.
            match &session.phase {
                Phase::Running(_) => {}
                Phase::Paused(pause) if pause.thread == thread => {}
                Phase::Paused(_) => {
                    session.others.insert(thread, (why, exception));
                    return ask_all(session, "threads", json!({}));
                }
                _ => return Vec::new(),
            }
            // What the pause now ending held, kept for the one pause that
            // follows it: taken here, where the old Pause is still whole, and
            // never from the new one, which has no values yet.
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
            // The Debug group and the Frames come forward; the keyboard stays
            // where it was, since a pause is something the program did, not
            // the reader. What the reader shows instead is not moved again
            // until the next pause.
            next.corner = layout::Corner::Frames;
            next.strip = layout::Group::Debug;
            effects
        }
        // An adapter may learn what it can do after `initialize` answered —
        // a language plugin loading, a program attached to — and says so with
        // this event. Read into the same field the initialize reply sets, so
        // the set-value Chip has one author.
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
        // What the program printed, as the adapter repeats it. It goes to the
        // Evaluator while a Snippet is in flight, because output during a run
        // is that run's: the Program output is the pty's own and keeps its
        // copy either way, so this takes nothing away from it.
        Some("output") => {
            let text = body["output"].as_str().unwrap_or_default();
            if let Some(ran) = in_flight(next) {
                ran.printed.push(printable(text.trim_end_matches('\n')));
            }
            Vec::new()
        }
        // Another thread running on is one fewer held, and only the one
        // being inspected running on dims the view.
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
        // The program is gone, so the adapter is told the session is over
        // and let go once it has answered, as a stop is: it may have more to
        // clean up than the program.
        // An attach session that watches a port waits for it instead, and
        // detaches rather than ending a program that may only be restarting.
        Some("terminated") => {
            let session = next.debug.as_mut().expect("a session");
            // A child session's program ending is its threads going; the
            // session's own is what ends the session.
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
            // Its letters step nothing while nothing is attached.
            next.stepping = false;
            effects
        }
        _ => Vec::new(),
    }
}

/// `initialized`: the Breakpoints, the Exception filters and then
/// `configurationDone`, in the protocol's order and in one batch — to the
/// connection that said it, since a child session is configured as its own.
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

/// What `setExceptionBreakpoints` carries: every switch that is on, and the
/// one named class where there is one. Whole every time, since the request
/// replaces what the adapter held rather than adding to it.
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

/// The switches at the top of the Breakpoint list: each filter the adapter
/// reported, and whether this project has it on for this adapter. None
/// without a session, since the filters are the adapter's and never Varde's.
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

/// Every row of the Breakpoint list: the switches, then the Breakpoints.
pub fn rows(state: &State) -> usize {
    switches(state).len() + state.breakpoints.len()
}

/// Enter on the switch at `at`: flipped, remembered, and sent at once.
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

/// The exception-class box's Enter: the class is sent with the switches, and
/// kept for the session so switching a filter later does not drop it.
pub fn name_class(next: &mut State, class: String) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    session.class = Some(class);
    let arguments = pause_on(next);
    let session = next.debug.as_mut().expect("a session");
    ask_all(session, "setExceptionBreakpoints", arguments)
}

/// A file's whole list, as `setBreakpoints` replaces it: every Breakpoint in it
/// but the Stale ones, whose line no longer holds what was set there. A file
/// left with none is sent an empty list, or the adapter would keep the last.
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

/// A Breakpoint set or removed in `file` reaches a configured session at once.
/// Before `initialized` it waits for [`configured`], which sends every file.
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

/// F9: continue the inspected thread while Paused, pause the program while
/// Running. The phase moves as the request goes, since a `continue` answered
/// is the adapter's word that it ran and a `continued` event is optional.
pub fn resume(next: &mut State) -> Vec<Effect> {
    let Some(session) = next.debug.as_mut() else {
        return Vec::new();
    };
    let effects = match &session.phase {
        Phase::Paused(pause) => {
            let (thread, last) = (pause.thread, pause.clone());
            session.phase = Phase::Running(Some(last));
            // Only this thread: the others are Paused requests the reader is
            // holding open, and it is theirs to let go of one at a time.
            vec![ask(
                session,
                "continue",
                json!({ "threadId": thread, "singleThread": true }),
            )]
        }
        // Pausing needs a thread, and a program that has never paused has
        // named none, so the adapter is asked for them first.
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

/// The Chip the action just taken belongs to, lit until another is taken —
/// but only where it acted: a Chip lit for a request that never went out says
/// something happened.
fn lit(next: &mut State, action: &'static str, effects: &[Effect]) {
    if !effects.is_empty() {
        next.transport_lit = Some(action);
    }
}

/// F8, F7 and Shift+F8, and the `n`, `i` and `o` chords: the inspected thread
/// runs on by one step. Only while Paused — a program that is running is
/// already between steps, and an adapter asked to step one that is not stopped
/// answers with an error.
///
/// The phase moves as the request goes, for `resume`'s reason: the program is
/// running until the `stopped` event says where it got to.
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

/// Ctrl+F2: a launched program is terminated and an attached one left
/// running. A session already stopping is let go at once.
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

/// Ctrl+F5, the `r` chord and the restart Chip: the last Launch configuration
/// started again. Refused by name with none — a key that quietly did nothing
/// would read as a key that failed.
pub fn restart(next: &mut State) -> Vec<Effect> {
    let Some(launch) = next.last_launch.clone() else {
        next.refusal = Some(Refusal::NoLastSession);
        return Vec::new();
    };
    let effects = launch_with(next, launch);
    lit(next, RESTART, &effects);
    effects
}

/// Enter or a click on the Frames' `row`: a Frame becomes the inspected one
/// and the Paused line moves to its call, a folded run unfolds, and a thread
/// flagged as Paused is jumped to — the keyboard's way to the next-thread
/// Chip's reach, and to a thread other than the next.
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

/// The next-thread Chip: the Paused thread after the inspected one, by the
/// adapter's number, round to the first.
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

/// `thread`, held Paused elsewhere, becomes the inspected one, and the one it
/// replaces is held in its place — it is still Paused, only no longer looked
/// at. From a running program too, since a thread held open does not stop
/// being there because the inspected one ran on.
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
    // Another thread's locals are not this one's earlier values: marking what
    // changed against them marks names that never moved.
    session.previous = None;
    inspect_thread(session, thread, why, exception)
}

/// `thread` becomes the one inspected, Paused as it stopped: its stack asked
/// for, and the threads again, since the one that stopped may be new.
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

/// What is on screen from the last pause: the pause itself while one holds,
/// and the one it left behind while the program runs — the Frames and the
/// Variables stay drawn, dimmed, so a program that runs on does not blank the
/// panes the reader was reading.
fn showing(state: &State) -> Option<&Pause> {
    match state.debug.as_ref().map(|session| &session.phase) {
        Some(Phase::Paused(pause)) => Some(pause),
        Some(Phase::Running(last)) => last.as_ref(),
        _ => None,
    }
}

/// Whether what is on screen is the last pause rather than this one, which is
/// what draws it dimmed: nothing dimmed is mistaken for current.
pub fn stale(state: &State) -> bool {
    matches!(
        state.debug.as_ref().map(|session| &session.phase),
        Some(Phase::Running(Some(_)))
    )
}

/// What the Variables' title says: the mode the keyboard is in while Stepping
/// mode is on — it is four letters acting without their Space, and a mode
/// nobody can see they are in is a mode that swallows keys — then that the
/// program is running, and otherwise the pane's own name.
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

/// The Frames of the thread being inspected, empty while no pause has
/// anything to show.
pub fn frames(state: &State) -> &[Frame] {
    showing(state).map_or(&[], |pause| &pause.frames)
}

/// One row of the Frames as the Corner draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameRow {
    /// A thread, heading its Frames. `paused` flags a thread Paused other
    /// than the one being inspected: a request the reader is holding open.
    /// `child` names the child session the thread belongs to, where it is
    /// not the session's own.
    Thread {
        id: i64,
        name: String,
        paused: bool,
        child: Option<String>,
    },
    /// The inspected thread's Frame at this index of [`frames`].
    Frame(usize),
    /// A run of Library frames folded into one row, by where it starts and
    /// how many it holds.
    Library { start: usize, count: usize },
}

/// The Frames grouped by thread: the inspected one first with its calls, so
/// what is being looked at never moves down as threads start, then every
/// other thread the adapter named. Read by `ui` to draw, by `mouse` to
/// hit-test and by `update` to act on the row chosen, so the three cannot
/// disagree about what a row is.
///
/// A run holding the chosen Frame never folds: a pause inside a library is
/// the one place its calls are the point.
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

/// The row the Frame at `index` is drawn on.
fn row_of(state: &State, index: usize) -> usize {
    frame_rows(state)
        .iter()
        .position(|row| *row == FrameRow::Frame(index))
        .unwrap_or_default()
}

/// The Variables as they are drawn: the exception that paused the program if
/// one did, then the scopes, and under each open row the children that have
/// arrived — the tree flattened to what is open, and nothing that is not.
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

/// Whether an expression calls something, read off the same syntax `ui`
/// colours the editor with — never by handing it to the adapter to try, which
/// is the call the mark exists to warn about. `named` is the file whose
/// language the expression is written in, and `f(x)` is a call in some
/// languages and an index in others, so the wrong one answers no and the
/// caller runs the call it was asking about.
///
/// The two callers name two different files on purpose: a Watch is written in
/// the Paused Frame's language, while a Hover's expression was read out of
/// the Buffer under the pointer — which need not be the file the program
/// stopped in.
pub fn paused_in(state: &State) -> String {
    file_named(paused_line(state).map(|(file, _, _)| file))
}

/// A path's last component, which is all [`crate::highlight`] reads a
/// language off. Empty for no path at all, which is the one plain token per
/// line an unknown extension already gets.
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

/// One member and, while it is open, everything under it — then the row that
/// stands for the rest of a collection whose page has not been asked for.
///
/// `walked` is what stands above this member, and a reference already on it is
/// not opened again: the references are the adapter's, which is untrusted
/// input, and one that holds itself would otherwise be a structure the reader
/// could open into a stack overflow.
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
    // A scope is a heading rather than a name the program knows, so it has no
    // expression of its own and its members start from theirs.
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

/// The row the keyboard is on in the Variables, if it names one.
pub fn row(state: &State) -> Option<Row> {
    variables(state).into_iter().nth(state.variables_selection)
}

/// The Chips the row at `index` carries: its own actions, and only on the row
/// the keyboard is on — a pane drawing every row's actions is a pane of
/// icons with one row's worth of meaning.
///
/// Dimmed, never hidden, so the reader learns the action exists and why it
/// cannot run here: setting a value needs an adapter that said it can, and
/// the last two are issues #60 and #70 — a Chip teaching a key nobody bound
/// is the cheatsheet contract broken from the other end.
pub fn row_chips(state: &State, index: usize) -> Vec<crate::Chip> {
    use crate::{Chip, Hue, Tone};
    // The cheap question first: `ui` asks this of every row it draws, and
    // flattening the tree per row is the pane's whole cost squared.
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
    // A scope is a heading, and the row that stands for a collection's next
    // page is a place in the list — neither is a member the program could be
    // asked about, so what acts on a member is dimmed on both. `parent` is
    // the container `setVariable` writes into and `expression` is what a
    // Watch would carry: a row with neither is a row those two cannot act on.
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
        // A row with no expression is a heading or a place in a list, which
        // is nothing the Evaluator could be opened on.
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

/// The watch Chip on a member's row, and a Watch typed into the box the `a`
/// key opens: the expression joins the Watches and is evaluated at the next
/// pause — and at this one, if the program is stopped, since a Watch added
/// while Paused with nothing to show is a Watch that looks broken.
pub fn add_watch(next: &mut State, expression: String) -> Vec<Effect> {
    if expression.is_empty() || next.watches.iter().any(|w| w.expression == expression) {
        return Vec::new();
    }
    next.watches.push(Watch {
        expression: expression.clone(),
        answer: Answer::Waiting,
    });
    // Only the one just added: the Watches above it have been answered for
    // this pause already, and asking for all of them again would blank every
    // value on screen because somebody added a sixth.
    evaluate(next, &[expression], WATCH_CONTEXT)
}

/// The remove-watch Chip on a Watch's row.
pub fn remove_watch(next: &mut State, index: usize) {
    if index < next.watches.len() {
        next.watches.remove(index);
    }
}

/// One `evaluate` per Watch, in the protocol's `watch` context and against
/// the Frame being inspected — asked at every pause and again whenever
/// another Frame is chosen, since the same expression means something else
/// one call up. Nothing at all while the program runs: an adapter asked to
/// evaluate in a Frame that is no longer stopped answers with an error.
fn evaluate_watches(next: &mut State) -> Vec<Effect> {
    let watches: Vec<String> = next
        .watches
        .iter()
        .map(|watch| watch.expression.clone())
        .collect();
    // Every answer goes back to waiting first: they are about the pause that
    // has just ended, and a value left standing under a new pause is a value
    // the reader has no way to tell is stale.
    for watch in next.watches.iter_mut() {
        watch.answer = Answer::Waiting;
    }
    evaluate(next, &watches, WATCH_CONTEXT)
}

/// The `evaluate` requests for `watches`, or none at all while the program is
/// not stopped in a Frame to evaluate them in: an adapter asked to evaluate
/// in a Frame that is running answers with an error.
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

/// The text typed into the row's box, sent to the adapter exactly as it was
/// written: it is an expression in the program's language, which Varde does
/// not parse and must not rewrite. Refused by the capability rather than by
/// trying it, which is what the dimmed Chip already says.
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

/// Enter on a Variables row, and a click on one: a member with children is
/// opened or closed, and the row that stands for a collection's next page asks
/// for it. Opening asks for one level — the children of that reference and
/// nothing under them — so walking a deep structure asks for what is opened
/// and nothing else.
pub fn open(next: &mut State, index: usize) -> Vec<Effect> {
    match variables(next).get(index).cloned() {
        Some(row) => opened(next, row),
        None => Vec::new(),
    }
}

/// What opening a row asks for, whichever list the row came from: the
/// Variables' and the Hover's are rows of one tree, so a reference opened in
/// one is opened in the other.
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
            // Asked for once: a row closed and opened again shows what already
            // arrived, since the values have not changed while the program is
            // stopped.
            match pause.children.contains_key(&reference) {
                true => Vec::new(),
                false => vec![fetch(session, reference, paged(indexed, 0))],
            }
        }
        // A next-page row exists only over a collection that is being read a
        // page at a time, so where it carries on from is the page to ask for.
        Opens::NextPage { reference, start } => vec![fetch(session, reference, Some(start))],
    }
}

/// A member's expression under whatever holds it: an indexed member carries
/// on from its container's own spelling, so `orders` and `[0]` read as
/// `orders[0]`, and anything else is reached through a dot. A scope has no
/// expression at all, so its members start from their own names.
fn joined(path: &str, name: &str) -> String {
    match (path.is_empty(), name.starts_with('[')) {
        (true, _) => name.to_string(),
        (false, true) => format!("{path}{name}"),
        (false, false) => format!("{path}.{name}"),
    }
}

/// Which page of a collection of `indexed` members to ask for, starting at
/// `start` — none at all for one small enough to come whole, since an adapter
/// answering a whole small scope is one round trip rather than two.
fn paged(indexed: usize, start: usize) -> Option<usize> {
    (indexed > PAGE).then_some(start)
}

/// One level of `reference`: the page named, or everything it holds.
fn fetch(session: &mut Session, reference: i64, page: Option<usize>) -> Effect {
    let arguments = match page {
        Some(start) => json!({ "variablesReference": reference, "start": start, "count": PAGE }),
        None => json!({ "variablesReference": reference }),
    };
    ask(session, "variables", arguments)
}

/// One member of the Variables, as the adapter worded it — with nothing in it
/// that could drive the terminal it is about to be drawn on, for the reason a
/// Frame's name is stripped.
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

/// What a Hover asked the Debug adapter while Paused: the expression the
/// syntax under the pointer named, where it starts so the editor can mark it,
/// and what came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hovered {
    pub expression: String,
    pub at: Place,
    pub held: Held,
}

/// What the Hover's expression holds, as far as the adapter has said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// The expression calls something, so the adapter was never asked: a
    /// pointer crossing `delete_order()` on its way somewhere else must not
    /// delete an order. Only the Evaluator runs a call, because somebody
    /// pressed a key for it.
    NeedsEvaluate,
    /// Asked and not yet answered.
    Waiting,
    Failed(String),
    Value {
        value: String,
        reference: i64,
        indexed: usize,
    },
}

/// The Hover's value section while Paused, and the `evaluate` that asks for
/// it. Nothing at all outside a session and while the program runs, where a
/// Hover is the language server's answer and nothing more.
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
    // The Buffer the expression was read out of, not the file the program
    // stopped in: they need not be the same file, and a call parsed under
    // another language's grammar is a call this is about to run.
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

/// The protocol's contexts an `evaluate` goes out in. Both what tells the
/// adapter how its answer will be used — `hover` is the one that asks it to
/// answer cheaply and without side effects — and, since a reply names nothing
/// of its question, how [`hover_asked`] tells a Hover's answer from a Watch's.
/// Named rather than written at each call site: a literal mistyped at one of
/// the three still compiles and silently files the answer against the wrong
/// thing.
const HOVER: &str = "hover";
const WATCH_CONTEXT: &str = "watch";

/// The expression the syntax under a place names, and the column it starts
/// at: `order.total` where the pointer is on `total`, and the whole call where
/// it is on the name of one. Character by character over the line the Buffer
/// holds rather than over [`crate::highlight`]'s tokens, because a token is
/// grouped by scope and not by name — `mentioned` splits them again for the
/// same reason.
///
/// A place that is not in a name is no expression at all: the `7` of `load(7)`
/// is a literal, and the only expression around it is the call it is an
/// argument to — which is the one thing a Hover may not ask about.
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
    // A name no language lets start with a digit is a literal, and a literal
    // has nothing the program could be asked about.
    if line[from].is_ascii_digit() {
        return None;
    }
    // Back through the receivers a field is read off: `total` on its own is
    // not a name the program knows, and asking for it would either fail or —
    // worse — answer about some other `total` in scope. A receiver that is
    // itself a call or an index comes too, closer first, which is what makes
    // `get().total` an expression that calls something rather than a bare
    // `total` the adapter would happily evaluate.
    while from > 1 && line[from - 1] == '.' {
        let Some(receiver) = ends_at(&line, from - 2) else {
            break;
        };
        from = receiver;
    }
    // And on over the call the name opens, if it opens one: a call's name
    // alone evaluates to the function, which is not what is being pointed at.
    // Balanced, so a call taking a call is one expression.
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

/// One line of the buffer on screen, 1-based, as characters — what both
/// readings of an expression walk.
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

/// What `\u{2423}e` opens the Evaluator on: the Selection if there is one, and
/// otherwise the whole expression the cursor stands in — `orders.len()` with
/// the cursor on `orders`, where a Hover would name `orders` alone.
///
/// The difference is deliberate and is the same one that lets this run a call
/// at all: a Hover describes whatever the pointer happens to cross, so it
/// stops at the part it is resting on and refuses to call anything; this ran
/// because somebody pressed a key for it, so the call at the end of the chain
/// is the point rather than the danger.
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

/// On through the chain the expression continues into: `.len()` after
/// `orders`, and the groups each name in it carries, balanced so a call
/// taking a call is one expression.
fn chain_end(line: &[char], mut to: usize) -> usize {
    loop {
        // The groups the name just passed carries — a call's arguments, an
        // index — balanced, so a call taking a call is one expression.
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
                    // Unbalanced, so there is no group to take: the expression
                    // ends where the name did.
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
        // A dot with no name after it ends nothing — a decimal point, or a
        // chain the reader has not finished typing.
        if after == to + 1 {
            return to;
        }
        to = after;
    }
}

fn named(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Where the expression ending at `last` begins: a name, or a name followed by
/// as many bracketed groups as it carries — `a.b()[0]` read right to left.
/// `None` where what ends there is not an expression at all, which leaves the
/// chain broken and the walk above stopping where it stands.
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
        // Past the group, onto whatever it hangs off: another group, or the
        // name the whole chain starts at.
        at = at.checked_sub(1)?;
    }
    while at > 0 && named(line[at - 1]) {
        at -= 1;
    }
    Some(at)
}

/// The expression the Hover describes, and how many characters of the line it
/// spans: the editor washes it while the box is up, so what was evaluated is
/// never left for the reader to guess from a box floating over the code.
pub fn hover_span(state: &State) -> Option<(Place, usize)> {
    let hovered = state.hover.as_ref()?.value.as_ref()?;
    Some((hovered.at, hovered.expression.chars().count()))
}

/// The Hover's value as rows, the tree flattened exactly as the Variables' is
/// — the same [`draw`], so a structure opens the same way in both places and
/// opening it in one is opening it in the other. Empty until the adapter has
/// answered: what a Hover says while it waits, and what it says about a call
/// it will not run, are [`crate::lsp::Said`]'s to draw.
pub fn hovered_rows(state: &State) -> Vec<Row> {
    let Some(hovered) = state.hover.as_ref().and_then(|hover| hover.value.as_ref()) else {
        return Vec::new();
    };
    let Some(pause) = showing(state) else {
        return Vec::new();
    };
    let member = match &hovered.held {
        Held::NeedsEvaluate => return Vec::new(),
        // The expression with nothing beside it yet, exactly as a Watch waits:
        // a box that showed nothing at all until the adapter answered would
        // open two columns wide and jump to its real size a moment later.
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

/// The Chips on the Hover's top border: the Evaluator, which is the only way
/// to know what a call returns, and a Watch, which keeps the expression on the
/// Variables once the pointer has moved on. Their own Chips and not the
/// Variables row's, because the two act on different expressions.
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

/// The Hover's Chips as they are drawn and hit-tested — the renderer, the
/// mouse and the box's own width read this one list, for the reason every
/// other strip of Chips has one. Nothing reserved for a title, unlike a pane's
/// border: the box has no name written on it.
pub fn hover_labels(state: &State, width: u16) -> Vec<String> {
    crate::layout::chip_labels(&hover_chips(state), width, 0)
}

/// The Watch Chip on the Hover: the expression the box describes joins the
/// Watches, so a value worth a second look outlives the pointer that found it.
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

/// A click on a row of the Hover's value, which opens it exactly as the same
/// row of the Variables opens.
pub fn open_hovered(next: &mut State, index: usize) -> Vec<Effect> {
    match hovered_rows(next).get(index).cloned() {
        Some(row) => opened(next, row),
        None => Vec::new(),
    }
}

/// The floating window that runs a Snippet inside the Paused program, in the
/// chosen Frame. The Snippet is a [`crate::editor::Buffer`] so it inherits
/// the editor's own gestures rather than a second text editor written on a
/// string — the reason the comment box is one too.
///
/// Core state rather than the session's, though it closes with one: what a
/// reader is in the middle of writing is theirs, and the Snippet they ran is
/// remembered past the program it ran in.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluator {
    pub snippet: crate::editor::Buffer,
    /// The run on screen, and `None` until the Snippet has been run once.
    /// Replaced whole at every run: the output is about the run just made,
    /// and a line left over from the one before it reads as this one's.
    pub ran: Option<Run>,
    /// How many rows of the window the Snippet takes, and `None` until the
    /// rule under it is dragged — half, until somebody names a number, the
    /// way the Strip's height is a share until somebody drags it. With the
    /// window rather than with the project: the reader is dividing the room
    /// they have between what they are writing and what came back.
    pub snippet_rows: Option<u16>,
    /// How far back through the project's Snippets Up has walked, and `None`
    /// while the keyboard is in the Snippet rather than in its history.
    recalled: Option<usize>,
}

/// The modifier-free keyboard mode that arranges the Evaluator's window: `␣m`
/// moves it and `␣z` resizes it, and the same four letters do both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrange {
    Moving,
    Sizing,
}

/// One run of the Snippet: what the program printed while it ran and what it
/// came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// What went to the adapter — the Selection where there was one, and not
    /// the whole Snippet. Taken at the run rather than read back off the
    /// Selection, which the reader has moved on by the time an answer lands:
    /// the value is labelled with the code that produced it or with nothing.
    ran: String,
    /// In the order it arrived, and before the value below: a side effect
    /// happens while the expression that has it is still running.
    printed: Vec<String>,
    answer: Ran,
}

/// Where a run has got to. An enum for the reason [`Phase`] is one: running
/// and failed at once has no answer for the Run Chip.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ran {
    /// The `seq` the `evaluate` went out with, which is what a cancel names —
    /// the protocol takes a request back by its number and by nothing else,
    /// and it is also how a reply is told from a reply to the run before it.
    Running(i64),
    Value {
        value: String,
        reference: i64,
        indexed: usize,
    },
    Failed(String),
}

/// One row of the Evaluator output as it is drawn, hit-tested and asserted
/// on: the prints, then the value's tree or the adapter's reason. One list
/// for the reason the Variables are one list. Named as [`crate::lsp::Said`]
/// is, and for its reason — a row of a box is what the box says — which also
/// keeps it apart from the renderer's own `Line`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Printed(String),
    /// The value, and everything opened under it — the adapter's tree, drawn
    /// and opened exactly as the Variables are.
    Value(Row),
    /// The adapter's reason, kept apart from a value for the reason a Watch's
    /// is: a reason drawn as a value reads as the program's own answer.
    Failed(String),
    /// Gone out and nothing back yet.
    Running,
}

/// The protocol's context a Snippet goes out in. `repl` is the one that tells
/// the adapter the answer is for a person to read and that side effects are
/// wanted — a Snippet is run because somebody pressed a key for it, which is
/// the whole difference from the `hover` context beside it.
const REPL: &str = "repl";

pub const RUN: &str = "evaluator-run";
pub const CANCEL: &str = "evaluator-cancel";
pub const CLOSE: &str = "evaluator-close";

/// `␣e`, the Hover's Evaluate Chip and a Variables row's: the Evaluator opens
/// on the expression the gesture named, prefilled and asking the adapter
/// nothing. What runs is what the reader presses Enter on, which is the whole
/// reason a Hover may refuse to evaluate a call and this may not.
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
    // Where the project last left it, and the middle of the screen the first
    // time — `settle` places it from there, so a rectangle recorded on a
    // bigger screen needs no arm of its own here.
    next.evaluator_at =
        Some(next.evaluator_at.unwrap_or_else(|| {
            crate::layout::centred_window(next.screen_width, next.screen_height)
        }));
    next.focus = Pane::Evaluator;
    // The Selection the expression was *read from* goes with it. It is a span
    // of the buffer behind the window, and the run below reads a Selection
    // against the Snippet: left standing, `\u{2423}e` on line 3 columns 17-28 would
    // be read against a one-line Snippet and send the adapter an empty
    // expression, silently. A Selection in the Snippet is one made in the
    // Snippet.
    next.selection = None;
}

/// Normal-mode Enter, the Run Chip and Ctrl+Enter: the Selection if there is
/// one and the whole Snippet otherwise, in the `repl` context and against the
/// Frame being inspected. Nothing at all while the program is not stopped in
/// a Frame to run it in — an adapter asked to evaluate then answers with an
/// error, which is why a Watch is not asked then either.
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

/// The cancel Chip: the run still in flight taken back by its number. Refused
/// by the capability rather than by trying it, which is what the dimmed Chip
/// already says — the reason a set value is refused that way.
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
    // Over the connection the request went out on, the only one whose
    // numbering `requestId` means anything in.
    let to = session.asked.get(&seq).map_or(0, |asked| asked.to);
    vec![ask_on(session, to, "cancel", json!({ "requestId": seq }))]
}

/// Where the window goes: the rectangle a gesture named, placed on the screen
/// and clear of the Paused line, and recorded for the project. Placed here as
/// well as in [`crate::settle`] — the one function, called twice — because the
/// number written to disk has to be the number on screen, and the effect
/// carrying it is built before the clamp every event goes through.
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

/// The keyboard's arrange: one cell of the window per key, in the mode `␣m`
/// or `␣z` opened. The letters are the editor's own motions, so moving the
/// window and moving the caret are the same four keys — the modifier-free
/// gesture `AGENTS.md` requires, and the arrows reach it too.
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

/// Which screen row the Paused line is drawn on, and nothing where the file
/// it is in is not the one on screen or the wheel has taken it off the pane.
/// Read by the clamp that keeps the Evaluator off it, off the same
/// `editor_scroll` the line itself is drawn from — two derivations of where a
/// line is would be a window covering the very line it was moved to clear.
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

/// Up and Down on an empty Snippet walk the project's Snippets, newest first,
/// and Down comes forward again — the gesture every shell has, for its
/// reason: the code that worked last time is the code being reached for.
/// `true` where the arrow was the history's, which is what leaves it the
/// caret's motion on a Snippet somebody is writing.
///
/// Walking outlives the emptiness that started it: the Snippet a recall put
/// there is not empty, and a second Up that moved the caret instead would be
/// a history one step deep.
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
        // Down with nothing recalled is the caret's, on a Snippet with
        // nothing above it to come forward from.
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

/// What this run sends: the Selection where the keyboard is in the Snippet
/// and there is one, so a line of a longer block can be tried on its own.
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

/// The whole Snippet, which is what is remembered whatever was run: the
/// reader wrote the block, and recalling the one line they tried of it would
/// hand back something they never typed.
fn snippet_text(state: &State) -> String {
    state
        .evaluator
        .as_ref()
        .map(|evaluator| evaluator.snippet.shown().to_string())
        .unwrap_or_default()
}

/// A Snippet kept for the project, newest last and never twice. Written where
/// the Snippet leaves the window — at a run and at the close that ends the
/// session — rather than on every keystroke, which would remember every
/// half-typed line on the way to the one that worked.
fn remember(next: &mut State, snippet: String) -> Vec<Effect> {
    if snippet.is_empty() {
        return Vec::new();
    }
    next.snippets.retain(|held| held != &snippet);
    next.snippets.push(snippet);
    vec![Effect::SaveState(crate::state_json(next))]
}

/// The Chips on the Evaluator's top border: run, the cancel that takes a run
/// back, and close. Run is dimmed while the program is not stopped, because there is
/// no Frame to run a Snippet in; cancel while there is nothing in flight or
/// the adapter cannot take one back.
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

/// The Evaluator's Chips as they are drawn and hit-tested, for the reason the
/// Hover's are one list.
pub fn evaluator_labels(state: &State, width: u16) -> Vec<String> {
    crate::layout::chip_labels(&evaluator_chips(state), width, 0)
}

/// The Evaluator output: the prints in the order they arrived, then the value
/// as a tree opened exactly as the Variables are, or the adapter's reason.
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

/// Every text the debug panes draw as code in the language of [`paused_in`]:
/// the Variables' values, the Evaluator's value rows and its Snippet — for the
/// edge to colour once as each arrives rather than once per frame. A Watch that
/// failed and the Evaluator's prints are left out: the adapter's reason and
/// what the program printed are words, not code.
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

/// A click on a row of the Evaluator's value, which opens it exactly as the
/// same row of the Variables opens.
pub fn open_evaluated(next: &mut State, index: usize) -> Vec<Effect> {
    match evaluator_output(next).into_iter().nth(index) {
        // A print has nothing under it to ask for, and neither has a reason.
        Some(Said::Value(row)) => opened(next, row),
        _ => Vec::new(),
    }
}

/// The run still in flight, which is what the program's prints belong to.
fn in_flight(next: &mut State) -> Option<&mut Run> {
    next.evaluator
        .as_mut()?
        .ran
        .as_mut()
        .filter(|ran| matches!(ran.answer, Ran::Running(_)))
}

/// The run an `evaluate` reply belongs to: the `repl` context is what names
/// it, and the `seq` has to still be the one in flight — a reply to the run
/// before this one belongs to output that has already been replaced.
fn ran_asked<'a>(next: &'a mut State, arguments: &Value, seq: i64) -> Option<&'a mut Run> {
    if arguments["context"] != json!(REPL) {
        return None;
    }
    in_flight(next).filter(|ran| ran.answer == Ran::Running(seq))
}

/// What asking the AI about the pause pastes: where it stopped, with two lines
/// either side when the editor holds the file, the Frames, and the Variables
/// exactly as drawn — so a row left closed costs the adapter no request, and
/// the exception comes first as it does on screen. `None` unless Paused.
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

/// Where the program is paused, as the chosen Frame names it, and why.
pub fn paused_line(state: &State) -> Option<(&Path, usize, Why)> {
    let Some(Phase::Paused(pause)) = state.debug.as_ref().map(|session| &session.phase) else {
        return None;
    };
    let frame = pause.frames.get(pause.chosen)?;
    Some((frame.file.as_deref()?, frame.line, pause.why))
}

/// One value drawn at the end of a line the Paused call has already run.
/// `text` is exactly what the renderer draws, its gap included and its value
/// already trimmed to the columns the line leaves — the trimming is the
/// library's because *does it fit* is a question about the pane, and a
/// renderer that cut it itself would be a second author for the width. `name`
/// is what the line mentioned, which is the only thing the value belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inline {
    pub name: String,
    pub text: String,
    /// Whether this pause is the one where the value moved. One pause only:
    /// a mark that stayed would say the same thing at every stop, and what
    /// makes stepping readable is seeing what the line just did.
    pub changed: bool,
}

/// The gap before an Inline value, and between two of them.
const GAP: usize = 2;

/// The Inline values of the Buffer on screen, by line: every local the chosen
/// Frame holds, drawn at the end of the line that mentions it, from the line
/// the call opens on down to the line before the Paused one. Never on the
/// Paused line or past it — the call has not run those, so a value shown there
/// would be the one from before it was assigned.
///
/// `tokens` are the current Buffer's, parsed once per edit by the edge and
/// handed in for the reason [`crate::minimap::cells`] is handed them: nothing
/// parses per frame. `columns` is what
/// [`crate::fits_in`] counted off the rectangles the renderer drew, handed in
/// for the reason `mouse` is handed them: one derivation of where the text
/// ends, or a value is trimmed against a pane of another size.
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
    // The Buffer on screen and the call being inspected have to be the same
    // file: a value drawn against another file's line numbers names a line
    // nobody is paused in.
    let Some(buffer) = frame
        .file
        .as_ref()
        .filter(|file| state.current_buffer.as_ref() == Some(*file))
        .and_then(|file| state.buffers.get(file))
    else {
        return drawn;
    };
    let held = locals(pause);
    // Nothing is marked while the program runs: a highlight says *this pause*
    // moved it, and between pauses there is no such pause. What is on screen
    // then is the last one's values, whole and faint, which is what dims them.
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
            // A name with no room left for even one column of its value is
            // not drawn at all, and neither is anything after it: code pushed
            // off screen is the one thing an Inline value must never do.
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

/// `text` cut to `columns` **display** columns, for the reason `ui`'s own
/// truncation is by width: a wide glyph cut on a character boundary still
/// overruns the column it was cut to fit, and the column it overruns is the
/// editor's last one.
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

/// What the chosen Frame's scopes hold, by name, in the adapter's own words.
/// The top level of the tree and nothing under it: a member called `id` inside
/// an order is not a name the code on screen mentions.
fn locals(pause: &Pause) -> BTreeMap<String, String> {
    pause
        .scopes
        .iter()
        .filter_map(|scope| pause.children.get(&scope.reference))
        .flatten()
        .map(|member| (member.name.clone(), member.value.clone()))
        .collect()
}

/// The names a line mentions, in the order it mentions them: the words of the
/// tokens the highlighter left plain. Which names are variables is the
/// grammar's answer and never a rule of Varde's — a name inside a comment, a
/// string, a call or a type is some other kind and never reaches here, in
/// every language the grammar set knows.
fn mentioned(tokens: &[crate::highlight::Token]) -> Vec<&str> {
    tokens
        .iter()
        .filter(|token| token.kind == crate::highlight::Kind::Plain)
        .flat_map(|token| token.text.split(|c: char| !c.is_alphanumeric() && c != '_'))
        .filter(|word| !word.is_empty())
        .collect()
}

/// The line the Paused call opens on: the innermost block holding the Paused
/// line, read off the indentation [`crate::fold`] already derives rather than
/// from a brace matcher — the same answer in Python as in Rust, and no
/// per-language rule about what a call looks like. A Paused line in no block
/// at all is a script's top level, where everything above it has run.
fn call_start(source: &str, line: usize) -> usize {
    crate::fold::blocks(source)
        .into_iter()
        .filter(|block| block.from < line && line <= block.to)
        .map(|block| block.from)
        .max()
        .unwrap_or(1)
}

/// The whole inspection moved to the chosen Frame: its file brought on screen
/// at its line — unless it is the Buffer already there, since the cursor of
/// the file being typed in is the reader's and a pause never moves it — and
/// its scopes asked for, which is what the Variables draw.
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
    // Asked for here rather than at the pause, because choosing another Frame
    // is the same question asked about another call.
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
    // The Watches with them: the same question asked of the same Frame, so
    // they go where the scopes go rather than at the `stopped` event, which
    // has no Frame to evaluate in yet.
    effects.extend(evaluate_watches(next));
    effects
}

/// What the Corner holds outside a session: what it held before one began,
/// which is what ending the session gives back and so what a restart, which
/// has no session, should show.
pub fn resting_corner(state: &State) -> layout::Corner {
    state
        .debug
        .as_ref()
        .map_or(state.corner, |session| session.corner)
}

/// The session ends: the Corner gets back what it held before, and the
/// keyboard leaves a pane that is going. Stepping mode goes with it — the
/// letters it claims do nothing without a session, and one that swallowed
/// them for nothing would be a mode nobody could see they were in.
fn end(next: &mut State) -> Vec<Effect> {
    next.corner = resting_corner(next);
    // The Strip the same way, which is the whole of what a session borrowed:
    // both slots go back to what they held before it began.
    next.strip = next
        .debug
        .as_ref()
        .map_or(next.strip, |session| session.strip);
    next.debug = None;
    next.stepping = false;
    // The mark is a claim that the Program output printed something nobody
    // has read; with the session gone there is nothing left to show, so it
    // goes too rather than standing over the next session's tab.
    next.output_unseen = false;
    // Both of the session's own panes go with it, so the keyboard is never
    // left in one that is no longer on screen. The Evaluator is a third: it
    // runs code inside a program, and a window offering to run one with
    // nothing to run it in is a window that can only refuse.
    if matches!(next.focus, Pane::Frames | Pane::Variables) {
        next.focus = Pane::Editor;
    }
    close_evaluator(next)
}

/// Escape, `:q`, or the session ending. The Snippet outlives the window it
/// was written in, whether or not it was ever run: closing over a
/// half-written block must not be what loses it.
pub fn close_evaluator(next: &mut State) -> Vec<Effect> {
    if next.focus == Pane::Evaluator {
        next.focus = Pane::Editor;
    }
    let snippet = snippet_text(next);
    next.evaluator = None;
    remember(next, snippet)
}

/// The Hover an `evaluate` was sent for, if this one was: the `hover` context
/// is what names it, and the expression has to still be the one the box is a
/// claim about — a reply for an expression the pointer has moved off belongs
/// to nothing on screen.
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

/// The Watch an `evaluate` was sent for, found by the expression the request
/// carried rather than by whichever row the keyboard has reached since.
fn watch_asked<'a>(next: &'a mut State, arguments: &Value) -> Option<&'a mut Watch> {
    let expression = arguments["expression"].as_str()?;
    next.watches
        .iter_mut()
        .find(|watch| watch.expression == expression)
}

/// Why the adapter refused, in its own words and stripped of anything that
/// could drive the terminal they are about to be drawn on. The readable text
/// is the error's `format` where the adapter sent one; `message` is often
/// only a short code, and the command it answered is the last resort.
fn adapter_error(message: &Value, command: &str) -> String {
    printable(
        message["body"]["error"]["format"]
            .as_str()
            .or(message["message"].as_str())
            .unwrap_or(command),
    )
}

/// The adapter's words with nothing left in them that could drive the
/// terminal they are about to be drawn on.
pub(crate) fn printable(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .collect()
}

/// One request, over the connection the thread it names lives on, or else the
/// one the thread being inspected does — a Frame's scopes, a member's
/// children and a Watch all belong to the child whose thread paused.
fn ask(session: &mut Session, command: &str, arguments: Value) -> Effect {
    let to = match (arguments["threadId"].as_i64(), &session.phase) {
        (Some(thread), _) => link_of(thread).0,
        (None, Phase::Paused(pause) | Phase::Running(Some(pause))) => link_of(pause.thread).0,
        (None, _) => 0,
    };
    ask_on(session, to, command, arguments)
}

/// One request over connection `to`, numbered and remembered until its answer
/// arrives. The numbering is the session's across every connection, so an
/// answer is found by it alone whichever child sent it.
fn ask_on(session: &mut Session, to: usize, command: &str, arguments: Value) -> Effect {
    session.seq += 1;
    // The request itself rather than a copy of the parts of it somebody
    // expected to need: the two could then say different things, and the
    // answer would be filed under the question nobody asked.
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

/// The same request over every connection: what is true of the whole program
/// — its Breakpoints, its threads, that it is being stopped — is true of each
/// child's part of it.
fn ask_all(session: &mut Session, command: &str, arguments: Value) -> Vec<Effect> {
    links(session)
        .into_iter()
        .map(|to| ask_on(session, to, command, arguments.clone()))
        .collect()
}

/// The session's own connection and every child's.
fn links(session: &Session) -> Vec<usize> {
    std::iter::once(0)
        .chain(session.children.keys().copied())
        .collect()
}

/// A child session gone: its threads with it, and an inspection of one of
/// them, which has nothing left to ask.
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

/// The answer to a request the adapter made of Varde, over the connection it
/// came from.
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

/// The `seq` a request still waiting for its answer went out with, so a test
/// can answer it the way the adapter would.
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

/// `state` with a session Paused on thread 1 in `main` at line 1 of
/// `/w/one.rs`, driven there through the protocol a real adapter speaks, for
/// the tests outside this module that need one.
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
    // Settled as `update` leaves every message the edge hands it, so the
    // first event a test sends is not the one that scrolls to the Frame.
    crate::settle(state, Vec::new(), false).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The session's own connection, which is what nearly every test speaks
    /// over; the child-session tests name theirs.
    fn received(next: &mut State, json: &str) -> Vec<Effect> {
        super::received(next, json, 0)
    }

    fn started(next: &mut State) -> Vec<Effect> {
        super::started(next, 0)
    }

    /// What `\u{2423}e` names where a Hover would name less: the chain the
    /// cursor stands in, the groups each name in it carries, and the two
    /// shapes that end it early. The scenarios drive one line of Rust; these
    /// are the shapes a real line has that no scenario would be readable
    /// enumerating.
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
        // On the receiver, and the call at the end of the chain comes too.
        assert_eq!(named("    let n = orders.len();", 17), "orders.len()");
        // Every link of a longer one, from anywhere along it.
        assert_eq!(named("a.b().c[0].d", 1), "a.b().c[0].d");
        assert_eq!(named("a.b().c[0].d", 7), "a.b().c[0].d");
        // A call taking a call is one expression, brackets balanced.
        assert_eq!(named("x.f(g(1)).y", 1), "x.f(g(1)).y");
        // A dot with no name after it ends the chain: a decimal point, and a
        // chain the reader has not finished typing.
        assert_eq!(named("total.", 1), "total");
        assert_eq!(named("n + 1.5", 1), "n");
        // A group nobody closed is a group there is nothing to take.
        assert_eq!(named("a.b(1", 1), "a.b");
        // A place in no name at all names nothing.
        assert_eq!(named("    let n = 7;", 13), "");
    }

    /// What the pointer names, branch by branch. The scenarios drive four
    /// places in one file; these are the shapes a real line has that no
    /// scenario would be readable enumerating.
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
        // A name, and the receivers a field is read off — however many.
        assert_eq!(
            named("    let order = load(7);", 9),
            Some(("order".to_string(), 9))
        );
        assert_eq!(
            named("    one.two.three = 1", 13),
            Some(("one.two.three".to_string(), 5))
        );
        // The call a name opens, balanced, so a call taking a call is one
        // expression — and the receiver in front of it comes too.
        assert_eq!(
            named("    delete_order(order.id);", 5),
            Some(("delete_order(order.id)".to_string(), 5))
        );
        assert_eq!(
            named("    let n = a.count(b(c));", 15),
            Some(("a.count(b(c))".to_string(), 13))
        );
        // A receiver that is itself a call or an index comes too, closer
        // first — which is what makes the first of these an expression that
        // calls something rather than a bare `total` the adapter would
        // happily evaluate.
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
        // A chain broken by something that is not an expression stops where
        // it stands rather than reaching past it.
        assert_eq!(
            named("    let n = 1 + .total;", 18),
            Some(("total".to_string(), 18))
        );
        // A literal is no expression: the only one around the `7` below is
        // the call it is an argument to, which is what a Hover may not ask.
        assert_eq!(named("    let order = load(7);", 22), None);
        // And neither is whitespace, punctuation, or a column past the line.
        assert_eq!(named("    let order = load(7);", 15), None);
        assert_eq!(named("    let order = load(7);", 90), None);
    }

    /// The Selection `\u{2423}e` read the expression off is a span of the buffer
    /// *behind* the window, and a run reads a Selection against the Snippet.
    /// Left standing it named columns 17-28 of a one-line Snippet, so the
    /// adapter was sent an empty expression and nothing on screen said so.
    /// Driven here rather than in a scenario because the defect is the absence
    /// of a Selection, and no `Then` can see one that is gone.
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

    /// An adapter that refuses an `evaluate` the Hover sent: its reason
    /// stands in the box, for the reason a Watch's does. Left waiting, the
    /// box would show the expression with nothing beside it forever, which
    /// reads as a debugger that lost the question.
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

    /// A Hover over a call is the absence this feature exists for, and the
    /// branch that decides it is [`calls`] — driven here over the shapes the
    /// scenarios' one file does not have.
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
        // The argument inside it is asked for, and for itself alone.
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

    /// The route a real adapter takes to the set-value capability: its
    /// `initialize` reply, which no scenario drives because every scenario
    /// reaches the same field through the `capabilities` event. Both write
    /// the one field, so the Chip cannot be dimmed by one and lit by the
    /// other.
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
        // And the event may take it away again, which is the same field.
        received(
            &mut can,
            r#"{"type":"event","event":"capabilities","body":{"capabilities":{"supportsSetVariable":false}}}"#,
        );
        assert!(!can.debug.as_ref().expect("a session").can_set);
    }

    /// The halves of R42.8 no scenario reaches: a switch turned back off is
    /// sent without it, a remembered filter this adapter did not report is
    /// never sent under its id, and a class once named survives a switch —
    /// the request replaces what the adapter held, so leaving either out of
    /// a later one would quietly undo it.
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

    /// A Watch typed rather than taken off a row — the `a` key's box, which
    /// is the half of "added from a row or typed" no scenario drives. The
    /// same `add_watch` either way, so a Watch typed twice is still one.
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

    /// The bug this closes: the answer used to be filed against whichever row
    /// the keyboard had reached by the time it arrived, and then against
    /// every member of that name anywhere in the tree — so setting
    /// `first.count` rewrote `second.count` too. No scenario reaches it,
    /// because `variables.feature` sets a member of the one flat scope.
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
        // Both structs opened, each holding a member called `count`.
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
        // The first struct's `count` is written, and then the selection moves
        // away before the answer lands — which is what used to decide it.
        state.variables_selection = 2;
        let effects = set_value(&mut state, "9".to_string());
        assert_eq!(effects.len(), 1, "one setVariable");
        // Onto a row of another name entirely, which is what the selection
        // read at reply time would have written instead.
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
                // The scope heading, which has a name and no value.
                (String::new(), String::new()),
                ("first".to_string(), "…".to_string()),
                ("first.count".to_string(), "9".to_string()),
                ("second".to_string(), "…".to_string()),
                ("second.count".to_string(), "0".to_string()),
                ("other".to_string(), "7".to_string()),
            ]
        );
    }

    /// A member's expression is the path a reader could type: an index
    /// carries on from its container, a field is reached through a dot, and a
    /// scope is neither — it is a heading the program does not know.
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

    /// What a Breakpoint does when hit decides its glyph: a Logpoint prints
    /// whatever condition it also carries, and a Stale one pauses nowhere.
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

    /// The whole of what an edit can do to a Breakpoint: nothing above it moves
    /// nothing, a line inserted or deleted above carries it, and deleting its
    /// own line takes it.
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

    /// Typing on a Breakpoint's own line is not deleting it, which a diff
    /// alone would say it was: the line is replaced by a line in its place.
    /// Its text follows, so the project remembers what the line holds now.
    #[test]
    fn a_line_edited_in_place_keeps_its_breakpoint_and_takes_its_text() {
        assert_eq!(
            followed(&[on(2, "b")], "a\nb\nc", "a\n    b2\nc"),
            [(2, "b2".to_string())]
        );
        // Two lines replaced by one keeps the first and takes the second.
        assert_eq!(
            followed(&[on(2, "b"), on(3, "c")], "a\nb\nc\nd", "a\nx\nd"),
            [(2, "x".to_string())]
        );
    }

    /// A Stale breakpoint still rides its line, but keeps the text it was
    /// remembered with: it is Stale because of that text, and quietly taking
    /// the line's would make it current without anybody having looked.
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

    /// The requests `effects` sent, as the adapter would read them.
    fn sent(effects: &[Effect]) -> Vec<Value> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::DapSend { json, .. } => serde_json::from_str(json).ok(),
                _ => None,
            })
            .collect()
    }

    /// Every request and reply sent, with the connection it went over.
    fn sent_on(effects: &[Effect]) -> Vec<(usize, Value)> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::DapSend { to, json } => Some((*to, serde_json::from_str(json).ok()?)),
                _ => None,
            })
            .collect()
    }

    /// A session Paused on its own thread 1, and a child session the adapter
    /// asked for, started and configured, whose own thread 1 has stopped and
    /// is named "worker".
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

    /// Two thread 1s, one the session's and one the child's, are two rows,
    /// and the child's is labelled with the child session's name.
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

    /// The Transport acts on the thread being inspected over the connection
    /// that owns it, in the id that child knows it by.
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

    /// A child session's connection going takes its threads, never the
    /// session, and says nothing: the program being debugged is still there.
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

    /// A child the adapter refuses to start is said in the status line and
    /// let go, and the session it would have joined goes on.
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

    /// A child's program ending is its threads going and its connection let
    /// go once it has answered, never the session.
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

    /// The session's own threads running on leaves a child's Paused thread
    /// held: every thread continuing is every thread on that connection.
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

    /// F9 while running asks every connection for its threads, and one that
    /// cannot say does not stop another's thread being paused.
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

    /// A connection the session never opened a child on is not heard: its
    /// thread stopping would be a pause in a session nobody started.
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

    /// A Breakpoint set while running reaches every part of the program.
    #[test]
    fn a_breakpoint_changed_mid_session_reaches_every_child() {
        let (mut state, child) = with_child();
        let asked = sent_on(&breakpoints_changed(&mut state, Path::new("/w/one.rs")));
        let to: Vec<usize> = asked.iter().map(|(to, _)| *to).collect();
        assert_eq!(to, [0, child]);
    }

    /// A reverse request nothing answers yet is refused out loud, naming the
    /// request it answers: an adapter left waiting is a session that hangs.
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

    /// An answer to nothing Varde asked is not taken for an answer to
    /// something it did.
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

    /// The adapter's own words reach the footer, and nothing in them can
    /// drive the terminal they are drawn on.
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

    /// Pausing a program that has never paused needs a thread nobody has
    /// named yet, so the adapter is asked for them and the first is paused.
    /// A second F9 before the answer asks nothing twice.
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

    /// Stopping waits for `disconnect`'s answer, and a second stop does not:
    /// an adapter that never answers is not a session nobody can end.
    #[test]
    fn a_second_stop_lets_the_adapter_go_at_once() {
        let mut state = paused(State::default());
        let asked = sent(&stop(&mut state));
        assert_eq!(asked[0]["command"], "disconnect");
        assert!(state.debug.is_some());
        assert_eq!(stop(&mut state), vec![Effect::StopDap]);
        assert_eq!(state.debug, None);
    }

    /// An `initialized` ahead of `initialize`'s answer sends nothing: the
    /// Breakpoints wait for the program they are for to have been launched.
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

    /// A second thread stopping leaves the inspected one where it is, and a
    /// session being stopped is not paused back into life.
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

    /// Thread 2 stopped at `/w/two.rs` line 7 while thread 1 is inspected.
    fn two_paused() -> State {
        let mut state = paused(State::default());
        received(
            &mut state,
            r#"{"type":"event","event":"stopped","body":{"threadId":2,"reason":"exception","text":"boom"}}"#,
        );
        state
    }

    /// The adapter's answer to the stack `thread` was asked for, oldest first.
    fn answer_stack(state: &mut State, frames: Value) {
        let seq = outstanding(state, "stackTrace");
        received(
            state,
            &json!({"type": "response", "request_seq": seq, "success": true,
                "command": "stackTrace", "body": {"stackFrames": frames}})
            .to_string(),
        );
    }

    /// The shapes of a Library frame no scenario spells out — the Frame's own
    /// `subtle` hint and a call with no source at all — and the one run that
    /// never folds: the one the program is Paused in.
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
        // The selection lands on the chosen Frame, below its thread.
        assert_eq!(state.frames_selection, 1);
    }

    /// A `continue` answered without saying otherwise let every thread go —
    /// the protocol's default — so nothing is held any more; one that says
    /// only the asked thread ran leaves the others flagged.
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

    /// Another thread running on is one fewer held, and leaves the view on
    /// the inspected thread undimmed.
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

    /// `continued` is optional, so a held thread can stop again with nobody
    /// having said it ran. Inspected then, it is not also counted as held.
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

    /// Jumping moves the inspection and holds the thread it left, in turn and
    /// round to the first; a stack answered for the thread left behind is not
    /// filed under the one jumped to, and nothing is marked changed against
    /// another thread's values.
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
        // Thread 2's own stack, then a late one for thread 1 that must not land.
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

    /// Enter on a thread flagged as Paused goes to it, as the Chip does; on
    /// the inspected thread's own header it does nothing.
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

    /// A `threads` that failed leaves F9 able to ask again.
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

    /// Stale is a claim about the text, which no answer changes — a Stale
    /// breakpoint is not even asked about — and a bound answer that names no
    /// line binds the line it was sent.
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

    /// An Unverified breakpoint with no reason has no box to show, and once
    /// the file's Breakpoints change the answer about them is forgotten rather
    /// than read as whichever Breakpoint now sits on a line it named.
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

    /// The program ending is told to the adapter with `disconnect`, and the
    /// adapter let go once it answers.
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

    /// The Corner a restart shows is the one the session will give back, not
    /// the Frames it borrowed.
    #[test]
    fn the_corner_at_rest_is_the_one_held_before_the_session() {
        let state = paused(State {
            corner: layout::Corner::Buffers,
            ..State::default()
        });
        assert_eq!(state.corner, layout::Corner::Frames);
        assert_eq!(resting_corner(&state), layout::Corner::Buffers);
    }

    /// A reference that holds itself is the adapter's word and the adapter is
    /// untrusted input: opened, it would be walked forever. It is drawn once
    /// and its second appearance is a closed row.
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

    /// A name the grammar coloured as something else is not a variable, which
    /// is what keeps a value off a line that only talks about one. No rule
    /// here says which languages have comments or strings.
    #[test]
    fn only_the_names_the_grammar_left_plain_are_variables() {
        let tokens = crate::highlight::highlight("main.rs", "    let total = 0; // count");
        assert_eq!(mentioned(&tokens[0]), ["total"]);
        let tokens = crate::highlight::highlight("main.rs", "    let total = \"count\";");
        assert_eq!(mentioned(&tokens[0]), ["total"]);
    }

    /// The call the Paused line is in, and not the one above it: a value drawn
    /// on a line of the function before this one is a value from a call that
    /// is not on the stack.
    #[test]
    fn the_call_opens_where_the_block_holding_the_paused_line_opens() {
        let source = "fn a() {\n    let x = 1;\n}\nfn b() {\n    let y = 2;\n}";
        assert_eq!(call_start(source, 5), 4);
    }

    /// A value the line has half the room for is cut where the screen runs
    /// out, which is not where its characters do: two of these glyphs fill
    /// four columns and the third would overrun the one column left.
    #[test]
    fn a_value_is_cut_by_display_width_and_never_by_character_count() {
        assert_eq!(clipped("東京タワー", 5), "東京");
        assert_eq!(clipped("abc", 2), "ab");
    }

    /// Indentation and nothing else, so a language with no braces at all
    /// answers the same question the same way.
    #[test]
    fn a_call_in_a_language_without_braces_opens_the_same_way() {
        let source = "def main():\n    total = 0\n    print(total)";
        assert_eq!(call_start(source, 3), 1);
        // A script's top level is in no block at all, and everything above the
        // Paused line has run.
        assert_eq!(call_start("total = 0\nprint(total)", 2), 1);
    }

    /// A hosted adapter's port is the answer to the latest asking and nothing
    /// else: not a reply to another id, not one from another server, and not
    /// one arriving after the session already holds its adapter.
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

    /// A row whose arguments name `${port}` anywhere in them is a server the
    /// edge connects to, and any other is spoken to over its stdio.
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

    /// An attach naming no port has nothing to watch, so it ends with its
    /// program as a launch does rather than waiting on nothing forever.
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

    /// The source a snapshot quotes is the Paused line and two on either side,
    /// cut short at the file's ends rather than padded, and none at all for a
    /// file the editor has not read — `src/` reads no files, so it names the
    /// place instead of guessing at its text.
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

    /// #93: what the edge colours is the values and the Snippet, in the
    /// Paused Frame's language — never a name, a print or an adapter's
    /// reason. A Rust value coloured that way is more than one kind of token.
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

    /// With no Paused source file there is no language, so whatever is
    /// coloured comes back plain rather than failing.
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
