import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { column, english, fill, LANGUAGES, resolve } from './i18n';

const TABLE = readFileSync(new URL('../../locales/strings.tsv', import.meta.url), 'utf8');

describe('languages', () => {
  it('reads one column of the shared table', () => {
    const fr = column(TABLE, 'fr');
    expect(fr['Control Centre']).toBe('Centre de contrôle');
    expect(column(TABLE, 'en')).toEqual({});
    expect(column(TABLE, 'xx')).toEqual({});
    expect(Object.keys(column(TABLE, 'ja')).length).toBeGreaterThan(500);
  });

  it('offers every column of the table', () => {
    const header = TABLE.split('\n')[0]?.split('\t');
    expect(LANGUAGES.slice(1).map((l) => l.code)).toEqual(header);
  });

  it('follows the system unless a language is picked', () => {
    expect(resolve('auto', 'es-MX')).toBe('es');
    expect(resolve('auto', 'sw-KE')).toBe('en');
    expect(resolve('de', 'fr-FR')).toBe('de');
    expect(resolve('xx', 'ja-JP')).toBe('ja');
  });

  it('fills values in, in any order', () => {
    expect(fill('Islet {} is available', ['0.7.0'])).toBe('Islet 0.7.0 is available');
    expect(fill('{1}の{0}を削除', ['数学', '月曜日'])).toBe('月曜日の数学を削除');
    expect(english('May‖short month')).toBe('May');
  });
});
