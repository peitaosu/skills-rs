use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_yaml::Value;

use crate::Result;
use crate::error::ParseError;
use crate::model::{SkillDocument, SkillProperties};

const SKILL_MD_UPPER: &str = "SKILL.md";
const SKILL_MD_LOWER: &str = "skill.md";

pub fn find_skill_md(skill_dir: &Path) -> Option<PathBuf> {
    for name in [SKILL_MD_UPPER, SKILL_MD_LOWER] {
        let path = skill_dir.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

pub fn parse_frontmatter(content: &str) -> Result<(BTreeMap<String, Value>, String)> {
    parse_frontmatter_with_mode(content, true)
}

pub fn parse_frontmatter_strict(content: &str) -> Result<(BTreeMap<String, Value>, String)> {
    parse_frontmatter_with_mode(content, false)
}

fn parse_frontmatter_with_mode(
    content: &str,
    lenient: bool,
) -> Result<(BTreeMap<String, Value>, String)> {
    let is_delimiter = |line: &str| line.trim_end() == "---";

    let mut lines = content.lines();
    match lines.next() {
        Some(first) if is_delimiter(first) => {}
        _ => {
            return Err(ParseError::new(
                "SKILL.md must start with YAML frontmatter (---)".to_owned(),
            )
            .into());
        }
    }

    let mut frontmatter: Vec<&str> = Vec::new();
    let mut body_lines: Option<Vec<&str>> = None;
    for line in lines {
        if body_lines.is_none() && is_delimiter(line) {
            body_lines = Some(Vec::new());
            continue;
        }
        match &mut body_lines {
            Some(body) => body.push(line),
            None => frontmatter.push(line),
        }
    }

    let Some(body_lines) = body_lines else {
        return Err(ParseError::new(
            "SKILL.md frontmatter not properly closed with ---".to_owned(),
        )
        .into());
    };

    let frontmatter_str = frontmatter.join("\n");
    let body = body_lines.join("\n").trim().to_owned();

    let parsed = parse_yaml_mapping(&frontmatter_str, lenient)?;
    Ok((parsed, body))
}

fn parse_yaml_mapping(yaml: &str, lenient: bool) -> Result<BTreeMap<String, Value>> {
    match serde_yaml::from_str::<Value>(yaml) {
        Ok(value) => mapping_to_btreemap(value),
        Err(original_error) if lenient => {
            let quoted = repair_colon_scalar_values(yaml);
            if let Ok(value) = serde_yaml::from_str::<Value>(&quoted) {
                return mapping_to_btreemap(value);
            }

            let block = repair_block_scalar_values(yaml);
            let value = serde_yaml::from_str::<Value>(&block).map_err(|_| {
                ParseError::new(format!("Invalid YAML in frontmatter: {original_error}"))
            })?;
            mapping_to_btreemap(value)
        }
        Err(error) => Err(ParseError::new(format!("Invalid YAML in frontmatter: {error}")).into()),
    }
}

fn mapping_to_btreemap(value: Value) -> Result<BTreeMap<String, Value>> {
    let metadata = value
        .as_mapping()
        .ok_or_else(|| ParseError::new("SKILL.md frontmatter must be a YAML mapping".to_owned()))?;

    let mut map = BTreeMap::new();
    for (key, value) in metadata {
        let key = key
            .as_str()
            .ok_or_else(|| ParseError::new("Frontmatter keys must be strings".to_owned()))?;
        map.insert(key.to_owned(), value.clone());
    }
    Ok(map)
}

fn repair_colon_scalar_values(yaml: &str) -> String {
    yaml.lines()
        .map(|line| {
            if line.trim().is_empty() || line.starts_with('#') {
                return line.to_owned();
            }

            let indent = line.len() - line.trim_start().len();
            if indent != 0 {
                return line.to_owned();
            }

            let Some((key, value)) = line.split_once(':') else {
                return line.to_owned();
            };

            let value = value.trim();
            if value.is_empty()
                || value.starts_with('"')
                || value.starts_with('\'')
                || value.starts_with('|')
                || value.starts_with('>')
                || !value.contains(':')
            {
                return line.to_owned();
            }

            format!("{}: \"{}\"", key.trim(), value.replace('"', "\\\""))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn repair_block_scalar_values(yaml: &str) -> String {
    yaml.lines()
        .map(|line| {
            if line.trim().is_empty() || line.starts_with('#') {
                return line.to_owned();
            }

            let indent = line.len() - line.trim_start().len();
            if indent != 0 {
                return line.to_owned();
            }

            let Some((key, value)) = line.split_once(':') else {
                return line.to_owned();
            };

            let value = value.trim();
            if value.is_empty()
                || value.starts_with('"')
                || value.starts_with('\'')
                || value.starts_with('|')
                || value.starts_with('>')
            {
                return line.to_owned();
            }

            format!("{}: |-\n  {value}", key.trim())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn read_skill_md_content(skill_md_path: &Path) -> Result<String> {
    let bytes = fs::read(skill_md_path)?;
    String::from_utf8(bytes).map_err(|_| {
        ParseError::new(format!(
            "SKILL.md is not valid UTF-8: {}",
            skill_md_path.display()
        ))
        .into()
    })
}

pub fn read_properties(skill_dir: &Path) -> Result<SkillProperties> {
    let skill_md = find_skill_md(skill_dir)
        .ok_or_else(|| ParseError::new(format!("SKILL.md not found in {}", skill_dir.display())))?;

    let content = read_skill_md_content(&skill_md)?;
    let (metadata, _) = parse_frontmatter(&content)?;
    SkillProperties::from_metadata(&metadata)
}

pub fn read_document(skill_dir: &Path) -> Result<SkillDocument> {
    let skill_md_path = find_skill_md(skill_dir)
        .ok_or_else(|| ParseError::new(format!("SKILL.md not found in {}", skill_dir.display())))?;

    let content = read_skill_md_content(&skill_md_path)?;
    let (metadata, body) = parse_frontmatter(&content)?;
    let properties = SkillProperties::from_metadata(&metadata)?;

    Ok(SkillDocument {
        properties,
        body,
        skill_md_path: skill_md_path.canonicalize().unwrap_or(skill_md_path),
        skill_dir: skill_dir
            .canonicalize()
            .unwrap_or_else(|_| skill_dir.to_path_buf()),
    })
}

pub fn read_document_at(skill_md_path: &Path) -> Result<SkillDocument> {
    read_document(skill_md_path.parent().ok_or_else(|| {
        ParseError::new(format!(
            "Invalid skill path (no parent): {}",
            skill_md_path.display()
        ))
    })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn valid_frontmatter() {
        let content =
            "---\nname: my-skill\ndescription: A test skill\n---\n# My Skill\n\nInstructions.\n";
        let (metadata, body) = parse_frontmatter(content).unwrap();
        assert_eq!(metadata.get("name").unwrap().as_str().unwrap(), "my-skill");
        assert!(body.contains("# My Skill"));
    }

    #[test]
    fn lenient_colon_in_description() {
        let content = "---\nname: my-skill\ndescription: Use this skill when: the user asks about PDFs\n---\nBody\n";
        let (metadata, _) = parse_frontmatter(content).unwrap();
        assert_eq!(
            metadata.get("description").unwrap().as_str().unwrap(),
            "Use this skill when: the user asks about PDFs"
        );
    }

    #[test]
    fn find_skill_md_prefers_uppercase() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(skill_dir.join(SKILL_MD_UPPER), "upper").unwrap();
        fs::write(skill_dir.join(SKILL_MD_LOWER), "lower").unwrap();
        assert_eq!(
            find_skill_md(&skill_dir).unwrap().file_name().unwrap(),
            SKILL_MD_UPPER
        );
    }

    #[test]
    fn lenient_block_scalar_fallback() {
        let yaml = "name: my-skill\ndescription: alpha: beta: gamma";
        let block = repair_block_scalar_values(yaml);
        assert!(block.contains("description: |"));
        let wrapped = format!("---\n{block}\n---\nBody\n");
        let (metadata, _) = parse_frontmatter(&wrapped).unwrap();
        assert_eq!(
            metadata.get("description").unwrap().as_str().unwrap(),
            "alpha: beta: gamma"
        );
    }

    #[test]
    fn delimiter_inside_quoted_value() {
        let content =
            "---\nname: my-skill\ndescription: \"Read --- separated receipts\"\n---\nBody\n";
        let (metadata, body) = parse_frontmatter(content).unwrap();
        assert_eq!(
            metadata.get("description").unwrap().as_str().unwrap(),
            "Read --- separated receipts"
        );
        assert_eq!(body, "Body");
    }

    #[test]
    fn first_line_must_be_exact_delimiter() {
        let content = "----\nname: my-skill\ndescription: Test\n----\nBody\n";
        assert!(parse_frontmatter(content).is_err());
    }

    #[test]
    fn delimiter_with_trailing_whitespace_accepted() {
        let content = "---  \nname: my-skill\ndescription: Test\n---  \nBody\n";
        let (metadata, body) = parse_frontmatter(content).unwrap();
        assert_eq!(metadata.get("name").unwrap().as_str().unwrap(), "my-skill");
        assert_eq!(body, "Body");
    }

    #[test]
    fn horizontal_rule_in_body() {
        let content = "---\nname: my-skill\ndescription: Test\n---\nIntro\n\n---\n\nOutro\n";
        let (metadata, body) = parse_frontmatter(content).unwrap();
        assert_eq!(metadata.get("name").unwrap().as_str().unwrap(), "my-skill");
        assert!(body.contains("---"));
    }

    #[test]
    fn crlf_line_endings() {
        let content = "---\r\nname: my-skill\r\ndescription: Test\r\n---\r\nBody\r\n";
        let (metadata, body) = parse_frontmatter(content).unwrap();
        assert_eq!(metadata.get("name").unwrap().as_str().unwrap(), "my-skill");
        assert_eq!(body, "Body");
    }

    #[test]
    fn multilingual_utf8_content() {
        let content = "---\nname: my-skill\ndescription: Héllo wörld — 日本語\n---\nCuerpo\n";
        let (metadata, _) = parse_frontmatter(content).unwrap();
        assert_eq!(
            metadata.get("description").unwrap().as_str().unwrap(),
            "Héllo wörld — 日本語"
        );
    }

    #[test]
    fn invalid_utf8_reported_as_parse_error() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join(SKILL_MD_UPPER),
            b"---\nname: my-skill\ndescription: \xff\n---\nBody\n",
        )
        .unwrap();
        let error = read_properties(&skill_dir).unwrap_err();
        assert!(error.to_string().contains("UTF-8"));
    }
}
