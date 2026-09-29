Feature: Finding inside the buffer

  `/` searches the file being edited. The query is typed on the line the editor
  already uses for `:` commands rather than in the floating modal, which is what
  keeps finding *here* visibly different from finding *everywhere*.

  The cursor moves to the closest match as each character arrives, so you can
  stop typing the moment you have arrived. Escape puts you back where the search
  started; Enter leaves the found text as the selection, so it can be copied or
  handed to project search without retyping it.

  Matching is the project searcher's, handed a single file rather than the whole
  workspace, so a lowercase query ignores case and a capital makes it exact —
  one rule, not two.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/main.rs" is open in the editor holding:
      """
      fn update(state)
      let Update = 1
      fn other(state)
      """

  Scenario: Slash searches here, not everywhere
    When I press "/" in the editor
    Then the in-file search is open
    And the search is not open

  Scenario: The cursor moves to the closest match as the query is typed
    Given I press "/" in the editor
    When I type "other" into the in-file search
    Then the cursor is at line 3 column 4

  Scenario: A character typed after moving left goes into the middle of the query
    Given I press "/" in the editor
    And I type "oer" into the in-file search
    And I press the key "Left"
    And I press the key "Left"
    When I type "th" into the in-file search
    Then the in-file search query is "other"
    And the cursor is at line 3 column 4

  Scenario: Backspace in the middle of the query takes the character before the caret
    Given I press "/" in the editor
    And I type "othxer" into the in-file search
    And I press the key "Left"
    And I press the key "Left"
    When I press the key "Backspace"
    Then the in-file search query is "other"
    And the cursor is at line 3 column 4

  Scenario: The cursor arrives before the query is finished
    Given I press "/" in the editor
    When I type "ot" into the in-file search
    Then the cursor is at line 3 column 4

  Scenario: A match behind the cursor is still the closest one
    Given I press "G" in the editor
    And I press "/" in the editor
    When I type "update" into the in-file search
    Then the cursor is at line 1 column 4

  Scenario: Escape restores the position the search started from
    Given I press "l" in the editor
    And I press "/" in the editor
    And I type "other" into the in-file search
    When I press Escape during the in-file search
    Then the cursor is at line 1 column 2
    And the in-file search is not open
    And the selection holds nothing

  Scenario: Enter leaves the found text as the selection
    Given I press "/" in the editor
    And I type "other" into the in-file search
    When I press Enter during the in-file search
    Then the selection holds "other"
    And the in-file search is not open

  Scenario: A lowercase query ignores case
    Given I press "/" in the editor
    When I type "update" into the in-file search
    Then the cursor is at line 1 column 4

  Scenario: A capital makes the query exact
    Given I press "/" in the editor
    When I type "Update" into the in-file search
    Then the cursor is at line 2 column 5

  Scenario: A query with no match leaves the cursor where it was
    Given I press "/" in the editor
    When I type "zzzz" into the in-file search
    Then the cursor is at line 1 column 1

  Scenario: The back-a-word key still moves back a word
    Given I press "w" in the editor
    When I press "b" in the editor
    Then the cursor is at line 1 column 1

  Scenario: Typing a query does not open the project search
    Given I press "/" in the editor
    When I type "other" into the in-file search
    Then the search is not open

  Scenario: What was found can be searched for everywhere without retyping it
    Given the editor pane has focus
    And I press "/" in the editor
    And I type "other" into the in-file search
    And I press Enter during the in-file search
    When I press "gr" in the editor
    Then the search is open
    And the search query is "other"

  Scenario: What was found can be copied straight away
    Given a system clipboard is available
    And I press "/" in the editor
    And I type "other" into the in-file search
    And I press Enter during the in-file search
    When I copy the selection
    Then the clipboard holds "other"

  Scenario: n moves the cursor to the next match
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press Enter during the in-file search
    When I press "n" in the editor
    Then the cursor is at line 3 column 10

  Scenario: N moves the cursor to the previous match
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press Enter during the in-file search
    And I press "n" in the editor
    When I press "N" in the editor
    Then the cursor is at line 1 column 11

  Scenario: n at the last match wraps to the first
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press Enter during the in-file search
    And I press "n" in the editor
    When I press "n" in the editor
    Then the cursor is at line 1 column 11

  Scenario: N at the first match wraps to the last
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press Enter during the in-file search
    When I press "N" in the editor
    Then the cursor is at line 3 column 10

  Scenario: Stepping leaves the match it landed on as the selection
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press Enter during the in-file search
    When I press "n" in the editor
    Then the selection holds "state"

  Scenario: Every match is highlighted, not only the one the cursor is on
    Given I press "/" in the editor
    When I type "state" into the in-file search
    Then the highlighted matches are:
      | 1 | 11 |
      | 3 | 10 |

  Scenario: Stepping with nothing found leaves the cursor where it was
    Given I press "/" in the editor
    And I type "zzzz" into the in-file search
    And I press Enter during the in-file search
    When I press "n" in the editor
    Then the cursor is at line 1 column 1

  Scenario: Editing the buffer does not leave a stale highlight behind
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press Enter during the in-file search
    When I press "d" in the editor
    Then the highlighted matches are:
      | 3 | 10 |

  Scenario: Escape clears a finished search, highlights and all
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press Enter during the in-file search
    When I press "Escape" in the editor
    Then nothing is highlighted
