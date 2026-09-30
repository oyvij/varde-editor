//! F5 and F13 — open buffers and modal editing.
//!
//! A buffer keeps what is on disk and the user's unsaved draft apart, which is
//! what lets a clean buffer follow the file while a dirty one is never
//! overwritten. Editing is vim-shaped: normal mode moves and deletes, insert
//! mode types.

use std::ops::{Bound, RangeBounds};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    /// Linewise only, as `V` is. A charwise selection is not a mode: it lives
    /// in [`crate::Selection`] and is entered by extending, not by a key.
    Visual,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Normal => "normal",
            Mode::Insert => "insert",
            Mode::Visual => "visual",
        }
    }
}

/// The indent width when no layer of the config names one, spelled once here
/// and read back by the TOML the merge starts from — a default that disagrees
/// with itself is a width nobody can predict, the same reason
/// `risk::DEFAULT_THRESHOLD` is one constant.
pub const DEFAULT_TAB_WIDTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    pub disk: String,
    pub draft: Option<String>,
    pub changed_on_disk: bool,
    /// 1-based, as the scenarios describe it.
    pub line: usize,
    pub column: usize,
    /// Which **row** of the Preview the cursor is on, 1-based, while this
    /// buffer is previewing. A separate cursor from `line` rather than the same
    /// one reinterpreted: a row is not a line, so one paragraph can hold six of
    /// them and `clamp` would pull a row back to a line the paragraph does not
    /// have. The two are converted into each other at the toggle, which is the
    /// only moment either has to agree with the other.
    pub row: usize,
    /// Which **column of the rendered row** the cursor is on, 1-based, while
    /// this buffer is previewing. A rendered column, never a source column:
    /// the row map cannot carry one, since the `## ` a heading no longer shows
    /// means screen column 3 is source column 5. So it indexes `Row::text()` —
    /// exactly what `/` matches in and a drag copies out of — which is why the
    /// sideways offset is allowed to follow it (ADR 0007, as amended) and why
    /// crossing in either direction still starts over at one.
    pub row_column: usize,
    /// Whether this buffer is read as the document it describes rather than as
    /// the characters it holds. Per buffer, so leaving one file in Source says
    /// nothing about the next one opened, and never persisted — the default is
    /// the thing the reader asked for rather than whatever they last did.
    pub previewing: bool,
    /// The lines that open the blocks folded away in this buffer, 1-based.
    /// Which lines that hides is [`crate::fold::hidden`]'s to derive, so the
    /// two cannot disagree; a line that no longer opens a block after an edit
    /// hides nothing rather than hiding the wrong thing. Per buffer for the
    /// reason `previewing` is, and never persisted: a file opens whole.
    pub folded: Vec<usize>,
    pub mode: Mode,
    /// The operator keys typed so far, waiting for what completes them: the
    /// `d` of `dd`, the `g` of `gg`, the `dg` of `dgg`. Empty when nothing is
    /// waiting. A string rather than one character because an operator that
    /// composes with a motion has to hold `dg` while the second `g` is on its
    /// way.
    pending: String,
    /// Bumped on every content change, so a cached parse can be invalidated
    /// without comparing the text.
    revision: u64,
    /// What the editor reads off the whole text, worked out with each
    /// revision. Shared, since `update` clones the buffer with every event.
    shape: std::sync::Arc<Shape>,
    /// Digits typed so far, so `3j` moves three lines.
    count: Option<usize>,
    /// Where a linewise visual selection started.
    anchor: usize,
    /// `editor.tab_width` as it stood when this file was opened, and the width
    /// [`indent_unit`] falls back to. On the buffer rather than threaded
    /// through every keystroke that can reach a new line: the config is read
    /// once at startup and there is no `:set`, so a snapshot taken at open
    /// cannot go stale — and a width passed down through `key` would be a
    /// parameter on the one method a hundred tests call to type a character.
    ///
    /// It is also why `Buffer` derives no `Default` and [`Buffer::open`] spells
    /// every field: a derived one hands out a buffer of width zero, whose
    /// indent unit is the empty string, so a caller who never named a width
    /// would lay no indentation at all and nothing would say so.
    tab_width: usize,
    /// The unnamed register. Vim has many; this has one.
    register: Vec<String>,
    undo: Vec<String>,
    /// What an undo took back, and where it left the cursor, newest last.
    /// Emptied by any edit, since what it holds was an edit to a text that is
    /// no longer there.
    redo: Vec<(String, crate::Place)>,
    /// The kind of edit the newest undo step is still open to. Anything that
    /// moves the cursor other than the edit itself closes it.
    step: Step,
}

/// Which kind of edit the newest undo step holds, so the next edit can join it
/// or start one of its own — the grouping VS Code does
/// (`shouldPushStackElementBetween`), so a typed word is one press of undo
/// rather than one per letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Typing,
    /// One space after a non-space: the next non-space still joins it.
    Space,
    /// Two or more: the next non-space starts a step of its own.
    Spaces,
    Deleting,
    /// Closed: whatever comes next starts a step.
    Other,
}

/// A place in a buffer named by the number of characters that *follow* it.
///
/// The form a snippet's tab stops are kept in, and the whole reason they land
/// where the reader expects: everything typed at one stop is typed *before* the
/// stops after it, so what follows a stop is the one measure a keystroke does
/// not move. A line and a column recorded when the snippet went in would name
/// the wrong place by the time Tab was pressed.
///
/// A newtype and not a `usize`, for the reason AGENTS.md gives for `AbsPath`:
/// the offsets [`Buffer::complete`] takes in count forwards from the start of
/// the text and the tails it hands back count backwards from the end of the
/// buffer, and the compiler is a better place for that distinction than a doc
/// comment the caller may not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tail(pub usize);

/// One row of a pty's grid as text, from what each of its cells holds: `None`
/// where the row has no such cell, `Some("")` where the cell holds nothing —
/// never written, or the second half of a wide character.
///
/// A cell holding nothing is a **space**, so column N of the string is column N
/// of the screen. Collecting only the cells that hold something collapses the
/// padding, which is what made a drag in the AI pane pick nothing: its output is
/// indented and full of wide glyphs, so the extracted row was far shorter than
/// the screen row and every column past the first gap named the wrong
/// character. The renderer substitutes the same space, and the two have to
/// agree about where a column is or a drag lands somewhere else than it looks.
///
/// Trailing blanks are dropped: a grid pads every row to its full width and
/// that padding is not text.
pub fn grid_row<'a>(cells: impl Iterator<Item = Option<&'a str>>) -> String {
    let row: String = cells
        .map(|cell| match cell {
            Some(text) if !text.is_empty() => text,
            _ => " ",
        })
        .collect();
    row.trim_end().to_string()
}

/// The text a charwise span covers, over the lines it covers as plain data:
/// the first line from the span's column, the lines between whole, the last up
/// to the span's column, both ends included. The caller supplies the lines —
/// from a buffer, or from a pty's visible grid — so a multi-row drag and a
/// keyboard-extended selection produce text the same way.
pub fn span_text(lines: &[String], from: crate::Place, to: crate::Place) -> String {
    let mut picked = Vec::new();
    for number in from.line..=to.line.min(lines.len()) {
        let chars: Vec<char> = lines[number - 1].chars().collect();
        let start = if number == from.line {
            from.column - 1
        } else {
            0
        };
        let end = if number == to.line {
            to.column.min(chars.len())
        } else {
            chars.len()
        };
        picked.push(chars[start.min(end)..end].iter().collect::<String>());
    }
    picked.join("\n")
}

/// The `http` or `https` URL under a 1-based column of one row of text, with
/// the punctuation a sentence hangs on it left off. Nothing for any other
/// scheme: the row is text a child printed, and handing `file:` or a custom
/// scheme to the operating system's opener is a way for printed text to launch
/// an application.
pub fn link_at(row: &str, column: usize) -> Option<String> {
    let index = row.char_indices().nth(column.checked_sub(1)?)?.0;
    linkify::LinkFinder::new()
        .kinds(&[linkify::LinkKind::Url])
        .links(row)
        .find(|link| link.start() <= index && index < link.end())
        .map(|link| link.as_str())
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .map(str::to_string)
}

/// Every place `word` starts, in document order — the runs the next-occurrence
/// gesture takes one at a time. Case-sensitive, since a name that differs in
/// case is a different name, and non-overlapping, so `aa` in `aaa` is one run
/// rather than two.
///
/// Over supplied lines rather than a method, for the reason [`span_text`] is
/// free: the same question is asked of a buffer's lines and answered the same
/// way whatever holds them.
pub fn occurrences(lines: &[String], word: &str) -> Vec<crate::Place> {
    if word.is_empty() {
        return Vec::new();
    }
    lines
        .iter()
        .enumerate()
        .flat_map(|(index, line)| {
            line.match_indices(word).map(move |(at, _)| crate::Place {
                line: index + 1,
                column: line[..at].chars().count() + 1,
            })
        })
        .collect()
}

/// Where an offset into the whole text sits, or nothing when the text is
/// shorter than that — the inverse of [`offset`], and the form a charwise
/// operator needs its span in, since the character behind the cursor is on the
/// line above when the cursor is at the start of one.
fn place_at(lines: &[String], offset: usize) -> Option<crate::Place> {
    let mut remaining = offset;
    for (index, line) in lines.iter().enumerate() {
        let width = line.chars().count();
        if remaining <= width {
            return Some(crate::Place {
                line: index + 1,
                column: remaining + 1,
            });
        }
        remaining -= width + 1;
    }
    None
}

/// Where a motion key lands, over the lines it is given as plain data —
/// `None` for a key that names no motion, which is what lets an operator
/// refuse one out loud instead of swallowing it.
///
/// Free rather than a method because a Preview's cursor moves over the
/// **rendered rows**, which are not the buffer's lines and live nowhere in it:
/// one definition of what `w` means, run over whichever text is on screen.
/// Columns are counted in **characters**, never display width — the same
/// measure `span_text`, `search::occurrences` and `ui::shift` use, so a column
/// names the same character to all of them.
///
/// `$` and `G` answer with the `usize::MAX` sentinels they always did: how far
/// a line or a text runs is the caller's clamp, and Source and a Preview clamp
/// against different things.
pub fn moved(lines: &[String], at: crate::Place, key: char) -> Option<crate::Place> {
    let word = |stop| {
        let text: Vec<char> = lines.join("\n").chars().collect();
        let landed = match stop {
            Word::Start => next_word_start(&text, offset(lines, at)),
            Word::End => word_end(&text, offset(lines, at)),
            Word::Back => previous_word_start(&text, offset(lines, at)),
        };
        // The break at the end of a line is a character of the text but not of
        // that line, so a landing on one names the column after the line's last
        // character. That is left to the caller's clamp, exactly as the `$` and
        // `G` sentinels below are: pulling it back here would take insert
        // mode's one extra column away from [`Buffer::word_motion`], which is
        // where Alt+Right lands after the last word of a line.
        place_at(lines, landed.min(text.len().saturating_sub(1))).unwrap_or(at)
    };
    Some(match key {
        'h' => crate::Place {
            column: at.column.saturating_sub(1).max(1),
            ..at
        },
        'l' => crate::Place {
            column: at.column + 1,
            ..at
        },
        'j' => crate::Place {
            line: at.line + 1,
            ..at
        },
        'k' => crate::Place {
            line: at.line.saturating_sub(1).max(1),
            ..at
        },
        '0' => crate::Place { column: 1, ..at },
        '$' => crate::Place {
            column: usize::MAX,
            ..at
        },
        'G' => crate::Place {
            line: usize::MAX,
            column: 1,
        },
        'w' => word(Word::Start),
        'e' => word(Word::End),
        'b' => word(Word::Back),
        _ => return None,
    })
}

impl Buffer {
    pub fn open(contents: &str, previewing: bool, tab_width: usize) -> Self {
        Self {
            disk: contents.to_string(),
            draft: None,
            changed_on_disk: false,
            tab_width,
            line: 1,
            column: 1,
            row: 1,
            row_column: 1,
            previewing,
            folded: Vec::new(),
            mode: Mode::Normal,
            pending: String::new(),
            // Reading the file is itself a content event, so a buffer that
            // holds text is at its first revision rather than at none. It is
            // the Document version a language server is told (F31), and a
            // version of zero is a document a server may treat as one it has
            // already seen.
            revision: 1,
            shape: std::sync::Arc::new(Shape::of(contents, tab_width)),
            count: None,
            anchor: 0,
            register: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            step: Step::Other,
        }
    }

    /// A one-line box being typed into — the `/` query and the project search
    /// query. Inserting from the start, since there is no normal mode in a box
    /// to leave, with the caret after whatever it was handed.
    pub fn text_box(contents: &str) -> Self {
        let mut typed = Self::open(contents, false, DEFAULT_TAB_WIDTH);
        typed.mode = Mode::Insert;
        typed.go_to_place(crate::Place {
            line: 1,
            column: usize::MAX,
        });
        typed
    }

    pub fn shown(&self) -> &str {
        self.draft.as_deref().unwrap_or(&self.disk)
    }

    pub fn is_dirty(&self) -> bool {
        self.draft.is_some()
    }

    /// The file changed underneath. A clean buffer follows it; a dirty one is
    /// flagged instead and keeps its draft, because unsaved work is never
    /// overwritten by something the user did not do.
    ///
    /// Assigning `disk` from outside would leave `revision` where it was, and
    /// the edge parses the buffer's tokens only when `revision` moves — so the
    /// screen kept painting the old file while `shown()` already returned the
    /// new one. That is why this is a method and `disk` is written nowhere else:
    /// following the file *is* a content change.
    pub fn follow(&mut self, contents: String) {
        self.disk = contents;
        self.changed_on_disk = self.draft.is_some();
        self.changed();
        self.clamp();
    }

    /// Take the version on disk and drop the draft — the user's answer to a
    /// divergence, and what `:e` does.
    pub fn reload(&mut self) {
        self.draft = None;
        self.changed_on_disk = false;
        self.changed();
        self.clamp();
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        self.shown().split('\n').map(str::to_string).collect()
    }

    /// Every content change goes through here, which is what makes `revision`
    /// trustworthy.
    fn set(&mut self, lines: &[String]) {
        self.draft = Some(lines.join("\n"));
        self.changed();
    }

    /// The text is not what it was: a new revision, and its [`Shape`].
    fn changed(&mut self) {
        self.revision += 1;
        self.shape = std::sync::Arc::new(Shape::of(self.shown(), self.tab_width));
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Line `number` of the text, 1-based, found by where it starts rather
    /// than by counting the lines above it.
    pub fn line_text(&self, number: usize) -> Option<&str> {
        let starts = &self.shape.starts;
        let from = *starts.get(number.checked_sub(1)?)?;
        let to = starts
            .get(number)
            .map_or(self.shown().len(), |next| next - 1);
        self.shown().get(from..to)
    }

    /// The lines a range of 1-based line numbers covers, each with its number.
    /// The editor draws a window of the file, and marking the lines it does not
    /// draw was most of what a frame of a big file cost (#101) — so what the
    /// renderer asks for is found in its window, and what steps through the
    /// whole file asks for all of it.
    pub fn lines_within(
        &self,
        lines: impl RangeBounds<usize>,
    ) -> impl Iterator<Item = (usize, &str)> {
        let first = match lines.start_bound() {
            Bound::Included(first) => *first,
            Bound::Excluded(first) => first + 1,
            Bound::Unbounded => 1,
        };
        (first.max(1)..=self.shape.starts.len())
            .take_while(move |number| lines.contains(number))
            .filter_map(|number| Some((number, self.line_text(number)?)))
    }

    /// Each line's own indentation in characters, and nothing for a line with
    /// nothing on it, worked out with the revision.
    pub(crate) fn indents(&self) -> &[Option<usize>] {
        &self.shape.indents
    }

    fn remember(&mut self, kind: Step) {
        let (joins, open) = match (self.step, kind) {
            (Step::Typing | Step::Space, Step::Typing) => (true, Step::Typing),
            (Step::Space | Step::Spaces, Step::Space) => (true, Step::Spaces),
            (Step::Deleting, Step::Deleting) => (true, Step::Deleting),
            _ => (false, kind),
        };
        if !joins {
            self.undo.push(self.shown().to_string());
        }
        self.step = open;
        self.redo.clear();
    }

    /// Keeps the cursor inside the text after any move or edit — and, while
    /// anything is folded, on a line that is actually drawn.
    fn clamp(&mut self) {
        let lines = self.lines();
        self.line = self.line.clamp(1, lines.len().max(1));
        // A folded block's body has no row on screen, so a cursor left on one
        // is a caret nobody can see and a key typed out of sight. It comes
        // back to the line the fold left showing, which is the outermost
        // folded block covering it — the first `blocks` yields, since a block
        // inside one that is folded is hidden with it.
        let line = self.line;
        let showing = self
            .folds()
            .find(|block| block.from < line && line <= block.to)
            .map(|block| block.from);
        if let Some(from) = showing {
            self.line = from;
        }
        let width = lines
            .get(self.line - 1)
            .map_or(0, |line| line.chars().count());
        if self.mode == Mode::Visual {
            self.anchor = self.anchor.clamp(1, lines.len().max(1));
        }
        // The one column past the text a folded line has that no other line
        // does: the glyph standing for what is hidden is a thing to press, so
        // it is a place the caret can reach with an arrow.
        let last = if self.mode == Mode::Insert || self.folded.contains(&self.line) {
            width + 1
        } else {
            width.max(1)
        };
        self.column = self.column.clamp(1, last.max(1));
    }

    /// The blocks this buffer has folded away.
    fn folds(&self) -> impl Iterator<Item = crate::fold::Block> + '_ {
        // Nothing parsed at all with nothing folded: `clamp` runs after every
        // key, and blocking out the whole file on each one to answer "no" is
        // the parse-per-frame rule wearing a different hat.
        let blocks = match self.folded.is_empty() {
            true => Vec::new(),
            false => crate::fold::blocks(self.shown()),
        };
        blocks
            .into_iter()
            .filter(|block| self.folded.contains(&block.from))
    }

    /// The last line a fold hides under `line`, or `line` itself when nothing
    /// is folded there. What `j` and the down arrow step over, so a folded
    /// block costs one keypress to pass rather than one per line nobody can
    /// see.
    fn past_fold(&self, line: usize) -> usize {
        self.folds()
            .find(|block| block.from == line)
            .map_or(line, |block| block.to)
    }

    /// Whether a place is the glyph a fold leaves at the end of the line that
    /// opens it: the one column past the text `clamp` allows there and nowhere
    /// else. One answer for the caret and for the pointer, because Enter on
    /// the dots and a click on them are the same gesture.
    pub fn fold_dots_at(&self, line: usize, column: usize) -> bool {
        self.folded.contains(&line)
            && column
                > self
                    .lines()
                    .get(line.saturating_sub(1))
                    .map_or(0, |text| text.chars().count())
    }

    /// Whether the caret is on those dots — where Enter opens the block.
    pub fn on_fold_dots(&self) -> bool {
        self.mode != Mode::Insert && self.fold_dots_at(self.line, self.column)
    }

    /// A keystroke, and the chord it left with nothing to do — `Some("dq")`
    /// for an operator over a key that names no motion. The one outcome a
    /// buffer cannot report by changing itself, since refusing is exactly the
    /// case where nothing changed, and silence is what made `dG` look like a
    /// broken key rather than an unimplemented one.
    pub fn key(&mut self, key: char) -> Option<String> {
        let refused = match self.mode {
            Mode::Insert => {
                self.insert(key);
                None
            }
            Mode::Normal | Mode::Visual => self.command(key),
        };
        self.clamp();
        refused
    }

    /// Arrows move in every mode, unlike `hjkl` which only move in normal mode.
    pub fn arrow(&mut self, direction: crate::Direction) {
        match direction {
            crate::Direction::Left => self.column = self.column.saturating_sub(1).max(1),
            crate::Direction::Right => self.column += 1,
            crate::Direction::Up => self.line = self.line.saturating_sub(1).max(1),
            crate::Direction::Down => self.line = self.past_fold(self.line) + 1,
        }
        self.step = Step::Other;
        self.clamp();
    }

    /// Deletes backwards, joining with the line above at the start of a line.
    pub fn backspace(&mut self) {
        let (left, right) = self.either_side();
        let mut lines = self.lines();
        if self.column > 1 {
            self.remember(Step::Deleting);
            let line = &mut lines[self.line - 1];
            let at = byte_index(line, self.column - 2);
            line.remove(at);
            self.column -= 1;
            // Both halves of an empty pair go together, so undoing the
            // editor's help costs the one key the help cost. Insert mode only:
            // the help was never given in normal mode, so taking two
            // characters there would be a rule of its own rather than the undo
            // of one.
            if self.mode == Mode::Insert
                && matches!((left, right), (Some(open), Some(close)) if closes(open) == Some(close))
            {
                line.remove(byte_index(line, self.column - 1));
            }
        } else if self.line > 1 {
            self.remember(Step::Deleting);
            let removed = lines.remove(self.line - 1);
            self.line -= 1;
            self.column = lines[self.line - 1].chars().count() + 1;
            lines[self.line - 1].push_str(&removed);
        } else {
            return;
        }
        self.set(&lines);
        self.clamp();
    }

    /// The word behind the cursor, which is what Alt+Backspace takes while
    /// inserting. `db` reaches the same delete in normal mode, and both go
    /// through one snapshot, so `u` costs the one key the delete cost.
    ///
    /// Bounded to the line, unlike the `b` the operator composes with: `b`
    /// reaches back across the break, so an unbounded delete on an indented
    /// line would take the indentation *and* the word above it. Clamped to the
    /// line, whitespace-only text behind the cursor is what goes — and at
    /// column 1 there is nothing behind on the line at all, so this degrades to
    /// the join [`Buffer::backspace`] already does rather than doing nothing.
    pub fn delete_word_back(&mut self) {
        if self.column == 1 {
            self.backspace();
            return;
        }
        let text: Vec<char> = self.shown().chars().collect();
        let here = self.offset();
        let line_start = here + 1 - self.column;
        self.take_chars(previous_word_start(&text, here).max(line_start), here);
    }

    /// The half-typed operator, so the app can claim `gt`/`gT` before the
    /// buffer sees them.
    pub fn pending(&self) -> &str {
        &self.pending
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// The word the cursor sits in — what `*` searches for. The cursor may sit
    /// past the last character of a line, where a pointer cannot: it is pulled
    /// back onto it, which is the one difference from [`Buffer::word_span`].
    ///
    /// The one line read rather than every line copied: the editor asks for
    /// this on every frame (#101).
    pub fn word_at_cursor(&self) -> Option<String> {
        let line: Vec<char> = self.line_text(self.line)?.chars().collect();
        let (from, to) = self.word_span(self.line, self.column.min(line.len()))?;
        Some(line[from - 1..to].iter().collect())
    }

    /// The columns of the word at a place, 1-based and inclusive — nothing when
    /// that place holds no word character at all. What `*` searches for and
    /// what a link underlines are the same run, so they are measured once.
    pub fn word_span(&self, line: usize, column: usize) -> Option<(usize, usize)> {
        let text: Vec<char> = self.line_text(line)?.chars().collect();
        let word = |c: &char| c.is_alphanumeric() || *c == '_';
        let at = column.checked_sub(1)?;
        if !text.get(at).is_some_and(word) {
            return None;
        }
        let mut from = at;
        while from > 0 && text.get(from - 1).is_some_and(word) {
            from -= 1;
        }
        let mut to = at;
        while text.get(to + 1).is_some_and(word) {
            to += 1;
        }
        Some((from + 1, to + 1))
    }

    /// The chosen completion in place of the identifier characters already
    /// typed, with the cursor after it.
    ///
    /// One edit rather than backspaces and keystrokes: those go through
    /// [`Buffer::key`], which is a mode away from meaning something else
    /// entirely, and an undo per character would take as many presses to get
    /// back to what was typed as the completion saved.
    ///
    /// Where the name before a place starts, 1-based — the run of identifier
    /// characters back from it, or that place when there is none.
    ///
    /// One function because two things must agree about it: what a completion
    /// *replaces*, and what the candidate list is *narrowed by*. Computed
    /// twice they can differ by a character, and then accepting overwrites a
    /// letter the reader typed.
    pub fn word_start(&self, line: usize, column: usize) -> usize {
        let Some(text) = self.shown().lines().nth(line - 1) else {
            return column;
        };
        let before: Vec<char> = text.chars().take(column - 1).collect();
        let name = before
            .iter()
            .rev()
            .take_while(|c| c.is_alphanumeric() || **c == '_')
            .count();
        column - name
    }

    /// The run before the cursor and not the whole word: completing in the
    /// middle of `wor|ker` replaces `wor`, because the characters after the
    /// cursor are not what the server was asked about.
    ///
    /// `stops` are a snippet's tab stops as character offsets into `text`,
    /// ascending. The cursor lands on the first of them, and what comes back is
    /// the rest as [`Tail`]s — which is the form a stop survives in, and the
    /// reason the two quantities are different types rather than two `usize`s
    /// in one signature. A completion that names no stops — every plain-text
    /// one — leaves the cursor after the text and hands back nothing.
    pub fn complete(&mut self, text: &str, stops: &[usize]) -> Vec<Tail> {
        self.remember(Step::Other);
        let start = self.word_start(self.line, self.column);
        let mut lines = self.lines();
        // Where the text is about to go, counted from the start of the buffer:
        // the newline joining each earlier line is a character of it, and a
        // snippet of several lines makes the difference between an offset and
        // a column.
        let before: usize = lines[..self.line - 1]
            .iter()
            .map(|line| line.chars().count() + 1)
            .sum();
        let line = &mut lines[self.line - 1];
        let typed: Vec<char> = line.chars().take(self.column - 1).collect();
        let head: String = typed[..start - 1].iter().collect();
        let tail: String = line.chars().skip(self.column - 1).collect();
        *line = format!("{head}{text}{tail}");
        self.column = head.chars().count() + text.chars().count() + 1;
        self.set(&lines);
        self.clamp();
        let at = before + head.chars().count();
        let total = self.shown().chars().count();
        let tails: Vec<Tail> = stops.iter().map(|stop| Tail(total - at - stop)).collect();
        // A stop the buffer cannot name is impossible here — it was measured
        // against the text this call just put there.
        if let Some(place) = tails.first().and_then(|first| self.place_before(*first)) {
            self.go_to_place(place);
        }
        tails.into_iter().skip(1).collect()
    }

    /// Where a [`Tail`] sits, as a line and a column — or nothing at all, when
    /// the buffer has since lost the text that measured it and there is no such
    /// place to name. A stop nobody can name ends the sequence rather than
    /// sending the cursor to the first character, which is a place nobody asked
    /// for.
    pub fn place_before(&self, tail: Tail) -> Option<crate::Place> {
        let text = self.shown();
        let head: String = text
            .chars()
            .take(text.chars().count().checked_sub(tail.0)?)
            .collect();
        Some(crate::Place {
            line: head.matches('\n').count() + 1,
            column: head.chars().rev().take_while(|c| *c != '\n').count() + 1,
        })
    }

    /// Puts the cursor at a place, clamped to the buffer — used after a jump
    /// and after a click, which can land past the end of a short line.
    pub fn go_to_place(&mut self, at: crate::Place) {
        self.line = at.line.max(1);
        self.column = at.column.max(1);
        self.step = Step::Other;
        self.clamp();
    }

    pub fn escape(&mut self) {
        self.mode = Mode::Normal;
        self.step = Step::Other;
        self.pending.clear();
        self.count = None;
        self.clamp();
    }

    /// The half-typed command, for the indicator at the bottom of the editor —
    /// vim shows the same thing in the corner.
    pub fn pending_command(&self) -> String {
        let mut shown = String::new();
        if let Some(count) = self.count {
            shown.push_str(&count.to_string());
        }
        shown.push_str(&self.pending);
        shown
    }

    /// The lines a visual selection covers, low to high.
    pub fn selected_lines(&self) -> Option<(usize, usize)> {
        (self.mode == Mode::Visual)
            .then(|| (self.anchor.min(self.line), self.anchor.max(self.line)))
    }

    /// The text a charwise span covers, both ends included. The ends arrive in
    /// order — see [`crate::Selection::buffer_span`].
    pub fn text_in(&self, from: crate::Place, to: crate::Place) -> String {
        span_text(&self.lines(), from, to)
    }

    /// The text a linewise span covers: whole lines, each with the break that
    /// ends it. Both ends included, and the ends arrive low first — see
    /// [`Buffer::selected_lines`], which is where the span comes from.
    pub fn lines_in(&self, from: usize, to: usize) -> String {
        self.lines()
            .into_iter()
            .skip(from - 1)
            .take(to + 1 - from)
            .map(|line| format!("{line}\n"))
            .collect()
    }

    /// The unnamed register's contents, for the keys that copy as well as yank.
    pub fn register_text(&self) -> Option<String> {
        (!self.register.is_empty()).then(|| self.register.join("\n"))
    }

    pub fn yank_in(&mut self, from: crate::Place, to: crate::Place) {
        self.register = self
            .text_in(from, to)
            .split('\n')
            .map(str::to_string)
            .collect();
    }

    /// Removes exactly the characters the span covers, so a span inside one
    /// line takes part of it and a span across a break joins what is left.
    pub fn delete_in(&mut self, from: crate::Place, to: crate::Place) {
        self.yank_in(from, to);
        let mut lines = self.lines();
        let last = to.line.min(lines.len());
        if from.line > lines.len() {
            return;
        }
        self.remember(Step::Other);
        let head: String = lines[from.line - 1].chars().take(from.column - 1).collect();
        let tail: String = lines[last - 1].chars().skip(to.column).collect();
        lines.splice(from.line - 1..last, [format!("{head}{tail}")]);
        self.line = from.line;
        self.column = from.column;
        self.set(&lines);
        self.clamp();
    }

    /// Types `text` at every place, each over the `width` characters that
    /// follow it, and answers where each place ended up — in the order they
    /// arrived, so the caller keeps track of which of them the cursor is on.
    ///
    /// One edit and one undo step however many places there are: a keystroke
    /// landing at six occurrences and costing six `u` presses to take back is
    /// a gesture nobody would use twice.
    ///
    /// The places are applied in document order and each one carries the shift
    /// the ones before it on its line left behind, which is what keeps two
    /// occurrences of a word on one line both landing where they were named.
    pub fn replace_at(
        &mut self,
        places: &[crate::Place],
        width: usize,
        text: &str,
    ) -> Vec<crate::Place> {
        let mut lines = self.lines();
        let mut order: Vec<usize> = (0..places.len()).collect();
        order.sort_by_key(|&index| (places[index].line, places[index].column));
        let mut landed = places.to_vec();
        let typed = text.chars().count();
        let (mut previous, mut shift) = (0usize, 0isize);
        self.remember(Step::Other);
        for index in order {
            let place = places[index];
            if place.line != previous {
                previous = place.line;
                shift = 0;
            }
            let Some(line) = lines.get_mut(place.line - 1) else {
                continue;
            };
            let chars: Vec<char> = line.chars().collect();
            let from = (place.column.saturating_add_signed(shift).max(1) - 1).min(chars.len());
            let to = (from + width).min(chars.len());
            let mut replaced: String = chars[..from].iter().collect();
            replaced.push_str(text);
            replaced.extend(&chars[to..]);
            *line = replaced;
            shift += typed as isize - (to - from) as isize;
            landed[index] = crate::Place {
                line: place.line,
                column: from + 1 + typed,
            };
        }
        self.set(&lines);
        self.clamp();
        landed
    }

    /// Typing over picked characters: they go and the typed one stands in for
    /// them. One snapshot, so `u` costs the one key the replacement cost — the
    /// same reason [`Buffer::paste`] and [`Buffer::reformat`] take one.
    pub fn replace_in(&mut self, from: crate::Place, to: crate::Place, key: char) {
        self.delete_in(from, to);
        let remembered = self.undo.len();
        self.insert(key);
        self.undo.truncate(remembered);
        self.clamp();
    }

    fn insert(&mut self, key: char) {
        let (left, right) = self.either_side();
        // Typing a closing character where that character already sits steps
        // over it instead of doubling it. Nothing tracks which halves the
        // editor inserted: what is to the right is the whole condition, and it
        // is what every editor that pairs checks.
        if right == Some(key) && PAIRS.iter().any(|(_, close)| *close == key) {
            self.column += 1;
            return;
        }
        if key == '\n' {
            self.new_line(left, right);
            return;
        }
        self.remember(match key {
            ' ' => Step::Space,
            _ => Step::Typing,
        });
        let mut lines = self.lines();
        let line = &mut lines[self.line - 1];
        let at = byte_index(line, self.column - 1);
        line.insert(at, key);
        self.column += 1;
        // The partner, placed after the cursor rather than typed at it. A
        // quote directly after a word character is an apostrophe or a
        // lifetime, so it stays a single quote — the one judgement in the
        // rules, and the reason no per-language table is needed yet.
        let word = left.is_some_and(char::is_alphanumeric);
        if let Some(close) = closes(key).filter(|close| !(word && *close == key)) {
            line.insert(byte_index(line, self.column - 1), close);
        }
        self.set(&lines);
    }

    /// Enter: the line splits at the cursor and the new one keeps its place.
    ///
    /// The indentation is *copied* rather than measured and re-emitted, so a
    /// file's own mix of tabs and spaces comes back unchanged.
    ///
    /// Between the halves of a bracket pair it opens a block instead. A quote
    /// is its own partner, so `open == close` is what keeps a string out of
    /// that — three lines of unterminated quote is not a block — and nothing
    /// here has to name a quote to say so.
    fn new_line(&mut self, left: Option<char>, right: Option<char>) {
        self.remember(Step::Typing);
        let mut lines = self.lines();
        let line = &lines[self.line - 1];
        let at = byte_index(line, self.column - 1);
        let (head, tail) = (line[..at].to_string(), line[at..].to_string());
        let indent: String = head.chars().take_while(|c| c.is_whitespace()).collect();
        let block = matches!((left, right), (Some(open), Some(close))
            if closes(open) == Some(close) && open != close);
        let deeper = if block {
            self.shape.unit.clone()
        } else {
            String::new()
        };
        self.column = indent.chars().count() + deeper.chars().count() + 1;
        let opened = format!("{indent}{deeper}");
        let split = if block {
            vec![head, opened, format!("{indent}{tail}")]
        } else {
            vec![head, opened + &tail]
        };
        lines.splice(self.line - 1..self.line, split);
        self.line += 1;
        self.set(&lines);
    }

    /// Pasted text, as one edit. A paste is not typing: run through `insert` a
    /// character at a time it would pair every bracket in what was pasted, and
    /// it would take as many presses to undo as it had characters.
    pub fn paste(&mut self, text: &str) {
        self.remember(Step::Other);
        let mut lines = self.lines();
        let line = &mut lines[self.line - 1];
        let at = byte_index(line, self.column - 1);
        line.insert_str(at, text);
        match text.rsplit_once('\n') {
            Some((before, after)) => {
                self.line += before.matches('\n').count() + 1;
                self.column = after.chars().count() + 1;
            }
            None => self.column += text.chars().count(),
        }
        self.set(&lines);
    }

    /// The text edits a server sent back, applied as one edit.
    ///
    /// **Back to front**, and that is the whole of the care this needs: every
    /// range in a reply is measured against the document the server was sent,
    /// so applying the first edit forwards moves the text every later range
    /// names. Sorted here rather than trusted to arrive in any order, since
    /// nothing in the protocol promises one.
    ///
    /// One `remember`, so `u` undoes the reformat. A reader pressing
    /// it means "undo the thing that just happened to my file", and a reply of
    /// six edits that takes six presses is an edit they did not make, handed
    /// back one sixth at a time.
    ///
    /// The span is the protocol's, not the editor's: `until` is the first place
    /// the edit does not cover — unlike [`Buffer::text_in`] and its
    /// neighbours, whose spans include both ends — which is what lets an edit
    /// that only inserts name a span of nothing. Both ends are clamped to the
    /// text: a range is a number from a child process before it is a place in a
    /// file.
    pub fn reformat(&mut self, edits: &[(crate::Place, crate::Place, String)]) {
        let lines = self.lines();
        // Reversed before it is sorted, so that two edits naming the *same*
        // place keep the protocol's rule that they appear in the order the
        // array gave them: applied back to front, the last one goes in first
        // and the one before it lands to its left. A stable sort over the array
        // as it came would put them in the buffer backwards.
        let mut spans: Vec<(usize, usize, &str)> = edits
            .iter()
            .rev()
            .map(|(from, until, text)| {
                (offset(&lines, *from), offset(&lines, *until), text.as_str())
            })
            .collect();
        spans.sort_by_key(|(from, _, _)| std::cmp::Reverse(*from));
        let mut text: Vec<char> = self.shown().chars().collect();
        let mut at = offset(
            &lines,
            crate::Place {
                line: self.line,
                column: self.column,
            },
        );
        self.remember(Step::Other);
        for (from, until, replacement) in spans {
            let from = from.min(text.len());
            let until = until.clamp(from, text.len());
            let replacement: Vec<char> = replacement.chars().collect();
            // The cursor rides the edits, in the three positions it can be in
            // relative to one. After it: text put in or taken out moves the
            // cursor by as much, or a brace that snapped four columns left
            // leaves the cursor four columns right of it. Inside it: there is
            // nowhere of its own left to be, so it lands at the end of what
            // replaced it. Before it: nothing changed for it.
            at = if at >= until {
                at - (until - from) + replacement.len()
            } else if at > from {
                from + replacement.len()
            } else {
                at
            };
            text.splice(from..until, replacement);
        }
        let text: String = text.into_iter().collect();
        self.set(
            &text
                .split('\n')
                .map(str::to_string)
                .collect::<Vec<String>>(),
        );
        // Through the tail the stops are kept as, for the reason they are kept
        // that way: `at` is an offset into the text the edits left behind, and
        // that is the text this reads it back off.
        if let Some(place) = self.place_before(Tail(text.chars().count().saturating_sub(at))) {
            self.go_to_place(place);
        }
    }

    /// A pair either side of a span, keeping what it covers.
    ///
    /// The closing half goes in first: inserting the opening one moves the end
    /// of a span that lies on the same line. Not typed at either end — a typed
    /// closing character would step over whatever is already there.
    pub fn wrap_in(&mut self, from: crate::Place, to: crate::Place, open: char, close: char) {
        let mut lines = self.lines();
        if from.line > lines.len() {
            return;
        }
        self.remember(Step::Other);
        let last = to.line.min(lines.len());
        let line = &mut lines[last - 1];
        line.insert(byte_index(line, to.column), close);
        let line = &mut lines[from.line - 1];
        line.insert(byte_index(line, from.column - 1), open);
        self.set(&lines);
    }

    /// The character to the left of the cursor, then the one to its right —
    /// all the pairing rules look at.
    fn either_side(&self) -> (Option<char>, Option<char>) {
        let lines = self.lines();
        let line: Vec<char> = lines
            .get(self.line - 1)
            .map(|line| line.chars().collect())
            .unwrap_or_default();
        (
            self.column
                .checked_sub(2)
                .and_then(|at| line.get(at).copied()),
            line.get(self.column - 1).copied(),
        )
    }

    fn command(&mut self, key: char) -> Option<String> {
        // A leading 0 is the motion; a 0 after digits is part of the count.
        if key.is_ascii_digit() && (key != '0' || self.count.is_some()) {
            let digit = key as usize - '0' as usize;
            self.count = Some(self.count.unwrap_or(0) * 10 + digit);
            return None;
        }

        if !self.pending.is_empty() {
            let chord = std::mem::take(&mut self.pending);
            return self.operator(&chord, key);
        }

        if self.mode == Mode::Visual && self.over_selection(key) {
            return None;
        }

        // An operator must not consume the count: in `2dd` the 2 belongs to the
        // dd that follows, not to the first d.
        if matches!(key, 'g' | 'd' | 'y') {
            self.pending.push(key);
            return None;
        }

        let times = self.count.take().unwrap_or(1);
        for _ in 0..times {
            self.motion(key);
        }
        None
    }

    /// An operator and the key that follows it, with the count the operator
    /// was not allowed to consume. A doubled operator — `dd`, `yy`, `gg` —
    /// acts on the line the cursor is on; `d` composes with the motion the key
    /// already names, so the operators grow with [`Buffer::moved`] rather than
    /// with a case per pair. `Some` is the chord that named no motion — the one
    /// outcome a buffer cannot report by changing itself, since refusing is
    /// exactly the case where nothing changed.
    fn operator(&mut self, chord: &str, key: char) -> Option<String> {
        // `dgg` is three keys, so `dg` is not an answer yet: the operator keeps
        // waiting, and the count waits with it.
        if chord == "d" && key == 'g' {
            self.pending = format!("{chord}{key}");
            return None;
        }
        let times = self.count.take().unwrap_or(1);
        match (chord, key) {
            ("d", 'd') => self.delete_lines(self.line, self.line + times - 1),
            ("y", 'y') => self.yank(self.line, self.line + times - 1),
            ("g", 'g') => {
                self.line = 1;
                self.column = 1;
            }
            // `dgg` arrives with the `g` already in the chord, so the key that
            // completes it takes the lines back to where `gg` lands.
            ("dg", 'g') => self.delete_lines(1, self.line),
            ("d", key) => return self.delete_over(key, times),
            _ => return Some(format!("{chord}{key}")),
        }
        None
    }

    /// A delete over the span a motion covers, or the chord itself when the key
    /// names no motion at all.
    ///
    /// The motion runs on a copy of the buffer, which is what keeps the
    /// operator and the motions from ever disagreeing about where a key lands:
    /// there is one definition of `G`, and `dG` reads its answer off a cursor
    /// that took it.
    fn delete_over(&mut self, key: char, times: usize) -> Option<String> {
        let mut probe = self.clone();
        if !probe.moved(key) {
            return Some(format!("d{key}"));
        }
        for _ in 1..times {
            probe.moved(key);
        }
        probe.clamp();
        // Which motions take whole lines is the operator's one table; vim keeps
        // the same one.
        if matches!(key, 'j' | 'k' | 'G') {
            self.delete_lines(self.line.min(probe.line), self.line.max(probe.line));
            return None;
        }
        let (here, there) = (self.offset(), probe.offset());
        let mut to = here.max(there);
        // `$` and `e` stop *on* the character they name rather than after it,
        // so their span includes it — the inclusive motions, as vim has them.
        // Only when there is one: the break at the end of an empty line is not
        // a character of that line, and `d$` there has nothing to take.
        if matches!(key, '$' | 'e') && self.shown().chars().nth(to) != Some('\n') {
            to += 1;
        }
        self.take_chars(here.min(there), to);
        None
    }

    /// An operator over the visual selection, which needs no second key.
    /// `false` for anything else, which is a key visual mode does not claim.
    fn over_selection(&mut self, key: char) -> bool {
        let (from, to) = self.selected_lines().expect("visual");
        match key {
            'd' => {
                self.mode = Mode::Normal;
                self.delete_lines(from, to);
                true
            }
            'y' => {
                self.yank(from, to);
                self.line = from;
                self.mode = Mode::Normal;
                true
            }
            _ => false,
        }
    }

    /// The two key sets are disjoint, so both run rather than one being chosen.
    fn motion(&mut self, key: char) {
        self.moved(key);
        self.edited(key);
    }

    /// The keys that only move the caret, over this buffer's own lines.
    /// `false` for a key that names no motion, which is what lets an operator
    /// refuse one out loud instead of swallowing it.
    fn moved(&mut self, key: char) -> bool {
        let at = crate::Place {
            // A folded block is one line to step over rather than one per line
            // it hides; `clamp` catches every other motion that lands inside
            // one and puts the caret back on the line that opens it.
            line: match key {
                'j' => self.past_fold(self.line),
                _ => self.line,
            },
            column: self.column,
        };
        match moved(&self.lines(), at, key) {
            Some(place) => {
                self.line = place.line;
                self.column = place.column;
                true
            }
            None => false,
        }
    }

    /// The keys that change the buffer or the mode.
    fn edited(&mut self, key: char) {
        match key {
            'V' => {
                self.mode = Mode::Visual;
                self.anchor = self.line;
            }
            'p' => self.put(),
            'i' => self.mode = Mode::Insert,
            'a' => {
                self.mode = Mode::Insert;
                self.column += 1;
            }
            'o' => self.open_line(1),
            'O' => self.open_line(0),
            'x' => self.delete_char(),
            'u' => self.undo(),
            'U' => self.redo(),
            _ => {}
        }
    }

    fn delete_char(&mut self) {
        let mut lines = self.lines();
        let line = &mut lines[self.line - 1];
        if line.is_empty() {
            return;
        }
        self.remember(Step::Other);
        let at = byte_index(line, self.column - 1);
        line.remove(at);
        self.set(&lines);
    }

    /// A run of whole lines, into the register and out of the buffer in one
    /// edit — so `u` costs the one key the delete cost, rather than one per
    /// line the way a delete repeated per line used to.
    ///
    /// Every line of the span goes, including the last: deleting one line at a
    /// time and stopping while the buffer still held one left the survivor
    /// standing with the register claiming it had been taken, so `dG` from the
    /// first line kept the last and `p` put a copy of it back.
    fn delete_lines(&mut self, from: usize, to: usize) {
        self.yank(from, to);
        let mut lines = self.lines();
        let to = to.min(lines.len());
        if from > to {
            return;
        }
        self.remember(Step::Other);
        lines.drain(from - 1..to);
        // A buffer always holds a line, so taking every one of them leaves an
        // empty line rather than no line — as it does in vim.
        if lines.is_empty() {
            lines.push(String::new());
        }
        self.line = from;
        self.set(&lines);
    }

    /// A run of whole lines moved one level in or out, as one edit — so `u`
    /// costs the one key the indent cost, whatever the span covered.
    ///
    /// The level is the file's own [`indent_unit`], for the reason Enter uses
    /// it: a tab-indented file stays tab-indented, and only a file with no
    /// indentation to copy falls back to `editor.tab_width`.
    ///
    /// An empty line is skipped rather than filled with whitespace nobody
    /// typed. Coming back out takes the unit when the line starts with it and
    /// up to that many leading spaces otherwise, so a file mixing tabs and
    /// spaces gets shallower either way instead of refusing.
    pub fn indent_lines(&mut self, from: usize, to: usize, deeper: bool) {
        let mut lines = self.lines();
        let to = to.min(lines.len());
        if from > to {
            return;
        }
        let unit = self.shape.unit.clone();
        if unit.is_empty() {
            return;
        }
        self.remember(Step::Other);
        for line in &mut lines[from - 1..to] {
            if deeper {
                if !line.is_empty() {
                    line.insert_str(0, &unit);
                }
            } else if let Some(rest) = line.strip_prefix(&unit) {
                *line = rest.to_string();
            } else {
                let spaces = line
                    .chars()
                    .take(unit.chars().count())
                    .take_while(|c| *c == ' ')
                    .count();
                *line = line.chars().skip(spaces).collect();
            }
        }
        self.set(&lines);
        // The cursor ends where the block does, so the caller reading it back
        // has the end of the span it just moved rather than a column that was
        // measured against the old indentation.
        self.line = to;
        self.column = usize::MAX;
        self.clamp();
    }

    /// Removes the characters a half-open span of offsets covers, register
    /// filled with what it took.
    ///
    /// Offsets rather than the [`crate::Place`]s [`Buffer::delete_in`] takes,
    /// because no column names a line break: the character behind a cursor
    /// sitting at column 1 is the break above it, and `db` from there takes the
    /// word above and joins the two lines it spanned. Half-open for the same
    /// reason a motion is — `b` lands on the first character of the word
    /// behind, and the character the cursor sat on is the one after the last
    /// that `db` takes.
    fn take_chars(&mut self, from: usize, to: usize) {
        let text = self.shown().to_string();
        let taken: String = text.chars().skip(from).take(to - from).collect();
        // A motion that reached where it already was covers no characters, and
        // an empty span is not an edit: `dh` at column 1 leaves the register
        // and the undo stack alone.
        if taken.is_empty() {
            return;
        }
        self.register = taken.split('\n').map(str::to_string).collect();
        self.remember(Step::Other);
        let kept: String = text
            .chars()
            .take(from)
            .chain(text.chars().skip(to))
            .collect();
        self.set(
            &kept
                .split('\n')
                .map(str::to_string)
                .collect::<Vec<String>>(),
        );
        if let Some(place) = place_at(&self.lines(), from) {
            self.go_to_place(place);
        }
    }

    fn open_line(&mut self, offset: usize) {
        self.remember(Step::Other);
        let mut lines = self.lines();
        lines.insert(self.line - 1 + offset, String::new());
        self.line += offset;
        self.column = 1;
        self.mode = Mode::Insert;
        self.set(&lines);
    }

    /// Offset of the cursor within the whole buffer, so word motions can cross
    /// line boundaries without special cases.
    fn offset(&self) -> usize {
        let lines = self.lines();
        lines[..self.line - 1]
            .iter()
            .map(|line| line.chars().count() + 1)
            .sum::<usize>()
            + self.column
            - 1
    }

    /// Word motions move in every mode, the way [`Buffer::arrow`] does. The
    /// keys that spell them do not: `w` while inserting is the letter, which is
    /// what Alt+Right used to type into the line instead of moving.
    ///
    /// One definition of where a word ends, shared with the keys that spell
    /// them: [`moved`] over this buffer's own lines. Only the clamp differs,
    /// and that is the caller's — insert mode may sit one column past the last
    /// character, where normal mode may not, and a Preview clamps against a
    /// rendered row instead of a line.
    pub fn word_motion(&mut self, stop: Word) {
        let key = match stop {
            Word::Start => 'w',
            Word::End => 'e',
            Word::Back => 'b',
        };
        let at = crate::Place {
            line: self.line,
            column: self.column,
        };
        if let Some(place) = moved(&self.lines(), at, key) {
            self.go_to_place(place);
        }
    }

    fn yank(&mut self, from: usize, to: usize) {
        let lines = self.lines();
        let to = to.min(lines.len());
        self.register = lines[from - 1..to].to_vec();
    }

    fn put(&mut self) {
        if self.register.is_empty() {
            return;
        }
        self.remember(Step::Other);
        let mut lines = self.lines();
        let at = self.line.min(lines.len());
        for (offset, text) in self.register.clone().into_iter().enumerate() {
            lines.insert(at + offset, text);
        }
        self.set(&lines);
    }

    /// Public for the Ctrl and Command spellings, which reach a buffer in
    /// insert mode too, where `u` is a letter.
    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            let at = crate::Place {
                line: self.line,
                column: self.column,
            };
            self.redo.push((self.shown().to_string(), at));
            self.draft = (previous != self.disk).then_some(previous);
            self.changed();
        }
        self.step = Step::Other;
    }

    /// Puts back what the last undo took, cursor included, so `u` then `U`
    /// leaves the buffer as it was before the `u`.
    pub fn redo(&mut self) {
        if let Some((next, at)) = self.redo.pop() {
            self.undo.push(self.shown().to_string());
            self.draft = (next != self.disk).then_some(next);
            self.changed();
            self.go_to_place(at);
        }
    }

    /// The bracket pair the cursor is inside, innermost first — or nothing,
    /// which is a cursor inside no pair at all. Marked so a pair reads as a
    /// pair *where the reader is*: every bracket on screen marked at once is a
    /// box on punctuation nobody is looking at.
    ///
    /// Resting on a bracket counts as being inside its own pair, which is what
    /// makes stepping onto one show what it closes.
    ///
    /// Brackets only, so [`closes`] is not the table asked: a quote is its own
    /// partner there, and a marked pair of quotes is not what a bracket mark
    /// is for.
    ///
    /// A bracket inside a string or a comment is counted like any other. The
    /// tokens that would tell them apart are the edge's, re-parsed per
    /// revision, and a mark one column wide is not worth threading them
    /// through — a mismatched closer is passed over rather than guessed at.
    ///
    /// Looked up in the pairs the revision found rather than read off the
    /// text: the editor asks on every frame, and a cursor deep in a big file
    /// was a read from the top of it down to the cursor (#101, #104).
    pub fn bracket_pair(&self) -> Option<(crate::Place, crate::Place)> {
        let at = (self.line, self.column);
        let brackets = &self.shape.brackets;
        // The last bracket opened at or before the cursor. Pairs nest, so the
        // pair around the cursor is that one or one of the brackets that were
        // open when it was.
        let mut within = brackets
            .partition_point(|(from, ..)| (from.line, from.column) <= at)
            .checked_sub(1);
        while let Some(index) = within {
            let (from, to, held) = brackets[index];
            if let Some(to) = to.filter(|to| at <= (to.line, to.column)) {
                return Some((from, to));
            }
            within = held;
        }
        None
    }

    /// Where the guides down this buffer's indentation go, one entry per line
    /// of `lines` — none past the last one. The block the cursor is inside is the active
    /// one, and it is the only thing the cursor decides here — a guide is a
    /// fact about the text, so the active one is the same line drawn with more
    /// weight and not a line of a different kind.
    ///
    /// Only the lines asked for, and only as much around them as a blank line
    /// and the cursor's block reach: the editor asks for its window, and
    /// working out every line's guides on every frame was a big file's frame
    /// spent on lines nobody sees (#101).
    pub fn guides(&self, lines: impl IntoIterator<Item = usize>) -> Vec<Vec<Guide>> {
        let unit = self.shape.unit.chars().count().max(1);
        let indent = |index: usize| indent_at(&self.shape.indents, index);
        let column = active_column(indent, unit, self.line - 1);
        let run = column.and_then(|column| block(indent, column, self.line - 1));
        lines
            .into_iter()
            .map(|number| {
                let index = number - 1;
                (0..indent(index).unwrap_or(0))
                    .step_by(unit)
                    .map(|at| Guide {
                        column: at,
                        active: column == Some(at)
                            && run.is_some_and(|(from, to)| index >= from && index <= to),
                    })
                    .collect()
            })
            .collect()
    }

    /// `:w`. The disk-change flag was the warning; writing goes ahead anyway.
    pub fn write(&mut self) -> String {
        let contents = self.shown().to_string();
        self.disk = contents.clone();
        self.draft = None;
        // Writing settles a divergence: what is in the buffer is now what is on
        // disk. Leaving the flag standing would keep offering a resolution to a
        // file with nothing left to resolve — and the watcher event that used to
        // clear it never arrives for a folder nobody is watching.
        self.changed_on_disk = false;
        self.revision += 1;
        contents
    }
}

/// Both versions of a diverged file, handed to the AI to reconcile. Inline for
/// the same reason a review's is (see [`crate::review::prompt`]): a path alone
/// is only half the story here, since the buffer's version exists nowhere on
/// disk for the CLI to read.
pub fn merge_prompt(path: &str, disk: &str, buffer: &str) -> String {
    format!(
        "{path} changed on disk while it had unsaved edits in Varde.\n\
         Merge the two and write the result to {path}.\n\n\
         --- on disk ---\n{disk}\n\
         --- unsaved in Varde ---\n{buffer}"
    )
}

/// Where a word motion lands: the start of the next word (`w`), the end of the
/// run ahead (`e`), or the start of the one behind (`b`).
pub enum Word {
    Start,
    End,
    Back,
}

impl Word {
    /// The stop a sideways word gesture — Alt+arrow, or Option's `^[b` and
    /// `^[f` — lands on: forwards to the next word's start, anything else
    /// back to the start of the one behind.
    pub fn toward(direction: crate::Direction) -> Self {
        match direction {
            crate::Direction::Right => Word::Start,
            _ => Word::Back,
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Class {
    Space,
    Word,
    Punctuation,
}

/// The run of one character class around a column of `line`, as 1-based
/// inclusive columns — what a double-click picks. `None` on whitespace and past
/// the end of the line: there is no word under the pointer there.
///
/// Vim's classes rather than identifier characters alone, so a double-click on
/// `==` picks the operator the way it picks a name.
pub fn word_span(line: &str, column: usize) -> Option<(usize, usize)> {
    let text: Vec<char> = line.chars().collect();
    let at = column.checked_sub(1)?;
    let here = class(text.get(at).copied());
    if here == Class::Space {
        return None;
    }
    let mut from = at;
    while from > 0 && class(text.get(from - 1).copied()) == here {
        from -= 1;
    }
    let mut to = at;
    while class(text.get(to + 1).copied()) == here {
        to += 1;
    }
    Some((from + 1, to + 1))
}

/// Vim's three character classes: a word motion stops where the class changes.
/// Leave the run you are on, then skip whitespace: the start of the next word.
fn next_word_start(text: &[char], mut at: usize) -> usize {
    let from = class(text.get(at).copied());
    while at < text.len() && class(text.get(at).copied()) == from {
        at += 1;
    }
    while at < text.len() && class(text.get(at).copied()) == Class::Space {
        at += 1;
    }
    at
}

/// The end of the run ahead of you.
fn word_end(text: &[char], mut at: usize) -> usize {
    at += 1;
    while at < text.len() && class(text.get(at).copied()) == Class::Space {
        at += 1;
    }
    while at + 1 < text.len() && class(text.get(at + 1).copied()) == class(text.get(at).copied()) {
        at += 1;
    }
    at
}

/// The start of the run behind you, whitespace skipped first.
fn previous_word_start(text: &[char], mut at: usize) -> usize {
    at = at.saturating_sub(1);
    while at > 0 && class(text.get(at).copied()) == Class::Space {
        at -= 1;
    }
    let run = class(text.get(at).copied());
    while at > 0 && class(text.get(at - 1).copied()) == run {
        at -= 1;
    }
    at
}

fn class(character: Option<char>) -> Class {
    match character {
        Some(c) if c.is_alphanumeric() || c == '_' => Class::Word,
        Some(c) if c.is_whitespace() => Class::Space,
        Some(_) => Class::Punctuation,
        None => Class::Space,
    }
}

/// The pairs the editor holds the other end of, spelled once. One table
/// rather than an arm per character for the reason `[lsp.*]` is a table: a
/// quote is its own partner, which the table says and a pair of arms would
/// have to remember to.
const PAIRS: [(char, char); 5] = [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')];

/// The bracket a bracket shuts, and nothing for anything else. Brackets only,
/// which is why this is not [`PAIRS`]: that table pairs a quote with itself,
/// because the editor types the other half of one.
fn shuts(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

/// What the editor reads off a buffer's whole text on every frame, worked out
/// once per revision (#104): the guides and the bracket pair are asked for with
/// every frame, and each of them read the whole file to answer, which is what
/// a frame of a big file cost with the cursor deep in it.
#[derive(Debug, PartialEq, Eq)]
struct Shape {
    /// Where each line starts, in bytes.
    starts: Vec<usize>,
    /// Each line's own indentation in characters, or nothing for a blank line.
    indents: Vec<Option<usize>>,
    /// [`indent_unit`]'s answer.
    unit: String,
    /// Every bracket opened, in the order it was: where, where it closed — if
    /// it did — and which of these was still open around it. A mismatched
    /// closer is passed over rather than guessed at.
    brackets: Vec<(crate::Place, Option<crate::Place>, Option<usize>)>,
}

impl Shape {
    fn of(text: &str, tab_width: usize) -> Self {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut starts = Vec::with_capacity(lines.len());
        let mut brackets: Vec<(crate::Place, Option<crate::Place>, Option<usize>)> = Vec::new();
        let mut open: Vec<(usize, char)> = Vec::new();
        let mut start = 0;
        for (index, line) in lines.iter().enumerate() {
            starts.push(start);
            start += line.len() + 1;
            for (offset, character) in line.chars().enumerate() {
                let here = crate::Place {
                    line: index + 1,
                    column: offset + 1,
                };
                if let Some(close) = shuts(character) {
                    brackets.push((here, None, open.last().map(|(held, _)| *held)));
                    open.push((brackets.len() - 1, close));
                } else if open.last().is_some_and(|(_, close)| *close == character) {
                    let (opened, _) = open.pop().expect("matched above");
                    brackets[opened].1 = Some(here);
                }
            }
        }
        Shape {
            starts,
            indents: lines.iter().map(|line| crate::fold::indent(line)).collect(),
            unit: indent_unit(&lines, tab_width),
            brackets,
        }
    }
}

pub fn closes(open: char) -> Option<char> {
    PAIRS
        .iter()
        .find(|(candidate, _)| *candidate == open)
        .map(|(_, close)| *close)
}

/// One level of indentation, taken from the shallowest the file already has.
///
/// It has to come from the file rather than from a number: a `4` taken first
/// is the answer that is wrong in every JavaScript project the reader opens.
/// `fallback` is `editor.tab_width` and is reached only where the file holds
/// nothing to measure, which is the *one* thing configuration decides here —
/// the project's number cannot overrule the lines in front of the reader.
/// Shallowest rather than first, and blank lines skipped, because
/// carrying the indentation down *leaves whitespace-only lines behind* — one
/// abandoned above would otherwise set the unit for everything below it.
fn indent_unit(lines: &[&str], fallback: usize) -> String {
    lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| &line[..line.len() - line.trim_start().len()])
        .filter(|indent| !indent.is_empty())
        .min_by_key(|indent| indent.len())
        .map(str::to_string)
        // `editor.tab_width` only where the file holds no indentation to copy.
        // The first block opened in it is then the evidence every later one
        // reads. A four written in here instead was the second indent site
        // disagreeing with the first: Tab laid the configured width and Enter
        // laid four, in the same file, on the same press of the same project's
        // settings.
        .unwrap_or_else(|| " ".repeat(fallback))
}

/// How many characters of the text come before a place, counting the line
/// breaks — the one measure an edit can be applied at, since a span may cross
/// as many lines as it likes. Clamped at both ends: a line the text does not
/// have is its end, and a column past the end of a line is the end of that
/// line.
fn offset(lines: &[String], at: crate::Place) -> usize {
    let above = (at.line - 1).min(lines.len());
    let before: usize = lines[..above]
        .iter()
        .map(|line| line.chars().count() + 1)
        .sum();
    let width = lines.get(above).map_or(0, |line| line.chars().count());
    before + (at.column - 1).min(width)
}

fn byte_index(line: &str, chars: usize) -> usize {
    line.char_indices()
        .nth(chars)
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

/// One column of a line's leading whitespace where a level of indentation
/// starts. `active` is the block the cursor is inside, drawn solid where the
/// others are dotted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guide {
    pub column: usize,
    pub active: bool,
}

/// One line's indentation in characters, or nothing past the last line. A
/// whitespace-only line has none of its own, so it takes the shallower of its
/// neighbours' — which carries a guide unbroken through the blank line inside
/// a block without drawing one past the end of it.
fn indent_at(indents: &[Option<usize>], index: usize) -> Option<usize> {
    Some(indents.get(index)?.unwrap_or_else(|| {
        let above = indents[..index].iter().rev().flatten().next();
        let below = indents[index + 1..].iter().flatten().next();
        (*above.unwrap_or(&0)).min(*below.unwrap_or(&0))
    }))
}

/// Which guide the cursor's block draws, or nothing where the cursor is in no
/// block at all. A line that opens a deeper one counts as inside what it
/// opens: the cursor on `if x {` is in the body it is about to hold, which is
/// where a reader would say it is.
fn active_column(
    indent: impl Fn(usize) -> Option<usize>,
    unit: usize,
    line: usize,
) -> Option<usize> {
    let here = indent(line)?;
    if indent(line + 1).is_some_and(|next| next > here) {
        return Some(here);
    }
    here.checked_sub(unit)
}

/// The run of lines the guide at `column` is drawn on around the cursor, so a
/// block is marked over its whole length rather than on the one line the
/// cursor sits on.
fn block(
    indent: impl Fn(usize) -> Option<usize>,
    column: usize,
    line: usize,
) -> Option<(usize, usize)> {
    let deep = |index: usize| indent(index).is_some_and(|indent| indent > column);
    let mut from = if deep(line) { line } else { line + 1 };
    if !deep(from) {
        return None;
    }
    let mut to = from;
    while from > 0 && deep(from - 1) {
        from -= 1;
    }
    while deep(to + 1) {
        to += 1;
    }
    Some((from, to))
}

#[cfg(test)]
mod tests {
    use super::{grid_row, link_at, moved, occurrences, span_text, word_span, Buffer, PAIRS};
    use crate::Place;

    #[test]
    fn a_link_is_the_url_under_the_column_and_nothing_else() {
        let row = "Sé (https://example.com/a_(b)). Not file:///etc/passwd or ftp://x.y";
        assert_eq!(
            link_at(row, 6).as_deref(),
            Some("https://example.com/a_(b)")
        );
        assert_eq!(link_at(row, 1), None, "a word before the link");
        assert_eq!(link_at(row, 31), None, "the sentence's own punctuation");
        assert_eq!(link_at(row, 40), None, "a scheme the opener would launch");
        assert_eq!(link_at(row, 60), None, "ftp");
        assert_eq!(link_at(row, 200), None, "past the end of the row");
        assert_eq!(link_at(row, 0), None);
    }

    /// A folded block is one line to step over, not one per line it hides —
    /// and nothing lands inside it, whichever way the caret arrives. The dots
    /// at the end of the line that opens it are the one column past the text
    /// only that line has, because they are a thing to press.
    #[test]
    fn a_fold_is_one_step_to_pass_and_its_dots_are_a_column_to_reach() {
        let mut buffer = Buffer::open("fn main() {\n    go();\n    stop();\n}\n", false, 4);
        crate::fold::toggle(&mut buffer, false);
        assert_eq!(buffer.folded, vec![1], "the block the cursor was in");

        buffer.arrow(crate::Direction::Down);
        assert_eq!(buffer.line, 4, "past the body rather than into it");
        buffer.arrow(crate::Direction::Up);
        assert_eq!(buffer.line, 1, "and back out of it on the way up");
        buffer.line = 4;
        buffer.key('k');
        assert_eq!(buffer.line, 1, "`k` lands on a hidden line and comes back");
        buffer.key('j');
        assert_eq!(buffer.line, 4, "and `j` steps the whole block in one");

        buffer.line = 1;
        buffer.key('$');
        assert_eq!(buffer.column, 12, "the end of a folded line is its dots");
        assert!(buffer.on_fold_dots());
        buffer.arrow(crate::Direction::Left);
        assert_eq!(buffer.column, 11, "the last character of the code");
        assert!(!buffer.on_fold_dots());
        buffer.arrow(crate::Direction::Right);
        assert!(buffer.on_fold_dots(), "and an arrow reaches them again");

        buffer.line = 4;
        buffer.key('$');
        buffer.arrow(crate::Direction::Right);
        assert!(
            !buffer.on_fold_dots(),
            "a line that opens no fold has no dots and no column past its text"
        );
    }

    fn place(line: usize, column: usize) -> Place {
        Place { line, column }
    }

    // The bug this exists for: a pty's blank cells carry no contents, so
    // collecting only what the cells hold collapsed the padding and every
    // column past the first gap named the wrong character. An AI CLI's output
    // is indented and full of wide glyphs, so a drag in it picked nothing at
    // all, while a drag over its unpadded prompt line looked fine.
    #[test]
    fn a_blank_grid_cell_keeps_its_column() {
        let cells = [Some("a"), Some(""), None, Some("b")];
        assert_eq!(grid_row(cells.into_iter()), "a  b");
    }

    // The second half of a wide character holds no contents either, and the
    // renderer draws a space there, so the text has to agree or the columns
    // after an emoji drift.
    #[test]
    fn a_wide_characters_second_cell_keeps_its_column() {
        let cells = [Some("\u{1f600}"), Some(""), Some("x")];
        assert_eq!(grid_row(cells.into_iter()), "\u{1f600} x");
    }

    #[test]
    fn a_grid_rows_trailing_blanks_are_not_text() {
        let cells = [Some("h"), Some("i"), Some(""), None];
        assert_eq!(grid_row(cells.into_iter()), "hi");
    }

    // The whole point: an indented, padded row picks by screen column.
    #[test]
    fn a_span_over_a_padded_grid_row_picks_by_screen_column() {
        let row = grid_row([Some(""), Some(""), Some("h"), Some("i")].into_iter());
        assert_eq!(span_text(&[row], place(1, 3), place(1, 4)), "hi");
    }

    // Enter in insert mode splits the line, so the cursor belongs at the head
    // of what moved down. Counting it as one more column left the cursor on
    // the line above, where `clamp` then pulled it back to the shortened
    // line's end — every typed newline dropped the cursor behind the text.
    #[test]
    fn a_typed_newline_carries_the_cursor_to_the_new_line() {
        let mut buffer = Buffer::open("one two", false, 4);
        for _ in 0..3 {
            buffer.key('l');
        }
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "one\n two");
        assert_eq!((buffer.line, buffer.column), (2, 1));
    }

    /// The Document version F31 sends is this number, and it counts from the
    /// read: a fresh Buffer at zero would be told to a server as a version it
    /// could read as one it already has.
    #[test]
    fn opening_is_already_a_revision() {
        assert_eq!(Buffer::open("fn main() {}", false, 4).revision(), 1);
    }

    #[test]
    fn editing_bumps_the_revision() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        let before = buffer.revision();
        buffer.key('x');
        assert_ne!(buffer.revision(), before, "an edit must invalidate a cache");
    }

    // Bug 11: the editor showed the old file until you closed and reopened it.
    // The text was right all along — `shown()` returned the new version — but
    // the edge parses tokens only when `revision` moves, and following the file
    // assigned `disk` without moving it, so the screen painted a stale parse.
    // Closing and reopening built a fresh Buffer at revision 0, which is why
    // that was the only thing that worked.
    #[test]
    fn following_the_file_bumps_the_revision() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        let before = buffer.revision();
        buffer.follow("three\nfour".to_string());
        assert_ne!(
            buffer.revision(),
            before,
            "a file that changed underneath must invalidate the render cache"
        );
    }

    #[test]
    fn reloading_bumps_the_revision() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        buffer.key('x');
        let before = buffer.revision();
        buffer.reload();
        assert_ne!(
            buffer.revision(),
            before,
            "dropping a draft changes what is on screen"
        );
    }

    #[test]
    fn a_clean_buffer_follows_the_file_and_is_not_flagged() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        buffer.follow("three".to_string());
        assert_eq!(buffer.shown(), "three");
        assert!(!buffer.changed_on_disk);
    }

    #[test]
    fn a_dirty_buffer_keeps_its_draft_and_is_flagged() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        buffer.key('x');
        let draft = buffer.shown().to_string();
        buffer.follow("three".to_string());
        assert_eq!(buffer.shown(), draft, "unsaved work is never overwritten");
        assert_eq!(buffer.disk, "three");
        assert!(buffer.changed_on_disk);
    }

    // A file that shrank underneath leaves the cursor on a line that is no
    // longer there, and every motion and render measures from it.
    #[test]
    fn a_file_that_shrank_pulls_the_cursor_back_into_it() {
        let mut buffer = Buffer::open("one\ntwo\nthree\nfour", false, 4);
        buffer.key('G');
        buffer.follow("one".to_string());
        assert_eq!(buffer.line, 1);
    }

    #[test]
    fn writing_settles_the_divergence_it_was_flagged_for() {
        let mut buffer = Buffer::open("one", false, 4);
        buffer.key('x');
        buffer.follow("changed underneath".to_string());
        assert!(buffer.changed_on_disk);
        buffer.write();
        assert!(!buffer.changed_on_disk);
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn a_span_across_a_break_takes_the_ends_partially_and_the_middle_whole() {
        let buffer = Buffer::open("one two\nthree\nfour five", false, 4);
        assert_eq!(buffer.text_in(place(1, 5), place(3, 4)), "two\nthree\nfour");
    }

    #[test]
    fn deleting_a_span_across_a_break_joins_what_is_left() {
        let mut buffer = Buffer::open("one two\nthree four", false, 4);
        buffer.delete_in(place(1, 4), place(2, 5));
        assert_eq!(buffer.shown(), "one four");
        assert_eq!((buffer.line, buffer.column), (1, 4));
    }

    /// The typed run goes and the rest of the line stays: completing what is
    /// half-typed must not eat the characters after the cursor, and inserting
    /// without taking the run leaves `worworkspace_root` — a completion the
    /// reader has to undo.
    #[test]
    fn completing_replaces_the_run_before_the_cursor_and_nothing_after_it() {
        let mut buffer = Buffer::open("let wor = other;", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 1, column: 8 });
        buffer.complete("workspace_root", &[]);
        assert_eq!(buffer.shown(), "let workspace_root = other;");
        assert_eq!(buffer.column, 19);
    }

    /// Nothing typed yet is a completion inserted whole — the case a
    /// take-the-word-before-the-cursor rule gets wrong by taking the space.
    #[test]
    fn completing_with_nothing_typed_inserts_the_whole_of_it() {
        let mut buffer = Buffer::open("let x = ", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 1, column: 9 });
        buffer.complete("other", &[]);
        assert_eq!(buffer.shown(), "let x = other");
    }

    /// A snippet's stops are offsets into the text it inserts, and the first
    /// of them is where the cursor lands: `println!("${1:msg}")$0` leaves the
    /// reader on `msg` with the call already written around it.
    ///
    /// Completed on the second line, because a stop is measured against the
    /// whole buffer and the newlines above it are characters of it.
    #[test]
    fn a_completed_snippet_puts_the_cursor_on_its_first_stop() {
        let mut buffer = Buffer::open("first\nlet x = ;\nlast", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 2, column: 9 });
        let left = buffer.complete(r#"println!("msg")"#, &[10, 15]);
        assert_eq!(buffer.shown(), "first\nlet x = println!(\"msg\");\nlast");
        assert_eq!((buffer.line, buffer.column), (2, 19));
        assert_eq!(
            buffer.place_before(left[0]),
            Some(crate::Place {
                line: 2,
                column: 24,
            })
        );
    }

    /// The reason a stop is kept as the characters that *follow* it: filling in
    /// one stop moves every stop after it, and a line and a column recorded
    /// when the text went in would send the last Tab of every completion into
    /// the middle of what was just typed.
    #[test]
    fn a_stop_survives_the_typing_done_at_the_stop_before_it() {
        let mut buffer = Buffer::open("let x = ;", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 1, column: 9 });
        let left = buffer.complete(r#"println!("msg")"#, &[10, 15]);
        assert_eq!(
            buffer.place_before(left[0]),
            Some(crate::Place {
                line: 1,
                column: 24,
            })
        );
        buffer.key('h');
        buffer.key('i');
        assert_eq!(buffer.shown(), r#"let x = println!("himsg");"#);
        assert_eq!(
            buffer.place_before(left[0]),
            Some(crate::Place {
                line: 1,
                column: 26,
            })
        );
    }

    /// A snippet of several lines is what a real server sends for a function,
    /// and a stop on its second line is a stop on the buffer's next line.
    #[test]
    fn a_stop_on_a_later_line_of_a_snippet_is_a_place_on_a_later_line() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        let left = buffer.complete("fn name() {\n    \n}", &[3, 16]);
        assert_eq!(buffer.shown(), "fn name() {\n    \n}");
        assert_eq!((buffer.line, buffer.column), (1, 4));
        assert_eq!(
            buffer.place_before(left[0]),
            Some(crate::Place { line: 2, column: 5 })
        );
    }

    /// A delete is one edit however many lines it takes, so `u` costs the one
    /// key the delete cost. `2dd` used to delete a line at a time and leave an
    /// undo entry per line, which took as many presses to get back as the
    /// delete saved.
    #[test]
    fn a_delete_over_several_lines_is_one_undo() {
        let mut buffer = Buffer::open("one\ntwo\nthree\nfour", false, 4);
        buffer.key('2');
        buffer.key('d');
        buffer.key('d');
        assert_eq!(buffer.shown(), "three\nfour");
        buffer.key('u');
        assert_eq!(buffer.shown(), "one\ntwo\nthree\nfour");
    }

    /// The end of the file is a line like any other. Deleting a line at a time
    /// and stopping while the buffer still held one left the last line
    /// standing while the register claimed to have taken it, so `dG` from the
    /// first line kept a line and `p` put a second copy of it back.
    #[test]
    fn dg_from_the_first_line_empties_the_buffer() {
        let mut buffer = Buffer::open("one\ntwo\nthree", false, 4);
        buffer.key('d');
        buffer.key('G');
        assert_eq!(buffer.shown(), "");
        buffer.key('p');
        assert_eq!(buffer.shown(), "\none\ntwo\nthree");
    }

    /// `$` and `e` land on the last character they name rather than after it,
    /// so an operator over them takes it too. An exclusive span for every
    /// motion left the last character of the line behind.
    #[test]
    fn the_inclusive_motions_take_the_character_they_land_on() {
        let mut buffer = Buffer::open("one two three\n", false, 4);
        buffer.go_to_place(crate::Place { line: 1, column: 5 });
        buffer.key('d');
        buffer.key('$');
        assert_eq!(buffer.shown(), "one \n");

        let mut buffer = Buffer::open("one two three", false, 4);
        buffer.go_to_place(crate::Place { line: 1, column: 5 });
        buffer.key('d');
        buffer.key('e');
        assert_eq!(buffer.shown(), "one  three");
    }

    /// An empty line has no last character, so there is nothing for `d$` to
    /// include — least of all the break that ends it, which belongs to no
    /// line and would pull the line below up.
    #[test]
    fn d_to_the_line_end_of_an_empty_line_takes_nothing() {
        let mut buffer = Buffer::open("\ntwo", false, 4);
        buffer.key('d');
        buffer.key('$');
        assert_eq!(buffer.shown(), "\ntwo");
    }

    /// A motion that went nowhere is not an edit. Without the guard the span is
    /// empty, and taking nothing still fills the register with it — so `dh`
    /// against the left edge would quietly throw away what was yanked.
    #[test]
    fn a_motion_that_went_nowhere_takes_nothing_and_keeps_the_register() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        buffer.key('y');
        buffer.key('y');
        buffer.key('d');
        buffer.key('h');
        assert_eq!(buffer.shown(), "one\ntwo");
        assert_eq!(buffer.register_text(), Some("one".to_string()));
    }

    /// A charwise operator's span is measured in offsets rather than columns,
    /// which is what lets one cross a line break: the character behind a
    /// cursor sitting at column 1 is the newline above it, and `db` from there
    /// joins the two lines the word it took spanned.
    #[test]
    fn db_at_the_start_of_a_line_takes_the_word_above_and_joins_them() {
        let mut buffer = Buffer::open("one two\nthree", false, 4);
        buffer.go_to_place(crate::Place { line: 2, column: 1 });
        buffer.key('d');
        buffer.key('b');
        assert_eq!(buffer.shown(), "one three");
        assert_eq!((buffer.line, buffer.column), (1, 5));
    }

    /// Insertion is not typing, and a snippet is the text that proves it: every
    /// character in this one pairs, so routing a completion through
    /// [`Buffer::insert`] would leave `println!(("{{}}""))` in the buffer.
    #[test]
    fn a_completions_own_brackets_are_not_paired() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.complete(r#"println!("{}")"#, &[]);
        assert_eq!(buffer.shown(), r#"println!("{}")"#);
    }

    // The edits a server sends back when a trigger character is typed —
    // `features/language_intelligence.feature`, "The server reformats as it is
    // typed".
    /// The whole of story 18: a brace typed at the wrong indentation snaps to
    /// the column the server named, and the cursor stays on it rather than
    /// where the characters it lost used to be.
    #[test]
    fn a_reformat_moves_the_line_to_where_the_server_put_it() {
        let mut buffer = Buffer::open("fn main() {\n        }", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place {
            line: 2,
            column: 10,
        });
        buffer.reformat(&[(
            crate::Place { line: 2, column: 1 },
            crate::Place { line: 2, column: 9 },
            String::new(),
        )]);
        assert_eq!(buffer.shown(), "fn main() {\n}");
        assert_eq!((buffer.line, buffer.column), (2, 2));
    }

    /// The other direction, and the one range shape only it has: a server that
    /// *indents* names a span of nothing and text to put there, so the brace
    /// moves right and the cursor with it. A cursor left where it was would sit
    /// on the indentation rather than after the brace.
    #[test]
    fn a_reformat_that_indents_carries_the_cursor_right() {
        let mut buffer = Buffer::open("fn main() {\n    if x {\n}", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 3, column: 2 });
        buffer.reformat(&[(
            crate::Place { line: 3, column: 1 },
            crate::Place { line: 3, column: 1 },
            "        ".to_string(),
        )]);
        assert_eq!(buffer.shown(), "fn main() {\n    if x {\n        }");
        assert_eq!((buffer.line, buffer.column), (3, 10));
    }

    /// Back to front, and the reason for it: every range in a reply is measured
    /// against the document the server was sent, so applying the first edit
    /// forwards moves the text every later range names. Handed over in the
    /// order a server sends them — ascending — because that is the order that
    /// goes wrong.
    #[test]
    fn edits_are_applied_back_to_front() {
        let mut buffer = Buffer::open("x  =  1", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 1, column: 8 });
        buffer.reformat(&[
            (
                crate::Place { line: 1, column: 2 },
                crate::Place { line: 1, column: 4 },
                " ".to_string(),
            ),
            (
                crate::Place { line: 1, column: 5 },
                crate::Place { line: 1, column: 7 },
                " ".to_string(),
            ),
        ]);
        assert_eq!(buffer.shown(), "x = 1");
        assert_eq!((buffer.line, buffer.column), (1, 6));
    }

    /// A reformat is one thing that happened, so it is one thing to undo. Six
    /// edits taking six presses of `u` is the reader undoing an edit they did
    /// not make one sixth at a time.
    ///
    /// The revision is the mechanical half of the same statement: one change to
    /// the buffer, which is also one `didChange` for the server and one parse
    /// for the renderer.
    #[test]
    fn a_reformat_is_one_undo_step() {
        let mut buffer = Buffer::open("x  =  1", false, 4);
        let before = buffer.revision();
        buffer.reformat(&[
            (
                crate::Place { line: 1, column: 2 },
                crate::Place { line: 1, column: 4 },
                " ".to_string(),
            ),
            (
                crate::Place { line: 1, column: 5 },
                crate::Place { line: 1, column: 7 },
                " ".to_string(),
            ),
        ]);
        assert_eq!(buffer.revision(), before + 1);
        buffer.key('u');
        assert_eq!(buffer.shown(), "x  =  1");
    }

    /// An edit that spans the break between two lines joins them, and the
    /// cursor below it comes up a line with the text — the arithmetic a
    /// reformat of a brace on its own line depends on.
    #[test]
    fn an_edit_across_a_break_joins_the_lines_and_brings_the_cursor_with_them() {
        let mut buffer = Buffer::open("fn f()\n{\n}\nnext", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 3, column: 2 });
        buffer.reformat(&[(
            crate::Place { line: 1, column: 7 },
            crate::Place { line: 2, column: 1 },
            " ".to_string(),
        )]);
        assert_eq!(buffer.shown(), "fn f() {\n}\nnext");
        assert_eq!((buffer.line, buffer.column), (2, 2));
    }

    /// A cursor inside what an edit replaced has nowhere of its own to be, so
    /// it lands at the end of what replaced it rather than at the start of the
    /// line.
    #[test]
    fn a_cursor_inside_a_replaced_span_lands_at_the_end_of_the_replacement() {
        let mut buffer = Buffer::open("foo(  )", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 1, column: 6 });
        buffer.reformat(&[(
            crate::Place { line: 1, column: 5 },
            crate::Place { line: 1, column: 7 },
            String::new(),
        )]);
        assert_eq!(buffer.shown(), "foo()");
        assert_eq!((buffer.line, buffer.column), (1, 5));
    }

    /// A range is a number from a child process, so it is clamped to the text
    /// rather than trusted to be in it: a server that names a line the buffer
    /// does not have must not take the buffer down with it.
    #[test]
    fn a_range_past_the_end_of_the_buffer_is_clamped_to_it() {
        let mut buffer = Buffer::open("abc", false, 4);
        buffer.reformat(&[(
            crate::Place { line: 9, column: 1 },
            crate::Place { line: 9, column: 3 },
            ";".to_string(),
        )]);
        assert_eq!(buffer.shown(), "abc;");
    }

    /// Two edits at one place, which the protocol says appear in the order the
    /// array gave them — the one ordering rule that back-to-front alone does
    /// not satisfy, since both are applied at the same offset.
    #[test]
    fn two_edits_at_one_place_go_in_the_order_they_arrived() {
        let mut buffer = Buffer::open("fn f()", false, 4);
        buffer.reformat(&[
            (
                crate::Place { line: 1, column: 1 },
                crate::Place { line: 1, column: 1 },
                "pub ".to_string(),
            ),
            (
                crate::Place { line: 1, column: 1 },
                crate::Place { line: 1, column: 1 },
                "async ".to_string(),
            ),
        ]);
        assert_eq!(buffer.shown(), "pub async fn f()");
    }

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    // A pty grid has no buffer behind it, so the arithmetic has to work over
    // lines handed in as data.
    #[test]
    fn a_span_over_supplied_lines_takes_the_ends_partially_and_the_middle_whole() {
        let grid = lines("bash-5.3$ ls\nCargo.toml  src\nbash-5.3$");
        assert_eq!(
            span_text(&grid, place(1, 11), place(2, 10)),
            "ls\nCargo.toml"
        );
    }

    #[test]
    fn occurrences_are_every_place_the_word_starts_in_document_order() {
        assert_eq!(
            occurrences(&lines("one two\nthree one"), "one"),
            vec![place(1, 1), place(2, 7)]
        );
    }

    #[test]
    fn an_occurrence_differing_in_case_is_a_different_word() {
        assert_eq!(occurrences(&lines("one One"), "one"), vec![place(1, 1)]);
    }

    #[test]
    fn overlapping_runs_count_once() {
        assert_eq!(occurrences(&lines("aaa"), "aa"), vec![place(1, 1)]);
    }

    #[test]
    fn typing_at_two_places_on_one_line_lands_at_both_of_them() {
        let mut buffer = Buffer::open("one and one", false, 4);
        let landed = buffer.replace_at(&[place(1, 1), place(1, 9)], 3, "ab");
        assert_eq!(buffer.shown(), "ab and ab");
        assert_eq!(landed, vec![place(1, 3), place(1, 10)]);
    }

    #[test]
    fn typing_at_every_place_is_one_undo_step() {
        let mut buffer = Buffer::open("one and one", false, 4);
        buffer.replace_at(&[place(1, 1), place(1, 9)], 3, "x");
        buffer.undo();
        assert_eq!(buffer.shown(), "one and one");
    }

    #[test]
    fn a_place_the_buffer_no_longer_has_is_left_where_it_was() {
        let mut buffer = Buffer::open("one", false, 4);
        let landed = buffer.replace_at(&[place(1, 1), place(9, 1)], 3, "x");
        assert_eq!(buffer.shown(), "x");
        assert_eq!(landed, vec![place(1, 2), place(9, 1)]);
    }

    #[test]
    fn a_span_within_one_line_takes_both_ends() {
        assert_eq!(
            span_text(&lines("one two"), place(1, 5), place(1, 7)),
            "two"
        );
    }

    #[test]
    fn a_span_keeps_a_break_it_covers_even_where_a_line_is_empty() {
        let text = lines("one\n\ntwo");
        assert_eq!(span_text(&text, place(1, 1), place(3, 3)), "one\n\ntwo");
    }

    // The pointer sits past the end of a short line, or past the last line:
    // a drag reaches wherever the mouse went, and the text stops where it stops.
    #[test]
    fn a_span_past_the_end_stops_at_what_is_there() {
        let text = lines("one\ntwo");
        assert_eq!(span_text(&text, place(1, 1), place(2, 40)), "one\ntwo");
        assert_eq!(span_text(&text, place(1, 1), place(9, 4)), "one\ntwo");
    }

    /// The classes are what a double-click picks by, so an operator is a word
    /// of its own and a space is no word at all — the second is what stops a
    /// click between two names from picking one of them.
    #[test]
    fn a_word_span_is_the_run_of_one_class_around_the_column() {
        let line = "run(\"unquoted path\") == 1";
        assert_eq!(word_span(line, 6), Some((6, 13)));
        assert_eq!(word_span(line, 13), Some((6, 13)));
        assert_eq!(word_span(line, 22), Some((22, 23)));
        assert_eq!(word_span(line, 14), None);
        assert_eq!(word_span(line, 40), None);
        assert_eq!(word_span(line, 0), None);
        assert_eq!(word_span("", 1), None);
    }

    /// The Preview's motion arm lays the rendered rows out only for the keys
    /// that read them: a markdown parse per keystroke is what ADR 0007 calls
    /// visible rather than merely wasteful, and a key that names no motion at
    /// all would pay for one and throw it away. That split is only sound while
    /// the word motions are the only keys `moved` looks at its lines for, so
    /// this is what goes red when a later motion starts reading them.
    #[test]
    fn only_the_word_motions_read_the_text_they_move_through() {
        let text = lines("one two three\nfour five six");
        let nothing: [String; 0] = [];
        let at = place(1, 5);
        for key in (' '..='~').filter(|key| !matches!(key, 'w' | 'e' | 'b')) {
            assert_eq!(moved(&nothing, at, key), moved(&text, at, key), "{key:?}");
        }
    }

    #[test]
    fn moving_does_not_bump_the_revision() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        let before = buffer.revision();
        buffer.key('j');
        assert_eq!(buffer.revision(), before, "a motion changes no content");
    }
    // The pairs — `features/editing.feature`, "The editor holds the other end
    // of a pair". Typing an opening character leaves the reader between
    // both halves, which is the whole of the insertion; the four rules below
    // are what stop it being annoying.
    #[test]
    fn an_opening_character_brings_its_partner_and_the_cursor_between() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.key('(');
        assert_eq!(buffer.shown(), "()");
        assert_eq!((buffer.line, buffer.column), (1, 2));
    }

    // Driven from the table rather than from a list of its contents: a copy of
    // the five pairs here could not see the table and the behaviour diverge,
    // and a sixth pair would be added to one and not the other.
    #[test]
    fn every_pair_in_the_table_closes_itself() {
        for (open, close) in PAIRS {
            let mut buffer = Buffer::open("", false, 4);
            buffer.key('i');
            buffer.key(open);
            assert_eq!(buffer.shown(), format!("{open}{close}"), "typing {open:?}");
            assert_eq!(buffer.column, 2, "typing {open:?}");
        }
    }

    // Without this the feature is a net loss: everybody types the closing half
    // out of habit, and every finished call would read `())`.
    #[test]
    fn typing_a_closing_character_over_itself_steps_past_it() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.key('(');
        buffer.key(')');
        assert_eq!(buffer.shown(), "()");
        assert_eq!(buffer.column, 3);
    }

    // The character to the right is the whole of the condition — nothing
    // remembers which halves the editor inserted — so a closing character
    // anywhere else is typed, as it always was.
    #[test]
    fn a_closing_character_where_something_else_sits_is_typed() {
        let mut buffer = Buffer::open("x", false, 4);
        buffer.key('i');
        buffer.key(')');
        assert_eq!(buffer.shown(), ")x");
        assert_eq!(buffer.column, 2);
    }

    /// The ticket's two routes to one delete, held to the same answer where it
    /// says they are the same one. `db` composes with `b` through
    /// `delete_over`, and this does not, so nothing but an assertion stops them
    /// drifting apart on the case both are reached for. Where they part company
    /// — a line's edge, since `b` crosses the break and this is bounded — has
    /// scenarios of its own.
    #[test]
    fn alt_backspace_and_db_take_the_same_word_mid_line() {
        let mut typed = Buffer::open("one two three\n", false, 4);
        typed.go_to_place(place(1, 9));
        typed.delete_word_back();
        let mut operator = Buffer::open("one two three\n", false, 4);
        operator.go_to_place(place(1, 9));
        for key in "db".chars() {
            _ = operator.key(key);
        }
        assert_eq!(typed.shown(), "one three\n");
        assert_eq!(typed.shown(), operator.shown());
    }

    #[test]
    fn backspace_between_an_empty_pairs_halves_takes_both() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.key('(');
        buffer.backspace();
        assert_eq!(buffer.shown(), "");
        assert_eq!(buffer.column, 1);
    }

    // One key undoes one key, and no more than that: a pair holding something
    // is two ordinary characters either side of it.
    #[test]
    fn backspace_on_a_pair_holding_something_takes_one_character() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.key('(');
        buffer.key('x');
        buffer.backspace();
        assert_eq!(buffer.shown(), "()");
    }

    // `don't` and Rust's `'a`: a quote after a word character is an apostrophe
    // or a lifetime, not the start of a string.
    #[test]
    fn a_quote_after_a_word_character_stays_one_quote() {
        let mut buffer = Buffer::open("don", false, 4);
        buffer.key('l');
        buffer.key('l');
        buffer.key('a');
        buffer.key('\'');
        buffer.key('t');
        assert_eq!(buffer.shown(), "don't");
    }

    // The rule is about a word, not about quotes: one opening a string still
    // brings its partner.
    #[test]
    fn a_quote_that_opens_a_string_still_pairs() {
        let mut buffer = Buffer::open("say ", false, 4);
        buffer.key('l');
        buffer.key('l');
        buffer.key('l');
        buffer.key('a');
        buffer.key('"');
        assert_eq!(buffer.shown(), "say \"\"");
    }

    // A bracket after a word is the common case — `foo(` — so the word rule
    // belongs to the quotes alone.
    #[test]
    fn a_bracket_after_a_word_character_still_pairs() {
        let mut buffer = Buffer::open("foo", false, 4);
        buffer.key('l');
        buffer.key('l');
        buffer.key('a');
        buffer.key('(');
        assert_eq!(buffer.shown(), "foo()");
    }

    // A paste is not typing. Deliberately unbalanced: a balanced paste is
    // rescued by the type-over rule above, which is exactly what would hide
    // this defect until somebody pasted half an expression.
    #[test]
    fn a_paste_pairs_nothing() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.paste("foo('bar");
        assert_eq!(buffer.shown(), "foo('bar");
        assert_eq!(buffer.column, 9);
    }

    #[test]
    fn a_pasted_line_break_splits_the_line_and_carries_the_cursor() {
        let mut buffer = Buffer::open("ab", false, 4);
        buffer.key('i');
        buffer.paste("x\ny");
        assert_eq!(buffer.shown(), "x\nyab");
        assert_eq!((buffer.line, buffer.column), (2, 2));
    }

    // One edit, so one undo: replayed as keystrokes a paste took as many
    // presses to undo as it had characters.
    #[test]
    fn a_paste_undoes_in_one_key() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.paste("one two");
        buffer.escape();
        buffer.key('u');
        assert_eq!(buffer.shown(), "");
    }

    /// Every text each press of undo goes back through, from what `keys` typed
    /// into an empty buffer while inserting.
    fn undone(keys: impl FnOnce(&mut Buffer)) -> Vec<String> {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        keys(&mut buffer);
        let mut texts = vec![buffer.shown().to_string()];
        while !buffer.undo.is_empty() {
            buffer.undo();
            texts.push(buffer.shown().to_string());
        }
        texts
    }

    fn type_in(buffer: &mut Buffer, text: &str) {
        text.chars().for_each(|key| _ = buffer.key(key));
    }

    #[test]
    fn redo_puts_back_the_text_and_the_cursor_an_undo_took() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        buffer.key('j');
        buffer.key('x');
        buffer.key('k');
        buffer.key('u');
        buffer.key('U');
        assert_eq!(buffer.shown(), "one\nwo");
        assert_eq!(place(buffer.line, buffer.column), place(1, 1));
    }

    #[test]
    fn an_edit_after_an_undo_leaves_nothing_to_redo() {
        let mut buffer = Buffer::open("one", false, 4);
        buffer.key('x');
        buffer.key('u');
        buffer.key('$');
        buffer.key('x');
        buffer.redo();
        assert_eq!(buffer.shown(), "on");
    }

    #[test]
    fn a_space_after_a_word_starts_an_undo_step() {
        assert_eq!(
            undone(|buffer| type_in(buffer, "hello world")),
            ["hello world", "hello", ""]
        );
    }

    // Monaco's rule: a non-space after one space still belongs to it, and
    // after two it does not.
    #[test]
    fn a_run_of_spaces_is_a_step_of_its_own() {
        assert_eq!(
            undone(|buffer| type_in(buffer, "a  b")),
            ["a  b", "a  ", "a", ""]
        );
    }

    #[test]
    fn switching_between_typing_and_deleting_starts_a_step() {
        let steps = undone(|buffer| {
            type_in(buffer, "abc");
            (0..3).for_each(|_| buffer.backspace());
            type_in(buffer, "x");
        });
        assert_eq!(steps, ["x", "", "abc", ""]);
    }

    #[test]
    fn moving_the_cursor_between_keys_starts_a_step() {
        let steps = undone(|buffer| {
            type_in(buffer, "ab");
            buffer.arrow(crate::Direction::Left);
            type_in(buffer, "x");
        });
        assert_eq!(steps, ["axb", "ab", ""]);
    }

    #[test]
    fn a_click_or_leaving_insert_mode_starts_a_step() {
        let clicked = undone(|buffer| {
            type_in(buffer, "ab");
            buffer.go_to_place(place(1, 1));
            type_in(buffer, "x");
        });
        assert_eq!(clicked, ["xab", "ab", ""]);
        let left = undone(|buffer| {
            type_in(buffer, "ab");
            buffer.escape();
            type_in(buffer, "ac");
        });
        assert_eq!(left, ["abc", "ab", ""]);
    }

    #[test]
    fn typing_after_an_undo_starts_a_step() {
        let steps = undone(|buffer| {
            type_in(buffer, "ab");
            buffer.undo();
            type_in(buffer, "c");
        });
        assert_eq!(steps, ["c", ""]);
    }

    #[test]
    fn enter_is_typing() {
        assert_eq!(
            undone(|buffer| type_in(buffer, "one\ntwo")),
            ["one\ntwo", ""]
        );
    }

    #[test]
    fn a_paste_joins_no_typing_on_either_side() {
        let steps = undone(|buffer| {
            type_in(buffer, "a");
            buffer.paste("b");
            type_in(buffer, "c");
        });
        assert_eq!(steps, ["abc", "ab", "a", ""]);
    }

    // The span is the workspace's selection, so the arithmetic is the
    // buffer's: the closing half goes in first, because inserting the opening
    // one moves the end of a span that lies on the same line.
    #[test]
    fn a_pair_around_a_span_keeps_what_it_covers() {
        let mut buffer = Buffer::open("one two", false, 4);
        buffer.wrap_in(place(1, 1), place(1, 3), '(', ')');
        assert_eq!(buffer.shown(), "(one) two");
    }

    #[test]
    fn a_pair_around_a_span_across_lines_takes_both_ends() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        buffer.wrap_in(place(1, 1), place(2, 3), '"', '"');
        assert_eq!(buffer.shown(), "\"one\ntwo\"");
    }

    // Story 7: normal mode is unchanged. Backspace there is Varde's own
    // backwards delete rather than vim's move, and it took one character
    // before the pairing rules existed — with the cursor on the `)` of `x()y`
    // the paired rule reached it and took two.
    #[test]
    fn backspace_in_normal_mode_takes_one_character() {
        let mut buffer = Buffer::open("x()y", false, 4);
        buffer.key('l');
        buffer.key('l');
        buffer.backspace();
        assert_eq!(buffer.shown(), "x)y");
    }

    // A selection outlives the text it names: a clean buffer follows the file
    // underneath it, so a span can reach past what is now there. Wrapping what
    // exists is the same answer `span_text` and `delete_in` give.
    #[test]
    fn a_pair_around_a_span_past_the_end_wraps_what_is_there() {
        let mut buffer = Buffer::open("one\ntwo", false, 4);
        buffer.wrap_in(place(1, 1), place(9, 40), '(', ')');
        assert_eq!(buffer.shown(), "(one\ntwo)");
    }

    #[test]
    fn a_pair_around_a_span_that_starts_past_the_end_changes_nothing() {
        let mut buffer = Buffer::open("one", false, 4);
        buffer.wrap_in(place(4, 1), place(4, 2), '(', ')');
        assert_eq!(buffer.shown(), "one");
    }

    // Every indented line in a file was indented by hand, and column one threw
    // that away again on the next Enter — the whole of the defect this closes.
    #[test]
    fn enter_carries_the_lines_indentation_down() {
        let mut buffer = Buffer::open("    foo", false, 4);
        buffer.key('$');
        buffer.key('a');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "    foo\n    ");
        assert_eq!((buffer.line, buffer.column), (2, 5));
    }

    #[test]
    fn the_indentation_is_copied_so_tabs_stay_tabs() {
        let mut buffer = Buffer::open("\tfoo", false, 4);
        buffer.key('$');
        buffer.key('a');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "\tfoo\n\t");
        assert_eq!((buffer.line, buffer.column), (2, 2));
    }

    // Four spaces because this buffer holds no indentation to copy — the block
    // just opened is what the next one reads.
    #[test]
    fn enter_between_a_pairs_halves_opens_a_block() {
        let mut buffer = Buffer::open("foo {}", false, 4);
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "foo {\n    \n}");
        assert_eq!((buffer.line, buffer.column), (2, 5));
    }

    // The one number this feature chooses, chosen from the file: a `4` written
    // in here is the answer that is wrong in every JavaScript project opened.
    #[test]
    fn the_indentation_unit_comes_from_the_file() {
        let mut buffer = Buffer::open("if (a) {\n  first()\n}\nfoo {}", false, 4);
        buffer.key('G');
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "if (a) {\n  first()\n}\nfoo {\n  \n}");
    }

    // Even the closing half of a pair: guessing that `if (a)` opens a block
    // needs the language's grammar, and the server owns that.
    #[test]
    fn a_line_ending_in_something_that_is_not_a_pair_gets_no_extra_indentation() {
        let mut buffer = Buffer::open("    if (a)", false, 4);
        buffer.key('$');
        buffer.key('a');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "    if (a)\n    ");
    }

    // Shallowest rather than first: a file whose first indented line is two
    // levels deep would otherwise hand back a double-width unit.
    #[test]
    fn the_indentation_unit_is_the_shallowest_the_file_has() {
        let mut buffer = Buffer::open("call(\n    deep,\n  less,\n)\nfoo {}", false, 4);
        buffer.key('G');
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "call(\n    deep,\n  less,\n)\nfoo {\n  \n}");
    }

    // A line holding nothing but whitespace is not evidence of a level, and
    // real files are full of them — including the ones this very rule leaves
    // behind. One stray space would otherwise make every block in the file one
    // space deep.
    #[test]
    fn a_whitespace_only_line_is_not_the_files_indentation() {
        let mut buffer = Buffer::open(" \nfn a() {\n  x\n}\nfoo {}", false, 4);
        buffer.key('G');
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), " \nfn a() {\n  x\n}\nfoo {\n  \n}");
    }

    #[test]
    fn a_block_indent_moves_every_line_of_the_span() {
        let mut buffer = Buffer::open("one\ntwo\nthree", false, 4);
        buffer.indent_lines(1, 2, true);
        assert_eq!(buffer.shown(), "    one\n    two\nthree");
    }

    // From the file, for the same reason Enter takes it from there: the width
    // in the config is the answer for a file that indents with nothing.
    #[test]
    fn a_block_indent_takes_the_level_from_the_file() {
        let mut buffer = Buffer::open("if (a) {\n  first()\n}", false, 4);
        buffer.indent_lines(2, 2, true);
        assert_eq!(buffer.shown(), "if (a) {\n    first()\n}");
    }

    #[test]
    fn coming_back_out_takes_one_level_off() {
        let mut buffer = Buffer::open("\tone\n\ttwo", false, 4);
        buffer.indent_lines(1, 2, false);
        assert_eq!(buffer.shown(), "one\ntwo");
    }

    // The mixed case, which is every real file that has been edited by two
    // people: the line does not start with the unit, so what leading spaces it
    // has go instead of nothing going at all.
    #[test]
    fn coming_out_of_a_shallower_line_takes_the_spaces_it_has() {
        let mut buffer = Buffer::open("    deep\n  less", false, 4);
        buffer.indent_lines(1, 2, false);
        assert_eq!(buffer.shown(), "  deep\nless");
    }

    // Whitespace nobody typed is whitespace somebody has to delete, and an
    // empty line carries no meaning into the block either way.
    #[test]
    fn an_empty_line_in_the_span_stays_empty() {
        let mut buffer = Buffer::open("one\n\ntwo", false, 4);
        buffer.indent_lines(1, 3, true);
        assert_eq!(buffer.shown(), "    one\n\n    two");
    }

    // One edit, so `u` costs the one key the indent cost however many lines it
    // moved — the same promise a paste and a linewise delete make.
    #[test]
    fn a_block_indent_undoes_in_one_press() {
        let mut buffer = Buffer::open("one\ntwo\nthree", false, 4);
        buffer.indent_lines(1, 3, true);
        buffer.undo();
        assert_eq!(buffer.shown(), "one\ntwo\nthree");
    }

    // A quote is its own partner, so `open == close` is the test that keeps a
    // string out of the block rule: three lines of unterminated quote is not a
    // block, and nothing here names a quote to say so.
    #[test]
    fn enter_between_the_halves_of_a_quote_is_an_ordinary_new_line() {
        let mut buffer = Buffer::open("say \"\"", false, 4);
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "say \"\n\"");
        assert_eq!((buffer.line, buffer.column), (2, 1));
    }

    /// #104: the guides and the bracket pair are asked for on every frame, so
    /// what they need of the whole text — the indent unit, every line's depth,
    /// every pair — is worked out when the text changes and only read after.
    /// A draft swapped in behind the buffer's back, with no revision to say so,
    /// is therefore invisible to both: reading the text again would see it.
    #[test]
    fn the_guides_and_the_pair_are_read_off_the_revision_not_the_text() {
        let mut buffer = Buffer::open("fn a() {\n    b(1);\n}", false, 4);
        buffer.go_to_place(Place { line: 2, column: 7 });
        let (guides, pair) = (buffer.guides(1..=3), buffer.bracket_pair());
        assert!(
            pair.is_some() && guides[1].len() == 1,
            "{pair:?} {guides:?}"
        );

        buffer.draft = Some("x\ny\nz".to_string());
        assert_eq!(
            (buffer.guides(1..=3), buffer.bracket_pair()),
            (guides, pair)
        );
    }
}
