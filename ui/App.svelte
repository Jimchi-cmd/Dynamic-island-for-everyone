<script lang="ts">
  import { onMount } from 'svelte';
  import { asLink, calculate, convert, formatNumber, matchApps, webSearchUrl, type AppEntry } from './lib/launcher';
  import * as api from './lib/bridge';
  import type { Info, Prefs, UpdateInfo } from './lib/bridge';
  import {
    ACCENTS,
    CARD_BUTTONS,
    STYLES,
    type PlaceSuggestion,
    alarmAddArgs,
    DAY_LETTERS,
    describeDays,
    formatAlarmTime,
    keyLooksValid,
    MODES,
    navStartArgs,
    parsePage,
    parsePro,
    type ProStatus,
    permissionsFor,
    signalBars,
    SWITCH_NAMES,
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

  type Page = 'general' | 'connections' | 'alarms' | 'timer' | 'navigation' | 'launcher' | 'pro' | 'updates' | 'about';
  // Each section's icon is a few strokes on a 24×24 grid, drawn for Islet,
  // on a coloured tile (`tint`).
  const PAGES: { id: Page; label: string; tint: string; icon: string; dots?: [number, number, number][] }[] = [
    {
      id: 'general',
      label: 'General',
      tint: 'linear-gradient(160deg, #a2a2a9, #77777e)',
      icon: 'M4 8h8M18 8h2M4 16h2M12 16h8',
      dots: [
        [15, 8, 2.2],
        [9, 16, 2.2],
      ],
    },
    {
      id: 'connections',
      label: 'Wi-Fi & Bluetooth',
      tint: 'linear-gradient(160deg, #46a4ff, #0a6fe8)',
      icon: 'M4.5 10.5a10.6 10.6 0 0 1 15 0M7.8 14a6 6 0 0 1 8.4 0',
      dots: [[12, 17.6, 1.5]],
    },
    {
      id: 'alarms',
      label: 'Alarms',
      tint: 'linear-gradient(160deg, #ffb340, #ff8a00)',
      icon: 'M6.5 16.5v-5.2a5.5 5.5 0 0 1 11 0v5.2l1.5 1.8h-14zM10.4 20.4a1.7 1.7 0 0 0 3.2 0',
    },
    {
      id: 'timer',
      label: 'Timer & focus',
      tint: 'linear-gradient(160deg, #8a86ff, #5a55e0)',
      icon: 'M12 6.5a7 7 0 1 0 0 14a7 7 0 0 0 0-14zM12 13.5V9.8M10 3.2h4',
    },
    {
      id: 'navigation',
      label: 'Navigation',
      tint: 'linear-gradient(160deg, #4cd97b, #1fa64d)',
      icon: 'M5.5 19.5L12 4.5l6.5 15l-6.5-4z',
    },
    {
      id: 'launcher',
      label: 'Launcher',
      tint: 'linear-gradient(160deg, #ff7a8a, #e0355a)',
      icon: 'M10.5 4.5a6 6 0 1 0 0 12a6 6 0 0 0 0-12zM15 15l4.5 4.5',
    },
    {
      id: 'pro',
      label: 'Islet Pro',
      tint: 'linear-gradient(135deg, #ff9f0a, #ff375f)',
      icon: 'M12 3.5l2 6.5l6.5 2l-6.5 2l-2 6.5l-2-6.5l-6.5-2l6.5-2z',
    },
    {
      id: 'updates',
      label: 'Updates',
      tint: 'linear-gradient(160deg, #54d3e6, #1a9fbd)',
      icon: 'M5 11.5a7 7 0 0 1 12.3-4.2L19.5 9.5M19.5 4.5v5h-5M19 12.5a7 7 0 0 1-12.3 4.2L4.5 14.5M4.5 19.5v-5h5',
    },
    {
      id: 'about',
      label: 'About',
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
    connNote = `Connecting to ${ssid}…`;
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
    // General needs to know about Pro too, for the Pro-only island styles.
    if ((page === 'pro' || page === 'general') && !pro) void loadPro();
    if (page === 'timer') {
      void loadStreak();
      void loadLessons();
    }
    if (page === 'launcher') {
      if (!apps.length) void loadApps();
      // Type straight away, as in any launcher.
      queueMicrotask(() => launcherInput?.focus());
    }
  });

  // Quick launcher: apps, web searches, sums and unit conversions.
  let apps = $state<AppEntry[]>([]);
  let launcherText = $state('');
  let launcherNote = $state('');
  let launcherInput: HTMLInputElement | null = $state(null);
  type LauncherRow = { kind: 'answer' | 'app' | 'link' | 'web'; title: string; detail: string; run?: () => Promise<void> | void };
  const launcherRows = $derived.by((): LauncherRow[] => {
    const q = launcherText.trim();
    if (!q) return [];
    const rows: LauncherRow[] = [];
    const sum = calculate(q);
    if (sum !== null) rows.push({ kind: 'answer', title: formatNumber(sum), detail: `${q} =` });
    const converted = convert(q);
    if (converted) rows.push({ kind: 'answer', title: converted.split(' = ')[1] ?? converted, detail: converted });
    for (const a of matchApps(apps, q)) {
      rows.push({ kind: 'app', title: a.name, detail: 'App', run: () => api.launchApp(a.path) });
    }
    const link = asLink(q);
    if (link) rows.push({ kind: 'link', title: link, detail: 'Open in your browser', run: () => api.openLink(link) });
    rows.push({ kind: 'web', title: `Search the web for “${q}”`, detail: 'DuckDuckGo', run: () => api.openLink(webSearchUrl(q)) });
    return rows;
  });

  async function loadApps() {
    try {
      apps = await api.listApps();
    } catch (e) {
      launcherNote = api.message(e);
    }
  }

  async function runLauncherRow(row: LauncherRow | undefined) {
    if (!row?.run) return;
    try {
      await row.run();
      launcherText = '';
      launcherNote = '';
    } catch (e) {
      launcherNote = api.message(e);
    }
  }

  // The island's own preferences (island.json via the CLI)
  let islandPrefs: IslandPrefs | null = $state(null);

  let displays = $state<string[]>([]);

  async function loadIslandPrefs() {
    try {
      islandPrefs = JSON.parse(await api.islet(['prefs', '--json']));
      displays = JSON.parse(await api.islet(['displays', '--json']));
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

  async function setLook(what: 'accent' | 'style' | 'shape' | 'clock' | 'size' | 'offset' | 'display', value: string) {
    try {
      await api.islet(['look', what, value]);
      await loadIslandPrefs();
    } catch (e) {
      problem = api.message(e);
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
    spotifyNote = 'Finish signing in in your browser…';
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
    weatherNote = 'Searching…';
    try {
      cityResults = JSON.parse(await api.islet(['weather', 'search', cityQuery.trim(), '--json']));
      weatherNote = cityResults.length ? '' : 'No places found.';
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

  // Islet Pro
  let pro: ProStatus | null = $state(null);
  let proKey = $state('');
  let proNote = $state('');

  // World clocks (Pro)
  let worldClocks: string[] = $state([]);
  let cities: string[] = $state([]);
  let cityPick = $state('');

  async function loadWorld() {
    try {
      worldClocks = JSON.parse(await api.islet(['world', '--json']));
      if (!cities.length) cities = JSON.parse(await api.islet(['world', 'cities', '--json']));
    } catch (e) {
      proNote = api.message(e);
    }
  }

  async function worldChange(add: boolean, city: string) {
    if (!city) return;
    proNote = '';
    try {
      await api.islet(['world', add ? 'add' : 'remove', city]);
      cityPick = '';
    } catch (e) {
      proNote = api.message(e);
    }
    await loadWorld();
  }

  // Notes and to-dos (Pro)
  let todos: { id: number; text: string; done: boolean }[] = $state([]);
  let todoText = $state('');

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
      proNote = api.message(e);
    }
  }

  async function todoRun(args: string[], input?: string) {
    proNote = '';
    try {
      await api.islet(args, input);
      todoText = '';
    } catch (e) {
      proNote = api.message(e);
    }
    await loadTodos();
  }

  // Your own buttons (Pro)
  let shortcuts: { name: string; target: string }[] = $state([]);
  let shortcutName = $state('');
  let shortcutTarget = $state('');

  async function loadShortcuts() {
    try {
      shortcuts = JSON.parse(await api.islet(['shortcut', '--json']));
    } catch (e) {
      proNote = api.message(e);
    }
  }

  async function shortcutRun(args: string[]) {
    proNote = '';
    try {
      await api.islet(args);
      shortcutName = '';
      shortcutTarget = '';
    } catch (e) {
      proNote = api.message(e);
    }
    await loadShortcuts();
  }

  async function loadPro() {
    void loadWorld();
    void loadTodos();
    void loadNotesPath();
    void loadShortcuts();
    try {
      pro = parsePro(await api.islet(['pro', 'status', '--json']));
    } catch (e) {
      proNote = api.message(e);
    }
  }

  async function proRun(args: string[], input?: string) {
    proNote = '';
    busy = true;
    try {
      proNote = await api.islet(args, input);
      proKey = '';
    } catch (e) {
      proNote = api.message(e);
    } finally {
      busy = false;
      await loadPro();
    }
  }

  async function saveKey() {
    keyNote = '';
    try {
      await api.islet(['key', 'routes'], key.trim());
      key = '';
      keyNote = 'Saved to your keychain.';
      await refresh();
    } catch (e) {
      keyNote = api.message(e);
    }
  }

  async function clearKey() {
    keyNote = '';
    try {
      await api.islet(['key', 'routes', '--clear']);
      keyNote = 'Removed.';
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
      tripNote = 'Stopped.';
      await refresh();
    } catch (e) {
      tripNote = api.message(e);
    }
  }

  async function checkUpdate() {
    updateNote = 'Checking…';
    try {
      update = await api.checkUpdate();
      updateNote = update.newer ? '' : `Islet ${update.current} is up to date.`;
      void updatePill();
    } catch (e) {
      update = null;
      updateNote = api.message(e);
    }
  }

  // Early builds (Islet Pro): the newest release of all, pre-releases included.
  let early = $state<{ current: string; latest: string; newer: boolean; url: string } | null>(null);
  let earlyNote = $state('');
  async function checkEarly() {
    earlyNote = 'Looking…';
    try {
      const v = JSON.parse(await api.islet(['update', 'check', '--early', '--json']));
      const url = typeof v.url === 'string' && v.url.startsWith('https://') ? v.url : '';
      early = { current: String(v.current), latest: String(v.latest), newer: v.newer === true && url !== '', url };
      earlyNote = early.newer ? `Islet ${early.latest} is out as an early build.` : 'No early build newer than yours.';
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
          '--title', `Islet ${update.latest} is available`,
          '--subtitle', 'Tap to open the download page',
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
    updateNote = 'Downloading and installing…';
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
    try {
      [info, prefs] = await Promise.all([api.info(), api.getPrefs()]);
    } catch (e) {
      problem = api.message(e);
    }
    await refresh();
    await loadIslandPrefs();
    const wanted = parsePage(await api.initialPage().catch(() => null));
    if (wanted?.page === 'launcher') page = 'launcher';
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

{#if info && !prefs.welcomed}
  <main class="welcome" aria-live="polite">
    {#if welcomeStep === 0}
      <h1>Welcome to Islet</h1>
      <p>
        Islet adds a Dynamic Island-style pill to the top of your screen. It shows what’s playing, the battery of
        headphones and mice when they connect, and your ETA when you’re heading somewhere.
      </p>
      <div class="card">
        <h2>Private by default</h2>
        <p class="muted">
          No accounts, no analytics, no telemetry. Islet uses the network only for features you turn on: navigation
          ETA (with your own Google key), weather, lyrics and update checks.
        </p>
      </div>
      <div class="actions"><button class="primary" onclick={() => (welcomeStep = 1)}>Continue</button></div>
    {:else if welcomeStep === 1}
      <h1>What Islet may ask for</h1>
      {#each permissionsFor(info.os) as p (p.title)}
        <div class="card">
          <h2>{p.title}</h2>
          <p class="muted">{p.body}</p>
        </div>
      {/each}
      <div class="actions">
        <button onclick={() => (welcomeStep = 0)}>Back</button>
        <button class="primary" onclick={() => (welcomeStep = 2)}>Continue</button>
      </div>
    {:else}
      <h1>Start Islet</h1>
      <p>Islet uses a few megabytes of memory and no CPU while nothing is happening, so it can run all the time.</p>
      <div class="actions">
        <button disabled={busy} onclick={() => finishWelcome(false)}>Start, just this once</button>
        <button class="primary" disabled={busy} onclick={() => finishWelcome(true)}>Start now and at login</button>
      </div>
      {#if problem}<p class="error" role="alert">{problem}</p>{/if}
    {/if}
    <p class="steps muted" aria-label="Step {welcomeStep + 1} of 3">
      {#each [0, 1, 2] as i (i)}<span class:on={i === welcomeStep}>●</span>{/each}
    </p>
  </main>
{:else}
  <div class="layout">
    <nav aria-label="Settings sections">
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
          {p.label}
        </button>
      {/each}
      <div class="grow"></div>
      {#if info}<div class="version muted">Version {info.version}</div>{/if}
    </nav>
    <main>
      {#if problem}<p class="error" role="alert">{problem}</p>{/if}

      {#if page === 'general'}
        <h1>General</h1>
        <div class="card row">
          <div>
            <h2>Island</h2>
            {#if status?.running}<p class="ok">Running</p>{:else}<p class="muted">Not running</p>{/if}
          </div>
          {#if status?.running}
            <button disabled={busy} onclick={() => run(async () => { await api.islet(['quit']); await refresh(); })}>
              Quit Islet
            </button>
          {:else}
            <button class="primary" disabled={busy} onclick={startIsland}>Start Islet</button>
          {/if}
        </div>
        <div class="card row">
          <div>
            <h2>Start at login</h2>
            <p class="muted">Islet opens by itself when you log in.</p>
          </div>
          <input
            type="checkbox"
            class="switch"
            aria-label="Start at login"
            checked={status?.autostart ?? false}
            disabled={busy || !status}
            onchange={(e) => setAutostart(e.currentTarget.checked)}
          />
        </div>
        <div class="card">
          <h2>Pop-ups</h2>
          <p class="muted small">The island shows a moment's notice when these change.</p>
          {#each [['volumeHud', 'Volume'], ['brightnessHud', 'Screen brightness'], ['capsLockHud', 'Caps Lock']] as [key, label] (key)}
            <div class="net">
              <span class="name">{label}</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="{label} pop-up"
                checked={islandPrefs ? Boolean(islandPrefs[key as keyof IslandPrefs]) : true}
                disabled={!islandPrefs}
                onchange={(e) => setSwitch(key as keyof IslandPrefs, e.currentTarget.checked)}
              />
            </div>
          {/each}
          {#if info?.os === 'macos'}
            <p class="muted small">macOS doesn't let apps see brightness changes, so there's no brightness pop-up on a Mac.</p>
          {:else if info?.os === 'windows'}
            <p class="muted small">
              On Windows the Caps Lock pop-up uses a keyboard hook that checks only the Caps Lock key. It's removed when
              you turn this off.
            </p>
          {/if}
        </div>
        <div class="card">
          <h2>Now Playing</h2>
          <div class="net">
            <span class="name">Show synced lyrics</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Show synced lyrics"
              checked={islandPrefs?.lyrics ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('lyrics', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            The line being sung lights up in the island as the song plays. Lyrics come from LRCLIB (lrclib.net), a free,
            open lyrics database: when this is on, Islet sends it the title, artist, album and length of the song that's
            playing, and nothing else. Songs without timed lyrics show none.
          </p>
          {#if info?.os === 'linux'}
            <div class="net">
              <span class="name">Download web artwork (e.g. Spotify covers)</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="Download web artwork"
                checked={islandPrefs?.webArtwork ?? false}
                disabled={!islandPrefs}
                onchange={(e) => setSwitch('webArtwork', e.currentTarget.checked)}
              />
            </div>
          {/if}
          <p class="muted small">Hide music and videos from these apps:</p>
          {#each islandPrefs?.mediaExclude ?? [] as app (app)}
            <div class="net">
              <span class="name">{app}</span>
              <button onclick={() => mediaVisibility(false, app)}>Show again</button>
            </div>
          {/each}
          <form class="join" onsubmit={(e) => { e.preventDefault(); void mediaVisibility(true, hideApp); }}>
            <input placeholder="App name, e.g. Firefox" bind:value={hideApp} aria-label="App to hide" />
            <button type="submit" disabled={!hideApp.trim()}>Hide</button>
          </form>
        </div>
        <div class="card">
          <h2>Look</h2>
          <div class="net">
            <span class="name">Show the island only when I point at it</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Show the island only when I point at it"
              checked={islandPrefs?.autoHide ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('autoHide', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            The empty island stays hidden until you move the pointer to the top of the screen. Music, timers, calls for
            attention and other live activities always show.
          </p>
          <div class="net">
            <span class="name">Show the time when nothing is playing</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Show the time when nothing is playing"
              checked={islandPrefs?.idleClock ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('idleClock', e.currentTarget.checked)}
            />
          </div>
          <div class="net">
            <span class="name">Tell me when a program slows the computer down</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Tell me when a program slows the computer down"
              checked={islandPrefs?.performanceAlert ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('performanceAlert', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            When one program has had more than half the processor for a minute, or 40% of the memory, the island says
            so and offers to close it. Islet looks at the running programs every 20 seconds while this is on.
          </p>
          <div class="net">
            <span class="name">Show weather, battery and processor when you point at it</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Show weather, battery and processor when you point at the island"
              checked={islandPrefs?.hoverStatus ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('hoverStatus', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">Accent colour for buttons and switches on the island.</p>
          <div class="swatches" role="radiogroup" aria-label="Accent colour">
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
          <p class="muted small">Buttons on the card that opens when you click the island. Wi-Fi and Bluetooth always show.</p>
          {#each CARD_BUTTONS.filter((b) => !b.pro || pro?.active) as b (b.id)}
            <div class="net">
              <span class="name">{b.label}</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="Show {b.label} on the island's card"
                checked={!(islandPrefs?.cardHidden ?? []).includes(b.id)}
                disabled={!islandPrefs}
                onchange={(e) => setCardButton(b.id, e.currentTarget.checked)}
              />
            </div>
          {/each}
          <p class="muted small">What the island is made of.</p>
          <div class="styles" role="radiogroup" aria-label="Island style">
            {#each STYLES as s (s.id)}
              {@const chosen = (islandPrefs?.style ?? 'black') === s.id}
              {@const accent = ACCENTS[islandPrefs?.accent ?? 'blue'] ?? ACCENTS.blue}
              <button
                class="style-tile"
                class:chosen
                role="radio"
                aria-checked={chosen}
                aria-label={s.pro && !pro?.active ? `${s.label} (Islet Pro)` : s.label}
                disabled={!islandPrefs}
                onclick={() => (s.pro && !pro?.active ? (page = 'pro') : setLook('style', s.id))}
              >
                <span class="preview" style:--tint={accent}>
                  <span class="mini {s.id}"></span>
                </span>
                <span class="style-name">
                  {s.label}
                  {#if s.pro && !pro?.active}<span class="pro-tag">Pro</span>{/if}
                </span>
              </button>
            {/each}
          </div>
          <div class="net">
            <span class="name">Shape {#if !pro?.active}<span class="pro-tag">Pro</span>{/if}</span>
            <select
              aria-label="Island shape"
              value={islandPrefs?.shape ?? 'pill'}
              disabled={!islandPrefs}
              onchange={(e) => (pro?.active ? setLook('shape', e.currentTarget.value) : (page = 'pro'))}
            >
              <option value="pill">Pill</option>
              <option value="notch">Notch (square at the top)</option>
            </select>
          </div>
          <div class="net">
            <span class="name">Empty island shows {#if !pro?.active}<span class="pro-tag">Pro</span>{/if}</span>
            <select
              aria-label="What the empty island shows"
              value={islandPrefs?.clockFace ?? 'time'}
              disabled={!islandPrefs}
              onchange={(e) => (pro?.active ? setLook('clock', e.currentTarget.value) : (page = 'pro'))}
            >
              <option value="time">The time</option>
              <option value="date">The day and the time</option>
              <option value="seconds">The time with seconds</option>
            </select>
          </div>
          {#if pro?.active}
            {#each [['albumGlow', 'Glow in the album’s colour while music plays'], ['visualizer', 'Moving bars as the song’s progress bar'], ['screenshotPreview', 'Show each new screenshot on the island']] as [key, label] (key)}
              <div class="net">
                <span class="name">{label}</span>
                <input
                  type="checkbox"
                  class="switch"
                  aria-label={label}
                  checked={islandPrefs ? Boolean(islandPrefs[key as keyof IslandPrefs]) : true}
                  disabled={!islandPrefs}
                  onchange={(e) => setSwitch(key as keyof IslandPrefs, e.currentTarget.checked)}
                />
              </div>
            {/each}
          {/if}
          <div class="net">
            <span class="name">Size</span>
            <select
              aria-label="Island size"
              value={islandPrefs?.size ?? 'medium'}
              disabled={!islandPrefs}
              onchange={(e) => setLook('size', e.currentTarget.value)}
            >
              <option value="small">Small</option>
              <option value="medium">Medium</option>
              <option value="large">Large</option>
            </select>
          </div>
          {#if displays.length > 1}
            <div class="net">
              <span class="name">Display</span>
              <select
                aria-label="Display for the island"
                value={islandPrefs?.display || 'primary'}
                disabled={!islandPrefs}
                onchange={(e) => setLook('display', e.currentTarget.value)}
              >
                <option value="primary">Primary display</option>
                <option value="all">All displays</option>
                {#each displays as d (d)}
                  <option value={d}>{d}</option>
                {/each}
              </select>
            </div>
          {/if}
          <div class="net">
            <span class="name">Gap above the island</span>
            <input
              type="range"
              min="0"
              max="40"
              step="2"
              aria-label="Gap above the island, in pixels"
              value={islandPrefs?.offset ?? 0}
              disabled={!islandPrefs}
              onchange={(e) => setLook('offset', e.currentTarget.value)}
            />
            <span class="muted small">{islandPrefs?.offset ?? 0} px</span>
          </div>
          <p class="muted small">Islet restarts itself to apply a new size, gap or display. Notched Macs always hang the island from the notch.</p>
        </div>
        <div class="card">
          <h2>Shortcut</h2>
          <p class="muted small">
            A key combination that opens the island, or closes it. Current:
            <strong>{islandPrefs?.hotkey || 'none'}</strong>
          </p>
          <form class="join" onsubmit={(e) => { e.preventDefault(); void saveHotkey(hotkeyDraft); }}>
            <input
              placeholder="Click here, then press e.g. Ctrl+Alt+I"
              value={hotkeyDraft}
              onkeydown={captureHotkey}
              readonly
              aria-label="Press the shortcut keys"
            />
            <button type="submit" disabled={!hotkeyDraft}>Save</button>
            {#if islandPrefs?.hotkey}
              <button type="button" onclick={() => saveHotkey('')}>Remove</button>
            {/if}
          </form>
          {#if info?.os === 'linux'}
            <p class="muted small">
              On native Wayland (KDE Plasma, sway, Hyprland) only the desktop can watch keys: add a shortcut in its
              keyboard settings that runs <code>islet toggle</code>.
            </p>
          {/if}
        </div>
        <div class="card">
          <h2>Privacy</h2>
          <div class="net">
            <span class="name">Camera and microphone dots</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Show camera and microphone dots"
              checked={islandPrefs?.privacyDots ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('privacyDots', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">A green dot while an app uses the camera, an orange one for the microphone.</p>
          <div class="net">
            <span class="name">Calls (when an app uses the microphone)</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Show calls when an app uses the microphone"
              checked={islandPrefs?.calls ?? true}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('calls', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            While Zoom, Teams, a browser call or any app has the microphone, the island shows how long the call has
            been going and a button that mutes your microphone.
          </p>
          <div class="net">
            <span class="name">Downloads in progress</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Show downloads in progress"
              checked={islandPrefs?.downloads ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('downloads', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            Shows what your browser is downloading (from your Downloads folder), then "Downloaded": tap it to open the
            file. Only file names and sizes are read, on this computer.{#if info?.os === 'macos'} macOS asks once
            whether Islet may see your Downloads folder.{/if}
          </p>
          <div class="net">
            <span class="name">Clipboard history</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Keep clipboard history"
              checked={islandPrefs?.clipboard ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('clipboard', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            Keeps the last 20 texts you copy, in memory only: nothing is saved to disk, and the list is gone when Islet
            quits. Copies that password managers mark as secret are skipped.{#if info?.os === 'macos'} macOS has no
            clipboard notifications, so while this is on Islet checks the clipboard twice a second.{/if}
          </p>
        </div>
        <div class="card">
          <h2>Spotify on your other devices</h2>
          <p class="muted small">
            Optional. Shows what's playing on your phone or a speaker through Spotify Connect, with controls (controls need
            Premium). It uses your own Spotify app and checks every 10 seconds while something plays, every minute
            otherwise.
          </p>
          {#if status?.spotifyLinked}
            <div class="net">
              <span class="name ok">Linked</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="Show Spotify playback from other devices"
                checked={islandPrefs?.spotify ?? false}
                onchange={(e) => spotifyCmd(e.currentTarget.checked ? 'on' : 'off')}
              />
            </div>
            <div class="actions left"><button class="link" onclick={() => spotifyCmd('logout')}>Unlink</button></div>
          {:else}
            <details class="small muted">
              <summary>How to set it up</summary>
              <p>1. Open <button class="link" onclick={() => api.openLink('https://developer.spotify.com/dashboard')}>developer.spotify.com/dashboard</button> and create an app (any name), choosing "Web API".</p>
              <p>2. Add this redirect URI exactly: <code>http://127.0.0.1:43821/callback</code></p>
              <p>3. Copy the app's Client ID here and sign in.</p>
            </details>
            <form class="join" onsubmit={(e) => { e.preventDefault(); if (/^[A-Za-z0-9]{1,64}$/.test(spotifyClient.trim())) void spotifyLogin(); }}>
              <input placeholder="Client ID" bind:value={spotifyClient} aria-label="Spotify client ID" autocomplete="off" />
              <button class="primary" type="submit" disabled={!/^[A-Za-z0-9]{1,64}$/.test(spotifyClient.trim())}>Sign in</button>
            </form>
          {/if}
          {#if spotifyNote}<p class="small" role="status">{spotifyNote}</p>{/if}
        </div>
        <div class="card">
          <h2>Notifications</h2>
          {#if info?.os === 'linux' || info?.os === 'windows'}
            <div class="net">
              <span class="name">Show notifications on the island</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="Show notifications on the island"
                checked={islandPrefs?.notifications ?? false}
                disabled={!islandPrefs}
                onchange={(e) => setSwitch('notifications', e.currentTarget.checked)}
              />
            </div>
            <p class="muted small">
              New notifications appear on the island for a few seconds. Click it to see the latest ones. They stay on this
              computer and are never logged.
            </p>
            {#if info?.os === 'windows'}
              <p class="muted small">
                Windows lets only packaged apps read other apps' notifications, so this needs the MSIX version of Islet.
                Windows then asks once; you can change it later in Settings → Privacy &amp; security → Notifications.
              </p>
            {/if}
          {:else}
            <p class="muted small">macOS doesn't let apps read other apps' notifications, so this isn't available on a Mac.</p>
          {/if}
        </div>
        <div class="card">
          <h2>Weather</h2>
          <p class="muted small">
            Shows on the clock card. Off until you choose a place. Then Islet asks Open-Meteo once an hour (no account or
            key; it sees only the place's coordinates).
          </p>
          {#if islandPrefs?.weather}
            <div class="net">
              <span class="name ok">{islandPrefs.weather.name}</span>
              <button onclick={weatherOff}>Turn off</button>
            </div>
            <div class="net">
              <span class="name">Fahrenheit</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="Show temperatures in Fahrenheit"
                checked={islandPrefs.fahrenheit}
                onchange={(e) => setSwitch('fahrenheit', e.currentTarget.checked)}
              />
            </div>
          {/if}
          <form class="join" onsubmit={(e) => { e.preventDefault(); if (cityQuery.trim() && !cityQuery.trim().startsWith('-')) void searchCity(); }}>
            <input placeholder="City, e.g. Kampala" bind:value={cityQuery} aria-label="City" />
            <button type="submit" disabled={!cityQuery.trim()}>Search</button>
          </form>
          {#each cityResults as p (p.name + p.lat)}
            <div class="net">
              <span class="name">{p.name}</span>
              <button class="primary" onclick={() => chooseCity(p)}>Use</button>
            </div>
          {/each}
          {#if weatherNote}<p class="small" role="status">{weatherNote}</p>{/if}
          <p class="muted small">Weather data by <button class="link" onclick={() => api.openLink('https://open-meteo.com/')}>Open-Meteo.com</button> (CC BY 4.0).</p>
        </div>
        <div class="card">
          <h2>Calendar</h2>
          <p class="muted small">
            Ten minutes before an event, the island shows what's next. Islet reads your calendar's private iCal link
            every 15 minutes while this is on. The link is kept in your keychain.
          </p>
          {#if islandPrefs}
            <div class="net">
              <span class="name">Show upcoming events</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="Show upcoming events"
                checked={islandPrefs.calendar}
                onchange={(e) => calendarCmd(e.currentTarget.checked ? 'on' : 'off')}
              />
            </div>
          {/if}
          <form class="join" onsubmit={(e) => { e.preventDefault(); if (calLink.trim()) void saveCalendar(); }}>
            <input type="password" autocomplete="off" placeholder="https://… or webcal://… (.ics)" bind:value={calLink} aria-label="Calendar link" />
            <button class="primary" type="submit" disabled={!calLink.trim()}>Save link</button>
          </form>
          <details class="small muted">
            <summary>Where do I find the link?</summary>
            <p><strong>Google Calendar:</strong> Settings → your calendar → Integrate calendar → "Secret address in iCal format".</p>
            <p><strong>Outlook:</strong> Settings → Calendar → Shared calendars → Publish a calendar → copy the ICS link.</p>
            <p><strong>iCloud:</strong> Calendar → share icon next to a calendar → Public Calendar → copy the link.</p>
            <p>Anyone with the link can read the calendar, so keep it private.</p>
          </details>
          <div class="actions left"><button class="link" onclick={() => calendarCmd('clear')}>Remove saved link</button></div>
          {#if calNote}<p class="small" role="status">{calNote}</p>{/if}
        </div>
        <p class="muted small">Right-click the island to close it. Click it to open, and press and hold for more.</p>
      {:else if page === 'connections'}
        <h1>Wi-Fi &amp; Bluetooth</h1>
        <p class="muted">The same lists open from the island: click it, then tap Wi-Fi or Bluetooth on the clock card.</p>
        <div class="card">
          <div class="row">
            <h2>Wi-Fi</h2>
            <button disabled={scanning} onclick={() => loadConnections(true)}>{scanning ? 'Searching…' : 'Refresh'}</button>
          </div>
          {#if wifi?.note}<p class="muted small">{wifi.note}</p>{/if}
          {#each wifi?.networks ?? [] as n (n.ssid)}
            <div class="net">
              <span class="bars" aria-hidden="true">{signalBars(n.strength)}</span>
              <span class="name" class:ok={n.active}>{n.ssid}</span>
              {#if n.active}
                <span class="ok small">Connected</span>
              {:else}
                <button onclick={() => joinWifi(n.ssid, n.secure && !n.known)}>
                  {n.secure && !n.known ? 'Join…' : 'Join'}
                </button>
              {/if}
            </div>
            {#if joining === n.ssid && !n.active}
              <form class="join" onsubmit={(e) => { e.preventDefault(); void joinWifi(n.ssid, true); }}>
                <input type="password" autocomplete="off" placeholder="Password for {n.ssid}" bind:value={wifiPassword} />
                <button class="primary" type="submit" disabled={!wifiPassword}>Join</button>
              </form>
            {/if}
          {:else}
            <p class="muted">{scanning ? 'Looking for networks…' : 'No networks found.'}</p>
          {/each}
        </div>
        <div class="card">
          <h2>Bluetooth</h2>
          {#if bt?.note}<p class="muted small">{bt.note}</p>{/if}
          {#each bt?.devices ?? [] as d (d.id)}
            <div class="net">
              <span class="name" class:ok={d.connected}>{d.name}</span>
              <button onclick={() => toggleBt(d.id, d.connected)}>
                {bt?.canConnect ? (d.connected ? 'Disconnect' : 'Connect') : 'Open settings'}
              </button>
            </div>
          {:else}
            <p class="muted">No paired devices. Pair new ones in your system's Bluetooth settings.</p>
          {/each}
        </div>
        {#if connNote}<p class="small" role="status">{connNote}</p>{/if}
      {:else if page === 'timer'}
        <h1>Timer &amp; focus</h1>
        <p class="muted">Timers count down on the island. When one ends, the island opens and chimes until you stop it.</p>
        <div class="card">
          <h2>Timer</h2>
          <div class="actions left">
            {#each [1, 5, 10, 15, 25, 60] as m (m)}
              <button onclick={() => startTimer(m)}>{m < 60 ? `${m} min` : '1 hour'}</button>
            {/each}
          </div>
          <div class="two">
            <div>
              <label for="tmin">Minutes</label>
              <input id="tmin" type="number" min="1" max="1440" bind:value={timerMinutes} />
            </div>
            <div>
              <label for="tlabel">Name (optional)</label>
              <input id="tlabel" bind:value={timerLabel} placeholder="Tea" />
            </div>
          </div>
          <div class="actions">
            <button class="primary" disabled={!(timerMinutes > 0)} onclick={() => startTimer(timerMinutes)}>Start</button>
          </div>
        </div>
        <div class="card">
          <h2>Focus session</h2>
          <p class="muted small">Work and break in turns. A chime and a quick look at the island mark each change.</p>
          <div class="three">
            <div>
              <label for="fwork">Work (min)</label>
              <input id="fwork" type="number" min="1" max="240" bind:value={focusWork} />
            </div>
            <div>
              <label for="fbreak">Break (min)</label>
              <input id="fbreak" type="number" min="1" max="60" bind:value={focusBreak} />
            </div>
            <div>
              <label for="frounds">Rounds</label>
              <input id="frounds" type="number" min="1" max="12" bind:value={focusRounds} />
            </div>
          </div>
          <div class="actions">
            <button
              class="primary"
              onclick={() =>
                timerCmd(['focus', '--work', `${focusWork}m`, '--break', `${focusBreak}m`, '--rounds', String(focusRounds)])}
              >Start focus</button
            >
          </div>
          <div class="net">
            <span class="name">Do not disturb while working</span>
            <input
              type="checkbox"
              class="switch"
              aria-label="Do not disturb while working"
              checked={islandPrefs?.focusQuiet ?? false}
              disabled={!islandPrefs}
              onchange={(e) => setSwitch('focusQuiet', e.currentTarget.checked)}
            />
          </div>
          <p class="muted small">
            Keeps notifications, volume and battery pop-ups and calendar reminders off the island during a work round.
            Breaks let them through, and alarms still ring.
          </p>
          {#if streak && streak.rounds > 0}
            <p class="small muted" role="status">{streakLine(streak)}</p>
          {/if}
        </div>
        <div class="actions left">
          <button onclick={() => timerCmd(['timer', 'pause'])}>Pause</button>
          <button onclick={() => timerCmd(['timer', 'resume'])}>Resume</button>
          <button onclick={() => timerCmd(['timer', 'stop'])}>Stop</button>
        </div>
        {#if timerNote}<p class="small" role="status">{timerNote}</p>{/if}
        <div class="card">
          <h2>Class timetable</h2>
          <p class="muted small">
            Enter your week once. The island shows a lesson ten minutes before it starts, and the clock card counts down
            to the next one and shows your free gaps. Kept on this computer only.
          </p>
          {#each WEEK_DAYS as dayName, d (d)}
            {#if lessons.some((l) => l.day === d)}
              <h3 class="day">{dayName}</h3>
              {#each lessons as l, i (i)}
                {#if l.day === d}
                  <div class="net">
                    <span class="when">{formatMinutes(l.start, h24)} – {formatMinutes(l.end, h24)}</span>
                    <span class="name">{l.name}</span>
                    <span class="muted small">{l.room}</span>
                    <button class="link" aria-label={`Delete ${l.name} on ${dayName}`} onclick={() => removeLesson(i)}>✕</button>
                  </div>
                {/if}
              {/each}
            {/if}
          {/each}
          <div class="three">
            <div>
              <label for="lday">Day</label>
              <select id="lday" bind:value={lessonDay}>
                {#each WEEK_DAYS as dayName, d (d)}<option value={d}>{dayName}</option>{/each}
              </select>
            </div>
            <div>
              <label for="lstart">Starts</label>
              <input id="lstart" type="time" bind:value={lessonStart} />
            </div>
            <div>
              <label for="lend">Ends</label>
              <input id="lend" type="time" bind:value={lessonEnd} />
            </div>
          </div>
          <div class="two">
            <div>
              <label for="lname">Lesson</label>
              <input id="lname" type="text" maxlength="40" placeholder="Maths" bind:value={lessonName} />
            </div>
            <div>
              <label for="lroom">Room (optional)</label>
              <input id="lroom" type="text" maxlength="40" placeholder="Room 4" bind:value={lessonRoom} />
            </div>
          </div>
          <div class="actions">
            <button class="primary" disabled={!lessonName.trim()} onclick={addLesson}>Add lesson</button>
          </div>
          {#if lessonNote}<p class="small" role="status">{lessonNote}</p>{/if}
        </div>
      {:else if page === 'alarms'}
        <h1>Alarms</h1>
        <p class="muted">
          When an alarm goes off, the island opens with a bell and plays a chime until you snooze or stop it. Alarms
          ring while Islet is running and your computer is awake.
        </p>
        <div class="card">
          <h2>New alarm</h2>
          <div class="two">
            <div>
              <label for="atime">Time</label>
              <input id="atime" type="time" bind:value={alarmTime} />
            </div>
            <div>
              <label for="alabel">Name (optional)</label>
              <input id="alabel" bind:value={alarmLabel} placeholder="Wake up" />
            </div>
          </div>
          <span class="label" id="days-label">Repeat</span>
          <div class="days" role="group" aria-labelledby="days-label">
            {#each DAY_LETTERS as d, i (i)}
              <button
                class="day"
                class:on={alarmDays & (1 << i)}
                aria-pressed={(alarmDays & (1 << i)) !== 0}
                aria-label={['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'][i]}
                onclick={() => (alarmDays ^= 1 << i)}>{d}</button
              >
            {/each}
            <span class="muted small">{describeDays(alarmDays)}</span>
          </div>
          <div class="actions"><button class="primary" onclick={addAlarm}>Add alarm</button></div>
          {#if alarmNote}<p class="small" role="status">{alarmNote}</p>{/if}
        </div>
        {#each status?.alarms ?? [] as a (a.id)}
          <div class="card row">
            <div>
              <h2 class:muted={!a.enabled}>{formatAlarmTime(a, h24)}</h2>
              <p class="muted">{a.label || 'Alarm'} · {describeDays(a.days)}</p>
            </div>
            <div class="row">
              <button class="link" onclick={() => alarmCmd('remove', a.id)}>Delete</button>
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
          <p class="muted">No alarms yet.</p>
        {/each}
      {:else if page === 'navigation'}
        <h1>Navigation</h1>
        <p class="muted">
          See your travel time with live traffic, then hand off to Google Maps or Waze. Uses the Google Routes API with
          <strong>your own</strong> key: Google bills your project, usually within its free monthly allowance.
          <button class="link" onclick={() => api.openLink(docs('docs/navigation.md'))}>How to get a key</button>
        </p>
        <div class="card">
          <h2>Google Routes API key</h2>
          <p class={status?.routesKey ? 'ok' : 'muted'}>{status?.routesKey ? 'Saved in your keychain' : 'Not set'}</p>
          <label for="key">{status?.routesKey ? 'Replace key' : 'Key'}</label>
          <input id="key" type="password" autocomplete="off" spellcheck="false" bind:value={key} placeholder="AIza…" />
          {#if key && !keyLooksValid(key)}<p class="warn small">This doesn’t look like a Google API key.</p>{/if}
          <div class="actions">
            {#if status?.routesKey}<button onclick={clearKey}>Remove key</button>{/if}
            <button class="primary" disabled={!key.trim()} onclick={saveKey}>Save key</button>
          </div>
          {#if keyNote}<p class="small" role="status">{keyNote}</p>{/if}
        </div>
        <div class="card">
          <h2>Trip</h2>
          {#if status?.trip}
            <p class="ok">Showing the trip to {status.trip.label || status.trip.destination}</p>
          {/if}
          <label for="dest">Destination</label>
          <input
            id="dest"
            bind:value={trip.destination}
            oninput={destinationTyped}
            placeholder="Plot 5, Kampala Road"
            autocomplete="off"
            aria-autocomplete="list"
          />
          {#if places.length}
            <div class="places" role="listbox" aria-label="Suggestions">
              {#each places as p (p.placeId)}
                <button role="option" aria-selected="false" onclick={() => pickPlace(p)}>
                  <strong>{p.main}</strong>
                  {#if p.secondary}<span class="muted small">{p.secondary}</span>{/if}
                </button>
              {/each}
              <p class="muted small attribution">Powered by Google</p>
            </div>
          {/if}
          <label for="from">From</label>
          <input id="from" bind:value={trip.origin} placeholder="An address or “lat,lng”" />
          <div class="two">
            <div>
              <label for="label">Name on the island (optional)</label>
              <input id="label" bind:value={trip.label} placeholder="Work" />
            </div>
            <div>
              <label for="mode">Travel by</label>
              <select id="mode" bind:value={trip.mode}>
                {#each MODES as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
              </select>
            </div>
          </div>
          <div class="actions">
            {#if status?.trip}<button onclick={stopTrip}>Stop trip</button>{/if}
            <button class="primary" onclick={startTrip}>{status?.trip ? 'Update trip' : 'Start trip'}</button>
          </div>
          {#if tripNote}<p class="small" role="status">{tripNote}</p>{/if}
        </div>
        <p class="muted small">
          Islet refreshes the ETA every minute while the card is open and every 2–5 minutes otherwise, and makes no
          requests without a trip. Trips go straight to Google; route results are never stored.
        </p>
      {:else if page === 'launcher'}
        <h1>Launcher</h1>
        <p class="muted">
          Type an app’s name, a sum (12 * 7.5), a conversion (5 km in miles) or anything to search the web for. Enter
          opens the first result.
        </p>
        <div class="card">
          <input
            type="text"
            class="launcher-input"
            aria-label="Search apps, the web, or type a sum"
            placeholder="Search…"
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
          To open the launcher with a key, bind one to <code>islet launcher</code> in your system’s keyboard settings.
          Web searches go to DuckDuckGo, and only when you choose that row.
        </p>
      {:else if page === 'pro'}
        <h1>Islet Pro {#if pro?.active}<span class="badge">Supporter</span>{/if}</h1>
        {#if pro?.active}
          <div class="card">
            <h2>Pro is on. Thank you!</h2>
            <p class="muted">
              Key {pro.keyHint}. You’re supporting an independent open-source app.
              {#if pro.expiresAt}It runs until {new Date(pro.expiresAt * 1000).toLocaleDateString()}.{/if}
            </p>
            <div class="actions left">
              <button disabled={busy} onclick={() => proRun(['pro', 'check'])}>Check my key</button>
              <button disabled={busy} onclick={() => proRun(['pro', 'deactivate'])}>Move Pro to another computer</button>
            </div>
            <p class="small muted">
              Islet asks the store once a week whether your key is still valid. Being offline never turns Pro off.
            </p>
          </div>
        {:else}
          <div class="card">
            <h2>One payment, yours for good</h2>
            <p class="muted">
              Everything Islet does today stays free. Pro adds the extras below and helps keep Islet going. Use your key
              on up to three of your computers.
            </p>
            {#if pro?.refused}
              <p class="small" role="alert">The store no longer accepts the key on this computer (refunded or revoked).</p>
            {/if}
            {#if pro?.forSale}
              <div class="actions left">
                {#if pro.buyUrl}
                  <button class="primary" onclick={() => pro?.buyUrl && api.openLink(pro.buyUrl)}>Buy Islet Pro</button>
                {/if}
              </div>
              <label for="pro-key">Already bought it? Paste your key</label>
              <input id="pro-key" type="password" autocomplete="off" spellcheck="false" bind:value={proKey} placeholder="Your key" />
              <div class="actions left">
                <button disabled={busy || !proKey.trim()} onclick={() => proRun(['pro', 'activate'], proKey.trim())}>
                  Activate
                </button>
              </div>
              <p class="small muted">
                Activating sends the key to the store to check it. The key is kept in your system keychain.
              </p>
            {:else if pro}
              <p class="muted">Islet Pro isn’t on sale in this build yet.</p>
            {/if}
          </div>
        {/if}
        {#if pro?.active}
          <div class="card">
            <h2>Performance</h2>
            <p class="muted">
              {#if islandPrefs?.powerSaver}
                Power saver is on. Islet keeps short histories and small album art, and the waveform stands still, so
                playing music costs no extra battery.
              {:else}
                The limits of the free app are off: animation at 60 frames a second, sharper album art, a clipboard
                history of 500 copies and room for 100 files on the shelf. Islet uses more memory and a little more
                power for it.
              {/if}
            </p>
            <div class="net">
              <span class="name">Power saver</span>
              <input
                type="checkbox"
                class="switch"
                aria-label="Power saver"
                checked={islandPrefs?.powerSaver ?? false}
                disabled={!islandPrefs}
                onchange={(e) => setSwitch('powerSaver', e.currentTarget.checked)}
              />
            </div>
            <p class="muted small">
              For a long day on battery. Nothing you saved is removed: your shelf, notes and shortcuts stay as they are.
            </p>
          </div>
          <div class="card">
            <h2>World clocks</h2>
            <p class="muted">Up to three cities on the clock card, with daylight saving taken care of.</p>
            {#if worldClocks.length}
              <div class="actions left">
                {#each worldClocks as c (c)}
                  <button onclick={() => worldChange(false, c)} aria-label={`Remove ${c}`}>{c} ✕</button>
                {/each}
              </div>
            {/if}
            {#if worldClocks.length < 3}
              <label for="city">Add a city</label>
              <select id="city" bind:value={cityPick} onchange={() => worldChange(true, cityPick)}>
                <option value="">Choose…</option>
                {#each cities.filter((c) => !worldClocks.includes(c)) as c (c)}<option value={c}>{c}</option>{/each}
              </select>
            {/if}
          </div>
          <div class="card">
            <h2>Quick notes</h2>
            <p class="muted">
              One line at a time, added with the date to a plain text file you can open anywhere:
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
                aria-label="Quick note"
                placeholder="Ask about the essay deadline…"
                bind:value={quickNote}
              />
              <button class="primary" type="submit" disabled={!quickNote.trim()}>Save</button>
            </form>
            {#if quickNoteSaid}<p class="small" role="status">{quickNoteSaid}</p>{/if}
            <p class="muted small">
              For a hotkey, bind a key to <code>islet note</code> in your system’s keyboard settings, or run
              <code>islet note "your text"</code> from a launcher.
            </p>
          </div>
          <div class="card">
            <h2>Notes &amp; to-do</h2>
            <p class="muted">
              Type here; tick things off on the island (Control Centre → the checklist button). Kept on this computer
              only.
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
                aria-label="New note or to-do"
                placeholder="Buy milk…"
                bind:value={todoText}
              />
              <button class="primary" type="submit" disabled={!todoText.trim()}>Add</button>
            </form>
            {#if todos.length}
              <ul class="todos">
                {#each todos as t (t.id)}
                  <li>
                    <label class:done={t.done}>
                      <input type="checkbox" checked={t.done} onchange={() => todoRun(['todo', 'done', String(t.id)])} />
                      {t.text}
                    </label>
                    <button class="link" aria-label={`Delete ${t.text}`} onclick={() => todoRun(['todo', 'remove', String(t.id)])}>
                      ✕
                    </button>
                  </li>
                {/each}
              </ul>
              {#if todos.some((t) => t.done)}
                <div class="actions left"><button onclick={() => todoRun(['todo', 'clear-done'])}>Clear finished</button></div>
              {/if}
            {/if}
          </div>
          <div class="card">
            <h2>Shortcuts</h2>
            <p class="muted">
              Buttons of your own on the island (Control Centre → the bolt button). Each opens a folder, a file, a script
              or a web page. Up to 50.
            </p>
            {#each shortcuts as s, i (i)}
              <div class="net">
                <span class="name">{s.name}</span>
                <span class="muted small target">{s.target}</span>
                <button class="link" aria-label={`Delete ${s.name}`} onclick={() => shortcutRun(['shortcut', 'remove', String(i + 1)])}>
                  ✕
                </button>
              </div>
            {/each}
            {#if shortcuts.length < 50}
              <div class="two">
                <div>
                  <label for="sc-name">Name</label>
                  <input id="sc-name" type="text" maxlength="40" placeholder="Homework" bind:value={shortcutName} />
                </div>
                <div>
                  <label for="sc-target">Opens</label>
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
                  Add shortcut
                </button>
              </div>
            {/if}
          </div>
          <div class="card">
            <h2>Stopwatch</h2>
            <p class="muted">Open the island and tap the stopwatch button in Control Centre. Laps and pause work from the island.</p>
          </div>
        {/if}
        {#if pro?.features.length}
          <div class="card">
            <h2>What Pro adds</h2>
            <ul class="features">
              {#each pro.features as f (f)}<li>{f}</li>{/each}
            </ul>
          </div>
        {/if}
        {#if proNote}<p class="small" role="status">{proNote}</p>{/if}
      {:else if page === 'updates'}
        <h1>Updates</h1>
        <div class="card row">
          <div>
            <h2>Check when Settings opens</h2>
            <p class="muted">Off by default: Islet makes no network requests you didn’t ask for.</p>
          </div>
          <input
            type="checkbox"
            class="switch"
            aria-label="Check for updates when Settings opens"
            bind:checked={prefs.checkUpdates}
            onchange={savePrefs}
          />
        </div>
        <div class="actions left"><button onclick={checkUpdate}>Check for updates now</button></div>
        {#if update?.newer}
          <div class="card">
            <h2>Islet {update.latest} is available</h2>
            {#if update.notes}<p class="muted notes">{update.notes}</p>{/if}
            {#if info?.selfUpdate}
              <button class="primary" onclick={installUpdate}>Install and restart</button>
            {:else}
              <button class="primary" onclick={() => api.openLink(update?.url || `${info?.repo}/releases/latest`)}>
                Open the download page
              </button>
            {/if}
          </div>
        {/if}
        {#if updateNote}<p class="small" role="status">{updateNote}</p>{/if}
        <div class="card">
          <h2>Early builds {#if !pro?.active}<span class="pro-tag">Pro</span>{/if}</h2>
          <p class="muted">
            New features before they are released to everyone. They can have rough edges; you install them by hand from
            the download page.
          </p>
          <div class="actions left">
            <button onclick={() => (pro?.active ? checkEarly() : (page = 'pro'))}>Look for an early build</button>
            {#if early?.newer}
              <button class="primary" onclick={() => early && api.openLink(early.url)}>Open the download page</button>
            {/if}
          </div>
          {#if earlyNote}<p class="small" role="status">{earlyNote}</p>{/if}
        </div>
      {:else}
        <h1>About Islet</h1>
        <p>Version {info?.version}</p>
        <p class="muted">
          Free and open source under the MIT License. Islet is not affiliated with, endorsed by or sponsored by Apple
          Inc. “Dynamic Island” is a trademark of Apple Inc. Google Maps is a trademark of Google LLC; Waze is a
          trademark of Waze Mobile Ltd.
        </p>
        <div class="actions left">
          <button onclick={() => api.openLink(info?.repo ?? '')}>Source code</button>
          <button onclick={() => api.openLink(docs('THIRD_PARTY_LICENSES.md'))}>Third-party licenses</button>
          <button onclick={() => api.openLink(`${info?.repo}/issues/new/choose`)}>Report a problem</button>
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
  .features {
    margin: 4px 0 0;
    padding-left: 18px;
    line-height: 1.6;
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
  .style-name {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-weight: 550;
  }
  .pro-tag {
    font-size: 9.5px;
    font-weight: 700;
    padding: 1px 5px;
    border-radius: 999px;
    color: #fff;
    background: linear-gradient(135deg, #ff9f0a, #ff375f);
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
  /* A row's control keeps to the right and leaves room for its label. */
  .net select {
    width: auto;
    min-width: 200px;
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
  .launcher-input {
    width: 100%;
    font-size: 17px;
    padding: 10px 12px;
    margin-bottom: 6px;
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
