Feature: The Strip and the Debug group

  The Strip is the slot along the bottom of the screen. Like the Corner it names one occupant at a
  time — the Shell group, or the Debug group while a Debug session exists — so showing one hides the
  other without stopping anything running in it. Group tabs on its top border say which is up and are
  clicked to switch; a key does the same.

  Starting a Debug session, by any route, brings the Debug group forward so the program's output is
  never printed out of sight; like a pause, it moves what is on screen and never the keyboard.

  Its height is the user's to drag, with or without a session, clamped so that neither the Strip nor
  the area above it can vanish: a drag that left a pane unreachable is a pane with no way back.

  The Debug group holds the Variables beside the Program output, the debugged program's own terminal,
  with a border between them that can be dragged. The Program output can be hidden to give the
  Variables the width without ending it, and what it prints while hidden is marked where the user
  will see it. Layout is asserted as the Strip's rows and the divider's column, never a screen.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the screen is 120 columns by 40 rows
    And a Debug adapter for "rust" is configured
    And the project config is:
      """
      [launch.server]
      adapter = "rust"
      request = "launch"
      args = { program = "target/debug/server" }
      """

  Rule: The Strip's height is dragged, and clamped so nothing vanishes

    Scenario: Dragging the border above the Strip makes it taller
      Given the Strip is 10 rows tall
      When I drag the border above the Strip up 6 rows
      Then the Strip is 16 rows tall

    Scenario: The Strip's height is dragged with no Debug session
      Given no Debug session exists
      And the Strip is 10 rows tall
      When I drag the border above the Strip down 4 rows
      Then the Strip is 6 rows tall

    Scenario: The Strip cannot be dragged away
      Given the Strip is 10 rows tall
      When I drag the border above the Strip down 30 rows
      Then the Strip is at its least height

    Scenario: The area above the Strip cannot be dragged away
      Given the Strip is 10 rows tall
      When I drag the border above the Strip up 40 rows
      Then the area above the Strip is at its least height

    Scenario: A smaller screen clamps the Strip again
      Given the Strip is 30 rows tall
      When the screen is resized to 120 columns by 20 rows
      Then the area above the Strip is at its least height

    Scenario: The Strip's height is remembered per project
      Given the Strip is 10 rows tall
      When I drag the border above the Strip up 6 rows
      Then the project ".varde/state.json" records the Strip as 16 rows tall

  Rule: The Shell group and the Debug group are one occupant each, switched by Group tab or key

    Scenario: With no session the Strip holds the Shell group alone
      Given no Debug session exists
      Then the Group tabs are:
        | Shells |
      And the Strip shows the Shell group

    Scenario: A session adds the Debug group tab
      When a Debug session was started from the Launch configuration "server"
      Then the Group tabs are:
        | Shells |
        | Debug  |

    Scenario: Starting a session from a Launch configuration shows the Debug group
      Given the Strip shows the Shell group
      When I start the Launch configuration "server" from the palette
      Then the Strip shows the Debug group

    Scenario: Starting a session from a Run mark's Debug offer shows the Debug group
      Given "src/lib.rs" is open in the editor holding:
        """
        #[test]
        fn adds() {}
        """
      And the Strip shows the Shell group
      When I choose "debug" on the Run mark on line 2
      Then the Strip shows the Debug group

    Scenario: Attaching shows the Debug group
      Given the global config is:
        """
        [launch.orders]
        adapter = "java"
        request = "attach"
        args = { hostName = "localhost", port = 5005 }
        """
      And a Debug adapter for "java" is configured
      And the Strip shows the Shell group
      When I start the Launch configuration "orders" from the palette
      Then the Strip shows the Debug group

    Scenario: Attaching again shows the Debug group
      Given the global config is:
        """
        [launch.orders]
        adapter = "java"
        request = "attach"
        args = { hostName = "localhost", port = 5005 }
        """
      And a Debug adapter for "java" is configured
      And a Debug session was started from the Launch configuration "orders"
      And the Debug adapter sends the event "terminated"
      And the Strip shows the Shell group
      When the edge reports the port 5005 answers
      Then the Strip shows the Debug group

    Scenario: A session starting leaves the keyboard where it was
      Given the terminal pane has focus
      When I start the Launch configuration "server" from the palette
      Then the terminal pane has focus

    Scenario: The group showing has the lit tab
      Given a Debug session is Paused at "src/main.rs" line 3
      Then the Group tab "Debug" is lit
      And the Group tab "Shells" is not lit

    Scenario: Clicking a Group tab shows that group
      Given a Debug session is Paused at "src/main.rs" line 3
      When I click the Group tab "Shells"
      Then the Strip shows the Shell group

    Scenario: Space then s switches the Strip's group
      Given a Debug session is Paused at "src/main.rs" line 3
      And the Strip shows the Shell group
      When I press "Space"
      And I press "s"
      Then the Strip shows the Debug group

    Scenario: Shells hidden by the Debug group keep running
      Given the terminal holds 2 shells
      And terminal 2 is running a process
      And a Debug session is Paused at "src/main.rs" line 3
      Then the terminal holds 2 shells
      And terminal 2 is running a process

    Scenario: Shell splits never join the Debug group
      Given the terminal holds 2 shells
      And a Debug session is Paused at "src/main.rs" line 3
      When I split the terminal from the command line
      Then the Strip shows the Shell group
      And the Debug group holds the Variables and the Program output

    Scenario: Clicking the Debug Group tab gives the keyboard to the Variables
      Given a Debug session is Running
      And the Strip shows the Shell group
      And terminal 1 has focus
      When I click the Group tab "Debug"
      Then the Variables have focus

  Rule: The border between the Variables and the Program output is dragged

    Scenario: Dragging the border gives the Program output more width
      Given a Debug session is Paused at "src/main.rs" line 3
      And the border between the Variables and the Program output is at column 60
      When I drag that border to column 40
      Then the Program output is 80 columns wide

    Scenario: The border cannot squeeze either side away
      Given a Debug session is Paused at "src/main.rs" line 3
      When I drag the border between the Variables and the Program output to column 0
      Then the Variables are at their least width

    Scenario: The Group tabs stay at the Strip's right edge whatever the border does
      Given a Debug session is Paused at "src/main.rs" line 3
      And the border between the Variables and the Program output is at column 40
      When I click the Group tab "Shells"
      Then the Strip shows the Shell group

  Rule: The Program output is hidden without ending it, and output while hidden is marked

    Scenario: Hiding the Program output gives the Variables the full width
      Given a Debug session is Paused at "src/main.rs" line 3
      When I press "Space"
      And I press "h"
      Then the Program output is hidden
      And the Variables have the Debug group's whole width

    Scenario: A hidden Program output shows a Chip that shows it again
      Given a Debug session is Paused at "src/main.rs" line 3
      And the Program output is hidden
      When I click the "show-output" Chip
      Then the Program output is shown

    Scenario: Output while hidden is marked on the Debug Group tab and on the Chip
      Given a Debug session is Running
      And the Program output is hidden
      When the program prints "panicked at src/main.rs:4"
      Then the Group tab "Debug" is marked as having unseen output
      And the "show-output" Chip is marked as having unseen output

    Scenario: Output while hidden is marked on the Debug Group tab even when the Shell group is up
      Given a Debug session is Running
      And the Strip shows the Shell group
      When the program prints "panicked at src/main.rs:4"
      Then the Group tab "Debug" is marked as having unseen output

    Scenario: Showing the Program output clears the mark
      Given a Debug session is Running
      And the Strip shows the Debug group
      And the Program output is hidden
      And the program prints "panicked at src/main.rs:4"
      When I click the "show-output" Chip
      Then the Group tab "Debug" is not marked as having unseen output

    Scenario: Output while the Program output is on screen marks nothing
      Given a Debug session is Running
      And the Strip shows the Debug group
      When the program prints "hello"
      Then the Group tab "Debug" is not marked as having unseen output

    Scenario: A hidden Program output keeps its size and everything it printed
      Given a Debug session is Running
      And the Strip shows the Debug group
      And the Program output is 60 columns wide
      And the Program output is hidden
      And the program prints "hello"
      When I click the "show-output" Chip
      Then the Program output is 60 columns wide
      And the Program output pty was never stopped

    Scenario: A hidden Program output is never resized to nothing
      Given a Debug session is Running
      And the Program output is hidden
      Then the Program output's pty is at least 2 rows by 1 column

  Rule: The adapter's runInTerminal starts the Program output inside the Debug group

    Scenario: A runInTerminal request starts the Program output
      Given a Debug session was started from the Launch configuration "server"
      When the Debug adapter asks to run "target/debug/server" in a terminal
      Then a Program output pty is asked for running "target/debug/server"
      And no shell is asked for
