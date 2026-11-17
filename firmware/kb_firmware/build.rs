fn main() {
    use std::fs;
    use std::path::PathBuf;

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tdefmt.x");

    // Provide a board-specific `memory.x` via OUT_DIR.
    // Note: `link.x` already does `INCLUDE memory.x`, so we must NOT pass `-Tmemory.x`
    // (doing so would load it twice and fail with "region 'FLASH' already defined").
    // We keep the authoritative per-board layout under `boards/<board>/memory.x`.

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    let (board_name, flash) = select_board();
    let board_dir = manifest_dir.join("boards").join(board_name);

    let memory_src = board_dir.join("memory.x");
    let memory_dst = out_dir.join("memory.x");
    println!("cargo:rerun-if-changed={}", memory_src.display());
    fs::copy(&memory_src, &memory_dst).expect("copy memory.x to OUT_DIR");
    println!("cargo:rustc-link-search={}", out_dir.display());

    let flash_layout_src = board_dir.join("flash_layout.rs");
    println!("cargo:rerun-if-changed={}", flash_layout_src.display());

    emit_flash_layout_artifacts(board_name, &flash);
}

#[path = "boards/e2b6_128k/flash_layout.rs"]
mod board_e2b6_128k;

#[derive(Clone, Copy)]
struct FlashLayout {
    chip: &'static str,
    flash_end: u32,
    flash_erase_page_size: u32,
    cfg_slot_size: usize,
    cfg_slot_a_base: u32,
    cfg_slot_b_base: u32,
    led_state_flash_page_base: u32,
    led_state_flash_page_size: u32,
}

fn select_board() -> (&'static str, FlashLayout) {
    // Only one board variant: e2b6_128k (STM32G474CB, 128K flash, SINGLE bank / DBANK=0,
    // 4KiB pages). Single bank is mandatory: in dual bank the flash isn't contiguous (bank 2
    // @ 0x0804_0000) and the firmware won't fit in one 64KiB bank. Keep this in sync with
    // the `flash_single_bank` Cargo feature and the on-chip DBANK option byte.
    // See firmware/docs/specs/Spec - Flash Banking.md.
    if std::env::var_os("CARGO_FEATURE_BOARD_E2B6_128K").is_none() {
        panic!("No board_* feature selected (expected board_e2b6_128k)");
    }

    let layout = FlashLayout {
        chip: board_e2b6_128k::PROBE_RS_CHIP,
        flash_end: board_e2b6_128k::FLASH_END,
        flash_erase_page_size: board_e2b6_128k::FLASH_ERASE_PAGE_SIZE,
        cfg_slot_size: board_e2b6_128k::CFG_SLOT_SIZE as usize,
        cfg_slot_a_base: board_e2b6_128k::CFG_SLOT_A_BASE,
        cfg_slot_b_base: board_e2b6_128k::CFG_SLOT_B_BASE,
        led_state_flash_page_base: board_e2b6_128k::LED_STATE_FLASH_PAGE_BASE,
        led_state_flash_page_size: board_e2b6_128k::LED_STATE_FLASH_PAGE_SIZE,
    };
    validate_layout(&layout);
    ("e2b6_128k", layout)
}

fn validate_layout(layout: &FlashLayout) {
    let slot = layout.cfg_slot_size as u32;

    if layout.flash_erase_page_size == 0 {
        panic!("flash_erase_page_size must be non-zero");
    }

    if layout.cfg_slot_a_base >= layout.flash_end {
        panic!(
            "cfg_slot_a_base (0x{:08X}) must be < flash_end (0x{:08X})",
            layout.cfg_slot_a_base, layout.flash_end
        );
    }
    if layout.cfg_slot_b_base >= layout.flash_end {
        panic!(
            "cfg_slot_b_base (0x{:08X}) must be < flash_end (0x{:08X})",
            layout.cfg_slot_b_base, layout.flash_end
        );
    }
    if layout.cfg_slot_b_base < layout.cfg_slot_a_base {
        panic!(
            "cfg_slot_b_base (0x{:08X}) must be >= cfg_slot_a_base (0x{:08X})",
            layout.cfg_slot_b_base, layout.cfg_slot_a_base
        );
    }

    let expected_b = layout.flash_end - slot;
    let expected_a = layout.flash_end - (2 * slot);
    if layout.cfg_slot_b_base != expected_b {
        panic!(
            "cfg_slot_b_base mismatch: got 0x{:08X}, expected 0x{:08X} (= flash_end - 1*slot)",
            layout.cfg_slot_b_base, expected_b
        );
    }
    if layout.cfg_slot_a_base != expected_a {
        panic!(
            "cfg_slot_a_base mismatch: got 0x{:08X}, expected 0x{:08X} (= flash_end - 2*slot)",
            layout.cfg_slot_a_base, expected_a
        );
    }
    if (layout.cfg_slot_b_base - layout.cfg_slot_a_base) != slot {
        panic!(
            "cfg slot size mismatch: (B - A) = 0x{:08X}, expected 0x{:08X}",
            layout.cfg_slot_b_base - layout.cfg_slot_a_base,
            slot
        );
    }

    // LED journal page must not overlap config slots and must be aligned.
    if layout.led_state_flash_page_size == 0 {
        panic!("led_state_flash_page_size must be non-zero");
    }

    if layout.led_state_flash_page_size != layout.flash_erase_page_size {
        panic!(
            "LED persistence page size must equal flash erase page size: led_state_flash_page_size=0x{:08X} flash_erase_page_size=0x{:08X}",
            layout.led_state_flash_page_size, layout.flash_erase_page_size
        );
    }
    if layout.led_state_flash_page_base >= layout.flash_end {
        panic!(
            "led_state_flash_page_base (0x{:08X}) must be < flash_end (0x{:08X})",
            layout.led_state_flash_page_base, layout.flash_end
        );
    }
    let led_end = layout
        .led_state_flash_page_base
        .checked_add(layout.led_state_flash_page_size)
        .expect("led page end overflow");
    if led_end > layout.cfg_slot_a_base {
        panic!(
            "LED persistence page overlaps config slots: led_end=0x{:08X} cfg_slot_a_base=0x{:08X}",
            led_end, layout.cfg_slot_a_base
        );
    }
    if (layout.led_state_flash_page_base % layout.led_state_flash_page_size) != 0 {
        panic!(
            "LED persistence page base must be aligned to page size: base=0x{:08X} size=0x{:08X}",
            layout.led_state_flash_page_base, layout.led_state_flash_page_size
        );
    }
}

fn emit_flash_layout_artifacts(board_name: &str, layout: &FlashLayout) {
    use std::fs;
    use std::path::PathBuf;

    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));

    // Rust constants compiled into firmware.
    let rust_path = out_dir.join("flash_layout.rs");
    let rust = format!(
        "// @generated by kb_firmware/build.rs from kb_firmware/boards/{board}/flash_layout.rs\n\
#[allow(dead_code)]\n\
pub const FLASH_END: u32 = 0x{flash_end:08X};\n\
#[allow(dead_code)]\n\
pub const FLASH_ERASE_PAGE_SIZE: u32 = {flash_erase_page_size};\n\
#[allow(dead_code)]\n\
pub const CFG_SLOT_SIZE: u32 = {cfg_slot_size};\n\
#[allow(dead_code)]\n\
pub const CFG_SLOT_A_BASE: u32 = 0x{cfg_slot_a_base:08X};\n\
#[allow(dead_code)]\n\
pub const CFG_SLOT_B_BASE: u32 = 0x{cfg_slot_b_base:08X};\n\
#[allow(dead_code)]\n\
pub const LED_STATE_FLASH_PAGE_BASE: u32 = 0x{led_base:08X};\n\
#[allow(dead_code)]\n\
pub const LED_STATE_FLASH_PAGE_SIZE: u32 = {led_size};\n\
#[allow(dead_code)]\n\
pub const PROBE_RS_CHIP: &str = \"{chip}\";\n",
        board = board_name,
        flash_end = layout.flash_end,
        flash_erase_page_size = layout.flash_erase_page_size,
        cfg_slot_size = layout.cfg_slot_size as u32,
        cfg_slot_a_base = layout.cfg_slot_a_base,
        cfg_slot_b_base = layout.cfg_slot_b_base,
        led_base = layout.led_state_flash_page_base,
        led_size = layout.led_state_flash_page_size,
        chip = layout.chip,
    );
    fs::write(&rust_path, rust).expect("write OUT_DIR/flash_layout.rs");

    // Shell-friendly environment file for scripts.
    // Keep it under kb_firmware/target so firmware builds are standalone.
    let stable_dir = manifest_dir.join("target").join("crs_artifacts");
    let _ = fs::create_dir_all(&stable_dir);
    let env_path = stable_dir.join("flash_layout.env");
    let env = format!(
        "# @generated by kb_firmware/build.rs from kb_firmware/boards/{board}/flash_layout.rs\n\
export KB_BOARD=\"{board}\"\n\
export PROBE_RS_CHIP=\"{chip}\"\n\
export FLASH_END=0x{flash_end:08X}\n\
export FLASH_ERASE_PAGE_SIZE={flash_erase_page_size}\n\
export CFG_SLOT_SIZE={cfg_slot_size}\n\
export CFG_SLOT_A_BASE=0x{cfg_slot_a_base:08X}\n\
export CFG_SLOT_B_BASE=0x{cfg_slot_b_base:08X}\n\
export LED_STATE_FLASH_PAGE_BASE=0x{led_base:08X}\n\
export LED_STATE_FLASH_PAGE_SIZE={led_size}\n",
        board = board_name,
        chip = layout.chip,
        flash_end = layout.flash_end,
        flash_erase_page_size = layout.flash_erase_page_size,
        cfg_slot_size = layout.cfg_slot_size,
        cfg_slot_a_base = layout.cfg_slot_a_base,
        cfg_slot_b_base = layout.cfg_slot_b_base,
        led_base = layout.led_state_flash_page_base,
        led_size = layout.led_state_flash_page_size,
    );
    fs::write(&env_path, env).expect("write kb_firmware/target/crs_artifacts/flash_layout.env");
}
