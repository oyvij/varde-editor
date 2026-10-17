# A file argument opens the Single-file editor

`varde path/to/file.txt` stops with `not-a-folder`. Varde only opens on a folder, and a folder
becomes a whole workspace: a tree, the Corner, the Strip, an AI pane and a Risk measurement. To edit
one config in `/etc`, a commit message or a note, the user opens the whole IDE around it or reaches
for another editor and loses Varde's highlighting, Language servers, Formatters and keys.

**`varde <file>` opens the Single-file editor: the editor alone, full screen, on that file. It writes
nothing but the file itself, and Varde's own state goes to a Sidecar that is deleted at exit.**

## The argument is still the switch

ADR 0016 made argv the switch: naming a folder asks for a project, naming none asks for a Bare
workspace. This extends the same rule by one case. **Naming a file asks for the Single-file editor.**
There is no flag and no config key, for the reason 0016 gave: reaching for Varde as an editor should
cost nothing to remember, and `varde file` is what `nvim file` already taught.

Varde still takes exactly one path. `varde a.txt b.txt` is rejected by the argument parser, and
`file:42` and `+42` are not parsed.

## The path rules

- **An existing file that can be read** opens the Single-file editor on it.
- **An existing file that can't be read** refuses with `file-not-readable`.
- **A missing path whose parent folder exists** opens the Single-file editor on an empty Buffer.
- **A missing path whose parent folder is missing** refuses with `no-such-folder`.
- **Folders are unchanged:** an existing one opens a project workspace, and one that can't be read
  refuses with `folder-not-readable`.

An unreadable file refuses rather than opening empty, because an empty Buffer over a file that
exists is one `:w` away from overwriting it. A read-only file is readable, so it opens; a `:w` that
fails is reported in the status line and the Buffer stays open.

A missing file is created on the first `:w`, through the write every Buffer already uses. Opening
and quitting without `:w` leaves nothing on disk.

## What the Single-file editor leaves out

It is not a workspace — there is no tree to be one. So it has none of what a workspace hangs off the
tree:

- **No tree, Corner, Strip or AI pane.** The editor takes the whole screen, Views other than Edit
  can't be entered, and the palette offers only Keys, Tools and Quit.
- **No project config layer.** Config is the shipped defaults plus `~/.varde/config.toml`. A
  `.varde/config.toml` beside the file has no effect, so how Varde behaves doesn't depend on where
  the file sits.
- **No global-config seeding.** Unlike a Bare workspace, a missing `~/.varde/config.toml` is not
  written. A quick edit doesn't write into the home folder; the shipped template still applies in
  memory, so Program rows work.
- **No `state.json`.** Nothing is read or saved, so no cursor, Buffer or Breakpoint survives a run.
- **No Risk, Story, Review or Debug.** None is measured, set or offered.
- **No shipped-Skill refresh and no Update check.** See below.

## Varde's state goes to a Sidecar

What Varde does write, such as a Language server's log, goes to a Sidecar keyed by the file's
**parent folder** and the pid, under ADR 0016's scheme. The Sidecar is deleted at exit, and the sweep
on start removes one a crash left behind. The parent folder is also the root: a Language server is
given it, so it finds the project around the file (`Cargo.toml`, `package.json`, …) where there is
one.

## Considered options

- **A flag, such as `varde --edit file`.** Rejected for the reason 0016 rejected one: nobody
  remembers to type it.
- **A Bare workspace on the file's folder, with the file open.** Rejected: it brings the tree, the
  Corner and the Strip back, and the point is the editor alone.
- **Refuse a missing path, as before.** Rejected: `varde new.txt` is how a file is started in every
  other editor.

## Consequences

**A mistyped folder no longer refuses.** `varde <missing-path>` used to stop with `no-such-folder`.
Now it opens an empty Buffer whenever the parent folder exists, so `varde ~/projets/varde` opens an
empty Single-file Buffer named `varde` instead of an error. Quitting without `:w` leaves nothing
behind, which is why this is accepted. `no-such-folder` now means the parent folder is missing.

**The shipped-Skill refresh and the Update check are left out by inference.** Nobody decided them
directly. They follow from "seeds nothing" — the refresh writes into `~/.varde/ai/skills/` — and from
a palette that offers no AI pane and no Update. The Spec asks the user to confirm both.

**Every Varde-owned write still routes through one accessor.** The Single-file editor sets the
Sidecar as a Bare workspace does, so the next feature that writes to `.varde` writes into the
Sidecar, not next to the file.
