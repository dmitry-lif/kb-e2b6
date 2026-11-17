# Spec — Flash Banking: single-bank mode of the STM32G474CB

Status: **normative**. Read it before changing flash offsets, the page size, the board layout module or the flashing scripts.

## Summary

- The STM32G474CBT6 has 128 KiB of flash that can run as one bank or two. Chips leave the factory in **dual bank** (`DBANK=1`); this firmware and its flashing scripts support **single bank only** (`DBANK=0`).
- Set it once per chip before flashing anything. What single and dual bank mean, why it matters and how to recover a stuck board: [recovery §2](../recovery.md#2-first-time-provisioning-set-single-bank-mode-dbank0).

  ```bash
  cd firmware/tools
  ./flash_set_single_bank.sh
  ```

- On a dual-bank chip the firmware still boots but disables its own flash writes (`hw/flash.rs`), and `embed_with_cfg.sh` refuses to write a configuration.

## Chip flash facts (STM32G474CBT6)

| Parameter                     | Value                                             |
| ----------------------------- | ------------------------------------------------- |
| Main flash                    | 128 KiB = 131,072 bytes, base `0x0800_0000`       |
| Architecture                  | Category 3, configurable single/dual bank         |
| Factory default               | **Dual bank (DBANK=1)**                            |
| Dual-bank organization        | 2 × 64 KiB, **2 KiB** pages, 32 pages/bank         |
| Dual-bank addresses           | Bank 1 `0x0800_0000`, bank 2 **`0x0804_0000`**; nothing documented in between |
| Dual-bank read width          | 64-bit                                            |
| Single-bank organization      | 1 × 128 KiB, **4 KiB** pages, 32 pages, contiguous `0x0800_0000..0x0801_FFFF` |
| Single-bank read width        | 128-bit                                           |
| Program granularity           | 64-bit / 8-byte double word                       |
| ECC                           | 8 ECC bits per 64-bit double word                 |
| Read-while-write              | **Dual-bank mode only**                           |
| OTP                           | 1 KiB, separate from main flash                   |
| Endurance / retention         | 10,000 erase/program cycles min; 30 years @ 85 °C after 1k cycles |
| DBANK control                 | User option byte (`FLASH_OPTR.DBANK`, bit 22 of `0x4002_2020`) |

## Why single bank

- The firmware layout needs one contiguous 128 KiB block from `0x0800_0000`: an image of about 80 KiB plus the LED journal and the two config slots in the last 12 KiB. In dual bank, everything above `0x0801_0000` is outside the documented flash.
- Dual bank's extras don't help: read-while-write needs all running code in bank 1 (at most 64 KiB), and 2 KiB pages would only save 6 KiB of reserved flash.

## Consequences of single bank for firmware flash writes

- **No read-while-write:** any erase/program of the LED journal page or a config slot stalls instruction fetch from flash (including ISRs) until it completes. A 4 KiB page erase takes tens of milliseconds, so expect a brief scan/USB pause on page erases. Record programming is short.
- **Program each double word once:** a 64-bit double word cannot be re-programmed without an erase (ECC is computed per double word). On-flash formats must be designed so every double word is written exactly once per erase cycle, e.g. a commit marker lives in its own double word.
- **Torn writes:** power loss during programming can leave a double word with inconsistent ECC. Reading it raises an uncorrectable-ECC error (NMI). Readers of the reserved pages must tolerate this. Implemented in `kb_firmware/src/hw/flash.rs`: an NMI handler records and clears `FLASH_ECCR.ECCD/ECCD2`, and `read_checked()` reports the error so the config loader treats the slot as invalid and the LED journal skips the record (then erases the page).

## Repository invariants (keep these consistent)

All three must agree or the device will mis-erase/mis-program flash:

1. **Cargo feature** `flash_single_bank` (maps to `embassy-stm32/single-bank`) — selects the single-bank flash region table (4 KiB pages) for the runtime driver.
2. **`FLASH_ERASE_PAGE_SIZE = 4096`** in `kb_firmware/boards/e2b6_128k/flash_layout.rs` — drives config slot size, slot/LED offsets, and the host flashing `--slot-size`.
3. **Device option byte `DBANK = 0`** — the physical chip must actually be in single bank (set with `firmware/tools/flash_set_single_bank.sh`).

Single-bank layout (derived; do not hand-edit offsets):

```text
FLASH_END               = 0x0802_0000   (128 KiB)
Firmware region         = 0x0800_0000 .. 0x0801_D000
LED journal page (4K)   = 0x0801_D000 .. 0x0801_E000
Config Slot A    (4K)   = 0x0801_E000 .. 0x0801_F000
Config Slot B    (4K)   = 0x0801_F000 .. 0x0802_0000
memory.x reserves        last 12 KiB (3 × 4 KiB pages)
```

If you ever move to dual bank (only conceivable if the firmware shrinks below 64 KiB, or for read-while-write with data in bank 2), you must switch ALL THREE together: replace the `flash_single_bank` feature with a dual-bank one (no such feature exists yet; it would map to embassy-stm32's dual-bank flash layout), `FLASH_ERASE_PAGE_SIZE = 2048`, and `DBANK=1`, and re-derive offsets for 2 KiB pages, 64 KiB banks and bank 2 at `0x0804_0000`. Do not mix.

## References

- STM32G474CB datasheet (flash size code `B` = 128 KB); RM0440 (STM32G4 reference manual): flash organization, `DBANK` option bit, single vs dual bank page sizes and read widths.
- Firmware: `kb_firmware/boards/e2b6_128k/flash_layout.rs`, `kb_firmware/boards/e2b6_128k/memory.x`, `kb_firmware/build.rs`, `kb_firmware/Cargo.toml`, `kb_firmware/src/hw/flash.rs`.
- Host tools: `firmware/tools/flash_set_single_bank.sh` (sets `DBANK=0`), `firmware/tools/embed_with_cfg.sh` (aborts unless `DBANK=0`).
- Related: [ARCHITECTURE §4](../ARCHITECTURE.md#4-flash--linker-layout-config-slots) (Flash + linker layout).
