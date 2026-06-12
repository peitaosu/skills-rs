use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use skills_rs::CatalogPlacement;

#[derive(Debug, Clone)]
pub struct HostSettings {
    pub project_root: PathBuf,
    pub trusted: bool,
    pub disabled_skills: HashSet<String>,
    pub denied_skills: HashSet<String>,
    pub require_activation_consent: bool,
    pub file_read_mode: bool,
    pub catalog_placement: CatalogPlacement,
    pub watch_reload: bool,
    pub compact_keep_unprotected: usize,
    pub compact_max_unprotected_tokens: usize,
    pub activation_log: VecDeque<String>,
}

impl Default for HostSettings {
    fn default() -> Self {
        Self {
            project_root: PathBuf::from("."),
            trusted: false,
            disabled_skills: HashSet::new(),
            denied_skills: HashSet::new(),
            require_activation_consent: true,
            file_read_mode: false,
            catalog_placement: CatalogPlacement::default(),
            watch_reload: false,
            compact_keep_unprotected: 5,
            compact_max_unprotected_tokens: 2_000,
            activation_log: VecDeque::new(),
        }
    }
}

impl HostSettings {
    pub fn trust_roots(&self) -> Vec<PathBuf> {
        if self.trusted {
            vec![self.project_root.clone()]
        } else {
            Vec::new()
        }
    }

    pub fn record_activation(&mut self, skill_name: &str) {
        self.activation_log
            .push_back(format!("activated {skill_name}"));
        while self.activation_log.len() > 50 {
            self.activation_log.pop_front();
        }
    }
}
