//! F2 — turning known folder contents into rows.
//!
//! Reads nothing. A folder's entries arrive when the edge expands it, which is
//! what makes the tree lazy: unopened folders are simply absent from `contents`.

use crate::{filter, review, story, State, View};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The pane's name, and what Risk says about the workspace. One place on
/// screen always answers the same question, so the figure lives on the border
/// rather than in a pane of its own. In the library rather than in `ui`
/// because the Diagnostic totals are drawn after it, and `mouse` hit-tests
/// them from where it ends.
pub fn title(state: &State) -> String {
    match crate::risk::border(state) {
        Some(figure) => format!("tree  {figure}"),
        None => "tree".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

/// What a path that turned up on disk turned out to be. The edge looks, because
/// only the edge may touch the filesystem — guessing here is how a new folder
/// came to be listed as a file until its parent was collapsed and reopened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: PathBuf,
    pub is_dir: bool,
    pub expanded: bool,
    /// Git-ignored: shown, but visually deprioritised.
    pub dimmed: bool,
}

/// What the left pane is listing: the tree in Edit view, the changed files in
/// Review view. Story view lists one of two things depending on `t`'s
/// toggle — the changed-files list again, joined rather than replaced, or the
/// spine, which is not a path and so is never rows here.
pub fn visible_rows(state: &State) -> Vec<Row> {
    match state.view {
        View::Edit if !state.filter.is_empty() => filtered(state),
        View::Edit => rows(state),
        View::Review => changed_files(state),
        View::Story => match state.story_listing {
            story::Listing::Files => changed_files(state),
            story::Listing::Spine => Vec::new(),
        },
    }
}

fn changed_files(state: &State) -> Vec<Row> {
    review::list(state)
        .into_iter()
        .map(|file| Row {
            path: state.root.join(file),
            is_dir: false,
            expanded: false,
            dimmed: false,
        })
        .collect()
}

/// The rows the filter box takes off the tree pane's list: two in Edit view,
/// where the box is drawn on the pane's bottom edge, none where there is no
/// box. Both the row count here and `/`'s own gate in `keys` read this one
/// answer, rather than deciding separately and risking a promise the box does
/// not keep.
pub fn filter_rows(view: View) -> usize {
    match view {
        View::Edit => 2,
        View::Review | View::Story => 0,
    }
}

/// Matches in ranked order, each preceded by the folders that lead to it so
/// you can see where it lives. A folder appears once.
fn filtered(state: &State) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    // Rescanning `out` for each folder is quadratic, and a one-letter needle in
    // a big project matches most of it: 54k files took minutes per keystroke.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for path in filter::matches(state) {
        let parts: Vec<&str> = path.split('/').collect();
        let mut walk = state.root.clone();
        for (index, part) in parts.iter().enumerate() {
            walk = walk.join(part);
            if !seen.insert(walk.clone()) {
                continue;
            }
            out.push(Row {
                is_dir: index + 1 < parts.len(),
                expanded: true,
                dimmed: state.ignored.contains(&walk),
                path: walk.clone(),
            });
        }
    }
    out
}

pub fn rows(state: &State) -> Vec<Row> {
    let mut out = Vec::new();
    push_folder(state, &state.root, &mut out);
    out
}

fn push_folder(state: &State, folder: &Path, out: &mut Vec<Row>) {
    let Some(entries) = state.contents.get(folder) else {
        return;
    };
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    // Directories first so the shape of the project stays stable as files come
    // and go; alphabetical within each group.
    sorted.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
    for entry in sorted {
        let path = folder.join(&entry.name);
        let expanded = state.expanded.contains(&path);
        out.push(Row {
            is_dir: entry.is_dir,
            expanded,
            dimmed: state.ignored.contains(&path),
            path: path.clone(),
        });
        if entry.is_dir && expanded {
            push_folder(state, &path, out);
        }
    }
}

/// What the focused row offers. Only the focused row offers anything, and a
/// file can only be deleted. `visible_rows`, not `rows`: in Review view the
/// rows are the changed files, none of which is in the unfiltered tree, so
/// looking there found nothing and drew no icons at all.
pub fn row_actions(state: &State, path: &Path) -> Vec<&'static str> {
    if state.tree_selection.as_deref() != Some(path) {
        return Vec::new();
    }
    match visible_rows(state).into_iter().find(|row| row.path == path) {
        Some(row) if row.is_dir => vec![
            "new-file",
            "new-directory",
            "go-here",
            "search-here",
            "delete",
            "copy-path",
        ],
        Some(_) => vec!["delete", "copy-path"],
        None => Vec::new(),
    }
}

/// What a row *is*, so the edge can colour its glyph. Colour is a theme's
/// business and lives at the edge — same split as `highlight::Kind`, which is
/// why no scenario asserts a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Directory,
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Go,
    Java,
    Vue,
    Svelte,
    Ruby,
    Php,
    C,
    Cpp,
    CSharp,
    Swift,
    Sql,
    Markup,
    Style,
    Data,
    Doc,
    Pdf,
    Image,
    Media,
    Archive,
    Shell,
    Spec,
    Lock,
    Git,
    Docker,
    Package,
    Source,
    Build,
    Plain,
}

impl IconKind {
    /// Every kind, so the palette at the edge can be swept for two kinds that
    /// ended up the same colour. The edge used to keep its own copy of this
    /// list, which is a list that silently stops covering the kinds added
    /// after it.
    pub const ALL: &'static [IconKind] = &[
        IconKind::Directory,
        IconKind::Rust,
        IconKind::JavaScript,
        IconKind::TypeScript,
        IconKind::Python,
        IconKind::Go,
        IconKind::Java,
        IconKind::Vue,
        IconKind::Svelte,
        IconKind::Ruby,
        IconKind::Php,
        IconKind::C,
        IconKind::Cpp,
        IconKind::CSharp,
        IconKind::Swift,
        IconKind::Sql,
        IconKind::Markup,
        IconKind::Style,
        IconKind::Data,
        IconKind::Doc,
        IconKind::Pdf,
        IconKind::Image,
        IconKind::Media,
        IconKind::Archive,
        IconKind::Shell,
        IconKind::Spec,
        IconKind::Lock,
        IconKind::Git,
        IconKind::Docker,
        IconKind::Package,
        IconKind::Source,
        IconKind::Build,
        IconKind::Plain,
    ];
}

/// Nerd Font glyph for a row and the kind that colours it, chosen by extension.
/// One table, not two: a glyph and a colour keyed separately on the same
/// extension diverge the first time someone adds a language to one of them.
/// A lookup table — pinned by a unit test rather than a scenario, per AGENTS.md.
/// Which glyph an extension earns, as data. A table rather than a match: this
/// decides nothing, and the extensions one icon answers to are a list that
/// grows.
const ICONS: &[(&[&str], &str, IconKind)] = &[
    (&["rs"], "\u{e7a8}", IconKind::Rust),
    (
        &["js", "mjs", "cjs", "jsx"],
        "\u{e74e}",
        IconKind::JavaScript,
    ),
    (
        &["ts", "tsx", "mts", "cts"],
        "\u{e628}",
        IconKind::TypeScript,
    ),
    (&["py", "pyi", "pyw"], "\u{e73c}", IconKind::Python),
    (&["go"], "\u{e627}", IconKind::Go),
    (&["java", "kt", "kts"], "\u{e738}", IconKind::Java),
    (&["vue"], "\u{e6a0}", IconKind::Vue),
    (&["svelte"], "\u{e697}", IconKind::Svelte),
    (&["rb", "erb", "gemspec"], "\u{e739}", IconKind::Ruby),
    (&["php"], "\u{e73d}", IconKind::Php),
    (&["c", "h"], "\u{e61e}", IconKind::C),
    (
        &["cpp", "cc", "cxx", "hpp", "hh"],
        "\u{e61d}",
        IconKind::Cpp,
    ),
    (&["cs"], "\u{e648}", IconKind::CSharp),
    (&["swift"], "\u{e699}", IconKind::Swift),
    (&["sql"], "\u{e706}", IconKind::Sql),
    (&["html", "htm", "xml", "svg"], "\u{e736}", IconKind::Markup),
    (
        &["css", "scss", "sass", "less"],
        "\u{e749}",
        IconKind::Style,
    ),
    (&["json", "jsonc"], "\u{e60b}", IconKind::Data),
    (
        &["toml", "yaml", "yml", "ini", "cfg", "conf", "env"],
        "\u{e615}",
        IconKind::Data,
    ),
    (&["md", "markdown", "mdx"], "\u{f48a}", IconKind::Doc),
    (&["txt", "rst", "adoc"], "\u{f15c}", IconKind::Doc),
    (&["pdf"], "\u{f1c1}", IconKind::Pdf),
    (
        &["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "avif"],
        "\u{f1c5}",
        IconKind::Image,
    ),
    (
        &[
            "mp3", "wav", "flac", "ogg", "m4a", "mp4", "mov", "avi", "mkv", "webm",
        ],
        "\u{f1c8}",
        IconKind::Media,
    ),
    (
        &["zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar"],
        "\u{f1c6}",
        IconKind::Archive,
    ),
    (
        &["sh", "bash", "zsh", "fish", "ps1"],
        "\u{f489}",
        IconKind::Shell,
    ),
    (&["feature"], "\u{f0c3}", IconKind::Spec),
    (&["dockerfile"], "\u{e650}", IconKind::Docker),
    (&["lock"], "\u{f023}", IconKind::Lock),
];

/// Which glyph a *name* earns, checked before the extension. A project's
/// landmarks are named, not typed: `Dockerfile` and `LICENSE` have no extension
/// at all, a dotfile's "extension" is its whole name, and `package.json` says
/// far more about itself than `json` does. Without this table every one of them
/// drew the same blank page, which is most of what made the pane look flat.
/// Lowercase spellings only — `icon` folds the name before it looks, because
/// `Dockerfile`, `dockerfile`, `LICENSE` and `license` are all spelled both
/// ways in the wild and a table keyed on one misses the other in silence.
const NAMES: &[(&[&str], &str, IconKind)] = &[
    (
        &[
            "package.json",
            "package-lock.json",
            "yarn.lock",
            "pnpm-lock.yaml",
            "bun.lockb",
            ".npmrc",
            ".nvmrc",
        ],
        "\u{e71e}",
        IconKind::Package,
    ),
    (&["cargo.toml", "cargo.lock"], "\u{e7a8}", IconKind::Rust),
    (
        &["tsconfig.json", "tsconfig.base.json", "tsconfig.node.json"],
        "\u{e628}",
        IconKind::TypeScript,
    ),
    (
        &[
            "dockerfile",
            ".dockerignore",
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ],
        "\u{e650}",
        IconKind::Docker,
    ),
    (
        &[".gitignore", ".gitattributes", ".gitmodules", ".gitkeep"],
        "\u{e702}",
        IconKind::Git,
    ),
    (
        &["license", "license.md", "license.txt", "copying", "notice"],
        "\u{f0e3}",
        IconKind::Doc,
    ),
    (
        &[
            "makefile",
            "justfile",
            "cmakelists.txt",
            "rakefile",
            "gemfile",
        ],
        "\u{f085}",
        IconKind::Shell,
    ),
    (
        &[
            ".env",
            ".env.local",
            ".env.example",
            ".editorconfig",
            ".prettierrc",
            ".eslintrc",
            ".eslintrc.json",
            ".babelrc",
            ".stylelintrc",
        ],
        "\u{e615}",
        IconKind::Data,
    ),
];

/// Which colour a *folder* borrows. The glyph stays the folder's own, because
/// open or closed is the one thing a folder's glyph has to say; what it holds is
/// carried by the colour instead. That is what makes `features` read as tests
/// and `node_modules` as somebody else's code without a second glyph table and
/// without spending the one signal the folder glyph already owns.
const FOLDERS: &[(&[&str], IconKind)] = &[
    (&[".git", ".github", ".gitea"], IconKind::Git),
    (&["node_modules", "vendor"], IconKind::Package),
    (&["src", "lib", "app"], IconKind::Source),
    (
        &["target", "dist", "build", "out", "node_modules/.cache"],
        IconKind::Build,
    ),
    (
        &["features", "tests", "test", "spec", "specs", "__tests__"],
        IconKind::Spec,
    ),
    (&["docs", "doc"], IconKind::Doc),
    (
        &["assets", "images", "img", "static", "public"],
        IconKind::Image,
    ),
    (&["config", ".config", ".vscode", ".idea"], IconKind::Data),
    (&["scripts", "bin"], IconKind::Shell),
];

/// A file with no extension, or one no entry above claims.
const PLAIN: (&str, IconKind) = ("\u{f15b}", IconKind::Plain);

pub fn icon(name: &str, is_dir: bool, expanded: bool) -> (&'static str, IconKind) {
    let lowered = name.to_ascii_lowercase();
    if is_dir {
        let glyph = if expanded { "\u{f07c}" } else { "\u{f07b}" };
        let kind = FOLDERS
            .iter()
            .find(|(names, _)| names.contains(&lowered.as_str()))
            .map_or(IconKind::Directory, |(_, kind)| *kind);
        return (glyph, kind);
    }
    if let Some((_, glyph, kind)) = NAMES
        .iter()
        .find(|(names, _, _)| names.contains(&lowered.as_str()))
    {
        return (*glyph, *kind);
    }
    // A leading dot is a name, not an extension: ".gitignore" splits to
    // ("", "gitignore") and ".rs" to ("", "rs"), and neither is a typed file.
    let extension = match lowered.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => Some(extension),
        _ => None,
    };
    let Some(extension) = extension else {
        return PLAIN;
    };
    ICONS
        .iter()
        .find(|(names, _, _)| names.contains(&extension))
        .map_or(PLAIN, |(_, glyph, kind)| (*glyph, *kind))
}

#[cfg(test)]
mod tests {
    use super::{icon, row_actions, visible_rows, IconKind, FOLDERS, ICONS, NAMES, PLAIN};
    use crate::review::{GitFile, GitStatus};
    use crate::tree::Entry;
    use crate::{State, View};
    use std::path::PathBuf;

    /// The bug: the lookup read the unfiltered tree, so in Review view — where
    /// the rows are the changed files, none of which is in `contents` — it found
    /// nothing and no action icons rendered at all.
    #[test]
    fn actions_come_from_the_rows_on_screen() {
        let mut state = State {
            root: PathBuf::from("/w"),
            view: View::Review,
            repo: Some(vec![GitFile {
                path: "only.rs".to_string(),
                status: GitStatus::Modified,
            }]),
            ..State::default()
        };
        let selected = state.root.join("only.rs");
        state.tree_selection = Some(selected.clone());
        assert_eq!(row_actions(&state, &selected), vec!["delete", "copy-path"]);
    }

    /// A filtered tree is the same mechanism: the rows on screen are a subset,
    /// and a row filtered off screen offers nothing.
    #[test]
    fn a_row_the_filter_hid_offers_nothing() {
        let mut state = State {
            root: PathBuf::from("/w"),
            ..State::default()
        };
        state.contents.insert(
            state.root.clone(),
            vec![
                Entry {
                    name: "a.rs".to_string(),
                    is_dir: false,
                },
                Entry {
                    name: "b.rs".to_string(),
                    is_dir: false,
                },
            ],
        );
        let hidden = state.root.join("b.rs");
        state.tree_selection = Some(hidden.clone());
        state.filter = "a.rs".to_string();
        assert_eq!(row_actions(&state, &hidden), Vec::<&str>::new());
    }

    /// A big project is the whole point of a filter, and a one-letter needle
    /// matches most of it. Deduplicating the folder rows by scanning what had
    /// been pushed so far made that quadratic: 54k files took over two minutes
    /// per keystroke, which reads as the TUI freezing.
    #[test]
    fn a_broad_match_over_a_big_project_is_not_quadratic() {
        let mut state = State {
            root: PathBuf::from("/w"),
            filter: "c".to_string(),
            ..State::default()
        };
        state.indexed = (0..40)
            .flat_map(|a| {
                (0..40).flat_map(move |b| {
                    (0..20).map(move |c| format!("pkg{a}/src/mod{b}/component_{c}.tsx"))
                })
            })
            .collect();
        let started = std::time::Instant::now();
        let rows = visible_rows(&state);
        assert_eq!(rows.len(), 32_000 + 40 + 40 + 1_600);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn folders_show_whether_they_are_open() {
        assert_ne!(icon("whatever", true, false), icon("whatever", true, true));
        assert_eq!(icon("whatever", true, false).1, IconKind::Directory);
    }

    #[test]
    fn extension_decides_the_glyph() {
        assert_eq!(icon("main.rs", false, false), icon("lib.rs", false, false));
        assert_ne!(icon("main.rs", false, false), icon("app.js", false, false));
    }

    #[test]
    fn unknown_and_extensionless_files_fall_back() {
        let fallback = icon("NOTES", false, false);
        assert_eq!(icon("notes.xyz", false, false), fallback);
        assert_eq!(fallback.1, IconKind::Plain);
    }

    #[test]
    fn a_dotfile_is_not_treated_as_an_extension() {
        // ".gitignore" rsplits to ("", "gitignore"), which must not match a
        // language — it is a file called .gitignore, not a gitignore-typed file.
        // It earns its icon from NAMES instead, which is a different answer
        // from the one the extension table would have given.
        assert_ne!(icon(".gitignore", false, false).1, IconKind::Plain);
        // ".rs" is the case that bites once the table is wide: it is a file
        // named ".rs", not a Rust file, and no name claims it.
        assert_eq!(
            icon(".rs", false, false),
            icon("NOTES", false, false),
            "a bare extension is a name, and an unclaimed name is plain"
        );
        // A dotted *stem* is still a typed file.
        assert_eq!(
            icon(".hidden.rs", false, false).1,
            icon("main.rs", false, false).1
        );
    }

    /// The names table is what gives a project's landmarks the icon they have
    /// in an editor: each of these has either no extension to key on, or an
    /// extension that says less than the name does.
    #[test]
    fn a_known_name_beats_its_extension() {
        // package.json is not a config file, and Cargo.toml is not one either.
        assert_eq!(icon("package.json", false, false).1, IconKind::Package);
        assert_eq!(icon("Cargo.toml", false, false).1, IconKind::Rust);
        assert_eq!(icon("tsconfig.json", false, false).1, IconKind::TypeScript);
        assert_ne!(
            icon("package.json", false, false),
            icon("settings.json", false, false)
        );
        // Extensionless landmarks, which fell back to a blank page before.
        for name in ["Dockerfile", "Makefile", "LICENSE", ".env", ".gitignore"] {
            assert_ne!(
                icon(name, false, false).1,
                IconKind::Plain,
                "{name} is a landmark, not an unknown file"
            );
        }
    }

    /// A folder keeps the folder glyph — open or closed is the one thing its
    /// glyph has to say — and borrows the colour of what it holds. That is the
    /// whole of a colourful folder at one cell wide.
    #[test]
    fn a_known_folder_is_coloured_by_what_it_holds() {
        assert_eq!(icon("features", true, false).1, IconKind::Spec);
        assert_eq!(icon("node_modules", true, false).1, IconKind::Package);
        assert_eq!(icon("whatever", true, false).1, IconKind::Directory);
        // `src` is the folder every project has, so leaving it on the default
        // blue-grey is what made the pane read greyer than it used to, not
        // more colourful.
        assert_eq!(icon("src", true, false).1, IconKind::Source);
        assert_eq!(icon("target", true, false).1, IconKind::Build);
        // Open still differs from closed, special or not.
        assert_ne!(
            icon("features", true, false).0,
            icon("features", true, true).0
        );
        // And the glyph is the folder's, not its contents'.
        assert_eq!(
            icon("features", true, false).0,
            icon("whatever", true, false).0
        );
    }

    /// `ALL` is what the colour table at the edge sweeps for a clash, so a kind
    /// missing from it is a kind nobody checked.
    #[test]
    fn every_kind_a_table_can_yield_is_listed_in_all() {
        for (names, _, kind) in ICONS {
            assert!(
                IconKind::ALL.contains(kind),
                "{kind:?} ({names:?}) is missing from IconKind::ALL"
            );
        }
        for (names, _, kind) in NAMES {
            assert!(IconKind::ALL.contains(kind), "{kind:?} ({names:?})");
        }
        for (names, kind) in FOLDERS {
            assert!(IconKind::ALL.contains(kind), "{kind:?} ({names:?})");
        }
        assert!(IconKind::ALL.contains(&PLAIN.1));
        assert!(IconKind::ALL.contains(&IconKind::Directory));
    }

    #[test]
    fn a_kind_travels_with_its_glyph() {
        // The pairing is the point: one table, so a language cannot have a
        // glyph without a kind.
        for (name, kind) in [
            ("a.py", IconKind::Python),
            ("a.go", IconKind::Go),
            ("a.java", IconKind::Java),
            ("a.ts", IconKind::TypeScript),
            ("a.css", IconKind::Style),
            ("a.png", IconKind::Image),
            ("a.yaml", IconKind::Data),
            ("a.yml", IconKind::Data),
            ("a.json", IconKind::Data),
            ("a.txt", IconKind::Doc),
        ] {
            assert_eq!(icon(name, false, false).1, kind, "{name}");
        }
    }

    #[test]
    fn kinds_that_share_a_glyph_still_share_a_colour() {
        // yaml and toml are one glyph and one kind; json is its own glyph but
        // the same kind, so config reads as config whatever the syntax.
        assert_eq!(icon("a.yml", false, false), icon("a.toml", false, false));
        assert_ne!(
            icon("a.json", false, false).0,
            icon("a.yml", false, false).0
        );
        assert_eq!(
            icon("a.json", false, false).1,
            icon("a.yml", false, false).1
        );
    }
}
