use crate::{Direction, State};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Results {
    pub hits: Vec<Hit>,
    pub truncated: bool,
}

pub const CAP: usize = 500;

pub fn sources(state: &State, disk: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut sources = disk;
    for (path, buffer) in &state.buffers {
        let Ok(relative) = path.strip_prefix(&state.root) else {
            continue;
        };
        let relative = relative.to_string_lossy().into_owned();
        let contents = buffer.shown().to_string();
        match sources.iter_mut().find(|(name, _)| *name == relative) {
            Some(entry) => entry.1 = contents,
            None => sources.push((relative, contents)),
        }
    }
    if let Some(scope) = state.search.as_ref().and_then(|s| s.scope.as_ref()) {
        sources.retain(|(name, _)| std::path::Path::new(name).starts_with(scope));
    }
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    sources
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Smart,
    Exact,
    Ignore,
}

impl Case {
    pub fn exact(self, query: &str) -> bool {
        match self {
            Case::Smart => query.chars().any(char::is_uppercase),
            Case::Exact => true,
            Case::Ignore => false,
        }
    }
}

pub fn occurrences(query: &str, line: &str, case: Case) -> Vec<u32> {
    if query.is_empty() {
        return Vec::new();
    }
    let (needle, haystack) = if case.exact(query) {
        (query.to_string(), line.to_string())
    } else {
        (query.to_lowercase(), line.to_lowercase())
    };
    let mut columns = Vec::new();
    let mut from = 0;
    while let Some(at) = haystack[from..].find(&needle) {
        let start = from + at;
        columns.push(haystack[..start].chars().count() as u32 + 1);
        from = start + needle.len();
    }
    columns
}

pub fn scan(query: &str, files: &[(String, String)]) -> Results {
    let mut results = Results::default();
    for (path, contents) in files {
        for (index, line) in contents.split('\n').enumerate() {
            let Some(&column) = occurrences(query, line, Case::Smart).first() else {
                continue;
            };
            if results.hits.len() == CAP {
                results.truncated = true;
                return results;
            }
            results.hits.push(Hit {
                file: path.clone(),
                line: index as u32 + 1,
                column,
                text: line.trim_end().to_string(),
            });
        }
    }
    results
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row<'a> {
    File(&'a str),
    Hit(usize),
}

pub fn rows(results: &Results) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    let mut current = "";
    for (index, hit) in results.hits.iter().enumerate() {
        if hit.file != current {
            current = &hit.file;
            rows.push(Row::File(&hit.file));
        }
        rows.push(Row::Hit(index));
    }
    rows
}

pub fn in_next_file(results: &Results, selected: usize, direction: Direction) -> usize {
    let starts = || {
        results
            .hits
            .iter()
            .enumerate()
            .filter(|(index, hit)| *index == 0 || results.hits[index - 1].file != hit.file)
            .map(|(index, _)| index)
    };
    match direction {
        Direction::Down | Direction::Right => starts().find(|start| *start > selected),
        _ => starts().rfind(|start| *start < selected),
    }
    .unwrap_or(selected)
}

pub fn files(results: &Results) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for hit in &results.hits {
        if !seen.contains(&hit.file) {
            seen.push(hit.file.clone());
        }
    }
    seen
}

fn candidates(query: &str, results: &Results) -> Vec<(String, usize)> {
    let lower = query.to_lowercase();
    let mut counts: Vec<(String, usize)> = Vec::new();
    for hit in &results.hits {
        for token in hit
            .text
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|token| token.len() > query.len())
            .filter(|token| token.to_lowercase().starts_with(&lower))
        {
            match counts.iter_mut().find(|(word, _)| word == token) {
                Some(entry) => entry.1 += 1,
                None => counts.push((token.to_string(), 1)),
            }
        }
    }
    counts
}

pub fn completion(query: &str, results: &Results) -> Option<String> {
    if query.is_empty() {
        return None;
    }
    let mut counts = candidates(query, results);
    counts.sort_by(|a, b| {
        let exact = |word: &str| word.starts_with(query);
        b.1.cmp(&a.1)
            .then_with(|| exact(&b.0).cmp(&exact(&a.0)))
            .then_with(|| a.0.len().cmp(&b.0.len()))
            .then_with(|| a.0.cmp(&b.0))
    });
    counts.into_iter().next().map(|(word, _)| word)
}

#[cfg(test)]
mod tests {
    use super::{completion, files, in_next_file, occurrences, rows, scan, Case, Results, Row};
    use crate::Direction;

    fn corpus() -> Vec<(String, String)> {
        vec![
            (
                "a.rs".to_string(),
                "fn update(x)\nlet Update = 1".to_string(),
            ),
            ("b.rs".to_string(), "// nothing".to_string()),
        ]
    }

    #[test]
    fn a_lowercase_query_ignores_case() {
        assert_eq!(scan("update", &corpus()).hits.len(), 2);
    }

    #[test]
    fn a_capital_makes_it_exact() {
        let hits = scan("Update", &corpus()).hits;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 2);
    }

    #[test]
    fn punctuation_is_literal() {
        assert_eq!(scan("update(x)", &corpus()).hits.len(), 1);
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(scan("", &corpus()).hits.is_empty());
    }

    #[test]
    fn a_hit_names_where_in_the_line_the_match_starts() {
        let hits = scan("update", &corpus()).hits;
        assert_eq!((hits[0].line, hits[0].column), (1, 4));
        assert_eq!((hits[1].line, hits[1].column), (2, 5));
    }

    #[test]
    fn every_occurrence_in_a_line_is_a_match() {
        assert_eq!(
            occurrences("state", "state and state", Case::Smart),
            vec![1, 11]
        );
    }

    #[test]
    fn case_overrides_smart_case_either_way() {
        assert_eq!(occurrences("State", "state State", Case::Smart), vec![7]);
        assert_eq!(
            occurrences("State", "state State", Case::Ignore),
            vec![1, 7]
        );
        assert_eq!(occurrences("state", "state State", Case::Smart), vec![1, 7]);
        assert_eq!(occurrences("state", "state State", Case::Exact), vec![1]);
    }

    #[test]
    fn a_match_is_not_counted_twice_where_it_overlaps_itself() {
        assert_eq!(occurrences("aa", "aaa", Case::Smart), vec![1]);
    }

    #[test]
    fn one_line_matching_twice_is_one_hit() {
        let files = vec![("a.rs".to_string(), "state and state".to_string())];
        let hits = scan("state", &files).hits;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].column, 1);
    }

    #[test]
    fn lines_are_numbered_from_one() {
        assert_eq!(scan("fn", &corpus()).hits[0].line, 1);
    }

    #[test]
    fn the_cap_is_reported_rather_than_hidden() {
        let many = vec![("big.rs".to_string(), "x\n".repeat(super::CAP + 50))];
        let results = scan("x", &many);
        assert_eq!(results.hits.len(), super::CAP);
        assert!(results.truncated, "a silent subset is worse than a count");
    }

    fn grouped() -> Results {
        scan(
            "update",
            &[
                ("a.rs".to_string(), "update\nupdate".to_string()),
                ("b.rs".to_string(), "update".to_string()),
            ],
        )
    }

    #[test]
    fn a_hits_row_counts_the_headings_above_it() {
        assert_eq!(
            rows(&grouped()),
            vec![
                Row::File("a.rs"),
                Row::Hit(0),
                Row::Hit(1),
                Row::File("b.rs"),
                Row::Hit(2),
            ]
        );
    }

    #[test]
    fn stepping_by_file_skips_the_rest_of_a_files_hits() {
        assert_eq!(in_next_file(&grouped(), 0, Direction::Down), 2);
    }

    #[test]
    fn stepping_back_lands_on_the_top_of_the_file_it_is_in() {
        assert_eq!(in_next_file(&grouped(), 1, Direction::Up), 0);
    }

    #[test]
    fn stepping_back_from_a_files_first_hit_lands_on_the_file_above() {
        assert_eq!(in_next_file(&grouped(), 2, Direction::Up), 0);
    }

    #[test]
    fn the_step_by_file_stops_at_the_first_and_last_file() {
        assert_eq!(in_next_file(&grouped(), 2, Direction::Down), 2);
        assert_eq!(in_next_file(&grouped(), 0, Direction::Up), 0);
    }

    #[test]
    fn files_keep_the_order_they_first_appear() {
        let results = scan("fn", &corpus());
        assert_eq!(files(&results), vec!["a.rs".to_string()]);
    }

    #[test]
    fn completion_prefers_the_commonest_word() {
        let results = Results {
            hits: vec![
                super::Hit {
                    file: "a".into(),
                    line: 1,
                    column: 1,
                    text: "update update".into(),
                },
                super::Hit {
                    file: "a".into(),
                    line: 2,
                    column: 1,
                    text: "updated".into(),
                },
            ],
            truncated: false,
        };
        assert_eq!(completion("upda", &results).as_deref(), Some("update"));
    }

    #[test]
    fn a_tie_completes_to_the_case_you_typed() {
        let results = Results {
            hits: vec![
                super::Hit {
                    file: "a".into(),
                    line: 1,
                    column: 1,
                    text: "Update".into(),
                },
                super::Hit {
                    file: "a".into(),
                    line: 2,
                    column: 1,
                    text: "update".into(),
                },
            ],
            truncated: false,
        };
        assert_eq!(completion("upda", &results).as_deref(), Some("update"));
        assert_eq!(completion("Upda", &results).as_deref(), Some("Update"));
    }

    #[test]
    fn there_is_nothing_to_complete_to_when_the_query_is_already_whole() {
        let results = Results {
            hits: vec![super::Hit {
                file: "a".into(),
                line: 1,
                column: 1,
                text: "update".into(),
            }],
            truncated: false,
        };
        assert_eq!(completion("update", &results), None);
    }
}
