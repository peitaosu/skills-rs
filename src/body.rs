use std::path::{Component, Path};

pub const RECOMMENDED_SKILL_BODY_LINES: usize = 500;
pub const RECOMMENDED_SKILL_BODY_TOKENS: usize = 5000;
pub const MAX_FILE_REFERENCE_DEPTH: usize = 2;

pub fn validate_body_advisories(body: &str, skill_dir: &Path) -> Vec<String> {
    let mut advisories = Vec::new();

    let line_count = body.lines().count();
    if line_count > RECOMMENDED_SKILL_BODY_LINES {
        advisories.push(format!(
            "SKILL.md body has {line_count} lines; spec recommends keeping under {RECOMMENDED_SKILL_BODY_LINES} lines"
        ));
    }

    let estimated_tokens = estimate_tokens(body);
    if estimated_tokens > RECOMMENDED_SKILL_BODY_TOKENS {
        advisories.push(format!(
            "SKILL.md body estimated at ~{estimated_tokens} tokens; spec recommends under {RECOMMENDED_SKILL_BODY_TOKENS} tokens for instructions"
        ));
    }

    for reference in extract_file_references(body) {
        advisories.extend(validate_file_reference(&reference, skill_dir));
    }

    advisories
}

fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

fn extract_file_references(body: &str) -> Vec<String> {
    let mut references = Vec::new();

    for line in body.lines() {
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            rest = &rest[start + 2..];
            let Some(end) = rest.find(')') else {
                break;
            };
            let target = rest[..end].trim();
            if is_relative_reference(target) {
                references.push(strip_anchor(target).to_owned());
            }
            rest = &rest[end + 1..];
        }

        for token in line.split_whitespace() {
            let cleaned = token.trim_matches(|character: char| {
                matches!(
                    character,
                    '`' | '(' | ')' | '[' | ']' | ',' | '.' | ';' | ':'
                )
            });
            if (cleaned.starts_with("scripts/")
                || cleaned.starts_with("references/")
                || cleaned.starts_with("assets/"))
                && is_relative_reference(cleaned)
            {
                references.push(strip_anchor(cleaned).to_owned());
            }
        }
    }

    references.sort();
    references.dedup();
    references
}

fn is_relative_reference(target: &str) -> bool {
    !target.is_empty()
        && !target.starts_with("http://")
        && !target.starts_with("https://")
        && !target.starts_with('#')
        && !target.starts_with('/')
}

fn strip_anchor(target: &str) -> &str {
    target.split('#').next().unwrap_or(target).trim()
}

fn validate_file_reference(reference: &str, skill_dir: &Path) -> Vec<String> {
    let mut advisories = Vec::new();
    let path = Path::new(reference);

    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        advisories.push(format!(
            "File reference '{reference}' must be relative to the skill root without parent segments"
        ));
        return advisories;
    }

    let depth = path.components().count();
    if depth > MAX_FILE_REFERENCE_DEPTH {
        advisories.push(format!(
            "File reference '{reference}' is nested deeper than one directory level from SKILL.md; spec recommends one level deep"
        ));
    }

    let resolved = skill_dir.join(path);
    if !resolved.exists() {
        advisories.push(format!(
            "File reference '{reference}' does not exist under skill directory"
        ));
    }

    advisories
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn warns_on_long_body_and_deep_reference() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("demo");
        fs::create_dir_all(skill_dir.join("references/sub")).unwrap();
        fs::write(skill_dir.join("references/sub/deep.md"), "x").unwrap();

        let body = format!(
            "{}\nSee [deep](references/sub/deep.md)",
            "line\n".repeat(501)
        );
        let advisories = validate_body_advisories(&body, &skill_dir);
        assert!(advisories.iter().any(|message| message.contains("lines")));
        assert!(
            advisories
                .iter()
                .any(|message| message.contains("one level deep"))
        );
    }
}
