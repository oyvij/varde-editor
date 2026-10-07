# Knowledge base

The knowledge base keeps what you learn in one folder of Markdown, the Vault, shared by every
workspace. You pick a Skill and the AI session in the AI pane writes Notes into the Vault or answers
from it. The Knowledge view lets you read the Vault without leaving your workspace. Obsidian can
open the same folder as a vault.

For the AI pane itself, see [ai-pane.md](ai-pane.md). For Markdown rendering, see the Preview in
[editing.md](editing.md).

## Turn it on

The knowledge base is off until your global config turns it on:

```toml
# ~/.varde/config.toml
[knowledge]
enabled = true
```

The Vault is `~/.varde/knowledge` unless you set `vault`. Varde creates the default Vault the first
time you open the Knowledge view. A Vault you name must already exist. If it does not, Varde refuses
to open the view and creates nothing.

```toml
[knowledge]
enabled = true
vault = "~/notes/brain"
```

A project config cannot turn the knowledge base on or move the Vault. Varde reads `[knowledge]`
only from `~/.varde/config.toml`.

## How the Vault is laid out

| Word | What it is |
|---|---|
| Note | One Markdown file in the Vault. |
| Topic | A folder at the root of the Vault, grouping the Notes that belong together. |
| Kind | A folder inside a Topic, naming what sort of Note it holds. A how-to and a reference page are written differently. |
| Vault rules | `Vault/Vault.md` and the Notes it links under `## Guidelines`. They decide what is worth keeping and how the Vault is organised. |
| Source map | `~/.varde/source-map.md`, outside the Vault. It gives each codebase on this machine a name that Notes use instead of a path. |

The Vault holds nothing that names this machine, so you can hand it to someone else whole. The
Skills write code references as `<Source map name>: <path inside that codebase>`, and they check
every Note for home folders, user names and absolute paths before writing it.

## Run a Skill

Open the Skills modal in any of three ways:

- Press `Ctrl+Space j`.
- Run `:skills`.
- Click the Skills chip on the AI pane's border.

Pick a Skill and Varde pastes one line into the AI session and submits it. The line points at the
Skill's `SKILL.md` and names the workspace. Varde never pastes the Skill's text. If no session is
running, Varde starts one and sends the line once the session is ready.

Some Skills ask a question first. They open a box, and your answer goes to the session with the
pointer. An empty answer or `Esc` sends nothing.

Varde ships one Workflow, `knowledge-vault`. The modal lists its Skills in the order you would use
them:

| Skill | What it does |
|---|---|
| `init-vault` | Sets up or revises the Vault rules, one question at a time. Run it first. |
| `update-knowledge` | Proposes what this session learned as Notes, and writes the ones you accept. It offers to add the workspace to the Source map. |
| `search-knowledge` | Asks what you are looking for and answers from the Vault and the code it names, citing the Notes it used. |
| `sweep-knowledge` | Compares Notes with the code they name and proposes updates where they have drifted apart. |
| `maintain-vault` | Renames, moves, merges or splits Notes, Kinds and Topics. Answer "check" for a check of broken links and missing indexes. |

A session learns where the Vault is only from a Skill you picked. Starting the AI on its own tells
it nothing about the Vault.

### Write your own Skill

A Skill is a folder under `~/.varde/ai/skills/` holding a `SKILL.md` with `name` and `description`
in its frontmatter. Add the folder and the modal lists it under Global:

```markdown
---
name: standup
description: Summarise today's work as a standup update
---
Write a three-line standup.
```

Three optional keys under `metadata` change how Varde hands a Skill over:

| Key | Effect |
|---|---|
| `varde-vault: true` | The pointer also names the Vault and the Source map. Without it, the session is told about neither. |
| `varde-asks: "<question>"` | Varde asks this question before handing the Skill over. |
| `varde-step: <number>` | The Skill's place in its Workflow's list. |

A folder under `skills/workflows/<name>/` is a Workflow, and its Skills are listed under its name.
The modal lists a `SKILL.md` it cannot read dimmed with the reason, and picking it sends nothing.

With the knowledge base off, the modal still lists your own Skills but not the shipped ones.

### Shipped Skills are replaced on start

Varde writes its Workflows into `~/.varde/ai/skills/workflows/` on start. If any shipped file there
differs from the one in the binary, Varde deletes the whole `workflows/` folder and writes it again.
Edits to a shipped Skill do not survive a start, and neither does anything else you put in
`workflows/`. Varde never touches a Global Skill, so put your own Skills directly under `skills/`.

## Read the Vault in the Knowledge view

Press `Ctrl+Space w` or run `:knowledge` to switch to the Knowledge view. Do it again to return to
the view you came from, with the same files open and the cursor where you left it.

The Knowledge view points four things at the Vault:

- The tree shows the Vault, and a Note written while you look appears in it.
- Filter narrows the Vault.
- Find searches the Vault, not the workspace.
- The editor opens a Note in the Preview.

The terminal, the AI pane, git and the language servers stay with the workspace. Varde draws no
Change bars on a Note and runs no git command in the Vault.

Notes are read-only in the Knowledge view. Insert mode, editing keys, Replace and `:w` refuse, and
the tree offers no file actions. Only a Skill writes the Vault. You can still select a passage and
send it to the AI with the AI Inject chip on the editor's border.

Varde does not remember the Knowledge view across a restart. It starts in Edit view.

### Follow a wikilink

Put the cursor on a `[[Name]]` link and press `gd` to open the Note it names. Links resolve the way
Obsidian resolves them: `Name.md` anywhere under the tree's root, or `[[Folder/Name]]` when two
files share a name. Varde refuses an ambiguous link rather than guess. Wikilinks work in source and
in the Preview, and in Edit view they resolve against the workspace.

## See also

- [ai-pane.md](ai-pane.md): the session Skills are handed to.
- [configuration.md](configuration.md): the `[knowledge]` table.
- `docs/adr/0025-the-vault-is-told-never-found.md`: why a session learns about the Vault only from a
  Skill.
- `docs/adr/0026-a-shipped-skill-belongs-to-varde.md` and
  `docs/adr/0028-a-shipped-workflow-is-replaced-whole.md`: why shipped Skills are replaced.
