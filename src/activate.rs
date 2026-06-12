use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::model::{ActivatedSkill, ActivationOptions, SkillEntry, WrapStyle};
use crate::parse::read_document;
use crate::Result;

pub struct ResourceListing {
    pub paths: Vec<PathBuf>,
    pub truncated: bool,
}

pub struct SkillActivator;

impl SkillActivator {
    pub fn activate(entry: &SkillEntry, options: ActivationOptions) -> Result<ActivatedSkill> {
        let document = read_document(&entry.skill_dir)?;
        let content_body = if options.strip_frontmatter {
            document.body
        } else {
            fs::read_to_string(&document.skill_md_path)?
        };

        let listing = if options.list_resources {
            list_resources(&entry.skill_dir, options.max_listed_resources)?
        } else {
            ResourceListing {
                paths: Vec::new(),
                truncated: false,
            }
        };

        let content = match options.wrap {
            WrapStyle::None => content_body,
            WrapStyle::SkillContent => wrap_skill_content(
                &entry.properties.name,
                &content_body,
                &entry.skill_dir,
                &listing,
            ),
        };

        Ok(ActivatedSkill {
            name: entry.properties.name.clone(),
            content,
            skill_dir: entry.skill_dir.clone(),
            resources: listing.paths,
            resources_truncated: listing.truncated,
        })
    }
}

fn wrap_skill_content(
    name: &str,
    body: &str,
    skill_dir: &Path,
    listing: &ResourceListing,
) -> String {
    let mut content = format!("<skill_content name=\"{name}\">\n{body}\n\n");
    content.push_str(&format!(
        "Skill directory: {}\n\
         Relative paths in this skill are relative to the skill directory.\n",
        skill_dir.display()
    ));

    if !listing.paths.is_empty() {
        content.push_str("\n<skill_resources>\n");
        for resource in &listing.paths {
            content.push_str("  <file>");
            content.push_str(&resource.display().to_string());
            content.push_str("</file>\n");
        }
        if listing.truncated {
            content.push_str("  <!-- listing may be incomplete -->\n");
        }
        content.push_str("</skill_resources>\n");
    }

    content.push_str("</skill_content>");
    content
}

pub fn list_resources(skill_dir: &Path, max_listed: usize) -> Result<ResourceListing> {
    let mut resources = Vec::new();
    let mut truncated = false;

    for subdir in ["scripts", "references", "assets"] {
        let path = skill_dir.join(subdir);
        if !path.is_dir() {
            continue;
        }
        let before = resources.len();
        collect_files(&path, skill_dir, &mut resources, max_listed, &mut truncated)?;
        if resources.len() >= max_listed && resources.len() > before {
            truncated = true;
            break;
        }
    }

    if resources.len() > max_listed {
        truncated = true;
        resources.truncate(max_listed);
    }

    Ok(ResourceListing {
        paths: resources,
        truncated,
    })
}

fn collect_files(
    dir: &Path,
    skill_dir: &Path,
    resources: &mut Vec<PathBuf>,
    max_listed: usize,
    truncated: &mut bool,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        if resources.len() >= max_listed {
            *truncated = true;
            return Ok(());
        }

        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, skill_dir, resources, max_listed, truncated)?;
            continue;
        }

        if path.is_file()
            && let Some(relative) = safe_relative_path(skill_dir, &path)
        {
            resources.push(relative);
        }
    }
    Ok(())
}

pub fn safe_relative_path(base: &Path, path: &Path) -> Option<PathBuf> {
    let base = base.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    if !path.starts_with(&base) {
        return None;
    }

    let relative = path.strip_prefix(&base).ok()?;
    if relative
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }

    Some(relative.to_path_buf())
}

pub fn resolve_under_skill_dir(skill_dir: &Path, relative: &Path) -> Result<PathBuf> {
    if relative
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(crate::SkillError::Other {
            message: format!(
                "Path escapes skill directory: {}",
                relative.display()
            ),
        });
    }

    let resolved = skill_dir.join(relative);
    let canonical = resolved.canonicalize().unwrap_or(resolved.clone());
    let skill_dir = skill_dir.canonicalize().unwrap_or_else(|_| skill_dir.to_path_buf());

    if !canonical.starts_with(&skill_dir) {
        return Err(crate::SkillError::Other {
            message: format!(
                "Resolved path outside skill directory: {}",
                relative.display()
            ),
        });
    }

    Ok(canonical)
}
