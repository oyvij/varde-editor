Feature: Walking a story step by step

  Walking a Story moves the cursor through its Steps in execution order. The editor pane shows the
  code at each Step's Site, read-only, and a narration band beneath it carries the claim and, when
  the Step has them, its cited values. Everything else the Step holds — why it exists, what flows in
  and out, the nudge — is one keypress away in an overlay, because a Step's prose and its Site do not
  fit on a screen together and the code is what the reviewer came for.

  The Site mark is what makes the code readable as a Step rather than as a screen of code: the Site's
  lines are marked and everything else is flattened to one grey. A cursor on the first line is not
  enough — it says where to start and nothing about how far the claim reaches, and it is not drawn at
  all while an overlay is up, which is exactly when a Step with a Prediction arrives. The mark says
  whether the Site is changed code or context, because a reviewer who cannot tell those apart cannot
  tell what the change did from what it merely passes through.

  Stepping is `n` and `p`. `j` and `k` scroll, meaning here exactly what they mean everywhere else: a
  Site can be taller than the pane, so a reviewer who cannot read around it cannot review it, and a
  key that changes meaning per view is the blind spot the cheatsheet sweep exists to close.

  A Step whose Site no longer holds the text it was written against says so, says what the Site used
  to hold, and stops claiming to describe what is on screen. It is never quietly re-pointed at
  whatever moved into its place — that is how every tool in the prior art degraded silently.

  A Story position is one person's place in a Story. It is disposable and gitignored, and it is
  discarded outright when the Story it belongs to is re-authored, because a position kept across a
  rewrite points at a different claim while claiming to be where you left off.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository
    And "src/keys.rs" holds:
      """
      fn route(k: Key) -> Event {
          match k {
              Key::Char(c) => Event::Type(c),
              other => Event::Raw(other),
          }
      }
      """
    And a story "Keys reach the child" holds the steps:
      | claim                      | file        | side | from | to | text                        |
      | The router matches the key | src/keys.rs | new  | 2    | 2  |     match k {               |
      | The catch-all keeps it     | src/keys.rs | new  | 4    | 4  |         other => Event::Raw(other), |

  Scenario: The step menu is empty before walking anything
    Given I opened Story view
    Then the step menu is empty

  Scenario: Entering a story puts the cursor on the first step's site
    Given I opened Story view
    When I enter the story "Keys reach the child"
    Then the cursor is on line 2 of "src/keys.rs"
    And the band claim is "The router matches the key"

  Scenario: Entering a story shows every step in the menu, the first marked current
    Given I opened Story view
    When I enter the story "Keys reach the child"
    Then the step menu lists:
      | name                        | current |
      | The router matches the key | true    |
      | The catch-all keeps it     | false   |

  Scenario: Stepping forward moves which row the menu marks current
    Given I am walking "Keys reach the child"
    When I press "n"
    Then the step menu lists:
      | name                        | current |
      | The router matches the key | false   |
      | The catch-all keeps it     | true    |

  Scenario: Stepping back moves the menu's current row back with it
    Given I am walking "Keys reach the child"
    And I pressed "n"
    When I press "p"
    Then the step menu lists:
      | name                        | current |
      | The router matches the key | true    |
      | The catch-all keeps it     | false   |

  Scenario: Entering a story from the spine focuses the editor
    Given I opened Story view
    And the file tree pane has focus
    When I enter the story "Keys reach the child"
    Then the editor pane has focus

  Scenario: Entering a story marks the step's site
    Given I opened Story view
    When I enter the story "Keys reach the child"
    Then the site mark covers lines 2 to 2 of "src/keys.rs"
    And the site mark is "changed"

  Scenario: The mark reaches the whole site, not just the line the cursor is on
    Given the step "The router matches the key" covers lines 2 to 4
    And I opened Story view
    When I enter the story "Keys reach the child"
    Then the cursor is on line 2 of "src/keys.rs"
    And the site mark covers lines 2 to 4 of "src/keys.rs"

  Scenario: Code outside the site is dimmed
    Given I am walking "Keys reach the child"
    Then line 2 of "src/keys.rs" is marked
    And line 1 of "src/keys.rs" is dimmed
    And line 3 of "src/keys.rs" is dimmed

  Scenario: Stepping moves the mark with the claim
    Given I am walking "Keys reach the child"
    When I press "n"
    Then the site mark covers lines 4 to 4 of "src/keys.rs"
    And line 2 of "src/keys.rs" is dimmed

  Scenario: Stepping back moves the mark with the claim
    Given I am walking "Keys reach the child"
    And I pressed "n"
    When I press "p"
    Then the site mark covers lines 2 to 2 of "src/keys.rs"
    And line 4 of "src/keys.rs" is dimmed

  Scenario: A site the story only passes through is marked as context
    Given the step "The router matches the key" points at context
    And I opened Story view
    When I enter the story "Keys reach the child"
    Then the site mark is "context"

  Scenario: The mark is drawn under an overlay, which the cursor is not
    Given the step "The router matches the key" carries a prediction
    And I opened Story view
    When I enter the story "Keys reach the child"
    Then the overlay is "prediction"
    And the site mark covers lines 2 to 2 of "src/keys.rs"

  Scenario: Stepping forward moves the cursor and the claim together
    Given I am walking "Keys reach the child"
    When I press "n"
    Then the cursor is on line 4 of "src/keys.rs"
    And the band claim is "The catch-all keeps it"

  Scenario: Stepping back returns to the previous step
    Given I am walking "Keys reach the child"
    And I pressed "n"
    When I press "p"
    Then the cursor is on line 2 of "src/keys.rs"

  Scenario: Stepping past the last step does not leave the story
    Given I am walking "Keys reach the child"
    And I pressed "n"
    When I press "n"
    Then the cursor is on line 4 of "src/keys.rs"
    And I am walking "Keys reach the child"

  Scenario: j scrolls the code, it does not step
    Given I am walking "Keys reach the child"
    When I press "j"
    Then the band claim is "The router matches the key"
    And the editor scrolled down

  Scenario: The arrows move the cursor through the code, they do not step or edit
    Given I am walking "Keys reach the child"
    When I press the Down arrow in the editor
    Then the cursor is on line 3 of "src/keys.rs"
    And the band claim is "The router matches the key"
    And "src/keys.rs" has no unsaved edits

  Scenario: The arrows reach across a line as well as down it
    Given I am walking "Keys reach the child"
    When I press the Right arrow in the editor
    Then the cursor is at line 2 column 2

  Scenario: Moving the cursor leaves the mark where the step put it
    Given I am walking "Keys reach the child"
    When I press the Down arrow in the editor
    Then the site mark covers lines 2 to 2 of "src/keys.rs"
    And line 3 of "src/keys.rs" is dimmed

  Scenario: Escape leaves the story for the spine
    Given I am walking "Keys reach the child"
    When I press "Escape"
    Then the story view state is "spine"
    And no file was opened in the editor

  Scenario Outline: Leaving Story view leaves the story
    Given I am walking "Keys reach the child"
    When I open <view> view
    Then the step menu is empty
    And the band has no claim

    Examples:
      | view   |
      | Edit   |
      | Review |

  Scenario: Backspace does not edit the code while walking
    Given I am walking "Keys reach the child"
    And I press the Right arrow in the editor
    When I press Backspace in the editor
    Then "src/keys.rs" has no unsaved edits

  Scenario: The code pane says it is read-only while walking
    Given I am walking "Keys reach the child"
    Then the editor mode label is "read-only"

  Scenario: Opening the step in Edit view restores the editing mode label
    Given I am walking "Keys reach the child"
    When I press "e"
    Then the editor mode label is "normal"

  Scenario: e opens the step's file in Edit view
    Given I am walking "Keys reach the child"
    When I press "e"
    Then the view is "edit"
    And "src/keys.rs" was opened in the editor

  Scenario: A step's cited values appear in the band
    Given the step "The catch-all keeps it" carries the values:
      | name              | value      | provenance | cite file   | cite line |
      | the unnamed key   | Key::F(13) | literal    | src/keys.rs | 3         |
    And I am walking "Keys reach the child"
    When I press "n"
    Then the band values are:
      | name            | value      | provenance |
      | the unnamed key | Key::F(13) | literal    |

  Scenario: A step with no values shows no values row
    Given I am walking "Keys reach the child"
    Then the band has no values row

  Scenario: A value with nowhere to point is shown as invented
    Given the step "The catch-all keeps it" carries the values:
      | name          | value | provenance |
      | a typical key | Esc   | invented   |
    And I am walking "Keys reach the child"
    When I press "n"
    Then the band values are:
      | name          | value | provenance |
      | a typical key | Esc   | invented   |

  Scenario: g jumps to a cited value's source
    Given the step "The catch-all keeps it" carries the values:
      | name            | value      | provenance | cite file   | cite line |
      | the unnamed key | Key::F(13) | literal    | src/keys.rs | 3         |
    And I am walking "Keys reach the child"
    And I pressed "n"
    When I press "g"
    Then the cursor is on line 3 of "src/keys.rs"

  Scenario: D opens the step's detail
    Given the step "The router matches the key" holds:
      | why                        | flow in       | flow out         | nudge                     |
      | The match is the fork      | a decoded key | one of two events | Raw carries the modifiers |
    And I am walking "Keys reach the child"
    When I press "D"
    Then the overlay is "step-detail"
    And the overlay sections are:
      | claim  |
      | why    |
      | flow   |
      | nudge  |

  Scenario: The detail omits the nudge section when the step has none
    Given the step "The router matches the key" holds:
      | why                   | flow in       | flow out          |
      | The match is the fork | a decoded key | one of two events |
    And I am walking "Keys reach the child"
    When I press "D"
    Then the overlay sections are:
      | claim |
      | why   |
      | flow  |

  Scenario: D closes the step's detail again
    Given I am walking "Keys reach the child"
    And I pressed "D"
    When I press "D"
    Then the overlay is "none"
    And I am walking "Keys reach the child"

  Scenario: Escape closes the step's detail without leaving the story
    Given I am walking "Keys reach the child"
    And I pressed "D"
    When I press "Escape"
    Then the overlay is "none"
    And I am walking "Keys reach the child"

  Scenario Outline: A site that no longer holds its text marks the step stale
    Given "src/keys.rs" <change>
    And I opened Story view
    When I enter the story "Keys reach the child"
    Then step 2 is stale as "<kind>"

    Examples:
      | change                             | kind              |
      | is gone                            | file-missing      |
      | holds only 2 lines                 | range-out-of-bounds |
      | now holds different text at line 4 | text-changed      |

  Scenario: Editing the buffer under a step makes it stale
    Given I am walking "Keys reach the child"
    And step 2 is not stale
    When I change line 4 of "src/keys.rs" in the editor
    Then step 2 is stale as "text-changed"

  Scenario: Whitespace-only reformatting does not mark a step stale
    Given I am walking "Keys reach the child"
    When I re-indent line 4 of "src/keys.rs" in the editor
    Then step 2 is not stale

  Scenario: A stale step warns before it is read
    Given "src/keys.rs" now holds different text at line 4
    And I opened Story view
    When I walk to step 2 of "Keys reach the child"
    Then the band warns that the step is stale
    And the band claim is "The catch-all keeps it"

  Scenario: A stale step's detail shows what the site used to hold
    Given "src/keys.rs" now holds different text at line 4
    And I walked to step 2 of "Keys reach the child"
    When I press "D"
    Then the overlay shows the site's stored text
    And the overlay shows what the site holds now

  Scenario: A stale step can still be commented on
    Given "src/keys.rs" now holds different text at line 4
    And I walked to step 2 of "Keys reach the child"
    When I press "c"
    Then the modal is "comment"

  Scenario: An old-side site is immune to the working tree moving
    Given the step "The router matches the key" points at the old side
    And I am walking "Keys reach the child"
    When I change line 2 of "src/keys.rs" in the editor
    Then step 1 is not stale

  Scenario: An old-side site says it cannot show the old text rather than showing the new
    Given the step "The router matches the key" points at the old side
    And I opened Story view
    When I enter the story "Keys reach the child"
    Then the step view state is "old-side-not-shown"
    And no site mark is drawn
    And the band claim is "The router matches the key"

  Scenario: An old-side step can still be commented on
    Given the step "The router matches the key" points at the old side
    And I am walking "Keys reach the child"
    When I press "c"
    Then the modal is "comment"

  Scenario: Stepping off an old-side site draws the mark again
    Given the step "The router matches the key" points at the old side
    And I am walking "Keys reach the child"
    When I press "n"
    Then the step view state is "shown"
    And the site mark covers lines 4 to 4 of "src/keys.rs"

  Scenario: Committing an uncommitted range moves the base under an old-side site
    Given the story set's range is "HEAD..worktree"
    And the step "The router matches the key" points at the old side
    And "src/keys.rs" now holds different text at line 2
    And I am walking "Keys reach the child"
    When the working tree is committed
    Then step 1 is stale as "text-changed"

  Scenario: A comment records the story and step it was made against
    Given I am walking "Keys reach the child"
    When I comment "ISSUE" on the current step with "the catch-all swallows the modifiers"
    Then the review holds a comment:
      | file        | from | to | type  | story                | step |
      | src/keys.rs | 2    | 2  | ISSUE | Keys reach the child | 1    |

  Scenario: A comment on a step of a Story set for a Range does not claim the working tree
    Given I am walking "Keys reach the child"
    When I comment "ISSUE" on the current step with "the catch-all swallows the modifiers"
    Then the comment's revision is not "worktree"

  Scenario: A comment made while walking stays visible under its line
    Given I am walking "Keys reach the child"
    When I comment "ISSUE" on the current step with "the catch-all swallows the modifiers"
    Then the code shows a comment on line 2 of "src/keys.rs"

  Scenario: A step's comment is still there when the step is walked back to
    Given I am walking "Keys reach the child"
    And I commented "ISSUE" on the current step with "the catch-all swallows the modifiers"
    And I pressed "n"
    When I press "p"
    Then the code shows a comment on line 2 of "src/keys.rs"

  # Walking back to the step would pass an implementation that only ever shows
  # the *current* Step's comments. Standing somewhere else is what catches it:
  # the rows answer to the file on screen, not to where the walk is standing.
  Scenario: A comment stays under its line once the walk has moved past its step
    Given I am walking "Keys reach the child"
    And I commented "ISSUE" on the current step with "the catch-all swallows the modifiers"
    When I press "n"
    Then the code shows a comment on line 2 of "src/keys.rs"

  Scenario: Walking the remainder steps through bare locations
    Given "src/mouse.rs" has an unclaimed hunk
    And I opened Story view
    When I enter the remainder
    Then the cursor is in "src/mouse.rs"
    And the band has no claim
    And no prediction is offered

  Scenario: Entering the remainder from the spine focuses the editor
    Given "src/mouse.rs" has an unclaimed hunk
    And I opened Story view
    And the file tree pane has focus
    When I enter the remainder
    Then the editor pane has focus

  Scenario: The remainder marks the whole unclaimed hunk
    Given "src/mouse.rs" has an unclaimed hunk covering lines 10 to 13
    And I opened Story view
    When I enter the remainder
    Then the site mark covers lines 10 to 13 of "src/mouse.rs"
    And the site mark is "changed"

  Scenario: The step menu is empty while walking the remainder, which has no names
    Given "src/mouse.rs" has an unclaimed hunk
    And I opened Story view
    When I enter the remainder
    Then the step menu is empty

  Scenario: The code surface slides sideways on the same gesture a diff answers
    Given the screen is 24 rows by 100 columns
    And I am walking "Keys reach the child"
    When I press "l" in the editor
    Then the editor view starts at column 9

  Scenario: A sideways swipe slides the code surface the same way the gesture does
    Given the screen is 24 rows by 100 columns
    And I am walking "Keys reach the child"
    When I scroll right with the pointer over the editor pane
    Then the editor view starts at column 9

  Scenario: Zero brings a slid code surface home
    Given the screen is 24 rows by 100 columns
    And I am walking "Keys reach the child"
    And I press "l" in the editor
    When I press "0" in the editor
    Then the editor view starts at column 1

  Rule: Arriving at a step frames its site

    A Site is a range, and arriving at one is about the range rather than about the cursor. The
    cursor stays on the Site's first line — that is where a comment starts, and where `V` and `c`
    have always begun — and the *scroll* is set on arrival so the Site opens downward into the pane.
    Left to the ordinary clamp, a downward jump puts its target on the bottom row, so every Step
    began with the first line of a claim whose remaining lines were below the fold.

    Framing counts rows, not lines. A comment row sits between two lines, so a frame computed in
    lines would sit one row off per comment above the Site.

    A Site shorter than the pane keeps a little context above it; one taller than the pane is framed
    from its top, because a claim is read downward from the line it starts on.

    Background:
      Given the screen is 24 rows by 100 columns
      And "src/long.rs" holds 40 numbered lines
      And a story "The long way round" holds the steps:
        | claim                   | file        | side | from | to | text    |
        | The claim in the middle | src/long.rs | new  | 20   | 20 | line 20 |
        | The claim further down  | src/long.rs | new  | 34   | 34 | line 34 |
      And the step "The claim in the middle" covers lines 20 to 24
      And the step "The claim further down" covers lines 34 to 35
      And I opened Story view

    Scenario: Entering a story shows the whole of its first site
      When I enter the story "The long way round"
      Then the whole site is in view
      And the cursor is on line 20 of "src/long.rs"

    Scenario: The frame leaves a little context above the site
      When I enter the story "The long way round"
      Then the editor view starts at row 18

    Scenario: A site taller than the pane is framed from its top
      Given the step "The claim in the middle" covers lines 20 to 39
      When I enter the story "The long way round"
      Then the editor view starts at row 20
      And the cursor is on line 20 of "src/long.rs"

    Scenario: A site at the top of the file does not scroll above the first row
      Given the step "The claim in the middle" covers lines 1 to 3
      When I enter the story "The long way round"
      Then the editor view starts at row 1

    Scenario: Stepping on frames the step it arrives at
      Given I am walking "The long way round"
      When I press "n"
      Then the whole site is in view
      And the cursor is on line 34 of "src/long.rs"

    Scenario: Stepping back frames the site the same way stepping on does
      Given I am walking "The long way round"
      And I pressed "n"
      When I press "p"
      Then the whole site is in view
      And the editor view starts at row 18

    Scenario: A comment above the site is counted in the frame
      Given I am walking "The long way round"
      And I commented "NOTE" on the current step with "why here"
      When I press "n"
      Then the whole site is in view
      And the editor view starts at row 33

    Scenario: Walking the remainder frames its hunk the same way
      Given the working tree is committed
      And "src/mouse.rs" has an unclaimed hunk covering lines 20 to 26
      When I enter the remainder
      Then the whole site is in view
      And the editor view starts at row 18

    Scenario: A jump to a citation is not an arrival, even landing on the site's line number
      Given "src/cited.rs" holds 40 numbered lines
      And the step "The claim in the middle" carries the values:
        | name         | value | provenance | cite file    | cite line |
        | the constant | 2     | literal    | src/cited.rs | 20        |
      And I am walking "The long way round"
      When I press "g"
      Then the cursor is on line 20 of "src/cited.rs"
      And the editor view starts at row 12

  Rule: d lays the range's diff over the site

    A changed Site says the change reached these lines, and nothing about which of them are new. `d`
    toggles the Range's own diff over the Site: its added lines are marked as added, and the lines
    the change removed come back as rows of their own, where they were deleted. It is a toggle and
    not a hold, because a key's release is an event stock tmux and most terminals never send — a
    held gesture would silently not exist for whoever is on one.

    The diff is the Range's, read from its two revisions, and never the buffer's or the last
    commit's. Only the Site gets it: code outside the Site is drawn as it always is. The toggle is
    part of where the reviewer stands in a Story, so it survives stepping and is gone once the Story
    is left.

    Background:
      Given "src/keys.rs" held:
        """
        fn route(key: Key) -> Event {
            match k {
                Key::Char(c) => Event::Char(c),
                Key::Esc => Event::Escape,
                other => Event::Raw(other),
            }
        }
        """
      And "src/keys.rs" now holds:
        """
        fn route(k: Key) -> Event {
            match k {
                Key::Char(c) => Event::Type(c),
                other => Event::Raw(other),
            }
        }
        """
      And the step "The router matches the key" covers lines 2 to 4

    Scenario: d marks the site's added lines and shows its removed lines as rows
      Given I am walking "Keys reach the child"
      When I press "d"
      Then line 3 of "src/keys.rs" is marked as added
      And line 2 of "src/keys.rs" is not marked as added
      And line 4 of "src/keys.rs" is not marked as added
      And the code shows the removed rows:
        | under | text                                    |
        | 2     |         Key::Char(c) => Event::Char(c), |
        | 2     |         Key::Esc => Event::Escape,      |

    Scenario: d again returns the site to its plain mark
      Given I am walking "Keys reach the child"
      And I pressed "d"
      When I press "d"
      Then line 3 of "src/keys.rs" is not marked as added
      And the code shows no removed rows
      And the site mark is "changed"

    Scenario: Code outside the site gains no diff while the diff is shown
      Given I am walking "Keys reach the child"
      When I press "d"
      Then line 1 of "src/keys.rs" is not marked as added
      And the code shows the removed rows:
        | under | text                                    |
        | 2     |         Key::Char(c) => Event::Char(c), |
        | 2     |         Key::Esc => Event::Escape,      |

    Scenario: An old-side site shows the lines it removed
      Given the step "The router matches the key" covers lines 3 to 4
      And the step "The router matches the key" points at the old side
      And I am walking "Keys reach the child"
      When I press "d"
      Then the code shows the removed rows:
        | under | text                                    |
        | 2     |         Key::Char(c) => Event::Char(c), |
        | 2     |         Key::Esc => Event::Escape,      |

    Scenario: d on a context step draws nothing and edits nothing
      Given the step "The router matches the key" points at context
      And I am walking "Keys reach the child"
      When I press "d"
      Then line 3 of "src/keys.rs" is not marked as added
      And the code shows no removed rows
      And "src/keys.rs" has no unsaved edits

    Scenario: The diff stays shown while stepping
      Given the step "The catch-all keeps it" covers lines 2 to 4
      And I am walking "Keys reach the child"
      And I pressed "d"
      When I press "n"
      Then line 3 of "src/keys.rs" is marked as added

    Scenario: Leaving the story hides the diff for the next walk
      Given I am walking "Keys reach the child"
      And I pressed "d"
      And I pressed "Escape"
      When I enter the story "Keys reach the child"
      Then line 3 of "src/keys.rs" is not marked as added
      And the code shows no removed rows

    Scenario: The diff is the range's, not the buffer's
      Given I am walking "Keys reach the child"
      And I pressed "d"
      When I change line 3 of "src/keys.rs" in the editor
      Then line 3 of "src/keys.rs" is marked as added

    Scenario Outline: The diff is shown over a committed range and an uncommitted one alike
      Given the story set's range is "<range>"
      And I am walking "Keys reach the child"
      When I press "d"
      Then line 3 of "src/keys.rs" is marked as added

      Examples:
        | range          |
        | main..HEAD     |
        | HEAD..worktree |
