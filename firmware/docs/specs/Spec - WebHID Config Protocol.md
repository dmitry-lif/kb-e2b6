# Spec — WebHID Config Protocol: the browser configurator's USB protocol (v1)

Status: **normative**. Scope: the USB interface, framing, commands and device behavior used by the browser configurator ([`web_config/`](../../../web_config/README.md)) to read the device configuration and save a new one.

Implementation:
- Shared, host-testable protocol code: `crs_tools/crs_format/src/webhid.rs` (framing, statuses, chunked write session, GET_INFO layout) and `crs_tools/crs_format/src/validate.rs` (validator).
- Firmware glue: `kb_firmware/src/webhid/mod.rs`; save via `config/slot_writer.rs`; defaults in `config/defaults.rs`.
- Browser client: `web_config/js/protocol.js`, `transport.js`, `crs.js`.

---

## 1) Principles

- **One user action: "Save to device".** The device validates the new configuration, stores it in flash (A/B slots) and reboots into it. If validation or the flash write fails, nothing changes.
- WebHID is transport only. The firmware stays authoritative for validation, A/B slot selection, integrity and recovery. The host can't choose a slot, address flash or read/write raw memory, and the protocol never updates firmware. GET_INFO does *report* slot status (active slot, valid slots, and `seq`, the save counter from [Spec - CRS Slots §4](Spec%20-%20CRS%20Slots.md#4-slotheader-v1-32-bytes-byte-layout)) for display.
- The configuration wire format is **CRS v1** (288 bytes, [Spec - CRS Format](Spec%20-%20CRS%20Format.md)): already versioned, fixed-size, little-endian, CRC-protected and compat-gated.
- Configuration stays boot-time only: the running tasks never see a config change; a saved config takes effect through a reboot.

---

## 2) USB interface

A separate vendor-defined HID interface is added to the composite device (keyboard/consumer HID and CDC are unchanged):

| Property | Value |
|---|---|
| VID / PID | `0xCAFE` / `0x4002` (hobby/test IDs; `kb_firmware/src/text.rs`), bcdDevice 2.00 |
| Serial number | `KBE2B6-` + the MCU's 96-bit unique ID in hex (24 digits), unique per unit |
| Usage page / usage | `0xFF00` / `0x0001` |
| Report IDs | none (WebHID: `sendReport(0, data)`, input `reportId == 0`) |
| Input / output report | 64 bytes each (interrupt IN + interrupt OUT) |
| Poll interval | 5 ms |

Linux hosts need read/write access to the hidraw node (udev rule in `web_config/linux/`).

---

## 3) Frame (64 bytes, both directions)

| Offset | Size | Field |
|---:|---:|---|
| 0 | 1 | `'K'` (0x4B) |
| 1 | 1 | `'B'` (0x42) |
| 2 | 1 | protocol version = 1 |
| 3 | 1 | opcode |
| 4 | 1 | flags |
| 5 | 1 | status (responses) |
| 6 | 2 | transaction ID (LE), echoed by the device |
| 8 | 2 | offset (LE) |
| 10 | 1 | payload length (0..52) |
| 11 | 1 | reserved (0) |
| 12 | 52 | payload, zero padded |

Flags: bit0 `RESPONSE`, bit1 `ERROR` (status ≠ OK), bit2 `MORE_DATA`, bit3 reserved, bit4 `REBOOTING` (the device restarts right after this response). One request in flight; the client matches responses by transaction ID and ignores stale ones.

---

## 4) Status codes

| Code | Name | Meaning |
|---:|---|---|
| 0x00 | OK | |
| 0x01 | BAD_MAGIC | not a protocol frame |
| 0x02 | UNSUPPORTED_PROTOCOL | version ≠ 1 |
| 0x03 | UNKNOWN_COMMAND | |
| 0x04 | INVALID_LENGTH | bad payload length / incomplete transfer / bad selector |
| 0x05 | INVALID_OFFSET | chunk out of sequence / read past end |
| 0x06 | NO_TRANSFER | WRITE_CHUNK/END without WRITE_BEGIN |
| 0x08 | BAD_CONFIG_SIZE | WRITE_BEGIN total ≠ 288 |
| 0x09 | BAD_CONFIG_CRC | transfer CRC mismatch |
| 0x0A | UNSUPPORTED_CONFIG_SCHEMA | CRS schema ≠ 1 |
| 0x0B | CONFIG_HEADER_INVALID | CRS header check failed; detail `[code, 0xFF]` (1 size, 2 magic, 4 len, 6 crc); schema and compat failures are reported as 0x0A and 0x11 with detail `[3, 0xFF]` and `[5, 0xFF]` |
| 0x0C | CONFIG_VALIDATION_FAILED | detail `[ValidationCode, CRS entry index]` (index 0xFF = globals) |
| 0x0D | BUSY | reserved |
| 0x0E | FLASH_ERROR | save failed; detail `[1 no flash, 2 seq exhausted, 3 erase, 4 program, 5 verify, 6 not selected]` |
| 0x0F | INTERNAL_ERROR | reserved |
| 0x10 | NO_CONFIG | READ_CONFIG of the running config while the device has none ([DisabledFallback](Spec%20-%20CRS%20Slots.md#7-slot-selection-algorithm-boot): no valid slot, inputs disabled) |
| 0x11 | FW_COMPAT_MISMATCH | CRS `fw_compat_id` ≠ firmware gate |

`ValidationCode` values are defined in `crs_format/src/validate.rs` (stable numbering).

---

## 5) Commands

| Opcode | Name | Request | Response |
|---:|---|---|---|
| 0x01 | GET_INFO | — | 52-byte info ([§6](#6-get_info-payload-52-bytes-le)) |
| 0x02 | READ_CONFIG | `offset`; payload `[selector]` (0 running config, 1 suggested setup) | ≤52 bytes at `offset`, `MORE_DATA` if more remain |
| 0x10 | WRITE_BEGIN | `u16 total=288, u16 schema=1, u32 crc32(all 288 bytes)` | — |
| 0x11 | WRITE_CHUNK | `offset == bytes received so far`; 1..52 bytes | — |
| 0x12 | WRITE_END | — | **Save**: `[1]` + `REBOOTING`, or `[0]` (unchanged) |
| 0x30 | PING | any | echo |

WRITE_BEGIN always restarts the transfer (a crashed client cannot wedge the device). A USB reset/unplug aborts it. Opcodes 0x20/0x21 are reserved.

### 5.1 WRITE_END (Save to device)

1. Transfer complete (288 bytes) and transfer CRC matches; the session ends in every case.
2. CRS header check (magic/schema/len/`fw_compat_id`/payload CRC) and the shared semantic validator (`validate_payload`). On failure: error status + detail; nothing changes.
3. Candidate payload equals the running (= saved) one → `OK [0]`, nothing written, no reboot.
4. Otherwise save to the **non-selected** slot as specified in [Spec - CRS Slots §8](Spec%20-%20CRS%20Slots.md#8-writer-rules) (`seq = max(valid seq) + 1`, `committed` double word programmed last, boot selector re-checked).
5. Failure → `FLASH_ERROR` + detail, no reboot; the previously saved slot is untouched and keeps running.
6. Success → `OK [1]` + `REBOOTING`. ~150 ms later (after waiting for any in-progress [LED journal](Spec%20-%20LED%20Runtime%20State%20Persistence.md) flash operation) the MCU performs a system reset and boots the new slot.

Power loss during the save is designed to leave the new slot uncommitted or torn (ECC error → invalid): the previous slot is still selected at boot. The flash erase stalls the CPU for tens of milliseconds (single bank, no read-while-write); inputs pause briefly, then the device reboots anyway. Each save alternates slots A/B, so 10,000 erase cycles per page allow ~20,000 saves.

---

## 6) GET_INFO payload (52 bytes, LE)

Bytes 15, 30 and 44..52 were filled in after the first layout; a client must accept a 44-byte reply (older firmware) and treat bytes 15, 30 and 44..52 as unknown (in the 44-byte form bytes 15 and 30 are reserved zeros).

| Offset | Type | Field |
|---:|---|---|
| 0 | u16 | device model = `0xE2B6` |
| 2 | u8×3 | firmware major, minor, patch |
| 5 | u8 | protocol version (1) |
| 6 | u16 | CRS schema version (1) |
| 8 | u32 | firmware compat ID (`FW_COMPAT_ID`) |
| 12 | u16 | config length (288) |
| 14 | u8 | state flags: bit0 device runs a valid saved config (else inputs disabled) |
| 15 | u8 | active slot: 0 none, 1 A, 2 B (slot the running config was loaded from) |
| 16 | u32 | payload CRC of the running config (0 if none) |
| 20 | u32 | `build_id` of the running config (0 if none) |
| 24 | u32 | capabilities: bit0 save, bit1 read suggested setup |
| 28 | u16 | max frame payload (52) |
| 30 | u8 | valid slots: bit0 A, bit1 B (valid + committed + compatible; empty and corrupted both 0) |
| 31 | u8 | reserved |
| 32 | u8×12 | MCU 96-bit unique ID (the USB serial is `KBE2B6-` + these bytes in hex) |
| 44 | u32 | slot `seq` of the running config (+1 per save; 0 if none) |
| 48 | u16 | LED journal: records written in the current page (page erased when full) |
| 50 | u16 | LED journal: records per page (256); 0 = LED state/brightness memory off |

---

## 7) Suggested setup (built-in template)

`READ_CONFIG` selector 1 returns a ready-made CRS built into the firmware (`config/defaults.rs`, compile-time checked against the validator; protocol name `FACTORY_DEFAULTS`). The configurator calls it the **suggested setup**: a starting point loaded into the form (automatically when the device has no configuration, or via **Load suggested setup**) that takes effect only once the user saves it.

It is deliberately **not** a fallback: a device without a valid saved configuration stays in DisabledFallback (inputs disabled), so it never sends actions the user did not choose: silent controls expose a missing/corrupted config, an unexpected mapping would hide it (rationale: [ARCHITECTURE §2.3](../ARCHITECTURE.md#23-no-valid-configuration-disabledfallback)). The UI avoids the term "factory defaults" because it suggests the device would revert to them.

---

## 8) CRC

CRC-32/ISO-HDLC (reflected polynomial `0xEDB88320`, init and xorout `0xFFFFFFFF`), the same algorithm used by CRS and the LED journal. Check value: `crc32("123456789") = 0xCBF43926` (asserted at compile time in `crs_format`, checked in `web_config/selftest.html`).

---

## 9) Design decisions (alternatives considered)

| Considered | Implemented | Why |
|---|---|---|
| New `ConfigWireV1` codec | CRS v1 reused | CRS is already the versioned, CRC'd, compat-gated host→device format; one codec to keep in sync |
| Apply to RAM + separate COMMIT / DISCARD / LOAD_DEFAULTS_RUNTIME | One step: WRITE_END validates, saves (A/B) and reboots | One action is easier to use and to explain than apply / commit / discard; a test-apply step was built first and then removed |
| Live runtime apply | Reboot after save | Reuses the proven boot path (pulls, scan rate, PWM, LED restore); no half-applied state |
| WRITE_BEGIN rejects an active transfer | WRITE_BEGIN restarts it | A crashed/reloaded page cannot wedge the device |
| TypeScript + Vite | Plain classic scripts, no build | No toolchain, no server: `index.html` opens directly from disk (ES modules would not load from `file://`) |
