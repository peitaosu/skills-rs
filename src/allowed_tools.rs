use std::path::Path;

use crate::model::SkillEntry;

pub struct AllowedToolsPolicy;

impl AllowedToolsPolicy {
    pub fn allows_script(
        entry: &SkillEntry,
        script_file_name: &str,
        script_content: Option<&str>,
    ) -> bool {
        let Some(allowed_tools) = &entry.properties.allowed_tools else {
            return true;
        };

        let tokens: Vec<&str> = allowed_tools.split_whitespace().collect();

        if tokens.contains(&"Bash") {
            return true;
        }

        if matches_any_bash_filename(&tokens, script_file_name) {
            return true;
        }

        if let Some(content) = script_content {
            let commands = extract_commands(content);
            if commands.is_empty() {
                return false;
            }
            return commands
                .iter()
                .all(|command| matches_any_bash_command(&tokens, command));
        }

        false
    }

    pub fn allows_read(entry: &SkillEntry) -> bool {
        let Some(allowed_tools) = &entry.properties.allowed_tools else {
            return true;
        };

        allowed_tools
            .split_whitespace()
            .any(|token| token == "Read" || token.starts_with("Read("))
    }

    pub fn allows_read_path(entry: &SkillEntry, relative_path: &Path) -> bool {
        let Some(allowed_tools) = &entry.properties.allowed_tools else {
            return true;
        };

        let relative = normalize_relative_path(relative_path);
        allowed_tools.split_whitespace().any(|token| {
            if token == "Read" {
                return true;
            }
            matches_read_pattern(token, &relative)
        })
    }

    pub fn describe(entry: &SkillEntry) -> String {
        entry
            .properties
            .allowed_tools
            .clone()
            .unwrap_or_else(|| "(any)".to_owned())
    }
}

fn normalize_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn matches_any_bash_filename(tokens: &[&str], script_file_name: &str) -> bool {
    tokens
        .iter()
        .any(|token| matches_bash_filename(token, script_file_name))
}

fn matches_any_bash_command(tokens: &[&str], command: &str) -> bool {
    tokens
        .iter()
        .any(|token| matches_bash_command(token, command))
}

fn matches_bash_filename(token: &str, script_file_name: &str) -> bool {
    let Some(inner) = token
        .strip_prefix("Bash(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return false;
    };

    if inner == "*" {
        return true;
    }

    if inner.contains(":*") {
        return false;
    }

    inner == script_file_name
}

fn matches_bash_command(token: &str, command: &str) -> bool {
    let Some(inner) = token
        .strip_prefix("Bash(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return false;
    };

    if inner == "*" {
        return true;
    }

    if let Some(prefix) = inner.strip_suffix(":*") {
        return command == prefix || command.starts_with(prefix);
    }

    inner == command
}

fn matches_read_pattern(token: &str, path: &str) -> bool {
    let Some(inner) = token
        .strip_prefix("Read(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return false;
    };

    if inner == "*" {
        return true;
    }

    if let Some(prefix) = inner.strip_suffix(":*") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }

    inner == path
}

fn extract_commands(content: &str) -> Vec<String> {
    let mut commands = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        for segment in split_command_segments(trimmed) {
            let Some(command) = command_name(segment) else {
                continue;
            };
            if !commands.iter().any(|existing| existing == &command) {
                commands.push(command);
            }
            break;
        }
    }

    commands
}

fn split_command_segments(line: &str) -> Vec<&str> {
    line.split(|character: char| character.is_whitespace() || matches!(character, '|' | ';' | '&'))
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn command_name(segment: &str) -> Option<String> {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return None;
    }

    let base = trimmed.rsplit('/').next()?.trim();
    if base.is_empty() || base.starts_with('-') {
        return None;
    }

    Some(base.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SkillProperties, SkillScope};
    use std::path::PathBuf;

    fn entry(allowed_tools: Option<&str>) -> SkillEntry {
        SkillEntry {
            properties: SkillProperties {
                name: "demo".to_owned(),
                description: "demo".to_owned(),
                license: None,
                compatibility: None,
                allowed_tools: allowed_tools.map(str::to_owned),
                metadata: Default::default(),
            },
            location: PathBuf::from("/tmp/demo/SKILL.md"),
            skill_dir: PathBuf::from("/tmp/demo"),
            scope: SkillScope::Bundled,
            source_root: PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn bash_pattern_matches_script_name() {
        let entry = entry(Some("Bash(hello.sh)"));
        assert!(AllowedToolsPolicy::allows_script(&entry, "hello.sh", None));
        assert!(!AllowedToolsPolicy::allows_script(&entry, "other.sh", None));
    }

    #[test]
    fn bash_command_pattern_matches_script_content() {
        let entry = entry(Some("Bash(git:*)"));
        let script = "#!/usr/bin/env bash\ngit status\n";
        assert!(AllowedToolsPolicy::allows_script(
            &entry,
            "deploy.sh",
            Some(script)
        ));
        assert!(!AllowedToolsPolicy::allows_script(
            &entry,
            "deploy.sh",
            Some("#!/usr/bin/env bash\ncurl https://example.com\n")
        ));
    }

    #[test]
    fn read_pattern_matches_relative_paths() {
        let entry = entry(Some("Read(references:*)"));
        assert!(AllowedToolsPolicy::allows_read_path(
            &entry,
            Path::new("references/guide.md")
        ));
        assert!(!AllowedToolsPolicy::allows_read_path(
            &entry,
            Path::new("scripts/run.sh")
        ));
    }
}
