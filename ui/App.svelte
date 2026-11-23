<script lang="ts">
  import { onMount } from 'svelte';
  import { setLanguage, tf, tr } from './lib/lang.svelte';
  import { LANGUAGES, N, resolve } from './lib/i18n';
  import Picker from './lib/Picker.svelte';
  import {
    asLink,
    calculate,
    convert,
    formatNumber,
    matchApps,
    matchWindows,
    webSearchUrl,
    type AppEntry,
    type OpenWindow,
  } from './lib/launcher';
  import * as api from './lib/bridge';
  import type { Info, Prefs, UpdateInfo } from './lib/bridge';
  import {
    ACCENTS,
    CARD_BUTTONS,
    CC_BUTTONS,
    CC_STYLES,
    ccOrder,
    moveButton,
    STYLES,
    type PlaceSuggestion,
    alarmAddArgs,
    DAY_LETTERS,
    WEEKDAY_NAMES,
    describeDays,
    formatAlarmTime,
    keyLooksValid,
    MODES,
    navStartArgs,
    parsePage,
    PERFORMANCE,
    cardShows,
    permissionsFor,
    signalBars,
    SWITCH_NAMES,
    parsePairing,
    qrPath,
    type PhonePairing,
    parseStreak,
    streakLine,
    type FocusStreak,
    formatMinutes,
    lessonAddArgs,
    parseLessons,
    WEEK_DAYS,
    type Lesson,
    type BtList,
    type IslandPrefs,
    type WifiList,
    type Status,
    type Trip,
  } from './lib/settings';

  type Page = 'general' | 'connections' | 'alarms' | 'timer' | 'navigation' | 'launcher' | 'extras' | 'updates' | 'about';
  // Each section's icon is a few strokes on a 24×24 grid, drawn for Islet,
  // on a coloured tile (`tint`).
  const PAGES: { id: Page; label: string; tint: string; icon: string; dots?: [number, number, number][] }[] = [
    {
      id: 'general',
      label: N('General'),
      tint: 'linear-gradient(160deg, #a2a2a9, #77777e)',
      icon: 'M4 8h8M18 8h2M4 16h2M12 16h8',
      dots: [
        [15, 8, 2.2],
        [9, 16, 2.2],
      ],
    },
    {
      id: 'connections',
      label: N('Wi-Fi, Bluetooth & phone'),
      tint: 'linear-gradient(160deg, #46a4ff, #0a6fe8)',
      icon: 'M4.5 10.5a10.6 10.6 0 0 1 15 0M7.8 14a6 6 0 0 1 8.4 0',
      dots: [[12, 17.6, 1.5]],
    },
    {
      id: 'alarms',
      label: N('Alarms'),
      tint: 'linear-gradient(160deg, #ffb340, #ff8a00)',
      icon: 'M6.5 16.5v-5.2a5.5 5.5 0 0 1 11 0v5.2l1.5 1.8h-14zM10.4 20.4a1.7 1.7 0 0 0 3.2 0',
    },
    {
      id: 'timer',
      label: N('Timer & focus'),
      tint: 'linear-gradient(160deg, #8a86ff, #5a55e0)',
      icon: 'M12 6.5a7 7 0 1 0 0 14a7 7 0 0 0 0-14zM12 13.5V9.8M10 3.2h4',
    },
    {
      id: 'navigation',
      label: N('Navigation'),
      tint: 'linear-gradient(160deg, #4cd97b, #1fa64d)',
      icon: 'M5.5 19.5L12 4.5l6.5 15l-6.5-4z',
    },
    {
      id: 'launcher',
      label: N('Launcher'),
      tint: 'linear-gradient(160deg, #ff7a8a, #e0355a)',
      icon: 'M10.5 4.5a6 6 0 1 0 0 12a6 6 0 0 0 0-12zM15 15l4.5 4.5',
    },
    {
      id: 'extras',
      label: N('Extras'),
      tint: 'linear-gradient(135deg, #ff9f0a, #ff375f)',
      icon: 'M12 3.5l2 6.5l6.5 2l-6.5 2l-2 6.5l-2-6.5l-6.5-2l6.5-2z',
    },
    {
      id: 'updates',
      label: N('Updates'),
      tint: 'linear-gradient(160deg, #54d3e6, #1a9fbd)',
      icon: 'M5 11.5a7 7 0 0 1 12.3-4.2L19.5 9.5M19.5 4.5v5h-5M19 12.5a7 7 0 0 1-12.3 4.2L4.5 14.5M4.5 19.5v-5h5',
    },
    {
      id: 'about',
      label: N('About'),
      tint: 'linear-gradient(160deg, #8f97a8, #5d6475)',
      icon: 'M12 4a8 8 0 1 0 0 16a8 8 0 0 0 0-16zM12 11v5.5',
      dots: [[12, 7.9, 1.2]],
    },
  ];

  let page: Page = $state('general');
  let info: Info | null = $state(null);
  let prefs: Prefs = $state({ welcomed: true, checkUpdates: false });
  let status: Status | null = $state(null);
  let problem = $state('');
  let welcomeStep = $state(0);
  let busy = $state(false);

  // Navigation form
  let key = $state('');
  let keyNote = $state('');
  let trip: Trip = $state({ label: '', destination: '', origin: '', mode: 'drive' });

  // Destination suggestions (Google Places, with the user's own key). Asked
  // for only while typing, after a pause; one session token per typing session.
  let places: PlaceSuggestion[] = $state([]);
  let placesSession = '';
  let placesTimer: ReturnType<typeof setTimeout> | undefined;

  function destinationTyped() {
    trip.placeId = undefined;
    clearTimeout(placesTimer);
    const q = trip.destination.trim();
    if (!status?.routesKey || q.length < 3 || q.startsWith('-')) {
      places = [];
      return;
    }
    if (!placesSession) placesSession = crypto.randomUUID();
    placesTimer = setTimeout(async () => {
      try {
        places = JSON.parse(await api.islet(['places', q, '--json', '--session', placesSession]));
      } catch {
        places = []; // e.g. Places API (New) isn't enabled for this key
      }
    }, 350);
  }

  function pickPlace(p: PlaceSuggestion) {
    trip.destination = p.full;
    trip.placeId = p.placeId;
    if (!trip.label.trim()) trip.label = p.main;
    places = [];
    placesSession = '';
  }
  let tripNote = $state('');

  // Wi-Fi & Bluetooth
  let wifi: WifiList | null = $state(null);
  let bt: BtList | null = $state(null);
  let joining: string | null = $state(null);
  let wifiPassword = $state('');
  let connNote = $state('');
  let scanning = $state(false);

  async function loadConnections(scan: boolean) {
    scanning = scan;
    try {
      const [w, b] = await Promise.all([
        api.islet(scan ? ['wifi', 'list', '--scan', '--json'] : ['wifi', 'list', '--json']),
        api.islet(['bt', 'list', '--json']),
      ]);
      wifi = JSON.parse(w);
      bt = JSON.parse(b);
    } catch (e) {
      connNote = api.message(e);
    } finally {
      scanning = false;
    }
  }

  async function joinWifi(ssid: string, needsPassword: boolean) {
    if (needsPassword && joining !== ssid) {
      joining = ssid;
      wifiPassword = '';
      return;
    }
    connNote = tf('Connecting to {}…', ssid);
    try {
      connNote = needsPassword
        ? await api.islet(['wifi', 'connect', ssid, '--password-stdin'], wifiPassword)
        : await api.islet(['wifi', 'connect', ssid]);
      joining = null;
      wifiPassword = '';
    } catch (e) {
      connNote = api.message(e);
      if (connNote.includes('needs a password')) joining = ssid;
    }
    await loadConnections(false);
  }

  async function toggleBt(id: string, connected: boolean) {
    if (bt && !bt.canConnect) {
      await api.openBluetoothSettings().catch((e) => (connNote = api.message(e)));
      return;
    }
    connNote = connected ? 'Disconnecting…' : 'Connecting…';
    try {
      connNote = await api.islet(['bt', connected ? 'disconnect' : 'connect', id]);
    } catch (e) {
      connNote = api.message(e);
    }
    await loadConnections(false);
  }

  $effect(() => {
    if (page === 'connections' && !wifi) void loadConnections(true);
    if (page === 'connections' && !phoneState) void loadPhone();
    if (page === 'extras' && !extrasLoaded) void loadExtras();
    if (page === 'timer') {
      void loadStreak();
      void loadLessons();
    }
    if (page === 'launcher') {
      if (!apps.length) void loadApps();
      void loadWindows();
      // Type straight away, as in any launcher.
      queueMicrotask(() => launcherInput?.focus());
    }
  });

  // Quick launcher: apps, web searches, sums and unit conversions.
  let apps = $state<AppEntry[]>([]);
  // The windows open now, read each time the launcher opens (none where the
  // desktop keeps them private).
  let openWindows = $state<OpenWindow[]>([]);
  let launcherText = $state('');
  let launcherNote = $state('');
  let launcherInput: HTMLInputElement | null = $state(null);
  type LauncherRow = { kind: 'answer' | 'window' | 'app' | 'link' | 'web'; title: string; detail: string; run?: () => Promise<void> | void };
  const launcherRows = $derived.by((): LauncherRow[] => {
    const q = launcherText.trim();
    if (!q) return [];
    const rows: LauncherRow[] = [];
    // "note: …" saves a quick note instead of searching.
    const note = /^note:\s*(.*)$/i.exec(q);
    if (note) {
      const text = note[1]!.trim();
      return [
        {
          kind: 'answer',
          title: text ? tf('Save the note “{}”', text) : tr('Type your note after “note:”'),
          detail: 'Islet notes.txt',
          run: text
            ? async () => {
                launcherNote = await api.islet(['note'], text);
              }
            : undefined,
        },
      ];
    }
    const sum = calculate(q);
    if (sum !== null) rows.push({ kind: 'answer', title: formatNumber(sum), detail: `${q} =` });
    const converted = convert(q);
    if (converted) rows.push({ kind: 'answer', title: converted.split(' = ')[1] ?? converted, detail: converted });
    for (const w of matchWindows(openWindows, q)) {
      const detail = w.title === w.app ? tr('Switch to this app') : tf('Switch to · {}', w.app);
      rows.push({ kind: 'window', title: w.title, detail, run: () => api.islet(['windows', 'focus', w.id]).then(() => {}) });
    }
    for (const a of matchApps(apps, q)) {
      rows.push({ kind: 'app', title: a.name, detail: 'App', run: () => api.launchApp(a.path) });
    }
    const link = asLink(q);
    if (link) rows.push({ kind: 'link', title: link, detail: 'Open in your browser', run: () => api.openLink(link) });
    rows.push({ kind: 'web', title: tf('Search the web for “{}”', q), detail: 'DuckDuckGo', run: () => api.openLink(webSearchUrl(q)) });
    return rows;
  });

  async function loadApps() {
    try {
      apps = await api.listApps();
    } catch (e) {
      launcherNote = api.message(e);
    }
  }

  // The speed test: run only when its button is pressed.
  type Speed = { latencyMs: number; downMbps: number; upMbps: number; bytesUsed: number };
  let speed = $state<Speed | null>(null);
  let speedRunning = $state(false);
  let speedNote = $state('');
  const formatMbps = (v: number) => (v < 100 ? v.toFixed(1) : Math.round(v).toString());

  async function runSpeedTest() {
    speedRunning = true;
    speedNote = '';
    try {
      speed = JSON.parse(await api.islet(['speedtest', '--json']));
    } catch (e) {
      speedNote = api.message(e);
    } finally {
      speedRunning = false;
    }
  }

  // Plugin widgets found in the plugins folder.
  type PluginInfo = { folder: string; name?: string; every?: number; error?: string };
  let plugins = $state<{ on: boolean; folder: string | null; plugins: PluginInfo[] } | null>(null);

  async function loadPlugins() {
    try {
      plugins = JSON.parse(await api.islet(['plugins', '--json']));
    } catch {
      plugins = null;
    }
  }

  async function reloadPlugins() {
    try {
      await api.islet(['plugins', 'reload']);
    } catch {
      // Nothing to reload while the island isn't running.
    }
    await loadPlugins();
  }

  async function loadWindows() {
    try {
      openWindows = JSON.parse(await api.islet(['windows', '--json']));
    } catch {
      openWindows = [];
    }
  }

  async function runLauncherRow(row: LauncherRow | undefined) {
    if (!row?.run) return;
    try {
      launcherNote = '';
      await row.run();
      launcherText = '';
    } catch (e) {
      launcherNote = api.message(e);
    }
  }

  // The island's own preferences (island.json via the CLI)
  let islandPrefs: IslandPrefs | null = $state(null);

  let displays = $state<string[]>([]);

  // Fonts installed on this computer, for the island's font (read once).
  let fonts = $state<string[]>([]);
  async function loadFonts() {
    try {
      fonts = JSON.parse(await api.islet(['fonts', '--json']));
    } catch {
      fonts = [];
    }
  }

  async function loadIslandPrefs() {
    try {
      islandPrefs = JSON.parse(await api.islet(['prefs', '--json']));
      setLanguage(resolve(islandPrefs?.language ?? 'auto', navigator.language));
      displays = JSON.parse(await api.islet(['displays', '--json']));
      void loadPlugins();
      if (!fonts.length) void loadFonts();
    } catch (e) {
      problem = api.message(e);
    }
  }

  async function setSwitch(key: keyof IslandPrefs, on: boolean) {
    const name = SWITCH_NAMES[key];
    if (!name) return;
    try {
      await api.islet(['set', name, on ? 'on' : 'off']);
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  // Global shortcut: press the keys in the field to record them.
  let hotkeyDraft = $state('');

  // The launcher's and the quick note's shortcuts, recorded the same way.
  let extraDraft = $state<Record<'launcher' | 'note', string>>({ launcher: '', note: '' });

  function comboOf(e: KeyboardEvent): string {
    if (e.key === 'Tab') return '';
    e.preventDefault();
    if (['Control', 'Alt', 'Shift', 'Meta', 'OS'].includes(e.key)) return '';
    let key = '';
    if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
    else if (/^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
    else if (e.code === 'Space') key = 'Space';
    else if (/^F([1-9]|1[0-2])$/.test(e.code)) key = e.code;
    if (!key) return '';
    const parts = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Super');
    parts.push(key);
    return parts.join('+');
  }

  async function saveExtraHotkey(which: 'launcher' | 'note', combo: string) {
    try {
      await api.islet(['hotkey', which, combo || 'off']);
      extraDraft[which] = '';
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  function captureHotkey(e: KeyboardEvent) {
    if (e.key === 'Tab') return;
    e.preventDefault();
    if (['Control', 'Alt', 'Shift', 'Meta', 'OS'].includes(e.key)) return;
    let key = '';
    if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
    else if (/^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
    else if (e.code === 'Space') key = 'Space';
    else if (/^F([1-9]|1[0-2])$/.test(e.code)) key = e.code;
    if (!key) return;
    const parts = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.altKey) parts.push('Alt');
    if (e.shiftKey) parts.push('Shift');
    if (e.metaKey) parts.push('Super');
    parts.push(key);
    hotkeyDraft = parts.join('+');
  }

  async function setCardButton(id: string, show: boolean) {
    try {
      await api.islet(['card', show ? 'show' : 'hide', id]);
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  async function displaySize(display: string, size: string) {
    try {
      await api.islet(['look', 'size', size, '--display', display]);
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  async function setLook(
    what:
      | 'accent'
      | 'style'
      | 'shape'
      | 'clock'
      | 'hours'
      | 'size'
      | 'position'
      | 'offset'
      | 'display'
      | 'font'
      | 'tint'
      | 'control-centre',
    value: string,
  ) {
    try {
      await api.islet(['look', what, value]);
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  // Control Centre: a photo behind it (sent on stdin, never as a path) and its buttons' order.
  let photoPreview = $state('');
  async function choosePhoto(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    if (file.size > 25 * 1024 * 1024) {
      problem = tr('That photo is too big (25 MB at most).');
      return;
    }
    try {
      const url = await new Promise<string>((ok, fail) => {
        const r = new FileReader();
        r.onload = () => ok(String(r.result));
        r.onerror = () => fail(r.error);
        r.readAsDataURL(file);
      });
      await api.islet(['look', 'photo', '-'], url);
      photoPreview = url;
      await loadIslandPrefs();
    } catch (err) {
      problem = api.message(err);
    }
  }
  async function removePhoto() {
    try {
      await api.islet(['look', 'photo', 'none']);
      photoPreview = '';
      await loadIslandPrefs();
    } catch (err) {
      problem = api.message(err);
    }
  }
  async function moveCc(id: string, by: -1 | 1) {
    try {
      await api.islet(['card', 'order', moveButton(ccOrder(islandPrefs?.ccOrder), id, by).join(',')]);
      await loadIslandPrefs();
    } catch (err) {
      problem = api.message(err);
    }
  }
  async function resetCcOrder() {
    try {
      await api.islet(['card', 'order', 'default']);
      await loadIslandPrefs();
    } catch (err) {
      problem = api.message(err);
    }
  }

  async function saveHotkey(combo: string) {
    try {
      await api.islet(['hotkey', combo || 'off']);
      hotkeyDraft = '';
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  // Apps the island steps aside for.
  let hideDraft = $state('');
  async function hideWith(verb: 'add' | 'remove', app: string) {
    const name = app.trim();
    if (!name || name.startsWith('-')) return;
    try {
      await api.islet(['rules', 'hide', verb, name]);
      hideDraft = '';
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  // Clipboard history: apps whose copies are never kept.
  let excludeDraft = $state('');
  async function clipExclude(verb: 'add' | 'remove', app: string) {
    const name = app.trim();
    if (!name || name.startsWith('-')) return;
    try {
      await api.islet(['clipboard', 'exclude', verb, name]);
      excludeDraft = '';
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  // Now Playing
  let hideApp = $state('');

  async function mediaVisibility(hide: boolean, app: string) {
    if (!app.trim() || app.trim().startsWith('-')) return;
    try {
      await api.islet(['media', hide ? 'hide' : 'show', app.trim()]);
      hideApp = '';
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  // Spotify
  let spotifyClient = $state('');
  let spotifyNote = $state('');

  async function spotifyLogin() {
    spotifyNote = tr('Finish signing in in your browser…');
    try {
      spotifyNote = await api.islet(['spotify', 'login', spotifyClient.trim()]);
      spotifyClient = '';
      await refresh();
      await loadIslandPrefs();
    } catch (e) {
      spotifyNote = api.message(e);
    }
  }

  async function spotifyCmd(verb: 'logout' | 'on' | 'off') {
    try {
      spotifyNote = await api.islet(['spotify', verb]);
      await refresh();
      await loadIslandPrefs();
    } catch (e) {
      spotifyNote = api.message(e);
    }
  }

  // Weather
  let cityQuery = $state('');
  let cityResults: { name: string; lat: number; lon: number }[] = $state([]);
  let weatherNote = $state('');

  async function searchCity() {
    weatherNote = tr('Searching…');
    try {
      cityResults = JSON.parse(await api.islet(['weather', 'search', cityQuery.trim(), '--json']));
      weatherNote = cityResults.length ? '' : tr('No places found.');
    } catch (e) {
      weatherNote = api.message(e);
    }
  }

  async function chooseCity(p: { name: string; lat: number; lon: number }) {
    try {
      weatherNote = await api.islet(['weather', 'set', '--lat', String(p.lat), '--lon', String(p.lon), '--name', p.name]);
      cityResults = [];
      await loadIslandPrefs();
    } catch (e) {
      weatherNote = api.message(e);
    }
  }

  async function weatherOff() {
    try {
      weatherNote = await api.islet(['weather', 'off']);
      await loadIslandPrefs();
    } catch (e) {
      weatherNote = api.message(e);
    }
  }

  // Calendar
  let calLink = $state('');
  let calNote = $state('');

  async function saveCalendar() {
    try {
      calNote = await api.islet(['calendar', 'set'], calLink.trim());
      calLink = '';
      await loadIslandPrefs();
    } catch (e) {
      calNote = api.message(e);
    }
  }

  async function calendarCmd(verb: 'on' | 'off' | 'clear') {
    try {
      calNote = await api.islet(['calendar', verb]);
      await loadIslandPrefs();
    } catch (e) {
      calNote = api.message(e);
    }
  }

  // Timer & focus
  let timerMinutes = $state(10);
  let timerLabel = $state('');
  let focusWork = $state(25);
  let focusBreak = $state(5);
  let focusRounds = $state(4);

  // Days in a row with a finished focus round (`islet focus streak --json`).
  let streak = $state<FocusStreak | null>(null);
  async function loadStreak() {
    try {
      streak = parseStreak(await api.islet(['focus', 'streak', '--json']));
    } catch {
      streak = null;
    }
  }
  let timerNote = $state('');

  // Class timetable (`islet class …`)
  let lessons = $state<Lesson[]>([]);
  let lessonDay = $state(0);
  let lessonStart = $state('09:00');
  let lessonEnd = $state('10:00');
  let lessonName = $state('');
  let lessonRoom = $state('');
  let lessonNote = $state('');
  async function loadLessons() {
    try {
      lessons = parseLessons(await api.islet(['class', '--json']));
    } catch (e) {
      lessonNote = api.message(e);
    }
  }

  async function addLesson() {
    const args = lessonAddArgs(lessonDay, lessonStart, lessonEnd, lessonName, lessonRoom);
    if (!Array.isArray(args)) {
      lessonNote = args.error;
      return;
    }
    try {
      lessonNote = await api.islet(args);
      lessonName = '';
      lessonRoom = '';
      // The next lesson usually follows this one.
      lessonStart = lessonEnd;
      const [h = 9, m = 0] = lessonEnd.split(':').map(Number);
      lessonEnd = `${String(Math.min(h + 1, 23)).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
    } catch (e) {
      lessonNote = api.message(e);
    }
    await loadLessons();
  }

  async function removeLesson(index: number) {
    try {
      await api.islet(['class', 'remove', String(index + 1)]);
      lessonNote = '';
    } catch (e) {
      lessonNote = api.message(e);
    }
    await loadLessons();
  }

  async function timerCmd(args: string[]) {
    try {
      timerNote = await api.islet(args);
    } catch (e) {
      timerNote = api.message(e);
    }
  }

  function startTimer(minutes: number) {
    const args = ['timer', 'start', `${minutes}m`];
    if (timerLabel.trim() && !timerLabel.trim().startsWith('-')) args.push('--label', timerLabel.trim());
    void timerCmd(args);
  }

  // Alarms
  let alarmTime = $state('07:00');
  let alarmLabel = $state('');
  let alarmDays = $state(0);
  let alarmNote = $state('');
  // 12/24-hour display follows the system locale.
  const h24 = !new Intl.DateTimeFormat(undefined, { hour: 'numeric' }).resolvedOptions().hour12;

  async function addAlarm() {
    const args = alarmAddArgs(alarmTime, alarmLabel, alarmDays);
    if ('error' in args) {
      alarmNote = args.error;
      return;
    }
    try {
      alarmNote = await api.islet(args);
      alarmLabel = '';
      await refresh();
    } catch (e) {
      alarmNote = api.message(e);
    }
  }

  async function alarmCmd(verb: 'on' | 'off' | 'remove', id: number) {
    try {
      await api.islet(['alarm', verb, String(id)]);
      await refresh();
    } catch (e) {
      alarmNote = api.message(e);
    }
  }

  // Updates
  let update: UpdateInfo | null = $state(null);
  let updateNote = $state('');

  const docs = (path: string) => `${info?.repo ?? ''}/blob/main/${path}`;

  async function refresh() {
    try {
      status = await api.status();
      problem = '';
    } catch (e) {
      problem = api.message(e);
    }
  }

  async function run<T>(f: () => Promise<T>): Promise<T | undefined> {
    busy = true;
    try {
      return await f();
    } catch (e) {
      problem = api.message(e);
    } finally {
      busy = false;
    }
  }

  async function startIsland() {
    await run(async () => {
      await api.startIsland();
      await new Promise((r) => setTimeout(r, 600));
      await refresh();
    });
  }

  async function setAutostart(on: boolean) {
    await run(async () => {
      await api.islet(['autostart', on ? 'on' : 'off']);
      await refresh();
    });
  }

  // Extras: performance, world clocks, notes, to-dos and shortcuts.
  let extrasLoaded = $state(false);
  let extrasNote = $state('');

  // World clocks
  let worldClocks: string[] = $state([]);
  let cities: string[] = $state([]);

  async function loadWorld() {
    try {
      worldClocks = JSON.parse(await api.islet(['world', '--json']));
      if (!cities.length) cities = JSON.parse(await api.islet(['world', 'cities', '--json']));
    } catch (e) {
      extrasNote = api.message(e);
    }
  }

  async function worldChange(add: boolean, city: string) {
    if (!city) return;
    extrasNote = '';
    try {
      await api.islet(['world', add ? 'add' : 'remove', city]);
    } catch (e) {
      extrasNote = api.message(e);
    }
    await loadWorld();
  }

  // Notes and to-dos
  let todos: { id: number; text: string; done: boolean }[] = $state([]);
  let todoText = $state('');

  // The Android companion: pairing shows a QR code the phone scans.
  let pairing = $state<PhonePairing | null>(null);
  let phoneState = $state<{ paired: boolean; on: boolean } | null>(null);
  let phoneNote = $state('');
  async function loadPhone() {
    try {
      const v = JSON.parse(await api.islet(['phone', 'status', '--json']));
      phoneState = { paired: v.paired === true, on: v.on === true };
      if (phoneState.paired) await loadQuickReplies();
    } catch {
      phoneState = null;
    }
  }
  // The phone and the computer together: ring, send, reply, quick replies.
  let phoneSendText = $state('');
  let phoneReplyText = $state('');
  let phoneSayNote = $state('');
  let quickReplies: string[] = $state(['', '', '', '']);
  async function loadQuickReplies() {
    try {
      const lines = (await api.islet(['phone', 'replies'])).split('\n');
      quickReplies = [0, 1, 2, 3].map((i) => lines[i] ?? '');
    } catch {
      // The island's command line isn't there: keep the boxes empty.
    }
  }
  async function phoneSay(args: string[], input?: string, then?: () => void) {
    phoneSayNote = '';
    try {
      phoneSayNote = await api.islet(args, input);
      then?.();
    } catch (e) {
      phoneSayNote = api.message(e);
    }
  }

  async function phoneRun(args: string[]) {
    phoneNote = '';
    try {
      const out = await api.islet(args);
      if (args[1] === 'pair') {
        pairing = parsePairing(out);
        if (!pairing) phoneNote = tr('The pairing code could not be drawn.');
      } else {
        pairing = null;
        phoneNote = out;
      }
    } catch (e) {
      phoneNote = api.message(e);
    }
    await loadPhone();
  }

  // Quick notes: one line into a plain file (`islet note`).
  let quickNote = $state('');
  let quickNoteSaid = $state('');
  let notesPath = $state('');
  async function loadNotesPath() {
    try {
      notesPath = (await api.islet(['note', '--where'])).trim();
    } catch {
      notesPath = '';
    }
  }
  async function saveQuickNote() {
    try {
      quickNoteSaid = await api.islet(['note'], quickNote.trim());
      quickNote = '';
    } catch (e) {
      quickNoteSaid = api.message(e);
    }
  }

  async function loadTodos() {
    try {
      todos = JSON.parse(await api.islet(['todo', '--json']));
    } catch (e) {
      extrasNote = api.message(e);
    }
  }

  async function todoRun(args: string[], input?: string) {
    extrasNote = '';
    try {
      await api.islet(args, input);
      todoText = '';
    } catch (e) {
      extrasNote = api.message(e);
    }
    await loadTodos();
  }

  // Your own buttons
  let shortcuts: { name: string; target: string }[] = $state([]);
  let shortcutName = $state('');
  let shortcutTarget = $state('');

  async function loadShortcuts() {
    try {
      shortcuts = JSON.parse(await api.islet(['shortcut', '--json']));
    } catch (e) {
      extrasNote = api.message(e);
    }
  }

  async function shortcutRun(args: string[]) {
    extrasNote = '';
    try {
      await api.islet(args);
      shortcutName = '';
      shortcutTarget = '';
    } catch (e) {
      extrasNote = api.message(e);
    }
    await loadShortcuts();
  }

  async function loadExtras() {
    extrasLoaded = true;
    void loadWorld();
    void loadTodos();
    void loadNotesPath();
    void loadShortcuts();
  }

  async function setLanguagePref(code: string) {
    try {
      await api.islet(['language', code]);
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  async function setPerformance(mode: string) {
    try {
      await api.islet(['performance', mode]);
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
    }
  }

  async function saveKey() {
    keyNote = '';
    try {
      await api.islet(['key', 'routes'], key.trim());
      key = '';
      keyNote = tr('Saved to your keychain.');
      await refresh();
    } catch (e) {
      keyNote = api.message(e);
    }
  }

  async function clearKey() {
    keyNote = '';
    try {
      await api.islet(['key', 'routes', '--clear']);
      keyNote = tr('Removed.');
      await refresh();
    } catch (e) {
      keyNote = api.message(e);
    }
  }

  async function startTrip() {
    const args = navStartArgs(trip);
    if ('error' in args) {
      tripNote = args.error;
      return;
    }
    try {
      tripNote = await api.islet(args);
      await refresh();
    } catch (e) {
      tripNote = api.message(e);
    }
  }

  async function stopTrip() {
    try {
      await api.islet(['nav', 'stop']);
      tripNote = tr('Stopped.');
      await refresh();
    } catch (e) {
      tripNote = api.message(e);
    }
  }

  async function checkUpdate() {
    updateNote = tr('Checking…');
    try {
      update = await api.checkUpdate();
      updateNote = update.newer ? '' : tf('Islet {} is up to date.', update.current);
      void updatePill();
    } catch (e) {
      update = null;
      updateNote = api.message(e);
    }
  }

  // Early builds: the newest release of all, pre-releases included.
  let early = $state<{ current: string; latest: string; newer: boolean; url: string } | null>(null);
  let earlyNote = $state('');
  async function checkEarly() {
    earlyNote = tr('Looking…');
    try {
      const v = JSON.parse(await api.islet(['update', 'check', '--early', '--json']));
      const url = typeof v.url === 'string' && v.url.startsWith('https://') ? v.url : '';
      early = { current: String(v.current), latest: String(v.latest), newer: v.newer === true && url !== '', url };
      earlyNote = early.newer ? tf('Islet {} is out as an early build.', early.latest) : tr('No early build newer than yours.');
    } catch (e) {
      early = null;
      earlyNote = api.message(e);
    }
  }

  // Tell the island too: a pill that opens the download page (and goes
  // once Islet is up to date).
  async function updatePill() {
    const link = update?.url && update.url.startsWith('https://') ? update.url : `${info?.repo}/releases/latest`;
    try {
      if (update?.newer && link.startsWith('https://')) {
        await api.islet([
          'activity', 'start', 'islet-update',
          '--title', tf('Islet {} is available', update.latest),
          '--subtitle', tr('Tap to open the download page'),
          // A full ring: an activity without a value would spin (and cost power).
          '--progress', '100',
          '--open', link,
        ]);
      } else {
        await api.islet(['activity', 'end', 'islet-update']);
      }
    } catch {
      // The island isn't running: nothing to show it on.
    }
  }

  async function installUpdate() {
    updateNote = tr('Downloading and installing…');
    try {
      await api.installUpdate();
    } catch (e) {
      updateNote = api.message(e);
    }
  }

  async function savePrefs() {
    await run(() => api.setPrefs(prefs));
  }

  async function finishWelcome(autostart: boolean) {
    await run(async () => {
      if (autostart) await api.islet(['autostart', 'on']);
      if (!status?.running) await api.startIsland();
      prefs.welcomed = true;
      await api.setPrefs(prefs);
      await new Promise((r) => setTimeout(r, 600));
      await refresh();
    });
  }

  onMount(async () => {
    // The system's language until the island's setting is read.
    setLanguage(resolve('auto', navigator.language));
    try {
      [info, prefs] = await Promise.all([api.info(), api.getPrefs()]);
    } catch (e) {
      problem = api.message(e);
    }
    await refresh();
    await loadIslandPrefs();
    const wanted = parsePage(await api.initialPage().catch(() => null));
    if (wanted?.page === 'launcher') page = 'launcher';
    // The quick-note shortcut: the launcher, ready to save a note.
    if (wanted?.page === 'note') {
      page = 'launcher';
      launcherText = 'note: ';
    }
    if (wanted?.page === 'connections') {
      page = 'connections';
      joining = wanted.ssid;
    }
    if (status?.trip) trip = { ...status.trip };
    // Opening Settings starts the island if it isn't running (after setup).
    if (prefs.welcomed && status && !status.running) await startIsland();
    if (prefs.checkUpdates) await checkUpdate();
  });
</script>

<!-- Celsius or Fahrenheit, on the Look and Weather pages. -->
{#snippet units()}
  <div class="segmented" role="radiogroup" aria-label={tr("Temperature unit")}>
    {#each [{ f: false, short: '°C', long: N('Celsius') }, { f: true, short: '°F', long: N('Fahrenheit') }] as u (u.short)}
      {@const on = (islandPrefs?.fahrenheit ?? false) === u.f}
      <button
        role="radio"
        aria-checked={on}
        aria-label={tr(u.long)}
        title={tr(u.long)}
        class:chosen={on}
        disabled={!islandPrefs}
        onclick={() => setSwitch('fahrenheit', u.f)}>{u.short}</button
      >
    {/each}
  </div>
{/snippet}

{#if info && !prefs.welcomed}
  <main class="welcome" aria-live="polite">
    {#if welcomeStep === 0}
      <h1>{tr("Welcome to Islet")}</h1>
      <p>
        {tr("Islet adds a Dynamic Island-style pill to the top of your screen. It shows what’s playing, the battery of headphones and mice when they connect, and your ETA when you’re heading somewhere.")}
      </p>
      <div class="card">
        <h2>{tr("Private by default")}</h2>
        <p class="muted">
          {tr("No accounts, no analytics, no telemetry. Islet uses the network only for features you turn on: navigation ETA (with your own Google key), weather, lyrics and update checks.")}
        </p>
      </div>
      <div class="actions"><button class="primary" onclick={() => (welcomeStep = 1)}>{tr("Continue")}</button></div>
    {:else if welcomeStep === 1}
      <h1>{tr("What Islet may ask for")}</h1>
      {#each permissionsFor(info.os) as p (p.title)}
        <div class="card">
          <h2>{tr(p.title)}</h2>
          <p class="muted">{tr(p.body)}</p>
        </div>
      {/each}
      <div class="actions">
        <button onclick={() => (welcomeStep = 0)}>{tr("Back")}</button>
        <button class="primary" onclick={() => (welcomeStep = 2)}>{tr("Continue")}</button>
      </div>
    {:else}
      <h1>{tr("Start Islet")}</h1>
      <p>{tr("Islet uses a few megabytes of memory and no CPU while nothing is happening, so it can run all the time.")}</p>
      <div class="actions">
        <button disabled={busy} onclick={() => finishWelcome(false)}>{tr("Start, just this once")}</button>
        <button class="primary" disabled={busy} onclick={() => finishWelcome(true)}>{tr("Start now and at login")}</button>
      </div>
      {#if problem}<p class="error" role="alert">{problem}</p>{/if}
    {/if}
    <p class="steps muted" aria-label="Step {welcomeStep + 1} of 3">
      {#each [0, 1, 2] as i (i)}<span class:on={i === welcomeStep}>●</span>{/each}
    </p>
  </main>
{:else}
  <div class="layout">
    <nav aria-label={tr("Settings sections")}>
      <div class="brand">
        <span class="logo" aria-hidden="true"></span>
        <span>Islet</span>
      </div>
      {#each PAGES as p (p.id)}
        <button class="tab" class:active={page === p.id} aria-current={page === p.id} onclick={() => (page = p.id)}>
          <span class="tile" style:background={p.tint} aria-hidden="true">
            <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round">
              <path d={p.icon} />
              {#each p.dots ?? [] as [cx, cy, r] (`${cx},${cy}`)}<circle {cx} {cy} {r} fill="currentColor" stroke="none" />{/each}
            </svg>
          </span>
          {tr(p.label)}
        </button>
      {/each}
      <div class="grow"></div>
      {#if info}<div class="version muted">{tr("Version")} {info.version}</div>{/if}
    </nav>
    <main>
      {#if problem}<p class="error" role="alert">{problem}</p>{/if}

      {#if page === 'general'}
        <h1>{tr("General")}</h1>
        <div class="card row">
          <div>
            <h2>{tr("Language")}</h2>
            <p class="muted">{tr("Islet, the island and these settings, in your language.")}</p>
          </div>
          <Picker
            label={tr("Language")}
            value={islandPrefs?.language ?? 'auto'}
            disabled={!islandPrefs}
            options={LANGUAGES.map((l) => ({ value: l.code, label: l.code === 'auto' ? tr(l.name) : l.name }))}
            onchange={(v) => setLanguagePref(v)}
          />
        </div>
        <div class="card row">
          <div>
            <h2>{tr("Island")}</h2>
            {#if status?.running}<p class="ok">{tr("Running")}</p>{:else}<p class="muted">{tr("Not running")}</p>{/if}
          </div>
          {#if status?.running}
            <button disabled={busy} onclick={() => run(async () => { await api.islet(['quit']); await refresh(); })}>
              {tr("Quit Islet")}
            </button>
          {:else}
            <button class="primary" disabled={busy} onclick={startIsland}>{tr("Start Islet")}</button>
          {/if}
        </div>
        <div class="card row">
          <div>
            <h2>{tr("Start at login")}</h2>
            <p class="muted">{tr("Islet opens by itself when you log in.")}</p>
          </div>
          <input
            type="checkbox"
            class="switch"
            aria-label={tr("Start at login")}
            checked={status?.autostart ?? false}
            disabled={busy || !status}
            onchange={(e) => setAutostart(e.currentTarget.checked)}
          />
        </div>
        <div class="card">
          <h2>{tr("Pop-ups")}</h2>
          <p class="muted small">{tr("The island shows a moment's notice when these change.")}</p>
          {#each [['volumeHud', N('Volume')], ['brightnessHud', N('Screen brightness')], ['capsLockHud', N('Caps Lock')], ['wifiPopup', N('Joining a Wi-Fi network')], ['modeIndicators', N('Airplane mode and Do Not Disturb')]] as [key, label] (key)}
            <div class="net">
              <span class="name">{tr(label)}</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="{tr(label)} pop-up"
                checked={islandPrefs ? Boolean(islandPrefs[key as keyof IslandPrefs]) : true}
                disabled={!islandPrefs}
                onchange={(e) => setSwitch(key as keyof IslandPrefs, e.currentTarget.checked)}
              />
            </div>
          {/each}
          {#if info?.os === 'macos'}
            <p class="muted small">{tr("macOS doesn't let apps see brightness changes, so there's no brightness pop-up on a Mac.")}</p>
          {:else if info?.os === 'windows'}
            <p class="muted small">
              {tr("On Windows the Caps Lock pop-up uses a keyboard hook that checks only the Caps Lock key. It's removed when you turn this off.")}
            </p>
          {/if}
          <p class="muted small">
            {tr("Airplane mode and Do Not Disturb also show as small icons on the empty island while they're on.")}
            {#if info?.os === 'macos'}{tr("Macs have no airplane mode, and macOS shares Focus only with App Store apps, so neither shows on a Mac.")}{:else if info?.os === 'windows'}{tr("Do Not Disturb follows Windows 11's Focus sessions.")}{/if}
          </p>
        </div>
        <div class="card">
          <h2>{tr("Now Playing")}</h2>
          <div class="net">
            <span class="name">{tr("Show synced lyrics")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Show synced lyrics")}
              checked={islandPrefs?.lyrics ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('lyrics', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            {tr("The line being sung lights up in the island as the song plays. Lyrics come from LRCLIB (lrclib.net), a free, open lyrics database: when this is on, Islet sends it the title, artist, album and length of the song that's playing, and nothing else. Songs without timed lyrics show none.")}
          </p>
          {#if info?.os === 'linux'}
            <div class="net">
              <span class="name">{tr("Download web artwork (e.g. Spotify covers)")}</span>
              <input
                type="checkbox"
                class="switch"
                aria-label={tr("Download web artwork")}
                checked={islandPrefs?.webArtwork ?? false}
                disabled={!islandPrefs}
                onchange={(e) => setSwitch('webArtwork', e.currentTarget.checked)}
              />
            </div>
          {/if}
          <p class="muted small">{tr("Hide music and videos from these apps:")}</p>
          {#each islandPrefs?.mediaExclude ?? [] as app (app)}
            <div class="net">
              <span class="name">{app}</span>
              <button onclick={() => mediaVisibility(false, app)}>{tr("Show again")}</button>
            </div>
          {/each}
          <form class="join" onsubmit={(e) => { e.preventDefault(); void mediaVisibility(true, hideApp); }}>
            <input placeholder={tr("App name, e.g. Firefox")} bind:value={hideApp} aria-label={tr("App to hide")} />
            <button type="submit" disabled={!hideApp.trim()}>{tr("Hide")}</button>
          </form>
        </div>
        <div class="card">
          <h2>{tr("Look")}</h2>
          <div class="net">
            <span class="name">{tr("Show the island only when I point at it")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Show the island only when I point at it")}
              checked={islandPrefs?.autoHide ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('autoHide', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            {tr("The empty island stays hidden until you move the pointer to the top of the screen. Music, timers, calls for attention and other live activities always show.")}
          </p>
          <p class="muted small">
            {tr("Step aside while one of these apps is in front, as over fullscreen videos (point at the island to bring it back):")}
          </p>
          <div class="actions left">
            {#each islandPrefs?.hideWith ?? [] as app (app)}
              <button onclick={() => hideWith('remove', app)} aria-label={tf('Show the island over {} again', app)}>{app} ✕</button>
            {/each}
          </div>
          <form
            class="join"
            onsubmit={(e) => {
              e.preventDefault();
              void hideWith('add', hideDraft);
            }}
          >
            <input placeholder={tr("An app's name, e.g. Steam")} maxlength="60" bind:value={hideDraft} aria-label={tr("App to step aside for")} />
            <button type="submit" disabled={!hideDraft.trim()}>{tr("Add")}</button>
          </form>
          <div class="net">
            <span class="name">{tr("Show the time when nothing is playing")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Show the time when nothing is playing")}
              checked={islandPrefs?.idleClock ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('idleClock', e.currentTarget.checked)}
            />
          </div>
          <div class="net">
            <span class="name">{tr("Tell me when a program slows the computer down")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Tell me when a program slows the computer down")}
              checked={islandPrefs?.performanceAlert ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('performanceAlert', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            {tr("When one program has had more than half the processor for a minute, or 40% of the memory, the island says so and offers to close it. Islet looks at the running programs every 20 seconds while this is on.")}
          </p>
          <div class="net">
            <span class="name">{tr("Show weather, battery and processor when you point at it")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Show weather, battery and processor when you point at the island")}
              checked={islandPrefs?.hoverStatus ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('hoverStatus', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">{tr("Accent colour for buttons and switches on the island.")}</p>
          <div class="swatches" role="radiogroup" aria-label={tr("Accent colour")}>
            {#each Object.entries(ACCENTS) as [name, hex] (name)}
              <button
                class="swatch"
                class:chosen={(islandPrefs?.accent ?? 'blue') === name}
                style="background: {hex}"
                role="radio"
                aria-checked={(islandPrefs?.accent ?? 'blue') === name}
                aria-label={name}
                title={name}
                disabled={!islandPrefs}
                onclick={() => setLook('accent', name)}
              ></button>
            {/each}
          </div>
          <p class="muted small">{tr("Buttons on the card that opens when you click the island. Wi-Fi and Bluetooth always show.")}</p>
          {#each CARD_BUTTONS as b (b.id)}
            <div class="net">
              <span class="name">{tr(b.label)}</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="Show {tr(b.label)} on the island's card"
                checked={cardShows(b, islandPrefs)}
                disabled={!islandPrefs}
                onchange={(e) => setCardButton(b.id, e.currentTarget.checked)}
              />
            </div>
          {/each}
          <p class="muted small">
            {tr("The order of Control Centre's buttons. Every layout places them in this order; when a layout has no room for them all, the ones at the end are left out.")}
          </p>
          <ol class="cc-order" aria-label={tr("Control Centre's buttons, in order")}>
            {#each ccOrder(islandPrefs?.ccOrder) as id, i (id)}
              {@const b = CC_BUTTONS.find((x) => x.id === id)}
              {@const card = CARD_BUTTONS.find((x) => x.id === id)}
              {@const off = card ? !cardShows(card, islandPrefs) : false}
              <li class:off>
                <span class="pos">{i + 1}</span>
                <span class="name">{tr(b?.label)}{#if off}<span class="muted">&nbsp;· {tr("hidden")}</span>{/if}</span>
                <button
                  class="icon-btn"
                  aria-label={tf('Move {} up', tr(b?.label))}
                  disabled={!islandPrefs || i === 0}
                  onclick={() => moveCc(id, -1)}>↑</button
                >
                <button
                  class="icon-btn"
                  aria-label={tf('Move {} down', tr(b?.label))}
                  disabled={!islandPrefs || i === CC_BUTTONS.length - 1}
                  onclick={() => moveCc(id, 1)}>↓</button
                >
              </li>
            {/each}
          </ol>
          <div class="row-end">
            <button disabled={!islandPrefs || !(islandPrefs?.ccOrder ?? []).length} onclick={resetCcOrder}
              >{tr("Usual order")}</button
            >
          </div>
          <p class="muted small">{tr("What the island is made of.")}</p>
          <div class="styles" role="radiogroup" aria-label={tr("Island style")}>
            {#each STYLES as s (s.id)}
              {@const chosen = (islandPrefs?.style ?? 'black') === s.id}
              {@const accent = islandPrefs?.tint || (ACCENTS[islandPrefs?.accent ?? 'blue'] ?? ACCENTS.blue)}
              <button
                class="style-tile"
                class:chosen
                role="radio"
                aria-checked={chosen}
                aria-label={tr(s.label)}
                disabled={!islandPrefs}
                onclick={() => setLook('style', s.id)}
              >
                <span class="preview" style:--tint={accent}>
                  <span class="mini {s.id}"></span>
                </span>
                <span class="style-name">{tr(s.label)}</span>
              </button>
            {/each}
          </div>
          {#if (islandPrefs?.style ?? 'black').startsWith('tinted')}
            {@const accentHex = ACCENTS[islandPrefs?.accent ?? 'blue'] ?? ACCENTS.blue}
            <div class="net">
              <span class="name">{tr("Tint colour")}</span>
              <span class="tint-pick">
                <input
                  type="color"
                  aria-label={tr("Tint colour")}
                  value={islandPrefs?.tint || accentHex}
                  disabled={!islandPrefs}
                  onchange={(e) => setLook('tint', e.currentTarget.value)}
                />
                <button disabled={!islandPrefs || !islandPrefs?.tint} onclick={() => setLook('tint', 'accent')}
                  >{tr("Same as accent")}</button
                >
              </span>
            </div>
          {/if}
          <p class="muted small">{tr("Control Centre's layout (the card that opens when you click the island).")}</p>
          <div class="styles" role="radiogroup" aria-label={tr("Control Centre layout")}>
            {#each CC_STYLES as c (c.id)}
              {@const chosen = (islandPrefs?.ccStyle ?? 'classic') === c.id}
              <button
                class="style-tile"
                class:chosen
                role="radio"
                aria-checked={chosen}
                aria-label={tr(c.label)}
                title={tr(c.hint)}
                disabled={!islandPrefs}
                onclick={() => setLook('control-centre', c.id)}
              >
                <span class="preview cc-preview">
                  <svg viewBox="0 0 60 34" aria-hidden="true">
                    <rect x="1" y="1" width="58" height="32" rx="9" fill={c.id === 'dock' ? '#1d2550' : '#000'} />
                    {#if c.id === 'classic'}
                      <rect x="7" y="6" width="20" height="6" rx="2" fill="#fff" />
                      <rect x="7" y="17" width="13" height="5" rx="2.5" fill="#0a84ff" />
                      <rect x="22" y="17" width="13" height="5" rx="2.5" fill="#0a84ff" />
                      <circle cx="41" cy="19.5" r="2.5" fill="#555" /><circle cx="48" cy="19.5" r="2.5" fill="#555" />
                      <rect x="7" y="25" width="18" height="4" rx="2" fill="#444" /><rect x="27" y="25" width="10" height="4" rx="2" fill="#ff9f0a" />
                    {:else if c.id === 'modules'}
                      <rect x="7" y="5" width="16" height="5" rx="2" fill="#fff" />
                      <rect x="7" y="13" width="16" height="16" rx="4" fill="#2a2a2e" />
                      <circle cx="11.5" cy="17.5" r="2.6" fill="#0a84ff" /><circle cx="18.5" cy="17.5" r="2.6" fill="#0a84ff" />
                      <circle cx="11.5" cy="24.5" r="2.6" fill="#555" /><circle cx="18.5" cy="24.5" r="2.6" fill="#555" />
                      <rect x="26" y="13" width="27" height="7" rx="3" fill="#2a2a2e" /><circle cx="49" cy="16.5" r="2.2" fill="#ff9f0a" />
                      <rect x="26" y="22" width="8" height="7" rx="2" fill="#2a2a2e" /><rect x="35.5" y="22" width="8" height="7" rx="2" fill="#2a2a2e" /><rect x="45" y="22" width="8" height="7" rx="2" fill="#2a2a2e" />
                    {:else if c.id === 'sliders'}
                      <rect x="7" y="5" width="16" height="5" rx="2" fill="#fff" />
                      <rect x="7" y="13" width="6" height="16" rx="3" fill="#333" /><rect x="7" y="20" width="6" height="9" rx="3" fill="#eee" />
                      <rect x="15" y="13" width="6" height="16" rx="3" fill="#333" /><rect x="15" y="16" width="6" height="13" rx="3" fill="#eee" />
                      <rect x="24" y="13" width="14" height="4.5" rx="2" fill="#2a2a2e" /><rect x="39.5" y="13" width="14" height="4.5" rx="2" fill="#2a2a2e" />
                      <rect x="24" y="19" width="22" height="4.5" rx="2" fill="#2a2a2e" /><rect x="47.5" y="19" width="6" height="4.5" rx="2" fill="#2a2a2e" />
                      <rect x="24" y="25" width="29.5" height="4.5" rx="2" fill="#2a2a2e" />
                    {:else}
                      <rect x="21" y="6" width="18" height="7" rx="2" fill="#fff" />
                      <rect x="5" y="20" width="50" height="10" rx="5" fill="#ffffff22" />
                      {#each [10, 17, 24, 31, 38, 45, 52] as cx, k (cx)}<circle {cx} cy="25" r="2.6" fill={k < 2 ? '#0a84ff' : k === 2 ? '#ff9f0a' : '#666'} />{/each}
                    {/if}
                  </svg>
                </span>
                <span class="style-name">{tr(c.label)}</span>
              </button>
            {/each}
          </div>
          <div class="net">
            <span class="name">{tr("Background photo")}</span>
            <span class="tint-pick">
              {#if photoPreview && islandPrefs?.ccPhoto}<img class="photo-thumb" src={photoPreview} alt="" />{/if}
              <label class="button-like" class:disabled={!islandPrefs}>
                {islandPrefs?.ccPhoto ? tr("Change…") : tr("Choose…")}
                <input type="file" accept="image/png,image/jpeg" hidden disabled={!islandPrefs} onchange={choosePhoto} />
              </label>
              {#if islandPrefs?.ccPhoto}<button onclick={removePhoto}>{tr("Remove")}</button>{/if}
            </span>
          </div>
          <p class="muted small">{tr("A photo (PNG or JPEG) behind Control Centre, darkened so its buttons stay easy to read. It stays on this computer.")}</p>
          <div class="net">
            <span class="name">{tr("Shape")}</span>
            <Picker
              label={tr("Island shape")}
              value={islandPrefs?.shape ?? 'pill'}
              disabled={!islandPrefs}
              options={[
                { value: 'pill', label: tr(N('Pill')) },
                { value: 'notch', label: tr(N('Notch')), hint: tr(N('joins the top of the screen')) },
              ]}
              onchange={(v) => setLook('shape', v)}
            />
          </div>
          <div class="net">
            <span class="name">{tr("Empty island shows")}</span>
            <Picker
              label={tr("What the empty island shows")}
              value={islandPrefs?.clockFace ?? 'time'}
              disabled={!islandPrefs}
              options={[
                { value: 'time', label: tr(N('The time')) },
                { value: 'date', label: tr(N('The day and the time')) },
                { value: 'seconds', label: tr(N('The time with seconds')) },
              ]}
              onchange={(v) => setLook('clock', v)}
            />
          </div>
          <div class="net">
            <span class="name">{tr("Clock")}</span>
            <div class="segmented" role="radiogroup" aria-label={tr("12- or 24-hour clock")}>
              {#each [{ v: 'auto', l: N('Automatic') }, { v: '12', l: N('12-hour') }, { v: '24', l: N('24-hour') }] as o (o.v)}
                {@const on = (islandPrefs?.clockHours ?? 'auto') === o.v}
                <button role="radio" aria-checked={on} class:chosen={on} disabled={!islandPrefs} onclick={() => setLook('hours', o.v)}
                  >{tr(o.l)}</button
                >
              {/each}
            </div>
          </div>
          <div class="net">
            <span class="name">{tr("Font")}</span>
            <Picker
              label={tr("The island's font")}
              value={islandPrefs?.font ?? ''}
              disabled={!islandPrefs}
              searchable
              options={[
                { value: '', label: tr(N('Built-in')), hint: 'Inter' },
                ...fonts.map((f) => ({ value: f, label: f, font: f })),
              ]}
              onchange={(v) => setLook('font', v || 'default')}
            />
          </div>
          <div class="net">
            <span class="name">{tr("Temperature")}</span>
            {@render units()}
          </div>
          <div class="net">
            <span class="name">{tr("Border in the album’s colour while music plays")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Border in the album's colour while music plays")}
              checked={islandPrefs?.albumBorder ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('albumBorder', e.currentTarget.checked)}
            />
          </div>
          {#each [['albumGlow', N('Glow in the album’s colour while music plays')], ['visualizer', N('Moving bars as the song’s progress bar')], ['screenshotPreview', N('Show each new screenshot on the island')]] as [key, label] (key)}
              <div class="net">
                <span class="name">{tr(label)}</span>
                <input
                  type="checkbox"
                  class="switch"
                  aria-label={tr(label)}
                  checked={islandPrefs ? Boolean(islandPrefs[key as keyof IslandPrefs]) : true}
                  disabled={!islandPrefs}
                  onchange={(e) => setSwitch(key as keyof IslandPrefs, e.currentTarget.checked)}
                />
              </div>
            {/each}
          <div class="net">
            <span class="name">{tr("Size")}</span>
            <div class="segmented" role="radiogroup" aria-label={tr("Island size")}>
              {#each [{ v: 'small', l: N('Small') }, { v: 'medium', l: N('Medium') }, { v: 'large', l: N('Large') }] as o (o.v)}
                {@const on = (islandPrefs?.size ?? 'medium') === o.v}
                <button role="radio" aria-checked={on} class:chosen={on} disabled={!islandPrefs} onclick={() => setLook('size', o.v)}
                  >{tr(o.l)}</button
                >
              {/each}
            </div>
          </div>
          {#if displays.length > 1}
            <div class="net">
              <span class="name">{tr("Display")}</span>
              <Picker
                label={tr("Display for the island")}
                value={islandPrefs?.display || 'primary'}
                disabled={!islandPrefs}
                options={[
                  { value: 'primary', label: tr(N('Primary display')) },
                  { value: 'all', label: tr(N('All displays')) },
                  ...displays.map((d) => ({ value: d, label: d })),
                ]}
                onchange={(v) => setLook('display', v)}
              />
            </div>
          {/if}
          <div class="net">
            <span class="name">{tr("Position")}</span>
            <Picker
              label={tr("Where the island sits")}
              value={islandPrefs?.position ?? 'top'}
              disabled={!islandPrefs}
              options={[
                { value: 'top', label: tr(N('Top centre')) },
                { value: 'top-left', label: tr(N('Top left')) },
                { value: 'top-right', label: tr(N('Top right')) },
                { value: 'bottom', label: tr(N('Bottom centre')) },
              ]}
              onchange={(v) => setLook('position', v)}
            />
          </div>
          {#if displays.length > 1 && islandPrefs?.display === 'all'}
            <p class="muted small">{tr("A size for each display:")}</p>
            {#each displays as d (d)}
              <div class="net">
                <span class="name">{d}</span>
                <Picker
                  label={tf('Island size on {}', d)}
                  value={islandPrefs?.displaySizes?.[d] ?? 'default'}
                  options={[
                    { value: 'default', label: tr(N('Same as above')) },
                    { value: 'small', label: tr(N('Small')) },
                    { value: 'medium', label: tr(N('Medium')) },
                    { value: 'large', label: tr(N('Large')) },
                  ]}
                  onchange={(v) => displaySize(d, v)}
                />
              </div>
            {/each}
          {/if}
          <div class="net">
            <span class="name">{tr("Gap above the island")}</span>
            <input
              type="range"
              min="0"
              max="40"
              step="2"
              aria-label={tr("Gap above the island, in pixels")}
              value={islandPrefs?.offset ?? 0}
              disabled={!islandPrefs}
              onchange={(e) => setLook('offset', e.currentTarget.value)}
            />
            <span class="muted small">{islandPrefs?.offset ?? 0} {tr("px")}</span>
          </div>
          <p class="muted small">{tr("Islet restarts itself to apply a new size, position, gap or display. Notched Macs always hang the island from the notch.")}</p>
        </div>
        <div class="card">
          <h2>{tr("Shortcut")}</h2>
          <p class="muted small">
            {tr("A key combination that opens the island, or closes it. Current:")}
            <strong>{islandPrefs?.hotkey || 'none'}</strong>
          </p>
          <form class="join" onsubmit={(e) => { e.preventDefault(); void saveHotkey(hotkeyDraft); }}>
            <input
              placeholder={tr("Click here, then press e.g. Ctrl+Alt+I")}
              value={hotkeyDraft}
              onkeydown={captureHotkey}
              readonly
              aria-label={tr("Press the shortcut keys")}
            />
            <button type="submit" disabled={!hotkeyDraft}>{tr("Save")}</button>
            {#if islandPrefs?.hotkey}
              <button type="button" onclick={() => saveHotkey('')}>{tr("Remove")}</button>
            {/if}
          </form>
          {#each [['launcher', N('Open the launcher'), islandPrefs?.launcherHotkey], ['note', N('Write a quick note'), islandPrefs?.noteHotkey]] as [which, label, current] (which)}
            <p class="muted small">{tr(label)}{tr(". Current:")} <strong>{current || tr('none')}</strong></p>
            <form
              class="join"
              onsubmit={(e) => {
                e.preventDefault();
                void saveExtraHotkey(which as 'launcher' | 'note', extraDraft[which as 'launcher' | 'note']);
              }}
            >
              <input
                placeholder={tr("Click here, then press e.g. Ctrl+Alt+Space")}
                value={extraDraft[which as 'launcher' | 'note']}
                onkeydown={(e) => {
                  const combo = comboOf(e);
                  if (combo) extraDraft[which as 'launcher' | 'note'] = combo;
                }}
                readonly
                aria-label={tf('Press the keys to: {}', tr(label))}
              />
              <button type="submit" disabled={!extraDraft[which as 'launcher' | 'note']}>{tr("Save")}</button>
              {#if current}
                <button type="button" onclick={() => saveExtraHotkey(which as 'launcher' | 'note', '')}>{tr("Remove")}</button>
              {/if}
            </form>
          {/each}
          {#if info?.os === 'linux'}
            <p class="muted small">
              {tr("On native Wayland (KDE Plasma, sway, Hyprland) only the desktop can watch keys: add a shortcut in its keyboard settings that runs")} <code>islet toggle</code>, <code>islet launcher</code> {tr("or")}
              <code>islet note</code>.
            </p>
          {/if}
        </div>
        <div class="card">
          <h2>{tr("Privacy")}</h2>
          <div class="net">
            <span class="name">{tr("Camera and microphone dots")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Show camera and microphone dots")}
              checked={islandPrefs?.privacyDots ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('privacyDots', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">{tr("A green dot while an app uses the camera, an orange one for the microphone.")}</p>
          <div class="net">
            <span class="name">{tr("Calls (when an app uses the microphone)")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Show calls when an app uses the microphone")}
              checked={islandPrefs?.calls ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('calls', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            {tr("While Zoom, Teams, a browser call or any app has the microphone, the island shows how long the call has been going and a button that mutes your microphone.")}
          </p>
          <div class="net">
            <span class="name">{tr("Downloads in progress")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Show downloads in progress")}
              checked={islandPrefs?.downloads ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('downloads', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            {tr("Shows what your browser is downloading (from your Downloads folder), then \"Downloaded\": tap it to open the file. Only file names and sizes are read, on this computer.")}{#if info?.os === 'macos'} {tr("macOS asks once whether Islet may see your Downloads folder.")}{/if}
          </p>
          <div class="net">
            <span class="name">{tr("Plugin widgets")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Run plugin widgets")}
              checked={islandPrefs?.plugins ?? false}
              disabled={!islandPrefs}
              onchange={async (e) => {
                await setSwitch('plugins', e.currentTarget.checked);
                await loadPlugins();
              }}
            />
          </div>
          <p class="muted small">
            {tr("Your own scripts, shown as live activities: each plugin is a folder with a")} <code>plugin.json</code> {tr("in")}
            {#if plugins?.folder}<code>{plugins.folder}</code>{:else}{tr("the plugins folder")}{/if}{tr(". Islet runs them on their schedule while this is on, on this computer only.")}
            <button class="link" onclick={reloadPlugins}>{tr("Reread the folder")}</button>
          </p>
          {#each plugins?.plugins ?? [] as p (p.folder)}
            <p class="small">
              <b>{p.name ?? p.folder}</b>
              {#if p.error}<span class="muted">{tr("— can’t run:")} {p.error}</span>{:else}<span class="muted">{tr("every")} {p.every} s</span>{/if}
            </p>
          {/each}
          <div class="net">
            <span class="name">{tr("Clipboard history")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Keep clipboard history")}
              checked={islandPrefs?.clipboard ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('clipboard', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            {tr("Keeps the last 20 texts and pictures you copy, in memory only: nothing is saved to disk, and the list is gone when Islet quits. Copies that password managers mark as secret are skipped, and so is everything copied from the apps below.")}{#if info?.os === 'macos'} {tr("macOS has no clipboard notifications, so while this is on Islet checks the clipboard twice a second.")}{/if}
          </p>
          <p class="muted small">{tr("Never keep copies from:")}</p>
          <div class="actions left">
            {#each islandPrefs?.clipboardExclude ?? [] as app (app)}
              <button onclick={() => clipExclude('remove', app)} aria-label={tf('Keep copies from {} again', app)}>{app} ✕</button>
            {/each}
          </div>
          <form
            class="join"
            onsubmit={(e) => {
              e.preventDefault();
              void clipExclude('add', excludeDraft);
            }}
          >
            <input placeholder={tr("An app's name, e.g. Slack")} maxlength="60" bind:value={excludeDraft} aria-label={tr("App to leave out")} />
            <button type="submit" disabled={!excludeDraft.trim()}>{tr("Add")}</button>
          </form>
        </div>
        <div class="card">
          <h2>{tr("Spotify on your other devices")}</h2>
          <p class="muted small">
            {tr("Optional. Shows what's playing on your phone or a speaker through Spotify Connect, with controls (controls need Premium). It uses your own Spotify app and checks every 10 seconds while something plays, every minute otherwise.")}
          </p>
          {#if status?.spotifyLinked}
            <div class="net">
              <span class="name ok">{tr("Linked")}</span>
              <input
                type="checkbox"
                class="switch"
                aria-label={tr("Show Spotify playback from other devices")}
                checked={islandPrefs?.spotify ?? false}
                onchange={(e) => spotifyCmd(e.currentTarget.checked ? 'on' : 'off')}
              />
            </div>
            <div class="actions left"><button class="link" onclick={() => spotifyCmd('logout')}>{tr("Unlink")}</button></div>
          {:else}
            <details class="small muted">
              <summary>{tr("How to set it up")}</summary>
              <p>{tr("1. Open")} <button class="link" onclick={() => api.openLink('https://developer.spotify.com/dashboard')}>developer.spotify.com/dashboard</button> {tr("and create an app (any name), choosing \"Web API\".")}</p>
              <p>{tr("2. Add this redirect URI exactly:")} <code>http://127.0.0.1:43821/callback</code></p>
              <p>{tr("3. Copy the app's Client ID here and sign in.")}</p>
            </details>
            <form class="join" onsubmit={(e) => { e.preventDefault(); if (/^[A-Za-z0-9]{1,64}$/.test(spotifyClient.trim())) void spotifyLogin(); }}>
              <input placeholder={tr("Client ID")} bind:value={spotifyClient} aria-label={tr("Spotify client ID")} autocomplete="off" />
              <button class="primary" type="submit" disabled={!/^[A-Za-z0-9]{1,64}$/.test(spotifyClient.trim())}>{tr("Sign in")}</button>
            </form>
          {/if}
          {#if spotifyNote}<p class="small" role="status">{spotifyNote}</p>{/if}
        </div>
        <div class="card">
          <h2>{tr("Notifications")}</h2>
          {#if info?.os === 'linux' || info?.os === 'windows'}
            <div class="net">
              <span class="name">{tr("Show notifications on the island")}</span>
              <input
                type="checkbox"
                class="switch"
                aria-label={tr("Show notifications on the island")}
                checked={islandPrefs?.notifications ?? false}
                disabled={!islandPrefs}
                onchange={(e) => setSwitch('notifications', e.currentTarget.checked)}
              />
            </div>
            <p class="muted small">
              {tr("New notifications appear on the island for a few seconds. Click it to see the latest ones. They stay on this computer and are never logged.")}
            </p>
            {#if info?.os === 'windows'}
              <p class="muted small">
                {tr("Windows lets only packaged apps read other apps' notifications, so this needs the MSIX version of Islet. Windows then asks once; you can change it later in Settings → Privacy & security → Notifications.")}
              </p>
            {/if}
          {:else}
            <p class="muted small">{tr("macOS doesn't let apps read other apps' notifications, so this isn't available on a Mac.")}</p>
          {/if}
        </div>
        <div class="card">
          <h2>{tr("Weather")}</h2>
          <p class="muted small">
            {tr("Shows on the clock card. Off until you choose a place. Then Islet asks Open-Meteo once an hour (no account or key; it sees only the place's coordinates).")}
          </p>
          {#if islandPrefs?.weather}
            <div class="net">
              <span class="name ok">{islandPrefs.weather.name}</span>
              <button onclick={weatherOff}>{tr("Turn off")}</button>
            </div>
            <div class="net">
              <span class="name">{tr("Temperature")}</span>
              {@render units()}
            </div>
          {/if}
          <form class="join" onsubmit={(e) => { e.preventDefault(); if (cityQuery.trim() && !cityQuery.trim().startsWith('-')) void searchCity(); }}>
            <input placeholder={tr("City, e.g. Kampala")} bind:value={cityQuery} aria-label={tr("City")} />
            <button type="submit" disabled={!cityQuery.trim()}>{tr("Search")}</button>
          </form>
          {#each cityResults as p (p.name + p.lat)}
            <div class="net">
              <span class="name">{p.name}</span>
              <button class="primary" onclick={() => chooseCity(p)}>{tr("Use")}</button>
            </div>
          {/each}
          {#if weatherNote}<p class="small" role="status">{weatherNote}</p>{/if}
          <p class="muted small">{tr("Weather data by")} <button class="link" onclick={() => api.openLink('https://open-meteo.com/')}>Open-Meteo.com</button> {tr("(CC BY 4.0).")}</p>
        </div>
        <div class="card">
          <h2>{tr("Calendar")}</h2>
          <p class="muted small">
            {tr("Ten minutes before an event, the island shows what's next. Islet reads your calendar's private iCal link every 15 minutes while this is on. The link is kept in your keychain.")}
          </p>
          {#if islandPrefs}
            <div class="net">
              <span class="name">{tr("Show upcoming events")}</span>
              <input
                type="checkbox"
                class="switch"
                aria-label={tr("Show upcoming events")}
                checked={islandPrefs.calendar}
                onchange={(e) => calendarCmd(e.currentTarget.checked ? 'on' : 'off')}
              />
            </div>
          {/if}
          <form class="join" onsubmit={(e) => { e.preventDefault(); if (calLink.trim()) void saveCalendar(); }}>
            <input type="password" autocomplete="off" placeholder="https://… or webcal://… (.ics)" bind:value={calLink} aria-label={tr("Calendar link")} />
            <button class="primary" type="submit" disabled={!calLink.trim()}>{tr("Save link")}</button>
          </form>
          <details class="small muted">
            <summary>{tr("Where do I find the link?")}</summary>
            <p><strong>{tr("Google Calendar:")}</strong> {tr("Settings → your calendar → Integrate calendar → \"Secret address in iCal format\".")}</p>
            <p><strong>{tr("Outlook:")}</strong> {tr("Settings → Calendar → Shared calendars → Publish a calendar → copy the ICS link.")}</p>
            <p><strong>{tr("iCloud:")}</strong> {tr("Calendar → share icon next to a calendar → Public Calendar → copy the link.")}</p>
            <p>{tr("Anyone with the link can read the calendar, so keep it private.")}</p>
          </details>
          <div class="actions left"><button class="link" onclick={() => calendarCmd('clear')}>{tr("Remove saved link")}</button></div>
          {#if calNote}<p class="small" role="status">{calNote}</p>{/if}
        </div>
        <p class="muted small">{tr("Right-click the island to close it. Click it to open, and press and hold for more.")}</p>
      {:else if page === 'connections'}
        <h1>{tr("Wi-Fi & Bluetooth")}</h1>
        <p class="muted">{tr("The same lists open from the island: click it, then tap Wi-Fi or Bluetooth on the clock card.")}</p>
        <div class="card">
          <div class="row">
            <h2>{tr("Wi-Fi")}</h2>
            <button disabled={scanning} onclick={() => loadConnections(true)}>{scanning ? 'Searching…' : 'Refresh'}</button>
          </div>
          {#if wifi?.note}<p class="muted small">{wifi.note}</p>{/if}
          {#each wifi?.networks ?? [] as n (n.ssid)}
            <div class="net">
              <span class="bars" aria-hidden="true">{signalBars(n.strength)}</span>
              <span class="name" class:ok={n.active}>{n.ssid}</span>
              {#if n.active}
                <span class="ok small">{tr("Connected")}</span>
              {:else}
                <button onclick={() => joinWifi(n.ssid, n.secure && !n.known)}>
                  {n.secure && !n.known ? 'Join…' : 'Join'}
                </button>
              {/if}
            </div>
            {#if joining === n.ssid && !n.active}
              <form class="join" onsubmit={(e) => { e.preventDefault(); void joinWifi(n.ssid, true); }}>
                <input type="password" autocomplete="off" placeholder="Password for {n.ssid}" bind:value={wifiPassword} />
                <button class="primary" type="submit" disabled={!wifiPassword}>{tr("Join")}</button>
              </form>
            {/if}
          {:else}
            <p class="muted">{scanning ? 'Looking for networks…' : 'No networks found.'}</p>
          {/each}
        </div>
        <div class="card">
          <h2>{tr("Bluetooth")}</h2>
          {#if bt?.note}<p class="muted small">{bt.note}</p>{/if}
          {#each bt?.devices ?? [] as d (d.id)}
            <div class="net">
              <span class="name" class:ok={d.connected}>{d.name}</span>
              <button onclick={() => toggleBt(d.id, d.connected)}>
                {bt?.canConnect ? (d.connected ? 'Disconnect' : 'Connect') : 'Open settings'}
              </button>
            </div>
          {:else}
            <p class="muted">{tr("No paired devices. Pair new ones in your system's Bluetooth settings.")}</p>
          {/each}
        </div>
        <div class="card">
          <div class="row">
            <h2>{tr("Speed test")}</h2>
            <button disabled={speedRunning} onclick={runSpeedTest}>{speedRunning ? 'Testing…' : 'Test speed'}</button>
          </div>
          {#if speed}
            <div class="speed" role="status">
              <span><b>{formatMbps(speed.downMbps)}</b> {tr("Mbps down")}</span>
              <span><b>{formatMbps(speed.upMbps)}</b> {tr("Mbps up")}</span>
              <span><b>{Math.round(speed.latencyMs)}</b> {tr("ms")}</span>
            </div>
          {/if}
          {#if speedNote}<p class="small" role="status">{speedNote}</p>{/if}
          <p class="muted small">
            {tr("Runs only when you press the button: it downloads about 25 MB and uploads 10 MB with Cloudflare’s public speed-test servers (speed.cloudflare.com), so mind a metered connection.")}
          </p>
        </div>
        <div class="card">
          <h2>{tr("Your Android phone")}</h2>
          <p class="muted">
            {tr("With the Islet app on your phone, its notifications, music and battery show on the island, and the island’s music buttons control the phone. The two talk directly over your Wi-Fi, encrypted with a key only they hold. Nothing goes through the internet.")}
          </p>
          {#if pairing}
            <div class="pairing">
              <svg class="qr" viewBox="-2 -2 {pairing.rows.length + 4} {pairing.rows.length + 4}" role="img" aria-label={tr("Pairing code")}>
                <rect x="-2" y="-2" width={pairing.rows.length + 4} height={pairing.rows.length + 4} fill="#fff" />
                <path d={qrPath(pairing.rows)} fill="#000" />
              </svg>
              <ol>
                <li>{tr("Install the Islet app on your phone (the APK is on Islet’s download page).")}</li>
                <li>{tr("Join the same Wi-Fi as this computer.")}</li>
                <li>{tr("In the app, tap")} <b>{tr("Scan the code")}</b> {tr("and point the camera here.")}</li>
                <li>{tr("Allow notification access when the app asks.")}</li>
              </ol>
            </div>
            <p class="small muted">
              {tr("Keep this code to yourself: it holds the pairing key. This computer listens on")}
              {pairing.hosts.join(', ')} {tr("port")} {pairing.port}{tr("; if your firewall asks, allow Islet on private networks.")}
            </p>
          {/if}
          <div class="actions left">
            <button class="primary" onclick={() => phoneRun(['phone', 'pair', '--json'])}>
              {pairing ? tr('Show the code again') : phoneState?.paired ? tr('Show the pairing code') : tr('Pair a phone')}
            </button>
            {#if phoneState?.paired}
              <button onclick={() => phoneRun(['phone', 'pair', '--new', '--json'])}>{tr("New code")}</button>
              <button onclick={() => phoneRun(['phone', 'unpair'])}>{tr("Unpair")}</button>
            {/if}
            <button onclick={() => api.openLink(`${info?.repo}/releases`)}>{tr("Get the app")}</button>
          </div>
          {#if phoneNote}<p class="small" role="status">{phoneNote}</p>{/if}
        </div>
        {#if phoneState?.paired}
          <div class="card">
            <h2>{tr("Phone and computer together")}</h2>
            <div class="net">
              <span class="name">{tr("Can't find your phone?")}</span>
              <button onclick={() => phoneSay(['phone', 'ring'])}>{tr("Ring my phone")}</button>
            </div>
            <p class="muted small">{tr("It rings loudly, even on silent, until you stop it on the phone (or for a minute).")}</p>
            <label for="psend">{tr("Send to your phone")}</label>
            <textarea id="psend" rows="2" bind:value={phoneSendText} placeholder={tr("A link, an address, a note…")}></textarea>
            <div class="actions">
              <button class="primary" disabled={!phoneSendText.trim()} onclick={() => phoneSay(['phone', 'send', '-'], phoneSendText, () => (phoneSendText = ''))}
                >{tr("Send")}</button
              >
            </div>
            <p class="muted small">{tr("It lands on the phone's clipboard, with a notification (links open with one tap). On the phone, Share → Islet sends things the other way, onto this computer's clipboard.")}</p>
            <label for="preply">{tr("Reply to the phone's newest message")}</label>
            <input id="preply" bind:value={phoneReplyText} placeholder={tr("Your reply")} />
            <div class="actions">
              <button disabled={!phoneReplyText.trim()} onclick={() => phoneSay(['phone', 'reply', '-'], phoneReplyText, () => (phoneReplyText = ''))}
                >{tr("Reply")}</button
              >
            </div>
            <span class="label">{tr("Quick replies")}</span>
            <p class="muted small">{tr("Shown under a message on the island that can be answered (WhatsApp, Messages and other chat apps). Up to four, each a few words.")}</p>
            <div class="two">
              {#each [0, 1, 2, 3] as i (i)}
                <input aria-label={tf('Quick reply {}', i + 1)} maxlength="30" bind:value={quickReplies[i]} />
              {/each}
            </div>
            <div class="actions">
              <button onclick={() => phoneSay(['phone', 'replies', 'default'], undefined, loadQuickReplies)}>{tr("Usual ones")}</button>
              <button class="primary" onclick={() => phoneSay(['phone', 'replies', '-'], quickReplies.map((r) => r.trim()).filter(Boolean).join('\n'))}
                >{tr("Save")}</button
              >
            </div>
            {#if phoneSayNote}<p class="small" role="status">{phoneSayNote}</p>{/if}
          </div>
        {/if}
        {#if connNote}<p class="small" role="status">{connNote}</p>{/if}
      {:else if page === 'timer'}
        <h1>{tr("Timer & focus")}</h1>
        <p class="muted">{tr("Timers count down on the island. When one ends, the island opens and chimes until you stop it.")}</p>
        <div class="card">
          <h2>{tr("Timer")}</h2>
          <div class="actions left">
            {#each [1, 5, 10, 15, 25, 60] as m (m)}
              <button onclick={() => startTimer(m)}>{m < 60 ? `${m} min` : '1 hour'}</button>
            {/each}
          </div>
          <div class="two">
            <div>
              <label for="tmin">{tr("Minutes")}</label>
              <input id="tmin" type="number" min="1" max="1440" bind:value={timerMinutes} />
            </div>
            <div>
              <label for="tlabel">{tr("Name (optional)")}</label>
              <input id="tlabel" bind:value={timerLabel} placeholder={tr("Tea")} />
            </div>
          </div>
          <div class="actions">
            <button class="primary" disabled={!(timerMinutes > 0)} onclick={() => startTimer(timerMinutes)}>{tr("Start")}</button>
          </div>
        </div>
        <div class="card">
          <h2>{tr("Focus session")}</h2>
          <p class="muted small">{tr("Work and break in turns. A chime and a quick look at the island mark each change.")}</p>
          <div class="three">
            <div>
              <label for="fwork">{tr("Work (min)")}</label>
              <input id="fwork" type="number" min="1" max="240" bind:value={focusWork} />
            </div>
            <div>
              <label for="fbreak">{tr("Break (min)")}</label>
              <input id="fbreak" type="number" min="1" max="60" bind:value={focusBreak} />
            </div>
            <div>
              <label for="frounds">{tr("Rounds")}</label>
              <input id="frounds" type="number" min="1" max="12" bind:value={focusRounds} />
            </div>
          </div>
          <div class="actions">
            <button
              class="primary"
              onclick={() =>
                timerCmd(['focus', '--work', `${focusWork}m`, '--break', `${focusBreak}m`, '--rounds', String(focusRounds)])}
              >{tr("Start focus")}</button
            >
          </div>
          <div class="net">
            <span class="name">{tr("Do not disturb while working")}</span>
            <input
              type="checkbox"
              class="switch"
              aria-label={tr("Do not disturb while working")}
              checked={islandPrefs?.focusQuiet ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('focusQuiet', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            {tr("Keeps notifications, volume and battery pop-ups and calendar reminders off the island during a work round. Breaks let them through, and alarms still ring.")}
          </p>
          {#if streak && streak.rounds > 0}
            <p class="small muted" role="status">{streakLine(streak)}</p>
          {/if}
        </div>
        <div class="actions left">
          <button onclick={() => timerCmd(['timer', 'pause'])}>{tr("Pause")}</button>
          <button onclick={() => timerCmd(['timer', 'resume'])}>{tr("Resume")}</button>
          <button onclick={() => timerCmd(['timer', 'stop'])}>{tr("Stop")}</button>
        </div>
        {#if timerNote}<p class="small" role="status">{timerNote}</p>{/if}
        <div class="card">
          <h2>{tr("Class timetable")}</h2>
          <p class="muted small">
            {tr("Enter your week once. The island shows a lesson ten minutes before it starts, and the clock card counts down to the next one and shows your free gaps. Kept on this computer only.")}
          </p>
          {#each WEEK_DAYS as dayName, d (d)}
            {#if lessons.some((l) => l.day === d)}
              <h3 class="day">{tr(dayName)}</h3>
              {#each lessons as l, i (i)}
                {#if l.day === d}
                  <div class="net">
                    <span class="when">{formatMinutes(l.start, h24)} – {formatMinutes(l.end, h24)}</span>
                    <span class="name">{l.name}</span>
                    <span class="muted small">{l.room}</span>
                    <button class="link" aria-label={tf('Delete {} on {}', l.name, tr(dayName))} onclick={() => removeLesson(i)}>✕</button>
                  </div>
                {/if}
              {/each}
            {/if}
          {/each}
          <div class="three">
            <div>
              <span class="label">{tr("Day")}</span>
              <Picker
                label={tr("Day")}
                value={String(lessonDay)}
                options={WEEK_DAYS.map((dayName, d) => ({ value: String(d), label: tr(dayName) }))}
                onchange={(v) => (lessonDay = Number(v))}
              />
            </div>
            <div>
              <label for="lstart">{tr("Starts")}</label>
              <input id="lstart" type="time" bind:value={lessonStart} />
            </div>
            <div>
              <label for="lend">{tr("Ends")}</label>
              <input id="lend" type="time" bind:value={lessonEnd} />
            </div>
          </div>
          <div class="two">
            <div>
              <label for="lname">{tr("Lesson")}</label>
              <input id="lname" type="text" maxlength="40" placeholder={tr("Maths")} bind:value={lessonName} />
            </div>
            <div>
              <label for="lroom">{tr("Room (optional)")}</label>
              <input id="lroom" type="text" maxlength="40" placeholder={tr("Room 4")} bind:value={lessonRoom} />
            </div>
          </div>
          <div class="actions">
            <button class="primary" disabled={!lessonName.trim()} onclick={addLesson}>{tr("Add lesson")}</button>
          </div>
          {#if lessonNote}<p class="small" role="status">{lessonNote}</p>{/if}
        </div>
      {:else if page === 'alarms'}
        <h1>{tr("Alarms")}</h1>
        <p class="muted">
          {tr("When an alarm goes off, the island opens with a bell and plays a chime until you snooze or stop it. Alarms ring while Islet is running and your computer is awake.")}
        </p>
        <div class="card">
          <h2>{tr("New alarm")}</h2>
          <div class="two">
            <div>
              <label for="atime">{tr("Time")}</label>
              <input id="atime" type="time" bind:value={alarmTime} />
            </div>
            <div>
              <label for="alabel">{tr("Name (optional)")}</label>
              <input id="alabel" bind:value={alarmLabel} placeholder={tr("Wake up")} />
            </div>
          </div>
          <span class="label" id="days-label">{tr("Repeat")}</span>
          <div class="days" role="group" aria-labelledby="days-label">
            {#each DAY_LETTERS as d, i (i)}
              <button
                class="day"
                class:on={alarmDays & (1 << i)}
                aria-pressed={(alarmDays & (1 << i)) !== 0}
                aria-label={tr(WEEKDAY_NAMES[i])}
                onclick={() => (alarmDays ^= 1 << i)}>{tr(d)}</button
              >
            {/each}
            <span class="muted small">{describeDays(alarmDays, tr)}</span>
          </div>
          <div class="actions"><button class="primary" onclick={addAlarm}>{tr("Add alarm")}</button></div>
          {#if alarmNote}<p class="small" role="status">{alarmNote}</p>{/if}
        </div>
        {#each status?.alarms ?? [] as a (a.id)}
          <div class="card row">
            <div>
              <h2 class:muted={!a.enabled}>{formatAlarmTime(a, h24)}</h2>
              <p class="muted">{a.label || tr('Alarm')} · {describeDays(a.days, tr)}</p>
            </div>
            <div class="row">
              <button class="link" onclick={() => alarmCmd('remove', a.id)}>{tr("Delete")}</button>
              <input
                type="checkbox"
                class="switch"
                aria-label="Alarm at {formatAlarmTime(a, h24)} on"
                checked={a.enabled}
                onchange={(e) => alarmCmd(e.currentTarget.checked ? 'on' : 'off', a.id)}
              />
            </div>
          </div>
        {:else}
          <p class="muted">{tr("No alarms yet.")}</p>
        {/each}
      {:else if page === 'navigation'}
        <h1>{tr("Navigation")}</h1>
        <p class="muted">
          {tr("See your travel time with live traffic, then hand off to Google Maps or Waze. Uses the Google Routes API with")}
          <strong>{tr("your own")}</strong> {tr("key: Google bills your project, usually within its free monthly allowance.")}
          <button class="link" onclick={() => api.openLink(docs('docs/navigation.md'))}>{tr("How to get a key")}</button>
        </p>
        <div class="card">
          <h2>{tr("Google Routes API key")}</h2>
          <p class={status?.routesKey ? 'ok' : 'muted'}>{status?.routesKey ? 'Saved in your keychain' : 'Not set'}</p>
          <label for="key">{status?.routesKey ? 'Replace key' : 'Key'}</label>
          <input id="key" type="password" autocomplete="off" spellcheck="false" bind:value={key} placeholder={tr("AIza…")} />
          {#if key && !keyLooksValid(key)}<p class="warn small">{tr("This doesn’t look like a Google API key.")}</p>{/if}
          <div class="actions">
            {#if status?.routesKey}<button onclick={clearKey}>{tr("Remove key")}</button>{/if}
            <button class="primary" disabled={!key.trim()} onclick={saveKey}>{tr("Save key")}</button>
          </div>
          {#if keyNote}<p class="small" role="status">{keyNote}</p>{/if}
        </div>
        <div class="card">
          <h2>{tr("Trip")}</h2>
          {#if status?.trip}
            <p class="ok">{tr("Showing the trip to")} {status.trip.label || status.trip.destination}</p>
          {/if}
          <label for="dest">{tr("Destination")}</label>
          <input
            id="dest"
            bind:value={trip.destination}
            oninput={destinationTyped}
            placeholder={tr("Plot 5, Kampala Road")}
            autocomplete="off"
            aria-autocomplete="list"
          />
          {#if places.length}
            <div class="places" role="listbox" aria-label={tr("Suggestions")}>
              {#each places as p (p.placeId)}
                <button role="option" aria-selected="false" onclick={() => pickPlace(p)}>
                  <strong>{p.main}</strong>
                  {#if p.secondary}<span class="muted small">{p.secondary}</span>{/if}
                </button>
              {/each}
              <p class="muted small attribution">{tr("Powered by Google")}</p>
            </div>
          {/if}
          <label for="from">{tr("From")}</label>
          <input id="from" bind:value={trip.origin} placeholder={tr("An address or “lat,lng”")} />
          <div class="two">
            <div>
              <label for="label">{tr("Name on the island (optional)")}</label>
              <input id="label" bind:value={trip.label} placeholder={tr("Work")} />
            </div>
            <div>
              <span class="label">{tr("Travel by")}</span>
              <Picker
                label={tr("Travel by")}
                value={trip.mode}
                options={MODES.map((m) => ({ value: m.id, label: tr(m.label) }))}
                onchange={(v) => (trip.mode = v as typeof trip.mode)}
              />
            </div>
          </div>
          <div class="actions">
            {#if status?.trip}<button onclick={stopTrip}>{tr("Stop trip")}</button>{/if}
            <button class="primary" onclick={startTrip}>{status?.trip ? 'Update trip' : 'Start trip'}</button>
          </div>
          {#if tripNote}<p class="small" role="status">{tripNote}</p>{/if}
        </div>
        <p class="muted small">
          {tr("Islet refreshes the ETA every minute while the card is open and every 2–5 minutes otherwise, and makes no requests without a trip. Trips go straight to Google; route results are never stored.")}
        </p>
      {:else if page === 'launcher'}
        <h1>{tr("Launcher")}</h1>
        <p class="muted">
          {tr("Type an app’s or an open window’s name, a sum (12 * 7.5), a conversion (5 km in miles) or anything to search the web for. Enter opens the first result. Start with")} <code>note:</code> {tr("to save a quick note.")}
        </p>
        <div class="card">
          <input
            type="text"
            class="launcher-input"
            aria-label={tr("Search apps, the web, or type a sum")}
            placeholder={tr("Search…")}
            autocomplete="off"
            spellcheck="false"
            bind:this={launcherInput}
            bind:value={launcherText}
            onkeydown={(e) => {
              if (e.key === 'Enter') void runLauncherRow(launcherRows.find((r) => r.run));
              if (e.key === 'Escape') launcherText = '';
            }}
          />
          {#each launcherRows as row, i (i)}
            {#if row.run}
              <button class="launcher-row" onclick={() => runLauncherRow(row)}>
                <span class="name">{row.title}</span>
                <span class="muted small">{row.detail}</span>
              </button>
            {:else}
              <div class="launcher-row answer" role="status">
                <span class="name">{row.title}</span>
                <span class="muted small">{row.detail}</span>
              </div>
            {/if}
          {/each}
          {#if launcherNote}<p class="small" role="status">{launcherNote}</p>{/if}
        </div>
        <p class="muted small">
          {tr("Give the launcher and quick notes their own keys in General → Shortcut. Web searches go to DuckDuckGo, and only when you choose that row.")}
        </p>
      {:else if page === 'extras'}
        <h1>{tr("Extras")}</h1>
        <div class="card">
          <h2>{tr("Performance")}</h2>
          <p class="muted">
            {#if islandPrefs?.performance === 'saver'}
              {tr("Power saver is on. Islet keeps short histories and small album art, and the waveform stands still, so playing music costs no extra battery.")}
            {:else if islandPrefs?.performance === 'light'}
              {tr("Light: short histories, small album art and a waveform at 30 frames a second. Made for laptops with 4 GB of memory.")}
            {:else if islandPrefs?.performance === 'full'}
              {tr("Full: animation at 60 frames a second, sharper album art, a clipboard history of 500 copies and room for 100 files on the shelf. Islet uses more memory and a little more power for it.")}
            {:else}
              {tr("Automatic: full on this computer, unless it has 4 GB of memory or less, or two processor cores, when Islet keeps to the light limits.")}
            {/if}
          </p>
          <div class="net">
            <span class="name">{tr("Mode")}</span>
            <Picker
              label={tr("Performance")}
              value={islandPrefs?.performance ?? 'auto'}
              disabled={!islandPrefs}
              options={PERFORMANCE.map((m) => ({ value: m.id, label: tr(m.label), hint: tr(m.hint) }))}
              onchange={(v) => setPerformance(v)}
            />
          </div>
          <p class="muted small">
            {tr("Nothing you saved is removed when you switch: your shelf, notes and shortcuts stay as they are.")}
          </p>
        </div>
        <div class="card">
          <h2>{tr("World clocks")}</h2>
          <p class="muted">{tr("Up to three cities on the clock card, with daylight saving taken care of.")}</p>
          {#if worldClocks.length}
            <div class="actions left">
              {#each worldClocks as c (c)}
                <button onclick={() => worldChange(false, c)} aria-label={tf('Remove {}', tr(c))}>{tr(c)} ✕</button>
              {/each}
            </div>
          {/if}
          {#if worldClocks.length < 3}
            <span class="label">{tr("Add a city")}</span>
            <Picker
              label={tr("Add a city")}
              value=""
              searchable
              options={[
                { value: '', label: tr(N('Choose…')) },
                ...cities
                  .filter((c) => !worldClocks.includes(c))
                  .map((c) => ({ value: c, label: tr(c) }))
                  .sort((a, b) => a.label.localeCompare(b.label)),
              ]}
              onchange={(v) => v && worldChange(true, v)}
            />
          {/if}
        </div>
        <div class="card">
          <h2>{tr("Quick notes")}</h2>
          <p class="muted">
            {tr("One line at a time, added with the date to a plain text file you can open anywhere:")}
            <span class="path">{notesPath || '“Islet notes.txt” in Documents'}</span>
          </p>
          <form
            class="actions left"
            onsubmit={(e) => {
              e.preventDefault();
              if (quickNote.trim()) void saveQuickNote();
            }}
          >
            <input
              type="text"
              maxlength="500"
              aria-label={tr("Quick note")}
              placeholder={tr("Ask about the essay deadline…")}
              bind:value={quickNote}
            />
            <button class="primary" type="submit" disabled={!quickNote.trim()}>{tr("Save")}</button>
          </form>
          {#if quickNoteSaid}<p class="small" role="status">{quickNoteSaid}</p>{/if}
          <p class="muted small">
            {tr("For a hotkey, bind a key to")} <code>islet note</code> {tr("in your system’s keyboard settings, or run")}
            <code>islet note "your text"</code> {tr("from a launcher.")}
          </p>
        </div>
        <div class="card">
          <h2>{tr("Notes & to-do")}</h2>
          <p class="muted">
            {tr("Type here; tick things off on the island (Control Centre → the checklist button, once it is switched on in General → Clock card buttons). Kept on this computer only.")}
          </p>
          <form
            class="actions left"
            onsubmit={(e) => {
              e.preventDefault();
              if (todoText.trim()) void todoRun(['todo', 'add'], todoText.trim());
            }}
          >
            <input
              type="text"
              maxlength="200"
              aria-label={tr("New note or to-do")}
              placeholder={tr("Buy milk…")}
              bind:value={todoText}
            />
            <button class="primary" type="submit" disabled={!todoText.trim()}>{tr("Add")}</button>
          </form>
          {#if todos.length}
            <ul class="todos">
              {#each todos as t (t.id)}
                <li>
                  <label class:done={t.done}>
                    <input type="checkbox" checked={t.done} onchange={() => todoRun(['todo', 'done', String(t.id)])} />
                    {t.text}
                  </label>
                  <button class="link" aria-label={tf('Delete {}', t.text)} onclick={() => todoRun(['todo', 'remove', String(t.id)])}>
                    ✕
                  </button>
                </li>
              {/each}
            </ul>
            {#if todos.some((t) => t.done)}
              <div class="actions left"><button onclick={() => todoRun(['todo', 'clear-done'])}>{tr("Clear finished")}</button></div>
            {/if}
          {/if}
        </div>
        <div class="card">
          <h2>{tr("Shortcuts")}</h2>
          <p class="muted">
            {tr("Buttons of your own on the island (Control Centre → the bolt button). Each opens a folder, a file, a script or a web page. Up to 50.")}
          </p>
          {#each shortcuts as s, i (i)}
            <div class="net">
              <span class="name">{s.name}</span>
              <span class="muted small target">{s.target}</span>
              <button class="link" aria-label={tf('Delete {}', s.name)} onclick={() => shortcutRun(['shortcut', 'remove', String(i + 1)])}>
                ✕
              </button>
            </div>
          {/each}
          {#if shortcuts.length < 50}
            <div class="two">
              <div>
                <label for="sc-name">{tr("Name")}</label>
                <input id="sc-name" type="text" maxlength="40" placeholder={tr("Homework")} bind:value={shortcutName} />
              </div>
              <div>
                <label for="sc-target">{tr("Opens")}</label>
                <input
                  id="sc-target"
                  type="text"
                  spellcheck="false"
                  placeholder={info?.os === 'windows' ? 'C:\\Users\\you\\Documents or https://…' : '/home/you/Documents or https://…'}
                  bind:value={shortcutTarget}
                />
              </div>
            </div>
            <div class="actions">
              <button
                class="primary"
                disabled={!shortcutName.trim() || !shortcutTarget.trim()}
                onclick={() => shortcutRun(['shortcut', 'add', '--name', shortcutName.trim(), '--target', shortcutTarget.trim()])}
              >
                {tr("Add shortcut")}
              </button>
            </div>
          {/if}
        </div>
        <div class="card">
          <h2>{tr("Stopwatch")}</h2>
          <p class="muted">
            {tr("Switch on its button in General → Clock card buttons, then open the island and tap it. Laps and pause work from the island.")}
          </p>
        </div>
        {#if extrasNote}<p class="small" role="status">{extrasNote}</p>{/if}
      {:else if page === 'updates'}
        <h1>{tr("Updates")}</h1>
        <div class="card row">
          <div>
            <h2>{tr("Check when Settings opens")}</h2>
            <p class="muted">{tr("Off by default: Islet makes no network requests you didn’t ask for.")}</p>
          </div>
          <input
            type="checkbox"
            class="switch"
            aria-label={tr("Check for updates when Settings opens")}
            bind:checked={prefs.checkUpdates}
            onchange={savePrefs}
          />
        </div>
        <div class="actions left"><button onclick={checkUpdate}>{tr("Check for updates now")}</button></div>
        {#if update?.newer}
          <div class="card">
            <h2>Islet {update.latest} {tr("is available")}</h2>
            {#if update.notes}<p class="muted notes">{update.notes}</p>{/if}
            {#if info?.selfUpdate}
              <button class="primary" onclick={installUpdate}>{tr("Install and restart")}</button>
            {:else}
              <button class="primary" onclick={() => api.openLink(update?.url || `${info?.repo}/releases/latest`)}>
                {tr("Open the download page")}
              </button>
            {/if}
          </div>
        {/if}
        {#if updateNote}<p class="small" role="status">{updateNote}</p>{/if}
        <div class="card">
          <h2>{tr("Early builds")}</h2>
          <p class="muted">
            {tr("New features before they are released to everyone. They can have rough edges; you install them by hand from the download page.")}
          </p>
          <div class="actions left">
            <button onclick={checkEarly}>{tr("Look for an early build")}</button>
            {#if early?.newer}
              <button class="primary" onclick={() => early && api.openLink(early.url)}>{tr("Open the download page")}</button>
            {/if}
          </div>
          {#if earlyNote}<p class="small" role="status">{earlyNote}</p>{/if}
        </div>
      {:else}
        <h1>{tr("About Islet")}</h1>
        <p>{tr("Version")} {info?.version}</p>
        <p class="muted">
          {tr("Free and open source under the MIT License. Islet is not affiliated with, endorsed by or sponsored by Apple Inc. “Dynamic Island” is a trademark of Apple Inc. Google Maps is a trademark of Google LLC; Waze is a trademark of Waze Mobile Ltd.")}
        </p>
        <div class="actions left">
          <button onclick={() => api.openLink(info?.repo ?? '')}>{tr("Source code")}</button>
          <button onclick={() => api.openLink(docs('THIRD_PARTY_LICENSES.md'))}>{tr("Third-party licenses")}</button>
          <button onclick={() => api.openLink(`${info?.repo}/issues/new/choose`)}>{tr("Report a problem")}</button>
        </div>
      {/if}
    </main>
  </div>
{/if}

<style>
  .badge {
    font-size: 12px;
    font-weight: 600;
    vertical-align: middle;
    padding: 2px 8px;
    margin-left: 6px;
    border-radius: 999px;
    color: white;
    background: linear-gradient(135deg, #ff9f0a, #ff375f);
  }
  .todos {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
  }
  .todos li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
  }
  .todos label {
    display: flex;
    gap: 8px;
    align-items: center;
    overflow-wrap: anywhere;
  }
  .todos label.done {
    opacity: 0.5;
    text-decoration: line-through;
  }
  .places {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 4px 0 8px;
  }
  .places button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
  }
  .places .attribution {
    margin: 4px 0 0;
  }
  .swatches {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    margin: 6px 0 12px;
  }
  .swatch {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 2px solid transparent;
    padding: 0;
    cursor: pointer;
  }
  .swatch.chosen {
    outline: 2px solid currentColor;
    outline-offset: 2px;
  }
  /* Island styles: each tile previews the island over a bit of wallpaper. */
  .styles {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    margin: 6px 0 12px;
  }
  .style-tile {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 6px 7px;
    border-radius: 12px;
    background: var(--field);
    box-shadow: none;
    font-size: 12.5px;
  }
  .style-tile.chosen {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .preview {
    display: grid;
    place-items: start center;
    height: 46px;
    padding-top: 9px;
    border-radius: 8px;
    background: linear-gradient(135deg, #ffbe78, #f06ea0 55%, #466ee6);
  }
  .mini {
    width: 68%;
    height: 17px;
    border-radius: 9px;
    background: #000;
  }
  .mini.tinted {
    background: color-mix(in srgb, var(--tint) 28%, #060606);
  }
  .mini.glass,
  .mini.tinted-glass {
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.16), transparent 60%), rgba(22, 22, 26, 0.62);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.4);
  }
  .mini.tinted-glass {
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.16), transparent 60%),
      color-mix(in srgb, color-mix(in srgb, var(--tint) 24%, #060606) 66%, transparent);
  }
  .cc-preview {
    place-items: center;
    padding-top: 0;
  }
  .cc-preview svg {
    height: 38px;
    width: auto;
  }
  .tint-pick {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .tint-pick input[type='color'] {
    width: 34px;
    height: 26px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }
  .photo-thumb {
    width: 46px;
    height: 28px;
    object-fit: cover;
    border-radius: 6px;
  }
  .button-like {
    display: inline-block;
    padding: 6px 13px;
    border-radius: 8px;
    border: 1px solid color-mix(in srgb, currentColor 18%, transparent);
    cursor: pointer;
  }
  .button-like.disabled {
    opacity: 0.5;
    pointer-events: none;
  }
  .cc-order {
    list-style: none;
    margin: 4px 0 6px;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .cc-order li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px 4px 10px;
    border-radius: 9px;
    background: var(--field);
  }
  .cc-order li.off {
    opacity: 0.6;
  }
  .cc-order .pos {
    width: 18px;
    color: var(--muted, #888);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .cc-order .name {
    flex: 1;
  }
  .icon-btn {
    width: 28px;
    height: 26px;
    padding: 0;
  }
  .row-end {
    display: flex;
    justify-content: flex-end;
    margin-bottom: 10px;
  }
  .style-name {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-weight: 550;
  }
  .layout {
    display: grid;
    grid-template-columns: 216px 1fr;
    height: 100%;
  }
  nav {
    background: var(--sidebar);
    backdrop-filter: blur(30px) saturate(1.6);
    border-right: 1px solid var(--sidebar-edge);
    padding: 20px 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 700;
    font-size: 17px;
    letter-spacing: -0.01em;
    padding: 0 10px 18px;
  }
  /* Islet's mark: the island itself, with a glint. */
  .logo {
    position: relative;
    width: 38px;
    height: 15px;
    border-radius: 8px;
    background: #000;
    box-shadow:
      0 0 0 0.5px rgba(255, 255, 255, 0.25),
      0 2px 6px rgba(0, 0, 0, 0.25);
  }
  .logo::after {
    content: '';
    position: absolute;
    right: 6px;
    top: 4.5px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 35%, #7fd0ff, #2a6fd6 60%, #0b2a66);
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 9px;
    box-shadow: none;
    background: none;
    text-align: left;
    padding: 6px 8px;
    border-radius: 8px;
    font-weight: 500;
  }
  .tab:hover:not(.active) {
    background: var(--hover);
    filter: none;
  }
  .tab.active {
    background: linear-gradient(180deg, var(--accent-2), var(--accent));
    color: #fff;
    font-weight: 600;
    box-shadow: 0 1px 3px color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .tile {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 6.5px;
    color: #fff;
    flex: none;
    box-shadow:
      inset 0 0.5px 0 rgba(255, 255, 255, 0.35),
      0 0.5px 1.5px rgba(0, 0, 0, 0.25);
  }
  .grow {
    flex: 1;
  }
  .version {
    font-size: 11.5px;
    padding: 0 10px;
  }
  main {
    padding: 30px 34px 40px;
    overflow-y: auto;
  }
  main > :global(*) {
    max-width: 680px;
  }
  main :global(h1) {
    animation: rise 0.22s ease both;
  }
  .welcome {
    max-width: 560px;
    margin: 0 auto;
    padding: 40px 24px;
  }
  .card {
    background: var(--panel);
    box-shadow: var(--shadow);
    border-radius: var(--radius);
    padding: 14px 16px;
    margin: 12px 0;
    animation: rise 0.28s ease both;
  }
  /* An open dropdown's list overlaps the cards below it. */
  .card:has(:global(.picker.open)) {
    position: relative;
    z-index: 20;
  }
  .card p:last-child {
    margin-bottom: 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .three {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 12px;
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 12px;
    flex-wrap: wrap;
  }
  .actions.left {
    justify-content: flex-start;
  }
  .actions + p {
    margin-top: 10px;
  }
  /* A text field and its button on one line. */
  form.actions {
    flex-wrap: nowrap;
    margin-top: 8px;
  }
  form.actions input {
    flex: 1;
    min-width: 0;
  }
  .small {
    font-size: 12.5px;
  }
  .notes {
    white-space: pre-wrap;
  }
  .steps {
    text-align: center;
    margin-top: 28px;
    letter-spacing: 6px;
    font-size: 10px;
  }
  .steps .on {
    color: var(--accent);
  }
  .net {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    border-top: 1px solid var(--line);
  }
  .net .name {
    flex: 1;
    font-weight: 550;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* A row of choices, the chosen one raised (size, °C / °F). */
  .segmented {
    display: inline-flex;
    flex: none;
    gap: 2px;
    padding: 2px;
    border-radius: 9px;
    background: var(--field);
  }
  .segmented button {
    box-shadow: none;
    background: transparent;
    padding: 4px 14px;
    border-radius: 7px;
    color: var(--muted);
    font-weight: 550;
  }
  .segmented button.chosen {
    background: var(--panel);
    color: var(--text);
    box-shadow:
      0 0 0 0.5px var(--line),
      0 1px 2px rgba(0, 0, 0, 0.12);
  }
  .net :global(.picker) {
    flex: none;
  }
  .net input[type='range'] {
    width: 220px;
    flex: none;
  }
  .path {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .pairing {
    display: flex;
    gap: 18px;
    align-items: center;
    margin: 10px 0;
  }
  .pairing .qr {
    flex: none;
    width: 190px;
    height: 190px;
    border-radius: 10px;
    shape-rendering: crispEdges;
  }
  .pairing ol {
    margin: 0;
    padding-left: 20px;
    line-height: 1.7;
  }
  .launcher-input {
    width: 100%;
    font-size: 17px;
    padding: 10px 12px;
    margin-bottom: 6px;
  }
  .speed {
    display: flex;
    gap: 24px;
    margin: 4px 0 8px;
  }
  .speed b {
    font-size: 1.4em;
    font-variant-numeric: tabular-nums;
  }
  .launcher-row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    width: 100%;
    padding: 9px 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    box-shadow: none;
  }
  button.launcher-row:hover,
  button.launcher-row:focus-visible {
    background: color-mix(in srgb, var(--accent, #0a84ff) 14%, transparent);
  }
  .launcher-row .name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .launcher-row.answer .name {
    font-size: 20px;
  }
  h3.day {
    margin: 14px 0 2px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .net .when {
    flex: none;
    width: 172px;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
    color: var(--muted);
  }
  .net .target {
    flex: 2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }
  .bars {
    font-family: ui-monospace, monospace;
    color: var(--muted);
    letter-spacing: -1px;
  }
  .join {
    display: flex;
    gap: 8px;
    padding: 4px 0 10px;
  }
  .label {
    display: block;
    font-weight: 550;
    margin: 10px 0 4px;
  }
  .days {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  .day {
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 50%;
  }
  .day.on {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }
  /* On/off switches: a sliding knob, green when on. */
  .switch {
    appearance: none;
    position: relative;
    width: 42px;
    height: 26px;
    flex: none;
    margin: 0;
    padding: 0;
    border-radius: 13px;
    background: var(--off);
    cursor: pointer;
    transition: background 0.2s ease;
  }
  .switch:hover {
    background: var(--off);
  }
  .switch::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: #fff;
    box-shadow:
      0 0 0 0.5px rgba(0, 0, 0, 0.06),
      0 2px 4px rgba(0, 0, 0, 0.22);
    transition: transform 0.22s cubic-bezier(0.3, 1.25, 0.5, 1);
  }
  .switch:checked,
  .switch:checked:hover {
    background: var(--on);
  }
  .switch:checked::after {
    transform: translateX(16px);
  }
  .switch:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
