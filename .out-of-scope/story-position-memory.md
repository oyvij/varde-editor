# Story position memory

Varde does not remember where a reviewer was in a Story. Leaving a Story with Escape and entering it
again starts at its first Step, and nothing about a Story position survives a restart.

## Why this is out of scope

`CONTEXT.md` already defined a Story position, then called a Walkthrough, as disposable: "started,
stopped and restarted freely".
Getting back to a Step is a few presses of `n`, so remembering it saves almost nothing, and it would
cost:

- a position per Story held in state, and written to disk so a restart could read it back
- a staleness rule for when the Story set is re-authored under a remembered position — the old spec
  had to discard it with a notice, because keeping a step index that now resolves to a *different*
  claim tells the reviewer they are where they left off when they are not

This was fully specified (R24.11 and Q62 in `docs/example-map.md`, three scenarios in
`features/walking_a_story.feature`) and never built, and its absence was never missed in daily use.
A behaviour nobody misses is not worth that storage and that rule.

## Prior requests

- #7: "There are 4 cucumber scenarios that fail." — the unbuilt Story position scenarios were removed
  rather than implemented
