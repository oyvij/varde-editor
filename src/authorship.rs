use crate::{State, View};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authored {
    pub author: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Authorship {
    Committed(Authored),
    NotCommittedYet,
}

pub fn at_cursor(state: &State) -> Option<Authorship> {
    if !state.git_installed || state.view != View::Edit {
        return None;
    }
    state.repo.as_ref()?;
    let path = state.current_buffer.as_ref()?;
    let buffer = state.buffers.get(path)?;
    let line = match crate::previewing(state) {
        true => crate::preview_rows(state)
            .get(buffer.row.saturating_sub(1))
            .map(|row| row.line)?,
        false => buffer.line,
    };
    let authors = state
        .authorship
        .get(path)
        .map(|lines| &lines[..])
        .unwrap_or_default();
    let at = traced_lines(state)?
        .get(line.checked_sub(1)?)
        .copied()
        .flatten();
    Some(match at.and_then(|at| authors.get(at - 1)) {
        Some(authored) => Authorship::Committed(authored.clone()),
        None => Authorship::NotCommittedYet,
    })
}

pub fn traced_lines(state: &State) -> Option<&[Option<usize>]> {
    let (path, revision, lines) = state.traced.as_ref()?;
    let buffer = crate::current_buffer(state)?;
    (state.current_buffer.as_ref() == Some(path) && buffer.revision() == *revision)
        .then_some(&lines[..])
}

pub fn traced(committed: Option<&str>, shown: &str) -> Vec<Option<usize>> {
    let Some(committed) = committed else {
        return Vec::new();
    };
    let count = shown.split('\n').count();
    let Ok(patch) =
        git2::Patch::from_buffers(committed.as_bytes(), None, shown.as_bytes(), None, None)
    else {
        return vec![None; count];
    };
    let mut hunks: BTreeMap<usize, Option<usize>> = BTreeMap::new();
    for hunk in 0..patch.num_hunks() {
        let Ok((_, lines)) = patch.hunk(hunk) else {
            continue;
        };
        for index in 0..lines {
            let Ok(line) = patch.line_in_hunk(hunk, index) else {
                continue;
            };
            if let Some(new) = line.new_lineno() {
                hunks.insert(new as usize, line.old_lineno().map(|old| old as usize));
            }
        }
    }
    (1..=count)
        .map(|line| match hunks.get(&line) {
            Some(found) => *found,
            None => {
                let offset = hunks
                    .range(..line)
                    .rev()
                    .find_map(|(new, old)| Some((*old)? as isize - *new as isize))
                    .unwrap_or(0);
                usize::try_from(line as isize + offset).ok()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";

    #[test]
    fn the_marks_read_the_trace_the_edge_told() {
        let path = std::path::PathBuf::from("/w/main.rs");
        let mut state = State {
            current_buffer: Some(path.clone()),
            ..State::default()
        };
        state.buffers.insert(
            path.clone(),
            crate::editor::Buffer::open("a\nb\n", false, 4),
        );
        state
            .committed
            .insert(path.clone(), Some("a\n".to_string()));
        assert!(crate::changed_lines(&state).is_empty(), "diffed on its own");

        let revision = state.buffers[&path].revision();
        state.traced = Some((path.clone(), revision, vec![Some(1), None, None].into()));
        assert_eq!(crate::changed_lines(&state), [2, 3]);

        state.traced = Some((path, revision - 1, vec![None, Some(1), None].into()));
        assert!(
            crate::changed_lines(&state).is_empty(),
            "an older revision's"
        );

        let other = std::path::PathBuf::from("/w/other.rs");
        state.traced = Some((other, revision, vec![None].into()));
        assert!(crate::changed_lines(&state).is_empty());
    }

    #[test]
    fn a_buffer_line_is_traced_back_to_the_line_the_commit_holds() {
        let at =
            |committed: &str, shown: &str, line: usize| traced(Some(committed), shown)[line - 1];

        let same = |line| at(COMMITTED, COMMITTED, line);
        assert_eq!((same(1), same(5), same(10)), (Some(1), Some(5), Some(10)));

        let inserted = COMMITTED.replace("five\n", "five\nFIVE AND A HALF\n");
        let after = |line| at(COMMITTED, &inserted, line);
        assert_eq!((after(5), after(6)), (Some(5), None));
        assert_eq!((after(7), after(11)), (Some(6), Some(10)));

        let edited = COMMITTED.replace("five\n", "FIVE\n");
        let over = |line| at(COMMITTED, &edited, line);
        assert_eq!((over(4), over(5), over(6)), (Some(4), None, Some(6)));

        let deleted = COMMITTED.replace("five\n", "");
        let short = |line| at(COMMITTED, &deleted, line);
        assert_eq!((short(4), short(5), short(9)), (Some(4), Some(6), Some(10)));
    }

    #[test]
    fn every_line_of_an_uncommitted_file_belongs_to_nobody() {
        assert_eq!(traced(Some(""), "fn new() {}"), [None]);
        assert_eq!(traced(Some(""), "fn new() {}\nfn old() {}"), [None, None]);
    }

    #[test]
    fn the_lines_the_commit_does_not_hold_are_the_ones_with_no_committed_line() {
        let changed = |committed: &str, shown: &str| -> Vec<usize> {
            traced(Some(committed), shown)
                .into_iter()
                .enumerate()
                .filter(|(_, at)| at.is_none())
                .map(|(index, _)| index + 1)
                .collect()
        };
        let old = "a\nb\nc\n";
        assert_eq!(changed(old, old), Vec::<usize>::new());
        assert_eq!(changed(old, "a\nx\nb\nc\n"), vec![2]);
        assert_eq!(changed(old, "a\nB\nc\n"), vec![2]);
        assert_eq!(changed(old, "a\nc\n"), Vec::<usize>::new());
        assert_eq!(changed(old, "1\na\nb\nc\n2\n"), vec![1, 5]);
    }
}
