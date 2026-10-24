Feature: File tree actions run commands in the terminal

  The tree never touches the filesystem itself. Every action becomes a shell command, so
  the terminal is the one place a file is made or removed and the user can see exactly what
  was run.

  An action that already names everything it needs — new file, new directory, delete, back
  to project root — runs. Only "go here" is left waiting for an enter, because a cd is the
  start of whatever the user meant to type next, not the whole of it.

  Actions that need a name — new file, new directory — collect it in a small prompt box
  first, so what reaches the terminal is a complete, runnable command rather than a
  half-typed one. A name may be a relative path, in which case missing parent folders are
  created by the same command.

  Injected paths are always absolute, so an action is correct no matter where the shell
  has been cd'd to, and are quoted only when the path needs it.

  Focus follows what is left to do. A command still waiting for an enter takes the
  terminal, because that enter has to land in the same pane. A command that ran leaves
  nothing to type there, so focus goes to the tree and the row it was about is selected —
  the new file is one enter away from the editor, and a delete leaves the selection on the
  folder that held it rather than on a path that is gone. Opening the name box moves focus
  nowhere: the name is still being typed.

  A created file can only be seen in a folder the tree has read, so creating opens that
  folder. The file itself lands when the watcher notices it — the selection names the path
  and waits for the row.

  A delete moves the item into this session's trash under ~/.varde rather than removing it,
  so the command is a mkdir of the trash and a mv into it. Each item gets a number of its
  own, so two items with the same name never meet there. The delete becomes undoable only
  once the tree's listing shows the path gone: a mv that never landed is nothing to undo.
  Quitting empties the trash, and nothing is ever written inside the project.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the session's trash is "/home/me/.varde/trash/%home%me%projects%varde-4242"
    And the terminal input is empty

  Scenario: Going to a folder
    Given the file tree shows the folder "src/tree"
    When I trigger "go here" on that folder
    Then the terminal input is "cd /home/me/projects/varde/src/tree"
    And no command has been executed

  Scenario: Creating a new file asks for a name first
    Given the file tree shows the folder "src"
    When I trigger "new file" on that folder
    Then the name box is shown
    And the terminal input is empty

  Scenario: Confirming a name runs the complete command
    Given the file tree shows the folder "src"
    And I trigger "new file" on that folder
    When I enter the name "tree.js"
    Then the name box is not shown
    And the terminal has executed "touch /home/me/projects/varde/src/tree.js"
    And the terminal input is empty

  Scenario: A nested name creates the missing parent folders
    Given the file tree shows the folder "src"
    And I trigger "new file" on that folder
    When I enter the name "tree/index.js"
    Then the terminal has executed "mkdir -p /home/me/projects/varde/src/tree && touch /home/me/projects/varde/src/tree/index.js"

  Scenario: Creating a new directory
    Given the file tree shows the folder "src"
    And I trigger "new directory" on that folder
    When I enter the name "tree"
    Then the terminal has executed "mkdir -p /home/me/projects/varde/src/tree"

  Scenario: Cancelling the name box runs nothing
    Given the file tree shows the folder "src"
    And I trigger "new file" on that folder
    When I press "Escape"
    Then the name box is not shown
    And the terminal input is empty

  Scenario: A name that needs quoting is quoted
    Given the file tree shows the folder "src"
    And I trigger "new file" on that folder
    When I enter the name "my notes.md"
    Then the terminal has executed "touch '/home/me/projects/varde/src/my notes.md'"

  Scenario: Deleting a file moves it into the session's trash
    Given the file tree shows the file "src/landing.js"
    When I trigger "delete" on that file
    Then the terminal has executed "mkdir -p '/home/me/.varde/trash/%home%me%projects%varde-4242' && mv /home/me/projects/varde/src/landing.js '/home/me/.varde/trash/%home%me%projects%varde-4242/1-landing.js'"
    And the terminal input is empty

  Scenario: Deleting a folder moves it whole with the same mv
    Given the file tree shows the folder "src/tree"
    When I trigger "delete" on that folder
    Then the terminal has executed "mkdir -p '/home/me/.varde/trash/%home%me%projects%varde-4242' && mv /home/me/projects/varde/src/tree '/home/me/.varde/trash/%home%me%projects%varde-4242/1-tree'"

  Scenario: Two items with the same name get their own places in the trash
    Given the file tree shows the file "src/index.js"
    And I trigger "delete" on that file
    And the file tree shows the file "lib/index.js"
    When I trigger "delete" on that file
    Then the terminal last executed "mkdir -p '/home/me/.varde/trash/%home%me%projects%varde-4242' && mv /home/me/projects/varde/lib/index.js '/home/me/.varde/trash/%home%me%projects%varde-4242/2-index.js'"

  Scenario: A delete is not undoable until the tree sees the path gone
    Given the file tree shows the file "src/landing.js"
    When I trigger "delete" on that file
    Then the undo list is empty
    And the reviewer is not told "moved-to-trash"

  Scenario: A delete the tree sees land is undoable and says so
    Given the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    When "src/landing.js" is deleted on disk
    Then the undo list holds "src/landing.js"
    And the reviewer is told "moved-to-trash"

  Scenario: Another path going does not land a delete
    Given the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    When "src/other.js" is deleted on disk
    Then the undo list is empty
    And the reviewer is not told "moved-to-trash"

  Scenario: A delete writes nothing inside the project
    Given the project has a ".gitignore"
    And the file tree shows the folder "src/tree"
    And I trigger "delete" on that folder
    When "src/tree" is deleted on disk
    Then the project ".gitignore" is unchanged
    And no file was written inside the workspace root

  Scenario Outline: A deleted path is quoted when it needs it
    Given the file tree shows the file "<path>"
    When I trigger "delete" on that file
    Then the terminal has executed:
      """
      mkdir -p '/home/me/.varde/trash/%home%me%projects%varde-4242' && mv <from> <to>
      """

    Examples:
      | path     | from                               | to                                                              |
      | my notes | '/home/me/projects/varde/my notes' | '/home/me/.varde/trash/%home%me%projects%varde-4242/1-my notes' |
      | don't    | "/home/me/projects/varde/don't"    | "/home/me/.varde/trash/%home%me%projects%varde-4242/1-don't"    |
      | a;b      | '/home/me/projects/varde/a;b'      | '/home/me/.varde/trash/%home%me%projects%varde-4242/1-a;b'      |

  Scenario: Quitting empties the session's trash
    Given Varde started in the project
    When I quit
    Then the session's trash was deleted

  Scenario: u in the tree restores the newest delete
    Given the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    And "src/landing.js" is deleted on disk
    When I press the key "u"
    Then the terminal last executed "mkdir -p /home/me/projects/varde/src && mv -n '/home/me/.varde/trash/%home%me%projects%varde-4242/1-landing.js' /home/me/projects/varde/src/landing.js"

  Scenario: Once the restored path is back, u restores the delete before it
    Given the file tree shows the file "src/index.js"
    And I trigger "delete" on that file
    And "src/index.js" is deleted on disk
    And the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    And "src/landing.js" is deleted on disk
    And I press the key "u"
    And "src/landing.js" is created on disk
    When I press the key "u"
    Then the terminal last executed "mkdir -p /home/me/projects/varde/src && mv -n '/home/me/.varde/trash/%home%me%projects%varde-4242/1-index.js' /home/me/projects/varde/src/index.js"

  Scenario: u with nothing to undo runs nothing and says so
    Given the file tree pane has focus
    When I press the key "u"
    Then no command has been executed
    And the reviewer is told "nothing-to-undo"

  Scenario: u never overwrites a path that is taken again
    Given the folder "src" is expanded
    And the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    And "src/landing.js" is deleted on disk
    And "src/landing.js" is created on disk
    When I press the key "u"
    Then the terminal has executed 1 command
    And the reviewer is told "restore-path-taken"

  Scenario: u while a restore is still landing runs nothing more
    Given the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    And "src/landing.js" is deleted on disk
    And I press the key "u"
    When I press the key "u"
    Then the terminal last executed "mkdir -p /home/me/projects/varde/src && mv -n '/home/me/.varde/trash/%home%me%projects%varde-4242/1-landing.js' /home/me/projects/varde/src/landing.js"
    And the terminal has executed 2 commands
    And the reviewer is told "restore-pending"

  Scenario: A file whose folder is gone comes back with the folder made again
    Given the file tree shows the file "src/tree/node.js"
    And I trigger "delete" on that file
    And "src/tree/node.js" is deleted on disk
    And "src/tree" is deleted on disk
    When I press the key "u"
    Then the terminal last executed "mkdir -p /home/me/projects/varde/src/tree && mv -n '/home/me/.varde/trash/%home%me%projects%varde-4242/1-node.js' /home/me/projects/varde/src/tree/node.js"

  Scenario Outline: A restored path is quoted when it needs it
    Given the file tree shows the file "<path>"
    And I trigger "delete" on that file
    And "<path>" is deleted on disk
    When I press the key "u"
    Then the terminal last executed:
      """
      mkdir -p /home/me/projects/varde && mv -n <from> <to>
      """

    Examples:
      | path     | from                                                            | to                                 |
      | my notes | '/home/me/.varde/trash/%home%me%projects%varde-4242/1-my notes' | '/home/me/projects/varde/my notes' |
      | don't    | "/home/me/.varde/trash/%home%me%projects%varde-4242/1-don't"    | "/home/me/projects/varde/don't"    |
      | a;b      | '/home/me/.varde/trash/%home%me%projects%varde-4242/1-a;b'      | '/home/me/projects/varde/a;b'      |

  Scenario: The Cheatsheet lists u for the tree
    Given the Cheatsheet is shown
    Then the Cheatsheet lists "tree: undo the last delete" under the keys "u"

  Scenario: Undo has no palette entry
    When I open the palette
    Then the palette offers no entry "Undo"

  Scenario: Clicking the undo Chip restores the newest delete
    Given the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    And "src/landing.js" is deleted on disk
    When I click the "undo" Chip
    Then the terminal last executed "mkdir -p /home/me/projects/varde/src && mv -n '/home/me/.varde/trash/%home%me%projects%varde-4242/1-landing.js' /home/me/projects/varde/src/landing.js"

  Scenario: Clicking the undo Chip with nothing to undo runs nothing and says so
    When I click the "undo" Chip
    Then no command has been executed
    And the reviewer is told "nothing-to-undo"

  Scenario: The undo Chip is dimmed while there is nothing to undo
    Then the "undo" Chip is dimmed

  Scenario: The undo Chip is lit once a delete is recorded
    Given the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    When "src/landing.js" is deleted on disk
    Then the "undo" Chip is lit

  Scenario: The undo Chip is dimmed again once the last delete is restored
    Given the file tree shows the file "src/landing.js"
    And I trigger "delete" on that file
    And "src/landing.js" is deleted on disk
    And I click the "undo" Chip
    When "src/landing.js" is created on disk
    Then the "undo" Chip is dimmed

  Scenario: Copying a path puts the absolute path on the clipboard
    Given a system clipboard is available
    And the file tree shows the file "src/landing.js"
    When I trigger "copy path" on that file
    Then the clipboard holds "/home/me/projects/varde/src/landing.js"
    And the terminal input is empty
    And no command has been executed

  Scenario: A folder's path is copied the same way
    Given a system clipboard is available
    And the file tree shows the folder "src/tree"
    When I trigger "copy path" on that folder
    Then the clipboard holds "/home/me/projects/varde/src/tree"

  Scenario: Injecting replaces text the user had already typed
    Given the terminal input is "git stat"
    And the file tree shows the folder "src"
    When I trigger "go here" on that folder
    Then the terminal input is "cd /home/me/projects/varde/src"
    And no command has been executed

  Scenario Outline: Paths are quoted only when they need it
    Given the file tree shows the folder "<path>"
    When I trigger "go here" on that folder
    Then the terminal input is:
      """
      <command>
      """

    Examples:
      | path     | command                               |
      | src      | cd /home/me/projects/varde/src         |
      | my notes | cd '/home/me/projects/varde/my notes'  |
      | don't    | cd "/home/me/projects/varde/don't"     |
      | a;b      | cd '/home/me/projects/varde/a;b'       |

  Scenario: Returning to the project root is the one action that runs
    Given the terminal is in "/home/me/projects/varde/src/tree"
    When I trigger "back to project root"
    Then the terminal has executed "cd /home/me/projects/varde"
    And the terminal input is empty

  Scenario: Injecting a command focuses the terminal
    Given the editor pane has focus
    And the file tree shows the folder "src"
    When I trigger "go here" on that folder
    Then the terminal pane has focus

  Scenario: A created file is selected in the tree, ready to open
    Given the editor pane has focus
    And the file tree shows the folder "src"
    And I trigger "new file" on that folder
    When I enter the name "tree.js"
    Then the file tree pane has focus
    And the tree selection is "src/tree.js"

  Scenario: The folder a new file lands in is opened, so the file can show up
    Given the folder "src" is collapsed
    And the file tree shows the folder "src"
    And I trigger "new file" on that folder
    When I enter the name "tree.js"
    Then "src" is expanded

  Scenario: A new directory is selected in the tree
    Given the file tree shows the folder "src"
    And I trigger "new directory" on that folder
    When I enter the name "tree"
    Then the file tree pane has focus
    And the tree selection is "src/tree"

  Scenario: Deleting leaves the selection on the folder that held the file
    Given the editor pane has focus
    And the file tree shows the file "src/landing.js"
    When I trigger "delete" on that file
    Then the file tree pane has focus
    And the tree selection is "src"

  Scenario: Asking for a name leaves focus alone
    Given the editor pane has focus
    And the file tree shows the folder "src"
    When I trigger "new file" on that folder
    Then the editor pane still has focus
