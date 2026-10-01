# Varde user guide

One file per area. Every key, palette letter and `:` command named here is also in the Cheatsheet
in the editor's top-right (`:help` toggles it). The Cheatsheet lists every binding. If a gesture is
not there and not in these pages, it does not exist.

| Guide | Read it when you want to |
|---|---|
| [Getting around](getting-around.md) | start Varde, learn the panes and views, open the palette, move focus, use the file tree, buffers, terminal and mouse, quit, update |
| [Editing](editing.md) | edit text: modes, motions, operators, selection, undo, find and project search, multi-cursor, folding, minimap, change marks, cursor history, markdown Preview, `:w :q :e` |
| [Language intelligence](language-intelligence.md) | jump to a definition, hover, see diagnostics, complete, format, and install or configure a language server or formatter |
| [Review](review.md) | read a change as diffs, annotate it with ISSUE / NOTE / SUGGESTION / COMMENT, and `:submit` it to the AI |
| [Stories](stories.md) | have the AI narrate a change as a Story set and walk its Spine and Steps, from a range, a branch, or a guest repo |
| [Risk](risk.md) | see which functions are too complex, and run the Refactor loop behind the project's test Gate |
| [AI pane](ai-pane.md) | host an AI CLI beside the editor, pick it with `:ai`, and understand what reaches it |
| [Debugging](debugging.md) | set Breakpoints, start a Launch configuration, see where the program paused and walk its Frames, continue and stop |
| [Reading aloud](reading-aloud.md) | have a Selection read to you, pause, skip and change speed, install a voice |
| [Configuration](configuration.md) | every config key, the global and project files, and what lives under `.varde/` |

Installing and updating Varde itself is [`docs/install.md`](../install.md). The specification the
guides are written from is [`docs/example-map.md`](../example-map.md); `features/*.feature` is its
executable form and is the final word where the two disagree.

## Ten things to know first

1. `Ctrl+Space` opens the palette from any pane, hosted ones included. `Esc Esc` does too from
   inside the terminal or AI pane.
2. Every motion is reachable without a modifier. `Alt+hjkl` moves focus, but so do palette letters.
3. There is one Selection in the whole workspace. Whatever you last picked (a drag, `V` lines, a
   search hit) is what `Ctrl+C` copies and `:read` reads.
4. Review view shows only what git says changed. `V` then `c` comments a range; `:submit` sends it.
5. `:story` asks the AI to narrate the current change; `:story?` picks a branch first.
6. The Risk list (palette `k`) names functions over the complexity threshold; its action starts a
   Refactor loop that only keeps an Iteration the project's tests accept.
7. Language servers and formatters are rows in config, installed from Tools
   (`Ctrl+Space v`); `install.sh` installs the package managers they need. Varde never installs anything silently.
8. `:format` formats the text on screen, not the file on disk. `:w` writes.
9. Config is layered: `~/.varde/config.toml`, then `<project>/.varde/config.toml`, key by key.
10. `:update` rebuilds Varde from its checkout, or replaces a binary install with the newer
    Release. On a source install the next launch is the new version.
