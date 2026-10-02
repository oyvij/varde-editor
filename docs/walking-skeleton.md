# How Varde is built: one pattern repeated

> **For human readers only. Agents must not rely on this document.** It is a snapshot of the code
> at commit `1703ff1` (2026-10-02). Its line numbers, counts and handler names go stale with every
> commit, and nothing checks them. Agents work from `AGENTS.md`, the code itself and `cargo test`.
> Where this page and the code disagree, the code is right.

## 1. The pattern in one sentence

Each feature is a **slice**. A gesture is turned into an **`Event`** in `keys.rs`/`mouse.rs`. One
`on_*` handler in `lib.rs` answers it, usually by calling a pure function in the feature's own
module (`fold`, `search`, `debug`, …). The handler writes into **`State`**, which is the carrier,
and returns any outside work as **`Effect`** values. **`main.rs`** is the runtime: it calls
`update`, carries out the effects, and has `ui.rs` draw the new `State`.

**Coverage: 96.3%** of 34,520 lines of production Rust (inline `#[cfg(test)]` modules excluded).
The slice parts are 80.0%, the shared types 3.9%, and the plumbing that connects them 12.4%. What's
left is helpers (`layout`, `highlight`, `rpc`, 2.8%) and pty code (`pty`, `queries`, 0.9%). If you
count `ui.rs` as edge rather than part of the slice, coverage is still 84.5%.

## 2. The walking skeleton

This was tested by adding a toy `:ping` feature to a scratch copy, then deleting one part at a time
and building or testing.

```rust
// src/lib.rs — the shared types: one variant/field per slice
pub enum Event { …, Ping }                      // lib.rs:274  the request
pub struct State { …, pub pings: usize }        // lib.rs:965  the carrier field
impl Default for State { … pings: 0 }           // lib.rs:1184 the compiler rejects a missing field
pub enum Effect { … }                           // lib.rs:730  outside work, as data (reuse one if you can)

// src/lib.rs — the slice itself: take your events, pass the rest on
fn on_ping(_state: &State, mut next: State, event: Event, wheeled: bool) -> Answered {
    let effects = match event {
        Event::Ping => {
            next.pings += 1;                    // usually: area::verb(&mut next, …)
            vec![Effect::Notify("pinged")]      // what the outside world should do
        }
        // Event::Other { .. } => …,            // one handler holds several related arms
        other => return Err((next, other)),     // not ours: decline
    };
    Ok(settle(next, effects, wheeled))          // keeps scroll and focus in bounds
}

// src/lib.rs — registration: one more link in a route_* chain (e.g. route_activate, lib.rs:2181)
    let declined = match on_ping(state, declined.0, declined.1, wheeled) {
        Ok(answer) => return Ok(answer), Err(declined) => declined, };

// src/keys.rs:1332 — the way in: a command, a key, or a chord (+ a CHEATSHEET row for a key)
        "ping" => vec![Event::Ping],

// src/main.rs — only for a NEW Effect: an arm in perform_terminal/_session/_files/_jobs (main.rs:2208)
// tests/cucumber.rs — the same arm in applied_to_* (cucumber.rs:949), the test copy of the runtime
// src/ui.rs — only if visible: a *_widget/*_lines fn reading &State
// Cargo.toml — version bump in the same commit
```

**Outcome:** `update(&State::default(), Event::Ping)` returns `pings == 1` and
`[Notify("pinged")]`. Typing `:ping` in the editor shows the notice.

What each part costs to leave out:

| Part | Without it |
|---|---|
| Registration in `route_*` | Build fails: `` error: function `on_ping` is never used `` (`#![deny(dead_code)]`) |
| The `Event::Ping` arm | Builds, then **panics**: `no group answers Ping` (`lib.rs:1662`) |
| `State` field / `Default` | Build fails |
| `keys.rs` line | Builds, but nothing ever creates the event, so the feature can't be reached |
| `main.rs` arm for a new `Effect` | **Builds cleanly.** A debug build stops on `debug_assert` (`main.rs:2811`), a release build drops the effect without a word, and cucumber panics (`cucumber.rs:1050`) |

## 3. The variants

| Variant | Where it lives | Where its input comes from | What it produces |
|---|---|---|---|
| Key or command | `keys.rs` → `on_*` | `KeyEvent` via `keys::on_key_event` | `State` + `Effect`s |
| Pointer | `mouse.rs` → `on_click_*`, `on_drag_row` | mouse `Input` hit-tested against `layout.rs` rectangles | `Event`s, a `Selection` request |
| Something the edge observed | `main.rs` queues `FileChanged`, `AiSpoke`, `Tick`, … | watcher, ptys, `tell_core` | `State` only (the core never writes these facts itself) |
| Language server / debugger protocol | `lsp.rs`, `debug.rs`: `fn(&mut State, json) -> Vec<Effect>` over `rpc.rs` | `LspReceived`/`DapReceived` | `LspSend`/`DapSend` |
| Slow job (ADR 0023) | an `Effect` → a thread in `perform_jobs` → an answering `Event` | search, risk, format, blame | `State` once the answer arrives |
| Draw only, no `Event` | `ui.rs` + `area::fn(&State)` (`fold::toggles`, `highlight`) | `State`, or the buffer text | widgets |

## 4. Composition

Routing is one chain of responsibility nested three levels deep: `route` →
`section_input/editing/workspace` → `route_*` → 61 `on_*` handlers. Each level returns
`Answered = Result<(State, Vec<Effect>), (State, Event)>`.

The runtime has the same shape: `perform` → `perform_terminal/session/files/jobs`, mirrored by
cucumber's `apply` → `applied_to_*`.

Slices also call each other through the same entry point. A handler can call `update` again with
another event (`lib.rs:3896` re-sends `ToggleFold`), and an `Effect` can come back later as a new
`Event` through the queue.

## 5. Analogy: Elm, or Redux with redux-loop

| Redux / Elm | Varde |
|---|---|
| action / `Msg` | `Event` |
| reducer / `update` | `update` → `on_*` arm |
| store | `State` |
| `Cmd` / loop effect | `Effect`, carried out by `main.rs` `perform` |
| dispatch from the UI | `keys.rs` / `mouse.rs` → queue |
| `view` / selectors | `ui.rs` + `area::fn(&State)` |
| `combineReducers` | the `section_*`/`route_*` chain, where each handler passes on what isn't its own instead of owning a slice of `State` |

## 6. Recipe: "`:trim` strips trailing whitespace from the current buffer"

1. Write the scenario in `features/editing.feature` (`When I run ":trim" in the editor` /
   `Then …`). Run `cargo test` and watch it fail as undefined or failing.
2. Touch `tests/cucumber.rs` only if you need a new step phrase.
3. Write the logic as a pure function plus a unit test next to it, in the area's module:
   `src/editor.rs` for buffer text (`search.rs`, `fold.rs`, … for other areas).
4. In `src/lib.rs`, add the `Event::TrimBuffer` variant. Add its arm to the handler that owns that
   area: grep `Event::FormatBuffer`, which lands in `on_lsp`, `lib.rs:4750`. Or add a new `on_*`
   handler and link it into a `route_*` chain. Add a `State` field plus its `Default` only if the
   slice needs one.
5. In `src/keys.rs`, add `"trim" => vec![Event::TrimBuffer]` next to `"format"` (`keys.rs:1338`).
   A key binding also needs a `CHEATSHEET` row, or the cheatsheet test fails.
6. Prefer an existing `Effect`. If you add a new one, also add its arm to `src/main.rs` `perform_*`
   **and** to `tests/cucumber.rs` `applied_to_*`. The compiler won't remind you.
7. If it's visible, add a `*_lines`/`*_widget` function in `src/ui.rs`. If it's clickable, add a
   rectangle in `src/layout.rs` and a hit-test in `src/mouse.rs`. If the editor frame changes,
   regenerate `tests/snapshots/editor_before_101.txt`.
8. Optionally, update `docs/guide/editing.md` and add any new domain word to `CONTEXT.md`.
9. Bump `Cargo.toml` (minor version), then run `cargo build` so `Cargo.lock` moves in the same
   commit.

**Replay check:** two recent commits were held out of the analysis, and their files were predicted
from the recipe before opening them.

- `fb0b8d7` (`[word]` toggle): the first recipe missed `layout.rs` and the snapshot, so step 7 now
  covers both.
- `0a673f6` (highlight by first line): no files missed. `cucumber.rs` was predicted but untouched,
  which is why step 2 is conditional.

## 7. Navigation

- `Event::Foo` connects every layer: it's produced in `keys`/`mouse`/`main`, matched in an `on_*`
  handler, and sent in `tests/cucumber.rs`.
- `Effect::Foo` goes from the emitter (`lib`, or `debug`/`lsp`/`reading`/…) to `main.rs perform_*`
  and `cucumber.rs applied_to_*`.
- `next.foo` / `state.foo` lead from a `State` field to whoever writes it (only `update`, except
  facts the edge sets in `tell_core`) and to whoever reads it (`ui.rs`).
- The `"cmd" =>` entries in `keys.rs` list every `:command`, and `CHEATSHEET` lists every key.

## 8. Deviations

1. **A new `Effect` with no runtime arm compiles.** A release build drops it silently
   (`main.rs:2811`). Confirmed in a scratch build.
2. **A new `Event` with no handler compiles**, then panics on the first event (`lib.rs:1662`).
   Routing is by declining rather than an exhaustive `match`, so the compiler can't catch a missing
   arm.
3. **`Event::AddComment`** (`lib.rs:322`, arm at `lib.rs:2761`) is only ever sent by
   `tests/cucumber.rs:3035`. The product never produces it; a test keeps dead code alive.
4. **Numbered handler names** (`on_key_2…6`, `on_editor_key_2…6`, `on_activate_2…4`) say nothing
   about what they own. Find a handler by grepping its `Event::` instead.
5. **Domain modules take `&mut State`** (`debug.rs`, `lsp.rs`, `risk.rs`) rather than "plain data",
   as `AGENTS.md` describes them. That ties them to the carrier, so a change to `State` reaches into
   those modules too.
6. **`lib.rs` holds about 7,600 lines of handlers for every area**, while `main.rs`/`ui.rs` both
   count as edge in `AGENTS.md` and as slice parts here. A feature always touches `lib.rs` even
   when its logic lives in an area module.
