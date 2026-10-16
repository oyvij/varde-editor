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
