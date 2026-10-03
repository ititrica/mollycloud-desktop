//! Rollout JSONL is a compatibility format, not the App Server protocol.
//! Retain lifecycle metadata and the heading of a public reasoning summary only.
//! serde skips message bodies, raw reasoning, encrypted content and tool arguments.
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub struct Record {
    #[serde(default)]
    pub timestamp: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub payload: Payload,
}

#[derive(Default, Deserialize)]
pub struct Payload {
    #[serde(rename = "type", default)]
    pub kind: String,
    pub id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub call_id: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub status: Option<String>,
    pub will_retry: Option<bool>,
    pub role: Option<String>,
    pub item: Option<Item>,
    #[serde(default, deserialize_with = "deserialize_summary")]
    pub summary: Vec<SummaryPart>,
    pub error: Option<serde::de::IgnoredAny>,
}

fn deserialize_summary<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<SummaryPart>, D::Error> {
    // Other desktop records use `summary` for a string. Ignore those values
    // without dropping lifecycle metadata or retaining their private content.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Summary { Parts(Vec<SummaryPart>), Other(serde::de::IgnoredAny) }
    Ok(match Summary::deserialize(deserializer)? { Summary::Parts(parts) => parts, Summary::Other(_) => Vec::new() })
}

#[derive(Default, Deserialize)]
pub struct Item {
    #[serde(rename = "type", default)]
    pub kind: String,
    pub status: Option<String>,
    pub exit_code: Option<i64>,
    #[serde(default, alias = "summaryText")]
    pub summary_text: Vec<SummaryTitle>,
}

#[derive(Default, Deserialize)]
pub struct SummaryPart {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(rename = "text", default)]
    pub title: SummaryTitle,
}

#[derive(Default)]
pub struct SummaryTitle(pub String);

impl<'de> Deserialize<'de> for SummaryTitle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Heading;
        impl serde::de::Visitor<'_> for Heading {
            type Value = SummaryTitle;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a public summary string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                let first = value
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("")
                    .trim();
                let title = first
                    .strip_prefix("**")
                    .and_then(|s| s.split_once("**").map(|(title, _)| title))
                    .or_else(|| {
                        first
                            .strip_prefix("__")
                            .and_then(|s| s.split_once("__").map(|(title, _)| title))
                    })
                    .or_else(|| {
                        first
                            .starts_with('#')
                            .then(|| first.trim_start_matches('#').strip_prefix(' '))
                            .flatten()
                    })
                    .unwrap_or("");
                Ok(SummaryTitle(
                    title
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(160)
                        .collect::<String>()
                        .trim()
                        .to_owned(),
                ))
            }
        }
        deserializer.deserialize_str(Heading)
    }
}

pub fn summary_title(record_kind: &str, payload: &Payload) -> Option<String> {
    let title = if record_kind == "response_item" && payload.kind == "reasoning" {
        payload
            .summary
            .iter()
            .rev()
            .filter(|part| part.kind == "summary_text")
            .map(|part| &part.title.0)
            .find(|title| !title.is_empty())
    } else if record_kind == "event_msg"
        && matches!(payload.kind.as_str(), "item_started" | "item_completed")
    {
        payload
            .item
            .as_ref()
            .filter(|item| matches!(item.kind.as_str(), "Reasoning" | "reasoning"))
            .and_then(|item| {
                item.summary_text
                    .iter()
                    .rev()
                    .map(|title| &title.0)
                    .find(|title| !title.is_empty())
            })
    } else {
        None
    };
    title.cloned()
}

pub fn valid_thread_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

pub fn label(value: Option<&str>) -> String {
    value
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect()
}

pub fn decode(line: &[u8]) -> Option<Record> {
    serde_json::from_slice(line).ok()
}

pub fn timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|t| t.timestamp_millis())
}

pub fn unknown_event(record: &Record) -> bool {
    match record.kind.as_str() {
        "session_meta" | "turn_context" | "response_item" | "compacted" | "world_state"
            | "token_usage_record" | "inter_agent_communication_metadata" => false,
        "event_msg" => !matches!(
            record.payload.kind.as_str(),
            "task_started"
                | "turn_started"
                | "task_complete"
                | "task_completed"
                | "turn_complete"
                | "turn_completed"
                | "turn_aborted"
                | "task_aborted"
                | "task_failed"
                | "turn_failed"
                | "system_error"
                | "error"
                | "request_user_input"
                | "approval_requested"
                | "exec_approval_request"
                | "apply_patch_approval_request"
                | "approval_resolved"
                | "user_input_received"
                | "user_message"
                | "agent_message"
                | "agent_reasoning"
                | "token_count"
                | "context_compacted"
                | "thread_rolled_back"
                | "item_started"
                | "item_completed"
                | "exec_command_begin"
                | "exec_command_end"
                | "retry"
                | "retrying"
                | "stream_error"
                | "patch_apply_begin"
                | "patch_apply_end"
                | "thread_settings_applied"
        ),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_reasoning_summary_does_not_discard_desktop_lifecycle() {
        let record = decode(br#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn","summary":"private content"}}"#).unwrap();
        assert_eq!(record.payload.kind, "task_started");
        assert!(record.payload.summary.is_empty());
    }
}
