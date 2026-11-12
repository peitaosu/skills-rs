use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use super::paths::{find_git_root, global_gitignore_paths};

pub struct GitignoreMatcher {
    matcher: Gitignore,
    source_root: PathBuf,
}

impl GitignoreMatcher {
    pub fn from_root(source_root: &Path) -> Option<Self> {
        let source_root = source_root
            .canonicalize()
            .unwrap_or_else(|_| source_root.to_path_buf());
        let mut builder = GitignoreBuilder::new(&source_root);
        let mut loaded = false;

        for gitignore_path in gitignore_sources(&source_root) {
            if gitignore_path.is_file() && builder.add(&gitignore_path).is_none() {
                loaded = true;
            }
        }

        if !loaded {
            return None;
        }

        Some(Self {
            matcher: builder.build().ok()?,
            source_root,
        })
    }

    pub fn is_ignored(&self, path: &Path) -> bool {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !path.starts_with(&self.source_root) {
            return false;
        }

        let mut current = Some(path.as_path());
        while let Some(current_path) = current {
            if !current_path.starts_with(&self.source_root) {
                break;
            }

            let Ok(relative) = current_path.strip_prefix(&self.source_root) else {
                break;
            };

            let relative = relative
                .strip_prefix(std::path::MAIN_SEPARATOR_STR)
                .unwrap_or(relative);
            if !relative.as_os_str().is_empty()
                && self
                    .matcher
                    .matched(relative, current_path.is_dir())
                    .is_ignore()
            {
                return true;
            }

            if current_path == self.source_root.as_path() {
                break;
            }
            current = current_path.parent();
        }

        false
    }
}

fn gitignore_sources(source_root: &Path) -> Vec<PathBuf> {
    let mut paths = gitignore_chain(source_root);

    if let Some(git_root) = find_git_root(source_root) {
        let info_exclude = git_root.join(".git/info/exclude");
        if info_exclude.is_file() {
            paths.push(info_exclude);
        }
    }

    paths.extend(global_gitignore_paths());
    paths
}

fn gitignore_chain(source_root: &Path) -> Vec<PathBuf> {
    let mut chain = Vec::new();
    let mut current = Some(source_root.to_path_buf());
    while let Some(dir) = current {
        chain.push(dir.join(".gitignore"));
        if let Some(parent) = dir.parent() {
            if parent == dir {
                break;
            }
            current = Some(parent.to_path_buf());
        } else {
            break;
        }
    }
    chain
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn ignores_directory_from_local_gitignore() {
        let dir = TempDir::new().unwrap();
        let skills_root = dir.path().join("skills");
        fs::create_dir(&skills_root).unwrap();
        fs::write(skills_root.join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir_all(skills_root.join("ignored/hidden-skill")).unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&skills_root)
            .output()
            .expect("git init");

        let matcher = GitignoreMatcher::from_root(&skills_root).expect("matcher");
        assert!(matcher.is_ignored(&skills_root.join("ignored")));
        assert!(matcher.is_ignored(&skills_root.join("ignored/hidden-skill")));
        assert!(!matcher.is_ignored(&skills_root.join("visible-skill")));
    }

    #[test]
    fn ignores_directory_from_git_info_exclude() {
        let dir = TempDir::new().unwrap();
        let skills_root = dir.path().join("skills");
        fs::create_dir_all(skills_root.join(".git/info")).unwrap();
        fs::write(skills_root.join(".git/info/exclude"), "excluded/\n").unwrap();
        fs::create_dir_all(skills_root.join("excluded/hidden-skill")).unwrap();
        fs::create_dir(skills_root.join("visible-skill")).unwrap();

        let matcher = GitignoreMatcher::from_root(&skills_root).expect("matcher");
        assert!(matcher.is_ignored(&skills_root.join("excluded")));
        assert!(matcher.is_ignored(&skills_root.join("excluded/hidden-skill")));
        assert!(!matcher.is_ignored(&skills_root.join("visible-skill")));
    }
}
