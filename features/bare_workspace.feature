Feature: A Bare workspace

  Running `varde` with no folder opens the folder it was started in and writes nothing of
  Varde's into it. The folder is still the workspace — the file tree is it, and `:w` writes
  there — but everything Varde needs for itself goes into a Sidecar under the user's own
  `~/.varde` instead. `varde <folder>` is untouched: every scenario in "Opening Varde on a
  folder" still holds, and this feature is the difference between the two.

  Background:
    Given the workspace root is "/home/me/projects/theirs"
    And the workspace folder holds the file "README.md"
    And the project has a ".gitignore"

  Scenario: A Bare workspace creates no directory in the folder
    When Varde starts with no folder in "/home/me/projects/theirs"
    Then no directory was created in the folder
    And the project ".gitignore" is unchanged

  Scenario: What Varde needs goes into the Sidecar instead
    When Varde starts with no folder in "/home/me/projects/theirs"
    Then Varde's own directory is the Sidecar

  Scenario: The file tree is the folder Varde was started in
    When Varde starts with no folder in "/home/me/projects/theirs"
    Then the workspace root is "/home/me/projects/theirs"
    And the file tree shows "README.md"

  Scenario: Saving writes to the folder it was started in, not the Sidecar
    Given Varde started with no folder in "/home/me/projects/theirs"
    And "README.md" is open in the editor holding:
      """
      one
      """
    When I run ":w"
    Then "README.md" was written into the folder Varde was started in

  # Three things a Bare workspace stops doing, because doing them into a
  # directory that is deleted at exit is worse than not doing them at all. The
  # Risk pane's own recompute is unchanged: the measurement stops happening
  # unasked, which removes no capability. The global config is seeded all the
  # same: it lives in the home directory, not the workspace, and outlives the
  # Sidecar.
  Scenario: A Bare workspace seeds the global config and nothing in the folder
    Given there is no global config
    When Varde starts with no folder in "/home/me/projects/theirs"
    Then the global config was seeded from the template
    And the project ".varde/config.toml" is unchanged
    And no config file was seeded in the workspace

  Scenario: A Bare workspace reads the global configuration
    Given the global config is:
      """
      [editor]
      theme = "nord"
      tab_width = 2
      """
    When Varde starts with no folder in "/home/me/projects/theirs"
    Then the effective setting "editor.theme" is "nord"
    And the effective setting "editor.tab_width" is "2"

  Scenario: A Bare workspace has no project configuration layer
    Given the global config is empty
    And the project config is:
      """
      [editor]
      tab_width = 2
      """
    When Varde starts with no folder in "/home/me/projects/theirs"
    Then the effective setting "editor.tab_width" is "4"

  Scenario: A Bare workspace measures no Risk at startup
    When Varde starts with no folder in "/home/me/projects/theirs"
    Then no analysis was asked for
    And the tree border risk state is "nothing-analysed"

  Scenario: Asking for Risk in a Bare workspace still measures
    Given Varde started with no folder in "/home/me/projects/theirs"
    When I ask for the figures to be recomputed
    Then an analysis was asked for over the scope "workspace"

  # Quitting leaves nothing behind: the Sidecar and everything in it goes, and
  # the session state a project keeps on the way out is not written at all —
  # a file saved into a directory deleted two effects later is the same
  # worthless write no seeded config is. A crash escapes this, which is what
  # the sweep on start exists for; process liveness is only observable at the
  # edge, so no scenario covers it (ADR 0016).

  Scenario: Quitting a Bare workspace deletes its Sidecar
    Given Varde started with no folder in "/home/me/projects/theirs"
    When I quit
    Then Varde exits
    And the Sidecar was deleted
    And no state was saved

  Scenario: Quitting a Bare workspace empties its trash
    Given the session's trash is "/home/me/.varde/trash/%home%me%projects%theirs-4242"
    And Varde started with no folder in "/home/me/projects/theirs"
    When I quit
    Then the session's trash was deleted

  Scenario: Quitting a project workspace deletes nothing but the session's trash
    Given Varde started in the project
    When I quit
    Then Varde exits
    And the project state was saved
    And no directory was deleted
    And the session's trash was deleted

  # The one place "leaves nothing behind" bends. A review is the output of the
  # reading, and losing an output is a different kind of nothing than leaving
  # no trace — so it goes to ~/.varde/reviews, durable and outside every
  # workspace, rather than into the Sidecar that is deleted two effects later
  # (ADR 0016).

  Scenario: A review submitted from a Bare workspace is written outside every workspace
    Given Varde started with no folder in "/home/me/projects/theirs"
    And an AI session is running in the AI pane
    And I add an ISSUE on "README.md" lines 1 to 1 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then the file "~/.varde/reviews/0001.json" exists
    And no file was written into the folder Varde was started in
    And no file was written into the Sidecar
    And the AI pane was sent a prompt containing "/home/me/.varde/reviews/0001.json"

  Scenario: The retention limit drops the oldest global review
    Given Varde started with no folder in "/home/me/projects/theirs"
    And the retention limit is 2 reviews
    And "~/.varde/reviews" holds 2 reviews numbered 0001 to 0002
    And I add an ISSUE on "README.md" lines 1 to 1 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then the file "~/.varde/reviews/0003.json" exists
    And the file "~/.varde/reviews/0001.json" does not exist
    And "~/.varde/reviews" holds 2 reviews

  # One directory shared by every Bare workspace, so the numbering cannot start
  # from nothing: a session that wrote 0001.json would write over the review
  # submitted from somebody else's folder — the loss this whole exception
  # exists to prevent.

  Scenario: A review numbers on from the ones already kept
    Given "~/.varde/reviews" holds 2 reviews numbered 0001 to 0002
    And Varde started with no folder in "/home/me/projects/theirs"
    And I add an ISSUE on "README.md" lines 1 to 1 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then the file "~/.varde/reviews/0003.json" exists
    And the file "~/.varde/reviews/0001.json" exists
