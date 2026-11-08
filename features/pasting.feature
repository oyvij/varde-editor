Feature: Pasting from the clipboard

  Copying and pasting are one gesture with two keys, and each key has two
  spellings: Ctrl and Command are aliases on both, so whichever hand's habit
  copies also pastes. Command+V worked long before Ctrl+V did, because the host
  terminal answers Command+V itself and sends Varde the text as a paste — which
  made copying and pasting feel like two unrelated features.

  The clipboard is the edge's to read: the core asks for it and the text comes
  back as the one edit a paste is. `y` and `p` stay the register's, which is a
  separate world on purpose — `selection.feature` says why.

  Background:
    Given the workspace root is "/home/me/projects/varde"

  Scenario Outline: Either spelling of paste puts the clipboard in the buffer
    Given a system clipboard is available
    And the clipboard holds "pasted "
    And "src/tree.js" is open in the editor holding:
      """
      one two
      """
    When I press "<key>" in the editor
    Then the buffer holds:
      """
      pasted one two
      """

    Examples:
      | key    |
      | Ctrl+v |
      | Cmd+v  |

  Scenario Outline: Either spelling of copy takes the selection
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      run("unquoted path")
      """
    And I drag across "unquoted path" in the editor pane
    When I press "<key>" in the editor
    Then the clipboard holds "unquoted path"

    Examples:
      | key    |
      | Ctrl+c |
      | Cmd+c  |

  Scenario: Pasting what was just copied needs no other application
    Given a system clipboard is available
    And "src/tree.js" is open in the editor holding:
      """
      one two
      """
    And I drag across "one" in the editor pane
    And I press "Ctrl+c" in the editor
    And I press "Escape" in the editor
    When I press "Ctrl+v" in the editor
    Then the buffer holds:
      """
      oneone two
      """

  Scenario: Pasting into a preview is refused rather than editing it unseen
    Given a system clipboard is available
    And the clipboard holds "pasted"
    And "README.md" is open in the editor holding:
      """
      # Setup
      """
    When I press "Ctrl+v" in the editor
    Then the buffer is unchanged
    And the editor refuses with "read-only-preview"

  Scenario: A paste replaces a selection made in normal mode
    Given the clipboard holds "four"
    And "src/tree.js" is open in the editor holding:
      """
      one two
      """
    And I press "W" in the editor
    When I press "Ctrl+v" in the editor
    Then the buffer holds:
      """
      four two
      """
    And the selection holds nothing
    And the editor mode is normal

  Scenario: A paste replaces a selection made in insert mode
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      """
    And the editor mode is insert
    And I hold shift and press the Right arrow in the editor 2 times
    When I paste "four" into the editor
    Then the buffer holds:
      """
      four two
      """
    And the selection holds nothing
    And the editor mode is insert

  Scenario: A paste after a single Ctrl+D with no other match replaces the word
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      """
    And I press "Ctrl+d" in the editor
    When I paste "four" into the editor
    Then the buffer holds:
      """
      four two
      """

  Scenario: One undo brings back the text a paste replaced
    Given "src/tree.js" is open in the editor holding:
      """
      one two
      """
    And I press "W" in the editor
    And I paste "four" into the editor
    When I press "u" in the editor
    Then the buffer holds:
      """
      one two
      """
