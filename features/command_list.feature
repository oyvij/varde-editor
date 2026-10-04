Feature: The command list under the command line

  The command line lists the Commands of the view on screen the moment it
  opens, so a Command nobody remembers is still reachable. It lists Commands
  only: keys and motions are the Cheatsheet's business.

  Typed text narrows the list against a Command's name and its description
  alike, because the word a reviewer reaches for is as often the description's
  as the name's. A Command is listed whether or not it would do anything in
  the state on screen — a list that changed shape with the state would teach a
  Command that is sometimes there and sometimes not.

  Picking an entry fills the line and leaves it open, and nothing runs until
  Enter is pressed with nothing picked. One Enter can never run a Command the
  reviewer was only reading.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the current view is Edit

  Scenario: Part of a Command's name finds it
    When I open the command line and type "map"
    Then the command list offers only "minimap"

  Scenario: A word from a Command's description finds it
    When I open the command line and type "darker"
    Then the command list offers only "dim"

  Scenario: A Command of Review view is listed there
    Given the project is a git repository
    And the working tree contains:
      | path        | git status |
      | src/tree.js | modified   |
    And I open Review view
    When I open the command line
    Then the command list offers "submit"

  Scenario: A Command of Review view is not an Edit view Command
    When I open the command line
    Then the command list does not offer "submit"

  Scenario: The list narrows over a diff
    Given the project is a git repository
    And the working tree contains:
      | path        | git status |
      | src/tree.js | modified   |
    And I open Review view
    When I open the command line and type "sub"
    Then the diff for "src/tree.js" is shown
    And the command list offers only "submit"

  Scenario: Picking an entry fills the line and runs nothing
    Given the Cheatsheet is hidden
    And I open the command line and type "lp"
    And I pick the entry below in the command list
    When I press Enter in the command line
    Then the command line holds "help"
    And the command line answered with no events
    And the Cheatsheet is not shown

  Scenario: A second Enter runs what the list filled in
    Given the Cheatsheet is hidden
    And I open the command line and type "lp"
    And I pick the entry below in the command list
    And I press Enter in the command line
    When I press Enter in the command line
    Then the command line is not open
    And the Cheatsheet is shown

  Scenario: A whole Command typed and run without touching the list
    Given the Cheatsheet is hidden
    And I open the command line and type "help"
    When I press Enter in the command line
    Then the command line is not open
    And the Cheatsheet is shown
