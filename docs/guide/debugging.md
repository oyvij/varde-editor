# Debugging

Varde debugs through the Debug Adapter Protocol: one Debug adapter per language, named in a
`[dap.<language>]` row rather than in Varde, the way a language server is
([configuration.md](configuration.md)). A Debug session runs on top of Edit view, and you can keep
editing while the program is Paused.

## Breakpoints

A click in the gutter's leftmost column, or `Space` then `b` on the cursor's line, sets or removes
a Breakpoint. They exist with or without a session, move with their line as you edit, and the
project remembers them. Palette `b` lists every one in the Corner.

## Starting a session

A Launch configuration names the adapter, whether to `launch` or `attach`, and the arguments the
adapter is handed:

```toml
[launch.server]
adapter = "rust"
request = "launch"
args = { program = "target/debug/server" }
```

`Ctrl+Space` then `n` lists every Launch configuration in the global and the project file; the
arrows pick one and `Enter` starts it. Varde runs the protocol's start sequence and sends your
Breakpoints before the program runs.

The Rust adapter is `codelldb`. If it is missing, Varde refuses to start the session and names the
adapter. Its row under **Debug adapters** in Tools (`Ctrl+Space` then `v`) installs it.

## Paused

When the program pauses, Varde opens the file it paused in if it was not open, highlights the paused
line across the editor with a `→` in the gutter, and shows the Frames (the call stack) in the Corner.
The keyboard stays where it was. Enter or a click on a Frame moves the Paused line to that call.

| Key | Does |
|---|---|
| `F9` | continue while Paused, pause while Running |
| `Ctrl+F2` | stop: terminates a launched program, leaves an attached one running |

While a session exists, both keys reach Varde from every pane, a shell included. With no session,
they go to the programs in your shells. Ending the session puts back what the Corner showed before.
