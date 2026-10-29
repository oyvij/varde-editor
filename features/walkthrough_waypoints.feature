Feature: Walking a Walkthrough's Waypoints in Edit view

  A Walkthrough is walked in Edit view, with the buffer editable, because the developer is the one
  writing the code. When a plan arrives Varde lands on its `checkpoint`, or on its first Waypoint
  when it names none, and every later change to the file lands on the `checkpoint` again: Varde
  follows the file.

  Landing on a code Waypoint for the first time writes its instruction into the file as a real
  comment, in the language's own line-comment token, and leaves the cursor on an empty indented line
  below it in normal mode, so `i` starts typing and `[` and `]` still step. A comment appears only
  when its Waypoint is landed on, never before, and never twice. Varde remembers every comment it
  inserted so Finish can take them out.

  A context Waypoint writes nothing. Its lines carry a blue gutter mark and a bordered box under
  them holds its Markdown, rendered by the Preview, at most 72 columns wide or the pane's width, and
  scrolled with `j` and `k` when it has focus. Landing on it gives the box focus, and `Esc` hands `j`
  and `k` back to the cursor.

  `[` and `]` step to the previous and next Waypoint, and so do the previous and next Chips on the
  editor's Transport. A verdict never blocks a step. Waypoint lines move with edits above them, the
  way a Breakpoint's do. A Waypoint in a file that does not exist yet opens an unsaved buffer at its
  path, and `:w` creates its missing folders inside the workspace and refuses a path outside it.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And "src/plan.rs" holds:
      """
      use std::fs;

      pub struct Plan;

      fn load() {
          let text = fs::read_to_string("plan.json");
      }
      """
    And "src/lib.rs" holds 40 numbered lines
    And "docs/plan.md" holds 10 numbered lines

  Rule: Varde lands where the file says

    Scenario: A plan's arrival lands on its first Waypoint when it names no checkpoint
      Given I submitted the walkthrough "add a plan parser"
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": null, "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 4, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """
      Then the current view is Edit
      And the current Waypoint is "w1"
      And "src/plan.rs" is open in the editor

    Scenario: A later change to the file lands on its checkpoint
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 4, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w2", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 4, "kind": "code", "text": "Parse the plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """
      Then the current Waypoint is "w2"
      And "src/lib.rs" is open in the editor

  Rule: Landing on a code Waypoint writes its instruction as a comment

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 6, "kind": "code", "text": "Parse the text into a Plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/plan.rs", "line": 3, "kind": "code", "text": "Derive Debug", "verdict": null, "ai": false},
          {"id": "w3", "file": "docs/plan.md", "line": 2, "kind": "context", "text": "The **schema** lives here.", "verdict": null, "ai": false}
        ]}
        """

    Scenario: The comment is inserted and the cursor waits on an empty indented line below it
      Then line 6 of "src/plan.rs" reads "    // Parse the text into a Plan"
      And line 7 of "src/plan.rs" holds only its indent
      And line 8 of "src/plan.rs" reads "    let text = fs::read_to_string(\"plan.json\");"
      And the cursor is on line 7 of "src/plan.rs"
      And the cursor is at line 7 column 5
      And the editor mode is normal

    Scenario: A comment is not inserted before its Waypoint is landed on
      Then Waypoint "w2" has no inserted comment
      And line 3 of "src/plan.rs" reads "pub struct Plan;"

    Scenario: Landing on a Waypoint again inserts no second comment
      Given I pressed "]"
      When I press "["
      Then "src/plan.rs" holds the comment "Parse the text into a Plan" once

    Scenario: The comment takes the language's own line-comment token
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w4", "waypoints": [
          {"id": "w4", "file": "scripts/plan.py", "line": 1, "kind": "code", "text": "Read the plan", "verdict": null, "ai": false}
        ]}
        """
      Then line 1 of "scripts/plan.py" reads "# Read the plan"

    Scenario: Varde records every comment it inserted
      When I press "]"
      Then Varde has inserted the comments for Waypoints:
        | w1 |
        | w2 |

  Rule: A context Waypoint explains and writes nothing

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w3", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 6, "kind": "code", "text": "Parse the text into a Plan", "verdict": null, "ai": false},
          {"id": "w3", "file": "src/lib.rs", "line": 4, "kind": "context", "text": "`update` is where **every** plan arrives. It is the one function that changes state, so the plan's arrival is an Event like any other, and the landing that follows it is an Effect described as data and executed at the edge, never performed here.", "verdict": null, "ai": false}
        ]}
        """

    Scenario: A context Waypoint marks its lines and shows its box
      Then line 4 of "src/lib.rs" carries the context mark
      And the context box is shown under line 4
      And the context box renders "every" in bold

    Scenario: A context Waypoint writes nothing to the file
      Then "src/lib.rs" is unchanged
      And the buffer has no unsaved edits
      And no file was written

    Scenario: The context box is no wider than 72 columns
      Given the editor pane is 120 columns wide
      Then the context box is 72 columns wide

    Scenario: The context box is no wider than a narrow pane
      Given the editor pane is 50 columns wide
      Then the context box is 50 columns wide

    Scenario: j scrolls a focused context box
      Given the editor pane is 30 columns wide
      And the context box has focus
      When I press "j"
      Then the context box is scrolled 1 row
      And the cursor is on line 4 of "src/lib.rs"

    Scenario: k scrolls a focused context box back
      Given the editor pane is 30 columns wide
      And the context box has focus
      And I pressed "j"
      When I press "k"
      Then the context box is scrolled 0 rows

    Scenario: Escape hands j and k back to the cursor
      Given the editor pane is 30 columns wide
      And the context box has focus
      And I press "Escape" in the editor
      When I press "j" in the editor
      Then the context box is scrolled 0 rows
      And the cursor is on line 5 of "src/lib.rs"

    Scenario: Escape in insert mode leaves insert mode first
      Given the context box has focus
      And I press "i" in the editor
      When I press "Escape" in the editor
      Then the editor mode is normal
      And the context box has focus

    Scenario: The context box wraps its Markdown inside its width
      Given the editor pane is 30 columns wide
      Then no context box row is wider than 30 columns
      And the context box renders "never performed here."

  Rule: [ and ] step between Waypoints, whatever their verdict

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 6, "kind": "code", "text": "Parse the text into a Plan", "verdict": {"state": "failing", "message": "The error is dropped"}, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """

    Scenario: ] steps on past a failing Waypoint
      When I press "]"
      Then the current Waypoint is "w2"
      And "src/lib.rs" is open in the editor

    Scenario: [ steps back
      Given I pressed "]"
      When I press "["
      Then the current Waypoint is "w1"

    Scenario: ] on the last Waypoint stays there
      Given I pressed "]"
      When I press "]"
      Then the current Waypoint is "w2"

    Scenario: The Transport's next Chip steps on
      When I click the "next" Chip on the editor's border
      Then the current Waypoint is "w2"

    Scenario: The Transport's previous Chip steps back
      Given I pressed "]"
      When I click the "previous" Chip on the editor's border
      Then the current Waypoint is "w1"

    Scenario: Every Walkthrough action is reachable from the keyboard
      Then every walkthrough Chip has a key or a command

  Rule: Waypoint lines move with the developer's edits

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 3, "kind": "code", "text": "Derive Debug", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/plan.rs", "line": 6, "kind": "code", "text": "Parse the text into a Plan", "verdict": null, "ai": false}
        ]}
        """

    Scenario: The landing comment above a later Waypoint moves it down
      Then Waypoint "w2" is on line 8 of "src/plan.rs"

    Scenario: A line opened above a later Waypoint moves it down
      When I press "o" in the editor
      Then Waypoint "w2" is on line 9 of "src/plan.rs"

    Scenario: Stepping lands on the moved line
      Given I press "o" in the editor
      And I leave insert mode
      When I press "]"
      Then the current Waypoint is "w2"
      And the cursor is on line 10 of "src/plan.rs"

    Scenario: The agent's rewrite of the file replaces the moved lines
      Given I press "o" in the editor
      And I leave insert mode
      When the walkthrough file changes to:
        """
        {"version": 1, "status": "active", "checkpoint": "w2", "waypoints": [
          {"id": "w2", "file": "src/plan.rs", "line": 7, "kind": "code", "text": "Parse the text into a Plan", "verdict": null, "ai": false}
        ]}
        """
      Then Waypoint "w2" is on line 7 of "src/plan.rs"

  Rule: A Waypoint in a new file opens an unsaved buffer at its path

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/guide/plan.rs", "line": 1, "kind": "code", "text": "Start the parser", "verdict": null, "ai": false}
        ]}
        """

    Scenario: Landing opens an unsaved buffer at the planned path
      Then "src/guide/plan.rs" is open in the editor
      And the buffer has unsaved edits
      And the file "src/guide/plan.rs" does not exist

    Scenario: :w creates the missing folders inside the workspace
      When I run ":w"
      Then the folder "src/guide" was created
      And the file "src/guide/plan.rs" was written

    Scenario: :w refuses a planned path that resolves outside the workspace
      Given "src/guide" links to the folder "/home/me/projects/elsewhere"
      When I run ":w"
      Then the notice is "outside-workspace"
      And no file was written
      And no folder was created

  Rule: A session in a project workspace survives a restart

    Background:
      Given a walkthrough is active on the plan:
        """
        {"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [
          {"id": "w1", "file": "src/plan.rs", "line": 6, "kind": "code", "text": "Parse the text into a Plan", "verdict": null, "ai": false},
          {"id": "w2", "file": "src/lib.rs", "line": 3, "kind": "code", "text": "Call the parser", "verdict": null, "ai": false}
        ]}
        """

    Scenario: A restart resumes on the last visited Waypoint
      Given I pressed "]"
      When Varde restarts
      Then the walkthrough state is "active"
      And the current Waypoint is "w2"

    Scenario: A finished session does not resume after a restart, though the file still says active
      Given I ran ":guide finish"
      When Varde restarts
      Then no walkthrough is active

    Scenario: In a Bare workspace no session remains after Varde ends
      Given the workspace is a Bare workspace
      When Varde restarts
      Then no walkthrough is active
