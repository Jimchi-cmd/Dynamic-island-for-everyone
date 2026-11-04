import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { WINDOW } from './geometry';

// The window size is declared in three places; keep them in step.
describe('window size', () => {
  it('matches tauri.conf.json', () => {
    const conf = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
    const win = conf.app.windows.find((w: { label: string }) => w.label === 'island');
    expect({ w: win.width, h: win.height }).toEqual(WINDOW);
  });

  it('matches WINDOW_LOGICAL in the Rust core', () => {
    const rs = readFileSync('src-tauri/src/lib.rs', 'utf8');
    const m = rs.match(/WINDOW_LOGICAL: \(f64, f64\) = \(([\d.]+), ([\d.]+)\)/);
    expect(m).not.toBeNull();
    expect({ w: Number(m![1]), h: Number(m![2]) }).toEqual(WINDOW);
  });
});
