use anyhow::{anyhow, Result};

/// Parsed inputs needed for packing.
#[derive(Debug, Clone, Copy)]
pub struct PackInput {
    pub fw_compat_id: u32,
}

pub fn parse_hex_u32(s: &str) -> Result<u32> {
    let s = s.trim();
    let s = s
        .strip_prefix("0x")
        .ok_or_else(|| anyhow!("expected hex string like 0x20260216"))?;
    u32::from_str_radix(s, 16).map_err(|e| anyhow!("bad hex: {e}"))
}
