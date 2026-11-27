import { describe, expect, it } from 'vitest';
import { choice, classify, detect, mb } from './download.js';

const asset = (name: string, size = 4_200_000) => ({
  name,
  browser_download_url: `https://example.invalid/${name}`,
  size,
});

const release = [
  'Islet-0.11.1-android.apk',
  'Islet-0.11.1-1.x86_64.rpm',
  'Islet_0.11.1_amd64.AppImage',
  'Islet_0.11.1_amd64.AppImage.sig',
  'Islet_0.11.1_amd64.deb',
  'Islet_0.11.1_universal.dmg',
  'Islet_0.11.1_x64-setup.exe',
  'Islet_0.11.1_x64-setup.exe.sig',
  'Islet_0.11.1_x64_en-US.msi',
  'Islet_0.11.1_x64_portable.zip',
  'islet@jimchi-cmd.github.io.shell-extension.zip',
  'SHA256SUMS',
  'latest.json',
].map((n) => asset(n));

describe('the download site', () => {
  it('finds each kind of file in a release, not its signature', () => {
    const f = classify(release);
    expect(f.windows?.name).toBe('Islet_0.11.1_x64-setup.exe');
    expect(f.appimage?.name).toBe('Islet_0.11.1_amd64.AppImage');
    expect(f.android?.name).toBe('Islet-0.11.1-android.apk');
    expect(f.gnome?.name).toBe('islet@jimchi-cmd.github.io.shell-extension.zip');
    expect(f.sums?.name).toBe('SHA256SUMS');
    expect(Object.keys(f)).not.toContain('androidDebug');
  });

  it('falls back to the debug phone app when a release has no signed one', () => {
    const f = classify([asset('Islet-0.10.0-android-debug.apk')]);
    expect(f.android?.name).toBe('Islet-0.10.0-android-debug.apk');
  });

  it('tells the systems apart, Android before Linux', () => {
    const android = 'Mozilla/5.0 (Linux; Android 14; SM-S911B) AppleWebKit/537.36 Chrome/131 Mobile Safari/537.36';
    expect(detect(android)).toBe('android');
    expect(detect('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36')).toBe('windows');
    expect(detect('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15')).toBe('mac');
    expect(detect('Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0')).toBe('linux');
    expect(detect('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)')).toBe('ios');
    expect(detect('', 'Windows')).toBe('windows');
    expect(detect('')).toBeNull();
  });

  it('points the big button at the right file, with its size', () => {
    const f = classify(release);
    expect(choice('windows', f)).toEqual({
      label: 'Download for Windows',
      url: 'https://example.invalid/Islet_0.11.1_x64-setup.exe',
      note: 'Windows 10 and 11 · 4.2 MB',
    });
    expect(choice('linux', f)?.url).toContain('_amd64.deb');
    expect(choice('ios', f)).toBeNull();
    expect(choice(null, f)).toBeNull();
    expect(choice('mac', {})).toBeNull();
  });

  it('writes sizes the way people read them', () => {
    expect(mb(3_690_024)).toBe('3.7 MB');
    expect(mb(84_000_000)).toBe('84 MB');
    expect(mb(0)).toBe('');
  });
});

describe('the hero light', () => {
  it('follows the pointer inside the picture and stays at its edges outside it', async () => {
    const { spot } = await import('./spotlight.js');
    const box = { left: 100, top: 50, width: 200, height: 100 };
    expect(spot(box, 200, 100)).toEqual({ x: 50, y: 50 });
    expect(spot(box, 0, 500)).toEqual({ x: 0, y: 100 });
  });
});
