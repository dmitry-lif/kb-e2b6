// src/config/slot_writer.rs
//
// Firmware-side A/B config save (WebHID "Save to device"). Implements the writer rules of
// Spec - CRS Slots §5/§8 with the STM32G4 "program each double word once" constraint:
//
// 1. Target = the slot NOT selected at boot (so the currently persisted config stays intact).
// 2. seq = max(valid seq) + 1, so the boot selector will prefer the new slot.
// 3. Erase the target page; program the slot header with the `committed` double word
//    (offset 0x10..0x17: committed + reserved1) left erased (0xFF), then the CRS bytes.
// 4. Read back (ECC-checked) and compare.
// 5. Program the `committed` double word to zero: committed = 0 (COMMITTED), reserved1 = 0.
// 6. Re-run the boot selector and confirm it now picks the new slot with identical bytes.
//
// A power loss before step 5 leaves the target uncommitted (ignored at boot); a torn write reads
// as an ECC error (slot invalid). Either way the previous slot is still selected.

use crs_format::{SLOT_CRS_OFFSET, SLOT_HDR_LEN, SLOT_HDR_VERSION, SLOT_MAGIC_LE, TOTAL_LEN};

use super::fw::{CFG_SLOT_A_BASE, CFG_SLOT_B_BASE, CFG_SLOT_SIZE};
use super::loader;
use super::status::{BootSource, SlotKind};
use crate::hw::flash;

/// Offset of the `committed` double word inside the slot header.
const COMMITTED_DWORD_OFFSET: usize = 0x10;

/// Commit failure reasons (reported as the FLASH_ERROR detail byte; keep values stable).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitError {
    NoFlash = 1,
    SeqExhausted = 2,
    Erase = 3,
    Program = 4,
    Verify = 5,
    NotSelected = 6,
}

#[derive(Clone, Copy, Debug)]
pub struct CommitResult {
    pub slot: SlotKind,
    pub seq: u32,
}

fn slot_base(kind: SlotKind) -> u32 {
    match kind {
        SlotKind::SlotA => CFG_SLOT_A_BASE,
        SlotKind::SlotB => CFG_SLOT_B_BASE,
    }
}

/// Slot header image with the `committed` double word left erased.
fn header_image(seq: u32) -> [u8; SLOT_HDR_LEN] {
    let mut h = [0u8; SLOT_HDR_LEN];
    h[0..4].copy_from_slice(&SLOT_MAGIC_LE.to_le_bytes());
    h[4..6].copy_from_slice(&SLOT_HDR_VERSION.to_le_bytes());
    // 6..8 reserved0 = 0
    h[8..12].copy_from_slice(&seq.to_le_bytes());
    h[12..16].copy_from_slice(&(TOTAL_LEN as u32).to_le_bytes());
    h[COMMITTED_DWORD_OFFSET..COMMITTED_DWORD_OFFSET + 8].fill(0xFF);
    // 24..32 reserved2/reserved3 = 0
    h
}

/// Persist `crs` (already validated) into the inactive slot.
pub async fn commit(crs: &[u8; TOTAL_LEN]) -> Result<CommitResult, CommitError> {
    let (st, _) = loader::load_slots();

    let valid_seq = |ok: bool, seq: Option<u32>| if ok { seq } else { None };
    let a = valid_seq(st.slot_a.ok, st.slot_a.seq);
    let b = valid_seq(st.slot_b.ok, st.slot_b.seq);

    let target = match st.source {
        BootSource::SlotA => SlotKind::SlotB,
        BootSource::SlotB => SlotKind::SlotA,
        _ => SlotKind::SlotA,
    };

    let max_seq = match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };
    let seq = match max_seq {
        None => 1,
        Some(m) => m.checked_add(1).ok_or(CommitError::SeqExhausted)?,
    };

    let base = slot_base(target);
    let off = flash::offset_of(base);
    let hdr = header_image(seq);

    defmt::info!(
        "cfg commit: target slot {} seq={}",
        if target == SlotKind::SlotA { "A" } else { "B" },
        seq
    );

    let written = flash::with_flash(|f| -> Result<(), CommitError> {
        f.blocking_erase(off, off + CFG_SLOT_SIZE)
            .map_err(|_| CommitError::Erase)?;

        // Header without the committed double word, then the CRS blob (288 = 36 double words).
        f.blocking_write(off, &hdr[..COMMITTED_DWORD_OFFSET])
            .map_err(|_| CommitError::Program)?;
        f.blocking_write(
            off + (COMMITTED_DWORD_OFFSET as u32) + 8,
            &hdr[COMMITTED_DWORD_OFFSET + 8..],
        )
        .map_err(|_| CommitError::Program)?;
        f.blocking_write(off + SLOT_CRS_OFFSET as u32, crs)
            .map_err(|_| CommitError::Program)?;

        // Verify everything before committing.
        let mut rb = [0u8; SLOT_HDR_LEN + TOTAL_LEN];
        flash::read_checked(base, &mut rb).map_err(|_| CommitError::Verify)?;
        if rb[..SLOT_HDR_LEN] != hdr[..] || rb[SLOT_HDR_LEN..] != crs[..] {
            return Err(CommitError::Verify);
        }

        // Commit marker: committed = 0x0000_0000, reserved1 = 0 (one double word, written once).
        f.blocking_write(off + COMMITTED_DWORD_OFFSET as u32, &[0u8; 8])
            .map_err(|_| CommitError::Program)?;
        Ok(())
    })
    .await;

    match written {
        None => return Err(CommitError::NoFlash),
        Some(Err(e)) => {
            defmt::error!("cfg commit: failed ({})", e as u8);
            return Err(e);
        }
        Some(Ok(())) => {}
    }

    // The boot selector must now pick exactly what we wrote.
    let (_, selected) = loader::load_slots();
    match selected {
        Some(p) if p.kind == target && p.seq == seq && p.bytes == *crs => {
            defmt::info!("cfg commit: ok");
            Ok(CommitResult { slot: target, seq })
        }
        _ => {
            defmt::error!("cfg commit: new slot not selected after write");
            Err(CommitError::NotSelected)
        }
    }
}
