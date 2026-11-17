// src/events/types.rs

use crate::config::action::ActionEncoding;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonState {
    Pressed,
    Released,
}

impl ButtonState {
    #[inline]
    pub fn from_pressed(pressed: bool) -> Self {
        if pressed { Self::Pressed } else { Self::Released }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputEvent {
    Button { id: u8, state: ButtonState },
    Encoder { id: u8, step: i8 }, // +1 / -1
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HidEvent {
    KbdPress { mods: u8, usage: u8 },
    KbdRelease,
    CcPress { usage: u16 },
    CcRelease,
    Internal { id: u8 },
}

impl HidEvent {
    pub fn from_press(a: ActionEncoding) -> Option<(Self, Option<Self>)> {
        match a {
            ActionEncoding::None => None,
            ActionEncoding::Kbd { mods, usage } => {
                Some((Self::KbdPress { mods, usage }, Some(Self::KbdRelease)))
            }
            ActionEncoding::Cc { usage } => Some((Self::CcPress { usage }, Some(Self::CcRelease))),
            ActionEncoding::Internal { id } => Some((Self::Internal { id }, None)),
        }
    }
}
