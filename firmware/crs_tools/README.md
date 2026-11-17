# crs_tools

Host tools for KB-E2B6 configurations. A configuration is written as TOML and packed into **CRS**, the fixed 288-byte binary configuration the firmware reads (28-byte header with CRC-32 + 260-byte payload; the device never parses TOML). On the device it is stored in one of two flash slots, A and B. Specs: [CRS Format](../docs/specs/Spec%20-%20CRS%20Format.md), [CRS Slots](../docs/specs/Spec%20-%20CRS%20Slots.md).

- `crs-pack` is the CLI tool that packs TOML → CRS and dumps/validates CRS.
- `crs-format` defines the frozen CRS v1 wire layout, the CRC-32, the semantic validator (`validate_payload`, the same rules the firmware applies to WebHID writes) and the WebHID configuration protocol core (`webhid` module). It is `no_std` and shared with the firmware.

## Build

```bash
cd firmware/crs_tools
cargo build -p crs-pack            # debug build
cargo build -p crs-pack --release  # release build
```

Binary output:
- `target/debug/crs-pack`
- `target/release/crs-pack`

## Test

```bash
cd firmware/crs_tools
cargo test
```

Tests are hermetic:
- TOML fixtures live in `testdata/`
- tests use `include_bytes!()` (no runtime file I/O)

## Usage

How a TOML configuration file is organised: [Spec - CRS Format §4.0](../docs/specs/Spec%20-%20CRS%20Format.md#40-file-organisation).

### Pack TOML → CRS binary

```bash
crs-pack pack --in cfg.toml --out cfg.crs
```

### Pack TOML → flashable slot image

```bash
crs-pack pack-slot --in cfg.toml --out slot_a.bin --slot-size 4096
```

This emits a slot-sized binary suitable for flashing directly into a config slot:
- SlotHeaderV1 at offset `0x00` (seq/committed)
- CRS bytes at offset `0x20` (288 bytes)
- `0xFF` padding (erased-flash value) up to `--slot-size`

`--slot-size` must match the device erase page size (4096 on STM32G474CB single bank; minimum 320 = 32 + 288).

Optional slot header controls:
- `--seq <U32>`: A/B sequence number (higher = newer). Defaults to the current unix time.
- `--uncommitted`: emit `committed = 0xFFFF_FFFF` (slot is ignored by firmware until committed).

Optional, for both `pack` and `pack-slot`: force a known `build_id` in the CRS header:

```bash
crs-pack pack --in cfg.toml --out cfg.crs --build-id 305419896
```

Note: `--build-id` is parsed as a decimal `u32`. If omitted, `crs-pack` derives it as CRC32 of the input TOML bytes.

### Validate TOML (no output file)

```bash
crs-pack validate --in cfg.toml
```

### Dump CRS (human summary)

```bash
crs-pack dump --in cfg.crs
```

Optionally require the CRS header `fw_compat_id` to match:

```bash
crs-pack dump --in cfg.crs --fw-compat-id 0x20260216
```

### Dump CRS → canonical TOML

```bash
crs-pack dump --in cfg.crs --toml
```

This prints a canonical TOML representation intended to round-trip back to the same CRS payload.

## Spec

Canonical specs live in `firmware/docs/specs/`:
- [../docs/specs/Spec - CRS Format.md](../docs/specs/Spec%20-%20CRS%20Format.md)
- [../docs/specs/Spec - Configuration Reference.md](../docs/specs/Spec%20-%20Configuration%20Reference.md)
- [../docs/specs/Spec - Action Set.md](../docs/specs/Spec%20-%20Action%20Set.md)
- [../docs/specs/Spec - CRS Slots.md](../docs/specs/Spec%20-%20CRS%20Slots.md)
- [../docs/specs/Spec - WebHID Config Protocol.md](../docs/specs/Spec%20-%20WebHID%20Config%20Protocol.md)
