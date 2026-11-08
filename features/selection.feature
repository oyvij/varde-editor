Feature: Selecting and copying

  Varde owns selection because enabling mouse capture disables the terminal's
  own drag-to-select. Each pane selects what it actually holds: characters in
  the editor, scrollback in the terminal, the visible screen in the AI pane, and
  in the tree a drag just moves the row selection — a filename is not text you
  copy character by character.

  The AI pane picks only what is on screen. An AI CLI runs full-screen, where
  there is no history behind it to select from; ADR-0002 records why.

  Copying uses the system clipboard, and falls back to OSC 52 so that copying
  still reaches your real machine when Varde is running over SSH.

  Background:
    Given the workspace root is "/home/me/projects/varde"

  Scenario: Dragging in the editor selects text
    Given "src/tree.js" is open in the editor holding:
      """
      run("unquoted path")
      """
    When I drag across "unquoted path" in the editor pane
    Then the selection holds "unquoted path"

  Scenario: Dragging across a line break takes both ends and the rows between
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      five six
      """
    When I drag in the editor pane from line 1 column 5 to line 3 column 4
    Then the selection holds:
      """
      two
      three four
      five
      """

  Scenario: Dragging upward picks what dragging downward picks
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      five six
      """
    When I drag in the editor pane from line 3 column 4 to line 1 column 5
    Then the selection holds:
      """
      two
      three four
      five
      """

  Scenario: Dragging within one row picks that row's characters
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      """
    When I drag in the editor pane from line 2 column 1 to line 2 column 5
    Then the selection holds "three"

  Scenario: Dragging in the terminal selects scrollback
    Given the terminal shows:
      """
      bash-5.3$ ls
      """
    When I drag across "bash-5.3$ ls" in the terminal pane
    Then the selection holds "bash-5.3$ ls"

  Scenario: Dragging across rows in the terminal takes its scrollback the same way
    Given the terminal shows:
      """
      bash-5.3$ ls
      Cargo.toml  src
      """
    When I drag in the terminal pane from line 1 column 11 to line 2 column 10
    Then the selection holds:
      """
      ls
      Cargo.toml
      """

  Scenario: Dragging in the AI pane selects the session's output
    Given the AI session shows:
      """
      Use ripgrep instead.
      """
    When I drag across "ripgrep" in the AI pane
    Then the selection holds "ripgrep"

  Scenario: Dragging across rows in the AI pane takes its screen the same way
    Given the AI session shows:
      """
      Use ripgrep instead.
      It is faster than grep.
      """
    When I drag in the AI pane from line 1 column 5 to line 2 column 8
    Then the selection holds:
      """
      ripgrep instead.
      It is fa
      """

  Scenario: Copying an AI answer takes it out of the workspace
    Given a system clipboard is available
    And the AI session shows:
      """
      Use ripgrep instead.
      """
    And I drag across "ripgrep" in the AI pane
    When I copy the selection
    Then the clipboard holds "ripgrep"

  Scenario: A drag in the AI pane picks its own screen, not the terminal's
    Given the terminal shows:
      """
      bash-5.3$ ls
      """
    And the AI session shows:
      """
      Use ripgrep instead.
      """
    When I drag across "ripgrep" in the AI pane
    Then the selection holds "ripgrep"

  Scenario: Clicking drops what a drag picked
    Given the AI session shows:
      """
      Use ripgrep instead.
      """
    And I drag across "ripgrep" in the AI pane
    When I click in the AI pane
    Then the selection holds nothing

  Scenario: Deleting after a drag removes exactly what was dragged
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      """
    And I drag in the editor pane from line 1 column 4 to line 2 column 5
    When I press "d" in the editor
    Then the buffer holds:
      """
      one four
      """

  Scenario: Dragging in the tree selects the row instead
    Given the file tree shows the collapsed folder "src"
    When I drag across the row "src" in the file tree pane
    Then the tree selection is "src"
    And the selection holds nothing

  Scenario: Copying uses the system clipboard
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      run("unquoted path")
      """
    And I drag across "unquoted path" in the editor pane
    When I copy the selection
    Then the clipboard holds "unquoted path"

  Scenario: Copying without a system clipboard falls back to the terminal
    Given no system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      run("unquoted path")
      """
    And I drag across "unquoted path" in the editor pane
    When I copy the selection
    Then the terminal was asked to hold "unquoted path"

  Scenario: Copying a selection that spans rows keeps its line breaks
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      """
    And I drag in the editor pane from line 1 column 5 to line 2 column 5
    When I copy the selection
    Then the clipboard holds:
      """
      two
      three
      """

  Scenario: Copying nothing does nothing
    Given a system clipboard is available
    When I copy the selection
    Then the clipboard holds nothing

  Scenario: Holding shift and pressing an arrow picks characters from the cursor
    Given "src/tree.js" is open in the editor holding:
      """
      abc
      def
      """
    When I hold shift and press the Right arrow in the editor
    Then the selection holds "ab"
    And the editor mode is normal

  Scenario: Extending continues across a line break
    Given "src/tree.js" is open in the editor holding:
      """
      abc
      def
      """
    When I hold shift and press the Down arrow in the editor
    Then the selection holds:
      """
      abc
      d
      """

  Scenario: A plain arrow clears the selection
    Given "src/tree.js" is open in the editor holding:
      """
      abc
      def
      """
    And I hold shift and press the Right arrow in the editor
    When I press the Right arrow in the editor
    Then the selection holds nothing
    And the cursor is at line 1 column 3

  Scenario: Escape clears the selection
    Given "src/tree.js" is open in the editor holding:
      """
      abc
      def
      """
    And I hold shift and press the Right arrow in the editor
    When I press "Escape" in the editor
    Then the selection holds nothing

  Scenario: Shift with the forward word key extends a word at a time
    Given "src/tree.js" is open in the editor holding:
      """
      one two three
      """
    When I press "W" in the editor
    Then the selection holds "one"

  Scenario: Extending by word twice reaches the next word
    Given "src/tree.js" is open in the editor holding:
      """
      one two three
      """
    And I press "W" in the editor
    When I press "W" in the editor
    Then the selection holds "one two"

  Scenario: Shift with the back word key extends to the start of the previous word
    Given "src/tree.js" is open in the editor holding:
      """
      one two three
      """
    And I press "ww" in the editor
    When I press "B" in the editor
    Then the selection holds "two t"

  Scenario: Shift and alt with an arrow extends by word as well
    Given "src/tree.js" is open in the editor holding:
      """
      one two three
      """
    When I hold shift and alt and press the Right arrow in the editor
    Then the selection holds "one"

  Scenario: Extending by word while inserting moves rather than typing a letter
    Given "src/tree.js" is open in the editor holding:
      """
      one two three
      """
    And I press "i" in the editor
    When I hold shift and alt and press the Right arrow in the editor
    Then the selection holds "one"
    And the buffer holds:
      """
      one two three
      """

  Scenario: The unmodified word keys move without extending
    Given "src/tree.js" is open in the editor holding:
      """
      one two three
      """
    When I press "w" in the editor
    Then the cursor is at line 1 column 5
    And the selection holds nothing

  Scenario: Extending by word from inside a word takes the remainder of that word
    Given "src/tree.js" is open in the editor holding:
      """
      one two three
      """
    And I press "l" in the editor
    When I press "W" in the editor
    Then the selection holds "ne"

  Scenario: Extending by word continues across a line break
    Given "src/tree.js" is open in the editor holding:
      """
      one
      two
      """
    And I press "W" in the editor
    When I press "W" in the editor
    Then the selection holds:
      """
      one
      two
      """

  Scenario: Deleting a charwise selection removes exactly the picked characters
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three
      """
    And I hold shift and press the Right arrow in the editor 3 times
    When I press "d" in the editor
    Then the buffer holds:
      """
      two
      three
      """

  Scenario: Yanking a charwise selection takes exactly the picked characters
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three
      """
    And I hold shift and press the Right arrow in the editor 2 times
    When I press "yp" in the editor
    Then the buffer holds:
      """
      one two
      one
      three
      """

  Scenario: Extending in the editor leaves the file tree's row selection alone
    Given "src/tree.js" is open in the editor holding:
      """
      abc
      def
      """
    And the tree selection is "src"
    When I hold shift and press the Right arrow in the editor
    Then the tree selection is "src"
    And the selection holds "ab"

  Scenario: Yanking a charwise selection also puts it on the clipboard
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      one two
      three
      """
    And I hold shift and press the Right arrow in the editor 2 times
    When I press "y" in the editor
    Then the clipboard holds "one"

  Scenario: Yanking a line also puts it on the clipboard
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      one two
      three
      """
    When I press "yy" in the editor
    Then the clipboard holds "one two"

  Scenario: Yanking without a system clipboard falls back to the terminal
    Given no system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      one two
      three
      """
    When I press "yy" in the editor
    Then the terminal was asked to hold "one two"

  Scenario: Putting reads the register, not the clipboard
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      one two
      three
      """
    And I press "yy" in the editor
    And the terminal shows:
      """
      bash-5.3$ ls
      """
    And I drag across "bash-5.3$ ls" in the terminal pane
    And I copy the selection
    When I press "p" in the editor
    Then the clipboard holds "bash-5.3$ ls"
    And the buffer holds:
      """
      one two
      one two
      three
      """

  Scenario: V leaves a linewise selection the workspace can report
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      five six
      """
    When I press "V" in the editor
    Then the selection holds the lines:
      """
      one two
      """

  Scenario: Copying a linewise selection puts its whole lines on the clipboard
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      five six
      """
    And I press "Vj" in the editor
    When I copy the selection
    Then the clipboard holds the lines:
      """
      one two
      three four
      """

  Scenario: V then G selects to the last line of the file
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      five six
      """
    And I press "VG" in the editor
    When I copy the selection
    Then the clipboard holds the lines:
      """
      one two
      three four
      five six
      """

  Scenario: V then gg selects back to the first line
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      five six
      """
    And I press "jj" in the editor
    When I press "Vgg" in the editor
    Then the selection holds the lines:
      """
      one two
      three four
      five six
      """

  Scenario: Extending a linewise selection past the bottom of the pane scrolls to follow it
    Given the screen is 12 rows by 40 columns
    And "src/long.js" is open in the editor with 12 lines
    When I press "V9j" in the editor
    Then the selected lines are 1 to 10
    And the editor scrolled down

  Scenario: Escape clears the linewise selection and leaves visual mode
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      """
    And I press "Vj" in the editor
    And the selection holds the lines:
      """
      one two
      three four
      """
    When I press "Escape" in the editor
    Then the selection holds nothing
    And the editor mode is normal

  Scenario: Yanking a linewise selection leaves visual mode and the selection with it
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      """
    And I press "Vj" in the editor
    When I press "y" in the editor
    Then the selection holds nothing
    And the editor mode is normal

  Scenario: A drag in the terminal keeps its own selection while a buffer waits in visual mode
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      """
    And I press "V" in the editor
    And the terminal shows:
      """
      bash-5.3$ ls
      """
    And the terminal pane has focus
    When I drag across "bash-5.3$ ls" in the terminal pane
    Then the selection holds "bash-5.3$ ls"

  Scenario: Crossing to Preview leaves no linewise selection over its rows
    Given "README.md" is open in the editor holding:
      """
      # Setup

      Install it.
      """
    And I run ":preview" in the editor
    And I press "V" in the editor
    And the selection holds the lines:
      """
      # Setup
      """
    When I run ":preview" in the editor
    Then the selection holds nothing

  Scenario: A diff shown over a buffer left in visual mode picks none of its lines
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      """
    And I press "V" in the editor
    When the diff for "src/tree.js" is shown
    Then the selection holds nothing

  Scenario: Extending charwise while visual mode is on stays linewise
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      three four
      """
    And I press "V" in the editor
    When I hold shift and press the Right arrow in the editor
    Then the selection holds the lines:
      """
      one two
      """

  Scenario: Pasting what was copied puts the lines back as they were
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      fn a() {
          deep();
      }
      """
    And I press "Vjj" in the editor
    And I copy the selection
    And I press "Escape" in the editor
    And I press "gg" in the editor
    When I paste what was copied into the editor
    Then the buffer holds:
      """
      fn a() {
          deep();
      }
      fn a() {
          deep();
      }
      """
    And the cursor is at line 4 column 1

  Scenario: Pasting what was copied into another buffer puts the lines back as they were
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      fn a() {
          deep();
      }
      """
    And I press "Vjj" in the editor
    And I copy the selection
    And "src/other.js" is open in the editor holding:
      """
      last
      """
    When I paste what was copied into the editor
    Then the buffer holds:
      """
      fn a() {
          deep();
      }
      last
      """
    And the cursor is at line 4 column 1

  Scenario: Double-clicking a word in the editor picks it
    Given "src/tree.js" is open in the editor holding:
      """
      run("unquoted path")
      """
    When I double-click on "unquoted" in the editor
    Then the selection holds "unquoted"

  Scenario: Two clicks further apart than the double-tap window are two clicks
    Given "src/tree.js" is open in the editor holding:
      """
      run("unquoted path")
      """
    When I click twice on "unquoted" in the editor 450ms apart
    Then the selection holds nothing

  Scenario: Double-clicking whitespace picks no word
    Given "src/tree.js" is open in the editor holding:
      """
      run("unquoted path")
      """
    When I double-click at line 1 column 14 in the editor
    Then the selection holds nothing

  Scenario: Double-clicking a word in a Preview picks it off the row
    Given the screen is 26 rows by 120 columns
    And "README.md" is open in the editor holding:
      """
      # Setup

      Install it.
      """
    And the editor is showing preview
    When I double-click on "Install" in the editor
    Then the selection holds "Install"
