Feature: Finishing, reverting and replacing a Walkthrough

  The developer ends a Walkthrough, never the agent. Finish (`✓ finish`, or `:guide finish`) keeps
  the code: it removes every comment Varde inserted, ends the session and clears the tree's colour,
  and it sends nothing. It is allowed at any time, and it is lit once the agent marks the plan
  `completed` — which lights it and nothing more.

  Revert (`↶ revert`, or `:guide revert`) drops the attempt. It is confirmed first, then the agent
  is asked to restore every file the session touched, and the session ends when the agent marks the
  file `ended`. Varde never restores a file itself
  (`docs/adr/0031-a-walkthrough-is-a-file-the-agent-owns.md`).

  `:guide!` during a session replaces it, and never silently loses work: it asks `k` keep, `r`
  revert or `Esc` cancel, runs the chosen end action, then opens the `Walkthrough` box for the new
  prompt. With no session it is `:guide`. These Chips replace the Transport's single exit Chip.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/plan.rs" holds 20 numbered lines
    And "src/lib.rs" holds 40 numbered lines
    And an AI session is running

  Rule: Finish keeps the code and removes the comments

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """

    Scenario: Finish removes every inserted comment and keeps the developer's code
      Given I type "let plan = parse(text);" in the editor
      And I pressed "]"
      When I run ":guide finish"
      Then "src/plan.rs" holds no comment "Parse the plan"
      And "src/lib.rs" holds no comment "Call the parser"
      And "src/plan.rs" still holds "let plan = parse(text);"
      And no walkthrough is active
      And no walkthrough request was sent

    Scenario: Finish is allowed before the plan is completed
      When I click the "finish" Chip on the editor's border
      Then no walkthrough is active

    Scenario: Finish closes the Waypoint list
      When I run ":guide finish"
      Then the Corner does not hold the Waypoint list

  Rule: completed lights Finish and ends nothing

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": {"state": "lgtm"}, "ai": false}
        ]}
        """

    Scenario: Finish is not lit while the plan is active
      Then the "finish" Chip is not lit

    Scenario: completed lights Finish and the session stays active
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "completed", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": {"state": "lgtm"}, "ai": false}
        ]}
        """
      Then the "finish" Chip is lit
      And the walkthrough state is "active"
      And the Corner holds the Waypoint list

  Rule: Revert is confirmed, and the agent restores the files

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """

    Scenario: Revert asks for confirmation first
      When I run ":guide revert"
      Then the modal is "confirm-revert-walkthrough"
      And no walkthrough request was sent

    Scenario: The revert Chip asks for confirmation too
      When I click the "revert" Chip on the editor's border
      Then the modal is "confirm-revert-walkthrough"

    Scenario: Declining sends nothing and the session goes on
      Given I run ":guide revert"
      When I press "n"
      Then no walkthrough request was sent
      And the walkthrough state is "active"

    Scenario: Confirming sends one revert request, and Varde writes no source file
      Given I run ":guide revert"
      When I press "y"
      Then exactly 1 "revert" walkthrough request was sent
      And the walkthrough state is "reverting"
      And no file was written

    Scenario: The session ends when the agent marks the file ended
      Given I reverted the walkthrough
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "ended", "checkpoint": null, "waypoints": []}
        """
      Then no walkthrough is active
      And no tree row carries the walkthrough colour

    Scenario: The Transport has no exit Chip
      Then the editor's Transport offers the "finish" Chip
      And the editor's Transport offers the "revert" Chip
      And the editor's Transport does not offer the "exit" Chip

  Rule: :guide! replaces a session without losing work

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """

    Scenario: :guide! asks keep, revert or cancel
      When I run ":guide!"
      Then the modal is "replace-walkthrough"
      And no walkthrough request was sent

    Scenario: k finishes the session and opens the Walkthrough box
      Given I run ":guide!"
      When I press "k"
      Then "src/plan.rs" holds no comment "Parse the plan"
      And no walkthrough is active
      And the modal is "walkthrough"

    Scenario: The new prompt after k is sent as a plan request
      Given I run ":guide!"
      And I press "k"
      And I type "add a plan writer" in the inquiry box
      When I press Ctrl+s in the inquiry box
      Then exactly 1 "plan" walkthrough request was sent

    Scenario: r reverts the session, then opens the Walkthrough box once it has ended
      Given I run ":guide!"
      And I press "r"
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "ended", "checkpoint": null, "waypoints": []}
        """
      Then exactly 1 "revert" walkthrough request was sent
      And no walkthrough is active
      And the modal is "walkthrough"

    Scenario: Escape changes nothing
      Given I run ":guide!"
      When I press Escape
      Then the modal is "none"
      And the walkthrough state is "active"
      And no walkthrough request was sent

    Scenario: With no session :guide! is :guide
      Given I ran ":guide finish"
      When I run ":guide!"
      Then the modal is "walkthrough"
