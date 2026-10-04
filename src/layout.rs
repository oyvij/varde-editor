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

pub fn strip_width(labels: &[String]) -> u16 {
    labels
        .iter()
        .map(|label| UnicodeWidthStr::width(label.as_str()) as u16 + 1)
        .sum()
}

pub fn strip_at(area: Area, labels: &[String], column: u16) -> Option<usize> {
    let mut at = (area.x + area.width.saturating_sub(1)).checked_sub(strip_width(labels))?;
    labels.iter().position(|label| {
        let width = UnicodeWidthStr::width(label.as_str()) as u16;
        let hit = column >= at && column < at + width;
        at += width + 1;
        hit
    })
}

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

pub const EDITOR_TITLE: u16 = 34;

pub const CORNER_TITLE: u16 = 14;

pub const GUTTER: u16 = 10;

pub const BREAKPOINT_COLUMN: u16 = 0;

pub const TOGGLE_COLUMN: u16 = 7;

pub const STEP_MENU_WIDTH: u16 = 20;

pub const MARKER: u16 = 2;

pub enum Gutter {
    Numbers,
    NumbersAndMarker,
    None,
}

pub fn gutter(shows: Gutter) -> u16 {
    match shows {
        Gutter::Numbers => GUTTER,
        Gutter::NumbersAndMarker => GUTTER + MARKER,
        Gutter::None => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiPane {
    #[default]
    Beside,
    Tall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Corner {
    #[default]
    Hidden,
    Risk,
    Buffers,
    History,
    Breakpoints,
    Frames,
    Diagnostics(crate::lsp::Severity),
    Conflicts,
}

impl Corner {
    pub fn pane(self) -> Option<Pane> {
        match self {
            Corner::Hidden => None,
            Corner::Risk => Some(Pane::Risk),
            Corner::Buffers => Some(Pane::Buffers),
            Corner::History => Some(Pane::History),
            Corner::Breakpoints => Some(Pane::Breakpoints),
            Corner::Frames => Some(Pane::Frames),
            Corner::Diagnostics(_) => Some(Pane::Diagnostics),
            Corner::Conflicts => Some(Pane::Conflicts),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Slot {
    #[default]
    Ai,
    Cheatsheet,
}

impl Slot {
    pub fn pane(self) -> Pane {
        match self {
            Slot::Ai => Pane::Ai,
            Slot::Cheatsheet => Pane::Cheatsheet,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Group {
    #[default]
    Shells,
    Debug,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Shells => "Shells",
            Group::Debug => "Debug",
        }
    }

    pub fn pane(self) -> Pane {
        match self {
            Group::Shells => Pane::Terminal,
            Group::Debug => Pane::Variables,
        }
    }

    pub fn holds(self, pane: Pane) -> bool {
        match self {
            Group::Shells => pane == Pane::Terminal,
            Group::Debug => matches!(pane, Pane::Variables | Pane::Output),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Output {
    #[default]
    Away,
    Shown(Option<u16>),
}

pub const GROUP_LEAST: u16 = 8;

pub const TOP_LEAST: u16 = 5;
pub const STRIP_LEAST: u16 = 4;

pub fn strip_height(screen_height: u16, asked: u16) -> u16 {
    asked
        .max(STRIP_LEAST)
        .min(screen_height.saturating_sub(TOP_LEAST))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shapes {
    pub ai: AiPane,
    pub slot: Slot,
    pub corner: Corner,
    pub group: Group,
    pub strip: Option<u16>,
    pub output: Output,
    pub evaluator: Option<Area>,
}

pub const WINDOW_LEAST_WIDTH: u16 = 24;
pub const WINDOW_LEAST_HEIGHT: u16 = 5;

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

const CONTEXT: usize = 2;

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
    pub band: Area,
    pub step_menu: Area,
    pub corner: Area,
    pub occupant: Corner,
    pub slot: Slot,
    pub group: Group,
    pub output: Area,
    pub evaluator: Area,
}

impl Layout {
    pub fn strip(&self) -> Area {
        Area {
            width: self.terminal.width + self.output.width,
            ..self.terminal
        }
    }
}

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
    // Floor of one column: vt100 panics on a zero-column pty
    let corner_width = match shapes.corner {
        Corner::Hidden => 0,
        _ => tree_width.min(shell_room.saturating_sub(1)),
    };
    let strip_width = shell_room.saturating_sub(corner_width).max(1);
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
        slot: shapes.slot,
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

/// Two rows minimum: vt100 underflows when wrapping on a one-row grid
pub fn pty_size(width: u16, height: u16) -> Option<(u16, u16)> {
    (width > 0 && height > 0).then(|| {
        (
            height.saturating_sub(2).max(2),
            width.saturating_sub(2).max(1),
        )
    })
}

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

pub const SEARCH_HEADER: u16 = 3;

pub fn search_box(width: u16, height: u16) -> Area {
    inset(width, height, 8, 3)
}

pub fn replace_box(editor: Area) -> Area {
    let width = editor.width.saturating_sub(2).min(44);
    Area {
        x: (editor.x + editor.width).saturating_sub(1 + width),
        y: editor.y + 1,
        width,
        height: editor.height.saturating_sub(2).min(5),
    }
}

pub fn command_list(editor: Area, rows: usize, screen_height: u16, widest: u16) -> Area {
    let line = editor.bottom().saturating_sub(1);
    let below = screen_height.saturating_sub(editor.bottom());
    let wanted = (rows as u16).saturating_add(2);
    let (y, height) = match wanted <= below {
        true => (editor.bottom(), wanted),
        false => {
            let height = wanted.min(line.saturating_sub(editor.y));
            (line.saturating_sub(height), height)
        }
    };
    Area {
        x: editor.x,
        y,
        width: widest.saturating_add(5).min(editor.width),
        height,
    }
}

pub fn replace_toggles(spot: Area) -> Vec<(crate::FindIcon, &'static str, Area)> {
    let mut right = spot.right().saturating_sub(2);
    let mut toggles: Vec<_> = crate::FIND_ICONS
        .iter()
        .filter(|(icon, _)| matches!(icon, crate::FindIcon::Case | crate::FindIcon::Word))
        .map(|(icon, label)| {
            let width = label.len() as u16;
            right = right.saturating_sub(width);
            let at = Area {
                x: right,
                y: spot.y + 1,
                width,
                height: 1,
            };
            right = right.saturating_sub(1);
            (*icon, *label, at)
        })
        .collect();
    toggles.reverse();
    toggles
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

pub fn split_at(strip: Area, n: usize, column: u16) -> usize {
    (0..n.max(1))
        .rev()
        .find(|&k| split(strip, n, k).x <= column)
        .unwrap_or(0)
}

pub fn pane_at(layout: &Layout, column: u16, row: u16) -> Option<Pane> {
    if layout.evaluator.holds(column, row) {
        return Some(Pane::Evaluator);
    }
    if layout.corner.holds(column, row) {
        layout.occupant.pane()
    } else if layout.output.holds(column, row) {
        Some(Pane::Output)
    } else if layout.tree.holds(column, row) {
        Some(Pane::Tree)
    } else if layout.editor.holds(column, row) || layout.band.holds(column, row) {
        Some(Pane::Editor)
    } else if layout.ai.holds(column, row) {
        Some(layout.slot.pane())
    } else if layout.terminal.holds(column, row) {
        Some(layout.group.pane())
    } else {
        None
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

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

        assert_eq!(group(Output::Shown(Some(120))).0.width, GROUP_LEAST);
        assert_eq!(group(Output::Shown(Some(0))).1.width, GROUP_LEAST);
    }

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
        assert_eq!(open.x, 120 - open.right());
        assert_eq!(open.y, 40 - open.bottom());

        let (snippet, output) = super::evaluator_split(open, None);
        assert_eq!((snippet.x, snippet.y, snippet.height), (25, 11, 8));
        assert_eq!((output.x, output.y, output.height), (25, 20, 9));
        assert_eq!(snippet.bottom() + 1, output.y);
        assert_eq!(output.bottom(), open.bottom() - 1);
        assert_eq!((snippet.width, output.width), (70, 70));

        let (snippet, output) = super::evaluator_split(open, Some(4));
        assert_eq!((snippet.height, output.height), (4, 13));
        assert_eq!(snippet.bottom() + 1, output.y);
        assert_eq!(super::evaluator_split(open, Some(0)).0.height, 1);
        assert_eq!(super::evaluator_split(open, Some(99)).1.height, 1);
    }

    #[test]
    fn a_window_is_placed_on_the_screen_and_clear_of_the_paused_line() {
        let window = Area {
            x: 20,
            y: 5,
            width: 60,
            height: 12,
        };
        assert_eq!(placed_window(window, 120, 40, None), window);
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
        assert_eq!(placed_window(window, 120, 40, Some(8)).y, 9);
        assert_eq!(placed_window(window, 120, 20, Some(14)).y, 2);
        assert_eq!(placed_window(window, 120, 12, Some(6)).y, 0);
        assert_eq!(placed_window(window, 120, 40, Some(30)), window);
    }

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
        Corner, Group, Output, Shapes, Slot, EDITOR_TITLE, STEP_MENU_WIDTH, STRIP_LEAST, TOP_LEAST,
    };

    #[test]
    fn the_gutter_is_ten_columns_with_breakpoints_leftmost() {
        use super::{gutter, Gutter, BREAKPOINT_COLUMN, TOGGLE_COLUMN};
        assert_eq!(gutter(Gutter::Numbers), 10);
        assert_eq!(BREAKPOINT_COLUMN, 0);
        assert_eq!(TOGGLE_COLUMN, 1 + 1 + 4 + 1);
    }
    use crate::Pane;

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
        assert_eq!(strip_height(7, 1), 2);
    }

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
        assert_eq!(menued.step_menu.height, menued.tree.height);
        assert_eq!(menued.tree, full.tree);
        assert_eq!(menued.ai, full.ai);
        assert_eq!(menued.terminal, full.terminal);
    }

    #[test]
    fn a_click_on_the_step_menu_hits_no_pane() {
        let layout = panes(120, 26, 30, None, 0, STEP_MENU_WIDTH, Shapes::default());
        let (column, row) = (layout.step_menu.x, layout.step_menu.y);
        assert_eq!(pane_at(&layout, column, row), None);
    }

    #[test]
    fn an_extreme_ai_width_empties_the_step_menu_before_the_editor_floor() {
        let squeezed = panes(120, 26, 30, Some(70), 0, STEP_MENU_WIDTH, Shapes::default());
        assert_eq!(squeezed.step_menu.width, 0);
        assert_eq!(squeezed.editor.width, 20);
    }

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
    fn the_cheatsheet_takes_the_ai_panes_rectangle_beside_and_tall() {
        for (ai, at) in [(AiPane::Beside, (100, 5)), (AiPane::Tall, (100, 20))] {
            let shapes = Shapes {
                ai,
                ..Shapes::default()
            };
            let session = panes(120, 26, 30, None, 0, 0, shapes);
            let cheatsheet = panes(
                120,
                26,
                30,
                None,
                0,
                0,
                Shapes {
                    slot: Slot::Cheatsheet,
                    ..shapes
                },
            );
            let expected = match ai {
                AiPane::Beside => Area {
                    x: 84,
                    y: 0,
                    width: 36,
                    height: 18,
                },
                AiPane::Tall => Area {
                    x: 84,
                    y: 0,
                    width: 36,
                    height: 26,
                },
            };
            assert_eq!(cheatsheet.ai, expected);
            assert_eq!(cheatsheet.ai, session.ai);
            assert_eq!(pane_at(&cheatsheet, at.0, at.1), Some(Pane::Cheatsheet));
            assert_eq!(pane_at(&session, at.0, at.1), Some(Pane::Ai));
        }
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
        assert_eq!(pane_at(&layout, 100, 20), Some(Pane::Ai));
        assert_eq!(pane_at(&layout, 40, 20), Some(Pane::Terminal));
    }

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
        assert_eq!(shown.terminal.x, shown.editor.x);
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
        assert_eq!(shown.tree, hidden.tree);
        assert_eq!(shown.editor, hidden.editor);
        assert_eq!(shown.ai, hidden.ai);
        assert_eq!(shown.terminal.y, hidden.terminal.y);
        assert_eq!(shown.terminal.height, hidden.terminal.height);
    }

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
                slot: Slot::Ai,
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
                    slot: Slot::Ai,
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

    #[test]
    fn the_results_box_leaves_room_for_its_query_and_its_borders() {
        let box_area = super::search_box(100, 16);
        assert_eq!((box_area.y, box_area.height), (3, 10));
        assert_eq!(super::search_hit_rows(100, 16), 5);
        assert_eq!(super::search_hit_rows(20, 4), 0);
    }

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

    #[test]
    fn the_command_list_sits_under_the_command_line_when_the_screen_has_room() {
        let editor = Area {
            x: 30,
            y: 0,
            width: 54,
            height: 18,
        };
        let spot = super::command_list(editor, 6, 26, 24);
        assert_eq!((spot.x, spot.y, spot.width, spot.height), (30, 18, 29, 8));
    }

    #[test]
    fn the_command_list_sits_above_the_command_line_when_the_screen_does_not() {
        let editor = Area {
            x: 30,
            y: 0,
            width: 54,
            height: 18,
        };
        let spot = super::command_list(editor, 12, 18, 24);
        assert_eq!((spot.x, spot.y, spot.width, spot.height), (30, 3, 29, 14));
    }

    #[test]
    fn the_command_list_is_clamped_to_the_editor() {
        let editor = Area {
            x: 30,
            y: 4,
            width: 20,
            height: 8,
        };
        let spot = super::command_list(editor, 30, 12, 40);
        assert_eq!((spot.x, spot.y, spot.width, spot.height), (30, 4, 20, 7));
    }

    #[test]
    fn the_replace_boxs_toggle_and_buttons_have_one_place_each() {
        let spot = Area {
            x: 45,
            y: 1,
            width: 44,
            height: 5,
        };
        let toggles: Vec<(crate::FindIcon, u16, u16, u16)> = super::replace_toggles(spot)
            .iter()
            .map(|(icon, _, at)| (*icon, at.x, at.y, at.width))
            .collect();
        assert_eq!(
            toggles,
            vec![
                (crate::FindIcon::Word, 76, 2, 6),
                (crate::FindIcon::Case, 83, 2, 4)
            ]
        );
        let buttons: Vec<(u16, u16, u16)> = super::replace_buttons(spot)
            .iter()
            .map(|(_, at)| (at.x, at.y, at.width))
            .collect();
        assert_eq!(buttons, vec![(47, 4, 9), (57, 4, 13)]);
    }

    #[test]
    fn a_small_terminal_gives_up_the_margin_before_the_box() {
        let box_area = inset(44, 12, 8, 3);
        assert_eq!((box_area.x, box_area.width), (2, 40));
        assert_eq!((box_area.y, box_area.height), (1, 10));
        let tiny = inset(20, 6, 8, 3);
        assert_eq!((tiny.x, tiny.width, tiny.y, tiny.height), (0, 20, 0, 6));
    }

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
                    slot: Slot::Ai,
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
        assert_eq!(strip_width(&labels), 10);
        assert_eq!(strip_at(area, &labels, 19), Some(0));
        assert_eq!(strip_at(area, &labels, 21), Some(1));
        assert_eq!(strip_at(area, &labels, 22), None);
        assert_eq!(strip_at(area, &labels, 23), Some(2));
        assert_eq!(strip_at(area, &labels, 27), Some(2));
        assert_eq!(strip_at(area, &labels, 28), None);
        assert_eq!(strip_at(area, &labels, 29), None);
        assert_eq!(strip_at(area, &labels, 18), None);
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

    #[test]
    fn short_of_room_every_chip_sheds_its_keys_together() {
        let chips = [chip("\u{25ba}", ":pause"), chip("1.25x", ":speed")];
        let whole = chip_labels(&chips, 60, EDITOR_TITLE);
        assert_eq!(whole, [" \u{25ba} :pause ", " 1.25x :speed "]);
        assert_eq!(strip_width(&whole), 26);
        let shed = chip_labels(&chips, 59, EDITOR_TITLE);
        assert_eq!(shed, [" \u{25ba} ", " 1.25x "]);
        assert_eq!(strip_width(&shed), 12);
    }
}
