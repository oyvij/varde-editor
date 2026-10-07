Feature: The Cheatsheet

  The Cheatsheet lists the keys of the view on screen. It takes the AI pane's
  place when asked for, and the AI session — or the box that starts one — goes
  on running behind it, untouched, until it is hidden again. It scrolls and
  takes focus like any pane, and nothing it is given reaches the session it is
  covering. `:help` and the palette show it and hide it.

  It starts hidden at every launch, whatever the project remembers, so whether
  it is up is not written to the project's state, and state an older Varde
  wrote with it is read without it.

  Text never goes into a session nobody can see: anything that starts the AI
  session or sends text into it hides the Cheatsheet first.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the current view is Edit

  Scenario: The Cheatsheet is hidden to begin with
    When Varde starts in the project
    Then the Cheatsheet is not shown

  Scenario: A Cheatsheet shown last session is still hidden at the next launch
    Given the project ".varde/state.json" records the Cheatsheet as shown
    When Varde starts in the project
    Then the Cheatsheet is not shown

  Scenario: The help command shows it
    Given the Cheatsheet is hidden
    When I ask Varde for help from the command line
    Then the Cheatsheet is shown

  Scenario: The help command hides it
    Given the Cheatsheet is shown
    When I ask Varde for help from the command line
    Then the Cheatsheet is not shown

  Scenario: Whether it is up is not remembered
    Given the Cheatsheet is shown
    When I dim the editor from the command line
    Then the saved project state does not mention the Cheatsheet

  Scenario: The palette shows it in the AI pane's place, and focus stays where it was
    Given the Cheatsheet is hidden
    And the editor pane has focus
    And the view palette is shown
    When I press "h"
    Then the Cheatsheet is shown
    And the view palette is not shown
    And the editor pane still has focus

  Scenario: Showing and hiding it leaves the AI session running and untouched
    Given an AI session is running in the AI pane
    And the Cheatsheet is hidden
    And I ask Varde for help from the command line
    When I ask Varde for help from the command line
    Then the Cheatsheet is not shown
    And nothing was asked of the AI session

  Scenario: Showing it over a focused AI pane hands the slot's focus to the Cheatsheet
    Given an AI session is running in the AI pane
    And the AI pane has focus
    When I ask Varde for help from the command line
    Then the Cheatsheet pane has focus

  Scenario: Hiding it while it has focus leaves focus on the AI pane
    Given an AI session is running in the AI pane
    And the Cheatsheet is shown
    And the Cheatsheet pane has focus
    When I ask Varde for help from the command line
    Then the AI pane has focus

  Scenario: A key scrolls the focused Cheatsheet and never reaches the session behind it
    Given the screen is 26 rows by 120 columns
    And an AI session is running in the AI pane
    And the Cheatsheet is shown
    And the Cheatsheet pane has focus
    When I press the key "j"
    Then the Cheatsheet is scrolled 1 rows down
    And no keys were sent to the AI pane

  Scenario: Down scrolls it the way j does
    Given the screen is 26 rows by 120 columns
    And the Cheatsheet is shown
    And the Cheatsheet pane has focus
    When I press the key "Down"
    Then the Cheatsheet is scrolled 1 rows down

  Scenario Outline: The arrows and the page keys scroll it too
    Given the screen is 26 rows by 120 columns
    And the Cheatsheet is shown
    And the Cheatsheet pane has focus
    And I press the key "PageDown"
    When I press the key "<key>"
    Then the Cheatsheet is scrolled <rows> rows down

    Examples:
      | key    | rows |
      | Down   | 17   |
      | Up     | 15   |
      | k      | 15   |
      | PageUp | 0    |

  Scenario: The wheel over the Cheatsheet scrolls it and is never reported to the session
    Given the screen is 26 rows by 120 columns
    And an AI session is running in the AI pane
    And the AI program asked for "SGR" mouse reporting
    And the Cheatsheet is shown
    And the editor pane has focus
    When I turn the wheel down over the AI pane's rectangle
    Then the Cheatsheet is scrolled 1 rows down
    And the AI pane did not scroll
    And no keys were sent to the AI pane

  Scenario: A Cheatsheet longer than the pane scrolls to its last row and no further
    Given the screen is 26 rows by 120 columns
    And the Cheatsheet is shown
    When I scroll down 100 times with the pointer over the Cheatsheet pane
    Then the Cheatsheet's last row is on screen

  Scenario: Scrolling back up stops at its first row
    Given the screen is 26 rows by 120 columns
    And the Cheatsheet is shown
    And I scroll down 100 times with the pointer over the Cheatsheet pane
    When I scroll up 100 times with the pointer over the Cheatsheet pane
    Then the Cheatsheet is scrolled 0 rows down

  Scenario: Starting the AI hides the Cheatsheet
    Given no AI session is running in the AI pane
    And the Cheatsheet is shown
    And the Cheatsheet pane has focus
    When I start the AI
    Then the Cheatsheet is not shown
    And the AI pane has focus

  Scenario: A review handed to the AI hides the Cheatsheet
    Given the project is a git repository
    And "src/tree.js" is changed at revision "a3f9c1"
    And an AI session is running in the AI pane
    And I add an ISSUE on "src/tree.js" lines 1 to 2 saying "unquoted path"
    And the Cheatsheet is shown
    And I submit the review
    When I confirm the submission
    Then the Cheatsheet is not shown
    And the prompt was submitted to the AI

  Scenario: It shows while the editor is inserting
    Given "src/main.rs" is open in the editor
    And the editor pane has focus
    And the editor mode is insert
    When I ask Varde for help from the command line
    Then the Cheatsheet lists the keys of the view on screen
