---
name: update-knowledge
description: Propose what this session learned as Notes in your knowledge Vault, and write them once you accept.
metadata:
  varde-vault: true
  varde-step: 2
---

# Update Knowledge

Capture what this session learned into the Vault: propose, revise, write on acceptance, leak check,
link.

The line that handed you this Skill names this file, the workspace, the Vault and the Source map.
Paths below are relative to the folder that holds this file.

## 0. Check the Vault has rules

Before anything else, check that the Note `Vault/Vault.md` exists in the Vault and links
`[[Writing knowledge Notes]]`. If it does not, reply only this, then stop:

> This Vault has no rules yet. Run **init-vault** from Varde in this session, then run this again.
> Keep this session open so what it learned is not lost.

## 1. Read

1. Read `../shared/vault-format.md` in full. It fixes the shape of every Note and how Notes
   reach the Vault.
2. Read the Vault rules. They decide what is worth keeping, how much detail a Note carries, and
   how Topics, Kinds and audiences are chosen.
3. Read the session. Find what was learned: decisions and their reasons, how something behaves,
   answers to questions somebody asked, mistakes and what fixed them.
4. Find what the Vault already holds on the session's subject. If your harness can start a
   sub-agent, start one: its instructions are the full text of `../agents/knowledge-searcher.md`,
   its task is the session's subject, the Vault's path and the Source map's path. Work from its
   report. Otherwise follow the agent file yourself.
5. Read the workspace wherever a takeaway depends on code, and confirm it is still true of the
   source.

Done when every takeaway is confirmed against the source or needs none, and you know which
existing Notes each one touches.

If nothing in the session passes the Vault rules, say so in one line and stop.

## 2. Propose

Show two bullet lists, then ask whether the user accepts or what should change.

- **Takeaways**: one line each, a summary for review. Drop every takeaway that fails the Vault
  rules, and say in one line what you dropped.
- **Notes**: the proposal, as `vault-format.md` describes it. In an empty Vault, say that every
  Topic and Kind is new.

Revise until the user accepts, as `vault-format.md` describes.

## 3. Write

Write the accepted Notes by following "Writing to the Vault" in `vault-format.md`. Write each Note
in its Kind's style, as the Vault rules and the Notes already in that Kind show it. For `source`,
find the workspace's name in the Source map.

## 4. Finish

1. If the workspace is not in the Source map, offer to add it. On acceptance, add one line,
   creating the file if it is missing: `- <name>: <absolute path to the workspace> (<remote URL,
   if there is one>)`. Use the repository's name unless the user picks another.
2. Report the Notes you added, merged into, updated and superseded, and that the leak check
   printed `clean`.
