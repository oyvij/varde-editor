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
    pub body_hash: u64,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Gone,
    Moved,
    SignatureChanged,
}

impl Label {
    pub fn as_str(self) -> &'static str {
        match self {
            Label::Gone => "gone",
            Label::Moved => "moved",
            Label::SignatureChanged => "signature changed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub label: Label,
    pub item: Item,
    pub now: Option<Item>,
}

pub fn sentence(row: &Row) -> String {
    match (row.label, &row.now) {
        (Label::Moved, Some(now)) => format!("same body now lives in {}", now.file),
        (Label::SignatureChanged, _) => {
            "still here; its parameters or return type changed".to_string()
        }
        (Label::Gone, _) | (Label::Moved, None) => {
            "existed in HEAD, not in this file now".to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    pub root: PathBuf,
    pub rows: Vec<Row>,
}

impl Comparison {
    pub fn per_file(&self) -> BTreeMap<&str, usize> {
        let mut counts = BTreeMap::new();
        for row in self.rows.iter().filter(|row| row.label == Label::Gone) {
            *counts.entry(row.item.file.as_str()).or_insert(0) += 1;
        }
        counts
    }

    pub fn per_root(&self) -> BTreeMap<&Path, usize> {
        BTreeMap::from([(self.root.as_path(), self.per_file().values().sum())])
    }
}

pub fn signature_and_body_hash(text: &str, name: &str) -> (String, u64) {
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
            body_hash: space.body_hash,
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
    let same_identity = |old: &Item, new: &Item| {
        new.file == old.file && new.container == old.container && new.name == old.name
    };
    let mut partners: Vec<Option<&Item>> = vec![None; was.len()];
    let mut taken = vec![false; now.len()];
    for by_body in [true, false] {
        for (old, partner) in was.iter().zip(partners.iter_mut()) {
            if partner.is_some() {
                continue;
            }
            let found = (0..now.len()).find(|&at| {
                !taken[at]
                    && same_identity(old, &now[at])
                    && (!by_body || now[at].body_hash == old.body_hash)
            });
            if let Some(at) = found {
                taken[at] = true;
                *partner = Some(&now[at]);
            }
        }
    }
    let mut rows = Vec::new();
    for (old, partner) in was.iter().zip(partners) {
        let elsewhere = |new: &Item| new.name == old.name && !same_identity(old, new);
        let (label, found) = match partner {
            Some(new) if new.signature != old.signature => (Label::SignatureChanged, Some(new)),
            Some(_) => continue,
            None => match (0..now.len())
                .find(|&at| !taken[at] && elsewhere(&now[at]) && now[at].body_hash == old.body_hash)
            {
                Some(at) => {
                    taken[at] = true;
                    (Label::Moved, Some(&now[at]))
                }
                None => (
                    Label::Gone,
                    now.iter()
                        .find(|new| elsewhere(new) && new.body_hash != old.body_hash),
                ),
            },
        };
        rows.push(Row {
            label,
            item: old.clone(),
            now: found.cloned(),
        });
    }
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
    let count = rows(state)
        .iter()
        .filter(|row| row.label == Label::Gone)
        .count();
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
            body_hash: 1,
            line: 1,
        }
    }

    #[test]
    fn an_item_in_the_old_revision_and_not_the_new_one_is_gone() {
        let was = [
            item("src/keys.rs", "", "route"),
            item("src/keys.rs", "Router", "spell"),
        ];
        let now = [item("src/keys.rs", "", "route")];

        let compared = compare(Path::new("/repo"), &was, &now);

        assert_eq!(
            compared.rows,
            vec![Row {
                label: Label::Gone,
                item: item("src/keys.rs", "Router", "spell"),
                now: None,
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

    fn with(item: Item, signature: &str, body_hash: u64, line: usize) -> Item {
        Item {
            signature: signature.to_string(),
            body_hash,
            line,
            ..item
        }
    }

    fn row(label: Label, item: Item, now: Option<Item>) -> Row {
        Row { label, item, now }
    }

    #[test]
    fn an_unchanged_item_and_one_whose_body_alone_changed_are_not_listed() {
        let was = [
            item("src/keys.rs", "", "route"),
            item("src/keys.rs", "", "spell"),
        ];
        let now = [
            item("src/keys.rs", "", "route"),
            with(item("src/keys.rs", "", "spell"), "()", 2, 9),
        ];

        assert_eq!(compare(Path::new("/repo"), &was, &now).rows, vec![]);
    }

    #[test]
    fn an_item_whose_body_now_lives_in_another_file_is_moved() {
        let was = [item("src/keys.rs", "", "spell")];
        let now = [item("src/ui.rs", "", "spell")];

        assert_eq!(
            compare(Path::new("/repo"), &was, &now).rows,
            vec![row(
                Label::Moved,
                item("src/keys.rs", "", "spell"),
                Some(item("src/ui.rs", "", "spell")),
            )]
        );
    }

    #[test]
    fn an_item_whose_body_now_lives_in_another_container_is_moved() {
        let was = [item("src/keys.rs", "Router", "spell")];
        let now = [item("src/keys.rs", "Speller", "spell")];

        assert_eq!(
            compare(Path::new("/repo"), &was, &now).rows,
            vec![row(
                Label::Moved,
                item("src/keys.rs", "Router", "spell"),
                Some(item("src/keys.rs", "Speller", "spell")),
            )]
        );
    }

    #[test]
    fn a_deleted_copy_of_a_body_its_namesake_still_holds_is_gone() {
        let was = [
            item("src/keys.rs", "", "spell"),
            item("src/ui.rs", "", "spell"),
        ];
        let now = [item("src/ui.rs", "", "spell")];

        assert_eq!(
            compare(Path::new("/repo"), &was, &now).rows,
            vec![row(Label::Gone, item("src/keys.rs", "", "spell"), None)]
        );
    }

    #[test]
    fn two_items_cannot_move_into_one() {
        let was = [
            item("src/keys.rs", "", "spell"),
            item("src/tree.rs", "", "spell"),
        ];
        let now = [item("src/ui.rs", "", "spell")];

        assert_eq!(
            compare(Path::new("/repo"), &was, &now).rows,
            vec![
                row(
                    Label::Moved,
                    item("src/keys.rs", "", "spell"),
                    Some(item("src/ui.rs", "", "spell")),
                ),
                row(Label::Gone, item("src/tree.rs", "", "spell"), None),
            ]
        );
    }

    #[test]
    fn an_item_moved_and_edited_is_gone_and_names_where_its_namesake_lives() {
        let was = [item("src/keys.rs", "", "spell")];
        let elsewhere = with(item("src/ui.rs", "", "spell"), "()", 2, 30);

        assert_eq!(
            compare(Path::new("/repo"), &was, std::slice::from_ref(&elsewhere)).rows,
            vec![row(
                Label::Gone,
                item("src/keys.rs", "", "spell"),
                Some(elsewhere),
            )]
        );
    }

    #[test]
    fn an_item_whose_parameters_changed_is_signature_changed() {
        let was = [item("src/keys.rs", "", "spell")];
        let changed = with(item("src/keys.rs", "", "spell"), "(word: &str)", 2, 1);

        assert_eq!(
            compare(Path::new("/repo"), &was, std::slice::from_ref(&changed)).rows,
            vec![row(
                Label::SignatureChanged,
                item("src/keys.rs", "", "spell"),
                Some(changed),
            )]
        );
    }

    #[test]
    fn overloads_pair_by_body_before_source_order() {
        let first = with(item("src/shape.cpp", "Shape", "area"), "(int)", 1, 10);
        let second = with(item("src/shape.cpp", "Shape", "area"), "(double)", 2, 20);
        let now = [
            with(item("src/shape.cpp", "Shape", "area"), "(double)", 2, 10),
            with(item("src/shape.cpp", "Shape", "area"), "(int)", 1, 20),
        ];

        assert_eq!(
            compare(Path::new("/repo"), &[first, second], &now).rows,
            vec![]
        );
    }

    #[test]
    fn overloads_whose_bodies_changed_pair_in_source_order() {
        let first = with(item("src/shape.cpp", "Shape", "area"), "(int)", 1, 10);
        let second = with(item("src/shape.cpp", "Shape", "area"), "(double)", 2, 20);
        let now = [
            with(item("src/shape.cpp", "Shape", "area"), "(int)", 3, 10),
            with(item("src/shape.cpp", "Shape", "area"), "(float)", 4, 20),
        ];

        assert_eq!(
            compare(Path::new("/repo"), &[first, second.clone()], &now).rows,
            vec![row(Label::SignatureChanged, second, Some(now[1].clone()))]
        );
    }

    #[test]
    fn an_old_overload_left_unpaired_is_gone() {
        let first = with(item("src/shape.cpp", "Shape", "area"), "(int)", 1, 10);
        let second = with(item("src/shape.cpp", "Shape", "area"), "(double)", 2, 20);
        let now = [with(
            item("src/shape.cpp", "Shape", "area"),
            "(double)",
            2,
            10,
        )];

        assert_eq!(
            compare(Path::new("/repo"), &[first.clone(), second], &now).rows,
            vec![row(Label::Gone, first, None)]
        );
    }

    #[test]
    fn only_gone_rows_are_counted() {
        let was = [
            item("src/keys.rs", "", "spell"),
            item("src/keys.rs", "", "route"),
            item("src/ui.rs", "", "draw"),
        ];
        let now = [
            item("src/tree.rs", "", "spell"),
            with(item("src/keys.rs", "", "route"), "(key: Key)", 1, 1),
        ];

        let compared = compare(Path::new("/repo"), &was, &now);

        assert_eq!(compared.rows.len(), 3);
        assert_eq!(compared.per_file(), BTreeMap::from([("src/ui.rs", 1)]));
        assert_eq!(
            compared.per_root(),
            BTreeMap::from([(Path::new("/repo"), 1)])
        );
    }

    #[test]
    fn each_label_has_its_word() {
        assert_eq!(Label::Gone.as_str(), "gone");
        assert_eq!(Label::Moved.as_str(), "moved");
        assert_eq!(Label::SignatureChanged.as_str(), "signature changed");
    }

    #[test]
    fn each_label_has_its_sentence() {
        let was = item("src/keys.rs", "", "spell");
        let now = item("src/ui.rs", "", "spell");

        assert_eq!(
            sentence(&row(Label::Gone, was.clone(), None)),
            "existed in HEAD, not in this file now"
        );
        assert_eq!(
            sentence(&row(Label::Moved, was.clone(), Some(now.clone()))),
            "same body now lives in src/ui.rs"
        );
        assert_eq!(
            sentence(&row(Label::SignatureChanged, was, Some(now))),
            "still here; its parameters or return type changed"
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
        assert_eq!(
            chip(&reviewing(Found::Stale)),
            Some((
                Tone::Dimmed,
                " \u{25cc} gone 1 stale \u{b7} x :gone ".to_string()
            ))
        );
    }
}
