import { N } from './i18n';
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
  { id: 'drive', label: N('Driving') },
  { id: 'two-wheeler', label: N('Motorbike / boda') },
  { id: 'walk', label: N('Walking') },
  { id: 'bicycle', label: N('Cycling') },
  { id: 'transit', label: N('Public transport') },
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
          title: N('Bluetooth'),
          body: N('macOS asks once, so the island can show the battery level of headphones, mice and keyboards.'),
        },
        {
          title: N('Automation (only if needed)'),
          body: N('If Now Playing can’t be read directly, Islet falls back to asking Spotify or Music. macOS then asks whether Islet may control them.'),
        },
        {
          title: N('Opening an app from the internet'),
          body: N('Islet is not notarized yet. If macOS blocks it, open System Settings → Privacy & Security and click “Open Anyway”.'),
        },
      ];
    case 'windows':
      return [
        {
          title: N('No permissions needed'),
          body: N('Music and battery levels come from Windows itself. If SmartScreen warns about an unrecognized app, choose “More info” → “Run anyway”.'),
        },
      ];
    default:
      return [
        {
          title: N('Keyring'),
          body: N('API keys are stored in your keyring (GNOME Keyring or KWallet) with secret-tool. Install libsecret-tools if it is missing.'),
        },
        {
          title: N('Wayland'),
          body: N('On GNOME with Wayland, Islet runs through XWayland. Nothing to set up.'),
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
  // Islet Pro's page became Extras.
  if (page === 'pro') return { page: 'extras', ssid: null };
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
  /** "auto", "full", "light" or "saver" (see `islet_core::limits::MODES`). */
  performance: string;
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
  /** Looks. */
  albumGlow: boolean;
  visualizer: boolean;
  screenshotPreview: boolean;
  phone: boolean;
  shape: string;
  clockFace: string;
  /** "auto", "12" or "24". */
  clockHours: string;
  /** "auto" or a language code (see `LANGUAGES` in i18n.ts). */
  language?: string;
  hotkey: string;
  launcherHotkey: string;
  noteHotkey: string;
  accent: string;
  style: string;
  /** The tinted styles' colour, "#rrggbb" ("" = the accent). */
  tint?: string;
  /** Control Centre's layout (see `CC_STYLES`), its buttons' order, and whether a photo is behind it. */
  ccStyle?: string;
  ccOrder?: string[];
  ccPhoto?: boolean;
  cardHidden: string[];
  /** Opt-in buttons (to-do, stopwatch) switched on. */
  cardShown: string[];
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
export const CARD_BUTTONS: { id: string; label: string; optIn: boolean }[] = [
  { id: 'timer', label: N('Quick timer'), optIn: false },
  { id: 'calendar', label: N('Calendar'), optIn: false },
  { id: 'screenshot', label: N('Screenshot'), optIn: false },
  { id: 'lock', label: N('Lock'), optIn: false },
  { id: 'system', label: N('System monitor'), optIn: false },
  { id: 'controls', label: N('Volume and brightness'), optIn: false },
  { id: 'todo', label: N('To-do list'), optIn: true },
  { id: 'stopwatch', label: N('Stopwatch'), optIn: true },
];

/** Control Centre's layouts, as in `islet_core::prefs::CC_STYLES`. */
export const CC_STYLES: { id: string; label: string; hint: string }[] = [
  { id: 'classic', label: N('Classic'), hint: N('The time, Wi-Fi and Bluetooth with their names, the tools in a row') },
  { id: 'modules', label: N('Modules'), hint: N('A block of round buttons, with the timer beside it') },
  { id: 'sliders', label: N('Sliders'), hint: N('Volume and brightness as tall sliders beside the buttons') },
  { id: 'dock', label: N('Dock'), hint: N('A big clock, with every button in one labelled row') },
];

/** Every Control Centre button, in the usual order (`islet_core::prefs::CC_BUTTONS`). */
export const CC_BUTTONS: { id: string; label: string }[] = [
  { id: 'wifi', label: N('Wi-Fi') },
  { id: 'bluetooth', label: N('Bluetooth') },
  { id: 'timer', label: N('Quick timer') },
  { id: 'system', label: N('System monitor') },
  { id: 'controls', label: N('Volume and brightness') },
  { id: 'calendar', label: N('Calendar') },
  { id: 'screenshot', label: N('Screenshot') },
  { id: 'lock', label: N('Lock') },
  { id: 'shelf', label: N('Shelf') },
  { id: 'todo', label: N('To-do list') },
  { id: 'stopwatch', label: N('Stopwatch') },
  { id: 'clipboard', label: N('Clipboard history') },
  { id: 'shortcuts', label: N('Shortcuts') },
];

/** The buttons in the user's order; ones left out follow in the usual order (as the island does). */
export function ccOrder(order: string[] | undefined): string[] {
  const ids = CC_BUTTONS.map((b) => b.id);
  const out: string[] = [];
  for (const id of order ?? []) if (ids.includes(id) && !out.includes(id)) out.push(id);
  return out.concat(ids.filter((id) => !out.includes(id)));
}

/** The order with one button moved one place up (-1) or down (+1). */
export function moveButton(order: string[], id: string, by: -1 | 1): string[] {
  const out = [...order];
  const i = out.indexOf(id);
  const j = i + by;
  if (i < 0 || j < 0 || j >= out.length) return out;
  const [a, b] = [out[i] as string, out[j] as string];
  out[i] = b;
  out[j] = a;
  return out;
}

/** Does the clock card show this button? Opt-in ones show once switched on. */
export function cardShows(b: { id: string; optIn: boolean }, p: { cardHidden?: string[]; cardShown?: string[] } | null): boolean {
  return b.optIn ? (p?.cardShown ?? []).includes(b.id) : !(p?.cardHidden ?? []).includes(b.id);
}

/** What the island is made of, as in `islet_core::prefs::STYLES`. */
export const STYLES: { id: string; label: string }[] = [
  { id: 'black', label: N('Black') },
  { id: 'tinted', label: N('Tinted') },
  { id: 'glass', label: N('Glass') },
  { id: 'tinted-glass', label: N('Tinted glass') },
];

/** The Performance choices, as in `islet_core::limits::MODES`. */
export const PERFORMANCE: { id: string; label: string; hint: string }[] = [
  { id: 'auto', label: N('Automatic'), hint: N('full, or light on a 4 GB laptop') },
  { id: 'full', label: N('Full'), hint: N('60 fps, sharp art, long histories') },
  { id: 'light', label: N('Light'), hint: N('for 4 GB laptops') },
  { id: 'saver', label: N('Power saver'), hint: N('light, and nothing moves by itself') },
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
