import { describe, expect, it } from 'vitest';
import { keyLooksValid, navStartArgs, parseStatus, permissionsFor } from './settings';

describe('navStartArgs', () => {
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
    expect(s).toEqual({ version: '0.1.0', running: true, autostart: false, routesKey: true, trip: null });
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
