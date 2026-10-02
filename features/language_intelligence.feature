Feature: Language intelligence

  Nothing in the workspace knows what an identifier *is*. Syntax highlighting colours `foo` because a
  grammar says it looks like a function; nothing knows whether `foo` exists, what type it has, where
  it is defined, or what may follow a dot. In a workspace whose whole premise is reviewing code an AI
  wrote, "does this compile" is the first question a reviewer has and the last one a diff answers.

  So Varde hosts a Language server: a second kind of hosted child, spawned from configuration, spoken
  to in JSON-RPC over its stdio, and never named in a branch —
  `docs/adr/0011-a-language-server-is-a-second-hosted-child.md` argues why the rule ADR-0004 states
  for a CLI in a Hosted pane transfers to a server, and why shipping defaults as TOML data in the
  bottom layer of the config merge is not the naming it forbids.

  The division of labour is the one the rest of Varde already draws. The edge spawns the process,
  frames the protocol, and reports what arrives; the core decides what to say, what a reply means, and
  what goes on screen. Whether a server exists at all is a fact only the edge can observe, so it is
  told to the core and never remembered by it — the failure `ai_running` demonstrated, in a second
  shape.

  No scenario here runs a Language server. Incoming messages are canned JSON driven straight in, the
  same discipline as never driving a real pty, and the assertions are on the state and the effects
  that result.

  Three absences carry as much of this feature as the capabilities do, because they are what a
  plausible-looking implementation breaks: with no server configured nothing is spawned and the editor
  behaves exactly as it does today; with a server that failed to start the Buffer is unchanged; and on
  dismissing the Candidate list the text is exactly what was typed.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the project is a git repository

  Rule: A server is a row in a config file, and nowhere else

    # ADR 0018: the template is what a fresh machine's global config is seeded
    # with, and it is the global layer of the very start that seeds it.
    Scenario: A common language has a server on a fresh install with nothing configured
      Given there is no global config
      And the project has no config file
      When Varde starts in the project
      Then a language server is configured for "rust"
      And a language server is configured for "typescript"

    Scenario: A global config naming no server starts no server for any language
      Given the global config is empty
      And the project has no config file
      When Varde starts in the project
      Then there is no language server configured for "rust"
      And there is no language server configured for "typescript"

    Scenario: The configured languages can be enumerated, not only looked up by name
      Given the global config is empty
      And the project config is:
        """
        [lsp.ada]
        command = "ada-language-server"

        [lsp.cobol]
        command = "cobol-language-server"
        """
      When Varde starts in the project
      Then the configured languages include "ada"
      And the configured languages include "cobol"

    Scenario: A project config overrides one language's server without disturbing the others
      Given the global config is:
        """
        [lsp.rust]
        command = "from-global-rust"

        [lsp.python]
        command = "from-global-python"
        """
      And the project config is:
        """
        [lsp.rust]
        command = "from-project-rust"
        """
      When Varde starts in the project
      Then the configured server command for "rust" is "from-project-rust"
      And the configured server command for "python" is "from-global-python"

    Scenario: A server row in the global config is used, and a project row overrides it key by key
      Given the global config is:
        """
        [lsp.rust]
        command = "from-global"
        args = ["--from-global"]
        extensions = ["rs"]
        """
      And the project config is:
        """
        [lsp.rust]
        args = ["--from-project"]
        """
      When Varde starts in the project
      Then the configured server command for "rust" is "from-global"
      And the configured server arguments for "rust" are:
        | arg            |
        | --from-project |

    Scenario: A server row in the global config is used when the project says nothing
      Given the global config is:
        """
        [lsp.rust]
        command = "from-global"
        """
      And the project has no config file
      When Varde starts in the project
      Then the configured server command for "rust" is "from-global"

    Scenario: Arguments travel with the command
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        args = ["--log-file", "/tmp/ra.log"]
        """
      When Varde starts in the project
      Then the configured server arguments for "rust" are:
        | arg         |
        | --log-file  |
        | /tmp/ra.log |

    Scenario: A language named in no layer has no server, and that is a state rather than an inference
      Given the global config is empty
      And the project config is:
        """
        [lsp.brainfuck]
        command = "bfls"
        """
      When Varde starts in the project
      Then the configured languages include "brainfuck"
      And the configured languages do not include "sanskrit"
      And there is no language server configured for "sanskrit"

    # A language is a row, not a match arm (ADR 0018): the row's `extensions`
    # are what sends a file to it, and its table name is the language id the
    # server is told.
    Scenario: A row for a language Varde never named serves the files it claims
      Given the project config is:
        """
        [lsp.ruby]
        command = "ruby-lsp"
        extensions = ["rb"]
        """
      And Varde started in the project
      And a language server for "ruby" is ready
      When I open "lib/app.rb"
      Then a language server was started with "ruby-lsp"
      And the language server for "ruby" was told "lib/app.rb" is a "ruby" document

    # A row's `language_ids` names the id for an extension the table name does
    # not cover: a .tsx file sent as "typescript" is parsed without JSX, so every
    # import used only as a tag reads as unused.
    Scenario: An extension a row gives its own language id is sent as that id
      Given a language server for "typescript" is ready
      When I open "src/App.tsx"
      Then the language server for "typescript" was told "src/App.tsx" is a "typescriptreact" document

    Scenario: Two rows claiming one extension stop Varde from starting
      Given the project config is:
        """
        [lsp.rustier]
        command = "rustier-ls"
        extensions = ["rs"]
        """
      When Varde starts in the project
      Then Varde refuses to start
      And the error names the file ".varde/config.toml"
      And the error names line 1
      And the fault is "extension-claimed-twice"
      And the error names the rows "lsp.rust" and "lsp.rustier"

    Scenario: A malformed server entry stops Varde from starting
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer
        """
      When Varde starts in the project
      Then Varde refuses to start
      And the error names the file ".varde/config.toml"
      And the error names line 2
      And the fault is "not-toml"

    Scenario: A layer names one key of a shipped language and keeps the command it did not name
      Given there is no global config
      And the project config is:
        """
        [lsp.rust]
        args = ["--log-file", "/tmp/ra.log"]
        """
      When Varde starts in the project
      Then the configured server command for "rust" is "rust-analyzer"
      And the configured server arguments for "rust" are:
        | arg         |
        | --log-file  |
        | /tmp/ra.log |

    Scenario: A layer names one key of a language only the global config introduced
      Given the global config is:
        """
        [lsp.brainfuck]
        command = "bfls"
        args = ["--from-global"]
        """
      And the project config is:
        """
        [lsp.brainfuck]
        args = ["--from-project"]
        """
      When Varde starts in the project
      Then the configured server command for "brainfuck" is "bfls"
      And the configured server arguments for "brainfuck" are:
        | arg            |
        | --from-project |

    Scenario: A language no layer ever gave a command stops Varde from starting
      Given the global config is empty
      And the project config is:
        """
        [lsp.brainfuck]
        args = ["--stdio"]
        """
      When Varde starts in the project
      Then Varde refuses to start
      And the error names the file ".varde/config.toml"
      And the error names line 1
      And the fault is "incomplete"

    Scenario: Configured initialization options reach the initialize request verbatim
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        initialization_options = { cargo = { features = ["all"] }, procMacro = true }
        """
      And Varde started in the project
      When I open "src/lib.rs"
      Then the initialize request for "rust" carried initialization options:
        """
        {"cargo": {"features": ["all"]}, "procMacro": true}
        """

    Scenario: A language with no initialization options says nothing about them
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      When I open "src/lib.rs"
      Then the initialize request for "rust" carried no initialization options

    Scenario: A project config overrides a global config's initialization options
      Given the global config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        extensions = ["rs"]
        initialization_options = { cargo = { features = ["from-global"] } }
        """
      And the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        initialization_options = { cargo = { features = ["from-project"] } }
        """
      And Varde started in the project
      When I open "src/lib.rs"
      Then the initialize request for "rust" carried initialization options:
        """
        {"cargo": {"features": ["from-project"]}}
        """

    Scenario: A malformed initialization options entry stops Varde from starting
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        initialization_options = 12
        """
      When Varde starts in the project
      Then Varde refuses to start
      And the error names the file ".varde/config.toml"
      And the error names line 3
      And the fault is "wrong-type"

    Scenario: Naming a server configures nothing else and spawns nothing
      Given the global config is empty
      And the project has no config file
      When Varde starts in the project
      Then no language server was started

  Rule: The handshake happens once per language, and nothing is asked before it completes

    Scenario: Opening a file in a configured language starts its server
      Given a language server "rust-analyzer" is configured for "rust"
      When I open "src/lib.rs"
      Then a language server was started with "rust-analyzer"
      And the language server for "rust" was sent an "initialize" request

    Scenario: The server is told the handshake is finished before it is told anything else
      Given a language server "rust-analyzer" is configured for "rust"
      And I open "src/lib.rs"
      When the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"hoverProvider":true,"definitionProvider":true,"completionProvider":{}}}
        """
      Then the language server for "rust" was sent an "initialized" notification
      And the language server for "rust" is ready

    Scenario: Nothing is sent to a server that has not finished its handshake
      Given a language server "rust-analyzer" is configured for "rust"
      When I open "src/lib.rs"
      Then the language server for "rust" is not ready
      And the language server for "rust" was sent no "textDocument/didOpen" notification

    Scenario: Once the handshake completes the open document goes
      Given a language server for "rust" is ready
      When I open "src/lib.rs"
      Then the language server for "rust" was told "src/lib.rs" is open

    Scenario: One server per language, reused across buffers
      Given a language server for "rust" is ready
      And I open "src/lib.rs"
      When I open "src/keys.rs"
      Then 1 language server was started for "rust"
      And the language server for "rust" was told "src/keys.rs" is open

    Scenario: A second language starts its own server
      Given a language server for "rust" is ready
      And a language server "typescript-language-server" is configured for "typescript"
      And I open "src/lib.rs"
      When I open "web/app.ts"
      Then a language server was started with "typescript-language-server"

    Scenario: A capability the server declined is never asked for
      Given a language server "rust-analyzer" is configured for "rust"
      And I open "src/lib.rs"
      When the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"definitionProvider":true}}
        """
      And I press "K" in the editor
      Then the language server for "rust" was sent no "textDocument/hover" request
      And the editor says "no-hover-support"

  Rule: The server sees the Buffer on screen, at the Buffer's own Document version

    Scenario: Opening sends the text and the Buffer's revision as the Document version
      Given a language server for "rust" is ready
      And "src/lib.rs" holds:
        """
        fn main() {}
        """
      When I open "src/lib.rs"
      Then the language server for "rust" was told "src/lib.rs" is open with:
        """
        fn main() {}
        """
      And the document version sent for "src/lib.rs" is 1

    Scenario: Every content change is sent, carrying the new revision
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor at revision 4
      And the editor mode is insert
      When I press "x" in the editor
      Then the language server for "rust" was told "src/lib.rs" changed
      And the document version sent for "src/lib.rs" is 5

    Scenario: The contents sent are the Buffer, not the file on disk
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk holds:
        """
        fn on_disk() {}
        """
      And "src/lib.rs" is open in the editor
      And the editor mode is insert
      When I type "!" in the editor
      Then the language server for "rust" was last told "src/lib.rs" holds:
        """
        !fn on_disk() {}
        """
      And "src/lib.rs" on disk still holds:
        """
        fn on_disk() {}
        """

    Scenario: A motion changes nothing, so the server is told nothing
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      When I press "j" in the editor
      Then the language server for "rust" was told "src/lib.rs" changed 0 times

  Rule: Nothing a server says is trusted about a Document version the Buffer has moved past

    Scenario: Diagnostics naming a version that is not the buffer's revision are dropped
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor at revision 7
      When the language server for "rust" publishes diagnostics for "src/lib.rs" at version 4:
        | line | severity | message           |
        | 3    | error    | cannot find value |
      Then "src/lib.rs" has no diagnostics

    Scenario: Diagnostics naming the buffer's current revision are kept
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor at revision 7
      When the language server for "rust" publishes diagnostics for "src/lib.rs" at version 7:
        | line | severity | message           |
        | 3    | error    | cannot find value |
      Then "src/lib.rs" has 1 diagnostic

    Scenario: Diagnostics carrying no version at all are kept
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor at revision 7
      When the language server for "rust" publishes diagnostics for "src/lib.rs" with no version:
        | line | severity | message           |
        | 3    | error    | cannot find value |
      Then "src/lib.rs" has 1 diagnostic

    Scenario: A reply to a request nobody made is dropped
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      When the language server for "rust" replies to request 999 with:
        """
        {"contents":{"kind":"plaintext","value":"fn main()"}}
        """
      Then no hover is shown

  Rule: Diagnostics mark the lines they belong to

    Two servers serve one `.vue` file, and each one has its own opinion of it: the Vue server marks a
    template mistake and a TypeScript server reports the type error. A push is a server's whole
    current opinion of a file, so what it replaces is *its own* last opinion and nothing else — a set
    kept per file alone made the second publisher erase the first, and the mark a reader was shown
    then depended on which server happened to speak last.

    Scenario: Varde says it can be told diagnostics, because it draws them
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        """
      And Varde started in the project
      When I open "src/lib.rs"
      Then the initialize request for "rust" said Varde can be told diagnostics

    Scenario: A diagnostic marks its line in the gutter
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      When the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message           |
        | 3    | error    | cannot find value |
      Then the gutter mark on line 3 of "src/lib.rs" is "error"
      And the gutter has no mark on line 2 of "src/lib.rs"

    Scenario Outline: Severities are distinguishable from one another
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      When the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity    | message |
        | 3    | <severity>  | a thing |
      Then the gutter mark on line 3 of "src/lib.rs" is "<severity>"

      Examples:
        | severity    |
        | error       |
        | warning     |
        | information |
        | hint        |

    Scenario: The message for the diagnostic under the cursor is shown
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk holds:
        """
        fn main() {
            wibble();
        }
        """
      And "src/lib.rs" is open in the editor
      And the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message                     |
        | 2    | error    | cannot find value `wibble`  |
      When the cursor is moved to line 2 column 5
      Then the diagnostic message shown is "cannot find value `wibble`"

    Scenario: With the cursor on a clean line no message is shown
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk holds:
        """
        fn main() {
            wibble();
        }
        """
      And "src/lib.rs" is open in the editor
      And the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message           |
        | 2    | error    | cannot find value |
      When the cursor is moved to line 1 column 1
      Then no diagnostic message is shown

    Scenario: Diagnostics survive switching to another buffer and back
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message           |
        | 3    | error    | cannot find value |
      When I open "src/keys.rs"
      And I open "src/lib.rs"
      Then "src/lib.rs" has 1 diagnostic
      And the gutter mark on line 3 of "src/lib.rs" is "error"

    Scenario: A later push replaces the earlier set rather than adding to it
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message           |
        | 3    | error    | cannot find value |
      When the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message      |
        | 8    | warning  | unused import |
      Then "src/lib.rs" has 1 diagnostic
      And the gutter has no mark on line 3 of "src/lib.rs"
      And the gutter mark on line 8 of "src/lib.rs" is "warning"

    Scenario: An empty push clears the file, which is how the gutter describes the present
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message           |
        | 3    | error    | cannot find value |
      When the language server for "rust" publishes no diagnostics for "src/lib.rs"
      Then "src/lib.rs" has no diagnostics

    Scenario: A file the server has said nothing about carries no marks
      Given a language server for "rust" is ready
      When I open "src/lib.rs"
      Then "src/lib.rs" has no diagnostics
      And the gutter has no mark on line 1 of "src/lib.rs"

    Scenario: Two servers publishing about one file both mark it
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 20 lines long
      And "src/App.vue" is open in the editor
      And the language server for "vue" publishes diagnostics for "src/App.vue":
        | line | severity | message             |
        | 3    | error    | missing closing tag |
      When the language server for "typescript" publishes diagnostics for "src/App.vue":
        | line | severity | message                                          |
        | 8    | error    | Type 'number' is not assignable to type 'string' |
      Then "src/App.vue" has 2 diagnostics
      And the gutter mark on line 3 of "src/App.vue" is "error"
      And the gutter mark on line 8 of "src/App.vue" is "error"
      And the language server for "typescript" was told "src/App.vue" is open

    Scenario: A server's push replaces its own last opinion and leaves the other's standing
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 20 lines long
      And "src/App.vue" is open in the editor
      And the language server for "vue" publishes diagnostics for "src/App.vue":
        | line | severity | message             |
        | 3    | error    | missing closing tag |
      And the language server for "typescript" publishes diagnostics for "src/App.vue":
        | line | severity | message                                          |
        | 8    | error    | Type 'number' is not assignable to type 'string' |
      When the language server for "vue" publishes diagnostics for "src/App.vue":
        | line | severity | message         |
        | 4    | warning  | unused property |
      Then "src/App.vue" has 2 diagnostics
      And the gutter has no mark on line 3 of "src/App.vue"
      And the gutter mark on line 4 of "src/App.vue" is "warning"
      And the gutter mark on line 8 of "src/App.vue" is "error"

    Scenario: One server saying the file is clean does not clear what the other said
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 20 lines long
      And "src/App.vue" is open in the editor
      And the language server for "vue" publishes diagnostics for "src/App.vue":
        | line | severity | message             |
        | 3    | error    | missing closing tag |
      And the language server for "typescript" publishes diagnostics for "src/App.vue":
        | line | severity | message                                          |
        | 8    | error    | Type 'number' is not assignable to type 'string' |
      When the language server for "vue" publishes no diagnostics for "src/App.vue"
      Then "src/App.vue" has 1 diagnostic
      And the gutter has no mark on line 3 of "src/App.vue"
      And the gutter mark on line 8 of "src/App.vue" is "error"

    Scenario: A stale version drops nothing, neither the sender's marks nor the other server's
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 20 lines long
      And "src/App.vue" is open in the editor at revision 7
      And the language server for "vue" publishes diagnostics for "src/App.vue" at version 7:
        | line | severity | message             |
        | 3    | error    | missing closing tag |
      And the language server for "typescript" publishes diagnostics for "src/App.vue" at version 7:
        | line | severity | message                                          |
        | 8    | error    | Type 'number' is not assignable to type 'string' |
      When the language server for "vue" publishes diagnostics for "src/App.vue" at version 4:
        | line | severity | message      |
        | 12   | error    | from an edit the reader has moved past |
      Then "src/App.vue" has 2 diagnostics
      And the gutter mark on line 3 of "src/App.vue" is "error"
      And the gutter mark on line 8 of "src/App.vue" is "error"
      And the gutter has no mark on line 12 of "src/App.vue"

    Scenario: The gutter width does not change when diagnostics arrive
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the editor gutter width is 10
      When the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message           |
        | 3    | error    | cannot find value |
      Then the editor gutter width is 10

  Rule: Hover says what the symbol under the cursor is

    Almost every server answers a hover in markdown, so a box that draws the reply verbatim shows
    `#`, `**` and fences as the characters they are spelled with — the markup instead of what it
    marks up. The box holds the same rows a Preview does, rendered by the same reader, which is why
    a fence in a hover is highlighted as its language rather than left as one flat colour.

    A reply the server labelled plain text is not read as markdown. `plaintext` is the server saying
    the `*` in it is an asterisk, and rendering it anyway is Varde inventing formatting the server
    denied having.

    Scenario: The binding asks the server about the symbol under the cursor
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the cursor is moved to line 1 column 4
      When I press "K" in the editor
      Then the language server for "rust" was sent a "textDocument/hover" request for "src/lib.rs" line 1 column 4

    Scenario: Type and documentation are both shown when the server provides both
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"markdown","value":"fn main()\n\nThe entry point."}}
        """
      Then the hover shows "fn main()"
      And the hover shows "The entry point."

    Scenario: A symbol the server knows nothing about says so rather than showing an empty box
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        null
        """
      Then no hover is shown
      And the editor says "nothing-known-here"

    Scenario: A hover reply that arrives after the cursor has moved is not shown
      Given a language server for "rust" is ready
      And "src/lib.rs" holds:
        """
        fn main() {
            let one = 1;
            let two = 2;
            let three = 3;
            let four = 4;
            println!("{one}{two}{three}{four}");
        }
        """
      And "src/lib.rs" is open in the editor
      And the cursor is moved to line 1 column 4
      And I press "K" in the editor
      When the cursor is moved to line 5 column 2
      And the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"plaintext","value":"fn main()"}}
        """
      Then no hover is shown

    Scenario: A long hover is wrapped before it is measured
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the screen is 12 rows by 40 columns
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"plaintext","value":"pub fn a_very_long_signature(one: usize, two: usize, three: usize) -> Result<Vec<String>, Error>"}}
        """
      Then no hover row is wider than 40 columns
      And the hover shows "Result<Vec<String>, Error>"

    Scenario: The hover does not cover the symbol it describes
      Given a language server for "rust" is ready
      And the screen is 26 rows by 100 columns
      And "src/lib.rs" holds:
        """
        fn main() {
            let one = 1;
            let two = 2;
            let three = 3;
            let four = 4;
            println!("{one}{two}{three}{four}");
        }
        """
      And "src/lib.rs" is open in the editor
      And the cursor is at line 6 column 4
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"plaintext","value":"fn main()"}}
        """
      Then the hover does not cover line 6

    Scenario: Markdown in a hover is rendered rather than shown as markup
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"markdown","value":"# Symbol\n\n**bold** and `code`"}}
        """
      Then the hover shows "Symbol"
      And no hover row holds "#"
      And no hover row holds "**"
      And no hover row holds "`"

    Scenario: A code fence in a hover is highlighted as its language
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"markdown","value":"```rust\nfn main()\n```"}}
        """
      Then the hover has 1 code row
      And "fn" in the hover's code row is a keyword
      And no hover row holds "```"

    Scenario: A hover sent as plain text keeps the characters it was sent as
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"plaintext","value":"*not* markdown"}}
        """
      Then the hover shows "*not* markdown"

    Scenario: A hover sent as a language string is shown as the code it is
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"language":"rust","value":"fn main()"}}
        """
      Then the hover has 1 code row
      And "fn" in the hover's code row is a keyword

    Scenario: A hover's code fence is not drawn wider than the screen
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the screen is 12 rows by 40 columns
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"markdown","value":"```rust\npub fn a_very_long_signature(one: usize, two: usize, three: usize) -> Result<Vec<String>, Error>\n```"}}
        """
      Then no hover row is wider than 40 columns

    Scenario: A hover on a wide screen is no wider than a column that reads
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the screen is 26 rows by 200 columns
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"plaintext","value":"Returns the number of elements in the collection, also referred to as its length, counted every time it is asked for."}}
        """
      Then no hover row is wider than 60 columns
      And the hover shows "Returns the number of elements in the collection, also referred to as its length, counted every time it is asked for."

    Scenario: A line too long for the box is broken rather than cut off
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the screen is 26 rows by 200 columns
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"markdown","value":"```rust\npub fn a_very_long_signature(one: usize, two: usize, three: usize) -> Result<Vec<String>, Error>\n```"}}
        """
      Then no hover row is wider than 60 columns
      And the hover shows "Result<Vec<String>, Error>"

    Scenario: A hover longer than the pane is capped rather than clipped
      Given a language server for "rust" is ready
      And the screen is 26 rows by 100 columns
      And "src/lib.rs" holds:
        """
        fn main() {
            let one = 1;
            let two = 2;
            let three = 3;
            let four = 4;
            println!("{one}{two}{three}{four}");
        }
        """
      And "src/lib.rs" is open in the editor
      And the cursor is at line 6 column 4
      And I press "K" in the editor
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"markdown","value":"one\n\ntwo\n\nthree\n\nfour\n\nfive\n\nsix\n\nseven\n\neight\n\nnine\n\nten\n\neleven\n\ntwelve\n\nthirteen\n\nfourteen\n\nfifteen\n\nsixteen\n\nseventeen\n\neighteen\n\nnineteen\n\ntwenty\n\ntwentyone"}}
        """
      Then the hover is no taller than 22 rows
      And the last hover row says it was cut short
      And the hover does not cover line 6

    Scenario: Varde says it can read markdown hovers, because it renders them
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        """
      And Varde started in the project
      When I open "src/lib.rs"
      Then the initialize request for "rust" said Varde can read markdown

    Scenario: Dismissing the hover leaves the buffer alone
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And a hover is shown
      When I press "Escape" in the editor
      Then no hover is shown
      And "src/lib.rs" has no unsaved edits

    Scenario: With no server for the language, hover does nothing and says why
      Given there is no language server configured for "cobol"
      And "legacy/report.cob" is open in the editor
      When I press "K" in the editor
      Then no hover is shown
      And the editor says "no-language-server"
      And no language server was started

  Rule: Go-to-definition opens where the name is introduced

    Scenario: The binding asks the server where the symbol is defined
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And the cursor is moved to line 4 column 9
      When I press "gd" in the editor
      Then the language server for "rust" was sent a "textDocument/definition" request for "src/lib.rs" line 4 column 9

    Scenario: A definition in the file already open moves the cursor without reopening the buffer
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And I press "gd" in the editor
      When the language server for "rust" answers the definition with:
        | path        | line | column |
        | src/lib.rs  | 12   | 5      |
      Then the cursor is at line 12 column 5
      And no file was opened in the editor

    Scenario: A definition in another workspace file opens that file at the right line
      Given a language server for "rust" is ready
      And "src/keys.rs" on disk is 120 lines long
      And "src/lib.rs" is open in the editor
      And I press "gd" in the editor
      When the language server for "rust" answers the definition with:
        | path         | line | column |
        | src/keys.rs  | 88   | 1      |
      Then the current buffer is "src/keys.rs"
      And the cursor is at line 88 column 1

    Scenario: Several definitions are listed rather than the first being taken quietly
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "gd" in the editor
      When the language server for "rust" answers the definition with:
        | path         | line | column |
        | src/keys.rs  | 88   | 1      |
        | src/tree.rs  | 14   | 1      |
      Then the hits are:
        | src/keys.rs | 88 |
        | src/tree.rs | 14 |
      And the current buffer is "src/lib.rs"

    Scenario: A symbol with no definition says so and moves nothing
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And the cursor is at line 4 column 9
      And I press "gd" in the editor
      When the language server for "rust" answers the definition with nothing
      Then the editor says "no-definition"
      And the cursor is at line 4 column 9
      And no file was opened in the editor

    Scenario: A definition outside the workspace root is refused, naming where it went
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And the cursor is moved to line 4 column 9
      And I press "gd" in the editor
      When the language server for "rust" answers the definition with:
        | path                                        | line | column |
        | /home/me/.cargo/registry/serde/src/lib.rs   | 4210 | 1      |
      Then the editor says "definition-outside-workspace"
      And the message names "/home/me/.cargo/registry/serde/src/lib.rs"
      And no file was opened in the editor
      And the cursor is at line 4 column 9

    Scenario: A definition reply for a cursor the user has moved away from is not acted on
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And the cursor is moved to line 4 column 9
      And I press "gd" in the editor
      When the cursor is moved to line 9 column 1
      And the language server for "rust" answers the definition with:
        | path         | line | column |
        | src/keys.rs  | 88   | 1      |
      Then no file was opened in the editor
      And the cursor is at line 9 column 1

    Scenario: With no server for the language, go-to-definition does nothing and says why
      Given there is no language server configured for "cobol"
      And "legacy/report.cob" is open in the editor
      When I press "gd" in the editor
      Then the editor says "no-language-server"
      And no file was opened in the editor
      And no language server was started

  Rule: One key asks everybody who serves the file, and the first answer is the answer

    A `.vue` file is served by the Vue server *and* by a TypeScript server; a linter server sits
    beside a type server; every arrangement other editors reach by attaching several clients to one
    buffer is this Rule. Which servers serve a path is configuration — the file's own language names
    who else serves its files — so no branch anywhere names a language.

    One keystroke is then several requests, and the replies arrive in whatever order the servers
    manage. The first non-empty one wins, and the empty-handed notice waits for the last: the server
    with no answer had less to look up, so it answers first, and letting it speak for the rest is a
    key that says nobody knows while somebody is still looking.

    Scenario Outline: One keystroke asks every server that serves the file
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 10 lines long
      And "src/App.vue" is open in the editor
      And the cursor is moved to line 4 column 15
      When I press "<key>" in the editor
      Then the language server for "vue" was sent a "<method>" request for "src/App.vue" line 4 column 15
      And the language server for "typescript" was sent a "<method>" request for "src/App.vue" line 4 column 15

      Examples:
        | key | method                  |
        | gd  | textDocument/definition |
        | K   | textDocument/hover      |

    Scenario: The server with no answer does not answer for the one that has one
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 10 lines long
      And "src/helper.ts" on disk is 20 lines long
      And "src/App.vue" is open in the editor
      And the cursor is moved to line 4 column 15
      And I press "gd" in the editor
      And the language server for "vue" answers the definition with nothing
      When the language server for "typescript" answers the definition with:
        | path          | line | column |
        | src/helper.ts | 2    | 17     |
      Then the current buffer is "src/helper.ts"
      And the cursor is at line 2 column 17
      And the editor never said "no-definition"

    Scenario: Nobody knowing is said once, when the last server has answered
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 10 lines long
      And "src/App.vue" is open in the editor
      And the cursor is moved to line 4 column 15
      And I press "gd" in the editor
      And the language server for "typescript" answers the definition with nothing
      When the language server for "vue" answers the definition with nothing
      Then the editor says "no-definition"
      And "no-definition" was reported 1 time
      And no file was opened in the editor

    Scenario: A second answer for the same keystroke is dropped rather than acted on
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 10 lines long
      And "src/helper.ts" on disk is 20 lines long
      And "src/keys.rs" on disk is 120 lines long
      And "src/App.vue" is open in the editor
      And the cursor is moved to line 4 column 15
      And I press "gd" in the editor
      And the language server for "vue" answers the definition with:
        | path          | line | column |
        | src/helper.ts | 2    | 17     |
      When the language server for "typescript" answers the definition with:
        | path        | line | column |
        | src/keys.rs | 88   | 1      |
      Then the current buffer is "src/helper.ts"
      And the cursor is at line 2 column 17

    Scenario: A server dying with the question outstanding leaves nothing waiting
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 10 lines long
      And "src/App.vue" is open in the editor
      And the cursor is moved to line 4 column 15
      And I press "gd" in the editor
      And the language server for "vue" answers the definition with nothing
      When the language server for "typescript" exits
      Then no language server request is outstanding for "typescript"
      And the editor says "language-server-stopped"

    Scenario: A file whose only server relays the question says so rather than blaming it
      Given a language server for "vue" is ready
      And the language server for "vue" relays "tsserver/request" to a companion Varde does not run
      And "src/App.vue" on disk is 10 lines long
      And "src/App.vue" is open in the editor
      And the cursor is moved to line 4 column 15
      And the language server for "vue" asks "tsserver/request" with:
        """
        [[1, "_vue:projectInfo", {"file": "src/App.vue"}]]
        """
      And I press "gd" in the editor
      When the language server for "vue" answers the definition with nothing
      Then the editor says "needs-a-companion"
      And the editor never said "no-definition"
      And no file was opened in the editor

    Scenario: A server that could have relayed and did not is a server that knows nothing
      Given a language server for "vue" is ready
      And the language server for "vue" relays "tsserver/request" to a companion Varde does not run
      And "src/App.vue" on disk is 10 lines long
      And "src/App.vue" is open in the editor
      And the cursor is moved to line 4 column 15
      And I press "gd" in the editor
      When the language server for "vue" answers the definition with nothing
      Then the editor says "no-definition"
      And the editor never said "needs-a-companion"

  Rule: Candidates are offered while typing, and are never something to undo

    Scenario: Typing an identifier asks the server for candidates
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the editor mode is insert
      When I type "wor" in the editor
      And the debounce window passes
      Then the language server for "rust" was sent a "textDocument/completion" request

    Scenario: The candidates the server returns are offered
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
        | working_dir    | working_dir    |
      Then the candidate list is open
      And the candidates are:
        | workspace_root |
        | working_dir    |
      And the selected candidate is "workspace_root"

    Scenario: Arrow keys move the choice
      Given a candidate list is open in the editor with:
        | workspace_root |
        | working_dir    |
      When I press "Down" in the editor
      Then the selected candidate is "working_dir"

    Scenario: The choice does not run off the end of the list
      Given a candidate list is open in the editor with:
        | workspace_root |
        | working_dir    |
      When I press "Down" in the editor
      And I press "Down" in the editor
      Then the selected candidate is "working_dir"

    Scenario: Enter accepts the selected candidate and closes the list
      Given "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And a candidate list is open in the editor with:
        | workspace_root |
        | working_dir    |
      When I press "Enter" in the editor
      Then the buffer holds:
        """
        workspace_root
        """
      And the candidate list is not open

    Scenario: Escape leaves the buffer holding exactly the characters that were typed
      Given "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And a candidate list is open in the editor with:
        | workspace_root |
        | working_dir    |
      When I press "Escape" in the editor
      Then the buffer holds:
        """
        wor
        """
      And the candidate list is not open

    Scenario: The list goes when a jump lands in another file
      Given "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And a candidate list is open in the editor with:
        | workspace_root |
        | working_dir    |
      When I jump to "src/other.rs" line 1
      Then the candidate list is not open
      And the project "src/other.rs" is unchanged

    Scenario: No candidate matching the prefix closes the list rather than showing an empty one
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the editor mode is insert
      And I type "zzz" in the editor
      And the debounce window passes
      When the language server for "rust" answers with no candidates
      Then the candidate list is not open

    Scenario: The candidate list does not cover the line being edited
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And the cursor is at line 3 column 4
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
      Then the candidate list is open
      And the candidate list does not cover line 3

    Scenario: A reply of several hundred candidates leaves the editor's lines visible
      Given a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And the cursor is at line 3 column 4
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When the language server for "rust" answers with 300 candidates
      Then the candidate list is open
      And the candidate list is 12 rows tall
      And the candidate list does not cover line 3

    Scenario: The choice scrolls the window rather than growing it
      Given a candidate list is open in the editor with 300 candidates
      When I press "Down" in the editor 12 times
      Then the candidate list is 12 rows tall
      And the first candidate shown is "worklist_005"
      And the selected candidate is "worklist_013"

    Scenario: The box sits at the column the cursor is in
      Given the editor pane is 60 columns wide
      And a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And the cursor is at line 3 column 18
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
      Then the candidate list starts at column 21

    Scenario: A box that would run off the right edge is shifted left
      Given the editor pane is 30 columns wide
      And the minimap is turned off
      And a language server for "rust" is ready
      And "src/lib.rs" on disk is 20 lines long
      And "src/lib.rs" is open in the editor
      And the cursor is at line 3 column 18
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
      Then the candidate list starts at column 6

    Scenario: A character typed while the list is open narrows it rather than closing it
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
        | working_dir    | working_dir    |
        | word_count     | word_count     |
      When I type "k" in the editor
      Then the candidate list is open
      And the candidates are:
        | workspace_root |
        | working_dir    |

    Scenario: Backspace widens the list back to the whole reply
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
        | working_dir    | working_dir    |
        | word_count     | word_count     |
      And I type "k" in the editor
      When I press "Backspace" in the editor
      Then the candidate list is open
      And the candidates are:
        | workspace_root |
        | working_dir    |
        | word_count     |

    Scenario: Typing past every candidate closes the list
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
      When I type "q" in the editor
      Then the candidate list is not open

    Scenario: A candidate the prefix does not match is not offered at all
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
        | zzz            | zzz            |
        | working_dir    | working_dir    |
      Then the candidates are:
        | workspace_root |
        | working_dir    |

    Scenario: A reply nothing in which matches the prefix closes the list
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When the language server for "rust" answers with candidates:
        | label | insert |
        | zzz   | zzz    |
      Then the candidate list is not open

    Scenario: The order the server asked for is the order the list is in
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When the language server for "rust" answers with candidates:
        | label         | insert    | filter    | sort |
        | workspace     | workspace | worse     | 0003 |
        | Write to disk | work_done | work_done | 0002 |
        | wormhole      | wormhole  | wormhole  | 0001 |
      Then the candidates are:
        | wormhole      |
        | Write to disk |
        | workspace     |

    Scenario: The server's own filter text is what a typed character narrows against
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label         | insert    | filter    | sort |
        | workspace     | workspace | worse     | 0003 |
        | Write to disk | work_done | work_done | 0002 |
        | wormhole      | wormhole  | wormhole  | 0001 |
      When I type "k" in the editor
      Then the candidates are:
        | Write to disk |

    Scenario: Fast typing does not queue a request for text already replaced
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the editor mode is insert
      When I type "w" in the editor
      And I type "o" in the editor
      And I type "r" in the editor
      And the debounce window passes
      Then the language server for "rust" was sent 1 "textDocument/completion" request

    Scenario: Nothing is asked before the debounce window has passed
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the editor mode is insert
      When I type "wor" in the editor
      Then the language server for "rust" was sent no "textDocument/completion" request

    Scenario: A reply for a prefix the user has typed past is dropped
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      When I type "k" in the editor
      And the language server for "rust" answers with candidates:
        | label          | insert         |
        | workspace_root | workspace_root |
      Then the candidate list is not open

    Scenario: With no server for the language typing behaves exactly as it does today
      Given there is no language server configured for "cobol"
      And "legacy/report.cob" is open in the editor holding nothing
      And the editor mode is insert
      When I type "wor" in the editor
      And the debounce window passes
      Then the candidate list is not open
      And no language server was started
      And the buffer holds:
        """
        wor
        """

  Rule: A completion fills in its own blanks

    Varde declares `completionItem.snippetSupport`, and that declaration is a claim about what Varde
    can *do* with a reply rather than a request to be sent one. Made any earlier than the code that
    honours it, every server starts answering with `${1:…}` and the placeholders reach the buffer as
    the characters they are spelled with — a defect that looks like the server's and is not.

    A snippet's stops are where the values go. Accepting one leaves the cursor on the first, Tab
    moves to the next, and the last leaves it where the code continues and ends the sequence. Escape
    ends the sequence and keeps the text, because abandoning a completion is not undoing it. A
    server that offers no snippet is untouched by all of it: its text goes in, the cursor lands
    after it, and there is no sequence to be in.

    Scenario: Varde says it can receive snippets, because it honours them
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        """
      And Varde started in the project
      When I open "src/lib.rs"
      Then the initialize request for "rust" said Varde can receive snippets

    Scenario: Accepting a snippet inserts the text with the placeholders gone
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "pri" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label    | insert            | format  |
        | println! | println!("$1")$0  | snippet |
      When I press "Enter" in the editor
      Then the buffer holds:
        """
        println!("")
        """
      And the cursor is at line 1 column 11

    Scenario: Tab moves to the next place a value is needed
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "foo" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label | insert        | format  |
        | foo   | foo($1, $2)$0 | snippet |
      And I press "Enter" in the editor
      And I type "x" in the editor
      When I press "Tab" in the editor
      Then the buffer holds:
        """
        foo(x, )
        """
      And the cursor is at line 1 column 8

    Scenario: The last stop leaves the cursor where the code continues, and the sequence is over
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "foo" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label | insert        | format  |
        | foo   | foo($1, $2)$0 | snippet |
      And I press "Enter" in the editor
      And I type "x" in the editor
      And I press "Tab" in the editor
      And I type "y" in the editor
      When I press "Tab" in the editor
      Then the buffer holds:
        """
        foo(x, y)
        """
      And the cursor is at line 1 column 10
      And no tab stops are pending

    Scenario: Escape ends the sequence and leaves the text where it is
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "pri" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label    | insert            | format  |
        | println! | println!("$1")$0  | snippet |
      And I press "Enter" in the editor
      When I press "Escape" in the editor
      Then the buffer holds:
        """
        println!("")
        """
      And no tab stops are pending
      And the editor mode is insert

    Scenario: Opening another file ends the sequence rather than carrying it into that file
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "pri" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label    | insert            | format  |
        | println! | println!("$1")$0  | snippet |
      And I press "Enter" in the editor
      When I open "src/tree.rs"
      Then no tab stops are pending

    Scenario: A plain-text completion is exactly what it is today, with no sequence to leave
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor holding nothing
      And the editor mode is insert
      And I type "wor" in the editor
      And the debounce window passes
      And the language server for "rust" answers with candidates:
        | label          | insert         | format |
        | workspace_root | workspace_root | plain  |
      When I press "Enter" in the editor
      Then the buffer holds:
        """
        workspace_root
        """
      And the cursor is at line 1 column 15
      And no tab stops are pending

  Rule: The server reformats as it is typed

    Where a server offers `onTypeFormatting` it names the characters it wants to be told about, and
    that list is the server's own: measured, rust-analyzer asks about `.` and `=`, jdtls about `;`,
    `}` and a newline, and clangd about a newline alone. No two agree and none of it is guessable
    from the language, so nothing in Varde spells a trigger character.

    Nobody pressed a key for this — they typed a brace. So it is silent both ways: a server that
    offers no formatter, one that names other characters and one that errors all leave the text
    exactly as it was typed, with nothing said. And the edits, when they come, are one edit: `u`
    means "undo that reformat", not "undo one sixth of it". A reply that arrives after the reader
    has typed on is about a document that no longer exists, and is dropped rather than applied.

    Scenario: Typing a character the server named asks it to reformat around it
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentOnTypeFormattingProvider":{"firstTriggerCharacter":"}"}}}
        """
      And the editor mode is insert
      And the cursor is at line 2 column 19
      And I press "Enter" in the editor
      When I type "}" in the editor
      Then the language server for "rust" was asked to format after "}" at line 3 column 10

    Scenario: The edits the server sends back move the lines it named
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentOnTypeFormattingProvider":{"firstTriggerCharacter":"}"}}}
        """
      And the editor mode is insert
      And the cursor is at line 2 column 19
      And I press "Enter" in the editor
      And I type "}" in the editor
      When the language server for "rust" answers the formatting with:
        | line | from | to | text |
        | 2    | 1    | 4  |      |
        | 3    | 1    | 8  |      |
      Then the buffer holds:
        """
        fn main() {
            let x = 1;
        }
        """
      And the cursor is at line 3 column 2

    Scenario: A reformat is one press of undo, however many edits it was made of
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentOnTypeFormattingProvider":{"firstTriggerCharacter":"}"}}}
        """
      And the editor mode is insert
      And the cursor is at line 2 column 19
      And I press "Enter" in the editor
      And I type "}" in the editor
      And the language server for "rust" answers the formatting with:
        | line | from | to | text |
        | 2    | 1    | 4  |      |
        | 3    | 1    | 8  |      |
      And I press "Escape" in the editor
      When I press "u" in the editor
      Then the buffer holds:
        """
        fn main() {
                let x = 1;
                }
        """

    Scenario: A reply that arrives after the reader has typed on is dropped
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentOnTypeFormattingProvider":{"firstTriggerCharacter":"}"}}}
        """
      And the editor mode is insert
      And the cursor is at line 2 column 19
      And I press "Enter" in the editor
      And I type "}" in the editor
      And I type "x" in the editor
      When the language server for "rust" answers the formatting with:
        | line | from | to | text |
        | 3    | 1    | 8  |      |
      Then the buffer holds:
        """
        fn main() {
                let x = 1;
                }x
        """

    Scenario: A character the server did not name asks it nothing
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentOnTypeFormattingProvider":{"firstTriggerCharacter":";"}}}
        """
      And the editor mode is insert
      And the cursor is at line 2 column 19
      And I press "Enter" in the editor
      When I type "}" in the editor
      Then the language server for "rust" was sent no "textDocument/onTypeFormatting" request
      And the buffer holds:
        """
        fn main() {
                let x = 1;
                }
        """

    Scenario: Typing in a language whose server offers no formatting is untouched
      Given a language server for "rust" is ready
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
        """
      And I open "src/lib.rs"
      And the editor mode is insert
      And the cursor is at line 2 column 19
      And I press "Enter" in the editor
      When I type "}" in the editor
      Then the language server for "rust" was sent no "textDocument/onTypeFormatting" request
      And the buffer holds:
        """
        fn main() {
                let x = 1;
                }
        """
      And no notice was raised

  Rule: Review view says whether the change under review carries errors

    Background:
      Given the working tree contains:
        | path        | git status |
        | src/keys.rs | modified   |
        | src/ui.rs   | modified   |
        | web/app.ts  | modified   |

    Scenario: Entering Review view tells a running server about the changed files it serves
      Given a language server for "rust" is already running
      When I open Review view
      Then the language server for "rust" was told "src/keys.rs" is open
      And the language server for "rust" was told "src/ui.rs" is open
      And no language server was started

    Scenario: A changed file synced for the review is not opened as a Buffer
      Given a language server for "rust" is already running
      When I open Review view
      Then the editor has no file open
      And the language server for "rust" was told "src/keys.rs" is open

    Scenario: A changed file in a language with no running server is told to nobody
      Given a language server "typescript-language-server" is configured for "typescript"
      And there is no language server running for "typescript"
      When I open Review view
      Then no language server was started
      And "web/app.ts" is not measured in the review

    Scenario: Each changed file shows how many errors and warnings its server reports
      Given a language server for "rust" is already running
      And the language server for "rust" publishes diagnostics for "src/keys.rs":
        | line | severity | message           |
        | 3    | error    | cannot find value |
        | 9    | warning  | unused import     |
      When I open Review view
      Then the review diagnostic counts are:
        | file        | errors | warnings |
        | src/keys.rs | 1      | 1        |

    Scenario: A file two servers report on is counted across both of them
      Given the working tree contains:
        | path        | git status |
        | src/App.vue | modified   |
      And a language server for "vue" is already running
      And a language server for "typescript" is already running
      And "vue" files are also served by the language server for "typescript"
      And the language server for "vue" publishes diagnostics for "src/App.vue":
        | line | severity | message             |
        | 3    | error    | missing closing tag |
      And the language server for "typescript" publishes diagnostics for "src/App.vue":
        | line | severity | message                                          |
        | 8    | error    | Type 'number' is not assignable to type 'string' |
        | 9    | warning  | unused variable                                  |
      When I open Review view
      Then the review diagnostic counts are:
        | file        | errors | warnings |
        | src/App.vue | 2      | 1        |

    Scenario: A total across the files under review is shown
      Given a language server for "rust" is already running
      And the language server for "rust" publishes diagnostics for "src/keys.rs":
        | line | severity | message           |
        | 3    | error    | cannot find value |
      And the language server for "rust" publishes diagnostics for "src/ui.rs":
        | line | severity | message           |
        | 7    | error    | mismatched types  |
        | 9    | warning  | unused import     |
      When I open Review view
      Then the review error total is 2
      And the review warning total is 1

    Scenario: A file in a language with no server reads as not measured, never as zero
      Given there is no language server configured for "typescript"
      When I open Review view
      Then "web/app.ts" is not measured in the review
      And the review diagnostic count for "web/app.ts" is not 0

    Scenario: A file whose server has not answered yet reads as not measured
      Given a language server for "rust" is already running
      And the language server for "rust" has published no diagnostics for "src/ui.rs"
      When I open Review view
      Then "src/ui.rs" is not measured in the review

    Scenario: It stops being not measured when the server answers
      Given a language server for "rust" is already running
      And I opened Review view
      When the language server for "rust" publishes no diagnostics for "src/ui.rs"
      Then "src/ui.rs" is measured in the review
      And the review diagnostic count for "src/ui.rs" is 0

    Scenario: The counts follow the files under review as the review's contents change
      Given a language server for "rust" is already running
      And I opened Review view
      When "src/ui.rs" is restored to what HEAD holds
      And "web/app.ts" is restored to what HEAD holds
      Then the review diagnostic counts name exactly:
        | src/keys.rs |

    Scenario: Opening Review view starts no server for a file nobody opened
      Given a language server "rust-analyzer" is configured for "rust"
      When I open Review view
      Then no language server was started

  Rule: Losing a server takes nothing away

    Scenario: With no server configured nothing is spawned and the editor is as it is today
      Given there is no language server configured for "cobol"
      When I open "legacy/report.cob"
      Then no language server was started
      And "legacy/report.cob" has no diagnostics
      And the current buffer is "legacy/report.cob"

    Scenario: A server whose command does not exist leaves the buffer unchanged
      Given a language server "not-installed-anywhere" is configured for "rust"
      And I open "src/lib.rs"
      When the language server for "rust" fails to start
      Then "src/lib.rs" has no unsaved edits
      And the current buffer is "src/lib.rs"
      And the editor says "language-server-failed"

    Scenario: The failure is surfaced once, not on every keystroke
      Given a language server "not-installed-anywhere" is configured for "rust"
      And I open "src/lib.rs"
      And the language server for "rust" fails to start
      When I open "src/keys.rs"
      Then "language-server-failed" was reported 1 time

    Scenario: A server that exits is observed by the edge and told to the core
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And the language server for "rust" publishes diagnostics for "src/lib.rs":
        | line | severity | message           |
        | 3    | error    | cannot find value |
      When the language server for "rust" exits
      Then the language server for "rust" is not ready
      And "src/lib.rs" has no diagnostics
      And "src/lib.rs" has no unsaved edits

    Scenario: A server that dies takes its own marks and leaves the other server's
      Given a language server for "vue" is ready
      And a language server for "typescript" is ready
      And "vue" files are also served by the language server for "typescript"
      And "src/App.vue" on disk is 20 lines long
      And "src/App.vue" is open in the editor
      And the language server for "vue" publishes diagnostics for "src/App.vue":
        | line | severity | message             |
        | 3    | error    | missing closing tag |
      And the language server for "typescript" publishes diagnostics for "src/App.vue":
        | line | severity | message                                          |
        | 8    | error    | Type 'number' is not assignable to type 'string' |
      When the language server for "vue" exits
      Then "src/App.vue" has 1 diagnostic
      And the gutter has no mark on line 3 of "src/App.vue"
      And the gutter mark on line 8 of "src/App.vue" is "error"

    Scenario: A request outstanding when the server dies is dropped, not left waiting
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "K" in the editor
      When the language server for "rust" exits
      Then no hover is shown
      And no language server request is outstanding for "rust"

    Scenario: A reply arriving from a language whose server has gone is dropped
      Given a language server for "rust" is ready
      And "src/lib.rs" is open in the editor
      And I press "K" in the editor
      And the language server for "rust" exits
      When the language server for "rust" answers the hover with:
        """
        {"contents":{"kind":"plaintext","value":"fn main()"}}
        """
      Then no hover is shown

  Rule: A server that is not installed can be installed from the palette, and Varde composes nothing

    The gap this closes is the one the rule above leaves open: configuration names a command, the
    command is not on this machine, and the reader is told once and left with a plain editor. The
    list of what could serve this workspace is already data Varde holds — the languages
    configuration names — so the palette can show it, and a key on a row can offer to install one.

    Where the install command comes from is the whole of the design.
    `docs/adr/0012-an-install-command-is-configuration.md` argues why it is one more key in the
    `[lsp.<language>]` table, shipped as TOML data exactly as the server names are: a match on
    language and OS inside the library is ADR 0011's forbidden arm with a package manager's name in
    it as well as a server's. So the assertion throughout is on the command Varde *ran*, which is
    the string configuration carried and nothing Varde composed.

    The command runs in the shell pane, visibly, where a `sudo` prompt can be answered and a default
    that is wrong for this machine fails on screen with its exit status recorded
    (`docs/adr/0018-the-global-config-is-the-list-of-programs.md` reverses ADR 0012's "typed, never
    run").

    Whether a command resolves on PATH is a fact only the edge can observe, so scenarios state it as
    one — the same way they state that a server is running. Which OS the binary was built for is
    handed in at startup, as the running version already is, so a Linux row is specifiable on a Mac.

    A row's claim is about the workspace, not about the filesystem. `which` finding a path is not a
    server answering: rustup installs a shim for every component whether or not the component is
    there, so a command that exists, is executable and dies on its first message reads as installed
    and serves nothing. What tells the two apart is already in the core — a server the edge stopped
    holding leaves a written-off conversation behind — so the row reads that rather than probing, and
    nothing is spawned to find out what a row says.

    Scenario: The palette offers Tools, and its servers are what configuration names
      Given there is no global config
      And the project has no config file
      When I open the palette
      And I press "v" in the palette
      Then the palette is listing tools
      And the list offers a row for "rust"
      And the list offers a row for "typescript"
      And the list offers a row for "java"
      And the row for "rust" names the command "rust-analyzer"

    Scenario: A row says whether its command is on this machine
      Given the command "rust-analyzer" is on PATH
      And the command "zls" is not on PATH
      And the command "brew" is on PATH
      When I open Tools
      Then the row for "rust" is "installed"
      And the row for "zig" is "missing"

    Scenario: A project's own server appears in the list instead of the default
      Given the project config is:
        """
        [lsp.rust]
        command = "/opt/ra/rust-analyzer"
        """
      When I open Tools
      Then the row for "rust" names the command "/opt/ra/rust-analyzer"

    Scenario: Installing a row runs its configured command for this OS in the shell pane
      Given Varde was built for "macos"
      And the command "zls" is not on PATH
      And the command "brew" is on PATH
      And the project config is:
        """
        [lsp.zig]
        command = "zls"
        install.macos = "brew install zls"
        install.linux = "zig build -Doptimize=ReleaseSafe"
        """
      When I open Tools
      And I install the row for "zig"
      Then the shell pane runs "brew install zls" reporting its exit status
      And the focus is the terminal

    Scenario: The same row on another OS runs that OS's command
      Given Varde was built for "linux"
      And the command "zls" is not on PATH
      And the command "zig" is on PATH
      And the project config is:
        """
        [lsp.zig]
        command = "zls"
        install.macos = "brew install zls"
        install.linux = "zig build -Doptimize=ReleaseSafe"
        """
      When I open Tools
      And I install the row for "zig"
      Then the shell pane runs "zig build -Doptimize=ReleaseSafe" reporting its exit status

    Scenario: A template install command needs no row written by hand
      Given Varde was built for "macos"
      And there is no global config
      And the project has no config file
      And the command "gopls" is not on PATH
      And the command "go" is on PATH
      When I open Tools
      And I install the row for "go"
      Then the shell pane runs a command mentioning "gopls"

    Scenario: A global config overrides a shipped install command
      Given Varde was built for "macos"
      And the command "gopls" is not on PATH
      And the command "my-own-installer" is on PATH
      And the global config is:
        """
        [lsp.go]
        command = "gopls"
        install.macos = "my-own-installer gopls"
        """
      When I open Tools
      And I install the row for "go"
      Then the shell pane runs "my-own-installer gopls" reporting its exit status

    Scenario: A language with no install command for this OS says so and offers nothing
      Given Varde was built for "linux"
      And the command "zls" is not on PATH
      And the project config is:
        """
        [lsp.zig]
        command = "zls"
        install.macos = "brew install zls"
        """
      When I open Tools
      Then the row for "zig" is "no-install-command"
      When I install the row for "zig"
      Then the terminal is offered nothing
      And no command has been executed

    Scenario: A server Varde watched die reads as stopped, not as installed
      Given Varde started in the project
      And "src/lib.rs" is open in the editor
      And the command "rust-analyzer" is on PATH
      When the language server for "rust" exits
      And I open Tools
      Then the row for "rust" is "stopped"

    Scenario: Opening the list spawns nothing to find out what a row says
      Given the command "rust-analyzer" is on PATH
      When I open Tools
      Then the row for "rust" is "installed"
      And no language server was started

    Scenario: A row already installed and answering is not installed again
      Given the command "rust-analyzer" is on PATH
      When I open Tools
      Then the row for "rust" is "installed"
      When I install the row for "rust"
      Then the terminal is offered nothing
      And the editor refuses with "tool-already-installed"

    Scenario: A row that reads stopped runs its install command rather than refusing
      Given Varde was built for "macos"
      And the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        install.macos = "install-the-rust-server"
        """
      And Varde started in the project
      And "src/lib.rs" is open in the editor
      And the command "rust-analyzer" is on PATH
      And the language server for "rust" exits
      When I install the row for "rust"
      Then the shell pane runs "install-the-rust-server" reporting its exit status

    Scenario: A stopped row with no install command for this OS offers nothing
      Given Varde was built for "linux"
      And Varde started in the project
      And "src/main.zig" is open in the editor
      And the command "zls" is on PATH
      And the language server for "zig" exits
      When I install the row for "zig"
      Then the row for "zig" is "stopped"
      And the terminal is offered nothing
      And no command has been executed

    Scenario: A command that appears is a reason to forget that it was missing
      Given a language server for "rust" failed to start
      And "src/lib.rs" is open in the editor
      When the command "rust-analyzer" is on PATH
      Then a language server is started for "rust"

    Scenario: A command that is still missing is not asked for again
      Given a language server for "rust" failed to start
      And "src/lib.rs" is open in the editor
      When the command "rust-analyzer" is not on PATH
      Then no language server is started

    Scenario: A re-check that still finds nothing offers a restart
      Given the command "zls" is not on PATH
      And the command "brew" is on PATH
      And I asked to install the row for "zig"
      When I re-check the row for "zig"
      And the command "zls" is not on PATH
      Then Varde asks whether to restart

    Scenario: Declining the restart leaves the workspace exactly as it was
      Given Varde is asking whether to restart
      And "src/lib.rs" is open in the editor holding:
        """
        fn main() {}
        """
      When I decline the restart
      Then the palette is closed
      And "src/lib.rs" has no unsaved edits
      And no command has been executed

    Scenario: A re-check that finds the command does not offer a restart
      Given the command "zls" is not on PATH
      And the command "brew" is on PATH
      And I asked to install the row for "zig"
      When I re-check the row for "zig"
      And the command "zls" is on PATH
      Then the row for "zig" is "installed"
      And Varde is not asking whether to restart

    Scenario: The list is left without installing anything
      Given the command "zls" is not on PATH
      When I open Tools
      And I press "Escape" in the palette
      Then the palette is closed
      And the terminal is offered nothing
      And no command has been executed

  Rule: A value that is a path on this machine is named in configuration and found by the edge

    R31.25 promises that installing the server is the whole of what the reader does, and the one thing
    that promise cannot survive is a value that differs per machine. `@vue/language-server` reads its
    TypeScript SDK from `--tsdk=`, and every Vue workspace already has that directory: nobody should
    have to say where it is, and nobody can say it once for two projects.

    So configuration names the fact and the edge finds it. `[facts.<name>]` says which marker file
    means "this directory configures the language" and whether the marker or its directory is the
    answer; the edge runs one search for all of them — from the directory of the file being served up
    to the workspace root, nearest first, never above it — and nothing in the library reads a
    filesystem to do it. The answers arrive the way `PATH` does, written by the edge and only read by
    the core. Which marker is data for the reason a command is data: `node_modules/typescript/lib`,
    `.venv/bin/python` and `compile_commands.json` are one question asked three times, and an arm per
    ecosystem is a server's name in an arm with more in it.

    Nearest-first is the whole point of walking from the file. A monorepo installs its dependencies
    per package, so the compiler that must typecheck a package's files is the one that package
    installed — a root that has none is not an answer, and a global 7.0 preview typechecking a project
    pinned to 5.6 is a wrong one.

    A fact the edge could not answer is a server that does not start. The row already says
    `missing-requirement`, and launching the server anyway is how a Vue server came to be spawned
    without the SDK the requirement was about and to die on the first file — a crash the reader has to
    interpret is strictly worse than the honest row. Nothing is written off by the skip, because
    nothing died: the pass that runs when the fact appears starts the server exactly as a fresh start
    would. A name no `[facts.*]` table declares is not a placeholder at all and is left alone.

    A fact the server cannot start without and a fact it is merely better with are two different
    facts, and only configuration can say which is which. `optional = true` is the second: the key
    that named it is dropped exactly as it already would be, and the server starts anyway. Without
    the distinction a plugin one language wants would have to be named on the server another language
    shares — and a machine that never installed that plugin would lose the shared server in every
    project, which is a requirement nobody declared.

    Optional is about the *spawn*, not about the value. An optional fact that was found is filled in
    like any other, and a row whose optional fact went unfound is missing no requirement — nothing is
    missing.

    Scenario: A shipped default names the SDK, and the edge's answer reaches the spawn
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And the edge resolved "typescript_sdk" to "/home/me/project/node_modules/typescript/lib"
      When I open "src/App.vue"
      Then a language server was started with "vue-language-server"
      And the language server for "vue" was started with arguments:
        | arg                                                |
        | --stdio                                            |
        | --tsdk=/home/me/project/node_modules/typescript/lib |

    Scenario: An SDK the edge could not find starts no server at all
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And the command "vue-language-server" is on PATH
      And the edge resolved no "typescript_sdk"
      When I open "src/App.vue"
      Then no language server was started
      And the row for "vue" is "missing-requirement"

    Scenario: The SDK appearing starts the server on the next pass, with no restart
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And the edge resolved no "typescript_sdk"
      And I open "src/App.vue"
      When the edge resolves "typescript_sdk" to "/home/me/project/components/frontend/node_modules/typescript/lib"
      Then a language server was started with "vue-language-server"
      And the language server for "vue" was started with arguments:
        | arg                                                                       |
        | --stdio                                                                   |
        | --tsdk=/home/me/project/components/frontend/node_modules/typescript/lib   |
      And Varde is not asking whether to restart

    Scenario: A project declares a fact of its own, and the edge's answer reaches the spawn
      Given the project config is:
        """
        [facts.python_env]
        marker = ".venv/bin/python"

        [lsp.python]
        command = "pyright-langserver"
        args = ["--stdio", "--pythonpath=${python_env}"]
        """
      And Varde started in the project
      And the edge resolved "python_env" to "/home/me/project/services/api/.venv/bin/python"
      When I open "src/app.py"
      Then the language server for "python" was started with arguments:
        | arg                                                    |
        | --stdio                                                |
        | --pythonpath=/home/me/project/services/api/.venv/bin/python |

    Scenario: A fact a project declares and the edge could not find stops that spawn too
      Given the project config is:
        """
        [facts.python_env]
        marker = ".venv/bin/python"

        [lsp.python]
        command = "pyright-langserver"
        args = ["--stdio", "--pythonpath=${python_env}"]
        """
      And Varde started in the project
      And the edge resolved no "python_env"
      When I open "src/app.py"
      Then no language server was started

    Scenario: An edge-resolved name reaches an initialization option too
      Given the project config is:
        """
        [lsp.typescript]
        command = "typescript-language-server"
        initialization_options = { tsserver = { path = "${typescript_sdk}" } }
        """
      And Varde started in the project
      And the edge resolved "typescript_sdk" to "/home/me/project/node_modules/typescript/lib"
      When I open "src/app.ts"
      Then the initialize request for "typescript" carried initialization options:
        """
        {"tsserver": {"path": "/home/me/project/node_modules/typescript/lib"}}
        """

    Scenario: A requirement named in an initialization option stops the spawn as an argument does
      Given the project config is:
        """
        [lsp.typescript]
        command = "typescript-language-server"
        initialization_options = { tsserver = { path = "${typescript_sdk}" }, maxTsServerMemory = 3072 }
        """
      And Varde started in the project
      And the edge resolved no "typescript_sdk"
      When I open "src/app.ts"
      Then no language server was started

    Scenario: A fact declared optional starts the server without it rather than not at all
      Given the project config is:
        """
        [facts.zig_plugin]
        marker = "node_modules/zig-plugin"
        optional = true

        [lsp.zig]
        command = "zls"
        initialization_options = { plugin = "${zig_plugin}", memory = 3072 }
        """
      And Varde started in the project
      And the edge resolved no "zig_plugin"
      When I open "src/main.zig"
      Then a language server was started with "zls"
      And the initialize request for "zig" carried initialization options:
        """
        {"memory": 3072}
        """

    Scenario: An optional fact that was found is filled in like any other
      Given the project config is:
        """
        [facts.zig_plugin]
        marker = "node_modules/zig-plugin"
        optional = true

        [lsp.zig]
        command = "zls"
        initialization_options = { plugin = "${zig_plugin}", memory = 3072 }
        """
      And Varde started in the project
      And the edge resolved "zig_plugin" to "/home/me/project/node_modules/zig-plugin"
      When I open "src/main.zig"
      Then the initialize request for "zig" carried initialization options:
        """
        {"memory": 3072, "plugin": "/home/me/project/node_modules/zig-plugin"}
        """

    Scenario: An optional fact named in an argument drops the argument and keeps the server
      Given the project config is:
        """
        [facts.zig_plugin]
        marker = "node_modules/zig-plugin"
        optional = true

        [lsp.zig]
        command = "zls"
        args = ["--stdio", "--plugin=${zig_plugin}"]
        """
      And Varde started in the project
      And the edge resolved no "zig_plugin"
      When I open "src/main.zig"
      Then the language server for "zig" was started with arguments:
        | arg     |
        | --stdio |

    Scenario: A row whose optional fact went unfound is missing no requirement
      Given the project config is:
        """
        [facts.zig_plugin]
        marker = "node_modules/zig-plugin"
        optional = true

        [lsp.zig]
        command = "zls"
        initialization_options = { plugin = "${zig_plugin}" }
        """
      And Varde started in the project
      And the command "zls" is on PATH
      And the edge resolved no "zig_plugin"
      When I open Tools
      Then the row for "zig" is "installed"

    Scenario: A fact that says nothing about it is still a requirement
      Given the project config is:
        """
        [facts.zig_plugin]
        marker = "node_modules/zig-plugin"

        [lsp.zig]
        command = "zls"
        initialization_options = { plugin = "${zig_plugin}" }
        """
      And Varde started in the project
      And the command "zls" is on PATH
      And the edge resolved no "zig_plugin"
      When I open "src/main.zig"
      Then no language server was started
      And the row for "zig" is "missing-requirement"

    Scenario Outline: A shipped default names the SDK the TypeScript server will not start without
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And the edge resolved "typescript_sdk" to "/home/me/project/node_modules/typescript/lib"
      When I open "src/app.<extension>"
      Then a language server was started with "typescript-language-server"
      And the initialize request for "<language>" carried initialization options:
        """
        {"tsserver": {"path": "/home/me/project/node_modules/typescript/lib/tsserver.js"}}
        """

      Examples:
        | language   | extension |
        | typescript | ts        |
        | javascript | js        |

    Scenario: A workspace with no TypeScript of its own starts no TypeScript server
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And the command "typescript-language-server" is on PATH
      And the edge resolved no "typescript_sdk"
      When I open "src/app.ts"
      Then no language server was started
      And the row for "typescript" is "missing-requirement"

    Scenario: A name the library does not supply is left exactly as it was written
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        args = ["--shell=${HOME}"]
        """
      And Varde started in the project
      When I open "src/lib.rs"
      Then the language server for "rust" was started with arguments:
        | arg              |
        | --shell=${HOME} |

    Scenario: A row installed but missing what it needs is neither installed nor missing
      Given the command "vue-language-server" is on PATH
      And the edge resolved no "typescript_sdk"
      When I open Tools
      Then the row for "vue" is "missing-requirement"

    Scenario: A language configuration says only partly works reads as neither
      Given the project config is:
        """
        [lsp.elm]
        command = "elm-language-server"
        partial = "type errors"
        """
      And the command "elm-language-server" is on PATH
      When I open Tools
      Then the row for "elm" is "partly-working"

    Scenario: What a partly-working language cannot do is named on its row
      Given the project config is:
        """
        [lsp.elm]
        command = "elm-language-server"
        partial = "type errors"
        """
      And the command "elm-language-server" is on PATH
      When I open Tools
      Then the row for "elm" says it cannot do "type errors"

    Scenario: A missing command is a missing command, whatever it would only partly do
      Given the project config is:
        """
        [lsp.elm]
        command = "elm-language-server"
        partial = "type errors"
        install.macos = "npm install -g @elm-tooling/elm-language-server"
        """
      And Varde was built for "macos"
      And the command "npm" is on PATH
      When I open Tools
      Then the row for "elm" is "missing"

    Scenario: The same row with the SDK found reads as installed
      Given the command "vue-language-server" is on PATH
      And the edge resolved "typescript_sdk" to "/home/me/project/node_modules/typescript/lib"
      When I open Tools
      Then the row for "vue" is "installed"

  Rule: A question Varde cannot answer is refused out loud, never met with silence

    A server may ask the *client* for something only another program can answer, and wait. Volar's
    servers do exactly that: `@vue/language-server` asks its client to put a question to a TypeScript
    server on its behalf, and every feature it has — diagnostics included — is behind the first such
    question. Asked and never answered, it waits forever, which reaches the reader as a server that
    started, stayed alive and said nothing: the one thing R31.25 forbids.

    Being that client — running a second server and relaying between the two — is a decision this
    suite does not take, and `.scratch/richer-editor/issues/19-a-server-that-needs-a-companion.md`
    records why. What it does take is the rule `queries::reply` already sets for a child's escape
    sequences and `received` already keeps for a server's *requests*: a question that will not be
    answered is refused, out loud, so the asker fails rather than hangs. The question arrives as a
    notification, so the protocol has no reply for it — which is why the pair of method names is
    configuration, exactly as a command is. No branch names the server that asks, or the method it
    asks on.

    Scenario: A question the server asks on the configured method is answered rather than ignored
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        unanswerable.request = "elsewhere/request"
        unanswerable.response = "elsewhere/response"
        """
      And Varde started in the project
      And a language server for "rust" is ready
      When the language server for "rust" asks "elsewhere/request" with:
        """
        [[7, "somethingOnlyAnotherServerKnows", {"file": "src/lib.rs"}]]
        """
      Then the language server for "rust" was sent "elsewhere/response" with:
        """
        [[7, null]]
        """

    Scenario: The answer carries the tag the question was asked under and nothing else
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        unanswerable.request = "elsewhere/request"
        unanswerable.response = "elsewhere/response"
        """
      And Varde started in the project
      And a language server for "rust" is ready
      When the language server for "rust" asks "elsewhere/request" with:
        """
        [41, "somethingOnlyAnotherServerKnows", {"file": "src/lib.rs"}]
        """
      Then the language server for "rust" was sent "elsewhere/response" with:
        """
        [41, null]
        """

    Scenario: A notification on any other method is not answered
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        unanswerable.request = "elsewhere/request"
        unanswerable.response = "elsewhere/response"
        """
      And Varde started in the project
      And a language server for "rust" is ready
      When the language server for "rust" asks "window/logMessage" with:
        """
        [[7, "somethingOnlyAnotherServerKnows", {}]]
        """
      Then the language server for "rust" was sent no "elsewhere/response" notification

    Scenario: A language that configures no such question answers nothing
      Given the project config is:
        """
        [lsp.rust]
        command = "rust-analyzer"
        """
      And Varde started in the project
      And a language server for "rust" is ready
      When the language server for "rust" asks "elsewhere/request" with:
        """
        [[7, "somethingOnlyAnotherServerKnows", {}]]
        """
      Then the language server for "rust" was sent no "elsewhere/response" notification

    Scenario: A fresh install serves a Vue file with the TypeScript server too
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And the edge resolved "typescript_sdk" to "/home/me/project/node_modules/typescript/lib"
      When I open "src/App.vue"
      Then a language server was started with "vue-language-server"
      And a language server was started with "typescript-language-server"

    Scenario: A fresh install tells the TypeScript server where the Vue plugin is
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And the edge resolved "typescript_sdk" to "/home/me/project/node_modules/typescript/lib"
      And the edge resolved "vue_typescript_plugin" to "/opt/node/lib/node_modules/@vue/typescript-plugin"
      When I open "src/App.vue"
      Then the initialize request for "typescript" carried initialization options:
        """
        {"plugins": [{"languages": ["vue"], "location": "/opt/node/lib/node_modules/@vue/typescript-plugin", "name": "@vue/typescript-plugin"}], "tsserver": {"path": "/home/me/project/node_modules/typescript/lib/tsserver.js"}}
        """

    Scenario: A machine with no Vue server still starts TypeScript for a TypeScript project
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And the edge resolved "typescript_sdk" to "/home/me/project/node_modules/typescript/lib"
      And the edge resolved no "vue_typescript_plugin"
      When I open "src/app.ts"
      Then a language server was started with "typescript-language-server"
      And the initialize request for "typescript" carried initialization options:
        """
        {"tsserver": {"path": "/home/me/project/node_modules/typescript/lib/tsserver.js"}}
        """

    Scenario: A fresh install refuses the question the shipped Vue server asks
      Given there is no global config
      And the project has no config file
      And Varde started in the project
      And a language server for "vue" is ready
      When the language server for "vue" asks "tsserver/request" with:
        """
        [[1, "_vue:projectInfo", {"file": "src/App.vue"}]]
        """
      Then the language server for "vue" was sent "tsserver/response" with:
        """
        [[1, null]]
        """

  Rule: The server lays the whole document out when it is asked to

    `:format` is the one formatting question somebody presses a key for, so it is the one that is
    allowed to speak. The server that declares `documentFormattingProvider` gets it and the Buffer's
    text is sent first — a server asked about text it has not heard answers about a document that no
    longer exists. What comes back is edits, applied as one thing to undo, and dropped when the
    reader has typed on since: the Document version rule, unchanged.

    A server that declares no formatter is not asked at all, and nothing is said here — an external
    Formatter is what answers instead, which is `features/formatting.feature`'s half.

    Scenario: Asking to format asks the server that says it formats
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
                }
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentFormattingProvider":true}}
        """
      When I run ":format" in the editor
      Then the language server for "rust" was sent a "textDocument/formatting" request
      And no formatter was run

    Scenario: The edits the server sends back move the lines it named
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
                }
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentFormattingProvider":true}}
        """
      And I run ":format" in the editor
      When the language server for "rust" answers the document formatting with:
        | line | from | to | text |
        | 2    | 1    | 4  |      |
        | 3    | 1    | 8  |      |
      Then the buffer holds:
        """
        fn main() {
            let x = 1;
        }
        """

    Scenario: A document reformat is one press of undo
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
                }
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentFormattingProvider":true}}
        """
      And I run ":format" in the editor
      And the language server for "rust" answers the document formatting with:
        | line | from | to | text |
        | 2    | 1    | 4  |      |
        | 3    | 1    | 8  |      |
      When I press "u" in the editor
      Then the buffer holds:
        """
        fn main() {
                let x = 1;
                }
        """

    Scenario: A reply that arrives after the reader has typed on is dropped
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
                }
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentFormattingProvider":true}}
        """
      And I run ":format" in the editor
      And the editor mode is insert
      And I type "z" in the editor
      When the language server for "rust" answers the document formatting with:
        | line | from | to | text |
        | 2    | 1    | 4  |      |
        | 3    | 1    | 8  |      |
      Then the buffer holds:
        """
        zfn main() {
                let x = 1;
                }
        """

    Scenario: A server with nothing to change says so, because a key was pressed
      Given a language server "rust-analyzer" is configured for "rust"
      And "src/lib.rs" holds:
        """
        fn main() {}
        """
      And I open "src/lib.rs"
      And the language server for "rust" replies to "initialize" with:
        """
        {"capabilities":{"documentFormattingProvider":true}}
        """
      And I run ":format" in the editor
      When the language server for "rust" answers the document formatting with nothing
      Then the notice is "nothing-to-format"
      And the buffer is unchanged

    Scenario: A server that declares no formatter is not asked
      Given a language server for "rust" is ready
      And "src/lib.rs" holds:
        """
        fn main() {
                let x = 1;
        }
        """
      And I open "src/lib.rs"
      When I run ":format" in the editor
      Then the language server for "rust" was sent no "textDocument/formatting" request
