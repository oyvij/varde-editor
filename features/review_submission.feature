Feature: Submitting a review puts the AI to work

  A reviewer annotates changed code with ISSUE, NOTE, SUGGESTION or COMMENT. Each comment
  covers a line range and records the revision that was reviewed, so it still points at what
  the reviewer actually saw.

  ISSUE is the only blocking type: a review containing one is changes-requested and the AI
  is expected to act on it. The others are context.

  Submitting is the integration point. It writes a durable artifact under .varde/reviews/
  and sends the review into the AI pane — sent, not merely typed — so the AI starts working
  the moment the reviewer confirms. The artifact keeps this independent of which AI CLI is
  configured.

  Submitting clears whatever the CLI is showing, which can be a half-written message, so it
  asks first. The clear is best-effort — a foreign CLI's prompt cannot be read without
  screen-scraping it — and the confirmation is what makes best-effort safe: nothing is
  destroyed without being announced.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository
    And "src/tree.js" is shown in the diff holding:
      """
      const p = path;
      run(`ls ${p}`);
      """
    And an AI session is running in the AI pane

  Scenario: Dragging the gutter selects the lines a comment covers
    When I drag the gutter of "src/tree.js" from line 14 to line 18
    Then the comment picker is shown
    When I choose the type ISSUE and enter "unquoted path"
    Then the review holds a comment:
      | file      | src/tree.js   |
      | from_line | 14            |
      | to_line   | 18            |
      | type      | ISSUE         |
      | body      | unquoted path |
    And the comment's revision is the blob oid of what "src/tree.js" held

  Scenario: A comment records its file, line range and reviewed revision
    When I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    Then the review holds a comment:
      | file        | src/tree.js  |
      | from_line   | 14           |
      | to_line     | 18           |
      | type        | ISSUE        |
      | body        | unquoted path |
    And the comment's revision is the blob oid of what "src/tree.js" held
    And the comment records no story

  Scenario: A comment made after the AI rewrites the file records the newer revision
    Given I add a NOTE on "src/tree.js" lines 1 to 1 saying "first pass"
    When "src/tree.js" is shown in the diff holding:
      """
      const p = shell_quote(path);
      run(`ls ${p}`);
      """
    And I add a NOTE on "src/tree.js" lines 1 to 1 saying "second pass"
    Then the two comments record different revisions

  Scenario: A comment on a deleted file records the revision it was deleted from
    Given "src/dead.js" is shown in the diff as deleted, having held:
      """
      module.exports = {};
      """
    When I add an ISSUE on "src/dead.js" lines 1 to 1 saying "still referenced"
    Then the comment's revision is the blob oid of what "src/dead.js" held

  Scenario: A review containing an ISSUE requests changes
    Given I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I add a NOTE on "src/tree.js" lines 22 to 22 saying "naming"
    And I submit the review
    When I confirm the submission
    Then the review verdict is "changes-requested"

  Scenario: A review without an ISSUE does not block
    Given I add a NOTE on "src/tree.js" lines 22 to 22 saying "naming"
    And I add a SUGGESTION on "src/tree.js" lines 30 to 32 saying "extract this"
    And I add a COMMENT on "src/tree.js" lines 40 to 40 saying "nice"
    And I submit the review
    When I confirm the submission
    Then the review verdict is "commented"

  Scenario: Submitting writes a durable artifact
    Given I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then the file ".varde/reviews/0001.json" exists

  Scenario: Submitting sends the review into the running AI session
    Given I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then the AI pane was sent a prompt containing "src/tree.js:14-18"
    And the AI pane was sent a prompt containing "unquoted path"
    And the AI pane was sent a prompt containing ".varde/reviews/0001.json"
    And the prompt was submitted to the AI
    And no new AI session was started

  Scenario: Submitting with no AI running launches the configured one first
    Given no AI session is running in the AI pane
    And the effective setting "ai.command" is "claude"
    And I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    And I confirm the submission
    When the AI session is ready for input
    Then an AI session was started with "claude"
    And the AI pane was sent a prompt containing ".varde/reviews/0001.json"
    And the prompt was submitted to the AI

  Scenario: A freshly started CLI is not typed at until it has printed something
    Given no AI session is running in the AI pane
    And the effective setting "ai.command" is "claude"
    And I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then an AI session was started with "claude"
    And no prompt was sent to the AI

  Scenario: A session started a moment ago is not typed at either
    Given no AI session is running in the AI pane
    And I start the AI with "opencode"
    And I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then only one AI session was started
    And no prompt was sent to the AI

  Scenario: A CLI that dies before printing takes the queued review with it
    Given no AI session is running in the AI pane
    And I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    And I confirm the submission
    And the AI session exits
    When the AI session is ready for input
    Then no prompt was sent to the AI

  Scenario: A review queued for a session that is replaced is not sent to its replacement
    Given no AI session is running in the AI pane
    And I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    And I confirm the submission
    And I force the AI to "opencode"
    When the AI session is ready for input
    Then no prompt was sent to the AI

  Scenario: A review queued for a CLI that never started is not sent to the next one
    Given no AI session is running in the AI pane
    And the AI CLI cannot be started
    And I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    And I confirm the submission
    And the AI CLI can be started
    And I start the AI with "opencode"
    When the AI session is ready for input
    Then no prompt was sent to the AI

  Scenario: An empty review cannot be submitted
    Given the review holds no comments
    When I submit the review
    Then the review is not submitted
    And the reviewer is told the review is empty
    And no prompt was sent to the AI

  Scenario: Submitting clears the comments for the next pass
    Given I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then the review holds no comments

  Scenario: Only the most recent reviews are kept
    Given the retention limit is 50 reviews
    And ".varde/reviews" holds 50 reviews numbered 0001 to 0050
    And I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    When I confirm the submission
    Then the file ".varde/reviews/0051.json" exists
    And the file ".varde/reviews/0001.json" does not exist
    And ".varde/reviews" holds 50 reviews

  Scenario: Submitting warns before it clears the AI prompt
    Given I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    When I submit the review
    Then the submission confirmation is shown
    And the review is not submitted
    And no prompt was sent to the AI

  Scenario: Declining leaves the AI prompt untouched and the review unsent
    Given I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I submit the review
    When I decline the submission
    Then the submission confirmation is not shown
    And the review is not submitted
    And no prompt was sent to the AI
    And the review holds a comment:
      | file | src/tree.js |

  Scenario: The review arrives as one paste, with the prompt cleared first
    Given the AI program asked for bracketed paste
    And I add an ISSUE on "src/tree.js" lines 14 to 18 saying "unquoted path"
    And I add a NOTE on "src/tree.js" lines 22 to 22 saying "naming"
    And I submit the review
    When I confirm the submission
    Then the prompt reached the AI as one paste, with its line breaks intact

  Rule: The comment box is a buffer

    The body is an editor Buffer, so it inherits the motions, the word delete, the newline,
    the undo and the one-edit paste the editor already has rather than a second text editor
    built on a draft string. Enter is therefore a newline, which means filing the comment is a gesture of its
    own — named in the box's own footer, the way anything that cannot be undone is confirmed
    deliberately rather than triggered by a key that means something else.

    Background:
      Given I drag the gutter of "src/tree.js" from line 14 to line 18
      And I pick the comment type ISSUE

    Scenario: Enter in the body starts a new line rather than filing the comment
      When I press Enter in the comment body
      Then the review holds no comments
      And the comment picker is shown

    Scenario: A comment body holds more than one line
      Given I type "first" in the comment body
      And I press Enter in the comment body
      When I type "second" in the comment body
      Then the comment body holds:
        """
        first
        second
        """

    Scenario: The filed comment carries the newlines that were typed
      Given I type "first" in the comment body
      And I press Enter in the comment body
      And I type "second" in the comment body
      When I file the comment
      Then the comment picker is not shown
      And the filed comment's body is:
        """
        first
        second
        """

    Scenario: Pasting multi-line text keeps every line and files nothing
      When I paste into the comment body:
        """
        thread panicked at src/tree.js
        note: run with RUST_BACKTRACE=1
        """
      Then the review holds no comments
      And the comment body holds:
        """
        thread panicked at src/tree.js
        note: run with RUST_BACKTRACE=1
        """

    Scenario: Backspace deletes a character in the body
      Given I type "issue" in the comment body
      When I press Backspace in the comment body
      Then the comment body holds:
        """
        issu
        """

    Scenario: Alt+Backspace deletes a word in the body
      Given I type "one two" in the comment body
      And I move a word left in the comment body
      When I press Alt+Backspace in the comment body
      Then the comment body holds:
        """
        two
        """

    Scenario: Character motion moves the caret inside the body
      Given I type "ab" in the comment body
      And I press the Left arrow in the comment body
      When I type "X" in the comment body
      Then the comment body holds:
        """
        aXb
        """

    Scenario: Word motion moves the caret inside the body
      Given I type "one two" in the comment body
      And I move a word left in the comment body
      When I type "X" in the comment body
      Then the comment body holds:
        """
        one Xtwo
        """

    Scenario: Ctrl+Z undoes the last word typed in the body
      Given I type "an issue" in the comment body
      When I press Ctrl+z in the comment body
      Then the comment body holds:
        """
        an
        """

    Scenario: Ctrl+Shift+Z puts back what Ctrl+Z took from the body
      Given I type "an issue" in the comment body
      And I press Ctrl+z in the comment body
      When I press Ctrl+Shift+z in the comment body
      Then the comment body holds:
        """
        an issue
        """

    Scenario: A pasted stack trace undoes in one key
      Given I type "look:" in the comment body
      And I paste into the comment body:
        """
        thread panicked at src/tree.js
        note: run with RUST_BACKTRACE=1
        """
      When I press Ctrl+z in the comment body
      Then the comment body holds:
        """
        look:
        """

    Scenario: A word delete undoes in one key
      Given I type "the path is quoted" in the comment body
      And I press Alt+Backspace in the comment body
      When I press Ctrl+z in the comment body
      Then the comment body holds:
        """
        the path is quoted
        """

    Scenario: Ctrl+Z with nothing left to undo leaves the body as it is
      When I press Ctrl+z in the comment body
      Then the review holds no comments
      And the comment picker is shown
      And the comment body holds:
        """
        """

    Scenario: A z with no modifier is a letter of the comment
      Given I type "si" in the comment body
      When I press z in the comment body
      Then the comment body holds:
        """
        siz
        """

    Scenario: Escape discards the comment and closes the box
      Given I type "half a thought" in the comment body
      When I press Escape in the comment body
      Then the comment picker is not shown
      And the review holds no comments
