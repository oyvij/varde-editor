---
name: search-knowledge
description: Answer your question from your knowledge Vault, and from the code it names, citing the Notes the answer came from.
metadata:
  varde-vault: true
  varde-step: 3
  varde-asks: What are you looking for?
---

# Search Knowledge

Answer the user's question from the Vault, and keep the search itself out of this session.

The line that handed you this Skill names this file, the workspace, the Vault, the Source map and
the user's question. Paths below are relative to the folder that holds this file. This Skill only
reads: the Vault's path and the Source map's path stay in this conversation.

## 0. Check the Vault has rules

Before anything else, check that the Note `Vault/Vault.md` exists in the Vault and links
`[[Writing knowledge Notes]]`. If it does not, reply only this, then stop:

> This Vault has no rules yet. Run **init-vault** from Varde, then run this again.

## 1. Search

If your harness can start a sub-agent, start one: its instructions are the full text of
`../agents/knowledge-searcher.md`, its task is the question, the Vault's path, the Source map's
path, and one line on what this session is doing if that helps the search. Otherwise follow the
agent file yourself.

## 2. Answer

Answer in the conversation from the agent's report:

- Answer the question first, in a few lines.
- Cite the Notes the answer rests on as `[[Name]]`, so the user can open them. Quote a short
  passage only where the exact wording matters.
- Name code by its Source map name and a path inside that codebase.
- When a Note and the code disagree, say so, and which one you trust and why.
- When the Vault does not answer the question, say so plainly and leave the gap open.

If the answer belongs in the Vault and is missing, suggest **update-knowledge**. If a Note turned
out to be wrong about the code, suggest **sweep-knowledge**.
