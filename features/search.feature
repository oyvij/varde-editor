Feature: Searching file contents

  How it's invoked: Ctrl+F, or `f` in the command palette. From the editor, `*`
  searches the word under the cursor and `gr` searches the visual selection —
  a function name looked up without typing it.

  A full-screen modal, absent until summoned. Hits are grouped by file with
  their line numbers and the matching line, all files at once, read top to
  bottom. Enter opens the selected hit at its line; `Ctrl+O` opens every file with a
  hit. Esc closes.

  The arrows step hit by hit and Ctrl+N/Ctrl+P step file by file — one file's
  hits can fill the box, so hit by hit is not a way through a long list. The
  list follows whichever the selection lands on, and the wheel moves it
  independently: the box covers every pane, so while it is up the wheel is its
  own.

  Files are scanned in alphabetical order, so the same query gives the same
  results every run rather than whatever order the filesystem offered.

  The search icon on a folder in the tree opens the same box confined to that
  folder. The scope belongs to the box, not to one query: it survives every
  retype until the box is closed.

  Matching is literal and case-insensitive unless the query contains a capital,
  so `foo(` needs no escaping — which matters when the query arrives from a
  selection.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project holds:
      | path        | contents                          |
      | src/main.rs | fn update(state)\nlet Update = 1  |
      | src/lib.rs  | pub fn update(s)\n// no match here |
      | README.md   | Update the docs                   |

  Scenario: Opening search asks for a query
    When I open search
    Then the search is open
    And the search query is ""
    And there are no hits

  Scenario: Escape closes it
    Given I open search
    When I close search
    Then the search is not open

  Scenario: A query finds matching lines
    Given I open search
    When I search for "update"
    Then the hits are:
      | README.md   | 1 |
      | src/lib.rs  | 1 |
      | src/main.rs | 1 |
      | src/main.rs | 2 |

  Scenario: A character typed after moving left goes into the middle of the query
    Given I open search
    And I type "upte" into the search
    And I press the key "Left"
    And I press the key "Left"
    When I type "da" into the search
    Then the search query is "update"
    And the hits are:
      | README.md   | 1 |
      | src/lib.rs  | 1 |
      | src/main.rs | 1 |
      | src/main.rs | 2 |

  Scenario: A search scoped to a folder ignores everything outside it
    Given I open search in "src"
    When I search for "update"
    Then the hits are:
      | src/lib.rs  | 1 |
      | src/main.rs | 1 |
      | src/main.rs | 2 |

  Scenario: An unscoped search is the whole project
    Given I open search
    When I search for "update"
    Then the search is scoped to the whole project

  Scenario: Hits are grouped by file, in the order they appear
    Given I open search
    When I search for "update"
    Then the hit files in order are:
      | README.md   |
      | src/lib.rs  |
      | src/main.rs |

  Scenario: A lowercase query ignores case
    Given I open search
    When I search for "update"
    Then 4 hits were found

  Scenario: A capital makes the query exact
    Given I open search
    When I search for "Update"
    Then 2 hits were found

  Scenario: A query with no matches finds nothing
    Given I open search
    When I search for "zzzz"
    Then there are no hits

  Scenario: An empty query finds nothing
    Given I open search
    When I search for ""
    Then there are no hits

  Scenario: Punctuation needs no escaping
    Given I open search
    When I search for "update(s)"
    Then 1 hits were found

  Scenario: Moving through the hits
    Given I open search
    And I search for "update"
    When I move down in the search
    Then the selected hit is "src/lib.rs" line 1

  Scenario: The selection stops at the last hit
    Given I open search
    And I search for "Update"
    When I move down in the search
    And I move down in the search
    And I move down in the search
    Then the selected hit is "src/main.rs" line 2

  Scenario: Ctrl+N steps to the first hit in the next file
    Given I open search
    And I search for "update"
    When I jump to the next file in the search
    Then the selected hit is "src/lib.rs" line 1

  Scenario: The step past a file skips the rest of its hits
    Given I open search
    And I search for "update"
    And I jump to the next file in the search
    When I jump to the next file in the search
    Then the selected hit is "src/main.rs" line 1

  Scenario: The last file is where the step forward stops
    Given I open search
    And I search for "update"
    And I jump to the next file in the search
    And I jump to the next file in the search
    When I jump to the next file in the search
    Then the selected hit is "src/main.rs" line 1

  Scenario: Ctrl+P steps back to the top of the file it is in
    Given I open search
    And I search for "update"
    And I move down in the search
    And I move down in the search
    And I move down in the search
    When I jump to the previous file in the search
    Then the selected hit is "src/main.rs" line 1

  Scenario: From a file's first hit the step back lands on the file above
    Given I open search
    And I search for "update"
    And I jump to the next file in the search
    When I jump to the previous file in the search
    Then the selected hit is "README.md" line 1

  Scenario: The first file is where the step back stops
    Given I open search
    And I search for "update"
    When I jump to the previous file in the search
    Then the selected hit is "README.md" line 1

  Scenario: Enter opens the selected hit at its line
    Given I open search
    And I search for "update"
    When I open the selected hit
    Then "/home/me/projects/varde/README.md" is open in the editor
    And the cursor is at line 1 column 1
    And the search is not open

  Scenario: Opening a hit lands on the word, not the start of its line
    Given I open search
    And I search for "update"
    And I move down in the search
    And I move down in the search
    And I move down in the search
    When I open the selected hit
    Then "/home/me/projects/varde/src/main.rs" is open in the editor
    And the cursor is at line 2 column 5

  Scenario: Opening a hit leaves the word selected
    Given I open search
    And I search for "update"
    And I move down in the search
    And I move down in the search
    And I move down in the search
    When I open the selected hit
    Then the selection holds "Update"

  Scenario: A hit in an unsaved buffer opens on the word too
    Given "src/lib.rs" is open in the editor with unsaved edits holding:
      """
      fn renamed(s)
      """
    And I open search
    And I search for "renamed"
    When I open the selected hit
    Then the cursor is at line 1 column 4
    And the selection holds "renamed"

  Scenario: o opens every file with a hit
    Given I open search
    And I search for "update"
    When I open every hit
    Then the open buffers are:
      | README.md   |
      | src/lib.rs  |
      | src/main.rs |

  Scenario: Unsaved edits are searched, not the saved file
    Given "src/lib.rs" is open in the editor with unsaved edits holding:
      """
      fn renamed(s)
      """
    And I open search
    When I search for "renamed"
    Then 1 hits were found

  Scenario: Star searches the word under the cursor
    Given "src/main.rs" is open in the editor holding:
      """
      fn update(state)
      """
    And the editor pane has focus
    And I press "w" in the editor
    When I press "*" in the editor
    Then the search is open
    And the search query is "update"

  Scenario: A star in insert mode is typed into the buffer
    Given "src/main.rs" is open in the editor holding:
      """
      fn update(state)
      """
    And the editor pane has focus
    And the editor mode is insert
    When I press "*" in the editor
    Then the search is not open
    And the buffer holds:
      """
      *fn update(state)
      """

  Scenario: gr searches the text selected with the mouse
    Given "src/main.rs" is open in the editor holding:
      """
      fn update(state)
      """
    And I drag across "update" in the editor pane
    And the editor pane has focus
    When I press "gr" in the editor
    Then the search is open
    And the search query is "update"

  Scenario: gr falls back to the word under the cursor
    Given "src/main.rs" is open in the editor holding:
      """
      fn update(state)
      """
    And the editor pane has focus
    When I press "gr" in the editor
    Then the search query is "fn"

  Scenario: Tab completes to a word from the hits
    Given I open search
    And I search for "upda"
    When I complete the search
    Then the search query is "update"

  Scenario: There is nothing to complete when nothing matches
    Given I open search
    And I search for "zzzz"
    When I complete the search
    Then the search query is "zzzz"

  Rule: The list follows the selection, and a click follows the list

    A hit's row on screen is its index plus the file headings above it. Counting
    only the hits is what left the selection walking off the bottom of the box
    with the view standing still — invisible until a fourth file had a hit, which
    is one more than every scenario above has.

    The click reads that arithmetic backwards: a wheel can reach a hit no key
    sequence reaches comfortably, and the hit then has to be markable where it
    sits. Clicking selects and nothing else — Enter is still what opens.

    Five files with a hit each is ten rows in a box that shows five.

    Background:
      Given the project holds:
        | path | contents |
        | a.rs | update   |
        | b.rs | update   |
        | c.rs | update   |
        | d.rs | update   |
        | e.rs | update   |
      And the screen is 16 rows by 100 columns
      And I open search
      And I search for "update"

    Scenario: The list scrolls to keep the selected hit on screen
      When I move down in the search
      And I move down in the search
      And I move down in the search
      And I move down in the search
      Then the results start at row 6
      And the selected hit is "e.rs" line 1

    Scenario: A new query takes the list back to the top
      Given I move down in the search
      And I move down in the search
      And I move down in the search
      And I move down in the search
      When I search for "updat"
      Then the results start at row 1

    Scenario: The wheel scrolls the results wherever the pointer is
      When I scroll down with the pointer over the editor pane
      Then the results start at row 2
      And the selected hit is "a.rs" line 1

    Scenario: A key after the wheel brings the selection back
      Given I scroll down with the pointer over the editor pane
      And I scroll down with the pointer over the editor pane
      When I move up in the search
      Then the results start at row 1

    Scenario: The wheel leaves the file tree behind the box alone
      When I scroll down with the pointer over the file tree pane
      Then the file tree view starts at row 1

    Scenario: A click marks the hit under the pointer after a scroll
      Given I scroll down 3 times with the pointer over the editor pane
      When I click result row 3
      Then the selected hit is "c.rs" line 1
      And no file was opened in the editor

    Scenario: Enter opens the hit the click marked
      Given I scroll down 3 times with the pointer over the editor pane
      And I click result row 3
      When I open the selected hit
      Then "/home/me/projects/varde/c.rs" is open in the editor

    Scenario: A click on a file heading marks nothing
      Given I scroll down with the pointer over the editor pane
      When I click result row 2
      Then the selected hit is "a.rs" line 1
      And no file was opened in the editor

    Scenario: A click on the box's own chrome marks nothing
      When I click result row 6
      Then the selected hit is "a.rs" line 1
      And no file was opened in the editor
