use std::collections::HashSet;

use crate::Result;
use crate::activate::SkillActivator;
use crate::filter::SkillFilter;
use crate::model::{ActivatedSkill, ActivationOptions, SkillEntry};

pub struct SkillSession {
    activated: HashSet<String>,
}

impl Default for SkillSession {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillSession {
    pub fn new() -> Self {
        Self {
            activated: HashSet::new(),
        }
    }

    pub fn is_active(&self, name: &str) -> bool {
        self.activated.contains(name)
    }

    pub fn active_names(&self) -> impl Iterator<Item = &str> {
        self.activated.iter().map(String::as_str)
    }

    pub fn protection_tags(&self) -> Vec<String> {
        self.activated
            .iter()
            .map(|name| format!("skill_content name=\"{name}\""))
            .collect()
    }

    pub fn try_activate(
        &mut self,
        entry: &SkillEntry,
        options: ActivationOptions,
        filter: &dyn SkillFilter,
    ) -> Result<Option<ActivatedSkill>> {
        if !filter.allow_activation(entry) {
            return Err(crate::SkillError::Other {
                message: format!("Activation denied for skill '{}'", entry.properties.name),
            });
        }

        if self.activated.contains(&entry.properties.name) {
            return Ok(None);
        }

        let activated = SkillActivator::activate(entry, options)?;
        self.activated.insert(entry.properties.name.clone());
        Ok(Some(activated))
    }

    pub fn force_activate(
        &mut self,
        entry: &SkillEntry,
        options: ActivationOptions,
    ) -> Result<ActivatedSkill> {
        let activated = SkillActivator::activate(entry, options)?;
        self.activated.insert(entry.properties.name.clone());
        Ok(activated)
    }

    pub fn reset(&mut self) {
        self.activated.clear();
    }
}
