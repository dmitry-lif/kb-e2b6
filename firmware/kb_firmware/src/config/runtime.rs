use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

use crs_format::TOTAL_LEN;

use crate::config::active::ActiveCfg;

pub static ACTIVE_CFG: Mutex<CriticalSectionRawMutex, ActiveCfg> =
    Mutex::new(ActiveCfg::disabled_fallback());

pub async fn set_active_cfg(cfg: ActiveCfg) {
    *ACTIVE_CFG.lock().await = cfg;
}

pub async fn get_active_cfg() -> ActiveCfg {
    *ACTIVE_CFG.lock().await
}

/// Raw CRS v1 bytes of the configuration running in this boot (= the saved slot selected at boot;
/// `None` in DisabledFallback). Served to the WebHID configurator.
static RUNNING_CRS: Mutex<CriticalSectionRawMutex, Option<[u8; TOTAL_LEN]>> = Mutex::new(None);

pub async fn set_running_crs(crs: Option<[u8; TOTAL_LEN]>) {
    *RUNNING_CRS.lock().await = crs;
}

pub async fn get_running_crs() -> Option<[u8; TOTAL_LEN]> {
    *RUNNING_CRS.lock().await
}

/// Payload CRC32 from a (validated) CRS header.
pub fn crs_payload_crc(b: &[u8; TOTAL_LEN]) -> u32 {
    u32::from_le_bytes([b[16], b[17], b[18], b[19]])
}

/// `build_id` from a CRS header.
pub fn crs_build_id(b: &[u8; TOTAL_LEN]) -> u32 {
    u32::from_le_bytes([b[20], b[21], b[22], b[23]])
}
