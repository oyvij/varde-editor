use std::ops::{Bound, RangeBounds};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
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

pub const DEFAULT_TAB_WIDTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    pub disk: String,
    pub draft: Option<String>,
    pub changed_on_disk: bool,
    pub line: usize,
    pub column: usize,
    pub row: usize,
    pub row_column: usize,
    pub previewing: bool,
    pub folded: Vec<usize>,
    pub mode: Mode,
    pending: String,
    revision: u64,
    shape: std::sync::Arc<Shape>,
    count: Option<usize>,
    anchor: usize,
    tab_width: usize,
    register: Vec<String>,
    undo: Vec<(String, crate::Place)>,
    redo: Vec<(String, crate::Place)>,
    step: Step,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Typing,
    Space,
    Spaces,
    Deleting,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tail(pub usize);

/// An empty pty cell (never written, or a wide char's second half) is a space, so columns match the screen
pub fn grid_row<'a>(cells: impl Iterator<Item = Option<&'a str>>) -> String {
    let row: String = cells
        .map(|cell| match cell {
            Some(text) if !text.is_empty() => text,
            _ => " ",
        })
        .collect();
    row.trim_end().to_string()
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Url(String),
    File { path: String, at: crate::Place },
}

pub fn link_at(row: &str, column: usize) -> Option<Link> {
    let chars: Vec<char> = row.chars().collect();
    let at = column.checked_sub(1).filter(|at| *at < chars.len())?;
    let index = row.char_indices().nth(at)?.0;
    let column_of = |byte: usize| row[..byte].chars().count() + 1;
    if let Some(url) = linkify::LinkFinder::new()
        .kinds(&[linkify::LinkKind::Url])
        .links(row)
        .find(|link| link.start() <= index && index < link.end())
    {
        return (url.as_str().starts_with("http://") || url.as_str().starts_with("https://")).then(
            || Link {
                from: column_of(url.start()),
                to: column_of(url.end()) - 1,
                target: Target::Url(url.as_str().to_string()),
            },
        );
    }
    let delimiter = |c: char| c.is_whitespace() || "\"'`()[]{}<>,;|*".contains(c);
    if delimiter(chars[at]) {
        return None;
    }
    let mut from = at;
    while from > 0 && !delimiter(chars[from - 1]) {
        from -= 1;
    }
    let token: String = chars[from..]
        .iter()
        .take_while(|c| !delimiter(**c))
        .collect();
    let path = token
        .split([':', '#'])
        .next()?
        .trim_end_matches(['.', '!', '?']);
    let named = std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().chars().any(char::is_alphabetic));
    if path.ends_with('/') || !(path.contains('/') || named) {
        return None;
    }
    let mut rest = &token[path.len()..];
    let mut taken = path.chars().count();
    let mut numbers = Vec::new();
    while numbers.len() < 2 {
        let Some(after) = rest
            .strip_prefix(':')
            .or_else(|| rest.strip_prefix("#L").filter(|_| numbers.is_empty()))
        else {
            break;
        };
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        let Ok(number) = after[..digits].parse::<usize>() else {
            break;
        };
        numbers.push(number);
        taken += rest.len() - after.len() + digits;
        rest = &after[digits..];
    }
    let to = from + taken;
    (at < to).then(|| Link {
        from: from + 1,
        to,
        target: Target::File {
            path: path.strip_prefix("./").unwrap_or(path).to_string(),
            at: crate::Place {
                line: numbers.first().copied().unwrap_or(1).max(1),
                column: numbers.get(1).copied().unwrap_or(1).max(1),
            },
        },
    })
}

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

pub fn moved(lines: &[String], at: crate::Place, key: char) -> Option<crate::Place> {
    let word = |stop| {
        let text: Vec<char> = lines.join("\n").chars().collect();
        let landed = match stop {
            Word::Start => next_word_start(&text, offset(lines, at)),
            Word::End => word_end(&text, offset(lines, at)),
            Word::Back => previous_word_start(&text, offset(lines, at)),
        };
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
            // Starts at 1: a language server may treat document version 0 as one it has already seen
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

    pub fn follow(&mut self, contents: String) {
        self.disk = contents;
        self.changed_on_disk = self.draft.is_some();
        self.changed();
        self.clamp();
    }

    pub fn reload(&mut self) {
        self.draft = None;
        self.changed_on_disk = false;
        self.changed();
        self.clamp();
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        self.shown().split('\n').map(str::to_string).collect()
    }

    fn set(&mut self, lines: &[String]) {
        self.draft = Some(lines.join("\n"));
        self.changed();
    }

    fn changed(&mut self) {
        self.revision += 1;
        self.shape = std::sync::Arc::new(Shape::of(self.shown(), self.tab_width));
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn line_text(&self, number: usize) -> Option<&str> {
        let starts = &self.shape.starts;
        let from = *starts.get(number.checked_sub(1)?)?;
        let to = starts
            .get(number)
            .map_or(self.shown().len(), |next| next - 1);
        self.shown().get(from..to)
    }

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

    pub(crate) fn indents(&self) -> &[Option<usize>] {
        &self.shape.indents
    }

    pub fn conflicts(&self) -> &[crate::conflict::Conflict] {
        &self.shape.conflicts
    }

    pub fn accept(&mut self, side: crate::conflict::Side) {
        let Some(conflict) = self
            .shape
            .conflicts
            .iter()
            .find(|conflict| (conflict.start..=conflict.end).contains(&self.line))
            .cloned()
        else {
            return;
        };
        let mut lines = self.lines();
        let kept = crate::conflict::kept(&lines, &conflict, side);
        self.remember(Step::Other);
        lines.splice(conflict.start - 1..conflict.end, kept);
        self.line = conflict.start;
        self.column = 1;
        self.set(&lines);
        self.clamp();
    }

    fn remember(&mut self, kind: Step) {
        let (joins, open) = match (self.step, kind) {
            (Step::Typing | Step::Space, Step::Typing) => (true, Step::Typing),
            (Step::Space | Step::Spaces, Step::Space) => (true, Step::Spaces),
            (Step::Deleting, Step::Deleting) => (true, Step::Deleting),
            _ => (false, kind),
        };
        if !joins {
            let at = crate::Place {
                line: self.line,
                column: self.column,
            };
            self.undo.push((self.shown().to_string(), at));
        }
        self.step = open;
        self.redo.clear();
    }

    fn clamp(&mut self) {
        let lines = self.lines();
        self.line = self.line.clamp(1, lines.len().max(1));
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
        let last = if self.mode == Mode::Insert || self.folded.contains(&self.line) {
            width + 1
        } else {
            width.max(1)
        };
        self.column = self.column.clamp(1, last.max(1));
    }

    fn folds(&self) -> impl Iterator<Item = crate::fold::Block> + '_ {
        let blocks = match self.folded.is_empty() {
            true => Vec::new(),
            false => crate::fold::blocks(self.shown()),
        };
        blocks
            .into_iter()
            .filter(|block| self.folded.contains(&block.from))
    }

    fn past_fold(&self, line: usize) -> usize {
        self.folds()
            .find(|block| block.from == line)
            .map_or(line, |block| block.to)
    }

    pub fn fold_dots_at(&self, line: usize, column: usize) -> bool {
        self.folded.contains(&line)
            && column
                > self
                    .lines()
                    .get(line.saturating_sub(1))
                    .map_or(0, |text| text.chars().count())
    }

    pub fn on_fold_dots(&self) -> bool {
        self.mode != Mode::Insert && self.fold_dots_at(self.line, self.column)
    }

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

    pub fn backspace(&mut self) {
        let (left, right) = self.either_side();
        let mut lines = self.lines();
        if self.column > 1 {
            self.remember(Step::Deleting);
            let line = &mut lines[self.line - 1];
            let at = byte_index(line, self.column - 2);
            line.remove(at);
            self.column -= 1;
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

    pub fn pending(&self) -> &str {
        &self.pending
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    pub fn word_at_cursor(&self) -> Option<String> {
        let line: Vec<char> = self.line_text(self.line)?.chars().collect();
        let (from, to) = self.word_span(self.line, self.column.min(line.len()))?;
        Some(line[from - 1..to].iter().collect())
    }

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

    pub fn complete(&mut self, text: &str, stops: &[usize]) -> Vec<Tail> {
        self.remember(Step::Other);
        let start = self.word_start(self.line, self.column);
        let mut lines = self.lines();
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
        if let Some(place) = tails.first().and_then(|first| self.place_before(*first)) {
            self.go_to_place(place);
        }
        tails.into_iter().skip(1).collect()
    }

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

    pub fn pending_command(&self) -> String {
        let mut shown = String::new();
        if let Some(count) = self.count {
            shown.push_str(&count.to_string());
        }
        shown.push_str(&self.pending);
        shown
    }

    pub fn selected_lines(&self) -> Option<(usize, usize)> {
        (self.mode == Mode::Visual)
            .then(|| (self.anchor.min(self.line), self.anchor.max(self.line)))
    }

    pub fn text_in(&self, from: crate::Place, to: crate::Place) -> String {
        span_text(&self.lines(), from, to)
    }

    pub fn lines_in(&self, from: usize, to: usize) -> String {
        self.lines()
            .into_iter()
            .skip(from - 1)
            .take(to + 1 - from)
            .map(|line| format!("{line}\n"))
            .collect()
    }

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

    pub fn replace_in(&mut self, from: crate::Place, to: crate::Place, key: char) {
        self.delete_in(from, to);
        let remembered = self.undo.len();
        self.insert(key);
        self.undo.truncate(remembered);
        self.clamp();
    }

    fn insert(&mut self, key: char) {
        let (left, right) = self.either_side();
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
        let word = left.is_some_and(char::is_alphanumeric);
        if let Some(close) = closes(key).filter(|close| !(word && *close == key)) {
            line.insert(byte_index(line, self.column - 1), close);
        }
        self.set(&lines);
    }

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

    pub fn reformat(&mut self, edits: &[(crate::Place, crate::Place, String)]) {
        let lines = self.lines();
        // Reversed before the stable sort so same-place edits keep the LSP array order when applied back to front
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
        if let Some(place) = self.place_before(Tail(text.chars().count().saturating_sub(at))) {
            self.go_to_place(place);
        }
    }

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

        if matches!(key, 'g' | 'd' | 'y' | 'c') {
            self.pending.push(key);
            return None;
        }

        let times = self.count.take().unwrap_or(1);
        for _ in 0..times {
            self.motion(key);
        }
        None
    }

    fn operator(&mut self, chord: &str, key: char) -> Option<String> {
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
            ("dg", 'g') => self.delete_lines(1, self.line),
            ("c", 'c') => self.accept(crate::conflict::Side::Current),
            ("c", 'i') => self.accept(crate::conflict::Side::Incoming),
            ("c", 'b') => self.accept(crate::conflict::Side::Both),
            ("d", key) => return self.delete_over(key, times),
            _ => return Some(format!("{chord}{key}")),
        }
        None
    }

    fn delete_over(&mut self, key: char, times: usize) -> Option<String> {
        let mut probe = self.clone();
        if !probe.moved(key) {
            return Some(format!("d{key}"));
        }
        for _ in 1..times {
            probe.moved(key);
        }
        probe.clamp();
        if matches!(key, 'j' | 'k' | 'G') {
            self.delete_lines(self.line.min(probe.line), self.line.max(probe.line));
            return None;
        }
        let (here, there) = (self.offset(), probe.offset());
        let mut to = here.max(there);
        if matches!(key, '$' | 'e') && self.shown().chars().nth(to) != Some('\n') {
            to += 1;
        }
        self.take_chars(here.min(there), to);
        None
    }

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

    fn motion(&mut self, key: char) {
        self.moved(key);
        self.edited(key);
    }

    fn moved(&mut self, key: char) -> bool {
        let at = crate::Place {
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

    fn delete_lines(&mut self, from: usize, to: usize) {
        self.yank(from, to);
        let mut lines = self.lines();
        let to = to.min(lines.len());
        if from > to {
            return;
        }
        self.remember(Step::Other);
        lines.drain(from - 1..to);
        if lines.is_empty() {
            lines.push(String::new());
        }
        self.line = from;
        self.set(&lines);
    }

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
        self.line = to;
        self.column = usize::MAX;
        self.clamp();
    }

    fn take_chars(&mut self, from: usize, to: usize) {
        let text = self.shown().to_string();
        let taken: String = text.chars().skip(from).take(to - from).collect();
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

    fn offset(&self) -> usize {
        let lines = self.lines();
        lines[..self.line - 1]
            .iter()
            .map(|line| line.chars().count() + 1)
            .sum::<usize>()
            + self.column
            - 1
    }

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
        let Some(place) = moved(&self.lines(), at, key) else {
            return;
        };
        if matches!(stop, Word::Start) && place.line > at.line {
            self.go_to_place(crate::Place {
                column: usize::MAX,
                ..at
            });
            if self.column != at.column {
                return;
            }
        }
        self.go_to_place(place);
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

    pub fn undo(&mut self) {
        if let Some((previous, back)) = self.undo.pop() {
            let at = crate::Place {
                line: self.line,
                column: self.column,
            };
            self.redo.push((self.shown().to_string(), at));
            self.draft = (previous != self.disk).then_some(previous);
            self.changed();
            self.go_to_place(back);
        }
        self.step = Step::Other;
    }

    pub fn redo(&mut self) {
        if let Some((next, at)) = self.redo.pop() {
            let back = crate::Place {
                line: self.line,
                column: self.column,
            };
            self.undo.push((self.shown().to_string(), back));
            self.draft = (next != self.disk).then_some(next);
            self.changed();
            self.go_to_place(at);
        }
    }

    pub fn bracket_pair(&self) -> Option<(crate::Place, crate::Place)> {
        let at = (self.line, self.column);
        let brackets = &self.shape.brackets;
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

    pub fn write(&mut self) -> String {
        let contents = self.shown().to_string();
        self.disk = contents.clone();
        self.draft = None;
        self.changed_on_disk = false;
        self.revision += 1;
        contents
    }
}

pub fn merge_prompt(path: &str, disk: &str, buffer: &str) -> String {
    format!(
        "{path} changed on disk while it had unsaved edits in Varde.\n\
         Merge the two and write the result to {path}.\n\n\
         --- on disk ---\n{disk}\n\
         --- unsaved in Varde ---\n{buffer}"
    )
}

pub enum Word {
    Start,
    End,
    Back,
}

impl Word {
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

const PAIRS: [(char, char); 5] = [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')];

fn shuts(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Shape {
    starts: Vec<usize>,
    indents: Vec<Option<usize>>,
    unit: String,
    brackets: Vec<(crate::Place, Option<crate::Place>, Option<usize>)>,
    conflicts: Vec<crate::conflict::Conflict>,
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
            conflicts: crate::conflict::find(lines.iter().copied()),
        }
    }
}

pub fn closes(open: char) -> Option<char> {
    PAIRS
        .iter()
        .find(|(candidate, _)| *candidate == open)
        .map(|(_, close)| *close)
}

fn indent_unit(lines: &[&str], fallback: usize) -> String {
    lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| &line[..line.len() - line.trim_start().len()])
        .filter(|indent| !indent.is_empty())
        .min_by_key(|indent| indent.len())
        .map(str::to_string)
        .unwrap_or_else(|| " ".repeat(fallback))
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guide {
    pub column: usize,
    pub active: bool,
}

fn indent_at(indents: &[Option<usize>], index: usize) -> Option<usize> {
    Some(indents.get(index)?.unwrap_or_else(|| {
        let above = indents[..index].iter().rev().flatten().next();
        let below = indents[index + 1..].iter().flatten().next();
        (*above.unwrap_or(&0)).min(*below.unwrap_or(&0))
    }))
}

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
    use super::{
        grid_row, link_at, moved, occurrences, span_text, word_span, Buffer, Link, Target, PAIRS,
    };
    use crate::Place;

    #[test]
    fn a_link_is_the_url_under_the_column_and_nothing_else() {
        let row = "Sé (https://example.com/a_(b)). Not file:///etc/passwd or ftp://x.y";
        assert_eq!(
            link_at(row, 6),
            Some(Link {
                from: 5,
                to: 29,
                target: Target::Url("https://example.com/a_(b)".to_string()),
            })
        );
        assert_eq!(link_at(row, 1), None, "a word before the link");
        assert_eq!(link_at(row, 31), None, "the sentence's own punctuation");
        assert_eq!(link_at(row, 40), None, "a scheme the opener would launch");
        assert_eq!(link_at(row, 60), None, "ftp");
        assert_eq!(link_at(row, 200), None, "past the end of the row");
        assert_eq!(link_at(row, 0), None);
    }

    #[test]
    fn a_path_is_a_link_to_the_line_and_column_it_names() {
        let file = |path: &str, line, column| {
            Some(Target::File {
                path: path.to_string(),
                at: Place { line, column },
            })
        };
        let target = |row: &str, column| link_at(row, column).map(|link| link.target);
        assert_eq!(
            target("  --> src/lib.rs:42:7", 9),
            file("src/lib.rs", 42, 7)
        );
        assert_eq!(
            target("in src/mouse.rs:12.", 5),
            file("src/mouse.rs", 12, 1)
        );
        assert_eq!(target("see ./Cargo.toml", 7), file("Cargo.toml", 1, 1));
        assert_eq!(target("open src/lib.rs.", 7), file("src/lib.rs", 1, 1));
        assert_eq!(target("src/a.rs:0:0", 1), file("src/a.rs", 1, 1));
        assert_eq!(target("see **src/a.rs:4**", 8), file("src/a.rs", 4, 1));
        assert_eq!(target("src/a.rs#L8", 2), file("src/a.rs", 8, 1));
        assert_eq!(target("lines src/a.rs:3-9", 9), file("src/a.rs", 3, 1));
        assert_eq!(target("/w/b.rs:5:fn main", 1), file("/w/b.rs", 5, 1));
        assert_eq!(target("(at /w/src/c.rs:5)", 10), file("/w/src/c.rs", 5, 1));
        assert_eq!(target("$ cargo test", 3), None, "a word is not a path");
        assert_eq!(target("version 1.2.3", 10), None, "a number is not a file");
        assert_eq!(
            target("into src/", 7),
            None,
            "a folder does not open in the editor"
        );
        assert_eq!(
            target("/w/b.rs:5:fn main", 11),
            None,
            "past what the path names"
        );
        assert_eq!(
            link_at("  --> src/lib.rs:42:7, here", 8).map(|link| (link.from, link.to)),
            Some((7, 21)),
            "the span is the path and its place, and not the comma after it"
        );
    }

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

    #[test]
    fn a_blank_grid_cell_keeps_its_column() {
        let cells = [Some("a"), Some(""), None, Some("b")];
        assert_eq!(grid_row(cells.into_iter()), "a  b");
    }

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

    #[test]
    fn a_span_over_a_padded_grid_row_picks_by_screen_column() {
        let row = grid_row([Some(""), Some(""), Some("h"), Some("i")].into_iter());
        assert_eq!(span_text(&[row], place(1, 3), place(1, 4)), "hi");
    }

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

    #[test]
    fn cc_ci_cb_accept_a_side_as_one_undo_step() {
        let text = "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> x\nz";
        for (key, left) in [
            ('c', "a\nours\nz"),
            ('i', "a\ntheirs\nz"),
            ('b', "a\nours\ntheirs\nz"),
        ] {
            let mut buffer = Buffer::open(text, false, 4);
            buffer.key('j');
            buffer.key('j');
            buffer.key('c');
            buffer.key(key);
            assert_eq!(buffer.shown(), left, "c{key}");
            assert!(buffer.conflicts().is_empty());
            buffer.undo();
            assert_eq!(buffer.shown(), text, "one undo puts c{key} back");
        }
    }

    #[test]
    fn cc_outside_a_conflict_changes_nothing() {
        let mut buffer = Buffer::open(
            "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> x",
            false,
            4,
        );
        let before = buffer.revision();
        buffer.key('c');
        assert_eq!(buffer.key('c'), None);
        assert_eq!(buffer.revision(), before);
        assert_eq!(buffer.pending(), "");
    }

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

    #[test]
    fn completing_replaces_the_run_before_the_cursor_and_nothing_after_it() {
        let mut buffer = Buffer::open("let wor = other;", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 1, column: 8 });
        buffer.complete("workspace_root", &[]);
        assert_eq!(buffer.shown(), "let workspace_root = other;");
        assert_eq!(buffer.column, 19);
    }

    #[test]
    fn completing_with_nothing_typed_inserts_the_whole_of_it() {
        let mut buffer = Buffer::open("let x = ", false, 4);
        buffer.key('i');
        buffer.go_to_place(crate::Place { line: 1, column: 9 });
        buffer.complete("other", &[]);
        assert_eq!(buffer.shown(), "let x = other");
    }

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

    #[test]
    fn dg_from_the_first_line_empties_the_buffer() {
        let mut buffer = Buffer::open("one\ntwo\nthree", false, 4);
        buffer.key('d');
        buffer.key('G');
        assert_eq!(buffer.shown(), "");
        buffer.key('p');
        assert_eq!(buffer.shown(), "\none\ntwo\nthree");
    }

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

    #[test]
    fn d_to_the_line_end_of_an_empty_line_takes_nothing() {
        let mut buffer = Buffer::open("\ntwo", false, 4);
        buffer.key('d');
        buffer.key('$');
        assert_eq!(buffer.shown(), "\ntwo");
    }

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

    #[test]
    fn db_at_the_start_of_a_line_takes_the_word_above_and_joins_them() {
        let mut buffer = Buffer::open("one two\nthree", false, 4);
        buffer.go_to_place(crate::Place { line: 2, column: 1 });
        buffer.key('d');
        buffer.key('b');
        assert_eq!(buffer.shown(), "one three");
        assert_eq!((buffer.line, buffer.column), (1, 5));
    }

    #[test]
    fn a_completions_own_brackets_are_not_paired() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.complete(r#"println!("{}")"#, &[]);
        assert_eq!(buffer.shown(), r#"println!("{}")"#);
    }

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

    #[test]
    fn a_span_past_the_end_stops_at_what_is_there() {
        let text = lines("one\ntwo");
        assert_eq!(span_text(&text, place(1, 1), place(2, 40)), "one\ntwo");
        assert_eq!(span_text(&text, place(1, 1), place(9, 4)), "one\ntwo");
    }

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
    #[test]
    fn an_opening_character_brings_its_partner_and_the_cursor_between() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.key('(');
        assert_eq!(buffer.shown(), "()");
        assert_eq!((buffer.line, buffer.column), (1, 2));
    }

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

    #[test]
    fn typing_a_closing_character_over_itself_steps_past_it() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.key('(');
        buffer.key(')');
        assert_eq!(buffer.shown(), "()");
        assert_eq!(buffer.column, 3);
    }

    #[test]
    fn a_closing_character_where_something_else_sits_is_typed() {
        let mut buffer = Buffer::open("x", false, 4);
        buffer.key('i');
        buffer.key(')');
        assert_eq!(buffer.shown(), ")x");
        assert_eq!(buffer.column, 2);
    }

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

    #[test]
    fn backspace_on_a_pair_holding_something_takes_one_character() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.key('(');
        buffer.key('x');
        buffer.backspace();
        assert_eq!(buffer.shown(), "()");
    }

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

    #[test]
    fn a_bracket_after_a_word_character_still_pairs() {
        let mut buffer = Buffer::open("foo", false, 4);
        buffer.key('l');
        buffer.key('l');
        buffer.key('a');
        buffer.key('(');
        assert_eq!(buffer.shown(), "foo()");
    }

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

    #[test]
    fn a_paste_undoes_in_one_key() {
        let mut buffer = Buffer::open("", false, 4);
        buffer.key('i');
        buffer.paste("one two");
        buffer.escape();
        buffer.key('u');
        assert_eq!(buffer.shown(), "");
    }

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

    #[test]
    fn backspace_in_normal_mode_takes_one_character() {
        let mut buffer = Buffer::open("x()y", false, 4);
        buffer.key('l');
        buffer.key('l');
        buffer.backspace();
        assert_eq!(buffer.shown(), "x)y");
    }

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

    #[test]
    fn enter_between_a_pairs_halves_opens_a_block() {
        let mut buffer = Buffer::open("foo {}", false, 4);
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "foo {\n    \n}");
        assert_eq!((buffer.line, buffer.column), (2, 5));
    }

    #[test]
    fn the_indentation_unit_comes_from_the_file() {
        let mut buffer = Buffer::open("if (a) {\n  first()\n}\nfoo {}", false, 4);
        buffer.key('G');
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "if (a) {\n  first()\n}\nfoo {\n  \n}");
    }

    #[test]
    fn a_line_ending_in_something_that_is_not_a_pair_gets_no_extra_indentation() {
        let mut buffer = Buffer::open("    if (a)", false, 4);
        buffer.key('$');
        buffer.key('a');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "    if (a)\n    ");
    }

    #[test]
    fn the_indentation_unit_is_the_shallowest_the_file_has() {
        let mut buffer = Buffer::open("call(\n    deep,\n  less,\n)\nfoo {}", false, 4);
        buffer.key('G');
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "call(\n    deep,\n  less,\n)\nfoo {\n  \n}");
    }

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

    #[test]
    fn coming_out_of_a_shallower_line_takes_the_spaces_it_has() {
        let mut buffer = Buffer::open("    deep\n  less", false, 4);
        buffer.indent_lines(1, 2, false);
        assert_eq!(buffer.shown(), "  deep\nless");
    }

    #[test]
    fn an_empty_line_in_the_span_stays_empty() {
        let mut buffer = Buffer::open("one\n\ntwo", false, 4);
        buffer.indent_lines(1, 3, true);
        assert_eq!(buffer.shown(), "    one\n\n    two");
    }

    #[test]
    fn a_block_indent_undoes_in_one_press() {
        let mut buffer = Buffer::open("one\ntwo\nthree", false, 4);
        buffer.indent_lines(1, 3, true);
        buffer.undo();
        assert_eq!(buffer.shown(), "one\ntwo\nthree");
    }

    #[test]
    fn enter_between_the_halves_of_a_quote_is_an_ordinary_new_line() {
        let mut buffer = Buffer::open("say \"\"", false, 4);
        buffer.key('$');
        buffer.key('i');
        buffer.key('\n');
        assert_eq!(buffer.shown(), "say \"\n\"");
        assert_eq!((buffer.line, buffer.column), (2, 1));
    }

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
