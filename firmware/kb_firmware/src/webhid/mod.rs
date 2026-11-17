// src/webhid/mod.rs
//
// WebHID configuration channel: a separate vendor-defined HID interface (usage page 0xFF00,
// usage 0x01, no report IDs, 64-byte IN + 64-byte OUT reports) carrying the request/response
// protocol defined in `crs_format::webhid`.
//
// One user action: "Save to device" = upload (WRITE_BEGIN/CHUNK) + WRITE_END, which
// 1. checks the candidate (CRS header + shared validator `crs_format::validate_payload`),
// 2. saves it to the inactive A/B slot (`config::slot_writer`), then
// 3. reboots, so the normal boot path runs with the new configuration.
// Nothing changes (flash or behavior) if the check or the save fails.
//
// It never exposes slot identity, flash addresses, or raw memory access.
// Normative spec: firmware/docs/specs/Spec - WebHID Config Protocol.md

use embassy_executor::task;
use embassy_time::Timer;
use embassy_usb::class::hid::{HidReader, HidWriter, ReadError};

use crs_format::webhid::{
    self as proto, active_slot as info_slot, capabilities, check_candidate, encode_response, flags, opcode, read_select,
    read_window, state_flags, InfoV1, Status, WriteSession, FRAME_LEN,
};
use crs_format::{CRS_SCHEMA_VERSION, HEADER_LEN, TOTAL_LEN};

use crate::config::fw::FW_COMPAT_ID;
use crate::config::runtime::{self, crs_build_id, crs_payload_crc};
use crate::config::{defaults, slot_writer, status};
use crate::hw::usb_composite::UsbDriver;

/// HID report descriptor of the configuration interface (vendor page, 64 B in / 64 B out).
pub const REPORT_DESCRIPTOR: &[u8] = &[
    0x06, 0x00, 0xFF, // Usage Page (Vendor Defined 0xFF00)
    0x09, 0x01,       // Usage (0x01: KB-E2B6 configuration channel)
    0xA1, 0x01,       // Collection (Application)
    0x15, 0x00,       //   Logical Minimum (0)
    0x26, 0xFF, 0x00, //   Logical Maximum (255)
    0x75, 0x08,       //   Report Size (8)
    0x95, 0x40,       //   Report Count (64)
    0x09, 0x02,       //   Usage (0x02: device → host frame)
    0x81, 0x02,       //   Input (Data,Var,Abs)
    0x95, 0x40,       //   Report Count (64)
    0x09, 0x03,       //   Usage (0x03: host → device frame)
    0x91, 0x02,       //   Output (Data,Var,Abs)
    0xC0,             // End Collection
];

/// Interrupt endpoint polling interval for the config interface.
pub const POLL_MS: u8 = 5;

const fn parse_u8(s: &str) -> u8 {
    let b = s.as_bytes();
    let mut i = 0;
    let mut v: u32 = 0;
    while i < b.len() {
        v = v * 10 + (b[i] - b'0') as u32;
        i += 1;
    }
    v as u8
}

pub const FW_MAJOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MAJOR"));
pub const FW_MINOR: u8 = parse_u8(env!("CARGO_PKG_VERSION_MINOR"));
pub const FW_PATCH: u8 = parse_u8(env!("CARGO_PKG_VERSION_PATCH"));

/// What the task must do after sending the response.
#[derive(Clone, Copy, Eq, PartialEq)]
enum After {
    Nothing,
    Reboot,
}

#[task]
pub async fn config_hid_task(
    mut reader: HidReader<'static, UsbDriver, FRAME_LEN>,
    mut writer: HidWriter<'static, UsbDriver, FRAME_LEN>,
) -> ! {
    let mut session = WriteSession::new();
    let mut rx = [0u8; FRAME_LEN];
    let mut tx = [0u8; FRAME_LEN];

    loop {
        match reader.read(&mut rx).await {
            Ok(n) => {
                let after = handle(&mut session, &rx[..n], &mut tx).await;
                let _ = writer.write(&tx).await;
                if after == After::Reboot {
                    reboot().await;
                }
            }
            Err(ReadError::Disabled) => {
                // USB reset/unplug: any half-received candidate is void.
                session.abort();
                reader.ready().await;
            }
            Err(_) => {}
        }
    }
}

/// Reboot through the normal boot path so the newly saved configuration takes effect.
async fn reboot() -> ! {
    defmt::info!("webhid: rebooting into the saved configuration");
    // Give the host a few poll intervals to collect the response.
    Timer::after_millis(150).await;
    // Never reset in the middle of a flash erase/program (LED journal).
    crate::hw::flash::quiesce().await;
    cortex_m::peripheral::SCB::sys_reset();
}

struct Reply {
    status: Status,
    flags: u8,
    offset: u16,
    len: usize,
    payload: [u8; proto::MAX_FRAME_PAYLOAD],
}

impl Reply {
    fn ok() -> Self {
        Self::status(Status::Ok)
    }

    fn status(status: Status) -> Self {
        Self {
            status,
            flags: 0,
            offset: 0,
            len: 0,
            payload: [0; proto::MAX_FRAME_PAYLOAD],
        }
    }

    fn with(mut self, data: &[u8]) -> Self {
        let n = data.len().min(proto::MAX_FRAME_PAYLOAD);
        self.payload[..n].copy_from_slice(&data[..n]);
        self.len = n;
        self
    }
}

async fn handle(session: &mut WriteSession, frame: &[u8], tx: &mut [u8; FRAME_LEN]) -> After {
    let req = match proto::parse_request(frame) {
        Ok(r) => r,
        Err(e) => {
            defmt::warn!("webhid: bad frame (status {})", e.status as u8);
            encode_response(tx, e.opcode, e.txid, e.status, 0, 0, &[]);
            return After::Nothing;
        }
    };

    let (reply, after) = match req.opcode {
        opcode::GET_INFO => (get_info().await, After::Nothing),
        opcode::READ_CONFIG => (read_config(req.offset, req.payload).await, After::Nothing),
        opcode::WRITE_BEGIN => {
            defmt::info!("webhid: WRITE_BEGIN");
            let r = match session.begin(req.payload) {
                Ok(()) => Reply::ok(),
                Err(s) => Reply::status(s),
            };
            (r, After::Nothing)
        }
        opcode::WRITE_CHUNK => {
            let r = match session.chunk(req.offset, req.payload) {
                Ok(()) => Reply::ok(),
                Err(s) => Reply::status(s),
            };
            (r, After::Nothing)
        }
        opcode::WRITE_END => write_end(session).await,
        opcode::PING => (Reply::ok().with(req.payload), After::Nothing),
        _ => (Reply::status(Status::UnknownCommand), After::Nothing),
    };

    let mut f = reply.flags;
    if after == After::Reboot {
        f |= flags::REBOOTING;
    }
    encode_response(
        tx,
        req.opcode,
        req.txid,
        reply.status,
        f,
        reply.offset,
        &reply.payload[..reply.len],
    );
    after
}

async fn get_info() -> Reply {
    let running = runtime::get_running_crs().await;
    let boot = status::get_boot_status().await;
    let (active_slot, config_seq) = match boot.source {
        status::BootSource::SlotA => (info_slot::A, boot.slot_a.seq.unwrap_or(0)),
        status::BootSource::SlotB => (info_slot::B, boot.slot_b.seq.unwrap_or(0)),
        status::BootSource::DisabledFallback => (info_slot::NONE, 0),
    };
    let led = crate::hw::led_persist::get_status().await;
    let (led_journal_used, led_journal_cap) = if led.enabled {
        (led.write_index as u16, led.cap as u16)
    } else {
        (0, 0)
    };

    let info = InfoV1 {
        device_model: proto::DEVICE_MODEL_ID,
        fw_major: FW_MAJOR,
        fw_minor: FW_MINOR,
        fw_patch: FW_PATCH,
        protocol_version: proto::PROTOCOL_VERSION,
        crs_schema_version: CRS_SCHEMA_VERSION,
        fw_compat_id: FW_COMPAT_ID,
        config_len: TOTAL_LEN as u16,
        state_flags: if running.is_some() {
            state_flags::HAS_CONFIG
        } else {
            0
        },
        config_crc: running.map(|b| crs_payload_crc(&b)).unwrap_or(0),
        config_build_id: running.map(|b| crs_build_id(&b)).unwrap_or(0),
        capabilities: capabilities::SAVE | capabilities::READ_FACTORY_DEFAULTS,
        max_frame_payload: proto::MAX_FRAME_PAYLOAD as u16,
        device_uid: embassy_stm32::uid::uid(),
        slot_valid: (boot.slot_a.ok as u8) | ((boot.slot_b.ok as u8) << 1),
        active_slot,
        config_seq,
        led_journal_used,
        led_journal_cap,
    };
    defmt::info!("webhid: GET_INFO has_config={}", running.is_some() as u8);
    Reply::ok().with(&info.encode())
}

async fn read_config(offset: u16, payload: &[u8]) -> Reply {
    let selector = payload.first().copied().unwrap_or(read_select::RUNTIME);
    let blob = match selector {
        read_select::RUNTIME => match runtime::get_running_crs().await {
            Some(b) => b,
            None => return Reply::status(Status::NoConfig),
        },
        read_select::FACTORY_DEFAULTS => defaults::factory_default_crs(),
        _ => return Reply::status(Status::InvalidLength),
    };

    match read_window(TOTAL_LEN, offset) {
        Ok((start, len, more)) => {
            let mut r = Reply::ok().with(&blob[start..start + len]);
            r.offset = offset;
            if more {
                r.flags |= flags::MORE_DATA;
            }
            r
        }
        Err(s) => Reply::status(s),
    }
}

/// WRITE_END = "Save to device": check the candidate, save it to flash, reboot into it.
async fn write_end(session: &mut WriteSession) -> (Reply, After) {
    let bytes = match session.finish() {
        Ok(b) => b,
        Err(s) => {
            defmt::warn!("webhid: WRITE_END transfer error (status {})", s as u8);
            return (Reply::status(s), After::Nothing);
        }
    };

    if let Err(e) = check_candidate(&bytes, FW_COMPAT_ID) {
        let d = e.detail();
        defmt::warn!(
            "webhid: WRITE_END rejected (status {} detail {} idx {})",
            e.status() as u8,
            d[0],
            d[1]
        );
        return (Reply::status(e.status()).with(&d), After::Nothing);
    }

    if let Some(cur) = runtime::get_running_crs().await {
        if cur[HEADER_LEN..] == bytes[HEADER_LEN..] {
            // Already saved and running exactly this configuration.
            defmt::info!("webhid: WRITE_END unchanged, nothing to save");
            return (Reply::ok().with(&[0]), After::Nothing);
        }
    }

    match slot_writer::commit(&bytes).await {
        Ok(r) => {
            defmt::info!(
                "webhid: saved to slot {} seq={}",
                if r.slot == status::SlotKind::SlotA { "A" } else { "B" },
                r.seq
            );
            (Reply::ok().with(&[1]), After::Reboot)
        }
        Err(e) => (Reply::status(Status::FlashError).with(&[e as u8]), After::Nothing),
    }
}
