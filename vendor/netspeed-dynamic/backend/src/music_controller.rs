use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{command, AppHandle, Emitter};
use tokio_tungstenite::connect_async;
use std::sync::OnceLock;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn get_http_client() -> reqwest::Client {
    HTTP_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(4))
            .build()
            .unwrap_or_default()
    }).clone()
}

// --- 引入 SMTC 需要的模块 ---
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

// 当前选中的媒体平台（由前端设置，空值表示尚未选择）
static TARGET_PLAYER: Mutex<String> = Mutex::new(String::new());

// 记录上一次是否发现 JustSolo，仅在“从无到有”的跳变时通知前端，避免轮询式重复触发
static LAST_JUSTSOLO_FOUND: AtomicBool = AtomicBool::new(false);

// 供前端调用：切换目标媒体平台
#[command]
pub fn set_target_player(player: String) {
    if let Ok(mut target) = TARGET_PLAYER.lock() {
        *target = player;
    }
}

// 根据前端选择的平台，匹配对应的系统媒体会话，返回会话及其应用包名
fn get_target_media_session() -> Option<(GlobalSystemMediaTransportControlsSession, String)> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .ok()?
        .get()
        .ok()?;

    let sessions = manager.GetSessions().ok()?;

    // 获取当前目标平台（前端未设置时默认 netease）
    let target = {
        let guard = TARGET_PLAYER.lock().unwrap_or_else(|e| e.into_inner()); // 锁中毒时用内部值兜底
        if guard.is_empty() {
            "netease".to_string()
        } else {
            guard.clone()
        }
    };

    // 通用模式（other）：优先匹配 JustSolo
    if target == "other" {
        // 第一轮：优先寻找 JustSolo 会话
        for session in manager.GetSessions().ok()? {
            if let Ok(app_id) = session.SourceAppUserModelId() {
                let app_id_str = app_id.to_string().to_lowercase();
                if app_id_str.contains("douyin") {
                    return None;
                }
                if app_id_str.contains("justsolo") {
                    return Some((session, app_id_str));
                }
            }
        }
        // 第二轮：未找到 JustSolo 时，回退返回第一个有效媒体会话
        for session in manager.GetSessions().ok()? {
            if let Ok(app_id) = session.SourceAppUserModelId() {
                return Some((session, app_id.to_string().to_lowercase()));
            }
        }
        return None;
    }

    // 指定平台：按包名匹配
    for session in sessions {
        if let Ok(app_id) = session.SourceAppUserModelId() {
            let app_id_str = app_id.to_string().to_lowercase();

            if app_id_str.contains("douyin") {
                return None;
            }

            // 网易云包名可能是 cloudmusic 或 netease
            if target == "netease"
                && (app_id_str.contains("cloudmusic") || app_id_str.contains("netease"))
            {
                return Some((session, app_id_str));
            }
            // 落雪音乐：包名为 cn.toside.music.desktop，lx-music 作为备用包名
            else if target == "lx-music"
                && (app_id_str.contains("cn.toside.music.desktop") || app_id_str.contains("lx-music"))
            {
                return Some((session, app_id_str));
            }
            // 浏览器Pro：基于通用媒体模式，识别所有媒体（不限定 Edge/Chrome）
            else if target == "browserPro" {
                return Some((session, app_id_str));
            }
            // 其他平台：按包名包含目标平台名匹配
            else if target != "netease" && app_id_str.contains(&target) {
                return Some((session, app_id_str));
            }
        }
    }
    None
}

#[command]
pub async fn fetch_netease_music_info(
    app: tauri::AppHandle,
) -> Result<Option<(String, String, bool, i64, i64, String)>, String>
{
    let (session, app_id_str) = match get_target_media_session() {
        Some(s) => s,
        None => {
            // 无媒体会话时重置发现标志，保证 JustSolo 下次出现时能再次通知前端
            LAST_JUSTSOLO_FOUND.store(false, Ordering::Relaxed);
            return Ok(None);
        }
    };

    // 后端在 SMTC 中首次发现 JustSolo（从无到有的跳变）时，通知前端发起 WS 连接/重连
    if app_id_str.contains("justsolo") {
        if !LAST_JUSTSOLO_FOUND.swap(true, Ordering::Relaxed) {
            let _ = app.emit("molly-netspeed:justsolo-discovered", ());
        }
    } else {
        LAST_JUSTSOLO_FOUND.store(false, Ordering::Relaxed);
    }

    let is_playing = if let Ok(playback_info) = session.GetPlaybackInfo() {
        if let Ok(status) = playback_info.PlaybackStatus() {
            status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
        } else {
            false
        }
    } else {
        false
    };

    let properties = session
        .TryGetMediaPropertiesAsync()
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())?;

    let title = properties.Title().unwrap_or_default().to_string();
    let artist = properties.Artist().unwrap_or_default().to_string();

    if title.is_empty() {
        // SMTC 已连上应用但尚未提供有效标题：仍返回会话信息（空标题 + 应用包名），
        // 让前端把单行展示改为显示"已连接的应用名"，而不是"未在播放"
        return Ok(Some((
            String::new(),
            String::new(),
            is_playing,
            0,
            0,
            app_id_str,
        )));
    }

    let mut position_ms: i64 = 0;
    let mut duration_ms: i64 = 0; // 歌曲总时长（毫秒）

    if title.contains("抖音") || title.contains("douyin") { // 识别到抖音
        return Ok(None);
    }

    if let Ok(timeline) = session.GetTimelineProperties() {
        if let Ok(pos) = timeline.Position() {
            position_ms = pos.Duration / 10000;

            // 提取歌曲总时长
            if let Ok(end) = timeline.EndTime() {
                duration_ms = end.Duration / 10000;
            }

            // 播放中时，用当前时间补偿位置偏移
            if is_playing {
                if let Ok(last_updated) = timeline.LastUpdatedTime() {
                    if let Ok(now) =
                        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                    {
                        let current_100ns = (now.as_nanos() / 100) as i64 + 116_444_736_000_000_000;
                        let diff_100ns = current_100ns - last_updated.UniversalTime;
                        let diff_ms = diff_100ns / 10000;

                        if diff_ms > 0 && diff_ms < 86400000 {
                            position_ms += diff_ms;
                        }
                    }
                }
            }
        }
    }

    // 识别到特定应用：bilibili 固定用应用名作歌手；edge/chrome/potplayer 在歌手缺失时用应用名兜底
    for app_name in ["bilibili", "edge", "chrome", "potplayer"] {
        if app_id_str.contains(app_name) {
            let fallback_artist = if app_name == "bilibili" {
                "bilibili".to_string()
            } else if artist.is_empty() {
                app_name.to_string()
            } else {
                artist.clone()
            };
            return Ok(Some((title, fallback_artist, is_playing, position_ms, duration_ms, app_id_str)));
        }
    }

    // 返回：标题、歌手、是否播放、当前位置、总时长、应用包名
    Ok(Some((title, artist, is_playing, position_ms, duration_ms, app_id_str)))
}

#[command]
pub async fn control_system_media(action: String) -> Result<(), String> {
    if let Some((session, _)) = get_target_media_session() {
        match action.as_str() {
            "play_pause" => {
                let _ = session.TryTogglePlayPauseAsync();
            }
            "next" => {
                let _ = session.TrySkipNextAsync();
            }
            "prev" => {
                let _ = session.TrySkipPreviousAsync();
            }
            _ => {}
        }
    }
    Ok(())
}

// 轻量 Base64 编码（避免为单处使用引入额外依赖）
fn inline_base64_encode(input: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        match chunk.len() {
            3 => {
                result.push(CHARSET[(chunk[0] >> 2) as usize] as char);
                result.push(CHARSET[(((chunk[0] & 0x03) << 4) | (chunk[1] >> 4)) as usize] as char);
                result.push(CHARSET[(((chunk[1] & 0x0F) << 2) | (chunk[2] >> 6)) as usize] as char);
                result.push(CHARSET[(chunk[2] & 0x3F) as usize] as char);
            }
            2 => {
                result.push(CHARSET[(chunk[0] >> 2) as usize] as char);
                result.push(CHARSET[(((chunk[0] & 0x03) << 4) | (chunk[1] >> 4)) as usize] as char);
                result.push(CHARSET[((chunk[1] & 0x0F) << 2) as usize] as char);
                result.push('=');
            }
            1 => {
                result.push(CHARSET[(chunk[0] >> 2) as usize] as char);
                result.push(CHARSET[((chunk[0] & 0x03) << 4) as usize] as char);
                result.push('=');
                result.push('=');
            }
            _ => {}
        }
    }
    result
}

// 通过 SMTC API 读取当前媒体的本地封面，转为 base64 数据 URI
fn get_smtc_thumbnail() -> Option<String> {
    use windows::Storage::Streams::{Buffer, DataReader, InputStreamOptions};

    let (session, _) = get_target_media_session()?;
    let properties = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let thumbnail_ref = properties.Thumbnail().ok()?;
    let stream = thumbnail_ref.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()? as u32;
    if size == 0 {
        return None;
    }

    let buffer = Buffer::Create(size).ok()?;
    stream
        .ReadAsync(&buffer, size, InputStreamOptions::None)
        .ok()?
        .get()
        .ok()?;
    let reader = DataReader::FromBuffer(&buffer).ok()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;

    Some(format!(
        "data:image/jpeg;base64,{}",
        inline_base64_encode(&bytes)
    ))
}

// 仅尝试读取 SMTC 本地封面，不联网兜底（浏览器/视频类应用专用）
#[command]
pub async fn get_smtc_cover() -> Result<Option<String>, String> {
    Ok(get_smtc_thumbnail())
}

#[command]
pub async fn get_random_cover_url(
    song_name: String,
    artist_name: String,
    prefer_smtc: Option<bool>,
) -> Result<String, String> {
    let url = find_cover_url(song_name, artist_name, prefer_smtc).await?;
    if url.starts_with("data:image/") && url.len() < 6 * 1024 * 1024 { return Ok(url); }
    let parsed = reqwest::Url::parse(&url).map_err(|e| e.to_string())?;
    let host = parsed.host_str().unwrap_or("");
    if parsed.scheme() != "https" || !["mzstatic.com", "music.126.net", "dzcdn.net"].iter().any(|domain| host == *domain || host.ends_with(&format!(".{domain}"))) {
        return Err("封面来源不受支持".into());
    }
    // Return a bounded image, so the island needs no remote-image CSP exception.
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e| e.to_string())?;
    let mut response = client.get(parsed).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
    let mime = response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|h| h.to_str().ok()).unwrap_or("").split(';').next().unwrap_or("").to_owned();
    if !["image/jpeg", "image/png", "image/webp"].contains(&mime.as_str()) { return Err("封面格式不受支持".into()); }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 { return Err("封面图片过大".into()); }
        bytes.extend_from_slice(&chunk);
    }
    Ok(format!("data:{mime};base64,{}", inline_base64_encode(&bytes)))
}

async fn find_cover_url(
    song_name: String,
    artist_name: String,
    prefer_smtc: Option<bool>,
) -> Result<String, String> {
    // 默认优先用 SMTC 本地封面；PotPlayer 音乐模式等场景前端会显式传 prefer_smtc=false 强制走网络封面
    if prefer_smtc.unwrap_or(true) {
        if let Some(base64_cover) = get_smtc_thumbnail() {
            return Ok(base64_cover);
        }
    }

    let client = get_http_client();

    let (tx, mut rx) = tokio::sync::mpsc::channel(3);

    // 封面源 1：Apple Music
    let tx_itunes = tx.clone();
    let client_itunes = client.clone();
    let query_itunes = format!("{} {}", song_name, artist_name);
    tokio::spawn(async move {
        let encoded_query = urlencoding::encode(&query_itunes).into_owned();
        let itunes_url = format!(
            "https://itunes.apple.com/search?term={}&media=music&limit=1",
            encoded_query
        );
        if let Ok(resp) = client_itunes.get(&itunes_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(artwork) = json
                    .pointer("/results/0/artworkUrl100")
                    .and_then(|v| v.as_str())
                {
                    let _ = tx_itunes
                        .send(artwork.replace("100x100bb", "300x300bb"))
                        .await;
                }
            }
        }
    });

    // 封面源 2：网易云 API
    let tx_netease = tx.clone();
    let client_netease = client.clone();
    let song_netease = song_name.clone();
    let artist_netease = artist_name.clone();
    tokio::spawn(async move {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";
        let query = format!("{} {}", song_netease, artist_netease);
        if let Ok(resp) = client_netease
            .post("https://music.163.com/api/search/get/web")
            .header("Referer", "https://music.163.com")
            .header("User-Agent", ua)
            .form(&[
                ("s", query.as_str()),
                ("type", "1"),
                ("limit", "1"),
                ("offset", "0"),
            ])
            .send()
            .await
        {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(pic) = json
                    .pointer("/result/songs/0/al/picUrl")
                    .and_then(|v| v.as_str())
                {
                    // 跳过网易云默认占位封面（无专辑图时返回的固定 URL）
                    if !pic.is_empty() && pic != "http://p4.music.126.net/UeTuwE7pvjBpypWLudqukQ==/3135032972947607.jpg" {
                        let _ = tx_netease.send(pic.replace("http://", "https://") + "?param=300y300").await;
                    }
                }
            }
        }
    });

    // 封面源 3：Deezer API
    let tx_deezer = tx.clone();
    let client_deezer = client.clone();
    let song_deezer = song_name.clone();
    let artist_deezer = artist_name.clone();
    tokio::spawn(async move {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";
        let deezer_url = format!(
            "https://api.deezer.com/search?q=track:\"{}\" artist:\"{}\"&limit=1",
            urlencoding::encode(&song_deezer).into_owned(),
            urlencoding::encode(&artist_deezer).into_owned()
        );
        if let Ok(resp) = client_deezer
            .get(&deezer_url)
            .header("User-Agent", ua)
            .send()
            .await
        {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(cover) = json
                    .pointer("/data/0/album/cover_medium")
                    .and_then(|v| v.as_str())
                {
                    if !cover.is_empty() {
                        let _ = tx_deezer.send(cover.to_string()).await;
                    }
                } else if let Some(cover) = json
                    .pointer("/data/0/album/cover_big")
                    .and_then(|v| v.as_str())
                {
                    if !cover.is_empty() {
                        let _ = tx_deezer.send(cover.to_string()).await;
                    }
                }
            }
        }
    });

    match tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await {
        Ok(Some(url)) => Ok(url),
        _ => Ok("data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMTUwIiBoZWlnaHQ9IjE1MCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48ZGVmcz48bGluZWFyR3JhZGllbnQgaWQ9ImciIHgxPSIwJSIgeTE9IjAlIiB4Mj0iMTAwJSIgeTI9IjEwMCUiPjxzdG9wIG9mZnNldD0iMCUiIHN0b3AtY29sb3I9IiNhOGVkZWEiLz48c3RvcCBvZmZzZXQ9IjEwMCUiIHN0b3AtY29sb3I9IiNmZWQ2ZTMiLz48L2xpbmVhckdyYWRpZW50PjwvZGVmcz48cmVjdCB3aWR0aD0iMTUwIiBoZWlnaHQ9IjE1MCIgcng9Ijc1IiBmaWxsPSJ1cmwoI2cpIi8+PC9zdmc+".to_string()),
    }
}

// 从 LRC 元数据标签提取标题/歌手（[ti:标题] [ar:歌手]），取不到返回空字符串
fn extract_lrc_meta(lrc: &str) -> (String, String) {
    let title = lrc
        .lines()
        .find_map(|l| {
            let l = l.trim();
            l.strip_prefix("[ti:").and_then(|rest| rest.strip_suffix(']'))
        })
        .unwrap_or("")
        .trim()
        .to_string();
    let artist = lrc
        .lines()
        .find_map(|l| {
            let l = l.trim();
            l.strip_prefix("[ar:").and_then(|rest| rest.strip_suffix(']'))
        })
        .unwrap_or("")
        .trim()
        .to_string();
    (title, artist)
}

// 单个引擎的搜索结果
struct EngineResult {
    title: String,
    artist: String,
    lrc: String,
}

// 处理单个引擎结果：保存第一个歌词；优先用 LRC 元数据（有歌手直接返回），
// 否则记录搜索结果的标题/歌手作为兜底，返回 None 继续下一个引擎。
fn process_engine_result(
    result: &EngineResult,
    saved_lrc: &mut Option<String>,
    saved_title: &mut String,
    saved_artist: &mut String,
) -> Option<(String, String, String)> {
    if !result.lrc.is_empty() && saved_lrc.is_none() {
        *saved_lrc = Some(result.lrc.clone());
    }
    let (lrc_title, lrc_artist) = extract_lrc_meta(&result.lrc);
    if !lrc_artist.is_empty() {
        return Some((
            lrc_title,
            lrc_artist,
            saved_lrc.clone().unwrap_or_else(|| result.lrc.clone()),
        ));
    }
    if !result.title.is_empty() && !result.artist.is_empty() {
        *saved_title = result.title.clone();
        *saved_artist = result.artist.clone();
    }
    None
}

// 从搜索结果中按 id 匹配找到歌曲，提取标题和歌手（歌手用 " / " 连接）
// 供 QQ音乐 / 网易云两个引擎复用（两者字段名不同，用闭包参数化）
fn extract_title_artist(
    songs: &[serde_json::Value],
    id_matches: impl Fn(&serde_json::Value) -> bool,
    get_name: impl Fn(&serde_json::Value) -> Option<&str>,
    get_singers: impl Fn(&serde_json::Value) -> Option<&Vec<serde_json::Value>>,
) -> (String, String) {
    let title = songs
        .iter()
        .find(|s| id_matches(s))
        .and_then(|s| get_name(s))
        .unwrap_or("")
        .to_string();
    let mut artist = String::new();
    if let Some(singers) = songs
        .iter()
        .find(|s| id_matches(s))
        .and_then(|s| get_singers(s))
    {
        let names: Vec<&str> = singers
            .iter()
            .filter_map(|s| s.get("name").and_then(|v| v.as_str()))
            .collect();
        artist = names.join(" / ");
    }
    (title, artist)
}

// 统一搜索函数：依次尝试 QQ音乐 / 网易云 / LRCLIB
// 歌词用第一个拿到歌词的引擎；标题/歌手优先用 LRC 元数据（[ti:]/[ar:]），
// 若 LRC 没歌手则继续下一个引擎用搜索结果提取标题/歌手。
async fn search_song_meta(
    song_name: &str,
    artist_name: &str,
    duration_ms: i64,
) -> Option<(String, String, String)> {
    let client = get_http_client();

    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

    // 清洗搜索词：去掉 SMTC 常见的"正在播放: "前缀、" - "分隔符，以及 edge/chrome 等占位歌手
    let mut clean_song = song_name.trim().to_string();
    for prefix in ["正在播放: ", "正在播放：", "Now Playing: ", "Playing: "] {
        if let Some(stripped) = clean_song.strip_prefix(prefix) {
            clean_song = stripped.trim().to_string();
            break;
        }
    }
    // 若歌名形如 "歌名 - 歌手"，只取 " - " 前的歌名部分
    if let Some(idx) = clean_song.find(" - ") {
        clean_song = clean_song[..idx].trim().to_string();
    }
    let clean_artist = {
        let a = artist_name.trim().to_lowercase();
        // 占位歌手（edge/chrome/potplayer/bilibili）与平台名（如"网易云音乐"）都视为无歌手：
        // 平台名作为歌手会污染搜索词（如搜"歌名 网易云音乐"），歌手交给搜索结果的真实字段兜底
        let is_placeholder = a.is_empty()
            || a == "edge"
            || a == "chrome"
            || a == "potplayer"
            || a == "bilibili"
            || [
                "网易云", "云音乐", "qq音乐", "qqmusic", "酷狗", "kugou", "酷我", "kuwo",
                "虾米", "咪咕", "汽水音乐", "5sing", "spotify", "apple music", "itunes",
                "youtube music", "soundcloud", "bandcamp", "tidal", "deezer", "pandora",
                "amazon music", "音乐", "music",
            ]
            .iter()
            .any(|p| a.contains(*p));
        if is_placeholder {
            String::new()
        } else {
            artist_name.trim().to_string()
        }
    };

    let query = format!("{} {}", clean_song, clean_artist);
    let query_name_lower = clean_song.to_lowercase();
    let query_artist_lower = clean_artist.to_lowercase();

    // 保存第一个拿到的歌词，以及搜索结果的标题/歌手（作为 LRC 无歌手时的兜底）
    let mut saved_lrc: Option<String> = None;
    let mut saved_title = String::new();
    let mut saved_artist = String::new();

    // 引擎 1：QQ 音乐（国内优选源）
    let qq_search_url = format!(
        "https://c.y.qq.com/soso/fcgi-bin/client_search_cp?w={}&n=5&format=json",
        urlencoding::encode(&query)
    );

    if let Ok(resp) = client
        .get(&qq_search_url)
        .header("User-Agent", ua)
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(songs) = json.pointer("/data/song/list").and_then(|v| v.as_array()) {
                let mut best_songmid = None;

                for song in songs {
                    let songmid = song.get("songmid").and_then(|v| v.as_str());
                    let interval = song.get("interval").and_then(|v| v.as_i64()).unwrap_or(0);
                    let name = song
                        .get("songname")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_lowercase();

                    let mut singer_name = String::new();
                    if let Some(singers) = song.get("singer").and_then(|v| v.as_array()) {
                        for s in singers {
                            if let Some(sname) = s.get("name").and_then(|v| v.as_str()) {
                                singer_name.push_str(&sname.to_lowercase());
                            }
                        }
                    }

                    let name_match =
                        name.contains(&query_name_lower) || query_name_lower.contains(&name);
                    let artist_match = singer_name.contains(&query_artist_lower)
                        || query_artist_lower.contains(&singer_name)
                        || query_artist_lower.is_empty();

                    let matched = if duration_ms > 0 {
                        let diff = (interval * 1000 - duration_ms).abs();
                        name_match && (artist_match || diff <= 3000)
                    } else {
                        name_match && artist_match
                    };

                    if matched {
                        if let Some(mid) = songmid {
                            best_songmid = Some(mid.to_string());
                            break;
                        }
                    }
                }

                if let Some(songmid) = best_songmid {
                    let (title, artist) = extract_title_artist(
                        songs,
                        |s| s.get("songmid").and_then(|v| v.as_str()) == Some(songmid.as_str()),
                        |s| s.get("songname").and_then(|v| v.as_str()),
                        |s| s.get("singer").and_then(|v| v.as_array()),
                    );

                    let qq_lyric_url = format!("https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid={}&format=json&nobase64=1", songmid);
                    let mut lrc = String::new();
                    if let Ok(lyric_resp) = client
                        .get(&qq_lyric_url)
                        .header("Referer", "https://y.qq.com/")
                        .header("User-Agent", ua)
                        .send()
                        .await
                    {
                        if let Ok(lyric_json) = lyric_resp.json::<serde_json::Value>().await {
                            if let Some(lyric_text) =
                                lyric_json.get("lyric").and_then(|v| v.as_str())
                            {
                                lrc = lyric_text
                                    .replace("&#10;", "\n")
                                    .replace("&#13;", "\r")
                                    .replace("&#32;", " ")
                                    .replace("&#45;", "-")
                                    .replace("&#40;", "(")
                                    .replace("&#41;", ")");
                            }
                        }
                    }
                    if let Some(result) = process_engine_result(
                        &EngineResult { title, artist, lrc },
                        &mut saved_lrc,
                        &mut saved_title,
                        &mut saved_artist,
                    ) {
                        return Some(result);
                    }
                }
            }
        }
    }

    // 引擎 2：网易云（兜底源）
    let fake_ip = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        format!(
            "{}.{}.{}.{}",
            rng.gen_range(11..250),
            rng.gen_range(11..250),
            rng.gen_range(11..250),
            rng.gen_range(11..250)
        )
    };

    if let Ok(resp) = client
        .post("https://music.163.com/api/search/get/web")
        .header("Referer", "https://music.163.com")
        .header("User-Agent", ua)
        .header("X-Real-IP", &fake_ip)
        .form(&[
            ("s", query.as_str()),
            ("type", "1"),
            ("limit", "8"),
            ("offset", "0"),
        ])
        .send()
        .await
    {
        if let Ok(json) = resp.json::<serde_json::Value>().await {
            if let Some(songs) = json.pointer("/result/songs").and_then(|v| v.as_array()) {
                let mut best_song_id = None;
                let mut min_diff = i64::MAX;

                for song in songs {
                    let song_duration = song
                        .get("duration")
                        .or(song.get("dt"))
                        .and_then(|v| v.as_i64());
                    let id = song.get("id").and_then(|v| v.as_i64());
                    let name = song
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_lowercase();

                    let mut singer_name = String::new();
                    if let Some(artists) = song
                        .get("artists")
                        .or(song.get("ar"))
                        .and_then(|v| v.as_array())
                    {
                        for a in artists {
                            if let Some(aname) = a.get("name").and_then(|v| v.as_str()) {
                                singer_name.push_str(&aname.to_lowercase());
                            }
                        }
                    }

                    let name_match =
                        name.contains(&query_name_lower) || query_name_lower.contains(&name);
                    let artist_match = singer_name.contains(&query_artist_lower)
                        || query_artist_lower.contains(&singer_name)
                        || query_artist_lower.is_empty();

                    if let (Some(id), Some(song_dur)) = (id, song_duration) {
                        if duration_ms > 0 {
                            let diff = (song_dur - duration_ms).abs();
                            if name_match && (artist_match || diff <= 3000) {
                                if diff < min_diff {
                                    min_diff = diff;
                                    best_song_id = Some(id);
                                }
                            }
                        } else if name_match && artist_match {
                            best_song_id = Some(id);
                            break;
                        }
                    }
                }

                if let Some(song_id) = best_song_id {
                    let (title, artist) = extract_title_artist(
                        songs,
                        |s| s.get("id").and_then(|v| v.as_i64()) == Some(song_id),
                        |s| s.get("name").and_then(|v| v.as_str()),
                        |s| s.get("artists").or(s.get("ar")).and_then(|v| v.as_array()),
                    );

                    let lyric_url = format!(
                        "https://music.163.com/api/song/lyric?id={}&lv=-1&kv=-1&tv=-1",
                        song_id
                    );
                    let mut lrc = String::new();
                    if let Ok(lyric_resp) = client
                        .get(&lyric_url)
                        .header("User-Agent", ua)
                        .header("X-Real-IP", &fake_ip)
                        .send()
                        .await
                    {
                        if let Ok(lyric_json) = lyric_resp.json::<serde_json::Value>().await {
                            if let Some(lyric_text) =
                                lyric_json.pointer("/lrc/lyric").and_then(|v| v.as_str())
                            {
                                lrc = lyric_text.to_string();
                            }
                        }
                    }
                    if let Some(result) = process_engine_result(
                        &EngineResult { title, artist, lrc },
                        &mut saved_lrc,
                        &mut saved_title,
                        &mut saved_artist,
                    ) {
                        return Some(result);
                    }
                }
            }
        }
    }

    // 引擎 3：LRCLIB（精确匹配，校验度高）
    let duration_sec = duration_ms / 1000;
    if duration_sec > 0 {
        let lrclib_url = format!(
            "https://lrclib.net/api/get?track_name={}&artist_name={}&duration={}",
            urlencoding::encode(&clean_song),
            urlencoding::encode(&clean_artist),
            duration_sec
        );

        if let Ok(resp) = client.get(&lrclib_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                let title = json
                    .pointer("/trackName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let artist = json
                    .pointer("/artistName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let lrc = json
                    .pointer("/syncedLyrics")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                // 最后一个引擎：直接返回。标题/歌手优先用前面引擎搜索到的（更准），
                // 否则用 LRCLIB 的；歌词用第一个拿到的。
                let final_lrc = saved_lrc.unwrap_or(lrc);
                if !saved_title.is_empty() {
                    return Some((saved_title, saved_artist, final_lrc));
                }
                return Some((title, artist, final_lrc));
            }
        }
    }

    // 兜底：用搜索结果的标题/歌手 + 第一个拿到的歌词
    if let Some(lrc) = saved_lrc {
        if !saved_title.is_empty() {
            return Some((saved_title, saved_artist, lrc));
        }
    }

    None
}

#[command]
pub async fn fetch_netease_lyrics(
    song_name: String,
    artist_name: String,
    duration_ms: i64,
) -> Result<String, String> {
    match search_song_meta(&song_name, &artist_name, duration_ms).await {
        Some((_, _, lrc)) if !lrc.is_empty() => Ok(lrc),
        _ => Ok("".to_string()),
    }
}

// 根据歌名/歌手搜索，返回更准确的标题/歌手（只取这两个字段，用于浏览器歌词元数据缺失时兜底）
#[command]
pub async fn fetch_song_meta(
    song_name: String,
    artist_name: String,
    duration_ms: i64,
) -> Result<(String, String), String> {
    match search_song_meta(&song_name, &artist_name, duration_ms).await {
        Some((title, artist, _)) if !title.is_empty() => Ok((title, artist)),
        _ => Err("未找到匹配歌曲元数据".to_string()),
    }
}

// WebSocket 实时歌词推送
async fn run_websocket_lyrics(url: String, app: AppHandle) -> Result<(), String> {
    let (ws_stream, _) = connect_async(&url)
        .await
        .map_err(|e| format!("WebSocket 连接失败: {}", e))?;

    let _ = app.emit("molly-netspeed:websocket-status", true);

    let (mut sender, mut receiver) = ws_stream.split();

    // 协议 v1.1.0+：连接建立后发送 hello 消息声明客户端名称
    let hello = r#"{"type":"hello","client":"NetSpeed Dynamic Pro"}"#;
    if let Err(e) = sender
        .send(tokio_tungstenite::tungstenite::Message::Text(hello.to_string()))
        .await
    {
        println!("[WebSocket 调试] 发送 hello 失败: {}", e);
    }

    while let Some(Ok(msg)) = receiver.next().await {
        if let Ok(text) = msg.to_text() {
            // JSON 解析成功则转发结构化数据；解析失败（如 JustSolo 的纯文本）则原样转发文本
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(text) {
                let _ = app.emit("molly-netspeed:websocket-lyrics", &payload);
            } else {
                let _ = app.emit("molly-netspeed:websocket-lyrics", text);
            }
        }
    }

    let _ = app.emit("molly-netspeed:websocket-status", false);
    Ok(())
}

#[command]
pub async fn start_websocket_lyrics(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
    url: Option<String>,
) -> Result<(), String> {
    let ws_url = url.unwrap_or_else(|| "ws://127.0.0.1:47290/".to_string());

    if ws_url != "ws://127.0.0.1:47290/" { return Err("仅允许本机歌词服务".into()); }
    let mut task_guard = state.ws_task.lock().await;
    if let Some(handle) = task_guard.take() {
        handle.abort();
    }

    let app_clone = app.clone();
    let app_err = app.clone();
    let handle = tokio::spawn(async move {
        if let Err(e) = run_websocket_lyrics(ws_url, app_clone).await {
            eprintln!("WebSocket 歌词任务出错: {}", e);
            let _ = app_err.emit("molly-netspeed:websocket-error", e);
            // 连接失败时，确保前端置灰状态
            let _ = app_err.emit("molly-netspeed:websocket-status", false);
        }
    });

    *task_guard = Some(handle);
    Ok(())
}

#[command]
pub async fn stop_websocket_lyrics(state: tauri::State<'_, crate::AppState>) -> Result<(), String> {
    let mut task_guard = state.ws_task.lock().await;
    if let Some(handle) = task_guard.take() {
        handle.abort();
    }
    Ok(())
}
