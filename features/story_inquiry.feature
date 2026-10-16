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
    When I type "how does a key" in the inquiry box
    And I press Enter in the inquiry box
    And I type "reach the child" in the inquiry box
    Then the inquiry reads "how does a key\nreach the child"
    And the modal is "inquiry"
    And no keys were sent to the AI pane

  Scenario: The editor's own keys edit the Inquiry
    Given I run ":story*"
    And I type "where does it go" in the inquiry box
    When I press Alt+Backspace in the inquiry box
    And I press Backspace in the inquiry box
    And I press Ctrl+z in the inquiry box
    Then the inquiry reads "where does it "

  Scenario: Exactly one Tag is picked, and Tab moves to the next
    Given I run ":story*"
    Then the inquiry tag is "feeling-lucky"
    When I press Tab in the inquiry box
    Then the inquiry tag is "bug"
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
