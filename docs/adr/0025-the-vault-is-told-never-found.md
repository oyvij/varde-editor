# The Vault is told, never found

The Vault is the user's knowledge, kept across every workspace, and a hosted AI session can read any
file the user can. Varde cannot fence it off: ADR 0004 makes a hosted pane a terminal Varde hosts,
not a process Varde sandboxes, and a sandbox would be a branch per provider by another name. So the
promise is made where Varde does have a say — **what the session is told**.

**A session learns the Vault exists only from a Skill the user picked.** Varde never puts the
Vault's path in a child's environment, in a prompt it writes for any other feature, or in a file
inside the workspace. A Skill whose frontmatter opts in (`metadata: { varde-vault: true }`) gets the
Vault's and the Source map's paths in the one-line prompt Varde pastes when it is picked; no other
Skill does. The shipped Skills say, in their own words, that they read and write the Vault only
while invoked. That is a promise kept by instruction, and it is written down here so nobody later
mistakes it for enforcement — or "fixes" it by adding an environment variable every session would
inherit.

**Only a Skill writes the Vault.** The Knowledge view reads, finds and filters Notes and edits
none: insert mode, Replace and the tree's new, rename and delete are refused there. A Note is
written by an AI session following a Skill — proposing takeaways, placing them in a Topic and Kind
the user accepted, linking them — or it is not written. Two authors would be two conventions for
frontmatter, links and folders, and the Vault's shape is the Skill's to keep.

**Nothing in the Vault names this machine.** The Source map, outside the Vault, is the one place a
local path lives; a Note names code by the Source map's name for it. The shipped Skill runs a
scripted check before writing, because a CLI can verify its own output with a program
(ADR 0006) and a sentence asking it to be careful cannot.

## Considered options

- **An environment variable naming the Vault** for every hosted child. Rejected: every session
  would know, whether the user asked or not.
- **Editing Notes in the Knowledge view.** Rejected: the view is for recall, and a hand edit is the
  one write that skips the privacy check and the link pass.

## Consequences

Varde never interprets what the session wrote (ADR 0006): a Skill's whole conversation — the
proposal, the user's changes, the acceptance — happens in the AI pane, and the file watcher shows
the result in the Knowledge view. A Vault under git is the user's own business; Varde reads none of
its history and runs no git there.
