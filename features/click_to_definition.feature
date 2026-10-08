Feature: A held modifier makes a name a link to its definition

  Go-to-definition is `gd`, and the same jump is a click: holding the jump modifier over a
  name in the editor underlines it, and clicking it asks the language server where that name
  is defined. The underline and a tint mark the link, the pointer becomes a hand on a terminal
  that can change it, and the key stays, so the jump is reachable with no modifier at all.

  The modifier is Cmd, and Ctrl is the same gesture: the mouse protocol has bits for shift,
  alt and ctrl and none for Cmd, so on most terminals a Cmd+click arrives with no modifier
  at all.

  Background:
    Given the workspace root is "/home/me/projects/varde"
    And the current view is Edit
    And a language server for "rust" is ready
    And "src/lib.rs" is open in the editor holding:
      """
      fn main() {
          greet(name);
      }
      """

  Scenario: Holding the jump modifier over a name underlines it
    When I hold the jump modifier over line 2 column 7 in the editor
    Then the editor underlines line 2 columns 5 to 9

  Scenario: Holding it over blank space underlines nothing
    When I hold the jump modifier over line 2 column 2 in the editor
    Then the editor underlines nothing

  Scenario: Pointing at a name without the modifier is not a link
    Given I hold the jump modifier over line 2 column 7 in the editor
    When I point at line 2 column 7 in the editor
    Then the editor underlines nothing

  Scenario: Pointing away from the name takes the underline with it
    Given I hold the jump modifier over line 2 column 7 in the editor
    When I hold the jump modifier over line 1 column 3 in the editor
    Then the editor underlines nothing

  Scenario: Clicking a link asks the server where that name is defined
    When I click at line 2 column 7 in the editor with the jump modifier held
    Then the cursor is at line 2 column 7
    And the language server for "rust" was sent a "textDocument/definition" request for "src/lib.rs" line 2 column 7

  Scenario: A plain click places the cursor and asks nothing
    When I click at line 2 column 7 in the editor
    Then the cursor is at line 2 column 7
    And the language server for "rust" was never sent a "textDocument/definition" request
