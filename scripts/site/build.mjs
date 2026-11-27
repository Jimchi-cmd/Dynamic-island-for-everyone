#!/usr/bin/env node
// Build the download website: fill site/index.html with the latest
// release's files, and copy it with its media, fonts and icon to a folder
// that GitHub Pages serves.
//
//   gh api repos/OWNER/REPO/releases/latest > release.json
//   node scripts/site/build.mjs release.json build/site
//
// The page makes no requests anywhere but its own folder and GitHub's
// release downloads: no fonts, scripts or analytics from other sites.

import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { classify, mb } from '../../site/download.js';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');

/** Fill the page's {{…}} slots from a release (GitHub's JSON for it). */
export function render(template, release) {
  const files = classify(release.assets ?? []);
  const version = String(release.tag_name ?? '').replace(/^v/, '');
  const date = new Date(release.published_at ?? Date.now()).toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'long',
    year: 'numeric',
    timeZone: 'UTC',
  });
  const slot = (key) => {
    const [what, kind] = key.split(':');
    if (what === 'version') return version;
    if (what === 'date') return date;
    if (what === 'release') return release.html_url;
    // The files, for the page's script (it only picks the big button's link).
    if (what === 'files') return JSON.stringify(files).replace(/</g, '\\u003c');
    const f = files[kind];
    if (!f) throw new Error(`the release has no ${kind} file`);
    if (what === 'url') return f.url;
    if (what === 'name') return f.name;
    if (what === 'size') return mb(f.size);
    throw new Error(`unknown slot {{${key}}}`);
  };
  return template.replace(/\{\{([a-z]+(?::[a-zA-Z]+)?)\}\}/g, (_, key) => slot(key));
}

/** Local files the page refers to (src="…", href="…" without a scheme or #). */
export function localRefs(html) {
  const refs = new Set();
  for (const m of html.matchAll(/\b(?:src|href|poster)="([^"#:]+)"/g)) refs.add(m[1]);
  for (const m of html.matchAll(/url\(["']?([^"')#:]+)["']?\)/g)) refs.add(m[1]);
  return [...refs];
}

function build(releaseFile, out) {
  const release = JSON.parse(readFileSync(releaseFile, 'utf8'));
  const site = join(root, 'site');
  const html = render(readFileSync(join(site, 'index.html'), 'utf8'), release);
  if (html.includes('{{')) throw new Error('index.html has a slot that was not filled');
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  writeFileSync(join(out, 'index.html'), html);
  for (const f of readdirSync(site)) {
    if (/\.(css|js)$/.test(f)) copyFileSync(join(site, f), join(out, f));
  }
  for (const d of ['media', 'fonts']) cpSync(join(site, d), join(out, d), { recursive: true });
  copyFileSync(join(root, 'assets', 'icon.svg'), join(out, 'icon.svg'));
  // Every file the page or its stylesheet uses must be there.
  const css = readFileSync(join(out, 'style.css'), 'utf8');
  const missing = [...localRefs(html), ...localRefs(css)].filter((r) => !existsSync(join(out, r)));
  if (missing.length) throw new Error(`missing from the site: ${missing.join(', ')}`);
  console.log(`site for ${release.tag_name} in ${out}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [releaseFile, out = join(root, 'build', 'site')] = process.argv.slice(2);
  if (!releaseFile) {
    console.error('usage: node scripts/site/build.mjs release.json [out-dir]');
    process.exit(2);
  }
  build(releaseFile, out);
}
