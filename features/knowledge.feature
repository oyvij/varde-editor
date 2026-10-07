Feature: Keeping knowledge

  The Vault is the user's markdown knowledge, kept across every workspace and openable as an
  Obsidian vault. Varde does three things with it: it hands a Skill to the AI session when the user
  picks one, it shows the Vault in the Knowledge view, and it keeps its own Skills current. What a
  Skill does — proposing takeaways, placing Notes, linking them — happens in the AI pane, in prose
  Varde never reads (ADR 0006). A session learns the Vault exists only from a Skill the user picked
  (ADR 0025).

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the home folder is "/home/me"
    And the effective setting "ai.command" is "claude"

  Rule: The knowledge base is off until the global config turns it on

    Scenario: With nothing configured the knowledge base is off
      Then the knowledge base is "disabled"

    Scenario: Turning it on uses the default Vault
      Given the global config holds:
        """
        [knowledge]
        enabled = true
        """
      Then the knowledge base is "enabled"
      And the Vault is "/home/me/.varde/knowledge"

    Scenario: A configured Vault is used, with the home folder expanded
      Given the global config holds:
        """
        [knowledge]
        enabled = true
        vault = "~/notes/brain"
        """
      Then the Vault is "/home/me/notes/brain"

    Scenario: A project cannot turn the knowledge base on
      Given the project config holds:
        """
        [knowledge]
        enabled = true
        """
      Then the knowledge base is "disabled"

    Scenario: A project cannot move the Vault
      Given the global config holds:
        """
        [knowledge]
        enabled = true
        """
      And the project config holds:
        """
        [knowledge]
        vault = "/tmp/elsewhere"
        """
      Then the Vault is "/home/me/.varde/knowledge"

    Scenario: A missing default Vault is created on first use
      Given the knowledge base is enabled with the default Vault
      And the folder "/home/me/.varde/knowledge" does not exist
      When I toggle the Knowledge view
      Then the folder "/home/me/.varde/knowledge" was created
      And the view is "knowledge"

    Scenario: A missing configured Vault is refused and not created
      Given the knowledge base is enabled with the Vault "/home/me/notes/brain"
      And the folder "/home/me/notes/brain" does not exist
      When I toggle the Knowledge view
      Then no folder was created
      And the view is "edit"
      And the reviewer is told "no-such-vault"

  Rule: The Skills modal lists every Skill on disk

    A Skill is a folder under `~/.varde/ai/skills/` holding an Agent Skills `SKILL.md`. Its row is
    the `name` and `description` in the frontmatter, so adding one is adding a folder. A Skill
    directly under `skills/` is Global and listed first; one under `skills/workflows/<workflow>/`
    is listed under its workflow, in the order its `metadata.varde-step` names. A folder with no
    `SKILL.md`, such as a workflow's `shared/`, is not a Skill.

    Background:
      Given the knowledge base is enabled with the default Vault

    Scenario: The shipped Skills are listed
      When I run ":skills"
      Then the modal is "skills"
      And the Skills modal lists "update-knowledge"
      And the Skills modal lists "search-knowledge"

    Scenario: A Skill the user added is listed beside them
      Given the Skill folder "standup" holds:
        """
        ---
        name: standup
        description: Summarise today's work as a standup update
        ---
        Write a three-line standup.
        """
      When I run ":skills"
      Then the Skills modal lists "standup" under "global"
      And the Skills modal lists "standup" before "init-vault"

    Scenario: A workflow's Skills are listed in the order they run
      When I run ":skills"
      Then the Skills modal lists under "knowledge-vault", in order:
        | init-vault       |
        | update-knowledge |
        | search-knowledge |
        | sweep-knowledge  |
        | maintain-vault   |

    Scenario: A Skill with no readable frontmatter is listed dimmed with its reason
      Given the Skill folder "broken" holds:
        """
        Just prose, no frontmatter.
        """
      When I run ":skills"
      Then the Skills modal row "broken" is dimmed as "no-frontmatter"

    Scenario: Picking a dimmed Skill hands nothing over
      Given the Skill folder "broken" holds:
        """
        Just prose, no frontmatter.
        """
      And an AI session is running in the AI pane
      And I run ":skills"
      When I pick the Skill "broken"
      Then the AI received nothing

    Scenario: The Skills Chip on the AI pane's border opens the modal
      Given no AI session is running in the AI pane
      When I click the Skills action on the AI pane's border
      Then the modal is "skills"
      And no new AI session was started

    Scenario: The palette opens the modal
      When I tap the palette entry "Skills"
      Then the modal is "skills"

    Scenario: With the knowledge base off its Skills are not listed
      Given the knowledge base is "disabled"
      And the Skill folder "standup" holds:
        """
        ---
        name: standup
        description: Summarise today's work as a standup update
        ---
        """
      When I run ":skills"
      Then the Skills modal does not list "update-knowledge"
      And the Skills modal does not list "search-knowledge"
      And the Skills modal lists "standup"

  Rule: Picking a Skill pastes one line pointing at it

    Varde pastes a pointer, never the Skill's text, and submits it. A session is started if none is
    running, and the line waits until it is ready, the way every prompt Varde queues does.

    Background:
      Given the knowledge base is enabled with the default Vault

    Scenario: A picked Skill reaches the running session as a pointer
      Given an AI session is running in the AI pane
      And I run ":skills"
      When I pick the Skill "update-knowledge"
      Then the AI was told to follow "/home/me/.varde/ai/skills/workflows/knowledge-vault/update-knowledge/SKILL.md"
      And the AI was told the workspace is "/home/me/projects/varde"
      And the prompt was submitted
      And no modal is open

    Scenario: A Skill that opts into the Vault is told where it is
      Given an AI session is running in the AI pane
      And I run ":skills"
      When I pick the Skill "update-knowledge"
      Then the AI was told the Vault is "/home/me/.varde/knowledge"
      And the AI was told the Source map is "/home/me/.varde/source-map.md"

    Scenario: A Skill that did not opt in is never told about the Vault
      Given the Skill folder "standup" holds:
        """
        ---
        name: standup
        description: Summarise today's work as a standup update
        ---
        """
      And an AI session is running in the AI pane
      And I run ":skills"
      When I pick the Skill "standup"
      Then the AI was not told about the Vault
      And the AI was not told about the Source map

    Scenario: With no session running, picking a Skill starts one and waits for it
      Given no AI session is running in the AI pane
      And I run ":skills"
      And I pick the Skill "update-knowledge"
      When the AI session is ready for input
      Then an AI session was started with "claude"
      And the AI was told to follow "/home/me/.varde/ai/skills/workflows/knowledge-vault/update-knowledge/SKILL.md"

    Scenario: Nothing reaches a session that has not spoken yet
      Given no AI session is running in the AI pane
      And I run ":skills"
      When I pick the Skill "update-knowledge"
      Then the AI received nothing

    Scenario: Picking a Skill shows the AI pane
      Given an AI session is running in the AI pane
      And the AI pane is hidden
      And I run ":skills"
      When I pick the Skill "update-knowledge"
      Then the AI pane is shown

    Scenario: No session is told about the Vault without a Skill
      Given no AI session is running in the AI pane
      When I start the AI
      Then the AI session's environment does not name the Vault
      And the AI received nothing

  Rule: A Skill may ask a question before it is handed over

    `search-knowledge` asks what to look for. Its `metadata.varde-asks` names the box, and the
    answer goes to the session with the pointer.

    Background:
      Given the knowledge base is enabled with the default Vault
      And an AI session is running in the AI pane

    Scenario: A Skill that asks opens a question box instead of pasting
      Given I run ":skills"
      When I pick the Skill "search-knowledge"
      Then the modal is "skill-question"
      And the AI received nothing

    Scenario: The question goes to the session with the pointer
      Given I run ":skills"
      And I pick the Skill "search-knowledge"
      And I type "how do customers renew a licence?"
      When I press Enter
      Then the AI was told to follow "/home/me/.varde/ai/skills/workflows/knowledge-vault/search-knowledge/SKILL.md"
      And the AI was told the question "how do customers renew a licence?"
      And the prompt was submitted

    Scenario: An empty question hands nothing over
      Given I run ":skills"
      And I pick the Skill "search-knowledge"
      When I press Enter
      Then the AI received nothing
      And no modal is open

    Scenario: Escape leaves the question unasked
      Given I run ":skills"
      And I pick the Skill "search-knowledge"
      And I type "renewals"
      When I press Escape
      Then the AI received nothing
      And no modal is open

  Rule: Varde keeps its shipped Skills current

    The binary carries every file of every workflow it ships. When any of them differs on disk,
    Varde deletes `skills/workflows/` and the folders older releases shipped, and writes the
    workflows afresh. A Global Skill is never touched (ADR 0026, ADR 0028).

    Scenario: Shipped Skills and the agent are written when missing
      Given "/home/me/.varde/ai" does not exist
      When Varde starts in the project
      Then "/home/me/.varde/ai/skills/workflows/knowledge-vault/update-knowledge/SKILL.md" holds the shipped text
      And "/home/me/.varde/ai/skills/workflows/knowledge-vault/search-knowledge/SKILL.md" holds the shipped text
      And "/home/me/.varde/ai/skills/workflows/knowledge-vault/agents/knowledge-searcher.md" holds the shipped text

    Scenario: An edited shipped Skill is replaced on start
      Given "/home/me/.varde/ai/skills/workflows/knowledge-vault/update-knowledge/SKILL.md" holds "my own edit"
      When Varde starts in the project
      Then "/home/me/.varde/ai/skills/workflows/knowledge-vault/update-knowledge/SKILL.md" holds the shipped text

    Scenario: A Skill the user wrote is left alone
      Given the Skill folder "standup" holds:
        """
        ---
        name: standup
        description: Mine
        ---
        """
      When Varde starts in the project
      Then the Skill folder "standup" is unchanged

    Scenario: An update removes what the workflows folder held before
      Given "/home/me/.varde/ai/skills/workflows/knowledge-vault/update-knowledge/SKILL.md" holds "an older release"
      And "/home/me/.varde/ai/skills/workflows/knowledge-vault/notes.md" holds "mine"
      And "/home/me/.varde/ai/agents/knowledge-searcher.md" holds "an older release"
      When Varde starts in the project
      Then the file "/home/me/.varde/ai/skills/workflows/knowledge-vault/notes.md" does not exist
      And the file "/home/me/.varde/ai/agents/knowledge-searcher.md" does not exist
      And "/home/me/.varde/ai/skills/workflows/knowledge-vault/update-knowledge/SKILL.md" holds the shipped text

    Scenario: A shipped Skill already current is not written
      Given every shipped Skill on disk holds its shipped text
      When Varde starts in the project
      Then no shipped Skill was written

  Rule: The Knowledge view points the tree, Filter, Find and editor at the Vault

    Everything else stays with the workspace: the terminal, the AI pane, git and language servers.
    Toggling back returns to the View you came from, as you left it.

    Background:
      Given the knowledge base is enabled with the default Vault
      And the Vault holds:
        | Varde/how-to/Install.md      |
        | Varde/documentation/Panes.md |
        | Acme/customers/Acme.md       |

    Scenario: The tree shows the Vault
      When I toggle the Knowledge view
      Then the view is "knowledge"
      And the tree's root is "/home/me/.varde/knowledge"
      And the tree shows "Varde"
      And the tree shows "Acme"

    Scenario: The palette toggles it
      When I tap the palette entry "Knowledge"
      Then the view is "knowledge"

    Scenario: Toggling again returns to the workspace as it was
      Given "src/tree.rs" is open in the editor
      And I press "jj" in the editor
      And I toggle the Knowledge view
      When I toggle the Knowledge view
      Then the view is "edit"
      And the tree's root is "/home/me/projects/varde"
      And "src/tree.rs" is open in the editor
      And the cursor is on line 3

    Scenario: Leaving returns to Review when that is where you came from
      Given the view is "review"
      And I toggle the Knowledge view
      When I toggle the Knowledge view
      Then the view is "review"

    Scenario: A Note opens in the Preview
      Given I toggle the Knowledge view
      When I open "Varde/how-to/Install.md" from the tree
      Then the editor shows "Varde/how-to/Install.md" in the Preview

    Scenario: Find searches the Vault, not the workspace
      Given I toggle the Knowledge view
      When I search the project for "renew"
      Then the search ran in "/home/me/.varde/knowledge"

    Scenario: Filter narrows the Vault
      Given I toggle the Knowledge view
      When I filter the tree for "acme"
      Then the tree shows "Acme/customers/Acme.md"
      And the tree does not show "Varde/how-to/Install.md"

    Scenario: The terminal and the AI stay with the workspace
      Given an AI session is running in the AI pane
      When I toggle the Knowledge view
      Then no command has been executed
      And no new AI session was started

    Scenario: A Note written while the view is shown appears in the tree
      Given I toggle the Knowledge view
      When the file "Acme/how-to/Renewals.md" appears in the Vault
      Then the tree shows "Acme/how-to/Renewals.md"

    Scenario: No Change bars are drawn on a Note
      Given the Vault is a git repository with uncommitted changes to "Varde/how-to/Install.md"
      And I toggle the Knowledge view
      When I open "Varde/how-to/Install.md" from the tree
      Then no Change bar is drawn
      And no git command was run in the Vault

    Scenario: The Knowledge view is not remembered across a restart
      Given I toggle the Knowledge view
      When Varde starts in the project
      Then the view is "edit"

    Scenario: An empty Vault shows an empty tree and points to Skills
      Given the Vault is empty
      When I toggle the Knowledge view
      Then the tree shows nothing
      And the reviewer is told "empty-vault"

    Scenario: With the knowledge base off the palette entry is dimmed and does nothing
      Given the knowledge base is "disabled"
      When I tap the palette entry "Knowledge"
      Then the view is "edit"
      And the palette entry "Knowledge" is dimmed
      And the reviewer is told "knowledge-disabled"

  Rule: In the Knowledge view Notes are read, never edited

    Only a Skill writes the Vault (ADR 0025).

    Background:
      Given the knowledge base is enabled with the default Vault
      And the Vault holds:
        | Varde/how-to/Install.md |
      And I toggle the Knowledge view
      And I open "Varde/how-to/Install.md" from the tree

    Scenario: Insert mode is refused
      When I press "i" in the editor
      Then the editor is not in insert mode
      And the reviewer is told "vault-is-read-only"

    Scenario: An editing key changes nothing
      When I press "dd" in the editor
      Then the buffer is unchanged
      And the reviewer is told "vault-is-read-only"

    Scenario: Replace is refused
      When I open the Replace box
      Then no modal is open
      And the reviewer is told "vault-is-read-only"

    Scenario: The tree offers no file actions
      When I select "Varde/how-to/Install.md" in the tree
      Then the tree row offers no actions

    Scenario: Saving is refused
      When I run ":w"
      Then nothing was written to disk
      And the reviewer is told "vault-is-read-only"

    Scenario: A selected passage can still be injected into the AI
      Given an AI session is running in the AI pane
      And I drag across "Install" in the editor pane
      When I click the AI Inject action on the editor pane's border
      Then the AI received the bytes "Install"

    Scenario: The Skills Chip stays on the AI pane
      When I click the Skills action on the AI pane's border
      Then the modal is "skills"
