//! Turning a keypress into events.
//!
//! Pure, and still touching no terminal: the edge converts whatever its
//! terminal sent into a lossless [`KeyEvent`] and this decides what it means.
//! For a hosted pane the only decision is whether the key is reserved —
//! everything else is encoded for the child, because sending a key to a child
//! is byte transport rather than a decision. The panes Varde interprets read
//! the code and the modifiers off the same event; there is no narrower key
//! type, because a type with a name for every key the editor cares about has a
//! catch-all for the rest, and a key that reached it was silently dropped.
//!
//! Every bug this module exists to prevent was a routing bug — a key reaching
//! the wrong pane, a modal not claiming what belonged to it, or a modifier
//! discarded on the way in.

use crate::{
    debug, tree, Direction, Event, Find, FindIcon, FindKeys, Modal, Pane, ReplaceField, Resolution,
    Selection, State, Tap, View, FIND_ICONS, REPLACE_FIELDS,
};
use terminput::{
    Encoding, Event as Input, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, ModifierKeyCode,
};

/// Half-typed input the edge collects on the core's behalf: a filename, a `:`
/// command, an AI command, a filter, a comment type. None of it is state the
/// core reasons about, which is why it lives here and not in [`State`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Drafts {
    pub name: String,
    pub command: Option<String>,
    pub ai: String,
    pub filter: Option<String>,
    pub comment_kind: String,
}

/// The reminder in the editor's corner: the keys, and what each one does. A
/// row names the views it applies to, the same way [`UNLISTED`] names the
/// views a binding is excused in — a row that only Edit view's buffer answers
/// must say so, since Review view draws this same table over a read-only
/// diff and a key it ignores is a promise the box should not make. Most-needed
/// first within a view, weighted by how discoverable the key is elsewhere —
/// the box truncates silently from the bottom, so the rows that survive a
/// short window should be the ones nothing else teaches you. That ordering is
/// the whole of what a short window gets, and it is not a preference: Edit view
/// has twenty-five rows and a 26-row terminal — the default height of a macOS
/// Terminal, and the size `AGENTS.md`'s replay recipe uses — draws sixteen of
/// them, so a row placed seventeenth does not exist for whoever has not resized
/// their window. `:format` was placed twenty-third and was therefore a feature
/// nobody could find. `0` and `:w` go near the last because a vim reflex and
/// Home/End already cover them, `C-f` and `D` because the palette draws
/// `(f) Find` and because the `unsaved-changes` and `buffer-diverged` notices
/// name `:w`, `:q!` and `D` in the sentence that reports what they answer — a
/// notice fires exactly when its key becomes the thing to press — and
/// `C-space v` after all of them because the palette it opens prints its own
/// second key. A reflex is not evidence, though: `gg` and `G` sat on [`UNLISTED`]
/// under exactly that excuse until the person who wrote the editor asked for
/// them as though they were not bound, so they are a row now, and high. The two
/// ways out of a hosted pane are here for the opposite reason: nothing else
/// teaches them, and a key nobody can discover is a key nobody uses. The `:`
/// commands that mean the same thing in every view — `:update`, `:tall`,
/// `:help` — are the palette's now, as are the two that act on the whole
/// workspace rather than on one thing, collapsing the tree: a row it spends
/// has to be about the view on screen, and the palette draws its own letters
/// where a cheatsheet row could only repeat them. What is left is what answers to what is in front of
/// you, `:submit` and `:w :q`. `C-space` is the
/// palette gesture on every terminal and in every pane, hosted ones included;
/// `Esc Esc` opens it from a hosted pane too, and is listed beside it because
/// nothing else teaches it. The motion rows are true of a Preview as well as
/// of Source, which the sweep below cannot reach: `h l 0 $ w b e gg G` and the
/// arrows move its cursor through the *rendered* rows (ADR 0007, as amended),
/// and `i` crosses to Source and arrives inserting. Each is the gesture the
/// label already names, so none is spelled twice — and a Preview state added to
/// the sweep would have to answer every other Edit row, which a read-only
/// surface does not. It lives
/// here rather than in the renderer so a test can hold it to the bindings
/// above; `ui` only draws it, filtered to the view on screen.
pub const CHEATSHEET: [(&str, &str, &[View]); 47] = [
    ("i a o O x", "edit", &[View::Edit]),
    ("w b e", "word", &[View::Edit]),
    ("gg G", "file ends", &[View::Edit]),
    ("S-arr W B", "extend", &[View::Edit]),
    // The operator composes with the motions above rather than pairing, so the
    // row spells two of the compositions rather than the rule: `db` because the
    // omissions list points Alt+Backspace's modifier-free route at this row,
    // and `dG` because it is the one the operator was reported broken over.
    // Neither is under the sweep — it builds chords only from keys that do
    // nothing alone, and `G` and `b` both do something — so what holds these
    // two spellings true is `editing_vim.feature`, not the tests below.
    ("dd dG db yy p", "cut yank put", &[View::Edit]),
    // Between the two rows it feeds, because it now feeds both: the lines it
    // picks are the workspace's one Selection, so `C-c` takes them and not just
    // a mouse drag's characters. "select lines" rather than "lines" for the
    // same reason — a mode is something the operators above know about, a
    // selection is something the row below acts on.
    ("V", "select lines", &[View::Edit]),
    // Four spellings of one pair of gestures: Command is an alias for Ctrl on
    // both keys, and a row per spelling would spend four of Edit's sixteen
    // rows saying "copy" twice. Paste rides this row rather than one of its
    // own for the same reason — on a 26-row window a row of its own would push
    // the last row off the bottom.
    (
        "C-c D-c C-v D-v",
        "copy / paste",
        &[View::Edit, View::Review, View::Story],
    ),
    // Tab is the find line's, not the buffer's: it walks from the query onto
    // `[Aa] [replace] [replace all]`, which Right does too at the query's end.
    ("/ n N Tab", "find / its icons", &[View::Edit]),
    ("gt gT", "buffer", &[View::Edit]),
    // Two rows for one gesture, because the two spellings do not reach the
    // same views. The chord needs a buffer to hold the waiting `g`: Review
    // view draws its diff over none, and a Story walk has already spent `g` on
    // jumping to a citation and `p` on stepping — so claiming `gp gn` there
    // printed a promise those two surfaces cannot keep, and the sweep below
    // that holds every chord to its own row is what found it. That leaves the
    // jump reachable only with a modifier on both read-only surfaces, which
    // R31.11 wants said out loud rather than quietly re-added here: it is a
    // spec decision about two views whose keys are already spoken for, not an
    // oversight in this table.
    // Split by view rather than by spelling: Edit gets one row naming both
    // ways, and the read-only surfaces get a row naming only the one that
    // works there. A row per spelling was the first shape and it cost Edit two
    // rows carrying the same label — which on a 26-row window pushed
    // `:format`, whose row is the only place the formatter is spelled at all,
    // off the bottom. One gesture reads as one row.
    ("C-p C-n gp gn", "jump back / forward", &[View::Edit]),
    (
        "C-p C-n",
        "jump back / forward",
        &[View::Review, View::Story],
    ),
    ("* gr", "project", &[View::Edit]),
    ("u U C-z C-S-z", "undo / redo", &[View::Edit]),
    // The second `K` is on the row the first is on: it is the same question
    // read further, and a key nobody can discover is a key nobody uses.
    ("K K", "what is this / read it", &[View::Edit]),
    ("gd", "definition", &[View::Edit]),
    // The fifteenth and sixteenth Edit rows, which is the last two a 26-row
    // terminal has room for, and the two on this table Varde says nowhere else.
    // The palette is the way to eighteen more commands and to every pane;
    // `:format` is in no palette, has no completion on the `:` line and is
    // spelled in no notice, so a reader who cannot see it here cannot find it
    // at all. They are above the rows below for exactly that: what a short
    // window costs should be what something else already teaches.
    (
        "C-space Esc Esc",
        "palette",
        &[View::Edit, View::Review, View::Story],
    ),
    (":format", "lay the file out", &[View::Edit]),
    ("Enter Esc", "candidates", &[View::Edit]),
    // Two meanings under one key, in the order the routing decides them: the
    // tab-stop modal claims Tab while an accepted Candidate has blanks left and
    // lets it through everywhere else, so indenting is what Tab is the rest of
    // the time. The row is earned by "next blank" — nothing teaches it, since
    // the stop sequence draws nothing on screen and, unlike Tools,
    // has no box footer to be discoverable in. "indent" is on it because a row
    // that names one of a key's two meanings says the other is not there.
    // Not an omission instead: [`UNLISTED`] excuses a *key*, and Tab is listed.
    ("Tab", "indent / next blank", &[View::Edit]),
    // Below the fold for the reason the two rows above `:format` are above it:
    // a short window's rows go to what nothing else in Varde teaches, and this
    // one is taught by every editor that has the gesture.
    ("C-d D-d gm", "same word again", &[View::Edit]),
    ("j k V c", "select comment", &[View::Review]),
    // A surface that is read rather than typed in has no column cursor for the
    // view to follow, so the only way to the tail of a long line is a gesture
    // — and one nothing else teaches, since in Edit view these same keys are
    // the cursor motion the omissions list excuses. Not claimed for Edit here,
    // where they move the cursor — in Source and in a Preview alike — which is
    // a different promise.
    ("h l 0", "slide sideways", &[View::Review, View::Story]),
    ("e", "edit the file", &[View::Review, View::Story]),
    (":submit", "send the review", &[View::Review, View::Story]),
    ("n p", "step", &[View::Story]),
    ("j k", "scroll", &[View::Story]),
    ("d", "show the diff", &[View::Story]),
    ("D", "step detail", &[View::Story]),
    ("g", "jump to citation", &[View::Story]),
    ("c", "comment", &[View::Story]),
    ("Esc", "back to spine", &[View::Story]),
    ("t", "spine / files", &[View::Story]),
    (
        "M-h M-j M-k M-l",
        "focus",
        &[View::Edit, View::Review, View::Story],
    ),
    ("0 $", "line ends", &[View::Edit]),
    (":preview", "read / edit markdown", &[View::Edit]),
    // Below the fold with `:preview`, and beside it because they are the two
    // things a markdown buffer can do that no other buffer can. One row for
    // both spellings: a reader who cannot find the one that starts a Reading
    // has no use for the one that ends it.
    (
        ":read :pause :next :prev :stop :speed",
        "read the selection aloud",
        &[View::Edit],
    ),
    (":dim", "darker editor", &[View::Edit]),
    (":minimap", "mirror of the file", &[View::Edit]),
    // The key alone where the chord cannot follow: a Space chord needs Edit
    // view's buffer, so the read-only surfaces get a row naming only the
    // spelling that works there — split by view for the reason
    // `C-p C-n gp gn` is, and the chord's own row is in [`CHORDS`].
    ("C-F5", "restart debugging", &[View::Review, View::Story]),
    // Said again where the reader is already looking, which is what puts them
    // here rather than above the fold: the palette draws `(f) Find`, and the
    // `buffer-diverged` and `unsaved-changes` notices name `D`, `:w`, `:e` and
    // `:q!` in the sentence that reports the problem each one answers. A notice
    // fires exactly when its key becomes the thing to press, which is a better
    // teacher than a standing row — the row is here so the key is on the
    // contract, not because this is where anybody learns it.
    (
        "C-f D-f",
        "project",
        &[View::Edit, View::Review, View::Story],
    ),
    ("D", "diverged from disk", &[View::Edit]),
    // The write key rides `:w`'s row rather than one of its own: Edit's rows
    // are at the limit a 26-row window has, and it is the same write.
    (":w C-s D-s :q :qa", "write quit close all", &[View::Edit]),
    // The gesture that opens the branch picker, which is what earns a row here
    // rather than the two keys the picker itself answers — those are
    // [`BRANCH_LIST_KEYS`], drawn in the box while it is up. Both spellings on
    // one row, because a reviewer who cannot find the one that lists branches
    // cannot find the one that stories the change either: `:story` had no row
    // at all, and `gt` is what a binding nobody printed costs.
    (
        ":story? :story",
        "story a branch / this change",
        &[View::Edit, View::Story],
    ),
    // The palette's second face, spelled as the gesture it is: the letter alone
    // means nothing outside the open palette, and a row nobody can perform from
    // where the box is drawn teaches the wrong key.
    (
        "C-space v",
        "tools",
        &[View::Edit, View::Review, View::Story],
    ),
    // Inside a Conflict: current, incoming, both. Low, because a merge is
    // the rare afternoon and the rows above are every day's.
    ("cc ci cb", "accept conflict side", &[View::Edit]),
    // The keys the Diagnostic list answers with the keyboard in it, from
    // whichever view it was opened over. Last, because the palette's `i` that
    // opens it is drawn in the palette itself, and a short window's rows go to
    // what nothing else teaches.
    (
        "e w i h j k Enter",
        "diagnostic list: severity, row, go",
        &[View::Edit, View::Review, View::Story],
    ),
    // The Conflict list's, last for the Diagnostic list's reason.
    (
        "j k Enter",
        "conflict list: row, go",
        &[View::Edit, View::Review, View::Story],
    ),
];

/// What a tapped Space can be followed by whatever else is going on — the
/// chords that need no Debug session. With [`DEBUG_CHORDS`] it is the one list
/// both the Chord hint and the cheatsheet draw, so the two cannot disagree:
/// every row spells its chord, and the hint reads the key off that spelling —
/// beside which a row may name the other ways to the same thing. Space is
/// shared — other features may claim other letters later.
pub const CHORDS: [(&str, &str, &[View]); 4] = [
    ("␣b", "breakpoint", &[View::Edit]),
    // The Breakpoint key's shifted letter, as `D` is the list's `d`.
    ("␣B", "breakpoint properties", &[View::Edit]),
    // What a click on the line's ▶ offers, from the keyboard.
    ("␣x", "run / debug this line", &[View::Edit]),
    // Here rather than in [`DEBUG_CHORDS`] because it is the one debug chord
    // that answers with no session: rerunning the last one is what it is for.
    // Both spellings on one row, the way `C-p C-n gp gn` carries two: the key
    // and the chord are one gesture, and a row apiece would say "restart"
    // twice in a box that truncates from the bottom. [`chord_rows`] reads the
    // hint's letter off the token spelled with the Space glyph, so a row may
    // name the other ways to the same thing.
    ("C-F5 ␣r", "restart debugging", &[View::Edit]),
];

/// The chords a Debug session answers, offered only while one exists for the
/// reason [`DEBUG_KEYS`] are listed only while one does: with no session they
/// do nothing, and a hint offering them would teach keys that are not there.
pub const DEBUG_CHORDS: [(&str, &str, &[View]); 9] = [
    ("␣n", "step over", &[View::Edit]),
    ("␣i", "step into", &[View::Edit]),
    ("␣o", "step out", &[View::Edit]),
    ("␣c", "continue / pause", &[View::Edit]),
    ("␣q", "stop debugging", &[View::Edit]),
    ("␣s", "switch the Strip's group", &[View::Edit]),
    ("␣h", "hide / show the Program output", &[View::Edit]),
    // Both spellings on one row, the way restart carries two: Ctrl+Enter runs
    // what the chord opens, and `chord_rows` reads the hint's letter off the
    // token spelled with the Space glyph.
    ("␣e C-Enter", "evaluate", &[View::Edit]),
    ("␣a", "ask the AI about the pause", &[View::Edit]),
];

/// The chords the Evaluator's window answers, offered only while one is open
/// for the reason the debug chords are offered only while a session exists: a
/// hint offering a window that is not there teaches a key that does nothing.
///
/// Both open a mode, which is what makes moving and resizing a window
/// reachable with no modifier at all — the rule every binding in Varde is
/// held to, and the one a window dragged by Alt-arrow would quietly fail for
/// whoever has not configured their terminal. What the letters do inside the
/// mode is `h j k l` and the arrows, for the reason Stepping mode's letters
/// are the chords' own: the gesture is the editor's.
pub const EVALUATOR_CHORDS: [(&str, &str, &[View]); 2] = [
    ("␣m", "move the Evaluator (hjkl)", &[View::Edit]),
    ("␣z", "resize the Evaluator (hjkl)", &[View::Edit]),
];

/// The keys a Debug session reserves, listed while one exists and absent while
/// none does — which is when the hosted panes have them back.
pub const DEBUG_KEYS: [(&str, &str, &[View]); 6] = [
    ("F9", "continue / pause", &[View::Edit]),
    ("F8", "step over", &[View::Edit]),
    ("F7", "step into", &[View::Edit]),
    ("S-F8", "step out", &[View::Edit]),
    ("C-F8", "toggle breakpoint", &[View::Edit]),
    ("C-F2", "stop debugging", &[View::Edit]),
];

/// The chords on offer in `state`: the ones that need a Debug session, only
/// while one exists. One answer, read by the Chord hint, by the cheatsheet and
/// through them by the mouse's hit-test, so the three cannot disagree about
/// what a waiting Space is waiting for.
///
/// The debug chords first, for the reason [`DEBUG_KEYS`] come first in the
/// cheatsheet: while a session exists they are the keys being reached for.
fn chords(
    state: &State,
) -> impl Iterator<Item = &'static (&'static str, &'static str, &'static [View])> {
    let debug: &'static [(&str, &str, &[View])] = match state.debug {
        Some(_) => &DEBUG_CHORDS,
        None => &[],
    };
    let evaluator: &'static [(&str, &str, &[View])] = match arranging_offered(state) {
        true => &EVALUATOR_CHORDS,
        false => &[],
    };
    debug.iter().chain(evaluator.iter()).chain(CHORDS.iter())
}

/// The cheatsheet as drawn: the debug keys while a session exists — first,
/// because while one does they are the keys being reached for — then
/// [`CHEATSHEET`] and the chords.
pub fn cheatsheet(
    state: &State,
) -> impl Iterator<Item = &'static (&'static str, &'static str, &'static [View])> {
    let debug: &'static [(&str, &str, &[View])] = match state.debug {
        Some(_) => &DEBUG_KEYS,
        None => &[],
    };
    debug.iter().chain(CHEATSHEET.iter()).chain(chords(state))
}

/// The Cheatsheet's rows for the view on screen: what `ui` draws and what
/// `update` clamps the offset against, so the two count the same rows.
/// Review's rows answer to `state.diff`, so a review whose diff has not
/// landed yet lists none of them.
pub fn cheatsheet_rows(state: &State) -> Vec<(&'static str, &'static str)> {
    if state.view == View::Review && state.diff.is_none() {
        return vec![];
    }
    cheatsheet(state)
        .filter(|(_, _, views)| applies_to(views, state.view))
        .map(|(keys, what, _)| (*keys, *what))
        .collect()
}

/// The Chord hint as drawn, each row carrying the key it offers or nothing —
/// the shape [`crate::palette_rows`] has, so the mouse hit-tests these same
/// rows and a click is the keystroke.
pub fn chord_rows(state: &State) -> Vec<(Option<char>, String)> {
    let mut rows: Vec<(Option<char>, String)> = chords(state)
        .filter_map(|(keys, what, _)| {
            // The token spelled with the Space glyph, since a row may name the
            // key that does the same thing beside its chord.
            let key = keys
                .split_whitespace()
                .find_map(|token| token.strip_prefix('\u{2423}'))?
                .chars()
                .next()?;
            Some((Some(key), format!("   ({key}) {what}")))
        })
        .collect();
    rows.push((None, "   Esc  cancel".to_string()));
    rows
}

/// The keys Tools answers and the word the box says for each — here,
/// beside the router that answers them, for the reason [`CHEATSHEET`] is here:
/// `ui` may only draw the contract, so a test can hold the two together. They
/// are not cheatsheet rows because they exist only while the list is up, and
/// a cheatsheet row is spent on the view on screen — the gesture
/// that opens the list is what earns a row there. The arrows are deliberately
/// absent for the reason `UNLISTED` gives for them everywhere else: every list
/// in Varde moves on them, and the Cheatsheet already cannot spell one label twice.
pub const TOOL_LIST_KEYS: [(&str, &str); 3] =
    [("i", "install"), ("r", "re-check"), ("Esc", "close")];

/// The keys the branch picker answers and the word its box says for each —
/// here, beside the router that answers them, for the reason
/// [`TOOL_LIST_KEYS`] is here. Not cheatsheet rows for the same reason
/// either: the list exists only while it is up.
///
/// The arrows are deliberately absent, as they are for Tools: every
/// list in Varde moves on them, and the box cannot spell one label twice. Enter
/// is what a picker is for, and Escape is how every box in Varde is left — both
/// reachable with no modifier (R31.11).
pub const BRANCH_LIST_KEYS: [(&str, &str); 2] = [("Enter", "story this branch"), ("Esc", "close")];

/// The keys the launch list answers and the word its box says for each, for
/// the reason [`BRANCH_LIST_KEYS`] is here.
pub const LAUNCH_LIST_KEYS: [(&str, &str); 2] = [("Enter", "start"), ("Esc", "close")];

/// What the picker's box says about being typed into, here rather than in `ui`
/// for the reason [`BRANCH_LIST_KEYS`] is: `ui` may only draw the contract, so
/// a test can hold the words against the router that makes them true.
///
/// Not a [`BRANCH_LIST_KEYS`] row, because every row there is spelled as a key
/// the sweep can press and no key spells "type" — a list narrowed by letters is
/// text, not a gesture, which is why the comment box's letters have no footer
/// row either. Backspace is part of the same gesture and named by the same
/// line: a box that spelled it separately would be spelling half of typing.
pub const BRANCH_FILTER_HINT: &str = "type to filter";

/// The keys the comment box's body answers and the word its footer says for
/// each — here, beside the router that answers them, for the reason
/// [`TOOL_LIST_KEYS`] is here: `ui` may only draw the contract, so a test can
/// hold the two together rather than trusting whoever remembers to edit two
/// files.
///
/// Least guessable first: Enter is a newline in a text box and Escape discards
/// in every box Varde has, and `C-z` is the undo of every editor outside vim,
/// so filing is the one key a reviewer cannot guess. Both Ctrl keys carry a
/// modifier for the same reason — every letter in the body is text, so there is
/// no unmodified key left to give them, and `u` is a letter of the comment
/// rather than the undo it is in a buffer. That is what earns them a footer
/// rather than a cheatsheet row: the box exists only while it is up, and the
/// gesture that opens it (`c`, or a gutter drag) is what the cheatsheet spends
/// a row on.
pub const COMMENT_BOX_KEYS: [(&str, &str); 4] = [
    ("C-s", "file"),
    ("Esc", "discard"),
    ("C-z", "undo"),
    ("C-S-z", "redo"),
];

/// The keys the replace box answers and the word it says for each, drawn in
/// its bottom border for the reason [`COMMENT_BOX_KEYS`] is: the box exists
/// only while it is up, and `[replace]` on the `/` line is what opens it.
/// Enter replaces from either field or `[replace]`, and replaces every match
/// from `[replace all]`.
pub const REPLACE_BOX_KEYS: [(&str, &str); 3] =
    [("Tab", "next"), ("Enter", "replace"), ("Esc", "close")];

/// The keys the results box answers and the word it says for each — here for
/// the reason [`TOOL_LIST_KEYS`] is here: the box exists only while a search
/// is up, so the gesture that opens it is what earns a cheatsheet row, and the
/// keys inside it are discoverable in the box itself. `ui` may only draw this,
/// so a test can hold the two together rather than trusting whoever remembers
/// to edit two files.
///
/// Movement first, because the row truncates from the right on a narrow
/// terminal and the keys that get somebody through a long list are the ones
/// nothing else teaches. The arrows are named here — unlike every other list
/// in Varde, where they are the motion too obvious to spend a row on — because
/// they and `C-n C-p` move by different things, and the box is where that
/// difference has to be readable.
pub const SEARCH_KEYS: [(&str, &str); 6] = [
    ("arr", "hit"),
    ("C-n C-p", "file"),
    ("Enter", "open"),
    ("C-o", "open all"),
    ("Tab", "complete"),
    ("Esc", "close"),
];

/// Whether a row of [`CHEATSHEET`] or [`UNLISTED`] names this view. Pulled out
/// once a third call site needed it — the sweep's two omissions checks and
/// `ui`'s own filter — per the rule of three.
pub fn applies_to(views: &[View], view: View) -> bool {
    views.contains(&view)
}

/// What a key means, given what is on screen and what has focus. The one way in
/// from the edge, and the only place a keypress is interpreted.
///
/// A hosted pane — the AI pane and the terminal pane — is a terminal Varde
/// hosts rather than a pane it interprets: its child owns the keyboard, so
/// Varde claims the reserved keys and encodes everything else for the child.
/// Every other pane routes against what is on screen. `at_ms` stamps the
/// double-tap; the edge supplies its clock.
pub fn on_key_event(state: &State, drafts: &mut Drafts, event: KeyEvent, at_ms: u64) -> Vec<Event> {
    if let Some(events) = reserved(state, drafts, event, at_ms) {
        return events;
    }
    let event = shifted(event);
    // Stepping mode, ahead of every arm below: the letters a chord just used
    // act without their Space, and any other key leaves the mode and then does
    // what it always does, so nobody is ever trapped in it. Leaving is an event
    // of its own queued in front of the key's own, because only `update` may
    // write the flag.
    let leaving = match state.stepping {
        false => None,
        true => match stepping_key(event) {
            Some(stepped) => return vec![stepped],
            None => Some(Event::LeaveStepping),
        },
    };
    // The Evaluator's arrange mode, on Stepping mode's own terms and for its
    // reason: the letters act without their Space while somebody is placing
    // the window, and any other key leaves the mode and then does what it
    // always does.
    let leaving_arrange = match state.arranging {
        None => None,
        Some(how) => match arranging_key(event, how) {
            Some(arranged) => return vec![arranged],
            None => Some(Event::LeaveArranging),
        },
    };
    let mut events = match claimed_everywhere(state, event) {
        Some(events) => events,
        None => modal_key(state, drafts, event),
    };
    for leave in leaving_arrange.into_iter().chain(leaving) {
        events.insert(0, leave);
    }
    events
}

/// What a key does in Stepping mode, or nothing for one the mode does not
/// claim — which is every key but the four [`stepping_letter`] names.
///
/// The `every_key` sweep cannot drive this mode, and the omission is a
/// decision rather than an oversight: in Stepping mode *every* key answers,
/// because a key the mode does not claim answers by leaving it, so a sweep
/// driven here would hold all sixty-four modifier combinations of every code
/// to a cheatsheet row. What the box promises about these four is their
/// chords, `DEBUG_CHORDS`, which the sweep does hold; that they do the same
/// thing without the Space is
/// `stepping_mode_claims_its_letters_and_hands_every_other_key_back`.
fn stepping_key(event: KeyEvent) -> Option<Event> {
    if !event.modifiers.is_empty() {
        return None;
    }
    stepping_letter(typed(event)?)
}

/// What a chord's second key does, given where the keyboard is: the one table
/// the hint's rows are answered from, so the letters the Chord hint offers and
/// the letters something happens for cannot part ways. Here rather than in
/// `update` because it is a key being interpreted, and a key nobody bound
/// answers with nothing.
pub fn chord(state: &State, key: char) -> Option<Event> {
    match key {
        // The line the caret is on, since the chord is pressed while reading
        // it. No buffer is line 0, which owns no Breakpoint.
        'b' => Some(Event::ToggleBreakpoint(
            crate::current_buffer(state).map_or(0, |buffer| buffer.line),
        )),
        'B' => Some(Event::EditBreakpoint(
            crate::current_buffer(state).map_or(0, |buffer| buffer.line),
        )),
        'x' => Some(Event::OfferRun(
            crate::current_buffer(state).map_or(0, |buffer| buffer.line),
        )),
        'q' => Some(Event::DebugStop),
        'e' if state.debug.is_some() => Some(Event::OpenEvaluator),
        // The two that open a mode rather than acting: the window is moved
        // and resized a cell at a time, and a chord per cell is a chord too
        // many — Stepping mode's reason, one window over.
        'm' if arranging_offered(state) => {
            Some(Event::ArrangeEvaluator(crate::debug::Arrange::Moving))
        }
        'z' if arranging_offered(state) => {
            Some(Event::ArrangeEvaluator(crate::debug::Arrange::Sizing))
        }
        'r' => Some(Event::DebugRestart),
        // The Group tab from the keyboard: whichever group the Strip is not
        // showing, since there are two and the gesture is "the other one".
        // Hiding the Program output and showing it again are one gesture, so
        // one letter: what pressing it does is whichever the reader can see.
        'h' if state.debug.is_some() => Some(Event::ToggleOutput),
        'a' if state.debug.is_some() => Some(Event::AskAboutPause),
        's' if state.debug.is_some() => Some(Event::ShowGroup(match state.strip {
            crate::layout::Group::Shells => crate::layout::Group::Debug,
            crate::layout::Group::Debug => crate::layout::Group::Shells,
        })),
        letter => stepping_letter(letter),
    }
}

/// Whether the two chords that arrange the Evaluator's window are on offer:
/// its window is up and the keyboard is in it. One answer for the hint, the
/// cheatsheet and the router, for the reason the debug chords have one — a
/// key the hint names and nothing answers is a key that lies. Not merely open:
/// with the keyboard back in the editor the reader is editing the file, and
/// `m` there is the motion it always was.
fn arranging_offered(state: &State) -> bool {
    state.evaluator.is_some() && state.focus == Pane::Evaluator
}

/// What a key does while the Evaluator's window is being arranged, or nothing
/// for one the mode does not claim — which is what leaves it, exactly as
/// Stepping mode is left, so nobody is ever trapped in a mode.
///
/// `h j k l` and the arrows beside them: moving a window and moving a caret
/// are the same gesture, and every motion in Varde is reachable without a
/// modifier. Resizing reads them the same way round — `l` widens and `j`
/// heightens — because the edge being moved is the bottom right one.
///
/// The `every_key` sweep cannot drive this mode, and the omission is the
/// decision [`stepping_key`] documents: every key answers here, because one
/// the mode does not claim answers by leaving it. What the cheatsheet
/// promises is the two chords that open it, [`EVALUATOR_CHORDS`], which the
/// sweep does hold, and the letters are named in the row beside them.
fn arranging_key(event: KeyEvent, how: crate::debug::Arrange) -> Option<Event> {
    if !event.modifiers.is_empty() {
        return None;
    }
    let direction = arrow(event.code).or_else(|| match typed(event) {
        Some('h') => Some(Direction::Left),
        Some('j') => Some(Direction::Down),
        Some('k') => Some(Direction::Up),
        Some('l') => Some(Direction::Right),
        _ => None,
    })?;
    Some(match how {
        crate::debug::Arrange::Moving => Event::MoveEvaluator(direction),
        crate::debug::Arrange::Sizing => Event::ResizeEvaluator(direction),
    })
}

/// The four letters Stepping mode is for, which are also four of the chords.
/// `q` is not among them, though its chord is: stopping is not a step, and a
/// session ended by a stray letter is one nothing can bring back.
fn stepping_letter(key: char) -> Option<Event> {
    match key {
        'n' => Some(Event::DebugStep(crate::debug::Step::Over)),
        'i' => Some(Event::DebugStep(crate::debug::Step::Into)),
        'o' => Some(Event::DebugStep(crate::debug::Step::Out)),
        'c' => Some(Event::DebugResume),
        _ => None,
    }
}

/// The keys claimed before the hosted-pane split, so they mean the same thing
/// in every pane.
fn reserved(state: &State, drafts: &mut Drafts, event: KeyEvent, at_ms: u64) -> Option<Vec<Event>> {
    // A lone Ctrl means the same thing in every pane, so it is claimed before
    // the split: double-tapped it opens the palette, and a lone modifier
    // produces no bytes in a pty, so a hosted pane's child could never have
    // received it. This is the reserved key that makes Option-less terminals
    // survivable.
    if let KeyCode::Modifier(ModifierKeyCode::Control, _) = event.code {
        return Some(vec![Event::Tapped {
            key: Tap::Ctrl,
            at_ms,
        }]);
    }
    // The one gesture that opens the palette, claimed before the split so it
    // means the same thing in every pane. It used to be claimed after it, which
    // left the two panes that host a child — the only panes you can be stuck in
    // — reaching it by a double-tap instead: bare Ctrl where the terminal
    // reported one, Escape where it did not. Neither survived: the bare Ctrl
    // press needs a keyboard flag that costs every composed character (see
    // `main.rs`), and a double-tap is a window, so it misfires. One key, no
    // window, no flag, no terminal to configure. The child pays a single NUL
    // byte for it, which is `RESERVED`'s entry below.
    if event.modifiers.contains(KeyModifiers::CTRL) && event.code == KeyCode::Char(' ') {
        return Some(vec![Event::FallbackBinding]);
    }
    // JetBrains' debugger keys, from every pane — a shell's included, since
    // stepping happens in bursts from wherever the keyboard is. Reserved only
    // while a session exists: with none, the child in a hosted pane gets them.
    if state.debug.is_some() {
        let ctrl = event.modifiers.contains(KeyModifiers::CTRL);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        let stepped = |step| Some(vec![Event::DebugStep(step)]);
        match event.code {
            KeyCode::F(9) if !ctrl => return Some(vec![Event::DebugResume]),
            KeyCode::F(2) if ctrl => return Some(vec![Event::DebugStop]),
            KeyCode::F(8) if ctrl => {
                let line = crate::current_buffer(state).map_or(0, |buffer| buffer.line);
                return Some(vec![Event::ToggleBreakpoint(line)]);
            }
            // Shift is a gesture of its own here, which is why `label` spells
            // it: stepping out is the same key as stepping over, held.
            KeyCode::F(8) if !ctrl && shift => return stepped(crate::debug::Step::Out),
            KeyCode::F(8) if !ctrl => return stepped(crate::debug::Step::Over),
            KeyCode::F(7) if !ctrl => return stepped(crate::debug::Step::Into),
            _ => {}
        }
    }
    if child_owns_keys(state, drafts) {
        let mut events = to_child(state, event, at_ms);
        // A child owns its letters, so a key arriving here leaves Stepping
        // mode rather than being claimed by it. The mode is only enterable
        // from a pane Varde interprets, so this is the one way out of it once
        // a click has put the keyboard in a shell — without it the mode, and
        // the title saying so, would outlive every key that could end it.
        if state.stepping {
            events.insert(0, Event::LeaveStepping);
        }
        return Some(events);
    }
    None
}

/// The keys Varde answers whatever is on screen, once the child has not taken
/// them.
fn claimed_everywhere(state: &State, event: KeyEvent) -> Option<Vec<Event>> {
    if event.modifiers.contains(KeyModifiers::SUPER) && event.code == KeyCode::Char('f') {
        return Some(vec![Event::OpenSearch]);
    }
    if event.modifiers.contains(KeyModifiers::CTRL) {
        match event.code {
            KeyCode::Char('q') => return Some(vec![Event::Quit]),
            KeyCode::Char('f') => return Some(vec![Event::OpenSearch]),
            // The one debug key that answers with no session — rerunning the
            // last one is what it is for. So it is claimed here rather than
            // beside the reserved F-keys above, and a hosted pane's child
            // keeps it whether or not a session exists, and every view
            // answers it the way `C-q` and `C-f` above are answered anywhere.
            KeyCode::F(5) => return Some(vec![Event::DebugRestart]),
            _ => {}
        }
    }
    // While search is showing it claims the keys: every printable one belongs
    // to the query, so the actions take a modifier.
    state.search.as_ref()?;
    Some(searching(event))
}

fn modal_key(state: &State, drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match &state.modal {
        Modal::Palette => match event.code {
            KeyCode::Esc => vec![Event::Cancel],
            _ => match typed(event) {
                Some(c) => vec![Event::Key(c)],
                None => vec![],
            },
        },
        // A list, so it swallows what it does not recognise — the same as
        // every modal but the candidate list, which is offered while typing.
        // The arrows move the selection, as they do in every list in Varde, and
        // `i` acts on the row they left it on. Both are reachable with no
        // modifier (R31.11), and the box draws its own footer row naming them,
        // which is where a key that only exists inside this list is
        // discoverable — the cheatsheet spends its row on the gesture that
        // opens it.
        Modal::Tools { .. } => match event.code {
            KeyCode::Esc => vec![Event::Cancel],
            KeyCode::Up => vec![Event::MoveToolRow(Direction::Up)],
            KeyCode::Down => vec![Event::MoveToolRow(Direction::Down)],
            _ => match typed(event) {
                Some('i') => vec![Event::InstallTool],
                Some('r') => vec![Event::RecheckTool],
                _ => vec![],
            },
        },
        // Tools' shape, with Enter as what the list is for.
        Modal::Launches { row } => match event.code {
            KeyCode::Esc => vec![Event::Cancel],
            KeyCode::Up => vec![Event::MoveLaunchRow(Direction::Up)],
            KeyCode::Down => vec![Event::MoveLaunchRow(Direction::Down)],
            KeyCode::Enter => crate::debug::launches(state)
                .get(*row)
                .map(|name| Event::StartLaunch(name.to_string()))
                .into_iter()
                .collect(),
            _ => vec![],
        },
        // A list that is also typed into: four hundred branches is a modal
        // nobody can walk, so a letter narrows it rather than being swallowed.
        // The text is the modal's, not a draft's — unlike the tree's filter box,
        // which is open over a pane the core knows nothing about — so a
        // keystroke is appended to what the picker already holds.
        Modal::Branches { filter, .. } => match event.code {
            KeyCode::Esc => vec![Event::Cancel],
            KeyCode::Up => vec![Event::MoveBranchRow(Direction::Up)],
            KeyCode::Down => vec![Event::MoveBranchRow(Direction::Down)],
            KeyCode::Enter => vec![Event::ChooseBranch],
            KeyCode::Backspace => {
                let mut text = filter.clone();
                text.pop();
                vec![Event::FilterBranches(text)]
            }
            _ => match typed(event) {
                Some(c) => vec![Event::FilterBranches(format!("{filter}{c}"))],
                None => vec![],
            },
        },
        // Every key takes the hint down; a letter is also the chord's second
        // key, and one nobody bound does nothing else. Command on a letter is
        // Command's gesture rather than the chord's, so it takes the hint down
        // like any other key: `D-c` is copy everywhere else in Varde, and
        // reading it as the `c` chord would continue a paused program because
        // somebody reached for the clipboard.
        Modal::Chord => {
            match typed(event).filter(|_| !event.modifiers.contains(KeyModifiers::SUPER)) {
                Some(c) => vec![Event::Key(c)],
                None => vec![Event::Cancel],
            }
        }
        Modal::Comment => comment_picker(drafts, event),
        // The same box, three uses: a file's name, a Variables row's new
        // value and a new Watch. One routing arm because the typing is
        // identical — Enter is where they part, and that is `update`'s to
        // tell from the modal it is in.
        Modal::NameBox { .. } | Modal::SetValue | Modal::NewWatch | Modal::ExceptionClass => {
            name_box(drafts, event)
        }
        Modal::Breakpoint { field, draft, .. } => breakpoint_box(*field, draft, event),
        Modal::Candidates(_) => candidate_list(state, drafts, event),
        // Two keys, and everything else goes on to the buffer: typing at a tab
        // stop is ordinary typing, so this passes keys through for the same
        // reason the candidate list does. Tab is the whole gesture and no
        // modifier is part of it, so a modifier it never inspects is folded
        // into the key it triggers.
        Modal::Stops { .. } => match event.code {
            KeyCode::Tab => vec![Event::NextStop],
            // The stops go and the text stays exactly as it was inserted —
            // abandoning a completion is not undoing it. `Cancel` and not
            // `EditorEscape`, so leaving the sequence is not also leaving
            // insert mode.
            KeyCode::Esc => vec![Event::Cancel],
            _ => routed(state, drafts, event),
        },
        // The modals that are answered rather than typed through.
        Modal::ConfirmSubmit
        | Modal::Diverged
        | Modal::StepDetail
        | Modal::ConfirmStory { .. }
        | Modal::Prediction { .. }
        | Modal::Restart
        | Modal::RunMark { .. } => answered(&state.modal, event),
        // The keyboard in a Hover reads it and nothing else: a letter that
        // reached the buffer would edit code the box is covering.
        Modal::None if state.hover.as_ref().is_some_and(|hover| hover.focused) => {
            match (event.code, typed(event)) {
                (KeyCode::Esc, _) => vec![Event::Cancel],
                (_, Some('j')) => vec![Event::ScrollHover(Direction::Down)],
                (_, Some('k')) => vec![Event::ScrollHover(Direction::Up)],
                _ => vec![],
            }
        }
        Modal::None => routed(state, drafts, event),
    }
}

/// The candidate list claims four keys and passes everything else on. Every
/// other modal swallows what it does not recognise, because it is a question;
/// this is a list offered *while typing*, so a letter it did not claim is a
/// letter that must still reach the buffer — and the next keystroke is
/// interpreted against the state this one left, which is what makes typing
/// through it work at all.
///
/// All four are reachable with no modifier: Option is not Alt on macOS unless
/// the terminal is told so, and a stock tmux strips the modifier reports, so a
/// modifier-only binding is a binding that silently does not exist. Enter and
/// Escape are the row [`CHEATSHEET`] spends on the list; the arrows are on the
/// omissions list beside it, because the box already cannot spell one label
/// twice and every list in Varde answers them.
fn candidate_list(state: &State, drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    // Modifiers are left to the buffer: Shift+Down extends a selection and
    // Alt+Down is a word motion, and neither is a choice in this list.
    if !event.modifiers.is_empty() {
        return routed(state, drafts, event);
    }
    match event.code {
        KeyCode::Up => vec![Event::MoveCandidate(Direction::Up)],
        KeyCode::Down => vec![Event::MoveCandidate(Direction::Down)],
        KeyCode::Enter => vec![Event::AcceptCandidate],
        // The list goes and the text stays exactly as it was typed. `Cancel`
        // and not `EditorEscape`: leaving insert mode as well would make
        // dismissing a list something the reader has to recover from.
        KeyCode::Esc => vec![Event::Cancel],
        _ => routed(state, drafts, event),
    }
}

/// A question is answered, not typed through: an unrelated key leaves it
/// standing rather than deciding it either way.
fn answered(modal: &Modal, event: KeyEvent) -> Vec<Event> {
    match modal {
        Modal::ConfirmSubmit => yes_no(Event::ConfirmSubmit, event),
        Modal::Diverged => diverged_answer(event),
        Modal::StepDetail => step_detail_answer(event),
        Modal::ConfirmStory { .. } => yes_no(Event::ConfirmStory, event),
        Modal::Prediction { .. } => prediction_answer(event),
        Modal::Restart => yes_no(Event::Restart, event),
        Modal::RunMark { .. } => run_mark_answer(event),
        Modal::None
        | Modal::NameBox { .. }
        | Modal::SetValue
        | Modal::NewWatch
        | Modal::ExceptionClass
        | Modal::Breakpoint { .. }
        | Modal::Palette
        | Modal::Chord
        | Modal::Tools { .. }
        | Modal::Launches { .. }
        | Modal::Branches { .. }
        | Modal::Comment
        | Modal::Candidates(_)
        | Modal::Stops { .. } => {
            unreachable!("{modal:?} is not answered")
        }
    }
}

/// Yes or no, spelled the two ways every one of them is reachable: Enter or
/// `y` for the act, Escape or `n` for the refusal, and an unrelated key leaves
/// the question standing rather than deciding it either way. Three questions
/// answer exactly this — submitting a review, authoring a story, and the
/// restart — which is the third duplicate AGENTS.md's rule of three waits for.
fn yes_no(yes: Event, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Enter => vec![yes],
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some('y' | 'Y') => vec![yes],
            Some('n' | 'N') => vec![Event::Cancel],
            _ => vec![],
        },
    }
}

/// Three named answers: an unrelated key leaves the divergence standing, which
/// is the one outcome that loses nothing.
fn diverged_answer(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some('r') => vec![Event::Resolve(Resolution::Reload)],
            Some('w') => vec![Event::Resolve(Resolution::Overwrite)],
            Some('m') => vec![Event::Resolve(Resolution::Merge)],
            _ => vec![],
        },
    }
}

/// `D` toggled it open, and toggles it shut again: the overlay's own "why" and
/// flow are read straight from state, never typed here.
fn step_detail_answer(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some('D') => vec![Event::Key('D')],
            _ => vec![],
        },
    }
}

/// The two Chips' keys, and Escape for neither.
fn run_mark_answer(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some('r') => vec![Event::ChooseRun(crate::run::RUN)],
            Some('d') => vec![Event::ChooseRun(crate::run::DEBUG)],
            _ => vec![],
        },
    }
}

/// A pick is typed through as a digit; `n`/`p` still step the walk, since
/// answering never blocks it. Everything else is read straight from the Step's
/// own Prediction, never typed here.
fn prediction_answer(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        _ => match typed(event) {
            Some(c @ ('n' | 'p' | '1' | '2' | '3')) => vec![Event::Key(c)],
            _ => vec![],
        },
    }
}

/// Whether the child asked to be told that a paste is a paste — DEC mode 2004,
/// which the terminal model in front of each pty reports. A child that never
/// asked reads pasted text as if it had been typed, which is what a real
/// terminal gives it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Paste {
    #[default]
    Bare,
    Bracketed,
}

/// Where a paste goes. Nothing here is per-character: a hosted pane's child
/// takes the whole paste as one event, which is the point — one send rather
/// than one keystroke per character, and nothing that can be re-interpreted
/// halfway through. Varde's own panes never saw a paste as anything but typing,
/// so the paste is replayed as the keystrokes the host terminal sent before it
/// was told to bracket them, and the edge runs each one through `update` in
/// turn because what a key means depends on the state the key before it left.
pub enum Pasted {
    ToChild(Event),
    /// One edit into the open buffer. A paste is not typing: replayed as
    /// keystrokes it would reach the buffer's `insert`, which closes a pair it
    /// sees opened.
    ToBuffer(Event),
    AsKeys(Vec<KeyEvent>),
}

pub fn on_paste(state: &State, drafts: &Drafts, text: String) -> Pasted {
    if child_owns_keys(state, drafts) {
        return Pasted::ToChild(Event::Pasted(text));
    }
    if buffer_takes_paste(state, drafts) {
        // A pasted line ending is a line break however the source spelled it —
        // the same normalisation the keystroke path below does by turning CR,
        // LF and CRLF alike into one Enter. Passed through, a carriage return
        // is a character in the buffer and then a character in the file.
        return Pasted::ToBuffer(Event::EditorPaste(
            text.replace("\r\n", "\n").replace('\r', "\n"),
        ));
    }
    // A pasted line ending is one Enter however the source spelled it: CRLF
    // typed as two keys would leave a blank line behind every line.
    Pasted::AsKeys(
        text.replace("\r\n", "\n")
            .chars()
            .map(|c| {
                KeyEvent::new(match c {
                    '\n' | '\r' => KeyCode::Enter,
                    '\t' => KeyCode::Tab,
                    _ => KeyCode::Char(c),
                })
            })
            .collect(),
    )
}

/// Whether the focused pane's child owns the keyboard. A hosted pane's keys
/// belong to its child — but only while nothing of Varde's own is collecting
/// them: a modal, either search, and the `:` and filter lines all claim the
/// keys wherever focus happens to be, and focus can move while one is open.
fn child_owns_keys(state: &State, drafts: &Drafts) -> bool {
    !a_box_has_the_keys(state, drafts)
        && match state.focus {
            Pane::Terminal => true,
            // With no session the AI pane is an input box asking which CLI to
            // start, so it is not hosting anything yet.
            Pane::Ai => state.ai_running,
            // And with no program started the Debug group shows the Variables
            // alone, so there is no child there to type at either.
            Pane::Output => state.output_running,
            Pane::Tree
            | Pane::Editor
            | Pane::Evaluator
            | Pane::Risk
            | Pane::Buffers
            | Pane::History
            | Pane::Breakpoints
            | Pane::Frames
            | Pane::Diagnostics
            | Pane::Conflicts
            | Pane::Variables
            | Pane::Cheatsheet => false,
        }
}

/// Whether one of Varde's own boxes has the keys — a modal, either search, the
/// command line or the tree filter. Read by both destinations a paste can have
/// besides the buffer, so it is one list rather than two spellings of it.
fn a_box_has_the_keys(state: &State, drafts: &Drafts) -> bool {
    !matches!(state.modal, Modal::None)
        || state.search.is_some()
        || state
            .find
            .as_ref()
            .is_some_and(|find| find.keys != FindKeys::Away)
        || drafts.command.is_some()
        || drafts.filter.is_some()
}

/// Whether a paste is text going into the open buffer — anywhere the buffer is
/// the surface an edit reaches, whichever mode it is in. Not only insert mode,
/// which is what this asked before: a paste replayed as keystrokes in normal
/// mode is read as commands, and pasted code reliably holds an `i`, `a` or `o`
/// early on — so the letters before it were spent as motions and everything
/// after it was *typed*, one auto-indenting Enter per line, which is how a
/// three-line paste came back as a staircase. Vim's rule that a pasted `dd` is
/// a command is not worth that: a paste is not typing and it is not a chord
/// either, and text that arrived whole goes in whole.
///
/// A read-only surface still gets none of it — a diff and a walkthrough answer
/// the editor's keys ahead of the buffer, and a Preview refuses every key that
/// edits — and neither does a box that is collecting keys, which is what the
/// first term below says. Which *view* is on screen decides nothing: a buffer
/// is edited in Story view as well.
fn buffer_takes_paste(state: &State, drafts: &Drafts) -> bool {
    // The comment box is the second buffer a paste can reach, and the one this
    // arm was extended for: replayed as keystrokes, the first newline in a
    // pasted stack trace was the Enter that filed the comment, so pasting one
    // filed a fragment of it. Only once a type is picked — before that the box
    // is a question, and its body is not on screen to paste into.
    if matches!(state.modal, Modal::Comment) && !drafts.comment_kind.is_empty() {
        return true;
    }
    !a_box_has_the_keys(state, drafts) && the_buffer_takes_edits(state)
}

/// Whether the open buffer is the surface an edit reaches: focus on it, and
/// nothing read-only claiming the editor's keys in front of it — a diff, a
/// walked Site and a Preview all answer them ahead of the buffer, so a
/// character that reached it there would edit a file being read.
///
/// Read by a paste and, with insert mode on top, by Tab: one list rather than
/// two spellings of it, for the reason [`a_box_has_the_keys`] is one — two
/// spellings of the same question are two answers waiting to disagree.
fn the_buffer_takes_edits(state: &State) -> bool {
    state.focus == Pane::Editor
        && state.diff.is_none()
        && state.walking.is_none()
        && !crate::previewing(state)
}

/// Whether the open buffer is the thing being *typed* into: the surface above,
/// in insert mode.
fn typing_into_the_buffer(state: &State) -> bool {
    the_buffer_takes_edits(state) && crate::editor_inserting(state)
}

/// The reserved keys, then bytes. These two arms and the bare Ctrl claimed by
/// the caller are the complete list of what Varde takes from a child;
/// everything else is transport, which is what makes a key nobody has thought
/// of arrive anyway. Escape is not one of them: it is counted *and* forwarded,
/// so a double-tap opens the palette without the child losing the key.
fn to_child(state: &State, event: KeyEvent, at_ms: u64) -> Vec<Event> {
    if event.modifiers == KeyModifiers::ALT {
        if let KeyCode::Char(letter @ ('h' | 'j' | 'k' | 'l')) = event.code {
            return vec![Event::MoveFocus(match letter {
                'h' => Direction::Left,
                'l' => Direction::Right,
                'k' => Direction::Up,
                _ => Direction::Down,
            })];
        }
    }
    // The selection was made with the mouse in Varde's own UI, so the child
    // cannot be mid-anything that wanted the byte. With nothing picked it
    // interrupts, which is what stops a running command. Only a selection in
    // *this* pane counts: one left in the editor, or in the other hosted pane,
    // used to take the interrupt away from a shell nobody could then stop.
    if (event.modifiers == KeyModifiers::CTRL || event.modifiers == KeyModifiers::SUPER)
        && event.code == KeyCode::Char('c')
        && matches!(state.selection, Some(Selection::Screen { pane, .. }) if pane == state.focus)
    {
        return vec![Event::Copy];
    }
    let mut events = match encode(event) {
        Some(bytes) => vec![Event::Bytes(bytes)],
        None => vec![],
    };
    // Escape is the palette's other tap: reported by every terminal, needing no
    // modifier, and — because it is forwarded here rather than claimed — the
    // only candidate that withholds no byte. Whether the pair opens the palette
    // is `update`'s call. A repeat is one press held, not two.
    if event.code == KeyCode::Esc && event.modifiers.is_empty() && event.kind == KeyEventKind::Press
    {
        events.push(Event::Tapped {
            key: Tap::Escape,
            at_ms,
        });
    }
    events
}

/// The bytes a real terminal would send, always as legacy xterm sequences: the
/// child is told it is an xterm, so it never asked for anything richer, and
/// encoding needs to know nothing about the child. An error means no such
/// sequence exists — a bare Shift, a media key — and a real terminal would send
/// nothing for it either, so nothing is being withheld here.
fn encode(event: KeyEvent) -> Option<Vec<u8>> {
    // Legacy xterm has no notion of a repeat: a held key sends the press again.
    // Terminals speaking the Kitty protocol report repeats, which Varde asks
    // for, so without this every repeat would encode as unsupported.
    let mut event = KeyEvent {
        kind: KeyEventKind::Press,
        ..event
    };
    // A control code carries no case and no Shift: a real terminal sends 0x03
    // for Ctrl+C, Ctrl+Shift+c and Ctrl+Shift+C alike. The encoder refuses all
    // but the lowercase form, so without this the shifted ones arrive as
    // nothing — and which of the three a terminal reports is not Varde's
    // choice.
    if let (true, KeyCode::Char(c)) = (event.modifiers.contains(KeyModifiers::CTRL), event.code) {
        event.modifiers.remove(KeyModifiers::SHIFT);
        event.code = KeyCode::Char(c.to_ascii_lowercase());
    }
    // Workaround: the encoder has sequences for F1 to F12 and, above those,
    // writes a bare CSI introducer and reports success. Sending that to a child
    // is worse than sending nothing — an unfinished escape sequence swallows
    // whatever the child reads next. Verified against terminput 0.5.15: F13
    // encodes as `ESC [`, and `ESC ; 1 [` with a modifier.
    if matches!(event.code, KeyCode::F(13..)) {
        return None;
    }
    let mut buf = [0u8; 32];
    let written = Input::Key(event).encode(&mut buf, Encoding::Xterm).ok()?;
    (written > 0).then(|| buf[..written].to_vec())
}

/// Some terminals report the base key plus Shift rather than the shifted
/// character, which would turn ':' into ';' and 'N' into 'n'. Applied once, on
/// the way in, so that every binding below reads the character the user typed —
/// including the ones that ask for a modifier, since `C-S-f` is not `C-f`.
fn shifted(mut event: KeyEvent) -> KeyEvent {
    if !event.modifiers.contains(KeyModifiers::SHIFT) {
        return event;
    }
    if let KeyCode::Char(c) = event.code {
        event.code = KeyCode::Char(match c {
            ';' => ':',
            '\'' => '"',
            '1' => '!',
            '/' => '?',
            '.' => '>',
            ',' => '<',
            other => other.to_ascii_uppercase(),
        });
    }
    event
}

/// The character a keypress types, if it types one. Ctrl and Alt make a key
/// something other than the character on it — `C-f` opens search, `M-h` moves
/// focus — so neither of those is text.
fn typed(event: KeyEvent) -> Option<char> {
    match event.code {
        KeyCode::Char(c)
            if !event
                .modifiers
                .intersects(KeyModifiers::CTRL | KeyModifiers::ALT) =>
        {
            Some(c)
        }
        _ => None,
    }
}

fn arrow(code: KeyCode) -> Option<Direction> {
    Some(match code {
        KeyCode::Left => Direction::Left,
        KeyCode::Right => Direction::Right,
        KeyCode::Up => Direction::Up,
        KeyCode::Down => Direction::Down,
        _ => return None,
    })
}

fn searching(event: KeyEvent) -> Vec<Event> {
    if let Some(events) = query_key(event) {
        return events;
    }
    // Up and down step between hits; sideways is the query's caret. A modifier
    // makes an arrow a different binding, and search has none.
    if let Some(direction @ (Direction::Up | Direction::Down)) = arrow(event.code) {
        if event
            .modifiers
            .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT)
        {
            return vec![];
        }
        return vec![Event::MoveHit(direction)];
    }
    match event.code {
        KeyCode::Esc => vec![Event::CloseSearch],
        KeyCode::Enter => vec![Event::OpenHit],
        KeyCode::Char('o') if event.modifiers.contains(KeyModifiers::CTRL) => {
            vec![Event::OpenEveryHit]
        }
        // File by file, and modifier-only on purpose: inside this box there is
        // no bare key left to spend, because every printable one is a letter of
        // the query. R31.11's hazard is Option and the Kitty modifier reports —
        // a stock tmux strips those — and a Ctrl'd letter is none of that: it
        // is one ASCII control byte every terminal sends unaided, which is why
        // `Ctrl+F`, `Ctrl+O` and `Ctrl+Q` are bound with no bare alias either.
        // The arrows remain the modifier-free way through the hits.
        KeyCode::Char('n') if event.modifiers.contains(KeyModifiers::CTRL) => {
            vec![Event::MoveHitFile(Direction::Down)]
        }
        KeyCode::Char('p') if event.modifiers.contains(KeyModifiers::CTRL) => {
            vec![Event::MoveHitFile(Direction::Up)]
        }
        KeyCode::Tab => vec![Event::CompleteSearch],
        _ => vec![],
    }
}

/// `/` in the editor, while its search has the keyboard: the query, one of
/// the icons after it, or the replace box. The query is state rather than a
/// draft because the cursor moves to the closest match on every keystroke.
fn finding(find: &Find, event: KeyEvent) -> Vec<Event> {
    let keys = |keys| vec![Event::FindKeys(keys)];
    let back = event.modifiers.contains(KeyModifiers::SHIFT);
    match find.keys {
        FindKeys::Away => vec![],
        FindKeys::Query => {
            let at_end = find.query.column > find.query.shown().chars().count();
            match event.code {
                KeyCode::Esc => vec![Event::CloseFind],
                KeyCode::Enter => vec![Event::AcceptFind],
                KeyCode::Tab => keys(FindKeys::Icon(FindIcon::Case)),
                KeyCode::Right if at_end && event.modifiers.is_empty() => {
                    keys(FindKeys::Icon(FindIcon::Case))
                }
                _ => query_key(event).unwrap_or_default(),
            }
        }
        FindKeys::Icon(icon) => {
            let at = FIND_ICONS.iter().position(|(each, _)| *each == icon);
            let at = at.unwrap_or(0);
            match event.code {
                KeyCode::Left if at == 0 => vec![
                    Event::FindKeys(FindKeys::Query),
                    Event::QueryEnd(Direction::Right),
                ],
                KeyCode::Left => keys(FindKeys::Icon(FIND_ICONS[at - 1].0)),
                KeyCode::Right => keys(FindKeys::Icon(
                    FIND_ICONS[(at + 1).min(FIND_ICONS.len() - 1)].0,
                )),
                KeyCode::Up | KeyCode::Esc => keys(FindKeys::Query),
                KeyCode::Enter => match icon {
                    FindIcon::Case => vec![Event::ToggleCase],
                    FindIcon::Replace | FindIcon::ReplaceAll => {
                        keys(FindKeys::Replace(ReplaceField::With))
                    }
                },
                _ => vec![],
            }
        }
        FindKeys::Replace(field) => {
            let at = REPLACE_FIELDS.iter().position(|each| *each == field);
            let at = at.unwrap_or(0);
            let count = REPLACE_FIELDS.len();
            match event.code {
                KeyCode::Esc => keys(FindKeys::Away),
                KeyCode::Tab if back => {
                    keys(FindKeys::Replace(REPLACE_FIELDS[(at + count - 1) % count]))
                }
                KeyCode::Tab => keys(FindKeys::Replace(REPLACE_FIELDS[(at + 1) % count])),
                KeyCode::Enter if field == ReplaceField::ReplaceAll => vec![Event::ReplaceAll],
                KeyCode::Enter => vec![Event::ReplaceMatch],
                _ if matches!(field, ReplaceField::Find | ReplaceField::With) => {
                    query_key(event).unwrap_or_default()
                }
                // On a button no letter is text, so `n` and `N` step past a
                // match without replacing it.
                _ => match typed(event) {
                    Some('n') => vec![Event::StepMatch(Direction::Right)],
                    Some('N') => vec![Event::StepMatch(Direction::Left)],
                    _ => vec![],
                },
            }
        }
    }
}

/// A key that edits a search query or moves its caret — the `/` line's and the
/// project search's alike, since both are a [`crate::editor::Buffer`] the core
/// hands these to. The word motions come in both spellings for the reason
/// [`comment_body`] takes both: `^[b` and `^[f` are what Option+arrow sends
/// on macOS.
fn query_key(event: KeyEvent) -> Option<Vec<Event>> {
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let ctrl = event.modifiers.contains(KeyModifiers::CTRL);
    let word = |direction| Some(vec![Event::EditorWord(direction)]);
    match event.code {
        KeyCode::Char('b') if alt && !ctrl => word(Direction::Left),
        KeyCode::Char('f') if alt && !ctrl => word(Direction::Right),
        KeyCode::Left if alt => word(Direction::Left),
        KeyCode::Right if alt => word(Direction::Right),
        KeyCode::Left => Some(vec![Event::EditorArrow(Direction::Left)]),
        KeyCode::Right => Some(vec![Event::EditorArrow(Direction::Right)]),
        KeyCode::Home => Some(vec![Event::QueryEnd(Direction::Left)]),
        KeyCode::End => Some(vec![Event::QueryEnd(Direction::Right)]),
        KeyCode::Backspace if alt => Some(vec![Event::EditorDeleteWord]),
        KeyCode::Backspace => Some(vec![Event::EditorBackspace]),
        _ => typed(event).map(|c| vec![Event::EditorKey(c)]),
    }
}

/// A key while the comment box is open. Two phases, and only the first is this
/// module's business: until a type is picked the box is a question, and after
/// it the box is a [`crate::editor::Buffer`] in [`State::comment`] that answers
/// the editor's own events. The body used to be `Drafts::name` — a string with
/// no cursor, no word motion and no newline, appended to here.
fn comment_picker(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    if drafts.comment_kind.is_empty() {
        return comment_type(drafts, event);
    }
    comment_body(drafts, event)
}

/// Picking the type. Only the four letters pick one, and a stray key picks
/// nothing — a catch-all here once chose COMMENT for you silently, which is why
/// this guard is load-bearing rather than tidy.
fn comment_type(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::EditorEscape],
        _ => {
            if let Some(kind) = typed(event).and_then(comment_kind) {
                drafts.comment_kind = kind.to_string();
            }
            vec![]
        }
    }
}

/// Typing the body, once a type is picked. Every gesture is the editor's, so
/// every event here is one `update` already hands to a buffer — spelled out
/// rather than routed through [`routed`], which asks where focus is and what
/// the pane behind the box is showing.
///
/// Enter is a newline, so filing the comment is a gesture of its own. The
/// letters are all text, which is what puts a modifier on it: [`COMMENT_BOX_KEYS`]
/// is where the box's footer names it, and the test beside that constant is
/// what holds the two together.
fn comment_body(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    if event.modifiers.contains(KeyModifiers::CTRL) && event.code == KeyCode::Char('s') {
        return vec![Event::FileComment {
            kind: std::mem::take(&mut drafts.comment_kind),
        }];
    }
    // The box's second Ctrl key, and for the first one's reason: every letter in
    // the body is text, so `u` is a letter of the comment and undo cannot have
    // the key the editor gives it.
    if let Some(undo) = undo_key(event) {
        return vec![undo];
    }
    // What Option+arrow actually sends on macOS — `^[b` and `^[f` — for the
    // reason [`word_motion_alias`] gives: Varde cannot ask for Option to be
    // reported as Alt without costing every character Option composes, so the
    // gesture is met in the shape the terminal chose for it. Without this the
    // body's word motion is unreachable for exactly the people the editor
    // already went to this trouble for.
    if let (true, false, KeyCode::Char(letter @ ('b' | 'f'))) = (
        alt,
        event.modifiers.contains(KeyModifiers::CTRL),
        event.code,
    ) {
        return vec![Event::EditorWord(if letter == 'f' {
            Direction::Right
        } else {
            Direction::Left
        })];
    }
    match event.code {
        KeyCode::Esc => {
            drafts.comment_kind.clear();
            vec![Event::EditorEscape]
        }
        KeyCode::Enter => vec![Event::EditorKey('\n')],
        // The word behind the cursor with Alt, one character without — the
        // editor's own pair, reached here with no mode to ask about: the body
        // is always inserting, so the normal-mode collision with the `db`
        // operator that keeps [`backspace_word`] off this key in the editor
        // cannot arise. `db` is still the modifier-free route the contract
        // asks for, and it is the same two keys in the same buffer.
        KeyCode::Backspace if alt => vec![Event::EditorDeleteWord],
        KeyCode::Backspace => vec![Event::EditorBackspace],
        // Sideways with Alt is a word motion, as it is in the editor; up and
        // down are left as plain moves there, and are here too.
        KeyCode::Left | KeyCode::Right if alt => {
            arrow(event.code).map_or(vec![], |direction| vec![Event::EditorWord(direction)])
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
            arrow(event.code).map_or(vec![], |direction| vec![Event::EditorArrow(direction)])
        }
        _ => match typed(event) {
            Some(c) => vec![Event::EditorKey(c)],
            None => vec![],
        },
    }
}

fn comment_kind(key: char) -> Option<&'static str> {
    match key {
        'i' => Some("ISSUE"),
        'n' => Some("NOTE"),
        's' => Some("SUGGESTION"),
        'c' => Some("COMMENT"),
        _ => None,
    }
}

/// The Breakpoint box: Tab and the arrows walk its rows, Space flips the
/// switch on the last one, and every other row is typed into. Its text is the
/// core's rather than a `Drafts` field, because there are three of it.
fn breakpoint_box(field: debug::Field, draft: &debug::Properties, event: KeyEvent) -> Vec<Event> {
    let walk = |by: usize| {
        let at = debug::FIELDS
            .iter()
            .position(|row| *row == field)
            .unwrap_or(0);
        debug::FIELDS[(at + by) % debug::FIELDS.len()]
    };
    let back = event.modifiers.contains(KeyModifiers::SHIFT);
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        KeyCode::Enter => vec![Event::ConfirmBreakpoint],
        KeyCode::Tab if back => vec![Event::BreakpointField(walk(debug::FIELDS.len() - 1))],
        KeyCode::Up => vec![Event::BreakpointField(walk(debug::FIELDS.len() - 1))],
        KeyCode::Tab | KeyCode::Down => vec![Event::BreakpointField(walk(1))],
        _ if field == debug::Field::Suspend => match typed(event) {
            Some(' ') => vec![Event::SwitchSuspend],
            _ => vec![],
        },
        KeyCode::Backspace => {
            let mut text = debug::field_text(draft, field).to_string();
            text.pop();
            vec![Event::BreakpointDraft(text)]
        }
        _ => match typed(event) {
            Some(c) => vec![Event::BreakpointDraft(format!(
                "{}{c}",
                debug::field_text(draft, field)
            ))],
            None => vec![],
        },
    }
}

fn name_box(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => {
            drafts.name.clear();
            vec![Event::Cancel]
        }
        KeyCode::Enter => vec![Event::EnterName(std::mem::take(&mut drafts.name))],
        KeyCode::Backspace => {
            drafts.name.pop();
            vec![]
        }
        _ => {
            if let Some(c) = typed(event) {
                drafts.name.push(c);
            }
            vec![]
        }
    }
}

fn routed(state: &State, drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    let alt = event.modifiers.contains(KeyModifiers::ALT);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);
    if let Some(events) = word_motion_alias(state, event, alt, shift) {
        return events;
    }
    if let Some(events) = focus_alias(event, alt) {
        return events;
    }
    if let Some(events) = collecting(state, drafts, event) {
        return events;
    }
    if let Some(events) = jump_alias(event) {
        return events;
    }
    if let Some(events) = occurrence_alias(state, event) {
        return events;
    }
    if let Some(events) = arrow_event(state, event, alt, shift) {
        return events;
    }
    if let Some(events) = tab_indent(state, event) {
        return events;
    }
    if let Some(events) = backspace_word(state, event) {
        return events;
    }
    pane_key(state, event)
}

/// `Ctrl+p` and `Ctrl+n`, once nothing ahead of them has claimed them: one step
/// back through the places the cursor has been, and one step forward. In
/// Varde's own panes only — a hosted pane's child has had them long since,
/// where they are readline's own history keys, and the `RESERVED` list is what
/// holds that true. Behind [`collecting`], so the results box keeps them for
/// `MoveHitFile` and the `:` line, the find box and the tree filter all keep
/// them as the nothing they were.
///
/// `gp`/`gn` is the modifier-free route the contract requires — a jump moves
/// the cursor, so it is a Motion — and it is spelled with the same letters, so
/// the two teach each other.
///
/// Ctrl+Option with the left and right arrows is the third spelling: back and
/// forward are what those two arrows mean everywhere a history is walked, and
/// the hand reading code is already on them. Both modifiers, never Option
/// alone — Alt+arrow is the word motion, and a jump is not a word. Ctrl and
/// not Cmd, which this was spelled with for as long as it did nothing: macOS
/// keeps Command for itself, so a terminal reports it on a letter at best and
/// never on an arrow, and a gesture no terminal can send is a binding that
/// exists only in the test that drives it. Ctrl+Option+arrow is `CSI 1;7`,
/// which every terminal sends and no window manager takes. No other modifier
/// is inspected on either spelling, so one that names no gesture of its own is
/// folded into the key it triggers.
fn jump_alias(event: KeyEvent) -> Option<Vec<Event>> {
    if event
        .modifiers
        .contains(KeyModifiers::ALT | KeyModifiers::CTRL)
    {
        match event.code {
            KeyCode::Left => return Some(vec![Event::JumpBack]),
            KeyCode::Right => return Some(vec![Event::JumpForward]),
            _ => {}
        }
    }
    if !event.modifiers.contains(KeyModifiers::CTRL) {
        return None;
    }
    match event.code {
        KeyCode::Char('p') => Some(vec![Event::JumpBack]),
        KeyCode::Char('n') => Some(vec![Event::JumpForward]),
        _ => None,
    }
}

/// `Ctrl+d`, once nothing ahead of it has claimed it: the next occurrence of
/// what is picked joins it, so the next keystroke lands at both. In the
/// editor's own buffer only — Review's diff and a walked Site claim every key
/// they are handed, and neither is a surface anything is typed into.
///
/// Command is its alias, which is how VS Code spells it on a Mac. `gm` is the
/// modifier-free route the contract requires and is answered in `update`,
/// beside `gd`, since a chord needs the buffer that holds the waiting `g`. No
/// modifier but Ctrl and Command is inspected, so one that names no gesture of
/// its own is folded into the key it triggers.
fn occurrence_alias(state: &State, event: KeyEvent) -> Option<Vec<Event>> {
    (state.focus == Pane::Editor
        && state.diff.is_none()
        && state.walking.is_none()
        && ctrl_or_command(event)
        && event.code == KeyCode::Char('d'))
    .then(|| vec![Event::EditorNextOccurrence])
}

/// Alt+Backspace, once nothing ahead of the buffer has claimed it: the word
/// behind the cursor rather than the character. Insert mode only — in normal
/// mode `d` and `b` are the operator that already spells this, and one gesture
/// must not mean two things in the one mode where both are reachable. `db` is
/// the modifier-free route the contract requires, which is what makes asking
/// for Alt here affordable.
///
/// [`typing_into_the_buffer`] is the whole condition, for the reason
/// [`tab_indent`] gives above.
fn backspace_word(state: &State, event: KeyEvent) -> Option<Vec<Event>> {
    (event.code == KeyCode::Backspace
        && event.modifiers.contains(KeyModifiers::ALT)
        && typing_into_the_buffer(state))
    .then(|| vec![Event::EditorDeleteWord])
}

/// Tab, once nothing ahead of the buffer has claimed it. A snippet's remaining
/// stops are the one thing that does — [`Modal::Stops`] answers Tab and passes
/// everything else through, so "the next blank" already wins where it applies
/// and this needs no rule of its own to stay behind it.
///
/// [`typing_into_the_buffer`] is the whole condition, and deliberately not
/// [`buffer_takes_paste`]: a box that is *collecting* keys has already claimed
/// this one long before `routed` is reached — search and find above, the
/// command and filter lines in [`collecting`] — so asking again would only
/// exclude the two modals that pass keys straight through. Both of those are
/// typing: a candidate list is offered *while* a name is being typed, and a
/// tab-stop sequence takes Tab itself first. Excluding them is how Tab came to
/// do nothing behind an open candidate list, which is insert mode by any other
/// name.
///
/// One [`Event::EditorPaste`] rather than a run of spaces: a paste is one edit,
/// so `u` costs the one key the indent cost. No modifier is inspected — Tab is
/// the whole gesture, so a modifier that names no gesture of its own is folded
/// into the key it triggers, exactly as at a tab stop.
///
/// A width of zero is unbound rather than an empty insertion: a paste is an
/// edit whatever it carries, so a config naming `0` would mark the buffer dirty
/// and cost an `u` for text nobody can see. It is the width the *cursor* case
/// reads; a block indent takes its level from the file, since a span of lines
/// already shows what that file indents with.
///
/// With characters picked, Tab moves them instead: an indent typed into a
/// selection lands in the middle of what was picked and leaves it where it was.
/// Shift is inspected on this key and nowhere else on it — Shift+Tab is the
/// other half of one gesture, spelled that way by every editor outside vim —
/// which is why the two are decided here rather than in a rule of their own.
fn tab_indent(state: &State, event: KeyEvent) -> Option<Vec<Event>> {
    if !(event.code == KeyCode::Tab && typing_into_the_buffer(state)) {
        return None;
    }
    if matches!(state.selection, Some(Selection::Buffer { .. })) {
        return Some(vec![Event::EditorIndent(
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                Direction::Left
            } else {
                Direction::Right
            },
        )]);
    }
    let indent = " ".repeat(state.tab_width);
    (!indent.is_empty()).then(|| vec![Event::EditorPaste(indent)])
}

/// The arrow aliases for the word motions: `w`/`b` bare, `W`/`B` with shift.
/// Word motion is horizontal, so up and down are unbound rather than guessed
/// at. Stepping buffers keeps its own modifier-free `gt`/`gT`.
///
/// Ctrl is not part of the gesture, and an *arrow* carrying it is something
/// else entirely — [`jump_alias`]'s Ctrl+Option spelling — for the reason
/// [`focus_alias`] says no to Ctrl. Declined here rather than reordered behind
/// the jump: Ctrl+p and Ctrl+n sit behind [`collecting`] on purpose, and one
/// spelling of a gesture that outranks a box the other waits for is the
/// difference this repo's sweeps exist to catch. The readline escapes below
/// keep every modifier they had — the pair below is read on an arrow and
/// nowhere else, so a letter carrying Ctrl names no gesture of its own here.
fn word_motion_alias(state: &State, event: KeyEvent, alt: bool, shift: bool) -> Option<Vec<Event>> {
    if !(state.focus == Pane::Editor && alt) {
        return None;
    }
    match (arrow(event.code), shift) {
        (Some(_), _) if event.modifiers.contains(KeyModifiers::CTRL) => return None,
        (Some(direction @ (Direction::Left | Direction::Right)), true) => {
            return Some(vec![Event::EditorExtendWord(direction)]);
        }
        (Some(direction @ (Direction::Left | Direction::Right)), false) => {
            return Some(vec![Event::EditorWord(direction)]);
        }
        _ => {}
    }
    // What Option+arrow actually sends on macOS: `^[b` and `^[f`, the readline
    // word escapes, rather than a modified arrow. Varde cannot ask for Option
    // to be reported as Alt — that costs every character Option composes, `[]`
    // and `{}` among them (see `main.rs`) — so the gesture has to be met in the
    // shape the terminal chose for it.
    let (KeyCode::Char(letter @ ('b' | 'f')), false) =
        (event.code, event.modifiers.contains(KeyModifiers::CTRL))
    else {
        return None;
    };
    let direction = if letter == 'f' {
        Direction::Right
    } else {
        Direction::Left
    };
    Some(vec![Event::EditorWord(direction)])
}

/// Alt+letters move focus. Ctrl is not part of the gesture, and a key carrying
/// it is something else entirely.
fn focus_alias(event: KeyEvent, alt: bool) -> Option<Vec<Event>> {
    if !(alt && !event.modifiers.contains(KeyModifiers::CTRL)) {
        return None;
    }
    let KeyCode::Char(letter @ ('h' | 'j' | 'k' | 'l')) = event.code else {
        return None;
    };
    Some(vec![Event::MoveFocus(match letter {
        'h' => Direction::Left,
        'l' => Direction::Right,
        'k' => Direction::Up,
        _ => Direction::Down,
    })])
}

/// Whatever of Varde's own is already collecting characters, and the keys that
/// open one. A line that is collecting claims the printable keys while it is
/// open and gives them straight back when it closes, which is why `/` costs no
/// motion.
fn collecting(state: &State, drafts: &mut Drafts, event: KeyEvent) -> Option<Vec<Event>> {
    if let Some(find) = state
        .find
        .as_ref()
        .filter(|find| find.keys != FindKeys::Away)
    {
        return Some(finding(find, event));
    }
    if drafts.command.is_some() {
        return Some(command_line(drafts, event));
    }
    if typed(event) == Some(':') && claims_colon(state) {
        drafts.command = Some(String::new());
        return Some(vec![]);
    }
    // With no session, the AI pane is an input box asking which CLI to start.
    if state.focus == Pane::Ai && !state.ai_running {
        return Some(start_ai_box(drafts, event));
    }
    if drafts.filter.is_some() {
        return Some(filter_box(drafts, event));
    }
    if state.focus == Pane::Tree && tree::filter_rows(state.view) > 0 && typed(event) == Some('/') {
        drafts.filter = Some(String::new());
        return Some(vec![]);
    }
    None
}

/// The terminal and AI panes keep their keys: a colon belongs to whatever is
/// running there. So does an inserting buffer — a colon is a character, and
/// claiming it made a turbofish or a YAML key untypable. The mode is read the
/// way `update` reads it for `/`; only the draft this opens lives at the edge,
/// which is why the decision could not be made there too. The mode gates the
/// *editor's* colon and nothing else: with focus on the tree the buffer is not
/// being typed into, so a buffer left inserting must not take the tree's colon
/// with it.
fn claims_colon(state: &State) -> bool {
    match state.focus {
        Pane::Tree => true,
        // The Snippet is a buffer being typed into on the same terms, so the
        // two answer alike — `editor_inserting` reads whichever of them the
        // keyboard is in.
        Pane::Editor | Pane::Evaluator => !crate::editor_inserting(state),
        // Varde's own panes, so the colon is Varde's.
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts
        | Pane::Variables
        | Pane::Cheatsheet => true,
        Pane::Ai | Pane::Terminal | Pane::Output => false,
    }
}

fn arrow_event(state: &State, event: KeyEvent, alt: bool, shift: bool) -> Option<Vec<Event>> {
    let direction = arrow(event.code)?;
    // Alt makes an arrow a word motion, which the editor claimed above and
    // nothing else binds. Shift extends, which only the editor does.
    if alt {
        return Some(vec![]);
    }
    Some(match (state.focus, shift) {
        // The Snippet is edited with the editor's own gestures, so its arrows
        // are the editor's arrows.
        (Pane::Editor | Pane::Evaluator, false) => vec![Event::EditorArrow(direction)],
        (Pane::Editor | Pane::Evaluator, true) => vec![Event::EditorExtend(direction)],
        (Pane::Tree, false) => list_arrow(direction),
        (Pane::Tree, true) => vec![],
        // The Risk list is a list, so the arrows do to it exactly what they do
        // to the tree: one gesture for both lists — a row action reachable only
        // by mouse is a row action the keyboard cannot get to at all.
        (Pane::Risk, false) => list_arrow(direction),
        (Pane::Risk, true) => vec![],
        // The Buffers pane is a list too, and the same gesture reaches it. Its
        // Left and Right step into row actions it has none of, which is the
        // same nothing they do on a tree row with none.
        (Pane::Buffers, false) => list_arrow(direction),
        (Pane::Buffers, true) => vec![],
        // The third list in the corner, and the same gesture reaches it. Its
        // rows have one action, so Right steps into the icon the mouse clicks.
        (Pane::History, false) => list_arrow(direction),
        (Pane::History, true) => vec![],
        // The fourth, and its rows have one action too. The Variables are a
        // list as well, in the Strip rather than the corner: the arrows reach
        // a tree the same way they reach a flat list, because what they move
        // is the selection either way.
        (
            Pane::Breakpoints
            | Pane::Frames
            | Pane::Diagnostics
            | Pane::Conflicts
            | Pane::Variables,
            false,
        ) => list_arrow(direction),
        (
            Pane::Breakpoints
            | Pane::Frames
            | Pane::Diagnostics
            | Pane::Conflicts
            | Pane::Variables,
            true,
        ) => vec![],
        (Pane::Cheatsheet, false) => match direction {
            Direction::Up | Direction::Down => {
                vec![Event::ScrollCheatsheet { direction, rows: 1 }]
            }
            Direction::Left | Direction::Right => vec![],
        },
        (Pane::Cheatsheet, true) => vec![],
        // A hosted pane's arrows went to its child; see below.
        (Pane::Ai | Pane::Terminal | Pane::Output, _) => vec![],
    })
}

/// Up and down move the selection; Right steps into the row's actions and Left
/// steps back out.
fn list_arrow(direction: Direction) -> Vec<Event> {
    match direction {
        Direction::Up | Direction::Down => vec![Event::MoveSelection(direction)],
        Direction::Left | Direction::Right => vec![Event::MoveAction(direction)],
    }
}

fn pane_key(state: &State, event: KeyEvent) -> Vec<Event> {
    match state.focus {
        // Ctrl+Enter runs the Snippet wherever the terminal reports it: an
        // alias for the normal-mode Enter and the Run Chip, never the only way
        // there, so a terminal that swallows it costs nothing. In the
        // Evaluator alone — everywhere else Enter means what it always meant,
        // and a modifier no arm inspects is no gesture of its own.
        Pane::Evaluator
            if event.code == KeyCode::Enter && event.modifiers.contains(KeyModifiers::CTRL) =>
        {
            vec![Event::RunSnippet]
        }
        // `:w` as a key, in either mode: the same event, so the command's
        // refusals are the key's too. Edit view's Editor alone — the
        // Evaluator's text is a Snippet, not the file `:w` writes, and Review
        // and Story are read.
        Pane::Editor
            if state.view == View::Edit
                && event.code == KeyCode::Char('s')
                && ctrl_or_command(event) =>
        {
            vec![Event::WriteBuffer]
        }
        Pane::Editor | Pane::Evaluator => editor_pane_key(event),
        Pane::Tree => tree_pane_key(event),
        // The corner's panes answer Enter — go to the code behind the figure,
        // go to that buffer — and hand their letters to `update`, which is
        // where the Risk list's `j`, `k`, `a`, `r` and `l` and the Buffers
        // pane's `j` and `k` are decided. One arm for both because the routing
        // is identical; what each letter *means* is `update`'s, and that is
        // where they differ. The panes' routing lives here rather than at the
        // edge for the reason every other pane's does: what a key means is a
        // decision, and `main.rs` has no test.
        Pane::Risk
        | Pane::Buffers
        | Pane::History
        | Pane::Breakpoints
        | Pane::Frames
        | Pane::Diagnostics
        | Pane::Conflicts
        | Pane::Variables => list_pane_key(event),
        Pane::Cheatsheet => {
            let (direction, rows) = match (event.code, typed(event)) {
                (_, Some('j')) => (Direction::Down, 1),
                (_, Some('k')) => (Direction::Up, 1),
                (KeyCode::PageDown, _) => (Direction::Down, crate::cheatsheet_fits(state)),
                (KeyCode::PageUp, _) => (Direction::Up, crate::cheatsheet_fits(state)),
                _ => return vec![],
            };
            vec![Event::ScrollCheatsheet { direction, rows }]
        }
        // A hosted pane never arrives here: with nothing of Varde's own
        // collecting, `child_owns_keys` sent the key to `to_child`, and with
        // something collecting one of the returns above took it. Every pane is
        // named rather than caught by a `_`, so a new one is a compiler
        // error rather than a pane whose keys go nowhere.
        Pane::Ai | Pane::Terminal | Pane::Output => vec![],
    }
}

/// Whether a key carries the modifier copy, paste, write and the next
/// occurrence answer to. Ctrl and Command are aliases on these four keys and on
/// no others: Command is the gesture the reader already has for them, and Ctrl
/// is the one that survives a terminal that never reports Command at all
/// (R31.11). Neither is the only route — the register's `y` and `p`, the `:w`
/// line and `gm` need no modifier.
fn ctrl_or_command(event: KeyEvent) -> bool {
    event
        .modifiers
        .intersects(KeyModifiers::CTRL | KeyModifiers::SUPER)
}

/// Undo and redo as every editor outside vim spells them, in either mode:
/// `u` and `U` are letters while inserting. Ctrl and Command are aliases here
/// too. Redo is Shift on top, which arrives as `Z` once [`shifted`] has run —
/// and as plain `C-z` from a terminal that folds Shift away, where `U` is the
/// way to redo.
fn undo_key(event: KeyEvent) -> Option<Event> {
    match event.code {
        KeyCode::Char('z') if ctrl_or_command(event) => Some(Event::EditorUndo),
        KeyCode::Char('Z') if ctrl_or_command(event) => Some(Event::EditorRedo),
        _ => None,
    }
}

fn editor_pane_key(event: KeyEvent) -> Vec<Event> {
    if let Some(undo) = undo_key(event) {
        return vec![undo];
    }
    match event.code {
        // Not claimed globally: with a hosted pane focused Ctrl+C is the
        // child's interrupt, and reaches it as bytes.
        KeyCode::Char('c') if ctrl_or_command(event) => vec![Event::Copy],
        // Pasting is the other half of one gesture, so it is spelled the same
        // way: Command+V worked long before this line existed only because the
        // host terminal answers it itself and sends the text on as a paste,
        // which made the two halves look like two unrelated features.
        KeyCode::Char('v') if ctrl_or_command(event) => vec![Event::PasteFromClipboard],
        KeyCode::Esc => vec![Event::EditorEscape],
        KeyCode::Home => vec![Event::EditorKey('0')],
        KeyCode::End => vec![Event::EditorKey('$')],
        KeyCode::Backspace => vec![Event::EditorBackspace],
        KeyCode::Enter => vec![Event::EditorKey('\n')],
        _ => match typed(event) {
            Some(c) => vec![Event::EditorKey(c)],
            None => vec![],
        },
    }
}

fn tree_pane_key(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => vec![Event::Cancel],
        KeyCode::Enter => vec![Event::Activate],
        _ => match typed(event) {
            Some(shortcut) => vec![Event::Key(shortcut)],
            None => vec![],
        },
    }
}

fn list_pane_key(event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Enter => vec![Event::Activate],
        _ => match typed(event) {
            Some(shortcut) => vec![Event::Key(shortcut)],
            None => vec![],
        },
    }
}

fn command_line(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => {
            drafts.command = None;
            vec![]
        }
        KeyCode::Backspace => {
            if let Some(draft) = drafts.command.as_mut() {
                draft.pop();
            }
            vec![]
        }
        KeyCode::Enter => command(drafts.command.take().unwrap_or_default().as_str()),
        _ => {
            if let (Some(c), Some(draft)) = (typed(event), drafts.command.as_mut()) {
                draft.push(c);
            }
            vec![]
        }
    }
}

/// `:` commands. `:q` closes the file, `:qa` closes them all. Leaving Varde is
/// not on this line at all: Ctrl+Q and the palette's `q` are the gestures for
/// it, so a mistyped clear-up cannot take the session with it.
fn command(line: &str) -> Vec<Event> {
    if let Some(events) = buffer_command(line) {
        return events;
    }
    if let Some(events) = view_command(line) {
        return events;
    }
    spawning_command(line)
}

/// The commands about buffers — the one in front of you, and all of them.
fn buffer_command(line: &str) -> Option<Vec<Event>> {
    Some(match line {
        "w" => vec![Event::WriteBuffer],
        "e" => vec![Event::ReloadBuffer],
        "q" => vec![Event::CloseBuffer { force: false }],
        "q!" => vec![Event::CloseBuffer { force: true }],
        "wq" => vec![Event::WriteBuffer, Event::CloseBuffer { force: false }],
        "qa" => vec![Event::CloseAllBuffers { force: false }],
        "qa!" => vec![Event::CloseAllBuffers { force: true }],
        _ => return None,
    })
}

/// The commands about what is on screen.
fn view_command(line: &str) -> Option<Vec<Event>> {
    // The one reading command that takes a number: the Transport's speed
    // control steps a ladder, and this is how a pace between its rungs is
    // reached. A number nobody can parse is not this command, so it falls
    // through to the same nothing every other mistyped line gets.
    if let Some(rest) = line.strip_prefix("speed ") {
        // A pace is a multiplier, so zero and below are not slow — they are a
        // division the synthesizer would have to swallow, and a Transport
        // reading `-3.00x`. Refused rather than clamped: a number nobody can
        // mean is not this command, and guessing which end of the range was
        // wanted would be answering a question the reader can retype.
        return rest
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|speed| speed.is_finite() && *speed > 0.0)
            .map(|speed| vec![Event::SetSpeed(speed)]);
    }
    Some(match line {
        "submit" => vec![Event::SubmitReview],
        "update" => vec![Event::Rebuild],
        "tall" => vec![Event::ToggleTallAi],
        "split" => vec![Event::SplitTerminal],
        "preview" => vec![Event::TogglePreview],
        "format" => vec![Event::FormatBuffer],
        // The `!` means the whole file here, as it does on `:ai!` and
        // `:story!`: the same command, answered for everything it could name.
        "toggle" => vec![Event::ToggleFold { all: false }],
        "toggle!" => vec![Event::ToggleFold { all: true }],
        "read" => vec![Event::StartReading],
        // One word for both halves of one control: R35.8's Transport has a
        // play/pause, and a reader who paused presses the same thing to go on.
        "pause" => vec![Event::PlayPause],
        "next" => vec![Event::NextUtterance],
        "prev" => vec![Event::PreviousUtterance],
        "stop" => vec![Event::StopReading],
        "help" => vec![Event::ToggleCheatsheet],
        "dim" => vec![Event::ToggleField],
        "minimap" => vec![Event::ToggleMinimap],
        _ => return None,
    })
}

/// The two that take an argument after the word, and a `!` that replaces
/// whatever is already running.
fn spawning_command(line: &str) -> Vec<Event> {
    let argument = || {
        line.split_once(' ')
            .map(|(_, rest)| rest.trim().to_string())
            .filter(|rest| !rest.is_empty())
    };
    if names(line, "ai") {
        return vec![Event::StartAi {
            command: argument(),
            force: line.starts_with("ai!"),
        }];
    }
    // Before `story`, which does not name it, and spelled as a suffix rather
    // than a subcommand word: `:story branch` would be shadowed by a repository
    // with a branch called `branch`. What follows it is a repository URL — a
    // Guest repo to clone (ADR 0015) — and nothing following it is this folder.
    if let Some(rest) = line.strip_prefix("story?") {
        let url = rest.trim();
        return vec![Event::PickBranch(
            (!url.is_empty()).then(|| url.to_string()),
        )];
    }
    if names(line, "story") {
        return vec![Event::Story {
            explicit: argument(),
            force: line.starts_with("story!"),
        }];
    }
    vec![]
}

/// The word alone, the word with a `!`, or either followed by an argument.
fn names(line: &str, word: &str) -> bool {
    line == word
        || line == format!("{word}!")
        || line.starts_with(&format!("{word} "))
        || line.starts_with(&format!("{word}! "))
}

fn start_ai_box(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Enter => {
            let command = std::mem::take(&mut drafts.ai);
            if command.trim().is_empty() {
                return vec![];
            }
            vec![Event::StartAi {
                command: Some(command.trim().to_string()),
                force: false,
            }]
        }
        KeyCode::Backspace => {
            drafts.ai.pop();
            vec![]
        }
        _ => {
            if let Some(c) = typed(event) {
                drafts.ai.push(c);
            }
            vec![]
        }
    }
}

fn filter_box(drafts: &mut Drafts, event: KeyEvent) -> Vec<Event> {
    match event.code {
        KeyCode::Esc => {
            drafts.filter = None;
            vec![Event::Filter(String::new())]
        }
        KeyCode::Enter => {
            drafts.filter = None;
            vec![Event::AcceptFilter]
        }
        KeyCode::Backspace => {
            let draft = drafts.filter.as_mut().expect("open");
            draft.pop();
            vec![Event::Filter(draft.clone())]
        }
        _ => match typed(event) {
            Some(c) => {
                let draft = drafts.filter.as_mut().expect("open");
                draft.push(c);
                vec![Event::Filter(draft.clone())]
            }
            None => vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{command, on_key_event, on_paste, Drafts, Pasted};
    use crate::{
        story, DiffLine, Direction, Event, Find, FindIcon, FindKeys, Modal, Pane, Place,
        ReplaceField, Selection, State, Tap, View,
    };
    use terminput::{
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MediaKeyCode, ModifierDirection,
        ModifierKeyCode,
    };

    fn focused(pane: Pane) -> State {
        State {
            focus: pane,
            ..State::default()
        }
    }

    /// `keys::CHEATSHEET`'s order decides what a short window shows before
    /// anyone scrolls: at 26 rows by 120 columns — the replay recipe's size and
    /// `features/ai_pane.feature`'s — the AI pane's rectangle shows sixteen of
    /// Edit view's rows. What is held is the two rows nothing else in Varde
    /// teaches: the palette gesture, and `:format`, which is in no palette, has
    /// no completion and is spelled nowhere else.
    #[test]
    fn the_rows_nothing_else_teaches_are_on_screen_before_scrolling() {
        let state = State {
            screen_width: 120,
            screen_height: 26,
            ..State::default()
        };
        let fits = crate::cheatsheet_fits(&state);
        assert_eq!(fits, 16);
        let shown: Vec<&str> = super::cheatsheet_rows(&state)
            .into_iter()
            .take(fits)
            .map(|(keys, _)| keys)
            .collect();
        for keys in ["C-space Esc Esc", ":format"] {
            assert!(
                shown.iter().any(|row| row.starts_with(keys)),
                "{keys:?} is below the fold at 26 rows: {shown:?}"
            );
        }
    }

    /// Tab and the arrows walk the Breakpoint box's rows and wrap; a key is
    /// typed onto the row it is on, Space included — except on the switch,
    /// where Space is the one key that does anything.
    #[test]
    fn the_breakpoint_box_types_into_its_rows_and_flips_its_switch() {
        use crate::debug::{Field, Properties};
        let draft = Properties {
            condition: "a".to_string(),
            ..Properties::default()
        };
        let key = |field, event| super::breakpoint_box(field, &draft, event);
        let plain = KeyEvent::new;
        assert_eq!(
            key(Field::Condition, plain(KeyCode::Char(' '))),
            vec![Event::BreakpointDraft("a ".to_string())]
        );
        assert_eq!(
            key(Field::Condition, plain(KeyCode::Backspace)),
            vec![Event::BreakpointDraft(String::new())]
        );
        assert_eq!(
            key(Field::Suspend, plain(KeyCode::Char(' '))),
            vec![Event::SwitchSuspend]
        );
        assert_eq!(key(Field::Suspend, plain(KeyCode::Char('x'))), vec![]);
        assert_eq!(
            key(Field::Suspend, plain(KeyCode::Tab)),
            vec![Event::BreakpointField(Field::Condition)]
        );
        assert_eq!(
            key(Field::Condition, plain(KeyCode::Up)),
            vec![Event::BreakpointField(Field::Suspend)]
        );
        assert_eq!(
            key(
                Field::HitCount,
                plain(KeyCode::Tab).modifiers(KeyModifiers::SHIFT)
            ),
            vec![Event::BreakpointField(Field::Condition)]
        );
    }

    #[test]
    fn a_paste_reaches_a_child_whole_and_varde_s_own_panes_as_keystrokes() {
        let hosted = State {
            ai_running: true,
            ..focused(Pane::Ai)
        };
        assert!(matches!(
            on_paste(&hosted, &Drafts::default(), "one\ntwo".to_string()),
            Pasted::ToChild(Event::Pasted(text)) if text == "one\ntwo"
        ));
        // The tree, rather than the editor: the open buffer takes a paste as
        // one edit, and every other pane of Varde's own interprets the keys.
        let Pasted::AsKeys(keys) = on_paste(
            &focused(Pane::Tree),
            &Drafts::default(),
            "a\r\nb\t".to_string(),
        ) else {
            panic!("Varde's own panes interpret their keys");
        };
        assert_eq!(
            keys.iter().map(|key| key.code).collect::<Vec<_>>(),
            vec![
                KeyCode::Char('a'),
                KeyCode::Enter,
                KeyCode::Char('b'),
                KeyCode::Tab
            ]
        );
    }

    #[test]
    fn undo_and_redo_have_a_modifier_spelling_in_either_mode() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        let shift = |event: KeyEvent| event.modifiers(event.modifiers | KeyModifiers::SHIFT);
        for state in [editing(), inserting.clone()] {
            for undo in [ctrl('z'), cmd('z')] {
                assert_eq!(press(&state, undo), vec![Event::EditorUndo]);
                assert_eq!(press(&state, shift(undo)), vec![Event::EditorRedo]);
            }
        }
        assert_eq!(press(&editing(), plain('U')), vec![Event::EditorKey('U')]);
        assert_eq!(press(&inserting, plain('u')), vec![Event::EditorKey('u')]);
    }

    // A paste into the open buffer is one edit, not the keystrokes it is
    // everywhere else: replayed character by character, `Buffer::insert` would
    // pair every bracket in what was pasted, and in normal mode the letters
    // would be spent as commands and the line breaks as auto-indenting Enters.
    #[test]
    fn a_paste_into_the_open_buffer_is_one_edit_whatever_mode_it_is_in() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        for state in [inserting.clone(), editing()] {
            assert!(matches!(
                on_paste(&state, &Drafts::default(), "foo('bar".to_string()),
                Pasted::ToBuffer(Event::EditorPaste(text)) if text == "foo('bar"
            ));
        }
        // A carriage return is a line ending, not a character to write into the
        // file: the keystroke path turns one into the same Enter a newline is.
        assert!(matches!(
            on_paste(&inserting, &Drafts::default(), "a\r\nb\rc".to_string()),
            Pasted::ToBuffer(Event::EditorPaste(text)) if text == "a\nb\nc"
        ));
        // Which view is on screen decides nothing: a buffer is typed into in
        // Story view too, and a paste there would otherwise be the keystrokes
        // that pair every bracket in it.
        let elsewhere = State {
            view: View::Story,
            ..inserting.clone()
        };
        assert!(matches!(
            on_paste(&elsewhere, &Drafts::default(), "foo('bar".to_string()),
            Pasted::ToBuffer(_)
        ));
    }

    /// Tab lays down indentation exactly where a typed character would be
    /// text, and nowhere else. The states that decide it have no scenario of
    /// their own: a diff and a walked Site answer the editor's keys ahead of
    /// the buffer, so a Tab that reached it would edit a file being read.
    #[test]
    fn tab_indents_only_where_a_typed_character_would_reach_the_buffer() {
        let tab = KeyEvent::new(KeyCode::Tab);
        let normal = State {
            tab_width: 2,
            ..editing()
        };
        assert_eq!(press(&normal, tab), vec![]);
        let inserting = crate::update(&normal, Event::EditorKey('i')).0;
        assert_eq!(
            press(&inserting, tab),
            vec![Event::EditorPaste("  ".to_string())]
        );
        for claimed in [
            State {
                diff: Some(vec![]),
                ..inserting.clone()
            },
            State {
                walking: Some(story::Walking::Story {
                    story: 0,
                    step: 0,
                    diff: story::Diff::Hidden,
                }),
                ..inserting.clone()
            },
        ] {
            assert_eq!(press(&claimed, tab), vec![]);
        }
        // A snippet's remaining stops keep the key, which is what makes the
        // ordering a consequence of the modal rather than a rule of its own.
        assert_eq!(
            press(&filling_in_a_snippet(&inserting), tab),
            vec![Event::NextStop]
        );
        // A candidate list is offered *while* a name is being typed and passes
        // on every key it did not claim, so Tab indents behind it. Excluding it
        // is the hole that asking `buffer_takes_paste` here would have left.
        let offering = State {
            modal: offering_candidates().modal,
            ..inserting.clone()
        };
        assert_eq!(
            press(&offering, tab),
            vec![Event::EditorPaste("  ".to_string())]
        );
        // The comment box routes its own keys and never reaches `routed`, so
        // Tab is unbound in it. Asserted rather than assumed: `buffer_takes_paste`
        // claims that box, which reads as though this indented it.
        let mut picked = Drafts {
            comment_kind: "NOTE".to_string(),
            ..Drafts::default()
        };
        let box_up = State {
            modal: crate::Modal::Comment,
            ..inserting.clone()
        };
        assert_eq!(on_key_event(&box_up, &mut picked, tab, 0), vec![]);
        // A width of nothing binds nothing: an empty paste is still an edit,
        // so it would dirty the buffer and cost an `u` for no visible text.
        let zero = State {
            tab_width: 0,
            ..inserting
        };
        assert_eq!(press(&zero, tab), vec![]);
    }

    /// Alt+Backspace takes a word exactly where a typed character would be
    /// text, and nowhere else — the same contract Tab is held to above, and the
    /// same states, since both read [`typing_into_the_buffer`]. None of them
    /// has a scenario: a diff and a walked Site answer the editor's keys ahead
    /// of the buffer, so a word delete that reached it would edit a file being
    /// read.
    ///
    /// Normal mode is the arm the ticket named: `d` and `b` are the operator
    /// there, and the key keeps the plain backspace it has always been.
    #[test]
    fn alt_backspace_takes_a_word_only_where_a_typed_character_would_reach_the_buffer() {
        let alt_backspace = KeyEvent::new(KeyCode::Backspace).modifiers(KeyModifiers::ALT);
        let normal = editing();
        assert_eq!(press(&normal, alt_backspace), vec![Event::EditorBackspace]);
        let inserting = crate::update(&normal, Event::EditorKey('i')).0;
        assert_eq!(
            press(&inserting, alt_backspace),
            vec![Event::EditorDeleteWord]
        );
        // A bare Backspace is untouched in both modes: the word delete is the
        // modifier, not the key.
        for state in [&normal, &inserting] {
            assert_eq!(
                press(state, KeyEvent::new(KeyCode::Backspace)),
                vec![Event::EditorBackspace]
            );
        }
        for claimed in [
            State {
                diff: Some(vec![]),
                ..inserting.clone()
            },
            State {
                walking: Some(story::Walking::Story {
                    story: 0,
                    step: 0,
                    diff: story::Diff::Hidden,
                }),
                ..inserting.clone()
            },
        ] {
            assert_eq!(press(&claimed, alt_backspace), vec![Event::EditorBackspace]);
        }
        // A line Varde is collecting claims the key wherever focus is, so
        // Alt+Backspace shortens the command rather than the buffer behind it.
        let mut typing_a_command = Drafts {
            command: Some(":wq".to_string()),
            ..Drafts::default()
        };
        assert_eq!(
            on_key_event(&inserting, &mut typing_a_command, alt_backspace, 0),
            vec![]
        );
        assert_eq!(typing_a_command.command.as_deref(), Some(":w"));
        // A hosted pane's child gets the key itself. Held here as well as in
        // the sweep above because it is the one place the modifier could have
        // been claimed out from under a CLI that binds it.
        for pane in [Pane::Terminal, Pane::Ai] {
            assert!(reached_the_child(&hosting(pane), alt_backspace));
        }
    }

    // Wherever something ahead of the buffer claims the editor's keys, a paste
    // is still those keys: a diff, a walkthrough and a Preview all answer
    // `EditorKey` themselves, and a paste that reached the buffer instead would
    // edit a file being read rather than navigating it. Insert mode used to
    // stand in for this, a Preview having no way into it — so the three are
    // asked about here now that the mode is not what decides.
    #[test]
    fn a_paste_is_not_the_buffers_where_something_else_claims_the_editors_keys() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        let reviewing = State {
            diff: Some(vec![]),
            ..inserting.clone()
        };
        assert!(matches!(
            on_paste(&reviewing, &Drafts::default(), "foo".to_string()),
            Pasted::AsKeys(_)
        ));
        let walking = State {
            walking: Some(story::Walking::Story {
                story: 0,
                step: 0,
                diff: story::Diff::Hidden,
            }),
            ..inserting.clone()
        };
        assert!(matches!(
            on_paste(&walking, &Drafts::default(), "foo".to_string()),
            Pasted::AsKeys(_)
        ));
        let mut rendered = inserting;
        let path = rendered
            .current_buffer
            .clone()
            .expect("the buffer `editing` opened");
        rendered
            .buffers
            .get_mut(&path)
            .expect("the buffer `editing` opened")
            .previewing = true;
        assert!(matches!(
            on_paste(&rendered, &Drafts::default(), "foo".to_string()),
            Pasted::AsKeys(_)
        ));
    }

    // A line Varde is collecting claims a paste wherever focus happens to be —
    // otherwise pasting a path into the filter box types it into the shell.
    #[test]
    fn a_paste_into_a_line_varde_is_collecting_never_reaches_the_child() {
        let drafts = Drafts {
            filter: Some(String::new()),
            ..Drafts::default()
        };
        assert!(matches!(
            on_paste(&focused(Pane::Terminal), &drafts, "src".to_string()),
            Pasted::AsKeys(_)
        ));
    }

    // The bug this module exists for: keys reached the terminal whatever had
    // focus, so typing in the editor ran text into the shell.
    #[test]
    fn keys_go_to_the_focused_pane() {
        assert_eq!(
            press(&focused(Pane::Editor), plain('x')),
            vec![Event::EditorKey('x')]
        );
        assert_eq!(
            press(&focused(Pane::Terminal), plain('x')),
            vec![Event::Bytes(b"x".to_vec())]
        );
        assert_eq!(
            press(&focused(Pane::Tree), plain('x')),
            vec![Event::Key('x')]
        );
    }

    #[test]
    fn an_idle_ai_pane_swallows_keys_rather_than_running_them_in_the_shell() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Ai);
        assert!(on_key_event(&state, &mut drafts, plain('c'), 0).is_empty());
        assert_eq!(drafts.ai, "c", "the keystroke belongs to the start box");
    }

    #[test]
    fn a_running_ai_pane_receives_keys() {
        let state = State {
            focus: Pane::Ai,
            ai_running: true,
            ..State::default()
        };
        assert_eq!(press(&state, plain('c')), vec![Event::Bytes(b"c".to_vec())]);
    }

    #[test]
    fn alt_letters_move_focus_and_alt_arrows_move_by_word() {
        assert_eq!(
            press(&focused(Pane::Editor), alt('h')),
            vec![Event::MoveFocus(Direction::Left)]
        );
        assert_eq!(
            press(
                &focused(Pane::Editor),
                arrow(Direction::Left, KeyModifiers::ALT)
            ),
            vec![Event::EditorWord(Direction::Left)]
        );
        assert_eq!(
            press(
                &focused(Pane::Editor),
                arrow(Direction::Right, KeyModifiers::ALT)
            ),
            vec![Event::EditorWord(Direction::Right)]
        );
        // A word motion has no vertical direction, and no other pane has words.
        assert!(press(
            &focused(Pane::Editor),
            arrow(Direction::Up, KeyModifiers::ALT)
        )
        .is_empty());
        assert!(press(
            &focused(Pane::Tree),
            arrow(Direction::Left, KeyModifiers::ALT)
        )
        .is_empty());
        // Ctrl is not part of the gesture: a key carrying it is a different key,
        // and moving focus on it would take Ctrl+Alt from whatever is bound to
        // it in the pane the focus left.
        assert!(press(
            &focused(Pane::Editor),
            KeyEvent::new(KeyCode::Char('h')).modifiers(KeyModifiers::CTRL | KeyModifiers::ALT)
        )
        .is_empty());
    }

    /// Option+arrow does not arrive as Alt+arrow on macOS: the terminal sends
    /// the readline escapes instead, which is why the arrow binding above did
    /// nothing for the person who asked for word jumping while inserting.
    #[test]
    fn the_readline_word_escapes_move_by_word_too() {
        assert_eq!(
            press(&focused(Pane::Editor), alt('b')),
            vec![Event::EditorWord(Direction::Left)]
        );
        assert_eq!(
            press(&focused(Pane::Editor), alt('f')),
            vec![Event::EditorWord(Direction::Right)]
        );
        // No other pane has words, and a hosted pane's child gets the escape
        // itself — the sweep below holds it to that.
        assert!(press(&focused(Pane::Tree), alt('b')).is_empty());
    }

    /// `K` asks the language server what the symbol under the cursor is, and
    /// reaches the buffer as an editor key so that inserting a capital still
    /// types one — the decision `update` makes on the mode. Here rather than in
    /// the binary, which has no test, and Shift's own spelling is driven too
    /// because `S-k` is how a keyboard actually sends it.
    #[test]
    fn asking_what_a_symbol_is_is_an_editor_key() {
        for event in [plain('K'), plain('k').modifiers(KeyModifiers::SHIFT)] {
            assert_eq!(
                press(&focused(Pane::Editor), event),
                vec![Event::EditorKey('K')]
            );
        }
        // No other pane holds a buffer to ask about, so nothing there sends it
        // as an editor key.
        assert_ne!(
            press(&focused(Pane::Tree), plain('K')),
            vec![Event::EditorKey('K')]
        );
    }

    /// With the keyboard in a Hover, `j`/`k` scroll it, Escape
    /// leaves it, and every other key is swallowed rather than typed into the
    /// code the box covers.
    #[test]
    fn the_keyboard_in_a_hover_scrolls_it_and_nothing_else() {
        let state = State {
            hover: Some(crate::lsp::Hover {
                lines: Vec::new(),
                from: 2,
                asked: crate::lsp::Ask {
                    path: std::path::PathBuf::from("/w/one.rs"),
                    place: crate::Place { line: 1, column: 1 },
                    revision: 0,
                    about: crate::lsp::About::Hover,
                },
                first: 0,
                focused: true,
                value: None,
            }),
            ..editing()
        };
        let down = vec![Event::ScrollHover(Direction::Down)];
        let up = vec![Event::ScrollHover(Direction::Up)];
        assert_eq!(press(&state, plain('j')), down);
        assert_eq!(press(&state, plain('k')), up);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Esc)),
            vec![Event::Cancel]
        );
        assert!(press(&state, plain('x')).is_empty(), "x reached the buffer");
    }

    // Shift is not a mode key: the editor extends, and no other pane Varde
    // interprets claims it. A hosted pane is not in here because its child gets
    // the arrow like any other key, which the sweep below holds it to.
    #[test]
    fn shift_with_an_arrow_extends_in_the_editor_only() {
        assert_eq!(
            press(
                &focused(Pane::Editor),
                arrow(Direction::Right, KeyModifiers::SHIFT)
            ),
            vec![Event::EditorExtend(Direction::Right)]
        );
        assert!(press(
            &focused(Pane::Tree),
            arrow(Direction::Right, KeyModifiers::SHIFT)
        )
        .is_empty());
    }

    #[test]
    fn shift_and_alt_with_an_arrow_extends_by_word() {
        assert_eq!(
            press(
                &focused(Pane::Editor),
                arrow(Direction::Left, KeyModifiers::SHIFT | KeyModifiers::ALT)
            ),
            vec![Event::EditorExtendWord(Direction::Left)]
        );
        // A word motion has no vertical direction, and no other pane extends.
        assert!(press(
            &focused(Pane::Editor),
            arrow(Direction::Up, KeyModifiers::SHIFT | KeyModifiers::ALT)
        )
        .is_empty());
        assert!(press(
            &focused(Pane::Tree),
            arrow(Direction::Right, KeyModifiers::SHIFT | KeyModifiers::ALT)
        )
        .is_empty());
    }

    #[test]
    fn a_modal_claims_the_keys_belonging_to_it() {
        let palette = State {
            modal: Modal::Palette,
            focus: Pane::Terminal,
            ..State::default()
        };
        // Terminal focus does not win while the palette is open.
        assert_eq!(press(&palette, plain('r')), vec![Event::Key('r')]);
    }

    #[test]
    fn the_comment_picker_only_accepts_the_four_types() {
        let state = State {
            modal: Modal::Comment,
            ..State::default()
        };
        let mut drafts = Drafts::default();
        on_key_event(&state, &mut drafts, plain('z'), 0);
        assert!(
            drafts.comment_kind.is_empty(),
            "a stray key must not pick a type"
        );
        on_key_event(&state, &mut drafts, plain('i'), 0);
        assert_eq!(drafts.comment_kind, "ISSUE");
        // With a type chosen, further keys are the body — which is a Buffer in
        // `State::comment`, not a string here, so a body key is an editor event
        // and this module stops interpreting text altogether.
        assert_eq!(
            on_key_event(&state, &mut drafts, plain('z'), 0),
            vec![Event::EditorKey('z')]
        );
    }

    /// Word motion in the body, in both the shapes a terminal sends it: a
    /// modified arrow, and the readline escapes macOS sends for Option+arrow.
    /// The body only answered the first, so the motion silently did not exist
    /// on the machine this was written on — the same defect `word_motion_alias`
    /// exists to prevent in the editor, repeated one box over.
    #[test]
    fn both_spellings_of_word_motion_reach_the_comment_body() {
        let showing = State {
            modal: crate::Modal::Comment,
            ..editing()
        };
        let motion = |event| {
            let mut drafts = Drafts {
                comment_kind: "ISSUE".to_string(),
                ..Drafts::default()
            };
            on_key_event(&showing, &mut drafts, event, 0)
        };
        let alt = |code| KeyEvent::new(code).modifiers(KeyModifiers::ALT);
        for back in [alt(KeyCode::Left), alt(KeyCode::Char('b'))] {
            assert_eq!(motion(back), vec![Event::EditorWord(Direction::Left)]);
        }
        for forward in [alt(KeyCode::Right), alt(KeyCode::Char('f'))] {
            assert_eq!(motion(forward), vec![Event::EditorWord(Direction::Right)]);
        }
        // Without Alt they are letters of the comment, as every other key is.
        assert_eq!(
            motion(KeyEvent::new(KeyCode::Char('b'))),
            vec![Event::EditorKey('b')]
        );
    }

    /// The same contract [`CHEATSHEET`] is held to, for the keys that exist only
    /// while a comment's body is being typed. Like the results box and unlike
    /// Tools, this box passes typing through: every printable key is a
    /// character of the comment and every arrow moves the caret, so what the
    /// footer must name is what means something *other* than writing. There are
    /// exactly two — the key that files and the key that discards — and filing
    /// is the one gesture a reviewer has no other route to, which is what makes
    /// a footer nobody prunes a comment nobody can file.
    #[test]
    fn the_comment_box_answers_exactly_the_keys_its_footer_names() {
        let showing = State {
            modal: crate::Modal::Comment,
            ..editing()
        };
        // A type already picked: before that the box is a question, and the
        // four letters that answer it are held by the test above.
        let picked = Drafts {
            comment_kind: "ISSUE".to_string(),
            ..Drafts::default()
        };
        // Writing is not a gesture: it is what every key the box does not claim
        // already does. A newline is writing too — that is the whole point of
        // filing needing a key of its own — and so is taking a word back, which
        // is the same category as the character `Bksp` takes and not something a
        // footer of two rows should spend a third on. The omissions list cannot
        // carry it either: `M-Bksp` may only name views the sweep sees it answer
        // in, and Review view with no box open is not one.
        let gesture = |event| {
            let mut drafts = picked.clone();
            let events = on_key_event(&showing, &mut drafts, event, 0);
            !events.is_empty()
                && !events.iter().all(|e| {
                    matches!(
                        e,
                        Event::EditorKey(_)
                            | Event::EditorArrow(_)
                            | Event::EditorWord(_)
                            | Event::EditorBackspace
                            | Event::EditorDeleteWord
                    )
                })
        };
        let named = |label: &str| super::COMMENT_BOX_KEYS.iter().any(|(key, _)| *key == label);
        for (key, word) in super::COMMENT_BOX_KEYS {
            let event = every_key()
                .into_iter()
                .find(|event| label(*event) == key)
                .unwrap_or_else(|| panic!("no key spells {key}"));
            assert!(
                gesture(event),
                "the footer offers {key} for {word} and the body does nothing with it"
            );
        }
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| gesture(*event))
            .map(label)
            .filter(|label| {
                !named(label)
                    // Claimed before the box and held by the cheatsheet where
                    // every view answers them.
                    && !listed(&State::default(), label, View::Review)
                    && !omitted_in(label, View::Review)
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "the comment box answers keys its footer does not name: {unnamed:?}"
        );
    }

    /// A confirmation that types through is a confirmation that can be answered
    /// by accident, which is the whole point of asking.
    #[test]
    fn the_submit_confirmation_is_answered_rather_than_typed_through() {
        let state = State {
            modal: Modal::ConfirmSubmit,
            focus: Pane::Ai,
            ai_running: true,
            ..State::default()
        };
        assert_eq!(press(&state, plain('y')), vec![Event::ConfirmSubmit]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Enter)),
            vec![Event::ConfirmSubmit]
        );
        assert_eq!(press(&state, plain('n')), vec![Event::Cancel]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Esc)),
            vec![Event::Cancel]
        );
        // A stray key leaves the question standing.
        assert!(press(&state, plain('z')).is_empty());
        // And the question outranks the focused pane's child, which would
        // otherwise be typed at rather than answered.
        assert_eq!(
            on_key_event(
                &state,
                &mut Drafts::default(),
                KeyEvent::new(KeyCode::Char('y')),
                0
            ),
            vec![Event::ConfirmSubmit]
        );
    }

    /// The same reasoning as the submit confirmation: answered, not typed
    /// through, so a stray keystroke cannot author minutes of AI time by
    /// accident.
    #[test]
    fn the_story_confirmation_is_answered_rather_than_typed_through() {
        let state = State {
            modal: Modal::ConfirmStory {
                spelling: "main..HEAD".to_string(),
                out: ".varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.json".to_string(),
            },
            ..State::default()
        };
        assert_eq!(press(&state, plain('y')), vec![Event::ConfirmStory]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Enter)),
            vec![Event::ConfirmStory]
        );
        assert_eq!(press(&state, plain('n')), vec![Event::Cancel]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Esc)),
            vec![Event::Cancel]
        );
        assert!(press(&state, plain('z')).is_empty());
    }

    #[test]
    fn enter_does_nothing_until_a_comment_type_is_picked() {
        let state = State {
            modal: Modal::Comment,
            ..State::default()
        };
        let mut drafts = Drafts::default();
        assert!(on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0).is_empty());
    }

    #[test]
    fn colon_opens_the_command_line_only_in_panes_varde_owns() {
        for pane in [
            Pane::Tree,
            Pane::Editor,
            Pane::Risk,
            Pane::Buffers,
            Pane::History,
        ] {
            let mut drafts = Drafts::default();
            on_key_event(&focused(pane), &mut drafts, plain(':'), 0);
            assert_eq!(
                drafts.command.as_deref(),
                Some(""),
                "{pane:?} should open it"
            );
        }
        let mut drafts = Drafts::default();
        let events = on_key_event(&focused(Pane::Terminal), &mut drafts, plain(':'), 0);
        assert!(drafts.command.is_none(), "the shell keeps its colon");
        assert_eq!(events, vec![Event::Bytes(b":".to_vec())]);
    }

    /// A buffer left in insert mode is not the tree's business: focus moves
    /// away without the mode changing, and the pane that has focus is the one
    /// whose colon is being asked about.
    #[test]
    fn an_inserting_buffer_keeps_only_the_editor_s_colon() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        let mut drafts = Drafts::default();
        assert_eq!(
            on_key_event(&inserting, &mut drafts, plain(':'), 0),
            vec![Event::EditorKey(':')]
        );
        assert!(drafts.command.is_none(), "the buffer keeps its colon");

        let elsewhere = State {
            focus: Pane::Tree,
            ..inserting
        };
        on_key_event(&elsewhere, &mut drafts, plain(':'), 0);
        assert_eq!(drafts.command.as_deref(), Some(""));
    }

    #[test]
    fn q_closes_the_file_and_qa_closes_them_all() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in [':', 'q'] {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::CloseBuffer { force: false }]
        );
        for c in [':', 'q', 'a'] {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::CloseAllBuffers { force: false }]
        );
    }

    /// Typed out in full rather than asserted on `command()` directly, because
    /// the thing worth pinning is that a word this long survives the draft: `q`
    /// and `qa` are prefixes of each other and nothing longer had been decoded.
    #[test]
    fn update_asks_for_a_rebuild() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":update".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::Rebuild]
        );
    }

    /// The box is drawn over the code, and it is the one place `:help` itself
    /// is advertised — so the command that takes it down is pinned here rather
    /// than trusted to the box that will be gone.
    #[test]
    fn help_toggles_the_key_box() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":help".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::ToggleCheatsheet]
        );
    }

    #[test]
    fn ai_takes_a_command_and_a_forcing_variant() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":ai! opencode".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::StartAi {
                command: Some("opencode".to_string()),
                force: true,
            }]
        );
    }

    #[test]
    fn story_takes_an_explicit_range_and_a_forcing_variant() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":story! main..HEAD".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::Story {
                explicit: Some("main..HEAD".to_string()),
                force: true,
            }]
        );
    }

    /// The suffix rather than a subcommand word, so a repository with a branch
    /// called `branch` cannot shadow the command that lists it.
    #[test]
    fn a_question_mark_asks_for_the_branch_picker() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":story?".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::PickBranch(None)]
        );
    }

    /// A URL after the question mark is a Guest repo (ADR 0015). Pasted, so
    /// the surrounding whitespace a paste carries is not part of it.
    #[test]
    fn a_url_after_the_question_mark_names_a_guest_repo() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":story? git@github.com:them/theirs.git ".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::PickBranch(Some(
                "git@github.com:them/theirs.git".to_string()
            ))]
        );
    }

    #[test]
    fn a_bare_story_resolves_offline() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Editor);
        for c in ":story".chars() {
            on_key_event(&state, &mut drafts, plain(c), 0);
        }
        assert_eq!(
            on_key_event(&state, &mut drafts, KeyEvent::new(KeyCode::Enter), 0),
            vec![Event::Story {
                explicit: None,
                force: false,
            }]
        );
    }

    #[test]
    fn the_filter_claims_the_trees_keys_while_it_is_open() {
        let mut drafts = Drafts::default();
        let state = focused(Pane::Tree);
        on_key_event(&state, &mut drafts, plain('/'), 0);
        let events = on_key_event(&state, &mut drafts, plain('n'), 0);
        assert_eq!(events, vec![Event::Filter("n".to_string())]);
        assert_eq!(
            drafts.filter.as_deref(),
            Some("n"),
            "not a new-file shortcut"
        );
    }

    /// The four keys the candidate list claims, and — the half that makes it a
    /// list offered *while typing* rather than a question — the letters it does
    /// not: a key it swallowed would be a character the reader typed and never
    /// got, which is how `/` once ate a paste.
    ///
    /// Every one of them without a modifier, because Option is not Alt on
    /// macOS unless the terminal is told so and a stock tmux strips the
    /// modifier reports: a modifier-only binding is a binding that silently
    /// does not exist.
    #[test]
    fn the_candidate_list_claims_four_keys_and_gives_back_the_rest() {
        let offering = offering_candidates();
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Down)),
            vec![Event::MoveCandidate(Direction::Down)]
        );
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Up)),
            vec![Event::MoveCandidate(Direction::Up)]
        );
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Enter)),
            vec![Event::AcceptCandidate]
        );
        // `Cancel` and not `EditorEscape`: dismissing the list must not also
        // drop the reader out of insert mode.
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Esc)),
            vec![Event::Cancel]
        );
        // Everything else is still the buffer's.
        assert_eq!(press(&offering, plain('k')), vec![Event::EditorKey('k')]);
        assert_eq!(
            press(&offering, KeyEvent::new(KeyCode::Backspace)),
            vec![Event::EditorBackspace]
        );
        assert_eq!(press(&offering, ctrl('c')), vec![Event::Copy]);
        // A modifier on an arrow is the buffer's own gesture, not a choice in
        // a one-column list: Shift extends a selection and Alt moves by word.
        assert_eq!(
            press(&offering, arrow(Direction::Down, KeyModifiers::SHIFT)),
            vec![Event::EditorExtend(Direction::Down)]
        );
    }

    // A key the in-file search does not give back is a motion lost: `b` typed
    // into the query must still move back a word once the search has closed.
    #[test]
    fn the_in_file_search_claims_the_editors_keys_only_while_it_is_open() {
        let finding = typing_a_query("");
        assert_eq!(press(&finding, plain('b')), vec![Event::EditorKey('b')]);
        assert_eq!(
            press(&finding, KeyEvent::new(KeyCode::Backspace)),
            vec![Event::EditorBackspace]
        );
        assert_eq!(
            press(&finding, KeyEvent::new(KeyCode::Esc)),
            vec![Event::CloseFind]
        );
        assert_eq!(
            press(&finding, KeyEvent::new(KeyCode::Enter)),
            vec![Event::AcceptFind]
        );
        // Closed, and the editor has its keys back.
        assert_eq!(
            press(&focused(Pane::Editor), plain('b')),
            vec![Event::EditorKey('b')]
        );
        // On, with the keyboard back in the buffer: the same.
        let mut away = typing_a_query("b");
        away.find.as_mut().expect("on").keys = FindKeys::Away;
        assert_eq!(press(&away, plain('b')), vec![Event::EditorKey('b')]);
    }

    fn typing_a_query(query: &str) -> State {
        State {
            find: Some(Find {
                query: crate::editor::Buffer::text_box(query),
                origin: Place { line: 1, column: 1 },
                case: crate::search::Case::Smart,
                keys: FindKeys::Query,
            }),
            ..State::default()
        }
    }

    fn with_find_keys(keys: FindKeys) -> State {
        let mut state = typing_a_query("state");
        state.find.as_mut().expect("on").keys = keys;
        state
    }

    /// Right moves the caret until the query runs out, and then walks onto
    /// the icons; Tab goes there from anywhere in it. Left off the first icon
    /// puts the caret back at the end rather than wherever it was left.
    #[test]
    fn the_icons_are_walked_from_the_end_of_the_query() {
        let right = KeyEvent::new(KeyCode::Right);
        let left = KeyEvent::new(KeyCode::Left);
        let icon = |icon| vec![Event::FindKeys(FindKeys::Icon(icon))];
        let mut middle = typing_a_query("state");
        middle
            .find
            .as_mut()
            .expect("on")
            .query
            .arrow(Direction::Left);
        assert_eq!(
            press(&middle, right),
            vec![Event::EditorArrow(Direction::Right)]
        );
        assert_eq!(
            press(&middle, KeyEvent::new(KeyCode::Tab)),
            icon(FindIcon::Case)
        );
        assert_eq!(press(&typing_a_query("state"), right), icon(FindIcon::Case));
        let on_case = with_find_keys(FindKeys::Icon(FindIcon::Case));
        assert_eq!(press(&on_case, right), icon(FindIcon::Replace));
        assert_eq!(
            press(&on_case, left),
            vec![
                Event::FindKeys(FindKeys::Query),
                Event::QueryEnd(Direction::Right)
            ]
        );
        assert_eq!(
            press(&on_case, KeyEvent::new(KeyCode::Enter)),
            vec![Event::ToggleCase]
        );
        let on_last = with_find_keys(FindKeys::Icon(FindIcon::ReplaceAll));
        assert_eq!(press(&on_last, right), icon(FindIcon::ReplaceAll));
        assert_eq!(press(&on_last, left), icon(FindIcon::Replace));
        for back in [KeyCode::Up, KeyCode::Esc] {
            assert_eq!(
                press(&on_last, KeyEvent::new(back)),
                vec![Event::FindKeys(FindKeys::Query)]
            );
        }
        assert_eq!(
            press(&on_last, KeyEvent::new(KeyCode::Enter)),
            vec![Event::FindKeys(FindKeys::Replace(ReplaceField::With))]
        );
    }

    /// What the replace box's footer names is what it answers, from the field
    /// the box opens on.
    #[test]
    fn the_replace_box_answers_the_keys_its_footer_names() {
        let with = with_find_keys(FindKeys::Replace(ReplaceField::With));
        for (key, word) in super::REPLACE_BOX_KEYS {
            let code = match key {
                "Tab" => KeyCode::Tab,
                "Enter" => KeyCode::Enter,
                "Esc" => KeyCode::Esc,
                other => panic!("no key spells {other}"),
            };
            assert!(
                !press(&with, KeyEvent::new(code)).is_empty(),
                "the box offers {key} for {word} and does nothing with it"
            );
        }
        let shift_tab = KeyEvent::new(KeyCode::Tab).modifiers(KeyModifiers::SHIFT);
        assert_eq!(
            press(&with, shift_tab),
            vec![Event::FindKeys(FindKeys::Replace(ReplaceField::Find))]
        );
        assert_eq!(press(&with, plain('n')), vec![Event::EditorKey('n')]);
        let on_all = with_find_keys(FindKeys::Replace(ReplaceField::ReplaceAll));
        assert_eq!(
            press(&on_all, KeyEvent::new(KeyCode::Enter)),
            vec![Event::ReplaceAll]
        );
        assert_eq!(
            press(&on_all, plain('n')),
            vec![Event::StepMatch(Direction::Right)]
        );
        assert_eq!(
            press(&on_all, KeyEvent::new(KeyCode::Tab)),
            vec![Event::FindKeys(FindKeys::Replace(ReplaceField::Find))]
        );
    }

    /// Both queries are a buffer with a caret, so the keys that move through
    /// text in the comment body move through a query too — Option+arrow in the
    /// shape macOS sends it included. Up and down stay the results box's.
    #[test]
    fn a_search_query_answers_the_keys_that_move_through_text() {
        // Mid-query, since Right at the end of one walks onto its icons.
        let mut finding = typing_a_query("ab");
        finding
            .find
            .as_mut()
            .expect("on")
            .query
            .arrow(Direction::Left);
        let searching = State {
            search: Some(crate::Search::default()),
            ..State::default()
        };
        let key = |code| KeyEvent::new(code);
        for state in [&finding, &searching] {
            for (event, expected) in [
                (key(KeyCode::Left), Event::EditorArrow(Direction::Left)),
                (key(KeyCode::Right), Event::EditorArrow(Direction::Right)),
                (key(KeyCode::Home), Event::QueryEnd(Direction::Left)),
                (key(KeyCode::End), Event::QueryEnd(Direction::Right)),
                (alt('b'), Event::EditorWord(Direction::Left)),
                (alt('f'), Event::EditorWord(Direction::Right)),
                (
                    key(KeyCode::Left).modifiers(KeyModifiers::ALT),
                    Event::EditorWord(Direction::Left),
                ),
                (
                    key(KeyCode::Backspace).modifiers(KeyModifiers::ALT),
                    Event::EditorDeleteWord,
                ),
            ] {
                assert_eq!(press(state, event), vec![expected], "{event:?}");
            }
        }
        assert_eq!(
            press(&searching, key(KeyCode::Down)),
            vec![Event::MoveHit(Direction::Down)]
        );
    }

    /// The gestures the results box adds to the arrows: file by file, so a
    /// file whose hits fill the box is one keystroke rather than thirty. A
    /// modifier for the reason `Ctrl+O` takes one — a bare `n` is a letter of
    /// the query — with the arrows as the modifier-free way through the hits.
    #[test]
    fn the_results_box_steps_file_by_file_on_ctrl_n_and_ctrl_p() {
        let state = State {
            search: Some(crate::Search::default()),
            ..State::default()
        };
        assert_eq!(
            press(&state, ctrl('n')),
            vec![Event::MoveHitFile(Direction::Down)]
        );
        assert_eq!(
            press(&state, ctrl('p')),
            vec![Event::MoveHitFile(Direction::Up)]
        );
        // The letters themselves still belong to the query.
        assert_eq!(press(&state, plain('n')), vec![Event::EditorKey('n')]);
    }

    #[test]
    fn search_claims_every_printable_key_for_its_query() {
        let state = State {
            search: Some(crate::Search::default()),
            focus: Pane::Terminal,
            ..State::default()
        };
        // Terminal focus does not win while search is showing.
        assert_eq!(press(&state, plain('a')), vec![Event::EditorKey('a')]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Backspace)),
            vec![Event::EditorBackspace]
        );
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Esc)),
            vec![Event::CloseSearch]
        );
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Enter)),
            vec![Event::OpenHit]
        );
        assert_eq!(press(&state, ctrl('o')), vec![Event::OpenEveryHit]);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::Tab)),
            vec![Event::CompleteSearch]
        );
    }

    #[test]
    fn ctrl_f_opens_search_from_the_panes_varde_owns() {
        for pane in [Pane::Editor, Pane::Tree] {
            assert_eq!(press(&focused(pane), ctrl('f')), vec![Event::OpenSearch]);
            assert_eq!(press(&focused(pane), cmd('f')), vec![Event::OpenSearch]);
            // `C-S-f` is not `C-f`. The shift a terminal reports separately is
            // applied on the way in, so a binding reads the key that was
            // pressed rather than the one underneath it.
            assert!(press(
                &focused(pane),
                KeyEvent::new(KeyCode::Char('f'))
                    .modifiers(KeyModifiers::CTRL | KeyModifiers::SHIFT)
            )
            .is_empty());
        }
    }

    /// The reserved list is exhaustive, so Varde's other global keys are not on
    /// it: in a hosted pane they belong to the child, which may well bind them,
    /// and Varde's own copy of each is reachable from the panes it owns.
    #[test]
    fn a_global_key_that_is_not_reserved_reaches_the_child() {
        let state = hosting(Pane::Terminal);
        for key in [ctrl('f'), ctrl('q'), ctrl('k'), ctrl('l')] {
            assert!(
                matches!(press(&state, key).as_slice(), [Event::Bytes(bytes)] if !bytes.is_empty()),
                "{key:?} was withheld from the child"
            );
        }
    }

    // Copying gets a direct key, but the shell keeps its interrupt: Ctrl+C with
    // the terminal focused must still stop a running command.
    #[test]
    fn ctrl_c_copies_in_the_editor_and_still_interrupts_the_shell() {
        assert_eq!(press(&focused(Pane::Editor), ctrl('c')), vec![Event::Copy]);
        assert_eq!(
            press(&focused(Pane::Terminal), ctrl('c')),
            vec![Event::Bytes(vec![3])]
        );
    }

    #[test]
    fn cmd_d_takes_the_next_occurrence_as_ctrl_d_does() {
        let editor = focused(Pane::Editor);
        assert_eq!(press(&editor, ctrl('d')), vec![Event::EditorNextOccurrence]);
        assert_eq!(press(&editor, cmd('d')), press(&editor, ctrl('d')));
    }

    // Command is Ctrl's alias on the write key as on copy and paste, and the
    // comment box keeps its own Ctrl+S: it files the comment, never the file.
    #[test]
    fn ctrl_s_and_cmd_s_write_the_buffer_unless_the_comment_box_is_open() {
        let editor = focused(Pane::Editor);
        assert_eq!(press(&editor, ctrl('s')), vec![Event::WriteBuffer]);
        assert_eq!(press(&editor, cmd('s')), press(&editor, ctrl('s')));
        let commenting = State {
            modal: crate::Modal::Comment,
            ..editor
        };
        let mut drafts = Drafts {
            comment_kind: "ISSUE".to_string(),
            ..Drafts::default()
        };
        assert_eq!(
            on_key_event(&commenting, &mut drafts, ctrl('s'), 0),
            vec![Event::FileComment {
                kind: "ISSUE".to_string()
            }]
        );
    }

    /// The way out of a hosted pane on a terminal that reports neither a bare
    /// Ctrl press nor Alt. It is on no reserved list because it withholds
    /// nothing: the escape reaches the child, and the tap is counted beside it.
    #[test]
    fn the_escape_key_is_counted_as_a_tap_and_still_reaches_the_child() {
        for pane in [Pane::Terminal, Pane::Ai] {
            let state = hosting(pane);
            let pressed = on_key_event(
                &state,
                &mut Drafts::default(),
                KeyEvent::new(KeyCode::Esc),
                120,
            );
            assert!(
                matches!(
                    pressed.as_slice(),
                    [
                        Event::Bytes(bytes),
                        Event::Tapped {
                            key: Tap::Escape,
                            at_ms: 120
                        }
                    ] if !bytes.is_empty()
                ),
                "{pressed:?}"
            );
            // With a modifier it is a key of the child's, not the gesture.
            let with_alt = press(
                &state,
                KeyEvent::new(KeyCode::Esc).modifiers(KeyModifiers::ALT),
            );
            assert!(with_alt
                .iter()
                .any(|event| matches!(event, Event::Bytes(_))));
            assert!(!with_alt
                .iter()
                .any(|event| matches!(event, Event::Tapped { .. })));
            // A repeat is one press held down, not the second of a pair.
            let held = press(
                &state,
                KeyEvent {
                    kind: KeyEventKind::Repeat,
                    ..KeyEvent::new(KeyCode::Esc)
                },
            );
            assert!(matches!(held.as_slice(), [Event::Bytes(_)]), "{held:?}");
        }
    }

    /// Everywhere else Escape is Varde's own — it leaves insert mode and
    /// dismisses a modal — so the vim reflex of two quick escapes must not open
    /// the palette. Those panes have the fallback binding instead, and they are
    /// not a pane anything can trap you in.
    #[test]
    fn the_escape_tap_is_a_hosted_pane_gesture_only() {
        for pane in [Pane::Editor, Pane::Tree] {
            assert!(!press(&focused(pane), KeyEvent::new(KeyCode::Esc))
                .iter()
                .any(|event| matches!(event, Event::Tapped { .. })));
        }
    }

    // A drag begins with a button press, which focuses the pane it landed in, so
    // by the time the copy key is pressed a pty pane has focus and Ctrl+C was
    // going to the child as an interrupt — which is why an AI answer could be
    // picked and then not copied.
    #[test]
    fn ctrl_c_copies_a_pty_selection_and_otherwise_still_interrupts() {
        let picked = |pane| State {
            focus: pane,
            ai_running: true,
            selection: Some(Selection::Screen {
                pane,
                from: Place { line: 1, column: 1 },
                to: Place { line: 1, column: 7 },
                text: "ripgrep".to_string(),
            }),
            ..State::default()
        };
        assert_eq!(press(&picked(Pane::Ai), ctrl('c')), vec![Event::Copy]);
        assert_eq!(press(&picked(Pane::Terminal), ctrl('c')), vec![Event::Copy]);
        // Nothing picked: the shell keeps the interrupt that stops a command.
        assert_eq!(
            press(&focused(Pane::Terminal), ctrl('c')),
            vec![Event::Bytes(vec![3])]
        );
        // A running AI CLI keeps it too: interrupting a response is what it is
        // for, and an idle pane swallows keys either way.
        let running = State {
            focus: Pane::Ai,
            ai_running: true,
            ..State::default()
        };
        assert_eq!(press(&running, ctrl('c')), vec![Event::Bytes(vec![3])]);
        // A selection somewhere else — the editor's, or the other hosted
        // pane's — is not this pane's to copy: the shell keeps its interrupt.
        let elsewhere = State {
            focus: Pane::Terminal,
            selection: Some(Selection::Buffer {
                anchor: Place { line: 1, column: 1 },
                cursor: Place { line: 1, column: 3 },
            }),
            ..State::default()
        };
        assert_eq!(press(&elsewhere, ctrl('c')), vec![Event::Bytes(vec![3])]);
        let other_pane = State {
            focus: Pane::Terminal,
            ai_running: true,
            ..picked(Pane::Ai)
        };
        assert_eq!(press(&other_pane, ctrl('c')), vec![Event::Bytes(vec![3])]);
    }

    #[test]
    fn a_bare_ctrl_is_stamped_for_the_double_tap() {
        let state = State::default();
        // A lone Ctrl press, which only a terminal speaking the Kitty protocol
        // reports. Which side of the keyboard it came from is not a binding.
        let bare_ctrl = KeyEvent::new(KeyCode::Modifier(
            ModifierKeyCode::Control,
            ModifierDirection::Unknown,
        ));
        assert_eq!(
            on_key_event(&state, &mut Drafts::default(), bare_ctrl, 450),
            vec![Event::Tapped {
                key: Tap::Ctrl,
                at_ms: 450
            }]
        );
    }

    /// Every key the lossless input type can express, across every combination
    /// of the six modifiers it can carry. Modifiers are enumerated as a bit set
    /// rather than a hand-picked handful because a modifier Varde has no use for
    /// is exactly the kind that used to be discarded on the way in.
    fn every_key() -> Vec<KeyEvent> {
        let mut codes = vec![
            KeyCode::Backspace,
            KeyCode::Enter,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::Delete,
            KeyCode::Insert,
            KeyCode::Esc,
            KeyCode::CapsLock,
            KeyCode::ScrollLock,
            KeyCode::NumLock,
            KeyCode::PrintScreen,
            KeyCode::Pause,
            KeyCode::Menu,
            KeyCode::KeypadBegin,
        ];
        codes.extend((1..=35).map(KeyCode::F));
        codes.extend((0x20u8..0x7f).map(|c| KeyCode::Char(c as char)));
        // A keyboard is not ASCII. These stand in for the rest of Unicode: a
        // letter with a diaeresis, one Varde's own author types daily, a
        // character outside the basic plane, and one outside every plane a
        // single UTF-16 unit can hold.
        codes.extend(['é', 'ø', '漢', '🙂'].map(KeyCode::Char));
        codes.extend(
            [
                MediaKeyCode::Play,
                MediaKeyCode::Pause,
                MediaKeyCode::PlayPause,
                MediaKeyCode::Reverse,
                MediaKeyCode::Stop,
                MediaKeyCode::FastForward,
                MediaKeyCode::Rewind,
                MediaKeyCode::TrackNext,
                MediaKeyCode::TrackPrevious,
                MediaKeyCode::Record,
                MediaKeyCode::LowerVolume,
                MediaKeyCode::RaiseVolume,
                MediaKeyCode::MuteVolume,
            ]
            .map(KeyCode::Media),
        );
        for modifier in [
            ModifierKeyCode::Shift,
            ModifierKeyCode::Control,
            ModifierKeyCode::Alt,
            ModifierKeyCode::Super,
            ModifierKeyCode::Hyper,
            ModifierKeyCode::Meta,
            ModifierKeyCode::IsoLevel3Shift,
            ModifierKeyCode::IsoLevel5Shift,
        ] {
            codes.extend(
                [
                    ModifierDirection::Left,
                    ModifierDirection::Right,
                    ModifierDirection::Unknown,
                ]
                .map(|side| KeyCode::Modifier(modifier, side)),
            );
        }
        let mut keys = Vec::new();
        for code in codes {
            for bits in 0..=KeyModifiers::all().bits() {
                keys.push(KeyEvent::new(code).modifiers(KeyModifiers::from_bits_truncate(bits)));
            }
        }
        keys
    }

    /// The **reserved** keys: the complete list of what Varde claims from a
    /// child, and the only keys it withholds by choice. Each one costs the child
    /// nothing.
    const RESERVED: [(&str, &str); 7] = [
        (
            "C-space",
            "opens the view palette from every pane, hosted ones included, so \
             it is the one gesture that is the same everywhere. The child \
             loses the NUL byte readline reads as set-mark",
        ),
        (
            "Ctrl",
            "double-tapped it opens the view palette, and a lone modifier \
             produces no bytes in a pty, so the child could never have \
             received it",
        ),
        ("M-h", "moves pane focus, on every pane"),
        ("M-j", "moves pane focus"),
        ("M-k", "moves pane focus"),
        ("M-l", "moves pane focus"),
        (
            "C-c with a selection",
            "copies what the mouse picked in Varde's own UI. With nothing \
             picked it interrupts the child, which the sweep below drives and \
             this list therefore does not excuse",
        ),
    ];

    /// Keys no legacy xterm sequence can express, so a real terminal sends
    /// nothing for them either and Varde is taking nothing away. Kept apart from
    /// [`RESERVED`] on purpose: conflating the two is how a key Varde really
    /// does drop could hide among the keys that were never expressible.
    const UNENCODABLE: [(&str, &str); 11] = [
        ("Shift alone", "a bare modifier has no sequence"),
        ("Alt alone", "a bare modifier has no sequence"),
        ("Super alone", "a bare modifier has no sequence"),
        ("Hyper alone", "a bare modifier has no sequence"),
        ("Meta alone", "a bare modifier has no sequence"),
        ("IsoLevel3Shift alone", "a bare modifier has no sequence"),
        ("IsoLevel5Shift alone", "a bare modifier has no sequence"),
        ("a media key", "no legacy sequence exists for it"),
        (
            "a lock or system key",
            "CapsLock, ScrollLock, NumLock, PrintScreen, Pause, Menu and \
             KeypadBegin have no legacy sequence",
        ),
        (
            "F13 and above",
            "legacy xterm stops at F12. The encoder writes a bare CSI \
             introducer above that, which `encode` drops rather than send \
             half a sequence",
        ),
        (
            "C- a character with no control code",
            "the control codes are the letters, space and 4 to 7; Ctrl with a \
             digit or a punctuation mark needs the Kitty protocol the child is \
             deliberately not offered",
        ),
    ];

    /// How a withheld key is written in the lists above. Grouped, because the
    /// reason a key is withheld is a property of its shape and not of the
    /// sixty-four modifier combinations it comes in.
    fn withheld_as(event: KeyEvent) -> String {
        match event.code {
            KeyCode::Modifier(ModifierKeyCode::Control, _) => "Ctrl".to_string(),
            KeyCode::Modifier(modifier, _) => format!("{modifier:?} alone"),
            KeyCode::Media(_) => "a media key".to_string(),
            KeyCode::Char(letter @ ('h' | 'j' | 'k' | 'l'))
                if event.modifiers == KeyModifiers::ALT =>
            {
                format!("M-{letter}")
            }
            KeyCode::F(13..) => "F13 and above".to_string(),
            // Spelled the way [`label`] spells it, so the palette gesture
            // reads alike in the cheatsheet and in the withheld list.
            KeyCode::Char(' ') if event.modifiers.contains(KeyModifiers::CTRL) => {
                "C-space".to_string()
            }
            // Shift and case are folded the way `encode` folds them, so
            // `C-S-A` is classified as the `C-a` it becomes.
            KeyCode::Char(c) if event.modifiers.contains(KeyModifiers::CTRL) => ctrl_withheld(c),
            KeyCode::CapsLock
            | KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin => "a lock or system key".to_string(),
            other => format!("{other:?}"),
        }
    }

    fn ctrl_withheld(c: char) -> String {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_lowercase() || c == ' ' || ('4'..='7').contains(&c) {
            format!("C-{c}")
        } else {
            "C- a character with no control code".to_string()
        }
    }

    fn excused(label: &str) -> bool {
        RESERVED
            .iter()
            .chain(UNENCODABLE.iter())
            .any(|(withheld, _)| withheld == &label)
    }

    /// A hosted pane with a child running and nothing of Varde's own on screen.
    fn hosting(pane: Pane) -> State {
        State {
            focus: pane,
            ai_running: true,
            ..State::default()
        }
    }

    /// Whether a key produced bytes for the child. Deliberately not *which*
    /// bytes: that is the encoding crate's tested concern, and a second copy of
    /// the xterm specification here would drift from the first.
    fn reached_the_child(state: &State, event: KeyEvent) -> bool {
        on_key_event(state, &mut Drafts::default(), event, 0)
            .iter()
            .any(|event| matches!(event, Event::Bytes(bytes) if !bytes.is_empty()))
    }

    /// The reason this ticket exists: the user should not be the mechanism by
    /// which a missing key is discovered. Every key the input type can express,
    /// in every modifier combination, must either reach the child or be on the
    /// list above — so withholding one is a test failure rather than something
    /// somebody finds out by pressing it.
    #[test]
    fn every_key_reaches_a_hosted_pane_or_is_recorded_as_withheld() {
        for pane in [Pane::Terminal, Pane::Ai] {
            let state = hosting(pane);
            let mut missing: Vec<String> = every_key()
                .into_iter()
                .filter(|event| !reached_the_child(&state, *event))
                .map(withheld_as)
                .filter(|label| !excused(label))
                .collect();
            missing.sort();
            missing.dedup();
            assert!(
                missing.is_empty(),
                "{pane:?} withheld keys that are on no list: {missing:?}"
            );
        }
    }

    /// A list nobody prunes is how the contract rots: an entry for a key that
    /// now reaches the child excuses nothing and hides the next real gap.
    #[test]
    fn the_withheld_list_holds_only_keys_that_are_really_withheld() {
        let state = hosting(Pane::Terminal);
        let withheld: Vec<String> = every_key()
            .into_iter()
            .filter(|event| !reached_the_child(&state, *event))
            .map(withheld_as)
            .collect();
        for (label, _) in RESERVED.iter().chain(UNENCODABLE.iter()) {
            // The one entry the sweep cannot show, because the sweep has
            // nothing picked. Its own test is below.
            if *label == "C-c with a selection" {
                continue;
            }
            assert!(
                withheld.iter().any(|found| found == label),
                "{label} is excused as withheld but reaches the child"
            );
        }
    }

    #[test]
    fn each_reserved_key_is_claimed_and_the_child_gets_nothing() {
        for pane in [Pane::Terminal, Pane::Ai] {
            let state = hosting(pane);
            for (letter, direction) in [
                ('h', Direction::Left),
                ('j', Direction::Down),
                ('k', Direction::Up),
                ('l', Direction::Right),
            ] {
                assert_eq!(
                    press(&state, alt(letter)),
                    vec![Event::MoveFocus(direction)]
                );
            }
            assert_eq!(press(&state, ctrl(' ')), vec![Event::FallbackBinding]);
            assert_eq!(
                on_key_event(
                    &state,
                    &mut Drafts::default(),
                    KeyEvent::new(KeyCode::Modifier(
                        ModifierKeyCode::Control,
                        ModifierDirection::Left
                    )),
                    450,
                ),
                vec![Event::Tapped {
                    key: Tap::Ctrl,
                    at_ms: 450
                }]
            );
        }
    }

    // A drag begins with a button press, which focuses the pane it landed in, so
    // a hosted pane has focus by the time the copy key is pressed. With
    // something picked it copies; with nothing picked the child keeps the
    // interrupt, which is what stops a running command.
    #[test]
    fn the_copy_key_copies_with_a_selection_and_interrupts_without_one() {
        let state = hosting(Pane::Ai);
        assert_eq!(press(&state, ctrl('c')), vec![Event::Bytes(vec![3])]);
        let picked = State {
            selection: Some(Selection::Screen {
                pane: Pane::Ai,
                from: Place { line: 1, column: 1 },
                to: Place { line: 1, column: 7 },
                text: "ripgrep".to_string(),
            }),
            ..state
        };
        assert_eq!(press(&picked, ctrl('c')), vec![Event::Copy]);
        // Command copies the same selection, since a reader who copies with it
        // everywhere else is not going to learn a second key for a pty pane.
        assert_eq!(press(&picked, cmd('c')), vec![Event::Copy]);
    }

    /// A control code carries no case and no Shift: a real terminal sends the
    /// same interrupt for all four shapes, and which one it reports is not
    /// Varde's choice. Left to the encoder, the three shifted ones arrive as
    /// nothing at all.
    #[test]
    fn a_shifted_control_key_still_reaches_the_child() {
        let state = hosting(Pane::Terminal);
        for event in [
            ctrl('c'),
            KeyEvent::new(KeyCode::Char('C')).modifiers(KeyModifiers::CTRL),
            KeyEvent::new(KeyCode::Char('c')).modifiers(KeyModifiers::CTRL | KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Char('C')).modifiers(KeyModifiers::CTRL | KeyModifiers::SHIFT),
        ] {
            assert_eq!(
                press(&state, event),
                vec![Event::Bytes(vec![3])],
                "{event:?}"
            );
        }
    }

    /// Half an escape sequence is worse than none: the child reads it as
    /// unfinished and swallows whatever arrives next. The encoder writes a bare
    /// introducer above F12, so those keys send nothing. F12's own bytes are
    /// pinned here as the contrast — a complete sequence, terminator and all.
    #[test]
    fn a_function_key_legacy_xterm_cannot_express_sends_nothing() {
        let state = hosting(Pane::Terminal);
        assert_eq!(
            press(&state, KeyEvent::new(KeyCode::F(12))),
            vec![Event::Bytes(b"\x1b[24~".to_vec())]
        );
        assert!(press(&state, KeyEvent::new(KeyCode::F(13))).is_empty());
    }

    /// The keys Varde's own UI is collecting belong to it wherever focus is:
    /// a modal, either search, and the `:` and filter lines all outrank the
    /// child, and focus can move while one of them is open.
    #[test]
    fn what_varde_is_collecting_outranks_the_child() {
        let showing = State {
            modal: Modal::Palette,
            ..hosting(Pane::Terminal)
        };
        assert_eq!(press(&showing, plain('r')), vec![Event::Key('r')]);
        let mut drafts = Drafts {
            command: Some(String::new()),
            ..Drafts::default()
        };
        let events = on_key_event(&hosting(Pane::Terminal), &mut drafts, plain('q'), 0);
        assert!(events.is_empty(), "the draft is collecting, not the child");
        assert_eq!(drafts.command.as_deref(), Some("q"));
    }

    /// The editor and the tree still read the character off the key, and still
    /// apply the shift a terminal reports separately rather than as the
    /// character it produced.
    #[test]
    fn the_editor_and_the_tree_read_the_character_the_key_typed() {
        assert_eq!(
            press(&focused(Pane::Editor), plain('x')),
            vec![Event::EditorKey('x')]
        );
        assert_eq!(
            press(&focused(Pane::Tree), plain('x')),
            vec![Event::Key('x')]
        );
        assert_eq!(
            press(
                &focused(Pane::Editor),
                KeyEvent::new(KeyCode::Char('n')).modifiers(KeyModifiers::SHIFT)
            ),
            vec![Event::EditorKey('N')]
        );
    }

    /// The Buffers pane is routed by the same arm the Risk list is, so what
    /// this holds is that the *pane* reaches it: the arrows move its selection,
    /// Enter is the go-to-that-buffer gesture, and `j`/`k` arrive as
    /// `Event::Key` for `update` to read. Without it the pane's entry in
    /// `pane_key` and its arms in `arrow_event` are two decisions no test makes.
    #[test]
    fn the_buffers_panes_keys_are_decided_here() {
        let pane = focused(Pane::Buffers);
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Down)),
            vec![Event::MoveSelection(Direction::Down)]
        );
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Up)),
            vec![Event::MoveSelection(Direction::Up)]
        );
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Enter)),
            vec![Event::Activate]
        );
        for key in ['j', 'k'] {
            assert_eq!(press(&pane, plain(key)), vec![Event::Key(key)]);
        }
    }

    /// The Risk list is one of Varde's own panes, so its keys are decided here
    /// and not at the edge: the arrows and the letters both reach the pane's
    /// selection, and Enter is the go-to-the-code gesture. Its letters arrive as
    /// `Event::Key` because what they mean is `update`'s — the same division the
    /// tree's own shortcuts keep.
    #[test]
    fn the_risk_lists_keys_are_decided_here() {
        let pane = focused(Pane::Risk);
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Down)),
            vec![Event::MoveSelection(Direction::Down)]
        );
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Up)),
            vec![Event::MoveSelection(Direction::Up)]
        );
        assert_eq!(
            press(&pane, KeyEvent::new(KeyCode::Enter)),
            vec![Event::Activate]
        );
        for key in ['j', 'k', 'a', 'r', 'l'] {
            assert_eq!(press(&pane, plain(key)), vec![Event::Key(key)]);
        }
        // Right steps into the row's actions and Left back out, the same
        // gesture the tree's rows answer: the Risk row's refactor ask is
        // reachable by keyboard, not by mouse alone.
        for (code, direction) in [
            (KeyCode::Right, Direction::Right),
            (KeyCode::Left, Direction::Left),
        ] {
            assert_eq!(
                press(&pane, KeyEvent::new(code)),
                vec![Event::MoveAction(direction)]
            );
        }
    }

    fn press(state: &State, event: KeyEvent) -> Vec<Event> {
        on_key_event(state, &mut Drafts::default(), event, 0)
    }

    fn plain(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c))
    }

    fn alt(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c)).modifiers(KeyModifiers::ALT)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c)).modifiers(KeyModifiers::CTRL)
    }

    /// Command, which a terminal reports as Super where it reports it at all.
    fn cmd(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c)).modifiers(KeyModifiers::SUPER)
    }

    fn arrow(direction: Direction, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(match direction {
            Direction::Left => KeyCode::Left,
            Direction::Right => KeyCode::Right,
            Direction::Up => KeyCode::Up,
            Direction::Down => KeyCode::Down,
        })
        .modifiers(modifiers)
    }

    /// Keys the editor answers to that the cheatsheet deliberately leaves out,
    /// each held to the views it is excused in — the same dimension
    /// [`CHEATSHEET`] carries, since an omission that claims a view where the
    /// key does nothing is as false a promise as a listing would be. A binding
    /// that answers in a view neither listed nor excused there fails the test
    /// below, so an omission is a decision somebody made rather than something
    /// nobody noticed. Every entry is held to still doing something in every
    /// view it names, so the list cannot quietly outlive the binding it
    /// excuses.
    const UNLISTED: [(&str, &str, &[View]); 24] = [
        (
            "Ctrl",
            "the router still answers a bare Ctrl press with a tap, but no \
             terminal reports one: that needs the keyboard flag whose cost is \
             every character a layout composes, which `main.rs` no longer pays. \
             Listing a gesture nobody can perform teaches the wrong key",
            &[View::Edit, View::Review, View::Story],
        ),
        // `h`, `j` and `k` are cursor motion too, and listed: the Diagnostic
        // list's row names them, since the cheatsheet is drawn per view and
        // not per pane.
        (
            "l",
            "cursor motion — the one thing nobody needs reminding of. Over a \
             Preview it is the same motion, run over the rendered row",
            &[View::Edit],
        ),
        (
            "arr",
            "cursor motion, in every mode — and through a walked Site too, \
             which is read-only about its contents, not about the cursor. \
             With a candidate list up they move the choice instead, which is \
             the same gesture the tree and the Risk list answer and the one \
             thing every list everywhere already teaches",
            &[View::Edit, View::Review, View::Story],
        ),
        ("Home", "an alias of 0, which is listed", &[View::Edit]),
        ("End", "an alias of $, which is listed", &[View::Edit]),
        (
            "S-M-arr",
            "the arrow alias of W and B, which are listed",
            &[View::Edit],
        ),
        (
            "M-b",
            "what the terminal sends for Option+Left on macOS, so it is the \
             word motion b, which is listed",
            &[View::Edit, View::Story],
        ),
        (
            "M-f",
            "what the terminal sends for Option+Right on macOS, so it is the \
             word motion w, which is listed",
            &[View::Edit, View::Story],
        ),
        (
            "M-arr",
            "the arrow alias of the word motions b and w, which are listed — \
             and, like the plain arrows, a motion rather than a key, so it \
             moves while inserting and through a walked Site too",
            &[View::Edit, View::Story],
        ),
        (
            "C-M-arr",
            "Ctrl+Option on the left and right arrows, so it is the jump back \
             and forward that C-p, C-n and gp/gn spell, which are listed. An \
             alias rather than a route of its own, for the reason M-arr is one",
            &[View::Edit, View::Review, View::Story],
        ),
        (
            "1",
            "a count is a rule about other keys, not a key of its own",
            &[View::Edit],
        ),
        ("2", "a count", &[View::Edit]),
        ("3", "a count", &[View::Edit]),
        ("4", "a count", &[View::Edit]),
        ("5", "a count", &[View::Edit]),
        ("6", "a count", &[View::Edit]),
        ("7", "a count", &[View::Edit]),
        ("8", "a count", &[View::Edit]),
        ("9", "a count", &[View::Edit]),
        (
            "Bksp",
            "deletes backwards — an insert-mode key, and the box is hidden there",
            &[View::Edit],
        ),
        (
            "M-Bksp",
            "takes the word behind the cursor while inserting, and the single \
             character Bksp takes everywhere else — both insert-mode keys, and \
             the box is hidden there. Spelled apart from Bksp rather than \
             folded onto it because the router reads the modifier, so it is a \
             gesture rather than a modifier a binding ignores. `db` is its \
             modifier-free route and is listed under the operator's row",
            &[View::Edit],
        ),
        (
            "D-d",
            "a walked Site claims the key before the next occurrence can, so \
             Command on it is the `d` it always was, which is listed",
            &[View::Story],
        ),
        (
            "C-q",
            "quits Varde, from wherever you are",
            &[View::Edit, View::Review, View::Story],
        ),
        (
            ":",
            "opens the command line in every view — only :update, which is \
             listed, answers with nothing open",
            &[View::Review, View::Story],
        ),
    ];

    /// How a key is written in the cheatsheet's left-hand column.
    ///
    /// One spelling per *gesture*, not per key event: the sweep drives all
    /// sixty-four modifier combinations of every code, and a modifier the router
    /// never inspects names no gesture of its own. `Super+x` types the `x` it
    /// always typed, so it is spelled `x` — except on the five keys where the
    /// router does inspect it, `D-c`, `D-v`, `D-s`, `D-d` and `D-f`, which are
    /// Command's own spellings of copy, paste, write, the next occurrence and
    /// project search and are folded
    /// onto no other row for the reason `M-Bksp` is not folded onto `Bksp`. Ctrl
    /// outranks Alt for the same reason — the Ctrl bindings ask `contains(CTRL)`
    /// and never look at Alt, so `C-M-q` is `C-q` carrying a modifier the
    /// binding ignores. Shift is folded the way the router folds it, on the way
    /// in, so `S-a` is the `A` that reached the buffer. [`folded`] is that
    /// dropping written out, and the test beside it is what keeps a group from
    /// hiding a difference.
    ///
    /// Exhaustive over the key codes, which is what the panic it replaces was
    /// approximating: a candidate whose spelling nobody chose would be held to a
    /// label the box never shows, which is the oversight this test exists to
    /// prevent. Naming every code rather than catching the rest means a code the
    /// input crate grows will not compile until somebody spells it.
    fn label(event: KeyEvent) -> String {
        let event = super::shifted(event);
        let ctrl = event.modifiers.contains(KeyModifiers::CTRL);
        let command = event.modifiers.contains(KeyModifiers::SUPER);
        let alt = event.modifiers.contains(KeyModifiers::ALT);
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        match event.code {
            KeyCode::Modifier(ModifierKeyCode::Control, _) => "Ctrl".to_string(),
            // Shapes rather than spellings — these two and the lock-and-system
            // group below — and the same words the hosted-pane sweep's withheld
            // list uses for the same keys, so a key reads alike in both lists. A
            // key inside one of these groups that ever fires needs its own
            // spelling before it can be listed or omitted.
            KeyCode::Modifier(modifier, _) => format!("{modifier:?} alone"),
            KeyCode::Media(_) => "a media key".to_string(),
            KeyCode::Char(' ') if ctrl => "C-space".to_string(),
            // The cheatsheet spells Space as the glyph its chords are written
            // with, because a space cannot be a token of a row.
            KeyCode::Char(' ') => "␣".to_string(),
            // Command is Ctrl's alias on undo and redo, so both fold into the
            // Ctrl gesture, and redo's Shift is spelled rather than read off
            // the `Z` it became.
            KeyCode::Char('z') if ctrl || command => "C-z".to_string(),
            KeyCode::Char('Z') if ctrl || command => "C-S-z".to_string(),
            KeyCode::Char(c) if ctrl => format!("C-{c}"),
            KeyCode::Char(c @ ('c' | 'v' | 's' | 'd' | 'f')) if command => format!("D-{c}"),
            KeyCode::Char(c) if alt => format!("M-{c}"),
            KeyCode::Char(c) => c.to_string(),
            // A gesture of its own since `backspace_word` inspects Alt on it:
            // folded onto `Bksp` it would inherit that row's excuse, which
            // describes a character delete, and the difference this sweep
            // exists to catch would be the one it hid.
            KeyCode::Backspace if alt => "M-Bksp".to_string(),
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                arrow_label(shift, alt, ctrl)
            }
            // Ctrl is inspected on F2, which stops a Debug session, and on F8,
            // which toggles a Breakpoint with it held. Shift is inspected on
            // F8 and nowhere else — held, it steps out where the key alone
            // steps over — so it is a gesture of its own there and a
            // modifier no binding reads on
            // every other function key, exactly as Command is on `c` and `v`.
            KeyCode::F(number) if ctrl => format!("C-F{number}"),
            KeyCode::F(8) if shift => "S-F8".to_string(),
            KeyCode::F(number) => format!("F{number}"),
            code => named_label(code),
        }
    }

    /// Ctrl is a gesture on an arrow only alongside Alt, and only there:
    /// `jump_alias` reads the pair, so `C-M-arr` is a spelling of its own, and
    /// Ctrl on an arrow without it is a modifier no binding inspects.
    fn arrow_label(shift: bool, alt: bool, ctrl: bool) -> String {
        if alt && ctrl {
            return "C-M-arr".to_string();
        }
        match (shift, alt) {
            (true, true) => "S-M-arr".to_string(),
            (true, false) => "S-arr".to_string(),
            (false, true) => "M-arr".to_string(),
            (false, false) => "arr".to_string(),
        }
    }

    /// The keys whose spelling is just their name. Exhaustive over `KeyCode`,
    /// so a code the input crate grows will not compile until somebody spells
    /// it — the last arm names the codes [`label`] already answered rather
    /// than catching them.
    fn named_label(code: KeyCode) -> String {
        match code {
            KeyCode::Backspace => "Bksp".to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::Delete => "Del".to_string(),
            KeyCode::Insert => "Ins".to_string(),
            KeyCode::CapsLock
            | KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin => "a lock or system key".to_string(),
            KeyCode::Modifier(..)
            | KeyCode::Media(_)
            | KeyCode::Char(_)
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::F(_) => unreachable!("{code:?} is spelled by `label`"),
        }
    }

    /// The event a label names, with every modifier and every distinction the
    /// label drops removed. The test below is what makes that dropping a
    /// decision: without it the fold would hide a divergence instead of naming
    /// one, and `C-M-Left` is exactly the shape that would hide — a live word
    /// motion, because the editor's Alt-arrow arm never looks at Ctrl, while
    /// `C-M-h` is dead because the focus arm does.
    fn folded(event: KeyEvent) -> KeyEvent {
        let mut event = super::shifted(event);
        event.code = match event.code {
            // Which side of the keyboard, and which media or system key, are not
            // gestures either: the label groups them because the router treats
            // them alike, and this is where that claim is checked.
            KeyCode::Modifier(modifier, _) => {
                KeyCode::Modifier(modifier, ModifierDirection::Unknown)
            }
            KeyCode::Media(_) => KeyCode::Media(MediaKeyCode::Play),
            KeyCode::ScrollLock
            | KeyCode::NumLock
            | KeyCode::PrintScreen
            | KeyCode::Pause
            | KeyCode::Menu
            | KeyCode::KeypadBegin => KeyCode::CapsLock,
            code => code,
        };
        // Command is Ctrl's alias on undo and redo, the claim `label` makes by
        // spelling both `C-`.
        if matches!(event.code, KeyCode::Char('z' | 'Z'))
            && event.modifiers.contains(KeyModifiers::SUPER)
        {
            event.modifiers = (event.modifiers - KeyModifiers::SUPER) | KeyModifiers::CTRL;
        }
        event.modifiers &= match event.code {
            KeyCode::Char(_) if event.modifiers.contains(KeyModifiers::CTRL) => KeyModifiers::CTRL,
            // Command is a gesture of its own on these five and nowhere else,
            // which is the claim `label` makes about them and this is where it
            // is checked.
            KeyCode::Char('c' | 'v' | 's' | 'd' | 'f')
                if event.modifiers.contains(KeyModifiers::SUPER) =>
            {
                KeyModifiers::SUPER
            }
            KeyCode::Char(_) => KeyModifiers::ALT,
            KeyCode::Backspace => KeyModifiers::ALT,
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down
                if event
                    .modifiers
                    .contains(KeyModifiers::ALT | KeyModifiers::CTRL) =>
            {
                KeyModifiers::ALT | KeyModifiers::CTRL
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                KeyModifiers::SHIFT | KeyModifiers::ALT
            }
            // The modifiers [`label`] spells on a function key: Ctrl on any
            // of them, Shift on F8 alone.
            KeyCode::F(8) => KeyModifiers::CTRL | KeyModifiers::SHIFT,
            KeyCode::F(_) => KeyModifiers::CTRL,
            _ => KeyModifiers::NONE,
        };
        event
    }

    /// A label groups keys, and a group that hides a difference is the blind
    /// spot this sweep exists to close. Every key must therefore do what the
    /// gesture its label names does — so a Ctrl+Alt combination that stops
    /// matching its plain-Alt twin, or a media key that starts doing something
    /// the others do not, fails here instead of folding onto a label the
    /// cheatsheet has already excused. Events rather than the state they leave:
    /// two keys producing the same events do the same thing after a waiting
    /// operator too, which is what carries this to the chords.
    #[test]
    fn a_key_does_what_the_gesture_its_label_names_does() {
        let state = editing();
        for event in every_key() {
            assert_eq!(
                press(&state, event),
                press(&state, folded(event)),
                "{:?} is spelled {} and does something else",
                event,
                label(event)
            );
        }
    }

    /// The keys that wait for a second one. Read by [`candidates`] and by the
    /// chord sweep below, so the two cannot disagree about what a chord is.
    const OPERATORS: [char; 3] = ['g', 'd', 'y'];

    /// Every binding the editor could plausibly be given, each with the label it
    /// would be written under. The candidates are [`every_key`] — the same sweep
    /// the hosted panes are held to — because a key nobody can discover is a key
    /// nobody uses whatever its shape, and this list used to enumerate only what
    /// the deleted key enum could name: printable ASCII, Ctrl and Alt letters,
    /// four arrow forms and eight named keys. The enum's blind spot outlived it
    /// in the one test whose job is to have none, so PageUp, a function key and
    /// every Ctrl+Alt combination went undriven.
    ///
    /// Chords are candidates too, because the binding that went missing was a
    /// chord: each waiting operator is followed by every key it can be followed
    /// by. A chord earns a row of its own when the operator gives an
    /// otherwise-dead key a meaning: `t` does nothing and `gt` steps a buffer. A
    /// second key that already does something alone is listed under that, and
    /// the keys claimed before the buffer sees them fire straight through a
    /// waiting operator anyway, so `d/` is `/` doing its job rather than a
    /// binding.
    fn candidates(state: &State) -> Vec<(String, Vec<KeyEvent>)> {
        let keys = every_key();
        let seconds: Vec<KeyEvent> = keys
            .iter()
            .copied()
            .filter(|key| !answers(state, &[*key]))
            .collect();
        let mut candidates: Vec<(String, Vec<KeyEvent>)> = keys
            .into_iter()
            .map(|key| (label(key), vec![key]))
            .collect();
        for operator in OPERATORS {
            for second in &seconds {
                candidates.push((
                    format!("{operator}{}", label(*second)),
                    vec![plain(operator), *second],
                ));
            }
        }
        candidates
    }

    /// A buffer in normal mode, which is the only mode the box is shown in: the
    /// insert-mode meaning of a printable key is "type it", and a cheatsheet
    /// listing every character would remind nobody of anything.
    fn editing() -> State {
        let opened = crate::update(
            &State::default(),
            Event::BufferOpened {
                path: std::path::PathBuf::from("/w/one.rs"),
                contents: "one two three\nfour five six\nseven eight nine\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        // Away from the edges in every direction: at line 1, column 1 a clamped
        // `h` or `k` changes nothing and would read as unbound.
        let mut state = State {
            focus: Pane::Editor,
            ..opened
        };
        for direction in [Direction::Down, Direction::Right, Direction::Right] {
            state = crate::update(&state, Event::EditorArrow(direction)).0;
        }
        state
    }

    /// A diff on screen with focus on the editor pane — the state the box is
    /// drawn over in Review view. Three lines so a diff-line step off either
    /// edge is visible, and the cursor is walked one line down for the same
    /// reason [`editing`] walks off the buffer's corner: a clamped `k` at the
    /// top would read as unbound.
    fn reviewing() -> State {
        let shown = crate::update(
            &crate::update(&State::default(), Event::OpenReviewView).0,
            Event::ShowDiff {
                file: "one.rs".to_string(),
                lines: vec![
                    DiffLine {
                        new_line: Some(1),
                        old_line: Some(1),
                        removed: false,
                        text: "one".to_string(),
                    },
                    DiffLine {
                        new_line: Some(2),
                        old_line: None,
                        removed: false,
                        text: "two".to_string(),
                    },
                    DiffLine {
                        new_line: None,
                        old_line: Some(2),
                        removed: true,
                        text: "three".to_string(),
                    },
                ],
                revision: "abc123".to_string(),
            },
        )
        .0;
        let focused = crate::update(&shown, Event::MoveFocus(Direction::Right)).0;
        crate::update(&focused, Event::EditorKey('j')).0
    }

    /// Story view mid-walk: a two-step Story, its first Step's Site already
    /// open in the editor. Focused where the palette sends it — the editor
    /// pane, since the tree pane it also draws over has no rows to browse —
    /// the way [`editing`] and [`reviewing`] are each walked away from their
    /// own starting corner, so a clamped `n`/`p`/`j`/`k` reads as unbound.
    fn story() -> State {
        let file = "one.rs".to_string();
        let site = |from, to| story::Site {
            file: file.clone(),
            side: story::Side::New,
            kind: story::Kind::Changed,
            from,
            to,
            text: String::new(),
        };
        let step = |claim: &str, site| story::Step {
            id: claim.to_string(),
            name: claim.to_string(),
            claim: claim.to_string(),
            why: String::new(),
            site,
            flow: None,
            values: Vec::new(),
            nudge: None,
            prediction: None,
        };
        let artifact = story::Artifact {
            protocol_version: 2,
            title: "Story set".to_string(),
            range: story::Range {
                base: "a".to_string(),
                head: "b".to_string(),
                spelling: "main..HEAD".to_string(),
            },
            stories: vec![story::Story {
                id: "s1".to_string(),
                name: "Story".to_string(),
                premise: String::new(),
                steps: vec![
                    story::Step {
                        // A cited value on the first Step, so `g` has
                        // somewhere to jump — otherwise the sweep could never
                        // see it answer.
                        values: vec![story::Value {
                            name: "n".to_string(),
                            value: "v".to_string(),
                            provenance: story::Provenance::Literal,
                            cite: Some(story::Cite {
                                file: file.clone(),
                                line: 1,
                            }),
                        }],
                        ..step("first", site(1, 1))
                    },
                    step("second", site(2, 2)),
                ],
            }],
        };
        let opened = crate::update(
            &State::default(),
            Event::BufferOpened {
                path: std::path::PathBuf::from("one.rs"),
                contents: "one\ntwo\nthree\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let walking = State {
            view: View::Story,
            focus: Pane::Editor,
            story_set: story::Set::Loaded(artifact),
            walking: Some(story::Walking::Story {
                story: 0,
                step: 0,
                diff: story::Diff::Hidden,
            }),
            ..opened
        };
        // Off the buffer's corner for the same reason [`editing`] walks off it:
        // at line 1 column 1 a motion that goes back changes nothing and reads
        // as unbound, which is how a live word motion came to look dead here.
        [Direction::Down, Direction::Right, Direction::Right]
            .into_iter()
            .fold(walking, |state, direction| {
                crate::update(&state, Event::EditorArrow(direction)).0
            })
    }

    /// The state with any half-typed operator dropped. Waiting for a second key
    /// is not an answer, and forgetting one is not either: without this, every
    /// `d` followed by an unbound key would look like a binding, because it
    /// cleared the `d`.
    fn settled(state: &State) -> State {
        let mut settled = state.clone();
        if let Some(buffer) = settled
            .current_buffer
            .clone()
            .and_then(|path| settled.buffers.get_mut(&path))
        {
            buffer.clear_pending();
        }
        settled
    }

    /// Drives a sequence of keys against a state and drafts already in
    /// progress, leaving both exactly where the sequence left them — a
    /// waiting chord's pending key included, since a second key is
    /// interpreted against the state the one before it left, the way the
    /// edge drives one keystroke after another.
    fn drive(state: &State, drafts: &mut Drafts, sequence: &[KeyEvent]) -> (State, bool) {
        let mut next = state.clone();
        let mut acted = false;
        for key in sequence {
            for event in on_key_event(&next, drafts, *key, 0) {
                let (after, effects) = crate::update(&next, event);
                next = after;
                // A refusal is not a binding. An operator over a key that
                // names no motion says so rather than swallowing it, which is
                // what the working contract asks of every dead end — counting
                // it here would ask the cheatsheet to list every key `d` has
                // no answer for.
                acted |= effects.iter().any(|effect| {
                    !matches!(
                        effect,
                        crate::Effect::NotifyAbout {
                            slug: "no-such-motion",
                            ..
                        }
                    )
                });
            }
        }
        (next, acted)
    }

    /// What driving a sequence of keys leaves behind, from a clean start.
    fn outcome(state: &State, sequence: &[KeyEvent]) -> (State, Drafts, bool) {
        let mut drafts = Drafts::default();
        let (next, acted) = drive(state, &mut drafts, sequence);
        (settled(&next), drafts, acted)
    }

    /// Rust cannot reflect over a match, so bound is measured rather than read:
    /// a sequence that leaves the state, the drafts and the effects untouched
    /// did nothing, and nothing needs no reminder.
    fn answers(state: &State, sequence: &[KeyEvent]) -> bool {
        let (after, drafts, acted) = outcome(state, sequence);
        acted || after != settled(state) || drafts != Drafts::default()
    }

    /// A chord candidate's own contribution: the second key measured against
    /// the state the operator alone already left, not the untouched state —
    /// so an operator with a persisting effect of its own (the Step-detail
    /// overlay `D` opens and leaves open) does not make every possible
    /// second key look like part of a distinct two-key binding. A genuine
    /// chord ("gt", "dd") still shows up: the operator's pending key
    /// survives into the second call exactly as it does when driven live.
    fn chord_answers(state: &State, operator: KeyEvent, second: KeyEvent) -> bool {
        let mut drafts = Drafts::default();
        let (after_operator, _) = drive(state, &mut drafts, &[operator]);
        let baseline = settled(&after_operator);
        let baseline_drafts = drafts.clone();
        let (after_both, acted) = drive(&after_operator, &mut drafts, &[second]);
        acted || settled(&after_both) != baseline || drafts != baseline_drafts
    }

    /// A candidate's own contribution, whichever shape it is: a lone key
    /// against the untouched state, or a chord's second key against the
    /// state its operator alone already left.
    fn sequence_answers(state: &State, sequence: &[KeyEvent]) -> bool {
        match sequence {
            [operator, second] => chord_answers(state, *operator, *second),
            _ => answers(state, sequence),
        }
    }

    /// Every view the sweep drives, paired with the state it is driven
    /// against. Adding a view here is what puts it under the same contract as
    /// the others — a view left out would be as blind as no reverse assertion
    /// at all.
    ///
    /// Edit appears three times, because the candidate list and a snippet's
    /// tab stops are each a second router the same view can be in: they are the
    /// two modals that let the keys they have no use for through to the buffer,
    /// so the keys they *do* claim are bindings nothing else in this sweep would
    /// ever drive. Tab was one of them, listed in neither the cheatsheet nor the
    /// omissions and invisible to the sweep, which is the exact blind spot this
    /// test exists to close.
    fn views() -> [(View, State); 6] {
        [
            (View::Edit, editing()),
            (View::Edit, offering_candidates()),
            (View::Edit, filling_in_a_snippet(&editing())),
            (View::Review, reviewing()),
            (View::Story, story()),
            (View::Edit, crate::debug::paused(editing())),
        ]
    }

    /// A state holding a snippet's remaining tab stops, so Tab means something
    /// — one stop at the end of the buffer, which is somewhere the cursor is
    /// not already. Tab answers only while a sequence is live, exactly as Copy
    /// and Undo answer only with a selection or an edit already made, so this
    /// joins those two in the sweep. A state with no buffer has no place for a
    /// stop and is left as it is.
    fn filling_in_a_snippet(state: &State) -> State {
        match state.current_buffer.clone() {
            Some(path) => State {
                modal: crate::Modal::Stops {
                    path,
                    at: vec![crate::editor::Tail(0)],
                },
                ..state.clone()
            },
            None => state.clone(),
        }
    }

    /// The editor with a candidate list up.
    fn offering_candidates() -> State {
        State {
            modal: crate::Modal::Candidates(crate::lsp::Candidates::offering(
                &State::default(),
                vec![
                    crate::lsp::Candidate {
                        label: "workspace_root".to_string(),
                        insert: "workspace_root".to_string(),
                        filter: None,
                        sort: None,
                        snippet: false,
                    },
                    crate::lsp::Candidate {
                        label: "working_dir".to_string(),
                        insert: "working_dir".to_string(),
                        filter: None,
                        sort: None,
                        snippet: false,
                    },
                ],
                crate::lsp::Ask {
                    // The buffer [`editing`] opens: a list is a claim about
                    // the file on screen, and one naming any other file is
                    // taken down before a key can reach it.
                    path: std::path::PathBuf::from("/w/one.rs"),
                    place: crate::Place { line: 2, column: 1 },
                    revision: 1,
                    about: crate::lsp::About::Candidates,
                },
            )),
            ..editing()
        }
    }

    /// Whether a row's token names a label, either on its own or — for a
    /// single key — inside a chord like `gt`. A chord names each of the keys
    /// it is made of, and every chord Varde has is two keys long: any longer
    /// token is one binding naming only itself, which is what stops `a` from
    /// matching the `a` in `S-arr` and `l` from matching the `l` in `Ctrl`.
    fn names(token: &str, label: &str) -> bool {
        token == label
            || (label.chars().count() == 1 && token.chars().count() == 2 && token.contains(label))
    }

    /// A binding is listed for a view if it appears in a left-hand column of a
    /// row naming that view.
    fn listed(state: &State, label: &str, view: View) -> bool {
        super::cheatsheet(state)
            .filter(|(_, _, views)| super::applies_to(views, view))
            .flat_map(|(keys, _, _)| keys.split_whitespace())
            .any(|token| names(token, label))
    }

    /// Whether an omission excuses a label in a view.
    fn omitted_in(label: &str, view: View) -> bool {
        UNLISTED
            .iter()
            .any(|(omitted, _, views)| *omitted == label && super::applies_to(views, view))
    }

    /// The reason this test exists: buffer stepping was bound without a modifier
    /// for months and missing from the cheatsheet, so the person who wrote it
    /// went looking for a modifier binding instead. Run per view, because a
    /// key that answers only in Review is exactly as unexcused there as one
    /// that answers only in Edit.
    #[test]
    fn every_binding_the_editor_answers_is_listed_or_a_recorded_omission() {
        for (view, state) in views() {
            let mut missing: Vec<String> = candidates(&state)
                .into_iter()
                .filter(|(_, sequence)| sequence_answers(&state, sequence))
                .map(|(label, _)| label)
                .filter(|label| !listed(&state, label, view) && !omitted_in(label, view))
                .collect();
            // One gesture is one line: the sweep drives every modifier
            // combination, so an unlisted binding is found sixty-four times over.
            missing.sort();
            missing.dedup();
            assert!(
                missing.is_empty(),
                "in {view:?} these bindings do something and are in neither the \
                 cheatsheet nor the omissions list: {missing:?}"
            );
        }
    }

    /// The same contract [`CHEATSHEET`] is held to, for the keys that only exist
    /// while Tools is up: what the box names must answer, and what
    /// answers must be named. Without it the footer is a string in the
    /// renderer, and a key added to the list — or taken out of it — is
    /// discoverable or not by whoever remembers to edit two files.
    #[test]
    fn tools_answers_exactly_the_keys_its_box_names() {
        // A row with something to offer: `i` on a list of nothing is a key
        // that does nothing, which is a statement about configuration rather
        // than about the binding.
        let mut listing = State {
            modal: crate::Modal::Tools { row: 0 },
            os: "macos".to_string(),
            ..editing()
        };
        listing.servers.insert(
            "zig".to_string(),
            crate::startup::Server {
                command: "zls".to_string(),
                args: Vec::new(),
                also_served_by: Vec::new(),
                extensions: Vec::new(),
                install: [("macos".to_string(), "brew install zls".to_string())]
                    .into_iter()
                    .collect(),
                initialization_options: None,
                partial: None,
                unanswerable: None,
            },
        );
        for (key, word) in super::TOOL_LIST_KEYS {
            let event = every_key()
                .into_iter()
                .find(|event| label(*event) == key)
                .unwrap_or_else(|| panic!("no key spells {key}"));
            assert!(
                answers(&listing, &[event]),
                "the box offers {key} for {word} and the list does nothing with it"
            );
        }
        // What the box has to name is what the *list* answers. A gesture that
        // answers with the list closed answers through it — `C-space`, `C-f`,
        // `C-q` — and is already listed or excused where the cheatsheet holds
        // it; naming those here would spell one label twice.
        let closed = State {
            modal: crate::Modal::None,
            ..listing.clone()
        };
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| answers(&listing, &[*event]) && !answers(&closed, &[*event]))
            .map(label)
            .filter(|label| {
                !super::TOOL_LIST_KEYS.iter().any(|(key, _)| key == label)
                    // Every list in Varde moves on the arrows, however they are
                    // modified: the router does not inspect a modifier here, so
                    // a modified arrow names no gesture of its own.
                    && !label.contains("arr")
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "Tools answers keys its box does not name: {unnamed:?}"
        );
    }

    /// The same contract [`CHEATSHEET`] is held to, for the keys that exist only
    /// while the branch picker is up: what the box names must answer, and what
    /// answers must be named. Held here rather than by a cheatsheet row for the
    /// reason Tools is: the box exists only while it is up.
    #[test]
    fn the_branch_picker_answers_exactly_the_keys_its_box_names() {
        // A row to act on: Enter on a list of nothing is a key that does
        // nothing, which is a statement about the repository rather than about
        // the binding.
        let listing = State {
            modal: crate::Modal::Branches {
                refs: vec![
                    crate::story::BranchRef {
                        name: "feature".to_string(),
                        remote: false,
                        when: 2,
                    },
                    crate::story::BranchRef {
                        name: "main".to_string(),
                        remote: false,
                        when: 1,
                    },
                ],
                filter: String::new(),
                row: 0,
            },
            ..editing()
        };
        for (key, word) in super::BRANCH_LIST_KEYS {
            let event = every_key()
                .into_iter()
                .find(|event| label(*event) == key)
                .unwrap_or_else(|| panic!("no key spells {key}"));
            assert!(
                answers(&listing, &[event]),
                "the box offers {key} for {word} and the picker does nothing with it"
            );
        }
        // What the box has to name is what the *picker* answers: a gesture that
        // answers with it closed answers through it, and is already listed or
        // excused where the cheatsheet holds it.
        let closed = State {
            modal: crate::Modal::None,
            ..listing.clone()
        };
        // Typing is not a key the box names, for the reason the comment box's
        // letters are not: narrowing a list is text, not a gesture, and a box
        // naming every character is a box naming nothing. What the box promises
        // instead is `BRANCH_FILTER_HINT`, and the test below holds it.
        let gesture = |event| {
            let events = on_key_event(&listing, &mut Drafts::default(), event, 0);
            !events.is_empty() && !events.iter().all(|e| matches!(e, Event::FilterBranches(_)))
        };
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| gesture(*event) && !answers(&closed, &[*event]))
            .map(label)
            .filter(|label| {
                !super::BRANCH_LIST_KEYS.iter().any(|(key, _)| key == label)
                    // Every list in Varde moves on the arrows, however they are
                    // modified: the router does not inspect a modifier here, so
                    // a modified arrow names no gesture of its own.
                    && !label.contains("arr")
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "the branch picker answers keys its box does not name: {unnamed:?}"
        );
    }

    /// The same contract for the launch list: what its box names answers,
    /// and nothing else it answers goes unnamed.
    #[test]
    fn the_launch_list_answers_exactly_the_keys_its_box_names() {
        let mut listing = State {
            modal: crate::Modal::Launches { row: 0 },
            ..editing()
        };
        listing.launches.insert(
            "app".to_string(),
            crate::startup::Launch {
                adapter: "rust".to_string(),
                request: "launch".to_string(),
                args: serde_json::Map::new(),
                reattach: true,
            },
        );
        for (key, word) in super::LAUNCH_LIST_KEYS {
            let event = every_key()
                .into_iter()
                .find(|event| label(*event) == key)
                .unwrap_or_else(|| panic!("no key spells {key}"));
            assert!(
                answers(&listing, &[event]),
                "the box offers {key} for {word} and the list does nothing with it"
            );
        }
        let closed = State {
            modal: crate::Modal::None,
            ..listing.clone()
        };
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| {
                !on_key_event(&listing, &mut Drafts::default(), *event, 0).is_empty()
                    && !answers(&closed, &[*event])
            })
            .map(label)
            .filter(|label| {
                !super::LAUNCH_LIST_KEYS.iter().any(|(key, _)| key == label)
                    && !label.contains("arr")
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "the launch list answers keys its box does not name: {unnamed:?}"
        );
    }

    /// The debug keys reach Varde from a shell only while a session exists;
    /// with none the shell's child has them, as every key it is not denied.
    /// Stepping is on this list for the reason F9 is: it happens in bursts
    /// from wherever the keyboard is, and a key that works in one pane is a
    /// key that fails the moment focus is elsewhere.
    #[test]
    fn the_debug_keys_are_reserved_only_while_a_session_exists() {
        let shell = State {
            focus: Pane::Terminal,
            ..State::default()
        };
        let debugging = State {
            focus: Pane::Terminal,
            ..crate::debug::paused(State::default())
        };
        let shift = |code| KeyEvent::new(code).modifiers(KeyModifiers::SHIFT);
        let claimed = [
            (KeyEvent::new(KeyCode::F(9)), Event::DebugResume),
            (
                KeyEvent::new(KeyCode::F(2)).modifiers(KeyModifiers::CTRL),
                Event::DebugStop,
            ),
            (
                KeyEvent::new(KeyCode::F(8)),
                Event::DebugStep(crate::debug::Step::Over),
            ),
            (
                KeyEvent::new(KeyCode::F(7)),
                Event::DebugStep(crate::debug::Step::Into),
            ),
            (
                shift(KeyCode::F(8)),
                Event::DebugStep(crate::debug::Step::Out),
            ),
        ];
        let press = |state: &State, key| on_key_event(state, &mut Drafts::default(), key, 0);
        for (key, event) in claimed {
            assert_eq!(press(&debugging, key), vec![event], "{key:?}");
            assert!(
                matches!(press(&shell, key)[..], [Event::Bytes(_)]),
                "{key:?} is withheld from the shell with no session"
            );
        }
    }

    /// Stepping mode: the letters act without their Space, and any other key
    /// leaves the mode and is then routed as usual rather than swallowed —
    /// which is what stops the mode from being somewhere anybody is trapped.
    /// `q` is not one of the letters, so it leaves the mode like any other.
    #[test]
    fn stepping_mode_claims_its_letters_and_hands_every_other_key_back() {
        let stepping = State {
            stepping: true,
            ..crate::debug::paused(editing())
        };
        let press = |key| on_key_event(&stepping, &mut Drafts::default(), key, 0);
        for (key, event) in [
            ('n', Event::DebugStep(crate::debug::Step::Over)),
            ('i', Event::DebugStep(crate::debug::Step::Into)),
            ('o', Event::DebugStep(crate::debug::Step::Out)),
            ('c', Event::DebugResume),
        ] {
            assert_eq!(press(plain(key)), vec![event], "{key}");
        }
        let plainly = |key| on_key_event(&editing(), &mut Drafts::default(), plain(key), 0);
        for key in ['j', '/', 'q'] {
            let events = press(plain(key));
            assert_eq!(events.first(), Some(&Event::LeaveStepping), "{key}");
            assert_eq!(events[1..], plainly(key), "{key} is not what it always is");
        }
        // The one way out that is not a key Varde interprets: a click can put
        // the keyboard in a shell, whose child owns these letters, and a mode
        // that outlived that would have nothing left that could end it.
        let shell = State {
            focus: Pane::Terminal,
            ..stepping
        };
        let in_shell = on_key_event(&shell, &mut Drafts::default(), plain('n'), 0);
        assert_eq!(in_shell.first(), Some(&Event::LeaveStepping));
        assert!(matches!(in_shell[1..], [Event::Bytes(_)]));
    }

    /// The Evaluator's arrange mode, on Stepping mode's terms: the four
    /// letters and the arrows move or resize the window, and any other key
    /// leaves the mode and then does what it always does — so the Snippet is
    /// never a buffer somebody is stuck outside of.
    #[test]
    fn the_arrange_mode_claims_its_motions_and_hands_every_other_key_back() {
        let mut open = crate::debug::paused(editing());
        crate::debug::open_evaluator(&mut open, "count".to_string());
        let arranging = |how| State {
            arranging: Some(how),
            ..open.clone()
        };
        let moving = arranging(crate::debug::Arrange::Moving);
        let press = |state: &State, key| on_key_event(state, &mut Drafts::default(), key, 0);
        for (key, direction) in [
            (plain('h'), Direction::Left),
            (plain('j'), Direction::Down),
            (plain('k'), Direction::Up),
            (plain('l'), Direction::Right),
            (KeyEvent::new(KeyCode::Right), Direction::Right),
        ] {
            assert_eq!(
                press(&moving, key),
                vec![Event::MoveEvaluator(direction)],
                "{key:?}"
            );
        }
        assert_eq!(
            press(&arranging(crate::debug::Arrange::Sizing), plain('j')),
            vec![Event::ResizeEvaluator(Direction::Down)]
        );
        // Any other key: the mode goes, and the key is the key it always was
        // in the Snippet.
        let plainly = |key| on_key_event(&open, &mut Drafts::default(), key, 0);
        for key in [plain('i'), plain('x'), KeyEvent::new(KeyCode::Esc)] {
            let events = press(&moving, key);
            assert_eq!(events.first(), Some(&Event::LeaveArranging), "{key:?}");
            assert_eq!(
                events[1..],
                plainly(key),
                "{key:?} is not what it always is"
            );
        }
    }

    /// The other half of the picker's contract: the sweep above excuses every
    /// key that only narrows the list, so this is what holds those keys to
    /// doing what the box says they do. Backspace is here because the excuse
    /// covers it too — a gesture nothing asserts is a gesture that can quietly
    /// stop working.
    #[test]
    fn the_pickers_hint_is_true_of_a_letter_and_of_backspace() {
        let listing = |filter: &str| State {
            modal: crate::Modal::Branches {
                refs: vec![crate::story::BranchRef {
                    name: "feature".to_string(),
                    remote: false,
                    when: 1,
                }],
                filter: filter.to_string(),
                row: 0,
            },
            ..editing()
        };
        assert!(super::BRANCH_FILTER_HINT.contains("type"));
        assert_eq!(
            press(&listing(""), plain('f')),
            vec![Event::FilterBranches("f".to_string())]
        );
        assert_eq!(
            press(&listing("fe"), KeyEvent::new(KeyCode::Backspace)),
            vec![Event::FilterBranches("f".to_string())]
        );
        // Nothing to delete is still an answer, not a key that does something
        // else: a picker that fell through to the tree on an empty filter would
        // move a selection nobody can see.
        assert_eq!(
            press(&listing(""), KeyEvent::new(KeyCode::Backspace)),
            vec![Event::FilterBranches(String::new())]
        );
    }

    /// The same contract [`CHEATSHEET`] is held to, for the keys that exist
    /// only while the results box is up. Unlike Tools, this box
    /// passes nothing through: every printable key is a letter of the query,
    /// so what the helper row must name is every key that means something
    /// *other* than typing — and a gesture the cheatsheet already holds
    /// (`C-f`, `C-q`, `C-space`) is not this row's to spell twice.
    #[test]
    fn the_results_box_answers_exactly_the_keys_its_helper_row_names() {
        let hit = |file: &str| crate::search::Hit {
            file: file.to_string(),
            line: 1,
            column: 1,
            text: "update".to_string(),
        };
        let showing = State {
            search: Some(crate::Search {
                query: crate::editor::Buffer::text_box("upd"),
                results: crate::search::Results {
                    // Two files, so stepping by file has somewhere to go: a key
                    // with nothing to act on is a statement about the results
                    // rather than about the binding.
                    hits: vec![hit("a.rs"), hit("b.rs")],
                    truncated: false,
                },
                ..crate::Search::default()
            }),
            ..editing()
        };
        // Editing the query is not a gesture: typing is what every key the box
        // does not claim already does, and moving through what was typed is
        // the cursor motion nobody needs reminding of.
        let gesture = |event| {
            let events = press(&showing, event);
            !events.is_empty()
                && !events.iter().all(|e| {
                    matches!(
                        e,
                        Event::EditorKey(_)
                            | Event::EditorBackspace
                            | Event::EditorDeleteWord
                            | Event::EditorArrow(_)
                            | Event::EditorWord(_)
                            | Event::QueryEnd(_)
                    )
                })
        };
        let named = |label: &str| {
            super::SEARCH_KEYS
                .iter()
                .any(|(keys, _)| keys.split_whitespace().any(|key| key == label))
        };
        // Some key a label spells, not every one: `arr` is up and down through
        // the hits, while left and right are the query's caret.
        for (keys, word) in super::SEARCH_KEYS {
            for key in keys.split_whitespace() {
                let spelled: Vec<KeyEvent> = every_key()
                    .into_iter()
                    .filter(|event| label(*event) == key)
                    .collect();
                assert!(!spelled.is_empty(), "no key spells {key}");
                assert!(
                    spelled.into_iter().any(gesture),
                    "the helper row offers {key} for {word} and the box does nothing with it"
                );
            }
        }
        // The two ways through a long list, asserted by name: the arrows are
        // excused everywhere in Varde by the omissions list below — cursor
        // motion nobody needs reminding of — so nothing else here would notice
        // this box dropping the one row that says how to get through it.
        for key in ["arr", "C-n", "C-p"] {
            assert!(named(key), "the helper row does not name {key}");
        }
        let mut unnamed: Vec<String> = every_key()
            .into_iter()
            .filter(|event| gesture(*event))
            .map(label)
            .filter(|label| {
                !named(label)
                    // Claimed before the box and held by the cheatsheet where
                    // every view answers them.
                    && !listed(&State::default(), label, View::Edit)
                    && !omitted_in(label, View::Edit)
            })
            .collect();
        unnamed.sort();
        unnamed.dedup();
        assert!(
            unnamed.is_empty(),
            "the results box answers keys its helper row does not name: {unnamed:?}"
        );
    }

    /// The Chord hint is drawn from [`CHORDS`] and [`DEBUG_CHORDS`], so what it
    /// names must be what a waiting Space answers, and what a waiting Space
    /// answers must be named. Measured against Escape, which takes the hint
    /// down and does nothing else: a second key that only does that is not a
    /// chord. A key claimed before the hint — `C-q`, `C-space` — answers
    /// through it and is not the hint's.
    ///
    /// Driven with a Debug session and without one, because the list is not
    /// fixed: the debug chords are on it only while a session exists, so a
    /// state with none would never see them and a state with one would never
    /// see them go. And with the Evaluator open, for the same reason once
    /// more: its two are on it only while its window is.
    #[test]
    fn a_waiting_space_answers_exactly_the_keys_the_chord_hint_names() {
        let arranging = {
            let mut state = crate::debug::paused(editing());
            crate::debug::open_evaluator(&mut state, String::new());
            state
        };
        for state in [editing(), crate::debug::paused(editing()), arranging] {
            let (waiting, _) = drive(&state, &mut Drafts::default(), &[plain(' ')]);
            assert_eq!(waiting.modal, crate::Modal::Chord, "the hint opens at once");
            let (cancelled, _) = drive(
                &waiting,
                &mut Drafts::default(),
                &[KeyEvent::new(KeyCode::Esc)],
            );
            assert_eq!(cancelled.modal, crate::Modal::None, "Escape cancels it");
            let chord = |event: KeyEvent| {
                let events = on_key_event(&waiting, &mut Drafts::default(), event, 0);
                let (after, acted) = drive(&waiting, &mut Drafts::default(), &[event]);
                matches!(events.as_slice(), [Event::Key(_)])
                    && (acted || settled(&after) != settled(&cancelled))
            };
            let mut answered: Vec<String> = every_key()
                .into_iter()
                .filter(|event| chord(*event))
                .map(label)
                .collect();
            answered.sort();
            answered.dedup();
            let mut named: Vec<String> = super::chord_rows(&waiting)
                .into_iter()
                .filter_map(|(key, _)| key.map(String::from))
                .collect();
            named.sort();
            assert_eq!(answered, named);
        }
    }

    /// Space is only a chord prefix in normal mode: inserting, it is a space.
    #[test]
    fn space_while_inserting_is_a_space() {
        let inserting = crate::update(&editing(), Event::EditorKey('i')).0;
        let (after, _) = drive(&inserting, &mut Drafts::default(), &[plain(' ')]);
        assert_eq!(after.modal, crate::Modal::None);
        assert!(crate::current_buffer(&after).is_some_and(|buffer| buffer.is_dirty()));
    }

    /// An omissions list nobody prunes is how the contract rots: an entry for a
    /// binding that no longer does anything in a view it names excuses
    /// nothing, and one that is also listed there is a contradiction about
    /// what the box promises.
    #[test]
    fn the_omissions_list_holds_only_live_unlisted_bindings() {
        for (view, state) in views() {
            let bound: Vec<String> = candidates(&state)
                .into_iter()
                .filter(|(_, sequence)| sequence_answers(&state, sequence))
                .map(|(label, _)| label)
                .collect();
            for (omitted, _, omitted_views) in UNLISTED {
                if !super::applies_to(omitted_views, view) {
                    continue;
                }
                assert!(
                    bound.iter().any(|label| label == omitted),
                    "{omitted} is excused as an omission in {view:?} but does nothing there"
                );
                assert!(
                    !listed(&State::default(), omitted, view),
                    "{omitted} is both listed and omitted in {view:?}"
                );
            }
        }
    }

    /// A selection already held, the way a mouse drag would leave one — Copy
    /// answers only with one of these in hand, in every view alike, and no
    /// key in the sweep produces one on its own for a pty-anchored selection.
    fn with_a_selection(state: &State) -> State {
        let mut selected = state.clone();
        selected.selection = Some(Selection::Screen {
            pane: Pane::Editor,
            from: Place { line: 0, column: 0 },
            to: Place { line: 0, column: 1 },
            text: "x".to_string(),
        });
        selected
    }

    /// A buffer with one edit already on its undo stack — Undo answers only
    /// once there is something to undo, which a fresh buffer never has.
    fn with_an_edit(state: &State) -> State {
        crate::update(state, Event::EditorKey('x')).0
    }

    /// The keystrokes a `:` command types out, its colon and its Enter
    /// included — a colon token is never a candidate on its own, since
    /// [`candidates`] only knows single keys and the `g`/`d`/`y` chords, so
    /// the reverse sweep types it out the way a reviewer would. The colon is
    /// what opens the command line, so dropping it drove the letters at the
    /// view instead: `:update` was passing in Review off the `e` in it, and a
    /// command spelled out of keys that mean nothing there — `:tall` — is
    /// what showed it.
    /// A pace is a multiplier: zero and below are a division the synthesizer
    /// would have to swallow, and a Transport reading `-3.00x`. Refused, the
    /// way any line nobody can act on is.
    #[test]
    fn a_speed_nobody_can_mean_is_not_a_command() {
        assert_eq!(command("speed 1.4"), vec![Event::SetSpeed(1.4)]);
        assert_eq!(command("split"), vec![Event::SplitTerminal]);
        assert_eq!(command("speed 0"), vec![]);
        assert_eq!(command("speed -3"), vec![]);
        assert_eq!(command("speed fast"), vec![]);
    }

    fn typed_command(token: &str) -> Vec<KeyEvent> {
        let mut sequence: Vec<KeyEvent> = token.chars().map(plain).collect();
        sequence.push(KeyEvent::new(KeyCode::Enter));
        sequence
    }

    /// The reverse of the first sweep, and the one that finds a defect instead
    /// of waiting for it to be asserted by hand: a row that claims a view but
    /// answers nothing there is exactly the promise Review view's cheatsheet
    /// used to break, advertising `dd yy p` against a diff that ignores all
    /// three. A row's claim is met if any one of its keys answers in that
    /// view — a row groups keys under one description, not one behaviour. A
    /// key is checked against the bare state and against one already holding
    /// a selection or an edit, since Copy and Undo only ever answer with one
    /// already made.
    #[test]
    fn every_listed_key_answers_in_a_view_it_claims() {
        for (view, state) in views() {
            let extra = [
                with_a_selection(&state),
                with_an_edit(&state),
                filling_in_a_snippet(&state),
            ];
            let states: Vec<&State> = std::iter::once(&state).chain(extra.iter()).collect();
            let answered: Vec<String> = states
                .iter()
                .flat_map(|s| candidates(s))
                .filter(|(_, sequence)| states.iter().any(|s| answers(s, sequence)))
                .map(|(label, _)| label)
                .collect();
            for (keys, what, views) in super::cheatsheet(&state) {
                if !super::applies_to(views, view) {
                    continue;
                }
                let claims = keys.split_whitespace().any(|token| {
                    if token.starts_with(':') {
                        let sequence = typed_command(token);
                        states.iter().any(|s| answers(s, &sequence))
                    } else {
                        answered.iter().any(|label| names(token, label))
                    }
                });
                assert!(
                    claims,
                    "\"{keys}\" ({what}) is listed for {view:?} but nothing in it answers there"
                );
            }
        }
    }

    /// A chord is held to its own claim, key by key, where the sweep above
    /// holds only the row. Row-level is the right rule for single keys — a
    /// state where `p` has a register to put and one where `n` has a query to
    /// step are states the sweep does not build, so per-key there reports
    /// two dozen keys dead that are only unarranged. A chord has no such
    /// excuse: its first key does nothing alone, so the chord either has a
    /// meaning in that view or it does not, and nothing about the arranged
    /// state can hide the difference. That is what let "C-p C-n gp gn" claim
    /// Review and Story for as long as it did: `Ctrl+p` answers in every view
    /// because `jump_alias` is routed ahead of the views, while `gp` needs a
    /// buffer to hold the waiting `g` — which Review has none of, and which a
    /// Story walk spends on jumping to a citation. One token answering is
    /// enough for the row; it is not enough for the chord printed beside it.
    #[test]
    fn every_listed_chord_answers_in_a_view_it_claims() {
        for (view, state) in views() {
            let extra = [
                with_a_selection(&state),
                with_an_edit(&state),
                filling_in_a_snippet(&state),
            ];
            let states: Vec<&State> = std::iter::once(&state).chain(extra.iter()).collect();
            for (keys, what, views) in super::cheatsheet(&state) {
                if !super::applies_to(views, view) {
                    continue;
                }
                for token in keys.split_whitespace() {
                    let mut spelling = token.chars();
                    let (Some(operator), Some(second), None) =
                        (spelling.next(), spelling.next(), spelling.next())
                    else {
                        continue;
                    };
                    if !OPERATORS.contains(&operator) {
                        continue;
                    }
                    assert!(
                        states
                            .iter()
                            .any(|s| chord_answers(s, plain(operator), plain(second))),
                        "\"{token}\" of \"{keys}\" ({what}) is listed for {view:?} and answers nothing there"
                    );
                }
            }
        }
    }
}
