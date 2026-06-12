use std::env;

use skills_rs::SkillCatalog;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub struct LlmClient {
    http: HttpLlm,
}

struct HttpLlm {
    client: reqwest::blocking::Client,
    base_url: String,
    model: String,
    api_key: String,
}

#[derive(Debug)]
pub struct SkillSelection {
    pub message: String,
    pub skill_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug)]
pub struct AgentTurn {
    pub assistant_message: String,
    pub tool_calls: Vec<AgentToolCall>,
}

impl LlmClient {
    pub fn from_env() -> Result<Self, String> {
        let api_key = env::var("OPENAI_API_KEY")
            .map_err(|_| "OPENAI_API_KEY is not set".to_owned())?;
        if api_key.is_empty() {
            return Err("OPENAI_API_KEY is empty".to_owned());
        }

        let base_url = env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_owned());
        let model = env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_owned());
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .build()
            .map_err(|error| format!("failed to create HTTP client: {error}"))?;

        Ok(Self {
            http: HttpLlm {
                client,
                base_url,
                model,
                api_key,
            },
        })
    }

    pub fn model(&self) -> &str {
        &self.http.model
    }

    pub fn select_skill(&self, catalog: &SkillCatalog, user_message: &str) -> SkillSelection {
        if catalog.is_empty() {
            return SkillSelection {
                message: "llm: no skills available".to_owned(),
                skill_name: None,
            };
        }

        let system = build_skill_selection_system_prompt(catalog);
        match self.complete(&system, user_message) {
            Ok(response) => parse_skill_selection(catalog, &response),
            Err(error) => SkillSelection {
                message: format!("llm error: {error}"),
                skill_name: None,
            },
        }
    }

    pub fn run_agent_turn(
        &self,
        catalog: &SkillCatalog,
        system_prompt: &str,
        messages: &[ChatMessagePayload],
    ) -> Result<AgentTurn, String> {
        let tools = build_host_tools(catalog);
        let response = self.complete_with_tools(system_prompt, messages, &tools)?;
        Ok(parse_agent_turn(response))
    }

    pub fn run_subagent_turn(
        &self,
        skill_name: &str,
        skill_content: &str,
        messages: &[ChatMessagePayload],
    ) -> Result<AgentTurn, String> {
        let system = format!(
            "You are a subagent executing skill '{skill_name}'. Follow the skill instructions, \
             use read_file and run_script when needed, then summarize results.\n\n{skill_content}"
        );
        let tools = build_subagent_tools();
        let response = self.complete_with_tools(&system, messages, &tools)?;
        Ok(parse_agent_turn(response))
    }

    pub fn summarize(&self, skill_name: &str, skill_content: &str, task: &str) -> Result<String, String> {
        let system = format!(
            "You are a subagent executing skill '{skill_name}'. Follow the skill instructions and produce a concise summary of work done.\n\n{skill_content}"
        );
        let user = format!("Task: {task}");
        self.complete(&system, &user)
    }

    fn complete(&self, system: &str, user: &str) -> Result<String, String> {
        let response = self.complete_with_tools(system, &[ChatMessagePayload::user(user)], &[])?;
        let turn = parse_agent_turn(response);
        if turn.assistant_message.trim().is_empty() {
            return Err("empty model response".to_owned());
        }
        Ok(turn.assistant_message)
    }

    fn complete_with_tools(
        &self,
        system: &str,
        messages: &[ChatMessagePayload],
        tools: &[Value],
    ) -> Result<ChatCompletionResponse, String> {
        let url = format!(
            "{}/chat/completions",
            self.http.base_url.trim_end_matches('/')
        );

        let mut payload_messages = vec![ChatMessagePayload::system(system)];
        payload_messages.extend_from_slice(messages);

        let mut body = json!({
            "model": self.http.model,
            "messages": payload_messages,
            "temperature": 0.0,
        });

        if !tools.is_empty() {
            body["tools"] = json!(tools);
            body["tool_choice"] = json!("auto");
        }

        let response = self
            .http
            .client
            .post(url)
            .bearer_auth(&self.http.api_key)
            .json(&body)
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;

        response.json().map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessagePayload {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCallPayload>>,
}

impl ChatMessagePayload {
    fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_owned(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_owned(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    fn assistant(content: impl Into<String>, tool_calls: Vec<ToolCallPayload>) -> Self {
        Self {
            role: "assistant".to_owned(),
            content: content.into(),
            tool_call_id: None,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
        }
    }

    fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_owned(),
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ToolCallPayload {
    id: String,
    r#type: &'static str,
    function: ToolCallFunctionPayload,
}

#[derive(Debug, Clone, Serialize)]
struct ToolCallFunctionPayload {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionMessage {
    content: Option<String>,
    tool_calls: Option<Vec<IncomingToolCall>>,
}

#[derive(Debug, Deserialize)]
struct IncomingToolCall {
    id: String,
    function: IncomingToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct IncomingToolCallFunction {
    name: String,
    arguments: String,
}

pub fn build_host_tools(catalog: &SkillCatalog) -> Vec<Value> {
    let skill_names: Vec<_> = catalog.names().map(str::to_owned).collect();
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "activate_skill",
                "description": "Load full instructions for a skill by name.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "enum": skill_names,
                        }
                    },
                    "required": ["name"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file under an allowlisted skill directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_script",
                "description": "Run a script under a skill's scripts/ directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "skill": { "type": "string" },
                        "script": { "type": "string" }
                    },
                    "required": ["skill", "script"]
                }
            }
        }),
    ]
}

pub fn build_subagent_tools() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file relative to the active skill directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "run_script",
                "description": "Run a script under the active skill's scripts/ directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "script": { "type": "string" }
                    },
                    "required": ["script"]
                }
            }
        }),
    ]
}

pub fn user_message_payload(message: &str) -> ChatMessagePayload {
    ChatMessagePayload::user(message)
}

pub fn tool_result_payload(tool_call_id: &str, content: impl Into<String>) -> ChatMessagePayload {
    ChatMessagePayload::tool(tool_call_id, content)
}

pub fn assistant_followup_payload(
    content: impl Into<String>,
    tool_calls: Vec<AgentToolCall>,
) -> ChatMessagePayload {
    ChatMessagePayload::assistant(
        content,
        tool_calls
            .into_iter()
            .map(|tool_call| ToolCallPayload {
                id: tool_call.id,
                r#type: "function",
                function: ToolCallFunctionPayload {
                    name: tool_call.name,
                    arguments: tool_call.arguments.to_string(),
                },
            })
            .collect(),
    )
}

fn parse_agent_turn(response: ChatCompletionResponse) -> AgentTurn {
    let Some(choice) = response.choices.into_iter().next() else {
        return AgentTurn {
            assistant_message: String::new(),
            tool_calls: Vec::new(),
        };
    };

    let assistant_message = choice.message.content.unwrap_or_default();
    let tool_calls = choice
        .message
        .tool_calls
        .unwrap_or_default()
        .into_iter()
        .filter_map(|tool_call| {
            serde_json::from_str::<Value>(&tool_call.function.arguments)
                .ok()
                .map(|arguments| AgentToolCall {
                    id: tool_call.id,
                    name: tool_call.function.name,
                    arguments,
                })
        })
        .collect();

    AgentTurn {
        assistant_message,
        tool_calls,
    }
}

fn build_skill_selection_system_prompt(catalog: &SkillCatalog) -> String {
    let catalog_xml = catalog
        .to_prompt_xml()
        .unwrap_or_else(|| "<available_skills></available_skills>".to_owned());
    format!(
        "Select the single best skill for the user task from the catalog below.\n\
         Reply with ONLY the skill name, or NONE if no skill matches.\n\n{catalog_xml}"
    )
}

pub fn parse_skill_selection(catalog: &SkillCatalog, response: &str) -> SkillSelection {
    let trimmed = response.trim();
    if trimmed.eq_ignore_ascii_case("none") || trimmed.is_empty() {
        return SkillSelection {
            message: "llm: no skill selected".to_owned(),
            skill_name: None,
        };
    }

    let normalized = trimmed
        .trim_matches('"')
        .trim_matches('\'')
        .to_lowercase();

    for entry in catalog.entries() {
        if entry.properties.name == normalized {
            return SkillSelection {
                message: format!(
                    "llm: selected skill '{}'\ntool call: activate_skill(name=\"{}\")",
                    entry.properties.name, entry.properties.name
                ),
                skill_name: Some(entry.properties.name.clone()),
            };
        }
    }

    SkillSelection {
        message: format!("llm: unrecognized skill name in response: {trimmed}"),
        skill_name: None,
    }
}

pub fn string_argument(arguments: &Value, key: &str) -> Option<String> {
    arguments.get(key).and_then(Value::as_str).map(str::to_owned)
}

pub fn path_argument(arguments: &Value, key: &str) -> Option<String> {
    string_argument(arguments, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skills_rs::{SkillCatalog, SkillEntry, SkillProperties, SkillScope};
    use std::path::PathBuf;

    fn entry(name: &str) -> SkillEntry {
        SkillEntry {
            properties: SkillProperties {
                name: name.to_owned(),
                description: "demo".to_owned(),
                license: None,
                compatibility: None,
                allowed_tools: None,
                metadata: Default::default(),
            },
            location: PathBuf::from(format!("/tmp/{name}/SKILL.md")),
            skill_dir: PathBuf::from(format!("/tmp/{name}")),
            scope: SkillScope::Bundled,
            source_root: PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn parses_skill_name_from_model_response() {
        let catalog = SkillCatalog::from_entries(vec![entry("cloud-skill")]);
        let selection = parse_skill_selection(&catalog, "cloud-skill");
        assert_eq!(selection.skill_name.as_deref(), Some("cloud-skill"));
    }

    #[test]
    fn builds_activate_skill_tool_schema() {
        let catalog = SkillCatalog::from_entries(vec![entry("demo-skill")]);
        let tools = build_host_tools(&catalog);
        assert_eq!(tools.len(), 3);
        assert_eq!(
            tools[0]["function"]["name"].as_str(),
            Some("activate_skill")
        );
    }
}
