// src/config/defaults.rs
//
// Built-in configuration template, shown as the "suggested setup" in the web configurator (served
// via READ_CONFIG selector FACTORY_DEFAULTS). It is never applied automatically: a board with no valid slot still
// boots into DisabledFallback until a config is saved.
//
// Mapping documented in Spec - Action Set §4:
// - Encoder 1: Volume Up/Down, push = Mute
// - Encoder 2: UI navigation Down/Up (arrow keys), push = Enter
// - Panel 1..6: Play/Pause, Next, Previous, Back (Esc), Home (AC Home), Menu (Keyboard Menu)

use crs_format::{
    crc32_payload, make_header, payload_bytes, serialize, validate_payload, ActionWireV1,
    ButtonEntryV1, CrsPayloadV1, EncoderEntryV1, GlobalDefaultsV1, TOTAL_LEN,
};

use super::fw::FW_COMPAT_ID;

/// `build_id` stamped into the factory-default CRS header.
pub const FACTORY_BUILD_ID: u32 = 0xFAC7_0001;

const fn cc(usage: u16) -> ActionWireV1 {
    ActionWireV1 {
        tag: 2,
        a: (usage & 0xFF) as u8,
        b: (usage >> 8) as u8,
        c: 0,
    }
}

const fn kbd(mods: u8, usage: u8) -> ActionWireV1 {
    ActionWireV1 {
        tag: 1,
        a: mods,
        b: usage,
        c: 0,
    }
}

const NONE: ActionWireV1 = ActionWireV1 {
    tag: 0,
    a: 0,
    b: 0,
    c: 0,
};

/// Enabled button inheriting every timing/electrical setting from the globals.
const fn button(short_action: ActionWireV1, long_action: ActionWireV1) -> ButtonEntryV1 {
    ButtonEntryV1 {
        enabled: 1,
        pull: -1,
        active_low: -1,
        debounce_ms: -1,
        long_press_ms: -1,
        pulse_ms: -1,
        repeat_period_ms: 0,
        reserved0: 0,
        short_action,
        long_action,
        reserved1: [0, 0],
        reserved2: [0, 0],
    }
}

const fn encoder(cw_action: ActionWireV1, ccw_action: ActionWireV1) -> EncoderEntryV1 {
    EncoderEntryV1 {
        enabled: 1,
        swap_ab: -1,
        detent_guard_ms: -1,
        encoder_pulse_ms: 30,
        reserved0: 0,
        cw_action,
        ccw_action,
        reserved1: [0, 0, 0, 0],
    }
}

pub const FACTORY_DEFAULT_PAYLOAD: CrsPayloadV1 = CrsPayloadV1 {
    globals: GlobalDefaultsV1 {
        scan_hz: 1000,
        pin_pull_default: 1, // up (board wiring is active-low to SIG_GND)
        active_low_default: 1,
        debounce_ms_default: 5,
        long_press_ms_default: 600,
        pulse_ms_button_default: 50,
        repeat_period_ms_default: 0,
        encoder_pulse_ms_default: 30,
        encoder_swap_ab_default: 0,
        encoder_detent_guard_ms_default: 2,
        led_state_persist: 1,
        led_state_on_boot: 2, // restore
        led_level_persist: 1,
        led_level_on_boot: 0, // restore
        led_level_default: 30,
        led_level_step: 5,
        led_level_min: 10,
        led_level_max: 100,
        led_autosave_delay_ms: 1500,
        reserved: [0, 0],
    },
    // CRS order: encoder push buttons (index 0..1), then panel buttons 1..6 (index 2..7).
    buttons: [
        button(cc(0x00E2), NONE),       // Encoder 1 push: Mute
        button(kbd(0x00, 0x28), NONE),  // Encoder 2 push: Enter (DPAD center)
        button(cc(0x00CD), NONE),       // Panel 1: Play/Pause
        button(cc(0x00B5), NONE),       // Panel 2: Next track
        button(cc(0x00B6), NONE),       // Panel 3: Previous track
        button(kbd(0x00, 0x29), NONE),  // Panel 4: Back (Escape)
        button(cc(0x0223), NONE),       // Panel 5: Home (AC Home)
        button(kbd(0x00, 0x76), NONE),  // Panel 6: Menu (Keyboard Menu)
    ],
    encoders: [
        encoder(cc(0x00E9), cc(0x00EA)),           // Encoder 1: Volume Up / Down
        encoder(kbd(0x00, 0x51), kbd(0x00, 0x52)), // Encoder 2: Down / Up
    ],
};

// The defaults must satisfy the same validator as any WebHID-written config.
const _: () = match validate_payload(&FACTORY_DEFAULT_PAYLOAD) {
    Ok(()) => (),
    Err(_) => panic!("factory default config fails validate_payload"),
};

/// Factory defaults as a complete CRS v1 blob for this firmware.
pub fn factory_default_crs() -> [u8; TOTAL_LEN] {
    let p = FACTORY_DEFAULT_PAYLOAD;
    let crc = crc32_payload(payload_bytes(&p));
    serialize(&make_header(FW_COMPAT_ID, crc, FACTORY_BUILD_ID), &p)
}
