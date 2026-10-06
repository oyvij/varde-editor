---
# Shipped with Varde and replaced on every update: copy this folder under a new name to change it.
name: search-knowledge
description: Answer the user's question from their knowledge Vault, and from the code it names, citing the Notes the answer came from.
metadata:
  varde-vault: true
  varde-asks: What are you looking for?
---

# Search Knowledge

Answer the user's question from the Vault without filling this session with search noise.

## What you were given

The line that handed you this Skill names this file, the workspace, the Vault, the Source map and
the user's question.

- **The Vault** is the user's folder of markdown Notes, kept across every workspace.
- **The Source map** is a file outside the Vault that maps a short name for each codebase to
  where it lives on this machine.
- **The knowledge-searcher agent** does the search. Its file is `../../agents/knowledge-searcher.md`,
  relative to the folder that holds this file.

You know the Vault exists only because the user picked this Skill. Read it only to answer this
question, and write nothing to it. Never copy the Vault's path or the Source map's path into the
workspace, into a memory file, into configuration or into an environment variable.

## Search

1. If your harness can start a sub-agent with its own context, start one. Give it the full text of
   the agent file as its instructions. Give it the question, the Vault's path and the Source
   map's path as its task, plus one line on what this session is doing if that helps the search.
2. If your harness cannot start a sub-agent, follow the agent file yourself. Keep only its report
   in the conversation, not every file you opened along the way.

## Answer

Answer in the conversation from the agent's report:

- Answer the question first, in a few lines.
- Cite the Notes the answer rests on by name, as `[[Name]]`, so the user can open them. Quote a
  short passage only where the exact wording matters. Never paste a Note whole.
- Where the answer came from code rather than a Note, name the code by its Source map name and a
  path inside that codebase.
- Say plainly when a Note and the code disagree, and which one you trust and why.
- Say plainly when the Vault does not answer the question. Do not fill the gap with a guess
  presented as knowledge.

If the answer belongs in the Vault and is not there yet, or a Note turned out to be wrong, say so
and suggest the update-knowledge Skill. Do not write the Vault from this Skill.
