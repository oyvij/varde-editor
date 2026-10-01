# Risk

Risk measures how hard the workspace's code will be to change safely. Varde measures it per
function, counts how many functions are above a threshold, and puts that count on the tree pane's
top border where you see it without asking. The Risk list turns the count into a worklist, worst
first. The Refactor loop points your AI session at that list, and Varde, not the AI, decides
whether each pass was good.

Risk is not a claim about correctness. Code can be risky and right, and a figure never says a
function is wrong.

## The words

- **Function**: the unit Varde measures Risk against. It is a function or method whose enclosing
  scope is not itself a function. A closure counts toward the Function holding it and is never
  listed alone. Varde reads containers (impls, classes, namespaces, files) but never counts them.
- **Figure**: a Function's measured complexity.
- **Risk count**: how many Functions in the Scope are above the threshold. Always a count and a
  list, never an average or a percentage.
- **Scope**: what a figure covers, either the whole workspace or the files under review. Never a
  mix.
- **CX**: the metric shown when Varde has read no test coverage, which is complexity alone. It is
  labelled `CX` rather than `CRAP` because `CRAP` means complexity weighted by coverage, and this
  figure is not that.
- **Stale figure**: a figure whose workspace has changed since Varde computed it. Varde still shows
  it, and marks it stale.
- **Unparsed**: a file in a language the analyser handles that it could not read. Varde counts and
  shows these, and never drops them without saying so.
- **Refactor loop**, **Iteration**, **Gate**: see below.

## The figure

The metric is cyclomatic complexity per Function, read by the `rust-code-analysis` library. That
library handles Python, Rust, C/C++, Java, JavaScript and TypeScript/TSX. A workspace in no
language it handles shows `nothing-analysed` and no count, because a zero would claim the code is
clean when nothing measured it. Alongside the main figure Varde records each Function's cognitive
complexity, maintainability index and line count, because the Refactor loop's Gate checks all of
them.

### When it is computed

- **On opening a project** (`varde <folder>`), without being asked. The border shows a spinner
  while the job runs and the count when it finishes. Varde caches the result against the commit it
  measured, so reopening on the same commit runs nothing, and reopening after the commit changed
  runs it again.
- **When you ask**, with the Risk pane's recompute action. A recompute runs whether or not the
  figure is stale. A recompute while one is already running replaces it rather than queueing,
  because only one of two analyses of two different states can be current.
- **After every Iteration** of the Refactor loop, so the Gate measures the result instead of
  trusting the session.
- **On entering Review view**, for the files under review only. See below.

Never on a keystroke and never on a save. Saving marks the figure **stale**: the border says so, and
the stale count stays up until you recompute or the commit changes. A figure that changed as you
typed would be noise, and one that claimed to be current when it was not would be worse.

A Bare workspace (`varde` with no folder) measures nothing at startup, because Varde would write the
result into a Sidecar it deletes at exit. Its border reads `nothing-analysed` until you ask, and the
recompute action still works.

### The threshold

A Function counts toward the Risk count when its figure is above `threshold`, under `[risk]` in
your config. The default is 15. A codebase with different norms sets its own. See
[Configuration](configuration.md) for where the file lives and how project and global settings
layer.

```toml
[risk]
threshold = 15
```

## The Risk list

The list is a pane beneath the file tree, at the tree's width. It takes that width from the shell
pane. Toggle it from the palette with `k`. Opening it moves focus into it, and closing it returns
focus to the tree and gives the shell its width back. Varde remembers per project whether it is
open.

With the pane open, `Alt+j` from the tree reaches it, `Alt+j` again reaches the shell below, and
`Alt+h` from the shell comes back to it. With the pane hidden, `Alt+j` from the tree reaches the
shell directly.

### Rows

The list is flat and sorted by figure, highest first, across the whole Scope. By default it shows
only Functions above the threshold, because it is a worklist, not an inventory. `a` toggles showing
every Function, for reading a figure that is not yet a problem. Each row names a Function, the line
it starts on and its figure. The pane's border shows the selected row's file, because the pane is
narrower than a path. The pane also shows how many files were Unparsed, so you know when the figure
covers less than the whole workspace.

While a job runs and nothing has been measured yet, the pane says `computing`. With a stale figure
it shows the stale list, marked stale, until the fresh one arrives. A labelled old answer is better
than no answer.

| Key | In the Risk list |
|---|---|
| `j` `k` / `Down` `Up` | move the selection |
| `Enter` | open the file with the cursor on the Function's first line, and move focus into the editor |
| click a row | the same, but focus stays in the pane |
| `a` | show every Function / only those above the threshold |
| `r` | recompute the figure |
| `l` | start the Refactor loop, or stop it while one runs |
| `j` past the last row | step onto the pane's own actions; `Left` `Right` choose, `Enter` runs, `k` steps back up |
| `Alt+h` `Alt+j` `Alt+k` `Alt+l` | move focus between panes |

The wheel scrolls the list without moving the selection, and any key scrolls the selection back
into view. The selection names a place to go, not text you picked, so Varde never copies it and a
drag over the pane selects nothing.

### Row action: ask for one refactor

Each row has an action, on its icon or through the same gesture the file tree's row actions use,
that asks the AI session to suggest a refactor of that one Function. It sends one prompt with the
name, the file and the figure. The prompt asks for splits that make sense on their own, and says
that a helper called from exactly one place counts as a failed pass. The result is yours to review:
no Iteration, no test run, no Gate, no revert, nothing committed. If no session is running, Varde
starts one and holds the prompt until it is ready.

### Pane actions

The pane's own two actions are on its border, next to the figure. Click their icons, or step down
off the end of the list to reach them:

- **recompute** measures the Scope again;
- **start the Refactor loop** becomes **stop** while a loop runs.

An empty list starts with the keyboard on these actions, because with no rows, recompute is the only
thing left to do.

## The Refactor loop

The loop hands your AI session a Scope and a goal: bring the Functions above the threshold down. It
waits for the session to say a pass is finished. Then it **runs the project's tests itself**,
**recomputes the figures itself**, and applies the Gate. It never asks the session whether the pass
was good. A session reporting success reports what it believes, and that is not a measurement. The
reasoning is in `docs/adr/0010-varde-owns-the-test-gate.md`.

Start it with the pane's action (`l`, or its icon). Varde refuses to start it, and says why, when:

- **it cannot determine a test command** (`no-test-command`). A Gate that passes without running
  anything is worse than no Gate;
- **nothing has been measured yet** (`no-figure`). The Gate takes its baseline from the last figure,
  and a loop judged against zero could never show an improvement. Recompute first. A *measured*
  zero is a fine baseline;
- **a loop is already running** (`loop-already-running`). Two loops editing the same files would
  undo each other's work.

### An Iteration

1. Varde deletes the sentinel file `.varde/refactor-done` if one is left over, so a stale one
   cannot complete the pass before it starts.
2. It snapshots the files under `.varde/snapshots/`, per Iteration.
3. It sends the session one prompt naming the Scope, where the figures are written
   (`.varde/risk.json`), the worst few Functions, the target threshold, what a good split is, and
   an instruction to follow whatever convention file the repository has. The prompt never contains
   the test command and never names an AI provider. If no session is running, Varde starts one and
   holds the prompt until it is ready.
4. It waits for the session to write `.varde/refactor-done`. Varde ignores the file's contents; only
   its existence matters. **There is no timeout.** A session thinking for forty seconds looks the
   same as one that finished, and a timeout would mean measuring a half-written edit. The pane says
   what it is waiting for (`session`, `tests`, `measuring`), so you can tell a long wait from a hang.
5. When the sentinel appears, Varde runs the test command outside the shell pane, so the shell stays
   yours, then recomputes the figures for the Scope.
6. The **Gate** checks three things: the tests still pass, the main figure went down, and no other
   recorded metric went up. "Went down" counts either the Risk count or the total, so lowering a
   Function's figure without yet crossing the threshold is progress. A pass that meets all three
   **stays in the working tree, uncommitted**, and the next Iteration begins. For a pass that fails
   any one, Varde **restores the snapshot and stops the loop**, naming the condition that failed:
   `tests-failed`, `no-improvement` or `other-metric-worsened`.

The third condition stops the session from gaming the count. Splitting one long Function into fifteen
trivial ones lowers the count while cognitive complexity stays the same or rises, so Varde reverts
it. The prompt warns about this up front to save an Iteration, and the Gate enforces it.

A revert restores the snapshot, never the last commit, and covers only the files that Iteration
touched. Varde never overwrites a file you edited that the loop did not touch, and your own
uncommitted edits in a file it did touch come back as yours. Varde explains a reverted Iteration to
the session as well as to you, with the failing condition and the tests' output, so the session
does not build its next answer on an edit it believes is still there.

### The cap and the stop

The loop runs at most `max_iterations` passes (default 10) and says `cap-reached` when it stops for
that reason.

```toml
[risk]
max_iterations = 10
```

To stop the loop, use the same action that started it, in the same place: the pane's icon, or `l`.
`Esc` does nothing to a running loop, because it does too many other things to be the key for
interrupting an edit to your files. Stopping restores the Iteration in progress and leaves the
workspace where the last accepted Iteration left it, with nothing half-applied.

### Reading a run

While a loop runs, the pane shows the Iteration and the cap, the figure as it changes, and the last
test result, and the tree border shows the live count. Once the loop is over, its verdict stays on
the pane until the next measurement. A recompute is a fresh measurement, so it clears a finished
run's verdict. It leaves a running loop's verdict alone.

**Varde never commits anything.** The loop's only output is a dirty working tree, and you judge it
in [Review view](review.md).

### The test command

The Gate runs the project's tests. Varde takes the command from the first of these that applies:

1. `test_command` under `[risk]` in your config;
2. otherwise, the files in the project:

| File present | Command |
|---|---|
| `Cargo.toml` | `cargo test` |
| `package.json` | `npm test` |
| `pyproject.toml` | `pytest` |
| `go.mod` | `go test ./...` |
| `pom.xml` | `mvn test` |
| `build.gradle` / `build.gradle.kts` | `gradle test` |
| `Makefile` | `make test` |

A Rust project that also has a `Makefile` gets `cargo test`. If neither the config nor the table
gives a command, Varde refuses to start the loop.

```toml
[risk]
test_command = "cargo nextest run"
```

## Risk in Review view

Entering [Review view](review.md) measures the files under review, that Scope and never a mix,
against the revision the diff starts from. The border shows the **delta** rather than the
workspace's count. Varde marks a change that raised the figure as worse, because a reviewer wants
that signal most and gets it least reliably from reading a diff. Leaving the view puts the
workspace's count back. If the figure is stale, Varde recomputes it for the reviewed files rather
than drawing a stale delta.

With the Risk list open, it lists the Functions in the changed files, each with its figure and its
delta, and no Function from outside the change. A reviewed file in a language the analyser does not
handle contributes nothing, and Varde never reports it as an improvement.

The pane's action here starts the loop over exactly those files. It works the same way with a
different file set: the same three-condition Gate, the same per-Iteration snapshot, the same cap,
the same stop, nothing committed, and no file outside the Scope touched. Only one loop runs at a
time across both Scopes.

## What lives under `.varde/`

| Path | What it is |
|---|---|
| `.varde/risk.json` | The last figure: the commit it was measured at, the metric (`CX`), and every Function with its file, name, first line and all four recorded metrics: `cyclomatic`, `cognitive`, `maintainability`, `lines`. The loop's prompt points the session here. |
| `.varde/snapshots/` | Per-Iteration copies of the files the loop let the session touch, restored when a pass fails the Gate. |
| `.varde/refactor-done` | The sentinel the session writes to say a pass is finished. Deleted before each Iteration. |

Varde never writes a git ignore rule for these or anything else. Whether to ignore them is your
project's decision, in your `.gitignore`.

## See also

- [Review](review.md): where you judge the loop's output, and the delta on the border.
- [AI pane](ai-pane.md): the session the loop drives.
- [Configuration](configuration.md): `[risk] threshold`, `max_iterations`, `test_command`.
