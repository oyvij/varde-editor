use crate::{Effect, State};
use std::sync::Arc;

pub fn score(needle: &str, haystack: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.to_lowercase().chars().collect();
    let mut points = 0;
    let mut at = 0;
    let mut previous: Option<usize> = None;
    for wanted in needle.to_lowercase().chars() {
        let found = hay[at..].iter().position(|c| *c == wanted)? + at;
        points += match previous {
            Some(last) if found == last + 1 => 8,
            Some(last) => -((found - last) as i32).min(4),
            None => 0,
        };
        previous = Some(found);
        at = found + 1;
    }
    Some(points - (haystack.len() as i32 / 8))
}

pub fn rank(needle: &str, path: &str) -> Option<i32> {
    let name = path.rsplit('/').next().unwrap_or(path);
    match score(needle, name) {
        Some(points) => {
            let start = name.to_lowercase().starts_with(&needle.to_lowercase());
            Some(points + 12 + if start { 12 } else { 0 })
        }
        None => score(needle, path),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub walk: u64,
    pub walking: bool,
    pub files: Arc<Vec<String>>,
    pub ranked: Arc<Vec<Ranked>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranked {
    pub literal: bool,
    pub points: i32,
    pub path: String,
}

pub fn narrow(next: &mut State, text: String) -> Vec<Effect> {
    let opening = next.filter.is_empty() && !text.is_empty();
    next.filter = text;
    let walk = next.index.walk;
    if next.filter.is_empty() {
        next.index = Index {
            walk,
            ..Index::default()
        };
        return vec![];
    }
    if opening {
        next.index = Index {
            walk: walk + 1,
            walking: true,
            ..Index::default()
        };
        return vec![Effect::IndexProject { walk: walk + 1 }];
    }
    next.index.ranked = Arc::new(ranked(&next.filter, &next.index.files));
    vec![]
}

pub fn indexed(next: &mut State, walk: u64, files: Vec<String>, done: bool) {
    if walk != next.index.walk || !next.index.walking {
        return;
    }
    let mut arrived = ranked(&next.filter, &files);
    let ranked = Arc::make_mut(&mut next.index.ranked);
    ranked.append(&mut arrived);
    ranked.sort_by(order);
    Arc::make_mut(&mut next.index.files).extend(files);
    next.index.walking = !done;
}

fn ranked(needle: &str, files: &[String]) -> Vec<Ranked> {
    let typed = needle.to_lowercase();
    let mut ranked: Vec<Ranked> = files
        .iter()
        .filter_map(|path| {
            rank(needle, path).map(|points| Ranked {
                literal: path.to_lowercase().contains(&typed),
                points,
                path: path.clone(),
            })
        })
        .collect();
    ranked.sort_by(order);
    ranked
}

fn order(a: &Ranked, b: &Ranked) -> std::cmp::Ordering {
    b.literal
        .cmp(&a.literal)
        .then_with(|| b.points.cmp(&a.points))
        .then_with(|| a.path.cmp(&b.path))
}

pub fn view_state(state: &State) -> &'static str {
    if state.filter.is_empty() {
        "unfiltered"
    } else if best(state).is_some() {
        "matches"
    } else if state.index.walking {
        "walking"
    } else {
        "no-matches"
    }
}

pub fn matches(state: &State) -> impl Iterator<Item = &str> {
    let literal = state
        .index
        .ranked
        .first()
        .is_some_and(|first| first.literal);
    state
        .index
        .ranked
        .iter()
        .take_while(move |found| found.literal || !literal)
        .map(|found| found.path.as_str())
}

pub fn best(state: &State) -> Option<&str> {
    matches(state).next()
}

#[cfg(test)]
mod tests {
    use super::Index;
    use crate::{update, Effect, Event, State};
    use std::sync::Arc;

    fn filtered(needle: &str) -> State {
        update(&State::default(), Event::Filter(needle.to_string())).0
    }

    fn walked(state: &State, walk: u64, files: &[&str], done: bool) -> State {
        let files = files.iter().map(|path| path.to_string()).collect();
        update(state, Event::Indexed { walk, files, done }).0
    }

    fn shown(state: &State) -> Vec<&str> {
        super::matches(state).collect()
    }

    #[test]
    fn a_literal_match_is_literal_in_any_case() {
        let state = walked(
            &filtered("TODO.md"),
            1,
            &["docs/todo.md", "notes/the-old-story.md"],
            true,
        );
        assert_eq!(shown(&state), vec!["docs/todo.md"]);
    }

    #[test]
    fn opening_the_filter_asks_for_one_walk_and_waits_on_nothing() {
        let (opened, effects) = update(&State::default(), Event::Filter("m".to_string()));
        assert_eq!(effects, vec![Effect::IndexProject { walk: 1 }]);
        assert!(opened.index.walking);
        assert_eq!(super::view_state(&opened), "walking", "not yet no-matches");
        let (typed, effects) = update(&opened, Event::Filter("ma".to_string()));
        assert!(effects.is_empty(), "a keystroke started a second walk");
        assert_eq!(typed.index.walk, 1);
    }

    #[test]
    fn each_batch_is_ranked_with_what_is_already_there() {
        let first = walked(&filtered("main"), 1, &["src/main.rs", "README.md"], false);
        assert_eq!(shown(&first), vec!["src/main.rs"]);
        assert!(first.index.walking, "the walk is still running");
        let second = walked(&first, 1, &["main.rs"], true);
        assert_eq!(shown(&second), vec!["main.rs", "src/main.rs"]);
        assert!(!second.index.walking, "the spinner outlived the walk");
    }

    #[test]
    fn a_batch_from_a_cancelled_walk_is_dropped() {
        let opened = filtered("main");
        let closed = update(&opened, Event::Filter(String::new())).0;
        assert_eq!(
            walked(&closed, 1, &["src/main.rs"], false).index,
            closed.index
        );
        let reopened = update(&closed, Event::Filter("main".to_string())).0;
        assert_eq!(reopened.index.walk, 2);
        let late = walked(&reopened, 1, &["src/main.rs"], true);
        assert!(
            shown(&late).is_empty(),
            "the cancelled walk's file was shown"
        );
        assert!(
            late.index.walking,
            "the cancelled walk ended the current one"
        );
    }

    #[test]
    fn a_finished_walk_takes_no_more_files() {
        let done = walked(&filtered("main"), 1, &[], true);
        assert!(shown(&walked(&done, 1, &["src/main.rs"], false)).is_empty());
    }

    #[test]
    fn a_new_query_ranks_the_files_already_walked() {
        let walked = walked(&filtered("main"), 1, &["src/main.rs", "src/lib.rs"], true);
        let retyped = update(&walked, Event::Filter("lib".to_string())).0;
        assert_eq!(shown(&retyped), vec!["src/lib.rs"]);
    }

    #[test]
    fn what_is_shown_is_the_stored_ranking_not_a_fresh_one() {
        let state = State {
            filter: "zzz".to_string(),
            index: Index {
                ranked: Arc::new(vec![super::Ranked {
                    literal: true,
                    points: 0,
                    path: "src/main.rs".to_string(),
                }]),
                ..Index::default()
            },
            ..State::default()
        };
        assert_eq!(shown(&state), vec!["src/main.rs"]);
        assert_eq!(super::best(&state), Some("src/main.rs"));
        assert_eq!(super::view_state(&state), "matches");
        assert!(crate::tree::visible_rows(&state)
            .iter()
            .any(|row| row.path.ends_with("src/main.rs")));
    }
}
