use crate::discovery;
use crate::state::{
    blocked_executable, public_network_warning, AppState, PendingOffer, Peer, WsEvent, HTTP_PORT,
    MAX_CLIP_CHARS,
};
use crate::transfer;
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{connect_info::ConnectInfo, DefaultBodyLimit, Multipart, Path, Query, State, WebSocketUpgrade};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tokio::sync::oneshot;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info};
use uuid::Uuid;

use tauri::Manager;

pub async fn spawn(app: tauri::AppHandle) -> Result<(), String> {
    let state = AppState::new(HTTP_PORT).map_err(|e| e.to_string())?;
    discovery::start(state.clone())?;

    let dist = dist_dir(&app);
    info!("serving UI from {}", dist.display());
    info!("join URL {}", state.join_url());

    let app = router(state, dist);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", HTTP_PORT))
        .await
        .map_err(|e| format!("could not bind port {HTTP_PORT}: {e}"))?;

    tokio::spawn(async move {
        if let Err(err) = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        {
            error!("LAN server stopped: {err}");
        }
    });
    Ok(())
}

fn dist_dir(app: &tauri::AppHandle) -> PathBuf {
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");
    if dev.join("index.html").exists() {
        return dev;
    }
    app.path()
        .resource_dir()
        .map(|p| p.join("dist"))
        .unwrap_or(dev)
}

fn router(state: AppState, dist: PathBuf) -> Router {
    let index = dist.join("index.html");
    let static_files = ServeDir::new(&dist).not_found_service(ServeFile::new(index));

    Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/name", post(set_name))
        .route("/api/save-dir", post(set_save_dir))
        .route("/api/qr.svg", get(qr_svg))
        .route("/api/inbox", get(inbox))
        .route("/api/ws", get(ws_upgrade))
        .route("/api/files/offer", post(offer))
        .route("/api/files/{id}/accept", post(accept))
        .route("/api/files/{id}/decline", post(decline))
        .route("/api/files/{id}/data", post(data))
        .route("/api/files/{id}/download", get(download))
        .route("/api/send", post(send_local))
        .route("/api/web-offer", post(web_offer))
        .route("/api/clips", get(list_clips).post(post_clip).delete(clear_clips))
        .fallback_service(static_files)
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}

fn client_ip(addr: SocketAddr) -> String {
    addr.ip().to_canonical().to_string()
}

fn guest_label(kind: &str, ip: &str) -> String {
    if kind == "phone" {
        format!("Phone {ip}")
    } else {
        format!("Browser {ip}")
    }
}

fn require_loopback(addr: SocketAddr) -> Option<axum::response::Response> {
    if addr.ip().to_canonical().is_loopback() {
        None
    } else {
        Some(
            (
                StatusCode::FORBIDDEN,
                "only the desktop app on this computer can change this",
            )
                .into_response(),
        )
    }
}

fn reject_blocked_file(filename: &str) -> Option<axum::response::Response> {
    if blocked_executable(filename) {
        Some(
            (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "executable files are not allowed",
            )
                .into_response(),
        )
    } else {
        None
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn list_clips(State(state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({ "clips": state.clips().await }))
}

async fn clear_clips(State(state): State<AppState>) -> impl IntoResponse {
    state.clear_clips().await;
    Json(serde_json::json!({ "ok": true })).into_response()
}

#[derive(Deserialize)]
struct ClipReq {
    text: String,
    from_name: Option<String>,
}

async fn post_clip(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<ClipReq>,
) -> impl IntoResponse {
    let text = body.text.trim();
    if text.is_empty() {
        return (StatusCode::BAD_REQUEST, "text is required").into_response();
    }
    if text.chars().count() > MAX_CLIP_CHARS {
        return (
            StatusCode::BAD_REQUEST,
            format!("text must be {MAX_CLIP_CHARS} characters or fewer"),
        )
            .into_response();
    }
    if !state.allow_clip(&client_ip(addr)).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "too many notes; wait a few seconds",
        )
            .into_response();
    }
    let hint = body.from_name.unwrap_or_default();
    let from = if addr.ip().is_loopback() {
        if hint.trim().is_empty() {
            state.inner.name.read().await.clone()
        } else {
            hint
        }
    } else {
        let kind = if hint.to_lowercase().contains("phone") {
            "phone"
        } else {
            "browser"
        };
        guest_label(kind, &client_ip(addr))
    };
    let note = state.push_clip(from, text.to_string()).await;
    Json(note).into_response()
}

fn pending_incoming_json(
    pending: &std::collections::HashMap<String, crate::state::PendingOffer>,
    target_id: Option<&str>,
) -> Vec<serde_json::Value> {
    pending
        .iter()
        .filter(|(_, p)| !p.accepted && target_id.map(|id| p.target_id == id).unwrap_or(true))
        .map(|(id, p)| {
            serde_json::json!({
                "type": "incoming",
                "id": id,
                "filename": p.filename,
                "size": p.size,
                "from": p.from_name,
                "from_id": p.from_id,
                "target_id": p.target_id,
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct InboxQuery {
    #[serde(rename = "for")]
    for_id: Option<String>,
}

/// Only ever answer with offers aimed at the asking device, so a sender is
/// never told about its own offer.
async fn inbox(
    State(state): State<AppState>,
    Query(query): Query<InboxQuery>,
) -> impl IntoResponse {
    let want = query
        .for_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(state.inner.id.as_str());
    let pending = state.inner.pending.read().await;
    let items = pending_incoming_json(&pending, Some(want));
    Json(serde_json::json!({ "id": state.inner.id, "incoming": items }))
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    let name = state.inner.name.read().await.clone();
    Json(serde_json::json!({
        "id": state.inner.id,
        "name": name,
        "port": state.inner.port,
        "url": state.join_url(),
        "save_dir": state.save_dir().await,
        "peers": state.all_peers().await,
        "clips": state.clips().await,
        "public_network": public_network_warning().await,
    }))
}

#[derive(Deserialize)]
struct NameBody {
    name: String,
}

async fn set_name(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<NameBody>,
) -> impl IntoResponse {
    if let Some(denied) = require_loopback(addr) {
        return denied;
    }
    let name = body.name.trim();
    if name.is_empty() || name.len() > 64 {
        return (StatusCode::BAD_REQUEST, "name must be 1-64 characters").into_response();
    }
    *state.inner.name.write().await = name.to_string();
    discovery::reregister(&state);
    state.broadcast_peers().await;
    Json(serde_json::json!({ "name": name })).into_response()
}

#[derive(Deserialize)]
struct SaveDirBody {
    path: String,
}

async fn set_save_dir(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<SaveDirBody>,
) -> impl IntoResponse {
    if let Some(denied) = require_loopback(addr) {
        return denied;
    }
    let path = body.path.trim();
    if path.is_empty() || path.len() > 512 {
        return (StatusCode::BAD_REQUEST, "Choose a folder path.").into_response();
    }
    match state.set_save_dir(PathBuf::from(path)).await {
        Ok(dir) => Json(serde_json::json!({ "save_dir": dir })).into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    }
}

async fn qr_svg(State(state): State<AppState>) -> impl IntoResponse {
    let url = state.join_url();
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let svg = code
                .render::<qrcode::render::svg::Color<'_>>()
                .min_dimensions(220, 220)
                .build();
            ([(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")], svg).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct WsQuery {
    role: Option<String>,
    name: Option<String>,
    id: Option<String>,
    device: Option<String>,
}

async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket, query, addr))
}

async fn handle_socket(state: AppState, socket: WebSocket, query: WsQuery, addr: SocketAddr) {
    let role = query.role.unwrap_or_else(|| "host".into());
    let mut web_id: Option<String> = None;
    if role != "host" {
        let ip = client_ip(addr);
        let id = query
            .id
            .as_deref()
            .map(str::trim)
            .filter(|s| s.starts_with("web-") && s.len() < 80)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("web-{}", Uuid::new_v4()));
        let device = query.device.unwrap_or_else(|| "browser".into());
        let name = query
            .name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| guest_label(&device, &ip));
        tracing::info!("guest connected {name} from {ip}");
        state.inner.web_clients.write().await.insert(
            id.clone(),
            Peer {
                id: id.clone(),
                name,
                host: ip,
                port: state.inner.port,
                kind: "web".into(),
            },
        );
        web_id = Some(id);
        state.broadcast_peers().await;
    }

    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.inner.events.subscribe();
    let inbox_for = web_id.as_deref().unwrap_or(state.inner.id.as_str());
    let incoming = {
        let pending = state.inner.pending.read().await;
        pending_incoming_json(&pending, Some(inbox_for))
    };
    let hello = serde_json::json!({
        "type": "hello",
        "id": state.inner.id,
        "web_id": web_id,
        "name": *state.inner.name.read().await,
        "url": state.join_url(),
        "save_dir": state.save_dir().await,
        "peers": state.all_peers().await,
        "clips": state.clips().await,
        "incoming": incoming,
        "public_network": public_network_warning().await,
    });
    let _ = sender.send(Message::Text(hello.to_string().into())).await;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(ev) => {
                        if sender.send(Message::Text(serde_json::to_string(&ev).unwrap_or_default().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(p))) => {
                        let _ = sender.send(Message::Pong(p)).await;
                    }
                    _ => {}
                }
            }
        }
    }

    if let Some(id) = web_id {
        state.inner.web_clients.write().await.remove(&id);
        state.broadcast_peers().await;
    }
}

#[derive(Deserialize)]
struct OfferReq {
    filename: String,
    size: u64,
    from_name: String,
    from_id: String,
}

async fn offer(State(state): State<AppState>, Json(body): Json<OfferReq>) -> impl IntoResponse {
    if let Some(denied) = reject_blocked_file(&body.filename) {
        return denied;
    }
    let from_id = body.from_id.clone();
    let id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    state.inner.pending.write().await.insert(
        id.clone(),
        PendingOffer {
            filename: body.filename.clone(),
            size: body.size,
            from_name: body.from_name.clone(),
            from_id: from_id.clone(),
            target_id: state.inner.id.clone(),
            decision: Some(tx),
            staged_path: None,
            accepted: false,
            remote_offer_id: None,
        },
    );
    state.emit(WsEvent::Incoming {
        id: id.clone(),
        filename: body.filename,
        size: body.size,
        from: body.from_name,
        from_id,
        target_id: state.inner.id.clone(),
    });

    match tokio::time::timeout(Duration::from_secs(120), rx).await {
        Ok(Ok(true)) => Json(serde_json::json!({ "id": id, "accepted": true })).into_response(),
        Ok(Ok(false)) | Ok(Err(_)) => {
            state.inner.pending.write().await.remove(&id);
            state.emit(WsEvent::Declined { id: id.clone() });
            (StatusCode::FORBIDDEN, Json(serde_json::json!({ "id": id, "accepted": false }))).into_response()
        }
        Err(_) => {
            state.inner.pending.write().await.remove(&id);
            (
                StatusCode::REQUEST_TIMEOUT,
                Json(serde_json::json!({ "id": id, "accepted": false, "error": "timed out" })),
            )
                .into_response()
        }
    }
}

async fn accept(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    {
        let mut pending_map = state.inner.pending.write().await;
        if let Some(pending) = pending_map.get_mut(&id) {
            if let Some(tx) = pending.decision.take() {
                pending.accepted = true;
                let _ = tx.send(true);
                return Json(serde_json::json!({ "ok": true })).into_response();
            }
            if let Some(path) = pending.staged_path.take() {
                let filename = pending.filename.clone();
                pending_map.remove(&id);
                drop(pending_map);
                return commit_staged_file(state, id, filename, path).await;
            }
            pending.accepted = true;
            state.emit(WsEvent::Accepted { id: id.clone() });
            return Json(serde_json::json!({ "ok": true })).into_response();
        }
    }
    if let Some(file) = state.inner.outbound.write().await.get_mut(&id) {
        file.allowed = true;
        return Json(serde_json::json!({ "ok": true, "download": format!("/api/files/{id}/download") })).into_response();
    }
    (StatusCode::NOT_FOUND, "unknown transfer").into_response()
}

async fn decline(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if let Some(mut pending) = state.inner.pending.write().await.remove(&id) {
        if let Some(tx) = pending.decision.take() {
            let _ = tx.send(false);
        }
        if let Some(path) = pending.staged_path.take() {
            let _ = tokio::fs::remove_file(path).await;
        }
        state.emit(WsEvent::Declined { id });
        return Json(serde_json::json!({ "ok": true })).into_response();
    }
    if state.inner.outbound.write().await.remove(&id).is_some() {
        state.emit(WsEvent::Declined { id });
        return Json(serde_json::json!({ "ok": true })).into_response();
    }
    (StatusCode::NOT_FOUND, "unknown transfer").into_response()
}

async fn data(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> impl IntoResponse {
    let Some(meta) = state.inner.pending.write().await.remove(&id) else {
        return (StatusCode::NOT_FOUND, "unknown transfer").into_response();
    };
    if !meta.accepted || meta.decision.is_some() || meta.staged_path.is_some() {
        state.inner.pending.write().await.insert(id, meta);
        return (StatusCode::FORBIDDEN, "not accepted").into_response();
    }
    let filename = headers
        .get("X-Filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or(&meta.filename)
        .to_string();
    if let Some(denied) = reject_blocked_file(&filename) {
        state.inner.pending.write().await.insert(id, meta);
        return denied;
    }
    let stream = body.into_data_stream();
    match transfer::write_incoming_body(&state, &id, &filename, meta.size, stream).await {
        Ok(path) => Json(serde_json::json!({ "saved_path": path })).into_response(),
        Err(err) => {
            state.emit(WsEvent::Error {
                id: Some(id),
                message: err.to_string(),
            });
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
    }
}

async fn download(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let file = {
        let map = state.inner.outbound.read().await;
        map.get(&id).cloned()
    };
    let Some(file) = file else {
        return (StatusCode::NOT_FOUND, "no file").into_response();
    };
    if !file.allowed {
        return (StatusCode::FORBIDDEN, "not accepted").into_response();
    }
    match tokio::fs::File::open(&file.path).await {
        Ok(reader) => {
            let mime = mime_guess::from_path(&file.filename).first_or_octet_stream();
            let mut headers = HeaderMap::new();
            if let Ok(value) = mime.essence_str().parse() {
                headers.insert(header::CONTENT_TYPE, value);
            }
            headers.insert(header::CONTENT_DISPOSITION, attachment_header(&file.filename));
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            if file.size > 0 {
                if let Ok(len) = HeaderValue::from_str(&file.size.to_string()) {
                    headers.insert(header::CONTENT_LENGTH, len);
                }
            }
            let body = Body::from_stream(ReaderStream::new(reader));
            (headers, body).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

fn attachment_header(filename: &str) -> HeaderValue {
    let ascii: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let ascii = if ascii.trim().is_empty() {
        "download".to_string()
    } else {
        ascii
    };
    format!("attachment; filename=\"{ascii}\"")
        .parse()
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

#[derive(Deserialize)]
struct WebOfferReq {
    target_id: String,
    filename: String,
    size: u64,
    from_name: Option<String>,
    from_id: Option<String>,
}

async fn web_offer(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(body): Json<WebOfferReq>,
) -> impl IntoResponse {
    let ip = client_ip(addr);
    let hint = body.from_name.unwrap_or_default();
    let kind = if hint.to_lowercase().contains("phone") {
        "phone"
    } else {
        "browser"
    };
    let from = if addr.ip().is_loopback() {
        if hint.trim().is_empty() {
            state.inner.name.read().await.clone()
        } else {
            hint
        }
    } else {
        guest_label(kind, &ip)
    };
    let filename = body.filename.trim();
    let filename = if filename.is_empty() {
        "file.bin".to_string()
    } else {
        filename.to_string()
    };
    if let Some(denied) = reject_blocked_file(&filename) {
        return denied;
    }
    let target_id = body.target_id.trim().to_string();
    let target = state
        .all_peers()
        .await
        .into_iter()
        .find(|peer| peer.id == target_id);
    if target_id.is_empty() || target.is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "ok": false, "error": "target device is no longer connected" })),
        )
            .into_response();
    }
    let target = target.expect("target checked above");
    let from_id = body.from_id.unwrap_or(ip);
    let id = Uuid::new_v4().to_string();
    state.inner.pending.write().await.insert(
        id.clone(),
        PendingOffer {
            filename: filename.clone(),
            size: body.size,
            from_name: from.clone(),
            from_id: from_id.clone(),
            target_id: target_id.clone(),
            decision: None,
            staged_path: None,
            accepted: false,
            remote_offer_id: None,
        },
    );
    tracing::info!("web-offer {filename} from {from} to {target_id} size={}", body.size);
    if target_id == state.inner.id || target.kind == "web" {
        state.emit(WsEvent::Incoming {
            id: id.clone(),
            filename,
            size: body.size,
            from,
            from_id: from_id.clone(),
            target_id,
        });
        let timeout_state = state.clone();
        let timeout_id = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(120)).await;
            let mut pending = timeout_state.inner.pending.write().await;
            let expired = pending
                .get(&timeout_id)
                .is_some_and(|offer| !offer.accepted);
            if expired {
                pending.remove(&timeout_id);
                drop(pending);
                timeout_state.emit(WsEvent::Declined { id: timeout_id });
            }
        });
    } else {
        let offer_state = state.clone();
        let offer_id = id.clone();
        tokio::spawn(async move {
            match transfer::request_acceptance(
                &target,
                &filename,
                body.size,
                &from,
                &from_id,
            )
            .await
            {
                Ok(remote_id) => {
                    let mut pending = offer_state.inner.pending.write().await;
                    if let Some(offer) = pending.get_mut(&offer_id) {
                        offer.accepted = true;
                        offer.remote_offer_id = Some(remote_id);
                        drop(pending);
                        offer_state.emit(WsEvent::Accepted { id: offer_id });
                    }
                }
                Err(err) => {
                    offer_state.inner.pending.write().await.remove(&offer_id);
                    tracing::warn!("remote offer failed: {err}");
                    offer_state.emit(WsEvent::Declined { id: offer_id });
                }
            }
        });
    }
    Json(serde_json::json!({ "ok": true, "id": id })).into_response()
}

async fn attach_web_offer(
    state: AppState,
    id: String,
    _filename: String,
    path: PathBuf,
) -> axum::response::Response {
    let mut pending_map = state.inner.pending.write().await;
    let Some(pending) = pending_map.get_mut(&id) else {
        drop(pending_map);
        let _ = tokio::fs::remove_file(path).await;
        return (StatusCode::GONE, "offer was declined or expired").into_response();
    };
    let target_id = pending.target_id.clone();
    let offered_filename = pending.filename.clone();
    let size = pending.size;
    let remote_offer_id = pending.remote_offer_id.clone();
    if !pending.accepted {
        drop(pending_map);
        let _ = tokio::fs::remove_file(path).await;
        return (StatusCode::FORBIDDEN, "offer has not been accepted").into_response();
    }
    pending_map.remove(&id);
    drop(pending_map);

    if target_id == state.inner.id {
        return commit_staged_file(state, id, offered_filename, path).await;
    }

    let peer = state
        .all_peers()
        .await
        .into_iter()
        .find(|peer| peer.id == target_id);
    let Some(peer) = peer else {
        let _ = tokio::fs::remove_file(path).await;
        return (StatusCode::NOT_FOUND, "target device disconnected").into_response();
    };
    if peer.kind == "web" {
        state.inner.outbound.write().await.insert(
            id.clone(),
            crate::state::OutboundFile {
                path,
                filename: offered_filename.clone(),
                size,
                allowed: true,
            },
        );
        state.emit(WsEvent::Ready {
            id: id.clone(),
            filename: offered_filename,
            size,
            target_id,
            download: format!("/api/files/{id}/download"),
        });
        return Json(serde_json::json!({ "ok": true, "id": id })).into_response();
    }

    let Some(remote_offer_id) = remote_offer_id else {
        let _ = tokio::fs::remove_file(path).await;
        return (StatusCode::CONFLICT, "remote acceptance is missing").into_response();
    };
    let send_state = state.clone();
    tokio::spawn(async move {
        if let Err(err) = transfer::send_to_lan_after_acceptance(
            send_state.clone(),
            peer,
            offered_filename,
            path.clone(),
            size,
            remote_offer_id,
            id.clone(),
        )
        .await
        {
            transfer::warn_send(&err);
            send_state.emit(WsEvent::Error {
                id: Some(id),
                message: err.to_string(),
            });
        }
        let _ = tokio::fs::remove_file(path).await;
    });
    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn send_local(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut peer_id = String::new();
    let mut filename = String::from("file.bin");
    let mut from_name = String::new();
    let mut from_id = String::new();
    let mut offer_id = String::new();
    let mut temp_path: Option<PathBuf> = None;

    while let Some(mut field) = match multipart.next_field().await {
        Ok(f) => f,
        Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
    } {
        let name = field.name().unwrap_or("").to_string();
        if name == "peer_id" {
            peer_id = field.text().await.unwrap_or_default();
        } else if name == "from_name" {
            from_name = field.text().await.unwrap_or_default();
        } else if name == "from_id" {
            from_id = field.text().await.unwrap_or_default();
        } else if name == "offer_id" {
            offer_id = field.text().await.unwrap_or_default();
        } else if name == "file" {
            if let Some(orig) = field.file_name() {
                filename = orig.to_string();
            }
            let tmp = state.save_dir().await.join(format!(
                ".send-{}-{}",
                Uuid::new_v4(),
                sanitize_filename::sanitize(&filename)
            ));
            let mut out = match tokio::fs::File::create(&tmp).await {
                Ok(f) => f,
                Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
            };
            while let Some(chunk) = field.next().await {
                match chunk {
                    Ok(bytes) => {
                        if let Err(err) = out.write_all(&bytes).await {
                            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
                        }
                    }
                    Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
                }
            }
            let _ = out.flush().await;
            temp_path = Some(tmp);
        }
    }

    let Some(path) = temp_path else {
        tracing::warn!("send_local: missing file field (peer_id={peer_id})");
        return (StatusCode::BAD_REQUEST, "file is required").into_response();
    };
    if let Some(denied) = reject_blocked_file(&filename) {
        let _ = tokio::fs::remove_file(&path).await;
        return denied;
    }
    if !offer_id.trim().is_empty() {
        return attach_web_offer(state, offer_id.trim().to_string(), filename, path).await;
    }
    if peer_id.is_empty() {
        peer_id = state.inner.id.clone();
    }
    let mut from = if from_name.trim().is_empty() {
        "Nearby device".to_string()
    } else {
        from_name
    };
    if !addr.ip().is_loopback() {
        from = guest_label(
            if from.to_lowercase().contains("phone") {
                "phone"
            } else {
                "browser"
            },
            &client_ip(addr),
        );
    }
    // Never attribute a guest's upload to this computer, or the desktop UI will
    // mistake the offer for its own outgoing send and hide the Accept banner.
    let sender_id = if !from_id.trim().is_empty() {
        from_id.trim().to_string()
    } else if addr.ip().is_loopback() {
        state.inner.id.clone()
    } else {
        client_ip(addr)
    };
    tracing::info!("send_local filename={filename} peer_id={peer_id} ip={}", client_ip(addr));

    let peer = state
        .all_peers()
        .await
        .into_iter()
        .find(|p| p.id == peer_id);

    let Some(peer) = peer else {
        tracing::warn!("unknown peer {peer_id}; offering to this computer");
        return ingest_to_this_device(state, filename, path, from, sender_id).await;
    };

    if peer.kind == "self" || peer.id == state.inner.id {
        return ingest_to_this_device(state, filename, path, from, sender_id).await;
    }

    let send_state = state.clone();
    let send_name = filename.clone();
    let cleanup_lan = peer.kind != "web";
    tokio::spawn(async move {
        let result = transfer::send_file(send_state.clone(), peer, send_name, path.clone()).await;
        if let Err(err) = result {
            transfer::warn_send(&err);
            send_state.emit(WsEvent::Error {
                id: None,
                message: err.to_string(),
            });
        }
        if cleanup_lan {
            let _ = tokio::fs::remove_file(path).await;
        }
    });

    Json(serde_json::json!({ "ok": true, "filename": filename })).into_response()
}

async fn ingest_to_this_device(
    state: AppState,
    filename: String,
    path: PathBuf,
    from_name: String,
    from_id: String,
) -> axum::response::Response {
    if let Some(denied) = reject_blocked_file(&filename) {
        let _ = tokio::fs::remove_file(&path).await;
        return denied;
    }
    let size = tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);
    let id = Uuid::new_v4().to_string();
    state.inner.pending.write().await.insert(
        id.clone(),
        PendingOffer {
            filename: filename.clone(),
            size,
            from_name: from_name.clone(),
            from_id: from_id.clone(),
            target_id: state.inner.id.clone(),
            decision: None,
            staged_path: Some(path),
            accepted: false,
            remote_offer_id: None,
        },
    );
    state.emit(WsEvent::Incoming {
        id: id.clone(),
        filename,
        size,
        from: from_name,
        from_id,
        target_id: state.inner.id.clone(),
    });
    Json(serde_json::json!({ "ok": true, "id": id })).into_response()
}

async fn commit_staged_file(
    state: AppState,
    id: String,
    filename: String,
    path: PathBuf,
) -> axum::response::Response {
    let dest = crate::state::unique_path(&state.save_dir().await, &filename);
    if let Err(err) = tokio::fs::rename(&path, &dest).await {
        if tokio::fs::copy(&path, &dest).await.is_err() {
            let _ = tokio::fs::remove_file(&path).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
        }
        let _ = tokio::fs::remove_file(&path).await;
    }
    state.emit(WsEvent::Complete {
        id,
        path: Some(dest.display().to_string()),
        filename,
    });
    Json(serde_json::json!({ "ok": true, "saved_path": dest })).into_response()
}
