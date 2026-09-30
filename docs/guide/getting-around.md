# Getting around

Varde opens on a folder and presents it as a workspace: a file tree, an editor, your own shell and
an AI CLI, side by side in one terminal. This guide covers the frame everything else sits in — the
panes, the views, the palette, focus, the tree, open files, the terminal, the mouse, the clipboard,
quitting and updating. Editing itself is in [editing.md](editing.md); the AI pane has its own guide
in [ai-pane.md](ai-pane.md); reviewing a change is [review.md](review.md), narrating one is
[stories.md](stories.md), and the Risk figure is [risk.md](risk.md). Settings are in
[configuration.md](configuration.md) and installing is in [../install.md](../install.md).

## Starting

```sh
varde .            # the current folder is the workspace
varde ~/some/repo  # a folder somewhere else
varde              # a Bare workspace, see below
```

The folder you name becomes the workspace root and its name is the title. An empty folder is a
perfectly good workspace — that is how a project begins. A path Varde cannot use stops it before the
TUI appears, and says which of three things went wrong: the folder does not exist, the path is a
file, or the folder cannot be read.

The first start in a project creates `<project>/.varde/` and seeds a `config.toml` there with every
key commented out, so the settings are discoverable in place. Anything already in that file is left
alone on every later start. Per-project state — which folders were expanded, which files were open,
where the dividers sit, the last view — is written on the way out, so reopening puts things back.

### A Bare workspace

`varde` with no folder opens the folder you are standing in, and writes nothing of Varde's into it:
no `.varde/`, no seeded config, nothing added to `.gitignore`. The folder is still the workspace —
the tree is it, and `:w` writes there — but everything Varde keeps for itself goes into a Sidecar
under your own `~/.varde` instead. A Bare workspace reads `~/.varde/config.toml` and the built-in
defaults, and has no project configuration layer at all, even if a `.varde/config.toml` happens to
sit in the folder. It measures no Risk at startup (asking for it still works). Quitting deletes the
Sidecar and saves no session state. The one thing that survives is a submitted review, which goes to
`~/.varde/reviews/` rather than into the Sidecar, because losing an output is a different kind of
nothing from leaving no trace.

## Panes and views

Edit view has four panes:

| Pane | Where | What it holds |
|---|---|---|
| Files | left | the file tree, with a filter box on its bottom edge |
| Editor | centre | the current Buffer, its line numbers and the Cheatsheet in its top-right |
| Terminal | bottom | your shell — one or more splits |
| AI | right | an AI CLI, or the box that starts one |

Beneath the tree there is one more slot, the Corner. It holds one occupant at a time — the Risk
list, the Buffers pane, the Cursor history, the Breakpoint list or the Diagnostic list — or nothing;
asking for one while another is showing replaces it.

The tree's top border counts the project's errors (red `✖`) and warnings (orange `▲`) as the
Language servers report them, and says nothing when there are none. Clicking a count opens the
Diagnostic list on that Severity.

The palette switches between three views:

- **Edit** — the four panes above.
- **Review** — the left pane lists only the files git reports as changed, and the editor shows a
  read-only diff of the selected one. Entering Review lands on the first changed file; leaving it
  clears the diff. See [review.md](review.md).
- **Story** — the left pane holds a Spine of steps and the editor walks their Sites. See
  [stories.md](stories.md).

## The palette

One gesture reaches every pane, view and project command, from anywhere:

| Gesture | Where it works |
|---|---|
| `Ctrl+Space` | every pane, hosted panes included — the one gesture that is the same everywhere |
| `Esc Esc` | inside a hosted pane (terminal or AI). Both escapes still reach the program running there |

The specification also names a bare Ctrl double-tap on terminals that report modifier presses, but
Varde does not currently ask terminals for that mode, so count on the two above.

The palette is grouped. Press the letter, or click the row:

| Group | Key | Entry | What it does |
|---|---|---|---|
| Panes | `o` | Editor | focus the editor pane, leaving the view alone |
| | `g` | Buffers | show or hide the Buffers pane in the Corner |
| | `d` | Files | focus the file tree |
| | `t` | Terminal | focus the terminal |
| | `k` | Risk | show or hide the Risk list in the Corner ([risk.md](risk.md)) |
| | `y` | Cursor history | show or hide the list of places you jumped from ([editing.md](editing.md)) |
| | `b` | Breakpoints | show or hide every Breakpoint in the workspace: Enter goes to one, `d` removes it, `D` clears them all |
| | `i` | Diagnostics | show or hide every Diagnostic the Language servers have reported, one Severity at a time: `e` `w` `i` `h` or a label on its border switch Severity, Enter goes to one |
| | `a` | AI | focus the AI pane — it does not start a session |
| | `l` | Tall | swap the AI pane between beside the editor and the whole right-hand edge |
| Views | `e` | Edit | switch to Edit view |
| | `r` | Review | switch to Review view |
| | `s` | Story | switch to Story view |
| Project | `f` | Find | project-wide search, the same as `Ctrl+F` |
| | `v` | Tools | everything Varde runs, and what is installed ([language-intelligence.md](language-intelligence.md)) |
| | `n` | Launch | start a Launch configuration under the debugger ([debugging.md](debugging.md)) |
| | `c` | Collapse | close every open folder in the tree |
| Help | `h` | Keys | take the Cheatsheet down or put it back, the same as `:help` |
| | `u` | Update | rebuild Varde from its checkout, the same as `:update` |
| | `q` | Quit | leave Varde |

`Esc` cancels and changes nothing. A key that is not in the list is ignored and the palette stays
open. Choosing the view already on screen redraws nothing but still moves focus, so `e` from the AI
pane is a way back to the editor. What is *not* here is anything that asks something of the file or
review in front of you — copy, `:w`, `:submit` — those belong to the view that answers them.

On a short terminal the palette drops the gaps between groups first, then the `Esc` line, and only
then an entry, which it announces with `…` on the last row rather than clipping in silence. Every
entry still answers the keyboard whether or not it is drawn.

## Moving focus

Keys go to the pane that has focus, and a pane with nothing to do with a key swallows it rather than
letting it fall through to the shell.

| Gesture | Effect |
|---|---|
| `Alt+h` `Alt+j` `Alt+k` `Alt+l` | focus the pane in that direction, from every pane |
| palette `o` `g` `d` `t` `k` `y` `a` | focus a pane by name |
| click | focus the pane under the pointer, and act |

The Alt keys are aliases, never the only route. On macOS, Option is not Alt unless the terminal is
told so (Ghostty: `macos-option-as-alt = true`), and a stock tmux strips modifier reports — the
palette reaches every pane without a modifier for exactly that reason. Inside the terminal strip,
`Alt+h`/`Alt+l` step through the splits before leaving the strip.

## The Cheatsheet

The box in the editor's top-right lists the keys for the view on screen. It is the contract for
what is bindable: every key that does something is either listed there or deliberately left off
because every editor teaches it — the arrows, `hjkl`, `Home`/`End`, `Alt+←/→` for word motions,
`Backspace`, and a count typed before a motion.

A terminal cell holds one character, so the box hides the code under it. `:help` or palette `h`
puts it up and takes it down. It starts hidden at every launch, and the choice is not remembered.
On a 26-row terminal only the first sixteen rows fit; the rows that survive are the ones nothing else in Varde teaches.

## The file tree

The tree is an honest view of the folder: directories first, then files, each alphabetical, with
every dotfile shown — `.git/` included. Git-ignored paths are shown but dimmed, so build output does
not read as source. Folders are read lazily: a folder is walked only when you expand it, so a huge
`node_modules` costs nothing until you open it. Expansion state is remembered per project.

| Key | Effect |
|---|---|
| `↑` `↓` | move the Row selection; landing on a file previews it in the editor without leaving the tree |
| `Enter` | expand or collapse a folder; open a file and move focus to the editor |
| click | the same, but focus stays in the tree |
| `/` | put the keyboard in the filter box |
| `c` | collapse every open folder (palette `c` from anywhere) |
| `n` `N` `d` | new file, new directory, delete — on the selected row |
| `-` | return the terminal to the project root |
| `→` `←` `Enter` `Esc` | step into the selected row's action icons, along them, run one, step out |

### Filtering

`/` opens the box on the tree's bottom edge. Matching is fuzzy and ranked — `ftr` finds
`file_tree.rs`, closest first — but once any path contains what you typed literally, only those are
shown. Only files match; folders appear when they lead to a match, opened so the match is visible,
and the tree is left as it was found when the filter clears. The filter walks the whole project, not
just the folders you have opened, skipping `.git`. The box shows the best match completed ahead of
what you typed, narrowing as you go. `Enter` selects that match in the tree, opening the folders
down to it, and clears the filter; it opens nothing — what to do with the file is yours to say. `Esc`
leaves and clears. With no match the tree says so rather than going blank.

### Tree actions

Nothing in Varde touches the filesystem itself. Every tree action becomes a shell command in the
terminal, so the terminal is the one place a file is made or removed and you can read exactly what
ran. Paths are absolute and quoted only when they need it.

The focused row shows its actions as icons on the right. A folder offers new-file, new-directory,
go-here, search-here, delete and copy-path; a file offers delete and copy-path. Click an icon, or
press `→` to step into them and `Enter` to run the one highlighted.

| Action | What runs | Runs by itself? |
|---|---|---|
| new file | a name box, then `touch <path>` — `mkdir -p` first for a nested name | yes |
| new directory | a name box, then `mkdir -p <path>` | yes |
| delete | `rm <file>` or `rm -r <folder>` | yes — no confirmation, no trash |
| go here | `cd <folder>` on the input line | no — it waits for your Enter |
| back to root (`-`) | `cd <root>` | yes |
| search here | opens project search scoped to that folder | — |
| copy path | the absolute path onto the clipboard | — |

`Esc` in the name box runs nothing. Focus follows what is left to do: an injected `cd` takes the
terminal, because the Enter that finishes it has to land there; a command that ran leaves nothing to
type, so focus returns to the tree with the created path selected — or, after a delete, the folder
that held it. A new file is ready to open with `Enter` as soon as the watcher lists it.

### Following the disk

The tree follows the filesystem: files created and deleted appear and disappear, in the folders you
have expanded. A file created inside a collapsed folder is not listed until you expand it — lazy
watching means Varde genuinely does not know about it yet. What is *open* is followed whether or not
its folder is expanded. A file appearing on disk never takes over the editor; a branch checkout does
not either.

A Buffer with no unsaved edits follows its file silently. A Buffer with unsaved edits is never
overwritten: it is flagged as diverged and the status line says so, naming the key. `D` in normal
mode opens a picker with three ways out — `r` reload from disk, `w` write your edits over the disk
version, `m` hand both versions to the AI session to merge. Only the first two settle it; a merge
leaves the flag standing until the AI's write comes back. `Esc` loses nothing. `:w` also writes
regardless of the flag, and `:e` reloads.

## Buffers

Opening a file keeps what is already open — there is no tab bar. What is open shows in two places:
a Buffer mark on the file's tree row, and a strip of dots on the editor's bottom edge, one per
Buffer. A mark is filled for the Buffer you are in or one with unsaved work, hollow for one merely
open; unsaved is coloured differently from current.

| Gesture | Effect |
|---|---|
| `gt` `gT` | step forward or back through the Buffers, wrapping |
| click a dot | switch to that Buffer |
| palette `g` | the Buffers pane, listing every Buffer by path in the order the dots draw them |

`gt`/`gT` need no modifier on purpose. `Alt+←`/`Alt+→` in the editor move by word, not by Buffer.

Switching selects the same file in the tree, opening the folders down to it, so the two never
disagree. Browsing the tree with the arrows opens a *preview*, which the next preview replaces and
which is not remembered for next time; `Enter` or a click makes it stay. Selecting a file that is
already open switches to it — it is not re-read, and its unsaved edits are untouched.

In the Buffers pane the arrows move a Row selection, `Enter` switches and follows into the editor, a
click switches and leaves the keyboard in the pane. The pane has no actions and a drag over it picks
nothing. Its row selection follows wherever you switch some other way, and its visibility is
remembered per project.

| Command | Effect |
|---|---|
| `:w` | write the Buffer |
| `:e` | reload it from disk, discarding the draft |
| `:q` | close the Buffer in front of you — refused while *it* has unsaved edits |
| `:q!` | close it, discarding edits |
| `:wq` | write, then close |
| `:qa` | close every Buffer with nothing unsaved, and say which ones it kept |
| `:qa!` | close every Buffer, dirty ones included |

Closing moves to a neighbouring Buffer; closing the last one empties the editor and puts focus in
the tree. `:q` never quits Varde. Reopening a project returns to the files that were open.

## The terminal

The bottom strip is your shell, and it behaves like one: the tree's actions type into it, `Tab` is
completion, `Ctrl+K` and `Ctrl+L` are readline's. It is a hosted pane, so almost every key goes to
the shell — including `Ctrl+Q` and `Ctrl+F`. The way out is `Alt`+direction, `Ctrl+Space` or
`Esc Esc`.

`:split` starts a second shell beside the one with the keyboard, in the folder that shell is
currently in, and the new one takes the keyboard. Splitting the middle of three puts the new shell
right after it. `Alt+h`/`Alt+l` step through the splits, stopping at the ends; a click moves the
keyboard to a split. Keys, paste, a drag and the wheel are all about the split with the keyboard.
`exit` in a shell closes its split, and the keyboard falls back to the last remaining one. The last
shell exiting is how Varde ends. Splits divide the strip's columns evenly — there are no stacked
splits and no dragging one wider.

A command Varde pushes at the terminal — a tree action's `touch`, a server install line — goes to a
shell whose prompt is waiting: the focused split if it is idle, else the first idle one, never to a
shell running a job where it would become the job's input. With every split busy a new shell is
split off and the command waits for its prompt.

## Mouse

| Gesture | Effect |
|---|---|
| click | focus the pane under the pointer, and act — there is no dead first click |
| click a tree row | toggle a folder, open a file, keep focus in the tree |
| click in the editor | put the cursor there, and clear the Selection |
| double-click a word | select the word — the run of one character class, so whitespace is no word |
| drag | select text in the editor or a pty pane; in the tree, move the Row selection |
| wheel | scroll the pane under the pointer, one row a notch, without moving focus or the selection |
| drag a divider | resize the panes; remembered per project. The AI pane's edge resizes it too |
| right-click | nothing. There are no context menus |

Scrolling the editor takes the cursor with it only when the view would otherwise leave it behind.
The two pty panes scroll their own history — unless the program running in one has asked for mouse
events, in which case clicks and the wheel are reported to it in the encoding it asked for. That is
what keeps vim, htop and lazygit usable in the terminal. A full-screen AI CLI has no history behind
it; it scrolls itself. See [ai-pane.md](ai-pane.md) for what a hosted pane does with a click.

## Copy and paste

There is exactly one Selection in the workspace. A mouse drag and a keyboard extend (`Shift`+arrow,
`W`/`B`, `V`) produce the same thing, and it is what copying copies. The editor selects characters
across lines; the terminal selects its scrollback; the AI pane selects only what is on its screen; a
drag in the tree moves the Row selection and selects no text. A click drops what a drag picked;
copying with nothing selected does nothing.

| Key | Effect |
|---|---|
| `Ctrl+C` or `Cmd+C` | copy the Selection to the system clipboard |
| `Ctrl+V` or `Cmd+V` | paste the clipboard into the Buffer as one edit, in whatever mode it is in |

Ctrl and Command are aliases on both. Over SSH, with no system clipboard to reach, copying falls
back to the terminal (OSC 52) so the text still lands on the machine you are sitting at — if that
terminal honours it: iTerm2 asks you to allow clipboard access, Apple's Terminal ignores it, and a
tmux in between needs `set -g set-clipboard on`. A paste
keeps its line breaks however the source spelled them. A Preview refuses a paste out loud; a diff or
a walked Site pastes nothing rather than editing a file nobody is looking at. `y` and `p` are the
register's, a separate world from the clipboard — yanking also copies, putting does not read the
clipboard.

In a hosted pane, `Ctrl+C` copies only while a Selection exists in *that* pane; with nothing
selected it interrupts the child, as it does in any terminal.

## Quitting

| Gesture | Effect |
|---|---|
| `Ctrl+Q` | quit, from any pane Varde interprets |
| palette `q` | quit, from anywhere — hosted panes included |

Quitting is refused while any Buffer has unsaved edits, and says so. It is deliberately not on the
`:` line: `:q` closes a Buffer and `:qa` clears them up, so a mistyped clear-up cannot take the
session with it. In a hosted pane `Ctrl+Q` reaches the child, so the palette is the gesture that
works from everywhere. Per-project state is written on the way out.

## Staying up to date

Varde is a symlink on your PATH pointing at the release binary inside its own checkout, so an
ordinary release build *is* the install. The cost is drift: the checkout moves ahead and the binary
keeps being the old one. So at launch Varde compares the Running version it was compiled from with
the Version its checkout's manifest claims, and when the checkout is strictly ahead there is an
Update. Nothing is announced as a notice; the version tag at the right end of the bottom row, which
always shows the Running version, turns blue and reads `v0.2.0 → v0.3.0  C-space u to update`. A
checkout behind the
binary is not an Update, so checking out an old branch never nags you to downgrade. The check is one
file read — no network, no git.

| Gesture | Effect |
|---|---|
| `:update` or palette `u` | run `cd <checkout> && cargo build --release` in the terminal |
| `:update` or palette `u`, binary install | download the newer Release, verify it, replace the binary and relaunch |

The command names Varde's checkout, not the open workspace, and it works whether or not an Update is
offered. It starts no AI session, opens and writes no file — everything happens in the terminal
where the compiler's output is readable.

A binary install has no checkout, so `:update` fetches the Release it found at startup instead,
checks the download against the Release's published checksums, and swaps it in for the running
binary. A bad download is refused and the old binary stays. Varde then relaunches with the same
arguments — unless a buffer is unsaved, which is refused the way quitting is; save and `:update`
again, and it relaunches without downloading twice. With neither a checkout nor a newer Release you
are told there is nothing to update from and nothing runs.

`cargo build --release` overwrites the file the symlink names, so the next `varde` you launch is the
new one; the session you are in keeps running the old binary until you restart it. A build that
fails writes nothing, and the last version that compiled stays exactly where it was. Two things
follow from the arrangement: the editor is whatever the checkout last compiled successfully, and
`cargo clean` deletes the installed binary — see [../install.md](../install.md) for reclaiming build
space without doing that.
