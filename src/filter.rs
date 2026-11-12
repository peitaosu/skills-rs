use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use crate::SkillScope;
use crate::model::SkillEntry;

pub trait SkillFilter {
    fn include_in_catalog(&self, entry: &SkillEntry) -> bool;
    fn allow_activation(&self, entry: &SkillEntry) -> bool;
}

pub struct AllowAll;

impl SkillFilter for AllowAll {
    fn include_in_catalog(&self, _entry: &SkillEntry) -> bool {
        true
    }

    fn allow_activation(&self, _entry: &SkillEntry) -> bool {
        true
    }
}

pub struct HideModelDisabled;

impl SkillFilter for HideModelDisabled {
    fn include_in_catalog(&self, entry: &SkillEntry) -> bool {
        !entry.disables_model_invocation()
    }

    fn allow_activation(&self, entry: &SkillEntry) -> bool {
        !entry.disables_model_invocation()
    }
}

pub struct DisabledSkillsFilter {
    disabled: HashSet<String>,
}

impl DisabledSkillsFilter {
    pub fn new(disabled: impl IntoIterator<Item = String>) -> Self {
        Self {
            disabled: disabled.into_iter().collect(),
        }
    }
}

impl SkillFilter for DisabledSkillsFilter {
    fn include_in_catalog(&self, entry: &SkillEntry) -> bool {
        !self.disabled.contains(&entry.properties.name)
    }

    fn allow_activation(&self, entry: &SkillEntry) -> bool {
        !self.disabled.contains(&entry.properties.name)
    }
}

pub struct PermissionDeniedFilter {
    denied: HashSet<String>,
}

impl PermissionDeniedFilter {
    pub fn new(denied: impl IntoIterator<Item = String>) -> Self {
        Self {
            denied: denied.into_iter().collect(),
        }
    }
}

impl SkillFilter for PermissionDeniedFilter {
    fn include_in_catalog(&self, entry: &SkillEntry) -> bool {
        !self.denied.contains(&entry.properties.name)
    }

    fn allow_activation(&self, entry: &SkillEntry) -> bool {
        !self.denied.contains(&entry.properties.name)
    }
}

pub struct CompositeFilter {
    filters: Vec<Box<dyn SkillFilter>>,
}

impl CompositeFilter {
    pub fn new(filters: Vec<Box<dyn SkillFilter>>) -> Self {
        Self { filters }
    }

    pub fn catalog_default() -> Self {
        Self::new(vec![Box::new(HideModelDisabled)])
    }

    pub fn host_default(
        trusted_project_roots: impl IntoIterator<Item = PathBuf>,
        disabled: impl IntoIterator<Item = String>,
        denied: impl IntoIterator<Item = String>,
    ) -> Self {
        Self::new(vec![
            Box::new(HideModelDisabled),
            Box::new(TrustedRootsFilter::new(trusted_project_roots)),
            Box::new(DisabledSkillsFilter::new(disabled)),
            Box::new(PermissionDeniedFilter::new(denied)),
        ])
    }
}

impl SkillFilter for CompositeFilter {
    fn include_in_catalog(&self, entry: &SkillEntry) -> bool {
        self.filters
            .iter()
            .all(|filter| filter.include_in_catalog(entry))
    }

    fn allow_activation(&self, entry: &SkillEntry) -> bool {
        self.filters
            .iter()
            .all(|filter| filter.allow_activation(entry))
    }
}

pub struct TrustedRootsFilter {
    trusted_project_roots: BTreeSet<PathBuf>,
}

impl TrustedRootsFilter {
    pub fn new(trusted_project_roots: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            trusted_project_roots: trusted_project_roots.into_iter().collect(),
        }
    }

    fn is_trusted(&self, entry: &SkillEntry) -> bool {
        if entry.scope != SkillScope::Project {
            return true;
        }

        self.trusted_project_roots
            .iter()
            .any(|root| entry.skill_dir.starts_with(root))
    }
}

impl SkillFilter for TrustedRootsFilter {
    fn include_in_catalog(&self, entry: &SkillEntry) -> bool {
        self.is_trusted(entry)
    }

    fn allow_activation(&self, entry: &SkillEntry) -> bool {
        self.is_trusted(entry)
    }
}
