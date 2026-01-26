use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::SystemTime;
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
    match Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                if stdin.write_all(text.as_bytes()).is_ok() {
                    let _ = child.wait();
                    info!("Copied to clipboard: {}", text);
                }
            }
        }
        Err(e) => {
            warn!("Failed to copy to clipboard: {}", e);
        }
    }
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

        let file_content = match fs::read(&path_owned) {
            Ok(content) => content,
            Err(e) => {
                error!("Failed to read file {:?}: {}", path_owned, e);
                return false;
            }
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

fn find_latest_image(folder: &Path) -> Option<PathBuf> {
    let mut latest: Option<(PathBuf, SystemTime)> = None;

    if let Ok(entries) = fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_image_file(&path) {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        match &latest {
                            Some((_, latest_time)) if modified > *latest_time => {
                                latest = Some((path, modified));
                            }
                            None => {
                                latest = Some((path, modified));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    latest.map(|(p, _)| p)
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

    let mut watcher: RecommendedWatcher = match Watcher::new(tx, Config::default()) {
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

    let mut processed: HashSet<PathBuf> = HashSet::new();

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

fn handle_event(config: &DaemonConfig, event: Event, processed: &mut HashSet<PathBuf>) {
    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if path.is_file() && is_image_file(&path) && !processed.contains(&path) {
                    info!("New image detected: {:?}", path);

                    std::thread::sleep(std::time::Duration::from_millis(500));

                    if path.exists() && !processed.contains(&path) {
                        processed.insert(path.clone());
                        upload_file(config, &path);
                    }
                }
            }
        }
        _ => {}
    }
}
