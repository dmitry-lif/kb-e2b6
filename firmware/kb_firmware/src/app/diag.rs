use core::fmt::Write;

use crate::text::CrlfWriter;

fn rtt_print_multiline(s: &str) {
    for line in s.split("\r\n") {
        if line.is_empty() {
            continue;
        }
        defmt::info!("{=str}", line);
    }
}

fn yes_no(v: bool) -> &'static str {
    if v {
        "Yes"
    } else {
        "No"
    }
}

fn ok_fail(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "succeeded",
        Some(false) => "failed",
        None => "n/a",
    }
}

fn pull_name(v: u8) -> &'static str {
    match v {
        0 => "None",
        1 => "Pull-up",
        2 => "Pull-down",
        _ => "invalid",
    }
}

fn active_level_name(active_low: bool) -> &'static str {
    if active_low {
        "Active-Low"
    } else {
        "Active-High"
    }
}

fn led_state_boot_name(v: u8) -> &'static str {
    match v {
        1 => "On",
        2 => "Restore",
        _ => "Off",
    }
}

pub fn info_text() -> heapless::String<2048> {
    let mut out: heapless::String<2048> = heapless::String::new();
    let mut w = CrlfWriter::new(&mut out);

    let _ = writeln!(w, "Info:");
    let _ = writeln!(
        w,
        "  USB device: {} (S/N {}, VID:PID {:04X}:{:04X})",
        crate::text::USB_PRODUCT,
        crate::text::usb_serial(),
        crate::text::USB_VID,
        crate::text::USB_PID
    );
    let _ = writeln!(
        w,
        "  Firmware: {} v{}",
        crate::text::FW_ID,
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(w, "  Chip: {}", crate::config::fw::PROBE_RS_CHIP);

    // Memory sizes are derived from the board-specific linker script (boards/*/memory.x).
    // RAM is fixed for STM32G474CB in our supported variants.
    let flash_end = crate::config::fw::FLASH_END;
    let flash_total = flash_end - 0x0800_0000;
    let _ = writeln!(
        w,
        "  Flash: {} KiB (end=0x{:08X})",
        flash_total / 1024,
        flash_end
    );
    let _ = writeln!(w, "  RAM: 128 KiB (per memory.x)");

    let _ = writeln!(w, "");
    let _ = writeln!(w, "Flash layout (reserved regions):");
    let _ = writeln!(
        w,
        "  Erase page size: {} bytes",
        crate::config::fw::FLASH_ERASE_PAGE_SIZE
    );
    let _ = writeln!(
        w,
        "  LED journal page: 0x{:08X}..0x{:08X} ({} bytes)",
        crate::config::fw::LED_STATE_FLASH_PAGE_BASE,
        crate::config::fw::LED_STATE_FLASH_PAGE_BASE + crate::config::fw::LED_STATE_FLASH_PAGE_SIZE,
        crate::config::fw::LED_STATE_FLASH_PAGE_SIZE
    );
    let _ = writeln!(
        w,
        "  Config slot A:    0x{:08X}..0x{:08X} ({} bytes)",
        crate::config::fw::CFG_SLOT_A_BASE,
        crate::config::fw::CFG_SLOT_A_BASE + crate::config::fw::CFG_SLOT_SIZE,
        crate::config::fw::CFG_SLOT_SIZE
    );
    let _ = writeln!(
        w,
        "  Config slot B:    0x{:08X}..0x{:08X} ({} bytes)",
        crate::config::fw::CFG_SLOT_B_BASE,
        crate::config::fw::CFG_SLOT_B_BASE + crate::config::fw::CFG_SLOT_SIZE,
        crate::config::fw::CFG_SLOT_SIZE
    );

    let _ = writeln!(w, "");
    let _ = writeln!(w, "Build configuration:");
    let board = if cfg!(feature = "board_e2b6_128k") {
        "e2b6_128k"
    } else {
        "(unknown)"
    };
    let _ = writeln!(w, "  Board: {}", board);
    let _ = writeln!(
        w,
        "  USB CDC console: {}",
        if cfg!(feature = "usb_cdc") {
            "Enabled"
        } else {
            "Disabled"
        }
    );
    let _ = writeln!(
        w,
        "  PWM: {}",
        if cfg!(feature = "hw_pwm") {
            "Hardware"
        } else {
            "Software"
        }
    );
    let bank = if cfg!(feature = "flash_single_bank") {
        "Single-bank"
    } else {
        "(default)"
    };
    let _ = writeln!(w, "  Flash banking: {}", bank);
    let _ = writeln!(
        w,
        "  FW compat ID: 0x{:08X}",
        crate::config::fw::FW_COMPAT_ID
    );
    let _ = writeln!(
        w,
        "  WebHID config interface: usage page 0x{:04X}, protocol v{}",
        crs_format::webhid::USAGE_PAGE,
        crs_format::webhid::PROTOCOL_VERSION
    );
    let (ecc_faults, ecc_addr) = crate::hw::flash::ecc_stats();
    let _ = writeln!(
        w,
        "  Flash ECC errors since boot: {} (last addr 0x{:05X})",
        ecc_faults, ecc_addr
    );

    out
}

pub async fn cfg_status_text() -> heapless::String<2048> {
    let st = crate::config::status::get_boot_status().await;
    let led = crate::hw::led_persist::get_status().await;

    let mut out: heapless::String<2048> = heapless::String::new();
    crate::config::status::format_status(&st, &mut out);

    let mut w = CrlfWriter::new(&mut out);
    let _ = writeln!(w, "");
    let _ = writeln!(w, "LED configuration storage status:");
    let _ = writeln!(w, "  Enabled: {}", yes_no(led.enabled));
    let _ = writeln!(
        w,
        "  Saved values available at boot: {} [{}]",
        yes_no(led.boot_found_record),
        led.boot_found_record
    );
    let _ = writeln!(
        w,
        "  Flash ops (last attempt): read: {}, erase: {}, write: {}",
        ok_fail(led.last_read_ok),
        ok_fail(led.last_erase_ok),
        ok_fail(led.last_write_ok)
    );
    let _ = writeln!(w, "  Base address: 0x{:08X}", led.page_base);
    let _ = writeln!(w, "  Page size (bytes): {}", led.page_size);
    let _ = writeln!(w, "  Record size (bytes): {}", led.rec_size);
    let _ = writeln!(w, "  Capacity (records): {}", led.cap);
    let _ = writeln!(w, "  Next write index: {}", led.write_index);
    let _ = writeln!(w, "  Dirty mask: 0x{:02X}", led.dirty_mask);
    let _ = writeln!(w, "  Pending commit: {}", yes_no(led.pending_commit));
    match led.last_seq {
        Some(v) => {
            let _ = writeln!(w, "  Last saved sequence number: {}", v);
        }
        None => {
            let _ = writeln!(w, "  Last saved sequence number: (none)");
        }
    }
    let _ = writeln!(w, "  Next sequence number: {}", led.next_seq);

    out
}

pub async fn cfg_show_text() -> heapless::String<2048> {
    let st = crate::config::status::get_boot_status().await;
    let cfg = crate::config::runtime::get_active_cfg().await;
    let pwm = crate::hw::pwm::get_state().await;
    let led = crate::hw::led_persist::get_status().await;

    let mut out: heapless::String<2048> = heapless::String::new();
    let mut w = CrlfWriter::new(&mut out);

    let _ = writeln!(w, "Active configuration settings:");
    let _ = writeln!(
        w,
        "Configuration source: {}",
        match st.source {
            crate::config::status::BootSource::SlotA => "Slot A",
            crate::config::status::BootSource::SlotB => "Slot B",
            crate::config::status::BootSource::DisabledFallback => "None (inputs disabled)",
        }
    );

    let _ = writeln!(w, "");
    let _ = writeln!(w, "Global parameters:");

    let _ = writeln!(w, "");
    let _ = writeln!(w, "LED related:");
    let _ = writeln!(
        w,
        "  [led_state_persist]: {} [{}]",
        yes_no(cfg.globals.led_state_persist),
        cfg.globals.led_state_persist
    );
    let _ = writeln!(
        w,
        "  [led_state_on_boot]: {} [{}]",
        led_state_boot_name(cfg.globals.led_state_on_boot),
        cfg.globals.led_state_on_boot
    );
    let _ = writeln!(
        w,
        "  [led_level_persist]: {} [{}]",
        yes_no(cfg.globals.led_level_persist),
        cfg.globals.led_level_persist
    );
    match cfg.globals.led_level_on_boot {
        0 => {
            let _ = writeln!(w, "  [led_level_on_boot]: Restore [0]");
        }
        1..=100 => {
            let raw = cfg.globals.led_level_on_boot;
            let _ = writeln!(w, "  [led_level_on_boot]: Fixed ({}) [{}]", raw, raw);
        }
        raw => {
            let _ = writeln!(w, "  [led_level_on_boot]: Default [{}]", raw);
        }
    }
    let _ = writeln!(
        w,
        "  [led_level_default]: {}",
        cfg.globals.led_level_default
    );
    let _ = writeln!(w, "  [led_level_step]: {}", cfg.globals.led_level_step);
    let _ = writeln!(w, "  [led_level_min]: {}", cfg.globals.led_level_min);
    let _ = writeln!(w, "  [led_level_max]: {}", cfg.globals.led_level_max);
    let _ = writeln!(
        w,
        "  [led_autosave_delay_ms]: {} ms",
        cfg.globals.led_autosave_delay_ms
    );

    let _ = writeln!(w, "");
    let _ = writeln!(w, "LED runtime state:");
    let current_state = if pwm.on { "ON" } else { "OFF" };
    let boot_state = if led.boot_on { "ON" } else { "OFF" };
    let state_from_restored = cfg.globals.led_state_on_boot == 2
        && cfg.globals.led_state_persist
        && led.boot_found_record;
    let state_from = if state_from_restored {
        "RESTORED"
    } else {
        "CFG"
    };
    let _ = writeln!(
        w,
        "  LED current state : {} (boot: [{}] from [{}])",
        current_state, boot_state, state_from
    );
    let level_from_restored = cfg.globals.led_level_on_boot == 0
        && cfg.globals.led_level_persist
        && led.boot_found_record;
    let level_from = if level_from_restored {
        "RESTORED"
    } else {
        "CFG"
    };
    let _ = writeln!(
        w,
        "  LED current level : {} (boot [{}] from [{}])",
        pwm.level, led.boot_level, level_from
    );

    let _ = writeln!(w, "");
    let _ = writeln!(w, "Scan and electrical defaults:");
    let _ = writeln!(w, "  [scan_hz]: {}", cfg.globals.scan_hz);
    let _ = writeln!(
        w,
        "  [pin_pull_default]: {} [{}]",
        pull_name(cfg.globals.pin_pull_default),
        cfg.globals.pin_pull_default
    );
    let _ = writeln!(
        w,
        "  [active_low_default]: {} [{}]",
        active_level_name(cfg.globals.active_low_default),
        cfg.globals.active_low_default
    );
    let _ = writeln!(
        w,
        "  [debounce_ms_default]: {} ms",
        cfg.globals.debounce_ms_default
    );
    let _ = writeln!(
        w,
        "  [long_press_ms_default]: {} ms",
        cfg.globals.long_press_ms_default
    );
    match cfg.globals.pulse_ms_button_default {
        0 => {
            let _ = writeln!(w, "  [pulse_ms_button_default]: Tracked-release [0]");
        }
        raw => {
            let _ = writeln!(
                w,
                "  [pulse_ms_button_default]: Pulsed ({} ms) [{}]",
                raw, raw
            );
        }
    }
    match cfg.globals.repeat_period_ms_default {
        0 => {
            let _ = writeln!(w, "  [repeat_period_ms_default]: Off [0]");
        }
        raw => {
            let _ = writeln!(w, "  [repeat_period_ms_default]: {} ms [{}]", raw, raw);
        }
    }
    let _ = writeln!(
        w,
        "  [encoder_pulse_ms_default]: {} ms",
        cfg.globals.encoder_pulse_ms_default
    );

    let _ = writeln!(w, "");
    let _ = writeln!(w, "Encoder decoding defaults:");
    let _ = writeln!(
        w,
        "  [encoder_swap_ab_default]: {} [{}]",
        yes_no(cfg.globals.encoder_swap_ab_default),
        cfg.globals.encoder_swap_ab_default
    );
    let _ = writeln!(
        w,
        "  [encoder_detent_guard_ms_default]: {} ms",
        cfg.globals.encoder_detent_guard_ms_default
    );

    out
}

pub async fn rtt_boot_dump() {
    // Banner is printed in main.rs already; do not repeat it here.
    let info = info_text();
    rtt_print_multiline(info.as_str());

    defmt::info!("----------------------------------------");

    let st = cfg_status_text().await;
    rtt_print_multiline(st.as_str());

    defmt::info!("----------------------------------------");

    let show = cfg_show_text().await;
    rtt_print_multiline(show.as_str());
}
