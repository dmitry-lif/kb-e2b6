// src/config/action.rs

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionEncoding {
    None,

    // Keyboard HID (mods bitmap + usage id)
    Kbd { mods: u8, usage: u8 },

    // Consumer Control: literal 16-bit Consumer Page usage ID (Usage Page 0x0C)
    Cc { usage: u16 },

    // Reserved for later (LED/profile/etc.)
    Internal { id: u8 },
}

// Internal actions are device-local and are executed by the firmware (not sent as HID).
// See the match in `hw/usb_composite.rs` for the authoritative mapping.

// Spec/UI names (used in docs and configs):
pub const INTERNAL_LED_TOGGLE: u8 = 0x01; // LED_TOGGLE
pub const INTERNAL_LED_SWING: u8 = 0x02; // LED_SWING (wrap max->min)
pub const INTERNAL_LED_ON: u8 = 0x03; // optional convenience action
pub const INTERNAL_LED_OFF: u8 = 0x04; // optional convenience action
pub const INTERNAL_LED_UP: u8 = 0x05; // LED_UP
pub const INTERNAL_LED_DOWN: u8 = 0x06; // LED_DOWN
