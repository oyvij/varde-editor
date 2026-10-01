// Enforces the code budget in AGENTS.md: an unused wrapper, an abstraction nothing
// calls, or a leftover helper is a build failure, not a matter of opinion.
#![deny(dead_code, unused)]
// `State` is the state, and it travels by value everywhere: `update` returns
// one and every group in its chain hands one back with the event it declined.
// The lint reads that hand-back as an accident to box; boxing it would put an
// allocation on the path of every keystroke to buy nothing, since the same
// struct is already moved by value on the way out.
#![allow(clippy::result_large_err)]

//! Varde — Command · Review · Integrated · Modal · Editor
//!
//! One state struct, one `update`, and everything the outside world must do is
//! returned as an [`Effect`] rather than performed here. Nothing in this crate
//! touches the terminal, the pty, the filesystem or git.

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

/// The name of Varde's own folder in a workspace. Only for the sites that need
/// it spelled *relative* to the workspace — what a prompt tells a session to
/// write, which is read against the session's own working directory. A path on
/// disk comes from [`varde_dir`].
pub const VARDE_DIR: &str = ".varde";

/// Where everything Varde writes for a workspace goes. Nothing joins
/// [`VARDE_DIR`] onto a root itself: where Varde's files live is one decision
/// in one place rather than one per site, with an eighth site to miss
/// (`docs/adr/0016-a-bare-workspace-leaves-nothing-behind.md`).
///
/// A Bare workspace has a Sidecar and answers with it, which is the whole of
/// "Varde writes nothing into a folder it was not given": every site above is
/// already routed here, so there is no site left that can still write into the
/// project. The Sidecar is derived by the edge from the folder and the process
/// id, and read here — never built here, since neither part is the library's
/// to observe.
pub fn varde_dir(root: &Path, sidecar: Option<&Path>) -> PathBuf {
    match sidecar {
        Some(sidecar) => sidecar.to_path_buf(),
        None => root.join(VARDE_DIR),
    }
}

/// Where a Reading's stream is written, and the one thing Varde writes that is
/// not about a workspace at all. Not the project's `.varde/`, which is drawn
/// in the file tree and walked by `git status` — a file that exists for eleven
/// seconds and vanishes is a flicker with no explanation there, and one left
/// by a crash is a mystery inside somebody's repository. Not the OS temp
/// directory either, because a sweep over a folder shared with every other
/// program is a pattern match against somebody else's filenames: everything
/// under here is Varde's, so all of it can go
/// (`docs/adr/0014-scratch-audio-lives-outside-the-workspace.md`).
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
    /// The Risk list, beneath the tree. Its own variant rather than a mode of
    /// the tree's: a click in it means something entirely different, and every
    /// place a rectangle is chosen from a pane has to account for it.
    Risk,
    /// The Buffers list, in the same corner. A second variant rather than a
    /// mode of the Risk pane's for the reason the Risk pane is not a mode of
    /// the tree's: the two share a rectangle and share nothing else — no
    /// actions, no rows of the same shape, and Enter means something different
    /// in each.
    Buffers,
    /// The Cursor history list, in the same corner. A third variant for the
    /// reason there is a second: the three share a rectangle and share nothing
    /// else, and a click in this one names a place to go back to rather than a
    /// buffer or a Function.
    History,
    /// The Breakpoint list, in the same corner, for the same reason again: a
    /// row names a Breakpoint, which is a line to go to and a thing to remove.
    Breakpoints,
    /// The Frames of a Paused Debug session, in the same corner: a row names a
    /// call to inspect.
    Frames,
    /// The Variables of the chosen Frame, in the Strip's Debug group — the
    /// one place a pane takes the Strip's rectangle from the shells. Its own
    /// variant for the reason the corner's four are each their own: a click in
    /// it names a member to open, which is nothing a shell's grid holds.
    Variables,
    /// The debugged program's own terminal, beside the Variables in the Debug
    /// group. A hosted pane like the shells and the AI: its child owns the
    /// keyboard, and Varde is the terminal answering its queries.
    Output,
    /// The Diagnostic list, in the same corner: a row names a place a
    /// Language server said something about.
    Diagnostics,
    /// The Conflict list, in the same corner: a row names a Conflict still in
    /// an unmerged file.
    Conflicts,
    /// The Evaluator's Snippet, in the floating window over the editor. Its
    /// own variant rather than a mode of the editor's: the editor goes on
    /// showing its file behind it, so a click lands in one or the other and
    /// the keys reach whichever holds the caret.
    Evaluator,
    /// The Cheatsheet, in the AI pane's rectangle while it is showing. Varde's
    /// own pane: nothing it is given reaches the session running behind it.
    Cheatsheet,
}

/// One control in a Transport (`docs/adr/0022-every-action-has-a-chip.md`):
/// what a click on it routes, what pressing it does now, the glyph, the keys
/// that do the same, and how it is drawn. `action` and `name` part ways on a
/// Chip that says what pressing it does — play and pause are one control, so
/// one action, lit as one whichever it read when it was pressed.
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub action: &'static str,
    pub name: &'static str,
    pub glyph: String,
    pub keys: &'static str,
    pub hue: Hue,
    pub tone: Tone,
}

/// What a Chip's colour says, named for its meaning so `ui` maps each to one
/// of the theme's named colours and never to a fixed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hue {
    Go,
    Hold,
    Step,
    Halt,
    Plain,
}

/// Dimmed and lit come from state, never from a timer: ADR 0009 allows a Tick
/// only while work is in flight, and a lit Chip that faded would need one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Dimmed,
    Plain,
    Lit,
    /// Something arrived that the reader has not seen — the Program output
    /// printing while it is out of sight. Its own tone rather than Lit: lit is
    /// "this is what you just did", and a mark is "this happened without you",
    /// which is the opposite claim.
    Marked,
}

/// What one of the terminal strip's shells is doing, told by the edge: a
/// foreground job is running in it, or its prompt is waiting. A command Varde
/// pushes goes to a waiting one — typed at a running job it is the job's input.
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

/// A key that decides nothing on its own: pressed twice inside the double-tap
/// window it opens the palette. Neither takes a byte from a hosted pane's
/// child — a lone modifier produces no bytes in a pty, and the escape is
/// forwarded as well as counted. What the palette then claims is the keys after
/// it, like any modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tap {
    Ctrl,
    Escape,
}

/// How far one notch of the wheel moves a list.
///
/// One row, not three. A trackpad flick is hundreds of reports and `pump`
/// draws one frame for the batch, so the cost of the smaller step is nothing
/// and the gain is that the text slides instead of jumping — three rows a
/// notch is a page every four notches, which is how you lose the line you
/// were reading.
const WHEEL_ROWS: usize = 1;

/// How far one press of the sideways gesture moves a surface that has no
/// column cursor. A tab stop, so a slide is a level of indentation rather than
/// a number nobody can picture: small enough to keep your place on the line,
/// and a held key's auto-repeat carries it across a long one.
const SLIDE_COLUMNS: usize = 8;

/// What the palette offers, grouped the way it is drawn: the panes to go to,
/// the views to switch to, then what applies to the project and to Varde
/// itself. Grouping is what let the list grow past the point where a flat
/// thirteen rows read as a heap — and what shows that `Edit` and `Editor` are
/// different questions, one a view and one a pane.
///
/// Only what applies *everywhere* belongs here. Copy, Write and Submit were
/// listed and are not: each one asks something of the buffer or the review in
/// front of you, so they stay the editor's — `C-c`, `:w`, `:submit` — and the
/// key box beside them advertises each one in the views it answers in.
///
/// The last group carries no heading: quitting is not a category. An entry's
/// own indent is part of its label, which is how `Tall` reads as the AI pane's
/// shape rather than a fourth pane.
pub const PALETTE: [(&str, &[(char, &str)]); 5] = [
    (
        "Panes",
        &[
            ('o', "Editor"),
            // `g` is Varde's own buffer gesture: `gt`/`gT` step buffers, so
            // the palette's `g` opens the list of what they step through.
            ('g', "Buffers"),
            ('d', "Files"),
            ('t', "Terminal"),
            ('k', "Risk"),
            // `y` because it is the free letter in "history": `p` and `n` are
            // what the jump itself is spelled with, so a palette letter
            // repeating one of them would read as performing the jump rather
            // than opening the list of where it goes. Two words where every
            // other entry is one because the user named it that way, and
            // because "History" alone reads as shell history in a workspace
            // that hosts a shell.
            ('y', "Cursor history"),
            ('b', "Breakpoints"),
            // `i` because it is the one letter in the word still free.
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
            // The palette's second face rather than a pane or a view, which is
            // why it is the one entry that leaves a modal open instead of
            // closing one. `v` because every letter that reads as "servers" or
            // "language" is already spent, and it must be reachable with no
            // modifier (R31.11).
            ('v', "Tools"),
            // The palette's third face: the Launch configurations. `n` is the
            // free letter in "launch".
            ('n', "Launch"),
            ('c', "Collapse"),
        ],
    ),
    ("Help", &[('h', "Keys"), ('u', "Update")]),
    ("", &[('q', "Quit")]),
];

/// The palette exactly as it is drawn, each row carrying the key it offers or
/// nothing at all — a heading, a gap, the cancel line. It lives here rather
/// than in `ui` because the mouse hit-tests these same rows: the two used to
/// format the list separately, which is one heading away from a click landing
/// on the row above.
///
/// `screen` is the screen's own height, which both callers must pass the same
/// of, because it is what decides how many of these rows exist.
/// `layout::overlay` caps the *box* at the screen and `ui` renders it with no
/// scroll offset, so a list longer than the box can draw lost its tail without
/// a word: two entries in the `Panes` group were enough to take `(q) Quit` off
/// a 26-row screen — the height the replay recipe uses — where it was then
/// neither drawn nor clickable. Bounded here rather than at either edge for the
/// reason [`lsp::TALLEST`] is: the box is sized from this count, so the count
/// has to be one the box can draw, and the renderer and the hit-test must be
/// handed the same vector or a click lands on a row nobody sees.
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

    // Cheapest row first, an entry last: what a short screen costs should be
    // what something else already teaches, and every row here stays reachable
    // from the keyboard whether it is drawn or not.
    //
    // The gaps between the groups carry nothing at all. The cancel line goes
    // next — Escape closes every box Varde has, so it is the one row a reader
    // can guess, which is the same argument that keeps it out of the
    // cheatsheet. The headings after it. Only then is an entry dropped, and
    // the last row says so: a command nobody can see is a command nobody uses,
    // so losing one is announced rather than clipped in silence.
    let budget = screen.saturating_sub(2) as usize;
    if rows.len() > budget {
        rows.retain(|(key, row)| key.is_some() || !row.is_empty());
    }
    if rows.len() > budget {
        rows.pop();
    }
    // Then the headings: the letters still say what each entry is, and a
    // heading is a label for rows rather than a thing to do.
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

/// The entry a palette key names, trimmed: an entry's indent is part of how it
/// is drawn, and nothing matching on it wants the spaces.
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
    /// Remembers which action asked for the name and the folder it was
    /// triggered on, which is what makes F4's tracking possible.
    NameBox {
        action: Action,
        dir: PathBuf,
    },
    Palette,
    /// A tapped Space waiting for its second key, with the Chord hint naming
    /// every key that can follow it. Gone at the second key or Escape.
    Chord,
    /// Picking the type and body for the selected diff lines.
    Comment,
    /// A box on the Variables row the keyboard is on, typed in the program's
    /// language rather than in Varde's: the row's new value, or a Watch being
    /// written from scratch. Two variants and not a flag, because Enter means
    /// two different things and a box that had to remember which is a box
    /// with two authors.
    SetValue,
    NewWatch,
    /// The one exception class to pause on, typed in the program's own
    /// spelling and never checked by Varde.
    ExceptionClass,
    /// Submitting clears whatever the AI's CLI is showing, which can be a
    /// half-written message. It cannot be read back, so it is announced.
    ConfirmSubmit,
    /// Authoring costs the reviewer minutes and clears the AI's prompt line
    /// exactly as submitting does, so the resolved range is always confirmed
    /// first — never guessed past.
    ConfirmStory {
        spelling: String,
        out: String,
    },
    /// The current Step's why, flow and Nudge, `D` away from the claim
    /// that is always on screen. Carries nothing of its own — every field
    /// it shows is read fresh from `state.walking` — so re-authoring or
    /// stepping to a new Step behind it never leaves it stale.
    StepDetail,
    /// The current Step's Prediction, asking the reviewer *why*. `picked` is
    /// the choice last picked, if any — everything else (its text, whether it
    /// is correct, its feedback) is read fresh from the Step's own
    /// Prediction, the same way `StepDetail` carries nothing of its own.
    Prediction {
        picked: Option<usize>,
    },
    /// What the language server says may follow the identifier being typed.
    /// A variant rather than a field beside the others for the reason
    /// [`lsp::Candidates`] gives, and the only modal that lets the keys it has
    /// no use for through to the buffer behind it — which is what keeps typing
    /// working while it is up.
    Candidates(lsp::Candidates),
    /// The palette's second face, Tools: every program configuration names
    /// and every template row it does not, and whether each command is on this
    /// machine. One variant rather than a bool beside `Palette`, for the reason
    /// [`Modal`] exists at all — two bools can both be true, and a list that is
    /// open and closed at once has no answer for the next keystroke.
    ///
    /// `row` is which row the install key acts on, carried here rather than
    /// beside the modal for the same reason: a selection that outlives the list
    /// is a selection in a list nobody can see.
    Tools {
        row: usize,
    },
    /// The Launch configurations both config layers name, and which row Enter
    /// starts — carried here for the reason [`Modal::Tools`] carries its row.
    Launches {
        row: usize,
    },
    /// A Run mark's offer of Run and Debug, on the line it stands on, holding
    /// what the mark captured: finding it is a parse, which a frame drawing
    /// the offer must not pay, and the offer answers every key so the buffer
    /// cannot change under it.
    RunMark {
        line: usize,
        mark: run::Mark,
    },
    /// The Breakpoint box: which Breakpoint, which row the keys type into,
    /// and what has been written so far — held here rather than on the
    /// Breakpoint until Enter, so Escape leaves it as it was.
    Breakpoint {
        file: PathBuf,
        line: usize,
        field: debug::Field,
        draft: debug::Properties,
    },
    /// The branch picker `:story?` opens: the repository's branches, and which
    /// row Enter acts on. One variant serves whatever repository was listed —
    /// the two flows differ only in that, so a second variant would be one
    /// list with two names. The rows are what [`story::branches`] makes of
    /// the refs and the filter together: the ordering, the dedup and the
    /// narrowing are that function's, and not this modal's to redo.
    ///
    /// `row` rides here rather than beside the modal for the reason
    /// [`Modal::Tools`]'s does: a selection that outlives the list is a
    /// selection in a list nobody can see.
    Branches {
        refs: Vec<story::BranchRef>,
        /// What has been typed into the picker. The refs stay whole and the
        /// filter narrows them on the way out, so clearing it restores the list
        /// without asking the edge to read the repository twice.
        filter: String,
        row: usize,
    },
    /// The one situation a restart answers, asked rather than taken: an
    /// installer that appended its directory to a shell profile is invisible to
    /// a process that inherited its environment at launch, so no probe this
    /// Varde runs will ever find it. Reached only by a re-check that still
    /// found nothing, which is what makes it distinguishable from the outside
    /// (R31.24). Carries nothing: what it offers is the same for every
    /// language, and the language it was asked about is already on screen
    /// behind it.
    Restart,
    /// The three ways out of a buffer that diverged from disk. Opened by `D`
    /// on a flagged buffer and never by the watcher itself: a modal that
    /// appeared on its own would take the keyboard mid-insert, and the pane
    /// beside it writes files all day.
    Diverged,
    /// What a snippet completion left to fill in: the tab stops it has not
    /// visited yet, and the file they are places in. Tab means "the next one"
    /// while this is up and means nothing otherwise, which is what makes it a
    /// modal — and it lets through everything it did not claim, as the
    /// candidate list does, because typing at a stop is ordinary typing.
    ///
    /// Here rather than on the [`editor::Buffer`]: the stops belong to the edit
    /// and not to the file, so a second buffer opened mid-sequence must not
    /// inherit them. `settle` ends the sequence the moment the reader is
    /// somewhere else, which is one rule rather than one per arm that could
    /// move them.
    ///
    /// Non-empty by construction: nothing left to visit is no sequence at all,
    /// so a snippet whose last stop is where the cursor already landed opens no
    /// modal.
    Stops {
        path: PathBuf,
        /// Each stop as the characters that follow it — see [`editor::Tail`],
        /// which is the reason filling one in does not move the next.
        at: Vec<editor::Tail>,
    },
}

/// What to do about a buffer and a file on disk that no longer agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Reload,
    Overwrite,
    Merge,
}

/// Which step of putting a Release in place of the running binary failed. One
/// notice each, because "the update failed" sends nobody anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceFailed {
    Download,
    /// The checksum list names no file for this platform.
    NoAsset,
    Checksum,
    Replace,
}

// No `Eq`: a speed is a multiplier, and a float has no total order.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Trigger(Action, Option<Target>),
    EnterName(String),
    /// Escape — dismisses whichever modal is open.
    Cancel,
    /// A tap, stamped by the caller. The edge supplies the real clock; tests
    /// supply whatever they need, so no timing test ever waits.
    Tapped {
        key: Tap,
        at_ms: u64,
    },
    /// Ctrl+Space, for terminals that cannot report a bare modifier press.
    FallbackBinding,
    Key(char),
    /// A palette row was clicked, named by the key it offers. The key rather
    /// than what it does: the row and the letter beside it are the same
    /// gesture, so a click goes through the arm the keystroke does and no
    /// second mapping can disagree with it — the click used to answer only the
    /// three views, and every other row it landed on did nothing.
    ClickPaletteEntry(char),
    /// Set or remove a Breakpoint on this line of the buffer on screen: a click
    /// in the gutter's Breakpoint column, or `␣b` on the cursor's line.
    ToggleBreakpoint(usize),
    /// Open the Breakpoint box for the Breakpoint on this line of the buffer
    /// on screen, setting one first if the line has none: `␣B`, or the `✎`
    /// Chip on the cursor's line.
    EditBreakpoint(usize),
    /// Offer Run and Debug for the Run mark on this line of the buffer on
    /// screen: a click on it, or `␣x` on its line.
    OfferRun(usize),
    /// Run or Debug, chosen on the offer: its key or its Chip.
    ChooseRun(&'static str),
    /// What the Breakpoint box's focused row reads now.
    BreakpointDraft(String),
    BreakpointField(debug::Field),
    /// The box's one switch. Applied as it is flipped rather than at Enter:
    /// a switch is not something written, so there is nothing to discard.
    SwitchSuspend,
    ConfirmBreakpoint,
    /// What a file holding remembered Breakpoints holds now, read by the edge
    /// because the core reads no files. Answers [`Effect::ReadBreakpointFile`].
    BreakpointFileRead {
        path: PathBuf,
        contents: String,
    },
    /// Files appeared on disk from somewhere that is not a tree action, each
    /// with what the edge saw it to be.
    FilesAppeared(Vec<(PathBuf, tree::Kind)>),
    FilesRemoved(Vec<PathBuf>),
    /// The edge read the folder because the user opened it.
    Expand {
        path: PathBuf,
        entries: Vec<Entry>,
    },
    /// `c` on the tree, and the palette's Collapse: every folder the tree has
    /// open closes at once, because a folder opened once stays open and a tree
    /// explored all day only ever grows.
    CollapseTree,
    FileChanged {
        path: PathBuf,
        contents: String,
    },
    Reload(PathBuf),
    /// The screen changed size. The core needs it to know how many rows of a
    /// list fit in a pane, which is what bounds every scroll offset.
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
    /// Files the comment being written, with the type the picker collected.
    /// The body is not carried: it lives in [`State::comment`] as a Buffer, so
    /// the event that files it names only what the edge holds.
    ///
    /// Its own gesture rather than Enter, which is a newline in the body — a
    /// comment cannot be unfiled, and the precedent for confirming one of those
    /// deliberately is `ConfirmSubmit` and `ConfirmStory`.
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
    /// `:help`, and the palette's Keys entry: shows the Cheatsheet in the AI
    /// pane's place or hides it. Never saved: every launch starts with it hidden.
    ToggleCheatsheet,
    ToggleField,
    /// `:minimap` — puts the mirror of the file up or takes it down, and gives
    /// its columns back to the text either way. Remembered, like the field.
    ToggleMinimap,
    /// A press or a drag in the minimap: the 0-based row of the strip the
    /// pointer is holding. The row rather than the line under it, because the
    /// mirror scrolls as the window travels — `minimap::travel` says why the
    /// line would run away from a pointer standing still. The caret comes
    /// along, the way a wheel over the editor already takes it.
    DragMinimap(u32),
    /// `:tall` — swaps the AI pane between beside the editor and down the
    /// whole right-hand edge. Remembered with the widths, since the shape you
    /// work in is as much a property of the project as they are.
    ToggleTallAi,
    /// `:split`: a second shell beside the one with the keyboard (F38).
    SplitTerminal,
    /// The keyboard to one of the terminal strip's shells, 0-based — what a
    /// click in a split says.
    FocusSplit(usize),
    /// A shell that had printed nothing has printed something, so it is
    /// reading its input — what a held command waits for (R38.5).
    ShellSpoke(usize),
    /// The debugged program printed something. Out of sight that marks the
    /// `Debug` Group tab and the Chip that shows it again; on screen it is
    /// already read, so it marks nothing.
    OutputSpoke,
    /// Hide the Program output, or show it again — `␣h` and the show Chip.
    ToggleOutput,
    ClickPane(Pane),
    /// A press and release in a pane with no drag between them: a click the
    /// pane's child may want, unlike the press, which is ours. `at` is the cell
    /// in the pane's own grid, because a child's report names its cell and knows
    /// nothing of the screen around it.
    ClickThrough {
        pane: Pane,
        at: Place,
    },
    /// The jump modifier's click on a pty: the row of the child's grid the
    /// pointer was on, as the edge read it, and the 1-based column it landed
    /// in. Whether that is a link is decided here, not where the grid is.
    ClickLink {
        row: String,
        column: usize,
    },
    ClickRow(PathBuf),
    /// A click on a row of the Risk list, by index into the list on screen. An
    /// index because a Function is not a path — and it goes straight through
    /// the arm Enter takes, so a click and a keystroke cannot drift apart.
    ClickRiskRow(usize),
    /// A click on a row of the Buffers pane, by index into the list on screen.
    /// Straight through the arm Enter takes, for the reason [`Event::ClickRiskRow`]
    /// is.
    ClickBufferRow(usize),
    /// A click on a row of the Cursor history pane, by index into the list on
    /// screen. Straight through the arm Enter takes, for the reason
    /// [`Event::ClickRiskRow`] is.
    ClickHistoryRow(usize),
    /// A click on a row of the Breakpoint list, by index into the list on
    /// screen. Straight through the arm Enter takes, for the reason
    /// [`Event::ClickRiskRow`] is.
    ClickBreakpointRow(usize),
    /// A click on a row of the Frames, the same shape again.
    ClickFrameRow(usize),
    /// A click on a row of the Diagnostic list — the Risk list's shape.
    ClickDiagnosticRow(usize),
    /// A click on a row of the Conflict list — the Risk list's shape.
    ClickConflictRow(usize),
    /// A button on a Conflict's bar: the side it accepts, for the Conflict the
    /// caret was put in by the same click.
    AcceptConflict(conflict::Side),
    /// A click on a row of the Variables, the same shape again.
    ClickVariablesRow(usize),
    /// A Group tab on the Strip's top border, clicked or reached by its chord:
    /// the Strip shows that group and nothing stops running in the other.
    ShowGroup(layout::Group),
    Scroll {
        pane: Pane,
        direction: Direction,
        /// Where the pointer is, in the pane's own grid. Only a hosted pane's
        /// child reads it; the tree and the editor scroll by rows.
        at: Place,
    },
    /// The Hover box moved a row, by the wheel over it or by `j`/`k` with the
    /// keyboard in it — never the editor underneath.
    ScrollHover(Direction),
    /// The Cheatsheet moved `rows` rows: a row by `j`/`k` or an arrow with
    /// the keyboard in it, a page by PgUp or PgDn. The wheel over it is a
    /// `Scroll` like any pane's.
    ScrollCheatsheet {
        direction: Direction,
        rows: usize,
    },
    RightClick(Pane),
    DragDivider(u32),
    /// The AI pane's left edge was dragged: how many columns wide it is now.
    /// A width rather than a column, because the tree's divider moves either
    /// side of it and the pane is meant to keep the size it was given.
    DragAiDivider(u32),
    /// The border between the Variables and the Program output, as a width for
    /// the Program output.
    DragOutput(u32),
    /// The border above the Strip was dragged: how many rows tall it asks to be.
    DragStrip(u32),
    Copy,
    /// Ctrl+V or Command+V: paste whatever the clipboard holds. The text is not
    /// here because only the edge can read it — the core answers with
    /// [`Effect::ReadClipboard`] and takes the text back as the
    /// [`Event::EditorPaste`] a paste already is.
    PasteFromClipboard,
    MoveFocus(Direction),
    MoveSelection(Direction),
    /// Right steps into the focused row's actions; Left steps back out.
    MoveAction(Direction),
    /// Enter on the selected row.
    Activate,
    /// Bytes for whichever pane has focus, as a terminal would send them.
    /// Bytes rather than text because a keypress is a byte sequence: an arrow
    /// key is an escape sequence, not a character.
    Bytes(Vec<u8>),
    /// Text pasted into whichever pane has focus, as one paste. A paste is not
    /// typing: it arrives whole, so a multi-line one can never be submitted a
    /// line at a time.
    Pasted(String),
    /// An action icon on the selected row, or its keyboard shortcut.
    RowAction(&'static str),
    /// A Chip on the Hover box, by what pressing it does. Its own event and
    /// not the Variables row's: the two act on different expressions, and one
    /// event reading the row under the keyboard would watch whatever the
    /// Variables happened to be on.
    HoverChip(&'static str),
    /// A row of the Hover's value section, opened or closed.
    OpenHoverRow(usize),
    /// `␣e`: the Evaluator opens on the expression the cursor is in or the
    /// Selection covers. The expression is the core's to read, so the chord
    /// carries nothing — where the Hover's Chip and a Variables row's each
    /// name one of their own.
    OpenEvaluator,
    /// Run the Snippet: normal-mode Enter, the Run Chip and Ctrl+Enter, which
    /// are one gesture and so one event.
    RunSnippet,
    /// A row of the Evaluator output's value, opened or closed.
    OpenEvaluatedRow(usize),
    /// Where the Evaluator's window goes, as a drag of its title bar, its
    /// border or one of its corners leaves it. Unclamped, the way a dragged
    /// divider's width is: where a window *may* be is `update`'s to answer,
    /// and the mouse only says where the pointer took it.
    PlaceEvaluator(layout::Area),
    /// How many rows the Snippet takes of the window, as the rule under it
    /// was dragged to. Unclamped for the same reason.
    SizeSnippet(u16),
    /// The keyboard's move and resize, one cell at a time: `␣m` and `␣z` open
    /// the mode and these are what its letters do. Two events rather than one
    /// carrying the mode, so what an arm does is written where it is read.
    MoveEvaluator(Direction),
    ResizeEvaluator(Direction),
    /// `␣m` and `␣z`, which put the keyboard in that mode.
    ArrangeEvaluator(debug::Arrange),
    /// Any other key, which leaves it — queued in front of the key's own
    /// event, the way leaving Stepping mode is.
    LeaveArranging,
    /// An action a *pane* offers, not a row: the Risk pane's recompute and its
    /// loop. Its own event because `RowAction` falls through to the tree's
    /// actions on a row, and a pane action stands on no row.
    PaneAction(&'static str),
    EditorKey(char),
    /// Text pasted into a buffer being typed into, as one edit. Its own event
    /// rather than the keystrokes a paste is everywhere else (see
    /// [`keys::on_paste`]): typed a character at a time the pairing rules
    /// would close every bracket in what was pasted, and the undo would take
    /// as many presses as the paste had characters.
    EditorPaste(String),
    /// Tab and Shift+Tab with characters picked: the lines the selection covers
    /// move one level in (Right) or out (Left). A direction rather than a flag
    /// because the editor's other two-way events already spell the two ways
    /// that way, and because Tab alone stays the indent it types at the cursor.
    EditorIndent(Direction),
    EditorArrow(Direction),
    /// Shift with an arrow: extends the selection instead of moving.
    EditorExtend(Direction),
    /// Shift with a word key, or shift and alt with an arrow: extends by word.
    EditorExtendWord(Direction),
    /// Alt with an arrow: a word motion, which moves in every mode. It is not
    /// the `w`/`b` keys under another name — those are text while inserting.
    EditorWord(Direction),
    /// `Ctrl+d` / `gm`: the next occurrence of what is picked joins it, so the
    /// keystroke after them lands at every one of them.
    EditorNextOccurrence,
    /// `gt` / `gT` through the open buffers.
    StepBuffer(Direction),
    ShowBuffer(PathBuf),
    Filter(String),
    OpenSearch,
    /// `*` — the word the cursor is in.
    SearchWordUnderCursor,
    /// `gr` — what the mouse selected, or the word under the cursor.
    SearchSelection,
    CloseSearch,
    /// `/` — an in-file search, on the editor's own command line.
    OpenFind,
    /// Home and End in a search query: Left is its start, Right its end. The
    /// rest of what a query answers is the editor's own events, since a query
    /// is a [`Buffer`]; these two are not, because the editor spends Home and
    /// End as `0` and `$`, and in a query those are letters.
    QueryEnd(Direction),
    /// Escape in the query — ends the search and goes back where it started.
    CloseFind,
    /// Enter in the query — leaves the match as the selection and hands the
    /// keyboard to the buffer. The search stays on.
    AcceptFind,
    /// `n` and `N` — the next and previous match of the search that is on.
    StepMatch(Direction),
    /// The keyboard moving within the search that is on, or out of it.
    FindKeys(FindKeys),
    /// `[Aa]` — flips whether the search that is on matches case exactly.
    ToggleCase,
    /// `[replace]` — the match at the cursor, or the next one after it, and
    /// on to the next.
    ReplaceMatch,
    /// `[replace all]` — every match in the buffer, as one undo step.
    ReplaceAll,
    /// The query changed; the edge scans and answers with `Searched`.
    SearchQuery(String),
    Searched(Results),
    MoveHit(Direction),
    /// `Ctrl+N` / `Ctrl+P` — the results file by file rather than hit by hit.
    MoveHitFile(Direction),
    /// A click on a result row, by the hit it holds — `mouse` resolves the
    /// screen position, so a heading or the box's chrome raises nothing at all.
    SelectHit(usize),
    OpenHit,
    OpenEveryHit,
    CompleteSearch,
    /// Puts the cursor on a place after a jump.
    JumpTo(Place),
    /// The edge walked the project and handed back its files.
    Indexed(Vec<String>),
    /// Enter in the filter box.
    AcceptFilter,
    /// The edge read a file: `preview` marks it as replaceable by the next one,
    /// and `at` is the place the cursor lands on — `Some` for the file
    /// `Effect::OpenAt` was asked to open *at* a line, `None` for a file that
    /// was merely opened.
    ///
    /// The place travels with the opening rather than following it as a second
    /// [`Event::JumpTo`], and that is load-bearing rather than tidy. A landing
    /// split across two events is a landing neither event knows the whole of:
    /// the first still sees the place being left and not where it is going, the
    /// second sees where it went and has already forgotten where it came from.
    /// The cursor history has to know both to answer "did this jump go
    /// anywhere" — a jump into the file already on screen moves nothing until
    /// the place arrives — so it cannot be answered by either half alone.
    BufferOpened {
        path: PathBuf,
        contents: String,
        preview: bool,
        at: Option<Place>,
    },
    EditorBackspace,
    /// Alt+Backspace while inserting. Its own event rather than a spelling of
    /// [`Event::EditorKey`] because `d` and `b` are text in insert mode, which
    /// is exactly the mode `db` cannot be typed in.
    EditorDeleteWord,
    /// `C-z` or `D-z`, in any buffer and either mode. Its own event rather
    /// than a spelling of [`Event::EditorKey`] for the reason
    /// [`Event::EditorDeleteWord`] is one: while inserting `u` is a letter.
    /// The editor's own `u` still goes through [`editor::Buffer::key`].
    EditorUndo,
    /// `C-S-z` or `D-S-z` — [`Event::EditorUndo`]'s other half, for its
    /// reason. `U` is the modifier-free spelling, through the buffer's keys.
    EditorRedo,
    EditorEscape,
    /// `:preview` — reads the open markdown file as the document it describes,
    /// or as the characters it holds. No key binding: the cheatsheet is the
    /// contract for what is bindable and a letter is too expensive to spend on
    /// a guess.
    TogglePreview,
    /// `:w`
    WriteBuffer,
    /// `:e`
    ReloadBuffer,
    /// One of the three answers the `Diverged` picker offers.
    Resolve(Resolution),
    /// No AI session exists any more — the child ended, or it never started.
    /// The pane goes back to asking, and whatever was queued for that session
    /// is dropped so it cannot fire into the next one.
    AiExited,
    /// The AI's child has printed something, so it is reading its stdin. A
    /// freshly spawned CLI drops what arrives before that.
    AiSpoke,
    StartAi {
        /// `None` keeps whatever is configured or remembered.
        command: Option<String>,
        force: bool,
    },
    /// `:q` and `:q!` — closes the open file, not Varde.
    CloseBuffer {
        force: bool,
    },
    /// `:qa` — closes every buffer with nothing unsaved in it. The one
    /// gesture in this group that cannot fail: a dirty buffer is one it skips,
    /// not a reason to refuse the rest. `:qa!` takes the dirty ones too, which
    /// is the only way it discards anything.
    CloseAllBuffers {
        force: bool,
    },
    /// Ctrl+Q, or the palette's `q`. Leaving Varde is not on the `:` line, so a
    /// mistyped clear-up cannot take the session with it.
    Quit,
    /// The same, abandoning unsaved work.
    QuitForce,
    /// `:update` — a release build of Varde's own checkout, or on a binary
    /// install the remembered Release put in place of the running binary.
    Rebuild,
    /// How putting the Release in place of the running binary ended. Success
    /// carries nothing: all it means is that a relaunch would land on it.
    BinaryReplaced(Result<(), ReplaceFailed>),
    /// What the latest-Release request came back with, unread: the body, or
    /// `None` when the request failed — which the edge has already logged, and
    /// which is shown nowhere (ADR 0017).
    ReleaseAnswered(Option<String>),
    /// Characters the edge read off a pty's grid — the one selection it has to
    /// finish itself, because there is no buffer behind a pty to anchor to.
    /// A drag over a pty pane: the span it covered and the text the edge read
    /// off that pane's grid.
    SelectIn {
        pane: Pane,
        from: Place,
        to: Place,
        text: String,
    },
    /// A click in the editor's text, at the place it landed on.
    ClickText(Place),
    /// The second click of a double-click, at the same place the first landed
    /// on — which picks the word there, as it does in an editor.
    DoubleClickText(Place),
    /// The pointer moved over the editor's text with the jump modifier held, or
    /// left it — the place a click would jump from, or nothing. Only the change
    /// is reported: `mouse` says nothing while the pointer stays on the place
    /// the state already names.
    HoverLink(Option<Place>),
    /// The pointer has come to rest on a clickable icon, or left the one it
    /// was on. Reported only when the answer changes, for the reason
    /// [`Event::HoverLink`] is: a pointer crossing a pane sends a report per
    /// cell, and each one reaching `update` would be a frame.
    HoverAction(Option<&'static str>),
    /// Whether the pointer is on the minimap, which is what lights its slider.
    /// Reported only when the answer changes, for the reason [`Event::HoverLink`]
    /// and [`Event::HoverAction`] are.
    ///
    /// One fact covers the gesture as well as the hover: a drag arrives as
    /// drag reports rather than moves, so the pointer that pressed the strip is
    /// still recorded as being on it for as long as it is held — and a wheel
    /// over the strip is a wheel under a pointer that is already there.
    HoverMinimap(bool),
    /// Where the name under the cursor is defined, asked for by the pointer
    /// rather than by `gd`. The cursor lands first, so this is the same
    /// question the key asks from the same place.
    AskDefinition,
    /// A drag inside the editor's text, over the span it covered. A buffer
    /// selection, not a pty one: nothing has to be read off a screen, so it
    /// survives scrolling and edits like a keyboard-extended one does.
    DragText {
        from: Place,
        to: Place,
    },
    DragRow(PathBuf),
    /// The edge loaded a diff for the file being reviewed.
    ShowDiff {
        file: String,
        lines: Vec<DiffLine>,
        /// The blob oid of the content shown — the working-tree file, or
        /// HEAD's blob when there is no working-tree file to hash.
        revision: String,
    },
    /// The edge read a story artifact off disk, and told git whether the
    /// range it is named for still resolves.
    StoryArtifact {
        contents: String,
        range: story::RangeStatus,
    },
    /// Every file an arriving artifact names, diffed and read — the answer to
    /// `Effect::ReadStoryFiles`, and the last thing between a parsed artifact
    /// and a walkable one. One channel rather than three, because the fill,
    /// the arrival checks and the stale check must all read the same texts:
    /// two readings of the same file can disagree, and the disagreement shows
    /// up as a Step that is stale the moment it arrives.
    StoryFiles(Vec<story::FileHunks>),
    /// `:story` and `:story!` — a range only git can resolve, so this asks
    /// rather than deciding.
    Story {
        /// `None` means work out what a bare `:story` means, offline.
        explicit: Option<String>,
        force: bool,
    },
    /// `:story?` — the branch picker. Which branches exist, and whether the
    /// folder is a repository with a clean tree at all, are git's to answer,
    /// so this asks rather than deciding.
    ///
    /// `Some(url)` is a repository Varde has never seen: a Guest repo, cloned
    /// into the Sidecar by the user's own `git` before its branches can be
    /// listed (ADR 0015). One event rather than two, because the picker is
    /// the same picker — the URL only says which repository it lists.
    PickBranch(Option<String>),
    /// The edge read the repository's refs, or found no repository, or found
    /// work nobody has committed.
    Branches(story::Branching),
    MoveBranchRow(Direction),
    /// What is typed in the branch picker, whole rather than a keystroke — the
    /// spelling [`Event::Filter`] uses, and for the same reason: the text is
    /// the state, so a core that assembled it from characters would have to
    /// answer for a backspace of its own.
    FilterBranches(String),
    /// Enter on a branch picker row: check that branch out and story what it
    /// introduced.
    ChooseBranch,
    /// The edge checked the picked branch out, and says which branch was left.
    /// Told rather than assumed: a checkout can fail, and a core that recorded
    /// it when it *asked* would name a branch the reviewer is not on — the
    /// defect `ai_running` was, in AGENTS.md's words. Where they are *now*
    /// arrives as [`State::branch`] on the next poll, so this carries only the
    /// half no read can answer.
    CheckedOut {
        left: String,
    },
    /// The checkout did not happen, with git's own reason. Said out loud rather
    /// than left as a picker that closed and did nothing: the reviewer is still
    /// on the branch they were on, and nothing else on screen would say so.
    CheckoutFailed(String),
    /// The edge walked the offline resolution ladder, or checked an explicit
    /// range, and is handing back the whole answer.
    StoryResolved(story::Resolution),
    /// The reviewer answered the `ConfirmStory` modal with "author".
    ConfirmStory,
    /// The watcher saw a new file land in `.varde/stories/` — told by name
    /// only, so the core can track retention without reading it. Loading
    /// what it holds is `StoryArtifact`'s, sent alongside this by the same
    /// watcher moment.
    StoryFileWritten(String),
    /// Chooses a Story from the spine, by its index in `story::spine`'s
    /// order — the reviewer's Enter on a spine row, or a test naming the
    /// Story directly.
    EnterStory(usize),
    /// `n`/`p` while walking a Story — steps to the next or previous Step,
    /// clamped so stepping past either end never leaves the Story.
    StepStory(Direction),
    /// Chooses the Remainder from the spine — the reviewer's Enter on its
    /// row, below the Stories.
    EnterRemainder,
    /// `n`/`p` while walking the Remainder — steps to the next or previous
    /// unclaimed hunk, clamped the same way `StepStory` clamps.
    StepRemainder(Direction),
    /// The analysis finished: the figures the edge derived, and the Scope they
    /// describe. Only the edge can run the analyser, so this is the core's only
    /// way to learn a figure — it never learns that a thread existed.
    RiskFigures {
        /// The generation of the request this answers. One below the latest is a
        /// superseded job's answer, and is dropped.
        generation: u64,
        figures: risk::Figures,
        /// The same files as the base revision had them, for a review-scoped
        /// answer: both sides are measured in one job, so the pair arrives
        /// together and the core never holds half a delta. `None` for the
        /// workspace, whose figure is a count with nothing to be a delta from —
        /// and it is what says which Scope the figure describes, so there is no
        /// second field to disagree with it.
        before: Option<risk::Figures>,
    },
    /// Starts the Refactor loop over a Scope: Varde hands a session the Scope
    /// and the goal, then judges the pass itself
    /// (`docs/adr/0010-varde-owns-the-test-gate.md`).
    StartRefactorLoop(risk::Scope),
    /// Stops the loop, putting the Iteration in flight back. Its own event
    /// rather than a second meaning for `Escape`: that key is heavily
    /// overloaded, and something midway through editing files must not be
    /// thrown away by a stray press.
    StopRefactorLoop,
    /// The tests the Gate runs finished: whether they passed, and what they
    /// printed. The output is carried because a bare "tests failed" is a
    /// report nobody can act on.
    TestsFinished {
        passed: bool,
        output: String,
    },
    /// An explicit ask for a fresh figure, whatever the current one's state:
    /// without it a Stale figure is a dead end until the commit moves.
    RecomputeRisk,
    /// The Risk list, on or off. A pure toggle: shown means hide, which is
    /// what makes one palette row enough for both.
    ToggleRiskList,
    /// The Buffers pane, on or off — into the same corner, so asking for it
    /// while the Risk list is there replaces it rather than needing a rule
    /// about which of the two wins.
    ToggleBuffersList,
    /// The Cursor history pane, on or off — the same corner again.
    ToggleCursorHistory,
    /// The Breakpoint list, on or off — the same corner again.
    ToggleBreakpointList,
    /// The Diagnostic list, on or off, opening on [`lsp::opening`].
    ToggleDiagnosticList,
    /// The Conflict list, on or off — the same corner again.
    ToggleConflictList,
    /// The Diagnostic list in the Corner showing one Severity: its letter, a
    /// click on its Severity label, or on its count on the tree's border.
    ShowDiagnostics(lsp::Severity),
    /// `Ctrl+p` / `gp` and `Ctrl+n` / `gn` — one step towards the oldest place
    /// the cursor has been, and one towards the newest.
    JumpBack,
    JumpForward,
    /// The whole list, or only the Functions above the threshold.
    ToggleRiskAll,
    /// One message a language server sent, exactly as it arrived. JSON rather
    /// than a parsed shape: what a message means is the core's to decide, and
    /// the edge that framed it decides nothing.
    LspReceived {
        language: String,
        json: String,
    },
    /// The edge holds a language server it did not hold before. Told rather
    /// than assumed, for the reason its going is: a spawn that was asked for is
    /// not a process that exists, and the conversation — the `initialize` and
    /// everything after it — begins against the process, not against the
    /// asking. The core learns *which* servers exist from `State::lsp_running`;
    /// this is what brings a pass around to read it.
    LspStarted {
        language: String,
    },
    /// What a file under review holds, read by the edge because the core reads
    /// no files. Answers [`Effect::ReadForReview`].
    ReviewFileRead {
        path: PathBuf,
        contents: String,
    },
    /// The edge stopped holding a language server. Pushed from every site that
    /// stops holding one, the way `AiExited` is: a derived field can say a
    /// server is gone, but not that the diagnostics it published and the
    /// requests it never answered went with it.
    LspGone {
        language: String,
        why: lsp::Gone,
    },
    /// One message the Debug adapter sent, exactly as it arrived — the
    /// language server's shape, for its reasons. `from` is the connection it
    /// came over: 0 for the one the session started, a child session's number
    /// (`Effect::DapChild`) otherwise.
    DapReceived {
        json: String,
        from: usize,
    },
    /// The edge holds the adapter a Debug session asked for, or a connection
    /// to it for a child session, so the conversation can begin against a
    /// process rather than against the asking.
    DapStarted {
        from: usize,
    },
    /// The edge stopped holding the adapter, or one child session's
    /// connection to it — pushed from every site that stops holding one,
    /// `AiExited`'s rule.
    DapGone {
        why: debug::Gone,
        from: usize,
    },
    /// The port a Waiting session watches accepted a connection, which only
    /// the edge can find out by trying.
    DapPortAnswers,
    /// Start the named Launch configuration.
    StartLaunch(String),
    /// The launch list's arrows.
    MoveLaunchRow(Direction),
    /// F9: continue while Paused, pause while Running.
    DebugResume,
    /// `\u{2423}a` and the Transport's ask-ai Chip: paste a Pause snapshot
    /// into the AI's prompt.
    AskAboutPause,
    /// F8, F7 and Shift+F8, and the chords that alias them: one step of the
    /// inspected thread.
    DebugStep(debug::Step),
    /// Ctrl+F2: stop the Debug session.
    DebugStop,
    /// Ctrl+F5 and the `r` chord: the last Launch configuration started
    /// again. Answered with no session, which is the point of it.
    DebugRestart,
    /// A key Stepping mode does not claim, so the mode ends here and the key
    /// does what it always does — the events queued behind this one. Its own
    /// event because only the router knows a key was not one of the mode's,
    /// and only `update` may write the flag.
    LeaveStepping,
    /// Typing paused for as long as the window `Effect::DebounceCandidates`
    /// asked for, so what may follow what was typed is worth asking about.
    /// Sent by the edge, which holds the
    /// timer and nothing else: how long to wait and what to ask are both
    /// decided here, and every keystroke resets it, so this arrives once for a
    /// typed word rather than once per letter.
    CandidatesDue,
    /// Where the pointer is now: in the buffer's text, on the Hover box, or
    /// anywhere else, which is over no symbol. A fact only the edge can
    /// observe, and one nobody pressed.
    ///
    /// Two things read it, and both are the reason it carries the place rather
    /// than an answer: the rest it may become is what the dwell window is
    /// armed off, and what is under it decides whether a diagnostic box is
    /// drawn — so a move that leaves the text is the event that takes one down.
    PointerMoved(Pointed),
    /// The pointer rested for as long as `Effect::DwellHover` asked for, so
    /// what is under it is worth asking the server about. The debounce's shape
    /// exactly: the edge holds the timer and the number came from here, and
    /// every cell the pointer crosses restarts it, so this arrives once for a
    /// rest rather than once per cell.
    HoverDue,
    /// The arrows, through the candidate list.
    MoveCandidate(Direction),
    /// Enter — the chosen candidate, in place of what was typed.
    AcceptCandidate,
    /// Tab, through a snippet's tab stops: the next place a value is needed.
    NextStop,
    /// The arrows, through Tools.
    MoveToolRow(Direction),
    /// `i` on a row of Tools: take it — write the row into the global config
    /// if the file lacks it, and run what installs it in the shell pane
    /// (`docs/adr/0018-the-global-config-is-the-list-of-programs.md`).
    InstallTool,
    /// The global config's text, read for the row being taken or for what its
    /// install configures, or `None` for a file that is there and could not
    /// be read. The row is carried out and back rather than kept: the list it
    /// was taken from is gone by now.
    GlobalConfigRead {
        kind: tools::Kind,
        name: String,
        write: tools::Write,
        text: Option<String>,
    },
    /// A config layer changed on disk while Varde runs: both layers as the
    /// edge read them after the change, merged here the way a start merges
    /// them.
    ConfigEdited {
        global: startup::OnDisk,
        project: startup::OnDisk,
    },
    /// What a taken row's install reported in [`tools::SENTINEL`], or `None`
    /// when the sentinel could not be read.
    InstallEnded(Option<String>),
    /// `r` on a row of Tools: ask `PATH` again about that row's
    /// command, because the user has just installed it.
    RecheckTool,
    /// A fresh `PATH` probe has landed — `State::commands_on_path` now
    /// describes the machine as it is. What it is for is the pass every event
    /// ends with: a command that has appeared is a reason to forget that it was
    /// missing (R31.24), and a re-check that still finds nothing is the one
    /// situation a restart answers.
    PathProbed,
    /// `:toggle` — fold the block the cursor is in away, or open it again.
    /// `all` is `:toggle!`, which answers for the whole file: every block at
    /// once, and every one of them open again on the next press.
    ToggleFold {
        all: bool,
    },
    /// `:format` — lay the file in the buffer out. Whoever answers it is
    /// decided in `update`: the Language server that says it formats, and
    /// otherwise the configured command.
    FormatBuffer,
    /// What running that command came to, which only the edge can observe: it
    /// held the child, or it did not. One event carrying [`format::Answer`]
    /// rather than three, because all three are the same fact arriving and the
    /// staleness rule below them is one rule.
    FormatterAnswered {
        language: String,
        path: PathBuf,
        revision: u64,
        answer: format::Answer,
    },
    /// `:read` — read the Selection aloud. A Reading covers the Selection and
    /// nothing else: there is no cursor-relative reading and no whole-file
    /// fallback, so a press with nothing picked refuses rather than guessing
    /// at a passage (R35.1).
    StartReading,
    /// `:stop` — end the Reading, and the stream with it.
    StopReading,
    /// The edge's player finished on its own or could not start, so the
    /// Reading is over. Not `StopReading`: nobody pressed stop, and a stop
    /// Chip lit for every Reading that ran to its end says somebody did.
    ReadingEnded,
    /// `:pause` — stop the sound where it is, or start it again from there.
    /// One event for both because it is one control: R35.8's Transport has a
    /// play/pause, not a play and a pause, and a reader who paused presses the
    /// same thing to go on.
    PlayPause,
    /// `:next` and `:prev` — one Utterance on, one back. They seek within the
    /// stream a Reading was already built into rather than synthesizing
    /// anything, which is what makes them instant and what keeps one player
    /// for the whole Reading (R35.6).
    NextUtterance,
    PreviousUtterance,
    /// A new pace, as the multiplier a human reads — never the synthesizer's
    /// backwards one, which is taken at the edge. It applies to the *next*
    /// Reading and leaves the one in flight alone: pace is baked in at
    /// synthesis, and re-pacing what is playing would repeat the words just
    /// heard (R35.7).
    SetSpeed(f32),
    /// Where the sound has got to, and where each Utterance starts in the
    /// stream the edge built. Both are facts only the edge can hold — a
    /// voice's pace is in the audio it produced, not in the text it was handed
    /// — and what the core makes of them is position, which is its own:
    /// R35.6's seek and R35.12's mark.
    Speaking {
        at_ms: u32,
        offsets: Vec<u32>,
    },
    /// The restart question answered yes. Declining it is `Cancel`, like every
    /// other modal.
    Restart,
    /// Time passed while the edge held work, so a spinner can turn. The edge
    /// sends it only while a job is in flight, which is what bounds the one
    /// exception to draw-only-when-something-changed by construction rather
    /// than by discipline. The core learns nothing about threads or timers: a
    /// tick is an event, batched with input like any other.
    Tick,
}

/// One line of a unified diff. `new_line` is the line number in the file as it
/// now is — what a comment anchors to, because that is the code to change.
/// The search modal's contents: what was asked and what came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    /// A [`Buffer`], so the caret can go back into it: a typo mid-query was
    /// otherwise fixed by deleting everything typed after it.
    pub query: Buffer,
    /// The folder the search is confined to, relative to the root. `None` is
    /// the whole project. A field on the search rather than an argument to the
    /// scan: the query is re-run on every keystroke, and a scope the caller has
    /// to remember to pass again is a scope that widens silently.
    pub scope: Option<PathBuf>,
    pub results: Results,
    /// Index into `results.hits`.
    pub selected: usize,
    /// The first [`search::Row`] the box shows. State rather than something the
    /// renderer works out, for the reason every other scroll in Varde is:
    /// clamped in one place, and read by whoever draws it.
    pub scroll: usize,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            query: Buffer::text_box(""),
            scope: None,
            results: Results::default(),
            selected: 0,
            scroll: 0,
        }
    }
}

/// An in-file search that is on: `/` began it and only Escape ends it, so its
/// line, count and highlights stay while the keyboard goes elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Find {
    /// What `/` is looking for. State rather than a draft the edge collects,
    /// because the cursor moves to the closest match on every keystroke.
    pub query: Buffer,
    /// Where the cursor was when `/` began the search, so Escape in the query
    /// costs nothing.
    pub origin: Place,
    pub case: search::Case,
    pub keys: FindKeys,
}

/// Where the keyboard is while a search is on. Core state rather than the
/// edge's, because what a key means depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindKeys {
    /// The buffer or another pane has it; the search is only showing.
    Away,
    Query,
    /// On one of the line's icons.
    Icon(FindIcon),
    /// In the replace box, which is open exactly as long as this is.
    Replace(ReplaceField),
}

/// The icons drawn after the count, in the order `Left` and `Right` walk them.
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

/// The replace box's stops, in the order Tab walks them.
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
    /// The line number this row had on the old side — what a removed row is
    /// looked up by when the old side is coloured, since its text is not in the
    /// new file at all. It is also what tells an added row from a context one:
    /// both are `removed: false` with a `new_line`, and only a context row has
    /// a line on both sides.
    pub old_line: Option<usize>,
    pub removed: bool,
    pub text: String,
}

// No `Eq`: a Reading's speed is a multiplier, and a float has no total
// equality to derive. Nothing compares effects as keys — they are compared for
// equality in tests and executed once — so `PartialEq` is the whole of what
// this type is asked for.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    SetTerminalInput(String),
    RunInTerminal(String),
    /// Start a shell beside split `from`, in the folder that shell is in.
    SplitTerminal {
        from: usize,
    },
    RenderView(View),
    /// Create the directory if it is not already there.
    EnsureDir(PathBuf),
    OpenBuffer(PathBuf),
    /// Read it, but mark it replaceable by the next preview.
    PreviewBuffer(PathBuf),
    WriteFile {
        path: PathBuf,
        contents: String,
    },
    DeleteFile(PathBuf),
    /// The directory and everything under it. Two producers, and the rule
    /// under both is one rule: the directory is Varde's own, so all of it can
    /// go. Quitting a Bare workspace removes its Sidecar, which is the whole
    /// of what "leaves nothing behind" means (ADR 0016), and starting sweeps
    /// [`tmp_dir`], which is what a crash mid-Reading escapes into (ADR 0014).
    /// Never a path in the workspace — a project's `.varde/` is the user's to
    /// keep.
    DeleteDir(PathBuf),
    SpawnAi {
        command: String,
    },
    Notify(&'static str),
    /// The same status line as [`Effect::Notify`], with something the message
    /// must say out loud beside it. A slug is a `&'static str` and cannot name
    /// a path, and a refusal that does not say *what* it refused is
    /// indistinguishable from a key that did nothing — which is the whole of
    /// what R31.13 asks for when a definition lands outside the workspace root.
    NotifyAbout {
        slug: &'static str,
        about: String,
    },
    /// Withdraws whatever the status line is currently saying. A notice about a
    /// fact outlives the fact otherwise: the line holds the last words it was
    /// given until something else replaces them, so settling a divergence left
    /// "changed on disk under your unsaved edits" standing over a buffer that
    /// now agrees with disk. Pushed from every arm that settles one — grep
    /// `ClearNotice` — for the same reason `AiExited` is pushed from every site
    /// that stops holding a pane: a withdrawal the arms have to remember is a
    /// withdrawal one of them will forget.
    ClearNotice,
    /// The edge reads the folder and replies with Expand — the tree stays lazy.
    ReadFolder(PathBuf),
    Scrolled(Pane, Direction),
    /// Hand this URL to the operating system's opener — the browser, for the
    /// `http` and `https` that [`editor::link_at`] is the only producer of.
    OpenUrl(String),
    SetClipboard(String),
    SaveState(String),
    /// Bytes for the pane's child, exactly as a terminal would send them —
    /// keystrokes, and the mouse reports a child that asked for them reads.
    SendKeys {
        pane: Pane,
        bytes: Vec<u8>,
    },
    /// Start the language server configuration names for this language, with
    /// the arguments it named. Whether one is running comes back as
    /// `State::lsp_running`, never remembered here.
    StartLsp {
        language: String,
        command: String,
        args: Vec<String>,
    },
    /// Start the Debug adapter a Launch configuration named. Whether it
    /// started comes back as `Event::DapStarted` or `Event::DapGone`.
    StartDap {
        command: String,
        args: Vec<String>,
        reach: debug::Reach,
    },
    /// One Debug Adapter Protocol message for the adapter, built here, over
    /// the connection `to` — `Event::DapReceived`'s numbering.
    DapSend {
        to: usize,
        json: String,
    },
    /// Open connection `child` to the adapter the session holds, the way that
    /// one was reached, for a child session it asked Varde to start. Whether
    /// it opened comes back as `Event::DapStarted` or `Event::DapGone` from
    /// `child`. `StopDap` lets every one go with the adapter.
    DapChild {
        child: usize,
    },
    /// Let one child session's connection go, the rest of the session going
    /// on. Answered with `Event::DapGone` from `child` where the edge held it.
    StopDapChild {
        child: usize,
    },
    /// Start the debugged program in the Debug group's own terminal, which is
    /// what the adapter's `runInTerminal` asks for. Never a shell: the Strip's
    /// shells are the reader's, and a program started in one would end with
    /// the next `:split` and print into whatever else was running there.
    /// Whether it started is the edge's to say, the way a spawned AI is.
    RunProgram {
        argv: Vec<String>,
        cwd: Option<PathBuf>,
        env: BTreeMap<String, String>,
    },
    /// Let the adapter go. The edge answers with `Event::DapGone`, as every
    /// site that stops holding one does.
    StopDap,
    /// Ask this process's `PATH` again, and say so when the answer has landed.
    /// The only producer is the Tools list's re-check: everywhere else the
    /// edge probes on its own, when the list opens. It answers with
    /// `Event::PathProbed` rather than with the set itself, so the set stays a
    /// field only the edge writes (R31.23) — the core reads what it is told and
    /// learns only that what it is reading is fresh.
    ProbePath,
    /// Ask this repository for its latest Release, answered with
    /// `Event::ReleaseAnswered`. Only a binary install asks: a checkout install
    /// reads its manifest and never touches the network (ADR 0017).
    CheckRelease {
        url: String,
    },
    /// One JSON-RPC message for that language's server. The library built it,
    /// for the reason `mouse::report` builds a mouse report: bytes decided in
    /// `main.rs` are bytes no test watches.
    LspSend {
        language: String,
        json: String,
    },
    /// Read a file that is under review but open in no Buffer, so its server
    /// can be told what it holds. The core reads no files, and this is not
    /// `OpenBuffer`: opening one would put a file nobody asked to see on
    /// screen. Answered with [`Event::ReviewFileRead`], or with nothing at all
    /// when the file cannot be read — which leaves it *not measured*, the same
    /// answer a file no server serves gets.
    ReadForReview(PathBuf),
    /// Read a file the project remembers Breakpoints in, so each can be held
    /// to the text its line had — at load, whether or not the file is open.
    /// Answered with [`Event::BreakpointFileRead`], empty for a file that is
    /// gone, which makes every Breakpoint in it Stale.
    ReadBreakpointFile(PathBuf),
    /// Wait this many milliseconds and then answer with
    /// [`Event::CandidatesDue`] — one timer, restarted every time this
    /// arrives, so a burst of typing asks once. The edge holds the clock and
    /// the number comes from here, which is what keeps a guessed delay out of
    /// `main.rs` and this behaviour inside a scenario.
    DebounceCandidates(u64),
    /// Wait this many milliseconds and then answer with [`Event::HoverDue`] —
    /// one timer, restarted by every cell the pointer crosses, so a pointer on
    /// its way somewhere else asks nothing. Beside the debounce because it is
    /// the same arrangement: the clock is the edge's and the window is the
    /// core's.
    DwellHover(u64),
    Exit,
    /// Leaving, onto the binary now on disk with the same arguments, in place
    /// of `Exit` (ADR 0017).
    Relaunch,
    /// Put this Release's Asset in place of the running binary, verified
    /// against the checksum list, answered with `Event::BinaryReplaced`.
    ReplaceBinary {
        asset: String,
        checksums: String,
    },
    StopAi,
    ReadDiff(PathBuf),
    RunSearch(String),
    /// Read the file and put the cursor on that place.
    OpenAt {
        path: PathBuf,
        at: Place,
    },
    /// Walk the project's files, skipping what git ignores.
    IndexProject,
    /// No system clipboard — ask the terminal to hold it (OSC 52), which is
    /// what makes copying work over SSH.
    ClipboardViaTerminal(String),
    /// Read the system clipboard and hand what it holds back as
    /// [`Event::EditorPaste`]. The core decided *that* a paste belongs in the
    /// buffer before asking; what the clipboard says is the edge's to observe,
    /// so it is never remembered here. A machine with no clipboard answers with
    /// nothing, the way OSC 52 gives copying somewhere to go but reading
    /// nowhere to come from.
    ReadClipboard,
    /// The project's story folder — read whenever Story view is opened, since
    /// the folder may have gained an artifact since it was last looked at.
    ReadStories {
        dir: PathBuf,
        /// The repository the set's range is read against — the Guest repo
        /// when one is under review, since whether a range still resolves is
        /// a question about the repository the Story describes.
        repo: PathBuf,
    },
    /// Diff and read every file an arriving artifact names — the text the AI
    /// no longer transcribes, and the hunks the arrival checks are run
    /// against. Only the core knows which files an artifact names (that needs
    /// the parse) and only the edge can read them, so this is a round trip.
    ///
    /// `base` and `head` are the arriving range's own, not the poll's:
    /// `head` is `None` when the range is the working tree's own, and
    /// `Some` names the commit a committed range ends at. Carried rather than
    /// left to the two-second poll, which is still describing whatever set was
    /// loaded before this one — checking a committed range against the working
    /// tree finds no hunks and flags every Step.
    ReadStoryFiles {
        /// Which repository to read them out of ([`State::repo_root`]).
        repo: PathBuf,
        base: String,
        head: Option<String>,
        files: Vec<String>,
    },
    /// Write the confirmed range's companion file (ticket 03) beside the Story
    /// set it was authored for. Only the edge can resolve the range's oids and
    /// read git's diff, but the format is [`story::context_file`]'s — the edge
    /// gathers and writes, and formats nothing.
    WriteStoryContext {
        /// Which repository the range is git's to answer for.
        repo: PathBuf,
        spelling: String,
        /// Absolute, and inside Varde's own directory: the prompt has already
        /// named this path to a session whose working directory is nobody's
        /// to move.
        path: PathBuf,
    },
    /// Read the global config for a Tools row being taken, or for what its
    /// install configures. Only the edge reads a file; what is written back is
    /// decided from the text it returns.
    ReadGlobalConfig {
        path: PathBuf,
        kind: tools::Kind,
        name: String,
        write: tools::Write,
    },
    /// A taken row's install has ended: read the status its sentinel holds.
    ReadInstallStatus(PathBuf),
    /// Read the repository's branch refs for the picker, with whether the
    /// folder is a repository and whether its tree is clean — three git
    /// questions one read answers, so the picker is refused before it is drawn
    /// rather than after a row is picked.
    ReadBranches,
    /// The clone the shell pane was given has ended: read the exit status the
    /// sentinel carries, and — for a clone that worked — the Guest repo's own
    /// refs. The status is a file's contents and the refs are `git2`'s, both
    /// of them reads only the edge can make; the sentinel's path and where
    /// the Guest repo went are the core's (ADR 0015).
    ReadGuestBranches {
        sentinel: PathBuf,
        repo: PathBuf,
        /// Which git wrote that status, carried out and back rather than read
        /// off the state when the answer returns: the state a download was
        /// asked in is not guaranteed to be the state its status arrives in,
        /// and a fetch reported as a failed clone is a sentence about a
        /// command nobody ran.
        how: story::Download,
    },
    /// Check the picked branch out, and say which branch was left. Only the
    /// edge can move `HEAD`, and only it knows where `HEAD` was.
    CheckoutBranch {
        repo: PathBuf,
        name: String,
    },
    /// Walk the offline default-branch ladder (or check an explicit range),
    /// and say whether the result is already authored — only the edge can
    /// ask git.
    ResolveStory {
        /// Which repository the ladder is walked in ([`State::repo_root`]).
        repo: PathBuf,
        /// Where the artifact belongs, so the path the AI is handed is
        /// absolute rather than relative to a working directory Varde does
        /// not set.
        dir: PathBuf,
        explicit: Option<String>,
        force: bool,
    },
    /// Measure Risk over a Scope, off the main loop. The edge walks the files,
    /// runs the analyser and answers with `Event::RiskFigures`; how long that
    /// takes is nothing the core waits for.
    AnalyseRisk {
        scope: risk::Scope,
        /// Which request this is, echoed back with the figures. A recompute
        /// supersedes rather than queues, so the job that was started first can
        /// still be the one that finishes last — and its answer describes a
        /// workspace state nobody is asking about any more.
        generation: u64,
        /// Exactly the files the Scope covers. `None` is the whole workspace,
        /// which only the edge can enumerate — the walk is what knows what the
        /// project ignores — and is not the same as a Scope that covers no
        /// files, which measures nothing.
        files: Option<Vec<String>>,
        /// A revision to measure the same files at as well, so the answer
        /// carries both sides of a delta. `None` measures the working tree
        /// alone.
        base: Option<String>,
    },
    /// Copy the working tree aside before an Iteration's session is let at it,
    /// under the Iteration's number. Per Iteration and never a commit:
    /// `git stash` and `git checkout HEAD -- .` both destroy uncommitted work
    /// that was the user's before the loop started
    /// (`docs/adr/0010-varde-owns-the-test-gate.md`).
    ///
    /// The whole tree whichever Scope the loop is over, so the Scope is not in
    /// it: the restore puts back only the files the Iteration touched, so a
    /// snapshot wider than the Scope costs disk and nothing else — while one
    /// narrower could not put back a file the session edited outside the Scope
    /// it was given.
    Snapshot {
        iteration: u32,
    },
    /// Put the files this Iteration changed back as its snapshot has them.
    /// Only those: a file the Iteration did not touch is never restored over,
    /// which is the whole reason the snapshot exists instead of git.
    RestoreSnapshot {
        iteration: u32,
    },
    /// Run the project's tests off the main loop and answer with
    /// `Event::TestsFinished`. Never in the shell pane: the shell is the
    /// user's, and a loop that types in it takes it away.
    RunTests {
        command: String,
    },
    /// Run a configured formatter over the text the Buffer holds, off the main
    /// loop, and answer with `Event::FormatterAnswered`. The text goes in on
    /// the child's stdin and the result comes back on its stdout: the file on
    /// disk is never read and never written, because the Buffer is what the
    /// reader is looking at — the same rule the Language server is held to
    /// (R32.4). Its *name* is another matter: a row that asked for `${file}`
    /// gets it, which is what lets one command tell JSON from YAML.
    ///
    /// `revision` rides along untouched so the answer can be measured against
    /// the Buffer it was asked about, and `language` so a command the edge
    /// could not find can be looked back up in the table that named it.
    RunFormatter {
        language: String,
        command: String,
        args: Vec<String>,
        path: PathBuf,
        revision: u64,
        text: String,
    },
    /// Say these words at this pace. Words and a multiplier, deliberately:
    /// never a path, never a command and never the synthesizer's own parameter
    /// — which scales *duration* and therefore runs backwards, so the
    /// reciprocal is the edge's to take (R35.7). What speaks them and what
    /// plays the result are `[speech]` rows the edge holds, so this effect
    /// names neither and no test has to mock one
    /// (`docs/adr/0013-a-voice-is-an-installed-binary.md`).
    ///
    /// The whole Reading in one effect, and the silence between its Utterances
    /// with it: one effect is one stream, and one stream is what stops the gap
    /// between sentences being ~150ms of process spawn (R35.4).
    Speak {
        utterances: Vec<reading::Utterance>,
        speed: f32,
    },
    /// Play the stream that is already built, from this offset — a resume, a
    /// next, a previous. Never a re-synthesis: the words and the pace are the
    /// same ones, and building them again would cost the second the whole
    /// budget is measured in (R35.6).
    ///
    /// The offset is milliseconds into the *Reading*, not into whatever the
    /// player was last handed, because that is the only frame both ends agree
    /// on — the edge rewrites the stream from it, which is sample-accurate and
    /// measured at 7.5ms, rather than freezing a player whose audio device
    /// then underruns and clicks.
    SpeakFrom {
        at_ms: u32,
    },
    /// Stop the sound and *keep* the stream. What a pause is, and the whole
    /// difference between it and the stop below: the file is what makes
    /// resuming free.
    PauseSpeaking,
    /// Stop the player and take the stream with it. Pushed from every site
    /// that ends a Reading, quitting included: the file lives outside the
    /// workspace so nothing in the tree flickers, and it is deleted when the
    /// player exits and again on the way out
    /// (`docs/adr/0014-scratch-audio-lives-outside-the-workspace.md`).
    StopSpeaking,
}

impl Effect {
    /// [`Effect::NotifyAbout`] with the control characters taken out of what it
    /// says. Grep `NotifyAbout`: every one that is *built* is built here, for
    /// the reason `Effect::SetTerminalInput` is stripped in one place too — the
    /// thing a notice says out loud is a path, a name off the filesystem or a
    /// child command's own words, and raw ANSI in file-derived text bound for
    /// the terminal can rewrite the screen, which is the rule AGENTS.md's
    /// Security section states. Four producers each remembering to strip is a
    /// fifth notice that forgets, and when they were counted one of the four
    /// remembered. In the core rather than at the edge because what a notice
    /// says is the core's to decide, and because a test can see it here — the
    /// terminal library drops a control character before it reaches a cell,
    /// which is this promise being kept by somebody else.
    ///
    /// It is only a stripping: `\u{1b}[2J` loses its escape and keeps its
    /// `[2J`, which is what makes the notice still name the path or the line
    /// the reader has to go and look at.
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

/// A place in a buffer: 1-based line and column, like the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub line: usize,
    pub column: usize,
}

/// What the pointer is over, as far as the buffer's text is concerned. The
/// Hover box is one answer among the three rather than a flag beside the place:
/// a pointer on the box is over no symbol, and the box floats over whatever is
/// under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pointed {
    #[default]
    Elsewhere,
    Text(Place),
    Hover,
    /// The gutter's Breakpoint column on this line, where an Unverified
    /// breakpoint says why.
    Breakpoint(usize),
}

/// The one selection the workspace holds. Its two representations do not merge
/// (ADR-0001): a buffer's is anchored to line and column so it survives
/// scrolling and edits, while a pty pane has no buffer to anchor to, so the edge
/// hands over the characters it read off the screen grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// Charwise, with both ends included, as vim's charwise span is: one shift
    /// and arrow from a cursor picks the character it left and the one it
    /// landed on.
    Buffer { anchor: Place, cursor: Place },
    /// Linewise, low line first, both ends included — what `V` picks.
    ///
    /// Derived from the buffer and never assigned: the Buffer is the author of
    /// the span (its clamp maintains the anchor, its operators act on it, its
    /// mode decides what the next key means) and `settle` reads it off there
    /// once per event. One author and one derived field, for the reason
    /// `AGENTS.md` gives for `ai_running`: a field the core sets *and* the
    /// buffer owns has two authors and diverges.
    Lines { from: usize, to: usize },
    /// Anchored to a pty pane's screen grid, because a pty has no buffer to
    /// anchor to. Carries the text as well as the span: the edge is the only
    /// thing that can read a grid, so it reads once and the span is what the
    /// renderer needs to show what was picked.
    Screen {
        pane: Pane,
        from: Place,
        to: Place,
        text: String,
    },
}

impl Selection {
    /// A buffer span's ends, earlier first: extending leftwards or upwards puts
    /// the cursor before the anchor.
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

    /// The cells a pty selection covers, if it is this pane's. Without it a
    /// drag over a pty pane picks text and shows nothing, which reads as
    /// selection not working at all.
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

// No `Eq`, for the reason `Effect` above has none: it holds the configured
// speed, which is a float.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub root: PathBuf,
    /// Where Varde's own files go, when they may not go in the workspace: the
    /// Sidecar of a Bare workspace, `None` for a project. Derived by the edge
    /// and read through [`varde_dir`] — never written here, the rule every
    /// edge-observed field follows.
    pub sidecar: Option<PathBuf>,
    /// `~/.varde`, handed in by the edge. The one thing written there is a
    /// review submitted from a Bare workspace, whose Sidecar is deleted at
    /// exit and whose folder is the user's own (ADR 0016).
    pub varde_home: PathBuf,
    pub modal: Modal,
    pub view: View,
    pub double_tap_ms: u64,
    /// How many spaces Tab lays down while inserting. Configuration rather
    /// than a scan of the file: a number measured off the text is a guess that
    /// disagrees with the project's own answer, and detecting one is a crate's
    /// job if it ever becomes one.
    pub tab_width: usize,
    pub reports_modifiers: bool,
    /// Folders whose entries are known. A folder that has never been expanded
    /// is absent, which is what makes the tree lazy.
    pub contents: BTreeMap<PathBuf, Vec<Entry>>,
    pub expanded: BTreeSet<PathBuf>,
    pub ignored: BTreeSet<PathBuf>,
    /// What is typed in the tree's filter box.
    pub filter: String,
    /// Every file in the project, relative to the root — walked once, when the
    /// filter is first used, because the tree itself is lazy.
    pub indexed: Vec<String>,
    pub buffers: BTreeMap<PathBuf, Buffer>,
    pub current_buffer: Option<PathBuf>,
    /// The buffer each view other than the one on screen was showing when it
    /// was left. The buffers are shared, the one in front is not: a file a
    /// story walk opened is not what Edit view was holding.
    pub view_buffers: BTreeMap<View, PathBuf>,
    /// How many of the files the last session had open are still on their way
    /// in — one per `Effect::OpenBuffer` starting asked for. Counted rather
    /// than assumed, because the openings arrive as the same event a reader's
    /// own arrives as, and a restored file is not somewhere anybody *went*: a
    /// history claiming the two files it just reopened is a history nobody
    /// walked (F34).
    pub restoring: usize,
    /// The buffer opened by browsing, which the next preview replaces. Opening
    /// a file properly clears this, and it stays.
    pub preview: Option<PathBuf>,
    /// git's answer, supplied by the edge. `None` means not a repository.
    pub repo: Option<Vec<GitFile>>,
    /// What the last commit holds for each open buffer, told by the edge on
    /// the same poll as `repo` and never written here — `None` for a file the
    /// commit does not hold. It is the side the change marks diff the buffer
    /// against, which is why the key is the buffer's path and not git's.
    pub committed: BTreeMap<PathBuf, Option<String>>,
    /// Who last committed each line of each open buffer, in the commit `HEAD`
    /// names — one entry per line of the file *as that commit holds it*, so a
    /// buffer line is traced back through the diff before it is indexed
    /// (`authorship::traced`). Told by the edge on the same poll as `committed`
    /// and cached there against the commit, never against `Buffer::revision`:
    /// reading it walks a file's history, and typing must not start one.
    /// Nothing at all for a file the commit has no copy of.
    /// Shared rather than owned: a row per line of every open buffer, and
    /// `update` clones the whole `State` per event, so owning it cost a tree
    /// step milliseconds of copying (#86).
    pub authorship: BTreeMap<PathBuf, std::sync::Arc<[authorship::Authored]>>,
    /// The current buffer traced back through its commit — which committed
    /// line each of its lines came from ([`authorship::traced`]) — keyed by
    /// the buffer and the revision it is about. Both the Change bars and the
    /// Authorship read it. Told by the edge, which works it out once per
    /// revision and once per commit, before the frame that draws them: it is a
    /// diff of the whole file, and worked out on every frame it was most of
    /// what a big file's frame cost (#101), while a trace a keystroke late put
    /// every bar below an added line on the wrong line (#104).
    pub traced: Option<Traced>,
    pub comments: Vec<Comment>,
    pub reviews: BTreeSet<u32>,
    pub retention_limit: usize,
    /// Whether a session exists — supplied by the edge, like the mouse and
    /// paste facts below, and never written here.
    pub ai_running: bool,
    pub ai_command: String,
    pub editor_theme: String,
    /// Whether the editor pane paints a darker field behind the code. On by
    /// default and remembered per project: it is a reading preference, and a
    /// terminal's transparency is the reason it is a preference at all — an
    /// explicitly painted cell is opaque, so the field buys contrast at the
    /// price of seeing through the window.
    pub editor_field: bool,
    /// Whether the minimap is showing. A reading preference like the field
    /// above, remembered per project, and the config's `editor.minimap` is
    /// only where it starts: the columns it costs the text are worth it in one
    /// file and not in another.
    pub minimap: bool,
    /// Whether the AI pane's rectangle shows the session or the Cheatsheet.
    /// The AI pane at every launch and never saved: `:help` and the palette
    /// are how the Cheatsheet comes up.
    pub ai_slot: layout::Slot,
    /// The first Cheatsheet row on screen, clamped in `settle`.
    pub cheatsheet_scroll: usize,
    pub system_clipboard: bool,
    /// Open when search is showing; `None` when the modal is closed.
    pub search: Option<Search>,
    /// The in-file search, from `/` until Escape; `None` while there is none.
    /// Separate from `search`: one finds here, the other everywhere.
    pub find: Option<Find>,
    /// What the replace box last replaced a match with — remembered for the
    /// session, so it outlives the search it was typed in.
    pub replace_with: Buffer,
    pub diff: Option<Vec<DiffLine>>,
    pub diff_file: Option<String>,
    /// The blob oid of what `diff_file` holds, told by `Event::ShowDiff` — a
    /// comment can only be made against the diff currently shown, so there is
    /// one of these rather than one per file.
    pub diff_revision: Option<String>,
    pub diff_line: usize,
    diff_anchor: Option<usize>,
    pub last_verdict: Option<String>,
    pub focus: Pane,
    pub tree_divider: u32,
    /// How wide the AI pane is, once its edge has been dragged. `None` until
    /// then, which `layout::panes` reads as its share of the screen.
    pub ai_width: Option<u32>,
    /// Whether that column stops above the terminal or runs the whole height.
    pub ai_pane: layout::AiPane,
    /// How tall the Strip is, once the border above it has been dragged, and
    /// `None` until then. Clamped here on every drag and every resize rather
    /// than only where it is drawn, so what is remembered is what was seen.
    pub strip_height: Option<u32>,
    /// Which group the Strip is showing.
    pub strip: layout::Group,
    /// Whether the edge holds the debugged program's own terminal. Told by the
    /// edge, for `ai_running`'s reason: a program asked for is not a process
    /// that exists, and the core would otherwise route keys to a pane nobody
    /// holds.
    pub output_running: bool,
    /// Whether the Program output is out of sight, which gives the Variables
    /// the whole Debug group. Hiding it stops nothing: the pty keeps its size
    /// and everything it printed, so this is the one fact hiding changes.
    pub output_hidden: bool,
    /// How wide it is once the border between it and the Variables has been
    /// dragged, and `None` until then — a share of the group, as `ai_width`
    /// is of the screen. Kept while it is hidden, which is what showing it
    /// again brings back.
    pub output_width: Option<u32>,
    /// Whether it has printed anything since it was last on screen, which is
    /// what the `Debug` Group tab and the show Chip are marked with.
    pub output_unseen: bool,
    pub output_mouse: mouse::Encoding,
    pub output_paste: keys::Paste,
    /// Every Breakpoint in the workspace, with or without a Debug session.
    pub breakpoints: Vec<debug::Breakpoint>,
    /// The floating window that runs a Snippet in the Paused program, while
    /// one is open.
    pub evaluator: Option<debug::Evaluator>,
    /// Where that window sits, whether or not one is open: the rectangle
    /// outlives the window, because a project reopens the Evaluator where it
    /// last left it. `None` until one has been opened here or the project
    /// records one, which is what centres the first. Clamped in [`settle`]
    /// like every other view's offset, so no arm that moves it has to
    /// remember the screen or the Paused line.
    pub evaluator_at: Option<layout::Area>,
    /// The modifier-free mode that moves and resizes that window from the
    /// keyboard, and `None` while nobody asked for one. An enum rather than
    /// two flags for the reason [`Modal`] is one: moving and resizing at once
    /// has no answer for `l`.
    pub arranging: Option<debug::Arrange>,
    /// The Snippets that have left that window, oldest first, remembered per
    /// project. Beside the Watches rather than inside the session for the
    /// reason a Watch is: code written to ask a program something outlives
    /// the run it was first asked in.
    pub snippets: Vec<String>,
    /// The Exception filters switched on, by the ids each Debug adapter
    /// reported them under, keyed by the adapter's row name and remembered
    /// per project. Outside the session so the next one starts with them,
    /// and keyed by adapter because one adapter's `uncaught` is not another's.
    pub exception_filters: BTreeMap<String, BTreeSet<String>>,
    /// The expressions kept at the top of the Variables, in the order they
    /// were added. Beside the Breakpoints rather than inside the session for
    /// the same reason: a Watch is a question about the program, and it
    /// outlives the session it was first asked in.
    pub watches: Vec<debug::Watch>,
    pub terminal_mouse: mouse::Encoding,
    /// The terminal strip's shells, side by side, and what each is doing.
    /// Told by the edge — it starts them, watches them exit and asks the OS
    /// what is in their foreground — and never counted here: a `:split` whose
    /// shell failed to start would otherwise be a split with no pty behind it.
    pub terminals: Vec<Shell>,
    /// A command that has no idle shell to go to yet: every split was busy,
    /// so one was asked for, and this waits for its prompt (R38.5).
    pub pending_command: Option<Effect>,
    /// Which of them has the keyboard when `focus` is the terminal. Read
    /// through `split()`, which bounds it by what the edge holds: it is set
    /// ahead of the shell it names, and the shell may never come.
    pub terminal_split: usize,
    /// The same for the AI pane. Two children, two independent facts — an AI CLI
    /// asks for mouse events (and the alternate screen, which has no history)
    /// while the shell beside it has not.
    pub ai_mouse: mouse::Encoding,
    /// Whether each child asked to be told that a paste is a paste, read from
    /// the same terminal model the mouse encoding above comes from.
    pub terminal_paste: keys::Paste,
    pub ai_paste: keys::Paste,
    /// Whether the AI's child has printed anything. Until it has, it drops
    /// what it is sent, so whatever is handed to it waits in
    /// `pending_prompt` — the same queue, since only one thing is ever
    /// waiting to reach the one AI pane.
    pub ai_spoken: bool,
    pub pending_prompt: Option<(String, Enter)>,
    pub selection: Option<Selection>,
    /// The other places the next-occurrence gesture has taken, each the start
    /// of a run as long as the one `selection` holds — the second cursor and
    /// every one after it. Empty is the ordinary case: one cursor and one
    /// selection.
    ///
    /// The primary is `selection` and its buffer's cursor rather than the
    /// first of these, so nothing here has two authors: the keys that move a
    /// cursor and the operators that act on a span go on reading the one place
    /// they always read, and a stale list would otherwise be a second answer
    /// to where the cursor is. Once typing has begun the selection is spent
    /// and these are insertion points of no width, exactly as the primary
    /// cursor is.
    pub occurrences: Vec<Place>,
    /// The occurrence the event being settled took, which the editor's clamp
    /// shows after the cursor and then forgets. Only the event that took it
    /// knows it was taken — typing moves every occurrence without taking one,
    /// and following the newest then would pull the cursor off the screen on
    /// every keystroke — and the clamp is the one writer of the offset.
    pub revealing: Option<Place>,
    /// Where the pointer is while the jump modifier is held, in the editor's
    /// own coordinates. The place, not the span: what it underlines is read off
    /// the buffer through [`link`], so an edit under the pointer cannot leave a
    /// remembered span pointing at characters that moved.
    pub link: Option<Place>,
    /// Which clickable icon the pointer is resting on, named the way every
    /// other answer about one is: the action itself, not a pane and an index.
    /// The names are already unique across the four strips that draw them —
    /// the click routes by this very string — so one field covers the tree's
    /// row actions, the Risk list's and its border's, the Cursor history's and
    /// the Transport, and a fifth strip needs no fifth spelling.
    ///
    /// A terminal has no hand pointer to turn the mouse into, so an icon that
    /// looks the same whether or not you are on it is an icon nobody knows is
    /// a button. This is the whole affordance, the way the underline is a
    /// link's.
    pub hovered_action: Option<&'static str>,
    /// The Transport action taken last, by click or by key, which its Chip
    /// stays lit for until another is taken. Named by action for the reason
    /// `hovered_action` is, so one field serves every Transport.
    pub transport_lit: Option<&'static str>,
    /// Whether the pointer is on the minimap. Transient like the hovered icon
    /// above and never saved: it is where the pointer is, not a preference.
    pub hovered_minimap: bool,
    /// The tree row the keyboard is on. Only this row offers actions.
    pub tree_selection: Option<PathBuf>,
    /// The first row each list shows. The wheel moves them freely; everything
    /// else pulls them back so the selected row and the cursor stay on screen.
    pub tree_scroll: usize,
    pub editor_scroll: usize,
    /// Why the last thing asked for did nothing, for the footer to say out
    /// loud. Cleared by the next event, so it reports the keystroke in front of
    /// the reader rather than one from earlier in the session.
    pub refusal: Option<preview::Refusal>,
    /// The first *column* of text the editor pane shows. Lines are longer than
    /// the pane is wide and there is another pane immediately to its right, so
    /// without this the tail of a line is drawn under a neighbour that has not
    /// been asked to give up the columns — text you typed, on screen, behind
    /// something else. Clamped by the same `viewport` the rows use: keeping the
    /// cursor in view is the same problem in the other dimension.
    pub editor_hscroll: usize,
    /// The screen, as last reported by the edge. Zero until it reports, which
    /// makes every pane one row tall — harmless, and it means no scroll offset
    /// can point past content that has not been measured yet.
    pub screen_width: u16,
    pub screen_height: u16,
    /// Which of that row's actions the keyboard is on, once you have stepped
    /// into them with Right. `None` means the keyboard is still on the row.
    pub selected_action: Option<usize>,
    gutter: Option<(String, u32, u32)>,
    /// The body of the comment being written, while the box is open. A
    /// [`Buffer`] rather than a string the edge appends to, so the box inherits
    /// the motions, the newline, the undo and the one-edit paste the editor
    /// already has — a string is where the second text editor gets written, and
    /// where the first newline in a pasted stack trace filed a fragment of one.
    ///
    /// Beside the modal rather than inside `Modal::Comment` for the reason
    /// `Modal::Tools` gives about its row: the renderer and `update` both read
    /// it, and a variant is the wrong place for something two callers need.
    pub comment: Option<Buffer>,
    last_tap: Option<(Tap, u64)>,
    /// Varde's own checkout, when the running binary came from one. Known here
    /// and not only at the edge, because the rebuild command has to name it —
    /// the terminal pane's working directory is the workspace, not the checkout.
    pub checkout: Option<PathBuf>,
    /// The Version of the Update, when there is one: the checkout's, set once at
    /// startup, or the Release's, set when it is answered. Neither can change
    /// afterwards, so there is nothing to watch and nothing to dismiss.
    pub update: Option<String>,
    /// The Release a binary install found newer than itself, which is what
    /// `:update` will fetch. `None` on a checkout install, and whenever the
    /// answer offered no Update.
    pub release: Option<startup::Release>,
    /// `Startup::running_version`, kept because a Release is answered after
    /// startup has returned.
    pub running_version: String,
    /// The binary on disk is already the Release, so `:update` relaunches
    /// rather than fetching again. Dies with the process, which is right: the
    /// next process is that binary.
    pub replaced: bool,
    /// The story artifact for the current change, or the reason there is
    /// none to walk.
    pub story_set: story::Set,
    /// The branch `HEAD` is on, as the edge last read it. Told on the same poll
    /// `head` is and never written here: which branch a repository is on is
    /// only observable at the edge, and a branch the core remembered would go
    /// on naming it after a `git switch` in the terminal pane.
    pub branch: Option<String>,
    /// The branch the picker left, once one was picked. Remembered rather than
    /// told, unlike [`State::branch`] beside it, because it is a *past* fact:
    /// no read of the repository can say where the reviewer was. Story view
    /// says it next to where they are now, because Varde does not put them
    /// back — a second checkout can fail and leave them somewhere neither they
    /// nor Varde chose.
    pub left_branch: Option<String>,
    /// The Guest repo in the Sidecar, once its branches have been listed —
    /// `None` while the repository under review is the workspace itself.
    /// Remembered rather than told: where the clone went is the core's own
    /// decision (ADR 0015), made before the clone ran, and re-deriving it at
    /// the edge would be the second spelling of one path ticket 01 exists to
    /// prevent.
    pub guest: Option<PathBuf>,
    /// Every Guest repo URL downloaded into this session's Sidecar, which is
    /// what [`story::Download`] is chosen from. Recorded when the branches are
    /// listed rather than when the download is asked for: a clone that failed
    /// left nothing to fetch from.
    pub guests: Vec<String>,
    /// What Story view's tree pane is listing — the spine, or the
    /// changed-files list `t` toggles it against.
    pub story_listing: story::Listing,
    /// Which spine row the keyboard is on — the spine's own selection,
    /// since its rows are Stories rather than tree paths and so cannot
    /// share `tree_selection`. Enter on this row is what walks a Story.
    pub story_selection: usize,
    /// The spine's own scroll offset — a Story is not a path, so the spine
    /// cannot share `tree_scroll` the way the changed-files listing does
    /// (which must come back untouched when `t` toggles away from the spine
    /// and back).
    pub spine_scroll: usize,
    /// The reviewer's position in the Story being walked, or `None` while
    /// the spine itself has focus. Distinct from `story_listing`, which
    /// toggles the spine against the changed-files list and stays whichever
    /// it was when walking began.
    pub walking: Option<story::Walking>,
    /// The hunks in every changed file, supplied by the edge alongside
    /// `repo` and on the same cadence — the Remainder recomputes from this
    /// rather than from a snapshot taken when the story set loaded. Shared
    /// rather than owned: it holds every changed file's text on both sides,
    /// and every event clones `State` (ADR 0023).
    pub file_hunks: std::sync::Arc<[story::FileHunks]>,
    /// Every story-set filename the watcher has told this workspace about,
    /// oldest first — the core's only way to know which one is oldest, since
    /// the AI writes each file and Varde never lists the directory itself.
    pub story_sets: Vec<String>,
    /// The workspace's figure, or the fact that it is being measured.
    pub risk: risk::Risk,
    /// Which pane occupies the corner beneath the tree, if any. Persisted per
    /// user, like the AI pane's shape: a corner nobody opened stays closed.
    pub corner: layout::Corner,
    /// Whether the list shows every Function or only those above the
    /// threshold. The worklist is the point, so the default is the worklist;
    /// everything is for reading a figure that is not yet a problem.
    pub risk_all: bool,
    /// Which row of the Risk list the keyboard is on — an index, because a
    /// Function is not a path and so cannot share `tree_selection`. It is a Row
    /// selection and never `selection`: it names something to go to rather than
    /// text somebody picked, so nothing copies it.
    pub risk_selection: usize,
    /// The first row the Risk list shows. Its own offset for the reason the
    /// spine has one: the pane holds rows nothing else holds, and the renderer
    /// and the hit-test read this same field rather than each deriving one.
    pub risk_scroll: usize,
    /// Which row of the Buffers pane the keyboard is on — an index into
    /// `buffers`, for the reason the Risk list's is an index. It follows
    /// `current_buffer` whenever something else moves it, so the highlight and
    /// the editor's dot strip cannot say different things about which buffer
    /// you are in; from there the arrows move it freely.
    pub buffers_selection: usize,
    /// The first row the Buffers pane shows. Its own offset rather than the
    /// Risk list's, even though the two share a rectangle: the lists have
    /// different lengths, and a shared offset would scroll one pane by
    /// whatever the other was left at.
    pub buffers_scroll: usize,
    /// Every place the cursor has been, oldest first. Session state and never
    /// persisted: a Visit names a line of a file, and a file moves between
    /// sessions.
    pub visits: Vec<history::Visit>,
    /// Where the history's cursor is standing — both the row the pane
    /// highlights and the position going back and forward move. One field for
    /// one position, for the reason `risk_selection` is one: two fields are two
    /// fields that can disagree about where the keyboard is. `visits.len()` is
    /// the position past the newest Visit, which is where you are before you
    /// have travelled anywhere.
    pub history_selection: usize,
    /// The first row the Cursor history pane shows. Its own offset for the
    /// reason the two panes beside it in the corner have their own.
    pub history_scroll: usize,
    /// The row the Breakpoint list highlights, and its first row on screen.
    pub breakpoints_selection: usize,
    pub breakpoints_scroll: usize,
    /// The same two for the Frames.
    pub frames_selection: usize,
    pub frames_scroll: usize,
    /// The same two for the Diagnostic list. An index into `lsp::listed`, so
    /// a report that removes the row it is on leaves it on the row that took
    /// its place rather than sending it back to the top.
    pub diagnostics_selection: usize,
    pub diagnostics_scroll: usize,
    /// The same two for the Conflict list, an index into `conflict::listed`
    /// for the same reason: resolving the Conflict it is on leaves it on the
    /// row that took its place.
    pub conflicts_selection: usize,
    pub conflicts_scroll: usize,
    /// The Conflicts in each unmerged file as the edge last read it off the
    /// disk, on the git poll's thread. Told, never written here; an open
    /// Buffer's own are read instead.
    pub conflicts_on_disk: BTreeMap<PathBuf, Vec<conflict::Conflict>>,
    /// And for the Variables, which is a list in the Strip rather than in the
    /// corner but is one all the same — a row to open, and a first row on
    /// screen.
    pub variables_selection: usize,
    pub variables_scroll: usize,
    /// What runs each language's Debug adapter, and the Launch
    /// configurations, as configuration named them — data for the reason
    /// `servers` is (ADR 0021).
    pub adapters: BTreeMap<String, startup::Adapter>,
    pub launches: BTreeMap<String, startup::Launch>,
    /// What Run marks stand beside and start, by row (R41.1).
    pub runs: BTreeMap<String, startup::Run>,
    /// The Launch configuration the last session was started from, which
    /// restart reruns. Outlives the session on purpose: rerunning is what the
    /// reader reaches for once a program has ended. The configuration and not
    /// its name, because a Run mark's has none.
    pub last_launch: Option<startup::Launch>,
    /// The Debug session, if one exists.
    pub debug: Option<debug::Session>,
    /// Stepping mode: a Space chord has just run, so the stepping letters act
    /// without their Space until any other key leaves it. Never on without a
    /// session — `n` is find-next the rest of the time, and a mode that
    /// swallowed it for nothing would be a trap.
    pub stepping: bool,
    /// How many ticks the edge has reported. The edge ticks only while it holds
    /// work, so this advances while a job runs and stands still otherwise —
    /// which is the whole of what the core knows about it
    /// (`docs/adr/0009-a-spinner-is-bounded-by-its-job.md`). Not a clock and
    /// not a duration: nothing may be timed off it.
    pub tick: u64,
    /// The figure above which a Function is on the worklist, from the
    /// project's configuration.
    pub risk_threshold: u32,
    /// The Refactor loop, running or stopped.
    pub refactor: risk::Refactor,
    /// How many Iterations one loop may run, from the project's configuration.
    pub max_iterations: u32,
    /// What the Gate runs, from the project's configuration. Absent means the
    /// project did not say, and the project's shape is asked instead.
    pub test_command: Option<String>,
    /// What `HEAD` resolves to, told by the edge on the same poll as `repo` and
    /// never remembered: it is the commit a cached figure is checked against,
    /// and the one a fresh figure is written under.
    pub head: Option<String>,
    /// Every `(story, step)` whose Prediction has already been put to the
    /// reviewer, whether they answered it or stepped past — never which
    /// choice, because that would be a score by another name. Cleared
    /// whenever a story artifact loads, so re-authoring puts every
    /// Prediction afresh.
    pub predictions_put: BTreeSet<(usize, usize)>,
    /// What runs each language's server, as configuration named it. Data, not a
    /// process: `docs/adr/0011-a-language-server-is-a-second-hosted-child.md`.
    pub servers: BTreeMap<String, startup::Server>,
    /// And what lays each language's files out when no server will. Data for
    /// the reason `servers` is data: which tool a project formats with is a row
    /// in a file, and an arm naming one is R31.1's forbidden arm wearing a
    /// second hat (R32.1).
    pub formatters: BTreeMap<String, startup::Formatter>,
    /// Which paths on this machine a server's configuration may name, and how
    /// the edge is to find each one — the `[facts.*]` tables. Data for the
    /// reason `servers` above is data: which marker file means "this directory
    /// configures the language" is an ecosystem's name, and an arm spelling one
    /// is the arm R31.1 forbids. The library decides nothing about a marker; it
    /// only knows which names a message may interpolate (R31.27).
    pub facts: BTreeMap<String, startup::Fact>,
    /// Where each language's conversation has got to. Everything here is a
    /// claim about a conversation the core is party to, never about a child
    /// process — which is why it may live here at all.
    pub lsp: BTreeMap<String, lsp::Conversation>,
    /// The question each key that asks about the symbol under the cursor is
    /// waiting on an answer to. One question rather than one request, because a
    /// file can be served by several servers and the reader pressed one key:
    /// which of them have answered, and whether one of them already has, is
    /// what `lsp::answered` arbitrates over.
    pub lsp_asked: BTreeMap<lsp::About, lsp::Question>,
    /// Which languages the edge holds a server for — supplied by it, like
    /// `ai_running` and the mouse facts, and never written here.
    pub lsp_running: BTreeSet<String>,
    /// Which of the configured commands this process can find on its `PATH`.
    /// A claim about the filesystem and this process's environment, so the
    /// edge's to observe and never `update`'s to write (R31.23) — the same
    /// split `lsp_running` above draws. Probed when Tools is opened
    /// rather than kept from startup, because the premise of the list is that
    /// what it describes is about to change.
    pub commands_on_path: BTreeSet<String>,
    /// Whether this machine has a `git` binary to run. The same split as
    /// `commands_on_path` above: the edge probes `PATH`, the core only reads
    /// the answer. `false` until told, because a capability nobody has checked
    /// is not a capability — a clone emitted on an assumption would be a
    /// command the shell pane cannot run and a wait nothing ends.
    pub git_installed: bool,
    /// What a `[speech]` row named, resolved for this OS. Configuration, so
    /// the core holds it — but it reads only whether a voice is named and what
    /// the install line says: which binary speaks and which one plays is the
    /// edge's copy of the same rows (ADR 0013).
    pub speech: reading::Speech,
    /// Whether the edge holds a synthesizer child, whether it found the
    /// configured player on this machine, and whether the file the voice names
    /// is on disk. Facts only the edge can observe, so `tell_core` sets them
    /// and `update` never does — `ai_running`'s lesson applied before it can
    /// bite, since a spawn that failed, a child that died and a model deleted
    /// after its install exited 0 are exactly the states a remembered flag
    /// gets wrong.
    pub voice_running: bool,
    pub player_installed: bool,
    pub voice_installed: bool,
    /// The Reading in flight, if there is one. The core's, unlike the two
    /// above: which passage is being read is a consequence of an event rather
    /// than an observation of a child.
    pub reading: Option<reading::Reading>,
    /// What the edge found in this workspace for each fact `facts` declares,
    /// and nothing for one it could not find. The same split as
    /// `commands_on_path` above and for the same reason: the value is a path on
    /// this machine, so looking it up is the edge's and reading it is all the
    /// core does (R31.27). A declared name with no entry here is a name no
    /// server that asks for it is started with.
    pub workspace_facts: BTreeMap<String, String>,
    /// The Tools row a re-check is waiting on an answer about, if any. Set by
    /// `r` in the list and taken by the probe landing: without it a
    /// probe from opening the list and a probe the user asked for would be the
    /// same event, and the restart question would be offered to somebody who
    /// only looked at the list.
    pub recheck: Option<(tools::Kind, String)>,
    /// The Tools row whose install is running in the shell pane, until its
    /// sentinel reports. One at a time: they share the sentinel.
    pub installing: Option<(tools::Kind, String)>,
    /// The rows whose install reported a failure, until each is taken again.
    pub install_failed: BTreeSet<(tools::Kind, String)>,
    /// Which operating system this binary was built for, as `Startup` handed it
    /// in — the key an install command is looked up under, and nothing else. It
    /// is here rather than read from the environment for the reason R31.22
    /// gives: a row's offer is then a value a scenario can set.
    pub os: String,
    /// And the CPU, as `std::env::consts::ARCH` spells it — with `os`, the name
    /// of this platform's Asset.
    pub arch: String,
    /// What each server says about each file, keyed by path rather than held on
    /// the Buffer: a file's marks have to survive switching away from it and
    /// back, and Review view is told about changed files that were never opened
    /// as Buffers at all. An absent path is a file nothing has spoken about; a
    /// present path with an empty set under a language is a file that server
    /// called clean.
    ///
    /// And by publisher under that, because a `.vue` file is served by two
    /// servers with two different opinions of it: the Vue server marks a
    /// template mistake and a TypeScript server reports the type error. A push
    /// is one server's whole current opinion of a file, so what it replaces is
    /// *its own* last opinion — keyed by path alone, the second publisher erased
    /// the first and which mark a reader saw depended on who spoke last. It is
    /// also what lets a server that dies take exactly its own marks
    /// (`lsp::gone`), rather than every mark on every file it served.
    pub diagnostics: BTreeMap<PathBuf, BTreeMap<String, Vec<lsp::Diagnostic>>>,
    /// How many of those there are per Severity, in `lsp::Severity::ALL`'s
    /// order. Kept by `lsp` as each report lands, so the tree's border does
    /// not walk every Diagnostic in the project once a frame.
    pub diagnostic_totals: [usize; 4],
    /// What the server said about the symbol under the cursor, once a reply
    /// matched the question that caused it. Absent until one does, and gone
    /// again on Escape — or as soon as it stops describing what is under the
    /// cursor, which `lsp::hover_stands` is asked on every pass.
    pub hover: Option<lsp::Hover>,
    /// Where the pointer is resting in the buffer's text, as the edge reports
    /// it, and nothing at all when it is over anything else. Told by the edge
    /// as an event and never written anywhere else — the pointer is a fact
    /// only the edge can observe.
    ///
    /// The place is kept rather than what is under it, and both the things
    /// read off it are derived: the hover a rest asks for is about this place
    /// rather than about the cursor, which is also what says the box still
    /// describes where the reader is pointing, and the diagnostic box
    /// (`lsp::pointed`) is answered from it on the way past. A box remembered
    /// here would need taking down by every arm that moved the text under it.
    pub pointed_at: Pointed,
}

impl State {
    /// Which split has the keyboard, bounded by what the edge holds: the
    /// index runs ahead of a spawn and outlives an exit, and both land here.
    pub fn split(&self) -> usize {
        self.terminal_split
            .min(self.terminals.len().saturating_sub(1))
    }

    /// Where a pushed command goes: the split with the keyboard if its prompt
    /// is waiting, else the first whose prompt is, else nowhere yet.
    fn idle_split(&self) -> Option<usize> {
        let idle = |split: &usize| self.terminals.get(*split) == Some(&Shell::Idle);
        Some(self.split())
            .filter(idle)
            .or_else(|| (0..self.terminals.len()).find(idle))
    }

    /// The repository under review — the Guest repo when one has been cloned,
    /// the workspace otherwise. Every git question a Story asks is asked of
    /// this and not of [`State::root`]: a Bare workspace's folder is often no
    /// repository at all, and a range resolved against it would be no range.
    /// The workspace stays [`State::root`]'s: the tree, the buffers a save
    /// writes and what `.gitignore` covers are all about the folder Varde was
    /// opened on.
    pub fn repo_root(&self) -> &Path {
        self.guest.as_deref().unwrap_or(&self.root)
    }

    /// What a comment being written is attached to: file, and the new-file
    /// line range.
    pub fn comment_target(&self) -> Option<(String, u32, u32)> {
        self.gutter.clone()
    }

    /// The selection as text — one answer whether it was dragged off a pty's
    /// screen or extended in a buffer with the keyboard.
    /// The Buffer the keyboard is typing into: the Evaluator's Snippet while
    /// the floating window has focus, and the buffer on screen otherwise. The
    /// editor goes on showing its own file behind the window, so "the buffer
    /// in front" and "the buffer being typed into" are two questions, and
    /// this is the second — the one a mode, a Selection and a copy answer to.
    pub fn edited(&self) -> Option<&Buffer> {
        match self.focus {
            Pane::Evaluator => self.evaluator.as_ref().map(|it| &it.snippet),
            _ => current_buffer(self),
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        match self.selection.as_ref()? {
            // A Preview drag already resolved its text in `update`, against
            // the row map rather than the buffer's own lines — this is the
            // same `Screen` shape a pty selection carries once the edge has
            // read its grid, just built in-core instead of over `Event::SelectIn`.
            Selection::Screen { text, .. } => Some(text.clone()),
            // Whole lines, ends included, so what reaches the clipboard is
            // lines rather than a run of characters — vim's linewise register,
            // and what makes a pasted `V` selection land on lines of its own.
            Selection::Lines { from, to } => Some(self.edited()?.lines_in(*from, *to)),
            selection => {
                let (from, to) = selection.buffer_span()?;
                Some(self.edited()?.text_in(from, to))
            }
        }
    }

    /// The diff rows a visual selection covers, for rendering.
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
            indexed: Vec::new(),
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

/// The scroll clamp and the focus rule every arm's answer passes through. An
/// arm that returns early skips this deliberately — the tick is why (see the
/// scroll rule in AGENTS.md), and so is every arm that answers with the whole
/// result rather than with effects.
fn settle(mut next: State, mut effects: Vec<Effect>, wheeled: bool) -> (State, Vec<Effect>) {
    // The linewise selection is read off the buffer that authors it, here
    // rather than in every arm that could move it — the same rule that pulls
    // both scrolls back, takes down a stale hover box and pulls the candidate
    // list over its choice. Outside the wheel guard below because it is not a
    // view: the wheel takes the cursor with it, so a `V` span the wheel
    // extended is a span that moved, not a viewport that was allowed to drift.
    //
    // Only while the editor holds the keyboard, and only while it is drawing
    // the buffer's lines: the workspace holds exactly one Selection, so a drag
    // in a hosted pane or a span of diff rows must not be taken back by a
    // buffer left in visual mode behind it, and a Preview's rows are not its
    // lines — a span of lines shown against reflowed rows is the second
    // meaning for one shape that `Event::DragText` refuses for the same reason.
    //
    // Visual mode wins over the charwise arms this runs after: `V` and then a
    // Shift-arrow leaves a linewise span, not the charwise one `EditorExtend`
    // wrote. That is the mode doing what a mode does — it is also what the
    // editor was already *drawing*, from `Buffer::selected_lines`, while `d`
    // took the charwise arm and deleted something else.
    // What the reader can see, told once: the mark on the `Debug` tab is a
    // claim that output arrived unseen, so it lasts exactly as long as the
    // Program output is out of sight. Here rather than in the arms that show
    // it — the tab, the Chip, the chord and a session that ends are four, and
    // a mark one of them forgot is a tab that says something arrived when the
    // reader is looking straight at it.
    if showing_output(&next) {
        next.output_unseen = false;
    }
    // A mode bounded by the thing it arranges: the window closing with the
    // session is what takes the keyboard out of it, so nobody is left holding
    // four letters that move nothing.
    if next.evaluator.is_none() {
        next.arranging = None;
    }
    // Focus on the AI pane's rectangle is focus on whoever is in it, so
    // showing or hiding the Cheatsheet never moves focus, and a key never
    // reaches a session the Cheatsheet is covering.
    if matches!(next.focus, Pane::Ai | Pane::Cheatsheet) {
        next.focus = next.ai_slot.pane();
    }
    // The Cheatsheet has no cursor to follow, so its offset is only held
    // inside the rows there are — whatever moved it, and a resize.
    if next.ai_slot == layout::Slot::Cheatsheet {
        next.cheatsheet_scroll = next.cheatsheet_scroll.min(
            keys::cheatsheet_rows(&next)
                .len()
                .saturating_sub(cheatsheet_fits(&next)),
        );
    }
    // The Evaluator's window, from the one clamp: wholly on the screen and
    // clear of the Paused line, whatever moved it — a drag, a resize, or a
    // program that stopped on a line the window was floating over. Here
    // rather than in those arms for the reason the scrolls below are here:
    // an arm that has to remember the screen is an arm that will forget.
    // Outside the wheel's exception, which is about views following a cursor:
    // no wheel moves this window, so nothing here undoes one.
    if let Some(at) = next.evaluator_at {
        next.evaluator_at = Some(layout::placed_window(
            at,
            next.screen_width,
            next.screen_height,
            debug::paused_row(&next),
        ));
    }
    // A pane the edge no longer holds is a pane the keyboard must not be in —
    // the rule `AiExited` exists for, here because `output_running` is derived
    // and there is no event to hang it on.
    if next.focus == Pane::Output && !next.output_running {
        next.focus = Pane::Editor;
    }
    // The in-file search has the keyboard only while nothing else took it:
    // another pane, the project search or a modal. Losing it here, rather than
    // in every arm that moves focus, is also what lands a return to the editor
    // in the buffer rather than back in the box. The search stays on.
    if let Some(find) = next.find.as_mut() {
        if next.focus != Pane::Editor || next.search.is_some() || next.modal != Modal::None {
            find.keys = FindKeys::Away;
        }
    }
    let showing_lines = next.focus == Pane::Editor && next.diff.is_none() && !previewing(&next);
    let linewise = showing_lines
        .then(|| current_buffer(&next).and_then(Buffer::selected_lines))
        .flatten();
    match linewise {
        Some((from, to)) => next.selection = Some(Selection::Lines { from, to }),
        // Leaving visual mode takes the selection with it. Only its own: a
        // charwise or pty selection is nobody's to clear from here.
        None if matches!(next.selection, Some(Selection::Lines { .. })) => next.selection = None,
        None => {}
    }
    // The wheel is the one event allowed to move a view away from the cursor.
    // Everything else — a keystroke, a selection, a resize, files appearing —
    // pulls both views back to what they are supposed to be showing, which is
    // why no arm has to remember to do it.
    if !wheeled {
        let (tree_fits, editor_fits, editor_columns) = fits(&next);
        // The spine is not path-shaped rows (a Story is not a path), so
        // `tree::visible_rows` is always empty for it — clamping against that
        // would zero the changed-files list's scroll on every keystroke taken
        // while looking at the spine, and `t` is supposed to hand that list
        // back exactly as it was left.
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
        // The Risk list, against its own rows: the same rule as every other
        // list, so no arm has to remember to scroll and the renderer and the
        // hit-test read one clamped field.
        // Clamped to the last row: the slot past it is the pane's actions, which
        // live on the border and are always on screen, so scrolling to bring it
        // into view would scroll one row past the end of the list for nothing.
        let risk_rows_count = risk::list(&next).len();
        next.risk_scroll = layout::viewport(
            next.risk_scroll,
            next.risk_selection.min(risk_rows_count.saturating_sub(1)),
            risk_rows_count,
            corner_rows(&next),
        );
        clamp_buffers(&mut next);
        // The Cursor history, against its own rows. Clamped to the last row:
        // the position past the newest Visit is where the keyboard sits before
        // anything has been travelled to, and scrolling to bring a row that
        // does not exist into view would scroll one row past the end for
        // nothing.
        let history_rows = history::list(&next).len();
        next.history_scroll = layout::viewport(
            next.history_scroll,
            next.history_selection.min(history_rows.saturating_sub(1)),
            history_rows,
            corner_rows(&next),
        );
        // Clamped here rather than by the arm that changed the list: a report
        // arrives from a server at any time, and the index staying where it
        // was is what leaves it on the row that took a fixed one's place.
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
        // Clamped here and not in the arms that remove one, so a Breakpoint
        // gone by any route — a row's Chip, a deleted line — leaves the
        // highlight on a row that exists.
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
        // The Variables against the Strip's rows rather than the corner's,
        // being the one list of the lot that does not live in the corner.
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
        // The results box, against the same `search::rows` the renderer draws.
        // The row above the selection first and the selection itself second:
        // that row is the heading of the file a hit is the first of, and a hit
        // brought into view from below would otherwise land on the top row with
        // the name of its file just off it — which is no way to step through a
        // list grouped by file. Second, so it is the selection that is
        // guaranteed to be on screen.
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
        // A hover box is a claim about the symbol under the cursor, so it is
        // taken down the moment that stops being what it describes — the same
        // one-rule-here reason the scrolls are clamped in this function rather
        // than in every arm that could move one.
        if next.hover.is_some() && !lsp::hover_stands(&next) {
            next.hover = None;
        }
        // The candidate list belongs to the text being typed, so it goes when
        // the keyboard does. Left standing over another pane it claims Enter
        // and the arrows from whatever has focus — and in a hosted pane that
        // is a child holding a keyboard that answers nothing.
        // The two modals that belong to the text being typed rather than to a
        // question somebody asked: both go when the keyboard leaves the editor,
        // because left standing they claim Enter, the arrows or Tab from
        // whatever has focus — and in a hosted pane that is a child holding a
        // keyboard that answers nothing. A snippet's stops go for a second
        // reason too: they are places in one file, and a stop pointing into a
        // file nobody is looking at is the stale state this feature was warned
        // about. Here rather than in every arm that could move either, which is
        // the reason the scrolls below are clamped here.
        let elsewhere = match &next.modal {
            // Both are claims about *one file's* text, so both go when the
            // file on screen stops being that file — not only when the
            // keyboard leaves. Nothing changed the current buffer with the
            // keyboard still in the editor until the jump back did: a list
            // left standing then claimed Enter for a completion asked about
            // another document, and accepting it wrote that document's word
            // into the file just jumped to.
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
        // The comment box's body belongs to the box. With the box gone there is
        // nothing typing into it, and a body left behind would take the next
        // paste into text nobody is looking at. Here for the reason the hover
        // box above is taken down here, and bounded the same way: every event a
        // person caused reaches this, so no arm that closes the modal has to
        // remember.
        if !matches!(next.modal, Modal::Comment) {
            next.comment = None;
        }
        // The list's own window, pulled back over the choice — here for the
        // reason the two scrolls below are here: one rule rather than one per
        // arm that could move a selection.
        if let Modal::Candidates(list) = &mut next.modal {
            list.scrolled();
        }
        // Laid out once and handed to both clamps: `preview::rows` is a
        // markdown parse, and ADR 0007 is explicit that paying it twice an
        // event is visible rather than merely wasteful. Empty on every other
        // surface, which has no rows of its own to count.
        let rows = preview_rows(&next);
        // A Preview's rows reflow, so a narrower pane can leave the cursor past
        // the last row there is, or past the end of the row it is on. Both are
        // pulled back here for the reason the offsets below are: no arm has to
        // remember to do it, and a caret on a row that no longer exists is a
        // caret that is not drawn at all. Characters, not display width — a
        // column names a character of `Row::text()`, the same projection `/`
        // matches in and a drag copies out of. A Diagram, a Rule and the blank
        // row between blocks all hold no text at all, so the column lands on
        // one, which is where a caret with nothing to sit on belongs.
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
        // After the cursor, so the occurrence just taken is the one on screen;
        // the next event a person causes pulls the view back over the cursor.
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
            // Home already: nothing to pull back, and the width is a scan of
            // every line the surface holds. The same early return `ui::shift`
            // makes, for the same reason.
            Sideways::Read if next.editor_hscroll == 0 => 0,
            Sideways::Read => next
                .editor_hscroll
                .min(slid_width(&next, &rows).saturating_sub(editor_columns)),
        };
    }

    // A command waiting in the terminal needs an enter that lands in the same
    // pane, so injecting it takes focus. One rule here rather than one per
    // action; every injection reaches this point.
    //
    // Which is why the control characters go here too. A newline in an
    // injected string *is* the Enter `Effect::RunInTerminal` appends on
    // purpose, so an install command carrying one is run rather than typed —
    // the one thing ADR 0012 promises never happens, and the string comes off a
    // config layer the opened folder supplies, which AGENTS.md names a trust
    // boundary. Raw ANSI in it can rewrite the screen besides, the same reason
    // `lsp` strips a child-derived path before a notice says it. Stripped
    // rather than refused: a command with a character taken out of it is
    // visible on the input line for the reader to fix, and refusing the
    // injection leaves the row that is wrong for this machine with nothing to
    // correct.
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
    // Pushed at a shell whose prompt is waiting, never at a running job (R38.5):
    // the keyboard goes to that split, so the command and the enter it may
    // still need land in the same pane. With every split busy a new shell is
    // asked for beside the focused one and the command waits for its prompt,
    // as a review does for the AI CLI's.
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

/// [`State::traced`]: the buffer, its revision, and the trace of that text.
pub type Traced = (PathBuf, u64, std::sync::Arc<[Option<usize>]>);

/// One event a group did not claim, handed back with the state it has not
/// touched. `update`'s match ran to a hundred and forty arms; the arms are the
/// same arms, in the same order, split across groups that each take what they
/// own and pass the rest on. Order is what makes that equivalent: a group is
/// tried before every group after it, exactly as an arm was tried before every
/// arm below it.
type Declined = (State, Event);

/// What every group answers with: the event's effects, or the whole result for
/// an arm that returned early and means to skip the clamp below.
type Answered = Result<(State, Vec<Effect>), Declined>;

/// The Buffers pane's selection into its list, and its window over the
/// selection. Two callers — the clamp every event runs through, and the follow
/// in `update` that has to leave the pane settled after moving the highlight
/// itself — because a second spelling of it is a second answer to how tall the
/// pane is.
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
    // Asked before `route` consumes the event, and answered as data rather than
    // by cloning it: a clone of every event on the path of every keystroke buys
    // nothing this three-variant answer does not.
    let jump = history::jumped(state, &event);
    let (mut next, mut effects) = route(state, event);
    // The Buffers pane's highlight is the buffer you are in, until the arrows
    // move it. Here rather than in an arm because `gt`, a tree click, a search
    // hit, a close and `:ga` all change which buffer is current, and a rule
    // each of them had to remember is a rule one of them will forget — a
    // highlight disagreeing with the dot strip about which buffer is current
    // reads as a bug in both. `settle` cannot ask this: it is a comparison with
    // the state the event arrived at, which is the one thing a clamp run on
    // `next` alone does not have, so the scroll is pulled back here too.
    // The buffer *set* changing counts as well: `:ga` on a dirty current
    // buffer leaves `current_buffer` alone and takes rows out from under the
    // highlight, which moves every index below the one that was removed. It is
    // the set and the current buffer that are asked, never the selected path —
    // that is what the arrows move, so keying off it would have
    // `Event::MoveSelection` drag the highlight straight back.
    if next.current_buffer != state.current_buffer || !next.buffers.keys().eq(state.buffers.keys())
    {
        next.buffers_selection = buffer_list(&next)
            .iter()
            .position(|path| Some(path.as_path()) == next.current_buffer.as_deref())
            .unwrap_or(0);
        clamp_buffers(&mut next);
    }
    // The place the event left, if the event was a jump. Here for the reason
    // `lsp::sync` below is here: this is the one function holding both the
    // state the event arrived at and the state it produced, and an arm that has
    // to remember to record where it came from is an arm that will forget.
    // What counts as a jump is one list, in `history`, and never an arm's
    // business.
    history::record(state, &mut next, jump);
    // A Guest repo's file cannot be edited: the clone goes with the Sidecar
    // when Varde exits, so an edit there is work nobody can keep. Asked here
    // for the reason `history::record` is asked here — this is the one
    // function holding both the state the event arrived at and the state it
    // produced, and an arm that has to remember to refuse is an arm that will
    // forget. By *outcome* rather than by a list of editing keys: a list would
    // have to name every one there is — insert-mode text, `p`, a bracketed
    // paste, the word delete, whatever a later ticket adds — and the one it
    // missed would silently edit a file that is deleted at exit. A draft is
    // what an edit leaves behind and what following the file on disk does not,
    // so it is the draft that is answered and a Guest repo's buffer that
    // changed underneath still follows it.
    //
    // Ahead of `lsp::sync` on purpose: a server told about text that has been
    // put back is a server describing a buffer nobody has.
    if refuse_guest_edits(state, &mut next) {
        next.refusal = Some(preview::Refusal::GuestReadOnly);
    }
    // A Breakpoint moves with its line whatever moved the line — a key, a
    // paste, an undo, a format, the file changing on disk — so it is carried
    // here, by outcome, for the reason the guest refusal above is decided here.
    // Remembered as soon as it moves: a project whose state still named the
    // old line would find a Breakpoint that followed its line Stale next time.
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
    // Every event passes through here for the reason it passes through the
    // scroll clamp: an arm that has to remember to tell the language server
    // what the buffer now holds is an arm that will forget. What has been sent
    // is recorded per path, so an event that moved no text sends nothing.
    effects.extend(lsp::sync(&mut next));
    (next, effects)
}

/// The arms themselves, in the order they had.
fn route(state: &State, event: Event) -> (State, Vec<Effect>) {
    let mut next = state.clone();
    // A refusal answers the event that earned it and nothing after it: left
    // standing, the footer would go on explaining a key pressed five minutes
    // ago. Whichever arm below refuses sets it again.
    next.refusal = None;
    // Taken before the match consumes the event; see the scroll rule in
    // `settle`.
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
    // Every event reaches a group: the arms these groups hold are the arms
    // `update` had, in the order it had them, and they were exhaustive over
    // `Event`. Chaining is what costs the compiler's own check, so this stands
    // in for it — a variant nobody answers fails the suite here rather than
    // going quietly missing.
    unreachable!("no group answers {:?}", declined.1)
}

/// Whether a Space is the chord prefix rather than a character: normal mode,
/// nothing else collecting the keys, and code a Breakpoint could be set in —
/// never over a half-typed operator, which it would otherwise swallow.
///
/// Read off [`State::edited`], which is the buffer the keyboard is *in*: the
/// Snippet is a buffer on the same terms as the file behind it, and a chord
/// decided against the editor's mode while somebody types in the Evaluator is
/// a Space that means two things. One answer for the arm that opens the hint
/// and for [`on_snippet`], which claims the editor's keys before it and hands
/// this one back rather than spelling the question a second way.
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

/// The Evaluator's Snippet, which is a [`Buffer`]: the editor's own gestures
/// with the Snippet as their destination — one arm per gesture here and none
/// in the edge, exactly as the comment box below inherits them.
///
/// Claimed before every other reader of them, because the window has the keys
/// while the keyboard is in it and the editor answers the same events: a key
/// meant for the Snippet would otherwise edit the file the window is floating
/// over, which is the one file the reader can see it is not typing in.
fn on_snippet(mut next: State, event: Event, wheeled: bool) -> Answered {
    if next.focus != Pane::Evaluator || next.evaluator.is_none() {
        return Err((next, event));
    }
    // Normal-mode Enter runs the Snippet; inserting, it is a newline like any
    // other, because a block is written on more than one line.
    if matches!(event, Event::EditorKey('\n')) && !editor_inserting(&next) {
        let effects = debug::run(&mut next);
        return Ok(settle(next, effects, wheeled));
    }
    // The Space that opens the Chord hint is not the Snippet's: the window is
    // arranged from inside it, and the hint is how anybody finds those two
    // chords. Handed back rather than answered here, so there is one spelling
    // of what a waiting Space is.
    if matches!(event, Event::EditorKey(' ')) && opens_a_chord(&next) {
        return Err((next, event));
    }
    // Up and Down on an empty Snippet are the project's Snippets rather than
    // the caret's motion, and go on being once a recall has started. Ahead of
    // the arms below because the arrow is the editor's own event: the Snippet
    // is a buffer, and a buffer answers an arrow by moving.
    if let Event::EditorArrow(direction @ (Direction::Up | Direction::Down)) = event {
        if debug::recall(&mut next, direction) {
            return Ok(settle(next, vec![], wheeled));
        }
    }
    // Escape closes the window once there is nothing left for it to leave:
    // inserting, a Visual selection, a half-typed command and a Selection are
    // each taken back by an Escape of their own first. `:q` closes it rather
    // than the file behind it, which is not the one being typed in.
    let snippet = &next.evaluator.as_ref().expect("checked just above").snippet;
    let settled = snippet.mode == editor::Mode::Normal
        && snippet.pending_command().is_empty()
        && next.selection.is_none();
    if matches!(event, Event::CloseBuffer { .. })
        || (matches!(event, Event::EditorEscape) && settled)
    {
        let effects = debug::close_evaluator(&mut next);
        return Ok(settle(next, effects, wheeled));
    }
    if matches!(event, Event::EditorEscape) {
        next.selection = None;
    }
    let snippet = &mut next.evaluator.as_mut().expect("checked just above").snippet;
    match event {
        Event::EditorKey(key) => _ = snippet.key(key),
        Event::EditorBackspace => snippet.backspace(),
        Event::EditorDeleteWord => snippet.delete_word_back(),
        Event::EditorUndo => snippet.undo(),
        Event::EditorRedo => snippet.redo(),
        Event::EditorArrow(direction) => snippet.arrow(direction),
        Event::EditorWord(direction) => snippet.word_motion(editor::Word::toward(direction)),
        Event::EditorEscape => snippet.escape(),
        // One edit, not a run of keys, for the reason the comment box's paste
        // is one: a newline in pasted code is text and not a gesture.
        Event::EditorPaste(text) => snippet.paste(&text),
        other => return Err((next, other)),
    }
    Ok(settle(next, vec![], wheeled))
}

/// The comment box's body, which is a [`Buffer`]. The box inherits the editor's
/// gestures rather than reimplementing them on a string, so these are the
/// editor's own events with the body as their destination — one arm per gesture
/// here and none in the edge, which is what a second text editor would have
/// cost.
///
/// Claimed before every other reader of them, because the box has the keys
/// while it is up and the surfaces behind it answer the same events: a diff
/// takes `j`, `V` and `c` itself, and a walked Site takes them too. Either
/// would act on what is *under* the box instead of what is in it.
fn on_comment_body(mut next: State, event: Event, wheeled: bool) -> Answered {
    if next.comment.is_none() {
        return Err((next, event));
    }
    let body = next.comment.as_mut().expect("checked just above");
    match event {
        // Insert mode, so a key is text: `key` refuses an operator over a key
        // that names no motion, and there is no operator to be pending here.
        Event::EditorKey(key) => _ = body.key(key),
        Event::EditorBackspace => body.backspace(),
        Event::EditorDeleteWord => body.delete_word_back(),
        Event::EditorUndo => body.undo(),
        Event::EditorRedo => body.redo(),
        Event::EditorArrow(direction) => body.arrow(direction),
        Event::EditorWord(direction) => body.word_motion(editor::Word::toward(direction)),
        // One edit, not a run of keys: this is the arm that stops the first
        // newline in a pasted stack trace from being read as the gesture that
        // files the comment.
        Event::EditorPaste(text) => body.paste(&text),
        other => return Err((next, other)),
    }
    Ok(settle(next, vec![], wheeled))
}

/// The open search's query, which is a [`Buffer`] for the reason the comment
/// box's body is one: the editor's own gestures with the query as their
/// destination. The project search's first, because its box claims the keys
/// ahead of the `/` line (`keys::reserved`) and can open over it.
///
/// A key is pasted rather than typed: typing pairs brackets, and a search for
/// `foo(` would look for `foo()`. Only a change to the text searches again, so
/// moving the caret leaves the selected hit and the cursor where they are.
///
/// The in-file search's query is typed on its line or in the replace box's
/// "find" field, and the box's "with" field is a query of the same shape that
/// searches nothing.
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
        // The one character before the caret, and never its partner: the
        // editor's backspace takes an empty pair whole, which only undoes
        // help a query was never given.
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
            // A new query is a new list, as it is for `SearchQuery`.
            search.scroll = 0;
            vec![Effect::RunSearch(search.query.shown().to_string())]
        }
        // Searching from the origin every time rather than from the cursor,
        // so a longer query cannot walk you down the file one keystroke at a
        // time. Nothing matching leaves the cursor alone: a query being
        // typed is half-finished, not wrong.
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

/// route_trigger, route_key, route_submit_review, in the order their arms had.
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

/// route_step_buffer, route_accept_filter, route_editor_key, in the order their arms had.
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

/// route_ai_spoke, route_story_file_written, route_activate, in the order their arms had.
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

/// 6 of the groups, in the order their arms had.
fn route_trigger(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let declined = (next, event);
    let declined = match on_snippet(declined.0, declined.1, wheeled) {
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

/// 6 of the groups, in the order their arms had.
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

/// 6 of the groups, in the order their arms had.
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

/// 6 of the groups, in the order their arms had.
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

/// 6 of the groups, in the order their arms had.
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

/// 6 of the groups, in the order their arms had.
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

/// 6 of the groups, in the order their arms had.
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

/// 6 of the groups, in the order their arms had.
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

/// 6 of the groups, in the order their arms had.
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

/// Trigger
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
            // The row is about to stop existing, so the selection cannot stay on
            // it — it moves up to the folder that held it, which is still there.
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
                // A file holds nothing, so the name goes beside it.
                Target::File(p) => p.parent().map(Path::to_path_buf).unwrap_or_default(),
            };
            next.modal = Modal::NameBox { action, dir };
            vec![]
        }
        // Combinations the tree never produces: an action needing a target that
        // got none, or "go here" on a file.
        Event::Trigger(..) => vec![],

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// Cancel, EnterName
fn on_enter_name(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EnterName(name) => match std::mem::take(&mut next.modal) {
            Modal::NameBox { action, dir } => {
                let base = state.root.join(dir);
                let path = base.join(name);
                // The shell makes it, so the row arrives with the watcher rather
                // than with this event. Naming the path now is enough: the
                // selection is a path, so it lights up whenever the row turns
                // up — and reading the folder is what gets it watched at all.
                let folder = path.parent().unwrap_or(&base).to_path_buf();
                next.tree_selection = Some(path.clone());
                next.focus = Pane::Tree;
                vec![
                    Effect::RunInTerminal(tree_actions::create(action, &base, &path)),
                    Effect::ReadFolder(folder),
                ]
            }
            // The box on a Variables row. The text is the program's
            // language, so it is never parsed here — set as written, and
            // watched as written.
            Modal::SetValue => debug::set_value(&mut next, name),
            Modal::NewWatch => debug::add_watch(&mut next, name),
            Modal::ExceptionClass => debug::name_class(&mut next, name),
            _ => vec![],
        },

        // Leaving the box is closing it, and nothing else: a box left standing
        // with the keyboard back in the editor is one the next `K` walks
        // straight back into, and an Escape that also left a Story or dropped
        // a wait would be the box's Escape answering for what is behind it.
        Event::Cancel if state.hover.as_ref().is_some_and(|hover| hover.focused) => {
            next.hover = None;
            vec![]
        }
        Event::Cancel => {
            next.modal = Modal::None;
            next.selected_action = None;
            // Leaves the Story for the spine, opening nothing new — the same
            // outcome `EditorEscape` reaches when Story view's own focus is
            // on the editor, since the router sends whichever fits where the
            // keystroke came from. A Step's detail overlay is dismissed
            // first, on its own: Escape closing it must not also leave the
            // Story it is detailing. A Prediction is dismissed the same way:
            // Escape answers it, it does not also walk out of the Story.
            if !matches!(state.modal, Modal::StepDetail | Modal::Prediction { .. }) {
                next.walking = None;
            }
            // Cancelling a wait is a deliberate choice, unlike the CLI dying
            // out from under it, so it drops back to "nothing authored"
            // rather than to `AuthoringAbandoned` (ADR 0006, ADR 0005). Gated
            // on Story view: the Tree pane answers a bare Escape with Cancel
            // even with no modal open (stepping out of a row's actions), and
            // that Escape has nothing to do with a wait the reviewer cannot
            // even see from wherever else they are.
            // A fix round is a wait like any other, so it is left the same
            // way: there is one way out, not two.
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

/// FallbackBinding, Tapped
fn on_tapped(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Tapped { key, at_ms } => {
            // Exactly one of the two is available on any given terminal: the
            // bare Ctrl press where the terminal reports one, and Escape where
            // it does not — which is the same terminal where Option is not Alt,
            // so it is the only gesture a hosted pane has left there. Arming
            // Escape everywhere would take the key *after* a double-tap from a
            // child that binds Esc Esc itself, on terminals that already have a
            // gesture costing the child nothing at all.
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

        // Works everywhere, not only where the double-tap is unavailable (Q36):
        // one binding that is the same on every terminal, and a way through if
        // the double-tap misfires.
        Event::FallbackBinding => {
            next.modal = Modal::Palette;
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// Key
fn on_key(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // The spine and the changed-files list share the tree pane, so this
        // has to fire regardless of which pane has focus — Story view's own
        // focus stays on the editor.
        Event::Key('t') if state.view == View::Story && state.modal == Modal::None => {
            next.story_listing = match state.story_listing {
                story::Listing::Spine => story::Listing::Files,
                story::Listing::Files => story::Listing::Spine,
            };
            vec![]
        }

        // Walking a Story's own keys, reachable the same way `t` above is —
        // regardless of pane, since Story view's focus stays on the editor.
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

/// Key
fn on_key_2(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        // `n`/`p` also fire with a Prediction open, since stepping on must
        // dismiss it rather than being swallowed by it — answering never
        // blocks the walk.
        Event::Key(key @ ('n' | 'p'))
            if state.view == View::Story
                && state.walking.is_some()
                && matches!(state.modal, Modal::None | Modal::Prediction { .. }) =>
        {
            Ok(walk_key(state, next, key))
        }

        // `D` alone answers with the detail overlay already open, since that
        // is the one walking key that must also close what it opened.
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

/// Key
fn on_key_3(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        // The Risk pane's own keys. `j`/`k` because the pane is a list and vim's
        // motions are what a list answers to without a modifier; `a` because
        // `ToggleRiskAll` arrived with the pane and was reachable from nothing,
        // and the pane's own keys are the only place a toggle about this list
        // belongs. Not in `CHEATSHEET`: that box's contract is the sweep over a
        // focused editor, which is why the tree's own `n`/`N`/`d`/`-` are not in
        // it either — a row claiming `j` in Edit view would claim the cursor
        // motion the omissions list already excuses there.
        //
        // `r` and `l` reach the pane's own two actions, which the mouse reaches
        // by clicking their icons on the border. A gesture only the mouse has
        // is a gesture half the users do not have.
        Event::Key(key @ ('j' | 'k' | 'a' | 'r' | 'l'))
            if state.focus == Pane::Risk && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                'k' => update(state, Event::MoveSelection(Direction::Up)),
                'a' => update(state, Event::ToggleRiskAll),
                'r' => update(state, Event::PaneAction(risk::RECOMPUTE)),
                // Whichever of start and stop the loop's slot is holding — by
                // name, from the same function the icon is drawn from, so the
                // key cannot start a loop while the icon beside it says stop.
                _ => update(state, Event::PaneAction(risk::loop_action(state))),
            })
        }

        // The Cursor history pane's own keys, for parity with the two lists it
        // shares the corner with: `j` and `k` because the pane is a list. Not
        // `p` and `n` — those are the jump itself, which answers in every pane
        // rather than only in this one, and a letter meaning one thing here and
        // another everywhere else is the drift the corner's panes exist without.
        Event::Key(key @ ('j' | 'k'))
            if state.focus == Pane::History && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                _ => update(state, Event::MoveSelection(Direction::Up)),
            })
        }

        // The Buffers pane's own keys, for parity with the list above it in the
        // same corner: `j` and `k` because the pane is a list and vim's motions
        // are what a list answers to without a modifier. Two and no more — the
        // pane has no actions, so there is nothing else a letter could reach.
        // Not in `CHEATSHEET`, for the reason the Risk list's are not.
        Event::Key(key @ ('j' | 'k'))
            if state.focus == Pane::Buffers && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                _ => update(state, Event::MoveSelection(Direction::Up)),
            })
        }

        // Space is the chord prefix in Varde's own Debug panes too, not only
        // on the code: stepping happens in bursts from wherever the keyboard
        // is, and a modifier-free alias that works in one pane is the failure
        // the F-keys' reserved-everywhere rule exists to prevent. The editor's
        // own arm is below, where it has a buffer's mode to answer for.
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

        // The Diagnostic list's `j` and `k`, and a Severity's letter to show
        // that Severity — the key its Severity label is shed to on a narrow
        // Corner.
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

        // The Frames' and the Conflict list's `j` and `k`, as every list in
        // the corner has.
        Event::Key(key @ ('j' | 'k'))
            if matches!(state.focus, Pane::Frames | Pane::Conflicts)
                && state.modal == Modal::None =>
        {
            Ok(match key {
                'j' => update(state, Event::MoveSelection(Direction::Down)),
                _ => update(state, Event::MoveSelection(Direction::Up)),
            })
        }

        // The Variables' own keys: `j` and `k` as every list has, and a
        // letter per row Chip — the Breakpoint list's shape, one pane over.
        // `Y` is the one action with no Chip of its own: the spec draws five
        // Chips on a row and copying as an expression is the sixth action, so
        // it rides the shifted copy key rather than a Chip the row has no
        // room for. Not in `CHEATSHEET`, for the reason the Breakpoint
        // list's are not — they answer only while this pane holds the
        // keyboard, which is where its Chips are on screen naming them.
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
                // The one that needs no row: a Watch typed from scratch is
                // not about whatever the keyboard happens to be standing on.
                _ => {
                    let mut opened = next;
                    opened.modal = Modal::NewWatch;
                    (opened, vec![])
                }
            })
        }

        // The Breakpoint list's own keys: `j` and `k` as every list in the
        // corner has, `e` and `d` for its row's Chips, and `D` and `x` for the
        // Transport's — vim's delete, and its shifted letter for the whole of
        // it; `x` for the exception. Not in `CHEATSHEET`, for the reason the
        // Risk list's are not; the Transport's Chips name them.
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

/// Key
fn on_key_4(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        // Tree shortcuts, so navigating by keyboard is not a dead end.
        Event::Key(key) if state.focus == Pane::Tree && state.modal == Modal::None => {
            // Two of them need no row, because they are about the tree rather
            // than a path in it: `-` returns the terminal to the workspace
            // root, `c` closes every folder the tree has open.
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

/// Key
fn on_key_5(state: &State, mut next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        // Whatever the second key is, the hint goes: a chord nobody bound
        // costs the one key, the way Escape does.
        Event::Key(key) if state.modal == Modal::Chord => {
            next.modal = Modal::None;
            let Some(chord) = keys::chord(state, key) else {
                return Ok((next, vec![]));
            };
            // The chord done, the keyboard is left in Stepping mode, where the
            // stepping letters act without their Space: stepping happens in
            // bursts, and a Space per step is a Space too many. The session is
            // what bounds it — see `State::stepping` — so `q`, which is the
            // chord that ends one, is the chord that does not leave it on:
            // between the request and the adapter letting go, the mode would
            // be four letters swallowed for a session on its way out.
            // Not for a chord that opens a mode of its own: `␣m` and
            // `␣z` put the keyboard in the Evaluator's arrange mode, and two
            // modes claiming the same letters is the trap both are shaped to
            // avoid.
            next.stepping = state.debug.is_some() && !matches!(key, 'q' | 'm' | 'z');
            Ok(update(&next, chord))
        }
        Event::Key(key) if state.modal == Modal::Palette => {
            let Some(entry) = palette_entry(key) else {
                // Unrecognised keys leave the palette open.
                return Ok((next, vec![]));
            };
            // The one entry that opens a face of the palette rather than
            // closing it. Nothing is probed from here: whether each command
            // exists is the edge's answer to the list being open (R31.23).
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
            // Focus moves before the view does, because switching to the view
            // already showing is a no-op: without this the palette is a dead
            // end for whoever opened it from a hosted pane, which is the one
            // place it has to be a way out.
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

/// Palette entries that stand for an event of their own.
fn palette_command(next: State, entry: &str) -> Result<(State, Vec<Effect>), State> {
    let event = match entry {
        // Just go there. With nothing running the pane asks which CLI it
        // should start.
        "Find" => Event::OpenSearch,
        // The shape of the AI pane is wanted while reading the AI pane, and a
        // hosted pane's child owns the colon — so `:tall` alone meant leaving
        // the pane to change it. Focus is left exactly where it was: you were
        // reading, and you still are.
        "Tall" => Event::ToggleTallAi,
        // Un-indented, because this is a pane rather than another pane's
        // shape. It costs no key binding: focus is directional, so the pane is
        // reachable by geometry once it is on screen.
        "Risk" => Event::ToggleRiskList,
        "Buffers" => Event::ToggleBuffersList,
        "Cursor history" => Event::ToggleCursorHistory,
        "Breakpoints" => Event::ToggleBreakpointList,
        "Diagnostics" => Event::ToggleDiagnosticList,
        "Merge conflicts" => Event::ToggleConflictList,
        // The tree's own `c` reaches this too, but only from the tree: the
        // palette is how it is reached from wherever the growing tree was
        // noticed, which is usually the pane being read rather than the tree.
        "Collapse" => Event::CollapseTree,
        // These two have to be here rather than only on the `:` line. The key
        // box no longer advertises either: it is the reminder for the keys a
        // view answers to, and a box already down would hide the way to put it
        // back.
        "Keys" => Event::ToggleCheatsheet,
        "Update" => Event::Rebuild,
        "Quit" => Event::Quit,
        _ => return Err(next),
    };
    Ok(update(&next, event))
}

/// Palette entries that only move focus. The tree is the one pane no entry
/// reached: Review focuses it but changes the view to get there, so in Edit it
/// was behind `M-h` alone — a modifier macOS need not send at all. Every pane
/// is reachable by the one gesture now. `Editor` is the pane, not the view:
/// `Edit` would take a reviewer reading a diff out of Review to reach the same
/// rectangle.
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

/// FilesAppeared, Key
fn on_key_6(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Key(_) => vec![],

        // The tree follows these; the editor never does. Files arriving from the
        // AI, a checkout or a build must not take over the editor, which is why
        // no arm here can emit OpenBuffer.
        Event::FilesAppeared(appeared) => {
            // The watcher that already exists is what sees a pass reported
            // finished (ADR 0010): a sentinel is a file, and a file is
            // something the edge may observe without reading a pane.
            // The whole path, not its last component: `SENTINEL` names the
            // file inside `varde_dir`, and matching on the name alone would
            // let a `refactor-done` anywhere in the watched tree complete an
            // Iteration and run the Gate.
            let sentinel = varde_dir(&next.root, next.sidecar.as_deref()).join(risk::SENTINEL);
            let reported = appeared.iter().any(|(path, _)| path == &sentinel);
            // The download's end, seen by the same watcher and for the same
            // reason (ADR 0015) — but the status is the sentinel's contents,
            // so it is read rather than inferred from the file being there.
            // Only while one is in flight: a `download-done` left in a
            // Sidecar is not a download this Varde asked for.
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
            // A taken row's install, by the same watcher and the same rule:
            // only while one is running.
            let install = varde_dir(&next.root, next.sidecar.as_deref()).join(tools::SENTINEL);
            let read_the_install = (next.installing.is_some()
                && appeared.iter().any(|(path, _)| path == &install))
            .then_some(Effect::ReadInstallStatus(install));
            for (path, kind) in appeared {
                let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
                    continue;
                };
                // Unknown parent means the folder was never expanded, so there is
                // nothing to add to and nothing being watched.
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
            // Both, never one instead of the other: a batch can hold a pass
            // reported and a clone finished, and an early return past either
            // is the wait nobody can end that this sentinel exists to avoid.
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

/// Expand, CollapseTree, FilesRemoved
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

        // `contents` is deliberately untouched: the tree's shape is
        // `expanded`'s alone, since `tree::rows` walks only into what it
        // names, so a folder's read entries are not part of what is on screen.
        // `tree_selection` is untouched too, and nothing here reconciles it —
        // `settle`'s clamp bounds the *scroll*, reading an off-screen selection
        // as row 0 and so pulling the tree back to the rows that are left, and
        // `MoveSelection` lands the selection itself on the next keypress. A
        // rule of its own here would be a second answer to what an off-screen
        // selection means, which the filter already asks.
        Event::CollapseTree => {
            next.expanded.clear();
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// FileChanged
fn on_file_changed(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::FileChanged { path, contents } => {
            let relative = path
                .strip_prefix(&state.root)
                .ok()
                .map(|path| path.to_string_lossy().into_owned());
            // R5.5: the review list follows git while you are reviewing.
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
                // Unsaved edits are never overwritten — flag and wait. A clean
                // buffer has nothing to lose, so it just follows the file.
                buffer.follow(contents);
                // The ⚠ in the title is a standing marker, easy to miss on a
                // screen an AI is writing to. Losing a draft is worth a word
                // on the status line the moment it is at risk.
                if buffer.changed_on_disk {
                    effects.push(Effect::Notify("buffer-diverged"));
                }
            }
            // A diff is a view of the file, so it follows the file too. Only
            // this file: a change elsewhere must not disturb the diff on screen.
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

/// AddComment, FileComment, DragGutter, OpenReviewView, Reload
fn on_reload(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Reload(path) => {
            if let Some(buffer) = next.buffers.get_mut(&path) {
                buffer.reload();
            }
            vec![Effect::ClearNotice]
        }

        // You switched here to look at changes, so land on one: first file
        // selected, its diff loading, keyboard on the list.
        Event::OpenReviewView => {
            move_to_view(&mut next, View::Review);
            next.focus = Pane::Tree;
            // The Scope on screen is now the files under review, so the figure
            // on the border has to be theirs: it is measured on the way in,
            // against the revision the diff is measured from. Whatever was
            // measured before describes another Scope, and a stale delta is the
            // wrong number in the wrong place — so it is asked for again rather
            // than shown while it lasts.
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
            // The mouse equivalent of V then c, so it opens the same picker.
            open_comment_box(&mut next, file, from_line, to_line);
            vec![]
        }

        Event::FileComment { kind } => {
            // Closing the picker is how the reviewer knows it worked. Without
            // this the box stays up and the filing gesture looks like nothing.
            next.modal = Modal::None;
            let body = next.comment.take().map_or(String::new(), |body| {
                // What was typed, newlines included: the body is a Buffer and a
                // Buffer's text is what it shows.
                body.shown().to_string()
            });
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

/// ClickPane, ClickThrough, ConfirmSubmit, SubmitReview
fn on_submit_review(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Submitting must mean the AI is working, so an empty review is refused
        // rather than confirmed.
        Event::SubmitReview if state.comments.is_empty() => vec![Effect::Notify("review-empty")],

        Event::SubmitReview => {
            next.modal = Modal::ConfirmSubmit;
            vec![]
        }

        Event::ConfirmSubmit => {
            next.modal = Modal::None;
            submit(&mut next)
        }

        // A click focuses the pane and does what you clicked — no dead first click.
        // It also drops what was picked, the same way a click in the text does:
        // a pty selection is drawn, so one left behind is a highlight over
        // output nobody selected. A drag's own button press comes first, so the
        // drags that follow it still land.
        Event::ClickPane(pane) => {
            next.focus = pane;
            next.selection = None;
            vec![]
        }

        // The release of a click that did not turn into a drag. A pty pane whose
        // child asked for mouse events gets it — which is how an AI CLI's own
        // buttons work, and the AI pane went without one for exactly as long as
        // it sat in a wildcard arm. Exhaustive so a fifth pane is a decision.
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

/// ClickRow
fn on_click_row(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::ClickRow(path) => {
            next.focus = Pane::Tree;
            next.tree_selection = Some(path.clone());
            // In Review view the left pane lists changed files, not the tree:
            // clicking one shows its diff and hands it the keyboard. The
            // exception to R11.4's "a click keeps focus where you clicked" is
            // deliberate: the list's whole job is choosing the file, and what
            // the reviewer does next — V to select lines, c to comment, e to
            // edit — are the editor's keys. With focus left on the list, `c`
            // was a tree shortcut and the picker could not be reached at all.
            if state.view == View::Review {
                next.focus = Pane::Editor;
                return Ok((next, vec![Effect::ReadDiff(path)]));
            }
            let is_folder = next.contents.contains_key(&path) || is_known_folder(state, &path);
            if !is_folder {
                // Clicking focuses what you clicked (R10.1); Enter is the
                // deliberate "open this" that follows into the editor.
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

/// Where one notch of the wheel puts a list's first visible row.
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

/// Where the editor's window goes, and the caret with it when the window would
/// leave it behind, as vim's Ctrl+E does: a cursor off screen is one you have
/// lost, and the next thing you type lands out of sight.
///
/// `offset` is asked for the new first row and handed how many rows the
/// surface has, so the one call to `editor_focus` — and the markdown parse
/// behind it — serves both the caller's arithmetic and the caret's. The two
/// callers are the wheel and a drag in the minimap, which are one gesture with
/// two spellings: a view moved away from the cursor.
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
                // Back into lines: the row it landed on is the row, and the
                // cursor lives on a line.
                buffer.go_to_place(Place {
                    line: story::line_at_row(state, landed),
                    column: 1,
                });
            }
        }
        _ => next.diff_line = landed.min(rows.max(1)),
    }
}

/// Where the editor's sideways offset goes, and the caret with it on a surface
/// that has one the offset follows — the sideways half of [`travel_editor`], and
/// the same reason: a caret off the edge of the screen is a caret you have lost,
/// and the next thing you type lands out of sight.
///
/// Bounded by the widest line the whole surface holds rather than by the rows in
/// view, exactly as [`slid_width`] bounds the keyboard slide, so a swipe past the
/// end stops instead of running on into empty space.
fn slide_editor(state: &State, next: &mut State, direction: Direction) {
    let columns = fits(state).2;
    // The one parse, handed to both the width and the cursor — the reason
    // `sideways` and `slid_width` are given rows rather than taking them.
    let rows = preview_rows(state);
    next.editor_hscroll = match direction {
        Direction::Left => state.editor_hscroll.saturating_sub(SLIDE_COLUMNS),
        Direction::Right => (state.editor_hscroll + SLIDE_COLUMNS)
            .min(slid_width(state, &rows).saturating_sub(columns)),
        // The caller settles the axis, and reaches this function only for a
        // sideways wheel. Spelled out rather than swallowed by a `_`: the
        // catch-all is the exact shape this issue existed to remove from the
        // edge, and a `Direction` that grows a variant should fail the build
        // here rather than quietly slide right.
        Direction::Up | Direction::Down => return,
    };
    let Sideways::Cursor { column, .. } = sideways(state, &rows) else {
        return;
    };
    // Onto the nearest column the new window shows: the clamp in `settle` reads
    // this column back on the very next event, so a caret left outside the
    // window would pull the swipe straight home again. Nothing keeps it on the
    // text here — `go_to_place` and `settle` each clamp their own surface's
    // cursor to the row it is on, and a second clamp would be a third opinion.
    let last = next.editor_hscroll + columns.max(1) - 1;
    let landed = column.clamp(next.editor_hscroll, last) + 1;
    let Some(path) = next.current_buffer.clone() else {
        return;
    };
    let Some(buffer) = next.buffers.get_mut(&path) else {
        return;
    };
    match buffer.previewing {
        // A Preview's cursor is a column of the *rendered* row (ADR 0007, as
        // amended), which is the column `sideways` reported and the one the
        // offset is measured against.
        true => buffer.row_column = landed,
        false => buffer.go_to_place(Place {
            line: buffer.line,
            column: landed,
        }),
    }
}

/// DragMinimap, Scroll, ScrollCheatsheet, ScrollHover
fn on_scroll(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Scrolling follows the pointer and never moves focus. The pty panes
        // keep a history only the pty can supply, so the edge scrolls those;
        // the tree and the editor are lists whose length this crate knows.
        Event::Scroll {
            pane,
            direction,
            at,
        } => {
            // Sideways is its own axis, and only two kinds of surface have an
            // offset on it: the editor pane, and a child that asked for mouse
            // events. Ahead of everything below because the lists there have no
            // sideways offset at all — a swipe reaching `wheeled_to` would take
            // its catch-all arm and scroll the tree *down*, which is a gesture
            // answering with a different gesture.
            if matches!(direction, Direction::Left | Direction::Right) {
                // The results box is covering the panes, and has nothing
                // sideways of its own — the same reason it claims the vertical
                // wheel rather than letting it through.
                if next.search.is_some() {
                    return Ok(settle(next, vec![], wheeled));
                }
                let effects = match pane {
                    Pane::Editor => {
                        slide_editor(state, &mut next, direction);
                        vec![]
                    }
                    // The same rule the vertical wheel follows, one axis over.
                    // No `Effect::Scrolled` when the child cannot be told: a
                    // pty's scrollback is rows, so there is no sideways history
                    // for the edge to answer with.
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
            // The results box covers every pane, so while it is up the wheel is
            // its own however the pointer got there — and the pane underneath
            // must not move, because a wheel over a modal that scrolls what it
            // is covering is a view you cannot get back.
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
                // The window is resized rather than scrolled: no scenario
                // asks the Snippet or its output to hold more than the room
                // they are given, and an offset nothing clamps is an offset
                // that strands the last rows. Nothing here rather than the
                // editor's scroll, which is a different pane's offset and
                // would move the code behind the window.
                Pane::Evaluator => vec![],
                // Never reported to the session behind it. `settle` holds
                // the offset inside the rows there are.
                Pane::Cheatsheet => {
                    next.cheatsheet_scroll = match direction {
                        Direction::Up => state.cheatsheet_scroll.saturating_sub(WHEEL_ROWS),
                        _ => state.cheatsheet_scroll + WHEEL_ROWS,
                    };
                    vec![]
                }
                // A program that asked for mouse events scrolls itself; ours is
                // the scrollback behind a shell that did not.
                Pane::Terminal | Pane::Ai | Pane::Output => match mouse::report(
                    mouse_encoding(state, pane),
                    mouse::Gesture::Wheel(direction),
                    at,
                ) {
                    Some(bytes) => vec![Effect::SendKeys { pane, bytes }],
                    // The wheel is the child's only while the child can be told
                    // about it — it asked for no mouse, or the legacy encoding
                    // has no byte for that cell. Otherwise it stays ours.
                    None => vec![Effect::Scrolled(pane, direction)],
                },
            }
        }

        // The row the pointer is holding is where the slider goes, which makes
        // a press and a drag one rule rather than a jump and a scroll with
        // different arithmetic — a mirror you hold is a place you are looking
        // at, not a place you are scrolling past.
        //
        // Not an exception to the clamp, unlike the wheel: the caret comes
        // along, so what `settle` pulls the view back to is where the drag
        // left it.
        Event::DragMinimap(row) => {
            let fits = fits(state).1;
            travel_editor(state, &mut next, fits, |_| {
                minimap::travel(row as usize, minimap::lines(state), fits)
            });
            vec![]
        }

        // Held inside the rows there are by `settle`, which owns the bound.
        Event::ScrollCheatsheet { direction, rows } => {
            next.cheatsheet_scroll = match direction {
                Direction::Up => state.cheatsheet_scroll.saturating_sub(rows),
                _ => state.cheatsheet_scroll + rows,
            };
            vec![]
        }

        // Bounded by the rows the box has on screen, which a box taller than
        // the pane has fewer of than it has lines — so the last line can come
        // up to the bottom border and no further.
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

/// Copy, DragAiDivider, DragDivider, DragStrip, Resized, RightClick
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

        // No context menus: every action lives in the toolbar and the keyboard.
        Event::RightClick(_) => vec![],

        Event::DragDivider(column) => {
            next.tree_divider = column;
            vec![Effect::SaveState(state_json(&next))]
        }

        // Remembered as a width off the Debug group's right-hand end rather
        // than as a column: the Strip's left edge moves with the Corner beside
        // it, and a column would put the border somewhere else every time the
        // Corner's occupant changed.
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
            // A stray press must leave the clipboard as it was.
            None => vec![],
        },

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// EditorArrow, EditorExtend, EditorExtendWord
fn on_editor_arrow(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // A diff is read-only: it moves and selects, and `e` leaves it for the
        // real file, but it never edits.
        Event::EditorArrow(direction) if state.diff.is_some() => {
            let length = state.diff.as_ref().map_or(1, Vec::len).max(1);
            match direction {
                Direction::Down => next.diff_line = (state.diff_line + 1).min(length),
                Direction::Up => next.diff_line = state.diff_line.saturating_sub(1).max(1),
                _ => {}
            }
            vec![]
        }

        // Read-only is a promise about the buffer's contents, not about the
        // cursor. Extending a selection still answers nothing — `S-arr` is an
        // Edit-view binding — but a plain arrow falls through to the ordinary
        // motion below, because a Site can be taller than the pane and a
        // reviewer who cannot move through it cannot read it. Nothing is lost
        // by moving: the mark is drawn from where the Walkthrough stands, not
        // from where the cursor is, so reading around a Site never costs the
        // reviewer the claim's place.
        Event::EditorExtend(_) | Event::EditorExtendWord(_) if state.walking.is_some() => {
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// BufferOpened
fn on_buffer_opened(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::BufferOpened {
            path,
            contents,
            preview,
            at,
        } => {
            // A preview occupies one slot: the previous preview goes, unless it
            // was opened properly in the meantime.
            let was_preview = next.preview.take();
            let already_open = next.buffers.contains_key(&path);
            if let Some(previous) = was_preview.clone() {
                if previous != path {
                    next.buffers.remove(&previous);
                }
            }
            // A file that is already open properly stays that way — selecting
            // it in the tree switches to it rather than re-previewing it.
            let permanent = already_open && was_preview.as_deref() != Some(path.as_path());
            next.preview = (preview && !permanent).then(|| path.clone());
            // `or_insert_with`, so the reader's own choice survives the file
            // being opened a second time: the default follows the file only
            // when there is no buffer to have chosen anything yet.
            let tab_width = next.tab_width;
            let buffer = next
                .buffers
                .entry(path.clone())
                .or_insert_with(|| Buffer::open(&contents, preview::is_markdown(&path), tab_width));
            // The edge read the file just now; the watcher is not the only
            // way a buffer hears the disk moved, and one change it missed
            // left a jump marking the text as it was before.
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
            // The rest of the landing, when the opening had a place to land on:
            // the same two lines `Event::JumpTo` is, run here rather than in a
            // second event for the reason [`Event::BufferOpened`] carries the
            // place at all.
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

/// Indexed, OpenSearch, ShowBuffer, StepBuffer
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

        Event::Indexed(files) => {
            next.indexed = files;
            vec![]
        }

        // On what the editor has selected, as `gr` opens it; a pick in a hosted
        // pane is the child's text, not something to look for in the project.
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

/// CloseSearch, OpenFind, SearchSelection, SearchWordUnderCursor
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

        // The mouse selection if there is one, so selecting a name and pressing
        // gr looks it up; otherwise the word you are sitting in.
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

        // With a search already on, `/` goes back into its query with the
        // text intact, and Escape from there still restores where the search
        // started.
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

/// AcceptFind, CloseFind, FindKeys, ReplaceAll, ReplaceMatch, ToggleCase
fn on_find_query(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Abandoning the search takes the highlights with it: they are what the
        // query is showing, and the query is gone.
        Event::CloseFind => {
            let Some(find) = next.find.take() else {
                return Ok((next, vec![]));
            };
            go_to_match(&mut next, find.origin);
            vec![]
        }

        // The match becomes the selection, which is what lets it be copied or
        // handed to project search without being retyped. Found again rather
        // than remembered: the query already says where it is, so there is no
        // second copy to keep true. The search stays on — only the keyboard
        // leaves it.
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
            // The replace box is over the editor, so it takes the keyboard
            // there from wherever the icon was clicked.
            if matches!(keys, FindKeys::Replace(_)) {
                next.focus = Pane::Editor;
            }
            vec![]
        }

        // Flipping what is *shown*: a smart-case query holding a capital is lit,
        // so pressing it turns it off rather than on.
        Event::ToggleCase => {
            if let Some(find) = next.find.as_mut() {
                find.case = match find.case.exact(find.query.shown()) {
                    true => search::Case::Ignore,
                    false => search::Case::Exact,
                };
            }
            vec![]
        }

        // Not while previewing: a match there is a rendered row and column,
        // not a place in the text the replacement would go into.
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

        // One `replace_at`, so one undo puts every match back. The box closes
        // and the search stays on, now most likely matching nothing.
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

/// StepMatch
fn on_step_match(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Wrapping at both ends, because stopping at the last match reads as
        // "there are no more" when there are. With nothing found the cursor
        // stays where it is: there is nowhere to go.
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

/// MoveHit, MoveHitFile, OpenEveryHit, OpenHit, SearchQuery, Searched, SelectHit
fn on_search_query(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::SearchQuery(query) => {
            let search = next.search.get_or_insert_with(Search::default);
            search.query = Buffer::text_box(&query);
            search.selected = 0;
            // A new query is a new list: the clamp only ever scrolls the
            // selection into view by the least it can, so without this the box
            // opens the next answer wherever the last one was left.
            search.scroll = 0;
            vec![Effect::RunSearch(query)]
        }

        Event::Searched(results) => {
            if let Some(search) = next.search.as_mut() {
                search.selected = search.selected.min(results.hits.len().saturating_sub(1));
                search.results = results;
            }
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
            // The match is left selected, so opening a hit and finding one in
            // place leave the workspace in the same shape. Matching is literal,
            // so the match is exactly as long as the query. The buffer is not
            // open yet — the cursor follows on the place the opening carries
            // back (R34.3b), which the selection outlives because it holds
            // places and not text.
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

/// CompleteSearch, Filter, JumpTo
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

        // A filter walks the project afresh rather than once ever. The index
        // used to be built only when it was empty, so a filter opened in a long
        // session searched the project as it stood hours earlier: a file
        // created since was not in it, nothing matched what was typed, and
        // Enter opened whichever stale path fuzzily matched instead. Walking on
        // the first character is one walk per filter, not one per keystroke.
        //
        // Nothing is expanded here. `tree::filtered` already draws the folders
        // leading to a match as open, and writing that into `expanded` as well
        // left every folder every match sat under standing open once the filter
        // was cleared — a filter is a way of looking at the tree, not a change
        // to it.
        Event::Filter(text) => {
            let opening = state.filter.is_empty() && !text.is_empty();
            next.filter = text;
            if opening {
                vec![Effect::IndexProject]
            } else {
                vec![]
            }
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// AcceptFilter, EditorArrow, EditorBackspace, EditorDeleteWord
fn on_accept_filter(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Enter marks the match in the tree and opens nothing: narrowing the
        // tree is looking for a file, and what to do with the file once it is
        // found is the reader's to say. The filter clears either way — closing
        // the box over a filter that matched nothing left the pane empty with
        // no box and no way to clear it.
        Event::AcceptFilter => {
            let best = filter::best(&next);
            next.filter.clear();
            match best {
                Some(relative) => {
                    next.tree_selection = Some(state.root.join(&relative));
                    // A mark you cannot see has not helped you, and the tree is
                    // lazy: the folders leading to the match are read, and their
                    // replies are what expand them. Only this one path, and
                    // only on accept.
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

        // Read-only means the contents: a diff and a walked Site both refuse
        // this. Walking was missing here, and only the corner of the buffer
        // hid it — at line 1 column 1 a backspace does nothing anyway.
        Event::EditorBackspace => {
            if state.diff.is_none() && state.walking.is_none() {
                // Picked characters are what goes, as typing over them
                // replaces them: the selection names the text, not the
                // character behind the cursor. Insert mode only, for the
                // reason the typing-over arm is.
                let picked = match editor_inserting(state) {
                    true => state.selection.as_ref().and_then(Selection::buffer_span),
                    false => None,
                };
                if picked.is_some() {
                    next.selection = None;
                }
                if let Some(buffer) = current(&mut next) {
                    match picked {
                        Some((from, to)) => buffer.delete_in(from, to),
                        None => buffer.backspace(),
                    }
                }
            }
            // Deleting inside a word is still typing it: the list a longer
            // prefix earned would otherwise stand over what is left.
            match editor_inserting(state) {
                true => lsp::typed(&mut next),
                false => vec![],
            }
        }

        // Alt+Backspace. The same guard the arm above carries, and for the same
        // reason: read-only means the contents, and a diff or a walked Site
        // answers the editor's keys ahead of the buffer. The router will not
        // send this there — but where a key goes is `update`'s to decide, and
        // the arm above records what leaving one surface out of it already
        // cost. `lsp::typed` for the reason a backspace asks: what is behind
        // the cursor decided the candidate list, and a word delete changed it.
        Event::EditorDeleteWord => {
            if state.diff.is_none() && state.walking.is_none() {
                if let Some(buffer) = current(&mut next) {
                    buffer.delete_word_back();
                }
            }
            lsp::typed(&mut next)
        }

        // The arrows and the word-motion arrows reach the same cursor the
        // letters do — a Preview is for reading, and the arrows are what a
        // reader who has not learned vim reaches for. Rewritten into the key
        // rather than given a Preview arm of their own: one motion table, so
        // Right and `l` cannot come to disagree about where they land.
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

        // Shift-extending picks what a drag picks: a span of the **rendered
        // rows**, never one of the source lines a `Selection::Buffer` names
        // everywhere else in this crate. Rewritten into the motion key and
        // then into `Event::DragText`, so the row the cursor lands on and the
        // text the span resolves to each have one implementation — a second
        // one here is how a keyboard selection and a dragged one come to
        // disagree about what is picked.
        //
        // The anchor is the end of the held span the cursor was sitting on,
        // which is what lets a second press extend the first rather than
        // starting again: `Selection::Screen` carries the span in reading
        // order, so which end is the anchor is read off where the cursor was.
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

/// EditorArrow, EditorExtend, EditorWord
fn on_editor_arrow_2(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::EditorArrow(direction) => {
            next.selection = None;
            next.occurrences.clear();
            if let Some(buffer) = current(&mut next) {
                buffer.arrow(direction);
            }
            vec![]
        }

        // The word-motion arrows, which move while inserting as well: a motion
        // is not the key that spells it, and `w` typed into a line is a `w`.
        Event::EditorWord(direction) => {
            next.selection = None;
            next.occurrences.clear();
            if let Some(buffer) = current(&mut next) {
                buffer.word_motion(editor::Word::toward(direction));
            }
            vec![]
        }

        // The anchor is wherever the cursor already was, so extending needs no
        // mode key first — and a plain arrow above leaves the mode by clearing.
        Event::EditorExtend(direction) => {
            let held = match state.selection {
                Some(Selection::Buffer { anchor, .. }) => Some(anchor),
                _ => None,
            };
            if let Some(buffer) = current(&mut next) {
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

/// EditorExtendWord, EditorNextOccurrence
fn on_editor_extend_word(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // A word extension is a charwise one with a longer step: `e` takes the
        // rest of the word the cursor is in, so one press picks an identifier.
        Event::EditorExtendWord(direction) => {
            let held = match state.selection {
                Some(Selection::Buffer { anchor, .. }) => Some(anchor),
                _ => None,
            };
            if let Some(buffer) = current(&mut next) {
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

        // Refused in a Preview even though extending is not: a Preview's span
        // is a span of rendered rows, and occurrences would be the source's,
        // so the word drawn as picked is not the word the next keystroke would
        // land on. Read-only is the reason said out loud, because taking
        // occurrences is the opening of an edit and a Preview takes none.
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

/// The next occurrence of what is picked, added to the ones already taken —
/// and with nothing picked, the word under the cursor, which is what makes the
/// gesture reachable while inserting: there is no key to press first there
/// that would not type a letter.
///
/// The search runs from the *first* occurrence and skips the ones already
/// held, so a run of presses walks the file downwards and then continues from
/// the top: an occurrence above the one it started from is one nobody would
/// find by pressing again, and stopping at the bottom of the file is how a
/// rename comes out half done.
///
/// A span with a line break in it is refused rather than searched for: a run
/// of lines is not a word, and every occurrence of one is a different gesture.
fn take_next_occurrence(next: &mut State) {
    let span = next.selection.as_ref().and_then(Selection::buffer_span);
    let Some(buffer) = current(next) else {
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
        next.revealing = Some(*found);
    }
}

/// Preview to Source: the current row's source line, taken straight off the
/// row still on screen rather than re-derived.
fn cross_to_source(state: &State, next: &mut State) {
    // A crossing starts at column one, in both directions. The two shapes
    // measure a column differently — a Preview row is not a line (ADR 0007) —
    // so an offset taken while reading a fence would survive the toggle as a
    // sideways jump nobody asked for, and one taken in Source would leave a
    // Preview scrolled past its own left edge.
    next.editor_hscroll = 0;
    // A selection belongs to the surface it was picked on: a Preview's span
    // names rendered rows and Source's names lines, so one carried across the
    // crossing is a highlight over characters nobody picked — the same reason
    // `settle` takes a linewise span down when the rows appear.
    next.selection = None;
    let row = current_buffer(state).map_or(1, |buffer| buffer.row);
    let line = source_line(state, row);
    if let Some(buffer) = current(next) {
        buffer.previewing = false;
        buffer.line = line;
        buffer.column = 1;
    }
}

/// The source line a rendered row came from — the row map read, never a line
/// derived a second time. Two callers, and the second is why it is a function:
/// the crossing to Source is not the only place a rendered row has to be named
/// as a line. A `Visit` is a source line by definition (R34.1), and the cursor
/// a Preview reader moves is a *row*, so the history crosses here as well —
/// storing a row in a field `line_of`, `stale` and every landing read as a
/// source line is the mixed-coordinate mistake ADR 0007 is about, and it
/// claimed a junk row was current rather than failing out loud.
fn source_line(state: &State, row: usize) -> usize {
    buffer_rows(state)
        .get(row.saturating_sub(1))
        .map_or(1, |row| row.line)
}

/// Putting the cursor on a place a landing carries. One function for every
/// landing, because a Preview's caret is a **row** and the place is always a
/// source line: `go_to_place` alone reads that line as a row number, which left
/// the caret on the top of the render while an invisible second cursor moved to
/// the hit. A search hit, a Risk row, a definition, a Visit and the crossing
/// from Source all arrive here, so a sixth landing cannot forget the crossing.
/// Column one while previewing, for the reason [`cross_to_preview`] gives: the
/// row map carries no source column, so there is nothing to hand over. Neither
/// field is clamped here — `settle` clamps both centrally against the rows it
/// laid out.
///
/// The rows are read off `next` rather than off the state the event arrived at,
/// because [`cross_to_preview`] flips `previewing` before landing: the map
/// searched has to be the one about to be drawn.
fn land_at(next: &mut State, at: Place) {
    if !previewing(next) {
        if let Some(buffer) = current(next) {
            buffer.go_to_place(at);
        }
        return;
    }
    let rows = buffer_rows(next);
    // The blank row `preview` inserts between blocks carries the block's line
    // purely so the two are not glued together on screen — it is spacing, not a
    // place that line's content landed, so a row with no pieces at all (only a
    // separator ever has none) is never where a landing goes.
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

/// Source to Preview: the rows are read only after the toggle flips
/// `previewing`, so the row map being searched is the one about to be drawn.
fn cross_to_preview(state: &State, next: &mut State) {
    // Column one and no selection, for the reasons [`cross_to_source`] says:
    // here as well as there, so neither direction is the one somebody forgets.
    next.editor_hscroll = 0;
    next.selection = None;
    let line = current_buffer(state).map_or(1, |buffer| buffer.line);
    if let Some(buffer) = current(next) {
        buffer.previewing = true;
    }
    land_at(next, Place { line, column: 1 });
}

/// TogglePreview, ToggleFold
fn on_toggle_preview(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // The row map is read, never re-derived, in both directions: Preview
        // to Source takes the current row's source line straight off the row
        // still on screen; Source to Preview reads the rows only after the
        // toggle flips `previewing`. The direction is decided from the
        // buffer's own flag, not `previewing(state)` — that composite also
        // folds in `diff` and `walking`, and keying the toggle off it means
        // the second arm always runs while either is showing, so the buffer
        // can only ever be set to previewing and never back.
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

        // Nothing to execute and nothing to persist: a fold is what the reader
        // is looking at, not what the file holds, so the whole answer is state
        // on the buffer.
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

/// EditorKey
fn on_editor_key(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Enter on the dots a fold leaves at the end of its opening line opens
        // the block — the same gesture as clicking them, and the same event,
        // because the caret is already on the line whose block it names.
        // First, or the newline reaches the buffer as text.
        Event::EditorKey('\n')
            if current_buffer(state).is_some_and(editor::Buffer::on_fold_dots) =>
        {
            return Ok(update(state, Event::ToggleFold { all: false }));
        }

        Event::EditorKey(' ') if opens_a_chord(state) => {
            next.modal = Modal::Chord;
            vec![]
        }

        // The sideways gesture, on the two surfaces that are read rather than
        // typed in and have no cursor of their own to follow — Review's diff
        // and the code a Story walk is showing. One arm rather than one per
        // surface: the keys do the same thing on both. Ahead of the diff's own
        // keys and the walk's, both of which claim every key they are handed.
        // A Preview is no longer here: its cursor has a rendered column, so
        // `h`, `l` and `0` are motions there and the offset follows them (ADR
        // 0007, as amended). Nothing is clamped here — the clamp in `settle`
        // runs on every event, so this arm cannot forget it and a surface that
        // shrinks under a held offset is pulled back anyway.
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

        // `gg` over rendered rows. Here rather than in the motion arm below
        // because a lone `g` names no motion at all — it only sets the
        // operator the second `g` completes, so the first one still has to
        // reach the buffer, and the second one must not, or `command` would
        // take `gg` to the *source* line 1 nobody is looking at.
        Event::EditorKey('g') if previewing(state) && pending_g(state) => {
            if let Some(buffer) = current(&mut next) {
                buffer.clear_pending();
                buffer.row = 1;
                buffer.row_column = 1;
            }
            vec![]
        }

        // Ahead of the refusal below, which `i` used to fall into. It is the
        // same crossing `:preview` does — read off the row map, never a line
        // derived again — and the mode is the only difference, because the two
        // are different intentions: `:preview` says "show me the characters"
        // and this says "let me type here".
        Event::EditorKey('i') if previewing(state) => {
            cross_to_source(state, &mut next);
            if let Some(buffer) = current(&mut next) {
                buffer.mode = editor::Mode::Insert;
            }
            vec![]
        }

        // `gp` — the modifier-free route to a jump back — over the refusal
        // below, which claims `p` and would answer a gesture nobody made:
        // going back writes nothing, so "read-only" is not an answer to it.
        // Here for the reason the `gg` arm above is here, and handed straight
        // down the router because `jump_chord` is where both spellings of the
        // jump already meet. `gn` needs no arm: `n` is not a key the refusal
        // claims, so it falls through the motion arm on its own — and a
        // modifier-free route that works one way and not the other is the
        // asymmetry `keys.rs` exists to make impossible.
        Event::EditorKey('p') if previewing(state) && pending_g(state) => {
            return Err((next, Event::EditorKey('p')));
        }

        // A Preview is read-only, out loud: these keys leave the buffer
        // untouched rather than vanishing silently, which is the failure
        // `src/keys.rs` is shaped to prevent. `V` is refused because linewise
        // visual exists only to set up an edit; everything that only reads —
        // motions, `/`, `n`/`N`, yanking a selection — falls through past
        // this arm and keeps working.
        Event::EditorKey(
            'a' | 'o' | 'O' | 'I' | 'x' | 'r' | 'd' | 'D' | 'p' | 'P' | 'u' | 'U' | 'V' | 'c',
        )
        | Event::EditorUndo
        | Event::EditorRedo
            if previewing(state) =>
        {
            next.refusal = Some(preview::Refusal::ReadOnlyPreview);
            // A key answered here is a key the buffer never sees, so a `g`
            // waiting for the one that completes it would still be waiting
            // after `gd` was refused — and fire as `gg` at the next `g`. The
            // refusal above is the whole answer to the chord, exactly as the
            // motion arm below is to `gl`.
            if let Some(buffer) = current(&mut next) {
                buffer.clear_pending();
            }
            vec![]
        }

        // After the refusal above, and guarded as the backspace is: a diff and
        // a walked Site are read-only about their contents.
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

        // Every motion, run over the **rendered rows**: a Preview answers what
        // Source answers, because a document you can only step down through is
        // a poorer read than the markup it was made from. One definition of
        // what `w` means — `editor::moved` over whichever text is on screen —
        // rather than a second table here that could drift from Source's.
        //
        // Last of this function's arms on purpose. A key naming no motion
        // falls straight through to the ladder below, which is what keeps the
        // `i` crossing and the read-only refusal above reachable; both key sets
        // are disjoint from this one, so the order costs nothing else. The
        // buffer is never handed the key, so nothing here can edit, and
        // clamping is `settle`'s.
        Event::EditorKey(key) if previewing(state) => {
            // Only a word motion reads the text it moves through; every other
            // one is arithmetic on the place it starts from. Laying the rows
            // out for all of them would buy a markdown parse for every `n`,
            // `y` and `:` that only falls through — on top of the one `settle`
            // pays — and ADR 0007 is explicit that a second parse an event is
            // visible rather than merely wasteful. A unit test beside `moved`
            // holds the split honest: every other key answers the same over no
            // lines at all as over the rows.
            let rows: Vec<String> = match key {
                'w' | 'e' | 'b' => preview_rows(state).iter().map(preview::Row::text).collect(),
                _ => vec![],
            };
            let at = cursor_place(state).unwrap_or(Place { line: 1, column: 1 });
            let Some(place) = editor::moved(&rows, at, key) else {
                return Err((next, Event::EditorKey(key)));
            };
            // A half-typed `g` is an operator waiting for the key that
            // completes it, and a motion does not complete it: `gl` is no more
            // a binding here than in Source, where `operator` answers with the
            // chord it refused. Refused *and* cleared, rather than moved with
            // the chord still hanging — a `g` that survives a motion fires as
            // `gg` at the next one, and teleports a reader to the first row for
            // a key they pressed once.
            if pending_g(state) {
                if let Some(buffer) = current(&mut next) {
                    buffer.clear_pending();
                }
                vec![Effect::notify_about("no-such-motion", format!("g{key}"))]
            } else {
                // A motion drops what was picked, the way a plain arrow does
                // in Source: the extending arms above run through this one and
                // then set the span they picked, so the cursor moving on its
                // own is the only thing that reaches here with one held.
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

/// EditorKey
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
                    // Removed lines have no new-file number to anchor to, so a
                    // selection of only those cannot carry a comment. Say so
                    // rather than doing nothing.
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

/// EditorKey
fn on_editor_key_3(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Walking a Story is read-only: every key is claimed here, ahead of
        // the vim-key arms below, so none of them ever reach the buffer.
        // `walk_key` still answers the keys this ticket adds, and still
        // toggles the spine against the changed-files list on `t` — the one
        // binding that already worked with nothing open at all.
        Event::EditorKey(key) if state.walking.is_some() => {
            return Ok(walk_key(state, next, key));
        }

        // `gd` asks where the symbol under the cursor is defined — vim's own
        // spelling, and no modifier, since Option is not Alt on macOS unless
        // the terminal is told to make it so. A waiting `g` is normal mode by
        // construction: only `command` sets one, so inserting a `d` still
        // types one.
        //
        // Ahead of the charwise `d` below, which is the whole reason this is an
        // arm here rather than a chord in the ladder further down. Opening a
        // project-search hit leaves a selection over the match, so `gd` on the
        // name it just took you to used to reach that arm and *delete* the
        // word — a jump that silently edits the file it was called from. The
        // `every_key` sweep cannot see it: the selection it drives every key
        // with is a pty one, and that arm only claims a Buffer one.
        Event::EditorKey('d') if pending_g(state) => {
            if let Some(buffer) = current(&mut next) {
                buffer.clear_pending();
            }
            lsp::ask(&mut next, lsp::About::Definition)
        }

        // `gm` is the modifier-free spelling of the next-occurrence gesture,
        // for the reason `gd` above has no modifier: Option is not Alt on
        // macOS unless the terminal is told to make it so, so a Ctrl binding
        // alone would be the only route to it. A waiting `g` is normal mode by
        // construction, exactly as it is for `gd`.
        Event::EditorKey('m') if pending_g(state) => {
            if let Some(buffer) = current(&mut next) {
                buffer.clear_pending();
            }
            take_next_occurrence(&mut next);
            vec![]
        }

        // Typing with other occurrences taken lands at every one of them at
        // once, which is the whole of what taking them was for. In normal mode
        // the first key starts inserting as well, the way `i` would: what the
        // reader is doing is editing, and a mode key first would have to be a
        // key that leaves the occurrences alone — which is every key this arm
        // claims.
        //
        // Ahead of the charwise `d` and `y` below: with occurrences held they
        // are letters of a rename, not operators, and the selection they act
        // on is the one this arm is about to replace.
        Event::EditorKey(key) if !state.occurrences.is_empty() => {
            let picked = state.selection.as_ref().and_then(Selection::buffer_span);
            next.selection = None;
            if let Some(buffer) = current(&mut next) {
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

        // Typing over picked characters replaces them, as every editor does:
        // while inserting, the selection names the text the next keystroke
        // stands in for. Ahead of the charwise `d` and `y` below, which claim
        // those keys in every mode — so typing a `d` over a picked word used to
        // delete the word and type nothing. An opening bracket is not claimed
        // here: it wraps what was picked, in the arm below.
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
            if let Some(buffer) = current(&mut next) {
                buffer.replace_in(from, to, key);
            }
            vec![]
        }

        // Charwise `d` and `y` are claimed here rather than in the buffer,
        // because the span they act on is the workspace's selection, not the
        // buffer's own linewise anchor.
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
            if let Some(buffer) = current(&mut next) {
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

        // A pair typed around a selection wraps it rather than replacing it.
        // Claimed here for the same reason as the two keys above — the span is
        // the workspace's selection, not the buffer's — which is where the
        // pairing rules stop being purely `Buffer`'s. Insert mode only: in
        // normal mode these keys are not typing.
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
            // The opening half shifts everything after it along its own line,
            // so the selection keeps the characters it had by moving with it.
            // The closing half goes in past the span's end and moves nothing.
            let shifted = |place: Place| Place {
                column: place.column + usize::from(place.line == from.line),
                ..place
            };
            if let Some(buffer) = current(&mut next) {
                buffer.wrap_in(from, to, key, close);
                buffer.go_to_place(shifted(cursor));
            }
            next.selection = Some(Selection::Buffer {
                anchor: shifted(anchor),
                cursor: shifted(cursor),
            });
            vec![]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// EditorKey
fn on_editor_key_4(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Shift with a word key extends, so `W` and `B` are claimed before the
        // buffer sees them. Normal mode only: inserting a capital must still
        // type one.
        Event::EditorKey(key @ ('W' | 'B')) if normal_mode(state) => {
            let direction = if key == 'W' {
                Direction::Right
            } else {
                Direction::Left
            };
            return Ok(update(state, Event::EditorExtendWord(direction)));
        }

        // `D` answers the ⚠ the watcher raised. Normal mode only: inserting a
        // `D` must still type one. Refused out loud on a buffer that agrees
        // with disk — a picker offering to resolve nothing reads as the flag
        // being wrong.
        Event::EditorKey('D') if normal_mode(state) => {
            match current(&mut next).is_some_and(|buffer| buffer.changed_on_disk) {
                true => {
                    next.modal = Modal::Diverged;
                    vec![]
                }
                false => vec![Effect::Notify("nothing-diverged")],
            }
        }

        // `K` asks what the symbol under the cursor is — no modifier, and in
        // the cheatsheet, because a key nobody can discover is a key nobody
        // uses. Normal mode only: inserting a capital must still type one.
        // A second `K` moves the keyboard into the box that is up, which is how
        // a Hover longer than the pane is read without reaching for the mouse.
        Event::EditorKey('K') if normal_mode(state) && state.hover.is_some() => {
            if let Some(hover) = next.hover.as_mut() {
                hover.focused = true;
            }
            vec![]
        }
        // The adapter is asked alongside the server, from the cursor's place:
        // what a Hover says while Paused is the same box asked for two ways,
        // and the key reaching only one of them is a `K` that tells the
        // reader less than resting the pointer does.
        Event::EditorKey('K') if normal_mode(state) => {
            let mut effects = lsp::ask(&mut next, lsp::About::Hover);
            if let Some(at) = cursor_place(state) {
                effects.extend(lsp::value_hover(&mut next, at));
            }
            effects
        }

        // `/` searches the file being edited, on the line the editor already
        // owns for `:` commands rather than in the floating modal — finding
        // *here* has to look different from finding *everywhere*. Normal mode
        // only: inserting a slash must still type one.
        Event::EditorKey('/') if normal_mode(state) => {
            return Ok(update(state, Event::OpenFind));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// EditorKey
fn on_editor_key_5(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        // `n` and `N` walk the matches of what `/` last looked for. Normal mode
        // only: inserting an `n` must still type one — and not while a `g`
        // operator is waiting, which is the same guard `list_chord` puts on
        // `t`: `gn` steps the cursor history, and an arm that claims the second
        // key of a chord ahead of the chord ladder is a chord that cannot be
        // reached at all.
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

/// EditorKey, EditorPaste, EditorIndent, PasteFromClipboard
fn on_editor_key_6(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    match event {
        Event::EditorKey(key) => Ok(editor_key(state, next, key, wheeled)),
        // Tab over picked characters is a block indent, not the indentation Tab
        // types at the cursor: what is picked would otherwise be left where it
        // was with a tab dropped in the middle of it. The span is handed back
        // covering whole lines, which is what lets a second Tab move the same
        // lines again — a charwise span whose columns were measured before the
        // edit names different characters after it.
        Event::EditorIndent(direction) => {
            let span = state.selection.as_ref().and_then(Selection::buffer_span);
            if let (Some((from, to)), Some(buffer)) = (span, current(&mut next)) {
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
        // Ctrl+V and Command+V, wherever the editor pane has the keys. Only
        // *whether* to paste is decided here: the text comes back as the
        // `EditorPaste` below, so one arm inserts every paste however it was
        // asked for.
        Event::PasteFromClipboard => {
            if previewing(state) {
                next.refusal = Some(preview::Refusal::ReadOnlyPreview);
                return Ok(settle(next, vec![], wheeled));
            }
            // A diff and a walked Site answer the editor's keys ahead of the
            // buffer — the same reason `keys::typing_into_the_buffer` excludes
            // them — so a paste there would edit a file nobody is looking at.
            let effects = if state.diff.is_some() || state.walking.is_some() {
                vec![]
            } else {
                vec![Effect::ReadClipboard]
            };
            Ok(settle(next, effects, wheeled))
        }
        // Pasted text asks for no candidates: a name that arrived whole is not
        // a name being typed.
        Event::EditorPaste(text) => {
            if let Some(buffer) = current(&mut next) {
                buffer.paste(&text);
            }
            Ok(settle(next, vec![], wheeled))
        }
        other => Err((next, other)),
    }
}

/// Every editor key the arms above did not claim: the chords first, then the
/// key itself, handed to the buffer.
fn editor_key(state: &State, next: State, key: char, wheeled: bool) -> (State, Vec<Effect>) {
    // `gt`/`gT` step buffers, which needs no modifier — Option is not Alt on
    // macOS unless the terminal is told to make it so.
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
    // A key that reached a buffer already inserting is somebody typing, which
    // is the one thing that asks for candidates. Read off the state *before*
    // the key, so the `i` that opens insert mode is a command rather than the
    // first letter of a name.
    // The same key may also be one a server asked to be told about, so that it
    // can lay out the text around it as it is typed.
    let mut typed = vec![];
    if inserting {
        typed = lsp::typed(&mut next);
        typed.extend(lsp::on_type(&mut next, key));
    }
    // `y` reaches the clipboard as well, so the vim key and the system
    // clipboard stop being separate worlds. Only `y`, and only when it
    // actually yanked: `d` fills the register too, and the first `y` of `yy`
    // would otherwise copy whatever the last delete left there.
    let mut effects = match register_of(&next) {
        Some(text) if key == 'y' && Some(&text) != held.as_ref() => to_clipboard(state, text),
        _ => vec![],
    };
    // An operator over a key that names no motion is refused out loud, and
    // names the chord it refused: `dq` is not a binding, and a chord that
    // answers with silence is indistinguishable from a broken key — which is
    // what `dG` looked like for as long as it fell through.
    if let Some(chord) = refused {
        effects.push(Effect::notify_about("no-such-motion", chord));
    }
    effects.extend(typed);
    settle(next, effects, wheeled)
}

/// Whether the buffer on screen is holding a half-typed `g` operator, waiting
/// for the key that completes the chord. Read in two places — the chord ladder
/// below, and the `gd` arm that has to sit ahead of charwise delete — so it is
/// one fact rather than two spellings of it.
fn pending_g(state: &State) -> bool {
    state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
        .map(|buffer| buffer.pending())
        == Some("g")
}

/// `gr` looks up the selection, `*` the word under the cursor. Normal mode
/// only, for the same reason `/` and `n` are: in insert mode a star is a
/// character somebody is typing, and a bullet list or a glob could not be
/// written at all.
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

/// `gt`/`gT` step buffers.
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

/// `gp`/`gn` step the cursor history — the modifier-free route to what `Ctrl+p`
/// and `Ctrl+n` do, spelled with the same letters so the two teach each other.
/// In the shape of `gt`/`gT` and for the same reason: Option is not Alt on
/// macOS unless the terminal is told to make it so, and a stock tmux strips the
/// modifier reports, so a modifier-only binding is one that silently does not
/// exist for whoever has not configured their terminal.
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

/// The spine and the changed-files list share the tree pane; a bare `t` in
/// Story view toggles them — unless a `g` chord is waiting for it (`gt` still
/// steps buffers, above) or the buffer is mid-insert, where a `t` must still
/// type one.
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

/// EditorEscape, ReloadBuffer, WriteBuffer
fn on_editor_escape(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Leaves the Story for the spine; the buffer stays open behind it,
        // but nothing new is opened.
        Event::EditorEscape if state.walking.is_some() => {
            next.walking = None;
            vec![]
        }

        Event::EditorEscape => {
            next.modal = Modal::None;
            // Escape is how everything on screen is dismissed, and a search
            // that is on is on screen until something ends it. The cursor
            // stays: this is ending a search, not abandoning one.
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
                // The workspace has moved, so the figure is a Stale figure — and
                // nothing is measured, because a job started per save is a job
                // competing with typing.
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

/// ToggleBreakpoint, BreakpointFileRead, EditBreakpoint, BreakpointDraft,
/// BreakpointField, SwitchSuspend, ConfirmBreakpoint, and the Run mark the
/// gutter's same column holds: OfferRun, ChooseRun
fn on_breakpoint(mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::OfferRun(line) => {
            run::offer(&mut next, line);
            vec![]
        }
        Event::ChooseRun(action) => run::choose(&mut next, action),
        // Setting one first is what makes `␣B` a way to write a conditional
        // Breakpoint in one gesture, rather than a key that does nothing on
        // every line but the few that already hold one.
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

        // Stale is decided here and nowhere else: a Breakpoint whose line no
        // longer holds its text is marked, never moved to where the text went.
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

/// AiExited, Resolve
fn on_resolve(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Reload and overwrite are `:e` and `:w` — the picker is a way to reach
        // them at the moment they are the question, not a second implementation
        // of either. Merge is the third, and only it leaves the flag standing:
        // the AI has been asked, not yet answered, so the file is still
        // diverged until its write comes back through the watcher.
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

        // Submitting a review starts the AI too, but that cannot be the only
        // way in.
        Event::AiExited => {
            next.ai_spoken = false;
            next.pending_prompt = None;
            // Told by the edge, not guessed at by a stopwatch (ADR 0006):
            // authoring has no timeout, so the only way it ends unconfirmed
            // is the CLI actually dying.
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

/// Where a seek leaves the Reading, and the sound with it. It takes the answer
/// rather than the question: forward and back differ only in which offset they
/// name, and what to do with one is the same decision either way.
fn seek(next: &mut State, sought: Option<reading::Seek>) -> Vec<Effect> {
    let Some(sought) = sought else {
        return vec![];
    };
    match sought {
        // Nothing to seek in yet, or nowhere to go back to. Not an end: a
        // Reading that stopped because a key arrived before its stream did
        // would be a press that lost the passage.
        reading::Seek::Nowhere => vec![],
        // Forward past the last Utterance is the end of the Selection. R35.1
        // says a Reading stops there rather than wrapping — select the next
        // passage.
        reading::Seek::Ended => {
            next.reading = None;
            vec![Effect::StopSpeaking]
        }
        reading::Seek::To(at_ms) => {
            let reading = next.reading.as_mut().expect("the Reading sought in");
            reading.at_ms = at_ms;
            // Seeking is an act of playing: a next pressed while paused means
            // that Utterance, not that Utterance in silence.
            reading.paused = false;
            vec![Effect::SpeakFrom { at_ms }]
        }
    }
}

/// StartReading, StopReading, ReadingEnded, PlayPause, SetSpeed,
/// NextUtterance, PreviousUtterance, Speaking
fn on_reading(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Superseding, never queueing: whatever was in flight is replaced, the
        // same shape a Risk generation and an abandoned LSP request already
        // have (R35.5). The stop goes first so the player holding the old
        // stream is gone before the next one is built over it.
        Event::StartReading => match reading::start(state) {
            // The stop goes first, so the player holding the old stream is
            // gone before the next one is built over it.
            Ok((reading, effects)) => {
                let stopping = next.reading.is_some().then_some(Effect::StopSpeaking);
                next.reading = Some(reading);
                stopping.into_iter().chain(effects).collect()
            }
            // A refusal takes nothing away: `:read` with nothing selected is a
            // press that did not name a passage, and silencing the one already
            // playing would make a mistyped key the way to lose your place.
            //
            // A missing piece is one key from installed: Tools opens on the
            // speech row that fixes it, and `i` takes it there exactly as it
            // would had the reader opened Tools (ADR 0018).
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

        // Lit only when there was a Reading to act on: a dimmed Chip does
        // nothing, and a Chip lit for nothing says something happened.
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

        // Pausing keeps the stream and the position; only the sound stops. The
        // player is not signalled: freezing a process while its audio device
        // drains underruns the buffer and clicks audibly, which was measured
        // rather than guessed (R35.6, and ADR 0013 names the reversal trigger
        // if a rewrite ever clicks instead).
        Event::PlayPause => match next.reading.as_mut() {
            // Play with no Reading in flight starts one over whatever is
            // selected, which is what a play control means everywhere else: a
            // button that does nothing until a command has been typed is a
            // button the reader presses twice and then stops believing.
            // `start` still refuses when nothing names a passage, and says so
            // out loud — so this is a route to a Reading, never a silent one.
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

        // The Reading in flight is not touched, which is the whole rule: the
        // stream it is playing was paced when it was built.
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

        // Returns rather than falling through to the clamp below, and for the
        // tick's reason: the clamp earns its keep by covering everything a
        // person did, and this is what the machine did. A report of where the
        // sound has got to arriving every frame and pulling a wheeled editor
        // back to the cursor would make it unscrollable for as long as a
        // Reading plays.
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

/// DapReceived, DapStarted, DapGone, DapPortAnswers, StartLaunch, MoveLaunchRow, DebugResume,
/// DebugStep, DebugStop, DebugRestart, LeaveStepping, AskAboutPause
fn on_debug(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::DapReceived { json, from } => debug::received(&mut next, &json, from),
        Event::DapStarted { from } => debug::started(&mut next, from),
        // An adapter let go after its session ended is the tail of whatever
        // ended it, so the footer keeps saying why: a launch the adapter
        // refused would otherwise be explained for exactly as long as it took
        // the edge to report the process gone. The same for one let go by a
        // session now Waiting, which lets go on purpose and goes on.
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
        // Nothing while Running, where the Chip is dimmed: a snapshot of the
        // last pause is a snapshot of a program that has moved on.
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

/// LspReceived, LspGone, CandidatesDue, PointerMoved, HoverDue, FormatBuffer, FormatterAnswered,
/// MoveCandidate, AcceptCandidate, NextStop, MoveToolRow, InstallTool,
/// ConfigEdited, GlobalConfigRead, InstallEnded, RecheckTool, PathProbed
fn on_lsp(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::LspReceived { language, json } => lsp::received(&mut next, &language, &json),

        // Nothing to decide: the pass every event ends with is the whole point
        // of the event, and it sends the `initialize` this makes possible.
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

        // Both return rather than falling through to the clamp, for the reason
        // the tick does: nobody pressed the pointer, and a rest that pulled a
        // wheeled editor back to the cursor would leave it unscrollable for as
        // long as the pointer lay over it.
        Event::PointerMoved(pointed) => {
            next.pointed_at = pointed;
            // The box goes with the pointer that asked for it. Asked here
            // rather than left to `settle`, because this arm returns before it:
            // `settle`'s take-down covers everything a person pressed, and this
            // is the one event that moves what a box is a claim about without
            // anybody pressing anything.
            if next.hover.is_some() && !lsp::hover_stands(&next) {
                next.hover = None;
            }
            // A report for the cell the pointer is already on is not a rest
            // interrupted, so the window is left running rather than restarted
            // — a terminal that repeats them would otherwise never let it end.
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

        // Nothing open is the one shape neither half can answer, and it is
        // answered here because the reader pressed a key: `format::run` sees an
        // immutable state and can only return effects, and the refusal
        // `:preview` gives for the same emptiness is the one the reader has
        // already met (R32.13).
        Event::FormatBuffer if state.current_buffer.is_none() => {
            next.refusal = Some(preview::Refusal::NoFileOpen);
            vec![]
        }

        // The server first and the command second, and the order is the whole
        // decision: a server already holding this file's syntax tree formats it
        // for free, and `None` is not a refusal but the handing-on that lets a
        // language no server serves be laid out at all (R32.2).
        Event::FormatBuffer => {
            // A Preview refuses the keys that edit and lets every `:` command
            // through (R26.9), and this is the first `:` command that *authors*
            // text — so it crosses to Source first, the way `i` does and for
            // `i`'s reason: "let me change this". Refusing was the other
            // candidate and is worse than both answers, because the single
            // undo step R32.5 promises is refused two keys later on the very
            // surface that made the edit, and a Preview has no line numbers to
            // show what moved (R32.12).
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

        // The list is taken down as the text goes in, so there is nothing left
        // to answer for a choice already made. Nothing is opened and nothing is
        // sent: `sync` sees the changed Buffer on the way out of `update` and
        // tells the server what it now holds, the same as any other edit.
        //
        // A snippet is parsed on the way in, and what it leaves to fill in
        // takes the list's place. A reply that is not one is its own text and
        // names no stops, which is every reply from every server that ignores
        // the capability — and the reason nothing about this is felt there.
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

        // The stops are the ones *left*, so taking one is both what moves the
        // cursor and what ends the sequence when it was the last: there is no
        // condition where Tab has run out of stops and the modal is still up.
        // A stop the buffer can no longer name ends it too — the text that
        // measured it is gone, and a cursor sent to the first character is a
        // place nobody asked for.
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
                    // The list is a column, so sideways is not a move in it.
                    Direction::Left | Direction::Right => *row,
                };
            }
            vec![]
        }

        // Taking a row, which is the whole install (ADR 0018). The key only
        // asks for the global config's text: what is written back is decided
        // against the file as it is now, never as it was when Varde started,
        // since a reader who edited it since would lose the edit.
        Event::InstallTool => {
            let offered = match next.modal {
                Modal::Tools { row } => tools::rows(&next).into_iter().nth(row),
                _ => None,
            };
            let Some(row) = offered else {
                return Ok(settle(next, vec![], wheeled));
            };
            match (&row.availability, &row.install) {
                // The command is here and installing it again would not add
                // what its row says is missing — that is a second program, or a
                // configuration key, and neither is what this binding runs.
                (tools::Availability::Installed | tools::Availability::Partial { .. }, _) => {
                    next.refusal = Some(preview::Refusal::ToolAlreadyInstalled);
                    vec![]
                }
                // No refusal for `Unpackaged`, a stopped row, or a missing
                // requirement with no install for this OS: a language nobody
                // has packaged here is a normal row, not an error, and the row
                // already says so under the cursor — an unmet one names the
                // fact it lacks.
                (
                    tools::Availability::Unpackaged
                    | tools::Availability::Stopped
                    | tools::Availability::Unmet { .. },
                    None,
                ) => vec![],
                // Speech is keys of one `[speech]` table, not a table of its
                // own, so there is no row to append — and an install run for a
                // row no file names installs a program nothing runs.
                (tools::Availability::Available, _) if row.kind == tools::Kind::Speech => vec![],
                (tools::Availability::NeedsInstaller { installer }, _) => {
                    next.refusal = Some(preview::Refusal::NeedsInstaller(installer.clone()));
                    vec![]
                }
                // The same refusal for a requirement's install, whose row goes
                // on naming the fact rather than the package manager: the fact
                // is what is missing, and the manager is only why it stays so.
                (tools::Availability::Unmet { .. }, Some(install))
                    if tools::installer(install)
                        .is_some_and(|installer| !next.commands_on_path.contains(&installer)) =>
                {
                    next.refusal = tools::installer(install).map(preview::Refusal::NeedsInstaller);
                    vec![]
                }
                // A command that is here and does not work is one an install
                // would fix, so a stopped row is taken rather than refused.
                _ => {
                    // The list goes: the install runs in the shell pane, where
                    // a `sudo` prompt is answered, and `settle` moves the focus
                    // there for every injection.
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

        // Said rather than refused: a start would not open on this file, but
        // the session is already open on the last one that worked.
        Event::ConfigEdited { global, project } => {
            match startup::reload(&mut next, global, project) {
                Ok(()) => vec![],
                Err(error) => vec![Effect::NotifyAbout {
                    slug: "broken-config",
                    about: error.to_string(),
                }],
            }
        }

        // What an install that exited 0 configures, decided against the file
        // as it is now: the reader may have chosen a voice while it ran.
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
                    // Speaks from now on, as a start reading this file would —
                    // unless a project's own voice beats the global one.
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

        // The row written and its install run, or neither: a file Varde would
        // refuse to start on is refused here too, before anything is written
        // or run.
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
                        // The rows it added run from now on, as a start
                        // reading this file would run them: a row the file
                        // names is a row Varde starts, with no restart.
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
                        // Where a `sudo` prompt or a passphrase is typed.
                        next.focus = Pane::Terminal;
                        effects.push(Effect::RunInTerminal(tools::reported(&install, &sentinel)));
                    }
                }
            }
            effects
        }

        // A status of 0 is a re-check of the row: the probe finds the command,
        // and the pass every event ends with starts the server as a fresh
        // start would. Anything else — a sentinel with nothing readable in it
        // included — is said on the row.
        Event::InstallEnded(status) => match next.installing.take() {
            Some(row) if status.as_deref().map(str::trim) == Some("0") => {
                // And what the install put on disk, named in the file: the
                // voice it fetched is what the row needs to speak. Only
                // `[speech]` carries `configures`.
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

        // The probe again, and named as the row it was asked about: the answer
        // has to be read against a command, and the list may be gone by the
        // time it lands. Nothing is decided here — an install that worked is
        // indistinguishable from one that has not finished until `PATH` is
        // asked, and asking is the edge's (R31.23).
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

        // Forgetting a write-off is `sync`'s, on the way out of every event —
        // this arm answers only the question a re-check asked. Still nothing:
        // an installer that appended its directory to a shell profile is
        // invisible to a process that inherited its environment at launch, and
        // that is the whole of what a restart answers, so it is offered here
        // and taken nowhere (R31.24).
        Event::PathProbed => {
            if let Some((kind, name)) = next.recheck.take() {
                // What a restart can change is `PATH`, so that is all this
                // asks. A requirement found is a search of the workspace,
                // which a restart finds no differently.
                let found = tools::rows(&next)
                    .into_iter()
                    .find(|row| row.kind == kind && row.name == name)
                    .is_some_and(|row| match &row.availability {
                        // Missing a requirement, what a restart can bring is
                        // the requirement's command, not the row's own.
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
                // And only over the list that asked. A question that replaced
                // whatever is on screen when the answer happens to land is a
                // modal that appeared on its own, which is the one thing no
                // modal in Varde does — the `Diverged` box is opened by a key
                // and never by the watcher for exactly this reason. At the edge
                // the answer lands in the same drain as the asking, so there is
                // nothing to step on; this is what keeps that true rather than
                // incidental.
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

/// AiSpoke, StartAi
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
                // Already running and no particular CLI asked for: just go there.
                (true, false, _) => return Ok((next, vec![])),
                // A different CLI is a deliberate act, so it needs a deliberate
                // `:ai!` rather than silently discarding the running session.
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

/// CloseBuffer, CloseAllBuffers, Quit, Restart
fn on_close_buffer(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Closing discards the buffer in front of you, so that is the one it
        // asks about. It used to ask whether *any* buffer was dirty — quitting's
        // question, and quitting is right to ask it because quitting really does
        // discard everything — which made one unsaved file elsewhere enough to
        // lock every clean buffer open for the rest of the day.
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
            // Move to a neighbour if there is one; only the last close empties.
            match next.buffers.keys().next().cloned() {
                Some(path) => return Ok(update(&next, Event::ShowBuffer(path))),
                None => next.focus = Pane::Tree,
            }
            vec![]
        }

        // Clearing up, so it refuses nothing: skipping a dirty buffer is what
        // the gesture is for, not a failure to report. What it kept is said out
        // loud all the same — buffers still open after a clear-up is otherwise
        // a fact you discover by counting dots.
        Event::CloseAllBuffers { force } => {
            // `unwrap_or`, never a filter: a buffer the workspace root does not
            // contain is still one this kept, and dropping it from the sentence
            // is the silence the notice exists to break.
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
                // Through closing's own last-close arm rather than a second
                // spelling of it — two of them are two that can come apart.
                // With nothing current left to take, `force` discards nothing:
                // all it skips is a refusal about a buffer that is already gone.
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

        // Q59: the restart is Varde leaving, not Varde re-executing itself.
        // Re-executing would inherit this process's environment — the very
        // environment the shell profile's new directory is missing from — so it
        // would answer the one situation the question exists for by not
        // answering it. Quitting is therefore the whole of it, and it *is*
        // `Quit`: a restart that could discard unsaved buffers would be a way
        // around the refusal quitting already makes, so there is one refusal
        // rather than two that can come apart.
        Event::Restart => {
            next.modal = Modal::None;
            return Ok(update(&next, Event::Quit));
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// QuitForce, ToggleCheatsheet, ToggleRiskAll, ToggleRiskList, ToggleTallAi
fn on_quit_force(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::QuitForce => leaving(&next),

        // `cd` first, because the terminal pane sits in the workspace and a bare
        // build would build whichever project the user happens to have open. No
        // known checkout means no directory this may build in, so it says so
        // rather than running something: a command that silently does nothing is
        // a bug.
        Event::ToggleCheatsheet => {
            // Focus stays on the slot, which `settle` hands to whoever is in it.
            next.ai_slot = match state.ai_slot {
                layout::Slot::Ai => layout::Slot::Cheatsheet,
                layout::Slot::Cheatsheet => layout::Slot::Ai,
            };
            vec![]
        }

        // Saved on the spot rather than at exit, for the same reason the tree
        // divider is: a session that ends by closing the terminal saves
        // nothing, and a field turned off before that has not been turned off.
        Event::ToggleField => {
            next.editor_field = !state.editor_field;
            vec![Effect::SaveState(state_json(&next))]
        }

        // Saved on the spot, for the reason the field above is.
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

        // Toggling it on moves focus into it, because opening a pane in order
        // to use it should not need a second gesture — and off hands focus back
        // to the tree above it rather than leaving it on a pane that is gone.
        //
        // One rule for every occupant, because the corner is one slot: asking
        // for the pane that is already there hides it, and asking for another
        // replaces it. A second rule per pane is where a precedence between
        // them would have to be written down, and there is nothing to decide.
        Event::ToggleRiskList => take_the_corner(state, &mut next, layout::Corner::Risk),
        Event::ToggleBuffersList => take_the_corner(state, &mut next, layout::Corner::Buffers),
        Event::ToggleCursorHistory => take_the_corner(state, &mut next, layout::Corner::History),
        Event::ToggleBreakpointList => {
            take_the_corner(state, &mut next, layout::Corner::Breakpoints)
        }
        // Hidden when it is there whatever it is showing, since the toggle
        // asks for the list rather than for one Severity of it.
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
        // Never a toggle: a letter or a label asks to see a Severity, and a
        // second press on the one showing is still asking to see it.
        Event::ShowDiagnostics(severity) => {
            if lsp::showing(state) != Some(severity) {
                next.diagnostics_selection = 0;
            }
            next.selected_action = None;
            next.corner = layout::Corner::Diagnostics(severity);
            next.focus = Pane::Diagnostics;
            vec![Effect::SaveState(state_json(&next))]
        }

        // The two gestures the pane exists beside, answered wherever Varde's
        // own keys are answered. `history` holds the whole of what a step is,
        // because a step from the middle of the list and a step from the live
        // end past the newest Visit are one question — and answering them in
        // two places is what would leave forward with nowhere to return to.
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

/// Which pane the corner beneath the tree holds, and where the keyboard lands.
/// `Corner::pane` is what makes a new occupant a compiler error rather than a
/// toggle that leaves the keyboard on a pane that is no longer drawn; an empty
/// corner hands the keyboard back to the tree above it.
///
/// Whatever row action was armed is let go of here, and in `Event::MoveFocus`
/// for the same reason: an armed icon is an index into the list of the pane
/// that armed it, and the pane focus lands on resolves it against its own.
fn take_the_corner(state: &State, next: &mut State, asked: layout::Corner) -> Vec<Effect> {
    next.selected_action = None;
    next.corner = match state.corner == asked {
        true => layout::Corner::Hidden,
        false => asked,
    };
    next.focus = next.corner.pane().unwrap_or(Pane::Tree);
    vec![Effect::SaveState(state_json(next))]
}

/// AskDefinition, ClickText, DoubleClickText, DragText, HoverLink, HoverAction, HoverMinimap,
/// Rebuild, ReleaseAnswered, SelectIn
fn on_rebuild(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Ahead of the scroll clamp and returning before it, for the reason the
        // tick is: a pointer nudged a cell after a wheel is not a request to go
        // back to the cursor, and pulling the editor back on one would leave it
        // unscrollable for as long as the pointer sits over it. Nothing here is
        // decided — what is under the pointer is `lsp::pointed`'s question,
        // asked of the place rather than answered into a field.
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
        // Replaced stays true across a refusal, so saving and asking again is
        // the whole cost of an unsaved buffer.
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

        // A click is a plain move: it takes the caret and drops what was picked,
        // the same way a plain arrow does. `at.line` names a **row** while
        // previewing — `mouse::place_in` never learns the difference, since the
        // row map is read here rather than re-derived in the hit-test.
        Event::ClickText(at) => {
            next.focus = Pane::Editor;
            next.selection = None;
            next.occurrences.clear();
            if previewing(state) {
                // The tail clamp below pulls a stale row and a column past the
                // end of it back into range on every event, previewing's own
                // included — so nothing here has to compute `preview_rows` a
                // second time to bound either. The column comes straight off
                // `mouse::place_in`, which already resolved the pane origin,
                // the absent gutter and the sideways offset.
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

        // The same question `gd` asks, from wherever the cursor now is: the
        // click that came with it placed the caret first, so nothing here has
        // to know a place at all.
        Event::AskDefinition => lsp::ask(&mut next, lsp::About::Definition),

        // A Preview's rows are already the library's, so the span resolves
        // to text right here rather than becoming a `Selection` request for
        // the edge to fulfil — the same shape `Event::SelectIn` gives a pty's
        // selection once the edge has read its grid, arrived at without one.
        // Built as a `Screen` selection rather than folding this into
        // `Buffer`'s: a `Buffer` selection's `Place`s are read as source
        // line and column everywhere else this crate uses one (project
        // search's landing selection, Shift-extending in Source), and a
        // second meaning for the same shape is exactly how the renderer and
        // the mouse came to disagree about where a click lands.
        Event::DragText { from, to } => {
            next.selection = Some(text_selection(state, from, to));
            vec![]
        }

        // The word under the pointer, off the row the click landed on — a
        // Preview's row rather than a source line while previewing, for the
        // reason the span above resolves against rows there: `at.line` names
        // whichever of the two the pane is showing.
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

/// The selection a span of the editor's text makes, however it was picked.
///
/// A Preview's rows are already the library's, so the span resolves to text
/// right here rather than becoming a `Selection` request for the edge to
/// fulfil — the same shape [`Event::SelectIn`] gives a pty's selection once the
/// edge has read its grid, arrived at without one. Built as a `Screen`
/// selection rather than folding this into `Buffer`'s: a `Buffer` selection's
/// `Place`s are read as source line and column everywhere else this crate uses
/// one (project search's landing selection, Shift-extending in Source), and a
/// second meaning for the same shape is exactly how the renderer and the mouse
/// came to disagree about where a click lands.
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

/// DragRow, ShowDiff, Story, StoryArtifact, StoryFiles
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
            // A re-read of the same file keeps the reviewer's place — the AI
            // writes constantly, and a diff that jumps to the top under you is
            // unreadable. A different file starts at the top.
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

        // Refused whole, or not walked at all: a broken artifact and one
        // whose range is gone are different reasons nothing is walkable, and
        // the reviewer needs to be able to tell which happened.
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
                // A re-read of the artifact already in hand, kept rather
                // than refilled — see `story::same_set`. `Fixing` as well as
                // `Loaded`: leaving Story view and coming back re-reads the
                // folder, and the failing set arriving again is that round
                // still open, not the AI's answer to the fix request. Without
                // this the reviewer spends the second attempt by walking away
                // from the pane and back.
                (Ok(artifact), story::RangeStatus::Resolves)
                    if matches!(&state.story_set,
                        story::Set::Loaded(held) | story::Set::Fixing { artifact: held, .. }
                        if story::same_set(held, &artifact)) =>
                {
                    vec![]
                }
                // Parsed but not yet walkable: only the edge can read what
                // the Sites hold, so the artifact is held until it answers.
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
                    // Which round wrote this, and whether anybody asked for
                    // it at all. Only a wait can be answered: an artifact
                    // arriving while a fix request is outstanding is that
                    // request's answer and counts as the next attempt, one
                    // arriving into an authoring wait is the first, and
                    // anything else — Story view opening on a set already on
                    // disk — was not asked for and gets no fix round.
                    let attempt = match &state.story_set {
                        story::Set::Authoring { .. } => Some(1),
                        story::Set::Fixing { attempt, .. } => Some(*attempt),
                        _ => None,
                    };
                    next.story_set = story::Set::Filling { artifact, attempt };
                    effects
                }
            };
            // Re-authoring puts every Prediction afresh, never reusing what
            // an earlier artifact's steps were asked.
            next.predictions_put = BTreeSet::new();
            effects
        }

        // The fill's other half, and the arrival checks with it. `Loaded` is
        // reached here and nowhere else for an arriving artifact, which is
        // what keeps a Step from ever being shown with no baseline behind its
        // stale check — or with the checks half run. An answer for a set that
        // has since moved on is dropped, the way a superseded `RiskFigures`
        // is: it describes files nothing is waiting for.
        //
        // A set that fails a check is handed back rather than thrown away
        // (ticket 06): the AI gets a fix request naming only the failing
        // Steps, and Varde keeps waiting. Bounded by rounds and never by a
        // clock — `story::ATTEMPTS` artifacts, then the set is refused whole
        // down the same path a malformed one takes. Nothing in either state is
        // walkable: a Step Varde already knows points at the wrong code is
        // worse than no Story at all.
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
                    // Only a round the reviewer is waiting on is handed back,
                    // and only while rounds are left. A set that arrived
                    // unbidden is refused where it stands: handing it back
                    // would start a CLI nobody asked for and spend it
                    // rewriting a Story nobody is waiting for.
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

        // Only the edge can walk the offline ladder or check an explicit
        // range, so this always asks rather than guessing here.
        Event::Story { explicit, force } => vec![resolve_story(&next, explicit, force)],

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// Branches, CheckedOut, CheckoutFailed, ChooseBranch, ConfirmStory,
/// FilterBranches, MoveBranchRow, PickBranch, StoryResolved
fn on_story_resolved(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::StoryResolved(resolution) => match resolution {
            // Already on disk — load it directly. Neither the AI pane nor a
            // session is touched.
            story::Resolution::Authored => return Ok(switch_view(&next, View::Story)),
            // Authoring costs minutes and clears the AI's prompt, so it is
            // always confirmed first (R22.4).
            story::Resolution::ToAuthor { spelling, out } => {
                next.modal = Modal::ConfirmStory { spelling, out };
                vec![]
            }
            // Not `switch_view`: its `Effect::ReadStories` would ask the edge
            // to re-read whatever is on disk, which could overwrite the
            // refusal the spine has to show with an unrelated set.
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

        // The confirmed range's authoring prompt goes into the AI pane
        // exactly as a submitted review's does (ADR 0006): inline text,
        // waiting for `AiSpoke` if the CLI has not printed anything yet.
        // Not `switch_view`: its `Effect::ReadStories` would immediately
        // re-read whatever is already on disk, overwriting the "authoring"
        // wait with a re-authored range's stale set.
        Event::ConfirmStory => match &state.modal {
            Modal::ConfirmStory { spelling, out } => {
                let spelling = spelling.clone();
                let out = out.clone();
                next.modal = Modal::None;
                move_to_view(&mut next, View::Story);
                next.story_set = story::Set::Authoring {
                    spelling: spelling.clone(),
                };
                // Written before the prompt reaches the pane: the prompt
                // points at it in one line, and an AI that read it first would
                // find nothing there.
                // Both paths are already absolute and inside Varde's own
                // directory, which is what lets the prompt go to a session
                // whose working directory Varde never set and never moves.
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

        // Only the edge can list refs, so this asks rather than guessing here —
        // the same round trip `Event::Story` takes. A URL is a repository that
        // is not on the machine yet, so it is cloned first.
        Event::PickBranch(url) => match url {
            // This folder, so no Guest repo is under review any more: the
            // branches about to be listed are the workspace's, and a `guest`
            // left standing would check one of them out in the clone.
            None => {
                next.guest = None;
                vec![Effect::ReadBranches]
            }
            Some(url) => download_guest(&mut next, url),
        },

        // The list, the dedup and the order are `story`'s decision; the two
        // refusals are shown the way every other unwalkable range is, so a
        // reviewer reads them where they are already looking.
        Event::Branches(branching) => match branching {
            story::Branching::Listed(refs) => {
                // A clone is over the moment its branches are listed, so the
                // message that described it goes: a spine still saying
                // "cloning" behind the picker is a message that outlived what
                // it named. Only that one — a local `:story?` opens the picker
                // over whatever set is loaded and must not drop it.
                //
                // It is also where the Guest repo is adopted: refs listed for
                // a clone in flight are that clone's, so from here every git
                // question a Story asks is asked of the Sidecar's copy rather
                // than of the folder Varde was opened on.
                if let story::Set::Downloading { url, .. } = &next.story_set {
                    let url = url.clone();
                    next.guest = Some(
                        varde_dir(&next.root, next.sidecar.as_deref())
                            .join(story::guest_name(&url)),
                    );
                    // Refs listed for a download in flight are a repository
                    // that is now on disk, so the next `:story?` on this URL
                    // fetches. Here rather than where the download was asked
                    // for: a clone that failed left nothing to fetch from.
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
            // Out loud, with the status: a download fails on a URL with a typo
            // in it, a host that refuses the key, a passphrase nobody
            // answered. Which of the two was running is the core's own state —
            // the edge read a number and nothing else — and the Guest repo a
            // failed fetch was aimed at stays where it is: the copy on disk is
            // still the one under review.
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
                    // The list is a column, so sideways is not a move in it.
                    Direction::Left | Direction::Right => *row,
                };
            }
            vec![]
        }

        // The selection is pulled back onto a row that is still shown, the way
        // every list in Varde clamps: a row picked before a filter narrowed the
        // list is a row nobody can see, and Enter on it would check out a branch
        // the reviewer is not looking at.
        Event::FilterBranches(text) => {
            if let Modal::Branches { refs, filter, row } = &mut next.modal {
                *filter = text;
                *row = (*row).min(story::branches(refs, filter).len().saturating_sub(1));
            }
            vec![]
        }

        // The picked row leaves with the modal: the checkout is the edge's, and
        // a list still up over a repository that has moved is a list of rows
        // nobody picked.
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

        // `explicit: None`, so the range is the offline ladder's — which, with
        // the picked branch now checked out, is the merge-base of the default
        // branch and it: exactly what the branch introduced.
        Event::CheckedOut { left } => {
            next.left_branch = Some(left);
            vec![resolve_story(&next, None, false)]
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// The ladder walk, aimed at the repository under review and at the directory
/// its artifact belongs in — one place, because a second spelling of either
/// path is how a Guest repo's Story set comes to be authored against the
/// workspace it is only visiting.
fn resolve_story(state: &State, explicit: Option<String>, force: bool) -> Effect {
    Effect::ResolveStory {
        repo: state.repo_root().to_path_buf(),
        dir: varde_dir(&state.root, state.sidecar.as_deref()),
        explicit,
        force,
    }
}

/// Whatever `:story?` has to say instead of a picker — a refusal, or a clone
/// still running — shown the way `NoDefaultBranch` and `BadRange` are: Story
/// view, saying which. Not `switch_view` for the reason those two are not —
/// its `Effect::ReadStories` would re-read whatever is on disk over the
/// message the reviewer has to read.
fn say_in_story(next: &mut State, set: story::Set) -> Vec<Effect> {
    move_to_view(next, View::Story);
    next.story_set = set;
    vec![Effect::RenderView(View::Story)]
}

/// `:story? <url>`: a Guest repo, cloned into the Sidecar by the user's own
/// `git` in the shell pane, where their SSH config, their per-host key and
/// their agent all work without being configured twice and a passphrase
/// prompt is answerable (ADR 0015).
///
/// A URL already downloaded this session is fetched instead ([`story::Download`]).
///
/// Refused in a project workspace: a project workspace is locked to its
/// project, and a foreign repository must never appear inside it. Refused
/// again with no `git` on the machine — the one runtime dependency this
/// feature adds, and an absence Varde says out loud rather than a command
/// that fails in a pane.
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
    // Through the accessor like every other Varde path (ticket 01), so the
    // directory the download lands in and the one the watcher matches the
    // sentinel against cannot be spelled two ways.
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

/// EnterRemainder, EnterStory, RecomputeRisk, RiskFigures, StartRefactorLoop, StepStory, StopRefactorLoop, StoryFileWritten
fn on_story_file_written(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Told by name only (ADR 0005): the AI writes each file, not Varde,
        // so the edge is the one that lists the directory and knows which
        // name is oldest — this only tracks order and prunes past it.
        // Re-authoring writes the same name again (a story dies with its
        // range), so the old position is dropped first: without that, one
        // range re-authored past the limit would count as several distinct
        // sets and evict a real one early.
        Event::StoryFileWritten(name) => {
            next.story_sets.retain(|existing| existing != &name);
            next.story_sets.push(name);
            let mut effects = Vec::new();
            let dir = varde_dir(&next.root, next.sidecar.as_deref()).join("stories");
            let kept = story::prune(&next.story_sets, story::RETENTION);
            for pruned in next.story_sets.iter().filter(|name| !kept.contains(name)) {
                // Retention stays one rule, not two: the hand-over is pruned
                // by whatever prunes the set it was written for.
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

        // Chosen from the spine like a Story is, and stepped through like
        // one too — but there is no premise or claim to load, only a bare
        // location, because the Remainder is not a Story.
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

        // The answer says which Scope it describes by carrying a before or not,
        // so there is one arm rather than one per Scope — and the field that
        // decides is the same field the delta is measured from.
        Event::RiskFigures {
            generation,
            figures,
            before,
        } => risk::figures_arrived(&mut next, generation, figures, before),

        // A recompute is a fresh measurement, so the last loop's verdict is
        // history: `stopped` and the tick beside it both feed `risk::status`,
        // which trails the border's figure, and neither was cleared by anything
        // but the *next* start. So a run that ended on a failing test left
        // "tests failed" sitting beside a number measured long after those
        // tests stopped failing — a border making two claims about two
        // different moments, only one of them still true.
        //
        // Only while no loop is running: mid-run the same two fields are the
        // Gate's live verdict, and the Gate recomputes as part of its own pass.
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

/// MoveFocus, PaneAction, StepRemainder, TestsFinished, Tick
fn on_pane_action(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // The pane's own actions, each straight through to the event its
        // gesture already had. Exhaustive over what `risk::pane_actions`
        // offers; a name from nowhere does nothing rather than guessing.
        Event::PaneAction(risk::RECOMPUTE) => return Ok(update(state, Event::RecomputeRisk)),
        Event::PaneAction(risk::START_LOOP) => {
            return Ok(update(
                state,
                Event::StartRefactorLoop(risk::on_screen(state.view)),
            ));
        }
        Event::PaneAction(risk::STOP_LOOP) => return Ok(update(state, Event::StopRefactorLoop)),
        // The Transport, each control straight through to the event its `:`
        // command already had — a click and the key are the same gesture, so
        // they are the same event and not two paths that can drift.
        Event::PaneAction(reading::PREVIOUS) => return Ok(update(state, Event::PreviousUtterance)),
        Event::PaneAction(reading::PLAY_PAUSE) => return Ok(update(state, Event::PlayPause)),
        Event::PaneAction(reading::NEXT) => return Ok(update(state, Event::NextUtterance)),
        Event::PaneAction(reading::STOP) => return Ok(update(state, Event::StopReading)),
        // The one control whose event carries a value: the ladder's next rung,
        // because a border glyph cannot be typed a number into. `:speed 1.4`
        // is the same event with the number said out loud.
        Event::PaneAction(reading::SPEED) => {
            return Ok(update(
                state,
                Event::SetSpeed(reading::next_speed(state.speech.speed)),
            ));
        }
        // The Variables' Transport, each Chip straight through to the event
        // its key already had, for the reason the Reading's are: a click and
        // the key are one gesture, so they are one event and not two paths
        // that can drift. `next-thread` has no key, so it is no event either.
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
        // Dimmed with nothing to clear, and a dimmed Chip does nothing.
        Event::PaneAction(debug::TOGGLE_OUTPUT) => return Ok(update(state, Event::ToggleOutput)),
        Event::PaneAction(debug::CLEAR_ALL) if !state.breakpoints.is_empty() => {
            next.breakpoints.clear();
            vec![Effect::SaveState(state_json(&next))]
        }
        // Read off the Chip, as the set-value box is: dimmed, it does nothing.
        Event::PaneAction(debug::EXCEPTION_CLASS) => {
            if offers(&debug::transport(state), debug::EXCEPTION_CLASS) {
                next.modal = Modal::ExceptionClass;
            }
            vec![]
        }
        // Unreachable from any gesture — the key, the icon and the hit-test all
        // read `risk::pane_actions` — and here because a `&str` match has to be
        // exhaustive. Nothing rather than a guess: a pane action is a name, and
        // a name from nowhere describes nothing to do.
        Event::PaneAction(_) => vec![],

        Event::TestsFinished { passed, output } => risk::tests_finished(&mut next, passed, output),

        // Returns rather than falling through to the clamp below: the wheel is
        // allowed to move a view away from the cursor, and a tick eighty
        // milliseconds later that pulled it back would make the tree
        // unscrollable for as long as a job runs.
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
            // Sideways through the strip's own shells first, and out of it
            // only past the last: the splits are one pane to every other
            // gesture, so this is the one place they are stepped through.
            match (state.focus, direction) {
                (Pane::Terminal, Direction::Right) if state.split() + 1 < state.terminals.len() => {
                    next.terminal_split = state.split() + 1
                }
                (Pane::Terminal, Direction::Left) if state.split() > 0 => {
                    next.terminal_split = state.split() - 1
                }
                _ => next.focus = neighbour(state, direction),
            }
            next.selected_action = None;
            vec![]
        }

        // The Shell group comes forward with it: a split asked for while the
        // Debug group is up is a shell nobody would see, and the Debug group
        // never grows a split of its own — the Program output is the
        // program's terminal, not another one of the reader's.
        Event::SplitTerminal => {
            let from = state.split();
            next.focus = Pane::Terminal;
            next.strip = layout::Group::Shells;
            next.terminal_split = from + 1;
            vec![Effect::SplitTerminal { from }]
        }

        // A click in a shell drops what was picked, as `ClickPane` does for
        // every other pane: a held selection is what turns Ctrl+C from the
        // interrupt into a copy, so one nobody can drop is a shell nobody
        // can stop.
        Event::FocusSplit(split) => {
            next.focus = Pane::Terminal;
            next.terminal_split = split;
            next.selected_action = None;
            next.selection = None;
            vec![]
        }

        // Only the split the keyboard was sent ahead to: the first shell's
        // prompt, or any other's, is not the one a held command was waiting
        // for. `settle` routes it again, to the split that is now idle.
        Event::ShellSpoke(split) if split == state.split() => {
            next.pending_command.take().into_iter().collect()
        }
        Event::ShellSpoke(_) => vec![],

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// MoveSelection
fn on_move_selection(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // The Risk list's own selection: its rows are Functions, so an index
        // rather than a path, and it moves only while the pane has the keyboard
        // — the tree below it is still the pane these motions belong to
        // otherwise.
        Event::MoveSelection(direction) if state.focus == Pane::Risk => {
            // `rows`, not `rows - 1`: the slot past the last row is the pane's
            // own actions (`risk::on_actions`), so stepping down off the end of
            // the worklist is how the recompute and the loop are reached without
            // the mouse. No early return on an empty list, for the same reason —
            // that is the Scope where the recompute is the only thing to reach.
            let rows = risk::list(state).len();
            next.risk_selection = match direction {
                Direction::Down => (state.risk_selection + 1).min(rows),
                Direction::Up => state.risk_selection.saturating_sub(1),
                _ => state.risk_selection.min(rows),
            };
            // Another row means the actions you had stepped into are gone —
            // the same rule the tree's own motion follows, and without it Enter
            // would ask for a refactor of a row nobody armed.
            //
            // The slot past the last row is the exception, and not an
            // inconsistency: a row is itself a target, so it is arrived at bare
            // and stepped into with Right, but the actions slot holds nothing
            // *but* its actions — arriving there with none of them armed would
            // be arriving somewhere Enter does nothing.
            next.selected_action = risk::on_actions(&next).then_some(0);
            vec![]
        }

        // The Cursor history's own selection, which is also the position going
        // back and forward move: one field, so arrowing onto a row and then
        // pressing Enter goes where the arrows left it. Clamped to the last row
        // rather than one past it — the pane has no actions slot below the
        // list, and the position past the newest Visit is arrived at by
        // recording rather than stepped onto.
        Event::MoveSelection(direction) if state.focus == Pane::History => {
            let last = history::list(state).len().saturating_sub(1);
            next.history_selection = match direction {
                Direction::Down => (state.history_selection + 1).min(last),
                Direction::Up => state.history_selection.saturating_sub(1),
                _ => state.history_selection.min(last),
            };
            // Another row means the icon you had stepped into is gone — the
            // same rule the tree's and the Risk list's motions follow.
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

        // The Buffers pane's own selection. Clamped to the last row rather than
        // one past it: the pane has no actions, so there is no slot below the
        // list to step into and stepping onto one would be arriving somewhere
        // Enter does nothing.
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

/// MoveSelection
fn on_move_selection_2(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // The spine's own selection: its rows are Stories, not tree paths,
        // so it cannot share `tree_selection`/`tree::visible_rows` the way
        // the changed-files listing does.
        Event::MoveSelection(direction)
            if state.view == View::Story && state.story_listing == story::Listing::Spine =>
        {
            if story::spine(state).is_empty() {
                return Ok((next, vec![]));
            }
            // The Remainder sits one slot past the last Story — selectable
            // only when there is something in it to walk.
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

/// MoveSelection
fn on_move_selection_3(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
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
            // Another row means the actions you had stepped into are gone.
            next.selected_action = None;
            // Browsing shows you what you are browsing — but never over
            // unsaved work, and never in Review view, where the left pane
            // lists files whose diffs Enter loads.
            match state.view {
                // Reviewing is reading, so show what you land on.
                View::Review => vec![Effect::ReadDiff(landed.path)],
                View::Edit if !landed.is_dir => vec![Effect::PreviewBuffer(landed.path)],
                View::Edit | View::Story => vec![],
            }
        }

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// Activate, MoveAction
fn on_activate(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Stepped into the actions, Enter runs the one you are on.
        Event::Activate if state.selected_action.is_some() => {
            let actions = selected_row_actions(state);
            match actions.get(state.selected_action.unwrap_or(0)) {
                Some(action) => {
                    next.selected_action = None;
                    // Which event the name is routed by follows which list it
                    // came from: the pane's two actions are what the border's
                    // icons already reach when they are clicked, so the keyboard
                    // arrives at the same arm rather than a second one.
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
                // Left at the first action steps back out to the row — except on
                // the Risk pane's actions slot, where there is no row under the
                // icons to step back out onto. Up is the way out of that one.
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

/// Activate, ClickRiskRow
fn on_activate_2(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Enter is the deliberate "open this" (R11.4), and in Review view the
        // thing opened is the diff — so it goes there, exactly as a click does.
        // Moving the selection is browsing and keeps the list.
        Event::Activate if state.view == View::Review => match state.tree_selection.clone() {
            Some(path) => {
                next.focus = Pane::Editor;
                vec![Effect::ReadDiff(path)]
            }
            None => vec![],
        },

        // Enter on a Risk row goes to the code behind the figure. The same
        // effect the Story jump and a project-search hit use — a place in a
        // file is one thing to open, however the reader got to it — and focus
        // follows, because Enter is the deliberate "take me there".
        //
        // Ahead of the spine's arm below: with the keyboard in this pane the
        // pane it is in decides what Enter means, not the view around it.
        // A switch, not a read: the buffer is already held, so nothing is
        // opened from disk and no effect is returned — the same `ShowBuffer`
        // the dot strip raises, so the two ways to reach a buffer cannot drift.
        // Focus follows into the editor because Enter is the deliberate "take
        // me there", which is the same rule the Risk list's Enter keeps below.
        Event::Activate if state.focus == Pane::Buffers => match buffer_selected(state).cloned() {
            Some(path) => {
                let (mut shown, effects) = update(&next, Event::ShowBuffer(path));
                shown.focus = Pane::Editor;
                return Ok((shown, effects));
            }
            None => vec![],
        },

        // Enter on a history row goes back to that place, through the same
        // `history::go` the row's action and a click on it reach: one way to
        // the jump, so the three cannot drift. Focus follows into the editor
        // because Enter is the deliberate "take me there".
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

        // Enter opens or closes the row the keyboard is on: the one gesture
        // that walks the tree, which the click below goes through.
        Event::Activate if state.focus == Pane::Variables => {
            debug::open(&mut next, state.variables_selection)
        }

        // The file opened if it is not, and the cursor put on the
        // Breakpoint's line either way — the Risk list's Enter below, for a
        // line rather than a Function.
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

        // A Landing on the Diagnostic's start, through the `OpenAt` every
        // Jump to a place in a file takes, so it records a Visit.
        Event::Activate if state.focus == Pane::Diagnostics => {
            match lsp::landing(state, state.diagnostics_selection) {
                Some((path, at)) => {
                    next.focus = Pane::Editor;
                    vec![Effect::OpenAt { path, at }]
                }
                None => vec![],
            }
        }

        // A Landing on the Conflict's first marker line, and so a Jump.
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

        // Straight through the arm Enter takes, the way a clicked palette row
        // goes through the arm its key takes: a click on a row is the same
        // gesture, so a second mapping is a second place for it to drift.
        //
        // The focus is re-asserted *after* that arm, exactly as `on_click_row`
        // re-asserts the tree's: R10.1 says a click focuses what you clicked,
        // and the arm Enter takes ends `next.focus = Pane::Editor` because Enter
        // is the deliberate "take me there". Delegating without this line was a
        // click that set the focus and immediately gave it away, so the only way
        // to put the keyboard in this pane with the mouse was to click its
        // border — the one part of it that is not a row.
        Event::ClickRiskRow(index) => {
            next.focus = Pane::Risk;
            next.risk_selection = index;
            let (mut opened, effects) = update(&next, Event::Activate);
            opened.focus = Pane::Risk;
            return Ok((opened, effects));
        }

        // The same shape again, and for the same reason.
        Event::ClickHistoryRow(index) => {
            next.focus = Pane::History;
            next.history_selection = index;
            let (mut opened, effects) = update(&next, Event::Activate);
            opened.focus = Pane::History;
            return Ok((opened, effects));
        }

        // The same shape as the Risk list's click, and for the same reason.
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

        // An edit to the Buffer and nothing else: nothing is saved, and
        // nothing is ever staged.
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

        // Through the arm Enter takes, for the reason the corner's rows go
        // through theirs: a click on a row is the same gesture, and a second
        // mapping is a second place for it to drift. The focus is not given
        // away afterwards, since opening a row leaves the keyboard in the
        // tree it opened.
        Event::ClickVariablesRow(index) => {
            next.focus = Pane::Variables;
            next.variables_selection = index;
            debug::open(&mut next, index)
        }

        // The keyboard follows the Strip when it was already in it — the pane
        // it was in is the one going off screen, and keys landing in a pane
        // nobody can see is the failure focus exists to prevent. From
        // anywhere else it stays where it was: showing a group is not asking
        // to leave the file being read.
        Event::ShowGroup(group) => {
            next.strip = group;
            if state.strip.holds(state.focus) {
                next.focus = group.pane();
            }
            vec![]
        }

        // Nothing is stopped and nothing is resized: the pty keeps its rows
        // and everything it printed, and `output_width` keeps the border where
        // the reader left it, so showing it again brings back what was there.
        Event::ToggleOutput => {
            next.output_hidden = !state.output_hidden;
            // Showing it is asking to see it, so the group it lives in comes
            // forward — the Chip is clickable from the Debug group alone, but
            // `␣h` is a key like the F-keys and reaches from anywhere.
            if state.output_hidden {
                next.strip = layout::Group::Debug;
            }
            vec![]
        }

        // Out of sight it is marked where the reader will look — the `Debug`
        // Group tab and the Chip that brings it back. On screen it has already
        // been read, so it marks nothing; `settle` is what clears the mark,
        // for the reason it clamps the scrolls.
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

        // The same shape, and for the same reason: a click on a row is the
        // gesture Enter is, and the focus is re-asserted after that arm has
        // given it to the editor because R10.1 says a click focuses what you
        // clicked.
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

/// Activate
fn on_activate_3(state: &State, next: State, event: Event, _wheeled: bool) -> Answered {
    match event {
        // Enter on the spine's selected row is how a reviewer walks a
        // Story — the keyboard path `EnterStory` exists for.
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

/// Activate
fn on_activate_4(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Activate => match selected_row(state) {
            Some(row) if row.is_dir && row.expanded => {
                next.expanded.remove(&row.path);
                vec![]
            }
            Some(row) if row.is_dir => vec![Effect::ReadFolder(row.path)],
            // Opening a file means you want to read it, so go there. Expanding
            // a folder means you are still browsing, so stay.
            Some(row) => return Ok(open_file(next, row.path)),
            None => vec![],
        },

        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// Bytes
fn on_bytes(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        // Keys reach the pane that has focus. A pane with no handler swallows
        // them rather than letting them run into the shell.
        Event::Bytes(bytes) => match state.focus {
            // With no session running the AI pane is an input box, so its keys
            // belong to that — never to the shell.
            Pane::Ai if !state.ai_running => vec![],
            // And with no program started the Debug group is the Variables
            // alone, so there is no child there for a key to reach.
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

/// Pasted, RowAction
fn on_pasted(state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Pasted(text) => {
            // Which child is told, and whether it asked to be told that a paste
            // is a paste. With no session running the AI pane is an input box,
            // so its paste belongs to that — never to the shell.
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

        // The first row action that asks an AI session rather than producing a
        // shell string, so it takes the hand-off a submitted review and a
        // confirmed story take — including holding the prompt until the CLI has
        // spoken. One prompt and nothing else: no Iteration, no tests, no Gate,
        // and nothing committed. The gated loop is `refactor_loop.feature`.
        Event::RowAction(risk::REFACTOR) => match risk::selected(state) {
            Some(function) => {
                queue_for_ai(&mut next, Enter::Pressed, risk::refactor_prompt(function))
            }
            None => vec![],
        },

        // The one action a history row has, straight through the same function
        // Enter and a click reach. The keyboard stays in the pane: an action
        // run on a row is not the deliberate "take me there" that Enter on the
        // row itself is.
        Event::RowAction(history::GO_TO) => return Ok(history::go(state, next)),
        // The Variables' row Chips, each straight through to what its key
        // does, for the reason the Transport's are one event: a click and a
        // key are one gesture.
        Event::RowAction(debug::SET_VALUE) => {
            // Read off the Chip itself rather than re-deciding: a dimmed Chip
            // does nothing, and a box that opened over an adapter that will
            // not take the value is a box that refuses at Enter.
            if offers(
                &debug::row_chips(state, state.variables_selection),
                debug::SET_VALUE,
            ) {
                next.modal = Modal::SetValue;
            }
            vec![]
        }
        // One value, by the path the Evaluator would take to it, and left
        // unsubmitted for the reason the whole snapshot is. Read off the Chip,
        // as set-value is: dimmed, it does nothing.
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
        // The row's evaluate Chip and the Hover's each open the Evaluator on
        // their own expression, which is why they are two names rather than
        // one: the row's is the path to a member and the Hover's is what the
        // pointer was resting on.
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
        // `\u{2423}e`: the expression is the core's to read off the cursor or
        // the Selection, which is why the chord carries none.
        Event::OpenEvaluator => {
            debug::open_evaluator(&mut next, debug::cursor_expression(state));
            vec![]
        }
        // Normal-mode Enter, the Run Chip and Ctrl+Enter are one gesture, so
        // they are one event and one arm.
        Event::RunSnippet | Event::RowAction(debug::RUN) => debug::run(&mut next),
        // The window's rectangle, from wherever a gesture took it: the mouse
        // names it whole and the keyboard a cell at a time. Unclamped on the
        // way in — `settle` places it on the screen and off the Paused line,
        // which is the one answer to where a window may be — and saved, the
        // way every other dragged edge in Varde is.
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
        // The mode the letters below act in, and the key that leaves it —
        // Stepping mode's shape, for its reason: nobody may be trapped in a
        // mode, so any key the mode does not claim leaves it and then does
        // what it always did.
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

/// ClickPaletteEntry, RowAction
fn on_row_action(state: &State, next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::RowAction(name) => {
            let action = row_action(name);
            match selected_target(state) {
                Some(target) => return Ok(update(state, Event::Trigger(action, Some(target)))),
                // A project with no row to stand on still has its root, which is
                // the only folder every project is guaranteed to have.
                None if matches!(action, Action::NewFile | Action::NewDirectory) => {
                    let root = Target::Folder(PathBuf::new());
                    return Ok(update(state, Event::Trigger(action, Some(root))));
                }
                None => vec![],
            }
        }

        // Straight through the arm the keystroke takes: the row and the letter
        // beside it are one gesture, and a second mapping is what left every
        // row but the three views doing nothing when clicked. Guarded rather
        // than assumed open: the mouse hit-tests the palette's rows only while
        // it is up, and a core that force-opened it here would answer a click
        // nobody could have made.
        Event::ClickPaletteEntry(_) if !matches!(state.modal, Modal::Palette | Modal::Chord) => {
            vec![]
        }
        Event::ClickPaletteEntry(key) => return Ok(update(state, Event::Key(key))),
        other => return Err((next, other)),
    };
    Ok(settle(next, effects, wheeled))
}

/// Pins the comment to the revision that was reviewed, so it still points at
/// what the reviewer actually saw.
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

/// The Story name and 1-based Step position a comment made right now would
/// argue with, or neither while walking the Remainder or not walking at all
/// — the Remainder has no claim, and Review view has no Story.
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

/// Pressed, Ctrl+U clears the line the CLI is showing — readline's convention,
/// and best-effort by nature, which is why submitting confirms first. The
/// prompt then goes in as one paste, so its line breaks survive and the Enter
/// riding along is not read as a submit at every newline. Withheld, the paste
/// is all that is sent.
fn injection(prompt: &str, paste: keys::Paste, enter: Enter) -> Vec<u8> {
    match enter {
        Enter::Pressed => {
            let mut bytes = b"\x15".to_vec();
            bytes.extend(paste_bytes(prompt, paste));
            bytes.push(b'\r');
            bytes
        }
        // Nor is the line cleared first: what the user had begun typing is
        // theirs, and the paste lands after it.
        Enter::Withheld => paste_bytes(prompt, paste),
    }
}

/// Whether a prompt handed to the AI is submitted, or left in its prompt for
/// the user to send. A Pause snapshot is left: variable values can be real
/// customer data, so whether they leave the machine is the user's Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enter {
    Pressed,
    Withheld,
}

/// Whether `chips` offer `action` undimmed. An action is read off its Chip
/// rather than decided a second time: a dimmed Chip does nothing.
fn offers(chips: &[Chip], action: &str) -> bool {
    chips
        .iter()
        .any(|chip| chip.action == action && chip.tone != Tone::Dimmed)
}

/// Marked as a paste when the child asked to be told a paste from typing. The
/// end marker is stripped out first, because the text is untrusted: one inside
/// it would close the bracketing early and hand the rest to the child as keys.
fn paste_bytes(text: &str, paste: keys::Paste) -> Vec<u8> {
    match paste {
        keys::Paste::Bracketed => {
            format!("\x1b[200~{}\x1b[201~", text.replace("\x1b[201~", "")).into_bytes()
        }
        keys::Paste::Bare => text.as_bytes().to_vec(),
    }
}

/// Queues a prompt for the AI pane, submitted or left for the user to send,
/// starting a session first when none is running. Every hand-off goes through
/// the one pane (ADR 0006), so all of them wait behind the same
/// `pending_prompt` for `AiSpoke` when the CLI has not printed anything yet.
pub(crate) fn queue_for_ai(next: &mut State, enter: Enter, prompt: String) -> Vec<Effect> {
    // Text never goes into a session nobody can see.
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

/// Where submitted reviews are kept. A project keeps its own in
/// `.varde/reviews/` — but a Bare workspace's `.varde` is the Sidecar, deleted
/// at exit, and a review written there is the output of the reading thrown
/// away with it. So it goes to `~/.varde/reviews/` instead: durable, outside
/// every workspace, under the same retention
/// (`docs/adr/0016-a-bare-workspace-leaves-nothing-behind.md`). This is the one
/// thing Varde writes that does not route through [`varde_dir`], and the only
/// one that may not.
///
/// The edge reads it too, for the numbers already there. It has to: one global
/// directory means a Bare workspace numbering from nothing writes `0001.json`
/// over the review somebody submitted from another folder — which is the loss
/// this whole feature exists to prevent.
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
    // How the prompt spells it: relative in a project, because the prompt is
    // read against the session's own working directory, and absolutely from a
    // Bare workspace, whose folder is the user's and holds no `.varde` to
    // resolve against.
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

/// How a file stands in the buffer list, for the tree marks and the dot strip.
/// Unsaved work is a different mark, not a different colour of the same one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    None,
    Open,
    Dirty,
    Current,
    CurrentDirty,
}

/// Every open buffer, in the order the dot strip draws them — one list, so the
/// pane's rows and the dots cannot come to be in different orders.
pub fn buffer_list(state: &State) -> Vec<&PathBuf> {
    state.buffers.keys().collect()
}

/// The buffer the Buffers pane's highlight is on, if the list has a row there.
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

/// What keys will do to the buffer on screen, for the pane title. Walking a
/// Story claims every editor key ahead of the vim arms, so the buffer's own
/// mode is not what the next keystroke obeys — saying `normal` there advertises
/// an editing mode the reviewer is in no position to use. A decision, not a
/// rendering detail, which is why it is here and not in `ui`.
pub fn mode_label(state: &State, buffer: &editor::Buffer) -> &'static str {
    if state.walking.is_some() {
        return "read-only";
    }
    // Same reasoning, one line down: a Guest repo's file refuses every key
    // that would change it, so naming the buffer's own mode there advertises
    // an editing mode nobody can use.
    if let (Some(guest), Some(open)) = (&state.guest, &state.current_buffer) {
        if open.starts_with(guest) {
            return "read-only";
        }
    }
    // The same reasoning one line down: a Preview obeys no editing key, so
    // naming the buffer's mode there advertises one that is not on offer — and
    // the title is where a reader finds out which of the two they are looking
    // at without having to guess from the absence of line numbers.
    if previewing(state) {
        return "preview";
    }
    // Ahead of the buffer's own mode, because it is the mode the next
    // keystroke obeys: in Stepping mode `n`, `i`, `o` and `c` drive the
    // program rather than the buffer, and the title is where a reader finds
    // out which of the two they are typing at. Behind the three above for the
    // reason they are there at all: a surface that refuses every edit is the
    // more useful thing to say, and Stepping mode adds keys rather than
    // taking any away.
    if state.stepping {
        return "stepping";
    }
    buffer.mode.as_str()
}

/// On the way out, from every gesture that leaves — `:qa`, `:qa!` and `:wq`
/// all route through here, so a fourth cannot save what the other three
/// delete. A project writes its state; a Bare workspace deletes its Sidecar
/// instead and writes nothing, because state saved into a directory removed
/// one effect later is the same worthless write ticket 04 stopped seeding.
/// A crash reaches neither, which is what the sweep on start is for.
///
/// The Reading's stream is one of the two things deleted twice on purpose: the
/// player exiting is the normal path, and this is the only one that runs after
/// a Reading was interrupted by quitting (ADR 0014). Unconditional — the edge
/// holds whether there is a stream at all, and a `StopSpeaking` with nothing
/// to stop costs a match arm doing nothing.
fn leaving(state: &State) -> Vec<Effect> {
    let stop = Effect::StopSpeaking;
    match &state.sidecar {
        Some(sidecar) => vec![stop, Effect::DeleteDir(sidecar.clone()), Effect::Exit],
        None => vec![stop, Effect::SaveState(state_json(state)), Effect::Exit],
    }
}

/// Restarting, with `Relaunch` where it would `Exit`: a relaunch discards what a
/// quit discards, so it is refused by the same refusal rather than a second one.
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

/// What Varde remembers about a project between sessions.
pub(crate) fn state_json(state: &State) -> String {
    let expanded: Vec<String> = state
        .expanded
        .iter()
        .filter_map(|path| path.strip_prefix(&state.root).ok())
        .map(|rest| rest.to_string_lossy().into_owned())
        .collect();
    // Minus the preview, whose whole nature is being replaced by the next one:
    // remembering it would open next session on whatever the tree selection
    // last passed over, which nobody asked to see (F19).
    let relative = |path: &std::path::Path| {
        (Some(path) != state.preview.as_deref())
            .then(|| path.strip_prefix(&state.root).ok())
            .flatten()
            .map(|rest| rest.to_string_lossy().into_owned())
    };
    let buffers: Vec<String> = state.buffers.keys().filter_map(|p| relative(p)).collect();
    // The text a Breakpoint was set against, so the next start can tell a
    // line that still holds it from one that does not.
    let breakpoints: Vec<serde_json::Value> = state
        .breakpoints
        .iter()
        .filter_map(|breakpoint| {
            let mut saved = serde_json::json!({
                "file": breakpoint.file.strip_prefix(&state.root).ok()?,
                "line": breakpoint.line,
                "text": breakpoint.text,
            });
            // Only what was set, so a plain Breakpoint is recorded as it
            // always was.
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

/// The tree follows the buffer, so the two views never disagree about where
/// you are. A file in a folder nobody opened is not a row at all, so every
/// folder on the way is expanded — and read, where its entries are not known
/// yet, since `tree::rows` walks only into `contents`.
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

/// Copying reaches the system clipboard, or asks the terminal to hold it
/// (OSC 52) when there is none — which is what makes copying work over SSH.
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

/// The new-file line numbers a visual selection in the diff covers.
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

/// Opening replaces the one buffer Varde holds, so unsaved edits would be
/// gone. Refuse instead, the same way quitting does.
fn open_file(mut next: State, path: PathBuf) -> (State, Vec<Effect>) {
    // Opening keeps what is already open, so nothing is discarded and the
    // previewed buffer simply stays.
    if next.preview.as_ref() == Some(&path) {
        next.preview = None;
    }
    next.focus = Pane::Editor;
    (next, vec![Effect::OpenBuffer(path)])
}

/// The one way to change view. Switching to the view already showing is a
/// no-op, which is why arriving is [`enter_view`] and not this.
fn switch_view(state: &State, view: View) -> (State, Vec<Effect>) {
    if view == state.view {
        return (state.clone(), vec![]);
    }
    enter_view(state, view)
}

/// What arriving in a view loads, whether it was switched to or started in, so
/// every route into Review lands on the first changed file with its diff
/// loading and every route into Story reads the story sets.
pub(crate) fn enter_view(state: &State, view: View) -> (State, Vec<Effect>) {
    if view == View::Review {
        return update(state, Event::OpenReviewView);
    }
    let mut next = state.clone();
    move_to_view(&mut next, view);
    let mut effects = vec![Effect::RenderView(view)];
    // A review-scoped figure does not survive the view it was measured for: the
    // number on the border describes the Scope on screen, and counting the
    // reviewed files' Functions as though they were the workspace's is the same
    // wrong number the other way round. Dropped, and the workspace measured
    // again.
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

/// Every folder Varde has to hear about changes in — the decision, so the edge
/// only has to add and drop watches to match it.
///
/// Folders, never files: one watch covers everything in a directory, and a
/// watch on a file is lost the moment a tool replaces it by rename instead of
/// writing through it, which is how most editors and AI CLIs save.
///
/// The tree's expanded folders are not enough. What is *open* also has to be
/// followed, and neither an open buffer nor the file a diff is shown for needs
/// its folder expanded: Review view lists changed files flat, so a diff of
/// `src/tree.js` was watched only if the tree happened to have `src` open — and
/// collapsing a folder stopped a buffer from following its file at all.
pub fn watched_folders(state: &State) -> BTreeSet<PathBuf> {
    let mut folders = BTreeSet::new();
    folders.insert(state.root.clone());
    // Git's own bookkeeping, for the branch and status the review list reads.
    folders.insert(state.root.join(".git"));
    // Always, not only while authoring (ADR 0006): the artifact can land any
    // time after the prompt is sent.
    folders.insert(varde_dir(&state.root, state.sidecar.as_deref()).join("stories"));
    // The Refactor loop's completion sentinel lands here, and the wait for it
    // has no timeout — a folder nobody watches is a loop that never finishes.
    folders.insert(varde_dir(&state.root, state.sidecar.as_deref()));
    // The global config is edited while Varde runs, and it lives outside the
    // workspace.
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

/// The one place the view changes. Each view gets back the buffer it was
/// showing, unless it has been closed since. A diff belongs to Review view:
/// leaving it without clearing would leave the editor showing a read-only diff
/// of a file you are trying to edit.
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

/// A path as the workspace names it — the spelling a Site is authored with, a
/// Visit records and the tree, the buffer title and every notice show. Outside
/// the root it stays whole: a file opened from elsewhere is not the root's to
/// rename, and dropping it would be silence where a name was asked for.
///
/// One copy. There were four, two of them written a commit apart in modules
/// neither of which could see the other's — which is the rule of three coming
/// due and being answered with a fourth copy. The sites that shorten a path and
/// then *drop* the ones outside the root are deliberately not these: a hit list
/// and a definition target are the root's or they are nothing, so they keep
/// their own `Result`.
pub fn relative(state: &State, path: &Path) -> String {
    path.strip_prefix(&state.root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Which pane sits in the given direction. Geometry, which is why the Risk
/// list needs no binding of its own: it is under the tree when it is on screen
/// and nowhere at all when it is not, so the same downward gesture reaches the
/// shell either way.
fn neighbour(state: &State, direction: Direction) -> Pane {
    // Whichever pane the corner is holding, or none: the geometry is the
    // corner's, not any one occupant's, so a second pane in that slot is
    // reached by the same gestures without a second set of arms.
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
        // The way back in, and the reason every direction is written both ways
        // round: the shell starts where the corner ends, so a pane that could be
        // left sideways and not re-entered was a list only the tree above it
        // could reach — and the tree is two panes away from the shell.
        (Pane::Terminal, Direction::Left) => corner.unwrap_or(Pane::Terminal),
        (Pane::Terminal, Direction::Up) => Pane::Editor,
        (unchanged, _) => unchanged,
    }
}

/// What the row the keyboard is on offers, in whichever pane holds the
/// keyboard. Both lists reach `Event::RowAction`, so stepping into the icons
/// with the arrows and running one with Enter is the same gesture in the Risk
/// list as it is in the tree — a second mechanism for the second pane is a
/// second place for it to drift.
fn selected_row_actions(state: &State) -> Vec<&'static str> {
    // Past the last Risk row the icons the arrows step into are the pane's own,
    // not a row's — same gesture, same field, one slot further down.
    if risk::on_actions(state) {
        return risk::pane_actions(state);
    }
    if state.focus == Pane::Risk {
        return risk::row_actions(state);
    }
    // The Buffers pane has none — its rows name a buffer to switch to and
    // nothing to do to one — and falling through would offer the tree's
    // instead, which is the row under a selection nobody can see from here.
    if state.focus == Pane::Buffers {
        return Vec::new();
    }
    // Its rows have one, and it is the only pane in the corner that does: a
    // Visit is somewhere to go, so going there is a thing to do to a row.
    if state.focus == Pane::History {
        return history::row_actions(state);
    }
    if state.focus == Pane::Breakpoints {
        return debug::row_actions(state);
    }
    // The one pane whose row actions are Chips rather than bare icons: the
    // names come off the Chips so the arrows, the click and the renderer read
    // one list, and only the dimming is `ui`'s to draw.
    if state.focus == Pane::Variables {
        return debug::row_chips(state, state.variables_selection)
            .into_iter()
            .map(|chip| chip.action)
            .collect();
    }
    match state.tree_selection.as_deref() {
        Some(path) => tree::row_actions(state, path),
        None => Vec::new(),
    }
}

/// Which mouse-report encoding the child in a pane asked for. Exhaustive: a
/// pane that hosts no child has no child to tell.
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

/// Whether the Program output is on screen: the Debug group up, a pty behind
/// it, and not hidden. One answer, read by the layout, by the mark that says
/// output arrived unseen and by the Chip that brings it back, so the three
/// cannot disagree about whether the reader can see it.
pub fn showing_output(state: &State) -> bool {
    state.strip == layout::Group::Debug && state.output_running && !state.output_hidden
}

/// One Group tab as it is drawn. Lit and marked are two facts rather than one
/// [`Tone`]: the Debug group can be up with the Program output hidden behind
/// it, which is a tab that is both, and a single tone would have to drop one
/// of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tab {
    pub group: layout::Group,
    /// Whether this is the group the Strip is showing.
    pub lit: bool,
    /// Whether it holds output the reader has not seen.
    pub unseen: bool,
}

/// The Group tabs on the Strip's top border, in the order they are drawn. The
/// Debug group only while a session exists, for the reason its chords are
/// offered only then: a tab that shows an empty Strip is a tab that lies.
pub fn group_tabs(state: &State) -> Vec<Tab> {
    [layout::Group::Shells]
        .into_iter()
        .chain(state.debug.as_ref().map(|_| layout::Group::Debug))
        .map(|group| Tab {
            group,
            lit: group == state.strip,
            // Only the group the Program output lives in has anything unseen
            // to say: the shells are the reader's own and nothing marks them.
            unseen: group == layout::Group::Debug && state.output_unseen,
        })
        .collect()
}

/// What the panes take out of the shell, as `state` has it: the AI pane's
/// shape, the corner's occupant, the Strip's group and its height. One answer
/// for the five places that ask `layout::panes` where the panes are — the
/// renderer, the mouse, the two clamps here and the World — because the four
/// travel together and a fifth of them added to one caller and not the others
/// is a pane drawn where nothing hit-tests it.
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

/// Whether the Strip's Transport is on screen. The Debug group's border
/// carries it; with the Shell group up — where a session that ended leaves the
/// Strip — the lone restart Chip is still drawn, because a control reachable
/// only while the thing it restarts is running is one nobody can press. One
/// answer for `ui`, which draws it, and `mouse`, which hit-tests it: two would
/// be a click landing on a Chip nobody can see.
pub fn showing_transport(state: &State) -> bool {
    state.strip == layout::Group::Debug || state.debug.is_none()
}

/// Where the Variables' Transport is drawn and hit-tested: the Strip's own
/// rectangle, less the columns the Group tabs keep at its right-hand end. One
/// derivation, for the reason [`layout::chip_labels`] is one — two would be a
/// Chip drawn where nothing clicks it.
pub fn transport_area(state: &State, strip: layout::Area) -> layout::Area {
    layout::Area {
        width: strip
            .width
            .saturating_sub(layout::strip_width(&group_labels(state))),
        ..strip
    }
}

/// The Group tabs as they are drawn, padded a column each side: `ui` draws
/// these and `mouse` hit-tests them through `layout::strip_at`, so the two
/// cannot disagree about which columns a tab is in — the reason a Transport's
/// Chip labels are built in one place too.
pub fn group_labels(state: &State) -> Vec<String> {
    group_tabs(state)
        .iter()
        .map(|tab| format!(" {} ", tab.group.label()))
        .collect()
}

/// Where the panes are, for the two things the core measures against them: how
/// many rows a list shows, and how many rows the Risk list shows. `ui` derives
/// its own rectangles from the same function — one layout, so a clamp and a
/// drawn pane can never disagree about how tall it is.
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

/// How many rows a pane in the Strip shows: its two border rows and nothing
/// else, for the reason [`corner_rows`] is the one answer for every occupant
/// of the corner.
pub fn strip_rows(state: &State) -> usize {
    panes_of(state).terminal.height.saturating_sub(2) as usize
}

/// How many Cheatsheet rows the AI pane's rectangle shows: its two border
/// rows and nothing else.
pub fn cheatsheet_fits(state: &State) -> usize {
    panes_of(state).ai.height.saturating_sub(2) as usize
}

/// How many rows the pane in the corner shows. One answer for every occupant,
/// because the corner is one rectangle and every pane in it spends the same
/// chrome on it: its two border rows, and nothing else — everything the Risk
/// list says about the figure is on its border, because the pane is narrower
/// than a path and rows inside the borders are for rows. Public because "the
/// selection is in view" is a claim about this number, and a scenario
/// recomputing it would be asserting its own arithmetic.
pub fn corner_rows(state: &State) -> usize {
    panes_of(state).corner.height.saturating_sub(2) as usize
}

/// How many rows of results the search box shows. Through `layout`, which owns
/// the box's rectangle: measuring it here as well is how a list comes to be
/// clamped against a box of another size.
fn search_rows(state: &State) -> usize {
    layout::search_hit_rows(state.screen_width, state.screen_height)
}

/// How many rows of a list each pane shows: the tree and the editor, less their
/// borders — and less the filter box that sits on the tree's bottom edge in Edit
/// view, whose two rows the list never gets. Measuring this as the pane height
/// is what left the last two files in a folder unreachable.
///
/// Public because a hover box is wrapped and placed against the same editor
/// pane the clamp measures — two derivations of one pane's size is how a box
/// comes to be drawn where it does not fit — and, for the reason
/// [`corner_rows`] is, because "the whole Site is in view" is a claim about the
/// second of these numbers.
pub fn fits(state: &State) -> (usize, usize, usize) {
    fits_in(state, &panes_of(state))
}

/// The same three counts against a layout the caller already has. `mouse` is
/// handed the rectangles the renderer drew and hit-tests against those and
/// never a second layout of its own — the one-layout rule — so a drag asks how
/// many rows it can run out of through here. Both entry points share the
/// arithmetic: two derivations of a row count is a drag that stops one row off.
pub fn fits_in(state: &State, panes: &layout::Layout) -> (usize, usize, usize) {
    let filter = tree::filter_rows(state.view) as u16 + story::title_rows(state) as u16;
    (
        panes.tree.height.saturating_sub(2 + filter) as usize,
        panes.editor.height.saturating_sub(2) as usize,
        // Borders, the gutter and the minimap are not text: a pane 26 columns
        // wide shows 19 characters of a line, and clamping against 26 leaves
        // the last seven permanently under the pane next door. The mirror's
        // columns go the same way — text clamped as if they were its own is
        // text the mirror is drawn over.
        panes
            .editor
            .width
            .saturating_sub(2 + gutter(state) + minimap::width(state)) as usize,
    )
}

/// Which **row** of the editor pane holds the focus and how many rows there
/// are: the diff when one is open, otherwise the buffer. Both scroll the same
/// pane, and `editor_scroll` is the first row shown — so a surface that draws
/// more rows than it has lines has to say so here, or the clamp keeps the
/// cursor "visible" one row past the bottom and the last line is unreachable.
/// Story view is that surface: it draws a comment row under the lines they
/// cover.
fn editor_focus(state: &State, rows: &[preview::Row]) -> (usize, usize) {
    // An old-side Step draws a refusal where its code would be, so there is no
    // focus row and nothing to scroll. Answered here rather than by the
    // renderer subtracting its own offset: `mouse` hit-tests the editor against
    // this same clamped field, and two derivations of one offset are how a
    // click comes to land on a row nobody pointed at.
    if story::refused(state) {
        return (0, 0);
    }
    // A Preview's rows are not its lines, so the clamp is handed the row the
    // cursor is on and the number of rows there are — read off the map rather
    // than derived, which is the whole of ADR 0007 at this call site.
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

/// What the editor pane's sideways offset answers to.
enum Sideways {
    /// A cursor the offset follows: which column the caret is on and how long
    /// the text it is on runs, both 0-based and in characters. A Buffer shown
    /// as Source, and a Preview — whose column is a column of the **rendered**
    /// row, so following it slides to the character the reader is looking at
    /// (ADR 0007, as amended).
    Cursor { column: usize, width: usize },
    /// A surface read rather than typed in — Review's diff and the code a
    /// Story walk is showing. Neither has a cursor the offset may follow: a
    /// diff's cursor is a row, and a walked Site's keys never reach the buffer
    /// at all. The pin is on *following a cursor*, not on the offset existing:
    /// the gesture moves it and [`slid_width`] stops it.
    Read,
}

/// `rows` is the Preview's, already laid out by the caller — a markdown parse
/// per call is what ADR 0007 says is visible rather than merely wasteful, and
/// `settle` has them in hand.
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

/// How far right the surface has text, in characters and without the gutter —
/// what stops a slide rather than letting it run on into empty space. Every
/// surface with an offset, not only the ones [`sideways`] answers `Read` for:
/// the wheel moves the offset on a Source buffer and a Preview too, and it is
/// bounded by the same width the keyboard slide is.
///
/// The whole surface, not the rows in view. A slide clamped to what is on
/// screen would be pulled home by a `j` off the end of a wide hunk, which is
/// the offset moving on its own — and the vertical clamp bounds
/// `editor_scroll` against the whole row count for the same reason.
fn slid_width(state: &State, rows: &[preview::Row]) -> usize {
    // An old-side Step draws a notice where its code would be, and there is
    // nothing past the edge of a sentence. Excluded for the reason
    // [`editor_focus`] excludes it: the buffer behind that pane is open and
    // its cursor is somewhere, so a rule that consulted it would slide a
    // notice about code nobody can see.
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
    // A Preview's rows are not its lines — the markdown behind a reflowed
    // paragraph, a link that hides its URL or a table is a different width from
    // what is drawn, and the offset is measured against what is drawn. The rows
    // are the caller's, already laid out: a markdown parse per call is what
    // ADR 0007 says is visible rather than merely wasteful.
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

/// The name a held jump modifier is over, as the line and the 1-based columns
/// it covers — what the renderer underlines to say a click here jumps. Nothing
/// when the pointer is on blank space or punctuation: a place with no name
/// under it has no definition to ask about, so nothing there is a link.
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

/// How wide the strip left of the editor's text is, for what is on screen.
/// One answer for the renderer, the mouse hit-test and the scroll clamp: each
/// asked `layout::gutter(previewing(state))` for itself, which is how Review's
/// diff — the one surface that draws a marker column as well as line numbers —
/// came to be measured as if it had only the numbers.
pub fn gutter(state: &State) -> u16 {
    layout::gutter(match (previewing(state), state.diff.is_some()) {
        (true, _) => layout::Gutter::None,
        (_, true) => layout::Gutter::NumbersAndMarker,
        _ => layout::Gutter::Numbers,
    })
}

/// Whether the editor pane is showing a rendered document rather than the
/// characters the file holds. Composite on purpose: previewing is a fact about
/// the buffer, but Review's diff and Story view's code surface substitute the
/// whole pane before ever reaching one — so `ui`, `mouse` and the suite ask
/// here instead of each remembering the two views that do not have Previews.
pub fn previewing(state: &State) -> bool {
    state.diff.is_none()
        && state.walking.is_none()
        && current_buffer(state).is_some_and(|buffer| buffer.previewing)
}

/// The document the open buffer holds, laid out to the editor pane.
///
/// Derived, never stored: rows in `State` would have two authors and diverge,
/// which is the `ai_running` failure `AGENTS.md` documents. The width comes
/// from the screen size `State` already carries, so no caller passes one and
/// none can disagree about it.
pub fn preview_rows(state: &State) -> Vec<preview::Row> {
    if !previewing(state) {
        return Vec::new();
    }
    buffer_rows(state)
}

/// The current buffer laid out to the pane, with no requirement that its
/// Preview is what the screen is showing right now — crossing reads this
/// while a diff or a Story walk is covering the pane, since a buffer's own
/// choice to preview does not depend on either. `preview_rows` is this plus
/// that requirement, for every other caller.
fn buffer_rows(state: &State) -> Vec<preview::Row> {
    let Some(buffer) = current_buffer(state) else {
        return Vec::new();
    };
    // Whatever the buffer holds — the draft if there is one, disk otherwise —
    // so unsaved edits, watcher reloads and divergence all inherit behaviour
    // that is already specified rather than gaining a rule of Preview's own.
    preview::rows(buffer.shown(), preview_columns(state))
}

/// How many columns of text a Preview lays out to: the editor pane's inside,
/// with no gutter taken off it. A Preview draws no line numbers, so those five
/// columns are text — and `layout::gutter` is the one place that says so.
///
/// Public because the edge caches the render against `(revision, pane width)`
/// and cannot know the second half of that key otherwise. Asking here is what
/// stops the cache from being keyed on a width the layout does not agree with.
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
    // Borders only. A Preview has no gutter at all — `layout::gutter` is where
    // that is said once, for the renderer and the hit-test both.
    panes.editor.width.saturating_sub(2) as usize
}

/// Opens the comment box over a line range, with an empty body ready to type
/// into. Three gestures reach it — a gutter drag, `c` on a diff selection and
/// `c` on a walked Step — and the body has to arrive with the modal, so this is
/// one function rather than three places that each have to remember it.
fn open_comment_box(next: &mut State, file: String, from: u32, to: u32) {
    next.gutter = Some((file, from, to));
    next.modal = Modal::Comment;
    let mut body = Buffer::open("", false, next.tab_width);
    // A text box is typed into the moment it opens: there is no normal mode
    // here to leave first, and no file to move around in.
    body.mode = editor::Mode::Insert;
    next.comment = Some(body);
}

/// Puts back every Guest repo buffer this event left holding a draft, and says
/// whether it put any back. See the call in [`update`].
fn refuse_guest_edits(state: &State, next: &mut State) -> bool {
    let Some(guest) = &state.guest else {
        return false;
    };
    let mut refused = false;
    for (path, clean) in &state.buffers {
        // `get`, not indexing: an event that closed the buffer took it out of
        // `next`, and closing one is not editing it.
        if !path.starts_with(guest) || !next.buffers.get(path).is_some_and(Buffer::is_dirty) {
            continue;
        }
        // As it stood: a Guest repo's buffer never holds a draft, so the one
        // the event arrived at is the clean one.
        next.buffers.insert(path.clone(), clean.clone());
        refused = true;
    }
    refused
}

/// The buffer the editor pane is showing. Public because the renderer asks it
/// too — which line the caret is on is one fact, and a second derivation of it
/// is a bright line number on a line nobody is on.
pub fn current_buffer(state: &State) -> Option<&Buffer> {
    state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
}

/// Where the word the cursor rests in also appears in the buffer, in reading
/// order — the word itself included, since the mark is about the word and not
/// about the other places. Whole words and case as written, which is what
/// keeps `state` out of `stated` and apart from `State`: the mark answers
/// "where is this name" and a substring is not the name.
///
/// Worked out on the spot for the reason [`matches`] is, and empty while a
/// Preview is up: rendered rows are not source, and a source column reported
/// over them names a place that is not on screen. Only in `lines`, for the
/// reason [`Buffer::lines_within`] gives.
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

/// Every match of the in-file query in the current buffer, in reading order —
/// what the editor highlights, and the places `n` and `N` step between. Worked
/// out on the spot rather than stored: an edit changes the contents and the next
/// call answers about the new ones, so a highlight cannot go stale. The matching
/// is the project searcher's, handed one file's lines and the search's own
/// [`search::Case`], so an untouched `[Aa]` behaves the same whether you are
/// finding here or everywhere.
///
/// Preview-aware rather than duplicated: while previewing, positions are in
/// **row** coordinates, searched over what `preview_rows` draws rather than
/// the source lines, so a marker the render consumed (a heading's `##`) is
/// never found, and a word only whole once markup is stripped is. Two
/// functions answering the same question is how the renderer and the mouse
/// came to disagree about where a click lands.
///
/// An empty query answers empty before laying anything out: every idle
/// Preview calls this every frame to know what to paint, and `preview_rows`
/// is not free — a mermaid pass on a query nobody typed is wasted work
/// `AGENTS.md`'s "never parse per frame" is aimed straight at.
///
/// Only in `lines` — rows, while previewing — for the reason [`Buffer::lines_within`]
/// gives: `n` and `N` ask for the whole file, the renderer for its window.
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

/// Which of `places` the cursor is inside, if any — the match a replace takes
/// and the one the count says you are on.
fn under_cursor(state: &State, places: &[Place]) -> Option<usize> {
    let cursor = cursor_place(state)?;
    let width = state.find.as_ref()?.query.shown().chars().count();
    places.iter().position(|at| {
        at.line == cursor.line && at.column <= cursor.column && cursor.column < at.column + width
    })
}

/// The in-file search's line on the editor's bottom border, piece by piece:
/// the query (with its caret while it is being typed), the count, and each
/// icon. `ui` draws these pieces and `mouse` hit-tests the same ones from the
/// border's first column, for the one-layout reason. Empty with no search on.
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

/// Which lines of the current buffer the last commit does not hold, 1-based:
/// the lines the gutter bars. Empty for a file the commit has no copy of, so
/// an untracked file is not a bar down every line.
pub fn changed_lines(state: &State) -> Vec<usize> {
    // The lines the one diff of the two sides has no commit for — the same
    // derivation the border's Authorship indexes through, so the gutter and the
    // border cannot disagree about which lines the commit holds.
    authorship::traced_lines(state)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter(|(_, at)| at.is_none())
        .map(|(index, _)| index + 1)
        .collect()
}

/// Every other occurrence of the picked word in the file, so a name picked once
/// is visible wherever else it is used without being searched for.
///
/// Exact, unlike [`matches`]: a pick is the text itself rather than a query
/// somebody typed, so `/`'s smart case would mark a word nobody picked.
/// Charwise and within one line — a pick spanning lines is a passage, and a
/// pty's or a Preview's pick is characters read off a screen with no buffer to
/// look through, which is what [`Selection::buffer_span`] answering nothing
/// already says.
///
/// Only in `lines`, for the reason [`Buffer::lines_within`] gives.
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

/// The match closest to a place: the first at or after it, wrapping to the top
/// of the file so the last match is not mistaken for there being no more.
fn closest_match(state: &State, from: Place) -> Option<Place> {
    let places = matches(state, ..);
    places
        .iter()
        .find(|at| (at.line, at.column) >= (from.line, from.column))
        .or_else(|| places.first())
        .copied()
}

/// Where the cursor is, in the same coordinates `matches` answers in: a row
/// while previewing, a source line and column otherwise. What `/` searches
/// from and what Escape has to put back. `None` with no buffer open, same as
/// `matches` answering nothing to search.
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

/// Puts the cursor on a match found by `matches`: a row and a rendered column
/// while previewing, the usual line and column otherwise. `go_to_place` alone
/// would read a row number as a source line and clamp it against the wrong
/// text. Neither field is clamped here — `settle` already clamps both centrally
/// against the rows it laid out.
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

/// Landing on a match: the cursor goes to it and it becomes the selection, which
/// is what lets what was found be copied or handed to project search without
/// being retyped. Matching is literal, so a match is exactly as long as the
/// query — there is no second copy of where it ended to keep true.
fn land_on(next: &mut State, at: Place) {
    let length = next
        .find
        .as_ref()
        .map_or(1, |find| find.query.shown().chars().count());
    let end = Place {
        line: at.line,
        column: at.column + length - 1,
    };
    // The same `Screen` shape a drag over a Preview produces, and for the same
    // reason: these `Place`s are a row and a *rendered* column, and a
    // `Selection::Buffer` is read as a source line and column everywhere it is
    // used — so what was found came back off the source line and copied the
    // markup the render had consumed. Resolved here rather than handed to the
    // edge because a Preview's rows are the library's own.
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

/// Arriving at a Step frames its Site: the cursor stays on the Site's first
/// line — where a comment starts, and where `V` and `c` have always begun —
/// and the *scroll* is what moves, so the claim opens downward into the pane.
/// Left to [`layout::viewport`] alone an arrival is a downward jump like any
/// other, which puts the line arrived at on the bottom row and the rest of the
/// claim below the fold. The clamp in [`settle`] does not fight this: it
/// returns the offset unchanged when the focus row is already inside the
/// window, and a framed Site's first row is.
///
/// Here rather than in the four walking arms because those return
/// `Effect::OpenAt` and the cursor does not land until that opening comes back
/// as a landing — a scroll set before the cursor moved is pulled straight back
/// by the clamp this exists to pre-empt. Both landings call it, which is why it
/// is a function and not two lines: `Event::BufferOpened`'s own place for a
/// file that had to be read (R34.3b), and `Event::JumpTo` for a cursor moving
/// inside the file already on screen. An arrival is recognised by the jump
/// landing on the first line of the Site being shown, which is what an arrival
/// *is*; [`story::mark`] answers for the Remainder's hunks and a Story's Sites
/// alike, and refuses an old-side Step, whose code is not on screen to frame.
/// The file is compared as well as the line, because `g` jumps to a cited
/// value without leaving the walk: a citation landing on the Site's line
/// number in another file would otherwise be framed as the Site, against rows
/// counted in the buffer the reviewer is no longer looking at.
///
/// Rows, not lines, via [`story::row_of`]: a comment row sits between two
/// lines, and a frame computed in lines sits one row off per comment above the
/// Site.
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

/// What arriving at a Step does to the prediction overlay: opens it, fresh,
/// the first time this Step is reached; dismisses it again if it was open,
/// put or not — stepping on always dismisses a Prediction, and a Prediction
/// already put is never asked again. Leaves any other modal alone: this
/// fires on every step, and `Modal::StepDetail` is meant to stay open and
/// track whichever Step is current, never closed out from under it.
/// `next.walking` is already the arrived-at Step, so `current_step` reads it
/// straight back rather than this re-deriving the same lookup.
fn arrive_at_step(next: &mut State, story: usize, step: usize) {
    let has_prediction = story::current_step(next).is_some_and(|step| step.prediction.is_some());
    if has_prediction && !next.predictions_put.contains(&(story, step)) {
        next.predictions_put.insert((story, step));
        next.modal = Modal::Prediction { picked: None };
    } else if matches!(next.modal, Modal::Prediction { .. }) {
        next.modal = Modal::None;
    }
}

/// A digit picks one of the current Prediction's choices by position. A
/// correct pick already replaced the choices with its own feedback, so a
/// further digit does nothing — undoing it would let the reviewer un-answer
/// a question they already got right.
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

/// `n`/`p`/`j`/`k`/`e` while walking a Story, shared by the two routes a
/// keystroke can arrive by: the real router's `EditorKey` (Story view's
/// focus stays on the editor) and the top-level `Key` a test drives directly
/// (see the `t`-toggle above for the same split).
fn walk_key(state: &State, next: State, key: char) -> (State, Vec<Effect>) {
    match walk_step_key(state, next, key) {
        Ok(answer) => answer,
        Err(next) => walk_place_key(state, next, key),
    }
}

/// The keys that move along the walk: to the next Step, or through the buffer
/// the current one cites.
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

/// The keys that act on where the walk currently is.
fn walk_place_key(state: &State, mut next: State, key: char) -> (State, Vec<Effect>) {
    match key {
        'e' => walk_to_step_file(state, next),
        // `D` opens the Step's detail overlay, and closes it again: the claim
        // stays on screen underneath, since the code is what the reviewer
        // came for and the overlay is one keypress away from it, not a
        // replacement for it.
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
        // The spine and the changed-files list share the tree pane; this key
        // toggles them regardless of whether anything is being walked (see
        // the `t` toggle in the top-level match), so walking must not swallow
        // it along with everything else the buffer would otherwise answer.
        't' => {
            next.story_listing = match state.story_listing {
                story::Listing::Spine => story::Listing::Files,
                story::Listing::Files => story::Listing::Spine,
            };
            (next, vec![])
        }
        // A stale Step is not a dead Step: the place its Site moved out from
        // under it is often exactly where a comment belongs, so this is
        // never gated on staleness.
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

/// `e` leaves the walk for the Step's own file, open in the editor.
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

/// `g` jumps to the current Step's first cited value's source. A value with
/// nowhere to point (`invented`) has no citation to jump to, so this answers
/// nothing rather than guessing at one.
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

/// Normal mode, where letters are commands rather than text. `W`, `B`, `/`, `n`
/// and `N` are claimed before the buffer sees them, and only here: inserting one
/// of them must still type it.
fn normal_mode(state: &State) -> bool {
    state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
        .is_some_and(|buffer| buffer.mode == editor::Mode::Normal)
}

/// Whether a keystroke would be typed into the buffer rather than read as a
/// command — true only with a buffer open and in Insert mode. With no buffer
/// open at all there is nothing to type into, so `t` is free to mean
/// something else, the way it does in Story view before anything is loaded.
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

    /// The one answer every site derives from, in both shapes: a project keeps
    /// Varde's files under its own root, and a Bare workspace keeps them in the
    /// Sidecar the edge handed in — the Sidecar whole, never joined onto, since
    /// it is already Varde's own directory and joining `.varde` onto it would
    /// bury every path one level deeper than the sweep looks.
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

    /// The candidate list belongs to the text being typed, so it goes when the
    /// keyboard does. No scenario reaches it: the list is arranged in the
    /// editor and the Scenarios that use it never leave. Left standing it
    /// claims Enter and the arrows from whatever has focus — and in a hosted
    /// pane that is a child holding a keyboard that answers nothing.
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
            // The file the list was asked about is the file on screen, because
            // that is the only state the core can reach: a list belonging to a
            // document nobody is looking at is taken down by the rule under
            // test here, and arranging one would prove the rule against
            // itself.
            current_buffer: Some(std::path::PathBuf::from("/src/lib.rs")),
            ..State::default()
        };
        // It stands while the editor has the keyboard.
        let stepped = update(&offering, Event::MoveCandidate(Direction::Down)).0;
        assert!(matches!(stepped.modal, Modal::Candidates(_)));
        let elsewhere = update(&offering, Event::ClickPane(Pane::Terminal)).0;
        assert_eq!(elsewhere.modal, Modal::None);
    }

    // A click in a shell is `FocusSplit`, not `ClickPane`, since the split
    // feature — and it stopped dropping the selection, which is what turned
    // Ctrl+C from the interrupt into a copy of stale text.
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

    /// A snippet's tab stops belong to the text being typed exactly as the list
    /// does, and the router asks what modal is up before it asks which pane has
    /// focus — so a sequence left standing claims Tab from wherever the
    /// keyboard went. No Scenario reaches it either: they fill a snippet in and
    /// never leave the editor. Typing is the other half of the rule, which must
    /// *not* end a sequence: that is what the stops are for.
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

    /// A stop is measured against the text that follows it, so a reader who
    /// deletes that text has left a stop nothing can name. Tab ends the
    /// sequence — all of it, the stops behind the unnamable one included, since
    /// a buffer that lost one stop's text is not a buffer the rest can be
    /// trusted in — and moves nothing, rather than sending the cursor to the
    /// first character of the file — which is the silent wrong answer, and the one
    /// outcome worse than the key doing nothing. No Scenario reaches it: it
    /// takes deleting past a stop the sequence is still holding.
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

    /// The ends of Tools, which no Scenario reaches: they walk to a
    /// row and act on it, so an unclamped selection would show up as an offer
    /// from the wrong row rather than as a panic. Down at the bottom stays, and
    /// Up at the top stays — the same as every other list in Varde.
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
        // The template's rows follow the configured ones, so the end is
        // wherever the list's own length puts it.
        let last = tools::rows(&state).len() - 1;
        state.modal = Modal::Tools { row: last };
        let bottom = update(&state, Event::MoveToolRow(Direction::Down)).0;
        assert_eq!(bottom.modal, Modal::Tools { row: last });
    }

    /// The ends of the Launch and branch lists, which the wheel reaches as
    /// easily as the keys do: a notch past either end stays on it.
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

    /// A re-check asks `PATH` again and says which row it asked about: the
    /// answer is read against a command, and the list may be gone by the time
    /// it lands. No scenario asserts the effect — a scenario can only see what
    /// the answer produced — so the asking is held here.
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
        // And the answer is what decides, not the asking: nothing is offered
        // until a probe has landed (R31.24).
        assert_eq!(asked.modal, Modal::Tools { row: 0 });
        let (told, _) = update(&asked, Event::PathProbed);
        assert_eq!(told.modal, Modal::Restart);
        assert_eq!(told.recheck, None, "the question was answered once");
        // And nowhere else: an answer that landed after the list went is an
        // answer to a question nobody is looking at, and no modal in Varde
        // opens itself.
        let elsewhere = State {
            modal: Modal::None,
            ..asked.clone()
        };
        let (told, _) = update(&elsewhere, Event::PathProbed);
        assert_eq!(told.modal, Modal::None);
        assert_eq!(told.recheck, None);
    }

    /// A global config that is there and could not be read is never written
    /// over: what cannot be read cannot be kept. No scenario reaches it — the
    /// world's modelled disk has no unreadable file.
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

    /// What an install configures is decided against the file too, so one
    /// that cannot be read is refused rather than written over.
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

    /// The voice written speaks from now on, as a start reading the file would
    /// — unless a voice beats the global file's, which only a project can.
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

    /// A taken row runs from now on, as a start reading the new file would run
    /// it, and taking it again is what clears the failure its last install
    /// left on it.
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

    /// No scenario takes an available speech row: nothing writes one yet.
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

    /// An install that reported 0 is a re-check of its row, asked outside the
    /// list: the probe that answers it is what forgets a server written off
    /// for a missing command, which is how the server starts with no restart.
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

    /// A preview is clean by construction, so clearing up closes it — and a
    /// `preview` left naming a buffer that is gone is the two-author
    /// divergence the one-owner rule exists to stop. Nothing on screen shows
    /// it, so it is pinned here rather than by a Scenario. The other branch is
    /// the one that makes it a decision: a preview typed into is dirty, stays
    /// open, and stays the slot the next preview replaces.
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

    /// Q59's constraint, and the reason the restart *is* `Quit`: a restart that
    /// could throw away unsaved edits would be a way around the refusal
    /// quitting already makes. One refusal rather than two that can come apart,
    /// so this holds the delegation rather than a second guard.
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
        // The question is answered either way: left standing it would take the
        // keyboard while the notice explaining the refusal is on screen.
        assert_eq!(next.modal, Modal::None);
    }

    /// The pane's start action takes the Scope the view on screen describes, not
    /// the one the last analysis ran over. A recompute is always the
    /// workspace's, so a review-scoped figure with a workspace recompute behind
    /// it is a Review view whose start action would otherwise let a session at
    /// files nobody is reviewing — while the border beside it showed the
    /// change's delta. No scenario reaches it: it takes a recompute fired from
    /// inside Review view.
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
                // The last analysis was a recompute, which is always the
                // workspace's.
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

    /// The path the scenarios cannot reach, because a review-scoped figure has
    /// to exist for it: leaving Review view does not leave the reviewed files'
    /// figure on the border to be counted as though it were the workspace's. It
    /// is dropped and the workspace measured again, so the number on the border
    /// always describes the Scope on screen.
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

    /// The Risk row's ask is reachable by keyboard, not by mouse alone: Right
    /// steps into the row's icons and Enter runs the one it is on. And stepping
    /// to another row takes the arming with it — Enter on a row nobody armed is
    /// the go-to-the-code gesture, not a prompt about whatever row the arrow
    /// left behind.
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

        // Another row, and the arming is gone: Enter opens the code instead.
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

    /// The Risk pane's own two actions are reachable by keyboard as well as by
    /// the icons on its border: `r` recomputes, and the second slot is
    /// whichever of the start and the stop the pane is offering — one list
    /// behind the key, the icon and the click, so a key cannot start a loop
    /// while the icon beside it says stop.
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

    /// The status line holds the last words it was handed until something
    /// replaces them, so a notice about a fact outlives the fact. Resolving a
    /// divergence left "changed on disk under your unsaved edits — D to
    /// resolve" standing over a buffer that agreed with disk, pointing at a `D`
    /// that would then refuse. Both answers that settle it withdraw it.
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

    /// The withdrawal is not the picker's: `:w` and `:e` settle a divergence
    /// without ever opening it, and a notice left standing after those points
    /// at a resolution there is nothing left to resolve.
    #[test]
    fn writing_or_reloading_withdraws_it_without_the_picker() {
        let root = PathBuf::from("/w");
        for event in [Event::WriteBuffer, Event::ReloadBuffer] {
            let (_, effects) = update(&diverged(&root), event);
            assert!(effects.contains(&Effect::ClearNotice));
        }
    }

    /// Bug 11: the watch set was assembled in `main.rs` from the tree's
    /// expanded folders alone. Review view lists changed files flat, so the
    /// file a diff was shown for was followed only by luck, and collapsing a
    /// folder stopped the buffer in it from following its file at all — which
    /// reads as the editor simply not updating.
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

    /// Folders only. A watch on the file itself is lost the moment a tool
    /// replaces it by rename rather than writing through it — which is how
    /// `sed -i`, most editors and most AI CLIs save.
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

    /// A Story is not a path, so the spine is never rows `tree::visible_rows`
    /// returns — which means the generic per-event scroll clamp would collapse
    /// `tree_scroll` to zero the moment `t` shows the spine, and the
    /// changed-files list would come back scrolled to wherever the empty spine
    /// left it rather than where it was. `t` has to leave `tree_scroll` alone
    /// while the spine is showing for the list to come back untouched.
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

    /// A tick turns the spinner and touches nothing else. Load-bearing: every
    /// event but the wheel pulls the tree back to its selection, and a tick
    /// eighty milliseconds behind the wheel doing that would make the tree
    /// unscrollable for as long as a job runs — the tick is the one event the
    /// user did not cause, so it must not undo the one they did.
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

    /// The mouse wheel over the spine scrolls the spine's own offset, not the
    /// changed-files list's — the two are deliberately separate fields (see
    /// the toggle test above), and the wheel has to respect that split too.
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

    /// The read the arrival checks ride on is aimed at the range the artifact
    /// names, not at whatever the two-second poll last diffed. A committed
    /// range checked against the working tree finds no hunks and flags every
    /// Step, so the head travels with the request.
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

    /// The Remainder sits one selectable row past the last Story — Down past
    /// the last Story lands there, and Enter on it walks it, the same two
    /// steps that reach and enter a Story.
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

    /// A refusal replaces the code with a handful of fixed rows, so the scroll
    /// the previous Step left would push the whole notice off the top and leave
    /// the pane blank — saying even less than the wrong code did. Clamped here
    /// rather than subtracted by the renderer, because `mouse` hit-tests the
    /// editor against this same field.
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
        // Where arriving at this Step leaves it: a deletion deep in a file, so
        // the clamp would otherwise scroll past the whole notice.
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

    /// A checkout that did not happen is said out loud. Silence here is the
    /// `ai_running` failure again: the picker would close, nothing would
    /// change, and the reviewer would be left on a branch they thought they had
    /// left.
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

    /// The prompt points at the hand-over in one line, so an AI that read it
    /// first would find nothing there. Same ordering `submit` already keeps
    /// between a review's artifact and the prompt naming it.
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

    /// `t` toggling the spine has to be Story view's own affair, not a
    /// keystroke the editor never gets to see: a buffer open in Story view is
    /// still a buffer, and inserting the word "test" must not toggle the tree
    /// pane on its first letter.
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

    /// A jump reads the file afresh, so what it read is what the jump lands
    /// on. The watcher is the only other way a buffer learns the disk moved,
    /// and a change it never reported left a walked Site marking the file as
    /// it was before the change — HEAD's text, for an uncommitted range.
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

    /// Following the file a jump read obeys the watcher's rule: a draft is
    /// never overwritten, and the divergence is announced — while reopening a
    /// drafted buffer over an unchanged file flags nothing at all.
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

    /// A read-only surface refuses the word delete, the way it refuses the
    /// backspace beside it. The router will not send `EditorDeleteWord` over a
    /// diff or a walked Site — `typing_into_the_buffer` is its whole condition
    /// — so no scenario can reach this, and that is the point: where a key goes
    /// is `update`'s to decide, and the backspace arm records what leaving one
    /// of these two surfaces out of the question already cost.
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
        // Away from column 1 before the surfaces are built: there a word
        // delete is the join a backspace does and changes nothing anyway, so
        // the assertion below would hold whether the guard ran or not.
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
        // And still does it where the buffer is what is being typed into, so
        // the guard cannot pass by refusing everywhere.
        let after = update(&inserting, Event::EditorDeleteWord).0;
        assert_eq!(after.buffers[&path].shown(), "two three\n");
    }

    /// A waiting `g` owns the next key even over a selection. Opening a
    /// project-search hit leaves one over the match, so `gd` on the name it
    /// took you to reached the charwise-delete arm and deleted the word — a
    /// jump that silently edits the file it was called from. The `every_key`
    /// sweep cannot see it: the selection it drives every key with is a pty
    /// one, and this arm only claims a Buffer selection.
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
        // No server for the language, so the jump says so rather than
        // silently doing nothing — which is what proves the key arrived.
        assert_eq!(after.1, vec![Effect::Notify("no-language-server")]);
        // A bare `d` still deletes the selection, which is the behaviour the
        // guard must not have taken away.
        assert_eq!(
            update(&state, Event::EditorKey('d')).0.buffers[&path].shown(),
            "let  = helper();\n"
        );
    }

    /// A `g` chord waiting for its second key still means `gt`/`gT` step
    /// buffers in Story view, exactly as it does everywhere else — Story
    /// view's `t` only takes over once no chord is pending.
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

    /// One write, in this order: clear the line, the review, Enter. A child that
    /// cannot be told a paste from typing gets the text bare — the same contract
    /// as an ordinary paste — so its newlines are its own to interpret.
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
        // A review quoting a filename that carries the end marker cannot close
        // its own bracketing and hand the rest to the child as keys.
        assert_eq!(
            injection(
                "safe\x1b[201~rm -rf /",
                keys::Paste::Bracketed,
                Enter::Pressed
            ),
            b"\x15\x1b[200~saferm -rf /\x1b[201~\r".to_vec()
        );
        // Withheld, the paste is all of it: nothing the user had typed is
        // cleared, and nothing is sent on their behalf.
        assert_eq!(
            injection("one\ntwo", keys::Paste::Bracketed, Enter::Withheld),
            b"\x1b[200~one\ntwo\x1b[201~".to_vec()
        );
    }

    // Two pty panes, one selection: the span belongs to the pane it was dragged
    // in, so the other pane must not show a highlight over its own cells.
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
        // A buffer selection is the editor's own highlight, not a grid's.
        assert_eq!(
            Selection::Buffer {
                anchor: Place { line: 1, column: 1 },
                cursor: Place { line: 1, column: 2 },
            }
            .screen_span(Pane::Ai),
            None
        );
    }

    // The seam the mouse encoding is asserted at: bytes reaching a pane. One
    // click on one cell is two different sequences depending on what the child
    // asked for, and sending the wrong one is the phantom text in its prompt.
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

    // A cell the legacy encoding cannot name leaves the child untold, and a
    // wheel nobody can be told about is ours rather than nobody's.
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

    /// Extending leftwards or upwards leaves the cursor before the anchor, and
    /// everything reading a span assumes the ends arrive in order.
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

    /// `y` copies what it yanked, so the half-typed first `y` of `yy` must copy
    /// nothing — otherwise it puts whatever the last delete left in the register
    /// on the clipboard.
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

    /// A diff answers the editor's keys ahead of the buffer, so the file behind
    /// it is not what Ctrl+V is aimed at: reading the clipboard there would
    /// edit a file nobody is looking at. The same is true of a walked Site,
    /// which the arm names beside it. The second half is what keeps this from
    /// passing on a paste that asks for nothing anywhere.
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

    /// A re-read that comes back shorter has to pull the cursor back with it:
    /// `comment_range` slices the diff by the cursor, so a line that no longer
    /// exists is a panic the moment the reviewer presses c.
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

    /// The whole table, because the claim is that walking wins over *every*
    /// mode rather than only Normal — a reviewer mid-insert when the walk began
    /// is in no position to type either. The two Normal rows the scenarios
    /// already cover stay in: a truth table with a hole in it sends the reader
    /// looking for the missing case in a feature file.
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
            // Stepping mode outranks the buffer's own mode and nothing else:
            // `n`, `i`, `o` and `c` drive the program rather than the buffer,
            // so naming the buffer's mode there advertises keys that are not
            // on offer — but a walk refuses every edit, which is the more
            // useful thing to say. Until the Variables' title exists this is
            // where Varde says the mode is on at all.
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

    /// Which chord leaves the keyboard in Stepping mode and which does not.
    /// `␣q` is the chord that ends the session the mode belongs to, so it is
    /// the one that leaves it off: in the interval between the request and the
    /// adapter letting go, the mode is four letters swallowed for a session on
    /// its way out. With no session the mode never opens at all, because `n`
    /// is find-next the rest of the time.
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

    /// The claim the guard in `update` makes that no list of keys could: an
    /// edit that never went through a key at all is refused too. A bracketed
    /// paste is the one that motivated it — `EditorPaste` is its own event and
    /// reaches the buffer past every arm the editor keys go through.
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

    /// The other surface every editing key is refused on, for the reason
    /// walking is: a buffer on a Guest repo's file has nowhere to save to.
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

    /// Space waits for a chord only in plain normal mode over Source: over a
    /// half-typed operator it would swallow the operator's second key, and a
    /// Preview's rows are not lines a Breakpoint could be set on.
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

    /// Moving is saved as it happens, not only at the next toggle or at quit:
    /// state naming the old line would make a Breakpoint that followed its
    /// line read as Stale at the next start.
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

    /// The list's keys reach what its Chips do: `d` the row's, `D` the
    /// Transport's. Removing the last row leaves the highlight on the row that
    /// is now last, and clearing with nothing to clear is dimmed and does
    /// nothing — not even a save.
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

    /// The list's `e` is its `✎` Chip: the box opens on the row the keyboard
    /// is on, and what Enter keeps is saved with the Breakpoint.
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

    /// `␣B` on a line with no Breakpoint sets one and opens its box, so it is
    /// never a key that does nothing.
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

    /// A line the buffer does not have is not a place for a Breakpoint: a
    /// click below a short file's last line sets nothing.
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

    /// A search that is on, with the keyboard back in the buffer.
    fn finding(query: &str) -> Find {
        Find {
            query: Buffer::text_box(query),
            origin: Place { line: 1, column: 1 },
            case: search::Case::Smart,
            keys: FindKeys::Away,
        }
    }

    /// The pane a Preview lays out to, arrived at through the events the edge
    /// sends: a screen size and an opened file.
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

    /// What ADR 0007's amendment puts in place of the old pin: the offset
    /// follows the rendered column, so leaving a wide fence for a row with
    /// less text on it brings the view home instead of leaving the reader
    /// looking at empty space past that row's end. The scenarios drive the
    /// slide itself; this holds the return, which no gesture asks for.
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

    /// The arrows reach the row cursor too. A modifier-free binding exists
    /// (`j`/`k`), and the arrows are aliases — but a key that does nothing
    /// silently is the failure `keys.rs` is shaped to prevent.
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

    /// A refusal answers the event that earned it and nothing after it.
    #[test]
    fn a_refusal_is_cleared_by_the_next_event() {
        let refused = update(&State::default(), Event::TogglePreview).0;
        assert_eq!(refused.refusal, Some(preview::Refusal::NoFileOpen));
        let after = update(&refused, Event::EditorKey('j')).0;
        assert_eq!(after.refusal, None);
    }

    /// A click's place names a row while previewing, not a line-and-column: the
    /// same event `mouse::pressed` sends for Source, interpreted the other way.
    #[test]
    fn clicking_a_preview_places_the_cursor_on_the_row() {
        let state = previewing_readme("# Setup\n\nInstall it.\n");
        let clicked = update(&state, Event::ClickText(Place { line: 3, column: 1 })).0;
        assert_eq!(current_buffer(&clicked).expect("a buffer").row, 3);
    }

    /// The row map is read, never re-derived: a click past the last row lands
    /// on it rather than a column ten wide off the page.
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

    /// The heart of ticket 10: a drag across a Preview names rows, and the
    /// selection is the text as drawn — markup consumed, not the source line.
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

    /// A wrapped paragraph's rows join with the newline the wrap put there, so
    /// a drag spanning them carries it — the same shape a multi-line Source
    /// drag already has.
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

    /// A narrower pane is fewer rows, and the cursor comes back with them.
    /// Without this the caret sits on a row that no longer exists, which draws
    /// as no caret at all.
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

    /// The row the cursor is on never leaves the rows there are, so every
    /// reader of the map — the scroll clamp included — has an answer.
    #[test]
    fn the_row_cursor_stops_at_the_last_row() {
        let state = previewing_readme("# Setup\n");
        let after = (0..10).fold(state, |state, _| update(&state, Event::EditorKey('j')).0);
        assert_eq!(
            current_buffer(&after).expect("a buffer").row,
            preview_rows(&after).len()
        );
    }

    /// The full refused set from ticket 08, driven one at a time rather than
    /// through the one `dd` the behavior suite pins the wording with: every
    /// key here must leave the buffer without a draft and name
    /// `read-only-preview`, or a plausible-looking guard that only catches
    /// `dd` would pass the scenario while `x` still edits. `i` left the set in
    /// ticket 07 — it crosses to Source instead of refusing.
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

    /// R35.1 at the arm no scenario reaches: a next past the last Utterance is
    /// the end of the Selection, and the Reading ends there rather than
    /// starting over. Wrapping would carry a reader who asked to move on back
    /// to a paragraph they had just heard.
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

    /// ADR 0022. The Chip lit is the Transport action taken last and moves
    /// only when another is taken — nothing unlights it on its own — while a
    /// dimmed Chip pressed did nothing, so it lights nothing either.
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
        // A Reading that ran to its end, or whose player never started, was
        // not stopped by anybody, and a play that started nothing — no
        // Selection to read — was refused out loud rather than done.
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

    /// A position report is what the machine did, so it is the tick's
    /// exception and not a second rule: the clamp earns its keep by covering
    /// everything a *person* did. A report arriving on the spinner's cadence
    /// and pulling a wheeled editor back to the cursor would make it
    /// unscrollable for as long as a Reading plays.
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

    /// The pointer is the tick's exception too, and for its own reason: motion
    /// is reported for every cell it crosses, so a move that pulled a wheeled
    /// editor back to the cursor would leave it unscrollable for as long as a
    /// hand rested on the mouse. The window is armed for the cell the pointer
    /// arrived at and left running by a report for the cell it is already on —
    /// a terminal that repeats them would otherwise never let a rest finish.
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

    /// A box of `lines` rows asked about line 2 of an open file, placed from
    /// line 3 — the state every wheel over a Hover below arrives into.
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

    /// The wheel stops with the last line against the bottom border: the
    /// pane's 18 rows are 16 of text inside the box's border, so a 30-line
    /// reply has 14 rows to scroll and a reply that fits has none.
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

    /// Scrolling the box is the wheel, and the wheel is the exception to the
    /// clamp: an editor wheeled away from the cursor stays where it was, or
    /// the box would jump with the lines it sits over.
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

    /// Escape with the keyboard in a Hover closes the box and nothing behind
    /// it: a Story being authored keeps waiting, which the bare Escape it
    /// shares an event with would cancel.
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

    /// The other half of ticket 07's guard: a markdown buffer already crossed
    /// to Source is not a Preview, so `i` is only the insert it has always
    /// been. The column is what catches a guard that re-crosses — the
    /// crossing resets it to 1, and typing where the caret was is the whole
    /// point of arriving there.
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

    /// Ticket 07: with no file open `i` still does nothing at all. Asserted
    /// against the whole `State` rather than the two or three fields a
    /// crossing would touch — there is no buffer to read a row map off, so
    /// every field is equally an absence, and naming a few would pass anything
    /// that reached for a different one.
    #[test]
    fn insert_with_no_file_open_changes_nothing() {
        let (after, effects) = update(&State::default(), Event::EditorKey('i'));
        assert_eq!(after, State::default());
        assert!(effects.is_empty(), "{effects:?}");
    }

    /// The keys that only read are not swept up by the refusal above — `j`/`k`
    /// already have their own tests; this pins the rest of the set the ticket
    /// names: motions, `/` and `n`/`N`.
    #[test]
    fn reading_keys_are_not_refused_in_a_preview() {
        for key in ['h', 'l', 'w', 'b', '/', 'n', 'N'] {
            let state = previewing_readme("# Setup\n");
            let after = update(&state, Event::EditorKey(key)).0;
            assert_eq!(after.refusal, None, "{key:?} was refused");
        }
    }

    /// Ticket 02, direction one: Preview to Source takes whatever row the
    /// cursor sits on straight off the map, column reset to 1, for every kind
    /// of row the render produces — reached by `RowKind`, not by markdown
    /// syntax, since two constructs sharing a kind would prove nothing twice.
    #[test]
    fn crossing_to_source_lands_on_the_rows_line_column_one() {
        let contents = [
            "## Install\n",                           // Heading
            "Prose in a paragraph.\n",                // Paragraph
            "- one\n- two\n",                         // List
            "> quoted\n",                             // Quote
            "| a | b |\n|---|---|\n| 1 | 2 |\n",      // Table
            "one\n\n***\n\ntwo\n",                    // Rule
            "```rust\nfn main() {}\n```\n",           // Code
            "```mermaid\ngraph TD\n  A --> B\n```\n", // Diagram
            "---\ntitle: Varde\n---\n\nProse.\n",     // Metadata
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
                // A column left over from before the buffer ever entered
                // Preview — proves the crossing resets it rather than
                // happening to find it already at 1.
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

    /// Ticket 02, direction two, across the same nine kinds: Source to
    /// Preview lands the cursor on the row of the kind the block at that
    /// source line actually is — the interesting failure a weaker assertion
    /// (any row, or a row at that line) would miss is landing on the blank
    /// separator `preview::rows` inserts right before a Rule sharing its
    /// exact line, which is why the Rule case targets line 3, not line 1.
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

    /// Ticket 02, direction two: Source to Preview goes to the first row at
    /// or after the cursor's line — and a blank source line, which renders no
    /// row of its own, lands on the next row that exists rather than failing.
    /// Line 3 is the interesting middle case: `preview::rows` inserts a blank
    /// separator row sharing that exact line to keep the heading and the
    /// paragraph apart on screen, and the separator is not the row the
    /// crossing means — the paragraph's own row is.
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

    /// Ticket 09: `varde::matches` searches the rendered row rather than the
    /// source line while previewing, so a heading's `##` — consumed by the
    /// render — is never found, and the word it left behind is, in row
    /// coordinates rather than source ones.
    #[test]
    fn matches_searches_rendered_rows_while_previewing() {
        let mut state = previewing_readme("## Install\n");
        state.find = Some(finding("Install"));
        assert_eq!(matches(&state, ..), vec![Place { line: 1, column: 1 }]);

        state.find = Some(finding("##"));
        assert!(matches(&state, ..).is_empty(), "a consumed marker matched");
    }

    /// The same query against the same content answers differently once the
    /// buffer leaves Preview: the source line still holds the marker
    /// `matches` refuses to find while rendered.
    #[test]
    fn matches_reverts_to_source_lines_once_preview_is_left() {
        let mut state = previewing_readme("## Install\n");
        let path = state.current_buffer.clone().unwrap();
        state.buffers.get_mut(&path).unwrap().previewing = false;
        state.find = Some(finding("##"));
        assert_eq!(matches(&state, ..), vec![Place { line: 1, column: 1 }]);
    }

    /// A Preview's matches are rendered rows and columns, not places in the
    /// text, so neither replace touches the buffer there.
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

    /// The replace box's "find" field is the search's own query, so typing in
    /// it searches again; its "with" field searches nothing.
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

    /// Going back into a search with `/` keeps where it started, so Escape
    /// from the query still puts the cursor back there.
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

    /// `n`/`N` step between preview matches by row: the cursor lands on
    /// `buffer.row`, not `buffer.line`, since a Preview cursor has no source
    /// line to sit on.
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

    /// The caret goes anywhere in a query, and a key lands where it is. Only a
    /// change to the text searches again: moving through the query is not a
    /// new question, and must not throw away the hit somebody had picked.
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
        assert_eq!(effects, vec![Effect::RunSearch("update".to_string())]);
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

    /// Escape restores the row the search started from, in a Preview exactly
    /// as it does in Source — `OpenFind` and `CloseFind` read and write
    /// `buffer.row` rather than `buffer.line`/`column`, which have no meaning
    /// once the cursor is a row.
    #[test]
    fn escape_restores_the_row_a_preview_search_started_from() {
        let mut state = previewing_readme("Install\n\nInstall\n");
        let path = state.current_buffer.clone().unwrap();
        // Row 2 is the blank separator between the two paragraphs — on no
        // match of its own, so the closest one found is the second "Install".
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

    /// A diff open over the pane makes `previewing(state)` false regardless
    /// of the buffer's own flag — Review's diff substitutes the whole pane.
    /// Keying the toggle's direction off that composite rather than off the
    /// buffer directly meant `:preview` could only ever turn a buffer's own
    /// flag on while a diff was showing, never back off, because the
    /// direction guard read as "not previewing" both before and after.
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

    /// A cursor past every row's line — the last line of the file falling in
    /// a construct the render skips, or simply past the end — lands on the
    /// last row rather than failing to find one at all.
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
    /// The Risk list's own geometry, and what it remembers. Every direction out
    /// of the pane is here rather than only the two a scenario names: a pane you
    /// can enter and cannot leave sideways is a dead end nobody would report as
    /// a bug, and the shell sits directly to its right.
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

        // Every direction written both ways round. The shell starts where the
        // Risk list ends, so leaving it sideways and not getting back in left a
        // list only the tree above it could reach — and the tree is two panes
        // from the shell, which is the whole of what made the pane feel
        // unreachable by keyboard.
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

    /// The palette fits the screen it is drawn on. `layout::overlay` caps the
    /// box at the screen's height and `ui` renders it with no scroll offset, so
    /// every row past what the box can draw is dropped in silence — which is
    /// where `(q) Quit` went once the Corner's Buffers and Cursor history
    /// entries had pushed the list to 27 rows. 26 is the height the replay
    /// recipe and the scenarios use; 24 is a stock macOS Terminal.
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

        // The groups still read as groups wherever there is room for the gaps,
        // and the way out is still spelled while there is room for it: an
        // entry is what a short screen must not cost.
        let roomy = palette_rows(40);
        assert!(roomy.iter().any(|(_, row)| row.is_empty()));
        assert_eq!(
            roomy.last().map(|(_, row)| row.as_str()),
            Some("   Esc  cancel")
        );
        // 28 and not 26: with eleven Panes entries the cancel line is the row a
        // 27-row screen gives up, and it is the first row a short screen gives up.
        assert!(palette_rows(28)
            .iter()
            .any(|(_, row)| row == "   Esc  cancel"));

        // Shorter than every entry fits, and the loss is said out loud rather
        // than clipped: the last row is the mark `lsp::TALLEST` uses.
        let tiny = palette_rows(14);
        assert_eq!(tiny.len(), 12);
        assert_eq!(tiny.last(), Some(&(None, "   …".to_string())));
    }

    /// An armed row action belongs to the pane that armed it. Carried across a
    /// focus change it is resolved against whatever list the pane it landed in
    /// has: nothing at all in the Buffers pane, which left Enter dead on every
    /// press with no icon anywhere on screen to explain it — the tree's own
    /// delete icon stays lit, in the pane the keyboard has left — and the first
    /// entry of `risk::row_actions` or `history::row_actions` in the other two,
    /// which is worse than dead, since Enter then asked for a refactor of a row
    /// nobody armed. Let go of where focus moves rather than in the arms that
    /// noticed: three of the four list arms cleared it on a motion and the
    /// fourth did not, and a rule each pane has to remember is a rule the fifth
    /// pane will be written without.
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

        // Down out of the tree lands in whichever pane the corner holds, and
        // the icon is let go of on the way.
        let corner = update(&armed, Event::MoveFocus(Direction::Down)).0;
        assert_eq!(corner.focus, Pane::Buffers);
        assert_eq!(corner.selected_action, None);
        let shown = update(&corner, Event::Activate).0;
        assert_eq!(shown.current_buffer, Some(PathBuf::from("/w/b.rs")));
        assert_eq!(shown.focus, Pane::Editor, "Enter is the deliberate go");

        // And the same on the other route into the corner: a toggle moves focus
        // too, so it is the other site that has to let go.
        let toggled = update(&armed, Event::ToggleCursorHistory).0;
        assert_eq!(toggled.focus, Pane::History);
        assert_eq!(toggled.selected_action, None);
    }

    /// The Risk list, shown and focused, over a figure whose worklist is two
    /// rows long — worst first, so `route` is row 0 and `draw` row 1 — on a
    /// screen big enough for the pane to have rows.
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

    /// The motions move the selection and Enter goes to the code behind the
    /// figure — through the same `OpenAt` the Story jump and a search hit use,
    /// with the cursor on the Function's own line and focus following, because
    /// Enter is the deliberate "take me there".
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
        // Past the last row is the pane's own actions, not a row nobody could
        // open: `risk::selected` finds nothing there, so Enter opens nothing.
        let past = update(&down, Event::MoveSelection(Direction::Down)).0;
        assert_eq!(past.risk_selection, 2);
        assert!(risk::on_actions(&past));
        assert_eq!(risk::selected(&past), None);
        // And no further: two rows and one actions slot is the whole pane.
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

        // A click is the same gesture, so it is the same arm: focus lands in
        // the pane, the row it named is selected, and the file opens.
        let clicked = update(&state, Event::ClickRiskRow(1));
        assert_eq!(clicked.1, effects);
        assert_eq!(clicked.0.risk_selection, 1);
        // R10.1: clicking focuses what you clicked. The arm this delegates to
        // is Enter's, which ends in the editor because Enter is the deliberate
        // "take me there" — so the focus is re-asserted after it, exactly as
        // `on_click_row` re-asserts the tree's. Without this the only clickable
        // part of the pane that focused it was its border.
        assert_eq!(clicked.0.focus, Pane::Risk);
        // A stale figure is still a figure: the row opens, and the figure is
        // still marked stale afterwards.
        let mut stale = state.clone();
        risk::went_stale(&mut stale.risk);
        let (after, effects) = update(&stale, Event::ClickRiskRow(1));
        assert!(matches!(&effects[..], [Effect::OpenAt { .. }]));
        assert_eq!(risk::view_state(&after), "stale");
    }

    /// The pane's own two actions are reached by stepping down off the end of
    /// the worklist and along with Left and Right — the same gesture a row's own
    /// icon takes, one slot further down. `r` and `l` are aliases for them, and a
    /// gesture only the mouse and a modifier-free letter have is a gesture
    /// nobody arrowing through the pane can find.
    #[test]
    fn the_risk_panes_actions_are_reached_by_arrowing_past_the_last_row() {
        let state = worklist();
        // Down twice: row 0, row 1, then the actions.
        let border = update(&update(&state, Event::Key('j')).0, Event::Key('j')).0;
        assert!(risk::on_actions(&border));
        // Arriving there arms the first of them: the slot holds nothing but its
        // actions, so arriving bare would be arriving where Enter does nothing.
        assert_eq!(border.selected_action, Some(0));
        assert_eq!(
            selected_row_actions(&border),
            vec![risk::RECOMPUTE, risk::START_LOOP]
        );

        // Enter runs the one that is lit, through the event the border's own
        // icons reach when they are clicked.
        let (_, effects) = update(&border, Event::Activate);
        assert!(
            matches!(&effects[..], [Effect::AnalyseRisk { .. }]),
            "the recompute, not a row action: {effects:?}"
        );

        // Right steps along to the loop, and no further.
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
            // The loop is refused here for want of a test command, which is a
            // refusal and not silence — what matters is that it was the loop's
            // arm and not the recompute's.
            Some(Effect::Notify(_))
        ));

        // Left steps back to the recompute, and stays there: there is no row
        // under these icons to step back out onto. Up is the way out.
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

    /// A Scope with no rows is exactly the one where the recompute is the only
    /// thing left to reach, so the actions slot is where an empty pane starts.
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

    /// A recompute is a fresh measurement, so the last loop's verdict goes with
    /// it: `stopped` and the tick beside it both trail the border's figure, and
    /// a run that ended on a failing test used to leave "tests failed" sitting
    /// beside a number measured long after those tests stopped failing.
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

        // Not while a loop is running: mid-run those same two fields are the
        // Gate's live verdict, and the Gate recomputes as part of its own pass.
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

    /// A Row selection names something to go to, not text somebody picked: it
    /// never becomes `selection`, so nothing copies it and Ctrl+C over the pane
    /// leaves the clipboard exactly as it was.
    #[test]
    fn a_risk_row_is_never_text_to_copy() {
        let moved = update(&worklist(), Event::Key('j')).0;
        assert_eq!(moved.selection, None);
        assert_eq!(moved.selected_text(), None);
        assert_eq!(update(&moved, Event::Copy).1, vec![]);
    }

    /// `a` is the toggle the pane arrived without: `ToggleRiskAll` was reachable
    /// from nothing, and the pane's own keys are where a toggle about this list
    /// belongs.
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

    /// The wheel is the one event allowed to take the list away from its
    /// selection; every other one pulls it back, which is why no arm has to
    /// remember to scroll.
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

    /// Five files with a hit each: ten rows — a heading and a hit apiece — in a
    /// box that shows five. The defect this pins is the arithmetic: counting
    /// hits alone made the last hit look like row four, so the box never
    /// scrolled and the highlight simply disappeared off the bottom. The wheel
    /// is the one event allowed to take it away from the selection, and the
    /// next key pulls it back.
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
                    truncated: false,
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

        // Whichever pane the pointer is over: the box covers all of them.
        let wheel = Event::Scroll {
            pane: Pane::Editor,
            direction: Direction::Down,
            at: Place { line: 1, column: 1 },
        };
        // Enough notches to reach the bottom of the list, so what is pinned
        // is where the clamp stops rather than how far one notch goes.
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

        // Sideways, the box has no offset of its own — and the pane it covers
        // must not take the swipe instead, for the reason the vertical wheel
        // does not reach through it either. The buffer behind it is wide enough
        // to slide, or the assertion could not fail.
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
