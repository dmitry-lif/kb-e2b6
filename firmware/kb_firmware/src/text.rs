use core::fmt;

// Technical identifier (crate/bin, CI artifacts, etc.).
#[allow(dead_code)]
pub const FW_ID: &str = "kb-firmware";

// USB identity (what the host shows).
// VID/PID: generic hobby/test IDs. If you ever get a PID (e.g. free from pid.codes, VID 0x1209),
// change these two and the copies in web_config/js/transport.js and
// web_config/linux/70-kb-e2b6-webhid.rules.
pub const USB_VID: u16 = 0xCAFE;
pub const USB_PID: u16 = 0x4002;
// No manufacturer string on purpose: hosts then name the device by the product alone
// (e.g. Linux: "KB-E2B6 Keyboard").
pub const USB_PRODUCT: &str = "KB-E2B6";

/// Serial number prefix; the rest is the MCU's factory-unique 96-bit ID in hex.
pub const USB_SERIAL_PREFIX: &str = "KBE2B6-";
const USB_SERIAL_LEN: usize = 7 + 24;

/// Per-unit USB serial number, e.g. `KBE2B6-3A0F1C2D4B5E6F708192A3B4`.
///
/// Unique and stable for every board without per-unit programming, so the PC can tell several
/// KB-E2B6 boards apart. WebHID GET_INFO reports the same UID so the configurator can show it.
pub fn usb_serial() -> &'static str {
    static mut SERIAL: [u8; USB_SERIAL_LEN] = [0; USB_SERIAL_LEN];
    static mut READY: bool = false;
    cortex_m::interrupt::free(|_| unsafe {
        if !READY {
            let s = &mut *core::ptr::addr_of_mut!(SERIAL);
            s[..USB_SERIAL_PREFIX.len()].copy_from_slice(USB_SERIAL_PREFIX.as_bytes());
            s[USB_SERIAL_PREFIX.len()..].copy_from_slice(embassy_stm32::uid::uid_hex_bytes());
            READY = true;
        }
        // Safety: ASCII only (fixed prefix + hex digits); written once, read-only afterwards.
        core::str::from_utf8_unchecked(&*core::ptr::addr_of!(SERIAL))
    })
}

// Short persistent banner (printed at boot over RTT).
pub const BANNER_RTT: &str = "KB-E2B6 USB HID Control Interface";
#[allow(dead_code)]
pub const BANNER_CDC: &[u8] = b"KB-E2B6 USB HID Control Interface\r\n";

/// Wraps a `heapless::String` and converts lone `\n` to `\r\n`.
///
/// This avoids the classic CDC/serial terminal behavior where `\n` moves to the
/// next line but doesn't return to column 0 ("drifting" output).
pub struct CrlfWriter<'a, const N: usize> {
    buf: &'a mut heapless::String<N>,
    last_was_cr: bool,
}

impl<'a, const N: usize> CrlfWriter<'a, N> {
    pub fn new(buf: &'a mut heapless::String<N>) -> Self {
        Self {
            buf,
            last_was_cr: false,
        }
    }
}

impl<'a, const N: usize> fmt::Write for CrlfWriter<'a, N> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.chars() {
            match ch {
                '\n' => {
                    if self.last_was_cr {
                        self.buf.push('\n').map_err(|_| fmt::Error)?;
                    } else {
                        self.buf.push('\r').map_err(|_| fmt::Error)?;
                        self.buf.push('\n').map_err(|_| fmt::Error)?;
                    }
                    self.last_was_cr = false;
                }
                '\r' => {
                    self.buf.push('\r').map_err(|_| fmt::Error)?;
                    self.last_was_cr = true;
                }
                _ => {
                    self.buf.push(ch).map_err(|_| fmt::Error)?;
                    self.last_was_cr = false;
                }
            }
        }
        Ok(())
    }
}
