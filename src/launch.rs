use crate::layout::{self, Area};
use crate::preview::Refusal;
use crate::startup::{self, Argument, ConfigError, ConfigFault, Launch};
use crate::{Chip, Event, Hue, Modal, State, Tone};
use std::collections::BTreeMap;
use std::path::PathBuf;
use terminput::{KeyCode, KeyEvent};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Launch,
    Attach,
}

impl Request {
    pub fn as_str(self) -> &'static str {
        match self {
            Request::Launch => "launch",
            Request::Attach => "attach",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Project,
    Global,
}

impl Target {
    pub fn as_str(self) -> &'static str {
        match self {
            Target::Project => "project",
            Target::Global => "global",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    Name,
    Adapter,
    Request,
    Argument(String),
    Key(usize),
    Value(usize),
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub name: String,
    pub adapter: String,
    pub request: Request,
    pub args: BTreeMap<String, String>,
    pub free: Vec<(String, String)>,
    pub target: Target,
    pub slot: Slot,
}

pub fn open(state: &State) -> Draft {
    Draft {
        name: String::new(),
        adapter: state.adapters.keys().next().cloned().unwrap_or_default(),
        request: Request::Launch,
        args: BTreeMap::new(),
        free: vec![(String::new(), String::new())],
        target: match state.sidecar {
            Some(_) => Target::Global,
            None => Target::Project,
        },
        slot: Slot::Name,
    }
}

pub fn arguments(state: &State, draft: &Draft) -> Vec<Argument> {
    let Some(adapter) = state.adapters.get(&draft.adapter) else {
        return Vec::new();
    };
    match draft.request {
        Request::Launch => adapter.launch_args.clone(),
        Request::Attach => adapter.attach_args.clone(),
    }
}

pub fn slots(state: &State, draft: &Draft) -> Vec<Slot> {
    let mut slots = vec![Slot::Name, Slot::Adapter, Slot::Request];
    let listed = arguments(state, draft);
    if listed.is_empty() {
        for index in 0..draft.free.len() {
            slots.push(Slot::Key(index));
            slots.push(Slot::Value(index));
        }
    } else {
        slots.extend(listed.into_iter().map(|row| Slot::Argument(row.key)));
    }
    if state.sidecar.is_none() {
        slots.push(Slot::Target);
    }
    slots
}

pub fn walk(state: &State, draft: &Draft, by: usize) -> Slot {
    let slots = slots(state, draft);
    let at = slots
        .iter()
        .position(|slot| *slot == draft.slot)
        .unwrap_or(0);
    slots[(at + by) % slots.len()].clone()
}

pub fn switchable(slot: &Slot) -> bool {
    matches!(slot, Slot::Adapter | Slot::Request | Slot::Target)
}

fn hint(keys: &[(&str, &str)]) -> (Tone, String) {
    let keys = keys
        .iter()
        .map(|(key, word)| format!("   {key}  {word}"))
        .collect();
    (Tone::Dimmed, keys)
}

fn list_rows(state: &State, selected: usize, height: u16) -> Vec<(Tone, String)> {
    let names = crate::debug::launches(state);
    let room = height.saturating_sub(4).max(1) as usize;
    let mut rows: Vec<(Tone, String)> = names
        .iter()
        .enumerate()
        .skip((selected + 1).saturating_sub(room))
        .take(room)
        .map(|(index, name)| {
            let cursor = match index == selected {
                true => '>',
                false => ' ',
            };
            (Tone::Plain, format!("{cursor} {name}"))
        })
        .collect();
    if rows.is_empty() {
        rows.push((
            Tone::Plain,
            "  No Launch configurations yet: c creates one.".to_string(),
        ));
    }
    rows.push((Tone::Plain, String::new()));
    rows.push(hint(&crate::keys::LAUNCH_LIST_KEYS));
    rows
}

fn form_rows(state: &State, draft: &Draft) -> Vec<(Tone, String)> {
    let explanations: BTreeMap<String, String> = arguments(state, draft)
        .into_iter()
        .map(|row| {
            let required = match row.required {
                true => " (required)",
                false => "",
            };
            (row.key, format!("{}{required}", row.explain))
        })
        .collect();
    let mut rows: Vec<(Tone, String)> = slots(state, draft)
        .into_iter()
        .map(|slot| {
            let cursor = match slot == draft.slot {
                true => '>',
                false => ' ',
            };
            let explain = match &slot {
                Slot::Argument(key) => explanations
                    .get(key)
                    .map(|explain| format!("   \u{2014} {explain}"))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            let row = format!(
                "{cursor} {:<22} {}{explain}",
                label(&slot),
                text(draft, &slot)
            );
            (Tone::Plain, row)
        })
        .collect();
    rows.push((Tone::Plain, String::new()));
    rows.push(hint(&crate::keys::LAUNCH_FORM_KEYS));
    rows
}

pub fn shown(state: &State, width: u16, height: u16) -> Option<(Area, Vec<(Tone, String)>)> {
    let rows = match &state.modal {
        Modal::Launches { row } => list_rows(state, *row, height),
        Modal::NewLaunch(draft) => form_rows(state, draft),
        _ => return None,
    };
    let measure = width.saturating_sub(4).max(20) as usize;
    let rows: Vec<(Tone, String)> = rows
        .into_iter()
        .flat_map(|(tone, row)| {
            if UnicodeWidthStr::width(row.as_str()) <= measure {
                return vec![(tone, row)];
            }
            let indent: String = row.chars().take_while(|c| *c == ' ').collect();
            let options = textwrap::Options::new(measure)
                .initial_indent(&indent)
                .subsequent_indent(&indent);
            textwrap::wrap(row.trim_start(), options)
                .into_iter()
                .map(|piece| (tone, piece.into_owned()))
                .collect()
        })
        .collect();
    let widest = rows
        .iter()
        .map(|(_, row)| UnicodeWidthStr::width(row.as_str()) as u16)
        .max()
        .unwrap_or(0);
    Some((
        layout::overlay(width, height, rows.len() as u16, widest),
        rows,
    ))
}

fn offered(state: &State) -> Vec<(Chip, KeyCode)> {
    let chip = |name, glyph: &str, keys, hue, tone| Chip {
        action: name,
        name,
        glyph: glyph.to_string(),
        keys,
        word: "",
        hue,
        tone,
    };
    let lit_when = |on: bool| match on {
        true => Tone::Plain,
        false => Tone::Dimmed,
    };
    let close = (
        chip("close", "\u{2715}", "Esc", Hue::Halt, Tone::Plain),
        KeyCode::Esc,
    );
    match &state.modal {
        Modal::Launches { .. } => vec![
            (
                chip(
                    "start",
                    "\u{25b6}",
                    "\u{21b5}",
                    Hue::Go,
                    lit_when(!crate::debug::launches(state).is_empty()),
                ),
                KeyCode::Enter,
            ),
            (
                chip("create", "+", "c", Hue::Plain, Tone::Plain),
                KeyCode::Char('c'),
            ),
            close,
        ],
        Modal::NewLaunch(draft) => vec![
            (
                chip("next", "\u{25bd}", "Tab", Hue::Step, Tone::Plain),
                KeyCode::Tab,
            ),
            (
                chip(
                    "switch",
                    "\u{25c7}",
                    "\u{2423}",
                    Hue::Step,
                    lit_when(switchable(&draft.slot)),
                ),
                KeyCode::Char(' '),
            ),
            (
                chip("create", "\u{25b6}", "\u{21b5}", Hue::Go, Tone::Plain),
                KeyCode::Enter,
            ),
            close,
        ],
        _ => Vec::new(),
    }
}

pub fn chips(state: &State) -> Vec<Chip> {
    offered(state).into_iter().map(|(chip, _)| chip).collect()
}

pub fn clicked(
    state: &State,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> Option<Vec<Event>> {
    let (area, _) = shown(state, width, height).filter(|(area, _)| area.y == row)?;
    let offered = offered(state);
    let chips: Vec<Chip> = offered.iter().map(|(chip, _)| chip.clone()).collect();
    let (chip, code) =
        &offered[layout::strip_at(area, &layout::chip_labels(&chips, area.width, 0), column)?];
    Some(match chip.tone {
        Tone::Dimmed => Vec::new(),
        _ => crate::keys::launch_key(state, KeyEvent::new(*code)),
    })
}

pub fn label(slot: &Slot) -> String {
    match slot {
        Slot::Name => "name".to_string(),
        Slot::Adapter => "adapter".to_string(),
        Slot::Request => "request".to_string(),
        Slot::Argument(key) => key.clone(),
        Slot::Key(index) => format!("key {}", index + 1),
        Slot::Value(index) => format!("value {}", index + 1),
        Slot::Target => "target".to_string(),
    }
}

pub fn text(draft: &Draft, slot: &Slot) -> String {
    match slot {
        Slot::Name => draft.name.clone(),
        Slot::Argument(key) => draft.args.get(key).cloned().unwrap_or_default(),
        Slot::Key(index) => draft.free[*index].0.clone(),
        Slot::Value(index) => draft.free[*index].1.clone(),
        Slot::Adapter => draft.adapter.clone(),
        Slot::Request => draft.request.as_str().to_string(),
        Slot::Target => draft.target.as_str().to_string(),
    }
}

pub fn typed(draft: &mut Draft, text: String) {
    match draft.slot.clone() {
        Slot::Name => draft.name = text,
        Slot::Argument(key) => {
            draft.args.insert(key, text);
        }
        Slot::Key(index) => {
            draft.free[index].0 = text;
            if draft.free.last().is_some_and(|(key, _)| !key.is_empty()) {
                draft.free.push((String::new(), String::new()));
            }
        }
        Slot::Value(index) => draft.free[index].1 = text,
        Slot::Adapter | Slot::Request | Slot::Target => {}
    }
}

pub fn switch(state: &State, draft: &mut Draft) {
    match draft.slot {
        Slot::Adapter => {
            let names: Vec<&String> = state.adapters.keys().collect();
            let at = names.iter().position(|name| **name == draft.adapter);
            if let Some(next) = at.and_then(|at| names.get((at + 1) % names.len())) {
                draft.adapter = (*next).clone();
            }
        }
        Slot::Request => {
            draft.request = match draft.request {
                Request::Launch => Request::Attach,
                Request::Attach => Request::Launch,
            }
        }
        Slot::Target => {
            draft.target = match draft.target {
                Target::Project => Target::Global,
                Target::Global => Target::Project,
            }
        }
        Slot::Name | Slot::Argument(_) | Slot::Key(_) | Slot::Value(_) => {}
    }
    if !slots(state, draft).contains(&draft.slot) {
        draft.slot = Slot::Name;
    }
}

pub fn missing(state: &State, draft: &Draft) -> Option<String> {
    if draft.name.trim().is_empty() {
        return Some("name".to_string());
    }
    if draft.adapter.is_empty() {
        return Some("adapter".to_string());
    }
    arguments(state, draft)
        .into_iter()
        .find(|row| {
            row.required
                && draft
                    .args
                    .get(&row.key)
                    .is_none_or(|value| value.trim().is_empty())
        })
        .map(|row| row.key)
}

pub fn path(state: &State, target: Target) -> PathBuf {
    match target {
        Target::Project => {
            crate::varde_dir(&state.root, state.sidecar.as_deref()).join(startup::CONFIG_FILE)
        }
        Target::Global => state.varde_home.join(startup::CONFIG_FILE),
    }
}

fn layer(target: Target) -> String {
    match target {
        Target::Project => startup::PROJECT_LABEL.to_string(),
        Target::Global => startup::GLOBAL_LABEL.to_string(),
    }
}

pub fn unreadable(target: Target) -> Refusal {
    Refusal::BrokenConfig(ConfigError {
        file: layer(target),
        line: 1,
        fault: ConfigFault::Unreadable,
    })
}

fn written(text: &str) -> toml_edit::Value {
    match text.parse::<toml_edit::Value>() {
        Ok(
            value @ (toml_edit::Value::Integer(_)
            | toml_edit::Value::Float(_)
            | toml_edit::Value::Boolean(_)
            | toml_edit::Value::Array(_)
            | toml_edit::Value::InlineTable(_)),
        ) => value,
        _ => text.into(),
    }
}

pub fn created(
    state: &State,
    draft: &Draft,
    file: &str,
) -> Result<(String, String, Launch), Refusal> {
    let broken = |fault: ConfigFault| {
        Refusal::BrokenConfig(ConfigError {
            file: layer(draft.target),
            line: 1,
            fault,
        })
    };
    let mut document: toml_edit::DocumentMut =
        file.parse().map_err(|_| broken(ConfigFault::NotToml))?;
    let name = draft.name.trim().to_string();
    let taken = document
        .get("launch")
        .and_then(toml_edit::Item::as_table_like)
        .is_some_and(|rows| rows.get(&name).is_some());
    if taken {
        return Err(Refusal::LaunchNameTaken(name));
    }
    let listed = arguments(state, draft);
    let pairs: Vec<(String, String)> = match listed.is_empty() {
        true => draft.free.clone(),
        false => listed
            .into_iter()
            .map(|row| (row.key.clone(), text(draft, &Slot::Argument(row.key))))
            .collect(),
    };
    let mut args = toml_edit::InlineTable::new();
    for (key, value) in pairs {
        if key.trim().is_empty() || value.trim().is_empty() {
            continue;
        }
        args.insert(key.trim(), written(value.trim()));
    }
    let mut row = toml_edit::Table::new();
    row.insert("adapter", toml_edit::value(draft.adapter.as_str()));
    row.insert("request", toml_edit::value(draft.request.as_str()));
    row.insert("args", toml_edit::value(args));
    let mut parent = toml_edit::Table::new();
    parent.set_implicit(true);
    document
        .entry("launch")
        .or_insert(toml_edit::Item::Table(parent))
        .as_table_mut()
        .ok_or_else(|| {
            broken(ConfigFault::WrongType(
                "[launch] is not a table".to_string(),
            ))
        })?
        .insert(&name, toml_edit::Item::Table(row));
    let contents = document.to_string();
    let launch = startup::Config(contents.parse().map_err(|_| broken(ConfigFault::NotToml))?)
        .launches()
        .remove(&name)
        .expect("the row just inserted reads back");
    Ok((contents, name, launch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startup::Adapter;

    fn with(adapter: &str, launch_args: Vec<Argument>) -> State {
        let mut state = State {
            root: PathBuf::from("/w"),
            varde_home: PathBuf::from("/home/me/.varde"),
            ..State::default()
        };
        state.adapters.insert(
            adapter.to_string(),
            Adapter {
                command: format!("{adapter}-adapter"),
                launch_args,
                ..Default::default()
            },
        );
        state
    }

    fn argument(key: &str, required: bool) -> Argument {
        Argument {
            key: key.to_string(),
            explain: format!("what {key} is"),
            required,
        }
    }

    fn filled(state: &State, name: &str, args: &[(&str, &str)]) -> Draft {
        let mut draft = open(state);
        draft.name = name.to_string();
        for (key, value) in args {
            draft.args.insert(key.to_string(), value.to_string());
        }
        draft
    }

    #[test]
    fn writing_into_an_existing_file_keeps_its_comments_and_layout() {
        let state = with("rust", vec![argument("program", true)]);
        let file = "# Servers I chose myself.\n[lsp.rust]\ncommand = \"rust-analyzer\"\nextensions = [\"rs\"]\n";
        let draft = filled(&state, "checkout", &[("program", "target/debug/orders")]);
        let (contents, name, launch) = created(&state, &draft, file).expect("written");
        assert_eq!(name, "checkout");
        assert!(
            contents.starts_with(file),
            "the file now reads:\n{contents}"
        );
        assert_eq!(launch.adapter, "rust");
        assert_eq!(launch.request, "launch");
        assert_eq!(
            launch.args.get("program"),
            Some(&serde_json::json!("target/debug/orders"))
        );
    }

    #[test]
    fn a_name_the_file_already_holds_is_refused() {
        let state = with("rust", vec![argument("program", true)]);
        let file = "[launch.checkout]\nadapter = \"rust\"\nrequest = \"launch\"\nargs = {}\n";
        let draft = filled(&state, "checkout", &[("program", "target/debug/orders")]);
        assert_eq!(
            created(&state, &draft, file),
            Err(Refusal::LaunchNameTaken("checkout".to_string()))
        );
    }

    #[test]
    fn an_argument_that_reads_as_a_number_or_a_list_is_written_as_one() {
        let state = with(
            "rust",
            vec![
                argument("program", true),
                argument("port", false),
                argument("args", false),
                argument("stopOnEntry", false),
            ],
        );
        let draft = filled(
            &state,
            "checkout",
            &[
                ("program", "target/debug/orders"),
                ("port", "5005"),
                ("args", "[\"--exact\"]"),
                ("stopOnEntry", "true"),
            ],
        );
        let (_, _, launch) = created(&state, &draft, "").expect("written");
        assert_eq!(launch.args.get("port"), Some(&serde_json::json!(5005)));
        assert_eq!(
            launch.args.get("args"),
            Some(&serde_json::json!(["--exact"]))
        );
        assert_eq!(
            launch.args.get("stopOnEntry"),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            launch.args.get("program"),
            Some(&serde_json::json!("target/debug/orders"))
        );
    }

    #[test]
    fn a_required_argument_left_empty_is_named_and_an_optional_one_is_not() {
        let state = with(
            "rust",
            vec![argument("program", true), argument("cwd", false)],
        );
        let empty = filled(&state, "checkout", &[]);
        assert_eq!(missing(&state, &empty), Some("program".to_string()));
        let named = filled(&state, "checkout", &[("program", "target/debug/orders")]);
        assert_eq!(missing(&state, &named), None);
        let unnamed = filled(&state, "  ", &[("program", "target/debug/orders")]);
        assert_eq!(missing(&state, &unnamed), Some("name".to_string()));
        let nothing_configured = State::default();
        let draft = Draft {
            name: "checkout".to_string(),
            ..open(&nothing_configured)
        };
        assert_eq!(
            missing(&nothing_configured, &draft),
            Some("adapter".to_string())
        );
    }

    #[test]
    fn a_file_whose_launch_key_is_not_a_table_is_refused() {
        let state = with("rust", vec![argument("program", true)]);
        let draft = filled(&state, "checkout", &[("program", "target/debug/orders")]);
        let refusal = created(&state, &draft, "launch = \"nope\"\n").expect_err("refused");
        assert_eq!(refusal.as_str(), "broken-config");
    }

    #[test]
    fn a_file_that_no_longer_parses_is_refused() {
        let state = with("rust", vec![argument("program", true)]);
        let draft = filled(&state, "checkout", &[("program", "target/debug/orders")]);
        let refusal = created(&state, &draft, "[launch.other\n").expect_err("refused");
        assert_eq!(refusal.as_str(), "broken-config");
    }

    #[test]
    fn a_name_toml_has_to_quote_is_written_and_reads_back_under_that_name() {
        let state = with("rust", vec![argument("program", true)]);
        let draft = filled(
            &state,
            "my app.debug",
            &[("program", "target/debug/orders")],
        );
        let (contents, name, launch) = created(&state, &draft, "").expect("written");
        assert_eq!(name, "my app.debug");
        assert_eq!(launch.adapter, "rust");
        assert!(
            contents.contains("[launch.\"my app.debug\"]"),
            "the file reads:\n{contents}"
        );
    }

    #[test]
    fn a_free_pair_whose_key_is_typed_grows_another_empty_pair() {
        let state = with("ada", vec![]);
        let mut draft = open(&state);
        assert_eq!(draft.free.len(), 1);
        draft.slot = Slot::Key(0);
        typed(&mut draft, "mainUnit".to_string());
        assert_eq!(draft.free.len(), 2);
        assert_eq!(slots(&state, &draft).len(), 3 + 4 + 1);
    }

    #[test]
    fn a_request_whose_fields_the_focused_one_is_not_among_puts_the_focus_back_on_the_name() {
        let state = with("rust", vec![argument("program", true)]);
        let mut draft = open(&state);
        draft.slot = Slot::Argument("program".to_string());
        switch(&state, &mut draft);
        assert_eq!(draft.slot, Slot::Argument("program".to_string()));
        draft.slot = Slot::Request;
        switch(&state, &mut draft);
        assert_eq!(draft.request, Request::Attach);
        assert_eq!(
            slots(&state, &draft),
            vec![
                Slot::Name,
                Slot::Adapter,
                Slot::Request,
                Slot::Key(0),
                Slot::Value(0),
                Slot::Target
            ]
        );
        let mut back = Draft {
            slot: Slot::Argument("program".to_string()),
            request: Request::Attach,
            ..open(&state)
        };
        switch(&state, &mut back);
        assert_eq!(back.slot, Slot::Name);
    }

    fn texts(state: &State, width: u16, height: u16) -> Vec<String> {
        shown(state, width, height)
            .expect("the box is open")
            .1
            .into_iter()
            .map(|(_, row)| row)
            .collect()
    }

    #[test]
    fn a_long_launch_list_follows_its_selection() {
        let mut state = State::default();
        for at in 10..50 {
            state.launches.insert(
                format!("launch-{at}"),
                Launch {
                    adapter: "rust".to_string(),
                    request: "launch".to_string(),
                    args: serde_json::Map::new(),
                    reattach: false,
                },
            );
        }
        state.modal = Modal::Launches { row: 30 };
        let rows = texts(&state, 120, 20);
        assert_eq!(rows.len() + 2, 20);
        assert_eq!(rows.first().map(String::as_str), Some("  launch-25"));
        assert_eq!(rows[15], "> launch-40");
    }

    #[test]
    fn the_form_shows_the_chosen_rows_arguments_with_their_explanations() {
        let mut state = with(
            "rust",
            vec![
                Argument {
                    key: "program".to_string(),
                    explain: "Path to the built executable to run".to_string(),
                    required: true,
                },
                Argument {
                    key: "cwd".to_string(),
                    explain: "Directory the program runs in".to_string(),
                    required: false,
                },
            ],
        );
        state.modal = Modal::NewLaunch(open(&state));
        let rows = texts(&state, 120, 26);
        assert_eq!(rows[0], "> name                   ");
        assert_eq!(rows[1], "  adapter                rust");
        assert_eq!(rows[2], "  request                launch");
        assert_eq!(
            rows[3],
            "  program                   \u{2014} Path to the built executable to run (required)"
        );
        assert_eq!(
            rows[4],
            "  cwd                       \u{2014} Directory the program runs in"
        );
        assert_eq!(rows[5], "  target                 project");
        assert!(
            rows.last().is_some_and(|keys| keys.contains("create")),
            "the box names no keys: {rows:?}"
        );
    }

    #[test]
    fn a_row_wider_than_the_screen_wraps_and_the_box_grows_a_row_for_it() {
        let mut state = with("rust", vec![argument("program", true)]);
        state.modal = Modal::NewLaunch(open(&state));
        let wide = shown(&state, 120, 26).expect("the box is open");
        let narrow = shown(&state, 40, 26).expect("the box is open");
        assert!(narrow.1.iter().all(|(_, row)| row.chars().count() <= 36));
        assert!(narrow.1.len() > wide.1.len());
        assert_eq!(narrow.0.height, narrow.1.len() as u16 + 2);
    }

    #[test]
    fn start_is_dimmed_with_nothing_to_start() {
        let state = State {
            modal: Modal::Launches { row: 0 },
            ..State::default()
        };
        let start = chips(&state)
            .into_iter()
            .find(|chip| chip.name == "start")
            .expect("a start Chip");
        assert_eq!(start.tone, Tone::Dimmed);
    }

    #[test]
    fn a_dimmed_switch_clicked_on_a_text_field_types_no_space_into_it() {
        let mut state = with("rust", vec![argument("program", true)]);
        state.modal = Modal::NewLaunch(open(&state));
        let (area, _) = shown(&state, 120, 26).expect("the box is open");
        let offered = chips(&state);
        let at = offered
            .iter()
            .position(|chip| chip.name == "switch")
            .expect("a switch Chip");
        let labels = layout::chip_labels(&offered, area.width, 0);
        let column = (area.x..area.right())
            .find(|&column| layout::strip_at(area, &labels, column) == Some(at))
            .expect("the Chip on screen");
        assert_eq!(clicked(&state, 120, 26, column, area.y), Some(Vec::new()));
    }

    #[test]
    fn switch_is_lit_only_on_a_field_that_switches() {
        let mut state = with("rust", vec![argument("program", true)]);
        let mut draft = open(&state);
        let tone = |state: &State| {
            chips(state)
                .into_iter()
                .find(|chip| chip.name == "switch")
                .expect("a switch Chip")
                .tone
        };
        state.modal = Modal::NewLaunch(draft.clone());
        assert_eq!(tone(&state), Tone::Dimmed);
        draft.slot = Slot::Request;
        state.modal = Modal::NewLaunch(draft);
        assert_eq!(tone(&state), Tone::Plain);
    }

    #[test]
    fn a_click_off_the_top_border_is_not_a_chip() {
        let state = State {
            modal: Modal::Launches { row: 0 },
            ..State::default()
        };
        let (area, _) = shown(&state, 120, 26).expect("the box is open");
        assert_eq!(clicked(&state, 120, 26, area.right() - 3, area.y + 1), None);
    }
}
