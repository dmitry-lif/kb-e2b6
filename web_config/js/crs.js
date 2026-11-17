// Plain script (no ES modules) so index.html works when opened directly from disk.
(() => {
  'use strict';
  const KB = (window.KB = window.KB || {});

// CRS v1 codec (288-byte config blob) + CRC-32, mirroring firmware/crs_tools/crs_format.
// Spec: firmware/docs/specs/Spec - CRS Format.md. All integers little-endian.

const CRS_MAGIC = 0x31535243; // "CRS1"
const CRS_SCHEMA_VERSION = 1;
const HEADER_LEN = 28;
const PAYLOAD_LEN = 260;
const TOTAL_LEN = HEADER_LEN + PAYLOAD_LEN; // 288

const N_ENCODER_BUTTONS = 2;
const N_PANEL_BUTTONS = 6;
const N_BUTTONS = N_ENCODER_BUTTONS + N_PANEL_BUTTONS; // CRS order: encoder pushes, then panel
const N_ENCODERS = 2;

// Absolute offsets inside the blob.
const OFF_GLOBALS = HEADER_LEN; // 28, 28 bytes
const OFF_BUTTONS = OFF_GLOBALS + 28; // 56, 8 x 24 bytes
const OFF_ENCODERS = OFF_BUTTONS + N_BUTTONS * 24; // 248, 2 x 20 bytes
const BUTTON_LEN = 24;
const ENCODER_LEN = 20;

// ---------------------------------------------------------------------------------------------
// CRC-32 (IEEE 802.3, reflected 0xEDB88320, init/xorout 0xFFFFFFFF) — same as the firmware.
// Check value: crc32("123456789") === 0xCBF43926.
// ---------------------------------------------------------------------------------------------
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[i] = c >>> 0;
  }
  return t;
})();

function crc32(bytes) {
  let c = 0xffffffff;
  for (let i = 0; i < bytes.length; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

// ---------------------------------------------------------------------------------------------
// Model helpers
// ---------------------------------------------------------------------------------------------
const NONE_ACTION = () => ({ tag: 0, a: 0, b: 0 });

function readAction(v, off) {
  return { tag: v.getUint8(off), a: v.getUint8(off + 1), b: v.getUint8(off + 2) };
}

function writeAction(v, off, a) {
  v.setUint8(off, a.tag);
  v.setUint8(off + 1, a.a);
  v.setUint8(off + 2, a.b);
  v.setUint8(off + 3, 0);
}

class CrsError extends Error {}

/** Decode a 288-byte CRS v1 blob into `{ header, model }`. Throws CrsError on a bad blob. */
function decodeCrs(bytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length !== TOTAL_LEN) {
    throw new CrsError(`config must be exactly ${TOTAL_LEN} bytes (got ${bytes?.length ?? 0})`);
  }
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);

  const header = {
    magic: v.getUint32(0, true),
    schema: v.getUint16(4, true),
    fwCompatId: v.getUint32(8, true),
    len: v.getUint16(12, true),
    crc32: v.getUint32(16, true),
    buildId: v.getUint32(20, true),
  };
  if (header.magic !== CRS_MAGIC) throw new CrsError('not a CRS v1 config (bad magic)');
  if (header.schema !== CRS_SCHEMA_VERSION) throw new CrsError(`unsupported CRS schema ${header.schema}`);
  if (header.len !== PAYLOAD_LEN) throw new CrsError(`bad payload length ${header.len}`);
  const crc = crc32(bytes.subarray(HEADER_LEN));
  if (crc !== header.crc32) throw new CrsError('config CRC mismatch (corrupted file?)');

  const g = OFF_GLOBALS;
  const globals = {
    scanHz: v.getUint16(g + 0, true),
    pinPullDefault: v.getUint8(g + 2),
    activeLowDefault: v.getUint8(g + 3),
    debounceMsDefault: v.getUint16(g + 4, true),
    longPressMsDefault: v.getUint16(g + 6, true),
    pulseMsButtonDefault: v.getUint16(g + 8, true),
    repeatPeriodMsDefault: v.getUint16(g + 10, true),
    encoderPulseMsDefault: v.getUint16(g + 12, true),
    encoderSwapAbDefault: v.getUint8(g + 14),
    encoderDetentGuardMsDefault: v.getUint8(g + 15),
    ledStatePersist: v.getUint8(g + 16),
    ledStateOnBoot: v.getUint8(g + 17),
    ledLevelPersist: v.getUint8(g + 18),
    ledLevelOnBoot: v.getUint8(g + 19),
    ledLevelDefault: v.getUint8(g + 20),
    ledLevelStep: v.getUint8(g + 21),
    ledLevelMin: v.getUint8(g + 22),
    ledLevelMax: v.getUint8(g + 23),
    ledAutosaveDelayMs: v.getUint16(g + 24, true),
  };

  const buttons = [];
  for (let i = 0; i < N_BUTTONS; i++) {
    const o = OFF_BUTTONS + i * BUTTON_LEN;
    const enabled = v.getUint8(o) === 1;
    buttons.push(
      enabled
        ? {
            pull: v.getInt8(o + 1),
            activeLow: v.getInt8(o + 2),
            debounceMs: v.getInt16(o + 3, true),
            longPressMs: v.getInt16(o + 5, true),
            pulseMs: v.getInt16(o + 7, true),
            repeatPeriodMs: v.getUint16(o + 9, true),
            short: readAction(v, o + 12),
            long: readAction(v, o + 16),
          }
        : newButton(),
    );
  }

  const encoders = [];
  for (let i = 0; i < N_ENCODERS; i++) {
    const o = OFF_ENCODERS + i * ENCODER_LEN;
    const enabled = v.getUint8(o) === 1;
    encoders.push(
      enabled
        ? {
            swapAb: v.getInt8(o + 1),
            detentGuardMs: v.getInt16(o + 2, true),
            pulseMs: v.getUint16(o + 4, true),
            cw: readAction(v, o + 8),
            ccw: readAction(v, o + 12),
          }
        : newEncoder(globals),
    );
  }

  return { header, model: { globals, buttons, encoders } };
}

/** An unused button that inherits every setting from the globals. */
function newButton() {
  return {
    pull: -1,
    activeLow: -1,
    debounceMs: -1,
    longPressMs: -1,
    pulseMs: -1,
    repeatPeriodMs: 0,
    short: NONE_ACTION(),
    long: NONE_ACTION(),
  };
}

/** An unused encoder (default step pulse from the globals). */
function newEncoder(globals) {
  return {
    swapAb: -1,
    detentGuardMs: -1,
    pulseMs: globals?.encoderPulseMsDefault || 30,
    cw: NONE_ACTION(),
    ccw: NONE_ACTION(),
  };
}

function buttonInUse(b) {
  return b.short.tag !== 0 || b.long.tag !== 0;
}

function encoderInUse(e) {
  return e.cw.tag !== 0 || e.ccw.tag !== 0;
}

/**
 * Encode a model into a 288-byte CRS v1 blob.
 * Canonical like crs-pack: a control is enabled iff it has at least one action; unused controls are
 * written as all-zero entries.
 */
function encodeCrs(model, fwCompatId, buildId) {
  const bytes = new Uint8Array(TOTAL_LEN);
  const v = new DataView(bytes.buffer);
  const G = model.globals;

  const g = OFF_GLOBALS;
  v.setUint16(g + 0, G.scanHz, true);
  v.setUint8(g + 2, G.pinPullDefault);
  v.setUint8(g + 3, G.activeLowDefault);
  v.setUint16(g + 4, G.debounceMsDefault, true);
  v.setUint16(g + 6, G.longPressMsDefault, true);
  v.setUint16(g + 8, G.pulseMsButtonDefault, true);
  v.setUint16(g + 10, G.repeatPeriodMsDefault, true);
  v.setUint16(g + 12, G.encoderPulseMsDefault, true);
  v.setUint8(g + 14, G.encoderSwapAbDefault);
  v.setUint8(g + 15, G.encoderDetentGuardMsDefault);
  v.setUint8(g + 16, G.ledStatePersist);
  v.setUint8(g + 17, G.ledStateOnBoot);
  v.setUint8(g + 18, G.ledLevelPersist);
  v.setUint8(g + 19, G.ledLevelOnBoot);
  v.setUint8(g + 20, G.ledLevelDefault);
  v.setUint8(g + 21, G.ledLevelStep);
  v.setUint8(g + 22, G.ledLevelMin);
  v.setUint8(g + 23, G.ledLevelMax);
  v.setUint16(g + 24, G.ledAutosaveDelayMs, true);
  // g+26..27 reserved = 0

  model.buttons.forEach((b, i) => {
    if (!buttonInUse(b)) return; // all-zero entry
    const o = OFF_BUTTONS + i * BUTTON_LEN;
    v.setUint8(o, 1);
    v.setInt8(o + 1, b.pull);
    v.setInt8(o + 2, b.activeLow);
    v.setInt16(o + 3, b.debounceMs, true);
    v.setInt16(o + 5, b.longPressMs, true);
    v.setInt16(o + 7, b.pulseMs, true);
    v.setUint16(o + 9, b.repeatPeriodMs, true);
    writeAction(v, o + 12, b.short);
    writeAction(v, o + 16, b.long);
  });

  model.encoders.forEach((e, i) => {
    if (!encoderInUse(e)) return;
    const o = OFF_ENCODERS + i * ENCODER_LEN;
    v.setUint8(o, 1);
    v.setInt8(o + 1, e.swapAb);
    v.setInt16(o + 2, e.detentGuardMs, true);
    v.setUint16(o + 4, e.pulseMs, true);
    writeAction(v, o + 8, e.cw);
    writeAction(v, o + 12, e.ccw);
  });

  // Header.
  v.setUint32(0, CRS_MAGIC, true);
  v.setUint16(4, CRS_SCHEMA_VERSION, true);
  v.setUint16(6, 0, true); // flags
  v.setUint32(8, fwCompatId >>> 0, true);
  v.setUint16(12, PAYLOAD_LEN, true);
  v.setUint32(16, crc32(bytes.subarray(HEADER_LEN)), true);
  v.setUint32(20, buildId >>> 0, true);
  return bytes;
}

/** True when two CRS blobs carry the same configuration (header/build id ignored). */
function samePayload(a, b) {
  if (!a || !b || a.length !== TOTAL_LEN || b.length !== TOTAL_LEN) return false;
  for (let i = HEADER_LEN; i < TOTAL_LEN; i++) if (a[i] !== b[i]) return false;
  return true;
}

function cloneModel(m) {
  return JSON.parse(JSON.stringify(m));
}

/** Friendly name of a CRS button index (0..1 encoder pushes, 2..7 panel buttons). */
function buttonName(i) {
  return i < N_ENCODER_BUTTONS ? `Encoder ${i + 1} push` : `Panel button ${i - N_ENCODER_BUTTONS + 1}`;
}

  KB.crs = {
    CRS_MAGIC, CRS_SCHEMA_VERSION, HEADER_LEN, PAYLOAD_LEN, TOTAL_LEN, N_ENCODER_BUTTONS, N_PANEL_BUTTONS, N_BUTTONS, N_ENCODERS, crc32, CrsError, decodeCrs, newButton, newEncoder, buttonInUse, encoderInUse, encodeCrs, samePayload, cloneModel, buttonName,
  };
})();
