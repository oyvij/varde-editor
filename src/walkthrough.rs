use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub const FILE: &str = "walkthrough.json";

pub const VERSION: u32 = 1;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
