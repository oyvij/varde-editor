Feature: The minimap

  A far-off mirror of the file down the editor's right-hand edge: two lines to a
  row and four source columns to a cell, so the shape of the file — its blocks,
  its comment runs, where the code stops — is there without reading it.

  Press and drag in the strip to travel: the slider comes to rest under the row
  the pointer is holding, and the caret comes with the view, the way the wheel
  already takes it — a cursor left off screen is one you have lost.

  A file too tall for the strip scrolls the strip as well, in proportion, so the
  window the editor is showing is always somewhere inside the mirror.

  The columns it takes are the editor's, so turning it off gives them back to
  the text.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the screen is 30 rows by 120 columns

  Scenario: A file short enough to fit is mirrored whole
    Given "src/main.rs" is open in the editor with 20 lines
    Then the minimap mirrors lines 1 through 20

  Scenario: A file too tall for the strip is mirrored from the top down
    Given "src/main.rs" is open in the editor with 200 lines
    Then the minimap mirrors lines 1 through 38

  Scenario: Reaching the end of the file scrolls the mirror to the end too
    Given "src/main.rs" is open in the editor with 200 lines
    When I press "G" in the editor
    Then the editor view starts at line 182
    And the minimap mirrors lines 163 through 200

  Scenario: Dragging in the mirror travels the file
    Given "src/main.rs" is open in the editor with 200 lines
    When I drag the minimap to row 10
    Then the editor view starts at line 96
    And the cursor is on line 96

  # The mirror scrolls as the window travels, so a rule that read the line drawn
  # under the pointer afresh on every report had the file running away from a
  # pointer standing still.
  Scenario: Holding the pointer still leaves the file still
    Given "src/main.rs" is open in the editor with 200 lines
    And I drag the minimap to row 10
    When I drag the minimap to row 10
    Then the editor view starts at line 96

  # The line is quiet until somebody reaches for it: under the pointer is the
  # one moment it has something to say, and it is the gesture's feedback too —
  # a drag arrives as drags rather than moves, so the hand that pressed the
  # strip keeps it lit for as long as it holds it.
  Scenario: The slider lights up under the pointer
    Given "src/main.rs" is open in the editor with 200 lines
    When I move the pointer onto the minimap
    Then the minimap slider is lit
    When I move the pointer into the editor's text
    Then the minimap slider is quiet

  # The load-bearing absence: the strip's columns belong to the editor pane, so
  # a press in them used to be a press in the text — every travel would have
  # picked a character and left a selection nobody asked for.
  Scenario: Dragging in the mirror picks no text
    Given "src/main.rs" is open in the editor with 200 lines
    When I drag the minimap to row 10
    Then the selection holds nothing

  Scenario: Turning the mirror off gives its columns back to the text
    Given "src/main.rs" is open in the editor with 20 lines
    When I run ":minimap" in the editor
    Then the minimap is hidden
    And the editor shows 42 columns of text

  Scenario: The editor keeps room for the mirror while it is showing
    Given "src/main.rs" is open in the editor with 20 lines
    Then the editor shows 30 columns of text

  # A Preview's rows are not the file's lines, so there is nothing a mirror of
  # lines could be a mirror of — the same refusal a Preview gets from every other
  # surface that speaks in lines. A diff and a walked Site are the other two.
  Scenario: A Preview has no lines to mirror
    Given "README.md" is open in the editor holding:
      """
      # Title

      Some prose.
      """
    Then the editor is showing preview
    And the minimap is hidden

  # Remembered, like the darker field: a reading preference
  # is a property of the project you read in, not of the session.
  Scenario: Turning the mirror off is remembered
    Given "src/main.rs" is open in the editor with 20 lines
    When I run ":minimap" in the editor
    Then the project state was saved

  # Twelve columns are worth spending on a mirror beside code and not instead of
  # it: a stock tree divider on a hundred-column terminal leaves the editor forty
  # columns, and the mirror would take the last of the room the code is read in.
  Scenario: A pane with no room for the mirror does without it
    Given the screen is 26 rows by 100 columns
    And "src/main.rs" is open in the editor with 200 lines
    Then the minimap is hidden
    And the editor shows 28 columns of text

  Scenario: A project that asks for no mirror starts without one
    Given the project config is:
      """
      [editor]
      minimap = false
      """
    When Varde starts in the project
    Then the minimap is hidden
