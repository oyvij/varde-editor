use crate::{risk, story, Event, State};
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
    Install,
    Session,
    Walkthrough,
}

fn all(state: &State) -> impl Iterator<Item = (Kind, &Wait)> {
    let authoring = match &state.story_set {
        story::Set::Authoring { wait, .. } => Some((Kind::Authoring, wait)),
        _ => None,
    };
    let install = (state.installing.as_ref()).map(|install| (Kind::Install, &install.wait));
    let session = match &state.refactor.running {
        Some(risk::Iteration {
            wait: risk::Wait::Session(wait),
            ..
        }) => Some((Kind::Session, wait)),
        _ => None,
    };
    let walkthrough = (state.walkthrough.as_ref())
        .filter(|session| session.queue.running.is_some())
        .map(|session| (Kind::Walkthrough, &session.queue.wait));
    authoring
        .into_iter()
        .chain(install)
        .chain(session)
        .chain(walkthrough)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watcher {
    Trusted,
    Blind,
}

const NFS_SUPER_MAGIC: u32 = 0x6969;
const SMB_SUPER_MAGIC: u32 = 0x517b;
const CIFS_MAGIC_NUMBER: u32 = 0xff53_4d42;
const SMB2_MAGIC_NUMBER: u32 = 0xfe53_4d42;
const FUSE_SUPER_MAGIC: u32 = 0x6573_5546;
const V9FS_MAGIC: u32 = 0x0102_1997;
const AFS_SUPER_MAGIC: u32 = 0x5346_414f;
const AFS_FS_MAGIC: u32 = 0x6b41_4653;
const CEPH_SUPER_MAGIC: u32 = 0x00c3_6400;

pub fn linux_watcher(f_type: u32) -> Watcher {
    match f_type {
        NFS_SUPER_MAGIC | SMB_SUPER_MAGIC | CIFS_MAGIC_NUMBER | SMB2_MAGIC_NUMBER
        | FUSE_SUPER_MAGIC | V9FS_MAGIC | AFS_SUPER_MAGIC | AFS_FS_MAGIC | CEPH_SUPER_MAGIC => {
            Watcher::Blind
        }
        _ => Watcher::Trusted,
    }
}

pub fn macos_watcher(fstypename: &str) -> Watcher {
    match fstypename {
        "nfs" | "smbfs" | "afpfs" | "webdav" => Watcher::Blind,
        fuse if fuse.contains("fuse") => Watcher::Blind,
        _ => Watcher::Trusted,
    }
}

fn in_working_tree(state: &State, path: &Path) -> bool {
    path.starts_with(&state.root)
        && !path.starts_with(state.root.join(".git"))
        && !path
            .ancestors()
            .any(|folder| state.ignored.contains(folder))
}

pub fn depends_on(state: &State, folder: &Path) -> bool {
    in_working_tree(state, folder)
        || folder.starts_with(crate::varde_dir(&state.root, state.sidecar.as_deref()))
}

pub fn all_mut(state: &mut State) -> impl Iterator<Item = (Kind, &mut Wait)> {
    let authoring = match &mut state.story_set {
        story::Set::Authoring { wait, .. } => Some((Kind::Authoring, wait)),
        _ => None,
    };
    let install = (state.installing.as_mut()).map(|install| (Kind::Install, &mut install.wait));
    let session = match &mut state.refactor.running {
        Some(risk::Iteration {
            wait: risk::Wait::Session(wait),
            ..
        }) => Some((Kind::Session, wait)),
        _ => None,
    };
    let walkthrough = (state.walkthrough.as_mut())
        .filter(|session| session.queue.running.is_some())
        .map(|session| (Kind::Walkthrough, &mut session.queue.wait));
    authoring
        .into_iter()
        .chain(install)
        .chain(session)
        .chain(walkthrough)
}

pub fn counts(state: &State, kind: Kind, path: &Path) -> bool {
    let varde = crate::varde_dir(&state.root, state.sidecar.as_deref());
    if kind == Kind::Authoring && path.starts_with(varde.join("stories")) {
        return true;
    }
    in_working_tree(state, path) && !path.starts_with(&varde)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub kind: Kind,
    pub elapsed_ms: u64,
    pub activity: Activity,
}

pub fn every(state: &State) -> impl Iterator<Item = Progress> + '_ {
    all(state).map(|(kind, wait)| Progress {
        kind,
        elapsed_ms: wait
            .since_ms
            .map_or(0, |since| state.clock_ms.saturating_sub(since)),
        activity: match (state.watcher, wait.activity_ms) {
            (Watcher::Blind, _) => Activity::Unknown,
            (Watcher::Trusted, None) => Activity::NoneYet,
            (Watcher::Trusted, Some(at)) => Activity::Ago(state.clock_ms.saturating_sub(at)),
        },
    })
}

pub fn progress(state: &State, kind: Kind) -> Option<Progress> {
    every(state).find(|progress| progress.kind == kind)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    NoneYet,
    Ago(u64),
    Unknown,
}

pub fn line(progress: Progress) -> String {
    let elapsed = clock(progress.elapsed_ms, 2);
    let activity = match progress.activity {
        Activity::NoneYet => "no file activity yet".to_string(),
        Activity::Ago(ms) => format!("last file activity {} ago", clock(ms, 1)),
        Activity::Unknown => "file activity unknown".to_string(),
    };
    match progress.kind {
        Kind::Authoring => {
            format!("authoring · {elapsed} · {activity} · usually 5–11 min, no timeout")
        }
        Kind::Install => format!("installing · {elapsed} · no timeout"),
        Kind::Session => format!("session · {elapsed} · {activity} · no timeout"),
        Kind::Walkthrough => format!("{elapsed} · {activity} · no timeout"),
    }
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
    fn the_session_wait_says_no_timeout_and_claims_no_usual_range() {
        assert_eq!(
            line(Progress {
                kind: Kind::Session,
                elapsed_ms: 151_000,
                activity: Activity::Ago(12_000),
            }),
            "session · 02:31 · last file activity 0:12 ago · no timeout"
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
    fn an_install_says_its_elapsed_time_and_no_timeout_but_never_its_file_activity() {
        for activity in [Activity::NoneYet, Activity::Ago(40_000), Activity::Unknown] {
            assert_eq!(
                line(Progress {
                    kind: Kind::Install,
                    elapsed_ms: 107_000,
                    activity,
                }),
                "installing · 01:47 · no timeout"
            );
        }
    }

    #[test]
    fn a_failed_watch_of_the_sidecar_stories_folder_makes_the_watcher_blind() {
        let state = State {
            root: PathBuf::from("/w"),
            sidecar: Some(PathBuf::from("/elsewhere/w")),
            watcher: Watcher::Trusted,
            ..State::default()
        };
        let failed = Event::WatchFailed(PathBuf::from("/elsewhere/w/stories"));
        assert_eq!(crate::update(&state, failed).0.watcher, Watcher::Blind);
        let elsewhere = Event::WatchFailed(PathBuf::from("/elsewhere/other"));
        assert_eq!(crate::update(&state, elsewhere).0.watcher, Watcher::Trusted);
    }

    #[test]
    fn network_and_fuse_filesystems_on_linux_make_the_watcher_blind() {
        let nfs = 0x6969;
        let smb = 0x517b;
        let cifs = 0xff53_4d42;
        let smb2 = 0xfe53_4d42;
        let fuse = 0x6573_5546;
        let v9fs = 0x0102_1997;
        let afs = 0x5346_414f;
        let kafs = 0x6b41_4653;
        let ceph = 0x00c3_6400;
        for f_type in [nfs, smb, cifs, smb2, fuse, v9fs, afs, kafs, ceph] {
            assert_eq!(linux_watcher(f_type), Watcher::Blind, "{f_type:#x}");
        }
        let ext4 = 0xef53;
        let btrfs = 0x9123_683e;
        let xfs = 0x5846_5342;
        let tmpfs = 0x0102_1994;
        let overlay = 0x794c_7630;
        for f_type in [ext4, btrfs, xfs, tmpfs, overlay] {
            assert_eq!(linux_watcher(f_type), Watcher::Trusted, "{f_type:#x}");
        }
    }

    #[test]
    fn network_and_fuse_filesystems_on_macos_make_the_watcher_blind() {
        for name in [
            "nfs", "smbfs", "afpfs", "webdav", "macfuse", "osxfuse", "fusefs",
        ] {
            assert_eq!(macos_watcher(name), Watcher::Blind, "{name}");
        }
        for name in ["apfs", "hfs", "msdos", "exfat"] {
            assert_eq!(macos_watcher(name), Watcher::Trusted, "{name}");
        }
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
