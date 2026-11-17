use crate::util::parse_hex_u32;
use anyhow::{anyhow, bail, Context, Result};
use crs_format::*;
use std::{fs, io::Write, path::Path, ptr};

// ---------------- packed field safe reads ----------------

#[inline]
fn read_u16_unaligned(p: *const u16) -> u16 {
    unsafe { ptr::read_unaligned(p) }
}

#[inline]
fn read_i16_unaligned(p: *const i16) -> i16 {
    unsafe { ptr::read_unaligned(p) }
}

fn actionwire_to_toml(w: ActionWireV1) -> String {
    match w.tag {
        0 => "{ none = {} }".to_string(),
        1 => format!(
            "{{ kbd = {{ mods = 0x{:02X}, usage = 0x{:02X} }} }}",
            w.a, w.b
        ),
        2 => {
            let u = (w.a as u16) | ((w.b as u16) << 8);
            format!("{{ cc = {{ usage = 0x{:04X} }} }}", u)
        }
        3 => format!("{{ internal = {{ id = 0x{:02X} }} }}", w.a),
        _ => format!("{{ none = {{}} }} # invalid tag {}", w.tag),
    }
}

fn pull_u8_to_str(v: u8) -> Result<&'static str> {
    match v {
        0 => Ok("none"),
        1 => Ok("up"),
        2 => Ok("down"),
        _ => bail!("invalid pull value in CRS: {}", v),
    }
}

fn dump_pull_i8_as_str(v: i8) -> Result<Option<&'static str>> {
    match v {
        -1 => Ok(None),
        0 => Ok(Some("none")),
        1 => Ok(Some("up")),
        2 => Ok(Some("down")),
        _ => bail!("invalid pull override in CRS: {}", v),
    }
}

fn dump_bool_i8(v: i8) -> Result<Option<bool>> {
    match v {
        -1 => Ok(None),
        0 => Ok(Some(false)),
        1 => Ok(Some(true)),
        _ => bail!("invalid bool override in CRS: {}", v),
    }
}

fn dump_i16(v: i16) -> Option<i16> {
    if v == -1 {
        None
    } else {
        Some(v)
    }
}

fn led_state_on_boot_u8_to_str(v: u8) -> Result<&'static str> {
    match v {
        0 => Ok("off"),
        1 => Ok("on"),
        2 => Ok("restore"),
        _ => bail!("invalid led_state_on_boot value in CRS: {}", v),
    }
}

pub fn dump_crs_file(path: &Path, as_toml: bool, fw_compat_id: Option<&str>) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    dump_crs_bytes(&bytes, as_toml, fw_compat_id)
}

pub fn dump_crs_bytes(bytes: &[u8], as_toml: bool, fw_compat_id: Option<&str>) -> Result<String> {
    let mut out = Vec::<u8>::new();
    dump_crs_bytes_to_writer(bytes, as_toml, fw_compat_id, &mut out)?;
    Ok(String::from_utf8(out).map_err(|_| anyhow!("dump output not utf8"))?)
}

fn dump_crs_bytes_to_writer(
    bytes: &[u8],
    as_toml: bool,
    fw_compat_id: Option<&str>,
    mut w: impl Write,
) -> Result<()> {
    let (hdr, payload) = parse_and_validate_basic(bytes).map_err(|e| anyhow!("{e}"))?;
    let hv: HeaderView = hdr.into();
    let gv: GlobalsView = payload.globals.into();

    if let Some(s) = fw_compat_id {
        let expected = parse_hex_u32(s)?;
        if hv.fw_compat_id != expected {
            bail!(
                "fw_compat_id mismatch: file 0x{:08X}, expected 0x{:08X}",
                hv.fw_compat_id,
                expected
            );
        }
    }

    if as_toml {
        writeln!(w, "[toml]")?;
        writeln!(w, "version = 1\n")?;

        writeln!(w, "[target]")?;
        writeln!(w, "fw_compat_id = \"0x{:08X}\"", hv.fw_compat_id)?;
        writeln!(w, "crs_schema_version = 1\n")?;

        writeln!(w, "[global]")?;
        writeln!(w, "scan_hz = {}", gv.scan_hz)?;
        writeln!(
            w,
            "pin_pull_default = \"{}\"",
            pull_u8_to_str(gv.pin_pull_default)?
        )?;
        writeln!(w, "active_low_default = {}", gv.active_low_default != 0)?;
        writeln!(w, "debounce_ms_default = {}", gv.debounce_ms_default)?;
        writeln!(w, "long_press_ms_default = {}", gv.long_press_ms_default)?;
        writeln!(
            w,
            "pulse_ms_button_default = {}",
            gv.pulse_ms_button_default
        )?;
        writeln!(
            w,
            "repeat_period_ms_default = {}",
            gv.repeat_period_ms_default
        )?;
        writeln!(
            w,
            "encoder_pulse_ms_default = {}",
            gv.encoder_pulse_ms_default
        )?;
        writeln!(
            w,
            "encoder_swap_ab_default = {}",
            gv.encoder_swap_ab_default != 0
        )?;
        writeln!(
            w,
            "encoder_detent_guard_ms_default = {}",
            gv.encoder_detent_guard_ms_default
        )?;
        writeln!(w, "led_state_persist = {}", gv.led_state_persist != 0)?;
        writeln!(
            w,
            "led_state_on_boot = \"{}\"",
            led_state_on_boot_u8_to_str(gv.led_state_on_boot)?
        )?;
        writeln!(w, "led_level_persist = {}", gv.led_level_persist != 0)?;
        writeln!(w, "led_level_on_boot = {}", gv.led_level_on_boot)?;
        writeln!(w, "led_level_default = {}", gv.led_level_default)?;
        writeln!(w, "led_level_step = {}", gv.led_level_step)?;
        writeln!(w, "led_level_min = {}", gv.led_level_min)?;
        writeln!(w, "led_level_max = {}", gv.led_level_max)?;
        writeln!(w, "led_autosave_delay_ms = {}", gv.led_autosave_delay_ms)?;
        writeln!(w)?;

        for (i, b) in payload.buttons.iter().enumerate() {
            if b.enabled == 0 {
                continue;
            }

            let rp = read_u16_unaligned(ptr::addr_of!(b.repeat_period_ms));
            let db = read_i16_unaligned(ptr::addr_of!(b.debounce_ms));
            let lm = read_i16_unaligned(ptr::addr_of!(b.long_press_ms));
            let pm = read_i16_unaligned(ptr::addr_of!(b.pulse_ms));

            let (kind, id) = if i < MAX_ENCODER_BUTTONS {
                ("encoder", (i + 1) as u8)
            } else {
                ("panel", (i - MAX_ENCODER_BUTTONS + 1) as u8)
            };

            writeln!(w, "[[buttons]]")?;
            writeln!(w, "kind = \"{}\"", kind)?;
            writeln!(w, "id = {}", id)?;
            writeln!(w, "enabled = true")?;

            if let Some(ps) = dump_pull_i8_as_str(b.pull)? {
                writeln!(w, "pin_pull = \"{}\"", ps)?;
            }
            if let Some(v) = dump_bool_i8(b.active_low)? {
                writeln!(w, "active_low = {}", v)?;
            }
            if let Some(v) = dump_i16(db) {
                writeln!(w, "debounce_ms = {}", v)?;
            }
            if let Some(v) = dump_i16(lm) {
                writeln!(w, "long_press_ms = {}", v)?;
            }
            if let Some(v) = dump_i16(pm) {
                writeln!(w, "pulse_ms = {}", v)?;
            }

            writeln!(w, "short_action = {}", actionwire_to_toml(b.short_action))?;
            if b.long_action.tag != 0 {
                writeln!(w, "long_action  = {}", actionwire_to_toml(b.long_action))?;
            }
            if rp != gv.repeat_period_ms_default {
                writeln!(w, "repeat_period_ms = {}", rp)?;
            }
            writeln!(w)?;
        }

        for (i, e) in payload.encoders.iter().enumerate() {
            if e.enabled == 0 {
                continue;
            }

            let det = read_i16_unaligned(ptr::addr_of!(e.detent_guard_ms));

            writeln!(w, "[[encoders]]")?;
            writeln!(w, "id = {}", i + 1)?;
            writeln!(w, "enabled = true")?;

            if let Some(v) = dump_bool_i8(e.swap_ab)? {
                writeln!(w, "swap_ab = {}", v)?;
            }
            if let Some(v) = dump_i16(det) {
                writeln!(w, "detent_guard_ms = {}", v)?;
            }
            let encoder_pulse_ms = read_u16_unaligned(ptr::addr_of!(e.encoder_pulse_ms));
            if encoder_pulse_ms != gv.encoder_pulse_ms_default {
                writeln!(w, "encoder_pulse_ms = {}", encoder_pulse_ms)?;
            }

            writeln!(w, "cw_action  = {}", actionwire_to_toml(e.cw_action))?;
            writeln!(w, "ccw_action = {}", actionwire_to_toml(e.ccw_action))?;
            writeln!(w)?;
        }

        return Ok(());
    }

    writeln!(w, "CRS v1")?;
    writeln!(w, "  schema: {}", hv.crs_schema_version)?;
    writeln!(w, "  fw_compat_id: 0x{:08X}", hv.fw_compat_id)?;
    writeln!(w, "  len: {}", hv.len)?;
    writeln!(w, "  crc32(payload): 0x{:08X} (ok)", hv.crc32)?;
    writeln!(w, "  build_id: 0x{:08X}", hv.build_id)?;
    writeln!(w)?;

    writeln!(w, "Globals:")?;
    writeln!(w, "  scan_hz: {}", gv.scan_hz)?;
    writeln!(
        w,
        "  pin_pull_default: {}",
        pull_u8_to_str(gv.pin_pull_default)?
    )?;
    writeln!(w, "  active_low_default: {}", gv.active_low_default != 0)?;
    writeln!(w, "  debounce_ms_default: {}", gv.debounce_ms_default)?;
    writeln!(w, "  long_press_ms_default: {}", gv.long_press_ms_default)?;
    writeln!(
        w,
        "  pulse_ms_button_default: {}",
        gv.pulse_ms_button_default
    )?;
    writeln!(
        w,
        "  repeat_period_ms_default: {}",
        gv.repeat_period_ms_default
    )?;
    writeln!(
        w,
        "  encoder_pulse_ms_default: {}",
        gv.encoder_pulse_ms_default
    )?;
    writeln!(
        w,
        "  encoder_swap_ab_default: {}",
        gv.encoder_swap_ab_default != 0
    )?;
    writeln!(
        w,
        "  encoder_detent_guard_ms_default: {}",
        gv.encoder_detent_guard_ms_default
    )?;
    writeln!(w, "  led_state_persist: {}", gv.led_state_persist != 0)?;
    writeln!(
        w,
        "  led_state_on_boot: {}",
        led_state_on_boot_u8_to_str(gv.led_state_on_boot)?
    )?;
    writeln!(w, "  led_level_persist: {}", gv.led_level_persist != 0)?;
    writeln!(w, "  led_level_on_boot: {}", gv.led_level_on_boot)?;
    writeln!(w, "  led_level_default: {}", gv.led_level_default)?;
    writeln!(w, "  led_level_step: {}", gv.led_level_step)?;
    writeln!(w, "  led_level_min: {}", gv.led_level_min)?;
    writeln!(w, "  led_level_max: {}", gv.led_level_max)?;
    writeln!(w, "  led_autosave_delay_ms: {}", gv.led_autosave_delay_ms)?;
    writeln!(w)?;

    writeln!(w, "Buttons (enabled only):")?;
    for (i, b) in payload.buttons.iter().enumerate() {
        if b.enabled == 0 {
            continue;
        }
        let rp = read_u16_unaligned(ptr::addr_of!(b.repeat_period_ms));
        let db = read_i16_unaligned(ptr::addr_of!(b.debounce_ms));
        let lm = read_i16_unaligned(ptr::addr_of!(b.long_press_ms));
        let pm = read_i16_unaligned(ptr::addr_of!(b.pulse_ms));

        let (kind, id) = if i < MAX_ENCODER_BUTTONS {
            ("encoder", (i + 1) as u8)
        } else {
            ("panel", (i - MAX_ENCODER_BUTTONS + 1) as u8)
        };

        writeln!(w, "  [{}] kind={} id={}", i, kind, id)?;
        writeln!(w, "    pull: {}", b.pull)?;
        writeln!(w, "    active_low: {}", b.active_low)?;
        writeln!(w, "    debounce_ms: {}", db)?;
        writeln!(w, "    long_press_ms: {}", lm)?;
        writeln!(w, "    pulse_ms: {}", pm)?;
        writeln!(w, "    repeat_period_ms: {}", rp)?;
        writeln!(
            w,
            "    short_action: {}",
            actionwire_to_toml(b.short_action)
        )?;
        writeln!(w, "    long_action:  {}", actionwire_to_toml(b.long_action))?;
    }
    writeln!(w)?;

    writeln!(w, "Encoders (enabled only):")?;
    for (i, e) in payload.encoders.iter().enumerate() {
        if e.enabled == 0 {
            continue;
        }
        let det = read_i16_unaligned(ptr::addr_of!(e.detent_guard_ms));
        let encoder_pulse_ms = read_u16_unaligned(ptr::addr_of!(e.encoder_pulse_ms));

        writeln!(w, "  [{}]", i)?;
        writeln!(w, "    swap_ab: {}", e.swap_ab)?;
        writeln!(w, "    detent_guard_ms: {}", det)?;
        writeln!(w, "    encoder_pulse_ms: {}", encoder_pulse_ms)?;
        writeln!(w, "    cw_action: {}", actionwire_to_toml(e.cw_action))?;
        writeln!(w, "    ccw_action: {}", actionwire_to_toml(e.ccw_action))?;
    }

    Ok(())
}
