# AGENTS.md — Ground Rules for This Repository

This file is the working contract for this repo. It applies to every developer and every AI coding
agent (Claude Code, Cursor, Copilot, Codex, …). Read it before changing anything. The structure and
conventions below are deliberate — **extend them, do not restructure or rewrite them.**

## How this repo is built (the groundwork)

- The product is an **IDE TUI**: a terminal UI that opens on a folder and presents that folder as a
  workspace. Early and exploratory — expect the domain to grow, not the structure to change.
- **The rendering logic is pure and the terminal is at the edge.** `src/` holds functions that take
  plain data and return a view model. Nothing in `src/` touches the terminal, the pty, git, or the
  filesystem — callers pass data in. This is the load-bearing decision in the repo: it is what keeps
  the behavior suite fast and free of fixture folders and real terminals.
- **Rust**, chosen after the spec was complete. The stack and the reasoning are in `docs/stack.md`;
  read it before adding a dependency.
- `src/` modules map to workspace concepts (tree actions, view mode, review, config), not to TUI
  widgets.
- **No provider-specific code, ever.** No branch anywhere may test which CLI is running in a hosted
  pane — not by name, not by version, not by sniffing its output. If a provider misbehaves, the
  defect is in the key transport, the mouse encoding or the query replies, and fixing it there fixes
  it for the providers nobody has tried. The first hard provider bug is where this costs something,
  which is the point: `docs/adr/0004-hosted-panes-are-transparent.md`.

## Architecture: state in one place, effects as data

One state struct. One function that changes it. Everything the outside world must do is **returned as
data**, never performed inside the logic:

```rust
pub fn update(state: &State, event: Event) -> (State, Vec<Effect>);

enum Effect {
    SetTerminalInput(String),
    RunInTerminal(String),
    OpenBuffer(AbsPath),
    SpawnAi { command: String },
    WriteReview { path: AbsPath, json: String },
}
```

- The edge (binary) executes effects; `src/` only describes them. This is what makes assertions like
  `no command has been executed`, `no file was opened in the editor` and `no new AI session was
  started` a comparison of two values instead of a mock.
- **All mutation lives in `update()`.** To trace how a value got there you read one function. No
  `Rc<RefCell<_>>` — reaching for it means the data flow is already untraceable.
- **Newtypes over primitives:** `AbsPath`, `RelPath`, `WorkspaceRoot`, `LineRange`. R3.3 says injected
  paths are always absolute; with `AbsPath` the compiler enforces it instead of a reviewer.
- **Enums, not booleans:** `enum Modal { None, Palette, NameBox }`, never two independent bools that
  can both be true.
- **Concrete types until duplication forces otherwise.** The one required indirection is the clock,
  behind a trait, for F6's 300 ms window.

## Code budget: the spec is the ceiling

Agents reliably over-build: wrapper functions, premature abstractions, context objects, file sprawl,
and comments restating the code. These rules are deliberately falsifiable so the failure is visible.

- **No code without a red test.** If no scenario or unit test fails when you delete it, delete it.
  This is the master rule; the rest are corollaries.
- **Use the crate, don't write the function.** If a maintained crate already solves it — shell
  quoting, gitignore matching, layered config, path handling, debouncing — take it as a black box.
  Someone else has already found the edge cases you haven't thought of. Hand-rolling a solved problem
  is the most expensive kind of code in this repo: it looks small, and it is wrong in ways no
  scenario covers. Check `std` first, then a crate already in the tree, then a new one.
  This does not license dependency sprawl: prefer one well-known crate over three niche ones, name
  the crate and the reason in the commit, and add it to `docs/stack.md`.
- **`#![deny(dead_code, unused)]`** in `src/lib.rs`. The compiler catches unused wrappers, so the
  question "is this needed?" is answered by the build rather than by opinion.
- **Rule of three.** No trait, generic, or shared helper until a *third* concrete duplicate exists.
  Two similar things are a coincidence.
- **No single-caller wrapper functions.** Inline it. A function that exists to rename another
  function is noise.
- **Banned type names:** `*Manager`, `*Context`, `*Service`, `*Helper`, `*Util`, `*Handler`. If the
  name doesn't say what it does, the type doesn't know either.
- **No struct that exists to carry one field.** Pass the field. No builder under four fields.
- **One module per feature area, maximum** — `tree_actions`, `view_mode`, `review`, `config`. A new
  file needs a stated reason in the commit message. Nine features do not need thirty files.
- **No comments.** The one exception is a single line naming a genuinely edge-case quirk the code
  cannot show — usually a terminal's, a library's or a protocol's. No doc comments, no rationale
  paragraphs, no history. A comment is a second copy of the code that nothing keeps in step with
  it; the why belongs in the commit message, an ADR or this file.
- **Delete on sight:** unused `impl Default`, `From` conversions nobody calls, re-export layers,
  `mod.rs` files that only `pub use`.

After each feature goes green, do a deletion pass before moving on: what can be removed while the
suite stays green? That pass is not optional and it is not a refactor — it is part of finishing.

## The edge

`src/main.rs`, `src/ui.rs` and `src/pty.rs` are the binary — the only code that touches the
terminal, the pty, the filesystem, git or the clipboard. They make no decisions: input is
translated into an `Event`, `varde::update` decides, and the returned `Effect`s are executed.

**The edge runs both ways.** It is not only "execute effects, render the screen": a terminal is
two-way, and Varde *is* the terminal for the children in its hosted panes. Programs print escape
sequences to ask where the cursor is, what the terminal is, and whether advanced modifier reporting
is available, then read the reply on their own stdin. Reading that as an output-only rule is why
Varde answered nothing for its whole life, and a child that hears silence picks the dumbest
fallback it has and never says so. Anything the edge learns from a child — its output, its queries,
its modes — comes back in as an `Event` or is answered as data the library decided.

- No scenario covers the edge, by design. It is verified by running it (`cargo run -- <folder>`).
- **Input interpretation is not edge work.** `src/keys.rs` and `src/mouse.rs` turn a
  `terminput::KeyEvent` or a mouse `Input` into events and are unit tested; `main.rs` only converts
  crossterm into those types. If you find yourself deciding what a key or click *means* in
  `main.rs`, it belongs in the library with a test. The key type is the terminal library's lossless
  one on purpose: Varde had a narrow enum of the shapes the editor was bound to, and its catch-all
  variant silently dropped every key nobody had named yet. Widen the type or add a test; never add
  a shape and a fallback.
- **Every motion is reachable without a modifier.** Modifier bindings are aliases, never the only
  way to reach something. Option is not Alt on macOS unless the terminal is told to make it so, and
  a stock tmux strips the Kitty protocol's modifier reports — so a modifier-only binding is a
  feature that silently does not exist for whoever has not configured their terminal. `w`/`b`
  exists next to Alt+arrow for exactly this reason, and `gt`/`gT` — which has no modifier alias
  at all — for the same one.
- **The cheatsheet is the contract for what is bindable.** A key nobody can discover is a key
  nobody uses: `gt` was bound for months and missing from the cheatsheet, so it went unused
  by the person who wrote it. It lives in `keys.rs`'s `CHEATSHEET`, next to the bindings it
  describes, so a test can see it and `ui` only draws it. That test drives the same `every_key` the
  hosted panes are swept with — every key code the input type can express, in all sixty-four
  modifier combinations, Unicode sampled rather than enumerated — because a candidate list of its
  own is a candidate list with its own blind spot, and it drives every chord a waiting operator
  makes, since `gt` was a chord. Each one that does something is held to being listed or named in an
  explicit omissions list, which makes leaving one out a decision rather than an oversight. A label
  names the *gesture*, not the event: a modifier the router never inspects — Super on a character,
  Alt on a Ctrl binding — names no gesture of its own, so it is folded into the one it triggers
  rather than adding sixty-four spellings nobody would print. The spellings are exhaustive over the
  key codes, so a code the input crate grows will not compile until somebody spells it — though
  driving it still means adding it to `every_key` by hand. A label groups keys, so one test holds
  every key to doing what the gesture its label names does: a group that hides a difference is the
  blind spot the sweep exists to close.
- **One layout.** `src/layout.rs` owns where the panes are — including `GUTTER`, the width of the
  editor's line-number strip, because a drag has to know where text starts; `ui` derives its ratatui
  rectangles from it and the mouse hit-tests against it. They used to compute it separately, which is how a click
  lands one row off. Its tests pin the exact rectangles ratatui's solver produced, so a change there
  is a visible change on screen and will fail. **Match on `Pane` exhaustively when a rectangle is
  chosen from it** — a `_ =>` arm is how the AI pane came to be hit-tested against the terminal's
  rectangle, so every drag in it asked for a span of the wrong pane and its output could not be
  copied at all. Exhaustiveness turns a fourth pane into a compiler error rather than a silent
  wrong answer.
- **Scrolling is state, clamped in `update`.** `tree_scroll` and `editor_scroll` are the
  first row each list shows, and `Event::Resized` tells the core the screen size so it can
  bound them. Every event but the wheel pulls them back so the selection and the cursor stay
  visible, which is why no arm has to remember to scroll. **The tick is the second exception**, and
  for the same reason: it is the one event the user did not cause, so a tick eighty milliseconds
  behind a wheel that had moved the tree would undo it and leave the tree unscrollable for as long
  as a job runs. Its arm returns before the clamp. **A Reading's position report is the same
  exception, not a third one** — where the sound has got to is what the machine did, and a report
  arriving on the tick's own cadence that pulled a wheeled editor back to the cursor would make it
  unscrollable for as long as a passage plays. The rule earns its keep by covering everything a
  *person* did; anything else earning an exception has to be something nobody pressed. The renderer and the mouse
  hit-test *read the same field* rather than each deriving an offset — same reason as the
  one-layout rule. A pane's row count is not its height: chrome inside the borders (the
  tree's filter box) takes rows too, and measuring it wrong leaves the last rows unreachable.
  A pty pane whose child asked for mouse events does not scroll: the wheel is reported to it
  instead, because a full-screen program like an AI CLI is on the alternate screen,
  where vt100 has no history to show. Anything forwarded to a child is translated into *its*
  grid first — a screen position means nothing to a program living inside a pane.
- **A fact only the edge can observe is told, never remembered.** Everything the core knows about a
  hosted pane's child — each one's mouse encoding, each one's paste mode, and whether an AI session
  exists at all — is set by `tell_core` in `main.rs` from the panes the edge holds. `update` reads
  those fields and never writes them; a `State` field the core sets *and* the edge observes has two
  authors and will diverge. `ai_running` was that field: core state, set when a spawn was *asked
  for* and cleared by an event, and a spawn that failed cleared nothing — so the core routed keys to
  a pane the edge did not hold, and refused `:ai` because a session was "already running", which is
  a dead pane with no way back. What a derived field cannot carry is the *consequence* of losing a
  session: a review queued for it must not be handed to the next one. So every site that stops
  holding a pane also queues `AiExited` — the child exiting, a spawn that failed, and `Effect::StopAi`
  replacing a session mid-`:ai!`. Grep `edge.ai = None`: each one is followed by that push.
- **A click's press is ours, its release is the child's.** A pty pane whose child asked for mouse
  events gets the click so its own buttons work — but on the *release*, and only if
  the pointer did not move in between. Forwarding the press as it arrives means every drag activates
  whatever the selection started on, because a drag begins with a press. `Pointer::dragged` tells the
  two apart, and the report carries press and release together, since a child left holding a button
  reads the next move as a drag of its own.
- **A drag belongs to the pane its button went down in, and the edge tells it where it is held.**
  `mouse::Pointer::pane` is resolved once, at the press; resolving it per report handed a selection
  dragged out of the editor to whatever pane it crossed, or to no pane at all, and it stopped
  growing. Held at or past that pane's first or last row or column of text, the drag names the place
  one step further on and moves the *caret* there — nothing scrolls a view directly, because
  `settle` already pulls both offsets back over the caret and the tree's selection, and a second
  author for an offset is a click landing a row off. The first row of text is a trigger and not
  only the last: the editor's top border is row 0 of the screen, so there is nothing above it to
  drag onto. The edge remembers the report and plays it again on the cadence above rather than
  working a row out itself — a pointer held still sends nothing, and arithmetic in `main.rs` is
  arithmetic without a test. Where a pane's text starts and how much of it there is, is
  `mouse::text_area` — the one rectangle `place_in` reads a screen cell against and a drag runs out
  of, counted by `crate::fits_in` off the very rectangles the renderer drew. Two derivations of where
  text begins is a click landing a column off the row it copies.
- **A mouse report is bytes, so it is the library's to build.** `mouse::report` encodes it and the
  core returns it as the same `SendKeys` effect keystrokes use, which is what puts the encoding
  under a test that watches bytes reach a pane. The encoding is the child's choice, never ours:
  sending SGR to a child that enabled legacy tracking is how clicks used to leave literal
  characters in an AI CLI's prompt. The legacy encoding cannot express a coordinate past 223, and
  a cell it cannot name is declined rather than wrapped — a wrapped coordinate is a click landing
  where nobody pointed.
- **A reply to the child is a decision, so it is the library's to make.** `queries::reply` is a pure
  function from a sequence's intermediate, parameters and final character — plus the cursor the
  callback reads off the screen, which the cursor-position reply cannot be built without — to the
  reply bytes or nothing; `pty.rs` receives the parser's unhandled-sequence callback, queues it, and
  writes it after the batch — the pty writer is not reachable from inside a parse. Every channel
  must say the same thing: the device-attributes reply names a deliberately low VT220 because Varde
  refuses modifier reporting, and claiming a recent xterm would invite a program to enable
  `modifyOtherKeys` and then read Varde's legacy key bytes under the wrong rules — which is also why
  the version query answers `VARDE(x.y.z)` rather than an xterm build. Refuse a query out loud;
  never by silence.
- Drag-to-select is the one thing the library cannot finish: reading characters needs the pty, so
  `mouse::on_mouse` returns a `Selection` request and the edge fulfils it. The request is a span in
  the pane's *own* text coordinates — a buffer's line and column, or a cell in a pty's grid — with
  the pane origin, the gutter, the scroll offset and the drag's direction already resolved, and
  `editor::span_text` turns lines into text. The edge reads characters and decides nothing: a drag
  spanning rows is arithmetic, and arithmetic in `main.rs` is arithmetic without a test.
- **A blank pty cell is a space.** A cell never written to, and the second half of a wide character,
  both hold no contents at all, so collecting only what the cells hold collapses a row and every
  column past the first gap names the wrong character. `editor::grid_row` substitutes the space the
  renderer draws, which is what keeps a column on screen and a column in the extracted text the same
  column. Reading a grid without it is why a drag over an AI CLI's indented, emoji-laden output
  picked nothing while a drag over its unpadded prompt line looked like it worked.
- To verify it without a terminal, drive it through a pty and replay the bytes:
  `script -q /dev/null bash -c 'stty rows 26 columns 100; ./varde .' > out` while feeding keys on
  stdin, then `cargo run --example replay -- out` renders the final frame. Hold stdin open with a
  trailing `sleep` — closing the pipe sends EOF to the shell, which exits and takes Varde with it.
- If you find yourself writing an `if` in `main.rs` that decides *product behaviour*, it belongs in
  `update` with a scenario instead.
- vt100 panics on a zero-sized grid, and a terminal that has not reported its size yet gives zero.
  Clamp every size to at least **two rows** and one column, and resize the parser *before* feeding
  it output. Two, not one: wrapping a column needs a row to scroll into, and on a one-row grid vt100
  subtracts the scroll off the row it came from and underflows — so a one-row clamp is not a clamp,
  it is the same panic on the second character the child prints. An eight-row window leaves the shell
  one row, and an occupied Corner squeezes it to one *column*, where every character wraps; that
  combination is how "clamp to at least 1" read as safe for Varde's whole life.
- **Draw only when something changed.** Input polling stays at 16ms so latency is unchanged, but the
  frame — and the syntax parse behind it — happens only after a key, a mouse event, pty output, a
  watcher event, or a git status that actually differs. Idle CPU is 0%. **Work in flight is the one
  other redraw source**, so a spinner can turn while an analysis runs and a Reading's place can
  follow the voice while a passage plays
  (`docs/adr/0009-a-spinner-is-bounded-by-its-job.md`): the edge queues `Event::Tick` while — and
  only while — it holds work, and `Event::Speaking` while — and only while — it holds a player, and
  it reports a held drag again while — and only while — `mouse::Pointer::held` names one, so the
  exception is bounded by construction rather than by discipline, and with nothing in flight the
  rule above is the whole of it. All three are on the same 80ms cadence: a spinner's blade, a mark
  that moves once a sentence and a selection creeping a line at a time are none of them worth sixty
  frames a second. A tick is an event like any other, so
  it is batched, not drawn on its own.
- **One frame per batch of input, never one per event.** `pump` drains everything crossterm already
  has (up to `INPUT_BATCH`) before drawing. A trackpad flick arrives as hundreds of wheel events and
  a frame costs milliseconds: drawing each one queued seconds of stale frames, which reads as the TUI
  freezing. Measured: 200 wheel events cost 253 draws and 2.3s before, 7 draws after.
  **One frame per batch, but one *interpretation* per key**: each key is run through `update` before
  the next is read, because what a key means depends on the state the key before it left. Deciding a
  whole batch against the state it started with is how `/` opened an in-file search and the letters
  pasted behind it went to the buffer instead of the query. A modal or line whose contents live in
  `Drafts` hides this — a `&mut` updates as the batch decodes — so it only bites the input the core
  owns.
- **Never parse per frame.** The edge caches the current buffer's tokens against `Buffer::revision`,
  which is bumped by every content change and nothing else. The parse itself runs on a thread, after
  an edit as much as on opening: until it lands, the last parse is carried onto the new text
  (`highlight::carried`), because a re-parse of a big file on the loop held a keystroke (#101).
- **Draw only the rows on screen.** The editor settles which lines it shows (the scroll offset, the
  pane's height and the folds, through `story::rows`) before building anything, then builds and
  marks only those. An analysis that needs the whole file is worked out once per revision, never
  once per frame: the buffer's `Shape` (line starts, depths, bracket pairs) with the edit itself,
  and the Change-bar trace (`State::traced`) by the edge before the frame that draws it, keyed by
  the revision so a trace of the text before an edit is never read. Only a parse — syntect, or the
  tree-sitter behind the Run marks — runs off the loop, and it is carried onto the new text until it
  lands. Built whole and scrolled, a frame of a 600 KB file cost 90 ms (#101).
- **Never type at a pty the instant you spawn it.** A CLI that has not printed its prompt yet drops
  what you send, so the edge reports when the child is ready (`Event::AiSpoke`) and the core holds
  what is queued until then. Ready is not its first byte: a CLI opens with cursor housekeeping and
  asks for bracketed paste a chunk later, and a prompt sent between the two went in bare and was
  submitted at its first newline. So the edge waits for the paste request, or for a child that
  printed and never asked to go quiet. No fixed delay from spawn: a guessed one is wrong on a slow
  start and slow on a fast one. Once it has spoken the text goes in one write, Enter included — bracketed as a paste
  when the child asked to be told a paste from typing, which is what stops a newline mid-text from
  being read as a submit.
- **A child's environment is Varde's, not the host's.** `queries::CHILD_ENV` is the whole terminal
  identity a hosted pane hands its child, and `Pane::spawn` walks it: `TERM` alone was not enough,
  because a CLI that sniffs `TERM_PROGRAM`, `KITTY_*`, `TMUX` or any other marker got an answer about
  the user's machine. It lives beside the escape-sequence replies, so a change to one identity is
  read next to the other. It may name terminal emulators where nothing may name a CLI provider
  because emulators are a small, slow-moving set, a missed marker costs one CLI a cosmetic
  difference rather than a broken feature, and nothing branches on the list — it is data, not a code
  path, and not licence for the branching this file forbids. It removes rather than invents: an
  unset `TERM_PROGRAM` is what xterm gives a child too, and a made-up name is one someone will
  branch on. `COLORTERM` stays, because Varde passes 24-bit colour through; `TERMINFO_DIRS` stays,
  because a capability database is not an identity and ncurses needs it to find an entry at all.

## Working philosophy: simplicity first

New code is the fallback, not the default. Work down this ladder before writing anything:

1. **No change** — is this a usage, configuration, or expectation problem?
2. **Latent capability** — a framework feature, library option, config flag, database constraint,
   or HTTP semantic that already solves it
3. **Recompose what exists** — rearrange or reuse existing components/configs
4. **Minimal new code** in existing files — prefer deleting over adding
5. **New files, modules, or dependencies** — last resort; justify each one

- Smallest diff that fully solves the task
- Match existing conventions — never introduce new frameworks, patterns, or dependencies unilaterally
- Weigh at least two approaches before committing to a design

## Methodology: BDD → DDD → TDD

**Every TUI behavior starts as a Gherkin scenario.** No feature work begins with implementation
code. The loop is: write the scenario → run `cargo test` and watch it fail as *undefined* → implement
step definitions and the `src/` function → green.

**Start every session by running `cargo test`.** The suite is the single source of truth for what is
done: scenarios report as undefined (not yet implemented), failed, or passed, grouped by feature.
Do not keep a hand-written progress checklist anywhere — it drifts, and a stale one is worse than
none. `docs/example-map.md` tracks the *spec*; the suite tracks the *work*.

**The whole spec comes first.** Implementation does not begin feature by feature — it begins when
the full feature set is defined in Gherkin and `docs/example-map.md` has no blocking questions left.
Specifying everything up front is what surfaces cross-feature couplings (a config key one feature
owns and another reads) and design changes that *delete* specification work, while they are still
cheap. Do not offer to start implementing early.

| Practice | Altitude | In this repo |
|----------|----------|--------------|
| **BDD** | System behavior | Scenarios in stakeholder language: happy path + key failure modes per feature. Location: `features/*.feature` |
| **DDD** | Architecture | `src/` modules map to workspace concepts (landing page, file tree, …), not to TUI widgets |
| **TDD** | Implementation | Test-first (red → green → refactor). Unit tests live next to the unit they cover. |

### Writing scenarios

- `Given` = pre-existing context · `When` = the single event under test · `Then` = an observable,
  falsifiable outcome. Never split one event across `Given` and `When`.
- Declarative, not imperative: `Given I opened the TUI in the folder "…"`, never argv or key codes.
- A `Then` that cannot fail is worse than no `Then`. If in doubt, break the implementation on
  purpose and confirm the scenario goes red.
- Step definitions contain glue only: pull values off the table/params, call a `src/` function,
  assert. Logic belongs in `src/`.
- State moves between steps through the `VardeWorld` struct in `tests/cucumber.rs`. Cucumber builds
  a fresh one per scenario, so scenarios cannot leak into each other — keep it that way, and never
  reach for a global or a `static`.

### Conventions the existing scenarios rely on

- **Assert state, not copy.** Scenarios check enums like `"not-a-git-repository"`,
  `"no-such-folder"`, `"changes-requested"` — never user-facing wording. Copy changes constantly; a
  suite that fails on rewording teaches people to ignore it. Pin exact strings in a unit test if it
  ever matters.
- **Assert absences deliberately.** `no command has been executed`, `the project ".gitignore" is
  unchanged`, `no file was opened in the editor` are load-bearing. They encode what Varde promises
  *not* to do, and they are the assertions a plausible-looking implementation quietly breaks.
- **Inject the clock.** `view_mode.feature` specifies a 300 ms double-tap window and a 450 ms
  non-tap. Drive these from a fake clock behind a trait — never `sleep` or real waiting. Real time is
  how a behaviour suite becomes slow and flaky.
- **DocStrings for expected values containing quotes.** Gherkin substitutes `<placeholders>` inside
  DocStrings, which is how the quoting Outline in `tree_actions.feature` expresses an expected
  command containing both `'` and `"`.
- **Data tables:** some scenarios use a header row (`| name | kind |`), some do not (a bare list of
  paths). Check which before indexing rows — the headerless ones have data in row 0.

## Test strategy: deliberate, not mindless

Tests are maintained forever — every test must pay rent.

- **Few** behavior tests per feature prove the whole slice end-to-end
- **Many** fast unit tests cover the logic branches underneath
- Never chase branch coverage through behavior tests; never re-prove wiring with unit tests
- Don't test getters, framework glue, or what the compiler already proves
- **Never drive a real pty or screen-scrape ANSI output in tests.** Assert on the view model that
  `src/` returns. If a terminal-level smoke test ever becomes necessary, it is one scenario, tagged,
  and justified here first.
- Behavior tests: `features/*.feature`, step definitions in `tests/` · Unit tests: `#[cfg(test)] mod
  tests` beside the code they cover — see `highlight` and the diff-clamp test in `lib.rs`.
- `.fail_on_skipped()` is set in `tests/cucumber.rs` so undefined and skipped steps fail the build.
  Without it a mistyped step name reads as a pass. Do not relax it.

## Security (OWASP)

Per https://owasp.org/: validate at trust boundaries; parameterized queries only; use the
framework's auth/authz and established secret stores — never hand-rolled crypto or session
handling; no secrets in code, logs, or images; never render untrusted HTML unsanitized.

For a TUI specifically: treat folder paths and file contents as untrusted input. Never interpolate
them into a shell command, and strip control/escape sequences before writing file-derived text to
the terminal — raw ANSI in a filename can rewrite the screen.

## Error handling: never silent, never raw

Failures must surface. Log with full technical context; show users/consumers a human-readable
message via the project's error envelope / i18n. No empty catch blocks, no `|| true`, no stack
traces or internal details in user-facing output.

## Conventions

- Build: `cargo build` · Test: `cargo test` (BDD only: `cargo test --test cucumber`) · Run: `cargo run -- <folder>` (not yet implemented)
- Lint: `cargo clippy -- -D warnings` · Format: `cargo fmt`
- Comments: none, apart from a one-line note on a genuinely edge-case quirk (see Code budget)
- Git: agents may commit — push or branch only when explicitly asked
- **A program Varde shells out to is installable, or the feature is not done.** A configured one
  is a template row in `startup::PROGRAMS` with an `install.<os>` key, taken from Tools inside
  Varde; `install.sh` learns the package manager that key starts with by asking the installed binary
  for `varde --deps`, and needs no edit for it. An unconfigured one (the toolchain, git, the default
  AI CLI, the player, the URL opener) is a line in the script. `./install.sh --list` shows what it sees. A feature that
  works on the machine that wrote it and nowhere else is the failure this closes: `docs/install.md`.
- Version bump: an agent asked to commit bumps the `version` in `Cargo.toml` in that same commit —
  major for a breaking change, minor for user-visible behaviour, patch for a fix. Build before you
  commit, so `Cargo.lock` moves with the manifest and the next build does not dirty the tree with a
  file nobody edited. A separate bump commit is rejected: it can be forgotten independently, which
  is the exact failure the rule exists to prevent. Forgetting to bump stays silent on purpose —
  nothing fails and no test catches it, because a number that moved on every commit would raise the
  update notice constantly and teach the user to ignore it. Why the number and not the commit hash:
  `docs/adr/0003-versions-not-commits.md`.
- More structural detail: `.claude/project-map.md` (if present)
