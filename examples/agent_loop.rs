use std::env;
use std::path::PathBuf;

use skills_rs::{
    ActivationMode, ActivationOptions, CompositeFilter, DiscoveryConfig, SkillCatalog,
    SkillSession, discover_and_merge,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap());
    let skill_name = env::args().nth(2);

    let home = dirs_home();
    let config = DiscoveryConfig {
        project_roots: vec![project_root.clone()],
        user_roots: vec![home.join(".agents/skills")],
        include_agents_convention: true,
        respect_gitignore: true,
        ..Default::default()
    };

    let (entries, diagnostics, collisions) = discover_and_merge(config)?;
    for diagnostic in diagnostics {
        eprintln!("[{:?}] {}", diagnostic.severity, diagnostic.message);
    }
    for collision in collisions {
        eprintln!(
            "collision: {} kept {} shadowed {}",
            collision.name,
            collision.kept_location.display(),
            collision.shadowed_location.display()
        );
    }

    let filter = CompositeFilter::catalog_default();
    let catalog = SkillCatalog::from_entries_filtered(entries, &filter);

    if let Some(disclosure) = catalog.disclosure_block(ActivationMode::default()) {
        println!("=== system prompt skills block ===\n{disclosure}");
    } else {
        println!("No skills available for this session.");
        return Ok(());
    }

    let target = skill_name
        .as_deref()
        .or_else(|| catalog.names().next())
        .ok_or("no skill name available")?;

    let mut session = SkillSession::new();
    let activation_options = ActivationOptions::default();

    let Some(entry) = catalog.get(target) else {
        return Err(format!("skill not found: {target}").into());
    };

    if let Some(activated) = session.try_activate(entry, activation_options, &filter)? {
        println!("\n=== activated skill: {target} ===\n{}", activated.content);
    } else {
        println!("skill '{target}' already active");
    }

    Ok(())
}

fn dirs_home() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}
