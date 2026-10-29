Feature: The walkthrough file is the contract with the agent

  A Walkthrough is held in one file, `walkthrough.json`, that the agent writes and Varde only reads
  (`docs/adr/0031-a-walkthrough-is-a-file-the-agent-owns.md`). It lives in the workspace's
  `.varde/`, or in the Sidecar for a Bare workspace. Varde watches it and loads it on every change.
  Plan order is the file's order, and Varde never sorts it, because the order a developer would
  build the change in is the agent's judgement.

  A file that does not parse, or a Waypoint whose file or line is out of range, never replaces the
  plan being walked: the last valid plan stays in use and the footer says what failed. A Waypoint in
  a file that does not exist yet is valid, because a change often starts a file.

  The agent's instructions ship as the `walkthrough` Skill (ADR 0026), so every request Varde sends
  is one short line.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/plan.rs" holds 20 numbered lines
    And "src/lib.rs" holds 40 numbered lines

  Scenario: The plan is read from the workspace's own folder
    Given no walkthrough is active
    When I run ":guide"
    Then the walkthrough file is "/home/me/projects/varde/.varde/walkthrough.json"

  Scenario: In a Bare workspace the plan is read from the Sidecar and nothing is written to the folder
    Given no walkthrough is active
    And the workspace is a Bare workspace
    When I run ":guide"
    Then the walkthrough file is in the Sidecar
    And no file was written into the folder Varde was started in

  Scenario: A valid plan is loaded with its Waypoints in file order
    Given I submitted the walkthrough "add a plan parser"
    When the walkthrough file changes to:
      """
      {"version": 1, "status": "active", "checkpoint": null, "waypoints": [
        {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
        {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "context", "text": "Why the plan lands here", "verdict": null, "ai": false},
        {"id": "w3", "file": "src/plan.rs", "line": 2, "kind": "code", "text": "Name the error", "verdict": null, "ai": false}
      ]}
      """
    Then the walkthrough state is "active"
    And the plan holds the Waypoints, in order:
      | w1 |
      | w2 |
      | w3 |

  Scenario: A Waypoint in a file that does not exist yet is valid
    Given I submitted the walkthrough "add a plan parser"
    When the walkthrough file changes to:
      """
      {"version": 1, "status": "active", "checkpoint": null, "waypoints": [
        {"id": "w1", "file": "src/guide/plan.rs", "line": 1, "kind": "code", "text": "Start the parser", "verdict": null, "ai": false}
      ]}
      """
    Then the walkthrough state is "active"
    And the plan holds the Waypoints, in order:
      | w1 |

  Scenario: Varde never writes the walkthrough file
    Given a walkthrough is active on the plan:
      """
      {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
        {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
        {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
      ]}
      """
    When I press "]"
    Then the walkthrough file was not written

  Rule: A broken plan never replaces the plan being walked

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """

    Scenario: A file that does not parse leaves the last valid plan in use
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "waypoints": [
        """
      Then the plan holds the Waypoints, in order:
        | w1 |
        | w2 |
      And the notice is "walkthrough-invalid"
      And the walkthrough failure is "unparsable"

    Scenario: A Waypoint past the end of its file leaves the last valid plan in use
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w9", "file": "src/lib.rs", "line": 41, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """
      Then the plan holds the Waypoints, in order:
        | w1 |
        | w2 |
      And the notice is "walkthrough-invalid"
      And the walkthrough failure names Waypoint "w9" as "line-out-of-range"
      And the walkthrough failure does not name Waypoint "w1"

    Scenario: A Waypoint naming a path outside the workspace leaves the last valid plan in use
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "../elsewhere/plan.rs", "line": 1, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """
      Then the plan holds the Waypoints, in order:
        | w1 |
        | w2 |
      And the walkthrough failure names Waypoint "w1" as "outside-workspace"

    Scenario: A valid plan after a broken one is loaded
      Given the walkthrough file changes to:
        """
        not json
        """
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w3", "waypoints": [
          {"id": "w3", "file": "src/lib.rs", "line": 9, "kind": "code", "text": "Call it here instead", "verdict": null, "ai": false}
        ]}
        """
      Then the plan holds the Waypoints, in order:
        | w3 |
      And the walkthrough has no failure

  Rule: The agent's instructions ship as the walkthrough Skill

    Scenario: The walkthrough Skill ships with Varde
      Given no walkthrough is active
      When Varde starts
      Then the shipped Skill "walkthrough" is on disk
      And the Skill "walkthrough" names the actions "plan", "evaluate", "adjust", "write" and "revert"
      And the Skill "walkthrough" documents the walkthrough file's schema
      And the Skill "walkthrough" asks for Waypoints in the order a developer would build the change
      And the Skill "walkthrough" allows writing source files only on "write", "adjust" and "revert"

    Scenario: Only one Walkthrough exists at a time
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """
      When I run ":guide"
      Then the modal is "adjust-walkthrough"
      And exactly one walkthrough is active
