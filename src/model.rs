use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;
use serde_yaml::Value;

pub const METADATA_DISABLE_MODEL_INVOCATION: &str = "disable-model-invocation";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillProperties {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(rename = "allowed-tools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl SkillProperties {
    pub fn from_metadata(metadata: &BTreeMap<String, Value>) -> crate::Result<Self> {
        let name = required_string(metadata, "name")?;
        let description = required_string(metadata, "description")?;

        let metadata_map = metadata
            .get("metadata")
            .map(parse_metadata_map)
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            name,
            description,
            license: optional_string(metadata, "license"),
            compatibility: optional_string(metadata, "compatibility"),
            allowed_tools: optional_string(metadata, "allowed-tools"),
            metadata: metadata_map,
        })
    }

    pub fn disables_model_invocation(&self) -> bool {
        self.metadata
            .get(METADATA_DISABLE_MODEL_INVOCATION)
            .map(|value| parse_truthy(value))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDocument {
    pub properties: SkillProperties,
    pub body: String,
    pub skill_md_path: PathBuf,
    pub skill_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillScope {
    Bundled,
    Organization,
    Project,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    pub properties: SkillProperties,
    pub location: PathBuf,
    pub skill_dir: PathBuf,
    pub scope: SkillScope,
    pub source_root: PathBuf,
}

impl SkillEntry {
    pub fn disables_model_invocation(&self) -> bool {
        self.properties.disables_model_invocation()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMode {
    FileRead,
    DedicatedTool { tool_name: &'static str },
}

impl Default for ActivationMode {
    fn default() -> Self {
        Self::DedicatedTool {
            tool_name: "activate_skill",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapStyle {
    None,
    SkillContent,
}

#[derive(Debug, Clone)]
pub struct ActivationOptions {
    pub strip_frontmatter: bool,
    pub wrap: WrapStyle,
    pub list_resources: bool,
    pub max_listed_resources: usize,
}

impl Default for ActivationOptions {
    fn default() -> Self {
        Self {
            strip_frontmatter: true,
            wrap: WrapStyle::SkillContent,
            list_resources: true,
            max_listed_resources: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivatedSkill {
    pub name: String,
    pub content: String,
    pub skill_dir: PathBuf,
    pub resources: Vec<PathBuf>,
    pub resources_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub skill: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionWarning {
    pub name: String,
    pub shadowed_location: PathBuf,
    pub kept_location: PathBuf,
}

fn required_string(metadata: &BTreeMap<String, Value>, field: &str) -> crate::Result<String> {
    let value = metadata.get(field).ok_or_else(|| {
        crate::ValidationError::single(format!("Missing required field in frontmatter: {field}"))
    })?;

    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            crate::ValidationError::single(format!("Field '{field}' must be a non-empty string"))
                .into()
        })
}

fn optional_string(metadata: &BTreeMap<String, Value>, field: &str) -> Option<String> {
    metadata.get(field).and_then(yaml_scalar_to_string)
}

fn parse_metadata_map(value: &Value) -> crate::Result<BTreeMap<String, String>> {
    let mapping = value.as_mapping().ok_or_else(|| {
        crate::ParseError::new("Field 'metadata' must be a YAML mapping".to_owned())
    })?;

    let mut result = BTreeMap::new();
    for (key, value) in mapping {
        let key = key
            .as_str()
            .ok_or_else(|| crate::ParseError::new("Metadata keys must be strings".to_owned()))?;
        let value = yaml_scalar_to_string(value).ok_or_else(|| {
            crate::ParseError::new(format!("Metadata value for '{key}' must be a scalar"))
        })?;
        result.insert(key.to_owned(), value);
    }
    Ok(result)
}

fn yaml_scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(string) => Some(string.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(boolean) => Some(boolean.to_string()),
        _ => None,
    }
}

pub fn parse_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}
