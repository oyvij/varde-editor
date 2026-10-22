use crate::{story, Event, State};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wait {
    pub since_ms: Option<u64>,
    pub activity_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Authoring,
}

fn of(state: &State) -> Option<(Kind, &Wait)> {
    match &state.story_set {
        story::Set::Authoring { wait, .. } => Some((Kind::Authoring, wait)),
        _ => None,
    }
}

pub fn counts(state: &State, path: &Path) -> bool {
    let Some((kind, _)) = of(state) else {
        return false;
    };
    let varde = crate::varde_dir(&state.root, state.sidecar.as_deref());
    if kind == Kind::Authoring && path.starts_with(varde.join("stories")) {
        return true;
    }
    path.starts_with(&state.root)
        && !path.starts_with(state.root.join(".git"))
        && !path.starts_with(&varde)
        && !path
            .ancestors()
            .any(|folder| state.ignored.contains(folder))
}

pub fn hunk_activity(before: &Arc<[story::FileHunks]>, state: &State) -> Vec<Event> {
    let after = &state.file_hunks;
    if Arc::ptr_eq(before, after) {
        return Vec::new();
    }
    after
        .iter()
        .filter(|file| !before.contains(file))
        .chain(
            before
                .iter()
                .filter(|file| after.iter().all(|kept| kept.file != file.file)),
        )
        .map(|file| Event::FileActivity(state.repo_root().join(&file.file)))
        .collect()
}

pub fn of_mut(state: &mut State) -> Option<&mut Wait> {
    match &mut state.story_set {
        story::Set::Authoring { wait, .. } => Some(wait),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub kind: Kind,
    pub elapsed_ms: u64,
    pub activity: Activity,
}

pub fn progress(state: &State) -> Option<Progress> {
    let (kind, wait) = of(state)?;
    let elapsed_ms = wait
        .since_ms
        .map_or(0, |since| state.clock_ms.saturating_sub(since));
    let activity = match wait.activity_ms {
        None => Activity::NoneYet,
        Some(at) => Activity::Ago(state.clock_ms.saturating_sub(at)),
    };
    Some(Progress {
        kind,
        elapsed_ms,
        activity,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    NoneYet,
    Ago(u64),
    Unknown,
}

pub fn line(progress: Progress) -> String {
    let (word, usually) = match progress.kind {
        Kind::Authoring => ("authoring", "usually 5–11 min, no timeout"),
    };
    let activity = match progress.activity {
        Activity::NoneYet => "no file activity yet".to_string(),
        Activity::Ago(ms) => format!("last file activity {} ago", clock(ms, 1)),
        Activity::Unknown => "file activity unknown".to_string(),
    };
    format!(
        "{word} · {} · {activity} · {usually}",
        clock(progress.elapsed_ms, 2)
    )
}

fn clock(ms: u64, minute_digits: usize) -> String {
    let seconds = ms / 1_000;
    let (hours, minutes, seconds) = (seconds / 3_600, seconds / 60 % 60, seconds % 60);
    match hours {
        0 => format!("{minutes:0minute_digits$}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn authoring(elapsed_ms: u64, activity: Activity) -> Progress {
        Progress {
            kind: Kind::Authoring,
            elapsed_ms,
            activity,
        }
    }

    #[test]
    fn authoring_says_its_elapsed_time_its_last_file_activity_and_its_usual_range() {
        assert_eq!(
            line(authoring(252_000, Activity::Ago(40_000))),
            "authoring · 04:12 · last file activity 0:40 ago · usually 5–11 min, no timeout"
        );
    }

    #[test]
    fn a_wait_that_has_seen_no_change_says_so_rather_than_showing_a_figure() {
        assert_eq!(
            line(authoring(5_000, Activity::NoneYet)),
            "authoring · 00:05 · no file activity yet · usually 5–11 min, no timeout"
        );
    }

    #[test]
    fn a_blind_watcher_shows_no_activity_figure() {
        assert_eq!(
            line(authoring(5_000, Activity::Unknown)),
            "authoring · 00:05 · file activity unknown · usually 5–11 min, no timeout"
        );
    }

    #[test]
    fn past_an_hour_the_figures_count_hours() {
        assert_eq!(
            line(authoring(3_723_000, Activity::Ago(3_600_000))),
            "authoring · 1:02:03 · last file activity 1:00:00 ago · usually 5–11 min, no timeout"
        );
    }

    #[test]
    fn past_eleven_minutes_the_line_stays_calm() {
        assert_eq!(
            line(authoring(754_999, Activity::Ago(661_000))),
            "authoring · 12:34 · last file activity 11:01 ago · usually 5–11 min, no timeout"
        );
    }

    #[test]
    fn a_file_whose_hunks_appear_change_or_go_is_file_activity() {
        let file = |name: &str, text: &str| story::FileHunks {
            file: name.to_string(),
            hunks: Vec::new(),
            old_exists: true,
            old_text: String::new(),
            new_exists: true,
            new_text: text.to_string(),
            head_exists: true,
            head_text: String::new(),
        };
        let before: Arc<[story::FileHunks]> = vec![
            file("kept.rs", "a"),
            file("edited.rs", "a"),
            file("gone.rs", "a"),
        ]
        .into();
        let state = State {
            root: PathBuf::from("/w"),
            file_hunks: vec![
                file("kept.rs", "a"),
                file("edited.rs", "b"),
                file("new.rs", "a"),
            ]
            .into(),
            ..State::default()
        };
        assert_eq!(
            hunk_activity(&before, &state),
            ["/w/edited.rs", "/w/new.rs", "/w/gone.rs"]
                .map(|path| Event::FileActivity(PathBuf::from(path)))
        );
        assert_eq!(hunk_activity(&state.file_hunks, &state), []);
    }
}
