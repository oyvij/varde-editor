Feature: The AI pane

  The AI pane runs whatever CLI `ai.command` names. Submitting a review starts
  one if none is running, but that cannot be the only way in — you should be
  able to just start it and talk to it.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the effective setting "ai.command" is "claude"

  Scenario: With nothing running, the pane asks which CLI to start
    Given no AI session is running in the AI pane
    Then the AI pane is asking which CLI to start

  Scenario: Once a session runs, the pane stops asking
    Given an AI session is running in the AI pane
    Then the AI pane is not asking which CLI to start

  Scenario: When the AI exits, the pane asks again
    Given an AI session is running in the AI pane
    When the AI session exits
    Then the AI pane is asking which CLI to start

  Scenario: A new CLI can be started after the old one exits
    Given an AI session is running in the AI pane
    And the AI session exits
    When I start the AI with "opencode"
    Then an AI session was started with "opencode"

  Scenario: A CLI that cannot be started leaves the pane asking again
    Given no AI session is running in the AI pane
    And the AI CLI cannot be started
    When I start the AI with "nosuchcli"
    Then the AI pane is asking which CLI to start

  Scenario: Keys do not reach a session that never started
    Given no AI session is running in the AI pane
    And the AI CLI cannot be started
    And I start the AI with "nosuchcli"
    And the AI pane has focus
    When I type "hello"
    Then the AI received nothing
    And the terminal received nothing

  Scenario: Another CLI can be started after one failed to start
    Given no AI session is running in the AI pane
    And the AI CLI cannot be started
    And I start the AI with "nosuchcli"
    And the AI CLI can be started
    When I start the AI with "opencode"
    Then the AI was started again with "opencode"
    And the reviewer is not told the AI is already running

  Scenario: Typing with no session running does not reach the shell
    Given no AI session is running in the AI pane
    And the AI pane has focus
    When I type "claude"
    Then the terminal received nothing

  Scenario: Starting the AI runs the configured command
    Given no AI session is running in the AI pane
    When I start the AI
    Then an AI session was started with "claude"
    And the AI pane has focus

  Scenario: Starting the AI when one is running just focuses it
    Given an AI session is running in the AI pane
    When I start the AI
    Then no new AI session was started
    And the AI pane has focus

  Scenario: Starting a named CLI runs that one
    Given no AI session is running in the AI pane
    When I start the AI with "opencode"
    Then an AI session was started with "opencode"
    And the AI pane has focus

  Scenario: The chosen CLI is remembered for next time
    Given no AI session is running in the AI pane
    When I start the AI with "opencode"
    Then the remembered AI command is "opencode"

  Scenario: Starting a different CLI while one runs is refused
    Given an AI session is running in the AI pane
    When I start the AI with "opencode"
    Then no new AI session was started
    And the reviewer is told the AI is already running

  Scenario: Forcing replaces the running session
    Given an AI session is running in the AI pane
    When I force the AI to "opencode"
    Then the AI session was stopped
    And an AI session was started with "opencode"

  Scenario: A submitted review goes to whichever CLI is running
    Given the project is a git repository
    And "src/tree.js" is changed at revision "a3f9c1"
    And no AI session is running in the AI pane
    And I start the AI with "opencode"
    And I add an ISSUE on "src/tree.js" lines 1 to 2 saying "unquoted path"
    And I submit the review
    And I confirm the submission
    When the AI session is ready for input
    Then only one AI session was started
    And the prompt was submitted to the AI

  Scenario: Submitting with nothing running starts the remembered CLI
    Given the project is a git repository
    And "src/tree.js" is changed at revision "a3f9c1"
    And the remembered AI command is "opencode"
    And no AI session is running in the AI pane
    And I add an ISSUE on "src/tree.js" lines 1 to 2 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then an AI session was started with "opencode"

  Scenario: Typing goes to the AI once it has focus
    Given an AI session is running in the AI pane
    And I start the AI
    When I type "hello"
    Then the AI received "hello"

  Scenario: Option+Enter reaches the session with its modifier intact
    Given an AI session is running in the AI pane
    And the AI pane has focus
    When I press the key "Alt+Enter"
    Then the AI received the bytes "\e\r"

  Scenario: A multi-line paste reaches the session as one paste, marked as one
    Given an AI session is running in the AI pane
    And the AI pane has focus
    And the AI program asked for bracketed paste
    When I paste "first\nsecond"
    Then the AI received the bytes "\e[200~first\nsecond\e[201~"

  Scenario: A session that did not ask for paste markers gets the text bare
    Given an AI session is running in the AI pane
    And the AI pane has focus
    When I paste "first\nsecond"
    Then the AI received the bytes "first\nsecond"

  Scenario: A paste cannot smuggle the end of its own bracketing
    Given an AI session is running in the AI pane
    And the AI pane has focus
    And the AI program asked for bracketed paste
    When I paste "safe\e[201~rm -rf /"
    Then the AI received the bytes "\e[200~saferm -rf /\e[201~"

  Scenario: Pasting with no session running does not reach the shell
    Given no AI session is running in the AI pane
    And the AI pane has focus
    When I paste "claude"
    Then the terminal received nothing
    And the AI received nothing

  Scenario: A reserved key moves focus rather than reaching the session
    Given an AI session is running in the AI pane
    And the AI pane has focus
    When I press the key "Alt+h"
    Then the AI received nothing
    And the editor pane has focus

  Rule: The pane can take the whole height down the right-hand edge

    Beside the editor with the terminal spanning the width beneath it is the
    default and the right one most of the time. But an AI CLI is a long
    conversation, and reading it two rows at a time is the wrong shape: the
    pane can take the whole height instead, and the terminal gives up the
    width rather than the AI pane giving up rows.

    Scenario: The AI pane takes the whole height
      Given the screen is 26 rows by 120 columns
      When I make the AI pane tall from the command line
      Then the AI pane spans the whole height
      And the terminal pane ends where the AI pane starts

    Scenario: The shape is reachable from the pane it changes
      Given the screen is 26 rows by 120 columns
      And an AI session is running in the AI pane
      And the AI pane has focus
      And the view palette is shown
      When I press the key "l"
      Then the AI pane spans the whole height
      And the AI pane has focus
      And the AI received nothing

    Scenario: Asking again puts it back beside the editor
      Given the screen is 26 rows by 120 columns
      And I make the AI pane tall from the command line
      When I make the AI pane tall from the command line
      Then the AI pane stops above the terminal
      And the terminal pane spans the whole width

    Scenario: A tall pane keeps the width its edge was dragged to
      Given the screen is 26 rows by 120 columns
      And I drag the AI pane's edge to column 80
      When I make the AI pane tall from the command line
      Then the AI pane is 40 columns wide
      And the AI pane spans the whole height

    Scenario: A tall pane is remembered per project
      Given the screen is 26 rows by 120 columns
      And I make the AI pane tall from the command line
      When Varde starts in the project
      Then the AI pane spans the whole height

  Rule: What you are looking at can be put in the AI's prompt without sending it

    Asking an AI about the code in front of you starts with getting the code
    into its prompt, and retyping it is the step nobody takes. `:inject` puts
    the selection there — from any pane, because the terminal's error message
    is as worth asking about as the editor's line — and leaves the Enter to
    you, so you can say what you want about it first. With nothing selected
    the cursor's line is enough of a pointer. The `▶ AI Inject` Chip sits on
    the border of each pane it injects from — the editor and the terminal —
    with a word beside its glyph, because a command nobody can see is a
    command nobody uses (ADR 0027). The AI pane's border carries Skills
    instead.

    Scenario: The selected text goes into the prompt unsubmitted
      Given an AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """
        run("unquoted path")
        """
      And I drag across "unquoted path" in the editor pane
      When I run ":inject"
      Then the AI received the bytes "unquoted path"
      And the AI pane has focus

    Scenario: With nothing selected the cursor's line goes in
      Given an AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """
        one two
        three four
        """
      And I press "j" in the editor
      When I run ":inject"
      Then the AI received the bytes "three four"

    Scenario: A selection in the terminal goes in the same way
      Given an AI session is running in the AI pane
      And the terminal shows:
        """
        bash-5.3$ ls
        """
      And I drag across "bash-5.3$ ls" in the terminal pane
      When I run ":inject"
      Then the AI received the bytes "bash-5.3$ ls"

    Scenario: A multi-line selection reaches the session as one paste
      Given an AI session is running in the AI pane
      And the AI program asked for bracketed paste
      And "src/tree.js" is open in the editor holding:
        """
        one two
        three four
        """
      And I press "Vj" in the editor
      When I run ":inject"
      Then the AI received the bytes "\e[200~one two\nthree four\e[201~"

    Scenario: Injecting with nothing running starts a session
      Given no AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """
        one two
        """
      When I run ":inject"
      Then an AI session was started with "claude"
      And the AI received nothing

    Scenario: The text waits for a session that has not spoken yet
      Given no AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """
        one two
        """
      And I run ":inject"
      When the AI session is ready for input
      Then the AI received the bytes "one two"

    Scenario: A session that cannot be told a paste from typing is not sent lines it would submit
      Given an AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """
        one two
        three four
        """
      And I press "Vj" in the editor
      When I run ":inject"
      Then the AI received nothing
      And the reviewer is told the lines cannot be injected

    Scenario: One line still goes to a session that never asked about pasting
      Given an AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """
        one two
        three four
        """
      When I run ":inject"
      Then the AI received the bytes "one two"

    Scenario: The action is on the editor's border before any session runs
      Given no AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """
        one two
        """
      When I click the AI Inject action on the editor pane's border
      Then an AI session was started with "claude"

    Scenario: With nothing to inject nothing is sent and nothing is started
      Given no AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """

        """
      When I run ":inject"
      Then the AI received nothing
      And no new AI session was started
      And the reviewer is told there is nothing to inject

    Scenario: The action is reachable on the editor's border
      Given an AI session is running in the AI pane
      And "src/tree.js" is open in the editor holding:
        """
        run("unquoted path")
        """
      And I drag across "unquoted path" in the editor pane
      When I click the AI Inject action on the editor pane's border
      Then the AI received the bytes "unquoted path"

    Scenario: The action is reachable on the terminal's border
      Given an AI session is running in the AI pane
      And the terminal shows:
        """
        bash-5.3$ ls
        """
      And I drag across "bash-5.3$ ls" in the terminal pane
      When I click the AI Inject action on the terminal pane's border
      Then the AI received the bytes "bash-5.3$ ls"

    Scenario: The AI pane's border carries Skills, not AI Inject
      Given an AI session is running in the AI pane
      Then the AI pane's border offers "skills"
      And the AI pane's border does not offer "inject-to-ai"
