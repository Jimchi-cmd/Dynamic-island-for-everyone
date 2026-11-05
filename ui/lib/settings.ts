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
