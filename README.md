# Varde

*A varde is a stone cairn somebody stacked to mark the way for whoever comes next.*

A terminal IDE that opens on a folder and presents it as a workspace: a file tree, a modal editor, a
real shell and an AI CLI, side by side in one terminal. It also has a git review you can walk,
comment on and hand back to the AI.

> **Disclaimer:** an LLM wrote every line of this editor. I build it feature by feature and bug by
> bug using Matt Pocock's skills and workflow. I made the editor for fun, and as a place to try ideas
> that help me work with code and agents. The goal is features that help with understanding code and
> with steering its design and implementation. It started as a simple Review view and grew into a
> Story view, which has helped me understand the design and control flow of an implementation. You're
> welcome to use it. If you want to contribute, open a PR and I'll look at it.

![Varde in Edit view](docs/images/edit.svg)

## Stack

Rust, chosen after the spec was complete. The two biggest parts of this product are an embedded
terminal emulator and a modal editor with syntax highlighting. Rust has mature crates for both, and
other languages have almost nothing equivalent.

| Concern | Crate |
|---|---|
| TUI, layout, widgets | `ratatui` + `crossterm` |
| Hosted panes (shell, AI CLI) | `portable-pty` + `vt100` |
| Key encoding for hosted children | `terminput` |
| Syntax highlighting | `syntect` + `two-face` (220 syntaxes) |
| Git status and diffs | `git2` |
| Markdown preview and diagrams | `pulldown-cmark` + `mermaid-text` |
| Language intelligence | `lsp-types` + `lsp-server` |
| Code metrics (Risk) | `rust-code-analysis` |
| File watching, ignore rules | `notify`, `ignore` |
| Behaviour tests | `cucumber` |

`docs/stack.md` names every dependency and the reasoning behind it. Read it before adding one.

## Getting started

One command, interactive, on macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/oyvij/varde-editor/main/install.sh | bash
```

By default it installs the prebuilt binary for your platform from the latest Release to
`~/.local/bin/varde`. It checks the binary against the Release's `SHA256SUMS` before writing
anything. You need no Rust toolchain and no compile, and `:update` inside the editor keeps it
current.

Answer "source" at its first prompt, or clone and run the same script from the clone, and it builds
into a checkout instead:

```sh
git clone https://github.com/oyvij/varde-editor.git && varde-editor/install.sh
```

It writes `~/.varde/config.toml`, then asks the installed `varde` which package managers its rows'
install commands need (`varde --deps`) and offers each one that is missing, naming the rows it is
for. Language servers, formatters and the voice themselves are one key each in Tools inside Varde.
Every missing program is a `y/N` prompt with the exact command it will run. Declining a required one
aborts, and declining an optional one skips it. Run the same command again later and it updates what
it finds behind `varde`: it replaces a binary with the latest Release, or pulls and rebuilds a
checkout. Then it offers whatever is still missing. `./install.sh --list` prints every program you
can configure Varde to run and whether it is installed, without changing anything.

From source by hand, Varde is a symlink on your PATH pointing at the release binary inside your
checkout:

```sh
cargo build --release
ln -sfn "$(pwd)/target/release/varde" ~/.local/bin/varde
```

Then, from any folder:

```sh
varde .            # open the current folder as the workspace
varde ~/some/repo  # open a folder somewhere else
```

On a source install the ordinary release build *is* the install, because `cargo build --release`
overwrites the file the symlink names. `docs/install.md` covers what follows from that, and how to
reclaim build space without uninstalling.

The first run seeds `<project>/.varde/config.toml` with every key commented out, so every setting is
in that file for you to find. Global defaults live in `~/.varde/`.

Development:

```sh
cargo test                    # unit tests + Gherkin scenarios
cargo test --test cucumber    # behaviour suite only
cargo clippy -- -D warnings
```

## Documentation

The user guide lives in [`docs/guide/`](docs/guide/README.md), one file per area:

| Guide | What it covers |
|---|---|
| [Getting around](docs/guide/getting-around.md) | Starting Varde, the panes and views, the palette, focus, the file tree, buffers, terminal, mouse, quitting, updating |
| [Editing](docs/guide/editing.md) | Modal editing, motions and operators, selection, search, multi-cursor, folding, minimap, preview, every `:` command |
| [Language intelligence](docs/guide/language-intelligence.md) | Language servers, definition, hover, diagnostics, completion, formatting, the server list |
| [Review](docs/guide/review.md) | Review view, annotating a diff, submitting to the AI |
| [Stories](docs/guide/stories.md) | Asking the AI to narrate a change, the spine, walking steps, branches and guest repos |
| [Risk](docs/guide/risk.md) | The Risk figure and list, the Refactor loop and its test Gate |
| [AI pane](docs/guide/ai-pane.md) | Hosting an AI CLI, `:ai`, Tall layout, what reaches it and how |
| [Reading aloud](docs/guide/reading-aloud.md) | `:read` a selection, pausing, speed, installing a voice |
| [Configuration](docs/guide/configuration.md) | Every config key, the two config files, what lives in `.varde/` |

Installing and updating: [`docs/install.md`](docs/install.md). The specification behind all of it is
[`docs/example-map.md`](docs/example-map.md), and `features/*.feature` is the executable form.

## Features

An overview. `docs/example-map.md` is the specification; `features/*.feature` is the executable one.

**Four panes, one terminal.** File tree with type icons, buffer marks and git-aware dimming; a modal
editor; your own shell; and an AI CLI. `Ctrl+Space` (or `Esc Esc`) opens the palette, which reaches
every pane, view and project command on any terminal.

![The command palette](docs/images/palette.svg)

**Modal editing.** Vim-style normal/insert modes, operators and motions, linewise and charwise
selection, registers, undo, in-file search and project-wide search. Every motion has a binding
without a modifier key. Modifier bindings are aliases, never the only way. The cheatsheet in the
editor's top-right lists every binding.

**Review view.** Only the files git reports as changed, shown as diffs, annotated with
`ISSUE` / `NOTE` / `SUGGESTION` / `COMMENT` against a range of lines. Submitting a review writes it
to disk *and* hands it to the AI session, so you can read a change and ask for the fix without
leaving Varde.

![Reviewing a change](docs/images/review.svg)

**Markdown preview.** Varde renders a Markdown buffer as the document it describes: headings,
tables, lists and Mermaid diagrams, laid out to the pane, with no browser and no image protocol. The
preview is read-only, and editing switches back to the source first.

![Markdown preview with a Mermaid diagram](docs/images/preview.svg)

**Risk, and a loop that lowers it.** Per-function complexity across the workspace or just the files
under review, shown as a count and a list, never a percentage. It is CX on its own, or CRAP once
test coverage is available. The refactor loop hands an AI session a scope and a goal, then measures
the result itself. If the tests fail or the number did not go down, Varde reverts the pass.

![The Risk list](docs/images/risk.svg)

**Language intelligence.** A language server per configured language gives hover, go-to-definition,
completion with snippet tab stops, and diagnostics per severity. Configuration names the servers, and
no code branches on which one is running. Where no server answers, `:format` runs the formatter the
project named.

**Stories.** An AI session can write a change up as walkthroughs. Each is a list of named steps, and
each step makes a claim about a place in the code. The steps follow the order the code runs, not the
order of the files. Coverage says which hunks the stories claim and which are left over, as a count
and a list.

**Hosted panes are transparent.** No code tests which CLI is running in a pane. Varde passes keys,
mouse reports, pastes and terminal queries through unchanged, or refuses them with an explicit
reply. That is why CLIs nobody has tried with Varde work too.

**Mouse, file watching, and the rest.** Click to focus, drag to select in any pane (including the
pty ones), and scroll where a pane scrolls. The tree and buffers follow files changing on disk.
Varde draws a frame only when something changed, so idle CPU is 0%.

## Architecture

One state struct, one `update()` that changes it, and everything the outside world must do returned
as data:

```rust
pub fn update(state: &State, event: Event) -> (State, Vec<Effect>);
```

`src/` is pure. Nothing in it touches the terminal, the pty, git or the filesystem. `src/main.rs`,
`src/ui.rs` and `src/pty.rs` are the edge that executes the effects, and they make no decisions.
That is what keeps the behaviour suite fast and free of fixture folders and real terminals.

`AGENTS.md` holds the rules for working in this repository. Read it before changing anything.
`CONTEXT.md` is the glossary, and `docs/adr/` holds the decisions.

## About the pictures

They are real frames, not mock-ups. A script drives Varde through a pty and captures the byte
stream. `examples/shot.rs` replays it through the same `vt100` parser the editor uses and renders the
final screen, colours included, to SVG:

```sh
cargo run --example replay -- capture.out 34 170            # the frame as text
cargo run --example shot -- capture.out shot.svg 34 170     # the frame as a picture
```
