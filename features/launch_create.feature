Feature: Creating a Launch configuration

  The Launch box lists what the config files name and starts the chosen one. `c`, or its Chip, opens
  a form that writes a new one, because a Launch configuration was otherwise only reachable by
  hand-editing `config.toml` with nothing to say which arguments an adapter takes.

  Which arguments it takes is the `[dap.*]` row's own knowledge, named there as `launch_args` and
  `attach_args` with a one-line explanation each, never Varde's
  (`docs/adr/0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md`). A row that names no
  list offers free key/value fields, which is what an adapter nobody here has tried gets.

  Background:
    Given the workspace root is "/home/me/projects/varde"

  Rule: The form writes a [launch.<name>] row into the layer it names

    Scenario: Creating a configuration writes it into the project config and lists it selected
      Given the project has no config file
      And a Debug adapter for "rust" is configured
      When I create a Launch configuration with:
        | name    | checkout             |
        | adapter | rust                 |
        | request | launch               |
        | program | target/debug/orders  |
      Then the project config names the launch "checkout"
      And the project launch "checkout" uses the adapter "rust" with the request "launch"
      And the project launch "checkout" sets the argument "program" to "target/debug/orders"
      And the launch palette offers "checkout"
      And the Launch box has "checkout" selected
      And no Debug adapter was asked for

    Scenario: Switching the target to global writes to the global config instead
      Given the project config is:
        """
        [editor]
        tab_width = 2
        """
      And a Debug adapter for "rust" is configured
      When I create a Launch configuration in the global layer with:
        | name    | checkout             |
        | adapter | rust                 |
        | request | launch               |
        | program | target/debug/orders  |
      Then the global config names the row "launch.checkout"
      And the project ".varde/config.toml" is unchanged

    Scenario: An argument whose text reads as a number is written as one
      Given the project has no config file
      And a Debug adapter for "java" is configured
      When I create a Launch configuration with:
        | name     | orders    |
        | adapter  | java      |
        | request  | attach    |
        | hostName | localhost |
        | port     | 5005      |
      Then the project launch "orders" sets the argument "port" to the number "5005"
      And the project launch "orders" sets the argument "hostName" to "localhost"

  Rule: The fields are the ones the chosen row lists for the chosen request

    Scenario: The form offers the chosen row's launch arguments with their explanations
      Given the project has no config file
      And a Debug adapter for "rust" is configured
      When I open the Launch form for the adapter "rust"
      Then the Launch form offers the field "program"
      And the Launch form explains the field "program" as "Path to the built executable to run"
      And the Launch form names "program" as required
      And the Launch form offers no field "pid"

    Scenario: The fields are the chosen request's, not the row's whole list
      Given the project has no config file
      And a Debug adapter for "java" is configured
      When I open the Launch form for the adapter "java"
      Then the Launch form offers the field "mainClass"
      And the Launch form offers no field "hostName"

    Scenario: Switching the request switches the fields
      Given the project has no config file
      And a Debug adapter for "java" is configured
      And I open the Launch form for the adapter "java"
      When I switch the Launch form's request
      Then the Launch form offers the field "hostName"
      And the Launch form offers no field "mainClass"

    Scenario: A row with no argument list offers free key/value fields
      Given the project has no config file
      And a Debug adapter for "ada" is configured
      When I open the Launch form for the adapter "ada"
      Then the Launch form offers a free key and value field
      And the Launch form offers no field "program"

    Scenario: A free key/value pair is written as an argument
      Given the project has no config file
      And a Debug adapter for "ada" is configured
      When I create a Launch configuration with:
        | name     | orders  |
        | adapter  | ada     |
        | request  | launch  |
        | key 1    | mainUnit |
        | value 1  | orders   |
      Then the project launch "orders" sets the argument "mainUnit" to "orders"

  Rule: Create and the form's actions are Chips as well as keys

    Scenario: The Launch box's create Chip opens the form
      Given the project has no config file
      And a Debug adapter for "rust" is configured
      And I open the launch palette
      When I click the Launch box's "create" Chip
      Then the Launch form offers the field "program"

    Scenario: The form's create Chip writes the configuration Enter would
      Given the project has no config file
      And a Debug adapter for "rust" is configured
      And I filled in a Launch configuration with:
        | name    | checkout            |
        | adapter | rust                |
        | request | launch              |
        | program | target/debug/orders |
      When I click the Launch box's "create" Chip
      Then the project config names the launch "checkout"
      And the Launch box has "checkout" selected

  Rule: Every refusal names the problem and writes nothing

    Scenario: A name already in the target file is refused, and the file is unchanged
      Given the project config is:
        """
        [launch.checkout]
        adapter = "rust"
        request = "launch"
        args = { program = "target/debug/mine" }
        """
      And a Debug adapter for "rust" is configured
      When I create a Launch configuration with:
        | name    | checkout             |
        | adapter | rust                 |
        | request | launch               |
        | program | target/debug/orders  |
      Then the editor refuses with "launch-name-taken"
      And the project ".varde/config.toml" is unchanged

    Scenario: An empty required argument is refused, and the field is named
      Given the project config is:
        """
        [editor]
        tab_width = 2
        """
      And a Debug adapter for "rust" is configured
      When I create a Launch configuration with:
        | name    | checkout |
        | adapter | rust     |
        | request | launch   |
      Then the editor refuses with "launch-field-needed"
      And the refusal names "program"
      And the project ".varde/config.toml" is unchanged

    Scenario: An unnamed configuration is refused, and the file is unchanged
      Given the project config is:
        """
        [editor]
        tab_width = 2
        """
      And a Debug adapter for "rust" is configured
      When I create a Launch configuration with:
        | adapter | rust                |
        | request | launch              |
        | program | target/debug/orders |
      Then the editor refuses with "launch-field-needed"
      And the refusal names "name"
      And the project ".varde/config.toml" is unchanged

    Scenario: A target file that no longer parses is refused, and nothing is written
      Given the project config is:
        """
        [launch.checkout]
        adapter = "rust"
        request = "launch"
        args = {}
        """
      And a Debug adapter for "rust" is configured
      And the project config has since been edited to:
        """
        [launch.checkout
        adapter = "rust"
        """
      When I create a Launch configuration with:
        | name    | orders              |
        | adapter | rust                |
        | request | launch              |
        | program | target/debug/orders |
      Then the editor refuses with "broken-config"
      And the project ".varde/config.toml" is unchanged

    Scenario: A target file that cannot be read is refused, naming the layer
      Given the project config is:
        """
        [editor]
        tab_width = 2
        """
      And a Debug adapter for "rust" is configured
      And the config files can no longer be read
      When I create a Launch configuration with:
        | name    | orders              |
        | adapter | rust                |
        | request | launch              |
        | program | target/debug/orders |
      Then the editor refuses with "broken-config"
      And the refusal names the layer ".varde/config.toml"
      And the project ".varde/config.toml" is unchanged

    Scenario: A global target that cannot be read names the global layer
      Given the project config is:
        """
        [editor]
        tab_width = 2
        """
      And a Debug adapter for "rust" is configured
      And the config files can no longer be read
      When I create a Launch configuration in the global layer with:
        | name    | orders              |
        | adapter | rust                |
        | request | launch              |
        | program | target/debug/orders |
      Then the editor refuses with "broken-config"
      And the refusal names the layer "~/.varde/config.toml"

  Rule: A Bare workspace has no project layer, so the target is global only

    Scenario: In a Bare workspace the form offers no target field
      Given the workspace folder holds the file "README.md"
      And Varde started with no folder in "/home/me/projects/theirs"
      And a Debug adapter for "rust" is configured
      When I open the Launch form for the adapter "rust"
      Then the Launch form offers no target field
      And the Launch form's target is "global"
