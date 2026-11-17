// crs-format/src/lib.rs
// Wire-format crate shared by host tooling and firmware.
//
// This crate is `no_std` so it can be reused by firmware. It remains usable on host.

#![no_std]
#![allow(non_camel_case_types)]

// The current implementation uses bytemuck to view packed structs as bytes.
// That implies native endianness; CRS v1 is specified little-endian.
// Guard against big-endian hosts to avoid silently producing wrong binaries.
#[cfg(target_endian = "big")]
compile_error!("crs-format assumes a little-endian host for CRS v1 packing/dumping");

use bytemuck::{Pod, Zeroable};
use static_assertions::const_assert_eq;

#[cfg(test)]
extern crate std;

// Semantic payload validation (shared by crs-pack and firmware).
pub mod validate;
pub use validate::{validate_payload, ValidationCode, ValidationError, GLOBAL_INDEX};

// WebHID configuration protocol v1 (framing + staged write session).
pub mod webhid;

// =========================
// Frozen capacity (CRS v1)
// =========================
pub const MAX_PANEL_BUTTONS: usize = 6;
pub const MAX_ENCODER_BUTTONS: usize = 2;
pub const MAX_BUTTONS_TOTAL: usize = MAX_PANEL_BUTTONS + MAX_ENCODER_BUTTONS; // 8
pub const MAX_ENCODERS: usize = 2;

// =========================
// Header constants
// =========================
pub const CRS_SCHEMA_VERSION: u16 = 1;
pub const CRS_MAGIC_LE: u32 = 0x31535243; // 'CRS1' little-endian

// =========================
// Slot header constants (Spec - CRS Slots.md)
// =========================
pub const SLOT_MAGIC_LE: u32 = 0x314F4C53; // 'SLO1' little-endian
pub const SLOT_HDR_VERSION: u16 = 1;
pub const SLOT_HDR_LEN: usize = 32;
pub const SLOT_CRS_OFFSET: usize = 32;
pub const SLOT_COMMITTED: u32 = 0x0000_0000;
pub const SLOT_NOT_COMMITTED: u32 = 0xFFFF_FFFF;

// =========================
// Wire layouts
// =========================

#[repr(C, packed)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct CrsHeaderV1 {
    pub magic: u32,
    pub crs_schema_version: u16,
    pub flags: u16,
    pub fw_compat_id: u32,
    pub len: u16,
    pub reserved0: u16,
    pub crc32: u32,
    pub build_id: u32,
    pub reserved1: u32,
}

/// Flash-resident per-slot header that wraps CRS bytes.
///
/// Layout is fixed 32 bytes, little-endian, per Spec - CRS Slots.md.
#[repr(C, packed)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct SlotHeaderV1 {
    pub slot_magic: u32,
    pub hdr_version: u16,
    pub reserved0: u16,
    pub seq: u32,
    pub crs_len: u32,
    pub committed: u32,
    pub reserved1: u32,
    pub reserved2: u32,
    pub reserved3: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct GlobalDefaultsV1 {
    pub scan_hz: u16,
    pub pin_pull_default: u8,
    pub active_low_default: u8,
    pub debounce_ms_default: u16,
    pub long_press_ms_default: u16,
    pub pulse_ms_button_default: u16,
    pub repeat_period_ms_default: u16,
    pub encoder_pulse_ms_default: u16,
    pub encoder_swap_ab_default: u8,
    pub encoder_detent_guard_ms_default: u8,
    pub led_state_persist: u8,
    pub led_state_on_boot: u8,
    pub led_level_persist: u8,
    pub led_level_on_boot: u8,
    pub led_level_default: u8,
    pub led_level_step: u8,
    pub led_level_min: u8,
    pub led_level_max: u8,
    pub led_autosave_delay_ms: u16,
    pub reserved: [u8; 2],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct ActionWireV1 {
    pub tag: u8,
    pub a: u8,
    pub b: u8,
    pub c: u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct ButtonEntryV1 {
    pub enabled: u8,
    pub pull: i8,
    pub active_low: i8,
    pub debounce_ms: i16,
    pub long_press_ms: i16,
    pub pulse_ms: i16,
    pub repeat_period_ms: u16,
    pub reserved0: u8,
    pub short_action: ActionWireV1,
    pub long_action: ActionWireV1,
    pub reserved1: [u8; 2],
    pub reserved2: [u8; 2],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct EncoderEntryV1 {
    pub enabled: u8,
    pub swap_ab: i8,
    pub detent_guard_ms: i16,
    pub encoder_pulse_ms: u16,
    pub reserved0: u16,
    pub cw_action: ActionWireV1,
    pub ccw_action: ActionWireV1,
    pub reserved1: [u8; 4],
}

#[repr(C, packed)]
#[derive(Clone, Copy, Zeroable, Pod)]
pub struct CrsPayloadV1 {
    pub globals: GlobalDefaultsV1,
    pub buttons: [ButtonEntryV1; MAX_BUTTONS_TOTAL],
    pub encoders: [EncoderEntryV1; MAX_ENCODERS],
}

// =========================
// Size locks (must match Spec - CRS Format.md)
// =========================
pub const HEADER_LEN: usize = core::mem::size_of::<CrsHeaderV1>();
pub const PAYLOAD_LEN: usize = core::mem::size_of::<CrsPayloadV1>();
pub const TOTAL_LEN: usize = HEADER_LEN + PAYLOAD_LEN;

const_assert_eq!(HEADER_LEN, 28);
const_assert_eq!(core::mem::size_of::<SlotHeaderV1>(), 32);
const_assert_eq!(core::mem::size_of::<GlobalDefaultsV1>(), 28);
const_assert_eq!(core::mem::size_of::<ActionWireV1>(), 4);
const_assert_eq!(core::mem::size_of::<ButtonEntryV1>(), 24);
const_assert_eq!(core::mem::size_of::<EncoderEntryV1>(), 20);
const_assert_eq!(PAYLOAD_LEN, 28 + (8 * 24) + (2 * 20)); // 260
const_assert_eq!(TOTAL_LEN, 28 + 260); // 288

// =========================
// Errors
// =========================
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CrsError {
    BadSize { got: usize, expected: usize },
    BadMagic(u32),
    BadSchema(u16),
    BadLen { got: u16, expected: u16 },
    BadCompat { file: u32, expected: u32 },
    BadCrc { computed: u32, header: u32 },
}

impl core::fmt::Display for CrsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CrsError::BadSize { got, expected } => {
                write!(f, "bad file size: got {got}, expected {expected}")
            }
            CrsError::BadMagic(m) => write!(f, "bad magic: got 0x{m:08X}"),
            CrsError::BadSchema(v) => write!(f, "bad schema version: got {v}, expected 1"),
            CrsError::BadLen { got, expected } => {
                write!(f, "bad payload len: got {got}, expected {expected}")
            }
            CrsError::BadCompat { file, expected } => write!(
                f,
                "fw_compat_id mismatch: file 0x{file:08X}, expected 0x{expected:08X}"
            ),
            CrsError::BadCrc { computed, header } => write!(
                f,
                "crc mismatch: computed 0x{computed:08X}, header 0x{header:08X}"
            ),
        }
    }
}

// =========================
// Helpers
// =========================

pub fn new_zero_payload() -> CrsPayloadV1 {
    CrsPayloadV1::zeroed()
}

pub fn payload_bytes(payload: &CrsPayloadV1) -> &[u8] {
    bytemuck::bytes_of(payload)
}

// CRC-32 (IEEE 802.3 / "ISO-HDLC": reflected poly 0xEDB88320, init and xorout 0xFFFFFFFF), the
// same algorithm as crc32fast / zlib. Bytewise with a 1 KiB table: firmware-sized (crc32fast's
// slicing-by-16 table alone is 16 KiB of flash).
const fn crc32_make_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static CRC32_TABLE: [u32; 256] = crc32_make_table();

const fn crc32_const(bytes: &[u8]) -> u32 {
    let table = crc32_make_table();
    let mut c = 0xFFFF_FFFFu32;
    let mut i = 0;
    while i < bytes.len() {
        c = table[((c ^ bytes[i] as u32) & 0xFF) as usize] ^ (c >> 8);
        i += 1;
    }
    c ^ 0xFFFF_FFFF
}

// Build-time proof of the algorithm against the standard check value.
const_assert_eq!(crc32_const(b"123456789"), 0xCBF4_3926);

pub fn crc32_bytes(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c = CRC32_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

pub fn crc32_payload(payload_bytes: &[u8]) -> u32 {
    crc32_bytes(payload_bytes)
}

pub fn make_header(fw_compat_id: u32, crc32: u32, build_id: u32) -> CrsHeaderV1 {
    CrsHeaderV1 {
        magic: CRS_MAGIC_LE,
        crs_schema_version: CRS_SCHEMA_VERSION,
        flags: 0,
        fw_compat_id,
        len: PAYLOAD_LEN as u16,
        reserved0: 0,
        crc32,
        build_id,
        reserved1: 0,
    }
}

pub fn make_slot_header_v1(seq: u32, committed: u32) -> SlotHeaderV1 {
    SlotHeaderV1 {
        slot_magic: SLOT_MAGIC_LE,
        hdr_version: SLOT_HDR_VERSION,
        reserved0: 0,
        seq,
        crs_len: TOTAL_LEN as u32,
        committed,
        reserved1: 0,
        reserved2: 0,
        reserved3: 0,
    }
}

pub fn serialize(header: &CrsHeaderV1, payload: &CrsPayloadV1) -> [u8; TOTAL_LEN] {
    let mut out = [0u8; TOTAL_LEN];
    let hb = bytemuck::bytes_of(header);
    let pb = bytemuck::bytes_of(payload);
    out[..hb.len()].copy_from_slice(hb);
    out[hb.len()..hb.len() + pb.len()].copy_from_slice(pb);
    out
}

/// Validate structural invariants (magic/schema/len/crc). Does NOT validate fw_compat_id.
pub fn parse_and_validate_basic(bytes: &[u8]) -> Result<(CrsHeaderV1, CrsPayloadV1), CrsError> {
    if bytes.len() != TOTAL_LEN {
        return Err(CrsError::BadSize {
            got: bytes.len(),
            expected: TOTAL_LEN,
        });
    }

    let (hb, pb) = bytes.split_at(HEADER_LEN);
    let header: CrsHeaderV1 = *bytemuck::from_bytes(hb);
    let payload: CrsPayloadV1 = *bytemuck::from_bytes(pb);

    if header.magic != CRS_MAGIC_LE {
        return Err(CrsError::BadMagic(header.magic));
    }
    if header.crs_schema_version != CRS_SCHEMA_VERSION {
        return Err(CrsError::BadSchema(header.crs_schema_version));
    }
    if header.len != (PAYLOAD_LEN as u16) {
        return Err(CrsError::BadLen {
            got: header.len,
            expected: PAYLOAD_LEN as u16,
        });
    }

    let computed = crc32_payload(bytemuck::bytes_of(&payload));
    if computed != header.crc32 {
        return Err(CrsError::BadCrc {
            computed,
            header: header.crc32,
        });
    }

    Ok((header, payload))
}

/// Validate everything including fw_compat_id against an expected value.
pub fn parse_and_validate_with_compat(
    bytes: &[u8],
    fw_compat_id_expected: u32,
) -> Result<(CrsHeaderV1, CrsPayloadV1), CrsError> {
    let (header, payload) = parse_and_validate_basic(bytes)?;

    if header.fw_compat_id != fw_compat_id_expected {
        return Err(CrsError::BadCompat {
            file: header.fw_compat_id,
            expected: fw_compat_id_expected,
        });
    }

    Ok((header, payload))
}

// Safe aligned "views" (avoid refs to packed fields)
#[derive(Debug, Clone, Copy)]
pub struct HeaderView {
    pub magic: u32,
    pub crs_schema_version: u16,
    pub flags: u16,
    pub fw_compat_id: u32,
    pub len: u16,
    pub reserved0: u16,
    pub crc32: u32,
    pub build_id: u32,
    pub reserved1: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalsView {
    pub scan_hz: u16,
    pub pin_pull_default: u8,
    pub active_low_default: u8,
    pub debounce_ms_default: u16,
    pub long_press_ms_default: u16,
    pub pulse_ms_button_default: u16,
    pub repeat_period_ms_default: u16,
    pub encoder_pulse_ms_default: u16,
    pub encoder_swap_ab_default: u8,
    pub encoder_detent_guard_ms_default: u8,
    pub led_state_persist: u8,
    pub led_state_on_boot: u8,
    pub led_level_persist: u8,
    pub led_level_on_boot: u8,
    pub led_level_default: u8,
    pub led_level_step: u8,
    pub led_level_min: u8,
    pub led_level_max: u8,
    pub led_autosave_delay_ms: u16,
}

impl From<CrsHeaderV1> for HeaderView {
    fn from(h: CrsHeaderV1) -> Self {
        Self {
            magic: h.magic,
            crs_schema_version: h.crs_schema_version,
            flags: h.flags,
            fw_compat_id: h.fw_compat_id,
            len: h.len,
            reserved0: h.reserved0,
            crc32: h.crc32,
            build_id: h.build_id,
            reserved1: h.reserved1,
        }
    }
}

impl From<GlobalDefaultsV1> for GlobalsView {
    fn from(g: GlobalDefaultsV1) -> Self {
        Self {
            scan_hz: g.scan_hz,
            pin_pull_default: g.pin_pull_default,
            active_low_default: g.active_low_default,
            debounce_ms_default: g.debounce_ms_default,
            long_press_ms_default: g.long_press_ms_default,
            pulse_ms_button_default: g.pulse_ms_button_default,
            repeat_period_ms_default: g.repeat_period_ms_default,
            encoder_pulse_ms_default: g.encoder_pulse_ms_default,
            encoder_swap_ab_default: g.encoder_swap_ab_default,
            encoder_detent_guard_ms_default: g.encoder_detent_guard_ms_default,
            led_state_persist: g.led_state_persist,
            led_state_on_boot: g.led_state_on_boot,
            led_level_persist: g.led_level_persist,
            led_level_on_boot: g.led_level_on_boot,
            led_level_default: g.led_level_default,
            led_level_step: g.led_level_step,
            led_level_min: g.led_level_min,
            led_level_max: g.led_level_max,
            led_autosave_delay_ms: g.led_autosave_delay_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;

    #[test]
    fn roundtrip_serialize_parse_ok() {
        let mut payload = new_zero_payload();
        payload.globals.scan_hz = 1000;
        payload.globals.pin_pull_default = 1;
        payload.globals.active_low_default = 1;
        payload.globals.debounce_ms_default = 20;
        payload.globals.long_press_ms_default = 400;
        payload.globals.pulse_ms_button_default = 50;
        payload.globals.repeat_period_ms_default = 0;
        payload.globals.encoder_pulse_ms_default = 30;
        payload.globals.encoder_swap_ab_default = 0;
        payload.globals.encoder_detent_guard_ms_default = 2;
        payload.globals.led_state_persist = 1;
        payload.globals.led_state_on_boot = 2;
        payload.globals.led_level_persist = 1;
        payload.globals.led_level_on_boot = 0;
        payload.globals.led_level_default = 30;
        payload.globals.led_level_step = 5;
        payload.globals.led_level_min = 10;
        payload.globals.led_level_max = 100;
        payload.globals.led_autosave_delay_ms = 1500;
        payload.globals.reserved = [0, 0];

        payload.buttons[0].enabled = 1;
        payload.buttons[0].pull = -1;
        payload.buttons[0].active_low = -1;
        payload.buttons[0].debounce_ms = -1;
        payload.buttons[0].long_press_ms = -1;
        payload.buttons[0].pulse_ms = 50;
        payload.buttons[0].repeat_period_ms = 70;
        payload.buttons[0].short_action = ActionWireV1 {
            tag: 2,
            a: 0xE9,
            b: 0x00,
            c: 0,
        };
        payload.buttons[0].long_action = ActionWireV1 {
            tag: 0,
            a: 0,
            b: 0,
            c: 0,
        };

        let crc = crc32_payload(payload_bytes(&payload));
        let header = make_header(0x20260216, crc, 0x12345678);
        let bytes = serialize(&header, &payload);

        let (h2, p2) = parse_and_validate_basic(&bytes).expect("parse ok");

        let hv: HeaderView = h2.into();
        let gv: GlobalsView = p2.globals.into();

        assert_eq!(hv.magic, CRS_MAGIC_LE);
        assert_eq!(hv.fw_compat_id, 0x20260216);
        assert_eq!(hv.build_id, 0x12345678);
        assert_eq!(gv.scan_hz, 1000);
        let b0_enabled = p2.buttons[0].enabled;
        assert_eq!(b0_enabled, 1);

        let rp = unsafe { ptr::read_unaligned(ptr::addr_of!(p2.buttons[0].repeat_period_ms)) };
        assert_eq!(rp, 70);
        let b0_short_tag = p2.buttons[0].short_action.tag;
        assert_eq!(b0_short_tag, 2);
    }

    #[test]
    fn crc_mismatch_is_detected() {
        let payload = new_zero_payload();
        let crc = crc32_payload(payload_bytes(&payload));
        let header = make_header(0x20260216, crc, 1);
        let mut bytes = serialize(&header, &payload);
        // Flip one payload byte.
        bytes[HEADER_LEN] ^= 0x01;
        match parse_and_validate_basic(&bytes) {
            Err(CrsError::BadCrc { .. }) => {}
            Err(other) => panic!("expected BadCrc, got {other}"),
            Ok(_) => panic!("expected CRC error, got Ok"),
        }
    }

    #[test]
    fn globals_roundtrip_selected_fields() {
        let mut payload = new_zero_payload();
        payload.globals.scan_hz = 2000;
        payload.globals.pin_pull_default = 2;
        payload.globals.active_low_default = 0;
        payload.globals.encoder_swap_ab_default = 1;
        payload.globals.led_state_on_boot = 1;
        payload.globals.led_level_on_boot = 42;

        let crc = crc32_payload(payload_bytes(&payload));
        let header = make_header(0x20260216, crc, 0xAABBCCDD);
        let bytes = serialize(&header, &payload);

        let (_h2, p2) = parse_and_validate_basic(&bytes).expect("parse ok");
        let scan_hz = p2.globals.scan_hz;
        let pin_pull_default = p2.globals.pin_pull_default;
        let active_low_default = p2.globals.active_low_default;
        let encoder_swap_ab_default = p2.globals.encoder_swap_ab_default;
        let led_state_on_boot = p2.globals.led_state_on_boot;
        let led_level_on_boot = p2.globals.led_level_on_boot;
        assert_eq!(scan_hz, 2000);
        assert_eq!(pin_pull_default, 2);
        assert_eq!(active_low_default, 0);
        assert_eq!(encoder_swap_ab_default, 1);
        assert_eq!(led_state_on_boot, 1);
        assert_eq!(led_level_on_boot, 42);
    }
}
