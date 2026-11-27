// Which download fits the visitor's device, and which release file is which.
// Used by the page (to set the big Download button) and by
// scripts/site/build.mjs (to fill in the links when the site is built).

/** The release files the site links to, by what they're for. */
export const KINDS = {
  android: /-android\.apk$/,
  androidDebug: /-android-debug\.apk$/,
  windows: /_x64-setup\.exe$/,
  windowsMsi: /_x64_en-US\.msi$/,
  windowsZip: /_x64_portable\.zip$/,
  mac: /_universal\.dmg$/,
  deb: /_amd64\.deb$/,
  rpm: /\.x86_64\.rpm$/,
  appimage: /_amd64\.AppImage$/,
  gnome: /\.shell-extension\.zip$/,
  sums: /^SHA256SUMS$/,
};

/**
 * Sort a release's files by kind: `{ windows: {name, url, size}, … }`.
 * A signed phone app wins over a debug one; a missing kind is left out.
 */
export function classify(assets) {
  const out = {};
  for (const [kind, re] of Object.entries(KINDS)) {
    const a = assets.find((x) => re.test(x.name));
    if (a) out[kind] = { name: a.name, url: a.browser_download_url ?? a.url, size: a.size ?? 0 };
  }
  if (!out.android && out.androidDebug) out.android = out.androidDebug;
  delete out.androidDebug;
  return out;
}

/**
 * The visitor's system, from the browser's user agent (and its platform
 * hint, where the browser gives one): "android", "windows", "mac", "linux",
 * "ios", or null when it can't tell.
 */
export function detect(ua = '', platform = '') {
  const p = platform.toLowerCase();
  if (p === 'android' || /Android/i.test(ua)) return 'android';
  if (/iPhone|iPad|iPod/i.test(ua)) return 'ios';
  if (p === 'windows' || /Windows/i.test(ua)) return 'windows';
  if (p === 'macos' || /Macintosh|Mac OS X/i.test(ua)) return 'mac';
  if (p === 'linux' || p === 'chrome os' || /Linux|X11|CrOS/i.test(ua)) return 'linux';
  return null;
}

const LABELS = {
  android: ['Download for Android', 'android', 'Android 8 or later'],
  windows: ['Download for Windows', 'windows', 'Windows 10 and 11'],
  mac: ['Download for Mac', 'mac', 'macOS, Apple silicon and Intel'],
  linux: ['Download for Linux', 'deb', '.deb · other formats below'],
};

/** What the big button should say and link to on this system (null: show every download). */
export function choice(system, files) {
  const l = system && LABELS[system];
  if (!l) return null;
  const f = files[l[1]];
  if (!f) return null;
  return { label: l[0], url: f.url, note: `${l[2]} · ${mb(f.size)}` };
}

/** A file size for people: "4.2 MB". */
export function mb(bytes) {
  if (!bytes) return '';
  const m = bytes / 1_000_000;
  return `${m < 10 ? m.toFixed(1) : Math.round(m)} MB`;
}

// On the page: point the big button at this device's download.
if (typeof document !== 'undefined') {
  const data = document.getElementById('release');
  const button = document.getElementById('get');
  if (data && button) {
    const files = JSON.parse(data.textContent || '{}');
    const nav = /** @type {any} */ (navigator);
    const system = detect(navigator.userAgent, nav.userAgentData?.platform ?? '');
    const c = choice(system, files);
    const note = document.getElementById('get-note');
    if (c) {
      button.textContent = c.label;
      button.setAttribute('href', c.url);
      if (note) note.textContent = c.note;
    } else if (system === 'ios' && note) {
      note.textContent = "Not for iPhone or iPad (Apple doesn't allow it). Get it for your computer or Android phone.";
    }
  }
}
