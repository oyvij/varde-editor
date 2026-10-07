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
Before changing them, or `src/keys.rs`, `src/mouse.rs`, `src/layout.rs` or `src/queries.rs`, read
`docs/edge.md`: input, the mouse, scrolling, hosted panes, drawing and how to verify the edge by
running it.

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
- **Assert the entries a scenario is about, not the whole list.** A Then that pins every palette
  entry or a scroll stop set by the cheatsheet's length goes red whenever the next feature adds a
  row. Spec #150's `j` and `w` broke two such scenarios that had nothing to do with them. Name the
  entries under test; let a unit test beside the list hold its full contents if that matters.
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
  On an integration branch that collects several tickets, the ticket branches do not bump; the
  integration branch bumps once, in its last commit, for everything it carries. Parallel bumps
  would conflict on every merge and stack minor versions for one release.
- More structural detail: `.claude/project-map.md` (if present)
