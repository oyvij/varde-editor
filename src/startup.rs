use crate::risk::{self, Scope};
use crate::{Effect, ReplaceFailed, State, View};
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use toml::Table;

pub const DEFAULTS: &str = r#"[view]
double_tap_ms = 300

# What Tab lays down while inserting. Named here rather than measured off the
# file: a project's own answer beats a scan of whichever lines happen to be
# open, and four is the width to disagree with in one line of TOML.
[editor]
tab_width = 4

# Whether the mirror of the file down the editor's right-hand edge is up in a
# project nobody has turned it off in. `:minimap` is the same switch from
# inside, and what it was left at beats this.
minimap = true

[ai]
env = ["HOME", "PATH", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE"]

[risk]
threshold = 15
max_iterations = 10

# `speed` is a multiplier where higher is faster, which is the convention every
# player has. The synthesizer's own parameter scales *duration* and runs
# backwards, so `${scale}` is its reciprocal and the inversion never reaches a
# file a human writes (R35.7).
[speech]
speed = 1.0

# What a Run mark stands beside and starts (F41): the files a row looks in, a
# tree-sitter query whose `@run` capture is the line it marks, a command for a
# shell whose prompt is waiting, and a Launch configuration for Debug. Each
# `${…}` is filled from the query capture of that name, and `${file}` with the
# file's path — quoted in `run`, since both are text from the folder.
#
# A row per kind of thing to start, because each runs its own way. Debugging a
# Rust binary starts at its path, which cargo names only as it builds, so LLDB
# builds it and reads the path out of cargo's own report.
[run.rust_main]
extensions = ["rs"]
query = '((function_item name: (identifier) @name @run) (#eq? @name "main"))'
run = "cargo run"
debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["script import json, subprocess; lldb.debugger.CreateTarget(next(m['executable'] for m in map(json.loads, subprocess.run(['cargo', 'build', '--message-format=json'], stdout=subprocess.PIPE, text=True).stdout.splitlines()) if m.get('executable')))"] } }

# The first test binary cargo builds, which is the crate's own tests; a test
# under `tests/` is in a binary of its own.
[run.rust_test]
extensions = ["rs"]
query = '((attribute_item (attribute (identifier) @attribute)) . (function_item name: (identifier) @name @run) (#eq? @attribute "test"))'
run = "cargo test ${name} -- --exact"
debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["script import json, subprocess; lldb.debugger.CreateTarget(next(m['executable'] for m in map(json.loads, subprocess.run(['cargo', 'test', '--no-run', '--message-format=json'], stdout=subprocess.PIPE, text=True).stdout.splitlines()) if m.get('executable') and m['profile']['test']))"], args = ["${name}", "--exact"] } }

# The same test inside a module, which `--exact` names by its path. Rows are
# tried by name, so this one takes the line before `rust_test` can.
[run.rust_module_test]
extensions = ["rs"]
query = '((mod_item name: (identifier) @module body: (declaration_list (attribute_item (attribute (identifier) @attribute)) . (function_item name: (identifier) @name @run))) (#eq? @attribute "test"))'
run = "cargo test ${module}::${name} -- --exact"
debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["script import json, subprocess; lldb.debugger.CreateTarget(next(m['executable'] for m in map(json.loads, subprocess.run(['cargo', 'test', '--no-run', '--message-format=json'], stdout=subprocess.PIPE, text=True).stdout.splitlines()) if m.get('executable') and m['profile']['test']))"], args = ["${module}::${name}", "--exact"] } }

# The source-file launcher, which compiles the file it is handed: no build to
# ask for a class path.
[run.java_main]
extensions = ["java"]
query = '((method_declaration name: (identifier) @name @run) (#eq? @name "main"))'
run = "java ${file}"
debug = { adapter = "java", request = "launch", args = { mainClass = "${file}" } }

# No `debug`: a test is started by its runner, which is on the class path the
# project's build knows and Varde does not.
[run.java_test]
extensions = ["java"]
query = '(class_declaration name: (identifier) @class body: (class_body (method_declaration (modifiers (marker_annotation name: (identifier) @annotation)) name: (identifier) @name @run) (#eq? @annotation "Test")))'
run = "mvn test -Dtest=${class}#${name}"

[run.javascript_test]
extensions = ["js", "jsx", "mjs", "cjs"]
query = '((call_expression function: (identifier) @function arguments: (arguments . (string (string_fragment) @name))) @run (#match? @function "^(test|it)$"))'
run = "npx vitest run ${file} -t ${name}"
debug = { adapter = "javascript", request = "launch", args = { type = "pwa-node", runtimeExecutable = "npx", runtimeArgs = ["vitest", "run", "${file}", "-t", "${name}"] } }

[run.typescript_test]
extensions = ["ts", "tsx", "mts", "cts"]
query = '((call_expression function: (identifier) @function arguments: (arguments . (string (string_fragment) @name))) @run (#match? @function "^(test|it)$"))'
run = "npx vitest run ${file} -t ${name}"
debug = { adapter = "typescript", request = "launch", args = { type = "pwa-node", runtimeExecutable = "npx", runtimeArgs = ["vitest", "run", "${file}", "-t", "${name}"] } }
"#;

pub const PROGRAMS: &str = r#"# The compiler a project pins, which is a per-package dependency in every
# JavaScript workspace and the file `@vue/language-server` resolves out of the
# directory `--tsdk=` names. `value = "directory"` because the server wants the
# `lib` holding it, not the file. The global install is where `tsc` points once
# symlinks are resolved, and it is checked for the same file rather than
# assumed: from 7 the package is the native compiler, on PATH as `tsc`, with no
# `typescript.js` at all — which is why the install is pinned to 6.
[facts.typescript_sdk]
marker = "node_modules/typescript/lib/typescript.js"
value = "directory"
command = "tsc"
command_marker = "../lib/typescript.js"
install.macos = "npm install -g typescript@6"
install.linux = "npm install -g typescript@6"
install.windows = "npm install -g typescript@6"

# What teaches a TypeScript server to answer about a `.vue` file. It needs no
# install of its own: `@vue/language-server` carries it in its own
# `node_modules`, which is exactly what the command fallback reaches once
# symlinks are resolved — so the Vue row's install brings the companion with it
# and a workspace that installed its own copy still wins, because the marker
# walk runs first.
#
# `optional` because it is named on the `[lsp.typescript]` row every TypeScript
# project shares: required, a machine with no Vue server would have no
# TypeScript server anywhere, which is a requirement nobody declared.
[facts.vue_typescript_plugin]
marker = "node_modules/@vue/typescript-plugin"
optional = true
command = "vue-language-server"
command_marker = "../node_modules/@vue/typescript-plugin"

# The plugin `[dap.java]` loads into jdtls: java-debug is no program of its own.
# Maven Central publishes the bundle, so the install fetches its newest release
# into `~/.varde/java-debug`, with a script beside it that prints where it is
# and a link to that on `PATH` — the command fallback reads the marker off the
# script's real directory, which is the only way a fact reaches a file outside
# the workspace. `optional` because jdtls starts without it: a Java server no
# Debug session can be started in is still a Java server.
[facts.java_debug_plugin]
marker = "com.microsoft.java.debug.plugin.jar"
optional = true
command = "java-debug-plugin"
command_marker = "com.microsoft.java.debug.plugin.jar"
install.macos = "curl -sfL https://repo1.maven.org/maven2/com/microsoft/java/com.microsoft.java.debug.plugin/maven-metadata.xml | sed -n 's:.*<release>::; s:</release>.*::p' | { read v && curl -sfL --create-dirs https://repo1.maven.org/maven2/com/microsoft/java/com.microsoft.java.debug.plugin/$v/com.microsoft.java.debug.plugin-$v.jar -o ~/.varde/java-debug/com.microsoft.java.debug.plugin.jar; } && mkdir -p ~/.local/bin && printf '#!/bin/sh\\necho %s\\n' ~/.varde/java-debug/com.microsoft.java.debug.plugin.jar > ~/.varde/java-debug/java-debug-plugin && chmod +x ~/.varde/java-debug/java-debug-plugin && ln -sf ~/.varde/java-debug/java-debug-plugin ~/.local/bin/java-debug-plugin"
install.linux = "curl -sfL https://repo1.maven.org/maven2/com/microsoft/java/com.microsoft.java.debug.plugin/maven-metadata.xml | sed -n 's:.*<release>::; s:</release>.*::p' | { read v && curl -sfL --create-dirs https://repo1.maven.org/maven2/com/microsoft/java/com.microsoft.java.debug.plugin/$v/com.microsoft.java.debug.plugin-$v.jar -o ~/.varde/java-debug/com.microsoft.java.debug.plugin.jar; } && mkdir -p ~/.local/bin && printf '#!/bin/sh\\necho %s\\n' ~/.varde/java-debug/com.microsoft.java.debug.plugin.jar > ~/.varde/java-debug/java-debug-plugin && chmod +x ~/.varde/java-debug/java-debug-plugin && ln -sf ~/.varde/java-debug/java-debug-plugin ~/.local/bin/java-debug-plugin"

[lsp.rust]
command = "rust-analyzer"
extensions = ["rs"]
install.macos = "rustup component add rust-analyzer"
install.linux = "rustup component add rust-analyzer"
install.windows = "rustup component add rust-analyzer"

# Every feature this server has is behind a question it puts to its client,
# expecting the client to be running a TypeScript server as well and to relay it
# there. Varde does not relay — the question is refused, which is what gets the
# server past it and answering with what it can answer on its own instead of
# waiting forever. What answers the rest is a *second server on the same file*:
# `also_served_by` puts every question about a `.vue` file to the TypeScript
# server too, and the plugin named on that server's row is what lets it answer.
# Two clients on one buffer, which is what every other editor does here, and no
# arm anywhere names either server.
[lsp.vue]
command = "vue-language-server"
args = ["--stdio", "--tsdk=${typescript_sdk}"]
extensions = ["vue"]
also_served_by = ["typescript"]
unanswerable.request = "tsserver/request"
unanswerable.response = "tsserver/response"
install.macos = "npm install -g @vue/language-server"
install.linux = "npm install -g @vue/language-server"
install.windows = "npm install -g @vue/language-server"

[lsp.java]
command = "jdtls"
extensions = ["java"]
install.macos = "brew install jdtls"

[lsp.zig]
command = "zls"
extensions = ["zig"]
install.macos = "brew install zls"

# The server resolves TypeScript itself, and on a machine whose global
# `typescript` is the 7.0 native preview it finds a package with no
# `tsserver.js` in it and never answers `initialize` at all. Named the file
# beside the `typescript.js` the fact already looks for, so the join is a
# string one in TOML rather than a second fact. Forward slashes: Node accepts
# them on Windows too, so a Windows answer joined this way still resolves.
[lsp.typescript]
command = "typescript-language-server"
args = ["--stdio"]
extensions = ["ts", "tsx", "mts", "cts"]
language_ids = { tsx = "typescriptreact" }
install.macos = "npm install -g typescript@6 typescript-language-server"
install.linux = "npm install -g typescript@6 typescript-language-server"
install.windows = "npm install -g typescript@6 typescript-language-server"

# Sub-tables rather than one inline table, which TOML would want on a single
# line, and this one is a paragraph long. The plugin is what makes this server
# answer about the `.vue` files `[lsp.vue].also_served_by` sends it, and it goes
# nowhere on a machine that has no Vue server: the fact is optional, so the key
# is dropped and the server starts exactly as it does in a project with no Vue
# in it.
[lsp.typescript.initialization_options.tsserver]
path = "${typescript_sdk}/tsserver.js"

[[lsp.typescript.initialization_options.plugins]]
name = "@vue/typescript-plugin"
location = "${vue_typescript_plugin}"
languages = ["vue"]

# The same server and the same SDK, spelled the same way as the row above so
# that the one real difference between them is the one a reader can see: a
# `.js` file is served by nobody else, so there is no plugin entry here.
[lsp.javascript]
command = "typescript-language-server"
args = ["--stdio"]
extensions = ["js", "jsx", "mjs", "cjs"]
language_ids = { jsx = "javascriptreact" }
install.macos = "npm install -g typescript@6 typescript-language-server"
install.linux = "npm install -g typescript@6 typescript-language-server"
install.windows = "npm install -g typescript@6 typescript-language-server"

[lsp.javascript.initialization_options.tsserver]
path = "${typescript_sdk}/tsserver.js"

[lsp.python]
command = "pyright-langserver"
args = ["--stdio"]
extensions = ["py", "pyi"]
install.macos = "npm install -g pyright"
install.linux = "npm install -g pyright"
install.windows = "npm install -g pyright"

[lsp.go]
command = "gopls"
extensions = ["go"]
install.macos = "go install golang.org/x/tools/gopls@latest"
install.linux = "go install golang.org/x/tools/gopls@latest"
install.windows = "go install golang.org/x/tools/gopls@latest"

# No macOS command: Homebrew's `llvm` is keg-only, so a successful install
# leaves `clangd` unreachable by name and the row still reading `missing`. A
# command that cannot make its own row read `installed` is the invention this
# table refuses.
[lsp.c]
command = "clangd"
extensions = ["c", "h"]
install.linux = "sudo apt install clangd"
install.windows = "winget install LLVM.LLVM"

[lsp.cpp]
command = "clangd"
extensions = ["cpp", "cc", "cxx", "hpp", "hh", "hxx"]
install.linux = "sudo apt install clangd"
install.windows = "winget install LLVM.LLVM"

# The long tail (ADR 0018): every language below has a server one command
# installs somewhere, and an OS nothing packages it for has no key. A language
# missing from this list is a row you write, exactly like these. PowerShell is
# missing on purpose: its server ships only as a release bundle, and starting it
# needs the path that bundle was unpacked to.

[lsp.shellscript]
command = "bash-language-server"
args = ["start"]
extensions = ["sh", "bash"]
install.macos = "npm install -g bash-language-server"
install.linux = "npm install -g bash-language-server"
install.windows = "npm install -g bash-language-server"

[lsp.lua]
command = "lua-language-server"
extensions = ["lua"]
install.macos = "brew install lua-language-server"
install.windows = "winget install LuaLS.lua-language-server"

[lsp.ruby]
command = "ruby-lsp"
extensions = ["rb", "rake", "gemspec", "ru"]
install.macos = "gem install ruby-lsp"
install.linux = "gem install ruby-lsp"
install.windows = "gem install ruby-lsp"

[lsp.php]
command = "intelephense"
args = ["--stdio"]
extensions = ["php"]
install.macos = "npm install -g intelephense"
install.linux = "npm install -g intelephense"
install.windows = "npm install -g intelephense"

[lsp.kotlin]
command = "kotlin-language-server"
extensions = ["kt", "kts"]
install.macos = "brew install kotlin-language-server"

# No install key: the server ships with the Swift toolchain, for the reason
# `[formatter.go]` has none.
[lsp.swift]
command = "sourcekit-lsp"
extensions = ["swift"]

[lsp.csharp]
command = "csharp-ls"
extensions = ["cs"]
install.macos = "dotnet tool install --global csharp-ls"
install.linux = "dotnet tool install --global csharp-ls"
install.windows = "dotnet tool install --global csharp-ls"

[lsp.haskell]
command = "haskell-language-server-wrapper"
args = ["--lsp"]
extensions = ["hs", "lhs"]
install.macos = "ghcup install hls"
install.linux = "ghcup install hls"

# No install key: `opam install ocaml-lsp-server` puts the server in the
# switch's own `bin`, which is on PATH only once the reader's shell has run
# `opam env`, so it would leave this row reading `missing` — `[lsp.c]`'s
# macOS reason.
[lsp.ocaml]
command = "ocamllsp"
extensions = ["ml", "mli"]

[lsp.elixir]
command = "elixir-ls"
extensions = ["ex", "exs"]
install.macos = "brew install elixir-ls"

[lsp.erlang]
command = "elp"
args = ["server"]
extensions = ["erl", "hrl"]
install.macos = "brew install erlang-language-platform"

[lsp.scala]
command = "metals"
extensions = ["scala", "sc", "sbt"]
install.macos = "cs install metals"
install.linux = "cs install metals"
install.windows = "cs install metals"

[lsp.clojure]
command = "clojure-lsp"
extensions = ["clj", "cljs", "cljc", "edn"]
install.macos = "brew install clojure-lsp"

# The server is the SDK's own subcommand, so there is nothing to install apart
# from the language.
[lsp.dart]
command = "dart"
args = ["language-server"]
extensions = ["dart"]

# The server is a package inside the language, so the probe finds `julia`, not
# the package: with Julia installed this row reads `installed` before its
# install has run, and only once a `.jl` file has tried the server and it has
# read `stopped` is the install offered. `[lsp.r]` is the same.
[lsp.julia]
command = "julia"
args = ["--startup-file=no", "--history-file=no", "-e", "using LanguageServer; runserver()"]
extensions = ["jl"]
install.macos = "julia -e 'using Pkg; Pkg.add(\"LanguageServer\")'"
install.linux = "julia -e 'using Pkg; Pkg.add(\"LanguageServer\")'"
install.windows = "julia -e 'using Pkg; Pkg.add(\"LanguageServer\")'"

[lsp.r]
command = "R"
args = ["--no-echo", "-e", "languageserver::run()"]
extensions = ["r", "R"]
install.macos = "R -e 'install.packages(\"languageserver\", repos = \"https://cloud.r-project.org\")'"
install.linux = "R -e 'install.packages(\"languageserver\", repos = \"https://cloud.r-project.org\")'"

[lsp.nix]
command = "nil"
extensions = ["nix"]
install.macos = "nix --extra-experimental-features 'nix-command flakes' profile install nixpkgs#nil"
install.linux = "nix --extra-experimental-features 'nix-command flakes' profile install nixpkgs#nil"

[lsp.terraform]
command = "terraform-ls"
args = ["serve"]
extensions = ["tf", "tfvars"]
install.macos = "brew install hashicorp/tap/terraform-ls"

# A file named `Dockerfile` has no extension to claim, so only the
# `name.dockerfile` spelling reaches this server.
[lsp.dockerfile]
command = "docker-langserver"
args = ["--stdio"]
extensions = ["dockerfile"]
install.macos = "npm install -g dockerfile-language-server-nodejs"
install.linux = "npm install -g dockerfile-language-server-nodejs"
install.windows = "npm install -g dockerfile-language-server-nodejs"

[lsp.toml]
command = "taplo"
args = ["lsp", "stdio"]
extensions = ["toml"]
install.macos = "cargo install --features lsp --locked taplo-cli"
install.linux = "cargo install --features lsp --locked taplo-cli"
install.windows = "cargo install --features lsp --locked taplo-cli"

[lsp.sql]
command = "sqls"
extensions = ["sql"]
install.macos = "go install github.com/sqls-server/sqls@latest"
install.linux = "go install github.com/sqls-server/sqls@latest"
install.windows = "go install github.com/sqls-server/sqls@latest"

[lsp.svelte]
command = "svelteserver"
args = ["--stdio"]
extensions = ["svelte"]
install.macos = "npm install -g svelte-language-server"
install.linux = "npm install -g svelte-language-server"
install.windows = "npm install -g svelte-language-server"

# Like the Vue server, this one does not start without the TypeScript SDK
# named, and it is the same fact that names it.
[lsp.astro]
command = "astro-ls"
args = ["--stdio"]
extensions = ["astro"]
install.macos = "npm install -g @astrojs/language-server"
install.linux = "npm install -g @astrojs/language-server"
install.windows = "npm install -g @astrojs/language-server"

[lsp.astro.initialization_options.typescript]
tsdk = "${typescript_sdk}"

[lsp.graphql]
command = "graphql-lsp"
args = ["server", "-m", "stream"]
extensions = ["graphql", "gql"]
install.macos = "npm install -g graphql-language-service-cli"
install.linux = "npm install -g graphql-language-service-cli"
install.windows = "npm install -g graphql-language-service-cli"

[lsp.proto]
command = "protols"
extensions = ["proto"]
install.macos = "cargo install protols"
install.linux = "cargo install protols"
install.windows = "cargo install protols"

# `CMakeLists.txt` ends in `.txt`, which is every text file's, so only the
# `.cmake` files reach this server.
[lsp.cmake]
command = "cmake-language-server"
extensions = ["cmake"]
install.macos = "pipx install cmake-language-server"
install.linux = "pipx install cmake-language-server"
install.windows = "pip install cmake-language-server"

[lsp.latex]
command = "texlab"
extensions = ["tex"]
install.macos = "brew install texlab"

[lsp.elm]
command = "elm-language-server"
extensions = ["elm"]
install.macos = "npm install -g @elm-tooling/elm-language-server"
install.linux = "npm install -g @elm-tooling/elm-language-server"
install.windows = "npm install -g @elm-tooling/elm-language-server"

[lsp.gleam]
command = "gleam"
args = ["lsp"]
extensions = ["gleam"]
install.macos = "brew install gleam"
install.windows = "winget install Gleam.Gleam"

[lsp.nim]
command = "nimlangserver"
extensions = ["nim", "nims"]
install.macos = "nimble install nimlangserver"
install.linux = "nimble install nimlangserver"
install.windows = "nimble install nimlangserver"

[lsp.perl]
command = "perlnavigator"
args = ["--stdio"]
extensions = ["pl", "pm"]
install.macos = "npm install -g perlnavigator-server"
install.linux = "npm install -g perlnavigator-server"
install.windows = "npm install -g perlnavigator-server"

[lsp.typst]
command = "tinymist"
extensions = ["typ"]
install.macos = "brew install tinymist"

[lsp.markdown]
command = "marksman"
args = ["server"]
extensions = ["md", "markdown"]
install.macos = "brew install marksman"
install.windows = "winget install Artempyanykh.Marksman"

[lsp.yaml]
command = "yaml-language-server"
args = ["--stdio"]
extensions = ["yaml", "yml"]
install.macos = "npm install -g yaml-language-server"
install.linux = "npm install -g yaml-language-server"
install.windows = "npm install -g yaml-language-server"

[lsp.json]
command = "vscode-json-language-server"
args = ["--stdio"]
extensions = ["json", "jsonc"]
install.macos = "npm install -g vscode-langservers-extracted"
install.linux = "npm install -g vscode-langservers-extracted"
install.windows = "npm install -g vscode-langservers-extracted"

[lsp.html]
command = "vscode-html-language-server"
args = ["--stdio"]
extensions = ["html", "htm"]
install.macos = "npm install -g vscode-langservers-extracted"
install.linux = "npm install -g vscode-langservers-extracted"
install.windows = "npm install -g vscode-langservers-extracted"

[lsp.css]
command = "vscode-css-language-server"
args = ["--stdio"]
extensions = ["css", "scss", "less"]
install.macos = "npm install -g vscode-langservers-extracted"
install.linux = "npm install -g vscode-langservers-extracted"
install.windows = "npm install -g vscode-langservers-extracted"

# The `[formatter.*]` tables, which are the `[lsp.*]` tables above in a second
# shape and are data for the same three reasons: a name in the bottom layer of
# the merge is beaten by a file, printable as a string, and extensible without a
# release, none of which a match arm spelling the same string is (ADR 0011,
# ADR 0012). This is also the whole of "HTML, CSS, JavaScript, JSON and YAML are
# supported" — they are rows here, not code.
#
# Every command named below reads the text on stdin and writes the result on
# stdout, because that is the only shape that can format what nobody has saved:
# a formatter told about the file on disk formats a file the reader is not
# looking at. A tool that can only rewrite a file in place therefore gets no row.
# `${file}` is how a stdin-reading command is still told what it is reading —
# JSON and YAML are the same bytes to a command with no name for them.
#
# Every row names the `extensions` it lays out, and is found by them alone: a
# file's formatter is not looked up through its server, so `rs` here and in
# `[lsp.rust]` is two choices that happen to agree (ADR 0018).
[formatter.rust]
command = "rustfmt"
extensions = ["rs"]
install.macos = "rustup component add rustfmt"
install.linux = "rustup component add rustfmt"
install.windows = "rustup component add rustfmt"

# `--quiet` because the diagnostics go to stdout beside the code otherwise, and
# `-` because this one wants stdin named rather than assumed.
[formatter.python]
command = "black"
args = ["--quiet", "-"]
extensions = ["py", "pyi"]
install.macos = "pipx install black"
install.linux = "pipx install black"
install.windows = "pip install black"

# No install key, for the reason `[lsp.c]` has no macOS one: this ships with the
# toolchain, so a command that installs it separately would be a row that cannot
# make itself true.
[formatter.go]
command = "gofmt"
extensions = ["go"]

# One command across eight rows, and eight rows rather than one because a
# formatter is looked up by the language a file is. `--stdin-filepath` is what
# tells it which of the eight it is reading, since the bytes do not say.
[formatter.javascript]
command = "prettier"
args = ["--stdin-filepath", "${file}"]
extensions = ["js", "jsx", "mjs", "cjs"]
install.macos = "npm install -g prettier"
install.linux = "npm install -g prettier"
install.windows = "npm install -g prettier"

[formatter.typescript]
command = "prettier"
args = ["--stdin-filepath", "${file}"]
extensions = ["ts", "tsx", "mts", "cts"]
install.macos = "npm install -g prettier"
install.linux = "npm install -g prettier"
install.windows = "npm install -g prettier"

[formatter.vue]
command = "prettier"
args = ["--stdin-filepath", "${file}"]
extensions = ["vue"]
install.macos = "npm install -g prettier"
install.linux = "npm install -g prettier"
install.windows = "npm install -g prettier"

[formatter.json]
command = "prettier"
args = ["--stdin-filepath", "${file}"]
extensions = ["json", "jsonc"]
install.macos = "npm install -g prettier"
install.linux = "npm install -g prettier"
install.windows = "npm install -g prettier"

[formatter.yaml]
command = "prettier"
args = ["--stdin-filepath", "${file}"]
extensions = ["yaml", "yml"]
install.macos = "npm install -g prettier"
install.linux = "npm install -g prettier"
install.windows = "npm install -g prettier"

[formatter.html]
command = "prettier"
args = ["--stdin-filepath", "${file}"]
extensions = ["html", "htm"]
install.macos = "npm install -g prettier"
install.linux = "npm install -g prettier"
install.windows = "npm install -g prettier"

[formatter.css]
command = "prettier"
args = ["--stdin-filepath", "${file}"]
extensions = ["css", "scss", "less"]
install.macos = "npm install -g prettier"
install.linux = "npm install -g prettier"
install.windows = "npm install -g prettier"

[formatter.markdown]
command = "prettier"
args = ["--stdin-filepath", "${file}"]
extensions = ["md", "markdown"]
install.macos = "npm install -g prettier"
install.linux = "npm install -g prettier"
install.windows = "npm install -g prettier"

# A Debug adapter per language, spoken to over its standard streams — or, where
# its `args` name `${port}`, started listening on a port Varde fills in and
# connected to over TCP — or, where it names a `server`, loaded into that
# language server as a plugin and reached on the port the server answers its
# `command` with
# (`docs/adr/0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md`).
# codelldb has spoken stdio since 1.11. It ships as a VS Code extension and
# nothing packages it, so the install unpacks the release into
# `~/.varde/codelldb` and puts a two-line script on `PATH` that runs it from
# there: the adapter finds its own LLDB beside its real path, which a symlink
# is not on every OS.
#
# `launch_args` and `attach_args` are what the Launch box's Create offers as
# fields for that request, each with the one-line explanation shown beside it
# and whether leaving it empty is refused. They are this row's knowledge, not
# Varde's: a row without them gets free key/value fields instead, which is what
# an adapter nobody here has tried gets
# (`docs/adr/0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md`).
[dap.rust]
command = "codelldb"
launch_args = [
  { key = "program", explain = "Path to the built executable to run", required = true },
  { key = "args", explain = "Arguments for the program, as a list" },
  { key = "cwd", explain = "Directory the program runs in" },
  { key = "env", explain = "Environment variables, as a table" },
  { key = "stopOnEntry", explain = "Pause on the first line, true or false" },
  { key = "sourceLanguages", explain = "Languages whose expressions LLDB should read, as a list" },
  { key = "initCommands", explain = "LLDB commands run before the target is created, as a list" },
  { key = "targetCreateCommands", explain = "LLDB commands that create the target instead of `program`, as a list" },
]
attach_args = [
  { key = "pid", explain = "Process id to attach to", required = true },
  { key = "program", explain = "Path to the running executable, if its symbols are wanted" },
  { key = "waitFor", explain = "Wait for the process to start, true or false" },
]
install.macos = "curl -sL --create-dirs https://github.com/vadimcn/codelldb/releases/latest/download/codelldb-darwin-$(uname -m | sed 's/x86_64/x64/').vsix -o ~/.varde/codelldb.vsix && unzip -qo ~/.varde/codelldb.vsix -d ~/.varde/codelldb && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec %s \"$@\"\\n' ~/.varde/codelldb/extension/adapter/codelldb > ~/.local/bin/codelldb && chmod +x ~/.local/bin/codelldb"
install.linux = "curl -sL --create-dirs https://github.com/vadimcn/codelldb/releases/latest/download/codelldb-linux-$(uname -m | sed 's/x86_64/x64/;s/aarch64/arm64/').vsix -o ~/.varde/codelldb.vsix && unzip -qo ~/.varde/codelldb.vsix -d ~/.varde/codelldb && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec %s \"$@\"\\n' ~/.varde/codelldb/extension/adapter/codelldb > ~/.local/bin/codelldb && chmod +x ~/.local/bin/codelldb"

# java-debug lives inside jdtls, where the classpath that maps a file and line to
# a class is. `plugin` is merged into the `[lsp.java]` server's
# `initializationOptions` when it starts, and `command` is what that server is
# sent once a session begins; it answers with the port the adapter listens on.
#
# `hot_replace` is how a rebuilt program reaches a paused process. The protocol
# has no request for it, so the row names the adapter's own: `request` is sent
# whenever the adapter sends `event`, and by `:hotswap` or the Chip. Varde knows
# neither name, the way it knows nothing about `command` above
# (`docs/adr/0021-a-debug-adapter-is-a-hosted-child-reached-three-ways.md`). A
# row without the key offers no hot replace.
[dap.java]
server = "java"
command = "vscode.java.startDebugSession"
plugin = { bundles = ["${java_debug_plugin}"] }
hot_replace = { request = "redefineClasses", event = "hotcodereplace" }
launch_args = [
  { key = "mainClass", explain = "Fully qualified class holding main, or a source file path", required = true },
  { key = "projectName", explain = "Project the class belongs to, where the workspace holds several" },
  { key = "args", explain = "Arguments for main, as a list" },
  { key = "vmArgs", explain = "Arguments for the JVM itself" },
  { key = "cwd", explain = "Directory the program runs in" },
  { key = "env", explain = "Environment variables, as a table" },
  { key = "classPaths", explain = "Entries to put on the class path, as a list" },
]
attach_args = [
  { key = "hostName", explain = "Host the paused JVM listens on", required = true },
  { key = "port", explain = "Port its debug agent listens on", required = true },
  { key = "projectName", explain = "Project whose sources the frames are read against" },
  { key = "timeout", explain = "Milliseconds to keep trying before giving up" },
]

# js-debug listens on the port it is given, and asks for a child session per
# process and worker it attaches to, each over another connection to that port.
# Nothing packages it and its release carries its version in the file name, so
# the install asks GitHub which one is latest, unpacks it into `~/.varde/js-debug`
# and puts a script on `PATH` that runs its server under node. One adapter for
# both languages, as one server is for `[lsp.javascript]` and
# `[lsp.typescript]`.
[dap.javascript]
command = "js-debug-adapter"
args = ["${port}"]
launch_args = [
  { key = "type", explain = "Which js-debug launcher: pwa-node for a program, pwa-chrome for a page", required = true },
  { key = "program", explain = "Entry file to run under node" },
  { key = "cwd", explain = "Directory the program runs in" },
  { key = "args", explain = "Arguments for the program, as a list" },
  { key = "env", explain = "Environment variables, as a table" },
  { key = "runtimeExecutable", explain = "Command to run instead of node, such as npx" },
  { key = "runtimeArgs", explain = "Arguments for that command, as a list" },
  { key = "skipFiles", explain = "Globs to step over rather than into, as a list" },
]
attach_args = [
  { key = "type", explain = "Which js-debug launcher: pwa-node for a process, pwa-chrome for a page", required = true },
  { key = "port", explain = "Inspector port the process was started with", required = true },
  { key = "address", explain = "Host the inspector listens on" },
  { key = "localRoot", explain = "Workspace directory the remote paths map from" },
  { key = "remoteRoot", explain = "Directory those paths are rooted at on the far side" },
]
install.macos = "curl -sL --create-dirs $(curl -s https://api.github.com/repos/microsoft/vscode-js-debug/releases/latest | grep -o 'https://[^\"]*js-debug-dap-v[^\"]*[.]tar[.]gz' | head -1) -o ~/.varde/js-debug.tar.gz && tar xzf ~/.varde/js-debug.tar.gz -C ~/.varde && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec node %s \"$@\"\\n' ~/.varde/js-debug/src/dapDebugServer.js > ~/.local/bin/js-debug-adapter && chmod +x ~/.local/bin/js-debug-adapter"
install.linux = "curl -sL --create-dirs $(curl -s https://api.github.com/repos/microsoft/vscode-js-debug/releases/latest | grep -o 'https://[^\"]*js-debug-dap-v[^\"]*[.]tar[.]gz' | head -1) -o ~/.varde/js-debug.tar.gz && tar xzf ~/.varde/js-debug.tar.gz -C ~/.varde && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec node %s \"$@\"\\n' ~/.varde/js-debug/src/dapDebugServer.js > ~/.local/bin/js-debug-adapter && chmod +x ~/.local/bin/js-debug-adapter"

[dap.typescript]
command = "js-debug-adapter"
args = ["${port}"]
launch_args = [
  { key = "type", explain = "Which js-debug launcher: pwa-node for a program, pwa-chrome for a page", required = true },
  { key = "program", explain = "Entry file to run under node" },
  { key = "cwd", explain = "Directory the program runs in" },
  { key = "args", explain = "Arguments for the program, as a list" },
  { key = "env", explain = "Environment variables, as a table" },
  { key = "runtimeExecutable", explain = "Command to run instead of node, such as npx" },
  { key = "runtimeArgs", explain = "Arguments for that command, as a list" },
  { key = "skipFiles", explain = "Globs to step over rather than into, as a list" },
]
attach_args = [
  { key = "type", explain = "Which js-debug launcher: pwa-node for a process, pwa-chrome for a page", required = true },
  { key = "port", explain = "Inspector port the process was started with", required = true },
  { key = "address", explain = "Host the inspector listens on" },
  { key = "localRoot", explain = "Workspace directory the remote paths map from" },
  { key = "remoteRoot", explain = "Directory those paths are rooted at on the far side" },
]
install.macos = "curl -sL --create-dirs $(curl -s https://api.github.com/repos/microsoft/vscode-js-debug/releases/latest | grep -o 'https://[^\"]*js-debug-dap-v[^\"]*[.]tar[.]gz' | head -1) -o ~/.varde/js-debug.tar.gz && tar xzf ~/.varde/js-debug.tar.gz -C ~/.varde && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec node %s \"$@\"\\n' ~/.varde/js-debug/src/dapDebugServer.js > ~/.local/bin/js-debug-adapter && chmod +x ~/.local/bin/js-debug-adapter"
install.linux = "curl -sL --create-dirs $(curl -s https://api.github.com/repos/microsoft/vscode-js-debug/releases/latest | grep -o 'https://[^\"]*js-debug-dap-v[^\"]*[.]tar[.]gz' | head -1) -o ~/.varde/js-debug.tar.gz && tar xzf ~/.varde/js-debug.tar.gz -C ~/.varde && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec node %s \"$@\"\\n' ~/.varde/js-debug/src/dapDebugServer.js > ~/.local/bin/js-debug-adapter && chmod +x ~/.local/bin/js-debug-adapter"

# What reads a Selection aloud (F35). The synthesizer, the voice and the player
# are named here and in no branch anywhere: a voice nobody has tried works for
# the same reason an untried AI CLI does
# (`docs/adr/0013-a-voice-is-an-installed-binary.md`). `${voice}` and `${scale}`
# are filled the way a server's arguments already are.
#
# `voice` ships blank on purpose. It is a 61MB file on somebody else's disk, so
# an invented path would be a row that reads as configured and cannot work —
# worse than an honest blank, for the reason `[lsp.*]` ships no toolchain paths.
# `install` is what puts it there, run in the shell pane when the row is taken
# in Tools, and `configures` is what it puts there: written into `voice` once
# the install exits 0, unless `voice` is already yours (ADR 0018).
#
# `--noise-w-scale` varies phoneme duration. The model defaults to 0.8 and 1.0
# was chosen by ear from a six-way comparison on the prototype: it is the
# difference between "static" and a voice worth listening to for a page.
#
# No `player.windows`: nothing ships there that plays a wav from a command line
# without a shell of its own, and a command that cannot work is worse than a
# missing row — the same gap `[lsp.zig]` leaves on Linux.
[speech]
command = "piper"
args = ["--model", "${voice}", "--length-scale", "${scale}", "--noise-w-scale", "1.0", "--output_dir", "${dir}"]
voice = ""

# How fast, as a multiplier — higher is faster. It applies to the next Reading,
# because the pace is baked in when the stream is built.
# speed = 1.0

player.macos = "afplay"
player.linux = "aplay"
install.macos = "uv tool install piper-tts && mkdir -p ~/.varde/voices && curl -sL --output-dir ~/.varde/voices -O -O https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/bryce/medium/en_US-bryce-medium.onnx https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/bryce/medium/en_US-bryce-medium.onnx.json"
install.linux = "uv tool install piper-tts && mkdir -p ~/.varde/voices && curl -sL --output-dir ~/.varde/voices -O -O https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/bryce/medium/en_US-bryce-medium.onnx https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/bryce/medium/en_US-bryce-medium.onnx.json"
configures.voice = "~/.varde/voices/en_US-bryce-medium.onnx"
"#;

pub const SEEDED_CONFIG: &str = r#"# Varde reads this file on every start. A project's .varde/config.toml beats
# ~/.varde/config.toml key by key, and both beat the defaults built into Varde.
# It arrives commented out on purpose: it is here so the keys can be found,
# not so this version's answers can be pinned. Uncomment a line to disagree
# with the default beside it; delete it again to go back to whatever the
# version you are running thinks is right.

[view]

# How long after a key is tapped a second tap of the same key still reads as a
# double-tap, in milliseconds. Longer if a second press meant as one keeps
# arriving too late to count.
# double_tap_ms = 300

[editor]

# What Tab lays down while inserting, and what Enter reaches for when it opens
# a block in a file that holds no indentation of its own to copy. Indentation
# the file already has still wins there: the lines in front of you are better
# evidence about that file than any number here.
# tab_width = 4

# Whether the mirror of the file down the editor's right-hand edge is up when a
# project is opened. `:minimap` is the same switch while you are in there, and
# what it was left at wins over this.
# minimap = true

[risk]

# The cyclomatic complexity a function may reach before Risk names it.
# threshold = 15

# How many times the Gate may hand a refactor back before it stops. A function
# the AI cannot get under the threshold is a function to look at yourself, and
# an uncapped loop spends tokens discovering that.
# max_iterations = 10

[speech]

# What speaks a Reading, and how it is called. `${voice}` is the row below,
# `${scale}` the reciprocal of the speed, and `${dir}` where the stream goes.
# command = "piper"
# args = ["--model", "${voice}", "--length-scale", "${scale}", "--noise-w-scale", "1.0", "--output_dir", "${dir}"]

# The voice model on this machine. Blank until you have one: taking the speech
# row in Tools fetches one and fills this in.
# voice = ""

# How fast, as a multiplier — higher is faster. It applies to the next Reading,
# because the pace is baked in when the stream is built.
# speed = 1.0
"#;

const TEMPLATE_SETTINGS: &str = r#"# Varde reads this file on every start. A project's .varde/config.toml beats
# it key by key, and both beat the defaults built into Varde.
#
# The settings come first, commented out on purpose: they are here so the keys
# can be found, not so this version's answers can be pinned. Uncomment a line
# to disagree with the default beside it.
#
# The programs Varde runs come after them — language servers, formatters, what
# they need, and the voice — and those rows are live. Edit a row to change what
# runs, or write one for a language this file does not name.

[view]

# How long after a key is tapped a second tap of the same key still reads as a
# double-tap, in milliseconds.
# double_tap_ms = 300

[editor]

# What Tab lays down while inserting, and what Enter reaches for when it opens
# a block in a file that holds no indentation of its own to copy.
# tab_width = 4

# Whether the mirror of the file down the editor's right-hand edge is up when a
# project is opened. `:minimap` is the same switch while you are in there.
# minimap = true

[ai]

# The only environment variables the AI pane's CLI is handed. Everything else
# Varde was started with stays out of it, secrets included: name a variable
# here to pass it through. A project's config cannot add to this list.
# env = ["HOME", "PATH", "USER", "LOGNAME", "SHELL", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE"]

[risk]

# The cyclomatic complexity a function may reach before Risk names it.
# threshold = 15

# How many times the Gate may hand a refactor back before it stops.
# max_iterations = 10

# What a Run mark stands beside and starts: the files a row looks in, a
# tree-sitter query whose `@run` capture is the line it marks, a command for a
# shell whose prompt is waiting, and a Launch configuration for Debug. Each
# `${…}` is filled from the query capture of that name, and `${file}` with the
# file's path. A row of your own gives a language one without a release.
[run.rust_main]
# extensions = ["rs"]
# query = '((function_item name: (identifier) @name @run) (#eq? @name "main"))'
# run = "cargo run"
# debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["script import json, subprocess; lldb.debugger.CreateTarget(next(m['executable'] for m in map(json.loads, subprocess.run(['cargo', 'build', '--message-format=json'], stdout=subprocess.PIPE, text=True).stdout.splitlines()) if m.get('executable')))"] } }

[run.rust_test]
# extensions = ["rs"]
# query = '((attribute_item (attribute (identifier) @attribute)) . (function_item name: (identifier) @name @run) (#eq? @attribute "test"))'
# run = "cargo test ${name} -- --exact"
# debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["script import json, subprocess; lldb.debugger.CreateTarget(next(m['executable'] for m in map(json.loads, subprocess.run(['cargo', 'test', '--no-run', '--message-format=json'], stdout=subprocess.PIPE, text=True).stdout.splitlines()) if m.get('executable') and m['profile']['test']))"], args = ["${name}", "--exact"] } }

[run.rust_module_test]
# extensions = ["rs"]
# query = '((mod_item name: (identifier) @module body: (declaration_list (attribute_item (attribute (identifier) @attribute)) . (function_item name: (identifier) @name @run))) (#eq? @attribute "test"))'
# run = "cargo test ${module}::${name} -- --exact"
# debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["script import json, subprocess; lldb.debugger.CreateTarget(next(m['executable'] for m in map(json.loads, subprocess.run(['cargo', 'test', '--no-run', '--message-format=json'], stdout=subprocess.PIPE, text=True).stdout.splitlines()) if m.get('executable') and m['profile']['test']))"], args = ["${module}::${name}", "--exact"] } }

[run.java_main]
# extensions = ["java"]
# query = '((method_declaration name: (identifier) @name @run) (#eq? @name "main"))'
# run = "java ${file}"
# debug = { adapter = "java", request = "launch", args = { mainClass = "${file}" } }

[run.java_test]
# extensions = ["java"]
# query = '(class_declaration name: (identifier) @class body: (class_body (method_declaration (modifiers (marker_annotation name: (identifier) @annotation)) name: (identifier) @name @run) (#eq? @annotation "Test")))'
# run = "mvn test -Dtest=${class}#${name}"

[run.javascript_test]
# extensions = ["js", "jsx", "mjs", "cjs"]
# query = '((call_expression function: (identifier) @function arguments: (arguments . (string (string_fragment) @name))) @run (#match? @function "^(test|it)$"))'
# run = "npx vitest run ${file} -t ${name}"
# debug = { adapter = "javascript", request = "launch", args = { type = "pwa-node", runtimeExecutable = "npx", runtimeArgs = ["vitest", "run", "${file}", "-t", "${name}"] } }

[run.typescript_test]
# extensions = ["ts", "tsx", "mts", "cts"]
# query = '((call_expression function: (identifier) @function arguments: (arguments . (string (string_fragment) @name))) @run (#match? @function "^(test|it)$"))'
# run = "npx vitest run ${file} -t ${name}"
# debug = { adapter = "typescript", request = "launch", args = { type = "pwa-node", runtimeExecutable = "npx", runtimeArgs = ["vitest", "run", "${file}", "-t", "${name}"] } }

"#;

pub fn template() -> String {
    [TEMPLATE_SETTINGS, PROGRAMS].concat()
}

pub const GLOBAL_LABEL: &str = "~/.varde/config.toml";
pub const PROJECT_LABEL: &str = ".varde/config.toml";

pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub file: String,
    pub line: usize,
    pub fault: ConfigFault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFault {
    NotToml,
    WrongType(String),
    Incomplete {
        entry: String,
        key: String,
    },
    ClaimedTwice {
        extension: String,
        rows: [String; 2],
    },
    Unreadable,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: ", self.file, self.line)?;
        match &self.fault {
            ConfigFault::NotToml => write!(f, "config is not valid TOML"),
            ConfigFault::WrongType(detail) => write!(f, "{detail}"),
            ConfigFault::Incomplete { entry, key } => write!(f, "[{entry}] names no {key}"),
            ConfigFault::ClaimedTwice {
                extension,
                rows: [first, second],
            } => write!(f, "[{first}] and [{second}] both claim .{extension}"),
            ConfigFault::Unreadable => write!(f, "config cannot be read"),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PathStatus {
    #[default]
    Folder,
    Missing,
    NotAFolder,
    Unreadable,
}

#[derive(Debug, Default)]
pub struct Startup {
    pub root: PathBuf,
    pub sidecar: Option<PathBuf>,
    pub varde_home: PathBuf,
    pub reviews: BTreeSet<u32>,
    pub path_status: PathStatus,
    pub global_config: Option<String>,
    pub project_config: Option<String>,
    pub shipped: BTreeMap<PathBuf, String>,
    pub state_json: Option<String>,
    pub risk_json: Option<String>,
    pub head: Option<String>,
    pub repo: Option<Vec<crate::review::GitFile>>,
    pub checkout: Option<PathBuf>,
    pub checkout_manifest: Option<String>,
    pub running_version: String,
    pub os: String,
    pub arch: String,
}

pub const RELEASE_URL: &str = "https://api.github.com/repos/oyvij/varde-editor/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub asset: String,
    pub checksums: String,
}

#[derive(serde::Deserialize)]
struct Published {
    tag_name: String,
    assets: Vec<Attached>,
}

#[derive(serde::Deserialize)]
struct Attached {
    name: String,
    browser_download_url: String,
}

fn asset_name(os: &str, arch: &str) -> String {
    format!("varde-{os}-{arch}")
}

pub fn release(body: &str, os: &str, arch: &str, running: &str) -> Option<Release> {
    let published: Published = serde_json::from_str(body).ok()?;
    let version = published.tag_name.strip_prefix('v')?;
    if !is_update(version, running) {
        return None;
    }
    let url = |name: &str| {
        published
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
    };
    Some(Release {
        version: version.to_string(),
        asset: url(&asset_name(os, arch))?,
        checksums: url("SHA256SUMS")?,
    })
}

pub fn verify(list: &str, asset_url: &str, downloaded: &[u8]) -> Result<(), ReplaceFailed> {
    let name = asset_url.rsplit('/').next().unwrap_or(asset_url);
    let expected = list
        .lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_, file)| file.trim_start().trim_start_matches('*') == name)
        .map(|(sum, _)| sum)
        .ok_or(ReplaceFailed::NoAsset)?;
    let actual: String = sha2::Sha256::digest(downloaded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    match actual.eq_ignore_ascii_case(expected) {
        true => Ok(()),
        false => Err(ReplaceFailed::Checksum),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Server {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub also_served_by: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub language_ids: BTreeMap<String, String>,
    #[serde(default)]
    pub install: BTreeMap<String, String>,
    /// JSON map, not toml::Table: TOML allows nan, which a later TOML-to-JSON conversion cannot express
    #[serde(default)]
    pub initialization_options: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub partial: Option<String>,
    #[serde(default)]
    pub unanswerable: Option<Unanswerable>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Unanswerable {
    pub request: String,
    pub response: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Formatter {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub install: BTreeMap<String, String>,
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Adapter {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub install: BTreeMap<String, String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub plugin: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub hot_replace: Option<HotReplace>,
    #[serde(default)]
    pub launch_args: Vec<Argument>,
    #[serde(default)]
    pub attach_args: Vec<Argument>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Argument {
    pub key: String,
    pub explain: String,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct HotReplace {
    pub request: String,
    pub event: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Launch {
    #[serde(default)]
    pub adapter: String,
    #[serde(default)]
    pub request: String,
    #[serde(default)]
    pub args: serde_json::Map<String, serde_json::Value>,
    #[serde(default = "attaches_again")]
    pub reattach: bool,
}

fn attaches_again() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Run {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub run: String,
    #[serde(default)]
    pub debug: Option<Launch>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Fact {
    #[serde(default)]
    pub marker: String,
    #[serde(default)]
    pub value: FactValue,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub command_marker: Option<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub install: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactValue {
    #[default]
    Marker,
    Directory,
}

#[derive(Default, serde::Deserialize)]
struct Ai {
    #[serde(default)]
    env: Vec<String>,
}

#[derive(Default, serde::Deserialize)]
struct Knowledge {
    #[serde(default)]
    enabled: bool,
    vault: Option<String>,
}

#[derive(Default, serde::Deserialize)]
struct Layer {
    #[serde(default)]
    ai: Ai,
    #[serde(default)]
    knowledge: Knowledge,
    #[serde(default)]
    lsp: BTreeMap<String, Server>,
    #[serde(default)]
    formatter: BTreeMap<String, Formatter>,
    #[serde(default)]
    facts: BTreeMap<String, Fact>,
    #[serde(default)]
    dap: BTreeMap<String, Adapter>,
    #[serde(default)]
    launch: BTreeMap<String, Launch>,
    #[serde(default)]
    run: BTreeMap<String, Run>,
}

/// toml::Spanned only deserializes from source text, never from a toml::Value
#[derive(serde::Deserialize)]
struct SourceLayer {
    #[serde(default)]
    lsp: BTreeMap<String, toml::Spanned<Server>>,
    #[serde(default)]
    formatter: BTreeMap<String, toml::Spanned<Formatter>>,
    #[serde(default)]
    facts: BTreeMap<String, toml::Spanned<Fact>>,
    #[serde(default)]
    dap: BTreeMap<String, toml::Spanned<Adapter>>,
    #[serde(default)]
    launch: BTreeMap<String, toml::Spanned<Launch>>,
    #[serde(default)]
    run: BTreeMap<String, toml::Spanned<Run>>,
}

type Origins = BTreeMap<String, (String, usize)>;

#[derive(Debug, Clone, Default)]
pub struct Config(pub(crate) Table);

impl Config {
    pub fn servers(&self) -> BTreeMap<String, Server> {
        self.layer().lsp
    }

    pub fn formatters(&self) -> BTreeMap<String, Formatter> {
        self.layer().formatter
    }

    pub fn facts(&self) -> BTreeMap<String, Fact> {
        self.layer().facts
    }

    pub fn adapters(&self) -> BTreeMap<String, Adapter> {
        self.layer().dap
    }

    pub fn launches(&self) -> BTreeMap<String, Launch> {
        self.layer().launch
    }

    pub fn runs(&self) -> BTreeMap<String, Run> {
        self.layer().run
    }

    fn layer(&self) -> Layer {
        toml::Value::Table(self.0.clone())
            .try_into::<Layer>()
            .unwrap_or_default()
    }

    pub fn get(&self, dotted: &str) -> Option<String> {
        let mut parts = dotted.split('.');
        let mut value = self.0.get(parts.next()?)?;
        for part in parts {
            value = value.as_table()?.get(part)?;
        }
        Some(match value {
            toml::Value::String(text) => text.clone(),
            other => other.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    Path(&'static str),
    Config(ConfigError),
}

pub fn start(input: &Startup) -> Result<(State, Config, Vec<Effect>), StartupError> {
    if let Some(reason) = path_refusal(input.path_status) {
        return Err(StartupError::Path(reason));
    }
    let project = match &input.sidecar {
        Some(_) => None,
        None => input.project_config.as_deref(),
    };
    let config = Config(
        merged_config(input.global_config.as_deref(), project).map_err(StartupError::Config)?,
    );
    let (checkout, update) = checkout(input);
    let binary_install = checkout.is_none();
    let mut state = initial_state(input, &config, checkout, update);

    let mut effects = vec![
        Effect::EnsureDir(crate::varde_dir(&input.root, input.sidecar.as_deref())),
        Effect::DeleteDir(crate::tmp_dir(&input.varde_home)),
        Effect::EnsureDir(crate::tmp_dir(&input.varde_home)),
    ];
    if binary_install {
        effects.push(Effect::CheckRelease {
            url: RELEASE_URL.to_string(),
        });
    }
    if input.sidecar.is_none() && input.project_config.is_none() {
        effects.push(Effect::WriteFile {
            path: crate::varde_dir(&input.root, input.sidecar.as_deref()).join(CONFIG_FILE),
            contents: SEEDED_CONFIG.to_string(),
        });
    }
    if input.global_config.is_none() {
        effects.push(Effect::WriteFile {
            path: input.varde_home.join(CONFIG_FILE),
            contents: template(),
        });
    }
    let stale = crate::skills::SHIPPED.iter().any(|(file, text)| {
        input
            .shipped
            .get(&input.varde_home.join(file))
            .map(String::as_str)
            != Some(*text)
    });
    if stale {
        effects.extend(
            crate::skills::OWNED
                .iter()
                .map(|folder| Effect::DeleteDir(input.varde_home.join(folder))),
        );
        effects.extend(
            crate::skills::SHIPPED
                .iter()
                .map(|(file, text)| Effect::WriteFile {
                    path: input.varde_home.join(file),
                    contents: text.to_string(),
                }),
        );
    }
    let buffers = saved_buffers(&input.root, input.state_json.as_deref());
    state.restoring = buffers.len();
    effects.extend(buffers.into_iter().map(Effect::OpenBuffer));
    let files: std::collections::BTreeSet<PathBuf> = state
        .breakpoints
        .iter()
        .map(|breakpoint| breakpoint.file.clone())
        .collect();
    effects.extend(files.into_iter().map(Effect::ReadBreakpointFile));

    let cache = cached(input);
    let unmeasured = cache.is_none();
    if let Some(figures) = cache {
        state.risk.figure = risk::Figure::Current(figures);
    }

    let (mut state, entered) = crate::enter_view(&state, state.view);
    effects.extend(entered);

    if unmeasured && input.sidecar.is_none() && !state.risk.in_flight() {
        effects.push(risk::analyse(&mut state, Scope::Workspace));
    }
    Ok((state, config, effects))
}

fn path_refusal(status: PathStatus) -> Option<&'static str> {
    match status {
        PathStatus::Folder => None,
        PathStatus::Missing => Some("no-such-folder"),
        PathStatus::NotAFolder => Some("not-a-folder"),
        PathStatus::Unreadable => Some("folder-not-readable"),
    }
}

pub(crate) fn merged_config(
    global: Option<&str>,
    project: Option<&str>,
) -> Result<Table, ConfigError> {
    let mut table = toml::Table::new();
    let mut origins = BTreeMap::new();
    let seeded = template();
    for (source, label) in [
        (DEFAULTS, "defaults"),
        (global.unwrap_or(&seeded), GLOBAL_LABEL),
        (project.unwrap_or_default(), PROJECT_LABEL),
    ] {
        let (mut overlay, mentioned) = parse(source, label)?;
        if label == PROJECT_LABEL {
            overlay.remove("ai");
            overlay.remove("knowledge");
        }
        origins.extend(mentioned);
        merge(&mut table, overlay);
    }
    refuse_incomplete(&table, &origins)?;
    refuse_claimed_twice(&table, &origins)?;
    refuse_unusable_runs(&table, &origins)?;
    Ok(table)
}

fn refuse_unusable_runs(table: &Table, origins: &Origins) -> Result<(), ConfigError> {
    let runs = Config(table.clone()).runs();
    for (name, row) in &runs {
        if let Some(why) = crate::run::unusable(row) {
            let entry = format!("run.{name}");
            let (file, line) = origins.get(&entry).cloned().unwrap_or_default();
            return Err(ConfigError {
                file,
                line,
                fault: ConfigFault::WrongType(format!("[{entry}] {why}")),
            });
        }
    }
    Ok(())
}

fn refuse_incomplete(table: &Table, origins: &Origins) -> Result<(), ConfigError> {
    for (section, key) in [
        ("lsp", "command"),
        ("formatter", "command"),
        ("facts", "marker"),
        ("dap", "command"),
        ("launch", "adapter"),
        ("launch", "request"),
    ] {
        let Some(entries) = table.get(section).and_then(toml::Value::as_table) else {
            continue;
        };
        for (name, values) in entries {
            let named = values
                .get(key)
                .and_then(toml::Value::as_str)
                .is_some_and(|value| !value.is_empty());
            if named {
                continue;
            }
            let entry = format!("{section}.{name}");
            let (file, line) = origins.get(&entry).cloned().unwrap_or_default();
            return Err(ConfigError {
                file,
                line,
                fault: ConfigFault::Incomplete {
                    entry,
                    key: key.to_string(),
                },
            });
        }
    }
    Ok(())
}

fn refuse_claimed_twice(table: &Table, origins: &Origins) -> Result<(), ConfigError> {
    let Some(rows) = table.get("lsp").and_then(toml::Value::as_table) else {
        return Ok(());
    };
    let mut owners: BTreeMap<&str, &str> = BTreeMap::new();
    for (name, values) in rows {
        let claimed = values.get("extensions").and_then(toml::Value::as_array);
        for extension in claimed
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
        {
            let Some(owner) = owners
                .insert(extension, name)
                .filter(|owner| *owner != name)
            else {
                continue;
            };
            let rows = [format!("lsp.{owner}"), format!("lsp.{name}")];
            let layer = |row: &String| {
                let file = origins.get(row).map(|(file, _)| file.as_str());
                ["defaults", GLOBAL_LABEL, PROJECT_LABEL]
                    .iter()
                    .position(|label| Some(*label) == file)
            };
            let nearest = rows.iter().max_by_key(|row| layer(row)).expect("two rows");
            let (file, line) = origins.get(nearest).cloned().unwrap_or_default();
            return Err(ConfigError {
                file,
                line,
                fault: ConfigFault::ClaimedTwice {
                    extension: extension.to_string(),
                    rows,
                },
            });
        }
    }
    Ok(())
}

fn initial_state(
    input: &Startup,
    config: &Config,
    checkout: Option<PathBuf>,
    update: Option<String>,
) -> State {
    let mut state = State {
        root: input.root.clone(),
        sidecar: input.sidecar.clone(),
        varde_home: input.varde_home.clone(),
        reviews: input.reviews.clone(),
        view: last_view(input.state_json.as_deref()).unwrap_or(View::Edit),
        expanded: saved_paths(&input.root, input.state_json.as_deref(), "expanded")
            .into_iter()
            .collect(),
        tree_divider: saved_number(input.state_json.as_deref(), "tree_divider").unwrap_or(30),
        ai_width: saved_number(input.state_json.as_deref(), "ai_width"),
        strip_height: saved_number(input.state_json.as_deref(), "strip_height"),
        output_width: saved_number(input.state_json.as_deref(), "output_width"),
        breakpoints: saved_breakpoints(&input.root, input.state_json.as_deref()),
        snippets: saved_list(input.state_json.as_deref(), "snippets"),
        evaluator_at: saved_window(input.state_json.as_deref()),
        exception_filters: input
            .state_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|mut parsed| serde_json::from_value(parsed["exception_filters"].take()).ok())
            .unwrap_or_default(),
        ai_pane: match saved_text(input.state_json.as_deref(), "ai_pane").as_deref() {
            Some("Tall") => crate::layout::AiPane::Tall,
            _ => crate::layout::AiPane::Beside,
        },
        corner: match saved_text(input.state_json.as_deref(), "corner").as_deref() {
            Some("Risk") => crate::layout::Corner::Risk,
            Some("Buffers") => crate::layout::Corner::Buffers,
            Some("History") => crate::layout::Corner::History,
            Some("Breakpoints") => crate::layout::Corner::Breakpoints,
            Some("Conflicts") => crate::layout::Corner::Conflicts,
            Some(saved) if saved.starts_with("Diagnostics") => {
                crate::layout::Corner::Diagnostics(crate::lsp::Severity::Error)
            }
            Some(_) => crate::layout::Corner::Hidden,
            None => match saved_text(input.state_json.as_deref(), "risk_list").as_deref() {
                Some("Shown") => crate::layout::Corner::Risk,
                _ => crate::layout::Corner::Hidden,
            },
        },
        editor_field: saved_flag(input.state_json.as_deref(), "editor_field").unwrap_or(true),
        minimap: saved_flag(input.state_json.as_deref(), "minimap")
            .unwrap_or_else(|| config.get("editor.minimap").as_deref() != Some("false")),
        ai_command: saved_text(input.state_json.as_deref(), "ai_command")
            .or_else(|| config.get("ai.command"))
            .unwrap_or_else(|| "claude".to_string()),
        os: input.os.clone(),
        arch: input.arch.clone(),
        running_version: input.running_version.clone(),
        checkout,
        update,
        head: input.head.clone(),
        repo: input.repo.clone(),
        ..State::default()
    };
    configure(&mut state, config);
    state
}

fn configure(state: &mut State, config: &Config) {
    *state = State {
        editor_theme: config
            .get("editor.theme")
            .unwrap_or_else(|| "dark".to_string()),
        double_tap_ms: config
            .get("view.double_tap_ms")
            .and_then(|ms| ms.parse().ok())
            .unwrap_or(300),
        tab_width: config
            .get("editor.tab_width")
            .and_then(|width| width.parse().ok())
            .unwrap_or(crate::editor::DEFAULT_TAB_WIDTH),
        risk_threshold: config
            .get("risk.threshold")
            .and_then(|figure| figure.parse().ok())
            .unwrap_or(risk::DEFAULT_THRESHOLD),
        max_iterations: config
            .get("risk.max_iterations")
            .and_then(|cap| cap.parse().ok())
            .unwrap_or(risk::DEFAULT_MAX_ITERATIONS),
        test_command: config.get("risk.test_command"),
        servers: config.servers(),
        formatters: config.formatters(),
        facts: config.facts(),
        adapters: config.adapters(),
        launches: config.launches(),
        runs: config.runs(),
        speech: speech(config, &state.os),
        ai_env: config.layer().ai.env,
        vault: vault(config, &state.varde_home),
        ..std::mem::take(state)
    };
}

pub(crate) fn default_vault(varde_home: &Path) -> PathBuf {
    varde_home.join("knowledge")
}

fn vault(config: &Config, varde_home: &Path) -> Option<PathBuf> {
    let knowledge = config.layer().knowledge;
    let home = varde_home.parent().unwrap_or(varde_home);
    knowledge.enabled.then(|| match knowledge.vault {
        None => default_vault(varde_home),
        Some(vault) => match Path::new(&vault).strip_prefix("~") {
            Ok(rest) => home.join(rest),
            Err(_) => PathBuf::from(vault),
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDisk {
    Missing,
    Text(String),
    Unreadable,
}

pub(crate) fn reload(
    state: &mut State,
    global: OnDisk,
    project: OnDisk,
) -> Result<(), ConfigError> {
    let text = |layer: OnDisk, label: &str| match layer {
        OnDisk::Missing => Ok(None),
        OnDisk::Text(text) => Ok(Some(text)),
        OnDisk::Unreadable => Err(ConfigError {
            file: label.to_string(),
            line: 1,
            fault: ConfigFault::Unreadable,
        }),
    };
    let global = text(global, GLOBAL_LABEL)?;
    let project = match &state.sidecar {
        Some(_) => None,
        None => text(project, PROJECT_LABEL)?,
    };
    let config = Config(merged_config(global.as_deref(), project.as_deref())?);
    configure(state, &config);
    Ok(())
}

pub(crate) fn speech(config: &Config, os: &str) -> crate::reading::Speech {
    let named = |key: &str| config.get(key).unwrap_or_default();
    crate::reading::Speech {
        command: named("speech.command"),
        args: config
            .0
            .get("speech")
            .and_then(|row| row.get("args")?.as_array())
            .map(|args| {
                args.iter()
                    .filter_map(|arg| Some(arg.as_str()?.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        voice: named("speech.voice"),
        speed: named("speech.speed").parse().unwrap_or(1.0),
        player: named(&format!("speech.player.{os}")),
        install: named(&format!("speech.install.{os}")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    pub kind: &'static str,
    pub name: String,
    pub command: String,
    pub install: Option<String>,
}

pub fn deps(global_config: Option<&str>, os: &str) -> Result<Vec<Dep>, ConfigError> {
    let config = Config(merged_config(global_config, None)?);
    let row = |kind, name: &str, command: &str, install: Option<&String>| Dep {
        kind,
        name: name.to_string(),
        command: command.to_string(),
        install: install.cloned(),
    };
    let speech = speech(&config, os);
    let install = (!speech.install.is_empty()).then_some(&speech.install);
    let servers = config.servers();
    Ok(config
        .servers()
        .iter()
        .map(|(name, s)| row("lsp", name, &s.command, s.install.get(os)))
        .chain(
            config
                .formatters()
                .iter()
                .map(|(name, f)| row("formatter", name, &f.command, f.install.get(os))),
        )
        .chain(config.adapters().iter().map(|(name, a)| {
            let command = match &a.server {
                Some(server) => servers
                    .get(server)
                    .map_or_else(|| server.clone(), |server| server.command.clone()),
                None => a.command.clone(),
            };
            row("dap", name, &command, a.install.get(os))
        }))
        .chain([
            row("speech", "speech", &speech.command, install),
            row("player", "speech", &speech.player, None),
        ])
        .filter(|dep| !dep.command.is_empty())
        .collect())
}

fn cached(input: &Startup) -> Option<risk::Figures> {
    risk::cached(input.risk_json.as_deref()?, input.head.as_deref()?)
}

fn checkout(input: &Startup) -> (Option<PathBuf>, Option<String>) {
    let Some(root) = input.checkout.as_ref() else {
        return (None, None);
    };
    let Some(package) = input
        .checkout_manifest
        .as_ref()
        .and_then(|source| source.parse::<Table>().ok())
        .and_then(|manifest| manifest.get("package")?.as_table().cloned())
    else {
        return (None, None);
    };
    if package.get("name").and_then(toml::Value::as_str) != Some("varde") {
        return (None, None);
    }
    let version = package
        .get("version")
        .and_then(toml::Value::as_str)
        .filter(|version| is_update(version, &input.running_version));
    (Some(root.clone()), version.map(str::to_string))
}

fn is_update(checkout: &str, running: &str) -> bool {
    match (
        semver::Version::parse(checkout),
        semver::Version::parse(running),
    ) {
        (Ok(checkout), Ok(running)) => checkout > running,
        _ => false,
    }
}

fn merge(base: &mut Table, overlay: Table) {
    for (key, value) in overlay {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(existing)), toml::Value::Table(incoming)) => {
                merge(existing, incoming);
            }
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

fn parse(source: &str, label: &str) -> Result<(Table, Origins), ConfigError> {
    let line = |offset: usize| source[..offset].matches('\n').count() + 1;
    let at = |error: &toml::de::Error| error.span().map_or(1, |span| line(span.start));
    let table = source.parse::<Table>().map_err(|error| ConfigError {
        file: label.to_string(),
        line: at(&error),
        fault: ConfigFault::NotToml,
    })?;
    let wrong_type = |error: toml::de::Error| ConfigError {
        file: label.to_string(),
        line: at(&error),
        fault: ConfigFault::WrongType(error.message().to_string()),
    };
    toml::from_str::<Layer>(source).map_err(wrong_type)?;
    let layer = toml::from_str::<SourceLayer>(source).map_err(wrong_type)?;
    let origins = layer
        .lsp
        .iter()
        .map(|(name, entry)| (format!("lsp.{name}"), entry.span()))
        .chain(
            layer
                .formatter
                .iter()
                .map(|(name, entry)| (format!("formatter.{name}"), entry.span())),
        )
        .chain(
            layer
                .facts
                .iter()
                .map(|(name, entry)| (format!("facts.{name}"), entry.span())),
        )
        .chain(
            layer
                .dap
                .iter()
                .map(|(name, entry)| (format!("dap.{name}"), entry.span())),
        )
        .chain(
            layer
                .launch
                .iter()
                .map(|(name, entry)| (format!("launch.{name}"), entry.span())),
        )
        .chain(
            layer
                .run
                .iter()
                .map(|(name, entry)| (format!("run.{name}"), entry.span())),
        )
        .map(|(entry, span)| (entry, (label.to_string(), line(span.start))))
        .collect();
    Ok((table, origins))
}

fn saved_paths(root: &Path, state_json: Option<&str>, key: &str) -> Vec<PathBuf> {
    let Some(parsed) = state_json.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    else {
        return Vec::new();
    };
    parsed
        .get(key)
        .and_then(|value| value.as_array())
        .map(|paths| {
            paths
                .iter()
                .filter_map(|p| p.as_str())
                .map(|p| root.join(p))
                .collect()
        })
        .unwrap_or_default()
}

fn saved_buffers(root: &Path, state_json: Option<&str>) -> Vec<PathBuf> {
    let mut paths = saved_paths(root, state_json, "buffers");
    let current = saved_text(state_json, "current_buffer").map(|rest| root.join(rest));
    if let Some(at) = current.and_then(|path| paths.iter().position(|open| *open == path)) {
        let current = paths.remove(at);
        paths.push(current);
    }
    paths
}

fn saved_breakpoints(root: &Path, state_json: Option<&str>) -> Vec<crate::debug::Breakpoint> {
    let Some(parsed) = state_json.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    else {
        return Vec::new();
    };
    let Some(saved) = parsed.get("breakpoints").and_then(|value| value.as_array()) else {
        return Vec::new();
    };
    saved
        .iter()
        .filter_map(|breakpoint| {
            let text = |key: &str| {
                breakpoint
                    .get(key)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            Some(crate::debug::Breakpoint {
                file: root.join(breakpoint.get("file")?.as_str()?),
                line: usize::try_from(breakpoint.get("line")?.as_u64()?).ok()?,
                text: breakpoint.get("text")?.as_str()?.to_string(),
                stale: false,
                properties: crate::debug::Properties {
                    condition: text("condition"),
                    hit_count: text("hit_count"),
                    log_message: text("log_message"),
                    suspend: match breakpoint.get("suspend").and_then(|value| value.as_str()) {
                        Some("all") => crate::debug::Suspend::All,
                        _ => crate::debug::Suspend::Thread,
                    },
                },
            })
        })
        .collect()
}

fn saved_list(state_json: Option<&str>, key: &str) -> Vec<String> {
    let Some(parsed) = state_json.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    else {
        return Vec::new();
    };
    parsed
        .get(key)
        .and_then(|value| value.as_array())
        .map(|held| {
            held.iter()
                .filter_map(|text| Some(text.as_str()?.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn saved_window(state_json: Option<&str>) -> Option<crate::layout::Area> {
    let parsed: serde_json::Value = serde_json::from_str(state_json?).ok()?;
    let at = parsed.get("evaluator")?;
    let number = |key: &str| u16::try_from(at.get(key)?.as_u64()?).ok();
    Some(crate::layout::Area {
        x: number("column")?,
        y: number("row")?,
        width: number("width")?,
        height: number("height")?,
    })
}

fn saved_text(state_json: Option<&str>, key: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(state_json?).ok()?;
    parsed.get(key)?.as_str().map(str::to_string)
}

fn saved_number(state_json: Option<&str>, key: &str) -> Option<u32> {
    let parsed: serde_json::Value = serde_json::from_str(state_json?).ok()?;
    parsed.get(key)?.as_u64().map(|n| n as u32)
}

fn saved_flag(state_json: Option<&str>, key: &str) -> Option<bool> {
    let parsed: serde_json::Value = serde_json::from_str(state_json?).ok()?;
    parsed.get(key)?.as_bool()
}

fn last_view(state_json: Option<&str>) -> Option<View> {
    let parsed: serde_json::Value = serde_json::from_str(state_json?).ok()?;
    match parsed.get("last_view")?.as_str()? {
        "Review" => Some(View::Review),
        "Story" => Some(View::Story),
        "Edit" => Some(View::Edit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        asset_name, deps, is_update, merged_config, release, start, template, verify, Config,
        ConfigError, ConfigFault, Dep, Effect, FactValue, ReplaceFailed, Startup, StartupError,
        DEFAULTS, GLOBAL_LABEL, PROGRAMS, PROJECT_LABEL, SEEDED_CONFIG,
    };

    fn fresh() -> Config {
        Config(merged_config(None, None).expect("the template parses"))
    }

    fn uncommented(text: &str) -> toml::Table {
        let is_key = |k: &str| k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        text.lines()
            .map(|line| match line.strip_prefix("# ") {
                Some(key) if key.split_once(" = ").is_some_and(|(k, _)| is_key(k)) => key,
                _ => line,
            })
            .collect::<Vec<&str>>()
            .join("\n")
            .parse()
            .expect("valid TOML uncommented")
    }

    #[test]
    fn the_breakpoint_list_is_back_in_the_corner_after_a_restart() {
        let (state, _, _) = start(&Startup {
            state_json: Some(r#"{"corner": "Breakpoints"}"#.to_string()),
            ..Startup::default()
        })
        .expect("started");
        assert_eq!(state.corner, crate::layout::Corner::Breakpoints);
    }

    #[test]
    fn a_breakpoints_properties_survive_a_restart() {
        let properties = crate::debug::Properties {
            condition: "count > 1".to_string(),
            hit_count: "10".to_string(),
            log_message: "count is {count}".to_string(),
            suspend: crate::debug::Suspend::All,
        };
        let saved = crate::State {
            breakpoints: vec![crate::debug::Breakpoint {
                file: std::path::PathBuf::from("src/main.rs"),
                line: 3,
                text: "let count = 3;".to_string(),
                stale: false,
                properties: properties.clone(),
            }],
            ..crate::State::default()
        };
        let (state, _, _) = start(&Startup {
            state_json: Some(crate::state_json(&saved)),
            ..Startup::default()
        })
        .expect("started");
        assert_eq!(state.breakpoints[0].properties, properties);
    }

    #[test]
    fn the_evaluators_place_and_snippets_survive_a_restart() {
        let saved = crate::State {
            evaluator_at: Some(crate::layout::Area {
                x: 30,
                y: 4,
                width: 50,
                height: 10,
            }),
            snippets: vec!["orders.len()".to_string(), "count + 1".to_string()],
            ..crate::State::default()
        };
        let (state, _, _) = start(&Startup {
            state_json: Some(crate::state_json(&saved)),
            ..Startup::default()
        })
        .expect("started");
        assert_eq!(state.evaluator_at, saved.evaluator_at);
        assert_eq!(state.snippets, saved.snippets);
        let (fresh, _, _) = start(&Startup::default()).expect("started");
        assert_eq!(fresh.evaluator_at, None);
    }

    #[test]
    fn the_diagnostic_list_is_back_in_the_corner_after_a_restart() {
        let (state, _, _) = start(&Startup {
            state_json: Some(r#"{"corner": "Diagnostics(Hint)"}"#.to_string()),
            ..Startup::default()
        })
        .expect("started");
        assert_eq!(
            state.corner,
            crate::layout::Corner::Diagnostics(crate::lsp::Severity::Error)
        );
    }

    #[test]
    fn the_conflict_list_is_back_in_the_corner_after_a_restart() {
        let (state, _, _) = start(&Startup {
            state_json: Some(r#"{"corner": "Conflicts"}"#.to_string()),
            ..Startup::default()
        })
        .expect("started");
        assert_eq!(state.corner, crate::layout::Corner::Conflicts);
    }

    fn refusal(project_config: &str) -> ConfigError {
        let error = start(&Startup {
            project_config: Some(project_config.to_string()),
            ..Startup::default()
        })
        .expect_err("Varde started on a config it cannot use");
        match error {
            StartupError::Config(problem) => problem,
            other => panic!("expected a config fault, got {other:?}"),
        }
    }

    #[test]
    fn a_command_that_is_not_a_string_refuses_to_start() {
        let problem = refusal("[lsp.rust]\ncommand = 12\n");
        assert_eq!((problem.file.as_str(), problem.line), (PROJECT_LABEL, 2));
        assert!(
            matches!(problem.fault, ConfigFault::WrongType(_)),
            "{problem}"
        );
    }

    #[test]
    fn an_argument_that_is_not_a_string_refuses_to_start() {
        let problem = refusal("[lsp.rust]\ncommand = \"rust-analyzer\"\nargs = [\"--stdio\", 3]\n");
        assert_eq!((problem.file.as_str(), problem.line), (PROJECT_LABEL, 3));
        assert!(
            matches!(problem.fault, ConfigFault::WrongType(_)),
            "{problem}"
        );
    }

    #[test]
    fn the_dependency_table_is_every_shipped_row_with_its_install_for_the_os() {
        let rustup = |c: &str| Some(format!("rustup component add {c}"));
        let npm = |p: &str| Some(format!("npm install -g {p}"));
        let brew = |p: &str| Some(format!("brew install {p}"));
        let apt = Some("sudo apt install clangd".to_string());
        let gopls = Some("go install golang.org/x/tools/gopls@latest".to_string());
        let ts = "typescript@6 typescript-language-server";
        let pinned = [
            ("lsp", "c", "clangd", None, apt.clone()),
            ("lsp", "cpp", "clangd", None, apt),
            ("lsp", "go", "gopls", gopls.clone(), gopls),
            ("lsp", "java", "jdtls", brew("jdtls"), None),
            (
                "lsp",
                "javascript",
                "typescript-language-server",
                npm(ts),
                npm(ts),
            ),
            (
                "lsp",
                "python",
                "pyright-langserver",
                npm("pyright"),
                npm("pyright"),
            ),
            (
                "lsp",
                "rust",
                "rust-analyzer",
                rustup("rust-analyzer"),
                rustup("rust-analyzer"),
            ),
            (
                "lsp",
                "typescript",
                "typescript-language-server",
                npm(ts),
                npm(ts),
            ),
            (
                "lsp",
                "vue",
                "vue-language-server",
                npm("@vue/language-server"),
                npm("@vue/language-server"),
            ),
            ("lsp", "zig", "zls", brew("zls"), None),
            (
                "formatter",
                "css",
                "prettier",
                npm("prettier"),
                npm("prettier"),
            ),
            ("formatter", "go", "gofmt", None, None),
            (
                "formatter",
                "html",
                "prettier",
                npm("prettier"),
                npm("prettier"),
            ),
            (
                "formatter",
                "javascript",
                "prettier",
                npm("prettier"),
                npm("prettier"),
            ),
            (
                "formatter",
                "json",
                "prettier",
                npm("prettier"),
                npm("prettier"),
            ),
            (
                "formatter",
                "markdown",
                "prettier",
                npm("prettier"),
                npm("prettier"),
            ),
            (
                "formatter",
                "python",
                "black",
                Some("pipx install black".into()),
                Some("pipx install black".into()),
            ),
            (
                "formatter",
                "rust",
                "rustfmt",
                rustup("rustfmt"),
                rustup("rustfmt"),
            ),
            (
                "formatter",
                "typescript",
                "prettier",
                npm("prettier"),
                npm("prettier"),
            ),
            (
                "formatter",
                "vue",
                "prettier",
                npm("prettier"),
                npm("prettier"),
            ),
            (
                "formatter",
                "yaml",
                "prettier",
                npm("prettier"),
                npm("prettier"),
            ),
        ];
        for (os, player) in [("macos", "afplay"), ("linux", "aplay")] {
            let rows = deps(None, os).expect("the template parses");
            let (table, speech) = rows.split_last_chunk::<2>().expect("the speech rows");
            let expected: Vec<Dep> = pinned
                .iter()
                .map(|(kind, name, command, macos, linux)| Dep {
                    kind,
                    name: name.to_string(),
                    command: command.to_string(),
                    install: if os == "macos" { macos } else { linux }.clone(),
                })
                .collect();
            let config = fresh();
            assert_eq!(
                table.len(),
                config.servers().len() + config.formatters().len() + config.adapters().len(),
                "{os}"
            );
            let codelldb = table
                .iter()
                .find(|dep| (dep.kind, dep.name.as_str()) == ("dap", "rust"))
                .expect("the rust adapter");
            assert_eq!(codelldb.command, "codelldb");
            let java = table
                .iter()
                .find(|dep| (dep.kind, dep.name.as_str()) == ("dap", "java"))
                .expect("the java adapter");
            assert_eq!(java.command, "jdtls");
            assert!(
                codelldb
                    .install
                    .as_deref()
                    .is_some_and(|install| install.contains(&format!(
                        "codelldb-{}-",
                        if os == "macos" { "darwin" } else { os }
                    ))),
                "{os}: {codelldb:?}"
            );
            for dep in expected {
                assert!(table.contains(&dep), "{os}: {dep:?}");
            }
            let [synth, play] = speech;
            assert_eq!(
                (synth.kind, synth.name.as_str(), synth.command.as_str()),
                ("speech", "speech", "piper")
            );
            let template: toml::Table = PROGRAMS.parse().expect("the template parses");
            assert_eq!(
                template["speech"]["configures"]["voice"].as_str(),
                Some("~/.varde/voices/en_US-bryce-medium.onnx")
            );
            assert!(synth
                .install
                .as_deref()
                .is_some_and(|line| line.starts_with("uv tool install piper-tts")
                    && line.contains("--output-dir ~/.varde/voices")
                    && line.contains("/en_US-bryce-medium.onnx ")));
            assert_eq!(
                play,
                &Dep {
                    kind: "player",
                    name: "speech".into(),
                    command: player.into(),
                    install: None
                }
            );
        }
    }

    #[test]
    fn deps_lists_the_rows_the_global_config_names() {
        let file = "[lsp.ruby]\ncommand = \"ruby-lsp\"\nextensions = [\"rb\"]\n";
        assert_eq!(
            deps(Some(file), "linux").expect("a usable file"),
            [Dep {
                kind: "lsp",
                name: "ruby".into(),
                command: "ruby-lsp".into(),
                install: None,
            }]
        );
        let broken = deps(Some("[lsp.ruby]\ncommand = 12\n"), "linux").expect_err("refused");
        assert_eq!((broken.file.as_str(), broken.line), (GLOBAL_LABEL, 2));
    }

    #[test]
    fn the_shipped_rows_serve_every_file_the_match_they_replaced_served() {
        let mut state = crate::State {
            servers: fresh().servers(),
            formatters: fresh().formatters(),
            ..crate::State::default()
        };
        let served = [
            ("rust", "rs"),
            ("typescript", "ts tsx mts cts"),
            ("javascript", "js jsx mjs cjs"),
            ("vue", "vue"),
            ("java", "java"),
            ("zig", "zig"),
            ("python", "py pyi"),
            ("go", "go"),
            ("c", "c h"),
            ("cpp", "cpp cc cxx hpp hh hxx"),
        ];
        for (language, extensions) in served {
            for extension in extensions.split(' ') {
                let path = std::path::PathBuf::from(format!("/w/a.{extension}"));
                assert_eq!(
                    crate::lsp::language(&state, &path),
                    Some(language),
                    "{path:?}"
                );
                if state.formatters.contains_key(language) {
                    state.current_buffer = Some(path.clone());
                    state
                        .buffers
                        .insert(path.clone(), crate::editor::Buffer::open("", false, 4));
                    assert!(
                        matches!(
                            crate::format::run(&state).as_slice(),
                            [Effect::RunFormatter { language: formatted, .. }] if formatted == language
                        ),
                        "{path:?} is not laid out by [formatter.{language}]"
                    );
                }
            }
        }
    }

    #[test]
    fn every_shipped_row_claims_an_extension() {
        let config = fresh();
        for (language, server) in config.servers() {
            assert!(!server.extensions.is_empty(), "[lsp.{language}]");
        }
        for (language, formatter) in config.formatters() {
            assert!(!formatter.extensions.is_empty(), "[formatter.{language}]");
        }
    }

    #[test]
    fn two_rows_claiming_one_extension_refuse_to_start_naming_both() {
        let problem = refusal("\n[lsp.rustier]\ncommand = \"r\"\nextensions = [\"rs\"]\n");
        assert_eq!((problem.file.as_str(), problem.line), (PROJECT_LABEL, 2));
        assert_eq!(
            problem.to_string(),
            ".varde/config.toml:2: [lsp.rust] and [lsp.rustier] both claim .rs"
        );
    }

    #[test]
    fn a_row_naming_its_own_extension_twice_claims_it_once() {
        start(&Startup {
            project_config: Some("[lsp.rust]\nextensions = [\"rs\", \"rs\"]\n".to_string()),
            ..Startup::default()
        })
        .expect("Varde started");
    }

    #[test]
    fn every_template_row_parses_into_a_valid_row() {
        let config = fresh();
        let named = |kind: &str| config.0[kind].as_table().expect(kind).len();
        assert_eq!(config.servers().len(), named("lsp"));
        assert_eq!(config.formatters().len(), named("formatter"));
        assert_eq!(config.facts().len(), named("facts"));
    }

    #[test]
    fn no_two_template_servers_claim_one_extension() {
        let template: toml::Table = PROGRAMS.parse().expect("the template parses");
        let mut owners = std::collections::BTreeMap::new();
        for (language, row) in template["lsp"].as_table().expect("lsp tables") {
            for extension in row["extensions"].as_array().expect("extensions") {
                let extension = extension.as_str().expect("a string");
                if let Some(owner) = owners.insert(extension, language) {
                    assert_eq!(owner, language, ".{extension} is claimed twice");
                }
            }
        }
    }

    #[test]
    fn an_install_command_that_is_not_a_string_refuses_to_start() {
        let problem = refusal("[lsp.zig]\ncommand = \"zls\"\ninstall.macos = 12\n");
        assert_eq!((problem.file.as_str(), problem.line), (PROJECT_LABEL, 3));
        assert!(
            matches!(problem.fault, ConfigFault::WrongType(_)),
            "{problem}"
        );
    }

    #[test]
    fn every_default_install_command_is_keyed_by_an_os_that_can_run_varde() {
        for (language, server) in fresh().servers() {
            for os in server.install.keys() {
                assert!(
                    ["macos", "linux", "windows"].contains(&os.as_str()),
                    "{language} names an install command for {os:?}"
                );
            }
        }
    }

    #[test]
    fn a_language_nobody_has_packaged_for_an_os_has_no_key_for_it() {
        let servers = fresh().servers();
        for language in ["zig", "java"] {
            assert_eq!(
                servers[language].install.keys().collect::<Vec<_>>(),
                vec!["macos"],
                "{language} claims a command somewhere it is not packaged"
            );
        }
    }

    #[test]
    fn a_layer_may_name_one_key_of_a_shipped_server() {
        let (state, _config, _effects) = start(&Startup {
            project_config: Some("[lsp.vue]\nargs = [\"--from-project\"]\n".to_string()),
            ..Startup::default()
        })
        .expect("Varde started");
        let patched = &state.servers["vue"];
        let shipped = &fresh().servers()["vue"];
        assert_eq!(patched.args, ["--from-project"]);
        assert_eq!(
            (
                &patched.command,
                &patched.also_served_by,
                &patched.install,
                &patched.unanswerable
            ),
            (
                &shipped.command,
                &shipped.also_served_by,
                &shipped.install,
                &shipped.unanswerable
            )
        );
    }

    #[test]
    fn a_layer_may_name_one_key_of_a_shipped_fact() {
        let (state, _config, _effects) = start(&Startup {
            project_config: Some("[facts.typescript_sdk]\nvalue = \"marker\"\n".to_string()),
            ..Startup::default()
        })
        .expect("Varde started");
        let patched = &state.facts["typescript_sdk"];
        assert_eq!(patched.value, FactValue::Marker);
        assert_eq!(patched.marker, fresh().facts()["typescript_sdk"].marker);
    }

    #[test]
    fn a_fact_no_layer_gave_a_marker_refuses_to_start() {
        let problem = refusal("[facts.python_env]\nvalue = \"directory\"\n");
        assert_eq!((problem.file.as_str(), problem.line), (PROJECT_LABEL, 1));
        assert_eq!(
            problem.fault,
            ConfigFault::Incomplete {
                entry: "facts.python_env".to_string(),
                key: "marker".to_string()
            }
        );
    }

    #[test]
    fn the_three_faults_read_differently() {
        let messages = [
            "[lsp.rust]\ncommand = \"rust-analyzer\n",
            "[lsp.rust]\ncommand = 12\n",
            "[lsp.brainfuck]\nargs = [\"--stdio\"]\n",
        ]
        .map(|source| refusal(source).to_string());
        assert_eq!(
            messages,
            [
                ".varde/config.toml:2: config is not valid TOML",
                ".varde/config.toml:2: invalid type: integer `12`, expected a string",
                ".varde/config.toml:1: [lsp.brainfuck] names no command",
            ]
        );
    }

    #[test]
    fn a_config_overrides_one_install_command_and_leaves_its_siblings() {
        let (state, _config, _effects) = start(&Startup {
            project_config: Some(
                "[lsp.rust]\ncommand = \"rust-analyzer\"\ninstall.macos = \"my-own-installer\"\n"
                    .to_string(),
            ),
            ..Startup::default()
        })
        .expect("Varde started");
        let install = &state.servers["rust"].install;
        assert_eq!(install["macos"], "my-own-installer");
        assert_eq!(install["linux"], "rustup component add rust-analyzer");
    }

    #[test]
    fn every_name_the_defaults_interpolate_is_one_the_defaults_declare() {
        let config = fresh();
        let declared = config.facts();
        let mut seen = 0;
        for (language, server) in config.servers() {
            let options = server
                .initialization_options
                .map(|options| serde_json::Value::Object(options).to_string())
                .unwrap_or_default();
            for text in server.args.iter().chain(std::iter::once(&options)) {
                for asked in interpolated(text) {
                    seen += 1;
                    assert!(
                        declared.contains_key(&asked),
                        "{language} asks for ${{{asked}}}, which nothing declares"
                    );
                }
            }
        }
        assert!(seen >= 3, "only {seen} interpolated names found");
    }

    fn interpolated(text: &str) -> Vec<String> {
        text.split("${")
            .skip(1)
            .filter_map(|rest| rest.split_once('}'))
            .map(|(name, _)| name.to_string())
            .collect()
    }

    #[test]
    fn the_shipped_fact_names_a_marker_and_the_directory_holding_it() {
        let facts = fresh().facts();
        let sdk = &facts["typescript_sdk"];
        assert_eq!(sdk.marker, "node_modules/typescript/lib/typescript.js");
        assert_eq!(sdk.value, FactValue::Directory);
        assert_eq!(sdk.command.as_deref(), Some("tsc"));
    }

    #[test]
    fn a_newer_version_in_any_component_is_an_update() {
        assert!(is_update("0.1.1", "0.1.0"));
        assert!(is_update("0.2.0", "0.1.0"));
        assert!(is_update("1.0.0", "0.1.0"));
    }

    #[test]
    fn the_defaults_are_the_ones_the_library_documents() {
        let table: toml::Table = DEFAULTS.parse().expect("valid TOML");
        assert_eq!(
            table["risk"]["threshold"].as_integer(),
            Some(i64::from(crate::risk::DEFAULT_THRESHOLD))
        );
        assert_eq!(
            table["risk"]["max_iterations"].as_integer(),
            Some(i64::from(crate::risk::DEFAULT_MAX_ITERATIONS))
        );
        assert_eq!(
            table["editor"]["tab_width"].as_integer(),
            Some(crate::editor::DEFAULT_TAB_WIDTH as i64)
        );
    }

    #[test]
    fn a_run_row_whose_query_cannot_mark_anything_is_refused_at_its_line() {
        let claims_nothing = merged_config(Some("[run.bare]\nquery = \"(x) @run\"\n"), None)
            .expect_err("a row claiming no files");
        assert_eq!(
            claims_nothing.fault,
            ConfigFault::WrongType("[run.bare] claims no extensions".to_string())
        );
        let refused = merged_config(
            Some("[view]\n\n[run.zig]\nextensions = [\"zig\"]\nquery = \"(test_declaration) @run\"\n"),
            None,
        )
        .expect_err("a row no grammar can parse");
        assert_eq!(
            refused,
            ConfigError {
                file: GLOBAL_LABEL.to_string(),
                line: 3,
                fault: ConfigFault::WrongType("[run.zig] no grammar parses .zig".to_string()),
            }
        );
    }

    #[test]
    fn a_config_that_is_there_but_says_nothing_is_not_seeded_over() {
        let (_state, _config, effects) = start(&Startup {
            global_config: Some(String::new()),
            project_config: Some(String::new()),
            ..Startup::default()
        })
        .expect("Varde started");
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::WriteFile { path, .. } if path.ends_with(super::CONFIG_FILE))),
            "starting wrote over a config file that is already there: {effects:?}"
        );
    }

    #[test]
    fn the_seeded_config_is_commented_out_and_quotes_the_live_defaults() {
        let seeded: toml::Table = SEEDED_CONFIG.parse().expect("valid TOML");
        assert!(
            seeded
                .values()
                .all(|table| table.as_table().is_some_and(toml::Table::is_empty)),
            "a live key in the seeded config: {seeded:?}"
        );

        let uncommented = uncommented(SEEDED_CONFIG);
        let mut defaults = fresh().0;
        assert!(
            defaults.remove("ai").is_some() && !uncommented.contains_key("ai"),
            "a project's config names [ai], which a project cannot set"
        );
        assert!(
            !uncommented.is_empty()
                && uncommented
                    .values()
                    .all(|table| !table.as_table().is_some_and(toml::Table::is_empty)),
            "the seeded config names nothing: {uncommented:?}"
        );
        for (table, keys) in &uncommented {
            for (key, value) in keys.as_table().expect("a table") {
                assert_eq!(
                    Some(value),
                    defaults[table].get(key),
                    "the seeded {table}.{key} is not what the defaults say"
                );
            }
        }
        for (table, keys) in &defaults {
            for (key, value) in keys.as_table().expect("a table") {
                assert!(
                    value.is_table()
                        || uncommented
                            .get(table)
                            .and_then(|named| named.get(key))
                            .is_some(),
                    "the defaults spell {table}.{key} and the seeded config never names it"
                );
            }
        }
    }

    #[test]
    fn the_template_has_live_program_rows_and_commented_settings() {
        let mut live: toml::Table = template().parse().expect("valid TOML");
        let programs: toml::Table = PROGRAMS.parse().expect("valid TOML");
        let settings: toml::Table = DEFAULTS.parse().expect("valid TOML");
        fn blank(value: &toml::Value) -> bool {
            value
                .as_table()
                .is_some_and(|table| table.values().all(blank))
        }
        for (table, keys) in &settings {
            for key in keys.as_table().expect("a table").keys() {
                assert!(
                    live[table].get(key).is_none_or(blank),
                    "a live Setting in the template: {table}.{key}"
                );
            }
        }
        assert_eq!(uncommented(&template()), fresh().0);
        live.retain(|table, keys| programs.contains_key(table) || !blank(keys));
        assert_eq!(live, programs);
    }

    #[test]
    fn a_double_digit_component_is_compared_as_a_number() {
        assert!(is_update("0.10.0", "0.9.0"));
        assert!(!is_update("0.9.0", "0.10.0"));
    }

    #[test]
    fn asset_names_match_the_release_workflow() {
        assert_eq!(asset_name("macos", "aarch64"), "varde-macos-aarch64");
        assert_eq!(asset_name("macos", "x86_64"), "varde-macos-x86_64");
        assert_eq!(asset_name("linux", "x86_64"), "varde-linux-x86_64");
        assert_eq!(asset_name("linux", "aarch64"), "varde-linux-aarch64");
    }

    #[test]
    fn a_download_matching_its_line_is_verified() {
        let list = "\
2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae  varde-linux-x86_64
0000000000000000000000000000000000000000000000000000000000000000  varde-linux-aarch64
";
        let url = "https://example.test/download/v0.2.0/varde-linux-x86_64";
        assert_eq!(verify(list, url, b"foo"), Ok(()));
        assert_eq!(verify(list, url, b"bar"), Err(ReplaceFailed::Checksum));
    }

    #[test]
    fn a_list_without_the_assets_line_names_no_asset() {
        let list = "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae  old-varde-linux-x86_64\n";
        assert_eq!(
            verify(list, "https://example.test/varde-linux-x86_64", b"foo"),
            Err(ReplaceFailed::NoAsset)
        );
    }

    #[test]
    fn a_release_without_a_checksum_list_is_no_release() {
        let body = r#"{"tag_name": "v0.2.0", "assets": [
            {"name": "varde-linux-x86_64", "browser_download_url": "https://example.test/a"}
        ]}"#;
        assert_eq!(release(body, "linux", "x86_64", "0.1.0"), None);
    }

    #[test]
    fn an_equal_version_is_not_an_update() {
        assert!(!is_update("0.1.0", "0.1.0"));
    }

    #[test]
    fn a_version_behind_the_binary_is_not_an_update() {
        assert!(!is_update("0.0.9", "0.1.0"));
        assert!(!is_update("1.0.0", "2.0.0"));
    }

    #[test]
    fn a_pre_release_qualifier_orders_below_its_release() {
        assert!(is_update("0.2.0-rc.1", "0.1.0"));
        assert!(!is_update("0.2.0-rc.1", "0.2.0"));
        assert!(is_update("0.2.0", "0.2.0-rc.1"));
    }

    #[test]
    fn a_version_that_is_not_a_version_is_not_an_update() {
        assert!(!is_update("nightly", "0.1.0"));
        assert!(!is_update("0.2.0", ""));
    }
}
