// KB-E2B6 web configurator (WebHID). No build step, no server, no network access.
// Plain scripts (no ES modules) so index.html works when opened directly from disk.
//
// Flow: Connect → read device info + running config → edit (dropdowns only) → Save to device
// (the device validates, saves to flash and restarts with the new settings) → page reconnects.

(() => {
'use strict';
const C = window.KB.catalog;
const {
  decodeCrs,
  encodeCrs,
  samePayload,
  buttonInUse,
  encoderInUse,
  buttonName,
  N_ENCODER_BUTTONS,
  N_PANEL_BUTTONS,
  N_ENCODERS,
  TOTAL_LEN,
} = window.KB.crs;
const { KbClient, DeviceError, READ_SELECT, DEVICE_MODEL_ID, PROTOCOL_VERSION } = window.KB.protocol;
const {
  webHidSupported,
  requestConfigDevice,
  authorizedConfigDevices,
  isConfigDevice,
  HidTransport,
  reconnectAfterReboot,
} = window.KB.transport;

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------
const state = {
  // unsupported | disconnected | connecting | ready | busy | rebooting
  phase: 'disconnected',
  busyText: '',
  transport: null,
  client: null,
  info: null,
  loadedBytes: null, // config the device is running (null: device has no valid config)
  model: null, // form model
  fromDefaults: false,
  showAdvanced: false,
  message: null, // { kind: 'info'|'ok'|'warn'|'error', text }
  open: {}, // card id -> expanded? (unset: General expanded, encoders/buttons collapsed)
};

const $ = (sel) => document.querySelector(sel);

function log(msg) {
  const box = $('#log');
  const line = `${new Date().toLocaleTimeString()}  ${msg}\n`;
  box.textContent = (box.textContent + line).split('\n').slice(-300).join('\n');
  box.scrollTop = box.scrollHeight;
  console.debug('[kb]', msg);
}

function setMessage(kind, text) {
  state.message = text ? { kind, text } : null;
  if (text) log(`${kind.toUpperCase()}: ${text}`);
}

function setPhase(phase, busyText = '') {
  state.phase = phase;
  state.busyText = busyText;
  render();
}

const editable = () => state.phase === 'ready';
const connected = () => ['ready', 'busy', 'rebooting'].includes(state.phase);

// ---------------------------------------------------------------------------------------------
// Device session
// ---------------------------------------------------------------------------------------------
function checkCompat(info) {
  if (info.deviceModel !== DEVICE_MODEL_ID) {
    throw new Error(`Unexpected device model 0x${info.deviceModel.toString(16)} — not a KB-E2B6.`);
  }
  if (info.protocolVersion !== PROTOCOL_VERSION) {
    throw new Error('Device communication protocol is not supported by this configurator.');
  }
  if (info.crsSchemaVersion !== 1 || info.configLen !== TOTAL_LEN) {
    throw new Error(
      `Firmware configuration schema ${info.crsSchemaVersion} is not supported by this configurator.`,
    );
  }
}

async function withRetry(fn, attempts = 3) {
  let last;
  for (let i = 0; i < attempts; i++) {
    try {
      return await fn();
    } catch (e) {
      last = e;
      await new Promise((r) => setTimeout(r, 300));
    }
  }
  throw last;
}

/** Read info + running config; optionally (re)load the form from it. */
async function refreshFromDevice({ loadForm }) {
  const info = await withRetry(() => state.client.getInfo());
  checkCompat(info);
  state.info = info;
  log(`info: fw ${info.fwVersion}, has config=${info.hasConfig}, crc=0x${info.configCrc.toString(16)}`);

  state.loadedBytes = info.hasConfig ? await state.client.readConfig(READ_SELECT.RUNNING) : null;

  if (loadForm) {
    if (state.loadedBytes) {
      state.model = decodeCrs(state.loadedBytes).model;
      state.fromDefaults = false;
    } else {
      state.model = decodeCrs(await state.client.readConfig(READ_SELECT.FACTORY_DEFAULTS)).model;
      state.fromDefaults = true;
    }
  }
}

async function attach(transport) {
  setPhase('connecting', 'Connecting…');
  try {
    await transport.open();
    state.transport = transport;
    state.client = new KbClient(transport, log);
    await refreshFromDevice({ loadForm: true });
    if (!state.info.hasConfig) {
      setMessage(
        'warn',
        'This board has no configuration yet, so its buttons and encoders do nothing. The form shows a suggested setup — adjust it and click “Save to device”.',
      );
    } else {
      setMessage('ok', 'Connected. The form shows the configuration the device is running.');
    }
    setPhase('ready');
  } catch (e) {
    await detach();
    setMessage('error', `Connection failed: ${e.message}`);
    render();
  }
}

async function detach() {
  state.client?.dispose();
  await state.transport?.close();
  state.client = null;
  state.transport = null;
  state.info = null;
  state.phase = 'disconnected';
}

/** The device restarts after saving: reopen it and re-read everything. */
async function waitForReboot(text) {
  setPhase('rebooting', text);
  const old = state.transport.device;
  state.client.dispose('device restarting');
  await state.transport.close();
  state.client = null;
  state.transport = null;

  const t = await reconnectAfterReboot(old, { log });
  state.transport = t;
  state.client = new KbClient(t, log);
  await refreshFromDevice({ loadForm: true });
}

async function runBusy(text, fn) {
  setPhase('busy', text);
  try {
    await fn();
    state.phase = state.transport ? 'ready' : 'disconnected';
  } catch (e) {
    log(`error: ${e.stack || e.message}`);
    setMessage('error', e.message);
    if (!state.transport || !state.transport.device.opened) {
      await detach();
    } else {
      state.phase = 'ready';
    }
  }
  render();
}

// ---------------------------------------------------------------------------------------------
// Form model helpers
// ---------------------------------------------------------------------------------------------
function formBytes(buildId = 0) {
  if (!state.model || !state.info) return null;
  return encodeCrs(state.model, state.info.fwCompatId, buildId);
}

function hasLocalEdits() {
  if (!state.model) return false;
  if (!state.loadedBytes) return true;
  return !samePayload(formBytes(), state.loadedBytes);
}

function validateModel(m) {
  const errors = [];
  const warnings = [];
  if (!m) return { errors, warnings };
  const g = m.globals;

  if (g.ledLevelMin > g.ledLevelMax) errors.push('Device LEDs: minimum brightness is above the maximum.');

  m.buttons.forEach((b, i) => {
    if (!buttonInUse(b)) return;
    const pulse = b.pulseMs < 0 ? g.pulseMsButtonDefault : b.pulseMs;
    if (b.repeatPeriodMs > 0 && pulse === 0) {
      errors.push(`${buttonName(i)}: “Repeat while held” needs a “Release after … ms” press style (not “Hold while pressed”).`);
    }
  });

  const keys = new Set();
  for (const b of m.buttons) [b.short, b.long].forEach((a) => keys.add(C.actionKey(a)));
  for (const e of m.encoders) [e.cw, e.ccw].forEach((a) => keys.add(C.actionKey(a)));
  if (keys.has('cc:006F') !== keys.has('cc:0070')) {
    warnings.push('Screen brightness: only one of “Brightness up/down” is assigned.');
  }
  m.encoders.forEach((e, i) => {
    if (encoderInUse(e) && (e.cw.tag === 0 || e.ccw.tag === 0)) {
      warnings.push(`Encoder ${i + 1}: only one turn direction has an action.`);
    }
  });
  if (!m.buttons.some(buttonInUse) && !m.encoders.some(encoderInUse)) {
    warnings.push('No control has an action — the device will not send anything.');
  }
  return { errors, warnings };
}

// ---------------------------------------------------------------------------------------------
// Actions (toolbar)
// ---------------------------------------------------------------------------------------------
async function onConnect() {
  try {
    const dev = await requestConfigDevice();
    if (!dev) {
      setMessage('warn', 'No KB-E2B6 configuration interface was selected.');
      render();
      return;
    }
    await attach(new HidTransport(dev));
  } catch (e) {
    setMessage('error', e.message);
    render();
  }
}

async function onDisconnect() {
  await detach();
  setMessage('info', 'Disconnected.');
  render();
}

async function onSave() {
  const { errors } = validateModel(state.model);
  if (errors.length) {
    setMessage('error', errors.join(' '));
    render();
    return;
  }
  const bytes = formBytes(Math.floor(Date.now() / 1000));
  const expected = bytes;

  await runBusy('Saving to the device…', async () => {
    let r;
    try {
      r = await state.client.saveConfig(bytes);
    } catch (e) {
      if (e instanceof DeviceError) {
        // Definite answer from the device: nothing was saved, the previous settings stay active.
        throw new Error(`Not saved: ${e.message}. The device keeps its previous settings.`);
      }
      // Timeout / disconnect: the outcome is unknown — never assume either way.
      throw new Error(
        'Lost contact while saving. Re-plug the device and click Connect: the form will show which settings it runs.',
      );
    }

    if (!r.rebooting) {
      setMessage('info', 'Nothing to save: the device already has exactly these settings.');
      return;
    }

    try {
      await waitForReboot('Saved. Device is restarting with the new settings…');
    } catch (e) {
      // Normal when the page is opened from a local file: the browser may not hand the restarted
      // device back without asking again. The save itself already succeeded.
      log(`reconnect after save: ${e.message}`);
      await detach();
      setMessage(
        'ok',
        'Saved. The device restarted with the new settings. Click “Connect device” to see it or make more changes.',
      );
      return;
    }
    if (samePayload(state.loadedBytes, expected)) {
      setMessage('ok', 'Saved. The device restarted and now works with the new settings.');
    } else {
      setMessage('warn', 'The device restarted, but it does not report the settings just saved. Check the form.');
    }
  });
}

async function onDefaults() {
  if (hasLocalEdits() && !confirm('Replace the form with the suggested setup? Unsaved edits are lost.')) return;
  await runBusy('Loading the suggested setup…', async () => {
    const bytes = await state.client.readConfig(READ_SELECT.FACTORY_DEFAULTS);
    state.model = decodeCrs(bytes).model;
    state.fromDefaults = true;
    setMessage('info', 'Form filled with the suggested setup (not saved yet). Review, then click “Save to device”.');
  });
}

async function onReload() {
  if (hasLocalEdits() && !confirm('Reload from the device? Unsaved edits are lost.')) return;
  await runBusy('Reading configuration…', async () => {
    await refreshFromDevice({ loadForm: true });
    setMessage('info', 'Form reloaded from the device.');
  });
}

function onExport() {
  const bytes = formBytes(Math.floor(Date.now() / 1000));
  const blob = new Blob([bytes], { type: 'application/octet-stream' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = `kb-e2b6-config-${new Date().toISOString().slice(0, 10)}.crs`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  setMessage('info', `Exported ${a.download} (CRS v1, 288 bytes; also readable with \`crs-pack dump --toml\`).`);
  render();
}

async function onImportFile(file) {
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const { header, model } = decodeCrs(bytes);
    state.model = model;
    state.fromDefaults = false;
    const note =
      state.info && header.fwCompatId !== state.info.fwCompatId
        ? ` Note: it was made for firmware compat 0x${header.fwCompatId.toString(16)}; it will be re-stamped for this device on Save.`
        : '';
    setMessage('info', `Loaded ${file.name} into the form (not saved yet).${note}`);
  } catch (e) {
    setMessage('error', `Cannot load ${file.name}: ${e.message}`);
  }
  render();
}

// ---------------------------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------------------------
function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === false || v == null) continue;
    if (k === 'class') e.className = v;
    else if (k.startsWith('on')) e.addEventListener(k.slice(2), v);
    else e.setAttribute(k, v === true ? '' : v);
  }
  // Flatten fully: callers pass nodes, arrays of nodes, or arrays of arrays (cards).
  for (const c of children.flat(Infinity)) {
    if (c == null || c === false) continue;
    e.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return e;
}

function changed() {
  render();
}

// Tooltip texts (hover a label). Each argument of lines() is one tooltip line. First draft —
// refine freely.
const lines = (...l) => l.join('\n');
const NEEDS_RELEASE = 'Needs a “Release after … ms” press style.';

const TIPS = {
  shortPress: lines(
    'Sent when you press and release the button quickly (before the hold time).',
    'If no long press is set, it is sent immediately when you press.',
  ),
  longPress: lines(
    'Sent when you keep holding the button past the hold time.',
    'Leave “Not used” for a simple one-action button.',
  ),
  mods: lines(
    'Modifier keys pressed together with the keyboard key, e.g. Ctrl+F1.',
    'Every action has its own: short press, long press,',
    'and each encoder turn direction (e.g. Ctrl+Up / Ctrl+Down).',
  ),
  statusConfig: lines(
    'The device keeps its settings in two flash slots, A and B.',
    'Active = used now. Backup = the previous settings (valid, not used).',
    'None = empty or invalid (the console command “cfg status” shows why).',
    'Each save writes the slot that is not active and switches to it only once',
    'fully written, so a power cut during a save never loses the settings.',
    'No valid slot: buttons and encoders stay disabled until you save.',
  ),
  statusLedMem: lines(
    'Records used in the flash page that remembers LED on/off and brightness.',
    'Each LED change adds one record; when the page is full it is erased',
    'and starts again at 0 (normal wear levelling).',
    'Off = “Remember on/off” and “Remember brightness” are both No (Controller LEDs).',
  ),
  modsNotKbd: lines(
    'Modifiers (Ctrl, Shift, Alt, Win) only work with keyboard keys.',
    'Media, Android/system and LED actions do not use them.',
  ),
  longPressTime: 'How long you must hold the button before it counts as a long press.',
  pressStyle: lines(
    'Applies to both the short and the long press of this button.',
    '',
    'Release after N ms — the key is pressed and released automatically after N ms.',
    'Hold while pressed — the key stays down for as long as you hold the button.',
  ),
  pressStyleDefault: lines(
    'Used by every push button whose press style is “Default”,',
    'and by the on-board LED button.',
    '',
    'Release after N ms — the key is pressed and released automatically after N ms.',
    'Hold while pressed — the key stays down for as long as you hold the button.',
  ),
  repeatShort: lines(
    'While the button is held, keep sending this action at this interval (e.g. volume up).',
    NEEDS_RELEASE,
    'Not available when a long press is set — then the long press repeats instead.',
  ),
  repeatLong: lines(
    'After the hold time, keep sending the long-press action at this interval',
    'for as long as the button is held.',
    NEEDS_RELEASE,
  ),
  debounce: lines(
    'Ignore contact bounce shorter than this.',
    'Increase if one press sometimes counts twice.',
  ),
  pull: lines('Input resistor setting.', 'The KB-E2B6 harness needs Pull-up.'),
  activeLevel: lines('Which electrical level means “pressed”.', 'The KB-E2B6 harness is Active-low.'),
  scanRate: lines('How often all inputs are sampled.', '1000 Hz is plenty for buttons and encoders.'),
  cw: 'Sent once per click (detent) when turning clockwise.',
  ccw: 'Sent once per click (detent) when turning counter-clockwise.',
  tapPerStep: lines(
    'How long each turn step holds the key down.',
    'Increase if the head unit misses fast steps.',
  ),
  direction: lines(
    'Swaps clockwise and counter-clockwise.',
    'Use it if an encoder works backwards.',
  ),
  detent: lines(
    'Filters very short glitches between encoder contacts.',
    'Increase if one click sometimes counts as two.',
  ),
  ledRememberOnOff: 'Remember whether the backlight was on or off when power was lost.',
  ledOnBoot: 'Backlight state when the controller powers up.',
  ledRememberLevel: 'Remember the last brightness when power was lost.',
  ledLevelOnBoot: 'Brightness when the controller powers up.',
  ledFirstUse: lines(
    'Brightness used when nothing has been remembered yet',
    '(first power-up).',
  ),
  ledStep: 'How much each “LED brighter / dimmer / step” action changes the brightness.',
  ledMin: 'Lowest brightness the LED actions can reach.',
  ledMax: 'Highest brightness the LED actions can reach.',
  ledSaveDelay: lines(
    'Waits this long after the last brightness change before saving it,',
    'to spare the flash memory.',
  ),
};

/** Label text with an optional tooltip (dotted underline marks a hover explanation). */
function labelEl(text, tip) {
  return el('span', { class: tip ? 'field-label tip' : 'field-label', title: tip || null }, text);
}

/** Dropdown bound to a numeric model field. Unknown current values are kept as "(current)". */
function selectField(label, options, value, onChange, { hint, disabled, adv, unit = '', tip } = {}) {
  const opts = [...options];
  if (!opts.some(([v]) => v === value)) opts.push([value, `${value}${unit} (current)`]);
  const sel = el(
    'select',
    { disabled: disabled || !editable(), title: tip || null },
    opts.map(([v, l]) => el('option', { value: String(v) }, l)),
  );
  sel.value = String(value);
  sel.addEventListener('change', () => {
    onChange(Number(sel.value));
    changed();
  });
  return el(
    'label',
    { class: `field${adv ? ' adv' : ''}` },
    labelEl(label, tip),
    sel,
    hint ? el('small', { class: 'hint' }, hint) : null,
  );
}

/** Option list with a leading "Default (global …)" entry (value -1 = inherit). */
function inheritOpts(list, globalValue, unit = '') {
  return [[-1, `Default (${C.optionLabel(list, globalValue, unit)})`], ...list];
}

/** Action dropdown + its own "Modifiers" dropdown row (each action has a separate one). */
function actionField(label, action, onChange, tip) {
  const key = C.actionKey(action);
  const sel = el('select', { disabled: !editable(), title: tip || null }, el('option', { value: 'none' }, '— Not used —'));
  if (!C.isCatalogKey(key)) {
    const bare = action.tag === C.TAG_KBD ? { ...action, a: 0 } : action;
    sel.append(el('optgroup', { label: 'From device (not in list)' }, el('option', { value: key }, C.describeAction(bare))));
  }
  for (const g of C.ACTION_GROUPS) {
    sel.append(
      el(
        'optgroup',
        { label: g.label },
        g.items.map(([kind, code, text]) => el('option', { value: C.catalogKey(kind, code) }, text)),
      ),
    );
  }
  sel.value = key;
  sel.addEventListener('change', () => {
    const keepMods = action.tag === C.TAG_KBD ? action.a : 0;
    onChange(C.actionFromKey(sel.value, keepMods));
    changed();
  });

  const fields = [el('label', { class: 'field' }, labelEl(label, tip), sel)];
  if (action.tag !== C.TAG_NONE) {
    // Its own "Modifiers" row under the action. Shown for every assigned action so the layout
    // stays stable; only usable with keyboard keys.
    const isKbd = action.tag === C.TAG_KBD;
    const bits = isKbd ? action.a : 0;
    const opts = [...C.MODIFIER_COMBOS];
    if (!opts.some(([v]) => v === bits)) opts.push([bits, `0x${bits.toString(16).toUpperCase()} (current)`]);
    const tipMods = isKbd ? TIPS.mods : TIPS.modsNotKbd;
    const mods = el(
      'select',
      { disabled: !isKbd || !editable(), title: tipMods },
      opts.map(([v, l]) => el('option', { value: String(v) }, l)),
    );
    mods.value = String(bits);
    mods.addEventListener('change', () => {
      onChange({ ...action, a: Number(mods.value) });
      changed();
    });
    fields.push(el('label', { class: 'field' }, labelEl('Modifiers', tipMods), mods));
  }
  return fields;
}

/**
 * A boxed sub-group inside a card. `adv`: shown only with "Advanced" on. `kind` picks the
 * accent colour (short / long / rotation / push).
 */
function sub(title, content, { adv = false, note, kind } = {}) {
  return el(
    'div',
    { class: `sub${adv ? ' adv' : ''}${kind ? ` k-${kind}` : ''}` },
    el('div', { class: 'sub-title' }, title, note ? el('span', { class: 'sub-note' }, ` ${note}`) : null),
    el('div', { class: 'fields' }, content),
  );
}

/** Sub-groups for one push button (panel button or encoder push button). */
function buttonSubs(i) {
  const b = state.model.buttons[i];
  const g = state.model.globals;
  const inUse = buttonInUse(b);
  const longSet = b.long.tag !== 0;
  const pulseEff = b.pulseMs < 0 ? g.pulseMsButtonDefault : b.pulseMs;

  // Firmware keeps ONE repeat value per button: it re-sends the LONG action when one is set,
  // otherwise the SHORT action. The Short box always shows the control, but with a long action
  // set it reads "Off" and is disabled; the active control is then in the Long box.
  const repeatField = (forLong) => {
    const active = forLong === longSet;
    return selectField('Repeat while held', C.OPTIONS.repeat, active ? b.repeatPeriodMs : 0, (v) => (b.repeatPeriodMs = v), {
      unit: ' ms',
      tip: forLong ? TIPS.repeatLong : TIPS.repeatShort,
      disabled: !active || (pulseEff === 0 && b.repeatPeriodMs === 0),
      hint: !active
        ? 'Off — a long press is set, so the long press repeats instead'
        : pulseEff === 0
          ? 'Needs a “Release after … ms” press style'
          : null,
    });
  };

  // Setting or clearing the long action moves which action repeats: reset repeat to Off so a
  // value meant for one action never silently applies to the other.
  const setLong = (a) => {
    if ((a.tag !== 0) !== longSet) b.repeatPeriodMs = 0;
    b.long = a;
  };

  const subs = [
    sub('Short press', [
      actionField('Action', b.short, (a) => (b.short = a), TIPS.shortPress),
      inUse ? repeatField(false) : null,
    ], { kind: 'short' }),
    sub('Long press', [
      actionField('Action', b.long, setLong, TIPS.longPress),
      longSet
        ? selectField('Hold time', inheritOpts(C.OPTIONS.longPress, g.longPressMsDefault, ' ms'), b.longPressMs, (v) => (b.longPressMs = v), {
            unit: ' ms',
            tip: TIPS.longPressTime,
          })
        : null,
      longSet ? repeatField(true) : null,
    ], { kind: 'long' }),
  ];
  if (!inUse) return subs;

  subs.push(
    // One setting per button, used by both the short and the long press.
    selectField('Press style', inheritOpts(C.OPTIONS.pulse, g.pulseMsButtonDefault, ' ms'), b.pulseMs, (v) => (b.pulseMs = v), {
      unit: ' ms',
      tip: TIPS.pressStyle,
    }),
    sub(
      'Wiring',
      [
        selectField('Debounce', inheritOpts(C.OPTIONS.debounce, g.debounceMsDefault, ' ms'), b.debounceMs, (v) => (b.debounceMs = v), {
          adv: true,
          unit: ' ms',
          tip: TIPS.debounce,
        }),
        selectField('Pull resistor', inheritOpts(C.OPTIONS.pull, g.pinPullDefault), b.pull, (v) => (b.pull = v), {
          adv: true,
          tip: TIPS.pull,
        }),
        selectField('Active level', inheritOpts(C.OPTIONS.activeLow, g.activeLowDefault), b.activeLow, (v) => (b.activeLow = v), {
          adv: true,
          tip: TIPS.activeLevel,
        }),
      ],
      { adv: true },
    ),
  );
  return subs;
}

/** Expanded state of a card: General cards start expanded, encoder/button cards collapsed. */
function isOpen(id, kind) {
  return state.open[id] ?? (kind === 'general' || kind === 'led');
}

/**
 * A collapsible card (click the header). `kind` picks the header icon (general / led / encoder /
 * button); `used` (encoder and button cards only) shows an "In use" / "Not used" badge;
 * `summary` is a one-line overview shown while the card is collapsed.
 */
function card({ id, title, note, kind, used, summary }, ...content) {
  const badge =
    used == null ? null : el('span', { class: used ? 'badge on' : 'badge' }, used ? 'In use' : 'Not used');
  const d = el(
    'details',
    { class: `card c-${kind}${used === false ? ' idle' : ''}`, open: isOpen(id, kind) },
    el(
      'summary',
      { class: 'card-head', title: 'Click to expand or collapse' },
      el('span', { class: 'card-icon', 'aria-hidden': 'true' }),
      el(
        'div',
        { class: 'card-title' },
        el('h3', {}, title),
        note ? el('div', { class: 'card-note' }, note) : null,
        summary ? el('div', { class: 'card-summary' }, summary) : null,
      ),
      badge,
      el('span', { class: 'chevron', 'aria-hidden': 'true' }),
    ),
    el('div', { class: 'fields' }, content),
  );
  d.addEventListener('toggle', () => (state.open[id] = d.open));
  return d;
}

/** "Short: Play / Pause · Long: Ctrl+F1" style overview of a list of [label, action]. */
function actionsSummary(pairs) {
  return pairs
    .filter(([, a]) => a.tag !== C.TAG_NONE)
    .map(([label, a]) => `${label}: ${C.describeAction(a)}`)
    .join(' · ');
}

function encoderCard(e) {
  const enc = state.model.encoders[e];
  const g = state.model.globals;
  const rot = [
    actionField('Turn clockwise', enc.cw, (a) => (enc.cw = a), TIPS.cw),
    actionField('Turn counter-clockwise', enc.ccw, (a) => (enc.ccw = a), TIPS.ccw),
  ];
  if (encoderInUse(enc)) {
    rot.push(
      selectField('Key tap per step', C.OPTIONS.encoderPulse, enc.pulseMs, (v) => (enc.pulseMs = v), {
        unit: ' ms',
        tip: TIPS.tapPerStep,
      }),
      selectField('Direction', inheritOpts(C.OPTIONS.swap, g.encoderSwapAbDefault), enc.swapAb, (v) => (enc.swapAb = v), {
        tip: TIPS.direction,
      }),
      selectField('Detent guard', inheritOpts(C.OPTIONS.detent, g.encoderDetentGuardMsDefault, ' ms'), enc.detentGuardMs, (v) => (enc.detentGuardMs = v), {
        adv: true,
        unit: ' ms',
        tip: TIPS.detent,
      }),
    );
  }
  const used = encoderInUse(enc) || buttonInUse(state.model.buttons[e]);
  const push = state.model.buttons[e];
  return card(
    {
      id: `enc-${e}`,
      title: `Encoder ${e + 1}`,
      kind: 'encoder',
      used,
      summary: actionsSummary([
        ['CW', enc.cw],
        ['CCW', enc.ccw],
        ['Push', push.short],
        ['Long push', push.long],
      ]),
    },
    sub('Rotation', rot, { kind: 'rotation' }),
    sub('Push button', buttonSubs(e), { kind: 'push' }),
  );
}

function globalsCards() {
  const g = state.model.globals;
  const set = (k) => (v) => (g[k] = v);
  return [
    card(
      { id: 'defaults', title: 'Defaults', note: 'Used wherever a control is set to “Default”', kind: 'general' },
      sub(
        'All push buttons',
        [
          selectField('Long-press hold time', C.OPTIONS.longPress, g.longPressMsDefault, set('longPressMsDefault'), {
            unit: ' ms',
            tip: TIPS.longPressTime,
          }),
          selectField('Press style', C.OPTIONS.pulse, g.pulseMsButtonDefault, set('pulseMsButtonDefault'), {
            unit: ' ms',
            tip: TIPS.pressStyleDefault,
          }),
          selectField('Debounce', C.OPTIONS.debounce, g.debounceMsDefault, set('debounceMsDefault'), {
            unit: ' ms',
            tip: TIPS.debounce,
          }),
        ],
        { note: '(panel buttons + encoder push buttons)' },
      ),
      sub('Encoder rotation', [
        selectField('Direction', C.OPTIONS.swap, g.encoderSwapAbDefault, set('encoderSwapAbDefault'), {
          tip: TIPS.direction,
        }),
        selectField('Detent guard', C.OPTIONS.detent, g.encoderDetentGuardMsDefault, set('encoderDetentGuardMsDefault'), {
          adv: true,
          unit: ' ms',
          tip: TIPS.detent,
        }),
      ], { note: '(both encoders)' }),
      sub(
        'Wiring & scanning',
        [
          selectField('Scan rate', C.OPTIONS.scanHz, g.scanHz, set('scanHz'), { adv: true, unit: ' Hz', tip: TIPS.scanRate }),
          selectField('Pull resistor', C.OPTIONS.pull, g.pinPullDefault, set('pinPullDefault'), { adv: true, tip: TIPS.pull }),
          selectField('Active level', C.OPTIONS.activeLow, g.activeLowDefault, set('activeLowDefault'), {
            adv: true,
            tip: TIPS.activeLevel,
          }),
        ],
        { adv: true, note: '(all inputs)' },
      ),
    ),
    card(
      { id: 'leds', title: 'Controller LEDs', note: 'Button / encoder backlight', kind: 'led' },
      sub('At power-up', [
        selectField('Remember on/off', C.OPTIONS.yesNo, g.ledStatePersist, set('ledStatePersist'), { tip: TIPS.ledRememberOnOff }),
        selectField('On/off', C.OPTIONS.ledStateOnBoot, g.ledStateOnBoot, set('ledStateOnBoot'), { tip: TIPS.ledOnBoot }),
        selectField('Remember brightness', C.OPTIONS.yesNo, g.ledLevelPersist, set('ledLevelPersist'), { tip: TIPS.ledRememberLevel }),
        selectField('Brightness', C.OPTIONS.ledLevelOnBoot, g.ledLevelOnBoot, set('ledLevelOnBoot'), { unit: ' %', tip: TIPS.ledLevelOnBoot }),
        selectField('First-use brightness', C.OPTIONS.ledLevelDefault, g.ledLevelDefault, set('ledLevelDefault'), {
          unit: ' %',
          tip: TIPS.ledFirstUse,
        }),
      ]),
      sub('Brightness control', [
        selectField('Step', C.OPTIONS.ledLevelStep, g.ledLevelStep, set('ledLevelStep'), { unit: ' %', tip: TIPS.ledStep }),
        selectField('Minimum', C.OPTIONS.ledLevelMin, g.ledLevelMin, set('ledLevelMin'), { unit: ' %', tip: TIPS.ledMin }),
        selectField('Maximum', C.OPTIONS.ledLevelMax, g.ledLevelMax, set('ledLevelMax'), { unit: ' %', tip: TIPS.ledMax }),
        selectField('Save delay', C.OPTIONS.ledAutosave, g.ledAutosaveDelayMs, set('ledAutosaveDelayMs'), {
          adv: true,
          unit: ' ms',
          tip: TIPS.ledSaveDelay,
        }),
      ]),
    ),
  ];
}

function renderStatus() {
  const box = $('#status');
  box.replaceChildren();
  const info = state.info;
  const phaseText = {
    unsupported: 'WebHID not available',
    disconnected: 'Not connected',
    connecting: 'Connecting…',
    ready: 'Connected',
    busy: state.busyText,
    rebooting: state.busyText,
  }[state.phase];
  $('#conn').textContent = phaseText;
  $('#conn').className = `pill ${state.phase}`;

  if (!info) return;
  const device = info.hasConfig ? 'good' : 'bad';
  const kv = (label, value) => el('div', { class: 'kv' }, el('span', { class: 'kv-label' }, label), value);
  // One status per flash slot: Active / Backup (valid, not in use) / None (empty or invalid).
  const slotStatus = (s) => {
    if (info.activeSlot === s) return el('span', { class: 'tag good' }, 'Active');
    if (info.slotValid?.[s]) return el('span', { class: 'tag' }, 'Backup');
    return el('span', { class: info.hasConfig ? 'tag none' : 'tag bad' }, 'None');
  };
  const slots = info.slotValid
    ? ['A', 'B'].map((s) =>
        el('div', { class: 'kv', title: TIPS.statusConfig }, el('span', { class: 'kv-label tip' }, `Config ${s}`), slotStatus(s)),
      )
    : // Older firmware (short GET_INFO): only "has a config" is known.
      [kv('Config', el('span', { class: `tag ${device}` }, info.hasConfig ? 'Saved' : 'None — inputs disabled'))];
  const ledMem =
    info.ledJournalCap == null
      ? null
      : info.ledJournalCap === 0
        ? el('span', { class: 'muted' }, 'Off')
        : el('span', { class: 'mono' }, `${info.ledJournalUsed} / ${info.ledJournalCap}`);

  let form = ['good', 'Same as the device'];
  if (hasLocalEdits()) form = ['warn', state.fromDefaults ? 'Suggested setup — not saved yet' : 'Changed — not saved yet'];

  box.append(
    el(
      'div',
      { class: 'device-strip' },
      el('div', { class: 'device-name' }, el('span', { class: `led ${device}`, 'aria-hidden': 'true' }), 'KB-E2B6'),
      kv('Serial', el('span', { class: 'mono' }, info.serial)),
      kv('Firmware', el('span', { class: 'mono' }, info.fwVersion)),
      kv('Compat', el('span', { class: 'mono' }, `0x${info.fwCompatId.toString(16).toUpperCase()}`)),
      slots,
      ledMem
        ? el('div', { class: 'kv', title: TIPS.statusLedMem }, el('span', { class: 'kv-label tip' }, 'LED journal'), ledMem)
        : null,
      el('span', { class: 'strip-gap' }),
      kv('Form', el('span', { class: `tag ${form[0]}` }, form[1])),
    ),
  );
}

function renderToolbar() {
  const ready = state.phase === 'ready';
  const edits = hasLocalEdits();
  const { errors } = validateModel(state.model);
  const info = state.info;

  $('#btn-connect').hidden = connected();
  $('#btn-connect').disabled = state.phase !== 'disconnected';
  $('#btn-disconnect').hidden = !connected();
  $('#btn-disconnect').disabled = !ready;
  $('#btn-save').disabled = !ready || !edits || errors.length > 0;
  $('#btn-save').title = errors.length
    ? 'Fix the problems listed above first'
    : 'Save to the device; it restarts with the new settings';
  $('#btn-defaults').disabled = !ready;
  $('#btn-reload').disabled = !ready;
  $('#btn-export').disabled = !state.model || !info;
  $('#btn-import').disabled = !ready;
  $('#adv-toggle').checked = state.showAdvanced;
}

function renderMessages() {
  const box = $('#messages');
  box.replaceChildren();
  if (state.message) box.append(el('div', { class: `msg ${state.message.kind}` }, state.message.text));
  if (!state.model || !connected()) return;
  const { errors, warnings } = validateModel(state.model);
  for (const e of errors) box.append(el('div', { class: 'msg error' }, e));
  for (const w of warnings) box.append(el('div', { class: 'msg warn' }, w));
}

function renderForm() {
  const form = $('#form');
  form.classList.toggle('show-adv', state.showAdvanced);
  form.replaceChildren();
  if (!state.model || !connected()) {
    if (state.phase === 'disconnected' || state.phase === 'connecting') form.append(welcome());
    return;
  }

  const m = state.model;
  const encUsed = m.encoders.filter((enc, e) => encoderInUse(enc) || buttonInUse(m.buttons[e])).length;
  const btnUsed = Array.from({ length: N_PANEL_BUTTONS }, (_, p) => buttonInUse(m.buttons[N_ENCODER_BUTTONS + p])).filter(Boolean).length;
  // Section heading with "Expand all" / "Collapse all" for the cards (ids) in that section.
  const setAll = (ids, open) => () => {
    for (const id of ids) state.open[id] = open;
    render();
  };
  const section = (title, count, ids) =>
    el(
      'h2',
      { class: 'section' },
      el('span', {}, title),
      count ? el('span', { class: 'count' }, count) : null,
      el('span', { class: 'rule' }),
      el('button', { class: 'link', onclick: setAll(ids, true) }, 'Expand all'),
      el('button', { class: 'link', onclick: setAll(ids, false) }, 'Collapse all'),
    );
  const encIds = Array.from({ length: N_ENCODERS }, (_, e) => `enc-${e}`);
  const btnIds = Array.from({ length: N_PANEL_BUTTONS }, (_, p) => `btn-${p}`);

  form.append(
    el('p', { class: 'muted form-hint' }, 'Click a card header to expand or collapse it. Hover a label with a dotted underline for an explanation.'),
    section('General', null, ['defaults', 'leds']),
    el('div', { class: 'grid' }, globalsCards()),
    section('Encoders', `${encUsed} of ${N_ENCODERS} in use`, encIds),
    el('div', { class: 'grid' }, Array.from({ length: N_ENCODERS }, (_, e) => encoderCard(e))),
    section('Panel buttons', `${btnUsed} of ${N_PANEL_BUTTONS} in use`, btnIds),
    el(
      'div',
      { class: 'grid' },
      Array.from({ length: N_PANEL_BUTTONS }, (_, p) => {
        const i = N_ENCODER_BUTTONS + p;
        const b = m.buttons[i];
        return card(
          {
            id: btnIds[p],
            title: `Button ${p + 1}`,
            kind: 'button',
            used: buttonInUse(b),
            summary: actionsSummary([
              ['Short', b.short],
              ['Long', b.long],
            ]),
          },
          buttonSubs(i),
        );
      }),
    ),
  );
}

/** Shown instead of the form until a device is connected. */
function welcome() {
  const busy = state.phase === 'connecting';
  return el(
    'section',
    { class: 'welcome' },
    el('div', { class: 'welcome-icon', 'aria-hidden': 'true' }),
    el('h2', {}, 'Connect your KB-E2B6'),
    el(
      'ol',
      { class: 'steps' },
      el('li', {}, 'Plug the controller into this computer with a data USB cable.'),
      el('li', {}, 'Click ', el('b', {}, 'Connect device'), ' and pick ', el('b', {}, 'KB-E2B6'), ' in the browser’s list.'),
      el('li', {}, 'Change what you need, then click ', el('b', {}, 'Save to device'), '. It restarts with the new settings.'),
    ),
    el('button', { class: 'primary big', disabled: busy, onclick: onConnect }, busy ? 'Connecting…' : 'Connect device'),
  );
}

function render() {
  if (state.phase === 'unsupported') {
    $('#unsupported').hidden = false;
  }
  renderStatus();
  renderToolbar();
  renderMessages();
  renderForm();
}

// ---------------------------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------------------------
function wire() {
  $('#btn-connect').addEventListener('click', onConnect);
  $('#btn-disconnect').addEventListener('click', onDisconnect);
  $('#btn-save').addEventListener('click', onSave);
  $('#btn-defaults').addEventListener('click', onDefaults);
  $('#btn-reload').addEventListener('click', onReload);
  $('#btn-export').addEventListener('click', onExport);
  $('#btn-import').addEventListener('click', () => $('#file').click());
  $('#file').addEventListener('change', (ev) => {
    const f = ev.target.files?.[0];
    ev.target.value = '';
    if (f) onImportFile(f);
  });
  $('#adv-toggle').addEventListener('change', (ev) => {
    state.showAdvanced = ev.target.checked;
    render();
  });

  window.addEventListener('beforeunload', (ev) => {
    if (connected() && hasLocalEdits()) {
      ev.preventDefault();
      ev.returnValue = '';
    }
  });

  navigator.hid.addEventListener('disconnect', async (ev) => {
    if (!state.transport || ev.device !== state.transport.device) return;
    if (state.phase === 'rebooting') return; // expected
    log('device disconnected');
    await detach();
    setMessage('warn', 'Device disconnected.');
    render();
  });

  navigator.hid.addEventListener('connect', async (ev) => {
    if (state.phase !== 'disconnected' || !isConfigDevice(ev.device)) return;
    const devs = await authorizedConfigDevices();
    if (devs.length === 1) {
      log('device plugged in, connecting');
      await attach(new HidTransport(devs[0]));
    }
  });
}

async function start() {
  if (!webHidSupported()) {
    state.phase = 'unsupported';
    render();
    return;
  }
  if (!window.isSecureContext) {
    state.phase = 'unsupported';
    setMessage('error', 'Open index.html directly from disk in a Chromium-based browser (Chrome, Edge, Brave…).');
    render();
    return;
  }
  wire();
  render();

  // Returning user: reconnect to a previously authorized device without the chooser.
  const devs = await authorizedConfigDevices();
  if (devs.length === 1) await attach(new HidTransport(devs[0]));
  else if (devs.length > 1) setMessage('info', 'Several KB-E2B6 boards found — click Connect and pick one.');
  render();
}

start();
})();
