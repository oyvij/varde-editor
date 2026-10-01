# Getting around

Varde opens on a folder and presents it as a workspace: a file tree, an editor, your own shell and
an AI CLI, side by side in one terminal. This guide covers the layout around everything else: the
panes, the views, the palette, focus, the tree, open files, the terminal, the mouse, the clipboard,
quitting and updating. Editing itself is in [editing.md](editing.md). The AI pane has its own guide
in [ai-pane.md](ai-pane.md). Reviewing a change is in [review.md](review.md), narrating one is in
[stories.md](stories.md), and the Risk figure is in [risk.md](risk.md). Settings are in
[configuration.md](configuration.md) and installing is in [../install.md](../install.md).

## Starting

```sh
varde .            # the current folder is the workspace
varde ~/some/repo  # a folder somewhere else
varde              # a Bare workspace, see below
```

The folder you name becomes the workspace root, and its name is the title. An empty folder is a
fine workspace, since that is how a project begins. If Varde cannot use the path, it stops before
the TUI appears and says which of three things went wrong: the folder does not exist, the path is a
file, or the folder cannot be read.

The first start in a project creates `<project>/.varde/` and seeds a `config.toml` there with every
key commented out, so you can find every setting in that file. Varde leaves anything already in that
file alone on every later start. Varde writes per-project state on the way out (which folders were
expanded, which files were open, where the dividers are, the last view), so reopening puts things
back.

### A Bare workspace

`varde` with no folder opens the folder you are in, and writes nothing of Varde's into it: no
`.varde/`, no seeded config, nothing added to `.gitignore`. The folder is still the workspace. The
tree shows it, and `:w` writes there. But everything Varde keeps for itself goes into a Sidecar under
your own `~/.varde` instead. A Bare workspace reads `~/.varde/config.toml` and the built-in defaults,
and has no project configuration layer at all, even if the folder has a `.varde/config.toml`. It
measures no Risk at startup, though asking for it still works. Quitting deletes the Sidecar and saves
no session state. The one thing that survives is a submitted review, which goes to
`~/.varde/reviews/` rather than into the Sidecar. Leaving no trace in the folder should not mean
losing your work.

## Panes and views

Edit view has four panes:

| Pane | Where | What it holds |
|---|---|---|
| Files | left | the file tree, with a filter box on its bottom edge |
| Editor | centre | the current Buffer, its line numbers and the Cheatsheet in its top-right |
| Terminal | bottom | your shell, in one or more splits |
| AI | right | an AI CLI, or the box that starts one |

Beneath the tree there is one more slot, the Corner. It shows one thing at a time, or nothing: the
Risk list, the Buffers pane, the Cursor history, the Breakpoint list, the Diagnostic list or the
Conflict list. Opening one while another is showing replaces it.

The tree's top border counts the project's errors (red `✖`) and warnings (orange `▲`) as the
Language servers report them, and shows nothing when there are none. Clicking a count opens the
Diagnostic list on that Severity.

The palette switches between three views:

- **Edit**: the four panes above.
- **Review**: the left pane lists only the files git reports as changed, and the editor shows a
  read-only diff of the selected one. Entering Review selects the first changed file, and leaving it
  clears the diff. See [review.md](review.md).
- **Story**: the left pane holds a Spine of steps and the editor walks their Sites. See
  [stories.md](stories.md).

## The palette

The palette reaches every pane, view and project command, from anywhere:

| Gesture | Where it works |
|---|---|
| `Ctrl+Space` | every pane, hosted panes included. It is the one gesture that is the same everywhere |
| `Esc Esc` | inside a hosted pane (terminal or AI). Both escapes still reach the program running there |

The specification also names a Ctrl double-tap on terminals that report modifier presses, but Varde
does not currently ask terminals for that mode, so use the two above.

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
| | `m` | Merge conflicts | show or hide the Conflict list: every file git reports as unmerged and the Conflicts still in it; Enter goes to one. A file with none left is ticked `✓` and stays until you stage it yourself |
| | `a` | AI | focus the AI pane. It does not start a session |
| | `l` | Tall | swap the AI pane between beside the editor and the whole right-hand edge |
| Views | `e` | Edit | switch to Edit view |
| | `r` | Review | switch to Review view |
| | `s` | Story | switch to Story view |
| Project | `f` | Find | project-wide search, the same as `Ctrl+F` |
| | `v` | Tools | every program Varde runs, and what is installed ([language-intelligence.md](language-intelligence.md)) |
| | `n` | Launch | start a Launch configuration under the debugger ([debugging.md](debugging.md)) |
| | `c` | Collapse | close every open folder in the tree |
| Help | `h` | Keys | hide or show the Cheatsheet, the same as `:help` |
| | `u` | Update | update Varde, the same as `:update` |
| | `q` | Quit | leave Varde |

`Esc` cancels and changes nothing. Varde ignores a key that is not in the list, and the palette stays
open. Choosing the view already on screen redraws nothing but still moves focus, so `e` from the AI
pane takes you back to the editor. The palette has nothing that acts on the file or review in front
of you, such as copy, `:w` or `:submit`. Those belong to the view that handles them.

On a short terminal the palette first drops the gaps between groups, then the `Esc` line, and only
then an entry. When it drops an entry it shows `…` on the last row rather than cutting it off without
a sign. Every entry still answers its key whether or not it is drawn.

## Moving focus

Keys go to the pane that has focus. A pane that has no use for a key ignores it rather than passing
it on to the shell.

| Gesture | Effect |
|---|---|
| `Alt+h` `Alt+j` `Alt+k` `Alt+l` | focus the pane in that direction, from every pane |
| palette `o` `g` `d` `t` `k` `y` `a` | focus a pane by name |
| click | focus the pane under the pointer, and act |

The Alt keys are aliases, never the only way. On macOS, Option is not Alt unless you configure the
terminal that way (Ghostty: `macos-option-as-alt = true`), and a stock tmux strips modifier reports.
That is why the palette reaches every pane without a modifier. Inside the terminal strip,
`Alt+h`/`Alt+l` step through the splits before leaving the strip.

## The Cheatsheet

The box in the editor's top-right lists the keys for the current view. It lists every binding. Each
key that does something is either listed there or deliberately left off because every editor already
teaches it: the arrows, `hjkl`, `Home`/`End`, `Alt+←/→` for word motions, `Backspace`, and a count
typed before a motion.

A terminal cell holds one character, so the box hides the code under it. `:help` or palette `h`
shows and hides it. It starts hidden at every launch, and Varde does not remember the choice. On a
26-row terminal only the first sixteen rows fit, and those are the ones nothing else in Varde
teaches.

## The file tree

The tree shows the folder as it is: directories first, then files, each alphabetical, with every
dotfile shown, `.git/` included. Varde shows git-ignored paths dimmed, so build output does not look
like source. Varde reads a folder only when you expand it, so a huge `node_modules` costs nothing
until you open it. Varde remembers which folders are expanded per project.

| Key | Effect |
|---|---|
| `↑` `↓` | move the Row selection; landing on a file previews it in the editor without leaving the tree |
| `Enter` | expand or collapse a folder; open a file and move focus to the editor |
| click | the same, but focus stays in the tree |
| `/` | put the keyboard in the filter box |
| `c` | collapse every open folder (palette `c` from anywhere) |
| `n` `N` `d` | new file, new directory, delete, on the selected row |
| `-` | return the terminal to the project root |
| `→` `←` `Enter` `Esc` | step into the selected row's action icons, along them, run one, step out |

### Filtering

`/` opens the box on the tree's bottom edge. Matching is fuzzy and ranked, so `ftr` finds
`file_tree.rs`, closest first. But once any path contains exactly what you typed, the tree shows
only those. Only files match. Folders appear when they lead to a match, opened so the match is
visible, and the tree goes back to how it was when the filter clears. The filter searches the whole
project, not only the folders you have opened, and skips `.git`. The box shows the best match
completed ahead of what you typed, and narrows as you type. `Enter` selects that match in the tree,
opens the folders down to it, and clears the filter. It does not open the file, so you decide what
to do with it. `Esc` leaves and clears. With no match, the tree says so rather than going blank.

### Tree actions

Varde never touches the filesystem itself. Every tree action becomes a shell command in the
terminal, so the terminal is the one place files are created or removed, and you can read exactly
what ran. Paths are absolute, and quoted only when they need it.

The focused row shows its actions as icons on the right. A folder has new-file, new-directory,
go-here, search-here, delete and copy-path. A file has delete and copy-path. Click an icon, or press
`→` to step into them and `Enter` to run the highlighted one.

| Action | What runs | Runs by itself? |
|---|---|---|
| new file | a name box, then `touch <path>`, with `mkdir -p` first for a nested name | yes |
| new directory | a name box, then `mkdir -p <path>` | yes |
| delete | `rm <file>` or `rm -r <folder>` | yes, with no confirmation and no trash |
| go here | `cd <folder>` on the input line | no, it waits for your Enter |
| back to root (`-`) | `cd <root>` | yes |
| search here | opens project search limited to that folder | n/a |
| copy path | the absolute path onto the clipboard | n/a |

`Esc` in the name box runs nothing. Focus goes where the next step is. A `cd` that Varde typed moves
focus to the terminal, because you have to press Enter there. A command that already ran leaves
nothing to type, so focus returns to the tree with the created path selected or, after a delete,
the folder that held it. You can open a new file with `Enter` as soon as the watcher lists it.

### Following the disk

The tree follows the filesystem. Files created and deleted appear and disappear in the folders you
have expanded. A file created inside a collapsed folder is not listed until you expand it, because
Varde only watches expanded folders and does not know about it yet. Varde follows *open* files
whether or not their folder is expanded. A file appearing on disk never takes over the editor, and
neither does a branch checkout.

A Buffer with no unsaved edits follows its file without telling you. Varde never overwrites a Buffer
with unsaved edits. It flags it as diverged, and the status line says so and names the key. `D` in
normal mode opens a picker with three choices: `r` reloads from disk, `w` writes your edits over the
disk version, and `m` hands both versions to the AI session to merge. Only the first two clear the
flag. A merge leaves it until the AI's write arrives. `Esc` loses nothing. `:w` also writes whatever
the flag says, and `:e` reloads.

## Buffers

Opening a file keeps what is already open. There is no tab bar. Open files show in two places: a
Buffer mark on the file's tree row, and a row of dots on the editor's bottom edge, one per Buffer. A
mark is filled for the current Buffer or one with unsaved work, and hollow for one that is only
open. Unsaved and current have different colours.

| Gesture | Effect |
|---|---|
| `gt` `gT` | step forward or back through the Buffers, wrapping |
| click a dot | switch to that Buffer |
| palette `g` | the Buffers pane, listing every Buffer by path in the same order as the dots |

`gt`/`gT` need no modifier on purpose. `Alt+←`/`Alt+→` in the editor move by word, not by Buffer.

Switching selects the same file in the tree and opens the folders down to it, so the two always
agree. Browsing the tree with the arrows opens a *preview*. The next preview replaces it, and Varde
does not remember it for next time. `Enter` or a click keeps it open. Selecting a file that is
already open switches to it without re-reading it, and leaves its unsaved edits alone.

In the Buffers pane the arrows move a Row selection, `Enter` switches and moves focus to the editor,
and a click switches and leaves the keyboard in the pane. The pane has no actions, and a drag over
it selects nothing. Its row selection follows when you switch some other way, and Varde remembers
per project whether it is shown.

| Command | Effect |
|---|---|
| `:w` | write the Buffer |
| `:e` | reload it from disk, discarding the draft |
| `:q` | close the current Buffer; refused while *it* has unsaved edits |
| `:q!` | close it, discarding edits |
| `:wq` | write, then close |
| `:qa` | close every Buffer with nothing unsaved, and say which ones it kept |
| `:qa!` | close every Buffer, unsaved ones included |

Closing moves to a neighbouring Buffer. Closing the last one empties the editor and puts focus in
the tree. `:q` never quits Varde. Reopening a project brings back the files that were open.

## The terminal

The bottom strip is your shell, and it behaves like one: the tree's actions type into it, `Tab`
completes, and `Ctrl+K` and `Ctrl+L` do what readline does. It is a hosted pane, so almost every key
goes to the shell, including `Ctrl+Q` and `Ctrl+F`. To leave it, use `Alt`+direction, `Ctrl+Space`
or `Esc Esc`.

`:split` starts a second shell beside the one with the keyboard, in the folder that shell is
currently in, and the new one takes the keyboard. Splitting the middle of three puts the new shell
right after it. `Alt+h`/`Alt+l` step through the splits, stopping at the ends, and a click moves the
keyboard to a split. Keys, paste, a drag and the wheel all act on the split with the keyboard.
`exit` in a shell closes its split, and the keyboard moves to the last remaining one. When the last
shell exits, Varde exits. Splits share the strip's width evenly. You cannot stack splits or drag one
wider.

When Varde types a command into the terminal, such as a tree action's `touch` or a server install
line, it picks a shell whose prompt is waiting: the focused split if it is idle, otherwise the first
idle one. It never types into a shell running a job, where the text would become the job's input.
If every split is busy, Varde opens a new split and waits for its prompt.

## Mouse

| Gesture | Effect |
|---|---|
| click | focus the pane under the pointer, and act; the first click is never only a focus click |
| click a tree row | toggle a folder, open a file, keep focus in the tree |
| click in the editor | put the cursor there, and clear the Selection |
| double-click a word | select the word: a run of one character class, so whitespace is not a word |
| drag | select text in the editor or a pty pane; in the tree, move the Row selection |
| wheel | scroll the pane under the pointer, one row a notch, without moving focus or the selection |
| drag a divider | resize the panes; remembered per project. The AI pane's edge resizes it too |
| right-click | nothing. There are no context menus |

Scrolling the editor moves the cursor only when the view would otherwise leave it behind. The two
pty panes scroll their own history, unless the program running in one has asked for mouse events.
Then Varde reports clicks and the wheel to it in the encoding it asked for, which keeps vim, htop
and lazygit usable in the terminal. A full-screen AI CLI has no history behind it, so it scrolls
itself. See [ai-pane.md](ai-pane.md) for what a hosted pane does with a click.

## Copy and paste

There is exactly one Selection in the workspace. A mouse drag and a keyboard extend (`Shift`+arrow,
`W`/`B`, `V`) produce the same thing, and it is what copying copies. The editor selects characters
across lines, the terminal selects its scrollback, and the AI pane selects only what is on its
screen. A drag in the tree moves the Row selection and selects no text. A click clears what a drag
selected, and copying with nothing selected does nothing.

| Key | Effect |
|---|---|
| `Ctrl+C` or `Cmd+C` | copy the Selection to the system clipboard |
| `Ctrl+V` or `Cmd+V` | paste the clipboard into the Buffer as one edit, in whatever mode it is in |

Ctrl and Command are aliases on both. Over SSH, with no system clipboard to reach, copying falls
back to the terminal's clipboard protocol (OSC 52), so the text still reaches the machine you are
sitting at, if that terminal supports it. iTerm2 asks you to allow clipboard access, Apple's
Terminal ignores it, and a tmux in between needs `set -g set-clipboard on`. A paste keeps its line
breaks whatever style the source used. A Preview refuses a paste with a message. A diff or a walked
Site pastes nothing rather than editing a file you are not looking at. `y` and `p` use the register,
which is separate from the clipboard. Yanking also copies, but putting does not read the clipboard.

In a hosted pane, `Ctrl+C` copies only while a Selection exists in *that* pane. With nothing
selected it interrupts the child, as it does in any terminal.

## Quitting

| Gesture | Effect |
|---|---|
| `Ctrl+Q` | quit, from any pane that is not hosting a program |
| palette `q` | quit, from anywhere, hosted panes included |

Varde refuses to quit while any Buffer has unsaved edits, and says so. Quitting is deliberately not
a `:` command: `:q` closes a Buffer and `:qa` closes them all, so a mistyped clean-up command cannot
end the session. In a hosted pane `Ctrl+Q` reaches the child, so the palette is the gesture that
works everywhere. Varde writes per-project state on the way out.

## Staying up to date

On a source install, Varde is a symlink on your PATH pointing at the release binary inside its own
checkout, so an ordinary release build *is* the install. The downside is drift: the checkout moves
ahead while the binary stays old. So at launch Varde compares the Running version it was compiled
from with the Version in its checkout's manifest. When the checkout is strictly ahead, there is an
Update. Varde shows no notice for it. Instead, the version tag at the right end of the bottom row,
which always shows the Running version, turns blue and reads `v0.2.0 → v0.3.0  C-space u to
update`. A checkout behind the binary is not an Update, so checking out an old branch never asks you
to downgrade. The check is one file read, with no network and no git.

| Gesture | Effect |
|---|---|
| `:update` or palette `u` | run `cd <checkout> && cargo build --release` in the terminal |
| `:update` or palette `u`, binary install | download the newer Release, verify it, replace the binary and relaunch |

The command names Varde's checkout, not the open workspace, and it works whether or not Varde offers
an Update. It starts no AI session and opens or writes no file. Everything happens in the terminal,
where you can read the compiler's output.

A binary install has no checkout, so `:update` fetches the Release it found at startup instead,
checks the download against the Release's published checksums, and swaps it in for the running
binary. If the download is bad, Varde refuses it and keeps the old binary. Varde then relaunches
with the same arguments, unless a buffer is unsaved, in which case it refuses the same way quitting
does. Save and run `:update` again, and it relaunches without downloading twice. With neither a
checkout nor a newer Release, Varde tells you there is nothing to update from, and runs nothing.

`cargo build --release` overwrites the file the symlink names, so the next `varde` you launch is the
new one. The current session keeps running the old binary until you restart it. A failed build
writes nothing, and the last version that compiled stays where it was. This setup has two
consequences: the editor is whatever the checkout last compiled successfully, and `cargo clean`
deletes the installed binary. See [../install.md](../install.md) for reclaiming build space without
doing that.
