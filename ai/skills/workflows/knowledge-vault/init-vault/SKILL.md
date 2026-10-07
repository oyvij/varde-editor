---
name: init-vault
description: Set up or revise your knowledge Vault's rules, one question at a time, so they match what you want the Vault for.
metadata:
  varde-vault: true
  varde-step: 1
---

# Init Vault

Write the Vault rules: what the Vault keeps and how it is organised. Every other Vault Skill reads
them before it touches a Note, and stops when they are missing.

The line that handed you this Skill names this file, the workspace, the Vault and the Source map.
Paths below are relative to the folder that holds this file.

## 1. Read

1. Read `../shared/vault-format.md` in full. It fixes the shape of a Note; the rules you write
   here decide content and organisation within that shape.
2. If the Note `Vault/Vault.md` exists, you are revising: read it and every Note it links under
   `## Guidelines`, and ask which decisions below the user wants to change. Grill only those.
3. List the Vault's Topics and Kinds and read a few Notes from each, so your recommendations rest
   on what is there. An empty Vault is fine.

## 2. Grill

Grill the user on each decision below, one question at a time, opening each with its number, for
example "3 of 7". Give your recommended answer and the reason for it with every question. When
the Vault or this session already answers a question, say what you found and ask only to confirm
it.

A decision is settled when the user accepts your recommendation or gives an answer of their own.
Ask a follow-up only when that answer contradicts another settled decision or `vault-format.md`,
and then settle the contradiction in one question. When the user tells you to take your
recommendations, they settle every open decision.

Topic and Kind names grow from content, so leave them to the first capture.

1. **Readers**: who reads the Vault: the user alone, a team, newcomers. This sets how much
   context a Note carries.
2. **Worth keeping**: the test a takeaway must pass. Recommend: still useful and true in six
   months, to a colleague who was not there.
3. **Level of detail**: what a Note holds and what it leaves to the code. Recommend: general
   knowledge; call chains, line numbers, edge cases and logs only when they change how someone uses
   or changes the system.
4. **Audiences**: the values of `audience`, and which of them get separate Notes. Recommend
   `product` (what it does, why, how people use it) and `engineering` (how it is built), in
   separate Notes linked both ways.
5. **Kinds**: how a Kind's style is set, and where knowledge goes that fits two Kinds. Recommend:
   a Kind's style is what its Notes already show; knowledge that fits two Kinds lives in one Note,
   linked from the other Kind's index.
6. **Change**: when a Note is updated and when it is superseded. Recommend: update while the
   subject stays the same, supersede when Notes are merged or split.
7. **Never in the Vault**: what is never written down, in the user's own words. Recommend:
   credentials, personal data, and anything naming this machine (the leak check enforces the last).

Done when every decision has an answer the user accepted.

## 3. Propose

Show both Notes in full, then revise until the user accepts, as `vault-format.md` describes.

- `Vault/Vault.md`, the Vault Topic's index Note: one paragraph on what the Vault is for and who
  reads it, then a section each for Audiences, Kinds, Change and Never in the Vault, then
  `## Guidelines` linking `[[Writing knowledge Notes]]`.
- `Vault/Guidelines/Writing knowledge Notes.md`: Readers, Worth keeping and Level of detail.

When revising, change the existing Notes in place, with a `## History` entry for each change.

## 4. Write

Write the accepted Notes by following "Writing to the Vault" in `vault-format.md`. Then tell the
user the Vault rules are in place and the other Vault Skills can run.
