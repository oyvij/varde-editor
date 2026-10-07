# A shipped Skill belongs to Varde

Varde ships Skills (`update-knowledge`, `search-knowledge`) and the agent they call
(`knowledge-searcher`) under `~/.varde/ai/`. ADR 0018 settled the opposite answer for the config
file — written once from the template, never over an existing file — and that answer would be
wrong here: a Skill is prose a later release is most likely to correct, and a user who never
touched it would keep the first version forever.

**The binary carries every shipped Skill and agent, and writes each on start wherever the file on
disk differs.** One writer covers a binary install, a self-update relaunch and a source build. A
hand edit to a shipped file is lost on the next start, and the file's first line says so (a `SKILL.md`'s first line
inside its frontmatter, since the file must open with `---`): to
change one, copy its folder under a new name. A Skill folder Varde did not ship is never read for
writing, never overwritten and never deleted.

## Considered options

- **Seed when missing**, as the config is. Rejected: improvements would never arrive.
- **Merge or ask before overwriting.** Rejected: a three-way merge of prose has no right answer,
  and a prompt on every upgrade teaches the reader to press yes.

## Consequences

Every Skill is open Agent Skills markdown — a folder with a `SKILL.md` and `name` and
`description` in its frontmatter — so any harness can follow one, and the Skills modal lists every
folder under `~/.varde/ai/skills/` with no release needed to add one. Varde reads two keys of its own
from `metadata`: `varde-vault`, which ADR 0025 explains, and `varde-asks`, the label of the question
box a Skill opens before it is handed over.
