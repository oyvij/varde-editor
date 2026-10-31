Feature: The Gone list

  The Gone Chip says how many named items were removed; the Gone list says which, and why each is
  listed. It opens over the centre diff, at the diff pane's full width, so no new pane appears and
  the rows have the most room. Palette `x` opens it, listed as "Gone" under Views and offered only in
  Review view, where it has something to show; so do `:gone` and a click on the Chip, which teaches
  its own keys. `Esc` closes it, back in the diff where I was.

  Each row's label sits in a fixed left column, followed by the sentence saying why the item is
  listed. While a result is on its way the list says it is computing, and a result a save made stale
  stays up, marked stale, until the next one replaces it: the list never goes blank. Opening it reads
  what Gone already found; it writes nothing, opens nothing and runs nothing.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository
    And the review diff is measured from "aaaaaaaaaaaa"
    And the working tree contains:
      | path        | git status |
      | src/keys.rs | modified   |
      | src/ui.rs   | modified   |

  Scenario: The palette offers the Gone list under Views in Review view
    Given I opened Review view
    And the view palette is shown
    Then the view palette offers "Gone" in the group "Views" under the key "x"

  Scenario Outline: The palette does not offer the Gone list outside Review view
    Given the current view is <view>
    And the view palette is shown
    Then the view palette does not offer "Gone"

    Examples:
      | view      |
      | Edit      |
      | Story     |
      | Knowledge |

  Scenario: Palette x opens the Gone list
    Given I opened Review view
    And the view palette is shown
    When I press "x"
    Then the Gone list is open

  Scenario: The command line opens the Gone list
    Given I opened Review view
    When I run ":gone"
    Then the Gone list is open

  Scenario: A click on the Gone Chip opens the Gone list
    Given the divider between the file tree and the editor is at column 70
    And I opened Review view
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    When I click the Gone Chip
    Then the Gone list is open

  Scenario: Esc closes the Gone list with the diff as it was
    Given I opened Review view
    And the diff for "src/ui.rs" is shown
    And I run ":gone"
    When I press the key "Escape"
    Then the Gone list is closed
    And the Review diff is as it was before the Gone list opened

  Scenario: The keys move the selected row
    Given I opened Review view
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
      | src/keys.rs |           | route | function | (key: Key)   | a1        | 88   | HEAD     |
      | src/ui.rs   |           | draw  | function | ()           | c3        | 17   | HEAD     |
    And I run ":gone"
    When I press the key "j"
    And I press the key "Down"
    Then the selected Gone row is 3
    When I press the key "k"
    Then the selected Gone row is 2
    When I press the key "Up"
    And I press the key "Up"
    Then the selected Gone row is 1

  Scenario: Each row shows its label
    Given I opened Review view
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision     |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD         |
      | src/keys.rs |           | route | function | (key: Key)   | a1        | 88   | HEAD         |
      | src/keys.rs |           | tidy  | function | ()           | d4        | 40   | HEAD         |
      | src/ui.rs   |           | spell | function | (word: &str) | b2        | 30   | working tree |
      | src/keys.rs |           | route | function | (key: &Key)  | a1        | 88   | working tree |
    When I run ":gone"
    Then the Gone list shows the labels:
      | label             | name  |
      | moved             | spell |
      | signature-changed | route |
      | gone              | tidy  |

  Scenario: The list says it is computing while a result is on its way
    When I open Review view
    And I run ":gone"
    Then the Gone list is "computing"

  Scenario: A stale result stays up, marked stale
    Given I opened Review view
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    And "src/keys.rs" is open in the editor holding:
      """
      fn route() {}
      """
    And I write the buffer
    When I run ":gone"
    Then the Gone list is "stale"
    And the Gone list shows the labels:
      | label | name  |
      | gone  | spell |

  Scenario: Opening the Gone list writes nothing, opens nothing and runs nothing
    Given I opened Review view
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    When I run ":gone"
    Then the Gone list is open
    And no file was written
    And no file was opened in the editor
    And no command has been executed
    And the git status is unchanged

  Scenario: The cheatsheet lists the Gone keys
    Given I opened Review view
    And the diff for "src/ui.rs" is shown
    Then the cheatsheet lists "x" for "Gone list"
    And the cheatsheet lists ":gone" for "Gone list"
    And the cheatsheet lists "Esc" for "Gone list"
    And the cheatsheet lists "j" for "Gone list"

  Scenario: The list header names the files Gone could not measure
    Given I opened Review view
    And Gone could not measure:
      | file             | language | uncovered             |
      | scripts/build.go | Go       | file                  |
      | src/shape.h      | C/C++    | prototypes-and-macros |
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    When I run ":gone"
    Then the Gone list header names:
      | file             |
      | scripts/build.go |
      | src/shape.h      |
