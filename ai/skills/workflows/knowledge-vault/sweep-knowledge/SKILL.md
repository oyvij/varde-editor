---
name: sweep-knowledge
description: Compare the Notes in your knowledge Vault with the code they name, and propose updates where they have drifted apart.
metadata:
  varde-vault: true
  varde-step: 4
---

# Sweep Knowledge

Find the Notes that no longer match the code they describe, and propose the fixes.

The line that handed you this Skill names this file, the workspace, the Vault and the Source map.
Paths below are relative to the folder that holds this file.

## 0. Check the Vault has rules

Before anything else, check that the Note `Vault/Vault.md` exists in the Vault and links
`[[Writing knowledge Notes]]`. If it does not, reply only this, then stop:

> This Vault has no rules yet. Run **init-vault** from Varde, then run this again.

## 1. Read

1. Read `../shared/vault-format.md` in full.
2. Read the Vault rules. They set the level of detail a Note is checked at.
3. Read the Source map.
4. List every Note whose `source` names a Source map name. Those are the sweep's Notes. If the user
   named Topics, keep only the Notes in them.
5. Fetch every codebase the sweep's Notes name (`git fetch origin`) before comparing anything. If
   any fetch fails, stop: list those codebases with the error, and ask the user to fix access or to
   accept a sweep of them against the local copy. Compare against the remote's default branch
   (`origin/HEAD`; if it is missing, run `git remote set-head origin --auto`), never the working
   copy: the user's checkout may sit on a feature branch, hold uncommitted changes or be months
   old. Read it without touching the checkout, for example through a detached worktree in a
   temporary folder that you remove afterwards. Never pull, switch branches or stash in the user's
   checkout.

## 2. Compare

For each sweep Note, read the code its `source` names on the remote's default branch, and mark the
Note:

- **current**: the code still says what the Note says, at the Vault rules' level of detail.
- **drifted**: the code changes what the Note says at that level. Finer drift leaves it current.
- **unverifiable**: its codebase is missing from the Source map, its path no longer exists, or its
  fetch failed. Say which.

Done when every sweep Note carries a mark.

## 3. Propose

Say in one line which branch and commit date each codebase was compared at. Mark every Note
compared against a local copy instead, with its branch and last commit date. Show the drifted Notes
as a proposal, as `vault-format.md` describes it, then the unverifiable
Notes with the reason for each. Say in one line how many Notes are current and what finer drift
you left out. Revise until the user accepts.

## 4. Write

Write the accepted changes by following "Writing to the Vault" in `vault-format.md`. Then report
the Notes you updated and superseded, and that the leak check printed `clean`.
