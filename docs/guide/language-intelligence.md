# Language intelligence

Varde runs a **Language server** for each language you configure and asks it what the code means:
what a name is, where it is defined, what is wrong with it, and what you might be typing. The server
is a separate program on your machine, such as `rust-analyzer`, `gopls` or `pyright-langserver`.
Varde starts it when you open a file in its language and talks to it in the background. It has no
pane. You only see its answers.

None of this removes anything. A language with no server configured, or whose server is not
installed, edits the same as it would otherwise. If a server cannot start, or exits, Varde reports it
once, never on every keystroke, and the editor keeps working.

## What a server gives you

### Diagnostics

Varde marks the errors, warnings, information and hints the server reports in the gutter beside the
line they name, each severity drawn differently. The footer shows the message for the diagnostic on
the cursor's line. Varde underlines the characters the server pointed at. Resting the pointer on an
underline shows that diagnostic's message in a box beside the line, and where two overlap, the box
shows the worse one. The gutter's width does not change when diagnostics arrive.

Marks survive switching to another buffer and back. Each time the server reports on a file, the new
report replaces its previous one, so the gutter shows the present rather than a history. When a
server stops, its own marks go with it and no others.

In Review view each changed file shows how many errors and warnings its server reports, with a total
on the border. A file whose language has no server running, or whose server has not answered yet,
reads as **not measured**, never as zero. A total that leaves such a file out has a leading `~`. See
[Review](review.md).

### Hover: what is this?

`K` asks what the symbol under the cursor is. The reply, its type and its documentation where the
server has any, opens in a box placed so it does not cover the symbol. `Esc` dismisses it. Most
servers answer in markdown, and the box renders it: headings, emphasis, and code fences highlighted
in their language. Varde caps a box taller than the pane and ends it with an ellipsis so you know it
was cut. If the server knows nothing about the symbol, Varde says so by name rather than showing an
empty box. Varde does not show a reply that arrives after the cursor has moved on.

You can ask with the mouse too: **rest the pointer on a symbol for half a second** and its hover
appears, in normal or insert mode, without moving the cursor. The box describes what is under the
pointer, so it closes when the pointer moves off the symbol or leaves the editor. A pointer passing
over the pane on its way somewhere else asks nothing.

### Definition: where is this from?

`gd` asks where the name under the cursor is defined.

- A definition in the file you are reading moves the cursor there.
- A definition in another workspace file opens that file at the line.
- Several definitions appear in the project search's results box, so you choose, rather than Varde
  taking the first.
- If there is no definition, Varde says so and moves nothing.
- Varde refuses a definition **outside the workspace root**, in a dependency's source for example,
  and names the path it would have opened. Every pane in Varde stays inside the root. The tree
  cannot mark a file outside it, and the watcher cannot follow one.

Varde records a jump to a definition, so `Ctrl+P` (or `gp`) takes you back, including from a
definition in the same file. See [Editing](editing.md#going-back).

**Click to definition.** Hold `Cmd` or `Ctrl` while pointing at a name in the editor, and Varde
underlines the name. The two modifiers do the same thing here, because most terminals report a
`Cmd`-click with no modifier at all. The underline is the only sign, since a terminal has no hand
pointer. Clicking the name with the modifier held asks the same question `gd` does. A plain click
only places the cursor.

### Candidates

While you type an identifier in insert mode, Varde waits for a short pause in your typing, asks the
server what you might mean, and shows the **Candidates** in a list just below your line, at the
cursor's column. The list shows ten rows at most and scrolls, so every Candidate stays reachable.

| Key | Does |
|---|---|
| arrows | move the choice |
| `Enter` | accept the selected Candidate, replacing the word you were typing |
| `Esc` | close the list, leaving the buffer holding exactly the characters you typed |
| more letters | narrow the list; `Backspace` widens it again |

Dismissing the list never changes what you wrote. If the word matches no Candidate, the list closes
rather than showing empty, and it also closes when you jump to another file. Where the server gave
an order, the list keeps it.

**Blanks.** Some Candidates are templates with places for values, such as `println!("…")` or
`foo(…, …)`. Accepting one inserts the text without the placeholders and puts the cursor on the
first blank. `Tab` moves to the next blank. At the last one, the cursor lands where the code
continues and the sequence ends. `Esc` ends the sequence and keeps the text, because abandoning a
Candidate is not undoing it. Opening another file ends it too. While blanks remain, `Tab` means
"next blank". The rest of the time it indents, as [Editing](editing.md#typing) describes. A plain
Candidate has no sequence: its text goes in and the cursor lands after it.

### Formatting as you type

A server may ask to be told when you type certain characters, such as `}` or `;` in some languages
or a newline in others. When you type one, it may send back edits that re-indent or re-space the
code around it. Varde applies those edits as one change, so a single `u` undoes them. It drops them
if you kept typing after the request went out.

### `:format`

`:format` lays out the whole buffer. It asks the Language server first. If the server has no
formatting of its own, Varde asks the **Formatter** configured for the language instead, an external
command such as `prettier`, `black` or `rustfmt`. Either way:

- The command sees the **text on screen**, never the file on disk. Varde sends the buffer on the
  command's stdin and replaces the buffer with what it writes on stdout. **Varde writes nothing to
  disk**: an unsaved buffer stays unsaved and `:w` is up to you. So a tool that can only rewrite a
  file in place is not supported.
- The result is **one undo step**, and the cursor moves with the change.
- If the command returns exactly what it was given, Varde says `nothing to format`, because a key
  that changes nothing and says nothing looks broken.
- If the command fails, Varde shows the first line of its error output and leaves the buffer alone.
  A command that exits cleanly and prints nothing also counts as a failure. Varde never empties the
  buffer because of it.
- For a language nothing configures, Varde refuses and names the key to add:
  `[formatter.<language>]` in `.varde/config.toml`.
- If the configured command is **not installed**, Varde puts its install command on the terminal's
  input line and moves focus there without pressing Enter. Varde does not remember that it was
  missing, so the next `:format` after you install it works, with no restart.
- Over a markdown Preview, `:format` switches to Source first, so the undo and the line numbers are
  where you can see them. With nothing open it refuses.

To decide which language a file is for formatting, Varde checks three things in order: the language
the server configuration knows it as, then the `extensions` a formatter row claims, then the file's
own extension. A file with no extension uses its name, so `[formatter.Makefile]` is a key you can
write.

## Tools

`Ctrl+Space` then `v` (the palette's **Tools** entry) opens the list of every program Varde runs,
grouped into language servers, formatters, requirements (`[facts.*]`) and speech (the synthesizer
with its voice, and the player). Each has one row, with the command it runs and its state on this
machine. Varde checks each one when the list opens, so open it right after an install to see the
result. Opening the list starts no server.

The list also shows every template row your config files do not name, as `available`. That is a row
you deleted, or one a newer Varde added. If your row differs from the template's, the row says so,
because a corrected template never edits a row you already have.

| State | Means |
|---|---|
| `installed` | the command is on your `PATH` and, if it has been started, it is answering |
| `missing` | the command is not on your `PATH` |
| `stopped` | the command is here but its server exited; see `.varde/lsp-<language>.log` |
| `missing-requirement` | the command is here but something it needs is not, e.g. a TypeScript SDK; the row says which, and `i` runs that requirement's install if it has one for this OS |
| `partly-working` | the configuration says this server cannot do something, and the row names it |
| `no-install-command` | not installed, and nothing is configured to install it on this OS |
| `available` | a template row no config file names, so Varde does not run it |
| `install-failed` | its install ran and exited with a failure; the output is in the terminal pane |
| `needs-installer` | not installed, and the program its install starts with (`npm`, `uv`, `go`, …, ignoring `sudo`) is not on your `PATH` either; the row names it, and taking it does nothing, because `install.sh` installs package managers |

| Key | Does |
|---|---|
| `i` | take the row: add it to `~/.varde/config.toml` if the file lacks it, and run its install command for this OS in the terminal pane |
| `r` | re-check the row |
| `Esc` | close the list |

Varde appends the row after the last line of your global config, with any requirement it names, and
leaves everything you wrote alone. It does not write a row the file already has. The install runs
where you can watch it and answer a `sudo` prompt, and when it ends Varde checks for the command
again. If your global config does not parse, `i` names the fault and writes and runs nothing. A row
that is already `installed` refuses `i`. A `stopped` row runs its install, because reinstalling is
what fixes a command that is present and does not work. Once the command appears, the server starts
with no restart.

One case needs a restart: an installer that added its directory to your shell profile, which a
running Varde cannot see. If a re-check after installing still finds nothing, Varde asks whether to
restart. Answering yes quits (Varde refuses to quit with unsaved buffers, the same as `Ctrl+Q`), and
declining leaves everything as it was.

## What ships

These servers come configured. Install one and it works with no other setup. Take one in Tools to
install it. The `install.sh` described in [Installing Varde](../install.md) offers the package
managers these commands start with. A cell with a dash means nobody has packaged that server for
that OS; add an `install.<os>` key yourself (below).

| Language | Server | macOS | Linux | Windows |
|---|---|---|---|---|
| Rust | `rust-analyzer` | `rustup component add rust-analyzer` | same | same |
| TypeScript | `typescript-language-server` | `npm install -g typescript@6 typescript-language-server` | same | same |
| JavaScript | `typescript-language-server` | same as TypeScript | same | same |
| Vue | `vue-language-server` | `npm install -g @vue/language-server` | same | same |
| Python | `pyright-langserver` | `npm install -g pyright` | same | same |
| Go | `gopls` | `go install golang.org/x/tools/gopls@latest` | same | same |
| C | `clangd` | — | `sudo apt install clangd` | `winget install LLVM.LLVM` |
| C++ | `clangd` | — | `sudo apt install clangd` | `winget install LLVM.LLVM` |
| Java | `jdtls` | `brew install jdtls` | — | — |
| Zig | `zls` | `brew install zls` | — | — |

C and C++ have no macOS command because Homebrew's `llvm` is keg-only. The install would succeed,
`clangd` would still not be on `PATH`, and the row would keep reading `missing`.

Formatters, for `:format` where the server does not format:

| Language | Formatter | Install |
|---|---|---|
| Rust | `rustfmt` | `rustup component add rustfmt` |
| Python | `black` | `pipx install black` (Windows: `pip install black`) |
| Go | `gofmt` | ships with the Go toolchain |
| JavaScript, TypeScript, Vue | `prettier` | `npm install -g prettier` |
| JSON (`.json`, `.jsonc`) | `prettier` | same |
| YAML (`.yaml`, `.yml`) | `prettier` | same |
| HTML (`.html`, `.htm`) | `prettier` | same |
| CSS (`.css`, `.scss`, `.less`) | `prettier` | same |
| Markdown (`.md`, `.markdown`) | `prettier` | same |

## Configuring a server or a formatter

Servers and formatters are configuration, never code. The shipped rows are the bottom layer.
`~/.varde/config.toml` overrides them and `<project>/.varde/config.toml` overrides that, **key by
key**. A project that sets one key of a shipped language keeps every other key of it. To add a
language Varde does not know, add a table. See [Configuration](configuration.md).

```toml
[lsp.zig]
command = "zls"                              # what to run; found on PATH or given as a path
args = ["--stdio"]                           # optional
install.macos = "brew install zls"           # one per OS; omit where nothing is packaged
install.linux = "zig build -Doptimize=ReleaseSafe"
partial = "type errors"                      # optional: what this server cannot do, in your words
also_served_by = ["typescript"]              # optional: other languages whose servers also serve these files

[lsp.zig.initialization_options]             # optional: passed to the server verbatim, never read by Varde
some_option = true
```

If a table is malformed, or no layer gives a language a `command`, Varde does not start and names
the file and the line.

```toml
[formatter.Makefile]
command = "some-formatter"                   # must read stdin and write stdout
args = ["--stdin-filepath", "${file}"]       # ${file} tells a multi-language tool what it is reading
extensions = ["mk"]                          # optional: which extensions this row claims
install.macos = "brew install some-formatter"
```

### Paths the server needs: facts

Some servers need a path inside your workspace that differs from machine to machine. The TypeScript
server, for example, will not answer at all unless told where a usable `tsserver.js` is, and the Vue
server needs the same SDK as `--tsdk=`. Varde ships no such path, because an invented path that does
not exist is worse than a blank. Instead it ships a **fact** that finds the path at every start:

```toml
[facts.typescript_sdk]
marker = "node_modules/typescript/lib/typescript.js"   # what to look for, from the file's folder up to the root
value = "directory"                                    # hand over the directory holding it, not the file
command = "tsc"                                        # fallback: a command on PATH...
command_marker = "../lib/typescript.js"                # ...and the marker relative to it
```

Anywhere a string reaches a server, in `args` or inside `initialization_options`, Varde replaces
`${typescript_sdk}` with what the search found. The shipped TypeScript row sets
`initialization_options.tsserver.path = "${typescript_sdk}/tsserver.js"`, and the Vue row passes
`--tsdk=${typescript_sdk}`. The search runs upward from the file being served, so in a monorepo each
package uses its own pinned TypeScript. A global preview build with no `typescript.js` does not
count as one.

If Varde cannot find a fact, it starts no server, and the row reads `missing-requirement` and names
the fact. A server launched without what it needs would only crash on the first file. Varde drops a
fact marked `optional = true` instead and starts the server without it. The shipped
`vue_typescript_plugin` fact is optional because the TypeScript row names it, every TypeScript
project shares that row, and a machine with no Vue server must still get a TypeScript server.

### Example: Vue

Two servers serve a `.vue` file at once. `[lsp.vue]` sets `also_served_by = ["typescript"]`, so
`gd`, `K` and diagnostics ask both. The first server with an answer wins, and Varde reports "nobody
knows" only after the last one has answered. The TypeScript server can only answer about a `.vue`
file when it is loaded with the plugin the Vue server keeps in its own `node_modules`. The optional
`vue_typescript_plugin` fact finds that plugin. Installing the two servers from the list above is
all the setup there is. Template mistakes from one server and type errors from the other appear in
the same gutter.

The Vue server also asks its client a question Varde cannot answer. `[lsp.vue].unanswerable` names
the pair of methods, so Varde sends an explicit refusal and the server carries on with what it can
do alone. If a file's only server has asked such a question, an empty `gd` or `K` says the server
needed a companion, rather than blaming the server for knowing nothing.

## When something goes wrong

Varde writes each server's error output to `.varde/lsp-<language>.log` in the workspace, and
truncates it each time the server starts. Two notices can send you there. "could not start the
language server" means you should check the `command` in `.varde/config.toml` and the log. "the
language server stopped" means there are no diagnostics, hover or completion for that language until
it runs again. Fix what the log says, then press `r` in the server list or open a file in that
language. Nothing needs restarting unless the list asks.

## See also

- [Editing](editing.md): the keys around these, such as `Tab`, `u`, `Ctrl+P` and the results box.
- [Configuration](configuration.md): where `[lsp.*]`, `[formatter.*]` and `[facts.*]` live and how
  the layers merge.
- [Installing Varde](../install.md): `install.sh` reads these same rows and offers the package
  managers they need.
- [Review](review.md): the error and warning counts over a change.
