# Security

Lanlink is built for a network you already trust — home Wi‑Fi, or a small office. It is not a private internet app: there is no login, and nothing is encrypted. So use it at home, skip it on café and airport Wi‑Fi, and don't open the join port on your router or expose it through a public tunnel. The yellow **Disclaimer** button in the bottom-right corner of the app says much the same thing.

## What the app already does for you

Nobody can push a file onto your machine. When someone sends you something, all you get is an offer — the bytes only start moving once you tap **Accept**. Decline it, or just ignore it for two minutes, and the offer expires on its own.

Programs and scripts are turned away by name, so `.exe`, `.bat`, `.ps1`, `.js`, `.apk` and a long list of their relatives won't come through. That includes disguised names like `photo.jpg.exe`.

Anything you do accept is saved under a cleaned-up filename, and if that name is already taken Lanlink picks a fresh one rather than overwriting what you had. Browsers are told to download these files, never to run them in the page.

Only the computer running the desktop window can rename this machine or change where accepted files are saved. A phone on the same network can't touch either setting.

Shared notes are deliberately small: 8,000 characters each, the host keeps only the last 50, and any one address can post at most 8 in a half-minute window. Notes live in memory only — they never land in your Downloads folder.

For devices to find each other, the app listens on port **7420** on this computer, and desktops may also announce themselves over mDNS on **UDP 5353**. Allow both on a private or home firewall profile, not a public one.

On Windows, if Settings has this Wi‑Fi or Ethernet marked as **Public**, the desktop window warns you not to hand out the join URL. Spare virtual adapters that sit on Public won't set that warning off — it follows the adapter you're actually sharing over.

The desktop window itself is unremarkable. It loads the local page and is granted no special system powers.

## What that still leaves open

Anyone who can reach the join URL can offer you a file, read the notes board, and see who else is connected. There's no gate beyond being on the network.

Traffic is plain HTTP, so another device on the same Wi‑Fi can read your notes, and can read file bytes once someone has accepted a transfer.

File names and note text are broadcast to every connected screen, not just to the two people involved in a transfer.

A webpage you happen to have open could also talk to Lanlink while it's running, because the LAN API doesn't check which site is calling it.

Two habits follow from all this: treat the notes board like a whiteboard in a shared room rather than somewhere to put passwords, and take a second to read the file name before you accept it.

## Not built yet

- Sending transfer events only to the two devices involved, instead of to everyone connected
- Encryption or a join PIN, for if this is ever used off a trusted LAN
- Listening only on the home/LAN address rather than every network interface
- Extra checks so a stray webpage can't post while Lanlink is open
- Tighter size limits so one rude machine can't fill the disk

The Windows public-network warning was on this list and is now done.

## Quick habits

Keep your firewall on, and allow **7420** — plus **5353** if devices can't find each other — on a **Private** network only.

Don't port-forward 7420, and don't put it behind a Tailscale funnel or any similar public link.

Share the join URL only with people who are on that network with you, and keep secrets out of Shared notes.
