# Varde — Example Map

**C**ommand · **R**eview · **I**ntegrated · **M**odal · **E**ditor

Working document for defining behaviour *before* writing Gherkin. Structure follows Example Mapping:
**Rules** (blue) — what must be true. **Examples** (green) — concrete cases that become scenarios.
**Questions** (red) — unresolved; a question left open here becomes a wrong scenario later.

Status legend: ✅ **scenario is written** · ❓ open question · ⛔ deliberately not a scenario

> ✅ means specified, **not implemented**. This document tracks the spec only. Implementation
> progress is not recorded here and must not be — see "How to resume" below.

---

## The product, as described

A terminal IDE with four regions in **Edit view**:

- **Left** — file tree: folders, files, type icons, per-row action buttons
- **Bottom** — real terminal (bash or whatever the user's shell is); fast, responsive, ordinary
- **Centre** — text editor: line numbers, syntax colours, vim-style modal editing
- **Right** — an AI CLI (Claude Code or another)

`Ctrl+Ctrl` opens **View mode**, which re-renders the whole TUI into one of:

- `(e)` **Edit view** — the four regions above
- `(r)` **Review view** — only files changed per git, reviewed like Monocle with
  `ISSUE` / `NOTE` / `SUGGESTION` / `COMMENT`; submitted reviews are picked up by the AI. Editing
  still works here, but the view exists for reading changes.

The TUI auto-updates when files change on disk.

Configuration and state live in `.varde/` — global at `~/.varde/` (created at install), per-project at
`<project>/.varde/` (created when the TUI starts in that folder).

---

## The load-bearing design rule

**Tree actions never touch the filesystem themselves. Every one of them becomes a shell command in
the terminal, so the terminal is the single place a file is made or removed and the user can read
exactly what was run.**

Whether that command *runs* turns on one thing: is it finished? An action that already names
everything it needs runs. Only `cd` waits, because a cd is the opening of whatever the user meant
to type next, not the whole of it. Any new action states which side of that line it is on here
first.

- New file → box for the name, then runs
- New directory → same
- Delete file or directory → runs
- Folder "go here" button → injects `cd <dir>` and waits for the user's enter
- A "back to project root" action exists, and runs (Q11)

Running was originally forbidden outright (the sole exception being "back to project root"), on the
grounds that the enter the user presses is the confirmation. That traded a real cost — every created
file needed an enter in a pane the user had to be moved to, and then a trip back to the tree — for a
confirmation nobody read. What replaces it is *reversibility of attention*: a command that ran leaves
the terminal with nothing to type, so focus returns to the tree with the row it was about selected.
The command and its output are on screen either way.

This is still the most testable thing in the whole product and the most valuable to lock down: the
assertion is *"the terminal ran exactly X"* or *"X is waiting on the input line and nothing ran"*. It
is pure data — no pty, no filesystem. Most of our behaviour coverage should live here.

---

## What gets a Cucumber scenario, and what does not

| Area | Verdict | Why |
|---|---|---|
| Command injection from tree actions | ✅ scenarios | Pure input/output, high stakes, user-visible contract |
| View mode switching (`Ctrl+Ctrl`, `e`, `r`) | ✅ scenarios | A state machine — exactly what Gherkin is good at |
| Review capture and submission | ✅ scenarios | Data in, data out; the AI hand-off is a contract |
| Review view's file list (git changed set) | ✅ scenarios | A rule about *which* files appear, independent of rendering |
| File tree contents, ordering, dotfiles | ✅ scenarios | Product decisions someone can disagree with |
| Config precedence (project over global) | ✅ scenarios | A rule with a right answer |
| Auto-update on file change | ✅ scenarios | Especially the unsaved-edits conflict case |
| Icon-per-filetype mapping | ⛔ unit test | A lookup table; a scenario per extension is rent no one pays |
| Syntax highlighting | ✅ scenarios (kinds only) | F14 asserts which *kind* text is — keyword, string, comment. Never the colour: themes change, and a suite that fails on a palette tweak teaches people to ignore it |
| Story authoring, spine, walking, predictions | ✅ scenarios | Data in, data out: an artifact arrives, a view model comes back. No pty, no AI, no clock |
| Which keys each view's cheatsheet lists | ⛔ unit test | It is copy, and `keys.rs`'s sweep already holds every key to being listed or explicitly omitted — per view, from F22 on |
| Risk figures, staleness, the Gate, the loop's verdicts | ✅ scenarios | Data in, data out through the one seam: "the tests were run", "nothing was committed", "these files were restored" are comparisons of two values |
| Where the Risk pane's rectangle is | ⛔ unit test | Pinned rectangles, as F10's are. A scenario cannot see a rectangle; the tests that can are what catch a click landing one row off |
| The analyser itself, and its six languages | ⛔ neither | Someone else's tree-sitter crate. No test drives it over this repository, runs a real test command, or spawns a real session |
| The spinner's tick, and zero idle CPU | ⛔ not Gherkin | Edge properties, measured by running it — ADR 0009 states them as ticket criteria instead |
| Vim keybinding coverage | ⛔ unit test | Hundreds of bindings; scenarios would be a transcription, not a spec |
| Terminal emulation itself | ⛔ neither | It's a real shell. Testing bash is not our job |
| "Fast and responsive" | ⛔ not Gherkin | A performance budget, not a behaviour. Measure it separately |
| AI CLI pane internals | ⛔ neither | It's someone else's process in a pty |

---

## Entry points

**Every feature must say how a user starts it.** Five features were once fully specified, fully
tested and completely unreachable — the spec described what they *did* and nothing caused them. This
table is the audit; keep it current, and add a row before writing a new feature's scenarios.

| Feature | How it's invoked |
|---|---|
| F1 startup | `varde <folder>`; `varde` with no argument for a Bare workspace |
| F2 file tree | drawn on start; `Enter`/click expands; `c` or palette `c` collapses all |
| F3 tree actions | row icons, `n` `N` `d` on the focused row, `-` for back-to-root |
| F5 watching | automatic (`notify`) |
| F6 command palette | `Ctrl+Ctrl` or `Ctrl+Space`; `Esc Esc` in a hosted pane; entries clickable |
| F7 review view | palette `r` |
| F8 submission | `V`+`c` or drag the diff gutter; `:submit` |
| F9 config & state | startup |
| F10 mouse | click, scroll, drag |
| F11 keyboard | `Alt+hjkl` focus, palette `o` `d` `t` `a` focus, arrows, `Enter` |
| F12 row affordances | icons on the focused row, `→` to step into them, `n` `N` `d`, `-` `c` |
| F13 editing | keys in the editor pane |
| F14 highlighting | automatic |
| F15 close & quit | `:q` `:q!` `:wq`, `:qa` `:qa!`, `Ctrl+Q`, palette `q` |
| F16 selection & copy | drag to select; double-click a word; `Ctrl+C`/`Cmd+C` to copy, `Ctrl+V`/`Cmd+V` to paste |
| F17 review flow | palette `r`, arrows, `Enter`, `e`, `V`+`c`, `:submit` |
| F18 AI pane | the pane's own input box, palette `a`, `:ai <cli>`, palette `l` or `:tall`, `:inject` or the pane's border icon |
| F19 buffers | `gt` `gT`, palette `g`, clickable dots, tree marks |
| F20 tree filter | `/` in the tree, `Enter` marks the best match in the tree and opens its folders |
| F21 content search | `Ctrl+F`, palette `f`, `*` on a word, `gr` on a selection |
| F27 Risk figure | automatic at startup; the Risk pane's recompute action |
| F28 Risk list | palette `k`; `Alt+j` from the tree once shown; row icons and `Enter` inside it |
| F29 Refactor loop | the Risk pane's own action, which becomes the stop action while it runs |
| F30 Risk in review | automatic on entering Review view; the pane's action starts the review-scoped loop |
| F39 binary releases | automatic at startup on a binary install; `:update` or palette `u` installs the Release and relaunches; `varde --deps`; `install.sh` |

`Event::AddComment` has no entry point by design — it is a `Given` shortcut so submission scenarios
can set up a review in one step. The user's path is `V`+`c`, covered by F17.

## F1 — Workspace startup — **DEFINED**

Scenarios live in `features/workspace_startup.feature`. `.varde/` creation and state reuse are
specified in F9 (`features/configuration.feature`), not repeated here. What `varde` with **no**
argument does differently — the folder is the workspace, but Varde's own files go to a Sidecar
outside it — is `features/bare_workspace.feature`, specified in
`.scratch/bare-workspace-and-branch-stories/spec.md` and argued in
`docs/adr/0016-a-bare-workspace-leaves-nothing-behind.md`.

**R1.1** Opening Varde on a folder makes that folder the workspace root; the title is its basename.
**R1.2** An **empty folder is a valid workspace** (Q3) — that is how a project begins.
**R1.3** A path Varde cannot use stops it before the TUI appears, with a **distinct reason** per case
(Q2): `no-such-folder`, `not-a-folder`, `folder-not-readable`.

- ✅ The folder opened becomes the workspace
- ✅ An empty folder is a valid workspace
- ✅ A path that does not exist stops Varde
- ✅ A path that is a file stops Varde
- ✅ A folder that cannot be read stops Varde

## F2 — File tree display — **DEFINED**

Scenarios live in `features/file_tree.feature`.

**R2.1** **Directories first, then files**, each alphabetical (Q4).
**R2.2** **All dotfiles are shown** (Q5), including `.git/` — no exceptions (Q43).
**R2.3** Git-ignored paths are **shown but dimmed** (Q6) — visible, visually deprioritised.
**R2.4** Icons derive from type — a lookup table, covered by unit tests, not scenarios.

- ✅ Directories are listed before files, each alphabetically
- ✅ Dotfiles are shown
- ✅ The .git folder is shown like any other dotfile
- ✅ Git-ignored paths are shown but dimmed

**R2.5** The tree is **lazy** (Q7): a folder is read *and watched* only while expanded; collapsing it
stops watching. Startup cost is independent of repo size, and `.git/objects` and `node_modules` are
never walked unless deliberately opened — which is what makes R2.2 and R2.3 affordable.
**R2.6** Expansion state is **restored from `state.json`**; a project opened for the first time shows
only the top level (Q7b).

- ✅ A project opened for the first time shows only the top level
- ✅ Expanding a folder lists its contents
- ✅ Reopening a project restores the folders that were expanded

Consequence for F5, now specified there: a file created inside a **collapsed** folder is not listed
until that folder is expanded. Lazy watching means Varde genuinely does not know about it yet. This
is the one place where "the TUI autoupdates" is bounded by what you have open.

**R2.7** **Collapsing** empties the set of open folders in one gesture, because a folder opened once
stays open (R2.5) and a tree explored all day only ever grows. It runs no command: nothing on disk
changes, so unlike every other gesture on the tree this one is not an F3 action. The tree is left
showing the workspace root's own entries, and a folder opens again from there exactly as it does on
a project opened for the first time — the state is the same one R2.6 describes. Watching is
unaffected by construction — R5.6's watch set unions the open folders with what is *open in the
editor*, so a collapsed folder holding a buffer stays watched. Collapsing is a **gesture, not a
setting**: nothing persists it beyond the expansion state R2.6 already saves.

The **row selection is left exactly where it is**, and is deliberately given no rule of its own. A
row still on screen stays selected. One inside a folder that just closed keeps naming a path that is
no longer drawn — so until the next keypress it carries no highlight and offers no row actions — and
the next Up or Down lands on the tree's first row, which is R11.3's existing answer for a selection
it cannot find. Nothing here reconciles the selection itself: the clamp reached on every event bounds
the tree's *scroll*, and with the selection off screen it reads as row 0, which is what brings the
tree back to the top where the remaining rows are. Giving collapsing a rule of its own was
considered and rejected — what an off-screen selection means is a question the filter already has
(R20.2), and it wants one answer rather than one per gesture.

Reached by `c` with the tree pane focused, and by the palette's `c` from anywhere. The keyboard
route's scenario lives with the tree's other keys in `features/tree_action_affordances.feature`.
Like the tree's other keys, `c` is the *pane's*, not the view's: it answers wherever that pane holds
the keyboard, and in Review or Story view — where the pane lists changed files or the spine — it
still empties the Edit view's open folders, which is the tree the reader will be back to. And like
them it is not reachable while the filter box is collecting, since every printable key there belongs
to the query; the palette route is the way in from a filtered tree.

- ✅ Collapsing closes every open folder at once
- ✅ Collapsing with nothing open changes nothing
- ✅ A selected row that is still there stays selected
- ✅ A selection inside a folder that closed lands on a row that is there
- ✅ Collapsing leaves the filter narrowing the tree
- ✅ Collapsing is reachable from the palette
- ✅ C collapses every open folder

## F3 — Tree actions run commands in the terminal — **DEFINED**

Scenarios live in `features/tree_actions.feature`. All questions answered.

**R3.1** Every tree action becomes a shell command in the terminal; nothing in `src/` touches the
filesystem. A command that names everything it needs **runs**; "go here" is injected and waits.
**R3.2** Actions that need a name (new file, new directory) collect it in a **small prompt box**,
then run the **complete** command. Esc cancels the box and runs nothing. A name may be a
relative path; missing parents are created by the same command (`mkdir -p … && touch …`).
**R3.3** Paths are always **absolute** (Q8), so an action is correct wherever the shell is.
**R3.4** Injection **replaces** whatever the user had typed on the input line (Q9).
**R3.5** Paths are quoted **only when they need it** (Q12): bare when plain, single-quoted when they
contain spaces or shell metacharacters, double-quoted when they contain a single quote.
**R3.6** "Back to project root" **executes immediately** (Q11).
**R3.7** Focus follows what is left to do. An injected command takes the terminal, because the enter
that finishes it has to land there. A command that **ran** leaves nothing to type, so focus goes to
the **tree**, and the row the action was about is selected: the created path, or — for a delete, whose
row is about to stop existing — the folder that held it. Creating also reads the folder the file
lands in, which is both what puts it in the tree and (R5.6) what puts it under the watcher.
The selection is a *path*, so it names the new file before the row exists and lights up when the
watcher delivers it — no "the command finished" signal is needed, which is what F4 could not get.

Command forms (Q10, Q-new):

| Action | Command | Runs? |
|---|---|---|
| Go here (folder) | `cd <abs-dir>` | no — waits for the user's enter |
| New file | box → `touch <abs-path>`, or `mkdir -p <parent> && touch <abs-path>` when nested | yes |
| New directory | box → `mkdir -p <abs-path>` | yes |
| Delete file | `rm <abs-path>` | yes |
| Delete directory | `rm -r <abs-path>` | yes |
| Back to root | `cd <root>` | yes |

No `-i`, no `-f`, no trash. The name box is the deliberate act for a create. A delete has none: it
is one keystroke on a focused row, and `rm -r` is not undoable — see Q36.

- ✅ Going to a folder
- ✅ Creating a new file asks for a name first
- ✅ Confirming a name runs the complete command
- ✅ A nested name creates the missing parent folders
- ✅ Creating a new directory
- ✅ Cancelling the name box runs nothing
- ✅ A name that needs quoting is quoted
- ✅ Deleting a file
- ✅ Deleting a directory uses the recursive form
- ✅ Injecting replaces text the user had already typed
- ✅ Paths are quoted only when they need it (Outline: plain, space, apostrophe, `;`)
- ✅ Returning to the project root is the one action that runs
- ✅ Injecting a command focuses the terminal
- ✅ A created file is selected in the tree, ready to open
- ✅ The folder a new file lands in is opened, so the file can show up
- ✅ A new directory is selected in the tree
- ✅ Deleting leaves the selection on the folder that held the file
- ✅ Asking for a name leaves focus alone

Deferred, worth revisiting once the above are green:

- ✅ Q34 — **Resolved.** `shlex` handles a path containing both quote characters
  (`/tmp/don't"quote.txt` → `"/tmp/don't\"quote.txt"`), and its output matches the F3 Outline
  exactly as written. No scenario change was needed. See `docs/stack.md`.
- ✅ Q35 — **Resolved by construction.** The root is not a row, so it is never `tree_selection` and
  never a `Target`; neither `d` nor a row icon can reach it. Nothing to block.
- ❓ Q36 — Does `delete` want a confirmation now that it runs? Under R3.1's original form the user's
  enter was the confirmation; R3.7 spends it. `d` on a focused row is now one keystroke from
  `rm -r <folder>`. The options are a `Modal::ConfirmDelete` (matching `ConfirmSubmit`, which exists
  for a *recoverable* loss), or leaving it and treating the terminal's visible command as the record.

## F4 — New file opens in the editor — **DELETED**

Removed, not deferred. It said: a file created through the new-file flow opens in the editor. Eight
scenarios passed and it never worked once in the running app.

The trigger was deliberately the **tracked command**, not the file watcher (Q13) — so that a branch
checkout or the AI writing three files could not hijack the editor. But Varde writes into a real pty
and gets back a stream of screen bytes: there is no "the command finished, exit status 0" in that
stream. `Event::CommandRan` had no producer and could not have one without shell integration
(OSC 133, which bash does not emit by default), a prompt-shape heuristic that would sometimes open
the wrong file, or creating files directly and breaking R3.1's "tree actions never execute".

The prize was one keystroke. New files appear in the tree via the watcher; `Enter` opens them.

**Since resolved from the other side (R3.7).** What could not be had was *opening* the file, which
needs to know the command finished. *Selecting* it does not: `tree_selection` is a path, so the core
names the created path the moment the name box is confirmed and the highlight appears when the
watcher supplies the row. Focus returns to the tree, `Enter` opens it — the same one keystroke, with
no producer for `Event::CommandRan` required and F5's protective scenarios untouched, because the
editor is still never taken over by a file appearing on disk.

Its two protective scenarios moved to F5, because that rule outlives the feature: **a file appearing
on disk never takes over the editor.** They now guard against anyone wiring the watcher to the
editor later.

## F5 — Auto-update on file change## F5 — Auto-update on file change — **DEFINED**

Scenarios live in `features/file_watching.feature`.

**R5.1** The tree follows disk immediately — files created and deleted appear and disappear.
**R5.2** A buffer with **no unsaved edits follows the file** silently (Q44).
**R5.3** A buffer **with unsaved edits is never overwritten** (Q14): it is flagged as changed on disk,
and reloading is an explicit act.
**R5.4** Reloading a flagged buffer takes the disk version and clears the flag.
**R5.5** The review list **live-updates** as git state changes (Q15), driven by watching `.git/HEAD`
and `.git/index` plus ordinary workspace file events (Q7c) — never all of `.git`, which would flood
on every commit.
**R5.6** *Discovery* is bounded by expansion (R2.5): a file created in a **collapsed** folder is not
listed until it is expanded.
**R5.7** *Following* is not. Whatever is **open** is followed whether or not its folder is expanded —
an open buffer and the file a diff is shown for. Discovery asks "what is in this folder"; following
asks "did this file I am showing change", and only the first is what expansion is about. Review view
lists changed files flat, so binding the two together left a diff on screen followed only when the
tree happened to have its folder open.
**R5.8** A divergence is **announced**, not only marked: the status line says so the moment a dirty
buffer and the file part ways, and the marker names the key that resolves it.
**R5.9** A flagged buffer offers **three ways out**, on a picker opened deliberately and never by the
watcher: reload from disk, overwrite disk, or hand both versions to the AI to merge. Only the first
two settle it — a merge has been *asked for*, so the buffer stays flagged until the AI's write comes
back through the watcher. Escape is always available and loses nothing.

- ✅ A file created in an expanded folder appears in the tree
- ✅ A file created in a collapsed folder is not listed yet
- ✅ A file created in a collapsed folder appears once it is expanded
- ✅ A file deleted on disk disappears from the tree
- ✅ A clean buffer follows the file
- ✅ A buffer with unsaved edits is never overwritten
- ✅ Reloading a flagged buffer takes the disk version
- ✅ The review list follows git while reviewing
- ✅ A file open in the editor is followed even with its folder collapsed
- ✅ The file a diff is shown for is followed while reviewing
- ✅ A divergence is announced, not only marked
- ✅ A flagged buffer offers the three ways out
- ✅ Resolving by reloading / by overwriting / by merging
- ✅ Escape leaves a divergence standing

R5.2 and R5.3 together resolve a tension in the original description: "the TUI autoupdates on
changes in files" is true for the tree, the review list, and clean buffers — but a buffer holding
unsaved work is the one place where following disk would destroy something, so it waits.

## F6 — View mode — **DEFINED**

Scenarios live in `features/view_mode.feature`.

**R6.1** Double-tapping Ctrl within the window enters View mode, on terminals that report modifier
key events (Kitty keyboard protocol: Ghostty, kitty, foot, WezTerm). Terminals without it use
`Ctrl+Space` (Q16). Where the bare Ctrl press is **not** reported, a double-tapped `Esc` opens it
from a **hosted pane** instead: there Option is not Alt and `Ctrl+Space` belongs to the child, so
`Esc` is the only gesture left. It withholds no byte — both escapes still reach the child, and only
the pair Varde counted beside them opens the palette. Exactly one of the two taps is available on any
terminal: arming `Esc` where the Ctrl tap already works would take the key *after* a double-tap from
a CLI that binds `Esc Esc` itself. Its scenarios live in `features/keyboard.feature`, beside the
focus keys they are the alternative to.
**R6.2** The double-tap window is **300 ms**, configurable in `~/.varde` (Q16b).
**R6.3** The palette lists **panes, views and commands, grouped** — Panes: `(o)` Editor, `(d)`
Files, `(t)` Terminal, `(k)` Risk, `(a)` AI, `(l)` Tall · Views: `(e)` Edit, `(r)` Review, `(s)`
Story · Project: `(f)` Find, `(v)` Tools · Help: `(h)` Keys, `(u)` Update · `(q)`
Quit (Q17). *Amended with F22: a view takes its own initial, so `s` moved from submit to Story and
**submit was `u`**. Amended again once the flat list reached thirteen rows: it is **grouped**,
`Editor` joined it as a pane of its own, `(u)` became Update, and **Copy, Write and Submit left**.*
A flat list of everything reads as a heap, and the grouping is what makes `Editor` and `Edit`
legible as the different questions they are — one goes to a pane and leaves the view alone, the
other changes the view. What left is what asked something of the buffer or the review in front of
you rather than of Varde: `C-c`, `:w` and `:submit` belong to the view that answers them, and the
key box advertises each one there. What is here is what means the same thing everywhere, because one
gesture is discoverable and a colon-prefixed command line is not. This came from running it: `:` was
undiscoverable and, on a terminal using the Kitty protocol, did not arrive at all.
**R6.4** `Esc` cancels and leaves the current view untouched (Q18).
**R6.5** An unrecognised key is **ignored**; the palette stays open (Q18).
**R6.6** Choosing the view already showing dismisses the palette and re-renders nothing (Q19) — but
it still moves focus, since R6.7 would otherwise be a dead end for whoever is already in that view.
**R6.7** **Every pane is reachable from the palette**, so the modifier focus keys of R11.1 are never
the only way out of a hosted pane: `o` the editor, `d` the tree, `a` the AI pane and `t` the
terminal. `o` and not `e`: the Edit view took the initial, and reaching the editor's rectangle must
not cost a reviewer the diff they were reading.
**R6.8** `(h)` keys shows the **Cheatsheet** in the AI pane's place or hides it, the same toggle as
`:help`; the AI session keeps running behind it and comes back unchanged (#90). Toggling never moves
focus: focus on that rectangle is focus on whichever pane is in it. It scrolls and takes focus like any
pane, and nothing it is given reaches the session behind it; anything that starts the AI or sends text
into it hides the Cheatsheet first. It belongs in the palette and not only on the `:` line because a
Cheatsheet already hidden would hide the way to bring it back — and the palette is where a command
that means the same thing in every view is advertised: `:update`, `:tall` and `:help` left the
Cheatsheet. It starts hidden at every launch and is never saved. An Update is not the
Cheatsheet's to announce: the **version tag** at the right end of the bottom row names it in every view and focus — `v<running>` in green, or
`v<running> → v<newer>  C-space u to update` in blue — so it **names the gesture** without costing
the Cheatsheet a row. A notice shares that row and is cut short before the tag; a row too narrow for both
drops the tag's hint, then the tag, since a tag may take at most half the row.
**R6.8a** **The Cheatsheet's row order is what a short window shows first.** It scrolls (#90), so no
row is unreachable, but a row below the fold is one nobody scrolls to: a 26-row terminal shows sixteen
of Edit view's rows. The rows on top are **the ones nothing else in Varde teaches**: the palette
gesture, and any `:` command that is in no palette group, has no completion on the `:` line and is named in no notice.
`:format` (R32) was placed twenty-third and was therefore a feature nobody could find. What is
excused off the bottom must be said again where the reader is already looking — the palette's own
letters, or a notice that names the key in the sentence reporting what it answers (`unsaved-changes`
names `:w` and `:q!`, `buffer-diverged` names `D`). Held by a unit test at 26 rows by 120 columns.
**R6.9** **The palette fits the screen it is drawn on.** The box is sized from its row count and is
drawn with no scroll offset, so a list longer than the box can draw loses its tail without a word —
and the mouse hit-tests the same rows, so those entries are unclickable too. Two entries in the
`Panes` group (R33.1, R34.10) were enough to take `(q)` Quit off a 26-row screen, the height the
replay recipe in `AGENTS.md` uses. The list is fitted **in the core**, where drawing and hit-testing
already read one answer, and what a short screen costs is what something else already teaches: the
gaps between the groups first, then the `Esc` line, because Escape closes every box Varde has — and
only then an entry, whose loss the last row says out loud (R31.30's rule, for R31.30's reason). No
entry stops answering the keyboard for being undrawn.
Scenarios: `features/cheatsheet.feature`.

- ✅ Double-tapping Ctrl opens the view palette
- ✅ Two Ctrl presses too far apart are not a double-tap
- ✅ A single Ctrl press does nothing on its own
- ✅ Terminals without modifier key reporting use the fallback binding
- ✅ Choosing Review renders the review view
- ✅ Choosing Story renders the story view
- ✅ Choosing Edit from Review renders the edit view
- ✅ Escape cancels without changing the view
- ✅ An unrecognised key is ignored and the palette stays open
- ✅ Choosing the view already showing re-renders nothing
- ✅ The palette goes to the terminal pane
- ✅ The palette goes to the editor pane from the view it is already showing
- ✅ The editor entry goes to the pane without leaving the view
- ✅ The buffer's own commands are not the palette's

Open:

- ✅ Q36 — **Resolved: `Ctrl+Space` works in every pane Varde interprets**, not only where the
  double-tap is unavailable. One binding that is the same on every terminal, and a way through if
  the double-tap misfires. This was found by running the TUI, where Ctrl+Space did nothing on
  Ghostty. Superseded in one place by R6.1: in a hosted pane it reaches the child, because the
  reserved list is exhaustive and `Ctrl+Space` is not on it — readline and emacs both bind that
  byte. `Esc Esc` is the gesture there.
- ✅ Q37 — **Resolved: a startup probe, not `TERM` and not config.** The edge asks the terminal
  whether it supports the Kitty keyboard enhancement and puts the answer in `State`, so R6.1's Given
  is observable state that a scenario sets. A terminal that does not reply reads as "no", which is
  the safe direction: it arms the `Esc` tap, and arming it where it was not needed costs less than
  leaving a hosted pane with no way out.

## F7 — Review view contents — **DEFINED**

Scenarios live in `features/review_view.feature`.

**R7.1** Review view lists everything **uncommitted against HEAD** — staged and unstaged (Q20).
**R7.2** **Untracked files are included**; git-ignored files never appear (Q21).
**R7.3** A folder that is not a git repository opens the view with an explanatory empty state (Q22).
**R7.4** A clean working tree gets a **distinct** empty state — "no changes" is not the same message
as "not a git repository" (Q23).
**R7.5** *(revised)* The diff is **read-only**; `e` opens the file in Edit view. Reviewing and
editing are different jobs, and a unified diff cannot be edited in place.
**R7.6** *(added with F22)* The cheatsheet drawn in Review view lists **Review view's** keys. It drew
the editor's over a read-only diff, so `dd yy p` were listed against a view that ignores them.
`keys.rs::CHEATSHEET` gains a third field naming the views each row applies to, `UNLISTED` likewise,
and the sweep gains the **reverse assertion** — listed implies answered in at least one view it
claims. That reverse check is what *finds* this defect rather than it being asserted by hand.
**R7.7** *(added)* A diff row's red and green are **muted**, because the row's colour is behind the
code rather than beside it: the 256-cube's own dark red and green (52, 22) are saturated primaries,
and a highlighted token — or, on a side that could not be highlighted, the whole row's text — sitting
on one of them is unreadable. No value is asserted anywhere and none is configurable: colour is a
theme's business (see the rendering table above, and R14.1). The guard is the existing unit test
that the three kinds of row stay **told apart by marker and background alone**, which is the only
claim about diff colour that survives a palette change.

- ✅ Modified, staged and untracked files all appear
- ✅ Git-ignored files are never listed
- ✅ A folder that is not a git repository explains itself
- ✅ A clean working tree is a different empty state

## F8 — Reviewing and submitting — **DEFINED**

Scenarios live in `features/review_submission.feature`.

**R8.1** *(revised with F24)* A comment covers a **line range** and records the **revision
reviewed** — the **blob oid of the content that was on screen** (`Oid::hash_object`), so it moves when
the AI rewrites the file, and a deletion records its HEAD blob oid by the same rule. It is told to the
core as a `revision` field on `Event::ShowDiff`, computed in the `Repository::open` the edge already
does; `state.revisions` is **deleted** in favour of one `diff_revision` beside `diff_file`, because
only the shown diff can be commented on. It is never derived by `tell_core`, which would re-derive
under a comment already made (Q24).
**R8.10** *(added with F24)* A comment also records the **story and step** it was made against, both
absent when the comment was made in Review view.
**R8.2** **ISSUE blocks**; NOTE, SUGGESTION and COMMENT are context (Q26). Any ISSUE makes the
verdict `changes-requested`, otherwise `commented`.
**R8.3** An **empty review cannot be submitted** — refused with a message (Q27).
**R8.4** Submitting writes `.varde/reviews/NNNN.json`, sequentially numbered.
**R8.5** Submitting **sends** the review into the AI pane — inline summary plus the artifact path,
and the prompt is submitted, not left for the user to press enter (Q25).
**R8.6** If no AI session is running, Varde launches `[ai] command` first, then sends.
**R8.7** After submitting, the comments **clear**, ready for the next pass.
**R8.8** Retention: the last **50** reviews, configurable; older ones pruned on submit.
**R8.9** Submitting **confirms first**, because sending clears whatever the AI's CLI is showing —
which can be a half-written message. Declining leaves the prompt untouched and the review unsent.

- ✅ A comment records its file, line range and reviewed revision
- ✅ A comment made after the AI rewrites the file records the newer revision
- ✅ A comment on a deleted file records the revision it was deleted from
- ✅ A review containing an ISSUE requests changes
- ✅ A review without an ISSUE does not block
- ✅ Submitting writes a durable artifact
- ✅ Submitting sends the review into the running AI session
- ✅ Submitting with no AI running launches the configured one first
- ✅ An empty review cannot be submitted
- ✅ Submitting clears the comments for the next pass
- ✅ Only the most recent reviews are kept
- ✅ Submitting warns before it clears the AI prompt
- ✅ Declining leaves the AI prompt untouched and the review unsent
- ✅ The review arrives as one paste, with the prompt cleared first

### Why a file *and* an injection

The artifact keeps the hand-off **CLI-agnostic** — Varde must work with any AI CLI, not just Claude
Code, and there is no cross-vendor protocol for "here is a review". The injection is what makes it
feel integrated: the AI acts on submit rather than discovering the file later. Checked against the
installed Claude Code CLI: `--resume`/`--continue` spawn a *new process* attached to old history;
there is no flag that pushes text into a running interactive session. So "append to the running
session" is a **pty stdin write** — the same primitive as F3's terminal injection — and only
"start a new session" is a process spawn.

**Note the deliberate asymmetry with F3.** Tree actions inject *without* submitting; review submit
injects *and* submits. Different because the user has already expressed intent by hitting submit,
whereas a tree action is the start of a command they still have to finish.

Open:

- ❓ Q28 — Does the AI's response come back into the TUI (e.g. review marked addressed), or does it
  only appear in the AI pane? Currently only the pane.
- ❓ Q40 — What identifies "the reviewed revision" for an untracked file, which has no blob in git?
- ❓ Q41 — Comments clear on submit (R8.7), so a mis-submitted review is unrecoverable from the UI.
  The artifact still has it. Acceptable, or does undo matter?
- ❓ Q42 — Numbering is sequential per project. After pruning, do numbers keep climbing (0051, 0052…)
  or get reused? Scenarios assume they keep climbing.

## F10 — Mouse interaction — **DEFINED**

Scenarios live in `features/mouse.feature`. Mouse was implicit from the start — the original
description said tree action buttons are *clicked* — but never specified.

**R10.1** Clicking a pane focuses it, and the same click **also acts** — no dead first click (Q49).
**R10.2** A single click on a tree row **toggles a folder / opens a file** (Q47). No select-then-open
state, no double-click.
**R10.3** The scroll wheel scrolls the pane **under the pointer**, not the focused pane, and does not
move focus (Q50).
**R10.4** **Right-click does nothing** (Q51). No context menus; every action stays reachable from the
toolbar and the keyboard.
**R10.5** Clicks inside the terminal pane are **forwarded to the running program when it has enabled
mouse reporting** (Q45), **in the encoding that program asked for**, and handled by Varde otherwise.
This is what keeps vim, htop and lazygit usable inside the pane.
**R10.6** Pane dividers are **draggable**, and sizes persist per project in `state.json` (Q48).
**R10.7** Varde implements **its own selection and clipboard copy** (Q46), because enabling mouse
capture disables the terminal emulator's native drag-to-select.
**R10.8** In Review view, **dragging the gutter** selects the line range for a comment, then a picker
takes the type and body (Q52) — this is how R8.1's line-range anchoring is driven by mouse.
**R10.9** A click with the **jump modifier held on a URL in a hosted pane opens it in the browser**,
and reaches the pane's program as no click at all. Only `http` and `https`: a URL is text the child
printed, and `open` on anything else is a way for printed text to launch an application. One row of
the grid at a time — a URL the terminal wrapped is not recognised.
**R10.10** The same click on a **file path in a hosted pane opens it in the editor**, at the line and
column a `:line:column` suffix names, or the line a `#L42` anchor does. A path is recognised by its shape — a `/` or an extension — and
resolved against the workspace root unless it is absolute, since the core cannot see the child's working directory or the
filesystem; a path that turns out not to exist is reported by the edge rather than opened empty.
Holding the jump modifier over either kind of link underlines it.

- ✅ Clicking a pane focuses it
- ✅ A click on an unfocused pane also acts
- ✅ Clicking a folder toggles it
- ✅ Clicking an expanded folder collapses it
- ✅ The scroll wheel scrolls the pane under the pointer
- ✅ Right-clicking does nothing
- ✅ Clicks reach a program in the encoding it asked for
- ✅ The wheel reaches a program in the encoding it asked for
- ✅ Clicks at a shell prompt are handled by Varde
- ✅ Clicking a link in the terminal with the jump modifier opens it in the browser
- ✅ Clicking plain text with the jump modifier opens nothing
- ✅ Clicking a file path in the terminal opens it at the line and column it names
- ✅ A file the AI mentions opens at the line it names
- ✅ Holding the jump modifier over a path in the terminal underlines it
- ✅ Holding it over a link in the AI pane underlines the link
- ✅ Pointing at a path without the modifier underlines nothing
- ✅ Dragging a pane divider resizes the panes
- ✅ Pane sizes are remembered per project
- ✅ Dragging the AI pane's edge resizes it
- ✅ The AI pane's width is remembered per project
- ✅ Dragging selects text and copying puts it on the clipboard
- ✅ The palette entries can be clicked (in `view_mode.feature`)
- ✅ Dragging the gutter selects the lines a comment covers (in `review_submission.feature`)

R10.5 is the hardest rule in the file. It requires the terminal model to expose whether the child has
enabled mouse reporting — `wezterm-term` tracks this as part of its mode state, which is another
reason not to hand-roll the emulator.

Open:

- ✅ Q55 — **Resolved by F15.**
- ✅ Q56 — **Resolved by F13.** The editor is modal and editable; `:w` saves, `:e` reloads.
- ✅ Q57 — **Partly resolved**: `features/editing_vim.feature` adds linewise visual mode, counts, an
  unnamed register, and word motions `w`/`b`/`e` (vim's three character classes, so a motion stops
  where the class changes). Home/End map to `0`/`$`. Still absent: character-wise `v`, named
  registers, macros, `f`/`t`, and search.
- ✅ Q53 — **Resolved by F16.**
- ✅ Q54 — **Resolved by F16**: `arboard`, with OSC 52 as the fallback.
- ❓ Q59 — With `REPORT_ALL_KEYS_AS_ESCAPE_CODES`, some terminals send the **base** key plus Shift, so
  `:` arrives as `;`+Shift and `N` as `n`+Shift. `REPORT_ALTERNATE_KEYS` is now pushed as well and the
  edge normalises the common pairs, but this has not been verified on a terminal that actually uses
  the protocol — only reasoned from crossterm's source. Confirm on Ghostty.
- ❓ Q58 — Drag selection at the edge is one row at a time; dragging across rows takes the row the
  pointer ended on. Multi-row character selection is not specified.

## F11 — Working without a mouse — **DEFINED**

Scenarios live in `features/keyboard.feature`. Added after running the TUI revealed that only the
terminal received keys, whatever had focus.

**R11.1** **Alt + h/j/k/l** moves focus. Chosen because the terminal pane runs a real shell: Tab must
stay with completion and Ctrl+K/Ctrl+L with readline. Alt is one of the few things nothing wants.
Those bindings are **aliases, never the only way**: Option is not Alt on macOS unless the terminal is
configured for it, so the palette reaches every pane too (R6.7). Both routes are named in the
editor's cheatsheet and in the status hint every pane shows — a key nobody can discover is a key
nobody uses.
**R11.2** Keys go to the pane that has focus. A pane with **no key handling swallows them** rather
than letting them fall through to the shell — which is exactly the bug that prompted this feature.
**R11.3** Up/Down move a **tree selection**, clamped at both ends. Enter expands a collapsed folder,
collapses an open one, and opens a file.
**R11.4** **Enter on a file moves focus to the editor** — it is the deliberate "open this". A
**click keeps focus where you clicked** (R10.1: clicking a pane focuses that pane), so the file
opens but the tree keeps the keyboard. Expanding a folder always keeps focus in the tree.
**In Review view both routes move focus to the editor**, click included: the left pane's whole job
is choosing which changed file to read, and everything done with it — `V` to select lines, `c` to
comment, `e` to edit — is the editor's keys. With the keyboard left on the list, `c` was a tree
shortcut and the comment picker could not be reached by keyboard at all. Moving the selection is
still browsing, so it still keeps the list.
**R11.5** Moving the selection onto a file **previews** it: the editor shows it, focus stays in the
tree. Browsing shows you what you are browsing. Preview never replaces a buffer with **unsaved
edits**, and does not apply in Review view, where the left pane lists files whose diffs Enter loads.

## F12 — Reaching the tree actions — **DEFINED**

Scenarios live in `features/tree_action_affordances.feature`.

**The gap this closes:** F3 had 15 green scenarios and no way to invoke any of them. Every scenario
asked "given a tree action, what command is injected?" and none asked "how does a user cause one?"
A fully specified, fully tested, unreachable feature.

**R12.1** Only the **focused row** offers actions. A folder offers new-file, new-directory, go-here,
search-here and delete; a file offers only delete. Every row offers copy-path.
**R12.5** **search-here** opens F21's search box confined to that folder — the same box, with a scope
the query keeps as it is retyped, so the folder is chosen once rather than on every keystroke.
**R12.2** The icons are clickable, right-aligned on the row.
**R12.3** The same actions have shortcuts: `n` new file, `N` new directory, `d` delete, Enter
open/expand — so keyboard navigation is not a dead end.
**R12.4** **Right steps into the focused row's actions**; Left and Right move along them, Enter runs
the one you are on, and Left at the first steps back out. Escape leaves them, and moving to another
row leaves them too. The selected icon is highlighted, so it is clear what Enter will do.

## F13 — Editing — **DEFINED**

Scenarios live in `features/editing.feature`. Answers Q56.

**R13.1** The editor is **modal**, as the product description said: normal mode moves and deletes,
insert mode types, Escape returns. First pass covers `hjkl`, `w`-less motions `0`/`$`/`gg`/`G`,
`i`/`a`/`o`/`O`, `x`, `dd`, `u`. Visual mode, counts, registers and macros are **not** here.
**R13.2** The cursor cannot leave the buffer, in any direction.
**R13.3** `:w` writes the draft to disk and the buffer becomes clean. If the file changed underneath,
**`:w` still writes** — the flag was the warning (R5.3), not a lock.
**R13.4** `:e` discards the draft and takes the disk version, clearing the flag.
**R13.5** **Arrows move in every mode**, unlike `hjkl` which move only in normal mode.
**R13.6** **Backspace** deletes backwards, joining with the line above at the start of a line. In
insert mode a line holding **only whitespace is the start of a line**: the indentation R13.8 lays
down is not something to walk back through one invisible space at a time, so one Backspace takes the
line and lands the cursor at the end of the line above — whether that line above holds a word or
nothing at all. The whitespace is dropped rather than carried up, where it would be a trailing run
nobody can see. The rule is the buffer's, so the review comment box (F17) — the other multi-line
buffer that opens in insert mode — follows it; a query box is one line and has nothing to jump to.
**R13.7** A **half-typed command is shown** — `2d` while you are still deciding — and clears when it
completes or is abandoned. The editor renders it on its bottom edge, with the cursor position.
**R13.8** **The indent width is `editor.tab_width`** — F9's layered, project-scoped key, four spaces
when no layer names one. Tab in insert mode lays that many, as one edit. The block Enter opens
between a bracket pair measures the file's **own shallowest indentation** first and reaches for the
configured width **only when the file holds none to copy**: what the lines in front of you already
do is better evidence about that file than any number is. Two sites, one number — a four hardcoded
at the second is how a project on two got two from Tab and four from Enter, in the same file.

All three came from using it: without a caret, without arrows and without backspace, a modal editor
is unusable no matter how correct its motions are.

## F14 — Syntax highlighting — **DEFINED**

Scenarios live in `features/highlighting.feature`.

**R14.1** Text is classified by **kind** — R14.4 lists the vocabulary. Scenarios assert kinds;
**colour is never asserted**, because it belongs to a theme.
**R14.2** The language comes from the **file extension**. An unknown extension, or no extension at
all, renders as plain text rather than failing.
**R14.3** Tokens **reassemble to the source** — highlighting may not drop or invent characters.

**R14.4** The kind vocabulary is **keyword, operator, string, comment, number, constant, function,
type, property, attribute, punctuation, markup, invalid, plain**. R14.1 is unchanged: scenarios
assert kinds, never colours.
**R14.5** The mapping from **scope family to kind is total**. Sublime scope names are a shared
convention, not per-grammar invention, and the convention has **fifteen** top-level families —
`comment`, `constant`, `embedded`, `entity`, `invalid`, `keyword`, `markup`, `meta`, `punctuation`,
`source`, `storage`, `string`, `support`, `text`, `variable`. Every one maps to a kind, so a language
nobody anticipated is coloured on arrival rather than after a bug report. A unit test beside
`classify` lists the fifteen and asserts each maps, which makes "no family is unhandled" a build
result rather than an intention, and goes red if a future syntax set introduces a sixteenth.

The second segment is read only where a family is genuinely two things. Those exceptions are the
whole of the per-grammar variation, which is why the mapping is a table and not a one-liner:

| family | second segment | kind |
|---|---|---|
| `keyword` | `operator` | operator |
| `keyword` | anything else (`control`, …) | keyword |
| `storage` | — | keyword — Rust spells `let` and `struct` as `storage.type`, Java spells `public` as `storage.modifier` |
| `constant` | `numeric` | number |
| `constant` | anything else (`language`, `character`, `other`) | constant |
| `entity` | `name.function` | function |
| `entity` | `name.type`, `name.class`, `name.struct` | type |
| `entity` | `name.tag`, `name.table`, `name.section` | markup |
| `entity` | `other.attribute-name` | attribute |
| `entity` | anything else | type |
| `support` | `function` | function |
| `support` | `type`, `class` | type |
| `support` | anything else | constant |
| `variable` | `annotation` | attribute |
| `variable` | `parameter`, `other.property`, `object.property` | property |
| `variable` | anything else | plain |
| `comment`, `string`, `punctuation`, `invalid`, `markup` | — | direct |
| `meta`, `source`, `text`, `embedded` | — | plain |

`meta` stays plain deliberately. It is a *structural* scope covering whole regions —
`meta.block`, `meta.function.parameters` — so colouring it would colour half the file.

Three of these rows are measured against the real grammars rather than assumed. A **macro,
annotation or decorator** — `#[derive]`, `@Override`, `@dec` — is `variable.annotation`, not
`entity.other.attribute-name`, which is why `variable` needs an `annotation` arm to make attributes
work at all. A **mapping key** in TOML or YAML is `entity.name.tag`, the same scope HTML uses for a
tag name, so a key takes the markup kind and not the property kind: one scope cannot be two kinds
without branching on the language, which is forbidden. Object properties in JavaScript and
TypeScript are `variable.other.property` / `variable.object.property` and *are* the property kind.

**R14.6** A **delimiter belongs to what it delimits**: punctuation is the **weakest** kind, taken
only when no other family in the scope stack claims the text. Grammars spell a string's quotes as
`punctuation.definition.string.begin` *inside* the `string` scope and a comment's `//` as
`punctuation.definition.comment` inside the `comment` scope, so a punctuation arm in an
innermost-first walk without this rule would split `"world"` into three tokens and turn `//` into
punctuation — breaking R14.1's existing scenarios. The same rule keeps a Markdown `**bold**` whole.

**R14.7** The syntax set is **every language syntect ships in `two-face`'s extended set** — bat's,
around 220 syntaxes — not Sublime's 75 defaults. TypeScript, TSX, JSX, Vue, Svelte, Kotlin, Swift,
Zig, Dart, TOML, Terraform, Nix, Elixir, protobuf, Dockerfile, GraphQL and SCSS are reached by that
one choice rather than by an arm per language, and Varde's own `config.toml` and `Cargo.toml` are
coloured. R14.2 is unchanged: an extension genuinely unknown to the set still renders plain.
Behaviour covers one language per newly reached family, not one per language — enumerating 220
through scenarios is the coverage-chasing the test strategy forbids.

A fenced code block in Markdown preview reaches the highlighter **by language name** rather than by
file name (`highlight("typescript", …)`), so it gains R14.7's coverage on the same commit; a scenario
pins that the two panes agree.

Colour is chosen in `src/ui.rs` from `editor.theme`, which flows through F9's config layering.

## F21 — Content search — **DEFINED** (phase 1)

Scenarios live in `features/search.feature`.

**R21.1** A **full-screen modal**, absent until summoned: `Ctrl+F`, palette `f`, `*` for the word
under the cursor, `gr` for the mouse selection (falling back to the word under the cursor). It is a
task you enter and leave, not a fifth pane to live with.
**R21.2** Hits are **grouped by file**, all files at once, with line numbers and the matching line,
read top to bottom. Files are scanned **alphabetically** so a query gives the same answer every run.
**R21.3** Matching is **literal, case-insensitive unless the query contains a capital** — ripgrep's
smart case. Literal so `foo(` needs no escaping, which matters when the query arrives from a
selection.
**R21.4** A file that is **open is searched as it stands on screen**, not as it sits on disk. Whoever
owns the file supplies its contents — the same rule as writing.
**R21.5** Capped at **500 hits**, and the count says so rather than showing a silent subset.
**R21.6** `↵` opens the selected hit **at its line**; `Ctrl+O` opens every file with a hit. Every
printable key belongs to the query, which is why the actions take a modifier.
**R21.7** `Tab` completes to the **commonest word in the hits** that starts with the query — always a
word that exists in the project. Ties go to whichever matches the case you typed.
**R21.8** Searching is **debounced** at the edge: the query is held until typing settles, so a repo
is read once per word rather than once per keystroke.
**R21.9** The list **scrolls to keep the selected hit on screen**, with the row above it — the
heading of the file it is the first hit of — kept in view too, so a step lands somewhere that says
which file it is. A hit's row is its index *plus the file headings above it*: counting only the hits
is what let the selection walk off the bottom of the box while the view stood still. The box covers
every pane, so **while it is up the mouse is the box's**: the wheel scrolls the list wherever the
pointer is, and a press is swallowed rather than delivered to a pane nobody can see.
**R21.10** `Ctrl+N` and `Ctrl+P` step **file to file** — the first hit of the next file, or of the
one it is in when the selection is not on its first hit already. One file's hits can fill the box,
so stepping hit by hit is not a way through a long result list; the arrows keep that. Modifier-only
against R31.11, and the reason is recorded beside the binding: inside this box every printable key
is a letter of the query, and a Ctrl'd letter is one ASCII byte every terminal sends unaided rather
than the Option-and-Kitty-reports hazard R31.11 is about.
**R21.11** The box **names its own keys, inside itself** — a dim row under the count, drawn from the
one list that lives beside the router answering them. Inside the box and not in its bottom border:
the keys worth naming here are the ones that get somebody through a long list, and a right-aligned
caption under a scrolling list is not where anyone looks for them. `Ctrl+N`/`Ctrl+P` was bound and
unnamed for exactly as long as it took to ask for it back. The arrows are named too — the only list
in Varde that spends a label on them — because they and `Ctrl+N`/`Ctrl+P` move by different things,
and that difference is the thing to be readable.
**R21.12** A **click selects the hit under the pointer**, which is the other half of R21.9's wheel: a
list scrolled to a hit no key sequence reached comfortably is a hit that has to be markable where it
now sits. Selecting is all it does — opening stays R21.6's `↵` — so a stray click never opens a
file. A click on a file heading, on the query, or on the box's chrome selects **nothing** rather
than the nearest hit: a guessed target is a different file opened by the `↵` that follows.

Phase 2 — replace across files — is deliberately not here. Q15 already settles where its writes go:
the draft if the file is open and dirty, disk otherwise. Its safety questions (undo, whole-word,
preview) get their own pass.

## F20 — Filtering the file tree## F20 — Filtering the file tree — **DEFINED**

Scenarios live in `features/filter.feature`. `/` opens the box; Esc closes it, Enter opens the best
match.

**R20.1** Matching is **fuzzy and ranked** — `ftr` finds `file_tree.rs`, closest first.
**R20.2** **Only files match.** Folders appear when they lead to a match, and are opened, because a
match you cannot see has not helped you.
**R20.3** The box **completes to the best match** ahead of the cursor, narrowing as you type.
**R20.4** An empty filter shows the tree as it was; a filter with no matches says **"no files
match"** rather than showing an empty pane.
**R20.6** The box is **always visible** on the tree's bottom edge, showing a dim "filter files" hint
when idle. `/` puts the keyboard in it; Esc leaves and clears.
**R20.5** The filter matches **the whole project**, not just expanded folders — the tree stays lazy
(R2.5), but the filter walks once on first use and caches. Without this it could only find files you
had already navigated to, which is the opposite of the point.

The walk is git-ignore aware and **skips `.git`**: dotfiles are part of the project (R2.2), but a
git database's objects are noise, not results.

Ranking scores the **filename**, falling back to the whole path. Scoring the path alone lets an
early letter in a directory hijack the match — `tree` latching onto the `t` in `features`.

## F19 — Several files open at once — **DEFINED**

Scenarios live in `features/buffers.feature`. Asked for explicitly without a tab bar.

**R19.1** Opening a file **keeps what is already open**. Unsaved edits survive, which is why the
single-buffer dirty guards on opening could go.
**R19.2** What is open shows as a **mark in the file tree** and as a **strip of dots on the editor's
bottom edge**. No tab bar. The mark is a rule, not a colour choice — `varde::mark` returns
`Current`, `CurrentDirty`, `Dirty`, `Open` or `None`, and scenarios assert on that. Rendering:
**filled** when it holds something you care about (the one you are in, or unsaved work), **hollow**
otherwise; **yellow** for unsaved, cyan for current, dim for the rest.
**R19.3** **`gt` / `gT`** step through buffers, wrapping, and need no modifier — on macOS, Option is
not Alt unless the terminal is configured for it (Ghostty: `macos-option-as-alt = true`), so this is
the only route: Alt+Left / Alt+Right move by word in the editor instead. The dots are clickable.
**R19.4** Switching **selects the same file in the tree**, so the two views never disagree.
**R19.5** Browsing does not fill the strip: moving the tree selection opens a **preview**, which the
next preview replaces. Enter or a click makes it stay.
**R19.6** Selecting a file that is **already open switches to it** — it is not re-read, and its
unsaved edits are untouched.
**R19.7** Closing moves to a neighbouring buffer; only closing the last one empties the editor.

## F18 — The AI pane — **DEFINED**

Scenarios live in `features/ai_pane.feature`.

**R18.1** With **no session running, the AI pane is an input box** asking which CLI to start,
prefilled with the remembered choice — claude, opencode, vibe, anything. Enter accepts, typing
replaces. The palette's `a` and `Alt+l` simply go to the pane; the box is already there.
`:ai <command>` does the same from the command line.
**R18.2** Starting **without** naming a CLI while one runs just focuses it. Starting a **different**
one is **refused** (Q from review): switching CLIs is deliberate, so it needs `:ai! <command>`,
which stops the running session first — the same idiom as `:q!`.
**R18.3** Once a session runs, ordinary typing goes to it — F11's rule, nothing special. While none
runs, the pane's keys belong to the box and **never reach the shell**.
**R18.4** The chosen CLI is **remembered in the project's `state.json`**, so reopening offers what
you used here last time. Config is the default when there is no history.
**R18.5** A submitted review goes to **whichever session is running**, and starts the remembered CLI
when none is.
**R18.6** When the CLI **exits, the pane goes back to asking**, prefilled and ready to start another.
The dead pty is dropped rather than left on screen. A CLI that **cannot be started** leaves the pane
asking in exactly the same way, prefilled with what failed. Whether a session exists is a fact the
edge supplies rather than one the core remembers having asked for — which is what stops a pane with
no child from taking keys nothing receives, and `:ai` from refusing because a session is "already
running". A review queued for a session that went away, exited or never started, is dropped rather
than handed to the next one.

**R18.7** The palette's `l` **swaps the pane between beside the editor and the whole right-hand
edge**, where the terminal ends at its border. An AI CLI is a long conversation and the default
two-thirds-height column is the wrong shape for reading one, so the terminal gives up width rather
than the AI pane giving up rows. `:tall` is the same thing from the command line, but it cannot be
the only way in: the shape is wanted *while reading the AI pane*, and a hosted pane's child owns the
colon — so changing it would have meant leaving the pane first. The palette is the one gesture that
reaches every pane (F6), and choosing the entry **leaves focus where it was**. The shape is
**remembered in the project's `state.json`** beside the widths, and a width its edge was dragged to
is the width in either shape.

**R18.8** `:inject` puts **what you are looking at into the session's prompt without submitting
it** — the selection from whichever pane holds one, because the terminal's error message is as worth
asking about as the editor's line, and the cursor's line when nothing is selected. The Enter is
left to you, so the question can be typed in front of the code. With no session running it starts
one and the text waits for it (R18.5's machinery). Focus moves to the pane, since typing the
question is the next thing you do.

Not submitting is the promise, so an **embedded newline is an implicit Enter and is refused out
loud** rather than risked: trailing newlines are cut, since a linewise selection carries one, and
**lines are declined** — `Enter::Refused`, named for the promise — when the child never asked to be
told a paste from typing, because bracketing is the only thing that stops a newline mid-text from
being read as a submit. Same reasoning as a mouse coordinate the legacy encoding cannot name. A
Pause snapshot (R23) still goes bare: it is `Enter::Withheld`, which asks only that *we* do not
press Enter.

The action also sits as an **icon on the AI pane's own border**, drawn in **both** of the pane's
states — the start box as well as a running session — because the hit-test cannot be reachable on
columns nothing is drawn on, and because `:inject` with nothing running starts a session, so the
button must too. A click on the AI pane's **title row is the pane's**, never reported through to its
child: without that, the icon would both inject and send a phantom click to a CLI that enabled
mouse tracking.

Submitting a review also starts a session if none is running (R8.6), but that cannot be the only
way in. Found by being asked how to start Claude in the AI pane, and having no answer.

## F15 — Quitting — **DEFINED**

Scenarios live in `features/quitting.feature`. Answers Q55.

**R15.1** `:q` **closes the open file**, as it closes a window in vim. Closing really is discarding,
which is why it **refuses while the buffer in front of you has unsaved edits** — and never because a
*different* buffer does, which had made one unsaved file enough to lock every clean buffer open.
`:q!` discards them. `:wq` writes then closes.
**R15.2** Leaving Varde is **not on the `:` line at all**: Ctrl+Q and the palette's `q` are the
gestures for it — unambiguous, wanted from anywhere, and so no confirmation is needed beyond the
dirty check. Keeping it off `:` is what stops a mistyped clear-up from taking the session with it,
which is the whole reason `:qa` is the clear-up and not vim's quit-all. Ctrl+Q is Varde's in the panes Varde interprets only; in a hosted pane it reaches
the child, like every key not on the reserved list. Same for R21.1's `Ctrl+F`. The palette is the
gesture that works from everywhere.
**R15.3** Per-project state is written on the way out, forced or not.
**R15.4** `:qa` **closes every buffer with nothing unsaved**, and `:qa!` takes the dirty ones with
it. Bare, it refuses nothing: a dirty buffer is one it skips, not a reason to refuse the rest. It is
the `:` line's alone — the palette keeps quitting, the gesture wanted from anywhere, and a clear-up
is a buffer command like `:q`. It **says which buffers it
kept** and names them, since buffers still open after a clear-up is otherwise a fact you discover by
counting dots. Afterwards the current buffer is one that still exists, or there is none and focus
moves to the tree — the arm R15.1 already has for its last close.

## F16 — Selecting and copying — **DEFINED**

Scenarios live in `features/selection.feature`, and pasting's in `features/pasting.feature`.
Answers Q53 and Q54.

**R16.1** Each pane selects what it holds: characters in the editor, scrollback in the terminal, and
in the tree a drag moves the **row selection** — a filename is not text you copy character by
character.
**R16.2** Copying uses the system clipboard, falling back to **OSC 52** so it still reaches the
machine the user is sitting at when Varde runs over SSH.
**R16.3** Copying with nothing selected does nothing.
**R16.4** **What was copied pastes back as it was**, into the buffer it came from or another one:
a paste that reaches the open buffer is **one edit, in whatever mode the buffer is in**, and its
line endings are line breaks however the source spelled them. Insert mode used to be the condition,
which left a paste in normal mode replayed as keystrokes — read as commands until an `i`, `a` or `o`
turned the rest into typing, one auto-indenting Enter per line, so three indented lines came back as
a staircase. A read-only surface still takes none of it: a diff, a walkthrough and a Preview answer
the editor's keys ahead of the buffer.
**R16.5** **Ctrl and Command are aliases** on copy and on paste. Command+V pasted long before
Ctrl+V did, because the host terminal answers it and sends the text on as a paste — which made one
gesture look like two unrelated features.
**R16.6** Pasting puts the clipboard into the buffer as **one edit**, wherever a typed character
would reach it: a preview refuses out loud, and a diff or a walked Site — which answer the editor's
keys ahead of the buffer — paste nothing rather than editing a file nobody is looking at. `y` and
`p` keep the **register**, which stays a separate world (R16.2's clipboard is not it).
**R16.7** **Double-clicking picks the word** under the pointer, as it does in every other editor —
the run of one character class, so an operator is picked like a name and whitespace is no word at
all. The second click counts as one only inside F6's double-tap window and on the same cell.

## F17 — Reaching the review flow — **DEFINED**

Scenarios live in `features/reviewing.feature`. F8 had ten green scenarios and, like F3 before F12,
**no way to invoke any of them** — the same reachability gap, found the same way: by running it.

**R17.1** The changed-file list occupies the tree pane, so F11's keys apply unchanged.
**R17.2** Enter shows a **read-only unified diff** of the selected file, and **moving onto a file
shows its diff** too — reviewing is reading, so show what you land on. Clicking a row does the same.
**R17.3** Comments anchor to **new-file line numbers** — the code the AI has to change. A comment on
a removed line points at where it was.
**R17.4** `V` selects lines in the diff, `c` opens the type picker, Escape records nothing.
**R17.5** `:submit` sends the review; `e` leaves the diff for the real file in Edit view.
**R17.6** Entering Review view **lands on the first changed file** — selected, diff loading, keyboard
on the list. You switched here to look at changes, so it shows one.
**R17.7** **Leaving Review view clears the diff.** Otherwise the editor keeps showing a read-only
diff of the file you are trying to edit.

Every route into a view goes through one `switch_view`, so the palette, a click and a direct open
all do the same work. Two of them did not, and the palette route silently skipped loading the diff.

## F9 — Configuration and state — **DEFINED**

Scenarios live in `features/configuration.feature`.

**R9.1** Global config lives in `~/.varde/config.toml`, created at install.
**R9.2** Project config and state live in `<project>/.varde/`, created on first start there. **A Bare
workspace has neither**: its state lives in the Sidecar, and it has **no project configuration layer
at all** — built-in defaults and `~/.varde/config.toml` and nothing else — so a `.varde/config.toml`
that happens to sit in the folder is not read, and nothing is seeded anywhere. A key written into a
directory deleted at exit is worse than a key never written, which is R9.7's argument read the other
way (`features/bare_workspace.feature`).
**R9.3** Config is **TOML**; per-user state is `state.json` (machine-written, so JSON) (Q33, Q1/Q30).
**R9.4** The effective config is a **deep merge**, project winning key by key; unset keys fall through
to global, then to built-in defaults (Q29).
**R9.5** A config file that does not parse **stops Varde from starting**, with an error naming the
file and the line (Q32). **The error says which of three faults it was**, because they send the reader
to three different places: TOML that does not parse, a value of the wrong type, and an entry no layer
ever completed. The words live in the error, not at the edge — one sentence for all three is how a
deserialize fault about a missing key came to wear the parse fault's words and send whoever read it
hunting a syntax error that was not there.
**R9.6** Varde **never writes git ignore rules** and never edits the project's `.gitignore` (Q1).
**R9.7** Starting **seeds `<project>/.varde/config.toml`** when nothing is there, and **leaves any
file that is there alone** — a reader's own settings survive every start (Q38). **Every key in the
seeded file is commented out**, table headers apart: a seeded file holding live values would make
"the project sets nothing" false on a project's first run, and would freeze one binary's numbers
into a file that outlives it, so a later correction to the shipped defaults would arrive and change
nothing. The headers are live because uncommenting `tab_width` under a commented `[editor]` sets a
top-level key nothing reads, and an empty table merges nothing. **The core decides whether to seed**:
the layer the edge read off `.varde/config.toml` is already an input to starting, and it is absent on
exactly the folders with no file to lose, so the promise is a plain `Effect::WriteFile` under a
condition a scenario can reach rather than an `if` in `main.rs` no scenario covers. **Absent means
absent**: the edge hands an *empty* layer for a file it found and could not read — not UTF-8, or
write-only — because reading every failure as "nothing is there" would seed over settings Varde
could not parse, which is a delete rather than the one-session fallback it looks like. An empty
layer merges nothing, so the effective config is the same either way and the file survives to be
fixed in another editor. What the file names is every scalar the shipped defaults spell —
`view.double_tap_ms`, `editor.tab_width`, `risk.threshold`, `risk.max_iterations` — and a unit test
walks that **both ways**: it uncomments all of them and holds each against that layer, because a
stale quoted number reads as advice and pins the answer the reader was trying to accept, and it
holds every scalar the defaults spell to appearing here, because a tunable the defaults grow is
otherwise findable nowhere while the suite stays green — the exact state `editor.tab_width` was in.
A setting whose default lives in Rust alone has no text to be held level with, and is left out for
that reason; the `[lsp.*]` and `[facts.*]` tables are data a reader reaches for a language rather
than numbers to tune, and are left out for theirs.

- ✅ Starting in a project for the first time creates its config folder
- ✅ Starting in a project for the first time seeds a config file
- ✅ Starting in a project that already has a config file leaves it alone
- ✅ A seeded config file changes nothing about the effective settings
- ✅ Starting again keeps the state already recorded
- ✅ Starting never touches the project's git files
- ✅ A project setting overrides only the key it names
- ✅ Global settings apply when the project sets nothing
- ✅ A setting neither config names falls back to its default
- ✅ A malformed project config stops Varde from starting
- ✅ A malformed global config stops Varde from starting

Accepted trade-off on R9.5: a broken `~/.varde/config.toml` locks the user out of Varde until they
fix it in another editor. Chosen deliberately over defaults-with-a-warning. This is why the error
must name file *and* line — that precision is the escape hatch.

Open:

- ❓ Q31 — Behaviour when `~/.varde/` is missing entirely (user never ran the installer). Create it on
  demand, or treat it as a broken install?
- ✅ Q38 — Does creating `<project>/.varde/` also seed a `config.toml`? **Yes, with every key
  commented out** (R9.7). Discoverability won: `editor.tab_width` was layered, merged and read on
  every start for its whole life while no `.varde/config.toml` existed on any disk to name it, so
  the key was configurable and unfindable at the same time. Commenting the keys out is what keeps
  the other half of the question answered too — "the project sets nothing" stays honest, because a
  file of comments contributes nothing to the merge.
- ❓ Q39 — What exactly does `state.json` hold? Working assumption: last view, open buffers, cursor
  positions, pane sizes. Needs pinning before F5 (auto-update) and F6 interact with it.

---

## F22 — Authoring a story set — **DEFINED**

Scenarios live in `features/story_authoring.feature`.

**R22.1** A Story is authored by the **hosted AI writing an artifact**, not by Varde parsing what the
CLI printed. `:story` pastes a prompt; the existing file watcher picks up `.varde/stories/…json`
(Q55, `docs/adr/0006-stories-arrive-as-an-artifact.md`).
**R22.2** A bare `:story` means **uncommitted-vs-HEAD when the tree is dirty**; otherwise
`origin/HEAD` → the upstream branch → `init.defaultBranch` → a probe for `main`, then `master` (Q56).
**R22.3** If none of those resolve, authoring is **refused with a notice**. A wrong guess costs the
reviewer five to eleven minutes on the wrong commits.
**R22.4** The resolved range is **always confirmed** before authoring, through a `ConfirmStory` modal
beside `ConfirmSubmit`. Authoring clears whatever the CLI is showing, exactly as submitting does.
**R22.5** A bad explicit range produces a **notice slug, never an empty list** — the edge's habit of
swallowing git errors into `None` is defensible for a status poll and wrong for an explicit command.
**R22.6** `:story` on an already-authored range **loads it**; `:story!` **re-authors** (Q57). Loading
is instant and offline; re-authoring costs minutes, clears the CLI's prompt and discards the
Walkthrough, so it takes the bang.
**R22.7** While authoring, the spine reads `authoring`, with **no timeout**. A guessed timeout is
wrong on a slow run and slow on a fast one — the same reasoning that made `Event::AiSpoke` replace a
fixed delay. A CLI that dies is detected by `AiExited`, which every site that stops holding a pane
already queues.
**R22.8** A malformed artifact is **refused whole**, never salvaged in part (Q58).
**R22.9** Artifacts are named `.varde/stories/<base12>-<head12>.json`, with `-worktree` as the head
for a dirty range. Retention is **ten** — stories are 30–70KB against a review's 1KB.
**R22.10** A Story **dies with its range** and is never updated to follow the code
(`docs/adr/0005-a-story-dies-with-its-range.md`).
**R22.11** *(added with the faster-authoring spec)* A Step names its Site's **file, side, kind and
line range, and nothing else** — the copy of the code it used to carry was 23–35% of every measured
artifact and pure transcription. Varde reads the text itself, **once, when the Story arrives**: the
core parses, holds the artifact, and asks the edge for each Site's current text; the edge answers and
only then is the set walkable. Filling on every load would compare each file against itself and make
the stale check vacuous. Old-side Sites read from the Story's recorded base, new-side from the working
tree — what staleness already compares against. The field stays **optional rather than gone**, and a
set that transcribed its own text keeps it, so the sets already in `.varde/stories/` still load.

- ✅ A bare story command on a dirty tree offers uncommitted against HEAD
- ✅ A bare story command on a clean tree uses the default branch
- ✅ The default branch is resolved in a fixed order, offline
- ✅ Nothing resolves, so authoring is refused rather than guessed
- ✅ An explicit range git cannot resolve is a notice, never an empty list
- ✅ Confirming sends the authoring prompt to the running AI session
- ✅ Confirming with no AI session launches the configured one first
- ✅ Declining leaves the AI prompt untouched and nothing authored
- ✅ The spine says it is authoring while it waits
- ✅ The artifact arrives and its stories are listed
- ✅ The AI exits before the artifact arrives
- ✅ Cancelling authoring leaves the working tree unchanged
- ✅ A malformed artifact is refused whole, never salvaged in part
- ✅ An artifact whose range no longer resolves is not walked
- ✅ A story set is named for the revisions it describes
- ✅ Only the ten most recent story sets are kept
- ✅ A range already authored loads without touching the AI
- ✅ Re-authoring takes a bang, and confirms first
- ✅ A step that writes no code text is filled in from the file itself
- ✅ A story set that carries its own code text keeps it, so the sets on disk still load
- ✅ An old-side step is filled from the base, a new-side one from the working tree
- ✅ A step whose code moves after the story arrived still says what its site used to hold
- ✅ Re-reading a story set does not refill it against the code as it reads now
- ✅ A story set is not walkable until Varde has read what its sites hold

**Q55 — Artifact, or parse the CLI's output? Resolved: artifact.** Rejected: reading the pane's text,
which would need a branch per provider and is forbidden by ADR 0004; and a structured protocol, which
does not exist across vendors. Measured: three fresh sessions given only the 5.7KB prompt produced
**83/83 byte-exact sites and 20/20 verified citations**, because all three *scripted* the extraction —
a CLI has tools, so the self-check is a program rather than a plea, and the prompt said so. R22.11
has since taken the extraction off the AI entirely: the measurement stands as the reason the artifact
channel works, not as a description of what the prompt still asks for.

**Q56 — How is a bare range resolved offline? Resolved: the four-step ladder above.** Rejected:
assuming `main` (wrong on any repo that renamed it), asking the forge (out of scope, needs network),
and asking the user every time (five keystrokes on the common path).

**Q57 — Does `:story` on an authored range load or re-author? Resolved: load; `:story!` re-authors.**
Rejected: always re-authoring, which spends eleven minutes and the CLI's prompt to regenerate
something already on disk.

**Q58 — Malformed artifact: refuse or salvage? Resolved: refuse whole.** Rejected: listing the stories
that parsed, because the reviewer cannot tell an omitted story from one the AI never wrote — the
silent degradation this whole feature is a reaction to.

## F23 — The spine and what a change's stories cover — **DEFINED**

Scenarios live in `features/story_spine.feature`.

**R23.1** The spine lives in the **tree pane**; `t` toggles it against the changed-files list. Review
view is untouched — the list is joined, never replaced.
**R23.2** **Coverage is never a ratio** — a count of unclaimed hunks and a list of where they are
(Q59).
**R23.3** The unit is the **hunk as Varde computed it**, with Varde's own pinned diff options, so both
sides of the subtraction agree and hunk-index instability never arises.
**R23.4** A Site claims a hunk by **overlap**, not containment; a `context`-kind Site claims nothing.
Containment would make any hunk taller than a pane permanently unclaimable.
**R23.5** Overlapping claims **count once**, and there is **no partition requirement** (Q60).
**R23.6** The Remainder is **recomputed, never frozen** — for an uncommitted range the AI keeps
writing, and a frozen Remainder reads "nothing left" while three new files appear.
**R23.7** The Remainder is a **spine entry, not a Story**. No premise, no claim, nothing authored.
**R23.8** **Deletions get their own line, present only when the range deletes something**, so "this
change deleted nothing" is distinguishable from "twelve lines went and nobody walked them".
**R23.9** The spine has **no filter box**. Filtering seven stories solves a problem nobody has, and
the row it costs is a row of story.
**R23.10** Staleness is **per step**, with a count on the story line, so a typo fix in one file does
not discredit a whole story while the reviewer still learns before choosing one.

- ✅ The tree pane toggles between the changed files and the spine
- ✅ The spine lists each story with its step count
- ✅ A story whose steps no longer match carries a stale count
- ✅ The remainder counts the hunks no step claims
- ✅ A site claims a hunk by overlapping it, not by containing it
- ✅ A context site claims nothing
- ✅ Two stories claiming the same hunk have it counted once
- ✅ The remainder recomputes when the change grows under it
- ✅ A range that deletes something says how much of it was walked
- ✅ A range that deletes nothing has no deletions line at all
- ✅ The spine has no filter box

**Q59 — What is the coverage denominator? Resolved: there isn't one.** Measured across 30 commits and
14,576 changed lines: `src/` 48%, `.scratch/` tickets **22%**, `features/` 13%, `tests/` 8%, docs 7%.
No split is defensible — excluding docs would have excluded the best step in the prototype. Rejected:
percent-of-changed-lines (35% reads as failure and teaches the reviewer to ignore the line, while
"12 changes no story reached" reads as twelve things to look at), percent-of-files (one Site in
`lib.rs` would claim all 2,435 of its lines), and an author-declared coverage field (the prototype's
was plausible, invented and wrong).

**Q60 — Must a story set partition the hunks? Resolved: no, and the requirement is deleted.** Three
real authoring runs covered **81% / 45% / 35%** — gaps are every run, not a failure mode, and the
Remainder is the answer to them. Overlaps are allowed and claim once, because coverage is a set and
not a sum, so forbidding them would make authoring harder for no reviewer benefit. Rejected: refusing
a set with overlaps, and reporting a gap as an error.

**Scenarios that must arrive through a diff, never a field.** Coverage and staleness are exactly the
shape the hollow-assertion sweep found: a `Given` that writes what the `Then` reads. So a scenario
supplies **the two sides of a file as DocStrings** and Varde hunks them itself, via a pure `src/`
function over `git2::Patch::from_buffers` — no repository, no filesystem, bytes in and hunks out.
Rejected: a hunk table in the Given (the arithmetic would be over a fiction and hunk boundaries would
never be tested) and a fixture repository (banned outright).

## F24 — Walking a story — **DEFINED**

Scenarios live in `features/walking_a_story.feature`.

**R24.1** Focus while walking is the **editor**; the cursor goes on the Site's first line and the
**scroll is framed on arrival**, so the whole Site opens downward into the pane. A Site is a range
and the ordinary clamp is about a cursor: left to it, a downward jump lands on the bottom row, which
is the first line of a claim with the rest of it below the fold. The cursor is not moved to the
Site's end instead — that mirrors the bug for a Site taller than the pane and moves where `V` and `c`
begin. Framing counts **rows**, never lines (`story::row_of`): a comment row sits between two lines.
A Site shorter than the pane keeps a little context above it; a taller one is framed from its top.
One mechanism, one focus.
**R24.2** **`n`/`p` step; `j`/`k` scroll**, meaning what they mean in every other view (Q61).
**R24.3** The narration band is a **fifth `Area` on `Layout`, never a fifth `Pane`** — it holds no
focus, but its values row is clickable, so drawing and hit-testing must read one rectangle. Height is
**fixed at 6 rows**; a values row takes a content row rather than adding one, so the code never moves
under a keypress.
**R24.4** The band carries the **claim** and, when present, the **cited values**. Why, flow and nudge
are one keypress away in the `d` overlay: a step's prose is ~16 wrapped lines against an editor pane
of 37 columns by 19 rows, so a step does not fit and the design is what pays.
**R24.5** A value with nowhere to point is **displayed as invented**. Varde cites and jumps (`g`); it
verifies nothing and needs no per-language parser.
**R24.6** Staleness has **three distinct members** — `FileMissing`, `RangeOutOfBounds`, `TextChanged`
— because they want different words and only the last can show what the Site used to hold.
**R24.7** The check runs in `update` against the buffer the step already opened, re-running when
`Buffer::revision` moves. The hash covers **the range only**, over **whitespace-trimmed** lines: one
`cargo fmt` marking every Story stale would teach the reviewer to ignore the mark.
**R24.8** A stale step **keeps everything except the claim to describe the screen**. It still
narrates, still opens, still takes a comment — a stale step is often exactly where a comment belongs.
**R24.9** An **`old`-side Site is immune**, except where an uncommitted range is committed and the
base moves under it.
**R24.10** A comment **records its story and step** alongside file, line, type and revision, so a
submitted review says which *claim* the reviewer rejected.
**R24.11** A Walkthrough is **not remembered**: entering a Story starts at its first Step every time,
and nothing survives a restart (Q62).

- ✅ Entering a story puts the cursor on the first step's site
- ✅ Stepping forward moves the cursor and the claim together
- ✅ Stepping back returns to the previous step
- ✅ Stepping past the last step does not leave the story
- ✅ j scrolls the code, it does not step
- ✅ Escape leaves the story for the spine
- ✅ e opens the step's file in Edit view
- ✅ A step's cited values appear in the band
- ✅ A step with no values shows no values row
- ✅ A value with nowhere to point is shown as invented
- ✅ g jumps to a cited value's source
- ✅ d opens the step's detail
- ✅ The detail omits the nudge section when the step has none
- ✅ A site that no longer holds its text marks the step stale
- ✅ Editing the buffer under a step makes it stale
- ✅ Whitespace-only reformatting does not mark a step stale
- ✅ A stale step warns before it is read
- ✅ A stale step's detail shows what the site used to hold
- ✅ A stale step can still be commented on
- ✅ An old-side site is immune to the working tree moving
- ✅ Committing an uncommitted range moves the base under an old-side site
- ✅ A comment records the story and step it was made against
- ✅ Walking the remainder steps through bare locations
- ✅ Entering a story shows the whole of its first site
- ✅ The frame leaves a little context above the site
- ✅ A site taller than the pane is framed from its top
- ✅ A site at the top of the file does not scroll above the first row
- ✅ Stepping on frames the step it arrives at
- ✅ Stepping back frames the site the same way stepping on does
- ✅ A comment above the site is counted in the frame
- ✅ Walking the remainder frames its hunk the same way
- ✅ A jump to a citation is not an arrival, even landing on the site's line number

**Q61 — Does `j` step or scroll? Resolved: it scrolls; `n`/`p` step.** Every tool in the prior art
binds next-step to `j`, but a Site runs up to 34 rows against a 19-row pane, so a reviewer who cannot
read around it cannot review it. Rejected: `j` steps and `Ctrl+d` scrolls — a key that changes meaning
per view is precisely the difference the cheatsheet sweep exists to close, and F22 has just given the
cheatsheet a view dimension.

**Q62 — Is a Walkthrough remembered when a Story is left, or across a restart? Resolved: no.** A
Walkthrough is disposable (`CONTEXT.md`), and stepping a Story again from the top costs a few keys.
Rejected: remembering the position per Story, persisted, and discarded with a notice when the Story is
re-authored — specified and never missed in daily use, so it would be storage and a staleness rule
paying no rent (issue #7, `.out-of-scope/walkthrough-memory.md`).

## F25 — Predictions — **DEFINED**

Scenarios live in `features/story_predictions.feature`.

**R25.1** **Exactly three choices, required** (Q63).
**R25.2** The prediction is **put on arrival** at its step, in the centred overlay; `n` dismisses it
and steps on, which is what makes it non-blocking (Q64).
**R25.3** A **wrong pick shows that choice's feedback and leaves the choices up**. You may pick again
as often as you like.
**R25.4** **Only a correct pick replaces the choices** with its feedback.
**R25.5** The Walkthrough records **that a Prediction was put, never which choice** was picked.
**R25.6** A Prediction already put is **not asked again**; re-authoring puts it afresh.
**R25.7** A Prediction may ask **any *why* about the step**, not only about the next hop — the
questions the CLIs actually wrote were better than the definition was.

- ✅ Arriving at a prediction step puts the question up
- ✅ A step with no prediction shows no overlay
- ✅ A wrong pick shows its feedback and leaves the choices up
- ✅ A wrong pick may be followed by another
- ✅ A correct pick replaces the choices with its feedback
- ✅ Answering never blocks the walk
- ✅ The walkthrough never records which choice was picked
- ✅ A prediction already put is not asked again
- ✅ A prediction skipped is not asked again either
- ✅ A prediction re-authored is put afresh
- ✅ A prediction that does not offer exactly three choices is invalid

**Q63 — How many choices? Resolved: exactly three, enforced.** Measured: all **14** predictions three
hosted CLIs wrote unprompted used exactly three, though the prompt's own example showed two. Rejected:
two (a coin flip, so a correct answer means nothing), four or more (thins the distractors, and the
prior art's warning is that a bad distractor costs more trust than the question earns), and a variable
count (the overlay is sized to its content).

**Q64 — Put on arrival, or opened with a key? Resolved: on arrival.** A prediction you have to ask for
is one nobody sees, and 13 of 18 stories carried exactly one, so it is rare enough not to interrupt.
Rejected: a key to open it, and blocking the walk until it is answered — a graded gate turns a review
into a test, and a test gets abandoned.

**⛔ Grading a typed guess with an AI.** Rejected in favour of one-keypress multiple choice: typing a
guess and waiting for a verdict is a test.

**⛔ Storing which choice was picked.** A history of wrong answers is a score by another name, and this
is a senior reviewer reading a colleague's change.

## F26 — Markdown preview — **DEFINED**

Scenarios live in `features/markdown_preview.feature`. The row-to-line map is argued in
`docs/adr/0007-a-preview-row-is-not-a-line.md`.

**R26.1** A `.md` or `.markdown` buffer opens as a **Preview** — the document, laid out — and any
other buffer opens as **Source**. The two words are `CONTEXT.md`'s; a Preview is not a fourth View.
**R26.2** `:preview` **toggles**, and nothing else does. The choice is **per buffer**: it does not
ride along to the next file opened and it is not persisted across restarts. `:preview` on a buffer
that is not markdown, or with no buffer open, refuses in the footer.
**R26.3** Preview exists in **Edit view only**. Story view's code surface and Review's diff draw into
the same rectangle and are untouched — a Site mark is a claim about lines, and rendering those lines
away leaves the reviewer unable to see what the Step points at.
**R26.4** The render **reflows to the pane width**, uncapped. Fenced code is the exception: it is not
wrapped, and it is syntax-highlighted through F14's highlighter, which takes a language name as well
as a file name.
**R26.5** What renders: headings (markers consumed, weighted by level), wrapped paragraphs,
emphasis, strong, strikethrough, inline code, `•`/`◦` lists keeping ordered numbers, `☐`/`☑` tasks,
prefixed block quotes and their GFM alert kinds, hard breaks, rules, aligned tables with truncated
cells and honoured column alignment, fenced *and* indented code, link **text** with the URL hidden,
`🖼 alt` for images, literal HTML set apart, and YAML frontmatter set apart above the document.
Frontmatter is shown rather than hidden: a Preview that silently omits part of the file gives the
reader no way to notice.
**R26.5a** **Nothing a markdown file can hold renders as nothing.** Every construct the parser can
emit either renders as itself or is **shown as the source the author typed** — never dropped. Which
of the two each construct gets is a recorded decision, not an accident: a terminal cell cannot
superscript or set an equation, so `H~2~O` and `$x^2$` are honestly better as the characters in the
file than as `H2O` and `x2`, while a footnote reference is better as `[1]` than as `[^1]`. The list
of constructs and which answer each one gets is in `.scratch/markdown-preview/spec.md`; the
mechanism that stops the list going stale is R26.5b.
**R26.5b** The construct list is **enforced, not maintained**. The classification of the parser's
tags is exhaustive, so a construct the parser grows does not compile until somebody decides what it
renders as, and a sweep drives one sample of every construct and holds each to producing rows,
rendering as the kind it was classified as, and leaking no marker — unless it is named in an explicit
omissions list with the ticket that owns it. The kind is in the sweep because the two mechanical
questions alone pass a block quote that renders as an ordinary paragraph, which is the gap in the
prose rather than in the reader's attention.
This is `src/keys.rs`'s key sweep applied one layer up, and for the same reason: the four formatting
gaps that were found by eye in a Preview (uniform headings, flat emphasis, flat inline code, missing
rules) were all things no test could fail on.
**R26.5c** **A row whose glyph is not in its text is held on the drawn line, not on the row.** A
thematic break is the one such row: it carries no pieces, so R26.5b's sweep — which asks that a
construct produces a row with text on it and that some row has the kind it was classified as — is
satisfied by an empty `Rule` row, and the missing rule shipped anyway. The glyph must stay out of
`Row::text`: the hover box's own width is measured off it, "every row is blank" is how a server
saying nothing is told from a server saying something, and find-in-file, the word motions and
drag-copy all read it — a bar of `─` in any of those is a rule that has become text. So the rule is
drawn at the width the caller is drawing across (the pane's measure, or the box's inside) and held by
a renderer unit test, which is the only place that can see it. This is not a Preview-only cost: a
server's hover is markdown through the same rows, and `rust-analyzer` divides every section with
`---`, so six of a sixteen-row hover box were blank.
**R26.6** A **mermaid fence renders as a diagram**. When it cannot — an unsupported diagram type, a
parse error, a diagram too wide for the pane — the reason is stated and the fence's source is shown
as highlighted code beneath it. That is exactly what the reader would have had without mermaid
support, plus the reason they are looking at it.
**R26.7** A **Preview row is not a source line.** The render returns rows and a row-to-line map;
every row carries the line of the block it came from. Everything that used to assume row = line reads
the map.
**R26.8** The cursor is a **row and a rendered column**, and it answers the motions Source answers —
`h l 0 $ w b e gg G`, the arrows, and the word-motion arrows — run over the **rendered rows** rather
than the source lines, so a word motion crosses a wrap the source line does not have and `$` stops at
the end of the text on screen rather than at the end of the markup. The column is a column of what is
drawn, counted in **characters**, so `editor_hscroll` follows it exactly as it follows Source's.
Crossing to Source keeps the **line** and resets the column; crossing to Preview goes to the first row
at or after that line, or the last row if there is none — column one in both directions, because the
row map carries no source column to hand over. On a row with no text of its own (a diagram, a rule,
the blank between blocks) the column is one, which is all the text there is to be on.
**R26.8a** A motion in a Preview **never touches the buffer**: it moves the row cursor and nothing
else, so the source line and column the reader crosses back to are exactly where they were left. A
Preview is read-only (R26.9) and a motion that quietly moved an invisible second cursor is the same
failure by another name.
**R26.8c** **A place arriving from outside is a source line, and lands on the row it was rendered
from.** A markdown file opens previewing, so a search Hit, a Risk row, a definition and a Visit all
arrive at a line while the caret lives on a row — and setting the line directly moved an invisible
second cursor and left the reader looking at the top of a render that never went where they asked.
The crossing is the landing's, in one place for every caller: R34.20a.
**R26.8b** A half-typed `g` is **answered by the key that follows it, whatever that key is**. `gg`
goes to the first row; a `g` over a motion is refused by name (`gl`) exactly as Source's operator
refuses it, and a `g` over a refused edit **that no chord claims** is answered by that refusal — in
every case the chord is cleared. The exception is not a special case for one key but the rule the
refusal is *about*: R26.9 refuses what would **edit**, and `gp` (R34.9a) neither edits nor dirties
anything, so the refusal is an answer to a gesture nobody made. `gg` and `gt` were never in the
refusal's key set and so never showed it; `gp` was, and inherited an answer about editing for a jump.
A `g` that outlives the key after it fires as `gg` at the next one, which teleports a reader to the
first row for a key they pressed once, and takes `gt` with it.
**R26.9** A Preview is **read-only**. `a o O I x r dd D p P u V` do nothing and say so in the
footer. Motions, `/` `n` `N`, yanking a selection, and every `:` command still work — but a
`:` command that *authors* text crosses to Source first (R32.12), because a Preview it edited is a
Preview whose `u` is refused two keys later.
The refusal is spoken rather than silent: a key that vanishes without a word is the failure
`src/keys.rs` exists to prevent. `i` is the exception, and not a refusal at all: it means "let me
change this", so it crosses to Source by R26.8 and arrives in insert mode. `:preview` crosses in
normal mode — the two are different intentions, "show me the characters" and "let me type here".
**R26.9a** Extending a selection with the **keyboard** — Shift-arrow, Shift-Alt-arrow — picks
exactly what a **drag** picks: a span of the *rendered* rows, never one of the source lines Source's
Shift-arrow names. Selecting reads nothing but what is drawn, so it is no read-only refusal; the
keyboard rewrites into the motion and then into the drag's own span, so one press of Shift-Right and
one drag of one character are the same selection. A plain motion drops it, the way a plain arrow does
in Source. Without this a Preview had no selection a keyboard could make, and `:read` (F35) — whose
passage *is* the Selection — was unreachable in the shape markdown opens in.
**R26.10** `/` searches the **rendered rows**, and a drag copies the **rendered text** exactly as
drawn, wrap newlines included — `docs/adr/0002-ai-pane-selects-the-visible-screen.md` settled that
shape for the other surface where what is on screen is not a file. What a match becomes when it is
landed on is the same rendered span: a found word copies as it is drawn, never as the source line
that is not on screen spells it.
**R26.11** Links are **styled text and nothing more**. Following one is a separate feature: it needs a
rule about resolving a path out of an untrusted file, which is a security question and not a clause
here.
**R26.12** A Preview renders **whatever the buffer holds** — the draft if there is one, disk
otherwise — so Source edits, watcher reloads and divergence all behave as they already do, with no
rule of Preview's own.
**R26.13** No line-number gutter in a Preview, and the pane title says `preview`. Line numbers in a
rendered document name rows the reader cannot act on, and the five columns are worth more as text.
**R26.14** A row carries **styled pieces, not one string**. Emphasis, strong, strikethrough and
inline code are differences *inside* a row, so a row whose text is one `String` cannot express any of
them — and wrapping then has to re-split those pieces at the wrap points rather than a plain string.
The renderer names what each piece **is** and never what colour it is, exactly as F14's highlighter
returns token kinds: a bullet glyph and a heading's weight are a theme's business in the same way a
colour is.

Deliberately not here: a wrap-width cap (a magic number that immediately wants to be a config key,
and the pane can already be narrowed), a bare key for the toggle (the cheatsheet is the contract for
what is bindable, and a letter is too expensive to spend on a guess), and relative-link navigation
per R26.11.

## F27 — Risk is computed for the workspace — **DEFINED**

Scenarios live in `features/risk_figure.feature`. The vocabulary is `CONTEXT.md`'s "Paying down
risk" section. Why Varde may redraw without input while a job runs is argued in
`docs/adr/0009-a-spinner-is-bounded-by-its-job.md`.

**R27.1** Opening a workspace **starts the analysis without being asked**. The figure being there
when you want it is the whole point; a figure you have to request is a figure nobody requests. **A
Bare workspace is the exception** (`features/bare_workspace.feature`): the figure would be measured
into a Sidecar deleted at exit, so opening Varde in a large repository to read one file analyses
nothing and the pane reads `nothing-analysed` until R27.13's recompute is asked for. Saying
`computing` there would name a job nobody asked for and that will never answer, so only a job **in
flight** is `computing`.
**R27.2** The headline is the **Risk count** — how many Functions sit above the threshold — and it
lives on the **tree pane's top border**: a spinner while the job runs, the count when it lands, so
the same place on screen always answers the same question. Not a percentage and not an average, for
the reason Coverage is a count and a list (R8.x, and the glossary): every denominator is a judgement.
**R27.3** The metric is **labelled by what it measured**. With no test coverage read it is `CX`,
never `CRAP`. The concept keeps the name **Risk** either way, so reading coverage later renames
nothing.
**R27.4** A workspace in no language the analyser handles is **`nothing-analysed`**, and shows no
count. A fabricated zero is a clean bill of health nobody was given. A Bare workspace that has not
been asked to measure lands in the same standing for the same reason: no figure, and a recompute is
how one arrives.
**R27.5** A file in a handled language the analyser could not read is **Unparsed**: counted and
shown, so the reader knows the figure describes less than the whole workspace when it does. **A space
the analyser could not name is the same fact** — the file was read and part of it could not be named,
so the space is excluded from the count and its file is Unparsed. Unparsed stays a fact about a file,
as the glossary has it; an unnamed space is how a file comes to be one.
**R27.6** A **Function** is a function whose enclosing space is not itself a function. A closure
counts toward the Function holding it; containers — impls, classes, namespaces, files — are read and
never counted. This is the rule that makes the count un-gameable by nesting, so it lives in the
library under unit test rather than at the edge.
**R27.7** The **threshold is configuration** (`risk.threshold`, default 15, in F9's per-project
config), because a codebase with different norms sets its own bar.
**R27.8** A workspace that has moved since the figure was computed makes it a **Stale figure**: it
says so, and it still shows the figure it has. A stale answer beaten into shape is worse than a
stale answer labelled.
**R27.9** **A save marks the figure stale and starts nothing.** Re-analysis happens when the commit
moves, when it is asked for, and after an Iteration. Analysing per keystroke-batch is exactly what
"never parse per frame" was written about.
**R27.10** The figure is **cached against the commit it was computed at**, written to `.varde/risk.json`
as **the library's own shape** — path, name, start line, **every recorded metric** (cyclomatic and
cognitive complexity, the maintainability index and lines), plus the commit and which metric produced
it. All four are recorded even though only the primary one is displayed, because R29.9's third
condition is over *any other recorded metric* and a Gate that only ever sees two is a Gate with two
blind spots. The analyser is pinned pre-1.0; its types may not reach the file format.
**R27.11** **A recompute supersedes one in flight** rather than queueing: two analyses of two
different workspace states cannot both still be true.
**R27.12** **Varde writes no git ignore rule** — F9's rule, unchanged. The figure file, the snapshot
directory and the sentinel are per-user derived data and belong in *this repo's* `.gitignore`
(alongside `.varde/state.json`), which is a commit in ticket 04 and not a behaviour.

**R27.13** The recompute has **a gesture of its own**: an action on the Risk pane, beside the loop's.
Without one, a Stale figure is a dead end until the commit moves — and the entry-points table below is
the audit that caught five fully specified, completely unreachable features. A `:risk` command was the
alternative and was turned down: it is new vocabulary for a gesture that belongs where the figure
already is, and R28.2 has just argued that the pane costs no new binding.

⛔ **The spinner's glyphs, and the tick that advances them.** The glyph sequence is copy. The tick,
the zero idle CPU it returns to and the one-frame-per-batch guarantee are edge properties measured by
running it — same verdict as "fast and responsive" in the table above, and the reason ADR 0009 states
them as acceptance criteria on ticket 05 rather than as scenarios.
⛔ **The six languages, one scenario each.** Python, Rust, C/C++, Java, JavaScript, TypeScript/TSX
are the languages the crate implements the metrics for. A scenario per language is a transcription of
a lookup table; the table gets a unit test, and R27.4 covers what a seventh language produces.
❓**Q55 — do the analyser's parent-space metrics include their children's contributions?** Assumed,
not verified, and R27.6 under-reports if they turn out to be exclusive. Deliberately not a scenario:
it is a fact about a pinned dependency, so it is settled by the one fixture-based unit test ticket 03
earns. Not blocking — the Function rule is written either way, and the test says which.

## F28 — The Risk list — **DEFINED**

Scenarios live in `features/risk_list.feature`.

**R28.1** The pane is **toggled from the palette**, one un-indented entry in the `Panes` group,
keyed `k`. A pure toggle: shown means hide. `r` is Review's, and keying it on the metric (`x` for CX)
is the one mistake the glossary explicitly forbids — the metric's name changes and Risk's does not.
**R28.2** **No new key binding and no cheatsheet obligation.** Focus is directional so the pane is
reachable by geometry, and a palette row is discoverable because the palette lists it. R6's palette
table gains the row when the toggle lands, in ticket 06 — not here, so this spec pass leaves the
suite's passing set exactly as it found it.
**R28.3** The pane sits **beneath the tree, at the tree's width, taking width from the shell pane** —
the mirror of what `:tall` does from the other side. Hiding it returns the shell pane's full width and
resizes its pty. The tree, the editor and the AI pane do not move either way, and with the AI pane
tall as well the shell pane keeps a floor of at least one column.
**R28.4** **Toggling it on moves focus into it**; toggling it off returns focus to the tree. Opening a
pane in order to use it should not need a second gesture.
**R28.5** The **downward focus gesture** reaches it from the tree and the shell pane from it; with the
pane hidden that gesture still reaches the shell pane directly. Every direction is written both ways
round, so the **leftward gesture reaches it back from the shell pane**, whose left edge is where the
pane ends: a pane that can be left sideways and not re-entered is reachable only from the tree, which
is two panes from the shell.
**R28.6** **Visibility persists** in F9's per-user state file. The layout is the user's.
**R28.7** The list is **flat, sorted by figure descending, above-threshold only by default**, with a
toggle showing every Function. A worklist, not an inventory — and the toggle is there for the figure
of something that is not currently a problem.
**R28.8** The **Unparsed count is shown in the pane**, for R27.5's reason.
**R28.9** With no figure yet the pane says **`computing`**; with a Stale figure it shows the **stale
list, marked stale**, while a fresh one is computed. The old answer beats no answer, labelled.
**R28.10** The **selected row's file goes in the border**, not in the row: the pane is narrower than a
path, and one ordering rule across the whole Scope is worth more than rows grouped under file headers.
**R28.11** `Enter` **and a click both open that file with the cursor on the Function's first line**,
through the existing open-at-a-place effect the Story jump and the search hits already use. The mouse
and the keyboard do not disagree about *what* opens — they do about where the keyboard lands: `Enter`
is the deliberate "take me there" and follows into the editor, while **a click leaves the keyboard in
the pane it clicked** (R10.1), the same exception the tree's own click already makes. Without it the
only clickable part of the pane that focused it was its border, which is the one part that is not a
row.
**R28.12** The selection is a **Row selection** (`CONTEXT.md`): it names something to go to, so it is
never copied as characters and never becomes the selection. **A drag over the pane picks nothing** —
it returns no span at all, rather than a span the edge fills off some other pane's grid.
**R28.13** **Scrolling is state clamped in the core**, as it is for every other list — the renderer and
the hit-test read the same field, every event but the wheel pulls the selection back into view, and the
pane's own chrome takes rows so its row count is not its height.
**R28.14** A row carries an **action asking an AI session for a suggested refactor of that one
Function**: one prompt carrying the name, the file and the figure, through the existing hand-off,
held until the session has spoken, starting a session if none runs. **No Iteration, no test run, no
Gate, no revert, nothing committed** — a suggestion is a suggestion. No provider named, in the code or
the prompt.
**R28.15** The **pane's own actions are reached by stepping down off the end of the list** — one slot
past the last row, the way the Story spine's Remainder is — and along with `Left` and `Right`, `Enter`
running the one that is lit. The same gesture a row's own icon takes, and the same event the icon
raises when it is clicked: an action reachable only by mouse or only by a letter is an action half
the users cannot find. An empty list starts there, because a Scope with no rows is exactly the one
where the recompute is the only thing left to reach.

⛔ **The exact rectangles.** Pinned by layout unit tests in both pane states and combined with the AI
pane's tall shape, as F10's already are — a scenario cannot see a rectangle, and the tests that can
are the ones that catch a click landing one row off.
⛔ **A draggable width.** It takes the tree's width; see Out of Scope in the spec.
⛔ **That the pane's own chrome takes rows, and its last row is reachable.** The same trap the tree's
filter box already documents, and the same verdict: a row count is not a height, and what catches it
is the layout and hit-test unit tests rather than a scenario that cannot see either.
⛔ **Any figure in the editor** — no gutter marker, no annotation, no injected row. The list is the
whole interface.

## F29 — The Refactor loop — **DEFINED**

Scenarios live in `features/refactor_loop.feature`. The decision that Varde measures rather than
believing the session is argued in `docs/adr/0010-varde-owns-the-test-gate.md`;
`docs/adr/0004-hosted-panes-are-transparent.md` is why it has no alternative.

**R29.1** Varde hands the session a **Scope and a goal** and decides for itself whether the pass was
good. One generic prompt as a constant, interpolating the Scope, where the figures are written, the
worst few Functions, the target, and an instruction to obey whatever convention file the repo holds.
It also carries **what a good split is**: the figure is the symptom rather than the goal, every
extracted function must have one responsibility its own name states, a helper called from exactly one
place or named after where it was cut from is structural scattering and a failed pass, fewer
well-named extractions beat many small ones, and a function that cannot be split that way is left
alone and said so rather than shredded. The row action's one-shot ask (R28.14) carries the same
instruction in one sentence, because it is the one with no Gate behind it.
**R29.2** **The test command is never in the prompt**, because Varde runs the tests. That removes the
most project-specific string in the system from a prompt that has to work on any workspace.
**R29.3** **No provider is named**, in code or prompt. Delivery reuses the hand-off review submission
and story confirmation already use, including holding the prompt until the session has spoken.
**R29.4** The **test command is configuration** (`risk.test_command`) **falling back to detection**
from the project's shape. It lives under `risk` because the Gate is its only consumer.
**R29.5** **A test command that cannot be determined refuses the loop.** A Gate that reports a pass
having run nothing is the one failure mode worse than no Gate.
**R29.6** **Completion is a filesystem fact**: the session writes `.varde/refactor-done`, seen by the
watcher that already exists, and Varde **deletes it before each Iteration begins** so a leftover
cannot instantly complete the next one. Its contents are ignored — Varde measures.
**R29.7** **Silence is never completion, and the wait has no timeout.** A session thinking for forty
seconds looks exactly like one that finished, and timing out means measuring a half-written edit. The
exits are the stop action and the cap. The pane therefore **says what it is waiting for**: an
indefinite wait is only survivable when it is distinguishable from a hang.
**R29.8** **The tests run off the shell pane.** The shell is the user's; a loop that types in it takes
it away.
**R29.9** **The Gate is three conditions**: the tests still pass, the primary figure moved down, and
no other recorded metric moved up. Failing any one **reverts and stops**, and **which condition
stopped it is recorded** — "it gave up" is a diagnosis, not a mystery.
**R29.10** **Improvement counts the Risk count or the total.** Chipping a Function down without yet
crossing the threshold is progress; without this a genuine first pass reads as zero and the loop quits.
**R29.11** **The third condition is the anti-gaming defence, and it is structural.** Shredding one long
Function into fifteen trivial ones lowers the count while cognitive complexity holds or rises, and is
reverted. Goodhart's law arriving on schedule; the answer is an objective that cannot be gamed, not a
prompt that asks nicely. The prompt now *states* that intent (R29.1) while the Gate still *enforces*
it: the wording spares an Iteration spent on a pass that would be reverted, and it is the only thing
covering the ungated row action — it does not soften the third condition.
**R29.12** **Snapshots are per Iteration and cover only the files that Iteration touched**, and a
revert goes to the snapshot, **never to the last commit**. So a file you had edited and the loop did
not is never restored over, and your own uncommitted edits in a file it did touch come back as yours.
**R29.13** **Nothing is committed, ever.** The loop's entire output is one dirty working tree, and
Review view is where it is judged.
**R29.14** **A reverted Iteration is explained to the session as well as to me**, carrying the
condition and the tests' output. A session that believes its reverted edit landed builds whatever you
ask next on a false premise — "never silent" applied to the agent as a consumer of errors.
**R29.15** **The cap is configuration** (`risk.max_iterations`, default 10) and is honoured, so a loop
cannot run indefinitely while nobody watches.
**R29.16** **A long run is legible**: the pane shows the Iteration and the cap, the figure as it moves,
and the last test result; the tree border shows the live figure. Once the run is over what it left
behind **trails the border's figure until the next measurement, not the next start**: a recompute is a
fresh measurement, so a `tests-failed` sitting beside a number measured long after those tests
stopped failing is a border making two claims about two different moments. A running loop's own
verdict is untouched by a recompute — mid-run those are the Gate's, and the Gate recomputes as part of
its own pass.
**R29.17** **Stop is the start action flipped**, in the same place. Not Escape: it is heavily
overloaded and too easy to hit by accident for something midway through editing files. Stopping leaves
the workspace where the last accepted Iteration left it — nothing half-applied.
**R29.18** **A second loop is refused, visibly.** Two loops editing the same files is two agents
fighting. Contrast R27.11: two analyses supersede, because only the newer answer can still be true.

**R29.19** **A loop with no figure yet is refused**, the way R29.5 refuses a missing test command: the
Gate takes its baseline from the figure Varde last measured, so a loop started while nothing has been
measured is judged against zero and can only ever read as `no-improvement` — it would revert an
honest first pass and stop. Refused rather than queued behind the analysis: a start that silently
waits is a start the user cannot tell from a hang, and the recompute is one gesture away in the same
place. A *measured* figure of zero is a baseline and starts fine; only "nothing measured yet" is
refused.

⛔ **Progress reporting from the job.** Completion only. A percentage needs a measurement to justify
it, and a thread reporting files scanned is the 253-draws measurement arrived at from another
direction (ADR 0009).
⛔ **A loop scoped to a single Function.** The row action (R28.14) is one prompt; scoping the gated
loop that narrowly is later work.
⛔ **Auto-committing anything**, per Iteration or at the end. R29.13 is the constraint the rest hangs
off.

## F30 — Risk in Review view — **DEFINED**

Scenarios live in `features/risk_in_review.feature`.

**R30.1** Entering Review view **computes Risk for the files under review** — the Scope is those files
and never a mix. A number on screen during a review that describes the whole codebase is the wrong
number in the wrong place.
**R30.2** The delta is measured **from the revision the review diff is already measured from**, and the
scenarios say exactly that rather than naming a revision. A figure that disagrees with the diff beside
it is worse than no figure. F7 measures against HEAD today; naming the relationship rather than the
revision is what keeps the two from drifting apart.
**R30.3** The **border shows the delta**, not the workspace's Risk count, and **a change that raised
the figure is marked worse**. That is the signal a reviewer most wants and least reliably gets from
reading a diff, so it must be impossible to miss. **Leaving the view puts the workspace's count back**:
the border always describes the Scope on screen.
**R30.4** The **Risk list in Review view lists the Functions in the changed files with their deltas**,
so "which part of this change added the risk" has an answer. No workspace Function appears.
**R30.5** A reviewed file in no language the analyser handles **contributes nothing and is never
reported as an improvement**. R27.4's rule, applied to a Scope of one change.
**R30.6** **The loop over the reviewed files is the same machinery** with a different file set, and
every guarantee holds identically — the Gate's three conditions, the per-Iteration snapshot, the cap,
the stop action, and nothing committed. A safety guarantee that depended on which Scope you chose
would not be a guarantee, and **the Gate stays three conditions** for exactly that reason. The prompt
names only the files under review, and that no file outside the Scope was changed is asserted as an
**absence**, the way this suite asserts every other promise not to do something. A fourth,
review-only Gate condition was the alternative and was turned down: it would make the review loop
gated differently from the workspace one, which is the thing this rule exists to forbid.
**R30.7** Entering Review view with a **Stale figure recomputes for the reviewed files** rather than
drawing a stale delta.

❓**Q56 — should the single-Function ask (R28.14) be offered on a Review-view row too?** Unspecified,
and not blocking: it is the same row action on the same list, so it follows whatever the row-actions
lookup does once it reads the rows actually on screen. Left for the ticket that has both in front of
it rather than guessed at here.

## F31 — Language intelligence — **DEFINED**

Scenarios live in `features/language_intelligence.feature`. The decision that a Language server is a
second hosted child — and that shipping defaults as TOML data is not the provider-naming ADR-0004
forbids — is argued in `docs/adr/0011-a-language-server-is-a-second-hosted-child.md`. `CONTEXT.md`'s
"Knowing what the code means" section is the vocabulary every scenario uses.

**R31.1** A server is **named in configuration**, in `[lsp.<language>]` tables carrying a `command`
and its `args`, and **no branch in `src/` names a server**. The falsifiable form is a grep: search for
any server's command name and every hit is inside the built-in defaults string or a test fixture.
**R31.2** **A server is a row in a config file, and nowhere else** (ADR 0018). The rows ship as the
template `~/.varde/config.toml` is seeded with, and a start that finds no global file reads the
template as that layer, so a fresh install works with nothing configured; a global file naming no
`[lsp.*]` row starts no server at all. F9's deep merge means a project
overriding one language leaves the others alone for free — **and a project naming one *key* of a
shipped language leaves that language's other keys alone too**. A layer is a *patch*, so every key in
it is optional and completeness is required of the **merged** table instead. Held of a layer, the
only partial override possible was one that repeated `command`, a value the reader had to copy out of
a binary's built-in defaults and which then silently stopped tracking them. The only change to F9's
interface is one accessor that can **enumerate** the configured languages — the current dotted-scalar
lookup cannot see sub-tables.
**R31.3** **A language named in no layer has no server, and that is a state the core expresses** rather
than an absence it infers from a failed lookup. The scenario proves the difference by **enumerating**:
a configured language is listed and an unconfigured one is not, which a naive failed lookup cannot
satisfy. A malformed server entry stops Varde from starting
naming file and line, which is R9.5 unchanged and is why that scenario passes on the commit that
writes it — the requirement is genuinely already met, and the scenario exists so that breaking R9.5
breaks it.
**R31.4** **The handshake happens once per language, and it begins against a process rather than
against the asking for one.** `sync` returns `Effect::StartLsp` and remembers nothing; the edge
spawns, holds the child, and says so — `Event::LspStarted` — and the pass that follows sends
`initialize` and creates the conversation. Recording it when the spawn was *asked for* is
`ai_running`'s shape, and it worked only because effects happen to run in the order they come back.
Nothing is asked of a server before the handshake completes.
One server per language, **reused across buffers** — a language is what a server serves, so
opening a second Rust file joins the conversation already running.
**R31.5** **A capability the server declined is never asked for.** Branching on what the server said
about itself in its own initialize reply is reading the protocol, not naming a provider; branching on
which server it is, is the thing R31.1 forbids.
**R31.6** **The server sees the Buffer on screen, never the file on disk**, and every message carries
the Buffer's revision as the **Document version**. `Buffer::revision` is bumped by every content
change and nothing else, which is already why the edge caches highlighting against it, so nothing new
needs remembering. A motion changes nothing and is therefore told to nobody.
**R31.7** **A message naming a Document version the Buffer has moved past is dropped in the core** —
one arm, and the behaviour most likely to be broken by a plausible-looking implementation while being
invisible without a test: a reply about text the user has already edited past, rendered as current,
points at lines that have moved. A message carrying **no** version is kept: the protocol permits it,
and dropping it would discard every diagnostic from a server that does not send one. **A reply to a
request nobody made is dropped**, not trusted, which is what request/response correlation buys.
**R31.8** **Diagnostics are a server push, keyed by path and then by publisher in state**, so a
file's marks survive switching away and back. **Severity is distinguished** — error, warning,
information, hint — because a hint drawn like an error is a gutter nobody reads. A later push
**replaces** the earlier set rather than adding to it, and an empty push clears the file: the gutter
is a statement about the present, not a log.

**By publisher, because a `.vue` file has two servers with two opinions of it** (R31.28): the Vue
server marks a template mistake and a TypeScript server reports the type error. A push is one
server's whole current opinion, so what it replaces is *its own* last opinion and nothing else —
keyed by path alone, the second publisher erased the first and which mark the reader saw depended on
who spoke last. Everything that reads them merges across publishers: the gutter takes the worst on
the line whoever said it, and Review view counts a file across every server that spoke about it. It
is also what makes a dying server exact rather than conservative — `lsp::gone` used to drop every
mark on every file the dead server *served*, so a Vue server exiting took the TypeScript server's
type errors with it. A path left with no publisher at all stops being a path anything has spoken
about, which is what keeps Review view reading *not measured* rather than announcing a dead server's
file clean. **Varde says it can be told them**, in the `capabilities` of `initialize`: a push is the one
thing a server sends unasked, and a server may hold it back from a client that never claimed it —
`typescript-language-server` does exactly that, and a file with every error withheld reads as a clean
file rather than as a server saying nothing. It is the only capability Varde claims, because
everything else it wants it asks for by name.

- ✅ Varde says it can be told diagnostics, because it draws them
- ✅ Two servers publishing about one file both mark it, and each replaces only its own set
- ✅ One server calling the file clean does not clear what the other said
- ✅ A server that dies takes its own marks and leaves the other server's
- ✅ A file two servers report on is counted across both of them
**R31.9** The mark goes in **the gutter the layout already owns** — the editor's line-number strip,
whose width is `layout::GUTTER` — so **the width does not change with a diagnostic's presence**.
Nothing re-derives it, for the reason the one-layout rule exists.
**R31.10** **Hover, go-to-definition and completion are asked for; diagnostics are not.** Diagnostics
therefore come first: they need no correlation and so prove the whole transport at the lowest cost. If
diagnostics land on the right lines, the pipe is right.
**R31.11** **Every binding is reachable without a modifier and is listed in the cheatsheet**, and the
existing `every_key` sweep holds it to that. `K` asks for Hover and `gd` for a Definition — vim's own
spellings, and neither is taken. Key handling goes in `keys.rs` with unit tests, never in `main.rs`.
**R31.12** **Go-to-definition reuses the effect that already opens a place in a file** — `OpenAt`,
which a search hit, a Story citation and a Risk row all reach a line through, and which is
`OpenBuffer` plus the place the cursor has to land on — and adds no effect of its own; a definition
in the file already open moves the cursor and opens nothing. That last case is the one landing with
no landing *event*, so it **tells the cursor history the place it is leaving** (R34.3c): `gd` onto a
name in the file you are reading is the most common back-jump there is, and a jump with no way back
is half a jump. **Several definitions are shown in the
list the project search fills**, because several definitions are several places to open, which is what
a Hit already is — the first is never taken quietly. They carry no line text, because the files are
not open: a definition is a file and a line, and reading the line would mean reading a file the core
may not read.
**R31.13** **A definition outside the workspace root is refused, naming the path it would have
opened.** Naming it is what needs an effect the feature did not have: a notice is a `&'static str`
slug the edge words, which cannot carry a path, so `Effect::NotifyAbout` says the slug *and* the
thing the message must name. That is not the new effect R31.12 forbids — R31.12 is about the jump,
and a refusal that names nothing is a key that did nothing. Every other pane in Varde is bounded by the root — the tree, Review view, the project
search, a Risk Scope — and a Buffer outside it is a Buffer the tree cannot mark, the review cannot
see and the watcher does not watch. Refusing *and saying where it went* is the difference between a
decision and nothing happening. The rejected alternative was opening it read-only: it is the more
useful behaviour for jumping into a dependency, and it can be added later without undoing this,
whereas opening it now puts an exception in the workspace boundary that nothing else honours.
**R31.14** **The Candidate list is one `Modal` variant**, never a pair of booleans that can both be
true — the existing "enums, not booleans" rule, enforced by the type and therefore not a scenario. Arrow keys move the choice, `Enter` accepts, `Escape` dismisses; accepting inserts the
Candidate's text and closes the list; no Candidate matching the prefix closes it rather than showing
an empty one; and the list is positioned so it **does not cover the line being edited**. The existing
"one interpretation per key" rule already makes a list that consumes keys behave correctly under the
batched input loop.
**R31.15** **`Escape` leaves the Buffer holding exactly the characters that were typed.** This is the
feature's load-bearing absence: completion that quietly changes what you wrote is worse than none.
**R31.15a** **A Candidate list is a claim about one file, and goes when that file leaves the
screen.** It went when the keyboard left the editor, which was the whole of it for as long as nothing
could change the current Buffer with the keyboard still *in* the editor — R34.9's jump back can, and
a list left standing over the file just jumped to claimed `Enter` for a completion asked about
another document and wrote that document's word into this one, which is R31.15's absence broken from
the other side. The rule is the one a snippet's stops already have (R31.14), and for the same stated
reason. A fixture arranging a list that names a file nobody is looking at is arranging a state the
core cannot be in.
**R31.16** **Requests are debounced, and the window is the core's number, never the edge's.** No
sleeping and no real waiting in tests, and no guessed delay anywhere. Fast typing queues one request,
not one per character. Mechanically this is the split F6's clock draws, one step further out: the
core returns the window as an effect on every change while inserting — a timer *reset*, not a queue —
and the edge, which holds the only clock, answers with one event when it expires. So the number is a
value a scenario can see, the way `RunSearch`'s 150 ms — chosen inside `main.rs` — is not.
**R31.17** **Review view shows, per changed file, how many errors and warnings its server reports, and
a total of each.** This is where language intelligence pays for itself in the product Varde actually is: the
workspace exists to review code an AI wrote, and "does it compile" is the first question a reviewer
has and the last one a diff answers. Same shape as F30 — a per-file figure over the files under
review — with diagnostics instead of Risk. Scope is a count and a total, deliberately **not a
diagnostics browser**.
**R31.18** **A file with no server, or a server that has not yet answered, reads as *not measured* and
never as zero.** F27's Unparsed distinction, applied to diagnostics: telling a reviewer that a broken
file is clean is the failure this rule exists to refuse. It stops being not measured when the server
answers, including when it answers with nothing.

**And so does the total.** A sum over the measured files alone reads as a statement about all of
them: `0 errors` over a change with an unmeasured file in it is the same lie, quieter — this rule
failing rather than a case it does not cover. So the total is one of three answers — nothing
measured, the whole change, or a **floor** — and the border marks the floor with a leading `~`.
Leading, because a border truncates from the right, and a marker the narrow pane drops leaves
exactly the bare figure the rule refuses.
**R31.19** **Entering Review view spawns nothing, and tells the servers already running about the
changed files they serve.** The two halves of that are one distinction, and getting it wrong is how
this rule was first written: *spawning a process* and *telling a running server about a document* are
different acts, and ticket 12 forbids only the first — "nothing here spawns a server that ticket 07
would not already have started for an open buffer". A keystroke that launches one process per changed
language is an invisible side effect nobody asked for; a keystroke that hands documents to a
conversation already open is the protocol working as intended.

So a changed file is **synced read-only** — the server is told its contents, and no Buffer is opened
for it. The contents are the edge's to read (`Effect::ReadForReview`, answered by
`Event::ReviewFileRead`), because the core reads no files and `OpenBuffer` would put a file nobody
asked to see on screen. A file that cannot be read is answered with nothing and stays Not
measured, which is the honest answer rather than a zero.

That is what makes the counts describe the change rather than describing whichever files the
reviewer happened to open first, which is the whole point of ticket 12: seeing whether the change has
errors *before* reading a line of the diff. A changed file whose language has **no server running** is
Not measured, which is R31.18 doing its job rather than a gap.

What Review view being on screen gates is the *asking*: a document, once opened for the review, stays
open for as long as the file is part of the change, so a trip out of the view and back neither closes
every document nor blanks the counts. A file that leaves the review — put back to what HEAD holds —
is told closed **and its diagnostics are dropped**, because they describe text that no longer exists;
a Buffer's marks survive being switched away from (R31.8) because the file is still the file, and
that is the difference. Left standing, the same file modified again would re-enter the review already
carrying the last version's errors.

A read-only sync never changes, so it carries one fixed Document version for its whole life and
R31.7 has nothing to drop for it. R31.6's "the Buffer's revision" is a rule about Buffers; a file with
no Buffer has no revision, and this is why that is not a hole.
**R31.20** **Losing a server takes nothing away.** With none configured nothing is spawned and the
editor behaves exactly as it does today; a command that does not exist leaves the Buffer unchanged and
is **surfaced once**, never silently and never on every keystroke; and a server that exits is
**observed by the edge and told to the core, which never sets that fact itself**. The telling is what
clears that language's diagnostics and its outstanding requests — a diagnostic left in the gutter by a
dead server is the gutter making a claim about the present out of a conversation that ended. This is
`ai_running`'s failure in a second shape, and the ADR records the split of what the core may hold from
what only the edge may observe. **And a lost server's own words are kept.** Its stderr goes to
`.varde/lsp-<language>.log`, truncated per spawn, and both notices name the file: the two servers that
died on one machine in one week each said why in one line, and Varde threw both away, so "no
diagnostics until it is restarted" was true and cost a debugging session anyway.

- ✅ A fresh install has a server for the common languages, and the configured languages can be enumerated
- ✅ Project over global over default, key by key, and arguments travel with the command
- ✅ A language named in no layer has no server; a malformed entry stops Varde from starting
- ✅ A layer names one key of a shipped language, or of one the layer below introduced, and keeps the rest
- ✅ A language no layer ever gave a command stops Varde from starting, naming the file it came from
- ✅ Opening a file starts its server, completes the handshake, and only then sends the document
- ✅ One server per language across buffers; a second language starts its own
- ✅ A capability the server declined is never asked for
- ✅ The Buffer on screen is what is sent, at the Buffer's revision; a motion sends nothing
- ✅ A stale Document version is dropped; no version at all is kept; a reply to no request is dropped
- ✅ Diagnostics mark their lines, by Severity, and the message under the cursor is shown
- ✅ Diagnostics survive a buffer switch; a later push replaces; an empty push clears
- ✅ The gutter's width does not move when diagnostics arrive
- ✅ Hover shows type and documentation, says so when nothing is known, and is not shown after the cursor moves
- ✅ A long hover wraps before it is measured, and does not cover the symbol it describes
- ✅ Markdown in a hover is rendered, a fence is highlighted and cut to the box, and plain text is left alone
- ✅ A hover taller than the pane is capped and says it was cut; Varde declares it can read markdown
- ✅ A pointer resting half a second on a symbol asks the same hover, in either mode; the box describes where the pointer is and goes when it leaves (`features/hover_dwell.feature`) — amended by R48.3: it stands while the pointer is on the box too, the wheel scrolls it, and a second `K` moves the keyboard into it
- ✅ Go-to-definition in the same file, in another file, several, none, and outside the root
- ✅ Candidates are offered, moved through, accepted, and dismissed leaving the text as typed
- ✅ No matching Candidate closes the list; the list does not cover the line being edited
- ✅ Requests are debounced from the injected clock; a reply for a prefix typed past is dropped
- ✅ Entering Review view syncs the changed files to a running server and spawns nothing
- ✅ Review view counts errors and warnings per changed file, with a total of each
- ✅ A file with no server, or no answer yet, reads as not measured
- ✅ No server configured: nothing spawned, and the editor behaves as today
- ✅ A server that fails to start, and one that exits, leave the Buffer unchanged

**R31.21** **A server that is not installed can be installed from the palette, and Varde composes
nothing.** The palette's second face lists the languages configuration names, one row each, carrying
the command that serves it and whether that command is on this machine. A key on a row **types that
language's configured install command into the terminal and does not run it** — the tree's actions
already work this way, and the reason transfers: an install has consequences on a machine Varde does
not own. The command is one more key in the `[lsp.<language>]` table, shipped as TOML data in
`DEFAULTS` exactly as the server names are, so it is overridable by a file, inspectable as a string
and extensible without a release. `docs/adr/0012-an-install-command-is-configuration.md` argues why
that is R31.1's rule rather than an exception to it, and records asking the AI session as the
rejected alternative and the natural fallback for the unpackaged long tail. *Amended by
`docs/adr/0018-the-global-config-is-the-list-of-programs.md`: the key **takes** the row — a row the
global config lacks is appended from the template with any `[facts.*]` row it names, and the install
**runs** in the shell pane as `<install>; echo $? > <sentinel>`, visibly, where a `sudo` prompt is
answered. A global config that does not parse refuses the key and nothing is written or run.*

**R31.22** **The command is per-OS, and a language with no command for this OS is a normal row.** The
key is a small table — `install.macos`, `install.linux`, `install.windows` — selected by the OS the
binary was built for, which is handed in on `Startup` the way `running_version` already is, so that a
Linux row is specifiable on a Mac. Several servers are genuinely not packaged anywhere, and inventing
a plausible command for them is worse than admitting the gap: the row says nothing is configured, and
one line of TOML fixes it for that machine and every future Varde on it.

**R31.23** **Whether a command is on this machine is the edge's fact, probed when the list is opened.**
A claim about the filesystem and this process's environment, so written by `tell_core` and never by
`update` — the same split R31.20 draws for whether a server is running. Re-probed on opening the list
rather than cached at startup, because the premise of the list is that what it describes is about to
change. **An install is observed, never believed:** nothing reads the terminal's output, which
ADR-0004 forbids anyway, and what changes Varde's behaviour is the probe finding the command on a
later pass. *Amended by ADR 0018: an install taken from Tools reports its exit status through a
sentinel the watcher sees, a non-zero status reads `install-failed` on its row, and a zero is a
re-check of that row. The program an install starts with, or the one after `sudo`, is probed with
the commands: a row whose command and package manager are both missing reads `needs-installer`,
names the package manager, and taking it writes nothing and runs nothing. A row may carry
`configures`, the keys its install makes true: once that install exits 0, each one
`~/.varde/config.toml` leaves blank or absent is written as the row spells it, `~` unexpanded, and a
key the reader set is never overwritten.*

**R31.24** **A command that appears is a reason to forget that it was missing, which is why there is
no restart.** A failed spawn writes a `Gone` conversation and `sync` skips any language that has one,
deliberately, so a second file of that language does not re-run a binary that is not there — which
also means the language is written off for the session. So the probe finding a command Varde holds a
`Gone` conversation for **drops the conversation**, and the next pass spawns it exactly as a fresh
start would. One case survives and it is real: an installer that appends its directory to a shell
profile is invisible to a process that inherited its environment at launch. That is the **only**
situation a restart answers, it is distinguishable from the outside — the user re-checks and the probe
still finds nothing — and the restart is then **offered, never taken**: a yes-or-no the user answers,
and declining leaves the workspace exactly as it was.

- ✅ The palette's second face lists what configuration names, with each row's command
- ✅ A project's own server shows in the row instead of the default
- ✅ A row says installed or missing, from the edge's probe
- ✅ Installing types the configured command for this OS into the terminal and runs nothing
- ✅ The same row on another OS offers that OS's command
- ✅ A shipped default install command works with no config file at all
- ✅ A global config overrides a shipped install command
- ✅ A language with no command for this OS says so and offers nothing
- ✅ A row already installed and answering refuses rather than offering again
- ✅ A server Varde watched die reads as `stopped`, not `installed`, and nothing is spawned to
  find that out
- ✅ A `stopped` row offers its install command rather than refusing as already installed, and
  offers nothing where this OS has no command
- ✅ A command that appears drops the `Gone` conversation and the server starts
- ✅ A command still missing is not asked for again
- ✅ A re-check that still finds nothing offers a restart; one that finds it does not
- ✅ Declining the restart changes nothing, and leaving the list installs nothing

**`installed` is a claim about the workspace, not about the filesystem.** R31.23 made it mean *the
command is on `PATH`*, which is a true statement about the filesystem and a false one about the
workspace: rustup installs a shim for every component whether or not the component is there, so
`~/.cargo/bin/rust-analyzer` exists, `which` is happy, the spawn succeeds, the child dies, and
R31.20's "the language server stopped" is the first the reader hears of it. So the row reads the
write-off the edge's own observation already left in the core — a `Gone` conversation is a **stopped**
row — and **nothing is spawned to decide what a row says**, since a probe that ran a language server
to see whether it works is a probe that starts a language server (R31.10). The two questions a reader
has, *is the command here* and *does it work*, are one enum and not two fields read together: a
conversation only exists for a language whose command could be spawned, so apart they would express
combinations nothing can be in. What is missing for this *workspace* is said ahead of the death it
explains — R31.27's row keeps its name for the fact it could not find — and `stopped` is what is left
when the command is here, has what it needs, and still does not answer. A stopped row **offers its
install command**: a command that is present and does not work is exactly the row an install would
fix, and refusing it as already installed is refusing the fix. Forgetting the write-off is unchanged
(R31.24): the command was never missing, so the probe finding it is no news, and the restart R31.24
offers stays the only way back for a session.

**R31.25** **A server that is installed works, and the reader types nothing.** R31.2 promised that "a
fresh install works with nothing configured", and its scenario proves only that a server is
*configured* — which is how three of the ten shipped languages came to name a command that cannot
serve a file on a normal machine. The promise is hereby the stronger one: **for every language
`DEFAULTS` names, installing the server is the whole of what the reader does.** No path to paste, no
flag to discover, no `initializationOptions` to research. That is a requirement about Varde, not about
the servers: where a server needs something to run, Varde either ships it as data (R31.1's rule, and
ADR 0012's) or the edge resolves it and tells the core (R31.23's split). A language that cannot meet
this yet is a language whose row says so — R31.22's honest blank generalised — and never a language
that reads as configured and answers nothing.

Three known failures define the work, each measured against the real server rather than inferred:
- **A default's `args` must be what the server actually needs.** `@vue/language-server` reads its
  TypeScript SDK from argv and dies on the first `didOpen` without it, which reaches the reader as
  R31.20's "the language server stopped".
- **A value that is a path on this machine is the edge's to find, not the reader's to type.** The
  TypeScript SDK lives in the workspace's own `node_modules`, so it is a fact about this workspace and
  belongs beside the `PATH` probe, not in a config file the reader maintains by hand. The same fact
  serves `typescript-language-server`, which resolves TypeScript itself and, on a machine whose global
  install is the 7.0 native preview, never answers `initialize` at all — it wants the SDK's
  `tsserver.js` in `initializationOptions.tsserver.path`, which is the same directory the fact already
  finds with one more path segment on the end, so both servers are served by one declared name and no
  second fact.
- **A server that asks the client a question Varde cannot answer is refused, and the refusal is data.**
  A server may ask the *client* to relay a request to a second server it does not itself hold, and
  waits for the answer — every feature `@vue/language-server` has, diagnostics included, is behind one
  such question. **Varde does not run the second server**, and ticket 19 records the measurement that
  settled it: on a fresh install the relay is worth nothing, because the companion needs a TypeScript
  plugin that installing either server does not bring, and without it a full relay and a plain refusal
  produce byte-for-byte the same answers. So what Varde does is refuse — out loud, which is the rule
  `queries::reply` sets and `received` already keeps for a server's *requests* — and the asker then
  answers with everything it can answer alone. The question arrives as a notification, so the protocol
  has no reply of its own for it and only the sender knows the method the answer comes back on: the
  pair of method names is therefore a `[lsp.<language>].unanswerable` key, configuration for the same
  reason a command is, since a branch naming either is R31.1's forbidden arm.

**A language that answers only part of it says which part, and the row is where it says so.** The
refusal above got `@vue/language-server` answering, and what it answers is template mistakes and no
type error at all — a third state between `installed` and `missing` that this rule had no word for,
and the same silence as "configured and answers nothing" in a smaller shape, since half of what a
reader opened the file for is absent with nothing on screen saying so. So `[lsp.<language>].partial`
names, in the reader's words, what the server still cannot do, and the row reads `partly-working` and
carries those words. **Declared, not observed**: nothing Varde can watch tells a server with less to
say from a file with less wrong in it, and a limitation is a fact about a server, so it is
configuration for exactly the reason a command is. It is behind `stopped` — a server that is not
running answers nothing, which is not "partly" — and behind `missing` and `missing-requirement`, which
are about whether it runs at all. The install key does nothing on such a row: the command is already
here, and what is missing is a second program or a configuration key, neither of which this binding
runs.

**And the shipped Vue row stopped needing the key**, which is the outcome the key was for rather than
a retreat from it. R31.28's multi-server row now puts the `.vue` file to a TypeScript server as well,
so the type errors arrive and `partly-working` would be a limitation that is no longer true — a row
that understates a server is the same kind of wrong sentence as one that overstates it. The mechanism
stays and is held by its own scenarios; what left is one row's claim about one server, because the
claim stopped being true.

- ✅ A language configuration says only partly works is neither `installed` nor `missing`
- ✅ What a partly-working language cannot do is named on its row
- ✅ A missing command is a missing command, whatever it would only partly do
- ✅ The shipped Vue row, with its SDK found, reads `installed`

**R31.26** **An initialization option is a config key, and Varde reads none of it.** `[lsp.<language>]`
carries an `initialization_options` table, passed verbatim as the `initializationOptions` of
`initialize`. R31.1's rule is the whole argument for why it is a key rather than a match on language —
a toolchain's layout in an arm is a server's name in an arm with more in it — and the three properties
ADR 0011 established are exactly the ones an option needs, so
`docs/adr/0012-an-install-command-is-configuration.md` records it beside the install command rather
than arguing it again. It is typed like the rest, so a wrong shape faults with a file and a line
(R9.5), and it is converted to JSON by a re-serialise rather than a walk deciding what each shape
means — **nothing in `src/` looks inside it**, which is the falsifiable form: grep for any option's own
key name and every hit is in `DEFAULTS` or a fixture, exactly as R31.1 already demands for commands.
A language with no options sends `null`, which is what the protocol has for a client that says
nothing. This is what makes R31.25's promise keepable for a server nobody here has run: a requirement
nobody anticipated is expressible in a file rather than in a release.

**Nothing ships whose right value is a path on somebody else's machine.** `typescript-language-server`
needs `initializationOptions.tsserver.path` and will not answer `initialize` without it, and
`@vue/language-server` needs its SDK directory in `args` — both are project-local paths, so no layer of
`DEFAULTS` may spell one. What ships instead is a *name* for the path, resolved by the edge, which is
R31.27. An invented path that does not exist is worse than none: a server failing on a
configured-looking value reads as Varde's bug rather than as a row to fix.

- ✅ Configured initialization options reach the `initialize` request verbatim
- ✅ A language with no initialization options sends `null`
- ✅ A project config overrides a global's initialization options
- ✅ A malformed `initialization_options` stops Varde naming a file and a line
- ✅ An option naming a fact the edge could not find stops the spawn (R31.27)
- ✅ A shipped default names the SDK the TypeScript server will not start without
- ✅ A workspace with no TypeScript of its own starts no TypeScript server

**R31.27** **A value that is a path on this machine is named in configuration and found by the edge.**
`DEFAULTS` and a user's config may write `${typescript_sdk}` wherever a string reaches a server — in
`args` and inside `initialization_options`, since the two servers that need the same SDK want it in
different places — and the edge resolves the name against *this* workspace before the message is
built. It is R31.23's split with an interpolation on the end: the edge looks, `tell_core` hands the
answers over, `update` reads them and never writes them. `@vue/language-server` is the measured case —
it reads its SDK from `--tsdk=`, and without it falls back to whatever `require('typescript')` finds,
which on a machine holding the 7.0 native preview is a package with no `typescript.js` in it and a
server that dies on the first `didOpen`.

- **The names are declared in configuration, not in the library**, as `[facts.<name>]` tables read
  the same way the `[lsp.*]` ones are and beaten by the same layers. A fact says which **marker** path
  means "this directory configures the language" and whether the answer handed over is the marker or
  the directory holding it; a machine-wide fallback is a command on `PATH` and the marker relative to
  the directory holding it once symlinks are resolved. Deliberately not a template language and not an
  environment-variable escape (ADR 0012's bargain): a **declared** name the edge could not find here
  starts no server at all, and a name **nothing declares** is left exactly as written, since a server
  whose own syntax uses `${...}` is nobody's placeholder to expand.
- **The edge implements exactly one search, and knows no ecosystem.** From the directory of the file
  being served up to the workspace root, nearest first, then the root, then the fallback — and never
  above the root, which would let a toolchain outside the workspace serve a file inside it. Which
  marker is looked for is data, so `node_modules/typescript/lib/typescript.js`, `.venv/bin/python` and
  `compile_commands.json` are one question asked three times rather than three arms. An arm per
  ecosystem is R31.1's forbidden arm with a package manager in it: grep `src/` for any of those paths
  and every hit is in `DEFAULTS`, a fixture or a comment.
- **From the file, because a monorepo is the ordinary shape.** Dependencies are installed per package,
  so the compiler that must typecheck a package's files is the one that package installed: resolving
  from the root alone found nothing in a real pnpm workspace whose root has no TypeScript, and a
  project pinning 5.6 must not be typechecked by a global 7.0. `State::workspace_facts` stays one map
  keyed by fact name — with buffers open in two packages the first answer wins, in path order — and
  keying it by language is the change a workspace whose packages genuinely disagree would force.
  Usable is still checked rather than assumed at every step, since it is the marker file that is
  looked for and not the package: the 7.0 preview is installed, on `PATH`, and has no `typescript.js`,
  which is the difference no version string reports.
- **A fact that cannot be found is a row that says so, and a server that does not start.** A command
  that is installed while a name its configuration interpolates resolves to nothing is not
  `installed`: it is one more state on the palette's row (R31.22's honest blank generalised), which is
  the one place the difference can be seen before a file is opened. And the row is now the *whole* of
  what happens — `sync` spawns nothing for that language, because a server launched without what its
  requirement was about dies on the first file and reaches the reader as R31.20's "the language server
  stopped", which is a crash to interpret in place of an honest row. **Nothing is written off by the
  skip**: nothing died, so there is no `Gone` conversation and no notice, and the pass that runs once
  the fact appears starts the server exactly as a fresh start would — the same shape R31.24 gives a
  command that appears. Substituting still drops the argument or the option key that carried an
  unresolvable name, because the edge re-probes on its own clock and a fact can vanish between the
  spawn and the handshake; that is the last line rather than the gate, and it is held by a unit test
  rather than a scenario.
- **A fact the server cannot start without and one it is merely better with are two facts**, and
  `[facts.<name>].optional` is the second. Required is the default and stays the rule above; optional
  changes the **gate only** — the key naming an unfound one is dropped exactly as it already would be,
  and the server starts. Not a weakening: it exists because the plugin that lets a TypeScript server
  answer about a `.vue` file is named on the `[lsp.typescript]` row *every* TypeScript project shares.
  Required there, a machine that never installed a Vue server would have no TypeScript server in any
  project — a requirement nobody declared, and machine-dependent, so it works for whoever tested it
  and silently takes the language away from whoever did not. A row whose optional fact went unfound
  reads `installed`, because nothing is missing.
- **A missing requirement offers its own install.** A `[facts.<name>]` table may carry
  `install.<os>` like a program row, and a `missing-requirement` row names the fact and, taken,
  runs the fact's install exactly as a server row runs the server's. The usual cause is
  machine-wide — a server on `PATH` with no classic `tsc` beside it — so the template's
  `typescript_sdk` carries `npm install -g typescript@6` on every OS — pinned, because from 7 the package is the native compiler with no `typescript.js`. A fact with no install for this
  OS offers none and still names what is missing, and one whose package manager is not on `PATH` is
  refused with `needs-installer` while the row goes on naming the fact. A re-check of the row asks
  after the fact's `command`, since that is what a restart could bring. The fact stays a search: nothing found is ever
  written into a config file (`docs/adr/0018-the-global-config-is-the-list-of-programs.md`).

  **Rejected: an option set conditional on the file being served.** The alternative was to send the
  plugin only when a `.vue` file is actually open — truer to the intent, and the option would then
  never reach a project that has no Vue in it. It is rejected because it is a bigger mechanism than
  the problem: `initialize` happens once per language, before the second file is opened, so the
  condition would have to either restart a server when a `.vue` file appears or make the options a
  function of the buffer set, and nothing else in this feature needs either. It also puts Varde back
  to *reading* an option table to decide when it applies, which is exactly what R31.26 promises it
  never does. One boolean on the fact, read only by the gate, keeps the option opaque.
- **A container below the top level that lost a part is dropped whole.** Per-key dropping is right for
  the options table itself — one option a fact could not fill must not take the rest of it — but
  `{"name": …, "location": "${unfound}"}` with the location gone is a plugin the server is told to
  load from nowhere, and an array quietly one entry shorter is a list whose length nobody configured.
  So the drop propagates upward from wherever the name was and stops at the top, where siblings
  survive.

- ✅ A shipped default names the SDK and the edge's answer reaches the spawn
- ✅ An SDK the edge could not find starts no server at all, and the row says why
- ✅ The SDK appearing starts the server on the next pass, with no restart
- ✅ A requirement named in an initialization option stops the spawn as an argument does
- ✅ An edge-resolved name reaches an initialization option too
- ✅ A project declares a fact of its own, and the edge's answer reaches the spawn
- ✅ A name nothing declares is left exactly as it was written
- ✅ A row installed but missing what it needs is neither `installed` nor `missing`

**R31.28** **One key is one question, put to everyone who serves the file.** `lsp::language` gave a
path exactly one language, which made a `.vue` file — served by the Vue server *and* by a TypeScript
one — unfixable in principle rather than in practice, and the same is true of a linter server beside
a type server and of every arrangement other editors reach by attaching several clients to one
buffer. So which servers serve a path is **data**: `[lsp.<language>].also_served_by` names the other
languages whose servers also serve this language's files, read from the file's own side so resolving
them is one lookup in the table the path already found. No arm names a language (R31.1), and the
document sync, the spawn pass and the ask all read the same list — a server asked about a file it was
never told about is a question it cannot answer.

- **The arbitration is core state and a pure decision over replies**, so it is held by unit tests
  with no process at all. One `Question` per `About` — the key that asked — holding the place, the id
  each server was asked under, which of them are still waiting, and whether anybody has answered.
  It replaces the per-conversation `asked` map, because both rules below are about the *set*.
- **The first non-empty answer wins**, and every later reply for that keystroke is dropped: a second
  jump would move the cursor off where the first one took it, and a second hover would replace a real
  answer with whichever server was slower — both read as the editor acting twice for one key.
- **The empty-handed notice waits for the last server.** Notifying on the first empty reply is the
  reported defect exactly: the server with no answer had less to look up, so it replies first, and
  letting it speak for the rest is `gd` saying nobody knows while somebody is still looking. A
  refusal counts as empty-handed for the same reason — a server that would not answer must not beat
  one that will.
- **`Ask::current`'s guard is unchanged** and still applies per reply: a cursor that moved abandons
  the whole question, since every reply for it fails the same test.
- **A won question is retired, not flagged.** A later reply then finds no question and is dropped by
  the guard that already drops a reply nobody asked for — and nothing is left waiting on a server
  that may never answer, which a `settled` flag would have left outstanding forever.
- **An error is remembered rather than reported on the spot**, so it is empty-handed for the
  arbitration without being lost: the empty hand says which kind of nothing it was, worst first —
  Varde's own refusal, then a server's error, then a server that simply knows nothing here. Each is
  true of strictly less than the one before it. Reporting a refusal as ignorance is an error
  surfacing as a domain answer, which the error-handling rule forbids.
- **A question whose last outstanding server dies is answered by nobody, which is an answer.** Its
  outstanding set is dropped in `lsp::gone` along with everything else that server was asked —
  R31.7's stale-version rule and R31.19's refuse-out-loud rule applied to a set instead of an id.
  Left in place it would wait forever: nothing is coming back to empty it. The loss is what the
  reader is told when it is news, since a server that died did not decline to answer.
- **`About::Candidates` is deliberately not shaped for.** Completion is asked for by typing rather
  than by a keystroke, so its empty hand is the list closing rather than a sentence; arbitration
  written over `About` accepts it, and nothing here merges two servers' lists.
- **And the multi-server default ships.** `[lsp.vue].also_served_by = ["typescript"]` is what puts
  the type errors in a `.vue` gutter, and it needed the two things it was once blocked on: a
  `State::diagnostics` keyed **per publisher** (R31.7), since two servers on one file used to clobber
  each other, and the plugin that lets the TypeScript server answer about a `.vue` file at all — a
  declared, **optional** fact (R31.27), because that plugin is named on the row every TypeScript
  project shares. Neither is Vue-shaped: grep `src/` and every hit is in `DEFAULTS`, a fixture or a
  comment. **Measured by running it** in the project the defect was reported from, with an empty
  `.varde/`: `gd` on `issuesFor` in a `.vue` file lands on
  `intervals.ts:40:17` — column 17 being the `i` of `export function issuesFor(`
  — `K` answers with the signature and the doc comment, and in a scratch workspace a type error
  (`4:7 Type 'number' is not assignable to type 'string'`) and a template mistake
  (`10:1 Element is missing end tag`) sit in the same gutter from two different servers. With no Vue
  server on `PATH` at all, a plain TypeScript project still starts its server and still publishes.
- **`needs-a-companion` stops firing for Vue and stays reachable.** The notice is conditioned on
  *every* server asked having refused a question of its own (R31.29), so a second server that answers
  ends it without a line changing. What still fires it is a server declaring `unanswerable` with no
  companion configured, which is the sentence that stopped blaming the wrong party.

- ✅ One keystroke asks every server that serves the file, for both keys
- ✅ The server with no answer does not answer for the one that has one
- ✅ Nobody knowing is said once, when the last server has answered
- ✅ A second answer for the same keystroke is dropped rather than acted on
- ✅ A server dying with the question outstanding leaves nothing waiting
- ✅ A fresh install serves a `.vue` file with the TypeScript server too, and tells it where the
  plugin is
- ✅ A machine with no Vue server still starts TypeScript for a TypeScript project

**R31.29** **A question Varde declined to relay says so, rather than reporting the server as knowing
nothing.** `no-definition` says *the language server knows of no definition* and `nothing-known-here`
says *the language server knows nothing about the symbol under the cursor*. When Varde refused the
question the server needed answered — R31.19's refuse-out-loud, which is what gets a Volar-shaped
server past a question nobody will answer — both sentences are false, and they are the sentences that
sent the reader looking at their own install instead of at Varde. One slug for both keys, because it
is one refusal, and R31.25's whole subject is a language that reads as working and answers nothing.

**The condition is a refusal that actually happened, never a configuration that permits one.** The
first attempt keyed it on `[lsp.<language>].unanswerable` being present, which blames a companion for
*every* empty hand in the file — the cursor on whitespace as much as on an unknown name — and makes
`no-definition` unreachable for such a server. That is a second false sentence exactly where the
first one was. **Measured on the wire** (`vue-language-server` behind a `tee`, driven from cold):

```
Varde -> textDocument/didOpen
server -> tsserver/request  [[1, "_vue:projectInfo", {"file": ".../App.vue"}]]
Varde -> tsserver/response  [[1, null]]            <- the refusal
Varde -> textDocument/definition
server -> id 2 result []                           <- and every answer after it
```

The question is asked **once, on `didOpen`**, is refused, and from then on the server answers `[]` to
every definition and hover in that file. So the refusal *precedes* the keystroke it explains, and a
window around the outstanding question — the second attempt — never sees it. It is therefore a fact
about the **conversation**, recorded where the refusal is made rather than inferred later, and a
server that could have relayed and did not is a server that knows nothing here.

- ✅ A file whose only server relays the question says so rather than blaming it
- ✅ A server that could have relayed and did not is a server that knows nothing

⛔ **A scenario that asserts a real server answered.** That needs the server installed on whatever
machine runs the suite, which is the fixture folder and the real terminal `AGENTS.md` keeps out of the
behaviour suite. What is scenariable is what Varde *sends* and what it *shipped*: that a fresh install
sends a Vue server the arguments it needs, that an edge-resolved path reaches the message, that a
relayed request goes where configuration said. Whether the server on the other end is happy is
verified by running it, like every other edge fact.

⛔ **A scenario per package manager, or per OS beyond the two that prove the selection.** The command
is data the row reads, so macOS and Linux prove the key is selected by the OS; a third asserts the
same mechanism with a different string, which is the coverage-chasing the test strategy forbids.
⛔ **Reading the terminal's output to learn whether the install worked.** ADR-0004 forbids reading what
a pane prints, and R31.23 makes it unnecessary: the probe is the answer, and it is a fact rather than
a claim.
⛔ **Pressing Enter for the user.** The unexecuted command is the review step, and it is what makes a
wrong default one word away from being right rather than a failure to diagnose.

**R31.30** **The Candidate list is a viewport onto the reply, not a rendering of it.** R31.14's three
promises were specified against replies a scenario hands the list — two or three items, at column
one, replaced wholesale by the next request — and a real server's reply is nothing like one. Each of
the three failures below is the same misjudgement, and each on its own leaves the list unusable.

**Ten rows, however many the reply carried.** `rust-analyzer` answers a bare prefix with several
hundred candidates, so a box sized from the reply was several hundred rows tall — and the renderer's
own clamp then pushed it to the top of the pane and drew it over every line including the one being
typed. R31.14's *does not cover the line being edited* was enforced by a scenario passing on a value
no server would ever produce. So the box's height is bounded and the selection scrolls inside it, off
a first-shown index held in the core and clamped in `update` exactly as `editor_scroll` and
`tree_scroll` are. Every item stays in the reply, because the narrowing below needs them all.

**Every number the box is placed by is the core's, and they travel as one.** The box's *row* was `Candidates::from`,
decided in `lsp`, unit tested and the subject of a scenario; its *column* was an expression in
`ui.rs`, decided nowhere and asserted by nothing — because the view model had no column to assert on.
It read the pane's origin plus the gutter, which is right for a hover, which describes a whole line,
and wrong for a list offering replacements for the word under the cursor. So the column sits beside
the row it belongs with: the cursor's, shifted left rather than drawn off the pane, and read by `ui`
exactly as `from` is. The renderer still keeps the box inside the screen — a box must not be drawn
outside its pane — but that clamp is a backstop rather than the thing deciding where the box goes.

**The width is the same decision twice over, which is why it went with the column.** Left in `ui.rs`,
it was measured over the rows the box *happened to be showing* while the core shifted the column by
the widest label in the whole list — so a three-hundred-candidate list was moved left by a width the
box was not drawn at, past the very edge the shift exists to stay inside. It is one `Placement` —
row, column, width, rows — measured in screen columns rather than characters, since that is what the
renderer lays a box out in and a wide script is wider than its character count.

**A reply is narrowed by the word it was asked about, from the moment it lands.** The word is not
empty when the list opens: `wor` went out with the request, so `wor` is what the reply is filtered and
ordered by. Left until the first keystroke, a reply of everything in scope would be shown whole for
exactly one keystroke — the unfiltered list this requirement is about, arriving slightly earlier. It
also makes a reply *nothing in which matches* the same empty hand as a reply with nothing in it at
all, which is R31.14's never-an-empty-box on one more road.

**A character typed narrows the list rather than closing it.** The original decision was that
filtering the reply again would be "a second opinion, and the one that cannot see what the server
matched on". The reasoning is sound and the premise is false: a server does not answer *the
completions for `wor`*, it answers with everything in scope at that position, marks the reply
`isIncomplete`, and expects the client to keep filtering and re-ranking — which is what
`filterText` and `sortText` are for, and Varde kept neither. The second opinion the comment refused
to give is the opinion the server asked for. So both fields are kept; the word being typed is
matched against `filterText` where a server gave one and the label otherwise, scored by
`src/filter.rs` — the same function the tree's filter box narrows with, because a completion list
that matches differently from every other list in Varde is a difference nobody could predict; a
backspace
widens it back to the whole reply, since nothing was discarded; and a word that matches nothing
closes the list, which is R31.14's promise of never showing an empty box reached by a shorter road.
The anchor is state rather than a re-read: where the word starts is found once, from the place the
request went out — by `Buffer::word_start`, which is the same function `Buffer::complete` replaces
from, because what a completion overwrites and what the list is narrowed by must be the same
characters. The debounce keeps doing its job underneath — a fresh request still goes out and still
replaces the list.

**Order is the server's where the server gave one, and Varde's ranking only where it did not.**
`sortText` is the field the protocol provides for a server to rank its own reply, so a reply carrying
it is left in that order and never re-ranked; only a reply that named no order is put closest-first.
And it must be *every* item or none: the protocol says `sortText` defaults to the label, so a mixed
reply sorted that way has the rest alphabetised — an order no server asked for, and the one this
decision exists to avoid.

- ✅ A reply of several hundred candidates leaves the editor's lines visible
- ✅ The choice scrolls the window rather than growing it
- ✅ The box sits at the column the cursor is in, and is shifted left rather than drawn off the edge
- ✅ A candidate the prefix does not match is not offered at all
- ✅ A reply nothing in which matches the prefix closes the list
- ✅ A character typed while the list is open narrows it, and the list stays open
- ✅ Backspace widens it again, up to the full reply
- ✅ Typing past every candidate closes the list
- ✅ The server's own order and its own filter text are what the list is ordered and narrowed by

⛔ **A scenario that the renderer honoured the row and the column.** That is the one thing a view
model cannot see, and it is exactly how the height defect hid: the core placed the box correctly and
the renderer overrode it. Verified by running Varde against a server answering three hundred
candidates and reading the frame back — the box below the line being typed, its left border in the
cursor's own column, and the lines around it still there.

**Q59 — is the restart Varde re-executing itself, or Varde exiting and telling the user? Resolved:
it exits.** Re-executing would inherit *this* process's environment — the very environment the
installer's new directory is missing from — so it would answer the one situation the question exists
for by not answering it. There is therefore no `Effect::Restart`: answering yes is `Event::Quit`,
which is also what holds the restart to the refusal quitting already makes on unsaved buffers. What
tells the user is the box that asks, since after the exit there is no screen left to tell anything
on. Settled by ticket 15.
❓**Q60 — does a row with no install command for this OS eventually ask the AI session instead?** ADR
0012 rejects the AI session as the *mechanism* but records it as the natural fallback for the servers
nobody has packaged, and the two compose cleanly. Not a scenario until the `install` keys have been
lived with — R31.22's honest blank row is the specified behaviour until then.

**R31.31** **A Hover is rendered, not quoted.** The box was specified against the replies a scenario
hands it — `fn main()`, one line, no markup — and every real server answers in markdown. Flattened
into one string, `#`, `**` and a fence reached the screen as the characters they are spelled with,
and a fence was drawn in one flat colour because the language it named was thrown away with the
comment "the box draws none". So the box holds the same `preview::Row`s a Preview does: the reader
that already turns markdown into styled, highlighted rows is the one that does it here, and `ui` maps
a piece to a span in one place for both. Three consequences follow from the box being sized by its
row count, which the wrapping rule already established:

**Plain text stays plain.** `MarkupKind::PlainText` is the server saying the `*` in its reply is an
asterisk. Rendering it anyway is Varde inventing formatting the server denied having, so that shape
keeps the hard wrap it always had — two paths, deliberately, and the only place in this feature where
the protocol's own distinction is honoured rather than flattened.

**A fence is cut, because a box does not slide.** `preview::rows` leaves a verbatim block unwrapped on
purpose — a line broken at the pane edge is a line the block's syntax does not permit — and a Preview
lets the reader slide sideways to reach the rest. A box has no sideways, and the renderer truncates
what does not fit in silence, so the cut is made in the core at the width the box was measured at.

**The box is capped, and says so.** A `rust-analyzer` doc comment renders to dozens of rows, which is
the same misjudgement R31.30 records for the Candidate list: a box taller than the pane is placed
where it fits worst and clipped without a word. The cap is a constant beside `WINDOW`, and the last
row is an ellipsis rather than nothing, because a box with no footer has nowhere else to say it was
cut. Declaring `textDocument.hover.contentFormat` is the promise that goes with all of it, made in the
same commit as the rendering exactly as `snippetSupport` was.

⛔ **A scenario per language.** The transport is language-agnostic by construction — the language
selects a config table, nothing more — so one Rust server and one TypeScript server prove the split.
Enumerating languages here is the coverage-chasing the test strategy forbids, and it is the same
answer R14.7 gives for the 220 syntaxes.
⛔ **The bindings and the cheatsheet.** Tickets 09, 10 and 11 each require a binding reachable without
a modifier and listed in the cheatsheet, and none of that is a scenario. It is already held harder
elsewhere: key handling is unit tested in `keys.rs`, and the existing `every_key` sweep drives every
key code in all sixty-four modifier combinations and fails any key that does something while being
neither listed in `CHEATSHEET` nor on the explicit omissions list. A scenario would re-prove wiring
the sweep already proves exhaustively, which the test strategy forbids.
⛔ **The Candidate list being one `Modal` variant.** R31.14 is enforced by the enum. A scenario
asserting it would be testing what the compiler proves.
⛔ **The edge.** The reader thread, the `Content-Length` framing and the spawn are verified by running
Varde on a folder and watching the server come up, exactly as `pty.rs` is. No scenario covers the
edge, by design.
⛔ **A server's log — that it is written, that it is truncated, that the screen is never painted on.**
The requirement is R31.20's last sentence and there is nothing in `src/` to assert about it: the file
is opened and the child's stderr is redirected in the edge, and the core may not so much as name the
path, since **nothing reads it back** — a core that parsed a server's log would be branching on which
server it is, R31.1's forbidden arm. Verified the way the spawn already is, by running Varde against a
server that logs on startup and one that dies on it, and by looking at the file.
⛔ **A test that runs a Language server.** The same discipline as never driving a real pty. Incoming
messages are canned JSON; a suite that needs `rust-analyzer` installed is a suite that is red on
somebody's machine for a reason that is not a defect.
⛔ **The rest of the protocol** — rename, code actions, find-references, workspace
symbols, inlay hints, signature help, call hierarchy, semantic tokens. Semantic
tokens specifically stays out because syntect is instant, offline and universal and remains the base
layer permanently. Whole-document formatting has come off this list: F32 specifies it, and it is
`documentFormattingProvider` beside the `documentOnTypeFormattingProvider` this feature already
speaks —
one `About`, five one-line arms, and no new transport. Range formatting stays out, because a
selection is a second question with a second staleness rule and nobody has asked for it.
⛔ **Varde composing an install command, updating a server, or falling back to a second server when
the configured one is missing.** The fallback stays forbidden for the reason the ADR gives — "tried
A, falling back to B" is a provider preference expressed in control flow. Installing is no longer on
this list: R31.21 below specifies it, and `docs/adr/0012-an-install-command-is-configuration.md`
argues why an install command shipped as TOML data pays neither cost R31.1 was protecting against.
What remains ⛔ is Varde *composing* one: a match on language and OS inside the library is that
forbidden arm with a package manager's name in it as well as a server's.
⛔ **Writing anything into the user's config file, on install, on update, or ever.** The commands ship
inside the binary as the bottom layer of the merge, which is the whole of "added automatically".
Writing them to disk would clobber deliberate edits, freeze what it wrote against every later Varde
version, and make a self-update into a migration. Argued in ADR 0012.
⛔ **Per-language configuration beyond `command` and `args`** — no initialization options, no
per-server settings blobs. Nothing has needed one yet, and the merge already handles the shape if
something does.

❓**Q57 — should a Definition outside the workspace root be openable read-only rather than refused?**
R31.13 decides it, so nothing is blocked; this records that the decision is the reversible one and
which way it reverses. Not a scenario until someone has wanted it.
❓**Q58 — how is a Diagnostic's range shown beyond its first line?** A diagnostic covers a range and
the gutter is per line; the scenarios mark the line a diagnostic starts on. Whether a multi-line
diagnostic marks every line it covers is unspecified and does not block ticket 08 — the mark is
derived from the range either way, so it is a rendering choice with one caller.

## F32 — Formatting a file — **DEFINED**

Scenarios live in `features/formatting.feature`, and the Language server half in the last Rule of
`features/language_intelligence.feature`. The decisions are F31's, reused rather than restated:
`docs/adr/0011-a-language-server-is-a-second-hosted-child.md` for where a tool's name may live, and
`docs/adr/0012-an-install-command-is-configuration.md` for the install command — amended by this
feature with the one thing it did not cover, which is Varde *executing* a workspace-configured
command against the user's source. `CONTEXT.md`'s "Knowing what the code means" section holds the
vocabulary.

**R32.1** **A Formatter is named in configuration**, in `[formatter.<language>]` tables carrying a
`command`, its `args`, an `install` per operating system and the `extensions` it claims — and **no
branch in `src/` names one**. The falsifiable form is the grep R31.1 and ADR 0012 already ask for,
now covering a third class of name: search `src/` for `prettier`, `black`, `rustfmt`, `gofmt` or any
package manager, and every hit is inside `startup::DEFAULTS`, a fixture or a comment. The shipped
rows are what "Rust supports basic formatting for HTML, CSS, JavaScript, JSON and YAML" means: they
are data in the bottom layer of F9's merge, so a project replaces one with a line of TOML, a reader
can print what Varde resolved, and a language nobody at Varde has heard of is served without a
release. The same three properties, for the same reason, as a server's command.
**R32.2** **`:format` asks the Language server first and the configured command second**, and the
order is the decision rather than an implementation detail: a server already holding this file's
syntax tree formats it for nothing, and it is the only party that can format a file with no external
tool at all. A server that declares no `documentFormattingProvider` **is not asked and nothing is
said** — it has not ended the question, it has handed it on, and a notice there would tell the reader
there is nobody to ask a moment before somebody answers. This is the one formatting question a key
was pressed for, so unlike the on-type half — which nobody pressed a key for — it **speaks when it
comes back empty**: `nothing-to-format`. **Both halves say it**, with one slug, because the reader
pressed one key and does not know which half answered: a command that hands back the text it was
given has changed nothing, and a key that changes nothing and says nothing reads as a broken key.
**R32.3** **Which language a file is, for the purpose of formatting, is two lookups and no second
table**: the `[formatter.*]` row whose own `extensions` claim it, then the name the file gives
itself — its extension, or, where it has none, the file name. The `[lsp.*]` rows are not asked: a
file's server and its formatter are separate choices, so every formatter row names its own
extensions (ADR 0018). A `Makefile` is
`[formatter.Makefile]`, which is a key a reader can write, and the last lookup *is* the map lookup so
writing it works; the empty extension it would otherwise fall back to names `[formatter.]`, a refusal
that asks somebody to write a key TOML will not take. The extension-as-a-name fallback is what makes a
`[formatter.<anything>]` a reader invents reachable with no code at all.
**R32.4** **The command sees the Buffer, never the file.** Its text goes in on the child's stdin and
what it writes on stdout replaces the Buffer. This is R31.6 in a second shape and it is not
negotiable here either: a tool told about the file on disk formats a file the reader is not looking
at. **Nothing is written** — a dirty Buffer stays dirty and `:w` is still the user's — which is the
load-bearing absence of this feature, and the one a plausible implementation breaks by reaching for
the in-place `--write` flag every one of these tools has. A formatter that can *only* rewrite a file
in place is therefore unsupported, and that is a stated cost rather than an oversight. The command is
split with `shlex` and executed directly, **never through `sh -c`**: it comes from the opened
folder's config, and AGENTS.md's security rule is that nothing from the workspace is interpolated
into a shell.
**R32.5** **The answer is applied as one undo step, hunk by hunk, and a stale one is dropped.** Old
against new is diffed with **zero context** and each hunk becomes one span for
`editor::Buffer::reformat`, which is the same function the Language server's edits already go
through: one undo step, one revision bump, and a cursor that rides the change. One span over the
whole file also "works" and parks the cursor on the last character, which is why it is named here as
the shortcut not taken. Context lines in a span would be text replaced with itself — an edit the
cursor has to ride for no reason, and an undo covering lines nothing touched — which is why this
diff is not `story::hunks`. A reply whose `revision` is not the Buffer's is dropped: R31.7's rule,
unchanged, for the same reason.
**R32.6** **A failure speaks, in the command's own words.** `formatter-failed` names the first line of
the child's stderr, because that is the line that says which line of the file to look at. Never
silent and never raw: the slug carries the tone and the wording, and the command's sentence is what
`Effect::NotifyAbout` puts beside it. Never in the command's own *bytes*, either — **control
characters are stripped from everything a notice says out loud**, in one constructor
(`Effect::notify_about`) that every built `NotifyAbout` goes through, which is R32.8's promise about
an install string in a second place and for the same reason: a formatter's stderr, a path a server
named and a file's own name are all text Varde did not write, and raw ANSI in text bound for the
status line can rewrite the screen. One funnel rather than four producers each remembering, because
the fifth notice is the one that forgets — and in the core rather than at the edge, because what a
notice says is the core's to decide and a test can see it there. That the terminal library happens to
drop a control character before it reaches a cell is not the promise being kept; it is the promise
being kept by somebody else.
**R32.7** **A language nothing configures refuses out loud, naming the key to write** —
`no-formatter-configured`, with `[formatter.<language>] in .varde/config.toml` as what the message
names. The library is the only party that knows which names a message may interpolate (R31.27), and a
refusal that says nothing is indistinguishable from a command that did nothing.
**R32.8** **A command that is not installed is typed into the terminal, never run.**
`Effect::SetTerminalInput` with the string configuration carried, exactly as `i` on a server row does
and for exactly ADR 0012's reason: an install has consequences on a machine Varde does not own, and
the person who owns it is sitting in front of the pane. **Focus goes with it** — the one rule at the
end of `update` that every injection reaches, not a second one here, because an install waiting on
the input line needs an Enter that lands in the same pane. So the `:format` that follows is a trip
back to the editor first, which is the gesture R32.9's scenario walks rather than assuming. Varde composes nothing, so **a row with no
install command for this operating system offers nothing** — it says the command is missing and
leaves the input line alone, which is `[lsp.c]`'s macOS gap in a second place. Held by a row whose
install command is for the *other* operating system, because a row with no install command at all
offers nothing whatever the lookup does: the scenario named for the OS key has to be one the key is
the only reason for, or it passes an implementation that never reads it. **Control characters
are stripped from the string on the way in**, because "typed, never run" is a promise about bytes: a
newline in an install value *is* the Enter `Effect::RunInTerminal` appends on purpose, so a two-line
value — from a hand-written config, or from a folder somebody else wrote — is executed rather than
offered, and raw ANSI in it can rewrite the screen besides. Stripped in the one rule every injection
already reaches rather than at either producer, which is what makes the server list's `i` (R31.22)
and this one the same promise. Stripped rather than refused: a command with a character taken out of
it is on the input line for the reader to fix, and refusing leaves the row that is wrong for this
machine with nothing to correct.
**R32.9** **Nothing is remembered about a command that was missing**, which is the whole of "format
the moment it is installed, with no restart". Whether the command exists is asked of the operating
system at the moment it matters — the spawn either finds it or does not, and a `NotFound` is the
fact only the edge can observe, reported as an event the way a failed server spawn already is. There
is no probe to invalidate and no cache to go stale, so the case R31.24 has to answer with a restart
question does not arise here at all. Held by **counting the runs**: a scenario that only asserts what
the buffer ends up holding cannot fail here, because an implementation that remembered the command
was missing would run nothing the second time and leave the *first* run standing — still naming this
path and this revision, so its answer still applies. The number of times the command was asked for is
the only thing the two implementations disagree about.
**R32.10** **The width the protocol is told is the project's**, not a constant: `editor.tab_width` is
what Tab lays down and what opening a block falls back to, so it is what a formatting request says
about this workspace's indentation. Measuring the *file* was tried and is worse than either, because
the shallowest indentation in a Java or C file is the single space of a block comment's ` * `
continuation. Most servers measured read none of it and format by their own configuration; the ones
that do now hear what the project said rather than a number that could disagree with it.
**R32.11** **A command that exits fine and prints nothing did not format the file empty.** Empty
stdout is refused with `formatter-failed`, naming the command, and the Buffer is left exactly as it
was. It is not a hypothetical: an in-place `--write` invocation is what R32.4 names as the shape
Varde does not support and the misconfiguration a hand-written row reaches first, and printing
nothing is precisely what it does. Read as an answer it is the whole file deleted with no notice at
all — worse than a refusal, and worse still with a Preview up, where `u` is refused outright and the
text has no visible way back. The Buffer holding only whitespace is the one case where nothing is a
true answer and is applied; held in a unit test rather than a scenario, because a Gherkin docstring
is dedented and a row of spaces arrives as a row of nothing.

**R32.12** **`:format` over a Preview crosses to Source and then formats.** A Preview is read-only to
the keys that edit and open to every `:` command (R26.9), and this is the first `:` command that
authors text rather than reading it or writing it out — so the asymmetry R26.9 left has to be decided
rather than inherited from whichever arm the event fell into. The three candidates were: refuse it,
which is the one answer R26.9 rules out and which leaves the reader unable to undo an edit `:format`
alone could make; edit the Preview, which is what shipped, and which strands R32.5's single undo step
behind a `read-only-preview` refusal on the very surface that made the edit, with no line numbers and
no Source cursor to show what moved; and cross first, which is `i`'s answer to the same intention
("let me change this", R26.9) and lands the reader where the undo, the line numbers and the cursor
all work. Crossing is what happens, through the same `cross_to_source` `i` uses so the two cannot
come apart, and it covers both halves of `:format` because both hang off one arm.
**R32.13** **`:format` with nothing open refuses out loud**, as `no-file-open` — the refusal
`:preview` already gives for the same emptiness (R26.2), reused rather than reworded, because the
reader who has met one has met the other. It is the one shape neither half of `:format` can answer,
and the argument is R32.2's: a key that changes nothing and says nothing reads as a broken key.
Decided in the arm rather than in `format::run`, which sees an immutable state and can only return
effects. `:w`, `:e` and `:q` are silent in the same state and are deliberately left that way here:
they are one lane's finding, and a fourth refusal is a decision about the whole router.

- ✅ A fresh install has a formatter for HTML, CSS, JavaScript, JSON, YAML and Rust
- ✅ The project's own row beats the shipped one, and an entry with no command stops Varde from starting
- ✅ The Buffer's text reaches the command on stdin, with `${file}` resolved in its arguments
- ✅ What the command answers replaces the Buffer, in one press of undo
- ✅ Formatting writes no file, and unsaved edits stay unsaved
- ✅ An answer about a Buffer the reader has typed on since is dropped
- ✅ A command that fails says so, in the command's own words
- ✅ A command that hands back what it was given says so, because a key was pressed
- ✅ A row's own extensions decide which formatter a file gets
- ✅ A language nothing configures refuses out loud, naming the key to write
- ✅ A file with no extension is named by the name it has
- ✅ A server that offers no formatting leaves it to the configured command, silently
- ✅ The install command is typed into the terminal and nothing is executed
- ✅ An install command carrying a newline is typed without it, and still nothing is executed
- ✅ A command that prints nothing says so, and the Buffer is unchanged
- ✅ A row with no install command for this OS offers nothing rather than inventing one
- ✅ The next `:format` after the command appears runs the command again, with no restart
- ✅ `:format` asks the server that declares `documentFormattingProvider`, and no command is run
- ✅ The server's edits move the lines it named, in one press of undo
- ✅ A server reply that arrives after the reader has typed on is dropped
- ✅ A server with nothing to change says so, because a key was pressed
- ✅ Formatting a Preview crosses to Source, where the undo it makes puts it back
- ✅ Formatting with nothing open refuses out loud

⛔ **Embedding a formatter as a Rust crate** — `malva` for CSS, `markup_fmt` for HTML, the `biome_*`
family for JavaScript, `taplo` for TOML. It is the shape that needs no install at all, and it is
rejected here for three reasons that compose: it is a large dependency surface, several of them are
0.x, and it would put formatting *policy* and tool names inside `src/` — which is R32.1 reversed, not
merely expensive. Configuration plus one well-known multi-language command already covers what the
ask names. **Reversible**: a bundled formatter would be one more `[formatter.<language>]` row whose
command happens to be Varde itself, and nothing above changes.
⛔ **`textDocument/rangeFormatting`, and formatting a selection.** A second question with a second
staleness rule — a selection can move while the reply is in flight, which is a third notion of stale
and the one F31 was told not to invent.
⛔ **Formatting on save.** `:w` is the one gesture in Varde that is exactly what it says, and a write
that silently rewrote the file would be the "completion that quietly changes what you wrote" failure
R31.15 refuses, one pane over. It is a config key away if anybody wants it.
⛔ **A formatter that rewrites the file in place.** Named in R32.4 as the cost of the stdin/stdout
rule rather than as an omission: supporting it would mean writing the Buffer to disk first, which is
the one thing this feature promises not to do.
⛔ **A scenario for the width the request names.** R32.10 is one number on one request and no
observable behaviour of its own, so it is held where it can be held falsifiably: a unit test in
`lsp.rs` sets `editor.tab_width` to a value that is *not* the default and reads the width back off
the request. Asserting it against a default four would pass against the constant it replaced, which
is the test that cannot fail this list exists to keep out.
⛔ **A formatter list in the palette, beside the server list.** Considered and refused: the server
list exists because a server is a long-lived process whose state a reader cannot otherwise see, and a
formatter has no state between two `:format`s. The refusals in R32.7 and R32.8 are the smaller answer.

## F33 — The Buffers pane — **DEFINED**

Scenarios live in `features/buffers_pane.feature`. F19 is what the pane is a second view of: the
buffers, their marks and the dot strip are all F19's, and this feature adds no fact about a buffer
that F19 did not already have. `CONTEXT.md`'s "Corner" and "Row selection" are the vocabulary.

**R33.1** The pane is **toggled from the palette**, one un-indented entry in the `Panes` group, keyed
`g`. `g` is Varde's own buffer gesture (`gt`/`gT` step buffers), so the palette's `g` opens the
list of what they step through.
**R33.2** **No new key binding and no cheatsheet obligation**, for R28.2's reason exactly: focus is
directional, so the pane is reachable by geometry, and a palette row is discoverable because the
palette lists it. F6's palette table gains the row in the same change, because that assertion is
whole-list equality.
**R33.3** The pane sits **in the Corner beneath the tree** — the slot R28.3 describes, at the tree's
width, taking those columns from the shell pane. **The Corner is one slot naming its occupant, not one
visibility flag per pane**: asking for this pane while the Risk list is showing replaces it, and
"both on screen at once" is not a state the slot can hold. Every occupant is the same rectangle, so
which pane is in the Corner changes what a click means and never where the Corner is.
**R33.4** **Toggling it on moves focus into it**; toggling it off returns focus to the tree; asking
for the occupant that is already there hides it. One rule for every occupant — a rule per pane is
where a precedence between them would have to be written down, and there is nothing to decide.
**R33.5** The **directional gestures reach it exactly as they reach the Risk list** (R28.5), both ways
round, because the geometry is the Corner's rather than any one occupant's.
**R33.6** **Visibility persists** in F9's per-user state file, under a key naming the Corner's
occupant. **A session saved by an older Varde still opens as it was left**: the legacy key naming the
Risk list's own visibility is read when the new one says nothing.
**R33.7** The pane lists **every open buffer, one row per file, in the order the dot strip draws
them** — one list, so the rows and the dots cannot come to be in different orders. A previewed file
is a buffer, so it has a row, and the next preview replaces it (R19's rule, not a second one).
**R33.8** A row carries **the same mark the editor's dot strip carries** — the same `Mark` and the
same glyph — then the path relative to the workspace root. The selected row's full path goes in the
border, for R28.10's reason: the pane is the tree's width and a path does not fit a row.
**R33.9** **The pane has no actions.** No row icons, no pane icons, and the selection stops at the
last row rather than stepping one past it: there is no actions slot below the list, and stepping onto
one would be arriving somewhere `Enter` does nothing.
**R33.10** `Enter` **and a click both show that buffer** — a switch, not a read. The buffer is
already held, so **nothing is opened from disk, nothing is written, and no effect is returned**;
both reach the same event the dot strip raises, so the two ways to a buffer cannot drift. `Enter`
follows into the editor because it is the deliberate "take me there", while **a click leaves the
keyboard in the pane it clicked** (R10.1) — the same exception the tree's and the Risk list's clicks
make.
**R33.11** The selection is a **Row selection** (`CONTEXT.md`): it names a buffer to switch to, so it
is never copied as characters and never becomes the selection. **A drag over the pane picks nothing**,
as R28.12 has it: the pane holds rows and no grid, so it hands back no span rather than one the edge
would fill with characters from the pane behind it.
**R33.12** **The row selection follows `current_buffer` whenever anything else moves it** — `gt`, a
dot, a tree click, a search hit, a close, `:ga`. A highlight disagreeing with the dot strip about
which buffer you are in reads as a bug in both. From there the arrows move it freely. **The buffer
set changing counts as "anything else"**, not only the current buffer changing: `:ga` while the
current buffer is dirty leaves it current and takes rows out from above it, which moves every index
below the one removed and would leave the highlight on a buffer nobody switched to.
**R33.13** **Scrolling is state clamped in the core**, as R28.13 has it, against the pane's own
offset: the two Corner panes share a rectangle and nothing else, so a shared offset would scroll one
by whatever the other was left at.
**R33.14** **An armed row action is let go of wherever focus moves.** An armed icon is an index into
the list of the pane that armed it, so carrying it across a focus change resolves it against a list
it does not belong to: nothing at all here, since R33.9 gives the pane no actions, which made `Enter`
dead on every press with no icon on screen to explain it — and the first entry of the Risk list's or
the Cursor history's actions in the other two occupants, which is worse than dead, since `Enter` then
asks for a refactor of a row nobody armed. Held where focus moves (the directional gesture of R33.5
and the toggle of R33.4) rather than in each pane's own motion arm: three of the four list arms let
go of it on a motion and the fourth did not, and a rule every pane has to remember is a rule the
next pane will be written without.

⛔ **The exact rectangles, and that the Corner's two occupants get the same one.** Pinned by the
layout unit tests, extended rather than duplicated — a scenario cannot see a rectangle.
⛔ **That a row's glyph is the dot strip's glyph.** Held by a renderer unit test that reads both and
compares them, rather than each being asked to agree with a constant. A scenario asserts the `Mark`,
which is state; the glyph is copy.
⛔ **A draggable width.** It takes the tree's, for R28's reason.
⛔ **Closing a buffer from the pane.** Considered and refused: `:bd` is the gesture, and a delete on a
list of unsaved work is the one place a mis-aimed row costs something. The pane has no actions.

## F34 — Cursor history — **DEFINED**

Scenarios live in `features/cursor_history.feature`. F33's Corner is the slot this is a third
occupant of, and R28.3's rectangle is the rectangle. `CONTEXT.md`'s "Visit", "Jump", "Cursor
history", "Motion" and "Row selection" are the vocabulary — a Visit is the fifth thing you go to,
beside a Motion's destination, a Match, a Hit and a Result row.

**R34.1** A **Visit** is one place the cursor has been: a file (workspace-relative), a line, a
column, and the whole line's text as it stood. The text is **carried, not looked up**, so a row
still says something about a file since edited or closed.
**R34.2** Only a **Jump** records a Visit, and the list of what a Jump is lives in **one place** —
never arm by arm. It is: opening a file, switching Buffer, an in-file search landing (`n`/`N` and
Enter on a query), and the ends of a file (`gg`, `G`). **Arrows and `j`/`k` are Motions and record
nothing, and a click in the editor is not a Jump either** — vim's jumplist records none of them, and
a list that did would be a keystroke log burying the four places worth returning to under four
hundred. Those absences are decisions, and they are stated in the module's own doc so they are not
"fixed" later.
**R34.3** What is recorded is the **place being left**, read off the state the event arrived at, so
going back returns you where you were. `G` reaching a read-only surface that ignores it records
nothing: **a Jump that went nowhere is not a place you have been.** An operator over a motion (`dG`)
is a delete, not a Jump.
**R34.3a** **A Jump into the file already on screen is a Jump.** A definition, a search hit or a
Risk row naming the file you are reading is the most common Jump there is, and the way back from it
is the whole feature — so "did this Jump go anywhere" is answered by **the place the landing carries**
and never by where the cursor stands when the landing arrives. That is why `Effect::OpenAt` comes
back as **one** event carrying the place (R34.3b) and not as an opening followed by a move: a landing
split in two is a landing neither half knows the whole of — the first still sees the place being
left and not where it is going, the second sees where it went and has forgotten where it came from.
A landing with no place at all, in the file already on screen, really did go nowhere.
**R34.3b** **One `Effect::OpenAt`, one landing event.** `Event::BufferOpened` carries
`at: Option<Place>` — `Some` for a file opened *at* a line, `None` for a file merely opened — and
the arm puts the cursor there. `Event::JumpTo` remains what it says: put the cursor here, which the
history's own walk uses to move within a file, and which is **not** a Jump (recording it would have
the history record every step of its own walk).
**R34.3c** **A landing the core makes in place is told, not inferred.** A definition in the file
already open moves the cursor and opens nothing (R31.12), so there is no landing event for R34.2's
list to see — the reply arrives as a language server's output. That one site tells the history the
place it is leaving, because `gd` onto a name in the file you are reading is the most common
back-jump an editor has and the alternative was returning `Effect::OpenAt` for a file already open,
which is the effect R31.12 exists to say it does not need. A definition the cursor already stands on
records nothing, by R34.3's rule.
**R34.3d** **A Jump the cursor has already left carries its own origin.** There is one: the in-file
search moves the cursor to the closest match on every keystroke of the query, so when `Enter` lands
the cursor already stands on the match and R34.3's "read it off the state the event arrived at"
records where you are rather than where you came from — which is nothing at all, leaving `/` the one
long move with no way back. The place the search was opened *from* is what is recorded, carried by
the Jump itself rather than read off a cursor that has moved on.
**R34.4** **The same place twice in a row is recorded once**, and the list is **capped**; the oldest
goes first. An unbounded history is a leak, and a pane cannot show one.
**R34.5** The list is a **log, not a tree**: a Jump made while travelling back appends rather than
truncating what was ahead, because the pane's job is to show where you have been and dropping rows
out from under it on every Jump is a pane nobody can read.
**R34.6** **Going back** steps the history's cursor towards the oldest Visit, **forward** towards the
newest, and **the place you were standing on when you first went back is itself recorded**, so
forward can return to it. Going back is not a one-way door.
**R34.6a** **The step back is chosen after the place standing here is recorded, not before.** A full
history drops its oldest Visit as that place goes on the end (R34.4), and every index below the one
dropped moves down with it — so a target chosen before the push names the row just pushed, which is
where the cursor already is. The symptom was a first `Ctrl+p` at a full history that moved nothing,
said nothing and silently dropped the oldest Visit, with the second press working: a key that reads
as flaky rather than as wrong.
**R34.6b** **R34.4 holds for the walk as well as for a Jump.** A Motion records nothing, so `k` back
onto the row a Jump recorded is enough to make the list already end with the place standing here.
Going back from there steps to the row **before** it rather than recording a second copy — a pane
showing the same row twice teaches nothing, and a step onto the row you are standing on is the
silent nothing R34.7 exists to forbid. With no row before it there is nowhere earlier to go, and it
refuses out loud like either end.
**R34.7** **Both ends refuse out loud** — `no-earlier-place`, `no-later-place`. A gesture that
silently does nothing is indistinguishable from a broken key.
**R34.7a** **A refusal is about an end of the list, never about the middle.** `:ga` closes every
Buffer and leaves the Visits standing, so there is no cursor to record and yet a whole list of places
to go back to. Going back then steps to the newest Visit with **nothing pushed** — forward has
nowhere to return to because there was nowhere to return *to*, which is the honest answer where
R34.6's recording has nothing to record. Refusing there refused in the *middle* of the list, while
the pane went on listing the rows and `Enter` on one went on opening it.
**R34.8** A Visit in the Buffer on screen moves the cursor directly; a Visit in another file is
**opened through the same effect a Risk row, a search hit and a definition all return**. One landing,
however the reader got there.
**R34.9** `Ctrl+p` and `Ctrl+n` are the bindings, and `gp`/`gn` the **modifier-free route** — a Jump
moves the cursor, so it is a Motion and owes one (R31.11). The same letters both ways, so the two
teach each other. **Neither is claimed from a hosted pane**: they are readline's own history keys,
and the reserved-key sweep is what holds that true. The results box keeps them for stepping hits file
by file, which is what puts the binding *behind* the box rather than in front of it.
**R34.10** The pane is **toggled from the palette**, one un-indented entry in the `Panes` group, keyed
`y` — the free letter in "history", since `p` and `n` are what the Jump itself is spelled with and a
palette letter repeating one would read as performing the Jump. **"Cursor history", two words**, where
every other entry is one: the user named it that, and "History" alone reads as shell history in a
workspace that hosts a shell. F6's palette table gains the row in the same change, because that
assertion is whole-list equality.
**R34.11** It sits **in the Corner** (R33.3): one slot naming its occupant, so asking for it while
another occupant shows is a replacement, every occupant is the same rectangle, and the directional
gestures reach it exactly as they reach the other two. **Visibility persists** and **the Visits do
not** — a Visit names a line of a file, and files move between sessions.
**R34.12** A row names **the file's name, the excerpt, then the line number** — the name and not the
path, for R28.10's reason, with the selected row's whole relative path on the border. The line goes
last before the icon, so a pane too narrow for both truncates the excerpt rather than where to go.
**R34.13** The **excerpt** is the word the cursor was on and a few more, then an ellipsis. It is the
**buffer's own text and never a Language server's answer**: "the definition the cursor was standing
on" is satisfied by the line it was on, and making a list of where you have been wait on a
conversation that may never answer would leave the pane blank exactly when a server is missing. The
rule is the core's, under a unit test — a renderer that decided how much of a line names it would be
a decision no test could see.
**R34.14** The excerpt is **coloured exactly as the editor colours that file**.
**R34.15** A row whose line **no longer holds what it recorded** still shows what it recorded and is
**dimmed rather than coloured as the code it no longer is** — the same thing `CONTEXT.md`'s Stale
does to a Risk figure. Only a file **still open** can be asked: nothing in the core reads the disk,
so a closed file is not claimed to be stale either way.
**R34.16** **One row action**, on the row the keyboard is on: go to that file and line. `Enter`,
the action and a click all reach the **same function**, so the three cannot drift. `Enter` follows
into the editor because it is the deliberate "take me there"; **a click leaves the keyboard in the
pane it clicked** (R10.1).
**R34.16a** **Being asked for a place nobody chose is its own refusal** — `no-place-here`, and not
R34.7's `no-earlier-place`. The history's cursor stands past the newest Visit until something has
been travelled to (R34.17), so `Enter` in the pane before travelling anywhere asks for a row that is
not there — and "no earlier place" answers a question about going back that nothing asked.
**R34.17** The **row the pane highlights is the history's cursor** — one field, so arrowing onto a
row and pressing `Enter` goes where the arrows left it, and the two can never disagree about where
you are. Its position **past the newest Visit** is where it sits before anything has been travelled
to, and **no row is highlighted then**: you are somewhere newer than everything recorded.
**R34.18** **Scrolling is state clamped in the core**, against the pane's own offset (R33.13).
**R34.19** **Browsing the tree is not a Jump.** Moving the tree selection over a file previews it
(R33.2's rule that the Buffers strip does not list a browse is the same distinction), and the reader
who scrolled past six files went to none of them. The *first* preview leaves the file being read,
which is worth coming back to; every one after it leaves the preview before it — a file seen for one
keypress, whose buffer the next preview has already discarded. What tells a browse from an open is
the flag the landing already carries, not a second event: `Enter` on the same row arrives as the same
event with it false. Recording browses is R34.2's keystroke log arriving by another door, and it
evicts real Visits through R34.4's cap while it does it.
**R34.11a** **The Cursor history's selection is a Row selection** (R28.12, R33.11): a drag over the
pane picks nothing and hands back **no span**, rather than one the edge fills in from whichever pane's
grid is behind it. Stated here because the pane inherited the guard when it joined the Corner and
inherited no scenario with it — and the shipped binary cannot show the difference, since it reads no
grid for a row pane, so only the library half is assertable and only a scenario can assert it.
**R34.11b** **The no-persistence promise is driven as a real round trip**: a Visit made, the
`SaveState` a session actually emitted, then a start against that saved state. The fixture it
replaced named only the Corner's occupant and so carried no Visits at all, which made "the history is
empty" an assertion no implementation could fail — including one that persists and restores the whole
list, which is how it was measured. `AGENTS.md`: a `Then` that cannot fail is worse than no `Then`.
**R34.20** **A Visit is a source line, even when it is taken from a Preview.** The cursor a Preview
reader moves is a rendered row (R26.8), and so is the origin `/` carries (R34.3d), while a Visit's
line is what `line_of`, the excerpt, Stale (R34.15) and every landing read as a source line. So the
history crosses the row map itself, in **one** place: recording a row in that field named a line
nobody was on, excerpted whatever the source line of the same number happened to hold — a blank one,
in the case that showed it — and then reported **not stale**, affirmatively claiming a junk row was
current. It also silenced the went-nowhere test (R34.3): `gg` and `G` in a Preview move only the row
cursor, so a Jump the reader watched happen recorded nothing at all and left them with no way back.
Column one, for the reason every crossing resets it (R26.8): the row map carries no source column.
**R34.20a** **One landing, and it is where the crossing into a Preview is made.** A Visit, a search
Hit, a Risk row, a definition and the crossing from Source all put the cursor on a *source line*, and
a markdown file opens previewing — so a landing that set the line directly moved an invisible second
cursor and left the caret on the top of the render, with the reader looking at a document that never
scrolled to what they asked for. One function answers all of them, which is what stops a sixth
landing from forgetting: while previewing it places the caret on the **row that line was rendered
from** (skipping the blank spacing row, exactly as R26.8's crossing does), and otherwise it is the
line and column it always was.
**R34.9a** **The modifier-free route works on a Preview too.** `gp` is answered ahead of R26.9's
read-only refusal, which claims `p` for the put: a Jump writes nothing, so being told the surface is
read-only answers a gesture nobody made — and `gn` worked all along, because `n` is not a key that
refusal claims. A route that works one way and not the other is worse than one that does not exist,
since the reader concludes the binding is imagined. See R26.8b for the narrowing this puts on the
refusal.
**R34.9b** **The cheatsheet claims each spelling only where that spelling answers.** `Ctrl+p`/`Ctrl+n`
are routed ahead of the views and work in Edit, Review and Story; `gp`/`gn` need a buffer to hold the
waiting `g`, which Review view's diff has none of, and a Story walk has already spent `g` on jumping
to a citation and `p` on stepping. So they are two rows, and the Jump is **modifier-only on those two
read-only surfaces** — a spec decision written down here rather than re-added to the table by the
next reader, because the keys it would need are already spoken for. A row's claim being met by any
one of its keys is right for single keys and wrong for a chord: a chord's first key does nothing
alone, so no unarranged state can excuse it, and a per-chord sweep is what holds this true.

⛔ **A Visit re-anchored to where its line moved to.** An edit above a Visit leaves it naming a line
that has shifted, and R34.15 only says so where the file is still open. Considered and deferred: it
needs the fuzzy re-anchoring already deferred on the story map, and inventing a mechanism here would
be a second answer to a question that has one owner.
⛔ **A closed file's row marked stale.** The core may not read the disk, so a Visit in a file nobody
has open is neither claimed current nor claimed stale. Q59.
⛔ **The excerpt from the Language server's own symbol name.** Recorded as a question, not as work
(Q60): an LSP-derived "definition the cursor was in" is richer, and it makes a list of where you have
been depend on a conversation that may never answer.
⛔ **Persisting the Visits.** They name lines of files that move between sessions, and a list of
stale places is worse than none.
⛔ **The exact rectangles, and that the Corner's three occupants get the same one.** Pinned by the
layout unit tests, extended rather than duplicated.
⛔ **That the row fills exactly the columns its icon is hit-tested from, and what the border says.**
Renderer unit tests, as the Risk pane's two are: a scenario cannot see a column, and the title is
the pane's only answer to *which file* a row is in.

## F35 — Reading aloud — **DEFINED**

Scenarios live in `features/reading.feature`. `CONTEXT.md`'s "Reading", "Utterance" and "Transport"
are the vocabulary, and "Selection" is the extent — a Reading is the sixth thing the Selection is
handed to, beside copying, yanking, a review comment, a story range and a site mark. The spec is
`.scratch/reading-aloud/spec.md`. Two decisions were hard enough to reverse that they became ADRs:
`0013-a-voice-is-an-installed-binary.md` and `0014-scratch-audio-lives-outside-the-workspace.md`.

**R35.1** A **Reading** covers the **Selection** and nothing else. There is no reading from the
cursor, no reading of a whole file, and no block fallback when nothing is selected: select, play,
and when it ends select the next passage. A Reading asked for with no Selection refuses out loud.
**R35.2** What is spoken is **stripped prose**. The Selection's source markdown is passed through
`preview::rows(text, 0)` and the rows' text is what the voice receives — so no `#`, no `*`, no link
URL, and a heading is read as the words it contains rather than announced as a heading. This is one
path, used whether the buffer is in Preview or Source, because a second path is a second place a `#`
can leak through.
**R35.2a** **No block kind is skipped, including code.** Skipping fences was specified and then
removed: a Reading that wanders into one is stopped by hand, and the arm that would prevent it is an
edge case earning less than it costs. This is a decision, not an oversight, and it is stated here so
it is not "fixed" later.
**R35.3** Reading applies to **markdown buffers only**, gated on the existing `preview::is_markdown`.
The **Transport** is drawn only there — absent elsewhere, never greyed.
**R35.4** A Reading is a sequence of **Utterances**, one per sentence, built as **one continuous
stream with real silence between them** — **550ms between sentences and 1000ms after the last one of
a block**, both chosen by ear on the prototype. A separate player per Utterance was built and
rejected by ear: it leaves a gap the machine chose (~150ms of process spawn) rather than one the
listener needs, and the result is staccato and hard to follow. The silence is part of the artifact,
not an accident of scheduling. A **block** is what `preview::rows` already separates — a paragraph,
a heading, a fence — **and each list item besides**, which nothing separates on screen but a reader
hears apart. The prototype inferred the longer gap from a sentence being short and sentence-final,
which was a guess forced by its own flattening regex; going through the Preview's rows there is no
guess to make.
**R35.5** Starting a Reading while one is in flight **replaces** it. Same shape as a Risk generation
superseding the answer still coming and the LSP abandoning an outstanding request; a queue would be a
fourth mechanism for the same idea.
**R35.6** **Previous and next move one Utterance**, by seeking to that Utterance's offset within the
stream. **Pause** stops the player and records the offset; **resume** rewrites the stream from that
offset and plays it. Pausing by signalling the player was built and rejected: freezing a process
while its audio device drains underruns the buffer and clicks.
**R35.7** **Speed is a multiplier and higher is faster.** The synthesizer's own parameter scales
*duration* and therefore runs backwards; the value the user sets and the value stored are the
multiplier, and the reciprocal is what the command receives. A speed change applies to the **next**
Reading, because pace is baked in at synthesis and re-pacing a stream in flight would repeat words
just heard.
**R35.8** The **Transport** sits right-aligned on the editor's top border — play/pause, previous,
next, stop, and the current speed — the way the Risk pane's action icons sit on its, truncating the
title beside it rather than being covered by it. It is clickable, **and every action it offers also
has a key binding**, because a control reachable only by mouse is one the cheatsheet cannot promise
and `AGENTS.md` forbids. Its width is measured with `unicode-width`, never by counting characters:
the obvious media glyphs report as narrow and are rendered emoji-wide by most terminal fonts, which
overflows the border and wraps the bar off it. **Play with no Reading in flight starts one over the
Selection** — R35.1's "select, play" is the gesture, and a control that does nothing until `:read`
has been typed is a control the reader presses twice and then stops believing; a play with nothing
selected still refuses out loud, through the same `reading::start`.
**R35.9** Every missing piece **refuses out loud** — no voice on disk, no synthesizer on `PATH`,
no player — naming which one, and opening Tools on the speech row that installs it, so one key takes
it exactly as it would in Tools (ADR 0018). Nothing is typed onto the terminal's input line. A
`voice` naming a file that is not there is `no-voice`, the same as a blank one. Silence is not an
acceptable failure here, because silence is also what success sounds like before the first word.
**R35.10** **Nothing appears in the workspace when Varde speaks.** The stream is written outside the
workspace root, the file tree is unchanged, `git status` is unchanged, and a project search finds
nothing new. It is deleted when the player exits and again when Varde exits.
**R35.11** The synthesizer is a **resident child**, started when the first markdown buffer opens
rather than on the first keypress, because loading a voice costs about as long as the budget for the
whole gesture. It is a third hosted child in the sense ADR 0011 means.
**R35.12** The Utterance being spoken is **marked**, subtly — a gutter marker and a dim background,
never an inversion. The mark follows elapsed time against the Utterance offsets, so it tracks the
voice rather than the last keypress.

**Q61** Is the dim background the right weight, or is the gutter marker alone enough? Refinement;
no scenario turns on it.
**Q62** Does the Transport need a stop distinct from pause, given that selecting elsewhere and
playing supersedes anyway (R35.5)?

## F36 — The minimap — **DEFINED**

Scenarios live in `features/minimap.feature`. `CONTEXT.md`'s "Minimap", "Slider" and "Thumb" are the
vocabulary. The spec is the request itself: VS Code's minimap, which the user asked for by name and
by screenshot.

**R36.1** The **Minimap** is a mirror of the file, not a widget of its own: it takes columns out of
the editor pane's own rectangle and is hit-tested inside it, the way the gutter is. There is no
fourth `Pane` and no second rectangle — `minimap::strip` is the one answer the renderer draws and the
mouse measures, for the reason `layout::GUTTER` is one answer.
**R36.2** A terminal's smallest mark is one cell, so the scale is **two source lines to a row** —
the halves of an upper-half block, foreground the line above and background the line below — and
**four source columns to a cell**, inked or blank. Past `WIDTH * COLUMNS` characters a line is not
mirrored: the mirror is for the shape of a file, and a strip wide enough for a long line is a strip
taking the columns the line is read in.
**R36.3** The mirror **scrolls itself**, in proportion, so the window the editor is showing is
always inside it — exactly, not approximately: at the end of the file the mirror is at its end too.
Its offset is **derived from `editor_scroll`** and never stored beside it, because a second scroll
field is a second author for one question.
**R36.4** A press or a drag in the strip **travels the file**, and the caret comes with the view the
way the wheel already takes it. What travels is the **row**, never the line drawn on it: the mirror
moves as the window does, so a rule that re-read the line under the pointer had the file running away
from a pointer standing still. The slider comes to rest under the row held, which makes a press and
a drag one rule rather than two.
**R36.5** A drag is decided by **where the button went down**: a travel that began in the strip stays
a travel however far the pointer wanders, and a selection that wanders into the strip stays a
selection. A press in the strip picks no text — the load-bearing absence, since those columns belong
to the editor pane.
**R36.6** The **Slider** is a line down the strip's first column, beside the rows in view, and
**not** a field behind them: a mirror this faint is mostly blank, so a wash behind it draws a grey
block in every cell the code does not fill and becomes the loudest thing on the pane. The **Thumb**
is the same line in the pane's last column, drawn only where there is no mirror to read the position
off — one indicator at a time, since two alike lines meaning different things (the window in the
mirror, the window in the file) is a difference nobody can read — and absent while the whole file is
on screen, because a bar spanning the edge says there is somewhere to scroll to.
**R36.6a** The Slider is **lit while the pointer is on the mirror** and quiet otherwise: a line
nobody is reaching for only has to be findable, and under the hand about to drag it is the one
moment it has something to say. One fact covers the gesture too — a drag arrives as drag reports
rather than moves, so the pointer that pressed the strip is still recorded as being on it for as
long as it is held, and a wheel over the strip is a wheel under a pointer already there. A wheel
over the *text* moves the Slider without lighting it: lighting it there would mean a light that has
to go out by itself, which is a clock and a tick, and ADR 0009 bounds those to work in flight.
**R36.7** The mirror is shown over a **file's own lines** only. A diff, a Preview's reflowed rows and
a walked Site are surfaces whose rows are not lines, and a mirror of lines shown against rows is the
second meaning for one shape this repo refuses elsewhere.
**R36.8** It is a **reading preference**: `editor.minimap` is where a project starts, `:minimap` is
the switch from inside, and what it was left at is remembered — the same shape `:dim` has. Its
columns come out of the clamp's count, so turning it off gives them back to the text.
**R36.9** **No room, no mirror.** Twelve columns are worth spending beside code and not instead of
it: a pane with fewer than twenty columns of text left does without, which is what a stock tree
divider on a hundred-column terminal comes to.

## F37 — Change marks — **DEFINED**

Scenarios live in `features/change_marks.feature`. `CONTEXT.md`'s "Change bar" is the vocabulary.
The spec is VS Code's gutter indicator, asked for by screenshot.

**R37.1** A line the buffer holds that **the last commit does not** carries a bar in the gutter. An
inserted line and an edited one are the same answer — the `+` side of a diff — and which lines is
what the scenarios assert; the glyph and the colour are the edge's. The diff itself is
`authorship::traced`, shared with F40's Authorship since R40.2a: the same lines the border says nobody
has committed are the lines the gutter bars.
**R37.2** The mark answers **the buffer as it is**, saved or not: a line typed a second ago is a
change. The other side is what `HEAD` holds, **told by the edge** on the git poll and never
remembered by the core, keyed by the buffer's own path — a file the commit has no copy of is told as
`None`, which is how an untracked file carries no bar rather than a bar down every line. A buffer
opened since the last poll is asked about at once rather than two seconds later.
**R37.3** The gutter's one column holds one mark: a diagnostic first, the voice's place second, the
change bar last, because the change is already visible on the line.
**R37.4** Deleted lines leave **no mark**. VS Code draws a triangle where lines went; nobody has asked
for it, and a line that is not there has no gutter row to mark.

**⛔ Not scenarios, deliberately.** Four of VS Code's settings have no terminal meaning and are not
implemented: `side` (the mirror is a mirror of the pane's right edge; a left one would be a second
layout for one feature), `scale` and `renderCharacters` (a cell is the smallest mark there is — the
block shapes *are* the render, and R36.2 is the only scale a terminal has), `autohide` and
`showSlider: mouseover` (a terminal has no hover state to hide behind, and Varde's one hover
affordance is a dwell that opens a box). `enabled` is R36.8.

## F38 — Terminal splits — **DEFINED**

Scenarios live in `features/terminal_splits.feature`. The spec is VS Code's *Split Terminal*, with
`terminal.integrated.splitCwd: inherited`.

**R38.1** `:split` starts a shell **beside the one with the keyboard**, side by side in the terminal
strip, and the new one takes the keyboard. Splitting the middle of three puts the new shell right
after it, not at the end.
**R38.2** The new shell starts **in the folder the split shell is in now** — where it has `cd`-ed to,
asked of the OS at the moment of the split — and in the workspace root only when the OS will not say.
**R38.3** How many shells the strip holds is **told by the edge**, never counted by the core: a split
whose shell failed to start leaves the keyboard where it was, and a split whose shell exited is gone,
the keyboard falling back to the last one that remains. The last shell exiting is still how Varde
ends.
**R38.4** `Alt+h`/`Alt+l` step **through the splits before leaving the strip**, and stop at the ends
rather than wrapping. A click in a split moves the keyboard to it. Every other gesture — keys, paste,
a drag to select, the wheel — is about the split with the keyboard.
**R38.5** A command Varde **pushes** at the terminal — a tree action, an install line — goes to a
shell **whose prompt is waiting**, and the keyboard with it: the focused split if it is idle, else the
first idle one. Never to a split running a foreground job, where it would be the job's input. Which
is which is **told by the edge**, from the pty's foreground process group. With every split busy a
new shell is split off beside the focused one and the command **waits for its prompt**, the way a
review waits for the AI CLI's — and only that shell's prompt releases it.

**⛔ Not scenarios, deliberately.** No stacked (vertical) splits and no drag to resize one: the strip
divides its columns evenly. No kill command: `exit` in the shell closes its split, which is what a
shell already does.

## F39 — Binary releases — **DEFINED**

Scenarios extend `features/self_update.feature`. The spec is `.scratch/binary-releases/spec.md`; the
decision hard to reverse is `docs/adr/0017-a-binary-install-updates-itself-from-a-release.md`.
`CONTEXT.md`'s "Staying up to date" holds Install kind, Release, Asset and Relaunch.
R39.1–R39.4 are scenarios; R39.5 and R39.6 are unit tests in `src/startup.rs`.

**R39.1** The **Install kind** is decided at startup from where the binary is: a known checkout is a
checkout install and keeps F-self-update exactly — manifest comparison, no network. Anything else
is a binary install and asks for this repository's latest Release **once**, never on a timer.
**R39.2** **The edge fetches, the core decides.** The body comes back unread; the core parses it,
compares its Version by the same `semver` ordering as the manifest, and picks the Asset named
`varde-<os>-<arch>`. Only a strictly newer Version with an Asset and a checksum list names the
Update in the version tag and is remembered.
**R39.3** A failed request, a body that does not parse, a Version that is not newer and a Release
with no Asset for this platform are all **silent**: no Update in the tag, no notice. The edge logs a failed
request.
**R39.4** `:update` and palette `u` on a binary install download the Asset, verify it against the
checksum list, replace the binary at its resolved path and **Relaunch** — refused with
`unsaved-changes` exactly as quitting is, and a second `:update` after that refusal relaunches
without fetching. Each failed step is its own notice.
**R39.5** `varde --deps` prints the dependency table one line per program, with no folder and no
terminal, so `install.sh` asks the binary rather than reading the source. The table is the rows
`~/.varde/config.toml` names, or the template's when there is no file (ADR 0018).
**R39.6** The four Asset names are pinned by a unit test that names `.github/workflows/release.yml`,
and the workflow names the test.

## F40 — Authorship — **DEFINED**

Scenarios live in `features/authorship.feature`. `CONTEXT.md`'s "Authorship" is the vocabulary — not
"blame", which is git's command and names a whole file. The spec is GitHub issue #33 ("Add git blame
feature") and the answers on it; the Change bar (F37) is the precedent every edge case here is
settled against.

**R40.1** The editor's **top border** says who last committed the line the cursor is on and the
**authored** date, `YYYY-MM-DD`. Authored rather than committed is what "who wrote this, when" means,
and it survives a rebase; the format does not vary by locale. Always on, and no key toggles it — a
toggle needs a binding, a Cheatsheet row and a sweep entry for a feature nobody has asked to hide.
No commit hash: a hash is worth showing once there is something to do with it.
**R40.2** It is the **committed** file's answer, so a buffer line is traced back through the diff
before it names anybody — a line inserted above the cursor would otherwise hand the cursor's line the
author of the line above it. A line the working tree has changed, and every line of a file the commit
has no copy of, are **not committed yet**.
**R40.2a** **One diff, not two.** `authorship::traced` is the only derivation of which committed line
a buffer line came from, and R37.1's Change bar now bars exactly the lines it answers `None` for —
`story::added_lines` was a second diff of the same two strings and is deleted. Two derivations would
be two answers to which lines the commit holds, and the disagreement is a gutter bar beside a line the
border credits to somebody.
**R40.3** **Told by the edge, cached against the commit.** Only a new commit can change what it
answers, so the key is the file and `HEAD` and never `Buffer::revision`: typing starts no walk of a
file's history. A buffer opened since the last poll is asked about at once, on the same poll as F37's
committed text.
**R40.4** **Silence outside a repository, and without git.** A border reporting "not a git
repository" on every file of a folder that is not one is noise — R37.2's answer to the same question,
where a file the commit has no copy of carries no bar.
**R40.5** **Edit View only.** Review and Story both put their own content in the editor's title, and
this must not disturb either. A Preview is not a View — it is one Buffer's way of being drawn inside
Edit (ADR 0007) — and a Preview row carries the source line it came from, so the Authorship is that
line's.
**R40.6** **The name gives, the date is kept whole.** A border with no room for both cuts the author
with an ellipsis: the date is ten columns whatever the commit, and a date cut short names the wrong
day. Pinned in a unit test in `src/ui.rs`, as is the exact wording; a border with no room for the
date at all says nothing rather than half of one.

**⛔ Not scenarios, deliberately.** Blame for a Selection or a range (this is the cursor's line);
opening, showing or diffing the commit the Authorship names; Authorship in a hosted pane, the file tree or
a Corner occupant; a setting for the date format or to turn it off; any per-line gutter presentation
— the gutter's one column is already spoken for by the Change bar, diagnostics and the voice's place
(R37.3).

**Still open.** Two costs, both measured against what F37 already paid and neither felt yet.
The `blame_file` walk is synchronous at the edge, so the first read of a very large file with a long
history is paid on the main loop; cached per commit it happens once, but off the loop through a
`Sender` — the way an analysis and a format already are — is the shape if it is ever felt. And
`traced` runs a patch per draw, which is what `changed_lines` already did before this feature and is
now the same single one; memoising it against `Buffer::revision` is the move if a frame ever shows it.

## F41–F50 — Debugging a running program — **DEFINED**

The spec is GitHub issue #45 and the Gherkin ticket is #46. `CONTEXT.md`'s "Debugging a running
program" section is the vocabulary, with Strip, Shell group, Group tab, Transport and Chip beside it.
Two decisions were hard enough to reverse that they became ADRs:
`0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md` and `0022-every-action-has-a-chip.md`.

**One seam for all ten.** Every scenario drives `update` from the World, with a **scripted Debug
adapter** standing in for a real one: it answers requests with canned replies and sends the events a
scenario names, the way the canned Language server does for F31. Scenarios assert which DAP requests
were sent and in what order, and the view model — never a pty, a port, a glyph or a colour. Sizes that
are *state*, such as the Strip's height, the divider's column and the Evaluator's rectangle, are
asserted in scenarios, the way the tree divider is. What ratatui makes of them is a unit test in
`layout`, `mouse` or `keys`, as #45's Testing Decisions say: the 9-column gutter, Chip widths, the
pinned rectangles, and hit-tests on Chips and Group tabs. DAP framing is a unit test in `debug`.

**⛔ Not scenarios, deliberately:** glyphs and theme colours; the Chip strings' exact cells; a real
adapter, pty or TCP port; DAP's `Content-Length` framing. Out of scope per #45: a Scratch runner,
compiling code into a paused Rust process, right-click menus, session pickers, data breakpoints,
memory, disassembly, reverse stepping, browser targets, and any per-language path.

### F41 — Starting and ending a Debug session

Scenarios live in `features/debug_session.feature`.

**R41.1** A **Run mark** stands beside what a `[run.<name>]` row's tree-sitter `query` matches, on
the line of its `@run` capture, in the files its `extensions` claim. The row also carries `run`, a
command template, and `debug`, a Launch configuration (R41.3) — each `${…}` filled from the query
capture of that name, and `${file}` with the file's path, quoted in `run`. Rows are named freely
because a language has more than one kind of thing to start. They ship in the built-in defaults, so a
global config written before them still has them. A language with no row has no Run marks, and one
with a grammar and no shipped row gets them from a row alone. The grammars are the ones
`rust-code-analysis` already builds; a row claiming an extension none of them parses, or whose query
does not compile, is refused at start (docs/stack.md). A row with no `debug` dims the Debug Chip.
**R41.2** Clicking a Run mark, or `␣x` on its line, offers `run` and `debug` as two Chips keyed
`r` and `d`. A Breakpoint on the line is drawn over the mark and takes the click, so it can be taken
away; `␣x` on a line with no mark refuses as `no-run-mark`. **Run** goes to a shell whose prompt is
waiting, exactly as a tree action does (R38.5), takes the keyboard there, and never starts a
session. **Debug** starts a session for exactly the marked function, and restart reruns it.
**R41.3** A **Launch configuration** is a `[launch.<name>]` table with `adapter`, `request`
(`launch` or `attach`), `args` and optionally `reattach`. It is allowed in both config layers, and a
project entry beats a global one of the same name — R9's precedence, applied by name. `args` reach the
adapter as opaque JSON. The launch palette lists both layers.
**R41.3a** `c` in the launch palette opens a form that writes one. Its fields are the name, the
adapter chosen from the configured `[dap.*]` rows, the request, the arguments that row lists for
that request — `launch_args` or `attach_args`, each a key, a one-line explanation and whether it is
required — and the target, `project` by default or `global`, switchable in the form and `global`
only in a Bare workspace. A row listing no arguments offers free key/value pairs instead, so no
branch in Varde names an adapter (R41.2). Confirming writes `[launch.<name>]` with `adapter`,
`request` and `args` into the chosen file through `toml_edit`, keeping its comments and layout and
creating a missing project file; an argument whose text reads as a TOML number, boolean, list or
table is written as one, and anything else as a string. A name the target file already holds
refuses as `launch-name-taken`, and a required field left empty refuses as `launch-field-needed`
naming the field; neither writes anything. On success the palette shows the new entry selected and
does not start it.
**R41.4** Startup follows the protocol: initialize → launch or attach → wait for `initialized` →
`setBreakpoints` per file → `setExceptionBreakpoints` → `configurationDone`. Nothing after launch is
sent before `initialized`.
**R41.5** **Restart** is `Ctrl+F5` (JetBrains' Rerun), `␣r` and the `restart` Chip, and it reruns
the last session's configuration. With no earlier session it refuses as `no-last-session`.
**R41.6** Stopping an **attach** session sends `disconnect` with `terminateDebuggee` false, and
stopping a **launch** session sends it true. The program going away ends a launch session. An
attach session goes **Waiting** instead, unless its configuration says `reattach = false`. Waiting
attaches again when the edge reports the port answers, and sends Breakpoints again. The port is the
`port` in the configuration's `args`, on its `hostName` or `localhost`; an attach naming no port has
nothing to watch, so it ends with its program as a launch does. A re-attach the adapter refuses is
reported as `launch-failed` and goes on Waiting: a program on its way up can take the connection
before it will take a debugger. Only stopping
ends a Waiting session. The Transport says `waiting`, and every stepping Chip is dimmed.
**R41.7** A `startDebugging` reverse request adds the child's threads to the one session. There is
never a picker, and stopping stops every child too.
**R41.8** Every failure is told once, by state: `no-debug-adapter` for an adapter that is not
configured or not on PATH, `debug-adapter-failed` when the edge could not spawn it, and
`launch-failed` when the adapter rejects the launch, and `no-language-server` when the language
server hosting the adapter has not finished starting. The edge tells the core the adapter exists
(ADR 0021), so a failed spawn is never a session.
**R41.9** Each `[dap.<language>]` row is a Tools row with `install.<os>` keys (ADR 0012, ADR 0018).
A row whose `args` name `${port}` is a server: the edge fills in a free port, spawns it and connects
over TCP.
**R41.10** A row naming a `server` is an adapter that language server hosts. Its `plugin` table is
merged into that server's `initializationOptions` when it starts (tables key by key, lists added
to), and starting a session sends its `command` to the server as `workspace/executeCommand`. The
integer the server answers with is the port the edge connects to, and a re-attach asks again. A
refusal from the server is `launch-failed` and ends the session. Its Tools row is as installed as
the server's command. The Java row ships this way, with java-debug a `[facts.*]` row that installs
the plugin.

### F42 — Breakpoints

Scenarios live in `features/breakpoints.feature`.

**R42.1** A click in the Breakpoint column toggles a Breakpoint. A click on the line numbers does
not. `␣b` toggles one on the cursor's line with or without a session, and `Ctrl+F8` (JetBrains) does
the same while a session exists. A change mid-session re-sends that file's list at once, and a file
emptied of Breakpoints is sent an empty list.
**R42.2** Breakpoints move with their line, and a Breakpoint whose line is deleted goes with it.
**R42.3** They persist in `.varde/state.json` with the line's text. A Bare workspace keeps them in
the Sidecar (ADR 0016), so the folder gets nothing. At load, a Breakpoint whose line no longer holds
its text is **Stale**. It is marked `stale` in the Breakpoint list, never re-pointed, and **never
sent** to the adapter, because sending it would pause on a line the user never chose. A Bare
workspace's Breakpoints are gone the next time Varde starts in that folder.
**R42.4** What the adapter answers is kept for the session only. An unverified Breakpoint is drawn
`unverified`, with the adapter's message on hover. One bound elsewhere is drawn at the bound line
for the session and listed at its own. Neither changes what is persisted.
**R42.5** Condition, hit count and log message are handed over as written. The gutter glyph kinds
are `plain`, `conditional` (condition or hit count), `logpoint` and `unverified`. The suspend scope
defaults to `thread` and switches to `all` in the Breakpoint box.
**R42.6** The **Breakpoint box** opens from the `✎` Chip on a Breakpoint or with `␣B`, the
Breakpoint key's shifted letter. Enter keeps what was written, and Escape discards it.
**R42.7** The **Breakpoint list** is a Corner occupant with or without a session. It is sorted by
path, then line. Enter goes to a row's line, the row's `remove` Chip removes that Breakpoint, and
the Transport's `clear-all` Chip removes them all.
**R42.8** **Exception filters** are exactly those the adapter reports, shown as switches. A switch
mid-session is sent at once. Choices persist per project, per adapter, and are sent at the next
start. Naming one exception class is offered only where the adapter reports exception-options
support; elsewhere its Chip is dimmed, not hidden (R46.3).

### F43 — Being Paused

Scenarios live in `features/paused.feature`.

**R43.1** The Paused line is marked in the gutter and highlighted across the editor's full width. A pause in a file
that is not open opens it. The line is drawn `exception` for an exception pause, which also puts the
exception first in the Variables, and `paused` otherwise.
**R43.2** Every change into Paused sets the Strip to the Debug group and the Corner to the Frames,
and **moves no focus**, not even out of a shell the switch has just hidden. The keys keep reaching
the pane that had them, because a pause is something the program did, not the user. What the user
then shows instead holds until the next pause.
**R43.3** Ending the session restores what the Strip and the Corner held before it began, including
an empty Corner. The `Debug` Group tab goes with the session.
**R43.4** Frames are grouped by thread. A run of Library frames (source outside the workspace root,
or the adapter's `deemphasize` hint) folds into one dimmed row that counts them, and Enter on that
row unfolds it.
**R43.5** Choosing a Frame moves the Paused line, the Variables (a `scopes` request), the Inline
values and the Evaluator's Frame.
**R43.6** Another thread pausing moves nothing. It is flagged in the Frames and counted on the
`next-thread` Chip, which jumps to it and is dimmed at zero. Continue resumes the inspected thread
alone (`singleThread`).
**R43.7** While Running, the last Frames, Variables and Inline values stay on screen, dimmed. The
Variables title says `running`, and no Paused line is marked.

### F44 — Stepping and the Transport

Scenarios live in `features/stepping.feature`.

**R44.1** `F8` next, `F7` stepIn, `Shift+F8` stepOut, `F9` continue (pause while Running) and
`Ctrl+F2` stop. They are Reserved keys from any pane, the Program output included, **only while a
session exists**. With none, a hosted pane receives them. Stepping while Running sends nothing.
**R44.2** The Space chords are `n` `i` `o` `c` `q` `r` `b` `B` `e` `a` `s` `h`, in normal mode and
in the debug panes. In insert mode Space is a space. With no session the Chord hint offers only
`b`, `B` and `r`. The **debug panes** are the Variables, the Frames, the Breakpoint list and the
Evaluator in normal mode. The Program output is a hosted pty and not one of them: Space reaches the
program there, and only the F-keys are reserved. The Evaluator adds `m` (move) and `z` (resize) while it has focus (R49.2).
**R44.3** A tapped Space opens the **Chord hint** at once, with no timer. Every entry is clickable.
The second key or Escape takes it down, and Escape does nothing else.
**R44.4** A chord leaves **Stepping mode** on. There, `n` `i` `o` `c` act without Space, and any
other key turns it off and is then routed as always, so `j` still moves and `/` still opens the
search. The Variables title says `stepping`, and the session ending turns it off.
**R44.5** The Variables' Transport holds the Chips `continue` (which becomes `pause` while
Running), `step-over`, `step-into`, `step-out`, `stop`, `restart`, `ask-ai` and `next-thread`, and
`show-output` while the Program output is hidden (R45.4). Each Chip names its F-key and its chord.
Dimmed and lit come from state: the last Chip used, by key or click, stays lit until the next
action, however much time passes.
**R44.6** Short of room, every Chip sheds its keys at once, and none is cut or wrapped. The widths
are pinned in a `layout` unit test. A `ui` unit test pins that every Chip's colour is one of the
theme's named colours and never a fixed RGB value, since no scenario can see a colour.
**R44.7** The Reading's Transport is restyled as Chips: `play`, `previous`, `next`, `stop`, `speed`.
**R44.8** The Chord hint and the cheatsheet rows come from one list beside the router. The F-keys
are listed only while a session exists, and every hint entry is a cheatsheet row. The key sweep
covers every new key or records its omission.

### F45 — The Strip and the Debug group

Scenarios live in `features/strip.feature`.

**R45.1** The border above the Strip is dragged with or without a session and clamped so that
neither the Strip nor the area above it goes below its least height, which is re-clamped on resize.
The height is remembered per project, like the tree divider.
**R45.2** The Group tabs are `Shells`, plus `Debug` while a session exists, and the tab for the
group showing is lit. A click on a tab, or `␣s`, switches groups. A click moves the keyboard into
the group it shows, as a click in a split does (R38.4). Shells hidden by the switch keep running,
and a `:split` always goes to the Shell group.
**R45.3** The border between the Variables and the Program output is dragged and clamped.
**R45.4** `␣h` hides the Program output and gives the Variables the whole width. The `show-output`
Chip shows it again. Output that arrives while it is out of sight, hidden or behind the Shell
group, marks the `Debug` Group tab and the Chip until it is shown. A hidden Program output keeps its size,
its pty is never stopped, and it is never resized below two rows by one column.
**R45.5** `runInTerminal` starts the Program output's pty inside the Debug group, never a shell.

### F46 — Variables and Watches

Scenarios live in `features/variables.feature`.

**R46.1** The Variables are grouped by the adapter's scopes and opened one level at a time by
reference, and a closed row costs no request. Indexed collections are paged **100 at a time**.
`private`, `readOnly` and `lazy` hints decide how a row is drawn, and a lazy member is fetched only
when opened.
**R46.2** The focused row carries the Chips `set-value`, `copy`, `watch`, `evaluate` and `ask-ai`,
and no other row carries any. Set value is sent as written in the program's language and replaced
by the adapter's answer. A refusal is `set-value-failed`. Copy takes the value, and copy as an
expression takes the path to it.
**R46.3** An action the adapter cannot do is **dimmed, never hidden**.
**R46.4** Watches are the top rows and are re-evaluated at every pause in the `watch` context. One
that contains a call, found from the syntax tree, is marked as calling. A Watch that fails shows
the adapter's error. Its row's `remove-watch` Chip removes it, and Watches outlive the session.

### F47 — Inline values

Scenarios live in `features/inline_values.feature`.

**R47.1** Inline values are drawn only on lines of the Paused call **before** the Paused line, never
on the Paused line, after it or in another file. While Running they stay, dimmed, and they go when
the session ends. Choosing a Frame draws that call's values.
**R47.2** A value that changed since the last pause, including one that has just appeared, is
highlighted for that pause only.
**R47.3** Values are trimmed to the width the line leaves, and a line that fills the width carries
none.

### F48 — A Hover while Paused, and every Hover

Scenarios live in `features/hover_while_paused.feature`, and the changes for every Hover extend
`features/hover_dwell.feature`.

**R48.1** While Paused, a Hover asks the adapter in the `hover` context for the expression the
syntax tree names under the pointer. `order.total` is asked for as a whole, never as `total`. The
value comes first, opens like the Variables, and its expression is highlighted in the editor. The
adapter is asked nothing while Running or with no session.
**R48.2** **A Hover never calls anything.** An expression containing a call, from the pointer or
from `K`, sends no `evaluate` request. The box says `needs-evaluate` and offers the `evaluate` Chip,
which opens the Evaluator without running it. The `watch` Chip adds the expression as a Watch.
**R48.3** For every Hover, not only while debugging, the box stands while the pointer is on the
symbol or on the box itself, and goes when the pointer leaves both. The wheel over it scrolls the
box and not the editor. A second `K` moves the keyboard into it, and Escape gives the keyboard back
and takes the box down. This amends F31's "goes when the pointer leaves".

### F49 — The Evaluator

Scenarios live in `features/evaluator.feature`.

**R49.1** It opens from `␣e` on the expression under the cursor or on the Selection, from a
Variables row, or from a Hover, and opening it evaluates nothing. It floats, so the editor behind it
stays editable, and it stays open while stepping.
**R49.2** Its rectangle is state, changed only in `update`. It opens centred the first time and
after that where `.varde/state.json` last recorded it, clamped wholly onto the screen then and on
every resize, and kept off the Paused line. The title bar drags it, the corners and borders resize
it, and the border under the Snippet is a divider. Without a modifier, `␣m` then `h` `j` `k` `l`
moves it and `␣z` then `h` `j` `k` `l` resizes it, one cell per key.
**R49.3** The Snippet is ordinary Vim editing. Normal-mode Enter runs the Selection, or else the
whole Snippet, with the `repl` context and the chosen Frame's id. Enter while inserting is a
newline, and `Ctrl+Enter` and the `run` Chip run it from either mode.
**R49.4** The Evaluator output shows what was printed while the Snippet ran, then the value, which
opens like the Variables. Each run replaces it. The adapter's errors are shown as given. A running
Snippet says so and can be cancelled where the adapter supports `cancel`. The `run` Chip is dimmed
while Running, and Enter then sends nothing.
**R49.5** Snippets that were run persist per project. `Up` and `Down` recall them only while the
Snippet is empty, and otherwise move the cursor as always. `Escape` in normal mode, once nothing
is left for it to cancel, `:q` and the `close` Chip close the Evaluator — `:q` never the file behind
it — and so
does the session ending; each keeps its Snippet in the history. Copying is the ordinary Selection.

### F50 — Asking the AI about a Paused program

Scenarios live in `features/pause_snapshot.feature`.

**R50.1** `␣a` and the `ask-ai` Chip paste a **Pause snapshot** into the AI's prompt, and
**nothing is submitted**. The snapshot holds the Paused line with two neighbours on each side, the
Frames, the Variables as shown (so closed rows cost no request), and the exception.
**R50.2** It goes the review hand-off's way (R8): bracketed when the CLI asked for that, and held
until the CLI is ready. With no AI session, the configured one is started first, as submitting a
review does. Focus does not move.
**R50.3** A Variables row's `ask-ai` Chip pastes that one value, still unsubmitted. The `ask-ai`
Chip is dimmed while Running.

**Still open, none blocking:**
**Q63** Should Watches persist across restarts per project, as Snippets do? R46.4 keeps them only
until Varde exits, and no scenario depends on the answer.
**Q64** Is 100 the right page size (R46.1)? This one is a refinement.
**Q65** Should `c` in Stepping mode leave Stepping mode on, given that a continue usually ends a
burst? It is on for now (R44.4), for one rule with no exceptions.

## F51 — Keeping knowledge — **DEFINED**

Scenarios live in `features/knowledge.feature`, with the wikilink rule in
`features/markdown_preview.feature` and the moved AI Inject Chip in `features/ai_pane.feature`.
`CONTEXT.md`'s "Keeping knowledge" section is the vocabulary: Vault, Note, Topic, Kind, Skill,
Source map, Knowledge view. Three decisions were hard enough to reverse that they became ADRs:
`0025-the-vault-is-told-never-found.md`, `0026-a-shipped-skill-belongs-to-varde.md` and
`0027-an-ai-facing-chip-carries-a-word.md`. It is one feature, specified whole, to be split into
tickets vertically afterwards.

**R51.1** `[knowledge] enabled` (default `false`) and `vault` (default `~/.varde/knowledge`, `~`
expanded) are read from the **global** config only; the project layer drops `[knowledge]` the way it
drops `[ai]`. Both are Settings in ADR 0018's sense, so the template is unchanged: the dimmed
palette entry's notice names the key.
**R51.2** A missing **default** Vault is created on first use. A missing **configured** Vault is
refused out loud and never created — a typo must not leave a stray folder.
**R51.3** A **Skill** is a folder under `~/.varde/ai/skills/` holding an Agent Skills `SKILL.md`.
The Skills modal lists every one, by the frontmatter's `name` and `description`; a folder without
readable frontmatter is listed dimmed with its reason, never silently skipped. A Skill directly
under `skills/` is **Global** and listed first; one under `skills/workflows/<workflow>/` is listed
under its **Workflow**, ordered by `metadata: { varde-step: <n> }`, the order a user runs them in.
A folder with no `SKILL.md` — a workflow's `shared/` or `agents/` — is not a Skill. It is opened by
`:skills`, palette `j`, and the `◆ Skills` Chip on the AI pane's top border.
**R51.4** Picking a Skill **pastes one line and submits it**: the path of its `SKILL.md` and the
workspace root — never the Skill's text, and nothing Varde reads back (ADR 0006). The prompt goes
through the same queue every prompt does: a session is started if none runs, and the line waits
until it is ready.
**R51.5** A Skill whose frontmatter carries `metadata: { varde-vault: true }` is also told the
Vault's and the Source map's paths, and is hidden while the knowledge base is disabled. No other
Skill, no environment variable and no other prompt names the Vault (ADR 0025).
**R51.6** A Skill whose frontmatter carries `metadata: { varde-asks: "<label>" }` opens a one-line
question box before it is handed over; the answer goes in the pasted line. An empty answer or
Escape hands nothing over.
**R51.7** Varde ships one Workflow, `knowledge-vault`: the Skills `init-vault`, `update-knowledge`,
`search-knowledge`, `sweep-knowledge` and `maintain-vault`, the agent `agents/knowledge-searcher.md`
and the files they share in `shared/`. Every one of its Skills opts into the Vault;
`search-knowledge` and `maintain-vault` ask a question first. The agent is never picked by the user
— the Skills tell the session to run it as a sub-agent where the harness has them, and to follow it
inline where it does not.
**R51.8** The binary carries every shipped file. **When any differs on disk, Varde deletes
`skills/workflows/` and the folders earlier releases shipped, and writes the workflows afresh** — a
hand edit or an added file inside a shipped workflow is lost. A Global Skill is never touched
(ADR 0026, ADR 0028).
**R51.9** The **Knowledge view** is a fourth View (palette `w`, `:knowledge`). It points the tree,
Filter, Find and editor at the Vault; the terminal, the AI pane, git and language servers stay with
the workspace. Toggling off returns to the View it was entered from, with the workspace's tree,
buffers, cursor and scroll as they were. It is not remembered across a restart. The file watcher
follows the Vault while it is shown, so a Note a Skill writes appears live.
**R51.10** In the Knowledge view Notes **open in the Preview and are never edited**: insert mode,
every editing key, Replace, saving and the tree's file actions are refused with
`vault-is-read-only`, and the tree's edit Chips are not drawn. Moving, selecting, yanking, Find,
Filter, the Preview toggle, a Reading, AI Inject and Skills all work (ADR 0025).
**R51.11** Varde runs **no git** in the Vault and draws no Change bars on a Note. A Vault under git
is the user's own; the session may push it if the user asks it to.
**R51.12** **`[[wikilinks]]` are followed everywhere**, not only in the Knowledge view: resolved
Obsidian's way against the root the tree shows — `Name.md` anywhere below it, `[[Folder/Name]]` to
disambiguate, `[[Name|alias]]` opening `Name`. An ambiguous name is refused with `ambiguous-link`
rather than resolved by order; a name matching nothing is `no-such-note` and creates nothing.
**R51.13** `▶ AI Inject` moves to the top borders of the **Editor and Terminal** — the panes it
injects from — with its word; the AI pane carries `◆ Skills` instead. Both glyphs are geometric
(ADR 0027). `:inject` is unchanged.

**What the shipped Skills say** is prose, not a scenario: Varde never reads what a session writes.
It is written generically — the Vault's shape is the user's, grown from their content, not this
spec's examples — and covers:

- `update-knowledge` reads the Vault, the session and the workspace; proposes the **key takeaways**
  as a short bullet list (the Notes written may be fuller); then a bullet list of Notes to **add,
  merge into or update**, including Notes the new knowledge invalidates; then asks. The user's
  changes revise all three; acceptance writes.
- It places Notes in a **Topic** and a **Kind**, deriving both from the existing Vault, and says
  plainly when either would be **new**; on an empty Vault it proposes from the first write. The user
  names every folder in the end. The same knowledge may be written once per Kind it fits, in that
  Kind's style.
- Every Note carries frontmatter — `kind`, `topic`, `audience: product | engineering`, `source`,
  `updated`, `supersedes`. Product and engineering knowledge are separate Notes, linked. A Note the
  new knowledge contradicts is rewritten in place with a History heading, not left stale.
- Its last writes are a **link pass** over the Vault, adding `[[wikilinks]]` between related Notes,
  and an index Note and an Obsidian `.base` per Topic.
- **Nothing names this machine**: a scripted check refuses home paths, the user name, the host name
  and absolute paths before anything is written. Code is named by the Source map's name for it;
  remote URLs and repository names are allowed. It offers to add the workspace to the Source map.
- Last, it asks whether to **sweep** the Source map: compare engineering Notes with the code they
  describe and propose updates where they have diverged.
- With nothing in the session worth capturing it asks what to do with the knowledge base instead.
- `search-knowledge` hands the user's question to `knowledge-searcher`, which may follow the Source
  map into code, and reports back citing Note names rather than pasting Notes whole.

**Q66** ✅ **Resolved: `gd`.** Following a link is the gesture that already goes to a definition:
the jump-modifier click asks for it, and `gd` is that click's modifier-free key. With the cursor
on a `[[wikilink]]` in a markdown buffer, in the Preview or in source, both follow the link instead
of asking a language server; anywhere else they ask as before. No new key to learn, and the
cheatsheet lists it as `gd` "definition / follow link".
**Q67** Should the Skills modal give each row a letter, the way the palette does, or is Enter on a
selected row enough? Refinement; no scenario turns on it.

## Spec status

Every feature is defined in Gherkin — **1707 scenario headings across 65 feature files** — and no
blocking questions remain. What is still open is listed per feature above, and every remaining item is
a refinement that does not change an existing scenario: Q28, Q31, Q34, Q35, Q36, Q37, Q39, Q40,
Q41, Q42, Q53, Q54, Q55, Q56, Q57, Q58, Q59, Q60, Q63, Q64, Q65.

Per `AGENTS.md`, implementation begins now that the spec is complete — not before.

**F22–F25 (stories and walkthroughs) are specified and not implemented.** Their route was charted on
`.scratch/story-walkthrough/map.md`, and the eleven tickets under it hold the measurements behind
every answered question above — three real authoring runs, 21 prior-art sources, and a 30-commit
coverage census. Two decisions were hard enough to reverse that they became ADRs:
`0005-a-story-dies-with-its-range.md` and `0006-stories-arrive-as-an-artifact.md`.

**F26 (markdown preview) is specified and not implemented.** One decision was hard enough to
reverse that it became an ADR: `0007-a-preview-row-is-not-a-line.md`. Two crates were chosen against
alternatives and are named in `docs/stack.md`.

**F27–F30 (Risk and the Refactor loop) are specified and not implemented.** The spec is
`.scratch/risk-and-refactor-loop/spec.md` and the thirteen tickets under it. Two decisions were hard
enough to reverse that they became ADRs: `0009-a-spinner-is-bounded-by-its-job.md` (the one deliberate
exception to draw-only-when-something-changed, bounded by construction) and
`0010-varde-owns-the-test-gate.md` (why Varde runs the tests rather than believing the session — a
consequence of `0004-hosted-panes-are-transparent.md`, which forbids reading what a pane prints).
`CONTEXT.md`'s "Paying down risk" section is the vocabulary every scenario uses.

**F41–F50 (debugging a running program) are specified and not implemented.** The spec is issue #45, and its
implementation tickets are sub-issues of it. The two ADRs are
`0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md` and `0022-every-action-has-a-chip.md`.

**F51 (keeping knowledge) is implemented**, as GitHub spec #150 split vertically into tickets
#151–#161. The three ADRs are `0025-the-vault-is-told-never-found.md`,
`0026-a-shipped-skill-belongs-to-varde.md` and `0027-an-ai-facing-chip-carries-a-word.md`.

**F31 (language intelligence) is specified and not implemented.** The spec is
`.scratch/richer-editor/spec.md` and the twelve tickets under it — half one is F14's colour, half two
is F31. One decision was hard enough to reverse that it became an ADR:
`0011-a-language-server-is-a-second-hosted-child.md` (why ADR-0004's no-provider-names rule extends to
a Language server, why TOML defaults are not that naming, and which facts about a server the core may
hold given the failure `ai_running` demonstrated), amended by
`0012-an-install-command-is-configuration.md` (why an install command is one more TOML key rather than
a match on language and OS, why it is typed into the terminal and never run, why nothing is ever
written to the user's config file, and why that deletes the restart in every case but one). `CONTEXT.md`'s "Knowing what the code means"
section is the vocabulary every scenario uses. One of its scenarios passes on arrival — R31.3's
malformed-entry case is R9.5 unchanged, and it is written so that breaking R9.5 breaks it.

**Deliberately deferred, with reasons on the map:** diagrams and animation (the spine is already a
diagram; nobody knows yet whether the missing picture is a call graph or a state chart), a live nudge
oracle (blocked on knowing how often pre-written text falls short), fuzzy re-anchoring (needs a crate
`git2` cannot supply), walking a story authored on another machine, whether the five-step attention
limit is a hard authoring constraint or a hint, and a richer values strip.

## How to resume

**The test suite is the progress tracker. Nothing else is.** Run it first, before reading anything
else:

```
cargo test --test cucumber
```

Cucumber reports every scenario as **undefined** (no step definitions yet), **pending**, **failed**,
or **passed**, grouped by feature file. That output *is* the state of the work — it cannot drift,
go stale, or disagree with the code, which any hand-maintained checklist eventually does.

- `74 undefined` → nothing implemented yet
- a feature file absent from the undefined list → that feature is done
- `failed` → in progress, or a regression

Then pick the next feature from the order below and implement it to green.

**Scenarios are fixed.** If one seems wrong while implementing, that is a spec question — raise it
and get a decision. Do not edit a scenario to match the code. Every scenario here is the recorded
answer to a question that was deliberately asked, and the rejected alternatives are documented above
precisely so they don't get quietly reintroduced.

## Recommended order

All questions are answered; this order is now about dependencies and risk, not blockers.

1. **F3** — command injection. Purest logic, highest value, no dependencies. Path building, quoting
   and the terminal input model are needed by F4 anyway.
2. **F6** — view mode state machine. Self-contained; introduces the injected clock.
3. **F9** — config and state. Small, and F2 needs `state.json` for expansion state.
4. **F4** — new-file flow. Builds directly on F3's tracked command.
5. **F1 / F2** — startup and tree. F2 depends on F9.
6. **F5** — watching. Depends on F2's expansion model and F7's git status.
7. **F7 / F8** — review view and submission. Largest surface, most edges (git, pty, artifacts).

## Current repo state

Spec complete, nothing implemented. The project is **Rust** — see `docs/stack.md` for the stack and
why it was chosen over Node/ink and Go. The Node scaffolding used to learn Cucumber has been removed;
the `.feature` files carried over unchanged, which was the point of writing the spec in Gherkin.

```
features/*.feature     the spec — 87 scenarios, all undefined
tests/cucumber.rs      the runner + VardeWorld; step definitions go here
src/lib.rs             pure logic, empty for now
docs/example-map.md    this file — rules, answers, rationale
docs/stack.md          crates and why each is load-bearing
AGENTS.md              conventions; CLAUDE.md imports it
```
