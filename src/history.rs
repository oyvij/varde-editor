use crate::{Effect, Event, Place, State};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

pub const CAP: usize = 100;

pub const WORDS: usize = 3;

pub const GO_TO: &str = "go-to-place";

pub fn list(state: &State) -> &[Visit] {
    &state.visits
}

pub fn selected(state: &State) -> Option<&Visit> {
    state.visits.get(state.history_selection)
}

pub fn row_actions(state: &State) -> Vec<&'static str> {
    match selected(state) {
        Some(_) => vec![GO_TO],
        None => Vec::new(),
    }
}

pub fn stale(state: &State, visit: &Visit) -> bool {
    let Some(buffer) = state
        .buffers
        .iter()
        .find(|(path, _)| crate::relative(state.shown_root(), path) == visit.file)
        .map(|(_, buffer)| buffer)
    else {
        return false;
    };
    line_of(buffer, visit.line).as_deref() != Some(visit.text.as_str())
}

pub fn excerpt(text: &str, column: usize, words: usize) -> String {
    let mut found: Vec<(usize, &str)> = Vec::new();
    let mut at = 1;
    for word in text.split_inclusive(char::is_whitespace) {
        let trimmed = word.trim_end();
        if !trimmed.is_empty() {
            found.push((at, trimmed));
        }
        at += word.chars().count();
    }
    let from = found
        .iter()
        .rposition(|(start, _)| *start <= column)
        .unwrap_or(0);
    let taken: Vec<&str> = found[from..]
        .iter()
        .take(words.max(1))
        .map(|(_, word)| *word)
        .collect();
    let mut excerpt = taken.join(" ");
    if from + taken.len() < found.len() {
        excerpt.push_str("...");
    }
    excerpt
}

pub enum Jump {
    ToFile { file: String, at: Option<Place> },
    InFile,
    From(Visit),
    No,
}

pub fn jumped(state: &State, event: &Event) -> Jump {
    match event {
        Event::BufferOpened { preview: true, .. } if state.preview == state.current_buffer => {
            Jump::No
        }
        Event::BufferOpened { .. } if state.restoring > 0 => Jump::No,
        Event::BufferOpened { path, at, .. } => Jump::ToFile {
            file: crate::relative(state.shown_root(), path),
            at: *at,
        },
        Event::ShowBuffer(path) => Jump::ToFile {
            file: crate::relative(state.shown_root(), path),
            at: None,
        },
        Event::StepMatch(_) => Jump::InFile,
        Event::AcceptFind => match state
            .find
            .as_ref()
            .and_then(|find| here(state, Some(find.origin)))
        {
            Some(origin) => Jump::From(origin),
            None => Jump::No,
        },
        Event::EditorKey('G') if pending(state).is_empty() => Jump::InFile,
        Event::EditorKey('g') if pending(state) == "g" => Jump::InFile,
        _ => Jump::No,
    }
}

pub fn record(state: &State, next: &mut State, jump: Jump) {
    let left = match &jump {
        Jump::From(origin) => Some(origin.clone()),
        _ => here(state, None),
    };
    let Some(left) = left else {
        return;
    };
    let been = match &jump {
        Jump::No => false,
        Jump::InFile | Jump::From(_) => here(next, None).as_ref() != Some(&left),
        Jump::ToFile { file, at } if *file == left.file => !went_nowhere(&left, *at),
        Jump::ToFile { file, .. } => !walking(state, file, &left),
    };
    if been {
        remember(next, left);
    }
}

fn went_nowhere(left: &Visit, at: Option<Place>) -> bool {
    match at {
        Some(at) => (at.line, at.column) == (left.line, left.column),
        None => true,
    }
}

pub fn leaving(state: &mut State, to: Place) {
    let Some(left) = here(state, None) else {
        return;
    };
    if went_nowhere(&left, Some(to)) {
        return;
    }
    remember(state, left);
}

fn remember(next: &mut State, left: Visit) {
    if next.visits.last() == Some(&left) {
        return;
    }
    next.visits.retain(|visit| {
        (&visit.file, visit.line, visit.column) != (&left.file, left.line, left.column)
    });
    push(next, left);
    next.history_selection = next.visits.len();
}

fn walking(state: &State, arriving: &str, left: &Visit) -> bool {
    let at = state.history_selection;
    let names_the_file = selected(state).map(|visit| visit.file.as_str()) == Some(arriving);
    let next_along = [at.checked_sub(1), at.checked_add(1)]
        .into_iter()
        .flatten()
        .any(|index| state.visits.get(index) == Some(left));
    names_the_file && next_along
}

pub fn back(state: &State, mut next: State) -> (State, Vec<Effect>) {
    let target = if state.history_selection >= state.visits.len() {
        match (here(state, None), next.visits.len().checked_sub(1)) {
            (_, None) => None,
            (None, newest) => newest,
            (Some(place), Some(newest)) if next.visits.last() == Some(&place) => {
                newest.checked_sub(1)
            }
            (Some(place), Some(_)) => {
                push(&mut next, place);
                next.visits.len().checked_sub(2)
            }
        }
    } else {
        state.history_selection.checked_sub(1)
    };
    match target {
        Some(target) => go_to(state, next, target),
        None => (next, vec![Effect::Notify("no-earlier-place")]),
    }
}

pub fn forward(state: &State, next: State) -> (State, Vec<Effect>) {
    match state.history_selection.checked_add(1) {
        Some(target) if target < state.visits.len() => go_to(state, next, target),
        _ => (next, vec![Effect::Notify("no-later-place")]),
    }
}

pub fn go(state: &State, next: State) -> (State, Vec<Effect>) {
    let target = state.history_selection;
    go_to(state, next, target)
}

fn go_to(state: &State, mut next: State, target: usize) -> (State, Vec<Effect>) {
    let Some(visit) = next.visits.get(target).cloned() else {
        return (next, vec![Effect::Notify("no-place-here")]);
    };
    next.history_selection = target;
    let path = state.shown_root().join(&visit.file);
    let at = Place {
        line: visit.line,
        column: visit.column,
    };
    if state.current_buffer.as_deref() == Some(path.as_path()) {
        return crate::update(&next, Event::JumpTo(at));
    }
    (next, vec![Effect::OpenAt { path, at }])
}

fn here(state: &State, at: Option<Place>) -> Option<Visit> {
    let path = state.current_buffer.as_ref()?;
    let buffer = state.buffers.get(path)?;
    let at = if crate::previewing(state) {
        Place {
            line: crate::source_line(state, at.map_or(buffer.row, |at| at.line)),
            column: 1,
        }
    } else {
        at.unwrap_or(Place {
            line: buffer.line,
            column: buffer.column,
        })
    };
    Some(Visit {
        file: crate::relative(state.shown_root(), path),
        line: at.line,
        column: at.column,
        text: line_of(buffer, at.line).unwrap_or_default(),
    })
}

fn push(next: &mut State, place: Visit) {
    next.visits.push(place);
    if next.visits.len() > CAP {
        next.visits.remove(0);
    }
}

fn line_of(buffer: &crate::editor::Buffer, line: usize) -> Option<String> {
    buffer
        .shown()
        .split('\n')
        .nth(line.checked_sub(1)?)
        .map(|text| text.trim_end().to_string())
}

fn pending(state: &State) -> &str {
    state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
        .map(|buffer| buffer.pending())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_excerpt_starts_at_the_word_the_cursor_was_on() {
        assert_eq!(excerpt("pub struct Buffer {", 1, 3), "pub struct Buffer...");
        assert_eq!(excerpt("pub struct Buffer {", 5, 3), "struct Buffer {");
        assert_eq!(excerpt("use std::fmt;", 5, 3), "std::fmt;");
    }

    #[test]
    fn an_excerpt_drops_the_indentation_and_survives_the_edges() {
        assert_eq!(excerpt("        let a = 1;", 1, 3), "let a =...");
        assert_eq!(excerpt("a b", 40, 3), "b");
        assert_eq!(excerpt("", 1, 3), "");
        assert_eq!(excerpt("     ", 3, 3), "");
    }

    #[test]
    fn the_ends_of_the_file_are_jumps_and_a_delete_over_one_is_not() {
        let state = crate::update(
            &State::default(),
            Event::BufferOpened {
                path: std::path::PathBuf::from("/w/a.rs"),
                contents: "one\ntwo\nthree\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        assert!(matches!(
            jumped(&state, &Event::EditorKey('G')),
            Jump::InFile
        ));
        let pending = crate::update(&state, Event::EditorKey('g')).0;
        assert!(matches!(
            jumped(&pending, &Event::EditorKey('g')),
            Jump::InFile
        ));
        let deleting = crate::update(&state, Event::EditorKey('d')).0;
        assert!(matches!(
            jumped(&deleting, &Event::EditorKey('G')),
            Jump::No
        ));
    }

    #[test]
    fn a_motion_is_not_a_jump() {
        let state = State::default();
        for event in [
            Event::EditorArrow(crate::Direction::Down),
            Event::EditorKey('j'),
            Event::EditorKey('k'),
            Event::ClickText(Place { line: 2, column: 2 }),
        ] {
            assert!(
                matches!(jumped(&state, &event), Jump::No),
                "{event:?} is not a jump"
            );
        }
        assert!(matches!(
            jumped(&state, &Event::StepMatch(crate::Direction::Right)),
            Jump::InFile
        ));
        assert!(matches!(
            jumped(&state, &Event::ShowBuffer("/w/a.rs".into())),
            Jump::ToFile { .. }
        ));
    }
}
