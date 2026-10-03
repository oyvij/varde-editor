# A Debug adapter is a hosted child, reached three ways

Varde debugs through the Debug Adapter Protocol: one client in Varde, and one Debug adapter per
language doing the language-specific work — codelldb for Rust, js-debug for JavaScript and
TypeScript, java-debug for Java. That is the only design in which "a debugger for many languages" is
a client and a table rather than a debugger per language, and it is the design
`docs/adr/0011-a-language-server-is-a-second-hosted-child.md` already made for language servers.
This ADR records that the argument transfers, and the one place it had to stretch.

**No branch in `src/` names a Debug adapter or the language it serves.** Not `codelldb`, not
`js-debug`, not Java — not by command, not by version, not by sniffing a reply for a string only one
adapter sends. Adapters are `[dap.<language>]` rows in the programs template of
`docs/adr/0018-the-global-config-is-the-list-of-programs.md`, with `install.<os>` keys so Tools can
install them, and a Launch configuration is a named entry in either config layer whose launch or
attach arguments are handed to the adapter untouched. The falsifiable form is ADR-0011's grep,
widened: every hit for an adapter's name in `src/` is in the template, a fixture or a comment.

## Three ways in, because the three languages that matter use all three

ADR-0011's child is spawned and spoken to over stdio. For debugging that covers one of the three
languages this was designed for, so a row says how its adapter is reached, as data:

- **stdio** — spawn `command` with `args`, speak over its standard streams. `lldb-dap` works this
  way.
- **server** — spawn `command` with `args` in which `${port}` is filled in, then connect to that
  port over TCP. codelldb and js-debug work this way.
- **through a language server** — name an `[lsp.*]` row, a plugin to load into that server when it
  starts, and a command to send it; the server answers with a port, and Varde connects to it over
  TCP. Java's adapter works this way: java-debug is not a program but a plugin inside jdtls, because
  mapping a file and line to a class needs the classpath only the language server has.

The third shape looks like the Java special case this repo forbids, and it is not one, because what
it names is a *relationship* any server could have — "this language's language server hosts its
Debug adapter" — and the row says which server, which plugin and which command. A second language
that hosts its adapter the same way is a second row. What would be the forbidden shape is Varde
knowing that `vscode.java.startDebugSession` is the command, and it never does.

Stdio alone was the alternative, and it was rejected on a concrete workflow rather than on
principle: attaching to a local Java service's JDWP port, where requests routed through a proxy
pause at the right line, is the debugging this was built for. Supporting only the shape language
servers already use would have left out the one language whose attach flow was the reason to build a
debugger.

## What carries over from ADR-0011 unchanged

**Only the edge observes the process.** That an adapter is running, that it exited, that its port
never answered — these are told to the core by the edge each pass, never remembered by the core from
having asked for a spawn. `ai_running` is the failure this rule names, and a Debug session is a
third place it could recur.

**Branching on a declared capability is not naming a provider.** An adapter that reports it cannot
set a variable gets a dimmed Chip. The Exception filters shown are the ones it lists. The Evaluator
is offered for hovers only if the adapter says it evaluates for hovers. Each of those reads the
protocol's own answer.

**A missing adapter is a message, never a fallback.** The configured adapter either runs or the user
is told once, by name.

## Consequences

**Adapter bookkeeping stays out of the user's model.** js-debug opens a child session per worker or
subprocess and asks the client to start it. Varde folds each one into the one Debug session, as more
threads. A session picker would be the adapter's structure leaking into the UI, and nothing Java or
Rust does needs one.

**Varde does not make up for a weak adapter.** Rust's evaluation under LLDB reads fields and does
arithmetic but mostly cannot call a method, and RustRover hits the same limit. The Evaluator shows
the adapter's refusal as the adapter gives it. Varde injecting compiled code into a paused process
would be per-language knowledge, and it was declined in the spec. When an adapter improves, Varde
improves with it without a release, which is the benefit ADR-0004 claimed for the same rule.

**The DAP framing is not the LSP framing, though it looks the same.** Both use `Content-Length`
headers, but DAP messages have no `jsonrpc` field, so the language server's message parser cannot
read them. The channel is a second one, not a reuse.

The behaviour this serves is specified in issue #45.

## Amendment: hot code replace is a second thing the row names as data

A program rebuilt under a paused process keeps running the bytecode it was loaded with. DAP has no
request for replacing it, so the adapters that can do it expose their own. The row names them:

```toml
hot_replace = { request = "redefineClasses", event = "hotcodereplace" }
```

Varde sends `request` when the adapter sends `event`, and when `:hotswap` or the Debug group's Chip
asks for it. It knows neither name, exactly as it does not know what
`vscode.java.startDebugSession` is. A row without the key offers no hot replace at all — no Chip,
and a Command that declines out loud. This is the second place the row carries adapter-specific
protocol as data, and it is held to the same falsifiable grep: every hit for `redefineClasses` or
`hotcodereplace` in `src/` is in the programs template or a comment.

The outcome is read from the protocol and nothing else. `success: false` is a failure, and its
`message` is shown as the adapter wrote it; anything else is a success. An adapter that answers
`success: true` with a body listing the classes it could not replace is therefore reported as a
success, because the shape of that body is the one adapter's and reading it would be the branch this
ADR forbids. The cost is a notice that overstates one adapter's partial replace; the alternative is
a parser for a body no second adapter shares.

A failure offers a restart and takes nothing. The session carries on with the old code, because the
pause is the reader's and a debugger that restarts a process on its own loses the state they were
looking at. The offer is the restart that already exists, undimmed for as long as the failure
stands, and taking it disconnects the adapter and starts the same Launch configuration once the
process is gone — `State::relaunch` is that one deferred Launch, because starting a session while
the old adapter is still being torn down would hand the new one the old one's exit.

**Does the event fire for builds run outside the language server?** For java-debug, no — not
reliably. Eclipse's hot code replace is driven off resource deltas on `.class` files on the
project's build path, so jdtls's own builds always announce themselves and a Maven, Gradle or
dev-server build only does when its output lands on that build path *and* jdtls has refreshed it as
a resource change. The request itself has no such limit: it reads the class files that are on disk
when it runs, so the manual trigger is what covers an external rebuild, and the brief stands as
written — which is why `:hotswap` and the Chip exist rather than the event alone. What the JVM
refuses is separate and unchanged by either path: method bodies can be replaced, a changed class
shape cannot, and that refusal is the failure the notice carries.

This was read off Eclipse's JDT debug documentation and java-debug's own, not verified against a
live jdtls; the acceptance run on a Java project is the thing that would.
