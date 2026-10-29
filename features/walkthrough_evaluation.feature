Feature: Evaluating a Waypoint, and AI Write

  The agent checks what the developer wrote at a code Waypoint. Saving a file the current Waypoint
  touches asks for an evaluation, and so does the check Chip on the editor's Transport, without
  saving. A context Waypoint is never evaluated, and neither is the save Varde makes before a
  hand-over: no evaluation on every keystroke, and none the developer did not cause.

  The agent writes the verdict on the Waypoint in the file. The footer shows the current Waypoint's
  latest verdict for as long as it is current — `failing` in red with the agent's message, `lgtm` as
  a green `LGTM` — and another notice shows over it, then gives way back to it. A verdict never
  blocks stepping.

  When the developer is stuck, `▶ AI Write` on the Waypoint's line, left of the minimap, asks the
  agent to write that snippet. The agent writes it and sets `lgtm` in one update; Varde reloads the
  buffer, keeps the cursor on the Waypoint, and the Waypoint list marks it `AI`.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/plan.rs" holds 20 numbered lines
    And "src/lib.rs" holds 40 numbered lines
    And "src/keys.rs" holds 10 numbered lines
    And an AI session is running

  Rule: A save of the current Waypoint's file, or the check Chip, asks for an evaluation

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "context", "text": "Why it lands here", "verdict": null, "ai": false}
        ]}
        """

    Scenario: Saving the current Waypoint's file sends one evaluate request
      Given I type "let plan = parse(text);" in the editor
      When I write the buffer
      Then exactly 1 "evaluate" walkthrough request was sent
      And the walkthrough request names Waypoint "w1"

    Scenario: Saving an unrelated file sends no evaluation
      Given I open "src/keys.rs" from the tree
      When I write the buffer
      Then no "evaluate" walkthrough request was sent

    Scenario: Saving on a context Waypoint sends no evaluation
      Given I pressed "]"
      When I write the buffer
      Then no "evaluate" walkthrough request was sent

    Scenario: The save before a hand-over sends no evaluation
      Given "src/plan.rs" has unsaved edits
      When I adjust the walkthrough with "don't duplicate this"
      Then "src/plan.rs" was saved before the request was sent
      And no "evaluate" walkthrough request was sent

    Scenario: Typing alone sends no evaluation
      When I type "let plan = parse(text);" in the editor
      Then no walkthrough request was sent

    Scenario: The check Chip sends an evaluate request without saving
      Given "src/plan.rs" has unsaved edits
      When I click the "check" Chip on the editor's border
      Then exactly 1 "evaluate" walkthrough request was sent
      And "src/plan.rs" was not saved

    Scenario: The check key sends an evaluate request
      When I run ":guide check"
      Then exactly 1 "evaluate" walkthrough request was sent

  Rule: The footer shows the current Waypoint's latest verdict

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": {"state": "lgtm"}, "ai": false}
        ]}
        """

    Scenario: A failing verdict is shown in the footer with its message
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": {"state": "failing", "message": "The error is dropped"}, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": {"state": "lgtm"}, "ai": false}
        ]}
        """
      Then the footer verdict is "failing"
      And the footer verdict carries the agent's message

    Scenario: An lgtm verdict is shown in the footer
      When I press "]"
      Then the footer verdict is "lgtm"

    Scenario: A Waypoint with no verdict shows none
      Then the footer verdict is "none"

    Scenario: Stepping back shows that Waypoint's latest verdict again
      Given I pressed "]"
      And I pressed "["
      When I press "]"
      Then the footer verdict is "lgtm"

    Scenario: A notice shows over the verdict and then gives way back to it
      Given I pressed "]"
      And I run ":nosuchcommand"
      When the notice times out
      Then the footer verdict is "lgtm"

  Rule: AI Write has the agent write the snippet

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """

    Scenario: The AI Write Chip sits on the Waypoint's line, left of the minimap
      Then the "AI Write" Chip is drawn on line 12 of "src/plan.rs"
      And the "AI Write" Chip ends left of the minimap

    Scenario: The AI Write Chip sends a write request
      When I click the "AI Write" Chip on the Waypoint's line
      Then exactly 1 "write" walkthrough request was sent
      And the walkthrough request names Waypoint "w1"
      And no file was written

    Scenario: The AI Write key sends a write request
      When I run ":guide write"
      Then exactly 1 "write" walkthrough request was sent

    Scenario: The agent's write reloads the buffer, keeps the cursor and marks the Waypoint AI
      Given I asked the agent to write Waypoint "w1"
      And the agent wrote "    let plan = parse(text)?;" to line 13 of "src/plan.rs"
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": {"state": "lgtm"}, "ai": true}
        ]}
        """
      Then line 13 of "src/plan.rs" reads "    let plan = parse(text)?;"
      And the current Waypoint is "w1"
      And the cursor is on line 13 of "src/plan.rs"
      And the footer verdict is "lgtm"
      And Waypoint "w1" is marked "ai" in the Waypoint list
