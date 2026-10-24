# A tree delete is undoable for the session

Delete in the tree (`d` or the delete icon) ran `rm` or `rm -r` at once, with no confirmation and no
trash. One mis-press lost a file or folder for good, and the files an AI agent has just written are
often untracked, so git could not bring them back either. It was the one action in Varde that could
not be undone.

**A tree delete is a visible `mv` into a trash folder for this session under `~/.varde`. `u` in the
tree, or its ` ↶ u ` Chip, moves the newest delete back, and pressing it again the one before. Undo
lasts only this session: quitting empties the trash.**

## Still a visible command, still at once

The tree touches the filesystem only through commands typed into the terminal, so you can read
exactly what ran. Delete keeps that: it types `mkdir -p <trash>` then `mv <path> <trash>/<n>-<name>`,
and restore types `mkdir -p <parent>` then `mv -n <trash>/<n>-<name> <path>`. Files and folders take
the same `mv`. `n` is a counter for the session, so two deleted items with the same name never
collide. Delete still runs with no confirmation; staging one is a different question (#185). Both
commands go through the tree's existing split-and-wait path, so a slow `mv` of a big folder runs in
the shell and never freezes Varde.

## Nothing is written in the project

The trash is `~/.varde/trash/<flattened-root>-<pid>/`, named like the Sidecars under `~/.varde/paths`
and for the same reasons ADR 0016 gave: two instances on one folder never share a trash, and the
sweep has one rule. A trash folder inside the project would show up in `git status` or need a
`.gitignore` line, and a Bare workspace would no longer leave nothing behind. A named project and a
Bare workspace both get one.

## A restore never overwrites

If the tree's listing shows something at the original path, `u` runs nothing and says what is in the
way. `mv -n` covers the race where the path appears between that check and the command landing. A
lost restore can be tried again; a file overwritten by a restore is the loss this exists to prevent.

## Recorded only once the listing shows the path gone

Issuing a delete records a pending pair of original and trash path. It joins the undo list only when
the tree's listing shows the original gone, which is also when the Notify naming it appears. A delete
that never lands, because the `mv` failed or the shell never ran it, is never undoable. Likewise a
restore stays the newest entry, and `u` answers that it is still restoring, until the listing shows
the path back, so one delete is never restored twice.

## Emptied on quit, swept after a crash

Quitting deletes the trash folder, for every workspace and not only those with a Sidecar, so no
client file stays on disk. A crash skips that, so the startup sweep also sweeps `~/.varde/trash` and
deletes each folder whose pid is dead, the way ADR 0014 and 0016 catch a crash. That is why undo does
not survive a restart, and every message about it says so.

## Considered options

- **The OS trash or recycle bin.** Rejected: it needs a crate per platform, it is not a command the
  terminal shows, and what it keeps outlives the session, which is a promise Varde would then have to
  keep.
- **A trash folder in the project's `.varde/`.** Rejected: a Bare workspace has none, and a named
  project would carry deleted files beside its committed artifacts.
- **Recording the delete when it is issued.** Rejected: the undo list could hold a delete that never
  happened, and `u` would type an `mv` from a trash path that is empty.

## Consequences

**No new dependency.** Plain `mv` and `mkdir -p`, no trash crate and no timer.

**Deleted files sit in the home folder until quit.** A big folder deleted by mistake costs its size
under `~/.varde` for the session, and a `mv` across filesystems copies it, which takes a while.
