// Pure helpers for the Settings window (unit-tested; no Tauri calls here).

export type Os = 'macos' | 'windows' | 'linux' | string;
export type TravelMode = 'drive' | 'two-wheeler' | 'walk' | 'bicycle' | 'transit';

export interface Trip {
  label: string;
  destination: string;
  origin: string;
  mode: TravelMode;
}

export interface Status {
  version: string;
  running: boolean;
  autostart: boolean;
  routesKey: boolean;
  trip: Trip | null;
  alarms: Alarm[];
  spotifyLinked: boolean;
}

export const MODES: { id: TravelMode; label: string }[] = [
  { id: 'drive', label: 'Driving' },
  { id: 'two-wheeler', label: 'Motorbike / boda' },
  { id: 'walk', label: 'Walking' },
  { id: 'bicycle', label: 'Cycling' },
  { id: 'transit', label: 'Public transport' },
];

export function parseStatus(json: string): Status {
  const v = JSON.parse(json);
  return {
    version: String(v.version ?? ''),
    running: v.running === true,
    autostart: v.autostart === true,
    routesKey: v.routesKey === true,
    trip: v.trip ?? null,
    alarms: Array.isArray(v.alarms) ? v.alarms : [],
    spotifyLinked: v.spotifyLinked === true,
  };
}

/** Arguments for `islet nav start`, or an error to show next to the form. */
export function navStartArgs(t: Trip): string[] | { error: string } {
  const destination = t.destination.trim();
  const origin = t.origin.trim();
  if (!destination) return { error: 'Where to? Enter a destination.' };
  if (!origin) return { error: 'Where from? Enter a starting address or "lat,lng".' };
  if (destination.startsWith('-') || origin.startsWith('-') || t.label.trim().startsWith('-')) {
    return { error: 'Addresses cannot start with "-".' };
  }
  const args = ['nav', 'start', destination, '--from', origin, '--mode', t.mode];
  if (t.label.trim()) args.push('--label', t.label.trim());
  return args;
}

/** Google API keys are 39 characters starting with "AIza"; warn on anything else. */
export function keyLooksValid(key: string): boolean {
  return /^AIza[0-9A-Za-z_-]{35}$/.test(key.trim());
}

export interface Permission {
  title: string;
  body: string;
}

/** What Islet may ask for on each OS, shown on the first-run screen. */
export function permissionsFor(os: Os): Permission[] {
  switch (os) {
    case 'macos':
      return [
        {
          title: 'Bluetooth',
          body: 'macOS asks once, so the island can show the battery level of headphones, mice and keyboards.',
        },
        {
          title: 'Automation (only if needed)',
          body: 'If Now Playing can’t be read directly, Islet falls back to asking Spotify or Music. macOS then asks whether Islet may control them.',
        },
        {
          title: 'Opening an app from the internet',
          body: 'Islet is not notarized yet. If macOS blocks it, open System Settings → Privacy & Security and click “Open Anyway”.',
        },
      ];
    case 'windows':
      return [
        {
          title: 'No permissions needed',
          body: 'Music and battery levels come from Windows itself. If SmartScreen warns about an unrecognized app, choose “More info” → “Run anyway”.',
        },
      ];
    default:
      return [
        {
          title: 'Keyring',
          body: 'API keys are stored in your keyring (GNOME Keyring or KWallet) with secret-tool. Install libsecret-tools if it is missing.',
        },
        {
          title: 'Wayland',
          body: 'On GNOME with Wayland, Islet runs through XWayland. Nothing to set up.',
        },
      ];
  }
}

export interface Alarm {
  id: number;
  hour: number;
  minute: number;
  label: string;
  /** Bit 0 = Sunday … bit 6 = Saturday; 0 = once. */
  days: number;
  enabled: boolean;
}

export const DAY_LETTERS = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];
const DAY_NAMES = ['sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat'];
const DAILY = 0b1111111;
const WEEKDAYS = 0b0111110;
const WEEKENDS = 0b1000001;

export function describeDays(days: number): string {
  switch (days & DAILY) {
    case 0:
      return 'Once';
    case DAILY:
      return 'Every day';
    case WEEKDAYS:
      return 'Weekdays';
    case WEEKENDS:
      return 'Weekends';
    default:
      return DAY_NAMES.filter((_, i) => days & (1 << i))
        .map((d) => d.charAt(0).toUpperCase() + d.slice(1))
        .join(', ');
  }
}

/** "07:30" (24-hour) or "7:30 AM". */
export function formatAlarmTime(a: Pick<Alarm, 'hour' | 'minute'>, h24: boolean): string {
  const mm = String(a.minute).padStart(2, '0');
  if (h24) return `${String(a.hour).padStart(2, '0')}:${mm}`;
  const h = a.hour % 12 === 0 ? 12 : a.hour % 12;
  return `${h}:${mm} ${a.hour < 12 ? 'AM' : 'PM'}`;
}

/** Arguments for `islet alarm add`, from an <input type="time"> value. */
export function alarmAddArgs(time: string, label: string, days: number): string[] | { error: string } {
  if (!/^\d{1,2}:\d{2}$/.test(time.trim())) return { error: 'Pick a time.' };
  if (label.trim().startsWith('-')) return { error: 'The name cannot start with "-".' };
  const args = ['alarm', 'add', time.trim()];
  if (label.trim()) args.push('--label', label.trim());
  const d = days & DAILY;
  if (d) args.push('--days', DAY_NAMES.filter((_, i) => d & (1 << i)).join(','));
  return args;
}

export function parseAlarms(json: string): Alarm[] {
  const v = JSON.parse(json);
  return Array.isArray(v) ? v : [];
}

export interface WifiNetwork {
  ssid: string;
  strength: number;
  secure: boolean;
  known: boolean;
  active: boolean;
}

export interface WifiList {
  available: boolean;
  current: string | null;
  networks: WifiNetwork[];
  note: string | null;
}

export interface BtDevice {
  id: string;
  name: string;
  connected: boolean;
}

export interface BtList {
  available: boolean;
  powered: boolean;
  canConnect: boolean;
  devices: BtDevice[];
  note: string | null;
}

/** `--page wifi:Cafe` → { page: 'connections', ssid: 'Cafe' }. */
export function parsePage(arg: string | null): { page: string; ssid: string | null } | null {
  if (!arg) return null;
  const [page, ...rest] = arg.split(':');
  const ssid = rest.join(':');
  if (page === 'wifi' || page === 'bluetooth') return { page: 'connections', ssid: ssid || null };
  return { page: page ?? '', ssid: null };
}

/** Signal 0–100 as 1–3 bars, for a text meter. */
export function signalBars(strength: number): string {
  const n = strength < 35 ? 1 : strength < 65 ? 2 : 3;
  return '▂▄▆'.slice(0, n).padEnd(3, '·');
}

/** island.json, as `islet prefs --json` prints it. */
export interface IslandPrefs {
  volumeHud: boolean;
  brightnessHud: boolean;
  capsLockHud: boolean;
  notifications: boolean;
  weather: { name: string; lat: number; lon: number } | null;
  fahrenheit: boolean;
  calendar: boolean;
  mediaExclude: string[];
  webArtwork: boolean;
  spotify: boolean;
}

/** Each switch's name for `islet set <name> on|off`. */
export const SWITCH_NAMES: Record<string, string> = {
  volumeHud: 'volume-hud',
  brightnessHud: 'brightness-hud',
  capsLockHud: 'caps-lock-hud',
  notifications: 'notifications',
  fahrenheit: 'fahrenheit',
  calendar: 'calendar',
  webArtwork: 'web-artwork',
  spotify: 'spotify',
};
