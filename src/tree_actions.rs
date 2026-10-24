use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    GoHere,
    NewFile,
    NewDirectory,
    Delete,
    SearchHere,
    BackToRoot,
    CopyPath,
    OpenExternally,
    Undo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    File(PathBuf),
    Folder(PathBuf),
}

pub(crate) fn create(action: Action, base: &Path, path: &Path) -> String {
    match action {
        Action::NewDirectory => command("mkdir -p", path),
        _ => match path.parent() {
            Some(parent) if parent != base => {
                format!(
                    "{} && {}",
                    command("mkdir -p", parent),
                    command("touch", path)
                )
            }
            _ => command("touch", path),
        },
    }
}

pub(crate) fn command(verb: &str, path: &Path) -> String {
    format!(
        "{verb} {}",
        shlex::try_quote(&path.to_string_lossy()).expect("path has no NUL")
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Trash {
    pub dir: PathBuf,
    pub issued: usize,
    pub pending: Vec<Trashed>,
    pub undoable: Vec<Trashed>,
    pub restoring: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trashed {
    pub original: PathBuf,
    pub trashed: PathBuf,
}

impl Trash {
    pub(crate) fn delete(&mut self, original: &Path) -> String {
        self.issued += 1;
        let name = original.file_name().unwrap_or_default().to_string_lossy();
        let trashed = self.dir.join(format!("{}-{name}", self.issued));
        let moved = format!(
            "{} && {}",
            command("mkdir -p", &self.dir),
            command(&command("mv", original), &trashed)
        );
        self.pending.push(Trashed {
            original: original.to_path_buf(),
            trashed,
        });
        moved
    }

    pub(crate) fn landed(&mut self, removed: &[PathBuf]) -> Vec<PathBuf> {
        let (gone, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|trashed| removed.contains(&trashed.original));
        self.pending = waiting;
        let originals = gone
            .iter()
            .map(|trashed| trashed.original.clone())
            .collect();
        self.undoable.extend(gone);
        originals
    }

    pub(crate) fn restore(&mut self) -> Option<String> {
        let newest = self.undoable.last()?;
        self.restoring = Some(newest.original.clone());
        let parent = newest.original.parent().unwrap_or(Path::new("/"));
        Some(format!(
            "{} && {}",
            command("mkdir -p", parent),
            command(&command("mv -n", &newest.trashed), &newest.original)
        ))
    }

    pub(crate) fn came_back(&mut self, appeared: &[(PathBuf, crate::tree::Kind)]) {
        let Some(original) = self
            .restoring
            .take_if(|original| appeared.iter().any(|(path, _)| path == original))
        else {
            return;
        };
        if let Some(at) = self
            .undoable
            .iter()
            .rposition(|trashed| trashed.original == original)
        {
            self.undoable.remove(at);
        }
    }
}

pub const UNDO: &str = "undo";

pub fn transport(state: &crate::State) -> Vec<crate::Chip> {
    vec![crate::Chip {
        action: UNDO,
        name: "undo",
        glyph: "\u{21b6}".to_string(),
        keys: "u",
        word: "",
        hue: crate::Hue::Plain,
        tone: match state.trash.undoable.is_empty() {
            true => crate::Tone::Dimmed,
            false => crate::Tone::Lit,
        },
    }]
}

pub fn transport_labels(state: &crate::State, width: u16) -> Vec<String> {
    let title: usize = std::iter::once(crate::tree::title(state))
        .chain(crate::lsp::nudge(state).into_iter().map(|(_, label)| label))
        .map(|label| unicode_width::UnicodeWidthStr::width(label.as_str()))
        .sum();
    crate::layout::chip_labels(&transport(state), width, title as u16 + 2)
}
