use crate::{Direction, Modal, State};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Artifact {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    pub title: String,
    pub range: Range,
    pub stories: Vec<Story>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Range {
    pub base: String,
    pub head: String,
    pub spelling: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Story {
    pub id: String,
    pub name: String,
    pub premise: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Step {
    pub id: String,
    pub name: String,
    pub claim: String,
    pub why: String,
    pub site: Site,
    #[serde(default)]
    pub flow: Option<Flow>,
    #[serde(default)]
    pub values: Vec<Value>,
    #[serde(default)]
    pub nudge: Option<String>,
    #[serde(default)]
    pub prediction: Option<Prediction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Flow {
    #[serde(rename = "in")]
    pub flow_in: String,
    #[serde(rename = "out")]
    pub flow_out: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Value {
    pub name: String,
    pub value: String,
    pub provenance: Provenance,
    #[serde(default)]
    pub cite: Option<Cite>,
}

impl Value {
    pub fn displayed_provenance(&self) -> &'static str {
        if self.cite.is_none() {
            Provenance::Invented.as_str()
        } else {
            self.provenance.as_str()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    Literal,
    Fixture,
    Invented,
}

impl Provenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Provenance::Literal => "literal",
            Provenance::Fixture => "fixture",
            Provenance::Invented => "invented",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Cite {
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Site {
    pub file: String,
    pub side: Side,
    pub kind: Kind,
    pub from: u32,
    pub to: u32,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Old,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Changed,
    Context,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Changed => "changed",
            Kind::Context => "context",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Prediction {
    pub question: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Choice {
    pub text: String,
    pub correct: bool,
    pub feedback: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Set {
    None,
    Loaded(Artifact),
    Filling {
        artifact: Artifact,
        attempt: Option<u8>,
    },
    Refused {
        because: String,
    },
    RangeGone,
    NoDefaultBranch,
    BadRange,
    NotARepository,
    WorkingTreeDirty,
    GuestNeedsBareWorkspace,
    NoGit,
    Downloading {
        url: String,
        how: Download,
    },
    DownloadFailed {
        how: Download,
        status: Option<String>,
    },
    Authoring {
        spelling: String,
    },
    AuthoringAbandoned,
    Fixing {
        artifact: Artifact,
        attempt: u8,
        problems: Vec<Problem>,
    },
}

pub const ATTEMPTS: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Authored,
    ToAuthor { spelling: String, out: String },
    NoDefaultBranch,
    BadRange,
}

pub fn decide(force: bool, authored: bool, spelling: String, out: String) -> Resolution {
    match (force, authored) {
        (false, true) => Resolution::Authored,
        _ => Resolution::ToAuthor { spelling, out },
    }
}

const AUTHORING_PROMPT: &str = r#"Write a guided walkthrough of the changes in `{RANGE}` as JSON (protocolVersion 2), to the file `{OUT}`.

You are writing for a developer who did not make this change and will walk it step by step, one
screen at a time, to build a mental model of it. Follow how the code runs, not how the files are
arranged: start where control enters, and go where it goes — into unchanged code when the flow
goes there. A file-by-file summary is not a walkthrough.

`{CONTEXT}` already holds everything Varde knows about this change: the two oids the range resolved
to, its diff, and every changed file's contents with line numbers down the side. Read it once rather
than running `git diff` or `git log`, and rather than reopening a changed file to count lines. It
carries the changes only — a step whose `site.kind` is `context` points deliberately at code this
change did not touch, so open those files yourself; they are not in there.

Give the whole set a `title` naming what the range accomplishes at large — a feature added, a bug
fixed, an improvement made — so a reviewer with zero context knows what it is for before reading a
single story.

Split it into one or more stories, each with a name and a one-line premise. A story's `name` must
state its concrete subject — what changed — never stand alone as a metaphor or narrative flourish;
the `premise` is where the why goes. Aim for three to six steps per story — a reader gets lost past
five, so prefer more short stories over one long one. Give each step a short `name` too, next to its
claim: the name is what sits in a menu, the claim is the full sentence the narration reads aloud.
Keep a step's `name` to about 18 characters — the menu it sits in is a fixed width, and a longer
name is truncated with an ellipsis rather than shown in full.

## The shape

A complete set, with one story and one step filled in:

```json
{
  "protocolVersion": 2,
  "title": "The wheel scrolls the tree without waiting for a redraw",
  "range": {
    "base": "1a2b3c4d5e6f7890abcdef1234567890abcdef12",
    "head": "0fedcba9876543210fedcba9876543210fedcba9",
    "spelling": "{RANGE}"
  },
  "stories": [
    {
      "id": "wheel-scroll",
      "name": "A wheel event moves the tree",
      "premise": "Where a scroll wheel becomes a new first visible row, and why the clamp is skipped.",
      "steps": [
        {
          "id": "wheel-scroll.1",
          "name": "Wheel sets scroll",
          "claim": "A wheel event moves `tree_scroll` by three rows instead of moving the selection.",
          "why": "Scrolling and selecting are different gestures; moving the selection under the wheel would drag the editor along with it.",
          "site": {
            "file": "src/lib.rs",
            "side": "new",
            "kind": "changed",
            "from": 412,
            "to": 415
          },
          "flow": {
            "in": "an `Event::Wheel` carrying a direction and the pane the pointer is over",
            "out": "a new `State` whose `tree_scroll` has moved, and no effects"
          },
          "values": [
            {
              "name": "rows per wheel notch",
              "value": "3",
              "provenance": "literal",
              "cite": { "file": "src/lib.rs", "line": 44 }
            },
            {
              "name": "the tree's first visible row while scrolling",
              "value": "17",
              "provenance": "invented"
            }
          ],
          "nudge": "The arm returns early, so the clamp every other event falls through to is skipped here on purpose.",
          "prediction": {
            "question": "Why does this arm return before the clamp the other arms fall through to?",
            "choices": [
              {
                "text": "The clamp would pull the scroll back to the selection, undoing the wheel.",
                "correct": true,
                "feedback": "Right: the clamp keeps the selection visible, which is the opposite of what a wheel asked for."
              },
              {
                "text": "The clamp is slow, and a wheel arrives hundreds of times a second.",
                "correct": false,
                "feedback": "The clamp is arithmetic on two integers; a wheel batch costs its time in drawing, not here."
              },
              {
                "text": "The renderer clamps `tree_scroll` already, so a second clamp is redundant.",
                "correct": false,
                "feedback": "The renderer reads `tree_scroll` and never writes it — all clamping lives in `update`."
              }
            ]
          }
        }
      ]
    }
  ]
}
```

## Rules, field by field

**`title`** names what the whole range accomplishes — a feature added, a bug fixed, an improvement
made — never a list of the stories under it. **`name`** on a story and on a step states its concrete
subject; a step's is about 18 characters, because it sits in a fixed-width menu.

**`range`** carries the two full oids the range resolved to and the `spelling` exactly as given
above. Do not abbreviate an oid and do not resolve the range yourself against a different `HEAD`.

**`id`** is a short slug on a story and `"<story-id>.<n>"` on a step. **`premise`** is one line: what
the story is about, so a reader can choose it from a list.

**`claim`** is one sentence — what this code does. **`why`** is why it exists, or why it is done this
way and not the obvious other way.

**`site`** is where the step points.
- `file` is a path relative to the repository root.
- `side` is `"new"` for a line as it is after the change, `"old"` for a line the change removed.
- `kind` is `"changed"` when the change touched those lines, `"context"` when it did not — a step
  that walks into untouched code to follow the flow is expected and useful.
- **A deletion is `side: "old"` with `kind: "changed"`, always.** When the change only *removed*
  lines, point at them on the old side, where they can still be read, and give the line numbers they
  had before the change. Never file a removal as `side: "new"` with `kind: "context"`: that says
  "unchanged code", and removed code is not unchanged code — it is the part of the change nobody
  will see if it is filed under context.
- `from`/`to` are line numbers at the revision `side` names, not at any other revision.
- Do not write the lines themselves. A site is the file, the side, the kind and the range, and
  nothing else — Varde reads what those lines hold off its own copy of the file.
- A site may point at prose — a design document that argues the change is a legitimate step, and
  often the most valuable one.

**`flow`** is what arrives at the site and what leaves it. Omit `flow` entirely when the step is
about a decision or an argument rather than data passing through. Do not invent an in and an out to
fill the field.

**`values`** are concrete values that make the flow real — an argument, a constant, a config value.
Omit `values` when the claim does not depend on one; most steps have none, and an unused value is
noise on a fixed-width strip. Each one's `provenance` is:
- `"literal"` — copied from a line of code. **Requires `cite`.**
- `"fixture"` — copied from a test or fixture in this repository. **Requires `cite`.**
- `"invented"` — an illustrative value you made up. **No `cite`.** Use this honestly: an assembled
  or representative value is invented even if its parts are real.

A `cite` names a `file` and a `line` that really contains that value. The reader can press one key to
jump there, and a citation that lands on the wrong code is worse than no value at all. If you are not
certain, mark it `"invented"` and drop the `cite`.

**`nudge`** is one more sentence for a reader who wants more detail than the claim. Omit `nudge` when
there is nothing further to say — a nudge restating the claim wastes the one line it gets.

**`prediction`** is optional and rare — at most one or two per story, at a step where the reasoning is
genuinely worth stopping for. Omit it everywhere else; a walkthrough is not a quiz.

When present it must offer **exactly three choices** — a set with a prediction of any other size is
refused whole, and nothing in it is walkable. Exactly one choice is `correct`. Ask **why**, never
"which line runs next" — guessing the next line measures nothing. Wrong choices must be genuinely
plausible reasons a competent developer might give, and their `feedback` must explain why they are
wrong; that explanation is the most valuable text in the file. Never invent a function, file or
behaviour that does not exist in order to make a wrong choice: draw wrong choices from real
alternatives.

**Coverage.** Every changed hunk should be claimed by some step. Do not add a list of what you left
out and do not guess at line numbers you have not read — what no story reaches is worked out from git
and reported separately. Leaving something out is allowed; misreporting it is not.

## Before you write the file

Check your own work with a script rather than by eye: open every `cite` and confirm the line contains
that value, confirm every prediction has exactly one `correct: true` and names nothing that does not
exist, confirm the JSON parses, and confirm the steps follow execution order rather than file order.

Write only the JSON file, at exactly the path above. Do not summarise it back; the file is the
whole output."#;

pub fn prompt(spelling: &str, out: &str, context: &str) -> String {
    AUTHORING_PROMPT
        .replace("{RANGE}", spelling)
        .replace("{OUT}", out)
        .replace("{CONTEXT}", context)
}

pub fn context_path(artifact: &str) -> String {
    let stem = artifact.strip_suffix(".json").unwrap_or(artifact);
    format!("{stem}.context.md")
}

pub const CONTEXT_CAP: usize = 256 * 1024;

pub const CONTEXT_UNAVAILABLE: &str =
    "# The change\n\nVarde could not read this range out of git, so nothing is handed over here.\n\
     Work the change out yourself, from the range the prompt names.\n";

const TRUNCATION: &str =
    "\n--- Truncated here. Read anything further you need from the files directly.\n";

pub fn context_file(
    base: &str,
    head: &str,
    spelling: &str,
    files: &[FileHunks],
    cap: usize,
) -> String {
    let changed = || files.iter().filter(|file| !file.hunks.is_empty());
    let mut out = format!(
        "# The change in `{spelling}`\n\n\
         - base `{base}`\n\
         - head `{head}`\n\n\
         This is the changes only: the diff of the range, then each changed file's lines with their\n\
         numbers. Code the range did not touch is not here — open those files directly.\n\n\
         ## The diff\n\n```diff\n"
    );
    for file in changed() {
        out.push_str(&diff_text(&file.old_text, &file.new_text, &file.file));
    }
    out.push_str("```\n");
    for file in changed() {
        let (side, text) = if file.new_exists {
            ("new", &file.new_text)
        } else {
            ("old", &file.old_text)
        };
        out.push_str(&format!("\n## `{}`, {} side\n\n```\n", file.file, side));
        for (index, line) in text.lines().enumerate() {
            out.push_str(&format!("{:>5} | {}\n", index + 1, line));
        }
        out.push_str("```\n");
    }
    truncated(out, cap)
}

fn diff_text(old: &str, new: &str, file: &str) -> String {
    let mut options = git2::DiffOptions::new();
    options.context_lines(CONTEXT_LINES);
    let named = Path::new(file);
    let Ok(mut patch) = git2::Patch::from_buffers(
        old.as_bytes(),
        Some(named),
        new.as_bytes(),
        Some(named),
        Some(&mut options),
    ) else {
        return String::new();
    };
    patch
        .to_buf()
        .ok()
        .and_then(|buffer| buffer.as_str().ok().map(str::to_string))
        .unwrap_or_default()
}

fn truncated(text: String, cap: usize) -> String {
    if text.len() <= cap {
        return text;
    }
    let cut = text
        .bytes()
        .take(cap)
        .rposition(|byte| byte == b'\n')
        .map_or_else(
            || {
                (0..=cap)
                    .rev()
                    .find(|at| text.is_char_boundary(*at))
                    .unwrap_or(0)
            },
            |at| at + 1,
        );
    format!("{}{TRUNCATION}", &text[..cut])
}

pub const RETENTION: usize = 10;

pub fn prune(existing: &[String], limit: usize) -> Vec<String> {
    let excess = existing.len().saturating_sub(limit);
    existing[excess..].to_vec()
}

pub fn default_branch(
    origin_head: Option<&str>,
    upstream: Option<&str>,
    configured: Option<&str>,
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    origin_head
        .or(upstream)
        .or(configured)
        .map(str::to_string)
        .or_else(|| exists("main").then(|| "main".to_string()))
        .or_else(|| exists("master").then(|| "master".to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRef {
    pub name: String,
    pub remote: bool,
    pub when: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Branching {
    Listed(Vec<BranchRef>),
    NotARepository,
    Dirty,
    DownloadFailed {
        how: Download,
        status: Option<String>,
    },
}

pub fn uncommitted(files: &[crate::review::GitFile]) -> bool {
    files
        .iter()
        .any(|file| !Path::new(&file.path).starts_with(crate::VARDE_DIR))
}

pub fn branches(refs: &[BranchRef], needle: &str) -> Vec<String> {
    let mut rows: Vec<&BranchRef> = refs.iter().filter(|row| short(row) != "HEAD").collect();
    rows.sort_by(|a, b| {
        b.when
            .cmp(&a.when)
            .then(a.remote.cmp(&b.remote))
            .then_with(|| short(a).cmp(short(b)))
    });
    let mut seen = BTreeSet::new();
    let names: Vec<String> = rows
        .into_iter()
        .filter(|row| seen.insert(short(row)))
        .map(|row| short(row).to_string())
        .collect();
    if needle.is_empty() {
        return names;
    }
    let typed = needle.to_lowercase();
    let literal: Vec<String> = names
        .iter()
        .filter(|name| name.to_lowercase().contains(&typed))
        .cloned()
        .collect();
    match literal.is_empty() {
        false => literal,
        true => names
            .into_iter()
            .filter(|name| crate::filter::score(needle, name).is_some())
            .collect(),
    }
}

pub const DOWNLOAD_SENTINEL: &str = "download-done";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Download {
    Clone,
    Fetch,
}

pub fn guest_name(url: &str) -> String {
    let last = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default();
    let name = last.strip_suffix(".git").unwrap_or(last);
    match name.starts_with('.') || name.is_empty() {
        true => "guest".to_string(),
        false => name.to_string(),
    }
}

/// Write then rename: a redirect creates the sentinel empty, and the watcher fires before the status lands
pub fn download_command(how: Download, url: &str, into: &Path, sentinel: &Path) -> String {
    let quoted = |text: &str| {
        shlex::try_quote(text)
            .expect("no NUL in a URL or a path")
            .into_owned()
    };
    let into = quoted(&into.to_string_lossy());
    let writing = quoted(&sentinel.with_extension("writing").to_string_lossy());
    let sentinel = quoted(&sentinel.to_string_lossy());
    // Fetch origin, not the URL: `git fetch <url>` updates only FETCH_HEAD, no remote-tracking refs
    let git = match how {
        Download::Clone => format!("git clone {} {into}", quoted(url)),
        Download::Fetch => format!("git -C {into} fetch"),
    };
    format!("rm -f {sentinel}; {git}; echo $? > {writing}; mv {writing} {sentinel}")
}

fn short(row: &BranchRef) -> &str {
    match row.remote {
        true => row
            .name
            .split_once('/')
            .map_or(&*row.name, |(_, rest)| rest),
        false => &row.name,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    Spine,
    Files,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeStatus {
    Resolves,
    Gone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineRow {
    pub name: String,
    pub steps: usize,
    pub stale: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Staleness {
    Fresh,
    Gone,
    TooShort,
    Changed { now: String },
}

impl Staleness {
    pub fn is_stale(&self) -> bool {
        !matches!(self, Staleness::Fresh)
    }

    pub fn kind(&self) -> Option<&'static str> {
        match self {
            Staleness::Fresh => None,
            Staleness::Gone => Some("file-missing"),
            Staleness::TooShort => Some("range-out-of-bounds"),
            Staleness::Changed { .. } => Some("text-changed"),
        }
    }
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn lines_in(text: &str, from: u32, to: u32) -> Option<String> {
    if from == 0 || from > to {
        return None;
    }
    let lines: Vec<&str> = text
        .strip_suffix('\n')
        .unwrap_or(text)
        .split('\n')
        .collect();
    if to as usize > lines.len() {
        return None;
    }
    Some(lines[(from as usize - 1)..(to as usize)].join("\n"))
}

fn staleness_against(site: &Site, exists: bool, current_text: &str) -> Staleness {
    if !exists {
        return Staleness::Gone;
    }
    match lines_in(current_text, site.from, site.to) {
        None => Staleness::TooShort,
        Some(now) if normalize_whitespace(&now) == normalize_whitespace(&site.text) => {
            Staleness::Fresh
        }
        Some(now) => Staleness::Changed { now },
    }
}

pub fn staleness(state: &State, step: &Step) -> Staleness {
    let hunks = state
        .file_hunks
        .iter()
        .find(|file| file.file == step.site.file);
    match step.site.side {
        Side::Old => match hunks {
            None => Staleness::Fresh,
            Some(file) => staleness_against(&step.site, file.old_exists, &file.old_text),
        },
        Side::New => {
            let path = state.root.join(&step.site.file);
            if let Some(buffer) = state.buffers.get(&path).filter(|buffer| buffer.is_dirty()) {
                return staleness_against(&step.site, true, buffer.shown());
            }
            match hunks {
                None => Staleness::Fresh,
                Some(file) => staleness_against(&step.site, file.new_exists, &file.new_text),
            }
        }
    }
}

pub fn parse(text: &str) -> Result<Artifact, String> {
    let artifact: Artifact = serde_json::from_str(text).map_err(|error| error.to_string())?;
    for story in &artifact.stories {
        for step in &story.steps {
            if let Some(prediction) = &step.prediction {
                if prediction.choices.len() != 3 {
                    return Err(format!(
                        "a prediction must offer exactly three choices, not {}",
                        prediction.choices.len()
                    ));
                }
            }
        }
    }
    Ok(artifact)
}

pub fn story_files(artifact: &Artifact) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for step in artifact.stories.iter().flat_map(|story| &story.steps) {
        let cited = step
            .values
            .iter()
            .filter_map(|value| value.cite.as_ref())
            .map(|cite| &cite.file);
        for file in std::iter::once(&step.site.file).chain(cited) {
            if !files.contains(file) {
                files.push(file.clone());
            }
        }
    }
    files
}

fn side_text<'a>(files: &'a [FileHunks], file: &str, side: Side) -> Option<&'a str> {
    let entry = files.iter().find(|entry| entry.file == file)?;
    let (exists, text) = match side {
        Side::Old => (entry.old_exists, &entry.old_text),
        Side::New => (entry.new_exists, &entry.new_text),
    };
    exists.then_some(text.as_str())
}

fn range_text<'a>(files: &'a [FileHunks], file: &str, side: Side) -> Option<&'a str> {
    let entry = files.iter().find(|entry| entry.file == file)?;
    let (exists, text) = match side {
        Side::Old => (entry.old_exists, &entry.old_text),
        Side::New => (entry.head_exists, &entry.head_text),
    };
    exists.then_some(text.as_str())
}

pub fn fill(artifact: &mut Artifact, files: &[FileHunks]) {
    for step in artifact
        .stories
        .iter_mut()
        .flat_map(|story| story.steps.iter_mut())
    {
        if !step.site.text.is_empty() {
            continue;
        }
        step.site.text = side_text(files, &step.site.file, step.site.side)
            .and_then(|text| lines_in(text, step.site.from, step.site.to))
            .unwrap_or_default();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    FileMissing,
    RangeOutOfBounds,
    ClaimsNoChange,
    CitationAbsent,
}

impl Fault {
    fn as_str(self) -> &'static str {
        match self {
            Fault::FileMissing => "file-missing",
            Fault::RangeOutOfBounds => "range-out-of-bounds",
            Fault::ClaimsNoChange => "claims-no-change",
            Fault::CitationAbsent => "citation-absent",
        }
    }

    fn complaint(self) -> &'static str {
        match self {
            Fault::FileMissing => {
                "there is no file at the path this step's `site.file` names. Paths are relative to \
                 the repository root."
            }
            Fault::RangeOutOfBounds => {
                "this step's `site.from`/`site.to` run past the end of that file at the side it \
                 names. Line numbers are 1-based, at the revision `side` names."
            }
            Fault::ClaimsNoChange => {
                "this step says `kind: \"changed\"` but those lines are not in this range's diff. \
                 Point at lines the change touched, or say `kind: \"context\"` if the step \
                 deliberately walks into untouched code. A removal is `side: \"old\"` with \
                 `kind: \"changed\"`."
            }
            Fault::CitationAbsent => {
                "a value on this step cites a line that does not contain it. Fix the line number, \
                 or mark the value `\"invented\"` and drop its `cite`."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub step: String,
    pub fault: Fault,
}

pub fn problems(artifact: &Artifact, files: &[FileHunks]) -> Vec<Problem> {
    let mut found = Vec::new();
    for step in artifact.stories.iter().flat_map(|story| &story.steps) {
        let site = &step.site;
        let mut fault = |fault| {
            found.push(Problem {
                step: step.id.clone(),
                fault,
            })
        };
        let placed = match range_text(files, &site.file, site.side) {
            None => {
                fault(Fault::FileMissing);
                false
            }
            Some(text) if lines_in(text, site.from, site.to).is_none() => {
                fault(Fault::RangeOutOfBounds);
                false
            }
            Some(_) => true,
        };
        if placed
            && matches!(site.kind, Kind::Changed)
            && !files.iter().any(|entry| {
                entry
                    .hunks
                    .iter()
                    .any(|hunk| claims(site, &entry.file, hunk))
            })
        {
            fault(Fault::ClaimsNoChange);
        }
        for value in &step.values {
            let Some(cite) = &value.cite else {
                continue;
            };
            let present = range_text(files, &cite.file, Side::New)
                .and_then(|cited| lines_in(cited, cite.line, cite.line))
                .is_some_and(|line| line.contains(&value.value));
            if !present {
                fault(Fault::CitationAbsent);
            }
        }
    }
    found
}

pub fn refusal(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(|problem| format!("{}: {}", problem.step, problem.fault.as_str()))
        .collect::<Vec<_>>()
        .join(", ")
}

const FIX_PROMPT: &str = r#"Varde checked the story set you wrote to `{OUT}` and cannot accept it yet. These steps do not hold up:

{PROBLEMS}

Rewrite only those steps. Keep the set's `title`, its `range`, its stories and every other step
exactly as they are, and write the whole set back to `{OUT}` — the same path, replacing what is
there. Do not re-author the walkthrough and do not renumber or drop the steps that were fine.

`{CONTEXT}` still holds the range's two oids, its diff and every changed file's contents with line
numbers; read the line numbers off it rather than guessing again.

Write only the JSON file. Do not summarise it back."#;

pub fn fix_prompt(dir: &Path, artifact: &Artifact, problems: &[Problem]) -> String {
    let out = artifact_path_of(dir, artifact);
    let listed = problems
        .iter()
        .map(|problem| {
            format!(
                "- `{}`: {} — {}",
                problem.step,
                problem.fault.as_str(),
                problem.fault.complaint()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    FIX_PROMPT
        .replace("{PROBLEMS}", &listed)
        .replace("{CONTEXT}", &context_path(&out))
        .replace("{OUT}", &out)
}

fn artifact_path_of(dir: &Path, artifact: &Artifact) -> String {
    let short = |oid: &str| oid.chars().take(12).collect::<String>();
    let head = if artifact.range.head == WORKTREE {
        WORKTREE.to_string()
    } else {
        short(&artifact.range.head)
    };
    artifact_path(dir, &short(&artifact.range.base), &head)
}

pub fn same_set(loaded: &Artifact, arrived: &Artifact) -> bool {
    let without_text = |artifact: &Artifact| {
        let mut copy = artifact.clone();
        for story in &mut copy.stories {
            for step in &mut story.steps {
                step.site.text = String::new();
            }
        }
        copy
    };
    without_text(loaded) == without_text(arrived)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walking {
    Story {
        story: usize,
        step: usize,
        diff: Diff,
    },
    Remainder {
        index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diff {
    Hidden,
    Shown,
}

pub const BAND_HEIGHT: u16 = 6;

pub fn band_height(state: &State) -> u16 {
    if state.walking.is_some() {
        BAND_HEIGHT
    } else {
        0
    }
}

pub fn step_menu_width(state: &State) -> u16 {
    if matches!(state.walking, Some(Walking::Story { .. })) {
        crate::layout::STEP_MENU_WIDTH
    } else {
        0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepMenuRow {
    pub name: String,
    pub current: bool,
}

pub fn step_menu(state: &State) -> Vec<StepMenuRow> {
    let Some(Walking::Story { story, step, .. }) = state.walking else {
        return Vec::new();
    };
    let Set::Loaded(artifact) = &state.story_set else {
        return Vec::new();
    };
    let Some(current) = artifact.stories.get(story) else {
        return Vec::new();
    };
    current
        .steps
        .iter()
        .enumerate()
        .map(|(index, s)| StepMenuRow {
            name: s.name.clone(),
            current: index == step,
        })
        .collect()
}

pub fn advance(total_steps: usize, current: usize, direction: Direction) -> usize {
    match direction {
        Direction::Left | Direction::Up => current.saturating_sub(1),
        _ => (current + 1).min(total_steps.saturating_sub(1)),
    }
}

pub fn detail_sections(state: &State, step: &Step) -> Vec<&'static str> {
    let mut sections = vec!["claim", "why"];
    if staleness(state, step).is_stale() {
        sections.push("stale");
    }
    if step.flow.is_some() {
        sections.push("flow");
    }
    if step.nudge.is_some() {
        sections.push("nudge");
    }
    sections
}

pub fn current_step(state: &State) -> Option<&Step> {
    let Walking::Story { story, step, .. } = state.walking? else {
        return None;
    };
    match &state.story_set {
        Set::Loaded(artifact) => artifact.stories.get(story)?.steps.get(step),
        _ => None,
    }
}

pub fn prediction_choices(state: &State) -> Vec<&str> {
    let Modal::Prediction { picked } = &state.modal else {
        return Vec::new();
    };
    let Some(prediction) = current_step(state).and_then(|step| step.prediction.as_ref()) else {
        return Vec::new();
    };
    let answered_correctly = picked
        .and_then(|index| prediction.choices.get(index))
        .is_some_and(|choice| choice.correct);
    if answered_correctly {
        Vec::new()
    } else {
        prediction
            .choices
            .iter()
            .map(|choice| choice.text.as_str())
            .collect()
    }
}

pub fn prediction_feedback(state: &State) -> Option<&str> {
    let Modal::Prediction { picked } = &state.modal else {
        return None;
    };
    let prediction = current_step(state)?.prediction.as_ref()?;
    prediction
        .choices
        .get((*picked)?)
        .map(|choice| choice.feedback.as_str())
}

fn loaded(state: &State) -> Option<&Artifact> {
    match &state.story_set {
        Set::Loaded(artifact) => Some(artifact),
        Set::None
        | Set::Filling { .. }
        | Set::Refused { .. }
        | Set::RangeGone
        | Set::NoDefaultBranch
        | Set::BadRange
        | Set::NotARepository
        | Set::WorkingTreeDirty
        | Set::GuestNeedsBareWorkspace
        | Set::NoGit
        | Set::Downloading { .. }
        | Set::DownloadFailed { .. }
        | Set::Authoring { .. }
        | Set::AuthoringAbandoned
        | Set::Fixing { .. } => None,
    }
}

pub fn spine(state: &State) -> Vec<SpineRow> {
    let Some(artifact) = loaded(state) else {
        return Vec::new();
    };
    artifact
        .stories
        .iter()
        .map(|story| SpineRow {
            name: story.name.clone(),
            steps: story.steps.len(),
            stale: story
                .steps
                .iter()
                .filter(|step| staleness(state, step).is_stale())
                .count(),
        })
        .collect()
}

pub fn title(state: &State) -> Option<&str> {
    loaded(state).map(|artifact| artifact.title.as_str())
}

pub fn spine_row_count(state: &State) -> usize {
    spine(state).len() + usize::from(!remainder_locations(state).is_empty())
}

pub fn title_rows(state: &State) -> usize {
    if state.view == crate::View::Story
        && state.story_listing == Listing::Spine
        && title(state).is_some()
    {
        1
    } else {
        0
    }
}

pub fn view_state(state: &State) -> &'static str {
    match &state.story_set {
        Set::None => "no-stories",
        Set::Loaded(_) => "spine",
        Set::Filling { .. } => "filling",
        Set::Refused { .. } => "story-artifact-invalid",
        Set::RangeGone => "range-unresolvable",
        Set::NoDefaultBranch => "no-default-branch",
        Set::BadRange => "bad-range",
        Set::NotARepository => "not-a-git-repository",
        Set::WorkingTreeDirty => "working-tree-dirty",
        Set::GuestNeedsBareWorkspace => "guest-needs-bare-workspace",
        Set::NoGit => "git-not-installed",
        Set::Downloading {
            how: Download::Clone,
            ..
        } => "cloning",
        Set::Downloading {
            how: Download::Fetch,
            ..
        } => "fetching",
        Set::DownloadFailed {
            how: Download::Clone,
            ..
        } => "clone-failed",
        Set::DownloadFailed {
            how: Download::Fetch,
            ..
        } => "fetch-failed",
        Set::Authoring { .. } => "authoring",
        Set::AuthoringAbandoned => "authoring-abandoned",
        Set::Fixing { .. } => "fixing",
    }
}

pub fn range_status(file: &Path, resolves: impl Fn(&str) -> bool) -> RangeStatus {
    let stem = file
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    match stem.split_once('-') {
        Some((base, head)) if resolves(base) && (head == WORKTREE || resolves(head)) => {
            RangeStatus::Resolves
        }
        _ => RangeStatus::Gone,
    }
}

pub fn artifact_path(dir: &Path, base: &str, head: &str) -> String {
    format!("{}/stories/{base}-{head}.json", dir.display())
}

pub fn range(base: &str, head: &str) -> String {
    format!("{base}...{head}")
}

pub fn revisions(spelling: &str) -> Option<(&str, &str, bool)> {
    match spelling.split_once("...") {
        Some((base, head)) => Some((base, head, true)),
        None => spelling
            .split_once("..")
            .map(|(base, head)| (base, head, false)),
    }
}

pub const WORKTREE: &str = "worktree";

pub const CONTEXT_LINES: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inventory<'a> {
    Worktree,
    Committed { base: &'a str, head: &'a str },
}

pub fn inventory(set: &Set) -> Inventory<'_> {
    match set {
        Set::Loaded(artifact) | Set::Filling { artifact, .. } => inventory_of(artifact),
        _ => Inventory::Worktree,
    }
}

pub fn named_files(set: &Set) -> Vec<String> {
    match set {
        Set::Loaded(artifact) | Set::Filling { artifact, .. } => story_files(artifact),
        Set::None
        | Set::Refused { .. }
        | Set::RangeGone
        | Set::NoDefaultBranch
        | Set::BadRange
        | Set::NotARepository
        | Set::WorkingTreeDirty
        | Set::GuestNeedsBareWorkspace
        | Set::NoGit
        | Set::Downloading { .. }
        | Set::DownloadFailed { .. }
        | Set::Authoring { .. }
        | Set::AuthoringAbandoned
        | Set::Fixing { .. } => Vec::new(),
    }
}

pub fn inventory_of(artifact: &Artifact) -> Inventory<'_> {
    if artifact.range.head == WORKTREE {
        return Inventory::Worktree;
    }
    Inventory::Committed {
        base: &artifact.range.base,
        head: &artifact.range.head,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
}

impl Hunk {
    fn is_deletion(&self) -> bool {
        self.new_lines == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHunks {
    pub file: String,
    pub hunks: Vec<Hunk>,
    pub old_exists: bool,
    pub old_text: String,
    pub new_exists: bool,
    pub new_text: String,
    pub head_exists: bool,
    pub head_text: String,
}

pub fn hunks(old: &[u8], new: &[u8]) -> Vec<Hunk> {
    let mut options = git2::DiffOptions::new();
    options.context_lines(CONTEXT_LINES);
    let Ok(patch) = git2::Patch::from_buffers(old, None, new, None, Some(&mut options)) else {
        return Vec::new();
    };
    (0..patch.num_hunks())
        .filter_map(|index| patch.hunk(index).ok())
        .map(|(hunk, _lines)| Hunk {
            old_start: hunk.old_start(),
            old_lines: hunk.old_lines(),
            new_start: hunk.new_start(),
            new_lines: hunk.new_lines(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Remainder {
    pub unclaimed: usize,
    pub locations: Vec<String>,
    pub unwalked_deletions: Option<usize>,
}

fn overlaps(from: u32, to: u32, start: u32, lines: u32) -> bool {
    lines > 0 && from < start + lines && start <= to
}

fn claim_sites(state: &State) -> Vec<&Site> {
    match &state.story_set {
        Set::Loaded(artifact) => artifact
            .stories
            .iter()
            .flat_map(|story| story.steps.iter().map(|step| &step.site))
            .collect(),
        Set::None
        | Set::Filling { .. }
        | Set::Refused { .. }
        | Set::RangeGone
        | Set::NoDefaultBranch
        | Set::BadRange
        | Set::NotARepository
        | Set::WorkingTreeDirty
        | Set::GuestNeedsBareWorkspace
        | Set::NoGit
        | Set::Downloading { .. }
        | Set::DownloadFailed { .. }
        | Set::Authoring { .. }
        | Set::AuthoringAbandoned
        | Set::Fixing { .. } => Vec::new(),
    }
}

fn claims(site: &Site, file: &str, hunk: &Hunk) -> bool {
    site.file == file
        && matches!(site.kind, Kind::Changed)
        && match site.side {
            Side::Old => overlaps(site.from, site.to, hunk.old_start, hunk.old_lines),
            Side::New => overlaps(site.from, site.to, hunk.new_start, hunk.new_lines),
        }
}

fn is_claimed(sites: &[&Site], file: &str, hunk: &Hunk) -> bool {
    sites.iter().any(|site| claims(site, file, hunk))
}

pub fn remainder(state: &State) -> Remainder {
    let sites = claim_sites(state);
    let mut result = Remainder::default();
    let mut deletions_total = 0usize;
    let mut deletions_unwalked = 0usize;

    for file in state.file_hunks.iter() {
        for hunk in &file.hunks {
            let claimed = is_claimed(&sites, &file.file, hunk);
            if hunk.is_deletion() {
                deletions_total += 1;
                if !claimed {
                    deletions_unwalked += 1;
                }
            } else if !claimed {
                result.unclaimed += 1;
                result.locations.push(file.file.clone());
            }
        }
    }

    result.unwalked_deletions = (deletions_total > 0).then_some(deletions_unwalked);
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemainderLocation {
    pub file: String,
    pub from: u32,
    pub to: u32,
}

pub fn remainder_locations(state: &State) -> Vec<RemainderLocation> {
    let sites = claim_sites(state);
    let mut locations = Vec::new();
    for file in state.file_hunks.iter() {
        for hunk in &file.hunks {
            if !hunk.is_deletion() && !is_claimed(&sites, &file.file, hunk) {
                // `new_lines - 1` relies on the is_deletion guard above; in release it would wrap, not panic
                locations.push(RemainderLocation {
                    file: file.file.clone(),
                    from: hunk.new_start,
                    to: hunk.new_start + hunk.new_lines - 1,
                });
            }
        }
    }
    locations
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteMark {
    Nothing,
    Site {
        file: String,
        from: u32,
        to: u32,
        kind: Kind,
    },
    Refused,
}

impl SiteMark {
    pub fn covers(&self, file: &str, line: u32) -> bool {
        match self {
            SiteMark::Site {
                file: marked,
                from,
                to,
                ..
            } => marked == file && (*from..=*to).contains(&line),
            SiteMark::Nothing | SiteMark::Refused => false,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SiteMark::Nothing => "not-walking",
            SiteMark::Site { .. } => "shown",
            SiteMark::Refused => "old-side-not-shown",
        }
    }
}

pub fn mark(state: &State) -> SiteMark {
    match state.walking {
        None => SiteMark::Nothing,
        Some(Walking::Story { .. }) => match current_step(state) {
            None => SiteMark::Nothing,
            Some(step) => match step.site.side {
                Side::Old => SiteMark::Refused,
                Side::New => SiteMark::Site {
                    file: step.site.file.clone(),
                    from: step.site.from,
                    to: step.site.to,
                    kind: step.site.kind,
                },
            },
        },
        Some(Walking::Remainder { index }) => match remainder_locations(state).get(index) {
            None => SiteMark::Nothing,
            Some(location) => SiteMark::Site {
                file: location.file.clone(),
                from: location.from,
                to: location.to,
                kind: Kind::Changed,
            },
        },
    }
}

pub fn refused(state: &State) -> bool {
    matches!(mark(state), SiteMark::Refused) && site_diff(state).is_none()
}

pub fn shown_file(state: &State) -> String {
    state
        .current_buffer
        .as_ref()
        .map(|path| {
            path.strip_prefix(state.repo_root())
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_default()
}

#[derive(Debug, PartialEq)]
pub enum Row<'a> {
    Code(u32),
    Comment(&'a crate::review::Comment),
    Removed(String),
}

pub fn rows(state: &State, lines: usize) -> impl Iterator<Item = Row<'_>> {
    let hidden = crate::fold::hidden(state);
    let shown = (1..=lines as u32).filter(move |number| !hidden.contains(&(*number as usize)));
    let file = state.walking.as_ref().map(|_| shown_file(state));
    let removed = site_diff(state).map_or_else(Vec::new, |diff| diff.removed);
    let removed_under = move |line: u32| -> Vec<Row<'_>> {
        removed
            .iter()
            .filter(|(under, _)| *under == line)
            .map(|(_, text)| Row::Removed(text.clone()))
            .collect()
    };
    removed_under(0)
        .into_iter()
        .chain(shown.flat_map(move |number| {
            let comments = file
                .as_deref()
                .map(|file| crate::review::comments_at(state, file, number))
                .unwrap_or_default();
            std::iter::once(Row::Code(number))
                .chain(comments.into_iter().map(Row::Comment))
                .chain(removed_under(number))
        }))
}

#[derive(Debug, PartialEq, Eq)]
pub struct SiteDiff {
    pub added: Vec<u32>,
    pub removed: Vec<(u32, String)>,
}

pub fn site_diff(state: &State) -> Option<SiteDiff> {
    let Some(Walking::Story {
        diff: Diff::Shown, ..
    }) = state.walking
    else {
        return None;
    };
    let site = &current_step(state)?.site;
    if site.kind == Kind::Context || shown_file(state) != site.file {
        return None;
    }
    let file = state
        .file_hunks
        .iter()
        .find(|file| file.file == site.file)?;
    Some(changes(
        &file.old_text,
        &file.head_text,
        site.side,
        site.from,
        site.to,
    ))
}

#[derive(Default)]
struct Block {
    removed: Vec<(u32, u32, String)>,
    added: Vec<u32>,
}

fn changes(old: &str, new: &str, side: Side, from: u32, to: u32) -> SiteDiff {
    let mut diff = SiteDiff {
        added: Vec::new(),
        removed: Vec::new(),
    };
    let Ok(patch) = git2::Patch::from_buffers(old.as_bytes(), None, new.as_bytes(), None, None)
    else {
        return diff;
    };
    let mut keep = |block: Block| {
        let reached = match side {
            Side::New if block.added.is_empty() => block
                .removed
                .first()
                .is_some_and(|(_, under, _)| (from..to).contains(under)),
            Side::New => block.added.iter().any(|line| (from..=to).contains(line)),
            Side::Old => block
                .removed
                .iter()
                .any(|(line, _, _)| (from..=to).contains(line)),
        };
        if !reached {
            return;
        }
        diff.added.extend(
            block
                .added
                .into_iter()
                .filter(|line| side == Side::Old || (from..=to).contains(line)),
        );
        diff.removed.extend(
            block
                .removed
                .into_iter()
                .map(|(_, under, text)| (under, text)),
        );
    };
    for hunk in 0..patch.num_hunks() {
        let Ok((header, count)) = patch.hunk(hunk) else {
            continue;
        };
        // git starts an empty new side at the line before a deletion, a non-empty one at its first line
        let mut under = match header.new_lines() {
            0 => header.new_start(),
            _ => header.new_start().saturating_sub(1),
        };
        let mut block = Block::default();
        for index in 0..count {
            let Ok(line) = patch.line_in_hunk(hunk, index) else {
                continue;
            };
            match (line.origin(), line.old_lineno(), line.new_lineno()) {
                ('-', Some(old_line), _) => block.removed.push((
                    old_line,
                    under,
                    String::from_utf8_lossy(line.content())
                        .trim_end_matches(['\n', '\r'])
                        .to_string(),
                )),
                ('+', _, Some(new_line)) => {
                    under = new_line;
                    block.added.push(new_line);
                }
                (' ', _, Some(new_line)) => {
                    keep(std::mem::take(&mut block));
                    under = new_line;
                }
                _ => {}
            }
        }
        keep(block);
    }
    diff
}

pub fn row_of(state: &State, line: u32) -> usize {
    let hidden = crate::fold::hidden(state);
    if state.walking.is_none() && hidden.is_empty() {
        return line as usize;
    }
    let above = rows(state, line.saturating_sub(1) as usize).count();
    match hidden.contains(&(line as usize)) {
        true => above.max(1),
        false => above + 1,
    }
}

pub fn line_at_row(state: &State, row: usize) -> usize {
    let hidden = crate::fold::hidden(state);
    if state.walking.is_none() && hidden.is_empty() {
        return row;
    }
    let mut line = row;
    let lines = crate::current_buffer(state).map_or(0, |buffer| buffer.shown().split('\n').count());
    for (index, entry) in rows(state, lines.max(row)).enumerate() {
        if let Row::Code(number) = entry {
            line = number as usize;
        }
        if index + 1 == row {
            break;
        }
    }
    line
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn rows_are_worked_out_only_as_far_as_they_are_read() {
        let state = State::default();
        assert_eq!(rows(&state, u32::MAX as usize).nth(2), Some(Row::Code(3)));
    }

    #[test]
    fn the_branch_list_is_deduped_by_short_name_and_newest_first() {
        let refs = [
            branch_ref("stale", false, 100),
            branch_ref("origin/feature", true, 300),
            branch_ref("feature", false, 300),
            branch_ref("main", false, 200),
            branch_ref("origin/pushed-only", true, 250),
            branch_ref("origin/HEAD", true, 400),
        ];
        assert_eq!(
            branches(&refs, ""),
            ["feature", "pushed-only", "main", "stale"]
        );
    }

    #[test]
    fn a_tie_on_the_commit_date_is_broken_by_name() {
        let refs = [
            branch_ref("b", false, 1),
            branch_ref("a", false, 1),
            branch_ref("origin/c", true, 1),
        ];
        assert_eq!(branches(&refs, ""), ["a", "b", "c"]);
    }

    #[test]
    fn varde_s_own_directory_is_not_uncommitted_work() {
        let file = |path: &str| crate::review::GitFile {
            path: path.to_string(),
            status: crate::review::GitStatus::Untracked,
        };
        assert!(!uncommitted(&[]));
        assert!(!uncommitted(&[
            file(".varde/state.json"),
            file(".varde/risk.json")
        ]));
        assert!(uncommitted(&[
            file(".varde/state.json"),
            file("src/keys.rs")
        ]));
        assert!(uncommitted(&[file(".varden/notes.md")]));
    }

    #[test]
    fn typing_narrows_the_list_without_reordering_it() {
        let refs = [
            branch_ref("fix-the-tree", false, 100),
            branch_ref("origin/feature", true, 300),
            branch_ref("main", false, 200),
        ];
        assert_eq!(branches(&refs, "fea"), ["feature"]);
        assert_eq!(branches(&refs, "e"), ["feature", "fix-the-tree"]);
        assert_eq!(branches(&refs, "fxt"), ["fix-the-tree"]);
        assert!(branches(&refs, "nope").is_empty());
        assert_eq!(branches(&refs, ""), ["feature", "main", "fix-the-tree"]);
    }

    fn branch_ref(name: &str, remote: bool, when: i64) -> BranchRef {
        BranchRef {
            name: name.to_string(),
            remote,
            when,
        }
    }

    #[test]
    fn a_range_is_the_merge_base_against_the_base() {
        assert_eq!(range("main", "HEAD"), "main...HEAD");
        assert_eq!(revisions("main...HEAD"), Some(("main", "HEAD", true)));
    }

    #[test]
    fn the_uncommitted_range_is_still_two_dots() {
        assert_eq!(revisions("HEAD..worktree"), Some(("HEAD", WORKTREE, false)));
        assert_eq!(revisions("HEAD"), None);
    }

    #[test]
    fn an_artifact_with_fields_this_schema_does_not_name_still_parses() {
        let text = r#"{
            "protocolVersion": 2,
            "title": "Keys reach the child",
            "range": { "base": "aaaaaaaaaaaa", "head": "bbbbbbbbbbbb", "spelling": "main..HEAD" },
            "stories": [{
                "id": "s1", "name": "Keys reach the child", "premise": "…",
                "steps": [{
                    "id": "s1e1", "name": "Router matches", "claim": "…", "why": "…",
                    "site": { "file": "src/keys.rs", "side": "new", "kind": "changed",
                              "from": 10, "to": 12, "hash": "abc123", "text": "a\nb\nc" },
                    "prediction": { "question": "why?", "choices": [
                        { "id": "c1", "text": "a", "correct": true, "feedback": "yes" },
                        { "id": "c2", "text": "b", "correct": false, "feedback": "no" },
                        { "id": "c3", "text": "c", "correct": false, "feedback": "no" }
                    ] }
                }]
            }]
        }"#;
        assert!(parse(text).is_ok());
    }

    #[test]
    fn a_missing_required_field_refuses_the_whole_artifact() {
        let text =
            r#"{ "protocolVersion": 2, "stories": [ { "id": "s1", "name": "Half a story" } ] }"#;
        assert!(parse(text).is_err());
    }

    #[test]
    fn a_side_the_schema_does_not_name_refuses() {
        let text = r#"{
            "protocolVersion": 2,
            "title": "t",
            "range": { "base": "a", "head": "b", "spelling": "main..HEAD" },
            "stories": [{ "id": "s1", "name": "n", "premise": "p", "steps": [{
                "id": "s1e1", "name": "n", "claim": "c", "why": "w",
                "site": { "file": "f", "side": "left", "kind": "changed", "from": 1, "to": 1, "text": "t" }
            }] }]
        }"#;
        assert!(parse(text).is_err());
    }

    #[test]
    fn an_artifact_missing_a_title_refuses() {
        let text = r#"{
            "protocolVersion": 2,
            "range": { "base": "a", "head": "b", "spelling": "main..HEAD" },
            "stories": [{ "id": "s1", "name": "n", "premise": "p", "steps": [{
                "id": "s1e1", "name": "n", "claim": "c", "why": "w",
                "site": { "file": "f", "side": "new", "kind": "changed", "from": 1, "to": 1, "text": "t" }
            }] }]
        }"#;
        assert!(parse(text).is_err());
    }

    #[test]
    fn a_step_missing_a_name_refuses_the_whole_artifact() {
        let text = r#"{
            "protocolVersion": 2,
            "title": "t",
            "range": { "base": "a", "head": "b", "spelling": "main..HEAD" },
            "stories": [{ "id": "s1", "name": "n", "premise": "p", "steps": [{
                "id": "s1e1", "claim": "c", "why": "w",
                "site": { "file": "f", "side": "new", "kind": "changed", "from": 1, "to": 1, "text": "t" }
            }] }]
        }"#;
        assert!(parse(text).is_err());
    }

    #[test]
    fn a_prediction_offering_other_than_three_choices_refuses_the_whole_artifact() {
        let text = r#"{
            "protocolVersion": 2,
            "title": "t",
            "range": { "base": "a", "head": "b", "spelling": "main..HEAD" },
            "stories": [{ "id": "s1", "name": "n", "premise": "p", "steps": [{
                "id": "s1e1", "name": "n", "claim": "c", "why": "w",
                "site": { "file": "f", "side": "new", "kind": "changed", "from": 1, "to": 1, "text": "t" },
                "prediction": { "question": "q", "choices": [
                    { "text": "a", "correct": true, "feedback": "f" },
                    { "text": "b", "correct": false, "feedback": "f" }
                ] }
            }] }]
        }"#;
        assert!(parse(text).is_err());
    }

    #[test]
    fn the_ladder_prefers_origin_head_over_every_other_candidate() {
        assert_eq!(
            default_branch(Some("main"), Some("develop"), Some("trunk"), |_| true),
            Some("main".to_string())
        );
    }

    #[test]
    fn the_ladder_falls_back_to_the_upstream_branch() {
        assert_eq!(
            default_branch(None, Some("develop"), Some("trunk"), |_| true),
            Some("develop".to_string())
        );
    }

    #[test]
    fn the_ladder_falls_back_to_the_configured_default() {
        assert_eq!(
            default_branch(None, None, Some("trunk"), |_| true),
            Some("trunk".to_string())
        );
    }

    #[test]
    fn the_ladder_probes_main_before_master() {
        assert_eq!(
            default_branch(None, None, None, |name| name == "main" || name == "master"),
            Some("main".to_string())
        );
    }

    #[test]
    fn the_ladder_falls_back_to_master_when_main_does_not_exist() {
        assert_eq!(
            default_branch(None, None, None, |name| name == "master"),
            Some("master".to_string())
        );
    }

    #[test]
    fn the_ladder_resolves_nothing_when_no_candidate_exists() {
        assert_eq!(default_branch(None, None, None, |_| false), None);
    }

    #[test]
    fn an_already_authored_range_loads_rather_than_re_authoring() {
        assert_eq!(
            decide(
                false,
                true,
                "main..HEAD".to_string(),
                "out.json".to_string()
            ),
            Resolution::Authored
        );
    }

    #[test]
    fn force_re_authors_even_over_a_range_already_on_disk() {
        assert_eq!(
            decide(true, true, "main..HEAD".to_string(), "out.json".to_string()),
            Resolution::ToAuthor {
                spelling: "main..HEAD".to_string(),
                out: "out.json".to_string(),
            }
        );
    }

    #[test]
    fn a_range_not_yet_authored_is_offered_for_authoring() {
        assert_eq!(
            decide(
                false,
                false,
                "main..HEAD".to_string(),
                "out.json".to_string()
            ),
            Resolution::ToAuthor {
                spelling: "main..HEAD".to_string(),
                out: "out.json".to_string(),
            }
        );
    }

    #[test]
    fn worktree_needs_no_commit_to_resolve() {
        let file = Path::new(".varde/stories/aaaaaaaaaaaa-worktree.json");
        assert_eq!(
            range_status(file, |revision| revision == "aaaaaaaaaaaa"),
            RangeStatus::Resolves
        );
    }

    #[test]
    fn a_base_git_cannot_resolve_is_gone() {
        let file = Path::new(".varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.json");
        assert_eq!(range_status(file, |_| false), RangeStatus::Gone);
    }

    #[test]
    fn a_filename_varde_did_not_write_is_gone() {
        let file = Path::new(".varde/stories/manifest.json");
        assert_eq!(range_status(file, |_| true), RangeStatus::Gone);
    }

    #[test]
    fn artifact_path_names_a_commit_range() {
        assert_eq!(
            artifact_path(Path::new("/w/.varde"), "aaaaaaaaaaaa", "bbbbbbbbbbbb"),
            "/w/.varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.json"
        );
        assert_eq!(
            artifact_path(Path::new("/home/me/.varde/paths/%w-9"), "aaaaaaaaaaaa", "b"),
            "/home/me/.varde/paths/%w-9/stories/aaaaaaaaaaaa-b.json"
        );
    }

    #[test]
    fn artifact_path_names_a_dirty_range_distinctly() {
        assert_eq!(
            artifact_path(Path::new("/w/.varde"), "aaaaaaaaaaaa", "worktree"),
            "/w/.varde/stories/aaaaaaaaaaaa-worktree.json"
        );
    }

    #[test]
    fn the_companion_file_is_named_for_the_range_the_story_set_is() {
        let set = artifact_path(Path::new("/w/.varde"), "aaaaaaaaaaaa", "bbbbbbbbbbbb");
        let companion = context_path(&set);
        assert_eq!(
            companion,
            "/w/.varde/stories/aaaaaaaaaaaa-bbbbbbbbbbbb.context.md"
        );
        assert_eq!(
            companion.strip_suffix(".context.md"),
            set.strip_suffix(".json")
        );
        assert_eq!(
            context_path(&artifact_path(
                Path::new("/w/.varde"),
                "aaaaaaaaaaaa",
                WORKTREE
            )),
            "/w/.varde/stories/aaaaaaaaaaaa-worktree.context.md"
        );
    }

    #[test]
    fn the_companion_file_carries_both_resolved_oids() {
        let text = context_file(
            "aaaaaaaaaaaa1111",
            "bbbbbbbbbbbb2222",
            "main..HEAD",
            &[file_hunks("keys.rs", "a\nb\nc\n", "a\nX\nc\n")],
            CONTEXT_CAP,
        );
        assert!(text.contains("aaaaaaaaaaaa1111"));
        assert!(text.contains("bbbbbbbbbbbb2222"));
        assert!(text.contains("main..HEAD"));
    }

    #[test]
    fn the_companion_file_carries_the_ranges_diff() {
        let text = context_file(
            "base",
            "head",
            "main..HEAD",
            &[file_hunks("keys.rs", "a\nb\nc\n", "a\nX\nc\n")],
            CONTEXT_CAP,
        );
        assert!(text.contains("keys.rs"), "{text}");
        assert!(text.contains("@@"), "{text}");
        assert!(text.contains("-b"), "{text}");
        assert!(text.contains("+X"), "{text}");
    }

    #[test]
    fn the_companion_file_numbers_the_changed_files_lines() {
        let text = context_file(
            "base",
            "head",
            "main..HEAD",
            &[file_hunks("keys.rs", "a\nb\nc\n", "a\nX\nc\n")],
            CONTEXT_CAP,
        );
        assert!(text.contains("2 | X"), "{text}");
        assert!(text.contains("3 | c"), "{text}");
    }

    #[test]
    fn a_deleted_file_is_numbered_on_the_side_that_still_has_lines() {
        let mut gone = file_hunks("keys.rs", "a\nb\n", "");
        gone.new_exists = false;
        let text = context_file("base", "head", "main..HEAD", &[gone], CONTEXT_CAP);
        assert!(text.contains("1 | a"), "{text}");
        assert!(text.contains("old"), "{text}");
    }

    #[test]
    fn the_companion_files_cap_truncates_out_loud() {
        let long = (0..400)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        let files = [file_hunks(
            "keys.rs",
            &long,
            &long.replace("line 7\n", "LINE 7\n"),
        )];
        let whole = context_file("base", "head", "main..HEAD", &files, CONTEXT_CAP);
        let capped = context_file("base", "head", "main..HEAD", &files, 400);
        assert!(whole.len() > 400, "the fixture has to overflow the cap");
        assert!(!whole.contains(TRUNCATION));
        assert!(capped.len() < whole.len());
        assert!(capped.contains(TRUNCATION), "{capped}");
        assert!(capped.ends_with('\n'));
        assert!(whole.starts_with(&capped[..200]));
    }

    #[test]
    fn text_with_no_line_boundary_is_cut_at_a_character_instead() {
        let one_long_line = "x".repeat(4096);
        let capped = truncated(one_long_line, 300);
        assert_eq!(capped, format!("{}{TRUNCATION}", "x".repeat(300)));
    }

    #[test]
    fn a_cut_lands_on_a_character_boundary() {
        let wide = "é".repeat(400);
        let capped = truncated(wide, 301);
        assert_eq!(capped, format!("{}{TRUNCATION}", "é".repeat(150)));
    }

    #[test]
    fn the_companion_file_holds_the_changes_only() {
        let text = context_file(
            "base",
            "head",
            "main..HEAD",
            &[
                file_hunks("keys.rs", "a\nb\n", "a\nX\n"),
                file_hunks("mouse.rs", "same\n", "same\n"),
            ],
            CONTEXT_CAP,
        );
        assert!(text.contains("keys.rs"));
        assert!(!text.contains("mouse.rs"), "{text}");
        assert!(text.contains("changes only"), "{text}");
    }

    #[test]
    fn the_authoring_prompt_points_at_the_companion_file() {
        let text = prompt("main..HEAD", "out.json", ".varde/stories/a-b.context.md");
        assert!(text.contains(".varde/stories/a-b.context.md"));
        assert!(text.contains("changes only"));
    }

    #[test]
    fn the_authoring_prompt_says_a_context_step_means_opening_a_file() {
        let text = prompt("main..HEAD", "out.json", "context.md");
        let sentence = text
            .split("\n\n")
            .find(|paragraph| paragraph.contains("context.md"))
            .expect("the companion file is named somewhere");
        assert!(sentence.contains("`context`"), "{sentence}");
        assert!(sentence.contains("open"), "{sentence}");
    }

    #[test]
    fn the_authoring_prompt_names_the_confirmed_range() {
        assert!(prompt("main..HEAD", "out.json", "context.md").contains("main..HEAD"));
    }

    #[test]
    fn the_authoring_prompt_names_the_output_file() {
        assert!(
            prompt("main..HEAD", ".varde/stories/a-b.json", "context.md")
                .contains(".varde/stories/a-b.json")
        );
    }

    #[test]
    fn the_authoring_prompt_leaves_no_substitution_unfilled() {
        let text = prompt("main..HEAD", "out.json", "context.md");
        assert!(!text.contains("{RANGE}"));
        assert!(!text.contains("{OUT}"));
        assert!(!text.contains("{CONTEXT}"));
    }

    #[test]
    fn the_authoring_prompt_asks_for_a_set_title_and_a_step_name_at_the_bumped_version() {
        let text = prompt("main..HEAD", "out.json", "context.md");
        assert!(text.contains("protocolVersion 2"));
        assert!(text.contains("`title`"));
        assert!(text.contains("`name`"));
    }

    #[test]
    fn the_authoring_prompt_asks_for_a_concrete_subject_not_a_metaphor() {
        let text = prompt("main..HEAD", "out.json", "context.md");
        assert!(text.contains("concrete subject"));
        assert!(text.contains("metaphor"));
        assert!(text.contains("`premise` is where the why goes"));
    }

    const SCHEMA_FIELDS: &[&str] = &[
        "protocolVersion",
        "title",
        "range",
        "base",
        "head",
        "spelling",
        "stories",
        "id",
        "name",
        "premise",
        "steps",
        "claim",
        "why",
        "site",
        "file",
        "side",
        "kind",
        "from",
        "to",
        "text",
        "flow",
        "in",
        "out",
        "values",
        "value",
        "provenance",
        "cite",
        "line",
        "nudge",
        "prediction",
        "question",
        "choices",
        "correct",
        "feedback",
    ];

    fn worked_example(text: &str) -> String {
        let start = text.find("```json").expect("a fenced worked example");
        let body = &text[start + "```json".len()..];
        let end = body.find("```").expect("a closed fence");
        body[..end].to_string()
    }

    #[test]
    fn the_authoring_prompt_names_every_field_the_schema_reads() {
        let text = prompt("main..HEAD", "out.json", "context.md");
        for field in SCHEMA_FIELDS {
            assert!(
                text.contains(&format!("\"{field}\":")),
                "the prompt never names `{field}`"
            );
        }
    }

    #[test]
    fn the_authoring_prompts_worked_example_is_a_valid_story_set() {
        let artifact = parse(&worked_example(&prompt(
            "main..HEAD",
            "out.json",
            "context.md",
        )))
        .expect("the worked example parses");
        assert_eq!(artifact.protocol_version, 2);
        assert_eq!(artifact.range.spelling, "main..HEAD");
        let step = &artifact.stories[0].steps[0];
        assert!(step.flow.is_some());
        assert!(step.nudge.is_some());
        assert!(step.prediction.is_some());
        assert!(!step.values.is_empty());
        assert!(step.values.iter().any(|value| value.cite.is_some()));
    }

    #[test]
    fn the_authoring_prompt_states_the_bounds_an_author_is_held_to() {
        let text = prompt("main..HEAD", "out.json", "context.md");
        for bound in [
            "optional and rare",
            "at most one or two per story",
            "exactly three choices",
            "Exactly one choice is `correct`",
            "Omit `flow`",
            "Omit `nudge`",
            "Omit `values`",
        ] {
            assert!(text.contains(bound), "the prompt never states `{bound}`");
        }
    }

    #[test]
    fn the_authoring_prompt_no_longer_asks_for_the_code_to_be_transcribed() {
        let text = prompt("main..HEAD", "out.json", "context.md");
        for gone in [
            "site.text",
            "extract it with a command",
            "re-read every site",
            "verbatim",
        ] {
            assert!(!text.contains(gone), "the prompt still says `{gone}`");
        }
        assert!(text.contains("Do not write the lines themselves"));
    }

    #[test]
    fn the_authoring_prompt_still_asks_for_execution_order() {
        assert!(prompt("main..HEAD", "out.json", "context.md")
            .contains("confirm the steps follow execution order rather than file order"));
    }

    #[test]
    fn a_set_still_being_filled_in_is_not_a_spine() {
        let state = State {
            story_set: Set::Filling {
                artifact: artifact_of(vec![(
                    "first",
                    vec![step("1", site("a.rs", Side::New, Kind::Changed, 1, 1))],
                )]),
                attempt: Some(1),
            },
            ..State::default()
        };
        assert_eq!(view_state(&state), "filling");
        assert!(spine(&state).is_empty());
        assert!(current_step(&state).is_none());
    }

    #[test]
    fn pruning_under_the_limit_keeps_everything() {
        let existing = vec!["a".to_string(), "b".to_string()];
        assert_eq!(prune(&existing, 10), existing);
    }

    #[test]
    fn pruning_over_the_limit_drops_the_oldest_first() {
        let existing: Vec<String> = (0..11).map(|n| n.to_string()).collect();
        let kept = prune(&existing, 10);
        assert_eq!(kept.len(), 10);
        assert_eq!(kept.first(), Some(&"1".to_string()));
        assert!(!kept.contains(&"0".to_string()));
    }

    #[test]
    fn authoring_reports_its_own_view_state() {
        let state = State {
            story_set: Set::Authoring {
                spelling: "main..HEAD".to_string(),
            },
            ..State::default()
        };
        assert_eq!(view_state(&state), "authoring");
        assert!(spine(&state).is_empty());
    }

    #[test]
    fn an_abandoned_authoring_reports_its_own_view_state() {
        let state = State {
            story_set: Set::AuthoringAbandoned,
            ..State::default()
        };
        assert_eq!(view_state(&state), "authoring-abandoned");
    }

    #[test]
    fn a_one_line_change_is_one_hunk_with_both_sides_lines() {
        let old = "a\nb\nc\n";
        let new = "a\nX\nc\n";
        let found = hunks(old.as_bytes(), new.as_bytes());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].old_start, 1);
        assert_eq!(found[0].old_lines, 3);
        assert_eq!(found[0].new_start, 1);
        assert_eq!(found[0].new_lines, 3);
    }

    #[test]
    fn identical_bytes_are_no_hunks_at_all() {
        assert!(hunks(b"same\n", b"same\n").is_empty());
    }

    #[test]
    fn a_file_deleted_entirely_is_one_hunk_with_no_new_lines() {
        let found = hunks(b"gone\n", b"");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].new_lines, 0);
        assert!(found[0].is_deletion());
    }

    fn site(file: &str, side: Side, kind: Kind, from: u32, to: u32) -> Site {
        Site {
            file: file.to_string(),
            side,
            kind,
            from,
            to,
            text: String::new(),
        }
    }

    fn step(id: &str, site: Site) -> Step {
        Step {
            id: id.to_string(),
            name: id.to_string(),
            claim: "claim".to_string(),
            why: "why".to_string(),
            site,
            flow: None,
            values: Vec::new(),
            nudge: None,
            prediction: None,
        }
    }

    fn artifact_of(stories: Vec<(&str, Vec<Step>)>) -> Artifact {
        let Set::Loaded(artifact) = loaded(stories).story_set else {
            unreachable!("the helper loads one")
        };
        artifact
    }

    #[test]
    fn every_file_the_artifact_names_is_asked_after_once() {
        let mut cited = step("2", site("b.rs", Side::Old, Kind::Changed, 3, 4));
        cited.values = vec![Value {
            name: "limit".to_string(),
            value: "10".to_string(),
            provenance: Provenance::Literal,
            cite: Some(Cite {
                file: "d.rs".to_string(),
                line: 7,
            }),
        }];
        let artifact = artifact_of(vec![
            (
                "first",
                vec![step("1", site("a.rs", Side::New, Kind::Changed, 1, 2))],
            ),
            (
                "second",
                vec![
                    cited,
                    step("3", site("a.rs", Side::New, Kind::Context, 5, 6)),
                ],
            ),
        ]);
        assert_eq!(story_files(&artifact), ["a.rs", "b.rs", "d.rs"]);
    }

    #[test]
    fn the_fill_lands_each_site_on_the_lines_it_names() {
        let mut artifact = artifact_of(vec![
            (
                "first",
                vec![step("1", site("a.rs", Side::New, Kind::Changed, 1, 1))],
            ),
            (
                "second",
                vec![step("2", site("b.rs", Side::Old, Kind::Changed, 2, 2))],
            ),
        ]);
        fill(
            &mut artifact,
            &[
                file_hunks(
                    "a.rs",
                    "",
                    "from a
second
",
                ),
                file_hunks(
                    "b.rs",
                    "first
from b
",
                    "",
                ),
            ],
        );
        assert_eq!(artifact.stories[0].steps[0].site.text, "from a");
        assert_eq!(artifact.stories[1].steps[0].site.text, "from b");
    }

    #[test]
    fn the_fill_leaves_a_text_the_artifact_transcribed_alone() {
        let mut transcribed = site("a.rs", Side::New, Kind::Changed, 1, 1);
        transcribed.text = "what it said then".to_string();
        let mut artifact = artifact_of(vec![("first", vec![step("1", transcribed)])]);
        fill(
            &mut artifact,
            &[file_hunks(
                "a.rs",
                "",
                "what it says now
",
            )],
        );
        assert_eq!(artifact.stories[0].steps[0].site.text, "what it said then");
    }

    #[test]
    fn a_re_read_of_the_loaded_set_is_the_same_set_however_it_was_filled() {
        let arrived = artifact_of(vec![(
            "first",
            vec![step("1", site("a.rs", Side::New, Kind::Changed, 1, 1))],
        )]);
        let mut loaded = arrived.clone();
        fill(
            &mut loaded,
            &[file_hunks(
                "a.rs",
                "",
                "what the file held
",
            )],
        );
        assert!(same_set(&loaded, &arrived));
    }

    #[test]
    fn a_re_authored_set_is_not_the_set_already_loaded() {
        let loaded = artifact_of(vec![(
            "first",
            vec![step("1", site("a.rs", Side::New, Kind::Changed, 1, 1))],
        )]);
        let arrived = artifact_of(vec![(
            "first",
            vec![step("1", site("a.rs", Side::New, Kind::Changed, 9, 9))],
        )]);
        assert!(!same_set(&loaded, &arrived));
    }

    fn loaded(stories: Vec<(&str, Vec<Step>)>) -> State {
        loaded_over("b", stories)
    }

    fn loaded_over(head: &str, stories: Vec<(&str, Vec<Step>)>) -> State {
        let artifact = Artifact {
            protocol_version: 2,
            title: "Title".to_string(),
            range: Range {
                base: "a".to_string(),
                head: head.to_string(),
                spelling: "main..HEAD".to_string(),
            },
            stories: stories
                .into_iter()
                .enumerate()
                .map(|(index, (name, steps))| Story {
                    id: format!("s{index}"),
                    name: name.to_string(),
                    premise: "premise".to_string(),
                    steps,
                })
                .collect(),
        };
        State {
            story_set: Set::Loaded(artifact),
            ..State::default()
        }
    }

    fn file_hunks(file: &str, old: &str, new: &str) -> FileHunks {
        FileHunks {
            file: file.to_string(),
            hunks: hunks(old.as_bytes(), new.as_bytes()),
            old_exists: true,
            old_text: old.to_string(),
            head_exists: !new.is_empty(),
            head_text: new.to_string(),
            new_exists: !new.is_empty(),
            new_text: new.to_string(),
        }
    }

    fn checked(sites: Vec<(&str, Site)>) -> Vec<Problem> {
        let artifact = artifact_of(vec![(
            "first",
            sites.into_iter().map(|(id, site)| step(id, site)).collect(),
        )]);
        problems(&artifact, &[file_hunks("a.rs", "", "one\ntwo\n")])
    }

    #[test]
    fn a_story_set_that_points_where_it_says_has_no_problems() {
        assert!(checked(vec![("1", site("a.rs", Side::New, Kind::Changed, 1, 2))]).is_empty());
    }

    #[test]
    fn a_site_naming_a_file_that_is_not_there_is_a_problem() {
        assert_eq!(
            checked(vec![("1", site("gone.rs", Side::New, Kind::Changed, 1, 1))]),
            [Problem {
                step: "1".to_string(),
                fault: Fault::FileMissing,
            }]
        );
    }

    #[test]
    fn a_site_running_past_the_end_of_its_file_is_a_problem() {
        assert_eq!(
            checked(vec![("1", site("a.rs", Side::New, Kind::Changed, 1, 9))]),
            [Problem {
                step: "1".to_string(),
                fault: Fault::RangeOutOfBounds,
            }]
        );
    }

    #[test]
    fn a_changed_site_that_overlaps_no_hunk_is_a_problem() {
        let artifact = artifact_of(vec![(
            "first",
            vec![step("1", site("b.rs", Side::New, Kind::Changed, 1, 1))],
        )]);
        let unchanged = file_hunks("b.rs", "same\n", "same\n");
        assert_eq!(
            problems(&artifact, &[unchanged]),
            [Problem {
                step: "1".to_string(),
                fault: Fault::ClaimsNoChange,
            }]
        );
    }

    #[test]
    fn a_context_site_that_overlaps_no_hunk_is_not_a_problem() {
        let artifact = artifact_of(vec![(
            "first",
            vec![step("1", site("b.rs", Side::New, Kind::Context, 1, 1))],
        )]);
        let unchanged = file_hunks("b.rs", "same\n", "same\n");
        assert!(problems(&artifact, &[unchanged]).is_empty());
    }

    #[test]
    fn a_working_tree_that_moved_under_a_committed_range_is_not_a_problem() {
        let mut moved = file_hunks("a.rs", "", "one\ntwo\n");
        moved.new_text = "one\n".to_string();
        let mut steps = vec![step("1", site("a.rs", Side::New, Kind::Changed, 1, 2))];
        steps[0].values = vec![Value {
            name: "limit".to_string(),
            value: "two".to_string(),
            provenance: Provenance::Literal,
            cite: Some(Cite {
                file: "a.rs".to_string(),
                line: 2,
            }),
        }];
        let artifact = artifact_of(vec![("first", steps)]);
        assert!(problems(&artifact, &[moved]).is_empty());
    }

    #[test]
    fn a_cited_value_that_is_not_on_the_line_it_cites_is_a_problem() {
        let cite = |value: &str| Value {
            name: "limit".to_string(),
            value: value.to_string(),
            provenance: Provenance::Literal,
            cite: Some(Cite {
                file: "a.rs".to_string(),
                line: 2,
            }),
        };
        let mut steps = vec![
            step("1", site("a.rs", Side::New, Kind::Changed, 1, 2)),
            step("2", site("a.rs", Side::New, Kind::Changed, 1, 2)),
        ];
        steps[0].values = vec![cite("two")];
        steps[1].values = vec![cite("three")];
        let artifact = artifact_of(vec![("first", steps)]);
        assert_eq!(
            problems(&artifact, &[file_hunks("a.rs", "", "one\ntwo\n")]),
            [Problem {
                step: "2".to_string(),
                fault: Fault::CitationAbsent,
            }]
        );
    }

    #[test]
    fn a_value_with_no_citation_is_not_checked() {
        let mut steps = vec![step("1", site("a.rs", Side::New, Kind::Changed, 1, 2))];
        steps[0].values = vec![Value {
            name: "guess".to_string(),
            value: "nowhere in the file".to_string(),
            provenance: Provenance::Invented,
            cite: None,
        }];
        let artifact = artifact_of(vec![("first", steps)]);
        assert!(problems(&artifact, &[file_hunks("a.rs", "", "one\ntwo\n")]).is_empty());
    }

    #[test]
    fn a_refusal_names_every_failing_step_and_its_fault() {
        let problems = vec![
            Problem {
                step: "s1e1".to_string(),
                fault: Fault::FileMissing,
            },
            Problem {
                step: "s2e3".to_string(),
                fault: Fault::CitationAbsent,
            },
        ];
        let because = refusal(&problems);
        assert!(because.contains("s1e1: file-missing"), "{because}");
        assert!(because.contains("s2e3: citation-absent"), "{because}");
    }

    fn one_failing_step() -> (Artifact, Vec<Problem>) {
        let artifact = artifact_of(vec![(
            "Story",
            vec![
                step("s1", site("a.rs", Side::New, Kind::Changed, 1, 1)),
                step("s2", site("b.rs", Side::New, Kind::Changed, 1, 1)),
            ],
        )]);
        let problems = vec![Problem {
            step: "s2".to_string(),
            fault: Fault::ClaimsNoChange,
        }];
        (artifact, problems)
    }

    #[test]
    fn a_fix_request_names_the_failing_step_and_no_other() {
        let (artifact, problems) = one_failing_step();
        let text = fix_prompt(Path::new("/w/.varde"), &artifact, &problems);
        assert!(text.contains("`s2`: claims-no-change"), "{text}");
        assert!(!text.contains("`s1`"), "{text}");
    }

    #[test]
    fn a_fix_request_names_every_failing_step() {
        let (artifact, _) = one_failing_step();
        let problems = vec![
            Problem {
                step: "s1".to_string(),
                fault: Fault::FileMissing,
            },
            Problem {
                step: "s2".to_string(),
                fault: Fault::RangeOutOfBounds,
            },
        ];
        let text = fix_prompt(Path::new("/w/.varde"), &artifact, &problems);
        assert!(text.contains("`s1`: file-missing"), "{text}");
        assert!(text.contains("`s2`: range-out-of-bounds"), "{text}");
    }

    #[test]
    fn a_fix_request_asks_for_the_path_the_artifact_came_from() {
        let (artifact, problems) = one_failing_step();
        let text = fix_prompt(Path::new("/w/.varde"), &artifact, &problems);
        assert!(
            text.contains(&artifact_path(Path::new("/w/.varde"), "a", "b")),
            "{text}"
        );
        assert!(
            text.contains(&context_path(&artifact_path(
                Path::new("/w/.varde"),
                "a",
                "b"
            ))),
            "{text}"
        );
    }

    #[test]
    fn a_fix_request_for_a_dirty_range_names_the_worktree_set() {
        let mut artifact = artifact_of(vec![(
            "Story",
            vec![step("s1", site("a.rs", Side::New, Kind::Changed, 1, 1))],
        )]);
        artifact.range.head = WORKTREE.to_string();
        let problems = vec![Problem {
            step: "s1".to_string(),
            fault: Fault::FileMissing,
        }];
        assert!(
            fix_prompt(Path::new("/w/.varde"), &artifact, &problems).contains(&artifact_path(
                Path::new("/w/.varde"),
                "a",
                WORKTREE
            )),
            "{artifact:?}"
        );
    }

    #[test]
    fn a_fix_request_for_an_abbreviated_oid_still_names_a_path() {
        let mut artifact = artifact_of(vec![(
            "Story",
            vec![step("s1", site("a.rs", Side::New, Kind::Changed, 1, 1))],
        )]);
        artifact.range.base = "ab".to_string();
        artifact.range.head = "cd".to_string();
        let problems = vec![Problem {
            step: "s1".to_string(),
            fault: Fault::FileMissing,
        }];
        assert!(
            fix_prompt(Path::new("/w/.varde"), &artifact, &problems).contains(&artifact_path(
                Path::new("/w/.varde"),
                "ab",
                "cd"
            )),
            "{artifact:?}"
        );
    }

    #[test]
    fn a_fix_request_leaves_no_substitution_unfilled() {
        let (artifact, problems) = one_failing_step();
        let text = fix_prompt(Path::new("/w/.varde"), &artifact, &problems);
        assert!(!text.contains("{OUT}"));
        assert!(!text.contains("{PROBLEMS}"));
        assert!(!text.contains("{CONTEXT}"));
    }

    #[test]
    fn every_fault_a_check_can_report_says_what_would_make_it_right() {
        let (artifact, _) = one_failing_step();
        for fault in [
            Fault::FileMissing,
            Fault::RangeOutOfBounds,
            Fault::ClaimsNoChange,
            Fault::CitationAbsent,
        ] {
            let problems = vec![Problem {
                step: "s2".to_string(),
                fault,
            }];
            let text = fix_prompt(Path::new("/w/.varde"), &artifact, &problems);
            assert!(text.contains(fault.complaint()), "{fault:?}");
        }
    }

    #[test]
    fn a_set_being_fixed_reports_its_own_view_state() {
        let state = State {
            story_set: Set::Fixing {
                artifact: artifact_of(vec![(
                    "Story",
                    vec![step("s1", site("a.rs", Side::New, Kind::Changed, 1, 1))],
                )]),
                attempt: 2,
                problems: vec![Problem {
                    step: "s1".to_string(),
                    fault: Fault::FileMissing,
                }],
            },
            ..State::default()
        };
        assert_eq!(view_state(&state), "fixing");
        assert!(spine(&state).is_empty());
        assert!(current_step(&state).is_none());
    }

    #[test]
    fn a_hunk_no_step_claims_is_the_remainder() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 4, 4))],
        )]);
        state.file_hunks = vec![
            file_hunks("keys.rs", "a\nb\nc\nd\ne\n", "a\nb\nc\nX\ne\n"),
            file_hunks("mouse.rs", "a\nb\nc\n", "a\nX\nc\n"),
        ]
        .into();
        let found = remainder(&state);
        assert_eq!(found.unclaimed, 1);
        assert_eq!(found.locations, vec!["mouse.rs".to_string()]);
    }

    #[test]
    fn remainder_locations_names_the_same_file_the_count_does() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 4, 4))],
        )]);
        state.file_hunks = vec![
            file_hunks("keys.rs", "a\nb\nc\nd\ne\n", "a\nb\nc\nX\ne\n"),
            file_hunks("mouse.rs", "a\nb\nc\n", "a\nX\nc\n"),
        ]
        .into();
        let found = remainder_locations(&state);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "mouse.rs");
        assert_eq!((found[0].from, found[0].to), (1, 3));
    }

    #[test]
    fn remainder_locations_excludes_a_pure_deletion() {
        let mut state = loaded(Vec::new());
        state.file_hunks = vec![file_hunks("dead.rs", "gone\n", "")].into();
        assert!(remainder_locations(&state).is_empty());
    }

    #[test]
    fn current_step_answers_nothing_while_walking_the_remainder() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 4, 4))],
        )]);
        state.walking = Some(Walking::Remainder { index: 0 });
        assert!(current_step(&state).is_none());
    }

    #[test]
    fn a_site_claims_a_hunk_by_overlapping_it_not_by_containing_it() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 1, 2))],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\nd\ne\n", "a\nb\nc\nX\ne\n")].into();
        assert_eq!(remainder(&state).unclaimed, 0);
    }

    #[test]
    fn a_context_site_claims_nothing() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Context, 4, 4))],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\nd\ne\n", "a\nb\nc\nX\ne\n")].into();
        assert_eq!(remainder(&state).unclaimed, 1);
    }

    #[test]
    fn two_stories_claiming_the_same_hunk_count_it_once() {
        let mut state = loaded(vec![
            (
                "Keys reach the child",
                vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 4, 4))],
            ),
            (
                "Clicks find a pane",
                vec![step("s2", site("keys.rs", Side::New, Kind::Changed, 4, 4))],
            ),
        ]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\nd\ne\n", "a\nb\nc\nX\ne\n")].into();
        assert_eq!(remainder(&state).unclaimed, 0);
    }

    #[test]
    fn a_range_that_deletes_nothing_has_no_deletions_line() {
        let state = loaded(Vec::new());
        assert_eq!(remainder(&state).unwalked_deletions, None);
    }

    #[test]
    fn a_deletion_no_old_side_step_claims_is_an_unwalked_deletion() {
        let mut state = loaded(Vec::new());
        state.file_hunks = vec![file_hunks("dead.rs", "gone\n", "")].into();
        assert_eq!(remainder(&state).unwalked_deletions, Some(1));
        assert_eq!(remainder(&state).unclaimed, 0);
    }

    #[test]
    fn stepping_forward_past_the_last_step_holds_there() {
        assert_eq!(advance(3, 2, Direction::Right), 2);
    }

    #[test]
    fn stepping_back_at_the_first_step_holds_there() {
        assert_eq!(advance(3, 0, Direction::Left), 0);
    }

    #[test]
    fn stepping_forward_advances_by_one() {
        assert_eq!(advance(3, 0, Direction::Right), 1);
    }

    #[test]
    fn an_old_side_step_over_the_deleted_range_walks_it() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("dead.rs", Side::Old, Kind::Changed, 1, 1))],
        )]);
        state.file_hunks = vec![file_hunks("dead.rs", "gone\n", "")].into();
        assert_eq!(remainder(&state).unwalked_deletions, Some(0));
    }

    fn only_step(state: &State) -> &Step {
        let Set::Loaded(artifact) = &state.story_set else {
            panic!("no story set loaded");
        };
        &artifact.stories[0].steps[0]
    }

    fn site_with_text(file: &str, side: Side, from: u32, to: u32, text: &str) -> Site {
        Site {
            file: file.to_string(),
            side,
            kind: Kind::Changed,
            from,
            to,
            text: text.to_string(),
        }
    }

    #[test]
    fn a_site_naming_a_file_never_diffed_is_fresh() {
        let state = loaded(vec![(
            "Story",
            vec![step(
                "s1",
                site_with_text("keys.rs", Side::New, 2, 2, "anything"),
            )],
        )]);
        assert_eq!(staleness(&state, only_step(&state)), Staleness::Fresh);
    }

    #[test]
    fn a_site_whose_lines_still_match_is_fresh() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site_with_text("keys.rs", Side::New, 2, 2, "b"))],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\n", "a\nb\nc\n")].into();
        assert_eq!(staleness(&state, only_step(&state)), Staleness::Fresh);
    }

    #[test]
    fn a_site_whose_file_no_longer_exists_is_gone() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site_with_text("keys.rs", Side::New, 2, 2, "b"))],
        )]);
        state.file_hunks = vec![FileHunks {
            file: "keys.rs".to_string(),
            hunks: Vec::new(),
            old_exists: true,
            old_text: "a\nb\nc\n".to_string(),
            head_exists: false,
            head_text: String::new(),
            new_exists: false,
            new_text: String::new(),
        }]
        .into();
        assert_eq!(staleness(&state, only_step(&state)), Staleness::Gone);
    }

    #[test]
    fn a_site_past_the_files_new_length_is_too_short() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site_with_text("keys.rs", Side::New, 4, 4, "d"))],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\nd\n", "a\nb\nc\n")].into();
        assert_eq!(staleness(&state, only_step(&state)), Staleness::TooShort);
    }

    #[test]
    fn a_site_whose_line_now_reads_something_else_is_changed() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site_with_text("keys.rs", Side::New, 2, 2, "b"))],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\n", "a\nX\nc\n")].into();
        assert_eq!(
            staleness(&state, only_step(&state)),
            Staleness::Changed {
                now: "X".to_string()
            }
        );
    }

    #[test]
    fn reindenting_the_site_does_not_mark_it_stale() {
        let mut state = loaded(vec![(
            "Story",
            vec![step(
                "s1",
                site_with_text("keys.rs", Side::New, 2, 2, "    b"),
            )],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\n    b\nc\n", "a\nb\nc\n")].into();
        assert_eq!(staleness(&state, only_step(&state)), Staleness::Fresh);
    }

    #[test]
    fn an_old_side_site_is_immune_to_the_new_side_moving() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site_with_text("keys.rs", Side::Old, 2, 2, "b"))],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\n", "a\nX\nc\n")].into();
        assert_eq!(staleness(&state, only_step(&state)), Staleness::Fresh);
    }

    #[test]
    fn an_old_side_site_goes_stale_once_the_old_text_itself_moves() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site_with_text("keys.rs", Side::Old, 2, 2, "b"))],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nX\nc\n", "a\nX\nc\n")].into();
        assert_eq!(
            staleness(&state, only_step(&state)),
            Staleness::Changed {
                now: "X".to_string()
            }
        );
    }

    #[test]
    fn a_dirty_buffer_overrides_the_polled_new_text() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site_with_text("keys.rs", Side::New, 2, 2, "b"))],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\n", "a\nb\nc\n")].into();
        let path = state.root.join("keys.rs");
        let mut buffer = crate::editor::Buffer::open("a\nb\nc\n", false, 4);
        buffer.key('j');
        buffer.key('x');
        state.buffers.insert(path, buffer);
        assert_eq!(
            staleness(&state, only_step(&state)),
            Staleness::Changed { now: String::new() }
        );
    }

    #[test]
    fn a_committed_range_is_an_inventory_of_its_own_two_revisions() {
        let state = loaded(vec![("Story", Vec::new())]);
        assert_eq!(
            inventory(&state.story_set),
            Inventory::Committed {
                base: "a",
                head: "b"
            }
        );
    }

    #[test]
    fn a_range_headed_at_the_worktree_reads_the_working_tree() {
        let state = loaded_over("worktree", vec![("Story", Vec::new())]);
        assert_eq!(inventory(&state.story_set), Inventory::Worktree);
    }

    #[test]
    fn an_arriving_artifact_names_its_own_two_revisions() {
        let committed = artifact_of(vec![("Story", Vec::new())]);
        assert_eq!(
            inventory_of(&committed),
            Inventory::Committed {
                base: "a",
                head: "b"
            }
        );
        let Set::Loaded(uncommitted) =
            loaded_over("worktree", vec![("Story", Vec::new())]).story_set
        else {
            unreachable!("the helper loads one")
        };
        assert_eq!(inventory_of(&uncommitted), Inventory::Worktree);
    }

    #[test]
    fn a_set_still_filling_is_read_as_the_range_it_describes() {
        let set = Set::Filling {
            artifact: artifact_of(vec![(
                "Story",
                vec![step("1", site("a.rs", Side::New, Kind::Changed, 1, 1))],
            )]),
            attempt: Some(1),
        };
        assert_eq!(
            inventory(&set),
            Inventory::Committed {
                base: "a",
                head: "b"
            }
        );
        assert_eq!(named_files(&set), ["a.rs"]);
    }

    #[test]
    fn a_set_that_loaded_nothing_reads_the_working_tree() {
        for set in [
            Set::None,
            Set::Refused {
                because: "why".to_string(),
            },
            Set::RangeGone,
            Set::NoDefaultBranch,
            Set::BadRange,
            Set::NotARepository,
            Set::WorkingTreeDirty,
            Set::GuestNeedsBareWorkspace,
            Set::NoGit,
            Set::Downloading {
                url: "git@github.com:them/theirs.git".to_string(),
                how: Download::Clone,
            },
            Set::DownloadFailed {
                how: Download::Fetch,
                status: Some("128".to_string()),
            },
            Set::Authoring {
                spelling: "main..HEAD".to_string(),
            },
            Set::AuthoringAbandoned,
        ] {
            assert_eq!(inventory(&set), Inventory::Worktree, "{set:?}");
            assert!(named_files(&set).is_empty(), "{set:?}");
        }
    }

    #[test]
    fn the_spine_counts_stale_steps_per_story() {
        let mut state = loaded(vec![(
            "Story",
            vec![
                step("s1", site_with_text("keys.rs", Side::New, 2, 2, "b")),
                step("s2", site_with_text("keys.rs", Side::New, 3, 3, "WRONG")),
            ],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\n", "a\nb\nc\n")].into();
        assert_eq!(spine(&state)[0].stale, 1);
    }

    #[test]
    fn a_step_marks_its_whole_site_with_its_kind() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 2, 4))],
        )]);
        state.walking = Some(Walking::Story {
            story: 0,
            step: 0,
            diff: Diff::Hidden,
        });
        assert_eq!(
            mark(&state),
            SiteMark::Site {
                file: "keys.rs".to_string(),
                from: 2,
                to: 4,
                kind: Kind::Changed,
            }
        );
    }

    #[test]
    fn a_site_the_story_only_passes_through_is_marked_as_context() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Context, 2, 2))],
        )]);
        state.walking = Some(Walking::Story {
            story: 0,
            step: 0,
            diff: Diff::Hidden,
        });
        assert!(matches!(
            mark(&state),
            SiteMark::Site {
                kind: Kind::Context,
                ..
            }
        ));
    }

    #[test]
    fn the_remainder_marks_its_location_as_changed_code() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 2, 2))],
        )]);
        state.file_hunks = vec![file_hunks("mouse.rs", "a\nb\nc\n", "a\nX\nc\n")].into();
        state.walking = Some(Walking::Remainder { index: 0 });
        assert!(matches!(
            mark(&state),
            SiteMark::Site {
                kind: Kind::Changed,
                ..
            }
        ));
    }

    fn named_step(id: &str, name: &str, site: Site) -> Step {
        Step {
            name: name.to_string(),
            ..step(id, site)
        }
    }

    #[test]
    fn the_step_menu_is_empty_while_not_walking() {
        let state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 2, 2))],
        )]);
        assert!(step_menu(&state).is_empty());
    }

    #[test]
    fn the_step_menu_is_empty_while_walking_the_remainder() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 2, 2))],
        )]);
        state.file_hunks = vec![file_hunks("mouse.rs", "a\nb\nc\n", "a\nX\nc\n")].into();
        state.walking = Some(Walking::Remainder { index: 0 });
        assert!(step_menu(&state).is_empty());
    }

    #[test]
    fn the_step_menu_lists_every_step_of_the_current_story_in_order() {
        let mut state = loaded(vec![(
            "Story",
            vec![
                named_step(
                    "s1",
                    "First",
                    site("keys.rs", Side::New, Kind::Changed, 1, 1),
                ),
                named_step(
                    "s2",
                    "Second",
                    site("keys.rs", Side::New, Kind::Changed, 2, 2),
                ),
                named_step(
                    "s3",
                    "Third",
                    site("keys.rs", Side::New, Kind::Changed, 3, 3),
                ),
            ],
        )]);
        state.walking = Some(Walking::Story {
            story: 0,
            step: 0,
            diff: Diff::Hidden,
        });
        assert_eq!(
            step_menu(&state),
            vec![
                StepMenuRow {
                    name: "First".to_string(),
                    current: true
                },
                StepMenuRow {
                    name: "Second".to_string(),
                    current: false
                },
                StepMenuRow {
                    name: "Third".to_string(),
                    current: false
                },
            ]
        );
    }

    #[test]
    fn the_step_menu_marks_a_middle_step_current() {
        let mut state = loaded(vec![(
            "Story",
            vec![
                named_step(
                    "s1",
                    "First",
                    site("keys.rs", Side::New, Kind::Changed, 1, 1),
                ),
                named_step(
                    "s2",
                    "Second",
                    site("keys.rs", Side::New, Kind::Changed, 2, 2),
                ),
                named_step(
                    "s3",
                    "Third",
                    site("keys.rs", Side::New, Kind::Changed, 3, 3),
                ),
            ],
        )]);
        state.walking = Some(Walking::Story {
            story: 0,
            step: 1,
            diff: Diff::Hidden,
        });
        let current: Vec<String> = step_menu(&state)
            .into_iter()
            .filter(|row| row.current)
            .map(|row| row.name)
            .collect();
        assert_eq!(current, vec!["Second".to_string()]);
    }

    #[test]
    fn the_step_menu_marks_the_last_step_current() {
        let mut state = loaded(vec![(
            "Story",
            vec![
                named_step(
                    "s1",
                    "First",
                    site("keys.rs", Side::New, Kind::Changed, 1, 1),
                ),
                named_step(
                    "s2",
                    "Second",
                    site("keys.rs", Side::New, Kind::Changed, 2, 2),
                ),
                named_step(
                    "s3",
                    "Third",
                    site("keys.rs", Side::New, Kind::Changed, 3, 3),
                ),
            ],
        )]);
        state.walking = Some(Walking::Story {
            story: 0,
            step: 2,
            diff: Diff::Hidden,
        });
        let current: Vec<String> = step_menu(&state)
            .into_iter()
            .filter(|row| row.current)
            .map(|row| row.name)
            .collect();
        assert_eq!(current, vec!["Third".to_string()]);
    }

    #[test]
    fn a_site_running_past_the_end_of_the_file_is_marked_whole() {
        let mut state = loaded(vec![(
            "Story",
            vec![step(
                "s1",
                site("keys.rs", Side::New, Kind::Changed, 2, 900),
            )],
        )]);
        state.file_hunks = vec![file_hunks("keys.rs", "a\nb\nc\n", "a\nX\nc\n")].into();
        state.walking = Some(Walking::Story {
            story: 0,
            step: 0,
            diff: Diff::Hidden,
        });
        assert!(matches!(mark(&state), SiteMark::Site { to: 900, .. }));
    }

    #[test]
    fn an_old_side_site_refuses_rather_than_marking() {
        let mut state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::Old, Kind::Changed, 2, 2))],
        )]);
        state.walking = Some(Walking::Story {
            story: 0,
            step: 0,
            diff: Diff::Hidden,
        });
        assert_eq!(mark(&state), SiteMark::Refused);
    }

    #[test]
    fn nothing_is_marked_when_no_story_is_being_walked() {
        let state = loaded(vec![(
            "Story",
            vec![step("s1", site("keys.rs", Side::New, Kind::Changed, 2, 2))],
        )]);
        assert_eq!(mark(&state), SiteMark::Nothing);
    }

    #[test]
    fn the_mark_covers_its_range_and_nothing_else() {
        let marked = SiteMark::Site {
            file: "keys.rs".to_string(),
            from: 2,
            to: 4,
            kind: Kind::Changed,
        };
        assert!(marked.covers("keys.rs", 2));
        assert!(marked.covers("keys.rs", 4));
        assert!(!marked.covers("keys.rs", 1));
        assert!(!marked.covers("keys.rs", 5));
        assert!(!marked.covers("mouse.rs", 3));
    }

    #[test]
    fn a_refusal_covers_nothing() {
        assert!(!SiteMark::Refused.covers("keys.rs", 1));
    }

    fn commented_on(file: &str, line: u32, body: &str) -> crate::review::Comment {
        crate::review::Comment {
            file: file.to_string(),
            from_line: line,
            to_line: line,
            kind: "ISSUE".to_string(),
            body: body.to_string(),
            revision: "abc".to_string(),
            story: Some("Story".to_string()),
            step: Some(1),
        }
    }

    fn showing(file: &str, comments: Vec<crate::review::Comment>) -> State {
        State {
            root: std::path::PathBuf::from("/work"),
            current_buffer: Some(std::path::PathBuf::from("/work").join(file)),
            comments,
            walking: Some(Walking::Story {
                story: 0,
                step: 0,
                diff: Diff::Hidden,
            }),
            ..State::default()
        }
    }

    #[test]
    fn a_comment_gets_a_row_under_the_line_it_covers() {
        let state = showing(
            "src/keys.rs",
            vec![commented_on("src/keys.rs", 2, "the catch-all")],
        );
        let rows: Vec<_> = rows(&state, 3).collect();
        assert_eq!(rows[0], Row::Code(1));
        assert_eq!(rows[1], Row::Code(2));
        assert!(matches!(rows[2], Row::Comment(comment) if comment.body == "the catch-all"));
        assert_eq!(rows[3], Row::Code(3));
    }

    #[test]
    fn a_comment_row_does_not_renumber_the_lines_below_it() {
        let state = showing("src/keys.rs", vec![commented_on("src/keys.rs", 1, "first")]);
        assert_eq!(
            rows(&state, 3)
                .filter_map(|row| match row {
                    Row::Code(number) => Some(number),
                    Row::Comment(_) | Row::Removed(_) => None,
                })
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn a_comment_on_another_file_gets_no_row() {
        let state = showing(
            "src/keys.rs",
            vec![commented_on("src/mouse.rs", 2, "elsewhere")],
        );
        assert_eq!(
            rows(&state, 2).collect::<Vec<_>>(),
            vec![Row::Code(1), Row::Code(2)]
        );
    }

    #[test]
    fn every_comment_on_one_line_gets_its_own_row() {
        let state = showing(
            "src/keys.rs",
            vec![
                commented_on("src/keys.rs", 2, "first"),
                commented_on("src/keys.rs", 2, "second"),
            ],
        );
        assert_eq!(rows(&state, 2).count(), 4);
    }

    #[test]
    fn a_file_being_edited_rather_than_walked_gets_no_comment_rows() {
        let mut state = showing("src/keys.rs", vec![commented_on("src/keys.rs", 1, "first")]);
        state.walking = None;
        assert_eq!(
            rows(&state, 2).collect::<Vec<_>>(),
            vec![Row::Code(1), Row::Code(2)]
        );
        assert_eq!(row_of(&state, 2), 2);
        assert_eq!(line_at_row(&state, 2), 2);
    }

    #[test]
    fn a_line_with_no_comment_above_it_is_drawn_on_its_own_row() {
        let state = showing("src/keys.rs", Vec::new());
        assert_eq!(row_of(&state, 3), 3);
    }

    #[test]
    fn a_comment_above_a_line_pushes_it_down_a_row() {
        let state = showing("src/keys.rs", vec![commented_on("src/keys.rs", 1, "first")]);
        assert_eq!(row_of(&state, 3), 4);
    }

    #[test]
    fn a_comment_on_the_line_itself_does_not_move_it() {
        let state = showing("src/keys.rs", vec![commented_on("src/keys.rs", 3, "here")]);
        assert_eq!(row_of(&state, 3), 3);
    }

    #[test]
    fn a_row_below_a_comment_names_the_line_actually_drawn_on_it() {
        let state = showing("src/keys.rs", vec![commented_on("src/keys.rs", 1, "first")]);
        assert_eq!(line_at_row(&state, 1), 1);
        assert_eq!(line_at_row(&state, 3), 2);
        assert_eq!(line_at_row(&state, 4), 3);
    }

    #[test]
    fn a_comment_row_names_the_line_it_sits_under() {
        let state = showing("src/keys.rs", vec![commented_on("src/keys.rs", 2, "here")]);
        assert_eq!(line_at_row(&state, 3), 2);
    }

    #[test]
    fn a_line_survives_the_trip_through_a_row_and_back() {
        let state = showing(
            "src/keys.rs",
            vec![
                commented_on("src/keys.rs", 1, "first"),
                commented_on("src/keys.rs", 3, "third"),
            ],
        );
        for line in 1..=6u32 {
            assert_eq!(line_at_row(&state, row_of(&state, line)), line as usize);
        }
    }

    const BASE: &str = "a\nb\nc\nd\ne\nf\n";

    fn diff_shown(head: &str, side: Side, from: u32, to: u32) -> State {
        let path = std::path::PathBuf::from("/work/src/keys.rs");
        let mut state = State {
            root: std::path::PathBuf::from("/work"),
            current_buffer: Some(path.clone()),
            file_hunks: vec![file_hunks("src/keys.rs", BASE, head)].into(),
            ..loaded(vec![(
                "Story",
                vec![step(
                    "s1",
                    site("src/keys.rs", side, Kind::Changed, from, to),
                )],
            )])
        };
        state
            .buffers
            .insert(path, crate::editor::Buffer::open(head, false, 4));
        state.walking = Some(Walking::Story {
            story: 0,
            step: 0,
            diff: Diff::Shown,
        });
        state
    }

    #[test]
    fn a_new_side_site_marks_only_the_additions_inside_it() {
        assert_eq!(
            changes(BASE, "a\nB\nC\nd\ne\nf\n", Side::New, 3, 5),
            SiteDiff {
                added: vec![3],
                removed: vec![(1, "b".to_string()), (1, "c".to_string())],
            }
        );
    }

    #[test]
    fn a_deletion_between_a_sites_lines_is_shown_and_one_beside_it_is_not() {
        let head = "a\nb\nd\ne\nf\n";
        assert_eq!(
            changes(BASE, head, Side::New, 1, 3).removed,
            vec![(2, "c".to_string())]
        );
        assert_eq!(changes(BASE, head, Side::New, 3, 5).removed, vec![]);
    }

    #[test]
    fn a_deletion_of_the_first_line_sits_above_the_first_row() {
        assert_eq!(
            changes(BASE, "b\nc\nd\ne\nf\n", Side::Old, 1, 1).removed,
            vec![(0, "a".to_string())]
        );
    }

    #[test]
    fn an_old_side_site_takes_the_edit_that_removed_its_lines_whole() {
        assert_eq!(
            changes(BASE, "X\nb\nc\nd\nY\nf\n", Side::Old, 5, 5),
            SiteDiff {
                added: vec![5],
                removed: vec![(4, "e".to_string())],
            }
        );
    }

    #[test]
    fn a_hidden_diff_and_a_context_site_draw_nothing() {
        let mut hidden = diff_shown("a\nB\nc\nd\ne\nf\n", Side::New, 1, 3);
        hidden.walking = Some(Walking::Story {
            story: 0,
            step: 0,
            diff: Diff::Hidden,
        });
        assert_eq!(site_diff(&hidden), None);
        let Set::Loaded(artifact) = &mut hidden.story_set else {
            unreachable!("the helper loads one")
        };
        artifact.stories[0].steps[0].site.kind = Kind::Context;
        hidden.walking = Some(Walking::Story {
            story: 0,
            step: 0,
            diff: Diff::Shown,
        });
        assert_eq!(site_diff(&hidden), None);
    }

    #[test]
    fn a_removed_row_sits_under_the_line_before_it_and_pushes_the_rest_down() {
        let state = diff_shown("a\nB\nc\nd\ne\nf\n", Side::New, 1, 3);
        assert_eq!(
            rows(&state, 3).collect::<Vec<_>>(),
            vec![
                Row::Code(1),
                Row::Removed("b".to_string()),
                Row::Code(2),
                Row::Code(3),
            ]
        );
        assert_eq!(row_of(&state, 2), 3);
        assert_eq!(row_of(&state, 3), 4);
    }

    #[test]
    fn a_click_on_a_removed_row_lands_on_the_line_it_sits_under() {
        let state = diff_shown("a\nB\nc\nd\ne\nf\n", Side::New, 1, 3);
        assert_eq!(line_at_row(&state, 2), 1);
        assert_eq!(line_at_row(&state, 3), 2);
        assert_eq!(line_at_row(&state, 7), 6);
        for line in 1..=6u32 {
            assert_eq!(line_at_row(&state, row_of(&state, line)), line as usize);
        }
    }

    #[test]
    fn the_scroll_clamp_counts_removed_rows() {
        let state = diff_shown("a\nB\nC\nd\ne\nf", Side::New, 1, 3);
        assert_eq!(crate::editor_focus(&state, &[]).1, 8);
        let old = diff_shown("a\nB\nC\nd\ne\nf", Side::Old, 2, 3);
        assert!(!refused(&old));
        assert_eq!(crate::editor_focus(&old, &[]).1, 8);
    }

    #[test]
    fn the_file_on_screen_is_named_relative_to_the_repository_under_review() {
        assert_eq!(
            shown_file(&showing("src/keys.rs", Vec::new())),
            "src/keys.rs"
        );
        let guest = std::path::PathBuf::from("/work/.varde/guest/theirs");
        let state = State {
            current_buffer: Some(guest.join("src/theirs.rs")),
            guest: Some(guest),
            ..showing("src/keys.rs", Vec::new())
        };
        assert_eq!(shown_file(&state), "src/theirs.rs");
    }

    #[test]
    fn a_guest_repo_is_named_for_the_repository() {
        for (url, expected) in [
            ("git@github.com:them/theirs.git", "theirs"),
            ("https://github.com/them/theirs.git", "theirs"),
            ("https://github.com/them/theirs", "theirs"),
            ("ssh://git@host:2222/them/theirs.git/", "theirs"),
            ("/home/me/projects/theirs", "theirs"),
            ("https://host/them/..", "guest"),
            ("https://host/them/", "them"),
            ("", "guest"),
        ] {
            assert_eq!(guest_name(url), expected, "for {url}");
        }
    }

    #[test]
    fn a_clone_records_the_exit_status_of_the_clone() {
        let command = download_command(
            Download::Clone,
            "git@github.com:them/theirs.git",
            Path::new("/home/me/.varde/paths/x-1/theirs"),
            Path::new("/home/me/.varde/paths/x-1/clone-done"),
        );
        assert_eq!(
            command,
            "rm -f /home/me/.varde/paths/x-1/clone-done; \
             git clone git@github.com:them/theirs.git /home/me/.varde/paths/x-1/theirs; \
             echo $? > /home/me/.varde/paths/x-1/clone-done.writing; \
             mv /home/me/.varde/paths/x-1/clone-done.writing \
             /home/me/.varde/paths/x-1/clone-done"
        );
    }

    #[test]
    fn a_fetch_updates_the_guest_repos_refs_and_records_its_status() {
        let command = download_command(
            Download::Fetch,
            "git@github.com:them/theirs.git",
            Path::new("/home/me/.varde/paths/x-1/theirs"),
            Path::new("/home/me/.varde/paths/x-1/download-done"),
        );
        assert_eq!(
            command,
            "rm -f /home/me/.varde/paths/x-1/download-done; \
             git -C /home/me/.varde/paths/x-1/theirs fetch; \
             echo $? > /home/me/.varde/paths/x-1/download-done.writing; \
             mv /home/me/.varde/paths/x-1/download-done.writing \
             /home/me/.varde/paths/x-1/download-done"
        );
    }

    #[test]
    fn a_url_that_is_shell_syntax_is_quoted() {
        let command = download_command(
            Download::Clone,
            "https://host/x; rm -rf ~",
            Path::new("/tmp/guest"),
            Path::new("/tmp/done"),
        );
        assert!(
            command.contains("'https://host/x; rm -rf ~'"),
            "unquoted: {command}"
        );
    }
}
