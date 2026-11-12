use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::model::{Diagnostic, Severity, SkillDocument, SkillEntry, SkillScope};
use crate::parse::{find_skill_md, parse_frontmatter, read_properties, read_skill_md_content};
use crate::validate::validate_lenient;
use crate::{Result, SkillError};

use super::archive::{resolve_skill_dir_after_unpack, unpack_skill_archive};
use super::{DiscoveryResult, SkillSource};

#[derive(Debug, Clone)]
pub struct RemoteRegistrySource {
    manifest_base: PathBuf,
    manifest: RegistryManifest,
    cache_dir: PathBuf,
    scope: SkillScope,
    client: reqwest::blocking::Client,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistryManifest {
    #[serde(default)]
    pub skills: Vec<RegistrySkillRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegistrySkillRef {
    pub name: Option<String>,
    pub path: Option<String>,
    pub url: Option<String>,
    pub archive_url: Option<String>,
}

impl RemoteRegistrySource {
    pub fn from_manifest_file(
        manifest_path: impl AsRef<Path>,
        cache_dir: impl AsRef<Path>,
        scope: SkillScope,
    ) -> Result<Self> {
        let manifest_path = manifest_path.as_ref();
        let manifest_base = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let manifest_text = fs::read_to_string(manifest_path)?;
        let manifest: RegistryManifest =
            serde_json::from_str(&manifest_text).map_err(|error| SkillError::Other {
                message: format!("Invalid registry manifest JSON: {error}"),
            })?;

        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            manifest_base,
            manifest,
            cache_dir: cache_dir.as_ref().to_path_buf(),
            scope,
            client: reqwest::blocking::Client::builder()
                .https_only(true)
                .build()
                .map_err(|error| SkillError::Other {
                    message: format!("Failed to create HTTP client: {error}"),
                })?,
        })
    }

    pub fn from_registry_url(
        registry_url: &str,
        cache_dir: impl AsRef<Path>,
        scope: SkillScope,
    ) -> Result<Self> {
        validate_https_url(registry_url)?;

        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .build()
            .map_err(|error| SkillError::Other {
                message: format!("Failed to create HTTP client: {error}"),
            })?;

        let manifest_text = client
            .get(registry_url)
            .send()
            .map_err(|error| SkillError::Other {
                message: format!("Failed to fetch registry manifest: {error}"),
            })?
            .error_for_status()
            .map_err(|error| SkillError::Other {
                message: format!("Registry manifest request failed: {error}"),
            })?
            .text()
            .map_err(|error| SkillError::Other {
                message: format!("Failed to read registry manifest body: {error}"),
            })?;

        let manifest: RegistryManifest =
            serde_json::from_str(&manifest_text).map_err(|error| SkillError::Other {
                message: format!("Invalid registry manifest JSON: {error}"),
            })?;

        fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            manifest_base: cache_dir.as_ref().to_path_buf(),
            manifest,
            cache_dir: cache_dir.as_ref().to_path_buf(),
            scope,
            client,
        })
    }

    fn provision_entries(&self) -> Result<(Vec<SkillEntry>, Vec<Diagnostic>)> {
        let mut entries = Vec::new();
        let mut diagnostics = Vec::new();

        for skill_ref in &self.manifest.skills {
            match self.provision_one(skill_ref) {
                Ok((entry, advisories)) => {
                    entries.push(entry);
                    diagnostics.extend(advisories);
                }
                Err(error) => diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    message: error.to_string(),
                    skill: skill_ref.name.clone(),
                }),
            }
        }

        Ok((entries, diagnostics))
    }

    fn provision_one(&self, skill_ref: &RegistrySkillRef) -> Result<(SkillEntry, Vec<Diagnostic>)> {
        let skill_dir = if let Some(path) = &skill_ref.path {
            self.resolve_local_path(path)?
        } else if let Some(url) = &skill_ref.url {
            self.fetch_skill_md(url, skill_ref.name.as_deref())?
        } else if let Some(archive_url) = &skill_ref.archive_url {
            self.fetch_archive(archive_url, skill_ref.name.as_deref())?
        } else {
            return Err(SkillError::Other {
                message: "Registry skill entry requires path, url, or archive_url".to_owned(),
            });
        };

        let properties = read_properties(&skill_dir)?;
        let skill_md = find_skill_md(&skill_dir).ok_or_else(|| SkillError::Other {
            message: format!("SKILL.md not found in {}", skill_dir.display()),
        })?;

        if let Some(expected_name) = &skill_ref.name
            && expected_name != &properties.name
        {
            return Err(SkillError::Other {
                message: format!(
                    "Registry name '{expected_name}' does not match skill name '{}'",
                    properties.name
                ),
            });
        }

        let advisories = lenient_advisories(&skill_dir, &properties, &skill_md)?;

        Ok((
            SkillEntry {
                properties,
                location: skill_md,
                skill_dir: skill_dir.clone(),
                scope: self.scope,
                source_root: self.cache_dir.clone(),
            },
            advisories,
        ))
    }

    fn resolve_local_path(&self, path: &str) -> Result<PathBuf> {
        let joined = self.manifest_base.join(path);
        let manifest_base = self
            .manifest_base
            .canonicalize()
            .unwrap_or_else(|_| self.manifest_base.clone());
        let canonical = joined.canonicalize().map_err(|error| SkillError::Other {
            message: format!("Registry path not found ({}): {error}", joined.display()),
        })?;

        if !canonical.starts_with(&manifest_base) {
            return Err(SkillError::Other {
                message: format!(
                    "Registry path escapes manifest base: {}",
                    canonical.display()
                ),
            });
        }

        Ok(canonical)
    }

    fn fetch_skill_md(&self, url: &str, name: Option<&str>) -> Result<PathBuf> {
        validate_https_url(url)?;

        let response = self
            .client
            .get(url)
            .send()
            .map_err(|error| SkillError::Other {
                message: format!("Failed to fetch skill URL: {error}"),
            })?
            .error_for_status()
            .map_err(|error| SkillError::Other {
                message: format!("Skill URL request failed: {error}"),
            })?;

        let content = response.text().map_err(|error| SkillError::Other {
            message: format!("Failed to read skill body: {error}"),
        })?;

        let skill_name = name
            .map(str::to_owned)
            .or_else(|| infer_name_from_url(url))
            .ok_or_else(|| SkillError::Other {
                message: format!("Could not infer skill name for URL: {url}"),
            })?;

        let skill_dir = safe_cache_skill_dir(&self.cache_dir, &skill_name)?;
        fs::create_dir_all(&skill_dir)?;
        fs::write(skill_dir.join("SKILL.md"), content)?;
        Ok(skill_dir)
    }

    fn fetch_archive(&self, archive_url: &str, name: Option<&str>) -> Result<PathBuf> {
        validate_https_url(archive_url)?;

        let response = self
            .client
            .get(archive_url)
            .send()
            .map_err(|error| SkillError::Other {
                message: format!("Failed to fetch skill archive: {error}"),
            })?
            .error_for_status()
            .map_err(|error| SkillError::Other {
                message: format!("Skill archive request failed: {error}"),
            })?;

        let bytes = response.bytes().map_err(|error| SkillError::Other {
            message: format!("Failed to read skill archive body: {error}"),
        })?;

        let skill_name = name
            .map(str::to_owned)
            .or_else(|| infer_name_from_url(archive_url))
            .ok_or_else(|| SkillError::Other {
                message: format!("Could not infer skill name for archive URL: {archive_url}"),
            })?;

        let skill_dir = safe_cache_skill_dir(&self.cache_dir, &skill_name)?;
        fs::create_dir_all(&skill_dir)?;

        unpack_skill_archive(bytes.as_ref(), archive_url, &skill_dir)?;

        let resolved_dir = resolve_skill_dir_after_unpack(&skill_dir)?;
        Ok(resolved_dir)
    }
}

impl SkillSource for RemoteRegistrySource {
    fn discover(&self) -> Result<DiscoveryResult> {
        let (entries, diagnostics) = self.provision_entries()?;
        Ok(DiscoveryResult {
            entries,
            diagnostics,
        })
    }
}

fn validate_https_url(url: &str) -> Result<()> {
    if url.starts_with("https://") {
        return Ok(());
    }

    #[cfg(test)]
    if url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:") {
        return Ok(());
    }

    Err(SkillError::Other {
        message: format!("Remote skill URLs must use HTTPS: {url}"),
    })
}

fn lenient_advisories(
    skill_dir: &Path,
    properties: &crate::model::SkillProperties,
    skill_md: &Path,
) -> Result<Vec<Diagnostic>> {
    let content = read_skill_md_content(skill_md)?;
    let (_, body) = parse_frontmatter(&content).map_err(|error| SkillError::Other {
        message: error.to_string(),
    })?;

    let document = SkillDocument {
        properties: properties.clone(),
        body,
        skill_md_path: skill_md.to_path_buf(),
        skill_dir: skill_dir.to_path_buf(),
    };

    Ok(validate_lenient(&document))
}

fn infer_name_from_url(url: &str) -> Option<String> {
    let path = url.rsplit('/').find(|segment| !segment.is_empty())?;
    let stem = path.strip_suffix(".tar.gz").unwrap_or(path);
    let stem = stem.strip_suffix(".tgz").unwrap_or(stem);
    let stem = stem.strip_suffix(".tar").unwrap_or(stem);
    let stem = stem.strip_suffix(".zip").unwrap_or(stem);
    let stem = stem.strip_suffix(".SKILL.md").unwrap_or(stem);
    let stem = stem.strip_suffix(".md").unwrap_or(stem);
    if stem.is_empty() || !is_safe_skill_name(stem) {
        return None;
    }
    Some(stem.to_owned())
}

fn is_safe_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn safe_cache_skill_dir(cache_dir: &Path, name: &str) -> Result<PathBuf> {
    if !is_safe_skill_name(name) {
        return Err(SkillError::Other {
            message: format!("Unsafe skill cache name: {name}"),
        });
    }

    let candidate = cache_dir.join(name);
    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            return Err(SkillError::Other {
                message: format!("Unsafe skill cache path: {}", candidate.display()),
            });
        }
    }

    Ok(candidate)
}

#[cfg(test)]
pub(crate) fn from_registry_url_for_test(
    registry_url: &str,
    cache_dir: impl AsRef<Path>,
    scope: SkillScope,
) -> Result<RemoteRegistrySource> {
    validate_https_url(registry_url)?;

    let client = reqwest::blocking::Client::builder()
        .build()
        .map_err(|error| SkillError::Other {
            message: format!("Failed to create HTTP client: {error}"),
        })?;

    let manifest_text = client
        .get(registry_url)
        .send()
        .map_err(|error| SkillError::Other {
            message: format!("Failed to fetch registry manifest: {error}"),
        })?
        .error_for_status()
        .map_err(|error| SkillError::Other {
            message: format!("Registry manifest request failed: {error}"),
        })?
        .text()
        .map_err(|error| SkillError::Other {
            message: format!("Failed to read registry manifest body: {error}"),
        })?;

    let manifest: RegistryManifest =
        serde_json::from_str(&manifest_text).map_err(|error| SkillError::Other {
            message: format!("Invalid registry manifest JSON: {error}"),
        })?;

    fs::create_dir_all(&cache_dir)?;

    Ok(RemoteRegistrySource {
        manifest_base: cache_dir.as_ref().to_path_buf(),
        manifest,
        cache_dir: cache_dir.as_ref().to_path_buf(),
        scope,
        client,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn test_client() -> reqwest::blocking::Client {
        reqwest::blocking::Client::builder()
            .build()
            .expect("test client")
    }

    fn from_manifest_with_client(
        manifest_path: impl AsRef<Path>,
        cache_dir: impl AsRef<Path>,
        scope: SkillScope,
        client: reqwest::blocking::Client,
    ) -> Result<RemoteRegistrySource> {
        let manifest_path = manifest_path.as_ref();
        let manifest_base = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let manifest_text = fs::read_to_string(manifest_path)?;
        let manifest: RegistryManifest =
            serde_json::from_str(&manifest_text).map_err(|error| SkillError::Other {
                message: format!("Invalid registry manifest JSON: {error}"),
            })?;

        fs::create_dir_all(&cache_dir)?;

        Ok(RemoteRegistrySource {
            manifest_base,
            manifest,
            cache_dir: cache_dir.as_ref().to_path_buf(),
            scope,
            client,
        })
    }

    #[test]
    fn loads_skills_from_local_manifest_paths() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("sample-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: sample-skill\ndescription: from registry\n---\nBody\n",
        )
        .unwrap();

        let manifest_path = dir.path().join("manifest.json");
        fs::write(&manifest_path, r#"{"skills":[{"path":"sample-skill"}]}"#).unwrap();

        let cache_dir = dir.path().join("cache");
        let source = from_manifest_with_client(
            &manifest_path,
            &cache_dir,
            SkillScope::Organization,
            test_client(),
        )
        .unwrap();

        let result = source.discover().unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].properties.name, "sample-skill");
    }

    #[test]
    fn registry_provision_emits_lenient_advisories() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("advisory-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: advisory-skill\ndescription: from registry\n---\n{}",
                "line\n".repeat(501)
            ),
        )
        .unwrap();

        let manifest_path = dir.path().join("manifest.json");
        fs::write(&manifest_path, r#"{"skills":[{"path":"advisory-skill"}]}"#).unwrap();

        let source = from_manifest_with_client(
            &manifest_path,
            dir.path().join("cache"),
            SkillScope::Organization,
            test_client(),
        )
        .unwrap();

        let result = source.discover().unwrap();
        assert_eq!(result.entries.len(), 1);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("lines"))
        );
    }

    #[test]
    fn fetches_skill_from_http_registry_url() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let skill_url = format!("http://127.0.0.1:{port}/remote-skill/SKILL.md");
        let manifest_body =
            format!(r#"{{"skills":[{{"url":"{skill_url}","name":"remote-skill"}}]}}"#);
        let skill_body = "---\nname: remote-skill\ndescription: fetched\n---\nBody\n";

        thread::spawn(move || {
            for mut stream in listener.incoming().take(2).flatten() {
                let mut buffer = [0u8; 4096];
                let read_bytes = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read_bytes]);
                let (status, body) = if request.contains("/manifest.json") {
                    ("200 OK", manifest_body.clone())
                } else if request.contains("/remote-skill/SKILL.md") {
                    ("200 OK", skill_body.to_owned())
                } else {
                    ("404 Not Found", String::new())
                };

                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let dir = TempDir::new().unwrap();
        let source = from_registry_url_for_test(
            &format!("http://127.0.0.1:{port}/manifest.json"),
            dir.path().join("cache"),
            SkillScope::Organization,
        )
        .unwrap();

        let result = source.discover().unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].properties.name, "remote-skill");
    }
}
