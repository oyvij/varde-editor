Feature: Markdown preview

  A markdown file opens as a Preview — the document it describes, laid out to the
  pane — and `:preview` switches it back to Source, which is the only shape it can
  be edited in. The choice belongs to the buffer, not to the workspace: leaving one
  file in Source says nothing about the next file opened.

  What is asserted here is which **kind** each rendered row is — heading, paragraph,
  code, diagram, metadata — and which source line it came from. Never the glyphs: a
  bullet is a theme's business, exactly as a colour is in `highlighting.feature`.

  A Preview row is not a source line. Reflowing means one line becomes several rows,
  or none, so every row carries the line of the block it came from and the cursor
  crosses between the two through that map. The argument is in
  `docs/adr/0007-a-preview-row-is-not-a-line.md`.

  Background:
    Given the workspace root is "/home/me/projects/varde"

  Scenario Outline: Markdown opens as a preview and everything else as source
    Given "<file>" contains:
      """
      # Setup
      """
    When "<file>" is opened in the editor
    Then the editor is showing <shape>

    Examples:
      | file        | shape   |
      | README.md   | preview |
      | NOTES.markdown | preview |
      | guide.mdx   | source  |
      | src/main.rs | source  |

  Scenario: Preview is what a markdown buffer starts in
    Given "README.md" is open in the editor holding:
      """
      # Setup

      Install it.
      """
    Then the editor is showing preview
    And row 1 is a heading
    And row 1 holds "Setup"

  Scenario: The colon command switches to source and back
    Given "README.md" is open in the editor holding:
      """
      # Setup
      """
    When I run ":preview" in the editor
    Then the editor is showing source
    When I run ":preview" in the editor
    Then the editor is showing preview

  Scenario: The choice belongs to the buffer, not to the next file opened
    Given "README.md" is open in the editor holding:
      """
      # Setup
      """
    And I run ":preview" in the editor
    When "CHANGELOG.md" is opened in the editor
    Then the editor is showing preview
    When "README.md" is opened in the editor
    Then the editor is showing source

  Scenario: Previewing a file that is not markdown is refused out loud
    Given "src/main.rs" is open in the editor holding:
      """
      fn main() {}
      """
    When I run ":preview" in the editor
    Then the editor is showing source
    And the editor refuses with "not-a-markdown-file"

  Scenario: Previewing with nothing open is refused out loud
    Given no file is open in the editor
    When I run ":preview" in the editor
    Then the editor refuses with "no-file-open"

  Scenario: Review view is not previewed
    Given "README.md" is open in the editor holding:
      """
      # Setup
      """
    When I switch to review view
    Then the editor is showing a diff
    And the editor is not showing preview

  Scenario: Walking a story is not previewed
    Given "README.md" is open in the editor holding:
      """
      # Setup
      """
    When I walk a story step pointing at "README.md"
    Then the editor is showing a story step
    And the editor is not showing preview

  Scenario: Markup is consumed rather than shown
    Given "README.md" is open in the editor holding:
      """
      ## Install

      Run **make** to build, ~~then don't~~.
      """
    Then row 1 is a heading
    And row 1 holds "Install"
    And no row holds "##"
    And no row holds "**"
    And no row holds "~~"

  Scenario: A paragraph wider than the pane becomes several rows of one line
    Given the editor pane is 20 columns wide
    And "README.md" is open in the editor holding:
      """
      one two three four five six seven eight
      """
    Then there is more than 1 row
    And every row is a paragraph
    And every row comes from source line 1

  Scenario: A code fence is not wrapped, and is highlighted as its language
    Given the editor pane is 20 columns wide
    And "README.md" is open in the editor holding:
      """
      ```rust
      fn main() { let name = "a long string that will not be wrapped"; }
      ```
      """
    Then there is 1 code row
    And "fn" in the code row is a keyword

  Scenario: A fence in no language is still one unwrapped row
    Given the editor pane is 20 columns wide
    And "README.md" is open in the editor holding:
      """
      ```
      a line that is far longer than the pane is wide
      ```
      """
    Then there is 1 code row
    And every token in the code row is plain text

  Scenario: A fenced block is coloured as fully as the editor colours a file
    Given "README.md" is open in the editor holding:
      """
      ```typescript
      const total: number = add(1, 2);
      ```
      """
    Then there is 1 code row
    And "number" in the code row is a type
    And "add" in the code row is a function

  Scenario: Frontmatter is set apart rather than hidden
    Given "README.md" is open in the editor holding:
      """
      ---
      title: Setup
      ---

      Install it.
      """
    Then row 1 is metadata
    And the last row is a paragraph

  Scenario: A link shows its text and hides its target
    Given "README.md" is open in the editor holding:
      """
      See [the guide](https://example.com/guide).
      """
    Then some row holds "the guide"
    And no row holds "https://example.com/guide"

  Scenario: A mermaid fence becomes a diagram
    Given "README.md" is open in the editor holding:
      """
      ```mermaid
      graph LR; A[Build] --> B[Test]
      ```
      """
    Then every row is a diagram
    And some row holds "Build"
    And some row holds "Test"

  Scenario: A diagram that cannot be drawn says why and shows its source
    Given "README.md" is open in the editor holding:
      """
      ```mermaid
      C4Context
        title System
      ```
      """
    Then the diagram was refused with "unsupported-diagram"
    And there is 1 code row
    And the code row holds "C4Context"

  Scenario: The cursor moves a row at a time, not a paragraph
    Given the editor pane is 20 columns wide
    And "README.md" is open in the editor holding:
      """
      one two three four five six seven eight
      """
    When I press "j" in the editor
    Then the cursor is on row 2
    And the cursor is on source line 1

  Scenario: Crossing to source keeps the line and starts the column over
    Given "README.md" is open in the editor holding:
      """
      # Setup

      Install it.
      """
    And the cursor is on the row holding "Install it."
    When I run ":preview" in the editor
    Then the cursor is at line 3 column 1
    And the editor mode is normal

  Scenario: Crossing to preview lands on the row that line became
    Given "README.md" is open in the editor holding:
      """
      # Setup

      Install it.
      """
    And I run ":preview" in the editor
    And the cursor is at line 3 column 4
    When I run ":preview" in the editor
    Then the cursor is on the row holding "Install it."

  Scenario: A source line that became no row lands on the next row that exists
    Given "README.md" is open in the editor holding:
      """
      # Setup

      Install it.
      """
    And I run ":preview" in the editor
    And the cursor is at line 2 column 1
    When I run ":preview" in the editor
    Then the cursor is on the row holding "Install it."

  Scenario: Insert crosses to source and lands ready to type
    Given "README.md" is open in the editor holding:
      """
      # Setup

      Install it.
      """
    And the cursor is on the row holding "Install it."
    When I press "i" in the editor
    Then the editor is showing source
    And the cursor is at line 3 column 1
    And the editor mode is insert

  Scenario: An editing key changes nothing and says so
    Given "README.md" is open in the editor holding:
      """
      # Setup
      """
    When I press "d" in the editor
    And I press "d" in the editor
    Then the buffer is unchanged
    And the editor refuses with "read-only-preview"
    And the editor is showing preview

  Scenario: Finding searches what is on the screen, not the markup
    Given "README.md" is open in the editor holding:
      """
      ## Install
      """
    And I press "/" in the editor
    When I type "Install" into the in-file search
    Then the highlighted matches are:
      | 1 | 1 |
    When I type "##" into the in-file search
    Then there are no matches

  Scenario: Copying takes the text as it is drawn
    Given "README.md" is open in the editor holding:
      """
      ## Install
      """
    When I drag across the row holding "Install"
    Then the selection is "Install"

  Scenario: The preview follows unsaved edits made in source
    Given "README.md" is open in the editor holding:
      """
      # Setup
      """
    And I run ":preview" in the editor
    And I type "# Teardown" into the buffer as its only line
    When I run ":preview" in the editor
    Then row 1 is a heading
    And row 1 holds "Teardown"
    And the project "README.md" is unchanged

  Scenario: A preview has no line numbers and says what it is
    Given "README.md" is open in the editor holding:
      """
      # Setup
      """
    Then the editor has no line-number gutter
    And the editor title says "preview"
    When I run ":preview" in the editor
    Then the editor has a line-number gutter
    And the editor title does not say "preview"

  Scenario Outline: A preview cursor moves through a rendered row
    Given "README.md" is open in the editor holding:
      """
      ## Install

      Run it.
      """
    When I press "<keys>" in the editor
    Then the cursor is on row <row> column <column>

    Examples:
      | keys | row | column |
      | h    | 1   | 1      |
      | l    | 1   | 2      |
      | ll   | 1   | 3      |
      | ll0  | 1   | 1      |
      | $    | 1   | 7      |
      | j    | 2   | 1      |
      | jk   | 1   | 1      |
      | $j   | 2   | 1      |
      | G    | 3   | 1      |
      | Ggg  | 1   | 1      |

  Scenario Outline: Word motions move by word inside a preview row
    Given the screen is 12 rows by 80 columns
    And "README.md" is open in the editor holding:
      """
      one two three four five six seven eight nine ten
      """
    When I press "<keys>" in the editor
    Then the cursor is on row <row> column <column>

    Examples:
      | keys   | row | column |
      | w      | 1   | 5      |
      | ww     | 1   | 9      |
      | www    | 1   | 15     |
      | wwww   | 1   | 20     |
      | wwwww  | 2   | 1      |
      | wwwwwb | 1   | 20     |
      | e      | 1   | 3      |
      | ee     | 1   | 7      |

  Scenario: The end-of-row motion stops at the rendered text, not the markup
    Given "README.md" is open in the editor holding:
      """
      ## Install
      """
    When I press "$" in the editor
    Then the cursor is on row 1 column 7

  Scenario: gg and G reach the first and last rendered row
    Given "README.md" is open in the editor holding:
      """
      # Setup

      Install it.
      """
    When I press "G" in the editor
    Then the cursor is on row 3 column 1
    When I press "gg" in the editor
    Then the cursor is on row 1 column 1

  Scenario: The arrows reach the same motions the letters do
    Given "README.md" is open in the editor holding:
      """
      ## Install
      """
    When I press the Right arrow in the editor
    Then the cursor is on row 1 column 2

  Scenario: The word-motion arrows move by word in a preview too
    Given "README.md" is open in the editor holding:
      """
      one two three
      """
    When I hold alt and press the Right arrow in the editor
    Then the cursor is on row 1 column 5

  Scenario: Extending a selection with the keyboard picks the rendered rows
    Given "README.md" is open in the editor holding:
      """
      one two three
      """
    When I hold shift and press the Right arrow in the editor
    Then the selection is "on"
    And the cursor is on row 1 column 2

  Scenario: Extending twice reaches one more character
    Given "README.md" is open in the editor holding:
      """
      one two three
      """
    And I hold shift and press the Right arrow in the editor
    When I hold shift and press the Right arrow in the editor
    Then the selection is "one"

  Scenario: Extending by word with the keyboard takes a word of the rendered rows
    Given "README.md" is open in the editor holding:
      """
      one two three
      """
    When I hold shift and alt and press the Right arrow in the editor
    Then the selection is "one"
    And the cursor is on row 1 column 3

  Scenario: Crossing to source drops what was picked over the rows
    Given "README.md" is open in the editor holding:
      """
      ## Install
      """
    And I drag across the row holding "Install"
    When I run ":preview" in the editor
    Then the selection holds nothing

  Scenario: Crossing back to a preview drops what was picked in source
    Given "README.md" is open in the editor holding:
      """
      ## Install
      """
    And I run ":preview" in the editor
    And I hold shift and press the Right arrow in the editor
    When I run ":preview" in the editor
    Then the selection holds nothing

  Scenario: A plain motion in a preview drops what was picked
    Given "README.md" is open in the editor holding:
      """
      one two three
      """
    And I hold shift and press the Right arrow in the editor
    When I press the Right arrow in the editor
    Then the selection holds nothing
    And the cursor is on row 1 column 3

  Scenario: A keyboard selection over a preview leaves the markup out of it
    Given "README.md" is open in the editor holding:
      """
      ## Install
      """
    When I hold shift and alt and press the Right arrow in the editor
    Then the selection is "Install"

  Scenario: A motion in a preview never edits the buffer
    Given "README.md" is open in the editor holding:
      """
      ## Install

      Run it.
      """
    When I press "l$wbeG" in the editor
    Then the cursor is on row 3 column 1
    And the buffer is unchanged
    And the project "README.md" is unchanged

  Scenario: A motion after a half-typed g is refused by name
    Given "README.md" is open in the editor holding:
      """
      ## Install

      Run it here now.
      """
    When I press "gl" in the editor
    Then the notice is "no-such-motion"
    And the message names "gl"
    And the cursor is on row 1 column 1

  Scenario Outline: A half-typed g does not survive the key that follows it
    Given "README.md" is open in the editor holding:
      """
      ## Install

      Run it here now.
      """
    And I press "j" in the editor
    When I press "g<key>" in the editor
    And I press "g" in the editor
    Then the cursor is on row 2 column 1

    Examples:
      | key |
      | l   |
      | j   |
      | d   |

  Scenario: A motion in a preview leaves the source cursor where it was
    Given "README.md" is open in the editor holding:
      """
      ## Install

      Run it.
      """
    When I press "w" in the editor
    Then the cursor is on row 3 column 1
    And the cursor is at line 1 column 1

  Scenario: A file opened at a line lands on the row that line was rendered from
    Given "README.md" is open in the editor holding:
      """
      # Notes

      Install it.
      Then run it.

      Done here.
      """
    When I jump to "README.md" line 6
    Then the cursor is on the row holding "Done here."

  Scenario: A click in a preview places the cursor on the character it points at
    Given the screen is 24 rows by 100 columns
    And "README.md" is open in the editor holding:
      """
      ## Install
      """
    When I click at line 1 column 5 in the editor
    Then the cursor is on row 1 column 5

  Scenario: Finding puts the cursor on the match's column, not the start of the row
    Given "README.md" is open in the editor holding:
      """
      Please install it now.
      """
    And I press "/" in the editor
    When I type "install" into the in-file search
    Then the cursor is on row 1 column 8

  Scenario: n steps between two matches on the same rendered row
    Given "README.md" is open in the editor holding:
      """
      Install it, then install it again.
      """
    And I press "/" in the editor
    And I type "install" into the in-file search
    And I press Enter during the in-file search
    When I press "n" in the editor
    Then the cursor is on row 1 column 18

  Scenario: What a preview finds is copied as it is drawn, not as it is written
    Given a system clipboard is available
    And "README.md" is open in the editor holding:
      """
      ## Install it
      """
    And I press "/" in the editor
    And I type "Install" into the in-file search
    And I press Enter during the in-file search
    When I copy the selection
    Then the clipboard holds "Install"

  Scenario: A code fence wider than the pane can be read to its end
    Given the screen is 12 rows by 80 columns
    And "README.md" is open in the editor holding a code fence 60 characters wide
    When I press "l" in the editor 30 times
    Then the cursor is on row 1 column 31
    And the editor view starts at column 8

  Scenario: The preview cursor slides the view to keep itself visible
    Given the screen is 12 rows by 80 columns
    And "README.md" is open in the editor holding a code fence 60 characters wide
    When I press "$" in the editor
    Then the cursor is on row 1 column 60
    And the editor view starts at column 37

  Scenario: A sideways swipe slides a preview and takes its cursor with it
    Given the screen is 12 rows by 80 columns
    And "README.md" is open in the editor holding a code fence 60 characters wide
    When I scroll right with the pointer over the editor pane
    Then the editor view starts at column 9
    And the cursor is on row 1 column 9

  Scenario: A swipe stops at the widest row drawn, not the widest line behind it
    Given the screen is 12 rows by 80 columns
    And "README.md" is open in the editor holding:
      """
      [x](https://example.com/a/very/long/url/that/is/never/drawn/on/the/screen)
      """
    When I scroll right 4 times with the pointer over the editor pane
    Then the editor view starts at column 1

  Scenario: Zero is the start-of-row motion, and the view comes home with it
    Given the screen is 12 rows by 80 columns
    And "README.md" is open in the editor holding a code fence 60 characters wide
    And I press "l" in the editor 30 times
    When I press "0" in the editor
    Then the cursor is on row 1 column 1
    And the editor view starts at column 1

  Scenario: Crossing to source from a slid preview starts at column one
    Given the screen is 12 rows by 80 columns
    And "README.md" is open in the editor holding a code fence 60 characters wide
    And I press "l" in the editor 30 times
    When I run ":preview" in the editor
    Then the editor is showing source
    And the cursor is at line 1 column 1
    And the editor view starts at column 1

  Scenario: Crossing back into a preview from a slid source starts at column one
    Given the screen is 12 rows by 80 columns
    And "README.md" is open in the editor holding a code fence 60 characters wide
    And I run ":preview" in the editor
    And I press "j" in the editor
    And I press "$" in the editor
    When I run ":preview" in the editor
    Then the editor is showing preview
    And the cursor is on row 1 column 1
    And the editor view starts at column 1

  Rule: A wikilink opens the Note it names

    `[[Name]]` resolves the way Obsidian resolves it, against whatever the tree is showing — the
    workspace in Edit view, the Vault in the Knowledge view: a file named `Name.md` anywhere below
    the root, and `[[Folder/Name]]` when two share a name. It works in the Preview and in source.

    Background:
      Given the workspace holds:
        | docs/Setup.md        |
        | docs/guide/Panes.md  |
        | notes/Panes.md       |

    Scenario: A wikilink opens the file of that name
      Given "README.md" is open in the editor holding:
        """
        See [[Setup]] first.
        """
      When I follow the link "Setup"
      Then "docs/Setup.md" is open in the editor

    Scenario: A wikilink is followed from source too
      Given "README.md" is open in the editor holding:
        """
        See [[Setup]] first.
        """
      And I run ":preview" in the editor
      When I follow the link "Setup"
      Then "docs/Setup.md" is open in the editor

    Scenario: A path-qualified wikilink picks one of two Notes sharing a name
      Given "README.md" is open in the editor holding:
        """
        See [[notes/Panes]].
        """
      When I follow the link "notes/Panes"
      Then "notes/Panes.md" is open in the editor

    Scenario: An ambiguous wikilink is refused rather than guessed
      Given "README.md" is open in the editor holding:
        """
        See [[Panes]].
        """
      When I follow the link "Panes"
      Then "README.md" is open in the editor
      And the reviewer is told "ambiguous-link"

    Scenario: A wikilink with an alias opens its target
      Given "README.md" is open in the editor holding:
        """
        See [[Setup|the setup notes]].
        """
      When I follow the link "the setup notes"
      Then "docs/Setup.md" is open in the editor

    Scenario: A wikilink to nothing creates nothing
      Given "README.md" is open in the editor holding:
        """
        See [[Nowhere]].
        """
      When I follow the link "Nowhere"
      Then "README.md" is open in the editor
      And no file was created
      And the reviewer is told "no-such-note"

    Scenario: In the Knowledge view a wikilink resolves in the Vault
      Given the knowledge base is enabled with the default Vault
      And the Vault holds:
        | Acme/customers/Acme.md |
        | Acme/how-to/Renew.md   |
      And I toggle the Knowledge view
      And I open "Acme/customers/Acme.md" from the tree holding:
        """
        Renewals: [[Renew]].
        """
      When I follow the link "Renew"
      Then "Acme/how-to/Renew.md" is open in the editor
      And the view is "knowledge"

  Rule: A wikilink looks like a link, and pointing at it marks it

    A wikilink is drawn in the link colour wherever it is shown. Pointing at one underlines and
    tints it, and on a terminal that can change the mouse pointer the pointer becomes a hand;
    Ctrl+click follows it. Unlike a name in code, it needs no held modifier to show it is a link.

    Background:
      Given the workspace holds:
        | docs/Setup.md |
      And "README.md" is open in the editor holding:
        """
        See [[Setup|the setup notes]] first.
        """

    Scenario: A wikilink is drawn as a link in the preview
      Given I run ":preview" in the editor
      When I run ":preview" in the editor
      Then the editor draws "the setup notes" as a link

    Scenario: A wikilink is drawn as a link in source
      When I run ":preview" in the editor
      Then the editor draws "[[Setup|the setup notes]]" as a link

    Scenario: Pointing at a wikilink in the preview underlines all of it
      When I point at "setup" in the editor pane
      Then the editor pane underlines "the setup notes"

    Scenario: Pointing at a wikilink in source underlines all of it
      Given I run ":preview" in the editor
      When I point at "Setup" in the editor pane
      Then the editor pane underlines "[[Setup|the setup notes]]"

    Scenario: Pointing away from a wikilink takes the underline with it
      Given I point at "setup" in the editor pane
      When I point at "first" in the editor pane
      Then the editor pane underlines nothing

    Scenario: Ctrl+click on a wikilink in the preview opens its Note
      When I click on "setup" in the editor pane with the jump modifier held
      Then "docs/Setup.md" is open in the editor
