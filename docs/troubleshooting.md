# Troubleshooting, limitations and safety

The status LED and common symptoms first, then the messages of the [configurator](configuring.md) (the web page you configure the board with), the limits of the design, what I tested, and safety.

- [The status LED](#the-status-led)
- [Symptoms](#symptoms)
- [Configurator messages](#configurator-messages)
- [Known limitations](#known-limitations)
- [What has been tested](#what-has-been-tested)
- [Safety](#safety)
- [Still stuck?](#still-stuck)

---

## The status LED

| Status LED | Meaning |
|---|---|
| Heartbeat: two short flashes every second | The firmware is running with a saved setup |
| Fast blink: about five times a second | The firmware is running, but has no usable setup (none saved yet, or the saved one is damaged or doesn't fit the firmware): the controls do nothing and the backlight stays off. Save a setup with the [configurator](configuring.md) |
| Steady off | No power, or no working firmware |
| Steady on | The firmware isn't running |

## Symptoms

| Symptom | Likely cause | What to do |
|---|---|---|
| Status LED steady on or off, no USB device | No power, or no firmware | Try another cable and port. Still steady: the firmware needs (re)flashing, which needs an SWD probe, see [recovery](../firmware/docs/recovery.md). Controller from the [reference kit](kit.md): [open an issue](https://github.com/dmitry-lif/kb-e2b6/issues) and I'll help |
| Status LED blinks (heartbeat or fast), but the computer doesn't see the board | A charge-only USB cable | Use a USB cable that carries data |
| Status LED blinks fast, controls do nothing | No saved setup: the configurator shows *Config A: None* and *Config B: None* | Save one, e.g. **Load suggested setup** → **Save to device** |
| Controls stopped working after a firmware update, status LED blinks fast | The new firmware doesn't accept setups from the old one | Save a setup again |
| One control does nothing, or does odd things | Cable (on adapter boards: a reversed cable, see [Cables to the adapter boards](wiring.md#cables-to-the-adapter-boards)), port number, or no action set | Unplug USB, check that the red wire is on `LED+` at both ends, check the port number, check the control has an action in the configurator |
| Encoder works backwards | Encoder wiring or type | *Encoders → Encoder N → Rotation → Direction* = *Reversed* |
| Encoder: one action every **two** clicks | The encoder has half as many pulses as detents (e.g. 15 pulses / 30 detents); the firmware sends one action per full pulse | Use an encoder with as many pulses as detents (e.g. 20 / 20) |
| Encoder: one click counts as two | Contact bounce | Raise *Detent guard* (Advanced) |
| Button fires twice | Contact bounce, or noise in a self-made cable | Raise *Defaults → All push buttons → Debounce* (or, with **⚙ Advanced (wiring & timing)** on, that button's *Wiring → Debounce*); keep `GND` and `LED−` separate in self-made cables |
| A key seems stuck on the host | *Press style: Hold while pressed* keeps the key down for as long as the button is held | Pick *Release after … ms* |
| Home / Back / Menu do odd things | Head units interpret these keys differently | Try the alternatives under *Android / system* and *Navigation keys* |
| On-screen keyboard no longer appears on Android | Android sees a physical keyboard | Language and input settings → *Physical keyboard* → *Show on-screen keyboard* (wording varies) |
| LEDs off | LEDs switched off, or no saved setup | Use a control with a backlight action, or short-press the [on-board button](hardware.md#status-led-and-on-board-button) (*LED ON/OFF / BRIGHTNESS*) if you can reach it. With no saved setup the LEDs stay off |
| RGB encoder LED dark, other LEDs fine | No colour jumper bridged on its adapter | Bridge B, G or R ([Hardware notes](hardware.md#rgb-encoder-adapter)) |
| One LED stays at full brightness | Its return is wired to `GND` instead of `LED−` | Fix the cable |
| All LEDs dimmer than expected, uneven or flickering | The LEDs together draw more than the LED supply's current limit (about 135 mA). Lowering the brightness doesn't help: dimming switches the LEDs fully on and off very fast, so the limit applies even when they're dimmed | Fewer lit controls, or less current per control (open the +50% jumpers) |
| The board resets when the LEDs are bright | The USB port can't supply enough current. The board itself needs little (about 30 mA with my 8 controls brightly lit), so suspect the port or the cable first | Lower *Controller LEDs → Brightness control → Maximum*, or use a stronger USB port |

## Configurator messages

| Message or situation | Meaning | What to do |
|---|---|---|
| "This browser cannot talk to USB devices" | The browser doesn't support WebHID, the USB access the page needs | Open the page in Google Chrome, Microsoft Edge, Brave or Opera on a computer. Firefox, Safari and phone browsers can't do it |
| Linux: the board is in the browser's list, but connecting fails (*Connection failed: …*) | Linux doesn't give the browser permission to open the board: the udev rule is missing | Install the udev rule, a one-time permission for the browser (`web_config/linux/install-udev-rule.sh`, see [Open the page](../web_config/README.md#open-the-page)), unplug and re-plug the board, then connect again. Still failing: [Linux: still can't connect?](#linux-still-cant-connect) |
| The device list is empty | The browser can't see the board | On Linux, check the udev rule first (see the row above). Check for a charge-only cable |
| "Connection failed: … timeout" | Another tab or program has the board open | Close the other configurator tab |
| "Not saved: Config validation failed: … The device keeps its previous settings." | The setup breaks a rule (the message names the control) | Fix that control and save again. Nothing changed on the board |
| "Lost contact while saving" | The cable was pulled or the browser was interrupted during the save | Re-plug the board, click **Connect device** and save again |
| "Several KB-E2B6 boards found" | More than one board is plugged in | Click **Connect device** and pick one |
| The device list appears every time you click **Connect device** | Normal: the page always opens it. A board you've allowed before is marked *Paired* | Pick **KB-E2B6** and click *Connect* |

### Linux: still can't connect?

If the configurator keeps saying *Connection failed: Failed to open the device.*, Linux isn't letting the browser open the board. The browser's device log (`brave://device-log` or `chrome://device-log`) then shows `FILE_ERROR_ACCESS_DENIED` for a `/dev/hidrawN` node. Check who may open the board's nodes:

```bash
for d in /sys/class/hidraw/hidraw*; do grep -qi '0000CAFE:00004002' "$d/device/uevent" && getfacl -p "/dev/$(basename "$d")"; done
```

Each node should list your user (`user:<your user>:rw-`). If it doesn't, install the udev rule (row above), then unplug and re-plug the board.

To get going right away, give your user access by hand. It lasts until the board is unplugged or restarts after a save:

```bash
for d in /sys/class/hidraw/hidraw*; do grep -qi '0000CAFE:00004002' "$d/device/uevent" && sudo setfacl -m "u:$USER:rw" "/dev/$(basename "$d")"; done
```

## Known limitations

By design:

- **2 encoders and 6 buttons**, plus the on-board button (backlight on/off and brightness).
- **One brightness for all LEDs.** How bright each control is relative to the others is set by hardware (adapter jumpers or your own resistors), its colour by the LED or the RGB jumpers.
- **The backlight doesn't follow the car's lights.** There's no input for the headlight switch or the dash dimmer; you set the brightness with a control.
- **All LEDs share one current-limited supply** (TPS2553, about 135 mA). Keep the total of all LEDs under 100 mA ([LED current budget](hardware.md#led-current-budget)).
- **A save restarts the board** (a couple of seconds) and the controls pause during that time. Setups are applied only at startup, never live.
- **Encoders need one pulse per detent.** Encoders with half as many pulses as detents give one action every second click.
- **Configuring needs Google Chrome, Microsoft Edge, Brave or Opera on a computer** (other Chromium-based browsers should work too). Firefox, Safari and browsers on phones, tablets and head units can't do it.
- **Not a "boot" keyboard.** The keyboard shares its USB interface with the media keys, so BIOS/UEFI setup screens and some KVM switches, which only understand the simplest keyboard type, usually won't see it. Operating systems (Windows, Linux, macOS, Android) do.
- **Firmware updates need an SWD probe.** There's no USB firmware update, and the SWD header has no reset pin.
- **USB IDs `CAFE:4002` are test IDs**, not officially assigned. Harmless, unless you own another hobby gadget that uses the same pair.

## What has been tested

I tested the project on **Ubuntu 24.04 Linux** (the configurator in Brave), and the controller runs in my car with a Mekede DUDU 4 head unit (Android 10). Windows and macOS are not tested.

## Safety

> [!WARNING]
> **USB power only (5 V).** Never connect the controller directly to the car's 12 V. Use the head unit's own USB port or a branded car USB charger.

- This is a **DIY, open-source electronics project**, built in small numbers. It is not certified automotive equipment. Install it so it can't interfere with the car's controls, the airbags or your view.
- Unplug USB before plugging or unplugging controls. Never force a connector.
- Secure the board and the cables so nothing can short against metal or rattle loose.
- **Temperature:** the connectors are rated −25…+85 °C, the MCU −40…+85 °C. The top of a dashboard in the summer sun can get hotter than 85 °C, so mount the controller behind or under the dash, out of direct sun.

## Still stuck?

[Open an issue](https://github.com/dmitry-lif/kb-e2b6/issues) on the project's GitHub page with: the firmware version and serial number (both shown in the configurator), your operating system and browser, what you expected and what happened, and a photo of the wiring.
