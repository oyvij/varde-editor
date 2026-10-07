Feature: The command palette

  A brief modal state, not a view of its own. It opens a palette listing the panes to go
  to, the views to switch to and the commands that apply anywhere, the user picks one
  letter, and it closes.

  The list is grouped, because it grew: a flat list of everything reads as a heap, and the
  grouping is also what makes `Editor` and `Edit` legible as the different questions they
  are — one goes to a pane and leaves the view alone, the other changes the view.

  A `:` command that applies anywhere is here too, because a palette you open with one
  gesture is discoverable and a colon-prefixed command line is not. One that asks something
  of the buffer or the review in front of you is not: Copy, Write and Submit were listed
  and are not, since `C-c`, `:w` and `:submit` belong to the view that answers them and the
  key box advertises each one there.

  It is entered by double-tapping Ctrl on terminals that report modifier key events (the
  Kitty keyboard protocol — Ghostty, kitty, foot, WezTerm). Ctrl+Space always works as well,
  so there is one way in that is the same everywhere and a way through if the double-tap
  misfires. Where a bare Ctrl press is not reported, a double-tapped Escape opens it
  from a hosted pane instead: on that terminal Option is not Alt and Ctrl+Space belongs
  to the child, so it is the only gesture left.

  Background:
    Given the terminal reports modifier key events
    And the double-tap window is 300 ms
    And the current view is Edit

  Scenario: Double-tapping Ctrl opens the view palette
    When I press Ctrl and press Ctrl again after 120 ms
    Then the view palette is shown
    And the view palette offers:
      | group   | key | entry           |
      | Panes   | o   | Editor          |
      | Panes   | g   | Buffers         |
      | Panes   | d   | Files           |
      | Panes   | t   | Terminal        |
      | Panes   | k   | Risk            |
      | Panes   | y   | Cursor history  |
      | Panes   | b   | Breakpoints     |
      | Panes   | i   | Diagnostics     |
      | Panes   | m   | Merge conflicts |
      | Panes   | a   | AI              |
      | Panes   | l   | Tall            |
      | Views   | e   | Edit            |
      | Views   | r   | Review          |
      | Views   | s   | Story           |
      | Views   | w   | Knowledge       |
      | Project | f   | Find            |
      | Project | v   | Tools           |
      | Project | n   | Launch          |
      | Project | c   | Collapse        |
      | Project | j   | Skills          |
      | Help    | h   | Keys            |
      | Help    | u   | Update          |
      |         | q   | Quit            |

  Scenario: Two Ctrl presses too far apart are not a double-tap
    When I press Ctrl and press Ctrl again after 450 ms
    Then the view palette is not shown
    And the current view is Edit

  Scenario: A single Ctrl press does nothing on its own
    When I press Ctrl
    Then the view palette is not shown

  Scenario: Terminals without modifier key reporting use the fallback binding
    Given the terminal does not report modifier key events
    When I press "Ctrl+Space"
    Then the view palette is shown

  Scenario: The fallback binding works where the double-tap works too
    Given the terminal reports modifier key events
    When I press "Ctrl+Space"
    Then the view palette is shown

  Scenario: The fallback binding opens the palette from a hosted pane
    Given the terminal pane has focus
    When I press the key "Ctrl+Space"
    Then the view palette is shown
    And the terminal received nothing

  Scenario: The palette focuses the file tree without leaving the view
    Given the terminal pane has focus
    And the view palette is shown
    When I press "d"
    Then the file tree pane has focus
    And the current view is Edit

  Scenario: Choosing Review renders the review view
    Given the view palette is shown
    When I press "r"
    Then the current view is Review
    And the view palette is not shown

  Scenario: Choosing Review lands on the first changed file, like opening it directly
    Given the project is a git repository
    And the working tree contains:
      | path        | git status |
      | src/tree.js | modified   |
    And the view palette is shown
    When I press "r"
    Then the tree selection is "src/tree.js"
    And the file tree pane has focus

  Scenario: Choosing Story renders the story view
    When I press Ctrl and press Ctrl again after 120 ms
    And I press "s"
    Then the current view is Story
    And the view palette is not shown

  Scenario: Choosing Edit from Review renders the edit view
    Given the current view is Review
    And the view palette is shown
    When I press "e"
    Then the current view is Edit
    And the view palette is not shown

  Scenario: Escape cancels without changing the view
    Given the view palette is shown
    When I press "Escape"
    Then the view palette is not shown
    And the current view is Edit

  Scenario: The palette entries can be clicked
    Given the view palette is shown
    When I click the palette entry "Review"
    Then the current view is Review
    And the view palette is not shown

  Scenario: The palette goes to the AI pane
    Given no AI session is running in the AI pane
    And the view palette is shown
    When I press "a"
    Then the AI pane has focus
    And no new AI session was started

  Scenario: The palette goes to the terminal pane
    Given the view palette is shown
    When I press "t"
    Then the terminal pane has focus

  Scenario: The palette goes to the editor pane from the view it is already showing
    Given the AI pane has focus
    And the view palette is shown
    When I press "e"
    Then the editor pane has focus
    And the current view is Edit

  Scenario: The editor entry goes to the pane without leaving the view
    Given the current view is Review
    And the file tree pane has focus
    And the view palette is shown
    When I press "o"
    Then the editor pane has focus
    And the current view is Review
    And the view palette is not shown

  Scenario: The buffer's own commands are not the palette's
    Given "src/tree.js" is open in the editor with unsaved edits
    And the view palette is shown
    When I press "x"
    Then "src/tree.js" has unsaved edits
    And the view palette is shown

  Scenario: The palette can quit
    Given the view palette is shown
    When I press "q"
    Then Varde exits

  Scenario: An unrecognised key is ignored and the palette stays open
    Given the view palette is shown
    When I press "z"
    Then the view palette is shown
    And the current view is Edit

  Scenario: Choosing the view already showing re-renders nothing
    Given the view palette is shown
    When I press "e"
    Then the view palette is not shown
    And the current view is Edit
    And the view was not re-rendered
