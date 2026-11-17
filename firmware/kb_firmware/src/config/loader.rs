use crs_format::{
    parse_and_validate_with_compat, CrsError, CrsPayloadV1, HeaderView, SLOT_COMMITTED,
    SLOT_CRS_OFFSET, SLOT_HDR_LEN, SLOT_HDR_VERSION, SLOT_MAGIC_LE, TOTAL_LEN,
};

use super::fw::{CFG_SLOT_A_BASE, CFG_SLOT_B_BASE, FW_COMPAT_ID};
use super::status::{BootSource, BootStatus, SlotError, SlotKind, SlotStatus};
use crate::hw::flash;

fn map_err(e: CrsError) -> SlotError {
    match e {
        CrsError::BadSize { .. } => SlotError::BadSize,
        CrsError::BadMagic(_) => SlotError::BadMagic,
        CrsError::BadSchema(_) => SlotError::BadSchema,
        CrsError::BadLen { .. } => SlotError::BadLen,
        CrsError::BadCompat { .. } => SlotError::BadCompat,
        CrsError::BadCrc { .. } => SlotError::BadCrc,
    }
}

fn read_flash_bytes(base: u32, out: &mut [u8]) -> Result<(), SlotError> {
    // A torn double word (power loss during a slot write) reads as an ECC error: the slot is
    // invalid, never a hang.
    flash::read_checked(base, out).map_err(|_| SlotError::FlashEcc)
}

#[derive(Clone, Copy, Debug)]
struct SlotHdrInfo {
    seq: u32,
}

fn parse_slot_header(base: u32) -> Result<SlotHdrInfo, SlotError> {
    let mut b = [0u8; SLOT_HDR_LEN];
    read_flash_bytes(base, &mut b)?;

    let slot_magic = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    if slot_magic != SLOT_MAGIC_LE {
        return Err(SlotError::BadSlotMagic);
    }

    let hdr_version = u16::from_le_bytes([b[4], b[5]]);
    if hdr_version != SLOT_HDR_VERSION {
        return Err(SlotError::BadSlotVersion);
    }

    let seq = u32::from_le_bytes([b[8], b[9], b[10], b[11]]);
    let crs_len = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
    let committed = u32::from_le_bytes([b[16], b[17], b[18], b[19]]);

    // Keep validation minimal and spec-aligned.
    if crs_len != (TOTAL_LEN as u32) {
        return Err(SlotError::BadSlotLen);
    }

    if committed != SLOT_COMMITTED {
        return Err(SlotError::SlotNotCommitted);
    }

    Ok(SlotHdrInfo { seq })
}

fn read_crs_bytes_in_slot(base: u32) -> Result<[u8; TOTAL_LEN], SlotError> {
    let mut out = [0u8; TOTAL_LEN];
    read_flash_bytes(base + (SLOT_CRS_OFFSET as u32), &mut out)?;
    Ok(out)
}

/// The persisted configuration selected from flash.
#[derive(Clone, Copy)]
pub struct PersistedCfg {
    pub kind: SlotKind,
    pub seq: u32,
    /// Raw CRS v1 bytes (header + payload), exactly as stored.
    pub bytes: [u8; TOTAL_LEN],
    pub payload: CrsPayloadV1,
}

fn validate_slot(kind: SlotKind, base: u32) -> (SlotStatus, Option<PersistedCfg>) {
    let mut st = SlotStatus::new(kind);

    let fail = |mut st: SlotStatus, e: SlotError| {
        st.ok = false;
        st.err = e;
        st.seq = None;
        st.header = None;
        (st, None)
    };

    let hdr_info = match parse_slot_header(base) {
        Ok(h) => h,
        Err(e) => return fail(st, e),
    };

    let bytes = match read_crs_bytes_in_slot(base) {
        Ok(b) => b,
        Err(e) => return fail(st, e),
    };

    match parse_and_validate_with_compat(&bytes, FW_COMPAT_ID) {
        Ok((hdr, payload)) => {
            st.ok = true;
            st.seq = Some(hdr_info.seq);
            st.header = Some(HeaderView::from(hdr));
            let cfg = PersistedCfg {
                kind,
                seq: hdr_info.seq,
                bytes,
                payload,
            };
            (st, Some(cfg))
        }
        Err(e) => fail(st, map_err(e)),
    }
}

/// Load and validate configuration slots.
///
/// Selection:
/// - If both slots are valid+committed, select the one with the higher `seq` (tie → Slot A).
/// - If only one is valid+committed, select it.
/// - Else DisabledFallback (all inputs disabled).
///
/// Also used by the A/B writer (`slot_writer`) to re-read what is actually persisted.
pub fn load_slots() -> (BootStatus, Option<PersistedCfg>) {
    let (a_st, a_cfg) = validate_slot(SlotKind::SlotA, CFG_SLOT_A_BASE);
    let (b_st, b_cfg) = validate_slot(SlotKind::SlotB, CFG_SLOT_B_BASE);

    let mut boot = BootStatus {
        source: BootSource::DisabledFallback,
        slot_a: a_st,
        slot_b: b_st,
    };

    let selected = match (a_cfg, b_cfg) {
        (Some(a), Some(b)) => {
            if a.seq >= b.seq {
                Some(a)
            } else {
                Some(b)
            }
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    boot.source = match selected {
        Some(c) if c.kind == SlotKind::SlotA => BootSource::SlotA,
        Some(_) => BootSource::SlotB,
        None => BootSource::DisabledFallback,
    };

    (boot, selected)
}
