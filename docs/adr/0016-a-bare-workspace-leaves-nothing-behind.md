# A bare workspace leaves nothing behind

`varde <folder>` opens a project: it creates `.varde/` in that folder, seeds a commented
`config.toml`, measures Risk unasked, and from then on keeps `state.json`, `risk.json`, `stories/`,
`reviews/`, `snapshots/` and a log per language server there. That is right for a repository somebody
works in, and wrong every other time Varde is opened — to read one file, to edit a config somewhere
in `/etc`, to review a branch of a repository that is not even on the machine.

**`varde` with no argument opens a Bare workspace: the folder is the workspace, and nothing of
Varde's is written into it. Its state lives in a Sidecar under `~/.varde/paths/`, which is deleted
when Varde exits.**

## The argument is the switch

`varde` and `varde .` name the same folder and now behave differently. That is the only contract in
Varde that cannot be put on the cheatsheet, because it is argv rather than a key, and it was chosen
anyway: the alternatives are a flag nobody remembers to type or a global config key that makes the
behaviour of `varde` depend on a file, and the whole point is that reaching for Varde as an editor
should cost nothing to remember. Naming a folder is asking for a project. Naming none is not.

## What "leaves nothing behind" costs

Three things follow that would otherwise look like omissions.

**No project config layer.** Layering is built-in defaults, then `~/.varde/config.toml`, then the
project's. A Bare workspace has the first two and no third, and nothing is seeded. `CONTEXT.md`
justifies Seeding by "a key nobody can find is a key nobody sets" — and a key written into a
directory that is deleted before anyone could find it is worse than a key never written.

**No Risk at startup.** Measuring starts without being asked and caches the answer keyed off `HEAD`.
In a Bare workspace that cache dies at exit, so every launch would re-analyse the whole tree for a
result nobody keeps. The Risk pane still recomputes on demand; it just stops happening unasked. This
is the clearest case where *bare* means fewer features running, not the same features writing
somewhere else.

**A submitted review does not go in the Sidecar.** This is the one place the rule bends, and it bends
deliberately. `:submit` writes `.varde/reviews/NNNN.json`, which in a Bare workspace would mean a
file destroyed at exit — you read someone's branch, wrote comments against it, submitted, and quit,
and the comments are gone. So a review submitted from a Bare workspace is written to
`~/.varde/reviews/`, durable, outside every workspace, under the existing retention. **This reverses
part of an earlier decision**: `.varde/reviews/` and `.varde/stories/` are deliberately committed
project artifacts, not gitignored, and a review that lands in the global directory instead is not one.
The reversal is scoped to the workspace that has no project to commit to. A review is the output of
the reading, and losing an output is not the same kind of nothing as leaving no trace.

## The Sidecar is keyed by path and pid

`~/.varde/paths/<abs-path>-<pid>/` rather than `~/.varde/paths/<abs-path>/`. Keyed by path alone, two
Bare workspaces on the same folder share one Sidecar and the first to quit deletes the other's state;
and a sweep-on-start — which ADR 0014 established as the only thing that catches a crash — would
delete the Sidecar of an instance currently running. With the pid in the name, neither can happen,
and the sweep has one rule with no exceptions to get wrong: delete every directory whose pid is dead.
The cost is a directory name nobody can eyeball.

## Consequences

**One accessor, not seven call sites.** There are roughly seven places that build
`root.join(".varde/…")`. The Sidecar has to be a single function all of them route through, or the
next feature that writes to `.varde` writes into the user's project by accident and nothing fails.

**A Bare workspace forgets everything.** View, tree divider, expanded folders, Story position,
buffers — none of it survives, because `state.json` is in the Sidecar. That is not a shortcoming to
be fixed later with a retention setting: a Bare workspace that remembers is a project, and somebody
asking for one is asking for a different feature.

**Editing and saving are unaffected.** The file tree is the folder Varde was started in and `:w`
writes there. The Sidecar holds what belongs to Varde; the folder holds what belongs to the user.
This is the same line ADR 0014 drew for a Reading, read from the other side.
