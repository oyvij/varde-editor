use crate::layout::{self, Area, Layout};
use crate::{tree, Direction, Event, FindKeys, Modal, Pane, Place, Pointed, ReplaceField, State};
use terminput::KeyModifiers;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    #[default]
    None,
    Legacy,
    Sgr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Click,
    Wheel(Direction),
}

/// Legacy fields are one byte offset by 32, so 223 is the last cell it can express
const LEGACY_LIMIT: usize = 223;

pub fn report(encoding: Encoding, gesture: Gesture, at: Place) -> Option<Vec<u8>> {
    let button: u8 = match gesture {
        Gesture::Click => 0,
        Gesture::Wheel(Direction::Up) => 64,
        Gesture::Wheel(Direction::Down) => 65,
        Gesture::Wheel(Direction::Left) => 66,
        Gesture::Wheel(Direction::Right) => 67,
    };
    let released = matches!(gesture, Gesture::Click);
    match encoding {
        Encoding::None => None,
        Encoding::Sgr => {
            let (column, line) = (at.column, at.line);
            let mut bytes = format!("\x1b[<{button};{column};{line}M").into_bytes();
            if released {
                bytes.extend_from_slice(format!("\x1b[<{button};{column};{line}m").as_bytes());
            }
            Some(bytes)
        }
        Encoding::Legacy => {
            let (column, line) = (offset(at.column)?, offset(at.line)?);
            let mut bytes = vec![0x1b, b'[', b'M', 32 + button, column, line];
            if released {
                // The legacy encoding has no per-button release: button 3 means release
                bytes.extend_from_slice(&[0x1b, b'[', b'M', 32 + 3, column, line]);
            }
            Some(bytes)
        }
    }
}

fn offset(coordinate: usize) -> Option<u8> {
    match (1..=LEGACY_LIMIT).contains(&coordinate) {
        true => Some(32 + coordinate as u8),
        false => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    LeftDown,
    LeftDrag,
    LeftUp,
    RightDown,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
    Moved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    pub kind: Kind,
    pub column: u16,
    pub row: u16,
    pub modifiers: KeyModifiers,
}

/// Mouse reports have no Super bit, so Cmd+click arrives unmodified on most terminals; Ctrl too
fn jumping(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::CTRL)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub pane: Pane,
    pub from: Place,
    pub to: Place,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    Tree,
    Ai,
    Strip,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Grab {
    took: Took,
    window: Area,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Took {
    Title,
    Edge(Edge),
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
    Bottom,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pointer {
    pub dragging: Option<Divider>,
    pub drag_from: Option<(u16, u16)>,
    pub dragged: bool,
    pub at_ms: u64,
    last_press: Option<(u16, u16, u64)>,
    pane: Option<Pane>,
    anchor: Option<Place>,
    grab: Option<Grab>,
    pub held: Option<Input>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    pub events: Vec<Event>,
    pub select: Option<Selection>,
    pub link: Option<(Pane, Place)>,
}

impl Outcome {
    fn of(events: Vec<Event>) -> Self {
        Self {
            events,
            select: None,
            link: None,
        }
    }
}

pub fn on_mouse(state: &State, panes: &Layout, pointer: &mut Pointer, input: Input) -> Outcome {
    if input.kind == Kind::Moved {
        // A terminal may never report a release outside its window; buttonless motion ends the drag
        pointer.held = None;
        let mut events = vec![Event::PointerMoved(resting(state, panes, input))];
        events.extend(hovered(state, panes, input));
        let icon = action_under(state, panes, input);
        if icon != state.hovered_action {
            events.push(Event::HoverAction(icon));
        }
        let on_minimap = crate::minimap::strip(state, panes.editor).holds(input.column, input.row);
        if on_minimap != state.hovered_minimap {
            events.push(Event::HoverMinimap(on_minimap));
        }
        return Outcome::of(events);
    }
    let notch = match input.kind {
        Kind::ScrollUp => Some(Direction::Up),
        Kind::ScrollDown => Some(Direction::Down),
        Kind::ScrollLeft => Some(Direction::Left),
        Kind::ScrollRight => Some(Direction::Right),
        _ => None,
    };
    if let Some(direction) = notch {
        let moved: Option<fn(Direction) -> Event> = match state.modal {
            Modal::Tools { .. } => Some(Event::MoveToolRow),
            Modal::Launches { .. } => Some(Event::MoveLaunchRow),
            Modal::Branches { .. } => Some(Event::MoveBranchRow),
            _ => None,
        };
        if let Some(moved) = moved {
            return Outcome::of(match direction {
                Direction::Up | Direction::Down => vec![moved(direction)],
                Direction::Left | Direction::Right => vec![],
            });
        }
    }
    if let Some(search) = state.search.as_ref() {
        if !matches!(
            input.kind,
            Kind::ScrollUp | Kind::ScrollDown | Kind::ScrollLeft | Kind::ScrollRight
        ) {
            *pointer = Pointer::default();
            return Outcome::of(result_click(state, search, input));
        }
    }
    let wheel = match input.kind {
        Kind::ScrollUp => Some(Direction::Up),
        Kind::ScrollDown => Some(Direction::Down),
        _ => None,
    };
    if let Some(direction) = wheel.filter(|_| on_hover(state, panes, input)) {
        return Outcome::of(vec![Event::ScrollHover(direction)]);
    }
    if input.kind == Kind::LeftDown && input.row == panes.terminal.y {
        if let Some(group) = group_tab_at(state, panes, input.column) {
            return Outcome::of(vec![Event::ShowGroup(group)]);
        }
        if let Some(action) = strip_chip_at(state, panes, input.column) {
            return Outcome::of(vec![Event::PaneAction(action)]);
        }
    }
    if pointer.pane.is_none() && panes.evaluator.holds(input.column, input.row) {
        return in_pane(state, panes, pointer, Pane::Evaluator, input);
    }
    if let Some(outcome) = divider_drag(panes, pointer, input) {
        return outcome;
    }
    let pane = match (input.kind, pointer.pane) {
        (Kind::LeftDrag | Kind::LeftUp, Some(pane)) => pane,
        _ => match layout::pane_at(panes, input.column, input.row) {
            Some(pane) => pane,
            None => return Outcome::default(),
        },
    };
    in_pane(state, panes, pointer, pane, input)
}

fn result_click(state: &State, search: &crate::Search, input: Input) -> Vec<Event> {
    let area = layout::search_box(state.screen_width, state.screen_height);
    let top = area.y + 1 + layout::SEARCH_HEADER;
    let offset = match input.kind {
        Kind::LeftDown if input.row >= top && area.holds(input.column, input.row) => {
            usize::from(input.row - top)
        }
        _ => return vec![],
    };
    if offset >= layout::search_hit_rows(state.screen_width, state.screen_height) {
        return vec![];
    }
    match crate::search::rows(&search.results).get(offset + search.scroll) {
        Some(crate::search::Row::Hit(index)) => vec![Event::SelectHit(*index)],
        _ => vec![],
    }
}

fn group_tab_at(state: &State, panes: &Layout, column: u16) -> Option<crate::layout::Group> {
    let strip = panes.strip();
    if !strip.holds(column, strip.y) {
        return None;
    }
    let labels = crate::group_labels(state);
    let index = crate::layout::strip_at(strip, &labels, column)?;
    Some(crate::group_tabs(state)[index].group)
}

fn strip_chip_at(state: &State, panes: &Layout, column: u16) -> Option<&'static str> {
    if !crate::showing_transport(state) {
        return None;
    }
    let chips = crate::debug::strip_transport(state);
    let area = crate::transport_area(state, panes.strip());
    let labels = crate::layout::chip_labels(&chips, area.width, crate::layout::CORNER_TITLE);
    Some(chips[crate::layout::strip_at(area, &labels, column)?].action)
}

fn divider_drag(panes: &Layout, pointer: &mut Pointer, input: Input) -> Option<Outcome> {
    let tree_edge = panes.tree.x + panes.tree.width.saturating_sub(1);
    let ai_edge = panes.ai.x;
    let screen = panes.ai.right();
    let beside_tree = input.row < panes.terminal.y;
    let beside_ai = input.row < panes.ai.bottom();
    let above_strip = input.row == panes.terminal.y && panes.strip().holds(input.column, input.row);
    let beside_output = panes.output.width > 0
        && input.row > panes.output.y
        && input.row < panes.output.bottom()
        && input.column.abs_diff(panes.output.x) <= 1;
    match input.kind {
        Kind::LeftDown if beside_tree && input.column.abs_diff(tree_edge) <= 1 => {
            pointer.dragging = Some(Divider::Tree);
            Some(Outcome::default())
        }
        Kind::LeftDown if beside_ai && input.column.abs_diff(ai_edge) <= 1 => {
            pointer.dragging = Some(Divider::Ai);
            Some(Outcome::default())
        }
        Kind::LeftDown if beside_output => {
            pointer.dragging = Some(Divider::Output);
            Some(Outcome::default())
        }
        Kind::LeftDown if above_strip => {
            pointer.dragging = Some(Divider::Strip);
            Some(Outcome::default())
        }
        Kind::LeftDrag if pointer.dragging == Some(Divider::Output) => {
            Some(Outcome::of(vec![Event::DragOutput(u32::from(
                panes.output.right().saturating_sub(input.column),
            ))]))
        }
        Kind::LeftDrag if pointer.dragging == Some(Divider::Strip) => {
            Some(Outcome::of(vec![Event::DragStrip(u32::from(
                panes.terminal.bottom().saturating_sub(input.row),
            ))]))
        }
        Kind::LeftDrag if pointer.dragging == Some(Divider::Tree) => {
            let width = input.column.saturating_sub(panes.tree.x) + 1;
            let most = screen.saturating_sub(30).max(12);
            Some(Outcome::of(vec![Event::DragDivider(u32::from(
                width.clamp(12, most),
            ))]))
        }
        Kind::LeftDrag if pointer.dragging == Some(Divider::Ai) => {
            let width = panes.ai.right().saturating_sub(input.column);
            let most = screen
                .saturating_sub(panes.tree.width + panes.step_menu.width + 20)
                .max(12);
            Some(Outcome::of(vec![Event::DragAiDivider(u32::from(
                width.clamp(12, most),
            ))]))
        }
        Kind::LeftUp => {
            pointer.dragging = None;
            None
        }
        _ => None,
    }
}

fn in_pane(
    state: &State,
    panes: &Layout,
    pointer: &mut Pointer,
    pane: Pane,
    input: Input,
) -> Outcome {
    match input.kind {
        Kind::LeftDown => {
            pointer.drag_from = Some((input.column, input.row));
            pointer.pane = Some(pane);
            pointer.anchor = None;
            pointer.grab = None;
            pointer.held = None;
            pointer.dragged = false;
            let doubled = pointer.last_press.is_some_and(|(column, row, ms)| {
                (column, row) == (input.column, input.row)
                    && pointer.at_ms.saturating_sub(ms) <= state.double_tap_ms
            });
            pointer.last_press = Some((input.column, input.row, pointer.at_ms));
            let mut events = pressed(state, panes, pointer, pane, input);
            if let (true, [Event::ClickText(at)]) = (doubled, events.as_slice()) {
                events.push(Event::DoubleClickText(*at));
            }
            Outcome::of(events)
        }
        Kind::RightDown => Outcome::of(vec![Event::RightClick(pane)]),
        Kind::ScrollUp => Outcome::of(vec![Event::Scroll {
            pane,
            direction: Direction::Up,
            at: place_in(state, panes, pane, (input.column, input.row)),
        }]),
        Kind::ScrollDown => Outcome::of(vec![Event::Scroll {
            pane,
            direction: Direction::Down,
            at: place_in(state, panes, pane, (input.column, input.row)),
        }]),
        Kind::ScrollLeft => Outcome::of(vec![Event::Scroll {
            pane,
            direction: Direction::Left,
            at: place_in(state, panes, pane, (input.column, input.row)),
        }]),
        Kind::ScrollRight => Outcome::of(vec![Event::Scroll {
            pane,
            direction: Direction::Right,
            at: place_in(state, panes, pane, (input.column, input.row)),
        }]),
        Kind::LeftUp => {
            let tapped = !pointer.dragged;
            pointer.drag_from = None;
            pointer.pane = None;
            pointer.anchor = None;
            pointer.grab = None;
            pointer.held = None;
            pointer.dragged = false;
            let at = place_in(state, panes, pane, (input.column, input.row));
            let hosted = matches!(pane, Pane::Terminal | Pane::Ai);
            match (tapped, hosted && jumping(input.modifiers)) {
                (true, true) => Outcome {
                    events: Vec::new(),
                    select: None,
                    link: Some((pane, at)),
                },
                (true, false) => Outcome::of(vec![Event::ClickThrough { pane, at }]),
                (false, _) => Outcome::default(),
            }
        }
        Kind::LeftDrag => {
            pointer.dragged = true;
            dragged(state, panes, pointer, pane, input)
        }
        Kind::Moved => Outcome::default(),
    }
}

fn hovered(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    let hosted = match layout::pane_at(panes, input.column, input.row) {
        Some(pane @ (Pane::Terminal | Pane::Ai))
            if text_area(state, panes, pane).holds(input.column, input.row) =>
        {
            Some(pane)
        }
        _ => None,
    };
    let at = match (
        jumping(input.modifiers),
        resting(state, panes, input),
        hosted,
    ) {
        (true, Pointed::Text(at), _) => Some((Pane::Editor, at)),
        (true, _, Some(pane)) => Some((
            pane,
            place_in(state, panes, pane, (input.column, input.row)),
        )),
        _ => None,
    };
    match at == state.link {
        true => vec![],
        false => vec![Event::HoverLink(at)],
    }
}

fn pressed(
    state: &State,
    panes: &Layout,
    pointer: &mut Pointer,
    pane: Pane,
    input: Input,
) -> Vec<Event> {
    if pane == Pane::Evaluator {
        return pressed_in_evaluator(state, panes, pointer, input);
    }
    if let Some(events) = pressed_in_hover(state, panes, input) {
        return events;
    }
    if let Some(events) = pressed_in_find(state, panes, input).filter(|_| pane == Pane::Editor) {
        return events;
    }
    if let Some(key) = palette_entry_at(state, panes, input.column, input.row) {
        return vec![Event::ClickPaletteEntry(key)];
    }
    let screen = (panes.ai.right(), panes.tree.height + panes.terminal.height);
    if let Some(action) = crate::run::chip_at(state, screen.0, screen.1, input.column, input.row) {
        return vec![Event::ChooseRun(action)];
    }
    if let Some(events) = crate::launch::clicked(state, screen.0, screen.1, input.column, input.row)
    {
        return events;
    }
    if let Some(index) = buffer_dot_at(state, panes, input.column, input.row) {
        return match state.buffers.keys().nth(index) {
            Some(path) => vec![Event::ShowBuffer(path.clone())],
            None => vec![],
        };
    }
    if pane == Pane::Tree && input.row == panes.tree.y {
        if let Some(severity) = nudge_at(state, panes, input.column) {
            return vec![Event::ShowDiagnostics(severity)];
        }
    }
    let row_index = row_index(state, panes, input.row);
    match (pane, action_at(state, panes, input.column, input.row)) {
        (Pane::Tree, Some(action)) => vec![Event::RowAction(action)],
        (Pane::Editor, _) if input.row == panes.editor.y => {
            match transport_at(state, panes, input.column) {
                Some(action) => vec![Event::PaneAction(action)],
                None => vec![Event::ClickPane(Pane::Editor)],
            }
        }
        (Pane::Editor, _)
            if crate::minimap::strip(state, panes.editor).holds(input.column, input.row) =>
        {
            vec![Event::DragMinimap(minimap_row(
                crate::minimap::strip(state, panes.editor),
                input.row,
            ))]
        }
        (Pane::Editor, _)
            if crate::debug::edit_chip(state, panes) == Some((input.column, input.row)) =>
        {
            vec![Event::EditBreakpoint(
                crate::current_buffer(state).map_or(0, |buffer| buffer.line),
            )]
        }
        (Pane::Editor, _) if breakpoint_column(state, panes, input) => {
            let at = place_in(state, panes, pane, (input.column, input.row));
            let offered = !crate::debug::marks(state).contains_key(&at.line)
                && crate::run::marks(state).contains_key(&at.line);
            match offered {
                true => vec![Event::OfferRun(at.line)],
                false => vec![Event::ToggleBreakpoint(at.line)],
            }
        }
        (Pane::Editor, _) if fold_toggle_at(state, panes, input) => {
            let at = place_in(state, panes, pane, (input.column, input.row));
            vec![Event::ClickText(at), Event::ToggleFold { all: false }]
        }
        (Pane::Editor, _) if conflict_button(state, panes, input).is_some() => {
            let at = place_in(state, panes, pane, (input.column, input.row));
            conflict_button(state, panes, input)
                .into_iter()
                .flat_map(|side| [Event::ClickText(at), Event::AcceptConflict(side)])
                .collect()
        }
        (Pane::Editor, _) if state.diff.is_none() && state.current_buffer.is_some() => {
            let at = place_in(state, panes, pane, (input.column, input.row));
            let mut events = vec![Event::ClickText(at)];
            if jumping(input.modifiers) {
                events.push(Event::AskDefinition);
            }
            events
        }
        (Pane::Tree, None) => match tree::visible_rows(state).get(row_index) {
            Some(row) => vec![Event::ClickRow(row.path.clone())],
            None => vec![Event::ClickPane(pane)],
        },
        (Pane::Risk, _) => pressed_in_risk(state, panes, input),
        (Pane::Buffers, _) => pressed_in_buffers(state, panes, input),
        (Pane::History, _) => pressed_in_history(state, panes, input),
        (Pane::Breakpoints, _) => pressed_in_breakpoints(state, panes, input),
        (Pane::Diagnostics, _) => pressed_in_diagnostics(state, panes, input),
        (Pane::Conflicts, _) => {
            let index = list_row(panes.corner, input.row, state.conflicts_scroll);
            let on_a_row = input.row > panes.corner.y
                && input.row < panes.corner.bottom().saturating_sub(1)
                && index < crate::conflict::listed(state).len();
            match on_a_row {
                true => vec![Event::ClickConflictRow(index)],
                false => vec![Event::ClickPane(Pane::Conflicts)],
            }
        }
        (Pane::Frames, _) => {
            let index = list_row(panes.corner, input.row, state.frames_scroll);
            let on_a_row = input.row > panes.corner.y
                && input.row < panes.corner.bottom().saturating_sub(1)
                && index < crate::debug::frame_rows(state).len();
            match on_a_row {
                true => vec![Event::ClickFrameRow(index)],
                false => vec![Event::ClickPane(Pane::Frames)],
            }
        }
        (Pane::Variables, _) => {
            let index = list_row(panes.terminal, input.row, state.variables_scroll);
            let on_a_row = input.row > panes.terminal.y
                && input.row < panes.terminal.bottom().saturating_sub(1)
                && index < crate::debug::variables(state).len();
            if on_a_row {
                if let Some(action) = variables_chip_at(state, panes, input.column, index) {
                    return vec![Event::RowAction(action)];
                }
            }
            match on_a_row {
                true => vec![Event::ClickVariablesRow(index)],
                false => vec![Event::ClickPane(Pane::Variables)],
            }
        }
        (Pane::Terminal, _) => vec![Event::FocusSplit(crate::layout::split_at(
            panes.terminal,
            state.terminals.len(),
            input.column,
        ))],
        _ => vec![Event::ClickPane(pane)],
    }
}

fn pressed_in_risk(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    if input.row == panes.corner.y {
        return match pane_action_at(state, panes, input.column) {
            Some(action) => vec![Event::PaneAction(action)],
            None => vec![Event::ClickPane(Pane::Risk)],
        };
    }
    if input.row >= panes.corner.bottom().saturating_sub(1) {
        return vec![Event::ClickPane(Pane::Risk)];
    }
    let index = list_row(panes.corner, input.row, state.risk_scroll);
    if let Some(action) = risk_action_at(state, panes, input.column, index) {
        return vec![Event::RowAction(action)];
    }
    match crate::risk::list(state).len() > index {
        true => vec![Event::ClickRiskRow(index)],
        false => vec![Event::ClickPane(Pane::Risk)],
    }
}

fn pressed_in_buffers(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    let index = list_row(panes.corner, input.row, state.buffers_scroll);
    let inside = input.row > panes.corner.y && input.row < panes.corner.bottom().saturating_sub(1);
    match inside && state.buffers.len() > index {
        true => vec![Event::ClickBufferRow(index)],
        false => vec![Event::ClickPane(Pane::Buffers)],
    }
}

fn pressed_in_history(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    if input.row <= panes.corner.y || input.row >= panes.corner.bottom().saturating_sub(1) {
        return vec![Event::ClickPane(Pane::History)];
    }
    let index = list_row(panes.corner, input.row, state.history_scroll);
    if let Some(action) = history_action_at(state, panes, input.column, index) {
        return vec![Event::RowAction(action)];
    }
    match crate::history::list(state).len() > index {
        true => vec![Event::ClickHistoryRow(index)],
        false => vec![Event::ClickPane(Pane::History)],
    }
}

fn pressed_in_breakpoints(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    if input.row == panes.corner.y {
        return match breakpoint_chip_at(state, panes, input.column) {
            Some(action) => vec![Event::PaneAction(action)],
            None => vec![Event::ClickPane(Pane::Breakpoints)],
        };
    }
    if input.row >= panes.corner.bottom().saturating_sub(1) {
        return vec![Event::ClickPane(Pane::Breakpoints)];
    }
    let index = list_row(panes.corner, input.row, state.breakpoints_scroll);
    if let Some(action) = breakpoint_action_at(state, panes, input.column, index) {
        return vec![Event::RowAction(action)];
    }
    match crate::debug::rows(state) > index {
        true => vec![Event::ClickBreakpointRow(index)],
        false => vec![Event::ClickPane(Pane::Breakpoints)],
    }
}

fn pressed_in_evaluator(
    state: &State,
    panes: &Layout,
    pointer: &mut Pointer,
    input: Input,
) -> Vec<Event> {
    let window = panes.evaluator;
    if input.row == window.y {
        let labels = crate::debug::evaluator_labels(state, window.width);
        let chips = crate::debug::evaluator_chips(state);
        if let Some(chip) =
            layout::strip_at(window, &labels, input.column).and_then(|at| chips.get(at))
        {
            return vec![Event::RowAction(chip.action)];
        }
    }
    let (_, output) = layout::evaluator_split(window, snippet_rows(state));
    pointer.grab = grab_at(window, output, input).map(|took| Grab { took, window });
    if pointer.grab.is_none() && output.holds(input.column, input.row) {
        let index = (input.row - output.y) as usize;
        if index < crate::debug::evaluator_output(state).len() {
            return vec![Event::OpenEvaluatedRow(index)];
        }
    }
    vec![Event::ClickPane(Pane::Evaluator)]
}

fn snippet_rows(state: &State) -> Option<u16> {
    state
        .evaluator
        .as_ref()
        .and_then(|evaluator| evaluator.snippet_rows)
}

fn grab_at(window: Area, output: Area, input: Input) -> Option<Took> {
    let bottom = input.row + 1 == window.bottom();
    let left = input.column == window.x;
    let right = input.column + 1 == window.right();
    if input.row == window.y {
        return Some(Took::Title);
    }
    if bottom {
        return Some(Took::Edge(match (left, right) {
            (true, _) => Edge::BottomLeft,
            (_, true) => Edge::BottomRight,
            (false, false) => Edge::Bottom,
        }));
    }
    match (left, right, input.row + 1 == output.y) {
        (true, _, _) => Some(Took::Edge(Edge::Left)),
        (_, true, _) => Some(Took::Edge(Edge::Right)),
        (_, _, true) => Some(Took::Rule),
        _ => None,
    }
}

fn moved_by(at: u16, by: i32) -> u16 {
    (i32::from(at) + by).clamp(0, i32::from(u16::MAX)) as u16
}

fn conflict_button(state: &State, panes: &Layout, input: Input) -> Option<crate::conflict::Side> {
    let text = text_area(state, panes, Pane::Editor);
    if !text.holds(input.column, input.row) {
        return None;
    }
    let line = place_in(state, panes, Pane::Editor, (input.column, input.row)).line;
    let crate::conflict::Drawn::Bar(pieces) = crate::conflict::drawn(state, line)? else {
        return None;
    };
    let mut from = usize::from(input.column - text.x);
    pieces.into_iter().find_map(|(piece, side)| {
        let width = piece.chars().count();
        match from < width {
            true => Some(side),
            false => {
                from -= width;
                None
            }
        }
    })?
}

fn pressed_in_diagnostics(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    if input.row == panes.corner.y {
        let labels = crate::lsp::severity_labels(state, panes.corner.width);
        return match layout::strip_at(panes.corner, &labels, input.column) {
            Some(at) => vec![Event::ShowDiagnostics(crate::lsp::Severity::ALL[at])],
            None => vec![Event::ClickPane(Pane::Diagnostics)],
        };
    }
    let index = list_row(panes.corner, input.row, state.diagnostics_scroll);
    let on_a_row = input.row < panes.corner.bottom().saturating_sub(1)
        && index < crate::lsp::listed(state).len();
    match on_a_row {
        true => vec![Event::ClickDiagnosticRow(index)],
        false => vec![Event::ClickPane(Pane::Diagnostics)],
    }
}

fn nudge_at(state: &State, panes: &Layout, column: u16) -> Option<crate::lsp::Severity> {
    if column + 1 >= panes.tree.right() {
        return None;
    }
    let mut at = panes.tree.x + 1 + UnicodeWidthStr::width(tree::title(state).as_str()) as u16;
    crate::lsp::nudge(state)
        .into_iter()
        .find_map(|(severity, label)| {
            let count = label.trim_start();
            at += (label.len() - count.len()) as u16;
            let width = UnicodeWidthStr::width(count) as u16;
            let hit = column >= at && column < at + width;
            at += width;
            hit.then_some(severity)
        })
}

fn dragged(
    state: &State,
    panes: &Layout,
    pointer: &mut Pointer,
    pane: Pane,
    input: Input,
) -> Outcome {
    let from = *pointer.drag_from.get_or_insert((input.column, input.row));
    pointer.held = None;
    match pane {
        Pane::Tree => {
            let rows = tree::visible_rows(state);
            let text = text_area(state, panes, pane);
            let (vertical, _) = push(text, input);
            let at = row_index(state, panes, clamped(input.row, text.y, text.bottom()));
            let stepped = match vertical {
                Some(Direction::Up) => at.saturating_sub(1),
                Some(Direction::Down) => (at + 1).min(rows.len().saturating_sub(1)),
                Some(Direction::Left) | Some(Direction::Right) | None => at,
            };
            pointer.held = (stepped != at).then_some(input);
            Outcome::of(match rows.get(stepped) {
                Some(row) => vec![Event::DragRow(row.path.clone())],
                None => vec![],
            })
        }
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts
        | Pane::Variables
        | Pane::Cheatsheet => Outcome::default(),
        Pane::Evaluator => {
            let Some(grab) = pointer.grab else {
                let anchor = *pointer
                    .anchor
                    .get_or_insert_with(|| place_in(state, panes, pane, from));
                let at = place_in(state, panes, pane, (input.column, input.row));
                let (from, to) = order(anchor, at);
                return Outcome::of(vec![Event::DragText { from, to }]);
            };
            let window = grab.window;
            let across = i32::from(input.column) - i32::from(from.0);
            let down = i32::from(input.row) - i32::from(from.1);
            let placed = match grab.took {
                Took::Title => Area {
                    x: moved_by(window.x, across),
                    y: moved_by(window.y, down),
                    ..window
                },
                Took::Edge(Edge::Right) => Area {
                    width: moved_by(window.width, across),
                    ..window
                },
                Took::Edge(Edge::Left) => Area {
                    x: moved_by(window.x, across),
                    width: moved_by(window.width, -across),
                    ..window
                },
                Took::Edge(Edge::Bottom) => Area {
                    height: moved_by(window.height, down),
                    ..window
                },
                Took::Edge(Edge::BottomRight) => Area {
                    width: moved_by(window.width, across),
                    height: moved_by(window.height, down),
                    ..window
                },
                Took::Edge(Edge::BottomLeft) => Area {
                    x: moved_by(window.x, across),
                    width: moved_by(window.width, -across),
                    height: moved_by(window.height, down),
                    ..window
                },
                Took::Rule => {
                    let rows = input.row.saturating_sub(panes.evaluator.y + 1);
                    return Outcome::of(vec![Event::SizeSnippet(rows)]);
                }
            };
            Outcome::of(vec![Event::PlaceEvaluator(placed)])
        }
        Pane::Editor => {
            let strip = crate::minimap::strip(state, panes.editor);
            if strip.holds(from.0, from.1) {
                return Outcome::of(vec![Event::DragMinimap(minimap_row(strip, input.row))]);
            }
            if state.diff.is_some() && input.column < panes.editor.x + 1 + layout::GUTTER {
                return Outcome::of(match gutter_range(state, panes, from.1, input.row) {
                    Some((file, from_line, to_line)) => vec![Event::DragGutter {
                        file,
                        from_line,
                        to_line,
                    }],
                    None => vec![],
                });
            }
            if state.current_buffer.is_none() {
                return Outcome::default();
            }
            let text = text_area(state, panes, pane);
            let (vertical, horizontal) = match state.diff.is_some() {
                true => (None, None),
                false => push(text, input),
            };
            let (at, end) = match vertical.is_some() || horizontal.is_some() {
                true => {
                    let at = bounded(
                        state,
                        place_in(
                            state,
                            panes,
                            pane,
                            (
                                clamped(input.column, text.x, text.right()),
                                clamped(input.row, text.y, text.bottom()),
                            ),
                        ),
                    );
                    (at, bounded(state, nudge(at, vertical, horizontal)))
                }
                false => {
                    let at = place_in(state, panes, pane, (input.column, input.row));
                    (at, at)
                }
            };
            pointer.held = (end != at).then_some(input);
            let anchor = *pointer
                .anchor
                .get_or_insert_with(|| place_in(state, panes, pane, from));
            let mut events = Vec::new();
            if pointer.held.is_some() {
                events.push(Event::ClickText(end));
            }
            let (from, to) = order(anchor, end);
            events.push(Event::DragText { from, to });
            Outcome::of(events)
        }
        Pane::Terminal | Pane::Ai | Pane::Output => {
            let (from, to) = order(
                place_in(state, panes, pane, from),
                place_in(state, panes, pane, (input.column, input.row)),
            );
            Outcome {
                events: Vec::new(),
                select: Some(Selection { pane, from, to }),
                link: None,
            }
        }
    }
}

fn minimap_row(strip: Area, row: u16) -> u32 {
    u32::from(row.clamp(strip.y, strip.bottom().saturating_sub(1)) - strip.y)
}

fn order(start: Place, end: Place) -> (Place, Place) {
    match (start.line, start.column) <= (end.line, end.column) {
        true => (start, end),
        false => (end, start),
    }
}

/// The .max(first) stops clamp panicking when a pane is narrower than its own gutter
fn clamped(at: u16, first: u16, past: u16) -> u16 {
    at.clamp(first, past.saturating_sub(1).max(first))
}

fn push(text: Area, input: Input) -> (Option<Direction>, Option<Direction>) {
    let vertical = match (input.row <= text.y, input.row + 1 >= text.bottom()) {
        (true, _) => Some(Direction::Up),
        (false, true) => Some(Direction::Down),
        (false, false) => None,
    };
    let horizontal = match (input.column <= text.x, input.column + 1 >= text.right()) {
        (true, _) => Some(Direction::Left),
        (false, true) => Some(Direction::Right),
        (false, false) => None,
    };
    (vertical, horizontal)
}

fn bounded(state: &State, at: Place) -> Place {
    let lines: Vec<String> = match crate::previewing(state) {
        true => crate::preview_rows(state)
            .iter()
            .map(crate::preview::Row::text)
            .collect(),
        false => match crate::current_buffer(state) {
            Some(buffer) => buffer.shown().split('\n').map(str::to_string).collect(),
            None => return at,
        },
    };
    let line = at.line.clamp(1, lines.len().max(1));
    Place {
        line,
        column: at.column.min(
            lines
                .get(line - 1)
                .map_or(1, |text| text.chars().count() + 1),
        ),
    }
}

fn nudge(at: Place, vertical: Option<Direction>, horizontal: Option<Direction>) -> Place {
    Place {
        line: match vertical {
            Some(Direction::Up) => at.line.saturating_sub(1).max(1),
            Some(Direction::Down) => at.line + 1,
            Some(Direction::Left) | Some(Direction::Right) | None => at.line,
        },
        column: match horizontal {
            Some(Direction::Left) => at.column.saturating_sub(1).max(1),
            Some(Direction::Right) => at.column + 1,
            Some(Direction::Up) | Some(Direction::Down) | None => at.column,
        },
    }
}

fn resting(state: &State, panes: &Layout, input: Input) -> Pointed {
    if on_hover(state, panes, input) {
        return Pointed::Hover;
    }
    if state.diff.is_some() || state.current_buffer.is_none() || crate::previewing(state) {
        return Pointed::Elsewhere;
    }
    if layout::pane_at(panes, input.column, input.row) != Some(Pane::Editor)
        || input.row <= panes.editor.y
    {
        return Pointed::Elsewhere;
    }
    let at = place_in(state, panes, Pane::Editor, (input.column, input.row));
    match breakpoint_column(state, panes, input) {
        true => Pointed::Breakpoint(at.line),
        false => Pointed::Text(at),
    }
}

fn pressed_in_hover(state: &State, panes: &Layout, input: Input) -> Option<Vec<Event>> {
    let spot = hover_spot(state, panes).filter(|spot| spot.holds(input.column, input.row))?;
    let chips = crate::debug::hover_chips(state);
    if chips.is_empty() {
        return None;
    }
    if input.row == spot.y {
        let labels = crate::debug::hover_labels(state, spot.width);
        return Some(match crate::layout::strip_at(spot, &labels, input.column) {
            Some(at) => vec![Event::HoverChip(chips[at].action)],
            None => Vec::new(),
        });
    }
    let first = state.hover.as_ref()?.first;
    let row = usize::from(input.row.saturating_sub(spot.y + 1)) + first;
    Some(match crate::lsp::sections(state).get(row) {
        Some(crate::lsp::Said::Value(_)) => vec![Event::OpenHoverRow(row)],
        _ => Vec::new(),
    })
}

fn pressed_in_find(state: &State, panes: &Layout, input: Input) -> Option<Vec<Event>> {
    let find = state.find.as_ref()?;
    let spot = layout::replace_box(panes.editor);
    if matches!(find.keys, FindKeys::Replace(_)) && spot.holds(input.column, input.row) {
        let field = |field| vec![Event::FindKeys(FindKeys::Replace(field))];
        let toggle = layout::replace_toggles(spot)
            .into_iter()
            .find(|(_, _, at)| at.holds(input.column, input.row));
        if let Some((icon, _, _)) = toggle {
            return Some(icon.pressed());
        }
        let button = layout::replace_buttons(spot)
            .into_iter()
            .find(|(_, at)| at.holds(input.column, input.row));
        return Some(match (button, input.row - spot.y) {
            (Some((ReplaceField::ReplaceAll, _)), _) => vec![Event::ReplaceAll],
            (Some((ReplaceField::Replace | ReplaceField::Find | ReplaceField::With, _)), _) => {
                vec![Event::ReplaceMatch]
            }
            (None, 1) => field(ReplaceField::Find),
            (None, 2) => field(ReplaceField::With),
            (None, _) => Vec::new(),
        });
    }
    if input.row + 1 != panes.editor.bottom() {
        return None;
    }
    let mut at = panes.editor.x + 1;
    for (text, icon) in crate::find_line(state) {
        let width = UnicodeWidthStr::width(text.as_str()) as u16;
        if (at..at + width).contains(&input.column) {
            return Some(icon?.pressed());
        }
        at += width;
    }
    None
}

fn on_hover(state: &State, panes: &Layout, input: Input) -> bool {
    hover_spot(state, panes).is_some_and(|spot| spot.holds(input.column, input.row))
}

fn hover_spot(state: &State, panes: &Layout) -> Option<crate::layout::Area> {
    Some(crate::lsp::placement(state)?.spot(state, panes))
}

fn place_in(state: &State, panes: &Layout, pane: Pane, (column, row): (u16, u16)) -> Place {
    let text = text_area(state, panes, pane);
    let (scroll, sideways) = match pane {
        Pane::Editor => (state.editor_scroll, state.editor_hscroll),
        Pane::Tree
        | Pane::Ai
        | Pane::Terminal
        | Pane::Output
        | Pane::Evaluator
        | Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts
        | Pane::Variables
        | Pane::Cheatsheet => (0, 0),
    };
    Place {
        line: crate::story::line_at_row(state, row.saturating_sub(text.y) as usize + 1 + scroll),
        column: column.saturating_sub(text.x) as usize + 1 + sideways,
    }
}

fn text_area(state: &State, panes: &Layout, pane: Pane) -> Area {
    let (tree_rows, editor_rows, editor_columns) = crate::fits_in(state, panes);
    let interior = |area: Area| Area {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    match pane {
        Pane::Editor => Area {
            x: panes.editor.x + 1 + crate::gutter(state),
            y: panes.editor.y + 1,
            width: editor_columns as u16,
            height: editor_rows as u16,
        },
        Pane::Tree => Area {
            height: tree_rows as u16,
            ..interior(panes.tree)
        },
        Pane::Ai | Pane::Cheatsheet => interior(panes.ai),
        Pane::Terminal => interior(crate::layout::split(
            panes.terminal,
            state.terminals.len(),
            state.split(),
        )),
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts => interior(panes.corner),
        Pane::Variables => interior(panes.terminal),
        Pane::Output => interior(panes.output),
        Pane::Evaluator => crate::layout::evaluator_split(panes.evaluator, snippet_rows(state)).0,
    }
}

pub fn palette_entry_at(state: &State, panes: &Layout, column: u16, row: u16) -> Option<char> {
    let height = panes.tree.height + panes.terminal.height;
    let rows = match state.modal {
        Modal::Palette => crate::palette_rows(height),
        Modal::Chord => crate::keys::chord_rows(state),
        _ => return None,
    };
    let widest = rows
        .iter()
        .map(|(_, line)| line.chars().count() as u16)
        .max()
        .unwrap_or(0);
    let width = panes.ai.right();
    let box_area = layout::overlay(width, height, rows.len() as u16, widest);
    if !box_area.holds(column, row) {
        return None;
    }
    let index = row.checked_sub(box_area.y + 1)? as usize;
    rows.get(index)?.0
}

pub fn position_label(line: usize, column: usize) -> String {
    format!(" {line}:{column} ")
}

pub fn buffer_dot_at(state: &State, panes: &Layout, column: u16, row: u16) -> Option<usize> {
    if row != panes.editor.bottom().saturating_sub(1) || state.buffers.len() < 2 {
        return None;
    }
    let shown = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
        .map(|buffer| position_label(buffer.line, buffer.column))?;
    let strip = state.buffers.len() as u16 * 2 - 1;
    let start = panes
        .editor
        .right()
        .saturating_sub(1 + shown.len() as u16 + strip);
    let index = (column.checked_sub(start)? / 2) as usize;
    (index < state.buffers.len()).then_some(index)
}

fn list_row(area: Area, row: u16, scroll: usize) -> usize {
    row.saturating_sub(area.y + 1) as usize + scroll
}

fn icon_at(actions: &[&'static str], area: Area, column: u16) -> Option<&'static str> {
    let start = (area.x + area.width.saturating_sub(1)).checked_sub(2 * actions.len() as u16)?;
    actions
        .get(column.checked_sub(start)? as usize / 2)
        .copied()
}

fn row_index(state: &State, panes: &Layout, row: u16) -> usize {
    list_row(panes.tree, row, state.tree_scroll)
}

fn transport_at(state: &State, panes: &Layout, column: u16) -> Option<&'static str> {
    let chips = crate::reading::transport(state);
    let labels = layout::chip_labels(&chips, panes.editor.width, layout::EDITOR_TITLE);
    let at = layout::strip_at(panes.editor, &labels, column)?;
    Some(chips[at].action)
}

fn breakpoint_column(state: &State, panes: &Layout, input: Input) -> bool {
    state.view == crate::View::Edit
        && state.diff.is_none()
        && state.walking.is_none()
        && !crate::previewing(state)
        && state.current_buffer.is_some()
        && input.column == panes.editor.x + 1 + layout::BREAKPOINT_COLUMN
        && input.row + 1 < panes.editor.bottom()
}

fn fold_toggle_at(state: &State, panes: &Layout, input: Input) -> bool {
    if state.diff.is_some() || crate::previewing(state) {
        return false;
    }
    let at = place_in(state, panes, Pane::Editor, (input.column, input.row));
    let Some(toggle) = crate::fold::toggles(state, at.line..=at.line)
        .get(&at.line)
        .copied()
    else {
        return false;
    };
    let pad = (panes.editor.x + 1 + layout::TOGGLE_COLUMN..=panes.editor.x + layout::GUTTER)
        .contains(&input.column);
    pad || (toggle == crate::fold::Toggle::Folded
        && crate::current_buffer(state)
            .is_some_and(|buffer| buffer.fold_dots_at(at.line, at.column)))
}

fn action_under(state: &State, panes: &Layout, input: Input) -> Option<&'static str> {
    let corner_row =
        input.row > panes.corner.y && input.row < panes.corner.bottom().saturating_sub(1);
    match layout::pane_at(panes, input.column, input.row)? {
        Pane::Tree => action_at(state, panes, input.column, input.row),
        Pane::Editor if input.row == panes.editor.y => transport_at(state, panes, input.column),
        Pane::Risk if input.row == panes.corner.y => pane_action_at(state, panes, input.column),
        Pane::Risk if corner_row => {
            let index = list_row(panes.corner, input.row, state.risk_scroll);
            risk_action_at(state, panes, input.column, index)
        }
        Pane::History if corner_row => {
            let index = list_row(panes.corner, input.row, state.history_scroll);
            history_action_at(state, panes, input.column, index)
        }
        Pane::Breakpoints if input.row == panes.corner.y => {
            breakpoint_chip_at(state, panes, input.column)
        }
        Pane::Breakpoints if corner_row => {
            let index = list_row(panes.corner, input.row, state.breakpoints_scroll);
            breakpoint_action_at(state, panes, input.column, index)
        }
        Pane::Variables
            if input.row > panes.terminal.y
                && input.row < panes.terminal.bottom().saturating_sub(1) =>
        {
            let index = list_row(panes.terminal, input.row, state.variables_scroll);
            variables_chip_at(state, panes, input.column, index)
        }
        _ => None,
    }
}

pub fn action_at(state: &State, panes: &Layout, column: u16, row: u16) -> Option<&'static str> {
    let clicked = tree::visible_rows(state)
        .into_iter()
        .nth(row_index(state, panes, row))?;
    icon_at(&tree::row_actions(state, &clicked.path), panes.tree, column)
}

fn history_action_at(
    state: &State,
    panes: &Layout,
    column: u16,
    index: usize,
) -> Option<&'static str> {
    if index != state.history_selection {
        return None;
    }
    icon_at(&crate::history::row_actions(state), panes.corner, column)
}

fn risk_action_at(
    state: &State,
    panes: &Layout,
    column: u16,
    index: usize,
) -> Option<&'static str> {
    if index != state.risk_selection {
        return None;
    }
    icon_at(&crate::risk::row_actions(state), panes.corner, column)
}

fn breakpoint_action_at(
    state: &State,
    panes: &Layout,
    column: u16,
    index: usize,
) -> Option<&'static str> {
    if index != state.breakpoints_selection {
        return None;
    }
    icon_at(&crate::debug::row_actions(state), panes.corner, column)
}

fn variables_chip_at(
    state: &State,
    panes: &Layout,
    column: u16,
    index: usize,
) -> Option<&'static str> {
    let chips: Vec<&'static str> = crate::debug::row_chips(state, index)
        .into_iter()
        .map(|chip| chip.action)
        .collect();
    icon_at(&chips, panes.terminal, column)
}

fn breakpoint_chip_at(state: &State, panes: &Layout, column: u16) -> Option<&'static str> {
    let chips = crate::debug::transport(state);
    let labels = layout::chip_labels(&chips, panes.corner.width, layout::CORNER_TITLE);
    let at = layout::strip_at(panes.corner, &labels, column)?;
    Some(chips[at].action)
}

fn pane_action_at(state: &State, panes: &Layout, column: u16) -> Option<&'static str> {
    icon_at(&crate::risk::pane_actions(state), panes.corner, column)
}

pub fn gutter_range(
    state: &State,
    panes: &Layout,
    from_row: u16,
    to_row: u16,
) -> Option<(String, u32, u32)> {
    let diff = state.diff.as_ref()?;
    let top = panes.editor.y + 1;
    let first = from_row.checked_sub(top)? as usize;
    let last = to_row.checked_sub(top)? as usize;
    let (lo, hi) = (first.min(last), first.max(last));
    let numbers: Vec<u32> = diff
        .get(lo..=hi.min(diff.len().saturating_sub(1)))?
        .iter()
        .filter_map(|line| line.new_line.map(|n| n as u32))
        .collect();
    Some((
        state.diff_file.clone()?,
        *numbers.first()?,
        *numbers.last()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        on_mouse, palette_entry_at, place_in, push, report, Divider, Encoding, Gesture, Input,
        KeyModifiers, Kind, Outcome, Pointer,
    };
    use crate::layout::panes;
    use crate::layout::Area;
    use crate::layout::{AiPane, Shapes};
    use crate::tree::Entry;
    use crate::Direction;
    use crate::{Event, Modal, Pane, Place, Pointed, State, View};
    use std::path::PathBuf;

    fn workspace() -> State {
        let mut state = State {
            root: PathBuf::from("/w"),
            tree_divider: 30,
            ..State::default()
        };
        state.contents.insert(
            PathBuf::from("/w"),
            vec![
                Entry {
                    name: "src".to_string(),
                    is_dir: true,
                },
                Entry {
                    name: "a.rs".to_string(),
                    is_dir: false,
                },
            ],
        );
        state
    }

    fn click(state: &State, column: u16, row: u16) -> Vec<Event> {
        on_mouse(
            state,
            &panes(120, 26, 30, None, 0, 0, Shapes::default()),
            &mut Pointer::default(),
            Input {
                kind: Kind::LeftDown,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            },
        )
        .events
    }

    #[test]
    fn a_press_in_the_replace_box_lands_on_what_is_drawn_there() {
        let mut state = workspace();
        state.find = Some(crate::Find {
            query: crate::editor::Buffer::text_box("state"),
            origin: Place { line: 1, column: 1 },
            case: crate::search::Case::Smart,
            extent: crate::search::Extent::Anywhere,
            keys: crate::FindKeys::Replace(crate::ReplaceField::With),
        });
        let editor = panes(120, 26, 30, None, 0, 0, Shapes::default()).editor;
        let spot = crate::layout::replace_box(editor);
        let field = |field| vec![Event::FindKeys(crate::FindKeys::Replace(field))];
        assert_eq!(
            click(&state, spot.x + 3, spot.y + 1),
            field(crate::ReplaceField::Find)
        );
        assert_eq!(
            click(&state, spot.x + 3, spot.y + 2),
            field(crate::ReplaceField::With)
        );
        assert_eq!(
            click(&state, spot.right() - 4, spot.y + 1),
            vec![Event::ToggleCase]
        );
        assert_eq!(
            click(&state, spot.right() - 9, spot.y + 1),
            vec![Event::ToggleWord]
        );
        assert_eq!(
            click(&state, spot.x + 2, spot.y + 3),
            vec![Event::ReplaceMatch]
        );
        assert_eq!(
            click(&state, spot.x + 12, spot.y + 3),
            vec![Event::ReplaceAll]
        );
        assert_eq!(click(&state, spot.x + 11, spot.y + 3), vec![]);
        assert_eq!(click(&state, spot.x + 3, spot.y), vec![], "the border");
    }

    const CELL: Place = Place { line: 5, column: 9 };

    #[test]
    fn a_child_asking_for_sgr_gets_sgr() {
        assert_eq!(
            report(Encoding::Sgr, Gesture::Click, CELL),
            Some(b"\x1b[<0;9;5M\x1b[<0;9;5m".to_vec())
        );
        assert_eq!(
            report(Encoding::Sgr, Gesture::Wheel(Direction::Up), CELL),
            Some(b"\x1b[<64;9;5M".to_vec())
        );
        assert_eq!(
            report(Encoding::Sgr, Gesture::Wheel(Direction::Down), CELL),
            Some(b"\x1b[<65;9;5M".to_vec())
        );
    }

    #[test]
    fn a_sideways_wheel_is_its_own_button() {
        assert_eq!(
            report(Encoding::Sgr, Gesture::Wheel(Direction::Left), CELL),
            Some(b"\x1b[<66;9;5M".to_vec())
        );
        assert_eq!(
            report(Encoding::Sgr, Gesture::Wheel(Direction::Right), CELL),
            Some(b"\x1b[<67;9;5M".to_vec())
        );
        assert_eq!(
            report(Encoding::Legacy, Gesture::Wheel(Direction::Left), CELL),
            Some(vec![0x1b, b'[', b'M', 32 + 66, 32 + 9, 32 + 5])
        );
        assert_eq!(
            report(Encoding::Legacy, Gesture::Wheel(Direction::Right), CELL),
            Some(vec![0x1b, b'[', b'M', 32 + 67, 32 + 9, 32 + 5])
        );
    }

    #[test]
    fn a_child_asking_for_legacy_gets_legacy() {
        assert_eq!(
            report(Encoding::Legacy, Gesture::Click, CELL),
            Some(vec![
                0x1b,
                b'[',
                b'M',
                32,
                32 + 9,
                32 + 5,
                0x1b,
                b'[',
                b'M',
                32 + 3,
                32 + 9,
                32 + 5
            ])
        );
        assert_eq!(
            report(Encoding::Legacy, Gesture::Wheel(Direction::Up), CELL),
            Some(vec![0x1b, b'[', b'M', 32 + 64, 32 + 9, 32 + 5])
        );
        assert_eq!(
            report(Encoding::Legacy, Gesture::Wheel(Direction::Down), CELL),
            Some(vec![0x1b, b'[', b'M', 32 + 65, 32 + 9, 32 + 5])
        );
    }

    #[test]
    fn a_child_that_asked_for_no_mouse_is_told_nothing() {
        assert_eq!(report(Encoding::None, Gesture::Click, CELL), None);
        assert_eq!(
            report(Encoding::None, Gesture::Wheel(Direction::Up), CELL),
            None
        );
    }

    #[test]
    fn a_cell_the_legacy_encoding_cannot_express_is_declined() {
        let last = Place {
            line: 223,
            column: 223,
        };
        assert!(report(Encoding::Legacy, Gesture::Click, last).is_some());
        for past in [
            Place {
                line: 1,
                column: 224,
            },
            Place {
                line: 224,
                column: 1,
            },
        ] {
            assert_eq!(report(Encoding::Legacy, Gesture::Click, past), None);
            assert!(report(Encoding::Sgr, Gesture::Click, past).is_some());
        }
    }

    #[test]
    fn a_click_finds_the_pane_under_it() {
        let state = workspace();
        assert_eq!(click(&state, 40, 5), vec![Event::ClickPane(Pane::Editor)]);
        assert_eq!(click(&state, 100, 5), vec![Event::ClickPane(Pane::Ai)]);
        assert_eq!(click(&state, 40, 20), vec![Event::FocusSplit(0)]);
    }

    #[test]
    fn a_click_in_a_split_moves_the_keyboard_to_it() {
        let state = State {
            terminals: vec![crate::Shell::Idle; 2],
            ..workspace()
        };
        let strip = panes(120, 26, 30, None, 0, 0, Shapes::default()).terminal;
        let second = crate::layout::split(strip, 2, 1);
        assert_eq!(click(&state, strip.x + 1, 20), vec![Event::FocusSplit(0)]);
        assert_eq!(click(&state, second.x + 1, 20), vec![Event::FocusSplit(1)]);
    }

    #[test]
    fn tree_clicks_resolve_against_the_rows_on_screen() {
        let state = workspace();
        assert_eq!(
            click(&state, 5, 1),
            vec![Event::ClickRow(PathBuf::from("/w/src"))]
        );
        assert_eq!(
            click(&state, 5, 2),
            vec![Event::ClickRow(PathBuf::from("/w/a.rs"))]
        );

        let mut review = workspace();
        review.view = View::Review;
        review.repo = Some(vec![crate::review::GitFile {
            path: "only.rs".to_string(),
            status: crate::review::GitStatus::Modified,
        }]);
        assert_eq!(
            click(&review, 5, 1),
            vec![Event::ClickRow(PathBuf::from("/w/only.rs"))]
        );
    }

    #[test]
    fn the_results_box_swallows_a_press_on_the_pane_behind_it() {
        let mut searching = workspace();
        searching.search = Some(crate::Search::default());
        assert_eq!(click(&searching, 5, 1), vec![]);
        let wheeled = on_mouse(
            &searching,
            &panes(120, 26, 30, None, 0, 0, Shapes::default()),
            &mut Pointer::default(),
            Input {
                kind: Kind::ScrollDown,
                column: 5,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        )
        .events;
        assert!(
            matches!(wheeled.first(), Some(Event::Scroll { .. })),
            "the wheel never reached the box: {wheeled:?}"
        );
    }

    #[test]
    fn the_wheel_moves_the_row_of_an_open_list_box() {
        let wheel = |state: &State, kind: Kind, column: u16, row: u16| {
            on_mouse(
                state,
                &panes(120, 26, 30, None, 0, 0, Shapes::default()),
                &mut Pointer::default(),
                Input {
                    kind,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
            .events
        };
        let branches = Modal::Branches {
            refs: Vec::new(),
            filter: String::new(),
            row: 0,
        };
        for (modal, down, up) in [
            (
                Modal::Tools { row: 0 },
                Event::MoveToolRow(Direction::Down),
                Event::MoveToolRow(Direction::Up),
            ),
            (
                Modal::Launches { row: 0 },
                Event::MoveLaunchRow(Direction::Down),
                Event::MoveLaunchRow(Direction::Up),
            ),
            (
                branches,
                Event::MoveBranchRow(Direction::Down),
                Event::MoveBranchRow(Direction::Up),
            ),
        ] {
            let open = State {
                modal: modal.clone(),
                ..workspace()
            };
            for (column, row) in [(60, 13), (5, 1), (80, 5)] {
                assert_eq!(
                    wheel(&open, Kind::ScrollDown, column, row),
                    vec![down.clone()],
                    "{modal:?} at {column},{row}"
                );
                assert_eq!(
                    wheel(&open, Kind::ScrollUp, column, row),
                    vec![up.clone()],
                    "{modal:?} at {column},{row}"
                );
                assert_eq!(wheel(&open, Kind::ScrollLeft, column, row), vec![]);
                assert_eq!(wheel(&open, Kind::ScrollRight, column, row), vec![]);
            }
        }
        assert!(matches!(
            wheel(&workspace(), Kind::ScrollDown, 5, 1).first(),
            Some(Event::Scroll { .. })
        ));
    }

    #[test]
    fn a_press_in_the_results_box_marks_the_hit_under_the_pointer() {
        let hit = |file: &str| crate::search::Hit {
            file: file.to_string(),
            line: 1,
            column: 1,
            text: "update".to_string(),
        };
        let mut searching = workspace();
        searching.screen_width = 120;
        searching.screen_height = 26;
        searching.search = Some(crate::Search {
            results: crate::search::Results {
                hits: vec![hit("a.rs"), hit("a.rs"), hit("b.rs")],
                ..Default::default()
            },
            ..crate::Search::default()
        });
        let top = 7;
        assert_eq!(click(&searching, 10, top - 1), vec![], "the query row");
        assert_eq!(click(&searching, 10, top), vec![], "a file heading");
        assert_eq!(click(&searching, 10, top + 1), vec![Event::SelectHit(0)]);
        assert_eq!(click(&searching, 10, top + 2), vec![Event::SelectHit(1)]);
        assert_eq!(click(&searching, 10, top + 3), vec![], "the next heading");
        assert_eq!(click(&searching, 10, top + 4), vec![Event::SelectHit(2)]);
        assert_eq!(click(&searching, 10, top + 5), vec![], "past the last row");

        searching.search.as_mut().unwrap().scroll = 3;
        assert_eq!(click(&searching, 10, top), vec![], "the heading, scrolled");
        assert_eq!(click(&searching, 10, top + 1), vec![Event::SelectHit(2)]);

        assert_eq!(click(&searching, 10, top + 15), vec![], "the box's chrome");
    }

    #[test]
    fn tree_clicks_count_from_the_scrolled_row() {
        let mut scrolled = workspace();
        scrolled.tree_scroll = 1;
        assert_eq!(
            click(&scrolled, 5, 1),
            vec![Event::ClickRow(PathBuf::from("/w/a.rs"))]
        );
    }

    #[test]
    fn action_clicks_count_from_the_scrolled_row() {
        let mut scrolled = workspace();
        scrolled.tree_scroll = 1;
        scrolled.tree_selection = Some(PathBuf::from("/w/a.rs"));
        assert_eq!(click(&scrolled, 25, 1), vec![Event::RowAction("delete")]);
        assert_eq!(click(&scrolled, 27, 1), vec![Event::RowAction("copy-path")]);

        let mut folder = workspace();
        folder.tree_scroll = 1;
        folder.tree_selection = Some(PathBuf::from("/w/src"));
        assert_eq!(
            click(&folder, 21, 1),
            vec![Event::ClickRow(PathBuf::from("/w/a.rs"))]
        );
    }

    #[test]
    fn risk_clicks_count_from_the_panes_own_first_row() {
        let mut state = risk_workspace();
        assert_eq!(risk_click(&state, 5, 19), vec![Event::ClickRiskRow(0)]);
        assert_eq!(risk_click(&state, 5, 20), vec![Event::ClickRiskRow(1)]);
        assert_eq!(
            risk_click(&state, 5, 21),
            vec![Event::ClickPane(Pane::Risk)]
        );
        state.risk_scroll = 1;
        assert_eq!(risk_click(&state, 5, 19), vec![Event::ClickRiskRow(1)]);
    }

    #[test]
    fn a_click_on_the_risk_panes_bottom_border_is_not_a_row() {
        let mut state = risk_workspace();
        state.risk.figure = crate::risk::Figure::Current(crate::risk::Figures {
            functions: (0..20)
                .map(|index| crate::risk::Function {
                    file: "src/keys.rs".to_string(),
                    name: format!("wide{index:02}"),
                    line: index + 1,
                    metrics: crate::risk::Metrics {
                        cyclomatic: 31,
                        ..crate::risk::Metrics::default()
                    },
                })
                .collect(),
            unparsed: 0,
        });
        assert_eq!(
            risk_click(&state, 5, 24),
            vec![Event::ClickRiskRow(5)],
            "the last row the pane draws"
        );
        assert_eq!(
            risk_click(&state, 5, 25),
            vec![Event::ClickPane(Pane::Risk)]
        );
    }

    #[test]
    fn a_click_on_the_panes_border_icons_reaches_the_panes_actions() {
        let state = risk_workspace();
        assert_eq!(
            risk_click(&state, 25, 18),
            vec![Event::PaneAction(crate::risk::RECOMPUTE)]
        );
        assert_eq!(
            risk_click(&state, 27, 18),
            vec![Event::PaneAction(crate::risk::START_LOOP)]
        );
        assert_eq!(
            risk_click(&state, 5, 18),
            vec![Event::ClickPane(Pane::Risk)]
        );
    }

    #[test]
    fn a_click_on_the_risk_rows_icon_asks_for_the_refactor() {
        let state = risk_workspace();
        assert_eq!(
            risk_click(&state, 27, 19),
            vec![Event::RowAction(crate::risk::REFACTOR)]
        );
        assert_eq!(risk_click(&state, 26, 19), vec![Event::ClickRiskRow(0)]);
        assert_eq!(risk_click(&state, 27, 20), vec![Event::ClickRiskRow(1)]);
    }

    #[test]
    fn a_click_in_the_buffers_pane_names_the_row_it_landed_on() {
        let buffers = |count: usize| {
            let mut state = workspace();
            state.corner = crate::layout::Corner::Buffers;
            for index in 0..count {
                state.buffers.insert(
                    state.root.join(format!("{index:02}.rs")),
                    crate::editor::Buffer::open("contents", false, 4),
                );
            }
            state
        };
        let click = |state: &State, column, row| {
            on_mouse(
                state,
                &panes(
                    120,
                    26,
                    30,
                    None,
                    0,
                    0,
                    Shapes {
                        corner: crate::layout::Corner::Buffers,
                        ..Shapes::default()
                    },
                ),
                &mut Pointer::default(),
                Input {
                    kind: Kind::LeftDown,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
            .events
        };
        let two = buffers(2);
        assert_eq!(click(&two, 5, 19), vec![Event::ClickBufferRow(0)]);
        assert_eq!(click(&two, 5, 20), vec![Event::ClickBufferRow(1)]);
        assert_eq!(click(&two, 5, 18), vec![Event::ClickPane(Pane::Buffers)]);
        assert_eq!(click(&two, 5, 21), vec![Event::ClickPane(Pane::Buffers)]);
        let many = buffers(20);
        let shown = panes(
            120,
            26,
            30,
            None,
            0,
            0,
            Shapes {
                corner: crate::layout::Corner::Buffers,
                ..Shapes::default()
            },
        )
        .corner
        .height as usize
            - 2;
        assert_eq!(shown, 6);
        assert!(many.buffers.len() > shown);
        assert_eq!(click(&many, 5, 24), vec![Event::ClickBufferRow(5)]);
        assert_eq!(click(&many, 5, 25), vec![Event::ClickPane(Pane::Buffers)]);
    }

    fn risk_workspace() -> State {
        let mut state = workspace();
        state.corner = crate::layout::Corner::Risk;
        state.risk_threshold = 20;
        state.risk.figure = crate::risk::Figure::Current(crate::risk::Figures {
            functions: vec![
                crate::risk::Function {
                    file: "src/keys.rs".to_string(),
                    name: "route".to_string(),
                    line: 88,
                    metrics: crate::risk::Metrics {
                        cyclomatic: 31,
                        ..crate::risk::Metrics::default()
                    },
                },
                crate::risk::Function {
                    file: "src/ui.rs".to_string(),
                    name: "draw".to_string(),
                    line: 17,
                    metrics: crate::risk::Metrics {
                        cyclomatic: 22,
                        ..crate::risk::Metrics::default()
                    },
                },
            ],
            unparsed: 0,
        });
        state
    }

    #[test]
    fn a_click_in_the_cursor_history_names_the_row_or_its_icon() {
        let mut state = workspace();
        state.corner = crate::layout::Corner::History;
        state.visits = (0..20)
            .map(|which| crate::history::Visit {
                file: format!("src/{which:02}.rs"),
                line: which + 1,
                column: 1,
                text: "let a = 1;".to_string(),
            })
            .collect();
        state.history_selection = 0;
        let click = |state: &State, column, row| {
            on_mouse(
                state,
                &panes(
                    120,
                    26,
                    30,
                    None,
                    0,
                    0,
                    Shapes {
                        corner: crate::layout::Corner::History,
                        ..Shapes::default()
                    },
                ),
                &mut Pointer::default(),
                Input {
                    kind: Kind::LeftDown,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
            .events
        };
        assert_eq!(click(&state, 5, 19), vec![Event::ClickHistoryRow(0)]);
        assert_eq!(click(&state, 5, 20), vec![Event::ClickHistoryRow(1)]);
        assert_eq!(
            click(&state, 27, 19),
            vec![Event::RowAction(crate::history::GO_TO)]
        );
        assert_eq!(click(&state, 26, 19), vec![Event::ClickHistoryRow(0)]);
        assert_eq!(click(&state, 27, 20), vec![Event::ClickHistoryRow(1)]);
        assert_eq!(click(&state, 5, 18), vec![Event::ClickPane(Pane::History)]);
        assert_eq!(click(&state, 5, 25), vec![Event::ClickPane(Pane::History)]);
    }

    #[test]
    fn a_click_in_the_breakpoint_list_names_the_row_its_icon_or_the_chip() {
        let mut state = workspace();
        state.corner = crate::layout::Corner::Breakpoints;
        state.breakpoints = (1..=2)
            .map(|line| crate::debug::Breakpoint {
                file: PathBuf::from("/w/a.rs"),
                line,
                text: String::new(),
                stale: false,
                properties: Default::default(),
            })
            .collect();
        let click = |state: &State, column, row| {
            on_mouse(
                state,
                &panes(
                    120,
                    26,
                    30,
                    None,
                    0,
                    0,
                    Shapes {
                        corner: crate::layout::Corner::Breakpoints,
                        ..Shapes::default()
                    },
                ),
                &mut Pointer::default(),
                Input {
                    kind: Kind::LeftDown,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
            .events
        };
        assert_eq!(click(&state, 5, 19), vec![Event::ClickBreakpointRow(0)]);
        assert_eq!(
            click(&state, 25, 19),
            vec![Event::RowAction(crate::debug::EDIT)]
        );
        assert_eq!(
            click(&state, 27, 19),
            vec![Event::RowAction(crate::debug::REMOVE)]
        );
        assert_eq!(click(&state, 27, 20), vec![Event::ClickBreakpointRow(1)]);
        assert_eq!(
            click(&state, 5, 21),
            vec![Event::ClickPane(Pane::Breakpoints)]
        );
        for column in 23..=27 {
            assert_eq!(
                click(&state, column, 18),
                vec![Event::PaneAction(crate::debug::CLEAR_ALL)]
            );
        }
        for column in [22, 28] {
            assert_eq!(
                click(&state, column, 18),
                vec![Event::ClickPane(Pane::Breakpoints)]
            );
        }
    }

    #[test]
    fn a_click_on_the_transport_reaches_the_chip_drawn() {
        use crate::reading::{NEXT, PLAY_PAUSE, PREVIOUS, SPEED, STOP};
        let mut state = workspace();
        state.current_buffer = Some(PathBuf::from("/w/guide.md"));
        state.speech.speed = 1.0;
        let press = |state: &State, width: u16, from_corner: u16, row: u16| {
            let panes = panes(width, 26, 30, None, 0, 0, Shapes::default());
            let corner = panes.editor.x + panes.editor.width - 1;
            on_mouse(
                state,
                &panes,
                &mut Pointer::default(),
                Input {
                    kind: Kind::LeftDown,
                    column: corner - from_corner,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
            .events
        };
        let chip = |action| vec![Event::PaneAction(action)];
        let border = vec![Event::ClickPane(Pane::Editor)];
        let shed = [
            (22..=24, PLAY_PAUSE),
            (18..=20, PREVIOUS),
            (14..=16, NEXT),
            (10..=12, STOP),
            (2..=8, SPEED),
        ];
        let whole = [
            (47..=56, PLAY_PAUSE),
            (37..=45, PREVIOUS),
            (27..=35, NEXT),
            (17..=25, STOP),
            (2..=15, SPEED),
        ];
        for (width, strip) in [(120, shed), (220, whole)] {
            for (columns, action) in strip {
                let gap = columns.end() + 1;
                assert_eq!(press(&state, width, gap, 0), border, "{width}: {gap}");
                for column in columns {
                    assert_eq!(
                        press(&state, width, column, 0),
                        chip(action),
                        "{width}: {column}"
                    );
                }
            }
            assert_eq!(press(&state, width, 1, 0), border, "{width}");
        }
        assert_eq!(press(&state, 120, 43, 0), border);
        assert!(matches!(
            press(&state, 120, 20, 3).as_slice(),
            [Event::ClickText(_)]
        ));
        state.current_buffer = Some(PathBuf::from("/w/a.rs"));
        assert_eq!(press(&state, 120, 5, 0), border);
    }

    #[test]
    fn a_move_answers_with_the_place_only_over_the_editors_text() {
        let state = editing();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let editor = panes.editor;
        let moved = |column, row| {
            on_mouse(
                &state,
                &panes,
                &mut Pointer::default(),
                Input {
                    kind: Kind::Moved,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
            .events
        };
        assert_eq!(
            moved(editor.x + 1 + crate::gutter(&state) + 3, editor.y + 2),
            vec![Event::PointerMoved(Pointed::Text(Place {
                line: 2,
                column: 4
            }))]
        );
        assert_eq!(
            moved(editor.x + 4, editor.y),
            vec![Event::PointerMoved(Pointed::Elsewhere)]
        );
        assert_eq!(
            moved(1, editor.y + 2),
            vec![Event::PointerMoved(Pointed::Elsewhere)]
        );
        for showing in [diffed(), previewing()] {
            assert_eq!(
                on_mouse(
                    &showing,
                    &panes,
                    &mut Pointer::default(),
                    Input {
                        kind: Kind::Moved,
                        column: editor.x + 8,
                        row: editor.y + 2,
                        modifiers: KeyModifiers::NONE,
                    },
                )
                .events,
                vec![Event::PointerMoved(Pointed::Elsewhere)]
            );
        }
    }

    #[test]
    fn a_held_modifier_says_where_the_link_is_once_and_says_when_it_is_gone() {
        let mut state = workspace();
        let path = PathBuf::from("/w/a.rs");
        state.current_buffer = Some(path.clone());
        state
            .buffers
            .insert(path, crate::editor::Buffer::open("fn main() {}", false, 4));
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let cell = (panes.editor.x + 5, panes.editor.y + 1);
        let hover = |state: &State, modifiers| {
            on_mouse(
                state,
                &panes,
                &mut Pointer::default(),
                Input {
                    kind: Kind::Moved,
                    column: cell.0,
                    row: cell.1,
                    modifiers,
                },
            )
            .events
        };
        let at = place_in(&state, &panes, Pane::Editor, cell);
        assert_eq!(
            hover(&state, KeyModifiers::SUPER),
            vec![
                Event::PointerMoved(Pointed::Text(at)),
                Event::HoverLink(Some((Pane::Editor, at)))
            ]
        );
        state.link = Some((Pane::Editor, at));
        assert_eq!(
            hover(&state, KeyModifiers::CTRL),
            vec![Event::PointerMoved(Pointed::Text(at))]
        );
        assert_eq!(
            hover(&state, KeyModifiers::NONE),
            vec![
                Event::PointerMoved(Pointed::Text(at)),
                Event::HoverLink(None)
            ]
        );
    }

    #[test]
    fn a_held_modifier_points_at_text_in_the_split_it_hovers_and_no_other() {
        let mut state = workspace();
        state.terminals = vec![crate::Shell::Idle, crate::Shell::Idle];
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let hover = |column| {
            on_mouse(
                &state,
                &panes,
                &mut Pointer::default(),
                Input {
                    kind: Kind::Moved,
                    column,
                    row: panes.terminal.y + 2,
                    modifiers: KeyModifiers::CTRL,
                },
            )
            .events
            .into_iter()
            .filter(|event| matches!(event, Event::HoverLink(_)))
            .collect::<Vec<_>>()
        };
        let active = crate::layout::split(panes.terminal, 2, 0);
        assert_eq!(
            hover(active.x + 4),
            vec![Event::HoverLink(Some((
                Pane::Terminal,
                Place { line: 2, column: 4 }
            )))]
        );
        assert_eq!(
            hover(active.x),
            vec![],
            "the split's border is not its text"
        );
        let other = crate::layout::split(panes.terminal, 2, 1);
        assert_eq!(
            hover(other.x + 4),
            vec![],
            "another split's text is not in this grid"
        );
    }

    fn risk_click(state: &State, column: u16, row: u16) -> Vec<Event> {
        on_mouse(
            state,
            &panes(
                120,
                26,
                30,
                None,
                0,
                0,
                Shapes {
                    corner: crate::layout::Corner::Risk,
                    ..Shapes::default()
                },
            ),
            &mut Pointer::default(),
            Input {
                kind: Kind::LeftDown,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            },
        )
        .events
    }

    #[test]
    fn a_click_past_the_last_row_focuses_the_pane() {
        assert_eq!(
            click(&workspace(), 5, 10),
            vec![Event::ClickPane(Pane::Tree)]
        );
    }

    #[test]
    fn the_divider_is_grabbed_before_the_row_beneath_it() {
        let state = workspace();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer::default();
        let outcome = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDown,
                column: 29,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(outcome.events.is_empty(), "no row click");
        assert_eq!(pointer.dragging, Some(Divider::Tree));

        let outcome = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDrag,
                column: 45,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(outcome.events, vec![Event::DragDivider(46)]);
    }

    #[test]
    fn the_divider_is_released_and_stops_resizing() {
        let state = workspace();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer {
            dragging: Some(Divider::Tree),
            ..Pointer::default()
        };
        on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftUp,
                column: 45,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(pointer.dragging, None);
    }

    #[test]
    fn the_divider_cannot_squeeze_a_pane_away() {
        let state = workspace();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer {
            dragging: Some(Divider::Tree),
            ..Pointer::default()
        };
        let narrow = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDrag,
                column: 0,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(narrow.events, vec![Event::DragDivider(12)]);
        let mut pointer = Pointer {
            dragging: Some(Divider::Tree),
            ..Pointer::default()
        };
        let wide = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDrag,
                column: 119,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(wide.events, vec![Event::DragDivider(90)]);
    }

    #[test]
    fn the_ai_pane_edge_is_grabbed_and_names_a_width() {
        let state = workspace();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer::default();
        let outcome = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDown,
                column: 84,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(outcome.events.is_empty(), "no click through to the pane");
        assert_eq!(pointer.dragging, Some(Divider::Ai));

        let outcome = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDrag,
                column: 80,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(outcome.events, vec![Event::DragAiDivider(40)]);
    }

    #[test]
    fn the_border_above_the_strip_is_grabbed_and_names_a_height() {
        let state = workspace();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer::default();
        on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDown,
                column: 50,
                row: 17,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(pointer.dragging, None);
        let mut pointer = Pointer::default();
        let press = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDown,
                column: 50,
                row: 18,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(press.events.is_empty(), "a press clicked through");
        assert_eq!(pointer.dragging, Some(Divider::Strip));
        let drag = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDrag,
                column: 50,
                row: 10,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(drag.events, vec![Event::DragStrip(16)]);
    }

    #[test]
    fn a_tall_ai_pane_is_not_the_border_above_the_strip() {
        let tall = Shapes {
            ai: AiPane::Tall,
            ..Shapes::default()
        };
        let panes = panes(120, 26, 30, None, 0, 0, tall);
        let mut pointer = Pointer::default();
        on_mouse(
            &workspace(),
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDown,
                column: panes.ai.x + 2,
                row: panes.terminal.y,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(pointer.dragging, None);
    }

    #[test]
    fn the_corners_border_is_not_the_border_above_the_strip() {
        let shapes = Shapes {
            corner: crate::layout::Corner::Risk,
            ..Shapes::default()
        };
        let panes = panes(120, 26, 30, None, 0, 0, shapes);
        let mut pointer = Pointer::default();
        on_mouse(
            &workspace(),
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDown,
                column: 5,
                row: panes.corner.y,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(pointer.dragging, None);
    }

    #[test]
    fn a_divider_column_in_the_terminal_pane_is_a_click_in_the_terminal() {
        let state = workspace();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        for column in [29, 84] {
            let mut pointer = Pointer::default();
            let outcome = on_mouse(
                &state,
                &panes,
                &mut pointer,
                Input {
                    kind: Kind::LeftDown,
                    column,
                    row: 20,
                    modifiers: KeyModifiers::NONE,
                },
            );
            assert_eq!(pointer.dragging, None, "column {column} grabbed a divider");
            assert!(
                !outcome.events.is_empty(),
                "column {column} reached nothing"
            );
        }
    }

    #[test]
    fn the_palette_is_hit_tested_where_it_is_drawn() {
        let state = State {
            modal: Modal::Palette,
            ..workspace()
        };
        for row in 0..26 {
            for column in 0..120 {
                let beside = palette_entry_at(
                    &state,
                    &panes(120, 26, 30, None, 0, 0, Shapes::default()),
                    column,
                    row,
                );
                let tall = palette_entry_at(
                    &state,
                    &panes(
                        120,
                        26,
                        30,
                        None,
                        0,
                        0,
                        Shapes {
                            ai: AiPane::Tall,
                            ..Shapes::default()
                        },
                    ),
                    column,
                    row,
                );
                assert_eq!(beside, tall, "entry at {column},{row} moved with the shape");
            }
        }
        let panes = panes(
            120,
            26,
            30,
            None,
            0,
            0,
            Shapes {
                ai: AiPane::Tall,
                ..Shapes::default()
            },
        );
        assert!((0..26).any(|row| palette_entry_at(&state, &panes, 60, row) == Some('r')));
    }

    #[test]
    fn every_chord_hint_entry_is_a_click_on_its_key() {
        let state = State {
            modal: Modal::Chord,
            ..workspace()
        };
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        for (key, _) in crate::keys::chord_rows(&state) {
            let Some(key) = key else { continue };
            let clicked = (0..26).any(|row| {
                (0..120)
                    .any(|column| click(&state, column, row) == vec![Event::ClickPaletteEntry(key)])
            });
            assert!(clicked, "the hint's ({key}) cannot be clicked");
        }
        let closed = workspace();
        assert!((0..26).all(|row| palette_entry_at(&closed, &panes, 60, row).is_none()));
    }

    #[test]
    fn a_click_in_the_breakpoint_column_toggles_that_lines_breakpoint() {
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let column = panes.editor.x + 1 + crate::layout::BREAKPOINT_COLUMN;
        assert_eq!(
            click(&editing(), column, 3),
            vec![Event::ToggleBreakpoint(3)]
        );
        assert_eq!(
            click(&editing(), column + 1, 3),
            vec![Event::ClickText(Place { line: 3, column: 1 })]
        );
        assert!(!click(&diffed(), column, 3).contains(&Event::ToggleBreakpoint(3)));
        assert!(!click(&previewing(), column, 3).contains(&Event::ToggleBreakpoint(3)));
        let border = panes.editor.bottom() - 1;
        assert!(!matches!(
            click(&editing(), column, border).as_slice(),
            [Event::ToggleBreakpoint(_)]
        ));
    }

    #[test]
    fn a_click_on_a_run_mark_offers_it_unless_a_breakpoint_is_drawn_over_it() {
        let mut state = editing();
        state.runs = crate::startup::start(&crate::startup::Startup::default())
            .expect("the defaults start")
            .0
            .runs;
        state.buffers.insert(
            PathBuf::from("/w/a.rs"),
            crate::editor::Buffer::open("\n\nfn main() {}\n", false, 4),
        );
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let column = panes.editor.x + 1 + crate::layout::BREAKPOINT_COLUMN;
        assert_eq!(click(&state, column, 3), vec![Event::OfferRun(3)]);
        let offered = crate::update(&state, Event::OfferRun(3)).0;
        for action in [crate::run::RUN, crate::run::DEBUG] {
            let clicked = (0..26).any(|row| {
                (0..120)
                    .any(|column| click(&offered, column, row) == vec![Event::ChooseRun(action)])
            });
            assert!(clicked, "the offer's {action} cannot be clicked");
        }
        state.breakpoints.push(crate::debug::Breakpoint {
            file: PathBuf::from("/w/a.rs"),
            line: 3,
            text: "fn main() {}".to_string(),
            stale: false,
            properties: crate::debug::Properties::default(),
        });
        assert_eq!(click(&state, column, 3), vec![Event::ToggleBreakpoint(3)]);
    }

    #[test]
    fn every_palette_entry_is_clickable_on_a_short_screen() {
        let state = State {
            modal: Modal::Palette,
            ..workspace()
        };
        for height in [24, 26] {
            let panes = panes(120, height, 30, None, 0, 0, Shapes::default());
            let rows = crate::palette_rows(height);
            let box_area = crate::layout::overlay(
                panes.ai.right(),
                panes.tree.height + panes.terminal.height,
                rows.len() as u16,
                rows.iter()
                    .map(|(_, line)| line.chars().count() as u16)
                    .max()
                    .unwrap_or(0),
            );
            let found: Vec<char> = (0..height)
                .flat_map(|row| (0..120).map(move |column| (column, row)))
                .filter_map(|(column, row)| palette_entry_at(&state, &panes, column, row))
                .collect();
            for (_, entries) in crate::PALETTE {
                for (key, entry) in entries {
                    assert!(
                        found.contains(key),
                        "{entry} is unclickable at {height} rows"
                    );
                }
            }
            let border = box_area.bottom().saturating_sub(1);
            for column in 0..120 {
                assert_eq!(
                    palette_entry_at(&state, &panes, column, border),
                    None,
                    "the box's bottom border answers an entry at {height} rows"
                );
            }
        }
    }

    #[test]
    fn a_tall_ai_panes_edge_is_grabbed_beside_the_terminal() {
        let state = workspace();
        let panes = panes(
            120,
            26,
            30,
            None,
            0,
            0,
            Shapes {
                ai: AiPane::Tall,
                ..Shapes::default()
            },
        );
        let mut pointer = Pointer::default();
        let outcome = on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDown,
                column: 84,
                row: 20,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert!(outcome.events.is_empty(), "no click through to the pane");
        assert_eq!(pointer.dragging, Some(Divider::Ai));
        let mut pointer = Pointer::default();
        on_mouse(
            &state,
            &panes,
            &mut pointer,
            Input {
                kind: Kind::LeftDown,
                column: 29,
                row: 20,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(pointer.dragging, None);
    }

    #[test]
    fn a_tall_ai_pane_does_not_shrink_what_a_divider_can_reach() {
        let state = workspace();
        for divider in [Divider::Tree, Divider::Ai] {
            let reached: Vec<Vec<Event>> = [AiPane::Beside, AiPane::Tall]
                .into_iter()
                .map(|shape| {
                    let panes = panes(
                        120,
                        26,
                        30,
                        None,
                        0,
                        0,
                        Shapes {
                            ai: shape,
                            ..Shapes::default()
                        },
                    );
                    let mut pointer = Pointer {
                        dragging: Some(divider),
                        ..Pointer::default()
                    };
                    on_mouse(
                        &state,
                        &panes,
                        &mut pointer,
                        Input {
                            kind: Kind::LeftDrag,
                            column: 110,
                            row: 1,
                            modifiers: KeyModifiers::NONE,
                        },
                    )
                    .events
                })
                .collect();
            assert_eq!(reached[0], reached[1], "{divider:?} reaches less when tall");
        }
    }

    #[test]
    fn the_ai_pane_edge_cannot_squeeze_the_editor_away() {
        let state = workspace();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        for (column, expected) in [(119u16, 12u32), (0, 70)] {
            let mut pointer = Pointer {
                dragging: Some(Divider::Ai),
                ..Pointer::default()
            };
            let outcome = on_mouse(
                &state,
                &panes,
                &mut pointer,
                Input {
                    kind: Kind::LeftDrag,
                    column,
                    row: 1,
                    modifiers: KeyModifiers::NONE,
                },
            );
            assert_eq!(outcome.events, vec![Event::DragAiDivider(expected)]);
        }
    }

    #[test]
    fn scrolling_follows_the_pointer_and_never_moves_focus() {
        let state = workspace();
        let terminal = panes(120, 26, 30, None, 0, 0, Shapes::default()).terminal;
        let outcome = on_mouse(
            &state,
            &panes(120, 26, 30, None, 0, 0, Shapes::default()),
            &mut Pointer::default(),
            Input {
                kind: Kind::ScrollUp,
                column: terminal.x + 4,
                row: terminal.y + 2,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(
            outcome.events,
            vec![Event::Scroll {
                pane: Pane::Terminal,
                direction: Direction::Up,
                at: Place { line: 2, column: 4 },
            }]
        );
    }

    #[test]
    fn moving_the_pointer_reports_where_it_rests_and_nothing_else() {
        let state = editing();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer::default();
        let moved = |pointer: &mut Pointer, column, row| {
            on_mouse(
                &state,
                &panes,
                pointer,
                Input {
                    kind: Kind::Moved,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
        };
        let over_text = moved(&mut pointer, 45, 5);
        assert_eq!(
            over_text.events,
            vec![Event::PointerMoved(Pointed::Text(Place {
                line: 5,
                column: 5
            }))]
        );
        assert_eq!(
            moved(&mut pointer, 5, 5).events,
            vec![Event::PointerMoved(Pointed::Elsewhere)]
        );
        assert_eq!(pointer, Pointer::default(), "a move took hold of something");
    }

    #[test]
    fn right_clicking_does_nothing_beyond_reporting_the_pane() {
        let state = workspace();
        let outcome = on_mouse(
            &state,
            &panes(120, 26, 30, None, 0, 0, Shapes::default()),
            &mut Pointer::default(),
            Input {
                kind: Kind::RightDown,
                column: 40,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(outcome.events, vec![Event::RightClick(Pane::Editor)]);
    }

    #[test]
    fn dragging_in_the_tree_selects_a_row_rather_than_text() {
        let state = workspace();
        let outcome = on_mouse(
            &state,
            &panes(120, 26, 30, None, 0, 0, Shapes::default()),
            &mut Pointer::default(),
            Input {
                kind: Kind::LeftDrag,
                column: 5,
                row: 1,
                modifiers: KeyModifiers::NONE,
            },
        );
        assert_eq!(
            outcome.events,
            vec![Event::DragRow(PathBuf::from("/w/src"))]
        );
        assert!(outcome.select.is_none(), "a filename is not text you copy");
    }

    fn drag(state: &State, from: (u16, u16), to: (u16, u16)) -> Outcome {
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer::default();
        let mut outcome = Outcome::default();
        for (column, row) in [from, to] {
            outcome = on_mouse(
                state,
                &panes,
                &mut pointer,
                Input {
                    kind: Kind::LeftDrag,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            );
        }
        outcome
    }

    fn editing() -> State {
        let mut state = workspace();
        state.current_buffer = Some(PathBuf::from("/w/a.rs"));
        state
    }

    fn previewing() -> State {
        let mut state = workspace();
        let path = PathBuf::from("/w/a.rs");
        state.current_buffer = Some(path.clone());
        state
            .buffers
            .insert(path, crate::editor::Buffer::open("", true, 4));
        state
    }

    fn diffed() -> State {
        let mut state = workspace();
        state.diff = Some(vec![crate::DiffLine {
            new_line: Some(1),
            old_line: None,
            removed: false,
            text: "x".repeat(40),
        }]);
        state.diff_file = Some("a.rs".to_string());
        state.current_buffer = Some(PathBuf::from("/w/a.rs"));
        state
    }

    #[test]
    fn a_diff_click_accounts_for_the_marker_column() {
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let row = panes.editor.y + 1;
        let column = panes.editor.x + 1 + crate::layout::GUTTER + 2;
        assert_eq!(
            place_in(&diffed(), &panes, Pane::Editor, (column, row)),
            Place { line: 1, column: 1 }
        );
        assert_eq!(
            place_in(&editing(), &panes, Pane::Editor, (column, row)),
            Place { line: 1, column: 3 },
            "Source has no marker, so the same column is two characters further in"
        );
        let mut slid = diffed();
        slid.editor_hscroll = 12;
        assert_eq!(
            place_in(&slid, &panes, Pane::Editor, (column, row)),
            Place {
                line: 1,
                column: 13
            }
        );
    }

    #[test]
    fn a_preview_click_accounts_for_the_absent_gutter() {
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let column = panes.editor.x + 3;
        let row = panes.editor.y + 1;
        assert_eq!(
            place_in(&previewing(), &panes, Pane::Editor, (column, row)),
            Place { line: 1, column: 3 }
        );
        assert_eq!(
            place_in(&editing(), &panes, Pane::Editor, (column, row)),
            Place { line: 1, column: 1 },
            "still inside Source's gutter, so it clamps to the first column"
        );
    }

    #[test]
    fn dragging_a_preview_resolves_in_core_without_a_selection_request() {
        let outcome = drag(&previewing(), (36, 1), (40, 3));
        assert_eq!(
            outcome.events,
            vec![Event::DragText {
                from: Place { line: 1, column: 6 },
                to: Place {
                    line: 3,
                    column: 10
                },
            }]
        );
        assert!(outcome.select.is_none(), "the rows are already in-core");
    }

    #[test]
    fn dragging_in_the_editor_covers_a_span_of_the_buffer() {
        let outcome = drag(&editing(), (41, 1), (45, 3));
        assert_eq!(
            outcome.events,
            vec![Event::DragText {
                from: Place { line: 1, column: 1 },
                to: Place { line: 3, column: 5 },
            }]
        );
        assert!(outcome.select.is_none(), "no pty to read");
    }

    #[test]
    fn dragging_upward_covers_what_dragging_downward_covers() {
        let state = editing();
        assert_eq!(
            drag(&state, (44, 3), (40, 2)).events,
            drag(&state, (40, 2), (44, 3)).events
        );
    }

    #[test]
    fn dragging_in_the_editor_counts_from_the_scrolled_line() {
        let mut scrolled = editing();
        scrolled.editor_scroll = 4;
        assert_eq!(
            drag(&scrolled, (42, 2), (42, 2)).events,
            vec![Event::DragText {
                from: Place { line: 6, column: 2 },
                to: Place { line: 6, column: 2 },
            }]
        );
    }

    #[test]
    fn dragging_in_the_editor_counts_from_the_scrolled_column() {
        let mut scrolled = editing();
        scrolled.editor_hscroll = 12;
        assert_eq!(
            drag(&scrolled, (42, 2), (42, 2)).events,
            vec![Event::DragText {
                from: Place {
                    line: 2,
                    column: 14
                },
                to: Place {
                    line: 2,
                    column: 14
                },
            }]
        );
    }

    #[test]
    fn clicking_in_the_editor_reports_the_place_clicked() {
        assert_eq!(
            click(&editing(), 45, 3),
            vec![Event::ClickText(Place { line: 3, column: 5 })]
        );
    }

    #[test]
    fn clicking_the_editor_with_nothing_to_edit_only_focuses_it() {
        assert_eq!(
            click(&workspace(), 40, 3),
            vec![Event::ClickPane(Pane::Editor)]
        );
    }

    #[test]
    fn dragging_in_the_terminal_asks_for_a_span_in_grid_coordinates() {
        let selection = drag(&workspace(), (11, 19), (10, 20))
            .select
            .expect("a selection request");
        assert_eq!(selection.pane, Pane::Terminal);
        assert_eq!(
            selection.from,
            Place {
                line: 1,
                column: 11
            }
        );
        assert_eq!(
            selection.to,
            Place {
                line: 2,
                column: 10
            }
        );
    }

    #[test]
    fn dragging_in_the_ai_pane_asks_for_a_span_in_its_own_grid() {
        let ai = panes(120, 26, 30, None, 0, 0, Shapes::default()).ai;
        let selection = drag(&workspace(), (ai.x + 11, ai.y + 1), (ai.x + 10, ai.y + 2))
            .select
            .expect("a selection request");
        assert_eq!(selection.pane, Pane::Ai);
        assert_eq!(
            selection.from,
            Place {
                line: 1,
                column: 11
            }
        );
        assert_eq!(
            selection.to,
            Place {
                line: 2,
                column: 10
            }
        );
    }

    #[test]
    fn dragging_in_the_ai_pane_reaches_no_further_than_the_visible_screen() {
        let ai = panes(120, 26, 30, None, 0, 0, Shapes::default()).ai;
        let selection = drag(
            &workspace(),
            (ai.x + 1, ai.y + 1),
            (ai.x + 1, ai.bottom() - 2),
        )
        .select
        .expect("a selection request");
        assert_eq!(selection.to.line, (ai.height - 2) as usize);
    }

    fn gesture(state: &State, path: &[(Kind, u16, u16)]) -> Vec<Event> {
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer::default();
        let mut events = Vec::new();
        for &(kind, column, row) in path {
            let input = Input {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            };
            events.extend(on_mouse(state, &panes, &mut pointer, input).events);
        }
        events
    }

    #[test]
    fn a_press_and_release_clicks_through_to_the_pane() {
        let ai = panes(120, 26, 30, None, 0, 0, Shapes::default()).ai;
        let path = [
            (Kind::LeftDown, ai.x + 4, ai.y + 2),
            (Kind::LeftUp, ai.x + 4, ai.y + 2),
        ];
        assert_eq!(
            gesture(&workspace(), &path),
            vec![
                Event::ClickPane(Pane::Ai),
                Event::ClickThrough {
                    pane: Pane::Ai,
                    at: Place { line: 2, column: 4 },
                }
            ]
        );
    }

    #[test]
    fn dragging_never_clicks_through_to_the_pane() {
        let ai = panes(120, 26, 30, None, 0, 0, Shapes::default()).ai;
        let path = [
            (Kind::LeftDown, ai.x + 4, ai.y + 2),
            (Kind::LeftDrag, ai.x + 9, ai.y + 3),
            (Kind::LeftUp, ai.x + 9, ai.y + 3),
        ];
        assert!(
            !gesture(&workspace(), &path)
                .iter()
                .any(|event| matches!(event, Event::ClickThrough { .. })),
            "a drag is ours, not the child's"
        );
    }

    fn twice(state: &State, first: (u16, u16, u64), second: (u16, u16, u64)) -> Vec<Event> {
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer::default();
        let mut events = Vec::new();
        for (column, row, at_ms) in [first, second] {
            pointer.at_ms = at_ms;
            events.extend(
                on_mouse(
                    state,
                    &panes,
                    &mut pointer,
                    Input {
                        kind: Kind::LeftDown,
                        column,
                        row,
                        modifiers: KeyModifiers::NONE,
                    },
                )
                .events,
            );
        }
        events
    }

    #[test]
    fn two_presses_on_one_cell_inside_the_window_pick_the_word() {
        let state = editing();
        let doubled = Event::DoubleClickText(Place { line: 3, column: 5 });
        assert!(twice(&state, (45, 3, 0), (45, 3, 299)).contains(&doubled));
        assert!(!twice(&state, (45, 3, 0), (45, 3, 301)).contains(&doubled));
        assert!(!twice(&state, (46, 3, 0), (45, 3, 100)).contains(&doubled));
    }

    #[test]
    fn two_presses_where_there_is_no_text_pick_no_word() {
        let state = workspace();
        assert!(!twice(&state, (40, 3, 0), (40, 3, 100))
            .iter()
            .any(|event| matches!(event, Event::DoubleClickText(_))));
    }

    fn folding() -> State {
        let mut state = workspace();
        let path = PathBuf::from("/w/a.rs");
        let mut buffer = crate::editor::Buffer::open("fn main() {\n    go();\n}\n", false, 4);
        crate::fold::toggle(&mut buffer, false);
        state.buffers.insert(path.clone(), buffer);
        state.current_buffer = Some(path);
        state
    }

    #[test]
    fn pressing_a_fold_toggle_or_its_dots_opens_the_block() {
        let state = folding();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let toggle = panes.editor.x + 1 + crate::layout::TOGGLE_COLUMN;
        assert_eq!(
            click(&state, toggle, panes.editor.y + 1),
            vec![
                Event::ClickText(Place { line: 1, column: 1 }),
                Event::ToggleFold { all: false }
            ]
        );
        let dots = panes.editor.x + 1 + crate::layout::GUTTER + 11;
        assert_eq!(
            click(&state, dots, panes.editor.y + 1),
            vec![
                Event::ClickText(Place {
                    line: 1,
                    column: 12
                }),
                Event::ToggleFold { all: false }
            ]
        );
        assert_eq!(
            click(&state, panes.editor.x + 2, panes.editor.y + 1),
            vec![Event::ClickText(Place { line: 1, column: 1 })],
            "a line number is not a toggle"
        );
        assert_eq!(
            click(&state, toggle, panes.editor.y + 2),
            vec![Event::ClickText(Place { line: 3, column: 1 })],
            "and neither is the pad of a line that opens no block"
        );
    }

    #[test]
    fn resting_on_an_action_icon_names_it_and_leaving_takes_it_back() {
        let mut state = workspace();
        state.tree_selection = Some(PathBuf::from("/w/a.rs"));
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let moved = |state: &State, column, row| {
            on_mouse(
                state,
                &panes,
                &mut Pointer::default(),
                Input {
                    kind: Kind::Moved,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
            .events
        };
        let icon = panes.tree.x + panes.tree.width - 3;
        let row = panes.tree.y + 2;
        assert!(moved(&state, icon, row).contains(&Event::HoverAction(Some("copy-path"))));
        state.hovered_action = Some("copy-path");
        assert!(
            !moved(&state, icon, row)
                .iter()
                .any(|event| matches!(event, Event::HoverAction(_))),
            "the same answer is not news"
        );
        assert!(moved(&state, panes.tree.x + 2, row).contains(&Event::HoverAction(None)));
    }

    #[test]
    fn a_click_outside_every_pane_does_nothing() {
        assert!(click(&workspace(), 200, 5).is_empty());
    }

    #[test]
    fn a_drag_pushes_at_the_edges_of_the_text_and_nowhere_inside() {
        let text = Area {
            x: 10,
            y: 4,
            width: 6,
            height: 5,
        };
        let at = |column, row| {
            push(
                text,
                Input {
                    kind: Kind::LeftDrag,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            )
        };
        assert_eq!(at(12, 6), (None, None), "inside");
        assert_eq!(at(12, 4), (Some(Direction::Up), None), "the first row");
        assert_eq!(at(12, 8), (Some(Direction::Down), None), "the last row");
        assert_eq!(at(12, 40), (Some(Direction::Down), None), "far below");
        assert_eq!(at(10, 6), (None, Some(Direction::Left)), "the first column");
        assert_eq!(at(15, 6), (None, Some(Direction::Right)), "the last column");
        assert_eq!(
            at(15, 8),
            (Some(Direction::Down), Some(Direction::Right)),
            "a corner moves both"
        );
    }

    #[test]
    fn a_press_decides_whether_a_drag_moves_the_window_resizes_it_or_picks_text() {
        let window = Area {
            x: 20,
            y: 5,
            width: 40,
            height: 12,
        };
        let panes = panes(
            120,
            40,
            30,
            None,
            0,
            0,
            Shapes {
                evaluator: Some(window),
                ..Shapes::default()
            },
        );
        let mut state = crate::debug::paused(workspace());
        crate::debug::open_evaluator(&mut state, "count".to_string());
        state.evaluator_at = Some(window);
        let drag = |from: (u16, u16), to: (u16, u16)| {
            let mut pointer = Pointer::default();
            let input = |kind, (column, row)| Input {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            };
            on_mouse(&state, &panes, &mut pointer, input(Kind::LeftDown, from));
            on_mouse(&state, &panes, &mut pointer, input(Kind::LeftDrag, to))
                .events
                .into_iter()
                .find(|event| {
                    matches!(
                        event,
                        Event::PlaceEvaluator(_) | Event::SizeSnippet(_) | Event::DragText { .. }
                    )
                })
        };
        let title = (window.x + 2, window.y);
        assert_eq!(
            drag(title, (title.0 + 10, title.1 + 2)),
            Some(Event::PlaceEvaluator(Area {
                x: 30,
                y: 7,
                ..window
            })),
            "the title bar moves it"
        );
        assert_eq!(
            drag(
                (window.right() - 1, window.bottom() - 1),
                (window.right() + 9, window.bottom() + 3)
            ),
            Some(Event::PlaceEvaluator(Area {
                width: 50,
                height: 16,
                ..window
            })),
            "the bottom-right corner resizes both ways"
        );
        assert_eq!(
            drag((window.x, window.y + 3), (window.x + 4, window.y + 3)),
            Some(Event::PlaceEvaluator(Area {
                x: 24,
                width: 36,
                ..window
            })),
            "the left border keeps the right one where it is"
        );
        assert_eq!(
            drag(
                (window.x + 5, window.bottom() - 1),
                (window.x + 5, window.bottom() + 2)
            ),
            Some(Event::PlaceEvaluator(Area {
                height: 15,
                ..window
            })),
            "the bottom border resizes downward alone"
        );
        assert_eq!(
            drag(
                (window.x, window.bottom() - 1),
                (window.x - 4, window.bottom() + 2)
            ),
            Some(Event::PlaceEvaluator(Area {
                x: 16,
                width: 44,
                height: 15,
                ..window
            })),
            "the bottom-left corner resizes both ways and keeps the right edge"
        );
        let (snippet, _) = crate::layout::evaluator_split(window, None);
        assert_eq!(
            drag(
                (window.x + 2, snippet.bottom()),
                (window.x + 2, snippet.bottom() - 2)
            ),
            Some(Event::SizeSnippet(snippet.height - 2)),
            "the rule between the halves divides them"
        );
        assert!(
            matches!(
                drag((snippet.x + 1, snippet.y), (snippet.x + 4, snippet.y + 1)),
                Some(Event::DragText { .. })
            ),
            "a drag inside the Snippet picks text"
        );
    }
}
