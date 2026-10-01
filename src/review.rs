//! F7 and F8 — what Review view lists, and what submitting sends.
//!
//! Runs no git commands and writes no files. The edge supplies git's answer;
//! submitting returns the artifact and the AI prompt as effects.

use crate::highlight::Token;
use crate::{DiffLine, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Staged,
    Untracked,
    Committed,
    Ignored,
    /// Unmerged: a merge or a rebase left Conflicts in it for somebody to
    /// resolve.
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
    /// The Story and Step this comment argues with, when it was made while
    /// walking one. Both absent for a comment made in Review view — there is
    /// no Story to name and no Step to number. One comment type with two
    /// optional fields, not two kinds of comment.
    pub story: Option<String>,
    /// The Step's 1-based position within its Story — the same numbering the
    /// spine and the walkthrough show the reviewer.
    pub step: Option<u32>,
}

/// Everything uncommitted against HEAD. Committed files are not under review;
/// ignored ones never are.
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
                            // It differs from HEAD, and resolving it is a
                            // change like any other.
                            | GitStatus::Conflicted
                    )
                })
                .map(|file| file.path.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The revision the diff on screen is measured from: `HEAD`, as the edge
/// resolved it. Every side of Review view already reads from there — `list` is
/// everything uncommitted against `HEAD`, and the edge diffs the working tree
/// against `HEAD`'s tree — so a Risk delta measured from anywhere else would
/// disagree with the diff beside it, which is worse than no figure at all. There
/// is no separate base to reuse: this *is* the base, named here so the delta and
/// the diff cannot come to be measured from two different places.
///
/// `None` in a folder that is no repository, and in one with no commit yet:
/// nothing to measure a change against, so there is no change.
pub fn base(state: &State) -> Option<&str> {
    state.head.as_deref()
}

/// A folder that is not a repository is not the same situation as a repository
/// with nothing to review, so they get different answers.
pub fn view_state(state: &State) -> &'static str {
    match &state.repo {
        None => "not-a-git-repository",
        Some(_) if list(state).is_empty() => "no-changes",
        Some(_) => "changes",
    }
}

/// ISSUE is the only blocking type; the rest are context.
pub fn verdict(comments: &[Comment]) -> &'static str {
    if comments.iter().any(|comment| comment.kind == "ISSUE") {
        "changes-requested"
    } else {
        "commented"
    }
}

/// Comments anchored to a line, so the diff can show them where they belong.
pub fn comments_at<'a>(state: &'a State, file: &str, line: u32) -> Vec<&'a Comment> {
    state
        .comments
        .iter()
        .filter(|comment| comment.file == file && comment.to_line == line)
        .collect()
}

/// Which tokens colour each row of `diff`: the ones its own side holds at its
/// own line number, or nothing where there are none to be had.
///
/// A row is looked up in the side it came from — a removed row in `old`, an
/// added or context row in `new` — because a removed line's colour is a fact
/// about the text that is going away. Taking it from whatever replaced it
/// would colour a line by code that is not the line.
///
/// Whole sides go in rather than a row's own text, and that is the point:
/// highlighting a row on its own restarts every token that spans lines, so a
/// row inside a block comment or a multi-line string reads as code. A diff is
/// two sources interleaved, and each of them is only a source when it is whole.
///
/// `None`, never a guess, in three cases. A side that could not be read — no
/// repository, no commit, a file `HEAD` does not have — arrives empty, and an
/// absence is drawn as an absence. A row the side has no line for is the same
/// answer for the same reason. The third is not an absence at all: the sides
/// are highlighted when the diff is read, so text that disagrees with the row
/// means one of the two moved underneath the other, and drawing one line's
/// tokens over another line's text puts code on screen that is in neither side.
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
            // A row's text is trimmed at its end where the side's line is not,
            // so the comparison is too.
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

/// The review inline, plus the artifact path. Inline so the prompt is
/// actionable by any AI CLI, path so the detail survives.
pub fn prompt(comments: &[Comment], artifact_path: &str) -> String {
    let mut lines = vec![format!("Review submitted ({} comments):", comments.len())];
    for comment in comments {
        // A body holds newlines now that the box it is written in is a buffer,
        // and its later lines are indented past the heading rather than left
        // flush: at column zero they are indistinguishable from `Full review:`
        // and the instruction under it, so a two-line comment reads as though
        // the reviewer wrote the prompt's own trailer.
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

/// How many of each kind a server has said about one file under review.
/// Errors and warnings only: the reviewer's question is whether the change
/// compiles, and a hint counted beside an error is a figure that reads worse
/// than the code is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub errors: usize,
    pub warnings: usize,
}

/// One row per file under review, in the order the review lists them: what its
/// server says about it, or nothing at all.
///
/// `None` is *not measured*, and it is never zero. A file in a language no
/// server serves and a file whose server has not answered yet are both files
/// nobody has said anything about — and telling a reviewer that a broken file
/// is clean is the failure this distinction exists to refuse. It stops being
/// `None` when the server answers, an answer of "nothing wrong" included: an
/// empty push is recorded as an empty set rather than forgotten, which is what
/// makes `Some(0)` reachable at all.
pub fn diagnostics(state: &State) -> Vec<(String, Option<Counts>)> {
    list(state)
        .into_iter()
        .map(|file| {
            let counted = state
                .diagnostics
                .get(&state.root.join(&file))
                .map(|by_language| {
                    // Across every server that spoke about the file, not one of
                    // them: a `.vue` file's template mistakes and its type errors
                    // come from two servers, and a reviewer counting only the first
                    // is told the change is smaller than it is.
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

/// What the change as a whole carries. Three answers rather than a figure and
/// a flag beside it, because a total of zero means something different in each
/// of them and two fields could disagree about which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Total {
    /// Nothing under review was measured, so there is no figure — not a zero.
    Unmeasured,
    /// Every file under review was measured, so the figure is the change.
    Whole(Counts),
    /// Some were and some were not: the figure is a floor, never the whole
    /// change. A sum over the measured files alone reads as a statement about
    /// all of them, and `0 errors` over a change with an unmeasured file in it
    /// is the reviewer being told a broken file is clean — the one thing this
    /// figure exists to refuse.
    Partial(Counts),
}

/// The change as a whole, summed over the files something is known about.
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

    /// The figure answers "does this compile", so the two that say it does not
    /// are counted and the two that are advice are not — counted in, a file
    /// full of hints reads exactly like a file that will not build.
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

    /// The distinction the whole figure turns on, in the one place it is
    /// easiest to lose: a sum over nothing is zero, and zero errors is a claim
    /// that the change is clean. A change with one file nobody measured is that
    /// same claim, quieter — so the figure says it is a floor rather than the
    /// change.
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

    /// The whole of what a diff row's colour turns on: which side it is read
    /// from. Both sides say `total` at line 1 and mean different things, so a
    /// removed row taking the new side's tokens is a line coloured by the code
    /// that replaced it — indistinguishable from correct until the two sides
    /// disagree, which is every diff worth reading.
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

    /// An absence is drawn as an absence. A folder that is no repository, a
    /// repository with no commit and a file `HEAD` never had all arrive as a
    /// side with nothing in it, and every one of them must leave the row at the
    /// flat colour it already has rather than borrowing the other side's.
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

    /// The sides are read and highlighted when the diff is read, so a file the
    /// AI rewrote in between leaves the two disagreeing. Colouring the row
    /// anyway draws the side's text, not the row's — code on screen that is in
    /// neither version of the file.
    #[test]
    fn a_side_that_disagrees_with_the_row_colours_nothing() {
        let new = crate::highlight::highlight("a.ts", "const total = 42;");
        assert_eq!(
            diff_tokens(&[row(Some(1), None, "const total = 7;")], &new, &[]),
            vec![None]
        );
    }

    /// A comment body can hold newlines now that it is written in a buffer, and
    /// the prompt is lines of text an AI reads. Flush left, a body's second line
    /// is indistinguishable from the prompt's own trailer — which is how a
    /// reviewer's paragraph becomes an instruction nobody wrote.
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
        // Nothing a body holds may reach column zero, where the prompt's own
        // lines live.
        for line in prompt.lines().skip(1) {
            assert!(
                line.starts_with(' ') || line == "Please address the ISSUEs.",
                "{line:?} reads as a line of the prompt rather than of a comment"
            );
        }
    }
}
