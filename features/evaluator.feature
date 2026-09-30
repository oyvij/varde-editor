Feature: The Evaluator

  The Evaluator is the reason Debug sessions exist in Varde: a floating window that runs a Snippet — an
  expression or a whole block — inside the Paused program, in the chosen Frame, with the real
  variables in scope. The user tries out the code they are about to write, and copies it into the
  editor once it works.

  It floats over the editor so the code behind it stays readable and stepping goes on. Its rectangle
  is state: it opens centred the first time and after that where the project last left it, clamped
  onto the screen, and never covers the Paused line. It is moved and resized by mouse or, without a
  modifier, by keyboard.

  The Snippet is edited with Varde's ordinary Vim editing. Normal-mode Enter runs the Selection if
  there is one and the whole Snippet otherwise. The Evaluator output shows what the program printed
  while the Snippet ran and then its value; each run replaces it, and what the Debug adapter refuses
  is shown as the adapter says it, never swallowed and never softened. Varde does not make up for a
  weak adapter (`docs/adr/0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md`).

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the screen is 120 columns by 40 rows
    And a Debug adapter for "rust" is configured
    And "src/main.rs" is open in the editor holding:
      """
      fn main() {
          let orders = load();
          let count = orders.len();
          println!("{}", count);
      }
      """
    And a Debug session is Paused at "src/main.rs" line 3

  Rule: The Evaluator opens from a Hover, a Variables row, or Space then e

    Scenario: Space then e opens it on the expression under the cursor
      Given the cursor is at line 3 column 17
      When I press "Space"
      And I press "e"
      Then the Evaluator is open holding "orders.len()"

    Scenario: Space then e opens it on the Selection
      Given the selection covers "orders.len()" on line 3
      When I press "Space"
      And I press "e"
      Then the Evaluator is open holding "orders.len()"

    Scenario: Opening it asks the adapter nothing
      Given the cursor is at line 3 column 17
      When I press "Space"
      And I press "e"
      Then the Debug adapter was sent no "evaluate" request

    Scenario: The Evaluator leaves the editor editable behind it
      Given the Evaluator is open holding "count"
      When I click line 2 column 5 in the editor
      Then the editor pane has focus
      And the Evaluator is open

    Scenario: The Evaluator stays open while stepping
      Given the Evaluator is open holding "count"
      When the Debug adapter sends the "stopped" event for thread 1 at "src/main.rs" line 4 with reason "step"
      Then the Evaluator is open

  Rule: Its rectangle is state — moved, resized, remembered, clamped, and off the Paused line

    Scenario: It opens centred the first time
      When I open the Evaluator
      Then the Evaluator is centred on the screen

    Scenario: Dragging the title bar moves it
      Given the Evaluator is open at column 20 row 5
      When I drag the Evaluator's title bar 10 columns right
      Then the Evaluator is at column 30 row 5

    Scenario: Dragging a corner resizes it
      Given the Evaluator is open 60 columns by 12 rows
      When I drag the Evaluator's bottom-right corner 10 columns right and 4 rows down
      Then the Evaluator is 70 columns by 16 rows

    Scenario: Dragging a border resizes it
      Given the Evaluator is open 60 columns by 12 rows
      When I drag the Evaluator's right border 6 columns left
      Then the Evaluator is 54 columns by 12 rows

    Scenario: Dragging the border between the Snippet and its output gives the output more room
      Given the Evaluator is open with a Snippet 6 rows tall
      When I drag the border under the Snippet up 2 rows
      Then the Snippet is 4 rows tall

    Scenario: The keyboard moves it without a modifier
      Given the Evaluator is open at column 20 row 5
      And the Evaluator has focus
      When I press "Space"
      And I press "m"
      And I press "l"
      Then the Evaluator is at column 21 row 5

    Scenario: The keyboard resizes it without a modifier
      Given the Evaluator is open 60 columns by 12 rows
      And the Evaluator has focus
      When I press "Space"
      And I press "z"
      And I press "j"
      Then the Evaluator is 60 columns by 13 rows

    Scenario: It reopens where the project last left it
      Given the project ".varde/state.json" records the Evaluator at column 30 row 4 sized 50 by 10
      When I open the Evaluator
      Then the Evaluator is at column 30 row 4

    Scenario: A remembered place off a smaller screen is clamped onto it
      Given the project ".varde/state.json" records the Evaluator at column 200 row 4 sized 50 by 10
      When I open the Evaluator
      Then the Evaluator lies wholly on the screen

    Scenario: A resize clamps it back onto the screen
      Given the Evaluator is open at column 60 row 5
      When the screen is resized to 80 columns by 40 rows
      Then the Evaluator lies wholly on the screen

    Scenario: It never covers the Paused line
      Given the Evaluator is open over editor line 4
      When the Debug adapter sends the "stopped" event for thread 1 at "src/main.rs" line 4 with reason "step"
      Then the Evaluator does not cover the Paused line

    Scenario: Dragging it onto the Paused line keeps it off
      Given the Evaluator is open
      When I drag the Evaluator's title bar onto the Paused line
      Then the Evaluator does not cover the Paused line

  Rule: The Snippet is written with Vim editing and run with Enter, the Run Chip, or Ctrl+Enter

    Background:
      Given the Evaluator is open holding:
        """
        let first = orders[0].id;
        first * 2
        """
      And the Evaluator has focus

    Scenario: Normal-mode Enter runs the whole Snippet in the repl context and the chosen Frame
      When I press "Enter"
      Then the Debug adapter was sent an "evaluate" request in the "repl" context holding:
        """
        let first = orders[0].id;
        first * 2
        """
      And that request names the Frame "main"

    Scenario: Normal-mode Enter with a Selection runs only the Selection
      Given the selection in the Snippet covers "orders[0].id"
      When I press "Enter"
      Then the Debug adapter was sent an "evaluate" request in the "repl" context holding:
        """
        orders[0].id
        """

    Scenario: Enter while inserting is a new line
      Given I press "i"
      When I press "Enter"
      Then the Debug adapter was sent no "evaluate" request

    Scenario: The Run Chip runs the Snippet
      When I click the Evaluator's "run" Chip
      Then the Debug adapter was sent an "evaluate" request in the "repl" context

    Scenario: Ctrl+Enter runs the Snippet while inserting
      Given I press "i"
      When I press "Ctrl+Enter"
      Then the Debug adapter was sent an "evaluate" request in the "repl" context

    Scenario: The Snippet is edited with Vim editing
      When I type "ggdd"
      Then the Snippet is "first * 2"

    Scenario: The Snippet undoes and redoes with the modifier keys
      Given I type "ggdd"
      And I press "Ctrl+z"
      When I press "Ctrl+Shift+z"
      Then the Snippet is "first * 2"

  Rule: The Evaluator output shows what was printed, then the value, and each run replaces it

    Background:
      Given the Evaluator is open holding "orders.len()"
      And the Evaluator has focus

    Scenario: What the program printed while the Snippet ran comes before its value
      Given I press "Enter"
      And the program prints "loading orders"
      When the Debug adapter answers the "evaluate" with the value "3"
      Then the Evaluator output is:
        | printed | loading orders |
        | value   | 3              |

    Scenario: Output printed while the Snippet runs goes to the Evaluator, not only the Program output
      Given I press "Enter"
      When the program prints "loading orders"
      Then the Evaluator output holds the print "loading orders"

    Scenario: The value opens like the Variables
      Given I press "Enter"
      And the Debug adapter answers the "evaluate" with reference 21
      When I open the Evaluator output's value
      Then the Debug adapter was sent a "variables" request for reference 21

    Scenario: Each run replaces the Evaluator output
      Given I press "Enter"
      And the Debug adapter answers the "evaluate" with the value "3"
      And I press "Enter"
      When the Debug adapter answers the "evaluate" with the value "4"
      Then the Evaluator output is:
        | value | 4 |

    Scenario: The adapter's error is shown, never swallowed
      Given I press "Enter"
      When the Debug adapter answers the "evaluate" with the error "no method named `len` found"
      Then the Evaluator output is:
        | error | no method named `len` found |

    Scenario: An inspection expression runs in Rust as in any language
      Given the Evaluator is open holding "orders[0].total * 2"
      And I press "Enter"
      When the Debug adapter answers the "evaluate" with the value "60"
      Then the Evaluator output is:
        | value | 60 |

    Scenario: A Snippet still running is shown as running and can be cancelled
      Given the Debug adapter reported it supports cancelling
      And I press "Enter"
      When I click the Evaluator's "cancel" Chip
      Then the Debug adapter was sent a "cancel" request for that evaluate

    Scenario: A Snippet still running says so
      When I press "Enter"
      Then the Evaluator output is running

    Scenario: The Run Chip is dimmed while the program runs
      When the Debug adapter reports the program continued
      Then the Evaluator's "run" Chip is dimmed

    Scenario: Enter while the program runs sends nothing
      Given the Debug adapter reports the program continued
      When I press "Enter"
      Then the Debug adapter was sent no "evaluate" request

  Rule: Snippets are recalled per project, and the Evaluator closes on Escape, :q, its Close Chip and with the session

    Scenario: Up on an empty Snippet recalls the last one run
      Given the project ".varde/state.json" records the Snippets:
        | orders.len() |
        | count + 1    |
      And the Evaluator is open holding ""
      And the Evaluator has focus
      When I press "Up"
      Then the Snippet is "count + 1"

    Scenario: Up again goes further back, and Down comes forward
      Given the project ".varde/state.json" records the Snippets:
        | orders.len() |
        | count + 1    |
      And the Evaluator is open holding ""
      And the Evaluator has focus
      When I press "Up"
      And I press "Up"
      And I press "Down"
      Then the Snippet is "count + 1"

    Scenario: Up on a Snippet with text moves the cursor as ever
      Given the project ".varde/state.json" records the Snippets:
        | orders.len() |
      And the Evaluator is open holding:
        """
        let a = 1;
        a
        """
      And the Evaluator has focus
      When I press "Up"
      Then the Snippet is:
        """
        let a = 1;
        a
        """

    Scenario: A Snippet that was run is remembered
      Given the Evaluator is open holding "count + 1"
      And the Evaluator has focus
      When I press "Enter"
      Then the project ".varde/state.json" records the Snippet "count + 1"

    Scenario: The Evaluator closes when the session ends, keeping its Snippet
      Given the Evaluator is open holding "count * 3"
      When the Debug adapter sends the event "terminated"
      Then the Evaluator is not open
      And the project ".varde/state.json" records the Snippet "count * 3"

    Scenario: Escape in normal mode closes the Evaluator, keeping its Snippet
      Given the Evaluator is open holding "count * 3"
      And the Evaluator has focus
      When I press "Escape"
      Then the Evaluator is not open
      And the editor pane has focus
      And the project ".varde/state.json" records the Snippet "count * 3"

    Scenario: Escape while inserting only leaves insert mode
      Given the Evaluator is open holding "count * 3"
      And the Evaluator has focus
      And I press "i"
      When I press "Escape"
      Then the Evaluator is open holding "count * 3"

    Scenario: The Close Chip closes the Evaluator, keeping its Snippet
      Given the Evaluator is open holding "count * 3"
      When I click the Evaluator's "close" Chip
      Then the Evaluator is not open
      And the project ".varde/state.json" records the Snippet "count * 3"

    Scenario: :q closes the Evaluator and not the file behind it
      Given the Evaluator is open holding "count * 3"
      And the Evaluator has focus
      When I run ":q"
      Then the Evaluator is not open
      And the current buffer is "src/main.rs"

    Scenario: A working Snippet is copied with the ordinary Selection
      Given the Evaluator is open holding "orders.len()"
      And the Evaluator has focus
      And the selection in the Snippet covers "orders.len()"
      When I copy the selection
      Then the clipboard holds "orders.len()"
