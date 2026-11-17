// Browser self-test for the configurator's codec and protocol client (no device needed).
// Open selftest.html directly from disk, like the app.

(() => {
'use strict';
const { crc32, decodeCrs, encodeCrs, samePayload, newButton, TOTAL_LEN, HEADER_LEN } = window.KB.crs;
const { KbClient, OP, FLAG, READ_SELECT } = window.KB.protocol;
const { actionFromKey, actionKey, catalogKey } = window.KB.catalog;

const results = [];
function check(name, cond, extra = '') {
  results.push({ name, ok: !!cond, extra });
}

function sampleModel() {
  const globals = {
    scanHz: 1000,
    pinPullDefault: 1,
    activeLowDefault: 1,
    debounceMsDefault: 5,
    longPressMsDefault: 600,
    pulseMsButtonDefault: 50,
    repeatPeriodMsDefault: 0,
    encoderPulseMsDefault: 30,
    encoderSwapAbDefault: 0,
    encoderDetentGuardMsDefault: 2,
    ledStatePersist: 1,
    ledStateOnBoot: 2,
    ledLevelPersist: 1,
    ledLevelOnBoot: 0,
    ledLevelDefault: 30,
    ledLevelStep: 5,
    ledLevelMin: 10,
    ledLevelMax: 100,
    ledAutosaveDelayMs: 1500,
  };
  const buttons = Array.from({ length: 8 }, newButton);
  buttons[0].short = actionFromKey('cc:00E2');
  buttons[2].short = actionFromKey('kbd:52', 0x03);
  buttons[2].long = actionFromKey('int:02');
  buttons[2].repeatPeriodMs = 100;
  buttons[7].short = actionFromKey('cc:0223');
  const enc = (cw, ccw) => ({ swapAb: -1, detentGuardMs: -1, pulseMs: 30, cw, ccw });
  const encoders = [enc(actionFromKey('cc:00E9'), actionFromKey('cc:00EA')), enc(actionFromKey('kbd:51'), actionFromKey('kbd:52'))];
  return { globals, buttons, encoders };
}

/** Minimal fake device speaking protocol v1 (READ_CONFIG / WRITE_* only). */
class FakeTransport {
  constructor(blob) {
    this.blob = blob;
    this.cb = null;
    this.staged = null;
  }
  onFrame(cb) {
    this.cb = cb;
    return () => (this.cb = null);
  }
  async send(req) {
    const res = new Uint8Array(64);
    res.set(req.subarray(0, 12));
    res[4] = FLAG.RESPONSE;
    res[10] = 0;
    const op = req[3];
    const off = req[8] | (req[9] << 8);
    if (op === OP.READ_CONFIG) {
      const n = Math.min(52, TOTAL_LEN - off);
      res.set(this.blob.subarray(off, off + n), 12);
      res[10] = n;
      if (off + n < TOTAL_LEN) res[4] |= FLAG.MORE_DATA;
    } else if (op === OP.WRITE_BEGIN) {
      this.staged = new Uint8Array(TOTAL_LEN);
    } else if (op === OP.WRITE_CHUNK) {
      this.staged.set(req.subarray(12, 12 + req[10]), off);
    } else if (op === OP.WRITE_END) {
      this.blob = this.staged;
      res[4] |= FLAG.REBOOTING;
      res[12] = 1;
      res[10] = 1;
    }
    setTimeout(() => this.cb?.(res), 1);
  }
}

async function run() {
  // CRC-32 check value (same as crs_format's compile-time assertion).
  check('crc32("123456789") == 0xCBF43926', crc32(new TextEncoder().encode('123456789')) === 0xcbf43926);

  // CRS round trip.
  const m = sampleModel();
  const blob = encodeCrs(m, 0x20260216, 42);
  check('encoded length 288', blob.length === 288);
  const { header, model } = decodeCrs(blob);
  check('header compat id', header.fwCompatId === 0x20260216);
  check('header build id', header.buildId === 42);
  check('re-encode is byte-identical', samePayload(encodeCrs(model, 0x20260216, 42), blob));
  check('unused button 3 is all-zero on wire', blob.subarray(56 + 3 * 24, 56 + 4 * 24).every((b) => b === 0));
  check('button 2 enabled flag', blob[56 + 2 * 24] === 1);
  check('kbd modifiers preserved', model.buttons[2].short.a === 0x03 && model.buttons[2].short.b === 0x52);
  check('AC Home usage LE', blob[56 + 7 * 24 + 12] === 2 && blob[56 + 7 * 24 + 13] === 0x23 && blob[56 + 7 * 24 + 14] === 0x02);

  let corrupt = blob.slice();
  corrupt[HEADER_LEN + 5] ^= 1;
  let threw = false;
  try {
    decodeCrs(corrupt);
  } catch {
    threw = true;
  }
  check('corrupted payload rejected (CRC)', threw);

  // Catalog keys.
  check('catalog key round trip', actionKey(actionFromKey(catalogKey('cc', 0x00cd))) === 'cc:00CD');

  // Protocol client against the fake device.
  const fake = new FakeTransport(blob);
  const client = new KbClient(fake);
  const read = await client.readConfig(READ_SELECT.RUNNING);
  check('READ_CONFIG reassembles 288 bytes', read.length === 288 && read.every((b, i) => b === blob[i]));
  const m2 = sampleModel();
  m2.buttons[4].short = actionFromKey('kbd:29');
  const blob2 = encodeCrs(m2, 0x20260216, 7);
  const w = await client.saveConfig(blob2);
  check('WRITE_* uploads identical bytes', fake.blob.every((b, i) => b === blob2[i]));
  check('WRITE_END reports REBOOTING', w.rebooting === true);

  const table = document.getElementById('results');
  let failed = 0;
  for (const r of results) {
    if (!r.ok) failed++;
    const tr = document.createElement('tr');
    tr.innerHTML = `<td>${r.ok ? '✅' : '❌'}</td><td>${r.name}</td><td>${r.extra}</td>`;
    table.append(tr);
  }
  document.getElementById('summary').textContent = failed
    ? `${failed} of ${results.length} checks FAILED`
    : `All ${results.length} checks passed`;
}

run().catch((e) => {
  document.getElementById('summary').textContent = `Self-test crashed: ${e.message}`;
});
})();
