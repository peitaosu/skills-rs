use std::fs;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use tar::Archive as TarArchive;

use crate::parse::find_skill_md;
use crate::{Result, SkillError};

const MAX_SKILL_SEARCH_DEPTH: usize = 8;

pub fn unpack_skill_archive(bytes: &[u8], archive_name: &str, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;

    if archive_name.ends_with(".tar.gz") || archive_name.ends_with(".tgz") {
        let decoder = GzDecoder::new(Cursor::new(bytes));
        let mut archive = TarArchive::new(decoder);
        archive.unpack(destination).map_err(|error| SkillError::Other {
            message: format!("Failed to unpack tar.gz archive: {error}"),
        })?;
        return Ok(());
    }

    if archive_name.ends_with(".tar") {
        let mut archive = TarArchive::new(Cursor::new(bytes));
        archive.unpack(destination).map_err(|error| SkillError::Other {
            message: format!("Failed to unpack tar archive: {error}"),
        })?;
        return Ok(());
    }

    if archive_name.ends_with(".zip") {
        unpack_zip(bytes, destination)?;
        return Ok(());
    }

    Err(SkillError::Other {
        message: format!(
            "Unsupported archive format for '{archive_name}'; use .tar.gz, .tar, or .zip"
        ),
    })
}

fn unpack_zip(bytes: &[u8], destination: &Path) -> Result<()> {
    let reader = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|error| SkillError::Other {
            message: format!("Failed to open zip archive: {error}"),
        })?;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| SkillError::Other {
            message: format!("Failed to read zip entry: {error}"),
        })?;
        let enclosed_name = file.enclosed_name().ok_or_else(|| SkillError::Other {
            message: "Zip entry has an unsafe path".to_owned(),
        })?;

        if !is_safe_relative_path(enclosed_name.as_path()) {
            return Err(SkillError::Other {
                message: format!(
                    "Zip entry escapes destination: {}",
                    enclosed_name.display()
                ),
            });
        }

        let outpath = destination.join(enclosed_name.as_path());
        if file.is_dir() {
            fs::create_dir_all(&outpath).map_err(|error| SkillError::Other {
                message: format!("Failed to create directory from zip: {error}"),
            })?;
            continue;
        }

        if let Some(parent) = outpath.parent() {
            fs::create_dir_all(parent).map_err(|error| SkillError::Other {
                message: format!("Failed to create parent directory from zip: {error}"),
            })?;
        }

        let mut outfile = fs::File::create(&outpath).map_err(|error| SkillError::Other {
            message: format!("Failed to create zip output file: {error}"),
        })?;
        std::io::copy(&mut file, &mut outfile).map_err(|error| SkillError::Other {
            message: format!("Failed to extract zip entry: {error}"),
        })?;
    }

    Ok(())
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
}

pub fn resolve_skill_dir_after_unpack(root: &Path) -> Result<PathBuf> {
    if find_skill_md(root).is_some() {
        return Ok(root.to_path_buf());
    }

    search_skill_dir(root, 0).ok_or_else(|| SkillError::Other {
        message: format!("No SKILL.md found under {}", root.display()),
    })
}

fn search_skill_dir(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth > MAX_SKILL_SEARCH_DEPTH {
        return None;
    }

    if find_skill_md(dir).is_some() {
        return Some(dir.to_path_buf());
    }

    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if let Some(found) = search_skill_dir(&path, depth + 1) {
            return Some(found);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::find_skill_md;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn unpacks_tar_gz_and_plain_tar_and_zip() {
        let dir = TempDir::new().unwrap();
        let skill_root = dir.path().join("payload/my-skill");
        fs::create_dir_all(&skill_root).unwrap();
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: my-skill\ndescription: archived\n---\nBody\n",
        )
        .unwrap();

        let tar_path = dir.path().join("skill.tar");
        {
            let file = fs::File::create(&tar_path).unwrap();
            let mut builder = tar::Builder::new(file);
            builder.append_dir_all("payload", dir.path().join("payload")).unwrap();
            builder.finish().unwrap();
        }

        let tar_bytes = fs::read(&tar_path).unwrap();
        let tar_dest = dir.path().join("out-tar");
        unpack_skill_archive(&tar_bytes, "skill.tar", &tar_dest).unwrap();
        assert!(find_skill_md(&tar_dest.join("payload/my-skill")).is_some());

        let tgz_path = dir.path().join("skill.tar.gz");
        {
            let file = fs::File::create(&tgz_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            builder.append_dir_all("payload", dir.path().join("payload")).unwrap();
            builder.finish().unwrap();
        }

        let tgz_bytes = fs::read(&tgz_path).unwrap();
        let tgz_dest = dir.path().join("out-tgz");
        unpack_skill_archive(&tgz_bytes, "skill.tar.gz", &tgz_dest).unwrap();
        assert!(find_skill_md(&tgz_dest.join("payload/my-skill")).is_some());

        let zip_path = dir.path().join("skill.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            let skill_md = dir.path().join("payload/my-skill/SKILL.md");
            let content = fs::read(&skill_md).unwrap();
            writer
                .start_file("payload/my-skill/SKILL.md", options)
                .unwrap();
            writer.write_all(&content).unwrap();
            writer.finish().unwrap();
        }

        let zip_bytes = fs::read(&zip_path).unwrap();
        let zip_dest = dir.path().join("out-zip");
        unpack_skill_archive(&zip_bytes, "skill.zip", &zip_dest).unwrap();
        assert!(find_skill_md(&zip_dest.join("payload/my-skill")).is_some());
    }

    #[test]
    fn resolve_skill_dir_finds_nested_layout() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("bundle/demo-skill");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            nested.join("SKILL.md"),
            "---\nname: demo-skill\ndescription: nested\n---\n",
        )
        .unwrap();

        let resolved = resolve_skill_dir_after_unpack(dir.path()).unwrap();
        assert_eq!(resolved, nested);
    }
}
