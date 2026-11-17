// src/input/scan.rs

use embassy_executor::task;
use embassy_stm32::gpio::Input;
use embassy_time::{Duration, Ticker};

use crate::config::active::{ActiveCfg, MAX_PANEL_BUTTONS, PCB_BTN_ID};
use crate::events::bus::INPUT_CH;
use crate::events::types::{ButtonState, InputEvent};

use super::debounce::Debounce;
use super::encoder::Encoder;

// PA0 is wired active-low (button connects to GND; Pull::Up used in main.rs).
const PCB_BTN_ACTIVE_LOW: bool = true;

#[inline]
fn emit(ev: InputEvent) {
    // Deterministic: never block scan. Drop newest on full.
    let _ = INPUT_CH.try_send(ev);
}

pub struct ScanPins {
    pub btn1: Input<'static>,
    pub btn2: Input<'static>,
    pub btn3: Input<'static>,
    pub btn4: Input<'static>,
    pub btn5: Input<'static>,
    pub btn6: Input<'static>,

    pub enc1_a: Input<'static>,
    pub enc1_b: Input<'static>,
    pub enc1_btn: Input<'static>,

    pub enc2_a: Input<'static>,
    pub enc2_b: Input<'static>,
    pub enc2_btn: Input<'static>,
    pub pcb_btn: Input<'static>,
}

#[task]
pub async fn scan_task(cfg: ActiveCfg, pins: ScanPins) -> ! {
    let ScanPins {
        btn1,
        btn2,
        btn3,
        btn4,
        btn5,
        btn6,
        enc1_a,
        enc1_b,
        enc1_btn,
        enc2_a,
        enc2_b,
        enc2_btn,
        pcb_btn,
    } = pins;

    let scan_hz = cfg.globals.scan_hz.max(1) as u64;
    let period_us = (1_000_000u64 / scan_hz).max(1);
    let mut ticker = Ticker::every(Duration::from_micros(period_us));

    let ticks_from_ms_nonzero = |ms: u16| -> u8 {
        let t = ((ms as u32) * (scan_hz as u32) + 999) / 1000;
        t.clamp(1, u8::MAX as u32) as u8
    };

    let ticks_from_ms_allow0 = |ms: u16| -> u8 {
        if ms == 0 {
            return 0;
        }
        let t = ((ms as u32) * (scan_hz as u32) + 999) / 1000;
        t.clamp(1, u8::MAX as u32) as u8
    };

    let b = cfg.panel_buttons;
    let eb = cfg.encoder_buttons;
    let e = cfg.encoders;

    let mk_db = |pin: &Input<'static>, active_low: bool, debounce_ms: u16| {
        Debounce::new(
            sample_pressed(pin, active_low),
            ticks_from_ms_nonzero(debounce_ms),
        )
    };

    let mut db_btn = [
        mk_db(&btn1, b[0].active_low, b[0].debounce_ms),
        mk_db(&btn2, b[1].active_low, b[1].debounce_ms),
        mk_db(&btn3, b[2].active_low, b[2].debounce_ms),
        mk_db(&btn4, b[3].active_low, b[3].debounce_ms),
        mk_db(&btn5, b[4].active_low, b[4].debounce_ms),
        mk_db(&btn6, b[5].active_low, b[5].debounce_ms),
    ];

    // Encoder push buttons are exposed as Button IDs 6..7 (= MAX_PANEL_BUTTONS + encoder index).
    let mut db_enc_btn = [
        mk_db(&enc1_btn, eb[0].active_low, eb[0].debounce_ms),
        mk_db(&enc2_btn, eb[1].active_low, eb[1].debounce_ms),
    ];
    // PCB button (ID = PCB_BTN_ID). Active-low hardcoded; debounce timing shared from globals.
    let mut db_pcb_btn = mk_db(
        &pcb_btn,
        PCB_BTN_ACTIVE_LOW,
        cfg.globals.debounce_ms_default,
    );

    let pack_ab = |a_high: bool, b_high: bool, swap_ab: bool| -> u8 {
        let (a_high, b_high) = if swap_ab {
            (b_high, a_high)
        } else {
            (a_high, b_high)
        };
        ((a_high as u8) << 1) | (b_high as u8)
    };

    let enc1_init_ab = pack_ab(enc1_a.is_high(), enc1_b.is_high(), e[0].swap_ab);
    let enc2_init_ab = pack_ab(enc2_a.is_high(), enc2_b.is_high(), e[1].swap_ab);

    let mut enc = [
        Encoder::new(
            enc1_init_ab,
            ticks_from_ms_allow0(e[0].detent_guard_ms),
            e[0].swap_ab,
        ),
        Encoder::new(
            enc2_init_ab,
            ticks_from_ms_allow0(e[1].detent_guard_ms),
            e[1].swap_ab,
        ),
    ];

    // Panel button pins — sampled per-tick.
    let btn_pins: [&Input<'static>; 6] = [&btn1, &btn2, &btn3, &btn4, &btn5, &btn6];
    // Encoder A/B wires and push button pins.
    let enc_ab_pins: [((&Input<'static>, &Input<'static>), &Input<'static>); 2] = [
        ((&enc1_a, &enc1_b), &enc1_btn),
        ((&enc2_a, &enc2_b), &enc2_btn),
    ];

    loop {
        ticker.next().await;

        // Panel buttons (IDs 0..5).
        for i in 0..6usize {
            if !b[i].enabled {
                continue;
            }
            let raw = sample_pressed(btn_pins[i], b[i].active_low);
            if let Some(pressed) = db_btn[i].update(raw) {
                emit(InputEvent::Button {
                    id: i as u8,
                    state: ButtonState::from_pressed(pressed),
                });
            }
        }

        // Encoder push buttons (IDs 6..7) and rotation.
        for i in 0..2usize {
            let ((a_pin, b_pin), btn_pin) = enc_ab_pins[i];

            if eb[i].enabled {
                let raw = sample_pressed(btn_pin, eb[i].active_low);
                if let Some(pressed) = db_enc_btn[i].update(raw) {
                    emit(InputEvent::Button {
                        id: (MAX_PANEL_BUTTONS + i) as u8,
                        state: ButtonState::from_pressed(pressed),
                    });
                }
            }

            if e[i].enabled {
                if let Some(step) = enc[i].update(a_pin.is_high(), b_pin.is_high()) {
                    emit(InputEvent::Encoder { id: i as u8, step });
                }
            }
        }

        // PCB button (ID = PCB_BTN_ID, always scanned, hardcoded active-low).
        {
            let raw = sample_pressed(&pcb_btn, PCB_BTN_ACTIVE_LOW);
            if let Some(pressed) = db_pcb_btn.update(raw) {
                emit(InputEvent::Button {
                    id: PCB_BTN_ID as u8,
                    state: ButtonState::from_pressed(pressed),
                });
            }
        }
    }
}

#[inline]
fn sample_pressed(pin: &Input<'static>, active_low: bool) -> bool {
    if active_low {
        pin.is_low()
    } else {
        pin.is_high()
    }
}
