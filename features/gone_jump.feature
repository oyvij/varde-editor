Feature: Enter on a Gone row lands on the old text in the Review diff

  The Gone list says what was removed; Enter on a row takes me to those lines in the Review view I
  already use. On a `gone` or `moved` row the list closes, the old file is selected in the
  changed-file list and its diff scrolls to the item's first removed line, with focus in the diff.
  On a `signature changed` row it lands on the changed line in that file's diff. The jump only moves
  what Review shows: it opens no buffer and runs nothing.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository
    And the review diff is measured from "aaaaaaaaaaaa"
    And the screen is 14 rows by 100 columns
    And the working tree contains:
      | path        | git status |
      | src/keys.rs | modified   |
      | src/ui.rs   | modified   |
    And "src/keys.rs" held:
      """
      fn route(key: Key) {
          key.send();
      }

      fn spell(word: &str) {
          let one = 1;
          let two = 2;
          let three = 3;
          let four = 4;
          let five = 5;
          let six = 6;
          let seven = 7;
          let eight = 8;
          let nine = 9;
          let ten = 10;
          check(word);
      }

      fn tidy() {
          sweep();
      }
      """
    And "src/keys.rs" now holds:
      """
      fn route(key: &Key) {
          key.send();
      }
      """
    And I opened Review view
    And the tree selection is "src/ui.rs"
    And the diff for "src/ui.rs" is shown
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision     |
      | src/keys.rs |           | route | function | (key: Key)   | a1        | 1    | HEAD         |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 5    | HEAD         |
      | src/keys.rs |           | tidy  | function | ()           | d4        | 19   | HEAD         |
      | src/keys.rs |           | route | function | (key: &Key)  | a1        | 1    | working tree |
      | src/ui.rs   |           | spell | function | (word: &str) | b2        | 30   | working tree |
    And I run ":gone"

  Scenario: Enter on a gone row lands on the item's first removed line
    Given I press the key "j"
    And I press the key "j"
    When I press the key "Enter"
    Then the Gone list is closed
    And the tree selection is "src/keys.rs"
    And the diff for "src/keys.rs" is shown
    And the diff cursor is on the removed line "fn tidy() {"
    And the diff cursor is on screen
    And the editor pane has focus

  Scenario: Enter on a moved row lands in the old file
    Given I press the key "j"
    When I press the key "Enter"
    Then the Gone list is closed
    And the tree selection is "src/keys.rs"
    And the diff for "src/keys.rs" is shown
    And the diff cursor is on the removed line "fn spell(word: &str) {"
    And the diff cursor is on screen
    And the editor pane has focus

  Scenario: Enter on a signature changed row lands on the changed line
    When I press the key "Enter"
    Then the Gone list is closed
    And the tree selection is "src/keys.rs"
    And the diff for "src/keys.rs" is shown
    And the diff cursor is on the removed line "fn route(key: Key) {"
    And the editor pane has focus

  Scenario: The jump opens no buffer and runs no command
    Given I press the key "j"
    When I press the key "Enter"
    Then no file was opened in the editor
    And no command has been executed
    And no file was written
    And the git status is unchanged

  Scenario: The cheatsheet lists Enter for the Gone list
    Then the cheatsheet lists "Enter" for "Gone list"
