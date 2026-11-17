# Tools

Host-side helpers for the firmware. Build/flash overview: [../README.md](../README.md).

| File | Purpose |
|---|---|
| `flash_set_single_bank.sh` | Check the MCU's flash bank mode and switch a new chip to single bank (`DBANK=0`). Run once per chip before flashing anything |
| `embed_with_cfg.sh` | Pack a TOML config and flash it into config slot A or B (the two flash copies of the configuration, see [CRS Slots](../docs/specs/Spec%20-%20CRS%20Slots.md)) over SWD (probe-rs 0.32.x) |
| `third_party_licenses.sh` | Regenerate the repo-root `THIRD_PARTY_LICENSES.md` from `cargo metadata` |
| `car_config_v1.toml` | The author's real in-car configuration |
| `example.toml`, `example_alt.toml` | Example configurations |
| `custom_smoke_test.toml` | Configuration used for smoke testing |

How the `.toml` files are organised and what every key means: [Spec - CRS Format §4.0](../docs/specs/Spec%20-%20CRS%20Format.md#40-file-organisation).

## Set single-bank mode (once per new chip)

> [!CAUTION]
> Run this before flashing anything to a new board. On a dual-bank chip it switches to single bank and erases the whole flash.

```bash
cd firmware/tools
./flash_set_single_bank.sh        # prints FLASH_OPTR and what it did
```

Why: [recovery §2](../docs/recovery.md#2-first-time-provisioning-set-single-bank-mode-dbank0).

## Flash a config over SWD

The script needs two files, created once by building:
- `kb_firmware/target/crs_artifacts/flash_layout.env` (the flash addresses), from a firmware build: [firmware README, Build](../README.md#build).
<br>
    ```bash
    cd firmware/kb_firmware
    cargo build --release
    ```

- `crs-pack` in `crs_tools/target/`, from a `crs-pack` build: [crs_tools README, Build](../crs_tools/README.md#build).
<br>
    ```bash
    cd firmware/crs_tools
    cargo build -p crs-pack --release
    ```

Then flash a config:

```bash
cd firmware/tools
./embed_with_cfg.sh --toml ./example.toml            # Slot A (default)
./embed_with_cfg.sh --toml ./example.toml --slot B
```

- **Every run clears both slots and the LED journal**, then writes the config into the chosen slot. Any previous config and the remembered backlight brightness and on/off are gone.
- Does **not** flash the firmware.
- Aborts unless the MCU is single-bank (see [Set single-bank mode](#set-single-bank-mode-once-per-new-chip)).

## Regenerate third-party notices

```bash
firmware/tools/third_party_licenses.sh      # run from anywhere, needs cargo + python3
```
