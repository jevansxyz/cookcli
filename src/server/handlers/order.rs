use crate::server::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use camino::{Utf8Component, Utf8Path};
use serde::Deserialize;
use std::sync::Arc;

fn check_path(p: &str) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let path = Utf8Path::new(p);
    if !path
        .components()
        .all(|c| matches!(c, Utf8Component::Normal(_)))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("Invalid path: {p}") })),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct OrderBody {
    pub order: Vec<String>,
}

pub async fn save_order_root(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OrderBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    write_order_file(&state.base_path, body.order).await
}

pub async fn save_order_path(
    Path(path): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<OrderBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    check_path(&path)?;
    let dir = state.base_path.join(&path);
    write_order_file(&dir, body.order).await
}

async fn write_order_file(
    dir: &camino::Utf8Path,
    order: Vec<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let order_file = dir.join(".order");
    let content = order.join("\n");
    tokio::fs::write(&order_file, content).await.map_err(|e| {
        tracing::error!("Failed to write .order file at {order_file}: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
