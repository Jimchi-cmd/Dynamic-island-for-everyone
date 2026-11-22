# Extras

Everything here is free, like the rest of Islet. (These features were once sold as "Islet Pro"; that's gone.)

## Looks

In Settings → General:

- **Style:** tinted, glass or tinted glass instead of black.
- **Shape:** "Notch" squares the island's top corners and joins it to the top of the screen, so it reads as growing out of the screen's edge, like a MacBook's notch. It applies while the island sits at the top with no gap above it.
- **Empty island shows:** the time, the day and the time ("Wed 4 · 14:05"), or the time with seconds. Seconds redraw every second, so Power saver shows the plain time instead.
- **Clock:** 12-hour (2:05 PM) or 24-hour (14:05). **Automatic** follows your system's language.
- **Glow in the album's colour:** while music plays, the island's edge glows in the cover's main colour.
- **Moving bars as the progress bar:** the song's progress is drawn as a row of moving bars. The motion is drawn, not measured: Islet does not listen to what is playing.

From a terminal:

```bash
islet look shape notch
```

```bash
islet look clock date
```

```bash
islet look hours 12
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

- **From Settings:** Extras → Quick notes.
- **From a terminal or a launcher:**

```bash
islet note "Ask about the essay deadline"
```

- **With a hotkey:** bind a key to `islet note` in your system's keyboard settings. Islet doesn't register a second global shortcut of its own yet.

To see where the file is:

```bash
islet note --where
```

## Performance

Settings → Extras → **Performance** decides how much Islet keeps and how smoothly it moves:

| | Light | Full | Power saver |
|---|---|---|---|
| Waveform, bell and spinner | 30 frames a second | 60 frames a second | still |
| Album art kept at | 112 px | 256 px | 112 px |
| Clipboard history | 20 copies of 10,000 characters | 500 copies of 100,000 characters | 20 copies of 10,000 characters |
| Files on the shelf | 12 | 100 | 100 |
| Notifications remembered | 20 | 200 | 20 |

- **Automatic** (the default) is Full, or Light on a computer with 4 GB of memory or less, or two processor cores or fewer.
- **Full** uses more memory, and a little more power while music plays. An idle island still does no work at all.
- **Power saver** is for a long day on battery. It holds the waveform still, so playing music costs no extra power, and an open music card moves its progress bar once a second.

Nothing you saved is removed when you switch: the shelf, notes and shortcuts stay as they are. The clipboard history shortens as you copy new things.

From a terminal:

```bash
islet performance saver
```

## Stopwatch and to-do buttons

The clock card's row keeps Wi-Fi's and Bluetooth's names with two tools beside them, so the **To-do list** and **Stopwatch** buttons wait until you switch them on in Settings → General → Clock card buttons.

```bash
islet card show stopwatch
```
