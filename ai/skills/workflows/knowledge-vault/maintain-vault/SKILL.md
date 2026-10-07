---
name: maintain-vault
description: Reorganise your knowledge Vault (rename, move, merge or split Notes, Kinds and Topics), or check it for broken links and missing indexes.
metadata:
  varde-vault: true
  varde-step: 5
  varde-asks: What do you want to change in the Vault? Say "check" for a health check.
---

# Maintain Vault

Change the Vault's structure without changing what it knows, and repair it where it has worn.

The line that handed you this Skill names this file, the workspace, the Vault, the Source map and
the user's request. Paths below are relative to the folder that holds this file.

## 0. Check the Vault has rules

Before anything else, check that the Note `Vault/Vault.md` exists in the Vault and links
`[[Writing knowledge Notes]]`. If it does not, reply only this, then stop:

> This Vault has no rules yet. Run **init-vault** from Varde, then run this again.

## 1. Read

1. Read `../shared/vault-format.md` in full.
2. Read the Vault rules.
3. List every Topic, Kind and Note in the Vault.

## 2. Plan the change

**A reorganisation** is the user's request turned into Notes:

- Rename or move a Note: one **move**.
- Rename a Kind or Topic, merge two, or split one: a **move** for every Note it carries. A renamed
  or merged Topic also renames its index Note and its Base.
- Merge Notes: a **supersede**.
- Split a Note: an **add** for each new Note, and an **update** for what stays.

Read every Note you will merge or split before you propose it. A Kind or Topic the user names that
fits the Vault rules poorly gets one line saying why, and stays the user's choice.

**A health check**, when the user asks for one, runs every check below on every Note and turns each
finding into the proposal that fixes it:

- a link whose target Note does not exist
- two Notes with the same file name
- a Note whose `topic` or `kind` does not match its folders
- a Note outside a Kind folder, other than its Topic's index Note
- a Topic without an index Note or a Base, or an index Note that misses a Note in its Topic
- a Note listed under `supersedes` that is not yet a pointer to its replacement

Done when every move, supersede, add and update the change needs is listed, or for a health check,
when every check has run across every Note.

## 3. Propose

Show the proposal as `vault-format.md` describes it, then revise until the user accepts. For a
health check, end with one line on how many Notes passed.

## 4. Write

Write the accepted changes by following "Writing to the Vault" in `vault-format.md`. Every Note
whose links a move rewrites is a changed Note, so it goes through staging too. Then report what
moved, merged, split and was repaired, and that the leak check printed `clean`.
