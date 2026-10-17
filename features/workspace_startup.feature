Feature: Opening Varde on a folder

  Varde opens on exactly one folder and that folder is the workspace; a file opens the
  Single-file editor instead. A path it cannot use stops it before the TUI appears, and each
  way a path can be unusable gets its own explanation — a typo and a permissions problem are
  different problems for the user to fix.

  An empty folder is not one of them. Starting Varde in a fresh directory is how a project
  begins.

  Scenario: The folder opened becomes the workspace
    Given the folder "/home/me/projects/varde" exists
    When Varde opens "/home/me/projects/varde"
    Then the workspace root is "/home/me/projects/varde"
    And the workspace title is "varde"

  Scenario: An empty folder is a valid workspace
    Given the folder "/home/me/projects/fresh" exists and is empty
    When Varde opens "/home/me/projects/fresh"
    Then the workspace root is "/home/me/projects/fresh"
    And the file tree is empty

  Scenario: A path that does not exist stops Varde
    Given "/home/me/projects/nope" does not exist
    When Varde opens "/home/me/projects/nope"
    Then Varde refuses to start
    And the reason is "no-such-folder"

  Scenario: A path that is a file opens the Single-file editor
    Given "/home/me/projects/varde/README.md" is a file
    When Varde opens "/home/me/projects/varde/README.md"
    Then Varde is the Single-file editor
    And "README.md" is open in the editor

  Scenario: A folder that cannot be read stops Varde
    Given the folder "/home/me/projects/locked" cannot be read
    When Varde opens "/home/me/projects/locked"
    Then Varde refuses to start
    And the reason is "folder-not-readable"
