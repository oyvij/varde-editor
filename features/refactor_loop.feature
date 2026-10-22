Feature: The Refactor loop

  Varde hands an AI session a Scope and a goal, waits to be told the pass is finished, then runs the
  project's tests itself, recomputes the figures itself, and applies the Gate. It does not ask the
  session whether the pass was good: Varde may not read what a hosted pane prints, so it cannot
  verify a claim, so it measures — `docs/adr/0010-varde-owns-the-test-gate.md`.

  Completion is a filesystem fact rather than a terminal one. The session writes a sentinel, seen by
  the watcher that already exists, and Varde deletes it before each Iteration begins so a leftover
  cannot instantly complete the next one. Its contents are ignored. Silence is never completion: a
  session thinking for forty seconds looks exactly like one that finished, so the wait has no timeout
  and the exits are the stop action and the Iteration cap.

  The Gate is three conditions: the tests still pass, the primary figure moved down, and no other
  recorded metric moved up. The third is not politeness — cyclomatic complexity is trivially gamed by
  shredding one long Function into fifteen trivial ones, and cognitive complexity does not fall for
  it. An Iteration failing any condition is returned to the snapshot Varde took before it started,
  and the loop stops saying which condition stopped it. An Iteration that passes stands —
  uncommitted, always, because the loop's whole output is a working tree for you to review.

  Reverting goes to that snapshot and covers only the files the Iteration touched, never to the last
  commit: a file you had edited and the loop did not is never restored over.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository
    And the risk threshold is 20
    And the configured test command is "cargo test"
    And the iteration cap is 3
    And an AI session is running in the AI pane
    And the figures were computed for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 31         | 24        |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    And the Risk list is shown

  Scenario: Starting the loop hands the session the Scope, the figures and the target
    When I start the Refactor loop over the scope "workspace"
    Then the AI pane was sent a prompt naming the file "src/keys.rs"
    And the AI pane was sent a prompt naming the file ".varde/risk.json"
    And the AI pane was sent a prompt naming the scope "workspace"
    And the AI pane was sent a prompt naming the target threshold "20"
    And the prompt was submitted to the AI

  Scenario: The prompt tells the session to obey the repo's own convention files
    When I start the Refactor loop over the scope "workspace"
    Then the AI pane was sent a prompt instructing the session to follow the repo's convention files

  Scenario: The prompt asks for splits that stand on their own, not for a lower number
    When I start the Refactor loop over the scope "workspace"
    Then the AI pane was sent a prompt asking for splits that stand on their own

  Scenario: The prompt names scattering into single-call-site helpers as a failed pass
    When I start the Refactor loop over the scope "workspace"
    Then the AI pane was sent a prompt naming structural scattering as a failed pass

  Scenario: The test command is never interpolated, because Varde runs the tests
    When I start the Refactor loop over the scope "workspace"
    Then no prompt sent to the AI contains "cargo test"

  Scenario: Nothing in what was sent names a provider
    When I start the Refactor loop over the scope "workspace"
    Then the prompt names no AI provider

  Scenario: With no session running the loop starts one and holds the prompt until it speaks
    Given no AI session is running in the AI pane
    And the effective setting "ai.command" is "claude"
    When I start the Refactor loop over the scope "workspace"
    Then no prompt was sent to the AI
    And an AI session was started with "claude"

  Scenario: Once the session speaks, the held prompt goes
    Given no AI session is running in the AI pane
    And I start the Refactor loop over the scope "workspace"
    When the AI session is ready for input
    Then the AI pane was sent a prompt naming the scope "workspace"

  Scenario: The test command falls back to detection from the project's shape
    Given no "risk.test_command" is configured
    And the project holds "Cargo.toml"
    When I start the Refactor loop over the scope "workspace"
    Then the loop's test command is "cargo test"

  Scenario: A test command that cannot be determined refuses the loop rather than passing a Gate
    Given no "risk.test_command" is configured
    And the project's shape names no test command
    When I start the Refactor loop over the scope "workspace"
    Then no Refactor loop is running
    And the Refactor loop refusal is "no-test-command"
    And no prompt was sent to the AI
    And no test command was run

  Scenario: A loop started before anything has been measured is refused rather than judged against zero
    Given no figures have been computed
    When I start the Refactor loop over the scope "workspace"
    Then no Refactor loop is running
    And the Refactor loop refusal is "no-figure"
    And no prompt was sent to the AI
    And no test command was run
    And no snapshot was taken

  Scenario: The sentinel is deleted before an Iteration begins
    When I start the Refactor loop over the scope "workspace"
    Then the sentinel ".varde/refactor-done" was deleted

  Scenario: A leftover sentinel cannot complete the Iteration it was left before
    Given the sentinel ".varde/refactor-done" already exists
    When I start the Refactor loop over the scope "workspace"
    Then the Refactor loop wait state is "waiting-for-session"
    And no test command was run

  Scenario: Silence never ends the wait
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    When the clock advances 30 minutes
    Then the Refactor loop wait state is "waiting-for-session"
    And no test command was run
    And the Refactor loop is running

  Scenario: The sentinel appearing ends the wait and runs the tests
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    When the sentinel ".varde/refactor-done" appears
    Then the test command "cargo test" was run
    And the Refactor loop wait state is "waiting-for-tests"

  Scenario: The tests run off the shell pane, so the shell stays the user's
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    When the sentinel ".varde/refactor-done" appears
    Then the test command "cargo test" was run
    And no command has been executed

  Scenario: The Iteration is snapshotted before the session is let at the files
    When I start the Refactor loop over the scope "workspace"
    Then a snapshot was taken for Iteration 1

  Scenario: Failing tests revert the Iteration and stop the loop
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the Iteration touched:
      | src/keys.rs |
    When the tests finish failing:
      """
      test route::spells ... FAILED
      """
    Then Iteration 1 was restored from its snapshot
    And the Refactor loop is not running
    And the Refactor loop stopped because "tests-failed"
    And nothing was committed

  Scenario: A revert restores only the files the Iteration touched
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And "src/ui.rs" had uncommitted changes before the loop started
    And the Iteration touched:
      | src/keys.rs |
    When the tests finish failing:
      """
      test route::spells ... FAILED
      """
    Then the files restored are:
      | src/keys.rs |
    And "src/ui.rs" was not restored

  Scenario: My own uncommitted edits in a file the Iteration also touched survive the revert
    Given "src/keys.rs" had uncommitted changes before the loop started
    And the Refactor loop is on Iteration 1 over the scope "workspace"
    And the Iteration touched:
      | src/keys.rs |
    When the tests finish failing:
      """
      test route::spells ... FAILED
      """
    Then "src/keys.rs" holds the uncommitted changes it held before the loop started
    And "src/keys.rs" was not restored from the last commit

  Scenario: A failure is explained rather than reported bare
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    When the tests finish failing:
      """
      test route::spells ... FAILED
      """
    Then the Refactor loop reports the tests' output:
      """
      test route::spells ... FAILED
      """

  Scenario: A reverted Iteration is explained to the session too, not only to me
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    When the tests finish failing:
      """
      test route::spells ... FAILED
      """
    Then the AI pane was sent a prompt naming the Gate condition "tests-failed"
    And the AI pane was sent a prompt containing the tests' output

  Scenario: The figures are recomputed before the Gate is evaluated
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    When the tests finish passing
    Then an analysis was asked for over the scope "workspace"
    And the Gate has not been evaluated

  Scenario: An Iteration that lowered the Risk count is accepted, and stays uncommitted
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 14         | 11        |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then Iteration 1 passed the Gate
    And Iteration 1 was not restored from its snapshot
    And nothing was committed
    And the Refactor loop is on Iteration 2

  Scenario: An unchanged count with a lower total is progress, not a plateau
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 26         | 20        |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then Iteration 1 passed the Gate
    And the risk count is 2

  Scenario: An Iteration that improved neither the count nor the total is reverted
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 31         | 24        |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then Iteration 1 was restored from its snapshot
    And the Refactor loop is not running
    And the Refactor loop stopped because "no-improvement"
    And nothing was committed

  Scenario: An Iteration that raised any other recorded metric is reverted
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 14         | 29        |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then Iteration 1 was restored from its snapshot
    And the Refactor loop stopped because "other-metric-worsened"
    And nothing was committed

  Scenario: Any other recorded metric, not only cognitive complexity, closes the Gate
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive | maintainability | lines |
      | src/keys.rs | route    | 88   | 14         | 11        | 12              | 96    |
      | src/ui.rs   | draw     | 17   | 22         | 18        | 55              | 40    |
    Then Iteration 1 was restored from its snapshot
    And the Refactor loop stopped because "other-metric-worsened"
    And nothing was committed

  Scenario: Shredding one Function into many trivial ones does not pass the Gate
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route_a  | 88   | 3          | 5         |
      | src/keys.rs | route_b  | 96   | 3          | 5         |
      | src/keys.rs | route_c  | 104  | 3          | 5         |
      | src/keys.rs | route_d  | 112  | 3          | 6         |
      | src/keys.rs | route_e  | 120  | 3          | 6         |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then the risk count is 1
    And Iteration 1 was restored from its snapshot
    And the Refactor loop stopped because "other-metric-worsened"

  Scenario: A reverted Iteration leaves nothing behind
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    And the Iteration touched:
      | src/keys.rs |
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 31         | 24        |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then the working tree holds no change from Iteration 1

  Scenario: The cap is honoured, and the loop says the cap is why it stopped
    Given the iteration cap is 2
    And 1 Iteration has passed the Gate over the scope "workspace"
    And the Refactor loop is on Iteration 2 over the scope "workspace"
    And the tests finished passing
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 9          | 7         |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then Iteration 2 passed the Gate
    And the Refactor loop is not running
    And the Refactor loop stopped because "cap-reached"
    And nothing was committed

  Scenario: The pane shows the last test result, passing as well as failing
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    When the tests finish passing
    Then the Refactor loop last test result is "passed"

  Scenario: A recompute is a fresh measurement, so the finished run's verdict goes with it
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the Iteration touched:
      | src/keys.rs |
    And the tests finish failing:
      """
      test route::spells ... FAILED
      """
    And the Refactor loop stopped because "tests-failed"
    When I click the "recompute-risk" action on the Risk list pane
    Then an analysis was asked for over the scope "workspace"
    And the Risk list border says nothing about the loop
    And no Refactor loop last test result is reported

  Scenario: A running loop's own verdict is not cleared out from under its Gate
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    When I click the "recompute-risk" action on the Risk list pane
    Then the Refactor loop last test result is "passed"

  Scenario: A long run is legible while it happens
    Given the Refactor loop is on Iteration 2 over the scope "workspace"
    Then the Refactor loop status shows Iteration 2 of 3
    And the Risk list shows the figure for Iteration 2

  Scenario: The tree border shows the figure as it moves
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the tests finished passing
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 14         | 11        |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then the risk count is 1

  Scenario: Stopping is exactly where starting was
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    Then the Risk list pane actions offered are:
      | recompute-risk     |
      | stop-refactor-loop |

  Scenario: With no loop running, the same place offers the start
    Then the Risk list pane actions offered are:
      | recompute-risk      |
      | start-refactor-loop |

  Scenario: Stopping leaves the workspace where the last accepted Iteration left it
    Given Iteration 1 passed the Gate over the scope "workspace"
    And the Refactor loop is on Iteration 2 over the scope "workspace"
    When I stop the Refactor loop
    Then the Refactor loop is not running
    And Iteration 2 was restored from its snapshot
    And Iteration 1 was not restored from its snapshot
    And nothing was committed

  Scenario: Escape does not stop the loop
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the Risk list pane has focus
    When I press "Escape"
    Then the Refactor loop is running

  Scenario: A second loop is refused, visibly, rather than queued
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    When I start the Refactor loop over the scope "workspace"
    Then 1 Refactor loop is running
    And the Refactor loop refusal is "loop-already-running"
    And exactly 1 prompt was sent to the AI

  Scenario: The session wait counts its elapsed time from the Ticks
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the clock reads 10 seconds
    When the clock reads 161 seconds
    Then the wait has run for 151 seconds
    And the wait's file activity is "none-yet"
    And the Refactor loop wait state is "waiting-for-session"

  Scenario: A change in the workspace is the session wait's last file activity
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the clock reads 30 seconds
    And a change to "src/keys.rs" is seen on disk
    When the clock reads 42 seconds
    Then the wait's last file activity was 12 seconds ago

  Scenario: The next Iteration's session wait starts its own count with no file activity
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the clock reads 30 seconds
    And a change to "src/keys.rs" is seen on disk
    And the tests finished passing
    And the clock reads 300 seconds
    When the figures arrive for the scope "workspace":
      | file        | function | line | cyclomatic | cognitive |
      | src/keys.rs | route    | 88   | 14         | 11        |
      | src/ui.rs   | draw     | 17   | 22         | 18        |
    Then the Refactor loop is on Iteration 2
    And the wait has run for 0 seconds
    And the wait's file activity is "none-yet"

  Scenario: The sentinel ends the session wait as before, and the wait's count with it
    Given the Refactor loop is on Iteration 1 over the scope "workspace"
    And the clock reads 30 seconds
    And a change to "src/keys.rs" is seen on disk
    When the sentinel ".varde/refactor-done" appears
    Then the test command "cargo test" was run
    And the Refactor loop wait state is "waiting-for-tests"
    And no command has been executed
    And no wait is in progress
