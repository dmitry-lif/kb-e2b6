// crs-format/src/validate.rs
//
// Semantic validation of a CRS v1 payload (wire level). This is the single authoritative rule set
// shared by host tooling (`crs-pack`) and firmware (WebHID config writes).
//
// Rules mirror `crs_pack` TOML validation, expressed on the wire representation:
// - signed override fields use `-1` = inherit global default
// - disabled entries are ignored (tools encode them as all-zero; firmware never reads them)
//
// `validate_payload` is a `const fn` so firmware can prove its compiled-in defaults valid at
// compile time.

use crate::{ActionWireV1, ButtonEntryV1, CrsPayloadV1, EncoderEntryV1, GlobalDefaultsV1};
use crate::{MAX_BUTTONS_TOTAL, MAX_ENCODERS};

/// `ValidationError::index` value used for global (non-entry) fields.
pub const GLOBAL_INDEX: u8 = 0xFF;

/// Stable numeric codes (reported over WebHID; keep values stable).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationCode {
    ScanHz = 1,
    PullDefault = 2,
    ActiveLowDefault = 3,
    DebounceDefault = 4,
    LongPressDefault = 5,
    PulseDefault = 6,
    RepeatDefault = 7,
    EncoderPulseDefault = 8,
    SwapDefault = 9,
    DetentDefault = 10,
    LedStatePersist = 11,
    LedStateOnBoot = 12,
    LedLevelPersist = 13,
    LedLevelOnBoot = 14,
    LedLevelDefault = 15,
    LedLevelStep = 16,
    LedLevelMin = 17,
    LedLevelMax = 18,
    LedMinGreaterThanMax = 19,
    LedAutosaveDelay = 20,
    GlobalsReserved = 21,

    ButtonEnabled = 32,
    ButtonPull = 33,
    ButtonActiveLow = 34,
    ButtonDebounce = 35,
    ButtonLongPress = 36,
    ButtonPulse = 37,
    ButtonRepeat = 38,
    ButtonRepeatNeedsPulse = 39,
    ButtonNoAction = 40,
    ButtonReserved = 41,
    ButtonAction = 42,

    EncoderEnabled = 48,
    EncoderSwap = 49,
    EncoderDetent = 50,
    EncoderPulse = 51,
    EncoderNoAction = 52,
    EncoderReserved = 53,
    EncoderAction = 54,
}

/// A validation failure: which rule, and which entry.
///
/// `index` is the CRS array index (buttons: 0..1 encoder push, 2..7 panel; encoders: 0..1),
/// or [`GLOBAL_INDEX`] for global fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationError {
    pub code: ValidationCode,
    pub index: u8,
}

impl ValidationCode {
    pub const fn message(self) -> &'static str {
        use ValidationCode::*;
        match self {
            ScanHz => "scan_hz must be 1000, 2000 or 3000",
            PullDefault => "pin_pull_default must be none|up|down",
            ActiveLowDefault => "active_low_default must be 0/1",
            DebounceDefault => "debounce_ms_default must be 1..=50",
            LongPressDefault => "long_press_ms_default must be 200..=3000",
            PulseDefault => "pulse_ms_button_default must be 0..=500",
            RepeatDefault => "repeat_period_ms_default must be 0 or 20..=2000",
            EncoderPulseDefault => "encoder_pulse_ms_default must be 1..=200",
            SwapDefault => "encoder_swap_ab_default must be 0/1",
            DetentDefault => "encoder_detent_guard_ms_default must be 0..=10",
            LedStatePersist => "led_state_persist must be 0/1",
            LedStateOnBoot => "led_state_on_boot must be off|on|restore",
            LedLevelPersist => "led_level_persist must be 0/1",
            LedLevelOnBoot => "led_level_on_boot must be 0..=100",
            LedLevelDefault => "led_level_default must be 1..=100",
            LedLevelStep => "led_level_step must be 1..=50",
            LedLevelMin => "led_level_min must be 1..=100",
            LedLevelMax => "led_level_max must be 1..=100",
            LedMinGreaterThanMax => "led_level_min must be <= led_level_max",
            LedAutosaveDelay => "led_autosave_delay_ms must be 0..=60000",
            GlobalsReserved => "globals reserved bytes must be 0",
            ButtonEnabled => "button enabled must be 0/1",
            ButtonPull => "button pull must be inherit|none|up|down",
            ButtonActiveLow => "button active_low must be inherit|0|1",
            ButtonDebounce => "button debounce_ms must be inherit or 1..=50",
            ButtonLongPress => "button long_press_ms must be inherit or 200..=3000",
            ButtonPulse => "button pulse_ms must be inherit or 0..=500",
            ButtonRepeat => "button repeat_period_ms must be 0 or 20..=2000",
            ButtonRepeatNeedsPulse => "button repeat requires pulse_ms > 0",
            ButtonNoAction => "enabled button has no short or long action",
            ButtonReserved => "button reserved bytes must be 0",
            ButtonAction => "button action has an invalid tag or reserved byte",
            EncoderEnabled => "encoder enabled must be 0/1",
            EncoderSwap => "encoder swap_ab must be inherit|0|1",
            EncoderDetent => "encoder detent_guard_ms must be inherit or 0..=10",
            EncoderPulse => "encoder_pulse_ms must be 1..=200",
            EncoderNoAction => "enabled encoder has no cw or ccw action",
            EncoderReserved => "encoder reserved bytes must be 0",
            EncoderAction => "encoder action has an invalid tag or reserved byte",
        }
    }
}

impl core::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.index == GLOBAL_INDEX {
            write!(f, "{}", self.code.message())
        } else {
            write!(f, "{} (entry index {})", self.code.message(), self.index)
        }
    }
}

const fn fail(code: ValidationCode, index: u8) -> Result<(), ValidationError> {
    Err(ValidationError { code, index })
}

const fn in_u16(v: u16, lo: u16, hi: u16) -> bool {
    v >= lo && v <= hi
}

const fn in_u8(v: u8, lo: u8, hi: u8) -> bool {
    v >= lo && v <= hi
}

const fn inherit_or_i16(v: i16, lo: i16, hi: i16) -> bool {
    v == -1 || (v >= lo && v <= hi)
}

const fn action_ok(a: ActionWireV1) -> bool {
    a.tag <= 3 && a.c == 0
}

const fn validate_globals(g: GlobalDefaultsV1) -> Result<(), ValidationError> {
    use ValidationCode::*;
    let x = GLOBAL_INDEX;

    let scan_hz = g.scan_hz;
    if !(scan_hz == 1000 || scan_hz == 2000 || scan_hz == 3000) {
        return fail(ScanHz, x);
    }
    if g.pin_pull_default > 2 {
        return fail(PullDefault, x);
    }
    if g.active_low_default > 1 {
        return fail(ActiveLowDefault, x);
    }
    if !in_u16(g.debounce_ms_default, 1, 50) {
        return fail(DebounceDefault, x);
    }
    if !in_u16(g.long_press_ms_default, 200, 3000) {
        return fail(LongPressDefault, x);
    }
    if g.pulse_ms_button_default > 500 {
        return fail(PulseDefault, x);
    }
    let rep = g.repeat_period_ms_default;
    if rep != 0 && !in_u16(rep, 20, 2000) {
        return fail(RepeatDefault, x);
    }
    if !in_u16(g.encoder_pulse_ms_default, 1, 200) {
        return fail(EncoderPulseDefault, x);
    }
    if g.encoder_swap_ab_default > 1 {
        return fail(SwapDefault, x);
    }
    if g.encoder_detent_guard_ms_default > 10 {
        return fail(DetentDefault, x);
    }
    if g.led_state_persist > 1 {
        return fail(LedStatePersist, x);
    }
    if g.led_state_on_boot > 2 {
        return fail(LedStateOnBoot, x);
    }
    if g.led_level_persist > 1 {
        return fail(LedLevelPersist, x);
    }
    if g.led_level_on_boot > 100 {
        return fail(LedLevelOnBoot, x);
    }
    if !in_u8(g.led_level_default, 1, 100) {
        return fail(LedLevelDefault, x);
    }
    if !in_u8(g.led_level_step, 1, 50) {
        return fail(LedLevelStep, x);
    }
    if !in_u8(g.led_level_min, 1, 100) {
        return fail(LedLevelMin, x);
    }
    if !in_u8(g.led_level_max, 1, 100) {
        return fail(LedLevelMax, x);
    }
    if g.led_level_min > g.led_level_max {
        return fail(LedMinGreaterThanMax, x);
    }
    if g.led_autosave_delay_ms > 60000 {
        return fail(LedAutosaveDelay, x);
    }
    if g.reserved[0] != 0 || g.reserved[1] != 0 {
        return fail(GlobalsReserved, x);
    }
    Ok(())
}

const fn validate_button(
    b: ButtonEntryV1,
    g: GlobalDefaultsV1,
    idx: u8,
) -> Result<(), ValidationError> {
    use ValidationCode::*;

    if b.enabled == 0 {
        return Ok(());
    }
    if b.enabled != 1 {
        return fail(ButtonEnabled, idx);
    }
    if !(b.pull >= -1 && b.pull <= 2) {
        return fail(ButtonPull, idx);
    }
    if !(b.active_low >= -1 && b.active_low <= 1) {
        return fail(ButtonActiveLow, idx);
    }
    if !inherit_or_i16(b.debounce_ms, 1, 50) {
        return fail(ButtonDebounce, idx);
    }
    if !inherit_or_i16(b.long_press_ms, 200, 3000) {
        return fail(ButtonLongPress, idx);
    }
    let pulse = b.pulse_ms;
    if !inherit_or_i16(pulse, 0, 500) {
        return fail(ButtonPulse, idx);
    }
    let rep = b.repeat_period_ms;
    if rep != 0 && !in_u16(rep, 20, 2000) {
        return fail(ButtonRepeat, idx);
    }
    let pulse_eff = if pulse < 0 {
        g.pulse_ms_button_default
    } else {
        pulse as u16
    };
    if rep > 0 && pulse_eff == 0 {
        return fail(ButtonRepeatNeedsPulse, idx);
    }
    if b.reserved0 != 0
        || b.reserved1[0] != 0
        || b.reserved1[1] != 0
        || b.reserved2[0] != 0
        || b.reserved2[1] != 0
    {
        return fail(ButtonReserved, idx);
    }
    if !action_ok(b.short_action) || !action_ok(b.long_action) {
        return fail(ButtonAction, idx);
    }
    if b.short_action.tag == 0 && b.long_action.tag == 0 {
        return fail(ButtonNoAction, idx);
    }
    Ok(())
}

const fn validate_encoder(e: EncoderEntryV1, idx: u8) -> Result<(), ValidationError> {
    use ValidationCode::*;

    if e.enabled == 0 {
        return Ok(());
    }
    if e.enabled != 1 {
        return fail(EncoderEnabled, idx);
    }
    if !(e.swap_ab >= -1 && e.swap_ab <= 1) {
        return fail(EncoderSwap, idx);
    }
    if !inherit_or_i16(e.detent_guard_ms, 0, 10) {
        return fail(EncoderDetent, idx);
    }
    if !in_u16(e.encoder_pulse_ms, 1, 200) {
        return fail(EncoderPulse, idx);
    }
    if e.reserved0 != 0
        || e.reserved1[0] != 0
        || e.reserved1[1] != 0
        || e.reserved1[2] != 0
        || e.reserved1[3] != 0
    {
        return fail(EncoderReserved, idx);
    }
    if !action_ok(e.cw_action) || !action_ok(e.ccw_action) {
        return fail(EncoderAction, idx);
    }
    if e.cw_action.tag == 0 && e.ccw_action.tag == 0 {
        return fail(EncoderNoAction, idx);
    }
    Ok(())
}

/// Validate every semantic rule of a CRS v1 payload.
///
/// Header checks (magic/schema/len/compat/CRC) are separate: see `parse_and_validate_with_compat`.
pub const fn validate_payload(p: &CrsPayloadV1) -> Result<(), ValidationError> {
    let g = p.globals;
    if let Err(e) = validate_globals(g) {
        return Err(e);
    }

    let mut i = 0;
    while i < MAX_BUTTONS_TOTAL {
        let b = p.buttons[i];
        if let Err(e) = validate_button(b, g, i as u8) {
            return Err(e);
        }
        i += 1;
    }

    let mut i = 0;
    while i < MAX_ENCODERS {
        let e = p.encoders[i];
        if let Err(err) = validate_encoder(e, i as u8) {
            return Err(err);
        }
        i += 1;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_zero_payload;

    fn cc(usage: u16) -> ActionWireV1 {
        ActionWireV1 {
            tag: 2,
            a: (usage & 0xFF) as u8,
            b: (usage >> 8) as u8,
            c: 0,
        }
    }

    fn valid_payload() -> CrsPayloadV1 {
        let mut p = new_zero_payload();
        p.globals.scan_hz = 1000;
        p.globals.pin_pull_default = 1;
        p.globals.active_low_default = 1;
        p.globals.debounce_ms_default = 5;
        p.globals.long_press_ms_default = 600;
        p.globals.pulse_ms_button_default = 50;
        p.globals.encoder_pulse_ms_default = 30;
        p.globals.encoder_detent_guard_ms_default = 2;
        p.globals.led_level_default = 30;
        p.globals.led_level_step = 5;
        p.globals.led_level_min = 10;
        p.globals.led_level_max = 100;
        p.buttons[2].enabled = 1;
        p.buttons[2].pull = -1;
        p.buttons[2].active_low = -1;
        p.buttons[2].debounce_ms = -1;
        p.buttons[2].long_press_ms = -1;
        p.buttons[2].pulse_ms = -1;
        p.buttons[2].short_action = cc(0x00CD);
        p
    }

    #[test]
    fn accepts_valid_payload() {
        assert_eq!(validate_payload(&valid_payload()), Ok(()));
    }

    #[test]
    fn rejects_bad_scan_hz() {
        let mut p = valid_payload();
        p.globals.scan_hz = 1500;
        assert_eq!(
            validate_payload(&p).unwrap_err().code,
            ValidationCode::ScanHz
        );
    }

    #[test]
    fn rejects_repeat_with_tracked_release() {
        let mut p = valid_payload();
        p.buttons[2].pulse_ms = 0;
        p.buttons[2].repeat_period_ms = 70;
        let e = validate_payload(&p).unwrap_err();
        assert_eq!(e.code, ValidationCode::ButtonRepeatNeedsPulse);
        assert_eq!(e.index, 2);
    }

    #[test]
    fn rejects_enabled_button_without_action() {
        let mut p = valid_payload();
        p.buttons[2].short_action = ActionWireV1 {
            tag: 0,
            a: 0,
            b: 0,
            c: 0,
        };
        assert_eq!(
            validate_payload(&p).unwrap_err().code,
            ValidationCode::ButtonNoAction
        );
    }

    #[test]
    fn ignores_disabled_entries() {
        let mut p = valid_payload();
        p.buttons[5].enabled = 0;
        p.buttons[5].repeat_period_ms = 9999;
        assert_eq!(validate_payload(&p), Ok(()));
    }

    #[test]
    fn rejects_led_min_above_max() {
        let mut p = valid_payload();
        p.globals.led_level_min = 60;
        p.globals.led_level_max = 50;
        assert_eq!(
            validate_payload(&p).unwrap_err().code,
            ValidationCode::LedMinGreaterThanMax
        );
    }
}
