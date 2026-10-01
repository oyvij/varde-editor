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
