use super::parser::{label, summary_title, valid_thread_id, Record};
use super::progress::{self, Progress, Step};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    NeedsInput,
    Ready,
    Blocked,
    Stopped,
}

impl Status {
    pub fn priority(self) -> u8 {
        match self {
            Self::NeedsInput => 4,
            Self::Blocked => 3,
            Self::Ready => 2,
            Self::Running => 1,
            Self::Stopped => 0,
        }
    }
    pub fn active(self) -> bool {
        matches!(self, Self::Running | Self::NeedsInput)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub key: String,
    pub thread_id: String,
    pub turn_id: String,
    pub model: String,
    pub provider: String,
    pub status: Status,
    pub unread: bool,
    pub updated_at: String,
    #[serde(default)]
    pub progress: Progress,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub progress_title: String,
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Default)]
pub struct Context {
    pub thread: String,
    pub turn: String,
    model: String,
    provider: String,
    waiting_call: Option<String>,
    waiting_async: bool,
    tool_calls: BTreeMap<String, Progress>,
}

pub type Tasks = BTreeMap<String, Task>;

pub fn apply(context: &mut Context, tasks: &mut Tasks, record: Record, baseline: bool) {
    let p = record.payload;
    if record.kind == "session_meta" {
        if let Some(id) = p.id.filter(|id| valid_thread_id(id)) {
            context.thread = id;
        }
        context.provider = label(p.model_provider.as_deref());
        return;
    }
    if let Some(id) = p.thread_id.as_ref().filter(|id| valid_thread_id(id)) {
        context.thread = id.clone();
    }
    if record.kind == "turn_context" {
        if let Some(turn) = p.turn_id {
            context.turn = label(Some(&turn));
        }
        context.model = label(p.model.as_deref());
        if p.model_provider.is_some() {
            context.provider = label(p.model_provider.as_deref());
        }
        for task in tasks
            .values_mut()
            .filter(|t| t.thread_id == context.thread && t.turn_id == context.turn)
        {
            task.model = context.model.clone();
            task.provider = context.provider.clone();
        }
        return;
    }
    if !valid_thread_id(&context.thread) {
        return;
    }
    // Replayed files and late events must not resurrect a superseded/acknowledged turn.
    if let Some(time) = super::parser::timestamp(&record.timestamp) {
        if tasks.values().any(|t| {
            t.thread_id == context.thread
                && super::parser::timestamp(&t.updated_at).is_some_and(|previous| previous > time)
        }) {
            return;
        }
    }
    let is_start =
        record.kind == "event_msg" && matches!(p.kind.as_str(), "task_started" | "turn_started");
    if let Some(turn) = p.turn_id.as_deref() {
        if !is_start && !context.turn.is_empty() && context.turn != turn {
            return;
        }
        context.turn = label(Some(turn));
    } else if is_start {
        context.turn = label(Some(&record.timestamp));
    }
    if context.turn.is_empty() {
        return;
    }
    let key = format!("{}:{}", context.thread, context.turn);
    let previous = tasks.get(&key);
    let mut phase = progress::event(&record.kind, &p);
    let title = summary_title(&record.kind, &p);
    if record.kind == "response_item" {
        if matches!(p.kind.as_str(), "function_call" | "custom_tool_call") {
            if let Some(name) = p.name.as_deref() {
                let step = progress::tool(name);
                phase = Some(step);
                // Question acknowledgements are not task progress. In particular,
                // the async tool returns immediately while the question stays open.
                if let Some(id) = p.call_id.as_ref().filter(|_| {
                    !matches!(
                        name.rsplit('.').next(),
                        Some("request_user_input" | "request_user_input_async")
                    )
                }) {
                    context.tool_calls.insert(id.clone(), step);
                }
            }
        } else if matches!(
            p.kind.as_str(),
            "function_call_output" | "custom_tool_call_output"
        ) {
            if let Some(id) = p.call_id.as_ref() {
                phase = context.tool_calls.remove(id).map(Progress::completed);
            }
        }
    }
    let lifecycle = match (record.kind.as_str(), p.kind.as_str()) {
        ("event_msg", "task_started" | "turn_started") => {
            context.waiting_call = None;
            context.tool_calls.clear();
            Some(Status::Running)
        }
        ("event_msg", "task_complete" | "task_completed" | "turn_complete" | "turn_completed") => {
            Some(match p.status.as_deref() {
                Some("failed") => Status::Blocked,
                Some("interrupted") => Status::Stopped,
                _ if p.error.is_some() => Status::Blocked,
                _ => Status::Ready,
            })
        }
        ("event_msg", "turn_aborted" | "task_aborted") => Some(Status::Stopped),
        ("event_msg", "task_failed" | "turn_failed" | "system_error") => Some(Status::Blocked),
        ("event_msg", "error") if p.will_retry != Some(true) => Some(Status::Blocked),
        (
            "event_msg",
            "request_user_input"
            | "approval_requested"
            | "exec_approval_request"
            | "apply_patch_approval_request",
        ) => Some(Status::NeedsInput),
        ("event_msg", "approval_resolved" | "user_input_received")
            if previous.is_some_and(|task| task.status == Status::NeedsInput) =>
        {
            Some(Status::Running)
        }
        ("event_msg", "user_message") if context.waiting_call.is_some() => {
            context.waiting_call = None;
            Some(Status::Running)
        }
        ("response_item", "function_call")
            if p.name.as_deref().is_some_and(|n| {
                matches!(
                    n.rsplit('.').next(),
                    Some("request_user_input" | "request_user_input_async")
                )
            }) =>
        {
            context.waiting_async = p
                .name
                .as_deref()
                .is_some_and(|n| n.ends_with("request_user_input_async"));
            context.waiting_call = p.call_id;
            Some(Status::NeedsInput)
        }
        ("response_item", "function_call_output")
            if !context.waiting_async
                && context.waiting_call.is_some()
                && context.waiting_call == p.call_id =>
        {
            context.waiting_call = None;
            Some(Status::Running)
        }
        _ => None,
    };
    // New progress supersedes waiting, even if an optional question timed out
    // or has no answer. Neither late answers nor steps may resurrect a terminal turn.
    let Some(status) = lifecycle
        .filter(|status| !status.active() || is_start || previous.is_none_or(|t| t.status.active()))
        .or_else(|| {
            phase.and_then(|_| {
                previous
                    .filter(|t| t.status.active())
                    .map(|_| Status::Running)
            })
        })
    else {
        return;
    };
    if status != Status::NeedsInput {
        context.waiting_call = None;
        context.waiting_async = false;
        if !status.active() {
            context.tool_calls.clear();
        }
    }
    let phase = match status {
        Status::NeedsInput => Progress::WaitingInput,
        Status::Ready => Progress::Ready,
        Status::Blocked => Progress::Blocked,
        Status::Stopped => Progress::Stopped,
        Status::Running if is_start => Progress::Starting,
        Status::Running => phase.unwrap_or(Progress::Thinking),
    };
    // Generic wrapper completions should not replace more precise command/file events.
    let phase = if phase == Progress::ToolComplete
        && previous.is_some_and(|t| {
            matches!(
                t.progress,
                Progress::CommandComplete
                    | Progress::CommandFailed
                    | Progress::Edited
                    | Progress::Read
                    | Progress::Searched
            )
        }) {
        previous.unwrap().progress
    } else {
        phase
    };
    let mut steps = if is_start {
        Vec::new()
    } else {
        previous.map(|t| t.steps.clone()).unwrap_or_default()
    };
    let progress_title = if status == Status::Running && phase == Progress::Thinking {
        title
            .or_else(|| {
                previous
                    .filter(|task| task.progress == Progress::Thinking)
                    .map(|task| task.progress_title.clone())
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    if steps.last().is_none_or(|step| step.progress != phase) {
        steps.push(Step {
            progress: phase,
            at: label(Some(&record.timestamp)),
        });
        if steps.len() > 8 {
            steps.remove(0);
        }
    }
    // Starting a newer turn supersedes previous turns in this thread.
    if is_start {
        tasks.retain(|_, t| t.thread_id != context.thread || t.key == key);
    }
    let unread = tasks
        .get(&key)
        .filter(|t| t.status == status && t.updated_at == record.timestamp)
        .map(|t| t.unread)
        .unwrap_or(!baseline && !status.active());
    tasks.insert(
        key.clone(),
        Task {
            key,
            thread_id: context.thread.clone(),
            turn_id: context.turn.clone(),
            model: context.model.clone(),
            provider: context.provider.clone(),
            status,
            unread,
            updated_at: label(Some(&record.timestamp)),
            progress: phase,
            progress_title,
            steps,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "01a09809-9dd4-7203-ae17-00575d41934a";
    fn feed(c: &mut Context, t: &mut Tasks, body: serde_json::Value, baseline: bool) {
        apply(c, t, serde_json::from_value(body).unwrap(), baseline);
    }
    #[test]
    fn public_summary_headings_match_codex_without_retaining_bodies() {
        let mut c = Context {
            thread: ID.into(),
            ..Default::default()
        };
        let mut tasks = Tasks::new();
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"a"}}),
            false,
        );
        for payload in [
            serde_json::json!({"type":"reasoning","summary":[{"type":"summary_text","text":"**Appending theme token CSS**\n\nprivate summary body"}],"encrypted_content":"private ciphertext"}),
            serde_json::json!({"type":"item_completed","item":{"type":"Reasoning","summary_text":["**Appending theme token CSS**\n\nprivate summary body"],"raw_content":["private reasoning"]}}),
        ] {
            let kind = if payload["type"] == "reasoning" {
                "response_item"
            } else {
                "event_msg"
            };
            feed(
                &mut c,
                &mut tasks,
                serde_json::json!({"type":kind,"payload":payload}),
                false,
            );
            assert_eq!(
                tasks.values().next().unwrap().progress_title,
                "Appending theme token CSS"
            );
            assert!(!serde_json::to_string(&tasks).unwrap().contains("private"));
        }
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","exit_code":0}}}),
            false,
        );
        assert_eq!(
            tasks.values().next().unwrap().progress,
            Progress::CommandComplete
        );
        assert!(tasks.values().next().unwrap().progress_title.is_empty());
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"private summary without a title"},{"type":"raw_text","text":"**private raw heading**"}]}}),
            false,
        );
        assert!(tasks.values().next().unwrap().progress_title.is_empty());
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":format!("**{}**\nprivate body", "字".repeat(200))}]}}),
            false,
        );
        assert_eq!(
            tasks
                .values()
                .next()
                .unwrap()
                .progress_title
                .chars()
                .count(),
            160
        );
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_complete"}}),
            false,
        );
        assert!(tasks.values().next().unwrap().progress_title.is_empty());
        assert!(!serde_json::to_string(&tasks)
            .unwrap()
            .contains("progressTitle"));
    }
    #[test]
    fn precise_steps_follow_reasoning_retry_commands_and_terminal_events() {
        let mut c = Context {
            thread: ID.into(),
            ..Default::default()
        };
        let mut tasks = Tasks::new();
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"a"}}),
            false,
        );
        let signals = [
            (
                serde_json::json!({"type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"private reasoning without a heading"}]}}),
                Progress::Thinking,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"error","will_retry":true,"message":"private failure"}}),
                Progress::Retrying,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"item_started","item":{"type":"CommandExecution","command":"private command"}}}),
                Progress::CommandRunning,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","exit_code":0,"stdout":"private output"}}}),
                Progress::CommandComplete,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","exit_code":1}}}),
                Progress::CommandFailed,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange"}}}),
                Progress::Edited,
            ),
        ];
        for (record, phase) in signals {
            feed(&mut c, &mut tasks, record, false);
            assert_eq!(tasks.values().next().unwrap().progress, phase);
            assert_eq!(tasks.values().next().unwrap().status, Status::Running);
        }
        assert!(!serde_json::to_string(&tasks).unwrap().contains("private"));
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_complete","error":{"message":"private terminal failure"}}}),
            false,
        );
        assert_eq!(tasks.values().next().unwrap().status, Status::Blocked);
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"response_item","payload":{"type":"reasoning"}}),
            false,
        );
        assert_eq!(tasks.values().next().unwrap().status, Status::Blocked);
    }

    #[test]
    fn custom_tools_preserve_specific_steps_and_history_is_bounded() {
        let mut c = Context {
            thread: ID.into(),
            ..Default::default()
        };
        let mut tasks = Tasks::new();
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"a"}}),
            false,
        );
        for _ in 0..6 {
            feed(
                &mut c,
                &mut tasks,
                serde_json::json!({"type":"response_item","payload":{"type":"custom_tool_call","name":"exec","call_id":"x","input":"private code"}}),
                false,
            );
            feed(
                &mut c,
                &mut tasks,
                serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","exit_code":0}}}),
                false,
            );
            feed(
                &mut c,
                &mut tasks,
                serde_json::json!({"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"x","output":"private result"}}),
                false,
            );
            assert_eq!(
                tasks.values().next().unwrap().progress,
                Progress::CommandComplete
            );
        }
        assert_eq!(tasks.values().next().unwrap().steps.len(), 8);
        // Old persisted unread records still deserialize after the metadata extension.
        let mut legacy = serde_json::to_value(tasks.values().next().unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("progress");
        legacy.as_object_mut().unwrap().remove("steps");
        let restored: Task = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.progress, Progress::Starting);
        assert!(restored.steps.is_empty());
    }
    #[test]
    fn replay_does_not_restore_read_completion_or_older_turn() {
        let mut context = Context {
            thread: ID.into(),
            ..Default::default()
        };
        let mut tasks = Tasks::new();
        let start = serde_json::json!({"timestamp":"2026-09-16T10:00:00Z","type":"event_msg","payload":{"type":"task_started","turn_id":"a"}});
        let complete = serde_json::json!({"timestamp":"2026-09-16T10:01:00Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"a"}});
        feed(&mut context, &mut tasks, start.clone(), false);
        feed(&mut context, &mut tasks, complete.clone(), false);
        tasks.values_mut().next().unwrap().unread = false;
        feed(&mut context, &mut tasks, start, false);
        feed(&mut context, &mut tasks, complete.clone(), false);
        assert!(!tasks.values().next().unwrap().unread);
        feed(
            &mut context,
            &mut tasks,
            serde_json::json!({"timestamp":"2026-09-16T10:02:00Z","type":"event_msg","payload":{"type":"task_started","turn_id":"b"}}),
            false,
        );
        feed(&mut context, &mut tasks, complete, false);
        assert_eq!(context.turn, "b");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks.values().next().unwrap().status, Status::Running);
    }
    #[test]
    fn lifecycle_wait_resume_complete_and_unknown() {
        let mut c = Context {
            thread: ID.into(),
            ..Default::default()
        };
        let mut t = Tasks::new();
        feed(
            &mut c,
            &mut t,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"a"}}),
            false,
        );
        feed(
            &mut c,
            &mut t,
            serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input","call_id":"x","arguments":"secret"}}),
            false,
        );
        assert_eq!(t.values().next().unwrap().status, Status::NeedsInput);
        feed(
            &mut c,
            &mut t,
            serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"other"}}),
            false,
        );
        assert_eq!(t.values().next().unwrap().status, Status::NeedsInput);
        feed(
            &mut c,
            &mut t,
            serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"x"}}),
            false,
        );
        assert_eq!(t.values().next().unwrap().status, Status::Running);
        feed(
            &mut c,
            &mut t,
            serde_json::json!({"type":"event_msg","payload":{"type":"future_event"}}),
            false,
        );
        assert_eq!(t.values().next().unwrap().status, Status::Running);
        feed(
            &mut c,
            &mut t,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_complete"}}),
            false,
        );
        assert!(t.values().next().unwrap().unread);
        assert!(!serde_json::to_string(&t).unwrap().contains("secret"));
    }
    #[test]
    fn baseline_and_abort_are_not_failures() {
        let mut c = Context {
            thread: ID.into(),
            turn: "a".into(),
            ..Default::default()
        };
        let mut t = Tasks::new();
        feed(
            &mut c,
            &mut t,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_complete"}}),
            true,
        );
        assert!(!t.values().next().unwrap().unread);
        feed(
            &mut c,
            &mut t,
            serde_json::json!({"type":"event_msg","payload":{"type":"turn_aborted"}}),
            false,
        );
        assert_eq!(t.values().next().unwrap().status, Status::Stopped);
        assert!(Status::NeedsInput.priority() > Status::Blocked.priority());
        assert!(Status::Blocked.priority() > Status::Ready.priority());
        assert!(Status::Ready.priority() > Status::Running.priority());
    }

    #[test]
    fn concurrent_threads_retry_errors_and_new_turns() {
        let mut c = Context {
            thread: ID.into(),
            turn: "a".into(),
            ..Default::default()
        };
        let mut other = Context {
            thread: ID.replace("01a09809", "01a09810"),
            turn: "b".into(),
            ..Default::default()
        };
        let mut tasks = Tasks::new();
        for ctx in [&mut c, &mut other] {
            feed(
                ctx,
                &mut tasks,
                serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":ctx.turn}}),
                false,
            );
        }
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"error","will_retry":true}}),
            false,
        );
        assert!(tasks.values().all(|t| t.status == Status::Running));
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"turn_failed"}}),
            false,
        );
        assert_eq!(tasks.len(), 2);
        assert_eq!(
            tasks
                .values()
                .max_by_key(|t| t.status.priority())
                .unwrap()
                .status,
            Status::Blocked
        );
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"c"}}),
            false,
        );
        assert_eq!(tasks.len(), 2);
        assert!(tasks.values().all(|t| !t.unread));
    }

    #[test]
    fn async_question_waits_for_user_message_not_tool_ack() {
        let mut c = Context {
            thread: ID.into(),
            turn: "a".into(),
            ..Default::default()
        };
        let mut tasks = Tasks::new();
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input_async","call_id":"x"}}),
            false,
        );
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"x"}}),
            false,
        );
        assert_eq!(tasks.values().next().unwrap().status, Status::NeedsInput);
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":"private response"}}),
            false,
        );
        assert_eq!(tasks.values().next().unwrap().status, Status::Running);
    }

    #[test]
    fn actual_progress_replaces_unanswered_questions_and_late_answers_do_not_rewind_it() {
        let signals = [
            (
                serde_json::json!({"type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"**Continuing the task**"}]}}),
                Progress::Thinking,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"error","will_retry":true}}),
                Progress::Retrying,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"exec_command_begin"}}),
                Progress::CommandRunning,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","exit_code":0}}}),
                Progress::CommandComplete,
            ),
            (
                serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.apply_patch","call_id":"edit"}}),
                Progress::Editing,
            ),
            (
                serde_json::json!({"type":"response_item","payload":{"type":"custom_tool_call","name":"functions.exec","call_id":"tool"}}),
                Progress::ToolRunning,
            ),
            (
                serde_json::json!({"type":"event_msg","payload":{"type":"agent_message"}}),
                Progress::Responding,
            ),
        ];
        for name in [
            "functions.request_user_input",
            "functions.request_user_input_async",
        ] {
            for (record, expected) in &signals {
                let mut c = Context {
                    thread: ID.into(),
                    turn: "a".into(),
                    ..Default::default()
                };
                let mut tasks = Tasks::new();
                feed(
                    &mut c,
                    &mut tasks,
                    serde_json::json!({"timestamp":"2026-09-18T10:00:00Z","type":"event_msg","payload":{"type":"task_started","turn_id":"a"}}),
                    false,
                );
                feed(
                    &mut c,
                    &mut tasks,
                    serde_json::json!({"timestamp":"2026-09-18T10:00:01Z","type":"response_item","payload":{"type":"function_call","name":name,"call_id":"question"}}),
                    false,
                );
                assert_eq!(tasks.values().next().unwrap().status, Status::NeedsInput);
                let mut progress = record.clone();
                // Progress after the optional question has timed out, with no answer.
                progress["timestamp"] = "2026-09-18T10:02:00Z".into();
                feed(&mut c, &mut tasks, progress, false);
                let resumed = tasks.values().next().unwrap().clone();
                assert_eq!(resumed.status, Status::Running, "{name}: {expected:?}");
                assert_eq!(resumed.progress, *expected);
                if *expected == Progress::Thinking {
                    assert_eq!(resumed.progress_title, "Continuing the task");
                }
                for late in [
                    serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"question"}}),
                    serde_json::json!({"type":"event_msg","payload":{"type":"user_message"}}),
                    serde_json::json!({"type":"event_msg","payload":{"type":"user_input_received"}}),
                    serde_json::json!({"type":"event_msg","payload":{"type":"approval_resolved"}}),
                ] {
                    let mut late = late;
                    late["timestamp"] = "2026-09-18T10:02:01Z".into();
                    feed(&mut c, &mut tasks, late, false);
                    assert_eq!(tasks.values().next().unwrap(), &resumed);
                }
                // A replayed waiting event cannot override the newer progress.
                feed(
                    &mut c,
                    &mut tasks,
                    serde_json::json!({"timestamp":"2026-09-18T10:00:01Z","type":"response_item","payload":{"type":"function_call","name":name,"call_id":"question"}}),
                    false,
                );
                assert_eq!(tasks.values().next().unwrap(), &resumed);
                // A genuinely new question still waits and accepts its answer.
                feed(
                    &mut c,
                    &mut tasks,
                    serde_json::json!({"timestamp":"2026-09-18T10:03:00Z","type":"response_item","payload":{"type":"function_call","name":name,"call_id":"next-question"}}),
                    false,
                );
                assert_eq!(tasks.values().next().unwrap().status, Status::NeedsInput);
                feed(
                    &mut c,
                    &mut tasks,
                    serde_json::json!({"timestamp":"2026-09-18T10:03:01Z","type":"event_msg","payload":{"type":"user_message"}}),
                    false,
                );
                assert_eq!(tasks.values().next().unwrap().status, Status::Running);
            }
        }
    }

    #[test]
    fn async_ack_and_bookkeeping_keep_waiting_but_outstanding_tool_completion_resumes() {
        let mut c = Context {
            thread: ID.into(),
            turn: "a".into(),
            ..Default::default()
        };
        let mut tasks = Tasks::new();
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"a"}}),
            false,
        );
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.exec_command","call_id":"command"}}),
            false,
        );
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input_async","call_id":"question"}}),
            false,
        );
        let waiting = tasks.values().next().unwrap().clone();
        for record in [
            serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"question"}}),
            serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"unknown"}}),
            serde_json::json!({"type":"event_msg","payload":{"type":"token_count"}}),
            serde_json::json!({"type":"event_msg","payload":{"type":"future_event"}}),
        ] {
            feed(&mut c, &mut tasks, record, false);
            assert_eq!(tasks.values().next().unwrap(), &waiting);
        }
        feed(
            &mut c,
            &mut tasks,
            serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"command"}}),
            false,
        );
        let resumed = tasks.values().next().unwrap();
        assert_eq!(resumed.status, Status::Running);
        assert_eq!(resumed.progress, Progress::CommandComplete);
    }

    #[test]
    fn answers_and_steps_after_question_ends_cannot_resurrect_terminal_tasks() {
        for (terminal, expected) in [
            ("task_complete", Status::Ready),
            ("task_failed", Status::Blocked),
            ("turn_aborted", Status::Stopped),
        ] {
            let mut c = Context {
                thread: ID.into(),
                turn: "a".into(),
                ..Default::default()
            };
            let mut tasks = Tasks::new();
            feed(
                &mut c,
                &mut tasks,
                serde_json::json!({"type":"response_item","payload":{"type":"function_call","name":"functions.request_user_input_async","call_id":"question"}}),
                false,
            );
            feed(
                &mut c,
                &mut tasks,
                serde_json::json!({"type":"event_msg","payload":{"type":terminal}}),
                false,
            );
            let ended = tasks.values().next().unwrap().clone();
            assert_eq!(ended.status, expected);
            for late in [
                serde_json::json!({"type":"event_msg","payload":{"type":"user_message"}}),
                serde_json::json!({"type":"event_msg","payload":{"type":"user_input_received"}}),
                serde_json::json!({"type":"event_msg","payload":{"type":"approval_resolved"}}),
                serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"question"}}),
                serde_json::json!({"type":"response_item","payload":{"type":"reasoning"}}),
                serde_json::json!({"type":"event_msg","payload":{"type":"exec_command_end"}}),
            ] {
                feed(&mut c, &mut tasks, late, false);
                assert_eq!(tasks.values().next().unwrap(), &ended);
            }
            feed(
                &mut c,
                &mut tasks,
                serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"b"}}),
                false,
            );
            assert_eq!(tasks.len(), 1);
            assert_eq!(tasks.values().next().unwrap().status, Status::Running);
        }
    }
}
