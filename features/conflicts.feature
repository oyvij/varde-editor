Feature: Conflicts

  A merge or a rebase that could not decide leaves Conflicts in a file's text: a current side and an
  incoming side, sometimes their common ancestor, between marker lines. The editor draws each marker
  as a bar naming its side, with buttons to accept one, and tints the sides — but the text is only
  ever text. Accepting a side is an edit to the Buffer like any other: nothing is saved, and nothing
  Varde does stages it.

  The Conflict list is the Corner occupant that walks them: every file git reports as unmerged, and
  under each the Conflicts still in its text. A file with none left stays listed until git stops
  reporting it, which is when the user has staged it themselves.

  Background:
    Given the workspace root is "/w"

  Rule: A Conflict is drawn over its text, and never changes it

    Background:
      Given "src/a.rs" is open in the editor holding:
        """
        fn a() {}
        <<<<<<< HEAD
        let x = 1;
        =======
        let x = 2;
        >>>>>>> feature
        fn b() {}
        """

    Scenario: A Conflict draws three bars and tints its two sides
      Then the editor draws the lines as:
        | line | as        |
        | 1    | text      |
        | 2    | buttons   |
        | 3    | current   |
        | 4    | separator |
        | 5    | incoming  |
        | 6    | bar       |
        | 7    | text      |
      And the Conflict at line 2 is between "HEAD" and "feature"
      And the buffer is unchanged

    Scenario: The marker line the cursor is on is drawn as its text
      When the cursor is at line 4 column 1
      Then the editor draws line 4 as "text"
      And the editor draws line 2 as "buttons"

    Scenario: A region whose markers are gone is no longer a Conflict
      Given the cursor is at line 4 column 1
      When I press "dd" in the editor
      Then the editor draws line 2 as "text"

  Rule: Accepting a side replaces the whole Conflict, as one undo step

    Background:
      Given "src/a.rs" is open in the editor holding:
        """
        fn a() {}
        <<<<<<< HEAD
        let x = 1;
        =======
        let x = 2;
        >>>>>>> feature
        fn b() {}
        """
      And the cursor is at line 3 column 1

    Scenario: cc keeps the current side
      When I press "cc" in the editor
      Then the buffer holds "fn a() {}\nlet x = 1;\nfn b() {}"
      And the buffer has unsaved edits
      And no file was written
      And no command has been executed

    Scenario: ci keeps the incoming side
      When I press "ci" in the editor
      Then the buffer holds "fn a() {}\nlet x = 2;\nfn b() {}"

    Scenario: cb keeps both, current first
      When I press "cb" in the editor
      Then the buffer holds "fn a() {}\nlet x = 1;\nlet x = 2;\nfn b() {}"

    Scenario: One u puts the whole Conflict back
      Given I press "cb" in the editor
      When I press "u" in the editor
      Then the buffer holds:
        """
        fn a() {}
        <<<<<<< HEAD
        let x = 1;
        =======
        let x = 2;
        >>>>>>> feature
        fn b() {}
        """

    Scenario: cc outside a Conflict changes nothing
      Given the cursor is at line 7 column 1
      When I press "cc" in the editor
      Then the buffer is unchanged

    Scenario Outline: A button on the bar does what its key does
      Given the cursor is at line 1 column 1
      When I click the "<side>" button on the Conflict's bar
      Then the buffer holds "<kept>"
      And no command has been executed

      Examples:
        | side     | kept                                          |
        | current  | fn a() {}\nlet x = 1;\nfn b() {}              |
        | incoming | fn a() {}\nlet x = 2;\nfn b() {}              |
        | both     | fn a() {}\nlet x = 1;\nlet x = 2;\nfn b() {}  |

  Rule: A diff3 Conflict carries its common ancestor

    Background:
      Given "src/a.rs" is open in the editor holding:
        """
        fn a() {}
        <<<<<<< HEAD
        let x = 1;
        ||||||| base
        let x = 0;
        =======
        let x = 2;
        >>>>>>> feature
        """

    Scenario: The ancestor has a bar and a tint of its own
      Then the editor draws the lines as:
        | line | as        |
        | 2    | buttons   |
        | 3    | current   |
        | 4    | bar       |
        | 5    | ancestor  |
        | 6    | separator |
        | 7    | incoming  |
        | 8    | bar       |

    Scenario: Accepting drops the ancestor
      Given the cursor is at line 5 column 1
      When I press "cb" in the editor
      Then the buffer holds "fn a() {}\nlet x = 1;\nlet x = 2;"

  Rule: The Conflict list walks every unmerged file's Conflicts

    Background:
      Given the project holds:
        | file     | contents                                                                                        |
        | src/a.rs | a\n<<<<<<< HEAD\none\n=======\ntwo\n>>>>>>> topic\nb\n<<<<<<< HEAD\nthree\n=======\nfour\n>>>>>>> topic |
        | src/b.rs | <<<<<<< HEAD\nx\n=======\ny\n>>>>>>> topic                                                      |
        | src/c.rs | clean                                                                                           |
      And git reports as unmerged:
        | src/b.rs |
        | src/a.rs |
      And "src/c.rs" is open in the editor holding "clean"

    Scenario: Palette m puts the Conflict list in the Corner
      Given I open the palette
      When I press "m" in the palette
      Then the Corner holds the Conflict list
      And the conflicts pane has focus

    Scenario: One row per Conflict under each unmerged file, by path
      When I show the Conflict list
      Then the Conflict list rows are:
        | file     | line |
        | src/a.rs |      |
        | src/a.rs | 2    |
        | src/a.rs | 8    |
        | src/b.rs |      |
        | src/b.rs | 1    |

    Scenario: Enter on a row opens the file at that Conflict, records a Visit and moves to the editor
      Given the Conflict list is shown
      And the Conflict list selection is on "src/a.rs" line 8
      When I press "Enter"
      Then the current buffer is "src/a.rs"
      And the cursor is at line 8 column 1
      And the editor pane has focus
      And the cursor history holds:
        | file     | line |
        | src/c.rs | 1    |

    Scenario: Enter on a file row lands on its first Conflict
      Given the Conflict list is shown
      And the Conflict list selection is on the row for "src/b.rs"
      When I press "Enter"
      Then the current buffer is "src/b.rs"
      And the cursor is at line 1 column 1

    Scenario: A click on a row lands there and leaves the keyboard in the list
      Given the Conflict list is shown
      When I click the Conflict list row for "src/a.rs" line 2
      Then the current buffer is "src/a.rs"
      And the cursor is at line 2 column 1
      And the conflicts pane has focus

    Scenario: A file whose last Conflict is resolved stays listed
      Given the Conflict list is shown
      And I jump to "src/b.rs" line 1
      When I press "ci" in the editor
      Then the Conflict list rows are:
        | file     | line |
        | src/a.rs |      |
        | src/a.rs | 2    |
        | src/a.rs | 8    |
        | src/b.rs |      |
      And no command has been executed
      And no file was written

    Scenario: Resolving the selected Conflict moves the selection to the next row
      Given the Conflict list is shown
      And the Conflict list selection is on "src/a.rs" line 2
      And I jump to "src/a.rs" line 2
      When I press "ci" in the editor
      Then the Conflict list selection is on "src/a.rs" line 4

    Scenario: A file git stops reporting leaves the list
      Given the Conflict list is shown
      When git reports as unmerged:
        | src/a.rs |
      Then the Conflict list rows are:
        | file     | line |
        | src/a.rs |      |
        | src/a.rs | 2    |
        | src/a.rs | 8    |
