use std::path::Path;

use skills_rs::{ActivationOptions, SkillEntry, SkillFilter, SkillSession, WrapStyle};

use crate::context::ConversationContext;
use crate::llm::{
    AgentToolCall, LlmClient, assistant_followup_payload, path_argument, string_argument,
    tool_result_payload, user_message_payload,
};

pub struct SubagentResult {
    pub summary: String,
    pub subagent_turns: usize,
}

pub struct SubagentHost<'a> {
    pub read_file: Box<dyn Fn(&Path) -> String + 'a>,
    pub run_script: Box<dyn Fn(&str) -> String + 'a>,
}

const MAX_SUBAGENT_TURNS: usize = 8;

pub fn run_subagent_task(
    entry: &SkillEntry,
    task: &str,
    filter: &dyn SkillFilter,
    llm: Option<&LlmClient>,
    host: SubagentHost<'_>,
) -> Result<SubagentResult, String> {
    if !filter.allow_activation(entry) {
        return Err(format!(
            "subagent activation denied for '{}'",
            entry.properties.name
        ));
    }

    let llm = llm.ok_or_else(|| {
        "subagent unavailable: set OPENAI_API_KEY (optional OPENAI_BASE_URL, OPENAI_MODEL)"
            .to_owned()
    })?;

    let mut subagent_session = SkillSession::new();
    let mut subagent_context = ConversationContext::default();

    let activated = subagent_session
        .force_activate(
            entry,
            ActivationOptions {
                strip_frontmatter: true,
                wrap: WrapStyle::None,
                list_resources: true,
                max_listed_resources: 3,
            },
        )
        .map_err(|error| error.to_string())?;

    subagent_context.push(
        format!("skill:{}", entry.properties.name),
        activated.content.clone(),
        true,
    );

    let mut messages = vec![user_message_payload(task)];
    let mut last_assistant_message = String::new();

    for _ in 0..MAX_SUBAGENT_TURNS {
        let turn = llm.run_subagent_turn(&entry.properties.name, &activated.content, &messages)?;

        if turn.tool_calls.is_empty() {
            last_assistant_message = turn.assistant_message;
            subagent_context.push(
                "assistant".to_owned(),
                last_assistant_message.clone(),
                false,
            );
            break;
        }

        messages.push(assistant_followup_payload(
            turn.assistant_message.clone(),
            turn.tool_calls.clone(),
        ));

        for tool_call in turn.tool_calls {
            let result = execute_subagent_tool(&tool_call, &host);
            messages.push(tool_result_payload(&tool_call.id, result));
        }
    }

    if last_assistant_message.is_empty() {
        last_assistant_message = llm.summarize(&entry.properties.name, &activated.content, task)?;
        subagent_context.push(
            "assistant".to_owned(),
            last_assistant_message.clone(),
            false,
        );
    }

    Ok(SubagentResult {
        summary: last_assistant_message,
        subagent_turns: subagent_context.len(),
    })
}

fn execute_subagent_tool(tool_call: &AgentToolCall, host: &SubagentHost<'_>) -> String {
    match tool_call.name.as_str() {
        "read_file" => {
            let Some(path) = path_argument(&tool_call.arguments, "path") else {
                return "missing path argument".to_owned();
            };
            (host.read_file)(Path::new(&path))
        }
        "run_script" => {
            let Some(script) = string_argument(&tool_call.arguments, "script") else {
                return "missing script argument".to_owned();
            };
            (host.run_script)(&script)
        }
        other => format!("unknown subagent tool: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skills_rs::{AllowAll, SkillProperties, SkillScope};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn subagent_requires_llm_configuration() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("demo");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: demo\ndescription: demo\n---\nBody\n",
        )
        .unwrap();

        let entry = SkillEntry {
            properties: SkillProperties {
                name: "demo".to_owned(),
                description: "demo".to_owned(),
                license: None,
                compatibility: None,
                allowed_tools: None,
                metadata: Default::default(),
            },
            location: skill_dir.join("SKILL.md"),
            skill_dir: skill_dir.clone(),
            scope: SkillScope::Bundled,
            source_root: dir.path().to_path_buf(),
        };

        let host = SubagentHost {
            read_file: Box::new(|_| "ok".to_owned()),
            run_script: Box::new(|_| "ok".to_owned()),
        };

        match run_subagent_task(&entry, "do thing", &AllowAll, None, host) {
            Err(error) => assert!(error.contains("OPENAI_API_KEY")),
            Ok(_) => panic!("expected missing llm configuration"),
        }
    }
}
