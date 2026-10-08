use crate::highlight;
use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, LinkType, Options, Parser, Tag,
    TagEnd,
};
use std::collections::HashMap;
use std::path::Path;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Heading(HeadingLevel),
    Paragraph,
    Code,
    Diagram,
    Metadata,
    Quote(Option<BlockQuoteKind>),
    List(ListItem),
    Table,
    Rule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Bullet,
    Ordinal(u64),
    Task(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListItem {
    pub depth: usize,
    pub marker: Option<Marker>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Emphasis {
    pub italic: bool,
    pub strong: bool,
    pub struck: bool,
    pub code: bool,
    pub image: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub emphasis: Emphasis,
    pub token: Option<highlight::Kind>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    pub line: usize,
    pub pieces: Vec<Piece>,
    pub refused: Option<DiagramRefusal>,
}

impl Row {
    pub fn text(&self) -> String {
        self.pieces
            .iter()
            .map(|piece| piece.text.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NotMarkdown,
    NoFileOpen,
    ReadOnlyPreview,
    GuestReadOnly,
    ToolAlreadyInstalled,
    BrokenConfig(crate::startup::ConfigError),
    NeedsInstaller(String),
    SessionRunning,
    NoDebugAdapter(String),
    NoLanguageServer(String),
    DebugAdapterFailed,
    DebugAdapterExited,
    LaunchFailed(String),
    NoLastSession,
    NoRunMark,
    SetValueFailed(String),
    LaunchNameTaken(String),
    LaunchFieldNeeded(String),
}

impl Refusal {
    pub fn as_str(&self) -> &'static str {
        match self {
            Refusal::NotMarkdown => "not-a-markdown-file",
            Refusal::NoFileOpen => "no-file-open",
            Refusal::ReadOnlyPreview => "read-only-preview",
            Refusal::GuestReadOnly => "guest-read-only",
            Refusal::ToolAlreadyInstalled => "tool-already-installed",
            Refusal::BrokenConfig(_) => "broken-config",
            Refusal::NeedsInstaller(_) => "needs-installer",
            Refusal::SessionRunning => "debug-session-running",
            Refusal::NoDebugAdapter(_) => "no-debug-adapter",
            Refusal::NoLanguageServer(_) => "no-language-server",
            Refusal::DebugAdapterFailed => "debug-adapter-failed",
            Refusal::DebugAdapterExited => "debug-adapter-exited",
            Refusal::LaunchFailed(_) => "launch-failed",
            Refusal::NoLastSession => "no-last-session",
            Refusal::NoRunMark => "no-run-mark",
            Refusal::SetValueFailed(_) => "set-value-failed",
            Refusal::LaunchNameTaken(_) => "launch-name-taken",
            Refusal::LaunchFieldNeeded(_) => "launch-field-needed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramRefusal {
    UnsupportedDiagram,
    MalformedDiagram,
    TooWide,
}

impl DiagramRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagramRefusal::UnsupportedDiagram => "unsupported-diagram",
            DiagramRefusal::MalformedDiagram => "malformed-diagram",
            DiagramRefusal::TooWide => "diagram-too-wide",
        }
    }
}

impl From<&mermaid_text::Error> for DiagramRefusal {
    fn from(error: &mermaid_text::Error) -> Self {
        match error {
            mermaid_text::Error::UnsupportedDiagram(_) => DiagramRefusal::UnsupportedDiagram,
            mermaid_text::Error::EmptyInput | mermaid_text::Error::ParseError(_) => {
                DiagramRefusal::MalformedDiagram
            }
            mermaid_text::Error::TooWide { .. } => DiagramRefusal::TooWide,
        }
    }
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        ["md", "markdown"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}

const FLAGS: [(Options, bool, &str); 15] = [
    (
        Options::ENABLE_TABLES,
        true,
        "ticket 04: a cell's pieces now have somewhere to land — a table row \
         laid out with columns aligned to the widest cell and each column's \
         declared alignment honoured — rather than the pipes and dashes the \
         parser hands back when this flag is off",
    ),
    (
        Options::ENABLE_FOOTNOTES,
        true,
        "ticket 13: a reference now has somewhere to become `[1]` rather than \
         staying the literal `[^1]`, and a definition's own frame carries the \
         same number so the two read as one construct rather than two \
         unrelated paragraphs",
    ),
    (
        Options::ENABLE_OLD_FOOTNOTES,
        false,
        "the pre-GFM footnote syntax. `ENABLE_FOOTNOTES` already renders the \
         construct; this is a second spelling of the same thing and turning \
         it on too would only risk a document tripping both parsers at once",
    ),
    (
        Options::ENABLE_STRIKETHROUGH,
        true,
        "ticket 12: now that a row carries pieces rather than one string, a \
         struck span has somewhere to put its modifier, so `~~struck~~` \
         renders struck instead of showing its tildes. `pulldown-cmark` \
         strikes a *single* pair of tildes too, so `H~2~O` arrives as the \
         three spans `H`, `2`, `O` and would render `H2O` — the exact example \
         ticket 13 uses to argue a subscript is better left as the author's \
         characters, and why subscript's own flag stays off rather than this \
         one turning back off",
    ),
    (
        Options::ENABLE_TASKLISTS,
        true,
        "ticket 03: a list item's row now carries a marker, so a task item's \
         checked state has somewhere to go — `Event::TaskListMarker` overrides \
         the item's `Marker::Bullet` with `Marker::Task(checked)` instead of \
         leaving `[ ]` and `[x]` as characters",
    ),
    (
        Options::ENABLE_SMART_PUNCTUATION,
        false,
        "it rewrites the author's characters — `--` becomes an em dash, quotes \
         become curly ones. A Preview that changes the text lies about the \
         file, and `/` searching rendered rows would stop finding what the \
         source holds",
    ),
    (
        Options::ENABLE_HEADING_ATTRIBUTES,
        true,
        "ticket 13: `block_kind`'s `Tag::Heading { level, .. }` arm already \
         ignores `id`, `classes` and `attrs` to classify a heading by its \
         level alone, so turning this on consumes `{#id}` into a field \
         nothing reads instead of leaving it as prose at the end of the \
         heading's text — no render code needed, since there is nowhere \
         left for the attribute block to leak into",
    ),
    (
        Options::ENABLE_YAML_STYLE_METADATA_BLOCKS,
        true,
        "ticket 06: a metadata block now has a layout — RowKind::Metadata's \
         own frame in rows() — so the block renders set apart from prose \
         instead of the closing `---` becoming a stray thematic break",
    ),
    (
        Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS,
        true,
        "ticket 13: `Tag::MetadataBlock(_)` already matches either style, so \
         `+++` frontmatter reuses the exact frame `RowKind::Metadata` gives \
         YAML's — no new arm, since the block only cares that it is a \
         metadata block, never which delimiter opened it",
    ),
    (
        Options::ENABLE_MATH,
        false,
        "ticket 13's decision, and it is to leave this off for good: a cell \
         cannot set an equation, so `$x^2$` reads better as itself than as the \
         `x2` that collecting the event's contents would produce",
    ),
    (
        Options::ENABLE_GFM,
        true,
        "ticket 03: `RowKind::Quote` now carries the `BlockQuoteKind` this \
         flag parses, so `> [!NOTE]` renders set apart as the note it \
         announces rather than as a quote whose first line says NOTE — the \
         parser consumes the `[!NOTE]` marker itself once the flag is on",
    ),
    (
        Options::ENABLE_DEFINITION_LIST,
        true,
        "ticket 13: a title and its definition now have somewhere to land — \
         `RowKind::List` frames at depth 0 and 1, the same shape nesting \
         already gives an indented list item — rather than the `term` and \
         `: meaning` the parser hands back as two bare prose lines when this \
         is off",
    ),
    (
        Options::ENABLE_SUPERSCRIPT,
        false,
        "ticket 13's decision, and it is to leave this off for good: a cell \
         cannot raise a glyph, so `x^2^` reads better as itself than as `x2`",
    ),
    (
        Options::ENABLE_SUBSCRIPT,
        false,
        "ticket 13's decision, and it is to leave this off for good, for the \
         same reason as superscript. `ENABLE_STRIKETHROUGH` is already on \
         (ticket 12), and pulldown-cmark strikes a single pair of tildes \
         too, so a whitespace-flanked subscript still loses its markers to \
         strikethrough regardless of this flag — `H~2~O`'s tildes are \
         non-flanking and survive, `log ~2~ n`'s do not. Turning this flag \
         on as well would not recover that case; it would only take the \
         tildes away from strikethrough for constructs where they flank",
    ),
    (
        Options::ENABLE_WIKILINKS,
        true,
        "#152: a `[[Note]]` is followed now (R51.12), so the Preview shows the \
         name or its alias the way Obsidian does and each piece of it carries \
         the Note it names, rather than brackets that lead nowhere",
    ),
];

fn options() -> Options {
    FLAGS
        .iter()
        .filter(|(_, on, _)| *on)
        .fold(Options::empty(), |all, (flag, _, _)| all | *flag)
}

fn block_kind(tag: &Tag) -> Option<RowKind> {
    match tag {
        Tag::Paragraph => Some(RowKind::Paragraph),
        Tag::Heading { level, .. } => Some(RowKind::Heading(*level)),
        Tag::BlockQuote(kind) => Some(RowKind::Quote(*kind)),
        Tag::CodeBlock(_) => Some(RowKind::Code),
        Tag::HtmlBlock => Some(RowKind::Code),
        Tag::List(_) | Tag::Item => Some(RowKind::List(ListItem {
            depth: 0,
            marker: None,
        })),
        Tag::FootnoteDefinition(_) => None,
        Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {
            Some(RowKind::List(ListItem {
                depth: 0,
                marker: None,
            }))
        }
        Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => Some(RowKind::Table),
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Superscript | Tag::Subscript => {
            None
        }
        Tag::Link { .. } | Tag::Image { .. } => None,
        Tag::MetadataBlock(_) => Some(RowKind::Metadata),
    }
}

struct Frame {
    kind: RowKind,
    line: usize,
    closes: TagEnd,
    segments: Vec<Vec<Piece>>,
    emphasis: Vec<Emphasis>,
    own_rows: Vec<Row>,
    language: Option<String>,
}

/// pulldown-cmark nests header cells directly under TableHead with no TableRow
struct TableBuild {
    line: usize,
    alignments: Vec<Alignment>,
    header: Vec<Vec<Piece>>,
    body: Vec<Vec<Vec<Piece>>>,
    current_row: Vec<Vec<Piece>>,
}

struct Build {
    starts: Vec<usize>,
    columns: usize,
    rows: Vec<Row>,
    stack: Vec<Frame>,
    lists: Vec<Option<u64>>,
    definition_lists: Vec<()>,
    quotes: Vec<Option<BlockQuoteKind>>,
    table: Option<TableBuild>,
    footnotes: HashMap<String, usize>,
    footnote_defs: Vec<Option<usize>>,
    note: Option<String>,
}

impl Build {
    fn line(&self, offset: usize) -> usize {
        line_of(&self.starts, offset)
    }

    fn start(&mut self, tag: &Tag, line: usize) {
        if let Tag::Link {
            link_type: LinkType::WikiLink { .. },
            dest_url,
            ..
        } = tag
        {
            self.note = Some(dest_url.to_string());
        }
        self.open_emphasis(tag);
        self.open_depth(tag);
        self.open_frame(tag, line);
        self.open_table(tag, line);
        if let Some(kind) = block_kind(tag) {
            self.open_block(kind, tag, line);
        }
    }

    fn open_emphasis(&mut self, tag: &Tag) {
        match tag {
            Tag::Emphasis => push_emphasis_on_top(&mut self.stack, |top| top.italic = true),
            Tag::Strong => push_emphasis_on_top(&mut self.stack, |top| top.strong = true),
            Tag::Strikethrough => push_emphasis_on_top(&mut self.stack, |top| top.struck = true),
            Tag::Image { .. } => push_emphasis_on_top(&mut self.stack, |top| top.image = true),
            _ => {}
        }
    }

    fn open_depth(&mut self, tag: &Tag) {
        match tag {
            Tag::List(start) => self.lists.push(*start),
            Tag::DefinitionList => self.definition_lists.push(()),
            Tag::BlockQuote(kind) => self.quotes.push(*kind),
            Tag::FootnoteDefinition(label) => {
                let n = footnote_number(label, &mut self.footnotes);
                self.footnote_defs.push(Some(n));
            }
            _ => {}
        }
    }

    fn open_frame(&mut self, tag: &Tag, line: usize) {
        match tag {
            Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {
                self.open_definition(tag, line)
            }
            Tag::Item => self.open_item(line),
            Tag::HtmlBlock => push_frame(
                &mut self.stack,
                RowKind::Code,
                line,
                TagEnd::HtmlBlock,
                self.columns,
            ),
            Tag::MetadataBlock(_) => push_frame(
                &mut self.stack,
                RowKind::Metadata,
                line,
                tag.to_end(),
                self.columns,
            ),
            Tag::CodeBlock(kind) => self.open_code_block(kind, line),
            Tag::TableCell => push_frame(
                &mut self.stack,
                RowKind::Table,
                line,
                TagEnd::TableCell,
                self.columns,
            ),
            _ => {}
        }
    }

    fn open_definition(&mut self, tag: &Tag, line: usize) {
        let depth = 2 * self.definition_lists.len().saturating_sub(1)
            + usize::from(matches!(tag, Tag::DefinitionListDefinition));
        push_frame(
            &mut self.stack,
            RowKind::List(ListItem {
                depth,
                marker: None,
            }),
            line,
            tag.to_end(),
            self.columns,
        );
    }

    /// pulldown-cmark gives a tight item's text no Paragraph wrapper, so the frame opens here
    fn open_item(&mut self, line: usize) {
        let marker = match self.lists.last_mut() {
            Some(Some(ordinal)) => {
                let this_one = *ordinal;
                *ordinal += 1;
                Marker::Ordinal(this_one)
            }
            _ => Marker::Bullet,
        };
        let kind = RowKind::List(ListItem {
            depth: self.lists.len(),
            marker: Some(marker),
        });
        push_frame(&mut self.stack, kind, line, TagEnd::Item, self.columns);
    }

    fn open_code_block(&mut self, kind: &CodeBlockKind, line: usize) {
        let language = match kind {
            CodeBlockKind::Fenced(info) => info.split_whitespace().next().map(str::to_string),
            CodeBlockKind::Indented => None,
        };
        let kind = if language.as_deref() == Some("mermaid") {
            RowKind::Diagram
        } else {
            RowKind::Code
        };
        push_frame(&mut self.stack, kind, line, TagEnd::CodeBlock, self.columns);
        if let Some(frame) = self.stack.last_mut() {
            frame.language = language;
        }
    }

    fn open_table(&mut self, tag: &Tag, line: usize) {
        match tag {
            Tag::Table(alignments) => {
                self.table = Some(TableBuild {
                    line,
                    alignments: alignments.clone(),
                    header: Vec::new(),
                    body: Vec::new(),
                    current_row: Vec::new(),
                });
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(build) = self.table.as_mut() {
                    build.current_row = Vec::new();
                }
            }
            _ => {}
        }
    }

    fn open_block(&mut self, kind: RowKind, tag: &Tag, line: usize) {
        match kind {
            RowKind::Heading(_) => {
                push_frame(&mut self.stack, kind, line, tag.to_end(), self.columns);
            }
            RowKind::Paragraph => self.open_paragraph(line),
            RowKind::Quote(_) | RowKind::List(_) => {}
            RowKind::Code
            | RowKind::Diagram
            | RowKind::Metadata
            | RowKind::Table
            | RowKind::Rule => {}
        }
    }

    fn open_paragraph(&mut self, line: usize) {
        let transparent = matches!(
            self.stack.last().map(|frame| frame.closes),
            Some(TagEnd::Item | TagEnd::DefinitionListTitle | TagEnd::DefinitionListDefinition)
        );
        if transparent {
            if let Some(frame) = self.stack.last_mut() {
                if !frame.own_rows.is_empty() || frame.segments != [Vec::new()] {
                    frame.segments.push(Vec::new());
                }
            }
            return;
        }
        let kind = match self.quotes.last() {
            Some(alert) => RowKind::Quote(*alert),
            None => RowKind::Paragraph,
        };
        push_frame(&mut self.stack, kind, line, TagEnd::Paragraph, self.columns);
        if let Some(n) = self.footnote_defs.last_mut().and_then(Option::take) {
            let frame = self.stack.last_mut().expect("just pushed above");
            push_piece(
                &mut frame.segments,
                &format!("[{n}] "),
                Emphasis::default(),
                None,
            );
        }
    }

    fn end(&mut self, end: TagEnd) {
        match end {
            TagEnd::TableCell => self.close_cell(),
            TagEnd::TableRow => {
                if let Some(build) = self.table.as_mut() {
                    let row = std::mem::take(&mut build.current_row);
                    build.body.push(row);
                }
            }
            TagEnd::TableHead => {
                if let Some(build) = self.table.as_mut() {
                    build.header = std::mem::take(&mut build.current_row);
                }
            }
            TagEnd::Table => self.close_table(),
            TagEnd::Link => {
                self.note = None;
                self.close_frame(end);
            }
            _ => self.close_frame(end),
        }
    }

    fn close_cell(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let pieces = frame.segments.into_iter().next().unwrap_or_default();
        if let Some(build) = self.table.as_mut() {
            build.current_row.push(pieces);
        }
    }

    fn close_table(&mut self) {
        let Some(build) = self.table.take() else {
            return;
        };
        let line = build.line;
        let laid = table_rows(build, self.columns);
        if !self.rows.is_empty() {
            self.rows.push(separator_row(line));
        }
        self.rows.extend(laid);
    }

    fn close_frame(&mut self, end: TagEnd) {
        if matches!(
            end,
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Image
        ) {
            if let Some(frame) = self.stack.last_mut() {
                frame.emphasis.pop();
            }
        }
        match end {
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::DefinitionList => {
                self.definition_lists.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.quotes.pop();
            }
            TagEnd::FootnoteDefinition => {
                self.footnote_defs.pop();
            }
            _ => {}
        }
        if matches!(self.stack.last(), Some(frame) if frame.closes == end) {
            let frame = self.stack.pop().expect("just matched Some above");
            close_frame(&mut self.stack, &mut self.rows, frame, self.columns);
        }
    }

    fn push_text(&mut self, text: &str) {
        if let Some(frame) = self.stack.last_mut() {
            let style = *frame.emphasis.last().unwrap();
            push_piece(&mut frame.segments, text, style, self.note.clone());
        }
    }

    fn push_code(&mut self, text: &str) {
        if let Some(frame) = self.stack.last_mut() {
            let mut style = *frame.emphasis.last().unwrap();
            style.code = true;
            push_piece(&mut frame.segments, text, style, None);
        }
    }

    fn hard_break(&mut self) {
        if let Some(frame) = self.stack.last_mut() {
            frame.segments.push(Vec::new());
        }
    }

    fn task_marker(&mut self, checked: bool) {
        if let Some(frame) = self.stack.last_mut() {
            if let RowKind::List(item) = &mut frame.kind {
                item.marker = Some(Marker::Task(checked));
            }
        }
    }

    fn footnote_reference(&mut self, label: &str) {
        if let Some(frame) = self.stack.last_mut() {
            let n = footnote_number(label, &mut self.footnotes);
            let style = *frame.emphasis.last().unwrap();
            push_piece(&mut frame.segments, &format!("[{n}]"), style, None);
        }
    }

    fn rule(&mut self, line: usize) {
        let follows_list = matches!(self.rows.last().map(|row| row.kind), Some(RowKind::List(_)));
        if !self.rows.is_empty() && !follows_list {
            self.rows.push(separator_row(line));
        }
        self.rows.push(Row {
            kind: RowKind::Rule,
            line,
            pieces: Vec::new(),
            refused: None,
        });
    }
}

pub fn rows(text: &str, columns: usize) -> Vec<Row> {
    let mut build = Build {
        starts: line_starts(text),
        columns,
        rows: Vec::new(),
        stack: Vec::new(),
        lists: Vec::new(),
        definition_lists: Vec::new(),
        quotes: Vec::new(),
        table: None,
        footnotes: HashMap::new(),
        footnote_defs: Vec::new(),
        note: None,
    };

    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
        let line = build.line(range.start);
        match event {
            Event::Start(tag) => build.start(&tag, line),
            Event::End(end) => build.end(end),
            Event::Code(text) | Event::InlineHtml(text) => build.push_code(&text),
            Event::Text(text) | Event::Html(text) => build.push_text(&text),
            Event::SoftBreak => build.push_text(" "),
            Event::HardBreak => build.hard_break(),
            Event::TaskListMarker(checked) => build.task_marker(checked),
            Event::InlineMath(_) | Event::DisplayMath(_) => {}
            Event::FootnoteReference(label) => build.footnote_reference(&label),
            Event::Rule => build.rule(line),
        }
    }
    build.rows
}

fn separator_row(line: usize) -> Row {
    Row {
        kind: RowKind::Paragraph,
        line,
        pieces: Vec::new(),
        refused: None,
    }
}

fn push_frame(stack: &mut Vec<Frame>, kind: RowKind, line: usize, closes: TagEnd, columns: usize) {
    if let Some(top) = stack.last_mut() {
        flush(top, columns);
    }
    stack.push(Frame {
        kind,
        line,
        closes,
        segments: vec![Vec::new()],
        emphasis: vec![Emphasis::default()],
        own_rows: Vec::new(),
        language: None,
    });
}

fn close_frame(stack: &mut [Frame], rows: &mut Vec<Row>, mut frame: Frame, columns: usize) {
    flush(&mut frame, columns);
    if let Some(parent) = stack.last_mut() {
        parent.own_rows.extend(frame.own_rows);
        return;
    }
    let both_list = matches!(rows.last().map(|row| row.kind), Some(RowKind::List(_)))
        && matches!(frame.kind, RowKind::List(_));
    if !rows.is_empty() && !both_list {
        rows.push(separator_row(frame.line));
    }
    rows.extend(frame.own_rows);
}

fn flush(frame: &mut Frame, columns: usize) {
    if frame.segments == [Vec::new()] {
        return;
    }
    let mut laid = match frame.kind {
        RowKind::Code => code_rows(
            RowKind::Code,
            frame.line,
            frame.language.as_deref().unwrap_or(""),
            &frame.segments,
        ),
        RowKind::Diagram => diagram_rows(frame.line, columns, &frame.segments),
        RowKind::Metadata => code_rows(RowKind::Metadata, frame.line, "", &frame.segments),
        _ => laid_out(frame.kind, frame.line, &frame.segments, columns),
    };
    if let RowKind::List(item) = frame.kind {
        let already_marked = !frame.own_rows.is_empty();
        for row in laid.iter_mut().skip(usize::from(!already_marked)) {
            row.kind = RowKind::List(ListItem {
                marker: None,
                ..item
            });
        }
    }
    frame.own_rows.extend(laid);
    frame.segments = vec![Vec::new()];
}

fn push_emphasis_on_top(stack: &mut [Frame], set: impl FnOnce(&mut Emphasis)) {
    if let Some(frame) = stack.last_mut() {
        let mut top = *frame.emphasis.last().unwrap();
        set(&mut top);
        frame.emphasis.push(top);
    }
}

fn push_piece(segments: &mut [Vec<Piece>], text: &str, emphasis: Emphasis, note: Option<String>) {
    if text.is_empty() {
        return;
    }
    let current = segments.last_mut().expect("a block always has a segment");
    if let Some(last) = current.last_mut() {
        if last.emphasis == emphasis && last.note == note {
            last.text.push_str(text);
            return;
        }
    }
    current.push(Piece {
        text: text.to_string(),
        emphasis,
        token: None,
        note,
    });
}

fn code_rows(kind: RowKind, line: usize, language: &str, segments: &[Vec<Piece>]) -> Vec<Row> {
    let raw: String = segments
        .iter()
        .flatten()
        .map(|piece| piece.text.as_str())
        .collect();
    let source = raw.strip_suffix('\n').unwrap_or(&raw);
    highlight::highlight(language, source)
        .into_iter()
        .map(|tokens| Row {
            kind,
            line,
            pieces: tokens
                .into_iter()
                .map(|token| Piece {
                    text: token.text,
                    emphasis: Emphasis::default(),
                    token: Some(token.kind),
                    note: None,
                })
                .collect(),
            refused: None,
        })
        .collect()
}

fn diagram_rows(line: usize, columns: usize, segments: &[Vec<Piece>]) -> Vec<Row> {
    let raw: String = segments
        .iter()
        .flatten()
        .map(|piece| piece.text.as_str())
        .collect();
    let source = raw.strip_suffix('\n').unwrap_or(&raw);
    let width = (columns > 0).then_some(columns);
    match mermaid_text::render_with_width(source, width) {
        Ok(diagram) if fits(width, &diagram) => diagram
            .lines()
            .map(|text| Row {
                kind: RowKind::Diagram,
                line,
                pieces: vec![Piece {
                    text: text.to_string(),
                    emphasis: Emphasis::default(),
                    token: None,
                    note: None,
                }],
                refused: None,
            })
            .collect(),
        Ok(_) => diagram_fallback(line, source, DiagramRefusal::TooWide),
        Err(error) => diagram_fallback(line, source, DiagramRefusal::from(&error)),
    }
}

fn fits(width: Option<usize>, diagram: &str) -> bool {
    width.is_none_or(|w| diagram.lines().all(|l| l.width() <= w))
}

fn diagram_fallback(line: usize, source: &str, reason: DiagramRefusal) -> Vec<Row> {
    let joined = source.lines().collect::<Vec<_>>().join(" ");
    let pieces = highlight::highlight("mermaid", &joined)
        .into_iter()
        .flatten()
        .map(|token| Piece {
            text: token.text,
            emphasis: Emphasis::default(),
            token: Some(token.kind),
            note: None,
        })
        .collect();
    vec![Row {
        kind: RowKind::Code,
        line,
        pieces,
        refused: Some(reason),
    }]
}

fn laid_out(kind: RowKind, line: usize, segments: &[Vec<Piece>], columns: usize) -> Vec<Row> {
    segments
        .iter()
        .flat_map(|pieces| wrapped(pieces, textwrap::Options::new(columns)))
        .map(|pieces| Row {
            kind,
            line,
            pieces,
            refused: None,
        })
        .collect()
}

pub(crate) fn wrapped(pieces: &[Piece], options: textwrap::Options<'_>) -> Vec<Vec<Piece>> {
    if options.width == 0 {
        return vec![pieces.to_vec()];
    }
    let flat: String = pieces.iter().map(|piece| piece.text.as_str()).collect();
    if flat.is_empty() {
        return vec![Vec::new()];
    }
    let mut at = 0;
    let spans: Vec<(usize, usize, &Piece)> = pieces
        .iter()
        .map(|piece| {
            let span = (at, at + piece.text.len(), piece);
            at += piece.text.len();
            span
        })
        .collect();
    let mut rows = Vec::new();
    let mut cursor = 0;
    for line in textwrap::wrap(&flat, options) {
        // textwrap may trim whitespace at a break, so search forward for each line rather than slicing
        let offset = flat[cursor..]
            .find(line.as_ref())
            .expect("a wrapped line is a substring of what was wrapped");
        let start = cursor + offset;
        let end = start + line.len();
        cursor = end;
        rows.push(
            spans
                .iter()
                .filter_map(|(piece_start, piece_end, piece)| {
                    let from = start.max(*piece_start);
                    let to = end.min(*piece_end);
                    (from < to).then(|| Piece {
                        text: piece.text[from - piece_start..to - piece_start].to_string(),
                        emphasis: piece.emphasis,
                        token: piece.token,
                        note: piece.note.clone(),
                    })
                })
                .collect(),
        );
    }
    rows
}

fn table_rows(build: TableBuild, columns: usize) -> Vec<Row> {
    let column_count = build
        .alignments
        .len()
        .max(build.header.len())
        .max(build.body.iter().map(Vec::len).max().unwrap_or(0));
    if column_count == 0 {
        return Vec::new();
    }
    let cell_width =
        |cell: &[Piece]| -> usize { cell.iter().map(|piece| piece.text.width()).sum() };
    let natural: Vec<usize> = (0..column_count)
        .map(|column| {
            let header_width = build.header.get(column).map_or(0, |cell| cell_width(cell));
            let body_width = build
                .body
                .iter()
                .map(|row| row.get(column).map_or(0, |cell| cell_width(cell)))
                .max()
                .unwrap_or(0);
            header_width.max(body_width).max(1)
        })
        .collect();
    const GAP: usize = 2;
    let widths = shrink_to_fit(&natural, columns, GAP);
    let alignment = |column: usize| {
        build
            .alignments
            .get(column)
            .copied()
            .unwrap_or(Alignment::None)
    };
    let mut rows = Vec::new();
    if !build.header.is_empty() {
        for mut pieces in row_pieces(&build.header, &widths, column_count, GAP, alignment) {
            for piece in &mut pieces {
                piece.emphasis.strong = true;
            }
            rows.push(Row {
                kind: RowKind::Table,
                line: build.line,
                pieces,
                refused: None,
            });
        }
    }
    for row in &build.body {
        for pieces in row_pieces(row, &widths, column_count, GAP, alignment) {
            rows.push(Row {
                kind: RowKind::Table,
                line: build.line,
                pieces,
                refused: None,
            });
        }
    }
    rows
}

const MIN_COLUMN: usize = 4;

fn shrink_to_fit(natural: &[usize], columns: usize, gap: usize) -> Vec<usize> {
    let mut widths = natural.to_vec();
    if columns == 0 {
        return widths;
    }
    let total = |widths: &[usize]| -> usize {
        widths.iter().sum::<usize>() + gap * widths.len().saturating_sub(1)
    };
    let floor = |column: usize| natural[column].min(MIN_COLUMN);
    while total(&widths) > columns {
        let Some((index, width)) = widths
            .iter()
            .copied()
            .enumerate()
            .filter(|(column, width)| *width > floor(*column))
            .max_by_key(|(_, width)| *width)
        else {
            break;
        };
        widths[index] = width - 1;
    }
    widths
}

fn row_pieces(
    row: &[Vec<Piece>],
    widths: &[usize],
    column_count: usize,
    gap: usize,
    alignment: impl Fn(usize) -> Alignment,
) -> Vec<Vec<Piece>> {
    let empty = Vec::new();
    let cells: Vec<Vec<Vec<Piece>>> = widths
        .iter()
        .enumerate()
        .take(column_count)
        .map(|(column, width)| {
            let cell = row.get(column).unwrap_or(&empty);
            wrapped(cell, textwrap::Options::new(*width).break_words(false))
        })
        .collect();
    let height = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
    (0..height)
        .map(|display| {
            let mut pieces = Vec::new();
            for (column, cell) in cells.iter().enumerate() {
                let line = cell.get(display).map_or(&empty[..], Vec::as_slice);
                pieces.extend(pad_cell(line, widths[column], alignment(column)));
                if column + 1 < column_count {
                    pieces.push(plain_spaces(gap));
                }
            }
            pieces
        })
        .collect()
}

fn pad_cell(cell: &[Piece], width: usize, alignment: Alignment) -> Vec<Piece> {
    let natural: usize = cell.iter().map(|piece| piece.text.width()).sum();
    let pad = width.saturating_sub(natural);
    let (left, right) = match alignment {
        Alignment::Right => (pad, 0),
        Alignment::Center => (pad / 2, pad - pad / 2),
        Alignment::None | Alignment::Left => (0, pad),
    };
    let mut pieces = Vec::new();
    if left > 0 {
        pieces.push(plain_spaces(left));
    }
    pieces.extend(cell.iter().cloned());
    if right > 0 {
        pieces.push(plain_spaces(right));
    }
    pieces
}

fn plain_spaces(count: usize) -> Piece {
    Piece {
        text: " ".repeat(count),
        emphasis: Emphasis::default(),
        token: None,
        note: None,
    }
}

fn footnote_number(label: &str, seen: &mut HashMap<String, usize>) -> usize {
    let next = seen.len() + 1;
    *seen.entry(label.to_string()).or_insert(next)
}

pub fn note_at(text: &str, line: usize, column: usize) -> Option<String> {
    let start = *line_starts(text).get(line.checked_sub(1)?)?;
    let offset = start
        + text[start..]
            .chars()
            .take(column.checked_sub(1)?)
            .map(char::len_utf8)
            .sum::<usize>();
    Parser::new_ext(text, options())
        .into_offset_iter()
        .find_map(|(event, range)| match event {
            Event::Start(Tag::Link {
                link_type: LinkType::WikiLink { .. },
                dest_url,
                ..
            }) if range.contains(&offset) => Some(dest_url.to_string()),
            _ => None,
        })
}

pub fn prefix(row: &Row) -> String {
    let RowKind::List(item) = row.kind else {
        return String::new();
    };
    let marker = match item.marker {
        Some(Marker::Bullet) => "\u{2022} ".to_string(),
        Some(Marker::Ordinal(ordinal)) => format!("{ordinal}. "),
        Some(Marker::Task(true)) => "\u{2611} ".to_string(),
        Some(Marker::Task(false)) => "\u{2610} ".to_string(),
        None => "  ".to_string(),
    };
    format!("{}{marker}", "  ".repeat(item.depth.saturating_sub(1)))
}

pub fn note_on(row: &Row, column: usize) -> Option<String> {
    let mut left = column
        .checked_sub(1)?
        .checked_sub(prefix(row).chars().count())?;
    row.pieces.iter().find_map(|piece| {
        let width = piece.text.chars().count();
        if left < width {
            return Some(piece.note.clone());
        }
        left -= width;
        None
    })?
}

pub fn wikilinks(text: &str) -> Vec<(usize, usize, usize)> {
    let starts = line_starts(text);
    Parser::new_ext(text, options())
        .into_offset_iter()
        .filter_map(|(event, range)| match event {
            Event::Start(Tag::Link {
                link_type: LinkType::WikiLink { .. },
                ..
            }) => {
                let line = line_of(&starts, range.start);
                let start = starts[line - 1];
                let column = |end: usize| text[start..end].chars().count();
                (line == line_of(&starts, range.end - 1))
                    .then(|| (line, column(range.start) + 1, column(range.end)))
            }
            _ => None,
        })
        .collect()
}

pub fn row_links(row: &Row) -> Vec<(usize, usize)> {
    let mut links: Vec<(usize, usize, &str)> = Vec::new();
    let mut column = prefix(row).chars().count() + 1;
    for piece in &row.pieces {
        let width = piece.text.chars().count();
        if let Some(note) = piece.note.as_deref().filter(|_| width > 0) {
            match links.last_mut() {
                Some(last) if last.1 + 1 == column && last.2 == note => last.1 += width,
                _ => links.push((column, column + width - 1, note)),
            }
        }
        column += width;
    }
    links.into_iter().map(|(from, to, _)| (from, to)).collect()
}

pub fn resolve(note: &str, files: &[String]) -> Result<String, &'static str> {
    let name = note.split(['#', '^']).next().unwrap_or(note).trim();
    let file = match name.ends_with(".md") {
        true => name.to_string(),
        false => format!("{name}.md"),
    };
    let below = format!("/{file}");
    let found: Vec<&String> = files
        .iter()
        .filter(|path| **path == file || path.ends_with(&below))
        .collect();
    match found.as_slice() {
        [one] => Ok(one.to_string()),
        [] => Err("no-such-note"),
        _ => Err("ambiguous-link"),
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
    starts
}

fn line_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|start| *start <= offset)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_wikilink_shows_its_alias_and_carries_the_note_it_names() {
        let rows = rows("See [[Setup|the setup notes]] first.\n", 80);
        assert_eq!(texts(&rows), vec!["See the setup notes first.".to_string()]);
        assert_eq!(note_on(&rows[0], 5).as_deref(), Some("Setup"));
        assert_eq!(note_on(&rows[0], 19).as_deref(), Some("Setup"));
        assert_eq!(note_on(&rows[0], 4), None, "the space before it");
        assert_eq!(note_on(&rows[0], 20), None, "the space after it");
        assert_eq!(note_on(&rows[0], 99), None, "past the end of the row");
    }

    #[test]
    fn a_wikilink_spans_its_brackets_in_source_and_its_shown_text_in_the_preview() {
        let text = "See [[Setup|the setup notes]] and [[Panes]].\n\n- [[a|b *c* d]]\n";
        assert_eq!(wikilinks(text), vec![(1, 5, 29), (1, 35, 43), (3, 3, 15)]);
        let rows = rows(text, 80);
        assert_eq!(row_links(&rows[0]), vec![(5, 19), (25, 29)]);
        assert_eq!(row_links(&rows[2]), vec![(3, 7)], "after the bullet");
        assert_eq!(note_on(&rows[2], 3).as_deref(), Some("a"));
        assert_eq!(note_on(&rows[2], 1), None, "the bullet is not the link");
    }

    #[test]
    fn a_wikilink_in_source_is_found_under_any_character_of_it() {
        let text = "# Title\nSee [[notes/Panes]] and [[Setup|setup]].\n";
        assert_eq!(note_at(text, 2, 5).as_deref(), Some("notes/Panes"));
        assert_eq!(note_at(text, 2, 19).as_deref(), Some("notes/Panes"));
        assert_eq!(note_at(text, 2, 20), None);
        assert_eq!(note_at(text, 2, 29).as_deref(), Some("Setup"));
        assert_eq!(note_at(text, 2, 4), None);
        assert_eq!(note_at(text, 2, 23), None);
        assert_eq!(note_at(text, 1, 3), None);
        assert_eq!(note_at(text, 9, 1), None);
    }

    #[test]
    fn a_wikilink_inside_a_code_fence_is_not_a_link() {
        assert_eq!(note_at("```\n[[Setup]]\n```\n", 2, 3), None);
    }

    #[test]
    fn a_wikilink_resolves_by_name_by_path_and_refuses_to_guess() {
        let files: Vec<String> = [
            "docs/Setup.md",
            "docs/guide/Panes.md",
            "notes/Panes.md",
            "Setup.rs",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(resolve("Setup", &files), Ok("docs/Setup.md".to_string()));
        assert_eq!(resolve("Setup.md", &files), Ok("docs/Setup.md".to_string()));
        assert_eq!(
            resolve("Setup#Install", &files),
            Ok("docs/Setup.md".to_string())
        );
        assert_eq!(
            resolve("notes/Panes", &files),
            Ok("notes/Panes.md".to_string())
        );
        assert_eq!(
            resolve("guide/Panes", &files),
            Ok("docs/guide/Panes.md".to_string())
        );
        assert_eq!(resolve("Panes", &files), Err("ambiguous-link"));
        assert_eq!(resolve("tes/Panes", &files), Err("no-such-note"));
        assert_eq!(resolve("Nowhere", &files), Err("no-such-note"));
    }
    use super::*;

    fn kinds(rows: &[Row]) -> Vec<RowKind> {
        rows.iter().map(|row| row.kind).collect()
    }

    fn texts(rows: &[Row]) -> Vec<String> {
        rows.iter().map(Row::text).collect()
    }

    const CONSTRUCTS: [(&str, &str, RowKind); 29] = [
        ("paragraph", "Prose in a paragraph.\n", RowKind::Paragraph),
        (
            "heading",
            "## Install\n",
            RowKind::Heading(HeadingLevel::H2),
        ),
        ("soft break", "one\ntwo\n", RowKind::Paragraph),
        (
            "heading level",
            "# One\n\n### Three\n",
            RowKind::Heading(HeadingLevel::H1),
        ),
        (
            "emphasis",
            "*em* **strong** ~~struck~~\n",
            RowKind::Paragraph,
        ),
        ("inline code", "Call `main` first.\n", RowKind::Paragraph),
        ("hard break", "one  \ntwo\n", RowKind::Paragraph),
        (
            "list",
            "- one\n- two\n  - nested\n",
            RowKind::List(ListItem {
                depth: 1,
                marker: Some(Marker::Bullet),
            }),
        ),
        (
            "ordered list",
            "1. first\n2. second\n",
            RowKind::List(ListItem {
                depth: 1,
                marker: Some(Marker::Ordinal(1)),
            }),
        ),
        (
            "task item",
            "- [ ] todo\n- [x] done\n",
            RowKind::List(ListItem {
                depth: 1,
                marker: Some(Marker::Task(false)),
            }),
        ),
        ("block quote", "> quoted\n", RowKind::Quote(None)),
        (
            "alert",
            "> [!NOTE]\n> mind this\n",
            RowKind::Quote(Some(BlockQuoteKind::Note)),
        ),
        ("table", "| a | b |\n|---|---|\n| 1 | 2 |\n", RowKind::Table),
        (
            "table alignment",
            "| a | b |\n|:--|--:|\n| 1 | 2 |\n",
            RowKind::Table,
        ),
        ("thematic break", "one\n\n***\n\ntwo\n", RowKind::Rule),
        ("fenced code", "```rust\nfn main() {}\n```\n", RowKind::Code),
        ("indented code", "    let x = 1;\n", RowKind::Code),
        (
            "link",
            "See [Varde](https://example.com).\n",
            RowKind::Paragraph,
        ),
        ("image", "![a cat](cat.png)\n", RowKind::Paragraph),
        (
            "raw html",
            "<div>block</div>\n\nInline <b>bold</b>.\n",
            RowKind::Code,
        ),
        (
            "yaml frontmatter",
            "---\ntitle: Varde\n---\n\nProse.\n",
            RowKind::Metadata,
        ),
        (
            "mermaid fence",
            "```mermaid\ngraph TD\n  A --> B\n```\n",
            RowKind::Diagram,
        ),
        (
            "footnote",
            "Text[^1].\n\n[^1]: a note\n",
            RowKind::Paragraph,
        ),
        (
            "definition list",
            "term\n: meaning\n",
            RowKind::List(ListItem {
                depth: 0,
                marker: None,
            }),
        ),
        ("superscript", "x^2^ and H~2~O\n", RowKind::Paragraph),
        ("math", "$x^2$ and $$y = mx + b$$\n", RowKind::Paragraph),
        (
            "heading attributes",
            "## Install {#install}\n",
            RowKind::Heading(HeadingLevel::H2),
        ),
        (
            "toml frontmatter",
            "+++\ntitle = \"Varde\"\n+++\n\nProse.\n",
            RowKind::Metadata,
        ),
        ("wikilink", "See [[Setup]] for more.\n", RowKind::Paragraph),
    ];

    const MARKERS: [&str; 9] = ["##", "**", "- ", "|", "`", "~~", "> ", "[!", "[ ]"];

    const UNRENDERED: [(&str, &str); 0] = [];

    fn unrendered(sample: &str, kind: RowKind) -> Option<String> {
        let rows = rows(sample, 200);
        if !rows.iter().any(|row| !row.text().trim().is_empty()) {
            return Some("renders no row with text on it".to_string());
        }
        if !rows
            .iter()
            .any(|row| std::mem::discriminant(&row.kind) == std::mem::discriminant(&kind))
        {
            return Some(format!("renders no {kind:?} row"));
        }
        for marker in MARKERS {
            if let Some(row) = rows
                .iter()
                .filter(|row| !matches!(row.kind, RowKind::Code | RowKind::Metadata))
                .find(|row| row.text().contains(marker))
            {
                return Some(format!("leaks {marker:?} in {:?}", row.text()));
            }
        }
        None
    }

    #[test]
    fn every_construct_renders_something_or_is_a_recorded_omission() {
        let missing: Vec<String> = CONSTRUCTS
            .iter()
            .filter(|(name, ..)| !UNRENDERED.iter().any(|(owed, _)| owed == name))
            .filter_map(|(name, sample, kind)| {
                unrendered(sample, *kind).map(|why| format!("{name} {why}"))
            })
            .collect();
        assert!(
            missing.is_empty(),
            "these constructs are in neither the render nor the UNRENDERED \
             list: {missing:#?}"
        );
    }

    #[test]
    fn the_unrendered_list_holds_only_constructs_that_still_do_not_render() {
        for (owed, ticket) in UNRENDERED {
            let (_, sample, kind) = CONSTRUCTS
                .iter()
                .find(|(name, ..)| *name == owed)
                .unwrap_or_else(|| panic!("{owed} is excused but the sweep does not drive it"));
            assert!(
                unrendered(sample, *kind).is_some(),
                "{owed} renders now, so its UNRENDERED entry is stale: {ticket}"
            );
        }
    }

    #[test]
    fn every_parser_flag_has_a_decision() {
        let decided = FLAGS
            .iter()
            .fold(Options::empty(), |all, (flag, _, _)| all | *flag);
        assert_eq!(
            decided,
            Options::all(),
            "undecided: {:?}",
            Options::all() - decided
        );
        for (flag, _, reason) in FLAGS {
            assert!(!reason.is_empty(), "{flag:?} is decided with no reason");
        }
    }

    #[test]
    fn strikethrough_tasklists_gfm_tables_yaml_and_toml_metadata_footnotes_definition_lists_heading_attributes_and_wikilinks_are_the_only_flags_on(
    ) {
        assert_eq!(
            options(),
            Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_GFM
                | Options::ENABLE_TABLES
                | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
                | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
                | Options::ENABLE_FOOTNOTES
                | Options::ENABLE_DEFINITION_LIST
                | Options::ENABLE_HEADING_ATTRIBUTES
                | Options::ENABLE_WIKILINKS
        );
    }

    #[test]
    fn a_heading_loses_its_hashes_and_keeps_its_level() {
        let rows = rows("## Install\n", 40);
        assert_eq!(kinds(&rows), vec![RowKind::Heading(HeadingLevel::H2)]);
        assert_eq!(texts(&rows), vec!["Install".to_string()]);
    }

    #[test]
    fn a_heading_level_is_the_run_of_hashes_that_opened_it() {
        let rows = rows("# One\n\n### Three\n", 40);
        assert_eq!(
            kinds(&rows),
            vec![
                RowKind::Heading(HeadingLevel::H1),
                RowKind::Paragraph,
                RowKind::Heading(HeadingLevel::H3),
            ]
        );
    }

    #[test]
    fn emphasis_strong_and_strikethrough_are_consumed_and_carried() {
        let rows = rows("Run **make** to *build* ~~fast~~.\n", 80);
        assert_eq!(texts(&rows), vec!["Run make to build fast.".to_string()]);
        let pieces = &rows[0].pieces;
        let make = pieces.iter().find(|piece| piece.text == "make").unwrap();
        assert!(make.emphasis.strong, "{pieces:?}");
        let build = pieces.iter().find(|piece| piece.text == "build").unwrap();
        assert!(build.emphasis.italic, "{pieces:?}");
        let fast = pieces.iter().find(|piece| piece.text == "fast").unwrap();
        assert!(fast.emphasis.struck, "{pieces:?}");
    }

    #[test]
    fn a_styled_piece_split_by_wrapping_carries_its_emphasis_into_both_halves() {
        let rows = rows("aaaa **bbbb cccc** dddd\n", 10);
        assert_eq!(
            texts(&rows),
            vec!["aaaa bbbb".to_string(), "cccc dddd".to_string()]
        );
        let bbbb = rows[0]
            .pieces
            .iter()
            .find(|piece| piece.text == "bbbb")
            .unwrap();
        assert!(bbbb.emphasis.strong, "{:?}", rows[0].pieces);
        let cccc = rows[1]
            .pieces
            .iter()
            .find(|piece| piece.text == "cccc")
            .unwrap();
        assert!(cccc.emphasis.strong, "{:?}", rows[1].pieces);
        let dddd = rows[1]
            .pieces
            .iter()
            .find(|piece| piece.text.contains("dddd"))
            .unwrap();
        assert!(!dddd.emphasis.strong, "{:?}", rows[1].pieces);
    }

    #[test]
    fn inline_code_is_a_piece_of_its_own() {
        let rows = rows("Call `main` first.\n", 80);
        assert_eq!(texts(&rows), vec!["Call main first.".to_string()]);
        let main = rows[0]
            .pieces
            .iter()
            .find(|piece| piece.text == "main")
            .unwrap();
        assert!(main.emphasis.code, "{:?}", rows[0].pieces);
    }

    #[test]
    fn every_row_of_a_wrapped_paragraph_carries_the_block_line() {
        let rows = rows("# Setup\n\none two three four five six seven eight\n", 18);
        let paragraph: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Paragraph && !row.text().is_empty())
            .collect();
        assert!(paragraph.len() > 1, "{:?}", texts(&rows));
        assert!(paragraph.iter().all(|row| row.line == 3), "{rows:?}");
    }

    #[test]
    fn a_soft_break_is_a_space_and_not_a_row() {
        let rows = rows("one\ntwo\n", 40);
        assert_eq!(texts(&rows), vec!["one two".to_string()]);
    }

    #[test]
    fn a_hard_break_ends_the_row_and_a_soft_break_still_does_not() {
        let rows = rows("one  \ntwo\n", 40);
        assert_eq!(texts(&rows), vec!["one".to_string(), "two".to_string()]);
        assert!(rows.iter().all(|row| row.kind == RowKind::Paragraph));
        assert!(rows.iter().all(|row| row.line == 1), "{rows:?}");
    }

    #[test]
    fn no_width_yet_means_no_wrapping_yet() {
        let rows = rows("one two three four five six seven eight\n", 0);
        assert_eq!(
            texts(&rows),
            vec!["one two three four five six seven eight".to_string()]
        );
    }

    #[test]
    fn a_block_after_the_first_is_set_apart_from_it() {
        let rows = rows("# Setup\n\nInstall it.\n", 40);
        assert_eq!(
            texts(&rows),
            vec![
                "Setup".to_string(),
                String::new(),
                "Install it.".to_string()
            ]
        );
        assert_eq!(
            rows.iter().map(|row| row.line).collect::<Vec<_>>(),
            vec![1, 3, 3]
        );
        assert_eq!(
            kinds(&rows),
            vec![
                RowKind::Heading(HeadingLevel::H1),
                RowKind::Paragraph,
                RowKind::Paragraph
            ]
        );
    }

    #[test]
    fn markdown_is_the_two_extensions_and_nothing_else() {
        for file in ["README.md", "NOTES.markdown", "README.MD"] {
            assert!(is_markdown(Path::new(file)), "{file}");
        }
        for file in ["guide.mdx", "src/main.rs", "Makefile"] {
            assert!(!is_markdown(Path::new(file)), "{file}");
        }
    }

    #[test]
    fn bulleted_items_are_list_rows_marked_bullet_at_depth_one() {
        let rows = rows("- one\n- two\n", 80);
        assert_eq!(texts(&rows), vec!["one".to_string(), "two".to_string()]);
        for row in &rows {
            assert_eq!(
                row.kind,
                RowKind::List(ListItem {
                    depth: 1,
                    marker: Some(Marker::Bullet)
                }),
                "{rows:?}"
            );
        }
    }

    #[test]
    fn ordered_items_keep_their_ordinal() {
        let rows = rows("3. first\n4. second\n", 80);
        let ordinals: Vec<u64> = rows
            .iter()
            .map(|row| match row.kind {
                RowKind::List(ListItem {
                    marker: Some(Marker::Ordinal(n)),
                    ..
                }) => n,
                other => panic!("expected an ordinal, got {other:?}"),
            })
            .collect();
        assert_eq!(ordinals, vec![3, 4]);
    }

    #[test]
    fn a_nested_list_item_is_indented_deeper_than_its_parent() {
        let rows = rows("- one\n- two\n  - nested\n", 80);
        assert_eq!(
            texts(&rows),
            vec!["one".to_string(), "two".to_string(), "nested".to_string()]
        );
        let depths: Vec<usize> = rows
            .iter()
            .map(|row| match row.kind {
                RowKind::List(ListItem { depth, .. }) => depth,
                other => panic!("expected a list row, got {other:?}"),
            })
            .collect();
        assert_eq!(depths, vec![1, 1, 2]);
    }

    #[test]
    fn task_items_are_ticked_distinguishably_from_unticked() {
        let rows = rows("- [ ] todo\n- [x] done\n", 80);
        assert_eq!(texts(&rows), vec!["todo".to_string(), "done".to_string()]);
        assert_eq!(
            rows[0].kind,
            RowKind::List(ListItem {
                depth: 1,
                marker: Some(Marker::Task(false))
            })
        );
        assert_eq!(
            rows[1].kind,
            RowKind::List(ListItem {
                depth: 1,
                marker: Some(Marker::Task(true))
            })
        );
    }

    #[test]
    fn a_wrapped_list_item_carries_its_marker_only_once() {
        let rows = rows("- aaaa bbbb cccc dddd\n", 10);
        assert!(rows.len() > 1, "{rows:?}");
        assert_eq!(
            rows[0].kind,
            RowKind::List(ListItem {
                depth: 1,
                marker: Some(Marker::Bullet)
            })
        );
        for row in &rows[1..] {
            assert_eq!(
                row.kind,
                RowKind::List(ListItem {
                    depth: 1,
                    marker: None
                }),
                "{rows:?}"
            );
        }
    }

    #[test]
    fn a_block_quote_is_set_apart_from_the_prose_around_it() {
        let rows = rows("Intro.\n\n> quoted\n", 80);
        assert_eq!(
            texts(&rows),
            vec!["Intro.".to_string(), String::new(), "quoted".to_string()]
        );
        assert_eq!(
            kinds(&rows),
            vec![RowKind::Paragraph, RowKind::Paragraph, RowKind::Quote(None)]
        );
    }

    #[test]
    fn every_gfm_alert_kind_is_distinguishable_and_its_marker_is_consumed() {
        let samples = [
            ("> [!NOTE]\n> mind this\n", BlockQuoteKind::Note),
            ("> [!TIP]\n> mind this\n", BlockQuoteKind::Tip),
            ("> [!IMPORTANT]\n> mind this\n", BlockQuoteKind::Important),
            ("> [!WARNING]\n> mind this\n", BlockQuoteKind::Warning),
            ("> [!CAUTION]\n> mind this\n", BlockQuoteKind::Caution),
        ];
        for (sample, kind) in samples {
            let rows = rows(sample, 80);
            assert!(
                rows.iter()
                    .any(|row| row.kind == RowKind::Quote(Some(kind))),
                "{sample:?} -> {rows:?}"
            );
            assert!(
                rows.iter().all(|row| !row.text().contains("[!")),
                "{sample:?} -> {rows:?}"
            );
        }
    }

    #[test]
    fn a_thematic_break_is_a_rule_row_set_apart_from_the_prose_around_it() {
        let rows = rows("one\n\n***\n\ntwo\n", 80);
        assert_eq!(
            texts(&rows),
            vec![
                "one".to_string(),
                String::new(),
                String::new(),
                String::new(),
                "two".to_string(),
            ]
        );
        assert_eq!(
            kinds(&rows),
            vec![
                RowKind::Paragraph,
                RowKind::Paragraph,
                RowKind::Rule,
                RowKind::Paragraph,
                RowKind::Paragraph,
            ]
        );
    }

    #[test]
    fn every_list_and_quote_row_carries_the_source_line_of_its_block() {
        let rows = rows("- one\n- two\n  - nested\n\n> quoted\n", 80);
        let lines: Vec<usize> = rows.iter().map(|row| row.line).collect();
        assert_eq!(texts(&rows).len(), lines.len());
        let one = &rows[texts(&rows).iter().position(|t| t == "one").unwrap()];
        assert_eq!(one.line, 1);
        let two = &rows[texts(&rows).iter().position(|t| t == "two").unwrap()];
        assert_eq!(two.line, 2);
        let nested = &rows[texts(&rows).iter().position(|t| t == "nested").unwrap()];
        assert_eq!(nested.line, 3);
        let quoted = &rows[texts(&rows).iter().position(|t| t == "quoted").unwrap()];
        assert_eq!(quoted.line, 5);
    }

    #[test]
    fn a_fenced_code_block_is_highlighted_in_its_language_and_unwrapped() {
        let rows = rows("```rust\nfn main() {}\n```\n", 5);
        let code: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Code)
            .collect();
        assert_eq!(code.len(), 1, "{rows:?}");
        assert_eq!(code[0].text(), "fn main() {}");
        let keyword = code[0]
            .pieces
            .iter()
            .find(|piece| piece.text == "fn")
            .unwrap_or_else(|| panic!("{:?}", code[0].pieces));
        assert_eq!(keyword.token, Some(highlight::Kind::Keyword));
    }

    #[test]
    fn a_fence_naming_no_language_is_still_one_unwrapped_plain_row() {
        let rows = rows(
            "```\na line that is far longer than the pane is wide\n```\n",
            5,
        );
        let code: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Code)
            .collect();
        assert_eq!(code.len(), 1, "{rows:?}");
        assert!(
            code[0]
                .pieces
                .iter()
                .all(|piece| piece.token == Some(highlight::Kind::Plain)),
            "{:?}",
            code[0].pieces
        );
    }

    #[test]
    fn a_fence_naming_an_unknown_language_renders_plain_rather_than_failing() {
        let rows = rows("```not-a-real-language\nx\n```\n", 40);
        let code: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Code)
            .collect();
        assert_eq!(code.len(), 1, "{rows:?}");
        assert_eq!(code[0].text(), "x");
    }

    #[test]
    fn an_indented_code_block_renders_the_same_way_as_a_fenced_one() {
        let rows = rows("    let x = 1;\n", 40);
        let code: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Code)
            .collect();
        assert_eq!(code.len(), 1, "{rows:?}");
        assert_eq!(code[0].text(), "let x = 1;");
    }

    #[test]
    fn a_multi_line_fence_keeps_one_row_per_source_line_and_no_wider_pane_would_merge_them() {
        let rows = rows("```\nline one\nline two\n```\n", 5);
        let code: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Code)
            .collect();
        assert_eq!(code.len(), 2, "{rows:?}");
        assert_eq!(code[0].text(), "line one");
        assert_eq!(code[1].text(), "line two");
    }

    #[test]
    fn fence_delimiters_contribute_no_row_and_the_code_rows_carry_the_fence_line() {
        let rows = rows("intro\n\n```rust\nfn main() {}\n```\n", 80);
        assert!(
            !rows.iter().any(|row| row.text().contains("```")),
            "{rows:?}"
        );
        let code = rows
            .iter()
            .find(|row| row.kind == RowKind::Code)
            .unwrap_or_else(|| panic!("{rows:?}"));
        assert_eq!(code.line, 3, "{rows:?}");
    }

    #[test]
    fn a_graph_fence_renders_as_a_diagram_holding_its_node_labels() {
        let rows = rows("```mermaid\ngraph LR; A[Build] --> B[Test]\n```\n", 76);
        assert!(!rows.is_empty(), "no rows");
        assert!(
            rows.iter().all(|row| row.kind == RowKind::Diagram),
            "{rows:?}"
        );
        assert!(rows.iter().all(|row| row.line == 1), "{rows:?}");
        let text: String = rows.iter().map(Row::text).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Build"), "{text}");
        assert!(text.contains("Test"), "{text}");
    }

    #[test]
    fn a_diagram_relayouts_when_the_pane_resizes() {
        let source = "```mermaid\ngraph LR; A[Build] --> B[Test] --> C[Deploy]\n```\n";
        let wide = rows(source, 76);
        let narrow = rows(source, 20);
        assert_ne!(texts(&wide), texts(&narrow));
    }

    #[test]
    fn an_unsupported_diagram_type_is_refused_and_shown_as_code() {
        let rows = rows("```mermaid\nC4Context\n  title System\n```\n", 76);
        let code = rows
            .iter()
            .find(|row| row.kind == RowKind::Code)
            .unwrap_or_else(|| panic!("no code row in {rows:?}"));
        assert_eq!(code.refused, Some(DiagramRefusal::UnsupportedDiagram));
        assert_eq!(code.line, 1);
        assert!(code.text().contains("C4Context"), "{:?}", code.text());
    }

    #[test]
    fn a_malformed_diagram_is_refused_and_shown_as_code() {
        let rows = rows("```mermaid\npie\n  bad\n```\n", 76);
        let code = rows
            .iter()
            .find(|row| row.kind == RowKind::Code)
            .unwrap_or_else(|| panic!("no code row in {rows:?}"));
        assert_eq!(code.refused, Some(DiagramRefusal::MalformedDiagram));
        assert!(code.text().contains("bad"), "{:?}", code.text());
    }

    #[test]
    fn a_diagram_wider_than_the_pane_is_refused_and_shown_as_code() {
        let source = "```mermaid\ngraph LR; A[LongLabelHere] --> B[AnotherLongLabel] --> \
                       C[YetAnotherLabel] --> D[MoreLabelText]\n```\n";
        let rows = rows(source, 10);
        let code = rows
            .iter()
            .find(|row| row.kind == RowKind::Code)
            .unwrap_or_else(|| panic!("no code row in {rows:?}"));
        assert_eq!(code.refused, Some(DiagramRefusal::TooWide));
    }

    #[test]
    fn a_refusal_reason_is_the_words_named_here_not_the_crates() {
        assert_eq!(
            DiagramRefusal::UnsupportedDiagram.as_str(),
            "unsupported-diagram"
        );
        assert_eq!(
            DiagramRefusal::MalformedDiagram.as_str(),
            "malformed-diagram"
        );
        assert_eq!(DiagramRefusal::TooWide.as_str(), "diagram-too-wide");
    }

    #[test]
    fn a_table_aligns_columns_to_their_widest_cell() {
        let rows = rows("| a | bb |\n|---|---|\n| ccc | d |\n", 80);
        let table: Vec<String> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect();
        assert_eq!(table, vec!["a    bb".to_string(), "ccc  d ".to_string()]);
    }

    #[test]
    fn each_columns_declared_alignment_is_honoured() {
        let rows = rows("| a | b | c |\n|:--|--:|:-:|\n| 11 | 222 | 3333 |\n", 80);
        let header = rows
            .iter()
            .find(|row| row.kind == RowKind::Table)
            .unwrap_or_else(|| panic!("{rows:?}"));
        assert_eq!(header.text(), "a     b   c  ");
    }

    #[test]
    fn a_tables_header_row_is_bold_and_the_body_is_not() {
        let rows = rows("| a |\n|---|\n| b |\n", 80);
        let table: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .collect();
        assert_eq!(table.len(), 2, "{rows:?}");
        assert!(
            table[0].pieces.iter().all(|piece| piece.emphasis.strong),
            "{:?}",
            table[0].pieces
        );
        assert!(
            !table[1].pieces.iter().any(|piece| piece.emphasis.strong),
            "{:?}",
            table[1].pieces
        );
    }

    #[test]
    fn a_table_wider_than_the_pane_wraps_its_cells_rather_than_dropping_text() {
        let rows = rows("| alpha beta | gamma delta |\n|---|---|\n| c | d |\n", 16);
        let table: String = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect::<Vec<_>>()
            .join("\n");
        for word in ["alpha", "beta", "gamma", "delta", "c", "d"] {
            assert!(table.contains(word), "{word:?} lost in {table:?}");
        }
    }

    #[test]
    fn a_wrapped_source_rows_columns_stay_aligned_down_its_whole_height() {
        let rows = rows("| one two three | x |\n|---|---|\n| a | b |\n", 12);
        let table: Vec<String> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect();
        assert!(table.len() > 2, "nothing wrapped: {table:?}");
        let width = table[0].width();
        assert!(
            table.iter().all(|row| row.width() == width),
            "ragged: {table:?}"
        );
        let column = table[0].find('x').unwrap_or_else(|| panic!("{table:?}"));
        assert_eq!(
            table[1].char_indices().nth(column).map(|(_, ch)| ch),
            Some(' '),
            "second column moved: {table:?}"
        );
    }

    #[test]
    fn a_wrapped_cells_alignment_is_honoured_on_every_display_row() {
        let rows = rows("| h |\n|--:|\n| one two |\n", 5);
        let table: Vec<String> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect();
        assert!(table.len() > 2, "nothing wrapped: {table:?}");
        for row in &table[1..] {
            assert!(row.starts_with(' '), "not right-aligned: {row:?}");
            assert!(!row.ends_with(' '), "not right-aligned: {row:?}");
        }
    }

    #[test]
    fn a_table_that_fits_the_pane_is_one_display_row_per_source_row() {
        let rows = rows("| a | bb |\n|---|---|\n| ccc | d |\n| e | ff |\n", 80);
        let table: Vec<String> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect();
        assert_eq!(
            table,
            vec![
                "a    bb".to_string(),
                "ccc  d ".to_string(),
                "e    ff".to_string()
            ]
        );
    }

    #[test]
    fn a_table_laid_out_against_no_reported_size_wraps_nothing() {
        let rows = rows("| alpha beta | gamma |\n|---|---|\n| c | d |\n", 0);
        let table: Vec<String> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect();
        assert_eq!(
            table,
            vec![
                "alpha beta  gamma".to_string(),
                "c           d    ".to_string()
            ]
        );
    }

    #[test]
    fn an_unbreakable_token_wider_than_its_column_overflows_rather_than_truncating() {
        let rows = rows("| aaaaaaaaaaaaaaaaaaaa |\n|---|\n| b |\n", 6);
        let table: Vec<String> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect();
        assert!(
            table.iter().any(|row| row.contains("aaaaaaaaaaaaaaaaaaaa")),
            "{table:?}"
        );
    }

    #[test]
    fn a_column_stops_shrinking_at_the_narrowest_width_still_worth_wrapping_to() {
        let rows = rows("| ab cd efg | cc |\n|---|---|\n| x | y |\n", 3);
        let table: Vec<String> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect();
        assert_eq!(table[0], "ab    cc", "{table:?}");
        assert!(
            table.iter().all(|row| row.width() == 8),
            "ragged: {table:?}"
        );
    }

    #[test]
    fn a_wrapped_cell_carries_its_emphasis_onto_its_continuation_rows() {
        let rows = rows("| **one two** |\n|---|\n| x |\n", 5);
        let table: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .collect();
        assert!(table.len() > 2, "nothing wrapped: {rows:?}");
        let carried = table[1]
            .pieces
            .iter()
            .find(|piece| piece.text.trim() == "two")
            .unwrap_or_else(|| panic!("{:?}", table[1].pieces));
        assert!(carried.emphasis.strong, "{:?}", table[1].pieces);
    }

    #[test]
    fn column_widths_are_measured_in_display_columns_not_characters() {
        let rows = rows("| 中文 |\n|---|\n| aaaa |\n", 80);
        let table: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .collect();
        assert_eq!(table.len(), 2, "{rows:?}");
        assert_eq!(table[0].text().width(), table[1].text().width(), "{rows:?}");
    }

    #[test]
    fn a_wrapped_wide_glyph_cells_rows_are_padded_back_up_to_its_column_width() {
        let rows = rows("| 中文文 | b |\n|---|---|\n| x | y |\n", 8);
        let table: Vec<String> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .map(Row::text)
            .collect();
        assert!(table.len() > 2, "nothing wrapped: {table:?}");
        let width = table[0].width();
        assert!(
            table.iter().all(|row| row.width() == width),
            "ragged: {table:?}"
        );
    }

    #[test]
    fn a_tables_rows_carry_the_source_line_the_table_started_on() {
        let rows = rows("intro\n\n| a |\n|---|\n| b |\n| c |\n", 80);
        let table: Vec<&Row> = rows
            .iter()
            .filter(|row| row.kind == RowKind::Table)
            .collect();
        assert_eq!(table.len(), 3, "{rows:?}");
        assert!(table.iter().all(|row| row.line == 3), "{rows:?}");
    }

    #[test]
    fn no_row_anywhere_holds_a_list_quote_alert_or_rule_marker() {
        let rows = rows(
            "- [ ] todo\n- [x] done\n  - nested\n\n> [!NOTE]\n> mind this\n\n***\n",
            80,
        );
        for marker in ["- ", "> ", "[!", "[ ]", "[x]"] {
            assert!(
                rows.iter().all(|row| !row.text().contains(marker)),
                "{marker:?} leaked in {rows:?}"
            );
        }
    }

    #[test]
    fn every_link_spelling_hides_its_url() {
        let rows = rows(
            "[inline](https://example.com/inline) and [full][r] and \
             [coll][] and [short].\n\n\
             [r]: https://example.com/ref\n\
             [coll]: https://example.com/coll\n\
             [short]: https://example.com/short\n",
            200,
        );
        assert_eq!(
            texts(&rows),
            vec!["inline and full and coll and short.".to_string()]
        );
    }

    #[test]
    fn an_autolink_shows_its_url_because_that_is_its_own_text() {
        let rows = rows("See <https://example.com/auto> for more.\n", 200);
        let text = texts(&rows).join(" ");
        assert!(text.contains("https://example.com/auto"), "{text}");
    }

    #[test]
    fn an_image_renders_its_alt_text_marked_as_an_image() {
        let rows = rows("![a cat](cat.png)\n", 200);
        assert_eq!(texts(&rows), vec!["a cat".to_string()]);
        let piece = rows[0]
            .pieces
            .iter()
            .find(|piece| piece.text.contains("cat"))
            .unwrap();
        assert!(piece.emphasis.image, "{:?}", rows[0].pieces);
        assert!(!rows[0].text().contains("cat.png"), "{:?}", rows[0]);
    }

    #[test]
    fn inline_html_is_shown_literally_and_set_apart() {
        let rows = rows("Press <kbd>x</kbd> to exit.\n", 200);
        assert_eq!(
            texts(&rows),
            vec!["Press <kbd>x</kbd> to exit.".to_string()]
        );
        let tag = rows[0]
            .pieces
            .iter()
            .find(|piece| piece.text == "<kbd>")
            .unwrap();
        assert!(tag.emphasis.code, "{:?}", rows[0].pieces);
    }

    #[test]
    fn a_raw_html_block_is_shown_literally_and_unwrapped() {
        let rows = rows("<div>\n  <p>hi</p>\n</div>\n", 10);
        assert!(
            kinds(&rows).iter().all(|kind| *kind == RowKind::Code),
            "{rows:?}"
        );
        let text = texts(&rows).join("\n");
        assert!(text.contains("<div>"), "{text}");
        assert!(text.contains("<p>hi</p>"), "{text}");
        assert!(text.contains("</div>"), "{text}");
    }

    #[test]
    fn yaml_frontmatter_renders_as_metadata_rows_set_apart() {
        let rows = rows("---\ntitle: Setup\nauthor: me\n---\n\nProse.\n", 80);
        assert_eq!(kinds(&rows)[0], RowKind::Metadata);
        assert_eq!(*kinds(&rows).last().unwrap(), RowKind::Paragraph);
        let text = texts(&rows).join("\n");
        assert!(text.contains("title: Setup"), "{text}");
        assert!(text.contains("author: me"), "{text}");
        assert!(!text.contains("---"), "{text}");
    }

    #[test]
    fn frontmatter_with_no_closing_marker_does_not_swallow_the_document() {
        let rows = rows("---\ntitle: Setup\n\nStill here.\n", 80);
        assert!(
            !rows.iter().any(|row| row.kind == RowKind::Metadata),
            "{rows:?}"
        );
        let text = texts(&rows).join(" ");
        assert!(text.contains("Still here."), "{text}");
    }

    #[test]
    fn toml_frontmatter_renders_as_metadata_rows_the_same_as_yamls() {
        let rows = rows("+++\ntitle = \"Setup\"\n+++\n\nProse.\n", 80);
        assert_eq!(kinds(&rows)[0], RowKind::Metadata);
        assert_eq!(*kinds(&rows).last().unwrap(), RowKind::Paragraph);
        let text = texts(&rows).join("\n");
        assert!(text.contains("title = \"Setup\""), "{text}");
        assert!(!text.contains("+++"), "{text}");
    }

    #[test]
    fn a_heading_attribute_is_consumed_rather_than_shown_as_prose() {
        let rows = rows("## Install {#install}\n", 80);
        assert_eq!(kinds(&rows), vec![RowKind::Heading(HeadingLevel::H2)]);
        assert_eq!(texts(&rows), vec!["Install".to_string()]);
    }

    #[test]
    fn a_footnote_reference_becomes_its_number_and_the_definition_carries_the_same_one() {
        let rows = rows("Text[^1].\n\n[^1]: a note\n", 80);
        let text = texts(&rows).join("\n");
        assert!(text.contains("Text[1]."), "{text}");
        assert!(text.contains("[1] a note"), "{text}");
        assert!(!text.contains("[^1]"), "{text}");
    }

    #[test]
    fn footnotes_are_numbered_in_reference_order_not_definition_order() {
        let rows = rows("One[^b] and two[^a].\n\n[^a]: second label, used first\n[^b]: first label, used second\n", 80);
        let text = texts(&rows).join("\n");
        assert!(text.contains("One[1]"), "{text}");
        assert!(text.contains("two[2]"), "{text}");
        assert!(text.contains("[1] first label, used second"), "{text}");
        assert!(text.contains("[2] second label, used first"), "{text}");
    }

    #[test]
    fn a_definition_lists_term_and_definition_are_list_rows_the_definition_indented_deeper() {
        let rows = rows("term\n: meaning\n", 80);
        assert_eq!(
            kinds(&rows),
            vec![
                RowKind::List(ListItem {
                    depth: 0,
                    marker: None
                }),
                RowKind::List(ListItem {
                    depth: 1,
                    marker: None
                }),
            ]
        );
        assert_eq!(
            texts(&rows),
            vec!["term".to_string(), "meaning".to_string()]
        );
    }

    #[test]
    fn a_loose_definitions_paragraph_still_indents_under_its_term() {
        let rows = rows(
            "Apple\n\n:   Pomaceous fruit.\n\nOrange\n\n:   Citrus fruit.\n",
            80,
        );
        assert_eq!(
            kinds(&rows),
            vec![
                RowKind::List(ListItem {
                    depth: 0,
                    marker: None
                }),
                RowKind::List(ListItem {
                    depth: 1,
                    marker: None
                }),
                RowKind::List(ListItem {
                    depth: 0,
                    marker: None
                }),
                RowKind::List(ListItem {
                    depth: 1,
                    marker: None
                }),
            ]
        );
        assert_eq!(
            texts(&rows),
            vec![
                "Apple".to_string(),
                "Pomaceous fruit.".to_string(),
                "Orange".to_string(),
                "Citrus fruit.".to_string(),
            ]
        );
    }

    #[test]
    fn a_nested_definition_list_indents_deeper_than_its_outer_term() {
        let rows = rows("outer\n: inner term\n  : inner def\n", 80);
        assert_eq!(
            kinds(&rows),
            vec![
                RowKind::List(ListItem {
                    depth: 0,
                    marker: None
                }),
                RowKind::List(ListItem {
                    depth: 2,
                    marker: None
                }),
                RowKind::List(ListItem {
                    depth: 3,
                    marker: None
                }),
            ]
        );
        assert_eq!(
            texts(&rows),
            vec![
                "outer".to_string(),
                "inner term".to_string(),
                "inner def".to_string(),
            ]
        );
    }

    #[test]
    fn superscript_subscript_and_math_flags_stay_off_so_their_characters_survive() {
        let superscript = rows("x^2^ and H~2~O\n", 80);
        let text = texts(&superscript).join(" ");
        assert!(text.contains("x^2^"), "{text}");
        assert!(text.contains("H~2~O"), "{text}");

        let math = rows("$x^2$ and $$y = mx + b$$\n", 80);
        let text = texts(&math).join(" ");
        assert!(text.contains("$x^2$"), "{text}");
        assert!(text.contains("$$y = mx + b$$"), "{text}");
    }

    #[test]
    fn a_whitespace_flanked_subscript_is_struck_because_strikethrough_claims_it_first() {
        let rows = rows("log ~2~ n is fine\n", 80);
        let text = texts(&rows).join(" ");
        assert!(text.contains("log 2 n is fine"), "{text}");
    }
}
