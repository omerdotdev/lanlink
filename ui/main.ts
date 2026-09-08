type Peer = {
  id: string;
  name: string;
  host: string;
  port: number;
  kind: string;
};

type Hello = {
  type: "hello";
  id: string;
  web_id: string | null;
  name: string;
  url: string;
  save_dir: string;
  peers: Peer[];
  clips?: ClipNote[];
  public_network?: boolean;
};

type ClipNote = {
  id: string;
  text: string;
  from: string;
  created_at: number;
};

type Incoming = {
  type: "incoming";
  id: string;
  filename: string;
  size: number;
  from: string;
  target_id: string;
};

type EventMsg =
  | Hello
  | Incoming
  | { type: "peers"; peers: Peer[] }
  | { type: "accepted"; id: string }
  | { type: "ready"; id: string; filename: string; size: number; target_id: string; download: string }
  | { type: "progress"; id: string; done: number; total: number; direction: string; filename: string }
  | { type: "complete"; id: string; path?: string | null; filename: string }
  | { type: "declined"; id: string }
  | { type: "error"; id?: string | null; message: string }
  | { type: "clip"; id: string; text: string; from: string; created_at: number }
  | { type: "clips_cleared" };

const peersEl = document.querySelector("#peers") as HTMLUListElement;
const emptyPeers = document.querySelector("#empty-peers") as HTMLParagraphElement;
const joinUrl = document.querySelector("#join-url") as HTMLParagraphElement;
const saveDir = document.querySelector("#save-dir") as HTMLParagraphElement;
const qr = document.querySelector("#qr") as HTMLImageElement;
const nameInput = document.querySelector("#device-name") as HTMLInputElement;
const nameForm = document.querySelector("#name-form") as HTMLFormElement;
const fileInput = document.querySelector("#file") as HTMLInputElement;
const fileHelp = document.querySelector("#file-help") as HTMLParagraphElement;
const fileLabel = document.querySelector("#file-label") as HTMLLabelElement;
const chosen = document.querySelector("#chosen") as HTMLParagraphElement;
const drop = document.querySelector("#drop") as HTMLElement;
const incomingWrap = document.querySelector("#incoming-wrap") as HTMLElement;
const incomingText = document.querySelector("#incoming-text") as HTMLParagraphElement;
const acceptBtn = document.querySelector("#accept") as HTMLButtonElement;
const declineBtn = document.querySelector("#decline") as HTMLButtonElement;
const transfersEl = document.querySelector("#transfers") as HTMLUListElement;
const emptyTransfers = document.querySelector("#empty-transfers") as HTMLParagraphElement;
const settingsRoot = document.querySelector("#settings-root") as HTMLElement;
const openSettings = document.querySelector("#open-settings") as HTMLButtonElement;
const closeSettings = document.querySelector("#close-settings") as HTMLButtonElement;
const settingsBackdrop = document.querySelector("#settings-backdrop") as HTMLElement;
const themeQuick = document.querySelector("#theme-quick") as HTMLButtonElement;
const copyUrl = document.querySelector("#copy-url") as HTMLButtonElement;
const xferPop = document.querySelector("#xfer-pop") as HTMLElement;
const xferList = document.querySelector("#xfer-list") as HTMLElement;
const saveDirInput = document.querySelector("#save-dir-input") as HTMLInputElement;
const saveDirForm = document.querySelector("#save-dir-form") as HTMLFormElement;
const saveDirStatus = document.querySelector("#save-dir-status") as HTMLParagraphElement;
const browseSaveDir = document.querySelector("#browse-save-dir") as HTMLButtonElement;
const openDisclaimer = document.querySelector("#open-disclaimer") as HTMLButtonElement;
const disclaimerRoot = document.querySelector("#disclaimer-root") as HTMLElement;
const closeDisclaimer = document.querySelector("#close-disclaimer") as HTMLButtonElement;
const disclaimerBackdrop = document.querySelector("#disclaimer-backdrop") as HTMLElement;
const sendHeading = document.querySelector("#send-heading") as HTMLHeadingElement;
const sendLede = document.querySelector("#send-lede") as HTMLParagraphElement;
const nearbyTitle = document.querySelector("#nearby-title") as HTMLHeadingElement;
const sendSelectedBtn = document.querySelector("#send-selected") as HTMLButtonElement;
const selectedDeviceCount = document.querySelector("#selected-device-count") as HTMLSpanElement;
const viewThumbnails = document.querySelector("#view-thumbnails") as HTMLButtonElement;
const viewList = document.querySelector("#view-list") as HTMLButtonElement;
const clearTransfers = document.querySelector("#clear-transfers") as HTMLButtonElement;
const clipText = document.querySelector("#clip-text") as HTMLTextAreaElement;
const shareClip = document.querySelector("#share-clip") as HTMLButtonElement;
const clearClipsBtn = document.querySelector("#clear-clips") as HTMLButtonElement;
const clipsEl = document.querySelector("#clips") as HTMLUListElement;
const emptyClips = document.querySelector("#empty-clips") as HTMLParagraphElement;
const clipStatus = document.querySelector("#clip-status") as HTMLParagraphElement;
const networkWarn = document.querySelector("#network-warn") as HTMLElement;
const isPhone = /Mobi|Android|iPhone|iPad/i.test(navigator.userAgent);

let selfId = "";
let hostId = "";
let peers: Peer[] = [];
let selectedFile: File | null = null;
let pendingIncoming: Incoming | null = null;
let socket: WebSocket | null = null;
let chooseThenSend = false;
let sendingBatch = false;
const selectedPeerIds = new Set<string>();
const decidedIncomingIds = new Set<string>();
let clips: ClipNote[] = [];
const isHost = ["localhost", "127.0.0.1", "[::1]"].includes(location.hostname);
document.body.classList.toggle("is-guest", !isHost);

type Cue = "send" | "receive" | "note" | "clear" | "copy";
let audioCtx: AudioContext | null = null;

function unlockAudio() {
  const Ctor =
    window.AudioContext ||
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctor) return;
  if (!audioCtx) audioCtx = new Ctor();
  if (audioCtx.state === "suspended") void audioCtx.resume();
}

function beep(freq: number, when: number, dur: number) {
  if (!audioCtx) return;
  const osc = audioCtx.createOscillator();
  const gain = audioCtx.createGain();
  osc.type = "sine";
  osc.frequency.setValueAtTime(freq, when);
  gain.gain.setValueAtTime(0.0001, when);
  gain.gain.exponentialRampToValueAtTime(0.07, when + 0.012);
  gain.gain.exponentialRampToValueAtTime(0.0001, when + dur);
  osc.connect(gain);
  gain.connect(audioCtx.destination);
  osc.start(when);
  osc.stop(when + dur + 0.02);
}

function playCue(kind: Cue) {
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  unlockAudio();
  if (!audioCtx) return;
  const t = audioCtx.currentTime;
  const notes: Record<Cue, number[][]> = {
    send: [
      [880, 0, 0.07],
      [1175, 0.07, 0.08],
    ],
    receive: [
      [523, 0, 0.08],
      [784, 0.08, 0.1],
    ],
    note: [[1047, 0, 0.055]],
    clear: [
      [392, 0, 0.05],
      [262, 0.05, 0.09],
    ],
    copy: [[1319, 0, 0.05]],
  };
  for (const [freq, delay, dur] of notes[kind]) beep(freq, t + delay, dur);
}

document.addEventListener("pointerdown", unlockAudio, { passive: true });

type ThemePref = "light" | "dark";
type SkinPref = "aero" | "liquid" | "cyber" | "astral" | "space";

function themePref(): ThemePref {
  return localStorage.getItem("lanlink-theme") === "dark" ? "dark" : "light";
}

function skinPref(): SkinPref {
  const stored = localStorage.getItem("lanlink-skin");
  if (stored === "aero" || stored === "liquid" || stored === "cyber" || stored === "astral" || stored === "space") {
    return stored;
  }
  return "astral";
}

function applyTheme(pref: ThemePref) {
  localStorage.setItem("lanlink-theme", pref);
  document.documentElement.dataset.theme = pref;
  for (const input of document.querySelectorAll<HTMLInputElement>("input[name=theme]")) {
    input.checked = input.value === pref;
  }
}

function applySkin(pref: SkinPref) {
  localStorage.setItem("lanlink-skin", pref);
  document.documentElement.dataset.skin = pref;
  for (const input of document.querySelectorAll<HTMLInputElement>("input[name=skin]")) {
    input.checked = input.value === pref;
  }
}

function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

function localTransferId(): string {
  if (typeof crypto.randomUUID === "function") {
    return `send-${crypto.randomUUID()}`;
  }
  const random = Math.random().toString(36).slice(2);
  return `send-${Date.now().toString(36)}-${random}`;
}

type Xfer = {
  filename: string;
  direction: string;
  done: number;
  total: number;
  state: "active" | "done" | "error";
};
const xfers = new Map<string, Xfer>();
const pendingUploads = new Map<string, { file: File; filename: string }>();

type ActivityStatus = "sending" | "receiving" | "waiting" | "done" | "declined" | "error" | "interrupted";
type ActivityRecord = {
  id: string;
  filename: string;
  direction: "send" | "receive";
  status: ActivityStatus;
  peer: string;
  size: number;
  updatedAt: number;
};
type TransferView = "thumbnails" | "list";

const HISTORY_KEY = "lanlink-transfer-history-v1";
const VIEW_KEY = "lanlink-transfer-view";
let transferView: TransferView =
  localStorage.getItem(VIEW_KEY) === "list" ? "list" : "thumbnails";
let activity = loadActivity();

function loadActivity(): ActivityRecord[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed.slice(0, 100).map((item) => ({
      ...item,
      status:
        item.status === "sending" || item.status === "receiving" || item.status === "waiting"
          ? "interrupted"
          : item.status,
    }));
  } catch {
    return [];
  }
}

function saveActivity() {
  localStorage.setItem(HISTORY_KEY, JSON.stringify(activity.slice(0, 100)));
}

function fileKind(filename: string): string {
  const extension = filename.split(".").pop();
  if (!extension || extension === filename) return "FILE";
  return extension.slice(0, 4).toUpperCase();
}

function statusLabel(status: ActivityStatus): string {
  switch (status) {
    case "sending":
      return "Sending";
    case "receiving":
      return "Receiving";
    case "waiting":
      return "Waiting for acceptance";
    case "done":
      return "Completed";
    case "declined":
      return "Declined";
    case "interrupted":
      return "Interrupted";
    default:
      return "Failed";
  }
}

function renderActivity() {
  transfersEl.replaceChildren();
  transfersEl.className = `transfers ${transferView}-view`;
  emptyTransfers.hidden = activity.length > 0;
  clearTransfers.disabled = activity.length === 0;
  viewThumbnails.setAttribute("aria-pressed", String(transferView === "thumbnails"));
  viewList.setAttribute("aria-pressed", String(transferView === "list"));

  for (const record of activity) {
    const li = document.createElement("li");
    li.dataset.tid = record.id;
    li.className = `transfer-record status-${record.status}`;

    const thumbnail = document.createElement("div");
    thumbnail.className = "file-thumbnail";
    thumbnail.textContent = fileKind(record.filename);

    const details = document.createElement("div");
    details.className = "transfer-details";
    const name = document.createElement("strong");
    name.className = "transfer-filename";
    name.textContent = record.filename;
    const meta = document.createElement("span");
    meta.className = "transfer-meta";
    const peer = record.peer ? ` · ${record.peer}` : "";
    const size = record.size ? ` · ${fmtSize(record.size)}` : "";
    meta.textContent = `${statusLabel(record.status)}${peer}${size}`;
    const time = document.createElement("time");
    time.dateTime = new Date(record.updatedAt).toISOString();
    time.textContent = new Date(record.updatedAt).toLocaleString([], {
      dateStyle: "medium",
      timeStyle: "short",
    });
    details.append(name, meta, time);
    li.append(thumbnail, details);
    transfersEl.append(li);
  }
}

function updateActivity(
  id: string,
  patch: Partial<Omit<ActivityRecord, "id" | "updatedAt">>,
) {
  const current = activity.find((record) => record.id === id);
  const xfer = xfers.get(id);
  const record: ActivityRecord = {
    id,
    filename: patch.filename ?? current?.filename ?? xfer?.filename ?? "Transfer",
    direction: patch.direction ?? current?.direction ?? (xfer?.direction === "send" ? "send" : "receive"),
    status: patch.status ?? current?.status ?? "waiting",
    peer: patch.peer ?? current?.peer ?? "",
    size: patch.size ?? current?.size ?? xfer?.total ?? 0,
    updatedAt: Date.now(),
  };
  activity = [record, ...activity.filter((item) => item.id !== id)].slice(0, 100);
  saveActivity();
  renderActivity();
}

function adoptTransferId(localId: string, serverId: string) {
  if (localId === serverId) return;
  const record = activity.find((item) => item.id === localId);
  if (record) {
    activity = [
      { ...record, id: serverId },
      ...activity.filter((item) => item.id !== localId && item.id !== serverId),
    ].slice(0, 100);
    saveActivity();
    renderActivity();
  }
  const xfer = xfers.get(localId);
  if (xfer) {
    xfers.delete(localId);
    xfers.set(serverId, xfer);
    renderXfers();
  }
}

function setXfer(id: string, patch: Partial<Xfer> & Pick<Xfer, "filename" | "direction">) {
  const cur = xfers.get(id);
  xfers.set(id, {
    filename: patch.filename,
    direction: patch.direction,
    done: patch.done ?? cur?.done ?? 0,
    total: patch.total ?? cur?.total ?? 0,
    state: patch.state ?? cur?.state ?? "active",
  });
  renderXfers();
}

function finishXfer(id: string, state: "done" | "error") {
  const cur = xfers.get(id);
  if (!cur) return;
  const alreadyDone = cur.state === "done";
  const total = Math.max(cur.total, cur.done, 1);
  xfers.set(id, {
    ...cur,
    state,
    done: state === "done" ? total : cur.done,
    total,
  });
  if (state === "done" && !alreadyDone) {
    playCue(cur.direction === "send" ? "send" : "receive");
  }
  renderXfers();
  window.setTimeout(() => {
    xfers.delete(id);
    renderXfers();
  }, 4000);
}

function renderXfers() {
  xferPop.hidden = xfers.size === 0;
  xferList.replaceChildren();
  for (const x of xfers.values()) {
    const pct = x.total > 0 ? Math.min(100, Math.round((x.done / x.total) * 100)) : x.state === "done" ? 100 : 0;
    const item = document.createElement("div");
    item.className = `xfer-item ${x.state}`;
    const label = x.direction === "send" ? "Sending" : "Receiving";
    item.innerHTML = `<div class="xfer-meta"><strong></strong><span></span></div><div class="xfer-bar"><span></span></div>`;
    (item.querySelector("strong") as HTMLElement).textContent = x.filename;
    (item.querySelector("span") as HTMLElement).textContent = `${label} · ${pct}% · ${fmtSize(x.done)}`;
    (item.querySelector(".xfer-bar > span") as HTMLElement).style.width = `${pct}%`;
    xferList.append(item);
  }
}

function showSaveDir(path: string) {
  saveDirInput.value = path;
  saveDir.textContent = `Files you accept on this computer are stored in ${path}.`;
}

function showNetworkWarn(publicNetwork: boolean | undefined) {
  networkWarn.hidden = !(isHost && publicNetwork);
}

function applyGuestCopy() {
  if (isHost) return;
  sendHeading.textContent = "Choose a file, then a device";
  sendLede.textContent =
    "Pick a file here, then tap Send on any computer or browser in the list. That device will get Accept / Decline.";
  nearbyTitle.textContent = "Nearby devices";
  if (fileHelp) fileHelp.textContent = "Drop a file here, or browse. Then send it to a device on the right. Programs and scripts are blocked.";
  if (fileLabel) fileLabel.textContent = "Choose file";
}

function peerKindLabel(peer: Peer): string {
  if (peer.kind === "self" || peer.id === hostId) return `Computer · ${peer.host}`;
  if (peer.kind === "web") return `Browser · ${peer.host}`;
  return `${peer.host}:${peer.port}`;
}

function peerName(peerId: string): string {
  return peers.find((peer) => peer.id === peerId)?.name ?? "Unknown device";
}

function updateSendControls() {
  const count = selectedPeerIds.size;
  selectedDeviceCount.textContent =
    count === 0 ? "Select one or more devices" : `${count} device${count === 1 ? "" : "s"} selected`;
  sendSelectedBtn.textContent = selectedFile
    ? count > 1
      ? `Send to ${count} devices`
      : "Send"
    : "Choose file";
  sendSelectedBtn.disabled = count === 0 || sendingBatch;
}

function appendLinkedText(target: HTMLElement, value: string) {
  const pattern = /\bhttps?:\/\/[^\s]+/gi;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(value))) {
    if (match.index > last) {
      target.append(value.slice(last, match.index));
    }
    let href = match[0];
    const trailing = href.match(/[),.;!?]+$/);
    if (trailing) {
      href = href.slice(0, -trailing[0].length);
    }
    const link = document.createElement("a");
    link.href = href;
    link.textContent = href;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    target.append(link);
    if (trailing) target.append(trailing[0]);
    last = match.index + match[0].length;
  }
  if (last < value.length) target.append(value.slice(last));
}

function renderClips() {
  clipsEl.replaceChildren();
  emptyClips.hidden = clips.length > 0;
  for (const note of clips) {
    const li = document.createElement("li");
    li.className = "clip-item";
    const body = document.createElement("div");
    body.className = "clip-body";
    const text = document.createElement("p");
    text.className = "clip-text";
    appendLinkedText(text, note.text);
    const meta = document.createElement("span");
    meta.className = "clip-meta";
    const when = new Date(note.created_at).toLocaleString(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    });
    meta.textContent = `${note.from} · ${when}`;
    body.append(text, meta);
    const copyBtn = document.createElement("button");
    copyBtn.type = "button";
    copyBtn.className = "clip-copy";
    copyBtn.setAttribute("aria-label", "Copy note");
    copyBtn.innerHTML =
      '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="5.5" y="5.5" width="8" height="8" rx="1.2"></rect><path d="M10.5 5.5V4.2A1.2 1.2 0 0 0 9.3 3H4.2A1.2 1.2 0 0 0 3 4.2v5.1A1.2 1.2 0 0 0 4.2 10.5H5.5"></path></svg>';
    copyBtn.addEventListener("click", async () => {
      const ok = await copyText(note.text);
      copyBtn.setAttribute("aria-label", ok ? "Copied" : "Copy failed");
      copyBtn.classList.toggle("copied", ok);
      if (ok) playCue("copy");
      window.setTimeout(() => {
        copyBtn.setAttribute("aria-label", "Copy note");
        copyBtn.classList.remove("copied");
      }, 1400);
    });
    li.append(body, copyBtn);
    clipsEl.append(li);
  }
}

function addClip(note: ClipNote): boolean {
  if (clips.some((item) => item.id === note.id)) return false;
  clips = [note, ...clips].slice(0, 50);
  renderClips();
  return true;
}

function replaceClips(notes: ClipNote[]) {
  clips = notes.slice(0, 50);
  renderClips();
}

async function clearBoardNotes() {
  if (!window.confirm("Clear the shared board for everyone on this LAN?")) return;
  clearClipsBtn.disabled = true;
  clipStatus.textContent = "";
  try {
    const res = await fetch("/api/clips", { method: "DELETE" });
    if (!res.ok) {
      clipStatus.textContent = await res.text();
      return;
    }
    replaceClips([]);
    clipStatus.textContent = "Board cleared.";
    playCue("clear");
    window.setTimeout(() => {
      if (clipStatus.textContent === "Board cleared.") clipStatus.textContent = "";
    }, 1600);
  } catch {
    clipStatus.textContent = "Could not clear.";
  } finally {
    clearClipsBtn.disabled = false;
  }
}

async function copyText(text: string): Promise<boolean> {
  if (!text) return false;
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const helper = document.createElement("textarea");
    helper.value = text;
    helper.setAttribute("readonly", "");
    helper.style.position = "fixed";
    helper.style.left = "-9999px";
    document.body.append(helper);
    helper.select();
    const ok = document.execCommand("copy");
    helper.remove();
    return ok;
  }
}

async function shareBoardNote() {
  const text = clipText.value.trim();
  if (!text) {
    clipStatus.textContent = "Paste something first.";
    return;
  }
  shareClip.disabled = true;
  clipStatus.textContent = "";
  try {
    const res = await fetch("/api/clips", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        text,
        from_name: isHost ? nameInput.value || "Lanlink" : isPhone ? "Phone" : "Browser",
      }),
    });
    if (!res.ok) {
      clipStatus.textContent = await res.text();
      return;
    }
    const note = (await res.json()) as ClipNote;
    addClip(note);
    clipText.value = "";
    clipStatus.textContent = "Shared.";
    playCue("note");
    window.setTimeout(() => {
      if (clipStatus.textContent === "Shared.") clipStatus.textContent = "";
    }, 1600);
  } catch {
    clipStatus.textContent = "Could not share.";
  } finally {
    shareClip.disabled = false;
  }
}

function renderPeers() {
  applyGuestCopy();
  peersEl.innerHTML = "";
  const visible = peers.filter((p) => p.id !== selfId && !(isHost && p.kind === "self"));
  const visibleIds = new Set(visible.map((peer) => peer.id));
  for (const peerId of selectedPeerIds) {
    if (!visibleIds.has(peerId)) selectedPeerIds.delete(peerId);
  }
  emptyPeers.hidden = visible.length > 0;
  emptyPeers.textContent = "Waiting for another device on this network…";
  for (const peer of visible) {
    const li = document.createElement("li");
    const label = document.createElement("label");
    label.className = "device-option";
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.value = peer.id;
    checkbox.checked = selectedPeerIds.has(peer.id);
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) selectedPeerIds.add(peer.id);
      else selectedPeerIds.delete(peer.id);
      updateSendControls();
    });
    const meta = document.createElement("div");
    meta.innerHTML = `<strong></strong><div class="kind"></div>`;
    (meta.querySelector("strong") as HTMLElement).textContent = peer.name;
    (meta.querySelector(".kind") as HTMLElement).textContent = peerKindLabel(peer);
    label.append(checkbox, meta);
    li.append(label);
    peersEl.append(li);
  }
  updateSendControls();
}

async function saveReceivedFile(id: string, url: string, filename: string, knownSize = 0) {
  setXfer(id, { filename, direction: "receive", done: 0, total: knownSize, state: "active" });
  const res = await fetch(url);
  if (!res.ok) {
    finishXfer(id, "error");
    updateActivity(id, { status: "error" });
    return;
  }
  const total = Number(res.headers.get("content-length")) || knownSize;
  const reader = res.body?.getReader();
  const chunks: Uint8Array[] = [];
  let done = 0;
  if (reader) {
    while (true) {
      const step = await reader.read();
      if (step.done) break;
      chunks.push(step.value);
      done += step.value.byteLength;
      setXfer(id, { filename, direction: "receive", done, total: total || done, state: "active" });
    }
  }
  const blob = chunks.length
    ? new Blob(chunks.map((part) => part.slice()))
    : await res.blob();
  finishXfer(id, "done");
  const objectUrl = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = objectUrl;
  a.download = filename;
  a.target = "_blank";
  a.rel = "noopener";
  a.textContent = `Save / open ${filename}`;
  a.click();
  window.setTimeout(() => URL.revokeObjectURL(objectUrl), 60_000);
  updateActivity(id, { filename, direction: "receive", status: "done", size: total });
}

function blockedExecutable(filename: string): boolean {
  const blocked = new Set([
    "exe", "bat", "cmd", "com", "cpl", "dll", "scr", "pif", "msi", "msix", "msp", "mst",
    "appx", "msixbundle", "js", "jse", "vbs", "vbe", "wsf", "wsh", "ws", "ps1", "psd1",
    "psm1", "reg", "inf", "ins", "isp", "job", "lnk", "scf", "msc", "hta", "jar", "apk",
    "app", "command", "dmg", "pkg", "deb", "rpm", "run", "bin", "elf", "so", "dylib",
    "appimage", "action", "workflow", "scpt", "sh", "bash", "zsh", "gadget", "application",
    "sys", "drv", "ocx",
  ]);
  return filename
    .split(".")
    .slice(1)
    .some((part) => blocked.has(part.toLowerCase()));
}

async function sendTo(peerId: string) {
  const file = fileInput.files?.[0] ?? selectedFile ?? null;
  if (!file) {
    chooseThenSend = true;
    fileInput.click();
    return;
  }
  if (!peerId && hostId) peerId = hostId;
  const filename = file.name?.trim() || `upload-${Date.now()}`;
  if (blockedExecutable(filename)) {
    chosen.textContent = "That file type is blocked (programs and scripts).";
    return;
  }
  let id = localTransferId();
  updateActivity(id, {
    filename,
    direction: "send",
    status: "waiting",
    peer: peerName(peerId),
    size: file.size,
  });

  try {
    const offerRes = await fetch("/api/web-offer", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        target_id: peerId,
        filename,
        size: file.size,
        from_name: isHost ? nameInput.value || "Lanlink" : isPhone ? "Phone" : "Browser",
        from_id: selfId,
      }),
    });
    const offerBody = (await offerRes.json().catch(() => ({}))) as {
      id?: string;
      error?: string;
    };
    if (!offerRes.ok || !offerBody.id) {
      finishXfer(id, "error");
      updateActivity(id, { status: "error" });
      return;
    }
    adoptTransferId(id, offerBody.id);
    id = offerBody.id;
    pendingUploads.set(id, { file, filename });
  } catch (err) {
    finishXfer(id, "error");
    updateActivity(id, { status: "error" });
  }
}

async function uploadAccepted(id: string) {
  const pending = pendingUploads.get(id);
  if (!pending) return;
  pendingUploads.delete(id);
  setXfer(id, {
    filename: pending.filename,
    direction: "send",
    done: 0,
    total: pending.file.size,
    state: "active",
  });
  updateActivity(id, { status: "sending" });
  const data = new FormData();
  data.set("offer_id", id);
  data.append("file", pending.file, pending.filename);
  const upload = await uploadForm(data, pending.file, pending.filename, id);
  if (!upload.ok) {
    finishXfer(id, "error");
    updateActivity(id, { status: "error" });
    return;
  }
  finishXfer(id, "done");
  updateActivity(id, { status: "done" });
}

async function sendSelected() {
  if (selectedPeerIds.size === 0 || sendingBatch) return;
  const file = fileInput.files?.[0] ?? selectedFile;
  if (!file) {
    chooseThenSend = true;
    fileInput.click();
    return;
  }
  sendingBatch = true;
  updateSendControls();
  const recipients = [...selectedPeerIds];
  await Promise.allSettled(recipients.map((peerId) => sendTo(peerId)));
  sendingBatch = false;
  updateSendControls();
}

function uploadForm(
  data: FormData,
  file: File,
  filename: string,
  transferId: string,
): Promise<{ ok: boolean; status: number; text: string }> {
  return new Promise((resolve) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", "/api/send");
    xhr.timeout = 30 * 60 * 1000;
    xhr.upload.addEventListener("progress", (event) => {
      if (!event.lengthComputable) return;
      setXfer(transferId, {
        filename,
        direction: "send",
        done: Math.min(event.loaded, file.size),
        total: file.size,
        state: "active",
      });
    });
    xhr.addEventListener("load", () => {
      resolve({
        ok: xhr.status >= 200 && xhr.status < 300,
        status: xhr.status,
        text: xhr.responseText,
      });
    });
    xhr.addEventListener("error", () =>
      resolve({ ok: false, status: 0, text: "network error" }),
    );
    xhr.addEventListener("timeout", () =>
      resolve({ ok: false, status: 0, text: "upload timed out" }),
    );
    xhr.send(data);
  });
}

function incomingIsForUs(targetId: string): boolean {
  if (selfId && targetId === selfId) return true;
  if (isHost && hostId && targetId === hostId) return true;
  if (isHost && !targetId.startsWith("web-")) return true;
  return false;
}

function knowsTransfer(id: string): boolean {
  return (
    pendingIncoming?.id === id ||
    xfers.has(id) ||
    activity.some((record) => record.id === id)
  );
}

function closeIncomingIfMatching(id: string) {
  if (pendingIncoming?.id !== id) return;
  incomingWrap.hidden = true;
  pendingIncoming = null;
}

function showIncoming(ev: Incoming) {
  if (decidedIncomingIds.has(ev.id)) return;
  pendingIncoming = ev;
  incomingWrap.hidden = false;
  incomingWrap.removeAttribute("hidden");
  incomingText.textContent = `${ev.from} wants to send ${ev.filename} (${fmtSize(ev.size)})`;
  updateActivity(ev.id, {
    filename: ev.filename,
    direction: "receive",
    status: "waiting",
    peer: ev.from,
    size: ev.size,
  });
}

function onEvent(ev: EventMsg) {
  switch (ev.type) {
    case "hello":
      selfId = ev.web_id ?? ev.id;
      if (ev.web_id) {
        sessionStorage.setItem("lanlink-web-id", ev.web_id);
      }
      if (isHost) nameInput.value = ev.name;
      hostId = ev.id;
      joinUrl.textContent = ev.url;
      showSaveDir(ev.save_dir);
      qr.src = `/api/qr.svg?t=${Date.now()}`;
      peers = ev.peers;
      renderPeers();
      if (ev.clips) replaceClips(ev.clips);
      showNetworkWarn(ev.public_network);
      break;
    case "peers":
      peers = ev.peers;
      renderPeers();
      break;
    case "incoming":
      if (!incomingIsForUs(ev.target_id)) return;
      showIncoming(ev);
      break;
    case "accepted":
      void uploadAccepted(ev.id);
      break;
    case "ready":
      if (!incomingIsForUs(ev.target_id) || !knowsTransfer(ev.id)) return;
      void saveReceivedFile(ev.id, ev.download, ev.filename, ev.size);
      break;
    case "progress":
      if (!knowsTransfer(ev.id)) return;
      setXfer(ev.id, {
        filename: ev.filename,
        direction: ev.direction === "send" ? "send" : "receive",
        done: ev.done,
        total: ev.total,
        state: "active",
      });
      updateActivity(ev.id, {
        filename: ev.filename,
        direction: ev.direction === "send" ? "send" : "receive",
        status: ev.direction === "send" ? "sending" : "receiving",
        size: ev.total,
      });
      break;
    case "complete": {
      if (!knowsTransfer(ev.id)) return;
      if (!xfers.has(ev.id)) {
        setXfer(ev.id, {
          filename: ev.filename,
          direction: "receive",
          done: 1,
          total: 1,
          state: "active",
        });
      }
      finishXfer(ev.id, "done");
      updateActivity(ev.id, { filename: ev.filename, status: "done" });
      closeIncomingIfMatching(ev.id);
      break;
    }
    case "declined":
      if (!knowsTransfer(ev.id)) return;
      pendingUploads.delete(ev.id);
      finishXfer(ev.id, "error");
      updateActivity(ev.id, { status: "declined" });
      closeIncomingIfMatching(ev.id);
      break;
    case "error":
      if (ev.id && !knowsTransfer(ev.id)) return;
      if (ev.id) finishXfer(ev.id, "error");
      updateActivity(ev.id ?? `error-${Date.now()}`, {
        filename: "Transfer",
        status: "error",
      });
      break;
    case "clip":
      if (addClip(ev)) playCue("note");
      break;
    case "clips_cleared": {
      const hadNotes = clips.length > 0;
      replaceClips([]);
      if (hadNotes) playCue("clear");
      break;
    }
  }
}

acceptBtn.addEventListener("click", async () => {
  if (!pendingIncoming) return;
  const incoming = pendingIncoming;
  decidedIncomingIds.add(incoming.id);
  incomingWrap.hidden = true;
  pendingIncoming = null;
  acceptBtn.disabled = true;
  try {
    const res = await fetch(`/api/files/${incoming.id}/accept`, { method: "POST" });
    const body = (await res.json().catch(() => ({}))) as { download?: string };
    if (!res.ok) {
      decidedIncomingIds.delete(incoming.id);
      showIncoming(incoming);
      updateActivity(incoming.id, { status: "error" });
      return;
    }
    if (body.download) {
      await saveReceivedFile(incoming.id, body.download, incoming.filename, incoming.size);
      return;
    }
    updateActivity(incoming.id, { status: "receiving" });
  } catch (err) {
    decidedIncomingIds.delete(incoming.id);
    showIncoming(incoming);
    updateActivity(incoming.id, { status: "error" });
  } finally {
    acceptBtn.disabled = false;
  }
});

declineBtn.addEventListener("click", async () => {
  if (!pendingIncoming) return;
  const incoming = pendingIncoming;
  decidedIncomingIds.add(incoming.id);
  incomingWrap.hidden = true;
  pendingIncoming = null;
  try {
    const res = await fetch(`/api/files/${incoming.id}/decline`, { method: "POST" });
    if (!res.ok) {
      decidedIncomingIds.delete(incoming.id);
      showIncoming(incoming);
      updateActivity(incoming.id, { status: "error" });
    }
  } catch {
    decidedIncomingIds.delete(incoming.id);
    showIncoming(incoming);
    updateActivity(incoming.id, { status: "error" });
  }
});

nameForm.addEventListener("submit", async (e) => {
  e.preventDefault();
  await fetch("/api/name", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name: nameInput.value }),
  });
  qr.src = `/api/qr.svg?t=${Date.now()}`;
});

async function commitSaveDir(path: string) {
  const res = await fetch("/api/save-dir", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ path }),
  });
  if (!res.ok) {
    saveDirStatus.textContent = await res.text();
    return;
  }
  const body = (await res.json()) as { save_dir: string };
  showSaveDir(body.save_dir);
  saveDirStatus.textContent = "New incoming files will be saved here.";
}

saveDirForm.addEventListener("submit", async (e) => {
  e.preventDefault();
  await commitSaveDir(saveDirInput.value);
});

browseSaveDir.addEventListener("click", () => {
  saveDirInput.focus();
  saveDirInput.select();
  saveDirStatus.textContent =
    "Paste or type a folder path, then save. Incoming files on this computer will go there.";
});

function takeSelectedFile(file: File | null) {
  if (file && blockedExecutable(file.name)) {
    selectedFile = null;
    fileInput.value = "";
    chosen.textContent = "That file type is blocked (programs and scripts).";
    updateSendControls();
    return;
  }
  selectedFile = file;
  chosen.textContent = selectedFile ? selectedFile.name : "No file selected yet.";
  updateSendControls();
}

function onFileChosen() {
  takeSelectedFile(fileInput.files?.[0] ?? null);
  renderPeers();
  if (selectedFile && chooseThenSend) {
    chooseThenSend = false;
    void sendSelected();
  }
}
fileInput.addEventListener("change", onFileChosen);
fileInput.addEventListener("input", onFileChosen);

drop.addEventListener("dragover", (e) => {
  e.preventDefault();
  drop.classList.add("hover");
});
drop.addEventListener("dragleave", () => drop.classList.remove("hover"));
drop.addEventListener("drop", (e) => {
  e.preventDefault();
  drop.classList.remove("hover");
  takeSelectedFile(e.dataTransfer?.files[0] ?? null);
  renderPeers();
});

function openDrawer() {
  settingsRoot.hidden = false;
  closeSettings.focus();
}

function closeDrawer() {
  settingsRoot.hidden = true;
  openSettings.focus();
}

function openDisclaimerPanel() {
  disclaimerRoot.hidden = false;
  closeDisclaimer.focus();
}

function closeDisclaimerPanel() {
  disclaimerRoot.hidden = true;
  openDisclaimer.focus();
}

openSettings.addEventListener("click", openDrawer);
closeSettings.addEventListener("click", closeDrawer);
settingsBackdrop.addEventListener("click", closeDrawer);
openDisclaimer.addEventListener("click", openDisclaimerPanel);
closeDisclaimer.addEventListener("click", closeDisclaimerPanel);
disclaimerBackdrop.addEventListener("click", closeDisclaimerPanel);
document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (!disclaimerRoot.hidden) {
    closeDisclaimerPanel();
    return;
  }
  if (!settingsRoot.hidden) closeDrawer();
});

applySkin(skinPref());
applyTheme(themePref());
for (const input of document.querySelectorAll<HTMLInputElement>("input[name=theme]")) {
  input.addEventListener("change", () => {
    if (input.checked) applyTheme(input.value as ThemePref);
  });
}
for (const input of document.querySelectorAll<HTMLInputElement>("input[name=skin]")) {
  input.addEventListener("change", () => {
    if (input.checked) applySkin(input.value as SkinPref);
  });
}
themeQuick.addEventListener("click", () => {
  applyTheme(themePref() === "dark" ? "light" : "dark");
});

sendSelectedBtn.addEventListener("click", () => void sendSelected());

function setTransferView(view: TransferView) {
  transferView = view;
  localStorage.setItem(VIEW_KEY, view);
  renderActivity();
}

viewThumbnails.addEventListener("click", () => setTransferView("thumbnails"));
viewList.addEventListener("click", () => setTransferView("list"));
clearTransfers.addEventListener("click", () => {
  activity = [];
  saveActivity();
  renderActivity();
});

copyUrl.addEventListener("click", async () => {
  const text = joinUrl.textContent ?? "";
  if (!text || text === "Starting…") return;
  const ok = await copyText(text);
  copyUrl.textContent = ok ? "Copied" : "Copy failed";
  if (ok) {
    setTimeout(() => {
      copyUrl.textContent = "Copy address";
    }, 1500);
  }
});

shareClip.addEventListener("click", () => void shareBoardNote());
clearClipsBtn.addEventListener("click", () => void clearBoardNotes());
clipText.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    void shareBoardNote();
  }
});

let connectGen = 0;

function connect() {
  connectGen += 1;
  const gen = connectGen;
  if (socket) {
    socket.close();
    socket = null;
  }
  const role = isHost ? "host" : "guest";
  const wsProto = location.protocol === "https:" ? "wss" : "ws";
  const params = new URLSearchParams({ role });
  if (!isHost) {
    const saved = sessionStorage.getItem("lanlink-web-id");
    if (saved) params.set("id", saved);
    params.set("device", isPhone ? "phone" : "browser");
  }
  const ws = new WebSocket(`${wsProto}://${location.host}/api/ws?${params}`);
  socket = ws;
  ws.addEventListener("message", (msg) => {
    onEvent(JSON.parse(String(msg.data)) as EventMsg);
  });
  ws.addEventListener("close", () => {
    if (gen !== connectGen) return;
    if (socket === ws) socket = null;
    window.setTimeout(() => {
      if (gen !== connectGen) return;
      connect();
    }, 1500);
  });
}

connect();
applyGuestCopy();
renderActivity();
renderClips();
updateSendControls();
void fetch("/api/status")
  .then((r) => r.json())
  .then((status: { id: string; name: string; url: string; save_dir: string; peers: Peer[]; clips?: ClipNote[]; public_network?: boolean }) => {
    hostId = status.id;
    if (!joinUrl.textContent || joinUrl.textContent === "Starting…") {
      joinUrl.textContent = status.url;
      if (isHost) nameInput.value = status.name;
      showSaveDir(status.save_dir);
      qr.src = `/api/qr.svg?t=${Date.now()}`;
    }
    if (!peers.length && Array.isArray(status.peers)) {
      peers = status.peers;
    }
    if (Array.isArray(status.clips) && status.clips.length) {
      replaceClips(status.clips);
    }
    showNetworkWarn(status.public_network);
    renderPeers();
  })
  .catch(() => undefined);

async function pollInbox() {
  if (!isHost) return;
  try {
    const data = (await (await fetch("/api/inbox")).json()) as {
      id?: string;
      incoming?: Incoming[];
    };
    if (data.id) hostId = data.id;
    const first = data.incoming?.[0];
    if (first && !pendingIncoming) showIncoming(first);
  } catch {
    /* desktop may still be starting */
  }
}
void pollInbox();
window.setInterval(pollInbox, 700);
