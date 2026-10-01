Feature: More of vim

  The second tranche: selecting lines, repeating with counts, and moving text
  through a register. Registers are unnamed only — one yank buffer, as vim's
  default behaves.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/tree.js" is open in the editor holding:
      """
      one
      two
      three
      four
      """
    And the editor pane has focus

  Scenario Outline: Word motions move by word
    Given "src/words.js" is open in the editor holding:
      """
      one two three
      four five
      """
    When I press "<keys>" in the editor
    Then the cursor is at line <line> column <column>

    Examples:
      | keys | line | column |
      | w    | 1    | 5      |
      | ww   | 1    | 9      |
      | www  | 2    | 1      |
      | wwb  | 1    | 5      |
      | e    | 1    | 3      |
      | ee   | 1    | 7      |
      | $    | 1    | 13     |
      | $0   | 1    | 1      |

  Scenario: Word motions stop at the end of the buffer
    Given "src/words.js" is open in the editor holding:
      """
      one two
      """
    When I press "wwww" in the editor
    Then the cursor is at line 1 column 7

  Scenario: b at the very start stays put
    Given "src/words.js" is open in the editor holding:
      """
      one two
      """
    When I press "b" in the editor
    Then the cursor is at line 1 column 1

  Scenario: Alt with an arrow moves by word while inserting
    Given "src/words.js" is open in the editor holding:
      """
      one two three
      """
    And I press "i" in the editor
    When I hold alt and press the Right arrow in the editor
    Then the cursor is at line 1 column 5
    And the buffer holds:
      """
      one two three
      """

  Scenario: Alt with an arrow stops after the last word of a line
    Given "src/words.js" is open in the editor holding:
      """
      one two
      three
      """
    And I press "wi" in the editor
    When I hold alt and press the Right arrow in the editor
    Then the cursor is at line 1 column 8

  Scenario: From the end of a line Alt with an arrow goes on to the next
    Given "src/words.js" is open in the editor holding:
      """
      one two
      three
      """
    And I press "wi" in the editor
    And I hold alt and press the Right arrow in the editor
    When I hold alt and press the Right arrow in the editor
    Then the cursor is at line 2 column 1

  Scenario: In normal mode Alt with an arrow leaves the last word's end for the next line
    Given "src/words.js" is open in the editor holding:
      """
      one two
      three
      """
    And I press "w" in the editor
    And I hold alt and press the Right arrow in the editor
    When I hold alt and press the Right arrow in the editor
    Then the cursor is at line 2 column 1

  Scenario: Alt with the back arrow moves by word while inserting
    Given "src/words.js" is open in the editor holding:
      """
      one two three
      """
    And I press "$i" in the editor
    When I hold alt and press the Left arrow in the editor
    Then the cursor is at line 1 column 9
    And the buffer holds:
      """
      one two three
      """

  Scenario: V selects the current line
    When I press "V" in the editor
    Then the editor mode is visual
    And the selected lines are 1 to 1

  Scenario: Moving in visual mode extends the selection
    When I press "Vjj" in the editor
    Then the selected lines are 1 to 3

  Scenario: Escape leaves visual mode
    Given I press "V" in the editor
    When I press "Escape" in the editor
    Then the editor mode is normal

  Scenario: d deletes the selected lines
    When I press "Vjd" in the editor
    Then the buffer holds:
      """
      three
      four
      """

  Scenario: A count repeats a motion
    When I press "3j" in the editor
    Then the cursor is at line 4 column 1

  Scenario: A count repeats an edit
    When I press "2dd" in the editor
    Then the buffer holds:
      """
      three
      four
      """

  Scenario: A count that runs past the end stops at the end
    When I press "9j" in the editor
    Then the cursor is at line 4 column 1

  Scenario: yy yanks a line and p puts it back below
    When I press "yyjp" in the editor
    Then the buffer holds:
      """
      one
      two
      one
      three
      four
      """

  Scenario: Deleting fills the register too
    When I press "ddp" in the editor
    Then the buffer holds:
      """
      two
      one
      three
      four
      """

  Scenario: Yanking a visual selection takes every selected line
    When I press "Vjy" in the editor
    And I press "Gp" in the editor
    Then the buffer holds:
      """
      one
      two
      three
      four
      one
      two
      """

  Scenario: dG deletes from the cursor to the end of the file
    When I press "jdG" in the editor
    Then the buffer holds:
      """
      one
      """

  Scenario: dG is one undo
    When I press "jdG" in the editor
    And I press "u" in the editor
    Then the buffer holds:
      """
      one
      two
      three
      four
      """

  Scenario: dgg deletes from the cursor back to the start of the file
    When I press "jjdgg" in the editor
    Then the buffer holds:
      """
      four
      """

  Scenario: db deletes the word behind the cursor
    Given "src/words.js" is open in the editor holding:
      """
      one two three
      """
    When I press "wwdb" in the editor
    Then the buffer holds:
      """
      one three
      """
    And the cursor is at line 1 column 5

  Scenario: A count belongs to the motion the operator takes
    When I press "d2j" in the editor
    Then the buffer holds:
      """
      four
      """

  Scenario: An operator over a key that names no motion says so
    When I press "dq" in the editor
    Then the notice is "no-such-motion"
    And the message names "dq"
    And the buffer holds:
      """
      one
      two
      three
      four
      """
    And the pending command shows ""

  Scenario: Escape cancels a half-typed operator
    Given I press "d" in the editor
    When I press "Escape" in the editor
    Then no notice was raised
    And the pending command shows ""
    And the buffer holds:
      """
      one
      two
      three
      four
      """
