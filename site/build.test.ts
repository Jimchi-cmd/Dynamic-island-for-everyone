import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
// @ts-expect-error: a plain Node script, without type declarations
import { localRefs, render } from '../scripts/site/build.mjs';

const names = [
  'Islet-0.11.1-android.apk',
  'Islet-0.11.1-1.x86_64.rpm',
  'Islet_0.11.1_amd64.AppImage',
  'Islet_0.11.1_amd64.deb',
  'Islet_0.11.1_universal.dmg',
  'Islet_0.11.1_x64-setup.exe',
  'Islet_0.11.1_x64_en-US.msi',
  'Islet_0.11.1_x64_portable.zip',
  'islet@jimchi-cmd.github.io.shell-extension.zip',
  'SHA256SUMS',
];
const release = {
  tag_name: 'v0.11.1',
  published_at: '2026-11-25T10:00:00Z',
  html_url: 'https://example.invalid/releases/v0.11.1',
  assets: names.map((name) => ({ name, browser_download_url: `https://example.invalid/${name}`, size: 3_500_000 })),
};

describe('building the site', () => {
  const page = readFileSync(new URL('./index.html', import.meta.url), 'utf8');

  it('fills every slot in the page from a release', () => {
    const html = render(page, release);
    expect(html).not.toContain('{{');
    expect(html).toContain('Download Islet 0.11.1');
    expect(html).toContain('Released 25 November 2026');
    expect(html).toContain('href="https://example.invalid/Islet_0.11.1_x64-setup.exe"');
    expect(html).toContain('Windows 10 and 11 · 3.5 MB');
    // The files the page's script reads, as JSON it can't break out of.
    const json = html.match(/<script id="release" type="application\/json">(.*?)<\/script>/)?.[1] ?? '';
    expect(JSON.parse(json).android.name).toBe('Islet-0.11.1-android.apk');
  });

  it('refuses a release that lacks a file the page links to', () => {
    const noMac = { ...release, assets: release.assets.filter((a) => !a.name.endsWith('.dmg')) };
    expect(() => render(page, noMac)).toThrow(/no mac file/);
  });

  it('loads nothing from other sites but the release downloads', () => {
    const html = render(page, release);
    const external = [...html.matchAll(/\b(?:src|href)="(https?:[^"]+)"/g)].map((m) => m[1]);
    for (const url of external) expect(url).toMatch(/^https:\/\/(github\.com\/Jimchi-cmd\/|example\.invalid\/)/);
    expect(html).not.toMatch(/<(?:script|link)[^>]+(?:src|href)="https?:/);
    expect(localRefs(html)).toContain('media/hero.mp4');
  });
});
