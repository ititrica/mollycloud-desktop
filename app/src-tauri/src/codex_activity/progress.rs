use super::parser::Payload;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Progress {
    #[default]
    Starting,
    Thinking,
    Retrying,
    CommandRunning,
    CommandComplete,
    CommandFailed,
    ToolRunning,
    ToolComplete,
    Editing,
    Edited,
    Searching,
    Searched,
    Reading,
    Read,
    Responding,
    Compacting,
    WaitingInput,
    Ready,
    Blocked,
    Stopped,
}

impl Progress {
    pub fn completed(self) -> Self {
        match self {
            Self::CommandRunning => Self::CommandComplete,
            Self::Editing => Self::Edited,
            Self::Searching => Self::Searched,
            Self::Reading => Self::Read,
            _ => Self::ToolComplete,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Step {
    pub progress: Progress,
    pub at: String,
}

pub fn tool(name: &str) -> Progress {
    match name.rsplit('.').next().unwrap_or(name) {
        "exec_command" | "shell_command" | "shell" | "run_shell" => Progress::CommandRunning,
        "apply_patch" => Progress::Editing,
        "web" | "web_search" | "search" => Progress::Searching,
        "view_image" | "read_file" | "read_resource" => Progress::Reading,
        _ => Progress::ToolRunning,
    }
}

pub fn event(kind: &str, p: &Payload) -> Option<Progress> {
    use Progress::*;
    match (kind, p.kind.as_str()) {
        ("response_item", "reasoning") | ("event_msg", "agent_reasoning") => Some(Thinking),
        ("response_item", "message") if p.role.as_deref() == Some("assistant") => Some(Responding),
        ("event_msg", "agent_message") => Some(Responding),
        ("event_msg", "retry" | "retrying") => Some(Retrying),
        ("event_msg", "stream_error") if p.will_retry == Some(true) => Some(Retrying),
        ("event_msg", "error") if p.will_retry == Some(true) => Some(Retrying),
        ("event_msg", "exec_command_begin") => Some(CommandRunning),
        ("event_msg", "exec_command_end") => Some(CommandComplete),
        ("event_msg", "patch_apply_begin") => Some(Editing),
        ("event_msg", "patch_apply_end") => Some(Edited),
        ("event_msg", "item_started" | "item_completed") => {
            let item = p.item.as_ref()?;
            let complete = p.kind == "item_completed";
            Some(match item.kind.as_str() {
                "Reasoning" | "reasoning" => Thinking,
                "CommandExecution" | "commandExecution" => {
                    if !complete {
                        CommandRunning
                    } else if item.exit_code.is_some_and(|code| code != 0)
                        || item.status.as_deref() == Some("failed")
                    {
                        CommandFailed
                    } else {
                        CommandComplete
                    }
                }
                "FileChange" | "fileChange" => {
                    if complete {
                        Edited
                    } else {
                        Editing
                    }
                }
                "McpToolCall" | "mcpToolCall" => {
                    if complete {
                        ToolComplete
                    } else {
                        ToolRunning
                    }
                }
                "WebSearch" | "webSearch" => {
                    if complete {
                        Searched
                    } else {
                        Searching
                    }
                }
                "ImageView" | "imageView" => {
                    if complete {
                        Read
                    } else {
                        Reading
                    }
                }
                "AgentMessage" | "agentMessage" => Responding,
                "ContextCompaction" | "contextCompaction" => Compacting,
                _ => return None,
            })
        }
        _ => None,
    }
}
