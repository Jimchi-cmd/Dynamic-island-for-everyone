// Pure helpers for the Settings window (unit-tested; no Tauri calls here).

export type Os = 'macos' | 'windows' | 'linux' | string;
export type TravelMode = 'drive' | 'two-wheeler' | 'walk' | 'bicycle' | 'transit';

export interface Trip {
  label: string;
  destination: string;
  origin: string;
  mode: TravelMode;
  /** Set when the destination was picked from Google's suggestions. */
  placeId?: string;
}

/** A destination suggestion (`islet places … --json`). */
export interface PlaceSuggestion {
  placeId: string;
  main: string;
  secondary: string;
  full: string;
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

export interface ProStatus {
  active: boolean;
  forSale: boolean;
  buyUrl: string | null;
  keyHint: string;
  /** The store stopped accepting the key (refunded, revoked). */
  refused: boolean;
  checkedAt: number;
  expiresAt: number | null;
  features: string[];
}

/** `islet pro status --json`. Anything missing reads as "Pro is off". */
export function parsePro(json: string): ProStatus {
  const v = JSON.parse(json);
  const buy = typeof v.buyUrl === 'string' && v.buyUrl.startsWith('https://') ? v.buyUrl : null;
  return {
    active: v.active === true,
    forSale: v.forSale === true,
    buyUrl: buy,
    keyHint: typeof v.keyHint === 'string' ? v.keyHint : '',
    refused: v.refused === true,
    checkedAt: Number(v.checkedAt) || 0,
    expiresAt: typeof v.expiresAt === 'number' ? v.expiresAt : null,
    features: Array.isArray(v.features) ? v.features.map(String) : [],
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
  if (t.placeId && /^[A-Za-z0-9_-]+$/.test(t.placeId)) args.push('--place-id', t.placeId);
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
  privacyDots: boolean;
  clipboard: boolean;
  /** Apps in front of which the island steps aside. */
  hideWith: string[];
  /** Apps whose copies are never kept. */
  clipboardExclude: string[];
  lyrics: boolean;
  autoHide: boolean;
  idleClock: boolean;
  calls: boolean;
  downloads: boolean;
  /** Islet Pro: the light limits again, and a still island. */
  powerSaver: boolean;
  /** Hold pop-ups while a focus work round runs. */
  focusQuiet: boolean;
  /** Weather, battery and processor beside the time when you point at the island. */
  hoverStatus: boolean;
  /** "Connected to Home" when Wi-Fi joins a network. */
  wifiPopup: boolean;
  /** The user's own plugin widgets. */
  plugins: boolean;
  /** Airplane mode and Do Not Disturb: pop-ups and icons on the island. */
  modeIndicators: boolean;
  /** A border in the playing song's cover colour. */
  albumBorder: boolean;
  /** An installed font for the island's text ("" = the built-in one). */
  font: string;
  /** Say when a program hogs the processor or memory. */
  performanceAlert: boolean;
  /** Islet Pro looks. */
  albumGlow: boolean;
  visualizer: boolean;
  screenshotPreview: boolean;
  phone: boolean;
  shape: string;
  clockFace: string;
  hotkey: string;
  launcherHotkey: string;
  noteHotkey: string;
  accent: string;
  style: string;
  cardHidden: string[];
  size: string;
  /** "top", "top-left", "top-right" or "bottom". */
  position: string;
  /** A size for particular displays, over `size`. */
  displaySizes: Record<string, string>;
  offset: number;
  display: string;
}

/** Accent colours, as in `islet_core::prefs::ACCENTS` (a test checks they match). */
export const ACCENTS: Record<string, string> = {
  blue: '#0a84ff',
  indigo: '#6260ee',
  purple: '#a064ff',
  pink: '#ff468c',
  red: '#ff453a',
  orange: '#ff961e',
  yellow: '#fac814',
  lime: '#96d728',
  green: '#28c864',
  mint: '#46d7b4',
  teal: '#28bec8',
  graphite: '#9698a2',
};

/** The clock card's optional buttons, as in `islet_core::prefs::CARD_BUTTONS`. */
export const CARD_BUTTONS: { id: string; label: string; pro: boolean }[] = [
  { id: 'timer', label: 'Quick timer', pro: false },
  { id: 'calendar', label: 'Calendar', pro: false },
  { id: 'screenshot', label: 'Screenshot', pro: false },
  { id: 'lock', label: 'Lock', pro: false },
  { id: 'system', label: 'System monitor', pro: false },
  { id: 'controls', label: 'Volume and brightness', pro: false },
  { id: 'todo', label: 'To-do list', pro: true },
  { id: 'stopwatch', label: 'Stopwatch', pro: true },
];

/** What the island is made of, as in `islet_core::prefs::STYLES`. All but black are Islet Pro. */
export const STYLES: { id: string; label: string; pro: boolean }[] = [
  { id: 'black', label: 'Black', pro: false },
  { id: 'tinted', label: 'Tinted', pro: true },
  { id: 'glass', label: 'Glass', pro: true },
  { id: 'tinted-glass', label: 'Tinted glass', pro: true },
];

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
  privacyDots: 'privacy-dots',
  clipboard: 'clipboard',
  lyrics: 'lyrics',
  autoHide: 'auto-hide',
  idleClock: 'idle-clock',
  calls: 'calls',
  downloads: 'downloads',
  powerSaver: 'power-saver',
  focusQuiet: 'focus-quiet',
  hoverStatus: 'hover-status',
  wifiPopup: 'wifi-popup',
  plugins: 'plugins',
  albumBorder: 'album-border',
  modeIndicators: 'mode-indicators',
  performanceAlert: 'performance-alert',
  albumGlow: 'album-glow',
  visualizer: 'visualizer',
  screenshotPreview: 'screenshot-preview',
  phone: 'phone',
};

/** `islet focus streak --json`. */
export interface FocusStreak {
  /** Days in a row, counting today or up to yesterday. */
  days: number;
  /** Focus rounds finished today. */
  today: number;
  best: number;
  rounds: number;
}

export function parseStreak(json: string): FocusStreak {
  const v = JSON.parse(json);
  const n = (x: unknown) => (typeof x === 'number' && x > 0 ? Math.floor(x) : 0);
  return { days: n(v.days), today: n(v.today), best: n(v.best), rounds: n(v.rounds) };
}

const plural = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);

/** One line under the focus form: the streak, and what today still needs. */
export function streakLine(s: FocusStreak): string {
  const best = `Best: ${plural(s.best, 'day', 'days')}.`;
  if (s.days === 0) return `No streak right now. Finish a focus round to start one. ${best}`;
  const run = s.days === 1 ? 'Day 1 of a new streak' : `${s.days} days in a row`;
  const today =
    s.today > 0 ? `${plural(s.today, 'round', 'rounds')} done today` : 'finish a round today to keep it going';
  return `${run}: ${today}. ${best}`;
}

/** A lesson of the class timetable (`islet class --json`). */
export interface Lesson {
  /** 0 = Monday … 6 = Sunday. */
  day: number;
  /** Minutes after midnight. */
  start: number;
  end: number;
  name: string;
  room: string;
}

export const WEEK_DAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];
const DAY_ARGS = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];

export function parseLessons(json: string): Lesson[] {
  const v = JSON.parse(json);
  if (!Array.isArray(v)) return [];
  return v
    .filter((l) => l && Number.isInteger(l.day) && l.day >= 0 && l.day <= 6 && typeof l.name === 'string')
    .map((l) => ({
      day: l.day,
      start: Number(l.start) || 0,
      end: Number(l.end) || 0,
      name: l.name,
      room: typeof l.room === 'string' ? l.room : '',
    }));
}

/** Minutes after midnight as "09:00" or "9:00 AM". */
export function formatMinutes(minutes: number, h24: boolean): string {
  return formatAlarmTime({ hour: Math.floor(minutes / 60) % 24, minute: minutes % 60 }, h24);
}

/** Arguments for `islet class add`, from the form, or what is missing. */
export function lessonAddArgs(
  day: number,
  start: string,
  end: string,
  name: string,
  room: string,
): string[] | { error: string } {
  const time = /^\d{1,2}:\d{2}$/;
  if (!name.trim()) return { error: 'Give the lesson a name.' };
  if (!time.test(start.trim()) || !time.test(end.trim())) return { error: 'Pick when it starts and ends.' };
  if (end.trim().padStart(5, '0') <= start.trim().padStart(5, '0')) return { error: 'A lesson ends after it starts.' };
  if (name.trim().startsWith('-') || room.trim().startsWith('-')) return { error: 'Names cannot start with "-".' };
  const args = ['class', 'add', DAY_ARGS[day] ?? 'mon', start.trim(), end.trim(), name.trim()];
  if (room.trim()) args.push('--room', room.trim());
  return args;
}

/** `islet phone pair --json`: what the phone scans. */
export interface PhonePairing {
  uri: string;
  /** The QR code: one string of '1' (dark) and '0' per row. */
  rows: string[];
  hosts: string[];
  port: number;
}

export function parsePairing(json: string): PhonePairing | null {
  const v = JSON.parse(json);
  const rows = Array.isArray(v.rows) ? v.rows.filter((r: unknown) => typeof r === 'string' && /^[01]+$/.test(r)) : [];
  if (typeof v.uri !== 'string' || !v.uri.startsWith('islet://pair?') || rows.length < 21) return null;
  if (rows.some((r: string) => r.length !== rows.length)) return null;
  return {
    uri: v.uri,
    rows,
    hosts: Array.isArray(v.hosts) ? v.hosts.map(String) : [],
    port: Number(v.port) || 0,
  };
}

/** The QR code as one SVG path (a unit square per dark module). */
export function qrPath(rows: string[]): string {
  let d = '';
  rows.forEach((row, y) => {
    for (let x = 0; x < row.length; x++) if (row[x] === '1') d += `M${x} ${y}h1v1h-1z`;
  });
  return d;
}
