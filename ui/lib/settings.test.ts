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
