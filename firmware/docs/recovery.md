# Recovery and first-time provisioning (SWD)

Engineering procedures for a KB-E2B6 that needs its firmware (re)flashed, a new MCU provisioned, or its configuration wiped. Most "it does nothing" cases are **not** recovery cases. See the table first.

---

## 0. Do you need this page?

| Symptom | Most likely state | What to do |
|---|---|---|
| Controls do nothing, status LED blinks fast. Configurator shows *Config A: None* and *Config B: None* | No valid configuration (**DisabledFallback**). The firmware is fine | No SWD needed: open the configurator, **Load suggested setup** (or build your own), **Save to device**. See [Troubleshooting](../../docs/troubleshooting.md#symptoms) |
| Controls stopped working right after a firmware update | The new firmware has a different config compatibility ID and rejected the old config | Same as above: save a new config from the configurator |
| Status LED blinks (heartbeat or fast blink: the firmware is running) but the configurator can't find the device or can't connect to it | Host/browser side (permissions, udev, charge-only cable) | [Troubleshooting, configurator messages](../../docs/troubleshooting.md#configurator-messages) |
| No USB device appears and the status LED is steady on or off | Firmware missing/corrupt, or a hardware fault | [§3](#3-flash-or-reflash-the-firmware), reflash the firmware |
| Flashing fails at `0x08010000` (`The page write of the page at address 0x08010000 failed`) | MCU still in factory **dual-bank** mode | [§2](#2-first-time-provisioning-set-single-bank-mode-dbank0): run `flash_set_single_bank.sh` |
| The probe cannot attach | Firmware hung, or the core is busy | [§4](#4-the-probe-cannot-attach) |
| You want a board with no configuration (factory state) | — | [§5](#5-wipe-restore-or-replace-the-configuration) |

The status LED (MCU pin `PC15`) shows a **heartbeat** (two 60 ms flashes every second) when the firmware runs with a valid config, and a **fast blink** (100 ms on, 100 ms off) in DisabledFallback (no valid config). A steady LED (on or off) means the firmware isn't running. The configurator status strip shows which slot is in use; `cfg status` on the USB serial (CDC) console also shows why a slot was rejected.

---

## 1. What you need

- A SWD probe supported by probe-rs: CMSIS-DAP (e.g. DAPLink), ST-LINK V2/V3 or J-Link. Mine is a WeAct Studio Mini Debugger in DAPLink mode.
- The board's SWD header (2.54 mm, 1×4): `1` 3V3 · `2` SWDIO · `3` SWCLK · `4` GND. `NRST` is not on the header (the MCU has an internal pull-up), so "connect under reset" is not available.
- Power the board from USB while programming. Genuine ST-LINKs have a target-voltage *sense* pin: connect it to `3V3`. Many cheap ST-LINK V2 dongles have a 3.3 V *output* there instead: leave it unconnected and wire only SWDIO, SWCLK and GND.
- [probe-rs](https://probe.rs) **0.32.x**. OpenOCD only for the last-resort recovery ([§2](#2-first-time-provisioning-set-single-bank-mode-dbank0)), STM32CubeProgrammer only for the page erase in [§5](#5-wipe-restore-or-replace-the-configuration).
- The firmware source, built with `cargo embed` ([§3](#3-flash-or-reflash-the-firmware); build setup: [../README.md](../README.md#build)).

---

## 2. First-time provisioning: set single-bank mode (`DBANK=0`)

This section exists because I ran into it myself: a board flashed in the chip's factory flash mode stopped booting, and I had to recover it. Do the mandatory step below and you shouldn't ever need the recovery part.

**Single bank, dual bank.** The STM32G474's 128 KiB of flash can be organised in two ways, and one setting in the chip decides which:

- **Single bank** (`DBANK=0`): one block, `0x08000000`–`0x0801FFFF`, erased in 4 KiB pages.
- **Dual bank** (`DBANK=1`): two 64 KiB blocks, the second one far away at `0x08040000`, erased in 2 KiB pages.

The setting lives in the chip's *option bytes*: a few configuration words stored apart from the normal flash, so erasing or reflashing the firmware doesn't change them. Chips leave the factory in dual-bank mode.

**Why single bank.** The firmware is laid out for it: one image of about 80 KiB from `0x08000000`, and the saved setup and LED journal in the last 12 KiB, up to `0x0801FFFF`. In dual-bank mode everything above `0x08010000` is outside the chip's documented flash. Firmware and configuration flashing support single bank only.

**The bit.** `DBANK` is **bit 22** (mask `0x00400000`) of the option register `FLASH_OPTR` at `0x40022020`:

| | Working board | Factory board |
|---|---|---|
| `FLASH_OPTR` | `0xFFAFF8AA` | `0xFFEFF8AA` |
| `DBANK` (bit 22) | 0 | 1 |
| Flash layout | Single bank | Dual bank |
| Status | Works with this firmware | Factory default |

**What happens in dual-bank mode.** probe-rs can flash a factory board as it is, and the firmware even runs. The trouble starts with the next flash, a configuration or a firmware update: it fails at `0x08010000` (`The page write of the page at address 0x08010000 failed`) and can leave the chip half old, half new, which may not boot. The firmware also can't save a setup in dual-bank mode. That's why the configuration script (`embed_with_cfg.sh`) refuses to run on a dual-bank board.

### The mandatory step

> [!CAUTION]
> **Before you flash anything to a new board, run `flash_set_single_bank.sh` once.** On a dual-bank chip it switches to single bank and erases the whole flash.

```bash
cd firmware/tools
./flash_set_single_bank.sh
```

It reads `FLASH_OPTR` and prints it. Already single bank: nothing changes. Dual bank: it switches to single bank, checks the result and erases the flash, because the old contents don't survive the switch. It only touches the option bytes if the read-protection byte (the low byte of `FLASH_OPTR`) reads `AA`, i.e. no protection.

Then flash whatever you need, in any order: the firmware ([§3](#3-flash-or-reflash-the-firmware)), a configuration ([§5](#5-wipe-restore-or-replace-the-configuration)), or both.

### If the board is already stuck

You flashed it in dual-bank mode after all (I did warn you), and now flashing fails at `0x08010000` or the board doesn't boot:

1. Run the script, then flash the firmware again ([§3](#3-flash-or-reflash-the-firmware)):

   ```bash
   cd firmware/tools
   ./flash_set_single_bank.sh
   ```

2. If that doesn't help (the script can't connect, or `DBANK` stays 1), use OpenOCD, below.

### Last resort: OpenOCD

This is the sequence I used to recover my board, with a DAPLink probe (WeAct Studio Mini Debugger, `interface/cmsis-dap.cfg`); with an ST-LINK, use `interface/stlink.cfg` instead (not tried). It works through the 4-pin SWD header as it is; no `NRST` needed. Step 3 is the checkpoint: don't erase or flash until it shows bit 22 cleared.

1. Check the connection and read `FLASH_OPTR` (offset `0x20` in the flash registers, address `0x40022020`):

```bash
openocd \
  -f interface/cmsis-dap.cfg \
  -f target/stm32g4x.cfg \
  -c "adapter speed 500" \
  -c "init" \
  -c "reset halt" \
  -c "stm32l4x option_read 0 0x20" \
  -c "shutdown"
```

A factory chip reads `0xffeff8aa`: bit 22 (`0x00400000`, `DBANK`) is set.

2. Clear `DBANK` only and reload the option bytes:

```bash
openocd \
  -f interface/cmsis-dap.cfg \
  -f target/stm32g4x.cfg \
  -c "adapter speed 500" \
  -c "init" \
  -c "reset halt" \
  -c "stm32l4x option_write 0 0x20 0x00000000 0x00400000" \
  -c "stm32l4x option_load 0" \
  -c "shutdown"
```

`option_write` takes a value (`0x00000000`) and a mask (`0x00400000`), so only bit 22 changes. `option_load` resets the MCU on purpose, so a disconnect or reset message at the end is normal.

3. **Checkpoint:** read `FLASH_OPTR` again:

```bash
probe-rs read --chip STM32G474CBTx b32 0x40022020 1
```

A factory chip now reads `ffaff8aa`: the same value with only bit 22 cleared (`0xFFEFF8AA` − `0x00400000` = `0xFFAFF8AA`). If bit 22 is still set, repeat step 2; don't go on.

4. Mass erase in the new single-bank layout:

```bash
openocd \
  -f interface/cmsis-dap.cfg \
  -f target/stm32g4x.cfg \
  -c "adapter speed 500" \
  -c "init" \
  -c "reset halt" \
  -c "stm32l4x mass_erase 0" \
  -c "shutdown"
```

5. Flash the firmware ([§3](#3-flash-or-reflash-the-firmware)), then a configuration if you like ([§5](#5-wipe-restore-or-replace-the-configuration)).

---

## 3. Flash or reflash the firmware

New chip: run [§2](#2-first-time-provisioning-set-single-bank-mode-dbank0)'s `flash_set_single_bank.sh` first. Then:

```bash
cd firmware/kb_firmware
cargo embed --release
```

---

## 4. The probe cannot attach

1. Unplug USB, reconnect the probe, plug USB back in and retry.
2. "Connect under reset" (`--connect-under-reset`, `KB_CONNECT_UNDER_RESET=1`, CubeProgrammer `mode=UR`) needs `NRST`, which the header doesn't have. Instead: start the connect command, then plug USB in so the probe catches the MCU right after power-up.
3. Last resort: [§2](#2-first-time-provisioning-set-single-bank-mode-dbank0)'s recovery (the script, then OpenOCD), then reflash ([§3](#3-flash-or-reflash-the-firmware)) and configure again.

---

## 5. Wipe, restore or replace the configuration

The configuration lives in two 4 KiB flash slots (A at `0x0801_E000`, B at `0x0801_F000`). The LED journal sits directly below them (`0x0801_D000`). In single-bank mode these are flash **pages 29, 30 and 31**.

**Write a known config** (every run clears both slots and the LED journal first):

```bash
cd firmware/tools
./embed_with_cfg.sh --toml ./car_config_v1.toml
```

**Restore a `.crs` backup exported from the configurator.** Easiest: configurator → **Import .crs** → **Save to device**. Over SWD, turn it back into TOML, then write `restored.toml` as above:

```bash
crs-pack dump --in backup.crs --toml > restored.toml
```

**Back to "no configuration" (DisabledFallback):** erase pages 29–31 (check the page-erase syntax of your CubeProgrammer version):

```bash
STM32_Programmer_CLI -c port=SWD -e 29 30 31
```

**Back to a working setup:** configurator → **Load suggested setup** → **Save to device**, or import your own exported `.crs` and save.

There is intentionally **no** "wipe" command over USB/WebHID. Saving a new config is the only WebHID write, and the old slot stays intact until the new one is verified.

---

## 6. Why a bad configuration shouldn't brick the board

- The device validates every config before saving it (same rules as the host tool).
- Saves go to the *other* A/B slot. The commit marker is written last, and the slot is read back and checked before the device reboots into it. By design, a power loss mid-save leaves the old config active.
- If both slots are invalid, the firmware still boots, enumerates USB and serves the configurator (DisabledFallback). Only the inputs are disabled.
- By design, a flash ECC error (a write torn by power loss) makes the slot read as invalid instead of hanging the MCU.

> [!IMPORTANT]
> The only states that need SWD are a missing or corrupt **firmware**, or an MCU that was never provisioned ([§2](#2-first-time-provisioning-set-single-bank-mode-dbank0)).
