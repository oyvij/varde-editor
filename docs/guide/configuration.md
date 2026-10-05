# Configuration

Varde reads its configuration from two TOML files, and keeps a little state beside the project one.
This page lists every key, its default, and what else you will find in a `.varde/` folder. For the
features the keys belong to, see [language-intelligence.md](language-intelligence.md),
[risk.md](risk.md), [reading-aloud.md](reading-aloud.md) and
[getting-around.md](getting-around.md). For installing Varde, see [../install.md](../install.md).

## The two files

| File | Scope | Created by |
|---|---|---|
| `~/.varde/config.toml` | You, on this machine, for every project | Varde seeds it from the template when it is missing. Taking a row in Tools appends that row, and the speech install fills in a blank `speech.voice`. Nothing already in the file changes. |
| `<project>/.varde/config.toml` | This project, for everyone who opens it | Varde, the first time it opens the folder and finds no file there. |

`[ai]` is the exception: it is a per-user setting, read only from the global file.

The effective configuration is a **deep merge** of three layers: the defaults built into the
binary, then `~/.varde/config.toml`, then the project's own file. **Project overrides global, and
global overrides built-in, key by key.** A project that sets `editor.tab_width = 2` inherits
everything else. A global file that changes the Rust server's command changes only that command,
and a project can still set its own `args` on the same row without repeating the command. A missing
layer counts as empty and merges nothing.

The shipped values are the bottom layer rather than something written to your disk. So when a new
version of Varde changes a default, such as a corrected install command or a new language, the
change takes effect on first run without touching any file you maintain.

### The seeded file

The first time Varde opens a folder, it creates `.varde/` and writes a `config.toml` there with
**every key commented out**. Only the table headers are live. The file is there so you can find the
keys, not to fix this version's values in place. A file full of live values would mean the project
sets everything from its first run. It would also freeze one binary's numbers into a file that
outlives it, so a later correction to a default would change nothing.

Uncomment a line to override the default beside it. Comment it out again to go back to whatever the
running version uses. The header lines are live on purpose: uncommenting `tab_width` under a
commented-out `[editor]` would set a top-level key nothing reads. Varde leaves a file that already
exists alone on every later start, even one it cannot read.

The seeded file lists `view.double_tap_ms`, `editor.tab_width`, `editor.minimap`, `risk.threshold`,
`risk.max_iterations` and the `[speech]` scalar keys. The global file also lists `ai.env`. A
project's file does not, because a project cannot set `[ai]`. It leaves out the `[lsp.*]`, `[formatter.*]`
and `[facts.*]` tables, because those are rows you add per language, not numbers to tune. It also
leaves out settings whose default exists only in code (`editor.theme`, `ai.command`), because there
is no shipped text to keep them in step with.

### A file that does not parse

If Varde cannot use a config file, **Varde does not start**. It names the file, the line, and which
of three faults it found:

- the text is not TOML;
- a value is the wrong type (`install.macos = 12`, or an `unanswerable` naming a request and no
  response);
- an entry parsed and has the right types but is missing the one key it needs. That is an `[lsp.*]`
  or `[formatter.*]` row that no layer gave a `command`, or a `[facts.*]` row with no `marker`.

That last check runs on the merged result, so a project that sets only `args` for a shipped language
is fine. Varde blames the fault on the last file that mentioned the row.

This is deliberate. A broken `~/.varde/config.toml` locks you out until you fix it in another
editor, which is why the error names the file *and* the line.

## Every key

Types are TOML types. "Shipped" means the built-in bottom layer. "code" means the default is in the
binary and does not appear as TOML anywhere.

### `[view]`

| Key | Default | Type | Meaning |
|---|---|---|---|
| `double_tap_ms` | `300` | integer, ms | How soon after a key is tapped a second tap of the same key still counts as a double-tap. Raise it if your intended double-taps keep arriving too late. This is the window for double-tapping Ctrl (or Esc from a hosted pane) to open the palette. |

### `[editor]`

| Key | Default | Type | Meaning |
|---|---|---|---|
| `tab_width` | `4` | integer | What Tab inserts while inserting, and what Enter falls back to when it opens a block in a file with no indentation of its own to copy. If the file has indentation, Enter uses that instead. |
| `minimap` | `true` | boolean | Whether the minimap down the editor's right-hand edge is shown when a project opens. `:minimap` toggles it from inside Varde. Varde remembers that choice per project, and it overrides this key. |
| `theme` | `"dark"` (code) | string | The editor's colours. `"light"` selects the light theme. Anything else selects the dark one. |

### `[ai]`

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | `"claude"` (code) | string | What `:ai` starts in the AI pane when the project has no history. It may carry arguments, such as `nono run --profile claude -- claude`. Varde reads it only from `~/.varde/config.toml`: a project file that sets `[ai]` is ignored. Varde remembers the command you last used in this project (`:ai <command>`) in `state.json`, and that overrides this key. |
| `env` | `["HOME", "PATH", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE"]` | list of names | The only environment variables the AI CLI is given. Varde starts it with an empty environment and copies in each name on this list that is set where Varde was started. Everything else stays out, including secrets a `.envrc` or your shell profile exported, and anything Claude runs inherits the same short list. To pass a secret, such as `ANTHROPIC_API_KEY`, name it here. Setting the key replaces the whole list, so repeat the defaults you still want. `TERM` and `COLORTERM` are set by Varde and need no entry. `SHELL` is always set to your login shell, by the pty library. Leaving out `PATH` means the command must be an absolute path. Read only from `~/.varde/config.toml`. A change applies to the next AI session. |

### `[risk]`

| Key | Default | Type | Meaning |
|---|---|---|---|
| `threshold` | `15` | integer | The cyclomatic complexity a function may reach before Risk lists it. |
| `max_iterations` | `10` | integer | How many passes the Refactor loop may run before it stops. A function the AI cannot get under the threshold needs your attention, and a loop with no cap spends tokens finding that out. |
| `test_command` | unset | string | What the Refactor loop's Gate runs. If unset, Varde picks the command from the project's files (`Cargo.toml` gives `cargo test`). If it finds nothing, Varde refuses to start the loop rather than pass a Gate that ran nothing. |

### `[facts.<name>]`

A fact is a path on this machine that a server or formatter needs and that you cannot write down
once, such as the TypeScript SDK a Vue project pinned or a virtualenv's interpreter. The
configuration says how to find it. Varde searches for it every time it starts a server, and the
result replaces any `${name}` in an `[lsp.*]` or `[formatter.*]` row. The search runs from the
directory of the file being served up to the workspace root, nearest first, and never above it. So
a monorepo package that installs its own toolchain gets its own answer.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `marker` | required | string | A relative path Varde looks for under each directory on the way up. The directory where it is found is the one that configures the language. |
| `value` | `"marker"` | `"marker"` or `"directory"` | Whether the answer is the marker file itself or the directory holding it. |
| `command` | unset | string | A machine-wide fallback: a command on `PATH` to resolve (following symlinks) when no marker is found. Set both this and `command_marker`, or neither. |
| `command_marker` | unset | string | Where the marker is, relative to the directory holding that command's real file. Varde checks it rather than assuming it. |
| `install.<os>` | unset | string | What puts `command` on this machine, as on an `[lsp.*]` row. A server that is installed but missing this fact offers it in Tools, and `i` runs it, or refuses with `needs-installer` when the package manager it starts with is not on `PATH`. |
| `optional` | `false` | boolean | Whether a server whose row names this fact starts without it. If the fact is required (the default) and not found, the server does not start and its row reads `missing-requirement`. If it is optional and not found, Varde drops the argument or option that asked for it and starts the server as if the row never mentioned it. A found optional fact is filled in like any other. |

**Example: the two shipped facts.** You must tell `@vue/language-server` its TypeScript SDK with
`--tsdk=<dir>`, and the directory differs per project and per machine:

```toml
[facts.typescript_sdk]
marker = "node_modules/typescript/lib/typescript.js"
value = "directory"                # the server wants the `lib` directory, not the file
command = "tsc"                    # fall back to the global install…
command_marker = "../lib/typescript.js"   # …but only if it really has typescript.js in it
install.linux = "npm install -g typescript@6" # what puts `tsc` there

[lsp.vue]
args = ["--stdio", "--tsdk=${typescript_sdk}"]
```

Opening `src/App.vue` looks for `node_modules/typescript/lib/typescript.js` in `src/`, then in the
project root. Failing that, Varde finds where `tsc` on `PATH` really lives and checks for the file
beside it. If found, the server starts with `--tsdk=/your/project/node_modules/typescript/lib`. If
not, no server starts and Tools says `missing-requirement`, naming `typescript_sdk`. `i` on the row
runs the fact's `install`, and the next check starts the server with no restart. Varde never writes
what the search finds into the file. The fact stays a search, so the row is right on every machine
and after every Node upgrade.

The second shipped fact, `vue_typescript_plugin`, lets the *TypeScript* server answer about `.vue`
files. It is `optional = true` because the `[lsp.typescript]` row names it, and every TypeScript
project shares that row. If it were required, a machine with no Vue server would have no TypeScript
server at all.

Your own facts work the same way. For example, `[facts.python_env]` with
`marker = ".venv/bin/python"`, and `args = ["--stdio", "--pythonpath=${python_env}"]` on the Python
row. A `${…}` that names no `[facts.*]` table is not a placeholder, and Varde passes it through as
written, so `${HOME}` stays a plain string.

### `[lsp.<language>]`

One table per language, where the language is what the file's extension maps to (`rust`,
`typescript`, `vue`, `python`, …). Naming a server does not start it. Varde starts it when a file in
that language is open, and only if the command is on `PATH`. Tools (palette, `v`) shows every row
and its state (`installed`, `missing`, `stopped`, `no-install-command`, `missing-requirement`,
`partly-working`), with `i` to offer the install and `r` to re-check.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | required after the merge | string | The server binary. |
| `args` | `[]` | array of strings | Its arguments. Varde fills in `${fact}` names. |
| `also_served_by` | `[]` | array of language names | Other languages' servers that also serve this language's files. The Vue server *and* the TypeScript server both serve a `.vue` file. |
| `language_ids` | `{}` | table of extension to string | The language id the server is told for a file with that extension, where it is not the table name. A `.tsx` file is a `typescriptreact` document; told `typescript`, the server parses it without JSX. |
| `install.macos`, `install.linux`, `install.windows` | per row | string | What installs the server on that OS. Runs in the terminal pane when you take the row in Tools. A row with no key for your OS says `no-install-command` and offers nothing. |
| `initialization_options` | unset | table | Passed to the server unchanged at start-up, as JSON. Varde reads nothing inside it. It fills in `${fact}` values, and drops a key whose value asked for an optional fact it did not find. |
| `partial` | unset | string | What this server still cannot do when installed and running, in your words. Shown on its row, which then reads `partly-working`. |
| `unanswerable.request`, `unanswerable.response` | unset | two strings, both or neither | A question this server asks its client that Varde will not answer, and the method to refuse it with, so the server moves on instead of waiting forever. |

Shipped rows:

| Language | Command | Install (macOS · Linux · Windows) | Notes |
|---|---|---|---|
| `rust` | `rust-analyzer` | `rustup component add rust-analyzer` on all three | |
| `typescript` | `typescript-language-server --stdio` | `npm install -g typescript@6 typescript-language-server` on all three | `language_ids = { tsx = "typescriptreact" }`; `initialization_options.tsserver.path = "${typescript_sdk}/tsserver.js"`; a `@vue/typescript-plugin` entry at `${vue_typescript_plugin}` for `vue` files. |
| `javascript` | `typescript-language-server --stdio` | same | `language_ids = { jsx = "javascriptreact" }`; same `tsserver.path`; no plugin. |
| `vue` | `vue-language-server --stdio --tsdk=${typescript_sdk}` | `npm install -g @vue/language-server` on all three | `also_served_by = ["typescript"]`; `unanswerable` = `tsserver/request` / `tsserver/response`. |
| `python` | `pyright-langserver --stdio` | `npm install -g pyright` on all three | |
| `go` | `gopls` | `go install golang.org/x/tools/gopls@latest` on all three | |
| `c`, `cpp` | `clangd` | Linux `sudo apt install clangd` · Windows `winget install LLVM.LLVM` | No macOS command: Homebrew's `llvm` is keg-only, so after an install `clangd` would still not be on `PATH`. |
| `java` | `jdtls` | macOS `brew install jdtls` | Not packaged elsewhere. |
| `zig` | `zls` | macOS `brew install zls` | Linux is a build from source. |

When nobody has packaged a language's server for an OS, the row has **no** install key rather than
an invented one. A command that fails looks configured, while a missing one is fixable in one line
of TOML.

### `[formatter.<language>]`

The same shape, for `:format`, which asks the language server first and this command second. Every
shipped command reads the text on stdin and writes the result on stdout, because only that can
format text you have not saved. A tool that only rewrites files in place gets no row.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | required after the merge | string | The formatter binary. |
| `args` | `[]` | array of strings | Its arguments. `${file}` is the buffer's own absolute path, and `${fact}` names are filled in too. |
| `install.<os>` | per row | string | Typed onto the terminal line when `:format` finds the command missing. Never run. |
| `extensions` | `[]` | array of strings | The file extensions this row claims, for languages no `[lsp.*]` table maps, such as `html`, `css`, `json` and `yaml`. Not needed where the language already has a server row. |

Shipped rows:

| Language | Command | Extensions | Install |
|---|---|---|---|
| `rust` | `rustfmt` | | `rustup component add rustfmt` |
| `python` | `black --quiet -` | | macOS/Linux `pipx install black` · Windows `pip install black` |
| `go` | `gofmt` | | none; ships with the toolchain |
| `javascript`, `typescript`, `vue` | `prettier --stdin-filepath ${file}` | | `npm install -g prettier` |
| `json` | same | `json`, `jsonc` | same |
| `yaml` | same | `yaml`, `yml` | same |
| `html` | same | `html`, `htm` | same |
| `css` | same | `css`, `scss`, `less` | same |
| `markdown` | same | `md`, `markdown` | same |

`--stdin-filepath ${file}` tells `prettier` whether it is reading JSON or YAML, since the bytes alone
do not say. For a language with no row, `:format` refuses with a message naming the table to add,
e.g. `[formatter.txt] in .varde/config.toml`.

### `[dap.<language>]`

The Debug adapter for a language ([debugging.md](debugging.md)). Varde talks to it over its standard
input and output. The table name is what a Launch configuration's `adapter` refers to.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | required after the merge | string | The adapter binary. |
| `args` | `[]` | array of strings | Its arguments. |
| `install.<os>` | per row | string | What puts `command` on this machine, run from Tools as on an `[lsp.*]` row. |

Shipped rows:

| Language | Command | Install |
|---|---|---|
| `rust` | `codelldb` | macOS/Linux: the latest release unpacked into `~/.varde/codelldb`, run by a script in `~/.local/bin` |

### `[launch.<name>]`

A named way to start a Debug session, allowed in both the global file and the project's. A project
entry overrides a global one of the same name, key by key.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `adapter` | required after the merge | string | Which `[dap.*]` row runs the session. |
| `request` | required after the merge | string | `launch` to start the program, `attach` to join one that is running. |
| `args` | `{}` | table | Passed to the adapter unchanged as the request's arguments. Each adapter documents its own. |

### `[speech]`

What reads a Selection aloud. [reading-aloud.md](reading-aloud.md) explains it in full.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | `"piper"` | string | The synthesizer. |
| `args` | `["--model", "${voice}", "--length-scale", "${scale}", "--noise-w-scale", "1.0", "--output_dir", "${dir}"]` | array of strings | `${voice}` is the row below, `${scale}` the reciprocal of `speed`, `${dir}` where Varde writes the stream. |
| `voice` | `""` | string | Path to the voice model; a leading `~` is your home directory. Blank until the install fills it in. |
| `speed` | `1.0` | float | Multiplier, higher is faster. Applies to the next Reading. |
| `player.macos`, `player.linux` | `"afplay"`, `"aplay"` | string | What plays the stream. No `player.windows` ships. |
| `install.macos`, `install.linux` | shipped | string | Installs `piper` with `uv` and fetches the `en_US-bryce-medium` voice into `~/.varde/voices/`. Runs in the shell pane when you take the row in Tools. No `install.windows`. |
| `configures.voice` | `"~/.varde/voices/en_US-bryce-medium.onnx"` | string | Written into `voice` once the install exits 0, unless `voice` is already set. |

### All substitutions

| Placeholder | Filled in | With |
|---|---|---|
| `${<fact>}` | `[lsp.*].args`, `[lsp.*].initialization_options`, `[formatter.*].args` | The path a `[facts.<fact>]` search found. If a required fact is not found, the server does not start. If an optional one is not found, Varde drops the argument or option key that used it. |
| `${file}` | `[formatter.*].args` | The absolute path of the buffer being formatted. |
| `${voice}`, `${scale}`, `${dir}` | `[speech].args` | The voice model path, `1 / speed`, and Varde's scratch directory. |

Nothing else is a placeholder. There is no environment-variable expansion and no template language.

### `install.<os>`: run when you take the row

Taking a row in Tools (`i`) does the whole install. Varde appends the row to `~/.varde/config.toml`
if the file lacks it, and runs its `install` key in the terminal pane, where you can watch it and
answer a `sudo` prompt. Its exit status comes back to the row, which reads `install-failed` if it
failed. If the install's first program (or the one after `sudo`) is not on your `PATH`, Varde does
not run it at all. The row reads `needs-installer` and names the package manager, which `install.sh`
installs. In other places, such as `:format` finding its command missing, or reading aloud, Varde
still only types the command onto the terminal pane's input line without pressing Enter. Opening a
file never installs anything. The key that applies is the one for the operating system the binary
was built for. Once the command exists, the next check finds it with no restart. The exception is
an installer that only added to your shell profile. Then Varde offers a restart and never restarts
on its own.

## What else lives in `.varde/`

A project's `.varde/` folder holds more than the config. Which of it belongs in version control is
your decision, and Varde never writes ignore rules. This repo's own `.gitignore` shows the
recommended split:

| Path | What it is | Commit? |
|---|---|---|
| `config.toml` | The project's settings, seeded commented-out | Yes. It holds the project's settings for everyone who opens it. |
| `reviews/NNNN.json` | Reviews you submitted from Review view, numbered, the newest 50 kept | Yes. You wrote them. |
| `stories/<base>-<head>.json` | Story sets the AI wrote for a range, the newest 10 kept | Yes. They are written work. |
| `state.json` | Per-user session state (below) | No. It is yours, not the project's. |
| `risk.json` | The Risk figure, cached against the commit it was measured at | No. It is derived and changes on every commit. |
| `snapshots/<n>/` | The Refactor loop's copy of the files an Iteration touched, so Varde can restore them after a failed Iteration | No. It is derived. |
| `refactor-done` | The sentinel the AI session writes to say an Iteration is finished. Varde deletes it before each one | No. |
| `lsp-<language>.log` | One log per language server, the place to look when a server will not start | No. It is per session. |

`state.json` holds what Varde remembers about *you* in this project between sessions: the last view
(Edit, Review or Story); the AI command you last started; which tree folders were expanded; the tree
divider's position and the AI pane's width and shape (beside the editor or tall); which pane is in
the Corner beneath the tree; whether the editor is dimmed and whether the minimap is on; and the open
buffers and which one was current. Varde writes it as JSON, and there is nothing in it to edit by
hand.

## The Bare workspace

`varde` with no folder argument opens the current directory as a **Bare workspace**. The folder is
still the workspace: the file tree shows it and `:w` writes there. But Varde writes nothing of its
own into it. There is no project config layer at all (Varde ignores a `.varde/config.toml` in the
folder), nothing is seeded, and Risk is not measured unless you ask. Everything that would have gone
in `.varde/` goes in a **Sidecar** under `~/.varde/paths/`, named for the folder and the process,
and deleted when Varde exits. So a Bare workspace forgets everything between runs. If you want Varde
to remember, open the folder as a project with `varde .`.

The one exception is a submitted review, which the Sidecar's deletion would otherwise destroy. From a
Bare workspace it goes to `~/.varde/reviews/`, which every Bare workspace shares and which keeps the
same number of reviews.

So `~/.varde/` holds `config.toml` (yours), `voices/` (where the shipped install puts a model),
`tmp/` (a Reading's audio, cleared on start), `paths/` (Sidecars) and `reviews/` (Bare-workspace
reviews). Varde creates the directories it needs and never edits the config.

## Versions and updates

On a source install, Varde is a symlink into its own checkout (see [../install.md](../install.md)),
so the binary you are running and the source on disk can drift apart. On start Varde compares the
**Version** in the checkout's `Cargo.toml` with the **Running version** the binary was built from.
When the checkout's is strictly newer, an **Update** exists. The version tag at the right end of the
bottom row then turns blue and reads `v0.2.0 → v0.3.0  C-space u to update`. Equal or older is not an
Update, so checking out an old branch never offers one.

Palette `u` (or `:update`) runs `cd <checkout> && cargo build --release` in the terminal pane. The
symlink already names the file that build writes, so the build *is* the install, and the next launch
is the new version. With no checkout above the binary, it is a binary install instead. Palette `u`
then downloads the newer Release for this platform, verifies its checksum, puts it in place of the
running binary and relaunches Varde on it. With neither a checkout nor a newer Release, `:update`
refuses with `nothing-to-update` and does nothing.

The comparison uses version numbers, not commits, so a change committed without a version bump does
not show up here. That is preferred over a notice that fires on every commit and teaches you to
ignore it (`docs/adr/0003-versions-not-commits.md`).
