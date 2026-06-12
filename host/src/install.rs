use std::fs;
use std::path::{Path, PathBuf};

use skills_rs::{
    find_skill_md, read_properties, unpack_skill_archive, validate_with_policy,
    NameValidationPolicy,
};

pub fn install_skill_directory(source: &Path, project_root: &Path) -> Result<PathBuf, String> {
    if !source.is_dir() {
        return Err(format!("not a directory: {}", source.display()));
    }

    let errors = validate_with_policy(source, NameValidationPolicy::SkillsRef);
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }

    let properties = read_properties(source).map_err(|error| error.to_string())?;
    let destination = project_root
        .join(".agents/skills")
        .join(&properties.name);

    if destination.exists() {
        return Err(format!(
            "skill '{}' already installed at {}",
            properties.name,
            destination.display()
        ));
    }

    copy_dir_recursive(source, &destination)?;
    Ok(destination)
}

pub fn install_skill_archive(archive_path: &Path, project_root: &Path) -> Result<Vec<PathBuf>, String> {
    if !archive_path.is_file() {
        return Err(format!("not a file: {}", archive_path.display()));
    }

    let bytes = fs::read(archive_path).map_err(|error| error.to_string())?;
    let archive_name = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "archive path has no file name".to_owned())?;

    let extract_root = project_root
        .join(".cache/archive-installs")
        .join(format!("{}-{}", archive_name, std::process::id()));
    if extract_root.exists() {
        fs::remove_dir_all(&extract_root).map_err(|error| error.to_string())?;
    }

    unpack_skill_archive(&bytes, archive_name, &extract_root).map_err(|error| error.to_string())?;

    let mut installed = Vec::new();
    for skill_md in find_skill_files(&extract_root) {
        let skill_dir = skill_md.parent().unwrap_or(skill_md.as_path()).to_path_buf();
        installed.push(install_skill_directory(&skill_dir, project_root)?);
    }

    if installed.is_empty() {
        return Err(format!(
            "no skills found in archive: {}",
            archive_path.display()
        ));
    }

    Ok(installed)
}

pub fn install_git_repository(url: &str, project_root: &Path) -> Result<Vec<PathBuf>, String> {
    if !url.starts_with("https://") {
        return Err("only https:// repository URLs are supported".to_owned());
    }

    if url.contains(char::is_whitespace) {
        return Err("repository URL must not contain whitespace".to_owned());
    }

    let cache_root = project_root.join(".cache/git-installs");
    fs::create_dir_all(&cache_root).map_err(|error| error.to_string())?;

    let repo_name = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("repo")
        .trim_end_matches(".git");
    let clone_dir = cache_root.join(format!("{repo_name}-{}", std::process::id()));

    if clone_dir.exists() {
        fs::remove_dir_all(&clone_dir).map_err(|error| error.to_string())?;
    }

    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", url, &clone_dir.to_string_lossy()])
        .status()
        .map_err(|error| format!("failed to run git: {error}"))?;

    if !status.success() {
        return Err(format!("git clone failed for {url}"));
    }

    let mut installed = Vec::new();
    for skill_md in find_skill_files(&clone_dir) {
        let skill_dir = skill_md.parent().unwrap_or(skill_md.as_path()).to_path_buf();
        installed.push(install_skill_directory(&skill_dir, project_root)?);
    }

    if installed.is_empty() {
        return Err(format!("no skills found in cloned repository: {url}"));
    }

    Ok(installed)
}

fn find_skill_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk_skill_files(root, &mut found);
    found
}

fn walk_skill_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if find_skill_md(&path).is_some() {
                if let Some(skill_md) = find_skill_md(&path) {
                    found.push(skill_md);
                }
                continue;
            }
            walk_skill_files(&path, found);
        }
    }
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;

    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let file_name = entry.file_name();
        let target_path = destination.join(file_name);

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            fs::copy(&source_path, &target_path).map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}
