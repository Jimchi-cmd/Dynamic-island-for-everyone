# Plugin widgets

A plugin is a small program of your own whose output Islet shows on the
island as a live activity: unread mail, a build's progress, a server's
status, the next bus. Plugins run on your computer only, on the schedule
you give them, and only while **Settings → General → Plugin widgets** is
on (or `islet set plugins on`). Islet sends nothing anywhere for them;
a plugin can of course use the network itself.

## Where they go

Each plugin is a folder in the `plugins` folder of Islet's settings folder.
`islet plugins --where` prints it:

| System  | Folder                                         |
|---------|------------------------------------------------|
| Linux   | `~/.config/islet/plugins`                      |
| macOS   | `~/Library/Application Support/Islet/plugins`  |
| Windows | `%APPDATA%\Islet\plugins`                      |

After adding or changing a plugin, run `islet plugins reload` (or press
*Reread the folder* in Settings). `islet plugins` lists what was found and
says why a plugin can't run.

## plugin.json

```json
{ "name": "Unread mail", "command": ["./unread.sh"], "every": 60 }
```

- `name`: 1 to 40 characters.
- `command`: the program and its arguments. It runs in the plugin's
  folder, never through a shell. A first word starting with `./` is a file
  in that folder; anything else is found on your `PATH`.
- `every`: seconds between runs (5 at least, once a day at most;
  60 if left out).

A run that takes longer than 10 seconds is stopped, and what was shown
stays. At most 8 plugins run, in folder-name order.

## What a plugin prints

Plain text: the first line is the title, the second the subtitle.

```
3 unread
from Alex and Sam
```

Or one JSON object, for a ring and a colour:

```json
{ "title": "Build", "subtitle": "412 of 600 tests", "progress": 0.69, "color": [52, 199, 89], "open": "https://ci.example.com/run/42" }
```

`progress` (0 to 1) fills the ring; `color` is red, green, blue;
`open` is a web link (https) or a file that a tap on the open card opens.
Printing nothing takes the activity away until the plugin prints again.

## Examples

**Linux and macOS**, `plugins/disk/plugin.json`:

```json
{ "name": "Disk", "command": ["sh", "./disk.sh"], "every": 300 }
```

`plugins/disk/disk.sh`:

```sh
#!/bin/sh
used=$(df -P / | awk 'NR==2 { print $5 }')
echo "Disk ${used} full"
```

**Windows**, `plugins\uptime\plugin.json`:

```json
{ "name": "Uptime", "command": ["powershell", "-NoProfile", "-File", "./uptime.ps1"], "every": 600 }
```

`plugins\uptime\uptime.ps1`:

```powershell
$up = (Get-Date) - (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
"Up $([int]$up.TotalHours) hours"
```

Scripts can also show activities on their own schedule with
`islet activity start|end` (see the manual); plugins are for things worth
checking regularly.
