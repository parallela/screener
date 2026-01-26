use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use rand::Rng;
use serde::Serialize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

#[derive(Serialize)]
struct UploadResponse {
    link: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

struct AppState {
    storage_path: PathBuf,
    base_url: String,
}

fn generate_id() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..5)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

fn get_extension(filename: &str) -> &str {
    if filename.to_lowercase().ends_with(".png") {
        "png"
    } else if filename.to_lowercase().ends_with(".jpeg") {
        "jpeg"
    } else if filename.to_lowercase().ends_with(".jpg") {
        "jpg"
    } else {
        "png"
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn upload_handler(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, Json<ErrorResponse>)> {
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Failed to read multipart field: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid multipart data".to_string(),
            }),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();

        if name != "file" {
            continue;
        }

        let filename = field.file_name().unwrap_or("image.png").to_string();
        let extension = get_extension(&filename);

        let data = field.bytes().await.map_err(|e| {
            error!("Failed to read file data: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Failed to read file".to_string(),
                }),
            )
        })?;

        if data.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Empty file".to_string(),
                }),
            ));
        }

        let file_id = generate_id();
        let stored_filename = format!("{}.{}", file_id, extension);
        let file_path = state.storage_path.join(&stored_filename);

        fs::write(&file_path, &data).map_err(|e| {
            error!("Failed to write file to {:?}: {}", file_path, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to store file".to_string(),
                }),
            )
        })?;

        let link = format!("{}/{}", state.base_url, file_id);
        info!("Stored {} as {} -> {}", filename, stored_filename, link);

        return Ok(Json(UploadResponse { link }));
    }

    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: "No file provided".to_string(),
        }),
    ))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let storage_path = PathBuf::from(env::var("STORAGE_PATH").unwrap_or_else(|_| "/data".to_string()));
    let base_url = env::var("BASE_URL").unwrap_or_else(|_| "https://i.lstan.eu".to_string());
    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let max_upload_mb: usize = env::var("MAX_UPLOAD_MB")
        .unwrap_or_else(|_| "50".to_string())
        .parse()
        .unwrap_or(50);

    if !storage_path.exists() {
        match fs::create_dir_all(&storage_path) {
            Ok(_) => info!("Created storage directory: {:?}", storage_path),
            Err(e) => {
                error!("Cannot create storage directory: {}", e);
                std::process::exit(1);
            }
        }
    }

    let state = Arc::new(AppState {
        storage_path,
        base_url,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/upload", post(upload_handler))
        .layer(DefaultBodyLimit::max(max_upload_mb * 1024 * 1024))
        .layer(cors)
        .with_state(state);

    info!("Starting remote server on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
