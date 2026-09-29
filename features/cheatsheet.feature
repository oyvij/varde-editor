Feature: Hiding the key reminder

  The box of keys in the editor's top-right corner sits over the code, and a terminal
  cell holds one character: there is no opacity to give it, so whatever it covers is
  gone while it is up. `:help` takes it down and puts it back.

  It starts hidden at every launch, whatever the project remembers: the box covers code
  the moment Varde opens, and someone who wants it can ask. So whether it is up is not
  written to the project's state, and state an older Varde wrote with it is read without
  it. The palette offers it too — the box is the only place the `:` commands
  are advertised, so a gesture that just the box tells you about would be unreachable
  once it is down.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the current view is Edit

  Scenario: The key reminder is down to begin with
    When Varde starts in the project
    Then the key reminder is not shown

  Scenario: A reminder shown last session is still down at the next launch
    Given the project ".varde/state.json" records the key reminder as shown
    When Varde starts in the project
    Then the key reminder is not shown

  Scenario: The help command puts the reminder up
    Given the key reminder is hidden
    When I ask Varde for help from the command line
    Then the key reminder is shown

  Scenario: The help command takes it down
    Given the key reminder is shown
    When I ask Varde for help from the command line
    Then the key reminder is not shown

  Scenario: Whether it is up is not remembered
    Given the key reminder is shown
    When I dim the editor from the command line
    Then the saved project state does not mention the key reminder

  Scenario: The palette puts a hidden reminder back
    Given the key reminder is hidden
    And the view palette is shown
    When I press "h"
    Then the key reminder is shown
    And the view palette is not shown
