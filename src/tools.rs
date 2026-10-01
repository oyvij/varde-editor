use crate::startup::{self, Config, ConfigError, PROGRAMS};
use crate::{lsp, State};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Server,
    Formatter,
    Adapter,
    Requirement,
    Speech,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Server => "language-servers",
            Kind::Formatter => "formatters",
            Kind::Adapter => "debug-adapters",
            Kind::Requirement => "requirements",
            Kind::Speech => "speech",
        }
    }

    fn section(self) -> Option<&'static str> {
        match self {
            Kind::Server => Some("lsp"),
            Kind::Formatter => Some("formatter"),
            Kind::Adapter => Some("dap"),
            Kind::Requirement => Some("facts"),
            Kind::Speech => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Template,
    Differs,
    Own,
}

#[derive(Debug)]
pub struct ToolRow {
    pub kind: Kind,
    pub name: String,
    pub command: String,
    pub install: Option<String>,
    pub availability: Availability,
    pub origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    Installed,
    Stopped,
    Missing,
    Unmet { needs: String },
    Unpackaged,
    Partial { without: String },
    Available,
    InstallFailed,
    NeedsInstaller { installer: String },
}

impl Availability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Availability::Installed => "installed",
            Availability::Partial { .. } => "partly-working",
            Availability::Stopped => "stopped",
            Availability::Missing => "missing",
            Availability::Unmet { .. } => "missing-requirement",
            Availability::Unpackaged => "no-install-command",
            Availability::Available => "available",
            Availability::InstallFailed => "install-failed",
            Availability::NeedsInstaller { .. } => "needs-installer",
        }
    }
}

pub fn rows(state: &State) -> Vec<ToolRow> {
    let template = Config(PROGRAMS.parse().expect("the template parses"));
    let on_path = |command: &str| state.commands_on_path.contains(command);
    let mut rows = group(
        Kind::Server,
        &state.servers,
        template.servers(),
        |server| server.command.clone(),
        |server| match on_path(&server.command) {
            true => match lsp::unmet(state, server) {
                Some(needs) => state
                    .facts
                    .get(&needs)
                    .and_then(|fact| fact.install.get(&state.os).cloned()),
                None => server.install.get(&state.os).cloned(),
            },
            false => server.install.get(&state.os).cloned(),
        },
        |language, server| match on_path(&server.command) {
            true => match lsp::unmet(state, server) {
                Some(needs) => Availability::Unmet { needs },
                None => match (lsp::written_off(state, language), &server.partial) {
                    (true, _) => Availability::Stopped,
                    (false, Some(without)) => Availability::Partial {
                        without: without.clone(),
                    },
                    (false, None) => Availability::Installed,
                },
            },
            false => absent(server.install.get(&state.os)),
        },
    );
    rows.extend(group(
        Kind::Formatter,
        &state.formatters,
        template.formatters(),
        |formatter| formatter.command.clone(),
        |formatter| formatter.install.get(&state.os).cloned(),
        |_, formatter| match on_path(&formatter.command) {
            true => Availability::Installed,
            false => absent(formatter.install.get(&state.os)),
        },
    ));
    let program = |adapter: &crate::startup::Adapter| match &adapter.server {
        Some(server) => state
            .servers
            .get(server)
            .map_or_else(|| server.clone(), |server| server.command.clone()),
        None => adapter.command.clone(),
    };
    rows.extend(group(
        Kind::Adapter,
        &state.adapters,
        template.adapters(),
        program,
        |adapter| adapter.install.get(&state.os).cloned(),
        |_, adapter| match on_path(&program(adapter)) {
            true => Availability::Installed,
            false => absent(adapter.install.get(&state.os)),
        },
    ));
    rows.extend(group(
        Kind::Requirement,
        &state.facts,
        template.facts(),
        |fact| fact.command.clone().unwrap_or_default(),
        |fact| fact.install.get(&state.os).cloned(),
        |name, _| match state.workspace_facts.contains_key(name) {
            true => Availability::Installed,
            false => Availability::Unmet {
                needs: name.to_string(),
            },
        },
    ));
    let speech = &state.speech;
    let shipped = startup::speech(&template, &state.os);
    let given = |install: &String| (!install.is_empty()).then(|| install.clone());
    let synthesizer = match speech.command.is_empty() {
        true => (
            shipped.command.clone(),
            given(&shipped.install),
            Availability::Available,
            Origin::Template,
        ),
        false => (
            speech.command.clone(),
            given(&speech.install),
            match on_path(&speech.command) && state.voice_installed {
                true => Availability::Installed,
                false => absent(given(&speech.install).as_ref()),
            },
            match (&speech.command, &speech.args, &speech.install)
                == (&shipped.command, &shipped.args, &shipped.install)
            {
                true => Origin::Template,
                false => Origin::Differs,
            },
        ),
    };
    let player = match speech.player.is_empty() {
        true => (
            shipped.player.clone(),
            None,
            Availability::Available,
            Origin::Template,
        ),
        false => (
            speech.player.clone(),
            None,
            match on_path(&speech.player) {
                true => Availability::Installed,
                false => Availability::Unpackaged,
            },
            match speech.player == shipped.player {
                true => Origin::Template,
                false => Origin::Differs,
            },
        ),
    };
    rows.extend(
        [("synthesizer", synthesizer), ("player", player)]
            .into_iter()
            .filter(|(_, (command, _, _, _))| !command.is_empty())
            .map(|(name, (command, install, availability, origin))| ToolRow {
                kind: Kind::Speech,
                name: name.to_string(),
                command,
                install,
                availability,
                origin,
            }),
    );
    for row in &mut rows {
        if state.install_failed.contains(&(row.kind, row.name.clone())) {
            row.availability = Availability::InstallFailed;
        }
    }
    for row in &mut rows {
        if matches!(
            row.availability,
            Availability::Missing | Availability::Available | Availability::InstallFailed
        ) && !on_path(&row.command)
        {
            if let Some(installer) = row
                .install
                .as_deref()
                .and_then(installer)
                .filter(|installer| !on_path(installer))
            {
                row.availability = Availability::NeedsInstaller { installer };
            }
        }
    }
    rows
}

fn absent(install: Option<&String>) -> Availability {
    match install {
        Some(_) => Availability::Missing,
        None => Availability::Unpackaged,
    }
}

pub fn installer(install: &str) -> Option<String> {
    let mut words = shlex::Shlex::new(install);
    match words.next()? {
        sudo if sudo == "sudo" => words.next(),
        first => Some(first),
    }
}

fn group<T: PartialEq>(
    kind: Kind,
    configured: &BTreeMap<String, T>,
    template: BTreeMap<String, T>,
    command: impl Fn(&T) -> String,
    install: impl Fn(&T) -> Option<String>,
    status: impl Fn(&str, &T) -> Availability,
) -> Vec<ToolRow> {
    let mut rows: Vec<ToolRow> = configured
        .iter()
        .map(|(name, row)| ToolRow {
            kind,
            name: name.clone(),
            command: command(row),
            install: install(row),
            availability: status(name, row),
            origin: match template.get(name) {
                Some(shipped) if shipped == row => Origin::Template,
                Some(_) => Origin::Differs,
                None => Origin::Own,
            },
        })
        .collect();
    rows.extend(
        template
            .iter()
            .filter(|(name, _)| !configured.contains_key(*name))
            .map(|(name, row)| ToolRow {
                kind,
                name: name.clone(),
                command: command(row),
                install: install(row),
                availability: Availability::Available,
                origin: Origin::Template,
            }),
    );
    rows
}

pub const SENTINEL: &str = "install-done";

pub fn reported(install: &str, sentinel: &Path) -> String {
    let quoted = |path: &Path| {
        shlex::try_quote(&path.to_string_lossy())
            .expect("no NUL in a path")
            .into_owned()
    };
    let writing = quoted(&sentinel.with_extension("writing"));
    let sentinel = quoted(sentinel);
    format!("rm -f {sentinel}; {install}; echo $? > {writing}; mv {writing} {sentinel}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Write {
    Row,
    Configures,
}

pub fn configure(global: &str) -> Result<Option<(String, Config)>, ConfigError> {
    startup::merged_config(Some(global), None)?;
    let mut file: toml_edit::DocumentMut =
        global.parse().expect("the file the merge just read parses");
    let Some(rows) = file
        .get_mut("speech")
        .and_then(toml_edit::Item::as_table_like_mut)
    else {
        return Ok(None);
    };
    let configures: Vec<(String, String)> = rows
        .get("configures")
        .and_then(toml_edit::Item::as_table_like)
        .into_iter()
        .flat_map(|keys| keys.iter())
        .filter_map(|(key, value)| Some((key.to_string(), value.as_str()?.to_string())))
        .collect();
    let mut wrote = false;
    for (key, value) in configures {
        let key = key.as_str();
        if rows.get(key).is_some_and(|set| set.as_str() != Some("")) {
            continue;
        }
        match rows.get_mut(key).and_then(toml_edit::Item::as_value_mut) {
            Some(blank) => {
                let decor = blank.decor().clone();
                *blank = value.as_str().into();
                *blank.decor_mut() = decor;
            }
            None => {
                rows.insert(key, toml_edit::value(value));
            }
        }
        wrote = true;
    }
    if !wrote {
        return Ok(None);
    }
    let text = file.to_string();
    let config = Config(startup::merged_config(Some(&text), None)?);
    Ok(Some((text, config)))
}

pub fn take(global: &str, kind: Kind, name: &str) -> Result<Option<(String, Config)>, ConfigError> {
    let has = startup::merged_config(Some(global), None)?;
    let Some(section) = kind.section() else {
        return Ok(None);
    };
    let template: toml_edit::DocumentMut = PROGRAMS.parse().expect("the template parses");
    let Some(row) = template.get(section).and_then(|rows| rows.get(name)) else {
        return Ok(None);
    };
    let lacks = |section: &str, name: &str| {
        !has.get(section)
            .and_then(toml::Value::as_table)
            .is_some_and(|rows| rows.contains_key(name))
    };
    if !lacks(section, name) {
        return Ok(None);
    }
    let mut appended = toml_edit::DocumentMut::new();
    let append =
        |to: &mut toml_edit::DocumentMut, section: &str, name: &str, row: &toml_edit::Item| {
            let mut parent = toml_edit::Table::new();
            parent.set_implicit(true);
            to.entry(section)
                .or_insert(toml_edit::Item::Table(parent))
                .as_table_mut()
                .expect("a section is a table")
                .insert(name, row.clone());
        };
    append(&mut appended, section, name, row);
    // Rendered, not read off the item: toml_edit leaves an item's sub-tables out of its own text
    let named = appended.to_string();
    for (fact, row) in template
        .get("facts")
        .and_then(toml_edit::Item::as_table)
        .into_iter()
        .flatten()
    {
        if named.contains(&format!("${{{fact}}}")) && lacks("facts", fact) {
            append(&mut appended, "facts", fact, row);
        }
    }
    let mut text = global.to_string();
    if !text.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push('\n');
    }
    text.push_str(appended.to_string().trim_start());
    let config = Config(startup::merged_config(Some(&text), None)?);
    Ok(Some((text, config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(state: &State, kind: Kind, name: &str) -> ToolRow {
        rows(state)
            .into_iter()
            .find(|row| row.kind == kind && row.name == name)
            .unwrap_or_else(|| panic!("no {} row {name}", kind.as_str()))
    }

    fn speaking() -> State {
        let mut state = State {
            os: "linux".to_string(),
            ..State::default()
        };
        state.speech.command = "piper".to_string();
        state.speech.install = "install-piper".to_string();
        state.speech.player = "aplay".to_string();
        state.commands_on_path.insert("piper".to_string());
        state
    }

    #[test]
    fn a_synthesizer_with_no_voice_is_missing_its_install() {
        let mut state = speaking();
        assert_eq!(
            row(&state, Kind::Speech, "synthesizer").availability,
            Availability::Missing
        );
        assert_eq!(
            row(&state, Kind::Speech, "synthesizer").install.as_deref(),
            Some("install-piper")
        );
        state.voice_installed = true;
        assert_eq!(
            row(&state, Kind::Speech, "synthesizer").availability,
            Availability::Installed
        );
    }

    #[test]
    fn a_player_reads_off_the_path_and_offers_no_install() {
        let mut state = speaking();
        assert_eq!(
            row(&state, Kind::Speech, "player").availability,
            Availability::Unpackaged
        );
        state.commands_on_path.insert("aplay".to_string());
        assert_eq!(
            row(&state, Kind::Speech, "player").availability,
            Availability::Installed
        );
    }

    #[test]
    fn a_requirement_is_installed_when_the_edge_found_it() {
        let mut state = State::default();
        let template = Config(PROGRAMS.parse().expect("the template parses"));
        state.facts = template.facts();
        state.commands_on_path.insert("tsc".to_string());
        assert_eq!(
            row(&state, Kind::Requirement, "typescript_sdk").availability,
            Availability::Unmet {
                needs: "typescript_sdk".to_string()
            }
        );
        state
            .workspace_facts
            .insert("typescript_sdk".to_string(), "/sdk".to_string());
        assert_eq!(
            row(&state, Kind::Requirement, "typescript_sdk").availability,
            Availability::Installed
        );
    }

    #[test]
    fn the_typescript_sdk_installs_everywhere() {
        let template = Config(PROGRAMS.parse().expect("the template parses"));
        let sdk = &template.facts()["typescript_sdk"];
        let servers = template.servers();
        for os in ["macos", "linux", "windows"] {
            assert_eq!(
                sdk.install.get(os).map(String::as_str),
                Some("npm install -g typescript@6")
            );
            for language in ["typescript", "javascript"] {
                assert_eq!(
                    servers[language].install.get(os).map(String::as_str),
                    Some("npm install -g typescript@6 typescript-language-server")
                );
            }
        }
    }

    #[test]
    fn taking_a_row_keeps_the_file_byte_for_byte_and_brings_its_requirement() {
        let global = "# mine, and hand-aligned\n[lsp.rust]\ncommand   = \"ra\"  # pinned\nextensions = [\"rs\"]";
        let (text, config) = take(global, Kind::Server, "vue")
            .expect("parses")
            .expect("the file lacks vue");
        assert!(text.starts_with(&format!("{global}\n\n")), "{text}");
        assert!(config.servers().contains_key("vue"));
        assert!(config.facts().contains_key("typescript_sdk"));
        assert_eq!(config.servers()["rust"].command, "ra");
        assert!(text.ends_with('\n'), "{text}");
    }

    #[test]
    fn taking_writes_only_what_the_file_lacks() {
        let (with_fact, _) = take("", Kind::Requirement, "typescript_sdk")
            .expect("parses")
            .expect("an empty file lacks it");
        let (text, _) = take(&with_fact, Kind::Server, "vue")
            .expect("parses")
            .expect("the file lacks vue");
        assert_eq!(text.matches("[facts.typescript_sdk]").count(), 1, "{text}");
        assert!(take(&text, Kind::Server, "vue").expect("parses").is_none());
    }

    #[test]
    fn a_row_with_sub_tables_is_appended_whole() {
        let global = "[lsp.rust]\ncommand = \"ra\"\nextensions = [\"rs\"]\n\n[formatter.rust]\ncommand = \"rustfmt\"\nextensions = [\"rs\"]\n";
        let (text, config) = take(global, Kind::Server, "typescript")
            .expect("parses")
            .expect("the file lacks typescript");
        assert!(text.starts_with(global), "{text}");
        assert!(config.facts().contains_key("typescript_sdk"), "{text}");
        assert!(
            config.facts().contains_key("vue_typescript_plugin"),
            "{text}"
        );
        let template = Config(PROGRAMS.parse().expect("the template parses"));
        assert_eq!(
            config.servers()["typescript"],
            template.servers()["typescript"]
        );
    }

    #[test]
    fn an_append_that_would_break_the_file_is_refused() {
        let global = "[lsp.mine]\ncommand = \"mine\"\nextensions = [\"go\"]\n";
        assert!(matches!(
            take(global, Kind::Server, "go"),
            Err(ConfigError {
                fault: startup::ConfigFault::ClaimedTwice { .. },
                ..
            })
        ));
    }

    #[test]
    fn configuring_fills_a_blank_in_place_and_keeps_the_rest() {
        let global = "# mine\n[speech]\ncommand = \"piper\"  # pinned\nvoice = \"\"   # blank\nconfigures.voice = \"~/.varde/voices/v.onnx\"\n\n[lsp.rust]\ncommand = \"ra\"\nextensions = [\"rs\"]\n";
        let (text, config) = configure(global)
            .expect("parses")
            .expect("the voice is blank");
        assert_eq!(
            text,
            global.replace("voice = \"\"", "voice = \"~/.varde/voices/v.onnx\"")
        );
        assert_eq!(
            config.get("speech.voice").as_deref(),
            Some("~/.varde/voices/v.onnx")
        );
    }

    #[test]
    fn configuring_adds_what_is_absent_and_never_beats_the_reader() {
        let absent = "[speech]\ncommand = \"piper\"\nconfigures.voice = \"~/v\"\n";
        let (text, _) = configure(absent)
            .expect("parses")
            .expect("the voice is absent");
        assert_eq!(text, format!("{absent}voice = \"~/v\"\n"));
        let chosen = "[speech]\nvoice = \"/mine.onnx\"\nconfigures.voice = \"~/v\"\n";
        assert!(configure(chosen).expect("parses").is_none());
        let unsaid = "[speech]\nvoice = \"\"\n";
        assert!(configure(unsaid).expect("parses").is_none());
    }

    #[test]
    fn configuring_a_file_that_does_not_parse_is_refused() {
        assert!(configure("[speech").is_err());
    }

    #[test]
    fn the_installer_is_the_first_word_or_the_one_after_sudo() {
        assert_eq!(installer("sudo apt install clangd").as_deref(), Some("apt"));
        assert_eq!(installer("npm install -g pyright").as_deref(), Some("npm"));
        assert_eq!(installer("").as_deref(), None);
    }

    #[test]
    fn only_a_command_that_is_not_here_needs_its_installer() {
        let mut state = State {
            os: "linux".to_string(),
            ..State::default()
        };
        state.commands_on_path.insert("gopls".to_string());
        assert_eq!(
            row(&state, Kind::Server, "go").availability,
            Availability::Available
        );
        state.commands_on_path.clear();
        state
            .install_failed
            .insert((Kind::Server, "go".to_string()));
        assert_eq!(
            row(&state, Kind::Server, "go").availability,
            Availability::NeedsInstaller {
                installer: "go".to_string()
            }
        );
    }

    #[test]
    fn a_row_the_template_does_not_know_is_the_readers_own() {
        let mut state = State::default();
        state.formatters.insert(
            "ruby".to_string(),
            startup::Formatter {
                command: "rubocop".to_string(),
                args: vec![],
                install: BTreeMap::new(),
                extensions: vec!["rb".to_string()],
            },
        );
        assert_eq!(row(&state, Kind::Formatter, "ruby").origin, Origin::Own);
    }
}
