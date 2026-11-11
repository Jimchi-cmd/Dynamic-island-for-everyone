import { describe, expect, it } from 'vitest';
import { keyLooksValid, navStartArgs, parseStatus, permissionsFor } from './settings';

describe('navStartArgs', () => {
  it('passes a picked place ID, and only a safe one', () => {
    const t = { label: '', destination: 'Garden City', origin: 'Ntinda', mode: 'drive' as const, placeId: 'ChIJ_x-1' };
    expect(navStartArgs(t)).toContain('--place-id');
    expect(navStartArgs({ ...t, placeId: 'bad id;rm' })).not.toContain('--place-id');
  });

  it('builds the command line', () => {
    expect(navStartArgs({ label: ' Work ', destination: 'Kampala Road', origin: 'Ntinda', mode: 'drive' })).toEqual([
      'nav', 'start', 'Kampala Road', '--from', 'Ntinda', '--mode', 'drive', '--label', 'Work',
    ]);
  });

  it('needs a destination and an origin', () => {
    expect(navStartArgs({ label: '', destination: ' ', origin: 'A', mode: 'drive' })).toHaveProperty('error');
    expect(navStartArgs({ label: '', destination: 'B', origin: '', mode: 'walk' })).toHaveProperty('error');
  });

  it('refuses values that look like options', () => {
    expect(navStartArgs({ label: '', destination: '--evil', origin: 'A', mode: 'drive' })).toHaveProperty('error');
  });
});

describe('parseStatus', () => {
  it('reads the island status', () => {
    const s = parseStatus('{"version":"0.1.0","running":true,"autostart":false,"routesKey":true,"trip":null}');
    expect(s).toEqual({
      version: '0.1.0',
      running: true,
      autostart: false,
      routesKey: true,
      trip: null,
      alarms: [],
      spotifyLinked: false,
    });
  });
});

describe('keyLooksValid', () => {
  it('recognises the Google key shape', () => {
    expect(keyLooksValid('AIza' + 'x'.repeat(35))).toBe(true);
    expect(keyLooksValid('hello')).toBe(false);
  });
});

describe('permissionsFor', () => {
  it('has guidance for every OS', () => {
    for (const os of ['macos', 'windows', 'linux']) expect(permissionsFor(os).length).toBeGreaterThan(0);
  });
});

import { alarmAddArgs, describeDays, formatAlarmTime } from './settings';

describe('alarms', () => {
  it('builds alarm add arguments', () => {
    expect(alarmAddArgs('07:30', ' Gym ', 0b0111110)).toEqual([
      'alarm', 'add', '07:30', '--label', 'Gym', '--days', 'mon,tue,wed,thu,fri',
    ]);
    expect(alarmAddArgs('21:00', '', 0)).toEqual(['alarm', 'add', '21:00']);
    expect(alarmAddArgs('', '', 0)).toHaveProperty('error');
  });

  it('describes days and times', () => {
    expect(describeDays(0)).toBe('Once');
    expect(describeDays(0b0101010)).toBe('Mon, Wed, Fri');
    expect(describeDays(0b1000001)).toBe('Weekends');
    expect(formatAlarmTime({ hour: 0, minute: 5 }, false)).toBe('12:05 AM');
    expect(formatAlarmTime({ hour: 18, minute: 0 }, true)).toBe('18:00');
  });
});

import { parsePage, signalBars } from './settings';

describe('connections', () => {
  it('reads the page the island asked for', () => {
    expect(parsePage('wifi:My Cafe:2')).toEqual({ page: 'connections', ssid: 'My Cafe:2' });
    expect(parsePage('bluetooth')).toEqual({ page: 'connections', ssid: null });
    expect(parsePage(null)).toBeNull();
  });

  it('draws signal bars', () => {
    expect(signalBars(90)).toBe('▂▄▆');
    expect(signalBars(20)).toBe('▂··');
  });
});

import { readFileSync } from 'node:fs';
import { SWITCH_NAMES } from './settings';

describe('switches', () => {
  it('match the ones the island understands', () => {
    const rust = readFileSync(new URL('../../crates/islet-core/src/prefs.rs', import.meta.url), 'utf8');
    const list = rust.match(/pub const SWITCHES: \[&str; \d+\] = \[([^\]]*)\]/)?.[1] ?? '';
    const names = [...list.matchAll(/"([a-z-]+)"/g)].map((m) => m[1]).sort();
    expect(Object.values(SWITCH_NAMES).sort()).toEqual(names);
  });
});

import { parsePro } from './settings';

describe('parsePro', () => {
  it('reads the status, and treats anything odd as "off"', () => {
    const on = parsePro(
      '{"active":true,"forSale":true,"buyUrl":"https://buy.example/islet","keyHint":"…7F2C","features":["Stopwatch"]}',
    );
    expect(on).toMatchObject({ active: true, forSale: true, keyHint: '…7F2C', features: ['Stopwatch'] });
    expect(parsePro('{}')).toMatchObject({ active: false, forSale: false, buyUrl: null, features: [] });
    expect(parsePro('{"active":"yes","buyUrl":"javascript:alert(1)"}')).toMatchObject({ active: false, buyUrl: null });
  });
});

import { ACCENTS, CARD_BUTTONS, STYLES } from './settings';

describe('looks', () => {
  const rust = readFileSync(new URL('../../crates/islet-core/src/prefs.rs', import.meta.url), 'utf8');

  it('offer the accent colours the island knows, in the same RGB', () => {
    const block = rust.match(/pub const ACCENTS: [^=]*= \[([\s\S]*?)\];/)?.[1] ?? '';
    const fromRust = [...block.matchAll(/\("([a-z]+)", \[(\d+), (\d+), (\d+)\]\)/g)].map(
      (m) => [m[1], '#' + [m[2], m[3], m[4]].map((n) => Number(n).toString(16).padStart(2, '0')).join('')] as const,
    );
    expect(fromRust.length).toBeGreaterThan(6);
    expect(Object.entries(ACCENTS)).toEqual(fromRust);
  });

  it('offer the styles the island knows', () => {
    const list = rust.match(/pub const STYLES: \[&str; \d+\] = \[([^\]]*)\]/)?.[1] ?? '';
    expect(STYLES.map((s) => s.id)).toEqual([...list.matchAll(/"([a-z-]+)"/g)].map((m) => m[1]));
    expect(STYLES.filter((s) => !s.pro).map((s) => s.id)).toEqual(['black']);
  });

  it('offer the clock-card buttons the island knows', () => {
    const block = rust.match(/pub const CARD_BUTTONS: [^=]*= \[([\s\S]*?)\];/)?.[1] ?? '';
    const fromRust = [...block.matchAll(/\("([a-z]+)", "([^"]+)"\)/g)].map((m) => [m[1], m[2]]);
    expect(CARD_BUTTONS.map((b) => [b.id, b.label])).toEqual(fromRust);
  });
});

import { parseStreak, streakLine } from './settings';

describe('focus streak', () => {
  it('reads the numbers and ignores anything odd', () => {
    expect(parseStreak('{"days":3,"today":1,"best":7,"rounds":20}')).toEqual({ days: 3, today: 1, best: 7, rounds: 20 });
    expect(parseStreak('{"days":"3","today":-1}')).toEqual({ days: 0, today: 0, best: 0, rounds: 0 });
  });

  it('says what today still needs', () => {
    expect(streakLine({ days: 3, today: 2, best: 7, rounds: 20 })).toBe('3 days in a row: 2 rounds done today. Best: 7 days.');
    expect(streakLine({ days: 3, today: 0, best: 3, rounds: 9 })).toBe(
      '3 days in a row: finish a round today to keep it going. Best: 3 days.',
    );
    expect(streakLine({ days: 1, today: 1, best: 1, rounds: 1 })).toBe('Day 1 of a new streak: 1 round done today. Best: 1 day.');
    expect(streakLine({ days: 0, today: 0, best: 4, rounds: 12 })).toBe(
      'No streak right now. Finish a focus round to start one. Best: 4 days.',
    );
  });
});
