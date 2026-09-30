//! Where the panes are.
//!
//! One source of truth: `ui` derives its rectangles from this and the mouse
//! hit-tests against it, so drawing and clicking cannot disagree about where a
//! pane sits. The arithmetic reproduces exactly what ratatui's solver produced
//! when the layout lived there — the tests pin the measured cases.

use crate::Pane;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Area {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Area {
    pub fn right(&self) -> u16 {
        self.x + self.width
    }

    pub fn bottom(&self) -> u16 {
        self.y + self.height
    }

    pub fn holds(&self, column: u16, row: u16) -> bool {
        column >= self.x && column < self.right() && row >= self.y && row < self.bottom()
    }
}

/// Where a strip of right-aligned labels on a pane's top border sits, and how
/// wide it is: each label followed by one space, the last space landing on the
/// column before the corner. Here rather than in `ui` for the reason [`GUTTER`]
/// is — the renderer leaves room for it and the mouse hit-tests against it, and
/// two derivations of one number is a click landing a column off.
///
/// Measured with `unicode-width`, never by counting characters: a glyph a font
/// draws two columns wide and a count reads as one is a bar that overflows the
/// border it was right-aligned on.
pub fn strip_width(labels: &[String]) -> u16 {
    labels
        .iter()
        .map(|label| UnicodeWidthStr::width(label.as_str()) as u16 + 1)
        .sum()
}

/// Which label of that strip is under a column, if any.
pub fn strip_at(area: Area, labels: &[String], column: u16) -> Option<usize> {
    let mut at = (area.x + area.width.saturating_sub(1)).checked_sub(strip_width(labels))?;
    labels.iter().position(|label| {
        let width = UnicodeWidthStr::width(label.as_str()) as u16;
        let hit = column >= at && column < at + width;
        at += width + 1;
        hit
    })
}

/// A Transport's labels, as [`strip_width`] measures them and [`strip_at`]
/// hit-tests them: each Chip its glyph and keys, padded a column each side, or
/// — when the whole strip would not fit in the pane's `width` less the `title`
/// its border keeps — every Chip its glyph alone. All at once, never one
/// by one, so the row has one shape at a given width
/// (`docs/adr/0022-every-action-has-a-chip.md`); and a glyph alone is never
/// cut, since a strip past its pane's width hit-tests as nothing. The one
/// derivation `ui` draws and `mouse` hit-tests, for the reason [`GUTTER`] is.
pub fn chip_labels(chips: &[crate::Chip], width: u16, title: u16) -> Vec<String> {
    let whole: Vec<String> = chips
        .iter()
        .map(|chip| format!(" {} {} ", chip.glyph, chip.keys))
        .collect();
    match strip_width(&whole) <= width.saturating_sub(title) {
        true => whole,
        false => chips
            .iter()
            .map(|chip| format!(" {} ", chip.glyph))
            .collect(),
    }
}

/// The columns of the editor's top border its Transport leaves to the
/// filename and the Authorship before its Chips show their keys: the corners,
/// a name, its dirty mark and its mode. The Chips give before the name does —
/// the keys are in the cheatsheet, and which file this is is nowhere else.
pub const EDITOR_TITLE: u16 = 34;

/// The same for a Corner occupant's top border: the corners and its name.
pub const CORNER_TITLE: u16 = 14;

/// How wide the editor's line-number gutter is, between its border and its
/// text. The renderer draws it and the mouse hit-tests past it, so both read
/// this rather than each counting columns.
///
/// Nine, not five. The first column holds Breakpoints; the next four are the
/// number; the sixth is the bar a diagnostic or a Reading draws; the seventh
/// is the fold toggle; the last two are air between the gutter and the code. A
/// toggle wedged between the last digit and the first character of the code is
/// a target too small to aim a pointer at, and code that starts against the
/// line number is code you read the number as part of.
pub const GUTTER: u16 = 9;

/// Which gutter column Breakpoints sit in, counted from the pane's inside
/// edge: the leftmost, where a JetBrains hand already reaches for them, and
/// apart from the line numbers so a click on a number sets nothing.
pub const BREAKPOINT_COLUMN: u16 = 0;

/// Which gutter column the fold toggle sits in, counted from the pane's inside
/// edge. Here beside [`GUTTER`] for the reason `GUTTER` is here — `ui` draws
/// it and `mouse` hit-tests it, and two derivations of one column is a click
/// landing beside the thing it pointed at.
pub const TOGGLE_COLUMN: u16 = 6;

/// How wide the step-menu is while walking a Story — a fixed constant, the
/// same shape as `GUTTER`, rather than sized to the longest Step name in
/// whichever Story happens to be loaded. A per-Story width would resize the
/// rectangle every time a reviewer entered a different Story, which is
/// exactly the class of bug the "one layout" rule exists to prevent
/// (`docs/adr/0008-a-story-set-is-titled-a-step-is-named.md`).
pub const STEP_MENU_WIDTH: u16 = 20;

/// The two columns a diff spends saying whether a row was added, removed or is
/// context — drawn immediately right of the line number, so they are gutter
/// too. Named here beside [`GUTTER`] rather than counted in `ui`'s format
/// string, for the reason that one is here.
pub const MARKER: u16 = 2;

/// What the editor pane draws between its border and its text. Three answers
/// rather than two, because a diff has one the others do not: a bool said only
/// "Preview or not", so the diff was measured as if it were Source and its
/// last two columns of code were unreachable, its caret sat two columns left
/// of its text and a click on it landed two columns off.
pub enum Gutter {
    /// Source and Story view's code: the line number.
    Numbers,
    /// Review's diff: the line number, and the marker that says which side the
    /// row came from.
    NumbersAndMarker,
    /// A Preview: none. Five columns spent naming rows the reader cannot act
    /// on are five columns not spent on text.
    None,
}

/// How wide that strip is.
///
/// One function rather than a check in the renderer and another in the
/// hit-test, for the reason `GUTTER` lives here at all — the two computed the
/// gutter separately once, which is how a click lands in the wrong column.
pub fn gutter(shows: Gutter) -> u16 {
    match shows {
        Gutter::Numbers => GUTTER,
        Gutter::NumbersAndMarker => GUTTER + MARKER,
        Gutter::None => 0,
    }
}

/// Where the AI pane sits. Beside the editor it stops above the terminal,
/// which spans the width beneath everything. Tall it runs the whole height
/// down the right-hand edge and the terminal ends at its border — the
/// terminal gives up width rather than the AI pane giving up rows, which is
/// the point of asking for it. Its left edge is in the same column either
/// way, so only its height and the terminal's width change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiPane {
    #[default]
    Beside,
    Tall,
}

/// Which pane is in the corner beneath the tree, or none at all. At the tree's
/// width, and the columns come out of the shell pane — the mirror of what the
/// tall AI pane does from the other side, and for the same reason: the pane
/// asked for is the one that gains, and the shell is what there is to give.
///
/// One slot naming its occupant rather than a visibility flag per pane. A flag
/// per pane is exactly what "enums, not booleans" forbids: it can say more than
/// one pane is on screen, which is a state one rectangle cannot draw and one
/// hit-test cannot answer for — so every site that chose between them would
/// need a precedence rule of its own, and the rules would drift. Here "both at
/// once" is unrepresentable, and asking for one while another shows is a
/// replacement with nothing to decide.
///
/// Hidden leaves a zero-width rectangle, which `Area::holds` answers `false`
/// for, so no arm has to remember to ask whether the corner is occupied before
/// hit-testing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Corner {
    #[default]
    Hidden,
    Risk,
    Buffers,
    History,
    Breakpoints,
    Frames,
    /// The Diagnostic list, and which Severity it is showing — held here so
    /// the list and the Severity cannot be out of step with the slot.
    Diagnostics(crate::lsp::Severity),
}

impl Corner {
    /// Which pane the corner is holding, or none at all. One answer, read by
    /// the hit-test, by the focus geometry and by the toggle — the three had a
    /// copy each, so the compiler named the same decision three times and a
    /// fifth occupant was three edits rather than one. Exhaustive, so a new
    /// occupant is still a compiler error, now at the one site that decides it.
    pub fn pane(self) -> Option<Pane> {
        match self {
            Corner::Hidden => None,
            Corner::Risk => Some(Pane::Risk),
            Corner::Buffers => Some(Pane::Buffers),
            Corner::History => Some(Pane::History),
            Corner::Breakpoints => Some(Pane::Breakpoints),
            Corner::Frames => Some(Pane::Frames),
            Corner::Diagnostics(_) => Some(Pane::Diagnostics),
        }
    }
}

/// Which group the Strip is showing. One slot naming its occupant, for the
/// reason [`Corner`] is one: the Debug group joins it, and "both at once" must
/// stay a state nobody can write down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Group {
    #[default]
    Shells,
    /// The Variables, while a Debug session exists. Offered only then: a tab
    /// for a group nothing can be in is a tab that shows an empty Strip.
    Debug,
}

impl Group {
    /// Its Group tab, as the Strip's top border draws it.
    pub fn label(self) -> &'static str {
        match self {
            Group::Shells => "Shells",
            Group::Debug => "Debug",
        }
    }

    /// Which pane the Strip is holding — the one the keyboard goes to when a
    /// group is brought forward. One answer, read by the hit-test, by the
    /// focus geometry and by the Group tabs, for the reason [`Corner::pane`]
    /// is one.
    pub fn pane(self) -> Pane {
        match self {
            Group::Shells => Pane::Terminal,
            Group::Debug => Pane::Variables,
        }
    }

    /// Whether this group is the one `pane` lives in. Not `pane()` compared,
    /// because the Debug group holds two: the keyboard left in the Program
    /// output when the Shells come forward is a keyboard in a pane nobody can
    /// see, which is the whole of what the comparison was there to prevent.
    pub fn holds(self, pane: Pane) -> bool {
        match self {
            Group::Shells => pane == Pane::Terminal,
            Group::Debug => matches!(pane, Pane::Variables | Pane::Output),
        }
    }
}

/// Whether the Debug group is showing the Program output beside the Variables,
/// and how wide it is once the border between them has been dragged — `None`
/// until then, a share of the group, exactly as the AI pane's width is. One
/// value rather than a flag beside a width, for the reason [`Corner`] is one:
/// "hidden and 60 columns wide" is a state the layout has no rectangle for,
/// and the width a hidden output keeps is `State`'s to remember, not the
/// layout's to be told twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Output {
    #[default]
    Away,
    Shown(Option<u16>),
}

/// The fewest columns either side of that border keeps. Small on purpose: the
/// gesture is "give the other one the room", and a floor wide enough to read
/// is a floor that stops the drag well short of where it was aimed. Hiding is
/// how the Program output goes away entirely.
pub const GROUP_LEAST: u16 = 8;

/// The fewest rows the area above the Strip keeps, and the fewest the Strip
/// does: its two borders and the two rows a pty needs, since vt100 underflows
/// on a one-row grid.
pub const TOP_LEAST: u16 = 5;
pub const STRIP_LEAST: u16 = 4;

/// A Strip height, bounded so that neither the Strip nor the area above it
/// vanishes. The area above wins on a screen too short for both, as it always
/// has: a Strip of fewer rows is one the edge clamps its pty against.
pub fn strip_height(screen_height: u16, asked: u16) -> u16 {
    asked
        .max(STRIP_LEAST)
        .min(screen_height.saturating_sub(TOP_LEAST))
}

/// The two panes that take their columns out of the shell, one from each side:
/// the AI pane's shape from the right and the corner from the left, and how
/// tall the Strip they sit in is. One value because they are one question —
/// how much of the bottom row is the shell's — and because a rectangle chosen
/// from a growing list of positional flags is a rectangle nobody can read at
/// the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shapes {
    pub ai: AiPane,
    pub corner: Corner,
    /// Which group the Strip is showing, carried here for the reason
    /// `corner` is: the rectangle is the same either way, and what is in it
    /// is what a hit-test has to answer from the layout alone.
    pub group: Group,
    /// `None` until the border above the Strip is dragged: a share of the
    /// screen until somebody names a height, as the AI pane's width is.
    pub strip: Option<u16>,
    /// Whether the Debug group is showing the Program output, and how wide.
    pub output: Output,
    /// Where the Evaluator's window is, and `None` while none is open. It
    /// floats over the editor and takes no columns from anything, so it
    /// changes no other rectangle — but where it is has to come from here, so
    /// the renderer and the hit-test read one answer, for the reason every
    /// other pane does. A rectangle rather than a flag because the window is
    /// moved and resized: `update` holds the rectangle and clamps it, the way
    /// it holds every other view's offset.
    pub evaluator: Option<Area>,
}

/// The least a window may be squeezed to: its two borders, a row of Snippet,
/// the rule between them and a row of output — and columns enough for the
/// Chips on its top border. A window smaller than this is one with nothing
/// readable in it, which is not a window somebody meant to drag.
pub const WINDOW_LEAST_WIDTH: u16 = 24;
pub const WINDOW_LEAST_HEIGHT: u16 = 5;

/// Where the Evaluator opens when the project has never left it anywhere: the
/// middle of the screen, three fifths across and half of it down, so the code
/// it floats over is still readable around it.
pub fn centred_window(width: u16, height: u16) -> Area {
    let box_width = (width as u32 * 3).div_ceil(5) as u16;
    let box_height = height / 2;
    Area {
        x: (width - box_width) / 2,
        y: (height - box_height) / 2,
        width: box_width,
        height: box_height,
    }
}

/// Where a floating window ends up once the screen and the Paused line have
/// had their say: never smaller than the floor above, never bigger than the
/// screen, wholly on it, and clear of the row `clear` names.
///
/// One function for both because they are one question — where may this
/// window be? — and a second author for a rectangle is a window drawn where
/// nothing hit-tests it. Called from `update` for every event, the way the
/// scroll offsets are clamped there: a drag, a resize and a program stopping
/// somewhere new all move the answer, and an arm that forgot to ask is a
/// window half off the screen.
///
/// The row and not its columns: the Paused line's wash runs the width of the
/// editor, so a window clear of it vertically is clear of it. Below it by
/// preference and above it where the screen leaves no room below, and left
/// where it is when neither fits — a window taller than the screen covers
/// every row there is, and shuffling it would only hide something else.
pub fn placed_window(window: Area, width: u16, height: u16, clear: Option<u16>) -> Area {
    let box_width = window.width.max(WINDOW_LEAST_WIDTH).min(width.max(1));
    let box_height = window.height.max(WINDOW_LEAST_HEIGHT).min(height.max(1));
    let mut placed = Area {
        x: window.x.min(width.saturating_sub(box_width)),
        y: window.y.min(height.saturating_sub(box_height)),
        width: box_width,
        height: box_height,
    };
    if let Some(row) = clear.filter(|row| (placed.y..placed.bottom()).contains(row)) {
        if row + 1 + box_height <= height {
            placed.y = row + 1;
        } else if box_height <= row {
            placed.y = row - box_height;
        }
    }
    placed
}

/// Where the Snippet and the Evaluator output are inside that window: the
/// Snippet takes `asked` rows — half of what the borders leave until the rule
/// between them is dragged — the output the rest, and one row between them
/// carries that rule. One answer, which `ui` draws and the mouse hit-tests,
/// for the reason [`GUTTER`] is one.
///
/// Clamped here rather than where the drag is read: both sides keep a row,
/// because a half dragged to nothing is a half nobody can drag back.
pub fn evaluator_split(window: Area, asked: Option<u16>) -> (Area, Area) {
    let interior = Area {
        x: window.x + 1,
        y: window.y + 1,
        width: window.width.saturating_sub(2),
        height: window.height.saturating_sub(2),
    };
    let half = interior.height.saturating_sub(1) / 2;
    let snippet = asked
        .unwrap_or(half)
        .clamp(1, interior.height.saturating_sub(2).max(1));
    (
        Area {
            height: snippet,
            ..interior
        },
        Area {
            y: interior.y + snippet + 1,
            height: interior.height.saturating_sub(snippet + 1),
            ..interior
        },
    )
}

/// The first content row a pane shows: where the wheel left it, pulled back so
/// the row you are on stays visible, and never further than the last screenful.
/// `focus` and the result are 0-based; `fits` is how many rows the pane shows,
/// which is not its height — chrome inside the borders takes rows too.
pub fn viewport(offset: usize, focus: usize, rows: usize, fits: usize) -> usize {
    let fits = fits.max(1);
    let offset = offset.min(rows.saturating_sub(fits));
    if focus < offset {
        return focus;
    }
    if focus >= offset + fits {
        return focus + 1 - fits;
    }
    offset
}

/// How many rows of context a framed range keeps above it when the pane has
/// rows to spare after it — enough that the range is not pinned hard against
/// the pane's top edge, and never enough to push its end off the bottom.
const CONTEXT: usize = 2;

/// The first content row a pane shows so that a whole *range* is on screen —
/// what [`viewport`] is for a cursor. `varde::frame_site` argues why the two
/// are different questions; here, `first` and `last` are 0-based rows and
/// `fits` is how many rows the pane shows. A range taller than the pane is
/// framed from its top: it cannot be shown whole, and it is read downward from
/// where it starts.
pub fn frame(first: usize, last: usize, fits: usize) -> usize {
    let height = last.saturating_sub(first) + 1;
    first.saturating_sub(CONTEXT.min(fits.max(1).saturating_sub(height)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Layout {
    pub tree: Area,
    pub editor: Area,
    pub ai: Area,
    pub terminal: Area,
    /// The narration band, directly under the editor — a rectangle on
    /// `Layout` rather than one `ui` or `mouse` each compute on their own, so
    /// drawing and hit-testing can never disagree about where it is. Empty
    /// (zero height) whenever `band_height` is 0.
    pub band: Area,
    /// The step-menu, directly left of the editor's gutter — a spatial
    /// index onto the Story being walked, not a control. Empty (zero width)
    /// whenever `step_menu_width` is 0.
    pub step_menu: Area,
    /// The corner beneath the tree, beside the shell. Empty (zero width)
    /// whenever nothing occupies it.
    pub corner: Area,
    /// Which pane that rectangle belongs to, carried here so `pane_at` answers
    /// from the layout alone — the alternative is every hit-test taking the
    /// occupant as a second argument and one of them forgetting.
    pub occupant: Corner,
    /// The same for the Strip, whose one rectangle is the shells or the Debug
    /// group: `terminal` is where it is, and this is whose it is.
    pub group: Group,
    /// The Program output, at the Strip's right-hand end, with the Variables
    /// keeping what is left. Empty (zero width) whenever it is not showing —
    /// which `Area::holds` answers `false` for, so no hit-test has to ask
    /// whether there is one before asking where it is.
    pub output: Area,
    /// The Evaluator's floating window. Empty whenever none is open, for the
    /// reason the Program output's is empty while it is hidden.
    pub evaluator: Area,
}

impl Layout {
    /// The Strip's whole rectangle: the Variables and the Program output
    /// together, or the shells. Where its top border is — which the Group tabs
    /// and the Variables' Transport are right-aligned on and hit-tested
    /// against, and which the handle that drags its height runs along.
    /// `terminal` alone stops at the border between the two, so a strip of
    /// labels measured against it would slide every time that border was
    /// dragged and vanish altogether once the Variables were squeezed to
    /// [`GROUP_LEAST`].
    pub fn strip(&self) -> Area {
        Area {
            width: self.terminal.width + self.output.width,
            ..self.terminal
        }
    }
}

/// Tree, editor and AI across the top; terminal beneath. The terminal takes 30%
/// of the height, or the Strip's dragged height, and the AI pane 30% of the
/// width, except that the editor keeps at least 20 columns and the top keeps
/// at least [`TOP_LEAST`] rows. `band_height` is 0
/// outside Story view's walk; the editor comes back already shortened by it,
/// so no caller has to remember to subtract it a second time.
///
/// `ai_width` is `None` until the pane's edge is dragged: a share of the screen
/// until somebody names a width, and after that the width they named — which is
/// what the tree does either side of it, and what keeps a resized terminal from
/// silently reflowing the child living in there. `ai` says whether that column
/// stops above the terminal or runs the whole height past it.
pub fn panes(
    width: u16,
    height: u16,
    tree_divider: u16,
    ai_width: Option<u16>,
    band_height: u16,
    step_menu_width: u16,
    shapes: Shapes,
) -> Layout {
    let terminal_height = shapes
        .strip
        .unwrap_or(((height as u32 * 3 + 5) / 10) as u16)
        .min(height.saturating_sub(TOP_LEAST));
    let top = height - terminal_height;

    let tree_width = tree_divider.min(width);
    let room = width.saturating_sub(tree_width);
    let share = ((width as u32 * 3 + 5) / 10) as u16;
    let ai_width = ai_width.unwrap_or(share).min(room.saturating_sub(20));
    let step_menu_width = step_menu_width.min(room.saturating_sub(ai_width).saturating_sub(20));
    let editor_width = room - ai_width - step_menu_width;
    let band_height = band_height.min(top);
    let editor_height = top - band_height;
    let (ai_height, shell_room) = match shapes.ai {
        AiPane::Beside => (top, width),
        AiPane::Tall => (height, width.saturating_sub(ai_width)),
    };
    // The corner takes its columns from the shell and nothing else: a floor of
    // one column, because a tall AI pane and a wide tree can between them ask
    // for more than the screen has, and a shell of zero columns is a pty vt100
    // panics on.
    // Its own width is where the shell starts, so the two tile whatever the
    // floor does to them. Every occupant is the same rectangle — which pane is
    // in it changes what is drawn, never where.
    let corner_width = match shapes.corner {
        Corner::Hidden => 0,
        _ => tree_width.min(shell_room.saturating_sub(1)),
    };
    let strip_width = shell_room.saturating_sub(corner_width).max(1);
    // The Program output takes its columns out of the Strip's right-hand end,
    // the way the corner takes its out of the left: the Variables keep what is
    // left, and neither can be squeezed past `GROUP_LEAST`.
    let output_width = match shapes.output {
        Output::Away => 0,
        Output::Shown(asked) => {
            let least = GROUP_LEAST.min(strip_width);
            let most = strip_width.saturating_sub(GROUP_LEAST).max(least);
            asked.unwrap_or(strip_width / 2).clamp(least, most)
        }
    };
    let terminal_width = strip_width - output_width;

    Layout {
        tree: Area {
            x: 0,
            y: 0,
            width: tree_width,
            height: top,
        },
        editor: Area {
            x: tree_width + step_menu_width,
            y: 0,
            width: editor_width,
            height: editor_height,
        },
        step_menu: Area {
            x: tree_width,
            y: 0,
            // The full top height, not `editor_height`: the band sits under
            // the editor's own column, never the step-menu's, so spanning
            // only `editor_height` here would leave the rows beside the band
            // belonging to no pane at all.
            width: step_menu_width,
            height: top,
        },
        ai: Area {
            x: tree_width + step_menu_width + editor_width,
            y: 0,
            width: ai_width,
            height: ai_height,
        },
        terminal: Area {
            x: corner_width,
            y: top,
            width: terminal_width,
            height: terminal_height,
        },
        corner: Area {
            x: 0,
            y: top,
            width: corner_width,
            height: terminal_height,
        },
        output: Area {
            x: corner_width + terminal_width,
            y: top,
            width: output_width,
            height: terminal_height,
        },
        occupant: shapes.corner,
        group: shapes.group,
        band: Area {
            x: tree_width + step_menu_width,
            y: editor_height,
            width: editor_width,
            height: band_height,
        },
        evaluator: shapes.evaluator.unwrap_or_default(),
    }
}

/// The pty size a pane's rectangle asks for: its interior, clamped so vt100
/// never sees a grid it panics on — two rows and not one, because wrapping a
/// column needs a row to scroll into and on a one-row grid vt100 subtracts the
/// scroll off the row it came from.
///
/// `None` for a rectangle with nothing in it, which is a pane that is not on
/// screen: a hidden Program output squeezed to the floor would reflow
/// everything its child had printed, and showing it again would bring back
/// something nobody could read.
pub fn pty_size(width: u16, height: u16) -> Option<(u16, u16)> {
    (width > 0 && height > 0).then(|| {
        (
            height.saturating_sub(2).max(2),
            width.saturating_sub(2).max(1),
        )
    })
}

/// A centred overlay box sized to its content. Shared so that what is drawn and
/// what is clickable are the same rectangle — they were not, and the palette's
/// click band was four columns wider than its box.
pub fn overlay(width: u16, height: u16, lines: u16, widest: u16) -> Area {
    let box_width = (widest + 4).clamp(24, width.max(24));
    let box_height = (lines + 2).min(height);
    Area {
        x: width.saturating_sub(box_width) / 2,
        y: height.saturating_sub(box_height) / 2,
        width: box_width,
        height: box_height,
    }
}

/// A box inset from every edge, so what is behind it stays visible. Shrinks the
/// margin rather than the box on a small terminal.
pub fn inset(width: u16, height: u16, columns: u16, rows: u16) -> Area {
    let columns = columns.min(width.saturating_sub(40) / 2);
    let rows = rows.min(height.saturating_sub(10) / 2);
    Area {
        x: columns,
        y: rows,
        width: width.saturating_sub(columns * 2),
        height: height.saturating_sub(rows * 2),
    }
}

/// Where the project search's results box is, and how many rows of results it
/// shows. Here rather than in `ui` for the reason [`GUTTER`] is: the scroll
/// clamp has to count the rows the renderer draws, and the two computing the
/// inset separately is how a list comes to be clamped against a box of another
/// size. `SEARCH_HEADER` is the three rows above the list — the query, its
/// count, and the row naming the box's own keys — which are chrome inside the
/// borders, and a pane's row count is not its height. The mouse counts from
/// past them too: the first result row is `y + 1 + SEARCH_HEADER`.
pub const SEARCH_HEADER: u16 = 3;

pub fn search_box(width: u16, height: u16) -> Area {
    inset(width, height, 8, 3)
}

/// The replace box: floating in the editor's top-right corner, inside its
/// border, so the text it covers is the text furthest from the `/` line.
/// Three rows inside — find, with, and the two buttons — which `ui` draws and
/// `mouse` hit-tests by their offset from `y + 1`.
pub fn replace_box(editor: Area) -> Area {
    let width = editor.width.saturating_sub(2).min(44);
    Area {
        x: (editor.x + editor.width).saturating_sub(1 + width),
        y: editor.y + 1,
        width,
        height: editor.height.saturating_sub(2).min(5),
    }
}

/// Where the replace box draws `[Aa]` — against its right border on the
/// "find" row — and each of its buttons on the third row, a column apart.
/// `ui` renders into these rectangles and `mouse` hit-tests them, so neither
/// works out a column of its own.
pub fn replace_case(spot: Area) -> Area {
    let width = crate::FIND_ICONS[0].1.len() as u16;
    Area {
        x: spot.right().saturating_sub(2 + width),
        y: spot.y + 1,
        width,
        height: 1,
    }
}

pub fn replace_buttons(spot: Area) -> Vec<(crate::ReplaceField, Area)> {
    let mut x = spot.x + 2;
    crate::REPLACE_BUTTONS
        .iter()
        .map(|(field, label)| {
            let width = label.len() as u16;
            let at = Area {
                x,
                y: spot.y + 3,
                width,
                height: 1,
            };
            x += width + 1;
            (*field, at)
        })
        .collect()
}

pub fn search_hit_rows(width: u16, height: u16) -> usize {
    search_box(width, height)
        .height
        .saturating_sub(2 + SEARCH_HEADER) as usize
}

/// The `k`-th of `n` shells side by side in the terminal strip. Even columns,
/// the last taking the remainder, so the splits tile the strip exactly — `ui`
/// draws these and `mouse` hit-tests them, for the one-layout reason.
pub fn split(strip: Area, n: usize, k: usize) -> Area {
    let n = n.clamp(1, usize::from(u16::MAX)) as u16;
    let k = (k.min(usize::from(n) - 1)) as u16;
    let each = strip.width / n;
    let x = strip.x + each * k;
    Area {
        x,
        y: strip.y,
        width: if k + 1 == n { strip.right() - x } else { each },
        height: strip.height,
    }
}

/// Which of `n` splits holds `column` — the last one whose left edge is at or
/// before it, so the remainder the last split takes is its own.
pub fn split_at(strip: Area, n: usize, column: u16) -> usize {
    (0..n.max(1))
        .rev()
        .find(|&k| split(strip, n, k).x <= column)
        .unwrap_or(0)
}

pub fn pane_at(layout: &Layout, column: u16, row: u16) -> Option<Pane> {
    // The Evaluator floats over the panes, so it is asked first: a click that
    // landed on the window belongs to the window whatever is drawn under it.
    // Empty while none is open, which `holds` answers `false` for.
    if layout.evaluator.holds(column, row) {
        return Some(Pane::Evaluator);
    }
    // Before the shell, and never folded into the tree's: a click in the corner
    // means something entirely different from a click in either, and a pane
    // hit-tested against its neighbour's rectangle is every drag in it asking
    // for a span of the wrong pane.
    if layout.corner.holds(column, row) {
        layout.occupant.pane()
    } else if layout.output.holds(column, row) {
        Some(Pane::Output)
    } else if layout.tree.holds(column, row) {
        Some(Pane::Tree)
    } else if layout.editor.holds(column, row) || layout.band.holds(column, row) {
        Some(Pane::Editor)
    } else if layout.ai.holds(column, row) {
        Some(Pane::Ai)
    } else if layout.terminal.holds(column, row) {
        Some(layout.group.pane())
    } else {
        None
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

    /// The Debug group tiles the Strip: the Variables keep what the Program
    /// output does not take, neither is squeezed past `GROUP_LEAST`, and
    /// hiding it gives the Variables every column back. Pinned here because
    /// two rectangles that do not tile are a click landing in a pane nobody
    /// pointed at.
    #[test]
    fn the_program_output_tiles_the_strip_with_the_variables() {
        let group = |output| {
            let layout = panes(
                120,
                40,
                30,
                None,
                0,
                0,
                Shapes {
                    corner: Corner::Frames,
                    group: Group::Debug,
                    output,
                    ..Shapes::default()
                },
            );
            (layout.terminal, layout.output)
        };
        let (variables, output) = group(Output::Away);
        assert_eq!((variables.x, variables.width), (30, 90));
        assert_eq!(output.width, 0);
        assert!(!output.holds(100, variables.y + 1));

        let (variables, output) = group(Output::Shown(Some(60)));
        assert_eq!((variables.x, variables.width), (30, 30));
        assert_eq!((output.x, output.width), (60, 60));
        assert_eq!(variables.right(), output.x);
        assert_eq!(output.right(), 120);

        // Dragged past either floor, the other side keeps `GROUP_LEAST`.
        assert_eq!(group(Output::Shown(Some(120))).0.width, GROUP_LEAST);
        assert_eq!(group(Output::Shown(Some(0))).1.width, GROUP_LEAST);
    }

    /// Where the Evaluator opens and how its two halves divide it. Pinned
    /// here for the reason every other rectangle is: the renderer draws these
    /// numbers and the mouse hit-tests them, and a window nobody can see the
    /// code around is the one thing it may not be.
    #[test]
    fn the_evaluator_is_centred_and_split_between_its_snippet_and_its_output() {
        let window = |open| {
            panes(
                120,
                40,
                30,
                None,
                0,
                0,
                Shapes {
                    evaluator: open,
                    ..Shapes::default()
                },
            )
            .evaluator
        };
        // Nothing at all while none is open, which `holds` answers `false`
        // for — so no hit-test has to ask whether there is one.
        assert!(!window(None).holds(60, 20));
        let open = window(Some(centred_window(120, 40)));
        assert_eq!(
            open,
            Area {
                x: 24,
                y: 10,
                width: 72,
                height: 20
            }
        );
        // Centred: the same room either side and above and below.
        assert_eq!(open.x, 120 - open.right());
        assert_eq!(open.y, 40 - open.bottom());

        let (snippet, output) = super::evaluator_split(open, None);
        // Both inside the borders, and the row between them belongs to
        // neither: it is the rule the renderer draws there.
        assert_eq!((snippet.x, snippet.y, snippet.height), (25, 11, 8));
        assert_eq!((output.x, output.y, output.height), (25, 20, 9));
        assert_eq!(snippet.bottom() + 1, output.y);
        assert_eq!(output.bottom(), open.bottom() - 1);
        assert_eq!((snippet.width, output.width), (70, 70));

        // The rule dragged up: the Snippet takes what it was asked for and
        // the output takes the rest, so a row given up by one is a row the
        // other gains.
        let (snippet, output) = super::evaluator_split(open, Some(4));
        assert_eq!((snippet.height, output.height), (4, 13));
        assert_eq!(snippet.bottom() + 1, output.y);
        // Neither side may be dragged away: a half with no rows is a half
        // nobody can drag back.
        assert_eq!(super::evaluator_split(open, Some(0)).0.height, 1);
        assert_eq!(super::evaluator_split(open, Some(99)).1.height, 1);
    }

    /// The window's rectangle is state, so every event is answered from one
    /// clamp: it stays on the screen it is drawn on, it is never squeezed to
    /// nothing, and it never covers the row the program is stopped on.
    #[test]
    fn a_window_is_placed_on_the_screen_and_clear_of_the_paused_line() {
        let window = Area {
            x: 20,
            y: 5,
            width: 60,
            height: 12,
        };
        assert_eq!(placed_window(window, 120, 40, None), window);
        // A screen it no longer fits on takes it back by the corner it hangs
        // off, and a screen smaller than the floor takes the floor with it.
        let squeezed = placed_window(window, 60, 12, None);
        assert_eq!((squeezed.x, squeezed.right()), (0, 60));
        assert_eq!((squeezed.y, squeezed.bottom()), (0, 12));
        assert_eq!(
            placed_window(window, 10, 3, None),
            Area {
                x: 0,
                y: 0,
                width: 10,
                height: 3
            }
        );
        // The Paused line: below it where the screen leaves room, above it
        // where it does not, and where it was when neither fits.
        assert_eq!(placed_window(window, 120, 40, Some(8)).y, 9);
        assert_eq!(placed_window(window, 120, 20, Some(14)).y, 2);
        assert_eq!(placed_window(window, 120, 12, Some(6)).y, 0);
        // A row it does not cover moves it not at all.
        assert_eq!(placed_window(window, 120, 40, Some(30)), window);
    }

    /// A pane with no rectangle asks for no pty, and one with a rectangle
    /// never asks for a grid vt100 panics on.
    #[test]
    fn a_hidden_pane_asks_for_no_pty_and_a_tiny_one_asks_for_the_floor() {
        assert_eq!(pty_size(0, 10), None);
        assert_eq!(pty_size(10, 0), None);
        assert_eq!(pty_size(1, 1), Some((2, 1)));
        assert_eq!(pty_size(62, 12), Some((10, 60)));
    }

    #[test]
    fn splits_tile_the_strip_and_the_last_takes_the_remainder() {
        let strip = Area {
            x: 5,
            y: 20,
            width: 10,
            height: 6,
        };
        let thirds: Vec<_> = (0..3).map(|k| split(strip, 3, k)).collect();
        assert_eq!(
            thirds.iter().map(|a| (a.x, a.width)).collect::<Vec<_>>(),
            vec![(5, 3), (8, 3), (11, 4)]
        );
        assert!(thirds.iter().all(|a| (a.y, a.height) == (20, 6)));
        assert_eq!(split(strip, 1, 0), strip);
        assert_eq!(split(strip, 0, 3), strip);
        assert_eq!(split_at(strip, 3, 5), 0);
        assert_eq!(split_at(strip, 3, 10), 1);
        assert_eq!(split_at(strip, 3, 14), 2);
        assert_eq!(split_at(strip, 1, 14), 0);
    }
}

#[cfg(test)]
mod viewport_tests {
    use super::viewport;

    // A 10-row list in a pane showing 4 of them.
    #[test]
    fn the_wheel_offset_is_kept_while_the_focused_row_is_visible() {
        assert_eq!(viewport(2, 3, 10, 4), 2);
    }

    #[test]
    fn a_focused_row_above_the_view_pulls_it_back() {
        assert_eq!(viewport(5, 1, 10, 4), 1);
    }

    #[test]
    fn a_focused_row_below_the_view_pushes_it_down() {
        assert_eq!(viewport(0, 7, 10, 4), 4);
    }

    #[test]
    fn scrolling_stops_at_the_last_screenful() {
        assert_eq!(viewport(99, 0, 10, 4), 0);
        assert_eq!(viewport(99, 9, 10, 4), 6);
    }

    #[test]
    fn content_shorter_than_the_pane_never_scrolls() {
        assert_eq!(viewport(99, 1, 3, 4), 0);
    }
}

#[cfg(test)]
mod frame_tests {
    use super::{frame, viewport};

    #[test]
    fn a_range_that_fits_keeps_a_little_context_above_it() {
        assert_eq!(frame(10, 14, 9), 8);
    }

    #[test]
    fn a_range_as_tall_as_the_pane_is_framed_from_its_top() {
        assert_eq!(frame(10, 18, 9), 10);
    }

    #[test]
    fn a_range_taller_than_the_pane_is_framed_from_its_top() {
        assert_eq!(frame(10, 40, 9), 10);
    }

    #[test]
    fn the_context_shrinks_rather_than_pushing_the_range_off_the_bottom() {
        assert_eq!(frame(10, 17, 9), 9);
    }

    #[test]
    fn a_range_at_the_top_never_frames_above_the_first_row() {
        assert_eq!(frame(0, 4, 9), 0);
        assert_eq!(frame(1, 5, 9), 0);
    }

    /// What the two functions have to agree on: `settle` re-clamps every
    /// offset, so a frame the clamp moved would be a frame nobody sees.
    #[test]
    fn the_clamp_leaves_a_framed_range_alone() {
        for (first, last) in [(10, 14), (10, 40), (0, 4), (10, 17)] {
            let offset = frame(first, last, 9);
            assert_eq!(viewport(offset, first, 60, 9), offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        chip_labels, inset, pane_at, panes, strip_at, strip_height, strip_width, AiPane, Area,
        Corner, Group, Output, Shapes, EDITOR_TITLE, STEP_MENU_WIDTH, STRIP_LEAST, TOP_LEAST,
    };

    /// Nine columns, the Breakpoint column leftmost and the fold toggle right
    /// of the four the number takes and the one a bar takes. `ui` draws and
    /// `mouse` hit-tests both columns from here, so moving one is a change
    /// on screen and fails here.
    #[test]
    fn the_gutter_is_nine_columns_with_breakpoints_leftmost() {
        use super::{gutter, Gutter, BREAKPOINT_COLUMN, TOGGLE_COLUMN};
        assert_eq!(gutter(Gutter::Numbers), 9);
        assert_eq!(BREAKPOINT_COLUMN, 0);
        assert_eq!(TOGGLE_COLUMN, 1 + 4 + 1);
    }
    use crate::Pane;

    /// Measured from ratatui's solver before the layout moved here. If these
    /// change, the panes moved on screen.
    #[test]
    fn matches_what_ratatui_produced() {
        let cases = [
            (120u16, 26u16, 30u16, (30u16, 54u16, 36u16), (18u16, 8u16)),
            (100, 26, 30, (30, 40, 30), (18, 8)),
            (80, 24, 45, (45, 20, 15), (17, 7)),
            (200, 50, 30, (30, 110, 60), (35, 15)),
        ];
        for (width, height, divider, (tree, editor, ai), (top, terminal)) in cases {
            let layout = panes(width, height, divider, None, 0, 0, Shapes::default());
            assert_eq!(
                (layout.tree.width, layout.editor.width, layout.ai.width),
                (tree, editor, ai),
                "widths for {width}x{height} divider {divider}"
            );
            assert_eq!(
                (layout.tree.height, layout.terminal.height),
                (top, terminal),
                "heights for {width}x{height}"
            );
        }
    }

    /// A dragged Strip height is the Strip's height and the top gets the rest,
    /// the bottom row still ending on the screen's.
    #[test]
    fn a_named_strip_height_is_kept_instead_of_the_share() {
        let shapes = Shapes {
            strip: Some(16),
            ..Shapes::default()
        };
        let layout = panes(120, 40, 30, None, 0, 0, shapes);
        assert_eq!((layout.terminal.y, layout.terminal.height), (24, 16));
        assert_eq!((layout.corner.y, layout.corner.height), (24, 16));
        assert_eq!(layout.tree.height, 24);
    }

    #[test]
    fn a_strip_height_keeps_both_the_strip_and_the_top() {
        assert_eq!(strip_height(40, 1), STRIP_LEAST);
        assert_eq!(strip_height(40, 40), 40 - TOP_LEAST);
        assert_eq!(strip_height(40, 16), 16);
        // Too short for both: the top keeps its rows, as the share always did.
        assert_eq!(strip_height(7, 1), 2);
    }

    /// A width the user dragged to is kept whatever the screen does — only the
    /// editor's 20-column floor overrides it.
    #[test]
    fn a_named_ai_width_is_kept_instead_of_the_share() {
        assert_eq!(
            panes(120, 26, 30, Some(40), 0, 0, Shapes::default())
                .ai
                .width,
            40
        );
        assert_eq!(
            panes(200, 50, 30, Some(40), 0, 0, Shapes::default())
                .ai
                .width,
            40
        );
        // 30 tree + 20 editor leaves 30, not the 60 asked for.
        assert_eq!(
            panes(80, 26, 30, Some(60), 0, 0, Shapes::default())
                .ai
                .width,
            30
        );
    }

    #[test]
    fn the_panes_tile_without_gaps_or_overlap() {
        let layout = panes(120, 26, 30, None, 0, 0, Shapes::default());
        assert_eq!(layout.tree.right(), layout.editor.x);
        assert_eq!(layout.editor.right(), layout.ai.x);
        assert_eq!(layout.ai.right(), 120);
        assert_eq!(layout.tree.bottom(), layout.terminal.y);
        assert_eq!(layout.terminal.bottom(), 26);
    }

    #[test]
    fn every_column_belongs_to_exactly_one_pane() {
        let layout = panes(120, 26, 30, None, 0, 0, Shapes::default());
        for column in 0..120 {
            for row in [0u16, 17, 18, 25] {
                assert!(
                    pane_at(&layout, column, row).is_some(),
                    "nothing at {column},{row}"
                );
            }
        }
    }

    #[test]
    fn hit_testing_finds_the_right_pane() {
        let layout = panes(120, 26, 30, None, 0, 0, Shapes::default());
        assert_eq!(pane_at(&layout, 5, 5), Some(Pane::Tree));
        assert_eq!(pane_at(&layout, 40, 5), Some(Pane::Editor));
        assert_eq!(pane_at(&layout, 100, 5), Some(Pane::Ai));
        assert_eq!(pane_at(&layout, 40, 20), Some(Pane::Terminal));
        assert_eq!(pane_at(&layout, 200, 5), None);
    }

    #[test]
    fn a_band_shortens_the_editor_and_sits_directly_under_it() {
        let full = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let banded = panes(120, 26, 30, None, 6, 0, Shapes::default());
        assert_eq!(banded.editor.height, full.editor.height - 6);
        assert_eq!(banded.editor.x, full.editor.x);
        assert_eq!(banded.editor.width, full.editor.width);
        assert_eq!(banded.band.x, banded.editor.x);
        assert_eq!(banded.band.width, banded.editor.width);
        assert_eq!(banded.band.y, banded.editor.bottom());
        assert_eq!(banded.band.height, 6);
        // Every other pane is untouched — the band is the only rectangle
        // that changes.
        assert_eq!(banded.tree, full.tree);
        assert_eq!(banded.ai, full.ai);
        assert_eq!(banded.terminal, full.terminal);
    }

    #[test]
    fn a_click_on_the_band_hit_tests_as_the_editor() {
        let layout = panes(120, 26, 30, None, 6, 0, Shapes::default());
        let (column, row) = (layout.band.x, layout.band.y);
        assert_eq!(pane_at(&layout, column, row), Some(Pane::Editor));
    }

    /// The step-menu narrows the editor by exactly its own width and sits
    /// directly to its left — pinned to the exact measured numbers, the same
    /// way `matches_what_ratatui_produced` pins the other panes, not just
    /// relationally.
    #[test]
    fn a_step_menu_narrows_the_editor_and_sits_directly_left_of_it() {
        let full = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let menued = panes(120, 26, 30, None, 0, STEP_MENU_WIDTH, Shapes::default());
        assert_eq!((menued.step_menu.x, menued.step_menu.y), (30, 0));
        assert_eq!((menued.step_menu.width, menued.step_menu.height), (20, 18));
        assert_eq!((menued.editor.x, menued.editor.width), (50, 34));
        assert_eq!(menued.editor.height, full.editor.height);
        assert_eq!(menued.editor.width, full.editor.width - STEP_MENU_WIDTH);
        assert_eq!(menued.editor.x, menued.step_menu.right());
        // The step-menu spans the tree's full height, not just the editor's:
        // the band sits under the editor's own column, never the
        // step-menu's, so a shorter step-menu would leave the rows beside
        // the band belonging to no pane.
        assert_eq!(menued.step_menu.height, menued.tree.height);
        // Every other pane is untouched — the step-menu is the only new
        // rectangle, taken out of the editor's width alone.
        assert_eq!(menued.tree, full.tree);
        assert_eq!(menued.ai, full.ai);
        assert_eq!(menued.terminal, full.terminal);
    }

    /// The step-menu is drawn, never hit-tested: the ticket asks for no new
    /// mouse region and no click-to-jump, so a click over it resolves to no
    /// pane at all rather than quietly falling into the editor's.
    #[test]
    fn a_click_on_the_step_menu_hits_no_pane() {
        let layout = panes(120, 26, 30, None, 0, STEP_MENU_WIDTH, Shapes::default());
        let (column, row) = (layout.step_menu.x, layout.step_menu.y);
        assert_eq!(pane_at(&layout, column, row), None);
    }

    /// An extreme AI-pane width squeezes the step-menu to nothing before it
    /// ever touches the editor's own 20-column floor — the menu is
    /// secondary furniture, the editor is not.
    #[test]
    fn an_extreme_ai_width_empties_the_step_menu_before_the_editor_floor() {
        let squeezed = panes(120, 26, 30, Some(70), 0, STEP_MENU_WIDTH, Shapes::default());
        assert_eq!(squeezed.step_menu.width, 0);
        assert_eq!(squeezed.editor.width, 20);
    }

    /// The whole point of the tall shape: the AI pane gains the terminal's
    /// rows and the terminal gives up its columns. Nothing else moves — the
    /// tree, the editor and the AI pane's own left edge are where they were,
    /// so switching shapes never reflows the buffer beside it.
    #[test]
    fn a_tall_ai_pane_takes_the_height_and_the_terminal_gives_up_the_width() {
        let beside = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let tall = panes(
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
        assert_eq!(tall.ai.height, 26);
        assert_eq!(tall.terminal.width, 120 - tall.ai.width);
        assert_eq!(tall.terminal.right(), tall.ai.x);
        assert_eq!((tall.ai.x, tall.ai.width), (beside.ai.x, beside.ai.width));
        assert_eq!(tall.tree, beside.tree);
        assert_eq!(tall.editor, beside.editor);
        assert_eq!(tall.terminal.height, beside.terminal.height);
    }

    #[test]
    fn a_tall_pane_still_tiles_without_gaps_or_overlap() {
        let layout = panes(
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
        for column in 0..120 {
            for row in 0..26 {
                assert!(
                    pane_at(&layout, column, row).is_some(),
                    "nothing at {column},{row}"
                );
            }
        }
        // The rows the terminal used to own beside the AI pane are the AI
        // pane's now, and the ones in front of it are still the terminal's.
        assert_eq!(pane_at(&layout, 100, 20), Some(Pane::Ai));
        assert_eq!(pane_at(&layout, 40, 20), Some(Pane::Terminal));
    }

    /// A dragged width is the width in either shape — going tall must not
    /// quietly hand the pane the share it had already been dragged off.
    #[test]
    fn a_tall_pane_keeps_a_named_width() {
        assert_eq!(
            panes(
                120,
                26,
                30,
                Some(40),
                0,
                0,
                Shapes {
                    ai: AiPane::Tall,
                    ..Shapes::default()
                }
            )
            .ai
            .width,
            40
        );
    }

    /// The whole point of the Risk list's shape: it sits beneath the tree at
    /// the tree's width, the shell gives up exactly those columns and starts
    /// where the editor starts, and nothing above it moves. Pinned to the
    /// measured numbers, the way the other panes are.
    #[test]
    fn the_risk_list_sits_under_the_tree_and_the_shell_gives_up_its_columns() {
        let hidden = panes(120, 26, 30, None, 0, 0, Shapes::default());
        let shown = panes(
            120,
            26,
            30,
            None,
            0,
            0,
            Shapes {
                corner: Corner::Risk,
                ..Shapes::default()
            },
        );
        assert_eq!(
            (hidden.corner.width, hidden.corner.height),
            (0, 8),
            "a hidden pane is zero-sized"
        );
        assert_eq!(
            (hidden.terminal.x, hidden.terminal.width),
            (0, 120),
            "and the shell has the full width"
        );
        assert_eq!(
            (
                shown.corner.x,
                shown.corner.y,
                shown.corner.width,
                shown.corner.height
            ),
            (0, 18, 30, 8)
        );
        assert_eq!((shown.terminal.x, shown.terminal.width), (30, 90));
        assert_eq!(shown.terminal.x, shown.corner.right());
        assert_eq!(shown.corner.width, shown.tree.width);
        assert_eq!(shown.corner.y, shown.tree.bottom());
        // The shell starts where the editor starts, which is what makes the
        // left-hand column read as one column.
        assert_eq!(shown.terminal.x, shown.editor.x);
        // Except while a Story is being walked: the step-menu moves the
        // editor right, and the Risk list stays at the tree's width rather
        // than following it — the pane is the tree's column, and the shell
        // still starts at the Risk list's own right edge.
        let menued = panes(
            120,
            26,
            30,
            None,
            0,
            STEP_MENU_WIDTH,
            Shapes {
                corner: Corner::Risk,
                ..Shapes::default()
            },
        );
        assert_eq!(menued.corner.width, menued.tree.width);
        assert_eq!(menued.terminal.x, menued.corner.right());
        assert_eq!(menued.editor.x, menued.tree.right() + STEP_MENU_WIDTH);
        // Nothing else moves either way: toggling the pane must not reflow the
        // buffer or the child beside it.
        assert_eq!(shown.tree, hidden.tree);
        assert_eq!(shown.editor, hidden.editor);
        assert_eq!(shown.ai, hidden.ai);
        assert_eq!(shown.terminal.y, hidden.terminal.y);
        assert_eq!(shown.terminal.height, hidden.terminal.height);
    }

    /// Both shapes at once: the AI pane takes the shell's columns from the
    /// right and the Risk list from the left, and the shell keeps what is
    /// between them.
    #[test]
    fn a_tall_ai_pane_and_the_risk_list_take_the_shell_from_both_sides() {
        let layout = panes(
            120,
            26,
            30,
            None,
            0,
            0,
            Shapes {
                group: Group::Shells,
                ai: AiPane::Tall,
                corner: Corner::Risk,
                strip: None,
                output: Output::Away,
                evaluator: None,
            },
        );
        assert_eq!((layout.corner.x, layout.corner.width), (0, 30));
        assert_eq!((layout.terminal.x, layout.terminal.width), (30, 54));
        assert_eq!(layout.terminal.right(), layout.ai.x);
        assert_eq!(layout.ai.height, 26);
    }

    /// The floor: a tree as wide as the screen would leave the shell no
    /// columns at all, and a pty of zero columns is one vt100 panics on. The
    /// Risk list gives way rather than the shell disappearing — and it is the
    /// tree's width that can reach this, never the tall AI pane, whose own
    /// width is already capped by the editor's 20-column floor.
    #[test]
    fn the_shell_keeps_a_column_when_the_risk_list_asks_for_everything() {
        for ai in [AiPane::Beside, AiPane::Tall] {
            let layout = panes(
                30,
                26,
                30,
                None,
                0,
                0,
                Shapes {
                    group: Group::Shells,
                    ai,
                    corner: Corner::Risk,
                    strip: None,
                    output: Output::Away,
                    evaluator: None,
                },
            );
            assert_eq!(layout.terminal.width, 1, "{ai:?}");
            assert_eq!(layout.corner.width, 29);
            assert_eq!(layout.terminal.x, layout.corner.right());
        }
    }

    #[test]
    fn the_risk_list_is_hit_tested_as_itself_and_never_as_the_tree_or_the_shell() {
        let layout = panes(
            120,
            26,
            30,
            None,
            0,
            0,
            Shapes {
                corner: Corner::Risk,
                ..Shapes::default()
            },
        );
        assert_eq!(pane_at(&layout, 5, 20), Some(Pane::Risk));
        assert_eq!(pane_at(&layout, 5, 5), Some(Pane::Tree));
        assert_eq!(pane_at(&layout, 40, 20), Some(Pane::Terminal));
        // The slot's other occupant: the same rectangle, answered as itself.
        // Which pane is in the corner changes what a click means and never
        // where the corner is, so a click that landed on a Risk row must land
        // on a buffer row at the same cell.
        let buffers = panes(
            120,
            26,
            30,
            None,
            0,
            0,
            Shapes {
                corner: Corner::Buffers,
                ..Shapes::default()
            },
        );
        assert_eq!(buffers.corner, layout.corner);
        assert_eq!(buffers.terminal, layout.terminal);
        assert_eq!(pane_at(&buffers, 5, 20), Some(Pane::Buffers));
        // And the third, which is what "every occupant is the same rectangle"
        // has to keep meaning as the slot grows: a third pane that took a
        // rectangle of its own would be a third set of columns for the shell to
        // lose and a third hit-test to get one row off.
        let history = panes(
            120,
            26,
            30,
            None,
            0,
            0,
            Shapes {
                corner: Corner::History,
                ..Shapes::default()
            },
        );
        assert_eq!(history.corner, layout.corner);
        assert_eq!(history.terminal, layout.terminal);
        assert_eq!(pane_at(&history, 5, 20), Some(Pane::History));
        // Hidden, the same column belongs to the shell again — there is no
        // rectangle left over to swallow a click.
        let hidden = panes(120, 26, 30, None, 0, 0, Shapes::default());
        assert_eq!(pane_at(&hidden, 5, 20), Some(Pane::Terminal));
        for column in 0..120 {
            for row in 0..26 {
                assert!(
                    pane_at(&layout, column, row).is_some(),
                    "nothing at {column},{row}"
                );
            }
        }
    }

    #[test]
    fn an_inset_box_leaves_the_edges_showing() {
        let box_area = inset(120, 26, 8, 3);
        assert_eq!(
            (box_area.x, box_area.y, box_area.width, box_area.height),
            (8, 3, 104, 20)
        );
    }

    /// The rectangle the results box is drawn in and the rows of results it
    /// leaves room for, pinned together: the clamp counts rows the renderer
    /// draws, so a change to either is a change on screen and fails here.
    #[test]
    fn the_results_box_leaves_room_for_its_query_and_its_borders() {
        let box_area = super::search_box(100, 16);
        assert_eq!((box_area.y, box_area.height), (3, 10));
        // Ten rows: two borders and three of query above the list.
        assert_eq!(super::search_hit_rows(100, 16), 5);
        // A terminal too short for a list still asks for no negative rows.
        assert_eq!(super::search_hit_rows(20, 4), 0);
    }

    /// Inside the editor's border at its top-right, and never wider or taller
    /// than the pane has room for.
    #[test]
    fn the_replace_box_sits_inside_the_editors_top_right_corner() {
        let editor = Area {
            x: 30,
            y: 0,
            width: 60,
            height: 20,
        };
        let spot = super::replace_box(editor);
        assert_eq!((spot.x, spot.y, spot.width, spot.height), (45, 1, 44, 5));
        let narrow = Area {
            width: 20,
            height: 4,
            ..editor
        };
        let spot = super::replace_box(narrow);
        assert_eq!((spot.x, spot.y, spot.width, spot.height), (31, 1, 18, 2));
    }

    /// `[Aa]` ends a column short of the right border, and the buttons start a
    /// column in from the left one with a column between them.
    #[test]
    fn the_replace_boxs_toggle_and_buttons_have_one_place_each() {
        let spot = Area {
            x: 45,
            y: 1,
            width: 44,
            height: 5,
        };
        let case = super::replace_case(spot);
        assert_eq!((case.x, case.y, case.width), (83, 2, 4));
        let buttons: Vec<(u16, u16, u16)> = super::replace_buttons(spot)
            .iter()
            .map(|(_, at)| (at.x, at.y, at.width))
            .collect();
        assert_eq!(buttons, vec![(47, 4, 9), (57, 4, 13)]);
    }

    #[test]
    fn a_small_terminal_gives_up_the_margin_before_the_box() {
        // 44 columns leaves room for 2 either side, not 8.
        let box_area = inset(44, 12, 8, 3);
        assert_eq!((box_area.x, box_area.width), (2, 40));
        assert_eq!((box_area.y, box_area.height), (1, 10));
        // And below the minimum there is simply no margin.
        let tiny = inset(20, 6, 8, 3);
        assert_eq!((tiny.x, tiny.width, tiny.y, tiny.height), (0, 20, 0, 6));
    }

    /// A narrow terminal must not panic or produce a negative-width pane.
    #[test]
    fn absurd_sizes_stay_sane() {
        for (width, height) in [(1u16, 1u16), (10, 3), (40, 6), (0, 0)] {
            for shapes in [
                (Shapes::default()),
                (Shapes {
                    corner: Corner::Risk,
                    ..Shapes::default()
                }),
                (Shapes {
                    ai: AiPane::Tall,
                    ..Shapes::default()
                }),
                (Shapes {
                    group: Group::Shells,
                    ai: AiPane::Tall,
                    corner: Corner::Risk,
                    strip: None,
                    output: Output::Away,
                    evaluator: None,
                }),
            ] {
                let layout = panes(width, height, 30, None, 6, 0, shapes);
                assert!(layout.ai.right() <= width.max(1) || width == 0);
                assert_eq!(layout.terminal.bottom(), height);
            }
        }
    }

    /// The strip's columns, which is the whole of what `ui` leaves room for
    /// and `mouse` hit-tests. Measured, never counted: `«` is one column and
    /// `1.25x` is five, and a count would read the same for a glyph a font
    /// draws twice as wide.
    #[test]
    fn a_right_aligned_strip_is_hit_tested_where_it_is_drawn() {
        let area = Area {
            x: 0,
            y: 0,
            width: 30,
            height: 4,
        };
        let labels: Vec<String> = ["\u{ab}", "\u{25b8}", "1.25x"]
            .iter()
            .map(|label| label.to_string())
            .collect();
        // One space after each, the last landing on the column before the
        // corner: 1 + 1 + 5, plus three spaces.
        assert_eq!(strip_width(&labels), 10);
        assert_eq!(strip_at(area, &labels, 19), Some(0));
        assert_eq!(strip_at(area, &labels, 21), Some(1));
        // The whole of the speed, and neither of the spaces beside it.
        assert_eq!(strip_at(area, &labels, 22), None);
        assert_eq!(strip_at(area, &labels, 23), Some(2));
        assert_eq!(strip_at(area, &labels, 27), Some(2));
        assert_eq!(strip_at(area, &labels, 28), None);
        // The corner is not a control, and neither is anything left of the
        // strip.
        assert_eq!(strip_at(area, &labels, 29), None);
        assert_eq!(strip_at(area, &labels, 18), None);
        // A pane narrower than its own strip has no columns to offer rather
        // than wrapping the strip onto columns nobody pointed at.
        assert_eq!(strip_at(Area { width: 4, ..area }, &labels, 2), None);
    }

    fn chip(glyph: &str, keys: &'static str) -> crate::Chip {
        crate::Chip {
            action: "a",
            name: "a",
            glyph: glyph.to_string(),
            keys,
            hue: crate::Hue::Plain,
            tone: crate::Tone::Plain,
        }
    }

    /// Short of room every Chip sheds its keys at once: one column short of
    /// the whole strip is a row of bare glyphs, never one Chip with its keys
    /// beside one without, and never a Chip cut to fit.
    #[test]
    fn short_of_room_every_chip_sheds_its_keys_together() {
        let chips = [chip("\u{25ba}", ":pause"), chip("1.25x", ":speed")];
        // " ► :pause " is 10 and " 1.25x :speed " is 14, each with its gap,
        // beside the 34 columns the title keeps.
        let whole = chip_labels(&chips, 60, EDITOR_TITLE);
        assert_eq!(whole, [" \u{25ba} :pause ", " 1.25x :speed "]);
        assert_eq!(strip_width(&whole), 26);
        let shed = chip_labels(&chips, 59, EDITOR_TITLE);
        assert_eq!(shed, [" \u{25ba} ", " 1.25x "]);
        assert_eq!(strip_width(&shed), 12);
    }
}
