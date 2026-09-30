//! The Conflicts git's merge left in a file's text, found on the text alone,
//! and the Conflict list that walks every unmerged file's. New file: a
//! Conflict is its own workspace concept — the editor draws and accepts one,
//! the Corner lists them — and none of the existing modules owns it.

use crate::{Place, State};
use std::path::{Path, PathBuf};

/// One Conflict, by the 1-based lines its markers are on, and the labels those
/// markers carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The `<<<<<<<` line.
    pub start: usize,
    /// The diff3 `|||||||` line, when git wrote the common ancestor too.
    pub base: Option<usize>,
    /// The `=======` line.
    pub middle: usize,
    /// The `>>>>>>>` line.
    pub end: usize,
    pub current: String,
    pub ancestor: String,
    pub incoming: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Current,
    Incoming,
    /// Current first, then incoming.
    Both,
}

/// The label after a marker, or `None` for a line that is not that marker.
/// Exactly seven characters and then a space or nothing, as git writes them: a
/// line of eight `=` is somebody's text.
fn marker<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.trim_end_matches('\r').strip_prefix(prefix)?;
    match rest.strip_prefix(' ') {
        Some(label) => Some(label.trim()),
        None => rest.is_empty().then_some(""),
    }
}

/// Every Conflict in the text, in order. A region whose markers are not all
/// there, or not in order, is not one — it is text somebody is editing.
pub fn find<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<Conflict> {
    let mut found = Vec::new();
    // The one being read, and whether its `=======` has been reached.
    let mut open: Option<(Conflict, bool)> = None;
    for (index, line) in lines.into_iter().enumerate() {
        let number = index + 1;
        if let Some(label) = marker(line, "<<<<<<<") {
            open = Some((
                Conflict {
                    start: number,
                    base: None,
                    middle: 0,
                    end: 0,
                    current: crate::debug::printable(label),
                    ancestor: String::new(),
                    incoming: String::new(),
                },
                false,
            ));
            continue;
        }
        let Some((conflict, divided)) = open.as_mut() else {
            continue;
        };
        if !*divided {
            if let Some(label) = marker(line, "|||||||").filter(|_| conflict.base.is_none()) {
                conflict.base = Some(number);
                conflict.ancestor = crate::debug::printable(label);
            } else if marker(line, "=======") == Some("") {
                conflict.middle = number;
                *divided = true;
            } else if marker(line, ">>>>>>>").is_some() {
                open = None;
            }
        } else if let Some(label) = marker(line, ">>>>>>>") {
            conflict.end = number;
            conflict.incoming = crate::debug::printable(label);
            found.extend(open.take().map(|(conflict, _)| conflict));
        }
    }
    found
}

/// What accepting `side` leaves in place of the whole region, marker lines and
/// ancestor included. `lines` is the whole text, one entry per line.
pub fn kept(lines: &[String], conflict: &Conflict, side: Side) -> Vec<String> {
    let current = &lines[conflict.start..conflict.base.unwrap_or(conflict.middle) - 1];
    let incoming = &lines[conflict.middle..conflict.end - 1];
    match side {
        Side::Current => current.to_vec(),
        Side::Incoming => incoming.to_vec(),
        Side::Both => [current, incoming].concat(),
    }
}

/// How the editor draws one of a Conflict's lines in place of its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drawn {
    /// A marker line, as a bar: its pieces left to right, the buttons naming
    /// the side they accept. The renderer runs the bar on to the pane's edge.
    Bar(Vec<(String, Option<Side>)>),
    Current,
    Ancestor,
    Incoming,
}

/// How line `number` of the Buffer on screen is drawn, or `None` for a line
/// drawn as its text. Only over the file's own lines — a diff, a Preview and a
/// walked Site draw rows that are not them — and a marker the cursor is on is
/// its text, so it can be read and edited. The one answer `ui` draws and
/// `mouse` hit-tests the buttons with.
pub fn drawn(state: &State, number: usize) -> Option<Drawn> {
    if state.diff.is_some() || state.walking.is_some() || crate::previewing(state) {
        return None;
    }
    let buffer = crate::current_buffer(state)?;
    let conflict = buffer
        .conflicts()
        .iter()
        .find(|conflict| (conflict.start..=conflict.end).contains(&number))?;
    let titled = |what: &str, label: &str| match label {
        "" => format!("┄┄ {what} ┄┄"),
        label => format!("┄┄ {what} ({label}) ┄┄"),
    };
    let bar = |pieces: Vec<(String, Option<Side>)>| match buffer.line == number {
        true => None,
        false => Some(Drawn::Bar(pieces)),
    };
    match number {
        start if start == conflict.start => {
            let mut pieces = vec![
                (titled("Current change", &conflict.current), None),
                (" ".into(), None),
                ("[accept current]".into(), Some(Side::Current)),
                (" ".into(), None),
                ("[accept incoming]".into(), Some(Side::Incoming)),
                (" ".into(), None),
                ("[accept both]".into(), Some(Side::Both)),
                (" ".into(), None),
            ];
            // The buttons are what a narrow editor keeps: the title says what
            // the bar below it says too, and a button cut off is one nobody
            // can press.
            let width: usize = pieces.iter().map(|(piece, _)| piece.chars().count()).sum();
            if width > crate::fits(state).2 {
                pieces.drain(..2);
            }
            bar(pieces)
        }
        base if Some(base) == conflict.base => {
            bar(vec![(titled("Common ancestor", &conflict.ancestor), None)])
        }
        middle if middle == conflict.middle => bar(vec![]),
        end if end == conflict.end => {
            bar(vec![(titled("Incoming change", &conflict.incoming), None)])
        }
        inside if inside < conflict.base.unwrap_or(conflict.middle) => Some(Drawn::Current),
        inside if inside < conflict.middle => Some(Drawn::Ancestor),
        _ => Some(Drawn::Incoming),
    }
}

/// The files git reports as unmerged, as absolute paths sorted by path.
fn unmerged(state: &State) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = state
        .repo
        .iter()
        .flatten()
        .filter(|file| file.status == crate::review::GitStatus::Conflicted)
        .map(|file| state.root.join(&file.path))
        .collect();
    paths.sort();
    paths
}

/// A file's Conflicts: its Buffer's when it is open, since that is the text
/// being resolved, and what the edge last read off the disk otherwise.
/// Nothing for a file it could not read, which is not one with none left.
fn conflicts_in<'a>(state: &'a State, path: &Path) -> Option<&'a [Conflict]> {
    match state.buffers.get(path) {
        Some(buffer) => Some(buffer.conflicts()),
        None => state.conflicts_on_disk.get(path).map(Vec::as_slice),
    }
}

/// The Conflict list's rows: each unmerged file — `None` — with a row under it
/// for every Conflict still in its text. Read by `ui` to draw, by `mouse` to
/// hit-test and by `update` to act on.
pub fn listed(state: &State) -> Vec<(PathBuf, Option<&Conflict>)> {
    if state.corner != crate::layout::Corner::Conflicts {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for path in unmerged(state) {
        let conflicts = conflicts_in(state, &path).unwrap_or_default();
        rows.push((path.clone(), None));
        rows.extend(
            conflicts
                .iter()
                .map(|conflict| (path.clone(), Some(conflict))),
        );
    }
    rows
}

/// Where the row at `index` lands: its Conflict's first marker line, or — for
/// a file row — its first Conflict's, and the file's top once none are left.
pub fn landing(state: &State, index: usize) -> Option<(PathBuf, Place)> {
    let (path, conflict) = listed(state).into_iter().nth(index)?;
    let line = conflict
        .or_else(|| conflicts_in(state, &path)?.first())
        .map_or(1, |conflict| conflict.start);
    Some((path, Place { line, column: 1 }))
}

/// What a row says: a file's path, ticked once nothing is left in it, or where
/// a Conflict starts and the two sides it is between.
pub fn row_text(state: &State, path: &std::path::Path, conflict: Option<&Conflict>) -> String {
    match conflict {
        Some(conflict) => format!(
            "  {}  {} ↔ {}",
            conflict.start, conflict.current, conflict.incoming
        ),
        None => {
            let name = crate::debug::printable(&crate::relative(state, path));
            match conflicts_in(state, path) {
                Some([]) => format!("{name} ✓"),
                _ => name,
            }
        }
    }
}

/// The list's border: how many files, and how many Conflicts are left in them.
pub fn title(state: &State) -> String {
    let files = unmerged(state);
    let left: usize = files
        .iter()
        .map(|path| conflicts_in(state, path).map_or(0, <[Conflict]>::len))
        .sum();
    let noun = match files.len() {
        1 => "file",
        _ => "files",
    };
    format!("conflicts {} {noun}, {left} left", files.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &str = "before\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nafter";
    const DIFF3: &str =
        "<<<<<<< HEAD\nours\n||||||| base\nold\n=======\ntheirs\ntheirs 2\n>>>>>>> topic";

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    #[test]
    fn a_conflict_is_found_by_its_markers_and_labels() {
        assert_eq!(
            find(PLAIN.split('\n')),
            vec![Conflict {
                start: 2,
                base: None,
                middle: 4,
                end: 6,
                current: "HEAD".into(),
                ancestor: String::new(),
                incoming: "feature".into(),
            }]
        );
    }

    #[test]
    fn a_diff3_conflict_carries_its_ancestor() {
        let found = find(DIFF3.split('\n'));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].base, Some(3));
        assert_eq!(found[0].ancestor, "base");
        assert_eq!((found[0].middle, found[0].end), (5, 8));
    }

    #[test]
    fn a_region_missing_a_marker_is_not_a_conflict() {
        assert_eq!(find("<<<<<<< HEAD\nours\n>>>>>>> x".split('\n')), vec![]);
        assert_eq!(
            find("<<<<<<< HEAD\nours\n=======\ntheirs".split('\n')),
            vec![]
        );
        assert_eq!(find("=======\ntheirs\n>>>>>>> x".split('\n')), vec![]);
        // Eight is somebody's text, not a marker.
        assert_eq!(
            find("<<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> x".split('\n')),
            vec![]
        );
    }

    #[test]
    fn a_second_opening_marker_starts_over() {
        let found = find("<<<<<<< a\nx\n<<<<<<< b\ny\n=======\nz\n>>>>>>> c".split('\n'));
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].start, found[0].current.as_str()), (3, "b"));
    }

    #[test]
    fn a_label_cannot_carry_an_escape() {
        let found = find("<<<<<<< \u{1b}[2J\nx\n=======\ny\n>>>>>>>".split('\n'));
        assert_eq!(found[0].current, "[2J");
        assert_eq!(found[0].incoming, "");
    }

    #[test]
    fn the_border_counts_what_is_left_and_a_resolved_file_is_ticked() {
        use crate::review::{GitFile, GitStatus};
        let mut state = State {
            root: PathBuf::from("/w"),
            corner: crate::layout::Corner::Conflicts,
            ..State::default()
        };
        state.repo = Some(
            ["b.rs", "a.rs", "c.rs"]
                .map(|path| GitFile {
                    path: path.into(),
                    status: GitStatus::Conflicted,
                })
                .to_vec(),
        );
        state.conflicts_on_disk = [
            (PathBuf::from("/w/a.rs"), find(PLAIN.split('\n'))),
            (PathBuf::from("/w/b.rs"), vec![]),
        ]
        .into();
        assert_eq!(title(&state), "conflicts 3 files, 1 left");
        let rows: Vec<String> = listed(&state)
            .iter()
            .map(|(path, conflict)| row_text(&state, path, *conflict))
            .collect();
        // `c.rs` was never read, so nothing says it is resolved.
        assert_eq!(rows, ["a.rs", "  2  HEAD ↔ feature", "b.rs ✓", "c.rs"]);
    }

    #[test]
    fn accepting_keeps_one_side_or_both_and_drops_the_ancestor() {
        let text = lines(DIFF3);
        let conflict = &find(DIFF3.split('\n'))[0];
        assert_eq!(kept(&text, conflict, Side::Current), ["ours"]);
        assert_eq!(
            kept(&text, conflict, Side::Incoming),
            ["theirs", "theirs 2"]
        );
        assert_eq!(
            kept(&text, conflict, Side::Both),
            ["ours", "theirs", "theirs 2"]
        );
        let text = lines(PLAIN);
        let conflict = &find(PLAIN.split('\n'))[0];
        assert_eq!(kept(&text, conflict, Side::Both), ["ours", "theirs"]);
    }
}
