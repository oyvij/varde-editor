//! Turning a mouse event into events.
//!
//! Pure, like [`crate::keys`]. The one thing it cannot do is read characters off
//! the terminal's screen, so a drag that selects text comes back as a
//! [`Selection`] request for the edge to fulfil.

use crate::layout::{self, Area, Layout};
use crate::{tree, Direction, Event, Modal, Pane, Place, Pointed, State};
use terminput::KeyModifiers;
use unicode_width::UnicodeWidthStr;

/// Which mouse-report encoding the program in a hosted pane asked for, as the
/// terminal model in front of its pty reports it. A report in any other
/// encoding is text the child cannot parse, and it lands in its prompt as
/// literal characters — which is the phantom text Varde used to leave behind by
/// sending SGR to every child that asked for the mouse at all.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    #[default]
    None,
    /// `ESC [ M` and one byte per field, each offset by 32.
    Legacy,
    /// `ESC [ <` and decimal fields, with no coordinate limit.
    Sgr,
}

/// What the child is being told happened. A click is press *and* release
/// together: a child left holding a button reads the next move as a drag of its
/// own. The wheel has no release — xterm never sends one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Click,
    Wheel(Direction),
}

/// The largest coordinate the legacy encoding can express: its fields are one
/// byte offset by 32, so 223 is the last cell that has a representation at all.
const LEGACY_LIMIT: usize = 223;

/// The bytes a child expecting `encoding` reads as `gesture` on the cell `at` of
/// its own grid, or nothing when it cannot be told: the child asked for no
/// mouse, or the legacy encoding has no byte for that cell. Declining is
/// deliberate — a wrapped coordinate names a different cell, and a child acting
/// on the wrong cell is worse than one that heard nothing.
pub fn report(encoding: Encoding, gesture: Gesture, at: Place) -> Option<Vec<u8>> {
    // xterm's wheel buttons, exhaustive over the directions rather than
    // catch-all: a sideways swipe answered with 65 is a child told to scroll
    // down, which is the substitution this function exists to refuse.
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
                // The legacy encoding has no separate release: button 3 is it.
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
    /// A trackpad swipe or a tilt wheel. Named rather than folded into the two
    /// above: the surfaces that answer it have an offset of their own, and the
    /// conversion in `main.rs` used to drop every sideways report on a
    /// catch-all arm, which is the shape AGENTS.md forbids for keys and forbids
    /// here for the same reason.
    ScrollLeft,
    ScrollRight,
    /// The pointer moving with nothing held down, reported only because the
    /// edge asks the terminal for motion. It presses nothing: it is where the
    /// pointer comes to rest — the one gesture nobody makes — and it is what
    /// turns a name under a held jump modifier into a link and takes the link
    /// away again.
    Moved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Input {
    pub kind: Kind,
    pub column: u16,
    pub row: u16,
    /// What was held while it happened, as the terminal reported it. The whole
    /// set rather than a flag: the same lossless-type reason `keys` takes a
    /// `KeyEvent`, and the edge is not the place to decide which modifier
    /// means what.
    pub modifiers: KeyModifiers,
}

/// Whether the modifier that turns a click into a jump is held. Cmd is the
/// gesture — VS Code's on macOS — and Ctrl is the same gesture everywhere
/// else, and both are accepted because the mouse protocol has bits for shift,
/// alt and ctrl and none for Super: a Cmd+click arrives with no modifier at
/// all on most terminals, so binding Cmd alone would be a gesture nobody's
/// terminal can send. `gd` remains the way to reach it with no modifier.
fn jumping(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::CTRL)
}

/// What a drag covers, for the edge to turn into text: a span in the pane's own
/// text coordinates — a buffer's lines and columns, or cells in a pty's visible
/// grid. Ordered, so dragging upward covers what dragging downward covers, and
/// the edge only has to read characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub pane: Pane,
    pub from: Place,
    pub to: Place,
}

/// Which pane edge a drag has hold of. Two handles now, so which one is not a
/// pair of booleans that could both be true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Divider {
    /// The tree's right border, which moves the tree/editor boundary.
    Tree,
    /// The AI pane's left border, which moves the editor/AI boundary.
    Ai,
    /// The border above the Strip, which moves the Strip/top boundary.
    Strip,
    /// The Program output's left border, which moves the Variables/Program
    /// output boundary inside the Debug group.
    Output,
}

/// What a press on the Evaluator's floating window took hold of, and the
/// rectangle the window had at the time. Every report is measured from the
/// press rather than from the report before it: a drag that runs off the
/// screen and comes back leaves the window under the pointer, where measuring
/// each step against the last would have left it a cell short for every one
/// the clamp swallowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Grab {
    took: Took,
    window: Area,
}

/// Which part of the window: its title bar moves the whole of it, a border or
/// a corner resizes it by the edges it names, and the rule between the
/// Snippet and the output divides the two. An enum over the edges rather than
/// a flag per side, for the reason [`Divider`] is an enum — dragging the top
/// and the bottom at once has no answer.
///
/// No top edge: that row is the title bar, Chips and all, so a press there is
/// the move gesture anybody would make on a title bar. Which is also why the
/// corners are the bottom two.
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

/// Where a drag began, and which divider it grabbed. Edge state, but the
/// routing needs it, so it lives with the routing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pointer {
    pub dragging: Option<Divider>,
    pub drag_from: Option<(u16, u16)>,
    /// Whether the pointer moved while the button was down. A child's click is
    /// sent on release and only if it did not, so dragging to select text never
    /// presses whatever the drag started on.
    pub dragged: bool,
    /// When the report being routed arrived, off the edge's clock. Told rather
    /// than remembered, the way `keys::on_key_event` is told the same
    /// millisecond: only the edge can read a clock, and a test supplies
    /// whatever it needs so no timing test ever waits.
    pub at_ms: u64,
    /// Where and when the last press landed, so a second press on the same cell
    /// inside the double-tap window is one gesture rather than two.
    last_press: Option<(u16, u16, u64)>,
    /// Which pane the button went down in. A drag belongs to it for as long as
    /// the button is held, wherever the pointer wanders — the rule the minimap
    /// arm in [`dragged`] already applies to its own gesture. Without it the
    /// pane was resolved against where the pointer is *now*, so a selection
    /// dragged out of the editor was handed to whatever pane it crossed, or to
    /// no pane at all, and stopped growing.
    pane: Option<Pane>,
    /// Where in the pane's own text the drag started, resolved once. Not
    /// `drag_from`, which is a screen cell: the two agree only while the view
    /// stands still, and the whole point of a held drag is that it does not —
    /// an anchor re-read off row three of a pane that has scrolled four lines
    /// names a different character every tick, so the selection eats itself
    /// from the top.
    anchor: Option<Place>,
    /// What a press on the Evaluator's window took hold of, for as long as
    /// the button is held. `None` for a press anywhere else and for one in
    /// either half of the window, which is text somebody is picking.
    grab: Option<Grab>,
    /// The last drag report while the button is held at or past its pane's
    /// edge. The edge replays it on a cadence, which is the only thing that can
    /// move a drag held *still*: a terminal reports nothing while nothing
    /// moves. Bounded by the button the way the spinner is bounded by its job
    /// (`docs/adr/0009-a-spinner-is-bounded-by-its-job.md`) — no drag, no
    /// `held`, and idle CPU is unchanged.
    pub held: Option<Input>,
}

// No `Eq`: an `Event` carries a speed, which is a float.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outcome {
    pub events: Vec<Event>,
    pub select: Option<Selection>,
    /// A click with the jump modifier held on a pty's cell: the edge reads the
    /// row it is on and hands it back as `Event::ClickLink`, because whether
    /// there is a URL under the pointer is written on the child's grid.
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
    // Before any pane is chosen, because where the pointer *is* is not a press
    // in a pane: a move over anything but the editor's text is the pointer
    // leaving, which is what takes a box it rested for down.
    if input.kind == Kind::Moved {
        // Nothing is held any more: motion with no button down is what the
        // terminal reports, and it is the one end condition a drag has that
        // does not depend on seeing the release. A pointer dragged off the
        // window keeps scrolling — which is what every editor does — but a
        // terminal that never reported the release outside its own window
        // would otherwise leave it scrolling for good.
        pointer.held = None;
        let mut events = vec![Event::PointerMoved(resting(state, panes, input))];
        events.extend(hovered(state, panes, input));
        let icon = action_under(state, panes, input);
        if icon != state.hovered_action {
            events.push(Event::HoverAction(icon));
        }
        // Against the strip the renderer draws, so what lights is what the
        // pointer is on. A zero-width strip answers `false` for every column,
        // so a hidden mirror needs no arm of its own here.
        let on_minimap = crate::minimap::strip(state, panes.editor).holds(input.column, input.row);
        if on_minimap != state.hovered_minimap {
            events.push(Event::HoverMinimap(on_minimap));
        }
        return Outcome::of(events);
    }
    // A list box has the keyboard, so it has the wheel too, wherever the
    // pointer is: a notch is Up or Down, and a pane scrolling behind the box
    // is the code moving while the list the reader is choosing from stays put.
    // Sideways is a notch too, and moves nothing in a column.
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
    // The results box covers every pane, so a press in one is a press on
    // something nobody can see: a click in the tree behind it opened whatever
    // file was under the box, and a drag picked text out of a pane the box was
    // drawn over. What the box itself does with a press is all that is left,
    // and the wheel is the exception that goes on — `update` gives it to the
    // box, for the same reason the box has the keyboard. Nothing is left
    // holding a divider or a half-finished drag either.
    if let Some(search) = state.search.as_ref() {
        if !matches!(
            input.kind,
            Kind::ScrollUp | Kind::ScrollDown | Kind::ScrollLeft | Kind::ScrollRight
        ) {
            *pointer = Pointer::default();
            return Outcome::of(result_click(state, search, input));
        }
    }
    // The wheel over the Hover box scrolls the box, whatever pane it floats
    // over: the reader is reading it, not the code underneath.
    let wheel = match input.kind {
        Kind::ScrollUp => Some(Direction::Up),
        Kind::ScrollDown => Some(Direction::Down),
        _ => None,
    };
    if let Some(direction) = wheel.filter(|_| on_hover(state, panes, input)) {
        return Outcome::of(vec![Event::ScrollHover(direction)]);
    }
    // Ahead of the border handle below, which the Group tabs are drawn into
    // the columns of: a handle that swallowed them would be tabs nobody could
    // click. Ahead of the pane dispatch too, because the border row a tab sits
    // on is neither a shell's grid nor a Variables row.
    if input.kind == Kind::LeftDown && input.row == panes.terminal.y {
        if let Some(group) = group_tab_at(state, panes, input.column) {
            return Outcome::of(vec![Event::ShowGroup(group)]);
        }
        if let Some(action) = strip_chip_at(state, panes, input.column) {
            return Outcome::of(vec![Event::PaneAction(action)]);
        }
    }
    // The Evaluator floats over the panes, so a press on it belongs to the
    // window before a divider *between* those panes can claim the column: the
    // AI pane's edge runs straight through the middle of a centred window, and
    // it was swallowing every press on the Chip drawn there. Only where no
    // drag is already held — a drag belongs to the pane its button went down
    // in, whatever it crosses.
    if pointer.pane.is_none() && panes.evaluator.holds(input.column, input.row) {
        return in_pane(state, panes, pointer, Pane::Evaluator, input);
    }
    if let Some(outcome) = divider_drag(panes, pointer, input) {
        return outcome;
    }
    // A drag belongs to the pane its button went down in, for as long as the
    // button is held. Resolved once at the press rather than per report: past
    // the pane's border the pointer is over a neighbour, or over the gap
    // between them, and either answer ends the gesture the person is making.
    let pane = match (input.kind, pointer.pane) {
        (Kind::LeftDrag | Kind::LeftUp, Some(pane)) => pane,
        _ => match layout::pane_at(panes, input.column, input.row) {
            Some(pane) => pane,
            None => return Outcome::default(),
        },
    };
    in_pane(state, panes, pointer, pane, input)
}

/// The hit a press in the results box lands on. Nothing for a file heading, for
/// the query above the list, or for the borders around it: the nearest hit is a
/// different file opened by the `Enter` that follows. The row is read against
/// `search.scroll` — the very field the clamp and the renderer read — so the
/// hit a click marks is the one under the pointer wherever the wheel has left
/// the list.
fn result_click(state: &State, search: &crate::Search, input: Input) -> Vec<Event> {
    let area = layout::search_box(state.screen_width, state.screen_height);
    let top = area.y + 1 + layout::SEARCH_HEADER;
    let offset = match input.kind {
        Kind::LeftDown if input.row >= top && area.holds(input.column, input.row) => {
            usize::from(input.row - top)
        }
        _ => return vec![],
    };
    // The box draws only as many rows as the layout leaves it, so a press below
    // the last of them is on the bottom border — inside the rectangle, and part
    // of no row.
    if offset >= layout::search_hit_rows(state.screen_width, state.screen_height) {
        return vec![];
    }
    match crate::search::rows(&search.results).get(offset + search.scroll) {
        Some(crate::search::Row::Hit(index)) => vec![Event::SelectHit(*index)],
        _ => vec![],
    }
}

/// Which Group tab is under a column of the Strip's top border, if any — the
/// labels `ui` draws, hit-tested by the `layout::strip_at` every other strip
/// of labels on a border is hit-tested by. Nothing at all off the Strip's own
/// columns: the corner's top border is on the same row and carries its icons.
fn group_tab_at(state: &State, panes: &Layout, column: u16) -> Option<crate::layout::Group> {
    let strip = panes.strip();
    if !strip.holds(column, strip.y) {
        return None;
    }
    let labels = crate::group_labels(state);
    let index = crate::layout::strip_at(strip, &labels, column)?;
    Some(crate::group_tabs(state)[index].group)
}

/// Which Chip of the Variables' Transport sits under a column of the Strip's
/// top border, at the columns `ui` draws them into — both off
/// `crate::transport_area`, for the reason the Group tabs above read one
/// `strip_at`.
fn strip_chip_at(state: &State, panes: &Layout, column: u16) -> Option<&'static str> {
    if !crate::showing_transport(state) {
        return None;
    }
    let chips = crate::debug::strip_transport(state);
    let area = crate::transport_area(state, panes.strip());
    let labels = crate::layout::chip_labels(&chips, area.width, crate::layout::CORNER_TITLE);
    Some(chips[crate::layout::strip_at(area, &labels, column)?].action)
}

/// A pane's border is the handle. Checked before anything else, because those
/// columns belong to a pane and would otherwise read as clicking a row. Only
/// where the border is, though: the terminal spans every column underneath, so
/// a handle that ignored the row would eat two of its columns.
fn divider_drag(panes: &Layout, pointer: &mut Pointer, input: Input) -> Option<Outcome> {
    let tree_edge = panes.tree.x + panes.tree.width.saturating_sub(1);
    let ai_edge = panes.ai.x;
    // How wide the screen is. Read off the AI pane rather than the terminal,
    // which spans it only while the AI pane stops above it — a tall one takes
    // those columns, and clamping against what was left would shrink both
    // handles' range every time somebody asked for it.
    let screen = panes.ai.right();
    // The AI pane's border runs as far down as the pane does, which in the
    // tall shape is past the terminal's first row.
    let beside_tree = input.row < panes.terminal.y;
    let beside_ai = input.row < panes.ai.bottom();
    // The Strip's own top border across the shell's columns, and nothing
    // else: the row above it is the editor's bottom border, where the buffer
    // dots are, and the Corner's top border carries its icons. Its Group tabs
    // have already been answered above.
    // The whole Strip's top border, the Program output's columns included: a
    // handle that stopped at the Variables would leave the height undraggable
    // from half of it.
    let above_strip = input.row == panes.terminal.y && panes.strip().holds(input.column, input.row);
    // The Program output's own left border, which runs the Strip's height —
    // its top row excepted, where the Group tabs and the Transport are.
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
        // Unclamped, like the Strip's height: the width is state, and the
        // layout bounds it against the group it was dragged in.
        Kind::LeftDrag if pointer.dragging == Some(Divider::Output) => {
            Some(Outcome::of(vec![Event::DragOutput(u32::from(
                panes.output.right().saturating_sub(input.column),
            ))]))
        }
        // Unclamped: the height is state, and `update` bounds it against the
        // screen it was dragged on.
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
            // The editor keeps its 20 columns, which is the same floor the
            // layout enforces — clamping here as well is what stops a drag
            // past it from being remembered as a width nobody can see. The
            // step-menu's width is taken out of that floor too, the same
            // way the layout takes it out of the editor's.
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
            // Where the button went down is where a selection starts. Waiting
            // for the first drag report anchors it a character late.
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
            // The place the press already named, rather than a hit-test of its
            // own: a double-click picks a word wherever a single click takes
            // the caret, and two hit-tests are how a click and what it selects
            // come to disagree about the cell under the pointer.
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
            // A press and a release with nothing in between is a click, and a
            // click is the only thing a child gets: Varde owns drags in its
            // panes, so forwarding the press as it happened would activate
            // whatever a text selection started on.
            let tapped = !pointer.dragged;
            pointer.drag_from = None;
            pointer.pane = None;
            pointer.anchor = None;
            pointer.grab = None;
            pointer.held = None;
            pointer.dragged = false;
            let at = place_in(state, panes, pane, (input.column, input.row));
            // The jump modifier's click on a pty is a question about the text
            // under it, not a click for the child — the same gesture that
            // follows a name to its definition in the editor.
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
        // Answered in `on_mouse`, before a pane was chosen: where the pointer
        // is is not a press in a pane.
        Kind::Moved => Outcome::default(),
    }
}

/// The link the pointer is on, as the change to what the state already names:
/// nothing to report while it stays on the same place, because a pointer
/// crossing a pane sends a report per cell and each one that reached `update`
/// would be a frame.
fn hovered(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    // The editor's own text only, and the border row the Transport lives on is
    // not text — the same rows `pressed` reads as places, and the same ones
    // `resting` answers for.
    let at = match (jumping(input.modifiers), resting(state, panes, input)) {
        (true, Pointed::Text(at)) => Some(at),
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
    // The Evaluator floats over the panes, so a press that landed on it is
    // the window's before anything drawn under it is asked — the reason
    // `layout::pane_at` asks it first. Without this the strips hit-tested
    // below answer for a rectangle the window is covering.
    if pane == Pane::Evaluator {
        return pressed_in_evaluator(state, panes, pointer, input);
    }
    if let Some(events) = pressed_in_hover(state, panes, input) {
        return events;
    }
    if let Some(key) = palette_entry_at(state, panes, input.column, input.row) {
        return vec![Event::ClickPaletteEntry(key)];
    }
    // The screen, read off the panes for the reason `palette_entry_at` gives.
    let screen = (panes.ai.right(), panes.tree.height + panes.terminal.height);
    if let Some(action) = crate::run::chip_at(state, screen.0, screen.1, input.column, input.row) {
        return vec![Event::ChooseRun(action)];
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
        // The Transport lives on the editor's top border, so a click on that
        // row is tested against it before it is read as a place in the text —
        // the border row is not one.
        (Pane::Editor, _) if input.row == panes.editor.y => {
            match transport_at(state, panes, input.column) {
                Some(action) => vec![Event::PaneAction(action)],
                None => vec![Event::ClickPane(Pane::Editor)],
            }
        }
        // The mirror's columns are the editor pane's, so a press in them is
        // tested before it is read as a place in the text: travelling a file
        // is not picking a character out of it.
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
        // The gutter's leftmost column is the Breakpoint column, and the line
        // numbers beside it set nothing. A Run mark shares it, and the click
        // is whichever of the two the column draws: a Breakpoint over a Run
        // mark, so the one drawn is the one a click takes away.
        (Pane::Editor, _) if breakpoint_column(state, panes, input) => {
            let at = place_in(state, panes, pane, (input.column, input.row));
            let offered = !crate::debug::marks(state).contains_key(&at.line)
                && crate::run::marks(state).contains_key(&at.line);
            match offered {
                true => vec![Event::OfferRun(at.line)],
                false => vec![Event::ToggleBreakpoint(at.line)],
            }
        }
        // The toggle in the gutter and the dots at the end of a folded line are
        // one affordance drawn in two places, so a press on either is one
        // event. The caret lands first, because the block toggled is the block
        // the cursor is in — no second place has to be carried with it, the
        // same shape the jump modifier's click has.
        (Pane::Editor, _) if fold_toggle_at(state, panes, input) => {
            let at = place_in(state, panes, pane, (input.column, input.row));
            vec![Event::ClickText(at), Event::ToggleFold { all: false }]
        }
        // A diff is read-only and has no cursor to place.
        (Pane::Editor, _) if state.diff.is_none() && state.current_buffer.is_some() => {
            let at = place_in(state, panes, pane, (input.column, input.row));
            let mut events = vec![Event::ClickText(at)];
            // A click with the jump modifier held is `gd` with the pointer.
            // The caret lands first, so the question is about the name that
            // was clicked and no second place has to be carried with it.
            if jumping(input.modifiers) {
                events.push(Event::AskDefinition);
            }
            events
        }
        // visible_rows, not rows: in Review view the pane lists changed files,
        // and in Edit view it may be filtered.
        (Pane::Tree, None) => match tree::visible_rows(state).get(row_index) {
            Some(row) => vec![Event::ClickRow(row.path.clone())],
            None => vec![Event::ClickPane(pane)],
        },
        // A row of the Risk list, through its own scroll offset — the pane has
        // rows of its own, so hit-testing it against the tree's offset would
        // name whichever row the tree happened to be scrolled to.
        (Pane::Risk, _) => pressed_in_risk(state, panes, input),
        (Pane::Buffers, _) => pressed_in_buffers(state, panes, input),
        (Pane::History, _) => pressed_in_history(state, panes, input),
        (Pane::Breakpoints, _) => pressed_in_breakpoints(state, panes, input),
        (Pane::Diagnostics, _) => pressed_in_diagnostics(state, panes, input),
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
        // A Variables row, through the Strip's rectangle and the Variables'
        // own scroll offset — the Frames' hit-test, one pane over. Its Chips
        // first, for the reason the Breakpoint list's row icons come before
        // its rows: an icon is on the row, so a row that answered first would
        // open the tree instead of acting on it.
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
        // Which of the strip's shells was pressed, so a click in a split is
        // the keyboard moving to it — the one gesture that tells them apart.
        (Pane::Terminal, _) => vec![Event::FocusSplit(crate::layout::split_at(
            panes.terminal,
            state.terminals.len(),
            input.column,
        ))],
        _ => vec![Event::ClickPane(pane)],
    }
}

/// A row of the Risk list, through its own scroll offset — the pane has rows of
/// its own, so hit-testing it against the tree's offset would name whichever row
/// the tree happened to be scrolled to.
fn pressed_in_risk(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    // The pane's own actions live on its top border, where the figure is — so a
    // click on that row is tested against them before it is read as a row,
    // which the border row is not.
    if input.row == panes.corner.y {
        return match pane_action_at(state, panes, input.column) {
            Some(action) => vec![Event::PaneAction(action)],
            None => vec![Event::ClickPane(Pane::Risk)],
        };
    }
    // The bottom border is chrome too, and a scrolled or overlong list has a
    // Function at its index: reading it as a row opens one nowhere near the
    // pointer, and its icon columns would arm that row's action.
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

/// A row of the Buffers pane, through its own scroll offset. No action icons to
/// test past and no pane actions on its top border — the pane has no actions —
/// so both borders are chrome and every cell between them is either a buffer or
/// nothing. Both are tested: an offset index is bounded by the list, but the
/// bottom border sits a row past the last one a scrolled list draws, so reading
/// it as a row names a buffer nowhere near the pointer.
fn pressed_in_buffers(state: &State, panes: &Layout, input: Input) -> Vec<Event> {
    let index = list_row(panes.corner, input.row, state.buffers_scroll);
    let inside = input.row > panes.corner.y && input.row < panes.corner.bottom().saturating_sub(1);
    match inside && state.buffers.len() > index {
        true => vec![Event::ClickBufferRow(index)],
        false => vec![Event::ClickPane(Pane::Buffers)],
    }
}

/// A row of the Cursor history pane, through its own scroll offset. Both
/// borders are chrome — the pane has no actions of its own on the top one — and
/// the row's own icon is tested before the row, exactly as the Risk list's is:
/// the icon columns are the row's last two, and reading them as the row would
/// go there twice over.
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

/// A row of the Breakpoint list, its row's icon, or a Chip on its top border —
/// the Risk list's three, tested in the Risk list's order and for its reasons.
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

/// The Evaluator's window: its Chips on the top border, the chrome a drag
/// takes hold of, a row of its output below the split, and the window itself
/// otherwise. The Chips first, for the reason the Breakpoint list's are first
/// — the border row is chrome, and reading it as content acts on a row nobody
/// pointed at. A Chip is not a title bar either: a press on one that was
/// remembered as a grab would move the window every time somebody ran a
/// Snippet.
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
    // Which gesture this press begins, decided here and held for as long as
    // the button is: a drag belongs to what it went down on, the rule the
    // pane a drag belongs to is one of.
    pointer.grab = grab_at(window, output, input).map(|took| Grab { took, window });
    if pointer.grab.is_none() && output.holds(input.column, input.row) {
        let index = (input.row - output.y) as usize;
        if index < crate::debug::evaluator_output(state).len() {
            return vec![Event::OpenEvaluatedRow(index)];
        }
    }
    vec![Event::ClickPane(Pane::Evaluator)]
}

/// How many rows of the window the Snippet has, as the reader left it. Here
/// beside the two hit-tests that read it, so the renderer and the mouse ask
/// `layout::evaluator_split` the same question.
fn snippet_rows(state: &State) -> Option<u16> {
    state
        .evaluator
        .as_ref()
        .and_then(|evaluator| evaluator.snippet_rows)
}

/// Which part of the window a press landed on, and nothing at all for one in
/// either half: that is text somebody is picking, and a window that moved
/// when a selection began would be a Snippet nobody could copy out of.
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

/// A coordinate moved by however far the pointer has travelled since the
/// press, which is a signed distance: a window dragged left past column zero
/// stops there rather than wrapping to the far side of the screen.
fn moved_by(at: u16, by: i32) -> u16 {
    (i32::from(at) + by).clamp(0, i32::from(u16::MAX)) as u16
}

/// A Severity label on the Diagnostic list's top border, at the columns `ui`
/// draws them from the same labels, or one of its rows.
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

/// Which of the Diagnostic totals on the tree's top border is under a column,
/// measured off the strings `ui` draws end to end after `tree::title` — the
/// gap each one opens with is no part of it. Nothing past the border's last
/// column, where a title too long for the pane has been cut off.
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
    // Nothing is held until an arm below says it is: the two surfaces that
    // scroll under a drag set this, and every other one leaving it clear is
    // what bounds the edge's cadence by construction.
    pointer.held = None;
    // By pane, exhaustively: a fall-through arm here handed every pane nobody
    // had thought about a span of characters to read, which is how the Buffers
    // pane arrived copying the shell's grid.
    match pane {
        // A filename is not text you copy character by character, so a drag in
        // the tree moves the row selection.
        Pane::Tree => {
            let rows = tree::visible_rows(state);
            let text = text_area(state, panes, pane);
            let (vertical, _) = push(text, input);
            // The row under the pointer, bounded by the rows on screen, and
            // one past that while the drag is held at either end. No
            // scrolling of its own: the clamp in `settle` keeps the tree
            // selection visible, so moving the selection *is* the scroll —
            // the same rule that makes the Down arrow scroll the tree.
            let at = row_index(state, panes, clamped(input.row, text.y, text.bottom()));
            let stepped = match vertical {
                Some(Direction::Up) => at.saturating_sub(1),
                Some(Direction::Down) => (at + 1).min(rows.len().saturating_sub(1)),
                Some(Direction::Left) | Some(Direction::Right) | None => at,
            };
            // Held only while the next step is a row somewhere else: at either
            // end of the list it is the row the drag is already on, so the
            // cadence stops with the list rather than turning for as long as a
            // button is down.
            pointer.held = (stepped != at).then_some(input);
            Outcome::of(match rows.get(stepped) {
                Some(row) => vec![Event::DragRow(row.path.clone())],
                None => vec![],
            })
        }
        // The corner's panes hold a Row selection, which names something to go
        // to rather than text somebody picked: there is nothing in them to
        // copy, and nothing to copy is not the same as copying whatever the
        // pane behind them holds.
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Variables => Outcome::default(),
        // Moving the window, resizing it and picking text in the Snippet are
        // all drags, and which one this is was decided at the press: the
        // window's chrome grabs, and everything inside it picks text.
        Pane::Evaluator => {
            let Some(grab) = pointer.grab else {
                // The Snippet is a buffer, so a drag in it is the editor's own
                // gesture against the window's own text coordinates — the same
                // span event, because there is nothing here to read off a
                // screen.
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
            // Unclamped, like every other dragged edge: where a window may be
            // is `update`'s one answer, and the mouse only says where the
            // pointer took it.
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
                // The rule goes where the pointer is, counted off the window
                // as it stands: an absolute row needs no anchor, and the row
                // the reader is looking at is the row they are dragging.
                Took::Rule => {
                    let rows = input.row.saturating_sub(panes.evaluator.y + 1);
                    return Outcome::of(vec![Event::SizeSnippet(rows)]);
                }
            };
            Outcome::of(vec![Event::PlaceEvaluator(placed)])
        }
        Pane::Editor => {
            // A drag that began in the mirror stays a travel however far the
            // pointer wanders, and a selection that wanders into the mirror
            // stays a selection: the gesture is decided by where the button
            // went down. Deciding it by where the pointer is now would have
            // every travel start picking text the moment it left the strip.
            let strip = crate::minimap::strip(state, panes.editor);
            if strip.holds(from.0, from.1) {
                return Outcome::of(vec![Event::DragMinimap(minimap_row(strip, input.row))]);
            }
            // Dragging the diff's gutter picks the lines a comment covers — the
            // mouse equivalent of V then c.
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
            // The editor needs nothing read off a screen: the span names lines
            // and columns of a buffer this crate already holds, so the drag
            // finishes here.
            if state.current_buffer.is_none() {
                return Outcome::default();
            }
            let text = text_area(state, panes, pane);
            // A diff has no cursor to place, so there is nothing for the clamp
            // in `settle` to follow and nothing here can scroll it.
            let (vertical, horizontal) = match state.diff.is_some() {
                true => (None, None),
                false => push(text, input),
            };
            let (at, end) = match vertical.is_some() || horizontal.is_some() {
                // The pointer brought back onto the text before it is read as
                // a place, and then one step past it: a drag ten rows below
                // the pane is still one step, because the rate is fixed and
                // the view has to keep up with whatever the selection names.
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
                // A drag inside the pane is the drag it always was: neither
                // clamped nor bounded, because both would change where it
                // lands, and this is the arm that has to behave exactly as it
                // did. It is also the one place a drag can be outside the text
                // without pushing, which is a diff.
                false => {
                    let at = place_in(state, panes, pane, (input.column, input.row));
                    (at, at)
                }
            };
            // Held only while the step lands somewhere new. At the end of the
            // text it lands where the drag already is, so the selection stops
            // growing, no caret is moved to where it already sits, and the
            // cadence stops with it: bounded by the text and not only by the
            // button. It is also what keeps a drag on the first visible row of
            // an unscrolled buffer the ordinary drag it looks like.
            pointer.held = (end != at).then_some(input);
            let anchor = *pointer
                .anchor
                .get_or_insert_with(|| place_in(state, panes, pane, from));
            let mut events = Vec::new();
            // The caret goes where the drag has got to, but only while it is
            // pushing — that is the whole of the scrolling. `settle` pulls the
            // view back over the caret on every event a person caused, so a
            // caret one place past the edge is a view one step further on. A
            // drag that stays inside the pane moves no caret and behaves
            // exactly as it did.
            if pointer.held.is_some() {
                events.push(Event::ClickText(end));
            }
            let (from, to) = order(anchor, end);
            events.push(Event::DragText { from, to });
            Outcome::of(events)
        }
        // A pty's cells are the child's, so this half of the drag is the edge's
        // to finish.
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

/// Which row of the mirror the pointer is holding, 0-based. Clamped to the
/// strip's own rows, so a drag that runs off the pane's edge travels to the end
/// rather than stopping where the rectangle does — the row, not the line drawn
/// on it, for the reason `minimap::travel` gives.
///
/// Takes the strip rather than hit-testing it a second time: both callers have
/// already asked whether the press is in it, and a hidden strip is zero-width,
/// which `Area::holds` answers `false` for — so there is no second answer here
/// for "the mirror is not showing", and nothing goes red if one is written.
fn minimap_row(strip: Area, row: u16) -> u32 {
    u32::from(row.clamp(strip.y, strip.bottom().saturating_sub(1)) - strip.y)
}

/// The span a drag covers, ordered so `from` is the earlier place however the
/// pointer travelled — a drag upwards names the same characters as the drag
/// back down over them.
fn order(start: Place, end: Place) -> (Place, Place) {
    match (start.line, start.column) <= (end.line, end.column) {
        true => (start, end),
        false => (end, start),
    }
}

/// A coordinate brought back inside a half-open range, whose last value is
/// `past - 1`. Not `clamp`, which panics when the two bounds cross: a pane
/// narrower than its own gutter has no text columns at all, and a window eight
/// rows tall with an occupied Corner is exactly that shape.
fn clamped(at: u16, first: u16, past: u16) -> u16 {
    at.clamp(first, past.saturating_sub(1).max(first))
}

/// Which way a drag is pushing the view, one answer per axis: nothing while the
/// pointer is inside the pane's text, and the direction it has run out of once
/// it reaches the first or last row or column. The last row of *text*, not the
/// border under it — a drag on the final visible line already means "I want
/// what is below this". Two answers rather than one direction, because a drag
/// into a corner moves both axes.
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

/// A held drag's place, bounded by the text there is: below the last row, or
/// past the end of the row it lands on, it asks for that same place every tick
/// rather than running off into text nobody can see while the caret and the
/// view have both already stopped. That equality is what ends the cadence, so
/// a surface left unbounded here is a surface a held drag scrolls for ever.
///
/// A Preview is bounded against its *rows*, which costs the markdown parse ADR
/// 0007 warns about paying twice in an event. Paid anyway, and only on a report
/// that is actually pushing: a `Place` in a Preview is a rendered row and not a
/// source line, so the buffer's lines are the wrong ruler — and unbounded is
/// not a cheaper answer, it is a drag that nothing stops.
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
        // One past the last character, which is where a caret at the end of a
        // row sits and where a span that takes the whole row ends.
        column: at.column.min(
            lines
                .get(line - 1)
                .map_or(1, |text| text.chars().count() + 1),
        ),
    }
}

/// The place a drag held against an edge names: one step past the last one on
/// screen, which is the character the reader is asking to see. One step per
/// report and per tick whatever the pointer's distance — a fixed rate, so the
/// selection and the view move together.
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

/// Which place in the buffer the pointer is over, the Hover box, or neither.
/// Neither is what says the pointer has left: the border row the Transport
/// lives on, another pane, the gap between them, a diff, a Preview and an empty
/// editor are one answer — there is no symbol under it. The box is asked first
/// because it is drawn over the text and over the panes beside it.
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

/// A Chip on the Hover's top border, or one of its value rows. `None` where
/// the pointer is not on the box at all, which is what leaves the press to the
/// pane underneath — the box floats over the text and over the panes beside
/// it, so it is asked first, the order `resting` asks in and for its reason.
fn pressed_in_hover(state: &State, panes: &Layout, input: Input) -> Option<Vec<Event>> {
    let spot = hover_spot(state, panes).filter(|spot| spot.holds(input.column, input.row))?;
    let chips = crate::debug::hover_chips(state);
    // A box with nothing to act on does not take the press at all: outside a
    // session it carries no Chips and no value, and a box that swallowed a
    // click there would be a change to the Hover this feature promised to
    // leave alone.
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
    // Through the box's own scroll, for the reason every list's rows are read
    // through theirs: a box scrolled down and hit-tested from its first row
    // opens whatever has moved into the row that was clicked.
    let first = state.hover.as_ref()?.first;
    let row = usize::from(input.row.saturating_sub(spot.y + 1)) + first;
    Some(match crate::lsp::sections(state).get(row) {
        Some(crate::lsp::Said::Value(_)) => vec![Event::OpenHoverRow(row)],
        // The border, the docs and the line that declines to evaluate are all
        // read: a box swallows the press rather than letting it place a caret
        // under itself.
        _ => Vec::new(),
    })
}

/// Whether the pointer is on the Hover box, border and all, against the
/// rectangle the renderer draws it in.
fn on_hover(state: &State, panes: &Layout, input: Input) -> bool {
    hover_spot(state, panes).is_some_and(|spot| spot.holds(input.column, input.row))
}

/// The rectangle the Hover box is drawn in, which the press below is
/// hit-tested against for the reason every other one is: one derivation, or a
/// Chip is clicked a column away from where it was drawn.
fn hover_spot(state: &State, panes: &Layout) -> Option<crate::layout::Area> {
    Some(crate::lsp::placement(state)?.spot(state, panes))
}

/// Where a screen position sits in the pane's own text, 1-based like the
/// cursor. The editor's text starts past the line-number gutter, its first row
/// is whatever it is scrolled to and its first column whatever it is scrolled
/// sideways to; a pty's grid starts at the border and is never scrolled past,
/// because the span names cells of the screen it is showing and there is no
/// history behind a full-screen program (ADR-0002).
///
/// Exhaustive on purpose: the AI pane used to fall into the terminal's arm, so
/// every drag in it asked for a span of the wrong rectangle.
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
        | Pane::Variables => (0, 0),
    };
    Place {
        // Through `line_at_row`, not straight off the row: Story view draws
        // comment rows between the lines, so the inverse of what the caret and
        // the scroll clamp use. The identity in every other view.
        line: crate::story::line_at_row(state, row.saturating_sub(text.y) as usize + 1 + scroll),
        column: column.saturating_sub(text.x) as usize + 1 + sideways,
    }
}

/// Where a pane's text sits on screen: the interior of its rectangle, past
/// whatever chrome is drawn inside the borders. One answer, because the two
/// questions asked of it are the same question — where a screen cell falls in
/// the text, and which cell is the last one there is — and deriving the origin
/// twice is how a click comes to land a column off the row it extracts. The
/// one-layout rule, one pane in.
///
/// Every pane answers, in its own rectangle: pointing the corner's panes at the
/// shell's is what made a drag in them a span of characters nobody had pointed
/// at. Only the two that scroll under a drag count their rows off
/// [`crate::fits_in`] — the counts the scroll clamp and the renderer read —
/// because only those two have chrome inside the borders that is not text: the
/// editor's line-number gutter and the mirror down its right-hand side, and
/// the filter box at the foot of the tree.
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
            // A Preview draws no line-number gutter, so its rows start five
            // columns to the left of Source's — the same reason `ui` and this
            // hit-test both ask `layout::gutter` rather than each assuming
            // `GUTTER`.
            x: panes.editor.x + 1 + crate::gutter(state),
            y: panes.editor.y + 1,
            width: editor_columns as u16,
            height: editor_rows as u16,
        },
        Pane::Tree => Area {
            height: tree_rows as u16,
            ..interior(panes.tree)
        },
        Pane::Ai => interior(panes.ai),
        // The split with the keyboard, which the press that began any drag
        // here has already chosen.
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
        | Pane::Diagnostics => interior(panes.corner),
        // The Strip's own rectangle less whatever the Program output beside it
        // is taking, which the Debug group has instead of the shells.
        Pane::Variables => interior(panes.terminal),
        Pane::Output => interior(panes.output),
        // The Snippet, which is the text in that window: the output below it
        // is rows of the adapter's tree, not characters somebody picks.
        // Empty while no window is open, which `Area::holds` answers `false`
        // for, so no hit-test has to ask whether there is one.
        Pane::Evaluator => crate::layout::evaluator_split(panes.evaluator, snippet_rows(state)).0,
    }
}

/// Which palette or Chord hint entry sits under the pointer, if one is open —
/// named by the key it offers, so a click is the keystroke and not a second
/// mapping beside it. Rows carrying no key (a heading, a gap, the cancel line)
/// answer nothing.
pub fn palette_entry_at(state: &State, panes: &Layout, column: u16, row: u16) -> Option<char> {
    // The screen, which `ui` centres the box against and sizes the list to.
    // Read off the panes rather than taken as an argument: `tree` and
    // `terminal` tile the screen's height between them by construction.
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
    // The screen, which `ui` centres the box against. Read off the AI pane
    // rather than the terminal: the two were the same number until the AI pane
    // could take the terminal's columns, and a box hit-tested against a
    // narrower screen than it was drawn on is offset from itself.
    let width = panes.ai.right();
    let box_area = layout::overlay(width, height, rows.len() as u16, widest);
    if !box_area.holds(column, row) {
        return None;
    }
    let index = row.checked_sub(box_area.y + 1)? as usize;
    rows.get(index)?.0
}

/// The label the editor's bottom edge ends with, and therefore where the buffer
/// dots sit. Shared so the hit-test and the rendering agree.
pub fn position_label(line: usize, column: usize) -> String {
    format!(" {line}:{column} ")
}

/// Which buffer dot is under the pointer.
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

/// Which row of a list a screen row names, through that list's own scroll
/// offset: a pane's first row is not its list's first row once it has been
/// scrolled. The area and the offset are arguments because the surviving
/// difference between the four lists is exactly those two — each pane keeps its
/// own origin and its own offset, and the corner's three share a rectangle and
/// nothing else, so a shared offset would name whichever row another list was
/// left scrolled to. What must not differ is the arithmetic: the tree's action
/// hit-test used to derive its own without the offset, so a click on an icon
/// armed the action of a row however far the tree was scrolled.
fn list_row(area: Area, row: u16, scroll: usize) -> usize {
    row.saturating_sub(area.y + 1) as usize + scroll
}

/// Which of a row's action icons sits under the pointer. Two columns each, hard
/// against the pane's right-hand border, which is where `ui` right-aligns them
/// — the four hit-tests that ask this had a copy each, and the fourth copy is
/// what the rule of three forbids. An empty list answers nothing, since there
/// is no column an icon could be in.
fn icon_at(actions: &[&'static str], area: Area, column: u16) -> Option<&'static str> {
    let start = (area.x + area.width.saturating_sub(1)).checked_sub(2 * actions.len() as u16)?;
    actions
        .get(column.checked_sub(start)? as usize / 2)
        .copied()
}

/// Which row of the tree's list a screen row names. Named for its three
/// callers — the row hit-test, the drag and the action hit-test — which read
/// one answer rather than each deriving an offset.
fn row_index(state: &State, panes: &Layout, row: u16) -> usize {
    list_row(panes.tree, row, state.tree_scroll)
}

/// Which Transport control, if any, sits under the pointer — on the editor's
/// top border, at the columns `ui` draws them: the same list feeds the glyph,
/// the click and the key, and `layout::strip_at` is the one place their
/// columns are worked out.
fn transport_at(state: &State, panes: &Layout, column: u16) -> Option<&'static str> {
    let chips = crate::reading::transport(state);
    let labels = layout::chip_labels(&chips, panes.editor.width, layout::EDITOR_TITLE);
    let at = layout::strip_at(panes.editor, &labels, column)?;
    Some(chips[at].action)
}

/// Whether a press lands in the gutter's Breakpoint column, on the code a
/// Breakpoint can be set in: not a diff, a Preview or a walked Site.
fn breakpoint_column(state: &State, panes: &Layout, input: Input) -> bool {
    state.view == crate::View::Edit
        && state.diff.is_none()
        && state.walking.is_none()
        && !crate::previewing(state)
        && state.current_buffer.is_some()
        && input.column == panes.editor.x + 1 + layout::BREAKPOINT_COLUMN
        && input.row + 1 < panes.editor.bottom()
}

/// Whether a press lands on a fold's affordance: the toggle in the gutter's
/// pad, or the dots a folded line carries at the end of its text.
///
/// The pad rather than the one column `layout::TOGGLE_COLUMN` names — a
/// one-column target is a target a pointer misses, and the column beside it is
/// air the line does not otherwise use. Only on a line that has a toggle at
/// all: everywhere else those columns are the gutter, and a gutter click puts
/// the caret at the start of the line the way it always did.
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

/// Which clickable icon the pointer is over, wherever it is: a tree row's
/// actions, the Risk list's and the ones on its border, the Cursor history's,
/// and the Transport on the editor's. One dispatcher rather than a hover test
/// per strip, and it reuses the very hit-tests the press uses — an icon that
/// lights up somewhere the click does not land is worse than one that never
/// lights up at all.
///
/// The row guards are `pressed`'s: a border row is chrome, not a row, and
/// reading one as a row names an entry nowhere near the pointer.
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

/// Which action icon, if any, sits under the pointer.
pub fn action_at(state: &State, panes: &Layout, column: u16, row: u16) -> Option<&'static str> {
    let clicked = tree::visible_rows(state)
        .into_iter()
        .nth(row_index(state, panes, row))?;
    icon_at(&tree::row_actions(state, &clicked.path), panes.tree, column)
}

/// Which action icon of the Cursor history pane, if any, sits under the
/// pointer. Only the row the keyboard is on, exactly as the Risk list's is:
/// that is the row the pane draws icons on.
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

/// Which action icon of the Risk list, if any, sits under the pointer. Only the
/// row the keyboard is on: that is the row the pane draws icons on, and the
/// renderer and this read the same arithmetic for the reason the tree and its
/// hit-test do.
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

/// Which icon of the Breakpoint list's focused row sits under the pointer.
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

/// Which Chip of the Variables' focused row sits under the pointer. Only the
/// row the keyboard is on, for the reason the Breakpoint list's are: that is
/// the row the pane draws Chips on.
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

/// Which Chip of the Breakpoint list's Transport sits under the pointer, at
/// the columns `ui` draws them from the same labels.
fn breakpoint_chip_at(state: &State, panes: &Layout, column: u16) -> Option<&'static str> {
    let chips = crate::debug::transport(state);
    let labels = layout::chip_labels(&chips, panes.corner.width, layout::CORNER_TITLE);
    let at = layout::strip_at(panes.corner, &labels, column)?;
    Some(chips[at].action)
}

/// Which of the Risk pane's own action icons sits under the pointer, on the
/// pane's top border — the same columns a row's icons occupy, because `ui`
/// draws both from `risk::pane_actions` at the same offsets.
fn pane_action_at(state: &State, panes: &Layout, column: u16) -> Option<&'static str> {
    icon_at(&crate::risk::pane_actions(state), panes.corner, column)
}

/// The new-file line numbers two gutter rows span.
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

    /// The cell the encoding tests report on, so each expectation reads as the
    /// bytes for one known cell.
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

    // Four buttons, four numbers. A sideways swipe answered with 65 is a child
    // scrolled down by a gesture nobody made sideways, which is the
    // substitution `report` refuses on coordinates and must refuse on buttons.
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

    // The phantom text: a child that enabled legacy tracking cannot parse an
    // SGR report, so it read one as characters typed at its prompt.
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

    // The legacy encoding's fields are one byte offset by 32, so column 224 has
    // no representation. Declining is the decision: wrapping it would report a
    // different cell, and the click would land somewhere nobody pointed at.
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
            // SGR has no such limit, so the same cell is reportable there.
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

    // The bug: hit-testing used the unfiltered tree, so in Review view — and in a
    // filtered tree — a click resolved to the wrong row.
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

        // In Review view the same rows are the changed files, not the tree.
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

    /// The box is drawn over the panes, so a press in one is a press on
    /// something nobody can see. The bug this guards: a click in the tree
    /// behind it opened whatever file was under the box. The wheel still comes
    /// through, because the box is what scrolls on it.
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

    /// A list box takes the wheel the way it takes Up and Down, over the box
    /// or over the panes behind it, so nothing behind it scrolls while it is
    /// open. Sideways is a notch that moves nothing in a column.
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
            // The middle of the screen, where the box is centred; the tree and
            // the editor, which are behind it or beside it.
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
        // With nothing open the wheel is the pane's, as it always was.
        assert!(matches!(
            wheel(&workspace(), Kind::ScrollDown, 5, 1).first(),
            Some(Event::Scroll { .. })
        ));
    }

    /// A press inside the box is the box's, and the row it lands on is read the
    /// way the renderer draws it: headings between the hits, and the list
    /// starting wherever the wheel left it. The bug this guards is the one the
    /// scroll rules exist for — a hit-test that counted hits, or counted from
    /// the top of the list, marks a different file from the one under the
    /// pointer, and `Enter` then opens it.
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
            // Five rows: a.rs, its two hits, b.rs, its one.
            results: crate::search::Results {
                hits: vec![hit("a.rs"), hit("a.rs"), hit("b.rs")],
                truncated: false,
            },
            ..crate::Search::default()
        });
        // The box is inset by three rows in a 26-row screen, and the border and
        // the three header rows come out of it — pinned rather than derived, so
        // moving the list on screen has to be admitted here.
        let top = 7;
        assert_eq!(click(&searching, 10, top - 1), vec![], "the query row");
        assert_eq!(click(&searching, 10, top), vec![], "a file heading");
        assert_eq!(click(&searching, 10, top + 1), vec![Event::SelectHit(0)]);
        assert_eq!(click(&searching, 10, top + 2), vec![Event::SelectHit(1)]);
        assert_eq!(click(&searching, 10, top + 3), vec![], "the next heading");
        assert_eq!(click(&searching, 10, top + 4), vec![Event::SelectHit(2)]);
        assert_eq!(click(&searching, 10, top + 5), vec![], "past the last row");

        // What the wheel left under the pointer, at the very same row.
        searching.search.as_mut().unwrap().scroll = 3;
        assert_eq!(click(&searching, 10, top), vec![], "the heading, scrolled");
        assert_eq!(click(&searching, 10, top + 1), vec![Event::SelectHit(2)]);

        // The bottom border is inside the rectangle and part of no row: the box
        // shows fifteen rows of a twenty-row inset.
        assert_eq!(click(&searching, 10, top + 15), vec![], "the box's chrome");
    }

    // The bug this guards: the renderer scrolls the pane but the hit-test still
    // counted from the top of the list, so every click landed rows too high.
    #[test]
    fn tree_clicks_count_from_the_scrolled_row() {
        let mut scrolled = workspace();
        scrolled.tree_scroll = 1;
        assert_eq!(
            click(&scrolled, 5, 1),
            vec![Event::ClickRow(PathBuf::from("/w/a.rs"))]
        );
    }

    // The bug: the action hit-test counted from the top of the list while the
    // row hit-test counted from the scrolled row, so once the tree was scrolled
    // an action click armed the action of a different row.
    #[test]
    fn action_clicks_count_from_the_scrolled_row() {
        // The tree pane is 30 wide, so its last inner column is 28 and a file's
        // two action icons sit at 25..=28 — the columns tree_lines reserves.
        let mut scrolled = workspace();
        scrolled.tree_scroll = 1;
        scrolled.tree_selection = Some(PathBuf::from("/w/a.rs"));
        assert_eq!(click(&scrolled, 25, 1), vec![Event::RowAction("delete")]);
        assert_eq!(click(&scrolled, 27, 1), vec![Event::RowAction("copy-path")]);

        // And the row scrolled off the top offers nothing at that position: its
        // four folder actions must not be reachable from the row below it.
        let mut folder = workspace();
        folder.tree_scroll = 1;
        folder.tree_selection = Some(PathBuf::from("/w/src"));
        assert_eq!(
            click(&folder, 21, 1),
            vec![Event::ClickRow(PathBuf::from("/w/a.rs"))]
        );
    }

    /// The Risk list has rows of its own, so it is hit-tested against its own
    /// rectangle and its own scroll offset. Against the tree's — which is what
    /// a `list_row` taking neither as an argument would have to use — a click
    /// would name whichever row the tree happened to be scrolled to, in a pane
    /// that is not the tree.
    #[test]
    fn risk_clicks_count_from_the_panes_own_first_row() {
        // The pane sits at row 18 and is 8 rows tall, so its first row inside
        // the borders is 19 — the row the layout's own test pins it at.
        let mut state = risk_workspace();
        assert_eq!(risk_click(&state, 5, 19), vec![Event::ClickRiskRow(0)]);
        assert_eq!(risk_click(&state, 5, 20), vec![Event::ClickRiskRow(1)]);
        // Past the last row is the pane and not a row: there is nothing there
        // to open.
        assert_eq!(
            risk_click(&state, 5, 21),
            vec![Event::ClickPane(Pane::Risk)]
        );
        // Scrolled, the pane's first row is the row it is scrolled to — the
        // renderer reads the same field.
        state.risk_scroll = 1;
        assert_eq!(risk_click(&state, 5, 19), vec![Event::ClickRiskRow(1)]);
    }

    /// The pane's bottom border is chrome, the way its top one is. A list
    /// longer than the pane draws does have a Function at that row's index, so
    /// reading the border as a row opens one nowhere near the pointer — and its
    /// icon columns would arm that row's action. The Buffers pane's hit-test was
    /// cloned from this one, which is where it turned up.
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
        // The pane draws six rows, 19..=24, so 25 is its bottom border.
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

    /// The pane's own two actions are on its top border, at the columns `ui`
    /// draws them: the pane is 30 wide, so two icons take 25..=28. The start
    /// flips to the stop in the same slot, which is the whole point of one
    /// list feeding the icon, the click and the key.
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
        // The border's name, not its icons.
        assert_eq!(
            risk_click(&state, 5, 18),
            vec![Event::ClickPane(Pane::Risk)]
        );
    }

    /// The Risk row's own action icon, on the columns the pane draws it at:
    /// the pane is 30 wide, so one icon sits at 27..=28, hard against the
    /// border — the same last two content columns the tree reserves.
    #[test]
    fn a_click_on_the_risk_rows_icon_asks_for_the_refactor() {
        let state = risk_workspace();
        assert_eq!(
            risk_click(&state, 27, 19),
            vec![Event::RowAction(crate::risk::REFACTOR)]
        );
        // Anywhere else on the row is the row, not its icon.
        assert_eq!(risk_click(&state, 26, 19), vec![Event::ClickRiskRow(0)]);
        // And the row the keyboard is not on draws no icon, so those columns
        // are that row rather than an action of the selected one.
        assert_eq!(risk_click(&state, 27, 20), vec![Event::ClickRiskRow(1)]);
    }

    /// The corner's other occupant, hit-tested against the same rectangle. A
    /// row of it is a buffer, its border is not a row, and a cell past the last
    /// buffer focuses the pane rather than naming a row that is not there — the
    /// pane has no action icons, so there is nothing else a column can mean.
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
        // The top border, which is chrome rather than the first row.
        assert_eq!(click(&two, 5, 18), vec![Event::ClickPane(Pane::Buffers)]);
        assert_eq!(click(&two, 5, 21), vec![Event::ClickPane(Pane::Buffers)]);
        // A list longer than the pane draws, unscrolled: the bottom border is
        // chrome too, and the list having a row at that index is not the
        // question — the pane is six rows tall, so index 6 is drawn nowhere.
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

    /// The Risk list, shown, with two Functions over the threshold — the same
    /// two the pane's own tests use.
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

    /// The corner's third occupant, hit-tested against the same rectangle. A
    /// row of it is a Visit, both borders are chrome, and the icon takes the
    /// row's last two content columns — 27..=28 in a 30-column pane, the same
    /// two the Risk row and the tree row reserve.
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
        // The row the keyboard is on, which is the only row that draws an icon.
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
        // Anywhere else on that row is the row, and the same columns of a row
        // the keyboard is not on draw no icon at all.
        assert_eq!(click(&state, 26, 19), vec![Event::ClickHistoryRow(0)]);
        assert_eq!(click(&state, 27, 20), vec![Event::ClickHistoryRow(1)]);
        // Both borders are chrome. The bottom one matters most: the list is
        // longer than the six rows the pane draws, so it has a Visit at that
        // index and reading it as a row would go somewhere nobody pointed.
        assert_eq!(click(&state, 5, 18), vec![Event::ClickPane(Pane::History)]);
        assert_eq!(click(&state, 5, 25), vec![Event::ClickPane(Pane::History)]);
    }

    /// The fourth occupant: a row, its icon on the row the keyboard is on, and
    /// the Transport on the top border — " ✕ D " five columns wide, with a
    /// column of border between it and the corner at 29.
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

    /// R35.8. The Transport is on the editor's top border, so a click there is
    /// a control and never a place in the text — and a buffer that cannot be
    /// read has no controls, so the same columns are just the pane again.
    ///
    /// Pinned in both of its shapes, every column of each Chip including its
    /// padding, and the border column between two naming neither: the columns
    /// `ui`'s render test pins.
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
        // 120 columns leave the editor 54, too few for the keys: ` ► ` ` « `
        // ` » ` ` ■ ` ` 1.00x `, each followed by a column of border.
        let shed = [
            (22..=24, PLAY_PAUSE),
            (18..=20, PREVIOUS),
            (14..=16, NEXT),
            (10..=12, STOP),
            (2..=8, SPEED),
        ];
        // 220 leave it 124: ` ► :pause ` ` « :prev ` ` » :next ` ` ■ :stop `
        // ` 1.00x :speed `.
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
        // The border's name, not its controls. The corner itself is not
        // tested here: it is the AI divider's handle, which is grabbed before
        // any pane sees the press — `layout`'s own test holds it to naming no
        // control.
        assert_eq!(press(&state, 120, 43, 0), border);
        // A row inside the pane is still text, which is what the border row
        // being tested first has to leave alone.
        assert!(matches!(
            press(&state, 120, 20, 3).as_slice(),
            [Event::ClickText(_)]
        ));
        // A buffer nothing can read draws no Transport, so those columns are
        // the pane and not a control that quietly does nothing.
        state.current_buffer = Some(PathBuf::from("/w/a.rs"));
        assert_eq!(press(&state, 120, 5, 0), border);
    }

    /// Where the pointer rests is the editor's text and nothing else: the row
    /// the Transport lives on is a border, and every other pane is the pointer
    /// having left — which is what takes a box it rested for down. Nothing is
    /// the answer for both, because both mean there is no symbol under it.
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
        // A diff and a Preview draw rows that are not the buffer's lines, so
        // the cell the pointer is on names no place in the text (ADR 0007).
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

    /// A pointer crossing the pane reports a cell at a time, so a link that
    /// was news on every one of them would be a frame per cell. Only the
    /// change is reported, and letting go of the modifier is a change. Where
    /// it rests is reported every time either way: that one is the place, not
    /// the news, and the dwell window is armed off it.
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
                Event::HoverLink(Some(at))
            ]
        );
        state.link = Some(at);
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
        // Column 29 is the tree's right border and also inside the tree pane.
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

    /// The AI pane's left border is the second handle, and it names a width
    /// rather than a column so the tree's divider can move under it.
    #[test]
    fn the_ai_pane_edge_is_grabbed_and_names_a_width() {
        let state = workspace();
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let mut pointer = Pointer::default();
        // Column 84 is the AI pane's left border: 30 tree + 54 editor.
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

    /// The Strip's top border is the handle, and the height it names is the
    /// rows from the pointer to the screen's bottom, left for `update` to
    /// clamp. The row above it is the editor's, whose buffer dots stay
    /// clickable.
    #[test]
    fn the_border_above_the_strip_is_grabbed_and_names_a_height() {
        let state = workspace();
        // 26 rows: the Strip is rows 18 to 25.
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

    /// A tall AI pane runs past the Strip, so its columns on that row are the
    /// AI pane's, not the handle.
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

    /// The Corner's top border is the Strip's too, and its icons stay clickable.
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

    /// The terminal runs the full width beneath, so its rows must reach the
    /// shell rather than a divider that is nowhere near them.
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

    /// The palette is centred on the screen, and `ui` centres it against the
    /// frame. Hit-testing it against the terminal's width agreed with that
    /// only while the terminal spanned the screen — a tall AI pane takes those
    /// columns, and the entries would sit half a pane to the right of where
    /// they were clicked. The same defect `layout::overlay` was extracted to
    /// stop, arriving by the other door.
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
        // And it really does find entries, or the sweep above proves nothing.
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

    /// Every key the Chord hint names is a cell a click lands on, and the click
    /// is that key: the hint is also a menu.
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

    /// The gutter's leftmost column is the Breakpoint column, and the line
    /// number beside it sets nothing — it is a click at the start of the line,
    /// as it always was. A diff has no Breakpoints to set.
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
        // A Preview has no gutter, so the column is its text; and the bottom
        // border is chrome, not the line scrolled beneath it.
        assert!(!click(&previewing(), column, 3).contains(&Event::ToggleBreakpoint(3)));
        let border = panes.editor.bottom() - 1;
        assert!(!matches!(
            click(&editing(), column, border).as_slice(),
            [Event::ToggleBreakpoint(_)]
        ));
    }

    /// A Run mark shares the Breakpoint column and takes its click, unless a
    /// Breakpoint is drawn over it: the one drawn is the one a click takes.
    /// With the offer up, each of its Chips is a click on that choice.
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

    /// Every entry the palette offers is clickable at some cell, at the two
    /// screen heights that matter: 26 rows, which the replay recipe and the
    /// scenarios use, and 24, which is a stock macOS Terminal. The box is sized
    /// from the row count and `ui` renders it with no scroll offset, so a list
    /// longer than the box can draw loses its tail without a word — and the
    /// hit-test loses the same rows, so `Quit` was neither drawn nor clickable
    /// on a 26-row screen once the Corner's two new panes had pushed it off.
    ///
    /// The border row is swept for the same defect from the other side:
    /// `Area::holds` includes it and the index counts from `box.y + 1`, so a
    /// clipped list put the *first clipped row* under the bottom border, and a
    /// click on the box's edge opened the cheatsheet.
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

    /// A tall pane's border runs the whole height, so the half of it beside
    /// the terminal is a handle too. Grabbing it only alongside the editor
    /// would leave two thirds of the border dead and the pane resizable only
    /// from its top rows.
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
        // Row 20 is in the terminal's band; column 84 is the AI pane's border.
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
        // The tree's border down there is still the terminal's own row: the
        // tree stops above it whatever shape the AI pane is in.
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

    /// Both handles clamp against the screen, not against whatever the
    /// terminal has left — a tall AI pane takes columns off the terminal, and
    /// reading its width as the screen's shrank the range of both dividers
    /// every time the shape was asked for.
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

    /// A move with no button down grabs no divider and finishes no drag: where
    /// it is is all it reports, in the editor's own line and column — and
    /// nothing at all once it is over another pane, which is what takes a
    /// diagnostic box down on the way out.
    #[test]
    fn moving_the_pointer_reports_where_it_rests_and_nothing_else() {
        // A file open, because a place in the text is what is reported and a
        // workspace with nothing open has none.
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
        let over_text = moved(&mut pointer, 44, 5);
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

    /// Drags from one screen position to another and reports the last outcome.
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

    /// A buffer previewing, so `place_in` and `dragged` take the Preview arm:
    /// no gutter, and a row rather than a line-and-column.
    fn previewing() -> State {
        let mut state = workspace();
        let path = PathBuf::from("/w/a.rs");
        state.current_buffer = Some(path.clone());
        state
            .buffers
            .insert(path, crate::editor::Buffer::open("", true, 4));
        state
    }

    /// A diff, which draws the two marker columns as well as the numbers.
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

    // A diff's text starts two columns right of Source's, because the marker
    // that says which side a row came from is gutter too. Measured against
    // Source in the same click, since the bug this guards is the two being
    // measured as one — and the slide is what made it visible: an offset the
    // clamp allowed ran two columns past the end of the longest line.
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
        // And the offset counts from wherever the slide left it, the way it
        // does for Source.
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

    // A Preview draws no line-number gutter, so its first column of text sits
    // `GUTTER` columns to the left of Source's — the bug this guards is a click
    // that still assumes `GUTTER` and picks a character nobody pointed at.
    #[test]
    fn a_preview_click_accounts_for_the_absent_gutter() {
        let panes = panes(120, 26, 30, None, 0, 0, Shapes::default());
        // Two columns in from the border: real text in a Preview, since it has
        // no gutter at all, but still inside Source's.
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

    // A Preview drag needs no help from the edge: the rows are already the
    // library's, so the span is a plain `DragText` and never a `Selection`
    // request the way a pty's drag is.
    #[test]
    fn dragging_a_preview_resolves_in_core_without_a_selection_request() {
        let outcome = drag(&previewing(), (36, 1), (40, 3));
        assert_eq!(
            outcome.events,
            // No gutter taken off, so the same screen columns name characters
            // five further into the row than they would in Source.
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

    // The editor's text starts past the border and the line-number gutter, and
    // row 1 of the pane is line 1 of the buffer. Nothing has to be read off a
    // screen for that, so the drag comes back finished.
    #[test]
    fn dragging_in_the_editor_covers_a_span_of_the_buffer() {
        let outcome = drag(&editing(), (40, 1), (44, 3));
        assert_eq!(
            outcome.events,
            vec![Event::DragText {
                from: Place { line: 1, column: 1 },
                to: Place { line: 3, column: 5 },
            }]
        );
        assert!(outcome.select.is_none(), "no pty to read");
    }

    /// Both drags well inside the pane, because the first row and the first
    /// column of text are the edges a held drag pushes against — the span is
    /// what this is about, and a drag that autoscrolls also places a caret.
    #[test]
    fn dragging_upward_covers_what_dragging_downward_covers() {
        let state = editing();
        assert_eq!(
            drag(&state, (44, 3), (40, 2)).events,
            drag(&state, (40, 2), (44, 3)).events
        );
    }

    // Same bug as the tree's: the renderer scrolls the pane, so the row under
    // the pointer is not the buffer's line unless the offset is counted in.
    #[test]
    fn dragging_in_the_editor_counts_from_the_scrolled_line() {
        let mut scrolled = editing();
        scrolled.editor_scroll = 4;
        assert_eq!(
            drag(&scrolled, (41, 2), (41, 2)).events,
            vec![Event::DragText {
                from: Place { line: 6, column: 2 },
                to: Place { line: 6, column: 2 },
            }]
        );
    }

    // The other half of the same bug: once a long line has slid the view
    // sideways, column 1 of the pane is not column 1 of the line, so a drag or
    // a click that ignores the offset picks characters nobody pointed at.
    #[test]
    fn dragging_in_the_editor_counts_from_the_scrolled_column() {
        let mut scrolled = editing();
        scrolled.editor_hscroll = 12;
        assert_eq!(
            drag(&scrolled, (41, 2), (41, 2)).events,
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

    // A click in the text is not just a focus change: it takes the caret.
    #[test]
    fn clicking_in_the_editor_reports_the_place_clicked() {
        assert_eq!(
            click(&editing(), 44, 3),
            vec![Event::ClickText(Place { line: 3, column: 5 })]
        );
    }

    // With no buffer open there is no place to click, and with a diff shown
    // there is no cursor to move.
    #[test]
    fn clicking_the_editor_with_nothing_to_edit_only_focuses_it() {
        assert_eq!(
            click(&workspace(), 40, 3),
            vec![Event::ClickPane(Pane::Editor)]
        );
    }

    // A pty has no buffer to anchor to, so the span is a cell in its grid.
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

    // The AI pane fell into the terminal's arm, so a drag in it named cells of
    // the terminal's grid: the edge then read the shell's screen and the
    // session's output could not be copied at all.
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

    // ADR-0002: an AI CLI runs full-screen, where there is no history behind it,
    // so a span never names a row the pane is not showing.
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

    /// Drives a whole gesture and reports every event it produced.
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

    // A press and a release with nothing between them is a click, and the
    // release is what a child that asked for mouse events gets.
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
                // The cell in the AI pane's own grid, not on the screen: a
                // child's report knows nothing of the panes around it.
                Event::ClickThrough {
                    pane: Pane::Ai,
                    at: Place { line: 2, column: 4 },
                }
            ]
        );
    }

    // The hazard the release exists to avoid: a drag begins with a press, so
    // forwarding the press would activate whatever the selection started on.
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

    /// Drives two presses on the given cells, stamped, through one pointer —
    /// which is what tells a double-click from two clicks.
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

    // The second press on a cell is a double-click only inside the same window
    // a double-tap has, and only on the same cell: two clicks a reader made
    // minutes apart, or on two different words, are two clicks.
    #[test]
    fn two_presses_on_one_cell_inside_the_window_pick_the_word() {
        let state = editing();
        let doubled = Event::DoubleClickText(Place { line: 3, column: 5 });
        assert!(twice(&state, (44, 3, 0), (44, 3, 299)).contains(&doubled));
        assert!(!twice(&state, (44, 3, 0), (44, 3, 301)).contains(&doubled));
        assert!(!twice(&state, (45, 3, 0), (44, 3, 100)).contains(&doubled));
    }

    // Nothing to pick where a press names no place in the text: the pane, a
    // row of the tree and a control on the border are all clicks that happen
    // twice rather than a word under the pointer.
    #[test]
    fn two_presses_where_there_is_no_text_pick_no_word() {
        let state = workspace();
        assert!(!twice(&state, (40, 3, 0), (40, 3, 100))
            .iter()
            .any(|event| matches!(event, Event::DoubleClickText(_))));
    }

    /// A buffer with a foldable block, folded, so the gutter carries a toggle
    /// and the line that opens it carries the dots.
    fn folding() -> State {
        let mut state = workspace();
        let path = PathBuf::from("/w/a.rs");
        let mut buffer = crate::editor::Buffer::open("fn main() {\n    go();\n}\n", false, 4);
        crate::fold::toggle(&mut buffer, false);
        state.buffers.insert(path.clone(), buffer);
        state.current_buffer = Some(path);
        state
    }

    /// The two halves of one affordance: the toggle in the gutter's pad and
    /// the dots at the end of the folded line both open the block, and both
    /// place the caret first so the block toggled is the one it is on.
    ///
    /// A gutter column that is *not* the toggle's stays a plain click, or
    /// every press on a line number would fold something.
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
        // `fn main() {` is eleven characters, so the dots are the twelfth.
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

    /// A terminal has no hand pointer, so resting on an icon is what says it
    /// is one. Reported by name and only when the answer changes: a pointer
    /// crossing a pane sends a report per cell, and each one reaching `update`
    /// would be a frame.
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
        // The last of the two icons a file offers, hard against the border.
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

    /// The four edges and the inside, on one rectangle: the sideways pair has
    /// no scenario of its own on the left, and a corner has to answer twice.
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

    /// The Evaluator's window is moved, resized and picked text out of by the
    /// same button, so which gesture a drag is has to be decided at the press
    /// and held. Every part of the chrome here on one rectangle, because the
    /// one that matters most is the press that grabs *nothing*: a window that
    /// moved when a selection began is a Snippet nobody can copy out of.
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
        // Press, then one report a few cells along: the drag is measured from
        // where the button went down.
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
