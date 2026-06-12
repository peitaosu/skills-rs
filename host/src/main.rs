mod context;
mod host;
mod install;
mod llm;
mod settings;
mod subagent;
mod watch;

use std::path::PathBuf;

use skills_rs::CatalogPlacement;
use clap::Parser;
use host::{prompt_line, trust_prompt_message, SampleHost};
use install::{install_git_repository, install_skill_archive, install_skill_directory};
use settings::HostSettings;

#[derive(Parser)]
#[command(name = "skills", about = "Reference host app for skills-rs")]
struct Args {
    #[arg(long, default_value = ".")]
    project: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let project_root = args.project.canonicalize().unwrap_or(args.project);

    let mut settings = HostSettings {
        project_root: project_root.clone(),
        trusted: false,
        require_activation_consent: true,
        ..Default::default()
    };

    let mut host = SampleHost::bootstrap(settings.clone())?;

    if host.has_untrusted_project_skills() {
        println!("{}\n", trust_prompt_message());
    }

    print_startup(&host);

    println!(
        "commands: /skill-name, skills, activate <name> [--yes], read <path>, run <skill> <script>, \
         llm <message>, llm-run <message>, agent <message>, install <dir>, install-archive <path>, \
         install-git <https-url>, mode tool|file-read, placement prompt|tool, watch on|off, trust on|off, \
         disable|enable|deny <name>, diagnostics, compact, subagent <name> <task>, analytics, reload, quit"
    );

    loop {
        if let Some(message) = host.maybe_reload_on_change()? {
            println!("{message}");
            print_startup(&host);
        }

        let line = prompt_line("skills")?;
        if line.is_empty() {
            continue;
        }

        if line == "quit" || line == "exit" {
            break;
        }

        if let Some(response) = host.slash_command(&line) {
            println!("{response}");
            continue;
        }

        let mut parts = line.split_whitespace();
        let command = parts.next().unwrap_or_default();

        match command {
            "skills" => println!("{}", host.list_skills()),
            "activate" => {
                let name = parts.next().unwrap_or_default();
                let consent = parts.any(|part| part == "--yes");
                println!("{}", host.activate_skill(name, consent));
            }
            "read" => {
                let path = parts.next().unwrap_or_default();
                println!("{}", host.read_file(PathBuf::from(path).as_path()));
            }
            "run" => {
                let skill_name = parts.next().unwrap_or_default();
                let script = parts.next().unwrap_or_default();
                println!("{}", host.run_script(skill_name, script));
            }
            "llm" => {
                let message = parts.collect::<Vec<_>>().join(" ");
                let selection = host.llm_select_skill(&message);
                println!("{}", selection.message);
            }
            "llm-run" => {
                let message = parts.collect::<Vec<_>>().join(" ");
                let selection = host.llm_select_skill(&message);
                println!("{}", selection.message);
                if let Some(name) = selection.skill_name {
                    println!("{}", host.activate_skill(&name, true));
                }
            }
            "agent" => {
                let message = parts.collect::<Vec<_>>().join(" ");
                println!("{}", host.run_llm_agent(&message));
            }
            "install" => {
                let source = parts.next().unwrap_or_default();
                match install_skill_directory(PathBuf::from(source).as_path(), &settings.project_root)
                {
                    Ok(destination) => {
                        host.reload()?;
                        println!("installed skill to {}", destination.display());
                        print_startup(&host);
                    }
                    Err(error) => println!("install failed: {error}"),
                }
            }
            "install-archive" => {
                let source = parts.next().unwrap_or_default();
                match install_skill_archive(PathBuf::from(source).as_path(), &settings.project_root)
                {
                    Ok(paths) => {
                        host.reload()?;
                        for path in paths {
                            println!("installed skill from archive: {}", path.display());
                        }
                        print_startup(&host);
                    }
                    Err(error) => println!("install-archive failed: {error}"),
                }
            }
            "install-git" => {
                let url = parts.next().unwrap_or_default();
                match install_git_repository(url, &settings.project_root) {
                    Ok(paths) => {
                        host.reload()?;
                        for path in paths {
                            println!("installed skill from git: {}", path.display());
                        }
                        print_startup(&host);
                    }
                    Err(error) => println!("install-git failed: {error}"),
                }
            }
            "mode" => {
                let value = parts.next().unwrap_or("tool");
                settings.file_read_mode = value == "file-read";
                host.settings.file_read_mode = settings.file_read_mode;
                print_startup(&host);
            }
            "placement" => {
                let value = parts.next().unwrap_or("prompt");
                settings.catalog_placement = if value == "tool" {
                    CatalogPlacement::ToolDescription
                } else {
                    CatalogPlacement::SystemPrompt
                };
                host.settings.catalog_placement = settings.catalog_placement;
                print_startup(&host);
            }
            "watch" => {
                let value = parts.next().unwrap_or("on");
                let enabled = value != "off";
                settings.watch_reload = enabled;
                host.settings.watch_reload = enabled;
                if let Err(error) = host.set_watch_reload(enabled) {
                    println!("watch error: {error}");
                } else {
                    println!(
                        "watch reload {}",
                        if enabled { "on" } else { "off" }
                    );
                }
            }
            "trust" => {
                let value = parts.next().unwrap_or("on");
                settings.trusted = value != "off";
                host.settings.trusted = settings.trusted;
                host.reload()?;
                println!("trust {}", if settings.trusted { "on" } else { "off" });
                print_startup(&host);
            }
            "disable" => {
                if let Some(name) = parts.next() {
                    settings.disabled_skills.insert(name.to_owned());
                    host.settings.disabled_skills.insert(name.to_owned());
                    host.reload()?;
                    println!("disabled {name}");
                    print_startup(&host);
                }
            }
            "enable" => {
                if let Some(name) = parts.next() {
                    settings.disabled_skills.remove(name);
                    host.settings.disabled_skills.remove(name);
                    host.reload()?;
                    println!("enabled {name}");
                    print_startup(&host);
                }
            }
            "deny" => {
                if let Some(name) = parts.next() {
                    settings.denied_skills.insert(name.to_owned());
                    host.settings.denied_skills.insert(name.to_owned());
                    host.reload()?;
                    println!("denied {name}");
                    print_startup(&host);
                }
            }
            "diagnostics" => println!("{}", host.diagnostics()),
            "compact" => println!("{}", host.compact_context()),
            "subagent" => {
                let name = parts.next().unwrap_or_default();
                let task = parts.collect::<Vec<_>>().join(" ");
                println!("{}", host.run_subagent(name, &task));
            }
            "analytics" => println!("{}", host.analytics()),
            "reload" => {
                host.reload()?;
                println!("reloaded skills");
                print_startup(&host);
            }
            "complete" => {
                let prefix = parts.next().unwrap_or("");
                println!("{}", host.autocomplete_prefix(prefix).join(", "));
            }
            "help" => {
                println!("see README.md for docs and startup command list");
            }
            _ => println!("unknown command: {command}"),
        }
    }

    Ok(())
}

fn print_startup(host: &SampleHost) {
    match host.llm_model() {
        Some(model) => println!("=== llm ===\nmodel: {model}\n"),
        None => println!(
            "=== llm ===\n(unconfigured — set OPENAI_API_KEY for llm/subagent commands)\n"
        ),
    }

    if let Some(prompt) = host.system_prompt() {
        println!("=== system prompt ===\n{prompt}\n");
    } else {
        println!("=== system prompt ===\n(no skills available)\n");
    }

    if let Some(schema) = host.tool_schema() {
        println!("=== tool schema ===\n{schema}\n");
    } else {
        println!("=== tool schema ===\n(not registered — no skills)\n");
    }
}
