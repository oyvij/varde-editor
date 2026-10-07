# A shipped Workflow is replaced whole

ADR 0026 had Varde rewrite each shipped file whose text differs. The knowledge Skills are now a
Workflow — five Skills, an agent and shared files under `~/.varde/ai/skills/workflows/` — and a
release may rename, split or drop any of them. Rewriting file by file leaves the dropped ones
behind, listed in the modal and pointing at files that no longer exist.

**When any shipped file differs on disk, Varde deletes `skills/workflows/` and every folder an
earlier release shipped (`ai/agents/`, `skills/update-knowledge/`, `skills/search-knowledge/`), then
writes every shipped file.** `skills/workflows/` belongs to Varde: anything added inside it is lost
on the next update. A Global Skill, directly under `skills/`, is never touched.

## Considered options

- **Delete all of `~/.varde/ai/`.** Rejected: it takes the user's Global Skills with it.
- **Keep a manifest of what was written and delete only that.** Rejected: one more file to keep in
  step, for a folder the user has no reason to write into.

## Consequences

A shipped file no longer opens by saying it is replaced: the folder it lives in says so. To change
a shipped Skill, copy it to a Global folder. A Skill's `metadata.varde-step` orders it within its
Workflow; the order is the order a user runs them in.
