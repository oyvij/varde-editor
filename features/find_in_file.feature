Feature: Finding inside the buffer

  `/` searches the file being edited. The query is typed on the line the editor
  already uses for `:` commands rather than in the floating modal, which is what
  keeps finding *here* visibly different from finding *everywhere*.

  The cursor moves to the closest match as each character arrives, so you can
  stop typing the moment you have arrived. Escape puts you back where the search
  started; Enter leaves the found text as the selection, so it can be copied or
  handed to project search without retyping it.

  The search stays on until it is ended: Enter hands the keyboard to the buffer
  and leaves the line, its count and its highlights where they are, and `/`
  goes back into the query. Escape in the buffer ends it where the cursor is.

  Matching is the project searcher's, handed a single file rather than the whole
  workspace, so a lowercase query ignores case and a capital makes it exact —
  one rule, not two — until `[Aa]` is pressed, which then stays where it was put
  for the rest of this search. `[word]` counts a match only where it is a whole
  word, so `state` stops finding `states` and `restate`; a new search starts
  with it off.

  `[replace]` and `[replace all]` open the replace box. Replacing edits the
  buffer and nothing else: it is written when it is saved, like any other edit.

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
    And the in-file search is open

  Scenario: Enter hands the keyboard to the buffer and the search stays on
    Given I press "/" in the editor
    And I type "update" into the in-file search
    And I press the key "Enter"
    When I press the key "j"
    Then the cursor is at line 2 column 4
    And the keyboard is not in the in-file search
    And the in-file search query is "update"

  Scenario: Slash goes back into the query with its text intact
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Enter"
    When I press the key "/"
    Then the keyboard is in the in-file search query
    And the in-file search query is "state"

  Scenario: Another pane takes the keyboard and the search stays on
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I click in the terminal pane
    When I press the key "x"
    Then the terminal received "x"
    And the in-file search query is "state"
    And the highlighted matches are:
      | 1 | 11 |
      | 3 | 10 |

  Scenario: Coming back to the editor lands in the buffer, not the query
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I click in the terminal pane
    When I click in the editor pane
    Then the keyboard is not in the in-file search

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

  Scenario: Escape in the buffer ends the search where the cursor is
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press Enter during the in-file search
    When I press "Escape" in the editor
    Then nothing is highlighted
    And the in-file search is not open
    And the cursor is at line 1 column 11

  Scenario: Right at the end of the query reaches the case icon
    Given I press "/" in the editor
    And I type "state" into the in-file search
    When I press the key "Right"
    Then the keyboard is on the in-file search's "case" icon

  Scenario: Right and Left walk the icons
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Tab"
    And I press the key "Right"
    And I press the key "Right"
    When I press the key "Left"
    Then the keyboard is on the in-file search's "word" icon

  Scenario: Left from the case icon returns to the end of the query
    Given I press "/" in the editor
    And I type "stat" into the in-file search
    And I press the key "Left"
    And I press the key "Left"
    And I press the key "Tab"
    And I press the key "Left"
    When I type "e" into the in-file search
    Then the in-file search query is "state"

  Scenario: A lowercase query leaves the case toggle dim and finds a capital
    Given I press "/" in the editor
    When I type "update" into the in-file search
    Then the case toggle is dim
    And the highlighted matches are:
      | 1 | 4 |
      | 2 | 5 |

  Scenario: A capital lights the case toggle and no longer finds the lowercase
    Given I press "/" in the editor
    When I type "Update" into the in-file search
    Then the case toggle is lit
    And the highlighted matches are:
      | 2 | 5 |

  Scenario: Pressing the case toggle on a capital query ignores case
    Given I press "/" in the editor
    And I type "Update" into the in-file search
    When I click the "case" icon on the in-file search line
    Then the case toggle is dim
    And the highlighted matches are:
      | 1 | 4 |
      | 2 | 5 |

  Scenario: The case toggle stays where it was put while the query is edited
    Given I press "/" in the editor
    And I type "up" into the in-file search
    And I click the "case" icon on the in-file search line
    When I type "date" into the in-file search
    Then the case toggle is lit
    And the highlighted matches are:
      | 1 | 4 |

  Scenario: A new search after Escape starts on smart case
    Given I press "/" in the editor
    And I type "up" into the in-file search
    And I click the "case" icon on the in-file search line
    And I press the key "Escape"
    And I press "/" in the editor
    When I type "update" into the in-file search
    Then the case toggle is dim
    And the highlighted matches are:
      | 1 | 4 |
      | 2 | 5 |

  Scenario: Whole word skips a match inside a longer word
    Given "src/words.rs" is open in the editor holding:
      """
      states = restate(state)
      state_x = state.x
      """
    And I press "/" in the editor
    And I type "state" into the in-file search
    When I click the "word" icon on the in-file search line
    Then the word toggle is lit
    And the highlighted matches are:
      | 1 | 18 |
      | 2 | 11 |

  Scenario: Whole word is toggled from the keyboard
    Given "src/words.rs" is open in the editor holding:
      """
      states = restate(state)
      """
    And I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Tab"
    And I press the key "Right"
    When I press the key "Enter"
    Then the word toggle is lit
    And the highlighted matches are:
      | 1 | 18 |

  Scenario: Pressing whole word again finds the match inside a longer word
    Given "src/words.rs" is open in the editor holding:
      """
      states = restate(state)
      """
    And I press "/" in the editor
    And I type "state" into the in-file search
    And I click the "word" icon on the in-file search line
    When I click the "word" icon on the in-file search line
    Then the word toggle is dim
    And the highlighted matches are:
      | 1 | 1  |
      | 1 | 12 |
      | 1 | 18 |

  Scenario: A new search after Escape starts with whole word off
    Given "src/words.rs" is open in the editor holding:
      """
      states = restate(state)
      """
    And I press "/" in the editor
    And I type "state" into the in-file search
    And I click the "word" icon on the in-file search line
    And I press the key "Escape"
    And I press "/" in the editor
    When I type "state" into the in-file search
    Then the word toggle is dim
    And the highlighted matches are:
      | 1 | 1  |
      | 1 | 12 |
      | 1 | 18 |

  Scenario: Replace takes the match at the cursor and lands on the next
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Enter"
    And I click the "replace" icon on the in-file search line
    And I type "status" into the replace box
    When I press the key "Enter"
    Then the buffer holds:
      """
      fn update(status)
      let Update = 1
      fn other(state)
      """
    And the cursor is at line 3 column 10
    And the replace box is open

  Scenario: Replace all replaces every match and writes nothing
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Enter"
    And I click the "replace all" icon on the in-file search line
    And I type "status" into the replace box
    And I press the key "Tab"
    And I press the key "Tab"
    When I press the key "Enter"
    Then the buffer holds:
      """
      fn update(status)
      let Update = 1
      fn other(status)
      """
    And the replace box is not open
    And the in-file search is open
    And no file was written

  Scenario: One undo puts back every match a replace all took
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Enter"
    And I click the "replace all" icon on the in-file search line
    And I type "status" into the replace box
    And I press the key "Tab"
    And I press the key "Tab"
    And I press the key "Enter"
    When I press "u" in the editor
    Then the buffer holds:
      """
      fn update(state)
      let Update = 1
      fn other(state)
      """

  Scenario: Replace all honours the case toggle
    Given I press "/" in the editor
    And I type "Update" into the in-file search
    And I press the key "Enter"
    And I click the "replace all" icon on the in-file search line
    And I type "Upgrade" into the replace box
    And I press the key "Tab"
    And I press the key "Tab"
    When I press the key "Enter"
    Then the buffer holds:
      """
      fn update(state)
      let Upgrade = 1
      fn other(state)
      """

  Scenario: Replace all honours whole word
    Given "src/words.rs" is open in the editor holding:
      """
      states = restate(state)
      state_x = state.x
      """
    And I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Enter"
    And I click the "word" icon on the in-file search line
    And I click the "replace all" icon on the in-file search line
    And I type "status" into the replace box
    And I press the key "Tab"
    And I press the key "Tab"
    When I press the key "Enter"
    Then the buffer holds:
      """
      states = restate(status)
      state_x = status.x
      """

  Scenario: The replace box has its own whole word toggle
    Given "src/words.rs" is open in the editor holding:
      """
      states = restate(state)
      """
    And I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Enter"
    And I click the "replace" icon on the in-file search line
    When I click the word toggle in the replace box
    Then the word toggle is lit
    And the highlighted matches are:
      | 1 | 18 |

  Scenario: Escape closes the replace box and the search stays on
    Given I press "/" in the editor
    And I type "state" into the in-file search
    And I press the key "Enter"
    And I click the "replace" icon on the in-file search line
    When I press the key "Escape"
    Then the replace box is not open
    And the in-file search query is "state"

  Scenario: Ctrl+F searches the project for what the editor has selected
    Given I press "/" in the editor
    And I type "other" into the in-file search
    And I press the key "Enter"
    When I press the key "Ctrl+f"
    Then the search is open
    And the search query is "other"

  Scenario: Ctrl+F with nothing selected opens an empty project search
    When I press the key "Ctrl+f"
    Then the search is open
    And the search query is ""
