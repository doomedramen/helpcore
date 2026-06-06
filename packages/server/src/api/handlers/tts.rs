use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;

use crate::{
    api::{error::AppError, extractor::AuthUser},
    plugins::secrets,
    state::AppState,
};

#[derive(Deserialize)]
pub struct TtsRequest {
    pub text: String,
    #[serde(default)]
    pub voice: Option<String>,
}

pub async fn tts_handler(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Json(request): Json<TtsRequest>,
) -> Result<(StatusCode, HeaderMap, Vec<u8>), AppError> {
    let text = request.text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("text is required".into()));
    }

    let uid = auth_user.id.clone();
    let pid = "voice".to_string();

    let (config_str, secrets_encrypted) = state
        .db
        .call(move |conn| {
            conn.query_row(
                "SELECT config, secrets FROM plugin_installs
                 WHERE user_id = ?1 AND plugin_id = ?2",
                rusqlite::params![uid, pid],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .map_err(|e| anyhow::anyhow!("{e}"))
        })
        .await
        .map_err(|_| AppError::BadRequest(
            "Voice plugin is not installed or not configured. Install and configure the Voice plugin first.".into()
        ))?;

    let config: serde_json::Value = serde_json::from_str(&config_str)?;

    let tts_url = config
        .get("tts_url")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_end_matches('/').to_string())
        .ok_or_else(|| {
            AppError::BadRequest(
                "TTS URL is not configured. Configure the Voice plugin's TTS URL first.".into(),
            )
        })?;

    let tts_api_key = secrets_encrypted.and_then(|encrypted| {
        secrets::decrypt(&state.data_dir, &auth_user.id, "voice", &encrypted)
            .ok()
            .and_then(|v| {
                v.get("tts_api_key")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
    });

    let model = config
        .get("tts_model")
        .and_then(|v| v.as_str())
        .unwrap_or("tts-1");

    let voice = request
        .voice
        .as_deref()
        .or_else(|| config.get("tts_voice").and_then(|v| v.as_str()))
        .unwrap_or("alloy");

    let tts_body = serde_json::json!({
        "model": model,
        "input": text,
        "voice": voice,
    });

    let client = reqwest::Client::new();
    let mut req_builder = client
        .post(format!("{}/v1/audio/speech", tts_url))
        .json(&tts_body);

    if let Some(ref api_key) = tts_api_key {
        req_builder = req_builder.header("Authorization", format!("Bearer {api_key}"));
    }

    let response = req_builder
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("TTS request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Upstream(format!(
            "TTS service returned HTTP {status}: {body}"
        )));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("audio/mpeg")
        .to_string();

    let audio_bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Upstream(format!("Failed to read TTS response: {e}")))?;

    let mut headers = HeaderMap::new();
    headers.insert(reqwest::header::CONTENT_TYPE, content_type.parse().unwrap());

    Ok((StatusCode::OK, headers, audio_bytes.to_vec()))
}
