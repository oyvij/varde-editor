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

  The box completes to the best match as you type, and the tree highlights it.
  Enter, Down or Tab puts the tree's selection on that match, opens the
  folders it lives in and moves the keyboard into the filtered list, leaving
  the filter on. It opens nothing: narrowing the tree is looking for a file,
  and what to do with it once found is the reader's to say. With nothing
  matching they do nothing, so the query can be fixed rather than retyped.
  Up in the box does nothing. Going back into the box and typing again starts
  over from the best match.

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
    When I press the key "Enter"
    Then the tree selection is "/home/me/projects/varde/src/tree_actions.rs"
    And no file was opened in the editor

  Scenario: The best match is highlighted while you type
    When I filter by "tree"
    Then the highlighted row is "/home/me/projects/varde/features/tree.feature"

  Scenario: Enter opens the folders the marked file lives in
    Given I filter by "treeac"
    When I press the key "Enter"
    Then "src" is expanded

  Scenario: Enter leaves the tree filtered
    Given I filter by "treeac"
    When I press the key "Enter"
    Then the filter query is "treeac"
    And the tree filter state is "matches"
    And the filter box does not have the keyboard
    And the tree pane has focus

  Scenario Outline: <key> in the box lands on the best match in the filtered list
    Given the tree selection is "README.md"
    And I filter by "tree"
    When I press the key "<key>"
    Then the tree selection is "/home/me/projects/varde/features/tree.feature"
    And "features" is expanded
    And the filter query is "tree"
    And the filter box does not have the keyboard
    And no file was opened in the editor

    Examples:
      | key  |
      | Down |
      | Tab  |

  Scenario: Up in the box does nothing
    Given the tree selection is "README.md"
    And I filter by "tree"
    When I press the key "Up"
    Then the tree selection is "/home/me/projects/varde/README.md"
    And the filter query is "tree"
    And the filter box has the keyboard

  Scenario Outline: <key> with nothing matching does nothing
    Given the tree selection is "README.md"
    And I filter by "zzzz"
    When I press the key "<key>"
    Then no file was opened in the editor
    And the tree selection is "/home/me/projects/varde/README.md"
    And the filter query is "zzzz"
    And the filter box has the keyboard

    Examples:
      | key   |
      | Enter |
      | Down  |
      | Tab   |

  Scenario Outline: <key> in an empty box closes it and leaves the keyboard in the tree
    Given I filter by ""
    When I press the key "<key>"
    Then the filter box is closed
    And the tree pane has focus

    Examples:
      | key   |
      | Enter |
      | Down  |
      | Tab   |

  Scenario: Typing again starts over from the best match
    Given I filter by "tre"
    And I press the key "Enter"
    And the tree selection is "src/tree_actions.rs"
    And I press the key "/"
    And I press the key "e"
    When I press the key "Enter"
    Then the tree selection is "/home/me/projects/varde/features/tree.feature"

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
    Given the folder "src" is collapsed
    And the tree selection is "src/main.rs"
    And I filter by "main"
    When I press the key "Escape"
    Then the tree is not filtered
    And the filter box is closed
    And the tree selection is "/home/me/projects/varde/src/main.rs"
    And "src" is collapsed

  Scenario: Escape in an empty box closes it and leaves the tree as it was
    Given the folder "src" is collapsed
    And the tree selection is "src/main.rs"
    And I filter by ""
    When I press the key "Escape"
    Then the filter box is closed
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

  Scenario: j in the filtered list walks to the next visible row, folders included
    Given I filter by "tree"
    And I click in the editor pane
    And I click in the tree pane
    And the tree selection is "features/tree.feature"
    When I press the key "j"
    Then the tree selection is "/home/me/projects/varde/src"
    And the filter query is "tree"

  Scenario: Enter on a file in the filtered list opens it and the tree stays filtered
    Given the folder "src" is collapsed
    And I filter by "treeac"
    And I click in the editor pane
    And I click in the tree pane
    And the tree selection is "src/tree_actions.rs"
    When I press the key "Enter"
    Then "src/tree_actions.rs" was opened in the editor
    And the filter query is "treeac"

  Scenario: Enter on a folder row in the filtered list does nothing
    Given the folder "src" is expanded
    And I filter by "main"
    And I click in the editor pane
    And I click in the tree pane
    And the tree selection is "src"
    When I press the key "Enter"
    Then no file was opened in the editor
    And the filter query is "main"
    And "src" is expanded

  Scenario: A file created while the filter is open waits for the next one
    Given I filter by "ke"
    And the project gains "src/keys.rs"
    When I filter by "keys"
    Then the filtered files are empty
