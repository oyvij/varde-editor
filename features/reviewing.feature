Feature: Reaching the review flow

  Review view is specified in detail but a reviewer has to be able to work it.
  The changed files list sits where the tree does, so the same keys apply. The
  editor pane shows a read-only unified diff of the selected file.

  Comments anchor to the new file's line numbers, because that is the code the
  AI has to change. A comment on a removed line points at where it was.

  Editing is not done inside a diff: e opens the file in Edit view instead.

  Choosing a file — a click, or Enter — hands the keyboard to the diff, because
  commenting is the reviewer's next act and only the editor's keys reach it.
  Moving the selection is browsing, so it keeps the list.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository
    And the working tree contains:
      | path        | git status |
      | src/tree.js | modified   |
    And I open Review view

  Scenario: Entering Review view lands on the first changed file
    Then the tree selection is "src/tree.js"
    And the diff for "src/tree.js" is shown
    And the file tree pane has focus

  Scenario: Entering Review view with nothing changed shows no diff
    Given the working tree has no changes
    And I open Review view
    Then no diff is shown

  Scenario: The changed file list takes the tree pane's keys
    Given the file tree pane has focus
    When I press "Down"
    Then the tree selection is "src/tree.js"

  Scenario: Moving through the changed files shows each diff
    Given the working tree contains:
      | path           | git status |
      | src/tree.js    | modified   |
      | src/landing.js | modified   |
    And I open Review view
    When I press "Down"
    Then the diff for "src/landing.js" is shown
    And the file tree pane has focus

  Scenario: Clicking a changed file hands the keyboard to its diff
    When I click the row "src/tree.js"
    Then the diff for "src/tree.js" is shown
    And no file was opened in the editor
    And the editor pane has focus

  Scenario: Enter opens the diff for the selected file
    Given the tree selection is "src/tree.js"
    When I press "Enter"
    Then the diff for "src/tree.js" is shown
    And the editor pane has focus

  Scenario: The diff is read-only
    Given the diff for "src/tree.js" is shown
    And the editor pane has focus
    When I press "x" in the editor
    Then "src/tree.js" has no unsaved edits

  Scenario: e opens the file for editing instead
    Given the diff for "src/tree.js" is shown
    And the editor pane has focus
    When I press "e" in the editor
    Then the current view is Edit
    And "/home/me/projects/varde/src/tree.js" is open in the editor

  Scenario: Leaving Review view puts the editor back
    Given the diff for "src/tree.js" is shown
    When I switch to Edit view
    Then no diff is shown

  Scenario: Selecting lines and pressing c opens the type picker
    Given the diff for "src/tree.js" is shown
    And the editor pane has focus
    When I press "Vj" in the editor
    And I press "c" in the editor
    Then the comment picker is shown

  Scenario: The picker records the type, body and line range
    Given the diff for "src/tree.js" is shown
    And the editor pane has focus
    And I press "Vj" in the editor
    And I press "c" in the editor
    When I choose the type ISSUE and enter "unquoted path"
    Then the comment picker is not shown
    And the reviewer is told the comment was added
    And the review holds a comment:
      | file      | src/tree.js   |
      | from_line | 1             |
      | to_line   | 2             |
      | type      | ISSUE         |
      | body      | unquoted path |

  Scenario: An added comment is shown against the line it covers
    Given the diff for "src/tree.js" is shown
    And I add an ISSUE on "src/tree.js" lines 1 to 2 saying "unquoted path"
    Then the diff shows the comment "unquoted path" against line 2
    And the diff shows no comment against line 1

  Scenario: Escaping the picker records nothing
    Given the diff for "src/tree.js" is shown
    And the editor pane has focus
    And I press "Vj" in the editor
    And I press "c" in the editor
    When I press "Escape" in the editor
    Then the comment picker is not shown
    And the review holds no comments

  Scenario: Submitting sends the review
    Given an AI session is running in the AI pane
    And I add an ISSUE on "src/tree.js" lines 1 to 2 saying "unquoted path"
    And I submit the review from the command line
    When I confirm the submission
    Then the AI pane was sent a prompt containing "unquoted path"
    And the prompt was submitted to the AI

  Rule: A read-only surface slides sideways on a gesture

    A diff is read, not typed in, so there is no column cursor for the view to
    follow the way a buffer's does — which left the tail of a long line
    unreachable without opening the file. The offset is moved by an explicit
    gesture instead: `l` and `h` slide it right and left, `0` brings it home, and
    sliding right stops at the longest row the surface draws rather than running
    on into empty space. Modifier-free, because a motion that needs a modifier is
    a motion that does not exist on a terminal nobody configured. A Preview and
    Story view's code surface answer the same three keys, for the same reason.

    Scenario: The gesture slides a diff to the right
      Given the screen is 12 rows by 80 columns
      And the diff for "src/tree.js" is shown holding a 60-character line
      And the editor pane has focus
      When I press "l" in the editor
      Then the editor view starts at column 9

    Scenario: The gesture slides a diff back to the left
      Given the screen is 12 rows by 80 columns
      And the diff for "src/tree.js" is shown holding a 60-character line
      And the editor pane has focus
      And I press "l" in the editor 2 times
      When I press "h" in the editor
      Then the editor view starts at column 9

    Scenario: Zero brings a slid diff home in one keypress
      Given the screen is 12 rows by 80 columns
      And the diff for "src/tree.js" is shown holding a 60-character line
      And the editor pane has focus
      And I press "l" in the editor 3 times
      When I press "0" in the editor
      Then the editor view starts at column 1

    Scenario: Sliding right stops at the longest row rather than at empty space
      Given the screen is 12 rows by 80 columns
      And the diff for "src/tree.js" is shown holding a 24-character line
      And the editor pane has focus
      When I press "l" in the editor 4 times
      Then the editor view starts at column 13

    Scenario: Moving through the diff never slides it on its own
      Given the screen is 12 rows by 80 columns
      And the diff for "src/tree.js" is shown holding a 60-character line
      And the editor pane has focus
      And I press "l" in the editor 2 times
      When I press "j" in the editor
      Then the editor view starts at column 17
