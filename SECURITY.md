# Security

Lanlink is a **trusted-LAN** tool. It is not a private internet service. There are no accounts, no TLS, and no sender authentication.

Use it on a home or office network you control. Do not use it on public Wi‑Fi. Do not forward **TCP 7420** (or **UDP 5353** for mDNS) off your LAN.

The in-app **Disclaimer** button (bottom right) shows the same warning.

## Measures in this release

- **File consent.** Bytes are not stored or forwarded until that recipient taps Accept. Decline or a 120-second timeout drops the offer.
- **No executables.** Names with program or script extensions (for example `.exe`, `.bat`, `.ps1`, `.js`, `.apk`, `.dmg`) are rejected on send and receive, including double extensions such as `photo.jpg.exe`.
- **Host-only settings.** `POST /api/name` and `POST /api/save-dir` work only from loopback (`127.0.0.1`). Guests on the LAN cannot rename this computer or change the save folder.
- **Safe writes.** Filenames are sanitized. A unique path is used so an accepted file does not overwrite an existing one. Downloads use `Content-Disposition: attachment`.
- **Shared notes caps.** Notes are at most 8,000 characters. The host keeps the last 50. Posting is rate-limited (8 notes per 30 seconds per client IP). Notes stay in memory, not in Downloads.
- **Ports.** The app listens on **TCP 7420** on all interfaces (`0.0.0.0`) for UI, files, WebSocket, and notes. Desktops may advertise with mDNS on **UDP 5353**. Allow these only on a **private/LAN** firewall profile.
- **Public-network warning.** On Windows, if the current firewall profile is Public, the UI warns you not to share the join URL.
- **Desktop privileges.** The Tauri window uses `core:default` only and loads `http://127.0.0.1:7420`.

## What is still open (by design)

- Anyone who can open the join URL can offer a file, read the notes board, and see connected devices.
- Traffic is HTTP, not encrypted. A device on the same LAN can read notes and file bytes after accept.
- WebSocket events are broadcast to every connected client (filenames and note text included).
- CORS allows any origin. A webpage you open while Lanlink is running could call LAN APIs.

Treat the board like a shared whiteboard, not a password vault. Review file names before Accept.

## Later (not in this pass)

- [ ] Scope WebSocket events to the sender and the target instead of broadcasting all events
- [ ] TLS or a join PIN if the app is ever used off a trusted LAN
- [x] ~~Warn on a Windows “Public” firewall profile~~
- [ ] Bind to the LAN address only
- [ ] CSRF / origin checks for mutating APIs
- [ ] Tighter upload quotas to limit disk-fill from a rude LAN client

## Operator checklist

1. Keep the OS firewall on. Allow 7420/tcp (and 5353/udp if discovery fails) on **Private** networks only.
2. Do not port-forward 7420 on the router, Tailscale funnel, or a public tunnel.
3. Share the join URL only with people on that LAN.
4. Do not paste secrets into Shared notes.
