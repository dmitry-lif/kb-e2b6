// crs-format/src/webhid.rs
//
// WebHID configuration protocol v1: framing, opcodes, status codes, the staged-write session and
// the GET_INFO layout. Pure `no_std` code (no USB, no flash), shared by firmware and host tests.
// The browser client (`web_config/js/protocol.js`) mirrors these constants.
//
// Normative description: firmware/docs/specs/Spec - WebHID Config Protocol.md

use crate::{
    crc32_bytes, parse_and_validate_with_compat, validate_payload, CrsError, CrsPayloadV1,
    ValidationError, CRS_SCHEMA_VERSION, TOTAL_LEN,
};

// =========================
// USB identity of the config interface
// =========================
pub const USAGE_PAGE: u16 = 0xFF00; // vendor-defined
pub const USAGE: u16 = 0x0001; // KB-E2B6 configuration channel
pub const DEVICE_MODEL_ID: u16 = 0xE2B6;

// =========================
// Framing
// =========================
pub const FRAME_LEN: usize = 64;
pub const FRAME_HEADER_LEN: usize = 12;
pub const MAX_FRAME_PAYLOAD: usize = FRAME_LEN - FRAME_HEADER_LEN; // 52
pub const MAGIC0: u8 = b'K';
pub const MAGIC1: u8 = b'B';
pub const PROTOCOL_VERSION: u8 = 1;

/// Frame header byte offsets.
pub mod off {
    pub const MAGIC0: usize = 0;
    pub const MAGIC1: usize = 1;
    pub const VERSION: usize = 2;
    pub const OPCODE: usize = 3;
    pub const FLAGS: usize = 4;
    pub const STATUS: usize = 5;
    pub const TXID: usize = 6; // u16 LE
    pub const OFFSET: usize = 8; // u16 LE
    pub const LEN: usize = 10;
    pub const RESERVED: usize = 11;
    pub const PAYLOAD: usize = 12;
}

pub mod opcode {
    pub const GET_INFO: u8 = 0x01;
    pub const READ_CONFIG: u8 = 0x02;
    pub const WRITE_BEGIN: u8 = 0x10;
    pub const WRITE_CHUNK: u8 = 0x11;
    /// Validate, save to flash (A/B), then reboot into the new configuration.
    pub const WRITE_END: u8 = 0x12;
    pub const PING: u8 = 0x30;
}

pub mod flags {
    pub const RESPONSE: u8 = 1 << 0;
    pub const ERROR: u8 = 1 << 1;
    pub const MORE_DATA: u8 = 1 << 2;
    // bit 3 reserved
    /// The device reboots right after sending this response (config saved).
    pub const REBOOTING: u8 = 1 << 4;
}

/// READ_CONFIG selector (request payload byte 0).
pub mod read_select {
    pub const RUNTIME: u8 = 0;
    pub const FACTORY_DEFAULTS: u8 = 1;
}

/// Response status codes (stable values).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Ok = 0x00,
    BadMagic = 0x01,
    UnsupportedProtocol = 0x02,
    UnknownCommand = 0x03,
    InvalidLength = 0x04,
    InvalidOffset = 0x05,
    NoTransfer = 0x06,
    BadConfigSize = 0x08,
    BadConfigCrc = 0x09,
    UnsupportedConfigSchema = 0x0A,
    ConfigHeaderInvalid = 0x0B,
    ConfigValidationFailed = 0x0C,
    Busy = 0x0D,
    FlashError = 0x0E,
    InternalError = 0x0F,
    NoConfig = 0x10,
    FwCompatMismatch = 0x11,
}

/// A parsed request frame.
#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    pub opcode: u8,
    pub txid: u16,
    pub offset: u16,
    pub payload: &'a [u8],
}

/// Why a frame could not be parsed; `opcode`/`txid` are echoed in the error response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameError {
    pub status: Status,
    pub opcode: u8,
    pub txid: u16,
}

pub fn parse_request(frame: &[u8]) -> Result<Request<'_>, FrameError> {
    let opcode = frame.get(off::OPCODE).copied().unwrap_or(0);
    let txid = if frame.len() >= off::TXID + 2 {
        u16::from_le_bytes([frame[off::TXID], frame[off::TXID + 1]])
    } else {
        0
    };
    let err = |status| FrameError {
        status,
        opcode,
        txid,
    };

    if frame.len() < FRAME_HEADER_LEN {
        return Err(err(Status::InvalidLength));
    }
    if frame[off::MAGIC0] != MAGIC0 || frame[off::MAGIC1] != MAGIC1 {
        return Err(err(Status::BadMagic));
    }
    if frame[off::VERSION] != PROTOCOL_VERSION {
        return Err(err(Status::UnsupportedProtocol));
    }
    if frame[off::FLAGS] & flags::RESPONSE != 0 {
        // A response frame sent to the device is not a request.
        return Err(err(Status::BadMagic));
    }
    let len = frame[off::LEN] as usize;
    if len > MAX_FRAME_PAYLOAD || off::PAYLOAD + len > frame.len() {
        return Err(err(Status::InvalidLength));
    }

    Ok(Request {
        opcode,
        txid,
        offset: u16::from_le_bytes([frame[off::OFFSET], frame[off::OFFSET + 1]]),
        payload: &frame[off::PAYLOAD..off::PAYLOAD + len],
    })
}

/// Encode a response frame into `out` (always the full 64 bytes, zero padded).
///
/// `ERROR` is set automatically when `status != Ok`. Payload longer than 52 bytes is truncated.
pub fn encode_response(
    out: &mut [u8; FRAME_LEN],
    opcode: u8,
    txid: u16,
    status: Status,
    extra_flags: u8,
    offset: u16,
    payload: &[u8],
) {
    let len = payload.len().min(MAX_FRAME_PAYLOAD);
    out.fill(0);
    out[off::MAGIC0] = MAGIC0;
    out[off::MAGIC1] = MAGIC1;
    out[off::VERSION] = PROTOCOL_VERSION;
    out[off::OPCODE] = opcode;
    let mut f = flags::RESPONSE | extra_flags;
    if status != Status::Ok {
        f |= flags::ERROR;
    }
    out[off::FLAGS] = f;
    out[off::STATUS] = status as u8;
    out[off::TXID..off::TXID + 2].copy_from_slice(&txid.to_le_bytes());
    out[off::OFFSET..off::OFFSET + 2].copy_from_slice(&offset.to_le_bytes());
    out[off::LEN] = len as u8;
    out[off::PAYLOAD..off::PAYLOAD + len].copy_from_slice(&payload[..len]);
}

/// Chunk window for READ_CONFIG: `(start, len, more)` for a blob of `total` bytes.
pub fn read_window(total: usize, offset: u16) -> Result<(usize, usize, bool), Status> {
    let start = offset as usize;
    if start >= total {
        return Err(Status::InvalidOffset);
    }
    let len = (total - start).min(MAX_FRAME_PAYLOAD);
    Ok((start, len, start + len < total))
}

// =========================
// Staged write (WRITE_BEGIN / WRITE_CHUNK / WRITE_END)
// =========================

/// WRITE_BEGIN payload: `u16 total_len, u16 crs_schema_version, u32 crc32(all bytes)`.
pub const WRITE_BEGIN_PAYLOAD_LEN: usize = 8;

/// Receive state for one candidate config. Chunks must arrive in order (`offset == received`).
/// A new WRITE_BEGIN always restarts the transfer (single client, one request in flight).
pub struct WriteSession {
    active: bool,
    expected_crc: u32,
    received: usize,
    buf: [u8; TOTAL_LEN],
}

impl WriteSession {
    pub const fn new() -> Self {
        Self {
            active: false,
            expected_crc: 0,
            received: 0,
            buf: [0u8; TOTAL_LEN],
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn abort(&mut self) {
        self.active = false;
        self.received = 0;
    }

    pub fn begin(&mut self, payload: &[u8]) -> Result<(), Status> {
        self.abort();
        if payload.len() != WRITE_BEGIN_PAYLOAD_LEN {
            return Err(Status::InvalidLength);
        }
        let total = u16::from_le_bytes([payload[0], payload[1]]) as usize;
        let schema = u16::from_le_bytes([payload[2], payload[3]]);
        let crc = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        if total != TOTAL_LEN {
            return Err(Status::BadConfigSize);
        }
        if schema != CRS_SCHEMA_VERSION {
            return Err(Status::UnsupportedConfigSchema);
        }
        self.expected_crc = crc;
        self.active = true;
        Ok(())
    }

    pub fn chunk(&mut self, offset: u16, data: &[u8]) -> Result<(), Status> {
        if !self.active {
            return Err(Status::NoTransfer);
        }
        if offset as usize != self.received {
            return Err(Status::InvalidOffset);
        }
        if data.is_empty() || self.received + data.len() > TOTAL_LEN {
            return Err(Status::InvalidLength);
        }
        self.buf[self.received..self.received + data.len()].copy_from_slice(data);
        self.received += data.len();
        Ok(())
    }

    /// Complete the transfer: length + transport CRC check. Always ends the session.
    pub fn finish(&mut self) -> Result<[u8; TOTAL_LEN], Status> {
        if !self.active {
            return Err(Status::NoTransfer);
        }
        let complete = self.received == TOTAL_LEN;
        self.abort();
        if !complete {
            return Err(Status::InvalidLength);
        }
        if crc32_bytes(&self.buf) != self.expected_crc {
            return Err(Status::BadConfigCrc);
        }
        Ok(self.buf)
    }
}

impl Default for WriteSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Why a received candidate was rejected; maps to a status + 2-byte detail payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateError {
    Header(CrsError),
    Validation(ValidationError),
}

impl CandidateError {
    pub fn status(&self) -> Status {
        match self {
            CandidateError::Header(CrsError::BadCompat { .. }) => Status::FwCompatMismatch,
            CandidateError::Header(CrsError::BadSchema(_)) => Status::UnsupportedConfigSchema,
            CandidateError::Header(_) => Status::ConfigHeaderInvalid,
            CandidateError::Validation(_) => Status::ConfigValidationFailed,
        }
    }

    /// Detail bytes for the error response payload: `[code, index]`.
    ///
    /// Header errors: code 1=size 2=magic 3=schema 4=len 5=compat 6=crc, index 0xFF.
    /// Validation errors: `ValidationCode` value + CRS entry index (0xFF = globals).
    pub fn detail(&self) -> [u8; 2] {
        match self {
            CandidateError::Header(e) => {
                let code = match e {
                    CrsError::BadSize { .. } => 1,
                    CrsError::BadMagic(_) => 2,
                    CrsError::BadSchema(_) => 3,
                    CrsError::BadLen { .. } => 4,
                    CrsError::BadCompat { .. } => 5,
                    CrsError::BadCrc { .. } => 6,
                };
                [code, 0xFF]
            }
            CandidateError::Validation(v) => [v.code as u8, v.index],
        }
    }
}

/// Full candidate check: CRS header (magic/schema/len/compat/CRC) + semantic payload validation.
pub fn check_candidate(bytes: &[u8], fw_compat_id: u32) -> Result<CrsPayloadV1, CandidateError> {
    let (_hdr, payload) =
        parse_and_validate_with_compat(bytes, fw_compat_id).map_err(CandidateError::Header)?;
    validate_payload(&payload).map_err(CandidateError::Validation)?;
    Ok(payload)
}

// =========================
// GET_INFO
// =========================

/// GET_INFO `state_flags` bits.
pub mod state_flags {
    /// The device runs a valid saved configuration (else: inputs disabled, DisabledFallback).
    pub const HAS_CONFIG: u8 = 1 << 0;
}

/// GET_INFO `capabilities` bits.
pub mod capabilities {
    /// WRITE_END validates, saves to flash and reboots into the new configuration.
    pub const SAVE: u32 = 1 << 0;
    pub const READ_FACTORY_DEFAULTS: u32 = 1 << 1;
}

// GET_INFO must fit in one response frame.
const _: () = assert!(InfoV1::ENCODED_LEN <= MAX_FRAME_PAYLOAD);

/// GET_INFO `active_slot` values.
pub mod active_slot {
    pub const NONE: u8 = 0;
    pub const A: u8 = 1;
    pub const B: u8 = 2;
}

/// GET_INFO response payload (little-endian, 52 bytes). The first 44 bytes are the original
/// layout; bytes 15, 30 and 44..52 were filled in later (clients must accept a 44-byte reply and
/// then treat those fields as unknown).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InfoV1 {
    pub device_model: u16,
    pub fw_major: u8,
    pub fw_minor: u8,
    pub fw_patch: u8,
    pub protocol_version: u8,
    pub crs_schema_version: u16,
    pub fw_compat_id: u32,
    pub config_len: u16,
    pub state_flags: u8,
    /// Flash slot the running configuration was loaded from (`active_slot::*`).
    pub active_slot: u8,
    /// Payload CRC32 of the running (= saved) configuration; 0 if none.
    pub config_crc: u32,
    /// `build_id` of the running configuration; 0 if none.
    pub config_build_id: u32,
    pub capabilities: u32,
    pub max_frame_payload: u16,
    /// Valid (committed, compatible) slots: bit0 A, bit1 B. Empty and corrupted both read 0.
    pub slot_valid: u8,
    /// MCU factory-unique 96-bit ID (the USB serial is `KBE2B6-` + this in hex).
    pub device_uid: [u8; 12],
    /// Slot sequence number of the running configuration (+1 per save); 0 if none.
    pub config_seq: u32,
    /// LED journal: records written in the current flash page (page erased when full).
    pub led_journal_used: u16,
    /// LED journal: records per page; 0 = LED on/off and brightness memory is off.
    pub led_journal_cap: u16,
}

impl InfoV1 {
    pub const ENCODED_LEN: usize = 52;

    pub fn encode(&self) -> [u8; Self::ENCODED_LEN] {
        let mut o = [0u8; Self::ENCODED_LEN];
        o[0..2].copy_from_slice(&self.device_model.to_le_bytes());
        o[2] = self.fw_major;
        o[3] = self.fw_minor;
        o[4] = self.fw_patch;
        o[5] = self.protocol_version;
        o[6..8].copy_from_slice(&self.crs_schema_version.to_le_bytes());
        o[8..12].copy_from_slice(&self.fw_compat_id.to_le_bytes());
        o[12..14].copy_from_slice(&self.config_len.to_le_bytes());
        o[14] = self.state_flags;
        o[15] = self.active_slot;
        o[16..20].copy_from_slice(&self.config_crc.to_le_bytes());
        o[20..24].copy_from_slice(&self.config_build_id.to_le_bytes());
        o[24..28].copy_from_slice(&self.capabilities.to_le_bytes());
        o[28..30].copy_from_slice(&self.max_frame_payload.to_le_bytes());
        o[30] = self.slot_valid;
        // 31 reserved (zero)
        o[32..44].copy_from_slice(&self.device_uid);
        o[44..48].copy_from_slice(&self.config_seq.to_le_bytes());
        o[48..50].copy_from_slice(&self.led_journal_used.to_le_bytes());
        o[50..52].copy_from_slice(&self.led_journal_cap.to_le_bytes());
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(opcode: u8, txid: u16, offset: u16, payload: &[u8]) -> [u8; FRAME_LEN] {
        let mut f = [0u8; FRAME_LEN];
        f[off::MAGIC0] = MAGIC0;
        f[off::MAGIC1] = MAGIC1;
        f[off::VERSION] = PROTOCOL_VERSION;
        f[off::OPCODE] = opcode;
        f[off::TXID..off::TXID + 2].copy_from_slice(&txid.to_le_bytes());
        f[off::OFFSET..off::OFFSET + 2].copy_from_slice(&offset.to_le_bytes());
        f[off::LEN] = payload.len() as u8;
        f[off::PAYLOAD..off::PAYLOAD + payload.len()].copy_from_slice(payload);
        f
    }

    #[test]
    fn crc32_check_vector() {
        // Standard CRC-32 (IEEE, reflected, init/xorout 0xFFFFFFFF). Mirrored in web_config.
        assert_eq!(crc32_bytes(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn parses_valid_request() {
        let f = request(opcode::PING, 7, 0, &[1, 2, 3]);
        let r = parse_request(&f).unwrap();
        assert_eq!(r.opcode, opcode::PING);
        assert_eq!(r.txid, 7);
        assert_eq!(r.payload, &[1, 2, 3]);
    }

    #[test]
    fn rejects_bad_magic_and_version_and_length() {
        let mut f = request(opcode::PING, 1, 0, &[]);
        f[0] = b'X';
        assert_eq!(parse_request(&f).unwrap_err().status, Status::BadMagic);

        let mut f = request(opcode::PING, 1, 0, &[]);
        f[off::VERSION] = 2;
        assert_eq!(
            parse_request(&f).unwrap_err().status,
            Status::UnsupportedProtocol
        );

        let mut f = request(opcode::PING, 1, 0, &[]);
        f[off::LEN] = 53;
        assert_eq!(parse_request(&f).unwrap_err().status, Status::InvalidLength);
    }

    #[test]
    fn write_session_happy_path_and_errors() {
        let blob = [0x5Au8; TOTAL_LEN];
        let crc = crc32_bytes(&blob);
        let mut begin = [0u8; 8];
        begin[0..2].copy_from_slice(&(TOTAL_LEN as u16).to_le_bytes());
        begin[2..4].copy_from_slice(&1u16.to_le_bytes());
        begin[4..8].copy_from_slice(&crc.to_le_bytes());

        let mut s = WriteSession::new();
        assert_eq!(s.chunk(0, &blob[..10]), Err(Status::NoTransfer));
        s.begin(&begin).unwrap();
        assert_eq!(s.chunk(4, &blob[..10]), Err(Status::InvalidOffset));
        let mut off = 0;
        while off < TOTAL_LEN {
            let n = (TOTAL_LEN - off).min(MAX_FRAME_PAYLOAD);
            s.chunk(off as u16, &blob[off..off + n]).unwrap();
            off += n;
        }
        assert_eq!(s.finish().unwrap(), blob);
        assert_eq!(s.finish(), Err(Status::NoTransfer));

        // CRC mismatch.
        begin[4] ^= 0xFF;
        s.begin(&begin).unwrap();
        let mut off = 0;
        while off < TOTAL_LEN {
            let n = (TOTAL_LEN - off).min(MAX_FRAME_PAYLOAD);
            s.chunk(off as u16, &blob[off..off + n]).unwrap();
            off += n;
        }
        assert_eq!(s.finish(), Err(Status::BadConfigCrc));
    }

    #[test]
    fn read_window_chunks_288_bytes_into_six_frames() {
        assert_eq!(read_window(TOTAL_LEN, 0), Ok((0, 52, true)));
        assert_eq!(read_window(TOTAL_LEN, 260), Ok((260, 28, false)));
        assert_eq!(read_window(TOTAL_LEN, 288), Err(Status::InvalidOffset));
    }
}
