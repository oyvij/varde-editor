use crate::{
    debug, tree, Direction, Event, Find, FindIcon, FindKeys, Modal, Pane, ReplaceField, Resolution,
    Selection, State, Tap, View, FIND_ICONS, REPLACE_FIELDS,
};
use terminput::{
    Encoding, Event as Input, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode,
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Drafts {
    pub name: String,
    pub command: Option<String>,
    pub ai: String,
    pub filter: Option<String>,
    pub comment_kind: String,
}

pub const CHEATSHEET: [(&str, &str, &[View]); 47] = [
    ("i a o O x", "edit", &[View::Edit]),
    ("w b e", "word", &[View::Edit]),
    ("gg G", "file ends", &[View::Edit]),
    ("S-arr W B", "extend", &[View::Edit]),
    ("dd dG db yy p", "cut yank put", &[View::Edit]),
    ("V", "select lines", &[View::Edit]),
    (
        "C-c D-c C-v D-v",
        "copy / paste",
        &[View::Edit, View::Review, View::Story],
    ),
    ("/ n N Tab", "find / its icons", &[View::Edit]),
    ("gt gT", "buffer", &[View::Edit]),
    ("C-p C-n gp gn", "jump back / forward", &[View::Edit]),
    (
        "C-p C-n",
        "jump back / forward",
        &[View::Review, View::Story],
    ),
    ("* gr", "project", &[View::Edit]),
    ("u U C-z C-S-z", "undo / redo", &[View::Edit]),
    ("K K", "what is this / read it", &[View::Edit]),
    ("gd", "definition", &[View::Edit]),
    (
        "C-space Esc Esc",
        "palette",
        &[View::Edit, View::Review, View::Story],
    ),
    (":format", "lay the file out", &[View::Edit]),
    ("Enter Esc", "candidates", &[View::Edit]),
    ("Tab", "indent / next blank", &[View::Edit]),
    ("C-d D-d gm", "same word again", &[View::Edit]),
    ("j k V c", "select comment", &[View::Review]),
    ("h l 0", "slide sideways", &[View::Review, View::Story]),
    ("e", "edit the file", &[View::Review, View::Story]),
    (":submit", "send the review", &[View::Review, View::Story]),
    ("n p", "step", &[View::Story]),
    ("j k", "scroll", &[View::Story]),
    ("d", "show the diff", &[View::Story]),
    ("D", "step detail", &[View::Story]),
    ("g", "jump to citation", &[View::Story]),
    ("c", "comment", &[View::Story]),
    ("Esc", "back to spine", &[View::Story]),
    ("t", "spine / files", &[View::Story]),
    (
        "M-h M-j M-k M-l",
        "focus",
        &[View::Edit, View::Review, View::Story],
    ),
    ("0 $", "line ends", &[View::Edit]),
    (":preview", "read / edit markdown", &[View::Edit]),
    (
        ":read :pause :next :prev :stop :speed",
        "read the selection aloud",
        &[View::Edit],
    ),
    (":dim", "darker editor", &[View::Edit]),
    (":minimap", "mirror of the file", &[View::Edit]),
    ("C-F5", "restart debugging", &[View::Review, View::Story]),
    (
        "C-f D-f",
        "project",
        &[View::Edit, View::Review, View::Story],
    ),
    ("D", "diverged from disk", &[View::Edit]),
    (":w C-s D-s :q :qa", "write quit close all", &[View::Edit]),
    (
        ":story? :story",
        "story a branch / this change",
        &[View::Edit, View::Story],
    ),
    (
        "C-space v",
        "tools",
        &[View::Edit, View::Review, View::Story],
    ),
    ("cc ci cb", "accept conflict side", &[View::Edit]),
    (
        "e w i h j k Enter",
        "diagnostic list: severity, row, go",
        &[View::Edit, View::Review, View::Story],
    ),
    (
        "j k Enter",
        "conflict list: row, go",
        &[View::Edit, View::Review, View::Story],
    ),
];

pub const CHORDS: [(&str, &str, &[View]); 4] = [
    ("␣b", "breakpoint", &[View::Edit]),
    ("␣B", "breakpoint properties", &[View::Edit]),
    ("␣x", "run / debug this line", &[View::Edit]),
    ("C-F5 ␣r", "restart debugging", &[View::Edit]),
];

pub const DEBUG_CHORDS: [(&str, &str, &[View]); 9] = [
    ("␣n", "step over", &[View::Edit]),
    ("␣i", "step into", &[View::Edit]),
    ("␣o", "step out", &[View::Edit]),
    ("␣c", "continue / pause", &[View::Edit]),
    ("␣q", "stop debugging", &[View::Edit]),
    ("␣s", "switch the Strip's group", &[View::Edit]),
    ("␣h", "hide / show the Program output", &[View::Edit]),
    ("␣e C-Enter", "evaluate", &[View::Edit]),
    ("␣a", "ask the AI about the pause", &[View::Edit]),
];

pub const EVALUATOR_CHORDS: [(&str, &str, &[View]); 2] = [
    ("␣m", "move the Evaluator (hjkl)", &[View::Edit]),
    ("␣z", "resize the Evaluator (hjkl)", &[View::Edit]),
];

pub const DEBUG_KEYS: [(&str, &str, &[View]); 6] = [
    ("F9", "continue / pause", &[View::Edit]),
    ("F8", "step over", &[View::Edit]),
    ("F7", "step into", &[View::Edit]),
    ("S-F8", "step out", &[View::Edit]),
    ("C-F8", "toggle breakpoint", &[View::Edit]),
    ("C-F2", "stop debugging", &[View::Edit]),
];

fn chords(
    state: &State,
) -> impl Iterator<Item = &'static (&'static str, &'static str, &'static [View])> {
    let debug: &'static [(&str, &str, &[View])] = match state.debug {
        Some(_) => &DEBUG_CHORDS,
        None => &[],
    };
    let evaluator: &'static [(&str, &str, &[View])] = match arranging_offered(state) {
        true => &EVALUATOR_CHORDS,
        false => &[],
    };
    debug.iter().chain(evaluator.iter()).chain(CHORDS.iter())
}

pub fn cheatsheet(
    state: &State,
) -> impl Iterator<Item = &'static (&'static str, &'static str, &'static [View])> {
    let debug: &'static [(&str, &str, &[View])] = match state.debug {
        Some(_) => &DEBUG_KEYS,
        None => &[],
    };
    debug.iter().chain(CHEATSHEET.iter()).chain(chords(state))
}

pub fn cheatsheet_rows(state: &State) -> Vec<(&'static str, &'static str)> {
    if state.view == View::Review && state.diff.is_none() {
        return vec![];
    }
    cheatsheet(state)
        .filter(|(_, _, views)| applies_to(views, state.view))
        .map(|(keys, what, _)| (*keys, *what))
        .collect()
}

pub fn chord_rows(state: &State) -> Vec<(Option<char>, String)> {
    let mut rows: Vec<(Option<char>, String)> = chords(state)
        .filter_map(|(keys, what, _)| {
            let key = keys
                .split_whitespace()
                .find_map(|token| token.strip_prefix('\u{2423}'))?
                .chars()
                .next()?;
            Some((Some(key), format!("   ({key}) {what}")))
        })
        .collect();
    rows.push((None, "   Esc  cancel".to_string()));
    rows
}

pub const TOOL_LIST_KEYS: [(&str, &str); 3] =
    [("i", "install"), ("r", "re-check"), ("Esc", "close")];

pub const BRANCH_LIST_KEYS: [(&str, &str); 2] = [("Enter", "story this branch"), ("Esc", "close")];

pub const LAUNCH_LIST_KEYS: [(&str, &str); 2] = [("Enter", "start"), ("Esc", "close")];

pub const BRANCH_FILTER_HINT: &str = "type to filter";

pub const COMMENT_BOX_KEYS: [(&str, &str); 4] = [
    ("C-s", "file"),
    ("Esc", "discard"),
    ("C-z", "undo"),
    ("C-S-z", "redo"),
];

pub const REPLACE_BOX_KEYS: [(&str, &str); 3] =
    [("Tab", "next"), ("Enter", "replace"), ("Esc", "close")];

pub const SEARCH_KEYS: [(&str, &str); 6] = [
    ("arr", "hit"),
    ("C-n C-p", "file"),
    ("Enter", "open"),
    ("C-o", "open all"),
    ("Tab", "complete"),
    ("Esc", "close"),
];

pub fn applies_to(views: &[View], view: View) -> bool {
    views.contains(&view)
}

pub fn on_key_event(state: &State, drafts: &mut Drafts, event: KeyEvent, at_ms: u64) -> Vec<Event> {
    if let Some(events) = reserved(state, drafts, event, at_ms) {
        return events;
    }
    let event = shifted(event);
    let leaving = match state.stepping {
        false => None,
        true => match stepping_key(event) {
            Some(stepped) => return vec![stepped],
            None => Some(Event::LeaveStepping),
        },
    };
    let leaving_arrange = match state.arranging {
        None => None,
        Some(how) => match arranging_key(event, how) {
            Some(arranged) => return vec![arranged],
            None => Some(Event::LeaveArranging),
        },
    };
    let mut events = match claimed_everywhere(state, event) {
        Some(events) => events,
        None => modal_key(state, drafts, event),
    };
    for leave in leaving_arrange.into_iter().chain(leaving) {
        events.insert(0, leave);
    }
    events
}

fn stepping_key(event: KeyEvent) -> Option<Event> {
    if !event.modifiers.is_empty() {
        return None;
    }
    stepping_letter(typed(event)?)
}

pub fn chord(state: &State, key: char) -> Option<Event> {
    match key {
        'b' => Some(Event::ToggleBreakpoint(
            crate::current_buffer(state).map_or(0, |buffer| buffer.line),
        )),
        'B' => Some(Event::EditBreakpoint(
            crate::current_buffer(state).map_or(0, |buffer| buffer.line),
        )),
        'x' => Some(Event::OfferRun(
            crate::current_buffer(state).map_or(0, |buffer| buffer.line),
        )),
        'q' => Some(Event::DebugStop),
        'e' if state.debug.is_some() => Some(Event::OpenEvaluator),
        'm' if arranging_offered(state) => {
            Some(Event::ArrangeEvaluator(crate::debug::Arrange::Moving))
        }
        'z' if arranging_offered(state) => {
            Some(Event::ArrangeEvaluator(crate::debug::Arrange::Sizing))
        }
        'r' => Some(Event::DebugRestart),
        'h' if state.debug.is_some() => Some(Event::ToggleOutput),
        'a' if state.debug.is_some() => Some(Event::AskAboutPause),
        's' if state.debug.is_some() => Some(Event::ShowGroup(match state.strip {
            crate::layout::Group::Shells => crate::layout::Group::Debug,
            crate::layout::Group::Debug => crate::layout::Group::Shells,
        })),
        letter => stepping_letter(letter),
    }
}

fn arranging_offered(state: &State) -> bool {
    state.evaluator.is_some() && state.focus == Pane::Evaluator
}

fn arranging_key(event: KeyEvent, how: crate::debug::Arrange) -> Option<Event> {
    if !event.modifiers.is_empty() {
        return None;
    }
    let direction = arrow(event.code).or_else(|| match typed(event) {
        Some('h') => Some(Direction::Left),
        Some('j') => Some(Direction::Down),
        Some('k') => Some(Direction::Up),
        Some('l') => Some(Direction::Right),
        _ => None,
    })?;
    Some(match how {
        crate::debug::Arrange::Moving => Event::MoveEvaluator(direction),
        crate::debug::Arrange::Sizing => Event::ResizeEvaluator(direction),
    })
}

fn stepping_letter(key: char) -> Option<Event> {
    match key {
        'n' => Some(Event::DebugStep(crate::debug::Step::Over)),
        'i' => Some(Event::DebugStep(crate::debug::Step::Into)),
        'o' => Some(Event::DebugStep(crate::debug::Step::Out)),
        'c' => Some(Event::DebugResume),
        _ => None,
    }
}

fn reserved(state: &State, drafts: &mut Drafts, event: KeyEvent, at_ms: u64) -> Option<Vec<Event>> {
    if let KeyCode::Modifier(ModifierKeyCode::Control, _) = event.code {
        return Some(vec![Event::Tapped {
            key: Tap::Ctrl,
            at_ms,
        }]);
    }
    if event.modifiers.contains(KeyModifiers::CTRL) && event.code == KeyCode::Char(' ') {
        return Some(vec![Event::FallbackBinding]);
    }
    if state.debug.is_some() {
        let ctrl = event.modifiers.contains(KeyModifiers::CTRL);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        let stepped = |step| Some(vec![Event::DebugStep(step)]);
        match event.code {
            KeyCode::F(9) if !ctrl => return Some(vec![Event::DebugResume]),
            KeyCode::F(2) if ctrl => return Some(vec![Event::DebugStop]),
            KeyCode::F(8) if ctrl => {
                let line = crate::current_buffer(state).map_or(0, |buffer| buffer.line);
                return Some(vec![Event::ToggleBreakpoint(line)]);
            }
            KeyCode::F(8) if !ctrl && shift => return stepped(crate::debug::Step::Out),
            KeyCode::F(8) if !ctrl => return stepped(crate::debug::Step::Over),
            KeyCode::F(7) if !ctrl => return stepped(crate::debug::Step::Into),
            _ => {}
        }
    }
    if child_owns_keys(state, drafts) {
        let mut events = to_child(state, event, at_ms);
        if state.stepping {
            events.insert(0, Event::LeaveStepping);
        }
        return Some(events);
    }
    None
}

fn claimed_everywhere(state: &State, event: KeyEvent) -> Option<Vec<Event>> {
    if event.modifiers.contains(KeyModifiers::SUPER) && event.code == KeyCode::Char('f') {
        return Some(vec![Event::OpenSearch]);
    }
    if event.modifiers.contains(KeyModifiers::CTRL) {
        match event.code {
            KeyCode::Char('q') => return Some(vec![Event::Quit]),
            KeyCode::Char('f') => return Some(vec![Event::OpenSearch]),
            KeyCode::F(5) => return Some(vec![Event::DebugRestart]),
            _ => {}
        }
    }
    state.search.as_ref()?;
    Some(searching(event))
}

fn modal_key(state: &State, drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match &state.modal {
        Modal::Palette => match event.code {
            KeyCode::Esc => vec![Event::Cancel],
            _ => match typed(event) {
                Some(c) => vec![Event::Key(c)],
                None => vec![],
            },
        },
        Modal::Tools { .. } => match event.code {
            KeyCode::Esc => vec![Event::Cancel],
            KeyCode::Up => vec![Event::MoveToolRow(Direction::Up)],
            KeyCode::Down => vec![Event::MoveToolRow(Direction::Down)],
            _ => match typed(event) {
                Some('i') => vec![Event::InstallTool],
                Some('r') => vec![Event::RecheckTool],
                _ => vec![],
            },
        },
        Modal::Launches { row } => match event.code {
            KeyCode::Esc => vec![Event::Cancel],
            KeyCode::Up => vec![Event::MoveLaunchRow(Direction::Up)],
            KeyCode::Down => vec![Event::MoveLaunchRow(Direction::Down)],
            KeyCode::Enter => crate::debug::launches(state)
                .get(*row)
                .map(|name| Event::StartLaunch(name.to_string()))
                .into_iter()
                .collect(),
            _ => vec![],
        },
        Modal::Branches { filter, .. } => match event.code {
            KeyCode::Esc => vec![Event::Cancel],
            KeyCode::Up => vec![Event::MoveBranchRow(Direction::Up)],
            KeyCode::Down => vec![Event::MoveBranchRow(Direction::Down)],
            KeyCode::Enter => vec![Event::ChooseBranch],
            KeyCode::Backspace => {
                let mut text = filter.clone();
                text.pop();
                vec![Event::FilterBranches(text)]
            }
            _ => match typed(event) {
                Some(c) => vec![Event::FilterBranches(format!("{filter}{c}"))],
                None => vec![],
            },
        },
        Modal::Chord => {
            match typed(event).filter(|_| !event.modifiers.contains(KeyModifiers::SUPER)) {
                Some(c) => vec![Event::Key(c)],
                None => vec![Event::Cancel],
            }
        }
        Modal::Comment => comment_picker(drafts, event),
        Modal::NameBox { .. } | Modal::SetValue | Modal::NewWatch | Modal::ExceptionClass => {
            name_box(drafts, event)
        }
        Modal::Breakpoint { field, draft, .. } => breakpoint_box(*field, draft, event),
        Modal::Candidates(_) => candidate_list(state, drafts, event),
        Modal::Stops { .. } => match event.code {
            KeyCode::Tab => vec![Event::NextStop],
            KeyCode::Esc => vec![Event::Cancel],
            _ => routed(state, drafts, event),
        },
        Modal::ConfirmSubmit
        | Modal::Diverged
        | Modal::StepDetail
        | Modal::ConfirmStory { .. }
        | Modal::Prediction { .. }
        | Modal::Restart
        | Modal::RunMark { .. } => answered(&state.modal, event),
        Modal::None if state.hover.as_ref().is_some_and(|hover| hover.focused) => {
            match (event.code, typed(event)) {
                (KeyCode::Esc, _) => vec![Event::Cancel],
                (_, Some('j')) => vec![Event::ScrollHover(Direction::Down)],
                (_, Some('k')) => vec![Event::ScrollHover(Direction::Up)],
                _ => vec![],
            }
        }
        Modal::None => routed(state, drafts, event),
    }
}

fn candidate_list(state: &State, drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    if !event.modifiers.is_empty() {
        return routed(state, drafts, event);
    }
    match event.code {
        KeyCode::Up => vec![Event::MoveCandidate(Direction::Up)],
        KeyCode::Down => vec![Event::MoveCandidate(Direction::Down)],
        KeyCode::Enter => vec![Event::AcceptCandidate],
        KeyCode::Esc => vec![Event::Cancel],
        _ => routed(state, drafts, event),
    }
}

fn answered(modal: &Modal, event: KeyEvent) -> Vec<Event> {
    match modal {
        Modal::ConfirmSubmit => yes_no(Event::ConfirmSubmit, event),
        Modal::Diverged => diverged_answer(event),
        Modal::StepDetail => step_detail_answer(event),
        Modal::ConfirmStory { .. } => yes_no(Event::ConfirmStory, event),
        Modal::Prediction { .. } => prediction_answer(event),
        Modal::Restart => yes_no(Event::Restart, event),
        Modal::RunMark { .. } => run_mark_answer(event),
        Modal::None
        | Modal::NameBox { .. }
        | Modal::SetValue
        | Modal::NewWatch
        | Modal::ExceptionClass
        | Modal::Breakpoint { .. }
        | Modal::Palette
        | Modal::Chord
        | Modal::Tools { .. }
        | Modal::Launches { .. }
        | Modal::Branches { .. }
        | Modal::Comment
        | Modal::Candidates(_)
        | Modal::Stops { .. } => {
            unreachable!("{modal:?} is not answered")
        }
    }
}

fn yes_no(yes: Event, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Enter => vec![yes],
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some('y' | 'Y') => vec![yes],
            Some('n' | 'N') => vec![Event::Cancel],
            _ => vec![],
        },
    }
}

fn diverged_answer(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some('r') => vec![Event::Resolve(Resolution::Reload)],
            Some('w') => vec![Event::Resolve(Resolution::Overwrite)],
            Some('m') => vec![Event::Resolve(Resolution::Merge)],
            _ => vec![],
        },
    }
}

fn step_detail_answer(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some('D') => vec![Event::Key('D')],
            _ => vec![],
        },
    }
}

fn run_mark_answer(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some('r') => vec![Event::ChooseRun(crate::run::RUN)],
            Some('d') => vec![Event::ChooseRun(crate::run::DEBUG)],
            _ => vec![],
        },
    }
}

fn prediction_answer(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some(c @ ('n' | 'p' | '1' | '2' | '3')) => vec![Event::Key(c)],
            _ => vec![],
        },
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Paste {
    #[default]
    Bare,
    Bracketed,
}

pub enum Pasted {
    ToChild(Event),
    ToBuffer(Event),
    AsKeys(Vec<KeyEvent>),
}

pub fn on_paste(state: &State, drafts: &Drafts, text: String) -> Pasted {
    if child_owns_keys(state, drafts) {
        return Pasted::ToChild(Event::Pasted(text));
    }
    if buffer_takes_paste(state, drafts) {
        return Pasted::ToBuffer(Event::EditorPaste(
            text.replace("\r\n", "\n").replace('\r', "\n"),
        ));
    }
    Pasted::AsKeys(
        text.replace("\r\n", "\n")
            .chars()
            .map(|c| {
                KeyEvent::new(match c {
                    '\n' | '\r' => KeyCode::Enter,
                    '\t' => KeyCode::Tab,
                    _ => KeyCode::Char(c),
                })
            })
            .collect(),
    )
}

fn child_owns_keys(state: &State, drafts: &Drafts) -> bool {
    !a_box_has_the_keys(state, drafts)
        && match state.focus {
            Pane::Terminal => true,
            Pane::Ai => state.ai_running,
            Pane::Output => state.output_running,
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
            | Pane::Cheatsheet => false,
        }
}

fn a_box_has_the_keys(state: &State, drafts: &Drafts) -> bool {
    !matches!(state.modal, Modal::None)
        || state.search.is_some()
        || state
            .find
            .as_ref()
            .is_some_and(|find| find.keys != FindKeys::Away)
        || drafts.command.is_some()
        || drafts.filter.is_some()
}

fn buffer_takes_paste(state: &State, drafts: &Drafts) -> bool {
    if matches!(state.modal, Modal::Comment) && !drafts.comment_kind.is_empty() {
        return true;
    }
    !a_box_has_the_keys(state, drafts) && the_buffer_takes_edits(state)
}

fn the_buffer_takes_edits(state: &State) -> bool {
    state.focus == Pane::Evaluator
        || (state.focus == Pane::Editor
            && state.diff.is_none()
            && state.walking.is_none()
            && !crate::previewing(state))
}

fn typing_into_the_buffer(state: &State) -> bool {
    the_buffer_takes_edits(state) && crate::editor_inserting(state)
}

fn to_child(state: &State, event: KeyEvent, at_ms: u64) -> Vec<Event> {
    if event.modifiers == KeyModifiers::ALT {
        if let KeyCode::Char(letter @ ('h' | 'j' | 'k' | 'l')) = event.code {
            return vec![Event::MoveFocus(match letter {
                'h' => Direction::Left,
                'l' => Direction::Right,
                'k' => Direction::Up,
                _ => Direction::Down,
            })];
        }
    }
    if (event.modifiers == KeyModifiers::CTRL || event.modifiers == KeyModifiers::SUPER)
        && event.code == KeyCode::Char('c')
        && matches!(state.selection, Some(Selection::Screen { pane, .. }) if pane == state.focus)
    {
        return vec![Event::Copy];
    }
    let mut events = match encode(event) {
        Some(bytes) => vec![Event::Bytes(bytes)],
        None => vec![],
    };
    if event.code == KeyCode::Esc && event.modifiers.is_empty() && event.kind == KeyEventKind::Press
    {
        events.push(Event::Tapped {
            key: Tap::Escape,
            at_ms,
        });
    }
    events
}

fn encode(event: KeyEvent) -> Option<Vec<u8>> {
    // Legacy xterm has no repeat; Kitty-protocol repeats must encode as presses or they send nothing
    let mut event = KeyEvent {
        kind: KeyEventKind::Press,
        ..event
    };
    // The encoder refuses shifted Ctrl letters; a real terminal sends one control code for every case
    if let (true, KeyCode::Char(c)) = (event.modifiers.contains(KeyModifiers::CTRL), event.code) {
        event.modifiers.remove(KeyModifiers::SHIFT);
        event.code = KeyCode::Char(c.to_ascii_lowercase());
    }
    // terminput encodes F13+ as a bare CSI introducer, which swallows whatever the child reads next
    if matches!(event.code, KeyCode::F(13..)) {
        return None;
    }
    let mut buf = [0u8; 32];
    let written = Input::Key(event).encode(&mut buf, Encoding::Xterm).ok()?;
    (written > 0).then(|| buf[..written].to_vec())
}

/// Some terminals report the base key plus Shift rather than the shifted character
fn shifted(mut event: KeyEvent) -> KeyEvent {
    if !event.modifiers.contains(KeyModifiers::SHIFT) {
        return event;
    }
    if let KeyCode::Char(c) = event.code {
        event.code = KeyCode::Char(match c {
            ';' => ':',
            '\'' => '"',
            '1' => '!',
            '/' => '?',
            '.' => '>',
            ',' => '<',
            other => other.to_ascii_uppercase(),
        });
    }
    event
}

fn typed(event: KeyEvent) -> Option<char> {
    match event.code {
        KeyCode::Char(c)
            if !event
                .modifiers
                .intersects(KeyModifiers::CTRL | KeyModifiers::ALT) =>
        {
            Some(c)
        }
        _ => None,
    }
}

fn arrow(code: KeyCode) -> Option<Direction> {
    Some(match code {
        KeyCode::Left => Direction::Left,
        KeyCode::Right => Direction::Right,
        KeyCode::Up => Direction::Up,
        KeyCode::Down => Direction::Down,
        _ => return None,
    })
}

fn searching(event: KeyEvent) -> Vec<Event> {
    if let Some(events) = query_key(event) {
        return events;
    }
    if let Some(direction @ (Direction::Up | Direction::Down)) = arrow(event.code) {
        if event
            .modifiers
            .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT)
        {
            return vec![];
        }
        return vec![Event::MoveHit(direction)];
    }
    match event.code {
        KeyCode::Esc => vec![Event::CloseSearch],
        KeyCode::Enter => vec![Event::OpenHit],
        KeyCode::Char('o') if event.modifiers.contains(KeyModifiers::CTRL) => {
            vec![Event::OpenEveryHit]
        }
        KeyCode::Char('n') if event.modifiers.contains(KeyModifiers::CTRL) => {
            vec![Event::MoveHitFile(Direction::Down)]
        }
        KeyCode::Char('p') if event.modifiers.contains(KeyModifiers::CTRL) => {
            vec![Event::MoveHitFile(Direction::Up)]
        }
        KeyCode::Tab => vec![Event::CompleteSearch],
        _ => vec![],
    }
}

fn finding(find: &Find, event: KeyEvent) -> Vec<Event> {
    let keys = |keys| vec![Event::FindKeys(keys)];
    let back = event.modifiers.contains(KeyModifiers::SHIFT);
    match find.keys {
        FindKeys::Away => vec![],
        FindKeys::Query => {
            let at_end = find.query.column > find.query.shown().chars().count();
            match event.code {
                KeyCode::Esc => vec![Event::CloseFind],
                KeyCode::Enter => vec![Event::AcceptFind],
                KeyCode::Tab => keys(FindKeys::Icon(FindIcon::Case)),
                KeyCode::Right if at_end && event.modifiers.is_empty() => {
                    keys(FindKeys::Icon(FindIcon::Case))
                }
                _ => query_key(event).unwrap_or_default(),
            }
        }
        FindKeys::Icon(icon) => {
            let at = FIND_ICONS.iter().position(|(each, _)| *each == icon);
            let at = at.unwrap_or(0);
            match event.code {
                KeyCode::Left if at == 0 => vec![
                    Event::FindKeys(FindKeys::Query),
                    Event::QueryEnd(Direction::Right),
                ],
                KeyCode::Left => keys(FindKeys::Icon(FIND_ICONS[at - 1].0)),
                KeyCode::Right => keys(FindKeys::Icon(
                    FIND_ICONS[(at + 1).min(FIND_ICONS.len() - 1)].0,
                )),
                KeyCode::Up | KeyCode::Esc => keys(FindKeys::Query),
                KeyCode::Enter => match icon {
                    FindIcon::Case => vec![Event::ToggleCase],
                    FindIcon::Replace | FindIcon::ReplaceAll => {
                        keys(FindKeys::Replace(ReplaceField::With))
                    }
                },
                _ => vec![],
            }
        }
        FindKeys::Replace(field) => {
            let at = REPLACE_FIELDS.iter().position(|each| *each == field);
            let at = at.unwrap_or(0);
            let count = REPLACE_FIELDS.len();
            match event.code {
                KeyCode::Esc => keys(FindKeys::Away),
                KeyCode::Tab if back => {
                    keys(FindKeys::Replace(REPLACE_FIELDS[(at + count - 1) % count]))
                }
                KeyCode::Tab => keys(FindKeys::Replace(REPLACE_FIELDS[(at + 1) % count])),
                KeyCode::Enter if field == ReplaceField::ReplaceAll => vec![Event::ReplaceAll],
                KeyCode::Enter => vec![Event::ReplaceMatch],
                _ if matches!(field, ReplaceField::Find | ReplaceField::With) => {
                    query_key(event).unwrap_or_default()
                }
                _ => match typed(event) {
                    Some('n') => vec![Event::StepMatch(Direction::Right)],
                    Some('N') => vec![Event::StepMatch(Direction::Left)],
                    _ => vec![],
                },
            }
        }
    }
}

fn query_key(event: KeyEvent) -> Option<Vec<Event>> {
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let ctrl = event.modifiers.contains(KeyModifiers::CTRL);
    let word = |direction| Some(vec![Event::EditorWord(direction)]);
    match event.code {
        KeyCode::Char('b') if alt && !ctrl => word(Direction::Left),
        KeyCode::Char('f') if alt && !ctrl => word(Direction::Right),
        KeyCode::Left if alt => word(Direction::Left),
        KeyCode::Right if alt => word(Direction::Right),
        KeyCode::Left => Some(vec![Event::EditorArrow(Direction::Left)]),
        KeyCode::Right => Some(vec![Event::EditorArrow(Direction::Right)]),
        KeyCode::Home => Some(vec![Event::QueryEnd(Direction::Left)]),
        KeyCode::End => Some(vec![Event::QueryEnd(Direction::Right)]),
        KeyCode::Backspace if alt => Some(vec![Event::EditorDeleteWord]),
        KeyCode::Backspace => Some(vec![Event::EditorBackspace]),
        _ => typed(event).map(|c| vec![Event::EditorKey(c)]),
    }
}

fn comment_picker(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    if drafts.comment_kind.is_empty() {
        return comment_type(drafts, event);
    }
    comment_body(drafts, event)
}

fn comment_type(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::EditorEscape],
        _ => {
            if let Some(kind) = typed(event).and_then(comment_kind) {
                drafts.comment_kind = kind.to_string();
            }
            vec![]
        }
    }
}

fn comment_body(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    if event.modifiers.contains(KeyModifiers::CTRL) && event.code == KeyCode::Char('s') {
        return vec![Event::FileComment {
            kind: std::mem::take(&mut drafts.comment_kind),
        }];
    }
    if let Some(undo) = undo_key(event) {
        return vec![undo];
    }
    // macOS Option+arrow arrives as the readline escapes ^[b and ^[f, not as Alt+arrow
    if let (true, false, KeyCode::Char(letter @ ('b' | 'f'))) = (
        alt,
        event.modifiers.contains(KeyModifiers::CTRL),
        event.code,
    ) {
        return vec![Event::EditorWord(if letter == 'f' {
            Direction::Right
        } else {
            Direction::Left
        })];
    }
    match event.code {
        KeyCode::Esc => {
            drafts.comment_kind.clear();
            vec![Event::EditorEscape]
        }
        KeyCode::Enter => vec![Event::EditorKey('\n')],
        KeyCode::Backspace if alt => vec![Event::EditorDeleteWord],
        KeyCode::Backspace => vec![Event::EditorBackspace],
        KeyCode::Left | KeyCode::Right if alt => {
            arrow(event.code).map_or(vec![], |direction| vec![Event::EditorWord(direction)])
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
            arrow(event.code).map_or(vec![], |direction| vec![Event::EditorArrow(direction)])
        }
        _ => match typed(event) {
            Some(c) => vec![Event::EditorKey(c)],
            None => vec![],
        },
    }
}

fn comment_kind(key: char) -> Option<&'static str> {
    match key {
        'i' => Some("ISSUE"),
        'n' => Some("NOTE"),
        's' => Some("SUGGESTION"),
        'c' => Some("COMMENT"),
        _ => None,
    }
}

fn breakpoint_box(field: debug::Field, draft: &debug::Properties, event: KeyEvent) -> Vec<Event> {
    let walk = |by: usize| {
        let at = debug::FIELDS
            .iter()
            .position(|row| *row == field)
            .unwrap_or(0);
        debug::FIELDS[(at + by) % debug::FIELDS.len()]
    };
    let back = event.modifiers.contains(KeyModifiers::SHIFT);
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        KeyCode::Enter => vec![Event::ConfirmBreakpoint],
        KeyCode::Tab if back => vec![Event::BreakpointField(walk(debug::FIELDS.len() - 1))],
        KeyCode::Up => vec![Event::BreakpointField(walk(debug::FIELDS.len() - 1))],
        KeyCode::Tab | KeyCode::Down => vec![Event::BreakpointField(walk(1))],
        _ if field == debug::Field::Suspend => match typed(event) {
            Some(' ') => vec![Event::SwitchSuspend],
            _ => vec![],
        },
        KeyCode::Backspace => {
            let mut text = debug::field_text(draft, field).to_string();
            text.pop();
            vec![Event::BreakpointDraft(text)]
        }
        _ => match typed(event) {
            Some(c) => vec![Event::BreakpointDraft(format!(
                "{}{c}",
                debug::field_text(draft, field)
            ))],
            None => vec![],
        },
    }
}

fn name_box(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => {
            drafts.name.clear();
            vec![Event::Cancel]
        }
        KeyCode::Enter => vec![Event::EnterName(std::mem::take(&mut drafts.name))],
        KeyCode::Backspace => {
            drafts.name.pop();
            vec![]
        }
        _ => {
            if let Some(c) = typed(event) {
                drafts.name.push(c);
            }
            vec![]
        }
    }
}

fn routed(state: &State, drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);
    if let Some(events) = word_motion_alias(state, event, alt, shift) {
        return events;
    }
    if let Some(events) = focus_alias(event, alt) {
        return events;
    }
    if let Some(events) = collecting(state, drafts, event) {
        return events;
    }
    if let Some(events) = jump_alias(event) {
        return events;
    }
    if let Some(events) = occurrence_alias(state, event) {
        return events;
    }
    if let Some(events) = arrow_event(state, event, alt, shift) {
        return events;
    }
    if let Some(events) = tab_indent(state, event) {
        return events;
    }
    if let Some(events) = backspace_word(state, event) {
        return events;
    }
    pane_key(state, event)
}

fn jump_alias(event: KeyEvent) -> Option<Vec<Event>> {
    if event
        .modifiers
        .contains(KeyModifiers::ALT | KeyModifiers::CTRL)
    {
        match event.code {
            KeyCode::Left => return Some(vec![Event::JumpBack]),
            KeyCode::Right => return Some(vec![Event::JumpForward]),
            _ => {}
        }
    }
    if !event.modifiers.contains(KeyModifiers::CTRL) {
        return None;
    }
    match event.code {
        KeyCode::Char('p') => Some(vec![Event::JumpBack]),
        KeyCode::Char('n') => Some(vec![Event::JumpForward]),
        _ => None,
    }
}

fn occurrence_alias(state: &State, event: KeyEvent) -> Option<Vec<Event>> {
    (matches!(state.focus, Pane::Editor | Pane::Evaluator)
        && state.diff.is_none()
        && state.walking.is_none()
        && ctrl_or_command(event)
        && event.code == KeyCode::Char('d'))
    .then(|| vec![Event::EditorNextOccurrence])
}

fn backspace_word(state: &State, event: KeyEvent) -> Option<Vec<Event>> {
    (event.code == KeyCode::Backspace
        && event.modifiers.contains(KeyModifiers::ALT)
        && typing_into_the_buffer(state))
    .then(|| vec![Event::EditorDeleteWord])
}

fn tab_indent(state: &State, event: KeyEvent) -> Option<Vec<Event>> {
    if !(event.code == KeyCode::Tab && typing_into_the_buffer(state)) {
        return None;
    }
    if matches!(state.selection, Some(Selection::Buffer { .. })) {
        return Some(vec![Event::EditorIndent(
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                Direction::Left
            } else {
                Direction::Right
            },
        )]);
    }
    let indent = " ".repeat(state.tab_width);
    (!indent.is_empty()).then(|| vec![Event::EditorPaste(indent)])
}

fn word_motion_alias(state: &State, event: KeyEvent, alt: bool, shift: bool) -> Option<Vec<Event>> {
    if !(matches!(state.focus, Pane::Editor | Pane::Evaluator) && alt) {
        return None;
    }
    match (arrow(event.code), shift) {
        (Some(_), _) if event.modifiers.contains(KeyModifiers::CTRL) => return None,
        (Some(direction @ (Direction::Left | Direction::Right)), true) => {
            return Some(vec![Event::EditorExtendWord(direction)]);
        }
        (Some(direction @ (Direction::Left | Direction::Right)), false) => {
            return Some(vec![Event::EditorWord(direction)]);
        }
        _ => {}
    }
    // macOS Option+arrow arrives as the readline escapes ^[b and ^[f, not as Alt+arrow
    let (KeyCode::Char(letter @ ('b' | 'f')), false) =
        (event.code, event.modifiers.contains(KeyModifiers::CTRL))
    else {
        return None;
    };
    let direction = if letter == 'f' {
        Direction::Right
    } else {
        Direction::Left
    };
    Some(vec![Event::EditorWord(direction)])
}

fn focus_alias(event: KeyEvent, alt: bool) -> Option<Vec<Event>> {
    if !(alt && !event.modifiers.contains(KeyModifiers::CTRL)) {
        return None;
    }
    let KeyCode::Char(letter @ ('h' | 'j' | 'k' | 'l')) = event.code else {
        return None;
    };
    Some(vec![Event::MoveFocus(match letter {
        'h' => Direction::Left,
        'l' => Direction::Right,
        'k' => Direction::Up,
        _ => Direction::Down,
    })])
}

fn collecting(state: &State, drafts: &mut Drafts, event: KeyEvent) -> Option<Vec<Event>> {
    if let Some(find) = state
        .find
        .as_ref()
        .filter(|find| find.keys != FindKeys::Away)
    {
        return Some(finding(find, event));
    }
    if drafts.command.is_some() {
        return Some(command_line(drafts, event));
    }
    if typed(event) == Some(':') && claims_colon(state) {
        drafts.command = Some(String::new());
        return Some(vec![]);
    }
    if state.focus == Pane::Ai && !state.ai_running {
        return Some(start_ai_box(drafts, event));
    }
    if drafts.filter.is_some() {
        return Some(filter_box(drafts, event));
    }
    if state.focus == Pane::Tree && tree::filter_rows(state.view) > 0 && typed(event) == Some('/') {
        drafts.filter = Some(String::new());
        return Some(vec![]);
    }
    None
}

fn claims_colon(state: &State) -> bool {
    match state.focus {
        Pane::Tree => true,
        Pane::Editor | Pane::Evaluator => !crate::editor_inserting(state),
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts
        | Pane::Variables
        | Pane::Cheatsheet => true,
        Pane::Ai | Pane::Terminal | Pane::Output => false,
    }
}

fn arrow_event(state: &State, event: KeyEvent, alt: bool, shift: bool) -> Option<Vec<Event>> {
    let direction = arrow(event.code)?;
    if alt {
        return Some(vec![]);
    }
    Some(match (state.focus, shift) {
        (Pane::Editor | Pane::Evaluator, false) => vec![Event::EditorArrow(direction)],
        (Pane::Editor | Pane::Evaluator, true) => vec![Event::EditorExtend(direction)],
        (Pane::Tree, false) => list_arrow(direction),
        (Pane::Tree, true) => vec![],
        (Pane::Risk, false) => list_arrow(direction),
        (Pane::Risk, true) => vec![],
        (Pane::Buffers, false) => list_arrow(direction),
        (Pane::Buffers, true) => vec![],
        (Pane::History, false) => list_arrow(direction),
        (Pane::History, true) => vec![],
        (
            Pane::Breakpoints
            | Pane::Frames
            | Pane::Diagnostics
            | Pane::Conflicts
            | Pane::Variables,
            false,
        ) => list_arrow(direction),
        (
            Pane::Breakpoints
            | Pane::Frames
            | Pane::Diagnostics
            | Pane::Conflicts
            | Pane::Variables,
            true,
        ) => vec![],
        (Pane::Cheatsheet, false) => match direction {
            Direction::Up | Direction::Down => {
                vec![Event::ScrollCheatsheet { direction, rows: 1 }]
            }
            Direction::Left | Direction::Right => vec![],
        },
        (Pane::Cheatsheet, true) => vec![],
        (Pane::Ai | Pane::Terminal | Pane::Output, _) => vec![],
    })
}

fn list_arrow(direction: Direction) -> Vec<Event> {
    match direction {
        Direction::Up | Direction::Down => vec![Event::MoveSelection(direction)],
        Direction::Left | Direction::Right => vec![Event::MoveAction(direction)],
    }
}

fn pane_key(state: &State, event: KeyEvent) -> Vec<Event> {
    match state.focus {
        Pane::Evaluator
            if event.code == KeyCode::Enter && event.modifiers.contains(KeyModifiers::CTRL) =>
        {
            vec![Event::RunSnippet]
        }
        Pane::Editor
            if state.view == View::Edit
                && event.code == KeyCode::Char('s')
                && ctrl_or_command(event) =>
        {
            vec![Event::WriteBuffer]
        }
        Pane::Evaluator if event.code == KeyCode::Char('s') && ctrl_or_command(event) => vec![],
        Pane::Editor | Pane::Evaluator => editor_pane_key(event),
        Pane::Tree => tree_pane_key(event),
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts
        | Pane::Variables => list_pane_key(event),
        Pane::Cheatsheet => {
            let (direction, rows) = match (event.code, typed(event)) {
                (_, Some('j')) => (Direction::Down, 1),
                (_, Some('k')) => (Direction::Up, 1),
                (KeyCode::PageDown, _) => (Direction::Down, crate::cheatsheet_fits(state)),
                (KeyCode::PageUp, _) => (Direction::Up, crate::cheatsheet_fits(state)),
                _ => return vec![],
            };
            vec![Event::ScrollCheatsheet { direction, rows }]
        }
        Pane::Ai | Pane::Terminal | Pane::Output => vec![],
    }
}

fn ctrl_or_command(event: KeyEvent) -> bool {
    event
        .modifiers
        .intersects(KeyModifiers::CTRL | KeyModifiers::SUPER)
}

fn undo_key(event: KeyEvent) -> Option<Event> {
    match event.code {
        KeyCode::Char('z') if ctrl_or_command(event) => Some(Event::EditorUndo),
        KeyCode::Char('Z') if ctrl_or_command(event) => Some(Event::EditorRedo),
        _ => None,
    }
}

fn editor_pane_key(event: KeyEvent) -> Vec<Event> {
    if let Some(undo) = undo_key(event) {
        return vec![undo];
    }
    match event.code {
        KeyCode::Char('c') if ctrl_or_command(event) => vec![Event::Copy],
        KeyCode::Char('v') if ctrl_or_command(event) => vec![Event::PasteFromClipboard],
        KeyCode::Esc => vec![Event::EditorEscape],
        KeyCode::Home => vec![Event::EditorKey('0')],
        KeyCode::End => vec![Event::EditorKey('$')],
        KeyCode::Backspace => vec![Event::EditorBackspace],
        KeyCode::Enter => vec![Event::EditorKey('\n')],
        _ => match typed(event) {
            Some(c) => vec![Event::EditorKey(c)],
            None => vec![],
        },
    }
}

fn tree_pane_key(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        KeyCode::Enter => vec![Event::Activate],
        _ => match typed(event) {
            Some(shortcut) => vec![Event::Key(shortcut)],
            None => vec![],
        },
    }
}

fn list_pane_key(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Enter => vec![Event::Activate],
        _ => match typed(event) {
            Some(shortcut) => vec![Event::Key(shortcut)],
            None => vec![],
        },
    }
}

fn command_line(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => {
            drafts.command = None;
            vec![]
        }
        KeyCode::Backspace => {
            if let Some(draft) = drafts.command.as_mut() {
                draft.pop();
            }
            vec![]
        }
        KeyCode::Enter => command(drafts.command.take().unwrap_or_default().as_str()),
        _ => {
            if let (Some(c), Some(draft)) = (typed(event), drafts.command.as_mut()) {
                draft.push(c);
            }
            vec![]
        }
    }
}

fn command(line: &str) -> Vec<Event> {
    if let Some(events) = buffer_command(line) {
        return events;
    }
    if let Some(events) = view_command(line) {
        return events;
    }
    spawning_command(line)
}

fn buffer_command(line: &str) -> Option<Vec<Event>> {
    Some(match line {
        "w" => vec![Event::WriteBuffer],
        "e" => vec![Event::ReloadBuffer],
        "q" => vec![Event::CloseBuffer { force: false }],
        "q!" => vec![Event::CloseBuffer { force: true }],
        "wq" => vec![Event::WriteBuffer, Event::CloseBuffer { force: false }],
        "qa" => vec![Event::CloseAllBuffers { force: false }],
        "qa!" => vec![Event::CloseAllBuffers { force: true }],
        _ => return None,
    })
}

fn view_command(line: &str) -> Option<Vec<Event>> {
    if let Some(rest) = line.strip_prefix("speed ") {
        return rest
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|speed| speed.is_finite() && *speed > 0.0)
            .map(|speed| vec![Event::SetSpeed(speed)]);
    }
    Some(match line {
        "submit" => vec![Event::SubmitReview],
        "update" => vec![Event::Rebuild],
        "tall" => vec![Event::ToggleTallAi],
        "split" => vec![Event::SplitTerminal],
        "preview" => vec![Event::TogglePreview],
        "format" => vec![Event::FormatBuffer],
        "toggle" => vec![Event::ToggleFold { all: false }],
        "toggle!" => vec![Event::ToggleFold { all: true }],
        "read" => vec![Event::StartReading],
        "pause" => vec![Event::PlayPause],
        "next" => vec![Event::NextUtterance],
        "prev" => vec![Event::PreviousUtterance],
        "stop" => vec![Event::StopReading],
        "help" => vec![Event::ToggleCheatsheet],
        "dim" => vec![Event::ToggleField],
        "minimap" => vec![Event::ToggleMinimap],
        _ => return None,
    })
}

fn spawning_command(line: &str) -> Vec<Event> {
    let argument = || {
        line.split_once(' ')
            .map(|(_, rest)| rest.trim().to_string())
            .filter(|rest| !rest.is_empty())
    };
    if names(line, "ai") {
        return vec![Event::StartAi {
            command: argument(),
            force: line.starts_with("ai!"),
        }];
    }
    if let Some(rest) = line.strip_prefix("story?") {
        let url = rest.trim();
        return vec![Event::PickBranch(
            (!url.is_empty()).then(|| url.to_string()),
        )];
    }
    if names(line, "story") {
        return vec![Event::Story {
            explicit: argument(),
            force: line.starts_with("story!"),
        }];
    }
    vec![]
}

fn names(line: &str, word: &str) -> bool {
    line == word
        || line == format!("{word}!")
        || line.starts_with(&format!("{word} "))
        || line.starts_with(&format!("{word}! "))
}

fn start_ai_box(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Enter => {
            let command = std::mem::take(&mut drafts.ai);
            if command.trim().is_empty() {
                return vec![];
            }
            vec![Event::StartAi {
                command: Some(command.trim().to_string()),
                force: false,
            }]
        }
        KeyCode::Backspace => {
            drafts.ai.pop();
            vec![]
        }
        _ => {
            if let Some(c) = typed(event) {
                drafts.ai.push(c);
            }
            vec![]
        }
    }
}

fn filter_box(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => {
            drafts.filter = None;
            vec![Event::Filter(String::new())]
        }
        KeyCode::Enter => {
            drafts.filter = None;
            vec![Event::AcceptFilter]
        }
        KeyCode::Backspace => {
            let draft = drafts.filter.as_mut().expect("open");
            draft.pop();
            vec![Event::Filter(draft.clone())]
        }
        _ => match typed(event) {
            Some(c) => {
                let draft = drafts.filter.as_mut().expect("open");
                draft.push(c);
                vec![Event::Filter(draft.clone())]
            }
            None => vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{command, on_key_event, on_paste, Drafts, Pasted};
    use crate::{
        story, DiffLine, Direction, Event, Find, FindIcon, FindKeys, Modal, Pane, Place,
        ReplaceField, Selection, State, Tap, View,
    };
    use terminput::{
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MediaKeyCode, ModifierDirection,
        ModifierKeyCode,
    };

    fn focused(pane: Pane) -> State {
        State {
            focus: pane,
            ..State::default()
        }
    }

    #[test]
    fn the_rows_nothing_else_teaches_are_on_screen_before_scrolling() {
        let state = State {
            screen_width: 120,
            screen_height: 26,
            ..State::default()
        };
        let fits = crate::cheatsheet_fits(&state);
        assert_eq!(fits, 16);
        let shown: Vec<&str> = super::cheatsheet_rows(&state)
            .into_iter()
            .take(fits)
            .map(|(keys, _)| keys)
            .collect();
        for keys in ["C-space Esc Esc", ":format"] {
            assert!(
                shown.iter().any(|row| row.starts_with(keys)),
                "{keys:?} is below the fold at 26 rows: {shown:?}"
            );
        }
    }

    #[test]
    fn the_breakpoint_box_types_into_its_rows_and_flips_its_switch() {
        use crate::debug::{Field, Properties};
        let draft = Properties {
            condition: "a".to_string(),
            ..Properties::default()
        };
        let key = |field, event| super::breakpoint_box(field, &draft, event);
        let plain = KeyEvent::new;
        assert_eq!(
            key(Field::Condition, plain(KeyCode::Char(' '))),
            vec![Event::BreakpointDraft("a ".to_string())]
        );
        assert_eq!(
            key(Field::Condition, plain(KeyCode::Backspace)),
            vec![Event::BreakpointDraft(String::new())]
        );
        assert_eq!(
            key(Field::Suspend, plain(KeyCode::Char(' '))),
            vec![Event::SwitchSuspend]
        );
        assert_eq!(key(Field::Suspend, plain(KeyCode::Char('x'))), vec![]);
        assert_eq!(
            key(Field::Suspend, plain(KeyCode::Tab)),
            vec![Event::BreakpointField(Field::Condition)]
        );
        assert_eq!(
            key(Field::Condition, plain(KeyCode::Up)),
            vec![Event::BreakpointField(Field::Suspend)]
        );
        assert_eq!(
            key(
                Field::HitCount,
                plain(KeyCode::Tab).modifiers(KeyModifiers::SHIFT)
            ),
            vec![Event::BreakpointField(Field::Condition)]
        );
    }

    #[test]
    fn a_paste_reaches_a_child_whole_and_varde_s_own_panes_as_keystrokes() {
        let hosted = State {
            ai_running: true,
            ..focused(Pane::Ai)
        };
        assert!(matches!(
            on_paste(&hosted, &Drafts::default(), "one\ntwo".to_string()),
            Pasted::ToChild(Event::Pasted(text)) if text == "one\ntwo"
        ));
        let Pasted::AsKeys(keys) = on_paste(
            &focused(Pane::Tree),
            &Drafts::default(),
            "a\r\nb\t".to_string(),
        ) else {
            panic!("Varde's own panes interpret their keys");
        };
        assert_eq!(
            keys.iter().map(|key| key.code).collect::<Vec<_>>(),
            vec![
                KeyCode::Char('a'),
                KeyCode::Enter,
                KeyCode::Char('b'),
                KeyCode::Tab
            ]
        );
    }

    #[test]
    fn undo_and_redo_have_a_modifier_spelling_in_either_mode() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        let shift = |event: KeyEvent| event.modifiers(event.modifiers | KeyModifiers::SHIFT);
        for state in [editing(), inserting.clone()] {
            for undo in [ctrl('z'), cmd('z')] {
                assert_eq!(press(&state, undo), vec![Event::EditorUndo]);
                assert_eq!(press(&state, shift(undo)), vec![Event::EditorRedo]);
            }
        }
        assert_eq!(press(&editing(), plain('U')), vec![Event::EditorKey('U')]);
        assert_eq!(press(&inserting, plain('u')), vec![Event::EditorKey('u')]);
    }

    #[test]
    fn a_paste_into_the_open_buffer_is_one_edit_whatever_mode_it_is_in() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        for state in [inserting.clone(), editing()] {
            assert!(matches!(
                on_paste(&state, &Drafts::default(), "foo('bar".to_string()),
                Pasted::ToBuffer(Event::EditorPaste(text)) if text == "foo('bar"
            ));
        }
        assert!(matches!(
            on_paste(&inserting, &Drafts::default(), "a\r\nb\rc".to_string()),
            Pasted::ToBuffer(Event::EditorPaste(text)) if text == "a\nb\nc"
        ));
        let elsewhere = State {
            view: View::Story,
            ..inserting.clone()
        };
        assert!(matches!(
            on_paste(&elsewhere, &Drafts::default(), "foo('bar".to_string()),
            Pasted::ToBuffer(_)
        ));
    }

    #[test]
    fn tab_indents_only_where_a_typed_character_would_reach_the_buffer() {
        let tab = KeyEvent::new(KeyCode::Tab);
        let normal = State {
            tab_width: 2,
            ..editing()
        };
        assert_eq!(press(&normal, tab), vec![]);
        let inserting = crate::update(&normal, Event::EditorKey('i')).0;
        assert_eq!(
            press(&inserting, tab),
            vec![Event::EditorPaste("  ".to_string())]
        );
        for claimed in [
            State {
                diff: Some(vec![]),
                ..inserting.clone()
            },
            State {
                walking: Some(story::Walking::Story {
                    story: 0,
                    step: 0,
                    diff: story::Diff::Hidden,
                }),
                ..inserting.clone()
            },
        ] {
            assert_eq!(press(&claimed, tab), vec![]);
        }
        assert_eq!(
            press(&filling_in_a_snippet(&inserting), tab),
            vec![Event::NextStop]
        );
        let offering = State {
            modal: offering_candidates().modal,
            ..inserting.clone()
        };
        assert_eq!(
            press(&offering, tab),
            vec![Event::EditorPaste("  ".to_string())]
        );
        let mut picked = Drafts {
            comment_kind: "NOTE".to_string(),
            ..Drafts::default()
        };
        let box_up = State {
            modal: crate::Modal::Comment,
            ..inserting.clone()
        };
        assert_eq!(on_key_event(&box_up, &mut picked, tab, 0), vec![]);
        let zero = State {
            tab_width: 0,
            ..inserting
        };
        assert_eq!(press(&zero, tab), vec![]);
    }

    #[test]
    fn alt_backspace_takes_a_word_only_where_a_typed_character_would_reach_the_buffer() {
        let alt_backspace = KeyEvent::new(KeyCode::Backspace).modifiers(KeyModifiers::ALT);
        let normal = editing();
        assert_eq!(press(&normal, alt_backspace), vec![Event::EditorBackspace]);
        let inserting = crate::update(&normal, Event::EditorKey('i')).0;
        assert_eq!(
            press(&inserting, alt_backspace),
            vec![Event::EditorDeleteWord]
        );
        for state in [&normal, &inserting] {
            assert_eq!(
                press(state, KeyEvent::new(KeyCode::Backspace)),
                vec![Event::EditorBackspace]
            );
        }
        for claimed in [
            State {
                diff: Some(vec![]),
                ..inserting.clone()
            },
            State {
                walking: Some(story::Walking::Story {
                    story: 0,
                    step: 0,
                    diff: story::Diff::Hidden,
                }),
                ..inserting.clone()
            },
        ] {
            assert_eq!(press(&claimed, alt_backspace), vec![Event::EditorBackspace]);
        }
        let mut typing_a_command = Drafts {
            command: Some(":wq".to_string()),
            ..Drafts::default()
        };
        assert_eq!(
            on_key_event(&inserting, &mut typing_a_command, alt_backspace, 0),
            vec![]
        );
        assert_eq!(typing_a_command.command.as_deref(), Some(":w"));
        for pane in [Pane::Terminal, Pane::Ai] {
            assert!(reached_the_child(&hosting(pane), alt_backspace));
        }
    }

    #[test]
    fn a_paste_is_not_the_buffers_where_something_else_claims_the_editors_keys() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        let reviewing = State {
            diff: Some(vec![]),
            ..inserting.clone()
        };
        assert!(matches!(
            on_paste(&reviewing, &Drafts::default(), "foo".to_string()),
            Pasted::AsKeys(_)
        ));
        let walking = State {
            walking: Some(story::Walking::Story {
                story: 0,
                step: 0,
                diff: story::Diff::Hidden,
            }),
            ..inserting.clone()
        };
        assert!(matches!(
            on_paste(&walking, &Drafts::default(), "foo".to_string()),
            Pasted::AsKeys(_)
        ));
        let mut rendered = inserting;
        let path = rendered
            .current_buffer
            .clone()
            .expect("the buffer `editing` opened");
        rendered
            .buffers
            .get_mut(&path)
            .expect("the buffer `editing` opened")
            .previewing = true;
        assert!(matches!(
            on_paste(&rendered, &Drafts::default(), "foo".to_string()),
            Pasted::AsKeys(_)
        ));
    }

    #[test]
    fn a_paste_into_a_line_varde_is_collecting_never_reaches_the_child() {
        let drafts = Drafts {
            filter: Some(String::new()),
            ..Drafts::default()
        };
        assert!(matches!(
            on_paste(&focused(Pane::Terminal), &drafts, "src".to_string()),
            Pasted::AsKeys(_)
        ));
    }

    #[test]
    fn keys_go_to_the_focused_pane() {
        assert_eq!(
            press(&focused(Pane::Editor), plain('x')),
            vec![Event::EditorKey('x')]
        );
        assert_eq!(
            press(&focused(Pane::Terminal), plain('x')),
            vec![Event::Bytes(b"x".to_vec())]
        );
        assert_eq!(
            press(&focused(Pane::Tree), plain('x')),
            vec![Event::Key('x')]
        );
    }

    #[test]
    fn an_idle_ai_pane_swallows_keys_rather_than_running_them_in_the_shell() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Ai);
        assert!(on_key_event(&state, &mut drafts, plain('c'), 0).is_empty());
        assert_eq!(drafts.ai, "c", "the keystroke belongs to the start box");
    }

    #[test]
    fn a_running_ai_pane_receives_keys() {
        let state = State {
            focus: Pane::Ai,
            ai_running: true,
            ..State::default()
        };
        assert_eq!(press(&state, plain('c')), vec![Event::Bytes(b"c".to_vec())]);
    }

    #[test]
    fn alt_letters_move_focus_and_alt_arrows_move_by_word() {
        assert_eq!(
            press(&focused(Pane::Editor), alt('h')),
            vec![Event::MoveFocus(Direction::Left)]
        );
        assert_eq!(
            press(
                &focused(Pane::Editor),
                arrow(Direction::Left, KeyModifiers::ALT)
            ),
            vec![Event::EditorWord(Direction::Left)]
        );
        assert_eq!(
            press(
                &focused(Pane::Editor),
                arrow(Direction::Right, KeyModifiers::ALT)
            ),
            vec![Event::EditorWord(Direction::Right)]
        );
        assert!(press(
            &focused(Pane::Editor),
            arrow(Direction::Up, KeyModifiers::ALT)
        )
        .is_empty());
        assert!(press(
            &focused(Pane::Tree),
            arrow(Direction::Left, KeyModifiers::ALT)
        )
        .is_empty());
        assert!(press(
            &focused(Pane::Editor),
            KeyEvent::new(KeyCode::Char('h')).modifiers(KeyModifiers::CTRL | KeyModifiers::ALT)
        )
        .is_empty());
    }

    #[test]
    fn the_readline_word_escapes_move_by_word_too() {
        assert_eq!(
            press(&focused(Pane::Editor), alt('b')),
            vec![Event::EditorWord(Direction::Left)]
        );
        assert_eq!(
            press(&focused(Pane::Editor), alt('f')),
            vec![Event::EditorWord(Direction::Right)]
        );
        assert!(press(&focused(Pane::Tree), alt('b')).is_empty());
    }

    #[test]
    fn asking_what_a_symbol_is_is_an_editor_key() {
        for event in [plain('K'), plain('k').modifiers(KeyModifiers::SHIFT)] {
            assert_eq!(
                press(&focused(Pane::Editor), event),
                vec![Event::EditorKey('K')]
            );
        }
        assert_ne!(
            press(&focused(Pane::Tree), plain('K')),
            vec![Event::EditorKey('K')]
        );
    }

    #[test]
    fn the_keyboard_in_a_hover_scrolls_it_and_nothing_else() {
        let state = State {
            hover: Some(crate::lsp::Hover {
                lines: Vec::new(),
                from: 2,
                asked: crate::lsp::Ask {
                    path: std::path::PathBuf::from("/w/one.rs"),
                    place: crate::Place { line: 1, column: 1 },
                    revision: 0,
                    about: crate::lsp::About::Hover,
                },
                first: 0,
                focused: true,
                value: None,
            }),
            ..editing()
        };
        let down = vec![Event::ScrollHover(Direction::Down)];
        let up = vec![Event::ScrollHover(Direction::Up)];
        assert_eq!(press(&state, plain('j')), down);
        assert_eq!(press(&state, plain('k')), up);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Esc)),
            vec![Event::Cancel]
        );
        assert!(press(&state, plain('x')).is_empty(), "x reached the buffer");
    }

    #[test]
    fn shift_with_an_arrow_extends_in_the_editor_only() {
        assert_eq!(
            press(
                &focused(Pane::Editor),
                arrow(Direction::Right, KeyModifiers::SHIFT)
            ),
            vec![Event::EditorExtend(Direction::Right)]
        );
        assert!(press(
            &focused(Pane::Tree),
            arrow(Direction::Right, KeyModifiers::SHIFT)
        )
        .is_empty());
    }

    #[test]
    fn shift_and_alt_with_an_arrow_extends_by_word() {
        assert_eq!(
            press(
                &focused(Pane::Editor),
                arrow(Direction::Left, KeyModifiers::SHIFT | KeyModifiers::ALT)
            ),
            vec![Event::EditorExtendWord(Direction::Left)]
        );
        assert!(press(
            &focused(Pane::Editor),
            arrow(Direction::Up, KeyModifiers::SHIFT | KeyModifiers::ALT)
        )
        .is_empty());
        assert!(press(
            &focused(Pane::Tree),
            arrow(Direction::Right, KeyModifiers::SHIFT | KeyModifiers::ALT)
        )
        .is_empty());
    }

    #[test]
    fn a_modal_claims_the_keys_belonging_to_it() {
        let palette = State {
            modal: Modal::Palette,
            focus: Pane::Terminal,
            ..State::default()
        };
        assert_eq!(press(&palette, plain('r')), vec![Event::Key('r')]);
    }

    #[test]
    fn the_comment_picker_only_accepts_the_four_types() {
        let state = State {
            modal: Modal::Comment,
            ..State::default()
        };
        let mut drafts = Drafts::default();
        on_key_event(&state, &mut drafts, plain('z'), 0);
        assert!(
            drafts.comment_kind.is_empty(),
            "a stray key must not pick a type"
        );
        on_key_event(&state, &mut drafts, plain('i'), 0);
        assert_eq!(drafts.comment_kind, "ISSUE");
        assert_eq!(
            on_key_event(&state, &mut drafts, plain('z'), 0),
            vec![Event::EditorKey('z')]
        );
    }

    #[test]
    fn both_spellings_of_word_motion_reach_the_comment_body() {
        let showing = State {
            modal: crate::Modal::Comment,
            ..editing()
        };
        let motion = |event| {
            let mut drafts = Drafts {
                comment_kind: "ISSUE".to_string(),
                ..Drafts::default()
            };
            on_key_event(&showing, &mut drafts, event, 0)
        };
        let alt = |code| KeyEvent::new(code).modifiers(KeyModifiers::ALT);
        for back in [alt(KeyCode::Left), alt(KeyCode::Char('b'))] {
            assert_eq!(motion(back), vec![Event::EditorWord(Direction::Left)]);
        }
        for forward in [alt(KeyCode::Right), alt(KeyCode::Char('f'))] {
            assert_eq!(motion(forward), vec![Event::EditorWord(Direction::Right)]);
        }
        assert_eq!(
            motion(KeyEvent::new(KeyCode::Char('b'))),
            vec![Event::EditorKey('b')]
        );
    }

    #[test]
    fn the_comment_box_answers_exactly_the_keys_its_footer_names() {
        let showing = State {
            modal: crate::Modal::Comment,
            ..editing()
        };
        let picked = Drafts {
            comment_kind: "ISSUE".to_string(),
            ..Drafts::default()
        };
        let gesture = |event| {
            let mut drafts = picked.clone();
            let events = on_key_event(&showing, &mut drafts, event, 0);
            !events.is_empty()
                && !events.iter().all(|e| {
                    matches!(
                        e,
                        Event::EditorKey(_)
                            | Event::EditorArrow(_)
                            | Event::EditorWord(_)
                            | Event::EditorBackspace
                            | Event::EditorDeleteWord
                    )
                })
        };
        let named = |label: &str| super::COMMENT_BOX_KEYS.iter().any(|(key, _)| *key == label);
        for (key, word) in super::COMMENT_BOX_KEYS {
            let event = every_key()
                .into_iter()
                .find(|event| label(*event) == key)
                .unwrap_or_else(|| panic!("no key spells {key}"));
            assert!(
                gesture(event),
                "the footer offers {key} for {word} and the body does nothing with it"
            );
        }
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| gesture(*event))
            .map(label)
            .filter(|label| {
                !named(label)
                    && !listed(&State::default(), label, View::Review)
                    && !omitted_in(label, View::Review)
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "the comment box answers keys its footer does not name: {unnamed:?}"
        );
    }

    #[test]
    fn the_submit_confirmation_is_answered_rather_than_typed_through() {
        let state = State {
            modal: Modal::ConfirmSubmit,
            focus: Pane::Ai,
            ai_running: true,
            ..State::default()
        };
        assert_eq!(press(&state, plain('y')), vec![Event::ConfirmSubmit]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Enter)),
            vec![Event::ConfirmSubmit]
        );
        assert_eq!(press(&state, plain('n')), vec![Event::Cancel]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Esc)),
            vec![Event::Cancel]
        );
        assert!(press(&state, plain('z')).is_empty());
        assert_eq!(
            on_key_event(
                &state,
                &mut Drafts::default(),
                KeyEvent::new(KeyCode::Char('y')),
                0
            ),
            vec![Event::ConfirmSubmit]
        );
    }

    #[test]
    fn the_story_confirmation_is_answered_rather_than_typed_through() {
        let state = State {
            modal: Modal::ConfirmStory {
                spelling: "main..HEAD".to_string(),
                out: ".varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.json".to_string(),
            },
            ..State::default()
        };
        assert_eq!(press(&state, plain('y')), vec![Event::ConfirmStory]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Enter)),
            vec![Event::ConfirmStory]
        );
        assert_eq!(press(&state, plain('n')), vec![Event::Cancel]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Esc)),
            vec![Event::Cancel]
        );
        assert!(press(&state, plain('z')).is_empty());
    }

    #[test]
    fn enter_does_nothing_until_a_comment_type_is_picked() {
        let state = State {
            modal: Modal::Comment,
            ..State::default()
        };
        let mut drafts = Drafts::default();
        assert!(on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0).is_empty());
    }

    #[test]
    fn colon_opens_the_command_line_only_in_panes_varde_owns() {
        for pane in [
            Pane::Tree,
            Pane::Editor,
            Pane::Risk,
            Pane::Buffers,
            Pane::History,
        ] {
            let mut drafts = Drafts::default();
            on_key_event(&focused(pane), &mut drafts, plain(':'), 0);
            assert_eq!(
                drafts.command.as_deref(),
                Some(""),
                "{pane:?} should open it"
            );
        }
        let mut drafts = Drafts::default();
        let events = on_key_event(&focused(Pane::Terminal), &mut drafts, plain(':'), 0);
        assert!(drafts.command.is_none(), "the shell keeps its colon");
        assert_eq!(events, vec![Event::Bytes(b":".to_vec())]);
    }

    #[test]
    fn an_inserting_buffer_keeps_only_the_editor_s_colon() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        let mut drafts = Drafts::default();
        assert_eq!(
            on_key_event(&inserting, &mut drafts, plain(':'), 0),
            vec![Event::EditorKey(':')]
        );
        assert!(drafts.command.is_none(), "the buffer keeps its colon");

        let elsewhere = State {
            focus: Pane::Tree,
            ..inserting
        };
        on_key_event(&elsewhere, &mut drafts, plain(':'), 0);
        assert_eq!(drafts.command.as_deref(), Some(""));
    }

    #[test]
    fn q_closes_the_file_and_qa_closes_them_all() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in [':', 'q'] {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::CloseBuffer { force: false }]
        );
        for c in [':', 'q', 'a'] {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::CloseAllBuffers { force: false }]
        );
    }

    #[test]
    fn update_asks_for_a_rebuild() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":update".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::Rebuild]
        );
    }

    #[test]
    fn help_toggles_the_key_box() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":help".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::ToggleCheatsheet]
        );
    }

    #[test]
    fn ai_takes_a_command_and_a_forcing_variant() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":ai! opencode".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::StartAi {
                command: Some("opencode".to_string()),
                force: true,
            }]
        );
    }

    #[test]
    fn story_takes_an_explicit_range_and_a_forcing_variant() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":story! main..HEAD".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::Story {
                explicit: Some("main..HEAD".to_string()),
                force: true,
            }]
        );
    }

    #[test]
    fn a_question_mark_asks_for_the_branch_picker() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":story?".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::PickBranch(None)]
        );
    }

    #[test]
    fn a_url_after_the_question_mark_names_a_guest_repo() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":story? git@github.com:them/theirs.git ".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::PickBranch(Some(
                "git@github.com:them/theirs.git".to_string()
            ))]
        );
    }

    #[test]
    fn a_bare_story_resolves_offline() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":story".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::Story {
                explicit: None,
                force: false,
            }]
        );
    }

    #[test]
    fn the_filter_claims_the_trees_keys_while_it_is_open() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Tree);
        on_key_event(&state, &mut drafts, plain('/'), 0);
        let events = on_key_event(&state, &mut drafts, plain('n'), 0);
        assert_eq!(events, vec![Event::Filter("n".to_string())]);
        assert_eq!(
            drafts.filter.as_deref(),
            Some("n"),
            "not a new-file shortcut"
        );
    }

    #[test]
    fn the_candidate_list_claims_four_keys_and_gives_back_the_rest() {
        let offering = offering_candidates();
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Down)),
            vec![Event::MoveCandidate(Direction::Down)]
        );
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Up)),
            vec![Event::MoveCandidate(Direction::Up)]
        );
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Enter)),
            vec![Event::AcceptCandidate]
        );
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Esc)),
            vec![Event::Cancel]
        );
        assert_eq!(press(&offering, plain('k')), vec![Event::EditorKey('k')]);
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Backspace)),
            vec![Event::EditorBackspace]
        );
        assert_eq!(press(&offering, ctrl('c')), vec![Event::Copy]);
        assert_eq!(
            press(&offering, arrow(Direction::Down, KeyModifiers::SHIFT)),
            vec![Event::EditorExtend(Direction::Down)]
        );
    }

    #[test]
    fn the_in_file_search_claims_the_editors_keys_only_while_it_is_open() {
        let finding = typing_a_query("");
        assert_eq!(press(&finding, plain('b')), vec![Event::EditorKey('b')]);
        assert_eq!(
            press(&finding, KeyEvent::new(KeyCode::Backspace)),
            vec![Event::EditorBackspace]
        );
        assert_eq!(
            press(&finding, KeyEvent::new(KeyCode::Esc)),
            vec![Event::CloseFind]
        );
        assert_eq!(
            press(&finding, KeyEvent::new(KeyCode::Enter)),
            vec![Event::AcceptFind]
        );
        assert_eq!(
            press(&focused(Pane::Editor), plain('b')),
            vec![Event::EditorKey('b')]
        );
        let mut away = typing_a_query("b");
        away.find.as_mut().expect("on").keys = FindKeys::Away;
        assert_eq!(press(&away, plain('b')), vec![Event::EditorKey('b')]);
    }

    fn typing_a_query(query: &str) -> State {
        State {
            find: Some(Find {
                query: crate::editor::Buffer::text_box(query),
                origin: Place { line: 1, column: 1 },
                case: crate::search::Case::Smart,
                keys: FindKeys::Query,
            }),
            ..State::default()
        }
    }

    fn with_find_keys(keys: FindKeys) -> State {
        let mut state = typing_a_query("state");
        state.find.as_mut().expect("on").keys = keys;
        state
    }

    #[test]
    fn the_icons_are_walked_from_the_end_of_the_query() {
        let right = KeyEvent::new(KeyCode::Right);
        let left = KeyEvent::new(KeyCode::Left);
        let icon = |icon| vec![Event::FindKeys(FindKeys::Icon(icon))];
        let mut middle = typing_a_query("state");
        middle
            .find
            .as_mut()
            .expect("on")
            .query
            .arrow(Direction::Left);
        assert_eq!(
            press(&middle, right),
            vec![Event::EditorArrow(Direction::Right)]
        );
        assert_eq!(
            press(&middle, KeyEvent::new(KeyCode::Tab)),
            icon(FindIcon::Case)
        );
        assert_eq!(press(&typing_a_query("state"), right), icon(FindIcon::Case));
        let on_case = with_find_keys(FindKeys::Icon(FindIcon::Case));
        assert_eq!(press(&on_case, right), icon(FindIcon::Replace));
        assert_eq!(
            press(&on_case, left),
            vec![
                Event::FindKeys(FindKeys::Query),
                Event::QueryEnd(Direction::Right)
            ]
        );
        assert_eq!(
            press(&on_case, KeyEvent::new(KeyCode::Enter)),
            vec![Event::ToggleCase]
        );
        let on_last = with_find_keys(FindKeys::Icon(FindIcon::ReplaceAll));
        assert_eq!(press(&on_last, right), icon(FindIcon::ReplaceAll));
        assert_eq!(press(&on_last, left), icon(FindIcon::Replace));
        for back in [KeyCode::Up, KeyCode::Esc] {
            assert_eq!(
                press(&on_last, KeyEvent::new(back)),
                vec![Event::FindKeys(FindKeys::Query)]
            );
        }
        assert_eq!(
            press(&on_last, KeyEvent::new(KeyCode::Enter)),
            vec![Event::FindKeys(FindKeys::Replace(ReplaceField::With))]
        );
    }

    #[test]
    fn the_replace_box_answers_the_keys_its_footer_names() {
        let with = with_find_keys(FindKeys::Replace(ReplaceField::With));
        for (key, word) in super::REPLACE_BOX_KEYS {
            let code = match key {
                "Tab" => KeyCode::Tab,
                "Enter" => KeyCode::Enter,
                "Esc" => KeyCode::Esc,
                other => panic!("no key spells {other}"),
            };
            assert!(
                !press(&with, KeyEvent::new(code)).is_empty(),
                "the box offers {key} for {word} and does nothing with it"
            );
        }
        let shift_tab = KeyEvent::new(KeyCode::Tab).modifiers(KeyModifiers::SHIFT);
        assert_eq!(
            press(&with, shift_tab),
            vec![Event::FindKeys(FindKeys::Replace(ReplaceField::Find))]
        );
        assert_eq!(press(&with, plain('n')), vec![Event::EditorKey('n')]);
        let on_all = with_find_keys(FindKeys::Replace(ReplaceField::ReplaceAll));
        assert_eq!(
            press(&on_all, KeyEvent::new(KeyCode::Enter)),
            vec![Event::ReplaceAll]
        );
        assert_eq!(
            press(&on_all, plain('n')),
            vec![Event::StepMatch(Direction::Right)]
        );
        assert_eq!(
            press(&on_all, KeyEvent::new(KeyCode::Tab)),
            vec![Event::FindKeys(FindKeys::Replace(ReplaceField::Find))]
        );
    }

    #[test]
    fn a_search_query_answers_the_keys_that_move_through_text() {
        let mut finding = typing_a_query("ab");
        finding
            .find
            .as_mut()
            .expect("on")
            .query
            .arrow(Direction::Left);
        let searching = State {
            search: Some(crate::Search::default()),
            ..State::default()
        };
        let key = |code| KeyEvent::new(code);
        for state in [&finding, &searching] {
            for (event, expected) in [
                (key(KeyCode::Left), Event::EditorArrow(Direction::Left)),
                (key(KeyCode::Right), Event::EditorArrow(Direction::Right)),
                (key(KeyCode::Home), Event::QueryEnd(Direction::Left)),
                (key(KeyCode::End), Event::QueryEnd(Direction::Right)),
                (alt('b'), Event::EditorWord(Direction::Left)),
                (alt('f'), Event::EditorWord(Direction::Right)),
                (
                    key(KeyCode::Left).modifiers(KeyModifiers::ALT),
                    Event::EditorWord(Direction::Left),
                ),
                (
                    key(KeyCode::Backspace).modifiers(KeyModifiers::ALT),
                    Event::EditorDeleteWord,
                ),
            ] {
                assert_eq!(press(state, event), vec![expected], "{event:?}");
            }
        }
        assert_eq!(
            press(&searching, key(KeyCode::Down)),
            vec![Event::MoveHit(Direction::Down)]
        );
    }

    #[test]
    fn the_results_box_steps_file_by_file_on_ctrl_n_and_ctrl_p() {
        let state = State {
            search: Some(crate::Search::default()),
            ..State::default()
        };
        assert_eq!(
            press(&state, ctrl('n')),
            vec![Event::MoveHitFile(Direction::Down)]
        );
        assert_eq!(
            press(&state, ctrl('p')),
            vec![Event::MoveHitFile(Direction::Up)]
        );
        assert_eq!(press(&state, plain('n')), vec![Event::EditorKey('n')]);
    }

    #[test]
    fn search_claims_every_printable_key_for_its_query() {
        let state = State {
            search: Some(crate::Search::default()),
            focus: Pane::Terminal,
            ..State::default()
        };
        assert_eq!(press(&state, plain('a')), vec![Event::EditorKey('a')]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Backspace)),
            vec![Event::EditorBackspace]
        );
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Esc)),
            vec![Event::CloseSearch]
        );
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Enter)),
            vec![Event::OpenHit]
        );
        assert_eq!(press(&state, ctrl('o')), vec![Event::OpenEveryHit]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Tab)),
            vec![Event::CompleteSearch]
        );
    }

    #[test]
    fn ctrl_f_opens_search_from_the_panes_varde_owns() {
        for pane in [Pane::Editor, Pane::Tree] {
            assert_eq!(press(&focused(pane), ctrl('f')), vec![Event::OpenSearch]);
            assert_eq!(press(&focused(pane), cmd('f')), vec![Event::OpenSearch]);
            assert!(press(
                &focused(pane),
                KeyEvent::new(KeyCode::Char('f'))
                    .modifiers(KeyModifiers::CTRL | KeyModifiers::SHIFT)
            )
            .is_empty());
        }
    }

    #[test]
    fn a_global_key_that_is_not_reserved_reaches_the_child() {
        let state = hosting(Pane::Terminal);
        for key in [ctrl('f'), ctrl('q'), ctrl('k'), ctrl('l')] {
            assert!(
                matches!(press(&state, key).as_slice(), [Event::Bytes(bytes)] if !bytes.is_empty()),
                "{key:?} was withheld from the child"
            );
        }
    }

    #[test]
    fn ctrl_c_copies_in_the_editor_and_still_interrupts_the_shell() {
        assert_eq!(press(&focused(Pane::Editor), ctrl('c')), vec![Event::Copy]);
        assert_eq!(
            press(&focused(Pane::Terminal), ctrl('c')),
            vec![Event::Bytes(vec![3])]
        );
    }

    #[test]
    fn cmd_d_takes_the_next_occurrence_as_ctrl_d_does() {
        let editor = focused(Pane::Editor);
        assert_eq!(press(&editor, ctrl('d')), vec![Event::EditorNextOccurrence]);
        assert_eq!(press(&editor, cmd('d')), press(&editor, ctrl('d')));
    }

    #[test]
    fn ctrl_s_and_cmd_s_write_the_buffer_unless_the_comment_box_is_open() {
        let editor = focused(Pane::Editor);
        assert_eq!(press(&editor, ctrl('s')), vec![Event::WriteBuffer]);
        assert_eq!(press(&editor, cmd('s')), press(&editor, ctrl('s')));
        let commenting = State {
            modal: crate::Modal::Comment,
            ..editor
        };
        let mut drafts = Drafts {
            comment_kind: "ISSUE".to_string(),
            ..Drafts::default()
        };
        assert_eq!(
            on_key_event(&commenting, &mut drafts, ctrl('s'), 0),
            vec![Event::FileComment {
                kind: "ISSUE".to_string()
            }]
        );
    }

    #[test]
    fn the_escape_key_is_counted_as_a_tap_and_still_reaches_the_child() {
        for pane in [Pane::Terminal, Pane::Ai] {
            let state = hosting(pane);
            let pressed = on_key_event(
                &state,
                &mut Drafts::default(),
                KeyEvent::new(KeyCode::Esc),
                120,
            );
            assert!(
                matches!(
                    pressed.as_slice(),
                    [
                        Event::Bytes(bytes),
                        Event::Tapped {
                            key: Tap::Escape,
                            at_ms: 120
                        }
                    ] if !bytes.is_empty()
                ),
                "{pressed:?}"
            );
            let with_alt = press(
                &state,
                KeyEvent::new(KeyCode::Esc).modifiers(KeyModifiers::ALT),
            );
            assert!(with_alt
                .iter()
                .any(|event| matches!(event, Event::Bytes(_))));
            assert!(!with_alt
                .iter()
                .any(|event| matches!(event, Event::Tapped { .. })));
            let held = press(
                &state,
                KeyEvent {
                    kind: KeyEventKind::Repeat,
                    ..KeyEvent::new(KeyCode::Esc)
                },
            );
            assert!(matches!(held.as_slice(), [Event::Bytes(_)]), "{held:?}");
        }
    }

    #[test]
    fn the_escape_tap_is_a_hosted_pane_gesture_only() {
        for pane in [Pane::Editor, Pane::Tree] {
            assert!(!press(&focused(pane), KeyEvent::new(KeyCode::Esc))
                .iter()
                .any(|event| matches!(event, Event::Tapped { .. })));
        }
    }

    #[test]
    fn ctrl_c_copies_a_pty_selection_and_otherwise_still_interrupts() {
        let picked = |pane| State {
            focus: pane,
            ai_running: true,
            selection: Some(Selection::Screen {
                pane,
                from: Place { line: 1, column: 1 },
                to: Place { line: 1, column: 7 },
                text: "ripgrep".to_string(),
            }),
            ..State::default()
        };
        assert_eq!(press(&picked(Pane::Ai), ctrl('c')), vec![Event::Copy]);
        assert_eq!(press(&picked(Pane::Terminal), ctrl('c')), vec![Event::Copy]);
        assert_eq!(
            press(&focused(Pane::Terminal), ctrl('c')),
            vec![Event::Bytes(vec![3])]
        );
        let running = State {
            focus: Pane::Ai,
            ai_running: true,
            ..State::default()
        };
        assert_eq!(press(&running, ctrl('c')), vec![Event::Bytes(vec![3])]);
        let elsewhere = State {
            focus: Pane::Terminal,
            selection: Some(Selection::Buffer {
                anchor: Place { line: 1, column: 1 },
                cursor: Place { line: 1, column: 3 },
            }),
            ..State::default()
        };
        assert_eq!(press(&elsewhere, ctrl('c')), vec![Event::Bytes(vec![3])]);
        let other_pane = State {
            focus: Pane::Terminal,
            ai_running: true,
            ..picked(Pane::Ai)
        };
        assert_eq!(press(&other_pane, ctrl('c')), vec![Event::Bytes(vec![3])]);
    }

    #[test]
    fn a_bare_ctrl_is_stamped_for_the_double_tap() {
        let state = State::default();
        let bare_ctrl = KeyEvent::new(KeyCode::Modifier(
            ModifierKeyCode::Control,
            ModifierDirection::Unknown,
        ));
        assert_eq!(
            on_key_event(&state, &mut Drafts::default(), bare_ctrl, 450),
            vec![Event::Tapped {
                key: Tap::Ctrl,
                at_ms: 450
            }]
        );
    }

    fn every_key() -> Vec<KeyEvent> {
        let mut codes = vec![
            KeyCode::Backspace,
            KeyCode::Enter,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::Esc,
            KeyCode::CapsLock,
            KeyCode::ScrollLock,
            KeyCode::NumLock,
            KeyCode::PrintScreen,
            KeyCode::Pause,
            KeyCode::Menu,
            KeyCode::KeypadBegin,
        ];
        codes.extend((1..=35).map(KeyCode::F));
        codes.extend((0x20u8..0x7f).map(|c| KeyCode::Char(c as char)));
        codes.extend(['é', 'ø', '漢', '🙂'].map(KeyCode::Char));
        codes.extend(
            [
                MediaKeyCode::Play,
                MediaKeyCode::Pause,
                MediaKeyCode::PlayPause,
                MediaKeyCode::Reverse,
                MediaKeyCode::Stop,
                MediaKeyCode::FastForward,
                MediaKeyCode::Rewind,
                MediaKeyCode::TrackNext,
                MediaKeyCode::TrackPrevious,
                MediaKeyCode::Record,
                MediaKeyCode::LowerVolume,
                MediaKeyCode::RaiseVolume,
                MediaKeyCode::MuteVolume,
            ]
            .map(KeyCode::Media),
        );
        for modifier in [
            ModifierKeyCode::Shift,
            ModifierKeyCode::Control,
            ModifierKeyCode::Alt,
            ModifierKeyCode::Super,
            ModifierKeyCode::Hyper,
            ModifierKeyCode::Meta,
            ModifierKeyCode::IsoLevel3Shift,
            ModifierKeyCode::IsoLevel5Shift,
        ] {
            codes.extend(
                [
                    ModifierDirection::Left,
                    ModifierDirection::Right,
                    ModifierDirection::Unknown,
                ]
                .map(|side| KeyCode::Modifier(modifier, side)),
            );
        }
        let mut keys = Vec::new();
        for code in codes {
            for bits in 0..=KeyModifiers::all().bits() {
                keys.push(KeyEvent::new(code).modifiers(KeyModifiers::from_bits_truncate(bits)));
            }
        }
        keys
    }

    const RESERVED: [(&str, &str); 7] = [
        (
            "C-space",
            "opens the view palette from every pane, hosted ones included, so \
             it is the one gesture that is the same everywhere. The child \
             loses the NUL byte readline reads as set-mark",
        ),
        (
            "Ctrl",
            "double-tapped it opens the view palette, and a lone modifier \
             produces no bytes in a pty, so the child could never have \
             received it",
        ),
        ("M-h", "moves pane focus, on every pane"),
        ("M-j", "moves pane focus"),
        ("M-k", "moves pane focus"),
        ("M-l", "moves pane focus"),
        (
            "C-c with a selection",
            "copies what the mouse picked in Varde's own UI. With nothing \
             picked it interrupts the child, which the sweep below drives and \
             this list therefore does not excuse",
        ),
    ];

    const UNENCODABLE: [(&str, &str); 11] = [
        ("Shift alone", "a bare modifier has no sequence"),
        ("Alt alone", "a bare modifier has no sequence"),
        ("Super alone", "a bare modifier has no sequence"),
        ("Hyper alone", "a bare modifier has no sequence"),
        ("Meta alone", "a bare modifier has no sequence"),
        ("IsoLevel3Shift alone", "a bare modifier has no sequence"),
        ("IsoLevel5Shift alone", "a bare modifier has no sequence"),
        ("a media key", "no legacy sequence exists for it"),
        (
            "a lock or system key",
            "CapsLock, ScrollLock, NumLock, PrintScreen, Pause, Menu and \
             KeypadBegin have no legacy sequence",
        ),
        (
            "F13 and above",
            "legacy xterm stops at F12. The encoder writes a bare CSI \
             introducer above that, which `encode` drops rather than send \
             half a sequence",
        ),
        (
            "C- a character with no control code",
            "the control codes are the letters, space and 4 to 7; Ctrl with a \
             digit or a punctuation mark needs the Kitty protocol the child is \
             deliberately not offered",
        ),
    ];

    fn withheld_as(event: KeyEvent) -> String {
        match event.code {
            KeyCode::Modifier(ModifierKeyCode::Control, _) => "Ctrl".to_string(),
            KeyCode::Modifier(modifier, _) => format!("{modifier:?} alone"),
            KeyCode::Media(_) => "a media key".to_string(),
            KeyCode::Char(letter @ ('h' | 'j' | 'k' | 'l'))
                if event.modifiers == KeyModifiers::ALT =>
            {
                format!("M-{letter}")
            }
            KeyCode::F(13..) => "F13 and above".to_string(),
            KeyCode::Char(' ') if event.modifiers.contains(KeyModifiers::CTRL) => {
                "C-space".to_string()
            }
            KeyCode::Char(c) if event.modifiers.contains(KeyModifiers::CTRL) => ctrl_withheld(c),
            KeyCode::CapsLock
            | KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin => "a lock or system key".to_string(),
            other => format!("{other:?}"),
        }
    }

    fn ctrl_withheld(c: char) -> String {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c == ' ' || ('4'..='7').contains(&c) {
            format!("C-{c}")
        } else {
            "C- a character with no control code".to_string()
        }
    }

    fn excused(label: &str) -> bool {
        RESERVED
            .iter()
            .chain(UNENCODABLE.iter())
            .any(|(withheld, _)| withheld == &label)
    }

    fn hosting(pane: Pane) -> State {
        State {
            focus: pane,
            ai_running: true,
            ..State::default()
        }
    }

    fn reached_the_child(state: &State, event: KeyEvent) -> bool {
        on_key_event(state, &mut Drafts::default(), event, 0)
            .iter()
            .any(|event| matches!(event, Event::Bytes(bytes) if !bytes.is_empty()))
    }

    #[test]
    fn every_key_reaches_a_hosted_pane_or_is_recorded_as_withheld() {
        for pane in [Pane::Terminal, Pane::Ai] {
            let state = hosting(pane);
            let mut missing: Vec<String> = every_key()
                .into_iter()
                .filter(|event| !reached_the_child(&state, *event))
                .map(withheld_as)
                .filter(|label| !excused(label))
                .collect();
            missing.sort();
            missing.dedup();
            assert!(
                missing.is_empty(),
                "{pane:?} withheld keys that are on no list: {missing:?}"
            );
        }
    }

    #[test]
    fn the_withheld_list_holds_only_keys_that_are_really_withheld() {
        let state = hosting(Pane::Terminal);
        let withheld: Vec<String> = every_key()
            .into_iter()
            .filter(|event| !reached_the_child(&state, *event))
            .map(withheld_as)
            .collect();
        for (label, _) in RESERVED.iter().chain(UNENCODABLE.iter()) {
            if *label == "C-c with a selection" {
                continue;
            }
            assert!(
                withheld.iter().any(|found| found == label),
                "{label} is excused as withheld but reaches the child"
            );
        }
    }

    #[test]
    fn each_reserved_key_is_claimed_and_the_child_gets_nothing() {
        for pane in [Pane::Terminal, Pane::Ai] {
            let state = hosting(pane);
            for (letter, direction) in [
                ('h', Direction::Left),
                ('j', Direction::Down),
                ('k', Direction::Up),
                ('l', Direction::Right),
            ] {
                assert_eq!(
                    press(&state, alt(letter)),
                    vec![Event::MoveFocus(direction)]
                );
            }
            assert_eq!(press(&state, ctrl(' ')), vec![Event::FallbackBinding]);
            assert_eq!(
                on_key_event(
                    &state,
                    &mut Drafts::default(),
                    KeyEvent::new(KeyCode::Modifier(
                        ModifierKeyCode::Control,
                        ModifierDirection::Left
                    )),
                    450,
                ),
                vec![Event::Tapped {
                    key: Tap::Ctrl,
                    at_ms: 450
                }]
            );
        }
    }

    #[test]
    fn the_copy_key_copies_with_a_selection_and_interrupts_without_one() {
        let state = hosting(Pane::Ai);
        assert_eq!(press(&state, ctrl('c')), vec![Event::Bytes(vec![3])]);
        let picked = State {
            selection: Some(Selection::Screen {
                pane: Pane::Ai,
                from: Place { line: 1, column: 1 },
                to: Place { line: 1, column: 7 },
                text: "ripgrep".to_string(),
            }),
            ..state
        };
        assert_eq!(press(&picked, ctrl('c')), vec![Event::Copy]);
        assert_eq!(press(&picked, cmd('c')), vec![Event::Copy]);
    }

    #[test]
    fn a_shifted_control_key_still_reaches_the_child() {
        let state = hosting(Pane::Terminal);
        for event in [
            ctrl('c'),
            KeyEvent::new(KeyCode::Char('C')).modifiers(KeyModifiers::CTRL),
            KeyEvent::new(KeyCode::Char('c')).modifiers(KeyModifiers::CTRL | KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('C')).modifiers(KeyModifiers::CTRL | KeyModifiers::SHIFT),
        ] {
            assert_eq!(
                press(&state, event),
                vec![Event::Bytes(vec![3])],
                "{event:?}"
            );
        }
    }

    #[test]
    fn a_function_key_legacy_xterm_cannot_express_sends_nothing() {
        let state = hosting(Pane::Terminal);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::F(12))),
            vec![Event::Bytes(b"\x1b[24~".to_vec())]
        );
        assert!(press(&state, KeyEvent::new(KeyCode::F(13))).is_empty());
    }

    #[test]
    fn what_varde_is_collecting_outranks_the_child() {
        let showing = State {
            modal: Modal::Palette,
            ..hosting(Pane::Terminal)
        };
        assert_eq!(press(&showing, plain('r')), vec![Event::Key('r')]);
        let mut drafts = Drafts {
            command: Some(String::new()),
            ..Drafts::default()
        };
        let events = on_key_event(&hosting(Pane::Terminal), &mut drafts, plain('q'), 0);
        assert!(events.is_empty(), "the draft is collecting, not the child");
        assert_eq!(drafts.command.as_deref(), Some("q"));
    }

    #[test]
    fn the_editor_and_the_tree_read_the_character_the_key_typed() {
        assert_eq!(
            press(&focused(Pane::Editor), plain('x')),
            vec![Event::EditorKey('x')]
        );
        assert_eq!(
            press(&focused(Pane::Tree), plain('x')),
            vec![Event::Key('x')]
        );
        assert_eq!(
            press(
                &focused(Pane::Editor),
                KeyEvent::new(KeyCode::Char('n')).modifiers(KeyModifiers::SHIFT)
            ),
            vec![Event::EditorKey('N')]
        );
    }

    #[test]
    fn the_buffers_panes_keys_are_decided_here() {
        let pane = focused(Pane::Buffers);
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Down)),
            vec![Event::MoveSelection(Direction::Down)]
        );
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Up)),
            vec![Event::MoveSelection(Direction::Up)]
        );
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Enter)),
            vec![Event::Activate]
        );
        for key in ['j', 'k'] {
            assert_eq!(press(&pane, plain(key)), vec![Event::Key(key)]);
        }
    }

    #[test]
    fn the_risk_lists_keys_are_decided_here() {
        let pane = focused(Pane::Risk);
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Down)),
            vec![Event::MoveSelection(Direction::Down)]
        );
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Up)),
            vec![Event::MoveSelection(Direction::Up)]
        );
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Enter)),
            vec![Event::Activate]
        );
        for key in ['j', 'k', 'a', 'r', 'l'] {
            assert_eq!(press(&pane, plain(key)), vec![Event::Key(key)]);
        }
        for (code, direction) in [
            (KeyCode::Right, Direction::Right),
            (KeyCode::Left, Direction::Left),
        ] {
            assert_eq!(
                press(&pane, KeyEvent::new(code)),
                vec![Event::MoveAction(direction)]
            );
        }
    }

    fn press(state: &State, event: KeyEvent) -> Vec<Event> {
        on_key_event(state, &mut Drafts::default(), event, 0)
    }

    fn plain(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c))
    }

    fn alt(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c)).modifiers(KeyModifiers::ALT)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c)).modifiers(KeyModifiers::CTRL)
    }

    fn cmd(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c)).modifiers(KeyModifiers::SUPER)
    }

    fn arrow(direction: Direction, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(match direction {
            Direction::Left => KeyCode::Left,
            Direction::Right => KeyCode::Right,
            Direction::Up => KeyCode::Up,
            Direction::Down => KeyCode::Down,
        })
        .modifiers(modifiers)
    }

    const UNLISTED: [(&str, &str, &[View]); 24] = [
        (
            "Ctrl",
            "the router still answers a bare Ctrl press with a tap, but no \
             terminal reports one: that needs the keyboard flag whose cost is \
             every character a layout composes, which `main.rs` no longer pays. \
             Listing a gesture nobody can perform teaches the wrong key",
            &[View::Edit, View::Review, View::Story],
        ),
        (
            "l",
            "cursor motion — the one thing nobody needs reminding of. Over a \
             Preview it is the same motion, run over the rendered row",
            &[View::Edit],
        ),
        (
            "arr",
            "cursor motion, in every mode — and through a walked Site too, \
             which is read-only about its contents, not about the cursor. \
             With a candidate list up they move the choice instead, which is \
             the same gesture the tree and the Risk list answer and the one \
             thing every list everywhere already teaches",
            &[View::Edit, View::Review, View::Story],
        ),
        ("Home", "an alias of 0, which is listed", &[View::Edit]),
        ("End", "an alias of $, which is listed", &[View::Edit]),
        (
            "S-M-arr",
            "the arrow alias of W and B, which are listed",
            &[View::Edit],
        ),
        (
            "M-b",
            "what the terminal sends for Option+Left on macOS, so it is the \
             word motion b, which is listed",
            &[View::Edit, View::Story],
        ),
        (
            "M-f",
            "what the terminal sends for Option+Right on macOS, so it is the \
             word motion w, which is listed",
            &[View::Edit, View::Story],
        ),
        (
            "M-arr",
            "the arrow alias of the word motions b and w, which are listed — \
             and, like the plain arrows, a motion rather than a key, so it \
             moves while inserting and through a walked Site too",
            &[View::Edit, View::Story],
        ),
        (
            "C-M-arr",
            "Ctrl+Option on the left and right arrows, so it is the jump back \
             and forward that C-p, C-n and gp/gn spell, which are listed. An \
             alias rather than a route of its own, for the reason M-arr is one",
            &[View::Edit, View::Review, View::Story],
        ),
        (
            "1",
            "a count is a rule about other keys, not a key of its own",
            &[View::Edit],
        ),
        ("2", "a count", &[View::Edit]),
        ("3", "a count", &[View::Edit]),
        ("4", "a count", &[View::Edit]),
        ("5", "a count", &[View::Edit]),
        ("6", "a count", &[View::Edit]),
        ("7", "a count", &[View::Edit]),
        ("8", "a count", &[View::Edit]),
        ("9", "a count", &[View::Edit]),
        (
            "Bksp",
            "deletes backwards — an insert-mode key, and the box is hidden there",
            &[View::Edit],
        ),
        (
            "M-Bksp",
            "takes the word behind the cursor while inserting, and the single \
             character Bksp takes everywhere else — both insert-mode keys, and \
             the box is hidden there. Spelled apart from Bksp rather than \
             folded onto it because the router reads the modifier, so it is a \
             gesture rather than a modifier a binding ignores. `db` is its \
             modifier-free route and is listed under the operator's row",
            &[View::Edit],
        ),
        (
            "D-d",
            "a walked Site claims the key before the next occurrence can, so \
             Command on it is the `d` it always was, which is listed",
            &[View::Story],
        ),
        (
            "C-q",
            "quits Varde, from wherever you are",
            &[View::Edit, View::Review, View::Story],
        ),
        (
            ":",
            "opens the command line in every view — only :update, which is \
             listed, answers with nothing open",
            &[View::Review, View::Story],
        ),
    ];

    fn label(event: KeyEvent) -> String {
        let event = super::shifted(event);
        let ctrl = event.modifiers.contains(KeyModifiers::CTRL);
        let command = event.modifiers.contains(KeyModifiers::SUPER);
        let alt = event.modifiers.contains(KeyModifiers::ALT);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        match event.code {
            KeyCode::Modifier(ModifierKeyCode::Control, _) => "Ctrl".to_string(),
            KeyCode::Modifier(modifier, _) => format!("{modifier:?} alone"),
            KeyCode::Media(_) => "a media key".to_string(),
            KeyCode::Char(' ') if ctrl => "C-space".to_string(),
            KeyCode::Char(' ') => "␣".to_string(),
            KeyCode::Char('z') if ctrl || command => "C-z".to_string(),
            KeyCode::Char('Z') if ctrl || command => "C-S-z".to_string(),
            KeyCode::Char(c) if ctrl => format!("C-{c}"),
            KeyCode::Char(c @ ('c' | 'v' | 's' | 'd' | 'f')) if command => format!("D-{c}"),
            KeyCode::Char(c) if alt => format!("M-{c}"),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Backspace if alt => "M-Bksp".to_string(),
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                arrow_label(shift, alt, ctrl)
            }
            KeyCode::F(number) if ctrl => format!("C-F{number}"),
            KeyCode::F(8) if shift => "S-F8".to_string(),
            KeyCode::F(number) => format!("F{number}"),
            code => named_label(code),
        }
    }

    fn arrow_label(shift: bool, alt: bool, ctrl: bool) -> String {
        if alt && ctrl {
            return "C-M-arr".to_string();
        }
        match (shift, alt) {
            (true, true) => "S-M-arr".to_string(),
            (true, false) => "S-arr".to_string(),
            (false, true) => "M-arr".to_string(),
            (false, false) => "arr".to_string(),
        }
    }

    fn named_label(code: KeyCode) -> String {
        match code {
            KeyCode::Backspace => "Bksp".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::Delete => "Del".to_string(),
            KeyCode::Insert => "Ins".to_string(),
            KeyCode::CapsLock
            | KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin => "a lock or system key".to_string(),
            KeyCode::Modifier(..)
            | KeyCode::Media(_)
            | KeyCode::Char(_)
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::F(_) => unreachable!("{code:?} is spelled by `label`"),
        }
    }

    fn folded(event: KeyEvent) -> KeyEvent {
        let mut event = super::shifted(event);
        event.code = match event.code {
            KeyCode::Modifier(modifier, _) => {
                KeyCode::Modifier(modifier, ModifierDirection::Unknown)
            }
            KeyCode::Media(_) => KeyCode::Media(MediaKeyCode::Play),
            KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin => KeyCode::CapsLock,
            code => code,
        };
        if matches!(event.code, KeyCode::Char('z' | 'Z'))
            && event.modifiers.contains(KeyModifiers::SUPER)
        {
            event.modifiers = (event.modifiers - KeyModifiers::SUPER) | KeyModifiers::CTRL;
        }
        event.modifiers &= match event.code {
            KeyCode::Char(_) if event.modifiers.contains(KeyModifiers::CTRL) => KeyModifiers::CTRL,
            KeyCode::Char('c' | 'v' | 's' | 'd' | 'f')
                if event.modifiers.contains(KeyModifiers::SUPER) =>
            {
                KeyModifiers::SUPER
            }
            KeyCode::Char(_) => KeyModifiers::ALT,
            KeyCode::Backspace => KeyModifiers::ALT,
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down
                if event
                    .modifiers
                    .contains(KeyModifiers::ALT | KeyModifiers::CTRL) =>
            {
                KeyModifiers::ALT | KeyModifiers::CTRL
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                KeyModifiers::SHIFT | KeyModifiers::ALT
            }
            KeyCode::F(8) => KeyModifiers::CTRL | KeyModifiers::SHIFT,
            KeyCode::F(_) => KeyModifiers::CTRL,
            _ => KeyModifiers::NONE,
        };
        event
    }

    #[test]
    fn a_key_does_what_the_gesture_its_label_names_does() {
        let state = editing();
        for event in every_key() {
            assert_eq!(
                press(&state, event),
                press(&state, folded(event)),
                "{:?} is spelled {} and does something else",
                event,
                label(event)
            );
        }
    }

    const OPERATORS: [char; 3] = ['g', 'd', 'y'];

    fn candidates(state: &State) -> Vec<(String, Vec<KeyEvent>)> {
        let keys = every_key();
        let seconds: Vec<KeyEvent> = keys
            .iter()
            .copied()
            .filter(|key| !answers(state, &[*key]))
            .collect();
        let mut candidates: Vec<(String, Vec<KeyEvent>)> = keys
            .into_iter()
            .map(|key| (label(key), vec![key]))
            .collect();
        for operator in OPERATORS {
            for second in &seconds {
                candidates.push((
                    format!("{operator}{}", label(*second)),
                    vec![plain(operator), *second],
                ));
            }
        }
        candidates
    }

    fn editing() -> State {
        let opened = crate::update(
            &State::default(),
            Event::BufferOpened {
                path: std::path::PathBuf::from("/w/one.rs"),
                contents: "one two three\nfour five six\nseven eight nine\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let mut state = State {
            focus: Pane::Editor,
            ..opened
        };
        for direction in [Direction::Down, Direction::Right, Direction::Right] {
            state = crate::update(&state, Event::EditorArrow(direction)).0;
        }
        state
    }

    fn reviewing() -> State {
        let shown = crate::update(
            &crate::update(&State::default(), Event::OpenReviewView).0,
            Event::ShowDiff {
                file: "one.rs".to_string(),
                lines: vec![
                    DiffLine {
                        new_line: Some(1),
                        old_line: Some(1),
                        removed: false,
                        text: "one".to_string(),
                    },
                    DiffLine {
                        new_line: Some(2),
                        old_line: None,
                        removed: false,
                        text: "two".to_string(),
                    },
                    DiffLine {
                        new_line: None,
                        old_line: Some(2),
                        removed: true,
                        text: "three".to_string(),
                    },
                ],
                revision: "abc123".to_string(),
            },
        )
        .0;
        let focused = crate::update(&shown, Event::MoveFocus(Direction::Right)).0;
        crate::update(&focused, Event::EditorKey('j')).0
    }

    fn story() -> State {
        let file = "one.rs".to_string();
        let site = |from, to| story::Site {
            file: file.clone(),
            side: story::Side::New,
            kind: story::Kind::Changed,
            from,
            to,
            text: String::new(),
        };
        let step = |claim: &str, site| story::Step {
            id: claim.to_string(),
            name: claim.to_string(),
            claim: claim.to_string(),
            why: String::new(),
            site,
            flow: None,
            values: Vec::new(),
            nudge: None,
            prediction: None,
        };
        let artifact = story::Artifact {
            protocol_version: 2,
            title: "Story set".to_string(),
            range: story::Range {
                base: "a".to_string(),
                head: "b".to_string(),
                spelling: "main..HEAD".to_string(),
            },
            stories: vec![story::Story {
                id: "s1".to_string(),
                name: "Story".to_string(),
                premise: String::new(),
                steps: vec![
                    story::Step {
                        values: vec![story::Value {
                            name: "n".to_string(),
                            value: "v".to_string(),
                            provenance: story::Provenance::Literal,
                            cite: Some(story::Cite {
                                file: file.clone(),
                                line: 1,
                            }),
                        }],
                        ..step("first", site(1, 1))
                    },
                    step("second", site(2, 2)),
                ],
            }],
        };
        let opened = crate::update(
            &State::default(),
            Event::BufferOpened {
                path: std::path::PathBuf::from("one.rs"),
                contents: "one\ntwo\nthree\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let walking = State {
            view: View::Story,
            focus: Pane::Editor,
            story_set: story::Set::Loaded(artifact),
            walking: Some(story::Walking::Story {
                story: 0,
                step: 0,
                diff: story::Diff::Hidden,
            }),
            ..opened
        };
        [Direction::Down, Direction::Right, Direction::Right]
            .into_iter()
            .fold(walking, |state, direction| {
                crate::update(&state, Event::EditorArrow(direction)).0
            })
    }

    fn settled(state: &State) -> State {
        let mut settled = state.clone();
        if let Some(buffer) = settled
            .current_buffer
            .clone()
            .and_then(|path| settled.buffers.get_mut(&path))
        {
            buffer.clear_pending();
        }
        settled
    }

    fn drive(state: &State, drafts: &mut Drafts, sequence: &[KeyEvent]) -> (State, bool) {
        let mut next = state.clone();
        let mut acted = false;
        for key in sequence {
            for event in on_key_event(&next, drafts, *key, 0) {
                let (after, effects) = crate::update(&next, event);
                next = after;
                acted |= effects.iter().any(|effect| {
                    !matches!(
                        effect,
                        crate::Effect::NotifyAbout {
                            slug: "no-such-motion",
                            ..
                        }
                    )
                });
            }
        }
        (next, acted)
    }

    fn outcome(state: &State, sequence: &[KeyEvent]) -> (State, Drafts, bool) {
        let mut drafts = Drafts::default();
        let (next, acted) = drive(state, &mut drafts, sequence);
        (settled(&next), drafts, acted)
    }

    fn answers(state: &State, sequence: &[KeyEvent]) -> bool {
        let (after, drafts, acted) = outcome(state, sequence);
        acted || after != settled(state) || drafts != Drafts::default()
    }

    fn chord_answers(state: &State, operator: KeyEvent, second: KeyEvent) -> bool {
        let mut drafts = Drafts::default();
        let (after_operator, _) = drive(state, &mut drafts, &[operator]);
        let baseline = settled(&after_operator);
        let baseline_drafts = drafts.clone();
        let (after_both, acted) = drive(&after_operator, &mut drafts, &[second]);
        acted || settled(&after_both) != baseline || drafts != baseline_drafts
    }

    fn sequence_answers(state: &State, sequence: &[KeyEvent]) -> bool {
        match sequence {
            [operator, second] => chord_answers(state, *operator, *second),
            _ => answers(state, sequence),
        }
    }

    fn views() -> [(View, State); 6] {
        [
            (View::Edit, editing()),
            (View::Edit, offering_candidates()),
            (View::Edit, filling_in_a_snippet(&editing())),
            (View::Review, reviewing()),
            (View::Story, story()),
            (View::Edit, crate::debug::paused(editing())),
        ]
    }

    fn filling_in_a_snippet(state: &State) -> State {
        match state.current_buffer.clone() {
            Some(path) => State {
                modal: crate::Modal::Stops {
                    path,
                    at: vec![crate::editor::Tail(0)],
                },
                ..state.clone()
            },
            None => state.clone(),
        }
    }

    fn offering_candidates() -> State {
        State {
            modal: crate::Modal::Candidates(crate::lsp::Candidates::offering(
                &State::default(),
                vec![
                    crate::lsp::Candidate {
                        label: "workspace_root".to_string(),
                        insert: "workspace_root".to_string(),
                        filter: None,
                        sort: None,
                        snippet: false,
                    },
                    crate::lsp::Candidate {
                        label: "working_dir".to_string(),
                        insert: "working_dir".to_string(),
                        filter: None,
                        sort: None,
                        snippet: false,
                    },
                ],
                crate::lsp::Ask {
                    path: std::path::PathBuf::from("/w/one.rs"),
                    place: crate::Place { line: 2, column: 1 },
                    revision: 1,
                    about: crate::lsp::About::Candidates,
                },
            )),
            ..editing()
        }
    }

    fn names(token: &str, label: &str) -> bool {
        token == label
            || (label.chars().count() == 1 && token.chars().count() == 2 && token.contains(label))
    }

    fn listed(state: &State, label: &str, view: View) -> bool {
        super::cheatsheet(state)
            .filter(|(_, _, views)| super::applies_to(views, view))
            .flat_map(|(keys, _, _)| keys.split_whitespace())
            .any(|token| names(token, label))
    }

    fn omitted_in(label: &str, view: View) -> bool {
        UNLISTED
            .iter()
            .any(|(omitted, _, views)| *omitted == label && super::applies_to(views, view))
    }

    #[test]
    fn every_binding_the_editor_answers_is_listed_or_a_recorded_omission() {
        for (view, state) in views() {
            let mut missing: Vec<String> = candidates(&state)
                .into_iter()
                .filter(|(_, sequence)| sequence_answers(&state, sequence))
                .map(|(label, _)| label)
                .filter(|label| !listed(&state, label, view) && !omitted_in(label, view))
                .collect();
            missing.sort();
            missing.dedup();
            assert!(
                missing.is_empty(),
                "in {view:?} these bindings do something and are in neither the \
                 cheatsheet nor the omissions list: {missing:?}"
            );
        }
    }

    const SNIPPET_DECLINES: [(&str, crate::editor::Mode, &str); 3] = [
        (
            "Enter",
            crate::editor::Mode::Normal,
            "runs the Snippet — the Selection if there is one — which is what \
             the Evaluator is for. Inserting, it is the newline it is in the \
             editor, and the sweep holds it to that",
        ),
        (
            "C-Enter",
            crate::editor::Mode::Normal,
            "the alias of normal-mode Enter, which runs the Snippet",
        ),
        (
            "C-Enter",
            crate::editor::Mode::Insert,
            "runs the Snippet while inserting, so a block need not be left to \
             be run — the editor reads no Ctrl on Enter and types the newline",
        ),
    ];

    fn gesture(label: String, sequence: &[KeyEvent]) -> String {
        match sequence {
            [key] if key.code == KeyCode::Enter && key.modifiers.contains(KeyModifiers::CTRL) => {
                "C-Enter".to_string()
            }
            _ => label,
        }
    }

    type Edit = (
        String,
        Place,
        crate::editor::Mode,
        Option<Selection>,
        Vec<Place>,
    );

    fn edit_of(state: &State) -> Option<Edit> {
        let buffer = state.edited()?;
        Some((
            buffer.shown().to_string(),
            Place {
                line: buffer.line,
                column: buffer.column,
            },
            buffer.mode,
            state.selection.clone(),
            state.occurrences.clone(),
        ))
    }

    fn behind(state: &State) -> Option<(String, Place)> {
        let buffer = state.buffers.get(state.current_buffer.as_ref()?)?;
        Some((
            buffer.shown().to_string(),
            Place {
                line: buffer.line,
                column: buffer.column,
            },
        ))
    }

    fn snippet_pairs() -> Vec<(State, State)> {
        let opened = crate::update(
            &State::default(),
            Event::BufferOpened {
                path: std::path::PathBuf::from("/w/one.rs"),
                contents: "one two one\ntwo one two\none two one\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let mut editor = crate::debug::paused(State {
            focus: Pane::Editor,
            ..opened
        });
        for direction in [Direction::Down, Direction::Right, Direction::Right] {
            editor = crate::update(&editor, Event::EditorArrow(direction)).0;
        }
        let mut evaluator = editor.clone();
        let text = editor
            .edited()
            .expect("a buffer is open")
            .shown()
            .to_string();
        crate::debug::open_evaluator(&mut evaluator, text);
        let at = edit_of(&editor).expect("a buffer is open").1;
        if let Some(it) = evaluator.evaluator.as_mut() {
            it.snippet.go_to_place(at);
        }
        let extend = KeyEvent::new(KeyCode::Right).modifiers(KeyModifiers::SHIFT);
        let take = KeyEvent::new(KeyCode::Char('d')).modifiers(KeyModifiers::CTRL);
        [
            vec![],
            vec![plain('i')],
            vec![extend],
            vec![plain('i'), extend],
            vec![take, take],
            vec![plain('i'), take, take],
        ]
        .into_iter()
        .map(|start| (outcome(&editor, &start).0, outcome(&evaluator, &start).0))
        .collect()
    }

    #[test]
    fn every_editing_key_does_to_the_snippet_what_it_does_in_the_editor() {
        let mut diverged: Vec<String> = Vec::new();
        for (editor, evaluator) in snippet_pairs() {
            let before = edit_of(&editor);
            assert_eq!(before, edit_of(&evaluator), "the pair starts equal");
            let mode = before.expect("a buffer is open").2;
            let file = behind(&evaluator);
            for (label, sequence) in candidates(&editor) {
                let label = gesture(label, &sequence);
                if SNIPPET_DECLINES
                    .iter()
                    .any(|(declined, declined_in, _)| *declined == label && *declined_in == mode)
                {
                    continue;
                }
                let edited = drive(&editor, &mut Drafts::default(), &sequence).0;
                let typed = drive(&evaluator, &mut Drafts::default(), &sequence).0;
                if edit_of(&typed) != edit_of(&edited) || behind(&typed) != file {
                    diverged.push(format!("{label} ({mode:?})"));
                }
            }
        }
        diverged.sort();
        diverged.dedup();
        assert!(
            diverged.is_empty(),
            "these editing keys do something else in the Snippet and are not \
             declined: {diverged:?}"
        );
    }

    #[test]
    fn every_declined_editing_key_still_differs_in_the_snippet() {
        for (declined, mode, _) in SNIPPET_DECLINES {
            let differs = snippet_pairs()
                .into_iter()
                .filter(|(editor, _)| edit_of(editor).map(|edit| edit.2) == Some(mode))
                .any(|(editor, evaluator)| {
                    candidates(&editor)
                        .into_iter()
                        .filter(|(label, sequence)| gesture(label.clone(), sequence) == declined)
                        .any(|(_, sequence)| {
                            let edited = drive(&editor, &mut Drafts::default(), &sequence);
                            let typed = drive(&evaluator, &mut Drafts::default(), &sequence);
                            edit_of(&edited.0) != edit_of(&typed.0)
                        })
                });
            assert!(
                differs,
                "{declined} is declined in {mode:?} and does the same"
            );
        }
    }

    #[test]
    fn tools_answers_exactly_the_keys_its_box_names() {
        let mut listing = State {
            modal: crate::Modal::Tools { row: 0 },
            os: "macos".to_string(),
            ..editing()
        };
        listing.servers.insert(
            "zig".to_string(),
            crate::startup::Server {
                command: "zls".to_string(),
                args: Vec::new(),
                also_served_by: Vec::new(),
                extensions: Vec::new(),
                install: [("macos".to_string(), "brew install zls".to_string())]
                    .into_iter()
                    .collect(),
                initialization_options: None,
                partial: None,
                unanswerable: None,
            },
        );
        for (key, word) in super::TOOL_LIST_KEYS {
            let event = every_key()
                .into_iter()
                .find(|event| label(*event) == key)
                .unwrap_or_else(|| panic!("no key spells {key}"));
            assert!(
                answers(&listing, &[event]),
                "the box offers {key} for {word} and the list does nothing with it"
            );
        }
        let closed = State {
            modal: crate::Modal::None,
            ..listing.clone()
        };
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| answers(&listing, &[*event]) && !answers(&closed, &[*event]))
            .map(label)
            .filter(|label| {
                !super::TOOL_LIST_KEYS.iter().any(|(key, _)| key == label) && !label.contains("arr")
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "Tools answers keys its box does not name: {unnamed:?}"
        );
    }

    #[test]
    fn the_branch_picker_answers_exactly_the_keys_its_box_names() {
        let listing = State {
            modal: crate::Modal::Branches {
                refs: vec![
                    crate::story::BranchRef {
                        name: "feature".to_string(),
                        remote: false,
                        when: 2,
                    },
                    crate::story::BranchRef {
                        name: "main".to_string(),
                        remote: false,
                        when: 1,
                    },
                ],
                filter: String::new(),
                row: 0,
            },
            ..editing()
        };
        for (key, word) in super::BRANCH_LIST_KEYS {
            let event = every_key()
                .into_iter()
                .find(|event| label(*event) == key)
                .unwrap_or_else(|| panic!("no key spells {key}"));
            assert!(
                answers(&listing, &[event]),
                "the box offers {key} for {word} and the picker does nothing with it"
            );
        }
        let closed = State {
            modal: crate::Modal::None,
            ..listing.clone()
        };
        let gesture = |event| {
            let events = on_key_event(&listing, &mut Drafts::default(), event, 0);
            !events.is_empty() && !events.iter().all(|e| matches!(e, Event::FilterBranches(_)))
        };
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| gesture(*event) && !answers(&closed, &[*event]))
            .map(label)
            .filter(|label| {
                !super::BRANCH_LIST_KEYS.iter().any(|(key, _)| key == label)
                    && !label.contains("arr")
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "the branch picker answers keys its box does not name: {unnamed:?}"
        );
    }

    #[test]
    fn the_launch_list_answers_exactly_the_keys_its_box_names() {
        let mut listing = State {
            modal: crate::Modal::Launches { row: 0 },
            ..editing()
        };
        listing.launches.insert(
            "app".to_string(),
            crate::startup::Launch {
                adapter: "rust".to_string(),
                request: "launch".to_string(),
                args: serde_json::Map::new(),
                reattach: true,
            },
        );
        for (key, word) in super::LAUNCH_LIST_KEYS {
            let event = every_key()
                .into_iter()
                .find(|event| label(*event) == key)
                .unwrap_or_else(|| panic!("no key spells {key}"));
            assert!(
                answers(&listing, &[event]),
                "the box offers {key} for {word} and the list does nothing with it"
            );
        }
        let closed = State {
            modal: crate::Modal::None,
            ..listing.clone()
        };
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| {
                !on_key_event(&listing, &mut Drafts::default(), *event, 0).is_empty()
                    && !answers(&closed, &[*event])
            })
            .map(label)
            .filter(|label| {
                !super::LAUNCH_LIST_KEYS.iter().any(|(key, _)| key == label)
                    && !label.contains("arr")
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "the launch list answers keys its box does not name: {unnamed:?}"
        );
    }

    #[test]
    fn the_debug_keys_are_reserved_only_while_a_session_exists() {
        let shell = State {
            focus: Pane::Terminal,
            ..State::default()
        };
        let debugging = State {
            focus: Pane::Terminal,
            ..crate::debug::paused(State::default())
        };
        let shift = |code| KeyEvent::new(code).modifiers(KeyModifiers::SHIFT);
        let claimed = [
            (KeyEvent::new(KeyCode::F(9)), Event::DebugResume),
            (
                KeyEvent::new(KeyCode::F(2)).modifiers(KeyModifiers::CTRL),
                Event::DebugStop,
            ),
            (
                KeyEvent::new(KeyCode::F(8)),
                Event::DebugStep(crate::debug::Step::Over),
            ),
            (
                KeyEvent::new(KeyCode::F(7)),
                Event::DebugStep(crate::debug::Step::Into),
            ),
            (
                shift(KeyCode::F(8)),
                Event::DebugStep(crate::debug::Step::Out),
            ),
        ];
        let press = |state: &State, key| on_key_event(state, &mut Drafts::default(), key, 0);
        for (key, event) in claimed {
            assert_eq!(press(&debugging, key), vec![event], "{key:?}");
            assert!(
                matches!(press(&shell, key)[..], [Event::Bytes(_)]),
                "{key:?} is withheld from the shell with no session"
            );
        }
    }

    #[test]
    fn stepping_mode_claims_its_letters_and_hands_every_other_key_back() {
        let stepping = State {
            stepping: true,
            ..crate::debug::paused(editing())
        };
        let press = |key| on_key_event(&stepping, &mut Drafts::default(), key, 0);
        for (key, event) in [
            ('n', Event::DebugStep(crate::debug::Step::Over)),
            ('i', Event::DebugStep(crate::debug::Step::Into)),
            ('o', Event::DebugStep(crate::debug::Step::Out)),
            ('c', Event::DebugResume),
        ] {
            assert_eq!(press(plain(key)), vec![event], "{key}");
        }
        let plainly = |key| on_key_event(&editing(), &mut Drafts::default(), plain(key), 0);
        for key in ['j', '/', 'q'] {
            let events = press(plain(key));
            assert_eq!(events.first(), Some(&Event::LeaveStepping), "{key}");
            assert_eq!(events[1..], plainly(key), "{key} is not what it always is");
        }
        let shell = State {
            focus: Pane::Terminal,
            ..stepping
        };
        let in_shell = on_key_event(&shell, &mut Drafts::default(), plain('n'), 0);
        assert_eq!(in_shell.first(), Some(&Event::LeaveStepping));
        assert!(matches!(in_shell[1..], [Event::Bytes(_)]));
    }

    #[test]
    fn the_arrange_mode_claims_its_motions_and_hands_every_other_key_back() {
        let mut open = crate::debug::paused(editing());
        crate::debug::open_evaluator(&mut open, "count".to_string());
        let arranging = |how| State {
            arranging: Some(how),
            ..open.clone()
        };
        let moving = arranging(crate::debug::Arrange::Moving);
        let press = |state: &State, key| on_key_event(state, &mut Drafts::default(), key, 0);
        for (key, direction) in [
            (plain('h'), Direction::Left),
            (plain('j'), Direction::Down),
            (plain('k'), Direction::Up),
            (plain('l'), Direction::Right),
            (KeyEvent::new(KeyCode::Right), Direction::Right),
        ] {
            assert_eq!(
                press(&moving, key),
                vec![Event::MoveEvaluator(direction)],
                "{key:?}"
            );
        }
        assert_eq!(
            press(&arranging(crate::debug::Arrange::Sizing), plain('j')),
            vec![Event::ResizeEvaluator(Direction::Down)]
        );
        let plainly = |key| on_key_event(&open, &mut Drafts::default(), key, 0);
        for key in [plain('i'), plain('x'), KeyEvent::new(KeyCode::Esc)] {
            let events = press(&moving, key);
            assert_eq!(events.first(), Some(&Event::LeaveArranging), "{key:?}");
            assert_eq!(
                events[1..],
                plainly(key),
                "{key:?} is not what it always is"
            );
        }
    }

    #[test]
    fn the_pickers_hint_is_true_of_a_letter_and_of_backspace() {
        let listing = |filter: &str| State {
            modal: crate::Modal::Branches {
                refs: vec![crate::story::BranchRef {
                    name: "feature".to_string(),
                    remote: false,
                    when: 1,
                }],
                filter: filter.to_string(),
                row: 0,
            },
            ..editing()
        };
        assert!(super::BRANCH_FILTER_HINT.contains("type"));
        assert_eq!(
            press(&listing(""), plain('f')),
            vec![Event::FilterBranches("f".to_string())]
        );
        assert_eq!(
            press(&listing("fe"), KeyEvent::new(KeyCode::Backspace)),
            vec![Event::FilterBranches("f".to_string())]
        );
        assert_eq!(
            press(&listing(""), KeyEvent::new(KeyCode::Backspace)),
            vec![Event::FilterBranches(String::new())]
        );
    }

    #[test]
    fn the_results_box_answers_exactly_the_keys_its_helper_row_names() {
        let hit = |file: &str| crate::search::Hit {
            file: file.to_string(),
            line: 1,
            column: 1,
            text: "update".to_string(),
        };
        let showing = State {
            search: Some(crate::Search {
                query: crate::editor::Buffer::text_box("upd"),
                results: crate::search::Results {
                    hits: vec![hit("a.rs"), hit("b.rs")],
                    ..Default::default()
                },
                ..crate::Search::default()
            }),
            ..editing()
        };
        let gesture = |event| {
            let events = press(&showing, event);
            !events.is_empty()
                && !events.iter().all(|e| {
                    matches!(
                        e,
                        Event::EditorKey(_)
                            | Event::EditorBackspace
                            | Event::EditorDeleteWord
                            | Event::EditorArrow(_)
                            | Event::EditorWord(_)
                            | Event::QueryEnd(_)
                    )
                })
        };
        let named = |label: &str| {
            super::SEARCH_KEYS
                .iter()
                .any(|(keys, _)| keys.split_whitespace().any(|key| key == label))
        };
        for (keys, word) in super::SEARCH_KEYS {
            for key in keys.split_whitespace() {
                let spelled: Vec<KeyEvent> = every_key()
                    .into_iter()
                    .filter(|event| label(*event) == key)
                    .collect();
                assert!(!spelled.is_empty(), "no key spells {key}");
                assert!(
                    spelled.into_iter().any(gesture),
                    "the helper row offers {key} for {word} and the box does nothing with it"
                );
            }
        }
        for key in ["arr", "C-n", "C-p"] {
            assert!(named(key), "the helper row does not name {key}");
        }
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| gesture(*event))
            .map(label)
            .filter(|label| {
                !named(label)
                    && !listed(&State::default(), label, View::Edit)
                    && !omitted_in(label, View::Edit)
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "the results box answers keys its helper row does not name: {unnamed:?}"
        );
    }

    #[test]
    fn a_waiting_space_answers_exactly_the_keys_the_chord_hint_names() {
        let arranging = {
            let mut state = crate::debug::paused(editing());
            crate::debug::open_evaluator(&mut state, String::new());
            state
        };
        for state in [editing(), crate::debug::paused(editing()), arranging] {
            let (waiting, _) = drive(&state, &mut Drafts::default(), &[plain(' ')]);
            assert_eq!(waiting.modal, crate::Modal::Chord, "the hint opens at once");
            let (cancelled, _) = drive(
                &waiting,
                &mut Drafts::default(),
                &[KeyEvent::new(KeyCode::Esc)],
            );
            assert_eq!(cancelled.modal, crate::Modal::None, "Escape cancels it");
            let chord = |event: KeyEvent| {
                let events = on_key_event(&waiting, &mut Drafts::default(), event, 0);
                let (after, acted) = drive(&waiting, &mut Drafts::default(), &[event]);
                matches!(events.as_slice(), [Event::Key(_)])
                    && (acted || settled(&after) != settled(&cancelled))
            };
            let mut answered: Vec<String> = every_key()
                .into_iter()
                .filter(|event| chord(*event))
                .map(label)
                .collect();
            answered.sort();
            answered.dedup();
            let mut named: Vec<String> = super::chord_rows(&waiting)
                .into_iter()
                .filter_map(|(key, _)| key.map(String::from))
                .collect();
            named.sort();
            assert_eq!(answered, named);
        }
    }

    #[test]
    fn space_while_inserting_is_a_space() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        let (after, _) = drive(&inserting, &mut Drafts::default(), &[plain(' ')]);
        assert_eq!(after.modal, crate::Modal::None);
        assert!(crate::current_buffer(&after).is_some_and(|buffer| buffer.is_dirty()));
    }

    #[test]
    fn the_omissions_list_holds_only_live_unlisted_bindings() {
        for (view, state) in views() {
            let bound: Vec<String> = candidates(&state)
                .into_iter()
                .filter(|(_, sequence)| sequence_answers(&state, sequence))
                .map(|(label, _)| label)
                .collect();
            for (omitted, _, omitted_views) in UNLISTED {
                if !super::applies_to(omitted_views, view) {
                    continue;
                }
                assert!(
                    bound.iter().any(|label| label == omitted),
                    "{omitted} is excused as an omission in {view:?} but does nothing there"
                );
                assert!(
                    !listed(&State::default(), omitted, view),
                    "{omitted} is both listed and omitted in {view:?}"
                );
            }
        }
    }

    fn with_a_selection(state: &State) -> State {
        let mut selected = state.clone();
        selected.selection = Some(Selection::Screen {
            pane: Pane::Editor,
            from: Place { line: 0, column: 0 },
            to: Place { line: 0, column: 1 },
            text: "x".to_string(),
        });
        selected
    }

    fn with_an_edit(state: &State) -> State {
        crate::update(state, Event::EditorKey('x')).0
    }

    #[test]
    fn a_speed_nobody_can_mean_is_not_a_command() {
        assert_eq!(command("speed 1.4"), vec![Event::SetSpeed(1.4)]);
        assert_eq!(command("split"), vec![Event::SplitTerminal]);
        assert_eq!(command("speed 0"), vec![]);
        assert_eq!(command("speed -3"), vec![]);
        assert_eq!(command("speed fast"), vec![]);
    }

    fn typed_command(token: &str) -> Vec<KeyEvent> {
        let mut sequence: Vec<KeyEvent> = token.chars().map(plain).collect();
        sequence.push(KeyEvent::new(KeyCode::Enter));
        sequence
    }

    #[test]
    fn every_listed_key_answers_in_a_view_it_claims() {
        for (view, state) in views() {
            let extra = [
                with_a_selection(&state),
                with_an_edit(&state),
                filling_in_a_snippet(&state),
            ];
            let states: Vec<&State> = std::iter::once(&state).chain(extra.iter()).collect();
            let answered: Vec<String> = states
                .iter()
                .flat_map(|s| candidates(s))
                .filter(|(_, sequence)| states.iter().any(|s| answers(s, sequence)))
                .map(|(label, _)| label)
                .collect();
            for (keys, what, views) in super::cheatsheet(&state) {
                if !super::applies_to(views, view) {
                    continue;
                }
                let claims = keys.split_whitespace().any(|token| {
                    if token.starts_with(':') {
                        let sequence = typed_command(token);
                        states.iter().any(|s| answers(s, &sequence))
                    } else {
                        answered.iter().any(|label| names(token, label))
                    }
                });
                assert!(
                    claims,
                    "\"{keys}\" ({what}) is listed for {view:?} but nothing in it answers there"
                );
            }
        }
    }

    #[test]
    fn every_listed_chord_answers_in_a_view_it_claims() {
        for (view, state) in views() {
            let extra = [
                with_a_selection(&state),
                with_an_edit(&state),
                filling_in_a_snippet(&state),
            ];
            let states: Vec<&State> = std::iter::once(&state).chain(extra.iter()).collect();
            for (keys, what, views) in super::cheatsheet(&state) {
                if !super::applies_to(views, view) {
                    continue;
                }
                for token in keys.split_whitespace() {
                    let mut spelling = token.chars();
                    let (Some(operator), Some(second), None) =
                        (spelling.next(), spelling.next(), spelling.next())
                    else {
                        continue;
                    };
                    if !OPERATORS.contains(&operator) {
                        continue;
                    }
                    assert!(
                        states
                            .iter()
                            .any(|s| chord_answers(s, plain(operator), plain(second))),
                        "\"{token}\" of \"{keys}\" ({what}) is listed for {view:?} and answers nothing there"
                    );
                }
            }
        }
    }
}
