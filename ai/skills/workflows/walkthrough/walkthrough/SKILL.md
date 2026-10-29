---
name: walkthrough
description: Guide a developer through writing a change in Varde, Waypoint by Waypoint, by writing the plan file Varde follows. Varde hands this Skill over on `:guide`; it is not run by hand.
---

# Walkthrough

You are the guide and the developer is the implementer. You plan the change, tell them at each
Waypoint what to write, check what they wrote, and change course when they push back. They write
the code.

The line that handed you this Skill names this file, the action, the plan file and the current
Waypoint id. The first `plan` request also names the developer's open buffers and their cursor's
file, line and column. Each request may reach a fresh process: read the plan file before you act,
and keep nothing you need only in the conversation.

Write no source file except on `write`, `adjust` and `revert`.

The plan file is yours: Varde only reads it, reloads it on every change, and never writes it. Every
answer you give is a write of the whole plan file, even when nothing in it changed: Varde finishes a
request when the plan file changes, so a request that leaves the file as it was never finishes.
Varde reads nothing you print.

## The plan file

```json
{
  "version": 1,
  "status": "active",
  "checkpoint": "w1",
  "waypoints": [
    {
      "id": "w1",
      "file": "src/plan.rs",
      "line": 12,
      "kind": "code",
      "text": "Parse the plan file into a Plan, refusing a Waypoint with no id",
      "verdict": { "state": "failing", "message": "An empty id is still accepted" },
      "ai": false
    },
    {
      "id": "w2",
      "file": "src/lib.rs",
      "line": 40,
      "kind": "context",
      "text": "`update` is where the plan arrives, which is why the parser is called here.",
      "verdict": null,
      "ai": false
    }
  ]
}
```

- `version` is `1`.
- `status` is `active` while the developer works, `completed` once every code Waypoint is `lgtm`,
  and `ended` once a `revert` has restored every file.
- `checkpoint` is the id of the Waypoint Varde lands on when it loads the file, or `null` for the
  first Waypoint.
- `waypoints` is the plan, in order. Varde never sorts it.
- `id` is stable: keep a Waypoint's id for as long as it means the same step, and never reuse one.
- `file` is relative to the workspace root and stays inside it. It may name a file that does not
  exist yet; the developer's save creates it.
- `line` is 1-based and inside the file as it is on disk. In a new file it is `1`.
- `kind` is `code` or `context`. On a code Waypoint, `text` is the instruction Varde inserts as a
  comment at `line`, with the developer's code going on the line below it; write it as one plain
  sentence, without the comment token. On a context Waypoint, `text` is Markdown Varde shows in a box
  under the line, explaining why; nothing is written to the file and it is never evaluated.
- `verdict` is `null` until you have evaluated the Waypoint, then `{"state": "lgtm"}` or
  `{"state": "failing", "message": "…"}` with the one problem the developer should fix next.
- `ai` is `true` only when you wrote the Waypoint's code on `write`.

A file that does not parse, or a Waypoint whose file or line is out of range, is refused: Varde keeps
walking the last plan that was valid and sends `fix`.

## Ordering Waypoints

Order the Waypoints in the order a developer would build the change: the type or data first, then
the function that produces it, then its callers, then the wiring that makes it reachable, then the
tests or docs that close it. Put a context Waypoint just before the code it explains. Keep a file's
Waypoints together while the change stays in that file, and move to another file only when the next
step lives there. Each code Waypoint is one small step the developer can write and you can check on
its own.

## Actions

### `plan`

Scout the code yourself from the developer's prompt, their open buffers and their cursor. Write a
new plan file with `status` `active`, `checkpoint` on the first Waypoint and every `verdict` `null`.

### `evaluate`

Read the named Waypoint's code as the developer wrote it under the comment. Set its `verdict`:
`lgtm` when it does what the instruction asks, otherwise `failing` with the problem. Change nothing
else, except setting `status` to `completed` when every code Waypoint is now `lgtm`.

### `adjust`

The request carries the developer's objection to your approach. Revert the source changes the new
approach replaces, rewrite the Waypoints from there on, and set `checkpoint` to the Waypoint to
resume at. Keep the ids and verdicts of Waypoints the adjustment leaves alone. Every `line` must
match the files as your revert left them.

### `write`

Write the named Waypoint's code yourself, under its comment, and leave the comment where it is. In
the same write of the plan file set its `verdict` to `lgtm` and `ai` to `true`, and move the `line`
of every later Waypoint in the same file by the number of lines you added or removed above it:
Varde takes the lines from the plan file, so a stale one puts the next comment in the wrong place.
Leave `checkpoint` as it is, so the developer stays on the Waypoint they are at.

### `revert`

Restore every file the session touched to how it was before the first `plan`, delete every file the
session created, then set `status` to `ended`.

### `fix`

Varde refused the plan file, and the request names what failed. Rewrite the plan file so it parses
and every named Waypoint is in range, keeping the plan otherwise as it was.
