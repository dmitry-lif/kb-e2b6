// Plain script (no ES modules) so index.html works when opened directly from disk.
(() => {
  'use strict';
  const KB = (window.KB = window.KB || {});

// WebHID transport for the KB-E2B6 configuration interface.
// The device exposes a separate vendor-defined HID interface (usage page 0xFF00, usage 0x01,
// no report IDs, 64-byte input + output reports) next to its keyboard/media interface.

// Same values as firmware/kb_firmware/src/text.rs (USB_VID / USB_PID).
const VENDOR_ID = 0xcafe;
const PRODUCT_ID = 0x4002;
const USAGE_PAGE = 0xff00;
const USAGE = 0x0001;

const FILTERS = [{ vendorId: VENDOR_ID, productId: PRODUCT_ID, usagePage: USAGE_PAGE, usage: USAGE }];

function webHidSupported() {
  return typeof navigator !== 'undefined' && 'hid' in navigator;
}

/** True for the HIDDevice that carries the configuration collection. */
function isConfigDevice(d) {
  return (
    d.vendorId === VENDOR_ID &&
    d.productId === PRODUCT_ID &&
    d.collections.some((c) => c.usagePage === USAGE_PAGE && c.usage === USAGE)
  );
}

/** Ask the user to pick the device (must be called from a click handler). */
async function requestConfigDevice() {
  const picked = await navigator.hid.requestDevice({ filters: FILTERS });
  return picked.find(isConfigDevice) ?? null;
}

/** Devices this page already has permission for. */
async function authorizedConfigDevices() {
  const all = await navigator.hid.getDevices();
  return all.filter(isConfigDevice);
}

class HidTransport {
  constructor(device) {
    this.device = device;
    this.listeners = new Set();
    this.onInput = (e) => {
      if (e.reportId !== 0) return;
      const d = e.data;
      const bytes = new Uint8Array(d.buffer.slice(d.byteOffset, d.byteOffset + d.byteLength));
      for (const cb of this.listeners) cb(bytes);
    };
  }

  async open() {
    if (!this.device.opened) await this.device.open();
    this.device.addEventListener('inputreport', this.onInput);
  }

  async close() {
    this.device.removeEventListener('inputreport', this.onInput);
    try {
      if (this.device.opened) await this.device.close();
    } catch {
      // Already gone (unplugged/rebooted).
    }
  }

  async send(frame) {
    await this.device.sendReport(0, frame);
  }

  onFrame(cb) {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * After the device reboots (after Save), wait for it to re-enumerate and open it again.
 * Uses the page's existing permission (no chooser). Resolves with an opened HidTransport.
 */
async function reconnectAfterReboot(oldDevice, { timeoutMs = 12000, log = () => {} } = {}) {
  const deadline = Date.now() + timeoutMs;

  // Phase 1: wait until the old connection is gone (the device resets ~150 ms after replying).
  const goneBy = Date.now() + 3000;
  while (Date.now() < goneBy) {
    const still = (await authorizedConfigDevices()).some((d) => d === oldDevice);
    if (!still) break;
    await sleep(100);
  }

  // Phase 2: wait for re-enumeration (~1 s) and open whatever permitted config device appears.
  while (Date.now() < deadline) {
    const candidates = await authorizedConfigDevices();
    if (candidates.length > 1) {
      throw new Error('Several KB-E2B6 boards are connected — click Connect and pick one.');
    }
    if (candidates.length === 1) {
      const t = new HidTransport(candidates[0]);
      try {
        await t.open();
        return t;
      } catch (e) {
        log(`reopen attempt failed: ${e.message}`);
        await t.close();
      }
    }
    await sleep(300);
  }
  throw new Error('The device did not come back after rebooting. Re-plug it and click Connect.');
}

  KB.transport = {
    VENDOR_ID, PRODUCT_ID, USAGE_PAGE, USAGE, FILTERS, webHidSupported, isConfigDevice, requestConfigDevice, authorizedConfigDevices, HidTransport, reconnectAfterReboot,
  };
})();
