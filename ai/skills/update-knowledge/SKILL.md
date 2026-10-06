---
# Shipped with Varde and replaced on every update: copy this folder under a new name to change it.
name: update-knowledge
description: Propose what this session learned as Notes in the user's knowledge Vault, and write them once the user accepts.
metadata:
  varde-vault: true
---

# Update Knowledge

Keep what this session learned. You propose takeaways and the Notes they become. You write
nothing until the user accepts. Then you place, link and index the Notes so the Vault stays
navigable as an Obsidian vault.

## What you were given

The line that handed you this Skill names four things:

- **This file.** The agent you call lives at `../../agents/knowledge-searcher.md`, relative to
  the folder that holds this file.
- **The workspace.** The repository this session is about.
- **The Vault.** The folder of markdown Notes the user keeps across every workspace.
- **The Source map.** A file outside the Vault that maps a short name for each codebase to where
  it lives on this machine.

You know the Vault exists only because the user picked this Skill. Read and write it only to carry
out this Skill. When you are done, do not open the Vault again unless the user picks a Skill again.
Never copy the Vault's path or the Source map's path into the workspace, into a memory file, into
configuration or into an environment variable.

## Words

- **Note**: one markdown file in the Vault.
- **Topic**: a folder at the root of the Vault. It groups the Notes that belong together.
- **Kind**: a folder inside a Topic. It names what sort of Note it holds, and so how that Note is
  written. A step-by-step guide reads differently from a reference page, and both read
  differently from a record of a conversation.

The Vault's Topics and Kinds are the user's. They grow from the user's content. No list of them
exists anywhere else, and you must not bring one from another Vault, another project or an
example.

## 1. Read before you propose

1. **Read the Vault.** List its Topics and the Kinds inside each one. Read the index Note of every
   Topic the session touches, and the Notes whose names or frontmatter match the session's
   subject. A large Vault is better scanned by the knowledge-searcher agent: give it the
   session's subject as its question (see "Calling the agent" below) and work from its report.
2. **Read the session.** Find what was learned: decisions and their reasons, how something
   behaves, answers to questions somebody asked, mistakes and what fixed them.
3. **Read the workspace** wherever the session's knowledge depends on code. Confirm that what you
   will write is still true of the source.
4. **Read the Source map**, if it exists, and find the workspace's name in it.

If the session holds nothing worth keeping, for example because it just started or it was about
something else, go to "When there is nothing to capture".

## 2. Propose

Show the proposal in the conversation as two short bullet lists, then ask.

**Takeaways.** One bullet per piece of knowledge, one line each. This is a summary for review,
not the text you will write. The Notes themselves are fuller: they carry the context, the
reasons, the examples and the links a reader needs months later.

**Notes.** One bullet per Note, each starting with what happens to it:

- **add** `Topic/Kind/Name`: a new Note.
- **merge into** `Topic/Kind/Name`: new knowledge joins an existing Note on the same subject.
- **update** `Topic/Kind/Name`: an existing Note that the new knowledge contradicts or makes
  out of date. Say in a few words what it says now and what it will say.

Put every Note in a **Topic** and a **Kind**:

- Choose from what the Vault already has. Prefer an existing Topic or Kind whenever the knowledge
  fits it.
- When a Topic or a Kind would be new, mark it `(new Topic)` or `(new Kind)` in the bullet, and
  name the closest existing one, so the user does not end up with near-duplicate folders.
- In an empty Vault, propose Topics and Kinds from this first write alone, and say that they are
  all new.
- Choose a Kind for what the content is, not from a fixed list. If the same knowledge fits more
  than one Kind, propose one Note per Kind, each written in that Kind's style, and say so.
- Keep **product** knowledge and **engineering** knowledge in separate Notes. Product Notes cover
  what the product does and why, and how people use it. Engineering Notes cover how it is built.
  When a takeaway has both sides, propose two Notes that link to each other.

End with a question: does the user accept, or what should change?

## 3. Revise until accepted

The user may change anything: a takeaway, a Note, a Topic, a Kind, a name, or whether something
is kept at all. Each time, show both lists again, revised. The user has the final say on every
folder name. Write nothing until the user accepts. "Yes", "go ahead" or "accept" count, and so
does an answer that accepts with small changes that need no new proposal.

## 4. Write

Give every Note this frontmatter:

```yaml
---
kind: <the Kind folder's name>
topic: <the Topic folder's name>
audience: product        # or engineering
source: <where this came from: a Source map name with a path inside that codebase, a public URL, or "conversation">
updated: <today, YYYY-MM-DD>
supersedes: []           # names of Notes this one replaces, as [[wikilinks]]
---
```

- Write in the Kind's style. Match the Notes already in that Kind.
- Name a Note by its subject in plain words. Its file name is how others link to it, so it must
  be unique across the whole Vault. Before you write a Note, check that no other Note anywhere in
  the Vault has the same file name.
- **A Note the new knowledge contradicts is rewritten in place,** never left stale beside a new
  one. Its body says what is true now. Add or extend a `## History` heading at the end, with one
  dated entry that says what the Note used to say and why it changed. Update its `updated` date.
- When a new Note replaces whole Notes, list them under `supersedes`, rewrite each replaced Note
  as a short pointer to the new one, and record that under its own `## History`.
- **Name code by its Source map name,** never by a path on this machine: `<Source map name>:
  <path inside that codebase>`, never the folder it is cloned into. Repository names and remote
  URLs are allowed, so a colleague can find the same code.

### Nothing in a Note names this machine

Before anything reaches the Vault, write every Note you will add or change into a staging folder
outside the Vault and outside the workspace, for example one made with `mktemp -d`. Then run this
check on that folder:

```sh
staged="<the staging folder>"
found=0
home="${HOME:-$USERPROFILE}"
user="$(id -un 2>/dev/null || printf '%s' "${USER:-$USERNAME}")"
host="$(hostname 2>/dev/null || uname -n)"
[ -n "$home" ] && grep -rnF -e "$home" "$staged" && found=1
for word in "$user" "$host" "${host%%.*}"; do
  [ -n "$word" ] && grep -rniwF -e "$word" "$staged" && found=1
done
grep -rnE -e '(^|[^[:alnum:]:/.~_-])(~/|file:/|/[[:alnum:]._-]+/|[A-Za-z]:[\\/])' "$staged" && found=1
[ "$found" = 0 ] && echo "clean"
```

It refuses the home folder, the user name, the host name and anything that looks like an absolute
path. Every line it prints is a refusal: rewrite that line, using the Source map name, a path
relative to a codebase or a remote URL, then run the check again. Copy the staged Notes into the
Vault only when the check prints `clean`. If a line it prints names nothing about this machine,
for example a URL route like `/api/orders/` or an ordinary word that happens to be the user name,
show the user the line and copy only if they confirm.
If you cannot run a shell, say so, then make the same four checks by reading every staged line.

## 5. Link, index and finish

After the Notes are written, finish the Vault:

1. **Link pass.** Read the Notes you wrote and the Notes related to them. Add `[[wikilinks]]`
   between related Notes, in both directions, where a reader of one would want the other. Link
   each product Note to its engineering counterpart, and the other way round. Link by file name,
   `[[Name]]`. Use `[[Topic/Kind/Name]]` only when two Notes share a name, and
   `[[Name|shown text]]` to change the shown text.
2. **Index Note per Topic.** Each Topic you touched has an index Note named after the Topic,
   `Topic/Topic.md`. It says in a few lines what the Topic covers and links every Kind's Notes.
   Create it if it is missing, and update it when Notes are added or renamed. A plain `index.md`
   in every Topic would give the Vault many Notes with the same name, and their links could not be
   told apart.
3. **A `.base` per Topic.** Each Topic you touched has an Obsidian Base named after the Topic,
   `Topic/Topic.base`, which lists the Topic's Notes with their frontmatter. Embed it in the index
   Note with `![[Topic.base]]`. Create it if it is missing, for example:

   ```yaml
   filters:
     and:
       - file.inFolder("<Topic>")
       - file.ext == "md"
   views:
     - type: table
       name: <Topic>
       order:
         - file.name
         - kind
         - audience
         - updated
   ```

4. **Source map.** If the session's code is not in the Source map yet, offer to add it. On
   acceptance, add one line, creating the file if it is missing:
   `- <name>: <absolute path to the workspace> (<remote URL, if there is one>)`. Use the
   repository's name as the name unless the user picks another. The Source map is the one place
   a path on this machine may be written. It is never inside the Vault.
5. **Report.** List the Notes you added, merged into and updated.

## 6. Offer a sweep

Last, ask whether to sweep the Source map. A sweep reads the engineering Notes, and the product
Notes that describe behaviour the code decides. It compares each of them with the code its
`source` names, through the Source map. Then it proposes updates where the Note and the code have
drifted apart. Leave Notes about people, customers, plans or strategy out of a sweep: code cannot
correct them. A sweep's changes are a proposal like any other. Show them as an **update** list,
revise on the user's changes, write on acceptance, and run the check and the link pass again.

## When there is nothing to capture

If the session is empty or has nothing worth keeping, say so in one line. Then ask what the user
wants to do with the knowledge base, for example:

- capture something they describe now,
- reorganise, rename or merge Topics, Kinds or Notes,
- run the link pass and rebuild the index Notes and Bases,
- sweep the Source map against the code,
- add this workspace to the Source map.

Whatever they choose follows the same rules: propose, revise, write on acceptance, check, link.

## Calling the agent

The knowledge-searcher agent answers a question from the Vault and the code in the Source map.
If your harness can start a sub-agent with its own context, start one. Give it the full text of
the agent file as its instructions, and give it the question and the Vault's and the Source
map's paths as its task. If your harness cannot start a sub-agent, follow the agent file
yourself, and keep only its report in the conversation.
