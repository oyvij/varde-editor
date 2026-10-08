# The AI pane

The right-hand pane hosts an AI CLI (Claude Code, opencode, anything that runs in a terminal) as a
program in its own terminal, inside Varde's. It is a hosted pane: its keyboard, mouse and clipboard
belong to the program running in it. Varde passes your keys in and its screen out unchanged. A
review you submit and a Story you ask for both arrive there as a prompt. The panes, the palette and
focus are in [getting-around.md](getting-around.md). What a review is and how to write one is in
[review.md](review.md). Stories are in [stories.md](stories.md).

## Starting a session

With nothing running, the pane is a small box asking which CLI to start. It is prefilled with the
one you used in this project last time or, with no history, the `ai.command` setting, which defaults
to `claude`. `Enter` starts it, and typing replaces the suggestion. While no session runs, keys typed
into the pane go to the box and never reach your shell.

| Gesture | Effect |
|---|---|
| palette `a` | focus the AI pane. It starts nothing, because the box is already there |
| `:ai` | start the remembered CLI, or focus the session already running |
| `:ai <command>` | start that CLI. If a different one is already running, Varde refuses and shows a notice |
| `:ai! <command>` | stop the running session and start that CLI instead |

Switching CLIs mid-session takes the `!`, the same idiom as `:q!`, so you cannot do it by accident.
Varde remembers the CLI you chose in the project's state, so reopening the project offers it again.

### Tall

The default shape puts the pane beside the editor, above a terminal that spans the whole width. An
AI CLI holds a long conversation, and two-thirds of a screen is too short to read one. So the pane
can take the whole right-hand edge instead, and the terminal gives up width rather than the AI pane
giving up rows.

| Gesture | Effect |
|---|---|
| palette `l` | swap between beside-the-editor and the whole right-hand edge |
| `:tall` | the same, from the command line |

The palette entry leaves focus where it was. That matters because you want the shape while reading
the pane, and a hosted pane's child owns the colon, so `:tall` means leaving the pane first. Varde
remembers the shape per project. If you dragged the pane's edge to a width, both shapes use it.

## What reaches it

A running session gets ordinary typing exactly as a terminal would send it. `Alt+Enter` arrives with
its modifier intact, `Shift+Tab` is `Shift+Tab`, and `Ctrl+Q` and `Ctrl+F` reach the CLI rather than
quitting Varde or opening its search. Varde keeps only the keys on a fixed, written list, the Reserved
keys. Every other key goes to the child:

| Reserved key | Why Varde keeps it |
|---|---|
| `Ctrl+Space` | opens the palette from every pane. The CLI loses one NUL byte |
| `Alt+h` `Alt+j` `Alt+k` `Alt+l` | move focus between panes |
| `Ctrl+C` while a Selection exists in this pane | copies it. With nothing selected it interrupts the child, as in any terminal |

`Esc Esc` opens the palette too, and is not on the list. Both escapes still reach the CLI, and Varde
opens the palette only because it counted the pair. From the palette, `o` returns you to the editor,
`e` `r` `s` change view, and `q` quits. None of it needs a modifier, so you can leave this pane on a
terminal where Option is not Alt.

The CLI gets only the environment variables `ai.env` names, so a secret exported in the shell you
started Varde from does not reach it. The default list is `HOME`, `PATH`, `USER`, `LOGNAME`,
`SHELL`, `TMPDIR` and the locale. See [Configuration](configuration.md#ai).

The CLI sees Varde's terminal identity, not your host terminal's. `TERM` is `xterm-256color`, and Varde
removes the variables a terminal emulator exports about itself. A CLI that checks `TERM_PROGRAM` or
`TMUX` gets an answer about Varde rather than about your machine. When a program asks by escape
sequence, Varde answers as `VARDE(x.y.z)`, sends an explicit refusal for advanced modifier
reporting, and answers cursor-position and device queries. A program that gets no reply falls back
to its most basic behaviour without telling you.

## Mouse

When the CLI asks for mouse events, as full-screen AI CLIs do, Varde reports clicks and the wheel to
it in the encoding it asked for. A click lands on its *release*, and only if the pointer did not
move in between. So starting a drag to select text never activates whatever the drag began on. The
legacy encoding some programs ask for cannot name a cell past column or row 223. Varde drops a click
there rather than sending it to the wrong place.

The wheel goes to the CLI only while it has asked for mouse events. A full-screen program runs on
the terminal's alternate screen, where Varde has no history to show, so the program scrolls itself.
When the CLI has not asked for mouse events, a click only focuses the pane and stays with Varde.

### Clicking a link

Point at a URL or a file path the CLI printed and Varde underlines and tints it, and the pointer
becomes a hand on a terminal that can show one. Hold the jump modifier (`Cmd` on macOS, `Ctrl`
elsewhere; Varde accepts both) and click it, and a URL opens in your browser. Nothing reaches the CLI. Only `http` and `https` URLs
count, and only within one row of the screen. Varde does not recognise a URL the terminal wrapped
onto the next row, and clicking plain text with the modifier held opens nothing.

## Copying from the pane

Drag across the pane to select, then press `Ctrl+C` or `Cmd+C` to put the text on the clipboard.
Over SSH, that is the clipboard of the machine you are sitting at. The Selection is the pane's own
screen, not the terminal's below it, and a drag across rows takes both ends and the rows between. A
click clears it.

The pane selects only what is on its screen. An AI CLI runs on the alternate screen, where the
terminal keeps no scrollback, so a drag cannot reach past the top of the pane. To copy more, scroll
the CLI itself and select again. Keeping a transcript of the CLI's output to select from does not
work. A full-screen program prints a stream of redraws and cursor moves, and replaying that as text
produces garbage.

## Pasting into it

Use your terminal's own paste gesture (`Cmd+V` on macOS). The text reaches the CLI in one write. If
the CLI asked for bracketed paste, as every modern CLI does, Varde wraps the text in the paste
markers so the CLI does not read a newline in the middle as a submit. A program that did not ask gets
the text bare. Varde strips the closing marker from the text before wrapping it, so a paste cannot
end its own bracketing early. With no session running, a paste reaches neither the box nor your
shell.

## What Varde sends it

**A submitted review.** `:submit` in Review view asks you to confirm, then writes the review to
`.varde/reviews/NNNN.json` and sends the CLI a prompt. The prompt is an inline summary of the
comments with their file and line ranges, plus the file's path. Varde submits it, Enter included, so
the CLI starts working the moment you confirm. It asks first because sending clears whatever the
CLI's prompt line shows, which may be a half-written message Varde cannot read back. With no
session running, Varde starts the remembered CLI first and holds the prompt until the CLI is ready
for input. It never types at a program that has not printed its prompt yet. From a Bare workspace
the review goes to `~/.varde/reviews/` instead.

**A Story.** `:story` resolves a revision range, asks you to confirm it, and pastes a prompt asking
the CLI to write a story file into `.varde/stories/`. The file watcher picks the file up. Varde
never reads what the CLI prints. See [stories.md](stories.md).

**A merge.** When a Buffer with unsaved edits differs from its file on disk, `D` then `m` hands the
CLI both versions and asks it to merge them. The Buffer stays flagged until the watcher sees the
CLI's write.

**A refactor.** The Risk list's row action asks the session to refactor one function, and the
refactor loop hands it a scope and a goal. See [risk.md](risk.md).

Nothing comes back. Varde does not read the CLI's screen to learn whether it addressed a review or
finished a story. The file on disk is the only channel. If a session exits, fails to start or is
replaced with `:ai!` before it is ready, Varde drops the review queued for it rather than handing it
to the next session.

## When the CLI exits

The pane goes back to the box, prefilled with the command that just ran, ready to start another. If
a CLI cannot start at all, for example because its name is not on your PATH, the pane asks in the
same way, prefilled with what failed. `:ai` still works afterwards and does not claim that something
is "already running". Keys typed at a pane with no session reach nothing.

## No special cases

Nothing in Varde tests which CLI is running in the pane: not by name, not by version, not by reading
its output. Varde passes every key, mouse report, paste and terminal query through unchanged or
sends an explicit refusal, and nothing else. So a CLI nobody has tried behaves the same as the ones
people have, and a key your CLI newly binds needs no Varde release.

So if a CLI misbehaves inside Varde, the fault is in how Varde passes input through, and fixing it
there fixes it for every CLI. Check these before reporting one:

- **A modifier that does nothing.** On macOS, Option is not Alt unless you configure your terminal
  that way (Ghostty: `macos-option-as-alt = true`), and a stock tmux strips modifier reports. Every
  gesture Varde owns has a route through the palette with no modifier, but a CLI's own Alt bindings
  need the terminal to send Alt.
- **Clicks leaving stray characters in the prompt.** The CLI asked for one mouse encoding and is
  getting another. That is a Varde bug, and the same one for every program.
- **A shortcut that seems disabled.** We checked: no shortcut depends on what Varde advertises.
  Either the key is on the Reserved list above, or it never arrived.

Report what the CLI received, not which CLI it is. Varde cannot tell one program from another, by
design.
