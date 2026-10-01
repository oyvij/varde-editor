use crate::State;

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

pub fn view_state(state: &State) -> &'static str {
    if state.filter.is_empty() {
        "unfiltered"
    } else if matches(state).is_empty() {
        "no-matches"
    } else {
        "matches"
    }
}

pub fn matches(state: &State) -> Vec<String> {
    let needle = &state.filter;
    if needle.is_empty() {
        return Vec::new();
    }
    let typed = needle.to_lowercase();
    let mut ranked: Vec<(bool, i32, &String)> = state
        .indexed
        .iter()
        .filter_map(|path| {
            let literal = path.to_lowercase().contains(&typed);
            rank(needle, path).map(|points| (literal, points, path))
        })
        .collect();
    if ranked.iter().any(|(literal, ..)| *literal) {
        ranked.retain(|(literal, ..)| *literal);
    }
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(b.2)));
    ranked.into_iter().map(|(.., path)| path.clone()).collect()
}

pub fn best(state: &State) -> Option<String> {
    matches(state).into_iter().next()
}

#[cfg(test)]
mod tests {
    use crate::State;

    fn narrowed_by(needle: &str, files: &[&str]) -> Vec<String> {
        let state = State {
            filter: needle.to_string(),
            indexed: files.iter().map(|path| path.to_string()).collect(),
            ..State::default()
        };
        super::matches(&state)
    }

    #[test]
    fn a_literal_match_is_literal_in_any_case() {
        assert_eq!(
            narrowed_by("TODO.md", &["docs/todo.md", "notes/the-old-story.md"]),
            vec!["docs/todo.md".to_string()]
        );
    }
}
