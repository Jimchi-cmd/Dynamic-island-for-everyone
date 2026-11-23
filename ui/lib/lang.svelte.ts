// The language in use, as reactive state: text that goes through tr() is
// shown again in the new language the moment it changes.
import TABLE from '../../locales/strings.tsv?raw';
import { column, english, fill, RTL } from './i18n';

const state = $state({ code: 'en', words: {} as Record<string, string> });

/** Show Settings in this language ("en", "fr", ...). */
export function setLanguage(code: string): void {
  if (code === state.code) return;
  state.code = code;
  state.words = code === 'en' ? {} : column(TABLE, code);
  document.documentElement.lang = code;
  document.documentElement.dir = RTL.includes(code) ? 'rtl' : 'ltr';
}

/** The language in use. */
export function language(): string {
  return state.code;
}

/** A sentence in the language in use (English when it isn't in the table). */
export function tr(key: string | undefined): string {
  if (key === undefined) return '';
  return state.words[key] ?? english(key);
}

/** A sentence with values: each "{}" filled from `args`, in order. */
export function tf(key: string, ...args: unknown[]): string {
  return fill(tr(key), args);
}
