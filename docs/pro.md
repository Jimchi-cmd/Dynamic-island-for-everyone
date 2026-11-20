# Islet Pro

Islet Pro is a one-time purchase that unlocks extra features. Everything Islet did before Pro stays free.

## For users

1. Buy Pro from the **Buy Islet Pro** button in Settings → Islet Pro. Your key arrives by email and on the receipt page.
2. Paste the key in Settings → Islet Pro and click **Activate**.
3. One key works on several of your computers (three, or five where Pro is sold through Gumroad). To move it, click **Move Pro to another computer** on the old one first.

**Privacy:**

- Activating sends the key and a label for this computer (for example "Islet on Windows") to the store, so it can check the key.
- After that, Islet asks the store once a week whether the key is still valid, which is how a refunded key stops working.
- Being offline never turns Pro off.
- The key is kept in your system keychain, and nothing else is sent.

From a terminal:

```bash
islet pro activate
```

```bash
islet pro status
```

```bash
islet pro deactivate
```

## Looks

In Settings → General:

- **Style:** tinted, glass or tinted glass instead of black.
- **Shape:** "Notch" squares the island's top corners, so it reads as growing out of the screen's edge. It applies while the gap above the island is 0.
- **Empty island shows:** the time, the day and the time ("Wed 4 · 14:05"), or the time with seconds. Seconds redraw every second, so Power saver shows the plain time instead.
- **Glow in the album's colour:** while music plays, the island's edge glows in the cover's main colour.
- **Moving bars as the progress bar:** the song's progress is drawn as a row of moving bars. The motion is drawn, not measured: Islet does not listen to what is playing.

From a terminal:

```bash
islet look shape notch
```

```bash
islet look clock date
```

## Shortcuts in the right-click menu, and early builds

- Your first two shortcut buttons also appear in the island's right-click menu, in the rows the menu has free.
- **Early builds:** Settings → Updates → **Look for an early build** shows the newest pre-release. You install it by hand from the download page. From a terminal:

```bash
islet update check --early
```

## Screenshot preview

When the system saves a screenshot, the island shows its thumbnail for eight seconds. Open the island and tap the card to open the picture.

- Islet watches the folder the system saves into: `Pictures/Screenshots` on Windows and Linux (or `Pictures` if there is no such folder), and the Desktop on macOS, where only files named like a screenshot count.
- It reads the new picture to draw the thumbnail. Nothing leaves the computer.
- Switch it off in Settings → General → **Show each new screenshot on the island**.

## Quick notes

One line at a time, added with the date and time to a plain text file: `Islet notes.txt` in your Documents folder. Open it in any editor.

- **From Settings:** Islet Pro → Quick notes.
- **From a terminal or a launcher:**

```bash
islet note "Ask about the essay deadline"
```

- **With a hotkey:** bind a key to `islet note` in your system's keyboard settings. Islet doesn't register a second global shortcut of its own yet.

To see where the file is:

```bash
islet note --where
```

## No low-end limits, and Power saver

The free app is tuned for a laptop with 4 GB of memory. With Pro those limits are off as soon as your key is active, with nothing to switch on:

| | Free | Pro | Pro with Power saver |
|---|---|---|---|
| Waveform, bell and spinner | 30 frames a second | 60 frames a second | still |
| Album art kept at | 112 px | 256 px | 112 px |
| Clipboard history | 20 copies of 10,000 characters | 500 copies of 100,000 characters | 20 copies of 10,000 characters |
| Files on the shelf | 12 | 100 | 100 |
| Notifications remembered | 20 | 200 | 20 |

Without the limits Islet uses more memory, and a little more power while music plays. An idle island still does no work at all.

**Power saver** (Settings → Islet Pro → Performance) is for a long day on battery:

- It brings the light limits back.
- It holds the waveform still, so playing music costs no extra power.
- An open music card moves its progress bar once a second.
- Nothing you saved is removed: the shelf, notes and shortcuts stay as they are. The clipboard history shortens to 20 as you copy new things.

From a terminal:

```bash
islet set power-saver on
```

## For the maintainer: setting up the store

Islet works with **Gumroad** (recommended: the best known, and the quickest to set up), **Polar** (popular with developers), or **Lemon Squeezy** (if you already sell there). Each handles the payment and issues license keys; check each one's current terms for how it handles sales tax and VAT where your buyers live. You need to be 18 or older to open a seller account.

| | Gumroad | Polar |
|---|---|---|
| Sign up | gumroad.com | polar.sh |
| Product | A digital product, price about $9 | One-time product, price about $9 |
| Keys | In the product's settings, turn on **Generate a unique license key per sale** | Add a **License keys** benefit with an activation limit of 3 |
| Id you need | The **product id** shown with the license key setting | Organization id (Settings → General) |
| Computers per key | 5 activations (Gumroad can't free one from the app; you can reset a buyer's count in Gumroad) | 3, and **Move Pro to another computer** frees one |

Then, in the GitHub repository, open **Settings → Secrets and variables → Actions → Variables** and add the following. These ids are public (they appear in every checkout link), so they are variables, not secrets:

- **For Gumroad:** `ISLET_GUMROAD_PRODUCT`, the product id.
- **For Polar:** `ISLET_POLAR_ORG`, your organization id.
- **For Lemon Squeezy:** `ISLET_LEMON_STORE` and `ISLET_LEMON_PRODUCT`.
- **For any of them:** `ISLET_BUY_URL`, the product's checkout link (it must start with `https://`; for Gumroad, something like `https://yourname.gumroad.com/l/islet-pro`).

If more than one is set, Gumroad is used first, then Polar.

The next release is built with these variables, and its Settings shows the Buy button. Builds without them say "Islet Pro isn't on sale in this build".

### Testing

To try Pro features in a development build, start the island with `ISLET_PRO=1`. Release builds ignore this variable.

### About the code

Islet is open source under the MIT License, and that includes the Pro features. Someone could build their own copy with the check removed. Pro is for people who want the ready-made, signed, auto-updating app and want to support it. Keep that in mind before adding anything more aggressive.
