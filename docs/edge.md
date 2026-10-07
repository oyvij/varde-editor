# The edge

The rule that frames all of this is in AGENTS.md, under The edge.

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
  `python3 examples/drive.py out 26 100 @400 ':' @300 -- ./target/debug/varde <folder>`, then
  `cargo run --example replay -- out 26 100` renders the final frame. A step is `@ms` to wait or keys
  with Python escapes (`'\r'`, `'\x1b'`). The driver answers Varde's start-up queries (colours,
  device attributes, cursor position) as a terminal would, and kills Varde after the last step; it
  runs the same on macOS and Linux. `script` does neither, so Varde waits on its queries and never
  exits.
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
