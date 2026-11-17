// src/events/bus.rs

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

use super::types::InputEvent;
use super::types::HidEvent;

pub static INPUT_CH: Channel<CriticalSectionRawMutex, InputEvent, 32> = Channel::new();
pub static ACTION_CH: Channel<CriticalSectionRawMutex, HidEvent, 32> = Channel::new();
