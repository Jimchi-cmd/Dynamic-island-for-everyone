# Navigation ETA

Islet shows your travel time to a destination, with live traffic, in the island. When you're ready to drive, one tap hands off to **Google Maps** or **Waze** for turn-by-turn directions.

Neither Google Maps nor Waze lets other apps read the navigation running inside them, so Islet works out the ETA itself with the **Google Routes API**. Each person uses their **own** API key. Islet ships with no key and never sends your trips anywhere except Google.

## 1. Get a Google Routes API key (one time)

1. Open the [Google Cloud console](https://console.cloud.google.com/) and create a project (or pick an existing one).
2. Turn on billing for the project. Google requires it even for free-tier usage.
3. Go to **APIs & Services → Library**, search for **Routes API** and click **Enable**.
4. Go to **APIs & Services → Credentials → Create credentials → API key**.
5. Click the new key and, under **API restrictions**, choose **Restrict key** → **Routes API**. A restricted key is useless to anyone else if it ever leaks.
6. Optional but wise: in **Billing → Budgets & alerts**, set a small monthly budget such as $1 so you get an email if usage ever climbs.

## 2. Give the key to Islet

```bash
islet key routes
```

Paste the key and press Enter. Islet stores it in your OS keychain (Keychain on macOS, Credential Manager on Windows, GNOME Keyring or KWallet on Linux) and never writes it to a file. On Linux this uses `secret-tool`; if you don't have it, install your distro's `libsecret-tools` (Debian/Ubuntu) or `libsecret` package.

The Settings app will offer the same thing in a form (Phase 4).

## 3. Start a trip

```bash
islet nav start "Plot 5, Kampala Road" --from "Ntinda, Kampala" --label Work
islet nav start "Entebbe Airport" --from "0.3476,32.5825" --mode drive
islet nav stop
```

- `--from` is required. Islet has no location access. An address or a `lat,lng` pair works.
- `--mode`: `drive` (default), `two-wheeler`, `walk`, `bicycle` or `transit`. Traffic is used for driving and two-wheelers.
- `--label` is the short name the island shows. It defaults to the destination.
- The trip stays until you stop it, even across restarts. If Islet isn't running, the trip is saved and shown when it starts.

The island shows a green arrow and the ETA. Click it to open the card with the arrival time, distance, traffic delay and first instruction, plus **Google Maps** and **Waze** buttons and **✕** to stop.

## What it costs

Traffic-aware routes are billed by Google under the Routes API **Pro** tier. At the time of writing, each project gets a monthly allowance of free requests (about 5,000), and after that you pay roughly $10 per 1,000. Check [Google's current pricing](https://developers.google.com/maps/billing-and-pricing/pricing). Islet keeps requests low:

| Situation | Refresh |
|---|---|
| No trip | never |
| Trip active, card closed | every 5 min (every 2 min in the last 15 min) |
| Card open | every minute |

A one-hour drive with the card closed uses about 15 requests. A trip left running all day uses about 290. **Stop trips you no longer need.** `ISLET_DEBUG=1` logs a running request count.

## Privacy and Google's terms

- Requests go straight from your computer to `routes.googleapis.com`, with your key, origin and destination. There is no Islet server.
- Islet saves only what you typed (destination, origin, label, mode) in `trip.json` in its config folder. Route results are kept in memory for display and never cached to disk, as Google's terms require.
- The card credits **Google Maps** because it shows Routes data without a Google map.
- Hand-off uses Google's documented [Maps URLs](https://developers.google.com/maps/documentation/urls/get-started) and Waze's documented [deep links](https://developers.google.com/waze/deeplinks). Neither needs a key.
- Islet doesn't scrape or call private APIs of Google Maps or Waze.

## Live turn-by-turn

Mirroring the turn-by-turn instructions from a Maps or Waze session on your **phone** is planned as an Android companion (Phase 8). iOS does not allow it.
