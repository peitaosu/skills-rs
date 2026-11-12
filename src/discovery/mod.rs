mod archive;
mod gitignore;
mod paths;
mod remote;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

pub use archive::{resolve_skill_dir_after_unpack, unpack_skill_archive};
pub use paths::{ancestor_directories, find_git_root, home_dir, xdg_skills_dir};
pub use remote::{RegistryManifest, RegistrySkillRef, RemoteRegistrySource};

use self::gitignore::GitignoreMatcher;
use crate::model::{CollisionWarning, Diagnostic, Severity, SkillEntry, SkillScope};
use crate::parse::{find_skill_md, parse_frontmatter, read_skill_md_content};
use crate::validate::{is_runtime_loadable, validate_lenient};
use crate::{Result, SkillDocument};

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub project_roots: Vec<PathBuf>,
    pub user_roots: Vec<PathBuf>,
    pub organization_roots: Vec<PathBuf>,
    pub bundled_roots: Vec<PathBuf>,
    pub extra_roots: Vec<PathBuf>,
    pub client_name: Option<String>,
    pub include_agents_convention: bool,
    pub include_claude_compat: bool,
    pub include_ancestor_scan: bool,
    pub include_xdg: bool,
    pub max_depth: usize,
    pub max_directories: usize,
    pub skip_dir_names: HashSet<String>,
    pub respect_gitignore: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        let mut skip_dir_names = HashSet::from([".git".to_owned(), "node_modules".to_owned()]);
        skip_dir_names.extend(
            ["target", ".venv", "dist", "build"]
                .into_iter()
                .map(str::to_owned),
        );

        Self {
            project_roots: Vec::new(),
            user_roots: Vec::new(),
            organization_roots: Vec::new(),
            bundled_roots: Vec::new(),
            extra_roots: Vec::new(),
            client_name: None,
            include_agents_convention: true,
            include_claude_compat: false,
            include_ancestor_scan: false,
            include_xdg: false,
            max_depth: 6,
            max_directories: 2000,
            skip_dir_names,
            respect_gitignore: false,
        }
    }
}

impl DiscoveryConfig {
    pub fn with_project(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_roots: vec![project_root.into()],
            ..Default::default()
        }
    }

    pub fn with_user_home(home: impl Into<PathBuf>) -> Self {
        Self {
            user_roots: vec![home.into()],
            ..Default::default()
        }
    }

    pub fn expanded_user_roots(&self) -> Vec<PathBuf> {
        let mut roots = self.user_roots.clone();
        if self.include_xdg
            && let Some(xdg) = xdg_skills_dir()
        {
            roots.push(xdg);
        }
        roots.extend(self.extra_roots.iter().cloned());
        roots
    }

    pub fn expanded_project_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        for project_root in &self.project_roots {
            roots.push(project_root.clone());
            if self.include_ancestor_scan {
                for ancestor in ancestor_directories(project_root) {
                    if ancestor != *project_root {
                        roots.push(ancestor);
                    }
                }
            }
        }
        roots
    }
}

pub struct DiscoveryResult {
    pub entries: Vec<SkillEntry>,
    pub diagnostics: Vec<Diagnostic>,
}

pub trait SkillSource {
    fn discover(&self) -> Result<DiscoveryResult>;
}

pub struct FilesystemDiscovery {
    config: DiscoveryConfig,
}

impl FilesystemDiscovery {
    pub fn new(config: DiscoveryConfig) -> Self {
        Self { config }
    }

    fn discover_scope(
        &self,
        scope: SkillScope,
        bases: &[PathBuf],
        direct_roots: bool,
    ) -> Result<(Vec<SkillEntry>, Vec<Diagnostic>)> {
        let mut entries = Vec::new();
        let mut diagnostics = Vec::new();
        let mut visited = 0usize;

        for base in bases {
            let roots = if direct_roots {
                vec![base.clone()]
            } else {
                self.collect_skill_roots(scope, base)
            };

            for root in roots {
                if !root.is_dir() {
                    continue;
                }

                let discovered =
                    scan_skill_root(&root, scope, &self.config, &mut visited, &mut diagnostics)?;
                entries.extend(discovered);
            }
        }

        Ok((entries, diagnostics))
    }

    fn collect_skill_roots(&self, scope: SkillScope, base: &Path) -> Vec<PathBuf> {
        let mut roots = Vec::new();

        if let Some(client_name) = &self.config.client_name {
            roots.push(base.join(format!(".{client_name}/skills")));
        }

        if self.config.include_agents_convention {
            roots.push(base.join(".agents/skills"));
        }

        if self.config.include_claude_compat {
            roots.push(base.join(".claude/skills"));
        }

        if scope == SkillScope::Project {
            roots.push(base.join("skills"));
        }

        roots.into_iter().filter(|path| path.is_dir()).collect()
    }
}

impl SkillSource for FilesystemDiscovery {
    fn discover(&self) -> Result<DiscoveryResult> {
        let mut entries = Vec::new();
        let mut diagnostics = Vec::new();

        let project_bases = self.config.expanded_project_roots();
        let user_bases = self.config.expanded_user_roots();

        let scopes = [
            (SkillScope::Bundled, &self.config.bundled_roots, true),
            (SkillScope::User, &user_bases, false),
            (
                SkillScope::Organization,
                &self.config.organization_roots,
                false,
            ),
            (SkillScope::Project, &project_bases, false),
        ];

        for (scope, bases, direct_roots) in scopes {
            let (mut scope_entries, mut scope_diagnostics) =
                self.discover_scope(scope, bases, direct_roots)?;
            entries.append(&mut scope_entries);
            diagnostics.append(&mut scope_diagnostics);
        }

        Ok(DiscoveryResult {
            entries,
            diagnostics,
        })
    }
}

pub struct StaticSkillSource {
    entries: Vec<SkillEntry>,
}

impl StaticSkillSource {
    pub fn new(entries: Vec<SkillEntry>) -> Self {
        Self { entries }
    }
}

impl SkillSource for StaticSkillSource {
    fn discover(&self) -> Result<DiscoveryResult> {
        Ok(DiscoveryResult {
            entries: self.entries.clone(),
            diagnostics: Vec::new(),
        })
    }
}

pub struct MultiSkillSource<'a> {
    sources: Vec<&'a dyn SkillSource>,
}

impl<'a> MultiSkillSource<'a> {
    pub fn new(sources: Vec<&'a dyn SkillSource>) -> Self {
        Self { sources }
    }
}

impl SkillSource for MultiSkillSource<'_> {
    fn discover(&self) -> Result<DiscoveryResult> {
        let mut entries = Vec::new();
        let mut diagnostics = Vec::new();

        for source in &self.sources {
            let result = source.discover()?;
            entries.extend(result.entries);
            diagnostics.extend(result.diagnostics);
        }

        Ok(DiscoveryResult {
            entries,
            diagnostics,
        })
    }
}

pub fn format_diagnostics(diagnostics: &[Diagnostic]) -> String {
    if diagnostics.is_empty() {
        return "No diagnostics.".to_owned();
    }

    diagnostics
        .iter()
        .map(|diagnostic| {
            let skill = diagnostic
                .skill
                .as_deref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default();
            format!("[{:?}]{skill} {}", diagnostic.severity, diagnostic.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn scan_skill_root(
    source_root: &Path,
    scope: SkillScope,
    config: &DiscoveryConfig,
    visited: &mut usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<SkillEntry>> {
    let gitignore = if config.respect_gitignore {
        GitignoreMatcher::from_root(source_root)
    } else {
        None
    };

    scan_skill_root_plain(
        source_root,
        scope,
        config,
        visited,
        diagnostics,
        gitignore.as_ref(),
    )
}

fn scan_skill_root_plain(
    source_root: &Path,
    scope: SkillScope,
    config: &DiscoveryConfig,
    visited: &mut usize,
    diagnostics: &mut Vec<Diagnostic>,
    gitignore: Option<&GitignoreMatcher>,
) -> Result<Vec<SkillEntry>> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(source_root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if should_skip_dir(&path, config) {
            continue;
        }

        if gitignore.is_some_and(|matcher| matcher.is_ignored(&path)) {
            continue;
        }

        if find_skill_md(&path).is_some() {
            if let Some(entry) = load_skill_entry(&path, scope, source_root, diagnostics) {
                entries.push(entry);
            }
            continue;
        }

        if *visited >= config.max_directories {
            push_max_directories_warning(source_root, config, diagnostics);
            break;
        }

        for nested_path in walkdir_directories(&path, config) {
            if should_skip_dir(&nested_path, config) {
                continue;
            }

            if gitignore.is_some_and(|matcher| matcher.is_ignored(&nested_path)) {
                continue;
            }

            if find_skill_md(&nested_path).is_none() {
                continue;
            }

            *visited += 1;
            if *visited > config.max_directories {
                push_max_directories_warning(source_root, config, diagnostics);
                return Ok(entries);
            }

            if let Some(entry) = load_skill_entry(&nested_path, scope, source_root, diagnostics) {
                entries.push(entry);
            }
        }
    }

    Ok(entries)
}

fn walkdir_directories(path: &Path, config: &DiscoveryConfig) -> Vec<PathBuf> {
    WalkDir::new(path)
        .min_depth(1)
        .max_depth(config.max_depth)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.into_path())
        .collect()
}

fn push_max_directories_warning(
    source_root: &Path,
    config: &DiscoveryConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.push(Diagnostic {
        severity: Severity::Warning,
        message: format!(
            "Discovery stopped at max_directories ({}) under {}",
            config.max_directories,
            source_root.display()
        ),
        skill: None,
    });
}

fn should_skip_dir(path: &Path, config: &DiscoveryConfig) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|name| config.skip_dir_names.contains(name))
        .unwrap_or(false)
}

fn load_skill_entry(
    skill_dir: &Path,
    scope: SkillScope,
    source_root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<SkillEntry> {
    let skill_md = find_skill_md(skill_dir)?;
    let content = read_skill_md_content(&skill_md).ok()?;
    let (metadata, body) = match parse_frontmatter(&content) {
        Ok(parsed) => parsed,
        Err(error) => {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: error.to_string(),
                skill: Some(skill_dir.display().to_string()),
            });
            return None;
        }
    };

    let properties = match is_runtime_loadable(&metadata) {
        Ok(properties) => properties,
        Err(error) => {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                message: error.to_string(),
                skill: skill_dir
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned()),
            });
            return None;
        }
    };

    let document = SkillDocument {
        properties: properties.clone(),
        body,
        skill_md_path: skill_md.canonicalize().unwrap_or(skill_md.clone()),
        skill_dir: skill_dir
            .canonicalize()
            .unwrap_or_else(|_| skill_dir.to_path_buf()),
    };

    diagnostics.extend(validate_lenient(&document));

    Some(SkillEntry {
        properties,
        location: document.skill_md_path,
        skill_dir: document.skill_dir,
        scope,
        source_root: source_root.to_path_buf(),
    })
}

pub fn merge_skills(entries: Vec<SkillEntry>) -> (Vec<SkillEntry>, Vec<CollisionWarning>) {
    let mut by_name: HashMap<String, SkillEntry> = HashMap::new();
    let mut warnings = Vec::new();

    let ordered: Vec<SkillEntry> = entries
        .into_iter()
        .filter(|entry| !entry.properties.description.trim().is_empty())
        .collect();

    for entry in ordered {
        let name = entry.properties.name.clone();
        if let Some(existing) = by_name.get(&name) {
            if scope_precedence(entry.scope) >= scope_precedence(existing.scope) {
                warnings.push(CollisionWarning {
                    name: name.clone(),
                    shadowed_location: existing.location.clone(),
                    kept_location: entry.location.clone(),
                });
                by_name.insert(name, entry);
            } else {
                warnings.push(CollisionWarning {
                    name: name.clone(),
                    shadowed_location: entry.location.clone(),
                    kept_location: existing.location.clone(),
                });
            }
            continue;
        }
        by_name.insert(name, entry);
    }

    let mut merged: Vec<SkillEntry> = by_name.into_values().collect();
    merged.sort_by(|left, right| left.properties.name.cmp(&right.properties.name));
    (merged, warnings)
}

fn scope_precedence(scope: SkillScope) -> u8 {
    match scope {
        SkillScope::Bundled => 0,
        SkillScope::User => 1,
        SkillScope::Organization => 2,
        SkillScope::Project => 3,
    }
}

pub fn discover_from(source: &dyn SkillSource) -> Result<DiscoveryResult> {
    source.discover()
}

pub fn discover_and_merge(
    config: DiscoveryConfig,
) -> Result<(Vec<SkillEntry>, Vec<Diagnostic>, Vec<CollisionWarning>)> {
    discover_and_merge_from(&FilesystemDiscovery::new(config))
}

pub fn discover_and_merge_from(
    source: &dyn SkillSource,
) -> Result<(Vec<SkillEntry>, Vec<Diagnostic>, Vec<CollisionWarning>)> {
    let result = source.discover()?;
    let (merged, collisions) = merge_skills(result.entries);
    Ok((merged, result.diagnostics, collisions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_skill(dir: &Path, name: &str, description: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\nBody\n"),
        )
        .unwrap();
    }

    #[test]
    fn project_overrides_user() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let project = temp.path().join("project");

        write_skill(
            &home.join(".agents/skills/shared-skill"),
            "shared-skill",
            "user version",
        );
        write_skill(
            &project.join(".agents/skills/shared-skill"),
            "shared-skill",
            "project version",
        );

        let config = DiscoveryConfig {
            project_roots: vec![project.clone()],
            user_roots: vec![home.clone()],
            include_agents_convention: true,
            ..Default::default()
        };

        let (entries, _, warnings) = discover_and_merge(config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].properties.description, "project version");
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn discovers_bundled_roots() {
        let temp = TempDir::new().unwrap();
        write_skill(
            &temp.path().join("builtin-skill"),
            "builtin-skill",
            "built in",
        );

        let config = DiscoveryConfig {
            bundled_roots: vec![temp.path().to_path_buf()],
            ..Default::default()
        };

        let (entries, _, _) = discover_and_merge(config).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].scope, SkillScope::Bundled);
    }

    #[test]
    fn ancestor_scan_finds_parent_skill() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        let package = repo.join("packages/app");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        write_skill(
            &repo.join(".agents/skills/repo-skill"),
            "repo-skill",
            "from repo root",
        );

        let config = DiscoveryConfig {
            project_roots: vec![package.clone()],
            include_agents_convention: true,
            include_ancestor_scan: true,
            ..Default::default()
        };

        let (entries, _, _) = discover_and_merge(config).unwrap();
        assert!(
            entries
                .iter()
                .any(|entry| entry.properties.name == "repo-skill")
        );
    }
}
