Feature: Asking for a story set with an Inquiry

  A Range is not the only way into a Story set. An Inquiry is free text the developer typed, plus one
  Tag that scopes it — Bug, Feature, Architecture or Feeling lucky. `:story*` opens the Inquiry box,
  a multiline box over the editor's own keys, where Enter starts a new line. Escape backs out and
  costs nothing: nothing is written, nothing reaches the AI pane, and no AI session is started
  (`docs/adr/0024-an-inquiry-is-a-story-without-a-range.md`).

  It is spelled `:story*` rather than with a word after `:story `, because everything after
  `:story ` is a Range spelling, and a branch named like a word must not behave differently from any
  other branch.

  Background:
    Given the workspace root is "/home/me/projects/varde"

  Scenario: The Inquiry box opens from the Edit view
    Given the current view is Edit
    When I run ":story*"
    Then the modal is "inquiry"
    And no keys were sent to the AI pane

  Scenario: The Inquiry box opens from the Story view
    Given I open Story view
    When I run ":story*"
    Then the modal is "inquiry"

  Scenario: Enter starts a new line rather than submitting
    Given I run ":story*"
    And I type "how does a key" in the inquiry box
    When I press Enter in the inquiry box
    Then the inquiry reads "how does a key\n"
    And the modal is "inquiry"
    And no keys were sent to the AI pane

  Scenario: The editor's word delete edits the Inquiry
    Given I run ":story*"
    And I type "where does it go" in the inquiry box
    When I press Alt+Backspace in the inquiry box
    Then the inquiry reads "where does it "

  Scenario: The editor's undo takes an edit back
    Given I run ":story*"
    And I type "where does it go" in the inquiry box
    And I press Backspace in the inquiry box
    When I press Ctrl+z in the inquiry box
    Then the inquiry reads "where does it go"

  Scenario: The box opens with one Tag picked
    When I run ":story*"
    Then the inquiry tag is "feeling-lucky"

  Scenario: Tab picks the next Tag in place of the last
    Given I run ":story*"
    And I press Tab in the inquiry box
    When I press Tab in the inquiry box
    Then the inquiry tag is "feature"

  Scenario: Escape closes the box and nothing is authored
    Given an AI session is running
    And I run ":story*"
    And I type "why is this slow" in the inquiry box
    When I press Escape in the inquiry box
    Then the modal is "none"
    And no keys were sent to the AI pane
    And no AI session was started
    And no file was written
    And the story view state is "no-stories"

  Scenario: Escape with no AI session running starts none
    Given no AI session is running
    And I run ":story*"
    When I press Escape in the inquiry box
    Then the modal is "none"
    And no AI session was started

  Scenario: The command list offers the Inquiry in the Edit view
    Given the current view is Edit
    When I open the command line and type "story"
    Then the command list offers "story*"

  Scenario: The command list offers the Inquiry in the Story view
    Given I open Story view
    When I open the command line and type "story"
    Then the command list offers "story*"

  Rule: Submitting hands the Inquiry to the AI session

    Ctrl+S, or the Chip, sends the Inquiry text and its Tag into the hosted AI session together with
    the Story schema, and names the `inquiry-<unix-ms>` artifact the AI must write. There is no
    confirmation, because the box is the confirmation, and there is no companion context file,
    because an Inquiry has no change to hand over. Nothing asks git a question, so neither a dirty
    working tree nor a folder that is not a repository is refused.

    Scenario: Submitting sends the Inquiry, its Tag and the artifact to the running AI session
      Given an AI session is running
      And I run ":story*"
      And I type "how does a key reach the child" in the inquiry box
      And I press Tab in the inquiry box
      When I press Ctrl+s in the inquiry box
      Then the AI pane was sent a prompt containing "how does a key reach the child"
      And the AI pane was sent a prompt for the tag "bug"
      And the AI pane was sent a prompt naming an Inquiry artifact
      And the modal is "none"
      And the story view state is "authoring"
      And the spine is empty
      And no file was written

    Scenario: Clicking the Ask Chip submits the Inquiry
      Given an AI session is running
      And I run ":story*"
      And I type "how does a key reach the child" in the inquiry box
      When I click the Inquiry box's Ask Chip
      Then the AI pane was sent a prompt containing "how does a key reach the child"
      And the story view state is "authoring"

    Scenario: Two Inquiries in one session name two artifacts
      Given an AI session is running
      And I submitted the Inquiry "where does a key go"
      When I submit the Inquiry "where does a key go"
      Then the two prompts name different Inquiry artifacts

    Scenario: Submitting with no AI session starts the configured one and holds the Inquiry
      Given no AI session is running
      And I run ":story*"
      And I type "why is this slow" in the inquiry box
      When I press Ctrl+s in the inquiry box
      Then the AI session "claude" was started
      And no keys were sent to the AI pane
      And the story view state is "authoring"

    Scenario: The held Inquiry is sent once the AI session is ready
      Given no AI session is running
      And I submitted the Inquiry "why is this slow"
      When the AI session is ready for input
      Then the AI pane was sent a prompt containing "why is this slow"

    Scenario: The AI session exiting before the artifact arrives abandons the set
      Given an AI session is running
      And I submitted the Inquiry "why is this slow"
      When the AI session exits
      Then the story view state is "authoring-abandoned"
      And the spine is empty

    Scenario: Escape during authoring leaves nothing authored
      Given the project has a ".gitignore"
      And an AI session is running
      And I submitted the Inquiry "why is this slow"
      When I press "Escape"
      Then the story view state is "no-stories"
      And the project ".gitignore" is unchanged
      And no file was written

    Scenario: Escape while the Inquiry waits for the AI session leaves nothing to send
      Given no AI session is running
      And I submitted the Inquiry "why is this slow"
      And I press "Escape"
      When the AI session is ready for input
      Then no keys were sent to the AI pane
      And the story view state is "no-stories"

    Scenario: A dirty working tree is neither refused nor confirmed
      Given the project is a git repository
      And the working tree contains:
        | path        | git status |
        | src/keys.rs | modified   |
      And an AI session is running
      When I submit the Inquiry "what am I in the middle of"
      Then the modal is "none"
      And the story view state is "authoring"
      And the AI pane was sent a prompt naming an Inquiry artifact

    Scenario: A folder that is not a git repository is not refused
      Given the project is not a git repository
      And an AI session is running
      When I submit the Inquiry "what is this"
      Then the story view state is "authoring"
      And the AI pane was sent a prompt naming an Inquiry artifact

    Scenario: In a Bare workspace the Inquiry set is written to the Sidecar
      Given the workspace is a Bare workspace
      And an AI session is running
      When I submit the Inquiry "what is this"
      Then the authoring prompt names the story set in the Sidecar

  Rule: Inquiry sets share the one pool of ten with Story sets

    An Inquiry set is kept and pruned like any other, and the oldest set of either kind falls out
    when the eleventh arrives. One pool was chosen over two: authoring Inquiry sets can evict the
    Story set for a review in flight, and that is the cheaper one to get back, since Varde resolves a
    Range for the developer while an Inquiry is only re-askable by whoever remembers it. An Inquiry
    has no key to look it up by, so it always authors fresh, and there is no `:story*!`.

    Scenario: An Inquiry set arriving at a full pool prunes the oldest set and its companion file
      Given the project holds 10 story sets
      When an Inquiry set is written
      Then 10 story sets remain
      And the oldest story set was pruned
      And the oldest story set's companion file was pruned
      And the Inquiry set is kept

    Scenario: An Inquiry set falls out of the pool by the same recency as a Story set
      Given an Inquiry set is written
      And the project holds 9 story sets
      When a new story set is written
      Then 10 story sets remain
      And the Inquiry set was pruned ahead of the oldest story set

    Scenario: Asking the same Inquiry again authors again rather than loading the earlier set
      Given an AI session is running
      And I submitted the Inquiry "where does a key go"
      And the Inquiry set that prompt named is written
      When I submit the Inquiry "where does a key go"
      Then the story view state is "authoring"
      And the two prompts name different Inquiry artifacts

    Scenario: There is no bang form of the Inquiry
      Given an AI session is running
      When I run ":story*!"
      Then the modal is "none"
      And no keys were sent to the AI pane
      And the story view state is "no-stories"

  Rule: An Inquiry set loads and walks as a Story set for a Range does

    At `protocolVersion` 3 a Story set may have no `range`, and such a set answers an Inquiry. Under
    version 2 a missing `range` is still a malformed file, refused whole, so a version 2 set that
    lost its range is never walked as an Inquiry. Everything walked needs no Range, so the Spine, the
    Step-menu, the Site mark and Stale steps come as they are for a Range; this does not re-prove
    them, it proves only that an Inquiry set reaches them.

    Scenario: A version 3 set with no range lists its stories in the spine
      Given "src/keys.rs" holds 20 numbered lines
      And an AI session is running
      And I submitted the Inquiry "how does a key reach the child"
      When the Inquiry artifact arrives:
        """
        {
          "protocolVersion": 3,
          "title": "How a key reaches the child",
          "stories": [
            { "id": "s1", "name": "The key is read", "premise": "…",
              "steps": [
                { "id": "s1e1", "name": "Read", "claim": "…", "why": "…",
                  "site": { "file": "src/keys.rs", "side": "new", "kind": "context",
                            "from": 1, "to": 2 } }
              ] },
            { "id": "s2", "name": "The key is routed", "premise": "…",
              "steps": [
                { "id": "s2e1", "name": "Route", "claim": "…", "why": "…",
                  "site": { "file": "src/keys.rs", "side": "new", "kind": "context",
                            "from": 5, "to": 6 } },
                { "id": "s2e2", "name": "Send", "claim": "…", "why": "…",
                  "site": { "file": "src/keys.rs", "side": "new", "kind": "context",
                            "from": 9, "to": 9 } }
              ] }
          ]
        }
        """
      Then the story view state is "spine"
      And the spine lists:
        | name              | steps |
        | The key is read   | 1     |
        | The key is routed | 2     |

    Scenario: A version 2 set with no range is still refused whole
      Given "src/keys.rs" holds 20 numbered lines
      And an AI session is running
      And I submitted the Inquiry "how does a key reach the child"
      When the Inquiry artifact arrives:
        """
        {
          "protocolVersion": 2,
          "title": "How a key reaches the child",
          "stories": [
            { "id": "s1", "name": "The key is read", "premise": "…",
              "steps": [
                { "id": "s1e1", "name": "Read", "claim": "…", "why": "…",
                  "site": { "file": "src/keys.rs", "side": "new", "kind": "context",
                            "from": 1, "to": 2 } }
              ] }
          ]
        }
        """
      Then the story view state is "story-artifact-invalid"
      And the spine is empty

    Scenario: Entering an Inquiry story puts its claim in the band and marks its site
      Given "src/keys.rs" holds 20 numbered lines
      And an AI session is running
      And I submitted the Inquiry "how does a key reach the child"
      And an Inquiry set claims:
        | story    | file        | from | to |
        | The keys | src/keys.rs | 4    | 6  |
      When I enter the story "The keys"
      Then the band claim is "claim"
      And the site mark covers lines 4 to 6 of "src/keys.rs"

    Scenario: An Inquiry step is filled in from the file itself when the set arrives
      Given "src/keys.rs" holds:
        """
        the first line
        the second line
        the third line
        """
      And an AI session is running
      And I submitted the Inquiry "what does the second line do"
      When an Inquiry set claims:
        | story    | file        | from | to |
        | The keys | src/keys.rs | 2    | 2  |
      Then the story view state is "spine"
      And the site text of step 1 is "the second line"

    Scenario: An Inquiry step whose code moved is stale and still says what its site held
      Given "src/keys.rs" holds:
        """
        the first line
        the second line
        the third line
        """
      And an AI session is running
      And I submitted the Inquiry "what does the second line do"
      And an Inquiry set claims:
        | story    | file        | from | to |
        | The keys | src/keys.rs | 2    | 2  |
      And I enter the story "The keys"
      When "src/keys.rs" now holds:
        """
        the first line
        something else entirely
        the third line
        """
      Then step 1 is stale as "text-changed"
      And the overlay shows what the site holds now
      And the site text of step 1 is "the second line"

    Scenario: An Inquiry set read off disk is not refilled against the code as it reads now
      Given "src/keys.rs" holds:
        """
        the first line
        the second line
        the third line
        """
      And an Inquiry set on disk claims lines 2 to 2 of "src/keys.rs"
      And I open Story view
      And "src/keys.rs" now holds:
        """
        the first line
        something else entirely
        the third line
        """
      And I leave Story view
      When I open Story view
      Then the story view state is "spine"
      And the site text of step 1 is "the second line"
