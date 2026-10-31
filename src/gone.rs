use crate::{risk, State, Tone, View};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

pub const GLYPH: &str = "\u{25cc}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Function,
    Method,
    Struct,
    Class,
    Trait,
    Interface,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub file: String,
    pub container: String,
    pub name: String,
    pub kind: Kind,
    pub signature: String,
    pub body: u64,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Gone,
}

impl Label {
    pub fn as_str(self) -> &'static str {
        match self {
            Label::Gone => "gone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub label: Label,
    pub item: Item,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    pub root: PathBuf,
    pub rows: Vec<Row>,
}

impl Comparison {
    pub fn per_file(&self) -> BTreeMap<&str, usize> {
        let mut counts = BTreeMap::new();
        for row in &self.rows {
            *counts.entry(row.item.file.as_str()).or_insert(0) += 1;
        }
        counts
    }

    pub fn per_root(&self) -> BTreeMap<&Path, usize> {
        BTreeMap::from([(self.root.as_path(), self.rows.len())])
    }
}

pub fn cut(text: &str, name: &str) -> (String, u64) {
    let word = |letter: char| letter.is_alphanumeric() || letter == '_';
    let after_name = text
        .match_indices(name)
        .find(|(at, _)| {
            !text[..*at].chars().next_back().is_some_and(word)
                && !text[at + name.len()..].chars().next().is_some_and(word)
        })
        .map_or(0, |(at, _)| at + name.len());
    let rest = &text[after_name..];
    let (head, body) = match rest.find('{') {
        Some(at) => rest.split_at(at),
        None => rest.split_once('\n').unwrap_or((rest, "")),
    };
    let signature = head.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hasher = DefaultHasher::new();
    body.split_whitespace()
        .for_each(|piece| piece.hash(&mut hasher));
    (
        signature.trim_end_matches(':').trim_end().to_string(),
        hasher.finish(),
    )
}

pub fn items(analysed: &[(String, Option<risk::Space>)]) -> Vec<Item> {
    let mut found = Vec::new();
    for (file, space) in analysed {
        if let Some(space) = space {
            walk(file, "", risk::Kind::Unit, space, &mut found);
        }
    }
    found
}

fn walk(
    file: &str,
    container: &str,
    within: risk::Kind,
    space: &risk::Space,
    found: &mut Vec<Item>,
) {
    let kind = match (space.kind, within) {
        (
            risk::Kind::Function,
            risk::Kind::Impl
            | risk::Kind::Class
            | risk::Kind::Struct
            | risk::Kind::Trait
            | risk::Kind::Interface,
        ) => Some(Kind::Method),
        (risk::Kind::Function, _) => Some(Kind::Function),
        (risk::Kind::Struct, _) => Some(Kind::Struct),
        (risk::Kind::Class, _) => Some(Kind::Class),
        (risk::Kind::Trait, _) => Some(Kind::Trait),
        (risk::Kind::Interface, _) => Some(Kind::Interface),
        _ => None,
    };
    if let (Some(kind), Some(name)) = (kind, &space.name) {
        found.push(Item {
            file: file.to_string(),
            container: container.to_string(),
            name: name.clone(),
            kind,
            signature: space.signature.clone(),
            body: space.body,
            line: space.line,
        });
    }
    if space.kind == risk::Kind::Function {
        return;
    }
    let inner = match space.kind {
        risk::Kind::Unit => container,
        _ => space.name.as_deref().unwrap_or(container),
    };
    for child in &space.children {
        walk(file, inner, space.kind, child, found);
    }
}

pub fn compare(root: &Path, was: &[Item], now: &[Item]) -> Comparison {
    let rows = was
        .iter()
        .filter(|old| {
            !now.iter().any(|new| {
                new.file == old.file && new.container == old.container && new.name == old.name
            })
        })
        .map(|old| Row {
            label: Label::Gone,
            item: old.clone(),
        })
        .collect();
    Comparison {
        root: root.to_path_buf(),
        rows,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Found {
    #[default]
    None,
    Current(Comparison),
    Stale(Comparison),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Gone {
    pub found: Found,
    pub answered: u64,
}

pub fn arrived(state: &mut State, generation: u64, was: Vec<Item>, now: Vec<Item>) {
    if generation != state.risk.asked || state.risk.scope != risk::Scope::Review {
        return;
    }
    state.gone.answered = generation;
    state.gone.found = Found::Current(compare(state.repo_root(), &was, &now));
}

pub fn went_stale(gone: &mut Gone) {
    if let Found::Current(comparison) = &gone.found {
        gone.found = Found::Stale(comparison.clone());
    }
}

fn computing(state: &State) -> bool {
    state.risk.scope == risk::Scope::Review && state.gone.answered < state.risk.asked
}

pub fn view_state(state: &State) -> &'static str {
    if computing(state) {
        return "computing";
    }
    match state.gone.found {
        Found::None => "none",
        Found::Current(_) => "current",
        Found::Stale(_) => "stale",
    }
}

pub fn rows(state: &State) -> &[Row] {
    match &state.gone.found {
        Found::None => &[],
        Found::Current(comparison) | Found::Stale(comparison) => &comparison.rows,
    }
}

pub fn shown(state: &State) -> Option<usize> {
    let count = rows(state).len();
    (state.view == View::Review && !computing(state) && count > 0).then_some(count)
}

pub fn chip(state: &State) -> Option<(Tone, String)> {
    let count = shown(state)?;
    let (tone, said) = match state.gone.found {
        Found::Stale(_) => (Tone::Dimmed, format!("gone {count} stale")),
        Found::Current(_) | Found::None => (Tone::Plain, format!("gone {count}")),
    };
    Some((tone, format!(" {GLYPH} {said} \u{b7} x :gone ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(file: &str, container: &str, name: &str) -> Item {
        Item {
            file: file.to_string(),
            container: container.to_string(),
            name: name.to_string(),
            kind: Kind::Function,
            signature: "()".to_string(),
            body: 1,
            line: 1,
        }
    }

    #[test]
    fn an_item_in_the_old_revision_and_not_the_new_one_is_gone() {
        let was = [
            item("src/keys.rs", "", "route"),
            item("src/keys.rs", "Router", "spell"),
        ];
        let now = [
            item("src/keys.rs", "", "route"),
            item("src/keys.rs", "", "spell"),
        ];

        let compared = compare(Path::new("/repo"), &was, &now);

        assert_eq!(
            compared.rows,
            vec![Row {
                label: Label::Gone,
                item: item("src/keys.rs", "Router", "spell"),
            }]
        );
    }

    #[test]
    fn the_rows_are_counted_per_file_and_per_root() {
        let was = [
            item("src/keys.rs", "", "route"),
            item("src/keys.rs", "", "spell"),
            item("src/ui.rs", "", "draw"),
            item("src/tree.rs", "", "icon"),
        ];
        let now = [item("src/tree.rs", "", "icon")];

        let compared = compare(Path::new("/repo"), &was, &now);

        assert_eq!(
            compared.per_file(),
            BTreeMap::from([("src/keys.rs", 2), ("src/ui.rs", 1)])
        );
        assert_eq!(
            compared.per_root(),
            BTreeMap::from([(Path::new("/repo"), 3)])
        );
    }

    fn reviewing(found: impl Fn(Comparison) -> Found) -> State {
        let mut state = State {
            view: View::Review,
            ..State::default()
        };
        state.risk.scope = risk::Scope::Review;
        state.gone.found = found(compare(
            Path::new("/repo"),
            &[item("src/keys.rs", "", "spell")],
            &[],
        ));
        state
    }

    #[test]
    fn the_chip_names_the_count_and_its_keys() {
        assert_eq!(
            chip(&reviewing(Found::Current)),
            Some((Tone::Plain, " \u{25cc} gone 1 \u{b7} x :gone ".to_string()))
        );
    }

    #[test]
    fn a_stale_chip_says_so_and_is_dimmed() {
        let (tone, text) = chip(&reviewing(Found::Stale)).expect("a chip");
        assert_eq!(tone, Tone::Dimmed);
        assert!(text.contains("gone 1 stale"), "{text}");
    }
}
