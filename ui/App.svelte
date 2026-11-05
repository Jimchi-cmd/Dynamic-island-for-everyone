<script lang="ts">
  import { onMount } from 'svelte';
  import * as api from './lib/bridge';
  import type { Info, Prefs, UpdateInfo } from './lib/bridge';
  import {
    alarmAddArgs,
    DAY_LETTERS,
    describeDays,
    formatAlarmTime,
    keyLooksValid,
    MODES,
    navStartArgs,
    parsePage,
    permissionsFor,
    signalBars,
    type BtList,
    type WifiList,
    type Status,
    type Trip,
  } from './lib/settings';

  type Page = 'general' | 'connections' | 'alarms' | 'timer' | 'navigation' | 'updates' | 'about';
  const PAGES: { id: Page; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'connections', label: 'Wi-Fi & Bluetooth' },
    { id: 'alarms', label: 'Alarms' },
    { id: 'timer', label: 'Timer & focus' },
    { id: 'navigation', label: 'Navigation' },
    { id: 'updates', label: 'Updates' },
    { id: 'about', label: 'About' },
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
  });

  // Timer & focus
  let timerMinutes = $state(10);
  let timerLabel = $state('');
  let focusWork = $state(25);
  let focusBreak = $state(5);
  let focusRounds = $state(4);
  let timerNote = $state('');

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
    } catch (e) {
      update = null;
      updateNote = api.message(e);
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
    const wanted = parsePage(await api.initialPage().catch(() => null));
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
          ETA (with your own Google key) and update checks.
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
      <div class="brand">Islet</div>
      {#each PAGES as p (p.id)}
        <button class="tab" class:active={page === p.id} aria-current={page === p.id} onclick={() => (page = p.id)}>
          {p.label}
        </button>
      {/each}
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
        </div>
        <div class="actions left">
          <button onclick={() => timerCmd(['timer', 'pause'])}>Pause</button>
          <button onclick={() => timerCmd(['timer', 'resume'])}>Resume</button>
          <button onclick={() => timerCmd(['timer', 'stop'])}>Stop</button>
        </div>
        {#if timerNote}<p class="small" role="status">{timerNote}</p>{/if}
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
          <input id="dest" bind:value={trip.destination} placeholder="Plot 5, Kampala Road" />
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
  .layout {
    display: grid;
    grid-template-columns: 180px 1fr;
    height: 100%;
  }
  nav {
    background: var(--sidebar);
    border-right: 1px solid var(--line);
    padding: 18px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .brand {
    font-weight: 700;
    font-size: 16px;
    padding: 0 10px 14px;
  }
  .tab {
    border: 0;
    background: none;
    text-align: left;
    padding: 7px 10px;
  }
  .tab.active {
    background: var(--panel);
    font-weight: 600;
  }
  main {
    padding: 24px 28px;
    overflow-y: auto;
  }
  .welcome {
    max-width: 560px;
    margin: 0 auto;
    padding: 40px 24px;
  }
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 14px 16px;
    margin: 12px 0;
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
  .switch {
    width: 20px;
    height: 20px;
    flex: none;
    accent-color: var(--accent);
  }
</style>
