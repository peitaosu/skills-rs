use std::fs;
use std::path::{Path, PathBuf};

const HEADER_SCAN_LINES: usize = 30;
const TRIVIAL_SCRIPT_MAX_LINES: usize = 3;

pub fn validate_scripts_advisories(skill_dir: &Path) -> Vec<String> {
    let scripts_dir = skill_dir.join("scripts");
    if !scripts_dir.is_dir() {
        return Vec::new();
    }

    let dependency_manifest = has_dependency_manifest(skill_dir, &scripts_dir);
    let mut advisories = Vec::new();
    collect_script_files(&scripts_dir, &mut Vec::new(), &mut |script_path| {
        let Ok(content) = fs::read_to_string(script_path) else {
            advisories.push(format!(
                "Script '{}' is not valid UTF-8 text",
                display_relative(script_path, skill_dir)
            ));
            return;
        };

        let relative = display_relative(script_path, skill_dir);
        advisories.extend(validate_script_content(
            &relative,
            &content,
            dependency_manifest,
        ));
    });

    advisories
}

fn has_dependency_manifest(skill_dir: &Path, scripts_dir: &Path) -> bool {
    [
        skill_dir.join("requirements.txt"),
        skill_dir.join("package.json"),
        skill_dir.join("Pipfile"),
        skill_dir.join("Cargo.toml"),
        skill_dir.join("go.mod"),
        scripts_dir.join("requirements.txt"),
        scripts_dir.join("package.json"),
        scripts_dir.join("README.md"),
        scripts_dir.join("README"),
    ]
    .iter()
    .any(|path| path.is_file())
}

fn collect_script_files(dir: &Path, files: &mut Vec<PathBuf>, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_script_files(&path, files, visit);
            continue;
        }

        if is_script_file(&path) {
            files.push(path.clone());
            visit(&path);
        }
    }
}

fn is_script_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("sh" | "bash" | "py" | "js" | "mjs" | "cjs")
    )
}

fn validate_script_content(
    relative_path: &str,
    content: &str,
    dependency_manifest: bool,
) -> Vec<String> {
    let mut advisories = Vec::new();
    let language = detect_language(relative_path);
    let header = header_text(content);
    let dependency_documented =
        dependency_manifest || documents_dependencies(&header, content, language);

    if !dependency_documented {
        advisories.push(format!(
            "Script '{relative_path}' should be self-contained or document dependencies in a header comment or skill manifest (requirements.txt, package.json, scripts/README.md)"
        ));
    }

    if !is_trivial_script(content) && !includes_helpful_errors(content, language) {
        advisories.push(format!(
            "Script '{relative_path}' should include helpful error messages (for example stderr output, explicit failures, or structured error handling)"
        ));
    }

    if !is_trivial_script(content) && !handles_edge_cases(content, language) {
        advisories.push(format!(
            "Script '{relative_path}' should handle edge cases gracefully (for example validate arguments before use)"
        ));
    }

    advisories
}

fn detect_language(relative_path: &str) -> ScriptLanguage {
    if relative_path.ends_with(".py") {
        ScriptLanguage::Python
    } else if relative_path.ends_with(".js")
        || relative_path.ends_with(".mjs")
        || relative_path.ends_with(".cjs")
    {
        ScriptLanguage::JavaScript
    } else {
        ScriptLanguage::Bash
    }
}

#[derive(Clone, Copy)]
enum ScriptLanguage {
    Bash,
    Python,
    JavaScript,
}

fn header_text(content: &str) -> String {
    content
        .lines()
        .take(HEADER_SCAN_LINES)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

fn documents_dependencies(header: &str, content: &str, language: ScriptLanguage) -> bool {
    if header.contains("dependencies")
        || header.contains("requires")
        || header.contains("requirement")
        || header.contains("prerequisite")
        || header.contains("needs:")
    {
        return true;
    }

    match language {
        ScriptLanguage::Bash => {
            !uses_external_bash_dependencies(content)
                || header.contains("requires:")
                || header.contains("needs ")
        }
        ScriptLanguage::Python => !uses_third_party_python_imports(content),
        ScriptLanguage::JavaScript => !uses_external_javascript_imports(content),
    }
}

fn uses_external_bash_dependencies(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("source ")
            || trimmed.starts_with(". ")
            || trimmed.contains(" curl ")
            || trimmed.contains(" docker ")
            || trimmed.contains(" jq ")
            || trimmed.contains(" npm ")
            || trimmed.contains(" pip ")
            || trimmed.contains(" python3 ")
            || trimmed.contains(" node ")
    })
}

fn uses_third_party_python_imports(content: &str) -> bool {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            let module = trimmed
                .trim_start_matches("import ")
                .split([' ', ','])
                .next()
                .unwrap_or_default()
                .split('.')
                .next()
                .unwrap_or_default();
            if !module.is_empty() && !is_python_stdlib(module) {
                return true;
            }
        }

        if let Some(module) = trimmed.strip_prefix("from ") {
            let module = module.split_whitespace().next().unwrap_or_default();
            if !module.is_empty() && !module.starts_with('.') && !is_python_stdlib(module) {
                return true;
            }
        }
    }

    false
}

fn is_python_stdlib(module: &str) -> bool {
    matches!(
        module,
        "os" | "sys"
            | "json"
            | "pathlib"
            | "re"
            | "math"
            | "datetime"
            | "typing"
            | "collections"
            | "itertools"
            | "functools"
            | "subprocess"
            | "shutil"
            | "tempfile"
            | "textwrap"
            | "argparse"
            | "logging"
            | "io"
            | "csv"
            | "hashlib"
            | "base64"
            | "uuid"
            | "time"
            | "random"
            | "statistics"
            | "dataclasses"
            | "enum"
            | "contextlib"
            | "unittest"
    )
}

fn uses_external_javascript_imports(content: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        (trimmed.starts_with("import ") && !trimmed.contains("./") && !trimmed.contains("../"))
            || (trimmed.starts_with("require(")
                && !trimmed.contains("./")
                && !trimmed.contains("../"))
    })
}

fn includes_helpful_errors(content: &str, language: ScriptLanguage) -> bool {
    match language {
        ScriptLanguage::Bash => {
            content.contains(">&2")
                || content.contains("|| echo")
                || content.contains("|| {")
                || content.contains("set -e")
                || content.contains("set -o pipefail")
                || content.contains("set -euo pipefail")
                || content.contains("set -eu")
                || content.contains("trap ")
                || content.contains("die(")
                || content.contains("error(")
                || content.contains("err(")
        }
        ScriptLanguage::Python => {
            content.contains("except ")
                || content.contains("raise ")
                || content.contains("sys.stderr")
                || content.contains("file=sys.stderr")
                || content.contains("logging.error")
                || content.contains("logging.exception")
        }
        ScriptLanguage::JavaScript => {
            content.contains("console.error")
                || content.contains("throw new Error")
                || content.contains("process.exit(")
        }
    }
}

fn handles_edge_cases(content: &str, language: ScriptLanguage) -> bool {
    match language {
        ScriptLanguage::Bash => {
            let uses_positional_args =
                content.contains("$1") || content.contains("\"$@\"") || content.contains("$*");
            if !uses_positional_args {
                return true;
            }

            content.contains("$#")
                || content.contains("[ $#")
                || content.contains("[[ $#")
                || content.contains("${1:?")
                || content.contains("getopts")
                || content.contains(": ${1:?")
        }
        ScriptLanguage::Python => {
            if !content.contains("sys.argv") {
                return true;
            }

            content.contains("len(sys.argv")
                || content.contains("argparse")
                || content.contains("if __name__") && content.contains("ArgumentParser")
        }
        ScriptLanguage::JavaScript => {
            if !content.contains("process.argv") {
                return true;
            }

            content.contains("process.argv.length")
                || content.contains("yargs")
                || content.contains("commander")
                || content.contains("minimist")
        }
    }
}

fn is_trivial_script(content: &str) -> bool {
    let non_empty_lines: Vec<_> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();

    non_empty_lines.len() <= TRIVIAL_SCRIPT_MAX_LINES
        && non_empty_lines.iter().all(|line| {
            line.starts_with("echo ")
                || line.starts_with("printf ")
                || line.starts_with("console.log(")
                || *line == "echo"
        })
}

fn display_relative(path: &Path, skill_dir: &Path) -> String {
    path.strip_prefix(skill_dir)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn warns_on_script_missing_error_handling_and_arg_checks() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("demo");
        let scripts_dir = skill_dir.join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::write(
            scripts_dir.join("run.sh"),
            "#!/usr/bin/env bash\ninput=\"$1\"\necho \"$input\"\n",
        )
        .unwrap();

        let advisories = validate_scripts_advisories(&skill_dir);
        assert!(
            advisories
                .iter()
                .any(|message| message.contains("error messages"))
        );
        assert!(
            advisories
                .iter()
                .any(|message| message.contains("edge cases"))
        );
    }

    #[test]
    fn accepts_documented_dependency_and_error_handling() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("demo");
        let scripts_dir = skill_dir.join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::write(
            scripts_dir.join("run.sh"),
            "#!/usr/bin/env bash\n# Requires: jq\nset -euo pipefail\nif [ $# -lt 1 ]; then echo \"usage\" >&2; exit 1; fi\necho \"$1\"\n",
        )
        .unwrap();

        let advisories = validate_scripts_advisories(&skill_dir);
        assert!(advisories.is_empty());
    }

    #[test]
    fn allows_trivial_echo_script() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("demo");
        let scripts_dir = skill_dir.join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::write(scripts_dir.join("hello.sh"), "echo \"hello\"\n").unwrap();

        let advisories = validate_scripts_advisories(&skill_dir);
        assert!(advisories.is_empty());
    }
}
