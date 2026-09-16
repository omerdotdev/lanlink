use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, RwLock};
use uuid::Uuid;

pub const HTTP_PORT: u16 = 7420;
pub const SERVICE_TYPE: &str = "_lanlink._tcp.local.";
pub const MAX_CLIPS: usize = 50;
pub const MAX_CLIP_CHARS: usize = 8000;
pub const CLIP_WINDOW_MS: u64 = 30_000;
pub const CLIP_MAX_PER_WINDOW: usize = 8;

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

pub struct Inner {
    pub id: String,
    pub port: u16,
    pub name: RwLock<String>,
    pub peers: RwLock<HashMap<String, Peer>>,
    pub web_clients: RwLock<HashMap<String, Peer>>,
    pub pending: RwLock<HashMap<String, PendingOffer>>,
    pub outbound: RwLock<HashMap<String, OutboundFile>>,
    pub clips: RwLock<VecDeque<ClipNote>>,
    pub clip_times: RwLock<HashMap<String, VecDeque<u64>>>,
    pub events: broadcast::Sender<WsEvent>,
    pub save_dir: RwLock<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipNote {
    pub id: String,
    pub text: String,
    pub from: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub kind: String,
}

#[allow(dead_code)]
pub struct PendingOffer {
    pub filename: String,
    pub size: u64,
    pub from_name: String,
    pub from_id: String,
    pub target_id: String,
    pub decision: Option<oneshot::Sender<bool>>,
    pub staged_path: Option<PathBuf>,
    pub accepted: bool,
    pub remote_offer_id: Option<String>,
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct OutboundFile {
    pub path: PathBuf,
    pub filename: String,
    pub size: u64,
    pub allowed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    Peers {
        peers: Vec<Peer>,
    },
    Incoming {
        id: String,
        filename: String,
        size: u64,
        from: String,
        from_id: String,
        target_id: String,
    },
    Accepted {
        id: String,
    },
    Ready {
        id: String,
        filename: String,
        size: u64,
        target_id: String,
        download: String,
    },
    Progress {
        id: String,
        done: u64,
        total: u64,
        direction: String,
        filename: String,
    },
    Complete {
        id: String,
        path: Option<String>,
        filename: String,
    },
    Declined {
        id: String,
    },
    Error {
        id: Option<String>,
        message: String,
    },
    Clip {
        id: String,
        text: String,
        from: String,
        created_at: u64,
    },
    ClipsCleared {},
}

impl AppState {
    pub fn new(port: u16) -> std::io::Result<Self> {
        let save_dir = user_facing_path(load_save_dir()?);
        std::fs::create_dir_all(&save_dir)?;

        let (events, _) = broadcast::channel(256);
        Ok(Self {
            inner: Arc::new(Inner {
                id: Uuid::new_v4().to_string(),
                port,
                name: RwLock::new(default_name()),
                peers: RwLock::new(HashMap::new()),
                web_clients: RwLock::new(HashMap::new()),
                pending: RwLock::new(HashMap::new()),
                outbound: RwLock::new(HashMap::new()),
                clips: RwLock::new(VecDeque::new()),
                clip_times: RwLock::new(HashMap::new()),
                events,
                save_dir: RwLock::new(save_dir),
            }),
        })
    }

    pub async fn save_dir(&self) -> PathBuf {
        self.inner.save_dir.read().await.clone()
    }

    pub async fn set_save_dir(&self, path: PathBuf) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&path)?;
        let canonical = user_facing_path(std::fs::canonicalize(&path).unwrap_or(path));
        persist_save_dir(&canonical)?;
        *self.inner.save_dir.write().await = canonical.clone();
        Ok(canonical)
    }

    pub fn emit(&self, event: WsEvent) {
        let _ = self.inner.events.send(event);
    }

    pub async fn all_peers(&self) -> Vec<Peer> {
        let mut list: Vec<Peer> = self.inner.peers.read().await.values().cloned().collect();
        list.extend(self.inner.web_clients.read().await.values().cloned());
        let name = self.inner.name.read().await.clone();
        list.push(Peer {
            id: self.inner.id.clone(),
            name,
            host: lan_ip(),
            port: self.inner.port,
            kind: "self".into(),
        });
        list.sort_by_key(|a| a.name.to_lowercase());
        list
    }

    pub async fn broadcast_peers(&self) {
        let peers = self.all_peers().await;
        self.emit(WsEvent::Peers { peers });
    }

    pub async fn clips(&self) -> Vec<ClipNote> {
        self.inner.clips.read().await.iter().cloned().collect()
    }

    pub async fn push_clip(&self, from: String, text: String) -> ClipNote {
        let note = ClipNote {
            id: Uuid::new_v4().to_string(),
            text,
            from,
            created_at: now_ms(),
        };
        {
            let mut clips = self.inner.clips.write().await;
            clips.push_front(note.clone());
            while clips.len() > MAX_CLIPS {
                clips.pop_back();
            }
        }
        self.emit(WsEvent::Clip {
            id: note.id.clone(),
            text: note.text.clone(),
            from: note.from.clone(),
            created_at: note.created_at,
        });
        note
    }

    pub async fn clear_clips(&self) {
        self.inner.clips.write().await.clear();
        self.emit(WsEvent::ClipsCleared {});
    }

    pub async fn allow_clip(&self, ip: &str) -> bool {
        let now = now_ms();
        let mut map = self.inner.clip_times.write().await;
        let queue = map.entry(ip.to_string()).or_default();
        while queue
            .front()
            .is_some_and(|stamp| now.saturating_sub(*stamp) > CLIP_WINDOW_MS)
        {
            queue.pop_front();
        }
        if queue.len() >= CLIP_MAX_PER_WINDOW {
            return false;
        }
        queue.push_back(now);
        true
    }

    pub fn join_url(&self) -> String {
        format!("http://{}:{}", lan_ip(), self.inner.port)
    }
}

#[derive(Serialize, Deserialize, Default)]
struct StoredConfig {
    save_dir: Option<String>,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lanlink")
        .join("config.json")
}

fn default_save_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Lanlink")
}

fn user_facing_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let value = path.to_string_lossy();
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

fn load_save_dir() -> std::io::Result<PathBuf> {
    let path = config_path();
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<StoredConfig>(&raw) {
            if let Some(dir) = cfg.save_dir.filter(|s| !s.trim().is_empty()) {
                return Ok(PathBuf::from(dir));
            }
        }
    }
    Ok(default_save_dir())
}

fn persist_save_dir(dir: &std::path::Path) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let cfg = StoredConfig {
        save_dir: Some(dir.display().to_string()),
    };
    std::fs::write(path, serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".into()))
}

pub fn default_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Lanlink device".into())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn lan_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

pub async fn public_network_warning() -> bool {
    tokio::task::spawn_blocking(detect_public_network)
        .await
        .unwrap_or(false)
}

fn detect_public_network() -> bool {
    #[cfg(windows)]
    {
        windows_lan_profile_is_public()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Match Settings → Network profile type for the adapter that owns the join IP.
/// `netsh advfirewall show currentprofile` is the wrong signal: Windows prints
/// every firewall profile that is ON (often including Public on vEthernet).
#[cfg(windows)]
fn windows_lan_profile_is_public() -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let ip = lan_ip();
    if !ip.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ':') {
        return false;
    }
    let script = format!(
        "$ip = '{ip}'; $cat = $null; $idx = @(Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object {{ $_.IPAddress -eq $ip }} | Select-Object -ExpandProperty InterfaceIndex -First 1); if ($idx) {{ $cat = (Get-NetConnectionProfile -InterfaceIndex $idx -ErrorAction SilentlyContinue).NetworkCategory }}; if (-not $cat) {{ $cats = @(Get-NetConnectionProfile -ErrorAction SilentlyContinue | Where-Object {{ $_.IPv4Connectivity -ne 'Disconnected' }} | ForEach-Object {{ $_.NetworkCategory.ToString() }}); if ($cats -contains 'Private' -or $cats -contains 'DomainAuthenticated') {{ $cat = 'Private' }} elseif ($cats -contains 'Public') {{ $cat = 'Public' }} else {{ $cat = 'Private' }} }}; Write-Output $cat"
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .eq_ignore_ascii_case("Public")
        }
        _ => false,
    }
}

pub fn blocked_executable(filename: &str) -> bool {
    const BLOCKED: &[&str] = &[
        "exe", "bat", "cmd", "com", "cpl", "dll", "scr", "pif", "msi", "msix", "msp", "mst",
        "appx", "msixbundle", "js", "jse", "vbs", "vbe", "wsf", "wsh", "ws", "ps1", "psd1",
        "psm1", "reg", "inf", "ins", "isp", "job", "lnk", "scf", "msc", "hta", "jar", "apk",
        "app", "command", "dmg", "pkg", "deb", "rpm", "run", "bin", "elf", "so", "dylib",
        "appimage", "action", "workflow", "scpt", "sh", "bash", "zsh", "gadget", "application",
        "sys", "drv", "ocx",
    ];
    filename
        .split('.')
        .skip(1)
        .any(|part| BLOCKED.contains(&part.to_ascii_lowercase().as_str()))
}

pub fn unique_path(dir: &std::path::Path, filename: &str) -> PathBuf {
    let safe = sanitize_filename::sanitize(filename);
    let safe = if safe.is_empty() {
        "file.bin".to_string()
    } else {
        safe
    };
    let candidate = dir.join(&safe);
    if !candidate.exists() {
        return candidate;
    }
    let stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = candidate
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for i in 1..1000 {
        let next = dir.join(format!("{stem}-{i}{ext}"));
        if !next.exists() {
            return next;
        }
    }
    dir.join(format!(
        "{stem}-{}{ext}",
        chrono::Local::now().format("%Y%m%d%H%M%S")
    ))
}
