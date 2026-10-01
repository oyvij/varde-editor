use crate::{lsp, Effect, Place, State};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Done(String),
    Missing,
    Failed(String),
}

pub fn run(state: &State) -> Vec<Effect> {
    let Some(path) = state.current_buffer.clone() else {
        return Vec::new();
    };
    let Some(buffer) = state.buffers.get(&path) else {
        return Vec::new();
    };
    let language = language(state, &path);
    let Some(formatter) = state.formatters.get(&language) else {
        return vec![Effect::notify_about(
            "no-formatter-configured",
            format!("[formatter.{language}] in .varde/config.toml"),
        )];
    };
    let mut names = lsp::facts(state);
    names.insert(
        "file".to_string(),
        Some(path.to_string_lossy().into_owned()),
    );
    vec![Effect::RunFormatter {
        language,
        command: formatter.command.clone(),
        args: formatter
            .args
            .iter()
            .filter_map(|arg| lsp::filled(arg, &names))
            .collect(),
        path: path.clone(),
        revision: buffer.revision(),
        text: buffer.shown().to_string(),
    }]
}

fn language(state: &State, path: &Path) -> String {
    let named = path
        .extension()
        .or_else(|| path.file_name())
        .and_then(|named| named.to_str())
        .unwrap_or_default();
    state
        .formatters
        .iter()
        .find(|(_, formatter)| formatter.extensions.iter().any(|claim| claim == named))
        .map(|(language, _)| language.clone())
        .unwrap_or_else(|| named.to_string())
}

pub fn answered(
    state: &mut State,
    language: &str,
    path: &Path,
    revision: u64,
    answer: Answer,
) -> Vec<Effect> {
    match answer {
        Answer::Missing => {
            let formatter = state.formatters.get(language);
            let install = formatter.and_then(|formatter| formatter.install.get(&state.os));
            let mut effects: Vec<Effect> = install
                .cloned()
                .into_iter()
                .map(Effect::SetTerminalInput)
                .collect();
            effects.push(Effect::notify_about(
                "formatter-missing",
                formatter
                    .map(|formatter| formatter.command.clone())
                    .unwrap_or_default(),
            ));
            effects
        }
        Answer::Failed(why) => vec![Effect::notify_about(
            "formatter-failed",
            why.lines().next().unwrap_or_default().trim().to_string(),
        )],
        Answer::Done(text) => {
            let named = state
                .formatters
                .get(language)
                .map(|formatter| formatter.command.clone())
                .unwrap_or_default();
            let Some(buffer) = state.buffers.get_mut(path) else {
                return Vec::new();
            };
            if buffer.revision() != revision {
                return Vec::new();
            }
            // empty stdout means the formatter rewrote in place; taking it as the answer deletes the file
            if text.is_empty() && !buffer.shown().trim().is_empty() {
                return vec![Effect::notify_about("formatter-failed", named)];
            }
            let spans = spans(buffer.shown(), &text);
            if spans.is_empty() {
                return vec![Effect::Notify("nothing-to-format")];
            }
            buffer.reformat(&spans);
            Vec::new()
        }
    }
}

fn spans(old: &str, new: &str) -> Vec<(Place, Place, String)> {
    let lines: Vec<&str> = new.split('\n').collect();
    let mut options = git2::DiffOptions::new();
    options.context_lines(0);
    let Ok(patch) = git2::Patch::from_buffers(
        old.as_bytes(),
        None,
        new.as_bytes(),
        None,
        Some(&mut options),
    ) else {
        return Vec::new();
    };
    (0..patch.num_hunks())
        .filter_map(|index| patch.hunk(index).ok())
        .map(|(hunk, _)| {
            let at = match hunk.old_lines() {
                0 => hunk.old_start() as usize + 1,
                _ => hunk.old_start() as usize,
            };
            let from = hunk.new_start() as usize - usize::from(hunk.new_lines() > 0);
            let text = lines
                .iter()
                .enumerate()
                .skip(from)
                .take(hunk.new_lines() as usize)
                .map(|(index, line)| match index + 1 == lines.len() {
                    true => (*line).to_string(),
                    false => format!("{line}\n"),
                })
                .collect();
            (
                Place {
                    line: at,
                    column: 1,
                },
                Place {
                    line: at + hunk.old_lines() as usize,
                    column: 1,
                },
                text,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::spans;
    use crate::Place;

    #[test]
    fn one_changed_line_is_one_span_over_that_line() {
        assert_eq!(
            spans("a\nb\nc\n", "a\nB\nc\n"),
            vec![(
                Place { line: 2, column: 1 },
                Place { line: 3, column: 1 },
                "B\n".to_string()
            )]
        );
    }

    #[test]
    fn an_inserted_line_lands_after_the_line_the_hunk_names() {
        assert_eq!(
            spans("a\nc\n", "a\nb\nc\n"),
            vec![(
                Place { line: 2, column: 1 },
                Place { line: 2, column: 1 },
                "b\n".to_string()
            )]
        );
    }

    #[test]
    fn a_deleted_line_is_a_span_with_nothing_in_it() {
        assert_eq!(
            spans("a\nb\nc\n", "a\nc\n"),
            vec![(
                Place { line: 2, column: 1 },
                Place { line: 3, column: 1 },
                String::new()
            )]
        );
    }

    #[test]
    fn nothing_answered_over_a_buffer_of_whitespace_is_applied() {
        let path = std::path::PathBuf::from("/w/thing.json");
        let mut state = crate::State::default();
        state
            .buffers
            .insert(path.clone(), crate::editor::Buffer::open("   \n", false, 4));
        let revision = state.buffers[&path].revision();
        let effects = super::answered(
            &mut state,
            "json",
            &path,
            revision,
            super::Answer::Done(String::new()),
        );
        assert!(effects.is_empty(), "{effects:?}");
        assert_eq!(state.buffers[&path].shown(), "");
    }

    #[test]
    fn a_failure_says_its_first_line_without_the_control_characters_in_it() {
        assert_eq!(
            super::answered(
                &mut crate::State::default(),
                "json",
                std::path::Path::new("/w/thing.json"),
                0,
                super::Answer::Failed("\u{1b}[2Jboom: line 1\nstack".to_string()),
            ),
            vec![crate::Effect::NotifyAbout {
                slug: "formatter-failed",
                about: "[2Jboom: line 1".to_string(),
            }]
        );
    }

    #[test]
    fn nothing_changed_is_no_span() {
        assert!(spans("a\nb\n", "a\nb\n").is_empty());
    }

    #[test]
    fn an_unterminated_last_line_stays_unterminated() {
        assert_eq!(
            spans("{\"a\":1}", "{\n  \"a\": 1\n}"),
            vec![(
                Place { line: 1, column: 1 },
                Place { line: 2, column: 1 },
                "{\n  \"a\": 1\n}".to_string()
            )]
        );
    }
}
