// The quick launcher's answers: sums, unit conversions and app matches.
// Pure functions (unit-tested); nothing here runs code or touches the network.

/** An app the launcher can start (`list_apps`). */
export interface AppEntry {
  name: string;
  path: string;
}

// ---- Calculator -------------------------------------------------------------

/**
 * Work out a sum such as "12.5 * (3 + 4) ^ 2" or "15% of 80".
 * Returns null for anything that isn't one (so plain words don't answer).
 * A small parser, never `eval`.
 */
export function calculate(input: string): number | null {
  const text = input.trim().replace(/,/g, '.').replace(/×/g, '*').replace(/÷/g, '/');
  const percentOf = /^(-?\d+(?:\.\d+)?)\s*%\s*of\s+(-?\d+(?:\.\d+)?)$/i.exec(text);
  if (percentOf) return (Number(percentOf[1]) / 100) * Number(percentOf[2]);
  // Needs an operator: a bare number is not a sum.
  if (!/^[\d\s.+\-*/^()%]+$/.test(text) || !/[+\-*/^%]/.test(text.replace(/^\s*-/, ''))) return null;
  let i = 0;
  const peek = () => text[i];
  const skip = () => {
    while (text[i] === ' ') i++;
  };
  const number = (): number => {
    skip();
    if (peek() === '(') {
      i++;
      const v = sum();
      skip();
      if (peek() !== ')') throw new Error('unclosed');
      i++;
      return v;
    }
    if (peek() === '-') {
      i++;
      return -number();
    }
    const m = /^\d+(?:\.\d+)?|^\.\d+/.exec(text.slice(i));
    if (!m) throw new Error('number');
    i += m[0].length;
    let v = Number(m[0]);
    skip();
    if (peek() === '%') {
      i++;
      v /= 100;
    }
    return v;
  };
  const power = (): number => {
    const base = number();
    skip();
    if (peek() === '^') {
      i++;
      return Math.pow(base, power());
    }
    return base;
  };
  const product = (): number => {
    let v = power();
    for (;;) {
      skip();
      const op = peek();
      if (op !== '*' && op !== '/') return v;
      i++;
      const r = power();
      v = op === '*' ? v * r : v / r;
    }
  };
  function sum(): number {
    let v = product();
    for (;;) {
      skip();
      const op = peek();
      if (op !== '+' && op !== '-') return v;
      i++;
      const r = product();
      v = op === '+' ? v + r : v - r;
    }
  }
  try {
    const v = sum();
    skip();
    return i === text.length && Number.isFinite(v) ? v : null;
  } catch {
    return null;
  }
}

/** A result for people: up to six decimals, no trailing zeros. */
export function formatNumber(v: number): string {
  if (Math.abs(v) >= 1e15 || (Math.abs(v) < 1e-6 && v !== 0)) return v.toExponential(4);
  return String(Number(v.toFixed(6)));
}

// ---- Unit conversion --------------------------------------------------------

// Each unit: its kind, and how many of the kind's base unit it is.
const UNITS: Record<string, [kind: string, factor: number]> = {};
const add = (kind: string, factor: number, ...names: string[]) => {
  for (const n of names) UNITS[n] = [kind, factor];
};
add('length', 0.001, 'mm', 'millimetre', 'millimetres', 'millimeter', 'millimeters');
add('length', 0.01, 'cm', 'centimetre', 'centimetres', 'centimeter', 'centimeters');
add('length', 1, 'm', 'metre', 'metres', 'meter', 'meters');
add('length', 1000, 'km', 'kilometre', 'kilometres', 'kilometer', 'kilometers');
add('length', 0.0254, 'in', 'inch', 'inches');
add('length', 0.3048, 'ft', 'foot', 'feet');
add('length', 0.9144, 'yd', 'yard', 'yards');
add('length', 1609.344, 'mi', 'mile', 'miles');
add('mass', 0.001, 'g', 'gram', 'grams');
add('mass', 1, 'kg', 'kilogram', 'kilograms', 'kilo', 'kilos');
add('mass', 0.028349523125, 'oz', 'ounce', 'ounces');
add('mass', 0.45359237, 'lb', 'lbs', 'pound', 'pounds');
add('mass', 1000, 't', 'tonne', 'tonnes');
add('volume', 0.001, 'ml', 'millilitre', 'millilitres', 'milliliter', 'milliliters');
add('volume', 1, 'l', 'litre', 'litres', 'liter', 'liters');
add('volume', 0.2365882365, 'cup', 'cups');
add('volume', 3.785411784, 'gal', 'gallon', 'gallons');
add('time', 1, 's', 'sec', 'secs', 'second', 'seconds');
add('time', 60, 'min', 'mins', 'minute', 'minutes');
add('time', 3600, 'h', 'hr', 'hrs', 'hour', 'hours');
add('time', 86400, 'day', 'days');
add('time', 604800, 'week', 'weeks');
add('data', 1, 'b', 'byte', 'bytes');
add('data', 1024, 'kb');
add('data', 1024 ** 2, 'mb');
add('data', 1024 ** 3, 'gb');
add('data', 1024 ** 4, 'tb');
add('speed', 1, 'm/s');
add('speed', 1 / 3.6, 'km/h', 'kph', 'kmh');
add('speed', 0.44704, 'mph');
const TEMPERATURE: Record<string, string> = {
  c: 'C', '°c': 'C', celsius: 'C',
  f: 'F', '°f': 'F', fahrenheit: 'F',
  k: 'K', kelvin: 'K',
};

function temperature(v: number, from: string, to: string): number {
  const kelvin = from === 'C' ? v + 273.15 : from === 'F' ? ((v - 32) * 5) / 9 + 273.15 : v;
  return to === 'C' ? kelvin - 273.15 : to === 'F' ? ((kelvin - 273.15) * 9) / 5 + 32 : kelvin;
}

/** "5 km in miles" → "5 km = 3.106856 miles". Null if it isn't a conversion. */
export function convert(input: string): string | null {
  const m = /^(-?\d+(?:[.,]\d+)?)\s*([a-z°/]+)\s+(?:to|in|as)\s+([a-z°/]+)$/i.exec(input.trim());
  if (!m) return null;
  const value = Number(m[1]!.replace(',', '.'));
  const [from, to] = [m[2]!.toLowerCase(), m[3]!.toLowerCase()];
  const [tf, tt] = [TEMPERATURE[from], TEMPERATURE[to]];
  if (tf && tt) return `${m[1]} °${tf} = ${formatNumber(temperature(value, tf, tt))} °${tt}`;
  const [a, b] = [UNITS[from], UNITS[to]];
  if (!a || !b || a[0] !== b[0]) return null;
  return `${m[1]} ${m[2]} = ${formatNumber((value * a[1]) / b[1])} ${m[3]}`;
}

// ---- Apps -------------------------------------------------------------------

/** Apps matching what was typed: names starting with it first, then words, then anywhere. */
export function matchApps(apps: AppEntry[], query: string, limit = 6): AppEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const score = (name: string): number => {
    const n = name.toLowerCase();
    if (n.startsWith(q)) return 0;
    if (n.split(/[\s\-_.]+/).some((w) => w.startsWith(q))) return 1;
    return n.includes(q) ? 2 : -1;
  };
  return apps
    .map((a) => ({ a, s: score(a.name) }))
    .filter((x) => x.s >= 0)
    .sort((x, y) => x.s - y.s || x.a.name.length - y.a.name.length || x.a.name.localeCompare(y.a.name))
    .slice(0, limit)
    .map((x) => x.a);
}

/** The web search for what was typed (DuckDuckGo: no account, no tracking). */
export function webSearchUrl(query: string): string {
  return `https://duckduckgo.com/?q=${encodeURIComponent(query.trim())}`;
}

/** Typed a web address? Then open it rather than search for it. */
export function asLink(query: string): string | null {
  const q = query.trim();
  if (/\s/.test(q)) return null;
  if (/^https:\/\/[^\s/]+\.[^\s]+$/i.test(q)) return q;
  return /^(?:[a-z0-9-]+\.)+[a-z]{2,}(?:\/\S*)?$/i.test(q) ? `https://${q}` : null;
}
