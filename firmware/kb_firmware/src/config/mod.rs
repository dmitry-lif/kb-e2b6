// src/config/mod.rs

pub mod action;

// Firmware: flash-loaded CRS config
pub mod active;
pub mod fw;
pub mod loader;
pub mod runtime;
pub mod status;

// WebHID configuration support: factory defaults, A/B save
pub mod defaults;
pub mod slot_writer;
