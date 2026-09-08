# Security

Lanlink is for a network you already trust, like home Wi‑Fi or a small office. It is not a private internet app. There is no login, and nothing is encrypted.

Use it there. Skip café and airport Wi‑Fi. Do not open the join port on your router or through a public tunnel.

The yellow **Disclaimer** button in the bottom-right of the app says the same thing.

## What the app already does

Someone sending you a file only ships the bytes after you tap **Accept**. Decline, or wait two minutes, and the offer goes away.

Program and script names are blocked (for example `.exe`, `.bat`, `.ps1`, `.js`, `.apk`). A name like `photo.jpg.exe` is blocked too.

Only the computer running the desktop window can change this machine’s display name or the folder where accepted files are saved. A phone on the LAN cannot.

Saved files get a cleaned-up name. If that name already exists, Lanlink picks a new one instead of overwriting. Browsers are told to download, not to run the file in the page.

Shared notes are short (8,000 characters), the host keeps the last 50, and one address can only post a handful every half minute. Notes live in memory, not in Downloads.

The app listens on port **7420** on this computer so phones and other PCs can join. Desktops may also announce themselves with mDNS on **UDP 5353**. Allow those on a private/home firewall, not a public one.

On Windows, if Settings marks this Wi‑Fi or Ethernet as **Public**, the desktop window warns you not to share the join URL. Extra virtual adapters that stay Public do not trigger that banner.

The desktop window is ordinary: it loads the local page and does not get extra system powers.

## What that still means

Anyone who can open the join URL can offer a file, read the notes board, and see who is connected.

Traffic is ordinary HTTP. Another device on the same Wi‑Fi can read notes, and can read file bytes after someone taps Accept.

File names and note text go out to every connected screen, not just the two people in a transfer.

A webpage you have open in a browser could talk to Lanlink while it is running, because the LAN API does not check which site called it.

Treat the board like a whiteboard in the room, not a place for passwords.

Glance at the file name before you Accept.

## Not built yet

- Send events only to the people in that transfer, instead of to every connected device
- Encryption or a join PIN, if this is ever used off a trusted LAN
- Listen only on the home/LAN address, not on every network interface
- Extra checks so a random webpage cannot post while Lanlink is open
- Stricter size limits so one rude machine cannot fill the disk

Windows Public-network warning is already in.

## Quick habits

Keep the firewall on. Allow **7420** (and **5353** if devices cannot find each other) on a **Private** network only.

Do not port-forward 7420, and do not put it on Tailscale funnel or a similar public link.

Share the join URL only with people on that LAN.

Do not paste secrets into Shared notes.
