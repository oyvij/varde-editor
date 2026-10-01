use crate::highlight::Token;
use crate::{DiffLine, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Staged,
    Untracked,
    Committed,
    Ignored,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFile {
    pub path: String,
    pub status: GitStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub file: String,
    pub from_line: u32,
    pub to_line: u32,
    pub kind: String,
    pub body: String,
    pub revision: String,
    pub story: Option<String>,
    pub step: Option<u32>,
}

pub fn list(state: &State) -> Vec<String> {
    state
        .repo
        .as_ref()
        .map(|files| {
            files
                .iter()
                .filter(|file| {
                    matches!(
                        file.status,
                        GitStatus::Modified
                            | GitStatus::Staged
                            | GitStatus::Untracked
                            | GitStatus::Conflicted
                    )
                })
                .map(|file| file.path.clone())
                .collect()
        })
        .unwrap_or_default()
}

pub fn base(state: &State) -> Option<&str> {
    state.head.as_deref()
}

pub fn view_state(state: &State) -> &'static str {
    match &state.repo {
        None => "not-a-git-repository",
        Some(_) if list(state).is_empty() => "no-changes",
        Some(_) => "changes",
    }
}

pub fn verdict(comments: &[Comment]) -> &'static str {
    if comments.iter().any(|comment| comment.kind == "ISSUE") {
        "changes-requested"
    } else {
        "commented"
    }
}

pub fn comments_at<'a>(state: &'a State, file: &str, line: u32) -> Vec<&'a Comment> {
    state
        .comments
        .iter()
        .filter(|comment| comment.file == file && comment.to_line == line)
        .collect()
}

pub fn diff_tokens<'a>(
    diff: &[DiffLine],
    new: &'a [Vec<Token>],
    old: &'a [Vec<Token>],
) -> Vec<Option<&'a [Token]>> {
    diff.iter()
        .map(|line| {
            let (side, number) = match line.removed {
                true => (old, line.old_line),
                false => (new, line.new_line),
            };
            let tokens = side.get(number?.checked_sub(1)?)?;
            let text: String = tokens.iter().map(|token| token.text.as_str()).collect();
            (text.trim_end() == line.text).then_some(tokens.as_slice())
        })
        .collect()
}

pub fn artifact(comments: &[Comment], verdict: &str) -> String {
    let entries: Vec<serde_json::Value> = comments
        .iter()
        .map(|comment| {
            serde_json::json!({
                "file": comment.file,
                "from_line": comment.from_line,
                "to_line": comment.to_line,
                "type": comment.kind,
                "body": comment.body,
                "revision": comment.revision,
                "story": comment.story,
                "step": comment.step,
            })
        })
        .collect();
    serde_json::json!({ "verdict": verdict, "comments": entries }).to_string()
}

pub fn prompt(comments: &[Comment], artifact_path: &str) -> String {
    let mut lines = vec![format!("Review submitted ({} comments):", comments.len())];
    for comment in comments {
        let body = comment.body.replace('\n', "\n      ");
        lines.push(format!(
            "  {}:{}-{} {} — {}",
            comment.file, comment.from_line, comment.to_line, comment.kind, body
        ));
    }
    lines.push(format!("  Full review: {artifact_path}"));
    lines.push("Please address the ISSUEs.".to_string());
    lines.join("\n")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub errors: usize,
    pub warnings: usize,
}

pub fn diagnostics(state: &State) -> Vec<(String, Option<Counts>)> {
    list(state)
        .into_iter()
        .map(|file| {
            let counted = state
                .diagnostics
                .get(&state.root.join(&file))
                .map(|by_language| {
                    by_language
                        .values()
                        .flatten()
                        .fold(Counts::default(), |mut counts, held| {
                            match held.severity {
                                crate::lsp::Severity::Error => counts.errors += 1,
                                crate::lsp::Severity::Warning => counts.warnings += 1,
                                crate::lsp::Severity::Information | crate::lsp::Severity::Hint => {}
                            }
                            counts
                        })
                });
            (file, counted)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Total {
    Unmeasured,
    Whole(Counts),
    Partial(Counts),
}

pub fn diagnostic_total(state: &State) -> Total {
    let rows = diagnostics(state);
    let measured: Vec<Counts> = rows.iter().filter_map(|(_, counts)| *counts).collect();
    let Some(total) = measured.iter().copied().reduce(|mut total, counts| {
        total.errors += counts.errors;
        total.warnings += counts.warnings;
        total
    }) else {
        return Total::Unmeasured;
    };
    match measured.len() == rows.len() {
        true => Total::Whole(total),
        false => Total::Partial(total),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::{Diagnostic, Severity};
    use std::path::PathBuf;

    fn reviewing(files: &[&str]) -> State {
        State {
            root: PathBuf::from("/home/me/project"),
            repo: Some(
                files
                    .iter()
                    .map(|path| GitFile {
                        path: path.to_string(),
                        status: GitStatus::Modified,
                    })
                    .collect(),
            ),
            ..State::default()
        }
    }

    fn said(state: &mut State, file: &str, severities: &[Severity]) {
        state
            .diagnostics
            .entry(state.root.join(file))
            .or_default()
            .insert(
                "rust".to_string(),
                severities
                    .iter()
                    .map(|severity| Diagnostic {
                        line: 1,
                        column: 1,
                        end_column: Some(1),
                        severity: *severity,
                        message: "said".to_string(),
                    })
                    .collect(),
            );
    }

    #[test]
    fn only_errors_and_warnings_are_counted() {
        let mut state = reviewing(&["src/lib.rs"]);
        said(
            &mut state,
            "src/lib.rs",
            &[
                Severity::Error,
                Severity::Warning,
                Severity::Warning,
                Severity::Information,
                Severity::Hint,
            ],
        );
        assert_eq!(
            diagnostics(&state),
            vec![(
                "src/lib.rs".to_string(),
                Some(Counts {
                    errors: 1,
                    warnings: 2
                })
            )]
        );
    }

    #[test]
    fn a_total_is_a_floor_until_every_file_under_review_is_measured() {
        let mut state = reviewing(&["src/lib.rs", "web/app.ts"]);
        assert_eq!(diagnostic_total(&state), Total::Unmeasured);
        said(&mut state, "src/lib.rs", &[]);
        assert_eq!(diagnostic_total(&state), Total::Partial(Counts::default()));
        said(&mut state, "web/app.ts", &[Severity::Error]);
        assert_eq!(
            diagnostic_total(&state),
            Total::Whole(Counts {
                errors: 1,
                warnings: 0
            })
        );
    }

    fn row(new_line: Option<usize>, old_line: Option<usize>, text: &str) -> DiffLine {
        DiffLine {
            new_line,
            old_line,
            removed: new_line.is_none(),
            text: text.to_string(),
        }
    }

    fn text_of(tokens: Option<&[crate::highlight::Token]>) -> String {
        tokens
            .unwrap_or_default()
            .iter()
            .map(|token| token.text.as_str())
            .collect()
    }

    #[test]
    fn each_row_is_coloured_from_its_own_side() {
        let new = crate::highlight::highlight("a.ts", "const total = 42;");
        let old = crate::highlight::highlight("a.ts", "const total = \"gone\";");
        let diff = [
            row(None, Some(1), "const total = \"gone\";"),
            row(Some(1), None, "const total = 42;"),
        ];
        let tokens = diff_tokens(&diff, &new, &old);
        assert_eq!(text_of(tokens[0]), "const total = \"gone\";");
        assert_eq!(text_of(tokens[1]), "const total = 42;");
    }

    #[test]
    fn a_side_that_could_not_be_read_colours_nothing() {
        let new = crate::highlight::highlight("a.ts", "const total = 42;");
        let diff = [
            row(None, Some(1), "const gone = 1;"),
            row(Some(1), None, "const total = 42;"),
        ];
        assert_eq!(diff_tokens(&diff, &new, &[])[0], None);
        assert_eq!(diff_tokens(&diff, &[], &[]), vec![None, None]);
    }

    #[test]
    fn a_side_that_disagrees_with_the_row_colours_nothing() {
        let new = crate::highlight::highlight("a.ts", "const total = 42;");
        assert_eq!(
            diff_tokens(&[row(Some(1), None, "const total = 7;")], &new, &[]),
            vec![None]
        );
    }

    #[test]
    fn a_multi_line_body_stays_inside_its_own_entry_in_the_prompt() {
        let comment = Comment {
            file: "src/tree.js".to_string(),
            from_line: 14,
            to_line: 18,
            kind: "ISSUE".to_string(),
            body: "the path is quoted now\nbut run() still takes it raw".to_string(),
            revision: String::new(),
            story: None,
            step: None,
        };
        let prompt = prompt(std::slice::from_ref(&comment), "/x/review.json");
        assert!(
            prompt.contains("  src/tree.js:14-18 ISSUE — the path is quoted now\n      but run()"),
            "the second line is indented under its own entry: {prompt}"
        );
        for line in prompt.lines().skip(1) {
            assert!(
                line.starts_with(' ') || line == "Please address the ISSUEs.",
                "{line:?} reads as a line of the prompt rather than of a comment"
            );
        }
    }
}
