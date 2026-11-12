use std::path::Path;

use crate::model::SkillEntry;
use crate::parse::find_skill_md;
use crate::{Result, SkillProperties, read_properties};

pub fn to_prompt(skills: &[SkillEntry]) -> Result<String> {
    to_prompt_from_dirs(skills.iter().map(|entry| entry.skill_dir.as_path()))
}

pub fn to_prompt_from_dirs<'a, I>(skill_dirs: I) -> Result<String>
where
    I: IntoIterator<Item = &'a Path>,
{
    let mut lines = vec!["<available_skills>".to_owned()];

    for skill_dir in skill_dirs {
        let properties = read_properties(skill_dir)?;
        append_skill_lines(&mut lines, &properties, skill_dir);
    }

    lines.push("</available_skills>".to_owned());
    Ok(lines.join("\n"))
}

pub fn to_prompt_from_properties(
    skills: &[(SkillProperties, std::path::PathBuf)],
) -> Result<String> {
    let mut lines = vec!["<available_skills>".to_owned()];

    for (properties, skill_dir) in skills {
        append_skill_lines(&mut lines, properties, skill_dir);
    }

    lines.push("</available_skills>".to_owned());
    Ok(lines.join("\n"))
}

fn append_skill_lines(lines: &mut Vec<String>, properties: &SkillProperties, skill_dir: &Path) {
    let skill_md_path = find_skill_md(skill_dir)
        .and_then(|path| path.canonicalize().ok())
        .unwrap_or_else(|| skill_dir.join("SKILL.md"));

    lines.push("<skill>".to_owned());
    lines.push("<name>".to_owned());
    lines.push(escape_xml(&properties.name));
    lines.push("</name>".to_owned());
    lines.push("<description>".to_owned());
    lines.push(escape_xml(&properties.description));
    lines.push("</description>".to_owned());
    lines.push("<location>".to_owned());
    lines.push(escape_xml(&skill_md_path.display().to_string()));
    lines.push("</location>".to_owned());
    lines.push("</skill>".to_owned());
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn empty_list() {
        assert_eq!(
            to_prompt_from_dirs([]).unwrap(),
            "<available_skills>\n</available_skills>"
        );
    }

    #[test]
    fn special_characters_escaped() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("special-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: special-skill\ndescription: Use <foo> & <bar> tags\n---\nBody\n",
        )
        .unwrap();

        let result = to_prompt_from_dirs([skill_dir.as_path()]).unwrap();
        assert!(result.contains("&lt;foo&gt;"));
        assert!(result.contains("&amp;"));
        assert!(!result.contains("<foo>"));
    }

    #[test]
    fn location_path_escaped() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("amp&root/bravo");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: bravo\ndescription: Synthetic path test.\n---\nBody\n",
        )
        .unwrap();

        let result = to_prompt_from_dirs([skill_dir.as_path()]).unwrap();
        assert!(result.contains("amp&amp;root/bravo/SKILL.md"));
    }

    #[test]
    fn location_path_double_escapes_entity() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("entity&amp;root/charlie");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: charlie\ndescription: Synthetic path test.\n---\nBody\n",
        )
        .unwrap();

        let result = to_prompt_from_dirs([skill_dir.as_path()]).unwrap();
        assert!(result.contains("entity&amp;amp;root/charlie/SKILL.md"));
    }
}
