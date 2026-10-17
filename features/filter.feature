Feature: Filtering the file tree

  A box at the foot of the tree pane narrows it as you type. Matching is fuzzy
  and ranked, so "ftr" finds "file_tree.rs" — the characters must appear in
  order, not next to each other, and the closest match sorts first.

  Fuzzy is the fallback, not the rule: once any path contains what was typed
  literally, only those are shown. Typing a whole filename is asking for that
  file, and the paths that merely spell its letters in order are noise.

  Only files match. Folders appear when they lead to a match, and are shown
  open so the match is visible, because a match you cannot see has not helped
  you — but the tree itself is left as it was found, so clearing the filter
  does not leave every folder a match sat under standing open.

  The box completes to the best match as you type. Enter marks that match in
  the tree and opens nothing: narrowing the tree is looking for a file, and
  what to do with it once found is the reader's to say. The arrows step from
  the best match through the others in rank order, so the second-best file is
  one key away rather than a reach for the mouse, and typing again starts over
  from the best match because the ranking it stepped through is gone. The tree
  highlights the match Enter would mark, and stepping is not choosing: it
  opens nothing, and until Enter the tree's own selection stays where it was.

  The box holds the keyboard only while you type in it. A filter stays on the
  tree while another pane has the keyboard, so the narrowed tree is a map while
  you work elsewhere, and what you type there goes there. Coming back to the
  tree lands in the tree, never in the box, so returning cannot type into the
  filter by accident; "/" goes back into the box with the query intact. An
  empty box closes when the keyboard leaves it. Escape, in the box or in the
  filtered tree, removes the filter, keeps the selection and opens the folders
  leading to it.

  Opening the filter walks the project afresh, in the background, so typing
  never waits for the walk. Matches appear as the walk finds them. The filter
  searches the project as it was when the filter opened: a file created since
  the last filter is a file it has to find, and a file created while it is
  open waits for the next one.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project contains:
      | src/main.rs        |
      | src/file_tree.rs   |
      | src/tree_actions.rs |
      | features/tree.feature |
      | README.md          |

  Scenario: An empty filter shows the tree as it was
    When I filter by ""
    Then the tree is not filtered

  Scenario: Typing narrows to matching files
    When I filter by "tree"
    Then the filtered files are:
      | src/file_tree.rs      |
      | src/tree_actions.rs   |
      | features/tree.feature |

  Scenario: Characters need only appear in order
    When I filter by "ftr"
    Then the filtered files include "src/file_tree.rs"

  Scenario: A literal match hides the scattered ones
    When I filter by "tree.rs"
    Then the filtered files are:
      | src/file_tree.rs |

  Scenario: Fuzzy still stands alone when nothing matches literally
    When I filter by "ftr"
    Then the filtered files are:
      | src/file_tree.rs      |
      | features/tree.feature |

  Scenario: The closest match ranks first
    When I filter by "tree"
    Then the best match is "features/tree.feature"

  Scenario: Typing more narrows further
    Given I filter by "tree"
    When I filter by "treeac"
    Then the filtered files are:
      | src/tree_actions.rs |

  Scenario: Folders leading to a match are shown open
    When I filter by "main"
    Then the row "src" is expanded

  Scenario: Filtering leaves no folder open behind it
    Given I filter by "main"
    When I filter by ""
    Then "src" is collapsed

  Scenario: Folders that lead nowhere are not shown
    When I filter by "main"
    Then the filtered files are:
      | src/main.rs |

  Scenario: Nothing matches
    When I filter by "zzzz"
    Then the filtered files are empty
    And the tree filter state is "no-matches"

  Scenario: An unfiltered tree says so
    When I filter by ""
    Then the tree filter state is "unfiltered"

  Scenario: A filter with matches says so
    When I filter by "tree"
    Then the tree filter state is "matches"

  Scenario: The box completes to the best match
    When I filter by "tree"
    Then the completion is "features/tree.feature"

  Scenario: The completion narrows as you type
    When I filter by "treeac"
    Then the completion is "src/tree_actions.rs"

  Scenario: Enter marks the best match in the tree
    Given I filter by "treeac"
    When I accept the filter
    Then the tree selection is "/home/me/projects/varde/src/tree_actions.rs"
    And no file was opened in the editor

  Scenario: The best match is highlighted before any arrow
    When I filter by "tree"
    Then the highlighted row is "/home/me/projects/varde/features/tree.feature"

  Scenario: Down steps from the best match to the next
    Given I filter by "tree"
    When I step down through the matches
    Then the highlighted row is "/home/me/projects/varde/src/tree_actions.rs"
    And no file was opened in the editor

  Scenario: Up from the best match stays on it
    Given I filter by "tree"
    When I step up through the matches
    Then the highlighted row is "/home/me/projects/varde/features/tree.feature"

  Scenario: Stepping with nothing matching does nothing
    Given I filter by "zzzz"
    When I step down through the matches
    Then no row is highlighted

  Scenario: Clearing the filter after stepping leaves the selection where it was
    Given the tree selection is "README.md"
    And I filter by "tree"
    And I step down through the matches
    When I filter by ""
    Then the tree selection is "/home/me/projects/varde/README.md"

  Scenario: Enter marks the match the arrows reached
    Given I filter by "tree"
    And I step down through the matches
    When I accept the filter
    Then the tree selection is "/home/me/projects/varde/src/tree_actions.rs"
    And the tree is not filtered

  Scenario: Typing again starts over from the best match
    Given I filter by "tre"
    And I step down through the matches
    And I filter by "tree"
    When I accept the filter
    Then the tree selection is "/home/me/projects/varde/features/tree.feature"

  Scenario: Enter opens the folders the marked file lives in
    Given I filter by "treeac"
    When I accept the filter
    Then "src" is expanded

  Scenario: Enter leaves the tree unfiltered
    Given I filter by "treeac"
    When I accept the filter
    Then the tree is not filtered
    And the filter box is closed

  Scenario: Enter with nothing matching opens nothing and leaves no filter
    Given I filter by "zzzz"
    When I accept the filter
    Then no file was opened in the editor
    And the tree is not filtered

  Scenario: Clearing the filter restores the tree
    Given I filter by "tree"
    When I filter by ""
    Then the tree is not filtered

  Scenario: A file created since the last filter is still findable
    Given I filter by "main"
    And I filter by ""
    And the project gains "src/keys.rs"
    When I filter by "keys"
    Then the filtered files are:
      | src/keys.rs |

  Scenario: Typing in the editor leaves the filter as it was
    Given "README.md" is open in the editor holding nothing
    And I filter by "tree"
    And I click in the editor pane
    When I press the key "i"
    Then the editor mode is insert
    And the filter query is "tree"

  Scenario: Typing in the AI pane leaves the filter as it was
    Given an AI session is running in the AI pane
    And I filter by "tree"
    And I click in the AI pane
    When I press the key "x"
    Then the AI received "x"
    And the filter query is "tree"

  Scenario: Opening the palette takes the keyboard out of the box
    Given the terminal reports modifier key events
    And the double-tap window is 300 ms
    And I filter by "tree"
    When I press Ctrl and press Ctrl again after 100 ms
    Then the view palette is shown
    And the filter box does not have the keyboard
    And the filter query is "tree"

  Scenario: The tree stays filtered while another pane has the keyboard
    Given I filter by "main"
    When I click in the editor pane
    Then the tree shows "src/main.rs"
    And the tree does not show "README.md"

  Scenario: Coming back to a filtered tree lands in the list, not the box
    Given I filter by "tree"
    And I click in the editor pane
    And I click in the tree pane
    When I press the key "q"
    Then the filter query is "tree"
    And the filter box does not have the keyboard

  Scenario: Away from the box the tree highlights its selection, not the best match
    Given I filter by "tree"
    And the tree selection is "src/tree_actions.rs"
    When I click in the editor pane
    Then the highlighted row is "/home/me/projects/varde/src/tree_actions.rs"

  Scenario: The keyboard leaving an empty box closes it
    Given I filter by ""
    When I click in the editor pane
    Then the filter box is closed

  Scenario: Backspace to an empty query leaves the box open
    Given I filter by "t"
    When I press the key "Backspace"
    Then the tree is not filtered
    And the filter box has the keyboard

  Scenario: Slash in the filtered tree goes back to the box with the query intact
    Given I filter by "tree"
    And I click in the editor pane
    And I click in the tree pane
    When I press the key "/"
    Then the filter box has the keyboard
    And the filter query is "tree"

  Scenario: Escape in the box removes the filter and leaves the tree as it was
    Given the tree selection is "README.md"
    And I filter by "main"
    When I press the key "Escape"
    Then the tree is not filtered
    And the filter box is closed
    And the tree selection is "/home/me/projects/varde/README.md"
    And "src" is collapsed

  Scenario: Escape in the filtered tree removes the filter and opens the way to the selection
    Given I filter by "main"
    And I click in the editor pane
    And I click in the tree pane
    And the tree selection is "src/main.rs"
    When I press the key "Escape"
    Then the tree is not filtered
    And the tree selection is "/home/me/projects/varde/src/main.rs"
    And "src" is expanded

  Scenario: A file created while the filter is open waits for the next one
    Given I filter by "ke"
    And the project gains "src/keys.rs"
    When I filter by "keys"
    Then the filtered files are empty
