# Lanlink

Share files between devices on the same local network. Written in Rust.

- **Desktop:** Windows and Linux (Tauri 2)
- **Phone / other computer:** open the join URL or scan the QR code in a browser — no mobile app yet
- **Laptop to laptop:** each app discovers the other with mDNS and sends files over HTTP

This is **local-network only**. There are no accounts and no encryption. File data waits for Accept. Shared notes are visible to everyone on the join URL. Do not expose port `7420` to the internet.

See **[SECURITY.md](SECURITY.md)** for measures, ports, and remaining work.

## Requirements

- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) 18+
- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) (WebView2 on Windows; webkitgtk on Linux)
- **Windows:** [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload (`link.exe`). A quiet winget install of this failed here with exit code 1603 — run the installer by hand if `cargo` says the MSVC linker was not found.

## Run (desktop)

```bash
npm install
npm run desktop
```

That builds the UI, starts the LAN server on port **7420**, and opens the desktop window.

Linux extra packages (Debian/Ubuntu):

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

On this Windows machine, after C++ tools and the Windows SDK are installed:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\dev.ps1
```

## How it works

1. The app advertises `_lanlink._tcp` on mDNS and listens on `0.0.0.0:7420`.
2. Nearby Lanlink desktops show up in **Devices nearby**.
3. Sending a file asks the receiver to **Accept** or **Decline**. Programs and scripts are blocked. Accepted files are written under `Downloads/Lanlink`.
4. A phone on the same Wi‑Fi opens `http://<your-lan-ip>:7420` (or the QR code) and can send to this computer, or receive a file you send to that browser session.
5. **Shared notes** broadcasts a line of text to every connected device. There is no Accept step for notes.

Allow TCP **7420** and UDP **5353** (mDNS) through the OS firewall **on a private/LAN profile** if devices cannot see each other. Do not forward those ports on the router.

## Project layout

- `src-tauri/src/lib.rs` — starts the Tauri app
- `src-tauri/src/server.rs` — Axum HTTP/WebSocket API + static UI
- `src-tauri/src/discovery.rs` — mDNS advertise/browse
- `src-tauri/src/transfer.rs` — outbound HTTP send + receive streaming
- `ui/` — Vite + vanilla TypeScript frontend
- `SECURITY.md` — trust model, ports, current controls, later TODOs

## License

MIT
