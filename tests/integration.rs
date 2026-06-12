use std::fs;
use std::path::{Path, PathBuf};

use skills_rs::{
    discover_and_merge, discover_and_merge_from, merge_skills, parse_frontmatter,
    parse_frontmatter_strict, read_properties, to_prompt_from_dirs, validate,
    validate_with_policy, ActivationMode, ActivationOptions, AllowedToolsPolicy, CatalogPlacement,
    CompositeFilter, DiscoveryConfig, FilesystemDiscovery, HideModelDisabled, MultiSkillSource,
    NameValidationPolicy, RemoteRegistrySource, SkillActivator, SkillCatalog, SkillEntry,
    SkillFilter, SkillScope, SkillSession, SkillSource, WrapStyle, AllowAll,
};
use tempfile::TempDir;

fn write_skill(dir: &Path, content: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("SKILL.md"), content).unwrap();
}

#[test]
fn validate_valid_skill() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    write_skill(
        &skill_dir,
        "---\nname: my-skill\ndescription: A test skill\n---\n# My Skill\n",
    );
    assert!(validate(&skill_dir).is_empty());
}

#[test]
fn validate_nonexistent_path() {
    let dir = TempDir::new().unwrap();
    let errors = validate(&dir.path().join("nonexistent"));
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("does not exist"));
}

#[test]
fn validate_not_a_directory() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("file.txt");
    fs::write(&file, "test").unwrap();
    let errors = validate(&file);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Not a directory"));
}

#[test]
fn validate_missing_skill_md() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    fs::create_dir(&skill_dir).unwrap();
    let errors = validate(&skill_dir);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Missing required file"));
}

#[test]
fn validate_uppercase_name_rejected() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("MySkill");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: MySkill\ndescription: A test skill\n---\nBody\n",
    );
    assert!(validate(&skill_dir).iter().any(|error| error.contains("lowercase")));
}

#[test]
fn validate_name_too_long() {
    let dir = TempDir::new().unwrap();
    let long_name = "a".repeat(70);
    let skill_dir = dir.path().join(&long_name);
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        &format!("---\nname: {long_name}\ndescription: A test skill\n---\nBody\n"),
    );
    assert!(
        validate(&skill_dir)
            .iter()
            .any(|error| error.contains("exceeds") && error.contains("character limit"))
    );
}

#[test]
fn validate_name_leading_hyphen() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("-my-skill");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: -my-skill\ndescription: A test skill\n---\nBody\n",
    );
    assert!(
        validate(&skill_dir)
            .iter()
            .any(|error| error.contains("cannot start or end with a hyphen"))
    );
}

#[test]
fn validate_name_consecutive_hyphens() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my--skill");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: my--skill\ndescription: A test skill\n---\nBody\n",
    );
    assert!(
        validate(&skill_dir)
            .iter()
            .any(|error| error.contains("consecutive hyphens"))
    );
}

#[test]
fn validate_name_invalid_characters() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my_skill");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: my_skill\ndescription: A test skill\n---\nBody\n",
    );
    assert!(
        validate(&skill_dir)
            .iter()
            .any(|error| error.contains("invalid characters"))
    );
}

#[test]
fn validate_name_directory_mismatch() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("wrong-name");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: correct-name\ndescription: A test skill\n---\nBody\n",
    );
    assert!(
        validate(&skill_dir)
            .iter()
            .any(|error| error.contains("must match skill name"))
    );
}

#[test]
fn validate_unexpected_fields() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: my-skill\ndescription: A test skill\nunknown_field: x\n---\nBody\n",
    );
    assert!(
        validate(&skill_dir)
            .iter()
            .any(|error| error.contains("Unexpected fields"))
    );
}

#[test]
fn validate_allowed_tools_accepted() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    write_skill(
        &skill_dir,
        "---\nname: my-skill\ndescription: A test skill\nallowed-tools: Bash(jq:*) Bash(git:*)\n---\nBody\n",
    );
    assert!(validate(&skill_dir).is_empty());
}

#[test]
fn validate_i18n_chinese_name() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("技能");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: 技能\ndescription: A skill with Chinese name\n---\nBody\n",
    );
    assert!(validate(&skill_dir).is_empty());
    assert!(!validate_with_policy(&skill_dir, NameValidationPolicy::SpecStrict).is_empty());
}

#[test]
fn validate_spec_strict_rejects_unicode_name() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("技能");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: 技能\ndescription: A skill with Chinese name\n---\nBody\n",
    );
    assert!(
        validate_with_policy(&skill_dir, NameValidationPolicy::SpecStrict)
            .iter()
            .any(|error| error.contains("invalid characters"))
    );
}

#[test]
fn remote_registry_loads_local_manifest_paths() {
    let dir = TempDir::new().unwrap();
    write_skill(
        &dir.path().join("registry-skill"),
        "---\nname: registry-skill\ndescription: from manifest\n---\nBody\n",
    );

    let manifest_path = dir.path().join("manifest.json");
    fs::write(
        &manifest_path,
        r#"{"skills":[{"path":"registry-skill"}]}"#,
    )
    .unwrap();

    let source = RemoteRegistrySource::from_manifest_file(
        &manifest_path,
        dir.path().join("cache"),
        SkillScope::Organization,
    )
    .unwrap();

    let result = skills_rs::discover_from(&source).unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].properties.name, "registry-skill");
}

#[test]
fn validate_i18n_russian_uppercase_rejected() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("НАВЫК");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: НАВЫК\ndescription: A skill with Russian uppercase name\n---\nBody\n",
    );
    assert!(validate(&skill_dir).iter().any(|error| error.contains("lowercase")));
}

#[test]
fn validate_description_too_long() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    fs::create_dir(&skill_dir).unwrap();
    let long_desc = "x".repeat(1100);
    write_skill(
        &skill_dir,
        &format!("---\nname: my-skill\ndescription: {long_desc}\n---\nBody\n"),
    );
    assert!(
        validate(&skill_dir)
            .iter()
            .any(|error| error.contains("exceeds") && error.contains("1024"))
    );
}

#[test]
fn validate_compatibility_too_long() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    fs::create_dir(&skill_dir).unwrap();
    let long_compat = "x".repeat(550);
    write_skill(
        &skill_dir,
        &format!(
            "---\nname: my-skill\ndescription: A test skill\ncompatibility: {long_compat}\n---\nBody\n"
        ),
    );
    assert!(
        validate(&skill_dir)
            .iter()
            .any(|error| error.contains("exceeds") && error.contains("500"))
    );
}

#[test]
fn validate_nfkc_normalization() {
    let dir = TempDir::new().unwrap();
    let decomposed_name = "cafe\u{0301}";
    let composed_name = "café";
    let skill_dir = dir.path().join(composed_name);
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        &format!("---\nname: {decomposed_name}\ndescription: A test skill\n---\nBody\n"),
    );
    assert!(validate(&skill_dir).is_empty(), "{:?}", validate(&skill_dir));
}

#[test]
fn parse_missing_frontmatter() {
    let result = parse_frontmatter_strict("# No frontmatter");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("must start with YAML frontmatter")
    );
}

#[test]
fn parse_unclosed_frontmatter() {
    let content = "---\nname: my-skill\ndescription: A test skill\n";
    let result = parse_frontmatter_strict(content);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("not properly closed")
    );
}

#[test]
fn parse_invalid_yaml_strict() {
    let content = "---\nname: [invalid\ndescription: broken\n---\nBody\n";
    assert!(parse_frontmatter_strict(content).is_err());
}

#[test]
fn parse_lenient_colon_description() {
    let content = "---\nname: my-skill\ndescription: Use this skill when: the user asks about PDFs\n---\nBody\n";
    let (metadata, _) = parse_frontmatter(content).unwrap();
    assert_eq!(
        metadata.get("description").unwrap().as_str().unwrap(),
        "Use this skill when: the user asks about PDFs"
    );
    assert!(parse_frontmatter_strict(content).is_err());
}

#[test]
fn read_properties_with_metadata() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    write_skill(
        &skill_dir,
        "---\nname: my-skill\ndescription: A test skill\nmetadata:\n  author: Test Author\n  version: 1.0\n---\nBody\n",
    );
    let props = read_properties(&skill_dir).unwrap();
    assert_eq!(props.metadata.get("author").map(String::as_str), Some("Test Author"));
    assert_eq!(props.metadata.get("version").map(String::as_str), Some("1.0"));
}

#[test]
fn read_properties_with_allowed_tools() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    write_skill(
        &skill_dir,
        "---\nname: my-skill\ndescription: A test skill\nallowed-tools: Bash(jq:*) Bash(git:*)\n---\nBody\n",
    );
    let props = read_properties(&skill_dir).unwrap();
    assert_eq!(props.allowed_tools.as_deref(), Some("Bash(jq:*) Bash(git:*)"));
}

#[test]
fn to_prompt_empty_list() {
    assert_eq!(
        to_prompt_from_dirs([]).unwrap(),
        "<available_skills>\n</available_skills>"
    );
}

#[test]
fn to_prompt_special_characters_escaped() {
    let dir = TempDir::new().unwrap();
    write_skill(
        &dir.path().join("special-skill"),
        "---\nname: special-skill\ndescription: Use <foo> & <bar> tags\n---\nBody\n",
    );
    let result = to_prompt_from_dirs([dir.path().join("special-skill").as_path()]).unwrap();
    assert!(result.contains("&lt;foo&gt;"));
    assert!(result.contains("&amp;"));
    assert!(!result.contains("<foo>"));
}

#[test]
fn validate_valid_compatibility() {
    let dir = TempDir::new().unwrap();
    write_skill(
        &dir.path().join("my-skill"),
        "---\nname: my-skill\ndescription: A test skill\ncompatibility: Requires Python 3.11+\n---\nBody\n",
    );
    assert!(validate(&dir.path().join("my-skill")).is_empty());
}

#[test]
fn validate_empty_compatibility_rejected() {
    let dir = TempDir::new().unwrap();
    write_skill(
        &dir.path().join("my-skill"),
        "---\nname: my-skill\ndescription: A test skill\ncompatibility: \"\"\n---\nBody\n",
    );
    assert!(
        validate(&dir.path().join("my-skill"))
            .iter()
            .any(|error| error.contains("compatibility"))
    );
}

#[test]
fn validate_valid_with_all_fields() {
    let dir = TempDir::new().unwrap();
    write_skill(
        &dir.path().join("my-skill"),
        "---\nname: my-skill\ndescription: A test skill\nlicense: MIT\nmetadata:\n  author: Test\n---\nBody\n",
    );
    assert!(validate(&dir.path().join("my-skill")).is_empty());
}

#[test]
fn validate_i18n_russian_name_with_hyphens() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("мой-навык");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: мой-навык\ndescription: A skill with Russian name\n---\nBody\n",
    );
    assert!(validate(&skill_dir).is_empty());
}

#[test]
fn validate_i18n_russian_lowercase_valid() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("навык");
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        "---\nname: навык\ndescription: A skill with Russian lowercase name\n---\nBody\n",
    );
    assert!(validate(&skill_dir).is_empty());
}

#[test]
fn discover_lenient_warns_on_name_too_long() {
    let dir = TempDir::new().unwrap();
    let long_name = "a".repeat(70);
    let skill_dir = dir.path().join(&long_name);
    fs::create_dir(&skill_dir).unwrap();
    write_skill(
        &skill_dir,
        &format!("---\nname: {long_name}\ndescription: long name skill\n---\nBody\n"),
    );

    let config = DiscoveryConfig {
        bundled_roots: vec![dir.path().to_path_buf()],
        ..Default::default()
    };
    let (entries, diagnostics, _) = discover_and_merge(config).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("character limit"))
    );
}

#[test]
fn hide_model_disabled_from_catalog() {
    let entry = SkillEntry {
        properties: skills_rs::SkillProperties {
            name: "manual-only".to_owned(),
            description: "manual".to_owned(),
            license: None,
            compatibility: None,
            allowed_tools: None,
            metadata: [(
                skills_rs::METADATA_DISABLE_MODEL_INVOCATION.to_owned(),
                "true".to_owned(),
            )]
            .into(),
        },
        location: PathBuf::from("/tmp/manual-only/SKILL.md"),
        skill_dir: PathBuf::from("/tmp/manual-only"),
        scope: SkillScope::User,
        source_root: PathBuf::from("/tmp"),
    };

    let catalog = SkillCatalog::from_entries_filtered(vec![entry], &HideModelDisabled);
    assert!(catalog.is_empty());
}

#[test]
fn catalog_omits_empty_disclosure() {
    let catalog = SkillCatalog::from_entries([]);
    assert!(catalog.to_prompt_xml().is_none());
}

#[test]
fn catalog_placement_blocks() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("demo");
    write_skill(
        &skill_dir,
        "---\nname: demo\ndescription: Demo skill\n---\n# Body\n",
    );

    let entry = SkillEntry {
        properties: read_properties(&skill_dir).unwrap(),
        location: skill_dir.join("SKILL.md"),
        skill_dir: skill_dir.clone(),
        scope: SkillScope::Bundled,
        source_root: dir.path().to_path_buf(),
    };

    let catalog = SkillCatalog::from_entries([entry]);
    let mode = ActivationMode::DedicatedTool {
        tool_name: "activate_skill",
    };

    let prompt_block = catalog
        .system_prompt_block(mode, CatalogPlacement::SystemPrompt)
        .unwrap();
    assert!(prompt_block.contains("<available_skills>"));

    let tool_only = catalog
        .system_prompt_block(mode, CatalogPlacement::ToolDescription)
        .unwrap();
    assert!(!tool_only.contains("<available_skills>"));

    let tool_block = catalog
        .tool_description_block(
            "activate_skill",
            CatalogPlacement::ToolDescription,
            &["read_file(path: string)", "run_script(skill: string, script: string)"],
        )
        .unwrap();
    assert!(tool_block.contains("<available_skills>"));
}

#[test]
fn allowed_tools_policy_gates_scripts() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("demo");
    write_skill(
        &skill_dir,
        "---\nname: demo\ndescription: Demo\nallowed-tools: Bash(hello.sh) Read(references:*)\n---\n",
    );

    let entry = SkillEntry {
        properties: read_properties(&skill_dir).unwrap(),
        location: skill_dir.join("SKILL.md"),
        skill_dir,
        scope: SkillScope::Bundled,
        source_root: dir.path().to_path_buf(),
    };

    assert!(AllowedToolsPolicy::allows_script(&entry, "hello.sh", None));
    assert!(!AllowedToolsPolicy::allows_script(&entry, "other.sh", None));
    assert!(!AllowedToolsPolicy::allows_read_path(
        &entry,
        std::path::Path::new("scripts/run.sh")
    ));
    assert!(AllowedToolsPolicy::allows_read_path(
        &entry,
        std::path::Path::new("references/guide.md")
    ));
}

#[test]
fn body_advisories_warn_on_spec_violations() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("demo");
    write_skill(
        &skill_dir,
        &format!(
            "---\nname: demo\ndescription: Demo skill\n---\n{}\nSee [missing](references/MISSING.md)",
            "line\n".repeat(501)
        ),
    );

    let advisories = skills_rs::validate_advisories(&skill_dir);
    assert!(advisories.iter().any(|message| message.contains("lines")));
    assert!(advisories
        .iter()
        .any(|message| message.contains("references/MISSING.md")));
}

#[test]
fn script_advisories_warn_on_unsafe_script() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("demo");
    let scripts_dir = skill_dir.join("scripts");
    fs::create_dir_all(&scripts_dir).unwrap();
    fs::write(
        scripts_dir.join("run.sh"),
        "#!/usr/bin/env bash\ninput=\"$1\"\necho \"$input\"\n",
    )
    .unwrap();
    write_skill(
        &skill_dir,
        "---\nname: demo\ndescription: Demo skill\n---\n",
    );

    let advisories = skills_rs::validate_advisories(&skill_dir);
    assert!(advisories.iter().any(|message| message.contains("edge cases")));
}

#[test]
fn activation_strips_frontmatter_and_lists_resources() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    write_skill(
        &skill_dir,
        "---\nname: my-skill\ndescription: A test skill\n---\n# Instructions\n",
    );
    fs::create_dir(skill_dir.join("scripts")).unwrap();
    fs::write(skill_dir.join("scripts/run.sh"), "echo hi").unwrap();

    let entry = SkillEntry {
        properties: read_properties(&skill_dir).unwrap(),
        location: skill_dir.join("SKILL.md"),
        skill_dir: skill_dir.clone(),
        scope: SkillScope::Project,
        source_root: dir.path().to_path_buf(),
    };

    let activated = SkillActivator::activate(
        &entry,
        ActivationOptions {
            strip_frontmatter: true,
            wrap: WrapStyle::SkillContent,
            list_resources: true,
            max_listed_resources: 10,
        },
    )
    .unwrap();

    assert!(activated.content.contains("# Instructions"));
    assert!(!activated.content.contains("name: my-skill"));
    assert!(activated.resources.iter().any(|path| path.ends_with("run.sh")));
}

#[test]
fn session_deduplicates_activation() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("my-skill");
    write_skill(
        &skill_dir,
        "---\nname: my-skill\ndescription: A test skill\n---\nBody\n",
    );

    let entry = SkillEntry {
        properties: read_properties(&skill_dir).unwrap(),
        location: skill_dir.join("SKILL.md"),
        skill_dir: skill_dir.clone(),
        scope: SkillScope::User,
        source_root: dir.path().to_path_buf(),
    };

    let mut session = SkillSession::new();
    let options = ActivationOptions {
        wrap: WrapStyle::None,
        ..Default::default()
    };

    assert!(session
        .try_activate(&entry, options.clone(), &AllowAll)
        .unwrap()
        .is_some());
    assert!(session
        .try_activate(&entry, options, &AllowAll)
        .unwrap()
        .is_none());
}

#[test]
fn discover_project_over_user_and_bundled() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let project = dir.path().join("project");
    write_skill(
        &home.join(".agents/skills/shared-skill"),
        "---\nname: shared-skill\ndescription: user version\n---\nBody\n",
    );
    write_skill(
        &project.join(".agents/skills/shared-skill"),
        "---\nname: shared-skill\ndescription: project version\n---\nBody\n",
    );
    write_skill(
        &dir.path().join("builtin-skill"),
        "---\nname: builtin-skill\ndescription: bundled\n---\nBody\n",
    );

    let config = DiscoveryConfig {
        project_roots: vec![project],
        user_roots: vec![home],
        bundled_roots: vec![dir.path().to_path_buf()],
        include_agents_convention: true,
        ..Default::default()
    };

    let (entries, _, _) = discover_and_merge(config).unwrap();
    assert_eq!(entries.len(), 2);
    let shared = entries.iter().find(|entry| entry.properties.name == "shared-skill").unwrap();
    assert_eq!(shared.properties.description, "project version");
}

#[test]
fn merge_skills_same_scope_last_wins_with_warning() {
    let dir = TempDir::new().unwrap();
    let first = SkillEntry {
        properties: skills_rs::SkillProperties {
            name: "dup".to_owned(),
            description: "first".to_owned(),
            license: None,
            compatibility: None,
            allowed_tools: None,
            metadata: Default::default(),
        },
        location: dir.path().join("a/SKILL.md"),
        skill_dir: dir.path().join("a"),
        scope: SkillScope::Project,
        source_root: dir.path().to_path_buf(),
    };
    let second = SkillEntry {
        properties: skills_rs::SkillProperties {
            description: "second".to_owned(),
            ..first.properties.clone()
        },
        location: dir.path().join("b/SKILL.md"),
        skill_dir: dir.path().join("b"),
        ..first.clone()
    };

    let (merged, warnings) = merge_skills(vec![first, second]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].properties.description, "second");
    assert_eq!(warnings.len(), 1);
}


#[test]
fn respect_gitignore_skips_ignored_skill() {
    let dir = TempDir::new().unwrap();
    let skills_root = dir.path().join("skills");
    fs::create_dir(&skills_root).unwrap();
    fs::write(skills_root.join(".gitignore"), "ignored/\n").unwrap();
    write_skill(
        &skills_root.join("visible-skill"),
        "---\nname: visible-skill\ndescription: visible\n---\nBody\n",
    );
    write_skill(
        &skills_root.join("ignored/hidden-skill"),
        "---\nname: hidden-skill\ndescription: hidden\n---\nBody\n",
    );

    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&skills_root)
        .output()
        .expect("git init");

    let without = DiscoveryConfig {
        bundled_roots: vec![skills_root.clone()],
        respect_gitignore: false,
        ..Default::default()
    };
    let (entries, _, _) = discover_and_merge(without).unwrap();
    assert_eq!(entries.len(), 2);

    let with = DiscoveryConfig {
        bundled_roots: vec![skills_root],
        respect_gitignore: true,
        ..Default::default()
    };
    let (entries, _, _) = discover_and_merge(with).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].properties.name, "visible-skill");
}

#[test]
fn composite_filter_applies_all_filters() {
    let entry = SkillEntry {
        properties: skills_rs::SkillProperties {
            name: "project-skill".to_owned(),
            description: "desc".to_owned(),
            license: None,
            compatibility: None,
            allowed_tools: None,
            metadata: Default::default(),
        },
        location: PathBuf::from("/tmp/project/skill/SKILL.md"),
        skill_dir: PathBuf::from("/tmp/project/skill"),
        scope: SkillScope::Project,
        source_root: PathBuf::from("/tmp/project"),
    };

    let filter = CompositeFilter::new(vec![
        Box::new(skills_rs::TrustedRootsFilter::new([PathBuf::from("/other")])),
        Box::new(HideModelDisabled),
    ]);

    assert!(!filter.include_in_catalog(&entry));
}

#[test]
fn bash_command_pattern_allows_git_in_script_content() {
    let dir = TempDir::new().unwrap();
    let skill_dir = dir.path().join("demo");
    write_skill(
        &skill_dir,
        "---\nname: demo\ndescription: Demo\nallowed-tools: Bash(git:*)\n---\n",
    );

    let entry = SkillEntry {
        properties: read_properties(&skill_dir).unwrap(),
        location: skill_dir.join("SKILL.md"),
        skill_dir,
        scope: SkillScope::Bundled,
        source_root: dir.path().to_path_buf(),
    };

    assert!(AllowedToolsPolicy::allows_script(
        &entry,
        "deploy.sh",
        Some("#!/usr/bin/env bash\ngit status\n")
    ));
    assert!(!AllowedToolsPolicy::allows_script(
        &entry,
        "deploy.sh",
        Some("#!/usr/bin/env bash\ncurl https://example.com\n")
    ));
}

#[test]
fn organization_scope_loses_to_project_on_name_collision() {
    let dir = TempDir::new().unwrap();
    let org_root = dir.path().join("org");
    let project_root = dir.path().join("project");
    write_skill(
        &org_root.join("shared-skill"),
        "---\nname: shared-skill\ndescription: organization copy\n---\nOrg body\n",
    );
    write_skill(
        &project_root.join(".agents/skills/shared-skill"),
        "---\nname: shared-skill\ndescription: project copy\n---\nProject body\n",
    );

    let manifest_path = org_root.join("manifest.json");
    fs::write(&manifest_path, r#"{"skills":[{"path":"shared-skill"}]}"#).unwrap();

    let org_source = RemoteRegistrySource::from_manifest_file(
        &manifest_path,
        dir.path().join("cache"),
        SkillScope::Organization,
    )
    .unwrap();

    let filesystem = FilesystemDiscovery::new(DiscoveryConfig {
        project_roots: vec![project_root],
        ..Default::default()
    });

    let sources: Vec<&dyn SkillSource> = vec![&org_source, &filesystem];
    let (entries, _, collisions) =
        discover_and_merge_from(&MultiSkillSource::new(sources)).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].scope, SkillScope::Project);
    assert_eq!(entries[0].properties.description, "project copy");
    assert_eq!(collisions.len(), 1);
    assert_eq!(collisions[0].name, "shared-skill");
}
