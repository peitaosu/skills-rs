use std::collections::BTreeMap;
use std::path::Path;

use serde_yaml::Value;
use unicode_normalization::UnicodeNormalization;

use crate::body::validate_body_advisories;
use crate::script::validate_scripts_advisories;
use crate::model::{Diagnostic, Severity, SkillDocument, SkillProperties};
use crate::parse::{find_skill_md, parse_frontmatter, parse_frontmatter_strict, read_skill_md_content};
use crate::Result;

pub const MAX_SKILL_NAME_LENGTH: usize = 64;
pub const MAX_DESCRIPTION_LENGTH: usize = 1024;
pub const MAX_COMPATIBILITY_LENGTH: usize = 500;

const ALLOWED_FIELDS: &[&str] = &[
    "name",
    "description",
    "license",
    "allowed-tools",
    "metadata",
    "compatibility",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameValidationPolicy {
    #[default]
    SkillsRef,
    SpecStrict,
}

pub fn validate(skill_dir: &Path) -> Vec<String> {
    validate_with_policy(skill_dir, NameValidationPolicy::SkillsRef)
}

pub fn validate_with_policy(skill_dir: &Path, policy: NameValidationPolicy) -> Vec<String> {
    if !skill_dir.exists() {
        return vec![format!("Path does not exist: {}", skill_dir.display())];
    }

    if !skill_dir.is_dir() {
        return vec![format!("Not a directory: {}", skill_dir.display())];
    }

    let Some(skill_md) = find_skill_md(skill_dir) else {
        return vec!["Missing required file: SKILL.md".to_owned()];
    };

    let content = match read_skill_md_content(&skill_md) {
        Ok(content) => content,
        Err(error) => return vec![error.to_string()],
    };

    let (metadata, _) = match parse_frontmatter_strict(&content) {
        Ok(parsed) => parsed,
        Err(error) => return vec![error.to_string()],
    };

    validate_metadata_with_policy(&metadata, Some(skill_dir), policy)
}

pub fn validate_advisories(skill_dir: &Path) -> Vec<String> {
    let Some(skill_md) = find_skill_md(skill_dir) else {
        return Vec::new();
    };

    let Ok(content) = read_skill_md_content(&skill_md) else {
        return Vec::new();
    };

    let Ok((_, body)) = parse_frontmatter_strict(&content) else {
        return Vec::new();
    };

    let mut advisories = validate_body_advisories(&body, skill_dir);
    advisories.extend(validate_scripts_advisories(skill_dir));
    advisories
}

pub fn validate_metadata(metadata: &BTreeMap<String, Value>, skill_dir: Option<&Path>) -> Vec<String> {
    validate_metadata_with_policy(metadata, skill_dir, NameValidationPolicy::SkillsRef)
}

pub fn validate_metadata_with_policy(
    metadata: &BTreeMap<String, Value>,
    skill_dir: Option<&Path>,
    policy: NameValidationPolicy,
) -> Vec<String> {
    let mut errors = validate_metadata_fields(metadata);

    match metadata.get("name") {
        Some(value) => {
            if let Some(name) = value.as_str() {
                errors.extend(validate_name_with_policy(name, skill_dir, policy, false));
            } else {
                errors.push("Field 'name' must be a non-empty string".to_owned());
            }
        }
        None => errors.push("Missing required field in frontmatter: name".to_owned()),
    }

    match metadata.get("description") {
        Some(value) => {
            if let Some(description) = value.as_str() {
                errors.extend(validate_description(description));
            } else {
                errors.push("Field 'description' must be a non-empty string".to_owned());
            }
        }
        None => errors.push("Missing required field in frontmatter: description".to_owned()),
    }

    if let Some(value) = metadata.get("compatibility") {
        if let Some(compatibility) = value.as_str() {
            errors.extend(validate_compatibility(compatibility));
        } else {
            errors.push("Field 'compatibility' must be a string".to_owned());
        }
    }

    errors
}

pub fn validate_lenient(document: &SkillDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let skill_name = Some(document.properties.name.clone());

    if let Ok(metadata) = metadata_from_document(document) {
        for message in validate_metadata_fields(&metadata) {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                message,
                skill: skill_name.clone(),
            });
        }

        if let Some(name) = metadata.get("name").and_then(Value::as_str) {
            for message in validate_name_with_policy(name, Some(&document.skill_dir), NameValidationPolicy::SkillsRef, true) {
                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    message,
                    skill: skill_name.clone(),
                });
            }
        }

        if let Some(description) = metadata.get("description").and_then(Value::as_str) {
            for message in validate_description_lenient(description) {
                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    message,
                    skill: skill_name.clone(),
                });
            }
        }
    }

    for message in validate_body_advisories(&document.body, &document.skill_dir) {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            message,
            skill: skill_name.clone(),
        });
    }

    for message in validate_scripts_advisories(&document.skill_dir) {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            message,
            skill: skill_name.clone(),
        });
    }

    diagnostics
}

pub fn is_runtime_loadable(metadata: &BTreeMap<String, Value>) -> Result<SkillProperties> {
    SkillProperties::from_metadata(metadata)
}

fn metadata_from_document(document: &SkillDocument) -> Result<BTreeMap<String, Value>> {
    let content = read_skill_md_content(&document.skill_md_path)?;
    let (metadata, _) = parse_frontmatter(&content)?;
    Ok(metadata)
}

fn validate_metadata_fields(metadata: &BTreeMap<String, Value>) -> Vec<String> {
    let allowed: std::collections::BTreeSet<&str> = ALLOWED_FIELDS.iter().copied().collect();
    let mut extra: Vec<String> = metadata
        .keys()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect();

    if extra.is_empty() {
        return Vec::new();
    }

    extra.sort();
    vec![format!(
        "Unexpected fields in frontmatter: {}. Only {:?} are allowed.",
        extra.join(", "),
        ALLOWED_FIELDS
    )]
}

fn normalize_name(name: &str) -> String {
    name.trim().nfkc().collect::<String>()
}

fn validate_name_with_policy(
    name: &str,
    skill_dir: Option<&Path>,
    policy: NameValidationPolicy,
    lenient_length: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    let name = normalize_name(name);

    if name.is_empty() {
        errors.push("Field 'name' must be a non-empty string".to_owned());
        return errors;
    }

    if name.len() > MAX_SKILL_NAME_LENGTH {
        errors.push(format!(
            "Skill name '{name}' exceeds {MAX_SKILL_NAME_LENGTH} character limit ({} chars)",
            name.len()
        ));
        if lenient_length {
            return errors;
        }
    }

    if !name_is_lowercase(&name, policy) {
        errors.push(format!("Skill name '{name}' must be lowercase"));
    }

    if name.starts_with('-') || name.ends_with('-') {
        errors.push("Skill name cannot start or end with a hyphen".to_owned());
    }

    if name.contains("--") {
        errors.push("Skill name cannot contain consecutive hyphens".to_owned());
    }

    if !name_has_valid_characters(&name, policy) {
        errors.push(invalid_characters_message(&name, policy));
    }

    if let Some(skill_dir) = skill_dir {
        let dir_name = normalize_name(
            skill_dir
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        );
        if dir_name != name {
            errors.push(format!(
                "Directory name '{}' must match skill name '{name}'",
                skill_dir.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
    }

    errors
}

fn name_is_lowercase(name: &str, policy: NameValidationPolicy) -> bool {
    match policy {
        NameValidationPolicy::SkillsRef => name == name.to_lowercase(),
        NameValidationPolicy::SpecStrict => name == name.to_ascii_lowercase(),
    }
}

fn name_has_valid_characters(name: &str, policy: NameValidationPolicy) -> bool {
    match policy {
        NameValidationPolicy::SkillsRef => name
            .chars()
            .all(|character| character.is_alphanumeric() || character == '-'),
        NameValidationPolicy::SpecStrict => name
            .chars()
            .all(|character| matches!(character, 'a'..='z' | '0'..='9' | '-')),
    }
}

fn invalid_characters_message(name: &str, policy: NameValidationPolicy) -> String {
    match policy {
        NameValidationPolicy::SkillsRef => format!(
            "Skill name '{name}' contains invalid characters. Only letters, digits, and hyphens are allowed."
        ),
        NameValidationPolicy::SpecStrict => format!(
            "Skill name '{name}' contains invalid characters. Only ASCII lowercase letters (a-z), digits (0-9), and hyphens are allowed."
        ),
    }
}

fn validate_description(description: &str) -> Vec<String> {
    validate_description_with_mode(description, false)
}

fn validate_description_lenient(description: &str) -> Vec<String> {
    validate_description_with_mode(description, true)
}

fn validate_description_with_mode(description: &str, _lenient: bool) -> Vec<String> {
    let description = description.trim();
    if description.is_empty() {
        return vec!["Field 'description' must be a non-empty string".to_owned()];
    }

    if description.len() > MAX_DESCRIPTION_LENGTH {
        return vec![format!(
            "Description exceeds {MAX_DESCRIPTION_LENGTH} character limit ({} chars)",
            description.len()
        )];
    }

    Vec::new()
}

fn validate_compatibility(compatibility: &str) -> Vec<String> {
    if compatibility.trim().is_empty() {
        return vec!["Field 'compatibility' must be a non-empty string when provided".to_owned()];
    }

    if compatibility.len() > MAX_COMPATIBILITY_LENGTH {
        return vec![format!(
            "Compatibility exceeds {MAX_COMPATIBILITY_LENGTH} character limit ({} chars)",
            compatibility.len()
        )];
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_ref_allows_unicode_names() {
        assert!(name_has_valid_characters("技能", NameValidationPolicy::SkillsRef));
        assert!(!name_has_valid_characters("技能", NameValidationPolicy::SpecStrict));
    }

    #[test]
    fn spec_strict_allows_ascii_names() {
        assert!(name_has_valid_characters("my-skill-2", NameValidationPolicy::SpecStrict));
    }
}
