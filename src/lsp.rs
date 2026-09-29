//! F31 — the transport: what a language server is told, and what the core makes
//! of what it says back.
//!
//! Nothing here spawns anything or reads a socket. The core decides what to say
//! and returns it as [`Effect::LspSend`]; the edge frames it over a child's
//! stdio and hands each message back as [`crate::Event::LspReceived`]. Whether a
//! process exists at all is never decided here —
//! `docs/adr/0011-a-language-server-is-a-second-hosted-child.md` argues why that
//! fact is the edge's alone, and `State::lsp_running` is where the edge puts it.

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

/// The id `initialize` is sent under, and the id every other request counts up
/// from. The handshake's is a constant because it is the one request sent
/// before there is a conversation to count in.
const INITIALIZE: i64 = 1;

/// How long typing must pause before the server is asked what may follow it.
/// The number lives here rather than at the edge, which holds the timer and
/// decides nothing: a delay chosen in `main.rs` is a delay no test can see.
/// Long enough that a typed word is asked about once rather than per letter,
/// short enough that a reader who stopped is not waiting for the list.
const DEBOUNCE_MS: u64 = 120;

/// How long the pointer must rest on a symbol before the server is asked what
/// it is. Here rather than at the edge for the reason the debounce's window is:
/// a delay chosen in `main.rs` is a delay no scenario can see. Half a second,
/// which is long enough that a pointer crossing the pane on its way somewhere
/// else asks nothing, and short enough that resting on purpose does not feel
/// like waiting.
pub const DWELL_MS: u64 = 500;

/// The formatting options a formatting request carries, because the protocol
/// requires them. The width is the project's own — `editor.tab_width`, what Tab
/// lays down and what opening a block falls back to — so the one place Varde
/// has an answer about this workspace's indentation is the one place the answer
/// comes from. Measuring the *file* was tried and is worse than either, because
/// the shallowest indentation in a Java or C file is the single space of a
/// block comment's ` * ` continuation, which would describe a four-space file
/// to its own formatter as a one-space one. Most servers measured format by
/// their own configuration (`rustfmt`, `.clang-format`, a formatter profile)
/// and read none of this; the ones that do now hear what the project said
/// rather than a constant that could disagree with it.
fn formatting(state: &State) -> FormattingOptions {
    FormattingOptions {
        tab_size: state.tab_width as u32,
        insert_spaces: true,
        ..FormattingOptions::default()
    }
}

/// Which language a file is: the `[lsp.*]` row claiming its extension, whose
/// table name is the language id the protocol is sent (ADR 0018). The rows are
/// the only mapping — start refuses two claiming one extension — so a language
/// Varde never named is served the moment a row claims its files, and a file
/// no row claims has no server, which is a value the core holds rather than a
/// silence it infers.
///
/// Extensions, not grammar names: syntect calls `.js` "JavaScript (Babel)" and
/// `.h` "Objective-C", neither of which is a language id.
pub fn language<'a>(state: &'a State, path: &Path) -> Option<&'a str> {
    let extension = path.extension()?.to_str()?;
    state
        .servers
        .iter()
        .find(|(_, server)| server.extensions.iter().any(|claim| claim == extension))
        .map(|(language, _)| language.as_str())
}

/// Where one language's conversation has got to. Not a claim that a process
/// exists: that is `State::lsp_running`, and only the edge writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handshake {
    /// `initialize` has gone; nothing else may until the reply arrives.
    Sent,
    Ready,
    /// The server is not coming back — its command was not found, or the child
    /// exited. Nothing more is sent and nothing is started in its place: a
    /// respawn against a server that dies on startup is a loop, and no scenario
    /// asks for one.
    Gone,
}

/// One language's conversation, as the core holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub handshake: Handshake,
    /// The Document version last sent for each path — what makes a second pass
    /// over the same buffer send nothing, and what tells `didOpen` from
    /// `didChange`.
    sent: BTreeMap<PathBuf, u64>,
    /// What the server said it can do, as it said it — the `capabilities`
    /// object off its own initialize reply, held rather than deserialised.
    /// `lsp_types`' `ServerCapabilities` is not `Eq` and [`State`] is, and a
    /// bool per capability made a third capability four edits: this is one, in
    /// [`About::asking`]. Reading what the server said about itself is not
    /// naming it — the distinction ADR 0011 draws. Only [`declares`] reads it,
    /// and only ever the one field asked about.
    capabilities: Value,
    /// Every file under review whose contents have been asked of the edge and
    /// not come back yet. Without it every pass would ask for the same file
    /// again while the first read was still in flight, and the server would be
    /// told the same document is open several times over.
    reading: BTreeSet<PathBuf>,
    /// The id the next request goes out under. Counted per conversation, since
    /// an id only has to be unique to the server it was sent to.
    next_id: i64,
    /// Whether Varde has refused a question this server asked — the
    /// `unanswerable` one its configuration names. A fact about the
    /// *conversation* rather than about one request, because that is how it
    /// happens: measured on the wire, `@vue/language-server` puts its
    /// `_vue:projectInfo` question once, on `didOpen`, is refused, and from
    /// then on answers `[]` to every definition and hover in the file. So the
    /// refusal precedes the keystroke it explains, and a window around the
    /// question would never see it.
    refused_its_question: bool,
    /// The command this language was written off for not having, and the only
    /// reason to forget the write-off (R31.24). `None` on every conversation
    /// but a [`Handshake::Gone`] one that was written off for a *missing*
    /// command: a server that started and then died was not absent, so a probe
    /// finding its command explains nothing and respawning it is the loop the
    /// write-off exists to stop. The command rather than a flag, so a
    /// configuration that renames it cannot be forgotten on the strength of the
    /// old one appearing.
    absent: Option<String>,
}

impl Conversation {
    /// A conversation at a given point, with nothing asked and nothing sent.
    /// Both construction sites go through here so a field added below cannot
    /// be forgotten at one of them.
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

/// A request sent and not yet answered: which file it asked about, where the
/// cursor was when it was asked, and what the document held. Both of the last
/// two are staleness rules, and which one applies is the question's — see
/// `Ask::current` below. A reply describing a symbol the reader has moved away
/// from, drawn as current, names the wrong code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub path: PathBuf,
    pub place: Place,
    /// The Buffer revision the question was asked at, which is the Document
    /// version the server was told. For a reply that *changes* the buffer this
    /// is the only rule that holds: a reader who typed and deleted is back
    /// where they were with different text, and a place cannot see that.
    pub revision: u64,
    pub about: About,
}

/// Which question a request asked. A reply carries an id and nothing else: the
/// method went out with the request, so once it has gone the id is all there is
/// to tell a hover's answer from a definition's, and [`answered`] would have to
/// guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum About {
    Hover,
    Definition,
    /// The identifiers that may follow what is being typed. The one question
    /// nobody presses a key for, which is why its refusal is silent below.
    Candidates,
    /// How the text around the cursor should be laid out now that this
    /// character has been typed — the character itself, because the request
    /// carries it and because the server names which ones it wants to hear
    /// about. The other question nobody presses a key for.
    ///
    /// Two characters typed in quick succession are two questions rather than
    /// one replacing the other, which the key being the character is what
    /// makes true. They cannot both land: the first reply to arrive with edits
    /// in it moves the revision, and `Ask::current` then drops the other as
    /// the answer about an older document it is. What is outstanding at once is
    /// bounded by the server's own list — at most one question per character it
    /// declared, each replaced by the next press of that character, and all of
    /// them dropped with the conversation if it dies.
    Formatting(char),
    /// How the whole file should be laid out — the one formatting question
    /// somebody presses a key for, which is what makes it the one that speaks
    /// when it comes back empty-handed (R32.2).
    Document,
}

impl About {
    /// Everything that differs between one question and another: the
    /// protocol's name for it, the `capabilities` field a server declares it
    /// under, and the slug for one that declined it. The whole of the table, so
    /// a third capability is spelled here and nowhere else — which is why the
    /// declarations are held as the server's own words rather than as a bool
    /// per capability, whose absence the compiler could not have caught.
    ///
    /// `None` is a question refused in silence, and the third capability is
    /// what put it there. Hover and definition are keystrokes: a key that
    /// silently does nothing reads as a broken key, so the reader is told there
    /// is nobody to ask. Candidates are asked for by *typing*, and a notice
    /// every time typing pauses in a file no server serves is the feature
    /// announcing itself constantly to say nothing — the one thing "typing
    /// behaves exactly as it does today" forbids.
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
            // Silent for a third reason, and not the other two: a server that
            // will not lay this file out is not the end of the question, it is
            // the point at which the configured command answers instead. Said
            // here, the reader would be told there is nobody to ask and then
            // watch somebody answer.
            About::Document => (
                "textDocument/formatting",
                "documentFormattingProvider",
                None,
            ),
        }
    }
}

/// One keystroke's question, and every server it went to. A `.vue` file is
/// served by the Vue server and by a TypeScript server, so one press of `gd` is
/// several requests — and the reader pressed one key, which is what everything
/// below arbitrates for.
///
/// It is one record rather than an [`Ask`] per conversation because the two
/// rules that matter are about the *set*: the first non-empty answer is the
/// answer, and the empty-handed notice waits for the last server. Held per
/// [`About`] because a question is what a key asked, and a second press of the
/// same key replaces the question rather than joining it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    asked: Ask,
    /// The id each server was asked under, keyed by language. Two things at
    /// once, and both are needed: it makes a reply a reply — one whose id is
    /// not here answers a question nobody asked — and it says *who* was asked,
    /// which is the difference between a server that knows nothing and a server
    /// that relays the question somewhere Varde will not follow.
    sent: BTreeMap<String, i64>,
    /// Which of them have not answered yet. A question with none left is over,
    /// however it ended.
    waiting: BTreeSet<String>,
    /// Whether one of them replied with an error. Kept because an error is
    /// empty-handed for the arbitration — a server that would not answer must
    /// not beat one that will — and the empty hand should still say which kind
    /// of nothing it was, rather than reporting a refusal as ignorance.
    refused: bool,
}

/// What one reply came to. `Nothing` is the reply another server may still
/// beat: the server with no answer had less to look up, and letting it speak
/// for the rest is the whole of the reported defect.
///
/// `Something` retires the question rather than flagging it: a later reply then
/// finds no question and is dropped by the guard that already drops a reply
/// nobody asked for, and nothing is left outstanding on a server that may never
/// answer.
enum Told {
    Nothing,
    Something(Vec<Effect>),
}

/// Every language whose server serves this path: the language the file *is*,
/// and every language its own `[lsp.<language>].also_served_by` names. Data,
/// never a branch naming one — the line ADR 0011 draws, and the reason a
/// Volar-shaped language, a linter beside a type server, and every arrangement
/// other editors reach by attaching several clients to one buffer are all one
/// mechanism here.
///
/// The file's own language comes first, and a name nothing configures is
/// dropped: a server that does not exist cannot be asked, and the caller would
/// have to filter it out again.
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

/// Whether that language's server serves this path — the reverse of
/// [`served_by`], for the passes that hold a language and are walking the
/// files.
fn serves(state: &State, language: &str, path: &Path) -> bool {
    served_by(state, path).iter().any(|held| held == language)
}

impl Ask {
    /// Whether the buffer on screen is still what this question was asked
    /// about. The file always counts — another file is another symbol, and a
    /// reply that edited a buffer the reader has left is an edit nobody saw
    /// made. What makes it the *same* question differs by what the reply is
    /// for: a box drawn beside the cursor is stale the moment the cursor
    /// moves, and edits are stale the moment the text does.
    ///
    /// Two rules rather than three, and no more: the version is R31.7's, the
    /// place is the hover box's, and inventing a third notion of stale is what
    /// this feature was told not to do.
    fn current(&self, state: &State) -> bool {
        state.current_buffer.as_ref() == Some(&self.path)
            && state
                .buffers
                .get(&self.path)
                .is_some_and(|buffer| match self.about {
                    About::Formatting(_) | About::Document => buffer.revision() == self.revision,
                    // A box asked for by resting the pointer is a claim about
                    // where the pointer is, so it stands while the pointer is
                    // still there: the cursor was never moved for it, and
                    // measured against the cursor the reply would be dropped as
                    // stale before it could be drawn.
                    About::Hover if state.pointed_at == Pointed::Text(self.place) => true,
                    // And while the pointer is on the box itself, which is
                    // the only way to read or scroll a box longer than a
                    // glance: the pointer has to cross from the symbol to get
                    // there, and a box that went when it left the symbol
                    // could never be reached.
                    About::Hover if state.pointed_at == Pointed::Hover => true,
                    _ => buffer.line == self.place.line && buffer.column == self.place.column,
                })
    }
}

/// Why a server stopped, as only the edge can know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gone {
    /// The command was not found, or the child could not be started at all.
    FailedToStart,
    Exited,
}

/// How bad one diagnostic is. Named rather than left as the protocol's integer
/// because the gutter distinguishes them and a Scenario spells them out: an
/// error is not a hint.
///
/// The variants are in the protocol's own order, worst first, and that order is
/// load-bearing — a line can carry several diagnostics and `Ord` is what picks
/// the one the gutter announces.
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

/// One thing a server says about one line. Not `lsp_types::Diagnostic`: that
/// carries a dozen fields nothing here reads, and its severity is optional,
/// which is a decision this type has already made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based, as the editor counts lines and as the Scenarios describe them.
    /// The protocol counts from zero.
    pub line: usize,
    /// The first column the underline covers, 1-based.
    pub column: usize,
    /// The last column it covers, 1-based and inclusive, or nothing at all
    /// when the server's range ends on a later line — the line's own end is
    /// then the end of the underline, and the length of a line is not a fact
    /// this type carries. Inclusive rather than the protocol's exclusive end
    /// so that a zero-width range, which servers emit routinely for a missing
    /// token, is a column behind its start and clamped forward onto it rather
    /// than being an empty span nothing is drawn under.
    pub end_column: Option<usize>,
    pub severity: Severity,
    pub message: String,
}

/// Whether a language's server has finished its handshake and may be asked
/// something.
pub fn ready(state: &State, language: &str) -> bool {
    matches!(
        state.lsp.get(language),
        Some(Conversation {
            handshake: Handshake::Ready,
            ..
        })
    )
}

/// Whether a language has been written off — the conversation [`gone`] leaves
/// behind. The core's own memory of what the edge observed, which is what lets
/// the palette's row say a command is here and does not work without spawning
/// anything to find out (R31.10).
pub(crate) fn written_off(state: &State, language: &str) -> bool {
    matches!(
        state.lsp.get(language),
        Some(Conversation {
            handshake: Handshake::Gone,
            ..
        })
    )
}

/// The pass every event goes through, once `update` has decided what the event
/// meant: whichever servers the open buffers need are started, and whichever
/// documents have moved are sent. One pass rather than a line in every editing
/// arm, for the reason the scroll clamp is one rule — an arm that has to
/// remember to tell the server is an arm that will forget.
///
/// It is idempotent by construction: what has been sent is recorded per path, so
/// running it after an event that changed nothing sends nothing.
pub fn sync(state: &mut State) -> Vec<Effect> {
    let mut effects = Vec::new();
    forgotten(state);
    effects.extend(closed(state));
    let facts = facts(state);
    // A process is asked for where an open buffer needs one and neither the
    // core nor the edge has one already. Nothing is remembered here: a spawn
    // that was asked for is not a process that exists, which is `ai_running`'s
    // whole lesson, so the conversation waits for the edge to say it holds one.
    for (language, server) in servers_for_open_buffers(state) {
        if state.lsp.contains_key(&language) || state.lsp_running.contains(&language) {
            continue;
        }
        // Nothing is started for a language whose configuration names a fact
        // this workspace has no answer for. The row already says
        // `missing-requirement`; spawning anyway is how a Vue server came to be
        // launched without the SDK the requirement was about, to die on the
        // first `didOpen` and reach the reader as "the language server
        // stopped" — a crash to interpret in place of an honest row.
        //
        // A skip, not a write-off: nothing died, so there is no `Gone`
        // conversation to forget and no notice to explain away, and the pass
        // that runs once the fact appears starts the server exactly as a fresh
        // start would — the same shape [`forgotten`] gives a command that
        // appears (R31.24).
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
    // A conversation begins when the edge holds a server to have it with —
    // whichever way it came to hold one, since which languages it holds is the
    // edge's fact and this reads it rather than a memory of having asked.
    // `Event::LspStarted` is what brings this pass around after a spawn.
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
    // Every conversation that has finished its handshake hears what it is
    // missing: the buffers on screen, and the files under review it serves.
    // Over the conversations rather than over the open buffers, because a file
    // under review has no buffer and its language need not have one either.
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

/// A command that has appeared is a reason to forget that it was missing, which
/// is why there is no retry loop and no restart (R31.24). The write-off itself
/// is not a bug and stays exactly as it was — [`gone`] holds the language and
/// [`sync`] skips it, which is what stops a second file of that language from
/// re-running a binary that is not there. What this adds is the *reason to
/// forget*: dropping the conversation is all it does, and the pass it runs at
/// the top of does the rest, spawning the server exactly as a fresh start
/// would. No timer, no counter, and no second spawn path.
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

/// The version a file under review is told to the server under. Read-only, so
/// it never changes and never needs a second one: the Buffer revisions it might
/// otherwise collide with start at 1, which leaves a file opened for review and
/// then opened as a Buffer moving forwards rather than back.
const REVIEW_VERSION: u64 = 0;

/// Which files are under review, absolute. Not gated on the view: a document
/// stays open at the server for as long as the file is part of the change,
/// which is what stops a trip out of Review view and back from closing and
/// reopening every one of them — and what keeps the counts current while the
/// reviewer is away. What Review view being on screen gates is the *asking*,
/// in [`review_reads`].
fn reviewed(state: &State) -> Vec<PathBuf> {
    crate::review::list(state)
        .into_iter()
        .map(|file| state.root.join(file))
        .collect()
}

/// Ask the edge for what a file under review holds, so its server can be told.
/// Nothing is spawned: only a conversation already running is handed documents,
/// which is the distinction R31.19 turns on — a keystroke that launches one
/// process per changed language is a side effect nobody asked for, and one that
/// hands documents to a conversation already open is the protocol working.
///
/// The contents cannot be read here, so the read is an effect and the sending
/// waits for [`read_for_review`]. What has been asked for is remembered, or the
/// next pass would ask again for every file already in flight.
fn review_reads(state: &mut State, language: &str) -> Vec<Effect> {
    // Only for the view that asks the question. Reading every changed file in a
    // workspace nobody is reviewing is work nobody asked for.
    if state.view != crate::View::Review {
        return Vec::new();
    }
    let wanted: Vec<PathBuf> = reviewed(state)
        .into_iter()
        .filter(|path| serves(state, language, path))
        // A file open as a Buffer is already told, at the revision on screen —
        // and the Buffer is the better answer, since it is what the reviewer
        // would be reading.
        .filter(|path| !state.buffers.contains_key(path))
        .filter(|path| {
            state
                .lsp
                .get(language)
                .is_some_and(|conversation| !conversation.sent.contains_key(path))
        })
        // Nobody already reading it, rather than this conversation not reading
        // it: the read is of a *file*, and [`read_for_review`] hands what comes
        // back to every server that serves it. Asking the edge once per server
        // would read the same file twice for one answer.
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

/// What a file under review holds, back from the edge: the server is told the
/// document is open, and told nothing about it ever again — nobody is editing
/// it, so there is no change to send and no version to move.
pub fn read_for_review(state: &mut State, path: &Path, contents: &str) -> Vec<Effect> {
    // Every server that serves it, for the reason [`served_by`] exists: the
    // read went out once, and a `reading` entry left standing on the second
    // server is a document it would never be told about and never ask for
    // again.
    let mut effects = Vec::new();
    for language in served_by(state, path) {
        if let Some(conversation) = state.lsp.get_mut(&language) {
            conversation.reading.remove(path);
        }
        // Everything the read was asked under can have moved while it was in
        // flight: the file can have left the review, the server can have gone,
        // and the reviewer can have opened the file as a Buffer — which owns it
        // from then on, at a version this would be sending backwards from.
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
                        // What the file *is*, which is not what the server that
                        // also serves it is keyed by: a `.vue` file told to a
                        // TypeScript server is still a Vue document.
                        language_id: self::language(state, path).unwrap_or(&language).to_string(),
                        version: document_version(REVIEW_VERSION),
                        text: contents.to_string(),
                    },
                },
            ),
        });
    }
    effects
}

/// A key changed the text while the Buffer was inserting, so what may follow
/// it is worth asking about once typing pauses.
///
/// Called from the two arms a key can change text through — a character and a
/// backspace — and deliberately from neither [`sync`] nor the arm that accepts
/// a candidate. `sync` sees every change alike: a file reloaded under the
/// cursor and an accepted completion both move the revision, and neither is
/// somebody typing a name. Arming there put a second list up over the word the
/// first one had just completed, 120 ms after accepting it.
///
/// What is asked about is the *text*, not the key: an identifier character
/// before the cursor means there is a word to complete, a backspace inside one
/// included. Anything else ends the word, so a list still up goes with it
/// rather than standing over the next one.
pub fn typed(state: &mut State) -> Vec<Effect> {
    if !typing_a_name(state) {
        if matches!(state.modal, crate::Modal::Candidates(_)) {
            state.modal = crate::Modal::None;
        }
        return Vec::new();
    }
    narrowed(state);
    // Nothing to ask means nothing to wait for: an armed timer is a redraw
    // when it fires, and a frame drawn for a question no server answers is a
    // frame drawn for nothing.
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

/// A character typed while inserting, which may be one a server asked to be
/// told about so it can lay out the text around it.
///
/// The narrowing is the one [`typed`] does for the debounce and it is there for
/// the same reason: almost no keystroke is a trigger character, and a language
/// whose server offers no formatter must not be slower for the feature
/// existing.
///
/// What the document pass is doing in here is the one surprising thing in this
/// feature. The server has to *hear* the text before it is asked about it, and
/// [`sync`] runs at the end of `update` — which would put this request in front
/// of the change it is about, a position past the end of the document the
/// server holds, answered with nothing. Pulled forward, and idempotent, so what
/// it sends here the pass at the end does not send again.
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

/// `:format`, put to whoever serves the file and says they can lay it out — or
/// to nobody, which is the caller's cue to run the configured command instead.
/// Nothing is said on the way past: a server that declares no formatter has not
/// ended the question, it has handed it on.
///
/// The document pass comes first for the reason [`on_type`] pulls it forward:
/// [`sync`] runs at the end of `update`, so a server asked here would be
/// answering about the text it was last told, and every range in its reply
/// would name a line in a document that no longer exists.
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

/// The list on screen, narrowed by the word as it now stands.
///
/// A character typed narrows rather than closes: the reply is not "the
/// completions for `wor`" but everything in scope at that position, marked
/// `isIncomplete`, with the client expected to keep filtering — so closing the
/// list on the next keystroke throws away the answer the server gave for it.
/// A backspace widens it again, back to the whole reply, because nothing was
/// discarded. The debounce keeps doing its job underneath: a fresh request
/// still goes out and still replaces the list.
///
/// A word that now matches nothing closes the list, which is R31.14's promise
/// of never showing an empty box reached by a shorter road. So does a list
/// belonging to another file or another line — the staleness rule the hover
/// box has.
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
    // The narrowed list is matched afresh, so the row that was chosen is not
    // the row that was chosen: an index kept across a re-ranking names a
    // different name. The closest match is what a reader typing another
    // character was reaching for.
    list.selected = 0;
    list.first = 0;
}

/// The word the list on screen is being narrowed by, or nothing at all when
/// there is no list or it no longer belongs to what is on screen. A cursor
/// left of where the word started is a backspace out past the anchor, which is
/// the whole reply rather than a closed list — [`typing_a_name`] has already
/// closed it if the word itself is gone.
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

/// What one line of a buffer holds between two columns, both 1-based. The word
/// a list was asked about and the word it is narrowed by now are the same
/// reading, so they are one function.
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

/// Whether the cursor sits just after a character an identifier can be made
/// of. The one thing that says a word is being typed, and it reads the Buffer
/// rather than the keystroke — which is what makes a backspace inside a word
/// ask again, and a space ask nothing.
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

/// Whatever is no longer open, told to the server that was told it was open.
/// Wherever the closing happened — `:q`, or the tree's preview replacing the
/// Buffer under it as the reader arrows past a folder of files.
///
/// Forgetting it is not enough on its own: the server would still believe the
/// document is open at the version it last heard, and a Buffer opened again
/// starts at its first revision — a version going *backwards*, which the
/// protocol forbids. Forgetting it is also not optional, because a version the
/// closed Buffer had is what made the reopened one look like a file the server
/// already had, and so never mentioned to it at all.
fn closed(state: &mut State) -> Vec<Effect> {
    // A file under review is open to the server without being a Buffer, so the
    // set is both — and leaving the review closes it exactly as closing a
    // Buffer does, which is what keeps the counts describing the change rather
    // than describing what the change used to be.
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
    // A read still in flight for a file nobody is reviewing any more is
    // dropped too, or the file could never be read again if it came back.
    for conversation in state.lsp.values_mut() {
        conversation.reading.retain(|path| open.contains(path));
    }
    for (language, path) in gone {
        if let Some(conversation) = state.lsp.get_mut(&language) {
            conversation.sent.remove(&path);
        }
        // A file that left the review takes its counts with it. A Buffer's
        // marks survive being switched away from — the file is still the file,
        // and R31.8 wants them back when it returns — but a file that is no
        // longer part of the change was put back to what HEAD holds, and what
        // the server said describes text that no longer exists. Left standing,
        // the same file modified again would re-enter the review already
        // carrying the last version's errors.
        if !state.buffers.contains_key(&path) {
            state.diagnostics.remove(&path);
        }
        let Some(uri) = uri(&path) else { continue };
        // Only a server still listening is told: one that is gone has no view
        // of the document left to correct.
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

/// Which languages the open buffers are in, and what configuration says runs
/// each one. A language nothing configures is absent, so nothing is spawned for
/// it and the editor behaves exactly as it does without this feature.
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

/// Every open buffer in the language, told as it stands on screen: opened if the
/// server has not seen it, changed if its revision has moved. The Buffer's own
/// revision is the Document version — it is bumped by every content change and
/// nothing else, which is already why the edge caches its tokens against it.
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
                        // What the file *is*, which is not always the
                        // conversation it is being told to: a `.vue` file told
                        // to a TypeScript server is still a Vue document
                        // ([`served_by`]).
                        language_id: self::language(state, path).unwrap_or(language).to_string(),
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

/// The overlay a hover reply put on screen: what the server said about the
/// symbol under the cursor, rendered and wrapped, and which lines it sits over.
///
/// Rendered and wrapped here rather than by the renderer, because the box is
/// sized from the row count — markdown read at draw time makes that count a
/// lie, and the box is drawn one row tall while the message needs three.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    /// The reply as rendered rows, not as the characters the server sent: a
    /// markdown reply drawn verbatim shows `#`, `**` and fences as the
    /// characters they are spelled with instead of what they mark up. The same
    /// [`crate::preview::Row`] a Preview holds, so the reader that already
    /// turns markdown into styled rows is the one that does it here, and `ui`
    /// maps a piece to a span in exactly one place.
    pub lines: Vec<crate::preview::Row>,
    /// The first buffer line the box sits over, 1-based. Never the line it
    /// describes: a box over the symbol answers the question by hiding what was
    /// asked about. The last line is not held beside it — it is this plus the
    /// lines there are, and a stored copy is a second author for one fact.
    pub from: usize,
    /// What the box is a claim about: the file and the place the question was
    /// asked at. Kept so the claim can be checked against the present on every
    /// pass — a box drawn over a different file, or over a cursor that has
    /// moved on, describes a symbol nobody is looking at.
    pub asked: Ask,
    /// The first of `lines` the box draws. A reply longer than the pane is
    /// only readable by scrolling the box, and scrolling the editor under it
    /// moves the symbol it describes instead. Clamped in `update`.
    pub first: usize,
    /// Whether the keyboard is in the box. Here and not a [`crate::Modal`]
    /// variant, so the keyboard goes with the box whatever takes it down.
    pub focused: bool,
    /// While Paused, what the expression under the pointer holds — asked of
    /// the Debug adapter and drawn above the type and docs. `None` outside a
    /// session, which is every Hover Varde drew before there was a debugger,
    /// and which is what keeps that box unchanged.
    pub value: Option<crate::debug::Hovered>,
}

/// One row of the box, and which of its two sections it belongs to. The value
/// comes first while Paused: a reader stopped in their program wants what the
/// expression *holds* before what it is.
///
/// One list, because the renderer draws these rows, the placement measures
/// them and the mouse hit-tests against the box they fill — three derivations
/// of how tall a box is is a box drawn where nobody put it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// What the expression holds, as a tree that opens like the Variables.
    Value(crate::debug::Row),
    /// That the expression would have to be evaluated to be known, because it
    /// calls something and a Hover never calls anything.
    Needs,
    /// The language server's type and docs.
    Docs(crate::preview::Row),
}

impl Said {
    /// What the row reads as, which is what the box is measured against.
    pub fn text(&self) -> String {
        match self {
            Said::Value(row) => format!("{}{} = {}", "  ".repeat(row.depth), row.name, row.value),
            Said::Needs => NEEDS_EVALUATE.to_string(),
            Said::Docs(row) => row.text(),
        }
    }
}

/// What the box says where the expression under it calls something. The words
/// and not a slug, unlike a refusal's, because the box is measured against
/// them: a renderer drawing a sentence the core sized a shorter one for is a
/// sentence clipped at the border.
pub const NEEDS_EVALUATE: &str = "calls something \u{2014} evaluate to see it";

/// The box's rows, top to bottom. Empty when there is no box.
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

/// How many rows the box takes on screen: the rows it says, plus the two its
/// border sits on. The renderer draws the border, but a border row hides a
/// line of code exactly as a text row does, so the count is the core's —
/// measured without it, the box sits one row over the symbol it describes,
/// which is what it did until a real server was driven at the bottom of the
/// pane.
pub fn rows(state: &State) -> usize {
    sections(state).len() + 2
}

/// Whether the box is drawn over `line`. Beside [`rows`] and [`placement`]
/// rather than on [`Hover`], because all three are measurements of the box on
/// screen and the box's own fields answer none of them alone.
pub fn covers(state: &State, line: usize) -> bool {
    state
        .hover
        .as_ref()
        .is_some_and(|hover| (hover.from..hover.from + rows(state)).contains(&line))
}

/// Where the box goes and how big it is, measured over everything it says.
pub fn placement(state: &State) -> Option<Placement> {
    let hover = state.hover.as_ref()?;
    let texts: Vec<String> = sections(state).iter().map(Said::text).collect();
    // Never narrower than its own Chips: the box's border is where they are
    // drawn, and a box measured over its text alone cuts one off — which is
    // the one thing a strip of Chips may never do.
    let chips = crate::layout::strip_width(&crate::debug::hover_labels(state, u16::MAX)) as usize;
    Some(Placement {
        from: hover.from,
        // At the text's left edge: a hover describes the whole line, where
        // the candidate list offers a replacement for one word in it.
        column: 1,
        width: measured(texts.iter().map(String::as_str)).max(chips + 2),
        rows: rows(state),
    })
}

/// Where a box over the buffer goes and how big it is — the four numbers the
/// core decides and the renderer only reads.
///
/// One type because they travel together and because every one of them that
/// was decided a second time inside `ui.rs` became a box drawn somewhere the
/// core had not put it: the column read the pane's left margin, and the width
/// was measured over the rows the box happened to be showing, so a list
/// shifted left by its widest label was drawn at the width of ten of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub from: usize,
    pub column: usize,
    pub width: usize,
    pub rows: usize,
}

impl Placement {
    /// Where the box is on screen, over the editor pane it was placed in: its
    /// buffer line and column turned into a row and column off the same
    /// `editor_scroll` and `editor_hscroll` the code is drawn from, and kept
    /// inside the screen across and inside the pane down. The box floats over
    /// its neighbours like every other overlay — the core wraps it to the
    /// screen for that reason — but its *rows* are that pane's lines, so a row
    /// outside the pane would sit beside a line nobody is looking at.
    ///
    /// Here rather than in `ui` because the pointer is hit-tested against the
    /// box as well as drawn under it: two copies of this arithmetic is a
    /// pointer on the box's border read as a pointer on the code. The screen's
    /// width is the AI pane's right edge, the last column the layout hands out.
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

/// How wide a box holding these lines is: its widest line in *screen columns*,
/// plus the two its border sits on. Screen columns and not characters, because
/// that is what the renderer lays the box out in — measured in characters, a
/// box holding a wide script is shifted by a width it does not have.
fn measured<'a>(lines: impl Iterator<Item = &'a str>) -> usize {
    lines
        .map(unicode_width::UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
        + 2
}

/// The list a completion reply put on screen. It is a [`crate::Modal`] variant
/// and not a field beside one: a list that is open, a list that has items and a
/// list with something chosen in it are one fact, and as three fields two of
/// them could disagree — an open list with nothing in it is exactly the empty
/// box this feature must never show.
///
/// Unlike every other modal it does not claim the keys it has no use for:
/// `keys::candidate_list` passes them on, because this is a list offered
/// *while typing* rather than a question that stops it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidates {
    /// Everything the reply carried, never narrowed down. A reply with nothing
    /// in it closes the list instead of building one, so there is always
    /// something for `selected` to name — and the whole reply is kept rather
    /// than the part matching what is typed, because a backspace widens the
    /// list back to it (R31.14) and a discarded item cannot come back.
    pub items: Vec<Candidate>,
    /// Which of [`Candidates::shown`] is chosen — an index into what is on
    /// screen, not into `items`, since narrowing reorders as well as filters
    /// and an index into the reply would name a different name after a
    /// keystroke.
    pub selected: usize,
    /// The first of [`Candidates::shown`] the box draws, for the reason
    /// `editor_scroll` and `tree_scroll` exist: a list is a viewport onto its
    /// contents rather than a rendering of them. Clamped in `update` off the
    /// selection, so no arm has to remember to scroll it.
    pub first: usize,
    /// The first buffer line the box sits over, 1-based — placed by the same
    /// [`placed`] rule the hover box is, and so never the line being edited.
    pub from: usize,
    /// The buffer column the box's left edge sits at, 1-based. Beside `from`
    /// and decided by the same function, because a box's column is as much a
    /// decision as its row: the cursor's, since the list offers replacements
    /// for the word under it, shifted left rather than drawn off the pane. It
    /// was an expression in `ui.rs` and so a decision nothing could assert on,
    /// which is how a list for column 40 was drawn at column 1.
    pub column: usize,
    /// What the list is a claim about: the file and the place the request went
    /// out from. The staleness rule the hover box has, and the anchor the
    /// narrowing is measured against.
    pub asked: Ask,
    /// The buffer column the word being completed starts at, 1-based — found
    /// once, from the place the request went out, rather than re-read from the
    /// buffer on every keystroke. Typing forward cannot move it, and a
    /// backspace out of the word closes the list rather than moving it.
    ///
    /// `Buffer::word_start` finds it, which is the same function
    /// `Buffer::complete` replaces from: what a completion overwrites and what
    /// the list is narrowed by must be the same characters.
    pub word_start: usize,
    /// Whether the reply named its own order, in which case [`shown`] keeps it
    /// rather than ranking. Read off the reply once rather than asked of the
    /// items on every keystroke.
    ///
    /// [`shown`]: Candidates::shown
    pub ordered: bool,
    /// The word as it now stands, from `word_start` to the cursor. The reply is the
    /// server's answer about everything in scope at that position — marked
    /// `isIncomplete`, and expecting the client to keep filtering as the reader
    /// types, which is what `filterText` and `sortText` are for. So a character
    /// typed narrows this rather than closing the list.
    pub typed: String,
}

/// How many candidates the box shows at once, however many the reply carried.
/// A real server answers a bare prefix with several hundred, and a box as tall
/// as the reply is a box the renderer clamps to the whole pane — covering
/// every line including the one being typed, which is R31.14's promise broken
/// by the value no Scenario had produced.
const WINDOW: usize = 10;

/// One thing the server offered: what the list shows, what accepting it puts
/// in the buffer, and the two fields the protocol provides for narrowing the
/// list as the reader keeps typing. Label and insert differ — a server offers
/// `push(…)` to read and `push` to type — and inserting the label is how a
/// completion becomes something to undo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub label: String,
    pub insert: String,
    /// What the typed word is matched against, where the server named
    /// something other than the label. A label is what a human reads —
    /// `Write to disk`, `crate::work_done` — and matching against it drops the
    /// item the server meant to offer for `work`.
    pub filter: Option<String>,
    /// The server's own ranking, where it gave one. The array's order is not
    /// it: the protocol says the client sorts by this.
    pub sort: Option<String>,
    /// Whether `insert` is a snippet — the reply's `insertTextFormat`, resolved
    /// where the reply's other defaults are. The protocol's own default is
    /// plain text, and a server only ever sends the other because Varde
    /// declared it could receive it.
    pub snippet: bool,
}

impl Candidate {
    /// What a typed word is matched against: the server's `filterText`, or the
    /// label, which is the protocol's own default.
    fn matched(&self) -> &str {
        self.filter.as_deref().unwrap_or(&self.label)
    }
}

impl Candidates {
    /// What the list is offering now: everything the reply carried, narrowed to
    /// what matches the word being typed and ranked closest-first.
    ///
    /// Matched by [`crate::filter`], the same function the tree's filter box
    /// narrows with, because a completion list that matches differently from
    /// every other list in Varde is a difference nobody could predict.
    ///
    /// **Ordered by the server where it gave an order.** A reply carrying
    /// `sortText` has already been put in the server's own order by
    /// [`candidates`], and re-ranking it here would override the ranking the
    /// protocol provides that field for. Only a reply that named no order at
    /// all is ranked closest-first, and there ties keep the order it arrived
    /// in.
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

    /// How many rows the box takes: what it shows, plus the two its border sits
    /// on — measured for the reason [`Hover::rows`] is, since a border row
    /// hides a line of code exactly as an item does. Bounded by [`WINDOW`], so
    /// this answers for the box rather than for the reply.
    pub fn rows(&self) -> usize {
        self.shown().len().min(WINDOW) + 2
    }

    pub fn placement(&self) -> Placement {
        Placement {
            from: self.from,
            column: self.column,
            // Over everything the list is offering rather than over the ten
            // rows on screen, so the box keeps its width as the window
            // scrolls — and so the width the column was shifted by is the
            // width the box is drawn at.
            width: measured(self.shown().into_iter().map(|item| item.label.as_str())),
            rows: self.rows(),
        }
    }

    pub fn covers(&self, line: usize) -> bool {
        (self.from..self.from + self.rows()).contains(&line)
    }

    /// The choice, moved. Clamped at both ends rather than wrapped: a list
    /// that jumps from the last row to the first is a list where holding an
    /// arrow accepts something nobody looked at.
    pub fn step(&mut self, direction: crate::Direction) {
        let last = self.shown().len().saturating_sub(1);
        match direction {
            crate::Direction::Down => self.selected = (self.selected + 1).min(last),
            crate::Direction::Up => self.selected = self.selected.saturating_sub(1),
            // The list is one column wide; sideways is the buffer's.
            crate::Direction::Left | crate::Direction::Right => {}
        }
    }

    /// The window pulled back over the selection — the same rule the editor
    /// and the tree scroll by, called from `update` so no arm has to remember
    /// it.
    pub fn scrolled(&mut self) {
        let shown = self.shown().len();
        let window = self.rows().saturating_sub(2);
        self.first = crate::layout::viewport(self.first, self.selected + 1, shown, window);
    }

    /// A reply, placed: below the line being typed and at the column the
    /// cursor is in, both bounded by what the pane shows. One function rather
    /// than a struct literal per caller, because the row and the column are
    /// the decisions this ticket moved into the core and a second author for
    /// either is a box in two places.
    pub fn offering(state: &State, items: Vec<Candidate>, asked: Ask) -> Candidates {
        let (_, rows, columns) = crate::fits(state);
        // Where the word starts, found from the place the request went out —
        // which `answered` has already checked is where the cursor still is.
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
            // The word the request went out about, so the reply is narrowed by
            // it from the moment it lands. Left empty, a reply of everything
            // in scope would be shown whole until the next keystroke — which
            // is the unfiltered list this whole requirement is about, arriving
            // one keystroke earlier.
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

    /// The choice. In range by construction: a list is only ever built with
    /// something in it, narrowing that empties it closes it instead, and
    /// [`Candidates::step`] clamps — but the selection is an index into a list
    /// a keystroke may have shortened, so it is clamped here too.
    pub fn chosen(&self) -> &Candidate {
        let shown = self.shown();
        shown[self.selected.min(shown.len() - 1)]
    }
}

/// A question about the symbol under the cursor, put to everyone who serves the
/// file. Every capability that *asks* rather than listens comes through here,
/// because the question is the only thing that differs: what went out is
/// recorded as one [`Question`] with the place, the [`About`] and an id per
/// server, and [`answered`] matches each reply back against all of it. Both
/// capabilities send the protocol's plain position parameters, so there is
/// nothing per-question left to branch on but the method's name.
///
/// One question, N requests: which servers serve a path is [`served_by`]'s
/// data, and a second press replaces the question rather than joining it.
pub fn ask(state: &mut State, about: About) -> Vec<Effect> {
    ask_at(state, about, None)
}

/// The same question, about a place the cursor is not at: `None` is the cursor,
/// which is where a keystroke asks from, and a place is where the pointer came
/// to rest.
pub fn ask_at(state: &mut State, about: About, at: Option<Place>) -> Vec<Effect> {
    let Some(path) = state.current_buffer.clone() else {
        return Vec::new();
    };
    let (method, capability, refusal) = about.asking();
    // A press abandons whatever the last one left outstanding for this key,
    // however this one turns out: a reply still owed for a cursor two presses
    // ago has nothing left to settle. Once, here, rather than at each of the
    // ways out below — one of which forgot.
    state.lsp_asked.remove(&about);
    // Everyone who serves the file, and only those that have got as far as
    // answering. A language nothing serves and a server mid-handshake are one
    // answer to the reader: there is nobody to ask. Said out loud for a
    // question somebody pressed a key for — see [`About::asking`] for why the
    // one nobody presses a key for is silent instead.
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
        // The server's own answer about itself. One that says it does not
        // answer this question is not asked it — and that branch reads what
        // arrived rather than guessing from a name, which is the line ADR 0011
        // draws. Per server, because two servers over one file need not agree
        // about what they can do.
        if !declares(&conversation.capabilities, capability) {
            continue;
        }
        // And, for the one question a server also names its own characters
        // for, whether it named *this* one. Two servers over one file need not
        // want the same characters any more than they need the same
        // capabilities.
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
                // The character goes with the question: a formatter is being
                // told what was typed, not asked what is here.
                About::Formatting(key) => request(
                    id,
                    method,
                    DocumentOnTypeFormattingParams {
                        text_document_position: at,
                        ch: key.to_string(),
                        options: options.clone(),
                    },
                ),
                // The whole document, so the cursor is not in the question at
                // all — the only one of these that names no place.
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
    // Nobody who serves the file answers this question, so the refusal is the
    // whole of the answer.
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

/// How many questions that language's server has been asked and not answered.
/// Public because "nothing is left waiting" is a claim about this number, and a
/// server that has gone must leave none behind.
pub fn outstanding(state: &State, language: &str) -> usize {
    state
        .lsp_asked
        .values()
        .filter(|question| question.waiting.contains(language))
        .count()
}

/// The protocol's own coordinates: lines from zero, and a column counted in
/// UTF-16 code units rather than characters — what a server reads unless the
/// client negotiates otherwise, and Varde negotiates nothing. An emoji earlier
/// in the line is two units wide there and one character here, so counting
/// characters would ask about the symbol next door.
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

/// A reply to something Varde asked, matched three times over: to the question
/// by the id its own server was asked under, to the present by the place that
/// question was asked about, and to the other servers asked the same thing. A
/// reply nobody asked for is dropped rather than trusted, one for a cursor the
/// reader has moved away from is dropped rather than drawn, and one arriving
/// after somebody else has already answered is dropped rather than acted on
/// twice.
///
/// The arbitration is the whole of the reason this is not three lines: the
/// server with nothing to say had less to look up, so it answers first, and
/// letting it speak for the rest is the reported defect — a `gd` that says the
/// server knows of no definition while the server that knows one is still
/// looking.
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
    // A refused question, remembered rather than reported on the spot. It is
    // empty-handed for the arbitration — a server that would not answer must
    // not beat one that still might — but reporting a refusal as "the server
    // knows nothing about this symbol" is an error surfacing as a domain
    // answer, which the error-handling rule forbids, so [`empty_handed`] is
    // told which kind of nothing this was.
    let error = message.get("error").is_some();
    question.refused |= error;
    let ask = question.asked.clone();
    let last = question.waiting.is_empty();
    let told = last.then(|| question.clone());
    if last {
        state.lsp_asked.remove(&about);
    }
    // `Ask::current`'s guard applies to each reply rather than to the question
    // — a cursor that moved abandons the whole question, since every reply for
    // it fails the same test.
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
        // The first non-empty answer wins, and the question is retired: every
        // later reply for this keystroke then finds no question and is dropped,
        // and nothing is left waiting on a server that may never answer.
        Told::Something(effects) => {
            state.lsp_asked.remove(&about);
            effects
        }
        // Nobody knew. Said only once every server asked has answered empty or
        // gone: notifying on the first empty reply is the reported defect
        // exactly.
        Told::Nothing => match told {
            Some(question) => empty_handed(state, &question),
            None => Vec::new(),
        },
    }
}

/// What a question every server answered empty comes to.
///
/// The two questions a key was pressed for say so, because a key that silently
/// does nothing reads as a broken key. The one nobody pressed a key for closes
/// its list instead: an empty box says "completion is broken", not "the server
/// knows of nothing that starts like this".
///
/// **Unless Varde is what declined to answer.** A server whose configuration
/// names an `unanswerable` request has, by construction, questions it cannot
/// answer without a companion Varde does not run — so a file served only by
/// such servers gets said that, rather than being told the server knows
/// nothing. Those two sentences were false in exactly the case that sent a
/// reader looking at their Vue install instead of at Varde (R31.25), and one
/// slug covers both keys because it is one refusal.
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
        // Nobody typed a brace to be told about formatting. A server with
        // nothing to change, one that offers no formatter for this file and
        // one that errored are the same thing to the reader: the text stays as
        // it was typed.
        About::Formatting(_) => return Vec::new(),
        // Somebody did press a key for this one, and a key that silently does
        // nothing reads as a broken key — the argument the two questions above
        // are said out loud for, reaching the third.
        About::Document => "nothing-to-format",
    };
    // Whose nothing it was, worst first. Varde's own refusal beats a server's
    // error, which beats a server that simply knows nothing here — each is
    // true of strictly less than the one before it.
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

/// What the server says the symbol is, put on screen.
fn hovered(state: &mut State, ask: Ask, result: &Value) -> Told {
    let (_, rows, _) = crate::fits(state);
    let Some(lines) = says(result, measure(state.screen_width as usize)) else {
        // An empty box beside the cursor says "the server answered" and nothing
        // else, which reads as hover being broken rather than as the server
        // knowing nothing about this symbol. What is said instead is
        // [`empty_handed`]'s, once every server has had its turn.
        return Told::Nothing;
    };
    // What the adapter already said about the same place is kept: the two
    // halves of the box are asked for separately and arrive in either order,
    // so a reply that replaced the box would drop whichever came first.
    let value = state
        .hover
        .take()
        .filter(|hover| hover.asked.place == ask.place)
        .and_then(|hover| hover.value);
    // Placed against the rows the box will take rather than the lines it holds,
    // which is why the box is built first and asked how tall it is.
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

/// The box put over the line it is about, against the rows it now has. Called
/// by both halves, because either can change how tall the box is and a box
/// placed against one half's rows sits over the symbol the other describes.
fn settle_box(state: &mut State, pane_rows: usize) {
    let (count, scroll) = (rows(state), state.editor_scroll);
    if let Some(hover) = state.hover.as_mut() {
        hover.from = placed(count, hover.asked.place.line, scroll, pane_rows);
    }
}

/// The Hover's value section while Paused: the expression under `at` and the
/// `evaluate` that asks what it holds, put up as a box of its own if the
/// language server has not answered yet.
///
/// The box goes up here rather than when an answer arrives, because a Hover
/// over a call has no answer coming — the adapter is never asked — and a box
/// that waited for one would never be drawn at all.
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

/// What the server says may follow what is being typed, put on screen.
///
/// A reply for a place the reader has already typed past never reaches here —
/// [`answered`] drops it against the [`Ask`], the same rule that keeps a hover
/// box off a symbol nobody is looking at.
fn offered(state: &mut State, ask: Ask, result: &Value) -> Told {
    let items = candidates(result);
    let open = matches!(state.modal, crate::Modal::Candidates(_));
    if items.is_empty() {
        // Nothing to choose closes the list rather than showing an empty box —
        // in [`empty_handed`], once every server has answered, so a server with
        // nothing to offer cannot close a list another one filled.
        return Told::Nothing;
    }
    // Nobody pressed a key for this, so it never takes the screen off
    // something that was asked for: a reply landing while the palette or a
    // confirmation is up leaves it alone.
    if !open && state.modal != crate::Modal::None {
        return Told::Nothing;
    }
    let list = Candidates::offering(state, items, ask);
    // Narrowed by the word it was asked about, a reply can arrive with nothing
    // in it that matches — which is the same empty hand as a reply with
    // nothing in it at all, and never an empty box.
    if list.shown().is_empty() {
        return Told::Nothing;
    }
    state.modal = crate::Modal::Candidates(list);
    Told::Something(Vec::new())
}

/// What the server says the text around the cursor should be, applied to the
/// buffer. The first reply Varde takes from a server that *changes* a file
/// rather than describing one, which is why everything careful about it —
/// applying back to front, as one undo step, clamped to the text — is in
/// [`crate::editor::Buffer::reformat`] under tests of its own.
///
/// A reply that is not a list of edits at all — `null`, which is the server
/// saying there is nothing to change — is the empty hand, and an empty list is
/// the same answer spelled the other way.
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

/// The buffer column the box's left edge sits at: the cursor's, shifted left
/// when a box that wide would run past the last column the pane shows, and
/// never left of the first. `hscroll` is the first column drawn, so the
/// comparison is about what is on screen rather than about the line.
///
/// The renderer keeps the box inside the screen as well — a box must not be
/// drawn outside its pane — but that clamp is a backstop now rather than the
/// thing deciding where the box goes.
fn at_column(cursor: usize, width: usize, columns: usize, hscroll: usize) -> usize {
    cursor
        .min(hscroll + (columns + 1).saturating_sub(width))
        .max(1)
}

/// Everything a completion reply offers, in the order the server offered it —
/// its own order, which is its ranking. Both shapes the protocol allows read
/// through the crate's own type; a `null` result is neither of them, which is
/// the server saying it knows of nothing here.
///
/// `filterText` and `sortText` are kept, because they are the two fields the
/// protocol provides for the client to keep narrowing and re-ranking as the
/// reader types — see [`Candidates::typed`]. Ordered by `sortText` only where
/// a server gave one: the array's order is a server's ranking when it names no
/// other, and re-ordering by label would override it with an alphabet.
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
            // What a server means by "insert this" when it says nothing else
            // is the label, which is the protocol's own default.
            insert: item.insert_text.unwrap_or_else(|| item.label.clone()),
            filter: item.filter_text,
            sort: item.sort_text,
            snippet: item.insert_text_format == Some(InsertTextFormat::SNIPPET),
            label: item.label,
        })
        .collect();
    // Only when *every* item names one. The protocol says `sortText` defaults
    // to the label, but a reply where one item carries it and the rest do not
    // would then have the rest alphabetised — an order no server asked for,
    // and the one this decision exists to avoid.
    if offered.iter().all(|candidate| candidate.sort.is_some()) {
        offered.sort_by(|a, b| a.sort.cmp(&b.sort));
    }
    offered
}

/// A snippet reply as the text to insert and the tab stops left in it, each a
/// character offset into that text, **in the order they appear in it**. A
/// snippet naming no `$0` ends at one — after everything it inserted — because
/// the last thing a sequence does is leave the cursor past the completion
/// rather than inside it, and that is also what makes a snippet with no
/// placeholders at all behave as the plain text it is.
///
/// **Their place in the text, not their numbers**, and that is a decision. A
/// stop is kept as the characters that *follow* it — see
/// [`crate::editor::Buffer::complete`] — which is what lets text typed at one
/// stop carry the stops after it along, and that only holds while the sequence
/// runs forwards. Honouring `${2:…} ${1:…}`'s numbering would visit the second
/// blank first and then land the next Tab inside what had just been typed into
/// it: the numbers right and the cursor in the wrong place, which is the worse
/// of the two ways to disagree with a server. Nothing numbers a snippet
/// backwards through its own text.
///
/// Only `$1`, `${1:default}` and `$0` are understood. Every other form the
/// grammar allows — choices, variables, transforms — is the literal characters
/// it is made of: the reply is a string from a child process, so this is a
/// trust boundary before it is a convenience, and a placeholder Varde cannot
/// resolve must reach the buffer as text rather than as its own syntax. `\$`
/// and `\\` are the protocol's escapes, and the second is not optional:
/// without it no server can write a backslash before a stop.
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
    // Ascending through the text, which is the invariant the tails rest on.
    // Stable, so two stops at one place keep the order they were written in —
    // the protocol's linked placeholders are out of scope, and visiting both in
    // turn is what falls out of that rather than a rule of its own.
    stops.sort_by_key(|(_, offset)| *offset);
    let ends_at_a_stop = stops.iter().any(|(number, _)| *number == 0);
    let mut ordered: Vec<usize> = stops.into_iter().map(|(_, offset)| offset).collect();
    if !ends_at_a_stop {
        ordered.push(out.chars().count());
    }
    (out, ordered)
}

/// One placeholder at the front of what is left: its number, the text it
/// stands for, and how many characters it spans. `$1` and `${1:default}` are
/// the two forms; anything else — a choice, a variable, a transform, a `$`
/// before nothing — is not a placeholder at all, and [`snippet`] leaves it as
/// the text it is.
///
/// The default is taken as text to the first `}`, so a placeholder nested
/// inside one is text like every other form Varde does not resolve.
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

/// Where the server says the name is introduced. Four situations, matched
/// exhaustively over the two sides of the workspace boundary, and the last two
/// are decisions rather than oversights.
///
/// The file already open moves the cursor and opens nothing — R31.12, and the
/// reason no effect is added for this feature at all: a place in a file is
/// already one thing to open, and `OpenAt` is what a search hit, a Story
/// citation and a Risk row all use to reach one.
///
/// **Several places are the project search's list**, because several
/// definitions are several places to open, which is exactly what a `Hit` is.
/// The first is never taken quietly.
///
/// **A place outside the workspace root is refused, naming it.** Every other
/// pane is bounded by the root — the tree, Review view, the project search, a
/// Risk Scope — so a Buffer outside it is one the tree cannot mark, the review
/// cannot see and the watcher does not watch. R31.13 records the rejected
/// alternative (opening it read-only) and why it can be added later without
/// undoing this. Saying *where* it went is what makes it a decision rather
/// than a key that did nothing: the path is the whole of the answer.
fn jumped(state: &mut State, ask: Ask, result: &Value) -> Told {
    let (inside, outside) = definitions(state, result)
        .into_iter()
        .partition::<Vec<_>, _>(|(path, _)| path.starts_with(&state.root));
    match (inside.as_slice(), outside.first()) {
        // Nowhere at all, which is the one reply another server may still beat
        // — what is said about it is [`empty_handed`]'s, once they all have.
        ([], None) => Told::Nothing,
        // Nothing in the workspace. The first place the server named is what
        // the message names — a reply naming several outside the root is one
        // jump the reader asked for, not a list of refusals. Built through
        // [`Effect::notify_about`], which is where the control characters a
        // child's path may carry are taken out.
        ([], Some((path, _))) => Told::Something(vec![Effect::notify_about(
            "definition-outside-workspace",
            path.display().to_string(),
        )]),
        ([(path, at)], _) if *path == ask.path => {
            // The cursor history is told before the cursor moves, because this
            // is the one landing with no landing *event* for `history::jumped`
            // to see: the reply arrives as a server's output and the cursor is
            // moved here. Without it, `gd` onto a name in the file you are
            // reading — the most common back-jump an editor has — leaves
            // nothing to come back from.
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
            // No line text: the files are not open, so there is nothing to
            // show but the file and the line — which is what a definition is,
            // and reading the line would mean reading a file the core may not
            // read. The query is the symbol asked about, because the modal
            // draws it and `OpenHit` measures the selection from it: an empty
            // one shows an empty search box and selects no characters at all.
            // Whatever else a list of hits has to be true of is `Results`' and
            // `Search`' own to say, so it is defaulted rather than restated.
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

/// Every place a definition reply names, in the editor's own 1-based
/// coordinates. All three shapes the protocol allows — one `Location`, several,
/// and the `LocationLink` form a server may send instead — read through the
/// crate's own type rather than by hand, and a `null` result deserialises as
/// none of them, which is the server saying it knows of nowhere.
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

/// The reverse of [`position`]: the protocol's zero-based line and UTF-16
/// column, read back as the editor's 1-based characters.
///
/// The text is what the column needs, and there is only text for a file already
/// open. For one that is not, the code units are counted as characters — exact
/// for every line that is ASCII, and the closest thing available for one that
/// is not, since the alternative is reading a file the core may not read. The
/// Buffer clamps what it is handed, so a column past the end of a line lands at
/// the end of it rather than nowhere.
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

/// Whether the box on screen is still a claim about what is under the cursor.
/// Asked on every pass through `update`, so no arm has to remember to take a
/// hover down: the cursor moving, the buffer closing, another file opening and
/// the pane starting to show something other than that buffer's lines are one
/// rule rather than four sites.
pub fn hover_stands(state: &State) -> bool {
    // A diff, a Preview and a walked Story all draw rows that are not the
    // buffer's lines (ADR 0007), so the line the box was placed on is not the
    // row it would be drawn at. Not a wrong box: a box about nothing.
    state.diff.is_none()
        && state.walking.is_none()
        && !crate::previewing(state)
        && state
            .hover
            .as_ref()
            .is_some_and(|hover| hover.asked.current(state))
}

/// How wide the box's text may be: the **screen**, less its border and the
/// space either side of it — the measure `ui::overlay_measure` wraps every
/// other overlay to, bounded above by [`WIDEST`], and a floor for the same
/// reason, since a screen has not always reported its size yet and a box
/// narrower than a word shows nothing.
///
/// The screen and not the editor pane, which is what this read first: the box
/// floats over its neighbours like every other overlay rather than being pinned
/// inside one pane. `features/language_intelligence.feature` settles it — on a
/// 40-column screen it asks for a 26-character type to be readable, and the
/// editor pane there is ten columns wide, so a box confined to the pane could
/// not show what the Scenario asks for.
fn measure(width: usize) -> usize {
    width.saturating_sub(4).clamp(20, WIDEST)
}

/// The widest a box's text is allowed to be, however wide the screen is. Prose
/// set across a hundred and ninety columns is prose nobody's eye can track back
/// from, and a box is read once and dismissed: "small in width, like 10cm max"
/// is a hand's width of text on a real screen, which is about this many columns
/// of it. The floor above still wins on a narrow screen, so the box is never
/// wider than the screen it is drawn on.
const WIDEST: usize = 60;

/// The most rows of text a box may hold. A doc comment `rust-analyzer` writes
/// renders to dozens of rows, and [`placed`] would then put a box taller than
/// the pane wherever it fits worst, with `ui` clipping the tail off in silence.
/// Bounded here for the same reason [`WINDOW`] bounds the candidate list: the
/// box is sized from this count, so the count has to be one the pane can draw.
/// The surplus is not dropped quietly — the last row says it was cut, which is
/// the only thing a box with no footer can say out loud.
const TALLEST: usize = 20;

/// What a hover reply says, rendered and wrapped, or nothing at all — a `null`
/// result, and a reply whose rows are all blank, are both the server saying it
/// knows nothing here.
fn says(result: &Value, measure: usize) -> Option<Vec<preview::Row>> {
    let hover: ServerHover = serde_json::from_value(result.clone()).ok()?;
    let rows = match hover.contents {
        // The one distinction the protocol draws and this used to flatten
        // away. `plaintext` is the server saying the `*` in its reply is an
        // asterisk, so reading it as markdown would be Varde inventing
        // formatting the server denied having — which is why there are two
        // paths here rather than one reader for both.
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

/// A server may answer with markdown, or with a block of a named language. The
/// second is a fence in everything but spelling, so it is spelled as one and
/// read by the same parser — which is what gets it highlighted instead of
/// drawn as flat prose, the whole of what this box was missing.
fn marked_rows(marked: MarkedString, measure: usize) -> Vec<preview::Row> {
    match marked {
        // The protocol says a bare string is markdown.
        MarkedString::String(text) => preview::rows(&text, measure),
        MarkedString::LanguageString(language) => preview::rows(
            &format!("```{}\n{}\n```", language.language, language.value),
            measure,
        ),
    }
}

/// A plain-text reply as rows: hard-wrapped, one paragraph row per line, and
/// every character the server sent kept as it sent it.
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

/// A verbatim row broken onto as many rows as it needs. [`preview::rows`]
/// leaves a fence's lines unwrapped on purpose — a line broken at the pane
/// edge is a line the block's own syntax does not permit — and a Preview lets
/// the reader slide sideways to reach the rest. A box does not slide, so a
/// signature wider than the box was measured at had its tail cut off, which is
/// the half of the reply the reader asked the question about. Broken by
/// [`preview::wrapped`], the same function a paragraph's rows are broken by, so
/// the pieces keep the highlighting they were given.
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

/// The row that stands where the rest of a capped reply was. A glyph rather
/// than a sentence: the box has no footer to put wording in, and a reader who
/// sees it can ask the server again with more room on screen.
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

/// The first line a box `tall` rows high sits over: below the line it
/// describes, or above it when there are not enough rows left below — never on
/// it. `top` is the first buffer line the editor is showing and `visible` how
/// many it shows, so the choice is made against what is on screen rather than
/// against the file.
fn placed(tall: usize, line: usize, top: usize, visible: usize) -> usize {
    // Below unless it would not fit — and below anyway when there is even less
    // room above, since a box pushed off the top shows less than a clipped one.
    if line + tall <= top + visible || line <= tall {
        return line + 1;
    }
    line - tall
}

/// One message from a server: a reply to something Varde asked, a request Varde
/// refuses, or a push it recognises by its method.
pub fn received(state: &mut State, language: &str, message: &str) -> Vec<Effect> {
    let Ok(mut message) = serde_json::from_str::<Value>(message) else {
        return Vec::new();
    };
    let id = message.get("id").and_then(Value::as_i64);
    // A message carrying both an id and a method is a *request*: the server is
    // waiting for an answer. Varde implements none of them, and a question
    // refused by silence leaves its asker waiting — the rule `queries::reply`
    // follows for a child that asks the terminal something it will not answer.
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
    // A server push: nothing correlates it to a request, so it is recognised by
    // its method and nothing else. Its params are taken rather than cloned —
    // a real server's first push for a file the compiler dislikes is hundreds
    // of diagnostics, and this runs on every message it ever sends.
    if message.get("method").and_then(Value::as_str) == Some("textDocument/publishDiagnostics") {
        published(state, language, message["params"].take());
        return Vec::new();
    }
    // A question the server asks on a method of its own, as a notification: the
    // protocol has no reply for one, so an asker that is waiting waits forever
    // unless something is said back on the method its configuration names.
    // Refused rather than answered — Varde runs no second server to ask — and
    // out loud rather than by silence, which is the rule `queries::reply`
    // follows for a child's escape sequences and the arm above already follows
    // for a server's requests.
    if let Some(asked) = state
        .servers
        .get(language)
        .and_then(|server| server.unanswerable.as_ref())
        .filter(|asked| message.get("method").and_then(Value::as_str) == Some(&asked.request))
        .cloned()
    {
        // Remembered, because it is why an answer of this server's own may
        // never come — and it is remembered *here*, where the refusal is
        // actually made, rather than inferred later from the configuration
        // that permits one.
        if let Some(conversation) = state.lsp.get_mut(language) {
            conversation.refused_its_question = true;
        }
        return refused(language, &asked, &message["params"]);
    }
    // Everything left is a reply, and a reply carries an id. Without one there
    // is nothing to match it to, which is the whole of what makes it a reply.
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

/// A question Varde will not answer, answered anyway.
///
/// The question is an array whose **first element is the tag its answer must
/// carry** — the asker holds one callback per tag and matches the two by it,
/// which is the whole of why the answer cannot simply be the method name. That
/// is the one thing read out of it: what the question actually asked is never
/// looked at, because there is nothing here that could answer it whatever it
/// said.
///
/// The nesting is the asker's own JSON-RPC library's, not a shape of ours: some
/// wrap an array of arguments in a further array and some do not, so the answer
/// goes back nested exactly as the question arrived. A question shaped like
/// neither is left alone — an answer tagged wrong is worse than none, since the
/// asker would match it to a callback nobody is waiting on.
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

/// The reply to `initialize`: what the server can do, and the go-ahead it must
/// hear before anything else.
fn handshaken(state: &mut State, language: &str, message: &Value) -> Vec<Effect> {
    // A handshake the server refused is a server that is not coming. Left as
    // "still starting" it is indistinguishable from one that has not answered
    // yet: nothing would ever be sent to it, and nobody would be told why.
    let Some(result) = message.get("result") else {
        return gone(state, language, Gone::FailedToStart);
    };
    let capabilities = result["capabilities"].clone();
    let Some(conversation) = state.lsp.get_mut(language) else {
        return Vec::new();
    };
    conversation.handshake = Handshake::Ready;
    conversation.capabilities = capabilities;
    // The open documents follow, from `sync` — this says only that the
    // handshake is over, which is what has to reach the server first.
    vec![Effect::LspSend {
        language: language.to_string(),
        json: notification("initialized", InitializedParams {}),
    }]
}

/// Whether the server declared one capability, read out of the `capabilities`
/// object it sent. The protocol lets every provider field be either a bool or
/// an options object, so all three answers are here: `false` is a refusal
/// rather than an absence, an object is a yes with details nothing reads, and a
/// server that says nothing about it is not asked.
///
/// The one field, not the whole reply: deserialising `ServerCapabilities`
/// wholesale means any *other* field the crate cannot type — a server that
/// spells `textDocumentSync` as a string, say — turns hover off, silently, for
/// a server that plainly declared it. Reading the field asked about can only be
/// wrong about the field asked about, which is where the ADR says a
/// non-conformant server is handled. That is also why this is three arms rather
/// than `lsp_types`' `HoverProviderCapability`: bool-or-object is the
/// protocol's own shape for every provider field, and a crate type per
/// capability would put the table back in two places.
fn declares(capabilities: &Value, capability: &str) -> bool {
    match &capabilities[capability] {
        Value::Bool(declared) => *declared,
        Value::Object(_) => true,
        _ => false,
    }
}

/// The characters one server named as the ones it wants to be told about, read
/// out of the capability it declared them in. Its own list and never Varde's,
/// which is the whole of this feature: measured, rust-analyzer asks about `.`,
/// `=`, `<`, `>`, `{`, `(`, `|` and `+`, jdtls about `;`, `}` and a newline,
/// and clangd about a newline alone. No two agree, and nothing in the table is
/// guessable from the language.
///
/// One character each, or nothing. The protocol's field is "a character", so a
/// string that is not one is not a character a server can have named — the
/// same reading [`snippet`] gives a placeholder form it does not understand,
/// and for the same reason: this is a string from a child process. That also
/// covers the provider declared as a bare `true`, which [`declares`] calls a
/// yes and which names nothing to ask about.
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

/// What a server says about one file, replacing whatever *that server* said
/// last. A push is the server's whole current opinion of that file, so its set
/// is replaced rather than added to — a gutter that accumulates is a log, not a
/// statement about the present — while the other servers serving the same file
/// keep theirs, since nothing in this push describes what they said.
///
/// An empty push is recorded as an empty set rather than forgotten. "The server
/// says this file is clean" and "the server has not spoken about this file" are
/// different facts, and Review view has to tell them apart to report a count of
/// zero rather than "not measured".
fn published(state: &mut State, language: &str, params: Value) {
    let Ok(params) = serde_json::from_value::<PublishDiagnosticsParams>(params) else {
        return;
    };
    let Ok(path) = params.uri.to_file_path() else {
        return;
    };
    // The stale-version drop. A reply naming a Document version the Buffer has
    // moved past describes text the reader has already edited away from: its
    // lines have shifted, so drawn as current it points at the wrong code.
    //
    // Only a Buffer can disagree. A file under review is told to the server
    // without being opened, so there is no revision for the version to be
    // measured against and nothing to be stale about.
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
                // The protocol lets a server omit it and leaves the reading to
                // the client. Unmarked is the loudest of the four rather than
                // the quietest: a diagnostic drawn as a hint is one a reader
                // scrolls past.
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
}

/// What the gutter marks one line of one file with — the worst of whatever sits
/// on it, since one line can carry several and the gutter has one column.
pub fn mark(state: &State, path: &Path, line: usize) -> Option<Severity> {
    worst(state, path, line).map(|diagnostic| diagnostic.severity)
}

/// The message for the diagnostic under the cursor of the buffer on screen, so
/// the reason for a mark is readable without leaving the pane. The same
/// `worst` the gutter asks, so the mark and the message cannot name different
/// diagnostics.
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

/// What is underlined on one line of one file: a column span per diagnostic
/// sitting there, 1-based and inclusive at both ends, with the severity that
/// colours it. A bar in the gutter says a line is wrong; this says which
/// characters are, which is the whole of what a reader of a line holding three
/// calls has to guess at otherwise.
///
/// Clamped to the characters the line holds, which is why it reads the buffer
/// rather than answering off the diagnostic alone: a server's range routinely
/// outruns the text — it ends on a later line, or it names a version the reader
/// has typed past — and a column past the end of a line is an underline under
/// nothing. A path with no buffer has nothing on screen to underline at all.
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
    // Before the line is read at all: this runs for every line the editor
    // draws, on every frame, and walking a file's characters to find one it has
    // nothing to say about is a walk for nothing.
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

/// The box beside the line the pointer rests on: what the server says about the
/// characters under the pointer, wrapped, and where the box goes. The reason a
/// mark is there, read without moving the cursor onto it.
///
/// Derived from where the pointer is and nothing else, so nothing has to
/// remember to take it down: moving off the span, or out of the editor
/// altogether, is already the answer changing. The worst of the diagnostics
/// under the pointer, the same `Ord` the gutter's one mark is picked with.
pub fn pointed(state: &State) -> Option<(Vec<String>, Placement)> {
    // Only while the editor is drawing the buffer's own lines. A Preview's rows
    // and a diff's rows are not lines, so a place read off one names a line
    // nobody is pointing at — the second meaning for one shape that
    // `Event::DragText` refuses for the same reason.
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
    // Per line of the message and not over the whole of it: a server writes a
    // multi-line message and `textwrap` does not read a newline as one, so the
    // rows would be counted short of what the box has to draw.
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
        // The span's own first column: the box belongs beside the characters it
        // is about, not at the margin the line happens to start at.
        column: from,
        width: measured(lines.iter().map(String::as_str)),
        rows,
    };
    Some((lines, placement))
}

/// Whether that language's server has been asked for the handshake and has not
/// answered. A reply to a handshake nobody is waiting for is a reply to a
/// request nobody made.
fn waiting(state: &State, language: &str) -> bool {
    matches!(
        state.lsp.get(language),
        Some(Conversation {
            handshake: Handshake::Sent,
            ..
        })
    )
}

/// The edge stopped holding a server. Told, never inferred: a conversation the
/// core went on holding would answer questions out of a conversation that ended.
///
/// Either way it is said out loud, and said once. A server that dies a second
/// after starting is, to the reader, the same event as one that never started —
/// no intelligence, and no way to know why — and the edge reports the loss from
/// every site that stops holding one, so being told twice about one loss must
/// not be two notices.
pub fn gone(state: &mut State, language: &str, why: Gone) -> Vec<Effect> {
    let told = written_off(state, language);
    // A fresh conversation rather than a flag on the old one: what it was
    // holding is exactly what must not outlive the server — every request sent
    // and not answered included, since nothing is coming back to match them.
    let mut written_off = Conversation::new(Handshake::Gone);
    // What the write-off was *for*, which is what says whether it can ever be
    // forgotten. Both halves are load-bearing, and each one is a loop the other
    // does not stop:
    //
    // - A handshake refused by a process the edge is **still holding** is a
    //   server that is there and does not work, and no probe will change that.
    //   `lsp_running` is the difference, and it is the edge's own fact rather
    //   than a second reason of ours.
    // - A spawn that failed while the probe **had already found** the command
    //   is a command that exists and cannot run — a wrong architecture, a bad
    //   interpreter line, an executable bit `which` was happy with. Forgetting
    //   that on the strength of the probe finding what it already found is an
    //   unbounded respawn loop: the write-off is dropped, `sync` spawns, the
    //   spawn fails, and the loss comes straight back round. So the write-off
    //   is forgettable only for a command nothing could find at the time
    //   (R31.24).
    let missing = state
        .servers
        .get(language)
        .map(|server| server.command.clone())
        .filter(|command| !state.commands_on_path.contains(command));
    if why == Gone::FailedToStart && !state.lsp_running.contains(language) {
        written_off.absent = missing;
    }
    state.lsp.insert(language.to_string(), written_off);
    // Its marks go with it. A gutter still carrying errors from a server that
    // is no longer running describes a file as it was at some unknown moment
    // and answers for nothing since — the same lie a stale version is, without
    // even a version to catch it by.
    //
    // Its own, exactly: this used to drop every mark on every file the dead
    // server *served*, because a map keyed by path alone could not say which of
    // two publishers wrote what — so a Vue server dying took the TypeScript
    // server's type errors with it. A path left with no publisher at all stops
    // being a path anything has spoken about, which is what keeps Review view
    // reading "not measured" rather than announcing a dead server's file clean.
    state.diagnostics.retain(|_, by_language| {
        by_language.remove(language);
        !by_language.is_empty()
    });
    // And so does anything it said about a symbol. ADR 0011 asks for exactly
    // this: a server that stops invalidates the state the core is holding *on
    // screen*, and a box quoting a conversation that ended is the same claim
    // about the present the marks above were.
    if state
        .hover
        .as_ref()
        .is_some_and(|hover| serves(state, language, &hover.asked.path))
    {
        state.hover = None;
    }
    // And so does its part of every question it was asked. A question whose
    // last outstanding server has died is answered by nobody, which is an
    // answer — R31.7's stale-version rule and ticket 19's refuse-out-loud rule,
    // applied to a set instead of an id. Left in place it would wait forever:
    // nothing is coming back to empty it.
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
    // The loss is the more specific thing to say when it is news, and it says
    // everything the empty hand would: a server that died did not decline to
    // answer. Once the reader has already been told, what is left is the
    // question they pressed a key for.
    if told {
        return answers;
    }
    // Nothing is started in its place, so the notice is also the whole of what
    // the reader gets: the conversation above is what stops the next open file
    // from asking for the same dead command again.
    match why {
        Gone::FailedToStart => vec![Effect::Notify("language-server-failed")],
        Gone::Exited => vec![Effect::Notify("language-server-stopped")],
    }
}

/// One configured string with the edge's answers in it, or nothing when a name
/// it carries has none.
///
/// Dropping is the whole of the decision: the caller drops the argument or the
/// option key that carried the name, because `--tsdk=` reaching a server is a
/// request to load an SDK from the root directory — a wrong answer where no
/// answer leaves the server's own fallback in place. This is the last line
/// rather than the gate — [`sync`] refuses to start a server whose requirement
/// is unmet at all — and it stays because the edge re-probes on its own clock:
/// a fact can vanish between the spawn and the handshake, and an empty
/// `--tsdk=` reaching the server then is worse than nothing.
///
/// Only names a `[facts.*]` table declared are touched: this is not a template
/// language and not an environment-variable escape, so `${HOME}` is a string a
/// server was configured with and is passed through exactly as written.
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

/// The same for the options table, which is a tree rather than a string
/// because `typescript-language-server` wants the SDK at
/// `initializationOptions.tsserver.path` while the Vue server wants it in
/// `args`. Nothing here reads a key — the walk is over values, and a key whose
/// value asked for a fact nobody could find goes with it, for the reason a
/// dropped argument does (R31.26 still holds: no option's own name appears
/// anywhere in `src/`).
///
/// **This is the one level where a sibling survives a dropped key.** Below it
/// [`filled_value`] drops the whole container, and the difference is the point:
/// one option a fact could not fill must not take the rest of the table with
/// it, while half of one option is a value no server was configured with.
fn filled_options(
    options: &serde_json::Map<String, Value>,
    facts: &BTreeMap<String, Option<String>>,
) -> serde_json::Map<String, Value> {
    options
        .iter()
        .filter_map(|(key, value)| Some((key.clone(), filled_value(value, facts)?)))
        .collect()
}

/// Below the top level a container that lost a part is dropped whole, rather
/// than sent with a hole in it. The top level keeps its siblings — one option a
/// fact could not fill must not take the rest of the table with it — but a
/// plugin entry that kept its name and lost the directory it lives in is a
/// server told to load something from nowhere, and an array that quietly lost
/// an entry is a list whose length nobody configured.
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

/// Which fact a server's configuration asks for that this workspace has no
/// answer to *and cannot start without*, if any — read off the text the message
/// would be built from, so an argument and an option are the same question.
/// What the row says, and what [`sync`] refuses to spawn against.
///
/// A fact declared `optional` is not one of them. Nothing here decides that: the
/// `[facts.*]` table says so, and this reads it — a server that is merely
/// better with something is a server that starts without it, and the key naming
/// it goes the way [`filled_options`] already sends it.
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

/// Every fact configuration declared, against what the edge found for it — or
/// `None` where this workspace has no answer. The two halves are joined here
/// because they are two different kinds of thing: which names exist is the
/// library's, read off the `[facts.*]` tables, and what each one is *here* is
/// the edge's, written by `tell_core` and never by `update` (R31.23). Together
/// they are what tells a placeholder nobody could resolve from a `${...}` that
/// was never a placeholder at all.
pub(crate) fn facts(state: &State) -> BTreeMap<String, Option<String>> {
    state
        .facts
        .keys()
        .map(|name| (name.clone(), state.workspace_facts.get(name).cloned()))
        .collect()
}

/// What a server is told in `initializationOptions`: its own row's options and
/// the `plugin` of every Debug adapter it hosts, each filled on its own and then
/// joined — so a plugin whose fact was not found goes alone, rather than taking
/// the server's own entry under the same key with it.
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

/// A table merged key by key and a list added to, so neither of two rows
/// passing one server options has to know what the other passes.
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
        // The client's own pid is the edge's to know, and null is what the
        // protocol has for a client that does not say.
        process_id: None,
        root_uri: uri(root),
        // One claim, and it is the one Varde acts on: diagnostics are drawn in
        // the gutter (R31.8), and a server may hold them back from a client
        // that never said it could receive them —
        // `typescript-language-server` does exactly that, which reads as a
        // clean file rather than as a server saying nothing. Everything else
        // Varde asks for it asks for by name, so announcing it here would only
        // invite a server to send what nobody reads.
        capabilities: ClientCapabilities {
            text_document: Some(TextDocumentClientCapabilities {
                publish_diagnostics: Some(PublishDiagnosticsClientCapabilities::default()),
                // The second claim, and the one that is a promise rather than a
                // request: without it the protocol requires a server to send
                // plain text, so a `${1:…}` reaching the buffer as the
                // characters it is spelled with is Varde's defect and not the
                // server's. Declared in the same commit as [`snippet`] and the
                // stops it hands to `Modal::Stops`, never before.
                completion: Some(CompletionClientCapabilities {
                    completion_item: Some(CompletionItemCapability {
                        snippet_support: Some(true),
                        ..CompletionItemCapability::default()
                    }),
                    ..CompletionClientCapabilities::default()
                }),
                // The third, and a promise of the same kind: without it the
                // protocol lets a server pick either format, and several pick
                // markdown regardless — which the box drew as the characters
                // it is spelled with for as long as it flattened a reply into
                // one string. Declared in the same commit as the rendering,
                // never before, exactly as the snippet claim was. Markdown
                // first because it is preferred, plaintext kept because a
                // server that has only that is not a server to refuse.
                hover: Some(HoverClientCapabilities {
                    content_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    ..HoverClientCapabilities::default()
                }),
                ..TextDocumentClientCapabilities::default()
            }),
            ..ClientCapabilities::default()
        },
        // Whatever configuration put there, with the edge's answers in place
        // of the facts it named and nothing else decided here: the table
        // arrives in the shape the message wants and is not read on the way,
        // which is what keeps Varde from holding an opinion about any key
        // inside it (R31.26). Unlike a capability this is not a claim about what
        // Varde can do — it is how a server is told where its own toolchain
        // lives, and several will not run without it. A language that
        // configured none says nothing.
        initialization_options: options,
        ..InitializeParams::default()
    }
}

/// The protocol counts versions in a signed 32-bit number and a revision is
/// unbounded, so the arithmetic is pinned rather than left to wrap: a version
/// that went negative would read as older than everything the server has.
fn document_version(revision: u64) -> i32 {
    i32::try_from(revision).unwrap_or(i32::MAX)
}

fn uri(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

/// A command put to `language`'s server — how a Debug adapter it hosts is
/// started — and the id its answer comes back under. `None` until that server
/// has finished its handshake, since it reads nothing sent before then.
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

    /// A hosted adapter's plugin joins what the server's own row passes it,
    /// each filled on its own: a plugin whose fact was not found goes without
    /// taking the server's entry under the same key with it.
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

    /// The last line, not the gate. [`sync`] refuses to spawn a server whose
    /// requirement is unmet, so at the system level the empty argument can no
    /// longer be sent — but the edge re-probes on its own clock, and a fact can
    /// vanish between the spawn and the handshake. `--tsdk=` reaching the
    /// server then is a request to load an SDK from the root directory, worse
    /// than the fallback it replaces, so the argument goes with it and so does
    /// the option key that carried it (R31.27). Held here rather than by a
    /// scenario for exactly that reason.
    ///
    /// And a fact declared `optional` reaches this same last line: what the
    /// declaration changes is [`sync`]'s gate, never the value, so a name with
    /// no answer is dropped here whichever kind of fact it was.
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
        // A name no `[facts.*]` table declares is not a placeholder at all.
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
        // A container below the top level goes whole rather than with a hole in
        // it. `{"name": …}` with no location is a plugin the server is told to
        // load from nowhere, and an array one entry shorter is a list whose
        // length nobody configured — so the key that held the array goes, and
        // the option beside it stays.
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
                // The only two languages these tests open files in.
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

    /// A pass, with the edge's half of it played out: whatever the pass asked
    /// to be started, the edge comes to hold — and it is holding the process
    /// that makes the pass after it open a conversation. Two passes rather than
    /// one because that is what happens in the binary, where the spawn is an
    /// effect and `Event::LspStarted` is what brings the second pass around.
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

    /// A workspace whose changed files are on screen in Review view, with a
    /// server the edge is already holding for them — the shape the review
    /// counts are read off, and the one nothing in a Buffer covers.
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

    /// Which files a pass asked the edge to read, so the same file asked for
    /// twice is visible as two.
    fn reads(effects: &[Effect]) -> Vec<PathBuf> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::ReadForReview(path) => Some(path.clone()),
                _ => None,
            })
            .collect()
    }

    /// The read is not instant, and every event in the meantime runs a pass.
    /// Asking again for a file already in flight is how one changed file comes
    /// to be opened at the server several times over.
    #[test]
    fn a_file_being_read_for_the_review_is_not_asked_for_again() {
        let mut state = reviewing(&["src/lib.rs"]);
        assert_eq!(
            reads(&pass(&mut state)),
            vec![state.root.join("src/lib.rs")]
        );
        assert_eq!(reads(&pass(&mut state)), Vec::<PathBuf>::new());
    }

    /// Everything the read was asked under can have moved while it was in
    /// flight. A Buffer owns the document from the moment it opens — at its own
    /// revision, which this would be sending the server backwards from.
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

    /// A file that leaves the review is a document the server still believes is
    /// open. Told, for the reason a closed Buffer is: nothing else will correct
    /// it, and a version starting over is a version going backwards.
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

    /// Nothing is spawned for a change nobody opened: a keystroke that launches
    /// one process per changed language is a side effect nobody asked for.
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

    /// The absence the whole feature is measured against: a language nothing
    /// configures asks the edge for nothing at all.
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

    /// A conversation begins against a process, so a server the edge is already
    /// holding is spoken to rather than started over — whether it came to hold
    /// it a moment ago for this very buffer, or before any of this.
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

    /// The reply is what lets the document go — and it goes without any arm
    /// remembering to send it, which is what makes the first file opened in a
    /// session reach the server at all.
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

    /// `update` calls itself for a few actions (a pane action that means another
    /// event), so the pass runs more than once for one event. What was sent is
    /// recorded in the state the inner call returns, which is what stops the
    /// outer one from saying it all again.
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

    /// A server that refuses the handshake is a server that is not coming, and
    /// left as "still starting" it is indistinguishable from one that has not
    /// answered yet: nothing would ever be sent to it and the reader would
    /// never be told why.
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

    /// A server asks Varde things too — to register a capability, to put a
    /// progress bar up — and a request carries an id because its sender is
    /// waiting for an answer. Varde implements none of them, and says so: the
    /// same rule `queries::reply` follows for a child that asks the terminal a
    /// question it will not answer.
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

    /// A question Varde cannot answer carries no tag it could answer under, so
    /// there is nothing to say back that the asker could match to anything. Both
    /// shapes of that are the same silence, and both are a question the server
    /// asked in a shape its own configuration does not describe — an answer
    /// tagged out of thin air would be matched to a callback nobody is waiting
    /// on, which is worse than the wait it would end.
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

    /// A notification is not a question, so it is not answered — a reply to one
    /// is a message the server has no request open for.
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

    /// The server is told what closed, not merely forgotten: it still believes
    /// the document is open at the version it last heard, and the tree's own
    /// preview opens and closes a Buffer per file arrowed past.
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
        // And opening it again is an open again: a fresh Buffer is at its first
        // revision, so a version remembered from the closed one would be a
        // version going backwards — or, as it did, a version that matched and
        // so said nothing at all.
        open(&mut state, "src/lib.rs", "fn main() {}");
        assert_eq!(methods(&pass(&mut state)), vec!["textDocument/didOpen"]);
    }

    /// A message that answers nothing says nothing about the handshake — a
    /// server that talks before it is ready cannot make itself ready.
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

    /// A push names a file and Varde may have no Buffer for it: Review view
    /// tells a server about every changed file without opening one. There is no
    /// revision for the version to disagree with, so there is nothing stale
    /// about it — dropped, every count in the review would read as unmeasured.
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

    /// The gutter has one column and a line can carry several diagnostics. The
    /// worst is the one worth a column: a line whose error is hidden behind a
    /// hint reads as a line with nothing much wrong.
    #[test]
    fn a_line_carrying_several_is_marked_with_the_worst() {
        let mut state = workspace("rust", "rust-analyzer");
        let path = state.root.join("src/lib.rs");
        received(&mut state, "rust", &publish(&path, None, &[(3, 4), (3, 1)]));
        assert_eq!(mark(&state, &path, 3), Some(Severity::Error));
        assert_eq!(message_at_cursor(&state), None, "no buffer, no cursor");
    }

    /// What the underline covers when the server's range does not fit the line:
    /// a range ending on a later line has no column of its own here, and a
    /// zero-width range — what a server sends for a token that is missing —
    /// has no width at all. Both are drawn under characters that exist,
    /// because a column past the end of a line is an underline under nothing.
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

    /// A Preview's rows are not the file's lines, so where the pointer rests
    /// names nothing a diagnostic can be about — the box stays down rather
    /// than being drawn beside a row that happens to carry the same number.
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

    /// The protocol counts characters from zero and ends a range one past the
    /// last of them; the underline is 1-based and inclusive at both ends. A
    /// range that ends on a later line keeps no end column: the line's own end
    /// is where the underline stops, and no diagnostic carries a line's length.
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

    /// One `publishDiagnostics` notification, from lines paired with the
    /// protocol's own severity numbers.
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

    /// The write-off that cannot be forgotten, which is the whole reason
    /// [`Conversation::absent`] holds a command rather than a flag. A server
    /// that started and then exited was never missing, so its command being on
    /// `PATH` explains nothing — and dropping the conversation on the strength
    /// of it is exactly the respawn loop the write-off exists to stop. No
    /// scenario reaches this: the Rule's scenarios are about a command that
    /// *appeared*.
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

    /// A row's two questions in the one order that matters. A command that is
    /// here and has been watched to die is `stopped` — the state that could not
    /// be said while `installed` meant a path exists — but a requirement this
    /// workspace cannot meet is said *ahead* of it, because that is the thing
    /// the reader can fix and it names itself, where `stopped` names nothing.
    /// No scenario pins the order: each of the two is a scenario of its own, and
    /// which wins when both hold is a decision inside one match.
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
        // Which names are facts is configuration's to say, so a workspace that
        // declares none has no `${typescript_sdk}` to be missing.
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

    /// The same for a handshake a *running* server refused: the edge is holding
    /// the process, so `FailedToStart` there means the server is here and does
    /// not work. `lsp_running` is what tells the two apart.
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

    /// The loop the second half of that condition stops: a command the probe
    /// found, that then failed to spawn — a wrong architecture, a bad
    /// interpreter line. Forgetting it on the strength of the probe finding
    /// what it had already found drops the write-off, spawns, fails, and comes
    /// straight back round, without a keystroke between. Held here because the
    /// scenarios are all about a command that was *not* there.
    #[test]
    fn a_command_that_is_there_and_cannot_run_is_written_off_for_good() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        state.commands_on_path.insert("rust-analyzer".to_string());
        // The pass asks for it, the spawn fails, and the edge never held a
        // process — which on its own would read as "the command was missing".
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

    /// And the write-off that *is* forgotten, asserted on the conversation
    /// rather than on the spawn the scenario already covers: what the probe
    /// drops is the whole conversation, so the pass after it is a fresh start
    /// with nothing remembered.
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

    /// A server that dies is not allowed to die quietly: the reader is looking
    /// at a file nothing is answering for, and a real run of Varde against a
    /// `rust-analyzer` shim that exited a second after starting said nothing at
    /// all.
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

    /// One loss, however many sites report it: the edge tells the core wherever
    /// it stops holding a server, and a write to a server it no longer holds is
    /// one of those sites.
    #[test]
    fn one_loss_is_one_notice() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "src/lib.rs", "fn main() {}");
        pass(&mut state);
        gone(&mut state, "rust", Gone::FailedToStart);
        assert_eq!(gone(&mut state, "rust", Gone::Exited), Vec::new());
    }

    /// Losing a server is told, and being told is what stops the next open file
    /// from asking for the same missing command again.
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

    /// The column the protocol wants is counted in UTF-16 code units, and an
    /// emoji is two of them where it is one character. Counting characters asks
    /// about the symbol next door, which is a hover for the wrong thing rather
    /// than a hover that fails.
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
        // Past the emoji: four characters before it, then its two units.
        assert_eq!(
            position(text, Place { line: 1, column: 7 }),
            Position {
                line: 0,
                character: 7
            }
        );
    }

    /// A server that says it does not do hover is a server that is not asked,
    /// and `false` is a refusal rather than an absence. Read per capability, so
    /// one declined leaves the others alone — a server that answers definitions
    /// and not hover is a real server, not a broken one.
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

    /// And a question the server declined is not sent — it is refused out loud,
    /// under the slug that names which question it was. Only the notice is
    /// unit-only: which slug a declined capability answers with is not
    /// something a Scenario names.
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

    /// The protocol's coordinates read back, and the round trip is the point:
    /// an emoji is two UTF-16 units and one character, so a reply's column
    /// counted as characters lands one place to the right of the name for every
    /// line that holds one. Exact whenever the file is already open, which is
    /// the only time the core has the text the count needs.
    #[test]
    fn a_column_read_back_is_the_column_that_was_sent() {
        let text = "let 🦀 = one;";
        let at = Place { line: 1, column: 7 };
        let sent = position(text, at);
        assert_eq!(sent.character, 7, "the units the server was given");
        assert_eq!(place(Some(text), sent), at);
    }

    /// The known limit, pinned so it is a decision and not a surprise: with no
    /// text there is nothing to count units against, so they are counted as
    /// characters. Exact for every ASCII line — which is what the second half
    /// shows — and one place to the right per non-ASCII character before the
    /// column, which is what the first half shows. Getting it exact means
    /// carrying the unit column through the read and resolving it against the
    /// Buffer, which is a wider change than any scenario asks for; the Buffer
    /// clamps what it is handed either way, so the cursor lands on the right
    /// line and never off the end.
    #[test]
    fn a_column_in_a_file_not_open_is_counted_in_code_units() {
        let sent = position("let 🦀 = one;", Place { line: 1, column: 7 });
        assert_eq!(place(None, sent), Place { line: 1, column: 8 });
        let ascii = position("let one = two;", Place { line: 1, column: 7 });
        assert_eq!(place(None, ascii), Place { line: 1, column: 7 });
    }

    /// Which query the list is drawn under. That the hits are the places, and
    /// that nothing is opened, is the Scenario's ("Several definitions are
    /// listed rather than the first being taken quietly"); the query is not
    /// something it names, and an empty one shows an empty search box and
    /// selects no characters.
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

    /// A reply naming places on both sides of the root keeps the ones inside
    /// rather than refusing the lot: the reader asked to be taken somewhere,
    /// and one place in the workspace is somewhere to go. The Scenario covers
    /// a reply with nothing inside it at all; only the straddling case is
    /// unit-only, since no Scenario names a server that answers with both.
    ///
    /// The escape sequence in the path is the second half: a path off a child's
    /// stdio reaches the status line, and raw ANSI there rewrites the screen.
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

    /// What one reply came to, for the tests that drive a reply rather than a
    /// question. `Nothing` is the empty hand, and what is *said* about it
    /// belongs to the question rather than to the reply — [`empty_handed`],
    /// once every server has had its turn.
    fn acted(told: Told) -> Vec<Effect> {
        match told {
            Told::Something(effects) => effects,
            Told::Nothing => Vec::new(),
        }
    }

    /// A definition reply's locations, in the protocol's own shape.
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

    /// Below the line it describes while there is room, above it when there is
    /// not, and never on it — which is the whole of the promise, so both
    /// branches are driven.
    #[test]
    fn the_box_sits_beside_the_line_it_describes_never_on_it() {
        // Two lines, so four rows once the border is counted.
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
        // Near the bottom the four rows do not fit below, so the box goes
        // above — and its own bottom border must still leave the line alone,
        // which counting only its lines did not.
        let above = boxed(19);
        assert_eq!(boxed_at(&above), 15);
        assert!(!covers(&above, 19) && covers(&above, 18));
    }

    /// A ready server with everything declared, for the questions below —
    /// built the way one becomes ready, through the handshake reply.
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

    /// A word being typed is what asks — read off the text rather than the
    /// keystroke, so a backspace inside the word asks again about what is
    /// left, and `sync` (which sees every change alike) asks for nothing at
    /// all.
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
        // The pass that tells the server what the buffer holds asks for
        // nothing: it cannot tell a keystroke from a file reloaded underneath.
        assert_eq!(
            pass(&mut state)
                .iter()
                .filter(|effect| matches!(effect, Effect::DebounceCandidates(_)))
                .count(),
            0
        );
    }

    /// Accepting closes the list and leaves it closed. It bumps the Buffer's
    /// revision like any other edit, and while *that* was what armed the
    /// debounce a second list came up over the word the first had just
    /// completed, 120 ms later — a completion the reader has to dismiss to get
    /// back to what they chose. Driven through `update`, because the arm that
    /// accepts and the pass that tells the server are both in it.
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
        // The server is still told what the buffer now holds — it is the
        // asking that must not happen again.
        assert_eq!(methods(&effects), vec!["textDocument/didChange"]);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::DebounceCandidates(_))),
            "accepting asked for a second list: {effects:?}"
        );
    }

    /// A character an identifier cannot hold ends the word, so a list still up
    /// goes with it rather than standing over the next one — and nothing is
    /// asked about a space.
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

    /// A server that offers no completion is never waited for: an armed timer
    /// is a redraw when it fires, and a frame drawn for a question nobody
    /// answers is a frame drawn for nothing.
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

    /// The debounce fires after the reader closed the file, or after the server
    /// went: nobody pressed a key for this, so there is nothing to say about it
    /// — a notice per pause in a file no server serves is the feature
    /// announcing itself constantly to say nothing.
    #[test]
    fn a_question_nobody_asked_for_is_refused_in_silence() {
        let mut state = workspace("rust", "rust-analyzer");
        open(&mut state, "legacy/report.cob", "DISPLAY 'HI'.");
        state.current_buffer = Some(state.root.join("legacy/report.cob"));
        assert_eq!(ask(&mut state, About::Candidates), Vec::new());
        // The same absence, said out loud for the key that was pressed.
        assert_eq!(
            ask(&mut state, About::Hover),
            vec![Effect::Notify("no-language-server")]
        );
    }

    /// A server that does not offer completion is not asked for it, and is not
    /// complained about either: the question was typing, not a keystroke.
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

    /// The list is offered, never imposed: a reply landing while something the
    /// reader opened is on screen leaves it there. Typing does not stop for a
    /// palette, so the reply for the last thing typed can arrive behind one.
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

    /// What a server means by "insert this" when it says nothing else is the
    /// label — and when it does say, what it says is what goes in, since a
    /// server offering `push(…)` to read means `push` to type.
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
        // Both other shapes: the list form a server may send instead, and the
        // `null` that is neither.
        assert_eq!(candidates(&json!({"isIncomplete": false, "items": []})), []);
        assert_eq!(candidates(&Value::Null), []);
    }

    /// The box the wrapping produces has to fit the screen it is drawn on,
    /// which is the whole of "not drawn clipped": the box is sized from the line
    /// count, so that count is only honest if no line is wider than there is
    /// room for. Nothing narrower than the floor `layout::overlay` already
    /// applies to every other overlay, since that is a screen neither can draw
    /// a box on.
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
            // The two columns the border sits on, plus the text: the width `ui`
            // gives the box.
            assert!(
                widest + 2 <= width,
                "a {width}-column screen cannot draw {widest} of text"
            );
        }
    }

    /// A refused question is a failure, not the server saying it knows nothing:
    /// reported as the second, the reader is told something false about their
    /// code rather than something true about the server.
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

    /// A box quoting a conversation that has ended is the claim about the
    /// present ADR 0011 says a dead server's state must not go on making.
    #[test]
    fn a_server_that_stops_takes_its_box_with_it() {
        let mut state = asking();
        received(&mut state, "rust", &reply(json!({"contents": "fn main()"})));
        assert!(state.hover.is_some(), "no box to lose");
        gone(&mut state, "rust", Gone::Exited);
        assert_eq!(state.hover, None);
    }

    /// The box is a claim about one place in one file, and `hover_stands` is
    /// what `update` asks on every pass — so a cursor that moved, and a file
    /// that is no longer the one on screen, both take it down.
    #[test]
    fn a_box_stands_only_while_it_describes_what_is_under_the_cursor() {
        let mut state = asking();
        received(&mut state, "rust", &reply(json!({"contents": "fn main()"})));
        assert!(hover_stands(&state));
        let path = state.current_buffer.clone().expect("a buffer");
        state.buffers.get_mut(&path).expect("a buffer").column += 1;
        assert!(!hover_stands(&state), "it stood after the cursor moved");
    }

    /// A workspace whose server is ready, one buffer open and focused, and a
    /// hover already asked for — the state every reply below arrives into.
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

    /// A `.vue` file with two servers on it, both ready, both saying they
    /// answer both questions. Configuration says so — the file's own language
    /// names who else serves its files — which is why no arm below names a
    /// language and this one is data.
    fn served_twice() -> State {
        let mut state = workspace("vue", "vue-language-server");
        // The same configured row under a second name: what these tests turn
        // on is that two servers serve one file, never what either command is.
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

    /// What a box on screen reads as, rows joined by a space: a scenario and a
    /// test both care what it says, never which row a wrap put a word on.
    fn said(hover: &Hover) -> String {
        hover
            .lines
            .iter()
            .map(preview::Row::text)
            .collect::<Vec<String>>()
            .join(" ")
    }

    /// A reply from one of them. Every conversation counts its own ids from
    /// the handshake's, so the one question went out as the same id twice.
    fn reply(result: Value) -> String {
        json!({"jsonrpc": "2.0", "id": INITIALIZE + 1, "result": result}).to_string()
    }

    /// One keystroke, every server that serves the file. The whole point of the
    /// arbitration below, and the shape `lsp::language` could not express: a
    /// `.vue` file is served by the Vue server *and* by a TypeScript one.
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
        // The file's own language first, which is the order [`served_by`]
        // resolves them in.
        assert_eq!(asked, vec!["vue".to_string(), "typescript".to_string()]);
        assert_eq!(outstanding(&state, "vue"), 1);
        assert_eq!(outstanding(&state, "typescript"), 1);
        // And both were told about the document, or there would be nothing to
        // ask them about.
        for language in ["vue", "typescript"] {
            assert!(
                state.lsp[language]
                    .sent
                    .contains_key(&state.root.join("src/App.vue")),
                "{language} was never told the document is open"
            );
        }
    }

    /// The first non-empty answer is the answer. A second jump would move the
    /// cursor off where the first one took it, and a second hover would replace
    /// a real answer with whichever server was slower.
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

    /// The reported defect, in one test: the server with no answer had less to
    /// look up, so it replies first, and letting it speak for the rest is a key
    /// that says nobody knows while somebody is still looking.
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

    /// And when they all answer empty, once — not once per server.
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

    /// A refusal is empty-handed for the arbitration too: a server that would
    /// not answer must not beat one that will.
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

    /// A question whose last outstanding server has died is answered by nobody,
    /// which is an answer. Left in place it would wait forever: nothing is
    /// coming back to empty it.
    #[test]
    fn a_server_dying_mid_question_does_not_leave_it_outstanding() {
        let mut state = served_twice();
        ask(&mut state, About::Definition);
        received(&mut state, "vue", &reply(Value::Null));
        // The loss is what the reader is told, since it is news and it explains
        // everything the empty hand would.
        assert_eq!(
            gone(&mut state, "typescript", Gone::Exited),
            vec![Effect::Notify("language-server-stopped")]
        );
        assert_eq!(outstanding(&state, "typescript"), 0);
        assert!(state.lsp_asked.is_empty(), "the question is still waiting");
    }

    /// One server, configured to relay a question Varde will not answer, and a
    /// buffer open at the cursor.
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

    /// The question the server needed answered, put and refused — which is why
    /// no answer of its own is coming. "The language server knows of no
    /// definition" is then false, and it is the sentence that sent a reader
    /// looking at their own install instead of at Varde (R31.25).
    #[test]
    fn a_question_varde_declined_to_relay_says_so_rather_than_blaming_the_server() {
        for about in [About::Hover, About::Definition] {
            let mut state = relaying_server();
            // Where it happens: the server asks on `didOpen`, long before the
            // key is pressed, and never asks again. Measured on the wire —
            // which is why the flag is the conversation's and not the
            // question's.
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

    /// And **only** then. A server that could have relayed and did not is a
    /// server that knows nothing about this symbol, and saying otherwise puts a
    /// second false sentence exactly where the first one was — every empty hand
    /// in the file blamed on a companion, with the cursor on whitespace as
    /// much as on an unknown name.
    #[test]
    fn a_server_that_could_relay_and_did_not_knows_nothing_rather_than_needing_anybody() {
        let mut state = relaying_server();
        ask(&mut state, About::Definition);
        assert_eq!(
            received(&mut state, "vue", &reply(Value::Null)),
            vec![Effect::Notify("no-definition")]
        );
    }

    /// An error is empty-handed for the arbitration, and the empty hand still
    /// says which kind of nothing it was: reporting a refusal as ignorance is
    /// an error surfacing as a domain answer. It is the *late* order that is
    /// easy to lose — the refusal arrives first and the last word is somebody
    /// else's.
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

    /// Which servers serve a path is data, and a name nothing configures is
    /// nobody to ask.
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

    /// Wrapped before it is measured, and a reply saying nothing is nothing
    /// rather than an empty box.
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

    /// The shape no scenario drives: a server may answer with a *list* of
    /// pieces, and each one is read as what it is rather than the list being
    /// glued into one document — a bare string is markdown, a named language
    /// is a block of that language, and both end up in the same box.
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

    /// The ticket's own example, and the whole of what the parser is for: the
    /// placeholders gone from the text, and the places they were in coming
    /// back as the order Tab visits them.
    #[test]
    fn a_snippet_is_the_text_it_inserts_and_the_stops_left_in_it() {
        assert_eq!(
            snippet(r#"println!("${1:msg}")$0"#),
            ("println!(\"msg\")".to_string(), vec![10, 15])
        );
    }

    /// A stop with no text of its own is a place and nothing else, which is
    /// what an argument list is made of.
    #[test]
    fn a_stop_with_no_default_leaves_no_text_behind() {
        assert_eq!(
            snippet("foo($1, $2)$0"),
            ("foo(, )".to_string(), vec![4, 6, 7])
        );
    }

    /// Tab runs forwards through the text whatever the numbers say: a server
    /// that numbers its stops backwards through its own snippet gets them
    /// visited in the order they are written, because the alternative is a Tab
    /// landing inside the blank just filled in. See [`snippet`].
    #[test]
    fn the_stops_are_visited_in_the_order_they_appear_in_the_text() {
        assert_eq!(
            snippet("${2:b} ${1:a}$0"),
            ("b a".to_string(), vec![0, 2, 3])
        );
    }

    /// `$0` is where the code continues, so a snippet that named no place for
    /// it means after everything it inserted — otherwise the last Tab of every
    /// such completion would leave the cursor inside the call.
    #[test]
    fn a_snippet_naming_no_final_stop_ends_at_one() {
        assert_eq!(snippet("foo($1)"), ("foo()".to_string(), vec![4, 5]));
        assert_eq!(snippet("worklist"), ("worklist".to_string(), vec![8]));
    }

    /// The trust boundary: the reply is a string from a child process, and a
    /// form Varde cannot resolve is text rather than syntax. A choice and a
    /// variable are both left exactly as they were written, and neither
    /// becomes a stop.
    #[test]
    fn a_placeholder_form_varde_does_not_understand_is_text() {
        assert_eq!(
            snippet("${1|a,b|} $TM_FILENAME"),
            ("${1|a,b|} $TM_FILENAME".to_string(), vec![22])
        );
    }

    /// A reply is a string from a child process, so a placeholder that is
    /// never closed is a placeholder that never was: it stays as the characters
    /// it is made of rather than swallowing the rest of the line into a
    /// default nobody wrote.
    #[test]
    fn a_placeholder_that_is_never_closed_is_text() {
        assert_eq!(snippet("${1:oops"), ("${1:oops".to_string(), vec![8]));
        assert_eq!(snippet("${1"), ("${1".to_string(), vec![3]));
    }

    /// Both escapes, and the reason the second is not optional: a backslash
    /// that cannot itself be escaped is a backslash no server can write before
    /// a stop, because `\\$1` would eat the stop instead.
    #[test]
    fn an_escaped_dollar_is_a_dollar_and_not_a_stop() {
        assert_eq!(
            snippet(r"cost \$${1:5}$0"),
            ("cost $5".to_string(), vec![6, 7])
        );
        assert_eq!(snippet(r"path \\$1$0"), (r"path \".to_string(), vec![6, 6]));
    }

    // `features/language_intelligence.feature`, "The server reformats as it is
    // typed".

    /// A conversation with a server that named its own trigger characters, and
    /// the file open with them not yet typed. The capability is spelled here
    /// rather than in a helper because the characters *are* the subject: they
    /// are the server's, and nothing in Varde may know them.
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

    /// The params of the one request that method carries, for the assertions
    /// that are about what went out rather than about how many did.
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

    /// Where the cursor is, so a Scenario's `Given` and a unit test's setup are
    /// the same two lines.
    fn typing_at(state: &mut State, line: usize, column: usize) {
        let path = state.current_buffer.clone().expect("a buffer");
        let buffer = state.buffers.get_mut(&path).expect("the buffer");
        buffer.key('i');
        buffer.go_to_place(Place { line, column });
    }

    /// Every character the server named, in the two places the protocol lets it
    /// name them. A list Varde could not have guessed: measured, rust-analyzer
    /// asks about `.` and `=`, jdtls about `;`, `}` and a newline, and clangd
    /// about a newline alone.
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

    /// A capability declared as a bare `true` names no characters, so it asks
    /// about none: the protocol has no such shape for this provider, and
    /// guessing which characters a server meant is the invention this feature
    /// refuses. Nor is a string that is not one character one — the same
    /// reading the snippet parser gives a placeholder it does not understand.
    #[test]
    fn a_capability_that_names_no_character_names_none() {
        assert_eq!(triggers(&json!(true)), Vec::<char>::new());
        assert_eq!(
            triggers(&json!({"firstTriggerCharacter": ";", "moreTriggerCharacter": ["", ">="]})),
            vec![';']
        );
    }

    /// The request goes out **after** the change it is about. A server asked to
    /// format a position past the end of the document it was last told about
    /// answers nothing, which is this feature not existing — so the pass that
    /// sends documents runs before the question rather than after it.
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

    /// What the request says: the character that asked for it, the cursor it
    /// was asked at, and the width the project named — `editor.tab_width`, four
    /// where no layer says otherwise, which is what this one holds.
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

    /// And the width is read rather than written down: a project on two spaces
    /// asks its server for two. Its own state rather than a second assertion on
    /// the test above, because a default that equals the constant it replaced
    /// is a test that cannot tell the two apart (R32.10).
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

    /// A newline is a character a server can name, and for clangd it is the
    /// only one — so a feature that works for `}` and not for Enter is a
    /// feature that does not exist in C at all. Enter reaches the buffer as
    /// `EditorKey('\n')`, which is what makes it one keystroke to this and not
    /// a shape of its own, and the position is where the cursor ended up: the
    /// new line, at the indentation the line before it was opened with.
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

    /// A character the server did not name asks nothing. The list is the
    /// server's whole answer about what it wants to be told: `;` named and `}`
    /// not is a server that formats statements and not blocks, and asking it
    /// anyway is a request per keystroke for a reply it has already said it has
    /// none of.
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

    /// Two servers over one file need not want the same characters, any more
    /// than they need the same capabilities: the character goes to the server
    /// that named it and to nobody else. Configuration is what makes one file
    /// two servers' business (ADR 0011), which is why this is data and no arm
    /// anywhere names a language.
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

    /// The edits, applied — and the coordinates converted on the way in: the
    /// server names the eight spaces as line 2, characters 0 to 8, and the
    /// brace comes back to the first column.
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

    /// The staleness rule, and it is the **document's** rather than the
    /// cursor's: edits describe the text the server was sent, so a reader who
    /// has typed on has a buffer those ranges are not about. R31.7 checks a
    /// version for diagnostics and this checks the same one — a third notion of
    /// stale is what this feature must not add.
    ///
    /// Typed and taken back again, which is the case that tells the two rules
    /// apart: the cursor is exactly where it was when the question went out and
    /// the text under it is not, so the hover box's rule would call this reply
    /// current and apply eight-column arithmetic to a line that has been
    /// rewritten twice since. Neither keystroke asks a question of its own — a
    /// character the server did not name asks nothing, and a backspace is not
    /// a character.
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

    /// Nobody pressed a key for this, so a server that declines says nothing:
    /// a notice every time a brace is typed in a file no server formats is the
    /// feature announcing itself constantly to say nothing, which is what
    /// [`About::asking`]'s silent refusal is for.
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

    /// A file whose server offers the capability at all is the whole of what
    /// this feature needs to be invisible in: typing in a language whose
    /// server declared nothing is untouched, which is story 19.
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
}
