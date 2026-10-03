Feature: Hot code replace

  A program rebuilt while it is Paused is running the bytecode it was loaded with: the Evaluator
  throws for a class the rebuild renumbered, and nothing on screen says why. Replacing the code in
  the running process is how the pause survives a build.

  The protocol has no request for it, so the adapter's row names its own — a request to send and the
  event the adapter announces a build with. Varde knows neither name
  (`docs/adr/0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md`): it sends what the row
  says on the event the row says, which is why the names here are made up. A row without the key has
  no hot replace, and offers neither the Command nor the Chip.

  Nothing restarts on its own. A replace the adapter refuses — a changed class shape the runtime
  will not take — says so and offers a restart, and until the offer is taken the session carries on
  with the old code, which is the pause the reader was keeping.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And a Debug adapter for "rust" is configured
    And "src/main.rs" is open in the editor holding:
      """
      fn main() {
          let total = 0;
      }
      """

  Rule: The adapter's event sends the adapter's request, and no other event does

    Scenario: The named event sends the named request
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      When the Debug adapter sends the event "classesBuilt"
      Then the Debug adapter was sent a "swapClasses" request

    Scenario: An event the row does not name sends nothing
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      When the Debug adapter sends the event "somethingElseEntirely"
      Then the Debug adapter was sent no "swapClasses" request

    Scenario: A row naming nothing has no hot replace at all
      Given a Debug session is Paused at "src/main.rs" line 2
      When the Debug adapter sends the event "classesBuilt"
      Then the Debug adapter was sent 0 "swapClasses" requests
      And the Transport has no "hot-replace" Chip

  Rule: The Command and the Chip send it, and a row naming nothing offers neither

    Scenario: The command line sends the named request
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      When I ask for a hot replace from the command line
      Then the Debug adapter was sent a "swapClasses" request

    Scenario: The Debug group offers a Chip that sends it
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      When I click the "hot-replace" Chip
      Then the Debug adapter was sent a "swapClasses" request
      And the "hot-replace" Chip is lit

    Scenario: A row naming nothing offers no Chip and declines the Command out loud
      Given a Debug session is Paused at "src/main.rs" line 2
      When I ask for a hot replace from the command line
      Then the notice is "no-hot-replace"
      And the Transport has no "hot-replace" Chip
      And the Debug adapter was sent no "swapClasses" request

  Rule: A replace that worked says so, and one that failed offers a restart it does not take

    Scenario: A successful replace raises a notice
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      And I ask for a hot replace from the command line
      When the Debug adapter answers "swapClasses"
      Then the notice is "hot-replaced"

    Scenario: A failed replace carries the adapter's reason and restarts nothing
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      And I ask for a hot replace from the command line
      When the Debug adapter answers "swapClasses" with the error "KeyFigure has a new field"
      Then the notice is "hot-replace-failed"
      And the message names "KeyFigure has a new field"
      And the Debug session is "paused"
      And the Debug adapter was sent no "disconnect" request

    Scenario: A failed replace offers the restart a running session otherwise declines
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      And I ask for a hot replace from the command line
      When the Debug adapter answers "swapClasses" with the error "KeyFigure has a new field"
      Then the "restart" Chip is not dimmed

    Scenario: A session nothing failed in dims the restart
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      Then the "restart" Chip is dimmed

    Scenario: A replace that works after one that failed withdraws the offer
      Given the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session is Paused at "src/main.rs" line 2
      And I ask for a hot replace from the command line
      And the Debug adapter answers "swapClasses" with the error "KeyFigure has a new field"
      And I ask for a hot replace from the command line
      When the Debug adapter answers "swapClasses"
      Then the "restart" Chip is dimmed

    Scenario: Taking the offer restarts the session with the same Launch configuration
      Given the project config is:
        """
        [launch.server]
        adapter = "rust"
        request = "launch"
        args = { program = "target/debug/server" }
        """
      And the Debug adapter's row names the hot replace request "swapClasses" on the event "classesBuilt"
      And a Debug session was started from the Launch configuration "server"
      And I ask for a hot replace from the command line
      And the Debug adapter answers "swapClasses" with the error "KeyFigure has a new field"
      When I click the "restart" Chip
      Then the Debug adapter was sent a "disconnect" request
      And the Debug adapter was sent 2 "launch" requests
      And the Debug adapter's launch arguments are those of "server"
