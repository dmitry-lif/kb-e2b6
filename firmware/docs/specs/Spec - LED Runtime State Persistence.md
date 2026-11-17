# Spec — LED Runtime State Persistence: backlight on/off and level in flash

Status: **normative**.  
Scope: Firmware-only behavior and flash storage layout for persisting **LED ON/OFF** and **LED brightness level** across power cycles.  
This spec aligns with the project-wide model: **gesture → action → deterministic emission**, `no_std`, bounded memory, and host-controlled configuration.

---

## 1) Summary

The device maintains a runtime LED state:

- `led_on` (ON/OFF)
- `led_level` (brightness level)

When enabled by configuration, the runtime LED state is persisted to flash using a **journaled append-only log** with **delayed commit**.

Key properties:

- **Deterministic**: flash operations run in a dedicated task, never in the scan path; a page erase still stalls the CPU (scan and USB included) for tens of ms (single bank, no read-while-write).
- **Power-loss tolerant**: records are self-validating with CRC; last valid record wins.
- **Flash-friendly**: repeated adjustments are coalesced by `led_autosave_delay_ms`; erase happens only when the log page fills.

---

## 2) Configuration inputs (from CRS)

These are `[global]` fields of the active configuration (the CRS blob, [Spec - CRS Format](Spec%20-%20CRS%20Format.md)); ranges and defaults are in [Spec - Configuration Reference §2.1](Spec%20-%20Configuration%20Reference.md#21-global-parameters):

### Persistence enablement

- `led_state_persist: bool`  
  Enables persisting `led_on` changes.

- `led_level_persist: bool`  
  Enables persisting `led_level` changes.

### Boot policy

- `led_state_on_boot: enum { off, on, restore }`  
  Determines boot-time LED ON/OFF state.
  - `restore` means “use persisted value if available”; otherwise use a safe fallback (OFF).

- `led_level_on_boot: u8`  
  Boot-time LED brightness.
  - `0` means `restore` (use persisted value if available)
  - `1..100` means explicit boot level

- `led_level_default: u8 (1..100)`  
  Used when `led_level_on_boot` requests restore but no persisted value exists.

### Limits and step (already in config)

- `led_level_min`, `led_level_max`: clamp range
- `led_level_step`: brightness step of the internal LED actions (does not affect persistence directly)

### Commit debounce

- `led_autosave_delay_ms: 0..60000`  
  Delay after the **last** LED change before writing to flash.
  - `0` means “commit immediately” (supported but not recommended for flash wear).

---

## 3) Storage strategy

### 3.1 Reserved flash region

Firmware reserves one 4 KiB flash page for LED runtime persistence (256 records of 16 B), directly below Slot A ([Spec - CRS Slots §2](Spec%20-%20CRS%20Slots.md#2-flash-placement-constants)):

- `LED_STATE_FLASH_PAGE_BASE: u32` (platform-specific)
- `LED_STATE_FLASH_PAGE_SIZE: u32` (platform-specific)

Only the firmware writes records here; the SWD config script (`embed_with_cfg.sh`) only wipes the page (all zeros), see [Spec - CRS Slots §8.2](Spec%20-%20CRS%20Slots.md#82-choose-a-target-slot). The region is not part of the CRS blob.

### 3.2 Append-only journal (single-page)

The page contains fixed-size records appended sequentially:

- Write pointer moves forward
- No in-place updates
- When the page is full:
  - erase the page
  - write one “snapshot” record with current state

**Rationale:** simplest robust approach for small state + `no_alloc`.

---

## 4) Record format (wire layout)

### 4.1 Fixed record size and alignment

- Record size: **16 bytes** = two 64-bit double words, aligned to 16 bytes.
- Each double word is programmed exactly once per page erase (STM32G4 rule: no re-programming of a double word without an erase).

### 4.2 Record structure (v1)

All fields are little-endian.

| Field | Type | Size | Description |
|---|---|---:|---|
| `magic` | `u16` | 2 | Constant marker `0x4C45` (bytes `45 4C`, "EL") |
| `version` | `u8` | 1 | Record version, `1` |
| `flags` | `u8` | 1 | Bit0 = `led_on` |
| `level` | `u8` | 1 | `1..100` (stored value) |
| `reserved0` | `u8` | 1 | Must be 0 |
| `reserved1` | `u16` | 2 | Must be 0 |
| `seq` | `u32` | 4 | Monotonic sequence number (wrap allowed) |
| `crc32` | `u32` | 4 | CRC32 over bytes `[magic..seq]` (first 12 bytes) |

**Validity rule:** a record is valid if:
- `magic` matches
- `version == 1`
- CRC32 matches computed CRC over first 12 bytes
- `level` is in `1..100` (else invalid)

> `seq` is used to choose the most recent record when multiple valid records exist.

### 4.3 Erased state

Erased flash bytes are `0xFF`. A record position is considered **empty** if `magic == 0xFFFF`.

---

## 5) Boot behavior

### 5.1 Boot-time scan vs restore

Firmware scans the journal page at boot when persistence is enabled for either field (`led_state_persist` or `led_level_persist`). This establishes:
- whether a valid record exists
- the next write position and next sequence number

Applying a restored value is still policy-driven:
- Restore `led_on` only when `led_state_on_boot == restore` and `led_state_persist == true`
- Restore `led_level` only when `led_level_on_boot == 0` and `led_level_persist == true`

### 5.2 Load latest valid record

When the page is scanned ([§5.1](#51-boot-time-scan-vs-restore)):

1) Scan the page to locate all valid records until the first empty slot.  
2) Choose the newest record by wrapping comparison: `a` is newer than `b` if `a.wrapping_sub(b) < 0x8000_0000` (`is_seq_newer`).

### 5.3 Apply boot policy

- LED ON/OFF:
  - If restore requested and record exists → apply record `led_on`
  - Else apply explicit boot selection:
    - `on` → ON
    - `off` → OFF
    - `restore` with no record → OFF (safe fallback)

- LED brightness:
  - If restore requested and record exists → apply record `level`
  - Else if `led_level_on_boot in 1..100` → apply it
  - Else → apply `led_level_default`

Final level must be clamped to `[led_level_min, led_level_max]`.

---

## 6) Runtime update & commit workflow

### 6.1 RAM state

Firmware maintains:

- `current_led_on: bool`
- `current_led_level: u8`
- `dirty_mask: bitset` (state changed since last persisted commit)
- `commit_deadline: Instant` (next eligible commit time)
- `next_seq: u32` (sequence number for next record)
- `write_index: usize` (next empty slot index in the page)

### 6.2 On state change

Whenever internal actions change LED state or level:

1) Update `current_led_on` / `current_led_level` (with clamp).
2) If persistence for the changed field is enabled:
   - set corresponding bit in `dirty_mask`
   - schedule commit:
     - `commit_deadline = now + led_autosave_delay_ms`
     - if already scheduled, push deadline forward (debounce)

If persistence is disabled for that field, do not set dirty and do not schedule commit.

### 6.3 Commit task (single writer)

A dedicated async task owns flash writes.

Loop:

- If `dirty_mask == 0`: sleep / await
- Else if `now < commit_deadline`: await until deadline
- Else:
  1) Construct `LedStateRecV1` from current state and `next_seq`
  2) Append record to flash at `write_index`
  3) `next_seq += 1`
  4) `dirty_mask = 0`

**Single-writer rule:** on the device, only this task performs flash writes/erase for the LED page.

---

## 7) Append, erase, and wear behavior

### 7.1 Finding the write position

At boot:

- Scan slots sequentially until the first empty slot (`magic==0xFFFF`).
- Set `write_index` to that slot.

If the page has no empty slots, treat as full.

Implementation note:
- If the page contains non-empty bytes but no valid record (CRC/version/magic mismatch), firmware treats the journal as unusable and forces an erase on the first commit. This avoids permanently reducing capacity due to junk flash contents.

### 7.2 Handling full page

If the page is full when a commit is needed:

1) Erase the page
2) Set `write_index = 0`
3) Write exactly one snapshot record representing the current state

This ensures the next boot can restore the latest state even after a page erase.

### 7.3 Power-loss tolerance

- If power is lost during a record write, CRC will fail and firmware will ignore that record at next boot.
- If power is lost after a page erase ([§7.2](#72-handling-full-page)) and before the snapshot record is written, the page is empty at the next boot and the boot policy falls back as if no record existed ([§5.3](#53-apply-boot-policy): `restore` → OFF, level from `led_level_on_boot` or `led_level_default`). The window is the page erase (tens of ms) plus one record write. Erase and append take the flash lock separately, so a WebHID save, or the reboot after it, can fall into that window.
- A record torn mid-program can also read as an uncorrectable flash ECC error; firmware detects it (ECC-checked reads), ignores the record, and erases the page on the next commit (writing a snapshot of the current state).
- Previously written valid records remain usable.
- The flash driver is shared with the WebHID config save (one owner, locked per operation).

### 7.4 Flash failure policy (implementation)

To avoid pathological flash retry loops, firmware caps erase/write retry attempts per operation. After repeated failures, persistence is suspended for the remainder of the boot (runtime LED state still updates in RAM).

---

## 8) Interaction with repeat and gesture behavior

- Persistence is triggered by **state changes** (internal actions), not by gesture phases.
- With repeat-enabled “brightness up/down” actions, many level changes may occur; delayed commit coalesces them into a single flash write after the user stops.

---

## 9) Validation and invariants

Firmware assumptions:

- CRS configuration has been validated by tooling (ranges, dependencies).
- `led_level_min <= led_level_max`
- `led_level_default` and explicit boot level are in 1..100 (tools enforce)

Firmware must still apply clamping for safety.

---

## 10) Implementation notes (non-normative)

- CRC32: same polynomial as CRS tooling (standard CRC32) is acceptable; consistency is more important than specific implementation.
- Record size is fixed to simplify flash alignment and scanning.
- The journal uses one page for simplicity.

---

## 11) Pseudocode

### Boot

1) Load CRS config
2) Determine restore request flags
3) If `led_state_persist` or `led_level_persist` is set → scan page → latest valid record (if any), next write position and `seq`
4) Apply boot policy → set runtime LED state

### Runtime

- On LED change:
  - Update RAM state
  - If persist enabled → dirty + debounce schedule

- Commit task:
  - When deadline reached → append record
  - If page full → erase → write snapshot record
