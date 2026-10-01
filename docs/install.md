# Installing Varde

Varde installs two ways. The **binary** is the default: the script downloads a prebuilt `varde` for
this platform from the latest GitHub Release, verifies it and puts it at `~/.local/bin/varde`. It
needs no toolchain and updates itself from inside the editor (`:update`, ADR 0017). **From source**
is a checkout, a release build and a symlink on your PATH pointing at it. It is for anyone working on
Varde, and the ordinary release build *is* the install. Neither way adds anything to your shell
configuration.

## The scripted way

`install.sh` at the repo root does either, and sets up what Varde needs around it, interactively:

```sh
curl -fsSL https://raw.githubusercontent.com/oyvij/varde-editor/main/install.sh | bash
```

It reads its prompts from `/dev/tty`, so it can ask questions while piped from `curl`. With no
`varde` on PATH it asks **binary or source**, binary by default. Then it installs Varde, writes the
global config, asks about package managers, and offers the default AI CLI, the speech player and the
URL opener. It prints every command before it runs it.

**The binary.** The script fetches `SHA256SUMS` and `varde-<os>-<arch>` from the latest Release,
checks the asset's SHA-256 against its line, and only then writes it to `~/.local/bin/varde` as a
real file, not a symlink. A missing asset, a failed download or a checksum mismatch aborts with a
message and installs nothing. Only `curl` is required.

**From source.** The script requires git, a C compiler and `cargo` via rustup (declining any of
these aborts), then clones or runs `git pull --ff-only`, runs `cargo build --release` and creates the
symlink. Run from inside a clone (`./install.sh` after `git clone`), it never asks binary or source.
It builds that clone and never clones again. If you answer "source" when piped from `curl`, it asks
where the checkout should live, defaulting to `~/.varde/src`. If the server refuses a clone, the
script offers to retry over SSH, for a private fork.

**Re-run**, it updates whichever kind it finds behind `varde`. It pulls and rebuilds a symlink into a
checkout's `target/release`. Anything else is a binary install, and it replaces that in place with
the latest Release. `VARDE_REPO` overrides the repository for a fork. The script derives both the
clone URL and the Release the binary comes from from it.

**The config.** Every run leaves a `~/.varde/config.toml`. When none is there, the script writes what
`varde --default-config` prints. That is the same template Varde seeds the file with when it starts
and finds none: every setting commented out and every program row live (ADR 0018). The script never
replaces an existing file. If the binary is too old to print the template, the script reports that
rather than leaving a partial file.

**The script does not install language servers, formatters or the voice.** You take each one from
Tools inside Varde, one key per row. Tools writes the row, runs its install in the shell pane and
fills in what the install configures (ADR 0018). The script only makes sure their install commands
can run. It asks the `varde` it just installed, on either path, with `varde --deps`. That lists the
rows `~/.varde/config.toml` names, or the template's rows when that file does not exist yet. The
script takes the package manager each `install.<os>` starts with: the first word, or the one after
`sudo`. Tools uses the same rule to mark a row `needs-installer`. For each manager this machine
lacks, the script asks one `y/N` and names the rows that need it:

```
npm is used to install javascript, python, typescript, vue, and the prettier formatters. Install npm with: sudo apt install -y nodejs npm ? [y/N]
```

A row that needs a new manager gets its prompt with no change to the script. The one table the
script still owns is how to install each manager (`installer_install`), since a package manager
cannot install itself. If a manager is missing from that table, the script says it cannot install it
on this OS. On macOS, a manager that comes from `brew` asks about `brew` first. Debian's npm, which
is what `apt` installs, has a global prefix only root can write. Every `npm install -g` row would
fail with `EACCES`, so the script offers `npm config set prefix ~/.local`. Putting `sudo` in the rows
instead would be wrong for a Homebrew or nvm npm.

`install.sh` itself names only what the edge runs *without* configuration: the build toolchain, git,
the default AI CLI (`claude`), the speech player's package (`alsa-utils`, on Linux, since Tools
offers no install for it) and the URL opener. **When a feature adds a program Varde shells out to,
it is either a row with an `install.<os>` key or a line in the script, and `./install.sh --list`
shows whether the script sees it.** `--list` asks whichever `varde` is installed, so it needs no
source.

`varde --deps` prints one line per row, tab-separated as `kind`, `name`, `command`, `install`. The
install column is this OS's `install.<os>`, or blank. The kinds are `lsp`, `formatter`, `speech`,
and `player` for the speech row's `player.<os>`. It does not list a speech command or player that no
file names. If Varde would refuse to start on a config file, `--deps` stops with the same file and
line. It needs no folder and no terminal, and exits before touching either. That is how a machine
with no checkout learns what to offer.

The script does not cover Windows. `PROGRAMS` has `install.windows` rows for a hand install.

## From source, by hand

From your checkout, build once and link the result into a directory that is already on your PATH:

```sh
cargo build --release
ln -sfn "$(pwd)/target/release/varde" ~/.local/bin/varde
```

`$(pwd)` makes this work from wherever you cloned into. The symlink records your own absolute path,
so a different checkout location needs no configuration and no edits. `~/.local/bin` is on PATH on
the target machine. If yours is somewhere else, substitute it. Confirm with `command -v varde`, which
should print the symlink's path.

That is the whole installation. From any folder in any terminal:

```sh
varde .            # open the current folder as the workspace
varde ~/some/repo  # open a folder somewhere else
```

## On a source install, the build is the install

`cargo build --release` writes to the same file the symlink already names, so a build updates the
installed command. There is no copy step and no reinstall to forget:

```sh
git pull && cargo build --release
```

A build that fails to compile writes no binary. The previous one stays where it was, so a broken
checkout never leaves you without a working editor. You keep the last version that compiled until
the next one does.

## What the symlink costs

The symlink has two consequences, and both are deliberate:

- **The running editor is whatever the checkout last compiled successfully.** A commit that builds
  but misbehaves gives you an editor that misbehaves. No second copy of the binary is kept as a
  known-good version.
- **Cleaning the build directory uninstalls Varde.** `cargo clean` deletes `target/`, which holds the
  file the symlink points at, and `varde` stops working until you build again.

A build fixes both, so neither is worth keeping a second copy of the binary.

## Reclaiming build space

Don't use `cargo clean` here, because it deletes the file the symlink names. The two halves of
`target/` have opposite risks, so never delete them together.

- **`target/release` is the installation.** Deleting it uninstalls Varde until the next build. The
  in-editor update runs `cargo build --release` in the checkout, which overwrites the binary in place
  and never needs a clean first. Nothing accumulates here: one binary, overwritten.
- **`target/debug` is disposable.** It holds the test-suite artifacts, and nothing on your PATH
  points into it. `rm -rf target/debug` costs one recompile of the suite and cannot break the
  installed editor.

The growth to watch for is in `target/debug/deps`, and it happens on macOS. Mach-O keeps DWARF in
the loose `.rcgu.o` object files a compilation emits, not in the binary, so those files have to
survive the link. Cargo never deletes them. Their names carry a hash of the code they came from, so
every recompile writes a *new* set instead of replacing the last one. This repo's edit-test loop
recompiles constantly, so the pile has no upper bound. Left alone for a year it reached 862,000
object files and 157 GB, against 38 MB of repository.

`debug = 0` in `[profile.dev]` stops it. With no debug info there are no object files to keep. The
cost is line numbers in panic backtraces. Function names remain, which is enough to find a failing
scenario. If a debugging session needs more, ask for it on the command line rather than in the
manifest, and expect a full rebuild either way:

```sh
RUSTFLAGS="-Cdebuginfo=1 -Csplit-debuginfo=packed" cargo test
```

`packed` is the important half. It runs `dsymutil` to gather the debug info into a single `.dSYM`
bundle that each build replaces, so a session under a debugger does not leave another pile behind.

Object files were most of it but not all. The artifact *names* carry a hash of the compilation
inputs, and the package version is one of them. This repo bumps the version on every commit, so each
commit builds a fresh set of test binaries and abandons the last. After 191 commits there were 489 of
them, 143 MB for the cucumber binary alone. Cargo does not delete those either.

That one is not worth a tool. A full rebuild of the whole suite from an empty `target/debug` takes 59
seconds, so delete the directory whenever it bothers you and let the next `cargo test` repopulate it:

```sh
rm -rf target/debug     # never `cargo clean`, which takes the installed binary with it
```
