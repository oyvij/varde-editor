# Editing

The editor is modal, the way vim is: normal mode moves and deletes, insert mode types, and `Esc`
returns you to normal mode. Every motion is reachable without a modifier key — where a `Ctrl` or
`Alt` binding exists it is an alias for a key you can press bare, so nothing depends on how your
terminal reports Option or on what a multiplexer strips.

The keys below are the ones a buffer answers in Edit view. `:help` shows the cheatsheet inside the
editor; a few keys are deliberately left off it and are called out here as **unlisted**.

## Modes

| Key | Does |
|---|---|
| `i` | insert at the cursor |
| `a` | insert after the cursor |
| `o` / `O` | open a line below / above and insert |
| `Esc` | back to normal mode; also abandons a half-typed command and clears a Selection |

Typing in normal mode never inserts. A half-typed command — `2d` while you decide what to delete —
is shown on the editor's bottom edge beside the cursor position, clears when it completes, and is
abandoned by `Esc`. An operator over a key that names no motion (`dq`) is refused by name.

## Moving

| Key | Does |
|---|---|
| `h` `j` `k` `l` | one cell left, down, up, right (normal mode; **unlisted**) |
| arrows | the same, in **every** mode — including while inserting (**unlisted**) |
| `w` `b` `e` | start of next word, start of previous word, end of word |
| `0` `$` | start and end of line; `Home` / `End` are aliases (**unlisted**) |
| `gg` `G` | first and last line |
| `Alt+Left` / `Alt+Right` | aliases of `b` / `w`, and what macOS sends for Option+arrow (**unlisted**) |
| `1`–`9` | a count: `3j` moves three lines, `9j` past the end stops at the end (**unlisted**) |

The cursor cannot leave the buffer in any direction. When a line is wider than the pane the view
slides sideways to keep the cursor visible and slides back when it returns; line numbers stay put.

## Operators

`d` (delete) and `y` (yank) compose with the motions above rather than having their own key each.
What is deleted or yanked goes into the register, and `p` puts it back.

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
| `u` | undo — every compound edit here is one press |

Deleting fills the register too, so `ddp` swaps two lines. `p` reads the register, never the
system clipboard; yanking also copies to the clipboard (see below), so the register is for moving
text inside the workspace and the clipboard for carrying it out.

`Alt+Backspace` while inserting deletes the word behind the cursor as one edit (**unlisted**). It
stays inside the line — on an indented line it takes the indentation, and at the start of a line
it joins with the line above, as plain `Backspace` does. In normal mode `db` is the same delete
with no modifier, which is why `Alt+Backspace` is not bound there.

## Typing

- **Backspace** deletes backwards and joins with the line above at the start of a line.
- **Enter** carries the current line's indentation down, copied as-is — tabs stay tabs, three spaces
  stay three. Between the halves of a bracket pair it opens a block: the closing half moves to its
  own line and the cursor lands one level deeper. That level is the file's own shallowest
  indentation, and only a file with none to copy falls back to `editor.tab_width`.
- **Tab** while inserting lays down one level of `editor.tab_width` (four spaces unless a config
  layer says otherwise) as one edit. With characters picked, `Tab` indents the lines the Selection
  covers and `Shift+Tab` brings them back out, by the file's own unit. In normal mode `Tab` is not
  an edit. While an accepted completion has blanks left, `Tab` means "next blank" instead — see
  [Language intelligence](language-intelligence.md#candidates).
- **Pairs**: typing `(`, `[`, `{` or a quote inserts its partner and leaves the cursor between them.
  Typing the closing half where it already sits steps over it; `Backspace` inside an empty pair
  takes both; a quote inside a word stays a single quote; a pair typed around a Selection wraps it.
  None of this applies in normal mode, and none of it applies to a paste.
- **Typing over a Selection** while inserting replaces the picked characters with what you type,
  and `Backspace` deletes them.

## Selecting and copying

There is exactly one **Selection** in the workspace, and it is what copying copies. A mouse drag,
a keyboard extend and `V` all produce the same thing.

| Keys | Does |
|---|---|
| `Shift+arrow` | extend a charwise Selection from the cursor, across line breaks |
| `W` / `B` | extend a word forward / back; `Shift+Alt+arrow` is the alias (**unlisted**) |
| `V` | select the current line; motions (`j`, `k`, `G`, `gg`) extend it linewise |
| `d` / `y` on a Selection | delete or yank exactly what is picked |
| `Ctrl+C` / `Cmd+C` | copy the Selection to the system clipboard |
| `Ctrl+V` / `Cmd+V` | paste the clipboard |
| `Esc`, a plain arrow, a click | drop the Selection |

Ctrl and Command are aliases on copy and paste. Copying with nothing selected does nothing.
Copying uses the system clipboard and falls back to the terminal's own clipboard protocol (OSC 52)
so it still reaches your machine when Varde runs over SSH. Yanking with `y` copies to the
clipboard as well as filling the register.

Double-clicking a word picks it. Dragging picks characters in the editor, scrollback in the
terminal, and the visible screen in the AI pane; in the file tree a drag only moves the row
selection — a filename is not text you copy character by character.

A picked word is **echoed**: every other exact, case-sensitive occurrence of it in the file is
marked more quietly than the pick itself. Independently of any Selection, resting the cursor in a
word marks that whole word wherever it occurs in the file, whole words only and case as written —
`state` inside `stated` is a different word. Neither marking is a Selection: nothing is picked and
nothing is copied.

To have a Selection read aloud, see [Reading aloud](reading-aloud.md).

### Pasting

A paste into the buffer is **one edit, in whatever mode the buffer is in**: it is never replayed as
keystrokes, so pasted code holding an `i` or a `dd` is inserted as text rather than read as
commands. Line endings become line breaks however the source spelled them, pasted brackets close
no pairs, and the whole paste undoes in one `u`. A Preview refuses a paste out loud rather than
editing a file you cannot see the characters of.

A paste while one of Varde's own lines has the keyboard — the `/` search line, the `:` command
line, the tree filter — is typed into that line character by character, and a newline in it is
`Enter`. A multi-line paste onto the `/` line therefore submits at its first line break.

### Editing every occurrence at once

`Ctrl+D` (or `gm`, with no modifier) adds the next occurrence of the picked word to the Selection.
With nothing picked it takes the word under the cursor first, which is what makes it usable while
inserting. Occurrences are taken in order, wrapping from the bottom of the file to the top, until
every one is held. Typing then replaces all of them at once — in normal mode the first character
starts inserting — and every further keystroke lands at each one. `Esc`, a motion or a click drops
the extra occurrences along with the Selection; a Selection spanning a line break names no word to
take. The whole multi-occurrence edit undoes in one `u`.

## Finding

### In this file

`/` opens the search on the editor's bottom line — the same line `:` commands use — and searches the
file you are editing. The cursor moves to the closest match as each character arrives, so you can
stop typing the moment you have arrived. Every match is highlighted, not only the current one.

| Key | Does |
|---|---|
| `Enter` | keep the position and leave the found text as the Selection |
| `Esc` | go back to where the search started, clearing the highlights |
| `n` / `N` | next / previous match, wrapping at either end; each lands as the Selection |

Matching is literal and case-insensitive unless the query contains a capital. Because `Enter`
leaves a Selection, the found text can be copied with `Ctrl+C` or handed to the project search
with `gr` without retyping it.

### In the project

`Ctrl+F`, or `f` in the palette, opens a full-screen results box over every pane. From the editor,
`*` searches the word under the cursor and `gr` searches the Selection, falling back to the word
under the cursor. The search icon on a folder in the file tree opens the same box confined to that
folder, and the scope holds until the box is closed.

Hits are grouped by file, alphabetically, with line numbers and the matching line. Open buffers
are searched as they stand on screen, unsaved edits included. Matching is literal — `foo(` needs
no escaping — and case-insensitive unless the query has a capital. Results are capped at 500 hits
and the count says so.

Inside the box every printable key is part of the query, so the actions take a modifier. The box
names its own keys on a dim row under the count:

| Key | Does |
|---|---|
| arrows | step hit by hit |
| `Ctrl+N` / `Ctrl+P` | step file by file — the first hit of the next file, or the top of this one |
| `Enter` | open the selected hit at its line, with the word selected |
| `Ctrl+O` | open every file with a hit |
| `Tab` | complete the query to the commonest word in the hits that starts with it |
| `Esc` | close |

The wheel scrolls the list wherever the pointer is, and a click marks the hit under the pointer
without opening it — `Enter` still does that. A click on a file heading marks nothing.

## Going back

Reading code is following it — a definition three files away, a search hit — and then returning.
Varde records a **Visit** for every **Jump**: opening a file, switching Buffer, an in-file search
landing (`n`, `N`, `Enter` on a query), `gg` and `G`, and `gd` to a definition. Arrows, `j`/`k`
and clicks are motions and record nothing, and browsing the tree does not count either. What is
recorded is the place being *left*, so going back returns you to where you were.

| Keys | Does |
|---|---|
| `Ctrl+P` / `Ctrl+N` | jump back / forward |
| `gp` / `gn` | the same, with no modifier |
| `Ctrl+Alt+Left` / `Ctrl+Alt+Right` | the same, for a hand already on the arrows (**unlisted**) |

Both ends refuse out loud rather than doing nothing. The place you stood when you first went back
is recorded too, so forward can return to it, and a Jump made mid-history appends rather than
discarding what was ahead. The same place twice in a row is recorded once, and the list is capped.

`y` in the palette opens the **Cursor history** pane in the Corner beneath the file tree, replacing
whatever occupied it. Each row names the file, an excerpt of the line the cursor was on, and the
line number, coloured as the editor colours that file; a row whose line no longer holds what it
recorded is dimmed rather than claimed as current. `Enter` on a row goes there; a click goes there
and leaves the keyboard in the pane. The Visits are this session's only — the pane's visibility is
remembered, its contents are not.

## Folding

`:toggle` folds the innermost block the cursor is in — or opens it again — and `:toggle!` folds
every block in the file at once, opening them all on the next press. A line that opens a block
carries a toggle in its gutter, which a click also flips. The caret steps over a folded block
rather than into it, and `Enter` with the cursor on the fold's dots opens it. With no file open
the command is refused out loud.

## Merge conflicts

A Conflict a merge left in the file is drawn over its text: each marker line becomes a dim bar
naming its side — `Current change (HEAD)`, `Common ancestor`, `Incoming change` — the current side
is tinted green and the incoming side blue. The text itself is unchanged, and the marker the cursor
is on shows as the text it is, so it can be edited by hand.

With the cursor anywhere inside a Conflict, `cc` keeps the current side, `ci` the incoming side and
`cb` both, current first; the bar's `[accept current]`, `[accept incoming]` and `[accept both]` do
the same with a click. Each is one `u`. Accepting only edits the buffer: nothing is saved, and Varde
never stages anything. Palette `m` lists every unmerged file's Conflicts.

## Reading the shape of the code

**Indent guides** run down each level of indentation, and every space is drawn as a dot, so a line's
shape is visible without counting. The guide of the block the cursor is inside is heavier than the
rest, and the innermost bracket pair the cursor is inside is marked — only that pair, so
punctuation nobody is looking at stays quiet.

**Syntax highlighting** follows the file extension across roughly 220 syntaxes — TypeScript, TSX,
Vue, Svelte, Kotlin, Swift, Zig, Dart, TOML, Terraform, Nix, Dockerfile, GraphQL and SCSS among
them, and Varde's own `config.toml`. An unknown extension, or none, renders as plain text. Colour
comes from `editor.theme` in configuration (`dark` unless set to `light`).

`:dim` toggles the field the code sits on — a shade under the rest of the TUI, which lifts every
token's contrast. Someone who chose a transparent terminal wants the desktop back through the code,
so it is a switch, and the choice is remembered per project.

The **change bar** is a mark in the gutter beside every line the last commit does not hold — an
inserted or an edited line, saved or not, so a line typed a second ago is a change. A file the
commit has no copy of carries no bar rather than a bar down every line, and deleted lines leave no
mark. The gutter's one column holds one mark: a diagnostic first, a Reading's place second, the
change bar last.

## Minimap

A far-off mirror of the whole file down the editor's right-hand edge: two source lines to a row and
four source columns to a cell, so the shape of the file — its blocks, its comment runs, where the
code stops — is there without reading it. It scrolls itself so the window you are looking at is
always inside it, and a line down its first column (the **Slider**) marks that window, lighting up
while the pointer is over the mirror.

Press or drag in the strip to travel the file; the caret comes with the view, and a drag that
started there picks no text however far it wanders. `:minimap` turns it off and on again, giving
its columns back to the text, and the choice is remembered per project; `editor.minimap = false`
in configuration starts a project without one. A pane with fewer than twenty columns of text left
does without it, and a Preview, whose rows are not lines, has nothing to mirror.

## Buffers, files and disk

Opening a file keeps what is already open; `gt` / `gT` step through the open Buffers and the dots
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

Leaving Varde is not on the `:` line at all — `Ctrl+Q`, or `q` in the palette — so a mistyped
clear-up cannot take the session with it.

A buffer with no unsaved edits follows the file on disk silently. A buffer **with** unsaved edits is
never overwritten: when the file changes underneath it the buffer is flagged and the status line
says so. `:w` still writes — the flag was a warning, not a lock — and `:e` takes the disk version.
`D` in normal mode opens a picker with the three ways out: `r` reload from disk, `w` overwrite
disk with your edits, `m` hand both versions to the AI pane to merge (the buffer stays flagged
until the AI's write lands). `Esc` leaves the divergence standing. `D` on a buffer that agrees
with disk says there is nothing to resolve, and while inserting `D` is just a letter.

## Markdown Preview

A `.md` or `.markdown` file opens as a **Preview** — the document, laid out to the pane width —
and everything else opens as **Source**. `:preview` toggles between the two for this buffer only;
on anything that is not markdown, or with no buffer open, it refuses in the footer.

A Preview renders headings, wrapped paragraphs, emphasis, lists, tasks, block quotes, tables,
fenced and indented code (highlighted by the language the fence names, never wrapped), link text
with the URL hidden, images as their alt text, literal HTML set apart, and YAML frontmatter shown
above the document rather than hidden. A `mermaid` fence is drawn as a diagram; when it cannot be —
an unsupported type, a parse error, too wide for the pane — the reason is stated and the fence's
source shown beneath it. Links are styled text only; nothing follows them.

The Preview is read-only. `a o O x dd p u V` do nothing and say so. Motions work over the
**rendered rows** — `h l 0 $ w b e gg G`, the arrows and the word-motion arrows — so `$` stops at
the end of the text on screen and a word motion crosses a wrap the source does not have. `/`
searches the rendered text, `Shift+arrow` picks rendered rows, and a copy takes the text exactly
as drawn, wrap newlines included. There is no line-number gutter, and the pane title says
`preview`.

`i` crosses to Source and arrives inserting — "let me change this" — while `:preview` crosses in
normal mode. Crossing keeps the line and starts at column one in either direction. `:w` and every
other `:` command still work over a Preview; `:format` crosses to Source first, so its undo works
where you can see it. A Preview shows whatever the buffer holds, so edits made in Source appear
the moment you cross back.

## Configuration

Two keys under `[editor]` in `~/.varde/config.toml` or `<project>/.varde/config.toml`, the project
winning key by key — see [Configuration](configuration.md):

```toml
[editor]
tab_width = 4      # what Tab lays down, and the block indent when the file holds none to copy
minimap = true     # whether the mirror is up in a project nobody has turned it off in
```

## See also

- [Language intelligence](language-intelligence.md) — hover, definitions, diagnostics, completion
  and `:format`.
- [Getting around](getting-around.md) — the palette, panes, focus and buffers.
- [Reading aloud](reading-aloud.md) — `:read` speaks the Selection.
- [Configuration](configuration.md) — the layered `config.toml` these keys live in.
