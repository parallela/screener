use cli_clipboard::{ClipboardContext, ClipboardProvider};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

#[derive(Deserialize)]
struct UploadResponse {
    link: String,
}

struct DaemonConfig {
    watch_folder: PathBuf,
    api_url: String,
    secret_key: String,
}

fn load_config() -> DaemonConfig {
    dotenvy::dotenv().ok();

    let watch_folder = env::var("WATCH_FOLDER").unwrap_or_else(|_| "/watch".to_string());
    let api_url = env::var("API_URL").unwrap_or_else(|_| "http://localhost:3000/upload".to_string());
    let secret_key = env::var("SECRET_KEY").expect("SECRET_KEY must be set");

    DaemonConfig {
        watch_folder: PathBuf::from(watch_folder),
        api_url,
        secret_key,
    }
}

fn is_image_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let lower = ext.to_lowercase();
            lower == "png" || lower == "jpeg" || lower == "jpg"
        }
        None => false,
    }
}

fn copy_to_clipboard(text: &str) {
    match ClipboardContext::new() {
        Ok(mut ctx) => {
            match ctx.set_contents(text.to_string()) {
                Ok(_) => info!("Clipboard: copied {}", text),
                Err(e) => error!("Clipboard: failed to set text: {}", e),
            }
        }
        Err(e) => error!("Clipboard: failed to access: {}", e),
    }
}

fn read_stable_file(path: &Path) -> Option<Vec<u8>> {
    const MAX_ATTEMPTS: usize = 20;
    const POLL_INTERVAL: Duration = Duration::from_millis(250);
    const STABLE_READS_REQUIRED: usize = 3;

    let mut last_size = None;
    let mut stable_reads = 0;

    for attempt in 1..=MAX_ATTEMPTS {
        match fs::read(path) {
            Ok(content) if !content.is_empty() => {
                let size = content.len();

                if last_size == Some(size) {
                    stable_reads += 1;
                } else {
                    last_size = Some(size);
                    stable_reads = 1;
                }

                if stable_reads >= STABLE_READS_REQUIRED {
                    return Some(content);
                }
            }
            Ok(_) => {
                last_size = Some(0);
                stable_reads = 0;
            }
            Err(e) => {
                if attempt == MAX_ATTEMPTS {
                    error!("Failed to read file {:?}: {}", path, e);
                    return None;
                }
            }
        }

        if attempt < MAX_ATTEMPTS {
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    warn!("File did not become stable in time: {:?}", path);
    None
}

fn upload_file(config: &DaemonConfig, file_path: &Path) -> bool {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let path_owned = file_path.to_path_buf();

    rt.block_on(async {
        let file_name = path_owned
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("image.png")
            .to_string();

        let file_content = match read_stable_file(&path_owned) {
            Some(content) => content,
            None => return false,
        };

        let mime_type = if file_name.to_lowercase().ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg"
        };

        let part = Part::bytes(file_content)
            .file_name(file_name.clone())
            .mime_str(mime_type)
            .unwrap();

        let form = Form::new().part("file", part);

        let client = reqwest::Client::new();

        info!("Uploading {} to {}", file_name, config.api_url);

        match client
            .post(&config.api_url)
            .header("X-Secret-Key", &config.secret_key)
            .multipart(form)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<UploadResponse>().await {
                        Ok(upload_resp) => {
                            info!("Uploaded {}: {}", file_name, upload_resp.link);
                            copy_to_clipboard(&upload_resp.link);
                            match fs::remove_file(&path_owned) {
                                Ok(_) => info!("Deleted local file: {:?}", path_owned),
                                Err(e) => error!("Failed to delete local file {:?}: {}", path_owned, e),
                            }
                            return true;
                        }
                        Err(e) => {
                            error!("Failed to parse upload response: {}", e);
                        }
                    }
                } else {
                    error!("Upload failed with status: {}", response.status());
                }
            }
            Err(e) => {
                error!("Upload request failed: {}", e);
            }
        }
        false
    })
}


fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = load_config();

    info!("Starting local daemon");
    info!("Watching folder: {:?}", config.watch_folder);
    info!("API URL: {}", config.api_url);

    if !config.watch_folder.exists() {
        match fs::create_dir_all(&config.watch_folder) {
            Ok(_) => info!("Created watch folder: {:?}", config.watch_folder),
            Err(e) => {
                error!("Cannot create watch folder: {}", e);
                std::process::exit(1);
            }
        }
    }

    let (tx, rx) = channel();

    let watcher_config = Config::default()
        .with_poll_interval(Duration::from_secs(1));

    let mut watcher: RecommendedWatcher = match Watcher::new(tx, watcher_config) {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to create watcher: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = watcher.watch(&config.watch_folder, RecursiveMode::NonRecursive) {
        error!("Failed to watch folder: {}", e);
        std::process::exit(1);
    }

    info!("Watcher started successfully");

    let mut processed: HashMap<PathBuf, Instant> = HashMap::new();

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                handle_event(&config, event, &mut processed);
            }
            Ok(Err(e)) => {
                warn!("Watch error: {}", e);
            }
            Err(e) => {
                error!("Channel error: {}", e);
                break;
            }
        }
    }
}

fn handle_event(config: &DaemonConfig, event: Event, processed: &mut HashMap<PathBuf, Instant>) {
    let debounce_duration = Duration::from_secs(2);

    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            for path in &event.paths {
                if !path.is_file() || !is_image_file(path) {
                    continue;
                }

                if let Some(last_time) = processed.get(path) {
                    if last_time.elapsed() < debounce_duration {
                        continue;
                    }
                }

                info!("File detected: {:?}", path);
                processed.insert(path.clone(), Instant::now());
                upload_file(config, path);
            }
        }
        _ => {}
    }
}
