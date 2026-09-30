//! The minimap — a far-off mirror of the file down the editor's right edge.
//!
//! A terminal cannot draw smaller than a cell, so the mirror folds [`COLUMNS`]
//! columns and [`LINES`] lines into each one and draws a **dot** there.
//!
//! The dot is the whole of why it reads as a miniature rather than as a bar
//! chart. Filling the cell was the first try — a half-block per line, each in
//! its own colour — and it came out a wall of slabs: code has no gaps four
//! columns wide, so every mark touched its neighbours and took the shape of the
//! file with it. Quadrant blocks were the second try, at twice the resolution,
//! and failed the same way for the same reason. What carries the shape at this
//! size is the *air*, and the air has to come from the glyph rather than from
//! the text: a dot brings its own padding, so a dense run of code is a run of
//! dots and the indentation, the blank lines and where each line ends are all
//! still there.
//!
//! Two dots rather than one, both of them ancient: a bullet where both lines
//! hold ink and a middle dot where one does, which is the only density a cell
//! has room to say. Braille is finer and was rejected for being a *font's* to
//! draw — a machine whose monospace font has none would draw the mirror as a
//! column of tofu, and this has to work in every terminal. These two are in
//! every font there is, and the editor already draws the middle dot for a
//! space.
//!
//! Past `(WIDTH - 2) * COLUMNS` characters — the strip less the slider's
//! column and the mark lane's — a line is not mirrored at all — the mirror
//! is for the shape of a file, and a strip wide enough for a long line is a
//! strip taking the columns the line is read in.
//!
//! Everything here is arithmetic on the state the editor already keeps:
//! `editor_scroll` is the window, and where the strip has got to is derived
//! from it rather than stored beside it. A second scroll field would be a
//! second author for one question, which is how a mirror comes to disagree
//! with what it mirrors.

use crate::highlight::{Kind, Token};
use crate::layout::Area;
use crate::State;

/// How many of the editor pane's columns the strip takes, the slider's first
/// column and the mark lane's last among them — so this is the whole cost.
const WIDTH: u16 = 12;

/// How many source columns one cell of the strip stands for.
const COLUMNS: usize = 4;

/// How many source lines one row of the strip stands for.
const LINES: usize = 2;

/// Is the editor drawing a file's own lines? Only then is there anything to
/// mirror: a diff, a Preview's reflowed rows and a walked Site are surfaces
/// whose rows are not the file's lines, and a mirror of lines shown against
/// rows is the second meaning for one shape the rest of this crate refuses.
///
/// Apart from [`showing`] because the scrollbar is not the strip: a thumb over
/// the text is what says how far through a file the window is when the mirror
/// has been turned off.
pub fn mirroring(state: &State) -> bool {
    state.diff.is_none()
        && state.walking.is_none()
        && !crate::previewing(state)
        && crate::current_buffer(state).is_some()
}

/// How many columns of text the pane must be left with before the mirror is
/// worth its own. A mirror is for reading the shape of a file you are also
/// reading, and on a narrow pane it is neither: a stock tree divider on a
/// hundred-column terminal leaves the editor forty columns, and twelve of them
/// spent on the mirror left the line numbers and the mirror with no code
/// between them.
const ROOM: u16 = 20;

/// Is the mirror on screen? Asked for, over a file's own lines, and with room
/// for both.
fn showing(state: &State) -> bool {
    state.minimap
        && mirroring(state)
        && crate::panes_of(state)
            .editor
            .width
            .saturating_sub(2 + crate::gutter(state) + WIDTH)
            >= ROOM
}

/// How many columns the editor gives up to it, which is none while it is
/// hidden. Read by the renderer, by the scroll clamp's column count and by the
/// hit-test, for the reason `layout::GUTTER` is read by all three.
pub fn width(state: &State) -> u16 {
    match showing(state) {
        true => WIDTH,
        false => 0,
    }
}

/// Where the strip is, inside the editor pane's borders. Here rather than in
/// `ui` and again in `mouse` for the reason `layout::GUTTER` is: the renderer
/// draws it and the hit-test measures against it, and two derivations of one
/// rectangle is a press landing in the text beside the thing it pointed at.
///
/// Zero-width while the mirror is hidden, which `Area::holds` answers `false`
/// for — so no caller has to ask whether it is showing before hit-testing it.
pub fn strip(state: &State, editor: Area) -> Area {
    let width = width(state);
    Area {
        x: editor.right().saturating_sub(1 + width),
        y: editor.y + 1,
        width,
        height: editor.height.saturating_sub(2),
    }
}

/// How many lines the surface under the mirror holds. The same count the
/// scroll clamp bounds `editor_scroll` against, so the two cannot disagree
/// about where the file ends.
pub fn lines(state: &State) -> usize {
    crate::current_buffer(state).map_or(0, |buffer| buffer.shown().lines().count())
}

/// The first line the strip mirrors, 0-based: proportional to how far the
/// editor's own window has travelled, so the window is always inside the
/// mirror and reaching the end of the file reaches the end of the strip.
///
/// The proportion is exact rather than approximate: at the bottom the strip
/// has moved by every line it cannot hold, which is precisely enough to leave
/// the editor's last screenful in view.
fn scroll(editor_scroll: usize, lines: usize, fits: usize) -> usize {
    let fits = fits.max(1);
    let hidden = lines.saturating_sub(fits * LINES);
    let travel = lines.saturating_sub(fits);
    match travel {
        0 => 0,
        _ => editor_scroll.min(travel) * hidden / travel,
    }
}

/// The 1-based inclusive lines the strip is mirroring, or nothing while it is
/// hidden. What a scenario asks and what the renderer draws, one answer.
pub fn mirrored(state: &State) -> Option<(usize, usize)> {
    if !showing(state) {
        return None;
    }
    let lines = lines(state);
    let fits = crate::fits(state).1.max(1);
    let first = scroll(state.editor_scroll, lines, fits);
    Some((first + 1, (first + fits * LINES).min(lines)))
}

/// Is the slider lit? The pointer being on the mirror is what lights it — a
/// line nobody is reaching for is a line that only has to be *findable*, and
/// under the hand that is about to drag it is the one moment it has something
/// to say. Asked here rather than read off the field, so the renderer cannot
/// light a slider on a mirror that is no longer there.
pub fn lit(state: &State) -> bool {
    state.hovered_minimap && showing(state)
}

/// Which rows of the strip the editor's own window covers — the slider, as a
/// first row and a count, 0-based in the strip.
pub fn slider(first: usize, editor_scroll: usize, fits: usize) -> (usize, usize) {
    let top = editor_scroll.saturating_sub(first) / LINES;
    let bottom = (editor_scroll.saturating_sub(first) + fits.max(1)).div_ceil(LINES);
    (top, bottom.saturating_sub(top).max(1))
}

/// Where the editor's window goes when the pointer holds row `row` of the
/// strip: far enough that the slider comes to rest under that row.
///
/// The inverse of [`scroll`] and [`slider`] composed, rather than "the line
/// drawn there, centred", which is the rule this started as. The mirror
/// scrolls as the window travels, so reading the line under the pointer afresh
/// on each report had the file running away underneath a pointer standing
/// still — every report named a line further down, because the one before it
/// had moved the mirror. This is a function of the row alone: press it twice
/// and the window is in the same place.
pub fn travel(row: usize, lines: usize, fits: usize) -> usize {
    let fits = fits.max(1);
    let travel = lines.saturating_sub(fits);
    // How far the slider itself moves down the strip while the window crosses
    // the file — the whole travel while the mirror holds still, and only the
    // part the mirror does not absorb once it scrolls too.
    let span = travel - lines.saturating_sub(fits * LINES);
    if span == 0 {
        return 0;
    }
    // Half the slider up, so the row held is the middle of what is shown
    // rather than its first line: a hand on a mirror is pointing at what it
    // covers.
    let top = row.saturating_sub(fits / (2 * LINES));
    (top * LINES * travel / span).min(travel)
}

/// Where the scrollbar's thumb sits, as a first row and a count of rows over a
/// pane `fits` rows tall. The whole file, not the part the strip mirrors: a
/// thumb is how far through the file the window is, which is the one thing the
/// strip stops saying once it scrolls itself.
///
/// Nothing at all when the file fits on screen: a bar spanning the whole edge
/// says there is somewhere to scroll to, and there is not.
pub fn thumb(editor_scroll: usize, lines: usize, fits: usize) -> Option<(usize, usize)> {
    let fits = fits.max(1);
    if lines <= fits {
        return None;
    }
    let height = (fits * fits / lines).max(1);
    let top = (fits - height) * editor_scroll.min(lines - fits) / (lines - fits);
    Some((top, height))
}

/// One cell of the strip: the kind of the first character with ink in each of
/// the two lines it stands for, or nothing where that line is blank there.
///
/// Kinds rather than colours, because colour is a theme's business and lives at
/// the edge — the same split `highlight` is built on. Which dot draws a given
/// pair is the edge's too: a dot is a shape on a screen, and this says only
/// where the ink is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell {
    pub top: Option<Kind>,
    pub bottom: Option<Kind>,
}

/// The strip's ink: `rows` rows of `width` cells, mirroring from the 0-based
/// line `first`. Past the end of the file the cells are blank, which is what
/// the editor shows there too.
pub fn cells(tokens: &[Vec<Token>], first: usize, rows: usize, width: usize) -> Vec<Vec<Cell>> {
    (0..rows)
        .map(|row| {
            let line = first + row * LINES;
            let ink = |at: usize| match tokens.get(at) {
                Some(tokens) => line_ink(tokens, width),
                None => vec![None; width],
            };
            let bottom = ink(line + 1);
            ink(line)
                .into_iter()
                .zip(bottom)
                .map(|(top, bottom)| Cell { top, bottom })
                .collect()
        })
        .collect()
}

/// What a row of the lane down the strip's last column says: the most
/// important thing on either of its two lines, anywhere in the file, so an
/// error below the window can be seen without scrolling to it. Declared in
/// that order, which `Ord` is what picks by.
///
/// The gutter's own priority, a Diagnostic before a change, with the two
/// quieter severities and the voice's reading place left out: a mark on a
/// strip this far off has to be worth a look, and the gutter already says the
/// rest where the line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    Error,
    Warning,
    Changed,
}

/// The lane's `rows` marks, mirroring from the 0-based line `first` — the rows
/// on screen, as [`cells`] is. Read off the same Diagnostics and the same
/// Change-bar trace the gutter reads, so the lane and the gutter cannot
/// disagree about a line.
pub fn lane(state: &State, first: usize, rows: usize) -> Vec<Option<Mark>> {
    let Some(path) = state.current_buffer.as_deref() else {
        return vec![None; rows];
    };
    // The trace `changed_lines` filters, indexed at the lines on screen rather
    // than filtered whole, so the lane costs its rows and not the file.
    let traced = crate::authorship::traced_lines(state).unwrap_or_default();
    let mark = |line: usize| match crate::lsp::mark(state, path, line) {
        Some(crate::lsp::Severity::Error) => Some(Mark::Error),
        Some(crate::lsp::Severity::Warning) => Some(Mark::Warning),
        _ => (traced.get(line - 1) == Some(&None)).then_some(Mark::Changed),
    };
    (0..rows)
        .map(|row| {
            let line = first + row * LINES + 1;
            (line..line + LINES).filter_map(mark).min()
        })
        .collect()
}

/// Which kind, if any, has ink in each cell of one line. The *first* inked
/// character of the four a cell stands for: a cell is one mark, and a mark
/// that averaged four kinds would be a colour the file does not hold.
fn line_ink(tokens: &[Token], width: usize) -> Vec<Option<Kind>> {
    let mut cells = vec![None; width];
    let mut column = 0;
    for token in tokens {
        for character in token.text.chars() {
            let cell = column / COLUMNS;
            if cell >= width {
                return cells;
            }
            if !character.is_whitespace() && cells[cell].is_none() {
                cells[cell] = Some(token.kind);
            }
            column += 1;
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant the whole derivation exists for: wherever the editor's
    /// window is, the strip is showing it. Swept over every offset of a file
    /// far taller than the strip can hold.
    #[test]
    fn the_window_is_always_inside_the_mirror() {
        let (lines, fits) = (500, 19);
        for editor_scroll in 0..=lines - fits {
            let first = scroll(editor_scroll, lines, fits);
            assert!(first <= editor_scroll, "{first} > {editor_scroll}");
            assert!(
                editor_scroll + fits <= first + fits * LINES,
                "window {editor_scroll}..{} past the mirror at {first}",
                editor_scroll + fits
            );
        }
    }

    #[test]
    fn a_file_that_fits_does_not_scroll_the_mirror() {
        assert_eq!(scroll(0, 20, 19), 0);
        assert_eq!(scroll(1, 38, 19), 0);
    }

    #[test]
    fn the_end_of_the_file_is_the_end_of_the_mirror() {
        assert_eq!(scroll(181, 200, 19), 162);
    }

    /// The property a drag depends on: the answer is the row's alone, so a
    /// pointer held still leaves the window still — and wherever it lands, the
    /// slider is drawn back under the row that was held.
    #[test]
    fn travelling_to_a_row_leaves_the_slider_on_it() {
        let (lines, fits) = (500, 19);
        for row in 0..fits {
            let editor_scroll = travel(row, lines, fits);
            assert_eq!(
                editor_scroll,
                travel(row, lines, fits),
                "row {row} moved on its own"
            );
            let (top, height) = slider(scroll(editor_scroll, lines, fits), editor_scroll, fits);
            assert!(
                row >= top.saturating_sub(1) && row <= top + height,
                "row {row} is outside the slider {top}..{}",
                top + height
            );
        }
    }

    #[test]
    fn the_slider_covers_the_rows_the_editor_is_showing() {
        assert_eq!(slider(0, 0, 19), (0, 10));
        assert_eq!(slider(162, 181, 19), (9, 10));
    }

    #[test]
    fn the_thumb_is_the_whole_file() {
        assert_eq!(thumb(0, 19, 19), None);
        assert_eq!(thumb(0, 200, 19), Some((0, 1)));
        assert_eq!(thumb(181, 200, 19), Some((18, 1)));
    }

    #[test]
    fn a_cell_takes_the_kind_of_its_first_inked_character() {
        let line = vec![
            Token {
                text: "    ".to_string(),
                kind: Kind::Plain,
            },
            Token {
                text: "fn".to_string(),
                kind: Kind::Keyword,
            },
        ];
        assert_eq!(line_ink(&line, 3), [None, Some(Kind::Keyword), None]);
    }

    #[test]
    fn a_row_mirrors_two_lines() {
        let keyword = |text: &str| {
            vec![Token {
                text: text.to_string(),
                kind: Kind::Keyword,
            }]
        };
        let tokens = vec![keyword("a"), keyword("b")];
        assert_eq!(
            cells(&tokens, 0, 2, 1),
            [
                vec![Cell {
                    top: Some(Kind::Keyword),
                    bottom: Some(Kind::Keyword)
                }],
                vec![Cell::default()],
            ]
        );
    }

    /// A 60-line file whose line 7 and line 9 the commit does not hold, with
    /// the Diagnostics given, 1-based.
    fn committed() -> String {
        (1..=60).map(|n| format!("line {n}\n")).collect()
    }

    fn marked(diagnostics: &[(usize, crate::lsp::Severity)]) -> State {
        let committed = committed();
        let text = committed
            .replace("line 7\n", "edited\n")
            .replace("line 9\n", "edited\n");
        let path = std::path::PathBuf::from("/w/main.rs");
        let buffer = crate::editor::Buffer::open(&text, false, 4);
        let mut state = State {
            current_buffer: Some(path.clone()),
            traced: Some((
                path.clone(),
                buffer.revision(),
                crate::authorship::traced(Some(&committed), &text).into(),
            )),
            ..State::default()
        };
        state.buffers.insert(path.clone(), buffer);
        let diagnostics = diagnostics
            .iter()
            .map(|&(line, severity)| crate::lsp::Diagnostic {
                line,
                column: 1,
                end_column: None,
                severity,
                message: "no".to_string(),
            })
            .collect();
        state
            .diagnostics
            .insert(path, [("rust".to_string(), diagnostics)].into());
        state
    }

    #[test]
    fn an_error_off_screen_marks_its_row() {
        let state = marked(&[(40, crate::lsp::Severity::Error)]);
        // Line 40 is the second line of row 19 from the top of the file.
        let lane = lane(&state, 0, 30);
        assert_eq!(lane[19], Some(Mark::Error));
        assert_eq!(
            lane.iter().flatten().filter(|&&m| m == Mark::Error).count(),
            1
        );
    }

    #[test]
    fn a_row_takes_its_most_important_line() {
        use crate::lsp::Severity::{Error, Warning};
        // Row 3 holds 7 (changed) and 8 (warning); row 5 holds 11 and 12.
        let state = marked(&[(8, Warning), (11, Error), (12, Warning)]);
        let lane = lane(&state, 0, 6);
        assert_eq!(lane[3], Some(Mark::Warning));
        assert_eq!(lane[4], Some(Mark::Changed));
        assert_eq!(lane[5], Some(Mark::Error));
    }

    #[test]
    fn the_quiet_severities_do_not_mark_the_lane() {
        use crate::lsp::Severity::{Hint, Information};
        let state = marked(&[(1, Information), (4, Hint)]);
        assert_eq!(lane(&state, 0, 2), [None, None]);
    }

    #[test]
    fn a_line_back_as_committed_is_not_marked_changed() {
        let mut state = marked(&[]);
        let path = state.current_buffer.clone().expect("open");
        let committed = committed();
        let revision = state.buffers[&path].revision();
        state.traced = Some((
            path,
            revision,
            crate::authorship::traced(Some(&committed), &committed).into(),
        ));
        assert_eq!(lane(&state, 0, 6), [None; 6]);
    }

    #[test]
    fn the_lane_starts_where_the_mirror_does() {
        let state = marked(&[(40, crate::lsp::Severity::Error)]);
        assert_eq!(lane(&state, 38, 1), [Some(Mark::Error)]);
    }
}
