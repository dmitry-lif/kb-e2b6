// Plain script (no ES modules) so index.html works when opened directly from disk.
(() => {
  'use strict';
  const KB = (window.KB = window.KB || {});

// Curated action catalog + dropdown option lists for the KB-E2B6 web configurator.
//
// Source of truth for the action set: firmware/docs/specs/Spec - Action Set.md.
// Numeric settings deliberately offer only a few sensible choices; a value read from
// the device that is not in a list is still shown (as "(current)") so nothing is lost.

// Action tags on the wire (CRS v1 ActionWireV1.tag).
const TAG_NONE = 0;
const TAG_KBD = 1;
const TAG_CC = 2;
const TAG_INTERNAL = 3;

// Keyboard modifier bits (left-hand modifiers; right-hand bits are preserved if present).
const MODIFIERS = [
  { bit: 0x01, label: 'Ctrl' },
  { bit: 0x02, label: 'Shift' },
  { bit: 0x04, label: 'Alt' },
  { bit: 0x08, label: 'Win' },
];

/** "Ctrl+Shift" style name for a modifier bit mask (left-hand modifiers). */
function modsLabel(bits) {
  return MODIFIERS.filter((m) => bits & m.bit).map((m) => m.label).join('+');
}

/**
 * Modifier dropdown choices: [bits, label] for every combination of the 4 left-hand modifiers,
 * single modifiers first, then pairs, triples, all four.
 */
const MODIFIER_COMBOS = (() => {
  const combos = [];
  for (let bits = 1; bits < 16; bits++) combos.push(bits);
  const count = (b) => (b & 1) + ((b >> 1) & 1) + ((b >> 2) & 1) + ((b >> 3) & 1);
  combos.sort((a, b) => count(a) - count(b) || a - b);
  return [[0, 'None'], ...combos.map((b) => [b, modsLabel(b)])];
})();

function range(first, count, fn) {
  return Array.from({ length: count }, (_, i) => fn(first + i, i));
}

const LETTERS = range(0x04, 26, (u, i) => ['kbd', u, String.fromCharCode(65 + i)]);
const DIGITS = [...range(0x1e, 9, (u, i) => ['kbd', u, String(i + 1)]), ['kbd', 0x27, '0']];
const F1_F12 = range(0x3a, 12, (u, i) => ['kbd', u, `F${i + 1}`]);
const PROG = ['PROG red', 'PROG green', 'PROG yellow', 'PROG blue'];
const F13_F24 = range(0x68, 12, (u, i) => ['kbd', u, i < 4 ? `F${i + 13} (${PROG[i]})` : `F${i + 13}`]);

// [kind, code, label] — kind: 'cc' (Consumer Control usage), 'kbd' (Keyboard usage), 'int' (device).
const ACTION_GROUPS = [
  {
    label: 'Media',
    items: [
      ['cc', 0x00cd, 'Play / Pause'],
      ['cc', 0x00b5, 'Next track'],
      ['cc', 0x00b6, 'Previous track'],
      ['cc', 0x00b7, 'Stop'],
      ['cc', 0x00b3, 'Fast forward'],
      ['cc', 0x00b4, 'Rewind'],
      ['cc', 0x00e9, 'Volume up'],
      ['cc', 0x00ea, 'Volume down'],
      ['cc', 0x00e2, 'Mute'],
    ],
  },
  {
    label: 'Android / system',
    items: [
      ['cc', 0x0223, 'Home'],
      ['cc', 0x0224, 'Back'],
      ['cc', 0x0040, 'Menu'],
      ['cc', 0x0221, 'Search'],
      ['cc', 0x00cf, 'Voice assistant'],
      ['cc', 0x0183, 'Open media player'],
    ],
  },
  {
    label: 'Screen brightness (head unit)',
    items: [
      ['cc', 0x006f, 'Brightness up'],
      ['cc', 0x0070, 'Brightness down'],
    ],
  },
  {
    label: 'Navigation keys',
    items: [
      ['kbd', 0x52, 'Arrow Up'],
      ['kbd', 0x51, 'Arrow Down'],
      ['kbd', 0x50, 'Arrow Left'],
      ['kbd', 0x4f, 'Arrow Right'],
      ['kbd', 0x28, 'Enter / OK'],
      ['kbd', 0x29, 'Escape (Back)'],
      ['kbd', 0x2a, 'Backspace'],
      ['kbd', 0x2b, 'Tab'],
      ['kbd', 0x2c, 'Space'],
      ['kbd', 0x4b, 'Page Up'],
      ['kbd', 0x4e, 'Page Down'],
      ['kbd', 0x4a, 'Home key'],
      ['kbd', 0x4d, 'End key'],
      ['kbd', 0x76, 'Menu key'],
      ['kbd', 0x65, 'Context menu key'],
    ],
  },
  { label: 'Function keys F1–F12', items: F1_F12 },
  { label: 'Function keys F13–F24', items: F13_F24 },
  { label: 'Letters', items: LETTERS },
  { label: 'Digits', items: DIGITS },
  {
    label: 'Controller LEDs (button/encoder backlight)',
    items: [
      ['int', 0x01, 'LED on/off (toggle)'],
      ['int', 0x02, 'LED brightness step (wraps)'],
      ['int', 0x03, 'LED on'],
      ['int', 0x04, 'LED off'],
      ['int', 0x05, 'LED brighter'],
      ['int', 0x06, 'LED dimmer'],
    ],
  },
];

const hex = (v, w) => v.toString(16).toUpperCase().padStart(w, '0');

/** Select-option key for an action (modifiers are edited separately). */
function actionKey(a) {
  switch (a.tag) {
    case TAG_KBD:
      return `kbd:${hex(a.b, 2)}`;
    case TAG_CC:
      return `cc:${hex(a.a | (a.b << 8), 4)}`;
    case TAG_INTERNAL:
      return `int:${hex(a.a, 2)}`;
    default:
      return 'none';
  }
}

/** Build an action from a select key; `mods` applies to keyboard actions only. */
function actionFromKey(key, mods = 0) {
  if (key === 'none') return { tag: TAG_NONE, a: 0, b: 0 };
  const [kind, code] = key.split(':');
  const v = parseInt(code, 16);
  if (kind === 'kbd') return { tag: TAG_KBD, a: mods & 0xff, b: v & 0xff };
  if (kind === 'cc') return { tag: TAG_CC, a: v & 0xff, b: (v >> 8) & 0xff };
  if (kind === 'int') return { tag: TAG_INTERNAL, a: v & 0xff, b: 0 };
  return { tag: TAG_NONE, a: 0, b: 0 };
}

/** Select-option key for a catalog entry. */
function catalogKey(kind, code) {
  return actionKey(actionFromKey(`${kind}:${hex(code, 4)}`));
}

const KEY_LABELS = new Map();
for (const g of ACTION_GROUPS) {
  for (const [kind, code, label] of g.items) {
    KEY_LABELS.set(catalogKey(kind, code), label);
  }
}

function isCatalogKey(key) {
  return key === 'none' || KEY_LABELS.has(key);
}

/** Human description of an action, including modifiers. */
function describeAction(a) {
  const key = actionKey(a);
  if (key === 'none') return 'Not used';
  let label = KEY_LABELS.get(key);
  if (!label) {
    if (a.tag === TAG_KBD) label = `Keyboard usage 0x${hex(a.b, 2)}`;
    else if (a.tag === TAG_CC) label = `Consumer usage 0x${hex(a.a | (a.b << 8), 4)}`;
    else label = `Device action 0x${hex(a.a, 2)}`;
  }
  if (a.tag === TAG_KBD && a.a) {
    const mods = MODIFIERS.filter((m) => a.a & m.bit).map((m) => m.label);
    if (a.a & 0xf0) mods.push(`R-mods 0x${hex(a.a & 0xf0, 2)}`);
    label = `${mods.join('+')}+${label}`;
  }
  return label;
}

// ---------------------------------------------------------------------------------------------
// Numeric / enum dropdown choices. Entries are [value, label].
// ---------------------------------------------------------------------------------------------

const ms = (v) => [v, `${v} ms`];
const pct = (v) => [v, `${v} %`];

const OPTIONS = {
  scanHz: [
    [1000, '1000 Hz (recommended)'],
    [2000, '2000 Hz'],
    [3000, '3000 Hz'],
  ],
  debounce: [3, 5, 10, 20, 30].map(ms),
  longPress: [400, 500, 600, 800, 1000].map(ms),
  pulse: [
    [0, 'Hold while pressed'],
    [30, 'Release after 30 ms'],
    [50, 'Release after 50 ms'],
    [100, 'Release after 100 ms'],
  ],
  repeat: [
    [0, 'Off'],
    [70, 'Every 70 ms (fast)'],
    [100, 'Every 100 ms'],
    [150, 'Every 150 ms'],
    [250, 'Every 250 ms (slow)'],
  ],
  encoderPulse: [10, 20, 30, 50].map(ms),
  detent: [0, 1, 2, 5].map(ms),
  swap: [
    [0, 'Normal'],
    [1, 'Reversed'],
  ],
  pull: [
    [1, 'Pull-up'],
    [2, 'Pull-down'],
    [0, 'None (external resistor)'],
  ],
  activeLow: [
    [1, 'Active-low (switch to GND)'],
    [0, 'Active-high'],
  ],
  yesNo: [
    [1, 'Yes'],
    [0, 'No'],
  ],
  ledStateOnBoot: [
    [2, 'Restore last'],
    [1, 'On'],
    [0, 'Off'],
  ],
  ledLevelOnBoot: [[0, 'Restore last'], ...[10, 30, 50, 100].map(pct)],
  ledLevelDefault: [10, 30, 50, 75, 100].map(pct),
  ledLevelStep: [1, 2, 5, 10, 20].map(pct),
  ledLevelMin: [1, 5, 10, 20].map(pct),
  ledLevelMax: [30, 50, 75, 100].map(pct),
  ledAutosave: [500, 1000, 1500, 3000, 5000].map(ms),
};

/** Label for a value from an option list (falls back to the raw value). */
function optionLabel(list, value, unit = '') {
  const hit = list.find(([v]) => v === value);
  return hit ? hit[1] : `${value}${unit}`;
}

  KB.catalog = {
    TAG_NONE, TAG_KBD, TAG_CC, TAG_INTERNAL, MODIFIERS, MODIFIER_COMBOS, modsLabel, ACTION_GROUPS, actionKey, actionFromKey, catalogKey, isCatalogKey, describeAction, OPTIONS, optionLabel,
  };
})();
