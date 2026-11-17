use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

use crs_format::HeaderView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotKind {
    SlotA,
    SlotB,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootSource {
    SlotA,
    SlotB,
    DisabledFallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotError {
    None,
    /// Uncorrectable flash ECC error while reading the slot (torn write).
    FlashEcc,
    BadSlotMagic,
    BadSlotVersion,
    BadSlotLen,
    SlotNotCommitted,
    BadSize,
    BadMagic,
    BadSchema,
    BadLen,
    BadCompat,
    BadCrc,
}

#[derive(Clone, Copy, Debug)]
pub struct SlotStatus {
    #[allow(dead_code)]
    pub kind: SlotKind,
    pub ok: bool,
    pub seq: Option<u32>,
    pub header: Option<HeaderView>,
    pub err: SlotError,
}

impl SlotStatus {
    pub const fn new(kind: SlotKind) -> Self {
        Self {
            kind,
            ok: false,
            seq: None,
            header: None,
            err: SlotError::None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BootStatus {
    pub source: BootSource,
    #[cfg_attr(not(feature = "usb_cdc"), allow(dead_code))]
    pub slot_a: SlotStatus,
    #[cfg_attr(not(feature = "usb_cdc"), allow(dead_code))]
    pub slot_b: SlotStatus,
}

impl BootStatus {
    pub const fn new() -> Self {
        Self {
            source: BootSource::DisabledFallback,
            slot_a: SlotStatus::new(SlotKind::SlotA),
            slot_b: SlotStatus::new(SlotKind::SlotB),
        }
    }
}

pub static BOOT_STATUS: Mutex<CriticalSectionRawMutex, BootStatus> = Mutex::new(BootStatus::new());

pub async fn set_boot_status(s: BootStatus) {
    *BOOT_STATUS.lock().await = s;
}

pub async fn get_boot_status() -> BootStatus {
    *BOOT_STATUS.lock().await
}

#[allow(dead_code)]
fn err_name(e: SlotError) -> &'static str {
    match e {
        SlotError::None => "None",
        SlotError::FlashEcc => "Flash ECC error (torn write)",
        SlotError::BadSlotMagic => "Invalid slot marker (magic)",
        SlotError::BadSlotVersion => "Unsupported slot version",
        SlotError::BadSlotLen => "Invalid slot length",
        SlotError::SlotNotCommitted => "Slot not committed",
        SlotError::BadSize => "Invalid CRS payload size",
        SlotError::BadMagic => "Invalid CRS header marker (magic)",
        SlotError::BadSchema => "Unsupported CRS schema version",
        SlotError::BadLen => "Invalid CRS payload length",
        SlotError::BadCompat => "Firmware compatibility mismatch",
        SlotError::BadCrc => "CRC mismatch",
    }
}

#[allow(dead_code)]
pub fn format_startup_line<const N: usize>(st: &BootStatus, buf: &mut heapless::String<N>) {
    use core::fmt::Write;

    buf.clear();

    let (slot_name, chosen) = match st.source {
        BootSource::SlotA => ("A", Some(st.slot_a)),
        BootSource::SlotB => ("B", Some(st.slot_b)),
        BootSource::DisabledFallback => ("-", None),
    };

    if let Some(ss) = chosen {
        if let Some(h) = ss.header {
            let seq = ss.seq.unwrap_or(0);
            let _ = write!(
                buf,
                "cfg: slot {} ok seq={} schema={} fw=0x{:08X} len={} crc=0x{:08X} build=0x{:08X}",
                slot_name, seq, h.crs_schema_version, h.fw_compat_id, h.len, h.crc32, h.build_id
            );
        } else {
            let _ = write!(buf, "cfg: slot {} ok", slot_name);
        }
        return;
    }

    let _ = write!(buf, "cfg: slots invalid -> inputs disabled");
}

pub fn format_status<const N: usize>(st: &BootStatus, buf: &mut heapless::String<N>) {
    use core::fmt::Write;
    buf.clear();

    let mut w = crate::text::CrlfWriter::new(buf);

    let _ = writeln!(w, "Configuration slot status:");

    let _ = writeln!(
        w,
        "Selected boot source: {}",
        match st.source {
            BootSource::SlotA => "Slot A",
            BootSource::SlotB => "Slot B",
            BootSource::DisabledFallback => "Disabled fallback (no valid config)",
        }
    );

    fn write_slot<const N: usize>(
        w: &mut crate::text::CrlfWriter<'_, N>,
        slot_name: &'static str,
        ss: SlotStatus,
    ) {
        let _ = writeln!(w, "");
        let _ = writeln!(w, "Slot {}:", slot_name);
        let _ = writeln!(w, "  Result: {}", if ss.ok { "OK" } else { "ERROR" });
        if !ss.ok {
            let _ = writeln!(w, "  Error: {}", err_name(ss.err));
        }

        match ss.seq {
            Some(v) => {
                let _ = writeln!(w, "  Sequence number: {}", v);
            }
            None => {
                let _ = writeln!(w, "  Sequence number: (not available)");
            }
        }

        if let Some(h) = ss.header {
            let _ = writeln!(w, "  CRS schema version: {}", h.crs_schema_version);
            let _ = writeln!(w, "  Firmware compatibility ID: 0x{:08X}", h.fw_compat_id);
            let _ = writeln!(w, "  Payload length (bytes): {}", h.len);
            let _ = writeln!(w, "  Payload CRC32: 0x{:08X}", h.crc32);
            let _ = writeln!(w, "  Build ID: 0x{:08X}", h.build_id);
        } else {
            let _ = writeln!(w, "  Header: (not parsed)");
        }
    }

    write_slot(&mut w, "A", st.slot_a);
    write_slot(&mut w, "B", st.slot_b);
}
