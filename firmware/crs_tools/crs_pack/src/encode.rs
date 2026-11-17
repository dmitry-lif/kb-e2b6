use crate::toml_model::*;
use crate::util::{parse_hex_u32, PackInput};
use anyhow::{bail, Context, Result};
use crs_format::*;
use std::collections::HashSet;

fn bail_range<T: core::fmt::Display>(field: &str, v: T, min: T, max: T) -> anyhow::Error {
    anyhow::anyhow!("{field} out of range: {v} (expected {min}..={max})")
}

fn pull_str_to_u8(s: &str) -> Result<u8> {
    match s {
        "none" => Ok(0),
        "up" => Ok(1),
        "down" => Ok(2),
        _ => bail!("invalid pull '{s}', expected none|up|down"),
    }
}

fn bool_to_u8(v: bool) -> u8 {
    v as u8
}

fn opt_inherit_i8_bool(v: Option<bool>) -> i8 {
    match v {
        None => -1,
        Some(false) => 0,
        Some(true) => 1,
    }
}

fn opt_inherit_i16_u16_checked(field: &str, v: Option<u16>) -> Result<i16> {
    match v {
        None => Ok(-1),
        Some(x) => {
            if x > i16::MAX as u16 {
                bail!("{field} out of range: {x} (must be <= {})", i16::MAX);
            }
            Ok(x as i16)
        }
    }
}

fn opt_inherit_i8_pull(v: &Option<String>) -> Result<i8> {
    match v {
        None => Ok(-1),
        Some(s) => Ok(pull_str_to_u8(s)? as i8),
    }
}

fn validate_action_exactly_one(a: &TomlAction) -> Result<()> {
    let mut n = 0;
    if a.none.is_some() {
        n += 1;
    }
    if a.kbd.is_some() {
        n += 1;
    }
    if a.cc.is_some() {
        n += 1;
    }
    if a.internal.is_some() {
        n += 1;
    }
    if n != 1 {
        bail!("action must be exactly one of: none|kbd|cc|internal");
    }
    Ok(())
}

fn action_to_wire(a: &TomlAction) -> Result<ActionWireV1> {
    validate_action_exactly_one(a)?;

    if a.none.is_some() {
        return Ok(ActionWireV1 {
            tag: 0,
            a: 0,
            b: 0,
            c: 0,
        });
    }
    if let Some(k) = &a.kbd {
        if k.mods > 0xFF {
            bail!("kbd.mods out of range: {:#X} (must fit u8)", k.mods);
        }
        if k.usage > 0xFF {
            bail!("kbd.usage out of range: {:#X} (must fit u8)", k.usage);
        }
        return Ok(ActionWireV1 {
            tag: 1,
            a: k.mods as u8,
            b: k.usage as u8,
            c: 0,
        });
    }
    if let Some(c) = &a.cc {
        if c.usage > 0xFFFF {
            bail!("cc.usage out of range: {:#X} (must fit u16)", c.usage);
        }
        let u = c.usage as u16;
        return Ok(ActionWireV1 {
            tag: 2,
            a: (u & 0xFF) as u8,
            b: (u >> 8) as u8,
            c: 0,
        });
    }
    if let Some(i) = &a.internal {
        if i.id > 0xFF {
            bail!("internal.id out of range: {:#X} (must fit u8)", i.id);
        }
        return Ok(ActionWireV1 {
            tag: 3,
            a: i.id as u8,
            b: 0,
            c: 0,
        });
    }

    unreachable!()
}

fn parse_toml_root(toml_bytes: &[u8]) -> Result<TomlRoot> {
    let s = std::str::from_utf8(toml_bytes).context("TOML must be UTF-8")?;
    let root: TomlRoot = toml::from_str(s).context("parse TOML")?;
    Ok(root)
}

fn build_payload(root: &TomlRoot) -> Result<CrsPayloadV1> {
    if root.toml.version != 1 {
        bail!("[toml].version must be 1");
    }
    if root.target.crs_schema_version != 1 {
        bail!("[target].crs_schema_version must be 1");
    }

    // ---------------- globals validation ----------------
    match root.global.scan_hz {
        1000 | 2000 | 3000 => {}
        other => bail!("[global].scan_hz must be one of 1000|2000|3000 (got {other})"),
    }

    let pin_pull_default = pull_str_to_u8(&root.global.pin_pull_default)?;

    if !(1..=50).contains(&root.global.debounce_ms_default) {
        return Err(bail_range(
            "[global].debounce_ms_default",
            root.global.debounce_ms_default,
            1u16,
            50u16,
        ));
    }
    if !(200..=3000).contains(&root.global.long_press_ms_default) {
        return Err(bail_range(
            "[global].long_press_ms_default",
            root.global.long_press_ms_default,
            200u16,
            3000u16,
        ));
    }
    if root.global.pulse_ms_button_default > 500 {
        bail!(
            "[global].pulse_ms_button_default out of range: {} (expected 0..=500)",
            root.global.pulse_ms_button_default
        );
    }

    let repeat_def = root.global.repeat_period_ms_default;
    if repeat_def != 0 && !(20..=2000).contains(&repeat_def) {
        bail!("[global].repeat_period_ms_default must be 0 or 20..=2000 (got {repeat_def})");
    }

    let enc_pulse_def = root.global.encoder_pulse_ms_default;
    if !(1..=200).contains(&enc_pulse_def) {
        bail!("[global].encoder_pulse_ms_default out of range: {enc_pulse_def} (expected 1..=200)");
    }
    if root.global.encoder_detent_guard_ms_default > 10 {
        bail!(
            "[global].encoder_detent_guard_ms_default out of range: {} (expected 0..=10)",
            root.global.encoder_detent_guard_ms_default
        );
    }

    let led_state_on_boot = match root.global.led_state_on_boot.as_str() {
        "off" => 0u8,
        "on" => 1u8,
        "restore" => 2u8,
        other => bail!("[global].led_state_on_boot invalid '{other}' (expected off|on|restore)"),
    };
    if root.global.led_level_on_boot > 100 {
        bail!(
            "[global].led_level_on_boot out of range: {} (expected 0..=100)",
            root.global.led_level_on_boot
        );
    }
    if !(1..=100).contains(&root.global.led_level_default) {
        bail!(
            "[global].led_level_default out of range: {} (expected 1..=100)",
            root.global.led_level_default
        );
    }
    if !(1..=50).contains(&root.global.led_level_step) {
        bail!(
            "[global].led_level_step out of range: {} (expected 1..=50)",
            root.global.led_level_step
        );
    }
    if !(1..=100).contains(&root.global.led_level_min) {
        bail!(
            "[global].led_level_min out of range: {} (expected 1..=100)",
            root.global.led_level_min
        );
    }
    if !(1..=100).contains(&root.global.led_level_max) {
        bail!(
            "[global].led_level_max out of range: {} (expected 1..=100)",
            root.global.led_level_max
        );
    }
    if root.global.led_level_min > root.global.led_level_max {
        bail!(
            "[global].led_level_min ({}) must be <= [global].led_level_max ({})",
            root.global.led_level_min,
            root.global.led_level_max
        );
    }
    if root.global.led_autosave_delay_ms > 60000 {
        bail!(
            "[global].led_autosave_delay_ms out of range: {} (expected 0..=60000)",
            root.global.led_autosave_delay_ms
        );
    }

    let globals = GlobalDefaultsV1 {
        scan_hz: root.global.scan_hz,
        pin_pull_default,
        active_low_default: bool_to_u8(root.global.active_low_default),
        debounce_ms_default: root.global.debounce_ms_default,
        long_press_ms_default: root.global.long_press_ms_default,
        pulse_ms_button_default: root.global.pulse_ms_button_default,
        repeat_period_ms_default: root.global.repeat_period_ms_default,
        encoder_pulse_ms_default: root.global.encoder_pulse_ms_default,
        encoder_swap_ab_default: bool_to_u8(root.global.encoder_swap_ab_default),
        encoder_detent_guard_ms_default: root.global.encoder_detent_guard_ms_default,
        led_state_persist: bool_to_u8(root.global.led_state_persist),
        led_state_on_boot,
        led_level_persist: bool_to_u8(root.global.led_level_persist),
        led_level_on_boot: root.global.led_level_on_boot,
        led_level_default: root.global.led_level_default,
        led_level_step: root.global.led_level_step,
        led_level_min: root.global.led_level_min,
        led_level_max: root.global.led_level_max,
        led_autosave_delay_ms: root.global.led_autosave_delay_ms,
        reserved: [0, 0],
    };

    let mut payload = new_zero_payload();
    payload.globals = globals;

    let zero_action = ActionWireV1 {
        tag: 0,
        a: 0,
        b: 0,
        c: 0,
    };

    let zero_button = ButtonEntryV1 {
        enabled: 0,
        pull: 0,
        active_low: 0,
        debounce_ms: 0,
        long_press_ms: 0,
        pulse_ms: 0,
        repeat_period_ms: 0,
        reserved0: 0,
        short_action: zero_action,
        long_action: zero_action,
        reserved1: [0, 0],
        reserved2: [0, 0],
    };

    let zero_encoder = EncoderEntryV1 {
        enabled: 0,
        swap_ab: 0,
        detent_guard_ms: 0,
        encoder_pulse_ms: 0,
        reserved0: 0,
        cw_action: zero_action,
        ccw_action: zero_action,
        reserved1: [0, 0, 0, 0],
    };

    payload.buttons = [zero_button; MAX_BUTTONS_TOTAL];
    payload.encoders = [zero_encoder; MAX_ENCODERS];

    // ---------------- buttons ----------------
    let mut seen_panel = HashSet::<u8>::new();
    let mut seen_encbtn = HashSet::<u8>::new();

    for b in &root.buttons {
        let idx: usize = match b.kind.as_str() {
            "panel" => {
                if b.id < 1 || b.id > MAX_PANEL_BUTTONS as u8 {
                    bail!(
                        "panel button id {} out of range 1..{}",
                        b.id,
                        MAX_PANEL_BUTTONS
                    );
                }
                if !seen_panel.insert(b.id) {
                    bail!("duplicate panel button id {}", b.id);
                }
                MAX_ENCODER_BUTTONS + (b.id as usize - 1)
            }
            "encoder" => {
                if b.id < 1 || b.id > MAX_ENCODER_BUTTONS as u8 {
                    bail!(
                        "encoder push button id {} out of range 1..{}",
                        b.id,
                        MAX_ENCODER_BUTTONS
                    );
                }
                if !seen_encbtn.insert(b.id) {
                    bail!("duplicate encoder push button id {}", b.id);
                }
                (b.id as usize) - 1
            }
            other => bail!("buttons.kind invalid '{other}' (expected panel|encoder)"),
        };

        if !b.enabled {
            payload.buttons[idx] = zero_button;
            continue;
        }

        let short_action = action_to_wire(&b.short_action)?;
        let long_action = match &b.long_action {
            None => zero_action,
            Some(x) => action_to_wire(x)?,
        };

        if short_action.tag == 0 && long_action.tag == 0 {
            bail!(
                "buttons(kind={}, id={}) enabled=true but both short_action and long_action are none",
                b.kind,
                b.id
            );
        }

        let repeat_ms = b
            .repeat_period_ms
            .unwrap_or(root.global.repeat_period_ms_default);
        if repeat_ms != 0 && !(20..=2000).contains(&repeat_ms) {
            bail!(
                "buttons(kind={}, id={}).repeat_period_ms must be 0 or 20..=2000 (got {repeat_ms})",
                b.kind,
                b.id
            );
        }

        let pulse_eff = b.pulse_ms.unwrap_or(root.global.pulse_ms_button_default);
        if pulse_eff > 500 {
            bail!(
                "buttons(kind={}, id={}).pulse_ms out of range: {} (expected 0..=500)",
                b.kind,
                b.id,
                pulse_eff
            );
        }

        // Repeat emits press->release pulses and depends on pulse_ms > 0.
        if repeat_ms > 0 && pulse_eff == 0 {
            bail!(
                "buttons(kind={}, id={}) repeat_period_ms>0 requires pulse_ms>0 (tracked-release not allowed with repeat)",
                b.kind,
                b.id
            );
        }

        // Basic ranges for timing overrides when provided.
        if let Some(db) = b.debounce_ms {
            if !(1..=50).contains(&db) {
                bail!(
                    "buttons(kind={}, id={}).debounce_ms out of range: {db} (expected 1..=50)",
                    b.kind,
                    b.id
                );
            }
        }
        if let Some(lm) = b.long_press_ms {
            if !(200..=3000).contains(&lm) {
                bail!("buttons(kind={}, id={}).long_press_ms out of range: {lm} (expected 200..=3000)", b.kind, b.id);
            }
        }

        payload.buttons[idx] = ButtonEntryV1 {
            enabled: 1,
            pull: opt_inherit_i8_pull(&b.pin_pull)?,
            active_low: opt_inherit_i8_bool(b.active_low),
            debounce_ms: opt_inherit_i16_u16_checked("buttons.debounce_ms", b.debounce_ms)?,
            long_press_ms: opt_inherit_i16_u16_checked("buttons.long_press_ms", b.long_press_ms)?,
            pulse_ms: opt_inherit_i16_u16_checked("buttons.pulse_ms", b.pulse_ms)?,
            repeat_period_ms: repeat_ms,
            reserved0: 0,
            short_action,
            long_action,
            reserved1: [0, 0],
            reserved2: [0, 0],
        };
    }

    // ---------------- encoders (rotation only) ----------------
    let mut enc_seen = HashSet::<u8>::new();
    for e in &root.encoders {
        if e.id < 1 || e.id > MAX_ENCODERS as u8 {
            bail!("encoder id {} out of range 1..{}", e.id, MAX_ENCODERS);
        }
        if !enc_seen.insert(e.id) {
            bail!("duplicate encoder id {}", e.id);
        }

        if !e.enabled {
            payload.encoders[(e.id as usize) - 1] = zero_encoder;
            continue;
        }

        let cw_action = action_to_wire(&e.cw_action)?;
        let ccw_action = action_to_wire(&e.ccw_action)?;
        if cw_action.tag == 0 && ccw_action.tag == 0 {
            bail!(
                "encoders(id={}) enabled=true but both cw_action and ccw_action are none",
                e.id
            );
        }

        if let Some(detent) = e.detent_guard_ms {
            if detent > 10 {
                bail!(
                    "encoders(id={}).detent_guard_ms out of range: {detent} (expected 0..=10)",
                    e.id
                );
            }
        }

        let pulse_ms = e
            .encoder_pulse_ms
            .unwrap_or(root.global.encoder_pulse_ms_default);
        if !(1..=200).contains(&pulse_ms) {
            bail!(
                "encoders(id={}).encoder_pulse_ms out of range: {pulse_ms} (expected 1..=200)",
                e.id
            );
        }

        payload.encoders[(e.id as usize) - 1] = EncoderEntryV1 {
            enabled: 1,
            swap_ab: opt_inherit_i8_bool(e.swap_ab),
            detent_guard_ms: opt_inherit_i16_u16_checked(
                "encoders.detent_guard_ms",
                e.detent_guard_ms,
            )?,
            encoder_pulse_ms: pulse_ms,
            reserved0: 0,
            cw_action,
            ccw_action,
            reserved1: [0, 0, 0, 0],
        };
    }

    // Belt and braces: the shared wire-level validator (also run by firmware on WebHID writes)
    // must accept everything this tool packs.
    validate_payload(&payload).map_err(|e| anyhow::anyhow!("payload validation failed: {e}"))?;

    Ok(payload)
}

pub fn derive_build_id_from_toml_bytes(toml_bytes: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(toml_bytes);
    h.finalize()
}

/// Build payload and parse pack-relevant header inputs from TOML bytes.
pub fn build_payload_from_toml_bytes(toml_bytes: &[u8]) -> Result<(PackInput, CrsPayloadV1)> {
    let root = parse_toml_root(toml_bytes)?;
    let fw_compat_id = parse_hex_u32(&root.target.fw_compat_id)?;
    let payload = build_payload(&root)?;
    Ok((PackInput { fw_compat_id }, payload))
}

/// Pack TOML bytes into a full CRS binary (header+payload).
pub fn pack_toml_bytes(toml_bytes: &[u8], build_id: Option<u32>) -> Result<[u8; TOTAL_LEN]> {
    let (inputs, payload) = build_payload_from_toml_bytes(toml_bytes)?;
    let crc = crc32_payload(payload_bytes(&payload));
    let bid = build_id.unwrap_or_else(|| derive_build_id_from_toml_bytes(toml_bytes));
    let hdr = make_header(inputs.fw_compat_id, crc, bid);
    Ok(serialize(&hdr, &payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_panel_button_ids() {
        let toml = br#"
[toml]
version = 1

[target]
fw_compat_id = "0x20260216"
crs_schema_version = 1

[global]
scan_hz = 1000
pin_pull_default = "up"
active_low_default = true
debounce_ms_default = 5
long_press_ms_default = 600
pulse_ms_button_default = 50
repeat_period_ms_default = 0
encoder_pulse_ms_default = 30
encoder_swap_ab_default = false
encoder_detent_guard_ms_default = 2
led_state_persist = true
led_state_on_boot = "restore"
led_level_persist = true
led_level_on_boot = 0
led_level_default = 30
led_level_step = 5
led_level_min = 10
led_level_max = 100
led_autosave_delay_ms = 1500

[[buttons]]
kind = "panel"
id = 1
enabled = true
short_action = { cc = { usage = 0x00CD } }

[[buttons]]
kind = "panel"
id = 1
enabled = true
short_action = { cc = { usage = 0x00E2 } }
"#;

        let err = build_payload_from_toml_bytes(toml)
            .err()
            .expect("expected error")
            .to_string();
        assert!(err.contains("duplicate panel button id"), "{err}");
    }

    #[test]
    fn rejects_repeat_with_tracked_release() {
        let toml = br#"
[toml]
version = 1

[target]
fw_compat_id = "0x20260216"
crs_schema_version = 1

[global]
scan_hz = 1000
pin_pull_default = "up"
active_low_default = true
debounce_ms_default = 5
long_press_ms_default = 600
pulse_ms_button_default = 0
repeat_period_ms_default = 0
encoder_pulse_ms_default = 30
encoder_swap_ab_default = false
encoder_detent_guard_ms_default = 2
led_state_persist = true
led_state_on_boot = "restore"
led_level_persist = true
led_level_on_boot = 0
led_level_default = 30
led_level_step = 5
led_level_min = 10
led_level_max = 100
led_autosave_delay_ms = 1500

[[buttons]]
kind = "panel"
id = 1
enabled = true
pulse_ms = 0
repeat_period_ms = 70
short_action = { cc = { usage = 0x00CD } }
"#;

        let err = build_payload_from_toml_bytes(toml)
            .err()
            .expect("expected error")
            .to_string();
        assert!(
            err.contains("repeat_period_ms>0 requires pulse_ms>0"),
            "{err}"
        );
    }
}
