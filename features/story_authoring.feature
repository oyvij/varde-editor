Feature: Authoring a story set for a change

  A Story is not written by hand. `:story` resolves a revision range, confirms it, and pastes a
  prompt into the hosted AI session asking it to write a story file into `.varde/stories/`. The
  existing file watcher picks it up. Varde never reads what the CLI prints — the artifact is the
  whole channel, which is what keeps this working with a provider nobody has tried
  (`docs/adr/0006-stories-arrive-as-an-artifact.md`).

  A Range is what the head *introduced*: git's three-dot form, the merge-base against the base,
  never tip against tip. A base branch that moved on after the head forked would otherwise read as
  the head having deleted every commit that landed meanwhile.

  Resolving the range is offline and deterministic, because authoring costs the reviewer five to
  eleven minutes and a cleared AI prompt: a wrong guess is expensive twice. When nothing resolves,
  Varde refuses out loud rather than picking something plausible.

  A Story dies with its range. It is named for the revisions it describes, replaced by naming when
  re-authored, and never updated to follow the code — see
  `docs/adr/0005-a-story-dies-with-its-range.md`.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository

  Scenario: A bare story command on a dirty tree offers uncommitted against HEAD
    Given the working tree contains:
      | path         | git status |
      | src/keys.rs  | modified   |
    When I run ":story"
    Then the story range is "HEAD..worktree"
    And the modal is "confirm-story"
    And no keys were sent to the AI pane

  Scenario: A bare story command on a clean tree covers what the head introduced
    Given the working tree has no changes
    And "origin/HEAD" resolves to "main"
    And "main" has commits "HEAD" does not
    When I run ":story"
    Then the story range is "main...HEAD"
    And the modal is "confirm-story"

  Scenario Outline: The default branch is resolved in a fixed order, offline
    Given the working tree has no changes
    And the repository resolves <source> to "<branch>"
    And "<branch>" has commits "HEAD" does not
    When I run ":story"
    Then the story range is "<branch>...HEAD"

    Examples:
      | source              | branch  |
      | "origin/HEAD"       | main    |
      | the upstream branch | develop |
      | "init.defaultBranch"| trunk   |
      | a probe for "main"  | main    |
      | a probe for "master"| master  |

  Scenario: Nothing resolves, so authoring is refused rather than guessed
    Given the working tree has no changes
    And the repository resolves no default branch
    When I run ":story"
    Then the story view state is "no-default-branch"
    And the modal is "none"
    And no keys were sent to the AI pane
    And no AI session was started

  Scenario: An explicit range git cannot resolve is a notice, never an empty list
    When I run ":story typo..HEAD"
    Then the story view state is "bad-range"
    And the spine is empty
    And no keys were sent to the AI pane

  Scenario: Confirming sends the authoring prompt to the running AI session
    Given the working tree contains:
      | path        | git status |
      | src/keys.rs | modified   |
    And an AI session is running
    And I ran ":story"
    When I confirm the story range
    Then the AI pane was sent a prompt naming the range "HEAD..worktree"
    And the story view state is "authoring"

  Scenario: Confirming with no AI session launches the configured one first
    Given the working tree contains:
      | path        | git status |
      | src/keys.rs | modified   |
    And no AI session is running
    And I ran ":story"
    When I confirm the story range
    And the AI session is ready for input
    Then the AI session "claude" was started
    And the AI pane was sent a prompt naming the range "HEAD..worktree"

  Scenario: Declining leaves the AI prompt untouched and nothing authored
    Given the working tree contains:
      | path        | git status |
      | src/keys.rs | modified   |
    And an AI session is running
    And I ran ":story"
    When I decline the story range
    Then no keys were sent to the AI pane
    And the story view state is "no-stories"
    And no file was written

  Scenario: The spine says it is authoring while it waits
    Given authoring has begun for "main..HEAD"
    Then the story view state is "authoring"
    And the spine is empty

  Scenario: The artifact arrives and its stories are listed
    Given "src/keys.rs" holds 20 numbered lines
    And authoring has begun for "main..HEAD"
    When the story artifact arrives:
      """
      {
        "protocolVersion": 2,
        "title": "The key reaches the child",
        "range": { "base": "aaaaaaaaaaaa", "head": "bbbbbbbbbbbb", "spelling": "main..HEAD" },
        "stories": [
          { "id": "s1", "name": "The key reaches the child", "premise": "…",
            "steps": [
              { "id": "s1e1", "name": "Router matches", "claim": "…", "why": "…",
                "site": { "file": "src/keys.rs", "side": "new", "kind": "changed",
                          "from": 10, "to": 12, "hash": "h1", "text": "a\nb\nc" } }
            ] }
        ]
      }
      """
    Then the story view state is "spine"
    And the spine lists:
      | name                      | steps |
      | The key reaches the child | 1     |

  Scenario: A story set read off disk that fails a check is refused, never handed back
    Given "src/keys.rs" holds 20 numbered lines
    And a story set on disk claims lines 30 to 40 of "src/keys.rs"
    When I open Story view
    Then the story view state is "story-artifact-invalid"
    And the refusal names step "s1e1" as "range-out-of-bounds"
    And no keys were sent to the AI pane
    And no AI session was started

  Rule: Varde fills in the code the AI no longer writes

    A Step names its file, its side, its kind and its line range, and nothing else. The literal copy
    of the code it used to carry was the largest single field in every measured artifact and pure
    transcription, so Varde reads it off its own copy instead — once, when the Story arrives, which
    is what leaves the stale check a baseline to compare against. Filling on every load would
    compare each file against itself and report every Step fresh for good.

    Scenario: A step that writes no code text is filled in from the file itself
      Given "src/keys.rs" holds:
        """
        the first line
        the second line
        the third line
        """
      And authoring has begun for "main..HEAD"
      When a story set for this change claims:
        | story    | file        | side | kind    | from | to |
        | The keys | src/keys.rs | new  | changed | 2    | 2  |
      Then the story view state is "spine"
      And the site text of step 1 is "the second line"

    Scenario: A story set that carries its own code text keeps it, so the sets on disk still load
      Given "src/keys.rs" holds:
        """
        the first line
        the second line
        """
      And authoring has begun for "main..HEAD"
      When a story set for this change claims:
        | story    | file        | side | kind    | from | to | text                |
        | The keys | src/keys.rs | new  | changed | 2    | 2  | what it used to say |
      Then the story view state is "spine"
      And the site text of step 1 is "what it used to say"

    Scenario: An old-side step is filled from the base, a new-side one from the working tree
      Given "src/keys.rs" held:
        """
        the line the change removed
        the line they share
        """
      And "src/keys.rs" now holds:
        """
        the line they share
        the line the change added
        """
      And authoring has begun for "main..HEAD"
      When a story set for this change claims:
        | story    | file        | side | kind    | from | to |
        | The keys | src/keys.rs | old  | changed | 1    | 1  |
        | The keys | src/keys.rs | new  | changed | 2    | 2  |
      Then the site text of step 1 is "the line the change removed"
      And the site text of step 2 is "the line the change added"

    Scenario: A step whose code moves after the story arrived still says what its site used to hold
      Given "src/keys.rs" holds:
        """
        the first line
        the second line
        the third line
        """
      And authoring has begun for "main..HEAD"
      And a story set for this change claims:
        | story    | file        | side | kind    | from | to |
        | The keys | src/keys.rs | new  | changed | 2    | 2  |
      And I enter the story "The keys"
      When "src/keys.rs" now holds:
        """
        the first line
        something else entirely
        the third line
        """
      Then step 1 is stale as "text-changed"
      And the overlay shows what the site holds now
      And the site text of step 1 is "the second line"

    Scenario: Re-reading a story set does not refill it against the code as it reads now
      Given "src/keys.rs" holds:
        """
        the first line
        the second line
        the third line
        """
      And a story set on disk claims lines 2 to 2 of "src/keys.rs"
      And I open Story view
      And "src/keys.rs" now holds:
        """
        the first line
        something else entirely
        the third line
        """
      And I leave Story view
      When I open Story view
      Then the site text of step 1 is "the second line"

    Scenario: A story set is not walkable until Varde has read what its sites hold
      Given "src/keys.rs" holds:
        """
        the first line
        the second line
        """
      And Varde has not yet read what the sites hold
      And authoring has begun for "main..HEAD"
      And a story set for this change claims:
        | story    | file        | side | kind    | from | to |
        | The keys | src/keys.rs | new  | changed | 2    | 2  |
      When I choose the first story from the spine
      Then the story view state is "filling"
      And the spine is empty
      And the band has no claim

  Scenario: The AI exits before the artifact arrives
    Given authoring has begun for "main..HEAD"
    When the AI session exits
    Then the story view state is "authoring-abandoned"
    And the spine is empty

  Scenario: Cancelling authoring leaves the working tree unchanged
    Given the project has a ".gitignore"
    And authoring has begun for "main..HEAD"
    When I press "Escape"
    Then the story view state is "no-stories"
    And the project ".gitignore" is unchanged

  Scenario: A malformed artifact is refused whole, never salvaged in part
    Given authoring has begun for "main..HEAD"
    When the story artifact arrives:
      """
      { "protocolVersion": 2, "stories": [ { "id": "s1", "name": "Half a story" } ] }
      """
    Then the story view state is "story-artifact-invalid"
    And the spine is empty
    And the review list is unchanged

  Scenario: An artifact with no title is refused whole
    Given authoring has begun for "main..HEAD"
    When the story artifact arrives:
      """
      {
        "protocolVersion": 2,
        "range": { "base": "aaaaaaaaaaaa", "head": "bbbbbbbbbbbb", "spelling": "main..HEAD" },
        "stories": [
          { "id": "s1", "name": "The key reaches the child", "premise": "…",
            "steps": [
              { "id": "s1e1", "name": "Router matches", "claim": "…", "why": "…",
                "site": { "file": "src/keys.rs", "side": "new", "kind": "changed",
                          "from": 10, "to": 12, "text": "a\nb\nc" } }
            ] }
        ]
      }
      """
    Then the story view state is "story-artifact-invalid"
    And the spine is empty

  Scenario: An artifact with a step missing a name is refused whole
    Given authoring has begun for "main..HEAD"
    When the story artifact arrives:
      """
      {
        "protocolVersion": 2,
        "title": "The key reaches the child",
        "range": { "base": "aaaaaaaaaaaa", "head": "bbbbbbbbbbbb", "spelling": "main..HEAD" },
        "stories": [
          { "id": "s1", "name": "The key reaches the child", "premise": "…",
            "steps": [
              { "id": "s1e1", "claim": "…", "why": "…",
                "site": { "file": "src/keys.rs", "side": "new", "kind": "changed",
                          "from": 10, "to": 12, "text": "a\nb\nc" } }
            ] }
        ]
      }
      """
    Then the story view state is "story-artifact-invalid"
    And the spine is empty

  Scenario: An artifact whose range no longer resolves is not walked
    Given a story set exists for the range "aaaaaaaaaaaa..bbbbbbbbbbbb"
    And "aaaaaaaaaaaa" is not a commit in this repository
    When I open Story view
    Then the story view state is "range-unresolvable"
    And the spine is empty

  Scenario: A story set for a dirty range is named with "worktree", not a commit oid
    Given the working tree contains:
      | path        | git status |
      | src/keys.rs | modified   |
    And "HEAD..worktree" resolves to base "aaaaaaaaaaaa" and head "worktree"
    And I ran ":story"
    When I confirm the story range
    And the AI session is ready for input
    Then the AI pane was sent a prompt naming the file ".varde/stories/aaaaaaaaaaaa-worktree.json"

  Scenario: A story set for a committed range is named for both commits
    Given "main..HEAD" resolves to base "aaaaaaaaaaaa" and head "bbbbbbbbbbbb"
    And I ran ":story main..HEAD"
    When I confirm the story range
    And the AI session is ready for input
    Then the AI pane was sent a prompt naming the file ".varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.json"

  Scenario: Confirming a range hands the change over beside the story set
    Given "main..HEAD" resolves to base "aaaaaaaaaaaa" and head "bbbbbbbbbbbb"
    And I ran ":story main..HEAD"
    When I confirm the story range
    And the AI session is ready for input
    Then the file ".varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.context.md" was written
    And the AI pane was sent a prompt naming the file ".varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.context.md"

  Scenario: Only the ten most recent story sets are kept, and their companion files with them
    Given the project holds 10 story sets
    When a new story set is written
    Then 10 story sets remain
    And the oldest story set was pruned
    And the oldest story set's companion file was pruned

  Scenario: A range already authored loads without touching the AI
    Given a story set exists for the range "aaaaaaaaaaaa..bbbbbbbbbbbb"
    And that range is what "main..HEAD" resolves to
    When I run ":story main..HEAD"
    Then the story view state is "spine"
    And no keys were sent to the AI pane
    And no AI session was started
    And the modal is "none"

  Scenario: Re-authoring takes a bang, and confirms first
    Given a story set exists for the range "aaaaaaaaaaaa..bbbbbbbbbbbb"
    And that range is what "main..HEAD" resolves to
    And an AI session is running
    When I run ":story! main..HEAD"
    Then the modal is "confirm-story"
    And no keys were sent to the AI pane

  Rule: Varde checks the artifact, and says what failed

    Four questions need no judgement: the file a Step's Site names exists, its line range fits
    inside that file, a Site claiming a change really overlaps one, and a cited value really
    appears on the line it cites. They are milliseconds of work and no AI time at all, and a set
    that fails any of them is not walkable — a Step pointing at the wrong code is otherwise walked
    with exactly the confidence of a right one. It is handed back to the AI rather than refused on
    the spot; the Rule below bounds that.

    What is never checked: whether the code does what the Step's sentence says, and whether the
    Steps are in the order the code runs. Those are judgements and they stay with the AI. The
    citation check is likewise the weakest thing that is honest — whether the value's characters
    are on the line named, never whether it is really a literal.

    Background:
      Given "src/keys.rs" holds 20 numbered lines
      And an AI session is running
      And authoring has begun for "main..HEAD"

    Scenario: A step naming a file that is not there is handed back
      When a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      Then the story view state is "fixing"
      And the fix request names step "s1" as "file-missing"
      And the spine is empty

    Scenario: A site running past the end of its file is handed back
      When a story set for this change claims:
        | story    | file        | side | kind    | from | to |
        | The keys | src/keys.rs | new  | changed | 18   | 40 |
      Then the story view state is "fixing"
      And the fix request names step "s1" as "range-out-of-bounds"
      And the spine is empty

    Scenario: A step claiming a change where nothing changed is handed back
      Given "src/mouse.rs" held:
        """
        fn hit(p: Point) -> Pane {
            Pane::Editor
        }
        """
      When a story set for this change claims:
        | story    | file         | side | kind    | from | to |
        | The keys | src/mouse.rs | new  | changed | 2    | 2  |
      Then the story view state is "fixing"
      And the fix request names step "s1" as "claims-no-change"

    Scenario: A step walking into unchanged code is not held to coverage
      Given "src/mouse.rs" held:
        """
        fn hit(p: Point) -> Pane {
            Pane::Editor
        }
        """
      When a story set for this change claims:
        | story    | file         | side | kind    | from | to |
        | The keys | src/keys.rs  | new  | changed | 1    | 1  |
        | The keys | src/mouse.rs | new  | context | 2    | 2  |
      Then the story view state is "spine"

    Scenario: A cited value that is not on the line it cites is handed back
      When a story set for this change claims:
        | story    | file        | side | kind    | from | to | value  | cite            |
        | The keys | src/keys.rs | new  | changed | 1    | 1  | line 3 | src/keys.rs:9   |
      Then the story view state is "fixing"
      And the fix request names step "s1" as "citation-absent"

    Scenario: A cited value that really is on the line it cites loads
      When a story set for this change claims:
        | story    | file        | side | kind    | from | to | value  | cite          |
        | The keys | src/keys.rs | new  | changed | 1    | 1  | line 9 | src/keys.rs:9 |
      Then the story view state is "spine"

    Scenario: A set is held back whole, so no step of it is walkable
      When a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/keys.rs   | new  | changed | 1    | 1  |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      Then the story view state is "fixing"
      And the fix request names step "s2" as "file-missing"
      And the fix request does not name step "s1"
      And the spine is empty
      And the band has no claim

  Rule: A failing story set is handed back to the AI, not thrown away

    Authoring costs ten to twenty minutes. Losing all of it over four bad line numbers is what this
    Rule exists to stop: a set that fails a check is handed back with a fix request naming only the
    failing Steps, asking for those Steps rewritten to the same path, and Varde keeps waiting.

    Bounded by rounds, never by a clock (`docs/adr/0006-stories-arrive-as-an-artifact.md`). Two
    attempts; if the second artifact still fails, the set is refused with what failed — the same
    refusal a malformed one gets, reached as a last resort rather than as a first response. The
    failure that needs catching is a dead CLI, and that already arrives as an event.

    Background:
      Given "src/keys.rs" holds 20 numbered lines
      And an AI session is running
      And authoring has begun for "main..HEAD"

    Scenario: A fix request asks for the same path, not for the set re-authored
      When a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      Then the fix request names the file ".varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.json"

    Scenario: A corrected story set arriving after a fix request loads and walks
      Given a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      When a story set for this change claims:
        | story    | file        | side | kind    | from | to |
        | The keys | src/keys.rs | new  | changed | 1    | 1  |
      Then the story view state is "spine"
      And exactly 2 prompts were sent to the AI
      When I enter the story "The keys"
      Then the band claim is "claim"

    Scenario: A second failing story set for the same range is refused
      Given a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      When a story set for this change claims:
        | story    | file        | side | kind    | from | to |
        | The keys | src/gone.rs | new  | changed | 1    | 1  |
      Then the story view state is "story-artifact-invalid"
      And the refusal names step "s1" as "file-missing"
      And the spine is empty
      And exactly 2 prompts were sent to the AI

    Scenario: The same failing story set arriving twice is one round, not two
      Given a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      When a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      Then the story view state is "fixing"
      And exactly 2 prompts were sent to the AI

    Scenario: A fix request names every failing step, and none that passed
      When a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/keys.rs   | new  | changed | 1    | 1  |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
        | The keys | src/keys.rs   | new  | changed | 18   | 40 |
      Then the fix request names step "s2" as "file-missing"
      And the fix request names step "s3" as "range-out-of-bounds"
      And the fix request does not name step "s1"

    Scenario: Cancelling during a fix round leaves nothing authored, exactly as authoring does
      Given the project has a ".gitignore"
      And a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      When I press "Escape"
      Then the story view state is "no-stories"
      And the project ".gitignore" is unchanged

    Scenario: The AI exiting during a fix round reports the set abandoned
      Given a story set for this change claims:
        | story    | file          | side | kind    | from | to |
        | The keys | src/nowhere.rs| new  | changed | 1    | 1  |
      When the AI session exits
      Then the story view state is "authoring-abandoned"
      And the spine is empty

  Rule: The wait shows how long it has run and when a file last changed, and nothing else changes

    Authoring takes five to eleven minutes and has no timeout. A slow run and a stuck one looked the
    same, so the wait counts its time from the clock the Ticks carry, and the last time a file in
    the workspace changed on disk. It never reads the AI pane.

    Scenario: Authoring counts its elapsed time from the Ticks
      Given authoring has begun for "main..HEAD"
      And the clock reads 10 seconds
      When the clock reads 262 seconds
      Then the wait has run for 252 seconds
      And the wait's file activity is "none-yet"
      And the story view state is "authoring"

    Scenario: A change in the workspace is the wait's last file activity
      Given authoring has begun for "main..HEAD"
      And the clock reads 30 seconds
      And a change to "src/keys.rs" is seen on disk
      When the clock reads 70 seconds
      Then the wait's last file activity was 40 seconds ago

    Scenario Outline: The stories folder counts, but git, ignored paths and Varde's own files do not
      Given git ignores "target"
      And authoring has begun for "main..HEAD"
      And the clock reads 30 seconds
      When a change to "<path>" is seen on disk
      Then the wait's file activity is "<activity>"

      Examples:
        | path                           | activity |
        | .varde/stories/main..HEAD.json | seen     |
        | .git/index                     | none-yet |
        | target/debug/varde             | none-yet |
        | .varde/config.toml             | none-yet |
        | .varde/state.json              | none-yet |
        | /etc/hosts                     | none-yet |

    Scenario: The AI exiting ends authoring as before, and the wait with it
      Given authoring has begun for "main..HEAD"
      And the clock reads 30 seconds
      And a change to "src/keys.rs" is seen on disk
      When the AI session exits
      Then the story view state is "authoring-abandoned"
      And the spine is empty
      And no wait is in progress

    Scenario: Cancelling ends authoring as before, and the wait with it
      Given the project has a ".gitignore"
      And authoring has begun for "main..HEAD"
      And the clock reads 30 seconds
      And a change to "src/keys.rs" is seen on disk
      When I press "Escape"
      Then the story view state is "no-stories"
      And the project ".gitignore" is unchanged
      And no wait is in progress

    Scenario: A new authoring wait starts with no file activity and its own count
      Given authoring has begun for "main..HEAD"
      And the clock reads 30 seconds
      And a change to "src/keys.rs" is seen on disk
      And the AI session exits
      And the clock reads 90 seconds
      When authoring begins again for "main..HEAD"
      Then the wait's file activity is "none-yet"
      And the wait has run for 0 seconds

    Scenario: A change seen before the wait's first Tick is not dated by an older clock
      Given the clock reads 10 seconds
      And authoring has begun for "main..HEAD"
      And a change to "src/keys.rs" is seen on disk
      When the clock reads 600 seconds
      Then the wait has run for 0 seconds
      And the wait's file activity is "none-yet"
