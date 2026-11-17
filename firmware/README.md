# KB-E2B6 firmware and host tools

Everything that runs on the controller's STM32G474CB, plus the host tools that pack configurations for it. If you have the [reference kit](../docs/kit.md) (the controller with tested encoders, buttons, adapter boards and cables), you don't need any of this: its controller comes flashed and is configured with the browser [configurator](../docs/configuring.md). The board side (pin map, LED drive, inputs) is in [docs/hardware.md](../docs/hardware.md); making a board from scratch in [docs/building.md](../docs/building.md).

| Folder | What it is |
|---|---|
| [`kb_firmware/`](kb_firmware/) | Firmware crate `kb-firmware` (Rust, Embassy, `no_std`, no heap) |
| [`crs_tools/`](crs_tools/README.md) | `crs_format` (config format, validator, WebHID protocol core; shared with the firmware) and the `crs-pack` CLI (TOML → config binary) |
| [`tools/`](tools/README.md) | Flash a TOML config over SWD, regenerate third-party notices, example configs |
| [`docs/`](docs/) | Architecture and design rules, recovery, specs (lists below) |

How the pieces depend on each other:
- `kb_firmware` uses `../crs_tools/crs_format` by relative path. Keep the two folders side by side.
- `web_config/js/{crs,protocol}.js` mirror `crs_format` (config layout, CRC-32, protocol). Change one, change both.
- Building the firmware writes `kb_firmware/target/crs_artifacts/flash_layout.env` (flash addresses); `tools/embed_with_cfg.sh` reads it.

---

## You need

- **Rust** (stable, developed with 1.92): install with [rustup](https://rustup.rs), then add the ARM target:

  ```bash
  rustup target add thumbv7em-none-eabihf
  ```

- **[probe-rs](https://probe.rs) 0.32.x** (`probe-rs`, `cargo-embed`) for flashing (or use the installer from probe.rs):

  ```bash
  cargo install probe-rs-tools --version "^0.32" --locked
  ```

- A **SWD probe** (CMSIS-DAP/DAPLink such as the WeAct Studio Mini Debugger I use, ST-LINK, J-Link) wired to the board's 4-pin header: `1` 3V3 · `2` SWDIO · `3` SWCLK · `4` GND (no reset pin). Cheap ST-LINK V2 dongles drive their 3.3 V pin: with the board on USB, leave it unconnected.

Building and `cargo embed` work on Windows, Linux and macOS. The `.sh` helpers in `tools/` need bash (Linux, macOS, WSL); from WSL the probe has to be passed through with [usbipd-win](https://github.com/dorssel/usbipd-win). Repository inside WSL: Windows tools can open it via `\\wsl.localhost\<distro>\...`.

## Build

```bash
cd firmware/kb_firmware
cargo build --release          # → target/thumbv7em-none-eabihf/release/kb-firmware
```

About 81 KB of the 116 KiB firmware area is used (`opt-level = "s"` + LTO). This is the standard build with the default features; other combinations: [Build options](#build-options-cargo-features).

Host tool `crs-pack`:

```bash
cd firmware/crs_tools
cargo build -p crs-pack --release
```

## Flash

> [!CAUTION]
> **New chip: run `flash_set_single_bank.sh` once before flashing anything.** STM32G474CB chips leave the factory in dual-bank mode; firmware and configuration flashing support single bank only ([why, and recovering a stuck board](docs/recovery.md#2-first-time-provisioning-set-single-bank-mode-dbank0)).

```bash
cd firmware/tools
./flash_set_single_bank.sh
```

Then build and flash the firmware:

```bash
cd firmware/kb_firmware
cargo embed --release          # build + flash + RTT log
```

`cargo embed` stays attached after flashing and shows the firmware's log; <kbd>Ctrl</kbd>+<kbd>C</kbd> quits.

Flashing firmware keeps the saved configuration (it lives in the reserved last 12 KiB of flash). A firmware that changes the config compatibility ID (`FW_COMPAT_ID`) rejects old configs.

Writing a config over SWD instead of the browser (engineering path). The config is a TOML file: how it is organised, [Spec - CRS Format §4.0](docs/specs/Spec%20-%20CRS%20Format.md#40-file-organisation); ready-made files and the script's options, [tools README](tools/README.md). It is written into one of two flash slots, A and B ([CRS Slots](docs/specs/Spec%20-%20CRS%20Slots.md)):

```bash
cd firmware/tools
./embed_with_cfg.sh --toml ./car_config_v1.toml      # Slot A; --slot B for B
```

## Build options (Cargo features)

The standard build uses all four default features:

| Feature | Default | What it does |
|---|---|---|
| `board_e2b6_128k` | on, required | The board: pin map and flash layout of the KB-E2B6 (STM32G474CB, 128 KiB). The build stops without it |
| `flash_single_bank` | on, required | Single-bank flash driver; must match the chip's `DBANK=0` ([Flash Banking](docs/specs/Spec%20-%20Flash%20Banking.md)) |
| `hw_pwm` | on | Backlight PWM in hardware (TIM2 on PA1, 1 kHz). Off: software PWM, a GPIO toggled at 100 Hz |
| `usb_cdc` | on | The USB serial debug console (CDC ACM). Off: no serial port; keyboard, media keys and the configuration interface stay |

Cargo can't switch off one default feature on its own: `--no-default-features` switches them all off, then `--features` lists the ones to keep (always including the two required ones).

Without the debug console:

```bash
cd firmware/kb_firmware
cargo build --release --no-default-features --features "board_e2b6_128k,hw_pwm,flash_single_bank"
```

Software PWM instead of hardware PWM:

```bash
cd firmware/kb_firmware
cargo build --release --no-default-features --features "board_e2b6_128k,flash_single_bank,usb_cdc"
```

The same flags work with `cargo embed`, which builds and flashes in one go:

```bash
cd firmware/kb_firmware
cargo embed --release --no-default-features --features "board_e2b6_128k,hw_pwm,flash_single_bank"
```

## Docs

| Document | Content |
|---|---|
| [ARCHITECTURE.md](docs/ARCHITECTURE.md) | Design rules and decisions with their reasons, tasks, boot flow, modules, flash layout, pitfalls |
| [recovery.md](docs/recovery.md) | Single-bank setup (why, and recovering a stuck board), SWD reflash, wiping/restoring the config |

Specs (normative; the code is the final truth):

| Spec | Content |
|---|---|
| [Configuration Reference](docs/specs/Spec%20-%20Configuration%20Reference.md) | How the board behaves and what every setting means: gestures, timing, all parameters with defaults |
| [Action Set](docs/specs/Spec%20-%20Action%20Set.md) | Action types, the configurator's key list, the internal backlight actions, the built-in template |
| [CRS Format](docs/specs/Spec%20-%20CRS%20Format.md) | How a TOML configuration file is organised (every key), and the 288-byte binary configuration (CRS v1) |
| [CRS Slots](docs/specs/Spec%20-%20CRS%20Slots.md) | The two flash slots A/B: header, validity, boot selection, writer rules |
| [LED Runtime State Persistence](docs/specs/Spec%20-%20LED%20Runtime%20State%20Persistence.md) | How backlight on/off and level are remembered in flash (the LED journal) |
| [WebHID Config Protocol](docs/specs/Spec%20-%20WebHID%20Config%20Protocol.md) | The USB protocol between the browser configurator and the board |
| [Flash Banking](docs/specs/Spec%20-%20Flash%20Banking.md) | Why the chip must run in single bank, and the settings that must agree |

## Maintenance

- After dependency changes, regenerate the root `THIRD_PARTY_LICENSES.md`:

  ```bash
  firmware/tools/third_party_licenses.sh
  ```

- Host tests (validator, WebHID protocol core, packer):

  ```bash
  cd firmware/crs_tools
  cargo test
  ```

- Clean the build artifacts:

  ```bash
  cd firmware
  ./clear_artifacts.sh
  ```
