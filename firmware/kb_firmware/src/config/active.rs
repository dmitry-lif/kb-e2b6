use crs_format::{ActionWireV1, CrsPayloadV1};

use crate::config::action::ActionEncoding;

pub const MAX_PANEL_BUTTONS: usize = crs_format::MAX_PANEL_BUTTONS;
pub const MAX_ENCODER_BUTTONS: usize = crs_format::MAX_ENCODER_BUTTONS;
pub const MAX_ENCODERS: usize = crs_format::MAX_ENCODERS;
// Fixed ID for the hardcoded PCB button (PA0). Sits just past all configurable button IDs.
pub const PCB_BTN_ID: usize = MAX_PANEL_BUTTONS + MAX_ENCODER_BUTTONS;

#[derive(Clone, Copy, Debug)]
pub struct Globals {
    pub scan_hz: u16,
    pub pin_pull_default: u8, // 0 none, 1 up, 2 down
    pub active_low_default: bool,
    pub debounce_ms_default: u16,
    pub long_press_ms_default: u16,

    pub pulse_ms_button_default: u16,
    #[cfg_attr(not(feature = "usb_cdc"), allow(dead_code))]
    pub repeat_period_ms_default: u16,

    pub encoder_pulse_ms_default: u16,
    pub encoder_swap_ab_default: bool,
    pub encoder_detent_guard_ms_default: u16,

    pub led_state_persist: bool,
    pub led_state_on_boot: u8,
    pub led_level_persist: bool,
    pub led_level_on_boot: u8,
    pub led_level_default: u8,
    pub led_level_step: u8,
    pub led_level_min: u8,
    pub led_level_max: u8,
    pub led_autosave_delay_ms: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct ButtonCfg {
    pub enabled: bool,
    pub pull: u8,
    pub active_low: bool,
    pub debounce_ms: u16,
    pub long_press_ms: u16,
    pub pulse_ms: u16,
    pub repeat_period_ms: u16,
    pub short_action: ActionEncoding,
    pub long_action: ActionEncoding,
}

#[derive(Clone, Copy, Debug)]
pub struct EncoderCfg {
    pub enabled: bool,
    pub swap_ab: bool,
    pub detent_guard_ms: u16,
    pub encoder_pulse_ms: u16,
    pub cw_action: ActionEncoding,
    pub ccw_action: ActionEncoding,
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveCfg {
    pub globals: Globals,
    pub panel_buttons: [ButtonCfg; MAX_PANEL_BUTTONS],
    pub encoder_buttons: [ButtonCfg; MAX_ENCODER_BUTTONS],
    pub encoders: [EncoderCfg; MAX_ENCODERS],
}

fn decode_pull(v: u8) -> u8 {
    match v {
        0 | 1 | 2 => v,
        _ => 1, // default to pull-up if corrupted
    }
}

fn decode_action(a: ActionWireV1) -> ActionEncoding {
    match a.tag {
        0 => ActionEncoding::None,
        1 => ActionEncoding::Kbd {
            mods: a.a,
            usage: a.b,
        },
        2 => ActionEncoding::Cc {
            usage: (a.a as u16) | ((a.b as u16) << 8),
        },
        3 => ActionEncoding::Internal { id: a.a },
        _ => ActionEncoding::None,
    }
}

fn inherit_u8(override_i8: i8, global: u8) -> u8 {
    if override_i8 < 0 {
        global
    } else {
        override_i8 as u8
    }
}

fn inherit_bool(override_i8: i8, global: bool) -> bool {
    if override_i8 < 0 {
        global
    } else {
        override_i8 != 0
    }
}

fn inherit_u16(override_i16: i16, global: u16) -> u16 {
    if override_i16 < 0 {
        global
    } else {
        override_i16 as u16
    }
}

impl ActiveCfg {
    pub const fn disabled_fallback() -> Self {
        let globals = Globals {
            scan_hz: 1000,
            pin_pull_default: 1,
            active_low_default: true,
            debounce_ms_default: 5,
            long_press_ms_default: 600,
            pulse_ms_button_default: 50,
            repeat_period_ms_default: 0,
            encoder_pulse_ms_default: 30,
            encoder_swap_ab_default: false,
            encoder_detent_guard_ms_default: 2,
            led_state_persist: true,
            led_state_on_boot: 2, // restore
            led_level_persist: true,
            led_level_on_boot: 0, // restore
            led_level_default: 30,
            led_level_step: 5,
            led_level_min: 10,
            led_level_max: 100,
            led_autosave_delay_ms: 1500,
        };

        let btn = ButtonCfg {
            enabled: false,
            pull: globals.pin_pull_default,
            active_low: globals.active_low_default,
            debounce_ms: globals.debounce_ms_default,
            long_press_ms: globals.long_press_ms_default,
            pulse_ms: globals.pulse_ms_button_default,
            repeat_period_ms: 0,
            short_action: ActionEncoding::None,
            long_action: ActionEncoding::None,
        };

        let enc = EncoderCfg {
            enabled: false,
            swap_ab: globals.encoder_swap_ab_default,
            detent_guard_ms: globals.encoder_detent_guard_ms_default,
            encoder_pulse_ms: globals.encoder_pulse_ms_default,
            cw_action: ActionEncoding::None,
            ccw_action: ActionEncoding::None,
        };

        Self {
            globals,
            panel_buttons: [btn; MAX_PANEL_BUTTONS],
            encoder_buttons: [btn; MAX_ENCODER_BUTTONS],
            encoders: [enc; MAX_ENCODERS],
        }
    }

    pub fn from_payload(p: &CrsPayloadV1) -> Self {
        let g = p.globals;

        let encoder_detent_guard_ms_default = g.encoder_detent_guard_ms_default as u16;

        let globals = Globals {
            scan_hz: g.scan_hz.max(1),
            pin_pull_default: decode_pull(g.pin_pull_default),
            active_low_default: g.active_low_default != 0,
            debounce_ms_default: g.debounce_ms_default,
            long_press_ms_default: g.long_press_ms_default,
            pulse_ms_button_default: g.pulse_ms_button_default,
            repeat_period_ms_default: g.repeat_period_ms_default,
            encoder_pulse_ms_default: g.encoder_pulse_ms_default,
            encoder_swap_ab_default: g.encoder_swap_ab_default != 0,
            encoder_detent_guard_ms_default,
            led_state_persist: g.led_state_persist != 0,
            led_state_on_boot: g.led_state_on_boot,
            led_level_persist: g.led_level_persist != 0,
            led_level_on_boot: g.led_level_on_boot,
            led_level_default: g.led_level_default,
            led_level_step: g.led_level_step,
            led_level_min: g.led_level_min,
            led_level_max: g.led_level_max,
            led_autosave_delay_ms: g.led_autosave_delay_ms,
        };

        let mut out = Self::disabled_fallback();
        out.globals = globals;

        // Buttons array order in CRS v1: encoder-buttons first (0..1), then panel (2..7).
        for i in 0..MAX_ENCODER_BUTTONS {
            let b = p.buttons[i];
            let pull = decode_pull(inherit_u8(b.pull, globals.pin_pull_default));
            let active_low = inherit_bool(b.active_low, globals.active_low_default);
            let debounce_ms = inherit_u16(b.debounce_ms, globals.debounce_ms_default);
            let long_press_ms = inherit_u16(b.long_press_ms, globals.long_press_ms_default);
            let pulse_ms = inherit_u16(b.pulse_ms, globals.pulse_ms_button_default);

            out.encoder_buttons[i] = ButtonCfg {
                enabled: b.enabled != 0,
                pull,
                active_low,
                debounce_ms,
                long_press_ms,
                pulse_ms,
                repeat_period_ms: b.repeat_period_ms,
                short_action: decode_action(b.short_action),
                long_action: decode_action(b.long_action),
            };
        }

        for i in 0..MAX_PANEL_BUTTONS {
            let b = p.buttons[MAX_ENCODER_BUTTONS + i];
            let pull = decode_pull(inherit_u8(b.pull, globals.pin_pull_default));
            let active_low = inherit_bool(b.active_low, globals.active_low_default);
            let debounce_ms = inherit_u16(b.debounce_ms, globals.debounce_ms_default);
            let long_press_ms = inherit_u16(b.long_press_ms, globals.long_press_ms_default);
            let pulse_ms = inherit_u16(b.pulse_ms, globals.pulse_ms_button_default);

            out.panel_buttons[i] = ButtonCfg {
                enabled: b.enabled != 0,
                pull,
                active_low,
                debounce_ms,
                long_press_ms,
                pulse_ms,
                repeat_period_ms: b.repeat_period_ms,
                short_action: decode_action(b.short_action),
                long_action: decode_action(b.long_action),
            };
        }

        for i in 0..MAX_ENCODERS {
            let e = p.encoders[i];

            let swap_ab = inherit_bool(e.swap_ab, globals.encoder_swap_ab_default);
            let detent_guard_ms =
                inherit_u16(e.detent_guard_ms, globals.encoder_detent_guard_ms_default);

            out.encoders[i] = EncoderCfg {
                enabled: e.enabled != 0,
                swap_ab,
                detent_guard_ms,
                encoder_pulse_ms: e.encoder_pulse_ms.max(1),
                cw_action: decode_action(e.cw_action),
                ccw_action: decode_action(e.ccw_action),
            };
        }

        out
    }
}
