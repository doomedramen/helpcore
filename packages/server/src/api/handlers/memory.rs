use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::sync::Arc;

use helpcore_api::{
    MemoryEntry, MemoryListResponse, MemoryReadResponse, MemoryWriteRequest, PersonalityResponse,
    PersonalityWriteRequest,
};

use crate::{
    api::{error::AppError, extractor::AuthUser},
    conversation::memory,
    state::AppState,
};

// ── Personality ───────────────────────────────────────────────────────────────

const VALID_PERSONALITY_NAMES: &[&str] = &["soul", "identity", "user"];

pub async fn get_personality(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(name): Path<String>,
) -> Result<Json<PersonalityResponse>, AppError> {
    if !VALID_PERSONALITY_NAMES.contains(&name.as_str()) {
        return Err(AppError::NotFound);
    }
    let uid = auth_user.id.clone();
    let n = name.clone();
    let (content, updated_at) = state
        .db
        .call(move |conn| memory::get_personality(conn, &uid, &n))
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(PersonalityResponse {
        name,
        content,
        updated_at,
    }))
}

pub async fn put_personality(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(name): Path<String>,
    Json(req): Json<PersonalityWriteRequest>,
) -> Result<StatusCode, AppError> {
    if !VALID_PERSONALITY_NAMES.contains(&name.as_str()) {
        return Err(AppError::NotFound);
    }
    let uid = auth_user.id.clone();
    state
        .db
        .call(move |conn| memory::set_personality(conn, &uid, &name, &req.content))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Memory list ───────────────────────────────────────────────────────────────

pub async fn list_memory(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
) -> Result<Json<MemoryListResponse>, AppError> {
    let uid = auth_user.id.clone();
    let entries = state
        .db
        .call(move |conn| memory::list_memory(conn, &uid))
        .await?;
    let files = entries
        .into_iter()
        .map(|e| MemoryEntry {
            path: e.path,
            updated_at: e.updated_at,
        })
        .collect();
    Ok(Json(MemoryListResponse { files }))
}

// ── Memory file CRUD ──────────────────────────────────────────────────────────

pub async fn get_memory(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(file_path): Path<String>,
) -> Result<Json<MemoryReadResponse>, AppError> {
    let path =
        memory::sanitize_path(&file_path).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let uid = auth_user.id.clone();
    let content = state
        .db
        .call(move |conn| memory::read_memory(conn, &uid, &path))
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(MemoryReadResponse {
        path: file_path,
        content,
    }))
}

pub async fn put_memory(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(file_path): Path<String>,
    Json(req): Json<MemoryWriteRequest>,
) -> Result<StatusCode, AppError> {
    let path =
        memory::sanitize_path(&file_path).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let uid = auth_user.id.clone();
    state
        .db
        .call(move |conn| memory::write_memory(conn, &uid, &path, &req.content))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    auth_user: AuthUser,
    Path(file_path): Path<String>,
) -> Result<StatusCode, AppError> {
    let path =
        memory::sanitize_path(&file_path).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let uid = auth_user.id.clone();
    let deleted = state
        .db
        .call(move |conn| memory::delete_memory(conn, &uid, &path))
        .await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}
