Feature: The Diagnostic list

  The editor marks what the Language servers say about the Buffer on screen, and nothing says what
  they say about the rest of the project. The Diagnostic list is the Corner occupant that does: every
  Diagnostic `State::diagnostics` holds, one Severity at a time, under a heading per file — for files
  nobody has open as much as for Buffers.

  It opens on the most severe Severity that has any, because that is what needs fixing first. A
  Severity label on its top border names each Severity with its count, and a letter or a click on
  one shows another. A row is a Landing on the Diagnostic's start, and Enter on it is a Jump.

  The file tree's border carries the project's error and warning totals, so a reader looking at one
  file learns that another is broken. A total of zero is not drawn, and neither is information or a
  hint: a nudge that is always there is one nobody reads.

  Background:
    Given the workspace root is "/w"
    And the project holds:
      | file     | contents                                              |
      | src/a.rs | fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn e() {} |
      | src/b.rs | fn f() {}\nfn g() {}                                  |
    And a language server for "rust" is ready
    And "src/editor.rs" is open in the editor holding:
      """
      fn main() {}
      """

  Rule: The palette puts the Diagnostic list in the Corner

    Scenario: Palette i replaces whatever the Corner holds
      Given the Corner shows the Breakpoint list
      And I open the palette
      When I press "i" in the palette
      Then the Corner holds the Diagnostic list
      And the diagnostics pane has focus

    Scenario: With errors present it opens on Errors
      Given the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity | message    |
        | 2    | warning  | unused     |
        | 3    | error    | no such fn |
      When I show the Diagnostic list
      Then the Diagnostic list shows "error"

    Scenario: With no errors it opens on the most severe Severity there is
      Given the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity | message |
        | 2    | hint     | rename  |
        | 3    | warning  | unused  |
      When I show the Diagnostic list
      Then the Diagnostic list shows "warning"

    Scenario: With nothing at all it opens on Errors, empty
      When I show the Diagnostic list
      Then the Diagnostic list shows "error"
      And the Diagnostic list has no rows

  Rule: A Severity label shows another Severity

    Background:
      Given the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity    | message |
        | 1    | error       | broken  |
        | 2    | warning     | unused  |
        | 3    | warning     | unused  |
        | 4    | information | note    |
        | 5    | hint        | rename  |
      And the Diagnostic list is shown

    Scenario Outline: A letter shows its Severity
      When I press "<key>"
      Then the Diagnostic list shows "<severity>"

      Examples:
        | key | severity    |
        | w   | warning     |
        | i   | information |
        | h   | hint        |
        | e   | error       |

    Scenario: A click on a Severity label shows its Severity
      When I click the Severity label for "hint"
      Then the Diagnostic list shows "hint"
      And the diagnostics pane has focus

    Scenario: Each Severity label carries its count
      Then the Severity labels count:
        | error       | 1 |
        | warning     | 2 |
        | information | 1 |
        | hint        | 1 |

  Rule: Rows are grouped under a heading per file

    Scenario: Files by path, then Diagnostics by line and column
      Given the language server for "rust" publishes diagnostics for "src/b.rs":
        | line | severity | message |
        | 2    | error    | second  |
      And the language server for "rust" publishes diagnostics for "src/a.rs" with spans:
        | line | from | to | severity | message |
        | 4    | 1    | 2  | error    | later   |
        | 2    | 8    | 9  | error    | right   |
        | 2    | 4    | 5  | error    | left    |
        | 3    | 1    | 2  | warning  | other   |
      When I show the Diagnostic list
      Then the Diagnostic list rows are:
        | file     | line | column |
        | src/a.rs |      |        |
        | src/a.rs | 2    | 4      |
        | src/a.rs | 2    | 8      |
        | src/a.rs | 4    | 1      |
        | src/b.rs |      |        |
        | src/b.rs | 2    | 1      |

  Rule: A row is a Landing on its Diagnostic

    Background:
      Given the language server for "rust" publishes diagnostics for "src/a.rs" with spans:
        | line | from | to | severity | message |
        | 2    | 4    | 5  | error    | broken  |
        | 4    | 6    | 7  | error    | broken  |
      And the Diagnostic list is shown

    Scenario: Enter on a row opens the file there, records a Visit and moves to the editor
      Given the Diagnostic list selection is on "src/a.rs" line 4
      When I press "Enter"
      Then the current buffer is "src/a.rs"
      And the cursor is at line 4 column 6
      And the editor pane has focus
      And the cursor history holds:
        | file          | line |
        | src/editor.rs | 1    |

    Scenario: Enter on a file heading lands on its first Diagnostic
      Given the Diagnostic list selection is on the heading for "src/a.rs"
      When I press "Enter"
      Then the current buffer is "src/a.rs"
      And the cursor is at line 2 column 4

    Scenario: A click on a row lands there and leaves the keyboard in the list
      When I click the Diagnostic list row for "src/a.rs" line 4
      Then the current buffer is "src/a.rs"
      And the cursor is at line 4 column 6
      And the diagnostics pane has focus

  Rule: The list reads the Diagnostics as they are now

    Scenario: A fixed Diagnostic hands the selection to the row that took its place
      Given the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity | message |
        | 1    | error    | one     |
        | 3    | error    | two     |
        | 5    | error    | three   |
      And the Diagnostic list is shown
      And the Diagnostic list selection is on "src/a.rs" line 3
      When the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity | message |
        | 1    | error    | one     |
        | 5    | error    | three   |
      Then the Diagnostic list selection is on "src/a.rs" line 5

  Rule: The tree's border counts the errors and warnings

    Scenario: Errors and warnings are counted, and nothing else is
      When the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity    | message |
        | 1    | error       | broken  |
        | 2    | warning     | unused  |
        | 3    | warning     | unused  |
        | 4    | information | note    |
        | 5    | hint        | rename  |
      Then the tree's border counts:
        | error   | 1 |
        | warning | 2 |

    Scenario: A Severity with nothing in it is not drawn
      When the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity    | message |
        | 2    | warning     | unused  |
        | 4    | information | note    |
      Then the tree's border counts:
        | warning | 1 |

    Scenario: The totals follow a report that clears them
      Given the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity | message |
        | 1    | error    | broken  |
      When the language server for "rust" publishes no diagnostics for "src/a.rs"
      Then the tree's border counts nothing

    Scenario Outline: A click on a count opens the Diagnostic list on its Severity
      Given the language server for "rust" publishes diagnostics for "src/a.rs":
        | line | severity | message |
        | 1    | error    | broken  |
        | 2    | warning  | unused  |
      When I click the tree's <severity> count
      Then the Corner holds the Diagnostic list
      And the Diagnostic list shows "<severity>"

      Examples:
        | severity |
        | error    |
        | warning  |
