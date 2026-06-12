use std::env;
use std::path::{Path, PathBuf};

pub fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn xdg_skills_dir() -> Option<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".config")))?;
    let path = config_home.join("agents/skills");
    path.is_dir().then_some(path)
}

pub fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

pub fn ancestor_directories(start: &Path) -> Vec<PathBuf> {
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let git_root = find_git_root(&start).unwrap_or_else(|| start.clone());
    let mut directories = Vec::new();
    let mut current = start;

    loop {
        directories.push(current.clone());
        if current == git_root {
            break;
        }
        if !current.pop() {
            break;
        }
    }

    directories
}

pub fn global_gitignore_paths() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };

    let mut paths = Vec::new();
    for candidate in [
        home.join(".config/git/ignore"),
        home.join(".gitignore_global"),
    ] {
        if candidate.is_file() {
            paths.push(candidate);
        }
    }

    if let Some(excludes_file) = read_core_excludesfile(&home)
        && excludes_file.is_file()
    {
        paths.push(excludes_file);
    }

    paths
}

fn read_core_excludesfile(home: &Path) -> Option<PathBuf> {
    for config_path in [home.join(".gitconfig"), home.join(".config/git/config")] {
        let content = std::fs::read_to_string(&config_path).ok()?;
        let mut in_core = false;
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_core = line == "[core]";
                continue;
            }
            if !in_core {
                continue;
            }
            let Some(rest) = line.strip_prefix("excludesfile") else {
                continue;
            };
            let value = rest.trim().trim_start_matches('=').trim();
            let value = value.trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(expand_home(value, home));
            }
        }
    }

    None
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest)
    } else if path == "~" {
        home.to_path_buf()
    } else {
        PathBuf::from(path)
    }
}
