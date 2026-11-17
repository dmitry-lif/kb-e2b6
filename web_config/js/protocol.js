// Plain script (no ES modules) so index.html works when opened directly from disk.
(() => {
  'use strict';
  const KB = (window.KB = window.KB || {});
  const { crc32, CRS_SCHEMA_VERSION, TOTAL_LEN, buttonName } = KB.crs;
// WebHID configuration protocol v1 client (mirrors firmware/crs_tools/crs_format/src/webhid.rs).
// Spec: firmware/docs/specs/Spec - WebHID Config Protocol.md


const FRAME_LEN = 64;
const FRAME_HEADER_LEN = 12;
const MAX_FRAME_PAYLOAD = 52;
const PROTOCOL_VERSION = 1;
const DEVICE_MODEL_ID = 0xe2b6;

const OP = {
  GET_INFO: 0x01,
  READ_CONFIG: 0x02,
  WRITE_BEGIN: 0x10,
  WRITE_CHUNK: 0x11,
  WRITE_END: 0x12, // validate + save to flash + reboot
  PING: 0x30,
};

const FLAG = {
  RESPONSE: 0x01,
  ERROR: 0x02,
  MORE_DATA: 0x04,
  REBOOTING: 0x10,
};

// FACTORY_DEFAULTS = the built-in template shown as "suggested setup" in the UI (never auto-applied).
const READ_SELECT = { RUNNING: 0, FACTORY_DEFAULTS: 1 };

const STATUS_TEXT = {
  0x00: 'OK',
  0x01: 'Bad frame magic',
  0x02: 'Unsupported protocol version',
  0x03: 'Unknown command',
  0x04: 'Invalid length',
  0x05: 'Invalid offset',
  0x06: 'No transfer in progress',
  0x08: 'Config has the wrong size',
  0x09: 'Transfer CRC mismatch',
  0x0a: 'Unsupported config schema',
  0x0b: 'Config header invalid',
  0x0c: 'Config validation failed',
  0x0d: 'Device busy',
  0x0e: 'Saving to flash failed',
  0x0f: 'Internal device error',
  0x10: 'Device has no configuration',
  0x11: 'Config was made for a different firmware (compat ID mismatch)',
};

// ValidationCode messages (crs_format/src/validate.rs).
const VALIDATION_TEXT = {
  1: 'scan rate must be 1000, 2000 or 3000 Hz',
  2: 'default pull must be none/up/down',
  3: 'default active level must be 0/1',
  4: 'default debounce must be 1–50 ms',
  5: 'default long-press time must be 200–3000 ms',
  6: 'default tap duration must be 0–500 ms',
  7: 'default repeat must be off or 20–2000 ms',
  8: 'default encoder step pulse must be 1–200 ms',
  9: 'default encoder direction must be 0/1',
  10: 'default detent guard must be 0–10 ms',
  11: 'LED state persistence must be yes/no',
  12: 'LED state on boot must be off/on/restore',
  13: 'LED level persistence must be yes/no',
  14: 'LED level on boot must be 0–100',
  15: 'LED default level must be 1–100',
  16: 'LED step must be 1–50',
  17: 'LED minimum must be 1–100',
  18: 'LED maximum must be 1–100',
  19: 'LED minimum must not exceed LED maximum',
  20: 'LED autosave delay must be 0–60000 ms',
  21: 'reserved global bytes must be 0',
  32: 'enabled flag must be 0/1',
  33: 'pull must be default/none/up/down',
  34: 'active level must be default/low/high',
  35: 'debounce must be default or 1–50 ms',
  36: 'long-press time must be default or 200–3000 ms',
  37: 'tap duration must be default or 0–500 ms',
  38: 'repeat must be off or 20–2000 ms',
  39: 'repeat needs a tap duration (not "hold while pressed")',
  40: 'has no short or long action',
  41: 'reserved bytes must be 0',
  42: 'has an invalid action',
  48: 'enabled flag must be 0/1',
  49: 'direction must be default/normal/reversed',
  50: 'detent guard must be default or 0–10 ms',
  51: 'step pulse must be 1–200 ms',
  52: 'has no clockwise or counter-clockwise action',
  53: 'reserved bytes must be 0',
  54: 'has an invalid action',
};

const HEADER_DETAIL_TEXT = {
  1: 'wrong size',
  2: 'bad magic',
  3: 'unsupported schema',
  4: 'bad payload length',
  5: 'firmware compatibility ID mismatch',
  6: 'payload CRC mismatch',
};

const COMMIT_DETAIL_TEXT = {
  1: 'flash not available',
  2: 'slot sequence exhausted',
  3: 'erase failed',
  4: 'program failed',
  5: 'read-back verify failed',
  6: 'new slot not selected after write',
};

class DeviceError extends Error {
  constructor(opcode, status, detail) {
    super(describeStatus(opcode, status, detail));
    this.opcode = opcode;
    this.status = status;
    this.detail = detail;
  }
}

class TimeoutError extends Error {}

function describeStatus(opcode, status, detail) {
  let msg = STATUS_TEXT[status] ?? `Device status 0x${status.toString(16)}`;
  if (status === 0x0c && detail?.length >= 2) {
    const [code, index] = detail;
    const what = VALIDATION_TEXT[code] ?? `rule ${code}`;
    let where = 'Global settings';
    if (index !== 0xff) where = code >= 48 ? `Encoder ${index + 1}` : buttonName(index);
    msg += `: ${where} — ${what}`;
  } else if (status === 0x0b && detail?.length >= 1) {
    msg += `: ${HEADER_DETAIL_TEXT[detail[0]] ?? `code ${detail[0]}`}`;
  } else if (status === 0x0e && detail?.length >= 1) {
    msg += `: ${COMMIT_DETAIL_TEXT[detail[0]] ?? `code ${detail[0]}`}`;
  }
  return msg;
}

const INFO_LEN = 44; // minimum; newer firmware appends bytes 44..52

/** Parse the GET_INFO payload (44 bytes, or 52 with slot / LED journal details). */
function parseInfo(p) {
  const v = new DataView(p.buffer, p.byteOffset, p.byteLength);
  const uidHex = Array.from(p.subarray(32, 44), (b) => b.toString(16).toUpperCase().padStart(2, '0')).join('');
  return {
    serial: `KBE2B6-${uidHex}`, // same string as the USB serial number descriptor
    deviceModel: v.getUint16(0, true),
    fwVersion: `${v.getUint8(2)}.${v.getUint8(3)}.${v.getUint8(4)}`,
    protocolVersion: v.getUint8(5),
    crsSchemaVersion: v.getUint16(6, true),
    fwCompatId: v.getUint32(8, true),
    configLen: v.getUint16(12, true),
    hasConfig: (v.getUint8(14) & 0x01) !== 0,
    configCrc: v.getUint32(16, true),
    configBuildId: v.getUint32(20, true),
    capabilities: v.getUint32(24, true),
    maxFramePayload: v.getUint16(28, true),
    // Appended fields (null when the device sends the short 44-byte form).
    activeSlot: p.byteLength >= 52 ? [null, 'A', 'B'][v.getUint8(15)] ?? null : null,
    // Valid slots (empty and corrupted both read false): { A: bool, B: bool }.
    slotValid: p.byteLength >= 52 ? { A: (v.getUint8(30) & 1) !== 0, B: (v.getUint8(30) & 2) !== 0 } : null,
    configSeq: p.byteLength >= 52 ? v.getUint32(44, true) : null,
    ledJournalUsed: p.byteLength >= 52 ? v.getUint16(48, true) : null,
    ledJournalCap: p.byteLength >= 52 ? v.getUint16(50, true) : null,
  };
}

/**
 * Protocol client over a transport exposing `send(Uint8Array)` and `onFrame(cb) -> unsubscribe`.
 * Only one request is ever in flight; responses are matched by transaction ID.
 */
class KbClient {
  constructor(transport, log = () => {}) {
    this.transport = transport;
    this.log = log;
    this.nextTxid = 1;
    this.pending = null;
    this.unsubscribe = transport.onFrame((f) => this.#onFrame(f));
  }

  dispose(reason = 'disconnected') {
    this.unsubscribe?.();
    this.cancelPending(reason);
  }

  cancelPending(reason) {
    if (this.pending) this.pending.reject(new Error(reason));
  }

  #onFrame(f) {
    if (f.length < FRAME_HEADER_LEN || f[0] !== 0x4b || f[1] !== 0x42) return;
    if (!(f[4] & FLAG.RESPONSE)) return;
    const txid = f[6] | (f[7] << 8);
    if (!this.pending || txid !== this.pending.txid) {
      this.log(`ignored stale response txid=${txid}`);
      return;
    }
    const len = Math.min(f[10], MAX_FRAME_PAYLOAD);
    this.pending.resolve({
      opcode: f[3],
      flags: f[4],
      status: f[5],
      offset: f[8] | (f[9] << 8),
      payload: f.slice(FRAME_HEADER_LEN, FRAME_HEADER_LEN + len),
    });
  }

  async request(opcode, { offset = 0, payload = new Uint8Array(0), timeoutMs = 1500 } = {}) {
    if (this.pending) throw new Error('another request is in progress');
    const txid = this.nextTxid;
    this.nextTxid = (this.nextTxid % 0xffff) + 1;

    const frame = new Uint8Array(FRAME_LEN);
    frame[0] = 0x4b; // 'K'
    frame[1] = 0x42; // 'B'
    frame[2] = PROTOCOL_VERSION;
    frame[3] = opcode;
    frame[6] = txid & 0xff;
    frame[7] = txid >> 8;
    frame[8] = offset & 0xff;
    frame[9] = offset >> 8;
    frame[10] = payload.length;
    frame.set(payload, FRAME_HEADER_LEN);

    const response = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending = null;
        reject(new TimeoutError(`no response to command 0x${opcode.toString(16)} (timeout)`));
      }, timeoutMs);
      this.pending = {
        txid,
        resolve: (r) => {
          clearTimeout(timer);
          this.pending = null;
          resolve(r);
        },
        reject: (e) => {
          clearTimeout(timer);
          this.pending = null;
          reject(e);
        },
      };
    });

    try {
      await this.transport.send(frame);
    } catch (e) {
      this.cancelPending(`send failed: ${e.message}`);
      throw e;
    }
    const r = await response;
    if (r.opcode !== opcode) throw new Error(`response opcode mismatch (0x${r.opcode.toString(16)})`);
    if (r.status !== 0) throw new DeviceError(opcode, r.status, r.payload);
    return r;
  }

  async ping() {
    const r = await this.request(OP.PING, { payload: new Uint8Array([0x4b, 0x42]) });
    return r.payload;
  }

  async getInfo() {
    const r = await this.request(OP.GET_INFO);
    if (r.payload.length < INFO_LEN) throw new Error('GET_INFO response too short');
    return parseInfo(r.payload);
  }

  /** Read the 288-byte config (running, or the built-in suggested setup) in 52-byte chunks. */
  async readConfig(selector = READ_SELECT.RUNNING) {
    const out = new Uint8Array(TOTAL_LEN);
    let offset = 0;
    while (offset < TOTAL_LEN) {
      const r = await this.request(OP.READ_CONFIG, {
        offset,
        payload: new Uint8Array([selector]),
      });
      if (r.offset !== offset || r.payload.length === 0) throw new Error('READ_CONFIG out of sequence');
      out.set(r.payload.subarray(0, TOTAL_LEN - offset), offset);
      offset += r.payload.length;
      if (!(r.flags & FLAG.MORE_DATA)) break;
    }
    if (offset !== TOTAL_LEN) throw new Error(`READ_CONFIG returned ${offset} of ${TOTAL_LEN} bytes`);
    return out;
  }

  /**
   * "Save to device": upload a config; the device validates it, saves it to flash (A/B) and
   * reboots into it (response flag REBOOTING). If it equals the running config, nothing happens.
   * On any error nothing changes on the device.
   */
  async saveConfig(bytes) {
    if (bytes.length !== TOTAL_LEN) throw new Error('config must be 288 bytes');
    const begin = new Uint8Array(8);
    const v = new DataView(begin.buffer);
    v.setUint16(0, TOTAL_LEN, true);
    v.setUint16(2, CRS_SCHEMA_VERSION, true);
    v.setUint32(4, crc32(bytes), true);
    await this.request(OP.WRITE_BEGIN, { payload: begin });

    for (let off = 0; off < TOTAL_LEN; off += MAX_FRAME_PAYLOAD) {
      const chunk = bytes.subarray(off, Math.min(off + MAX_FRAME_PAYLOAD, TOTAL_LEN));
      await this.request(OP.WRITE_CHUNK, { offset: off, payload: chunk });
    }

    // Includes a flash page erase + program + verify on the device.
    const r = await this.request(OP.WRITE_END, { timeoutMs: 6000 });
    return { rebooting: (r.flags & FLAG.REBOOTING) !== 0, saved: r.payload[0] === 1 };
  }
}

  KB.protocol = {
    FRAME_LEN, FRAME_HEADER_LEN, MAX_FRAME_PAYLOAD, PROTOCOL_VERSION, DEVICE_MODEL_ID, OP, FLAG, READ_SELECT, STATUS_TEXT, DeviceError, TimeoutError, INFO_LEN, parseInfo, KbClient,
  };
})();
