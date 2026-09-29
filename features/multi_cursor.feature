Feature: Editing every occurrence of a word at once

  Renaming a local is the edit a reader makes most often, and doing it with the
  find box means reading every hit to check it is the one meant. Taking the
  occurrences one press at a time keeps that judgement with the reader while
  the typing happens once: what is picked joins what was already picked, and
  the next keystroke lands at all of them.

  The next occurrence is the next one *after* the one picked first, and once
  the bottom of the file is reached it continues from the top — a run of
  presses ends up holding every occurrence in the file whichever one it
  started from, rather than stopping short of the ones above it.

  It answers in normal and insert mode alike. With nothing picked it takes the
  word under the cursor, which is what makes it reachable while inserting:
  there is no key to press first there that would not type a letter.

  Background:
    Given the workspace root is "/home/me/projects/varde"

  Scenario: The next occurrence of the picked word joins it
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    When I press "Ctrl+d" in the editor
    Then the other occurrences picked are:
      | line | column |
      | 2    | 7      |
    And the selection holds "one"

  Scenario: Reaching the bottom continues from the top
    Given "src/tree.js" is open in the editor holding:
      """
      one alpha
      one beta
      one gamma
      """
    And I press "jW" in the editor
    When I press "Ctrl+d" in the editor 2 times
    Then the other occurrences picked are:
      | line | column |
      | 3    | 1      |
      | 1    | 1      |

  Scenario: An occurrence below the visible rows is scrolled into view
    Given the screen is 26 rows by 100 columns
    And "src/tree.js" is open in the editor with 60 lines, "one" on lines 1 and 50
    And I press "W" in the editor
    When I press "Ctrl+d" in the editor
    Then the other occurrences picked are:
      | line | column |
      | 50   | 1      |
    And line 50 is on screen in the editor

  Scenario: An occurrence found by continuing from the top is scrolled into view
    Given the screen is 26 rows by 100 columns
    And "src/tree.js" is open in the editor with 60 lines, "one" on lines 1 and 60
    And I press "GW" in the editor
    When I press "Ctrl+d" in the editor
    Then the other occurrences picked are:
      | line | column |
      | 1    | 1      |
    And line 1 is on screen in the editor

  Scenario: Once every occurrence is taken there is nothing left to take
    Given "src/tree.js" is open in the editor holding:
      """
      one alpha
      one beta
      """
    When I press "Ctrl+d" in the editor 4 times
    Then the other occurrences picked are:
      | line | column |
      | 2    | 1      |

  Scenario: With nothing picked the word under the cursor is what is taken
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    When I press "Ctrl+d" in the editor
    Then the selection holds "one"
    And no other occurrence is picked

  Scenario: Typing replaces every occurrence picked
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    And I press "Ctrl+d" in the editor
    When I type "x" in the editor
    Then the buffer holds:
      """
      x two
      three x
      """
    And the cursor is at line 1 column 2
    And the selection holds nothing

  Scenario: Typing in normal mode starts inserting at every occurrence
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    And I press "Ctrl+d" in the editor
    When I type "x" in the editor
    Then the editor mode is insert

  Scenario: The keystrokes after the first go in at every occurrence too
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    And I press "Ctrl+d" in the editor
    When I type "xy" in the editor
    Then the buffer holds:
      """
      xy two
      three xy
      """

  Scenario: Two occurrences on one line both take what is typed
    Given "src/tree.js" is open in the editor holding:
      """
      one and one
      """
    And I press "W" in the editor
    And I press "Ctrl+d" in the editor
    When I type "ab" in the editor
    Then the buffer holds:
      """
      ab and ab
      """

  Scenario: Taking occurrences while inserting needs no mode key first
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "i" in the editor
    And I press "Ctrl+d" in the editor 2 times
    When I type "x" in the editor
    Then the buffer holds:
      """
      x two
      three x
      """
    And the editor mode is insert

  Scenario: Escape drops the occurrences with the selection
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    And I press "Ctrl+d" in the editor
    When I press "Escape" in the editor
    Then no other occurrence is picked
    And the selection holds nothing

  Scenario: Moving the cursor drops them the way a plain arrow drops a selection
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    And I press "Ctrl+d" in the editor
    When I press the Right arrow in the editor
    Then no other occurrence is picked

  Scenario: Clicking in the text drops them the way it drops a selection
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    And I press "Ctrl+d" in the editor
    When I click at line 2 column 1 in the editor
    Then no other occurrence is picked

  Scenario: A span with a line break in it names no word to take again
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I hold shift and press the Down arrow in the editor
    When I press "Ctrl+d" in the editor
    Then no other occurrence is picked

  Scenario: A Preview has no words to take, being read-only
    Given "README.md" is open in the editor holding:
      """
      one two three
      """
    When I press "Ctrl+d" in the editor
    Then the editor refuses with "read-only-preview"
    And the selection holds nothing

  Scenario: The modifier-free spelling takes the same occurrence
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    When I press "gm" in the editor
    Then the other occurrences picked are:
      | line | column |
      | 2    | 7      |

  Scenario: A run of undo takes the whole multiple edit back in one press
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three one
      """
    And I press "W" in the editor
    And I press "Ctrl+d" in the editor
    And I type "x" in the editor
    When I press "Escape" in the editor
    And I press "u" in the editor
    Then the buffer holds:
      """
      one two
      three one
      """
