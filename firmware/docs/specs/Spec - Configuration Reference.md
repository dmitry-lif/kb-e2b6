# Spec — Configuration Reference: behaviour and parameters

Status: **normative**.

This device is a **USB input controller**. It translates physical **gestures** from **push buttons** and **rotary encoders** into configured **actions** in a predictable way.  
This document explains how the device behaves and what each configuration setting means. It does not describe the file format or the tools used to create or load the configuration.

---

## 0) Key principles

- A **gesture** is detected (tap/hold/turn), then mapped to a configured **action**.
- When an action is emitted, the device emits a host-visible **press** and later a **release**.
- **Button release behavior is configurable**:
  - **Pulsed mode**: release is emitted automatically after a configured `pulse_ms` press duration.
  - **Tracked-release mode**: release is emitted on **physical button release** (`pulse_ms = 0`).
- If repeat is enabled, holding produces repeated action emissions (rules below).
- All button and encoder timings are configurable. The firmware behavior is deterministic and bounded.

> Note: Encoder rotation steps are **always pulsed** and use a separate `encoder_pulse_ms` setting.

---

## 0.1) Terms

- **Gesture**: what happens physically (tap, hold, turn CW/CCW).
- **Action**: what the host/device should do when a gesture is recognized (volume up, next track, home, LED toggle, etc.).
- **Action emission**: the host-visible “down” and “up” parts of an emitted action (press + release).

---

## 0.2) Action categories

Gestures can map to different categories of actions. Some actions are sent to the host over USB HID. Other actions are internal and affect only the device itself. The list of actions and their HID usages: [Spec - Action Set](Spec%20-%20Action%20Set.md).

### Quick understanding

- **Keyboard actions** behave like pressing keys on a regular keyboard, optionally with modifiers such as Shift/Ctrl/Alt.
- **Consumer controls** behave like media keys (volume, play/pause, track control).
- **Internal actions** affect the device itself (LED controls) and are not sent to the host.

---

### Keyboard actions (with modifiers)

Keyboard actions simulate normal keyboard key presses. They can include modifier keys such as:

- **Shift**
- **Ctrl**
- **Alt**
- **Windows/Command** (system key)

Examples:

- **“A”** (types the letter a)
- **“Shift + A”** (types capital A)
- **“Ctrl + C”** (Copy)
- **“Ctrl + V”** (Paste)
- **“Alt + Tab”** (switch applications)
- **“Windows + D”** (show desktop)
- Single keys: **Enter**, **Escape**, **Backspace**
- Navigation keys: **Up**, **Down**, **Left**, **Right**
- Common UI keys: **Escape** (Back on Android), **Menu** (Keyboard Menu `0x76`); Android *Home* is a consumer control (AC Home `0x0223`), not a keyboard key

Typical gesture→action mappings:

- Button **short** gesture → **Enter** or **Back**
- Button **long** gesture → **Home** or **Menu**
- Encoder **CW/CCW** gestures → **Up/Down** (scroll lists) or **Right/Left** (step through items)

---

### Consumer controls (media-style actions)

Consumer controls are the common “media buttons” found on many keyboards and steering wheel controls. They are designed for audio/video control and are widely supported by PCs and Android devices.

Examples:

- **Volume Up / Volume Down**
- **Mute**
- **Play / Pause**
- **Next Track / Previous Track**
- **Stop**
- **Brightness Up / Brightness Down** *(support depends on the host/app)*

Typical gesture→action mappings:

- Encoder **CW/CCW** gestures → **Volume Up / Volume Down**
- Button **short** gesture → **Play/Pause**
- Button **long** gesture → **Mute** or **Next Track**

---

### Internal actions (LED controls)

Internal actions affect the device itself and are **not sent to the host** as key presses. Internal actions are limited to LED brightness and LED output control.  
The internal actions, their IDs and effects: [Spec - Action Set §3](Spec%20-%20Action%20Set.md#3-internal-actions).

---

## 1) Timing parameters (behavior)

These parameters define how gestures are classified and how action press/release are emitted.

- **`long_press_ms`**: how long a hold gesture must last to be classified as “long”.
- **`pulse_ms`** *(buttons only)*: how release is generated for button-triggered actions:
  - **`pulse_ms > 0`** → release is emitted automatically `pulse_ms` after the press.
  - **`pulse_ms = 0`** → release is emitted on physical button release (tracked-release).
- **`repeat_period_ms`** *(buttons only, optional)*: enables repeat when `> 0` and sets the interval between repeated action emissions.
- **`encoder_pulse_ms`** *(encoder rotation only)*: press duration for each CW/CCW step action (**must be > 0**).

> In tables below, default fields are `*_default` and per-control overrides inherit unless explicitly set.

---

## 2) Configuration tables (parameters + defaults)

## 2.1) Global parameters

### LED related

| Name                         | Values             | Internal Label          | Internal Type / Values             | Recommended Default | Comment                                                                                                               |
| ---------------------------- | ------------------ | ----------------------- | ---------------------------------- | ------------------: | --------------------------------------------------------------------------------------------------------------------- |
| Persist LED ON/OFF state     | Yes / No           | `led_state_persist`     | bool                               |                true | If enabled, last LED ON/OFF runtime state is stored in flash and can be restored on next boot.                        |
| LED ON/OFF on boot           | ON / OFF / RESTORE | `led_state_on_boot`     | enum: `on \| off \| restore`       |             restore | Boot-time LED state selection. `restore` uses the persisted state if `led_state_persist` is true and a valid record exists; otherwise OFF.              |
| Persist LED brightness level | Yes / No           | `led_level_persist`     | bool                               |                true | If enabled, last LED brightness runtime level is stored in flash and can be restored on next boot.                    |
| LED brightness on boot       | 1..100 / RESTORE   | `led_level_on_boot`     | u8: `1..100`, sentinel `0=restore` |                   0 | Boot-time brightness selection. `0` restores the saved brightness if `led_level_persist` is true and a valid record exists; otherwise `led_level_default`.               |
| LED default brightness       | 1..100             | `led_level_default`     | u8: `1..100`                       |                  30 | Used only when boot mode cannot restore a prior level (first boot / no saved state).                                  |
| Brightness step              | 1..50              | `led_level_step`        | u8: `1..50`                        |                   5 | Step used by internal “brightness up/down” actions.                                                                   |
| Brightness min               | 1..100             | `led_level_min`         | u8: `1..100`                       |                  10 | Clamp floor for brightness. Must be ≤ `led_level_max`.                                                                |
| Brightness max               | 1..100             | `led_level_max`         | u8: `1..100`                       |                 100 | Clamp ceiling for brightness. Must be ≥ `led_level_min`.                                                              |
| Auto-save delay              | 0..60000 ms        | `led_autosave_delay_ms` | u16 ms                             |                1500 | Debounce flash commits for runtime LED changes. Timer resets on each change. `0` saves immediately (not recommended). |

---

### Scan, electrical and timing defaults

| Name                                | Values                     | Internal Label             | Internal Type / Values     | Recommended Default | Comment                                                                                                  |
| ----------------------------------- | -------------------------- | -------------------------- | -------------------------- | ------------------: | -------------------------------------------------------------------------------------------------------- |
| Scan rate                           | 1000 / 2000 / 3000 Hz      | `scan_hz`                  | enum u16                   |                1000 | Deterministic polling frequency. Higher rates increase CPU load and may raise system jitter.             |
| Default pin pull                    | Pull-Up / Pull-Down / None | `pin_pull_default`         | enum: `up \| down \| none` |                  up | Default input bias for buttons/encoders. Pull-up is robust in noisy environments with active-low wiring. |
| Default active level                | Active-Low / Active-High   | `active_low_default`       | bool                       |                true | Electrical default for interpreting inputs. Typically used with pull-up wiring (active-low).             |
| Debounce time                       | 1..50 ms                   | `debounce_ms_default`      | u16 ms                     |                   5 | Debounce window for mechanical buttons (and encoder push buttons).                                       |
| Long press threshold                | 200..3000 ms               | `long_press_ms_default`    | u16 ms                     |                 600 | Used by buttons that have a `long_action` (unless overridden) and always by the on-board button ([§3.3](#33-on-board-button-fixed-actions)).                                               |
| Button pulse duration default       | 0..500 ms                  | `pulse_ms_button_default`  | u16 ms; `0=tracked`        |                  50 | Default for buttons: `>0` = pulsed mode, `0` = tracked-release.                                          |
| Repeat period default               | Off / 20..2000 ms          | `repeat_period_ms_default` | u16 ms; `0=off`            |                   0 | Applied by `crs-pack` to TOML buttons that omit `repeat_period_ms`; the firmware reads only each button's own value, and the configurator does not show this field.              |
| Encoder step pulse duration default | 1..200 ms                  | `encoder_pulse_ms_default` | u16 ms (must be > 0)       |                  30 | Default press duration for encoder CW/CCW step actions. Encoder rotation is always pulsed.               |

---

### Encoder decoding defaults (rotation)

| Name     | Values    | Internal Label | Internal Type / Values | Recommended Default | Comment |
|---|---|---|---|---:|---|
| Swap A/B default | Yes / No | `encoder_swap_ab_default` | bool | false | Harness correction default. Per-encoder override is supported. |
| Detent guard default | 0..10 ms | `encoder_detent_guard_ms_default` | u8 ms | 2 | Bounce protection / stability guard for quadrature decoding. Helps avoid double-steps. |

---

## 2.2) Per-control parameters

Per-control configuration provides:
- a stable **ID** for routing/diagnostics/UI, and
- an **enabled** flag (TOML). `crs-pack` writes a disabled entry as all zeros, so its mappings are not stored on the device; the configurator treats a control as enabled when it has at least one action.

### Encoder (rotation)

| Name                | Values              | Internal Label     | Internal Type / Values       | Recommended Default | Comment                                                                                         |
| ------------------- | ------------------- | ------------------ | ---------------------------- | ------------------: | ----------------------------------------------------------------------------------------------- |
| Encoder ID          | 1..2                | `id`               | u8                           |                   — | 1-based, matches PCB `ENC 1`/`ENC 2`. Must be unique among encoders.                              |
| Enabled             | Yes / No            | `enabled`          | bool                         |                true | If false, encoder rotation gestures are ignored (CW/CCW actions never emitted).                 |
| CW action           | configured / none   | `cw_action`        | ActionRef/Code               |                   — | Action emitted for each CW step gesture.                                                        |
| CCW action          | configured / none   | `ccw_action`       | ActionRef/Code               |                   — | Action emitted for each CCW step gesture.                                                       |
| Swap A/B            | Default / Yes / No  | `swap_ab`          | inherit/bool                 |             inherit | Overrides global `encoder_swap_ab_default`. Use to fix reversed direction due to wiring.        |
| Step pulse duration | Default / 1..200 ms | `encoder_pulse_ms` | inherit/u16 ms (must be > 0) |             inherit | Overrides `encoder_pulse_ms_default` for this encoder. Each step emits one press→release pulse. |
| Detent guard        | Default / 0..10 ms  | `detent_guard_ms`  | inherit/u8 ms                |             inherit | Overrides global `encoder_detent_guard_ms_default`. Guard time for stability/bounce filtering.  |

> If the encoder has a push button, it is configured as a **button** entry and follows the button rules below.
>
> Encoder A/B lines always use the global `pin_pull_default` (there is no per-encoder pull override) and are sampled as raw levels (`active_low` does not apply to A/B; use `swap_ab` to fix direction).

---

### Button (including encoder push button)

| Name                          | Values                      | Internal Label     | Internal Type / Values           | Recommended Default | Comment                                                                                                                                                                                             |
| ----------------------------- | --------------------------- | ------------------ | -------------------------------- | ------------------: | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Button kind                   | Panel / Encoder             | `kind`             | enum: `panel \| encoder`        |                   — | Distinguishes the physical source: panel buttons use IDs `1..6`, encoder push buttons use IDs `1..2` (matching encoder ID). IDs must be unique **within** each kind.                               |
| Button ID                     | 1..6 / 1..2                 | `id`               | u8                               |                   — | 1-based, matches the port names `BTN 1`…`BTN 6`, `ENC 1`/`ENC 2` (bottom silkscreen; the top shows B1–B6, E1–E2). Must be unique within its `kind`.                                                                                                 |
| Enabled                       | Yes / No                    | `enabled`          | bool                             |                true | If false, button gestures are ignored (no short/long/repeat emission).                                                                                                                              |
| Short action                  | configured / none           | `short_action`     | ActionRef/Code                   |                   — | Action mapped to the **short** gesture. Selection rules are defined in [§3.1](#31-buttons).                                                                                                                 |
| Long action                   | configured / none           | `long_action`      | ActionRef/Code                   |                   — | Action mapped to the **long** gesture. Selection rules are defined in [§3.1](#31-buttons).                                                                                                                  |
| Long press threshold          | Default / 200..3000 ms      | `long_press_ms`    | inherit/u16 ms                   |             inherit | Per-button override. Relevant when `long_action` is configured.                                                                                                                                     |
| Pulse duration / release mode | Default / 0..500 ms         | `pulse_ms`         | inherit/u16 ms; `0=tracked`      |             inherit | `>0`: pulsed mode (auto-release after `pulse_ms`). `0`: tracked-release on physical release.                                                                                                        |
| Repeat period                 | Off / Default / 20..2000 ms | `repeat_period_ms` | `inherit` \| `off` \| `20..2000` |             inherit | When enabled, emits repeated action while held. If `long_action` exists, repeat applies to `long_action`; otherwise applies to `short_action`. Interval between repeats is the configured ms value. |
| Debounce override | Default / 1..50 ms | `debounce_ms` | inherit/u16 ms | inherit | Overrides global `debounce_ms_default`. |
| Active level override          | Default / Low / High        | `active_low`       | inherit/bool                     |             inherit | Overrides global `active_low_default`. Use only for non-standard wiring.                                                                                                                            |
| Pull override                 | Default / Up / Down / None  | `pin_pull`         | inherit/enum                     |             inherit | Overrides global `pin_pull_default`. `none` requires external resistors; use cautiously in noisy installs.                                                                                          |

> TOML note: encoder rotation is configured under `[[encoders]]`, while encoder push buttons are configured under `[[buttons]]` with `kind = "encoder"`.

---

## 3) Behavior by control type

## 3.1 Buttons

### 3.1.1 Short vs long gesture selection

For one physical press cycle (**DOWN → UP**), exactly one **gesture** is selected:

- **SHORT gesture**: released before `long_press_ms`  
  → emits the action mapped as **short_action** *(if assigned)*
- **LONG gesture**: held until `long_press_ms`  
  → emits the action mapped as **long_action** *(if assigned)*

**Never both.** If the selected gesture has no action assigned, nothing is emitted. This selection applies only when a `long_action` is configured; a button with only a `short_action` emits it on press, however long it is held ([§3.1.3](#313-short-and-long-both-configured)).

---

### 3.1.2 Press/Release emission modes (`pulse_ms`)

Once a gesture is selected and its mapped action is **pressed**, the corresponding **release** is emitted according to `pulse_ms`:

- **Pulsed mode (`pulse_ms > 0`)**  
  Emit press → wait `pulse_ms` → emit release automatically.
- **Tracked-release mode (`pulse_ms = 0`)**  
  Emit press → emit release on physical button release.

---

### 3.1.3 Short and long both configured

If a button has **both** a short-mapped action and a long-mapped action, the device must choose **only one** per press.

- If the button is **released before** `long_press_ms`, the **short gesture** is selected and the **short_action** is emitted.
- If the button remains **pressed until** `long_press_ms`, the **long gesture** is selected and the **long_action** is emitted.

In this **short+long** configuration only, the device delays emitting the short action until it can determine whether the gesture becomes long. With `pulse_ms = 0`, that delayed short action is sent as press + release at the physical release.  
If **no long action** is configured, there is nothing to wait for, so the **short action is emitted immediately** after debounce.

---

### 3.1.4 Repeat (buttons only)

Repeat is controlled by `repeat_period_ms`.

#### Enable rule

- `repeat_period_ms = 0` disables repeat.
- `repeat_period_ms > 0` enables repeat and defines the interval between consecutive repeated action emissions while the button remains held.

#### Applicability rule

When repeat is enabled:

1. If the button has a **long action** configured → repeat applies to **long_action**.
2. Otherwise, if the button has a **short action** configured → repeat applies to **short_action**.
3. Otherwise → repeat produces no output.

#### Timing rule

- The **first emission** follows the normal button behavior (short/long gesture selection) and is not affected by repeat.
- After that, while the button remains held, the repeated action is emitted again every `repeat_period_ms`.
- Repeating stops immediately on physical release.

#### Release mode dependency

- Repeat emits a stream of discrete **press → release** pulses.
- Each pulse uses `pulse_ms` as the press duration (release is emitted `pulse_ms` after press).
- Therefore, repeat depends on `pulse_ms > 0`. Tracked-release mode (`pulse_ms = 0`) is not compatible with repeat because repeat produces multiple emissions during a single physical hold.

---

## 3.2 Rotary encoders

Encoder rotation is step-based. Each step gesture maps directly to its configured action:

- **CW step gesture** → `cw_action` *(if assigned)*
- **CCW step gesture** → `ccw_action` *(if assigned)*

### Encoder press/release semantics

Encoder steps are naturally **pulse-like** gestures.

- Each CW/CCW step emits exactly one **press → release** pulse.
- The press duration of each step is `encoder_pulse_ms` (**must be > 0**).

If the encoder has a push button, it follows the **button** rules above (including tracked release when `pulse_ms = 0`).

---

## 3.3 On-board button (fixed actions)

The PCB carries one additional push button (silkscreen *LED ON/OFF / BRIGHTNESS*, MCU pin `PA0`) for local LED control. Its actions are fixed and cannot be remapped; its timing and brightness range come from the global settings:

- Always enabled; wired active-low with the internal pull-up.
- **Short** gesture → `LED_TOGGLE`; **long** gesture → `LED_SWING` (brightness step up, wrapping `max → min`).
- While held after the long gesture, `LED_SWING` repeats every **100 ms** (only when `pulse_ms_button_default > 0`, per the repeat rules above).
- Timing uses the global defaults: `debounce_ms_default`, `long_press_ms_default`, `pulse_ms_button_default`.
- In DisabledFallback (no valid configuration) LED actions are ignored, so the button has no effect.

