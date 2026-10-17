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

  Scenario: A missing file whose folder is missing too stops Varde
    Given neither "/home/me/projects/nope/new.txt" nor its folder exists
    When Varde opens "/home/me/projects/nope/new.txt"
    Then Varde refuses to start
    And the reason is "no-such-folder"

  # The full-screen editor

  # The palette and the Command list

  # Closing

  # Language intelligence and more Buffers

  # Write failures
