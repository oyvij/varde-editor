use crate::highlight::{Kind, Token};
use crate::layout::Area;
use crate::State;

const WIDTH: u16 = 12;

const COLUMNS: usize = 4;

const LINES: usize = 2;

pub fn mirroring(state: &State) -> bool {
    state.diff.is_none()
        && state.walking.is_none()
        && !crate::previewing(state)
        && crate::current_buffer(state).is_some()
}

const ROOM: u16 = 20;

fn showing(state: &State) -> bool {
    state.minimap
        && mirroring(state)
        && crate::panes_of(state)
            .editor
            .width
            .saturating_sub(2 + crate::gutter(state) + WIDTH)
            >= ROOM
}

pub fn width(state: &State) -> u16 {
    match showing(state) {
        true => WIDTH,
        false => 0,
    }
}

pub fn strip(state: &State, editor: Area) -> Area {
    let width = width(state);
    Area {
        x: editor.right().saturating_sub(1 + width),
        y: editor.y + 1,
        width,
        height: editor.height.saturating_sub(2),
    }
}

pub fn lines(state: &State) -> usize {
    crate::current_buffer(state).map_or(0, |buffer| buffer.shown().lines().count())
}

fn scroll(editor_scroll: usize, lines: usize, fits: usize) -> usize {
    let fits = fits.max(1);
    let hidden = lines.saturating_sub(fits * LINES);
    let travel = lines.saturating_sub(fits);
    match travel {
        0 => 0,
        _ => editor_scroll.min(travel) * hidden / travel,
    }
}

pub fn mirrored(state: &State) -> Option<(usize, usize)> {
    if !showing(state) {
        return None;
    }
    let lines = lines(state);
    let fits = crate::fits(state).1.max(1);
    let first = scroll(state.editor_scroll, lines, fits);
    Some((first + 1, (first + fits * LINES).min(lines)))
}

pub fn lit(state: &State) -> bool {
    state.hovered_minimap && showing(state)
}

pub fn slider(first: usize, editor_scroll: usize, fits: usize) -> (usize, usize) {
    let top = editor_scroll.saturating_sub(first) / LINES;
    let bottom = (editor_scroll.saturating_sub(first) + fits.max(1)).div_ceil(LINES);
    (top, bottom.saturating_sub(top).max(1))
}

pub fn travel(row: usize, lines: usize, fits: usize) -> usize {
    let fits = fits.max(1);
    let travel = lines.saturating_sub(fits);
    let span = travel - lines.saturating_sub(fits * LINES);
    if span == 0 {
        return 0;
    }
    let top = row.saturating_sub(fits / (2 * LINES));
    (top * LINES * travel / span).min(travel)
}

pub fn thumb(editor_scroll: usize, lines: usize, fits: usize) -> Option<(usize, usize)> {
    let fits = fits.max(1);
    if lines <= fits {
        return None;
    }
    let height = (fits * fits / lines).max(1);
    let top = (fits - height) * editor_scroll.min(lines - fits) / (lines - fits);
    Some((top, height))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell {
    pub top: Option<Kind>,
    pub bottom: Option<Kind>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    Error,
    Warning,
    Changed,
}

pub fn lane(state: &State, first: usize, rows: usize) -> Vec<Option<Mark>> {
    let Some(path) = state.current_buffer.as_deref() else {
        return vec![None; rows];
    };
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
