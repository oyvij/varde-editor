Feature: Predictions make the reviewer commit to a reason

  At a Step the author chose, a Prediction asks *why* — why the code is written this way and not the
  obvious other way. Three reasons to choose between, one keypress, never graded and never blocking.
  It is never a bet on which line runs next: that is trivia, and the prior art measured it at zero
  discrimination.

  Three choices is required, not a convention. Two is a coin flip, which makes a correct answer
  meaningless; four thins the distractors, and a bad distractor costs more trust than the question
  earns. Every prediction three hosted CLIs wrote unprompted used exactly three.

  A wrong pick shows that choice's feedback and leaves the choices up, so you may pick again. Being
  told why a reason is wrong is exactly the information needed to choose better, and revealing the
  answer on the first wrong pick removes any reason to think afterwards. Only a correct pick replaces
  the choices.

  The Story position records that a Prediction was put to the reviewer, never which choice they picked.
  A stored history of wrong answers is a score by another name, and this is a senior reviewer reading
  a colleague's change, not a learner being assessed.

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
      | claim                      | file        | side | from | to | text          |
      | The router matches the key | src/keys.rs | new  | 2    | 2  |     match k { |
      | The catch-all keeps it     | src/keys.rs | new  | 4    | 4  |         other => Event::Raw(other), |
    And the step "The catch-all keeps it" asks "Why a catch-all rather than a named variant?":
      | choice | correct | feedback                                            |
      | Because no enum can name every key the crate grows | yes | Right — the narrow enum is the bug this replaced. |
      | Because named variants are slower to match         | no  | Matching cost is not why; the enum dropped keys.  |
      | Because the router forwards it unchanged anyway    | no  | It does, but that is the consequence, not the reason. |

  Scenario: Arriving at a prediction step puts the question up
    Given I am walking "Keys reach the child"
    When I press "n"
    Then the overlay is "prediction"
    And the overlay offers 3 choices

  Scenario: A step with no prediction shows no overlay
    Given I am walking "Keys reach the child"
    Then the overlay is "none"

  Scenario: A wrong pick shows its feedback and leaves the choices up
    Given I am walking "Keys reach the child"
    And I pressed "n"
    When I press "2"
    Then the overlay shows the feedback for choice 2
    And the overlay offers 3 choices
    And the overlay does not show the correct choice

  Scenario: A wrong pick may be followed by another
    Given I am walking "Keys reach the child"
    And I pressed "n"
    And I pressed "2"
    When I press "1"
    Then the overlay shows the feedback for choice 1
    And the overlay offers no choices

  Scenario: A correct pick replaces the choices with its feedback
    Given I am walking "Keys reach the child"
    And I pressed "n"
    When I press "1"
    Then the overlay shows the feedback for choice 1
    And the overlay offers no choices

  Scenario: Answering never blocks the walk
    Given I am walking "Keys reach the child"
    And I pressed "n"
    When I press "n"
    Then the overlay is "none"
    And I am walking "Keys reach the child"

  Scenario: The story position never records which choice was picked
    Given I am walking "Keys reach the child"
    And I pressed "n"
    When I press "2"
    Then the story position records step 2 as put
    And the story position holds no choice

  Scenario: A prediction already put is not asked again
    Given I am walking "Keys reach the child"
    And I pressed "n"
    And I pressed "1"
    And I pressed "Escape"
    When I enter the story "Keys reach the child"
    Then the overlay is "none"

  Scenario: A prediction skipped is not asked again either
    Given I am walking "Keys reach the child"
    And I pressed "n"
    And I pressed "n"
    And I pressed "Escape"
    When I enter the story "Keys reach the child"
    Then the overlay is "none"

  Scenario: Stepping back to an already-put prediction does not ask it again
    Given I am walking "Keys reach the child"
    And I pressed "n"
    And I pressed "Escape"
    And I pressed "p"
    When I press "n"
    Then the overlay is "none"

  Scenario: A correct pick cannot be undone by picking again
    Given I am walking "Keys reach the child"
    And I pressed "n"
    And I pressed "1"
    When I press "2"
    Then the overlay shows the feedback for choice 1
    And the overlay offers no choices

  Scenario: A prediction re-authored is put afresh
    Given I am walking "Keys reach the child"
    And I pressed "n"
    And I pressed "1"
    And I pressed "Escape"
    When the story set is re-authored
    And I walk to step 2 of "Keys reach the child"
    Then the overlay is "prediction"

  Scenario Outline: A prediction that does not offer exactly three choices is invalid
    Given authoring has begun for "main..HEAD"
    When a story artifact arrives whose prediction offers <count> choices
    Then the story view state is "story-artifact-invalid"
    And the spine is empty

    Examples:
      | count |
      | 2     |
      | 4     |
