# Configuration

Varde is configured in TOML, from two files, and keeps a little state beside the project one. This
page is every key, what it defaults to, and what else you will find in a `.varde/` folder. For the
features the keys belong to, see [language-intelligence.md](language-intelligence.md),
[risk.md](risk.md), [reading-aloud.md](reading-aloud.md) and
[getting-around.md](getting-around.md); for putting Varde on the machine, [../install.md](../install.md).

## The two files

| File | Scope | Created by |
|---|---|---|
| `~/.varde/config.toml` | You, on this machine — every project | Varde seeds it from the template when it is missing. Taking a row in Tools appends that row, and the speech install fills in a blank `speech.voice`; nothing already in the file is changed. |
| `<project>/.varde/config.toml` | This project — everyone who opens it | Varde, the first time it opens the folder and finds nothing there. |

The effective configuration is a **deep merge** of three layers: the defaults built into the binary,
then `~/.varde/config.toml`, then the project's own file. **Project beats global, global beats
built-in, key by key.** A project that sets `editor.tab_width = 2` inherits everything else; a global
file that renames the Rust server's command touches only that command, and a project can still put
its own `args` on the same row without repeating the command. A layer that is not there is an empty
one and merges nothing.

Because the shipped values are the bottom layer rather than something written to your disk, a new
version of Varde's defaults — a corrected install command, a new language — is live on first run
without touching any file you maintain.

### The seeded file

The first time Varde opens a folder it creates `.varde/` and writes a `config.toml` there in which
**every key is commented out** — only the table headers are live. It is there so the keys can be
found, not to pin this version's answers: a file full of live values would make "the project sets
nothing" false on its first run, and would freeze one binary's numbers into a file that outlives it,
so a later correction to the defaults would arrive and change nothing.

Uncomment a line to disagree with the default beside it; delete it again to go back to whatever the
version you are running thinks is right. The header lines are live on purpose — uncommenting
`tab_width` under a commented-out `[editor]` would set a top-level key nothing reads. A file that is
already there is left alone on every later start, including one Varde cannot read.

The seeded file names `view.double_tap_ms`, `editor.tab_width`, `editor.minimap`, `risk.threshold`,
`risk.max_iterations` and the `[speech]` scalars. The `[lsp.*]`, `[formatter.*]` and `[facts.*]`
tables are not in it — they are rows you reach for by language, not numbers to tune — and settings
whose default lives in code alone (`editor.theme`, `ai.command`) are left out because there is no
shipped text to hold them level with.

### A file that does not parse

A config file Varde cannot use **stops Varde from starting**, naming the file and the line and which
of three faults it was: the text is not TOML; a value is the wrong type (`install.macos = 12`, or an
`unanswerable` naming a request and no response); or an entry that parsed, typed, and is still
missing the one key it cannot be used without — a `[lsp.*]` or `[formatter.*]` row the merged
layers never gave a `command`, or a `[facts.*]` row with no `marker`. That last check is over the
merge, so a project that names only `args` for a shipped language is fine, and the fault is blamed on
the last file that mentioned the row.

The trade-off is deliberate: a broken `~/.varde/config.toml` locks you out until you fix it in
another editor, which is why the error names file *and* line.

## Every key

Types are TOML's. "Shipped" is the built-in bottom layer; "code" means the default is in the binary
and does not appear as TOML anywhere.

### `[view]`

| Key | Default | Type | Meaning |
|---|---|---|---|
| `double_tap_ms` | `300` | integer, ms | How long after a key is tapped a second tap of the same key still reads as a double-tap. Longer if a second press meant as one keeps arriving too late to count. This is the window for double-tapping Ctrl (or Esc from a hosted pane) to open the palette. |

### `[editor]`

| Key | Default | Type | Meaning |
|---|---|---|---|
| `tab_width` | `4` | integer | What Tab lays down while inserting, and what Enter falls back to when it opens a block in a file with no indentation of its own to copy. Indentation the file already has still wins there. |
| `minimap` | `true` | boolean | Whether the mirror of the file down the editor's right-hand edge is up when a project is opened. `:minimap` is the same switch from inside, and what it was left at is remembered per project and beats this. |
| `theme` | `"dark"` (code) | string | The editor's palette. `"light"` selects the light one; anything else is the dark one. |

### `[ai]`

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | `"claude"` (code) | string | What `:ai` starts in the AI pane when the project has no history. The command you last used in this project (`:ai <command>`) is remembered in `state.json` and wins over this. |

### `[risk]`

| Key | Default | Type | Meaning |
|---|---|---|---|
| `threshold` | `15` | integer | The cyclomatic complexity a function may reach before Risk names it. |
| `max_iterations` | `10` | integer | How many times the Refactor loop's Gate may hand a refactor back before it stops. A function the AI cannot get under the threshold is one to look at yourself, and an uncapped loop spends tokens discovering that. |
| `test_command` | unset | string | What the Refactor loop's Gate runs. Absent, the command is detected from the project's shape (`Cargo.toml` gives `cargo test`); a project whose shape names nothing refuses the loop rather than passing a Gate that ran nothing. |

### `[facts.<name>]`

A fact is a path on this machine that a server or formatter needs and that nobody can write down
once — the TypeScript SDK a Vue project pinned, a virtualenv's interpreter. Configuration says how to
find it; Varde searches for it every time a server is started; the answer fills any `${name}` in a
`[lsp.*]` or `[formatter.*]` row. The search runs from the directory of the file being served up to
the workspace root, nearest first, and never above it — so a monorepo package that installs its own
toolchain gets its own answer.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `marker` | required | string | A relative path looked for under each directory on the way up. The directory it is found in is the one that "configures the language". |
| `value` | `"marker"` | `"marker"` or `"directory"` | Whether the answer is the marker file itself or the directory holding it. |
| `command` | unset | string | A machine-wide fallback: a command on `PATH` to resolve (symlinks followed) when no marker is found. Both this and `command_marker`, or neither. |
| `command_marker` | unset | string | Where the marker sits relative to the directory holding that command's real file. It is checked, not assumed. |
| `install.<os>` | unset | string | What puts `command` on this machine, as on an `[lsp.*]` row. A server here without this fact offers it in Tools, and `i` runs it — or refuses with `needs-installer` when the package manager it starts with is not on `PATH`. |
| `optional` | `false` | boolean | Whether a server whose row names this fact starts without it. Required (the default) and unfound: the server is not started and its row reads `missing-requirement`. Optional and unfound: the argument or option that asked for it is dropped and the server starts as if the row never mentioned it. A found optional fact is filled in like any other. |

**Worked example — the two shipped facts.** `@vue/language-server` must be told its TypeScript SDK
with `--tsdk=<dir>`, and the directory differs per project and per machine:

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

Opening `src/App.vue` looks for `node_modules/typescript/lib/typescript.js` in `src/`, then the
project root; failing that, resolves where `tsc` on `PATH` really lives and checks for the file
beside it. Found, the server starts with `--tsdk=/your/project/node_modules/typescript/lib`. Not
found, no server starts and Tools says `missing-requirement` naming `typescript_sdk`; `i` on the
row runs the fact's `install`, and the next check starts the server, no restart needed. Nothing the
search finds is ever written into the file: the fact stays a search, so the row is right on every
machine and after every Node upgrade.

The second shipped fact, `vue_typescript_plugin`, is what lets the *TypeScript* server answer about
`.vue` files. It is `optional = true` because it is named on the `[lsp.typescript]` row every
TypeScript project shares: were it required, a machine with no Vue server would have no TypeScript
server anywhere.

Your own facts work the same way — `[facts.python_env]` with `marker = ".venv/bin/python"` and
`args = ["--stdio", "--pythonpath=${python_env}"]` on the Python row, for instance. A `${…}` that
names no `[facts.*]` table is not a placeholder and is passed through as written; `${HOME}` is just
a string.

### `[lsp.<language>]`

One table per language, the language being what the file's extension maps to (`rust`, `typescript`,
`vue`, `python`, …). Naming a server is not starting one: it is spawned when a file in that language
is open, and only if the command is on `PATH`. Tools (palette, `v`) shows every row and
its state — `installed`, `missing`, `stopped`, `no-install-command`, `missing-requirement`,
`partly-working` — with `i` to offer the install and `r` to re-check.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | required after the merge | string | The server binary. |
| `args` | `[]` | array of strings | Its arguments. `${fact}` names are filled. |
| `also_served_by` | `[]` | array of language names | Other languages' servers that also serve this language's files. A `.vue` file is served by the Vue server *and* the TypeScript server. |
| `install.macos`, `install.linux`, `install.windows` | per row | string | What installs the server on that OS. Run in the terminal pane when the row is taken in Tools. A row with no key for your OS says `no-install-command` and offers nothing. |
| `initialization_options` | unset | table | Handed to the server untouched at start-up, as JSON. Varde reads nothing inside it; `${fact}` values are filled, and a key whose value asked for an unfound optional fact is dropped. |
| `partial` | unset | string | What this server, installed and running, still cannot do — in your words. Shown on its row, which then reads `partly-working`. |
| `unanswerable.request`, `unanswerable.response` | unset | two strings, both or neither | A question this server puts to its client that Varde will not answer, and the method to refuse it on — so the server moves on instead of waiting forever. |

Shipped rows:

| Language | Command | Install (macOS · Linux · Windows) | Notes |
|---|---|---|---|
| `rust` | `rust-analyzer` | `rustup component add rust-analyzer` on all three | |
| `typescript` | `typescript-language-server --stdio` | `npm install -g typescript@6 typescript-language-server` on all three | `initialization_options.tsserver.path = "${typescript_sdk}/tsserver.js"`; a `@vue/typescript-plugin` entry at `${vue_typescript_plugin}` for `vue` files. |
| `javascript` | `typescript-language-server --stdio` | same | Same `tsserver.path`; no plugin. |
| `vue` | `vue-language-server --stdio --tsdk=${typescript_sdk}` | `npm install -g @vue/language-server` on all three | `also_served_by = ["typescript"]`; `unanswerable` = `tsserver/request` / `tsserver/response`. |
| `python` | `pyright-langserver --stdio` | `npm install -g pyright` on all three | |
| `go` | `gopls` | `go install golang.org/x/tools/gopls@latest` on all three | |
| `c`, `cpp` | `clangd` | Linux `sudo apt install clangd` · Windows `winget install LLVM.LLVM` | No macOS command: Homebrew's `llvm` is keg-only, so an install would still leave `clangd` unreachable by name. |
| `java` | `jdtls` | macOS `brew install jdtls` | Not packaged elsewhere. |
| `zig` | `zls` | macOS `brew install zls` | Linux is a build from source. |

A language nobody has packaged for an OS gets **no** install key rather than an invented one: a
command that fails looks configured, while a blank one is fixable in one line of TOML.

### `[formatter.<language>]`

The same shape for `:format`, which asks the language server first and this command second. Every
shipped command reads the text on stdin and writes the result on stdout — that is the only shape that
can format what you have not saved — so a tool that only rewrites files in place gets no row.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | required after the merge | string | The formatter binary. |
| `args` | `[]` | array of strings | Its arguments. `${file}` is the buffer's own absolute path; `${fact}` names are filled too. |
| `install.<os>` | per row | string | Typed onto the terminal line when `:format` finds the command missing; never run. |
| `extensions` | `[]` | array of strings | The file extensions this row claims, for languages no `[lsp.*]` table maps — `html`, `css`, `json`, `yaml`. Not needed where the language already has a server row. |

Shipped rows:

| Language | Command | Extensions | Install |
|---|---|---|---|
| `rust` | `rustfmt` | | `rustup component add rustfmt` |
| `python` | `black --quiet -` | | macOS/Linux `pipx install black` · Windows `pip install black` |
| `go` | `gofmt` | | none — ships with the toolchain |
| `javascript`, `typescript`, `vue` | `prettier --stdin-filepath ${file}` | | `npm install -g prettier` |
| `json` | same | `json`, `jsonc` | same |
| `yaml` | same | `yaml`, `yml` | same |
| `html` | same | `html`, `htm` | same |
| `css` | same | `css`, `scss`, `less` | same |
| `markdown` | same | `md`, `markdown` | same |

`--stdin-filepath ${file}` is how one `prettier` is told whether it is reading JSON or YAML — the
bytes do not say. A language with no row refuses `:format` out loud and names the table to write,
e.g. `[formatter.txt] in .varde/config.toml`.

### `[dap.<language>]`

The Debug adapter a language is debugged with ([debugging.md](debugging.md)), spoken to over its
standard input and output. The table name is the name a Launch configuration's `adapter` gives.

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

A named way to start a Debug session, allowed in the global file and the project's. A project entry
beats a global one of the same name, key by key.

| Key | Default | Type | Meaning |
|---|---|---|---|
| `adapter` | required after the merge | string | Which `[dap.*]` row runs the session. |
| `request` | required after the merge | string | `launch` to start the program, `attach` to join one that is running. |
| `args` | `{}` | table | Handed to the adapter as the request's arguments, untouched — each adapter documents its own. |

### `[speech]`

What reads a Selection aloud. Explained in full in [reading-aloud.md](reading-aloud.md).

| Key | Default | Type | Meaning |
|---|---|---|---|
| `command` | `"piper"` | string | The synthesizer. |
| `args` | `["--model", "${voice}", "--length-scale", "${scale}", "--noise-w-scale", "1.0", "--output_dir", "${dir}"]` | array of strings | `${voice}` is the row below, `${scale}` the reciprocal of `speed`, `${dir}` where the stream is written. |
| `voice` | `""` | string | Path to the voice model; a leading `~` is your home directory. Blank until the install fills it in. |
| `speed` | `1.0` | float | Multiplier, higher is faster. Applies to the next Reading. |
| `player.macos`, `player.linux` | `"afplay"`, `"aplay"` | string | What plays the stream. No `player.windows` is shipped. |
| `install.macos`, `install.linux` | shipped | string | Installs `piper` with `uv` and fetches the `en_US-bryce-medium` voice into `~/.varde/voices/`. Run in the shell pane when the row is taken in Tools. No `install.windows`. |
| `configures.voice` | `"~/.varde/voices/en_US-bryce-medium.onnx"` | string | Written into `voice` once the install exits 0, unless `voice` is already set. |

### Substitutions, in one place

| Placeholder | Filled in | With |
|---|---|---|
| `${<fact>}` | `[lsp.*].args`, `[lsp.*].initialization_options`, `[formatter.*].args` | The path a `[facts.<fact>]` search found. Unfound and required: the server is not started. Unfound and optional: the argument or option key carrying it is dropped. |
| `${file}` | `[formatter.*].args` | The absolute path of the buffer being formatted. |
| `${voice}`, `${scale}`, `${dir}` | `[speech].args` | The voice model path, `1 / speed`, and Varde's scratch directory. |

Nothing else is a placeholder. There is no environment-variable expansion and no template language.

### `install.<os>`: run when you take the row

Taking a row in Tools (`i`) is the whole install: the row is appended to `~/.varde/config.toml` if
the file lacks it, and its `install` key runs in the terminal pane, where you watch it and answer a
`sudo` prompt. Its exit status comes back to the row, which reads `install-failed` if it failed.
An install whose first program (or the one after `sudo`) is not on your `PATH` is not run at all:
the row reads `needs-installer` and names the package manager, which `install.sh` installs.
Elsewhere — `:format` finding its command missing, reading aloud — the command is still put on the
terminal pane's input line without Enter pressed. Nothing is installed because a file was opened.
Which key applies is the operating system the binary was built for. Once the command exists, the next check picks it up
with no restart — unless the installer only appended to your shell profile, in which case a restart is
offered, never taken.

## What else lives in `.varde/`

A project's `.varde/` folder holds more than the config. Which of it belongs in version control is
your decision — Varde never writes ignore rules — but this repo's own `.gitignore` is the recommended
split:

| Path | What it is | Commit? |
|---|---|---|
| `config.toml` | The project's settings, seeded commented-out | Yes — it is the project's answer, for everyone who opens it. |
| `reviews/NNNN.json` | Reviews you submitted from Review view, numbered, the newest 50 kept | Yes — authored artifacts. |
| `stories/<base>-<head>.json` | Story sets the AI authored for a range, the newest 10 kept | Yes — authored artifacts. |
| `state.json` | Per-user session state (below) | No — it is yours, not the project's. |
| `risk.json` | The Risk figure, cached against the commit it was measured at | No — derived, changes on every commit. |
| `snapshots/<n>/` | The Refactor loop's copy of the files an Iteration touched, so a failed Iteration can be put back | No — derived. |
| `refactor-done` | The sentinel the AI session writes to say an Iteration is finished; Varde deletes it before each one | No. |
| `lsp-<language>.log` | One transcript per language server, the place to look when a server would not start | No — per session. |

`state.json` is what Varde remembers about *you* in this project between sessions: the last view
(Edit, Review or Story); the AI command you last started; which tree folders were expanded; the tree
divider's position and the AI pane's width and shape (beside the editor or tall); which pane is in
the Corner beneath the tree; whether the editor is dimmed and whether the minimap is on; the open
buffers and which one was current. It is machine-written JSON and there is nothing in it to edit by
hand.

## The Bare workspace

`varde` with no folder argument opens the current directory as a **Bare workspace**: the folder is
still the workspace — the file tree is it and `:w` writes there — but nothing of Varde's is written
into it. There is no project config layer at all (a `.varde/config.toml` that happens to sit in the
folder is not read), nothing is seeded, Risk is not measured unasked, and everything that would have
gone in `.varde/` goes in a **Sidecar** under `~/.varde/paths/`, named for the folder and the
process and deleted when Varde exits. A Bare workspace therefore forgets everything between runs;
one that remembers is a project, and `varde .` is how you ask for that.

The one exception is a submitted review, which would otherwise be destroyed at exit: from a Bare
workspace it goes to `~/.varde/reviews/`, durable and shared by every Bare workspace, under the same
retention.

So `~/.varde/` holds: `config.toml` (yours), `voices/` (where the shipped install puts a model),
`tmp/` (a Reading's stream, swept on start), `paths/` (Sidecars) and `reviews/` (Bare-workspace
reviews). Varde creates the directories it needs and never edits the config.

## Versions and the Update notice

Varde is installed as a symlink into its own checkout (see [../install.md](../install.md)), so the
binary you are running and the source on disk can drift apart. On start Varde compares the
**Version** the checkout's `Cargo.toml` claims against the **Running version** the binary was built
from. When the checkout's is strictly newer, an **Update** exists, and one yellow line —
`Update available: palette u` — appears above the key reminder. Equal or older is not an Update, so
an old branch checked out is never offered as one.

Palette `u` (or `:update`) runs `cd <checkout> && cargo build --release` in the terminal pane; since
the symlink already names the file that build writes, the build *is* the install, and the next
launch is the new version. With no checkout above the binary it is a binary install instead: palette
`u` downloads the newer Release for this platform, verifies its checksum, puts it in place of the
running binary and relaunches Varde on it. With neither a checkout nor a newer Release, `:update`
refuses with `nothing-to-update` and does nothing.

The comparison is of version numbers, not commits, so a change committed without a version bump is
invisible here; that is accepted over a notice that fires on every commit and teaches you to ignore
it (`docs/adr/0003-versions-not-commits.md`).
