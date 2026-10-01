Feature: Editing a file

  The editor is modal, as vim is. Normal mode moves and deletes, insert mode
  types, Escape returns. This first pass covers the motions and edits needed to
  genuinely work in a file; visual mode, counts, registers and macros are not
  here yet.

  Saving writes the draft to disk with :w. If the file changed underneath while
  you were editing, the buffer was flagged for exactly that reason — :w still
  writes, because you were told. :e discards the draft and takes the disk version.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/tree.js" is open in the editor holding:
      """
      one
      two
      three
      """
    And the editor pane has focus

  Scenario: The editor starts in normal mode
    Then the editor mode is normal

  Scenario: Typing in normal mode does not insert
    When I type "x" in the editor
    Then the buffer holds:
      """
      ne
      two
      three
      """

  Scenario: i enters insert mode
    When I press "i" in the editor
    Then the editor mode is insert

  Scenario: Escape returns to normal mode
    Given the editor mode is insert
    When I press "Escape" in the editor
    Then the editor mode is normal

  Scenario: Typing in insert mode inserts at the cursor
    Given the editor mode is insert
    When I type "zero " in the editor
    Then the buffer holds:
      """
      zero one
      two
      three
      """

  Rule: The command line is a normal-mode gesture

    A colon opens the command line, and in insert mode a colon is a character
    like any other — vim's own rule, and the reason a Rust turbofish or a YAML
    key can be typed at all. The distinction is the buffer's mode, not the
    pane's focus: with no buffer open there is nothing to type into, so the
    colon still opens the line.

    Scenario: A colon in normal mode opens the command line
      When I press the key ":"
      Then the command line is open
      And the buffer holds:
        """
        one
        two
        three
        """

    Scenario: A colon in insert mode is typed into the buffer
      Given the editor mode is insert
      When I press the key ":"
      Then the command line is not open
      And the buffer holds:
        """
        :one
        two
        three
        """

    Scenario: A command typed in insert mode is text, not a command
      Given the editor mode is insert
      When I press the key ":"
      And I press the key "q"
      Then the command line is not open
      And the current buffer is "src/tree.js"
      And the buffer holds:
        """
        :qone
        two
        three
        """

  Scenario Outline: Motions move the cursor
    When I press "<keys>" in the editor
    Then the cursor is at line <line> column <column>

    Examples:
      | keys | line | column |
      | l    | 1    | 2      |
      | j    | 2    | 1      |
      | jk   | 1    | 1      |
      | $    | 1    | 3      |
      | jl0  | 2    | 1      |
      | G    | 3    | 1      |
      | Ggg  | 1    | 1      |

  Scenario: Arrow keys move the cursor in normal mode
    When I press the Down arrow in the editor
    Then the cursor is at line 2 column 1

  Scenario: Arrow keys move the cursor in insert mode too
    Given the editor mode is insert
    When I press the Down arrow in the editor
    Then the cursor is at line 2 column 1
    And the editor mode is insert

  Scenario: Arrows stop at the edges like motions do
    Given the editor mode is insert
    When I press the Up arrow in the editor
    Then the cursor is at line 1 column 1

  Scenario: Backspace deletes the character before the cursor
    Given the editor mode is insert
    And I press the Right arrow in the editor
    When I press Backspace in the editor
    Then the buffer holds:
      """
      ne
      two
      three
      """

  Scenario: Backspace at the start of a line joins it to the one above
    Given the editor mode is insert
    And I press the Down arrow in the editor
    When I press Backspace in the editor
    Then the buffer holds:
      """
      onetwo
      three
      """
    And the cursor is at line 1 column 4

  Scenario: Backspace at the very start does nothing
    Given the editor mode is insert
    When I press Backspace in the editor
    Then the buffer holds:
      """
      one
      two
      three
      """

  Scenario: A half-typed command is shown while it is being typed
    When I press "2d" in the editor
    Then the pending command shows "2d"

  Scenario: The pending command clears once it completes
    When I press "2dd" in the editor
    Then the pending command shows ""

  Scenario: Escape abandons a half-typed command
    Given I press "2d" in the editor
    When I press "Escape" in the editor
    Then the pending command shows ""

  Scenario: The cursor cannot leave the buffer
    When I press "kkk" in the editor
    Then the cursor is at line 1 column 1

  Scenario: x deletes the character under the cursor
    When I press "x" in the editor
    Then the buffer holds:
      """
      ne
      two
      three
      """

  Scenario: dd deletes the whole line
    When I press "jdd" in the editor
    Then the buffer holds:
      """
      one
      three
      """

  Scenario: o opens a line below and enters insert mode
    When I press "o" in the editor
    And I type "new" in the editor
    Then the editor mode is insert
    And the buffer holds:
      """
      one
      new
      two
      three
      """

  Scenario: u undoes the last edit
    Given I press "x" in the editor
    When I press "u" in the editor
    Then the buffer holds:
      """
      one
      two
      three
      """

  Scenario: u brings the cursor back to the edit it undid
    Given I press "x" in the editor
    And I press "j" in the editor 2 times
    When I press "u" in the editor
    Then the cursor is at line 1 column 1

  # The modifier spellings, which reach a buffer while inserting too, where
  # `u` and `U` are letters.
  Scenario: Ctrl+Shift+z redoes what Ctrl+z undid while inserting
    Given I press "i" in the editor
    And I type "new" in the editor
    And I press the key "Ctrl+z"
    When I press the key "Ctrl+Shift+z"
    Then the editor mode is insert
    And the buffer holds:
      """
      newone
      two
      three
      """

  Scenario: An edited buffer is dirty
    When I press "x" in the editor
    Then "src/tree.js" has unsaved edits

  Scenario: Writing saves the draft to disk
    Given I press "x" in the editor
    When I write the buffer
    Then "src/tree.js" was written with:
      """
      ne
      two
      three
      """
    And "src/tree.js" has no unsaved edits

  Scenario: Writing a file that changed underneath still writes
    Given I press "x" in the editor
    And "src/tree.js" is changed on disk
    When I write the buffer
    Then "src/tree.js" was written with:
      """
      ne
      two
      three
      """

  Scenario: Ctrl+S writes the buffer from normal mode
    Given I press "x" in the editor
    When I press the key "Ctrl+s"
    Then "src/tree.js" was written with:
      """
      ne
      two
      three
      """
    And the editor mode is normal

  Scenario: Ctrl+S writes the buffer without leaving insert mode
    Given the editor mode is insert
    And I type "zero " in the editor
    When I press the key "Ctrl+s"
    Then "src/tree.js" was written with:
      """
      zero one
      two
      three
      """
    And the editor mode is insert
    And the cursor is at line 1 column 6

  Scenario: Reloading discards the draft
    Given I press "x" in the editor
    When I reload the buffer
    Then "src/tree.js" has no unsaved edits
    And "src/tree.js" is not flagged as changed on disk

  Rule: The view follows the cursor sideways

    The editor pane is narrower than the lines people write in it, and there is
    another pane immediately to its right. Text past the pane's edge is neither
    lost nor hidden behind its neighbour: the view slides sideways to keep the
    cursor visible, in both modes, and slides back when the cursor returns. Line
    numbers are not text, so they stay where they are.

    Scenario: A cursor past the right edge slides the view sideways
      Given the screen is 12 rows by 80 columns
      And the minimap is turned off
      And "src/long.js" is open in the editor holding a 40-character line above a short one
      When I press "$" in the editor
      Then the editor view starts at column 27
      And the cursor is at line 1 column 40

    Scenario: Typing past the right edge slides the view along with the text
      Given the screen is 12 rows by 80 columns
      And the minimap is turned off
      And "src/long.js" is open in the editor holding a 18-character line above a short one
      And I press "$" in the editor
      And I press "a" in the editor
      When I type "xyz" in the editor
      Then the editor view starts at column 9
      And the cursor is at line 1 column 22

    Scenario: The view slides back when the cursor returns to the start
      Given the screen is 12 rows by 80 columns
      And "src/long.js" is open in the editor holding a 40-character line above a short one
      And I press "$" in the editor
      When I press "0" in the editor
      Then the editor view starts at column 1

    Scenario: A line that fits is never scrolled sideways
      Given the screen is 12 rows by 80 columns
      And "src/long.js" is open in the editor holding a 40-character line above a short one
      And I press "$" in the editor
      When I press "j" in the editor
      Then the editor view starts at column 1

  Rule: The editor holds the other end of a pair

    Typing an opening bracket, brace or quote inserts its partner and leaves the
    cursor between them. The insertion is the easy part; the rules around it are
    what stop it being annoying — the closing half steps over itself rather than
    doubling, backspace on an empty pair takes both halves, a quote inside a word
    stays a quote, and a pair typed around a selection wraps it.

    None of it applies in normal mode, where those keys are commands, and none of
    it applies to a paste: text that arrived whole was not typed.

    Scenario: Typing an opening bracket brings its closing half
      Given the editor mode is insert
      When I type "run(" in the editor
      Then the buffer holds:
        """
        run()one
        two
        three
        """
      And the cursor is at line 1 column 5

    Scenario: Typing the closing half where it already sits steps over it
      Given the editor mode is insert
      When I type "run()" in the editor
      Then the buffer holds:
        """
        run()one
        two
        three
        """
      And the cursor is at line 1 column 6

    Scenario: Backspace between the halves of an empty pair takes both
      Given the editor mode is insert
      And I type "run(" in the editor
      When I press Backspace in the editor
      Then the buffer holds:
        """
        runone
        two
        three
        """
      And the cursor is at line 1 column 4

    Scenario: A quote inside a word stays a single quote
      Given the editor mode is insert
      When I type "don't " in the editor
      Then the buffer holds:
        """
        don't one
        two
        three
        """

    Scenario: A pair typed around a selection wraps what was picked and keeps it
      Given the editor mode is insert
      And I hold shift and press the Right arrow in the editor 2 times
      When I type "(" in the editor
      Then the buffer holds:
        """
        (one)
        two
        three
        """
      And the selection holds "one"
      And the cursor is at line 1 column 4

    Scenario: An opening bracket in normal mode is not typed at all
      When I type "(" in the editor
      Then the buffer holds:
        """
        one
        two
        three
        """

    Scenario: Pasted text closes no pairs
      Given the editor mode is insert
      When I paste "foo('bar" into the editor
      Then the buffer holds:
        """
        foo('barone
        two
        three
        """
      And the cursor is at line 1 column 9

    Scenario: A pasted pair undoes in one key
      Given the editor mode is insert
      And I paste "foo('bar" into the editor
      When I press "Escape" in the editor
      And I press "u" in the editor
      Then the buffer holds:
        """
        one
        two
        three
        """

  Rule: A new line keeps its place

    Enter carries the current line's indentation down with it. The whitespace is
    copied rather than measured and re-emitted, so a file indented with tabs stays
    indented with tabs and one indented with three spaces stays indented with
    three — Varde holds no opinion about which is right.

    Between the halves of a bracket pair, Enter opens a block: the closing half
    moves to a line of its own at the original indentation and the cursor lands on
    a blank line one level deeper between them. That one level comes from the file
    too — the shallowest indentation it already has, blank lines not counting. A
    line ending in anything but a pair gets no extra level: deciding that `if (a)`
    opens one needs the language's grammar, and that is the server's answer.

    # Gherkin cannot show the line Enter leaves behind: a docstring line holding
    # only whitespace is dedented away, so a buffer asserted here would claim the
    # new line is empty. The cursor proves it instead — a column is clamped to one
    # past its line's width, so column five is unreachable unless four characters
    # of indentation are really there — and what the buffer holds is asserted by
    # typing into it, which is the reader's own next keystroke anyway.

    Scenario: Enter carries the line's indentation down
      Given "src/deep.js" is open in the editor holding:
        """
        if (a)
            first()
        after()
        """
      And I type "j$a" in the editor
      When I press "Enter" in the editor
      Then the cursor is at line 3 column 5

    Scenario: Enter between the halves of a brace opens a block
      Given "src/blocks.js" is open in the editor holding:
        """
        function outer() {
          first()
        }
        function inner()
        """
      And I type "G$a" in the editor
      And I type " {" in the editor
      When I press "Enter" in the editor
      Then the cursor is at line 5 column 3

    Scenario: The body of an opened block is typed one level deeper, as the file indents
      Given "src/blocks.js" is open in the editor holding:
        """
        function outer() {
          first()
        }
        function inner()
        """
      And I type "G$a" in the editor
      And I type " {" in the editor
      And I press "Enter" in the editor
      When I type "second()" in the editor
      Then the buffer holds:
        """
        function outer() {
          first()
        }
        function inner() {
          second()
        }
        """

    Scenario: Enter on an unindented line still starts at column one
      Given I type "$a" in the editor
      When I press "Enter" in the editor
      Then the cursor is at line 2 column 1
      And the buffer holds:
        """
        one

        two
        three
        """

  Rule: Tab indents while inserting

    Tab in insert mode lays down one level of indentation at the cursor, as one
    edit, so `u` costs the one key the indent cost. The width is configuration's
    answer — `editor.tab_width`, four spaces when no layer names one — rather
    than a number measured off whichever lines happen to be open.

    It is the same width Enter reaches for when it opens a block in a file that
    holds no indentation to copy. Only the fallback: measured indentation still
    wins there, because the lines already in the file are better evidence about
    that file than any configured number is.

    With characters picked, Tab moves them instead of typing into them: the
    lines the selection covers go one level in, and Shift+Tab brings them one
    level back out. An indent typed at the cursor would land in the middle of
    what was picked and leave it exactly where it was, which is the whole reason
    the key means two things here. Shift is read on this key and nowhere else on
    it, because Shift+Tab is the other half of one gesture rather than a
    modifier the binding ignores.

    The level a block moves by comes from the *file*, as the block Enter opens
    does, and not from `editor.tab_width`: a span of lines is already evidence
    of what that file indents with. Coming back out takes the file's unit when
    the line starts with it and the leading spaces it has otherwise, so a file
    two people have edited gets shallower either way instead of refusing.

    The span is handed back covering whole lines, which is what lets a second
    Tab move the same lines again — a charwise span whose columns were measured
    before the edit names different characters after it.

    A snippet's remaining stops keep the key while there are any: the tab-stop
    sequence claims Tab and passes everything else through, so "the next blank"
    wins where it applies and needs no rule of its own to stay ahead of this
    one. In normal mode, over a diff and inside a walked Site a typed character
    does not reach the buffer, and neither does Tab.

    The caret that could not move right on an empty line is the same gap seen
    from the other side, which is why the arrow belongs in this Rule: a column
    is clamped to one past its line's width, so on an empty line there is
    nothing to the right to reach. Getting there is *typing*, which is what Tab
    now is — no virtual space, and no whitespace a motion left behind.

    # Indentation cannot be asserted in a docstring holding only it: gherkin
    # dedents by the shortest non-blank line and emits a whitespace-only line as
    # empty. The cursor proves it instead — a column is clamped to one past its
    # line's width, so column five is unreachable unless four characters are
    # really there.

    Scenario: Tab on an empty line indents it
      Given "src/empty.js" is open in the editor holding nothing
      And the editor mode is insert
      When I press "Tab" in the editor
      Then the cursor is at line 1 column 5

    Scenario: Tab mid-line indents at the cursor, not at the start of the line
      Given the editor mode is insert
      And I press the Right arrow in the editor
      When I press "Tab" in the editor
      Then the buffer holds:
        """
        o    ne
        two
        three
        """
      And the cursor is at line 1 column 6

    # Twice, so the assertion can fail: one level left after one undo is what
    # tells an indent that undid whole apart from one that never happened.
    Scenario: A tabbed indent undoes in one key
      Given the editor mode is insert
      And I press "Tab" in the editor
      And I press "Tab" in the editor
      And I press "Escape" in the editor
      When I type "u" in the editor
      Then the buffer holds:
        """
            one
        two
        three
        """

    # Through a real config layer and a real start, so what is proved is the
    # whole chain: the TOML a project writes, the merge under it, and the width
    # Tab reaches for. A step that set the width on the state directly would
    # have proved only the last link.
    Scenario: The indent width is the project's answer
      Given the project config is:
        """
        [editor]
        tab_width = 2
        """
      And Varde started in the project
      And "src/empty.js" is open in the editor holding nothing
      And the editor mode is insert
      When I press "Tab" in the editor
      Then the cursor is at line 1 column 3

    # The other half of `editor.tab_width`, and the reason it lives in this
    # Rule rather than beside the block scenarios above: Tab never measures, but
    # the block Enter opens measures the file first and only reaches for the
    # configured width when there is nothing in the file to copy. A four
    # hardcoded at that second site is the number the two indent sites
    # disagreed on, so a project on two got two from Tab and four from Enter.
    Scenario: Opening a block uses the project's indent width when the file has none to copy
      Given the project config is:
        """
        [editor]
        tab_width = 2
        """
      And Varde started in the project
      And "src/empty.js" is open in the editor holding nothing
      And the editor mode is insert
      And I type "{" in the editor
      When I press "Enter" in the editor
      Then the cursor is at line 2 column 3

    # The deliberate absence: configuration is the fallback, never the override.
    # A file indented by four keeps four however the project spells its width,
    # because the lines already there are better evidence about this file than
    # any key is.
    Scenario: Opening a block still copies the file's own indentation
      Given the project config is:
        """
        [editor]
        tab_width = 2
        """
      And Varde started in the project
      And "src/blocks.js" is open in the editor holding:
        """
        function outer() {
            first()
        }
        function inner()
        """
      And I type "G$a" in the editor
      And I type " {" in the editor
      When I press "Enter" in the editor
      Then the cursor is at line 5 column 5

    Scenario: Tab with characters picked indents the lines they cover
      Given the editor mode is insert
      And I hold shift and press the Down arrow in the editor
      When I press "Tab" in the editor
      Then the buffer holds:
        """
            one
            two
        three
        """

    Scenario: Shift and Tab brings the picked lines back out
      Given the editor mode is insert
      And I hold shift and press the Down arrow in the editor
      And I press "Tab" in the editor
      When I press "Shift+Tab" in the editor
      Then the buffer holds:
        """
        one
        two
        three
        """

    # The picked lines stay picked, which is what makes indenting twice two
    # presses rather than a selection that has to be made again.
    Scenario: A second Tab moves the same lines again
      Given the editor mode is insert
      And I hold shift and press the Down arrow in the editor
      And I press "Tab" in the editor
      When I press "Tab" in the editor
      Then the buffer holds:
        """
                one
                two
        three
        """

    # The level is measured off the file, so a span inside an indented file
    # moves by what that file uses rather than by the configured width.
    Scenario: A block indent takes its level from the file
      Given "src/two.js" is open in the editor holding:
        """
        if (a) {
          first()
          second()
        }
        """
      And the editor mode is insert
      And I hold shift and press the Down arrow in the editor
      When I press "Tab" in the editor
      Then the buffer holds:
        """
          if (a) {
            first()
          second()
        }
        """

    Scenario: Tab in normal mode is not an edit
      When I press "Tab" in the editor
      Then the buffer holds:
        """
        one
        two
        three
        """
      And the buffer is unchanged

    Scenario: Moving right on an empty line leaves no whitespace behind
      Given "src/empty.js" is open in the editor holding nothing
      And the editor mode is insert
      When I press the Right arrow in the editor
      Then the cursor is at line 1 column 1
      And the buffer is unchanged

  Rule: Alt+Backspace deletes the word behind the cursor

    Insert mode had no way back over a mistyped identifier but one press per
    character. Alt+Backspace takes the word behind the cursor instead, as one
    edit, so `u` costs the one key the delete cost.

    The modifier is affordable because it is not the only route to the delete:
    `db` — covered in "More of vim" — reaches it with no modifier held, which
    the contract requires, since Option is not Alt on macOS unless the terminal
    is told to make it so and a stock multiplexer strips the extended key
    reports. That is also why the key is unclaimed in normal mode: `d` and `b`
    are the operator there, and one gesture must not mean two things in the one
    mode where both are reachable.

    The two agree on the word behind the cursor and part company at a line's
    edge, deliberately. `db` composes with `b`, which reaches back across the
    break; this is bounded to the line. So on an indented line it takes the
    indentation rather than the indentation *and* the word above it, and at the
    start of a line — where there is nothing behind on it at all — it degrades
    to the join Backspace already does rather than deleting backwards into the
    line above.

    Scenario: Alt+Backspace deletes the word behind the cursor
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And the cursor is at line 1 column 9
      When I press "Alt+Backspace" in the editor
      Then the buffer holds:
        """
        one three
        """
      And the cursor is at line 1 column 5

    Scenario: Pressing it again takes the word before that
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And the cursor is at line 1 column 9
      And I press "Alt+Backspace" in the editor
      When I press "Alt+Backspace" in the editor
      Then the buffer holds:
        """
        three
        """
      And the cursor is at line 1 column 1

    Scenario: Inside a word it deletes back to that word's start only
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And the cursor is at line 1 column 7
      When I press "Alt+Backspace" in the editor
      Then the buffer holds:
        """
        one o three
        """
      And the cursor is at line 1 column 5

    # The docstring holds one line at its own margin so the indented one keeps
    # its spaces: gherkin dedents by the shortest non-blank line.
    Scenario: With only whitespace behind it, the whitespace is what goes
      Given "src/words.js" is open in the editor holding:
        """
        one
            two
        """
      And the editor mode is insert
      And the cursor is at line 2 column 5
      When I press "Alt+Backspace" in the editor
      Then the buffer holds:
        """
        one
        two
        """
      And the cursor is at line 2 column 1

    Scenario: At the start of a line it joins with the line above
      Given "src/words.js" is open in the editor holding:
        """
        one two
        three
        """
      And the editor mode is insert
      And the cursor is at line 2 column 1
      When I press "Alt+Backspace" in the editor
      Then the buffer holds:
        """
        one twothree
        """
      And the cursor is at line 1 column 8

    # Twice, so the assertion can fail: one word back after one undo is what
    # tells a delete that undid whole apart from one that never happened.
    Scenario: A word delete undoes in one key
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And the cursor is at line 1 column 9
      And I press "Alt+Backspace" in the editor
      And I press "Alt+Backspace" in the editor
      And I press "Escape" in the editor
      When I type "u" in the editor
      Then the buffer holds:
        """
        one three
        """

    Scenario: A bare Backspace still takes one character
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And the cursor is at line 1 column 9
      When I press "Backspace" in the editor
      Then the buffer holds:
        """
        one twothree
        """

    Scenario: In normal mode Alt+Backspace is not a word delete
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the cursor is at line 1 column 9
      When I press "Alt+Backspace" in the editor
      Then the buffer holds:
        """
        one twothree
        """

  Rule: Typing over picked characters replaces them

    While inserting, a charwise selection names the text the next keystroke stands
    in for: the picked characters go and the typed one takes their place, which is
    what every editor does and what makes selecting a word a way to rewrite it. A
    key that is also a normal-mode operator is still a typed character here, and
    an opening bracket is the one exception — it wraps what was picked instead.

    Scenario: Typing while characters are picked replaces them
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And I hold shift and press the Right arrow in the editor 2 times
      When I type "X" in the editor
      Then the buffer holds:
        """
        X two three
        """
      And the selection holds nothing
      And the cursor is at line 1 column 2

    Scenario: A letter that is also an operator types over what is picked
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And I hold shift and press the Right arrow in the editor 2 times
      When I type "d" in the editor
      Then the buffer holds:
        """
        d two three
        """

    Scenario: Backspace while characters are picked deletes them
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And I hold shift and press the Right arrow in the editor 3 times
      When I press "Backspace" in the editor
      Then the buffer holds:
        """
        two three
        """
      And the selection holds nothing
      And the cursor is at line 1 column 1

    Scenario: Backspace over what is picked in normal mode is unchanged
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the cursor is at line 1 column 5
      And I hold shift and press the Right arrow in the editor 2 times
      When I press "Backspace" in the editor
      Then the buffer holds:
        """
        one to three
        """

    Scenario: Replacing what is picked undoes in one key
      Given "src/words.js" is open in the editor holding:
        """
        one two three
        """
      And the editor mode is insert
      And I hold shift and press the Right arrow in the editor 2 times
      And I type "X" in the editor
      When I press "Escape" in the editor
      And I press "u" in the editor
      Then the buffer holds:
        """
        one two three
        """
