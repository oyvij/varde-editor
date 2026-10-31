Feature: Gone

  An agent asked for a small fix sometimes rewrites a whole file and quietly drops a function that
  was working. In Review view the deletion hides inside a long diff of reshuffled lines, and the
  Risk delta measures only the code that is still there. So entering Review view also compares the
  named items in HEAD against the working tree, and when one has disappeared a Gone Chip sits on the
  Review border beside the Risk delta. It is a fact, not a verdict: it grades nothing, and it says
  nothing when nothing was removed.

  Gone rides on the analysis Review view already asks for, so it is computed when Review view is
  entered and when Risk recomputes, and never on a keystroke or a save. A save marks what it found
  stale rather than hiding it.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository
    And the review diff is measured from "aaaaaaaaaaaa"
    And the working tree contains:
      | path        | git status |
      | src/keys.rs | modified   |
      | src/ui.rs   | modified   |

  Scenario: Entering Review view asks for one analysis, and it covers Gone
    When I open Review view
    Then 1 analysis was asked for
    And the analysis covers Gone

  Scenario: An item in HEAD that is not in the working tree is shown as gone
    Given I opened Review view
    When the review figures arrive:
      | file        | function | line | cyclomatic | cognitive | was cyclomatic | was cognitive |
      | src/keys.rs | route    | 88   | 26         | 20        | 31             | 24            |
    And the Gone items arrive:
      | file        | container | name  | kind     | signature         | body hash | line | revision     |
      | src/keys.rs |           | route | function | (key: Key) -> Act | a1        | 88   | HEAD         |
      | src/keys.rs |           | spell | function | (word: &str)      | b2        | 120  | HEAD         |
      | src/keys.rs |           | route | function | (key: Key) -> Act | a1        | 88   | working tree |
    Then the tree border shows the review delta
    And the Gone Chip shows 1
    And the Gone rows are:
      | label | file        | name  |
      | gone  | src/keys.rs | spell |

  Scenario: Nothing removed shows no Gone Chip
    Given I opened Review view
    When the Gone items arrive:
      | file        | container | name  | kind     | signature         | body hash | line | revision     |
      | src/keys.rs |           | route | function | (key: Key) -> Act | a1        | 88   | HEAD         |
      | src/keys.rs |           | route | function | (key: Key) -> Act | a1        | 88   | working tree |
      | src/ui.rs   |           | draw  | function | (frame: &Frame)   | c3        | 17   | working tree |
    Then no Gone Chip is shown

  Scenario: An item whose exact body moved to another file is moved, not gone
    Given I opened Review view
    When the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision     |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD         |
      | src/ui.rs   |           | spell | function | (word: &str) | b2        | 30   | working tree |
    Then no Gone Chip is shown
    And the Gone rows are:
      | label | file        | name  |
      | moved | src/keys.rs | spell |

  Scenario: Removed items and files Gone could not measure are both counted on the Chip
    Given I opened Review view
    And Gone could not measure:
      | file             | language | uncovered              |
      | scripts/build.go | Go       | file                   |
      | src/shape.h      | C++      | prototypes-and-macros  |
    When the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    Then the Gone Chip shows 1 gone and 2 not measured

  Scenario: Files Gone could not measure show the Chip even when nothing measured was removed
    Given I opened Review view
    And Gone could not measure:
      | file             | language | uncovered |
      | scripts/build.go | Go       | file      |
    When the Gone items arrive:
      | file        | container | name  | kind     | signature         | body hash | line | revision     |
      | src/keys.rs |           | route | function | (key: Key) -> Act | a1        | 88   | HEAD         |
      | src/keys.rs |           | route | function | (key: Key) -> Act | a1        | 88   | working tree |
    Then the Gone Chip shows 0 gone and 1 not measured

  Scenario: No Gone Chip while the analysis is on its way
    When I open Review view
    Then the Gone state is "computing"
    And no Gone Chip is shown

  Scenario: Typing asks for no analysis
    Given I opened Review view
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    And "src/keys.rs" is open in the editor holding:
      """
      fn route() {}
      """
    When I type "x" in the editor
    Then 1 analysis was asked for
    And the Gone Chip shows 1

  Scenario: A save asks for no analysis and marks what Gone found stale
    Given I opened Review view
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    And "src/keys.rs" is open in the editor holding:
      """
      fn route() {}
      """
    When I write the buffer
    Then 1 analysis was asked for
    And the Gone state is "stale"
    And the Gone Chip shows 1

  Scenario: An answer for an older analysis is dropped
    Given I opened Review view
    And I opened Review view
    And the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    When the Gone items for the earlier analysis arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
      | src/ui.rs   |           | tidy  | function | ()           | d4        | 40   | HEAD     |
    Then the Gone state is "current"
    And the Gone rows are:
      | label | file        | name  |
      | gone  | src/keys.rs | spell |

  Scenario: Gone writes nothing, opens nothing and runs nothing
    Given I opened Review view
    When the Gone items arrive:
      | file        | container | name  | kind     | signature    | body hash | line | revision |
      | src/keys.rs |           | spell | function | (word: &str) | b2        | 120  | HEAD     |
    Then no file was written
    And no file was opened in the editor
    And no command has been executed
    And the git status is unchanged
