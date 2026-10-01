use crate::{Direction, Effect, State};
use std::path::PathBuf;

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
    pub run: Run,
    pub generation: u64,
    pub query: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Run {
    Searching,
    #[default]
    Finished,
    CutShort,
}

pub const CAP: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub generation: u64,
    pub query: String,
    pub under: Option<PathBuf>,
    pub only: Option<Vec<String>>,
    pub buffers: Vec<(String, String)>,
}

impl Request {
    pub fn covers(&self, file: &str) -> bool {
        !self.buffers.iter().any(|(open, _)| open == file)
            && self
                .under
                .as_ref()
                .is_none_or(|under| std::path::Path::new(file).starts_with(under))
            && self
                .only
                .as_ref()
                .is_none_or(|only| only.iter().any(|hit| hit == file))
    }
}

pub fn ask(next: &mut State) -> Vec<Effect> {
    next.searches_asked += 1;
    let generation = next.searches_asked;
    let buffers = buffers(next);
    let Some(search) = next.search.as_mut() else {
        return Vec::new();
    };
    search.asked = generation;
    let query = search.query.shown().to_string();
    if query.is_empty() {
        search.results = Results {
            generation,
            ..Results::default()
        };
        return Vec::new();
    }
    let previous = &search.results;
    let narrows = previous.run == Run::Finished && extends(&query, &previous.query);
    vec![Effect::RunSearch(Request {
        generation,
        only: narrows.then(|| files(previous)),
        query,
        under: search.scope.clone(),
        buffers,
    })]
}

fn extends(query: &str, previous: &str) -> bool {
    if previous.is_empty() {
        return false;
    }
    match Case::Smart.exact(previous) {
        true => query.contains(previous),
        false => query.to_lowercase().contains(previous),
    }
}

fn buffers(state: &State) -> Vec<(String, String)> {
    let scope = state
        .search
        .as_ref()
        .and_then(|search| search.scope.as_ref());
    state
        .buffers
        .iter()
        .filter_map(|(path, buffer)| {
            let relative = path.strip_prefix(&state.root).ok()?;
            scope
                .is_none_or(|scope| relative.starts_with(scope))
                .then(|| {
                    (
                        relative.to_string_lossy().into_owned(),
                        buffer.shown().to_string(),
                    )
                })
        })
        .collect()
}

pub fn arrived(next: &mut State, generation: u64, hits: Vec<Hit>, done: bool) {
    let Some(search) = next.search.as_mut() else {
        return;
    };
    if generation != search.asked {
        return;
    }
    if search.results.generation != generation {
        search.results = Results {
            generation,
            query: search.query.shown().to_string(),
            run: Run::Searching,
            ..Results::default()
        };
        search.selected = 0;
    }
    let results = &mut search.results;
    if results.run != Run::Searching {
        return;
    }
    let held = results
        .hits
        .get(search.selected)
        .map(|hit| (hit.file.clone(), hit.line));
    results.hits.extend(hits);
    results
        .hits
        .sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    results
        .hits
        .dedup_by(|a, b| (&a.file, a.line) == (&b.file, b.line));
    if results.hits.len() > CAP {
        results.hits.truncate(CAP);
        results.run = Run::CutShort;
    } else if done {
        results.run = Run::Finished;
    }
    search.selected = held
        .and_then(|(file, line)| {
            results
                .hits
                .iter()
                .position(|hit| hit.file == file && hit.line == line)
        })
        .unwrap_or(search.selected.min(results.hits.len().saturating_sub(1)));
}

pub fn running(state: &State) -> Option<u64> {
    let search = state.search.as_ref()?;
    (search.asked != search.results.generation || search.results.run == Run::Searching)
        .then_some(search.asked)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extent {
    Anywhere,
    Word,
}

pub fn occurrences(query: &str, line: &str, case: Case, extent: Extent) -> Vec<u32> {
    if query.is_empty() {
        return Vec::new();
    }
    let (needle, haystack) = if case.exact(query) {
        (query.to_string(), line.to_string())
    } else {
        (query.to_lowercase(), line.to_lowercase())
    };
    let wordy = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut columns = Vec::new();
    let mut from = 0;
    while let Some(at) = haystack[from..].find(&needle) {
        let start = from + at;
        let end = start + needle.len();
        let bounded =
            !wordy(haystack[..start].chars().next_back()) && !wordy(haystack[end..].chars().next());
        if extent == Extent::Anywhere || bounded {
            columns.push(haystack[..start].chars().count() as u32 + 1);
            from = end;
        } else {
            from = start + haystack[start..].chars().next().map_or(1, char::len_utf8);
        }
    }
    columns
}

pub fn hit(query: &str, file: &str, line: u64, text: &str) -> Option<Hit> {
    let &column = occurrences(query, text, Case::Smart, Extent::Anywhere).first()?;
    Some(Hit {
        file: file.to_string(),
        line: line as u32,
        column,
        text: text.trim_end().to_string(),
    })
}

pub fn scan(query: &str, files: &[(String, String)]) -> Vec<Hit> {
    files
        .iter()
        .flat_map(|(path, contents)| {
            contents
                .split('\n')
                .enumerate()
                .filter_map(move |(index, line)| hit(query, path, index as u64 + 1, line))
        })
        .take(CAP + 1)
        .collect()
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
    use super::{
        completion, files, in_next_file, occurrences, rows, running, scan, Case, Extent, Hit,
        Request, Results, Row,
    };
    use crate::{update, Direction, Effect, Event, Search, State};
    use std::path::PathBuf;

    fn at(file: &str, line: u32) -> Hit {
        Hit {
            file: file.to_string(),
            line,
            column: 1,
            text: String::new(),
        }
    }

    fn requested(effects: Vec<Effect>) -> Request {
        match effects.as_slice() {
            [Effect::RunSearch(request)] => request.clone(),
            other => panic!("no search was asked for: {other:?}"),
        }
    }

    fn typed(state: &State, query: &str) -> (State, Request) {
        let (state, effects) = update(state, Event::SearchQuery(query.to_string()));
        (state, requested(effects))
    }

    fn asked(query: &str) -> (State, u64) {
        let (state, request) = typed(&State::default(), query);
        (state, request.generation)
    }

    fn arrive(state: &State, generation: u64, hits: Vec<Hit>, done: bool) -> State {
        update(
            state,
            Event::Searched {
                generation,
                hits,
                done,
            },
        )
        .0
    }

    fn shown(state: &State) -> Vec<(&str, u32)> {
        state
            .search
            .as_ref()
            .unwrap()
            .results
            .hits
            .iter()
            .map(|hit| (hit.file.as_str(), hit.line))
            .collect()
    }

    fn selected(state: &State) -> (&str, u32) {
        let search = state.search.as_ref().unwrap();
        let hit = &search.results.hits[search.selected];
        (hit.file.as_str(), hit.line)
    }

    #[test]
    fn hits_arriving_out_of_order_are_shown_by_path_then_line() {
        let (state, generation) = asked("x");
        let state = arrive(
            &state,
            generation,
            vec![at("b.rs", 3), at("a.rs", 9)],
            false,
        );
        let state = arrive(
            &state,
            generation,
            vec![at("b.rs", 1), at("a.rs", 2)],
            false,
        );
        assert_eq!(
            shown(&state),
            vec![("a.rs", 2), ("a.rs", 9), ("b.rs", 1), ("b.rs", 3)]
        );
    }

    #[test]
    fn the_selection_stays_on_its_hit_as_hits_arrive_above_it() {
        let (state, generation) = asked("x");
        let state = arrive(
            &state,
            generation,
            vec![at("m.rs", 1), at("m.rs", 5)],
            false,
        );
        let state = update(&state, Event::MoveHit(Direction::Down)).0;
        assert_eq!(selected(&state), ("m.rs", 5));
        let state = arrive(
            &state,
            generation,
            vec![at("a.rs", 1), at("b.rs", 1)],
            false,
        );
        assert_eq!(
            selected(&state),
            ("m.rs", 5),
            "the selection slid off its hit"
        );
        assert_eq!(state.search.as_ref().unwrap().selected, 3);
    }

    #[test]
    fn the_same_hit_arriving_twice_is_shown_once() {
        let (state, generation) = asked("x");
        let state = arrive(&state, generation, vec![at("a.rs", 1)], false);
        let state = arrive(&state, generation, vec![at("a.rs", 1), at("a.rs", 1)], true);
        assert_eq!(shown(&state), vec![("a.rs", 1)]);
    }

    #[test]
    fn hits_from_an_older_search_are_dropped() {
        let (first, old) = asked("x");
        let (second, request) = typed(&first, "y");
        assert!(request.generation > old);
        let late = arrive(&second, old, vec![at("a.rs", 1)], true);
        assert!(shown(&late).is_empty(), "the older search's hit was shown");
        assert_eq!(running(&late), Some(request.generation));
    }

    #[test]
    fn a_closed_and_reopened_box_does_not_reuse_a_generation() {
        let (first, old) = asked("x");
        let closed = update(&first, Event::CloseSearch).0;
        let reopened = update(&closed, Event::OpenSearch).0;
        let (_, request) = typed(&reopened, "x");
        assert_ne!(request.generation, old);
    }

    #[test]
    fn the_spinner_turns_until_the_search_is_done() {
        let (state, generation) = asked("x");
        assert_eq!(
            running(&state),
            Some(generation),
            "waiting for the first hit"
        );
        let state = arrive(&state, generation, vec![at("a.rs", 1)], false);
        assert_eq!(running(&state), Some(generation), "hits still coming");
        let state = arrive(&state, generation, vec![], true);
        assert_eq!(running(&state), None);
    }

    #[test]
    fn an_open_buffer_is_searched_in_place_of_its_file() {
        let root = PathBuf::from("/w");
        let mut buffers = std::collections::BTreeMap::new();
        buffers.insert(
            root.join("a.rs"),
            crate::editor::Buffer::text_box("let x = 1"),
        );
        let state = State {
            root,
            buffers,
            search: Some(Search::default()),
            ..State::default()
        };
        let (_, request) = typed(&state, "x");
        assert_eq!(
            request.buffers,
            vec![("a.rs".to_string(), "let x = 1".to_string())]
        );
        assert!(!request.covers("a.rs"), "the file on disk was searched too");
        assert!(request.covers("b.rs"));
    }

    #[test]
    fn a_request_covers_only_its_folder_and_the_files_it_narrowed_to() {
        let request = Request {
            generation: 1,
            query: "x".to_string(),
            under: Some(PathBuf::from("src")),
            only: Some(vec!["src/a.rs".to_string(), "docs/a.md".to_string()]),
            buffers: Vec::new(),
        };
        assert!(request.covers("src/a.rs"));
        assert!(!request.covers("src/b.rs"), "a file the last query missed");
        assert!(!request.covers("docs/a.md"), "a file outside the folder");
    }

    #[test]
    fn an_extended_query_searches_only_the_files_it_hit() {
        let (state, generation) = asked("upd");
        let state = arrive(&state, generation, vec![at("a.rs", 1), at("c.rs", 4)], true);
        let (_, request) = typed(&state, "update");
        assert_eq!(
            request.only,
            Some(vec!["a.rs".to_string(), "c.rs".to_string()])
        );
    }

    #[test]
    fn a_cut_short_search_is_not_narrowed() {
        let (state, generation) = asked("x");
        let many = (1..=super::CAP as u32 + 1)
            .map(|line| at("a.rs", line))
            .collect();
        let state = arrive(&state, generation, many, false);
        assert_eq!(typed(&state, "xy").1.only, None);
    }

    #[test]
    fn an_unfinished_search_is_not_narrowed() {
        let (state, generation) = asked("x");
        let state = arrive(&state, generation, vec![at("a.rs", 1)], false);
        assert_eq!(typed(&state, "xy").1.only, None);
    }

    #[test]
    fn a_query_that_is_not_an_extension_searches_everything() {
        let (state, generation) = asked("update");
        let state = arrive(&state, generation, vec![at("a.rs", 1)], true);
        assert_eq!(typed(&state, "upd").1.only, None, "a shorter query");
        assert!(
            typed(&state, "Update").1.only.is_some(),
            "every Update is an update"
        );
        let (state, generation) = asked("Upd");
        let state = arrive(&state, generation, vec![at("a.rs", 1)], true);
        assert_eq!(
            typed(&state, "update").1.only,
            None,
            "a capital narrowed nothing case-insensitive"
        );
    }

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
        assert_eq!(scan("update", &corpus()).len(), 2);
    }

    #[test]
    fn a_capital_makes_it_exact() {
        let hits = scan("Update", &corpus());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 2);
    }

    #[test]
    fn punctuation_is_literal() {
        assert_eq!(scan("update(x)", &corpus()).len(), 1);
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(scan("", &corpus()).is_empty());
    }

    #[test]
    fn a_hit_names_where_in_the_line_the_match_starts() {
        let hits = scan("update", &corpus());
        assert_eq!((hits[0].line, hits[0].column), (1, 4));
        assert_eq!((hits[1].line, hits[1].column), (2, 5));
    }

    #[test]
    fn every_occurrence_in_a_line_is_a_match() {
        assert_eq!(
            occurrences("state", "state and state", Case::Smart, Extent::Anywhere),
            vec![1, 11]
        );
    }

    #[test]
    fn case_overrides_smart_case_either_way() {
        assert_eq!(
            occurrences("State", "state State", Case::Smart, Extent::Anywhere),
            vec![7]
        );
        assert_eq!(
            occurrences("State", "state State", Case::Ignore, Extent::Anywhere),
            vec![1, 7]
        );
        assert_eq!(
            occurrences("state", "state State", Case::Smart, Extent::Anywhere),
            vec![1, 7]
        );
        assert_eq!(
            occurrences("state", "state State", Case::Exact, Extent::Anywhere),
            vec![1]
        );
    }

    #[test]
    fn a_match_is_not_counted_twice_where_it_overlaps_itself() {
        assert_eq!(
            occurrences("aa", "aaa", Case::Smart, Extent::Anywhere),
            vec![1]
        );
    }

    #[test]
    fn a_whole_word_is_bounded_by_anything_but_a_word_character() {
        let word = |query, line| occurrences(query, line, Case::Smart, Extent::Word);
        assert_eq!(word("state", "state state.x (state)"), vec![1, 7, 16]);
        assert_eq!(
            word("state", "states restate state_x _state"),
            Vec::<u32>::new()
        );
        assert_eq!(word("state", "østate stateø"), Vec::<u32>::new());
        assert_eq!(word("state", "ø state"), vec![3]);
    }

    #[test]
    fn a_rejected_whole_word_candidate_resumes_one_character_on() {
        assert_eq!(
            occurrences("aa", "aaa aa", Case::Smart, Extent::Word),
            vec![5]
        );
        assert_eq!(
            occurrences("a a", "ba a a", Case::Smart, Extent::Word),
            vec![4]
        );
    }

    #[test]
    fn one_line_matching_twice_is_one_hit() {
        let files = vec![("a.rs".to_string(), "state and state".to_string())];
        let hits = scan("state", &files);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].column, 1);
    }

    #[test]
    fn lines_are_numbered_from_one() {
        assert_eq!(scan("fn", &corpus())[0].line, 1);
    }

    #[test]
    fn the_cap_is_reported_rather_than_hidden() {
        let many = vec![("big.rs".to_string(), "x\n".repeat(super::CAP + 50))];
        let (state, generation) = asked("x");
        let state = arrive(&state, generation, scan("x", &many), false);
        let results = &state.search.as_ref().unwrap().results;
        assert_eq!(results.hits.len(), super::CAP);
        assert_eq!(
            results.run,
            super::Run::CutShort,
            "a silent subset is worse than a count"
        );
        assert_eq!(running(&state), None, "the search went on past the cap");
    }

    fn grouped() -> Results {
        Results {
            hits: scan(
                "update",
                &[
                    ("a.rs".to_string(), "update\nupdate".to_string()),
                    ("b.rs".to_string(), "update".to_string()),
                ],
            ),
            ..Results::default()
        }
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
        let results = Results {
            hits: scan("fn", &corpus()),
            ..Results::default()
        };
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
        };
        assert_eq!(completion("update", &results), None);
    }
}
