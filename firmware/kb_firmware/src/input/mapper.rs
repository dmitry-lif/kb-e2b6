// src/input/mapper.rs

use embassy_executor::task;
use embassy_futures::select::Either;
use embassy_time::{Duration, Instant, Ticker};

use crate::config::action::{ActionEncoding, INTERNAL_LED_SWING, INTERNAL_LED_TOGGLE};
use crate::config::active::{
    ActiveCfg, MAX_ENCODERS, MAX_ENCODER_BUTTONS, MAX_PANEL_BUTTONS, PCB_BTN_ID,
};
use crate::events::bus::{ACTION_CH, INPUT_CH};
use crate::events::types::{ButtonState, HidEvent, InputEvent};

const N_BTNLIKE: usize = MAX_PANEL_BUTTONS + MAX_ENCODER_BUTTONS + 1; // 0..5 panel, 6..7 encoder pushes, 8 PCB button

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RepeatTarget {
    None,
    Short,
    Long,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReleaseKind {
    None,
    Kbd,
    Cc,
}

#[derive(Clone, Copy)]
struct BtnState {
    down: bool,
    fired_long: bool,
    t_down_ms: u64,
    next_repeat_ms: u64,
    repeat: RepeatTarget,

    // Timed release (pulse mode).
    pending_release: ReleaseKind,
    pending_release_ms: u64,

    // Tracked release (pulse_ms=0).
    tracked_release: ReleaseKind,
}

impl BtnState {
    const fn new() -> Self {
        Self {
            down: false,
            fired_long: false,
            t_down_ms: 0,
            next_repeat_ms: 0,
            repeat: RepeatTarget::None,
            pending_release: ReleaseKind::None,
            pending_release_ms: 0,
            tracked_release: ReleaseKind::None,
        }
    }
}

fn now_ms() -> u64 {
    Instant::now().as_millis() as u64
}

enum MapperWake {
    Tick,
    Ev(InputEvent),
}

async fn send_release(kind: ReleaseKind) {
    match kind {
        ReleaseKind::None => {}
        ReleaseKind::Kbd => ACTION_CH.send(HidEvent::KbdRelease).await,
        ReleaseKind::Cc => ACTION_CH.send(HidEvent::CcRelease).await,
    }
}

async fn send_press(a: ActionEncoding) -> ReleaseKind {
    let Some((ev, rel)) = HidEvent::from_press(a) else {
        return ReleaseKind::None;
    };

    ACTION_CH.send(ev).await;

    match rel {
        Some(HidEvent::KbdRelease) => ReleaseKind::Kbd,
        Some(HidEvent::CcRelease) => ReleaseKind::Cc,
        _ => ReleaseKind::None,
    }
}

async fn press_with_pulse(
    a: ActionEncoding,
    pulse_ms: u16,
    pending_kind: &mut ReleaseKind,
    pending_ms: &mut u64,
) {
    if a == ActionEncoding::None {
        return;
    }

    // Avoid overlapping presses: release anything pending before pressing again.
    if *pending_kind != ReleaseKind::None {
        send_release(*pending_kind).await;
        *pending_kind = ReleaseKind::None;
        *pending_ms = 0;
    }

    let rel_kind = send_press(a).await;
    if rel_kind == ReleaseKind::None {
        return;
    }

    let t0 = now_ms();
    *pending_kind = rel_kind;
    *pending_ms = t0 + pulse_ms as u64;
}

async fn press_and_release_immediate(a: ActionEncoding) {
    if a == ActionEncoding::None {
        return;
    }

    let rel_kind = send_press(a).await;
    send_release(rel_kind).await;
}

fn btnlike_cfg(
    cfg: &ActiveCfg,
    id: usize,
) -> Option<(bool, u16, u16, u16, ActionEncoding, ActionEncoding)> {
    if id < MAX_PANEL_BUTTONS {
        let b = cfg.panel_buttons[id];
        return Some((
            b.enabled,
            b.long_press_ms,
            b.repeat_period_ms,
            b.pulse_ms,
            b.short_action,
            b.long_action,
        ));
    }

    let enc_btn_id = id - MAX_PANEL_BUTTONS;
    if enc_btn_id < MAX_ENCODER_BUTTONS {
        let b = cfg.encoder_buttons[enc_btn_id];
        return Some((
            b.enabled,
            b.long_press_ms,
            b.repeat_period_ms,
            b.pulse_ms,
            b.short_action,
            b.long_action,
        ));
    }

    // PCB button: always enabled, hardcoded actions.
    // short=LED_TOGGLE, long=LED_SWING (level next, wraps), repeat_period=100 ms.
    if id == PCB_BTN_ID {
        return Some((
            true,
            cfg.globals.long_press_ms_default,
            100, // hardcoded 100 ms repeat period for PCB button
            cfg.globals.pulse_ms_button_default,
            ActionEncoding::Internal {
                id: INTERNAL_LED_TOGGLE,
            },
            ActionEncoding::Internal {
                id: INTERNAL_LED_SWING,
            },
        ));
    }

    None
}

async fn on_btn_pressed(cfg: &ActiveCfg, id: usize, st: &mut [BtnState; N_BTNLIKE]) {
    let Some((enabled, _long_ms, repeat_ms, pulse_ms, short_a, long_a)) = btnlike_cfg(cfg, id)
    else {
        return;
    };
    if !enabled {
        return;
    }

    let t0 = now_ms();
    let s = &mut st[id];
    s.down = true;
    s.fired_long = false;
    s.t_down_ms = t0;
    s.next_repeat_ms = 0;
    s.repeat = RepeatTarget::None;
    s.tracked_release = ReleaseKind::None;

    if long_a == ActionEncoding::None {
        // Short-only: press immediately.
        if pulse_ms == 0 {
            s.tracked_release = send_press(short_a).await;
        } else {
            press_with_pulse(
                short_a,
                pulse_ms,
                &mut s.pending_release,
                &mut s.pending_release_ms,
            )
            .await;
        }

        // Repeat (if enabled) targets short, but requires pulse_ms>0.
        if repeat_ms > 0 && pulse_ms > 0 {
            s.repeat = RepeatTarget::Short;
            s.next_repeat_ms = t0 + repeat_ms as u64;
        }
    } else {
        // Short+long:
        // - long emitted when threshold reached
        // - short emitted on release if long never fired
        // - repeat targets long (if enabled; requires pulse_ms>0)
        if repeat_ms > 0 && pulse_ms > 0 {
            s.repeat = RepeatTarget::Long;
        }
    }
}

async fn on_btn_released(cfg: &ActiveCfg, id: usize, st: &mut [BtnState; N_BTNLIKE]) {
    let Some((enabled, _long_ms, _repeat_ms, pulse_ms, short_a, long_a)) = btnlike_cfg(cfg, id)
    else {
        return;
    };
    if !enabled {
        return;
    }

    let s = &mut st[id];
    let emit_short = s.down && !s.fired_long && long_a != ActionEncoding::None;

    // Tracked-release: release any currently-held HID.
    if s.tracked_release != ReleaseKind::None {
        send_release(s.tracked_release).await;
        s.tracked_release = ReleaseKind::None;
    }

    s.down = false;
    s.fired_long = false;
    s.next_repeat_ms = 0;
    s.repeat = RepeatTarget::None;

    if emit_short {
        if pulse_ms == 0 {
            // Tracked-release mode: the physical release is happening now, so
            // emit a minimal tap as press+release immediately.
            press_and_release_immediate(short_a).await;
        } else {
            press_with_pulse(
                short_a,
                pulse_ms,
                &mut s.pending_release,
                &mut s.pending_release_ms,
            )
            .await;
        }
    }
}

async fn tick_btns(cfg: &ActiveCfg, st: &mut [BtnState; N_BTNLIKE]) {
    let t = now_ms();

    for id in 0..N_BTNLIKE {
        let Some((enabled, long_ms, repeat_ms, pulse_ms, short_a, long_a)) = btnlike_cfg(cfg, id)
        else {
            continue;
        };
        if !enabled {
            continue;
        }

        let s = &mut st[id];

        // Timed release for pulsed actions.
        if s.pending_release != ReleaseKind::None
            && s.pending_release_ms != 0
            && t >= s.pending_release_ms
        {
            send_release(s.pending_release).await;
            s.pending_release = ReleaseKind::None;
            s.pending_release_ms = 0;
        }

        if !s.down {
            continue;
        }

        // Long threshold
        if long_a != ActionEncoding::None && !s.fired_long {
            let held = t.saturating_sub(s.t_down_ms);
            if held >= long_ms as u64 {
                s.fired_long = true;
                if pulse_ms == 0 {
                    s.tracked_release = send_press(long_a).await;
                } else {
                    press_with_pulse(
                        long_a,
                        pulse_ms,
                        &mut s.pending_release,
                        &mut s.pending_release_ms,
                    )
                    .await;
                }

                if repeat_ms > 0 && pulse_ms > 0 {
                    s.repeat = RepeatTarget::Long;
                    s.next_repeat_ms = t + repeat_ms as u64;
                }
            }
        }

        // Repeat
        if repeat_ms > 0
            && pulse_ms > 0
            && s.repeat != RepeatTarget::None
            && s.next_repeat_ms != 0
            && t >= s.next_repeat_ms
        {
            match s.repeat {
                RepeatTarget::Short => {
                    press_with_pulse(
                        short_a,
                        pulse_ms,
                        &mut s.pending_release,
                        &mut s.pending_release_ms,
                    )
                    .await;
                }
                RepeatTarget::Long => {
                    press_with_pulse(
                        long_a,
                        pulse_ms,
                        &mut s.pending_release,
                        &mut s.pending_release_ms,
                    )
                    .await;
                }
                RepeatTarget::None => {}
            }
            s.next_repeat_ms = t + repeat_ms as u64;
        }
    }
}

#[task]
pub async fn mapper_task(cfg: ActiveCfg) -> ! {
    let mut tick = Ticker::every(Duration::from_millis(5));
    let mut st: [BtnState; N_BTNLIKE] = [BtnState::new(); N_BTNLIKE];
    let mut enc_pending_release: [ReleaseKind; MAX_ENCODERS] = [ReleaseKind::None; MAX_ENCODERS];
    let mut enc_pending_release_ms: [u64; MAX_ENCODERS] = [0; MAX_ENCODERS];

    loop {
        let wake = embassy_futures::select::select(
            async {
                tick.next().await;
                MapperWake::Tick
            },
            async { MapperWake::Ev(INPUT_CH.receive().await) },
        )
        .await;

        let wake = match wake {
            Either::First(w) => w,
            Either::Second(w) => w,
        };

        match wake {
            MapperWake::Tick => {
                tick_btns(&cfg, &mut st).await;

                // Encoder timed releases.
                let t = now_ms();
                for i in 0..MAX_ENCODERS {
                    if enc_pending_release[i] != ReleaseKind::None
                        && enc_pending_release_ms[i] != 0
                        && t >= enc_pending_release_ms[i]
                    {
                        send_release(enc_pending_release[i]).await;
                        enc_pending_release[i] = ReleaseKind::None;
                        enc_pending_release_ms[i] = 0;
                    }
                }
            }
            MapperWake::Ev(ev) => match ev {
                InputEvent::Button {
                    id,
                    state: ButtonState::Pressed,
                } => {
                    let id = id as usize;
                    if id < N_BTNLIKE {
                        on_btn_pressed(&cfg, id, &mut st).await;
                    }
                }
                InputEvent::Button {
                    id,
                    state: ButtonState::Released,
                } => {
                    let id = id as usize;
                    if id < N_BTNLIKE {
                        on_btn_released(&cfg, id, &mut st).await;
                    }
                }

                InputEvent::Encoder { id, step } => {
                    let enc_id = id as usize;
                    if enc_id >= MAX_ENCODERS {
                        continue;
                    }
                    let e = cfg.encoders[enc_id];
                    if !e.enabled {
                        continue;
                    }

                    let a = if step > 0 {
                        e.cw_action
                    } else if step < 0 {
                        e.ccw_action
                    } else {
                        ActionEncoding::None
                    };

                    if a == ActionEncoding::None {
                        continue;
                    }

                    // Avoid overlapping: release pending encoder press before pressing again.
                    if enc_pending_release[enc_id] != ReleaseKind::None {
                        send_release(enc_pending_release[enc_id]).await;
                        enc_pending_release[enc_id] = ReleaseKind::None;
                        enc_pending_release_ms[enc_id] = 0;
                    }

                    let rel_kind = send_press(a).await;
                    if rel_kind != ReleaseKind::None {
                        enc_pending_release[enc_id] = rel_kind;
                        enc_pending_release_ms[enc_id] = now_ms() + e.encoder_pulse_ms as u64;
                    }

                    // Note: if step magnitude ever becomes >1, this currently emits one pulse.
                }
            },
        }
    }
}
