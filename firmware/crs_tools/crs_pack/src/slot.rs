use anyhow::{bail, Result};
use crs_format::{make_slot_header_v1, SlotHeaderV1, SLOT_CRS_OFFSET, SLOT_HDR_LEN, TOTAL_LEN};

/// Build a flashable slot image: 32-byte SlotHeaderV1 + CRS blob at offset 0x20,
/// padded to `slot_size` with 0xFF (erased flash).
pub fn make_slot_image_v1(crs_bytes: &[u8], seq: u32, committed: bool, slot_size: usize) -> Result<Vec<u8>> {
    if crs_bytes.len() != TOTAL_LEN {
        bail!("CRS blob must be exactly {} bytes (got {})", TOTAL_LEN, crs_bytes.len());
    }
    if slot_size < SLOT_HDR_LEN + TOTAL_LEN {
        bail!(
            "slot_size too small: need at least {} (got {})",
            SLOT_HDR_LEN + TOTAL_LEN,
            slot_size
        );
    }

    let committed_val = if committed {
        crs_format::SLOT_COMMITTED
    } else {
        crs_format::SLOT_NOT_COMMITTED
    };
    let slot_header: SlotHeaderV1 = make_slot_header_v1(seq, committed_val);

    let mut out = vec![0xFFu8; slot_size];
    out[..SLOT_HDR_LEN].copy_from_slice(bytemuck::bytes_of(&slot_header));
    out[SLOT_CRS_OFFSET..SLOT_CRS_OFFSET + TOTAL_LEN].copy_from_slice(crs_bytes);
    Ok(out)
}
