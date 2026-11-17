use serde::Deserialize;

// ---------------- TOML schema (strict) ----------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlRoot {
    pub toml: TomlMeta,
    pub target: TomlTarget,
    pub global: TomlGlobal,
    #[serde(default)]
    pub buttons: Vec<TomlButton>,
    #[serde(default)]
    pub encoders: Vec<TomlEncoder>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlMeta {
    pub version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlTarget {
    pub fw_compat_id: String,
    pub crs_schema_version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlGlobal {
    pub scan_hz: u16,

    pub pin_pull_default: String, // "none"|"up"|"down"
    pub active_low_default: bool,
    pub debounce_ms_default: u16,
    pub long_press_ms_default: u16,

    // Buttons: release mode default
    pub pulse_ms_button_default: u16, // 0..500; 0=tracked-release

    // Buttons: repeat default
    pub repeat_period_ms_default: u16, // 0 or 20..2000

    // Encoder rotation: pulse duration default
    pub encoder_pulse_ms_default: u16, // 1..200

    // Encoder decoding defaults
    pub encoder_swap_ab_default: bool,
    pub encoder_detent_guard_ms_default: u8, // 0..10

    // LED behavior
    pub led_state_persist: bool,
    pub led_state_on_boot: String, // "on"|"off"|"restore"

    pub led_level_persist: bool,
    pub led_level_on_boot: u8, // 0=restore, else 1..100
    pub led_level_default: u8, // 1..100

    pub led_level_step: u8,         // 1..50
    pub led_level_min: u8,          // 1..100
    pub led_level_max: u8,          // 1..100
    pub led_autosave_delay_ms: u16, // 0..60000
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlButton {
    pub kind: String, // "panel"|"encoder"
    pub id: u8,
    pub enabled: bool,

    #[serde(default)]
    pub pin_pull: Option<String>,
    #[serde(default)]
    pub active_low: Option<bool>,
    #[serde(default)]
    pub debounce_ms: Option<u16>,
    #[serde(default)]
    pub long_press_ms: Option<u16>,

    // Release mode override
    #[serde(default)]
    pub pulse_ms: Option<u16>,

    #[serde(default)]
    pub repeat_period_ms: Option<u16>,

    pub short_action: TomlAction,
    #[serde(default)]
    pub long_action: Option<TomlAction>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlEncoder {
    pub id: u8,
    pub enabled: bool,

    #[serde(default)]
    pub swap_ab: Option<bool>,
    #[serde(default)]
    pub detent_guard_ms: Option<u16>,

    #[serde(default)]
    pub encoder_pulse_ms: Option<u16>,

    pub cw_action: TomlAction,
    pub ccw_action: TomlAction,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Empty {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlAction {
    #[serde(default)]
    pub none: Option<Empty>,
    #[serde(default)]
    pub kbd: Option<TomlKbd>,
    #[serde(default)]
    pub cc: Option<TomlCc>,
    #[serde(default)]
    pub internal: Option<TomlInternal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlKbd {
    pub mods: u32,
    pub usage: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlCc {
    pub usage: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlInternal {
    pub id: u32,
}
