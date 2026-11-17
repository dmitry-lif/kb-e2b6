// Board flash layout constants.
// This file is consumed by kb_firmware/build.rs to emit:
// - OUT_DIR/flash_layout.rs (compiled into firmware)
// - kb_firmware/target/crs_artifacts/flash_layout.env (sourced by scripts)
//
// FLASH BANKING: this board runs the MCU in SINGLE bank (DBANK=0). Do not change the
// page size or bank mode without reading the spec — dual bank breaks probe-rs 0.31 and
// won't fit the firmware in one 64KiB bank.
// See firmware/docs/specs/Spec - Flash Banking.md.

pub const PROBE_RS_CHIP: &str = "STM32G474CB";
// STM32G474CB configured as SINGLE bank (option byte DBANK=0): one contiguous
// 128KiB bank (0x0800_0000–0x0801_FFFF), 4KiB erase page. In dual bank the flash is
// not contiguous (bank 1 = 0x0800_0000..0x0800_FFFF, bank 2 starts at 0x0804_0000),
// so flashing this layout fails at 0x0801_0000.
pub const FLASH_END: u32 = 0x0802_0000;

// Single-bank STM32G4 erase page size is 4KiB (dual-bank would be 2KiB).
// Must stay consistent with the device DBANK option byte and the embassy
// `single-bank` feature so firmware + scripts agree on page geometry.
pub const FLASH_ERASE_PAGE_SIZE: u32 = 4096;

// Config slots are exactly one erase page each (A/B update policy).
pub const CFG_SLOT_SIZE: u32 = FLASH_ERASE_PAGE_SIZE;
pub const CFG_SLOT_A_BASE: u32 = FLASH_END - (2 * CFG_SLOT_SIZE); // 0x0801_E000
pub const CFG_SLOT_B_BASE: u32 = FLASH_END - CFG_SLOT_SIZE; // 0x0801_F000

// Reserved flash page for LED runtime state persistence journal.
pub const LED_STATE_FLASH_PAGE_SIZE: u32 = FLASH_ERASE_PAGE_SIZE;
pub const LED_STATE_FLASH_PAGE_BASE: u32 = CFG_SLOT_A_BASE - LED_STATE_FLASH_PAGE_SIZE; // 0x0801_D000
