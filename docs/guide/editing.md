# Editing

The editor is modal, the way vim is: normal mode moves and deletes, insert mode types, and `Esc`
returns you to normal mode. You can reach every motion without a modifier key. Where a `Ctrl` or
`Alt` binding exists, it is an alias for a key you can press on its own, so nothing depends on how
your terminal reports Option or on what a multiplexer strips.

The keys below are the ones a buffer answers in Edit view. `:help` shows the cheatsheet inside the
editor. A few keys are deliberately left off it, and this page marks them **unlisted**.

## Modes

| Key | Does |
|---|---|
| `i` | insert at the cursor |
| `a` | insert after the cursor |
| `o` / `O` | open a line below / above and insert |
| `Esc` | back to normal mode; also abandons a half-typed command and clears a Selection |

Typing in normal mode never inserts. Varde shows a half-typed command, such as `2d` while you decide
what to delete, on the editor's bottom edge beside the cursor position. It clears when the command
completes, and `Esc` abandons it. An operator followed by a key that names no motion (`dq`) is
refused, and Varde names the key.

## Moving

| Key | Does |
|---|---|
| `h` `j` `k` `l` | one cell left, down, up, right (normal mode; **unlisted**) |
| arrows | the same, in **every** mode, including while inserting (**unlisted**) |
| `w` `b` `e` | start of next word, start of previous word, end of word |
| `0` `$` | start and end of line; `Home` / `End` are aliases (**unlisted**) |
| `gg` `G` | first and last line |
| `Alt+Left` / `Alt+Right` | aliases of `b` / `w`, and what macOS sends for Option+arrow (**unlisted**) |
| `1`–`9` | a count: `3j` moves three lines, `9j` past the end stops at the end (**unlisted**) |

The cursor cannot leave the buffer in any direction. When a line is wider than the pane, the view
slides sideways to keep the cursor visible and slides back when it returns. Line numbers stay put.

## Operators

`d` (delete) and `y` (yank) combine with the motions above rather than having their own key each.
Whatever you delete or yank goes into the register, and `p` puts it back.

| Keys | Does |
|---|---|
| `x` | delete the character under the cursor |
| `dd` | delete the line |
| `dG` / `dgg` | delete from the cursor to the end / start of the file |
| `db` | delete the word behind the cursor |
| `d2j` | a count belongs to the motion: delete this line and the two below |
| `yy` | yank the line |
| `p` | put the register below the cursor |
| `2dd` | a count repeats an edit |
| `u` | undo; every compound edit here undoes in one press |

Deleting fills the register too, so `ddp` swaps two lines. `p` reads the register, never the system
clipboard. Yanking also copies to the clipboard (see below), so the register is for moving text
inside the workspace and the clipboard is for taking it out.

`Alt+Backspace` while inserting deletes the word behind the cursor as one edit (**unlisted**). It
stays inside the line. On an indented line it takes the indentation, and at the start of a line it
joins with the line above, as plain `Backspace` does. In normal mode `db` does the same delete with
no modifier, which is why `Alt+Backspace` is not bound there.

## Typing

- **Backspace** deletes backwards and joins with the line above at the start of a line.
- **Enter** copies the current line's indentation to the new line as-is: tabs stay tabs, three
  spaces stay three. Between the halves of a bracket pair it opens a block. The closing half moves
  to its own line and the cursor lands one level deeper. That level is the file's own shallowest
  indentation. Only a file with no indentation to copy falls back to `editor.tab_width`.
- **Tab** while inserting inserts one level of `editor.tab_width` (four spaces unless a config layer
  says otherwise) as one edit. With characters selected, `Tab` indents the lines the Selection
  covers and `Shift+Tab` unindents them, by the file's own unit. In normal mode `Tab` is not an
  edit. While an accepted completion has blanks left, `Tab` means "next blank" instead. See
  [Language intelligence](language-intelligence.md#candidates).
- **Pairs**: typing `(`, `[`, `{` or a quote inserts its partner and leaves the cursor between them.
  Typing the closing half where it already is steps over it. `Backspace` inside an empty pair
  deletes both. A quote inside a word stays a single quote. A pair typed around a Selection wraps
  it. None of this applies in normal mode, and none of it applies to a paste.
- **Typing over a Selection** while inserting replaces the selected characters with what you type,
  and `Backspace` deletes them.

## Selecting and copying

There is exactly one **Selection** in the workspace, and it is what copying copies. A mouse drag, a
keyboard extend and `V` all produce the same thing.

| Keys | Does |
|---|---|
| `Shift+arrow` | extend a charwise Selection from the cursor, across line breaks |
| `W` / `B` | extend a word forward / back; `Shift+Alt+arrow` is the alias (**unlisted**) |
| `V` | select the current line; motions (`j`, `k`, `G`, `gg`) extend it linewise |
| `d` / `y` on a Selection | delete or yank exactly what is selected |
| `Ctrl+C` / `Cmd+C` | copy the Selection to the system clipboard |
| `Ctrl+V` / `Cmd+V` | paste the clipboard |
| `Esc`, a plain arrow, a click | clear the Selection |

Ctrl and Command are aliases on copy and paste. Copying with nothing selected does nothing. Copying
uses the system clipboard and falls back to the terminal's own clipboard protocol (OSC 52), so it
still reaches your machine when Varde runs over SSH. Yanking with `y` copies to the clipboard as well
as filling the register.

Double-clicking a word selects it. Dragging selects characters in the editor, scrollback in the
terminal, and the visible screen in the AI pane. In the file tree a drag only moves the row
selection, because a filename is not text you copy character by character.

Varde **echoes** a selected word: it marks every other exact, case-sensitive occurrence of it in the
file, more faintly than the selection itself. Separately from any Selection, resting the cursor in a
word marks that whole word wherever it occurs in the file. It matches whole words only, with the case
as written, so `state` inside `stated` is a different word. Neither marking is a Selection. Nothing
is selected and nothing is copied.

To have a Selection read aloud, see [Reading aloud](reading-aloud.md).

### Pasting

A paste into the buffer is **one edit, in whatever mode the buffer is in**. Varde never replays it
as keystrokes, so pasted code containing an `i` or a `dd` goes in as text rather than running as
commands. Every line-ending style becomes a line break, pasted brackets do not auto-close, and the
whole paste undoes in one `u`. A Preview refuses a paste with a message, rather than editing a file
whose characters you cannot see.

When one of Varde's own input lines has the keyboard (the `/` search line, the `:` command line, the
tree filter), Varde types a paste into that line character by character, and a newline in it is
`Enter`. So a multi-line paste onto the `/` line submits at its first line break.

### Editing every occurrence at once

`Ctrl+D` (or `gm`, with no modifier) adds the next occurrence of the selected word to the Selection.
With nothing selected, it takes the word under the cursor first, so it also works while inserting.
It takes occurrences in order, wrapping from the bottom of the file to the top, until it has all of
them. Typing then replaces all of them at once (in normal mode the first character starts
inserting), and every further keystroke lands at each one. `Esc`, a motion or a click drops the
extra occurrences along with the Selection. A Selection spanning a line break names no word to take.
The whole multi-occurrence edit undoes in one `u`.

## Finding

### In this file

`/` opens the search on the editor's bottom line, the same line `:` commands use, and searches the
file you are editing. The cursor moves to the closest match as you type each character, so you can
stop typing as soon as you get there. Varde highlights every match, not only the current one.

| Key | Does |
|---|---|
| `Enter` | keep the position and leave the found text as the Selection |
| `Esc` | go back to where the search started, clearing the highlights |
| `n` / `N` | next / previous match, wrapping at either end; each becomes the Selection |

Matching is literal and case-insensitive unless the query contains a capital. Because `Enter` leaves
a Selection, you can copy the found text with `Ctrl+C` or send it to the project search with `gr`
without retyping it.

### In the project

`Ctrl+F`, or `f` in the palette, opens a full-screen results box over every pane. From the editor,
`*` searches for the word under the cursor, and `gr` searches for the Selection, or the word under
the cursor if there is none. The search icon on a folder in the file tree opens the same box limited
to that folder, and the limit stays until you close the box.

Varde groups hits by file, alphabetically, with line numbers and the matching line. It searches open
buffers as they are on screen, unsaved edits included. Matching is literal, so `foo(` needs no
escaping, and case-insensitive unless the query has a capital. Varde caps results at 500 hits, and
the count says so.

Inside the box every printable key goes into the query, so the actions need a modifier. The box
lists its own keys on a dim row under the count:

| Key | Does |
|---|---|
| arrows | step hit by hit |
| `Ctrl+N` / `Ctrl+P` | step file by file: the first hit of the next file, or the top of this one |
| `Enter` | open the selected hit at its line, with the word selected |
| `Ctrl+O` | open every file with a hit |
| `Tab` | complete the query to the most common word in the hits that starts with it |
| `Esc` | close |

The wheel scrolls the list wherever the pointer is. A click marks the hit under the pointer without
opening it, and `Enter` opens it. A click on a file heading marks nothing.

## Going back

Reading code means following it, to a definition three files away or to a search hit, and then
coming back. Varde records a **Visit** for every **Jump**: opening a file, switching Buffer, an
in-file search landing (`n`, `N`, `Enter` on a query), `gg` and `G`, and `gd` to a definition.
Arrows, `j`/`k` and clicks are motions and record nothing, and browsing the tree does not count
either. Varde records the place you are *leaving*, so going back returns you to where you were.

| Keys | Does |
|---|---|
| `Ctrl+P` / `Ctrl+N` | jump back / forward |
| `gp` / `gn` | the same, with no modifier |
| `Ctrl+Alt+Left` / `Ctrl+Alt+Right` | the same, for a hand already on the arrows (**unlisted**) |

At either end of the history, Varde says so rather than doing nothing. It also records where you
were when you first went back, so forward can return there. A Jump made in the middle of the history
is added to the end rather than discarding what was ahead. Varde records the same place twice in a
row only once, and caps the list.

`y` in the palette opens the **Cursor history** pane in the Corner beneath the file tree, replacing
whatever was there. Each row names the file, an excerpt of the line the cursor was on, and the line
number, coloured the way the editor colours that file. If a row's line no longer holds what it
recorded, Varde dims the row rather than presenting it as current. `Enter` on a row goes there, and
a click goes there and leaves the keyboard in the pane. The Visits are from this session only. Varde
remembers whether the pane is shown, but not its contents.

## Folding

`:toggle` folds the innermost block the cursor is in, or opens it again, and `:toggle!` folds every
block in the file at once and opens them all on the next use. A line that opens a block has a toggle
in its gutter, which a click also flips. The cursor steps over a folded block rather than into it,
and `Enter` with the cursor on the fold's dots opens it. With no file open, Varde refuses the
command with a message.

## Merge conflicts

Varde draws a Conflict that a merge left in the file over its text. Each marker line becomes a dim
bar naming its side (`Current change (HEAD)`, `Common ancestor`, `Incoming change`), the current side
is tinted green and the incoming side blue. The text itself is unchanged, and the marker the cursor
is on shows as its raw text, so you can edit it by hand.

With the cursor anywhere inside a Conflict, `cc` keeps the current side, `ci` the incoming side, and
`cb` both, current first. The bar's `[accept current]`, `[accept incoming]` and `[accept both]` do
the same with a click. Each one undoes with one `u`. Accepting only edits the buffer: Varde saves
nothing and never stages anything. Palette `m` lists every unmerged file's Conflicts.

## Reading the shape of the code

**Indent guides** run down each level of indentation, and Varde draws every space as a dot, so you
can see a line's shape without counting. The guide of the block the cursor is in is heavier than the
rest. Varde also marks the innermost bracket pair the cursor is inside, and only that pair, so the
rest of the punctuation stays plain.

**Syntax highlighting** follows the file extension across roughly 220 syntaxes, including
TypeScript, TSX, Vue, Svelte, Kotlin, Swift, Zig, Dart, TOML, Terraform, Nix, Dockerfile, GraphQL,
SCSS and Varde's own `config.toml`. An unknown extension, or none, renders as plain text. Colours
come from `editor.theme` in configuration (`dark` unless set to `light`).

`:dim` toggles the background behind the code. It is a shade darker than the rest of the TUI, which
raises every token's contrast. If you use a transparent terminal and want the desktop to show through
the code, turn it off. Varde remembers the choice per project.

The **change bar** is a mark in the gutter beside every line the last commit does not have: an
inserted or an edited line, saved or not, so a line typed a second ago counts as a change. A file
the commit has no copy of shows no bar rather than a bar down every line, and deleted lines leave no
mark. The gutter's one column shows one mark at a time: a diagnostic first, a Reading's place
second, the change bar last.

## Minimap

The minimap is a small copy of the whole file down the editor's right-hand edge. Each row covers two
source lines and each cell four source columns, so you can see the shape of the file (its blocks,
its runs of comments, where the code ends) without reading it. It scrolls so the part you are
looking at is always inside it. A line down its first column, the **Slider**, marks that part, and
lights up while the pointer is over the minimap.

Press or drag in the minimap to move through the file. The cursor moves with the view, and a drag
that started there selects no text wherever it goes. `:minimap` turns it off and on again, giving
its columns back to the text, and Varde remembers the choice per project. `editor.minimap = false`
in configuration starts a project without one. A pane with fewer than twenty columns of text left
has no minimap, and neither does a Preview, whose rows are not lines.

## Buffers, files and disk

Opening a file keeps what is already open. `gt` / `gT` step through the open Buffers, and the dots
on the editor's bottom edge are clickable. See [Getting around](getting-around.md).

| Command | Does |
|---|---|
| `:w` | write the buffer to disk |
| `:e` | discard the draft and reload from disk |
| `:q` | close this buffer; refuses while *this* buffer has unsaved edits |
| `:q!` | close it, discarding edits |
| `:wq` | write, then close |
| `:qa` | close every buffer with nothing unsaved; names the ones it kept |
| `:qa!` | close every buffer, discarding edits |

Quitting Varde is not a `:` command. It is `Ctrl+Q`, or `q` in the palette, so a mistyped clean-up
command cannot end the session.

A buffer with no unsaved edits follows the file on disk without telling you. Varde never overwrites
a buffer **with** unsaved edits. When the file changes underneath it, Varde flags the buffer and the
status line says so. `:w` still writes, because the flag is a warning, not a lock, and `:e` takes
the disk version. `D` in normal mode opens a picker with three choices: `r` reloads from disk, `w`
overwrites the disk with your edits, and `m` hands both versions to the AI pane to merge (the buffer
stays flagged until the AI's write arrives). `Esc` leaves the difference as it is. `D` on a buffer
that matches the disk says there is nothing to resolve, and while inserting `D` is just a letter.

## Markdown Preview

A `.md` or `.markdown` file opens as a **Preview**, the document laid out to the pane width. Every
other file opens as **Source**. `:preview` toggles between the two for this buffer only. On anything
that is not markdown, or with no buffer open, it refuses in the footer.

A Preview renders headings, wrapped paragraphs, emphasis, lists, tasks, block quotes, tables, fenced
and indented code (highlighted in the language the fence names, never wrapped), link text with the
URL hidden, images as their alt text, literal HTML set apart, and YAML frontmatter shown above the
document rather than hidden. Varde draws a `mermaid` fence as a diagram. When it cannot (an
unsupported type, a parse error, too wide for the pane), it states the reason and shows the fence's
source beneath it. Links are styled text. A `[[wikilink]]` opens its file with `gd`, in the Preview and in
source: see [knowledge.md](knowledge.md#follow-a-wikilink).

The Preview is read-only. `a o O x dd p u V` do nothing and say so. Motions work over the **rendered
rows** (`h l 0 $ w b e gg G`, the arrows and the word-motion arrows), so `$` stops at the end of the
text on screen and a word motion crosses a wrap that the source does not have. `/` searches the
rendered text, `Shift+arrow` selects rendered rows, and a copy takes the text exactly as drawn,
including the newlines from wrapping. There is no line-number gutter, and the pane title says
`preview`.

`i` switches to Source and starts inserting, for when you want to change something. `:preview`
switches in normal mode. Switching keeps the line and starts at column one in either direction. `:w`
and every other `:` command still work over a Preview. `:format` switches to Source first, so you
can see its undo. A Preview shows whatever the buffer holds, so edits made in Source appear as soon
as you switch back.

## Configuration

Two keys under `[editor]` in `~/.varde/config.toml` or `<project>/.varde/config.toml`, with the
project file overriding key by key. See [Configuration](configuration.md):

```toml
[editor]
tab_width = 4      # what Tab lays down, and the block indent when the file holds none to copy
minimap = true     # whether the mirror is up in a project nobody has turned it off in
```

## See also

- [Language intelligence](language-intelligence.md): hover, definitions, diagnostics, completion
  and `:format`.
- [Getting around](getting-around.md): the palette, panes, focus and buffers.
- [Reading aloud](reading-aloud.md): `:read` speaks the Selection.
- [Configuration](configuration.md): the layered `config.toml` these keys live in.
