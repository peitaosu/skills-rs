use std::path::{Path, PathBuf};
use std::process::ExitCode;

use skills_rs::{
    discover_and_merge, format_diagnostics, read_properties, to_prompt_from_dirs,
    validate_advisories, validate_with_policy, DiscoveryConfig, NameValidationPolicy,
};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "skills-rs", about = "Agent Skills reference CLI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum NamePolicyArg {
    SkillsRef,
    Spec,
}

impl From<NamePolicyArg> for NameValidationPolicy {
    fn from(value: NamePolicyArg) -> Self {
        match value {
            NamePolicyArg::SkillsRef => NameValidationPolicy::SkillsRef,
            NamePolicyArg::Spec => NameValidationPolicy::SpecStrict,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    Validate {
        skill_path: PathBuf,
        #[arg(long, value_enum, default_value_t = NamePolicyArg::SkillsRef)]
        name_policy: NamePolicyArg,
    },
    ReadProperties {
        skill_path: PathBuf,
    },
    ToPrompt {
        skill_paths: Vec<PathBuf>,
    },
    Discover {
        #[arg(long)]
        project: Option<PathBuf>,
        #[arg(long)]
        user: Option<PathBuf>,
        #[arg(long)]
        bundled: Option<PathBuf>,
        #[arg(long)]
        org: Option<PathBuf>,
        #[arg(long)]
        client_name: Option<String>,
        #[arg(long)]
        respect_gitignore: bool,
        #[arg(long)]
        ancestor_scan: bool,
        #[arg(long)]
        xdg: bool,
        #[arg(long)]
        claude_compat: bool,
        #[arg(long)]
        agents_convention: bool,
    },
}

struct DiscoverOptions {
    project: Option<PathBuf>,
    user: Option<PathBuf>,
    bundled: Option<PathBuf>,
    org: Option<PathBuf>,
    client_name: Option<String>,
    respect_gitignore: bool,
    ancestor_scan: bool,
    xdg: bool,
    claude_compat: bool,
    agents_convention: bool,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Validate {
            skill_path,
            name_policy,
        } => match run_validate(skill_path, name_policy.into()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(code) => code,
        },
        Command::ReadProperties { skill_path } => match run_read_properties(skill_path) {
            Ok(()) => ExitCode::SUCCESS,
            Err(code) => code,
        },
        Command::ToPrompt { skill_paths } => match run_to_prompt(skill_paths) {
            Ok(()) => ExitCode::SUCCESS,
            Err(code) => code,
        },
        Command::Discover {
            project,
            user,
            bundled,
            org,
            client_name,
            respect_gitignore,
            ancestor_scan,
            xdg,
            claude_compat,
            agents_convention,
        } => match run_discover(DiscoverOptions {
            project,
            user,
            bundled,
            org,
            client_name,
            respect_gitignore,
            ancestor_scan,
            xdg,
            claude_compat,
            agents_convention,
        }) {
            Ok(()) => ExitCode::SUCCESS,
            Err(code) => code,
        },
    }
}

fn run_validate(skill_path: PathBuf, name_policy: NameValidationPolicy) -> Result<(), ExitCode> {
    let skill_dir = resolve_skill_dir(&skill_path);
    let errors = validate_with_policy(&skill_dir, name_policy);
    if errors.is_empty() {
        println!("Valid skill: {}", skill_dir.display());
        for advisory in validate_advisories(&skill_dir) {
            eprintln!("  advisory: {advisory}");
        }
        Ok(())
    } else {
        eprintln!("Validation failed for {}:", skill_dir.display());
        for error in errors {
            eprintln!("  - {error}");
        }
        Err(ExitCode::from(1))
    }
}

fn run_read_properties(skill_path: PathBuf) -> Result<(), ExitCode> {
    let skill_dir = resolve_skill_dir(&skill_path);
    match read_properties(&skill_dir) {
        Ok(properties) => {
            println!("{}", serde_json::to_string_pretty(&properties).unwrap());
            Ok(())
        }
        Err(error) => {
            eprintln!("Error: {error}");
            Err(ExitCode::from(1))
        }
    }
}

fn run_to_prompt(skill_paths: Vec<PathBuf>) -> Result<(), ExitCode> {
    let dirs: Vec<PathBuf> = skill_paths
        .into_iter()
        .map(|path| resolve_skill_dir(&path))
        .collect();

    let dir_refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    match to_prompt_from_dirs(dir_refs) {
        Ok(output) => {
            println!("{output}");
            Ok(())
        }
        Err(error) => {
            eprintln!("Error: {error}");
            Err(ExitCode::from(1))
        }
    }
}

fn run_discover(options: DiscoverOptions) -> Result<(), ExitCode> {
    let config = DiscoveryConfig {
        project_roots: options.project.into_iter().collect(),
        user_roots: options.user.into_iter().collect(),
        bundled_roots: options.bundled.into_iter().collect(),
        organization_roots: options.org.into_iter().collect(),
        client_name: options.client_name,
        respect_gitignore: options.respect_gitignore,
        include_ancestor_scan: options.ancestor_scan,
        include_xdg: options.xdg,
        include_claude_compat: options.claude_compat,
        include_agents_convention: options.agents_convention,
        ..Default::default()
    };

    match discover_and_merge(config) {
        Ok((entries, diagnostics, collisions)) => {
            println!("skills: {}", entries.len());
            for entry in &entries {
                println!(
                    "- {} [{}] {}",
                    entry.properties.name,
                    format!("{:?}", entry.scope).to_lowercase(),
                    entry.location.display()
                );
            }

            if !collisions.is_empty() {
                println!("\ncollisions:");
                for collision in &collisions {
                    println!(
                        "- {} kept {} shadowed {}",
                        collision.name,
                        collision.kept_location.display(),
                        collision.shadowed_location.display()
                    );
                }
            }

            println!("\ndiagnostics:\n{}", format_diagnostics(&diagnostics));
            Ok(())
        }
        Err(error) => {
            eprintln!("Error: {error}");
            Err(ExitCode::from(1))
        }
    }
}

fn resolve_skill_dir(path: &Path) -> PathBuf {
    if path.is_file() && path.file_name().is_some_and(|name| name.eq_ignore_ascii_case("skill.md")) {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}
