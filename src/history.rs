//! F34 — cursor history: where the cursor has been, and the way back.
//!
//! A [`Visit`] is one place the cursor has been. Only a *jump* records one, and
//! the list of what counts as a jump is here rather than spread over the arms
//! that answer them: an arm-by-arm rule is a rule an arm will be added without.
//! Arrows, `j`/`k` and a click in the editor are Motions and are deliberately
//! not jumps — vim's jumplist records none of them, and a list that did would
//! be a keystroke log burying the four places worth returning to under four
//! hundred. Somebody will read that absence as an oversight; it is not.
//!
//! What is recorded is the place being **left**, read off the state the event
//! arrived at, so going back returns you where you were.
//!
//! The Visits are session state and are not persisted: they name lines of files
//! that move between sessions, and a list of stale places is worse than none.
//! The Corner's occupant is persisted, as every occupant's is.

use crate::{Effect, Event, Place, State};

/// One place the cursor has been: where it was, and what the line held at the
/// time. The text is carried rather than looked up so a row still says
/// something about a file that has since been edited or closed — with
/// [`stale`] to say the claim is no longer current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visit {
    /// Relative to the workspace root, as `risk::Function::file` and
    /// `search::Hit::file` are: the pane is the tree's width, and the rows are
    /// read against a workspace rather than a filesystem.
    pub file: String,
    pub line: usize,
    pub column: usize,
    /// The whole line, `trim_end`ed — the same shape `search::scan` stores a
    /// hit's line in, so the excerpt and a search result are cut from the same
    /// cloth.
    pub text: String,
}

/// The most places worth keeping. A history nobody bounded is a leak that
/// grows for as long as the session lives, and a pane cannot show it either.
/// Named for the reason `search::CAP` is named. The oldest goes first.
pub const CAP: usize = 100;

/// How many words of the line a row shows before the ellipsis. Enough to
/// recognise the line, not enough to need the whole pane: the row also carries
/// a file name, a line number and an action icon inside the tree's width.
pub const WORDS: usize = 3;

/// The one action a row offers: go to that file and that line. Named once,
/// because the core routes it, the mouse hit-tests it and the pane draws it.
pub const GO_TO: &str = "go-to-place";

/// The list, oldest first. A log of where you have been rather than a tree: a
/// jump made while travelling back appends rather than truncating what was
/// ahead, because the pane's job is to show where you have been and dropping
/// rows out from under it on every jump is a pane nobody can read.
pub fn list(state: &State) -> &[Visit] {
    &state.visits
}

/// The Visit the history's cursor is standing on, if it is standing on one.
/// `history_selection == visits.len()` is the position past the newest — where
/// you are before you have travelled anywhere, which is a place newer than
/// everything recorded and therefore no row.
pub fn selected(state: &State) -> Option<&Visit> {
    state.visits.get(state.history_selection)
}

/// What the row the keyboard is on offers. Only that row, the way a tree row
/// and a Risk row do, so the pane draws one icon rather than a column of them.
pub fn row_actions(state: &State) -> Vec<&'static str> {
    match selected(state) {
        Some(_) => vec![GO_TO],
        None => Vec::new(),
    }
}

/// Whether the line a Visit was taken from no longer holds what it recorded.
/// Only ever asked of a file that is *still open*: with the buffer gone there
/// is nothing to compare against and Varde may not read the disk from here, so
/// a closed file is not claimed to be stale either way. A row that is stale
/// still shows what it recorded — it is the claim that it is current that is
/// dropped, which is what `CONTEXT.md`'s Stale does for a Risk figure.
pub fn stale(state: &State, visit: &Visit) -> bool {
    let Some(buffer) = state
        .buffers
        .iter()
        .find(|(path, _)| crate::relative(state, path) == visit.file)
        .map(|(_, buffer)| buffer)
    else {
        return false;
    };
    line_of(buffer, visit.line).as_deref() != Some(visit.text.as_str())
}

/// The word the cursor was on and the [`WORDS`] - 1 after it, then an ellipsis
/// if the line goes on. A decision, not a rendering detail, which is why it is
/// here under a test rather than in `ui`: how much of a line names it is the
/// whole question this pane answers.
///
/// Leading indentation is dropped — a row thirty columns wide cannot spend
/// eight of them on nothing — and a cursor past the last word falls back to
/// the last one, so a click at the end of a line still names something.
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

/// What kind of jump an event is, decided before `update`'s match consumes it.
///
/// A jump into a *file* carries which one, because the arrival of the history's
/// own jump is the same `Event::BufferOpened` a jump to somewhere new arrives
/// as, and the file is what tells the two apart. It carries the *place* as well
/// where the event has one, because "did this jump go anywhere" cannot be
/// answered without it: a jump into the file already on screen has not moved
/// the cursor at the point this is asked — the landing event carries the place
/// and the arm behind it puts the cursor there.
pub enum Jump {
    ToFile {
        file: String,
        at: Option<Place>,
    },
    InFile,
    /// A jump that carries the place it started from, because the cursor has
    /// already left it by the time the jump is recorded. There is exactly one:
    /// the in-file search moves the cursor to the closest match on every
    /// keystroke of the query, so when Enter lands the cursor stands on the
    /// match and the origin the search was opened from is the only place worth
    /// coming back to.
    From(Visit),
    No,
}

/// Whether an event is a jump, and which kind. One list, never an arm-by-arm
/// rule.
///
/// `Event::JumpTo` is deliberately absent, though it reads like the landing of
/// every long-distance jump. It is not one: `Effect::OpenAt` comes back as a
/// single `Event::BufferOpened` carrying the place, and what is left of
/// `JumpTo` is "put the cursor here" — which the history's own walk uses to
/// move within a file, and which counting as a jump would have the history
/// record every step of its own walk.
///
/// `Event::AcceptFind` is the one jump that carries its own origin. Reading it
/// off the cursor, as every other jump is read, would record the match — which
/// is where the query already put the cursor, so nothing would be recorded at
/// all and `/` would be the one long move with no way back.
///
/// `gg` and `G` are jumps and the arrows are not, which is vim's line exactly.
/// `G` only with no operator waiting: `dG` is a delete, and a delete is not
/// somewhere you went.
pub fn jumped(state: &State, event: &Event) -> Jump {
    match event {
        // Browsing the tree previews each file the highlight lands on, and one
        // preview replacing the last is not a place anybody went: the *first*
        // one leaves the file being read, which is worth coming back to, and
        // every one after that leaves the preview before it — a file the reader
        // saw for one keypress on their way past. Recording those is the
        // keystroke log this module opens by refusing to be, and it evicts real
        // jumps through [`CAP`] while it does it. The `preview` flag is the
        // whole difference between a browse and an open: Enter on the same row
        // arrives as the same event with it false.
        Event::BufferOpened { preview: true, .. } if state.preview == state.current_buffer => {
            Jump::No
        }
        // The files starting restores are not places the reader went — see
        // `State::restoring`.
        Event::BufferOpened { .. } if state.restoring > 0 => Jump::No,
        Event::BufferOpened { path, at, .. } => Jump::ToFile {
            file: crate::relative(state, path),
            at: *at,
        },
        Event::ShowBuffer(path) => Jump::ToFile {
            file: crate::relative(state, path),
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

/// The place being left, recorded once the event that leaves it has been
/// answered. Read off `state` — the state the event arrived at — because a
/// jump's whole value is returning to where you were, not to where you ended
/// up.
///
/// Two things are not recorded. A jump that **went nowhere** — the place left
/// and the place arrived at are the same — because a key the surface in front
/// of you ignores is not somewhere you have been: `G` reaches the buffer in
/// Story view and moves nothing, and a list that recorded it would claim a
/// Visit for every key a read-only surface swallowed. And the arrival of the
/// history's *own* jump: `Effect::OpenAt` comes back as the same
/// `Event::BufferOpened` a jump to somewhere new arrives as, and a list that
/// recorded that would record every step of its own walk *and* move the cursor
/// it was walking, so going back and then forward would land nowhere near where
/// going back started. What tells them apart is that the file arrived at is the
/// one the history's cursor already names and the place being left is the Visit
/// next to it in the list — which is what one step through the list *is*.
///
/// "Went nowhere" is two different questions and that is where this was wrong.
/// For a [`Jump::InFile`] the cursor has already moved by the time this is
/// asked, so the state the event produced answers it. For a [`Jump::ToFile`] it
/// has *not*: the landing carries the place and the arm behind it is what puts
/// the cursor there, so asking the resulting state answers "no" for every jump
/// that lands in the file already on screen — a definition, a search hit, a
/// Risk row in the file you are reading, which is the most common jump there is
/// and the one Varde could not come back from. The place the landing carries is
/// what answers it instead, and a landing with no place at all (a file merely
/// opened, and it is already the one on screen) really did go nowhere.
pub fn record(state: &State, next: &mut State, jump: Jump) {
    let left = match &jump {
        Jump::From(origin) => Some(origin.clone()),
        _ => here(state, None),
    };
    let Some(left) = left else {
        return;
    };
    // Whether the place left is a place you have *been*, which is a different
    // question for each kind of jump — see above.
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

/// Whether a landing in the file already on screen leaves the cursor where it
/// found it. A landing with no place of its own is a file merely opened, which
/// in its own file is nowhere at all. One answer for both ways a landing
/// arrives — as an event and as [`leaving`] — because a second spelling of "did
/// it move" is a second answer to it.
fn went_nowhere(left: &Visit, at: Option<Place>) -> bool {
    match at {
        Some(at) => (at.line, at.column) == (left.line, left.column),
        None => true,
    }
}

/// The place left by a move the core made **in place**, with no landing event
/// of its own to be recorded against. There is exactly one: a definition in the
/// file already open, which R31.12 answers by moving the cursor and opening
/// nothing — so [`jumped`] has nothing to see, the reply arriving as a language
/// server's output rather than as a jump. `gd` onto a name in the file you are
/// reading is the most common back-jump an editor has, and it is the reason this
/// is not simply left to `record`: the alternative was making that reply return
/// `Effect::OpenAt` for a file already open, which is the effect R31.12 exists
/// to say it does not need.
///
/// A definition the cursor is already standing on records nothing, for
/// [`record`]'s went-nowhere reason.
pub fn leaving(state: &mut State, to: Place) {
    let Some(left) = here(state, None) else {
        return;
    };
    if went_nowhere(&left, Some(to)) {
        return;
    }
    remember(state, left);
}

/// A place appended and the history's cursor left past the newest, which is
/// where somebody who has not travelled anywhere stands. A place identical to
/// the newest already recorded is dropped: a list holding the same row twice
/// teaches nothing.
fn remember(next: &mut State, left: Visit) {
    if next.visits.last() == Some(&left) {
        return;
    }
    // A place already in the list is the same place rather than a second one,
    // so the older row goes and this one lands at the end — vim's jumplist
    // rule, and the tail-only comparison above is what it was missing. Without
    // it a bounce between two places fills the pane with the two rows it
    // already had: `G`, `gg`, `G`, `gg` leaves six, and a hundred of those
    // evict every real jump through [`CAP`]. The file, line and column name the
    // place; the excerpt is what the line held at the time and is deliberately
    // not compared, since the same place re-recorded after an edit is still
    // that place.
    next.visits.retain(|visit| {
        (&visit.file, visit.line, visit.column) != (&left.file, left.line, left.column)
    });
    push(next, left);
    next.history_selection = next.visits.len();
}

/// Whether the arrival being recorded is one step of the history's own walk.
/// See [`record`].
fn walking(state: &State, arriving: &str, left: &Visit) -> bool {
    let at = state.history_selection;
    let names_the_file = selected(state).map(|visit| visit.file.as_str()) == Some(arriving);
    let next_along = [at.checked_sub(1), at.checked_add(1)]
        .into_iter()
        .flatten()
        .any(|index| state.visits.get(index) == Some(left));
    names_the_file && next_along
}

/// One step towards the oldest Visit. From the position past the newest — where
/// you are before travelling anywhere — the place standing here is recorded
/// first, so forward can bring you back to it: going back is not a one-way
/// door.
///
/// Three things the straightforward reading of that gets wrong. The target is
/// taken *after* the push, because a full history drops its oldest Visit as the
/// place standing here goes on the end, and every index below the one dropped
/// moves down with it — an index taken before the push named the row just
/// pushed, so the first Ctrl+p at a full history moved nowhere and said nothing.
/// And the list may already end with the place standing here: a Motion records
/// nothing by design, so `k` back onto the row a jump recorded is enough. Then
/// the step is the row *before* it rather than a second copy of it, and with no
/// row before it there is nowhere earlier to go — which is refused out loud,
/// because a gesture that silently does nothing is indistinguishable from a
/// broken key.
///
/// The third is that there may be nowhere to push *from*: `:ga` closes every
/// buffer and leaves the Visits standing, so there is no cursor to record and
/// yet a whole list of places to go back to. Then the step is the newest Visit,
/// with nothing pushed — forward has nowhere to return to because there was
/// nowhere to return *to*, which is the honest answer rather than R34.7's
/// refusal. Refusing there was a refusal in the *middle* of the list, while the
/// pane went on listing the rows and Enter on one went on opening it.
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

/// One step towards the newest Visit. The newest is where you were standing
/// when you first went back, which is what makes forward the way home.
pub fn forward(state: &State, next: State) -> (State, Vec<Effect>) {
    match state.history_selection.checked_add(1) {
        Some(target) if target < state.visits.len() => go_to(state, next, target),
        _ => (next, vec![Effect::Notify("no-later-place")]),
    }
}

/// Going to the Visit the history's cursor names — what `Enter`, the row's one
/// action and a click on it all reach. The history's cursor stands past the
/// newest Visit until something is travelled to, and there it names no row
/// (R34.17), so this is the one route that can be asked for a place nobody
/// chose — refused with its own slug, because "no earlier place" is an answer
/// to a question about going back and nothing here asked to go back.
pub fn go(state: &State, next: State) -> (State, Vec<Effect>) {
    let target = state.history_selection;
    go_to(state, next, target)
}

/// The jump itself. A file already open needs nothing read, so the cursor moves
/// through the same `Event::JumpTo` every other landing uses; a file that is
/// not open is read by the edge, through the same `Effect::OpenAt` a Risk row,
/// a search hit and a definition all return. Neither is recorded — see
/// [`record`].
fn go_to(state: &State, mut next: State, target: usize) -> (State, Vec<Effect>) {
    let Some(visit) = next.visits.get(target).cloned() else {
        return (next, vec![Effect::Notify("no-place-here")]);
    };
    next.history_selection = target;
    let path = state.root.join(&visit.file);
    let at = Place {
        line: visit.line,
        column: visit.column,
    };
    if state.current_buffer.as_deref() == Some(path.as_path()) {
        return crate::update(&next, Event::JumpTo(at));
    }
    (next, vec![Effect::OpenAt { path, at }])
}

/// Where the cursor is now, as a Visit — or, given a place, that place in the
/// buffer on screen, which is what a jump carrying its own origin is recorded
/// from. `None` with no buffer open: nothing to record and nowhere to come back
/// to.
fn here(state: &State, at: Option<Place>) -> Option<Visit> {
    let path = state.current_buffer.as_ref()?;
    let buffer = state.buffers.get(path)?;
    // A Preview reader's cursor is a rendered row, and so is the origin
    // `Event::OpenFind` carries — while a Visit is a source line and column,
    // which is what `line_of` below, [`stale`] and every landing read it as.
    // The crossing is made here, once, rather than left to whoever reads a
    // Visit: a row stored in that field named a line nobody was on, excerpted
    // whatever the source line of the same number happened to hold, and was
    // then claimed to be current because the two agreed. Column one, for the
    // reason every other crossing resets it: the row map carries no source
    // column.
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
        file: crate::relative(state, path),
        line: at.line,
        column: at.column,
        text: line_of(buffer, at.line).unwrap_or_default(),
    })
}

/// The list with a place appended, and the oldest dropped once it is full.
/// Two callers, one answer: an unbounded history is a leak, and a cap spelled
/// twice is a cap one of the two will grow past.
///
/// It does not de-duplicate, because both callers answer that question before
/// they get here and answer it differently. [`record`] drops a place identical
/// to the newest recorded. [`back`], pushing the place standing here so forward
/// can return to it, steps one row further back instead of pushing when the
/// list already ends with that place — which a Motion since it was recorded is
/// enough to make true, so it is not a formality.
fn push(next: &mut State, place: Visit) {
    next.visits.push(place);
    if next.visits.len() > CAP {
        next.visits.remove(0);
    }
}

/// A 1-based line of a buffer as it stands on screen, `trim_end`ed.
fn line_of(buffer: &crate::editor::Buffer, line: usize) -> Option<String> {
    buffer
        .shown()
        .split('\n')
        .nth(line.checked_sub(1)?)
        .map(|text| text.trim_end().to_string())
}

/// The half-typed operator the buffer on screen is holding, or nothing.
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
        // Column 1 is the first word, and the line goes on past three words.
        assert_eq!(excerpt("pub struct Buffer {", 1, 3), "pub struct Buffer...");
        // Mid-line: the word under the cursor leads, and nothing follows the
        // three taken, so nothing is elided.
        assert_eq!(excerpt("pub struct Buffer {", 5, 3), "struct Buffer {");
        // A cursor inside a word names that word, not the next one.
        assert_eq!(excerpt("use std::fmt;", 5, 3), "std::fmt;");
    }

    #[test]
    fn an_excerpt_drops_the_indentation_and_survives_the_edges() {
        // Three words of four, so the fourth is elided and the indentation is
        // not one of the three.
        assert_eq!(excerpt("        let a = 1;", 1, 3), "let a =...");
        // Past the last word: the last word still names the line, because a
        // row that says nothing is worse than a row that says the tail.
        assert_eq!(excerpt("a b", 40, 3), "b");
        assert_eq!(excerpt("", 1, 3), "");
        // A blank line is not a word, so nothing is elided out of nothing.
        assert_eq!(excerpt("     ", 3, 3), "");
    }

    /// The whole rule in one test: a Motion is not a jump, and a click is not
    /// one either. Both are the absences somebody will "fix" later.
    /// The two ends of the file are jumps, and an operator over one is not:
    /// `dG` is a delete, and a delete is not somewhere you went.
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
        // `gg` — the second `g` of the chord, with the operator waiting.
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
