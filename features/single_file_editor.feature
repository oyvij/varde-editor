Feature: The Single-file editor

  Naming a file rather than a folder opens Varde on that file alone: the editor, and nothing
  around it. It writes nothing but the file itself and remembers nothing between runs, so
  Varde's own state goes into a Sidecar keyed by the file's folder, deleted at exit, exactly
  as a Bare workspace's does. The file's folder is the root a Language server is given, not a
  workspace: there is no project config layer, no seeding, no Risk and no Update check.

  # Opening a file

  Scenario: Opening an existing file gives one Buffer holding it, in Edit view
    Given "/home/me/projects/theirs/notes.txt" is a file
    When Varde opens "/home/me/projects/theirs/notes.txt"
    Then Varde is the Single-file editor
    And the open buffers are:
      | notes.txt |
    And "notes.txt" is open in the editor
    And the current view is Edit

  Scenario: A file that cannot be read stops Varde
    Given the file "/home/me/projects/theirs/secret.txt" cannot be read
    When Varde opens "/home/me/projects/theirs/secret.txt"
    Then Varde refuses to start
    And the reason is "file-not-readable"

  Scenario: A read-only file opens normally
    Given "/home/me/projects/theirs/hosts" is a read-only file
    When Varde opens "/home/me/projects/theirs/hosts"
    Then Varde is the Single-file editor
    And "hosts" is open in the editor

  Scenario: A path that is neither a file nor a folder stops Varde
    Given "/home/me/projects/theirs/pipe" is neither a file nor a folder
    When Varde opens "/home/me/projects/theirs/pipe"
    Then Varde refuses to start
    And the reason is "not-a-folder"

  Scenario: The global config applies and a project config beside the file is ignored
    Given the global config is:
      """
      [editor]
      tab_width = 2
      """
    And the project config is:
      """
      [editor]
      tab_width = 8
      """
    And "/home/me/projects/theirs/notes.txt" is a file
    When Varde opens "/home/me/projects/theirs/notes.txt"
    Then the effective setting "editor.tab_width" is "2"

  Scenario: With no global config nothing is seeded and nothing is written next to the file
    Given there is no global config
    And "/home/me/projects/theirs/notes.txt" is a file
    When Varde opens "/home/me/projects/theirs/notes.txt"
    Then no config file was seeded
    And no file was written next to the file
    And no directory was created next to the file
    And the effective setting "editor.tab_width" is "4"

  Scenario: Varde's own directory is the Sidecar and nothing else is started
    Given "/home/me/projects/theirs/notes.txt" is a file
    When Varde opens "/home/me/projects/theirs/notes.txt"
    Then Varde's own directory is the Sidecar
    And no analysis was asked for
    And Varde does not ask for a Release
    And no shipped Skill was written

  Scenario: Quitting deletes the Sidecar and saves no state
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I quit
    Then Varde exits
    And the Sidecar was deleted
    And no state was saved

  # A new file

  Scenario: A missing file in an existing folder opens one empty Buffer and writes nothing
    Given "/home/me/projects/theirs/new.txt" does not exist but its folder does
    When Varde opens "/home/me/projects/theirs/new.txt"
    Then Varde is the Single-file editor
    And the open buffers are:
      | new.txt |
    And the buffer holds ""
    And no file was written next to the file

  Scenario: Saving the new Buffer writes the file at the named path
    Given "/home/me/projects/theirs/new.txt" does not exist but its folder does
    And Varde opened "/home/me/projects/theirs/new.txt"
    And the editor mode is insert
    And I type "hello" in the editor
    And the editor mode is normal
    When I run ":w"
    Then "new.txt" was written with:
      """
      hello
      """

  Scenario: Quitting without saving the new Buffer writes nothing
    Given "/home/me/projects/theirs/new.txt" does not exist but its folder does
    And Varde opened "/home/me/projects/theirs/new.txt"
    When I quit
    Then Varde exits
    And the Sidecar was deleted
    And no file was written next to the file
    And the file "new.txt" does not exist

  Scenario: A missing file whose folder is missing too stops Varde
    Given neither "/home/me/projects/nope/new.txt" nor its folder exists
    When Varde opens "/home/me/projects/nope/new.txt"
    Then Varde refuses to start
    And the reason is "no-such-folder"

  # The full-screen editor

  Scenario: The editor is the only pane and fills the screen
    Given "/home/me/projects/theirs/notes.txt" is a file
    When Varde opens "/home/me/projects/theirs/notes.txt"
    Then the editor is the only pane and fills the screen

  Scenario Outline: A focus move leaves the editor alone on the screen
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I press "<key>"
    Then the editor is the only pane and fills the screen
    And the editor pane has focus

    Examples:
      | key   |
      | Alt+h |
      | Alt+j |
      | Alt+l |

  Scenario Outline: A Command that would show the AI pane or the Strip leaves the layout unchanged
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I run "<command>"
    Then the editor is the only pane and fills the screen
    And the editor pane has focus
    And <absence>

    Examples:
      | command | absence                   |
      | :ai     | no AI session was started |
      | :split  | no shell is asked for     |

  Scenario Outline: Picking another pane from the palette leaves the editor focused
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I click the palette entry "<entry>"
    Then the editor is the only pane and fills the screen
    And the editor pane has focus

    Examples:
      | entry    |
      | Files    |
      | AI       |
      | Terminal |

  Scenario Outline: Asking for another View leaves the View as Edit
    Given the global config holds:
      """
      [knowledge]
      enabled = true
      """
    And "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I <ask>
    Then the current view is Edit
    And the editor is the only pane and fills the screen
    And the editor pane has focus

    Examples:
      | ask                              |
      | open Review view                 |
      | click the palette entry "Review" |
      | open Story view                  |
      | toggle the Knowledge view        |

  Scenario: A Diagnostic shows in the gutter on its line
    Given "/home/me/projects/theirs/lib.rs" is a file
    And Varde opened "/home/me/projects/theirs/lib.rs"
    And a language server for "rust" is ready
    When the language server for "rust" publishes diagnostics for "lib.rs":
      | line | severity | message           |
      | 3    | error    | cannot find value |
    Then the gutter mark on line 3 of "lib.rs" is "error"
    And the gutter has no mark on line 2 of "lib.rs"

  # The palette and the Command list

  Scenario: The palette offers Keys, Tools and Quit and no pane or View
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I open the palette
    Then the palette lists "Keys" under the key "h"
    And the palette lists "Tools" under the key "v"
    And the palette lists "Quit" under the key "q"
    And the palette does not list "Files"
    And the palette does not list "Terminal"
    And the palette does not list "Review"

  Scenario: Tools opens from the palette
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I open Tools
    Then the palette is listing tools

  Scenario: A palette key the Single-file editor doesn't offer does nothing
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    And I open the palette
    When I press "r" in the palette
    Then the current view is Edit
    And the view palette is shown

  Scenario: The Command list offers the Buffer's Commands and not the workspace's
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I open the command line
    Then the command list offers "format"
    And the command list offers "speed <rate>"
    And the command list does not offer "story"
    And the command list does not offer "ai"

  Scenario: A Command the Single-file editor doesn't offer does nothing when typed
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    And I open the command line and type "inject"
    When I press Enter in the command line
    Then the command line answered with no events

  # Closing

  Scenario: Closing the only Buffer exits Varde and deletes the Sidecar
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I run ":q"
    Then Varde exits
    And the Sidecar was deleted
    And no state was saved

  Scenario: Closing a Buffer with unsaved edits is refused and Varde keeps running
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    And the editor mode is insert
    And I type "hello" in the editor
    And the editor mode is normal
    When I run ":q"
    Then Varde is still running
    And the reviewer is told there are unsaved changes
    And the open buffers are:
      | notes.txt |

  Scenario: Force closing a Buffer with unsaved edits exits without writing the file
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    And the editor mode is insert
    And I type "hello" in the editor
    And the editor mode is normal
    When I run ":q!"
    Then Varde exits
    And the Sidecar was deleted
    And no state was saved
    And no file was written next to the file

  Scenario: Writing and closing the only Buffer writes the file and exits
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    And the editor mode is insert
    And I type "hello" in the editor
    And the editor mode is normal
    When I run ":wq"
    Then "notes.txt" was written with:
      """
      hellocontents
      """
    And Varde exits
    And the Sidecar was deleted
    And no state was saved

  Scenario: Closing every Buffer exits Varde
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    When I run ":qa"
    Then Varde exits
    And the Sidecar was deleted
    And no state was saved

  Scenario: Force closing every Buffer exits Varde without writing
    Given "/home/me/projects/theirs/notes.txt" is a file
    And Varde opened "/home/me/projects/theirs/notes.txt"
    And the editor mode is insert
    And I type "hello" in the editor
    And the editor mode is normal
    When I run ":qa!"
    Then Varde exits
    And the Sidecar was deleted
    And no state was saved
    And no file was written next to the file

  # Language intelligence and more Buffers

  # Write failures
