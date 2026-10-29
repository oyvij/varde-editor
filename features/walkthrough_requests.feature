Feature: Starting and adjusting a Walkthrough

  `:guide` opens the Inquiry box, titled `Walkthrough` with no session and `Adjust walkthrough`
  during one. `Ctrl+S` sends it and `Esc` cancels it, as the Inquiry box always does, and the
  `◆ AI Guide` Chip opens the same box. The first prompt asks the agent to `plan`; one during a
  session asks it to `adjust`, reverting what the new approach replaces, rewriting the plan and
  naming the `checkpoint` to resume at.

  Every hand-over is one line naming the `walkthrough` Skill, the action, the plan file and the
  current Waypoint, so a fresh process knows where the session is without any provider's resume
  flags. The first `plan` also carries the open buffers and the cursor's file, line and column, so
  the agent starts from where the developer is. The agent scouts the code itself on every request:
  no index, language server link or MCP server is kept for it.

  Two drivers carry a request. `walkthrough.driver = "pane"`, the default, pastes it into the hosted
  AI pane, started on demand as `:story` does, so the developer can watch the agent and talk to it.
  `"background"` runs `walkthrough.command` (default `claude -p`) once per request, off the loop,
  with the request on stdin; its output is never read, and a non-zero exit is shown. Neither driver
  has provider-specific code.

  One request is in flight at a time. A newer evaluation replaces a queued one, and an adjustment or
  a revert queues ahead of evaluations. Before every hand-over Varde saves every buffer the
  Walkthrough touches, so the agent never works on stale files, and that save asks for no
  evaluation.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/plan.rs" holds 20 numbered lines
    And "src/lib.rs" holds 40 numbered lines

  Rule: :guide opens the box, and Escape costs nothing

    Scenario: With no session the box is titled Walkthrough
      Given no walkthrough is active
      When I run ":guide"
      Then the modal is "walkthrough"
      And no keys were sent to the AI pane

    Scenario: The AI Guide Chip opens the same box
      Given no walkthrough is active
      When I click the "AI Guide" Chip on the editor's border
      Then the modal is "walkthrough"

    Scenario: The command list offers the Walkthrough
      Given no walkthrough is active
      When I open the command line and type "gu"
      Then the command list offers "guide"

    Scenario: Escape closes the box and no request is sent
      Given no walkthrough is active
      And an AI session is running
      And I run ":guide"
      And I type "add a plan parser" in the inquiry box
      When I press Escape in the inquiry box
      Then the modal is "none"
      And no walkthrough request was sent
      And no keys were sent to the AI pane
      And no walkthrough is active

  Rule: The first request plans, and carries where the developer is

    Scenario: Ctrl+S sends a plan request carrying the open buffers and the cursor
      Given no walkthrough is active
      And an AI session is running
      And "src/lib.rs" is open in the editor
      And "src/plan.rs" is open in the editor
      And the cursor is at line 4 column 2
      And I run ":guide"
      And I type "add a plan parser" in the inquiry box
      When I press Ctrl+s in the inquiry box
      Then a "plan" walkthrough request was sent
      And the walkthrough request names the Skill "walkthrough"
      And the walkthrough request names the plan file
      And the walkthrough request carries "add a plan parser"
      And the walkthrough request carries the open buffers "src/lib.rs" and "src/plan.rs"
      And the walkthrough request carries the cursor at "src/plan.rs" line 4 column 2
      And the modal is "none"
      And no file was written

    Scenario: The agent is never asked to keep an index or reach a server
      Given no walkthrough is active
      And an AI session is running
      When I submit the walkthrough "add a plan parser"
      Then no language server was started
      And no file was written

  Rule: The pane driver pastes into the AI pane

    Scenario: The pane driver is the default
      Given no walkthrough config is set
      Then the walkthrough driver is "pane"
      And the walkthrough command is "claude -p"

    Scenario: A request is pasted into the running AI pane
      Given the walkthrough driver is "pane"
      And an AI session is running
      When I submit the walkthrough "add a plan parser"
      Then a "plan" walkthrough request was pasted into the AI pane
      And no background command was run

    Scenario: With no AI session the configured one is started and the request is held
      Given the walkthrough driver is "pane"
      And no AI session is running
      When I submit the walkthrough "add a plan parser"
      Then the AI session "claude" was started
      And no keys were sent to the AI pane
      And the walkthrough spinner names "plan" as running

    Scenario: The held request is sent once the AI session is ready
      Given the walkthrough driver is "pane"
      And no AI session is running
      And I submitted the walkthrough "add a plan parser"
      When the AI session is ready for input
      Then the AI pane was sent a prompt containing "add a plan parser"

  Rule: The background driver runs the configured command with the request on stdin

    Scenario: A request runs the configured command once, with the request on stdin
      Given the walkthrough driver is "background"
      And the walkthrough command is "claude -p --continue"
      When I submit the walkthrough "add a plan parser"
      Then the background command "claude -p --continue" was run once
      And its stdin is a "plan" walkthrough request
      And no keys were sent to the AI pane
      And no AI session was started

    Scenario: A non-zero exit is shown in the footer
      Given the walkthrough driver is "background"
      And I submitted the walkthrough "add a plan parser"
      When the background command exits with status 2
      Then the notice is "walkthrough-command-failed"
      And no walkthrough request is in flight

    Scenario: A failed first plan leaves no session, so :guide plans again
      Given the walkthrough driver is "background"
      And I submitted the walkthrough "add a plan parser"
      When the background command exits with status 1
      Then no walkthrough is active

    Scenario: A zero exit raises no notice
      Given the walkthrough driver is "background"
      And I submitted the walkthrough "add a plan parser"
      When the background command exits with status 0
      Then no notice was raised

    Scenario: Each background request names the plan file and the current Waypoint
      Given the walkthrough driver is "background"
      And a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w2", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """
      When I adjust the walkthrough with "don't duplicate this"
      Then the background command's stdin names the plan file
      And the background command's stdin names Waypoint "w2"

  Rule: During a session :guide adjusts the plan

    Background:
      Given an AI session is running
      And a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """

    Scenario: During a session the box is titled Adjust walkthrough
      When I run ":guide"
      Then the modal is "adjust-walkthrough"

    Scenario: Sending the adjustment asks for adjust, naming the current Waypoint
      Given I pressed "]"
      And I run ":guide"
      And I type "move these functions to a new file" in the inquiry box
      When I press Ctrl+s in the inquiry box
      Then an "adjust" walkthrough request was sent
      And the walkthrough request names Waypoint "w2"
      And the walkthrough request carries "move these functions to a new file"
      And the walkthrough file was not written

    Scenario: The agent's rewritten plan lands on its checkpoint
      Given I adjusted the walkthrough with "don't duplicate this"
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w4", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": {"state": "lgtm"}, "ai": false},
          {"id": "w4", "file": "src/lib.rs", "line": 7, "kind": "code", "text": "Reuse the parser", "verdict": null, "ai": false}
        ]}
        """
      Then the current Waypoint is "w4"
      And the cursor is on line 8 of "src/lib.rs"

  Rule: Every hand-over saves the Walkthrough's buffers first, and asks for no evaluation

    Background:
      Given an AI session is running
      And a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """

    Scenario: An adjustment saves every touched buffer before it is sent
      Given "src/plan.rs" has unsaved edits
      And "src/lib.rs" has unsaved edits
      When I adjust the walkthrough with "don't duplicate this"
      Then "src/plan.rs" was saved before the request was sent
      And "src/lib.rs" was saved before the request was sent
      And no "evaluate" walkthrough request was sent

    Scenario: A buffer the Walkthrough does not touch is not saved
      Given "src/other.rs" holds 5 numbered lines
      And "src/other.rs" has unsaved edits
      When I adjust the walkthrough with "don't duplicate this"
      Then "src/other.rs" was not saved

  Rule: One request is in flight at a time

    Background:
      Given an AI session is running
      And a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """

    Scenario: A second request waits for the first
      Given an "evaluate" walkthrough request is in flight
      When I adjust the walkthrough with "don't duplicate this"
      Then exactly 1 walkthrough request was sent
      And the walkthrough queue holds "adjust"

    Scenario: A newer evaluation replaces a queued one
      Given an "adjust" walkthrough request is in flight
      And an evaluation of Waypoint "w1" is queued
      When an evaluation of Waypoint "w2" is queued
      Then the walkthrough queue holds "evaluate w2"

    Scenario: An adjustment queues ahead of evaluations
      Given an "adjust" walkthrough request is in flight
      And an evaluation of Waypoint "w1" is queued
      When I adjust the walkthrough with "use the existing parser"
      Then the walkthrough queue holds, in order:
        | adjust      |
        | evaluate w1 |

    Scenario: The next queued request goes when the plan file changes
      Given an "adjust" walkthrough request is in flight
      And an evaluation of Waypoint "w1" is queued
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """
      Then an "evaluate" walkthrough request was sent
      And the walkthrough queue is empty

    Scenario: The spinner names what is running and what is queued
      Given an "adjust" walkthrough request is in flight
      And an evaluation of Waypoint "w1" is queued
      When the clock advances 1 minutes
      Then the walkthrough spinner names "adjust" as running
      And the walkthrough spinner names "evaluate" as queued

  Rule: A broken plan is handed back for two rounds, as Stories are

    Background:
      Given an AI session is running
      And a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """

    Scenario: A broken plan sends one fix request naming the failing Waypoints
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 99, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """
      Then a "fix" walkthrough request was sent
      And the fix request names Waypoint "w2" as "line-out-of-range"
      And the fix request does not name Waypoint "w1"

    Scenario: After two failed rounds no further fix request is sent
      Given the walkthrough file changes to:
        """
        not json
        """
      And the walkthrough file changes to:
        """
        still not json
        """
      When the walkthrough file changes to:
        """
        never json
        """
      Then exactly 2 "fix" walkthrough requests were sent
      And the notice is "walkthrough-invalid"
      And the plan holds the Waypoints, in order:
        | w1 |
