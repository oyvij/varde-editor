# The global config is the list of programs Varde runs

`startup::DEFAULTS` has been the bottom layer of F9's merge since ADR 0011, and ADR 0012 defended it
at length: shipped defaults reach every machine on upgrade, and nothing is ever written to the
user's config file. That design delivered what it promised, and the promise turned out to be the
wrong one. What runs was split between a string compiled into the binary and a file on disk. Nobody
could read the whole answer in one place. And one piece of it could not be configured at all:
which file belongs to which language server was a `match` in `lsp::language`, so a server for a
language that `match` did not name could not be added at all.

This decision splits the settings by kind. **A setting is a number Varde cannot work without; a
program is a row naming something Varde starts.** The two get opposite answers.

## Settings stay built in

`view.double_tap_ms`, `editor.tab_width`, `editor.minimap`, `risk.threshold`, `risk.max_iterations`
and `speech.speed` keep their values in the binary, beaten by `~/.varde/config.toml` and that by the
project's `.varde/config.toml`, key by key — F9 exactly as it is. The code needs a value whatever
any file says, and a key missing from a file must never break the editor. Everything ADR 0012 said
about frozen values is true here and still decides it: these are the numbers a later release is
most likely to correct.

## Programs live only in the file

`[lsp.*]`, `[formatter.*]`, `[facts.*]` and every `[speech]` key except `speed` are read from
`~/.varde/config.toml` (and, above it, the project's `.varde/config.toml`) **and from nowhere else**.
A row that is not in a file does not run. There is no bottom layer for them: what the file says is
what Varde starts, and what `varde --deps`, `install.sh --list` and the Tools list report.

The binary still carries the rows, as a **template**: the text written into `~/.varde/config.toml`
when there is none. That happens in two places, and both write the same text — `install.sh`, which
asks the binary for it (`varde --default-config`), and Varde's own start, as an ordinary seeded
write when the edge read no global layer. Deleting the file gets it back on the next start. It is
never written over an existing file.

Every program row in the template is **live**, not commented. A commented row would mean a server
the user already installed does nothing until they find its line and remove a `#`, and the probe on
`PATH` already makes an uninstalled row harmless: its command is missing, so it is reported, not
started.

## A language is a row, not a match arm

Every `[lsp.<language>]` row names the files it serves: `extensions = ["rs"]`. `lsp::language` is
deleted, and the table name is the language id the protocol is sent — `rust`, `typescript`, `c`, the
ids the LSP specification lists. Where the specification gives one row's extensions different ids,
the row's `language_ids` names them: a `.tsx` file is `typescriptreact`. A server for Ruby is then
four lines of TOML and no release:

```toml
[lsp.ruby]
command = "ruby-lsp"
extensions = ["rb"]
```

ADR 0011 chose to name servers from the *file's* side (`also_served_by`) rather than have each
server declare the extensions it fancies, because the thing needing two answers is the file. That
reasoning survives unchanged: `extensions` says which row *owns* an extension, and `also_served_by`
still says who else answers about that row's files. An extension claimed by two `[lsp.*]` rows is a
`ConfigError` naming both rows. It is never resolved by order: the rows come from a merge, and
"whichever table serde saw first" is not something a reader can predict.

Formatter rows already carry `extensions` where `lsp::language` could not answer; they now carry it
on every row, since there is nothing left to ask first. The two tables do not consult each other: a
file's server and its formatter are separate choices, so a `.rs` extension named in both is two
facts, not a duplicate.

What stays in code is what Varde *links* rather than *runs*. The Risk metric's parser
(`rust_code_analysis`) and the syntax highlighter are libraries in the binary, so choosing one for a
file is not a program row and is outside this decision.

## Tools: one list of everything Varde runs

The palette's Servers list (`v`) becomes **Tools**, and it lists everything Varde runs, grouped by
kind: language servers, formatters, requirements (`[facts.*]`) and speech (the synthesizer with its
voice, and the player). Formatters had no list at all before: `:format` ran them, and nowhere showed
which were installed. There is one list rather than one per kind because every row has the same
shape: a status, an install, and a config write. Three lists would repeat one piece of code three
times.

Every row's status uses the same words: `installed`, `missing`, `missing-requirement`,
`needs-installer`, `install-failed`, `no-install-command`, `partly-working`, `stopped`. Every row is
taken with the same key.

Tools shows the rows the file names **and** every template row the file does not have. The second group is marked as available, not configured. The same rule answers
two situations: a row the user deleted, and a row a newer Varde added to its template. So an upgrade
never edits the file, and a new server still reaches the user. It shows up in the list, and it
becomes theirs when they take it.

Taking a row is one keypress and does everything:

1. **It writes the row.** If the file does not have it, it is appended from the template, together
   with any `[facts.*]` row it names.
2. **It runs the row's `install.<os>`** in the shell pane, the way a clone runs
   (`docs/adr/0015-a-clone-is-the-users-own-git.md`). The command's output is on screen, and a
   `sudo` password prompt can be answered where it appears.
3. **It learns how the install ended.** The command is run as `<install>; echo $? > <sentinel>`,
   so the file watcher that is already running reports the exit status. An install that failed says
   so on its row. It is never silent.
4. **The probe does the rest.** The next pass finds the command on `PATH`, and the server starts as
   it would on a fresh start (ADR 0012's consequence). Nobody restarts anything.

The write never touches an existing table. It appends a whole table whose name the file does not
have, through `toml_edit`, so every comment and every hand edit around it survives. If the file does
not parse, Varde writes nothing and runs nothing, and the refusal names the `ConfigError`. Taking a
row the file already has writes nothing and only runs the install.

**This reverses ADR 0012's "the command is typed, never run".** Its reasons were real: an install
changes a machine Varde does not own, is often `sudo` or a global `npm`, and a default wrong for this
distribution could be fixed on the input line first. What answers them now is where it runs, not
whether Varde presses Enter. The shell pane is visible and interactive, so the network request is
still one the reader watches and a `sudo` still asks the reader. A wrong default fails on screen
with its exit status recorded, and the row is one edit away from right. What the reader asked for
by taking the row was the install. Stopping one Enter short of it only added a step.

**A missing package manager is refused before anything runs.** The program an install command
starts with, or the one after `sudo`, is probed on `PATH` like any command. If it is missing, the
row reads `needs-installer` and names it (`npm`, `uv`, `go`), and nothing is written or run. Package
managers are `install.sh`'s job, as the next section explains.

## install.sh installs Varde and the package managers, and nothing else

`install.sh` stops asking about language servers, formatters and the voice. They are all installed
from Tools inside Varde, one keypress each, when the reader wants them. Nothing is preinstalled.
The script gets faster and has fewer ways to fail, since each extra install was another network
request and another prompt before Varde had even started.

What it still does:

- Installs Varde itself, from a Release or from source.
- Writes `~/.varde/config.toml` from the template when it is missing.
- **Asks about each package manager the template's install commands use** (`npm`, `uv`, `go`,
  `brew`, and on Linux `apt`'s packages), y/N. Each question explains why it is asked by naming
  the rows that need that manager: "npm is used to install typescript, javascript, vue, python,
  and the prettier formatters". The list comes from `varde --deps`, so a new row that needs a new
  manager is asked about with no change to the script.
- The toolchain for a source build, git, the default AI CLI and the URL opener, as before.

Which package managers the template needs is data the binary reports. Which command installs each
package manager is the one table `install.sh` still owns (`installer_install`), because a package
manager cannot be installed by itself.

## The template covers many languages, and is not the boundary

The template ships rows for as many languages as have a language server that can be installed with
one command: the mainstream ones, and a long tail with it — shell, Lua, Ruby, PHP, Kotlin, Swift,
C#, Haskell, OCaml, Elixir, Erlang, Scala, Clojure, Dart, Julia, R, Nix, Terraform, Dockerfile,
TOML, SQL, Svelte, Astro, GraphQL, protobuf, CMake, LaTeX, Elm, Gleam, Nim, Perl, PowerShell,
Typst and more. Each is a row and nothing else, held by the same test as every row: every row
parses, and no two `[lsp.*]` rows claim one extension.

The list is a helper, not a boundary. A language the template does not know is a row the reader
writes, as the Ruby example above shows, and it is treated exactly like a template row once it is in
the file. So Tools stays useful: finding a server and installing it takes one key, whether or not
Varde has heard of the language before.

A row whose install command is wrong for some machine is a bug in data, not in code. Fixing it is a
template edit and a release, and readers who already have the row keep their own copy, as the
consequences below state.

## A requirement offers its own install

A `[facts.*]` row may carry `install.<os>` like a program row:

```toml
[facts.typescript_sdk]
marker = "node_modules/typescript/lib/typescript.js"
value = "directory"
command = "tsc"
command_marker = "../lib/typescript.js"
install.linux = "npm install -g typescript"
```

A server whose command is installed but whose required fact finds nothing reads
`missing-requirement`. Until now that row offered no install, on the theory that the missing piece
was a directory in the workspace that no package manager provides. In practice the usual cause was
machine-wide: `typescript-language-server` on `PATH` with no classic `tsc` beside it, or `tsc` from
the TypeScript 7 native preview, which has no `typescript.js`. The fix was always one command, and
the row gave the reader nothing to act on. So the row now names the missing fact and offers the
fact's install, run the same way a server's is. A fact with no install
for this OS offers none, and the row still says which fact is missing.

A fact travels with the rows that name it. Taking an available server row whose values name
`${typescript_sdk}` appends `[facts.typescript_sdk]` from the template in the same write, if the
file lacks it, so no row is written that names a fact nothing declares. A fact needs no
`configures`: once its install puts `tsc` on `PATH`, the next search finds the SDK beside it.

A fact stays a search, not a path. It is written into the file as a declaration of where to look,
so the same row is correct on every machine and after every Node upgrade. Nothing in it is
per-computer, and install writes no found path back into it.

## An install writes what it configures

Some installs put a value on disk that the row then has to name. The voice is the case that exists:
the speech install downloads a model to `~/.varde/voices/`, and `speech.voice` stayed blank, so the
reader ran the install, pressed read, and was offered the same install again. Writing a config row
is not enough when the row itself needs the install's result.

So a row may carry `configures`, the keys its install makes true, written into
`~/.varde/config.toml` once the install exits with status 0:

```toml
[speech]
install.linux = "uv tool install piper-tts && mkdir -p ~/.varde/voices && curl … "
configures.voice = "~/.varde/voices/en_US-bryce-medium.onnx"
```

The value is a path the row's own install command names, not one this machine happened to have.
So this is not the resolved-path write ADR 0012 rejected: the same row writes the same value on
every machine, because its install puts the file there on every machine. A `~` is expanded when the
value is read, never written expanded.

A key the file already sets is left alone, by the rule that governs every write here: the reader's
value beats the template's. `configures` never touches a key the reader chose.

A successful exit is still not proof that the file is there now: it may have been deleted since.
So the edge reports whether the voice file exists, a fact it tells the core as
it tells `player_installed`, and a `voice` naming a missing file reads `no-voice` exactly as a blank
one does, with the install offered again. The loop closes: install, then read, and the second press
speaks.

Speech is also offered where it is needed. Pressing read with speech not installed refuses with a
notice naming what is missing, and one key takes the speech row, the same act as taking it in Tools.
A reader who never opened Tools is still one key from reading aloud.

The template's `voice` stays blank, for ADR 0013's reason: a 61MB file on somebody else's disk is
not a value to claim before the install that puts it there. `configures` is what fills it in, at
the moment that install succeeds. `install.sh` no longer installs the voice or writes this key:
speech is taken from inside Varde like every other row.

## What this supersedes

ADR 0012's sections "Nothing is ever written to the user's config file" and "The command is
typed, never run", and its first paragraph's claim that install commands live only inside the
binary. Its argument against writing *resolved
paths* survives in full. A `[facts.*]` row in the file is a search that runs at every spawn, not an
answer, so it is still never written with a path this machine happened to have. So is its argument
against a self-update touching the file: the update replaces the binary, and new rows arrive through
the list, not a migration.

ADR 0011's "the defaults ship as TOML data in `startup::DEFAULTS` — the bottom layer of F9's merge",
for program rows, and `Availability::Unmet`'s "it offers no install". Its falsifiable form carries
over in a new place: search `src/` for a server's, formatter's or package manager's name, and every hit must be in the template, a fixture or a comment.

## Consequences

**The file is long, and that is the feature.** Every server Varde knows about is in it, readable,
with its install command beside it. Nothing Varde starts is invisible.

**A corrected row does not reach a file that already has it.** When a release fixes a server's
`args`, a user who already has that row keeps the old one. This is the cost ADR 0012 predicted, and
it is accepted: the list shows the template's version beside a row that differs from it, and taking
it again replaces nothing — the user reads the difference and edits their own row. A marker on
rows Varde wrote, so it could refresh the ones nobody has edited, was considered and rejected:
it is a migration with a second author for one file, which is what ADR 0012 warned about.

**A broken global file stops every program.** Before, a syntax error in `~/.varde/config.toml`
still left the built-in servers running. Now it leaves none running, and the `ConfigError` is the
whole story. That is the same refusal Varde already gives, and it is honest: Varde no longer knows
which servers the user wants.

**Taking a row is the whole install.** The row, its facts, what its install configures and the
install itself come from one keypress. What is left to the reader is a `sudo` password, a missing
package manager (which `install.sh` offers), and languages whose server nothing packages for this
OS.
