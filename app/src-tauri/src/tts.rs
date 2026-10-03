//! Online speech: credentials and upstream requests stay in the native process.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{sync::Mutex, time::Duration};
use tauri::{ipc::Channel, AppHandle, Manager, State, Webview};
use tokio::sync::watch;

const MIMO_BASE: &str = "https://api.xiaomimimo.com/v1";
const MAX_AUDIO: usize = 24 * 1024 * 1024;
const MAX_EVENT: usize = 2 * 1024 * 1024;
const MIMO_STYLE: &str = r#"〖角色〗
一个十四五岁到十六岁左右的萝莉少女，声线比普通少女更高、更亮、更清脆，接近派蒙那种动画小精灵式的高音少女音。性格活泼、黏人、爱撒娇，带一点小傲娇和小得意，情绪外放，说话有点夸张。

〖场景〗
你正要出门，她跟在你旁边，急着想让你留下来陪她，或者带她一起。她一边耍赖一边撒娇，偶尔带点奶凶的“喂！”和“不行！”。

〖指导〗
- 整体音色：高音调、清亮、奶甜、偏尖但不刺耳；共鸣位置靠前，有动画少女/小精灵的明亮感。
- 语速与节奏：轻快偏快，短句多，节奏跳跃；尾音经常上扬，撒娇句可以轻轻拖长。
- 情绪基调：活泼、撒娇、黏人、小傲娇、奶凶；精神饱满，反应夸张。
- 气声运用：少用气声，声音要实、亮、甜；不要沙哑、不要慵懒、不要低沉。
- 重点处理：在“不行”“陪我”“好不好嘛”“喂”这类词上提高音调，加强上扬和耍赖感，做出派蒙式的奶凶和娇憨。
- 避免：御姐音、成熟感、低沉、慢速、刚睡醒、过分尖锐刺耳。"#;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeechConfig {
    enabled: bool,
    provider: String,
    base_url: String,
    model: String,
    voice: String,
    volume: f32,
}
impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "mimo".into(),
            base_url: MIMO_BASE.into(),
            model: "mimo-v2.5-tts".into(),
            voice: "冰糖".into(),
            volume: 0.8,
        }
    }
}
#[derive(Default, Deserialize, Serialize)]
struct StoredSpeech {
    config: SpeechConfig,
    api_key: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechSettings {
    config: SpeechConfig,
    api_key_configured: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpeechUpdate {
    config: SpeechConfig,
    api_key: Option<String>,
    #[serde(default)]
    clear_api_key: bool,
}
struct Active {
    id: String,
    cancelled: watch::Sender<bool>,
}
#[derive(Default)]
pub struct SpeechState {
    active: Mutex<Option<Active>>,
    settings_lock: Mutex<()>,
}
impl SpeechState {
    fn stop(&self, id: Option<&str>) {
        if let Ok(mut slot) = self.active.lock() {
            if slot
                .as_ref()
                .is_some_and(|active| id.is_none_or(|id| active.id == id))
            {
                if let Some(active) = slot.take() {
                    let _ = active.cancelled.send(true);
                }
            }
        }
    }
}
struct StreamGuard<'a> {
    state: &'a SpeechState,
    id: String,
}
impl Drop for StreamGuard<'_> {
    fn drop(&mut self) {
        self.state.stop(Some(&self.id));
    }
}
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SpeechAudio {
    Start {
        format: &'static str,
        #[serde(rename = "sampleRate")]
        sample_rate: u32,
        volume: f32,
    },
    Chunk {
        data: String,
    },
}
fn allowed(label: &str) -> Result<(), String> {
    if matches!(label, "main" | "console") {
        Ok(())
    } else {
        Err("此窗口不能访问语音设置或播放".into())
    }
}
fn validate_id(id: &str) -> Result<(), String> {
    if !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err("语音请求 ID 无效".into())
    }
}
fn validate_config(mut config: SpeechConfig) -> Result<SpeechConfig, String> {
    if !config.volume.is_finite() || !(0.0..=1.0).contains(&config.volume) {
        return Err("语音音量无效".into());
    }
    match config.provider.as_str() {
        "mimo" => {
            config.base_url = MIMO_BASE.into();
            config.model = "mimo-v2.5-tts".into();
            config.voice = "冰糖".into();
        }
        "custom" => {
            let base = config.base_url.trim().trim_end_matches('/');
            let url = url::Url::parse(base).map_err(|_| "请输入有效的语音 API 端点")?;
            let local = matches!(
                url.host_str(),
                Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
            );
            if url.host_str().is_none()
                || !(url.scheme() == "https" || local && url.scheme() == "http")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || base.len() > 2048
            {
                return Err(
                    "语音端点须使用 HTTPS，本机可用 HTTP；不能包含账号、查询参数或片段".into(),
                );
            }
            config.base_url = base.to_string();
            config.model = config.model.trim().into();
            config.voice = config.voice.trim().into();
            if config.model.is_empty()
                || config.model.chars().count() > 160
                || config.voice.is_empty()
                || config.voice.chars().count() > 160
            {
                return Err("请填写有效的语音模型与音色".into());
            }
        }
        _ => return Err("不支持该语音服务商".into()),
    }
    Ok(config)
}
fn same_endpoint(a: &SpeechConfig, b: &SpeechConfig) -> bool {
    a.provider == b.provider && a.base_url == b.base_url
}
fn settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(if cfg!(target_os = "macos") { "tts-settings-v2.bin" } else { "tts-settings.bin" }))
        .map_err(|_| "无法访问语音设置目录".into())
}
fn load(app: &AppHandle) -> Result<StoredSpeech, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(StoredSpeech::default());
    }
    let bytes = std::fs::read(path).map_err(|_| "无法读取语音设置")?;
    let plaintext = crate::dpapi_unprotect(&bytes).map_err(|_| "无法解密语音设置")?;
    let mut stored: StoredSpeech =
        serde_json::from_slice(&plaintext).map_err(|_| "语音设置损坏，请重新配置")?;
    stored.config = validate_config(stored.config)?;
    Ok(stored)
}
fn save(app: &AppHandle, stored: &StoredSpeech) -> Result<(), String> {
    let path = settings_path(app)?;
    let parent = path.parent().ok_or("无法访问语音设置目录")?;
    std::fs::create_dir_all(parent).map_err(|_| "无法创建语音设置目录")?;
    let bytes = serde_json::to_vec(stored).map_err(|_| "无法保存语音设置")?;
    let encrypted = crate::dpapi_protect(&bytes).map_err(|_| "无法加密语音设置")?;
    use std::io::Write;
    let mut file =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "无法创建语音设置临时文件")?;
    file.write_all(&encrypted)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "无法写入语音设置")?;
    file.persist(&path).map_err(|_| "无法保存语音设置")?;
    Ok(())
}
fn public(stored: &StoredSpeech) -> SpeechSettings {
    SpeechSettings {
        config: stored.config.clone(),
        api_key_configured: !stored.api_key.trim().is_empty(),
    }
}
fn updated(previous: &StoredSpeech, update: SpeechUpdate) -> Result<StoredSpeech, String> {
    let config = validate_config(update.config)?;
    let supplied = update.api_key.unwrap_or_default().trim().to_string();
    if supplied.len() > 4096 || supplied.contains(['\r', '\n']) {
        return Err("语音 API 密钥无效".into());
    }
    let api_key = if update.clear_api_key {
        String::new()
    } else if !supplied.is_empty() {
        supplied
    } else if same_endpoint(&previous.config, &config) {
        previous.api_key.clone()
    } else {
        String::new()
    };
    if config.enabled && api_key.is_empty() {
        return Err("请填写独立的语音 API 密钥后开启朗读".into());
    }
    Ok(StoredSpeech { config, api_key })
}
#[tauri::command]
pub fn get_speech_settings(
    webview: Webview,
    app: AppHandle,
    state: State<'_, SpeechState>,
) -> Result<SpeechSettings, String> {
    allowed(webview.label())?;
    let _lock = state
        .settings_lock
        .lock()
        .map_err(|_| "语音设置忙，请稍后重试")?;
    Ok(public(&load(&app)?))
}
#[tauri::command]
pub fn save_speech_settings(
    webview: Webview,
    app: AppHandle,
    state: State<'_, SpeechState>,
    update: SpeechUpdate,
) -> Result<SpeechSettings, String> {
    allowed(webview.label())?;
    let _lock = state
        .settings_lock
        .lock()
        .map_err(|_| "语音设置忙，请稍后重试")?;
    let next = updated(&load(&app)?, update)?;
    save(&app, &next)?;
    state.stop(None);
    Ok(public(&next))
}
#[tauri::command]
pub fn save_pet_credentials(
    webview: Webview,
    app: AppHandle,
    state: State<'_, SpeechState>,
    update: SpeechUpdate,
    assistant_api_key: Option<String>,
) -> Result<SpeechSettings, String> {
    allowed(webview.label())?;
    let _lock = state
        .settings_lock
        .lock()
        .map_err(|_| "语音设置忙，请稍后重试")?;
    let next = updated(&load(&app)?, update)?;
    let key_path = crate::api_key_path(&app)?;
    let old_key = if key_path.exists() {
        Some(std::fs::read(&key_path).map_err(|_| "无法备份原对话密钥")?)
    } else {
        None
    };
    if let Some(key) = &assistant_api_key {
        if key.len() > 4096 {
            return Err("对话密钥过长".into());
        }
        crate::set_api_key(app.clone(), key.clone())?;
    }
    if let Err(error) = save(&app, &next) {
        if assistant_api_key.is_some() {
            let restored = match old_key {
                Some(bytes) => std::fs::write(&key_path, bytes),
                None => std::fs::remove_file(&key_path),
            };
            if restored.is_err() {
                return Err("语音设置保存失败，原对话密钥恢复失败，请重新配置密钥".into());
            }
        }
        return Err(error);
    }
    state.stop(None);
    Ok(public(&next))
}
#[tauri::command]
pub fn cancel_speech(
    webview: Webview,
    state: State<'_, SpeechState>,
    request_id: String,
) -> Result<(), String> {
    allowed(webview.label())?;
    validate_id(&request_id)?;
    state.stop(Some(&request_id));
    Ok(())
}
fn body(config: &SpeechConfig, text: &str) -> (&'static str, Value) {
    if config.provider == "mimo" {
        (
            "chat/completions",
            json!({"model":config.model,"messages":[{"role":"user","content":MIMO_STYLE},{"role":"assistant","content":text}],"audio":{"format":"pcm16","voice":"冰糖"},"stream":true}),
        )
    } else {
        (
            "audio/speech",
            json!({"model":config.model,"input":text,"voice":config.voice,"response_format":"wav"}),
        )
    }
}
#[derive(Default)]
struct SseDecoder {
    pending: Vec<u8>,
    data: Vec<String>,
    done: bool,
    finished: bool,
    audio_bytes: usize,
}
impl SseDecoder {
    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > MAX_EVENT {
            return Err("语音流分块过大".into());
        }
        let mut output = Vec::new();
        while let Some(index) = self.pending.iter().position(|b| *b == b'\n') {
            let line = self.pending.drain(..=index).collect::<Vec<_>>();
            let line = std::str::from_utf8(&line)
                .map_err(|_| "语音流编码无效")?
                .trim_end_matches(['\r', '\n']);
            let line = line.trim_start_matches('\u{feff}');
            if line.is_empty() {
                if self.data.is_empty() {
                    continue;
                }
                let data = self.data.join("\n");
                self.data.clear();
                if data.trim() == "[DONE]" {
                    self.done = true;
                    continue;
                }
                if self.done {
                    continue;
                }
                let value: Value = serde_json::from_str(&data).map_err(|_| "语音流返回格式无效")?;
                if value.get("error").is_some() {
                    return Err("语音服务返回错误，请检查模型、音色与余额".into());
                }
                if value["choices"][0]["finish_reason"].as_str().is_some() {
                    self.finished = true;
                }
                if let Some(data) = value["choices"][0]["delta"]["audio"]["data"].as_str() {
                    let chunk = STANDARD.decode(data).map_err(|_| "语音分块解码失败")?;
                    self.audio_bytes += chunk.len();
                    if self.audio_bytes > MAX_AUDIO {
                        return Err("语音音频超出长度限制".into());
                    }
                    if !chunk.is_empty() {
                        output.push(chunk);
                    }
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data.push(data.trim_start().to_string());
                if self.data.iter().map(String::len).sum::<usize>() > MAX_EVENT {
                    return Err("语音流事件过大".into());
                }
            }
        }
        Ok(output)
    }
    fn complete(&self) -> Result<(), String> {
        if self.audio_bytes == 0 {
            return Err("语音服务未返回音频".into());
        }
        if !self.done && !self.finished || !self.pending.is_empty() || !self.data.is_empty() {
            return Err("语音流提前中断，请重试".into());
        }
        if self.audio_bytes % 2 != 0 {
            return Err("语音 PCM 数据不完整".into());
        }
        Ok(())
    }
}
fn send_chunks(channel: &Channel<SpeechAudio>, bytes: &[u8]) -> Result<(), String> {
    for chunk in bytes.chunks(16 * 1024) {
        channel
            .send(SpeechAudio::Chunk {
                data: STANDARD.encode(chunk),
            })
            .map_err(|_| "语音播放窗口已关闭")?;
    }
    Ok(())
}
#[tauri::command]
pub async fn synthesize_speech(
    webview: Webview,
    app: AppHandle,
    state: State<'_, SpeechState>,
    request_id: String,
    text: String,
    on_audio: Channel<SpeechAudio>,
    preview: Option<SpeechUpdate>,
    use_saved_key: Option<bool>,
) -> Result<bool, String> {
    allowed(webview.label())?;
    validate_id(&request_id)?;
    if text.trim().is_empty() || text.chars().count() > 4096 {
        return Err("朗读文本为空或过长".into());
    }
    if webview.label() != "console" && preview.is_some() {
        return Err("仅控制台可以试听未保存的语音设置".into());
    }
    let stored = {
        let _lock = state.settings_lock.lock().map_err(|_| "语音设置忙")?;
        load(&app)?
    };
    let connection = if let Some(mut update) = preview {
        if !use_saved_key.unwrap_or(false)
            && update
                .api_key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty())
        {
            return Err("请填写语音 API 密钥再试听".into());
        }
        update.config.enabled = true;
        updated(&stored, update)?
    } else {
        if !stored.config.enabled {
            return Ok(false);
        }
        stored
    };
    if connection.api_key.trim().is_empty() {
        return Err("未配置语音 API 密钥".into());
    }
    let (sender, mut cancelled) = watch::channel(false);
    {
        let mut slot = state.active.lock().map_err(|_| "语音播放忙")?;
        if let Some(old) = slot.take() {
            let _ = old.cancelled.send(true);
        }
        *slot = Some(Active {
            id: request_id.clone(),
            cancelled: sender,
        });
    }
    let _guard = StreamGuard {
        state: &state,
        id: request_id,
    };
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| "无法初始化语音连接")?;
    let (path, payload) = body(&connection.config, text.trim());
    let request = client
        .post(format!("{}/{path}", connection.config.base_url))
        .bearer_auth(&connection.api_key)
        .json(&payload);
    let response = tokio::select! { biased;
        _ = cancelled.changed() => return Ok(false),
        result = request.send() => result.map_err(|_| "无法连接语音服务，请检查端点与网络")?,
    };
    stream_response(
        response,
        &on_audio,
        &mut cancelled,
        connection.config.provider == "mimo",
        connection.config.volume,
    )
    .await
}
async fn stream_response(
    mut response: reqwest::Response,
    on_audio: &Channel<SpeechAudio>,
    cancelled: &mut watch::Receiver<bool>,
    mimo: bool,
    volume: f32,
) -> Result<bool, String> {
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 | 403 => "语音密钥无效或没有访问权限".to_string(),
            429 => "语音服务限流或额度不足，请稍后重试".to_string(),
            code => format!("语音服务请求失败（HTTP {code}）"),
        });
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if mimo && !content_type.contains("text/event-stream") {
        return Err("MiMo 未返回流式语音，请检查服务接口".into());
    }
    if !mimo && !(content_type.starts_with("audio/") || content_type.contains("octet-stream")) {
        return Err("自定义语音接口未返回音频".into());
    }
    on_audio
        .send(SpeechAudio::Start {
            format: if mimo { "pcm16" } else { "wav" },
            sample_rate: 24_000,
            volume: volume,
        })
        .map_err(|_| "语音播放窗口已关闭")?;
    let mut decoder = SseDecoder::default();
    let mut total = 0;
    loop {
        let chunk = tokio::select! { biased;
            _ = cancelled.changed() => return Ok(false),
            result = tokio::time::timeout(Duration::from_secs(20), response.chunk()) => result.map_err(|_| "语音流接收超时")?.map_err(|_| "语音连接中断")?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        total += chunk.len();
        if total > MAX_AUDIO * 2 {
            return Err("语音响应过大".into());
        }
        if mimo {
            for audio in decoder.feed(&chunk)? {
                send_chunks(&on_audio, &audio)?;
            }
            if decoder.done {
                break;
            }
        } else {
            send_chunks(&on_audio, &chunk)?;
        }
    }
    if *cancelled.borrow() {
        return Ok(false);
    }
    if mimo {
        decoder.complete()?;
    } else if total == 0 {
        return Err("语音服务未返回音频".into());
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_protocol_match_mimo() {
        let config = validate_config(SpeechConfig::default()).unwrap();
        let (path, payload) = body(&config, "你好");
        assert_eq!(path, "chat/completions");
        assert_eq!(payload["stream"], true);
        assert_eq!(payload["audio"], json!({"voice":"冰糖","format":"pcm16"}));
        assert_eq!(payload["messages"][1]["content"], "你好");
        assert!(payload["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("〖角色〗"));
        assert!(!serde_json::to_string(&public(&StoredSpeech::default()))
            .unwrap()
            .contains("派蒙"));
    }
    #[test]
    fn changing_endpoint_never_reuses_a_saved_secret() {
        let old = StoredSpeech {
            config: SpeechConfig::default(),
            api_key: "synthetic-secret".into(),
        };
        let mut config = old.config.clone();
        config.provider = "custom".into();
        config.base_url = "https://example.com/v1".into();
        config.model = "tts-1".into();
        config.voice = "alloy".into();
        let next = updated(
            &old,
            SpeechUpdate {
                config: config.clone(),
                api_key: None,
                clear_api_key: false,
            },
        )
        .unwrap();
        assert!(next.api_key.is_empty());
        config.enabled = true;
        assert!(updated(
            &old,
            SpeechUpdate {
                config,
                api_key: None,
                clear_api_key: false
            }
        )
        .is_err());
        let value = serde_json::to_string(&public(&old)).unwrap();
        assert!(!value.contains("synthetic-secret"));
    }
    #[test]
    fn custom_endpoints_and_window_scope_are_validated() {
        for label in ["netspeed-widget", "payment-1", "skills", "image"] {
            assert!(allowed(label).is_err());
        }
        for value in [
            "http://example.com/v1",
            "https://user:secret@example.com",
            "https://example.com/?key=x",
        ] {
            let mut c = SpeechConfig::default();
            c.provider = "custom".into();
            c.base_url = value.into();
            assert!(validate_config(c).is_err());
        }
        let mut c = SpeechConfig::default();
        c.provider = "custom".into();
        c.base_url = "http://127.0.0.1:8080/v1/".into();
        assert_eq!(body(&validate_config(c).unwrap(), "hi").0, "audio/speech");
    }
    #[test]
    fn sse_decodes_fragmented_frames_and_checks_completion() {
        let raw = format!("data: {{\"choices\":[{{\"delta\":{{\"audio\":{{\"data\":\"{}\"}}}}}}]}}\r\n\r\ndata: [DONE]\n\n", STANDARD.encode([0,1,2,3]));
        let mut parser = SseDecoder::default();
        let mut audio = Vec::new();
        for b in raw.as_bytes().chunks(3) {
            for c in parser.feed(b).unwrap() {
                audio.extend(c);
            }
        }
        assert_eq!(audio, vec![0, 1, 2, 3]);
        assert!(parser.complete().is_ok());
        let mut partial = SseDecoder::default();
        partial.audio_bytes = 4;
        assert!(partial.complete().is_err());
        assert!(SseDecoder::default()
            .feed(b"data: {\"error\":{}}\n\n")
            .is_err());
        assert!(SseDecoder::default().feed(b"data: invalid\n\n").is_err());
    }
    #[test]
    fn old_cancellation_cannot_stop_new_request() {
        let state = SpeechState::default();
        let (tx, rx) = watch::channel(false);
        *state.active.lock().unwrap() = Some(Active {
            id: "new".into(),
            cancelled: tx,
        });
        state.stop(Some("old"));
        assert!(!*rx.borrow());
        state.stop(Some("new"));
        assert!(*rx.borrow());
    }
    #[tokio::test]
    async fn actual_http_stream_delivers_audio_before_end_and_rejects_truncation() {
        use std::{
            io::{Read, Write},
            sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            },
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let delivered = Arc::new(AtomicBool::new(false));
        let gate = delivered.clone();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut req = [0u8; 2048];
            let _ = socket.read(&mut req).unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n").unwrap();
            let first =
                b"data: {\"choices\":[{\"delta\":{\"audio\":{\"data\":\"AAAAAA==\"}}}]}\n\n";
            write!(socket, "{:X}\r\n", first.len()).unwrap();
            socket.write_all(first).unwrap();
            socket.write_all(b"\r\n").unwrap();
            socket.flush().unwrap();
            // The response is deliberately held open until the consumer gets audio.
            for _ in 0..300 {
                if gate.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                gate.load(Ordering::SeqCst),
                "audio was buffered until EOF instead of streamed"
            );
            socket.write_all(b"0\r\n\r\n").unwrap();
        });
        let mark = delivered.clone();
        let channel = Channel::new(move |message| {
            if let tauri::ipc::InvokeResponseBody::Json(data) = message {
                if data.contains("chunk") {
                    mark.store(true, Ordering::SeqCst);
                }
            }
            Ok(())
        });
        let response = reqwest::get(format!("http://{address}")).await.unwrap();
        let (_tx, mut rx) = watch::channel(false);
        assert!(stream_response(response, &channel, &mut rx, true, 0.8)
            .await
            .unwrap_err()
            .contains("提前中断"));
        server.join().unwrap();
        assert!(delivered.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn cancelled_stream_closes_before_receiving_audio() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut socket, _) = listener.accept().unwrap();
            let mut req = [0u8; 2048];
            let _ = socket.read(&mut req).unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 100\r\n\r\n").unwrap();
        });
        let response = reqwest::get(format!("http://{address}")).await.unwrap();
        let (tx, mut rx) = watch::channel(false);
        tx.send(true).unwrap();
        let channel = Channel::new(|_| Ok(()));
        assert!(!stream_response(response, &channel, &mut rx, true, 0.8)
            .await
            .unwrap());
        server.join().unwrap();
    }
}
