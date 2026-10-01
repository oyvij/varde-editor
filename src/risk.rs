use crate::{Effect, Entry, State, View};
use serde::{Deserialize, Serialize};

pub const METRIC: &str = "CX";

pub const DEFAULT_THRESHOLD: u32 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Unknown,
    Function,
    Class,
    Struct,
    Trait,
    Impl,
    Unit,
    Namespace,
    Interface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub cyclomatic: u32,
    pub cognitive: u32,
    pub maintainability: u32,
    pub lines: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Space {
    pub name: Option<String>,
    pub line: usize,
    pub kind: Kind,
    pub metrics: Metrics,
    pub children: Vec<Space>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function {
    pub file: String,
    pub name: String,
    pub line: usize,
    pub metrics: Metrics,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Figures {
    pub functions: Vec<Function>,
    pub unparsed: usize,
}

impl Figures {
    pub fn nothing_analysed(&self) -> bool {
        self.functions.is_empty() && self.unparsed == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Figure {
    #[default]
    None,
    Current(Figures),
    Stale(Figures),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Risk {
    pub figure: Figure,
    pub asked: u64,
    pub answered: u64,
    pub scope: Scope,
    pub before: Option<Figures>,
}

impl Risk {
    pub fn in_flight(&self) -> bool {
        self.answered < self.asked
    }

    pub fn figures(&self) -> Option<&Figures> {
        match &self.figure {
            Figure::None => None,
            Figure::Current(figures) | Figure::Stale(figures) => Some(figures),
        }
    }
}

pub fn analyse(state: &mut State, scope: Scope) -> Effect {
    let (files, base) = match scope {
        Scope::Workspace => (None, None),
        Scope::Review => (
            Some(crate::review::list(state)),
            crate::review::base(state).map(str::to_string),
        ),
    };
    state.risk.asked += 1;
    state.risk.scope = scope;
    Effect::AnalyseRisk {
        scope,
        generation: state.risk.asked,
        files,
        base,
    }
}

fn arrived(
    risk: &mut Risk,
    generation: u64,
    figures: Figures,
    commit: Option<&str>,
) -> Option<String> {
    if generation != risk.asked || !risk.in_flight() {
        return None;
    }
    risk.answered = generation;
    let cache = commit.map(|commit| persist(&figures, commit));
    risk.figure = Figure::Current(figures);
    cache
}

pub fn went_stale(risk: &mut Risk) {
    if let Figure::Current(figures) = &risk.figure {
        risk.figure = Figure::Stale(figures.clone());
    }
}

pub const FILE: &str = "risk.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Persisted {
    pub commit: String,
    pub metric: String,
    pub functions: Vec<Function>,
    pub unparsed: usize,
}

pub fn persist(figures: &Figures, commit: &str) -> String {
    serde_json::to_string_pretty(&Persisted {
        commit: commit.to_string(),
        metric: METRIC.to_string(),
        functions: figures.functions.clone(),
        unparsed: figures.unparsed,
    })
    .expect("Varde's own shape holds nothing serde can refuse")
}

pub fn cached(json: &str, head: &str) -> Option<Figures> {
    let saved: Persisted = serde_json::from_str(json).ok()?;
    (saved.commit == head && saved.metric == METRIC).then_some(Figures {
        functions: saved.functions,
        unparsed: saved.unparsed,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    Workspace,
    Review,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Workspace => "workspace",
            Scope::Review => "review",
        }
    }
}

pub fn on_screen(view: View) -> Scope {
    match view {
        View::Review => Scope::Review,
        View::Edit | View::Story => Scope::Workspace,
    }
}

pub fn figures(analysed: Vec<(String, Option<Space>)>) -> Figures {
    let mut all = Vec::new();
    let mut unparsed = 0;
    for (file, space) in analysed {
        match space {
            Some(space) => {
                let (found, unnamed) = functions(&file, &space);
                all.extend(found);
                unparsed += unnamed;
            }
            None => unparsed += 1,
        }
    }
    Figures {
        functions: all,
        unparsed,
    }
}

pub fn functions(file: &str, root: &Space) -> (Vec<Function>, usize) {
    let mut found = Vec::new();
    let mut unnamed = 0;
    walk(file, root, &mut found, &mut unnamed);
    (found, unnamed)
}

fn walk(file: &str, space: &Space, found: &mut Vec<Function>, unnamed: &mut usize) {
    if space.kind == Kind::Function {
        match &space.name {
            Some(name) => found.push(Function {
                file: file.to_string(),
                name: name.clone(),
                line: space.line,
                metrics: space.metrics,
            }),
            None => *unnamed += 1,
        }
        return;
    }
    for child in &space.children {
        walk(file, child, found, unnamed);
    }
}

pub fn count(functions: &[Function], threshold: u32) -> usize {
    functions
        .iter()
        .filter(|function| function.metrics.cyclomatic > threshold)
        .count()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Computing,
    NothingAnalysed,
    Stale,
    Computed,
}

fn for_scope(risk: &Risk, scope: Scope) -> Option<&Figures> {
    let describes = match scope {
        Scope::Workspace => risk.before.is_none(),
        Scope::Review => risk.before.is_some(),
    };
    describes.then(|| risk.figures()).flatten()
}

fn scoped(state: &State) -> Option<&Figures> {
    for_scope(&state.risk, on_screen(state.view))
}

pub fn standing(state: &State) -> Standing {
    let Some(figures) = scoped(state) else {
        return if state.risk.in_flight() {
            Standing::Computing
        } else {
            Standing::NothingAnalysed
        };
    };
    if figures.nothing_analysed() {
        return Standing::NothingAnalysed;
    }
    match state.risk.figure {
        Figure::Stale(_) => Standing::Stale,
        Figure::Current(_) | Figure::None => Standing::Computed,
    }
}

pub fn view_state(state: &State) -> &'static str {
    match standing(state) {
        Standing::Computing => "computing",
        Standing::NothingAnalysed => "nothing-analysed",
        Standing::Stale => "stale",
        Standing::Computed => "computed",
    }
}

pub fn risk_count(state: &State) -> Option<usize> {
    scoped(state)
        .filter(|figures| !figures.nothing_analysed())
        .map(|figures| count(&figures.functions, state.risk_threshold))
}

pub fn list(state: &State) -> Vec<&Function> {
    let all = state.risk_all || state.view == View::Review;
    worst_first(scoped(state))
        .into_iter()
        .filter(|function| all || function.metrics.cyclomatic > state.risk_threshold)
        .collect()
}

fn worst_first(figures: Option<&Figures>) -> Vec<&Function> {
    let mut rows: Vec<&Function> = figures
        .map(|figures| figures.functions.iter().collect())
        .unwrap_or_default();
    rows.sort_by_key(|function| std::cmp::Reverse(function.metrics.cyclomatic));
    rows
}

pub fn selected(state: &State) -> Option<&Function> {
    list(state).get(state.risk_selection).copied()
}

pub const REFACTOR: &str = "refactor-function";

pub fn row_actions(state: &State) -> Vec<&'static str> {
    match selected(state) {
        Some(_) => vec![REFACTOR],
        None => Vec::new(),
    }
}

pub fn on_actions(state: &State) -> bool {
    state.focus == crate::Pane::Risk && state.risk_selection >= list(state).len()
}

pub fn refactor_prompt(function: &Function) -> String {
    format!(
        "Refactor the function `{name}` in {file}, at line {line}, to lower its complexity \
without changing its behaviour. Its {METRIC} figure is {figure} (cyclomatic {figure}, \
cognitive {cognitive}), measured over this working tree.\n\n\
The figure is the symptom, not the goal: extract only functions whose one responsibility \
their own name states, and where it cannot be split that way leave it as it is and tell me \
why, rather than trading nested complexity for structural scattering into helpers called \
from a single place and named after where they were cut from.\n\n\
Change nothing else, follow the convention files this repository holds, and do not commit: \
the change is for me to review in the working tree.",
        name = function.name,
        file = function.file,
        line = function.line,
        figure = function.metrics.cyclomatic,
        cognitive = function.metrics.cognitive,
    )
}

pub const SENTINEL: &str = "refactor-done";

pub const DEFAULT_MAX_ITERATIONS: u32 = 10;

const SHAPES: [(&str, &str); 8] = [
    ("Cargo.toml", "cargo test"),
    ("package.json", "npm test"),
    ("pyproject.toml", "pytest"),
    ("go.mod", "go test ./..."),
    ("pom.xml", "mvn test"),
    ("build.gradle", "gradle test"),
    ("build.gradle.kts", "gradle test"),
    ("Makefile", "make test"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    Session,
    Tests,
    Figures(u64),
}

impl Wait {
    pub fn as_str(self) -> &'static str {
        match self {
            Wait::Session => "waiting-for-session",
            Wait::Tests => "waiting-for-tests",
            Wait::Figures(_) => "waiting-for-figures",
        }
    }

    fn caption(self) -> &'static str {
        match self {
            Wait::Session => "session",
            Wait::Tests => "tests",
            Wait::Figures(_) => "measuring",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iteration {
    pub scope: Scope,
    pub number: u32,
    pub test_command: String,
    pub wait: Wait,
    pub before: Option<Figures>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Refactor {
    pub running: Option<Iteration>,
    pub refusal: Option<&'static str>,
    pub stopped: Option<&'static str>,
    pub tests: Option<(bool, String)>,
}

impl Refactor {
    pub fn last_test(&self) -> Option<&'static str> {
        self.tests
            .as_ref()
            .map(|(passed, _)| if *passed { "passed" } else { "failed" })
    }
}

pub fn test_command(configured: Option<&str>, entries: &[Entry]) -> Option<String> {
    if let Some(command) = configured
        .map(str::trim)
        .filter(|command| !command.is_empty())
    {
        return Some(command.to_string());
    }
    SHAPES
        .iter()
        .find(|(marker, _)| {
            entries
                .iter()
                .any(|entry| !entry.is_dir && entry.name == *marker)
        })
        .map(|(_, command)| command.to_string())
}

fn refused(state: &State, scope: Scope) -> Option<&'static str> {
    if state.refactor.running.is_some() {
        return Some(LOOP_ALREADY_RUNNING);
    }
    let coming = state.risk.in_flight() && state.risk.scope == scope;
    (for_scope(&state.risk, scope).is_none() && !coming).then_some(NO_FIGURE)
}

pub fn start(state: &mut State, scope: Scope) -> Vec<Effect> {
    if let Some(refusal) = refused(state, scope) {
        state.refactor.refusal = Some(refusal);
        return vec![Effect::Notify(refusal)];
    }
    state.refactor.refusal = None;
    let entries = state.contents.get(&state.root).cloned().unwrap_or_default();
    let Some(test_command) = test_command(state.test_command.as_deref(), &entries) else {
        state.refactor.refusal = Some("no-test-command");
        return vec![Effect::Notify("no-test-command")];
    };
    let prompt = loop_prompt(state, scope);
    state.refactor = Refactor {
        running: Some(Iteration {
            scope,
            number: 1,
            test_command,
            wait: Wait::Session,
            before: for_scope(&state.risk, scope).cloned(),
        }),
        ..Refactor::default()
    };
    let mut effects = vec![
        Effect::DeleteFile(crate::varde_dir(&state.root, state.sidecar.as_deref()).join(SENTINEL)),
        Effect::Snapshot { iteration: 1 },
    ];
    effects.extend(crate::queue_for_ai(state, crate::Enter::Pressed, prompt));
    effects
}

pub fn pass_reported(state: &mut State) -> Vec<Effect> {
    let Some(iteration) = state.refactor.running.as_mut() else {
        return vec![];
    };
    if iteration.wait != Wait::Session {
        return vec![];
    }
    iteration.wait = Wait::Tests;
    vec![Effect::RunTests {
        command: iteration.test_command.clone(),
    }]
}

pub fn tests_finished(state: &mut State, passed: bool, output: String) -> Vec<Effect> {
    let Some(iteration) = state.refactor.running.clone() else {
        return vec![];
    };
    state.refactor.tests = Some((passed, output.clone()));
    if passed {
        let asked = analyse(state, iteration.scope);
        state.refactor.running = Some(Iteration {
            wait: Wait::Figures(state.risk.asked),
            ..iteration
        });
        return vec![asked];
    }
    revert(state, TESTS_FAILED, &iteration, &output)
}

pub fn stop(state: &mut State) -> Vec<Effect> {
    let Some(iteration) = state.refactor.running.clone() else {
        return Vec::new();
    };
    state.refactor.running = None;
    state.refactor.stopped = Some(STOPPED);
    vec![
        Effect::RestoreSnapshot {
            iteration: iteration.number,
        },
        Effect::Notify(STOPPED),
    ]
}

pub const RECOMPUTE: &str = "recompute-risk";
pub const START_LOOP: &str = "start-refactor-loop";
pub const STOP_LOOP: &str = "stop-refactor-loop";

pub fn pane_actions(state: &State) -> Vec<&'static str> {
    vec![RECOMPUTE, loop_action(state)]
}

pub fn loop_action(state: &State) -> &'static str {
    match state.refactor.running {
        Some(_) => STOP_LOOP,
        None => START_LOOP,
    }
}

pub fn status(state: &State) -> Option<String> {
    let Some(iteration) = state.refactor.running.as_ref() else {
        return state
            .refactor
            .stopped
            .map(|condition| condition.replace('-', " "));
    };
    let mut said = format!(
        "{number}/{cap} {wait}",
        number = iteration.number,
        cap = state.max_iterations,
        wait = iteration.wait.caption(),
    );
    if let Some((passed, _)) = state.refactor.tests {
        said.push(' ');
        said.push(if passed { '✓' } else { '✗' });
    }
    Some(said)
}

pub const TESTS_FAILED: &str = "tests-failed";
pub const NO_IMPROVEMENT: &str = "no-improvement";
pub const OTHER_METRIC_WORSENED: &str = "other-metric-worsened";
pub const CAP_REACHED: &str = "cap-reached";

pub const LOOP_ALREADY_RUNNING: &str = "loop-already-running";
pub const NO_FIGURE: &str = "no-figure";
pub const NO_BASELINE: &str = "no-baseline";
pub const STOPPED: &str = "stopped";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Passed,
    NoImprovement,
    OtherMetricWorsened,
}

fn totals(figures: &Figures) -> Metrics {
    figures
        .functions
        .iter()
        .fold(Metrics::default(), |sum, function| Metrics {
            cyclomatic: sum.cyclomatic + function.metrics.cyclomatic,
            cognitive: sum.cognitive + function.metrics.cognitive,
            maintainability: sum.maintainability + function.metrics.maintainability,
            lines: sum.lines + function.metrics.lines,
        })
}

/// mi_visual_studio rises as code gets easier to maintain, so that index worsens downward
fn gate(before: &Figures, after: &Figures, threshold: u32) -> Verdict {
    let improved = count(&after.functions, threshold) < count(&before.functions, threshold)
        || totals(after).cyclomatic < totals(before).cyclomatic;
    if !improved {
        return Verdict::NoImprovement;
    }
    let Metrics {
        cyclomatic: _,
        cognitive,
        maintainability,
        lines,
    } = totals(before);
    let after = totals(after);
    match after.cognitive > cognitive
        || after.maintainability < maintainability
        || after.lines > lines
    {
        true => Verdict::OtherMetricWorsened,
        false => Verdict::Passed,
    }
}

pub fn figures_arrived(
    state: &mut State,
    generation: u64,
    figures: Figures,
    before: Option<Figures>,
) -> Vec<Effect> {
    let commit = state.head.clone().filter(|_| before.is_none());
    let mut effects = Vec::new();
    if let Some(contents) = arrived(&mut state.risk, generation, figures, commit.as_deref()) {
        effects.push(Effect::WriteFile {
            path: crate::varde_dir(&state.root, state.sidecar.as_deref()).join(FILE),
            contents,
        });
    }
    if state.risk.answered != generation {
        return effects;
    }
    state.risk.before = before;
    let waiting = state
        .refactor
        .running
        .as_ref()
        .filter(|iteration| iteration.wait == Wait::Session && iteration.before.is_none())
        .map(|iteration| iteration.scope);
    if let Some(scope) = waiting {
        let measured = for_scope(&state.risk, scope).cloned();
        if let Some(iteration) = state.refactor.running.as_mut() {
            iteration.before = measured;
        }
    }
    effects.extend(judge(state, generation));
    effects
}

fn judge(state: &mut State, generation: u64) -> Vec<Effect> {
    let Some(iteration) = state.refactor.running.clone() else {
        return Vec::new();
    };
    if iteration.wait != Wait::Figures(generation) {
        return Vec::new();
    }
    let Some(before) = iteration.before.clone() else {
        return revert(
            state,
            NO_BASELINE,
            &iteration,
            "the figures the pass would have been judged against were never measured",
        );
    };
    let after = state.risk.figures().cloned().unwrap_or_default();
    let explanation = verdict_report(&before, &after, state.risk_threshold);
    match gate(&before, &after, state.risk_threshold) {
        Verdict::Passed => accept(state, &iteration, after),
        Verdict::NoImprovement => revert(state, NO_IMPROVEMENT, &iteration, &explanation),
        Verdict::OtherMetricWorsened => {
            revert(state, OTHER_METRIC_WORSENED, &iteration, &explanation)
        }
    }
}

fn verdict_report(before: &Figures, after: &Figures, threshold: u32) -> String {
    let (was, now) = (totals(before), totals(after));
    format!(
        "{METRIC} above {threshold}: {} functions before, {} after. Totals — cyclomatic {} to {}, cognitive {} to {}, maintainability {} to {}, lines {} to {}.",
        count(&before.functions, threshold),
        count(&after.functions, threshold),
        was.cyclomatic,
        now.cyclomatic,
        was.cognitive,
        now.cognitive,
        was.maintainability,
        now.maintainability,
        was.lines,
        now.lines,
    )
}

fn accept(state: &mut State, iteration: &Iteration, after: Figures) -> Vec<Effect> {
    if iteration.number >= state.max_iterations {
        state.refactor.running = None;
        state.refactor.stopped = Some(CAP_REACHED);
        return vec![Effect::Notify(CAP_REACHED)];
    }
    let number = iteration.number + 1;
    state.refactor.running = Some(Iteration {
        number,
        wait: Wait::Session,
        before: Some(after),
        ..iteration.clone()
    });
    let prompt = loop_prompt(state, iteration.scope);
    let mut effects = vec![
        Effect::DeleteFile(crate::varde_dir(&state.root, state.sidecar.as_deref()).join(SENTINEL)),
        Effect::Snapshot { iteration: number },
    ];
    effects.extend(crate::queue_for_ai(state, crate::Enter::Pressed, prompt));
    effects
}

fn revert(
    state: &mut State,
    condition: &'static str,
    iteration: &Iteration,
    output: &str,
) -> Vec<Effect> {
    state.refactor.running = None;
    state.refactor.stopped = Some(condition);
    let mut effects = vec![
        Effect::RestoreSnapshot {
            iteration: iteration.number,
        },
        Effect::Notify(condition),
    ];
    effects.extend(crate::queue_for_ai(
        state,
        crate::Enter::Pressed,
        reverted_prompt(condition, output),
    ));
    effects
}

fn loop_prompt(state: &State, scope: Scope) -> String {
    let worst: String = worst_first(for_scope(&state.risk, scope))
        .iter()
        .take(3)
        .map(|function| {
            format!(
                "  - `{name}` in {file}, line {line} — cyclomatic {cyclomatic}, cognitive {cognitive}\n",
                name = function.name,
                file = function.file,
                line = function.line,
                cyclomatic = function.metrics.cyclomatic,
                cognitive = function.metrics.cognitive,
            )
        })
        .collect();
    let files = match scope {
        Scope::Workspace => String::new(),
        Scope::Review => format!(
            "Only these files are under review, and they are the whole of what you may change:\n\n{}\n",
            crate::review::list(state)
                .iter()
                .map(|file| format!("  - {file}\n"))
                .collect::<String>(),
        ),
    };
    format!(
        "Lower the Risk in this workspace, over the scope {scope}.\n\n\
{files}\
The target is no function above a {METRIC} figure of {threshold}. I measured the figures \
myself and wrote every one of them to {VARDE_DIR}/{FILE}; the worst are:\n\n\
{worst}\n\
Refactor those to lower their complexity without changing behaviour. Follow the convention \
files this repository holds, change nothing else, and do not commit: the working tree is what \
I review.\n\n\
The figure is the symptom, not the goal. Moving branches somewhere else lowers it without \
making anything clearer, so a split only counts when the pieces stand on their own: every \
function you extract must have one responsibility, and its name must say what that \
responsibility is. A helper called from exactly one place, or named after the part of the \
original it was cut out of rather than after what it does, has traded nested complexity for \
structural scattering and left the code harder to read than it found it. Fewer well-named \
extractions beat many small ones. Where a function cannot be split that way, leave it as it \
is and tell me why rather than shredding it.\n\n\
When the pass is finished, create the file {VARDE_DIR}/{SENTINEL}. Its contents are ignored. I then run \
this project's tests and measure the figures again myself, and put the whole pass back if \
either got worse — so finish and write that file rather than judging the pass yourself.",
        scope = scope.as_str(),
        threshold = state.risk_threshold,
        VARDE_DIR = crate::VARDE_DIR,
    )
}

fn reverted_prompt(condition: &str, output: &str) -> String {
    format!(
        "I put your last pass back: the working tree is as it was before it, and the loop has \
stopped.\n\nThe gate condition that failed is `{condition}`, and what ran said:\n\n{output}\n\n\
Do not redo the pass, and do not try to fix this on your own — wait for what I ask next.",
    )
}

pub fn unparsed(state: &State) -> usize {
    scoped(state).map_or(0, |figures| figures.unparsed)
}

pub fn job(state: &State) -> Option<(&'static str, &'static str)> {
    state.risk.in_flight().then_some(match state.risk.scope {
        Scope::Workspace => ("workspace-analysis", "measuring Risk"),
        Scope::Review => ("review-analysis", "measuring the change"),
    })
}

const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Count(usize),
    Delta { delta: i64, worse: bool },
}

pub fn shown(state: &State) -> Option<Shown> {
    let Some(before) = state.risk.before.as_ref() else {
        return risk_count(state).map(Shown::Count);
    };
    let after = scoped(state)?;
    let delta = i64::from(totals(after).cyclomatic) - i64::from(totals(before).cyclomatic);
    Some(Shown::Delta {
        delta,
        worse: delta > 0,
    })
}

pub fn row_delta(state: &State, function: &Function) -> Option<i64> {
    let before = state.risk.before.as_ref()?;
    let was = before
        .functions
        .iter()
        .find(|other| other.file == function.file && other.name == function.name)
        .map_or(0, |other| other.metrics.cyclomatic);
    Some(i64::from(function.metrics.cyclomatic) - i64::from(was))
}

pub fn border(state: &State) -> Option<String> {
    let figure = shown(state).map(|shown| {
        let said = match shown {
            Shown::Count(count) => format!("{METRIC} {count}"),
            Shown::Delta {
                delta,
                worse: false,
            } => format!("{METRIC} {delta:+}"),
            Shown::Delta { delta, worse: true } => format!("{METRIC} {delta:+} worse"),
        };
        match state.risk.figure {
            Figure::Stale(_) => format!("{said} stale"),
            Figure::Current(_) | Figure::None => said,
        }
    });
    let spinning = job(state).map(|(_, caption)| {
        format!(
            "{} {caption}",
            FRAMES[(state.tick % FRAMES.len() as u64) as usize]
        )
    });
    match (figure, spinning) {
        (Some(figure), Some(spinning)) => Some(format!("{figure} · {spinning}")),
        (figure, spinning) => figure.or(spinning),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn space(name: Option<&str>, kind: Kind, cyclomatic: u32, children: Vec<Space>) -> Space {
        Space {
            name: name.map(str::to_string),
            line: 1,
            kind,
            metrics: Metrics {
                cyclomatic,
                ..Metrics::default()
            },
            children,
        }
    }

    #[test]
    fn a_function_inside_a_container_is_the_only_function_counted() {
        let tree = space(
            Some("src/keys.rs"),
            Kind::Unit,
            40,
            vec![space(
                Some("Router"),
                Kind::Impl,
                31,
                vec![space(Some("route"), Kind::Function, 31, Vec::new())],
            )],
        );
        let (functions, unparsed) = functions("src/keys.rs", &tree);
        assert_eq!(
            functions
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            ["route"]
        );
        assert_eq!(unparsed, 0);
    }

    #[test]
    fn a_closure_is_counted_toward_the_function_holding_it() {
        let tree = space(
            Some("src/ui.rs"),
            Kind::Unit,
            22,
            vec![space(
                Some("draw"),
                Kind::Function,
                22,
                vec![space(Some("<anonymous>"), Kind::Function, 14, Vec::new())],
            )],
        );
        let (functions, _) = functions("src/ui.rs", &tree);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].metrics.cyclomatic, 22);
    }

    #[test]
    fn a_space_the_analyser_could_not_name_is_unparsed() {
        let tree = space(
            Some("src/keys.rs"),
            Kind::Unit,
            57,
            vec![
                space(Some("route"), Kind::Function, 31, Vec::new()),
                space(None, Kind::Function, 26, Vec::new()),
            ],
        );
        let (functions, unparsed) = functions("src/keys.rs", &tree);
        assert_eq!(functions.len(), 1);
        assert_eq!(unparsed, 1);
    }

    #[test]
    fn a_file_the_analyser_could_not_read_is_unparsed() {
        let assembled = figures(vec![
            (
                "src/keys.rs".to_string(),
                Some(space(
                    Some("src/keys.rs"),
                    Kind::Unit,
                    31,
                    vec![space(Some("route"), Kind::Function, 31, Vec::new())],
                )),
            ),
            ("src/ui.rs".to_string(), None),
        ]);
        assert_eq!(assembled.functions.len(), 1);
        assert_eq!(assembled.unparsed, 1);
        assert!(!assembled.nothing_analysed());
        assert!(figures(Vec::new()).nothing_analysed());
    }

    #[test]
    fn the_count_is_the_functions_above_the_threshold() {
        let (functions, _) = functions(
            "src/keys.rs",
            &space(
                Some("src/keys.rs"),
                Kind::Unit,
                65,
                vec![
                    space(Some("route"), Kind::Function, 31, Vec::new()),
                    space(Some("spell"), Kind::Function, 12, Vec::new()),
                    space(Some("draw"), Kind::Function, 22, Vec::new()),
                ],
            ),
        );
        assert_eq!(count(&functions, 10), 3);
        assert_eq!(count(&functions, 20), 2);
        assert_eq!(count(&functions, 30), 1);
        assert_eq!(count(&functions, 40), 0);
    }

    #[test]
    fn the_border_names_the_metric_and_the_count() {
        let mut state = State {
            risk_threshold: 20,
            ..State::default()
        };
        assert_eq!(border(&state), None, "no figure and no job: nothing to say");
        state.risk.figure = Figure::Current(Figures::default());
        assert_eq!(border(&state), None);
        state.risk.figure = Figure::Current(one_function());
        assert_eq!(border(&state).as_deref(), Some("CX 1"));
    }

    #[test]
    fn a_function_the_change_added_is_charged_and_one_it_deleted_is_credited() {
        let function = |name: &str, cyclomatic: u32| Function {
            file: "src/keys.rs".to_string(),
            name: name.to_string(),
            line: 1,
            metrics: Metrics {
                cyclomatic,
                ..Metrics::default()
            },
        };
        let figures = |functions: Vec<Function>| Figures {
            functions,
            unparsed: 0,
        };
        let state = State {
            view: View::Review,
            risk_threshold: 20,
            risk: Risk {
                figure: Figure::Current(figures(vec![function("route", 26), function("new", 7)])),
                before: Some(figures(vec![function("route", 31), function("gone", 10)])),
                ..Risk::default()
            },
            ..State::default()
        };
        assert_eq!(
            shown(&state),
            Some(Shown::Delta {
                delta: -8,
                worse: false
            })
        );
        assert_eq!(border(&state).as_deref(), Some("CX -8"));
        assert_eq!(row_delta(&state, &function("route", 26)), Some(-5));
        assert_eq!(
            row_delta(&state, &function("new", 7)),
            Some(7),
            "a Function the base revision did not have was credited for existing"
        );
        let names: Vec<&str> = list(&state)
            .iter()
            .map(|function| function.name.as_str())
            .collect();
        assert_eq!(names, ["route", "new"]);
        let worse = State {
            risk: Risk {
                figure: Figure::Current(figures(vec![function("route", 38)])),
                before: Some(figures(vec![function("route", 31)])),
                ..Risk::default()
            },
            ..state.clone()
        };
        assert_eq!(border(&worse).as_deref(), Some("CX +7 worse"));
    }

    #[test]
    fn the_border_turns_a_spinner_with_a_caption_while_a_job_runs() {
        let mut state = State {
            risk_threshold: 20,
            ..State::default()
        };
        let _ = analyse(&mut state, Scope::Workspace);
        assert_eq!(job(&state), Some(("workspace-analysis", "measuring Risk")));
        let frames: Vec<String> = (0..FRAMES.len() as u64 + 1)
            .map(|tick| {
                state.tick = tick;
                border(&state).expect("a border while the job runs")
            })
            .collect();
        assert!(
            frames.iter().all(|frame| frame.ends_with("measuring Risk")),
            "a spinner with no caption: {frames:?}"
        );
        assert_ne!(frames[0], frames[1], "the spinner stood still");
        assert_eq!(frames[0], frames[FRAMES.len()], "the frames do not cycle");
    }

    #[test]
    fn a_recompute_spins_beside_the_figure_it_is_replacing() {
        let mut state = State {
            risk_threshold: 20,
            ..State::default()
        };
        state.risk.figure = Figure::Current(one_function());
        went_stale(&mut state.risk);
        let _ = analyse(&mut state, Scope::Workspace);
        let drawn = border(&state).expect("a border while the job runs");
        assert!(drawn.starts_with("CX 1 stale"), "{drawn}");
        assert!(drawn.ends_with("measuring Risk"), "{drawn}");
        assert_eq!(view_state(&state), "stale");
    }

    #[test]
    fn the_count_replaces_the_spinner_when_the_figures_arrive() {
        let mut state = State {
            risk_threshold: 20,
            tick: 7,
            ..State::default()
        };
        let _ = analyse(&mut state, Scope::Workspace);
        assert!(arrived(&mut state.risk, 1, one_function(), None).is_none());
        assert_eq!(job(&state), None);
        assert_eq!(border(&state).as_deref(), Some("CX 1"));
    }

    #[test]
    fn a_stale_figure_is_drawn_as_stale() {
        let mut state = State {
            risk_threshold: 20,
            ..State::default()
        };
        state.risk.figure = Figure::Current(one_function());
        went_stale(&mut state.risk);
        assert_eq!(view_state(&state), "stale");
        let mut empty = State::default();
        empty.risk.figure = Figure::Current(Figures::default());
        went_stale(&mut empty.risk);
        assert_eq!(view_state(&empty), "nothing-analysed");
        assert_eq!(risk_count(&state), Some(1));
        assert_eq!(border(&state).as_deref(), Some("CX 1 stale"));
        went_stale(&mut state.risk);
        assert_eq!(risk_count(&state), Some(1));
    }

    #[test]
    fn a_superseded_analysis_answers_into_nothing() {
        let mut state = State::default();
        let first = analyse(&mut state, Scope::Workspace);
        let second = analyse(&mut state, Scope::Workspace);
        assert_ne!(first, second);
        let risk = &mut state.risk;
        assert!(risk.in_flight());
        assert_eq!(arrived(risk, 1, one_function(), Some("aaaa")), None);
        assert_eq!(risk.figure, Figure::None);
        assert!(risk.in_flight());
        assert!(arrived(risk, 2, one_function(), Some("aaaa")).is_some());
        assert!(!risk.in_flight());
        assert_eq!(
            arrived(risk, 2, Figures::default(), Some("aaaa")),
            None,
            "an answer already taken is not taken again"
        );
        assert_eq!(risk.figure, Figure::Current(one_function()));
    }

    #[test]
    fn a_workspace_with_no_commit_is_measured_and_never_cached() {
        let mut state = State::default();
        let _ = analyse(&mut state, Scope::Workspace);
        let risk = &mut state.risk;
        assert_eq!(arrived(risk, 1, one_function(), None), None);
        assert_eq!(risk.figure, Figure::Current(one_function()));
    }

    #[test]
    fn the_cache_is_read_back_only_for_the_commit_it_names() {
        let written = persist(&one_function(), "aaaaaaaaaaaa");
        assert_eq!(cached(&written, "aaaaaaaaaaaa"), Some(one_function()));
        assert_eq!(cached(&written, "bbbbbbbbbbbb"), None);
        assert_eq!(cached("not json", "aaaaaaaaaaaa"), None);
        assert_eq!(
            cached(&written.replace("\"CX\"", "\"CRAP\""), "aaaaaaaaaaaa"),
            None,
            "another metric's figure is not this one's"
        );
    }

    fn one_function() -> Figures {
        Figures {
            functions: vec![Function {
                file: "src/keys.rs".to_string(),
                name: "route".to_string(),
                line: 88,
                metrics: Metrics {
                    cyclomatic: 31,
                    cognitive: 24,
                    maintainability: 41,
                    lines: 96,
                },
            }],
            unparsed: 0,
        }
    }

    #[test]
    fn the_list_holds_exactly_the_functions_the_count_counts() {
        let mut state = State {
            risk_threshold: 20,
            ..State::default()
        };
        state.risk.figure = Figure::Current(three_functions());
        assert_eq!(list(&state).len(), risk_count(&state).expect("a figure"));
        assert_eq!(
            list(&state)
                .iter()
                .map(|function| function.name.as_str())
                .collect::<Vec<_>>(),
            ["route", "draw"],
            "worst first, and only above the threshold"
        );
        state.risk_all = true;
        assert_eq!(
            list(&state)
                .iter()
                .map(|function| function.name.as_str())
                .collect::<Vec<_>>(),
            ["route", "draw", "cheatsheet"]
        );
        assert_eq!(unparsed(&state), 3);
        state.risk.figure = Figure::None;
        assert!(list(&state).is_empty());
        assert_eq!(unparsed(&state), 0);
    }

    fn three_functions() -> Figures {
        let function = |file: &str, name: &str, cyclomatic| Function {
            file: file.to_string(),
            name: name.to_string(),
            line: 1,
            metrics: Metrics {
                cyclomatic,
                ..Metrics::default()
            },
        };
        Figures {
            functions: vec![
                function("src/ui.rs", "draw", 22),
                function("src/keys.rs", "cheatsheet", 4),
                function("src/keys.rs", "route", 31),
            ],
            unparsed: 3,
        }
    }

    #[test]
    fn a_function_exactly_on_the_threshold_is_not_counted() {
        let (functions, _) = functions(
            "src/keys.rs",
            &space(
                Some("src/keys.rs"),
                Kind::Unit,
                20,
                vec![space(Some("route"), Kind::Function, 20, Vec::new())],
            ),
        );
        assert_eq!(count(&functions, 20), 0);
        assert_eq!(count(&functions, 19), 1);
    }

    #[test]
    fn the_gate_takes_a_lower_count_or_a_lower_total_and_no_other_metric_rising() {
        let one = |cyclomatic, cognitive| Figures {
            functions: vec![Function {
                file: "src/keys.rs".to_string(),
                name: "route".to_string(),
                line: 88,
                metrics: Metrics {
                    cyclomatic,
                    cognitive,
                    ..Metrics::default()
                },
            }],
            unparsed: 0,
        };
        let before = one(31, 24);
        assert_eq!(gate(&before, &before, 20), Verdict::NoImprovement);
        assert_eq!(gate(&before, &one(14, 11), 20), Verdict::Passed);
        assert_eq!(gate(&before, &one(26, 20), 20), Verdict::Passed);
        assert_eq!(
            gate(&before, &one(14, 29), 20),
            Verdict::OtherMetricWorsened
        );
        let shredded = |lines| Figures {
            functions: (0..5)
                .map(|which| Function {
                    file: "src/keys.rs".to_string(),
                    name: format!("route_{which}"),
                    line: 88 + which * 8,
                    metrics: Metrics {
                        cyclomatic: 3,
                        cognitive: 5,
                        lines,
                        ..Metrics::default()
                    },
                })
                .collect(),
            unparsed: 0,
        };
        assert_eq!(
            gate(&before, &shredded(0), 20),
            Verdict::OtherMetricWorsened
        );
        let quiet = Figures {
            functions: shredded(20)
                .functions
                .into_iter()
                .map(|function| Function {
                    metrics: Metrics {
                        cognitive: 4,
                        ..function.metrics
                    },
                    ..function
                })
                .collect(),
            unparsed: 0,
        };
        assert_eq!(gate(&before, &quiet, 20), Verdict::OtherMetricWorsened);
    }

    #[test]
    fn the_maintainability_index_worsens_downward_not_upward() {
        let one = |cyclomatic, maintainability| Figures {
            functions: vec![Function {
                file: "src/keys.rs".to_string(),
                name: "route".to_string(),
                line: 88,
                metrics: Metrics {
                    cyclomatic,
                    cognitive: 24,
                    maintainability,
                    lines: 96,
                },
            }],
            unparsed: 0,
        };
        let before = one(31, 40);
        assert_eq!(gate(&before, &one(14, 60), 20), Verdict::Passed);
        assert_eq!(
            gate(&before, &one(14, 21), 20),
            Verdict::OtherMetricWorsened
        );
    }

    #[test]
    fn a_recompute_mid_iteration_is_not_the_iterations_measurement() {
        let mut state = State {
            root: std::path::PathBuf::from("/w"),
            risk_threshold: 20,
            test_command: Some("cargo test".to_string()),
            risk: Risk {
                figure: Figure::Current(three_functions()),
                ..Risk::default()
            },
            ..State::default()
        };
        start(&mut state, Scope::Workspace);
        pass_reported(&mut state);
        tests_finished(&mut state, true, "ok".to_string());
        analyse(&mut state, Scope::Workspace);
        let generation = state.risk.asked;
        let effects = figures_arrived(&mut state, generation, Figures::default(), None);
        assert!(
            !effects
                .iter()
                .any(|effect| matches!(effect, Effect::RestoreSnapshot { .. })),
            "the Gate closed on a measurement the Iteration never asked for: {effects:?}"
        );
        assert_eq!(state.refactor.stopped, None);
        assert_eq!(
            state.refactor.running.map(|iteration| iteration.wait),
            Some(Wait::Figures(generation - 1))
        );
    }

    #[test]
    fn a_gate_with_no_baseline_reverts_the_pass_and_says_why() {
        let mut state = State {
            root: std::path::PathBuf::from("/w"),
            view: crate::View::Review,
            risk_threshold: 20,
            max_iterations: 3,
            test_command: Some("cargo test".to_string()),
            ..State::default()
        };
        analyse(&mut state, Scope::Review);
        start(&mut state, Scope::Review);
        assert_eq!(state.refactor.refusal, None, "a measured Scope was refused");
        pass_reported(&mut state);
        tests_finished(&mut state, true, "ok".to_string());
        let generation = state.risk.asked;
        let effects = figures_arrived(&mut state, generation, Figures::default(), None);
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::RestoreSnapshot { iteration: 1 })),
            "the pass was kept without ever being judged: {effects:?}"
        );
        assert_eq!(state.refactor.stopped, Some(NO_BASELINE));
        assert_eq!(state.refactor.running, None);
    }

    #[test]
    fn a_review_scoped_prompt_names_the_reviewed_files_and_their_own_worst() {
        let reviewed = |path: &str, cyclomatic| Function {
            file: path.to_string(),
            name: format!("in_{}", path.replace(['/', '.'], "_")),
            line: 1,
            metrics: Metrics {
                cyclomatic,
                ..Metrics::default()
            },
        };
        let state = State {
            risk_threshold: 20,
            view: crate::View::Review,
            repo: Some(vec![
                crate::review::GitFile {
                    path: "src/keys.rs".to_string(),
                    status: crate::review::GitStatus::Modified,
                },
                crate::review::GitFile {
                    path: "src/tree.rs".to_string(),
                    status: crate::review::GitStatus::Committed,
                },
            ]),
            risk: Risk {
                figure: Figure::Current(Figures {
                    functions: vec![reviewed("src/keys.rs", 31)],
                    unparsed: 0,
                }),
                before: Some(Figures::default()),
                ..Risk::default()
            },
            ..State::default()
        };
        let prompt = loop_prompt(&state, Scope::Review);
        assert!(prompt.contains("src/keys.rs"), "{prompt}");
        assert!(prompt.contains("in_src_keys_rs"), "{prompt}");
        assert!(
            !prompt.contains("src/tree.rs"),
            "a file nobody is reviewing was handed to the session: {prompt}"
        );
    }

    #[test]
    fn every_refactor_ask_says_what_a_good_split_is() {
        let function = Function {
            file: "src/keys.rs".to_string(),
            name: "route".to_string(),
            line: 88,
            metrics: Metrics {
                cyclomatic: 31,
                cognitive: 24,
                ..Metrics::default()
            },
        };
        let state = State {
            risk_threshold: 20,
            view: crate::View::Review,
            repo: Some(vec![crate::review::GitFile {
                path: "src/keys.rs".to_string(),
                status: crate::review::GitStatus::Modified,
            }]),
            risk: Risk {
                figure: Figure::Current(Figures {
                    functions: vec![function.clone()],
                    unparsed: 0,
                }),
                before: Some(Figures::default()),
                ..Risk::default()
            },
            ..State::default()
        };
        for prompt in [
            loop_prompt(&state, Scope::Workspace),
            loop_prompt(&state, Scope::Review),
            refactor_prompt(&function),
        ] {
            assert!(prompt.contains("the symptom, not the goal"), "{prompt}");
            assert!(prompt.contains("structural scattering"), "{prompt}");
        }
    }

    #[test]
    fn an_answer_about_another_scope_is_not_the_iterations_baseline() {
        let mut state = State {
            root: std::path::PathBuf::from("/w"),
            view: crate::View::Review,
            risk_threshold: 20,
            max_iterations: 3,
            test_command: Some("cargo test".to_string()),
            ..State::default()
        };
        analyse(&mut state, Scope::Review);
        start(&mut state, Scope::Review);
        analyse(&mut state, Scope::Workspace);
        let generation = state.risk.asked;
        figures_arrived(&mut state, generation, three_functions(), None);
        assert_eq!(
            state
                .refactor
                .running
                .and_then(|iteration| iteration.before),
            None,
            "the review Iteration will be judged against the workspace"
        );
    }

    #[test]
    fn the_border_names_the_iteration_the_cap_and_what_is_being_waited_for() {
        let mut state = State {
            root: std::path::PathBuf::from("/w"),
            risk_threshold: 20,
            max_iterations: 3,
            test_command: Some("cargo test".to_string()),
            risk: Risk {
                figure: Figure::Current(three_functions()),
                ..Risk::default()
            },
            ..State::default()
        };
        assert_eq!(status(&state), None, "an idle pane says nothing");
        start(&mut state, Scope::Workspace);
        assert_eq!(status(&state).as_deref(), Some("1/3 session"));
        pass_reported(&mut state);
        assert_eq!(status(&state).as_deref(), Some("1/3 tests"));
        tests_finished(&mut state, true, "ok".to_string());
        assert_eq!(status(&state).as_deref(), Some("1/3 measuring ✓"));
        stop(&mut state);
        assert_eq!(status(&state).as_deref(), Some("stopped"));
        state.refactor.stopped = Some(TESTS_FAILED);
        assert_eq!(status(&state).as_deref(), Some("tests failed"));
    }

    fn file(name: &str) -> Entry {
        Entry {
            name: name.to_string(),
            is_dir: false,
        }
    }

    #[test]
    fn the_test_command_is_configuration_then_the_projects_shape() {
        let rust = [file("Cargo.toml"), file("Makefile")];
        assert_eq!(
            test_command(Some("cargo nextest run"), &rust).as_deref(),
            Some("cargo nextest run")
        );
        assert_eq!(test_command(None, &rust).as_deref(), Some("cargo test"));
        assert_eq!(
            test_command(None, &[file("Makefile")]).as_deref(),
            Some("make test")
        );
        assert_eq!(test_command(None, &[file("README.md")]), None);
        assert_eq!(
            test_command(
                Some("   "),
                &[Entry {
                    name: "Cargo.toml".to_string(),
                    is_dir: true,
                }]
            ),
            None
        );
    }
}
