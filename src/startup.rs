//! F1 and F9 — starting on a folder, and the configuration behind it.
//!
//! Reads nothing. The edge loads the files and passes their contents in; this
//! module decides what they mean and what should exist.

use crate::risk::{self, Scope};
use crate::{Effect, ReplaceFailed, State, View};
use sha2::Digest;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use toml::Table;

/// The Settings: numbers Varde cannot work without, built in and beaten key by
/// key by `~/.varde/config.toml` and that by the project's own
/// (`docs/adr/0018-the-global-config-is-the-list-of-programs.md`).
/// `risk.threshold` is `risk::DEFAULT_THRESHOLD` and `editor.tab_width` is
/// `editor::DEFAULT_TAB_WIDTH`, spelled as TOML — a test below holds each pair
/// level, since a default that disagrees with itself is a figure nobody can
/// predict.
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
debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["platform shell cargo build --message-format=json | sed -n 's/.*\"executable\":\"\\([^\"]*\\)\".*/target create \\1/p' | head -1 > target/varde-debug.lldb", "command source target/varde-debug.lldb"] } }

# The first test binary cargo builds, which is the crate's own tests; a test
# under `tests/` is in a binary of its own.
[run.rust_test]
extensions = ["rs"]
query = '((attribute_item (attribute (identifier) @attribute)) . (function_item name: (identifier) @name @run) (#eq? @attribute "test"))'
run = "cargo test ${name} -- --exact"
debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["platform shell cargo test --no-run --message-format=json | sed -n 's/.*\"executable\":\"\\([^\"]*\\)\".*/target create \\1/p' | head -1 > target/varde-debug.lldb", "command source target/varde-debug.lldb"], args = ["${name}", "--exact"] } }

# The same test inside a module, which `--exact` names by its path. Rows are
# tried by name, so this one takes the line before `rust_test` can.
[run.rust_module_test]
extensions = ["rs"]
query = '((mod_item name: (identifier) @module body: (declaration_list (attribute_item (attribute (identifier) @attribute)) . (function_item name: (identifier) @name @run))) (#eq? @attribute "test"))'
run = "cargo test ${module}::${name} -- --exact"
debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["platform shell cargo test --no-run --message-format=json | sed -n 's/.*\"executable\":\"\\([^\"]*\\)\".*/target create \\1/p' | head -1 > target/varde-debug.lldb", "command source target/varde-debug.lldb"], args = ["${module}::${name}", "--exact"] } }

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

/// The Program rows the binary carries: the rows [`template`] seeds
/// `~/.varde/config.toml` with, live. They are not a layer of the merge: a row
/// no config file names does not run (ADR 0018).
///
/// The `[lsp.*]` tables are the only place in the library a language server is
/// named, and they are data rather than a branch on purpose: once in the file
/// they are the reader's to edit, and the project's own config beats them key
/// by key. A match arm spelling the same strings could only be beaten by a fork —
/// `docs/adr/0011-a-language-server-is-a-second-hosted-child.md` argues why
/// where the name lives is the whole of the distinction.
///
/// The `install` keys are the same kind of data for the same reasons, one per
/// operating system, and this is the only place in the library a package
/// manager is named at all
/// (`docs/adr/0012-an-install-command-is-configuration.md`). A language nobody has packaged
/// for an OS gets **no key** — `zls` on Linux is a build from source and `jdtls`
/// is in no distribution — because an invented command that fails looks
/// configured, while a blank one is fixable in one line of TOML.
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
[dap.rust]
command = "codelldb"
install.macos = "curl -sL --create-dirs https://github.com/vadimcn/codelldb/releases/latest/download/codelldb-darwin-$(uname -m | sed 's/x86_64/x64/').vsix -o ~/.varde/codelldb.vsix && unzip -qo ~/.varde/codelldb.vsix -d ~/.varde/codelldb && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec %s \"$@\"\\n' ~/.varde/codelldb/extension/adapter/codelldb > ~/.local/bin/codelldb && chmod +x ~/.local/bin/codelldb"
install.linux = "curl -sL --create-dirs https://github.com/vadimcn/codelldb/releases/latest/download/codelldb-linux-$(uname -m | sed 's/x86_64/x64/;s/aarch64/arm64/').vsix -o ~/.varde/codelldb.vsix && unzip -qo ~/.varde/codelldb.vsix -d ~/.varde/codelldb && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec %s \"$@\"\\n' ~/.varde/codelldb/extension/adapter/codelldb > ~/.local/bin/codelldb && chmod +x ~/.local/bin/codelldb"

# java-debug lives inside jdtls, where the classpath that maps a file and line to
# a class is. `plugin` is merged into the `[lsp.java]` server's
# `initializationOptions` when it starts, and `command` is what that server is
# sent once a session begins; it answers with the port the adapter listens on.
[dap.java]
server = "java"
command = "vscode.java.startDebugSession"
plugin = { bundles = ["${java_debug_plugin}"] }

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
install.macos = "curl -sL --create-dirs $(curl -s https://api.github.com/repos/microsoft/vscode-js-debug/releases/latest | grep -o 'https://[^\"]*js-debug-dap-v[^\"]*[.]tar[.]gz' | head -1) -o ~/.varde/js-debug.tar.gz && tar xzf ~/.varde/js-debug.tar.gz -C ~/.varde && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec node %s \"$@\"\\n' ~/.varde/js-debug/src/dapDebugServer.js > ~/.local/bin/js-debug-adapter && chmod +x ~/.local/bin/js-debug-adapter"
install.linux = "curl -sL --create-dirs $(curl -s https://api.github.com/repos/microsoft/vscode-js-debug/releases/latest | grep -o 'https://[^\"]*js-debug-dap-v[^\"]*[.]tar[.]gz' | head -1) -o ~/.varde/js-debug.tar.gz && tar xzf ~/.varde/js-debug.tar.gz -C ~/.varde && mkdir -p ~/.local/bin && printf '#!/bin/sh\\nexec node %s \"$@\"\\n' ~/.varde/js-debug/src/dapDebugServer.js > ~/.local/bin/js-debug-adapter && chmod +x ~/.local/bin/js-debug-adapter"

[dap.typescript]
command = "js-debug-adapter"
args = ["${port}"]
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

/// What starting lays down at `<project>/.varde/config.toml` the first time,
/// and only when nothing is there (Q38) — and what `install.sh` lays down at
/// `~/.varde/config.toml` the same way, asked of `varde --default-config`, so
/// both files hold one text and the test below holds both. A key nobody can find is a key nobody
/// sets: `editor.tab_width` was layered, merged and read on every start for its
/// whole life while no `.varde/config.toml` existed anywhere to name it.
///
/// **Every key is commented out**, and that is the whole design. A seeded file
/// holding live values would make "the project sets nothing" false — the merge
/// would see a project layer on its first run — and it would freeze *this*
/// binary's numbers into a file that outlives it, so a later correction to
/// [`DEFAULTS`] would arrive and change nothing, which is the same trap
/// [`DEFAULTS`]'s own doc argues the install commands out of. The trap is only
/// half sprung by the comment: a reader who uncomments a line that has since
/// gone stale pins the old number by hand, so a test below uncomments every key
/// here and holds each one against the [`DEFAULTS`] layer.
///
/// That test is also why this quotes only keys [`DEFAULTS`] spells. A setting
/// whose default lives in Rust alone — `editor.theme`, `ai.command` — has no
/// text to be held level with, and a number written here with nothing holding
/// it is exactly the frozen answer the comments exist to prevent.
///
/// The table headers are live where the keys under them are not, because a
/// reader who uncomments `tab_width` alone under a commented `[editor]` sets a
/// top-level key that nothing reads. An empty table merges nothing, so they
/// cost the effective config exactly what the comments do.
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

/// The Settings half of [`template`], commented out for the reason
/// [`SEEDED_CONFIG`]'s are. `speech.speed` is not here but commented out inside
/// [`PROGRAMS`]'s `[speech]` row, since TOML allows that table only once.
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
# debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["platform shell cargo build --message-format=json | sed -n 's/.*\"executable\":\"\\([^\"]*\\)\".*/target create \\1/p' | head -1 > target/varde-debug.lldb", "command source target/varde-debug.lldb"] } }

[run.rust_test]
# extensions = ["rs"]
# query = '((attribute_item (attribute (identifier) @attribute)) . (function_item name: (identifier) @name @run) (#eq? @attribute "test"))'
# run = "cargo test ${name} -- --exact"
# debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["platform shell cargo test --no-run --message-format=json | sed -n 's/.*\"executable\":\"\\([^\"]*\\)\".*/target create \\1/p' | head -1 > target/varde-debug.lldb", "command source target/varde-debug.lldb"], args = ["${name}", "--exact"] } }

[run.rust_module_test]
# extensions = ["rs"]
# query = '((mod_item name: (identifier) @module body: (declaration_list (attribute_item (attribute (identifier) @attribute)) . (function_item name: (identifier) @name @run))) (#eq? @attribute "test"))'
# run = "cargo test ${module}::${name} -- --exact"
# debug = { adapter = "rust", request = "launch", args = { targetCreateCommands = ["platform shell cargo test --no-run --message-format=json | sed -n 's/.*\"executable\":\"\\([^\"]*\\)\".*/target create \\1/p' | head -1 > target/varde-debug.lldb", "command source target/varde-debug.lldb"], args = ["${module}::${name}", "--exact"] } }

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

/// What `~/.varde/config.toml` starts as, when Varde starts and the edge read
/// none, and what `varde --default-config` prints for `install.sh` to lay down
/// the same way: every Setting commented out, every Program row live (ADR 0018).
pub fn template() -> String {
    [TEMPLATE_SETTINGS, PROGRAMS].concat()
}

pub const GLOBAL_LABEL: &str = "~/.varde/config.toml";
pub const PROJECT_LABEL: &str = ".varde/config.toml";

/// The file both layers are read from: the global one under `~/.varde`, and
/// the project's own under [`crate::varde_dir`].
pub const CONFIG_FILE: &str = "config.toml";

/// Why Varde refused to start. Precise enough to fix the file in another
/// editor, which matters because a broken global config locks the user out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub file: String,
    pub line: usize,
    pub fault: ConfigFault,
}

/// What is wrong with a layer, because the three faults send the reader to
/// three different places. The edge printed one sentence for all of them, so a
/// deserialize fault about a missing key wore the parse fault's words and sent
/// whoever read it hunting a syntax error that was not there — the same failure
/// as a notice blaming a server for Varde's own refusal, one layer down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFault {
    /// The text is not TOML at all.
    NotToml,
    /// TOML, but a value is not the shape its key must be (R9.5) — including
    /// half of a pair that is one fact, such as an `unanswerable` naming a
    /// request and no response. The `toml` crate's own words, which name the
    /// type found and the type wanted; the line points at the key.
    WrongType(String),
    /// An entry that parsed, typed, and is still unusable: no layer ever gave
    /// it the one key it cannot be used without. Found after the merge,
    /// because a layer is a patch and completeness is not a patch's to satisfy.
    Incomplete { entry: String, key: String },
    /// Two `[lsp.*]` rows claiming one extension, named in the order the
    /// merged table holds them. Found after the merge for the same reason.
    ClaimedTwice {
        extension: String,
        rows: [String; 2],
    },
    /// The file is there and the edge could not read it. Never written over:
    /// what cannot be read cannot be kept.
    Unreadable,
}

impl std::fmt::Display for ConfigError {
    /// The words, here rather than at the edge: a reason the edge has to
    /// supply is a reason no test can read, which is how one sentence came to
    /// stand for three faults.
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

/// What the edge found at the path it was asked to open.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PathStatus {
    #[default]
    Folder,
    Missing,
    NotAFolder,
    Unreadable,
}

/// Everything the edge read before starting.
#[derive(Debug, Default)]
pub struct Startup {
    pub root: PathBuf,
    /// The Sidecar, if the edge was given no folder to open: `varde` with no
    /// argument is a Bare workspace, `varde <folder>` is a project. The edge
    /// reports which it was by handing one or none — it never interprets argv,
    /// and `varde` and `varde .` name the same folder
    /// (`docs/adr/0016-a-bare-workspace-leaves-nothing-behind.md`).
    pub sidecar: Option<PathBuf>,
    /// `~/.varde`, the user's own directory — where a submitted review goes
    /// when the workspace has nowhere durable to keep it (ADR 0016). Read at
    /// the edge like the Sidecar is, for the same reason: a home directory is
    /// not the library's to observe.
    pub varde_home: PathBuf,
    /// The reviews already kept, by number, as the edge found them in
    /// [`crate::reviews_dir`]. Read from the directory rather than remembered
    /// in `state.json`: a Bare workspace has no state to remember it in, and
    /// the one directory is shared by every one of them.
    pub reviews: BTreeSet<u32>,
    pub path_status: PathStatus,
    pub global_config: Option<String>,
    pub project_config: Option<String>,
    pub state_json: Option<String>,
    /// `.varde/risk.json` as the edge found it, and what `HEAD` resolves to.
    /// Whether the cached figure still describes the workspace is decided here,
    /// not there.
    pub risk_json: Option<String>,
    pub head: Option<String>,
    /// git's answer, as `State::repo` holds it. Handed in rather than told
    /// after starting, because a session restored into Review view lands on
    /// the first changed file, and without it there is none to land on.
    pub repo: Option<Vec<crate::review::GitFile>>,
    /// The directory above the running binary, if the edge found one. Whether it
    /// is Varde's own checkout is decided here, not there.
    pub checkout: Option<PathBuf>,
    pub checkout_manifest: Option<String>,
    /// What this binary was compiled from — the one version a running Varde
    /// knows for certain.
    pub running_version: String,
    /// Which operating system this binary was built for, as
    /// `std::env::consts::OS` spells it. Handed in rather than read here for
    /// the reason `running_version` is: a value handed in is a value a scenario
    /// can set, and "the Linux row offers the Linux command" is otherwise
    /// unspecifiable on a Mac (R31.22).
    pub os: String,
    /// And the CPU, as `std::env::consts::ARCH` spells it, for the same reason.
    pub arch: String,
}

/// This repository's latest Release — the one place the release host is named
/// (ADR 0017). Unauthenticated: one request per launch is far inside the
/// anonymous limit.
pub const RELEASE_URL: &str = "https://api.github.com/repos/oyvij/varde-editor/releases/latest";

/// A published Version newer than the Running version, and the two URLs
/// `:update` fetches: this platform's Asset and the checksum list.
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

/// The file in a Release built for this platform. The release workflow's matrix
/// spells the same names (`.github/workflows/release.yml`).
fn asset_name(os: &str, arch: &str) -> String {
    format!("varde-{os}-{arch}")
}

/// The Release a latest-release body describes, if it is an Update this
/// platform can install. Anything short of that — a body that is not the JSON,
/// a tag that is not a newer Version, no Asset or no checksum list to verify it
/// against — is no Release, and silently so.
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

/// Whether a downloaded Asset is the file the checksum list names. The Asset's
/// name is the last segment of its download URL, and its line is the one
/// `sha256sum` wrote for exactly that name.
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

/// What runs a language's server, as configuration named it. Nothing here is a
/// claim that a process exists: that is the edge's to observe, never the core's
/// to remember.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Server {
    /// Defaulted, because a layer is a patch: naming one key of a language
    /// `DEFAULTS` ships must not mean repeating a command the reader would have
    /// to copy out of a binary's built-in defaults and which then silently
    /// stops tracking them. Empty means no layer named one, which
    /// `refuse_incomplete` refuses before any `Server` reaches `State`.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Which *other* languages' servers also serve this language's files. A
    /// `.vue` file is served by the Vue server and by a TypeScript server, a
    /// linter server sits beside a type server, and every arrangement other
    /// editors reach by attaching several clients to one buffer is this key —
    /// so which servers serve a path is data, and no arm anywhere names a
    /// language (R31.1, ADR 0011).
    ///
    /// Named from the file's own side rather than the serving server's: a
    /// `.vue` file is what needs two answers, and resolving them is then one
    /// lookup in the table the path already found rather than a scan over
    /// every configured server asking whether it fancies this extension.
    #[serde(default)]
    pub also_served_by: Vec<String>,
    /// The file extensions this row owns, and the only way a file finds its
    /// server: the table name is then the language id the protocol is sent, so
    /// a language Varde has never heard of is a row and no release (ADR 0018).
    /// Two rows claiming one extension is refused at start rather than settled
    /// by the order the merge happened to leave them in.
    #[serde(default)]
    pub extensions: Vec<String>,
    /// What installs this server, keyed by the OS the binary was built for.
    /// Typed like the rest, so `install.macos = 12` faults with a file and a
    /// line rather than being dropped (R9.5), and a map rather than three
    /// fields because which key applies is a lookup by the string `Startup`
    /// carried in — never a branch on the OS.
    #[serde(default)]
    pub install: BTreeMap<String, String>,
    /// What the server is told about its own world in the `initializationOptions`
    /// of `initialize` — where its toolchain lives, most often, which several
    /// servers will not run without. An arbitrary table, and that is the point:
    /// Varde inspects none of it, so a requirement nobody here anticipated is a
    /// row in a file rather than a release
    /// (`docs/adr/0012-an-install-command-is-configuration.md`, R31.26).
    ///
    /// Deserialized straight into the shape the message wants rather than into
    /// a `toml::Table` converted later: serde does not care which format a
    /// value came from, so there is nothing to convert and nothing that can
    /// fail on the way out — a re-serialise into JSON has one failure mode
    /// (TOML has `nan`, JSON has no number for it) and it would surface as a
    /// panic in a running TUI. A map rather than a bare value so that
    /// `initialization_options = 12` faults with a file and a line (R9.5)
    /// instead of being sent as a number no server can read.
    #[serde(default)]
    pub initialization_options: Option<serde_json::Map<String, serde_json::Value>>,
    /// What this server, installed and running, still cannot do — the reader's
    /// words, shown on its row. R31.25 forbids a language that reads as
    /// configured and answers nothing, and a language that answers *some* of it
    /// is the same silence in a smaller shape: nothing Varde can observe tells
    /// a server with less to say from a file with less wrong in it. Named here
    /// for the reason a command is named here, and read nowhere except onto the
    /// row.
    #[serde(default)]
    pub partial: Option<String>,
    /// A question this server asks the *client* that Varde will not answer, and
    /// the method to say so on. Data for the reason a command is data: which
    /// servers ask one, and what they ask it on, is a fact about a server, and
    /// an arm naming either is the one R31.1 forbids.
    ///
    /// It exists because a question asked and never answered is a server that
    /// waits forever — alive, configured, and silent, which is what R31.25
    /// refuses. The question arrives as a notification, so the protocol has no
    /// reply of its own for it; only the sender knows the method its answer
    /// comes back on, so only configuration can say.
    #[serde(default)]
    pub unanswerable: Option<Unanswerable>,
}

/// The two method names one such question needs: what the server asks on, and
/// what Varde answers on. Both, because they are one fact — a request method
/// with no response method is a refusal that cannot be spoken, and neither is
/// any use alone. Named for what the key holds rather than for what Varde does
/// about it: `preview::Refusal` is already a different thing, and two of that
/// word would send a reader to the wrong one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Unanswerable {
    pub request: String,
    pub response: String,
}

/// What lays a language's files out, as configuration named it. The `[lsp.*]`
/// table's shape a second time, and deliberately so: a formatter is a command
/// on this machine that a project chooses, an OS packages differently, and
/// nobody at Varde can enumerate — the three properties
/// `docs/adr/0012-an-install-command-is-configuration.md` argues a name into
/// the bottom layer of the merge for.
///
/// Not merged into [`Server`]. They share four field *names* and no field
/// meaning: a server is spoken to over stdio for the life of the session and a
/// formatter is one process per keystroke, so `also_served_by`,
/// `initialization_options` and `unanswerable` are nonsense here and
/// `extensions` is nonsense there. Two similar things are a coincidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Formatter {
    /// Defaulted, and held to being named by the merged table, for the reason
    /// [`Server::command`] is: a layer is a patch, and a project naming only
    /// this language's `args` must not have to repeat a command out of a
    /// binary's built-in defaults.
    #[serde(default)]
    pub command: String,
    /// What it is run with. `${file}` is the Buffer's own path and every
    /// `[facts.*]` name resolves here too, which is what lets a command that
    /// reads stdin still be told which language it is reading.
    #[serde(default)]
    pub args: Vec<String>,
    /// What installs it, keyed by the OS the binary was built for — a lookup
    /// under the string `Startup` carried in, never a branch on it (R31.22).
    #[serde(default)]
    pub install: BTreeMap<String, String>,
    /// The file extensions this row claims. Asked on its own, never after the
    /// `[lsp.*]` rows: a file's server and its formatter are separate choices,
    /// so `rs` named in both tables is two facts rather than one with two
    /// authors (ADR 0018).
    #[serde(default)]
    pub extensions: Vec<String>,
}

/// What runs a language's Debug adapter, as configuration named it — the
/// `[dap.*]` row ADR 0021 puts every adapter in, so no arm names one. Spoken
/// to over its standard streams, over TCP where `args` name `${port}`, or —
/// where it names a `server` — over TCP on the port that language server
/// answers `command` with.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Adapter {
    /// Defaulted and held to being named by the merged table, for the reason
    /// [`Server::command`] is. For a hosted adapter it is not a program but
    /// the command sent to its `server`.
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub install: BTreeMap<String, String>,
    /// The `[lsp.*]` row whose server hosts this adapter as a plugin.
    #[serde(default)]
    pub server: Option<String>,
    /// Merged into that server's `initializationOptions` when it starts,
    /// which is how a server is told what to load: what the keys mean is the
    /// server's business, for the reason its own options are.
    #[serde(default)]
    pub plugin: Option<serde_json::Map<String, serde_json::Value>>,
}

/// A named way to start a Debug session: which adapter, whether it launches
/// or attaches, and the arguments that request carries. Allowed in either
/// layer, and the project's beats the global one of the same name because the
/// merge is key by key.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Launch {
    #[serde(default)]
    pub adapter: String,
    /// `launch` or `attach`, sent as the request's own name: the protocol has
    /// the two, and a Scenario naming a third reaches an adapter that says no.
    #[serde(default)]
    pub request: String,
    /// Handed to the adapter untouched, for the reason a server's
    /// `initialization_options` are: what an adapter needs to be told is its
    /// own business. Varde reads one thing in it and writes nothing: the
    /// `hostName` and `port` an attach session watches to attach again.
    #[serde(default)]
    pub args: serde_json::Map<String, serde_json::Value>,
    /// Whether an attach session whose program went away waits for it to
    /// answer again. On unless said otherwise: a remote machine that is not
    /// coming back is the case for saying so.
    #[serde(default = "attaches_again")]
    pub reattach: bool,
}

fn attaches_again() -> bool {
    true
}

/// What a Run mark stands beside and what it starts: the files it looks in, a
/// syntax-tree query whose `@run` capture is the line it marks, and the two
/// commands it fills from the query's other captures — `${name}` from `@name`
/// — and from `${file}`. Named freely rather than by language, since a
/// language has more than one kind of thing to start and each runs its own
/// way; `debug` names its adapter the way a Launch configuration does.
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

/// A path on this machine a server's configuration may name, and how to find
/// it. Data for the reason a command is data: which marker file means "this
/// directory configures the language" is the whole of what differs between
/// `node_modules/typescript/lib/typescript.js`, `.venv/bin/python` and
/// `compile_commands.json`, and an arm per ecosystem is a server's name in an
/// arm with more in it (R31.1, ADR 0011). The edge runs one search for every
/// fact and knows nothing about any of them.
///
/// Deliberately not a template language and not an expression: a marker path,
/// and which of the two things found the answer is. Anything a marker cannot
/// say is a fact Varde does not ship, which is the same bargain
/// `docs/adr/0012-an-install-command-is-configuration.md` strikes for installs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Fact {
    /// Looked for under each directory from the file being served up to the
    /// workspace root, nearest first. A monorepo installs its dependencies per
    /// package, so the toolchain that must serve a package's files is the one
    /// that package installed.
    ///
    /// Defaulted for the reason `Server::command` is, and held to the same
    /// check on the merged table.
    #[serde(default)]
    pub marker: String,
    #[serde(default)]
    pub value: FactValue,
    /// A machine-wide install to fall back to: a command on `PATH`, and where
    /// the marker sits relative to the directory holding it once symlinks are
    /// resolved. Both keys or neither — a command with nowhere to look from is
    /// no answer, and a marker with no command has nothing to look from.
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub command_marker: Option<String>,
    /// Whether the server starts without it. A fact a server cannot run at all
    /// without and a fact it is merely better with are two different facts, and
    /// nothing Varde can observe tells them apart — so configuration says,
    /// which is the reason a command is configuration.
    ///
    /// Required is the default and the interesting case is why the other exists:
    /// the plugin that gives a TypeScript server its `.vue` intelligence is
    /// named on the `[lsp.typescript]` row *every* TypeScript project shares.
    /// Required, a machine that never installed a Vue server would have no
    /// TypeScript server in any project — a requirement nobody declared, and
    /// machine-dependent, so it would work for whoever tested it. Optional
    /// changes only the spawn: a value that was found is filled in like any
    /// other, and the key naming one that was not is dropped exactly as it
    /// already would be.
    #[serde(default)]
    pub optional: bool,
    /// What puts the fallback `command` on this machine, per OS, as a program
    /// row's `install` does. A search still: nothing it finds is written back.
    #[serde(default)]
    pub install: BTreeMap<String, String>,
}

/// What is handed over once the marker is found: the marker itself, or the
/// directory holding it. Both are real — clangd wants the directory holding
/// `compile_commands.json` and pyright wants the interpreter itself — and
/// guessing from the marker's shape would make `.venv/bin/python` and
/// `compile_commands.json` indistinguishable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactValue {
    #[default]
    Marker,
    Directory,
}

/// The `[lsp.*]` and `[facts.*]` tables of one layer, typed, so that reading
/// them is a deserialize the `toml` crate can fault with a span rather than a
/// walk over `Value`s that has to decide what to do about each wrong shape
/// itself.
#[derive(Default, serde::Deserialize)]
struct Layer {
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

/// The same two tables read off a layer's source text, keeping where each entry
/// was named. A second type rather than a parameter on the first because
/// `Spanned` cannot be deserialized from a `toml::Value` at all — only the text
/// deserializer carries spans — and `Config` reads the merged table, which is
/// values by then.
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

/// Where each layer named an `[lsp.*]` or `[facts.*]` entry: the dotted name
/// the fault message prints, against the file and the line it was named on. A
/// later layer overwrites an earlier one, so the file named is the nearest one
/// — whose values won, and the one the reader is editing.
type Origins = BTreeMap<String, (String, usize)>;

#[derive(Debug, Clone, Default)]
pub struct Config(pub(crate) Table);

impl Config {
    /// Which languages have a server, and what runs each one. `get` below
    /// cannot answer this: a dotted scalar lookup reaches a value it is told the
    /// name of, and the languages are the names. A language in no layer is
    /// absent from the map, which is what makes "no server for this language" a
    /// value the core can hold rather than a silence it infers.
    ///
    /// Every layer was held to these same types as it was parsed and the merged
    /// table was held to being complete, so nothing here can fail for a config
    /// Varde started on — which is why this is a deserialize and not a walk
    /// deciding what to do about each wrong shape it meets, and why every
    /// `command` it returns is non-empty.
    pub fn servers(&self) -> BTreeMap<String, Server> {
        self.layer().lsp
    }

    /// And which command lays each language out, read the same way and for the
    /// same reasons: a project that formats its own files with its own tool is
    /// a row in a file rather than a release.
    pub fn formatters(&self) -> BTreeMap<String, Formatter> {
        self.layer().formatter
    }

    /// Which paths on this machine configuration lets a server name, and how
    /// the edge is to find each one. Read the same way and for the same
    /// reasons: a project declaring a fact its own toolchain needs is a row in
    /// a file rather than a release.
    pub fn facts(&self) -> BTreeMap<String, Fact> {
        self.layer().facts
    }

    /// Which languages have a Debug adapter, and what runs each one.
    pub fn adapters(&self) -> BTreeMap<String, Adapter> {
        self.layer().dap
    }

    /// The Launch configurations both layers name, by name.
    pub fn launches(&self) -> BTreeMap<String, Launch> {
        self.layer().launch
    }

    /// What Run marks stand beside, by row.
    pub fn runs(&self) -> BTreeMap<String, Run> {
        self.layer().run
    }

    /// The typed tables of the merged config. Nothing here can fail for a
    /// config Varde started on, for the reason `servers` gives.
    fn layer(&self) -> Layer {
        toml::Value::Table(self.0.clone())
            .try_into::<Layer>()
            .unwrap_or_default()
    }

    /// Looks up a dotted key such as `editor.tab_width`.
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

/// Why Varde would not open. Each way a path can be unusable gets its own
/// reason, because a typo, a file and a permissions problem are three
/// different things for the user to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    Path(&'static str),
    Config(ConfigError),
}

pub fn start(input: &Startup) -> Result<(State, Config, Vec<Effect>), StartupError> {
    if let Some(reason) = path_refusal(input.path_status) {
        return Err(StartupError::Path(reason));
    }
    // A Bare workspace has no project configuration layer at all, so a
    // `.varde/config.toml` that happens to sit in the folder — most likely
    // somebody else's — is not read. Skipped rather than refused: an
    // unparseable file Varde never looks at must not stop it starting either.
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
        // Swept before it is made, because a crash mid-Reading escapes both
        // the deletion the player's exit makes and the one quitting makes.
        // Safe with no pattern to match against precisely because everything
        // under it is Varde's (ADR 0014).
        Effect::DeleteDir(crate::tmp_dir(&input.varde_home)),
        Effect::EnsureDir(crate::tmp_dir(&input.varde_home)),
    ];
    if binary_install {
        effects.push(Effect::CheckRelease {
            url: RELEASE_URL.to_string(),
        });
    }
    // A reader's own settings survive every start, and the core is already
    // holding the fact that decides it: `project_config` is what the edge read
    // off `.varde/config.toml`, and it is `None` on exactly the folders that
    // have no file to lose. An `Effect` that asked the disk again would put
    // this promise in `main.rs`, where no scenario can reach it.
    // A Bare workspace seeds nothing: the file would land in a Sidecar deleted
    // at exit, and a key written where nobody can find it is worse than a key
    // never written — the same argument the seed itself makes, read the other
    // way. It also has no project layer to lose, so `project_config` cannot be
    // what decides it.
    if input.sidecar.is_none() && input.project_config.is_none() {
        effects.push(Effect::WriteFile {
            path: crate::varde_dir(&input.root, input.sidecar.as_deref()).join(CONFIG_FILE),
            contents: SEEDED_CONFIG.to_string(),
        });
    }
    // The global file by the same rule, and in a Bare workspace too: it lives
    // in `~/.varde`, not the workspace, so nothing about the folder decides it.
    if input.global_config.is_none() {
        effects.push(Effect::WriteFile {
            path: input.varde_home.join(CONFIG_FILE),
            contents: template(),
        });
    }
    // The editor comes back to the files it had, which is the only part of the
    // saved state the core cannot simply be started holding: a Buffer needs its
    // contents, and contents are the edge's to read.
    let buffers = saved_buffers(&input.root, input.state_json.as_deref());
    state.restoring = buffers.len();
    effects.extend(buffers.into_iter().map(Effect::OpenBuffer));
    // Each file a remembered Breakpoint is in is read once, open or not, so a
    // Breakpoint whose line has moved on is Stale from the start.
    let files: std::collections::BTreeSet<PathBuf> = state
        .breakpoints
        .iter()
        .map(|breakpoint| breakpoint.file.clone())
        .collect();
    effects.extend(files.into_iter().map(Effect::ReadBreakpointFile));

    // Measuring starts without being asked: the figure is there when the user
    // wants it rather than after they remember to ask for it. Unless the cache
    // already answers for the commit that is checked out — reopening on code
    // nobody has changed is the common case, and re-measuring it spends seconds
    // to arrive at the number already on disk.
    // A Bare workspace is the exception, and for the same reason the seed is:
    // the figure would be measured into a Sidecar deleted at exit. Nothing is
    // removed — the Risk pane's own recompute still measures on demand.
    let cache = cached(input);
    let unmeasured = cache.is_none();
    if let Some(figures) = cache {
        state.risk.figure = risk::Figure::Current(figures);
    }

    // Starting in a view is arriving in it, and arriving loads what the view
    // shows: Story's sets, Review's first diff and its own figure. After the
    // cache, so a restored Review treats the workspace figure exactly as
    // switching into Review would.
    let (mut state, entered) = crate::enter_view(&state, state.view);
    effects.extend(entered);

    // Review view asked for its own Scope on the way in, and a workspace job
    // would only be superseded by it.
    if unmeasured && input.sidecar.is_none() && !state.risk.in_flight() {
        effects.push(risk::analyse(&mut state, Scope::Workspace));
    }
    Ok((state, config, effects))
}

/// Why the path Varde was opened on is not a workspace, if it is not one.
fn path_refusal(status: PathStatus) -> Option<&'static str> {
    match status {
        PathStatus::Folder => None,
        PathStatus::Missing => Some("no-such-folder"),
        PathStatus::NotAFolder => Some("not-a-folder"),
        PathStatus::Unreadable => Some("folder-not-readable"),
    }
}

/// The Settings, under the global config, under the project's own. A project
/// layer that is not there is an empty one, which merges nothing; a global one
/// that is not there is the [`template`] this same start seeds it with, so a
/// fresh machine has its servers on the start that writes them. The Program
/// rows have no layer of their own beneath the files: what the files name is
/// what runs (ADR 0018).
///
/// Each layer is held to the types as it is parsed, because only the source
/// text can name a line — but a layer is a *patch*, so completeness is the
/// merged table's to satisfy and is checked once, at the end. `origins` is what
/// lets that fault still name a file and a line after the source text is gone.
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
        let (overlay, mentioned) = parse(source, label)?;
        origins.extend(mentioned);
        merge(&mut table, overlay);
    }
    refuse_incomplete(&table, &origins)?;
    refuse_claimed_twice(&table, &origins)?;
    refuse_unusable_runs(&table, &origins)?;
    Ok(table)
}

/// A `[run.*]` row that could never mark a line, refused rather than left to
/// read as configured (R41.1): its query is compiled against the grammar of
/// every extension it claims.
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

/// The merged table is what must be complete. The one key an entry cannot be
/// used without — a server's `command`, a fact's `marker` — is required of the
/// merge rather than of each layer, because requiring it of a layer refuses
/// every partial override of a shipped language, and requiring it only of a
/// layer that introduces a *new* entry refuses a project patching what the
/// global config introduced, which is the same defect one layer up.
///
/// A presence check over the merged values rather than a deserialize: every
/// layer was already held to the types, so nothing here can be the wrong shape
/// — only absent. Blamed on the last layer that mentioned the entry, whose
/// values won and whose file the reader is the one editing.
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
            // Every entry in the merged table was named by some layer, so the
            // origin is there. Were one ever missing, the entry and the fault
            // are still spoken — that is the part that must not be silent.
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

/// An extension two `[lsp.*]` rows claim has no server a reader can predict:
/// the rows come from a merge, and whichever one serde met first is not an
/// answer (ADR 0018). Blamed on whichever of the two the nearest layer named,
/// which is the file the reader is editing.
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
            // A row naming one extension twice is one claim, not a collision.
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

/// The workspace as the saved state and the config leave it.
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
        // Absent until the AI pane's edge has been dragged, which is what the
        // layout reads as its share of the screen.
        ai_width: saved_number(input.state_json.as_deref(), "ai_width"),
        strip_height: saved_number(input.state_json.as_deref(), "strip_height"),
        output_width: saved_number(input.state_json.as_deref(), "output_width"),
        breakpoints: saved_breakpoints(&input.root, input.state_json.as_deref()),
        // The Snippets this project has run, oldest first, and where it last
        // left the Evaluator's window. Both outlive the session they were
        // made in, which is what makes them the project's rather than the
        // Debug adapter's.
        snippets: saved_list(input.state_json.as_deref(), "snippets"),
        evaluator_at: saved_window(input.state_json.as_deref()),
        exception_filters: input
            .state_json
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .and_then(|mut parsed| serde_json::from_value(parsed["exception_filters"].take()).ok())
            .unwrap_or_default(),
        // Beside the editor unless the project was last worked in the tall
        // shape — including state recorded before `:tall` existed, which names
        // no shape at all.
        ai_pane: match saved_text(input.state_json.as_deref(), "ai_pane").as_deref() {
            Some("Tall") => crate::layout::AiPane::Tall,
            _ => crate::layout::AiPane::Beside,
        },
        // A corner nobody opened stays closed: an absent key reads as hidden.
        //
        // The legacy key second, and only when the new one says nothing: state
        // written before the corner held more than the Risk list names the pane
        // rather than the slot, and reading it is what keeps a session saved by
        // an older Varde from silently resetting its layout.
        corner: match saved_text(input.state_json.as_deref(), "corner").as_deref() {
            Some("Risk") => crate::layout::Corner::Risk,
            Some("Buffers") => crate::layout::Corner::Buffers,
            Some("History") => crate::layout::Corner::History,
            Some("Breakpoints") => crate::layout::Corner::Breakpoints,
            Some("Conflicts") => crate::layout::Corner::Conflicts,
            // On Errors whatever it showed last: nothing has been reported
            // yet, and with nothing the list opens on Errors.
            Some(saved) if saved.starts_with("Diagnostics") => {
                crate::layout::Corner::Diagnostics(crate::lsp::Severity::Error)
            }
            Some(_) => crate::layout::Corner::Hidden,
            None => match saved_text(input.state_json.as_deref(), "risk_list").as_deref() {
                Some("Shown") => crate::layout::Corner::Risk,
                _ => crate::layout::Corner::Hidden,
            },
        },
        // On unless it was turned off: state recorded before `:dim` existed
        // has no such key.
        editor_field: saved_flag(input.state_json.as_deref(), "editor_field").unwrap_or(true),
        // What you turned it to here last time wins; `editor.minimap` is only
        // where a project with no history starts, the same shape `ai_command`
        // has.
        minimap: saved_flag(input.state_json.as_deref(), "minimap")
            .unwrap_or_else(|| config.get("editor.minimap").as_deref() != Some("false")),
        // What you used here last time wins; config is the default when there
        // is no history.
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

/// Every `State` field the merged config alone decides — what a reload
/// recomputes, and so the one derivation startup and a reload share. A key
/// that only sets where a project with no history *starts*, such as
/// `editor.minimap` or `ai.command`, is not here: it is the session's once it
/// has started, and a saved config must not flip what the project recorded.
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
        // Absent unless the project said so: what the Gate runs is then read
        // off the project's shape instead, and a project whose shape says
        // nothing refuses the loop rather than passing a Gate having run
        // nothing.
        test_command: config.get("risk.test_command"),
        // Which command serves which language, as the merged layers left it.
        // Naming one is not starting one: the spawn is the edge's, and only
        // when a Buffer in that language is open.
        servers: config.servers(),
        // And what lays each language out, which is the same table in a second
        // shape: a formatter is named in a file, never in an arm.
        formatters: config.formatters(),
        // And which paths a server may name, which is data for the same
        // reason: the edge searches, the library decides nothing (R31.27).
        facts: config.facts(),
        adapters: config.adapters(),
        launches: config.launches(),
        runs: config.runs(),
        speech: speech(config, &state.os),
        ..std::mem::take(state)
    };
}

/// One config layer as the edge found it when the file changed on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDisk {
    /// No file: merged as a start with no file would merge it.
    Missing,
    Text(String),
    /// There, and the edge could not read it.
    Unreadable,
}

/// Both layers again, while Varde runs: the fields startup derives from the
/// merge are derived again, and nothing else moves. Nothing is spawned or
/// stopped either — a running child keeps the command it was started with, and
/// the next one started reads the new one. A layer that is broken leaves the
/// last good config in place and is returned, as a start would refuse it.
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
    // The Bare workspace rule `start` keeps: no project layer at all.
    let project = match &state.sidecar {
        Some(_) => None,
        None => text(project, PROJECT_LABEL)?,
    };
    let config = Config(merged_config(global.as_deref(), project.as_deref())?);
    configure(state, &config);
    Ok(())
}

/// The `[speech]` row, with the two per-OS tables already resolved for the OS
/// this binary was built for — a lookup under the string `Startup` carried in,
/// never a branch on it (R31.22). A row this machine has no entry on is a
/// blank, which is the refusal `reading::start` names out loud rather than a
/// command that cannot work.
pub(crate) fn speech(config: &Config, os: &str) -> crate::reading::Speech {
    let named = |key: &str| config.get(key).unwrap_or_default();
    crate::reading::Speech {
        command: named("speech.command"),
        // The one key `get` cannot answer, because it is a list and `get`
        // flattens what it cannot name into a printing nobody can split back.
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

/// A program Varde can be configured to run, and what installs it on one OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dep {
    pub kind: &'static str,
    pub name: String,
    pub command: String,
    pub install: Option<String>,
}

/// The `[lsp.*]`, `[formatter.*]`, `[dap.*]` and `[speech]` rows `~/.varde/config.toml`
/// names — or the [`template`], when there is no file, since that is what the
/// next start runs — read by the same merge startup does, so what `install.sh`
/// offers is what Varde would start. A file Varde would refuse to start on is
/// refused here too. The speech row's player is a row of its own, kind
/// `player`, with no install: the table names none, since it ships with macOS
/// and comes with alsa-utils on Linux. Either is left out when no file names
/// its command, since `[speech]` is the one table the Settings share.
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
            // A hosted adapter is there when the server it is loaded into is,
            // as Tools reads it.
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

/// The figure on disk, if it describes the commit that is checked out. A folder
/// that is no repository has no commit to check against, so its cache is never
/// believed — there is nothing that could say the code had moved.
fn cached(input: &Startup) -> Option<risk::Figures> {
    risk::cached(input.risk_json.as_deref()?, input.head.as_deref()?)
}

/// Which checkout the running binary came from, and whether it has moved ahead.
///
/// A directory counts as Varde's checkout only when its manifest parses and names
/// the `varde` package. Another crate's manifest, a manifest that is not valid
/// TOML, and a copied binary with nothing above it are all somebody else's
/// directory, so none of them yields a checkout — which is what stops Varde from
/// ever running a build somewhere the user did not expect. Note the deliberate
/// contrast with the config files above: one that does not parse stops Varde
/// from starting, because the user handed it over and needs to fix it, while the
/// checkout manifest was never handed to Varde at all.
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

/// An Update is a Version *strictly newer* than the Running version: a checkout
/// behind the binary offers nothing, so checking out an old branch cannot nag
/// anyone to downgrade. Ordering comes from `semver` because a string comparison
/// puts `0.10.0` below `0.9.0` and calls the older one newer.
fn is_update(checkout: &str, running: &str) -> bool {
    match (
        semver::Version::parse(checkout),
        semver::Version::parse(running),
    ) {
        (Ok(checkout), Ok(running)) => checkout > running,
        _ => false,
    }
}

/// Project values override global ones key by key; a table on both sides is
/// merged rather than replaced, so naming one key does not drop its siblings.
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

/// One layer of the merge, and where it mentioned each `[lsp.*]` and
/// `[facts.*]` entry. Two ways it can be unusable are refused here: TOML that
/// does not parse, and a table naming a value that is not the shape it must be.
/// The second is checked here rather than where the servers are read, because
/// only the source text can say which line to name — and a server entry Varde
/// cannot use, dropped quietly, is a language the user configured and nothing
/// serves.
///
/// The third way — an entry no layer ever completed — cannot be seen from one
/// layer, so the spans come back with the table and `refuse_incomplete` decides.
fn parse(source: &str, label: &str) -> Result<(Table, Origins), ConfigError> {
    let line = |offset: usize| source[..offset].matches('\n').count() + 1;
    let at = |error: &toml::de::Error| error.span().map_or(1, |span| line(span.start));
    let table = source.parse::<Table>().map_err(|error| ConfigError {
        file: label.to_string(),
        line: at(&error),
        fault: ConfigFault::NotToml,
    })?;
    let layer = toml::from_str::<SourceLayer>(source).map_err(|error| ConfigError {
        file: label.to_string(),
        line: at(&error),
        fault: ConfigFault::WrongType(error.message().to_string()),
    })?;
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

/// A list of project-relative paths the last session recorded — which folders
/// the tree had open, which files were in the editor. Absent state means a
/// fresh project, so the answer is empty rather than a guess.
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

/// The files that were open last time, the one that was current *last*: the
/// buffers are restored by opening them, and opening a file is what makes it
/// current, so the order is the whole of "you come back where you left".
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

/// A recorded list of strings — the Snippets — in the order it was written.
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

/// Where the project last left the Evaluator's window, and `None` where it
/// never opened one. Read whole or not at all: three of four numbers is a
/// rectangle nobody drew, and the centred one is the better answer than a
/// guess at the fourth. The screen it is placed on is `update`'s to say — a
/// rectangle recorded on a bigger screen is clamped onto this one there,
/// which is why nothing here asks how wide the terminal is.
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

    /// What a fresh machine starts on: the Settings under the template it
    /// seeds as the global layer, as several tests below read the rows.
    fn fresh() -> Config {
        Config(merged_config(None, None).expect("the template parses"))
    }

    /// A config text with every `# key = value` line made live, and nothing
    /// else: prose that happens to hold ` = ` has a space or a backtick in
    /// what would be its key.
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

    /// The Corner's occupant persists, as it does for every occupant — the
    /// Breakpoint list too, which needs no session to be shown.
    #[test]
    fn the_breakpoint_list_is_back_in_the_corner_after_a_restart() {
        let (state, _, _) = start(&Startup {
            state_json: Some(r#"{"corner": "Breakpoints"}"#.to_string()),
            ..Startup::default()
        })
        .expect("started");
        assert_eq!(state.corner, crate::layout::Corner::Breakpoints);
    }

    /// A Breakpoint's properties, written by `state_json` and read back by a
    /// start — both halves in one test, for the reason the Evaluator's are.
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

    /// What a project keeps of the Evaluator, written by `state_json` and read
    /// back by a start. Both halves in one test because a key spelled one way
    /// in the writer and another in the reader is a window that silently
    /// reopens centred every time, and nothing else in the suite compares the
    /// two spellings: a scenario that records a rectangle writes the JSON
    /// itself.
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
        // A project that never opened one opens centred, which is `None` and
        // not a rectangle of zeroes.
        let (fresh, _, _) = start(&Startup::default()).expect("started");
        assert_eq!(fresh.evaluator_at, None);
    }

    /// The Diagnostic list comes back too, on Errors: the Severity it showed
    /// described reports from servers that are not running yet.
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

    /// The refusal a project layer earns, so that the tests below assert the
    /// file, the line *and* which of the three faults it was.
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

    /// R9.5's refusal, for the half of "malformed" that still parses: an entry
    /// naming a command that is not a string is a server Varde cannot run, and
    /// dropping it quietly leaves a language the user configured served by
    /// nothing, with no message and nothing to fix. The line comes from the
    /// source text, which is why the check is at the layer and not where the
    /// servers are read.
    #[test]
    fn a_command_that_is_not_a_string_refuses_to_start() {
        let problem = refusal("[lsp.rust]\ncommand = 12\n");
        assert_eq!((problem.file.as_str(), problem.line), (PROJECT_LABEL, 2));
        assert!(
            matches!(problem.fault, ConfigFault::WrongType(_)),
            "{problem}"
        );
    }

    /// The arguments are checked with the command, since a server started with
    /// an argument nobody can spell is the same unusable entry.
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
            // The Debug adapter's row, installable on both.
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
            // What the row configures is the file its own install fetches.
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

    /// `varde --deps` answers from the file, not the binary: a global config
    /// naming one server lists that server and nothing the template ships, and
    /// one Varde would refuse to start on is refused with its file and line.
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

    /// The files each language was served before its rows named their own
    /// extensions — the `match` those rows replaced, kept as the list it was —
    /// are served the same way after, by both tables. A row that dropped one
    /// is a file that quietly lost its server or its formatter.
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

    /// A row claiming nothing is a server no file reaches, and a formatter
    /// that lays nothing out: data that never fires.
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

    /// The claim is refused naming both rows, and blamed on the layer that
    /// made it rather than on the template it collided with.
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

    /// The template held to the same check as a file a reader wrote, and named
    /// here so a typo in the data fails as itself rather than as every scenario
    /// that starts Varde. One bad row empties its whole kind, so a count short
    /// of the tables is any row failing.
    #[test]
    fn every_template_row_parses_into_a_valid_row() {
        let config = fresh();
        let named = |kind: &str| config.0[kind].as_table().expect(kind).len();
        assert_eq!(config.servers().len(), named("lsp"));
        assert_eq!(config.formatters().len(), named("formatter"));
        assert_eq!(config.facts().len(), named("facts"));
    }

    /// Read off the template itself rather than through the merge, so the
    /// template stays held if the merge's own refusal ever stops holding it.
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

    /// R9.5 again, for the key this ticket adds: an install command that is not
    /// a string is an entry Varde cannot type, and dropping it quietly leaves a
    /// row that offers nothing for a reason nobody can see.
    #[test]
    fn an_install_command_that_is_not_a_string_refuses_to_start() {
        let problem = refusal("[lsp.zig]\ncommand = \"zls\"\ninstall.macos = 12\n");
        assert_eq!((problem.file.as_str(), problem.line), (PROJECT_LABEL, 3));
        assert!(
            matches!(problem.fault, ConfigFault::WrongType(_)),
            "{problem}"
        );
    }

    /// An install key spelled for an OS no binary is built for is a command
    /// that can never fire, and it would look configured in the file that
    /// carries it. The three spellings are `std::env::consts::OS`'s, which is
    /// what `main.rs` hands in.
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

    /// Honesty, held to by the two languages the ADR names: `zls` on Linux is a
    /// build from source and `jdtls` is in no distribution, so neither has a key
    /// there. An invented command that fails looks configured; a blank one is
    /// one line of TOML away from being right.
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

    /// A layer is a patch, so naming one key of a language `DEFAULTS` ships
    /// leaves every key it did not name standing. Held over the whole entry
    /// rather than over the one key the caller went looking for: `[lsp.vue]` is
    /// the entry with the most on it, and a `command` that survived while
    /// `unanswerable` was dropped is a server that starts and then waits
    /// forever.
    ///
    /// Before this, `Server::command` was required of each layer, so a project
    /// naming one key was refused outright — and the only workaround was to
    /// copy `command` out of a binary's built-in defaults, which then silently
    /// stopped tracking them.
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

    /// The same for a `[facts.*]` table, because it is the same mechanism: a
    /// layer is a patch for both, and `marker` is to a fact what `command` is
    /// to a server. Left asymmetric, a project overriding a shipped fact's
    /// `value` alone would meet the refusal this ticket removed.
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

    /// The `[lsp.*]` half of this is a scenario; the `[facts.*]` half is only
    /// here, because a fact is not a stakeholder-visible thing to write one
    /// about — and it is a table with a key the entry is no use without, so
    /// leaving it unchecked would let a project declare a fact the edge is then
    /// asked to find nothing for.
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

    /// The three faults, as they read on screen. Pinned because the whole of
    /// the second half of this ticket is that they read *differently*: one
    /// sentence stood for all three, so a missing key wore the parse fault's
    /// words and sent the reader hunting a syntax error that was not there.
    /// Valid TOML is never called invalid.
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

    /// The merge is key by key inside the install table too, so replacing the
    /// command for this machine does not silently drop the others — the point
    /// of shipping the defaults at all is that a later binary's corrections
    /// arrive for every OS the user did not override. The layer repeats
    /// `command` because each one is checked against `Server` on its own, which
    /// is what buys the file and the line R9.5 asks for.
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

    /// A name a `[facts.*]` table declares is expanded, and every other
    /// `${...}` is passed through as the string it is (R31.27) — so a typo in
    /// the data ships an argument that reaches the server literally, and
    /// nobody would see it until a server complained. Arguments *and*
    /// initialization options, because the same substitution reaches both.
    /// Held here, where the data is.
    ///
    /// **Every occurrence, not every string.** Asking whether some declared
    /// name appears in the text is satisfied by one correct name in a string
    /// holding two, and the options table is serialized whole — so once the
    /// TypeScript row named both an SDK and a plugin, a typo in either was
    /// covered by the other's match. The names are pulled out of the text
    /// instead, which is what makes the assertion count.
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
        // A test that found nothing to check would pass for a `DEFAULTS` that
        // stopped interpolating at all.
        assert!(seen >= 3, "only {seen} interpolated names found");
    }

    /// Every `${name}` in one configured string, in order. Only used by the
    /// test above: `lsp::filled` substitutes by walking the declared names
    /// rather than by parsing the text, which is what leaves an undeclared
    /// `${...}` alone (R31.27), and this is the reverse question.
    fn interpolated(text: &str) -> Vec<String> {
        text.split("${")
            .skip(1)
            .filter_map(|rest| rest.split_once('}'))
            .map(|(name, _)| name.to_string())
            .collect()
    }

    /// The shipped fact, read back as the shape the edge searches with. A
    /// `value` key that stopped deserializing would leave the SDK resolving to
    /// `typescript.js` itself and every Vue server pointed at a file.
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

    /// Two spellings of one default: the TOML the merge starts from, and the
    /// number the library counts with when no config was read at all. Every
    /// key, because a default that disagrees with itself is a figure — a cap,
    /// or an indent width — nobody can predict.
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

    /// A Run row that could never mark a line is refused where it was
    /// written, rather than read as configured and silently marking nothing.
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

    /// Each seed is decided by one fact: its layer is `None`. The edge
    /// hands `Some("")` for a file it found and could not read — not UTF-8, or
    /// write-only — because an empty layer merges nothing and still says "a
    /// file is here". Seeding over it would be a silent delete of settings
    /// Varde could not parse, which is the one way this feature can destroy
    /// something.
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
                .any(|effect| matches!(effect, Effect::WriteFile { .. })),
            "starting wrote over a config file that is already there: {effects:?}"
        );
    }

    /// Three promises about the file starting lays down, and the first is the
    /// one no scenario can see break. Every table it holds must be *empty*: a
    /// live key would make "the project sets nothing" false on a project's
    /// first run, and the effective-settings scenario cannot catch it, because
    /// the numbers quoted here are the defaults — uncommenting them changes
    /// nothing and every assertion stays green.
    ///
    /// The second is why quoting the defaults is safe at all. Uncomment every
    /// key and what is left must be the [`DEFAULTS`] layer, value for value, so
    /// a number that moves there and not here is caught before it ships. A
    /// stale line is worse than no line: it reads as advice and pins the answer
    /// the reader was trying to accept.
    ///
    /// The third is the same promise read the other way, and it is the one
    /// this feature exists for: every scalar [`DEFAULTS`] spells must be named
    /// here. Walking only seeded → defaults leaves a tunable the defaults grow
    /// findable nowhere while the whole suite stays green, which is precisely
    /// the state `editor.tab_width` was in for its whole life. `[lsp.*]`,
    /// `[formatter.*]` and `[facts.*]` are excluded by holding no scalar
    /// directly under their own table: they are data a reader reaches for a
    /// language, not numbers to tune, and listing every server here would bury
    /// the four that are.
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
        let defaults = fresh().0;
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

    /// The template's two halves, held the way the project seed is. Its live
    /// text is exactly the Program rows — the Settings' tables are there, and
    /// empty. Made live, it is exactly the built-in layers, so every Setting is
    /// named, commented, and quoted at its default, and no Program row hides
    /// behind a `#`.
    #[test]
    fn the_template_has_live_program_rows_and_commented_settings() {
        let mut live: toml::Table = template().parse().expect("valid TOML");
        let programs: toml::Table = PROGRAMS.parse().expect("valid TOML");
        let settings: toml::Table = DEFAULTS.parse().expect("valid TOML");
        // A header over commented keys, which is how a Setting that is a
        // table of tables — a `[run.*]` row — is named and left unset.
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

    /// The reason `semver` is a dependency: `"0.10.0" < "0.9.0"` as strings,
    /// because `1` sorts below `9`, so a hand-rolled compare would call the
    /// newer checkout older and never offer the Update.
    #[test]
    fn a_double_digit_component_is_compared_as_a_number() {
        assert!(is_update("0.10.0", "0.9.0"));
        assert!(!is_update("0.9.0", "0.10.0"));
    }

    /// Held equal by hand to the matrix in `.github/workflows/release.yml`,
    /// which points back here: YAML and Rust share no compiler, so a renamed
    /// Asset on either side would leave every binary install finding nothing
    /// built for it.
    #[test]
    fn asset_names_match_the_release_workflow() {
        assert_eq!(asset_name("macos", "aarch64"), "varde-macos-aarch64");
        assert_eq!(asset_name("macos", "x86_64"), "varde-macos-x86_64");
        assert_eq!(asset_name("linux", "x86_64"), "varde-linux-x86_64");
        assert_eq!(asset_name("linux", "aarch64"), "varde-linux-aarch64");
    }

    /// `sha256sum`'s own line shape, which is what the release workflow writes.
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

    /// A name that only ends in the Asset's is another file's line.
    #[test]
    fn a_list_without_the_assets_line_names_no_asset() {
        let list = "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae  old-varde-linux-x86_64\n";
        assert_eq!(
            verify(list, "https://example.test/varde-linux-x86_64", b"foo"),
            Err(ReplaceFailed::NoAsset)
        );
    }

    /// An Asset nobody can verify is not one `:update` may install.
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

    /// Strictly greater, so checking out an old branch does not offer to
    /// downgrade the binary that is already newer.
    #[test]
    fn a_version_behind_the_binary_is_not_an_update() {
        assert!(!is_update("0.0.9", "0.1.0"));
        assert!(!is_update("1.0.0", "2.0.0"));
    }

    /// A pre-release sits below its own release and above the version before it,
    /// which is what makes an rc in the checkout an Update but not a downgrade
    /// of the release it precedes.
    #[test]
    fn a_pre_release_qualifier_orders_below_its_release() {
        assert!(is_update("0.2.0-rc.1", "0.1.0"));
        assert!(!is_update("0.2.0-rc.1", "0.2.0"));
        assert!(is_update("0.2.0", "0.2.0-rc.1"));
    }

    /// Neither side is something Varde wrote, so neither is trusted to parse.
    #[test]
    fn a_version_that_is_not_a_version_is_not_an_update() {
        assert!(!is_update("nightly", "0.1.0"));
        assert!(!is_update("0.2.0", ""));
    }
}
