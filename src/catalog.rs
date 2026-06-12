use std::collections::BTreeMap;

use crate::model::{ActivationMode, SkillEntry};
use crate::prompt::to_prompt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CatalogPlacement {
    #[default]
    SystemPrompt,
    ToolDescription,
}

pub struct SkillCatalog {
    skills: BTreeMap<String, SkillEntry>,
}

impl SkillCatalog {
    pub fn from_entries(entries: impl IntoIterator<Item = SkillEntry>) -> Self {
        let mut skills = BTreeMap::new();
        for entry in entries {
            skills.insert(entry.properties.name.clone(), entry);
        }
        Self { skills }
    }

    pub fn from_entries_filtered(
        entries: impl IntoIterator<Item = SkillEntry>,
        filter: &dyn crate::filter::SkillFilter,
    ) -> Self {
        Self::from_entries(
            entries
                .into_iter()
                .filter(|entry| filter.include_in_catalog(entry)),
        )
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn get(&self, name: &str) -> Option<&SkillEntry> {
        self.skills.get(name)
    }

    pub fn entries(&self) -> impl Iterator<Item = &SkillEntry> {
        self.skills.values()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.skills.keys().map(String::as_str)
    }

    pub fn to_prompt_xml(&self) -> Option<String> {
        if self.skills.is_empty() {
            return None;
        }

        let entries: Vec<_> = self.skills.values().cloned().collect();
        to_prompt(&entries).ok()
    }

    pub fn behavioral_instructions(&self, mode: ActivationMode) -> Option<String> {
        if self.skills.is_empty() {
            return None;
        }

        Some(match mode {
            ActivationMode::FileRead => {
                "The following skills provide specialized instructions for specific tasks.\n\
                 When a task matches a skill's description, use your file-read tool to load \
                 the SKILL.md at the listed location before proceeding.\n\
                 When a skill references relative paths, resolve them against the skill's \
                 directory (the parent of SKILL.md) and use absolute paths in tool calls."
                    .to_owned()
            }
            ActivationMode::DedicatedTool { tool_name } => format!(
                "The following skills provide specialized instructions for specific tasks.\n\
                 When a task matches a skill's description, call the {tool_name} tool \
                 with the skill's name to load its full instructions."
            ),
        })
    }

    pub fn disclosure_block(&self, mode: ActivationMode) -> Option<String> {
        let catalog = self.to_prompt_xml()?;
        let instructions = self.behavioral_instructions(mode)?;
        Some(format!("{instructions}\n\n{catalog}"))
    }

    pub fn system_prompt_block(
        &self,
        mode: ActivationMode,
        placement: CatalogPlacement,
    ) -> Option<String> {
        if self.skills.is_empty() {
            return None;
        }

        match placement {
            CatalogPlacement::SystemPrompt => self.disclosure_block(mode),
            CatalogPlacement::ToolDescription => self.behavioral_instructions(mode),
        }
    }

    pub fn tool_description_block(
        &self,
        tool_name: &str,
        placement: CatalogPlacement,
        auxiliary_tools: &[&str],
    ) -> Option<String> {
        if self.skills.is_empty() {
            return None;
        }

        let names: Vec<_> = self.names().collect();
        let mut block = format!(
            "{tool_name}(name: enum{})\n",
            serde_json::to_string(&names).unwrap_or_default()
        );

        for auxiliary_tool in auxiliary_tools {
            block.push_str(auxiliary_tool);
            block.push('\n');
        }

        if placement == CatalogPlacement::ToolDescription
            && let Some(catalog) = self.to_prompt_xml()
        {
            block.push('\n');
            block.push_str(&catalog);
        }

        Some(block)
    }
}
