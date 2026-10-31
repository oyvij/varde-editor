# Gone

Gone lists the named items a change removed. Ask an AI for a small fix and it can rewrite a whole
file and drop a function that was working along the way. In a long diff that is mostly reshuffled
lines, that deletion is easy to miss. The [Risk](risk.md) delta does not catch it either, because
it measures the code that is still there, not the code that is gone.

Gone is a fact about the change, not a verdict on it. Removing a function is often exactly what was
wanted. Gone tells you it happened so that you can check.

## What Gone compares

In [Review view](review.md), Gone compares the named items in `HEAD` with those in the working tree,
in the files under review. It reads the files and git, and nothing else. It never reads the AI pane,
so what the AI says it did plays no part.

An item is known by its file, its container and its name. The container is the impl, class, trait
or namespace the item sits in. Gone tracks these kinds:

- functions;
- methods, in an impl, class, struct, trait or interface;
- classes;
- traits;
- interfaces;
- C and C++ structs.

The languages are the ones Risk measures: Python, Rust, C/C++, Java, JavaScript and
TypeScript/TSX. In a C or C++ header, a function with a body is measured like any other, so a
function body moved from a `.h` to a `.c` shows as `moved`.

## The three labels

Each row in the list carries one of three labels, followed by a sentence that says why the item is
listed. The labels and sentences are exactly these:

| Label | When | Sentence |
|---|---|---|
| `gone` | the item was in `HEAD` and nothing matching it is there now | `existed in HEAD, not in this file now` |
| `moved` | the item is no longer at the same file and container, but an item with the same name and the same body now lives in another file or container | `same body now lives in <file>` |
| `signature changed` | the item is still at the same file, container and name, but its parameters or return type changed | `still here; its parameters or return type changed` |

The body comparison ignores whitespace, so reindenting a moved function keeps it `moved`.

A `gone` item whose name now turns up somewhere else with a different body gets a second line under
it: `same name now in <file> <name>; its body differs`. It was moved and edited at once, so Gone
cannot tell you it is the same item. You can check the rewrite yourself.

A `signature changed` row shows the old and the new signature under it, after `was:` and `now:`.
They sit side by side when each gets at least 30 columns, and `was:` is stacked over `now:` when
the list is narrower.

Same-named overloads are paired by body first and then in source order, so they are not reported
as gone. An old overload left without a partner is `gone`.

An item whose name and signature stayed the same is not listed, even if its body changed. Gone is
about what went missing, not what was edited.

## The Chip

When the change removed something, a Chip appears on the tree pane's top border, beside
the Risk delta:

```
 ◌ gone 2 · x :gone
```

The count covers the `gone` rows only. `moved` and `signature changed` rows are in the list but
are not counted, because nothing is missing there. The Chip is drawn in the theme's plain
foreground, never in red.

| Chip | What it means |
|---|---|
| no Chip | nothing named was removed and every changed file was measured, or Gone is still measuring |
| `gone 2` | two named items were removed |
| `gone 2 · 1 not measured` | two were removed, and one changed file could not be measured, so the count may be short |
| `gone · 1 not measured` | nothing measured was removed, but one changed file could not be measured |
| `gone 2 stale` | the last result, dimmed, because a file was saved since it was measured |

Gone never shows `gone 0`. A zero would claim more than Gone knows when a file went unmeasured.

While Gone is measuring there is no Chip. The `measuring the change` spinner already on the border
covers both.

On a narrow terminal the Chip sheds its keys first, then shortens to `gone 2 · 1?`, then to just
` ◌ `. It is never cut mid-word. Clicking the Chip opens the list.

## The list

Open the list with palette `x` (`Ctrl+Space x`, "Gone" under Views), with `:gone`, or by clicking
the Chip. Both keys work only in Review view. The palette does not offer `x` anywhere else. The list
opens over the centre diff, at the diff pane's full width.

| Key | In the Gone list |
|---|---|
| `j` `k` / `Down` `Up` | move the selection |
| `Enter` | jump to the item's lines in the Review diff |
| `Esc` | close the list and go back to the diff |

`Enter` on a `gone` or `moved` row closes the list, selects the old file in the changed-file list,
and scrolls its diff to the item's first removed line, with focus in the diff. `Enter` on a
`signature changed` row lands on the changed line in that file's diff. A jump only moves you within
Review view. It opens no file in the editor.

Every line in the list wraps inside its box, never off the edge. A long path, name or signature
wraps onto the next line, indented under the row text rather than under the label.

The list can say:

- `computing`, while a result is on its way;
- `stale`, above the last result, which stays up until a new one replaces it;
- `nothing measured yet`, before any result has arrived;
- `nothing named was removed`, when the result has no rows.

### Files Gone could not measure

When a changed file could not be measured, a header at the top of the list names it and its
language, labelled `not measured`, for example `src/server.go (Go)`. Read those files yourself.

Every changed C or C++ header is listed there too, with `prototypes and macros`, for example
`src/shape.h (C/C++): prototypes and macros`. Gone measures the functions in a header that have a
body, but not the prototypes and macros around them.

## When Gone computes

- **On entering Review view.** Gone rides on the Risk measurement Review already runs, not on a
  second one.
- **When Risk recomputes in Review view**, with the Risk pane's recompute action.

Never on a keystroke and never on a save. Saving marks the result **stale**: the Chip says `stale`
and dims, and the list keeps the last result, marked stale, until the next measurement. Parsing
runs in the background, so typing never waits on it. Each version of a file is parsed once a
session, held in memory.

## What Gone does not cover

These are not tracked yet, so removing one does not show up in Gone:

- enums;
- consts;
- type aliases;
- C and C++ prototypes and macros;
- Rust structs. Their methods are tracked, under the impl named after the struct, so a removed
  struct whose methods went too still shows through those methods;
- trait methods without a body.

## What Gone never does

- **It grades nothing.** No verdict, no pass or fail, no red. A removed item may be exactly what the
  change was for.
- **It runs nothing.** No tests, no build, no command.
- **It writes nothing into the repo.** No file under `.varde/`, nothing in the Sidecar, and
  `git status` is the same after a review as before it. Its results live in memory only.

## See also

- [Review](review.md): the view Gone works in, and the diff a row jumps to.
- [Risk](risk.md): the delta beside the Chip, and the measurement Gone rides on.
