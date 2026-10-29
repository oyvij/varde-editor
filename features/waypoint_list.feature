Feature: The Waypoint list and the Walkthrough's files in the tree

  The Waypoint list is the Corner occupant that shows a Walkthrough's whole plan, the way chapters
  show a story's: its files in plan order, each Waypoint nested under its file and marked `LGTM`,
  open, failing, context or `new`, with `AI` beside it when the agent wrote the code. The current
  Waypoint is lit, and `Enter` lands on the selected one. It opens when a session starts.

  While a session is active, every file the Walkthrough touches is drawn in the file tree in one
  named theme colour, so the developer sees which files the change will touch. The colour says
  nothing about state — that is the Waypoint list's job — and it goes when the session ends.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/plan.rs" holds 20 numbered lines
    And "src/lib.rs" holds 40 numbered lines
    And "src/keys.rs" holds 10 numbered lines

  Rule: The Waypoint list shows the plan

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": {"state": "lgtm"}, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "context", "text": "Why it lands here", "verdict": null, "ai": false},
          {"id": "w3", "file": "src/plan.rs", "line": 2, "kind": "code", "text": "Name the error", "verdict": {"state": "failing", "message": "Unused"}, "ai": false},
          {"id": "w4", "file": "src/guide/new.rs", "line": 1, "kind": "code", "text": "Start the module", "verdict": null, "ai": false},
          {"id": "w5", "file": "src/lib.rs", "line": 9, "kind": "code", "text": "Call the parser", "verdict": {"state": "lgtm"}, "ai": true}
        ]}
        """

    Scenario: The Waypoint list opens in the Corner when a session starts
      Then the Corner holds the Waypoint list

    Scenario: Files are listed in plan order with their Waypoints nested
      Then the Waypoint list shows, in order:
        | file             | waypoint |
        | src/plan.rs      |          |
        |                  | w1       |
        |                  | w3       |
        | src/lib.rs       |          |
        |                  | w2       |
        |                  | w5       |
        | src/guide/new.rs |          |
        |                  | w4       |

    Scenario Outline: Each Waypoint is marked by its status
      Then Waypoint "<id>" is marked "<mark>" in the Waypoint list

      Examples:
        | id | mark    |
        | w1 | lgtm    |
        | w2 | context |
        | w3 | failing |
        | w4 | new     |

    Scenario: A Waypoint the agent wrote is marked AI beside its status
      Then Waypoint "w5" is marked "lgtm" in the Waypoint list
      And Waypoint "w5" is marked "ai" in the Waypoint list
      And Waypoint "w1" is not marked "ai" in the Waypoint list

    Scenario: A Waypoint with no verdict is open
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """
      Then Waypoint "w1" is marked "open" in the Waypoint list

    Scenario: A new file stops being new once it exists
      When a change to "src/guide/new.rs" is seen on disk
      Then Waypoint "w4" is not marked "new" in the Waypoint list

    Scenario: The current Waypoint is lit, and it follows ]
      Given Waypoint "w1" is lit in the Waypoint list
      When I press "]"
      Then Waypoint "w2" is lit in the Waypoint list
      And Waypoint "w1" is not lit in the Waypoint list
      And Waypoint "w2" is selected in the Waypoint list

    Scenario: Enter on a Waypoint lands on its line
      Given the Waypoint list has focus
      And Waypoint "w5" is selected in the Waypoint list
      When I press Enter
      Then the current Waypoint is "w5"
      And the cursor is on line 10 of "src/lib.rs"

  Rule: The tree shows which files the Walkthrough touches

    Scenario: Walkthrough files carry the theme colour and other files do not
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": {"state": "failing", "message": "Unused"}, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": {"state": "lgtm"}, "ai": false}
        ]}
        """
      Then the tree row "src/plan.rs" carries the walkthrough colour
      And the tree row "src/lib.rs" carries the walkthrough colour
      And the tree row "src/keys.rs" does not carry the walkthrough colour

    Scenario: A new file takes the colour once it exists
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/guide/new.rs", "line": 1, "kind": "code", "text": "Start the module", "verdict": null, "ai": false}
        ]}
        """
      When a change to "src/guide/new.rs" is seen on disk
      Then the tree row "src/guide/new.rs" carries the walkthrough colour

    Scenario: With no active session no file carries the colour
      Given no walkthrough is active
      Then no tree row carries the walkthrough colour

    Scenario: The colour goes when the session ends
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 12, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false}
        ]}
        """
      When I run ":guide finish"
      Then no tree row carries the walkthrough colour
      And the Corner does not hold the Waypoint list
