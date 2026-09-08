use crate::state::{blocked_executable, unique_path, AppState, OutboundFile, Peer, WsEvent};
use bytes::Bytes;
use futures::StreamExt;
use reqwest::Body;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::{info, warn};

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(serde::Deserialize)]
struct OfferResponse {
    id: String,
    accepted: bool,
}

pub async fn send_file(
    state: AppState,
    peer: Peer,
    filename: String,
    path: PathBuf,
) -> Result<(), TransferError> {
    if blocked_executable(&filename) {
        return Err(TransferError::Message("executable files are not allowed".into()));
    }
    let size = tokio::fs::metadata(&path).await?.len();
    if peer.kind == "web" {
        return send_to_web(state, peer, filename, path, size).await;
    }
    send_to_lan(state, peer, filename, path, size).await
}

async fn send_to_web(
    state: AppState,
    peer: Peer,
    filename: String,
    path: PathBuf,
    size: u64,
) -> Result<(), TransferError> {
    let id = uuid::Uuid::new_v4().to_string();
    state.inner.outbound.write().await.insert(
        id.clone(),
        OutboundFile {
            path,
            filename: filename.clone(),
            size,
            allowed: false,
        },
    );
    let from = state.inner.name.read().await.clone();
    state.emit(WsEvent::Incoming {
        id: id.clone(),
        filename,
        size,
        from,
        target_id: peer.id,
    });
    Ok(())
}

async fn send_to_lan(
    state: AppState,
    peer: Peer,
    filename: String,
    path: PathBuf,
    size: u64,
) -> Result<(), TransferError> {
    let from_name = state.inner.name.read().await.clone();
    let remote_id = request_acceptance(
        &peer,
        &filename,
        size,
        &from_name,
        &state.inner.id,
    )
    .await?;
    send_to_lan_after_acceptance(state, peer, filename, path, size, remote_id.clone(), remote_id)
        .await
}

pub async fn request_acceptance(
    peer: &Peer,
    filename: &str,
    size: u64,
    from_name: &str,
    from_id: &str,
) -> Result<String, TransferError> {
    let base = format!("http://{}:{}", peer.host, peer.port);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60 * 30))
        .build()?;

    let offer = client
        .post(format!("{base}/api/files/offer"))
        .json(&serde_json::json!({
            "filename": filename,
            "size": size,
            "from_name": from_name,
            "from_id": from_id,
        }))
        .timeout(Duration::from_secs(130))
        .send()
        .await?;

    if offer.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(TransferError::Message("Receiver declined".into()));
    }
    if !offer.status().is_success() {
        let status = offer.status();
        let body = offer.text().await.unwrap_or_default();
        return Err(TransferError::Message(format!(
            "offer failed: {status} {body}"
        )));
    }
    let OfferResponse { id, accepted } = offer.json().await?;
    if !accepted {
        return Err(TransferError::Message("Receiver declined".into()));
    }
    Ok(id)
}

pub async fn send_to_lan_after_acceptance(
    state: AppState,
    peer: Peer,
    filename: String,
    path: PathBuf,
    size: u64,
    remote_id: String,
    event_id: String,
) -> Result<(), TransferError> {
    let base = format!("http://{}:{}", peer.host, peer.port);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60 * 30))
        .build()?;
    let file = tokio::fs::File::open(&path).await?;
    let done = Arc::new(AtomicU64::new(0));
    let events = state.inner.events.clone();
    let progress_id = event_id.clone();
    let progress_name = filename.clone();
    let stream = ReaderStream::new(file).map(move |chunk| {
        if let Ok(ref bytes) = chunk {
            let n = done.fetch_add(bytes.len() as u64, Ordering::Relaxed) + bytes.len() as u64;
            let _ = events.send(WsEvent::Progress {
                id: progress_id.clone(),
                done: n,
                total: size,
                direction: "send".into(),
                filename: progress_name.clone(),
            });
        }
        chunk
    });

    let response = client
        .post(format!("{base}/api/files/{remote_id}/data"))
        .header("X-Filename", filename.clone())
        .body(Body::wrap_stream(stream))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(TransferError::Message(format!(
            "upload failed: {status} {body}"
        )));
    }

    info!("sent {filename} to {}", peer.name);
    state.emit(WsEvent::Complete {
        id: event_id,
        path: Some(path.display().to_string()),
        filename,
    });
    Ok(())
}

pub async fn write_incoming_body<S, E>(
    state: &AppState,
    id: &str,
    filename: &str,
    size: u64,
    mut body: S,
) -> Result<PathBuf, TransferError>
where
    S: futures::Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    if blocked_executable(filename) {
        return Err(TransferError::Message("executable files are not allowed".into()));
    }
    let path = unique_path(&state.save_dir().await, filename);
    let tmp = path.with_extension(format!(
        "{}part",
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| format!("{e}."))
            .unwrap_or_default()
    ));
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut done = 0u64;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| TransferError::Message(e.to_string()))?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        state.emit(WsEvent::Progress {
            id: id.to_string(),
            done,
            total: if size == 0 { done } else { size },
            direction: "receive".into(),
            filename: filename.to_string(),
        });
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&tmp, &path).await?;
    state.emit(WsEvent::Complete {
        id: id.to_string(),
        path: Some(path.display().to_string()),
        filename: filename.to_string(),
    });
    Ok(path)
}

pub fn warn_send(err: &TransferError) {
    warn!("send failed: {err}");
}
