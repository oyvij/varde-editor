use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

pub const FILE: &str = "walkthrough.json";

pub const VERSION: u32 = 1;

pub const SKILL: &str = "workflows/walkthrough/walkthrough";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plan {
    pub version: u32,
    pub status: Status,
    pub checkpoint: Option<String>,
    pub waypoints: Vec<Waypoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Active,
    Completed,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Waypoint {
    pub id: String,
    pub file: String,
    pub line: usize,
    pub kind: Kind,
    pub text: String,
    pub verdict: Option<Verdict>,
    pub ai: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Code,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Verdict {
    Lgtm,
    Failing { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Unparsable(String),
    Waypoints(Vec<(String, Problem)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    LineOutOfRange,
    OutsideWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Session {
    pub plan: Option<Plan>,
    pub failure: Option<Failure>,
    pub queue: Queue,
    pub fixes: u8,
    pub current: Option<String>,
    pub inserted: BTreeSet<String>,
    pub missing: BTreeSet<String>,
    pub opening: Option<String>,
    pub box_focus: Option<usize>,
    pub selected: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Driver {
    #[default]
    Pane,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Plan,
    Evaluate,
    Adjust,
    Write,
    Revert,
    Fix,
}

impl Action {
    pub fn slug(self) -> &'static str {
        match self {
            Action::Plan => "plan",
            Action::Evaluate => "evaluate",
            Action::Adjust => "adjust",
            Action::Write => "write",
            Action::Revert => "revert",
            Action::Fix => "fix",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub action: Action,
    pub waypoint: Option<String>,
    pub detail: String,
}

impl Request {
    pub fn line(&self, skill: &Path, root: &Path, plan: &Path) -> String {
        let mut line = format!(
            "{} Action: {}. Plan file: {}.",
            crate::skills::pointer(skill, root, None),
            self.action.slug(),
            plan.display()
        );
        if let Some(id) = &self.waypoint {
            line.push_str(&format!(" Waypoint: {id}."));
        }
        if !self.detail.is_empty() {
            line.push(' ');
            line.push_str(&self.detail);
        }
        line.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Queue {
    pub running: Option<Request>,
    pub waiting: Vec<Request>,
    pub wait: crate::wait::Wait,
}

impl Queue {
    pub fn push(&mut self, request: Request) -> Option<Request> {
        if self.running.is_none() {
            self.running = Some(request.clone());
            self.wait = crate::wait::Wait::default();
            return Some(request);
        }
        let first_evaluation = self
            .waiting
            .iter()
            .position(|waiting| waiting.action == Action::Evaluate);
        match (request.action, first_evaluation) {
            (Action::Evaluate, _) => {
                self.waiting
                    .retain(|waiting| waiting.action != Action::Evaluate);
                self.waiting.push(request);
            }
            (Action::Adjust | Action::Revert, Some(at)) => self.waiting.insert(at, request),
            _ => self.waiting.push(request),
        }
        None
    }

    pub fn finish(&mut self) -> Option<Request> {
        self.running = None;
        self.wait = crate::wait::Wait::default();
        match self.waiting.is_empty() {
            true => None,
            false => {
                let next = self.waiting.remove(0);
                self.push(next)
            }
        }
    }

    pub fn spinner(&self) -> Option<String> {
        let running = self.running.as_ref()?;
        let mut line = format!("{} running", running.action.slug());
        if !self.waiting.is_empty() {
            let queued: Vec<&str> = self
                .waiting
                .iter()
                .map(|request| request.action.slug())
                .collect();
            line.push_str(&format!(" · {} queued", queued.join(", ")));
        }
        Some(line)
    }
}

pub const PREVIOUS: &str = "waypoint-previous";
pub const NEXT: &str = "waypoint-next";

const BOX_COLUMNS: usize = 72;
const BOX_ROWS: usize = 8;

pub struct ContextBox {
    pub line: usize,
    pub width: usize,
    pub rows: Vec<crate::preview::Row>,
    pub scroll: usize,
}

impl ContextBox {
    pub fn shown(&self) -> &[crate::preview::Row] {
        let end = (self.scroll + BOX_ROWS).min(self.rows.len());
        &self.rows[self.scroll.min(end)..end]
    }

    pub fn last_scroll(&self) -> usize {
        self.rows.len().saturating_sub(BOX_ROWS)
    }

    pub fn placement(&self) -> crate::lsp::Placement {
        crate::lsp::Placement {
            from: self.line + 1,
            column: 1,
            width: self.width,
            rows: self.shown().len() + 2,
        }
    }
}

pub fn current(state: &crate::State) -> Option<&Waypoint> {
    let session = state.walkthrough.as_ref()?;
    let id = session.current.as_deref()?;
    session
        .plan
        .as_ref()?
        .waypoints
        .iter()
        .find(|waypoint| waypoint.id == id)
}

fn shown_context(state: &crate::State) -> Option<&Waypoint> {
    current(state).filter(|waypoint| {
        waypoint.kind == Kind::Context
            && state.current_buffer.as_deref() == Some(state.root.join(&waypoint.file).as_path())
    })
}

pub fn context_mark(state: &crate::State, path: &Path, line: usize) -> bool {
    shown_context(state)
        .is_some_and(|waypoint| waypoint.line == line && state.root.join(&waypoint.file) == path)
}

pub fn context_box(state: &crate::State) -> Option<ContextBox> {
    let waypoint = shown_context(state)?;
    let width = BOX_COLUMNS.min(crate::panes_of(state).editor.width as usize);
    let rows = crate::preview::rows(&waypoint.text, width.saturating_sub(2));
    Some(ContextBox {
        line: waypoint.line,
        width,
        scroll: state
            .walkthrough
            .as_ref()
            .and_then(|session| session.box_focus)
            .unwrap_or(0),
        rows,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Lgtm,
    Open,
    Failing,
    Context,
    New,
}

impl Mark {
    pub fn slug(self) -> &'static str {
        match self {
            Mark::Lgtm => "lgtm",
            Mark::Open => "open",
            Mark::Failing => "failing",
            Mark::Context => "context",
            Mark::New => "new",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listed {
    File(String),
    Waypoint {
        id: String,
        text: String,
        mark: Mark,
        ai: bool,
        lit: bool,
    },
}

fn mark(waypoint: &Waypoint, missing: &BTreeSet<String>) -> Mark {
    match (waypoint.kind, &waypoint.verdict) {
        (Kind::Context, _) => Mark::Context,
        _ if missing.contains(&waypoint.file) => Mark::New,
        (Kind::Code, Some(Verdict::Lgtm)) => Mark::Lgtm,
        (Kind::Code, Some(Verdict::Failing { .. })) => Mark::Failing,
        (Kind::Code, None) => Mark::Open,
    }
}

pub fn listed(state: &crate::State) -> Vec<Listed> {
    let Some(Session {
        plan: Some(plan),
        current,
        missing,
        ..
    }) = &state.walkthrough
    else {
        return Vec::new();
    };
    let mut files: Vec<&str> = Vec::new();
    for waypoint in &plan.waypoints {
        if !files.contains(&waypoint.file.as_str()) {
            files.push(&waypoint.file);
        }
    }
    files
        .into_iter()
        .flat_map(|file| {
            let nested = plan
                .waypoints
                .iter()
                .filter(move |waypoint| waypoint.file == file)
                .map(|waypoint| Listed::Waypoint {
                    id: waypoint.id.clone(),
                    text: waypoint.text.clone(),
                    mark: mark(waypoint, missing),
                    ai: waypoint.ai,
                    lit: current.as_deref() == Some(waypoint.id.as_str()),
                });
            std::iter::once(Listed::File(file.to_string())).chain(nested)
        })
        .collect()
}

pub fn listed_ids(state: &crate::State) -> Vec<String> {
    listed(state)
        .into_iter()
        .filter_map(|row| match row {
            Listed::Waypoint { id, .. } => Some(id),
            Listed::File(_) => None,
        })
        .collect()
}

pub fn selected(state: &crate::State) -> Option<String> {
    let at = state.walkthrough.as_ref()?.selected;
    listed_ids(state).into_iter().nth(at)
}

pub fn coloured(state: &crate::State, path: &Path) -> bool {
    let Some(Session {
        plan: Some(plan),
        missing,
        ..
    }) = &state.walkthrough
    else {
        return false;
    };
    plan.waypoints.iter().any(|waypoint| {
        !missing.contains(&waypoint.file) && state.root.join(&waypoint.file) == path
    })
}

pub fn transport(state: &crate::State) -> Vec<crate::Chip> {
    use crate::{Chip, Hue, Tone};
    if current(state).is_none() {
        return Vec::new();
    }
    let chip = |action, name, glyph: &str, keys| Chip {
        action,
        name,
        glyph: glyph.to_string(),
        keys,
        word: "",
        hue: Hue::Step,
        tone: Tone::Plain,
    };
    vec![
        chip(PREVIOUS, "previous", "\u{ab}", "["),
        chip(NEXT, "next", "\u{bb}", "]"),
    ]
}

impl Failure {
    pub fn slug(&self) -> &'static str {
        match self {
            Failure::Unparsable(_) => "unparsable",
            Failure::Waypoints(_) => "waypoints",
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Failure::Unparsable(because) => because.clone(),
            Failure::Waypoints(problems) => problems
                .iter()
                .map(|(id, problem)| format!("{id} {}", problem.slug()))
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

impl Problem {
    pub fn slug(self) -> &'static str {
        match self {
            Problem::LineOutOfRange => "line-out-of-range",
            Problem::OutsideWorkspace => "outside-workspace",
        }
    }
}

pub fn parse(text: &str) -> Result<Plan, Failure> {
    let plan: Plan =
        serde_json::from_str(text).map_err(|error| Failure::Unparsable(error.to_string()))?;
    if plan.version != VERSION {
        return Err(Failure::Unparsable(format!(
            "version {} is not version {VERSION}",
            plan.version
        )));
    }
    Ok(plan)
}

fn inside_workspace(file: &str) -> bool {
    !file.is_empty()
        && Path::new(file)
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

pub fn files(plan: &Plan) -> Vec<String> {
    let mut files: Vec<String> = plan
        .waypoints
        .iter()
        .map(|waypoint| waypoint.file.clone())
        .filter(|file| inside_workspace(file))
        .collect();
    files.sort();
    files.dedup();
    files
}

pub fn check(plan: &Plan, lines: &BTreeMap<String, usize>) -> Result<(), Failure> {
    let problems: Vec<(String, Problem)> = plan
        .waypoints
        .iter()
        .filter_map(|waypoint| {
            let last = lines
                .get(&waypoint.file)
                .map_or(usize::MAX, |&count| count.max(1));
            let problem = if !inside_workspace(&waypoint.file) {
                Problem::OutsideWorkspace
            } else if !(1..=last).contains(&waypoint.line) {
                Problem::LineOutOfRange
            } else {
                return None;
            };
            Some((waypoint.id.clone(), problem))
        })
        .collect();
    match problems.is_empty() {
        true => Ok(()),
        false => Err(Failure::Waypoints(problems)),
    }
}

pub fn comment_token(file: &str) -> &'static str {
    match Path::new(file)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(
            "py" | "sh" | "bash" | "zsh" | "rb" | "toml" | "yaml" | "yml" | "pl" | "r" | "ex"
            | "exs",
        ) => "#",
        Some("sql" | "lua" | "hs" | "elm") => "--",
        _ => "//",
    }
}

pub fn with_comment(text: &str, line: usize, token: &str, comment: &str) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    let at = line.saturating_sub(1).min(lines.len());
    let held = lines.get(at).copied().unwrap_or_default();
    let indent = &held[..held.len() - held.trim_start().len()];
    let commented = format!("{indent}{token} {comment}");
    lines.splice(at..at, [commented.as_str(), indent]);
    lines.join("\n")
}

pub fn follow(waypoints: &mut [Waypoint], file: &str, old: &str, new: &str) {
    let carried = crate::debug::carry(old, new);
    let last = new.split('\n').count();
    for waypoint in waypoints
        .iter_mut()
        .filter(|waypoint| waypoint.file == file)
    {
        let (Ok(line) | Err(line)) = carried(waypoint.line);
        waypoint.line = line.clamp(1, last);
    }
}

pub fn step<'a>(
    plan: &'a Plan,
    from: Option<&str>,
    direction: crate::Direction,
) -> Option<&'a str> {
    let at = plan
        .waypoints
        .iter()
        .position(|waypoint| Some(waypoint.id.as_str()) == from);
    let to = match (at, direction) {
        (None, _) => 0,
        (Some(at), crate::Direction::Left | crate::Direction::Up) => at.saturating_sub(1),
        (Some(at), _) => (at + 1).min(plan.waypoints.len().saturating_sub(1)),
    };
    plan.waypoints.get(to).map(|waypoint| waypoint.id.as_str())
}

pub fn landing<'a>(plan: &'a Plan, current: Option<&str>) -> Option<&'a str> {
    let named = |id: Option<&str>| {
        plan.waypoints
            .iter()
            .find(|waypoint| Some(waypoint.id.as_str()) == id)
    };
    named(plan.checkpoint.as_deref())
        .or_else(|| named(current))
        .or(plan.waypoints.first())
        .map(|waypoint| waypoint.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Direction;

    fn plan(waypoints: &str) -> String {
        format!(
            r#"{{"version": 1, "status": "active", "checkpoint": "w1", "waypoints": [{waypoints}]}}"#
        )
    }

    fn waypoint(id: &str, file: &str, line: usize) -> String {
        format!(
            r#"{{"id": "{id}", "file": "{file}", "line": {line}, "kind": "code", "text": "Write it", "verdict": null, "ai": false}}"#
        )
    }

    fn lines(files: &[(&str, usize)]) -> BTreeMap<String, usize> {
        files
            .iter()
            .map(|(file, count)| (file.to_string(), *count))
            .collect()
    }

    #[test]
    fn a_valid_plan_keeps_its_waypoints_in_file_order() {
        let text = plan(&format!(
            "{}, {}, {}",
            waypoint("w1", "src/plan.rs", 12),
            waypoint("w2", "src/lib.rs", 3),
            waypoint("w3", "src/plan.rs", 2)
        ));
        let parsed = parse(&text).expect("a plan");
        let ids: Vec<&str> = parsed.waypoints.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["w1", "w2", "w3"]);
        assert_eq!(parsed.status, Status::Active);
        assert_eq!(parsed.checkpoint.as_deref(), Some("w1"));
        assert_eq!(
            check(&parsed, &lines(&[("src/plan.rs", 20), ("src/lib.rs", 40)])),
            Ok(())
        );
    }

    #[test]
    fn a_waypoint_reads_its_kind_verdict_and_authorship() {
        let parsed = parse(
            r#"{"version": 1, "status": "completed", "checkpoint": null, "waypoints": [
              {"id": "w1", "file": "a.rs", "line": 1, "kind": "context", "text": "Why", "verdict": {"state": "lgtm"}, "ai": true},
              {"id": "w2", "file": "a.rs", "line": 2, "kind": "code", "text": "Do", "verdict": {"state": "failing", "message": "Empty id accepted"}, "ai": false}
            ]}"#,
        )
        .expect("a plan");
        assert_eq!(parsed.status, Status::Completed);
        assert_eq!(parsed.checkpoint, None);
        assert_eq!(parsed.waypoints[0].kind, Kind::Context);
        assert_eq!(parsed.waypoints[0].verdict, Some(Verdict::Lgtm));
        assert!(parsed.waypoints[0].ai);
        assert_eq!(
            parsed.waypoints[1].verdict,
            Some(Verdict::Failing {
                message: "Empty id accepted".to_string()
            })
        );
    }

    #[test]
    fn a_file_that_does_not_parse_is_unparsable() {
        let failure = parse(r#"{"version": 1, "status": "active", "waypoints": ["#);
        assert!(matches!(failure, Err(Failure::Unparsable(_))));
        assert!(matches!(parse("not json"), Err(Failure::Unparsable(_))));
    }

    #[test]
    fn an_unknown_status_kind_or_version_is_unparsable() {
        let unparsable = |text: &str| matches!(parse(text), Err(Failure::Unparsable(_)));
        assert!(unparsable(
            r#"{"version": 1, "status": "paused", "checkpoint": null, "waypoints": []}"#
        ));
        assert!(unparsable(&plan(
            &waypoint("w1", "a.rs", 1).replace("code", "prose")
        )));
        assert!(unparsable(
            r#"{"version": 2, "status": "active", "checkpoint": null, "waypoints": []}"#
        ));
    }

    #[test]
    fn a_line_past_the_end_of_its_file_is_out_of_range() {
        let parsed = parse(&plan(&format!(
            "{}, {}, {}",
            waypoint("w1", "src/plan.rs", 12),
            waypoint("w9", "src/lib.rs", 41),
            waypoint("w0", "src/lib.rs", 0)
        )))
        .expect("a plan");
        assert_eq!(
            check(&parsed, &lines(&[("src/plan.rs", 20), ("src/lib.rs", 40)])),
            Err(Failure::Waypoints(vec![
                ("w9".to_string(), Problem::LineOutOfRange),
                ("w0".to_string(), Problem::LineOutOfRange),
            ]))
        );
    }

    #[test]
    fn an_empty_file_takes_a_waypoint_on_its_first_line() {
        let parsed = parse(&plan(&waypoint("w1", "empty.rs", 1))).expect("a plan");
        assert_eq!(check(&parsed, &lines(&[("empty.rs", 0)])), Ok(()));
    }

    #[test]
    fn a_waypoint_in_a_missing_file_is_valid() {
        let parsed = parse(&plan(&waypoint("w1", "src/guide/plan.rs", 1))).expect("a plan");
        assert_eq!(check(&parsed, &lines(&[])), Ok(()));
    }

    #[test]
    fn a_path_leaving_the_workspace_is_outside_it() {
        for file in ["../elsewhere/plan.rs", "/etc/passwd", "src/../../x.rs", ""] {
            let parsed = parse(&plan(&waypoint("w1", file, 1))).expect("a plan");
            assert_eq!(
                check(&parsed, &lines(&[])),
                Err(Failure::Waypoints(vec![(
                    "w1".to_string(),
                    Problem::OutsideWorkspace
                )])),
                "{file}"
            );
            assert!(files(&parsed).is_empty(), "{file}");
        }
    }

    #[test]
    fn the_files_to_read_are_each_named_once() {
        let parsed = parse(&plan(&format!(
            "{}, {}, {}",
            waypoint("w1", "src/plan.rs", 12),
            waypoint("w2", "src/lib.rs", 3),
            waypoint("w3", "src/plan.rs", 2)
        )))
        .expect("a plan");
        assert_eq!(files(&parsed), ["src/lib.rs", "src/plan.rs"]);
    }

    fn request(action: Action, waypoint: &str) -> Request {
        Request {
            action,
            waypoint: Some(waypoint.to_string()),
            detail: String::new(),
        }
    }

    fn actions(queue: &Queue) -> Vec<(Action, Option<&str>)> {
        queue
            .waiting
            .iter()
            .map(|request| (request.action, request.waypoint.as_deref()))
            .collect()
    }

    #[test]
    fn a_request_with_nothing_in_flight_goes_at_once() {
        let mut queue = Queue::default();
        let adjust = request(Action::Adjust, "w1");
        assert_eq!(queue.push(adjust.clone()), Some(adjust.clone()));
        assert_eq!(queue.running, Some(adjust));
        assert!(queue.waiting.is_empty());
    }

    #[test]
    fn one_request_is_in_flight_and_the_rest_wait() {
        let mut queue = Queue::default();
        queue.push(request(Action::Evaluate, "w1"));
        assert_eq!(queue.push(request(Action::Adjust, "w1")), None);
        assert_eq!(
            queue.running.as_ref().map(|r| r.action),
            Some(Action::Evaluate)
        );
        assert_eq!(actions(&queue), [(Action::Adjust, Some("w1"))]);
    }

    #[test]
    fn a_newer_evaluation_replaces_a_queued_one() {
        let mut queue = Queue::default();
        queue.push(request(Action::Adjust, "w1"));
        queue.push(request(Action::Evaluate, "w1"));
        queue.push(request(Action::Fix, "w1"));
        queue.push(request(Action::Evaluate, "w2"));
        assert_eq!(
            actions(&queue),
            [(Action::Fix, Some("w1")), (Action::Evaluate, Some("w2"))]
        );
    }

    #[test]
    fn adjust_and_revert_queue_ahead_of_evaluations() {
        let mut queue = Queue::default();
        queue.push(request(Action::Write, "w1"));
        queue.push(request(Action::Evaluate, "w1"));
        queue.push(request(Action::Adjust, "w1"));
        queue.push(request(Action::Revert, "w1"));
        assert_eq!(
            actions(&queue),
            [
                (Action::Adjust, Some("w1")),
                (Action::Revert, Some("w1")),
                (Action::Evaluate, Some("w1")),
            ]
        );
    }

    #[test]
    fn finishing_sends_the_next_request_in_line() {
        let mut queue = Queue::default();
        queue.push(request(Action::Adjust, "w1"));
        queue.push(request(Action::Evaluate, "w1"));
        let evaluate = request(Action::Evaluate, "w1");
        assert_eq!(queue.finish(), Some(evaluate.clone()));
        assert_eq!(queue.running, Some(evaluate));
        assert_eq!(queue.finish(), None);
        assert_eq!(queue, Queue::default());
    }

    #[test]
    fn the_spinner_names_what_runs_and_what_waits() {
        let mut queue = Queue::default();
        assert_eq!(queue.spinner(), None);
        queue.push(request(Action::Adjust, "w1"));
        assert_eq!(queue.spinner().as_deref(), Some("adjust running"));
        queue.push(request(Action::Revert, "w1"));
        queue.push(request(Action::Evaluate, "w2"));
        assert_eq!(
            queue.spinner().as_deref(),
            Some("adjust running · revert, evaluate queued")
        );
    }

    #[test]
    fn a_request_is_one_line_naming_the_skill_action_plan_and_waypoint() {
        let request = Request {
            action: Action::Adjust,
            waypoint: Some("w2".to_string()),
            detail: "The developer says: don't\nduplicate this".to_string(),
        };
        assert_eq!(
            request.line(
                Path::new("/home/me/.varde/ai/skills/workflows/walkthrough/walkthrough/SKILL.md"),
                Path::new("/w"),
                Path::new("/w/.varde/walkthrough.json"),
            ),
            "Follow the Skill in /home/me/.varde/ai/skills/workflows/walkthrough/walkthrough/SKILL.md \
             for the workspace at /w. Action: adjust. Plan file: /w/.varde/walkthrough.json. \
             Waypoint: w2. The developer says: don't duplicate this"
        );
    }

    #[test]
    fn a_request_before_any_waypoint_names_none() {
        let request = Request {
            action: Action::Plan,
            waypoint: None,
            detail: String::new(),
        };
        assert_eq!(
            request.line(
                Path::new("/s/SKILL.md"),
                Path::new("/w"),
                Path::new("/w/p.json")
            ),
            "Follow the Skill in /s/SKILL.md for the workspace at /w. Action: plan. \
             Plan file: /w/p.json."
        );
    }

    #[test]
    fn a_failure_names_each_failing_waypoint() {
        let failure = Failure::Waypoints(vec![
            ("w9".to_string(), Problem::LineOutOfRange),
            ("w1".to_string(), Problem::OutsideWorkspace),
        ]);
        assert_eq!(
            failure.describe(),
            "w9 line-out-of-range, w1 outside-workspace"
        );
    }

    #[test]
    fn a_landing_comment_takes_the_line_and_its_indent_and_opens_one_below() {
        assert_eq!(
            with_comment("fn load() {\n    let text = 1;\n}", 2, "//", "Parse it"),
            "fn load() {\n    // Parse it\n    \n    let text = 1;\n}"
        );
    }

    #[test]
    fn a_landing_comment_takes_the_languages_own_token() {
        assert_eq!(comment_token("src/plan.rs"), "//");
        assert_eq!(comment_token("web/app.ts"), "//");
        assert_eq!(comment_token("scripts/plan.py"), "#");
        assert_eq!(comment_token("deploy.sh"), "#");
        assert_eq!(comment_token("db/schema.sql"), "--");
        assert_eq!(
            with_comment("import os\nprint(os.name)", 2, "#", "Read the plan"),
            "import os\n# Read the plan\n\nprint(os.name)"
        );
    }

    #[test]
    fn a_landing_comment_in_an_empty_file_is_its_first_line() {
        assert_eq!(with_comment("", 1, "//", "Start"), "// Start\n\n");
    }

    fn followed(old: &str, new: &str) -> Vec<usize> {
        let mut waypoints: Vec<Waypoint> = parse(&plan(&format!(
            "{}, {}, {}",
            waypoint("w1", "a.rs", 2),
            waypoint("w2", "a.rs", 4),
            waypoint("w3", "b.rs", 2)
        )))
        .expect("a plan")
        .waypoints;
        follow(&mut waypoints, "a.rs", old, new);
        waypoints.iter().map(|waypoint| waypoint.line).collect()
    }

    #[test]
    fn a_waypoint_moves_with_lines_inserted_or_deleted_above_it() {
        let old = "a\nb\nc\nd\ne";
        assert_eq!(followed(old, "new\na\nb\nc\nd\ne"), [3, 5, 2]);
        assert_eq!(followed(old, "a\nb\nnew\nnew\nc\nd\ne"), [2, 6, 2]);
        assert_eq!(followed(old, "b\nc\nd\ne"), [1, 3, 2]);
        assert_eq!(followed(old, "a\nb\nd\ne"), [2, 3, 2]);
    }

    #[test]
    fn a_waypoint_stays_with_lines_inserted_or_deleted_below_it() {
        let old = "a\nb\nc\nd\ne";
        assert_eq!(followed(old, "a\nb\nc\nd\ne\nnew"), [2, 4, 2]);
        assert_eq!(followed(old, "a\nb\nc\nd"), [2, 4, 2]);
    }

    #[test]
    fn a_waypoint_stays_on_its_own_line_when_that_line_is_edited_or_deleted() {
        let old = "a\nb\nc\nd\ne";
        assert_eq!(followed(old, "a\nb edited\nc\nd\ne"), [2, 4, 2]);
        assert_eq!(followed(old, "a\nc\nd\ne"), [2, 3, 2]);
        assert_eq!(followed(old, "a\nb\nc\nd"), [2, 4, 2]);
        assert_eq!(followed(old, "a\nb\nc"), [2, 3, 2]);
    }

    #[test]
    fn a_waypoint_is_marked_by_its_kind_then_its_file_then_its_verdict() {
        let parsed = parse(
            r#"{"version": 1, "status": "active", "checkpoint": null, "waypoints": [
              {"id": "w1", "file": "new.rs", "line": 1, "kind": "context", "text": "Why", "verdict": null, "ai": false},
              {"id": "w2", "file": "new.rs", "line": 1, "kind": "code", "text": "Do", "verdict": {"state": "lgtm"}, "ai": false},
              {"id": "w3", "file": "a.rs", "line": 1, "kind": "code", "text": "Do", "verdict": {"state": "failing", "message": "No"}, "ai": false},
              {"id": "w4", "file": "a.rs", "line": 1, "kind": "code", "text": "Do", "verdict": {"state": "lgtm"}, "ai": true},
              {"id": "w5", "file": "a.rs", "line": 1, "kind": "code", "text": "Do", "verdict": null, "ai": false}
            ]}"#,
        )
        .expect("a plan");
        let missing = BTreeSet::from(["new.rs".to_string()]);
        let marks: Vec<Mark> = parsed
            .waypoints
            .iter()
            .map(|waypoint| mark(waypoint, &missing))
            .collect();
        assert_eq!(
            marks,
            [
                Mark::Context,
                Mark::New,
                Mark::Failing,
                Mark::Lgtm,
                Mark::Open
            ]
        );
    }

    #[test]
    fn stepping_stays_at_either_end_of_the_plan() {
        let parsed = parse(&plan(&format!(
            "{}, {}",
            waypoint("w1", "a.rs", 1),
            waypoint("w2", "b.rs", 1)
        )))
        .expect("a plan");
        let stepped = |from: &str, direction| step(&parsed, Some(from), direction);
        assert_eq!(stepped("w1", Direction::Right), Some("w2"));
        assert_eq!(stepped("w2", Direction::Right), Some("w2"));
        assert_eq!(stepped("w2", Direction::Left), Some("w1"));
        assert_eq!(stepped("w1", Direction::Left), Some("w1"));
        assert_eq!(step(&parsed, None, Direction::Right), Some("w1"));
    }

    #[test]
    fn a_plan_lands_on_its_checkpoint_or_else_its_first_waypoint() {
        let text = plan(&format!(
            "{}, {}",
            waypoint("w1", "a.rs", 1),
            waypoint("w2", "b.rs", 1)
        ));
        let named =
            parse(&text.replace(r#""checkpoint": "w1""#, r#""checkpoint": "w2""#)).expect("a plan");
        let unnamed =
            parse(&text.replace(r#""checkpoint": "w1""#, r#""checkpoint": null"#)).expect("a plan");
        assert_eq!(landing(&named, None), Some("w2"));
        assert_eq!(landing(&named, Some("w1")), Some("w2"));
        assert_eq!(landing(&unnamed, None), Some("w1"));
        assert_eq!(landing(&unnamed, Some("w2")), Some("w2"));
        assert_eq!(landing(&unnamed, Some("gone")), Some("w1"));
    }
}
