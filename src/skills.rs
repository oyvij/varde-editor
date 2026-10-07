use pulldown_cmark::{Event, MetadataBlockKind, Options, Parser, Tag};
use std::path::Path;
use yaml_rust::YamlLoader;

pub const FOLDER: &str = "ai/skills";

pub const FILE: &str = "SKILL.md";

pub const NO_FRONTMATTER: &str = "no-frontmatter";

pub const SOURCE_MAP: &str = "source-map.md";

pub const SHIPPED: [(&str, &str); 3] = [
    (
        "ai/skills/update-knowledge/SKILL.md",
        include_str!("../ai/skills/update-knowledge/SKILL.md"),
    ),
    (
        "ai/skills/search-knowledge/SKILL.md",
        include_str!("../ai/skills/search-knowledge/SKILL.md"),
    ),
    (
        "ai/agents/knowledge-searcher.md",
        include_str!("../ai/agents/knowledge-searcher.md"),
    ),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub folder: String,
    pub read: Result<Frontmatter, &'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    pub vault: bool,
    pub asks: Option<String>,
}

pub fn read(folder: &str, text: &str) -> Skill {
    Skill {
        folder: folder.to_string(),
        read: frontmatter(text).ok_or(NO_FRONTMATTER),
    }
}

fn frontmatter(text: &str) -> Option<Frontmatter> {
    let mut events = Parser::new_ext(text, Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    let Some(Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle))) = events.next() else {
        return None;
    };
    let Some(Event::Text(yaml)) = events.next() else {
        return None;
    };
    let documents = YamlLoader::load_from_str(&yaml).ok()?;
    let document = documents.first()?;
    Some(Frontmatter {
        name: document["name"].as_str()?.to_string(),
        description: document["description"].as_str()?.to_string(),
        vault: document["metadata"]["varde-vault"].as_bool() == Some(true),
        asks: document["metadata"]["varde-asks"]
            .as_str()
            .map(str::to_string),
    })
}

pub fn pointer(skill: &Path, workspace: &Path, knowledge: Option<(&Path, &Path)>) -> String {
    let line = format!(
        "Follow the Skill in {} for the workspace at {}.",
        skill.display(),
        workspace.display()
    );
    match knowledge {
        Some((vault, source_map)) => format!(
            "{line} The Vault is {} and the Source map is {}.",
            vault.display(),
            source_map.display()
        ),
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skill_is_its_frontmatters_name_and_description() {
        let skill = read(
            "standup",
            "---\nname: standup\ndescription: >\n  Summarise today's work\n  as a standup\n---\nWrite it.\n",
        );
        assert_eq!(
            skill.read,
            Ok(Frontmatter {
                name: "standup".to_string(),
                description: "Summarise today's work as a standup\n".to_string(),
                vault: false,
                asks: None,
            })
        );
    }

    #[test]
    fn a_skill_opts_into_the_vault_through_its_metadata() {
        let opted = |metadata: &str| {
            read(
                "keep",
                &format!("---\nname: keep\ndescription: Keep it\n{metadata}---\n"),
            )
            .read
            .map(|front| front.vault)
        };
        assert_eq!(opted("metadata:\n  varde-vault: true\n"), Ok(true));
        assert_eq!(opted("metadata: { varde-vault: false }\n"), Ok(false));
        assert_eq!(opted("metadata:\n  varde-vault: \"true\"\n"), Ok(false));
        assert_eq!(opted("varde-vault: true\n"), Ok(false));
        assert_eq!(opted(""), Ok(false));
    }

    #[test]
    fn a_skill_asks_the_question_its_metadata_names() {
        let asks = |metadata: &str| {
            read(
                "search",
                &format!("---\nname: search\ndescription: Search it\n{metadata}---\n"),
            )
            .read
            .map(|front| front.asks)
        };
        assert_eq!(
            asks("metadata:\n  varde-asks: \"What are you looking for?\"\n"),
            Ok(Some("What are you looking for?".to_string()))
        );
        assert_eq!(asks("metadata: { varde-asks: true }\n"), Ok(None));
        assert_eq!(asks("varde-asks: Question\n"), Ok(None));
        assert_eq!(asks(""), Ok(None));
    }

    #[test]
    fn only_a_pointer_given_the_knowledge_names_the_vault_and_the_source_map() {
        let skill = Path::new("/h/.varde/ai/skills/keep/SKILL.md");
        let workspace = Path::new("/h/work");
        let vault = Path::new("/h/brain");
        let source_map = Path::new("/h/.varde/source-map.md");
        let told = pointer(skill, workspace, Some((vault, source_map)));
        assert!(told.contains("/h/brain") && told.contains("/h/.varde/source-map.md"));
        let untold = pointer(skill, workspace, None);
        assert!(!untold.contains("/h/brain") && !untold.contains("source-map"));
        assert!(untold.contains("/h/.varde/ai/skills/keep/SKILL.md") && untold.contains("/h/work"));
    }

    #[test]
    fn every_shipped_file_opens_by_saying_it_is_replaced_on_update() {
        for (path, text) in SHIPPED {
            let first = text.lines().find(|line| *line != "---").expect("a line");
            assert!(
                first.contains("replaced on every update"),
                "{path} opens with {first:?}"
            );
        }
    }

    #[test]
    fn prose_without_frontmatter_is_unreadable() {
        assert_eq!(
            read("broken", "Just prose, no frontmatter.").read,
            Err(NO_FRONTMATTER)
        );
    }

    #[test]
    fn frontmatter_without_a_description_is_unreadable() {
        assert_eq!(
            read("half", "---\nname: half\n---\n").read,
            Err(NO_FRONTMATTER)
        );
    }

    #[test]
    fn frontmatter_that_is_not_yaml_is_unreadable() {
        assert_eq!(
            read("bad", "---\nname: [unclosed\n---\n").read,
            Err(NO_FRONTMATTER)
        );
    }
}
