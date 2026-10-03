//! Silent PCM playback in a hidden, isolated WebView2; no real TTS or credentials.
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::ipc::Channel;
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Audio {
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
#[tauri::command]
async fn synthesize_speech(on_audio: Channel<Audio>, text: String) -> Result<bool, String> {
    on_audio
        .send(Audio::Start {
            format: "pcm16",
            sample_rate: 24_000,
            volume: 0.0,
        })
        .unwrap();
    use base64::Engine;
    let silence = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 2400]);
    for _ in 0..8 {
        on_audio
            .send(Audio::Chunk {
                data: silence.clone(),
            })
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(8)).await;
    }
    println!("Synthetic stream returned: {text}");
    Ok(true)
}
#[tauri::command]
fn cancel_speech(request_id: String) {
    println!("Synthetic cancellation received: {request_id}");
}
#[tauri::command]
fn smoke_done(
    app: tauri::AppHandle,
    state: tauri::State<Arc<AtomicBool>>,
    error: Option<String>,
    started: u32,
) {
    if let Some(error) = error {
        eprintln!("Speech WebView2 failed: {error}");
        app.exit(1);
    } else {
        std::fs::write(
            std::env::args().nth(2).expect("completion marker"),
            format!("{started} streamed buffers, cancellation and context cleanup passed"),
        )
        .unwrap();
        println!("Speech WebView2 passed: {started} buffers, streaming, cancellation, context cleanup; silent audio");
        state.store(true, Ordering::SeqCst);
        app.exit(0);
    }
}
fn main() {
    let path = std::env::args().nth(1).expect("local Vite smoke URL");
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("webview");
    let passed = Arc::new(AtomicBool::new(false));
    let outcome = passed.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().identifier = "cn.mollycloud.tts-playback-smoke".into();
    tauri::Builder::default()
        .manage(passed)
        .invoke_handler(tauri::generate_handler![
            synthesize_speech,
            cancel_speech,
            smoke_done
        ])
        .setup(move |app| {
            let watchdog = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(15));
                watchdog.exit(2);
            });
            tauri::WebviewWindowBuilder::new(
                app,
                "speech-smoke",
                tauri::WebviewUrl::External(path.parse().unwrap()),
            )
            .visible(false)
            .skip_taskbar(true)
            .data_directory(directory)
            .build()?;
            Ok(())
        })
        .run(context)
        .unwrap();
    assert!(
        outcome.load(Ordering::SeqCst),
        "Speech playback test did not pass"
    );
}
