// src/app/console.rs

use crate::config::action::ActionEncoding;
use crate::events::bus::ACTION_CH;
use crate::events::types::HidEvent;

pub const PROMPT: &[u8] = b"> ";

const HELP: &[u8] = b"Commands:\r\n\
help\r\n\
info\r\n\
cfg status\r\n\
cfg show\r\n\
hid kbd <mods_hex> <usage_hex>\r\n\
hid cc <usage16_hex>\r\n\
examples:\r\n\
hid kbd 00 29      (Esc)\r\n\
hid kbd 00 4F      (Right)\r\n\
hid cc 00E9        (Vol+)\r\n";

/// Small response enum — transport layer decides how to print.
pub enum Response {
    None,
    Static(&'static [u8]),
    Owned(heapless::String<2048>),
    Ok,
    Err,
}

pub async fn handle_line(line: &str) -> Response {
    let s = line.trim();
    if s.is_empty() {
        return Response::None;
    }

    if s == "help" || s == "?" {
        return Response::Static(HELP);
    }

    if s == "info" {
        return Response::Owned(crate::app::diag::info_text());
    }

    if s == "cfg status" {
        return Response::Owned(crate::app::diag::cfg_status_text().await);
    }

    if s == "cfg show" {
        return Response::Owned(crate::app::diag::cfg_show_text().await);
    }

    match parse_action(s) {
        Parse::Act(a) => {
            if let Some((press, release)) = HidEvent::from_press(a) {
                ACTION_CH.send(press).await;
                if let Some(release) = release {
                    embassy_time::Timer::after_millis(20).await;
                    ACTION_CH.send(release).await;
                }
            }
            Response::Ok
        }
        Parse::Help => Response::Static(HELP),
        Parse::Err => Response::Err,
    }
}

enum Parse {
    Act(ActionEncoding),
    Help,
    Err,
}

fn hex_u8(s: &str) -> Option<u8> {
    u8::from_str_radix(s, 16).ok()
}

fn hex_u16(s: &str) -> Option<u16> {
    u16::from_str_radix(s, 16).ok()
}

fn parse_action(s: &str) -> Parse {
    let mut it = s.split_whitespace();
    let a0 = match it.next() {
        Some(x) => x,
        None => return Parse::Err,
    };

    if a0 != "hid" {
        return Parse::Err;
    }

    let a1 = match it.next() {
        Some(x) => x,
        None => return Parse::Help,
    };

    match a1 {
        "kbd" => {
            let mods_s = match it.next() {
                Some(x) => x,
                None => return Parse::Help,
            };
            let usage_s = match it.next() {
                Some(x) => x,
                None => return Parse::Help,
            };
            if it.next().is_some() {
                return Parse::Err;
            }

            let mods = match hex_u8(mods_s) {
                Some(v) => v,
                None => return Parse::Err,
            };
            let usage = match hex_u8(usage_s) {
                Some(v) => v,
                None => return Parse::Err,
            };

            Parse::Act(ActionEncoding::Kbd { mods, usage })
        }
        "cc" => {
            let usage_s = match it.next() {
                Some(x) => x,
                None => return Parse::Help,
            };
            if it.next().is_some() {
                return Parse::Err;
            }

            let usage = match hex_u16(usage_s) {
                Some(v) => v,
                None => return Parse::Err,
            };
            Parse::Act(ActionEncoding::Cc { usage })
        }
        _ => Parse::Err,
    }
}
