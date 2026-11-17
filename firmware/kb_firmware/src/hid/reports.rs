// src/hid/reports.rs

pub const REPORT_ID_KEYBOARD: u8 = 1;
pub const REPORT_ID_CONSUMER: u8 = 2;

// Keyboard report size: 1 (ReportID) + 8 bytes standard boot-like packet = 9 bytes
// [ID, mods, reserved, k0, k1, k2, k3, k4, k5]
pub fn keyboard_press(mods: u8, usage: u8) -> [u8; 9] {
    [REPORT_ID_KEYBOARD, mods, 0x00, usage, 0, 0, 0, 0, 0]
}

pub fn keyboard_release() -> [u8; 9] {
    [REPORT_ID_KEYBOARD, 0x00, 0x00, 0, 0, 0, 0, 0, 0]
}

// Consumer reports: [ReportID, usage_lo, usage_hi]
pub fn consumer_press(usage: u16) -> [u8; 3] {
    [REPORT_ID_CONSUMER, (usage & 0xFF) as u8, (usage >> 8) as u8]
}
pub fn consumer_release() -> [u8; 3] {
    [REPORT_ID_CONSUMER, 0, 0]
}

// Descriptor: Keyboard (ID 1) + Consumer (ID 2, 16-bit array)
pub const REPORT_DESCRIPTOR: &[u8] = &[
    // --- Keyboard (Report ID 1) ---
    0x05, 0x01,       // Usage Page (Generic Desktop)
    0x09, 0x06,       // Usage (Keyboard)
    0xA1, 0x01,       // Collection (Application)
    0x85, 0x01,       //   Report ID (1)

    0x05, 0x07,       //   Usage Page (Key Codes)
    0x19, 0xE0,       //   Usage Min (224)
    0x29, 0xE7,       //   Usage Max (231)
    0x15, 0x00,       //   Logical Min (0)
    0x25, 0x01,       //   Logical Max (1)
    0x75, 0x01,       //   Report Size (1)
    0x95, 0x08,       //   Report Count (8)
    0x81, 0x02,       //   Input (Data,Var,Abs) ; Modifier byte

    0x75, 0x08,       //   Report Size (8)
    0x95, 0x01,       //   Report Count (1)
    0x81, 0x01,       //   Input (Const,Array,Abs) ; Reserved

    0x05, 0x07,       //   Usage Page (Key Codes)
    0x19, 0x00,       //   Usage Min (0)
    0x29, 0xFF,       //   Usage Max (0xFF)
    0x15, 0x00,       //   Logical Min (0)
    0x25, 0xFF,       //   Logical Max (0xFF)
    0x75, 0x08,       //   Report Size (8)
    0x95, 0x06,       //   Report Count (6)
    0x81, 0x00,       //   Input (Data,Array,Abs) ; Keys
    0xC0,             // End Collection

    // --- Consumer Control (Report ID 2): 16-bit usage value (array, 1 item) ---
    0x05, 0x0C,       // Usage Page (Consumer)
    0x09, 0x01,       // Usage (Consumer Control)
    0xA1, 0x01,       // Collection (Application)
    0x85, 0x02,       //   Report ID (2)

    0x15, 0x00,       //   Logical Min (0)
    0x26, 0xFF, 0x0F, //   Logical Max (0x0FFF)
    0x19, 0x00,       //   Usage Min (0)
    0x2A, 0xFF, 0x0F, //   Usage Max (0x0FFF)

    0x75, 0x10,       //   Report Size (16)
    0x95, 0x01,       //   Report Count (1)
    0x81, 0x00,       //   Input (Data,Array,Abs)

    0xC0,             // End Collection
];
