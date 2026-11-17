# Spec — CRS Format (v1): TOML schema and CRS binary format

Status: **normative**.

This document is a **brief but sufficient** reference for the configuration shape supported by host tooling and consumed by firmware. **CRS** is this project's name for the fixed-size binary configuration blob (288 bytes); it is unrelated to the STM32 clock recovery system (CRS) mentioned in the hardware notes.

---

## 1) Big picture

Authoring flow:

1) Configuration is authored in **TOML** (human-editable).
2) A host tool validates TOML and packs it into a **CRS binary** (the fixed-size, 288-byte configuration format).
3) Firmware loads CRS from flash at boot.

The web configurator builds the same CRS blob in the browser (no TOML); the firmware validates it on save ([Spec - WebHID Config Protocol](Spec%20-%20WebHID%20Config%20Protocol.md)).

Why TOML then CRS?
- TOML is easy to author and review.
- CRS is fixed-layout, bounded, and cheap to decode in firmware.

---

## 2) Frozen limits

These limits are compile-time in firmware and define the payload layout:

- `MAX_PANEL_BUTTONS = 6` (TOML IDs `1..6`)
- `MAX_ENCODERS = 2` (TOML IDs `1..2`)
- `MAX_ENCODER_BUTTONS = 2` (TOML IDs `1..2`) — one "push button" per encoder
- Total configurable buttons: **8** (2 encoder push buttons + 6 panel buttons); the on-board button is not configurable

**ID numbering:** TOML IDs are **1-based** and match the port names on the PCB's bottom silkscreen (`BTN 1` … `BTN 6`, `ENC 1`, `ENC 2`). CRS array indices ([§5.3](#53-payload-layout-crspayloadv1-260-bytes)) are 0-based.

Internal storage order (CRS buttons array): encoder-buttons first, then panel buttons.

Any entry not listed in TOML is treated as **disabled**.

---

## 3) Versioning and compatibility gates

TOML includes two “version” concepts:

```toml
[toml]
version = 1

[target]
fw_compat_id = "0x20260216"
crs_schema_version = 1
```

- `[toml].version` — TOML grammar/shape version (tool-only).
- `[target].crs_schema_version` — CRS binary layout version (tool + firmware).
- `[target].fw_compat_id` — firmware compatibility gate.

> `fw_compat_id` must match the firmware gate (`FW_COMPAT_ID` in `kb_firmware/src/config/fw.rs`).

Rules:
- A config that doesn't match the running firmware (`fw_compat_id`, `crs_schema_version`) is rejected as a whole, never decoded "best effort".
- A change that breaks compatibility needs a deliberate bump of `FW_COMPAT_ID` and/or the CRS schema version.

---

## 4) TOML schema

Behaviour and terms (pulsed vs tracked release, repeat, detent guard): [Spec - Configuration Reference](Spec%20-%20Configuration%20Reference.md). This section gives only the TOML shape.

### 4.0 File organisation

A configuration file has five parts. TOML doesn't enforce an order; the examples keep this one:

| Part | Required | Content |
|---|---|---|
| `[toml]` | yes | `version = 1`: the file format version ([§3](#3-versioning-and-compatibility-gates)) |
| `[target]` | yes | `fw_compat_id`, `crs_schema_version`: the firmware the file is for ([§3](#3-versioning-and-compatibility-gates)) |
| `[global]` | yes, all 19 keys | Defaults for every control (`*_default`) and the backlight settings ([§4.1](#41-globals)) |
| `[[buttons]]` | no; one entry per push button | `kind = "panel"` with `id` 1–6, or `kind = "encoder"` with `id` 1–2 for an encoder's push button ([§4.3](#43-buttons-panel-buttons--encoder-push-buttons)) |
| `[[encoders]]` | no; one entry per encoder | The rotation of encoder `id` 1–2 ([§4.4](#44-encoders-rotation-only)) |

Principles:
- **One entry per physical control**, numbered like the PCB silkscreen (`BTN 1`…`BTN 6`, `ENC 1`/`ENC 2`; [§2](#2-frozen-limits)). Each `id` appears at most once per kind. An encoder is two entries: its rotation in `[[encoders]]`, its push button in `[[buttons]]` with `kind = "encoder"`.
- **No entry = disabled.** An entry with `enabled = true` needs at least one action ([§6](#6-tool-validation-expectations-non-exhaustive)).
- **Defaults and overrides:** a per-control timing or electrical field is optional; left out, the control uses the matching `[global]` default:

  | Per-control field | `[global]` default |
  |---|---|
  | `pin_pull`, `active_low` (buttons) | `pin_pull_default`, `active_low_default` |
  | `debounce_ms`, `long_press_ms` (buttons) | `debounce_ms_default`, `long_press_ms_default` |
  | `pulse_ms`, `repeat_period_ms` (buttons) | `pulse_ms_button_default`, `repeat_period_ms_default` |
  | `swap_ab`, `detent_guard_ms`, `encoder_pulse_ms` (encoders) | `encoder_swap_ab_default`, `encoder_detent_guard_ms_default`, `encoder_pulse_ms_default` |

  The encoder A/B lines always use the global pull (`pin_pull_default`); `active_low` doesn't apply to them.
- **Actions are objects**, never strings: exactly one of `none`, `kbd`, `cc`, `internal` ([§4.2](#42-actions-exactly-one)). `short_action`, `cw_action` and `ccw_action` are required in their entries; `long_action` is optional. What each action type does, and every key with its value: [Spec - Action Set](Spec%20-%20Action%20Set.md).
- **Strict:** an unknown or misspelled key is an error, not ignored; values out of range are rejected ([§6](#6-tool-validation-expectations-non-exhaustive)). Numbers can be written in hex (`0x00E9`) or decimal; `#` starts a comment.

Check a file before flashing it (it prints `ok: <file>` when the file is valid, an error otherwise):

```bash
crs-pack validate --in my.toml
```

Ready-made files: `firmware/tools/*.toml` ([tools README](../../tools/README.md)); a complete annotated file: [§4.5](#45-complete-example).

### 4.1 Globals

```toml
[global]
scan_hz = 1000                      # 1000|2000|3000

pin_pull_default = "up"             # "none" | "up" | "down"
active_low_default = true

debounce_ms_default = 5             # 1..50
long_press_ms_default = 600         # 200..3000

# Buttons: release mode default
pulse_ms_button_default = 50        # 0..500; 0=tracked-release

# Buttons: repeat default (0 disables repeat)
repeat_period_ms_default = 0        # 0 or 20..2000

# Encoder rotation: step pulse duration default (always >0)
encoder_pulse_ms_default = 30       # 1..200

# Encoder decoding defaults
encoder_swap_ab_default = false
encoder_detent_guard_ms_default = 2 # 0..10

# LED (internal actions / device behavior)
led_state_persist = true            # bool
led_state_on_boot = "restore"       # "on" | "off" | "restore"

led_level_persist = true            # bool
led_level_on_boot = 0               # 0=restore, or 1..100
led_level_default = 30              # 1..100

led_level_step = 5                  # 1..50
led_level_min = 10                  # 1..100
led_level_max = 100                 # 1..100
led_autosave_delay_ms = 1500        # 0..60000
```

Notes:
- `repeat_period_ms_default` uses `0` to mean “repeat disabled by default”.
- `pulse_ms_button_default` uses `0` to mean “tracked-release by default”.

---

### 4.2 Actions (exactly one)

Every action value is **exactly one** of the following one-of forms (tool must enforce one-of). What each type does, the usages the configurator offers and explained examples: [Spec - Action Set](Spec%20-%20Action%20Set.md).

- None:
  ```toml
  short_action = { none = {} }
  ```
- Keyboard:
  ```toml
  short_action = { kbd = { mods = 0x00, usage = 0x4F } }
  ```
  `mods` and `usage` must fit in `u8`.
- Consumer Control:
  ```toml
  short_action = { cc = { usage = 0x00E9 } }
  ```
  `usage` must fit in `u16`.
- Internal:
  ```toml
  short_action = { internal = { id = 0x01 } }
  ```
  `id` must fit in `u8`.

Why strict one-of?
- Avoids ambiguous user intent.
- Keeps the firmware decoder simple.

---

### 4.3 Buttons (panel buttons + encoder push buttons)

Buttons are authored as a **single list** with a required `kind` field that distinguishes **panel buttons** from **encoder push buttons**.

- `kind = "panel"` → `id` is in `1..6` (PCB `BTN 1`…`BTN 6`; stored after encoder-buttons in CRS)
- `kind = "encoder"` → `id` is in `1..2` (push button of encoder `ENC 1`/`ENC 2`; stored first in CRS buttons array)

All other fields are identical across both kinds (so encoder push buttons are treated the same as normal buttons in TOML).

#### Minimal examples

Panel button (id 1):
```toml
[[buttons]]
kind = "panel"
id = 1
enabled = true
short_action = { cc = { usage = 0x00CD } }   # Play/Pause
```

Encoder push button for encoder 1:
```toml
[[buttons]]
kind = "encoder"
id = 1
enabled = true
short_action = { cc = { usage = 0x00E2 } }   # Mute
long_action  = { cc = { usage = 0x00E9 } }   # Volume Up
```

#### Optional per-button overrides (inherit unless set)

```toml
[[buttons]]
kind = "panel"
id = 6
enabled = true

# Electrical overrides (optional)
pin_pull = "up"              # "none" | "up" | "down" (inherit if omitted)
active_low = false           # inherit if omitted

# Timing overrides (optional)
debounce_ms = 35             # inherit if omitted
long_press_ms = 600          # inherit if omitted

# Release mode override (optional)
pulse_ms = 50                # 0=tracked-release, >0=pulsed release after ms

# Repeat override (optional)
repeat_period_ms = 70        # 0 disables repeat for this button; >0 enables (requires pulse_ms > 0)

short_action = { cc = { usage = 0x00B5 } }
long_action  = { cc = { usage = 0x00B6 } }
```

Repeat behaviour and its `pulse_ms > 0` dependency: [Spec - Configuration Reference §3.1.4](Spec%20-%20Configuration%20Reference.md#314-repeat-buttons-only).

---

### 4.4 Encoders (rotation only)

Encoders are a list of entries, each with an `id` in `1..2` (PCB `ENC 1`/`ENC 2`). Encoder rotation maps CW/CCW step gestures to actions.

Minimal encoder:
```toml
[[encoders]]
id = 1
enabled = true
cw_action  = { cc = { usage = 0x00E9 } }   # Volume Up
ccw_action = { cc = { usage = 0x00EA } }   # Volume Down
```

Optional per-encoder overrides:
```toml
[[encoders]]
id = 2
enabled = true

swap_ab = true              # inherit if omitted
detent_guard_ms = 2         # inherit if omitted

encoder_pulse_ms = 35       # inherit if omitted; must be > 0

cw_action  = { cc = { usage = 0x00B5 } }
ccw_action = { cc = { usage = 0x00B6 } }
```

Notes:
- Encoder rotation is **always pulsed**; `encoder_pulse_ms` defines the press duration per step.
- Encoder push buttons are not configured here; they are configured in `[[buttons]]` with `kind="encoder"`.

### 4.5 Complete example

One complete file. `crs-pack` requires every `[global]` key. Unlike the built-in template, encoder 2 drives the screen brightness here. More files: `firmware/tools/` (`example.toml`, `example_alt.toml`, `car_config_v1.toml`).

```toml
[toml]
version = 1

[target]
fw_compat_id = "0x20260216"
crs_schema_version = 1

[global]
scan_hz = 1000
pin_pull_default = "up"
active_low_default = true
debounce_ms_default = 5
long_press_ms_default = 600
pulse_ms_button_default = 50
repeat_period_ms_default = 0
encoder_pulse_ms_default = 30
encoder_swap_ab_default = false
encoder_detent_guard_ms_default = 2
led_state_persist = true
led_state_on_boot = "restore"
led_level_persist = true
led_level_on_boot = 0
led_level_default = 30
led_level_step = 5
led_level_min = 10
led_level_max = 100
led_autosave_delay_ms = 1500

# --- Encoders (rotation only) ---
[[encoders]]
id = 1
enabled = true
cw_action  = { cc = { usage = 0x00E9 } }  # Volume Increment
ccw_action = { cc = { usage = 0x00EA } }  # Volume Decrement

[[encoders]]
id = 2
enabled = true
cw_action  = { cc = { usage = 0x006F } }  # Brightness Increment
ccw_action = { cc = { usage = 0x0070 } }  # Brightness Decrement

# --- Encoder push buttons (configured as buttons) ---
[[buttons]]
kind = "encoder"
id = 1
enabled = true
short_action = { cc = { usage = 0x00E2 } } # Mute

[[buttons]]
kind = "encoder"
id = 2
enabled = true
short_action = { kbd = { mods = 0x00, usage = 0x29 } } # Back (Esc)

# --- Panel buttons ---
[[buttons]]
kind = "panel"
id = 1
enabled = true
short_action = { cc = { usage = 0x00CD } } # Play/Pause

[[buttons]]
kind = "panel"
id = 2
enabled = true
short_action = { cc = { usage = 0x00B5 } } # Next Track

[[buttons]]
kind = "panel"
id = 3
enabled = true
short_action = { cc = { usage = 0x00B6 } } # Previous Track

[[buttons]]
kind = "panel"
id = 4
enabled = true
short_action = { kbd = { mods = 0x00, usage = 0x29 } } # Back (Esc)

[[buttons]]
kind = "panel"
id = 5
enabled = true
short_action = { cc = { usage = 0x0223 } } # Home (AC Home)

[[buttons]]
kind = "panel"
id = 6
enabled = true
short_action = { kbd = { mods = 0x00, usage = 0x76 } } # Menu

# Hold-to-repeat variant (repeat while held) — replace any panel entry above, e.g.:
# [[buttons]]
# kind = "panel"
# id = 6
# enabled = true
# pulse_ms = 50
# repeat_period_ms = 70
# short_action = { cc = { usage = 0x00E9 } } # Volume Increment
```

---


## 5) CRS binary schema (v1)

CRS is a **fixed-size** header + **fixed-size** payload. It is designed for `no_std` / `no_alloc` firmware: no runtime parsing on device.

All integer fields are **little-endian** on wire.

### 5.0 Sizes and constants

Fixed sizes (bytes):

- Header (`CrsHeaderV1`): **28**
- Payload (`CrsPayloadV1`): **260**
- Total file length: **288**

Frozen capacities:

- `MAX_PANEL_BUTTONS = 6` (array slots `2..7`)
- `MAX_ENCODER_BUTTONS = 2` (array slots `0..1`)
- `MAX_BUTTONS_TOTAL = 8` (6 panel + 2 encoder-buttons)
- `MAX_ENCODERS = 2` (array slots `0..1`)

### 5.1 File layout (absolute offsets)

| Region | Offset | Size | Notes |
|---|---:|---:|---|
| Header (`CrsHeaderV1`) | 0 | 28 | Validates magic/schema/len/crc/compat |
| Payload (`CrsPayloadV1`) | 28 | 260 | Fixed-layout config data |

Within the payload:

| Payload region | Payload offset | Absolute offset | Size |
|---|---:|---:|---:|
| Globals (`GlobalDefaultsV1`) | 0 | 28 | 28 |
| Buttons array (`ButtonEntryV1[8]`) | 28 | 56 | 192 |
| Encoders array (`EncoderEntryV1[2]`) | 220 | 248 | 40 |

### 5.2 Header layout (`CrsHeaderV1`, 28 bytes)

Header field order (little-endian integers), with sizes:

| Field | Type | Size | Description |
|---|---|---:|---|
| `magic` | `u32` | 4 | ASCII `CRS1` (`0x31535243` LE) |
| `crs_schema_version` | `u16` | 2 | Must be `1` |
| `flags` | `u16` | 2 | Reserved, written as `0` (not checked by the reader) |
| `fw_compat_id` | `u32` | 4 | Firmware compatibility gate |
| `len` | `u16` | 2 | Payload length in bytes (must be `260`) |
| `reserved0` | `u16` | 2 | Written as `0` (not checked by the reader) |
| `crc32` | `u32` | 4 | CRC32 of payload bytes only |
| `build_id` | `u32` | 4 | Tool-defined build identifier |
| `reserved1` | `u32` | 4 | Written as `0` (not checked by the reader) |

Recommended firmware validation order:
1) `magic`
2) `crs_schema_version`
3) `len`
4) `crc32`
5) `fw_compat_id`

### 5.3 Payload layout (`CrsPayloadV1`, 260 bytes)

The payload is a concatenation of:
1) `GlobalDefaultsV1` (28 bytes)
2) `ButtonEntryV1[8]` (8 × 24 bytes)
3) `EncoderEntryV1[2]` (2 × 20 bytes)

#### 5.3.1 Globals (`GlobalDefaultsV1`, 28 bytes)

Field order (little-endian integers), with sizes:

| Field                             | Type      | Size | Meaning                                 |
| --------------------------------- | --------- | ---: | --------------------------------------- |
| `scan_hz`                         | `u16`     |    2 | Scan/poll rate                          |
| `pin_pull_default`                | `u8`      |    1 | Default pull enum                       |
| `active_low_default`              | `u8`      |    1 | Default active level (0/1)              |
| `debounce_ms_default`             | `u16`     |    2 | Default debounce                        |
| `long_press_ms_default`           | `u16`     |    2 | Default long threshold                  |
| `pulse_ms_button_default`         | `u16`     |    2 | Default button `pulse_ms` (0=tracked)   |
| `repeat_period_ms_default`        | `u16`     |    2 | Tool-only default (0=off); not used by firmware |
| `encoder_pulse_ms_default`        | `u16`     |    2 | Tool-only default (>0); not used by firmware |
| `encoder_swap_ab_default`         | `u8`      |    1 | Default swap A/B (0/1)                  |
| `encoder_detent_guard_ms_default` | `u8`      |    1 | Default detent guard (0..10)            |
| `led_state_persist`               | `u8`      |    1 | Persist LED state (0/1)                 |
| `led_state_on_boot`               | `u8`      |    1 | Boot LED state enum                     |
| `led_level_persist`               | `u8`      |    1 | Persist LED level (0/1)                 |
| `led_level_on_boot`               | `u8`      |    1 | 0=restore, else 1..100                  |
| `led_level_default`               | `u8`      |    1 | 1..100                                  |
| `led_level_step`                  | `u8`      |    1 | 1..50                                   |
| `led_level_min`                   | `u8`      |    1 | 1..100                                  |
| `led_level_max`                   | `u8`      |    1 | 1..100                                  |
| `led_autosave_delay_ms`           | `u16`     |    2 | 0..60000                                |
| `reserved`                        | `[u8; 2]` |    2 | Must be 0                               |

Enums (on wire):
- `pin_pull_default`: `0=none`, `1=up`, `2=down`
- `led_state_on_boot`: `0=off`, `1=on`, `2=restore`

#### 5.3.2 Action encoding (`ActionWireV1`, 4 bytes)

Each action is 4 bytes:

| Field | Type | Size | Meaning |
|---|---|---:|---|
| `tag` | `u8` | 1 | 0=None, 1=Kbd, 2=CC, 3=Internal |
| `a` | `u8` | 1 | Tag-specific payload |
| `b` | `u8` | 1 | Tag-specific payload |
| `c` | `u8` | 1 | Reserved, must be 0 |

Tag payload mapping:
- None: `a`/`b` ignored; `c` must be 0 (checked for every tag)
- Kbd: `a = mods (u8)`, `b = usage (u8)`
- CC: `usage (u16) = a + (b << 8)`
- Internal: `a = id (u8)`

#### 5.3.3 Buttons (`ButtonEntryV1`, 24 bytes each; 8 entries)

Buttons are stored as a fixed array of 8 entries:

- indices `0..1`: encoder push buttons (TOML `kind="encoder"`, `id=1..2`)
- indices `2..7`: panel buttons (TOML `kind="panel"`, `id=1..6`)

Mapping from TOML (1-based IDs) to CRS button index (0-based):
- `kind="encoder"`, `id=e (1..2)` → `index = e - 1`
- `kind="panel"`, `id=p (1..6)` → `index = 2 + (p - 1)`

Encoders array: TOML `encoders.id=n (1..2)` → `index = n - 1`.


Field order:

| Field | Type | Size | Meaning |
|---|---|---:|---|
| `enabled` | `u8` | 1 | 0/1 |
| `pull` | `i8` | 1 | Override enum or inherit sentinel (see note) |
| `active_low` | `i8` | 1 | Override bool or inherit sentinel |
| `debounce_ms` | `i16` | 2 | Override or inherit sentinel |
| `long_press_ms` | `i16` | 2 | Override or inherit sentinel |
| `pulse_ms` | `i16` | 2 | Override or inherit sentinel; 0=tracked |
| `repeat_period_ms` | `u16` | 2 | Effective value: 0 = no repeat, or 20..2000; no inherit sentinel (the tool fills an omitted TOML value from the global) |
| `reserved0` | `u8` | 1 | Must be 0 |
| `short_action` | `ActionWireV1` | 4 | Short gesture action |
| `long_action` | `ActionWireV1` | 4 | Long gesture action |
| `reserved1` | `[u8; 2]` | 2 | Must be 0 |
| `reserved2` | `[u8; 2]` | 2 | Must be 0 |

Override/inherit note:
- Signed fields (`i8`/`i16`) use `-1` as the inherit sentinel (the validator rejects other negative values), meaning “inherit global default”.

#### 5.3.4 Encoders (`EncoderEntryV1`, 20 bytes each; 2 entries)

Encoder entries store **rotation** configuration only (CW/CCW). Push buttons are represented in the Buttons array.

Field order:

| Field | Type | Size | Meaning |
|---|---|---:|---|
| `enabled` | `u8` | 1 | 0/1 |
| `swap_ab` | `i8` | 1 | Override bool or inherit sentinel |
| `detent_guard_ms` | `i16` | 2 | Override or inherit sentinel |
| `encoder_pulse_ms` | `u16` | 2 | Effective value 1..200; no inherit sentinel (the tool fills an omitted TOML value from the global) |
| `reserved0` | `u16` | 2 | Must be 0 |
| `cw_action` | `ActionWireV1` | 4 | CW step action |
| `ccw_action` | `ActionWireV1` | 4 | CCW step action |
| `reserved1` | `[u8; 4]` | 4 | Must be 0 |

### 5.4 CRC32 definition

`crc32` is computed over the **payload bytes only** (absolute offsets `28..287`, length 260). It does not include the header. Algorithm: CRC-32/ISO-HDLC (reflected polynomial `0xEDB88320`, init and xorout `0xFFFFFFFF`); check value `crc32("123456789") = 0xCBF43926`.

### 5.5 Canonicalization note

To keep round-trips stable, tools may canonicalize:
- `enabled=false` entries encoded as **all-zero** on wire.
- Fields omitted in TOML may be encoded as inherit sentinels in CRS.
- Dumps may omit disabled entries.

---

## 6) Tool validation expectations (non-exhaustive)

Host tooling is responsible for strict validation, including:
- Range checks for all numeric fields.
- **Enablement rule:** if a control has no gesture→action mapping (i.e., all gesture action fields are effectively “none/unset”), it must be `enabled = false`; the tool rejects `enabled = true` for such a control.
- Uniqueness constraints:
  - `buttons(kind="panel").id` unique in `1..6`
  - `buttons(kind="encoder").id` unique in `1..2`
  - `encoders.id` unique in `1..2`
- Dependency checks:
  - repeat implies a pulsed press duration (`pulse_ms > 0`) for the affected button.
  - `encoder_pulse_ms` must be `> 0` globally and per encoder when overridden.


