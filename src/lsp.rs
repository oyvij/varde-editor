use crate::layout::{Area, Layout};
use crate::search::{Hit, Results};
use crate::startup::Server;
use crate::{preview, Effect, Place, Pointed, Search, State};
use lsp_types::{
    ClientCapabilities, CompletionClientCapabilities, CompletionItemCapability, CompletionResponse,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentFormattingParams, DocumentOnTypeFormattingOptions,
    DocumentOnTypeFormattingParams, FormattingOptions, GotoDefinitionResponse,
    Hover as ServerHover, HoverClientCapabilities, HoverContents, InitializeParams,
    InitializedParams, InsertTextFormat, MarkedString, MarkupKind, Position,
    PublishDiagnosticsClientCapabilities, PublishDiagnosticsParams, Range,
    TextDocumentClientCapabilities, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams, TextEdit, Url, VersionedTextDocumentIdentifier,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const INITIALIZE: i64 = 1;

const DEBOUNCE_MS: u64 = 120;

pub const DWELL_MS: u64 = 500;

fn formatting(state: &State) -> FormattingOptions {
    FormattingOptions {
        tab_size: state.tab_width as u32,
        insert_spaces: true,
        ..FormattingOptions::default()
    }
}

pub fn language<'a>(state: &'a State, path: &Path) -> Option<&'a str> {
    let extension = path.extension()?.to_str()?;
    state
        .servers
        .iter()
        .find(|(_, server)| server.extensions.iter().any(|claim| claim == extension))
        .map(|(language, _)| language.as_str())
}

fn language_id<'a>(state: &'a State, path: &Path) -> Option<&'a str> {
    let language = language(state, path)?;
    let extension = path.extension()?.to_str()?;
    Some(
        state.servers[language]
            .language_ids
            .get(extension)
            .map_or(language, String::as_str),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handshake {
    Sent,
    Ready,
    Gone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub handshake: Handshake,
    sent: BTreeMap<PathBuf, u64>,
    capabilities: Value,
    reading: BTreeSet<PathBuf>,
    next_id: i64,
    /// @vue/language-server answers [] for the whole file once its projectInfo question is refused
    refused_its_question: bool,
    absent: Option<String>,
}

impl Conversation {
    fn new(handshake: Handshake) -> Self {
        Conversation {
            handshake,
            sent: BTreeMap::new(),
            reading: BTreeSet::new(),
            capabilities: Value::Null,
            refused_its_question: false,
            next_id: INITIALIZE + 1,
            absent: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub path: PathBuf,
    pub place: Place,
    pub revision: u64,
    pub about: About,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum About {
    Hover,
    Definition,
    Candidates,
    Formatting(char),
    Document,
}

impl About {
    fn asking(self) -> (&'static str, &'static str, Option<&'static str>) {
        match self {
            About::Hover => (
                "textDocument/hover",
                "hoverProvider",
                Some("no-hover-support"),
            ),
            About::Definition => (
                "textDocument/definition",
                "definitionProvider",
                Some("no-definition-support"),
            ),
            About::Candidates => ("textDocument/completion", "completionProvider", None),
            About::Formatting(_) => (
                "textDocument/onTypeFormatting",
                "documentOnTypeFormattingProvider",
                None,
            ),
            About::Document => (
                "textDocument/formatting",
                "documentFormattingProvider",
                None,
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    asked: Ask,
    sent: BTreeMap<String, i64>,
    waiting: BTreeSet<String>,
    refused: bool,
}

enum Told {
    Nothing,
    Something(Vec<Effect>),
}

pub fn served_by(state: &State, path: &Path) -> Vec<String> {
    let Some(own) = language(state, path) else {
        return Vec::new();
    };
    let mut languages = vec![own.to_string()];
    if let Some(server) = state.servers.get(own) {
        for also in &server.also_served_by {
            if !languages.contains(also) && state.servers.contains_key(also) {
                languages.push(also.clone());
            }
        }
    }
    languages
}

fn serves(state: &State, language: &str, path: &Path) -> bool {
    served_by(state, path).iter().any(|held| held == language)
}

impl Ask {
    fn current(&self, state: &State) -> bool {
        state.current_buffer.as_ref() == Some(&self.path)
            && state
                .buffers
                .get(&self.path)
                .is_some_and(|buffer| match self.about {
                    About::Formatting(_) | About::Document => buffer.revision() == self.revision,
                    About::Hover if state.pointed_at == Pointed::Text(self.place) => true,
                    About::Hover if state.pointed_at == Pointed::Hover => true,
                    _ => buffer.line == self.place.line && buffer.column == self.place.column,
                })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    FailedToStart,
    Exited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Information => "information",
            Severity::Hint => "hint",
        }
    }
}

impl Severity {
    pub const ALL: [Severity; 4] = [
        Severity::Error,
        Severity::Warning,
        Severity::Information,
        Severity::Hint,
    ];
}

pub fn total(state: &State, severity: Severity) -> usize {
    state.diagnostic_totals[severity as usize]
}

fn recount(state: &mut State) {
    let mut totals = [0; 4];
    for diagnostic in state
        .diagnostics
        .values()
        .flat_map(|by| by.values().flatten())
    {
        totals[diagnostic.severity as usize] += 1;
    }
    state.diagnostic_totals = totals;
}

pub fn opening(state: &State) -> Severity {
    Severity::ALL
        .into_iter()
        .find(|severity| total(state, *severity) > 0)
        .unwrap_or(Severity::Error)
}

pub fn showing(state: &State) -> Option<Severity> {
    match state.corner {
        crate::layout::Corner::Diagnostics(severity) => Some(severity),
        _ => None,
    }
}

pub fn listed(state: &State) -> Vec<(&Path, Option<&Diagnostic>)> {
    let Some(severity) = showing(state) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (path, by_server) in &state.diagnostics {
        let mut under: Vec<&Diagnostic> = by_server
            .values()
            .flatten()
            .filter(|diagnostic| diagnostic.severity == severity)
            .collect();
        if under.is_empty() {
            continue;
        }
        under.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
        rows.push((path.as_path(), None));
        rows.extend(
            under
                .into_iter()
                .map(|diagnostic| (path.as_path(), Some(diagnostic))),
        );
    }
    rows
}

pub fn landing(state: &State, index: usize) -> Option<(PathBuf, Place)> {
    listed(state)
        .into_iter()
        .skip(index)
        .find_map(|(path, diagnostic)| {
            diagnostic.map(|diagnostic| {
                (
                    path.to_path_buf(),
                    Place {
                        line: diagnostic.line,
                        column: diagnostic.column,
                    },
                )
            })
        })
}

pub fn row_text(diagnostic: &Diagnostic) -> String {
    format!(
        "{}:{} {}",
        diagnostic.line,
        diagnostic.column,
        crate::debug::printable(diagnostic.message.lines().next().unwrap_or_default())
    )
}

pub fn severity_labels(state: &State, width: u16) -> Vec<String> {
    let whole: Vec<String> = Severity::ALL
        .into_iter()
        .map(|severity| {
            let name = match severity {
                Severity::Error => "Errors",
                Severity::Warning => "Warnings",
                Severity::Information => "Info",
                Severity::Hint => "Hints",
            };
            format!(" {name} {} ", total(state, severity))
        })
        .collect();
    match crate::layout::strip_width(&whole) <= width.saturating_sub(crate::layout::CORNER_TITLE) {
        true => whole,
        false => Severity::ALL
            .into_iter()
            .map(|severity| format!(" {} {} ", letter(severity), total(state, severity)))
            .collect(),
    }
}

pub fn letter(severity: Severity) -> char {
    match severity {
        Severity::Error => 'e',
        Severity::Warning => 'w',
        Severity::Information => 'i',
        Severity::Hint => 'h',
    }
}

pub fn nudge(state: &State) -> Vec<(Severity, String)> {
    [
        (Severity::Error, '\u{2716}'),
        (Severity::Warning, '\u{25b2}'),
    ]
    .into_iter()
    .filter(|(severity, _)| total(state, *severity) > 0)
    .map(|(severity, glyph)| (severity, format!("  {glyph} {}", total(state, severity))))
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    /// Inclusive end, since servers send zero-width ranges for missing tokens
    pub end_column: Option<usize>,
    pub severity: Severity,
    pub message: String,
}

pub fn ready(state: &State, language: &str) -> bool {
    matches!(
        state.lsp.get(language),
        Some(Conversation {
            handshake: Handshake::Ready,
            ..
        })
    )
}

pub(crate) fn written_off(state: &State, language: &str) -> bool {
    matches!(
        state.lsp.get(language),
        Some(Conversation {
            handshake: Handshake::Gone,
            ..
        })
    )
}

pub fn sync(state: &mut State) -> Vec<Effect> {
    let mut effects = Vec::new();
    forgotten(state);
    effects.extend(closed(state));
    let facts = facts(state);
    for (language, server) in servers_for_open_buffers(state) {
        if state.lsp.contains_key(&language) || state.lsp_running.contains(&language) {
            continue;
        }
        if unmet(state, &server).is_some() {
            continue;
        }
        effects.push(Effect::StartLsp {
            language,
            command: server.command,
            args: server
                .args
                .iter()
                .filter_map(|arg| filled(arg, &facts))
                .collect(),
        });
    }
    for language in state.lsp_running.clone() {
        if state.lsp.contains_key(&language) {
            continue;
        }
        effects.push(Effect::LspSend {
            language: language.clone(),
            json: request(
                INITIALIZE,
                "initialize",
                initialize(&state.root, options(state, &language, &facts)),
            ),
        });
        state
            .lsp
            .insert(language, Conversation::new(Handshake::Sent));
    }
    let ready: Vec<String> = state
        .lsp
        .iter()
        .filter(|(_, held)| held.handshake == Handshake::Ready)
        .map(|(language, _)| language.clone())
        .collect();
    for language in ready {
        effects.extend(documents(state, &language));
        effects.extend(review_reads(state, &language));
    }
    effects
}

fn forgotten(state: &mut State) {
    let appeared: Vec<String> = state
        .lsp
        .iter()
        .filter(|(_, held)| match &held.absent {
            Some(command) => state.commands_on_path.contains(command),
            None => false,
        })
        .map(|(language, _)| language.clone())
        .collect();
    for language in appeared {
        state.lsp.remove(&language);
    }
}

const REVIEW_VERSION: u64 = 0;

fn reviewed(state: &State) -> Vec<PathBuf> {
    crate::review::list(state)
        .into_iter()
        .map(|file| state.root.join(file))
        .collect()
}

fn review_reads(state: &mut State, language: &str) -> Vec<Effect> {
    if state.view != crate::View::Review {
        return Vec::new();
    }
    let wanted: Vec<PathBuf> = reviewed(state)
        .into_iter()
        .filter(|path| serves(state, language, path))
        .filter(|path| !state.buffers.contains_key(path))
        .filter(|path| {
            state
                .lsp
                .get(language)
                .is_some_and(|conversation| !conversation.sent.contains_key(path))
        })
        .filter(|path| {
            !state
                .lsp
                .values()
                .any(|conversation| conversation.reading.contains(path))
        })
        .collect();
    let Some(conversation) = state.lsp.get_mut(language) else {
        return Vec::new();
    };
    conversation.reading.extend(wanted.iter().cloned());
    wanted.into_iter().map(Effect::ReadForReview).collect()
}

pub fn read_for_review(state: &mut State, path: &Path, contents: &str) -> Vec<Effect> {
    let mut effects = Vec::new();
    for language in served_by(state, path) {
        if let Some(conversation) = state.lsp.get_mut(&language) {
            conversation.reading.remove(path);
        }
        if !ready(state, &language)
            || state.buffers.contains_key(path)
            || !reviewed(state).contains(&path.to_path_buf())
        {
            continue;
        }
        let Some(uri) = uri(path) else {
            continue;
        };
        let Some(conversation) = state.lsp.get_mut(&language) else {
            continue;
        };
        if conversation.sent.contains_key(path) {
            continue;
        }
        conversation.sent.insert(path.to_path_buf(), REVIEW_VERSION);
        effects.push(Effect::LspSend {
            language: language.clone(),
            json: notification(
                "textDocument/didOpen",
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri,
                        language_id: language_id(state, path).unwrap_or(&language).to_string(),
                        version: document_version(REVIEW_VERSION),
                        text: contents.to_string(),
                    },
                },
            ),
        });
    }
    effects
}

pub fn typed(state: &mut State) -> Vec<Effect> {
    if !typing_a_name(state) {
        if matches!(state.modal, crate::Modal::Candidates(_)) {
            state.modal = crate::Modal::None;
        }
        return Vec::new();
    }
    narrowed(state);
    let (_, capability, _) = About::Candidates.asking();
    let declared = state
        .current_buffer
        .as_deref()
        .and_then(|path| language(state, path))
        .filter(|language| ready(state, language))
        .and_then(|language| state.lsp.get(language))
        .is_some_and(|conversation| declares(&conversation.capabilities, capability));
    match declared {
        true => vec![Effect::DebounceCandidates(DEBOUNCE_MS)],
        false => Vec::new(),
    }
}

/// Sync first: a server asked before it hears the edit answers about stale text
pub fn on_type(state: &mut State, key: char) -> Vec<Effect> {
    let Some(path) = state.current_buffer.clone() else {
        return Vec::new();
    };
    let (_, capability, _) = About::Formatting(key).asking();
    let named = served_by(state, &path)
        .into_iter()
        .filter(|language| ready(state, language))
        .filter_map(|language| state.lsp.get(&language))
        .any(|conversation| triggers(&conversation.capabilities[capability]).contains(&key));
    if !named {
        return Vec::new();
    }
    let mut effects = sync(state);
    effects.extend(ask(state, About::Formatting(key)));
    effects
}

/// Sync first: a server asked before it hears the edit returns ranges into stale text
pub fn format(state: &mut State) -> Option<Vec<Effect>> {
    let path = state.current_buffer.clone()?;
    let (_, capability, _) = About::Document.asking();
    let declared = served_by(state, &path)
        .into_iter()
        .filter(|language| ready(state, language))
        .filter_map(|language| state.lsp.get(&language))
        .any(|conversation| declares(&conversation.capabilities, capability));
    if !declared {
        return None;
    }
    let mut effects = sync(state);
    effects.extend(ask(state, About::Document));
    Some(effects)
}

fn narrowed(state: &mut State) {
    let Some(word) = typed_word(state) else {
        if matches!(state.modal, crate::Modal::Candidates(_)) {
            state.modal = crate::Modal::None;
        }
        return;
    };
    let crate::Modal::Candidates(list) = &mut state.modal else {
        return;
    };
    list.typed = word;
    if list.shown().is_empty() {
        state.modal = crate::Modal::None;
        return;
    }
    list.selected = 0;
    list.first = 0;
}

fn typed_word(state: &State) -> Option<String> {
    let crate::Modal::Candidates(list) = &state.modal else {
        return None;
    };
    if state.current_buffer.as_ref() != Some(&list.asked.path) {
        return None;
    }
    let buffer = state.buffers.get(&list.asked.path)?;
    if buffer.line != list.asked.place.line {
        return None;
    }
    word_between(
        state,
        &list.asked.path,
        buffer.line,
        list.word_start,
        buffer.column,
    )
}

fn word_between(
    state: &State,
    path: &Path,
    line: usize,
    start: usize,
    column: usize,
) -> Option<String> {
    let text = state.buffers.get(path)?.shown().lines().nth(line - 1)?;
    Some(
        text.chars()
            .skip(start - 1)
            .take(column.saturating_sub(start))
            .collect(),
    )
}

fn typing_a_name(state: &State) -> bool {
    let Some(buffer) = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
    else {
        return false;
    };
    let Some(before) = buffer.column.checked_sub(2) else {
        return false;
    };
    buffer
        .shown()
        .lines()
        .nth(buffer.line.saturating_sub(1))
        .and_then(|line| line.chars().nth(before))
        .is_some_and(|character| character.is_alphanumeric() || character == '_')
}

fn closed(state: &mut State) -> Vec<Effect> {
    let mut open: BTreeSet<PathBuf> = state.buffers.keys().cloned().collect();
    open.extend(reviewed(state));
    let gone: Vec<(String, PathBuf)> = state
        .lsp
        .iter()
        .flat_map(|(language, conversation)| {
            conversation
                .sent
                .keys()
                .filter(|path| !open.contains(*path))
                .map(move |path| (language.clone(), path.clone()))
        })
        .collect();
    let mut effects = Vec::new();
    for conversation in state.lsp.values_mut() {
        conversation.reading.retain(|path| open.contains(path));
    }
    for (language, path) in gone {
        if let Some(conversation) = state.lsp.get_mut(&language) {
            conversation.sent.remove(&path);
        }
        if !state.buffers.contains_key(&path) {
            state.diagnostics.remove(&path);
            recount(state);
        }
        let Some(uri) = uri(&path) else { continue };
        if ready(state, &language) {
            effects.push(Effect::LspSend {
                language,
                json: notification(
                    "textDocument/didClose",
                    DidCloseTextDocumentParams {
                        text_document: TextDocumentIdentifier { uri },
                    },
                ),
            });
        }
    }
    effects
}

fn servers_for_open_buffers(state: &State) -> Vec<(String, Server)> {
    let mut named: BTreeMap<String, Server> = BTreeMap::new();
    for path in state.buffers.keys() {
        for language in served_by(state, path) {
            if let Some(server) = state.servers.get(&language) {
                named.insert(language, server.clone());
            }
        }
    }
    named.into_iter().collect()
}

fn documents(state: &mut State, language: &str) -> Vec<Effect> {
    let mut sent = state
        .lsp
        .get(language)
        .map(|held| held.sent.clone())
        .unwrap_or_default();
    let mut effects = Vec::new();
    for (path, buffer) in &state.buffers {
        if !serves(state, language, path) {
            continue;
        }
        let version = buffer.revision();
        let Some(uri) = uri(path) else { continue };
        let json = match sent.get(path) {
            Some(last) if *last == version => continue,
            Some(_) => notification(
                "textDocument/didChange",
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier {
                        uri,
                        version: document_version(version),
                    },
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: buffer.shown().to_string(),
                    }],
                },
            ),
            None => notification(
                "textDocument/didOpen",
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri,
                        language_id: language_id(state, path).unwrap_or(language).to_string(),
                        version: document_version(version),
                        text: buffer.shown().to_string(),
                    },
                },
            ),
        };
        sent.insert(path.clone(), version);
        effects.push(Effect::LspSend {
            language: language.to_string(),
            json,
        });
    }
    if let Some(conversation) = state.lsp.get_mut(language) {
        conversation.sent = sent;
    }
    effects
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    pub lines: Vec<crate::preview::Row>,
    pub from: usize,
    pub asked: Ask,
    pub first: usize,
    pub focused: bool,
    pub value: Option<crate::debug::Hovered>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Value(crate::debug::Row),
    Needs,
    Docs(crate::preview::Row),
}

impl Said {
    pub fn text(&self) -> String {
        match self {
            Said::Value(row) => format!("{}{} = {}", "  ".repeat(row.depth), row.name, row.value),
            Said::Needs => NEEDS_EVALUATE.to_string(),
            Said::Docs(row) => row.text(),
        }
    }
}

pub const NEEDS_EVALUATE: &str = "calls something \u{2014} evaluate to see it";

pub fn sections(state: &State) -> Vec<Said> {
    let Some(hover) = state.hover.as_ref() else {
        return Vec::new();
    };
    let value = match hover.value.as_ref().map(|hovered| &hovered.held) {
        Some(crate::debug::Held::NeedsEvaluate) => vec![Said::Needs],
        None => Vec::new(),
        Some(_) => crate::debug::hovered_rows(state)
            .into_iter()
            .map(Said::Value)
            .collect(),
    };
    value
        .into_iter()
        .chain(hover.lines.iter().cloned().map(Said::Docs))
        .collect()
}

pub fn rows(state: &State) -> usize {
    sections(state).len() + 2
}

pub fn covers(state: &State, line: usize) -> bool {
    state
        .hover
        .as_ref()
        .is_some_and(|hover| (hover.from..hover.from + rows(state)).contains(&line))
}

pub fn placement(state: &State) -> Option<Placement> {
    let hover = state.hover.as_ref()?;
    let texts: Vec<String> = sections(state).iter().map(Said::text).collect();
    let chips = crate::layout::strip_width(&crate::debug::hover_labels(state, u16::MAX)) as usize;
    Some(Placement {
        from: hover.from,
        column: 1,
        width: measured(texts.iter().map(String::as_str)).max(chips + 2),
        rows: rows(state),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub from: usize,
    pub column: usize,
    pub width: usize,
    pub rows: usize,
}

impl Placement {
    pub fn spot(&self, state: &State, panes: &Layout) -> Area {
        let editor = panes.editor;
        let screen_width = panes.ai.right();
        let height = (self.rows as u16).min(editor.height);
        let width = (self.width as u16).min(screen_width);
        let row = self.from.saturating_sub(1 + state.editor_scroll) as u16;
        let across = self.column.saturating_sub(1 + state.editor_hscroll) as u16;
        Area {
            x: (editor.x + crate::gutter(state) + across).min(screen_width.saturating_sub(width)),
            y: (editor.y + 1 + row).min(editor.bottom().saturating_sub(height)),
            width,
            height,
        }
    }
}

/// Screen columns, not chars: a wide glyph takes two cells
fn measured<'a>(lines: impl Iterator<Item = &'a str>) -> usize {
    lines
        .map(unicode_width::UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
        + 2
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidates {
    pub items: Vec<Candidate>,
    pub selected: usize,
    pub first: usize,
    pub from: usize,
    pub column: usize,
    pub asked: Ask,
    pub word_start: usize,
    pub ordered: bool,
    pub typed: String,
}

const WINDOW: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub label: String,
    pub insert: String,
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub snippet: bool,
}

impl Candidate {
    fn matched(&self) -> &str {
        self.filter.as_deref().unwrap_or(&self.label)
    }
}

impl Candidates {
    pub fn shown(&self) -> Vec<&Candidate> {
        if self.typed.is_empty() {
            return self.items.iter().collect();
        }
        let mut ranked: Vec<(i32, &Candidate)> = self
            .items
            .iter()
            .filter_map(|item| {
                crate::filter::score(&self.typed, item.matched()).map(|points| (points, item))
            })
            .collect();
        if !self.ordered {
            ranked.sort_by_key(|(points, _)| std::cmp::Reverse(*points));
        }
        ranked.into_iter().map(|(_, item)| item).collect()
    }

    pub fn rows(&self) -> usize {
        self.shown().len().min(WINDOW) + 2
    }

    pub fn placement(&self) -> Placement {
        Placement {
            from: self.from,
            column: self.column,
            width: measured(self.shown().into_iter().map(|item| item.label.as_str())),
            rows: self.rows(),
        }
    }

    pub fn covers(&self, line: usize) -> bool {
        (self.from..self.from + self.rows()).contains(&line)
    }

    pub fn step(&mut self, direction: crate::Direction) {
        let last = self.shown().len().saturating_sub(1);
        match direction {
            crate::Direction::Down => self.selected = (self.selected + 1).min(last),
            crate::Direction::Up => self.selected = self.selected.saturating_sub(1),
            crate::Direction::Left | crate::Direction::Right => {}
        }
    }

    pub fn scrolled(&mut self) {
        let shown = self.shown().len();
        let window = self.rows().saturating_sub(2);
        self.first = crate::layout::viewport(self.first, self.selected + 1, shown, window);
    }

    pub fn offering(state: &State, items: Vec<Candidate>, asked: Ask) -> Candidates {
        let (_, rows, columns) = crate::fits(state);
        let word_start = state
            .buffers
            .get(&asked.path)
            .map_or(asked.place.column, |buffer| {
                buffer.word_start(asked.place.line, asked.place.column)
            });
        let mut list = Candidates {
            ordered: !items.is_empty() && items.iter().all(|item| item.sort.is_some()),
            items,
            selected: 0,
            first: 0,
            from: 0,
            column: 1,
            word_start,
            typed: word_between(
                state,
                &asked.path,
                asked.place.line,
                word_start,
                asked.place.column,
            )
            .unwrap_or_default(),
            asked,
        };
        list.from = placed(
            list.rows(),
            list.asked.place.line,
            state.editor_scroll,
            rows,
        );
        list.column = at_column(
            list.asked.place.column,
            list.placement().width,
            columns,
            state.editor_hscroll,
        );
        list
    }

    pub fn chosen(&self) -> &Candidate {
        let shown = self.shown();
        shown[self.selected.min(shown.len() - 1)]
    }
}

pub fn ask(state: &mut State, about: About) -> Vec<Effect> {
    ask_at(state, about, None)
}

pub fn ask_at(state: &mut State, about: About, at: Option<Place>) -> Vec<Effect> {
    let Some(path) = state.current_buffer.clone() else {
        return Vec::new();
    };
    let (method, capability, refusal) = about.asking();
    state.lsp_asked.remove(&about);
    let serving: Vec<String> = served_by(state, &path)
        .into_iter()
        .filter(|language| ready(state, language))
        .collect();
    if serving.is_empty() {
        return match refusal {
            Some(_) => vec![Effect::Notify("no-language-server")],
            None => Vec::new(),
        };
    }
    let (Some(buffer), Some(uri)) = (state.buffers.get(&path), uri(&path)) else {
        return Vec::new();
    };
    let place = at.unwrap_or(Place {
        line: buffer.line,
        column: buffer.column,
    });
    let position = position(buffer.shown(), place);
    let revision = buffer.revision();
    let options = formatting(state);
    let mut sent = BTreeMap::new();
    let mut effects = Vec::new();
    for language in serving {
        let Some(conversation) = state.lsp.get_mut(&language) else {
            continue;
        };
        if !declares(&conversation.capabilities, capability) {
            continue;
        }
        if let About::Formatting(key) = about {
            if !triggers(&conversation.capabilities[capability]).contains(&key) {
                continue;
            }
        }
        let id = conversation.next_id;
        conversation.next_id += 1;
        sent.insert(language.clone(), id);
        let at = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position,
        };
        effects.push(Effect::LspSend {
            language,
            json: match about {
                About::Formatting(key) => request(
                    id,
                    method,
                    DocumentOnTypeFormattingParams {
                        text_document_position: at,
                        ch: key.to_string(),
                        options: options.clone(),
                    },
                ),
                About::Document => request(
                    id,
                    method,
                    DocumentFormattingParams {
                        text_document: at.text_document,
                        options: options.clone(),
                        work_done_progress_params: Default::default(),
                    },
                ),
                _ => request(id, method, at),
            },
        });
    }
    if sent.is_empty() {
        return refusal.map(Effect::Notify).into_iter().collect();
    }
    state.lsp_asked.insert(
        about,
        Question {
            asked: Ask {
                path,
                place,
                revision,
                about,
            },
            waiting: sent.keys().cloned().collect(),
            sent,
            refused: false,
        },
    );
    effects
}

pub fn outstanding(state: &State, language: &str) -> usize {
    state
        .lsp_asked
        .values()
        .filter(|question| question.waiting.contains(language))
        .count()
}

/// LSP columns are UTF-16 code units, so an emoji counts as two
fn position(text: &str, place: Place) -> Position {
    let line = text.lines().nth(place.line.saturating_sub(1)).unwrap_or("");
    let character: usize = line
        .chars()
        .take(place.column.saturating_sub(1))
        .map(char::len_utf16)
        .sum();
    Position {
        line: place.line.saturating_sub(1) as u32,
        character: character as u32,
    }
}

fn answered(state: &mut State, language: &str, id: i64, message: &Value) -> Vec<Effect> {
    let Some(about) = state
        .lsp_asked
        .iter()
        .find(|(_, question)| question.sent.get(language) == Some(&id))
        .map(|(about, _)| *about)
    else {
        return Vec::new();
    };
    let Some(question) = state.lsp_asked.get_mut(&about) else {
        return Vec::new();
    };
    question.waiting.remove(language);
    let error = message.get("error").is_some();
    question.refused |= error;
    let ask = question.asked.clone();
    let last = question.waiting.is_empty();
    let told = last.then(|| question.clone());
    if last {
        state.lsp_asked.remove(&about);
    }
    if !ask.current(state) {
        return Vec::new();
    }
    let answer = match error {
        true => Told::Nothing,
        false => match about {
            About::Hover => hovered(state, ask, &message["result"]),
            About::Definition => jumped(state, ask, &message["result"]),
            About::Candidates => offered(state, ask, &message["result"]),
            About::Formatting(_) | About::Document => formatted(state, ask, &message["result"]),
        },
    };
    match answer {
        Told::Something(effects) => {
            state.lsp_asked.remove(&about);
            effects
        }
        Told::Nothing => match told {
            Some(question) => empty_handed(state, &question),
            None => Vec::new(),
        },
    }
}

fn empty_handed(state: &mut State, question: &Question) -> Vec<Effect> {
    let nobody_knew = match question.asked.about {
        About::Hover => "nothing-known-here",
        About::Definition => "no-definition",
        About::Candidates => {
            if matches!(state.modal, crate::Modal::Candidates(_)) {
                state.modal = crate::Modal::None;
            }
            return Vec::new();
        }
        About::Formatting(_) => return Vec::new(),
        About::Document => "nothing-to-format",
    };
    if question.sent.keys().all(|language| {
        state
            .lsp
            .get(language)
            .is_some_and(|conversation| conversation.refused_its_question)
    }) {
        return vec![Effect::Notify("needs-a-companion")];
    }
    match question.refused {
        true => vec![Effect::Notify("language-server-error")],
        false => vec![Effect::Notify(nobody_knew)],
    }
}

fn hovered(state: &mut State, ask: Ask, result: &Value) -> Told {
    let (_, rows, _) = crate::fits(state);
    let Some(lines) = says(result, measure(state.screen_width as usize)) else {
        return Told::Nothing;
    };
    let value = state
        .hover
        .take()
        .filter(|hover| hover.asked.place == ask.place)
        .and_then(|hover| hover.value);
    state.hover = Some(Hover {
        lines,
        from: 0,
        asked: ask,
        first: 0,
        focused: false,
        value,
    });
    settle_box(state, rows);
    Told::Something(Vec::new())
}

fn settle_box(state: &mut State, pane_rows: usize) {
    let (count, scroll) = (rows(state), state.editor_scroll);
    if let Some(hover) = state.hover.as_mut() {
        hover.from = placed(count, hover.asked.place.line, scroll, pane_rows);
    }
}

pub fn value_hover(state: &mut State, at: Place) -> Vec<Effect> {
    let (hovered, effects) = crate::debug::hovered(state, at);
    let Some(path) = state.current_buffer.clone() else {
        return effects;
    };
    let Some(revision) = state
        .buffers
        .get(&path)
        .map(crate::editor::Buffer::revision)
    else {
        return effects;
    };
    match state.hover.as_mut().filter(|hover| hover.asked.place == at) {
        Some(hover) => hover.value = hovered,
        None if hovered.is_some() => {
            state.hover = Some(Hover {
                lines: Vec::new(),
                from: 0,
                asked: Ask {
                    path,
                    place: at,
                    revision,
                    about: About::Hover,
                },
                first: 0,
                focused: false,
                value: hovered,
            });
        }
        None => return effects,
    }
    let pane_rows = crate::fits(state).1;
    settle_box(state, pane_rows);
    effects
}

fn offered(state: &mut State, ask: Ask, result: &Value) -> Told {
    let items = candidates(result);
    let open = matches!(state.modal, crate::Modal::Candidates(_));
    if items.is_empty() {
        return Told::Nothing;
    }
    if !open && state.modal != crate::Modal::None {
        return Told::Nothing;
    }
    let list = Candidates::offering(state, items, ask);
    if list.shown().is_empty() {
        return Told::Nothing;
    }
    state.modal = crate::Modal::Candidates(list);
    Told::Something(Vec::new())
}

fn formatted(state: &mut State, ask: Ask, result: &Value) -> Told {
    let Ok(edits) = serde_json::from_value::<Vec<TextEdit>>(result.clone()) else {
        return Told::Nothing;
    };
    let Some(buffer) = state.buffers.get_mut(&ask.path) else {
        return Told::Nothing;
    };
    let text = buffer.shown().to_string();
    let spans: Vec<(Place, Place, String)> = edits
        .into_iter()
        .map(|edit| {
            (
                place(Some(&text), edit.range.start),
                place(Some(&text), edit.range.end),
                edit.new_text,
            )
        })
        .collect();
    if spans.is_empty() {
        return Told::Nothing;
    }
    buffer.reformat(&spans);
    Told::Something(Vec::new())
}

fn at_column(cursor: usize, width: usize, columns: usize, hscroll: usize) -> usize {
    cursor
        .min(hscroll + (columns + 1).saturating_sub(width))
        .max(1)
}

fn candidates(result: &Value) -> Vec<Candidate> {
    let Ok(response) = serde_json::from_value::<CompletionResponse>(result.clone()) else {
        return Vec::new();
    };
    let items = match response {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => list.items,
    };
    let mut offered: Vec<Candidate> = items
        .into_iter()
        .map(|item| Candidate {
            insert: item.insert_text.unwrap_or_else(|| item.label.clone()),
            filter: item.filter_text,
            sort: item.sort_text,
            snippet: item.insert_text_format == Some(InsertTextFormat::SNIPPET),
            label: item.label,
        })
        .collect();
    if offered.iter().all(|candidate| candidate.sort.is_some()) {
        offered.sort_by(|a, b| a.sort.cmp(&b.sort));
    }
    offered
}

pub fn snippet(text: &str) -> (String, Vec<usize>) {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut stops: Vec<(u32, usize)> = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '\\' if matches!(chars.get(at + 1), Some('$' | '\\')) => {
                out.push(chars[at + 1]);
                at += 2;
            }
            '$' => match placeholder(&chars[at..]) {
                Some((number, default, taken)) => {
                    stops.push((number, out.chars().count()));
                    out.push_str(&default);
                    at += taken;
                }
                None => {
                    out.push('$');
                    at += 1;
                }
            },
            c => {
                out.push(c);
                at += 1;
            }
        }
    }
    stops.sort_by_key(|(_, offset)| *offset);
    let ends_at_a_stop = stops.iter().any(|(number, _)| *number == 0);
    let mut ordered: Vec<usize> = stops.into_iter().map(|(_, offset)| offset).collect();
    if !ends_at_a_stop {
        ordered.push(out.chars().count());
    }
    (out, ordered)
}

fn placeholder(rest: &[char]) -> Option<(u32, String, usize)> {
    let digits = |from: usize| {
        let taken: String = rest[from..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        taken
            .parse::<u32>()
            .ok()
            .map(|number| (number, taken.len()))
    };
    match rest.get(1)? {
        '{' => {
            let (number, spelled) = digits(2)?;
            let body = 2 + spelled;
            match rest.get(body)? {
                '}' => Some((number, String::new(), body + 1)),
                ':' => {
                    let default: String =
                        rest[body + 1..].iter().take_while(|c| **c != '}').collect();
                    let closed = rest.get(body + 1 + default.chars().count()) == Some(&'}');
                    closed.then(|| (number, default.clone(), body + default.chars().count() + 2))
                }
                _ => None,
            }
        }
        _ => {
            let (number, spelled) = digits(1)?;
            Some((number, String::new(), 1 + spelled))
        }
    }
}

fn jumped(state: &mut State, ask: Ask, result: &Value) -> Told {
    let (inside, outside) = definitions(state, result)
        .into_iter()
        .partition::<Vec<_>, _>(|(path, _)| path.starts_with(&state.root));
    match (inside.as_slice(), outside.first()) {
        ([], None) => Told::Nothing,
        ([], Some((path, _))) => Told::Something(vec![Effect::notify_about(
            "definition-outside-workspace",
            path.display().to_string(),
        )]),
        ([(path, at)], _) if *path == ask.path => {
            crate::history::leaving(state, *at);
            if let Some(buffer) = state.buffers.get_mut(path) {
                buffer.go_to_place(*at);
            }
            Told::Something(Vec::new())
        }
        ([(path, at)], _) => Told::Something(vec![Effect::OpenAt {
            path: path.clone(),
            at: *at,
        }]),
        (several, _) => {
            let hits = several
                .iter()
                .filter_map(|(path, at)| {
                    Some(Hit {
                        file: path
                            .strip_prefix(&state.root)
                            .ok()?
                            .to_string_lossy()
                            .into_owned(),
                        line: at.line as u32,
                        column: at.column as u32,
                        text: String::new(),
                    })
                })
                .collect();
            state.search = Some(Search {
                query: crate::editor::Buffer::text_box(
                    &crate::word_under_cursor(state).unwrap_or_default(),
                ),
                results: Results {
                    hits,
                    ..Results::default()
                },
                ..Search::default()
            });
            Told::Something(Vec::new())
        }
    }
}

fn definitions(state: &State, result: &Value) -> Vec<(PathBuf, Place)> {
    let Ok(response) = serde_json::from_value::<GotoDefinitionResponse>(result.clone()) else {
        return Vec::new();
    };
    let located: Vec<(Url, Range)> = match response {
        GotoDefinitionResponse::Scalar(location) => vec![(location.uri, location.range)],
        GotoDefinitionResponse::Array(locations) => locations
            .into_iter()
            .map(|location| (location.uri, location.range))
            .collect(),
        GotoDefinitionResponse::Link(links) => links
            .into_iter()
            .map(|link| (link.target_uri, link.target_selection_range))
            .collect(),
    };
    located
        .into_iter()
        .filter_map(|(uri, range)| {
            let path = uri.to_file_path().ok()?;
            let text = state.buffers.get(&path).map(|buffer| buffer.shown());
            Some((path, place(text, range.start)))
        })
        .collect()
}

fn place(text: Option<&str>, position: Position) -> Place {
    let line = position.line as usize + 1;
    let wanted = position.character as usize;
    let column = match text.and_then(|text| text.lines().nth(line - 1)) {
        Some(held) => {
            let mut units = 0;
            let mut column = 1;
            for character in held.chars() {
                if units >= wanted {
                    break;
                }
                units += character.len_utf16();
                column += 1;
            }
            column
        }
        None => wanted + 1,
    };
    Place { line, column }
}

pub fn hover_stands(state: &State) -> bool {
    state.diff.is_none()
        && state.walking.is_none()
        && !crate::previewing(state)
        && state
            .hover
            .as_ref()
            .is_some_and(|hover| hover.asked.current(state))
}

fn measure(width: usize) -> usize {
    width.saturating_sub(4).clamp(20, WIDEST)
}

const WIDEST: usize = 60;

const TALLEST: usize = 20;

fn says(result: &Value, measure: usize) -> Option<Vec<preview::Row>> {
    let hover: ServerHover = serde_json::from_value(result.clone()).ok()?;
    let rows = match hover.contents {
        HoverContents::Markup(markup) => match markup.kind {
            MarkupKind::Markdown => preview::rows(&markup.value, measure),
            MarkupKind::PlainText => plain_rows(&markup.value, measure),
        },
        HoverContents::Scalar(marked) => marked_rows(marked, measure),
        HoverContents::Array(marked) => marked
            .into_iter()
            .flat_map(|one| marked_rows(one, measure))
            .collect(),
    };
    let mut rows: Vec<preview::Row> = rows
        .into_iter()
        .flat_map(|row| broken(row, measure))
        .collect();
    if rows.iter().all(|row| row.text().trim().is_empty()) {
        return None;
    }
    if rows.len() > TALLEST {
        rows.truncate(TALLEST - 1);
        rows.push(cut_short(rows.last().map_or(1, |row| row.line)));
    }
    Some(rows)
}

fn marked_rows(marked: MarkedString, measure: usize) -> Vec<preview::Row> {
    match marked {
        MarkedString::String(text) => preview::rows(&text, measure),
        MarkedString::LanguageString(language) => preview::rows(
            &format!("```{}\n{}\n```", language.language, language.value),
            measure,
        ),
    }
}

fn plain_rows(text: &str, measure: usize) -> Vec<preview::Row> {
    text.trim()
        .lines()
        .enumerate()
        .flat_map(|(at, line)| {
            textwrap::wrap(line, measure)
                .into_iter()
                .map(|piece| preview::Row {
                    kind: preview::RowKind::Paragraph,
                    line: at + 1,
                    pieces: vec![preview::Piece {
                        text: piece.into_owned(),
                        emphasis: preview::Emphasis::default(),
                        token: None,
                    }],
                    refused: None,
                })
                .collect::<Vec<preview::Row>>()
        })
        .collect()
}

fn broken(row: preview::Row, measure: usize) -> Vec<preview::Row> {
    if !matches!(row.kind, preview::RowKind::Code | preview::RowKind::Diagram)
        || unicode_width::UnicodeWidthStr::width(row.text().as_str()) <= measure
    {
        return vec![row];
    }
    preview::wrapped(&row.pieces, textwrap::Options::new(measure))
        .into_iter()
        .map(|pieces| preview::Row {
            pieces,
            ..row.clone()
        })
        .collect()
}

fn cut_short(line: usize) -> preview::Row {
    preview::Row {
        kind: preview::RowKind::Paragraph,
        line,
        pieces: vec![preview::Piece {
            text: "\u{2026}".to_string(),
            emphasis: preview::Emphasis::default(),
            token: None,
        }],
        refused: None,
    }
}

fn placed(tall: usize, line: usize, top: usize, visible: usize) -> usize {
    if line + tall <= top + visible || line <= tall {
        return line + 1;
    }
    line - tall
}

pub fn received(state: &mut State, language: &str, message: &str) -> Vec<Effect> {
    let Ok(mut message) = serde_json::from_str::<Value>(message) else {
        return Vec::new();
    };
    let id = message.get("id").and_then(Value::as_i64);
    if let (Some(id), Some(method)) = (id, message.get("method")) {
        return vec![Effect::LspSend {
            language: language.to_string(),
            json: json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("Varde does not implement {method}")},
            })
            .to_string(),
        }];
    }
    if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics") {
        published(state, language, message["params"].take());
        return Vec::new();
    }
    if let Some(asked) = state
        .servers
        .get(language)
        .and_then(|server| server.unanswerable.as_ref())
        .filter(|asked| message.get("method").and_then(Value::as_str) == Some(&asked.request))
        .cloned()
    {
        if let Some(conversation) = state.lsp.get_mut(language) {
            conversation.refused_its_question = true;
        }
        return refused(language, &asked, &message["params"]);
    }
    let Some(id) = id else {
        return Vec::new();
    };
    if id == INITIALIZE && waiting(state, language) {
        return handshaken(state, language, &message);
    }
    if let Some(effects) = crate::debug::ported(state, language, id, &message) {
        return effects;
    }
    answered(state, language, id, &message)
}

/// Some JSON-RPC libraries nest the args in an extra array; answer in the shape that arrived
fn refused(language: &str, asked: &crate::startup::Unanswerable, params: &Value) -> Vec<Effect> {
    let Some(outer) = params.as_array() else {
        return Vec::new();
    };
    let (wrapped, question) = match outer.first() {
        Some(Value::Array(inner)) => (true, inner.as_slice()),
        Some(_) => (false, outer.as_slice()),
        None => return Vec::new(),
    };
    let Some(tag) = question.first() else {
        return Vec::new();
    };
    let answer = json!([tag, Value::Null]);
    vec![Effect::LspSend {
        language: language.to_string(),
        json: json!({
            "jsonrpc": "2.0",
            "method": asked.response,
            "params": if wrapped { json!([answer]) } else { answer },
        })
        .to_string(),
    }]
}

fn handshaken(state: &mut State, language: &str, message: &Value) -> Vec<Effect> {
    let Some(result) = message.get("result") else {
        return gone(state, language, Gone::FailedToStart);
    };
    let capabilities = result["capabilities"].clone();
    let Some(conversation) = state.lsp.get_mut(language) else {
        return Vec::new();
    };
    conversation.handshake = Handshake::Ready;
    conversation.capabilities = capabilities;
    vec![Effect::LspSend {
        language: language.to_string(),
        json: notification("initialized", InitializedParams {}),
    }]
}

/// One field, not ServerCapabilities: one malformed sibling field would fail the whole parse
fn declares(capabilities: &Value, capability: &str) -> bool {
    match &capabilities[capability] {
        Value::Bool(declared) => *declared,
        Value::Object(_) => true,
        _ => false,
    }
}

fn triggers(declared: &Value) -> Vec<char> {
    let Ok(named) = serde_json::from_value::<DocumentOnTypeFormattingOptions>(declared.clone())
    else {
        return Vec::new();
    };
    std::iter::once(named.first_trigger_character)
        .chain(named.more_trigger_character.unwrap_or_default())
        .filter_map(|named| match named.chars().collect::<Vec<char>>()[..] {
            [one] => Some(one),
            _ => None,
        })
        .collect()
}

fn published(state: &mut State, language: &str, params: Value) {
    let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(params) else {
        return;
    };
    let Ok(path) = params.uri.to_file_path() else {
        return;
    };
    if let (Some(version), Some(buffer)) = (params.version, state.buffers.get(&path)) {
        if version != document_version(buffer.revision()) {
            return;
        }
    }
    state.diagnostics.entry(path).or_default().insert(
        language.to_string(),
        params
            .diagnostics
            .into_iter()
            .map(|diagnostic| Diagnostic {
                line: diagnostic.range.start.line as usize + 1,
                column: diagnostic.range.start.character as usize + 1,
                end_column: (diagnostic.range.end.line == diagnostic.range.start.line)
                    .then_some(diagnostic.range.end.character as usize),
                severity: match diagnostic.severity {
                    Some(DiagnosticSeverity::WARNING) => Severity::Warning,
                    Some(DiagnosticSeverity::INFORMATION) => Severity::Information,
                    Some(DiagnosticSeverity::HINT) => Severity::Hint,
                    _ => Severity::Error,
                },
                message: diagnostic.message,
            })
            .collect(),
    );
    recount(state);
}

pub fn mark(state: &State, path: &Path, line: usize) -> Option<Severity> {
    worst(state, path, line).map(|diagnostic| diagnostic.severity)
}

pub fn message_at_cursor(state: &State) -> Option<&str> {
    let path = state.current_buffer.as_ref()?;
    let buffer = state.buffers.get(path)?;
    worst(state, path, buffer.line).map(|diagnostic| diagnostic.message.as_str())
}

fn worst<'a>(state: &'a State, path: &Path, line: usize) -> Option<&'a Diagnostic> {
    state
        .diagnostics
        .get(path)?
        .values()
        .flatten()
        .filter(|diagnostic| diagnostic.line == line)
        .min_by_key(|diagnostic| diagnostic.severity)
}

pub fn underlines(state: &State, path: &Path, line: usize) -> Vec<(usize, usize, Severity)> {
    spans(state, path, line)
        .into_iter()
        .map(|(from, to, diagnostic)| (from, to, diagnostic.severity))
        .collect()
}

fn spans<'a>(state: &'a State, path: &Path, line: usize) -> Vec<(usize, usize, &'a Diagnostic)> {
    let Some(by_language) = state.diagnostics.get(path) else {
        return Vec::new();
    };
    let here: Vec<&Diagnostic> = by_language
        .values()
        .flatten()
        .filter(|diagnostic| diagnostic.line == line)
        .collect();
    if here.is_empty() {
        return Vec::new();
    }
    let Some(len) = state
        .buffers
        .get(path)
        .and_then(|buffer| buffer.shown().lines().nth(line.checked_sub(1)?))
        .map(|text| text.chars().count())
        .filter(|len| *len > 0)
    else {
        return Vec::new();
    };
    here.into_iter()
        .map(|diagnostic| {
            let from = diagnostic.column.clamp(1, len);
            let to = diagnostic.end_column.unwrap_or(len).clamp(from, len);
            (from, to, diagnostic)
        })
        .collect()
}

pub fn pointed(state: &State) -> Option<(Vec<String>, Placement)> {
    if state.diff.is_some() || crate::previewing(state) {
        return None;
    }
    let Pointed::Text(at) = state.pointed_at else {
        return None;
    };
    let path = state.current_buffer.as_ref()?;
    let (from, _, diagnostic) = spans(state, path, at.line)
        .into_iter()
        .filter(|(from, to, _)| (*from..=*to).contains(&at.column))
        .min_by_key(|(_, _, diagnostic)| diagnostic.severity)?;
    let columns = measure(state.screen_width as usize);
    // textwrap does not break on newlines, so wrap each line of the message separately
    let mut lines: Vec<String> = diagnostic
        .message
        .lines()
        .flat_map(|line| {
            textwrap::wrap(line, columns)
                .into_iter()
                .map(|row| row.into_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    lines.truncate(TALLEST);
    let (_, visible, _) = crate::fits(state);
    let rows = lines.len() + 2;
    let placement = Placement {
        from: placed(rows, at.line, state.editor_scroll, visible),
        column: from,
        width: measured(lines.iter().map(String::as_str)),
        rows,
    };
    Some((lines, placement))
}

fn waiting(state: &State, language: &str) -> bool {
    matches!(
        state.lsp.get(language),
        Some(Conversation {
            handshake: Handshake::Sent,
            ..
        })
    )
}

pub fn gone(state: &mut State, language: &str, why: Gone) -> Vec<Effect> {
    let told = written_off(state, language);
    let mut written_off = Conversation::new(Handshake::Gone);
    let missing = state
        .servers
        .get(language)
        .map(|server| server.command.clone())
        .filter(|command| !state.commands_on_path.contains(command));
    if why == Gone::FailedToStart && !state.lsp_running.contains(language) {
        written_off.absent = missing;
    }
    state.lsp.insert(language.to_string(), written_off);
    state.diagnostics.retain(|_, by_language| {
        by_language.remove(language);
        !by_language.is_empty()
    });
    recount(state);
    if state
        .hover
        .as_ref()
        .is_some_and(|hover| serves(state, language, &hover.asked.path))
    {
        state.hover = None;
    }
    let over: Vec<About> = state
        .lsp_asked
        .iter_mut()
        .filter_map(|(about, question)| {
            question.waiting.remove(language);
            question.waiting.is_empty().then_some(*about)
        })
        .collect();
    let mut answers = Vec::new();
    for about in over {
        let Some(question) = state.lsp_asked.remove(&about) else {
            continue;
        };
        if !question.asked.current(state) {
            continue;
        }
        answers.extend(empty_handed(state, &question));
    }
    if told {
        return answers;
    }
    match why {
        Gone::FailedToStart => vec![Effect::Notify("language-server-failed")],
        Gone::Exited => vec![Effect::Notify("language-server-stopped")],
    }
}

pub(crate) fn filled(text: &str, facts: &BTreeMap<String, Option<String>>) -> Option<String> {
    let mut filled = text.to_string();
    for (name, found) in facts {
        let asked = format!("${{{name}}}");
        if filled.contains(&asked) {
            filled = filled.replace(&asked, found.as_ref()?);
        }
    }
    Some(filled)
}

fn filled_options(
    options: &serde_json::Map<String, Value>,
    facts: &BTreeMap<String, Option<String>>,
) -> serde_json::Map<String, Value> {
    options
        .iter()
        .filter_map(|(key, value)| Some((key.clone(), filled_value(value, facts)?)))
        .collect()
}

fn filled_value(value: &Value, facts: &BTreeMap<String, Option<String>>) -> Option<Value> {
    Some(match value {
        Value::String(text) => Value::String(filled(text, facts)?),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| Some((key.clone(), filled_value(value, facts)?)))
                .collect::<Option<_>>()?,
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| filled_value(item, facts))
                .collect::<Option<_>>()?,
        ),
        other => other.clone(),
    })
}

pub(crate) fn unmet(state: &State, server: &Server) -> Option<String> {
    let options = server
        .initialization_options
        .as_ref()
        .map(|options| Value::Object(options.clone()).to_string())
        .unwrap_or_default();
    facts(state)
        .iter()
        .find(|(name, found)| {
            let asked = format!("${{{name}}}");
            found.is_none()
                && !state.facts.get(*name).is_some_and(|fact| fact.optional)
                && (server.args.iter().any(|arg| arg.contains(&asked)) || options.contains(&asked))
        })
        .map(|(name, _)| name.clone())
}

pub(crate) fn facts(state: &State) -> BTreeMap<String, Option<String>> {
    state
        .facts
        .keys()
        .map(|name| (name.clone(), state.workspace_facts.get(name).cloned()))
        .collect()
}

fn options(
    state: &State,
    language: &str,
    facts: &BTreeMap<String, Option<String>>,
) -> Option<Value> {
    let own = state
        .servers
        .get(language)
        .and_then(|server| server.initialization_options.as_ref());
    let plugins = state
        .adapters
        .values()
        .filter(|adapter| adapter.server.as_deref() == Some(language))
        .filter_map(|adapter| adapter.plugin.as_ref());
    own.into_iter()
        .chain(plugins)
        .map(|options| Value::Object(filled_options(options, facts)))
        .reduce(|mut into, from| {
            joined(&mut into, from);
            into
        })
}

fn joined(into: &mut Value, from: Value) {
    match (into, from) {
        (Value::Object(into), Value::Object(from)) => {
            for (key, value) in from {
                match into.get_mut(&key) {
                    Some(held) => joined(held, value),
                    None => {
                        into.insert(key, value);
                    }
                }
            }
        }
        (Value::Array(into), Value::Array(from)) => into.extend(from),
        (into, from) => *into = from,
    }
}

fn initialize(root: &Path, options: Option<Value>) -> InitializeParams {
    InitializeParams {
        process_id: None,
        root_uri: uri(root),
        // typescript-language-server withholds diagnostics unless publishDiagnostics is declared
        capabilities: ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                publish_diagnostics: Some(PublishDiagnosticsClientCapabilities::default()),
                completion: Some(CompletionClientCapabilities {
                    completion_item: Some(CompletionItemCapability {
                        snippet_support: Some(true),
                        ..CompletionItemCapability::default()
                    }),
                    ..CompletionClientCapabilities::default()
                }),
                hover: Some(HoverClientCapabilities {
                    content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    ..HoverClientCapabilities::default()
                }),
                ..TextDocumentClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        },
        initialization_options: options,
        ..InitializeParams::default()
    }
}

/// LSP versions are i32; a wrapped negative version reads as older than everything
fn document_version(revision: u64) -> i32 {
    i32::try_from(revision).unwrap_or(i32::MAX)
}

fn uri(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

pub(crate) fn execute(state: &mut State, language: &str, command: &str) -> Option<(i64, Effect)> {
    let conversation = state
        .lsp
        .get_mut(language)
        .filter(|held| held.handshake == Handshake::Ready)?;
    let id = conversation.next_id;
    conversation.next_id += 1;
    let json = request(
        id,
        "workspace/executeCommand",
        json!({ "command": command }),
    );
    Some((
        id,
        Effect::LspSend {
            language: language.to_string(),
            json,
        },
    ))
}

fn request(id: i64, method: &str, params: impl Serialize) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn notification(method: &str, params: impl Serialize) -> String {
    json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Buffer;

    #[test]
    fn a_plugin_joins_the_options_of_the_server_that_hosts_it() {
        let mut state = workspace("rust", "rust-analyzer");
        state.facts.insert(
            "plugin_jar".to_string(),
            serde_json::from_value(json!({"marker": "plugin.jar"})).unwrap(),
        );
        state
            .servers
            .get_mut("rust")
            .unwrap()
            .initialization_options =
            serde_json::from_str(r#"{"bundles": ["/own.jar"], "x": {"y": 1}}"#).ok();
        let adapter = |server: &str, plugin: &str| crate::startup::Adapter {
            command: "start".to_string(),
            args: Vec::new(),
            install: std::collections::BTreeMap::new(),
            server: Some(server.to_string()),
            plugin: serde_json::from_str(plugin).ok(),
            hot_replace: None,
            ..Default::default()
        };
        state.adapters.insert(
            "rust".to_string(),
            adapter("rust", r#"{"bundles": ["${plugin_jar}"], "x": {"z": 2}}"#),
        );
        state
            .adapters
            .insert("other".to_string(), adapter("go", r#"{"elsewhere": true}"#));
        assert_eq!(
            options(&state, "rust", &facts(&state)),
            Some(json!({"bundles": ["/own.jar"], "x": {"y": 1, "z": 2}}))
        );
        state
            .workspace_facts
            .insert("plugin_jar".to_string(), "/plugin.jar".to_string());
        assert_eq!(
            options(&state, "rust", &facts(&state)),
            Some(json!({"bundles": ["/own.jar", "/plugin.jar"], "x": {"y": 1, "z": 2}}))
        );
        assert_eq!(
            options(&state, "go", &facts(&state)),
            Some(json!({"elsewhere": true}))
        );
        assert_eq!(options(&state, "vue", &facts(&state)), None);
    }

    #[test]
    fn a_fact_that_vanished_takes_its_argument_and_its_option_key_with_it() {
        let unfound: BTreeMap<String, Option<String>> =
            [("typescript_sdk".to_string(), None)].into_iter().collect();
        let found: BTreeMap<String, Option<String>> = [(
            "typescript_sdk".to_string(),
            Some("/w/node_modules/typescript/lib".to_string()),
        )]
        .into_iter()
        .collect();
        assert_eq!(filled("--tsdk=${typescript_sdk}", &unfound), None);
        assert_eq!(
            filled("--tsdk=${typescript_sdk}", &found).as_deref(),
            Some("--tsdk=/w/node_modules/typescript/lib")
        );
        assert_eq!(
            filled("--shell=${HOME}", &unfound).as_deref(),
            Some("--shell=${HOME}")
        );
        let options: serde_json::Map<String, Value> = serde_json::from_str(
            r#"{"tsserver": {"path": "${typescript_sdk}"}, "maxTsServerMemory": 3072}"#,
        )
        .expect("valid JSON");
        assert_eq!(
            Value::Object(filled_options(&options, &unfound)),
            serde_json::json!({"maxTsServerMemory": 3072}),
            "the key that carried the name goes, and its siblings stay"
        );
        let nested: serde_json::Map<String, Value> = serde_json::from_str(
            r#"{"plugins": [{"name": "a-plugin", "location": "${typescript_sdk}"}], "maxTsServerMemory": 3072}"#,
        )
        .expect("valid JSON");
        assert_eq!(
            Value::Object(filled_options(&nested, &unfound)),
            serde_json::json!({"maxTsServerMemory": 3072})
        );
        assert_eq!(
            Value::Object(filled_options(&nested, &found)),
            serde_json::json!({
                "plugins": [{"name": "a-plugin", "location": "/w/node_modules/typescript/lib"}],
                "maxTsServerMemory": 3072,
            })
        );
    }

    fn workspace(language: &str, command: &str) -> State {
        let mut state = State {
            root: PathBuf::from("/home/me/project"),
            ..State::default()
        };
        state.servers.insert(
            language.to_string(),
            Server {
                command: command.to_string(),
                args: Vec::new(),
                also_served_by: Vec::new(),
                language_ids: std::collections::BTreeMap::new(),
                extensions: vec![if language == "vue" { "vue" } else { "rs" }.to_string()],
                install: std::collections::BTreeMap::new(),
                initialization_options: None,
                partial: None,
                unanswerable: None,
            },
        );
        state
    }

    fn open(state: &mut State, path: &str, contents: &str) {
        state
            .buffers
            .insert(state.root.join(path), Buffer::open(contents, false, 4));
    }

    fn pass(state: &mut State) -> Vec<Effect> {
        let mut effects = sync(state);
        let started: Vec<String> = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::StartLsp { language, .. } => Some(language.clone()),
                _ => None,
            })
            .collect();
        if started.is_empty() {
            return effects;
        }
        state.lsp_running.extend(started);
        effects.extend(sync(state));
        effects
    }

    fn reviewing(files: &[&str]) -> State {
        let mut state = workspace("rust", "rust-analyzer");
        state.view = crate::View::Review;
        state.repo = Some(
            files
                .iter()
                .map(|path| crate::review::GitFile {
                    path: path.to_string(),
                    status: crate::review::GitStatus::Modified,
                })
                .collect(),
        );
        state.lsp_running.insert("rust".to_string());
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#,
        );
        state
    }

    fn reads(effects: &[Effect]) -> Vec<PathBuf> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::ReadForReview(path) => Some(path.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_file_being_read_for_the_review_is_not_asked_for_again() {
        let mut state = reviewing(&["src/lib.rs"]);
        assert_eq!(
            reads(&pass(&mut state)),
            vec![state.root.join("src/lib.rs")]
        );
        assert_eq!(reads(&pass(&mut state)), Vec::<PathBuf>::new());
    }

    #[test]
    fn a_read_that_comes_back_for_a_file_now_open_is_dropped() {
        let mut state = reviewing(&["src/lib.rs"]);
        pass(&mut state);
        let path = state.root.join("src/lib.rs");
        open(&mut state, "src/lib.rs", "fn main() {}");
        assert_eq!(
            read_for_review(&mut state, &path, "fn main() {}"),
            Vec::new()
        );
    }

    #[test]
    fn a_file_that_leaves_the_review_is_told_it_is_closed() {
        let mut state = reviewing(&["src/lib.rs"]);
        pass(&mut state);
        let path = state.root.join("src/lib.rs");
        read_for_review(&mut state, &path, "fn main() {}");
        state.diagnostics.entry(path.clone()).or_default().insert(
            "rust".to_string(),
            vec![Diagnostic {
                line: 1,
                column: 1,
                end_column: Some(1),
                severity: Severity::Error,
                message: "from the version that was under review".to_string(),
            }],
        );
        state.repo = Some(Vec::new());
        assert_eq!(
            methods(&pass(&mut state)),
            vec!["textDocument/didClose"],
            "the server was left believing a file nobody is reviewing is open"
        );
        assert!(
            !state.diagnostics.contains_key(&path),
            "the counts outlived the change they described"
        );
    }

    #[test]
    fn a_review_in_a_language_with_no_server_running_starts_nothing() {
        let mut state = workspace("rust", "rust-analyzer");
        state.view = crate::View::Review;
        state.repo = Some(vec![crate::review::GitFile {
            path: "src/lib.rs".to_string(),
            status: crate::review::GitStatus::Modified,
        }]);
        assert_eq!(pass(&mut state), Vec::new());
    }

    fn methods(effects: &[Effect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::LspSend { json, .. } => Some(json.clone()),
                _ => None,
            })
            .map(|json| {
                serde_json::from_str::<Value>(&json).expect("json")["method"]
                    .as_str()
                    .expect("a method")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn a_language_with_no_server_starts_nothing() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "legacy/report.cob", "DISPLAY 'HI'.");
        assert_eq!(pass(&mut state), Vec::new());
        assert!(state.lsp.is_empty(), "a conversation with nobody");
    }

    #[test]
    fn opening_starts_the_server_and_asks_for_the_handshake() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        let effects = pass(&mut state);
        assert!(matches!(
            effects.first(),
            Some(Effect::StartLsp { command, .. }) if command == "rust-analyzer"
        ));
        assert_eq!(methods(&effects), vec!["initialize"]);
        assert!(!ready(&state, "rust"));
    }

    #[test]
    fn a_server_the_edge_already_holds_is_spoken_to_rather_than_started() {
        let mut state = workspace("rust", "rust-analyzer");
        state.lsp_running.insert("rust".to_string());
        open(&mut state, "src/lib.rs", "fn main() {}");
        let effects = pass(&mut state);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::StartLsp { .. })),
            "{effects:?}"
        );
        assert_eq!(methods(&effects), vec!["initialize"]);
    }

    #[test]
    fn the_handshake_reply_is_followed_by_the_open_document() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        let mut effects = received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#,
        );
        assert!(ready(&state, "rust"));
        effects.extend(pass(&mut state));
        assert_eq!(
            methods(&effects),
            vec!["initialized", "textDocument/didOpen"]
        );
    }

    #[test]
    fn a_pass_over_a_buffer_that_has_not_moved_sends_nothing() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#,
        );
        pass(&mut state);
        assert_eq!(pass(&mut state), Vec::new());
    }

    #[test]
    fn a_second_pass_over_the_state_the_first_left_sends_nothing_twice() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#,
        );
        let first = pass(&mut state);
        assert_eq!(methods(&first), vec!["textDocument/didOpen"]);
        assert_eq!(pass(&mut state), Vec::new());
    }

    #[test]
    fn a_handshake_the_server_refused_is_a_server_that_is_gone() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        assert_eq!(
            received(
                &mut state,
                "rust",
                r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"no workspace"}}"#,
            ),
            vec![Effect::Notify("language-server-failed")]
        );
        assert!(!ready(&state, "rust"));
        assert_eq!(pass(&mut state), Vec::new());
    }

    #[test]
    fn a_request_varde_does_not_implement_is_refused_out_loud() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        let effects = received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":7,"method":"client/registerCapability","params":{}}"#,
        );
        let [Effect::LspSend { json, .. }] = effects.as_slice() else {
            panic!("nothing was said back: {effects:?}")
        };
        let reply: Value = serde_json::from_str(json).expect("json");
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["error"]["code"], -32601);
    }

    #[test]
    fn a_question_with_no_tag_in_it_is_not_answered() {
        let mut state = workspace("rust", "rust-analyzer");
        state
            .servers
            .get_mut("rust")
            .expect("configured")
            .unanswerable = Some(crate::startup::Unanswerable {
            request: "elsewhere/request".to_string(),
            response: "elsewhere/response".to_string(),
        });
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        for params in [json!({"id": 7}), json!([])] {
            let effects = received(
                &mut state,
                "rust",
                &notification("elsewhere/request", params.clone()),
            );
            assert!(effects.is_empty(), "{params} was answered: {effects:?}");
        }
    }

    #[test]
    fn a_notification_is_not_answered() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        assert_eq!(
            received(
                &mut state,
                "rust",
                r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{}}"#,
            ),
            Vec::new()
        );
    }

    #[test]
    fn a_buffer_that_closed_is_told_to_the_server() {
        let mut state = workspace("rust", "rust-analyzer");
        let path = state.root.join("src/lib.rs");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}"#,
        );
        pass(&mut state);
        state.buffers.remove(&path);
        assert_eq!(methods(&pass(&mut state)), vec!["textDocument/didClose"]);
        assert_eq!(pass(&mut state), Vec::new());
        open(&mut state, "src/lib.rs", "fn main() {}");
        assert_eq!(methods(&pass(&mut state)), vec!["textDocument/didOpen"]);
    }

    #[test]
    fn a_message_that_answers_no_request_does_not_finish_the_handshake() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        let effects = received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{}}"#,
        );
        assert_eq!(effects, Vec::new());
        assert!(!ready(&state, "rust"));
    }

    #[test]
    fn a_push_for_a_file_no_buffer_holds_is_kept() {
        let mut state = workspace("rust", "rust-analyzer");
        let path = state.root.join("src/keys.rs");
        assert_eq!(
            received(&mut state, "rust", &publish(&path, Some(4), &[(3, 1)])),
            Vec::new()
        );
        assert_eq!(mark(&state, &path, 3), Some(Severity::Error));
    }

    #[test]
    fn a_line_carrying_several_is_marked_with_the_worst() {
        let mut state = workspace("rust", "rust-analyzer");
        let path = state.root.join("src/lib.rs");
        received(&mut state, "rust", &publish(&path, None, &[(3, 4), (3, 1)]));
        assert_eq!(mark(&state, &path, 3), Some(Severity::Error));
        assert_eq!(message_at_cursor(&state), None, "no buffer, no cursor");
    }

    #[test]
    fn a_span_is_clamped_to_the_characters_the_line_holds() {
        let mut state = workspace("rust", "rust-analyzer");
        let path = state.root.join("src/lib.rs");
        open(&mut state, "src/lib.rs", "fn main() {}");
        state.diagnostics.entry(path.clone()).or_default().insert(
            "rust".to_string(),
            vec![
                Diagnostic {
                    line: 1,
                    column: 4,
                    end_column: None,
                    severity: Severity::Error,
                    message: "runs off the end of the line".to_string(),
                },
                Diagnostic {
                    line: 1,
                    column: 4,
                    end_column: Some(3),
                    severity: Severity::Warning,
                    message: "no width at all".to_string(),
                },
                Diagnostic {
                    line: 1,
                    column: 99,
                    end_column: Some(120),
                    severity: Severity::Hint,
                    message: "past the end entirely".to_string(),
                },
            ],
        );
        assert_eq!(
            underlines(&state, &path, 1),
            vec![
                (4, 12, Severity::Error),
                (4, 4, Severity::Warning),
                (12, 12, Severity::Hint),
            ]
        );
        assert_eq!(
            underlines(&state, &path, 2),
            Vec::new(),
            "a line the file does not have"
        );
    }

    #[test]
    fn nothing_is_said_about_a_line_the_editor_is_not_drawing() {
        let mut state = workspace("rust", "rust-analyzer");
        let path = state.root.join("src/lib.rs");
        open(&mut state, "src/lib.rs", "fn main() {}");
        state.current_buffer = Some(path.clone());
        state.pointed_at = Pointed::Text(crate::Place { line: 1, column: 4 });
        state.diagnostics.entry(path.clone()).or_default().insert(
            "rust".to_string(),
            vec![Diagnostic {
                line: 1,
                column: 4,
                end_column: Some(7),
                severity: Severity::Error,
                message: "cannot find value".to_string(),
            }],
        );
        assert!(pointed(&state).is_some(), "the box is not shown at all");
        state.buffers.get_mut(&path).expect("the buffer").previewing = true;
        assert_eq!(pointed(&state), None);
    }

    #[test]
    fn the_columns_of_a_pushed_range_are_the_protocols_own() {
        let mut state = workspace("rust", "rust-analyzer");
        let path = state.root.join("src/lib.rs");
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri(&path).expect("a uri"),
                "diagnostics": [
                    {
                        "range": {
                            "start": {"line": 1, "character": 12},
                            "end": {"line": 1, "character": 16},
                        },
                        "severity": 1,
                        "message": "cannot find value",
                    },
                    {
                        "range": {
                            "start": {"line": 1, "character": 12},
                            "end": {"line": 3, "character": 0},
                        },
                        "severity": 1,
                        "message": "unclosed delimiter",
                    },
                ],
            },
        })
        .to_string();
        received(&mut state, "rust", &notification);
        let held = &state.diagnostics[&path]["rust"];
        assert_eq!((held[0].column, held[0].end_column), (13, Some(16)));
        assert_eq!((held[1].column, held[1].end_column), (13, None));
    }

    fn publish(path: &Path, version: Option<i64>, lines: &[(u32, u8)]) -> String {
        let diagnostics: Vec<Value> = lines
            .iter()
            .map(|(line, severity)| {
                json!({
                    "range": {
                        "start": {"line": line - 1, "character": 0},
                        "end": {"line": line - 1, "character": 1},
                    },
                    "severity": severity,
                    "message": "something",
                })
            })
            .collect();
        let mut params = json!({
            "uri": uri(path).expect("a uri"),
            "diagnostics": diagnostics,
        });
        if let Some(version) = version {
            params["version"] = json!(version);
        }
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": params,
        })
        .to_string()
    }

    #[test]
    fn a_server_that_exited_is_not_forgotten_because_its_command_is_there() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        gone(&mut state, "rust", Gone::Exited);
        state.commands_on_path.insert("rust-analyzer".to_string());
        assert_eq!(
            pass(&mut state),
            Vec::new(),
            "the command was never the reason, so finding it is not a reason to try again"
        );
        assert!(state.lsp.contains_key("rust"));
    }

    #[test]
    fn a_requirement_nothing_met_is_said_ahead_of_the_death_it_explains() {
        let mut state = workspace("rust", "rust-analyzer");
        state.os = "macos".to_string();
        state.commands_on_path.insert("rust-analyzer".to_string());
        state.servers.get_mut("rust").expect("configured").install =
            [("macos".to_string(), "install-it".to_string())]
                .into_iter()
                .collect();
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        gone(&mut state, "rust", Gone::Exited);
        let row = &crate::tools::rows(&state)[0];
        assert_eq!(
            (&row.availability, row.install.as_deref()),
            (&crate::tools::Availability::Stopped, Some("install-it")),
            "the command is here and Varde watched it go, so the row offers the fix"
        );
        state
            .servers
            .get_mut("rust")
            .expect("configured")
            .args
            .push("--tsdk=${typescript_sdk}".to_string());
        state.facts.insert(
            "typescript_sdk".to_string(),
            crate::startup::Fact {
                marker: "node_modules/typescript/lib/typescript.js".to_string(),
                value: crate::startup::FactValue::Directory,
                command: None,
                command_marker: None,
                optional: false,
                install: Default::default(),
            },
        );
        assert_eq!(
            crate::tools::rows(&state)[0].availability,
            crate::tools::Availability::Unmet {
                needs: "typescript_sdk".to_string()
            },
            "and what it is missing is said in front of the fact that it died"
        );
    }

    #[test]
    fn a_refused_handshake_is_not_forgotten_either() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        assert!(state.lsp_running.contains("rust"), "the edge holds it");
        gone(&mut state, "rust", Gone::FailedToStart);
        state.commands_on_path.insert("rust-analyzer".to_string());
        assert_eq!(pass(&mut state), Vec::new());
        assert!(state.lsp.contains_key("rust"));
    }

    #[test]
    fn a_command_that_is_there_and_cannot_run_is_written_off_for_good() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        state.commands_on_path.insert("rust-analyzer".to_string());
        assert_eq!(
            sync(&mut state),
            vec![Effect::StartLsp {
                language: "rust".to_string(),
                command: "rust-analyzer".to_string(),
                args: Vec::new(),
            }]
        );
        gone(&mut state, "rust", Gone::FailedToStart);
        for _ in 0..3 {
            assert_eq!(
                sync(&mut state),
                Vec::new(),
                "the probe already found it, so finding it again is no news"
            );
        }
        assert!(state.lsp.contains_key("rust"));
    }

    #[test]
    fn a_command_appearing_drops_the_conversation_it_was_written_off_in() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        gone(&mut state, "rust", Gone::FailedToStart);
        assert_eq!(sync(&mut state), Vec::new(), "still nothing on PATH");
        state.commands_on_path.insert("rust-analyzer".to_string());
        assert_eq!(
            sync(&mut state),
            vec![Effect::StartLsp {
                language: "rust".to_string(),
                command: "rust-analyzer".to_string(),
                args: Vec::new(),
            }]
        );
    }

    #[test]
    fn a_server_that_exits_says_so() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        assert_eq!(
            gone(&mut state, "rust", Gone::Exited),
            vec![Effect::Notify("language-server-stopped")]
        );
    }

    #[test]
    fn one_loss_is_one_notice() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        gone(&mut state, "rust", Gone::FailedToStart);
        assert_eq!(gone(&mut state, "rust", Gone::Exited), Vec::new());
    }

    #[test]
    fn a_server_that_is_gone_is_neither_talked_to_nor_started_again() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        assert_eq!(
            gone(&mut state, "rust", Gone::FailedToStart),
            vec![Effect::Notify("language-server-failed")]
        );
        open(&mut state, "src/keys.rs", "fn keys() {}");
        assert_eq!(pass(&mut state), Vec::new());
        assert!(!ready(&state, "rust"));
    }

    #[test]
    fn a_position_is_counted_the_way_the_protocol_counts() {
        let text = "let 🦀 = crab;";
        assert_eq!(
            position(text, Place { line: 1, column: 5 }),
            Position {
                line: 0,
                character: 4
            }
        );
        assert_eq!(
            position(text, Place { line: 1, column: 7 }),
            Position {
                line: 0,
                character: 7
            }
        );
    }

    #[test]
    fn a_declined_capability_is_not_a_declared_one() {
        let declared =
            |capability: &str, said: Value| declares(&json!({capability: said}), capability);
        for capability in ["hoverProvider", "definitionProvider"] {
            assert!(declared(capability, json!(true)));
            assert!(declared(capability, json!({})));
            assert!(!declared(capability, json!(false)));
            assert!(!declares(&json!({}), capability));
        }
        let one = json!({"definitionProvider": true});
        assert!(declares(&one, "definitionProvider") && !declares(&one, "hoverProvider"));
    }

    #[test]
    fn a_question_the_server_declined_is_refused_by_name() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        state.current_buffer = Some(state.root.join("src/lib.rs"));
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true}}}"#,
        );
        assert_eq!(
            ask(&mut state, About::Definition),
            vec![Effect::Notify("no-definition-support")]
        );
        assert_eq!(outstanding(&state, "rust"), 0, "it was asked anyway");
    }

    #[test]
    fn a_column_read_back_is_the_column_that_was_sent() {
        let text = "let 🦀 = one;";
        let at = Place { line: 1, column: 7 };
        let sent = position(text, at);
        assert_eq!(sent.character, 7, "the units the server was given");
        assert_eq!(place(Some(text), sent), at);
    }

    #[test]
    fn a_column_in_a_file_not_open_is_counted_in_code_units() {
        let sent = position("let 🦀 = one;", Place { line: 1, column: 7 });
        assert_eq!(place(None, sent), Place { line: 1, column: 8 });
        let ascii = position("let one = two;", Place { line: 1, column: 7 });
        assert_eq!(place(None, ascii), Place { line: 1, column: 7 });
    }

    #[test]
    fn a_definition_list_is_drawn_under_the_symbol_it_asked_about() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() { helper() }");
        let path = state.root.join("src/lib.rs");
        state.current_buffer = Some(path.clone());
        state.buffers.get_mut(&path).expect("a buffer").column = 13;
        let reply = located(&state.root, &[("src/keys.rs", 88), ("src/tree.rs", 14)]);
        let effects = acted(jumped(
            &mut state,
            Ask {
                path,
                place: Place {
                    line: 1,
                    column: 13,
                },
                revision: 1,
                about: About::Definition,
            },
            &reply,
        ));
        assert!(effects.is_empty(), "something was opened: {effects:?}");
        assert_eq!(
            state.search.expect("the search list").query.shown(),
            "helper"
        );
    }

    #[test]
    fn a_reply_straddling_the_root_keeps_what_is_inside_it() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        let path = state.root.join("src/lib.rs");
        state.current_buffer = Some(path.clone());
        let ask = Ask {
            path: path.clone(),
            place: Place { line: 1, column: 1 },
            revision: 1,
            about: About::Definition,
        };
        let outside = "/home/me/.cargo/\u{1b}[2Jserde/src/lib.rs";
        let mut both = located(&state.root, &[("src/keys.rs", 88)])
            .as_array()
            .expect("an array")
            .clone();
        both.push(location(Path::new(outside), 4210));
        assert!(matches!(
            acted(jumped(&mut state, ask.clone(), &Value::Array(both)))[..],
            [Effect::OpenAt { .. }]
        ));
        let nowhere = Value::Array(vec![location(Path::new(outside), 4210)]);
        assert_eq!(
            acted(jumped(&mut state, ask, &nowhere)),
            vec![Effect::NotifyAbout {
                slug: "definition-outside-workspace",
                about: "/home/me/.cargo/[2Jserde/src/lib.rs".to_string(),
            }]
        );
    }

    fn acted(told: Told) -> Vec<Effect> {
        match told {
            Told::Something(effects) => effects,
            Told::Nothing => Vec::new(),
        }
    }

    fn located(root: &Path, places: &[(&str, u32)]) -> Value {
        Value::Array(
            places
                .iter()
                .map(|(path, line)| location(&root.join(path), *line))
                .collect(),
        )
    }

    fn location(path: &Path, line: u32) -> Value {
        let at = json!({"line": line - 1, "character": 0});
        json!({
            "uri": Url::from_file_path(path).expect("a file url"),
            "range": {"start": at, "end": at},
        })
    }

    #[test]
    fn the_box_sits_beside_the_line_it_describes_never_on_it() {
        let boxed = |line: usize| {
            let mut state = State {
                hover: Some(Hover {
                    lines: says(
                        &json!({"contents": {"kind": "plaintext", "value": "one\ntwo"}}),
                        40,
                    )
                    .expect("two rows"),
                    from: 0,
                    asked: Ask {
                        path: PathBuf::from("/w/one.rs"),
                        place: Place { line, column: 1 },
                        revision: 1,
                        about: About::Hover,
                    },
                    first: 0,
                    focused: false,
                    value: None,
                }),
                ..State::default()
            };
            settle_box(&mut state, 20);
            state
        };
        let boxed_at = |state: &State| state.hover.as_ref().expect("a box").from;
        let below = boxed(6);
        assert_eq!(boxed_at(&below), 7);
        assert!(!covers(&below, 6) && covers(&below, 10));
        let above = boxed(19);
        assert_eq!(boxed_at(&above), 15);
        assert!(!covers(&above, 19) && covers(&above, 18));
    }

    fn answering(path: &str, contents: &str) -> State {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, path, contents);
        state.current_buffer = Some(state.root.join(path));
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true,"completionProvider":{}}}}"#,
        );
        pass(&mut state);
        state
    }

    #[test]
    fn typing_a_name_arms_the_debounce_and_the_document_pass_never_does() {
        let mut state = answering("src/lib.rs", "");
        let path = state.root.join("src/lib.rs");
        let buffer = state.buffers.get_mut(&path).expect("the buffer");
        buffer.key('i');
        buffer.key('w');
        buffer.key('o');
        assert_eq!(typed(&mut state), vec![Effect::DebounceCandidates(120)]);
        state
            .buffers
            .get_mut(&path)
            .expect("the buffer")
            .backspace();
        assert_eq!(typed(&mut state), vec![Effect::DebounceCandidates(120)]);
        assert_eq!(
            pass(&mut state)
                .iter()
                .filter(|effect| matches!(effect, Effect::DebounceCandidates(_)))
                .count(),
            0
        );
    }

    #[test]
    fn accepting_a_candidate_does_not_ask_for_another_list() {
        let mut state = answering("src/lib.rs", "");
        let path = state.root.join("src/lib.rs");
        state = crate::update(&state, crate::Event::EditorKey('i')).0;
        state = crate::update(&state, crate::Event::EditorKey('w')).0;
        state.modal = crate::Modal::Candidates(Candidates::offering(
            &state,
            vec![Candidate {
                label: "workspace_root".to_string(),
                insert: "workspace_root".to_string(),
                filter: None,
                sort: None,
                snippet: false,
            }],
            Ask {
                path: state.root.join("src/lib.rs"),
                place: Place { line: 1, column: 2 },
                revision: 1,
                about: About::Candidates,
            },
        ));
        let (accepted, effects) = crate::update(&state, crate::Event::AcceptCandidate);
        assert_eq!(accepted.buffers[&path].shown(), "workspace_root");
        assert_eq!(accepted.modal, crate::Modal::None);
        assert_eq!(methods(&effects), vec!["textDocument/didChange"]);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::DebounceCandidates(_))),
            "accepting asked for a second list: {effects:?}"
        );
    }

    #[test]
    fn a_key_that_ends_the_word_takes_the_list_with_it() {
        let mut state = answering("src/lib.rs", "");
        let buffer = state
            .buffers
            .get_mut(&state.root.join("src/lib.rs"))
            .expect("the buffer");
        buffer.key('i');
        buffer.key('w');
        buffer.key(' ');
        state.modal = crate::Modal::Candidates(Candidates::offering(
            &state,
            vec![Candidate {
                label: "workspace_root".to_string(),
                insert: "workspace_root".to_string(),
                filter: None,
                sort: None,
                snippet: false,
            }],
            Ask {
                path: state.root.join("src/lib.rs"),
                place: Place { line: 1, column: 2 },
                revision: 1,
                about: About::Candidates,
            },
        ));
        assert_eq!(typed(&mut state), Vec::new());
        assert_eq!(state.modal, crate::Modal::None);
    }

    #[test]
    fn a_server_that_offers_no_completion_is_never_waited_for() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "");
        state.current_buffer = Some(state.root.join("src/lib.rs"));
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true}}}"#,
        );
        let buffer = state
            .buffers
            .get_mut(&state.root.join("src/lib.rs"))
            .expect("the buffer");
        buffer.key('i');
        buffer.key('w');
        assert_eq!(typed(&mut state), Vec::new());
    }

    #[test]
    fn a_question_nobody_asked_for_is_refused_in_silence() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "legacy/report.cob", "DISPLAY 'HI'.");
        state.current_buffer = Some(state.root.join("legacy/report.cob"));
        assert_eq!(ask(&mut state, About::Candidates), Vec::new());
        assert_eq!(
            ask(&mut state, About::Hover),
            vec![Effect::Notify("no-language-server")]
        );
    }

    #[test]
    fn a_server_that_declares_no_completion_is_asked_for_none() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        state.current_buffer = Some(state.root.join("src/lib.rs"));
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true}}}"#,
        );
        assert_eq!(ask(&mut state, About::Candidates), Vec::new());
        assert_eq!(outstanding(&state, "rust"), 0);
    }

    #[test]
    fn a_reply_never_takes_the_screen_off_a_modal_somebody_opened() {
        let mut state = answering("src/lib.rs", "");
        state.modal = crate::Modal::Palette;
        let ask = Ask {
            path: state.root.join("src/lib.rs"),
            place: Place { line: 1, column: 1 },
            revision: 1,
            about: About::Candidates,
        };
        offered(&mut state, ask, &json!([{"label": "workspace_root"}]));
        assert_eq!(state.modal, crate::Modal::Palette);
    }

    #[test]
    fn a_candidate_inserts_what_the_server_said_and_the_label_when_it_said_nothing() {
        let offered = candidates(&json!([
            {"label": "workspace_root"},
            {"label": "push(…)", "insertText": "push"},
        ]));
        assert_eq!(
            offered,
            vec![
                Candidate {
                    label: "workspace_root".to_string(),
                    insert: "workspace_root".to_string(),
                    filter: None,
                    sort: None,
                    snippet: false,
                },
                Candidate {
                    label: "push(…)".to_string(),
                    insert: "push".to_string(),
                    filter: None,
                    sort: None,
                    snippet: false,
                },
            ]
        );
        assert_eq!(candidates(&json!({"isIncomplete": false, "items": []})), []);
        assert_eq!(candidates(&Value::Null), []);
    }

    #[test]
    fn the_wrapped_box_fits_the_screen_it_is_drawn_on() {
        let long =
            "pub fn a_very_long_signature(one: usize, two: usize) -> Result<Vec<String>, Error>";
        for width in [24, 40, 100, 220] {
            let lines = says(&json!({"contents": long}), measure(width)).expect("a reply");
            let widest = lines
                .iter()
                .map(|row| row.text().chars().count())
                .max()
                .unwrap_or(0);
            assert!(
                widest + 2 <= width,
                "a {width}-column screen cannot draw {widest} of text"
            );
        }
    }

    #[test]
    fn a_refused_question_is_not_an_answer() {
        let mut state = asking();
        let refusal =
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32801,"message":"content modified"}}"#;
        assert_eq!(
            received(&mut state, "rust", refusal),
            vec![Effect::Notify("language-server-error")]
        );
        assert_eq!(state.hover, None);
        assert_eq!(outstanding(&state, "rust"), 0);
    }

    #[test]
    fn a_server_that_stops_takes_its_box_with_it() {
        let mut state = asking();
        received(&mut state, "rust", &reply(json!({"contents": "fn main()"})));
        assert!(state.hover.is_some(), "no box to lose");
        gone(&mut state, "rust", Gone::Exited);
        assert_eq!(state.hover, None);
    }

    #[test]
    fn a_box_stands_only_while_it_describes_what_is_under_the_cursor() {
        let mut state = asking();
        received(&mut state, "rust", &reply(json!({"contents": "fn main()"})));
        assert!(hover_stands(&state));
        let path = state.current_buffer.clone().expect("a buffer");
        state.buffers.get_mut(&path).expect("a buffer").column += 1;
        assert!(!hover_stands(&state), "it stood after the cursor moved");
    }

    fn asking() -> State {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        state.current_buffer = Some(state.root.join("src/lib.rs"));
        pass(&mut state);
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true}}}"#,
        );
        pass(&mut state);
        assert_eq!(ask(&mut state, About::Hover).len(), 1, "nothing was asked");
        state
    }

    fn served_twice() -> State {
        let mut state = workspace("vue", "vue-language-server");
        let second = state.servers.get("vue").expect("the vue server").clone();
        state.servers.insert(
            "typescript".to_string(),
            Server {
                extensions: Vec::new(),
                ..second
            },
        );
        state
            .servers
            .get_mut("vue")
            .expect("the vue server")
            .also_served_by = vec!["typescript".to_string()];
        open(&mut state, "src/App.vue", "const total = addNumbers(1, 2)");
        state.current_buffer = Some(state.root.join("src/App.vue"));
        pass(&mut state);
        for language in ["vue", "typescript"] {
            received(
                &mut state,
                language,
                r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true,"definitionProvider":true}}}"#,
            );
        }
        pass(&mut state);
        state
    }

    fn said(hover: &Hover) -> String {
        hover
            .lines
            .iter()
            .map(preview::Row::text)
            .collect::<Vec<String>>()
            .join(" ")
    }

    fn reply(result: Value) -> String {
        json!({"jsonrpc": "2.0", "id": INITIALIZE + 1, "result": result}).to_string()
    }

    #[test]
    fn one_question_is_put_to_everybody_who_serves_the_file() {
        let mut state = served_twice();
        let asked: Vec<String> = ask(&mut state, About::Definition)
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::LspSend { language, .. } => Some(language),
                _ => None,
            })
            .collect();
        assert_eq!(asked, vec!["vue".to_string(), "typescript".to_string()]);
        assert_eq!(outstanding(&state, "vue"), 1);
        assert_eq!(outstanding(&state, "typescript"), 1);
        for language in ["vue", "typescript"] {
            assert!(
                state.lsp[language]
                    .sent
                    .contains_key(&state.root.join("src/App.vue")),
                "{language} was never told the document is open"
            );
        }
    }

    #[test]
    fn the_first_answer_wins_and_every_later_one_is_dropped() {
        let mut state = served_twice();
        ask(&mut state, About::Hover);
        received(&mut state, "vue", &reply(json!({"contents": "the first"})));
        received(
            &mut state,
            "typescript",
            &reply(json!({"contents": "the second"})),
        );
        assert_eq!(
            state.hover.as_ref().map(said),
            Some("the first".to_string())
        );
        assert_eq!(outstanding(&state, "typescript"), 0);
    }

    #[test]
    fn nobody_is_told_nothing_until_everybody_has_answered() {
        let mut state = served_twice();
        ask(&mut state, About::Definition);
        assert_eq!(
            received(&mut state, "vue", &reply(Value::Null)),
            Vec::new(),
            "the empty-handed notice beat the server with an answer"
        );
        let elsewhere = json!({
            "uri": format!("file://{}/src/helper.ts", state.root.display()),
            "range": {"start": {"line": 1, "character": 16}, "end": {"line": 1, "character": 26}},
        });
        assert_eq!(
            received(&mut state, "typescript", &reply(elsewhere)),
            vec![Effect::OpenAt {
                path: state.root.join("src/helper.ts"),
                at: Place {
                    line: 2,
                    column: 17
                },
            }]
        );
    }

    #[test]
    fn the_empty_handed_notice_is_given_when_the_last_server_has_answered() {
        let mut state = served_twice();
        ask(&mut state, About::Definition);
        assert_eq!(received(&mut state, "typescript", &reply(Value::Null)), []);
        assert_eq!(
            received(&mut state, "vue", &reply(Value::Null)),
            vec![Effect::Notify("no-definition")]
        );
        assert_eq!(outstanding(&state, "vue"), 0);
    }

    #[test]
    fn an_error_does_not_beat_an_answer() {
        let mut state = served_twice();
        ask(&mut state, About::Hover);
        let refused =
            json!({"jsonrpc": "2.0", "id": INITIALIZE + 1, "error": {"code": -1, "message": "no"}})
                .to_string();
        assert_eq!(received(&mut state, "typescript", &refused), []);
        received(&mut state, "vue", &reply(json!({"contents": "a type"})));
        assert!(state.hover.is_some(), "the refusal answered for the pair");
    }

    #[test]
    fn a_server_dying_mid_question_does_not_leave_it_outstanding() {
        let mut state = served_twice();
        ask(&mut state, About::Definition);
        received(&mut state, "vue", &reply(Value::Null));
        assert_eq!(
            gone(&mut state, "typescript", Gone::Exited),
            vec![Effect::Notify("language-server-stopped")]
        );
        assert_eq!(outstanding(&state, "typescript"), 0);
        assert!(state.lsp_asked.is_empty(), "the question is still waiting");
    }

    fn relaying_server() -> State {
        let mut state = workspace("vue", "vue-language-server");
        state
            .servers
            .get_mut("vue")
            .expect("the vue server")
            .unanswerable = Some(crate::startup::Unanswerable {
            request: "tsserver/request".to_string(),
            response: "tsserver/response".to_string(),
        });
        open(&mut state, "src/App.vue", "const total = addNumbers(1, 2)");
        state.current_buffer = Some(state.root.join("src/App.vue"));
        pass(&mut state);
        received(
            &mut state,
            "vue",
            r#"{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true,"definitionProvider":true}}}"#,
        );
        pass(&mut state);
        state
    }

    #[test]
    fn a_question_varde_declined_to_relay_says_so_rather_than_blaming_the_server() {
        for about in [About::Hover, About::Definition] {
            let mut state = relaying_server();
            received(
                &mut state,
                "vue",
                &json!({
                    "jsonrpc": "2.0",
                    "method": "tsserver/request",
                    "params": [[1, "_vue:projectInfo", {"file": "src/App.vue"}]],
                })
                .to_string(),
            );
            ask(&mut state, about);
            assert_eq!(
                received(&mut state, "vue", &reply(Value::Null)),
                vec![Effect::Notify("needs-a-companion")],
                "{about:?} blamed the server for Varde's refusal"
            );
        }
    }

    #[test]
    fn a_server_that_could_relay_and_did_not_knows_nothing_rather_than_needing_anybody() {
        let mut state = relaying_server();
        ask(&mut state, About::Definition);
        assert_eq!(
            received(&mut state, "vue", &reply(Value::Null)),
            vec![Effect::Notify("no-definition")]
        );
    }

    #[test]
    fn an_error_is_still_reported_when_somebody_else_has_the_last_word() {
        let mut state = served_twice();
        ask(&mut state, About::Definition);
        let refused =
            json!({"jsonrpc": "2.0", "id": INITIALIZE + 1, "error": {"code": -1, "message": "no"}})
                .to_string();
        assert_eq!(received(&mut state, "typescript", &refused), []);
        assert_eq!(
            received(&mut state, "vue", &reply(Value::Null)),
            vec![Effect::Notify("language-server-error")]
        );
    }

    #[test]
    fn a_language_naming_a_server_nothing_configures_is_served_by_itself_alone() {
        let mut state = served_twice();
        assert_eq!(
            served_by(&state, &state.root.join("src/App.vue")),
            vec!["vue".to_string(), "typescript".to_string()]
        );
        state.servers.remove("typescript");
        assert_eq!(
            served_by(&state, &state.root.join("src/App.vue")),
            vec!["vue".to_string()]
        );
        assert_eq!(
            served_by(&state, &state.root.join("README.md")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_long_reply_is_broken_into_lines_that_fit() {
        let plain = |value: &str| json!({"contents": {"kind": "plaintext", "value": value}});
        let lines = says(&plain("one two three four five"), 10).expect("a reply");
        assert!(
            lines.iter().all(|row| row.text().chars().count() <= 10),
            "{lines:?}"
        );
        assert_eq!(
            lines
                .iter()
                .map(preview::Row::text)
                .collect::<Vec<String>>()
                .join(" "),
            "one two three four five"
        );
        assert_eq!(says(&Value::Null, 40), None);
        assert_eq!(says(&plain("  "), 40), None);
    }

    #[test]
    fn a_reply_that_arrives_as_a_list_is_read_piece_by_piece() {
        let rows = says(
            &json!({"contents": [
                {"language": "rust", "value": "fn main()"},
                "the **entry** point",
            ]}),
            40,
        )
        .expect("a reply");
        assert_eq!(
            rows.iter().map(|row| row.kind).collect::<Vec<_>>(),
            vec![preview::RowKind::Code, preview::RowKind::Paragraph]
        );
        assert_eq!(rows[1].text(), "the entry point");
    }

    #[test]
    fn a_snippet_is_the_text_it_inserts_and_the_stops_left_in_it() {
        assert_eq!(
            snippet(r#"println!("${1:msg}")$0"#),
            ("println!(\"msg\")".to_string(), vec![10, 15])
        );
    }

    #[test]
    fn a_stop_with_no_default_leaves_no_text_behind() {
        assert_eq!(
            snippet("foo($1, $2)$0"),
            ("foo(, )".to_string(), vec![4, 6, 7])
        );
    }

    #[test]
    fn the_stops_are_visited_in_the_order_they_appear_in_the_text() {
        assert_eq!(
            snippet("${2:b} ${1:a}$0"),
            ("b a".to_string(), vec![0, 2, 3])
        );
    }

    #[test]
    fn a_snippet_naming_no_final_stop_ends_at_one() {
        assert_eq!(snippet("foo($1)"), ("foo()".to_string(), vec![4, 5]));
        assert_eq!(snippet("worklist"), ("worklist".to_string(), vec![8]));
    }

    #[test]
    fn a_placeholder_form_varde_does_not_understand_is_text() {
        assert_eq!(
            snippet("${1|a,b|} $TM_FILENAME"),
            ("${1|a,b|} $TM_FILENAME".to_string(), vec![22])
        );
    }

    #[test]
    fn a_placeholder_that_is_never_closed_is_text() {
        assert_eq!(snippet("${1:oops"), ("${1:oops".to_string(), vec![8]));
        assert_eq!(snippet("${1"), ("${1".to_string(), vec![3]));
    }

    #[test]
    fn an_escaped_dollar_is_a_dollar_and_not_a_stop() {
        assert_eq!(
            snippet(r"cost \$${1:5}$0"),
            ("cost $5".to_string(), vec![6, 7])
        );
        assert_eq!(snippet(r"path \\$1$0"), (r"path \".to_string(), vec![6, 6]));
    }

    fn triggering(contents: &str, declared: &str) -> State {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", contents);
        state.current_buffer = Some(state.root.join("src/lib.rs"));
        pass(&mut state);
        received(
            &mut state,
            "rust",
            &format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"capabilities":{{"documentOnTypeFormattingProvider":{declared}}}}}}}"#
            ),
        );
        pass(&mut state);
        state
    }

    fn params(effects: &[Effect], method: &str) -> Value {
        let sent: Vec<Value> = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::LspSend { json, .. } => serde_json::from_str::<Value>(json).ok(),
                _ => None,
            })
            .filter(|message| message["method"] == method)
            .collect();
        assert_eq!(sent.len(), 1, "{method} went out {} times", sent.len());
        sent[0]["params"].clone()
    }

    fn typing_at(state: &mut State, line: usize, column: usize) {
        let path = state.current_buffer.clone().expect("a buffer");
        let buffer = state.buffers.get_mut(&path).expect("the buffer");
        buffer.key('i');
        buffer.go_to_place(Place { line, column });
    }

    #[test]
    fn the_characters_that_ask_are_the_servers_own() {
        assert_eq!(
            triggers(&json!({"firstTriggerCharacter": ";", "moreTriggerCharacter": ["}", "\n"]})),
            vec![';', '}', '\n']
        );
        assert_eq!(
            triggers(&json!({"firstTriggerCharacter": "\n"})),
            vec!['\n']
        );
    }

    #[test]
    fn a_capability_that_names_no_character_names_none() {
        assert_eq!(triggers(&json!(true)), Vec::<char>::new());
        assert_eq!(
            triggers(&json!({"firstTriggerCharacter": ";", "moreTriggerCharacter": ["", ">="]})),
            vec![';']
        );
    }

    #[test]
    fn the_server_hears_the_text_before_it_is_asked_to_format_it() {
        let mut state = triggering(
            "fn main() {\n    let x = 1;\n        ",
            r#"{"firstTriggerCharacter": "}"}"#,
        );
        typing_at(&mut state, 3, 9);
        let (_, effects) = crate::update(&state, crate::Event::EditorKey('}'));
        assert_eq!(
            methods(&effects),
            vec!["textDocument/didChange", "textDocument/onTypeFormatting"]
        );
    }

    #[test]
    fn the_request_names_the_character_the_place_and_the_configured_width() {
        let mut state = triggering(
            "fn main() {\n    let x = 1;\n        ",
            r#"{"firstTriggerCharacter": "}"}"#,
        );
        typing_at(&mut state, 3, 9);
        let (_, effects) = crate::update(&state, crate::Event::EditorKey('}'));
        assert_eq!(
            params(&effects, "textDocument/onTypeFormatting"),
            json!({
                "textDocument": {"uri": "file:///home/me/project/src/lib.rs"},
                "position": {"line": 2, "character": 9},
                "ch": "}",
                "options": {"tabSize": 4, "insertSpaces": true},
            })
        );
    }

    #[test]
    fn the_width_is_the_projects_and_not_a_constant() {
        let mut state = triggering(
            "fn main() {\n  let x = 1;\n    ",
            r#"{"firstTriggerCharacter": "}"}"#,
        );
        state.tab_width = 2;
        typing_at(&mut state, 3, 5);
        let (_, effects) = crate::update(&state, crate::Event::EditorKey('}'));
        assert_eq!(
            params(&effects, "textDocument/onTypeFormatting")["options"],
            json!({"tabSize": 2, "insertSpaces": true})
        );
    }

    #[test]
    fn a_newline_is_a_character_a_server_can_ask_about() {
        let mut state = triggering(
            "fn main() {\n    let x = 1;",
            r#"{"firstTriggerCharacter": "\n"}"#,
        );
        typing_at(&mut state, 2, 15);
        let (_, effects) = crate::update(&state, crate::Event::EditorKey('\n'));
        assert_eq!(
            methods(&effects),
            vec!["textDocument/didChange", "textDocument/onTypeFormatting"]
        );
        let asked = params(&effects, "textDocument/onTypeFormatting");
        assert_eq!(asked["ch"], "\n");
        assert_eq!(asked["position"], json!({"line": 2, "character": 4}));
    }

    #[test]
    fn a_character_the_server_did_not_name_asks_nothing() {
        let mut state = triggering(
            "fn main() {\n    let x = 1;\n        ",
            r#"{"firstTriggerCharacter": ";"}"#,
        );
        typing_at(&mut state, 3, 9);
        let (_, effects) = crate::update(&state, crate::Event::EditorKey('}'));
        assert_eq!(methods(&effects), vec!["textDocument/didChange"]);
    }

    #[test]
    fn only_the_server_that_named_the_character_is_asked() {
        let mut state = workspace("vue", "vue-language-server");
        let second = state.servers.get("vue").expect("the vue server").clone();
        state.servers.insert(
            "typescript".to_string(),
            Server {
                extensions: Vec::new(),
                ..second
            },
        );
        state
            .servers
            .get_mut("vue")
            .expect("the vue server")
            .also_served_by = vec!["typescript".to_string()];
        open(&mut state, "src/App.vue", "const total = 1");
        state.current_buffer = Some(state.root.join("src/App.vue"));
        pass(&mut state);
        for (language, trigger) in [("vue", "}"), ("typescript", ".")] {
            received(
                &mut state,
                language,
                &format!(
                    r#"{{"jsonrpc":"2.0","id":1,"result":{{"capabilities":{{"documentOnTypeFormattingProvider":{{"firstTriggerCharacter":"{trigger}"}}}}}}}}"#
                ),
            );
        }
        pass(&mut state);
        typing_at(&mut state, 1, 16);
        let (_, effects) = crate::update(&state, crate::Event::EditorKey('}'));
        let asked: Vec<String> = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::LspSend { language, json } => {
                    let message: Value = serde_json::from_str(json).expect("json");
                    (message["method"] == "textDocument/onTypeFormatting").then(|| language.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(asked, vec!["vue".to_string()]);
    }

    #[test]
    fn the_edits_a_server_sends_back_are_applied() {
        let mut state = triggering(
            "fn main() {\n    let x = 1;\n        ",
            r#"{"firstTriggerCharacter": "}"}"#,
        );
        typing_at(&mut state, 3, 9);
        state = crate::update(&state, crate::Event::EditorKey('}')).0;
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":2,"result":[{"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":8}},"newText":""}]}"#,
        );
        assert_eq!(
            state.buffers[&state.root.join("src/lib.rs")].shown(),
            "fn main() {\n    let x = 1;\n}"
        );
    }

    #[test]
    fn a_reply_about_text_the_reader_has_typed_on_is_dropped() {
        let mut state = triggering(
            "fn main() {\n    let x = 1;\n        ",
            r#"{"firstTriggerCharacter": "}"}"#,
        );
        typing_at(&mut state, 3, 9);
        state = crate::update(&state, crate::Event::EditorKey('}')).0;
        state = crate::update(&state, crate::Event::EditorKey('x')).0;
        state = crate::update(&state, crate::Event::EditorBackspace).0;
        let path = state.root.join("src/lib.rs");
        assert_eq!(
            (state.buffers[&path].line, state.buffers[&path].column),
            (3, 10),
            "the cursor is not back where the question was asked"
        );
        received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":2,"result":[{"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":8}},"newText":""}]}"#,
        );
        assert_eq!(
            state.buffers[&path].shown(),
            "fn main() {\n    let x = 1;\n        }"
        );
    }

    #[test]
    fn a_server_that_will_not_format_says_nothing() {
        let mut state = triggering(
            "fn main() {\n    let x = 1;\n        ",
            r#"{"firstTriggerCharacter": "}"}"#,
        );
        typing_at(&mut state, 3, 9);
        state = crate::update(&state, crate::Event::EditorKey('}')).0;
        let answered = received(
            &mut state,
            "rust",
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32603,"message":"no formatter"}}"#,
        );
        assert_eq!(answered, Vec::new());
        assert_eq!(
            state.buffers[&state.root.join("src/lib.rs")].shown(),
            "fn main() {\n    let x = 1;\n        }"
        );
    }

    #[test]
    fn typing_where_the_server_offers_no_formatting_asks_nothing() {
        let mut state = answering("src/lib.rs", "fn main() {\n    let x = 1;\n        ");
        typing_at(&mut state, 3, 9);
        let (state, effects) = crate::update(&state, crate::Event::EditorKey('}'));
        assert_eq!(methods(&effects), vec!["textDocument/didChange"]);
        assert_eq!(
            state.buffers[&state.root.join("src/lib.rs")].shown(),
            "fn main() {\n    let x = 1;\n        }"
        );
    }

    #[test]
    fn a_diagnostic_row_says_where_and_the_first_line_stripped() {
        let diagnostic = Diagnostic {
            line: 3,
            column: 7,
            end_column: None,
            severity: Severity::Error,
            message: "\u{1b}[2Jbad\u{7} thing\nsecond line".to_string(),
        };
        assert_eq!(row_text(&diagnostic), "3:7 [2Jbad thing");
    }

    #[test]
    fn the_severity_labels_shed_their_names_before_they_overflow() {
        let mut state = workspace("rust", "rust-analyzer");
        let root = state.root.clone();
        let publish = |file: &str, severity: u8| {
            json!({
                "uri": format!("file://{}", root.join(file).display()),
                "diagnostics": [{
                    "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}},
                    "severity": severity,
                    "message": "m",
                }],
            })
        };
        published(&mut state, "rust", publish("a.rs", 1));
        published(&mut state, "rust", publish("b.rs", 4));
        assert_eq!(opening(&state), Severity::Error);
        assert_eq!(
            severity_labels(&state, 60),
            [" Errors 1 ", " Warnings 0 ", " Info 0 ", " Hints 1 "]
        );
        assert_eq!(
            severity_labels(&state, 30),
            [" e 1 ", " w 0 ", " i 0 ", " h 1 "]
        );
        assert_eq!(
            nudge(&state),
            [(Severity::Error, "  \u{2716} 1".to_string())]
        );
    }
}
