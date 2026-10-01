Feature: Syntax highlighting

  Code in the editor is coloured by token type. What is asserted here is which
  token type a piece of text belongs to — never which colour it ends up, because
  colour is a theme's business and themes change.

  The language comes from the file extension, or, when the extension is unknown,
  from the first line, such as an XML declaration. A file whose language is
  unknown renders as plain text rather than failing.

  Background:
    Given the workspace root is "/home/me/projects/varde"

  Scenario: Rust keywords and strings are told apart
    Given "src/main.rs" contains:
      """
      fn main() { let name = "world"; }
      """
    When "src/main.rs" is highlighted
    Then "fn" is a keyword
    And '"world"' is a string

  Scenario: Comments are recognised
    Given "src/main.rs" contains:
      """
      // a note
      fn main() {}
      """
    When "src/main.rs" is highlighted
    Then "// a note" is a comment

  Scenario: The language follows the extension
    Given "app.js" contains:
      """
      const answer = 42;
      """
    When "app.js" is highlighted
    Then "const" is a keyword

  Scenario: An unknown extension renders as plain text
    Given "notes.zzz" contains:
      """
      fn main() {}
      """
    When "notes.zzz" is highlighted
    Then every token is plain text

  Scenario: A file with no extension renders as plain text
    Given "LICENSE" contains:
      """
      fn main() {}
      """
    When "LICENSE" is highlighted
    Then every token is plain text

  Scenario: An unknown extension is recognised by its first line
    Given "process.bpmn" contains:
      """
      <?xml version="1.0" encoding="UTF-8"?>
      <bpmn:task id="Task_1"/>
      """
    When "process.bpmn" is highlighted
    Then "id" is an attribute

  Scenario: Highlighting covers the whole line
    Given "src/main.rs" contains:
      """
      fn main() {}
      """
    When "src/main.rs" is highlighted
    Then the highlighted tokens reassemble to the original line

  Scenario: An operator is not a keyword
    Given "src/main.rs" contains:
      """
      let total = 1 + 2;
      """
    When "src/main.rs" is highlighted
    Then "let" is a keyword
    And "=" is an operator
    And "+" is an operator

  Scenario: A function call is a function, not plain text
    Given "src/main.rs" contains:
      """
      let n = compute(1);
      """
    When "src/main.rs" is highlighted
    Then "compute" is a function

  Scenario: A library type is a type
    Given "src/main.rs" contains:
      """
      let names: Vec<String> = read();
      """
    When "src/main.rs" is highlighted
    Then "Vec" is a type
    And "String" is a type

  Scenario: A language literal is a constant
    Given "src/main.rs" contains:
      """
      let ok = true;
      """
    When "src/main.rs" is highlighted
    Then "true" is a constant

  Scenario: Punctuation is told apart from identifiers
    Given "src/main.rs" contains:
      """
      let point: Pair = make(1, 2);
      """
    When "src/main.rs" is highlighted
    Then ":" is punctuation
    And "," is punctuation

  Scenario: An object property is a property
    Given "app.js" contains:
      """
      config.debug = false;
      """
    When "app.js" is highlighted
    Then "debug" is a property

  Scenario: An annotation is an attribute
    Given "src/main.rs" contains:
      """
      #[derive(Debug)]
      struct S;
      """
    When "src/main.rs" is highlighted
    Then "derive" is an attribute

  Scenario: A tag is markup and its attribute name is an attribute
    Given "index.html" contains:
      """
      <div class="a">hi</div>
      """
    When "index.html" is highlighted
    Then "div" is markup
    And "class" is an attribute

  Scenario: Emphasis in prose is markup
    Given "README.md" contains:
      """
      A **bold** word.
      """
    When "README.md" is highlighted
    Then "**bold**" is markup

  Scenario: Text a grammar reports as malformed is invalid
    Given "main.go" contains:
      """
      x := 09
      """
    When "main.go" is highlighted
    Then "09" is invalid

  Scenario: A TypeScript file is highlighted
    Given "src/app.ts" contains:
      """
      const total: number = add(1, 2);
      """
    When "src/app.ts" is highlighted
    Then "const" is a keyword
    And "number" is a type
    And "add" is a function

  Scenario: A Vue single-file component is highlighted
    Given "src/App.vue" contains:
      """
      <template><div class="a">hi</div></template>
      """
    When "src/App.vue" is highlighted
    Then "template" is markup
    And "class" is an attribute

  Scenario: A TOML file is highlighted
    Given "config.toml" contains:
      """
      theme = "base16-ocean.dark"
      width = 100
      """
    When "config.toml" is highlighted
    Then "theme" is markup
    And "=" is punctuation
    And '"base16-ocean.dark"' is a string
    And "100" is a number

  # A diff is not a whole source: it interleaves two of them. So each side is
  # highlighted whole and the rows are looked up in the side they came from —
  # which is what keeps a row inside a block comment coloured as a comment, and
  # a removed row coloured by the text that is going away rather than by
  # whatever replaced it.

  Scenario: A diff row is coloured by the language of the file under review
    Given "src/app.ts" held:
      """
      const total: number = add(1, 2);
      """
    And "src/app.ts" now holds:
      """
      const total: number = add(3, "return");
      """
    And the diff for "src/app.ts" is shown against HEAD
    When diff row 2 is highlighted
    Then "const" is a keyword
    And "number" is a type
    And "add" is a function
    And '"return"' is a string

  Scenario: A row inside a block comment is a comment rather than code
    Given "src/app.ts" held:
      """
      /* a note
         let x = 1;
         done */
      const n = 1;
      """
    And "src/app.ts" now holds:
      """
      /* a note
         let x = 1;
         done */
      const n = 2;
      """
    And the diff for "src/app.ts" is shown against HEAD
    When diff row 2 is highlighted
    Then "let x = 1;" is a comment

  Scenario: A removed row is coloured from the old side's text
    Given "src/app.ts" held:
      """
      const n = "return";
      """
    And "src/app.ts" now holds:
      """
      const n = 42;
      """
    And the diff for "src/app.ts" is shown against HEAD
    When diff row 1 is highlighted
    Then '"return"' is a string
    And the highlighted tokens reassemble to the original line

  Scenario: A file HEAD does not have is coloured from the side that exists
    Given "src/new.ts" now holds:
      """
      const total: number = 1;
      """
    And the diff for "src/new.ts" is shown against HEAD
    When diff row 1 is highlighted
    Then "const" is a keyword

  Scenario: A removed row whose old side cannot be read carries no tokens
    Given "src/gone.ts" is shown in the diff as deleted, having held:
      """
      const total: number = 1;
      """
    When diff row 1 is highlighted
    Then the diff row carries no tokens of its own

  Scenario: A diff of a file in an unknown language is plain text
    Given "notes.zzz" held:
      """
      fn main() {}
      """
    And "notes.zzz" now holds:
      """
      fn other() {}
      """
    And the diff for "notes.zzz" is shown against HEAD
    When diff row 2 is highlighted
    Then every token is plain text

  Rule: The editor's field can be turned off

    The code sits on a field a shade under the rest of the TUI, which lifts every
    token's contrast at once. It is a preference rather than part of the theme
    because a terminal cell painted with a background is opaque: the field is also
    what stops a transparent window showing the desktop through the code, and
    someone who chose that transparency wants it back. `:dim` toggles it, and the
    choice is remembered per project, like the last view and the tree divider.

    Scenario: The field is on to begin with
      Then the editor field is shown

    Scenario: The dim command takes the field away
      When I dim the editor from the command line
      Then the editor field is not shown

    Scenario: The dim command puts it back
      Given the editor field is off
      When I dim the editor from the command line
      Then the editor field is shown

    Scenario: Turning it off is remembered
      When I dim the editor from the command line
      Then the remembered editor field is "off"

    Scenario: A field turned off last session is still off
      Given the project ".varde/state.json" records the editor field as off
      When Varde starts in the project
      Then the editor field is not shown

    Scenario: State recorded before the field could be turned off leaves it on
      Given the project ".varde/state.json" records the last view as "Edit"
      When Varde starts in the project
      Then the editor field is shown
