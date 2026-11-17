# Spec — Action Set: keys, media keys and backlight actions

Status: **normative**. Defines what a gesture can do: the action types the firmware sends, the list the web configurator offers (`web_config/js/catalog.js` follows this spec), the internal backlight actions and the built-in template's mapping.

How gestures are detected and timed: [Spec - Configuration Reference §3](Spec%20-%20Configuration%20Reference.md#3-behavior-by-control-type). TOML syntax: [Spec - CRS Format §4.2](Spec%20-%20CRS%20Format.md#42-actions-exactly-one); byte encoding: [§5.3.2](Spec%20-%20CRS%20Format.md#532-action-encoding-actionwirev1-4-bytes).

The actions are standard USB HID usages, picked with Android head units in mind. PCs, phones and tablets understand the same usages, but how a host reacts to a given usage varies ([§5](#5-host-notes)).

---

## 1) Action types

Each gesture (short press, long press, one encoder step CW or CCW) carries exactly one action:

| Type | TOML | Values | Goes to |
|---|---|---|---|
| Keyboard | `{ kbd = { mods = 0x00, usage = 0x28 } }` | `mods`: HID modifier bits (`u8`: bit 0 Ctrl, 1 Shift, 2 Alt, 3 GUI/Win; bits 4–7 the right-hand ones). `usage`: Keyboard/Keypad page usage (`u8`) | Host (keyboard report) |
| Consumer control | `{ cc = { usage = 0x00E9 } }` | Consumer page (`0x0C`) usage (`u16`): a literal usage ID, not a bit mask | Host (consumer report) |
| Internal | `{ internal = { id = 0x01 } }` | Internal action ID (`u8`, [§3](#3-internal-actions)) | The board itself (backlight) |
| None | `{ none = {} }` | — | Nothing |

The firmware sends any keyboard or consumer usage it is given. The configurator offers the list in [§2](#2-configurator-list); a usage outside it can be set in TOML (`crs-pack`), and the configurator then shows it as "(current)".

### 1.1 Actions in TOML

How a configuration file is organised (header, `[global]` defaults, one `[[buttons]]` entry per push button, one `[[encoders]]` entry per encoder): [Spec - CRS Format §4.0](Spec%20-%20CRS%20Format.md#40-file-organisation). The actions sit in these fields:

| Entry | Field | Gesture |
|---|---|---|
| `[[buttons]]` | `short_action` | Short press |
| `[[buttons]]` | `long_action` | Long press (held for `long_press_ms`) |
| `[[encoders]]` | `cw_action`, `ccw_action` | One detent clockwise / counter-clockwise |

Every action value is a structured TOML object, never a string such as `"VOL_UP"`: the firmware only gets the resolved numbers. A gesture without an action: `long_action` can be left out; `short_action`, `cw_action` and `ccw_action` are required in every entry, so write `{ none = {} }` there. All fields: [Spec - CRS Format §4](Spec%20-%20CRS%20Format.md#4-toml-schema), a complete file in [§4.5](Spec%20-%20CRS%20Format.md#45-complete-example); ready-made files: `firmware/tools/*.toml`.

Examples:

```toml
# Panel button 1: Play/Pause on a short press, backlight on/off on a long press
[[buttons]]
kind = "panel"
id = 1
enabled = true
short_action = { cc = { usage = 0x00CD } }        # consumer control: Play / Pause
long_action  = { internal = { id = 0x01 } }       # internal: LED_TOGGLE (backlight)

# Panel button 2: Ctrl+C (modifier bit 0x01 = Ctrl, keyboard usage 0x06 = C)
[[buttons]]
kind = "panel"
id = 2
enabled = true
short_action = { kbd = { mods = 0x01, usage = 0x06 } }

# Panel button 3: hold to keep turning the volume up (repeat every 70 ms; repeat needs pulse_ms > 0)
[[buttons]]
kind = "panel"
id = 3
enabled = true
pulse_ms = 50
repeat_period_ms = 70
short_action = { cc = { usage = 0x00E9 } }        # Volume up

# Encoder 2: list navigation with the arrow keys, its push button = Enter
[[encoders]]
id = 2
enabled = true
cw_action  = { kbd = { mods = 0x00, usage = 0x51 } }   # Arrow Down
ccw_action = { kbd = { mods = 0x00, usage = 0x52 } }   # Arrow Up

[[buttons]]
kind = "encoder"
id = 2
enabled = true
short_action = { kbd = { mods = 0x00, usage = 0x28 } } # Enter
```

How short, long and repeat interact (which gesture fires when, press styles, repeat rules): [Spec - Configuration Reference §3](Spec%20-%20Configuration%20Reference.md#3-behavior-by-control-type).

---

## 2) Configurator list

The list aims at a minimal but complete control surface for an Android head unit: navigation (D-pad), media and volume, system keys (Home, Back, Menu), screen brightness and the voice assistant. Function keys, letters and digits are the escape hatch for PCs and for remapping on the host.

Groups and labels as in the configurator. *TOML* is the value to write in a file ([§1.1](#11-actions-in-toml)). *HID usage name* is the usage's name in the USB HID Usage Tables, useful when a host's documentation names its keys.

**Media** (consumer control):

| Configurator | TOML | HID usage name |
|---|---|---|
| Play / Pause | `{ cc = { usage = 0x00CD } }` | Play/Pause |
| Next track | `{ cc = { usage = 0x00B5 } }` | Scan Next Track |
| Previous track | `{ cc = { usage = 0x00B6 } }` | Scan Previous Track |
| Stop | `{ cc = { usage = 0x00B7 } }` | Stop |
| Fast forward | `{ cc = { usage = 0x00B3 } }` | Fast Forward |
| Rewind | `{ cc = { usage = 0x00B4 } }` | Rewind |
| Volume up | `{ cc = { usage = 0x00E9 } }` | Volume Increment |
| Volume down | `{ cc = { usage = 0x00EA } }` | Volume Decrement |
| Mute | `{ cc = { usage = 0x00E2 } }` | Mute |

**Android / system** (consumer control):

| Configurator | TOML | HID usage name | Notes |
|---|---|---|---|
| Home | `{ cc = { usage = 0x0223 } }` | AC Home | Android Home; the template's Home ([§5](#5-host-notes)) |
| Back | `{ cc = { usage = 0x0224 } }` | AC Back |  |
| Menu | `{ cc = { usage = 0x0040 } }` | Menu | Linux and Android map it to MENU ([§5](#5-host-notes)) |
| Search | `{ cc = { usage = 0x0221 } }` | AC Search |  |
| Voice assistant | `{ cc = { usage = 0x00CF } }` | Voice Command | a key event only, no microphone needed |
| Open media player | `{ cc = { usage = 0x0183 } }` | AL Consumer Control Configuration |  |

**Screen brightness (head unit)** (consumer control): they change the host's screen, not the controls' backlight ([§3](#3-internal-actions)).

| Configurator | TOML | HID usage name |
|---|---|---|
| Brightness up | `{ cc = { usage = 0x006F } }` | Display Brightness Increment |
| Brightness down | `{ cc = { usage = 0x0070 } }` | Display Brightness Decrement |

**Navigation keys** (keyboard):

| Configurator | TOML | HID usage name | Notes |
|---|---|---|---|
| Arrow Up | `{ kbd = { mods = 0x00, usage = 0x52 } }` | Keyboard UpArrow | Android `KEYCODE_DPAD_UP` |
| Arrow Down | `{ kbd = { mods = 0x00, usage = 0x51 } }` | Keyboard DownArrow | Android `KEYCODE_DPAD_DOWN` |
| Arrow Left | `{ kbd = { mods = 0x00, usage = 0x50 } }` | Keyboard LeftArrow | Android `KEYCODE_DPAD_LEFT` |
| Arrow Right | `{ kbd = { mods = 0x00, usage = 0x4F } }` | Keyboard RightArrow | Android `KEYCODE_DPAD_RIGHT` |
| Enter / OK | `{ kbd = { mods = 0x00, usage = 0x28 } }` | Keyboard Return (ENTER) | confirms like the D-pad centre (`KEYCODE_DPAD_CENTER`) |
| Escape (Back) | `{ kbd = { mods = 0x00, usage = 0x29 } }` | Keyboard ESCAPE | Android treats it as Back; the template's Back ([§5](#5-host-notes)) |
| Backspace | `{ kbd = { mods = 0x00, usage = 0x2A } }` | Keyboard DELETE (Backspace) |  |
| Tab | `{ kbd = { mods = 0x00, usage = 0x2B } }` | Keyboard Tab |  |
| Space | `{ kbd = { mods = 0x00, usage = 0x2C } }` | Keyboard Spacebar |  |
| Page Up | `{ kbd = { mods = 0x00, usage = 0x4B } }` | Keyboard PageUp |  |
| Page Down | `{ kbd = { mods = 0x00, usage = 0x4E } }` | Keyboard PageDown |  |
| Home key | `{ kbd = { mods = 0x00, usage = 0x4A } }` | Keyboard Home | the keyboard's Home key; for the Home screen use *Home* above |
| End key | `{ kbd = { mods = 0x00, usage = 0x4D } }` | Keyboard End |  |
| Menu key | `{ kbd = { mods = 0x00, usage = 0x76 } }` | Keyboard Menu | the template's Menu ([§5](#5-host-notes)) |
| Context menu key | `{ kbd = { mods = 0x00, usage = 0x65 } }` | Keyboard Application |  |

**Function keys, letters, digits** (keyboard; `mods = 0x00` unless a modifier is chosen):

| Configurator | TOML `usage` | HID usage name | Notes |
|---|---|---|---|
| F1–F12 | `0x3A`–`0x45` | Keyboard F1–F12 |  |
| F13 (PROG red), F14 (PROG green), F15 (PROG yellow), F16 (PROG blue) | `0x68`, `0x69`, `0x6A`, `0x6B` | Keyboard F13–F16 | named after Android's `KEYCODE_PROG_RED` / `GREEN` / `YELLOW` / `BLUE` (names only, see below) |
| F17–F24 | `0x6C`–`0x73` | Keyboard F17–F24 |  |
| A–Z | `0x04`–`0x1D` | Keyboard a and A … z and Z | lower case; Shift (`0x02`) for capitals |
| 1–9, 0 | `0x1E`–`0x26`, `0x27` | Keyboard 1 and ! … 0 and ) |  |

**Controller LEDs** (internal): the six backlight actions in [§3](#3-internal-actions).

**Modifiers** go into `mods`: Ctrl `0x01`, Shift `0x02`, Alt `0x04`, Win `0x08`, added together for combinations (Ctrl+Shift = `0x03`); the configurator offers none or any combination of these four (left-hand bits). Example Ctrl+C: `{ kbd = { mods = 0x01, usage = 0x06 } }`. Right-hand modifier bits (`0x10`–`0x80`) read from a board are kept.

F13–F16 are spare keys meant for remapping on the host. The *PROG* labels are only names, after Android's programmable colour keys (`KEYCODE_PROG_*`): the board sends F13–F16, and what they do is up to the host (on Android, e.g. a Key Mapper rule).

---

## 3) Internal actions

| ID | Name | Configurator label | Effect |
|---|---|---|---|
| `0x01` | `LED_TOGGLE` | LED on/off (toggle) | Backlight on/off |
| `0x02` | `LED_SWING` | LED brightness step (wraps) | One `led_level_step` brighter; after `led_level_max` back to `led_level_min` |
| `0x03` | `LED_ON` | LED on | Backlight on |
| `0x04` | `LED_OFF` | LED off | Backlight off |
| `0x05` | `LED_UP` | LED brighter | One step brighter, stops at `led_level_max` |
| `0x06` | `LED_DOWN` | LED dimmer | One step dimmer, stops at `led_level_min` |

They control the backlight (the shared LED PWM output on the `BTN`/`ENC` ports), not the red status LED and not the host's screen brightness (for that, use *Brightness up / down*, [§2](#2-configurator-list)). The on-board button has `0x01` (short press) and `0x02` (long press) built in ([Spec - Configuration Reference §3.3](Spec%20-%20Configuration%20Reference.md#33-on-board-button-fixed-actions)). With no valid configuration they are ignored ([ARCHITECTURE §2.3](../ARCHITECTURE.md#23-no-valid-configuration-disabledfallback)).

---

## 4) Built-in template

The template in `config/defaults.rs` is what the configurator loads into its form as a starting point; it is never applied by itself ([ARCHITECTURE §2.3](../ARCHITECTURE.md#23-no-valid-configuration-disabledfallback)). Short presses only, no long-press actions:

| Control | Action |
|---|---|
| Encoder 1 | CW Volume up, CCW Volume down; push Mute |
| Encoder 2 | CW Arrow Down, CCW Arrow Up; push Enter |
| Panel buttons 1–6 | Play / Pause, Next track, Previous track, Escape (Back), Home (AC Home `0x0223`), Menu key (`0x76`) |

Other common patterns:
- Encoder for volume: rotation Volume up / down, push Mute (encoder 1 of the template).
- Encoder for list navigation: rotation Arrow Down / Arrow Up, push Enter (encoder 2 of the template).
- Encoder for screen brightness: rotation Brightness up / down, push Voice assistant or Back ([CRS Format §4.5](Spec%20-%20CRS%20Format.md#45-complete-example)).
- Buttons for Play / Pause, Next and Previous; hold-to-repeat for volume or brightness ([§1.1](#11-actions-in-toml)).
- If the arrow keys navigate, put Enter on some control too, so a selection can be confirmed (nothing checks this).

---

## 5) Host notes

- **Navigation:** the arrow keys plus Enter and Escape are the most reliable set across head units and PCs. On Android the arrow keys act as the D-pad (`KEYCODE_DPAD_UP` / `DOWN` / `LEFT` / `RIGHT`) and Enter confirms like the D-pad centre.
- **Back:** the template uses Escape (Key `0x29`): Android treats it as Back, and it means "back / cancel" on most PCs. The list also has AC Back (CC `0x0224`).
- **Home:** AC Home (CC `0x0223`), not Consumer Menu (`0x0040`): Linux and Android map `0x0040` to MENU.
- **Menu:** the template uses the keyboard Menu key (`0x76`); the list also has Consumer Menu (`0x0040`).
- **Media keys** (consumer page) are widely supported by PCs and Android.
- **Screen brightness** (`0x006F` / `0x0070`) works only where the host supports these usages. Otherwise pick another usage, or remap a spare key on the host.
- **Phone keys** (answer / end call) have no list entry: set an explicit `kbd` or `cc` usage in TOML if your host supports one. Like the voice assistant key, they are only key events; the board needs no microphone.
- Head units differ, above all for Home, Menu and the assistant key: check on the target and change the usage if needed.
