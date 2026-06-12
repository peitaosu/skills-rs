use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use skills_rs::{
    discover_and_merge_from, format_diagnostics, resolve_under_skill_dir, ActivationMode,
    ActivationOptions, AllowedToolsPolicy, CompositeFilter, Diagnostic,
    DiscoveryConfig, FilesystemDiscovery, MultiSkillSource, RemoteRegistrySource,
    SkillCatalog, SkillEntry, SkillFilter, SkillScope, SkillSession, SkillSource, WrapStyle,
};

use crate::context::ConversationContext;
use crate::llm::{
    assistant_followup_payload, path_argument, string_argument, tool_result_payload,
    user_message_payload, AgentToolCall, LlmClient,
};
use crate::settings::HostSettings;
use crate::subagent::{run_subagent_task, SubagentHost};
use crate::watch::SkillWatcher;

const MAX_AGENT_TURNS: usize = 10;

pub struct SampleHost {
    pub settings: HostSettings,
    llm: Option<LlmClient>,
    catalog: SkillCatalog,
    entries: Vec<SkillEntry>,
    session: SkillSession,
    context: ConversationContext,
    allowlisted_dirs: HashSet<PathBuf>,
    filter: CompositeFilter,
    registry_source: RemoteRegistrySource,
    filesystem: FilesystemDiscovery,
    watcher: SkillWatcher,
    last_diagnostics: Vec<Diagnostic>,
}

impl SampleHost {
    pub fn bootstrap(settings: HostSettings) -> skills_rs::Result<Self> {
        let host_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let home = skills_rs::home_dir().unwrap_or_else(|| settings.project_root.clone());
        let config = DiscoveryConfig {
            project_roots: vec![settings.project_root.clone()],
            user_roots: vec![home],
            bundled_roots: vec![host_root.join("skills/bundled")],
            include_agents_convention: true,
            include_claude_compat: true,
            include_ancestor_scan: true,
            include_xdg: true,
            respect_gitignore: true,
            client_name: Some("skills".to_owned()),
            ..Default::default()
        };

        let registry_source = RemoteRegistrySource::from_manifest_file(
            host_root.join("registry/manifest.json"),
            host_root.join(".cache/registry"),
            SkillScope::Organization,
        )?;
        let filesystem = FilesystemDiscovery::new(config);
        let sources: Vec<&dyn SkillSource> = vec![&registry_source, &filesystem];
        let (entries, diagnostics, collisions) =
            discover_and_merge_from(&MultiSkillSource::new(sources))?;

        eprintln!("diagnostics:\n{}", format_diagnostics(&diagnostics));
        for collision in &collisions {
            eprintln!(
                "collision: {} kept {}",
                collision.name,
                collision.kept_location.display()
            );
        }

        let filter = CompositeFilter::host_default(
            settings.trust_roots(),
            settings.disabled_skills.iter().cloned(),
            settings.denied_skills.iter().cloned(),
        );

        let catalog = SkillCatalog::from_entries_filtered(entries.clone(), &filter);
        let allowlisted_dirs = catalog
            .entries()
            .map(|entry| entry.skill_dir.clone())
            .collect();

        let llm = LlmClient::from_env().ok();

        let watcher = if settings.watch_reload {
            SkillWatcher::live(&entries).unwrap_or_else(|_| SkillWatcher::snapshot(&entries))
        } else {
            SkillWatcher::snapshot(&entries)
        };

        Ok(Self {
            settings,
            llm,
            catalog,
            entries,
            session: SkillSession::new(),
            context: ConversationContext::default(),
            allowlisted_dirs,
            filter,
            registry_source,
            filesystem,
            watcher,
            last_diagnostics: diagnostics,
        })
    }

    pub fn reload(&mut self) -> skills_rs::Result<()> {
        let sources: Vec<&dyn SkillSource> = vec![&self.registry_source, &self.filesystem];
        let (entries, diagnostics, _) = discover_and_merge_from(&MultiSkillSource::new(sources))?;
        self.entries = entries;
        self.last_diagnostics = diagnostics;
        self.filter = CompositeFilter::host_default(
            self.settings.trust_roots(),
            self.settings.disabled_skills.iter().cloned(),
            self.settings.denied_skills.iter().cloned(),
        );
        self.catalog = SkillCatalog::from_entries_filtered(self.entries.clone(), &self.filter);
        self.allowlisted_dirs = self
            .catalog
            .entries()
            .map(|entry| entry.skill_dir.clone())
            .collect();
        let _ = self.watcher.reset(&self.entries, self.settings.watch_reload);
        Ok(())
    }

    pub fn set_watch_reload(&mut self, enabled: bool) -> Result<(), String> {
        self.settings.watch_reload = enabled;
        self.watcher.reset(&self.entries, enabled)
    }

    pub fn llm_model(&self) -> Option<&str> {
        self.llm.as_ref().map(LlmClient::model)
    }

    pub fn llm_select_skill(&self, message: &str) -> crate::llm::SkillSelection {
        let Some(llm) = &self.llm else {
            return crate::llm::SkillSelection {
                message: "llm unavailable: set OPENAI_API_KEY (optional OPENAI_BASE_URL, OPENAI_MODEL)"
                    .to_owned(),
                skill_name: None,
            };
        };

        llm.select_skill(&self.catalog, message)
    }

    pub fn run_llm_agent(&mut self, user_message: &str) -> String {
        if self.llm.is_none() {
            return "llm unavailable: set OPENAI_API_KEY (optional OPENAI_BASE_URL, OPENAI_MODEL)"
                .to_owned();
        }

        if self.catalog.is_empty() {
            return "llm: no skills available".to_owned();
        }

        let system_prompt = self
            .system_prompt()
            .unwrap_or_else(|| "You are a helpful agent with access to skills.".to_owned());
        let mut messages = vec![user_message_payload(user_message)];
        let mut transcript = Vec::new();

        for _ in 0..MAX_AGENT_TURNS {
            let agent_turn = {
                let llm = self.llm.as_ref().expect("llm checked above");
                let catalog = &self.catalog;
                match llm.run_agent_turn(catalog, &system_prompt, &messages) {
                    Ok(turn) => turn,
                    Err(error) => return format!("llm error: {error}"),
                }
            };

            if !agent_turn.assistant_message.is_empty() {
                transcript.push(agent_turn.assistant_message.clone());
            }

            if agent_turn.tool_calls.is_empty() {
                return transcript.join("\n");
            }

            messages.push(assistant_followup_payload(
                agent_turn.assistant_message.clone(),
                agent_turn.tool_calls.clone(),
            ));

            for tool_call in agent_turn.tool_calls {
                let result = self.execute_agent_tool(&tool_call);
                transcript.push(format!("tool {} -> {}", tool_call.name, result));
                messages.push(tool_result_payload(&tool_call.id, result));
            }
        }

        format!("{}\n(max agent turns reached)", transcript.join("\n"))
    }

    fn execute_agent_tool(&mut self, tool_call: &AgentToolCall) -> String {
        match tool_call.name.as_str() {
            "activate_skill" => {
                let Some(name) = string_argument(&tool_call.arguments, "name") else {
                    return "missing name argument".to_owned();
                };
                self.activate_skill(&name, true)
            }
            "read_file" => {
                let Some(path) = path_argument(&tool_call.arguments, "path") else {
                    return "missing path argument".to_owned();
                };
                self.read_file(Path::new(&path))
            }
            "run_script" => {
                let Some(skill) = string_argument(&tool_call.arguments, "skill") else {
                    return "missing skill argument".to_owned();
                };
                let Some(script) = string_argument(&tool_call.arguments, "script") else {
                    return "missing script argument".to_owned();
                };
                self.run_script(&skill, &script)
            }
            other => format!("unknown tool: {other}"),
        }
    }

    pub fn maybe_reload_on_change(&mut self) -> skills_rs::Result<Option<String>> {
        if !self.settings.watch_reload {
            return Ok(None);
        }

        if !self.watcher.changed(&self.entries) {
            return Ok(None);
        }

        self.reload()?;
        Ok(Some("skills changed on disk, reloaded".to_owned()))
    }

    pub fn has_untrusted_project_skills(&self) -> bool {
        !self.settings.trusted
            && self
                .entries
                .iter()
                .any(|entry| entry.scope == SkillScope::Project)
    }

    pub fn activation_mode(&self) -> ActivationMode {
        if self.settings.file_read_mode {
            ActivationMode::FileRead
        } else {
            ActivationMode::DedicatedTool {
                tool_name: "activate_skill",
            }
        }
    }

    pub fn system_prompt(&self) -> Option<String> {
        self.catalog
            .system_prompt_block(self.activation_mode(), self.settings.catalog_placement)
    }

    pub fn tool_schema(&self) -> Option<String> {
        const HOST_AUXILIARY_TOOLS: &[&str] = &[
            "read_file(path: string, allowed under skill dirs only)",
            "run_script(skill: string, script: string, allowed-tools gated)",
        ];

        self.catalog.tool_description_block(
            "activate_skill",
            self.settings.catalog_placement,
            HOST_AUXILIARY_TOOLS,
        )
    }

    pub fn activate_skill(&mut self, name: &str, consent: bool) -> String {
        let Some(entry) = self.catalog.get(name).cloned() else {
            return format!("unknown skill: {name}");
        };

        if self.settings.require_activation_consent && !consent {
            return format!("activation of '{name}' requires consent (/activate {name} --yes)");
        }

        if !self.filter.allow_activation(&entry) {
            return format!("activation denied for '{name}'");
        }

        match self.session.try_activate(
            &entry,
            ActivationOptions {
                strip_frontmatter: true,
                wrap: WrapStyle::SkillContent,
                list_resources: true,
                max_listed_resources: 5,
            },
            &self.filter,
        ) {
            Ok(Some(activated)) => {
                self.settings.record_activation(name);
                self.context
                    .push(format!("skill:{name}"), activated.content.clone(), true);
                activated.content
            }
            Ok(None) => format!("skill '{name}' already active"),
            Err(error) => format!("activation error: {error}"),
        }
    }

    pub fn read_file(&self, path: &Path) -> String {
        for entry in self.catalog.entries() {
            let skill_dir = &entry.skill_dir;
            let resolved = if path.starts_with(skill_dir) {
                path.to_path_buf()
            } else if let Ok(resolved) = resolve_under_skill_dir(skill_dir, path) {
                resolved
            } else {
                continue;
            };

            let relative = resolved
                .strip_prefix(skill_dir)
                .unwrap_or(resolved.as_path());

            if !AllowedToolsPolicy::allows_read_path(entry, relative) {
                return format!(
                    "denied by allowed-tools policy for '{}' (allows: {})",
                    entry.properties.name,
                    AllowedToolsPolicy::describe(entry)
                );
            }

            return fs::read_to_string(&resolved)
                .unwrap_or_else(|error| format!("read error: {error}"));
        }

        "permission denied: path outside allowlisted skill directories".to_owned()
    }

    pub fn run_script(&self, skill_name: &str, script_path: &str) -> String {
        let Some(entry) = self.catalog.get(skill_name) else {
            return format!("unknown skill: {skill_name}");
        };

        let relative = Path::new(script_path);
        let Ok(resolved) = resolve_under_skill_dir(&entry.skill_dir, relative) else {
            return format!("invalid script path: {script_path}");
        };

        let scripts_root = entry.skill_dir.join("scripts");
        if !resolved.starts_with(&scripts_root) {
            return "only files under scripts/ may be executed".to_owned();
        }

        if !resolved.is_file() {
            return format!("script not found: {}", resolved.display());
        }

        let script_file_name = resolved
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();

        let script_content = fs::read_to_string(&resolved).ok();

        if !AllowedToolsPolicy::allows_script(
            entry,
            script_file_name,
            script_content.as_deref(),
        ) {
            return format!(
                "denied by allowed-tools policy (skill allows: {})",
                AllowedToolsPolicy::describe(entry)
            );
        }

        let mut command = match resolved
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
        {
            "py" => {
                let mut command = Command::new("python3");
                command.arg(&resolved);
                command
            }
            "js" | "mjs" => {
                let mut command = Command::new("node");
                command.arg(&resolved);
                command
            }
            _ => {
                let mut command = Command::new("bash");
                command.arg(&resolved);
                command
            }
        };

        let output = match command
            .current_dir(&entry.skill_dir)
            .env_clear()
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", "/tmp")
            .output()
        {
            Ok(output) => output,
            Err(error) => return format!("failed to execute script: {error}"),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!(
            "exit={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            output.status.code().unwrap_or(-1)
        )
    }

    pub fn user_activate(&mut self, name: &str) -> String {
        self.activate_skill(name, true)
    }

    pub fn slash_command(&mut self, line: &str) -> Option<String> {
        if !line.starts_with('/') {
            return None;
        }

        let name = line.trim_start_matches('/').trim();
        if name.is_empty() {
            return Some("usage: /skill-name".to_owned());
        }

        Some(self.user_activate(name))
    }

    pub fn autocomplete_prefix(&self, prefix: &str) -> Vec<String> {
        self.catalog
            .names()
            .filter(|name| name.starts_with(prefix))
            .map(str::to_owned)
            .collect()
    }

    pub fn run_subagent(&mut self, name: &str, task: &str) -> String {
        let Some(entry) = self.catalog.get(name).cloned() else {
            return format!("unknown skill: {name}");
        };

        let host_ref = &*self;
        let skill_name = name.to_owned();
        let host = SubagentHost {
            read_file: Box::new(|path| host_ref.read_file(path)),
            run_script: Box::new(move |script| host_ref.run_script(&skill_name, script)),
        };

        let result = run_subagent_task(
            &entry,
            task,
            &host_ref.filter,
            host_ref.llm.as_ref(),
            host,
        );

        match result {
            Ok(result) => {
                self.context
                    .push(format!("subagent:{name}"), result.summary.clone(), false);
                format!(
                    "{}\n\n[subagent: {} turns]",
                    result.summary, result.subagent_turns
                )
            }
            Err(error) => format!("subagent failed: {error}"),
        }
    }

    pub fn compact_context(&mut self) -> String {
        self.context
            .compact_with_token_budget(self.settings.compact_max_unprotected_tokens);
        self.context
            .compact_with_budget(self.settings.compact_keep_unprotected);
        format!(
            "protected tags: {}\n{}",
            self.session.protection_tags().join(", "),
            self.context.dump()
        )
    }

    pub fn analytics(&self) -> String {
        if self.settings.activation_log.is_empty() {
            return "no activations recorded".to_owned();
        }

        self.settings
            .activation_log
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn diagnostics(&self) -> String {
        format_diagnostics(&self.last_diagnostics)
    }

    pub fn list_skills(&self) -> String {
        if self.catalog.is_empty() {
            return "no skills in catalog".to_owned();
        }

        self.catalog
            .entries()
            .map(|entry| {
                format!(
                    "{} [{}] {} (allowed-tools: {})",
                    entry.properties.name,
                    format!("{:?}", entry.scope).to_lowercase(),
                    entry.properties.description,
                    AllowedToolsPolicy::describe(entry)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn prompt_line(label: &str) -> io::Result<String> {
    print!("{label}> ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

pub fn trust_prompt_message() -> &'static str {
    "project skills are present but trust is off; project-scoped skills are hidden until you run: trust on"
}
