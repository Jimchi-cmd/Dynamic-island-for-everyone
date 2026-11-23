// The words Settings shows, in the language picked for Islet. Every
// translation lives in locales/strings.tsv, shared with the island and the
// phone app: one line per English sentence, one column per language. A key
// may carry a note for translators after "‖", which is never shown.

/** The languages Islet speaks, by code, each in its own name. */
export const LANGUAGES: { code: string; name: string }[] = [
  { code: 'auto', name: 'Automatic' },
  { code: 'en', name: 'English' },
  { code: 'es', name: 'Español' },
  { code: 'fr', name: 'Français' },
  { code: 'de', name: 'Deutsch' },
  { code: 'pt', name: 'Português' },
  { code: 'ru', name: 'Русский' },
  { code: 'zh', name: '中文（简体）' },
  { code: 'ja', name: '日本語' },
  { code: 'ar', name: 'العربية' },
  { code: 'hi', name: 'हिन्दी' },
];

/** Marks a label in a list of data for translation; tr() translates it where it's shown. */
export const N = (key: string): string => key;

/** Languages written right to left. */
export const RTL = ['ar'];

/** The English text of a table key: the key without its note. */
export function english(key: string): string {
  const i = key.indexOf('‖');
  return i < 0 ? key : key.slice(0, i).trimEnd();
}

/** One language's column of the table, keyed by the English. */
export function column(table: string, code: string): Record<string, string> {
  const lines = table.split('\n');
  const header = (lines[0] ?? '').split('\t');
  const col = header.indexOf(code);
  const words: Record<string, string> = {};
  if (col <= 0) return words;
  for (const line of lines.slice(1)) {
    if (!line || line.startsWith('#')) continue;
    const cells = line.split('\t');
    const key = cells[0];
    const tr = cells[col];
    if (key && tr) words[key] = tr;
  }
  return words;
}

/** The language to use: the setting, or with "auto" the system's when Islet has it; else English. */
export function resolve(setting: string, systemLocale: string): string {
  const has = (c: string) => LANGUAGES.some((l) => l.code === c && c !== 'auto');
  if (setting !== 'auto' && has(setting)) return setting;
  const base = (systemLocale.split(/[-_.]/)[0] ?? '').toLowerCase();
  return has(base) ? base : 'en';
}

/** Fill each "{}" in order, and "{0}", "{1}", ... by position. */
export function fill(template: string, args: unknown[]): string {
  let next = 0;
  return template.replace(/\{(\d*)\}/g, (_, n: string) => {
    const i = n === '' ? next++ : Number(n);
    return i < args.length ? String(args[i]) : '';
  });
}
