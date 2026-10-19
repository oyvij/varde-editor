Feature: Reaching the tree actions

  The tree actions exist and are specified in detail, but a user has to be able
  to start them. The focused row carries icons for what makes sense on it, and
  the same actions have keyboard shortcuts so the tree is usable either way.

  A folder can take a new file, a new directory, a cd, a search inside it, or a
  delete. A file can only be deleted. Every row, whatever its kind, can have its
  absolute path copied to the clipboard.

  Two of the keys need no row at all, because they are about the tree rather than
  a path in it: one returns the terminal to the project root, the other closes
  every folder the tree has open.

  The keys always have somewhere to put a new file, because a project without one
  selectable folder still has its root: the selected folder, the folder holding a
  selected file, or the project root when nothing is selected.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the file tree shows the collapsed folder "src"
    And "src" contains "tree.js"
    And the file tree pane has focus

  Scenario: A focused folder offers every action that applies to it
    Given the tree selection is "src"
    Then the row actions offered are:
      | new-file      |
      | new-directory |
      | go-here       |
      | search-here   |
      | delete        |
      | copy-path     |

  Scenario: A focused file offers delete and copy-path
    Given the folder "src" is expanded
    And the tree selection is "src/tree.js"
    Then the row actions offered are:
      | delete    |
      | copy-path |

  Scenario: A focused page offers to open it externally first
    Given "index.html" is on disk under collapsed folders
    And the tree selection is "index.html"
    Then the row actions offered are:
      | open-externally |
      | delete          |
      | copy-path       |

  Scenario: A focused source file is not offered to open externally
    Given "main.rs" is on disk under collapsed folders
    And the tree selection is "main.rs"
    Then the row actions offered are:
      | delete    |
      | copy-path |

  Scenario: Clicking the open-externally icon opens the file in its default app
    Given "index.html" is on disk under collapsed folders
    And the tree selection is "index.html"
    When I click the "open-externally" action on "index.html"
    Then the browser opens "file:///home/me/projects/varde/index.html"

  Scenario: A file name with a space still opens externally
    Given "my page.html" is on disk under collapsed folders
    And the tree selection is "my page.html"
    When I click the "open-externally" action on "my page.html"
    Then the browser opens "file:///home/me/projects/varde/my%20page.html"

  Scenario: o opens the selected image externally
    Given "shot.png" is on disk under collapsed folders
    And the tree selection is "shot.png"
    When I press "o"
    Then the browser opens "file:///home/me/projects/varde/shot.png"

  Scenario: o on a source file opens nothing
    Given "main.rs" is on disk under collapsed folders
    And the tree selection is "main.rs"
    When I press "o"
    Then the browser opens nothing

  Scenario: Rows that are not focused offer nothing
    Given the tree selection is "src"
    Then the row "src/tree.js" offers no actions

  Scenario: Clicking the delete icon runs the delete command
    Given the tree selection is "src"
    When I click the "delete" action on "src"
    Then the terminal has executed "rm -r /home/me/projects/varde/src"

  Scenario: Clicking the search icon searches inside that folder only
    Given the tree selection is "src"
    When I click the "search-here" action on "src"
    Then the search is open
    And the search is scoped to "src"

  Scenario: Clicking the new-file icon opens the name box
    Given the tree selection is "src"
    When I click the "new-file" action on "src"
    Then the name box is shown

  Scenario: n starts a new file in the selected folder
    Given the tree selection is "src"
    When I press "n"
    Then the name box is shown

  Scenario: Shift-N starts a new directory
    Given the tree selection is "src"
    When I press "N"
    And I enter the name "tree"
    Then the terminal has executed "mkdir -p /home/me/projects/varde/src/tree"

  Scenario: d deletes the selected row
    Given the folder "src" is expanded
    And the tree selection is "src/tree.js"
    When I press "d"
    Then the terminal has executed "rm /home/me/projects/varde/src/tree.js"

  Scenario: Right steps into the row's actions
    Given the tree selection is "src"
    When I press "Right"
    Then the selected action is "new-file"

  Scenario: Right again moves along the actions
    Given the tree selection is "src"
    And I press "Right"
    When I press "Right"
    Then the selected action is "new-directory"

  Scenario: The action selection stops at the last one
    Given the tree selection is "src"
    And I press "Right"
    And I press "Right"
    And I press "Right"
    And I press "Right"
    And I press "Right"
    When I press "Right"
    Then the selected action is "copy-path"

  Scenario: Left steps back out to the row
    Given the tree selection is "src"
    And I press "Right"
    When I press "Left"
    Then no action is selected

  Scenario: Left moves back along the actions first
    Given the tree selection is "src"
    And I press "Right"
    And I press "Right"
    When I press "Left"
    Then the selected action is "new-file"

  Scenario: Enter runs the selected action
    Given the tree selection is "src"
    And I press "Right"
    And I press "Right"
    And I press "Right"
    When I press "Enter"
    Then the terminal input is "cd /home/me/projects/varde/src"

  Scenario: Enter on the first action asks for a name
    Given the tree selection is "src"
    And I press "Right"
    When I press "Enter"
    Then the name box is shown

  Scenario: Moving to another row leaves the actions
    Given the tree selection is "src"
    And I press "Right"
    When I press "Down"
    Then no action is selected

  Scenario: Escape leaves the actions
    Given the tree selection is "src"
    And I press "Right"
    When I press "Escape"
    Then no action is selected

  Scenario: A file's first action is delete
    Given the folder "src" is expanded
    And the tree selection is "src/tree.js"
    When I press "Right"
    Then the selected action is "delete"

  Scenario: Clicking the copy-path icon copies the absolute path
    Given a system clipboard is available
    And the tree selection is "src"
    When I click the "copy-path" action on "src"
    Then the clipboard holds "/home/me/projects/varde/src"

  Scenario: A row with no actions cannot be stepped into
    Given the tree selection is "src"
    And the file tree pane does not have focus
    When I press "Right"
    Then no action is selected

  Scenario: Dash returns the terminal to the project root
    When I press "-"
    Then the terminal has executed "cd /home/me/projects/varde"

  Scenario: C collapses every open folder
    Given the folder "src" is expanded
    When I press "c"
    Then "src" is collapsed

  Scenario: A new file goes into the selected folder, not its parent
    Given the folder "src" is expanded
    And the tree selection is "src"
    When I press "n"
    And I enter the name "new.js"
    Then the terminal has executed "touch /home/me/projects/varde/src/new.js"

  Scenario: With no row selected, a new file lands in the project root
    Given no row is selected in the tree
    When I press "n"
    And I enter the name "notes.md"
    Then the terminal has executed "touch /home/me/projects/varde/notes.md"

  Scenario: With no row selected, a new directory lands in the project root
    Given no row is selected in the tree
    When I press "N"
    And I enter the name "docs"
    Then the terminal has executed "mkdir -p /home/me/projects/varde/docs"

  Scenario: A new file started on a file lands in that file's folder
    Given the folder "src" is expanded
    And the tree selection is "src/tree.js"
    When I press "n"
    And I enter the name "helper.js"
    Then the terminal has executed "touch /home/me/projects/varde/src/helper.js"
