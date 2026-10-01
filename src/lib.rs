#![deny(dead_code, unused)]
// State moves by value on every keystroke; boxing the Err would add an allocation per key
#![allow(clippy::result_large_err)]

pub mod authorship;
pub mod conflict;
pub mod debug;
pub mod editor;
pub mod filter;
pub mod fold;
pub mod format;
pub mod highlight;
pub mod history;
pub mod keys;
pub mod layout;
pub mod lsp;
pub mod minimap;
pub mod mouse;
pub mod preview;
pub mod queries;
pub mod reading;
pub mod review;
pub mod risk;
pub mod run;
pub mod search;
pub mod startup;
pub mod story;
pub mod tools;
pub mod tree;
pub mod tree_actions;

use editor::Buffer;
use review::{Comment, GitFile};
use search::Results;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeBounds;
use std::path::{Path, PathBuf};
use tree::Entry;
use tree_actions::{Action, Target};

pub const VARDE_DIR: &str = ".varde";

pub fn varde_dir(root: &Path, sidecar: Option<&Path>) -> PathBuf {
    match sidecar {
        Some(sidecar) => sidecar.to_path_buf(),
        None => root.join(VARDE_DIR),
    }
}

pub fn tmp_dir(varde_home: &Path) -> PathBuf {
    varde_home.join("tmp")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    Tree,
    #[default]
    Editor,
    Terminal,
    Ai,
    Risk,
    Buffers,
    History,
    Breakpoints,
    Frames,
    Variables,
    Output,
    Diagnostics,
    Conflicts,
    Evaluator,
    Cheatsheet,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub action: &'static str,
    pub name: &'static str,
    pub glyph: String,
    pub keys: &'static str,
    pub hue: Hue,
    pub tone: Tone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hue {
    Go,
    Hold,
    Step,
    Halt,
    Plain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Dimmed,
    Plain,
    Lit,
    Marked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Idle,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum View {
    Edit,
    Review,
    Story,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    Ctrl,
    Escape,
}

const WHEEL_ROWS: usize = 1;

const SLIDE_COLUMNS: usize = 8;

pub const PALETTE: [(&str, &[(char, &str)]); 5] = [
    (
        "Panes",
        &[
            ('o', "Editor"),
            ('g', "Buffers"),
            ('d', "Files"),
            ('t', "Terminal"),
            ('k', "Risk"),
            ('y', "Cursor history"),
            ('b', "Breakpoints"),
            ('i', "Diagnostics"),
            ('m', "Merge conflicts"),
            ('a', "AI"),
            ('l', "  Tall"),
        ],
    ),
    ("Views", &[('e', "Edit"), ('r', "Review"), ('s', "Story")]),
    (
        "Project",
        &[
            ('f', "Find"),
            ('v', "Tools"),
            ('n', "Launch"),
            ('c', "Collapse"),
        ],
    ),
    ("Help", &[('h', "Keys"), ('u', "Update")]),
    ("", &[('q', "Quit")]),
];

pub fn palette_rows(screen: u16) -> Vec<(Option<char>, String)> {
    let mut rows: Vec<(Option<char>, String)> = Vec::new();
    for (heading, entries) in PALETTE {
        if !rows.is_empty() {
            rows.push((None, String::new()));
        }
        if !heading.is_empty() {
            rows.push((None, format!("  {heading}")));
        }
        for (key, entry) in entries {
            rows.push((Some(*key), format!("   ({key}) {entry}")));
        }
    }
    rows.push((None, "   Esc  cancel".to_string()));

    let budget = screen.saturating_sub(2) as usize;
    if rows.len() > budget {
        rows.retain(|(key, row)| key.is_some() || !row.is_empty());
    }
    if rows.len() > budget {
        rows.pop();
    }
    if rows.len() > budget {
        rows.retain(|(key, _)| key.is_some());
    }
    if rows.len() > budget {
        rows.truncate(budget);
        if let Some(last) = rows.last_mut() {
            *last = (None, "   …".to_string());
        }
    }
    rows
}

pub fn palette_entry(key: char) -> Option<&'static str> {
    PALETTE
        .iter()
        .flat_map(|(_, entries)| entries.iter())
        .find(|(k, _)| *k == key)
        .map(|(_, entry)| entry.trim())
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Modal {
    #[default]
    None,
    NameBox {
        action: Action,
        dir: PathBuf,
    },
    Palette,
    Chord,
    Comment,
    SetValue,
    NewWatch,
    ExceptionClass,
    ConfirmSubmit,
    ConfirmStory {
        spelling: String,
        out: String,
    },
    StepDetail,
    Prediction {
        picked: Option<usize>,
    },
    Candidates(lsp::Candidates),
    Tools {
        row: usize,
    },
    Launches {
        row: usize,
    },
    RunMark {
        line: usize,
        mark: run::Mark,
    },
    Breakpoint {
        file: PathBuf,
        line: usize,
        field: debug::Field,
        draft: debug::Properties,
    },
    Branches {
        refs: Vec<story::BranchRef>,
        filter: String,
        row: usize,
    },
    Restart,
    Diverged,
    Stops {
        path: PathBuf,
        at: Vec<editor::Tail>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Reload,
    Overwrite,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceFailed {
    Download,
    NoAsset,
    Checksum,
    Replace,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Trigger(Action, Option<Target>),
    EnterName(String),
    Cancel,
    Tapped {
        key: Tap,
        at_ms: u64,
    },
    FallbackBinding,
    Key(char),
    ClickPaletteEntry(char),
    ToggleBreakpoint(usize),
    EditBreakpoint(usize),
    OfferRun(usize),
    ChooseRun(&'static str),
    BreakpointDraft(String),
    BreakpointField(debug::Field),
    SwitchSuspend,
    ConfirmBreakpoint,
    BreakpointFileRead {
        path: PathBuf,
        contents: String,
    },
    FilesAppeared(Vec<(PathBuf, tree::Kind)>),
    FilesRemoved(Vec<PathBuf>),
    Expand {
        path: PathBuf,
        entries: Vec<Entry>,
    },
    CollapseTree,
    FileChanged {
        path: PathBuf,
        contents: String,
    },
    Reload(PathBuf),
    Resized {
        width: u16,
        height: u16,
    },
    OpenReviewView,
    DragGutter {
        file: String,
        from_line: u32,
        to_line: u32,
    },
    FileComment {
        kind: String,
    },
    AddComment {
        file: String,
        from_line: u32,
        to_line: u32,
        kind: String,
        body: String,
    },
    SubmitReview,
    ConfirmSubmit,
    ToggleCheatsheet,
    ToggleField,
    ToggleMinimap,
    DragMinimap(u32),
    ToggleTallAi,
    SplitTerminal,
    FocusSplit(usize),
    ShellSpoke(usize),
    OutputSpoke,
    ToggleOutput,
    ClickPane(Pane),
    ClickThrough {
        pane: Pane,
        at: Place,
    },
    ClickLink {
        row: String,
        column: usize,
    },
    ClickRow(PathBuf),
    ClickRiskRow(usize),
    ClickBufferRow(usize),
    ClickHistoryRow(usize),
    ClickBreakpointRow(usize),
    ClickFrameRow(usize),
    ClickDiagnosticRow(usize),
    ClickConflictRow(usize),
    AcceptConflict(conflict::Side),
    ClickVariablesRow(usize),
    ShowGroup(layout::Group),
    Scroll {
        pane: Pane,
        direction: Direction,
        at: Place,
    },
    ScrollHover(Direction),
    ScrollCheatsheet {
        direction: Direction,
        rows: usize,
    },
    RightClick(Pane),
    DragDivider(u32),
    DragAiDivider(u32),
    DragOutput(u32),
    DragStrip(u32),
    Copy,
    PasteFromClipboard,
    MoveFocus(Direction),
    MoveSelection(Direction),
    MoveAction(Direction),
    StepFilter(Direction),
    Activate,
    Bytes(Vec<u8>),
    Pasted(String),
    RowAction(&'static str),
    HoverChip(&'static str),
    OpenHoverRow(usize),
    OpenEvaluator,
    RunSnippet,
    OpenEvaluatedRow(usize),
    PlaceEvaluator(layout::Area),
    SizeSnippet(u16),
    MoveEvaluator(Direction),
    ResizeEvaluator(Direction),
    ArrangeEvaluator(debug::Arrange),
    LeaveArranging,
    PaneAction(&'static str),
    EditorKey(char),
    EditorPaste(String),
    EditorIndent(Direction),
    EditorArrow(Direction),
    EditorExtend(Direction),
    EditorExtendWord(Direction),
    EditorWord(Direction),
    EditorNextOccurrence,
    StepBuffer(Direction),
    ShowBuffer(PathBuf),
    Filter(String),
    OpenSearch,
    SearchWordUnderCursor,
    SearchSelection,
    CloseSearch,
    OpenFind,
    QueryEnd(Direction),
    CloseFind,
    AcceptFind,
    StepMatch(Direction),
    FindKeys(FindKeys),
    ToggleCase,
    ReplaceMatch,
    ReplaceAll,
    SearchQuery(String),
    Searched {
        generation: u64,
        hits: Vec<search::Hit>,
        done: bool,
    },
    MoveHit(Direction),
    MoveHitFile(Direction),
    SelectHit(usize),
    OpenHit,
    OpenEveryHit,
    CompleteSearch,
    JumpTo(Place),
    Indexed {
        walk: u64,
        files: Vec<String>,
        done: bool,
    },
    AcceptFilter,
    BufferOpened {
        path: PathBuf,
        contents: String,
        preview: bool,
        at: Option<Place>,
    },
    EditorBackspace,
    EditorDeleteWord,
    EditorUndo,
    EditorRedo,
    EditorEscape,
    TogglePreview,
    WriteBuffer,
    ReloadBuffer,
    Resolve(Resolution),
    AiExited,
    AiSpoke,
    StartAi {
        command: Option<String>,
        force: bool,
    },
    CloseBuffer {
        force: bool,
    },
    CloseAllBuffers {
        force: bool,
    },
    Quit,
    QuitForce,
    Rebuild,
    BinaryReplaced(Result<(), ReplaceFailed>),
    ReleaseAnswered(Option<String>),
    SelectIn {
        pane: Pane,
        from: Place,
        to: Place,
        text: String,
    },
    ClickText(Place),
    DoubleClickText(Place),
    HoverLink(Option<Place>),
    HoverAction(Option<&'static str>),
    HoverMinimap(bool),
    AskDefinition,
    DragText {
        from: Place,
        to: Place,
    },
    DragRow(PathBuf),
    ShowDiff {
        file: String,
        lines: Vec<DiffLine>,
        revision: String,
    },
    StoryArtifact {
        contents: String,
        range: story::RangeStatus,
    },
    StoryFiles(Vec<story::FileHunks>),
    Story {
        explicit: Option<String>,
        force: bool,
    },
    PickBranch(Option<String>),
    Branches(story::Branching),
    MoveBranchRow(Direction),
    FilterBranches(String),
    ChooseBranch,
    CheckedOut {
        left: String,
    },
    CheckoutFailed(String),
    StoryResolved(story::Resolution),
    ConfirmStory,
    StoryFileWritten(String),
    EnterStory(usize),
    StepStory(Direction),
    EnterRemainder,
    StepRemainder(Direction),
    RiskFigures {
        generation: u64,
        figures: risk::Figures,
        before: Option<risk::Figures>,
    },
    StartRefactorLoop(risk::Scope),
    StopRefactorLoop,
    TestsFinished {
        passed: bool,
        output: String,
    },
    RecomputeRisk,
    ToggleRiskList,
    ToggleBuffersList,
    ToggleCursorHistory,
    ToggleBreakpointList,
    ToggleDiagnosticList,
    ToggleConflictList,
    ShowDiagnostics(lsp::Severity),
    JumpBack,
    JumpForward,
    ToggleRiskAll,
    LspReceived {
        language: String,
        json: String,
    },
    LspStarted {
        language: String,
    },
    ReviewFileRead {
        path: PathBuf,
        contents: String,
    },
    LspGone {
        language: String,
        why: lsp::Gone,
    },
    DapReceived {
        json: String,
        from: usize,
    },
    DapStarted {
        from: usize,
    },
    DapGone {
        why: debug::Gone,
        from: usize,
    },
    DapPortAnswers,
    StartLaunch(String),
    MoveLaunchRow(Direction),
    DebugResume,
    AskAboutPause,
    DebugStep(debug::Step),
    DebugStop,
    DebugRestart,
    LeaveStepping,
    CandidatesDue,
    PointerMoved(Pointed),
    HoverDue,
    MoveCandidate(Direction),
    AcceptCandidate,
    NextStop,
    MoveToolRow(Direction),
    InstallTool,
    GlobalConfigRead {
        kind: tools::Kind,
        name: String,
        write: tools::Write,
        text: Option<String>,
    },
    ConfigEdited {
        global: startup::OnDisk,
        project: startup::OnDisk,
    },
    InstallEnded(Option<String>),
    RecheckTool,
    PathProbed,
    ToggleFold {
        all: bool,
    },
    FormatBuffer,
    FormatterAnswered {
        language: String,
        path: PathBuf,
        revision: u64,
        answer: format::Answer,
    },
    StartReading,
    StopReading,
    ReadingEnded,
    PlayPause,
    NextUtterance,
    PreviousUtterance,
    SetSpeed(f32),
    Speaking {
        at_ms: u32,
        offsets: Vec<u32>,
    },
    Restart,
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    pub query: Buffer,
    pub scope: Option<PathBuf>,
    pub results: Results,
    pub selected: usize,
    pub scroll: usize,
    pub asked: u64,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            query: Buffer::text_box(""),
            scope: None,
            results: Results::default(),
            selected: 0,
            scroll: 0,
            asked: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Find {
    pub query: Buffer,
    pub origin: Place,
    pub case: search::Case,
    pub keys: FindKeys,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindKeys {
    Away,
    Query,
    Icon(FindIcon),
    Replace(ReplaceField),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindIcon {
    Case,
    Replace,
    ReplaceAll,
}

pub const FIND_ICONS: [(FindIcon, &str); 3] = [
    (FindIcon::Case, "[Aa]"),
    (FindIcon::Replace, "[replace]"),
    (FindIcon::ReplaceAll, "[replace all]"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceField {
    Find,
    With,
    Replace,
    ReplaceAll,
}

pub const REPLACE_BUTTONS: [(ReplaceField, &str); 2] = [
    (ReplaceField::Replace, "[replace]"),
    (ReplaceField::ReplaceAll, "[replace all]"),
];

pub const REPLACE_FIELDS: [ReplaceField; 4] = [
    ReplaceField::Find,
    ReplaceField::With,
    ReplaceField::Replace,
    ReplaceField::ReplaceAll,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub new_line: Option<usize>,
    pub old_line: Option<usize>,
    pub removed: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    SetTerminalInput(String),
    RunInTerminal(String),
    SplitTerminal {
        from: usize,
    },
    RenderView(View),
    EnsureDir(PathBuf),
    OpenBuffer(PathBuf),
    PreviewBuffer(PathBuf),
    WriteFile {
        path: PathBuf,
        contents: String,
    },
    DeleteFile(PathBuf),
    DeleteDir(PathBuf),
    SpawnAi {
        command: String,
    },
    Notify(&'static str),
    NotifyAbout {
        slug: &'static str,
        about: String,
    },
    ClearNotice,
    ReadFolder(PathBuf),
    Scrolled(Pane, Direction),
    OpenUrl(String),
    SetClipboard(String),
    SaveState(String),
    SendKeys {
        pane: Pane,
        bytes: Vec<u8>,
    },
    StartLsp {
        language: String,
        command: String,
        args: Vec<String>,
    },
    StartDap {
        command: String,
        args: Vec<String>,
        reach: debug::Reach,
    },
    DapSend {
        to: usize,
        json: String,
    },
    DapChild {
        child: usize,
    },
    StopDapChild {
        child: usize,
    },
    RunProgram {
        argv: Vec<String>,
        cwd: Option<PathBuf>,
        env: BTreeMap<String, String>,
    },
    StopDap,
    ProbePath,
    CheckRelease {
        url: String,
    },
    LspSend {
        language: String,
        json: String,
    },
    ReadForReview(PathBuf),
    ReadBreakpointFile(PathBuf),
    DebounceCandidates(u64),
    DwellHover(u64),
    Exit,
    Relaunch,
    ReplaceBinary {
        asset: String,
        checksums: String,
    },
    StopAi,
    ReadDiff(PathBuf),
    RunSearch(search::Request),
    OpenAt {
        path: PathBuf,
        at: Place,
    },
    IndexProject {
        walk: u64,
    },
    ClipboardViaTerminal(String),
    ReadClipboard,
    ReadStories {
        dir: PathBuf,
        repo: PathBuf,
    },
    ReadStoryFiles {
        repo: PathBuf,
        base: String,
        head: Option<String>,
        files: Vec<String>,
    },
    WriteStoryContext {
        repo: PathBuf,
        spelling: String,
        path: PathBuf,
    },
    ReadGlobalConfig {
        path: PathBuf,
        kind: tools::Kind,
        name: String,
        write: tools::Write,
    },
    ReadInstallStatus(PathBuf),
    ReadBranches,
    ReadGuestBranches {
        sentinel: PathBuf,
        repo: PathBuf,
        how: story::Download,
    },
    CheckoutBranch {
        repo: PathBuf,
        name: String,
    },
    ResolveStory {
        repo: PathBuf,
        dir: PathBuf,
        explicit: Option<String>,
        force: bool,
    },
    AnalyseRisk {
        scope: risk::Scope,
        generation: u64,
        files: Option<Vec<String>>,
        base: Option<String>,
    },
    Snapshot {
        iteration: u32,
    },
    RestoreSnapshot {
        iteration: u32,
    },
    RunTests {
        command: String,
    },
    RunFormatter {
        language: String,
        command: String,
        args: Vec<String>,
        path: PathBuf,
        revision: u64,
        text: String,
    },
    Speak {
        utterances: Vec<reading::Utterance>,
        speed: f32,
    },
    SpeakFrom {
        at_ms: u32,
    },
    PauseSpeaking,
    StopSpeaking,
}

impl Effect {
    fn notify_about(slug: &'static str, about: String) -> Self {
        Effect::NotifyAbout {
            slug,
            about: about
                .chars()
                .filter(|character| !character.is_control())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pointed {
    #[default]
    Elsewhere,
    Text(Place),
    Hover,
    Breakpoint(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Buffer {
        anchor: Place,
        cursor: Place,
    },
    Lines {
        from: usize,
        to: usize,
    },
    Screen {
        pane: Pane,
        from: Place,
        to: Place,
        text: String,
    },
}

impl Selection {
    pub fn buffer_span(&self) -> Option<(Place, Place)> {
        match self {
            Selection::Buffer { anchor, cursor } => Some(
                if (anchor.line, anchor.column) <= (cursor.line, cursor.column) {
                    (*anchor, *cursor)
                } else {
                    (*cursor, *anchor)
                },
            ),
            Selection::Lines { .. } | Selection::Screen { .. } => None,
        }
    }

    pub fn screen_span(&self, pane: Pane) -> Option<(Place, Place)> {
        match self {
            Selection::Screen {
                pane: picked,
                from,
                to,
                ..
            } if *picked == pane => Some((*from, *to)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub root: PathBuf,
    pub sidecar: Option<PathBuf>,
    pub varde_home: PathBuf,
    pub modal: Modal,
    pub view: View,
    pub double_tap_ms: u64,
    pub tab_width: usize,
    pub reports_modifiers: bool,
    pub contents: BTreeMap<PathBuf, Vec<Entry>>,
    pub expanded: BTreeSet<PathBuf>,
    pub ignored: BTreeSet<PathBuf>,
    pub filter: String,
    pub index: filter::Index,
    pub buffers: BTreeMap<PathBuf, Buffer>,
    pub current_buffer: Option<PathBuf>,
    pub view_buffers: BTreeMap<View, PathBuf>,
    pub restoring: usize,
    pub preview: Option<PathBuf>,
    pub repo: Option<Vec<GitFile>>,
    pub committed: BTreeMap<PathBuf, Option<String>>,
    /// Arc, not owned: update clones State per event, and copying this per keystroke is slow
    pub authorship: BTreeMap<PathBuf, std::sync::Arc<[authorship::Authored]>>,
    pub traced: Option<Traced>,
    pub comments: Vec<Comment>,
    pub reviews: BTreeSet<u32>,
    pub retention_limit: usize,
    pub ai_running: bool,
    pub ai_command: String,
    pub editor_theme: String,
    pub editor_field: bool,
    pub minimap: bool,
    pub ai_slot: layout::Slot,
    pub cheatsheet_scroll: usize,
    pub system_clipboard: bool,
    pub search: Option<Search>,
    pub searches_asked: u64,
    pub find: Option<Find>,
    pub replace_with: Buffer,
    pub diff: Option<Vec<DiffLine>>,
    pub diff_file: Option<String>,
    pub diff_revision: Option<String>,
    pub diff_line: usize,
    diff_anchor: Option<usize>,
    pub last_verdict: Option<String>,
    pub focus: Pane,
    pub tree_divider: u32,
    pub ai_width: Option<u32>,
    pub ai_pane: layout::AiPane,
    pub strip_height: Option<u32>,
    pub strip: layout::Group,
    pub output_running: bool,
    pub output_hidden: bool,
    pub output_width: Option<u32>,
    pub output_unseen: bool,
    pub output_mouse: mouse::Encoding,
    pub output_paste: keys::Paste,
    pub breakpoints: Vec<debug::Breakpoint>,
    pub evaluator: Option<debug::Evaluator>,
    pub evaluator_at: Option<layout::Area>,
    pub arranging: Option<debug::Arrange>,
    pub snippets: Vec<String>,
    pub exception_filters: BTreeMap<String, BTreeSet<String>>,
    pub watches: Vec<debug::Watch>,
    pub terminal_mouse: mouse::Encoding,
    pub terminals: Vec<Shell>,
    pub pending_command: Option<Effect>,
    pub terminal_split: usize,
    pub ai_mouse: mouse::Encoding,
    pub terminal_paste: keys::Paste,
    pub ai_paste: keys::Paste,
    pub ai_spoken: bool,
    pub pending_prompt: Option<(String, Enter)>,
    pub selection: Option<Selection>,
    pub occurrences: Vec<Place>,
    pub revealing: Option<Place>,
    pub link: Option<Place>,
    pub hovered_action: Option<&'static str>,
    pub transport_lit: Option<&'static str>,
    pub hovered_minimap: bool,
    pub tree_selection: Option<PathBuf>,
    pub tree_scroll: usize,
    pub editor_scroll: usize,
    pub refusal: Option<preview::Refusal>,
    pub editor_hscroll: usize,
    pub screen_width: u16,
    pub screen_height: u16,
    pub selected_action: Option<usize>,
    gutter: Option<(String, u32, u32)>,
    pub comment: Option<Buffer>,
    last_tap: Option<(Tap, u64)>,
    pub checkout: Option<PathBuf>,
    pub update: Option<String>,
    pub release: Option<startup::Release>,
    pub running_version: String,
    pub replaced: bool,
    pub story_set: story::Set,
    pub branch: Option<String>,
    pub left_branch: Option<String>,
    pub guest: Option<PathBuf>,
    pub guests: Vec<String>,
    pub story_listing: story::Listing,
    pub story_selection: usize,
    pub spine_scroll: usize,
    pub walking: Option<story::Walking>,
    /// Arc, not owned: update clones State per event, and this holds every changed file's text
    pub file_hunks: std::sync::Arc<[story::FileHunks]>,
    pub story_sets: Vec<String>,
    pub risk: risk::Risk,
    pub corner: layout::Corner,
    pub risk_all: bool,
    pub risk_selection: usize,
    pub risk_scroll: usize,
    pub buffers_selection: usize,
    pub buffers_scroll: usize,
    pub visits: Vec<history::Visit>,
    pub history_selection: usize,
    pub history_scroll: usize,
    pub breakpoints_selection: usize,
    pub breakpoints_scroll: usize,
    pub frames_selection: usize,
    pub frames_scroll: usize,
    pub diagnostics_selection: usize,
    pub diagnostics_scroll: usize,
    pub conflicts_selection: usize,
    pub conflicts_scroll: usize,
    pub conflicts_on_disk: BTreeMap<PathBuf, Vec<conflict::Conflict>>,
    pub variables_selection: usize,
    pub variables_scroll: usize,
    pub adapters: BTreeMap<String, startup::Adapter>,
    pub launches: BTreeMap<String, startup::Launch>,
    pub runs: BTreeMap<String, startup::Run>,
    pub last_launch: Option<startup::Launch>,
    pub debug: Option<debug::Session>,
    pub stepping: bool,
    pub tick: u64,
    pub risk_threshold: u32,
    pub refactor: risk::Refactor,
    pub max_iterations: u32,
    pub test_command: Option<String>,
    pub head: Option<String>,
    pub predictions_put: BTreeSet<(usize, usize)>,
    pub servers: BTreeMap<String, startup::Server>,
    pub formatters: BTreeMap<String, startup::Formatter>,
    pub facts: BTreeMap<String, startup::Fact>,
    pub lsp: BTreeMap<String, lsp::Conversation>,
    pub lsp_asked: BTreeMap<lsp::About, lsp::Question>,
    pub lsp_running: BTreeSet<String>,
    pub commands_on_path: BTreeSet<String>,
    pub git_installed: bool,
    pub speech: reading::Speech,
    pub voice_running: bool,
    pub player_installed: bool,
    pub voice_installed: bool,
    pub reading: Option<reading::Reading>,
    pub workspace_facts: BTreeMap<String, String>,
    pub recheck: Option<(tools::Kind, String)>,
    pub installing: Option<(tools::Kind, String)>,
    pub install_failed: BTreeSet<(tools::Kind, String)>,
    pub os: String,
    pub arch: String,
    pub diagnostics: BTreeMap<PathBuf, BTreeMap<String, Vec<lsp::Diagnostic>>>,
    pub diagnostic_totals: [usize; 4],
    pub hover: Option<lsp::Hover>,
    pub pointed_at: Pointed,
}

impl State {
    pub fn split(&self) -> usize {
        self.terminal_split
            .min(self.terminals.len().saturating_sub(1))
    }

    fn idle_split(&self) -> Option<usize> {
        let idle = |split: &usize| self.terminals.get(*split) == Some(&Shell::Idle);
        Some(self.split())
            .filter(idle)
            .or_else(|| (0..self.terminals.len()).find(idle))
    }

    pub fn repo_root(&self) -> &Path {
        self.guest.as_deref().unwrap_or(&self.root)
    }

    pub fn comment_target(&self) -> Option<(String, u32, u32)> {
        self.gutter.clone()
    }

    pub fn edited(&self) -> Option<&Buffer> {
        match self.focus {
            Pane::Evaluator => self.evaluator.as_ref().map(|it| &it.snippet),
            _ => current_buffer(self),
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        match self.selection.as_ref()? {
            Selection::Screen { text, .. } => Some(text.clone()),
            Selection::Lines { from, to } => Some(self.edited()?.lines_in(*from, *to)),
            selection => {
                let (from, to) = selection.buffer_span()?;
                Some(self.edited()?.text_in(from, to))
            }
        }
    }

    pub fn selected_diff_lines(&self) -> Option<(usize, usize)> {
        let anchor = self.diff_anchor?;
        Some((anchor.min(self.diff_line), anchor.max(self.diff_line)))
    }

    pub fn title(&self) -> String {
        self.root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

impl Default for State {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            sidecar: None,
            varde_home: PathBuf::new(),
            modal: Modal::None,
            view: View::Edit,
            double_tap_ms: 300,
            tab_width: editor::DEFAULT_TAB_WIDTH,
            reports_modifiers: false,
            evaluator: None,
            evaluator_at: None,
            arranging: None,
            snippets: Vec::new(),
            exception_filters: BTreeMap::new(),
            contents: BTreeMap::new(),
            expanded: BTreeSet::new(),
            ignored: BTreeSet::new(),
            filter: String::new(),
            index: filter::Index::default(),
            buffers: BTreeMap::new(),
            current_buffer: None,
            view_buffers: BTreeMap::new(),
            restoring: 0,
            preview: None,
            repo: None,
            comments: Vec::new(),
            reviews: BTreeSet::new(),
            retention_limit: 50,
            ai_running: false,
            ai_command: "claude".to_string(),
            editor_theme: "dark".to_string(),
            editor_field: true,
            minimap: true,
            ai_slot: layout::Slot::Ai,
            cheatsheet_scroll: 0,
            system_clipboard: true,
            search: None,
            searches_asked: 0,
            find: None,
            replace_with: Buffer::text_box(""),
            diff: None,
            diff_file: None,
            diff_revision: None,
            diff_line: 1,
            diff_anchor: None,
            last_verdict: None,
            focus: Pane::Editor,
            tree_divider: 30,
            ai_width: None,
            ai_pane: layout::AiPane::Beside,
            strip_height: None,
            strip: layout::Group::Shells,
            breakpoints: Vec::new(),
            watches: Vec::new(),
            corner: layout::Corner::Hidden,
            risk_all: false,
            risk_selection: 0,
            risk_scroll: 0,
            buffers_selection: 0,
            buffers_scroll: 0,
            visits: Vec::new(),
            history_selection: 0,
            history_scroll: 0,
            breakpoints_selection: 0,
            breakpoints_scroll: 0,
            frames_selection: 0,
            frames_scroll: 0,
            diagnostics_selection: 0,
            diagnostics_scroll: 0,
            conflicts_selection: 0,
            conflicts_scroll: 0,
            conflicts_on_disk: BTreeMap::new(),
            variables_selection: 0,
            variables_scroll: 0,
            adapters: BTreeMap::new(),
            launches: BTreeMap::new(),
            runs: BTreeMap::new(),
            last_launch: None,
            debug: None,
            stepping: false,
            output_running: false,
            output_hidden: false,
            output_width: None,
            output_unseen: false,
            output_mouse: mouse::Encoding::None,
            output_paste: keys::Paste::Bare,
            terminal_mouse: mouse::Encoding::None,
            terminals: vec![Shell::Idle],
            terminal_split: 0,
            pending_command: None,
            ai_mouse: mouse::Encoding::None,
            terminal_paste: keys::Paste::Bare,
            ai_paste: keys::Paste::Bare,
            ai_spoken: false,
            pending_prompt: None,
            selection: None,
            occurrences: Vec::new(),
            revealing: None,
            link: None,
            tree_selection: None,
            tree_scroll: 0,
            editor_scroll: 0,
            refusal: None,
            editor_hscroll: 0,
            screen_width: 0,
            screen_height: 0,
            selected_action: None,
            gutter: None,
            comment: None,
            last_tap: None,
            checkout: None,
            update: None,
            release: None,
            running_version: String::new(),
            replaced: false,
            story_set: story::Set::None,
            left_branch: None,
            guest: None,
            guests: Vec::new(),
            branch: None,
            story_listing: story::Listing::Spine,
            story_selection: 0,
            spine_scroll: 0,
            walking: None,
            file_hunks: std::sync::Arc::default(),
            committed: BTreeMap::new(),
            traced: None,
            authorship: BTreeMap::new(),
            story_sets: Vec::new(),
            predictions_put: BTreeSet::new(),
            risk: risk::Risk::default(),
            tick: 0,
            risk_threshold: risk::DEFAULT_THRESHOLD,
            refactor: risk::Refactor::default(),
            max_iterations: risk::DEFAULT_MAX_ITERATIONS,
            test_command: None,
            head: None,
            servers: BTreeMap::new(),
            formatters: BTreeMap::new(),
            facts: BTreeMap::new(),
            lsp: BTreeMap::new(),
            lsp_asked: BTreeMap::new(),
            lsp_running: BTreeSet::new(),
            commands_on_path: BTreeSet::new(),
            git_installed: false,
            speech: reading::Speech::default(),
            voice_running: false,
            player_installed: false,
            voice_installed: false,
            reading: None,
            workspace_facts: BTreeMap::new(),
            recheck: None,
            installing: None,
            install_failed: BTreeSet::new(),
            os: String::new(),
            arch: String::new(),
            diagnostics: BTreeMap::new(),
            diagnostic_totals: [0; 4],
            hover: None,
            hovered_action: None,
            transport_lit: None,
            hovered_minimap: false,
            pointed_at: Pointed::Elsewhere,
        }
    }
}

fn settle(mut next: State, mut effects: Vec<Effect>, wheeled: bool) -> (State, Vec<Effect>) {
    if showing_output(&next) {
        next.output_unseen = false;
    }
    if next.evaluator.is_none() {
        next.arranging = None;
    }
    if matches!(next.focus, Pane::Ai | Pane::Cheatsheet) {
        next.focus = next.ai_slot.pane();
    }
    if next.ai_slot == layout::Slot::Cheatsheet {
        next.cheatsheet_scroll = next.cheatsheet_scroll.min(
            keys::cheatsheet_rows(&next)
                .len()
                .saturating_sub(cheatsheet_fits(&next)),
        );
    }
    if let Some(at) = next.evaluator_at {
        next.evaluator_at = Some(layout::placed_window(
            at,
            next.screen_width,
            next.screen_height,
            debug::paused_row(&next),
        ));
    }
    if next.focus == Pane::Output && !next.output_running {
        next.focus = Pane::Editor;
    }
    if let Some(find) = next.find.as_mut() {
        if next.focus != Pane::Editor || next.search.is_some() || next.modal != Modal::None {
            find.keys = FindKeys::Away;
        }
    }
    let showing_lines = next.focus == Pane::Evaluator
        || (next.focus == Pane::Editor && next.diff.is_none() && !previewing(&next));
    let linewise = showing_lines
        .then(|| next.edited().and_then(Buffer::selected_lines))
        .flatten();
    match linewise {
        Some((from, to)) => next.selection = Some(Selection::Lines { from, to }),
        None if matches!(next.selection, Some(Selection::Lines { .. })) => next.selection = None,
        None => {}
    }
    if !wheeled {
        let (tree_fits, editor_fits, editor_columns) = fits(&next);
        let showing_spine = next.view == View::Story && next.story_listing == story::Listing::Spine;
        if showing_spine {
            next.spine_scroll = layout::viewport(
                next.spine_scroll,
                next.story_selection,
                story::spine_row_count(&next),
                tree_fits,
            );
        } else {
            let rows = tree::visible_rows(&next);
            let selected = next
                .tree_selection
                .as_ref()
                .and_then(|path| rows.iter().position(|row| &row.path == path))
                .unwrap_or(0);
            next.tree_scroll = layout::viewport(next.tree_scroll, selected, rows.len(), tree_fits);
        }
        let risk_rows_count = risk::list(&next).len();
        next.risk_scroll = layout::viewport(
            next.risk_scroll,
            next.risk_selection.min(risk_rows_count.saturating_sub(1)),
            risk_rows_count,
            corner_rows(&next),
        );
        clamp_buffers(&mut next);
        let history_rows = history::list(&next).len();
        next.history_scroll = layout::viewport(
            next.history_scroll,
            next.history_selection.min(history_rows.saturating_sub(1)),
            history_rows,
            corner_rows(&next),
        );
        let diagnostic_rows = lsp::listed(&next).len();
        next.diagnostics_selection = next
            .diagnostics_selection
            .min(diagnostic_rows.saturating_sub(1));
        next.diagnostics_scroll = layout::viewport(
            next.diagnostics_scroll,
            next.diagnostics_selection,
            diagnostic_rows,
            corner_rows(&next),
        );
        let conflict_rows = conflict::listed(&next).len();
        next.conflicts_selection = next
            .conflicts_selection
            .min(conflict_rows.saturating_sub(1));
        next.conflicts_scroll = layout::viewport(
            next.conflicts_scroll,
            next.conflicts_selection,
            conflict_rows,
            corner_rows(&next),
        );
        let frame_rows = debug::frame_rows(&next).len();
        next.frames_selection = next.frames_selection.min(frame_rows.saturating_sub(1));
        next.frames_scroll = layout::viewport(
            next.frames_scroll,
            next.frames_selection,
            frame_rows,
            corner_rows(&next),
        );
        let breakpoint_rows = debug::rows(&next);
        next.breakpoints_selection = next
            .breakpoints_selection
            .min(breakpoint_rows.saturating_sub(1));
        next.breakpoints_scroll = layout::viewport(
            next.breakpoints_scroll,
            next.breakpoints_selection,
            breakpoint_rows,
            corner_rows(&next),
        );
        let variable_rows = debug::variables(&next).len();
        next.variables_selection = next
            .variables_selection
            .min(variable_rows.saturating_sub(1));
        next.variables_scroll = layout::viewport(
            next.variables_scroll,
            next.variables_selection,
            variable_rows,
            strip_rows(&next),
        );
        let search_fits = search_rows(&next);
        if let Some(search) = next.search.as_mut() {
            let rows = search::rows(&search.results);
            let focus = rows
                .iter()
                .position(|row| *row == search::Row::Hit(search.selected))
                .unwrap_or(0);
            let above = layout::viewport(
                search.scroll,
                focus.saturating_sub(1),
                rows.len(),
                search_fits,
            );
            search.scroll = layout::viewport(above, focus, rows.len(), search_fits);
        }
        if next.hover.is_some() && !lsp::hover_stands(&next) {
            next.hover = None;
        }
        let elsewhere = match &next.modal {
            Modal::Candidates(list) => {
                next.focus != Pane::Editor || next.current_buffer.as_ref() != Some(&list.asked.path)
            }
            Modal::Stops { path, .. } => {
                next.focus != Pane::Editor || next.current_buffer.as_ref() != Some(path)
            }
            _ => false,
        };
        if elsewhere {
            next.modal = Modal::None;
        }
        if !matches!(next.modal, Modal::Comment) {
            next.comment = None;
        }
        if let Modal::Candidates(list) = &mut next.modal {
            list.scrolled();
        }
        let rows = preview_rows(&next);
        if previewing(&next) {
            let widths: Vec<usize> = rows.iter().map(|row| row.text().chars().count()).collect();
            if let Some(buffer) = current(&mut next) {
                buffer.row = buffer.row.clamp(1, widths.len().max(1));
                let width = widths.get(buffer.row - 1).copied().unwrap_or(0);
                buffer.row_column = buffer.row_column.clamp(1, width.max(1));
            }
        }
        let (line, lines) = editor_focus(&next, &rows);
        let following = layout::viewport(next.editor_scroll, line, lines, editor_fits);
        next.editor_scroll = match next.revealing.take() {
            Some(taken) => {
                let row = story::row_of(&next, taken.line as u32).saturating_sub(1);
                layout::viewport(following, row, lines, editor_fits)
            }
            None => following,
        };
        next.editor_hscroll = match sideways(&next, &rows) {
            Sideways::Cursor { column, width } => {
                layout::viewport(next.editor_hscroll, column, width, editor_columns)
            }
            Sideways::Read if next.editor_hscroll == 0 => 0,
            Sideways::Read => next
                .editor_hscroll
                .min(slid_width(&next, &rows).saturating_sub(editor_columns)),
        };
    }

    // Control characters are stripped: a newline in an injected command would run it, not type it
    let mut injected = false;
    let mut pushed = false;
    for effect in &mut effects {
        match effect {
            Effect::SetTerminalInput(text) => {
                injected = true;
                pushed = true;
                *text = text
                    .chars()
                    .filter(|character| !character.is_control())
                    .collect();
            }
            Effect::RunInTerminal(_) => pushed = true,
            _ => {}
        }
    }
    if injected {
        next.focus = Pane::Terminal;
    }
    if pushed {
        match next.idle_split() {
            Some(split) => next.terminal_split = split,
            None => {
                let from = next.split();
                next.terminal_split = from + 1;
                let (held, rest): (Vec<Effect>, Vec<Effect>) =
                    effects.into_iter().partition(|effect| {
                        matches!(
                            effect,
                            Effect::SetTerminalInput(_) | Effect::RunInTerminal(_)
                        )
                    });
                effects = rest;
                next.pending_command = held.into_iter().last();
                effects.push(Effect::SplitTerminal { from });
            }
        }
    }
    (next, effects)
}

pub type Traced = (PathBuf, u64, std::sync::Arc<[Option<usize>]>);

type Declined = (State, Event);

type Answered = Result<(State, Vec<Effect>), Declined>;

fn clamp_buffers(next: &mut State) {
    let rows = next.buffers.len();
    next.buffers_selection = next.buffers_selection.min(rows.saturating_sub(1));
    next.buffers_scroll = layout::viewport(
        next.buffers_scroll,
        next.buffers_selection,
        rows,
        corner_rows(next),
    );
}

pub fn update(state: &State, event: Event) -> (State, Vec<Effect>) {
    let jump = history::jumped(state, &event);
    let (mut next, mut effects) = route(state, event);
    if next.focus != state.focus {
        next.selected_action = None;
    }
    if next.current_buffer != state.current_buffer || !next.buffers.keys().eq(state.buffers.keys())
    {
        next.buffers_selection = buffer_list(&next)
            .iter()
            .position(|path| Some(path.as_path()) == next.current_buffer.as_deref())
            .unwrap_or(0);
        clamp_buffers(&mut next);
    }
    history::record(state, &mut next, jump);
    if refuse_guest_edits(state, &mut next) {
        next.refusal = Some(preview::Refusal::GuestReadOnly);
    }
    let mut moved = false;
    for (path, buffer) in &next.buffers {
        match state.buffers.get(path) {
            Some(before)
                if before.revision() != buffer.revision()
                    && next.breakpoints.iter().any(|b| &b.file == path) =>
            {
                let was = next.breakpoints.clone();
                debug::follow(&mut next.breakpoints, path, before.shown(), buffer.shown());
                moved |= was != next.breakpoints;
            }
            _ => {}
        }
    }
    if moved {
        effects.push(Effect::SaveState(state_json(&next)));
    }
    debug::forget_changed(&state.breakpoints, &mut next);
    effects.extend(lsp::sync(&mut next));
    (next, effects)
}

fn route(state: &State, event: Event) -> (State, Vec<Effect>) {
    let mut next = state.clone();
    next.refusal = None;
    let wheeled = matches!(event, Event::Scroll { .. } | Event::ScrollHover(_));
    let declined = (next, event);
    let declined = match section_input(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return answer,
        Err(declined) => declined,
    };
    let declined = match section_editing(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return answer,
        Err(declined) => declined,
    };
    let declined = match section_workspace(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return answer,
        Err(declined) => declined,
    };
    unreachable!("no group answers {:?}", declined.1)
}

fn opens_a_chord(state: &State) -> bool {
    state.modal == Modal::None
        && state.view == View::Edit
        && state.diff.is_none()
        && state.walking.is_none()
        && !previewing(state)
        && state.edited().is_some_and(|buffer| {
            buffer.mode == editor::Mode::Normal && buffer.pending().is_empty()
        })
}

fn on_snippet(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    if next.focus != Pane::Evaluator || next.evaluator.is_none() {
        return Err((next, event));
    }
    if matches!(event, Event::EditorKey('\n')) && !editor_inserting(&next) {
        let effects = debug::run(&mut next);
        return Ok(settle(next, effects, wheeled));
    }
    if matches!(event, Event::EditorKey(' ')) && opens_a_chord(&next) {
        return Err((next, event));
    }
    if let Event::EditorArrow(direction @ (Direction::Up | Direction::Down)) = event {
        if debug::recall(&mut next, direction) {
            return Ok(settle(next, vec![], wheeled));
        }
    }
    let snippet = &next.evaluator.as_ref().expect("checked just above").snippet;
    let settled = snippet.mode == editor::Mode::Normal
        && snippet.pending_command().is_empty()
        && next.selection.is_none()
        && next.occurrences.is_empty();
    if matches!(event, Event::CloseBuffer { .. })
        || (matches!(event, Event::EditorEscape) && settled)
    {
        let effects = debug::close_evaluator(&mut next);
        return Ok(settle(next, effects, wheeled));
    }
    if matches!(event, Event::EditorEscape) {
        next.selection = None;
        next.occurrences.clear();
    }
    let shared = match event {
        Event::EditorArrow(_) | Event::EditorWord(_) | Event::EditorExtend(_) => {
            on_editor_arrow_2(state, next, event, wheeled)
        }
        Event::EditorExtendWord(_) | Event::EditorNextOccurrence => {
            on_editor_extend_word(state, next, event, wheeled)
        }
        Event::EditorIndent(_) | Event::EditorPaste(_) => {
            on_editor_key_6(state, next, event, wheeled)
        }
        Event::PasteFromClipboard => Ok(settle(next, vec![Effect::ReadClipboard], wheeled)),
        Event::EditorKey('d') if pending_g(state) => Err((next, event)),
        Event::EditorKey(_) => on_editor_key_3(state, next, event, wheeled),
        other => Err((next, other)),
    };
    let (mut next, event) = match shared {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let snippet = &mut next.evaluator.as_mut().expect("checked just above").snippet;
    match event {
        Event::EditorKey(key) => _ = snippet.key(key),
        Event::EditorBackspace => erase(state, &mut next),
        Event::EditorDeleteWord => snippet.delete_word_back(),
        Event::EditorUndo => snippet.undo(),
        Event::EditorRedo => snippet.redo(),
        Event::EditorEscape => snippet.escape(),
        other => return Err((next, other)),
    }
    Ok(settle(next, vec![], wheeled))
}

fn erase(state: &State, next: &mut State) {
    if !state.occurrences.is_empty() {
        let picked = state.selection.as_ref().and_then(Selection::buffer_span);
        next.selection = None;
        if let Some(buffer) = edited_mut(next) {
            buffer.mode = editor::Mode::Insert;
            let primary = picked.map_or(
                Place {
                    line: buffer.line,
                    column: buffer.column,
                },
                |(from, _)| from,
            );
            let places: Vec<Place> = std::iter::once(primary)
                .chain(state.occurrences.iter().copied())
                .collect();
            let landed = match picked {
                Some((from, to)) => buffer.replace_at(&places, to.column + 1 - from.column, ""),
                None => {
                    let behind: Vec<Place> = places
                        .iter()
                        .filter(|place| place.column > 1)
                        .map(|place| Place {
                            column: place.column - 1,
                            ..*place
                        })
                        .collect();
                    let mut erased = buffer.replace_at(&behind, 1, "").into_iter();
                    places
                        .iter()
                        .map(|place| match place.column > 1 {
                            true => erased.next().expect("one per place behind"),
                            false => *place,
                        })
                        .collect()
                }
            };
            buffer.go_to_place(landed[0]);
            next.occurrences = landed[1..].to_vec();
        }
        return;
    }
    let picked = match editor_inserting(state) {
        true => state.selection.as_ref().and_then(Selection::buffer_span),
        false => None,
    };
    if picked.is_some() {
        next.selection = None;
    }
    if let Some(buffer) = edited_mut(next) {
        match picked {
            Some((from, to)) => buffer.delete_in(from, to),
            None => buffer.backspace(),
        }
    }
}

fn on_comment_body(mut next: State, event: Event, wheeled: bool) -> Answered {
    if next.comment.is_none() {
        return Err((next, event));
    }
    let body = next.comment.as_mut().expect("checked just above");
    match event {
        Event::EditorKey(key) => _ = body.key(key),
        Event::EditorBackspace => body.backspace(),
        Event::EditorDeleteWord => body.delete_word_back(),
        Event::EditorUndo => body.undo(),
        Event::EditorRedo => body.redo(),
        Event::EditorArrow(direction) => body.arrow(direction),
        Event::EditorWord(direction) => body.word_motion(editor::Word::toward(direction)),
        Event::EditorPaste(text) => body.paste(&text),
        other => return Err((next, other)),
    }
    Ok(settle(next, vec![], wheeled))
}

fn on_query(mut next: State, event: Event, wheeled: bool) -> Answered {
    let keys = next.find.as_ref().map(|find| find.keys);
    let query = if let Some(search) = next.search.as_mut() {
        &mut search.query
    } else if let (Some(find), Some(FindKeys::Query | FindKeys::Replace(ReplaceField::Find))) =
        (next.find.as_mut(), keys)
    {
        &mut find.query
    } else if keys == Some(FindKeys::Replace(ReplaceField::With)) {
        &mut next.replace_with
    } else {
        return Err((next, event));
    };
    let before = query.revision();
    match event {
        Event::EditorKey(key) => query.paste(&key.to_string()),
        Event::EditorBackspace if query.column > 1 => {
            let before = Place {
                line: 1,
                column: query.column - 1,
            };
            query.delete_in(before, before);
        }
        Event::EditorBackspace => {}
        Event::EditorDeleteWord => query.delete_word_back(),
        Event::EditorArrow(direction) => query.arrow(direction),
        Event::EditorWord(direction) => query.word_motion(editor::Word::toward(direction)),
        Event::QueryEnd(direction) => query.go_to_place(Place {
            line: 1,
            column: if direction == Direction::Left {
                1
            } else {
                usize::MAX
            },
        }),
        other => return Err((next, other)),
    }
    if query.revision() == before {
        return Ok(settle(next, vec![], wheeled));
    }
    let find = next.find.as_ref().map(|find| (find.keys, find.origin));
    let effects = match (next.search.as_mut(), find) {
        (Some(search), _) => {
            search.selected = 0;
            search.scroll = 0;
            search::ask(&mut next)
        }
        (None, Some((FindKeys::Replace(ReplaceField::With), _))) | (None, None) => vec![],
        (None, Some((_, origin))) => {
            if let Some(at) = closest_match(&next, origin) {
                go_to_match(&mut next, at);
            }
            vec![]
        }
    };
    Ok(settle(next, effects, wheeled))
}

fn section_input(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match route_trigger(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match route_key(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match route_submit_review(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn section_editing(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match route_step_buffer(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match route_accept_filter(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match route_editor_key(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn section_workspace(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match route_ai_spoke(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match route_story_file_written(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match route_activate(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_trigger(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_snippet(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_comment_body(declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_query(declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_trigger(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_enter_name(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_tapped(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_key(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_key_2(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_key_3(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_key(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_key_4(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_key_5(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_key_6(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_files_removed(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_file_changed(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_reload(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_submit_review(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_submit_review(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_click_row(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_scroll(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_resized(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_arrow(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_buffer_opened(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_step_buffer(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_step_buffer(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_search_word_under_cursor(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_find_query(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_step_match(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_search_query(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_complete_search(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_accept_filter(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_accept_filter(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_arrow_2(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_extend_word(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_toggle_preview(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_key(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_key_2(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_editor_key(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_editor_key_3(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_key_4(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_key_5(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_key_6(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_editor_escape(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_breakpoint(declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_resolve(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_ai_spoke(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_ai_spoke(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_close_buffer(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_quit_force(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_rebuild(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_drag_row(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_story_resolved(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_lsp(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_debug(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_reading(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_story_file_written(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_story_file_written(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_pane_action(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_move_selection(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_move_selection_2(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_move_selection_3(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_activate(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn route_activate(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_activate_2(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_activate_3(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_activate_4(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_bytes(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_pasted(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    let declined = match on_row_action(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer),
        Err(declined) => declined,
    };
    Err(declined)
}

fn on_trigger(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Trigger(Action::BackToRoot, _) => {
            vec![Effect::RunInTerminal(tree_actions::command(
                "cd",
                &state.root,
            ))]
        }
        Event::Trigger(Action::GoHere, Some(Target::Folder(path))) => {
            vec![Effect::SetTerminalInput(tree_actions::command(
                "cd",
                &state.root.join(path),
            ))]
        }
        Event::Trigger(Action::Delete, Some(target)) => {
            let (verb, path) = match target {
                Target::File(p) => ("rm", p),
                Target::Folder(p) => ("rm -r", p),
            };
            next.tree_selection = Some(state.root.join(path.parent().unwrap_or(Path::new(""))));
            next.focus = Pane::Tree;
            vec![Effect::RunInTerminal(tree_actions::command(
                verb,
                &state.root.join(path),
            ))]
        }
        Event::Trigger(Action::SearchHere, Some(Target::Folder(path))) => {
            next.search = Some(Search {
                scope: Some(path),
                ..Search::default()
            });
            vec![]
        }
        Event::Trigger(Action::CopyPath, Some(target)) => {
            let path = match target {
                Target::File(p) | Target::Folder(p) => p,
            };
            to_clipboard(state, state.root.join(path).to_string_lossy().into_owned())
        }
        Event::Trigger(action @ (Action::NewFile | Action::NewDirectory), Some(target)) => {
            let dir = match target {
                Target::Folder(p) => p,
                Target::File(p) => p.parent().map(Path::to_path_buf).unwrap_or_default(),
            };
            next.modal = Modal::NameBox { action, dir };
            vec![]
        }
        Event::Trigger(..) => vec![],

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_enter_name(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EnterName(name) => match std::mem::take(&mut next.modal) {
            Modal::NameBox { action, dir } => {
                let base = state.root.join(dir);
                let path = base.join(name);
                let folder = path.parent().unwrap_or(&base).to_path_buf();
                next.tree_selection = Some(path.clone());
                next.focus = Pane::Tree;
                vec![
                    Effect::RunInTerminal(tree_actions::create(action, &base, &path)),
                    Effect::ReadFolder(folder),
                ]
            }
            Modal::SetValue => debug::set_value(&mut next, name),
            Modal::NewWatch => debug::add_watch(&mut next, name),
            Modal::ExceptionClass => debug::name_class(&mut next, name),
            _ => vec![],
        },

        Event::Cancel if state.hover.as_ref().is_some_and(|hover| hover.focused) => {
            next.hover = None;
            vec![]
        }
        Event::Cancel => {
            next.modal = Modal::None;
            next.selected_action = None;
            if !matches!(state.modal, Modal::StepDetail | Modal::Prediction { .. }) {
                next.walking = None;
            }
            if next.view == View::Story
                && matches!(
                    next.story_set,
                    story::Set::Authoring { .. } | story::Set::Fixing { .. }
                )
            {
                next.story_set = story::Set::None;
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_tapped(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Tapped { key, at_ms } => {
            let available = match key {
                Tap::Ctrl => state.reports_modifiers,
                Tap::Escape => !state.reports_modifiers,
            };
            let within_window = state.last_tap.is_some_and(|(prev, ms)| {
                prev == key && at_ms.saturating_sub(ms) <= state.double_tap_ms
            });
            if available && within_window {
                next.last_tap = None;
                next.modal = Modal::Palette;
            } else {
                next.last_tap = Some((key, at_ms));
            }
            vec![]
        }

        Event::FallbackBinding => {
            next.modal = Modal::Palette;
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_key(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Key('t') if state.view == View::Story && state.modal == Modal::None => {
            next.story_listing = match state.story_listing {
                story::Listing::Spine => story::Listing::Files,
                story::Listing::Files => story::Listing::Spine,
            };
            vec![]
        }

        Event::Key(key @ ('j' | 'k' | 'e' | 'g' | 'c' | 'd'))
            if state.view == View::Story
                && state.walking.is_some()
                && state.modal == Modal::None =>
        {
            return Ok(walk_key(state, next, key));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_key_2(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        Event::Key(key @ ('n' | 'p'))
            if state.view == View::Story
                && state.walking.is_some()
                && matches!(state.modal, Modal::None | Modal::Prediction { .. }) =>
        {
            Ok(walk_key(state, next, key))
        }

        Event::Key('D')
            if state.view == View::Story
                && state.walking.is_some()
                && matches!(state.modal, Modal::None | Modal::StepDetail) =>
        {
            Ok(walk_key(state, next, 'D'))
        }

        Event::Key(key @ ('1' | '2' | '3')) if matches!(state.modal, Modal::Prediction { .. }) => {
            Ok(pick_prediction(state, next, key))
        }

        other => Err((next, other)),
    }
}

fn on_key_3(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        Event::Key(key @ ('j' | 'k' | 'a' | 'r' | 'l'))
            if state.focus == Pane::Risk && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                'k' => update(state, Event::MoveSelection(Direction::Up)),
                'a' => update(state, Event::ToggleRiskAll),
                'r' => update(state, Event::PaneAction(risk::RECOMPUTE)),
                _ => update(state, Event::PaneAction(risk::loop_action(state))),
            })
        }

        Event::Key(key @ ('j' | 'k'))
            if state.focus == Pane::History && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                _ => update(state, Event::MoveSelection(Direction::Up)),
            })
        }

        Event::Key(key @ ('j' | 'k'))
            if state.focus == Pane::Buffers && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                _ => update(state, Event::MoveSelection(Direction::Up)),
            })
        }

        Event::Key(' ')
            if state.modal == Modal::None
                && state.debug.is_some()
                && matches!(state.focus, Pane::Variables | Pane::Frames) =>
        {
            Ok((
                State {
                    modal: Modal::Chord,
                    ..next
                },
                vec![],
            ))
        }

        Event::Key(key @ ('j' | 'k' | 'e' | 'w' | 'i' | 'h'))
            if state.focus == Pane::Diagnostics && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                'k' => update(state, Event::MoveSelection(Direction::Up)),
                letter => match lsp::Severity::ALL
                    .into_iter()
                    .find(|severity| lsp::letter(*severity) == letter)
                {
                    Some(severity) => update(state, Event::ShowDiagnostics(severity)),
                    None => (next, vec![]),
                },
            })
        }

        Event::Key(key @ ('j' | 'k'))
            if matches!(state.focus, Pane::Frames | Pane::Conflicts)
                && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                _ => update(state, Event::MoveSelection(Direction::Up)),
            })
        }

        Event::Key(key @ ('j' | 'k' | 's' | 'y' | 'Y' | 'w' | 'd' | 'a'))
            if state.focus == Pane::Variables && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                'k' => update(state, Event::MoveSelection(Direction::Up)),
                's' => update(state, Event::RowAction(debug::SET_VALUE)),
                'y' => update(state, Event::RowAction(debug::COPY_VALUE)),
                'Y' => update(state, Event::RowAction(debug::COPY_EXPRESSION)),
                'w' => update(state, Event::RowAction(debug::WATCH)),
                'd' => update(state, Event::RowAction(debug::REMOVE_WATCH)),
                _ => {
                    let mut opened = next;
                    opened.modal = Modal::NewWatch;
                    (opened, vec![])
                }
            })
        }

        Event::Key(key @ ('j' | 'k' | 'e' | 'd' | 'D' | 'x'))
            if state.focus == Pane::Breakpoints && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                'k' => update(state, Event::MoveSelection(Direction::Up)),
                'e' => update(state, Event::RowAction(debug::EDIT)),
                'd' => update(state, Event::RowAction(debug::REMOVE)),
                'x' => update(state, Event::PaneAction(debug::EXCEPTION_CLASS)),
                _ => update(state, Event::PaneAction(debug::CLEAR_ALL)),
            })
        }

        other => Err((next, other)),
    }
}

fn on_key_4(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        Event::Key(key) if state.focus == Pane::Tree && state.modal == Modal::None => {
            let action = match key {
                '-' => return Ok(update(state, Event::Trigger(Action::BackToRoot, None))),
                'c' => return Ok(update(state, Event::CollapseTree)),
                'n' => "new-file",
                'N' => "new-directory",
                'd' => "delete",
                _ => return Ok((next, vec![])),
            };
            Ok(update(state, Event::RowAction(action)))
        }

        other => Err((next, other)),
    }
}

fn on_key_5(state: &State, mut next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        Event::Key(key) if state.modal == Modal::Chord => {
            next.modal = Modal::None;
            let Some(chord) = keys::chord(state, key) else {
                return Ok((next, vec![]));
            };
            next.stepping = state.debug.is_some() && !matches!(key, 'q' | 'm' | 'z');
            Ok(update(&next, chord))
        }
        Event::Key(key) if state.modal == Modal::Palette => {
            let Some(entry) = palette_entry(key) else {
                return Ok((next, vec![]));
            };
            if entry == "Tools" {
                next.modal = Modal::Tools { row: 0 };
                return Ok((next, vec![]));
            }
            if entry == "Launch" {
                next.modal = Modal::Launches { row: 0 };
                return Ok((next, vec![]));
            }
            next.modal = Modal::None;
            let next = match palette_command(next, entry) {
                Ok(answer) => return Ok(answer),
                Err(next) => next,
            };
            let mut next = match palette_pane(next, entry) {
                Ok(answer) => return Ok(answer),
                Err(next) => next,
            };
            let (view, pane) = match entry {
                "Review" => (View::Review, Pane::Tree),
                "Story" => (View::Story, Pane::Editor),
                _ => (View::Edit, Pane::Editor),
            };
            next.focus = pane;
            Ok(switch_view(&next, view))
        }
        other => Err((next, other)),
    }
}

fn palette_command(next: State, entry: &str) -> Result<(State, Vec<Effect>), State> {
    let event = match entry {
        "Find" => Event::OpenSearch,
        "Tall" => Event::ToggleTallAi,
        "Risk" => Event::ToggleRiskList,
        "Buffers" => Event::ToggleBuffersList,
        "Cursor history" => Event::ToggleCursorHistory,
        "Breakpoints" => Event::ToggleBreakpointList,
        "Diagnostics" => Event::ToggleDiagnosticList,
        "Merge conflicts" => Event::ToggleConflictList,
        "Collapse" => Event::CollapseTree,
        "Keys" => Event::ToggleCheatsheet,
        "Update" => Event::Rebuild,
        "Quit" => Event::Quit,
        _ => return Err(next),
    };
    Ok(update(&next, event))
}

fn palette_pane(mut next: State, entry: &str) -> Result<(State, Vec<Effect>), State> {
    next.focus = match entry {
        "AI" => Pane::Ai,
        "Terminal" => Pane::Terminal,
        "Files" => Pane::Tree,
        "Editor" => Pane::Editor,
        _ => return Err(next),
    };
    Ok((next, vec![]))
}

fn on_key_6(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Key(_) => vec![],

        Event::FilesAppeared(appeared) => {
            // Match the whole path: a `refactor-done` anywhere else in the watched tree must not count
            let sentinel = varde_dir(&next.root, next.sidecar.as_deref()).join(risk::SENTINEL);
            let reported = appeared.iter().any(|(path, _)| path == &sentinel);
            let read_the_download = match &next.story_set {
                story::Set::Downloading { url, how } => {
                    let sidecar = varde_dir(&next.root, next.sidecar.as_deref());
                    let done = sidecar.join(story::DOWNLOAD_SENTINEL);
                    appeared.iter().any(|(path, _)| path == &done).then(|| {
                        Effect::ReadGuestBranches {
                            repo: sidecar.join(story::guest_name(url)),
                            sentinel: done,
                            how: *how,
                        }
                    })
                }
                _ => None,
            };
            let install = varde_dir(&next.root, next.sidecar.as_deref()).join(tools::SENTINEL);
            let read_the_install = (next.installing.is_some()
                && appeared.iter().any(|(path, _)| path == &install))
            .then_some(Effect::ReadInstallStatus(install));
            for (path, kind) in appeared {
                let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                    continue;
                };
                if let Some(entries) = next.contents.get_mut(parent) {
                    let name = name.to_string_lossy().into_owned();
                    if !entries.iter().any(|entry| entry.name == name) {
                        entries.push(Entry {
                            name,
                            is_dir: kind == tree::Kind::Folder,
                        });
                    }
                }
            }
            let mut effects = match reported {
                true => risk::pass_reported(&mut next),
                false => vec![],
            };
            effects.extend(read_the_download);
            effects.extend(read_the_install);
            effects
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_files_removed(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::FilesRemoved(paths) => {
            for path in paths {
                let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                    continue;
                };
                if let Some(entries) = next.contents.get_mut(parent) {
                    entries.retain(|entry| Some(entry.name.as_str()) != name.to_str());
                }
            }
            vec![]
        }

        Event::Expand { path, entries } => {
            next.contents.insert(path.clone(), entries);
            next.expanded.insert(path);
            vec![]
        }

        Event::CollapseTree => {
            next.expanded.clear();
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_file_changed(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::FileChanged { path, contents } => {
            let relative = path
                .strip_prefix(&state.root)
                .ok()
                .map(|path| path.to_string_lossy().into_owned());
            if let (Some(files), Some(relative)) = (next.repo.as_mut(), relative.as_deref()) {
                if !files.iter().any(|file| file.path == relative) {
                    files.push(review::GitFile {
                        path: relative.to_string(),
                        status: review::GitStatus::Modified,
                    });
                }
            }
            let mut effects = Vec::new();
            if let Some(buffer) = next.buffers.get_mut(&path) {
                buffer.follow(contents);
                if buffer.changed_on_disk {
                    effects.push(Effect::Notify("buffer-diverged"));
                }
            }
            match relative {
                Some(relative) if next.diff_file.as_deref() == Some(relative.as_str()) => {
                    effects.push(Effect::ReadDiff(path));
                }
                _ => {}
            }
            effects
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_reload(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Reload(path) => {
            if let Some(buffer) = next.buffers.get_mut(&path) {
                buffer.reload();
            }
            vec![Effect::ClearNotice]
        }

        Event::OpenReviewView => {
            move_to_view(&mut next, View::Review);
            next.focus = Pane::Tree;
            let mut effects = vec![
                Effect::RenderView(View::Review),
                risk::analyse(&mut next, risk::Scope::Review),
            ];
            match tree::visible_rows(&next).first() {
                Some(row) => {
                    next.tree_selection = Some(row.path.clone());
                    effects.push(Effect::ReadDiff(row.path.clone()));
                }
                None => {
                    next.diff = None;
                    next.diff_file = None;
                }
            }
            effects
        }

        Event::DragGutter {
            file,
            from_line,
            to_line,
        } => {
            open_comment_box(&mut next, file, from_line, to_line);
            vec![]
        }

        Event::FileComment { kind } => {
            next.modal = Modal::None;
            let body = next
                .comment
                .take()
                .map_or(String::new(), |body| body.shown().to_string());
            match next.gutter.take() {
                Some((file, from_line, to_line)) => {
                    next.comments
                        .push(comment(state, file, from_line, to_line, kind, body));
                    vec![Effect::Notify("comment-added")]
                }
                None => vec![],
            }
        }

        Event::AddComment {
            file,
            from_line,
            to_line,
            kind,
            body,
        } => {
            next.comments
                .push(comment(state, file, from_line, to_line, kind, body));
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_submit_review(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::SubmitReview if state.comments.is_empty() => vec![Effect::Notify("review-empty")],

        Event::SubmitReview => {
            next.modal = Modal::ConfirmSubmit;
            vec![]
        }

        Event::ConfirmSubmit => {
            next.modal = Modal::None;
            submit(&mut next)
        }

        Event::ClickPane(pane) => {
            next.focus = pane;
            next.selection = None;
            vec![]
        }

        Event::ClickThrough { pane, at } => match pane {
            Pane::Terminal | Pane::Ai | Pane::Output => {
                match mouse::report(mouse_encoding(state, pane), mouse::Gesture::Click, at) {
                    Some(bytes) => vec![Effect::SendKeys { pane, bytes }],
                    None => vec![],
                }
            }
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
            | Pane::Cheatsheet => vec![],
        },
        Event::ClickLink { row, column } => editor::link_at(&row, column)
            .map(Effect::OpenUrl)
            .into_iter()
            .collect(),

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_click_row(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::ClickRow(path) => {
            next.focus = Pane::Tree;
            next.tree_selection = Some(path.clone());
            if state.view == View::Review {
                next.focus = Pane::Editor;
                return Ok((next, vec![Effect::ReadDiff(path)]));
            }
            let is_folder = next.contents.contains_key(&path) || is_known_folder(state, &path);
            if !is_folder {
                let (mut opened, effects) = open_file(next, path);
                opened.focus = Pane::Tree;
                return Ok((opened, effects));
            } else if next.expanded.remove(&path) {
                vec![]
            } else {
                vec![Effect::ReadFolder(path)]
            }
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn wheeled_to(direction: Direction, offset: usize, rows: usize, fits: usize) -> usize {
    match direction {
        Direction::Up => offset.saturating_sub(WHEEL_ROWS),
        _ => (offset + WHEEL_ROWS).min(rows.saturating_sub(fits.max(1))),
    }
}

fn scroll_tree(state: &State, next: &mut State, direction: Direction, fits: usize) {
    let showing_spine = state.view == View::Story && state.story_listing == story::Listing::Spine;
    if showing_spine {
        next.spine_scroll = wheeled_to(
            direction,
            state.spine_scroll,
            story::spine_row_count(state),
            fits,
        );
    } else {
        next.tree_scroll = wheeled_to(
            direction,
            state.tree_scroll,
            tree::visible_rows(state).len(),
            fits,
        );
    }
}

fn travel_editor(
    state: &State,
    next: &mut State,
    fits: usize,
    offset: impl FnOnce(usize) -> usize,
) {
    let (row, rows) = editor_focus(state, &preview_rows(state));
    next.editor_scroll = offset(rows).min(rows.saturating_sub(fits.max(1)));
    let last = next.editor_scroll + fits.max(1) - 1;
    let landed = row.clamp(next.editor_scroll, last) + 1;
    if landed == row + 1 {
        return;
    }
    match next.current_buffer.clone() {
        Some(path) if state.diff.is_none() => {
            if let Some(buffer) = next.buffers.get_mut(&path) {
                buffer.go_to_place(Place {
                    line: story::line_at_row(state, landed),
                    column: 1,
                });
            }
        }
        _ => next.diff_line = landed.min(rows.max(1)),
    }
}

fn slide_editor(state: &State, next: &mut State, direction: Direction) {
    let columns = fits(state).2;
    let rows = preview_rows(state);
    next.editor_hscroll = match direction {
        Direction::Left => state.editor_hscroll.saturating_sub(SLIDE_COLUMNS),
        Direction::Right => (state.editor_hscroll + SLIDE_COLUMNS)
            .min(slid_width(state, &rows).saturating_sub(columns)),
        Direction::Up | Direction::Down => return,
    };
    let Sideways::Cursor { column, .. } = sideways(state, &rows) else {
        return;
    };
    let last = next.editor_hscroll + columns.max(1) - 1;
    let landed = column.clamp(next.editor_hscroll, last) + 1;
    let Some(path) = next.current_buffer.clone() else {
        return;
    };
    let Some(buffer) = next.buffers.get_mut(&path) else {
        return;
    };
    match buffer.previewing {
        true => buffer.row_column = landed,
        false => buffer.go_to_place(Place {
            line: buffer.line,
            column: landed,
        }),
    }
}

fn on_scroll(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Scroll {
            pane,
            direction,
            at,
        } => {
            if matches!(direction, Direction::Left | Direction::Right) {
                if next.search.is_some() {
                    return Ok(settle(next, vec![], wheeled));
                }
                let effects = match pane {
                    Pane::Editor => {
                        slide_editor(state, &mut next, direction);
                        vec![]
                    }
                    Pane::Terminal | Pane::Ai | Pane::Output => match mouse::report(
                        mouse_encoding(state, pane),
                        mouse::Gesture::Wheel(direction),
                        at,
                    ) {
                        Some(bytes) => vec![Effect::SendKeys { pane, bytes }],
                        None => vec![],
                    },
                    Pane::Tree
                    | Pane::Evaluator
                    | Pane::Risk
                    | Pane::Buffers
                    | Pane::History
                    | Pane::Breakpoints
                    | Pane::Frames
                    | Pane::Diagnostics
                    | Pane::Conflicts
                    | Pane::Variables
                    | Pane::Cheatsheet => {
                        vec![]
                    }
                };
                return Ok(settle(next, effects, wheeled));
            }
            if let Some(rows) = next
                .search
                .as_ref()
                .map(|search| search::rows(&search.results).len())
            {
                let fits = search_rows(&next);
                if let Some(search) = next.search.as_mut() {
                    search.scroll = wheeled_to(direction, search.scroll, rows, fits);
                }
                return Ok(settle(next, vec![], wheeled));
            }
            let (tree_fits, editor_fits, _) = fits(state);
            match pane {
                Pane::Tree => {
                    scroll_tree(state, &mut next, direction, tree_fits);
                    vec![]
                }
                Pane::Editor => {
                    travel_editor(state, &mut next, editor_fits, |rows| {
                        wheeled_to(direction, state.editor_scroll, rows, editor_fits)
                    });
                    vec![]
                }
                Pane::Risk => {
                    next.risk_scroll = wheeled_to(
                        direction,
                        state.risk_scroll,
                        risk::list(state).len(),
                        corner_rows(state),
                    );
                    vec![]
                }
                Pane::Buffers => {
                    next.buffers_scroll = wheeled_to(
                        direction,
                        state.buffers_scroll,
                        state.buffers.len(),
                        corner_rows(state),
                    );
                    vec![]
                }
                Pane::History => {
                    next.history_scroll = wheeled_to(
                        direction,
                        state.history_scroll,
                        state.visits.len(),
                        corner_rows(state),
                    );
                    vec![]
                }
                Pane::Diagnostics => {
                    next.diagnostics_scroll = wheeled_to(
                        direction,
                        state.diagnostics_scroll,
                        lsp::listed(state).len(),
                        corner_rows(state),
                    );
                    vec![]
                }
                Pane::Conflicts => {
                    next.conflicts_scroll = wheeled_to(
                        direction,
                        state.conflicts_scroll,
                        conflict::listed(state).len(),
                        corner_rows(state),
                    );
                    vec![]
                }
                Pane::Frames => {
                    next.frames_scroll = wheeled_to(
                        direction,
                        state.frames_scroll,
                        debug::frame_rows(state).len(),
                        corner_rows(state),
                    );
                    vec![]
                }
                Pane::Breakpoints => {
                    next.breakpoints_scroll = wheeled_to(
                        direction,
                        state.breakpoints_scroll,
                        state.breakpoints.len(),
                        corner_rows(state),
                    );
                    vec![]
                }
                Pane::Variables => {
                    next.variables_scroll = wheeled_to(
                        direction,
                        state.variables_scroll,
                        debug::variables(state).len(),
                        strip_rows(state),
                    );
                    vec![]
                }
                Pane::Evaluator => vec![],
                Pane::Cheatsheet => {
                    next.cheatsheet_scroll = match direction {
                        Direction::Up => state.cheatsheet_scroll.saturating_sub(WHEEL_ROWS),
                        _ => state.cheatsheet_scroll + WHEEL_ROWS,
                    };
                    vec![]
                }
                Pane::Terminal | Pane::Ai | Pane::Output => match mouse::report(
                    mouse_encoding(state, pane),
                    mouse::Gesture::Wheel(direction),
                    at,
                ) {
                    Some(bytes) => vec![Effect::SendKeys { pane, bytes }],
                    None => vec![Effect::Scrolled(pane, direction)],
                },
            }
        }

        Event::DragMinimap(row) => {
            let fits = fits(state).1;
            travel_editor(state, &mut next, fits, |_| {
                minimap::travel(row as usize, minimap::lines(state), fits)
            });
            vec![]
        }

        Event::ScrollCheatsheet { direction, rows } => {
            next.cheatsheet_scroll = match direction {
                Direction::Up => state.cheatsheet_scroll.saturating_sub(rows),
                _ => state.cheatsheet_scroll + rows,
            };
            vec![]
        }

        Event::ScrollHover(direction) => {
            let panes = panes_of(state);
            let spot = lsp::placement(state).map(|placement| placement.spot(state, &panes));
            let said = lsp::sections(state).len();
            if let (Some(spot), Some(hover)) = (spot, next.hover.as_mut()) {
                let shown = usize::from(spot.height).saturating_sub(2);
                let last = said.saturating_sub(shown);
                hover.first = match direction {
                    Direction::Down => (hover.first + 1).min(last),
                    Direction::Up | Direction::Left | Direction::Right => {
                        hover.first.saturating_sub(1)
                    }
                };
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_resized(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Resized { width, height } => {
            next.screen_width = width;
            next.screen_height = height;
            next.strip_height = state
                .strip_height
                .map(|rows| u32::from(layout::strip_height(height, rows as u16)));
            vec![]
        }

        Event::DragStrip(rows) => {
            let rows =
                layout::strip_height(state.screen_height, rows.min(u32::from(u16::MAX)) as u16);
            next.strip_height = Some(u32::from(rows));
            vec![Effect::SaveState(state_json(&next))]
        }

        Event::RightClick(_) => vec![],

        Event::DragDivider(column) => {
            next.tree_divider = column;
            vec![Effect::SaveState(state_json(&next))]
        }

        Event::DragOutput(width) => {
            next.output_width = Some(width);
            vec![Effect::SaveState(state_json(&next))]
        }

        Event::DragAiDivider(width) => {
            next.ai_width = Some(width);
            vec![Effect::SaveState(state_json(&next))]
        }

        Event::Copy => match state.selected_text() {
            Some(text) => to_clipboard(state, text),
            None => vec![],
        },

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_editor_arrow(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorArrow(direction) if state.diff.is_some() => {
            let length = state.diff.as_ref().map_or(1, Vec::len).max(1);
            match direction {
                Direction::Down => next.diff_line = (state.diff_line + 1).min(length),
                Direction::Up => next.diff_line = state.diff_line.saturating_sub(1).max(1),
                _ => {}
            }
            vec![]
        }

        Event::EditorExtend(_) | Event::EditorExtendWord(_) if state.walking.is_some() => {
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_buffer_opened(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::BufferOpened {
            path,
            contents,
            preview,
            at,
        } => {
            let was_preview = next.preview.take();
            let already_open = next.buffers.contains_key(&path);
            if let Some(previous) = was_preview.clone() {
                if previous != path {
                    next.buffers.remove(&previous);
                }
            }
            let permanent = already_open && was_preview.as_deref() != Some(path.as_path());
            next.preview = (preview && !permanent).then(|| path.clone());
            let tab_width = next.tab_width;
            let buffer = next
                .buffers
                .entry(path.clone())
                .or_insert_with(|| Buffer::open(&contents, preview::is_markdown(&path), tab_width));
            let diverged = buffer.disk != contents && {
                buffer.follow(contents);
                buffer.changed_on_disk
            };
            next.current_buffer = Some(path.clone());
            next.restoring = next.restoring.saturating_sub(1);
            let mut effects = reveal_in_tree(&mut next, &path);
            if diverged {
                effects.push(Effect::Notify("buffer-diverged"));
            }
            if let Some(at) = at {
                land_at(&mut next, at);
                frame_site(&mut next, at);
            }
            effects
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_step_buffer(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::StepBuffer(direction) => {
            let paths: Vec<PathBuf> = next.buffers.keys().cloned().collect();
            if paths.is_empty() {
                return Ok((next, vec![]));
            }
            let at = next
                .current_buffer
                .as_ref()
                .and_then(|path| paths.iter().position(|other| other == path))
                .unwrap_or(0);
            let step = match direction {
                Direction::Left | Direction::Up => paths.len() - 1,
                _ => 1,
            };
            let path = paths[(at + step) % paths.len()].clone();
            return Ok(update(&next, Event::ShowBuffer(path)));
        }

        Event::ShowBuffer(path) => {
            next.current_buffer = Some(path.clone());
            reveal_in_tree(&mut next, &path)
        }

        Event::Indexed { walk, files, done } => {
            filter::indexed(&mut next, walk, files, done);
            vec![]
        }

        Event::OpenSearch => {
            let in_editor = matches!(
                state.selection,
                Some(
                    Selection::Buffer { .. }
                        | Selection::Lines { .. }
                        | Selection::Screen {
                            pane: Pane::Editor,
                            ..
                        }
                )
            );
            match state.selected_text().filter(|text| !text.trim().is_empty()) {
                Some(query) if in_editor => return Ok(open_search_for(next, query)),
                _ => next.search = Some(Search::default()),
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_search_word_under_cursor(
    state: &State,
    mut next: State,
    event: Event,
    wheeled: bool,
) -> Answered {
    let effects = match event {
        Event::SearchWordUnderCursor => match word_under_cursor(state) {
            Some(word) => return Ok(open_search_for(next, word)),
            None => vec![],
        },

        Event::SearchSelection => {
            let query = state
                .selected_text()
                .filter(|text| !text.trim().is_empty())
                .or_else(|| word_under_cursor(state));
            match query {
                Some(query) => return Ok(open_search_for(next, query)),
                None => vec![],
            }
        }

        Event::CloseSearch => {
            next.search = None;
            vec![]
        }

        Event::OpenFind => {
            let Some(origin) = cursor_place(state) else {
                return Ok((next, vec![]));
            };
            next.find = Some(match next.find.take() {
                Some(find) => Find {
                    keys: FindKeys::Query,
                    ..find
                },
                None => Find {
                    query: Buffer::text_box(""),
                    origin,
                    case: search::Case::Smart,
                    keys: FindKeys::Query,
                },
            });
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_find_query(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::CloseFind => {
            let Some(find) = next.find.take() else {
                return Ok((next, vec![]));
            };
            go_to_match(&mut next, find.origin);
            vec![]
        }

        Event::AcceptFind => {
            let Some(find) = next.find.as_mut() else {
                return Ok((next, vec![]));
            };
            find.keys = FindKeys::Away;
            let origin = find.origin;
            if let Some(at) = closest_match(state, origin) {
                land_on(&mut next, at);
            }
            vec![]
        }

        Event::FindKeys(keys) => {
            if let Some(find) = next.find.as_mut() {
                find.keys = keys;
            }
            if matches!(keys, FindKeys::Replace(_)) {
                next.focus = Pane::Editor;
            }
            vec![]
        }

        Event::ToggleCase => {
            if let Some(find) = next.find.as_mut() {
                find.case = match find.case.exact(find.query.shown()) {
                    true => search::Case::Ignore,
                    false => search::Case::Exact,
                };
            }
            vec![]
        }

        Event::ReplaceMatch => {
            let (Some(find), Some(cursor), false) =
                (state.find.as_ref(), cursor_place(state), previewing(state))
            else {
                return Ok((next, vec![]));
            };
            let width = find.query.shown().chars().count();
            let places = matches(state, ..);
            let after = |at: &&Place| (at.line, at.column) > (cursor.line, cursor.column);
            let Some(&target) = under_cursor(state, &places)
                .and_then(|at| places.get(at))
                .or_else(|| places.iter().find(after))
                .or_else(|| places.first())
            else {
                return Ok((next, vec![]));
            };
            let with = state.replace_with.shown().to_string();
            let Some(buffer) = current(&mut next) else {
                return Ok((next, vec![]));
            };
            let landed = buffer.replace_at(&[target], width, &with)[0];
            buffer.go_to_place(landed);
            next.selection = None;
            if let Some(at) = closest_match(&next, landed) {
                land_on(&mut next, at);
            }
            vec![]
        }

        Event::ReplaceAll => {
            let (Some(find), false) = (next.find.as_mut(), previewing(state)) else {
                return Ok((next, vec![]));
            };
            find.keys = FindKeys::Away;
            let width = find.query.shown().chars().count();
            let places = matches(state, ..);
            let with = state.replace_with.shown().to_string();
            next.selection = None;
            if let (false, Some(buffer)) = (places.is_empty(), current(&mut next)) {
                buffer.replace_at(&places, width, &with);
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_step_match(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::StepMatch(direction) => {
            let places = matches(state, ..);
            let Some(origin) = cursor_place(state) else {
                return Ok((next, vec![]));
            };
            let from = (origin.line, origin.column);
            let found = match direction {
                Direction::Right => places
                    .iter()
                    .find(|place| (place.line, place.column) > from)
                    .or_else(|| places.first()),
                _ => places
                    .iter()
                    .rev()
                    .find(|place| (place.line, place.column) < from)
                    .or_else(|| places.last()),
            };
            if let Some(&place) = found {
                land_on(&mut next, place);
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_search_query(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::SearchQuery(query) => {
            let search = next.search.get_or_insert_with(Search::default);
            search.query = Buffer::text_box(&query);
            search.selected = 0;
            search.scroll = 0;
            search::ask(&mut next)
        }

        Event::Searched {
            generation,
            hits,
            done,
        } => {
            search::arrived(&mut next, generation, hits, done);
            vec![]
        }

        Event::MoveHit(direction) => {
            if let Some(search) = next.search.as_mut() {
                let last = search.results.hits.len().saturating_sub(1);
                search.selected = match direction {
                    Direction::Down | Direction::Right => (search.selected + 1).min(last),
                    _ => search.selected.saturating_sub(1),
                };
            }
            vec![]
        }

        Event::SelectHit(index) => {
            if let Some(search) = next.search.as_mut() {
                search.selected = index;
            }
            vec![]
        }

        Event::MoveHitFile(direction) => {
            if let Some(search) = next.search.as_mut() {
                search.selected = search::in_next_file(&search.results, search.selected, direction);
            }
            vec![]
        }

        Event::OpenHit => {
            let Some(search) = state.search.as_ref() else {
                return Ok((next, vec![]));
            };
            let Some(hit) = search.results.hits.get(search.selected) else {
                return Ok((next, vec![]));
            };
            let at = Place {
                line: hit.line as usize,
                column: hit.column as usize,
            };
            next.selection = Some(Selection::Buffer {
                anchor: at,
                cursor: Place {
                    line: at.line,
                    column: at.column + search.query.shown().chars().count().saturating_sub(1),
                },
            });
            next.search = None;
            next.focus = Pane::Editor;
            vec![Effect::OpenAt {
                path: state.root.join(&hit.file),
                at,
            }]
        }

        Event::OpenEveryHit => {
            let Some(search) = state.search.as_ref() else {
                return Ok((next, vec![]));
            };
            next.search = None;
            next.focus = Pane::Editor;
            search::files(&search.results)
                .into_iter()
                .map(|file| Effect::OpenBuffer(state.root.join(file)))
                .collect()
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_complete_search(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::CompleteSearch => match state.search.as_ref() {
            Some(search) => match search::completion(search.query.shown(), &search.results) {
                Some(word) => return Ok(update(&next, Event::SearchQuery(word))),
                None => vec![],
            },
            None => vec![],
        },

        Event::JumpTo(at) => {
            land_at(&mut next, at);
            frame_site(&mut next, at);
            vec![]
        }

        Event::Filter(text) => filter::narrow(&mut next, text),

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_accept_filter(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::AcceptFilter => {
            let chosen = filter::chosen(state).map(str::to_string);
            filter::narrow(&mut next, String::new());
            match chosen {
                Some(relative) => {
                    next.tree_selection = Some(state.root.join(&relative));
                    let parts: Vec<&str> = relative.split('/').collect();
                    let mut folder = state.root.clone();
                    let mut reads = Vec::new();
                    for part in &parts[..parts.len() - 1] {
                        folder = folder.join(part);
                        reads.push(Effect::ReadFolder(folder.clone()));
                    }
                    reads
                }
                None => vec![],
            }
        }

        Event::EditorBackspace => {
            if state.diff.is_none() && state.walking.is_none() {
                erase(state, &mut next);
            }
            match editor_inserting(state) {
                true => lsp::typed(&mut next),
                false => vec![],
            }
        }

        Event::EditorDeleteWord => {
            if state.diff.is_none() && state.walking.is_none() {
                if let Some(buffer) = current(&mut next) {
                    buffer.delete_word_back();
                }
            }
            lsp::typed(&mut next)
        }

        Event::EditorArrow(direction) if previewing(state) => {
            let key = match direction {
                Direction::Up => 'k',
                Direction::Down => 'j',
                Direction::Left => 'h',
                Direction::Right => 'l',
            };
            return Ok(update(state, Event::EditorKey(key)));
        }

        Event::EditorWord(direction) if previewing(state) => {
            let key = if direction == Direction::Right {
                'w'
            } else {
                'b'
            };
            return Ok(update(state, Event::EditorKey(key)));
        }

        Event::EditorExtend(direction) | Event::EditorExtendWord(direction)
            if previewing(state) =>
        {
            let key = match (&event, direction) {
                (Event::EditorExtendWord(_), Direction::Right) => 'e',
                (Event::EditorExtendWord(_), _) => 'b',
                (_, Direction::Up) => 'k',
                (_, Direction::Down) => 'j',
                (_, Direction::Left) => 'h',
                (_, Direction::Right) => 'l',
            };
            let at = cursor_place(state).unwrap_or(Place { line: 1, column: 1 });
            let anchor = match state
                .selection
                .as_ref()
                .and_then(|held| held.screen_span(Pane::Editor))
            {
                Some((from, to)) if to == at => from,
                Some((from, to)) if from == at => to,
                _ => at,
            };
            let moved = update(state, Event::EditorKey(key)).0;
            let cursor = cursor_place(&moved).unwrap_or(at);
            let (from, to) = match (anchor.line, anchor.column) <= (cursor.line, cursor.column) {
                true => (anchor, cursor),
                false => (cursor, anchor),
            };
            return Ok(update(&moved, Event::DragText { from, to }));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_editor_arrow_2(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorArrow(direction) => {
            next.selection = None;
            next.occurrences.clear();
            if let Some(buffer) = edited_mut(&mut next) {
                buffer.arrow(direction);
            }
            vec![]
        }

        Event::EditorWord(direction) => {
            next.selection = None;
            next.occurrences.clear();
            if let Some(buffer) = edited_mut(&mut next) {
                buffer.word_motion(editor::Word::toward(direction));
            }
            vec![]
        }

        Event::EditorExtend(direction) => {
            let held = match state.selection {
                Some(Selection::Buffer { anchor, .. }) => Some(anchor),
                _ => None,
            };
            if let Some(buffer) = edited_mut(&mut next) {
                let anchor = held.unwrap_or(Place {
                    line: buffer.line,
                    column: buffer.column,
                });
                buffer.arrow(direction);
                let cursor = Place {
                    line: buffer.line,
                    column: buffer.column,
                };
                next.selection = Some(Selection::Buffer { anchor, cursor });
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_editor_extend_word(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorExtendWord(direction) => {
            let held = match state.selection {
                Some(Selection::Buffer { anchor, .. }) => Some(anchor),
                _ => None,
            };
            if let Some(buffer) = edited_mut(&mut next) {
                let anchor = held.unwrap_or(Place {
                    line: buffer.line,
                    column: buffer.column,
                });
                buffer.word_motion(if direction == Direction::Right {
                    editor::Word::End
                } else {
                    editor::Word::Back
                });
                let cursor = Place {
                    line: buffer.line,
                    column: buffer.column,
                };
                next.selection = Some(Selection::Buffer { anchor, cursor });
            }
            vec![]
        }

        Event::EditorNextOccurrence if previewing(state) => {
            next.refusal = Some(preview::Refusal::ReadOnlyPreview);
            vec![]
        }

        Event::EditorNextOccurrence => {
            take_next_occurrence(&mut next);
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn take_next_occurrence(next: &mut State) {
    let span = next.selection.as_ref().and_then(Selection::buffer_span);
    let Some(buffer) = edited_mut(next) else {
        return;
    };
    let lines: Vec<String> = buffer.shown().split('\n').map(str::to_string).collect();
    let (first, word) = match span {
        Some((from, to)) if from.line == to.line => (from, editor::span_text(&lines, from, to)),
        Some(_) => return,
        None => {
            let Some(word) = buffer.word_at_cursor() else {
                return;
            };
            let from = Place {
                line: buffer.line,
                column: buffer.word_start(buffer.line, buffer.column),
            };
            let cursor = Place {
                line: from.line,
                column: from.column + word.chars().count() - 1,
            };
            buffer.go_to_place(cursor);
            next.selection = Some(Selection::Buffer {
                anchor: from,
                cursor,
            });
            next.occurrences.clear();
            return;
        }
    };
    let taken: Vec<Place> = std::iter::once(first)
        .chain(next.occurrences.iter().copied())
        .collect();
    let all = editor::occurrences(&lines, &word);
    let untaken = all
        .iter()
        .find(|place| {
            (place.line, place.column) > (first.line, first.column) && !taken.contains(place)
        })
        .or_else(|| all.iter().find(|place| !taken.contains(place)));
    if let Some(found) = untaken {
        next.occurrences.push(*found);
        if next.focus != Pane::Evaluator {
            next.revealing = Some(*found);
        }
    }
}

fn cross_to_source(state: &State, next: &mut State) {
    next.editor_hscroll = 0;
    next.selection = None;
    let row = current_buffer(state).map_or(1, |buffer| buffer.row);
    let line = source_line(state, row);
    if let Some(buffer) = current(next) {
        buffer.previewing = false;
        buffer.line = line;
        buffer.column = 1;
    }
}

fn source_line(state: &State, row: usize) -> usize {
    buffer_rows(state)
        .get(row.saturating_sub(1))
        .map_or(1, |row| row.line)
}

fn land_at(next: &mut State, at: Place) {
    if !previewing(next) {
        if let Some(buffer) = current(next) {
            buffer.go_to_place(at);
        }
        return;
    }
    let rows = buffer_rows(next);
    let row = rows
        .iter()
        .position(|row| {
            row.line >= at.line
                && !(row.kind == preview::RowKind::Paragraph && row.pieces.is_empty())
        })
        .map_or(rows.len(), |index| index + 1);
    if let Some(buffer) = current(next) {
        buffer.row = row;
        buffer.row_column = 1;
    }
}

fn cross_to_preview(state: &State, next: &mut State) {
    next.editor_hscroll = 0;
    next.selection = None;
    let line = current_buffer(state).map_or(1, |buffer| buffer.line);
    if let Some(buffer) = current(next) {
        buffer.previewing = true;
    }
    land_at(next, Place { line, column: 1 });
}

fn on_toggle_preview(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::TogglePreview => {
            match state.current_buffer.as_ref() {
                None => next.refusal = Some(preview::Refusal::NoFileOpen),
                Some(path) if !preview::is_markdown(path) => {
                    next.refusal = Some(preview::Refusal::NotMarkdown)
                }
                Some(_) if current_buffer(state).is_some_and(|buffer| buffer.previewing) => {
                    cross_to_source(state, &mut next)
                }
                Some(_) => cross_to_preview(state, &mut next),
            }
            vec![]
        }

        Event::ToggleFold { all } => {
            match current(&mut next) {
                Some(buffer) => fold::toggle(buffer, all),
                None => next.refusal = Some(preview::Refusal::NoFileOpen),
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_editor_key(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorKey('\n')
            if current_buffer(state).is_some_and(editor::Buffer::on_fold_dots) =>
        {
            return Ok(update(state, Event::ToggleFold { all: false }));
        }

        Event::EditorKey(' ') if opens_a_chord(state) => {
            next.modal = Modal::Chord;
            vec![]
        }

        Event::EditorKey(key @ ('h' | 'l' | '0'))
            if state.diff.is_some() || state.walking.is_some() =>
        {
            next.editor_hscroll = match key {
                'h' => state.editor_hscroll.saturating_sub(SLIDE_COLUMNS),
                'l' => state.editor_hscroll + SLIDE_COLUMNS,
                _ => 0,
            };
            vec![]
        }

        Event::EditorKey('g') if previewing(state) && pending_g(state) => {
            if let Some(buffer) = current(&mut next) {
                buffer.clear_pending();
                buffer.row = 1;
                buffer.row_column = 1;
            }
            vec![]
        }

        Event::EditorKey('i') if previewing(state) => {
            cross_to_source(state, &mut next);
            if let Some(buffer) = current(&mut next) {
                buffer.mode = editor::Mode::Insert;
            }
            vec![]
        }

        Event::EditorKey('p') if previewing(state) && pending_g(state) => {
            return Err((next, Event::EditorKey('p')));
        }

        Event::EditorKey(
            'a' | 'o' | 'O' | 'I' | 'x' | 'r' | 'd' | 'D' | 'p' | 'P' | 'u' | 'U' | 'V' | 'c',
        )
        | Event::EditorUndo
        | Event::EditorRedo
            if previewing(state) =>
        {
            next.refusal = Some(preview::Refusal::ReadOnlyPreview);
            if let Some(buffer) = current(&mut next) {
                buffer.clear_pending();
            }
            vec![]
        }

        Event::EditorUndo | Event::EditorRedo => {
            if state.diff.is_none() && state.walking.is_none() {
                if let Some(buffer) = current(&mut next) {
                    if matches!(event, Event::EditorUndo) {
                        buffer.undo();
                    } else {
                        buffer.redo();
                    }
                }
            }
            vec![]
        }

        Event::EditorKey(key) if previewing(state) => {
            let rows: Vec<String> = match key {
                'w' | 'e' | 'b' => preview_rows(state).iter().map(preview::Row::text).collect(),
                _ => vec![],
            };
            let at = cursor_place(state).unwrap_or(Place { line: 1, column: 1 });
            let Some(place) = editor::moved(&rows, at, key) else {
                return Err((next, Event::EditorKey(key)));
            };
            if pending_g(state) {
                if let Some(buffer) = current(&mut next) {
                    buffer.clear_pending();
                }
                vec![Effect::notify_about("no-such-motion", format!("g{key}"))]
            } else {
                next.selection = None;
                if let Some(buffer) = current(&mut next) {
                    buffer.row = place.line;
                    buffer.row_column = place.column;
                }
                vec![]
            }
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_editor_key_2(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorKey(key) if state.diff.is_some() => {
            let length = state.diff.as_ref().map_or(1, Vec::len).max(1);
            match key {
                'j' => next.diff_line = (state.diff_line + 1).min(length),
                'k' => next.diff_line = state.diff_line.saturating_sub(1).max(1),
                'V' => next.diff_anchor = Some(state.diff_line),
                'c' => match comment_range(state) {
                    Some((file, from, to)) => {
                        open_comment_box(&mut next, file, from, to);
                    }
                    None => return Ok((next, vec![Effect::Notify("nothing-to-comment")])),
                },
                'e' => {
                    let file = state.diff_file.clone().unwrap_or_default();
                    move_to_view(&mut next, View::Edit);
                    return Ok((
                        next,
                        vec![
                            Effect::RenderView(View::Edit),
                            Effect::OpenBuffer(state.root.join(file)),
                        ],
                    ));
                }
                _ => {}
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_editor_key_3(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorKey(key) if state.walking.is_some() => {
            return Ok(walk_key(state, next, key));
        }

        // Must stay ahead of charwise `d`: a search hit leaves a selection, and `gd` would delete it
        Event::EditorKey('d') if pending_g(state) => {
            if let Some(buffer) = edited_mut(&mut next) {
                buffer.clear_pending();
            }
            lsp::ask(&mut next, lsp::About::Definition)
        }

        Event::EditorKey('m') if pending_g(state) => {
            if let Some(buffer) = edited_mut(&mut next) {
                buffer.clear_pending();
            }
            take_next_occurrence(&mut next);
            vec![]
        }

        Event::EditorKey(key) if !state.occurrences.is_empty() => {
            let picked = state.selection.as_ref().and_then(Selection::buffer_span);
            next.selection = None;
            if let Some(buffer) = edited_mut(&mut next) {
                buffer.mode = editor::Mode::Insert;
                let primary = picked.map_or(
                    Place {
                        line: buffer.line,
                        column: buffer.column,
                    },
                    |(from, _)| from,
                );
                let width = picked.map_or(0, |(from, to)| to.column + 1 - from.column);
                let places: Vec<Place> = std::iter::once(primary)
                    .chain(state.occurrences.iter().copied())
                    .collect();
                let landed = buffer.replace_at(&places, width, &key.to_string());
                buffer.go_to_place(landed[0]);
                next.occurrences = landed[1..].to_vec();
            }
            vec![]
        }

        Event::EditorKey(key)
            if editor_inserting(state)
                && editor::closes(key).is_none()
                && matches!(state.selection, Some(Selection::Buffer { .. })) =>
        {
            let (from, to) = state
                .selection
                .as_ref()
                .and_then(Selection::buffer_span)
                .expect("matched above");
            next.selection = None;
            if let Some(buffer) = edited_mut(&mut next) {
                buffer.replace_in(from, to, key);
            }
            vec![]
        }

        Event::EditorKey(key @ ('d' | 'y'))
            if matches!(state.selection, Some(Selection::Buffer { .. })) =>
        {
            let (from, to) = state
                .selection
                .as_ref()
                .and_then(Selection::buffer_span)
                .expect("matched above");
            let picked = state.selected_text();
            next.selection = None;
            if let Some(buffer) = edited_mut(&mut next) {
                match key {
                    'd' => buffer.delete_in(from, to),
                    _ => buffer.yank_in(from, to),
                }
            }
            match (key, picked) {
                ('y', Some(text)) => to_clipboard(state, text),
                _ => vec![],
            }
        }

        Event::EditorKey(key)
            if editor::closes(key).is_some()
                && editor_inserting(state)
                && matches!(state.selection, Some(Selection::Buffer { .. })) =>
        {
            let (close, anchor, cursor) = match (editor::closes(key), &state.selection) {
                (Some(close), Some(Selection::Buffer { anchor, cursor })) => {
                    (close, *anchor, *cursor)
                }
                _ => unreachable!("the guard proved both"),
            };
            let (from, to) = state
                .selection
                .as_ref()
                .and_then(Selection::buffer_span)
                .expect("the ends in order, which only this knows");
            let shifted = |place: Place| Place {
                column: place.column + usize::from(place.line == from.line),
                ..place
            };
            if let Some(buffer) = edited_mut(&mut next) {
                buffer.wrap_in(from, to, key, close);
                buffer.go_to_place(shifted(cursor));
            }
            next.selection = Some(Selection::Buffer {
                anchor: shifted(anchor),
                cursor: shifted(cursor),
            });
            vec![]
        }

        Event::EditorKey(key @ ('W' | 'B')) if normal_mode(state) => {
            let direction = if key == 'W' {
                Direction::Right
            } else {
                Direction::Left
            };
            return Ok(update(state, Event::EditorExtendWord(direction)));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_editor_key_4(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorKey('D') if normal_mode(state) => {
            match current(&mut next).is_some_and(|buffer| buffer.changed_on_disk) {
                true => {
                    next.modal = Modal::Diverged;
                    vec![]
                }
                false => vec![Effect::Notify("nothing-diverged")],
            }
        }

        Event::EditorKey('K') if normal_mode(state) && state.hover.is_some() => {
            if let Some(hover) = next.hover.as_mut() {
                hover.focused = true;
            }
            vec![]
        }
        Event::EditorKey('K') if normal_mode(state) => {
            let mut effects = lsp::ask(&mut next, lsp::About::Hover);
            if let Some(at) = cursor_place(state) {
                effects.extend(lsp::value_hover(&mut next, at));
            }
            effects
        }

        Event::EditorKey('/') if normal_mode(state) => {
            return Ok(update(state, Event::OpenFind));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_editor_key_5(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        Event::EditorKey(key @ ('n' | 'N')) if normal_mode(state) && !pending_g(state) => {
            let direction = if key == 'n' {
                Direction::Right
            } else {
                Direction::Left
            };
            Ok(update(state, Event::StepMatch(direction)))
        }

        other => Err((next, other)),
    }
}

fn on_editor_key_6(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    match event {
        Event::EditorKey(key) => Ok(editor_key(state, next, key, wheeled)),
        Event::EditorIndent(direction) => {
            let span = state.selection.as_ref().and_then(Selection::buffer_span);
            if let (Some((from, to)), Some(buffer)) = (span, edited_mut(&mut next)) {
                buffer.indent_lines(from.line, to.line, direction == Direction::Right);
                let cursor = Place {
                    line: buffer.line,
                    column: buffer.column,
                };
                next.selection = Some(Selection::Buffer {
                    anchor: Place {
                        line: from.line,
                        column: 1,
                    },
                    cursor,
                });
            }
            Ok(settle(next, vec![], wheeled))
        }
        Event::PasteFromClipboard => {
            if previewing(state) {
                next.refusal = Some(preview::Refusal::ReadOnlyPreview);
                return Ok(settle(next, vec![], wheeled));
            }
            let effects = if state.diff.is_some() || state.walking.is_some() {
                vec![]
            } else {
                vec![Effect::ReadClipboard]
            };
            Ok(settle(next, effects, wheeled))
        }
        Event::EditorPaste(text) => {
            if let Some(buffer) = edited_mut(&mut next) {
                buffer.paste(&text);
            }
            Ok(settle(next, vec![], wheeled))
        }
        other => Err((next, other)),
    }
}

fn editor_key(state: &State, next: State, key: char, wheeled: bool) -> (State, Vec<Effect>) {
    let pending_g = pending_g(state);
    let next = match search_chord(next, key, pending_g, normal_mode(state)) {
        Ok(answer) => return answer,
        Err(next) => next,
    };
    let next = match step_chord(next, key, pending_g) {
        Ok(answer) => return answer,
        Err(next) => next,
    };
    let next = match jump_chord(next, key, pending_g) {
        Ok(answer) => return answer,
        Err(next) => next,
    };
    let mut next = match list_chord(state, next, key, pending_g) {
        Ok(answer) => return answer,
        Err(next) => next,
    };
    let held = register_of(state);
    let inserting = editor_inserting(state);
    let refused = current(&mut next).and_then(|buffer| buffer.key(key));
    let mut typed = vec![];
    if inserting {
        typed = lsp::typed(&mut next);
        typed.extend(lsp::on_type(&mut next, key));
    }
    let mut effects = match register_of(&next) {
        Some(text) if key == 'y' && Some(&text) != held.as_ref() => to_clipboard(state, text),
        _ => vec![],
    };
    if let Some(chord) = refused {
        effects.push(Effect::notify_about("no-such-motion", chord));
    }
    effects.extend(typed);
    settle(next, effects, wheeled)
}

fn pending_g(state: &State) -> bool {
    state.edited().map(|buffer| buffer.pending()) == Some("g")
}

fn search_chord(
    mut next: State,
    key: char,
    pending_g: bool,
    normal_mode: bool,
) -> Result<(State, Vec<Effect>), State> {
    if !normal_mode {
        return Err(next);
    }
    if pending_g && key == 'r' {
        if let Some(buffer) = current(&mut next) {
            buffer.clear_pending();
        }
        return Ok(update(&next, Event::SearchSelection));
    }
    if key == '*' {
        return Ok(update(&next, Event::SearchWordUnderCursor));
    }
    Err(next)
}

fn step_chord(mut next: State, key: char, pending_g: bool) -> Result<(State, Vec<Effect>), State> {
    if !(pending_g && matches!(key, 't' | 'T')) {
        return Err(next);
    }
    if let Some(buffer) = current(&mut next) {
        buffer.clear_pending();
    }
    let direction = if key == 't' {
        Direction::Right
    } else {
        Direction::Left
    };
    Ok(update(&next, Event::StepBuffer(direction)))
}

fn jump_chord(mut next: State, key: char, pending_g: bool) -> Result<(State, Vec<Effect>), State> {
    if !(pending_g && matches!(key, 'p' | 'n')) {
        return Err(next);
    }
    if let Some(buffer) = current(&mut next) {
        buffer.clear_pending();
    }
    let event = match key {
        'p' => Event::JumpBack,
        _ => Event::JumpForward,
    };
    Ok(update(&next, event))
}

fn list_chord(
    state: &State,
    mut next: State,
    key: char,
    pending_g: bool,
) -> Result<(State, Vec<Effect>), State> {
    if !(key == 't' && state.view == View::Story && !pending_g && !editor_inserting(state)) {
        return Err(next);
    }
    next.story_listing = match state.story_listing {
        story::Listing::Spine => story::Listing::Files,
        story::Listing::Files => story::Listing::Spine,
    };
    Ok((next, vec![]))
}

fn on_editor_escape(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorEscape if state.walking.is_some() => {
            next.walking = None;
            vec![]
        }

        Event::EditorEscape => {
            next.modal = Modal::None;
            next.find = None;
            next.gutter = None;
            next.hover = None;
            next.diff_anchor = None;
            next.selection = None;
            next.occurrences.clear();
            if let Some(buffer) = current(&mut next) {
                buffer.escape();
            }
            vec![]
        }

        Event::WriteBuffer => match (next.current_buffer.clone(), current(&mut next)) {
            (Some(path), Some(buffer)) => {
                let contents = buffer.write();
                risk::went_stale(&mut next.risk);
                vec![Effect::WriteFile { contents, path }, Effect::ClearNotice]
            }
            _ => vec![],
        },

        Event::ReloadBuffer => match next.current_buffer.clone() {
            Some(path) => return Ok(update(state, Event::Reload(path))),
            None => vec![],
        },

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_breakpoint(mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::OfferRun(line) => {
            run::offer(&mut next, line);
            vec![]
        }
        Event::ChooseRun(action) => run::choose(&mut next, action),
        Event::EditBreakpoint(line) => {
            let Some(file) = next.current_buffer.clone() else {
                return Ok((next, vec![]));
            };
            let held = next
                .breakpoints
                .iter()
                .any(|breakpoint| breakpoint.file == file && breakpoint.line == line);
            let (mut opened, effects) = match held {
                true => (next, vec![]),
                false => update(&next, Event::ToggleBreakpoint(line)),
            };
            debug::open_box(&mut opened, file, line);
            return Ok(settle(opened, effects, wheeled));
        }
        Event::BreakpointDraft(text) => {
            if let Modal::Breakpoint { field, draft, .. } = &mut next.modal {
                match field {
                    debug::Field::Condition => draft.condition = text,
                    debug::Field::HitCount => draft.hit_count = text,
                    debug::Field::LogMessage => draft.log_message = text,
                    debug::Field::Suspend => {}
                }
            }
            vec![]
        }
        Event::BreakpointField(to) => {
            if let Modal::Breakpoint { field, .. } = &mut next.modal {
                *field = to;
            }
            vec![]
        }
        Event::SwitchSuspend => {
            let Modal::Breakpoint {
                file, line, draft, ..
            } = &mut next.modal
            else {
                return Ok((next, vec![]));
            };
            draft.suspend = match draft.suspend {
                debug::Suspend::Thread => debug::Suspend::All,
                debug::Suspend::All => debug::Suspend::Thread,
            };
            let (file, line, suspend) = (file.clone(), *line, draft.suspend);
            for breakpoint in next.breakpoints.iter_mut() {
                if breakpoint.file == file && breakpoint.line == line {
                    breakpoint.properties.suspend = suspend;
                }
            }
            vec![Effect::SaveState(state_json(&next))]
        }
        Event::ConfirmBreakpoint => {
            let Modal::Breakpoint {
                file, line, draft, ..
            } = std::mem::take(&mut next.modal)
            else {
                return Ok((next, vec![]));
            };
            for breakpoint in next.breakpoints.iter_mut() {
                if breakpoint.file == file && breakpoint.line == line {
                    breakpoint.properties = draft.clone();
                }
            }
            vec![Effect::SaveState(state_json(&next))]
        }
        Event::ToggleBreakpoint(line) => {
            let Some((file, text)) = next.current_buffer.clone().and_then(|file| {
                let text = debug::held(next.buffers.get(&file)?.shown(), line)?.to_string();
                Some((file, text))
            }) else {
                return Ok((next, vec![]));
            };
            match next
                .breakpoints
                .iter()
                .position(|breakpoint| breakpoint.file == file && breakpoint.line == line)
            {
                Some(at) => {
                    next.breakpoints.remove(at);
                }
                None => next.breakpoints.push(debug::Breakpoint {
                    file: file.clone(),
                    line,
                    text,
                    stale: false,
                    properties: debug::Properties::default(),
                }),
            }
            let mut effects = debug::breakpoints_changed(&mut next, &file);
            effects.push(Effect::SaveState(state_json(&next)));
            effects
        }

        Event::BreakpointFileRead { path, contents } => {
            for breakpoint in next.breakpoints.iter_mut().filter(|b| b.file == path) {
                breakpoint.stale =
                    debug::held(&contents, breakpoint.line) != Some(breakpoint.text.as_str());
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_resolve(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Resolve(resolution) => {
            next.modal = Modal::None;
            match resolution {
                Resolution::Reload => return Ok(update(&next, Event::ReloadBuffer)),
                Resolution::Overwrite => return Ok(update(&next, Event::WriteBuffer)),
                Resolution::Merge => {
                    let asked = next
                        .current_buffer
                        .clone()
                        .and_then(|path| next.buffers.get(&path).map(|buffer| (path, buffer)))
                        .map(|(path, buffer)| {
                            let name = path
                                .strip_prefix(&next.root)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .into_owned();
                            editor::merge_prompt(&name, &buffer.disk, buffer.shown())
                        });
                    match asked {
                        Some(prompt) => queue_for_ai(&mut next, Enter::Pressed, prompt),
                        None => vec![],
                    }
                }
            }
        }

        Event::AiExited => {
            next.ai_spoken = false;
            next.pending_prompt = None;
            if matches!(
                next.story_set,
                story::Set::Authoring { .. } | story::Set::Fixing { .. }
            ) {
                next.story_set = story::Set::AuthoringAbandoned;
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn seek(next: &mut State, sought: Option<reading::Seek>) -> Vec<Effect> {
    let Some(sought) = sought else {
        return vec![];
    };
    match sought {
        reading::Seek::Nowhere => vec![],
        reading::Seek::Ended => {
            next.reading = None;
            vec![Effect::StopSpeaking]
        }
        reading::Seek::To(at_ms) => {
            let reading = next.reading.as_mut().expect("the Reading sought in");
            reading.at_ms = at_ms;
            reading.paused = false;
            vec![Effect::SpeakFrom { at_ms }]
        }
    }
}

fn on_reading(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::StartReading => match reading::start(state) {
            Ok((reading, effects)) => {
                let stopping = next.reading.is_some().then_some(Effect::StopSpeaking);
                next.reading = Some(reading);
                stopping.into_iter().chain(effects).collect()
            }
            Err((slug, offers)) => {
                if let Some(row) = offers.and_then(|name| {
                    tools::rows(&next)
                        .iter()
                        .position(|row| row.kind == tools::Kind::Speech && row.name == name)
                }) {
                    next.modal = Modal::Tools { row };
                }
                vec![Effect::Notify(slug)]
            }
        },

        Event::StopReading => {
            if next.reading.take().is_some() {
                next.transport_lit = Some(reading::STOP);
            }
            vec![Effect::StopSpeaking]
        }
        Event::ReadingEnded => {
            next.reading = None;
            vec![Effect::StopSpeaking]
        }

        // Never signal the player: freezing it while its audio drains underruns and clicks audibly
        Event::PlayPause => match next.reading.as_mut() {
            None => {
                let (mut started, effects) = update(state, Event::StartReading);
                if started.reading.is_some() {
                    started.transport_lit = Some(reading::PLAY_PAUSE);
                }
                return Ok((started, effects));
            }
            Some(reading) if reading.paused => {
                reading.paused = false;
                let at_ms = reading.at_ms;
                next.transport_lit = Some(reading::PLAY_PAUSE);
                vec![Effect::SpeakFrom { at_ms }]
            }
            Some(reading) => {
                reading.paused = true;
                next.transport_lit = Some(reading::PLAY_PAUSE);
                vec![Effect::PauseSpeaking]
            }
        },

        Event::SetSpeed(speed) => {
            next.speech.speed = speed;
            next.transport_lit = Some(reading::SPEED);
            vec![]
        }

        Event::NextUtterance => {
            let sought = next.reading.as_ref().map(reading::Reading::forward);
            if sought.is_some() {
                next.transport_lit = Some(reading::NEXT);
            }
            seek(&mut next, sought)
        }
        Event::PreviousUtterance => {
            let sought = next.reading.as_ref().map(reading::Reading::back);
            if sought.is_some() {
                next.transport_lit = Some(reading::PREVIOUS);
            }
            seek(&mut next, sought)
        }

        Event::Speaking { at_ms, offsets } => {
            if let Some(reading) = next.reading.as_mut() {
                reading.at_ms = at_ms;
                reading.offsets = offsets;
            }
            return Ok((next, vec![]));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_debug(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::DapReceived { json, from } => debug::received(&mut next, &json, from),
        Event::DapStarted { from } => debug::started(&mut next, from),
        Event::DapGone { .. } if state.debug.is_none() || debug::waiting_on(state).is_some() => {
            next.refusal = state.refusal.clone();
            vec![]
        }
        Event::DapGone { why, from } => debug::gone(&mut next, why, from),
        Event::DapPortAnswers => debug::reattach(&mut next),
        Event::StartLaunch(name) => {
            next.modal = Modal::None;
            debug::start(&mut next, &name)
        }
        Event::DebugResume => debug::resume(&mut next),
        Event::AskAboutPause => match debug::snapshot(state) {
            Some(snapshot) => queue_for_ai(&mut next, Enter::Withheld, snapshot),
            None => vec![],
        },
        Event::DebugStep(step) => debug::step(&mut next, step),
        Event::DebugStop => debug::stop(&mut next),
        Event::DebugRestart => debug::restart(&mut next),
        Event::LeaveStepping => {
            next.stepping = false;
            vec![]
        }
        Event::MoveLaunchRow(direction) => {
            let last = state.launches.len().saturating_sub(1);
            if let Modal::Launches { row } = &mut next.modal {
                *row = match direction {
                    Direction::Down => (*row + 1).min(last),
                    Direction::Up => row.saturating_sub(1),
                    _ => (*row).min(last),
                };
            }
            vec![]
        }
        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_lsp(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::LspReceived { language, json } => lsp::received(&mut next, &language, &json),

        Event::LspStarted { .. } => Vec::new(),
        Event::ReviewFileRead { path, contents } => {
            lsp::read_for_review(&mut next, &path, &contents)
        }
        Event::LspGone { language, why } => {
            let mut effects = lsp::gone(&mut next, &language, why);
            effects.extend(debug::unhosted(&mut next, &language));
            effects
        }

        Event::CandidatesDue => lsp::ask(&mut next, lsp::About::Candidates),

        Event::PointerMoved(pointed) => {
            next.pointed_at = pointed;
            if next.hover.is_some() && !lsp::hover_stands(&next) {
                next.hover = None;
            }
            // Some terminals repeat reports for the same cell; restarting the rest timer would never fire
            let crossed = matches!(pointed, Pointed::Text(_)) && pointed != state.pointed_at;
            let effects = match crossed {
                true => vec![Effect::DwellHover(lsp::DWELL_MS)],
                false => Vec::new(),
            };
            return Ok((next, effects));
        }
        Event::HoverDue => {
            let effects = match state.pointed_at {
                Pointed::Text(at) => {
                    let mut effects = lsp::ask_at(&mut next, lsp::About::Hover, Some(at));
                    effects.extend(lsp::value_hover(&mut next, at));
                    effects
                }
                Pointed::Hover | Pointed::Breakpoint(_) | Pointed::Elsewhere => Vec::new(),
            };
            return Ok((next, effects));
        }

        Event::FormatBuffer if state.current_buffer.is_none() => {
            next.refusal = Some(preview::Refusal::NoFileOpen);
            vec![]
        }

        Event::FormatBuffer => {
            if previewing(state) {
                cross_to_source(state, &mut next);
            }
            lsp::format(&mut next).unwrap_or_else(|| format::run(&next))
        }

        Event::FormatterAnswered {
            language,
            path,
            revision,
            answer,
        } => format::answered(&mut next, &language, &path, revision, answer),

        Event::MoveCandidate(direction) => {
            if let Modal::Candidates(list) = &mut next.modal {
                list.step(direction);
            }
            vec![]
        }

        Event::AcceptCandidate => {
            if let Modal::Candidates(list) = &next.modal {
                let chosen = list.chosen().clone();
                next.modal = Modal::None;
                let (text, stops) = match chosen.snippet {
                    true => lsp::snippet(&chosen.insert),
                    false => (chosen.insert, Vec::new()),
                };
                let left = current(&mut next)
                    .map(|buffer| buffer.complete(&text, &stops))
                    .unwrap_or_default();
                if let (Some(path), false) = (next.current_buffer.clone(), left.is_empty()) {
                    next.modal = Modal::Stops { path, at: left };
                }
            }
            vec![]
        }

        Event::NextStop => {
            if let Modal::Stops { path, at } = next.modal.clone() {
                let (next_stop, rest) = at.split_first().expect("a stop left to visit");
                let place = next
                    .buffers
                    .get(&path)
                    .and_then(|buffer| buffer.place_before(*next_stop));
                next.modal = match (place, rest.is_empty()) {
                    (Some(_), false) => Modal::Stops {
                        path: path.clone(),
                        at: rest.to_vec(),
                    },
                    _ => Modal::None,
                };
                if let (Some(place), Some(buffer)) = (place, next.buffers.get_mut(&path)) {
                    buffer.go_to_place(place);
                }
            }
            vec![]
        }

        Event::MoveToolRow(direction) => {
            let last = tools::rows(&next).len().saturating_sub(1);
            if let Modal::Tools { row } = &mut next.modal {
                *row = match direction {
                    Direction::Up => row.saturating_sub(1),
                    Direction::Down => (*row + 1).min(last),
                    Direction::Left | Direction::Right => *row,
                };
            }
            vec![]
        }

        Event::InstallTool => {
            let offered = match next.modal {
                Modal::Tools { row } => tools::rows(&next).into_iter().nth(row),
                _ => None,
            };
            let Some(row) = offered else {
                return Ok(settle(next, vec![], wheeled));
            };
            match (&row.availability, &row.install) {
                (tools::Availability::Installed | tools::Availability::Partial { .. }, _) => {
                    next.refusal = Some(preview::Refusal::ToolAlreadyInstalled);
                    vec![]
                }
                (
                    tools::Availability::Unpackaged
                    | tools::Availability::Stopped
                    | tools::Availability::Unmet { .. },
                    None,
                ) => vec![],
                (tools::Availability::Available, _) if row.kind == tools::Kind::Speech => vec![],
                (tools::Availability::NeedsInstaller { installer }, _) => {
                    next.refusal = Some(preview::Refusal::NeedsInstaller(installer.clone()));
                    vec![]
                }
                (tools::Availability::Unmet { .. }, Some(install))
                    if tools::installer(install)
                        .is_some_and(|installer| !next.commands_on_path.contains(&installer)) =>
                {
                    next.refusal = tools::installer(install).map(preview::Refusal::NeedsInstaller);
                    vec![]
                }
                _ => {
                    next.modal = Modal::None;
                    vec![Effect::ReadGlobalConfig {
                        path: next.varde_home.join(startup::CONFIG_FILE),
                        kind: row.kind,
                        name: row.name,
                        write: tools::Write::Row,
                    }]
                }
            }
        }

        Event::ConfigEdited { global, project } => {
            match startup::reload(&mut next, global, project) {
                Ok(()) => vec![],
                Err(error) => vec![Effect::NotifyAbout {
                    slug: "broken-config",
                    about: error.to_string(),
                }],
            }
        }

        Event::GlobalConfigRead {
            write: tools::Write::Configures,
            text,
            ..
        } => {
            let configured = match text {
                Some(text) => tools::configure(&text),
                None => Err(startup::ConfigError {
                    file: startup::GLOBAL_LABEL.to_string(),
                    line: 1,
                    fault: startup::ConfigFault::Unreadable,
                }),
            };
            match configured {
                Err(error) => {
                    next.refusal = Some(preview::Refusal::BrokenConfig(error));
                    vec![]
                }
                Ok(None) => vec![],
                Ok(Some((contents, config))) => {
                    if next.speech.voice.is_empty() {
                        next.speech.voice = startup::speech(&config, &next.os).voice;
                    }
                    vec![Effect::WriteFile {
                        path: next.varde_home.join(startup::CONFIG_FILE),
                        contents,
                    }]
                }
            }
        }

        Event::GlobalConfigRead {
            kind,
            name,
            write: tools::Write::Row,
            text,
        } => {
            let Some(row) = tools::rows(&next)
                .into_iter()
                .find(|row| row.kind == kind && row.name == name)
            else {
                return Ok(settle(next, vec![], wheeled));
            };
            let taken = match text {
                Some(text) => tools::take(&text, kind, &name),
                None => Err(startup::ConfigError {
                    file: startup::GLOBAL_LABEL.to_string(),
                    line: 1,
                    fault: startup::ConfigFault::Unreadable,
                }),
            };
            let mut effects = vec![];
            match taken {
                Err(error) => next.refusal = Some(preview::Refusal::BrokenConfig(error)),
                Ok(written) => {
                    if let Some((contents, config)) = written {
                        for (name, server) in config.servers() {
                            next.servers.entry(name).or_insert(server);
                        }
                        for (name, formatter) in config.formatters() {
                            next.formatters.entry(name).or_insert(formatter);
                        }
                        for (name, fact) in config.facts() {
                            next.facts.entry(name).or_insert(fact);
                        }
                        effects.push(Effect::WriteFile {
                            path: next.varde_home.join(startup::CONFIG_FILE),
                            contents,
                        });
                    }
                    if let Some(install) = row.install {
                        let sentinel =
                            varde_dir(&next.root, next.sidecar.as_deref()).join(tools::SENTINEL);
                        next.install_failed.remove(&(kind, name.clone()));
                        next.installing = Some((kind, name));
                        next.focus = Pane::Terminal;
                        effects.push(Effect::RunInTerminal(tools::reported(&install, &sentinel)));
                    }
                }
            }
            effects
        }

        Event::InstallEnded(status) => match next.installing.take() {
            Some(row) if status.as_deref().map(str::trim) == Some("0") => {
                let configures = (row.0 == tools::Kind::Speech).then(|| Effect::ReadGlobalConfig {
                    path: next.varde_home.join(startup::CONFIG_FILE),
                    kind: row.0,
                    name: row.1.clone(),
                    write: tools::Write::Configures,
                });
                next.recheck = Some(row);
                std::iter::once(Effect::ProbePath)
                    .chain(configures)
                    .collect()
            }
            Some(row) => {
                next.install_failed.insert(row);
                vec![]
            }
            None => vec![],
        },

        Event::RecheckTool => {
            let row = match next.modal {
                Modal::Tools { row } => tools::rows(&next).into_iter().nth(row),
                _ => None,
            };
            match row {
                Some(row) => {
                    next.recheck = Some((row.kind, row.name));
                    vec![Effect::ProbePath]
                }
                None => vec![],
            }
        }

        Event::PathProbed => {
            if let Some((kind, name)) = next.recheck.take() {
                let found = tools::rows(&next)
                    .into_iter()
                    .find(|row| row.kind == kind && row.name == name)
                    .is_some_and(|row| match &row.availability {
                        tools::Availability::Unmet { needs } => next
                            .facts
                            .get(needs)
                            .and_then(|fact| fact.command.as_ref())
                            .is_none_or(|command| next.commands_on_path.contains(command)),
                        _ => {
                            row.kind == tools::Kind::Requirement
                                || next.commands_on_path.contains(&row.command)
                        }
                    });
                if !found && matches!(next.modal, Modal::Tools { .. }) {
                    next.modal = Modal::Restart;
                }
            }
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_ai_spoke(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::AiSpoke => {
            next.ai_spoken = true;
            match next.pending_prompt.take() {
                Some((prompt, enter)) => vec![Effect::SendKeys {
                    pane: Pane::Ai,
                    bytes: injection(&prompt, state.ai_paste, enter),
                }],
                None => vec![],
            }
        }

        Event::StartAi { command, force } => {
            next.ai_slot = layout::Slot::Ai;
            next.focus = Pane::Ai;
            let named = command.is_some();
            if let Some(command) = command {
                next.ai_command = command;
            }
            let mut effects = Vec::new();
            match (state.ai_running, named, force) {
                (true, false, _) => return Ok((next, vec![])),
                (true, true, false) => {
                    next.ai_command = state.ai_command.clone();
                    return Ok((next, vec![Effect::Notify("ai-already-running")]));
                }
                (true, true, true) => effects.push(Effect::StopAi),
                (false, _, _) => {}
            }
            next.ai_spoken = false;
            effects.push(Effect::SpawnAi {
                command: next.ai_command.clone(),
            });
            effects.push(Effect::SaveState(state_json(&next)));
            effects
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_close_buffer(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::CloseBuffer { force } => {
            let dirty = next
                .current_buffer
                .as_ref()
                .and_then(|path| next.buffers.get(path))
                .is_some_and(|buffer| buffer.is_dirty());
            if !force && dirty {
                return Ok((next, vec![Effect::Notify("unsaved-changes")]));
            }
            if let Some(path) = next.current_buffer.take() {
                next.buffers.remove(&path);
                if next.preview.as_ref() == Some(&path) {
                    next.preview = None;
                }
            }
            match next.buffers.keys().next().cloned() {
                Some(path) => return Ok(update(&next, Event::ShowBuffer(path))),
                None => next.focus = Pane::Tree,
            }
            vec![]
        }

        Event::CloseAllBuffers { force } => {
            let kept: Vec<String> = next
                .buffers
                .iter()
                .filter(|(_, buffer)| !force && buffer.is_dirty())
                .map(|(path, _)| relative(&next, path))
                .collect();
            next.buffers.retain(|_, buffer| !force && buffer.is_dirty());
            if let Some(path) = next.preview.as_ref() {
                if !next.buffers.contains_key(path) {
                    next.preview = None;
                }
            }
            let told = match next.buffers.is_empty() {
                true => Effect::Notify("buffers-closed"),
                false => Effect::notify_about("buffers-kept", kept.join(", ")),
            };
            let survived = next
                .current_buffer
                .as_ref()
                .is_some_and(|path| next.buffers.contains_key(path));
            if !survived {
                next.current_buffer = None;
                let (next, mut effects) = update(&next, Event::CloseBuffer { force: true });
                effects.push(told);
                return Ok((next, effects));
            }
            vec![told]
        }

        Event::Quit => {
            if next.buffers.values().any(|buffer| buffer.is_dirty()) {
                vec![Effect::Notify("unsaved-changes")]
            } else {
                leaving(&next)
            }
        }

        // Quit, never re-exec: a re-exec inherits the stale PATH the restart exists to refresh
        Event::Restart => {
            next.modal = Modal::None;
            return Ok(update(&next, Event::Quit));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_quit_force(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::QuitForce => leaving(&next),

        Event::ToggleCheatsheet => {
            next.ai_slot = match state.ai_slot {
                layout::Slot::Ai => layout::Slot::Cheatsheet,
                layout::Slot::Cheatsheet => layout::Slot::Ai,
            };
            vec![]
        }

        Event::ToggleField => {
            next.editor_field = !state.editor_field;
            vec![Effect::SaveState(state_json(&next))]
        }

        Event::ToggleMinimap => {
            next.minimap = !state.minimap;
            vec![Effect::SaveState(state_json(&next))]
        }

        Event::ToggleTallAi => {
            next.ai_pane = match state.ai_pane {
                layout::AiPane::Beside => layout::AiPane::Tall,
                layout::AiPane::Tall => layout::AiPane::Beside,
            };
            vec![Effect::SaveState(state_json(&next))]
        }

        Event::ToggleRiskList => take_the_corner(state, &mut next, layout::Corner::Risk),
        Event::ToggleBuffersList => take_the_corner(state, &mut next, layout::Corner::Buffers),
        Event::ToggleCursorHistory => take_the_corner(state, &mut next, layout::Corner::History),
        Event::ToggleBreakpointList => {
            take_the_corner(state, &mut next, layout::Corner::Breakpoints)
        }
        Event::ToggleDiagnosticList => {
            next.diagnostics_selection = 0;
            let asked = match state.corner {
                layout::Corner::Diagnostics(showing) => layout::Corner::Diagnostics(showing),
                _ => layout::Corner::Diagnostics(lsp::opening(state)),
            };
            take_the_corner(state, &mut next, asked)
        }
        Event::ToggleConflictList => {
            next.conflicts_selection = 0;
            take_the_corner(state, &mut next, layout::Corner::Conflicts)
        }
        Event::ShowDiagnostics(severity) => {
            if lsp::showing(state) != Some(severity) {
                next.diagnostics_selection = 0;
            }
            next.corner = layout::Corner::Diagnostics(severity);
            next.focus = Pane::Diagnostics;
            vec![Effect::SaveState(state_json(&next))]
        }

        Event::JumpBack => return Ok(history::back(state, next)),
        Event::JumpForward => return Ok(history::forward(state, next)),

        Event::ToggleRiskAll => {
            next.risk_all = !state.risk_all;
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn take_the_corner(state: &State, next: &mut State, asked: layout::Corner) -> Vec<Effect> {
    next.corner = match state.corner == asked {
        true => layout::Corner::Hidden,
        false => asked,
    };
    next.focus = next.corner.pane().unwrap_or(Pane::Tree);
    vec![Effect::SaveState(state_json(next))]
}

fn on_rebuild(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Rebuild => match (&state.checkout, &state.release) {
            _ if state.replaced => return Ok(relaunching(next)),
            (Some(checkout), _) => vec![Effect::RunInTerminal(format!(
                "{} && cargo build --release",
                tree_actions::command("cd", checkout)
            ))],
            (None, Some(release)) => vec![Effect::ReplaceBinary {
                asset: release.asset.clone(),
                checksums: release.checksums.clone(),
            }],
            (None, None) => vec![Effect::Notify("nothing-to-update")],
        },
        Event::BinaryReplaced(Ok(())) => {
            next.replaced = true;
            return Ok(relaunching(next));
        }
        Event::BinaryReplaced(Err(failed)) => vec![Effect::Notify(match failed {
            ReplaceFailed::Download => "update-download",
            ReplaceFailed::NoAsset => "update-no-asset",
            ReplaceFailed::Checksum => "update-checksum",
            ReplaceFailed::Replace => "update-replace",
        })],
        Event::ReleaseAnswered(body) => {
            next.release = body.and_then(|body| {
                startup::release(&body, &state.os, &state.arch, &state.running_version)
            });
            if let Some(release) = &next.release {
                next.update = Some(release.version.clone());
            }
            vec![]
        }

        Event::SelectIn {
            pane,
            from,
            to,
            text,
        } => {
            next.selection = Some(Selection::Screen {
                pane,
                from,
                to,
                text,
            });
            vec![]
        }

        Event::ClickText(at) => {
            next.focus = Pane::Editor;
            next.selection = None;
            next.occurrences.clear();
            if previewing(state) {
                if let Some(buffer) = current(&mut next) {
                    buffer.row = at.line.max(1);
                    buffer.row_column = at.column.max(1);
                }
            } else if let Some(buffer) = current(&mut next) {
                buffer.go_to_place(at);
            }
            vec![]
        }

        Event::HoverLink(at) => {
            next.link = at;
            vec![]
        }

        Event::HoverAction(action) => {
            next.hovered_action = action;
            vec![]
        }

        Event::HoverMinimap(on) => {
            next.hovered_minimap = on;
            vec![]
        }

        Event::AskDefinition => lsp::ask(&mut next, lsp::About::Definition),

        Event::DragText { from, to } => {
            next.selection = Some(text_selection(state, from, to));
            vec![]
        }

        Event::DoubleClickText(at) => {
            let row = match previewing(state) {
                true => preview_rows(state)
                    .get(at.line.saturating_sub(1))
                    .map(preview::Row::text),
                false => current_buffer(state)
                    .and_then(|buffer| buffer.shown().lines().nth(at.line.saturating_sub(1)))
                    .map(str::to_string),
            };
            next.selection =
                row.and_then(|row| editor::word_span(&row, at.column))
                    .map(|(from, to)| {
                        text_selection(
                            state,
                            Place {
                                line: at.line,
                                column: from,
                            },
                            Place {
                                line: at.line,
                                column: to,
                            },
                        )
                    });
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn text_selection(state: &State, from: Place, to: Place) -> Selection {
    match previewing(state) {
        true => {
            let rows: Vec<String> = preview_rows(state).iter().map(preview::Row::text).collect();
            Selection::Screen {
                pane: Pane::Editor,
                from,
                to,
                text: editor::span_text(&rows, from, to),
            }
        }
        false => Selection::Buffer {
            anchor: from,
            cursor: to,
        },
    }
}

fn on_drag_row(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::DragRow(path) => {
            next.tree_selection = Some(path);
            next.selection = None;
            vec![]
        }

        Event::ShowDiff {
            file,
            lines,
            revision,
        } => {
            if state.diff_file.as_deref() != Some(file.as_str()) {
                next.diff_line = 1;
                next.diff_anchor = None;
            }
            let length = lines.len().max(1);
            next.diff_line = next.diff_line.min(length);
            next.diff_anchor = next.diff_anchor.map(|anchor| anchor.min(length));
            next.diff = Some(lines);
            next.diff_file = Some(file);
            next.diff_revision = Some(revision);
            vec![]
        }

        Event::StoryArtifact { contents, range } => {
            let effects = match (story::parse(&contents), range) {
                (Err(because), _) => {
                    next.story_set = story::Set::Refused { because };
                    vec![]
                }
                (Ok(_), story::RangeStatus::Gone) => {
                    next.story_set = story::Set::RangeGone;
                    vec![]
                }
                (Ok(artifact), story::RangeStatus::Resolves)
                    if matches!(&state.story_set,
                        story::Set::Loaded(held) | story::Set::Fixing { artifact: held, .. }
                        if story::same_set(held, &artifact)) =>
                {
                    vec![]
                }
                (Ok(artifact), story::RangeStatus::Resolves) => {
                    let head = match story::inventory_of(&artifact) {
                        story::Inventory::Committed { head, .. } => Some(head.to_string()),
                        story::Inventory::Worktree => None,
                    };
                    let effects = vec![Effect::ReadStoryFiles {
                        repo: next.repo_root().to_path_buf(),
                        base: artifact.range.base.clone(),
                        head,
                        files: story::story_files(&artifact),
                    }];
                    let attempt = match &state.story_set {
                        story::Set::Authoring { .. } => Some(1),
                        story::Set::Fixing { attempt, .. } => Some(*attempt),
                        _ => None,
                    };
                    next.story_set = story::Set::Filling { artifact, attempt };
                    effects
                }
            };
            next.predictions_put = BTreeSet::new();
            effects
        }

        Event::StoryFiles(files) => {
            if let story::Set::Filling { artifact, attempt } = &state.story_set {
                let attempt = *attempt;
                let mut artifact = artifact.clone();
                story::fill(&mut artifact, &files);
                let problems = story::problems(&artifact, &files);
                match attempt {
                    _ if problems.is_empty() => {
                        next.story_set = story::Set::Loaded(artifact);
                        vec![]
                    }
                    Some(attempt) if attempt < story::ATTEMPTS => {
                        let prompt = story::fix_prompt(
                            &varde_dir(&next.root, next.sidecar.as_deref()),
                            &artifact,
                            &problems,
                        );
                        next.story_set = story::Set::Fixing {
                            artifact,
                            attempt: attempt + 1,
                            problems,
                        };
                        queue_for_ai(&mut next, Enter::Pressed, prompt)
                    }
                    _ => {
                        next.story_set = story::Set::Refused {
                            because: story::refusal(&problems),
                        };
                        vec![]
                    }
                }
            } else {
                vec![]
            }
        }

        Event::Story { explicit, force } => vec![resolve_story(&next, explicit, force)],

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_story_resolved(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::StoryResolved(resolution) => match resolution {
            story::Resolution::Authored => return Ok(switch_view(&next, View::Story)),
            story::Resolution::ToAuthor { spelling, out } => {
                next.modal = Modal::ConfirmStory { spelling, out };
                vec![]
            }
            story::Resolution::NoDefaultBranch => {
                move_to_view(&mut next, View::Story);
                next.story_set = story::Set::NoDefaultBranch;
                vec![Effect::RenderView(View::Story)]
            }
            story::Resolution::BadRange => {
                move_to_view(&mut next, View::Story);
                next.story_set = story::Set::BadRange;
                vec![Effect::RenderView(View::Story)]
            }
        },

        Event::ConfirmStory => match &state.modal {
            Modal::ConfirmStory { spelling, out } => {
                let spelling = spelling.clone();
                let out = out.clone();
                next.modal = Modal::None;
                move_to_view(&mut next, View::Story);
                next.story_set = story::Set::Authoring {
                    spelling: spelling.clone(),
                };
                let context = story::context_path(&out);
                let mut effects = vec![
                    Effect::RenderView(View::Story),
                    Effect::WriteStoryContext {
                        repo: next.repo_root().to_path_buf(),
                        spelling: spelling.clone(),
                        path: PathBuf::from(&context),
                    },
                ];
                effects.extend(queue_for_ai(
                    &mut next,
                    Enter::Pressed,
                    story::prompt(&spelling, &out, &context),
                ));
                effects
            }
            _ => vec![],
        },

        Event::PickBranch(url) => match url {
            None => {
                next.guest = None;
                vec![Effect::ReadBranches]
            }
            Some(url) => download_guest(&mut next, url),
        },

        Event::Branches(branching) => match branching {
            story::Branching::Listed(refs) => {
                if let story::Set::Downloading { url, .. } = &next.story_set {
                    let url = url.clone();
                    next.guest = Some(
                        varde_dir(&next.root, next.sidecar.as_deref())
                            .join(story::guest_name(&url)),
                    );
                    if !next.guests.contains(&url) {
                        next.guests.push(url);
                    }
                    next.story_set = story::Set::None;
                }
                next.modal = Modal::Branches {
                    refs,
                    filter: String::new(),
                    row: 0,
                };
                vec![]
            }
            story::Branching::NotARepository => say_in_story(&mut next, story::Set::NotARepository),
            story::Branching::Dirty => say_in_story(&mut next, story::Set::WorkingTreeDirty),
            story::Branching::DownloadFailed { how, status } => {
                say_in_story(&mut next, story::Set::DownloadFailed { how, status })
            }
        },

        Event::MoveBranchRow(direction) => {
            if let Modal::Branches { refs, filter, row } = &mut next.modal {
                let shown = story::branches(refs, filter).len();
                *row = match direction {
                    Direction::Up => row.saturating_sub(1),
                    Direction::Down => (*row + 1).min(shown.saturating_sub(1)),
                    Direction::Left | Direction::Right => *row,
                };
            }
            vec![]
        }

        Event::FilterBranches(text) => {
            if let Modal::Branches { refs, filter, row } = &mut next.modal {
                *filter = text;
                *row = (*row).min(story::branches(refs, filter).len().saturating_sub(1));
            }
            vec![]
        }

        Event::ChooseBranch => match &state.modal {
            Modal::Branches { refs, filter, row } => {
                match story::branches(refs, filter).get(*row) {
                    Some(name) => {
                        let name = name.clone();
                        next.modal = Modal::None;
                        vec![Effect::CheckoutBranch {
                            repo: next.repo_root().to_path_buf(),
                            name,
                        }]
                    }
                    None => vec![],
                }
            }
            _ => vec![],
        },

        Event::CheckoutFailed(because) => {
            vec![Effect::notify_about("checkout-failed", because)]
        }

        Event::CheckedOut { left } => {
            next.left_branch = Some(left);
            vec![resolve_story(&next, None, false)]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn resolve_story(state: &State, explicit: Option<String>, force: bool) -> Effect {
    Effect::ResolveStory {
        repo: state.repo_root().to_path_buf(),
        dir: varde_dir(&state.root, state.sidecar.as_deref()),
        explicit,
        force,
    }
}

fn say_in_story(next: &mut State, set: story::Set) -> Vec<Effect> {
    move_to_view(next, View::Story);
    next.story_set = set;
    vec![Effect::RenderView(View::Story)]
}

fn download_guest(next: &mut State, url: String) -> Vec<Effect> {
    if next.sidecar.is_none() {
        return say_in_story(next, story::Set::GuestNeedsBareWorkspace);
    }
    if !next.git_installed {
        return say_in_story(next, story::Set::NoGit);
    }
    let how = match next.guests.contains(&url) {
        true => story::Download::Fetch,
        false => story::Download::Clone,
    };
    let sidecar = varde_dir(&next.root, next.sidecar.as_deref());
    let command = story::download_command(
        how,
        &url,
        &sidecar.join(story::guest_name(&url)),
        &sidecar.join(story::DOWNLOAD_SENTINEL),
    );
    let mut effects = say_in_story(next, story::Set::Downloading { url, how });
    effects.push(Effect::RunInTerminal(command));
    effects
}

fn on_story_file_written(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::StoryFileWritten(name) => {
            next.story_sets.retain(|existing| existing != &name);
            next.story_sets.push(name);
            let mut effects = Vec::new();
            let dir = varde_dir(&next.root, next.sidecar.as_deref()).join("stories");
            let kept = story::prune(&next.story_sets, story::RETENTION);
            for pruned in next.story_sets.iter().filter(|name| !kept.contains(name)) {
                effects.push(Effect::DeleteFile(dir.join(pruned)));
                effects.push(Effect::DeleteFile(dir.join(story::context_path(pruned))));
            }
            next.story_sets = kept;
            effects
        }

        Event::EnterStory(index) => {
            let story::Set::Loaded(artifact) = &state.story_set else {
                return Ok((next, vec![]));
            };
            let Some(story) = artifact.stories.get(index) else {
                return Ok((next, vec![]));
            };
            let Some(step) = story.steps.first() else {
                return Ok((next, vec![]));
            };
            next.walking = Some(story::Walking::Story {
                story: index,
                step: 0,
                diff: story::Diff::Hidden,
            });
            arrive_at_step(&mut next, index, 0);
            next.focus = Pane::Editor;
            vec![Effect::OpenAt {
                path: state.repo_root().join(&step.site.file),
                at: Place {
                    line: step.site.from as usize,
                    column: 1,
                },
            }]
        }

        Event::StepStory(direction) => {
            let (Some(story::Walking::Story { story, step, diff }), story::Set::Loaded(artifact)) =
                (state.walking, &state.story_set)
            else {
                return Ok((next, vec![]));
            };
            let Some(walked_story) = artifact.stories.get(story) else {
                return Ok((next, vec![]));
            };
            let new_step = story::advance(walked_story.steps.len(), step, direction);
            let Some(walked_step) = walked_story.steps.get(new_step) else {
                return Ok((next, vec![]));
            };
            next.walking = Some(story::Walking::Story {
                story,
                step: new_step,
                diff,
            });
            arrive_at_step(&mut next, story, new_step);
            vec![Effect::OpenAt {
                path: state.repo_root().join(&walked_step.site.file),
                at: Place {
                    line: walked_step.site.from as usize,
                    column: 1,
                },
            }]
        }

        Event::EnterRemainder => {
            let locations = story::remainder_locations(state);
            let Some(first) = locations.first() else {
                return Ok((next, vec![]));
            };
            next.walking = Some(story::Walking::Remainder { index: 0 });
            next.focus = Pane::Editor;
            vec![Effect::OpenAt {
                path: state.repo_root().join(&first.file),
                at: Place {
                    line: first.from as usize,
                    column: 1,
                },
            }]
        }

        Event::RiskFigures {
            generation,
            figures,
            before,
        } => risk::figures_arrived(&mut next, generation, figures, before),

        Event::RecomputeRisk => {
            if next.refactor.running.is_none() {
                next.refactor = risk::Refactor::default();
            }
            vec![risk::analyse(&mut next, risk::Scope::Workspace)]
        }

        Event::StartRefactorLoop(scope) => risk::start(&mut next, scope),

        Event::StopRefactorLoop => risk::stop(&mut next),

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_pane_action(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::PaneAction(risk::RECOMPUTE) => return Ok(update(state, Event::RecomputeRisk)),
        Event::PaneAction(risk::START_LOOP) => {
            return Ok(update(
                state,
                Event::StartRefactorLoop(risk::on_screen(state.view)),
            ));
        }
        Event::PaneAction(risk::STOP_LOOP) => return Ok(update(state, Event::StopRefactorLoop)),
        Event::PaneAction(reading::PREVIOUS) => return Ok(update(state, Event::PreviousUtterance)),
        Event::PaneAction(reading::PLAY_PAUSE) => return Ok(update(state, Event::PlayPause)),
        Event::PaneAction(reading::NEXT) => return Ok(update(state, Event::NextUtterance)),
        Event::PaneAction(reading::STOP) => return Ok(update(state, Event::StopReading)),
        Event::PaneAction(reading::SPEED) => {
            return Ok(update(
                state,
                Event::SetSpeed(reading::next_speed(state.speech.speed)),
            ));
        }
        Event::PaneAction(debug::RESUME) => return Ok(update(state, Event::DebugResume)),
        Event::PaneAction(debug::ASK_AI) => return Ok(update(state, Event::AskAboutPause)),
        Event::PaneAction(debug::NEXT_THREAD) => debug::next_thread(&mut next),
        Event::PaneAction(debug::STEP_OVER) => {
            return Ok(update(state, Event::DebugStep(debug::Step::Over)));
        }
        Event::PaneAction(debug::STEP_INTO) => {
            return Ok(update(state, Event::DebugStep(debug::Step::Into)));
        }
        Event::PaneAction(debug::STEP_OUT) => {
            return Ok(update(state, Event::DebugStep(debug::Step::Out)));
        }
        Event::PaneAction(debug::STOP) => return Ok(update(state, Event::DebugStop)),
        Event::PaneAction(debug::RESTART) => return Ok(update(state, Event::DebugRestart)),
        Event::PaneAction(debug::TOGGLE_OUTPUT) => return Ok(update(state, Event::ToggleOutput)),
        Event::PaneAction(debug::CLEAR_ALL) if !state.breakpoints.is_empty() => {
            next.breakpoints.clear();
            vec![Effect::SaveState(state_json(&next))]
        }
        Event::PaneAction(debug::EXCEPTION_CLASS) => {
            if offers(&debug::transport(state), debug::EXCEPTION_CLASS) {
                next.modal = Modal::ExceptionClass;
            }
            vec![]
        }
        Event::PaneAction(_) => vec![],

        Event::TestsFinished { passed, output } => risk::tests_finished(&mut next, passed, output),

        // Returns before the clamp: a tick must not pull a wheeled view back to the cursor
        Event::Tick => {
            next.tick = state.tick.wrapping_add(1);
            return Ok((next, vec![]));
        }

        Event::StepRemainder(direction) => {
            let Some(story::Walking::Remainder { index }) = state.walking else {
                return Ok((next, vec![]));
            };
            let locations = story::remainder_locations(state);
            let new_index = story::advance(locations.len(), index, direction);
            let Some(location) = locations.get(new_index) else {
                return Ok((next, vec![]));
            };
            next.walking = Some(story::Walking::Remainder { index: new_index });
            vec![Effect::OpenAt {
                path: state.repo_root().join(&location.file),
                at: Place {
                    line: location.from as usize,
                    column: 1,
                },
            }]
        }

        Event::MoveFocus(direction) => {
            match (state.focus, direction) {
                (Pane::Terminal, Direction::Right) if state.split() + 1 < state.terminals.len() => {
                    next.terminal_split = state.split() + 1
                }
                (Pane::Terminal, Direction::Left) if state.split() > 0 => {
                    next.terminal_split = state.split() - 1
                }
                _ => next.focus = neighbour(state, direction),
            }
            vec![]
        }

        Event::SplitTerminal => {
            let from = state.split();
            next.focus = Pane::Terminal;
            next.strip = layout::Group::Shells;
            next.terminal_split = from + 1;
            vec![Effect::SplitTerminal { from }]
        }

        Event::FocusSplit(split) => {
            next.focus = Pane::Terminal;
            next.terminal_split = split;
            next.selection = None;
            vec![]
        }

        Event::ShellSpoke(split) if split == state.split() => {
            next.pending_command.take().into_iter().collect()
        }
        Event::ShellSpoke(_) => vec![],

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_move_selection(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::MoveSelection(direction) if state.focus == Pane::Risk => {
            let rows = risk::list(state).len();
            next.risk_selection = match direction {
                Direction::Down => (state.risk_selection + 1).min(rows),
                Direction::Up => state.risk_selection.saturating_sub(1),
                _ => state.risk_selection.min(rows),
            };
            next.selected_action = risk::on_actions(&next).then_some(0);
            vec![]
        }

        Event::MoveSelection(direction) if state.focus == Pane::History => {
            let last = history::list(state).len().saturating_sub(1);
            next.history_selection = match direction {
                Direction::Down => (state.history_selection + 1).min(last),
                Direction::Up => state.history_selection.saturating_sub(1),
                _ => state.history_selection.min(last),
            };
            next.selected_action = None;
            vec![]
        }

        Event::MoveSelection(direction) if state.focus == Pane::Variables => {
            let last = debug::variables(state).len().saturating_sub(1);
            next.variables_selection = match direction {
                Direction::Down => (state.variables_selection + 1).min(last),
                Direction::Up => state.variables_selection.saturating_sub(1),
                _ => state.variables_selection.min(last),
            };
            vec![]
        }

        Event::MoveSelection(direction) if state.focus == Pane::Diagnostics => {
            let last = lsp::listed(state).len().saturating_sub(1);
            next.diagnostics_selection = match direction {
                Direction::Down => (state.diagnostics_selection + 1).min(last),
                Direction::Up => state.diagnostics_selection.saturating_sub(1),
                _ => state.diagnostics_selection.min(last),
            };
            vec![]
        }

        Event::MoveSelection(direction) if state.focus == Pane::Conflicts => {
            let last = conflict::listed(state).len().saturating_sub(1);
            next.conflicts_selection = match direction {
                Direction::Down => (state.conflicts_selection + 1).min(last),
                Direction::Up => state.conflicts_selection.saturating_sub(1),
                _ => state.conflicts_selection.min(last),
            };
            vec![]
        }

        Event::MoveSelection(direction) if state.focus == Pane::Frames => {
            let last = debug::frame_rows(state).len().saturating_sub(1);
            next.frames_selection = match direction {
                Direction::Down => (state.frames_selection + 1).min(last),
                Direction::Up => state.frames_selection.saturating_sub(1),
                _ => state.frames_selection.min(last),
            };
            vec![]
        }

        Event::MoveSelection(direction) if state.focus == Pane::Breakpoints => {
            let last = debug::rows(state).saturating_sub(1);
            next.breakpoints_selection = match direction {
                Direction::Down => (state.breakpoints_selection + 1).min(last),
                Direction::Up => state.breakpoints_selection.saturating_sub(1),
                _ => state.breakpoints_selection.min(last),
            };
            next.selected_action = None;
            vec![]
        }

        Event::MoveSelection(direction) if state.focus == Pane::Buffers => {
            let last = state.buffers.len().saturating_sub(1);
            next.buffers_selection = match direction {
                Direction::Down => (state.buffers_selection + 1).min(last),
                Direction::Up => state.buffers_selection.saturating_sub(1),
                _ => state.buffers_selection.min(last),
            };
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_move_selection_2(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::MoveSelection(direction)
            if state.view == View::Story && state.story_listing == story::Listing::Spine =>
        {
            if story::spine(state).is_empty() {
                return Ok((next, vec![]));
            }
            let max = story::spine_row_count(state) - 1;
            next.story_selection = match direction {
                Direction::Down => (state.story_selection + 1).min(max),
                Direction::Up => state.story_selection.saturating_sub(1),
                _ => state.story_selection,
            };
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_move_selection_3(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::StepFilter(direction) => {
            filter::step(state, &mut next, direction);
            vec![]
        }
        Event::MoveSelection(direction) => {
            let rows = tree::visible_rows(state);
            if rows.is_empty() {
                return Ok((next, vec![]));
            }
            let current = state
                .tree_selection
                .as_ref()
                .and_then(|path| rows.iter().position(|row| &row.path == path));
            let index = match (current, direction) {
                (None, _) => 0,
                (Some(index), Direction::Down) => (index + 1).min(rows.len() - 1),
                (Some(index), Direction::Up) => index.saturating_sub(1),
                (Some(index), _) => index,
            };
            let landed = rows[index].clone();
            next.tree_selection = Some(landed.path.clone());
            next.selected_action = None;
            match state.view {
                View::Review => vec![Effect::ReadDiff(landed.path)],
                View::Edit if !landed.is_dir => vec![Effect::PreviewBuffer(landed.path)],
                View::Edit | View::Story => vec![],
            }
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_activate(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Activate if state.selected_action.is_some() => {
            let actions = selected_row_actions(state);
            match actions.get(state.selected_action.unwrap_or(0)) {
                Some(action) => {
                    next.selected_action = None;
                    return Ok(match risk::on_actions(state) {
                        true => update(&next, Event::PaneAction(action)),
                        false => update(&next, Event::RowAction(action)),
                    });
                }
                None => vec![],
            }
        }

        Event::MoveAction(direction) => {
            let actions = selected_row_actions(state);
            if actions.is_empty() {
                return Ok((next, vec![]));
            }
            next.selected_action = match (state.selected_action, direction) {
                (None, Direction::Right) => Some(0),
                (Some(at), Direction::Right) => Some((at + 1).min(actions.len() - 1)),
                (Some(0), Direction::Left) => risk::on_actions(state).then_some(0),
                (Some(at), Direction::Left) => Some(at - 1),
                (current, _) => current,
            };
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_activate_2(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Activate if state.view == View::Review => match state.tree_selection.clone() {
            Some(path) => {
                next.focus = Pane::Editor;
                vec![Effect::ReadDiff(path)]
            }
            None => vec![],
        },

        Event::Activate if state.focus == Pane::Buffers => match buffer_selected(state).cloned() {
            Some(path) => {
                let (mut shown, effects) = update(&next, Event::ShowBuffer(path));
                shown.focus = Pane::Editor;
                return Ok((shown, effects));
            }
            None => vec![],
        },

        Event::Activate if state.focus == Pane::History => {
            let (mut gone, effects) = history::go(state, next);
            if history::selected(state).is_some() {
                gone.focus = Pane::Editor;
            }
            return Ok((gone, effects));
        }

        Event::Activate if state.focus == Pane::Frames => {
            debug::choose(&mut next, state.frames_selection)
        }

        Event::Activate if state.focus == Pane::Variables => {
            debug::open(&mut next, state.variables_selection)
        }

        Event::Activate
            if state.focus == Pane::Breakpoints
                && state.breakpoints_selection < debug::switches(state).len() =>
        {
            debug::switch(&mut next, state.breakpoints_selection)
        }
        Event::Activate if state.focus == Pane::Breakpoints => match debug::selected(state) {
            Some(breakpoint) => {
                next.focus = Pane::Editor;
                vec![Effect::OpenAt {
                    path: breakpoint.file.clone(),
                    at: Place {
                        line: breakpoint.line,
                        column: 1,
                    },
                }]
            }
            None => vec![],
        },

        Event::Activate if state.focus == Pane::Diagnostics => {
            match lsp::landing(state, state.diagnostics_selection) {
                Some((path, at)) => {
                    next.focus = Pane::Editor;
                    vec![Effect::OpenAt { path, at }]
                }
                None => vec![],
            }
        }

        Event::Activate if state.focus == Pane::Conflicts => {
            match conflict::landing(state, state.conflicts_selection) {
                Some((path, at)) => {
                    next.focus = Pane::Editor;
                    vec![Effect::OpenAt { path, at }]
                }
                None => vec![],
            }
        }

        Event::Activate if state.focus == Pane::Risk => match risk::selected(state) {
            Some(function) => {
                let path = state.root.join(&function.file);
                let at = Place {
                    line: function.line,
                    column: 1,
                };
                next.focus = Pane::Editor;
                vec![Effect::OpenAt { path, at }]
            }
            None => vec![],
        },

        Event::ClickRiskRow(index) => {
            next.focus = Pane::Risk;
            next.risk_selection = index;
            let (mut opened, effects) = update(&next, Event::Activate);
            opened.focus = Pane::Risk;
            return Ok((opened, effects));
        }

        Event::ClickHistoryRow(index) => {
            next.focus = Pane::History;
            next.history_selection = index;
            let (mut opened, effects) = update(&next, Event::Activate);
            opened.focus = Pane::History;
            return Ok((opened, effects));
        }

        Event::ClickDiagnosticRow(index) => {
            next.focus = Pane::Diagnostics;
            next.diagnostics_selection = index;
            let (mut opened, effects) = update(&next, Event::Activate);
            opened.focus = Pane::Diagnostics;
            return Ok((opened, effects));
        }

        Event::ClickConflictRow(index) => {
            next.focus = Pane::Conflicts;
            next.conflicts_selection = index;
            let (mut opened, effects) = update(&next, Event::Activate);
            opened.focus = Pane::Conflicts;
            return Ok((opened, effects));
        }

        Event::AcceptConflict(side) => {
            if let Some(buffer) = current(&mut next) {
                buffer.accept(side)
            }
            vec![]
        }

        Event::ClickFrameRow(index) => {
            next.focus = Pane::Frames;
            next.frames_selection = index;
            debug::choose(&mut next, index)
        }

        Event::ClickVariablesRow(index) => {
            next.focus = Pane::Variables;
            next.variables_selection = index;
            debug::open(&mut next, index)
        }

        Event::ShowGroup(group) => {
            next.strip = group;
            if state.strip.holds(state.focus) {
                next.focus = group.pane();
            }
            vec![]
        }

        Event::ToggleOutput => {
            next.output_hidden = !state.output_hidden;
            if state.output_hidden {
                next.strip = layout::Group::Debug;
            }
            vec![]
        }

        Event::OutputSpoke => {
            next.output_unseen = !showing_output(state);
            vec![]
        }

        Event::ClickBreakpointRow(index) => {
            next.focus = Pane::Breakpoints;
            next.breakpoints_selection = index;
            let (mut opened, effects) = update(&next, Event::Activate);
            opened.focus = Pane::Breakpoints;
            return Ok((opened, effects));
        }

        Event::ClickBufferRow(index) => {
            next.focus = Pane::Buffers;
            next.buffers_selection = index;
            let (mut opened, effects) = update(&next, Event::Activate);
            opened.focus = Pane::Buffers;
            return Ok((opened, effects));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_activate_3(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        Event::Activate
            if state.view == View::Story && state.story_listing == story::Listing::Spine =>
        {
            let rows = story::spine(state);
            if rows.is_empty() {
                return Ok((next, vec![]));
            }
            if state.story_selection >= rows.len() {
                return Ok(update(state, Event::EnterRemainder));
            }
            Ok(update(state, Event::EnterStory(state.story_selection)))
        }

        other => Err((next, other)),
    }
}

fn on_activate_4(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Activate => match selected_row(state) {
            Some(row) if row.is_dir && row.expanded => {
                next.expanded.remove(&row.path);
                vec![]
            }
            Some(row) if row.is_dir => vec![Effect::ReadFolder(row.path)],
            Some(row) => return Ok(open_file(next, row.path)),
            None => vec![],
        },

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_bytes(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Bytes(bytes) => match state.focus {
            Pane::Ai if !state.ai_running => vec![],
            Pane::Output if !state.output_running => vec![],
            Pane::Terminal | Pane::Ai | Pane::Output => vec![Effect::SendKeys {
                pane: state.focus,
                bytes,
            }],
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
            | Pane::Cheatsheet => vec![],
        },

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_pasted(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Pasted(text) => {
            let asked = match state.focus {
                Pane::Terminal => Some(state.terminal_paste),
                Pane::Ai if state.ai_running => Some(state.ai_paste),
                Pane::Output if state.output_running => Some(state.output_paste),
                Pane::Output
                | Pane::Ai
                | Pane::Tree
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
                | Pane::Cheatsheet => None,
            };
            match asked {
                Some(paste) => vec![Effect::SendKeys {
                    pane: state.focus,
                    bytes: paste_bytes(&text, paste),
                }],
                None => vec![],
            }
        }

        Event::RowAction(risk::REFACTOR) => match risk::selected(state) {
            Some(function) => {
                queue_for_ai(&mut next, Enter::Pressed, risk::refactor_prompt(function))
            }
            None => vec![],
        },

        Event::RowAction(history::GO_TO) => return Ok(history::go(state, next)),
        Event::RowAction(debug::SET_VALUE) => {
            if offers(
                &debug::row_chips(state, state.variables_selection),
                debug::SET_VALUE,
            ) {
                next.modal = Modal::SetValue;
            }
            vec![]
        }
        Event::RowAction(debug::ROW_ASK_AI) => {
            let offered = offers(
                &debug::row_chips(state, state.variables_selection),
                debug::ROW_ASK_AI,
            );
            match debug::row(state).filter(|_| offered) {
                Some(row) => queue_for_ai(
                    &mut next,
                    Enter::Withheld,
                    format!("In my Paused program, {} = {}", row.expression, row.value),
                ),
                None => vec![],
            }
        }
        Event::RowAction(debug::EVALUATE) => {
            if let Some(row) = debug::row(state) {
                debug::open_evaluator(&mut next, row.expression);
            }
            vec![]
        }
        Event::HoverChip(debug::WATCH) => debug::watch_hovered(&mut next),
        Event::HoverChip(debug::EVALUATE) => {
            if let Some(hovered) = state.hover.as_ref().and_then(|hover| hover.value.as_ref()) {
                debug::open_evaluator(&mut next, hovered.expression.clone());
            }
            vec![]
        }
        Event::OpenEvaluator => {
            debug::open_evaluator(&mut next, debug::cursor_expression(state));
            vec![]
        }
        Event::RunSnippet | Event::RowAction(debug::RUN) => debug::run(&mut next),
        Event::PlaceEvaluator(at) => debug::place(&mut next, at),
        Event::MoveEvaluator(direction) => {
            debug::arrange(&mut next, direction, debug::Arrange::Moving)
        }
        Event::ResizeEvaluator(direction) => {
            debug::arrange(&mut next, direction, debug::Arrange::Sizing)
        }
        Event::SizeSnippet(rows) => {
            if let Some(evaluator) = next.evaluator.as_mut() {
                evaluator.snippet_rows = Some(rows);
            }
            vec![]
        }
        Event::ArrangeEvaluator(how) => {
            next.arranging = state.evaluator.is_some().then_some(how);
            vec![]
        }
        Event::LeaveArranging => {
            next.arranging = None;
            vec![]
        }
        Event::RowAction(debug::CANCEL) => debug::cancel(&mut next),
        Event::RowAction(debug::CLOSE) => debug::close_evaluator(&mut next),
        Event::OpenEvaluatedRow(index) => debug::open_evaluated(&mut next, index),
        Event::OpenHoverRow(index) => debug::open_hovered(&mut next, index),
        Event::RowAction(debug::COPY_VALUE) => match debug::row(state) {
            Some(row) => to_clipboard(state, row.value),
            None => vec![],
        },
        Event::RowAction(debug::COPY_EXPRESSION) => match debug::row(state) {
            Some(row) => to_clipboard(state, row.expression),
            None => vec![],
        },
        Event::RowAction(debug::WATCH) => match debug::row(state) {
            Some(row) => debug::add_watch(&mut next, row.expression),
            None => vec![],
        },
        Event::RowAction(debug::REMOVE_WATCH) => {
            if let Some(debug::Of::Watch { index, .. }) = debug::row(state).map(|row| row.of) {
                debug::remove_watch(&mut next, index);
            }
            vec![]
        }
        Event::RowAction(debug::EDIT) => {
            if let Some(chosen) = debug::selected(state).cloned() {
                debug::open_box(&mut next, chosen.file, chosen.line);
            }
            vec![]
        }
        Event::RowAction(debug::REMOVE) => match debug::selected(state).cloned() {
            Some(gone) => {
                next.breakpoints.retain(|breakpoint| *breakpoint != gone);
                vec![Effect::SaveState(state_json(&next))]
            }
            None => vec![],
        },

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn on_row_action(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::RowAction(name) => {
            let action = row_action(name);
            match selected_target(state) {
                Some(target) => return Ok(update(state, Event::Trigger(action, Some(target)))),
                None if matches!(action, Action::NewFile | Action::NewDirectory) => {
                    let root = Target::Folder(PathBuf::new());
                    return Ok(update(state, Event::Trigger(action, Some(root))));
                }
                None => vec![],
            }
        }

        Event::ClickPaletteEntry(_) if !matches!(state.modal, Modal::Palette | Modal::Chord) => {
            vec![]
        }
        Event::ClickPaletteEntry(key) => return Ok(update(state, Event::Key(key))),
        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

fn comment(
    state: &State,
    file: String,
    from_line: u32,
    to_line: u32,
    kind: String,
    body: String,
) -> Comment {
    let revision = state.diff_revision.clone().unwrap_or_default();
    let (story, step) = walking_position(state);
    Comment {
        file,
        from_line,
        to_line,
        kind,
        body,
        revision,
        story,
        step,
    }
}

fn walking_position(state: &State) -> (Option<String>, Option<u32>) {
    let Some(story::Walking::Story { story, step, .. }) = state.walking else {
        return (None, None);
    };
    let story::Set::Loaded(artifact) = &state.story_set else {
        return (None, None);
    };
    match artifact.stories.get(story) {
        Some(found) => (Some(found.name.clone()), Some(step as u32 + 1)),
        None => (None, None),
    }
}

fn injection(prompt: &str, paste: keys::Paste, enter: Enter) -> Vec<u8> {
    match enter {
        Enter::Pressed => {
            let mut bytes = b"\x15".to_vec();
            bytes.extend(paste_bytes(prompt, paste));
            bytes.push(b'\r');
            bytes
        }
        Enter::Withheld => paste_bytes(prompt, paste),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enter {
    Pressed,
    Withheld,
}

fn offers(chips: &[Chip], action: &str) -> bool {
    chips
        .iter()
        .any(|chip| chip.action == action && chip.tone != Tone::Dimmed)
}

/// Strip the end marker: untrusted text containing it would close the paste and arrive as keys
fn paste_bytes(text: &str, paste: keys::Paste) -> Vec<u8> {
    match paste {
        keys::Paste::Bracketed => {
            format!("\x1b[200~{}\x1b[201~", text.replace("\x1b[201~", "")).into_bytes()
        }
        keys::Paste::Bare => text.as_bytes().to_vec(),
    }
}

pub(crate) fn queue_for_ai(next: &mut State, enter: Enter, prompt: String) -> Vec<Effect> {
    next.ai_slot = layout::Slot::Ai;
    let mut effects = Vec::new();
    if !next.ai_running {
        effects.push(Effect::SpawnAi {
            command: next.ai_command.clone(),
        });
        next.ai_spoken = false;
    }
    if next.ai_spoken {
        effects.push(Effect::SendKeys {
            pane: Pane::Ai,
            bytes: injection(&prompt, next.ai_paste, enter),
        });
    } else {
        next.pending_prompt = Some((prompt, enter));
    }
    effects
}

pub fn reviews_dir(root: &Path, sidecar: Option<&Path>, varde_home: &Path) -> PathBuf {
    match sidecar {
        Some(_) => varde_home.join("reviews"),
        None => varde_dir(root, None).join("reviews"),
    }
}

fn submit(next: &mut State) -> Vec<Effect> {
    let number = next.reviews.iter().next_back().copied().unwrap_or(0) + 1;
    let dir = reviews_dir(&next.root, next.sidecar.as_deref(), &next.varde_home);
    let file = format!("{number:04}.json");
    let named = match next.sidecar {
        Some(_) => dir.join(&file).display().to_string(),
        None => format!("{VARDE_DIR}/reviews/{file}"),
    };
    let verdict = review::verdict(&next.comments);
    let comments = std::mem::take(&mut next.comments);

    let mut effects = vec![Effect::WriteFile {
        path: dir.join(&file),
        contents: review::artifact(&comments, verdict),
    }];
    effects.extend(queue_for_ai(
        next,
        Enter::Pressed,
        review::prompt(&comments, &named),
    ));

    next.reviews.insert(number);
    while next.reviews.len() > next.retention_limit {
        let oldest = *next.reviews.iter().next().expect("non-empty");
        next.reviews.remove(&oldest);
        effects.push(Effect::DeleteFile(dir.join(format!("{oldest:04}.json"))));
    }

    next.last_verdict = Some(verdict.to_string());
    effects
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    Open,
    Dirty,
    Current,
    CurrentDirty,
}

pub fn buffer_list(state: &State) -> Vec<&PathBuf> {
    state.buffers.keys().collect()
}

pub fn buffer_selected(state: &State) -> Option<&PathBuf> {
    buffer_list(state)
        .get(state.buffers_selection)
        .map(|path| &**path)
}

pub fn mark(state: &State, path: &std::path::Path) -> Mark {
    let Some(buffer) = state.buffers.get(path) else {
        return Mark::None;
    };
    let current = state.current_buffer.as_deref() == Some(path);
    match (current, buffer.is_dirty()) {
        (true, true) => Mark::CurrentDirty,
        (true, false) => Mark::Current,
        (false, true) => Mark::Dirty,
        (false, false) => Mark::Open,
    }
}

pub fn mode_label(state: &State, buffer: &editor::Buffer) -> &'static str {
    if state.walking.is_some() {
        return "read-only";
    }
    if let (Some(guest), Some(open)) = (&state.guest, &state.current_buffer) {
        if open.starts_with(guest) {
            return "read-only";
        }
    }
    if previewing(state) {
        return "preview";
    }
    if state.stepping {
        return "stepping";
    }
    buffer.mode.as_str()
}

fn leaving(state: &State) -> Vec<Effect> {
    let stop = Effect::StopSpeaking;
    match &state.sidecar {
        Some(sidecar) => vec![stop, Effect::DeleteDir(sidecar.clone()), Effect::Exit],
        None => vec![stop, Effect::SaveState(state_json(state)), Effect::Exit],
    }
}

fn relaunching(next: State) -> (State, Vec<Effect>) {
    let (next, effects) = update(&next, Event::Restart);
    let effects = effects
        .into_iter()
        .map(|effect| match effect {
            Effect::Exit => Effect::Relaunch,
            other => other,
        })
        .collect();
    (next, effects)
}

pub(crate) fn state_json(state: &State) -> String {
    let expanded: Vec<String> = state
        .expanded
        .iter()
        .filter_map(|path| path.strip_prefix(&state.root).ok())
        .map(|rest| rest.to_string_lossy().into_owned())
        .collect();
    let relative = |path: &std::path::Path| {
        (Some(path) != state.preview.as_deref())
            .then(|| path.strip_prefix(&state.root).ok())
            .flatten()
            .map(|rest| rest.to_string_lossy().into_owned())
    };
    let buffers: Vec<String> = state.buffers.keys().filter_map(|p| relative(p)).collect();
    let breakpoints: Vec<serde_json::Value> = state
        .breakpoints
        .iter()
        .filter_map(|breakpoint| {
            let mut saved = serde_json::json!({
                "file": breakpoint.file.strip_prefix(&state.root).ok()?,
                "line": breakpoint.line,
                "text": breakpoint.text,
            });
            let properties = &breakpoint.properties;
            for (key, text) in [
                ("condition", &properties.condition),
                ("hit_count", &properties.hit_count),
                ("log_message", &properties.log_message),
            ] {
                if !text.is_empty() {
                    saved[key] = serde_json::json!(text);
                }
            }
            if properties.suspend == debug::Suspend::All {
                saved["suspend"] = serde_json::json!("all");
            }
            Some(saved)
        })
        .collect();
    serde_json::json!({
        "last_view": format!("{:?}", state.view),
        "ai_command": state.ai_command,
        "expanded": expanded,
        "tree_divider": state.tree_divider,
        "ai_width": state.ai_width,
        "strip_height": state.strip_height,
        "output_width": state.output_width,
        "ai_pane": format!("{:?}", state.ai_pane),
        "corner": format!("{:?}", debug::resting_corner(state)),
        "editor_field": state.editor_field,
        "minimap": state.minimap,
        "buffers": buffers,
        "current_buffer": state.current_buffer.as_deref().and_then(relative),
        "breakpoints": breakpoints,
        "snippets": state.snippets,
        "exception_filters": state.exception_filters,
        "evaluator": state.evaluator_at.map(|at| serde_json::json!({
            "column": at.x,
            "row": at.y,
            "width": at.width,
            "height": at.height,
        })),
    })
    .to_string()
}

fn reveal_in_tree(next: &mut State, path: &Path) -> Vec<Effect> {
    next.tree_selection = Some(path.to_path_buf());
    let Ok(relative) = path.strip_prefix(&next.root) else {
        return vec![];
    };
    let Some(parent) = relative.parent() else {
        return vec![];
    };
    let mut folder = next.root.clone();
    let mut effects = vec![];
    for part in parent.components() {
        folder.push(part);
        next.expanded.insert(folder.clone());
        if !next.contents.contains_key(&folder) {
            effects.push(Effect::ReadFolder(folder.clone()));
        }
    }
    effects
}

fn is_known_folder(state: &State, path: &std::path::Path) -> bool {
    path.parent()
        .and_then(|parent| state.contents.get(parent))
        .zip(path.file_name())
        .is_some_and(|(entries, name)| {
            entries
                .iter()
                .any(|entry| entry.is_dir && Some(entry.name.as_str()) == name.to_str())
        })
}

fn to_clipboard(state: &State, text: String) -> Vec<Effect> {
    if state.system_clipboard {
        vec![Effect::SetClipboard(text)]
    } else {
        vec![Effect::ClipboardViaTerminal(text)]
    }
}

fn register_of(state: &State) -> Option<String> {
    state
        .buffers
        .get(state.current_buffer.as_ref()?)?
        .register_text()
}

fn comment_range(state: &State) -> Option<(String, u32, u32)> {
    let lines = state.diff.as_ref()?;
    let anchor = state.diff_anchor.unwrap_or(state.diff_line);
    let (from, to) = (anchor.min(state.diff_line), anchor.max(state.diff_line));
    let numbers: Vec<usize> = lines[from - 1..to.min(lines.len())]
        .iter()
        .filter_map(|line| line.new_line)
        .collect();
    Some((
        state.diff_file.clone()?,
        *numbers.first()? as u32,
        *numbers.last()? as u32,
    ))
}

fn open_file(mut next: State, path: PathBuf) -> (State, Vec<Effect>) {
    if next.preview.as_ref() == Some(&path) {
        next.preview = None;
    }
    next.focus = Pane::Editor;
    (next, vec![Effect::OpenBuffer(path)])
}

fn switch_view(state: &State, view: View) -> (State, Vec<Effect>) {
    if view == state.view {
        return (state.clone(), vec![]);
    }
    enter_view(state, view)
}

pub(crate) fn enter_view(state: &State, view: View) -> (State, Vec<Effect>) {
    if view == View::Review {
        return update(state, Event::OpenReviewView);
    }
    let mut next = state.clone();
    move_to_view(&mut next, view);
    let mut effects = vec![Effect::RenderView(view)];
    if next.risk.before.take().is_some() {
        next.risk.figure = risk::Figure::None;
        effects.push(risk::analyse(&mut next, risk::Scope::Workspace));
    }
    if view == View::Story {
        effects.push(Effect::ReadStories {
            dir: varde_dir(&next.root, next.sidecar.as_deref()).join("stories"),
            repo: next.repo_root().to_path_buf(),
        });
    }
    (next, effects)
}

/// Folders, never files: a file watch is lost when a tool saves by rename
pub fn watched_folders(state: &State) -> BTreeSet<PathBuf> {
    let mut folders = BTreeSet::new();
    folders.insert(state.root.clone());
    folders.insert(state.root.join(".git"));
    folders.insert(varde_dir(&state.root, state.sidecar.as_deref()).join("stories"));
    // The Refactor loop's sentinel lands here; unwatched, the loop never finishes
    folders.insert(varde_dir(&state.root, state.sidecar.as_deref()));
    folders.insert(state.varde_home.clone());
    folders.extend(state.expanded.iter().cloned());
    let open = state
        .buffers
        .keys()
        .cloned()
        .chain(state.diff_file.as_ref().map(|file| state.root.join(file)));
    folders.extend(open.filter_map(|path| path.parent().map(Path::to_path_buf)));
    folders
}

fn move_to_view(state: &mut State, view: View) {
    if state.view != view {
        if let Some(path) = state.current_buffer.take() {
            state.view_buffers.insert(state.view, path);
        }
        state.current_buffer = state
            .view_buffers
            .remove(&view)
            .filter(|path| state.buffers.contains_key(path));
        state.view = view;
    }
    if state.view != View::Review {
        state.diff = None;
        state.diff_file = None;
        state.diff_anchor = None;
    }
    if state.view != View::Story {
        state.walking = None;
    }
}

fn current(state: &mut State) -> Option<&mut Buffer> {
    let path = state.current_buffer.clone()?;
    state.buffers.get_mut(&path)
}

fn edited_mut(state: &mut State) -> Option<&mut Buffer> {
    match state.focus {
        Pane::Evaluator => state.evaluator.as_mut().map(|it| &mut it.snippet),
        _ => current(state),
    }
}

pub fn relative(state: &State, path: &Path) -> String {
    path.strip_prefix(&state.root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn neighbour(state: &State, direction: Direction) -> Pane {
    let corner = state.corner.pane();
    match (state.focus, direction) {
        (Pane::Tree, Direction::Right) => Pane::Editor,
        (Pane::Editor, Direction::Left) => Pane::Tree,
        (Pane::Editor, Direction::Right) => Pane::Ai,
        (Pane::Ai, Direction::Left) => Pane::Editor,
        (Pane::Tree, Direction::Down) => corner.unwrap_or(Pane::Terminal),
        (
            Pane::Editor
            | Pane::Ai
            | Pane::Risk
            | Pane::Buffers
            | Pane::History
            | Pane::Breakpoints,
            Direction::Down,
        ) => Pane::Terminal,
        (Pane::Risk | Pane::Buffers | Pane::History | Pane::Breakpoints, Direction::Up) => {
            Pane::Tree
        }
        (Pane::Risk | Pane::Buffers | Pane::History | Pane::Breakpoints, Direction::Right) => {
            Pane::Terminal
        }
        (Pane::Terminal, Direction::Left) => corner.unwrap_or(Pane::Terminal),
        (Pane::Terminal, Direction::Up) => Pane::Editor,
        (unchanged, _) => unchanged,
    }
}

fn selected_row_actions(state: &State) -> Vec<&'static str> {
    if risk::on_actions(state) {
        return risk::pane_actions(state);
    }
    match state.focus {
        Pane::Risk => risk::row_actions(state),
        Pane::History => history::row_actions(state),
        Pane::Breakpoints => debug::row_actions(state),
        Pane::Variables => debug::row_chips(state, state.variables_selection)
            .into_iter()
            .map(|chip| chip.action)
            .collect(),
        Pane::Tree => match state.tree_selection.as_deref() {
            Some(path) => tree::row_actions(state, path),
            None => Vec::new(),
        },
        Pane::Editor
        | Pane::Terminal
        | Pane::Ai
        | Pane::Buffers
        | Pane::Frames
        | Pane::Output
        | Pane::Diagnostics
        | Pane::Conflicts
        | Pane::Evaluator
        | Pane::Cheatsheet => Vec::new(),
    }
}

fn mouse_encoding(state: &State, pane: Pane) -> mouse::Encoding {
    match pane {
        Pane::Terminal => state.terminal_mouse,
        Pane::Ai => state.ai_mouse,
        Pane::Output => state.output_mouse,
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
        | Pane::Cheatsheet => mouse::Encoding::None,
    }
}

pub fn showing_output(state: &State) -> bool {
    state.strip == layout::Group::Debug && state.output_running && !state.output_hidden
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tab {
    pub group: layout::Group,
    pub lit: bool,
    pub unseen: bool,
}

pub fn group_tabs(state: &State) -> Vec<Tab> {
    [layout::Group::Shells]
        .into_iter()
        .chain(state.debug.as_ref().map(|_| layout::Group::Debug))
        .map(|group| Tab {
            group,
            lit: group == state.strip,
            unseen: group == layout::Group::Debug && state.output_unseen,
        })
        .collect()
}

pub fn shapes(state: &State) -> layout::Shapes {
    layout::Shapes {
        ai: state.ai_pane,
        slot: state.ai_slot,
        corner: state.corner,
        group: state.strip,
        strip: state.strip_height.map(|height| height as u16),
        output: match showing_output(state) {
            true => layout::Output::Shown(state.output_width.map(|width| width as u16)),
            false => layout::Output::Away,
        },
        evaluator: state
            .evaluator
            .is_some()
            .then_some(state.evaluator_at)
            .flatten(),
    }
}

pub fn showing_transport(state: &State) -> bool {
    state.strip == layout::Group::Debug || state.debug.is_none()
}

pub fn transport_area(state: &State, strip: layout::Area) -> layout::Area {
    layout::Area {
        width: strip
            .width
            .saturating_sub(layout::strip_width(&group_labels(state))),
        ..strip
    }
}

pub fn group_labels(state: &State) -> Vec<String> {
    group_tabs(state)
        .iter()
        .map(|tab| format!(" {} ", tab.group.label()))
        .collect()
}

pub(crate) fn panes_of(state: &State) -> layout::Layout {
    layout::panes(
        state.screen_width,
        state.screen_height,
        state.tree_divider as u16,
        state.ai_width.map(|width| width as u16),
        story::band_height(state),
        story::step_menu_width(state),
        shapes(state),
    )
}

pub fn strip_rows(state: &State) -> usize {
    panes_of(state).terminal.height.saturating_sub(2) as usize
}

pub fn cheatsheet_fits(state: &State) -> usize {
    panes_of(state).ai.height.saturating_sub(2) as usize
}

pub fn corner_rows(state: &State) -> usize {
    panes_of(state).corner.height.saturating_sub(2) as usize
}

fn search_rows(state: &State) -> usize {
    layout::search_hit_rows(state.screen_width, state.screen_height)
}

pub fn fits(state: &State) -> (usize, usize, usize) {
    fits_in(state, &panes_of(state))
}

pub fn fits_in(state: &State, panes: &layout::Layout) -> (usize, usize, usize) {
    let filter = tree::filter_rows(state.view) as u16 + story::title_rows(state) as u16;
    (
        panes.tree.height.saturating_sub(2 + filter) as usize,
        panes.editor.height.saturating_sub(2) as usize,
        panes
            .editor
            .width
            .saturating_sub(2 + gutter(state) + minimap::width(state)) as usize,
    )
}

fn editor_focus(state: &State, rows: &[preview::Row]) -> (usize, usize) {
    if story::refused(state) {
        return (0, 0);
    }
    if previewing(state) {
        let row = current_buffer(state).map_or(1, |buffer| buffer.row);
        return (
            row.saturating_sub(1).min(rows.len().saturating_sub(1)),
            rows.len(),
        );
    }
    match (&state.diff, state.current_buffer.as_ref()) {
        (Some(diff), _) => (state.diff_line.saturating_sub(1), diff.len()),
        (None, Some(path)) => match state.buffers.get(path) {
            Some(buffer) => {
                let lines = buffer.shown().lines().count();
                (
                    story::row_of(state, buffer.line as u32).saturating_sub(1),
                    story::rows(state, lines).count(),
                )
            }
            None => (0, 0),
        },
        _ => (0, 0),
    }
}

enum Sideways {
    Cursor { column: usize, width: usize },
    Read,
}

fn sideways(state: &State, rows: &[preview::Row]) -> Sideways {
    let Some(buffer) = state
        .current_buffer
        .as_ref()
        .filter(|_| state.diff.is_none() && state.walking.is_none())
        .and_then(|path| state.buffers.get(path))
    else {
        return Sideways::Read;
    };
    if buffer.previewing {
        return Sideways::Cursor {
            column: buffer.row_column.saturating_sub(1),
            width: rows
                .get(buffer.row.saturating_sub(1))
                .map_or(0, |row| row.text().chars().count()),
        };
    }
    let width = buffer
        .shown()
        .split('\n')
        .nth(buffer.line.saturating_sub(1))
        .map_or(0, |line| line.chars().count());
    Sideways::Cursor {
        column: buffer.column.saturating_sub(1),
        width,
    }
}

fn slid_width(state: &State, rows: &[preview::Row]) -> usize {
    if story::refused(state) {
        return 0;
    }
    if let Some(diff) = &state.diff {
        return diff
            .iter()
            .map(|line| line.text.chars().count())
            .max()
            .unwrap_or(0);
    }
    if previewing(state) {
        return rows
            .iter()
            .map(|row| row.text().chars().count())
            .max()
            .unwrap_or(0);
    }
    current_buffer(state).map_or(0, |buffer| {
        buffer
            .shown()
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0)
    })
}

fn word_under_cursor(state: &State) -> Option<String> {
    let path = state.current_buffer.as_ref()?;
    state.buffers.get(path)?.word_at_cursor()
}

const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn spinner(tick: u64) -> char {
    SPINNER[(tick % SPINNER.len() as u64) as usize]
}

pub fn link(state: &State) -> Option<(usize, usize, usize)> {
    let at = state.link?;
    let buffer = state.buffers.get(state.current_buffer.as_ref()?)?;
    let (from, to) = buffer.word_span(at.line, at.column)?;
    Some((at.line, from, to))
}

fn open_search_for(mut next: State, query: String) -> (State, Vec<Effect>) {
    next.search = Some(Search::default());
    update(&next, Event::SearchQuery(query.trim().to_string()))
}

pub fn gutter(state: &State) -> u16 {
    layout::gutter(match (previewing(state), state.diff.is_some()) {
        (true, _) => layout::Gutter::None,
        (_, true) => layout::Gutter::NumbersAndMarker,
        _ => layout::Gutter::Numbers,
    })
}

pub fn previewing(state: &State) -> bool {
    state.diff.is_none()
        && state.walking.is_none()
        && current_buffer(state).is_some_and(|buffer| buffer.previewing)
}

pub fn preview_rows(state: &State) -> Vec<preview::Row> {
    if !previewing(state) {
        return Vec::new();
    }
    buffer_rows(state)
}

fn buffer_rows(state: &State) -> Vec<preview::Row> {
    let Some(buffer) = current_buffer(state) else {
        return Vec::new();
    };
    preview::rows(buffer.shown(), preview_columns(state))
}

pub fn preview_columns(state: &State) -> usize {
    let panes = layout::panes(
        state.screen_width,
        state.screen_height,
        state.tree_divider as u16,
        state.ai_width.map(|width| width as u16),
        story::band_height(state),
        story::step_menu_width(state),
        shapes(state),
    );
    panes.editor.width.saturating_sub(2) as usize
}

fn open_comment_box(next: &mut State, file: String, from: u32, to: u32) {
    next.gutter = Some((file, from, to));
    next.modal = Modal::Comment;
    let mut body = Buffer::open("", false, next.tab_width);
    body.mode = editor::Mode::Insert;
    next.comment = Some(body);
}

fn refuse_guest_edits(state: &State, next: &mut State) -> bool {
    let Some(guest) = &state.guest else {
        return false;
    };
    let mut refused = false;
    for (path, clean) in &state.buffers {
        // `get`, not indexing: the event may have closed this buffer
        if !path.starts_with(guest) || !next.buffers.get(path).is_some_and(Buffer::is_dirty) {
            continue;
        }
        next.buffers.insert(path.clone(), clean.clone());
        refused = true;
    }
    refused
}

pub fn current_buffer(state: &State) -> Option<&Buffer> {
    state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
}

pub fn word_occurrences(state: &State, lines: impl RangeBounds<usize>) -> Vec<Place> {
    if previewing(state) {
        return Vec::new();
    }
    let Some(buffer) = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
    else {
        return Vec::new();
    };
    let Some(word) = buffer.word_at_cursor() else {
        return Vec::new();
    };
    let part = |c: char| c.is_alphanumeric() || c == '_';
    let mut places = Vec::new();
    for (number, line) in buffer.lines_within(lines) {
        for (at, _) in line.match_indices(&word) {
            if line[..at].chars().next_back().is_some_and(part)
                || line[at + word.len()..].chars().next().is_some_and(part)
            {
                continue;
            }
            places.push(Place {
                line: number,
                column: line[..at].chars().count() + 1,
            });
        }
    }
    places
}

pub fn matches(state: &State, lines: impl RangeBounds<usize>) -> Vec<Place> {
    let Some((query, case)) = state
        .find
        .as_ref()
        .map(|find| (find.query.shown(), find.case))
        .filter(|(query, _)| !query.is_empty())
    else {
        return Vec::new();
    };
    if previewing(state) {
        return buffer_rows(state)
            .iter()
            .enumerate()
            .filter(|(index, _)| lines.contains(&(index + 1)))
            .flat_map(|(index, row)| {
                search::occurrences(query, &row.text(), case)
                    .into_iter()
                    .map(move |column| Place {
                        line: index + 1,
                        column: column as usize,
                    })
            })
            .collect();
    }
    let Some(buffer) = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
    else {
        return Vec::new();
    };
    buffer
        .lines_within(lines)
        .flat_map(|(number, line)| {
            search::occurrences(query, line, case)
                .into_iter()
                .map(move |column| Place {
                    line: number,
                    column: column as usize,
                })
        })
        .collect()
}

fn under_cursor(state: &State, places: &[Place]) -> Option<usize> {
    let cursor = cursor_place(state)?;
    let width = state.find.as_ref()?.query.shown().chars().count();
    places.iter().position(|at| {
        at.line == cursor.line && at.column <= cursor.column && cursor.column < at.column + width
    })
}

pub fn find_line(state: &State) -> Vec<(String, Option<FindIcon>)> {
    let Some(find) = state.find.as_ref() else {
        return Vec::new();
    };
    let text = find.query.shown();
    let query = match find.keys {
        FindKeys::Query => {
            let at = find.query.column.saturating_sub(1);
            let before: String = text.chars().take(at).collect();
            let after: String = text.chars().skip(at).collect();
            format!("/{before}█{after}")
        }
        _ => format!("/{text}"),
    };
    let places = matches(state, ..);
    let count = match (under_cursor(state, &places), places.len()) {
        (_, 0) if text.is_empty() => String::new(),
        (_, 0) => "no match".to_string(),
        (Some(at), all) => format!("{} of {all}", at + 1),
        (None, all) => format!("{all} found"),
    };
    let mut pieces = vec![(format!(" {query} "), None), (count, None)];
    for (icon, label) in FIND_ICONS {
        pieces.push((" ".to_string(), None));
        pieces.push((label.to_string(), Some(icon)));
    }
    pieces.push((" ".to_string(), None));
    pieces
}

pub fn changed_lines(state: &State) -> Vec<usize> {
    authorship::traced_lines(state)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter(|(_, at)| at.is_none())
        .map(|(index, _)| index + 1)
        .collect()
}

pub fn echoes(state: &State, lines: impl RangeBounds<usize>) -> Vec<Place> {
    let Some((from, to)) = state.selection.as_ref().and_then(Selection::buffer_span) else {
        return Vec::new();
    };
    if from.line != to.line {
        return Vec::new();
    }
    let Some(word) = state.selected_text().filter(|text| !text.trim().is_empty()) else {
        return Vec::new();
    };
    let Some(buffer) = current_buffer(state) else {
        return Vec::new();
    };
    buffer
        .lines_within(lines)
        .flat_map(|(number, line)| {
            line.match_indices(&word).map(move |(at, _)| Place {
                line: number,
                column: line[..at].chars().count() + 1,
            })
        })
        .filter(|at| *at != from)
        .collect()
}

fn closest_match(state: &State, from: Place) -> Option<Place> {
    let places = matches(state, ..);
    places
        .iter()
        .find(|at| (at.line, at.column) >= (from.line, from.column))
        .or_else(|| places.first())
        .copied()
}

fn cursor_place(state: &State) -> Option<Place> {
    let buffer = current_buffer(state)?;
    Some(if previewing(state) {
        Place {
            line: buffer.row,
            column: buffer.row_column,
        }
    } else {
        Place {
            line: buffer.line,
            column: buffer.column,
        }
    })
}

fn go_to_match(next: &mut State, at: Place) {
    let row = previewing(next);
    if let Some(buffer) = current(next) {
        if row {
            buffer.row = at.line;
            buffer.row_column = at.column;
        } else {
            buffer.go_to_place(at);
        }
    }
}

fn land_on(next: &mut State, at: Place) {
    let length = next
        .find
        .as_ref()
        .map_or(1, |find| find.query.shown().chars().count());
    let end = Place {
        line: at.line,
        column: at.column + length - 1,
    };
    next.selection = Some(if previewing(next) {
        let rows: Vec<String> = preview_rows(next).iter().map(preview::Row::text).collect();
        Selection::Screen {
            pane: Pane::Editor,
            from: at,
            to: end,
            text: editor::span_text(&rows, at, end),
        }
    } else {
        Selection::Buffer {
            anchor: at,
            cursor: end,
        }
    });
    go_to_match(next, at);
}

fn frame_site(next: &mut State, at: Place) {
    let story::SiteMark::Site { file, from, to, .. } = story::mark(next) else {
        return;
    };
    if at.line != from as usize || story::shown_file(next) != file {
        return;
    }
    let rows = fits(next).1;
    next.editor_scroll = layout::frame(
        story::row_of(next, from).saturating_sub(1),
        story::row_of(next, to).saturating_sub(1),
        rows,
    );
}

fn arrive_at_step(next: &mut State, story: usize, step: usize) {
    let has_prediction = story::current_step(next).is_some_and(|step| step.prediction.is_some());
    if has_prediction && !next.predictions_put.contains(&(story, step)) {
        next.predictions_put.insert((story, step));
        next.modal = Modal::Prediction { picked: None };
    } else if matches!(next.modal, Modal::Prediction { .. }) {
        next.modal = Modal::None;
    }
}

fn pick_prediction(state: &State, mut next: State, key: char) -> (State, Vec<Effect>) {
    if story::prediction_choices(state).is_empty() {
        return (next, vec![]);
    }
    let index = (key as u8 - b'1') as usize;
    next.modal = Modal::Prediction {
        picked: Some(index),
    };
    (next, vec![])
}

fn walk_key(state: &State, next: State, key: char) -> (State, Vec<Effect>) {
    match walk_step_key(state, next, key) {
        Ok(answer) => answer,
        Err(next) => walk_place_key(state, next, key),
    }
}

fn walk_step_key(state: &State, next: State, key: char) -> Result<(State, Vec<Effect>), State> {
    let event = match key {
        'n' if matches!(state.walking, Some(story::Walking::Remainder { .. })) => {
            Event::StepRemainder(Direction::Right)
        }
        'p' if matches!(state.walking, Some(story::Walking::Remainder { .. })) => {
            Event::StepRemainder(Direction::Left)
        }
        'n' => Event::StepStory(Direction::Right),
        'p' => Event::StepStory(Direction::Left),
        'j' => Event::Scroll {
            pane: Pane::Editor,
            direction: Direction::Down,
            at: Place { line: 0, column: 0 },
        },
        'k' => Event::Scroll {
            pane: Pane::Editor,
            direction: Direction::Up,
            at: Place { line: 0, column: 0 },
        },
        _ => return Err(next),
    };
    Ok(update(state, event))
}

fn walk_place_key(state: &State, mut next: State, key: char) -> (State, Vec<Effect>) {
    match key {
        'e' => walk_to_step_file(state, next),
        'D' => {
            next.modal = if state.modal == Modal::StepDetail {
                Modal::None
            } else {
                Modal::StepDetail
            };
            (next, vec![])
        }
        'g' => walk_to_citation(state, next),
        'd' => {
            if let Some(story::Walking::Story { diff, .. }) = &mut next.walking {
                *diff = match diff {
                    story::Diff::Hidden => story::Diff::Shown,
                    story::Diff::Shown => story::Diff::Hidden,
                };
            }
            (next, vec![])
        }
        't' => {
            next.story_listing = match state.story_listing {
                story::Listing::Spine => story::Listing::Files,
                story::Listing::Files => story::Listing::Spine,
            };
            (next, vec![])
        }
        'c' => match story::current_step(state) {
            Some(step) => {
                open_comment_box(
                    &mut next,
                    step.site.file.clone(),
                    step.site.from,
                    step.site.to,
                );
                (next, vec![])
            }
            None => (next, vec![]),
        },
        _ => (next, vec![]),
    }
}

fn walk_to_step_file(state: &State, mut next: State) -> (State, Vec<Effect>) {
    let Some(step) = story::current_step(state) else {
        return (next, vec![]);
    };
    let file = step.site.file.clone();
    move_to_view(&mut next, View::Edit);
    (
        next,
        vec![
            Effect::RenderView(View::Edit),
            Effect::OpenBuffer(state.root.join(file)),
        ],
    )
}

fn walk_to_citation(state: &State, next: State) -> (State, Vec<Effect>) {
    let cite = story::current_step(state)
        .and_then(|step| step.values.iter().find_map(|value| value.cite.as_ref()));
    let Some(cite) = cite else {
        return (next, vec![]);
    };
    (
        next,
        vec![Effect::OpenAt {
            path: state.root.join(&cite.file),
            at: Place {
                line: cite.line as usize,
                column: 1,
            },
        }],
    )
}

fn normal_mode(state: &State) -> bool {
    state
        .edited()
        .is_some_and(|buffer| buffer.mode == editor::Mode::Normal)
}

pub(crate) fn editor_inserting(state: &State) -> bool {
    state
        .edited()
        .is_some_and(|buffer| buffer.mode == editor::Mode::Insert)
}

fn selected_row(state: &State) -> Option<tree::Row> {
    let selection = state.tree_selection.as_ref()?;
    tree::rows(state)
        .into_iter()
        .find(|row| &row.path == selection)
}

fn selected_target(state: &State) -> Option<Target> {
    let row = selected_row(state)?;
    let relative = row.path.strip_prefix(&state.root).ok()?.to_path_buf();
    Some(if row.is_dir {
        Target::Folder(relative)
    } else {
        Target::File(relative)
    })
}

fn row_action(name: &str) -> Action {
    match name {
        "new-file" => Action::NewFile,
        "new-directory" => Action::NewDirectory,
        "go-here" => Action::GoHere,
        "search-here" => Action::SearchHere,
        "delete" => Action::Delete,
        "copy-path" => Action::CopyPath,
        other => unreachable!("unknown row action {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vardes_own_folder_is_the_sidecar_or_under_the_root() {
        let root = Path::new("/home/me/projects/varde");
        assert_eq!(
            varde_dir(root, None),
            PathBuf::from("/home/me/projects/varde/.varde")
        );
        let sidecar = Path::new("/home/me/.varde/paths/%home%me%projects%varde-91");
        assert_eq!(varde_dir(root, Some(sidecar)), sidecar);
    }

    #[test]
    fn the_candidate_list_goes_when_the_keyboard_leaves_the_editor() {
        let offering = State {
            modal: Modal::Candidates(lsp::Candidates::offering(
                &State::default(),
                vec![lsp::Candidate {
                    label: "workspace_root".to_string(),
                    insert: "workspace_root".to_string(),
                    filter: None,
                    sort: None,
                    snippet: false,
                }],
                lsp::Ask {
                    path: std::path::PathBuf::from("/src/lib.rs"),
                    place: Place { line: 1, column: 2 },
                    revision: 1,
                    about: lsp::About::Candidates,
                },
            )),
            focus: Pane::Editor,
            current_buffer: Some(std::path::PathBuf::from("/src/lib.rs")),
            ..State::default()
        };
        let stepped = update(&offering, Event::MoveCandidate(Direction::Down)).0;
        assert!(matches!(stepped.modal, Modal::Candidates(_)));
        let elsewhere = update(&offering, Event::ClickPane(Pane::Terminal)).0;
        assert_eq!(elsewhere.modal, Modal::None);
    }

    #[test]
    fn a_click_in_a_shell_drops_what_was_picked() {
        let picked = State {
            selection: Some(Selection::Screen {
                pane: Pane::Terminal,
                from: Place { line: 1, column: 1 },
                to: Place { line: 1, column: 7 },
                text: "ripgrep".to_string(),
            }),
            ..State::default()
        };
        let clicked = update(&picked, Event::FocusSplit(0)).0;
        assert_eq!(clicked.selection, None);
        assert_eq!(clicked.focus, Pane::Terminal);
    }

    #[test]
    fn a_paste_into_the_snippet_is_not_refused_by_a_preview_behind_it() {
        let (opened, _) = update(
            &State::default(),
            Event::BufferOpened {
                path: PathBuf::from("/w/notes.md"),
                contents: "# notes".to_string(),
                preview: false,
                at: None,
            },
        );
        let mut state = debug::paused(opened);
        if let Some(buffer) = current(&mut state) {
            buffer.previewing = true;
        }
        debug::open_evaluator(&mut state, "count".to_string());
        let (pasted, effects) = update(&state, Event::PasteFromClipboard);
        assert_eq!(pasted.refusal, None);
        assert!(effects.contains(&Effect::ReadClipboard));
    }

    #[test]
    fn a_snippet_sequence_goes_when_the_keyboard_leaves_the_editor() {
        let path = std::path::PathBuf::from("/src/lib.rs");
        let filling = State {
            modal: Modal::Stops {
                path: path.clone(),
                at: vec![editor::Tail(0)],
            },
            current_buffer: Some(path),
            focus: Pane::Editor,
            ..State::default()
        };
        let typed = update(&filling, Event::EditorKey('x')).0;
        assert!(matches!(typed.modal, Modal::Stops { .. }));
        let elsewhere = update(&filling, Event::ClickPane(Pane::Terminal)).0;
        assert_eq!(elsewhere.modal, Modal::None);
    }

    #[test]
    fn a_stop_the_buffer_can_no_longer_name_ends_the_sequence() {
        let path = std::path::PathBuf::from("/src/lib.rs");
        let mut state = State {
            modal: Modal::Stops {
                path: path.clone(),
                at: vec![editor::Tail(40), editor::Tail(0)],
            },
            current_buffer: Some(path.clone()),
            focus: Pane::Editor,
            ..State::default()
        };
        let mut buffer = editor::Buffer::open("one\ntwo", false, 4);
        buffer.go_to_place(Place { line: 2, column: 2 });
        state.buffers.insert(path.clone(), buffer);
        let after = update(&state, Event::NextStop).0;
        assert_eq!(after.modal, Modal::None);
        let left = &after.buffers[&path];
        assert_eq!((left.line, left.column), (2, 2));
        assert_eq!(left.shown(), "one\ntwo");
    }

    #[test]
    fn the_tools_selection_stops_at_both_ends() {
        let mut state = State {
            modal: Modal::Tools { row: 0 },
            os: "macos".to_string(),
            ..State::default()
        };
        for language in ["rust", "zig"] {
            state.servers.insert(
                language.to_string(),
                startup::Server {
                    command: language.to_string(),
                    args: Vec::new(),
                    also_served_by: Vec::new(),
                    extensions: Vec::new(),
                    install: BTreeMap::new(),
                    initialization_options: None,
                    partial: None,
                    unanswerable: None,
                },
            );
        }
        let up = update(&state, Event::MoveToolRow(Direction::Up)).0;
        assert_eq!(up.modal, Modal::Tools { row: 0 });
        let down = update(&state, Event::MoveToolRow(Direction::Down)).0;
        assert_eq!(down.modal, Modal::Tools { row: 1 });
        let last = tools::rows(&state).len() - 1;
        state.modal = Modal::Tools { row: last };
        let bottom = update(&state, Event::MoveToolRow(Direction::Down)).0;
        assert_eq!(bottom.modal, Modal::Tools { row: last });
    }

    #[test]
    fn the_launch_and_branch_selections_stop_at_both_ends() {
        let mut state = State {
            modal: Modal::Launches { row: 0 },
            ..State::default()
        };
        for name in ["app", "tests"] {
            state.launches.insert(
                name.to_string(),
                startup::Launch {
                    adapter: "rust".to_string(),
                    request: "launch".to_string(),
                    args: serde_json::Map::new(),
                    reattach: false,
                },
            );
        }
        let up = update(&state, Event::MoveLaunchRow(Direction::Up)).0;
        assert_eq!(up.modal, Modal::Launches { row: 0 });
        state.modal = Modal::Launches { row: 1 };
        let down = update(&state, Event::MoveLaunchRow(Direction::Down)).0;
        assert_eq!(down.modal, Modal::Launches { row: 1 });

        let branches = |row| Modal::Branches {
            refs: ["main", "feature"]
                .map(|name| story::BranchRef {
                    name: name.to_string(),
                    remote: false,
                    when: 0,
                })
                .to_vec(),
            filter: String::new(),
            row,
        };
        state.modal = branches(0);
        let up = update(&state, Event::MoveBranchRow(Direction::Up)).0;
        assert_eq!(up.modal, branches(0));
        state.modal = branches(1);
        let down = update(&state, Event::MoveBranchRow(Direction::Down)).0;
        assert_eq!(down.modal, branches(1));
    }

    #[test]
    fn a_re_check_asks_path_again_and_names_the_row_it_asked_about() {
        let mut state = State {
            modal: Modal::Tools { row: 0 },
            os: "macos".to_string(),
            ..State::default()
        };
        state.servers.insert(
            "zig".to_string(),
            startup::Server {
                command: "zls".to_string(),
                args: Vec::new(),
                also_served_by: Vec::new(),
                extensions: Vec::new(),
                install: BTreeMap::new(),
                initialization_options: None,
                partial: None,
                unanswerable: None,
            },
        );
        let (asked, effects) = update(&state, Event::RecheckTool);
        assert_eq!(effects, vec![Effect::ProbePath]);
        assert_eq!(
            asked.recheck,
            Some((tools::Kind::Server, "zig".to_string()))
        );
        assert_eq!(asked.modal, Modal::Tools { row: 0 });
        let (told, _) = update(&asked, Event::PathProbed);
        assert_eq!(told.modal, Modal::Restart);
        assert_eq!(told.recheck, None, "the question was answered once");
        let elsewhere = State {
            modal: Modal::None,
            ..asked.clone()
        };
        let (told, _) = update(&elsewhere, Event::PathProbed);
        assert_eq!(told.modal, Modal::None);
        assert_eq!(told.recheck, None);
    }

    #[test]
    fn an_unreadable_global_config_is_refused_and_nothing_runs() {
        let state = State {
            os: "linux".to_string(),
            ..State::default()
        };
        let (refused, effects) = update(
            &state,
            Event::GlobalConfigRead {
                kind: tools::Kind::Server,
                name: "go".to_string(),
                write: tools::Write::Row,
                text: None,
            },
        );
        assert_eq!(effects, vec![]);
        assert!(matches!(
            refused.refusal,
            Some(preview::Refusal::BrokenConfig(startup::ConfigError {
                fault: startup::ConfigFault::Unreadable,
                ..
            }))
        ));
    }

    #[test]
    fn an_unreadable_global_config_configures_nothing() {
        let state = State::default();
        let (refused, effects) = update(
            &state,
            Event::GlobalConfigRead {
                kind: tools::Kind::Speech,
                name: "synthesizer".to_string(),
                write: tools::Write::Configures,
                text: None,
            },
        );
        assert_eq!(effects, vec![]);
        assert_eq!(refused.speech.voice, "");
        assert!(matches!(
            refused.refusal,
            Some(preview::Refusal::BrokenConfig(_))
        ));
    }

    #[test]
    fn a_configured_voice_speaks_now_unless_one_beats_it() {
        let mut state = State::default();
        let read = Event::GlobalConfigRead {
            kind: tools::Kind::Speech,
            name: "synthesizer".to_string(),
            write: tools::Write::Configures,
            text: Some("[speech]\nvoice = \"\"\nconfigures.voice = \"~/v\"\n".to_string()),
        };
        let (configured, effects) = update(&state, read.clone());
        assert_eq!(configured.speech.voice, "~/v");
        assert!(matches!(effects.as_slice(), [Effect::WriteFile { .. }]));
        state.speech.voice = "/project/voice.onnx".to_string();
        let (beaten, _) = update(&state, read);
        assert_eq!(beaten.speech.voice, "/project/voice.onnx");
    }

    #[test]
    fn a_taken_row_is_configured_now_and_forgets_its_last_failure() {
        let mut state = State {
            os: "linux".to_string(),
            ..State::default()
        };
        let go = (tools::Kind::Server, "go".to_string());
        state.install_failed.insert(go.clone());
        let (taken, effects) = update(
            &state,
            Event::GlobalConfigRead {
                kind: go.0,
                name: go.1.clone(),
                write: tools::Write::Row,
                text: Some(String::new()),
            },
        );
        assert!(taken.servers.contains_key("go"));
        assert!(taken.install_failed.is_empty());
        assert_eq!(taken.installing, Some(go));
        assert!(matches!(
            effects.as_slice(),
            [Effect::WriteFile { .. }, Effect::RunInTerminal(_)]
        ));
    }

    #[test]
    fn an_available_speech_row_is_not_taken() {
        let mut state = State {
            os: "linux".to_string(),
            ..State::default()
        };
        let synthesizer = tools::rows(&state)
            .iter()
            .position(|row| row.kind == tools::Kind::Speech && row.name == "synthesizer")
            .expect("the template names a synthesizer");
        state.modal = Modal::Tools { row: synthesizer };
        let (_, effects) = update(&state, Event::InstallTool);
        assert_eq!(effects, vec![]);
    }

    #[test]
    fn an_install_that_worked_asks_path_again_about_its_row() {
        let state = State {
            installing: Some((tools::Kind::Server, "go".to_string())),
            ..State::default()
        };
        let (asked, effects) = update(&state, Event::InstallEnded(Some("0\n".to_string())));
        assert_eq!(effects, vec![Effect::ProbePath]);
        assert_eq!(asked.recheck, Some((tools::Kind::Server, "go".to_string())));
        assert_eq!(asked.installing, None);
        assert!(asked.install_failed.is_empty());
    }

    #[test]
    fn clearing_up_never_leaves_a_preview_naming_a_closed_buffer() {
        let root = PathBuf::from("/home/me/project");
        let path = root.join("src/one.js");
        let previewed = |draft: Option<&str>| {
            let (mut state, _) = update(
                &State {
                    root: root.clone(),
                    ..State::default()
                },
                Event::BufferOpened {
                    path: path.clone(),
                    contents: "one".to_string(),
                    preview: true,
                    at: None,
                },
            );
            assert_eq!(state.preview, Some(path.clone()));
            if let Some(draft) = draft {
                state.buffers.get_mut(&path).expect("the buffer").draft = Some(draft.to_string());
            }
            update(&state, Event::CloseAllBuffers { force: false }).0
        };
        let closed = previewed(None);
        assert!(closed.buffers.is_empty());
        assert_eq!(closed.preview, None);
        let kept = previewed(Some("my edits"));
        assert!(kept.buffers.contains_key(&path));
        assert_eq!(kept.preview, Some(path));
    }

    #[test]
    fn a_restart_refuses_on_unsaved_buffers_exactly_as_quitting_does() {
        let mut state = State {
            modal: Modal::Restart,
            root: PathBuf::from("/home/me/project"),
            ..State::default()
        };
        let path = state.root.join("src/lib.rs");
        let mut buffer = Buffer::open("fn main() {}", false, 4);
        buffer.draft = Some("fn main() { todo!() }".to_string());
        assert!(buffer.is_dirty());
        state.buffers.insert(path.clone(), buffer);
        state.current_buffer = Some(path);
        let (next, effects) = update(&state, Event::Restart);
        assert_eq!(effects, vec![Effect::Notify("unsaved-changes")]);
        assert!(
            !effects.contains(&Effect::Exit),
            "a restart is not a way out of unsaved edits"
        );
        assert_eq!(next.modal, Modal::None);
    }

    #[test]
    fn the_panes_start_action_takes_the_scope_the_view_shows() {
        let state = State {
            view: View::Review,
            risk_threshold: 20,
            test_command: Some("cargo test".to_string()),
            repo: Some(vec![review::GitFile {
                path: "src/keys.rs".to_string(),
                status: review::GitStatus::Modified,
            }]),
            risk: risk::Risk {
                figure: risk::Figure::Current(risk::Figures::default()),
                scope: risk::Scope::Workspace,
                before: Some(risk::Figures::default()),
                ..risk::Risk::default()
            },
            ..State::default()
        };
        let (started, _) = update(&state, Event::PaneAction(risk::START_LOOP));
        assert_eq!(
            started
                .refactor
                .running
                .map(|iteration| iteration.scope.as_str()),
            Some("review"),
            "the action started a loop over a Scope nobody was looking at"
        );
    }

    #[test]
    fn leaving_review_view_drops_the_reviewed_files_figure_and_measures_the_workspace() {
        let figures = risk::Figures {
            functions: vec![risk::Function {
                file: "src/keys.rs".to_string(),
                name: "route".to_string(),
                line: 88,
                metrics: risk::Metrics {
                    cyclomatic: 38,
                    ..risk::Metrics::default()
                },
            }],
            unparsed: 0,
        };
        let state = State {
            view: View::Review,
            risk_threshold: 20,
            risk: risk::Risk {
                figure: risk::Figure::Current(figures.clone()),
                before: Some(figures),
                ..risk::Risk::default()
            },
            ..State::default()
        };
        assert!(matches!(
            risk::shown(&state),
            Some(risk::Shown::Delta { .. })
        ));
        let (left, effects) = switch_view(&state, View::Edit);
        assert_eq!(left.risk.before, None);
        assert_eq!(
            risk::shown(&left),
            None,
            "the reviewed files were counted as the workspace"
        );
        assert!(
            effects.iter().any(|effect| matches!(
                effect,
                Effect::AnalyseRisk {
                    scope: risk::Scope::Workspace,
                    files: None,
                    base: None,
                    ..
                }
            )),
            "the workspace was never measured again: {effects:?}"
        );
    }

    fn diverged(root: &Path) -> State {
        let path = root.join("test.md");
        let mut buffer = Buffer::open("on disk", false, 4);
        buffer.key('x');
        buffer.follow("changed underneath".to_string());
        assert!(
            buffer.changed_on_disk,
            "the buffer is diverged to begin with"
        );
        State {
            root: root.to_path_buf(),
            current_buffer: Some(path.clone()),
            buffers: [(path, buffer)].into_iter().collect(),
            ..State::default()
        }
    }

    #[test]
    fn the_risk_rows_ask_is_armed_by_the_arrow_and_run_by_enter() {
        let mut state = State {
            root: PathBuf::from("/w"),
            focus: Pane::Risk,
            risk_threshold: 20,
            ai_running: true,
            ai_spoken: true,
            ..State::default()
        };
        let function = |name: &str, file: &str, line: usize, cyclomatic: u32| risk::Function {
            file: file.to_string(),
            name: name.to_string(),
            line,
            metrics: risk::Metrics {
                cyclomatic,
                ..risk::Metrics::default()
            },
        };
        state.risk.figure = risk::Figure::Current(risk::Figures {
            functions: vec![
                function("route", "src/keys.rs", 88, 31),
                function("draw", "src/ui.rs", 17, 22),
            ],
            unparsed: 0,
        });

        let (armed, _) = update(&state, Event::MoveAction(Direction::Right));
        assert_eq!(armed.selected_action, Some(0));
        let (_, effects) = update(&armed, Event::Activate);
        let sent = match effects.as_slice() {
            [Effect::SendKeys { pane, bytes }] if *pane == Pane::Ai => {
                String::from_utf8_lossy(bytes).into_owned()
            }
            other => panic!("expected one prompt to the AI pane, got {other:?}"),
        };
        assert!(sent.contains("route"), "{sent}");

        let (moved, _) = update(&armed, Event::MoveSelection(Direction::Down));
        assert_eq!(moved.selected_action, None);
        let (_, effects) = update(&moved, Event::Activate);
        assert_eq!(
            effects,
            vec![Effect::OpenAt {
                path: PathBuf::from("/w/src/ui.rs"),
                at: Place {
                    line: 17,
                    column: 1
                },
            }]
        );
    }

    #[test]
    fn the_risk_panes_own_actions_are_reachable_by_keyboard() {
        let state = State {
            root: PathBuf::from("/w"),
            focus: Pane::Risk,
            risk_threshold: 20,
            max_iterations: 3,
            test_command: Some("cargo test".to_string()),
            risk: risk::Risk {
                figure: risk::Figure::Current(risk::Figures {
                    functions: vec![risk::Function {
                        file: "src/keys.rs".to_string(),
                        name: "route".to_string(),
                        line: 88,
                        metrics: risk::Metrics {
                            cyclomatic: 31,
                            ..risk::Metrics::default()
                        },
                    }],
                    unparsed: 0,
                }),
                ..risk::Risk::default()
            },
            ..State::default()
        };
        let (recomputed, _) = update(&state, Event::Key('r'));
        assert!(recomputed.risk.in_flight(), "`r` measured nothing");

        let (running, _) = update(&state, Event::Key('l'));
        assert_eq!(
            risk::pane_actions(&running),
            vec![risk::RECOMPUTE, risk::STOP_LOOP]
        );
        let (stopped, effects) = update(&running, Event::Key('l'));
        assert_eq!(stopped.refactor.running, None);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::RestoreSnapshot { iteration: 1 })),
            "the stop left the Iteration half-applied: {effects:?}"
        );
    }

    #[test]
    fn resolving_a_divergence_withdraws_the_notice_about_it() {
        let root = PathBuf::from("/w");
        for resolution in [Resolution::Reload, Resolution::Overwrite] {
            let (next, effects) = update(&diverged(&root), Event::Resolve(resolution));
            assert!(
                !next.buffers.values().any(|buffer| buffer.changed_on_disk),
                "the divergence is settled"
            );
            assert!(
                effects.contains(&Effect::ClearNotice),
                "and the words about it are taken back"
            );
        }
    }

    #[test]
    fn writing_or_reloading_withdraws_it_without_the_picker() {
        let root = PathBuf::from("/w");
        for event in [Event::WriteBuffer, Event::ReloadBuffer] {
            let (_, effects) = update(&diverged(&root), event);
            assert!(effects.contains(&Effect::ClearNotice));
        }
    }

    #[test]
    fn what_is_open_is_watched_whether_or_not_its_folder_is_expanded() {
        let root = PathBuf::from("/w");
        let state = State {
            root: root.clone(),
            buffers: [(root.join("src/deep/tree.js"), Buffer::open("x", false, 4))]
                .into_iter()
                .collect(),
            diff_file: Some("other/landing.js".to_string()),
            varde_home: PathBuf::from("/home/me/.varde"),
            ..State::default()
        };
        let watched = watched_folders(&state);
        assert!(state.expanded.is_empty(), "nothing is expanded");
        assert!(
            watched.contains(&state.varde_home),
            "the global config's folder"
        );
        assert!(watched.contains(&root.join("src/deep")), "the open buffer");
        assert!(watched.contains(&root.join("other")), "the diff on screen");
        assert!(watched.contains(&root), "the root, always");
    }

    #[test]
    fn nothing_watched_is_a_file() {
        let root = PathBuf::from("/w");
        let state = State {
            root: root.clone(),
            buffers: [(root.join("src/tree.js"), Buffer::open("x", false, 4))]
                .into_iter()
                .collect(),
            ..State::default()
        };
        assert!(!watched_folders(&state).contains(&root.join("src/tree.js")));
    }

    #[test]
    fn toggling_the_spine_and_back_leaves_the_changed_files_scroll_where_it_was() {
        let repo = (0..20)
            .map(|n| review::GitFile {
                path: format!("file{n}.rs"),
                status: review::GitStatus::Modified,
            })
            .collect();
        let state = State {
            view: View::Story,
            story_listing: story::Listing::Files,
            repo: Some(repo),
            tree_selection: Some(PathBuf::from("file10.rs")),
            tree_scroll: 10,
            screen_width: 120,
            screen_height: 10,
            tree_divider: 30,
            ..State::default()
        };
        let spine = update(&state, Event::Key('t')).0;
        assert_eq!(spine.story_listing, story::Listing::Spine);
        let files_again = update(&spine, Event::Key('t')).0;
        assert_eq!(files_again.story_listing, story::Listing::Files);
        assert_eq!(files_again.tree_scroll, state.tree_scroll);
    }

    #[test]
    fn a_tick_turns_the_spinner_and_scrolls_nothing_back() {
        let scrolled = State {
            tree_scroll: 12,
            ..State::default()
        };
        let (after, effects) = update(&scrolled, Event::Tick);
        assert_eq!(after.tick, 1);
        assert_eq!(after.tree_scroll, 12, "the tick pulled the wheel back");
        assert!(effects.is_empty(), "a tick asked the edge for something");
        assert_eq!(
            State {
                tick: 0,
                ..after.clone()
            },
            scrolled,
            "a tick changed something other than the frame"
        );
    }

    #[test]
    fn wheeling_over_the_spine_scrolls_it_rather_than_the_tree_scroll() {
        let stories: Vec<String> = (1..=8)
            .map(|n| {
                format!(
                    r#"{{"id": "s{n}", "name": "Story{n}", "premise": "p",
                        "steps": [{{"id": "s{n}e1", "name": "step", "claim": "c", "why": "w",
                            "site": {{"file": "keys.rs", "side": "new", "kind": "changed",
                                     "from": {n}, "to": {n}, "text": "X"}}}}]}}"#
                )
            })
            .collect();
        let artifact = story::parse(&format!(
            r#"{{
                "protocolVersion": 2,
                "title": "Title",
                "range": {{"base": "a", "head": "b", "spelling": "main..HEAD"}},
                "stories": [{}]
            }}"#,
            stories.join(",")
        ))
        .expect("parses");
        let state = State {
            view: View::Story,
            story_listing: story::Listing::Spine,
            story_set: story::Set::Loaded(artifact),
            screen_width: 40,
            screen_height: 12,
            tree_divider: 30,
            ..State::default()
        };
        let scrolled = update(
            &state,
            Event::Scroll {
                pane: Pane::Tree,
                direction: Direction::Down,
                at: Place { line: 0, column: 0 },
            },
        )
        .0;
        assert_eq!(scrolled.spine_scroll, WHEEL_ROWS);
        assert_eq!(scrolled.tree_scroll, 0);
    }

    #[test]
    fn an_arriving_artifact_is_read_against_its_own_range() {
        let artifact = |head: &str| {
            format!(
                r#"{{
                "protocolVersion": 2,
                "title": "Title",
                "range": {{"base": "aaaa", "head": "{head}", "spelling": "main..HEAD"}},
                "stories": [{{
                    "id": "s1", "name": "Story", "premise": "p",
                    "steps": [{{
                        "id": "s1e1", "name": "step", "claim": "c", "why": "w",
                        "site": {{"file": "keys.rs", "side": "new", "kind": "changed",
                                 "from": 4, "to": 4}}
                    }}]
                }}]
            }}"#
            )
        };
        let asked = |head: &str| {
            update(
                &State::default(),
                Event::StoryArtifact {
                    contents: artifact(head),
                    range: story::RangeStatus::Resolves,
                },
            )
            .1
        };
        assert_eq!(
            asked("bbbb"),
            vec![Effect::ReadStoryFiles {
                repo: PathBuf::new(),
                base: "aaaa".to_string(),
                head: Some("bbbb".to_string()),
                files: vec!["keys.rs".to_string()],
            }]
        );
        assert_eq!(
            asked("worktree"),
            vec![Effect::ReadStoryFiles {
                repo: PathBuf::new(),
                base: "aaaa".to_string(),
                head: None,
                files: vec!["keys.rs".to_string()],
            }]
        );
    }

    #[test]
    fn moving_past_the_last_story_row_selects_and_enters_the_remainder() {
        let artifact = story::parse(
            r#"{
                "protocolVersion": 2,
                "title": "Title",
                "range": {"base": "a", "head": "b", "spelling": "main..HEAD"},
                "stories": [{
                    "id": "s1", "name": "Story", "premise": "p",
                    "steps": [{
                        "id": "s1e1", "name": "step", "claim": "c", "why": "w",
                        "site": {"file": "keys.rs", "side": "new", "kind": "changed",
                                 "from": 4, "to": 4, "text": "X"}
                    }]
                }]
            }"#,
        )
        .expect("parses");
        let state = State {
            view: View::Story,
            story_listing: story::Listing::Spine,
            story_set: story::Set::Loaded(artifact),
            file_hunks: vec![story::FileHunks {
                file: "mouse.rs".to_string(),
                hunks: story::hunks(b"a\nb\nc\n", b"a\nX\nc\n"),
                old_exists: true,
                old_text: "a\nb\nc\n".to_string(),
                head_exists: true,
                head_text: "a\nX\nc\n".to_string(),
                new_exists: true,
                new_text: "a\nX\nc\n".to_string(),
            }]
            .into(),
            ..State::default()
        };
        let moved = update(&state, Event::MoveSelection(Direction::Down)).0;
        assert_eq!(moved.story_selection, 1);
        let (entered, effects) = update(&moved, Event::Activate);
        assert!(matches!(
            entered.walking,
            Some(story::Walking::Remainder { index: 0 })
        ));
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::OpenAt { path, .. } if path.ends_with("mouse.rs"))
        ));
    }

    #[test]
    fn an_old_side_step_has_nothing_to_scroll() {
        let artifact = story::parse(
            r#"{
                "protocolVersion": 2,
                "title": "Title",
                "range": {"base": "aaaaaaaaaaaa", "head": "bbbbbbbbbbbb",
                          "spelling": "main..HEAD"},
                "stories": [{
                    "id": "s1", "name": "Story", "premise": "p",
                    "steps": [{
                        "id": "s1e1", "name": "step", "claim": "c", "why": "w",
                        "site": {"file": "keys.rs", "side": "old", "kind": "changed",
                                 "from": 300, "to": 300, "text": "b"}
                    }]
                }]
            }"#,
        )
        .expect("parses");
        let path = PathBuf::from("/w/keys.rs");
        let mut buffer = Buffer::open(&"line\n".repeat(400), false, 4);
        buffer.go_to_place(Place {
            line: 300,
            column: 1,
        });
        let mut buffers = BTreeMap::new();
        buffers.insert(path.clone(), buffer);
        let state = State {
            view: View::Story,
            story_set: story::Set::Loaded(artifact),
            walking: Some(story::Walking::Story {
                story: 0,
                step: 0,
                diff: story::Diff::Hidden,
            }),
            current_buffer: Some(path),
            buffers,
            editor_scroll: 120,
            screen_width: 100,
            screen_height: 40,
            ..State::default()
        };
        let resized = Event::Resized {
            width: 100,
            height: 40,
        };
        assert_eq!(update(&state, resized).0.editor_scroll, 0);
    }

    #[test]
    fn a_checkout_that_failed_is_a_notice_and_not_a_story() {
        let effects = update(
            &State::default(),
            Event::CheckoutFailed("1 file would be overwritten".to_string()),
        )
        .1;
        assert_eq!(
            effects,
            [Effect::notify_about(
                "checkout-failed",
                "1 file would be overwritten".to_string()
            )]
        );
    }

    #[test]
    fn the_hand_over_is_written_before_the_prompt_naming_it_goes_out() {
        let state = State {
            root: PathBuf::from("/w"),
            ai_running: true,
            ai_spoken: true,
            modal: Modal::ConfirmStory {
                spelling: "main..HEAD".to_string(),
                out: ".varde/stories/aaa-bbb.json".to_string(),
            },
            ..State::default()
        };
        let effects = update(&state, Event::ConfirmStory).1;
        let wrote = effects
            .iter()
            .position(|effect| matches!(effect, Effect::WriteStoryContext { .. }))
            .expect("the hand-over is written");
        let sent = effects
            .iter()
            .position(|effect| matches!(effect, Effect::SendKeys { pane: Pane::Ai, .. }))
            .expect("the prompt goes out");
        assert!(wrote < sent, "{effects:?}");
    }

    #[test]
    fn a_bare_t_in_story_view_still_types_while_inserting() {
        let path = PathBuf::from("/w/one.rs");
        let opened = update(
            &State::default(),
            Event::BufferOpened {
                path: path.clone(),
                contents: "one\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let state = State {
            view: View::Story,
            ..opened
        };
        let inserting = update(&state, Event::EditorKey('i')).0;
        let after = update(&inserting, Event::EditorKey('t')).0;
        assert_eq!(after.story_listing, state.story_listing);
        assert!(after.buffers[&path].shown().starts_with('t'));
    }

    #[test]
    fn a_jump_into_an_open_buffer_lands_on_the_file_it_read() {
        let path = PathBuf::from("/w/one.rs");
        let opened = |state: &State, contents: &str, at| {
            update(
                state,
                Event::BufferOpened {
                    path: path.clone(),
                    contents: contents.to_string(),
                    preview: false,
                    at,
                },
            )
            .0
        };
        let before = opened(&State::default(), "old\n", None);
        let after = opened(&before, "new\nold\n", Some(Place { line: 0, column: 0 }));
        assert_eq!(after.buffers[&path].shown(), "new\nold\n");
    }

    #[test]
    fn a_jump_into_a_drafted_buffer_flags_the_file_it_read() {
        let path = PathBuf::from("/w/one.rs");
        let opened = |state: &State, contents: &str| {
            update(
                state,
                Event::BufferOpened {
                    path: path.clone(),
                    contents: contents.to_string(),
                    preview: false,
                    at: Some(Place { line: 0, column: 0 }),
                },
            )
        };
        let drafted = update(
            &update(&opened(&State::default(), "old\n").0, Event::EditorKey('i')).0,
            Event::EditorKey('x'),
        )
        .0;
        let (same, quiet) = opened(&drafted, "old\n");
        assert!(!same.buffers[&path].changed_on_disk);
        assert!(!quiet.contains(&Effect::Notify("buffer-diverged")));
        let (moved, effects) = opened(&drafted, "new\n");
        assert_eq!(moved.buffers[&path].shown(), "xold\n");
        assert!(moved.buffers[&path].changed_on_disk);
        assert!(effects.contains(&Effect::Notify("buffer-diverged")));
    }

    #[test]
    fn a_read_only_surface_refuses_the_word_delete() {
        let path = PathBuf::from("/w/one.rs");
        let opened = update(
            &State::default(),
            Event::BufferOpened {
                path: path.clone(),
                contents: "one two three\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let inserting = update(
            &update(&opened, Event::EditorKey('i')).0,
            Event::EditorWord(Direction::Right),
        )
        .0;
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
            let after = update(&claimed, Event::EditorDeleteWord).0;
            assert_eq!(after.buffers[&path].shown(), "one two three\n");
        }
        let after = update(&inserting, Event::EditorDeleteWord).0;
        assert_eq!(after.buffers[&path].shown(), "two three\n");
    }

    #[test]
    fn a_waiting_g_owns_its_second_key_over_a_selection() {
        let path = PathBuf::from("/w/one.rs");
        let opened = update(
            &State::default(),
            Event::BufferOpened {
                path: path.clone(),
                contents: "let one = helper();\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let state = State {
            selection: Some(Selection::Buffer {
                anchor: Place { line: 1, column: 5 },
                cursor: Place { line: 1, column: 7 },
            }),
            ..opened
        };
        let after = update(
            &update(&state, Event::EditorKey('g')).0,
            Event::EditorKey('d'),
        );
        assert_eq!(after.0.buffers[&path].shown(), "let one = helper();\n");
        assert_eq!(after.1, vec![Effect::Notify("no-language-server")]);
        assert_eq!(
            update(&state, Event::EditorKey('d')).0.buffers[&path].shown(),
            "let  = helper();\n"
        );
    }

    #[test]
    fn a_pending_g_chord_still_steps_buffers_in_story_view() {
        let one = PathBuf::from("/w/one.rs");
        let two = PathBuf::from("/w/two.rs");
        let with_two = update(
            &update(
                &State::default(),
                Event::BufferOpened {
                    path: one,
                    contents: "one\n".to_string(),
                    preview: false,
                    at: None,
                },
            )
            .0,
            Event::BufferOpened {
                path: two.clone(),
                contents: "two\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let state = State {
            view: View::Story,
            ..with_two
        };
        let pending = update(&state, Event::EditorKey('g')).0;
        let stepped = update(&pending, Event::EditorKey('t')).0;
        assert_eq!(stepped.story_listing, state.story_listing);
        assert_ne!(stepped.current_buffer, state.current_buffer);
    }

    #[test]
    fn an_injection_is_one_write_and_only_a_pressed_one_clears_and_submits() {
        assert_eq!(
            injection("one\ntwo", keys::Paste::Bracketed, Enter::Pressed),
            b"\x15\x1b[200~one\ntwo\x1b[201~\r".to_vec()
        );
        assert_eq!(
            injection("one\ntwo", keys::Paste::Bare, Enter::Pressed),
            b"\x15one\ntwo\r".to_vec()
        );
        assert_eq!(
            injection(
                "safe\x1b[201~rm -rf /",
                keys::Paste::Bracketed,
                Enter::Pressed
            ),
            b"\x15\x1b[200~saferm -rf /\x1b[201~\r".to_vec()
        );
        assert_eq!(
            injection("one\ntwo", keys::Paste::Bracketed, Enter::Withheld),
            b"\x1b[200~one\ntwo\x1b[201~".to_vec()
        );
    }

    #[test]
    fn a_pty_selections_span_belongs_to_the_pane_it_was_dragged_in() {
        let picked = Selection::Screen {
            pane: Pane::Ai,
            from: Place { line: 1, column: 5 },
            to: Place { line: 2, column: 8 },
            text: "ripgrep".to_string(),
        };
        assert_eq!(
            picked.screen_span(Pane::Ai),
            Some((Place { line: 1, column: 5 }, Place { line: 2, column: 8 }))
        );
        assert_eq!(picked.screen_span(Pane::Terminal), None);
        assert_eq!(
            Selection::Buffer {
                anchor: Place { line: 1, column: 1 },
                cursor: Place { line: 1, column: 2 },
            }
            .screen_span(Pane::Ai),
            None
        );
    }

    #[test]
    fn a_click_reaches_the_child_as_the_bytes_that_child_asked_for() {
        let at = Place { line: 5, column: 9 };
        let child = |encoding| State {
            ai_running: true,
            ai_mouse: encoding,
            ..State::default()
        };
        let click = |state: &State| update(state, Event::ClickThrough { pane: Pane::Ai, at }).1;
        assert_eq!(
            click(&child(mouse::Encoding::Sgr)),
            vec![Effect::SendKeys {
                pane: Pane::Ai,
                bytes: b"\x1b[<0;9;5M\x1b[<0;9;5m".to_vec(),
            }]
        );
        assert_eq!(
            click(&child(mouse::Encoding::Legacy)),
            vec![Effect::SendKeys {
                pane: Pane::Ai,
                bytes: vec![
                    0x1b,
                    b'[',
                    b'M',
                    32,
                    32 + 9,
                    32 + 5,
                    0x1b,
                    b'[',
                    b'M',
                    32 + 3,
                    32 + 9,
                    32 + 5,
                ],
            }]
        );
        assert!(click(&child(mouse::Encoding::None)).is_empty());
    }

    #[test]
    fn a_wheel_the_child_cannot_be_told_about_stays_ours() {
        let state = State {
            terminal_mouse: mouse::Encoding::Legacy,
            ..State::default()
        };
        let (_, effects) = update(
            &state,
            Event::Scroll {
                pane: Pane::Terminal,
                direction: Direction::Down,
                at: Place {
                    line: 1,
                    column: 300,
                },
            },
        );
        assert_eq!(
            effects,
            vec![Effect::Scrolled(Pane::Terminal, Direction::Down)]
        );
    }

    fn diff_of(count: usize) -> Vec<DiffLine> {
        (1..=count)
            .map(|number| DiffLine {
                new_line: Some(number),
                old_line: Some(number),
                removed: false,
                text: String::new(),
            })
            .collect()
    }

    #[test]
    fn a_backwards_span_is_ordered_before_it_is_read() {
        let backwards = Selection::Buffer {
            anchor: Place { line: 2, column: 3 },
            cursor: Place { line: 1, column: 5 },
        };
        assert_eq!(
            backwards.buffer_span(),
            Some((Place { line: 1, column: 5 }, Place { line: 2, column: 3 }))
        );
    }

    #[test]
    fn a_half_typed_yank_copies_nothing() {
        let state = update(
            &State::default(),
            Event::BufferOpened {
                path: PathBuf::from("/f.js"),
                contents: "one\ntwo\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let state = update(&state, Event::EditorKey('d')).0;
        let state = update(&state, Event::EditorKey('d')).0;
        assert!(update(&state, Event::EditorKey('y')).1.is_empty());
    }

    #[test]
    fn a_paste_behind_a_diff_asks_for_no_clipboard() {
        let opened = update(
            &State::default(),
            Event::BufferOpened {
                path: PathBuf::from("/f.js"),
                contents: "one\ntwo\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let reviewing = update(
            &opened,
            Event::ShowDiff {
                file: "src/tree.js".to_string(),
                lines: diff_of(2),
                revision: "abc123".to_string(),
            },
        )
        .0;
        assert!(update(&reviewing, Event::PasteFromClipboard).1.is_empty());
        assert_eq!(
            update(&opened, Event::PasteFromClipboard).1,
            vec![Effect::ReadClipboard]
        );
    }

    #[test]
    fn a_shorter_diff_pulls_the_cursor_back() {
        let show = |state: &State, count| {
            update(
                state,
                Event::ShowDiff {
                    file: "src/tree.js".to_string(),
                    lines: diff_of(count),
                    revision: "abc123".to_string(),
                },
            )
            .0
        };
        let mut state = show(&State::default(), 5);
        for _ in 0..4 {
            state = update(&state, Event::EditorKey('j')).0;
        }
        assert_eq!(state.diff_line, 5);

        let state = show(&state, 2);
        assert_eq!(state.diff_line, 2);
        assert_eq!(
            update(&state, Event::EditorKey('c')).0.modal,
            Modal::Comment
        );
    }

    #[test]
    fn the_mode_label_names_each_mode_and_walking_overrides_all_of_them() {
        for (walking, mode, expected) in [
            (true, editor::Mode::Normal, "read-only"),
            (true, editor::Mode::Insert, "read-only"),
            (true, editor::Mode::Visual, "read-only"),
            (false, editor::Mode::Normal, "normal"),
            (false, editor::Mode::Insert, "insert"),
            (false, editor::Mode::Visual, "visual"),
        ] {
            let state = State {
                walking: walking.then_some(story::Walking::Story {
                    story: 0,
                    step: 0,
                    diff: story::Diff::Hidden,
                }),
                ..State::default()
            };
            let mut buffer = editor::Buffer::open("", false, editor::DEFAULT_TAB_WIDTH);
            buffer.mode = mode;
            assert_eq!(mode_label(&state, &buffer), expected);
            let stepping = State {
                stepping: true,
                ..state
            };
            let expected = match walking {
                true => "read-only",
                false => "stepping",
            };
            assert_eq!(mode_label(&stepping, &buffer), expected);
        }
    }

    #[test]
    fn a_chord_leaves_stepping_mode_on_unless_it_ends_the_session() {
        let chord = |state: &State, key| {
            let waiting = State {
                modal: Modal::Chord,
                ..state.clone()
            };
            update(&waiting, Event::Key(key)).0
        };
        let paused = debug::paused(State::default());
        assert!(chord(&paused, 'n').stepping);
        assert!(chord(&paused, 'b').stepping);
        assert!(!chord(&paused, 'q').stepping);
        assert!(!chord(&State::default(), 'b').stepping);
    }

    #[test]
    fn an_edit_that_is_not_a_key_is_refused_on_a_guest_repos_file() {
        let path = PathBuf::from("/side/guest/src/lib.rs");
        let opened = update(
            &State {
                guest: Some(PathBuf::from("/side/guest")),
                ..State::default()
            },
            Event::BufferOpened {
                path: path.clone(),
                contents: "theirs\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let inserting = update(&opened, Event::EditorKey('i')).0;
        let pasted = update(&inserting, Event::EditorPaste("mine".to_string())).0;
        assert_eq!(pasted.buffers[&path].shown(), "theirs\n");
        assert_eq!(pasted.refusal, Some(preview::Refusal::GuestReadOnly));
    }

    #[test]
    fn the_mode_label_reads_read_only_on_a_guest_repos_file() {
        for (open, expected) in [
            ("/side/guest/src/lib.rs", "read-only"),
            ("/w/src/lib.rs", "normal"),
        ] {
            let state = State {
                guest: Some(PathBuf::from("/side/guest")),
                current_buffer: Some(PathBuf::from(open)),
                ..State::default()
            };
            let buffer = editor::Buffer::open("", false, editor::DEFAULT_TAB_WIDTH);
            assert_eq!(mode_label(&state, &buffer), expected);
        }
    }

    #[test]
    fn space_opens_the_chord_hint_only_over_source_in_plain_normal_mode() {
        let source = update(
            &State::default(),
            Event::BufferOpened {
                path: PathBuf::from("/w/a.rs"),
                contents: "one\ntwo\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let spaced = |state: &State| update(state, Event::EditorKey(' ')).0.modal;
        assert_eq!(spaced(&source), Modal::Chord);
        let operator = update(&source, Event::EditorKey('d')).0;
        assert_eq!(spaced(&operator), Modal::None);
        assert_eq!(spaced(&previewing_readme("# Title\n")), Modal::None);
    }

    #[test]
    fn a_breakpoint_moved_by_an_edit_is_remembered_where_it_went() {
        let source = update(
            &State::default(),
            Event::BufferOpened {
                path: PathBuf::from("/a.rs"),
                contents: "one\ntwo".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let set = update(&source, Event::ToggleBreakpoint(2)).0;
        let (moved, effects) = update(&set, Event::EditorKey('O'));
        assert_eq!(moved.breakpoints[0].line, 3);
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::SaveState(json) if json.contains("\"line\":3")
        )));
    }

    #[test]
    fn the_breakpoint_lists_keys_remove_one_and_clear_them_all() {
        let state = State {
            breakpoints: ["/w/b.rs", "/w/a.rs", "/w/c.rs"]
                .into_iter()
                .map(|file| debug::Breakpoint {
                    file: PathBuf::from(file),
                    line: 1,
                    text: String::new(),
                    stale: false,
                    properties: Default::default(),
                })
                .collect(),
            ..State::default()
        };
        let shown = update(&state, Event::ToggleBreakpointList).0;
        assert_eq!(shown.focus, Pane::Breakpoints);
        let last = update(&update(&shown, Event::Key('j')).0, Event::Key('j')).0;
        assert_eq!(
            debug::selected(&last).unwrap().file,
            PathBuf::from("/w/c.rs")
        );
        let (removed, effects) = update(&last, Event::Key('d'));
        assert!(matches!(effects[..], [Effect::SaveState(_)]));
        assert_eq!(
            debug::selected(&removed).unwrap().file,
            PathBuf::from("/w/b.rs")
        );
        let cleared = update(&removed, Event::Key('D')).0;
        assert!(cleared.breakpoints.is_empty());
        let clear_all = debug::transport(&cleared)
            .into_iter()
            .find(|chip| chip.action == debug::CLEAR_ALL)
            .expect("the clear-all Chip");
        assert_eq!(clear_all.tone, Tone::Dimmed);
        assert_eq!(update(&cleared, Event::Key('D')).1, vec![]);
    }

    #[test]
    fn the_breakpoint_lists_e_opens_the_box_on_its_row() {
        let state = State {
            breakpoints: ["/w/a.rs", "/w/b.rs"]
                .into_iter()
                .map(|file| debug::Breakpoint {
                    file: PathBuf::from(file),
                    line: 1,
                    text: String::new(),
                    stale: false,
                    properties: Default::default(),
                })
                .collect(),
            ..State::default()
        };
        let shown = update(&state, Event::ToggleBreakpointList).0;
        let open = update(&update(&shown, Event::Key('j')).0, Event::Key('e')).0;
        assert!(matches!(
            &open.modal,
            Modal::Breakpoint { file, .. } if file == Path::new("/w/b.rs")
        ));
        let written = update(&open, Event::BreakpointDraft("n > 1".to_string())).0;
        let (kept, effects) = update(&written, Event::ConfirmBreakpoint);
        assert_eq!(kept.breakpoints[1].properties.condition, "n > 1");
        assert!(matches!(
            &effects[..],
            [Effect::SaveState(json)] if json.contains("\"condition\":\"n > 1\"")
        ));
    }

    #[test]
    fn opening_the_box_on_a_bare_line_sets_a_breakpoint_there() {
        let source = update(
            &State::default(),
            Event::BufferOpened {
                path: PathBuf::from("/a.rs"),
                contents: "one\ntwo".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let (open, effects) = update(&source, Event::EditBreakpoint(2));
        assert_eq!(open.breakpoints[0].line, 2);
        assert!(matches!(open.modal, Modal::Breakpoint { line: 2, .. }));
        assert!(matches!(effects[..], [Effect::SaveState(_)]));
    }

    #[test]
    fn a_breakpoint_is_only_set_on_a_line_the_buffer_has() {
        let source = update(
            &State::default(),
            Event::BufferOpened {
                path: PathBuf::from("/w/a.rs"),
                contents: "one\ntwo".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let (past, effects) = update(&source, Event::ToggleBreakpoint(9));
        assert!(past.breakpoints.is_empty());
        assert!(effects.is_empty());
        let (set, _) = update(&source, Event::ToggleBreakpoint(2));
        assert_eq!(set.breakpoints[0].text, "two");
    }

    fn finding(query: &str) -> Find {
        Find {
            query: Buffer::text_box(query),
            origin: Place { line: 1, column: 1 },
            case: search::Case::Smart,
            keys: FindKeys::Away,
        }
    }

    fn previewing_readme(contents: &str) -> State {
        let state = update(
            &State::default(),
            Event::Resized {
                width: 60,
                height: 40,
            },
        )
        .0;
        update(
            &state,
            Event::BufferOpened {
                path: PathBuf::from("README.md"),
                contents: contents.to_string(),
                preview: false,
                at: None,
            },
        )
        .0
    }

    #[test]
    fn a_narrower_row_brings_a_slid_preview_home() {
        let fence = "x".repeat(60);
        let state = previewing_readme(&format!("```rust\n{fence}\n```\n\nshort\n"));
        assert!(
            preview_columns(&state) < 60,
            "the fence has to run off the pane"
        );
        let slid = update(&state, Event::EditorKey('$')).0;
        assert!(slid.editor_hscroll > 0, "the fence never slid");
        let after = update(&slid, Event::EditorKey('G')).0;
        assert_eq!(after.editor_hscroll, 0);
    }

    #[test]
    fn the_arrows_move_a_preview_by_a_row() {
        let state = previewing_readme("one two three four five six seven eight\n");
        let width = preview_columns(&state);
        assert!(preview_rows(&state).len() > 1, "{width} columns");
        let down = update(&state, Event::EditorArrow(Direction::Down)).0;
        assert_eq!(current_buffer(&down).expect("a buffer").row, 2);
        let up = update(&down, Event::EditorArrow(Direction::Up)).0;
        assert_eq!(current_buffer(&up).expect("a buffer").row, 1);
    }

    #[test]
    fn a_refusal_is_cleared_by_the_next_event() {
        let refused = update(&State::default(), Event::TogglePreview).0;
        assert_eq!(refused.refusal, Some(preview::Refusal::NoFileOpen));
        let after = update(&refused, Event::EditorKey('j')).0;
        assert_eq!(after.refusal, None);
    }

    #[test]
    fn clicking_a_preview_places_the_cursor_on_the_row() {
        let state = previewing_readme("# Setup\n\nInstall it.\n");
        let clicked = update(&state, Event::ClickText(Place { line: 3, column: 1 })).0;
        assert_eq!(current_buffer(&clicked).expect("a buffer").row, 3);
    }

    #[test]
    fn clicking_a_preview_past_the_last_row_clamps_to_it() {
        let state = previewing_readme("# Setup\n");
        let rows = preview_rows(&state).len();
        let clicked = update(
            &state,
            Event::ClickText(Place {
                line: 99,
                column: 1,
            }),
        )
        .0;
        assert_eq!(current_buffer(&clicked).expect("a buffer").row, rows);
    }

    #[test]
    fn dragging_a_preview_selects_the_text_as_drawn() {
        let state = previewing_readme("## Install\n");
        let dragged = update(
            &state,
            Event::DragText {
                from: Place { line: 1, column: 1 },
                to: Place { line: 1, column: 7 },
            },
        )
        .0;
        assert_eq!(dragged.selected_text().as_deref(), Some("Install"));
    }

    #[test]
    fn dragging_across_preview_rows_carries_the_wrap_newlines() {
        let state = previewing_readme("one two three four five six seven eight nine ten\n");
        let rows = preview_rows(&state).len();
        assert!(rows > 1, "the paragraph did not wrap");
        let dragged = update(
            &state,
            Event::DragText {
                from: Place { line: 1, column: 1 },
                to: Place {
                    line: rows,
                    column: 1,
                },
            },
        )
        .0;
        let selected = dragged.selected_text().expect("a selection");
        assert_eq!(selected.matches('\n').count(), rows - 1);
    }

    #[test]
    fn a_narrower_pane_pulls_the_row_cursor_back() {
        let state = previewing_readme("one two three four five six seven eight\n");
        let down = (0..8).fold(state, |state, _| update(&state, Event::EditorKey('j')).0);
        assert!(current_buffer(&down).expect("a buffer").row > 1);
        let wider = update(
            &down,
            Event::Resized {
                width: 200,
                height: 40,
            },
        )
        .0;
        let rows = preview_rows(&wider).len();
        assert_eq!(rows, 1, "{:?}", preview_rows(&wider));
        assert_eq!(current_buffer(&wider).expect("a buffer").row, rows);
    }

    #[test]
    fn the_row_cursor_stops_at_the_last_row() {
        let state = previewing_readme("# Setup\n");
        let after = (0..10).fold(state, |state, _| update(&state, Event::EditorKey('j')).0);
        assert_eq!(
            current_buffer(&after).expect("a buffer").row,
            preview_rows(&after).len()
        );
    }

    #[test]
    fn every_refused_key_leaves_a_preview_unchanged() {
        for key in ['a', 'o', 'O', 'I', 'x', 'r', 'd', 'D', 'p', 'P', 'u', 'V'] {
            let state = previewing_readme("# Setup\n");
            let after = update(&state, Event::EditorKey(key)).0;
            assert!(
                !current_buffer(&after).expect("a buffer").is_dirty(),
                "{key:?} edited the buffer"
            );
            assert_eq!(
                after.refusal,
                Some(preview::Refusal::ReadOnlyPreview),
                "{key:?} was not refused"
            );
        }
    }

    #[test]
    fn a_next_past_the_last_utterance_ends_the_reading() {
        let state = State {
            reading: Some(reading::Reading {
                utterances: reading::utterances("One. Two."),
                offsets: vec![0, 1_550],
                at_ms: 1_600,
                paused: false,
                file: None,
            }),
            ..State::default()
        };
        let (after, effects) = update(&state, Event::NextUtterance);
        assert_eq!(after.reading, None);
        assert_eq!(effects, vec![Effect::StopSpeaking]);
    }

    #[test]
    fn the_last_transport_action_taken_is_the_one_lit() {
        let state = State {
            reading: Some(reading::Reading {
                utterances: reading::utterances("One. Two."),
                offsets: vec![0, 1_550],
                at_ms: 0,
                paused: false,
                file: None,
            }),
            ..State::default()
        };
        let lit = |state: &State, event| update(state, event).0.transport_lit;
        assert_eq!(lit(&state, Event::PlayPause), Some(reading::PLAY_PAUSE));
        assert_eq!(lit(&state, Event::NextUtterance), Some(reading::NEXT));
        assert_eq!(
            lit(&state, Event::PreviousUtterance),
            Some(reading::PREVIOUS)
        );
        assert_eq!(lit(&state, Event::SetSpeed(1.5)), Some(reading::SPEED));
        let (stopped, _) = update(&state, Event::StopReading);
        assert_eq!(stopped.transport_lit, Some(reading::STOP));
        assert_eq!(
            update(&stopped, Event::Tick).0.transport_lit,
            Some(reading::STOP)
        );
        let (ended, _) = update(&state, Event::ReadingEnded);
        assert_eq!((ended.reading, ended.transport_lit), (None, None));
        assert_eq!(lit(&State::default(), Event::PlayPause), None);
        for dimmed in [
            Event::NextUtterance,
            Event::PreviousUtterance,
            Event::StopReading,
        ] {
            assert_eq!(lit(&State::default(), dimmed), None);
        }
    }

    #[test]
    fn a_position_report_scrolls_nothing_back() {
        let state = State {
            editor_scroll: 12,
            reading: Some(reading::Reading {
                utterances: reading::utterances("One. Two."),
                offsets: Vec::new(),
                at_ms: 0,
                paused: false,
                file: None,
            }),
            ..State::default()
        };
        let (after, effects) = update(
            &state,
            Event::Speaking {
                at_ms: 1_600,
                offsets: vec![0, 1_550],
            },
        );
        assert_eq!(after.editor_scroll, 12, "the report pulled the wheel back");
        assert!(effects.is_empty(), "a report asked the edge for something");
        let reading = after.reading.expect("the reading");
        assert_eq!((reading.at_ms, reading.at()), (1_600, 1));
    }

    #[test]
    fn a_pointer_move_scrolls_nothing_back_and_arms_the_window_once() {
        let at = Place { line: 3, column: 4 };
        let state = State {
            editor_scroll: 12,
            ..State::default()
        };
        let (moved, effects) = update(&state, Event::PointerMoved(Pointed::Text(at)));
        assert_eq!(moved.editor_scroll, 12, "the pointer pulled the wheel back");
        assert_eq!(moved.pointed_at, Pointed::Text(at));
        assert_eq!(effects, vec![Effect::DwellHover(lsp::DWELL_MS)]);
        let (rested, effects) = update(&moved, Event::PointerMoved(Pointed::Text(at)));
        assert!(effects.is_empty(), "the window was armed a second time");
        let (asked, effects) = update(&rested, Event::HoverDue);
        assert_eq!(asked.editor_scroll, 12, "the rest pulled the wheel back");
        assert!(effects.is_empty(), "there is no server to ask");
    }

    fn hovering(lines: usize) -> State {
        let path = PathBuf::from("/w/src/lib.rs");
        let mut state = State {
            screen_width: 120,
            screen_height: 26,
            current_buffer: Some(path.clone()),
            ..State::default()
        };
        state.buffers.insert(
            path.clone(),
            editor::Buffer::open(
                &(1..=40).map(|n| format!("line {n}\n")).collect::<String>(),
                false,
                4,
            ),
        );
        state.hover = Some(lsp::Hover {
            lines: vec![
                preview::Row {
                    kind: preview::RowKind::Paragraph,
                    line: 1,
                    pieces: Vec::new(),
                    refused: None,
                };
                lines
            ],
            from: 3,
            asked: lsp::Ask {
                path,
                place: Place { line: 2, column: 9 },
                revision: 0,
                about: lsp::About::Hover,
            },
            first: 0,
            focused: false,
            value: None,
        });
        state.pointed_at = Pointed::Hover;
        state
    }

    #[test]
    fn the_wheel_scrolls_a_hover_as_far_as_its_last_line() {
        let mut state = hovering(30);
        for _ in 0..20 {
            state = update(&state, Event::ScrollHover(Direction::Down)).0;
        }
        assert_eq!(state.hover.as_ref().map(|hover| hover.first), Some(14));
        let state = update(&state, Event::ScrollHover(Direction::Up)).0;
        assert_eq!(state.hover.as_ref().map(|hover| hover.first), Some(13));
        let fits = update(&hovering(3), Event::ScrollHover(Direction::Down)).0;
        assert_eq!(fits.hover.as_ref().map(|hover| hover.first), Some(0));
    }

    #[test]
    fn scrolling_a_hover_leaves_a_wheeled_editor_where_it_was() {
        let state = State {
            editor_scroll: 2,
            ..hovering(30)
        };
        let (scrolled, _) = update(&state, Event::ScrollHover(Direction::Down));
        assert_eq!(
            scrolled.editor_scroll, 2,
            "the editor was pulled back to the cursor"
        );
    }

    #[test]
    fn escape_in_a_hover_closes_the_box_and_nothing_else() {
        let mut state = State {
            view: View::Story,
            story_set: story::Set::Authoring {
                spelling: "HEAD".to_string(),
            },
            ..hovering(3)
        };
        if let Some(hover) = state.hover.as_mut() {
            hover.focused = true;
        }
        let (left, _) = update(&state, Event::Cancel);
        assert_eq!(left.hover, None);
        assert!(
            matches!(left.story_set, story::Set::Authoring { .. }),
            "the wait was dropped"
        );
    }

    #[test]
    fn insert_on_a_markdown_buffer_in_source_only_enters_insert() {
        let preview = previewing_readme("# Setup\n\nInstall it.\n");
        let source = update(&preview, Event::TogglePreview).0;
        let path = source.current_buffer.clone().expect("a buffer");
        let mut placed = source.clone();
        let buffer = placed.buffers.get_mut(&path).expect("a buffer");
        buffer.line = 3;
        buffer.column = 5;
        let after = update(&placed, Event::EditorKey('i')).0;
        let buffer = current_buffer(&after).expect("a buffer");
        assert_eq!(buffer.mode, editor::Mode::Insert);
        assert_eq!((buffer.line, buffer.column), (3, 5));
        assert_eq!(after.refusal, None);
    }

    #[test]
    fn insert_with_no_file_open_changes_nothing() {
        let (after, effects) = update(&State::default(), Event::EditorKey('i'));
        assert_eq!(after, State::default());
        assert!(effects.is_empty(), "{effects:?}");
    }

    #[test]
    fn reading_keys_are_not_refused_in_a_preview() {
        for key in ['h', 'l', 'w', 'b', '/', 'n', 'N'] {
            let state = previewing_readme("# Setup\n");
            let after = update(&state, Event::EditorKey(key)).0;
            assert_eq!(after.refusal, None, "{key:?} was refused");
        }
    }

    #[test]
    fn crossing_to_source_lands_on_the_rows_line_column_one() {
        let contents = [
            "## Install\n",
            "Prose in a paragraph.\n",
            "- one\n- two\n",
            "> quoted\n",
            "| a | b |\n|---|---|\n| 1 | 2 |\n",
            "one\n\n***\n\ntwo\n",
            "```rust\nfn main() {}\n```\n",
            "```mermaid\ngraph TD\n  A --> B\n```\n",
            "---\ntitle: Varde\n---\n\nProse.\n",
        ];
        for content in contents {
            let state = previewing_readme(content);
            let rows = preview_rows(&state);
            assert!(!rows.is_empty(), "{content:?} produced no rows");
            for (index, row) in rows.iter().enumerate() {
                let path = state.current_buffer.clone().unwrap();
                let mut on_row = state.clone();
                let buffer = on_row.buffers.get_mut(&path).unwrap();
                buffer.row = index + 1;
                buffer.column = 40;
                let source = update(&on_row, Event::TogglePreview).0;
                let buffer = current_buffer(&source).expect("a buffer");
                assert_eq!(
                    (buffer.line, buffer.column),
                    (row.line, 1),
                    "{content:?} row {index}"
                );
            }
        }
    }

    #[test]
    fn crossing_to_preview_lands_on_the_row_of_the_right_kind() {
        use preview::{Marker, RowKind};
        use pulldown_cmark::HeadingLevel;
        let cases = [
            ("## Install\n", 1, RowKind::Heading(HeadingLevel::H2)),
            ("Prose in a paragraph.\n", 1, RowKind::Paragraph),
            (
                "- one\n- two\n",
                1,
                RowKind::List(preview::ListItem {
                    depth: 1,
                    marker: Some(Marker::Bullet),
                }),
            ),
            ("> quoted\n", 1, RowKind::Quote(None)),
            ("| a | b |\n|---|---|\n| 1 | 2 |\n", 1, RowKind::Table),
            ("one\n\n***\n\ntwo\n", 3, RowKind::Rule),
            ("```rust\nfn main() {}\n```\n", 1, RowKind::Code),
            (
                "```mermaid\ngraph TD\n  A --> B\n```\n",
                1,
                RowKind::Diagram,
            ),
            ("---\ntitle: Varde\n---\n\nProse.\n", 1, RowKind::Metadata),
        ];
        for (content, line, expected_kind) in cases {
            let mut state = previewing_readme(content);
            let path = state.current_buffer.clone().unwrap();
            let buffer = state.buffers.get_mut(&path).unwrap();
            buffer.previewing = false;
            buffer.line = line;
            buffer.column = 1;
            let preview = update(&state, Event::TogglePreview).0;
            let rows = preview_rows(&preview);
            let row = current_buffer(&preview).expect("a buffer").row;
            assert_eq!(rows[row - 1].kind, expected_kind, "{content:?} line {line}");
        }
    }

    #[test]
    fn crossing_to_preview_skips_blank_source_lines_and_separator_rows() {
        let content = "# Setup\n\nInstall it.\n";
        fn on_source_line(content: &str, line: usize) -> State {
            let mut state = previewing_readme(content);
            let path = state.current_buffer.clone().unwrap();
            let buffer = state.buffers.get_mut(&path).unwrap();
            buffer.previewing = false;
            buffer.line = line;
            buffer.column = 1;
            state
        }
        let landed_on = |line: usize| {
            let preview = update(&on_source_line(content, line), Event::TogglePreview).0;
            let rows = preview_rows(&preview);
            rows[current_buffer(&preview).expect("a buffer").row - 1]
                .text()
                .to_string()
        };
        assert_eq!(landed_on(1), "Setup");
        assert_eq!(landed_on(2), "Install it.", "the blank line between blocks");
        assert_eq!(
            landed_on(3),
            "Install it.",
            "the separator sharing the paragraph's line"
        );
    }

    #[test]
    fn matches_searches_rendered_rows_while_previewing() {
        let mut state = previewing_readme("## Install\n");
        state.find = Some(finding("Install"));
        assert_eq!(matches(&state, ..), vec![Place { line: 1, column: 1 }]);

        state.find = Some(finding("##"));
        assert!(matches(&state, ..).is_empty(), "a consumed marker matched");
    }

    #[test]
    fn matches_reverts_to_source_lines_once_preview_is_left() {
        let mut state = previewing_readme("## Install\n");
        let path = state.current_buffer.clone().unwrap();
        state.buffers.get_mut(&path).unwrap().previewing = false;
        state.find = Some(finding("##"));
        assert_eq!(matches(&state, ..), vec![Place { line: 1, column: 1 }]);
    }

    #[test]
    fn nothing_is_replaced_while_previewing() {
        let mut state = previewing_readme("Install\n\nInstall\n");
        state.find = Some(finding("Install"));
        state.replace_with = Buffer::text_box("Setup");
        for replace in [Event::ReplaceMatch, Event::ReplaceAll] {
            let after = update(&state, replace).0;
            let buffer = current_buffer(&after).expect("a buffer");
            assert_eq!(buffer.shown(), "Install\n\nInstall\n");
        }
    }

    #[test]
    fn the_replace_boxs_fields_edit_the_query_and_the_replacement() {
        let mut state = State::default();
        let path = PathBuf::from("/w/a.rs");
        state.current_buffer = Some(path.clone());
        state
            .buffers
            .insert(path, Buffer::open("stat state\n", false, 4));
        let mut find = finding("stat");
        find.keys = FindKeys::Replace(ReplaceField::Find);
        state.find = Some(find);
        let typed = update(&state, Event::EditorKey('e')).0;
        assert_eq!(typed.find.as_ref().expect("on").query.shown(), "state");
        assert_eq!(matches(&typed, ..), vec![Place { line: 1, column: 6 }]);
        let with = update(
            &typed,
            Event::FindKeys(FindKeys::Replace(ReplaceField::With)),
        )
        .0;
        let typed = update(&with, Event::EditorKey('x')).0;
        assert_eq!(typed.replace_with.shown(), "x");
        assert_eq!(typed.find.as_ref().expect("on").query.shown(), "state");
    }

    #[test]
    fn slash_back_into_a_search_keeps_where_it_started() {
        let mut state = State::default();
        let path = PathBuf::from("/w/a.rs");
        state.current_buffer = Some(path.clone());
        state
            .buffers
            .insert(path, Buffer::open("a\nb\nc\n", false, 4));
        state.find = Some(finding("c"));
        let moved = update(&state, Event::StepMatch(Direction::Right)).0;
        let back = update(&moved, Event::OpenFind).0;
        let escaped = update(&back, Event::CloseFind).0;
        let buffer = current_buffer(&escaped).expect("a buffer");
        assert_eq!((buffer.line, buffer.column), (1, 1));
    }

    #[test]
    fn stepping_matches_in_a_preview_moves_the_row() {
        let mut state = previewing_readme("Install\n\nInstall\n");
        state.find = Some(finding("Install"));
        let path = state.current_buffer.clone().unwrap();
        state.buffers.get_mut(&path).unwrap().row = 1;
        let after = update(&state, Event::StepMatch(Direction::Right)).0;
        let buffer = current_buffer(&after).expect("a buffer");
        assert_eq!(
            buffer.row, 3,
            "the second paragraph's row, past the blank separator"
        );
    }

    #[test]
    fn a_search_query_is_edited_at_its_caret() {
        let searching = State {
            search: Some(Search {
                query: Buffer::text_box("pdate"),
                selected: 2,
                ..Search::default()
            }),
            ..State::default()
        };
        let (home, effects) = update(&searching, Event::QueryEnd(Direction::Left));
        assert!(effects.is_empty(), "moving the caret searched again");
        assert_eq!(home.search.as_ref().unwrap().selected, 2);
        let (typed, effects) = update(&home, Event::EditorKey('u'));
        assert!(
            matches!(effects.as_slice(), [Effect::RunSearch(request)] if request.query == "update")
        );
        let (end, _) = update(&typed, Event::QueryEnd(Direction::Right));
        let (bracket, _) = update(&end, Event::EditorKey('('));
        assert_eq!(
            bracket.search.as_ref().unwrap().query.shown(),
            "update(",
            "a query is not code, so a bracket is not paired"
        );
        let (closed, _) = update(&bracket, Event::EditorKey(')'));
        let (between, _) = update(&closed, Event::EditorArrow(Direction::Left));
        let (erased, _) = update(&between, Event::EditorBackspace);
        assert_eq!(
            erased.search.unwrap().query.shown(),
            "update)",
            "Backspace takes the one character before the caret"
        );
    }

    #[test]
    fn escape_restores_the_row_a_preview_search_started_from() {
        let mut state = previewing_readme("Install\n\nInstall\n");
        let path = state.current_buffer.clone().unwrap();
        state.buffers.get_mut(&path).unwrap().row = 2;
        let opened = update(&state, Event::OpenFind).0;
        let typed = "Install".chars().fold(opened, |typing, key| {
            update(&typing, Event::EditorKey(key)).0
        });
        assert_eq!(
            current_buffer(&typed).expect("a buffer").row,
            3,
            "typing moved the cursor to the match"
        );
        let escaped = update(&typed, Event::CloseFind).0;
        assert_eq!(
            current_buffer(&escaped).expect("a buffer").row,
            2,
            "Escape put the row back"
        );
    }

    #[test]
    fn toggling_preview_still_flips_the_buffer_with_a_diff_open() {
        let state = State {
            diff: Some(vec![DiffLine {
                new_line: Some(1),
                old_line: Some(1),
                removed: false,
                text: "line".to_string(),
            }]),
            ..previewing_readme("# Setup\n")
        };
        assert!(current_buffer(&state).expect("a buffer").previewing);
        let once = update(&state, Event::TogglePreview).0;
        assert!(!current_buffer(&once).expect("a buffer").previewing);
        let twice = update(&once, Event::TogglePreview).0;
        assert!(current_buffer(&twice).expect("a buffer").previewing);
    }

    #[test]
    fn a_cursor_past_every_lines_lands_on_the_last_row() {
        let mut state = previewing_readme("# Setup\n\nInstall it.\n");
        let path = state.current_buffer.clone().unwrap();
        let buffer = state.buffers.get_mut(&path).unwrap();
        buffer.previewing = false;
        buffer.line = 1_000;
        let preview = update(&state, Event::TogglePreview).0;
        let rows = preview_rows(&preview);
        assert_eq!(current_buffer(&preview).expect("a buffer").row, rows.len());
    }
    #[test]
    fn the_risk_list_is_reached_by_geometry_and_its_visibility_is_remembered() {
        let hidden = State::default();
        assert_eq!(
            update(&hidden, Event::MoveFocus(Direction::Down)).0.focus,
            Pane::Terminal,
            "the shell is still under the tree with no list in between"
        );

        let (shown, effects) = update(&hidden, Event::ToggleRiskList);
        assert_eq!(shown.corner, layout::Corner::Risk);
        assert_eq!(shown.focus, Pane::Risk, "opening a pane to use it");
        assert!(
            matches!(&effects[..], [Effect::SaveState(json)] if json.contains("\"corner\":\"Risk\"")),
            "the visibility is nobody's but this user's: {effects:?}"
        );

        let from_tree = State {
            focus: Pane::Tree,
            ..shown.clone()
        };
        let down = update(&from_tree, Event::MoveFocus(Direction::Down)).0;
        assert_eq!(down.focus, Pane::Risk);
        for (direction, landed) in [
            (Direction::Up, Pane::Tree),
            (Direction::Down, Pane::Terminal),
            (Direction::Right, Pane::Terminal),
        ] {
            assert_eq!(
                update(&shown, Event::MoveFocus(direction)).0.focus,
                landed,
                "{direction:?} out of the Risk list"
            );
        }

        let from_shell = State {
            focus: Pane::Terminal,
            ..shown.clone()
        };
        assert_eq!(
            update(&from_shell, Event::MoveFocus(Direction::Left))
                .0
                .focus,
            Pane::Risk
        );
        assert_eq!(
            update(
                &State {
                    focus: Pane::Terminal,
                    ..hidden.clone()
                },
                Event::MoveFocus(Direction::Left)
            )
            .0
            .focus,
            Pane::Terminal,
            "nothing to the shell's left with no list there"
        );

        let (off, effects) = update(&shown, Event::ToggleRiskList);
        assert_eq!(off.corner, layout::Corner::Hidden);
        assert_eq!(off.focus, Pane::Tree, "focus left on a pane that is gone");
        assert!(
            matches!(&effects[..], [Effect::SaveState(json)] if json.contains("\"corner\":\"Hidden\"")),
        );
    }

    #[test]
    fn the_palette_fits_the_screen_it_is_drawn_on() {
        for screen in [24u16, 26, 30, 40] {
            let rows = palette_rows(screen);
            assert!(
                rows.len() <= (screen - 2) as usize,
                "{} rows in a box that draws {} at {screen}",
                rows.len(),
                screen - 2
            );
            let offered: Vec<char> = rows.iter().filter_map(|(key, _)| *key).collect();
            for (_, entries) in PALETTE {
                for (key, entry) in entries {
                    assert!(offered.contains(key), "{entry} is gone at {screen} rows");
                }
            }
        }

        let roomy = palette_rows(40);
        assert!(roomy.iter().any(|(_, row)| row.is_empty()));
        assert_eq!(
            roomy.last().map(|(_, row)| row.as_str()),
            Some("   Esc  cancel")
        );
        assert!(palette_rows(28)
            .iter()
            .any(|(_, row)| row == "   Esc  cancel"));

        let tiny = palette_rows(14);
        assert_eq!(tiny.len(), 12);
        assert_eq!(tiny.last(), Some(&(None, "   …".to_string())));
    }

    #[test]
    fn a_pane_without_row_actions_never_arms_the_trees() {
        let mut state = State {
            root: PathBuf::from("/w"),
            tree_selection: Some(PathBuf::from("/w/a.rs")),
            screen_width: 100,
            screen_height: 30,
            ..State::default()
        };
        state.contents.insert(
            PathBuf::from("/w"),
            vec![tree::Entry {
                name: "a.rs".to_string(),
                is_dir: false,
            }],
        );
        for (pane, corner) in [
            (
                Pane::Diagnostics,
                layout::Corner::Diagnostics(lsp::Severity::Error),
            ),
            (Pane::Conflicts, layout::Corner::Conflicts),
            (Pane::Frames, layout::Corner::Hidden),
        ] {
            let focused = State {
                focus: pane,
                corner,
                ..state.clone()
            };
            let armed = update(&focused, Event::MoveAction(Direction::Right)).0;
            assert_eq!(armed.selected_action, None, "{pane:?}");
        }

        let tree = State {
            focus: Pane::Tree,
            ..state
        };
        let armed = update(&tree, Event::MoveAction(Direction::Right)).0;
        assert_eq!(armed.selected_action, Some(0));
        let clicked = update(&armed, Event::ClickPane(Pane::Breakpoints)).0;
        assert_eq!(clicked.selected_action, None);
    }

    #[test]
    fn moving_focus_lets_go_of_the_row_action_it_leaves_behind() {
        let mut state = State {
            root: PathBuf::from("/w"),
            corner: layout::Corner::Buffers,
            focus: Pane::Tree,
            tree_selection: Some(PathBuf::from("/w/a.rs")),
            screen_width: 100,
            screen_height: 30,
            ..State::default()
        };
        state.contents.insert(
            PathBuf::from("/w"),
            vec![tree::Entry {
                name: "a.rs".to_string(),
                is_dir: false,
            }],
        );
        state.buffers.insert(
            PathBuf::from("/w/b.rs"),
            editor::Buffer::open("one\ntwo", false, 4),
        );

        let armed = update(&state, Event::MoveAction(Direction::Right)).0;
        assert_eq!(armed.selected_action, Some(0), "the tree's delete icon");

        let corner = update(&armed, Event::MoveFocus(Direction::Down)).0;
        assert_eq!(corner.focus, Pane::Buffers);
        assert_eq!(corner.selected_action, None);
        let shown = update(&corner, Event::Activate).0;
        assert_eq!(shown.current_buffer, Some(PathBuf::from("/w/b.rs")));
        assert_eq!(shown.focus, Pane::Editor, "Enter is the deliberate go");

        let toggled = update(&armed, Event::ToggleCursorHistory).0;
        assert_eq!(toggled.focus, Pane::History);
        assert_eq!(toggled.selected_action, None);
    }

    fn worklist() -> State {
        let function = |file: &str, name: &str, line, cyclomatic| risk::Function {
            file: file.to_string(),
            name: name.to_string(),
            line,
            metrics: risk::Metrics {
                cyclomatic,
                ..risk::Metrics::default()
            },
        };
        State {
            root: PathBuf::from("/w"),
            corner: layout::Corner::Risk,
            focus: Pane::Risk,
            risk_threshold: 20,
            screen_width: 100,
            screen_height: 30,
            risk: risk::Risk {
                figure: risk::Figure::Current(risk::Figures {
                    functions: vec![
                        function("src/ui.rs", "draw", 17, 22),
                        function("src/keys.rs", "cheatsheet", 402, 4),
                        function("src/keys.rs", "route", 88, 31),
                    ],
                    unparsed: 0,
                }),
                ..risk::Risk::default()
            },
            ..State::default()
        }
    }

    #[test]
    fn a_risk_row_opens_the_function_behind_the_figure() {
        let state = worklist();
        assert_eq!(
            risk::selected(&state).map(|row| row.name.as_str()),
            Some("route")
        );
        let down = update(&state, Event::Key('j')).0;
        assert_eq!(
            risk::selected(&down).map(|row| row.name.as_str()),
            Some("draw")
        );
        let up = update(&down, Event::Key('k')).0;
        assert_eq!(up.risk_selection, 0, "and back up again");
        let past = update(&down, Event::MoveSelection(Direction::Down)).0;
        assert_eq!(past.risk_selection, 2);
        assert!(risk::on_actions(&past));
        assert_eq!(risk::selected(&past), None);
        let further = update(&past, Event::MoveSelection(Direction::Down)).0;
        assert_eq!(further.risk_selection, 2);

        let (opened, effects) = update(&down, Event::Activate);
        assert_eq!(
            effects,
            vec![Effect::OpenAt {
                path: PathBuf::from("/w/src/ui.rs"),
                at: Place {
                    line: 17,
                    column: 1
                },
            }]
        );
        assert_eq!(opened.focus, Pane::Editor);

        let clicked = update(&state, Event::ClickRiskRow(1));
        assert_eq!(clicked.1, effects);
        assert_eq!(clicked.0.risk_selection, 1);
        assert_eq!(clicked.0.focus, Pane::Risk);
        let mut stale = state.clone();
        risk::went_stale(&mut stale.risk);
        let (after, effects) = update(&stale, Event::ClickRiskRow(1));
        assert!(matches!(&effects[..], [Effect::OpenAt { .. }]));
        assert_eq!(risk::view_state(&after), "stale");
    }

    #[test]
    fn the_risk_panes_actions_are_reached_by_arrowing_past_the_last_row() {
        let state = worklist();
        let border = update(&update(&state, Event::Key('j')).0, Event::Key('j')).0;
        assert!(risk::on_actions(&border));
        assert_eq!(border.selected_action, Some(0));
        assert_eq!(
            selected_row_actions(&border),
            vec![risk::RECOMPUTE, risk::START_LOOP]
        );

        let (_, effects) = update(&border, Event::Activate);
        assert!(
            matches!(&effects[..], [Effect::AnalyseRisk { .. }]),
            "the recompute, not a row action: {effects:?}"
        );

        let loop_slot = update(&border, Event::MoveAction(Direction::Right)).0;
        assert_eq!(loop_slot.selected_action, Some(1));
        assert_eq!(
            update(&loop_slot, Event::MoveAction(Direction::Right))
                .0
                .selected_action,
            Some(1)
        );
        assert!(matches!(
            update(&loop_slot, Event::Activate).1.first(),
            Some(Effect::Notify(_))
        ));

        let back = update(&loop_slot, Event::MoveAction(Direction::Left)).0;
        assert_eq!(back.selected_action, Some(0));
        assert_eq!(
            update(&back, Event::MoveAction(Direction::Left))
                .0
                .selected_action,
            Some(0)
        );
        let out = update(&back, Event::Key('k')).0;
        assert_eq!(out.risk_selection, 1, "back onto the last row");
        assert!(!risk::on_actions(&out));
        assert_eq!(out.selected_action, None, "and the icons are let go of");
    }

    #[test]
    fn an_empty_risk_list_still_reaches_its_recompute() {
        let empty = State {
            risk_threshold: 500,
            ..worklist()
        };
        assert_eq!(risk::list(&empty).len(), 0);
        assert!(risk::on_actions(&empty));
        let armed = update(&empty, Event::MoveAction(Direction::Right)).0;
        assert_eq!(armed.selected_action, Some(0));
        assert!(matches!(
            &update(&armed, Event::Activate).1[..],
            [Effect::AnalyseRisk { .. }]
        ));
    }

    #[test]
    fn a_recompute_clears_the_finished_loops_verdict() {
        let mut state = worklist();
        state.refactor.stopped = Some(risk::TESTS_FAILED);
        state.refactor.tests = Some((false, "1 failed".to_string()));
        assert!(risk::status(&state).is_some());

        let (fresh, effects) = update(&state, Event::RecomputeRisk);
        assert!(
            matches!(&effects[..], [Effect::AnalyseRisk { .. }]),
            "{effects:?}"
        );
        assert_eq!(risk::status(&fresh), None, "the border stops saying it");
        assert_eq!(fresh.refactor.tests, None);

        let mut running = state.clone();
        running.refactor.running = Some(risk::Iteration {
            scope: risk::Scope::Workspace,
            number: 2,
            test_command: "cargo test".to_string(),
            wait: risk::Wait::Session,
            before: None,
        });
        running.refactor.stopped = None;
        let (mid, _) = update(&running, Event::RecomputeRisk);
        assert_eq!(mid.refactor.tests, running.refactor.tests);
    }

    #[test]
    fn a_risk_row_is_never_text_to_copy() {
        let moved = update(&worklist(), Event::Key('j')).0;
        assert_eq!(moved.selection, None);
        assert_eq!(moved.selected_text(), None);
        assert_eq!(update(&moved, Event::Copy).1, vec![]);
    }

    #[test]
    fn the_risk_panes_own_key_shows_every_function() {
        let all = update(&worklist(), Event::Key('a')).0;
        assert!(all.risk_all);
        assert_eq!(
            risk::list(&all).len(),
            3,
            "the two, plus the one under the bar"
        );
        assert!(!update(&all, Event::Key('a')).0.risk_all, "a pure toggle");
    }

    #[test]
    fn the_wheel_scrolls_the_risk_list_and_the_next_key_pulls_it_back() {
        let mut state = worklist();
        let function = |which: usize| risk::Function {
            file: format!("src/{which}.rs"),
            name: format!("f{which}"),
            line: 1,
            metrics: risk::Metrics {
                cyclomatic: 100 - which as u32,
                ..risk::Metrics::default()
            },
        };
        state.risk.figure = risk::Figure::Current(risk::Figures {
            functions: (0..40).map(function).collect(),
            unparsed: 0,
        });
        let scrolled = update(
            &state,
            Event::Scroll {
                pane: Pane::Risk,
                direction: Direction::Down,
                at: Place { line: 1, column: 1 },
            },
        )
        .0;
        assert_eq!(scrolled.risk_scroll, WHEEL_ROWS);
        assert_eq!(scrolled.risk_selection, 0, "the wheel moves no selection");

        let keyed = update(&scrolled, Event::Key('j')).0;
        assert_eq!(keyed.risk_selection, 1);
        assert!(
            keyed.risk_scroll <= keyed.risk_selection,
            "the selection is above the rows on screen: {}",
            keyed.risk_scroll
        );
    }

    #[test]
    fn the_results_box_follows_its_selection_and_the_wheel_takes_it_away() {
        let state = State {
            screen_width: 100,
            screen_height: 16,
            search: Some(Search {
                query: Buffer::text_box("update"),
                results: Results {
                    hits: "abcde"
                        .chars()
                        .map(|file| search::Hit {
                            file: format!("{file}.rs"),
                            line: 1,
                            column: 1,
                            text: "update".to_string(),
                        })
                        .collect(),
                    ..Default::default()
                },
                ..Search::default()
            }),
            ..State::default()
        };
        assert_eq!(search_rows(&state), 5);
        assert_eq!(
            search::rows(&state.search.as_ref().unwrap().results).len(),
            10
        );

        let mut moved = state.clone();
        for _ in 0..4 {
            moved = update(&moved, Event::MoveHit(Direction::Down)).0;
        }
        let last = moved.search.as_ref().unwrap();
        assert_eq!(last.selected, 4);
        assert_eq!(
            last.scroll, 5,
            "the last hit is on row nine of ten, and five rows show"
        );

        let wheel = Event::Scroll {
            pane: Pane::Editor,
            direction: Direction::Down,
            at: Place { line: 1, column: 1 },
        };
        let mut scrolled = state.clone();
        for _ in 0..10 {
            scrolled = update(&scrolled, wheel.clone()).0;
        }
        assert_eq!(scrolled.search.as_ref().unwrap().scroll, 5);
        assert_eq!(
            scrolled.search.as_ref().unwrap().selected,
            0,
            "the wheel moves no selection"
        );
        assert_eq!(
            scrolled.editor_scroll, 0,
            "the wheel moved the pane behind the box"
        );

        let keyed = update(&scrolled, Event::MoveHit(Direction::Up)).0;
        assert_eq!(
            keyed.search.as_ref().unwrap().scroll,
            0,
            "the first hit is on row one, under the heading the clamp keeps above it"
        );

        let path = std::path::PathBuf::from("/w/wide.rs");
        let mut behind = state.clone();
        behind.current_buffer = Some(path.clone());
        behind
            .buffers
            .insert(path, editor::Buffer::open(&"x".repeat(200), false, 4));
        let swiped = update(
            &behind,
            Event::Scroll {
                pane: Pane::Editor,
                direction: Direction::Right,
                at: Place { line: 1, column: 1 },
            },
        )
        .0;
        assert_eq!(swiped.editor_hscroll, 0);
        assert_eq!(swiped.search.as_ref().unwrap().scroll, 0);
    }
}
