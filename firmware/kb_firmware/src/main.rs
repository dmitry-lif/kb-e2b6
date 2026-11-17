// src/main.rs

#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_time::Timer;

use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::rcc::{Hsi48Config, Pll, PllMul, PllPreDiv, PllRDiv, PllSource, Sysclk};

use {defmt_rtt as _, panic_probe as _};

mod app;
mod config;
mod events;
mod hid;
mod hw;
mod input;
mod text;
mod webhid;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Crystalless clock configuration for STM32G474CB.
    // Source: HSI16 (internal 16 MHz RC oscillator, no external crystal).
    // PLL: 16 MHz / 4 * 85 / 2 = 170 MHz SYSCLK.
    // USB clock: HSI48, trimmed by the clock recovery system (CRS) from the USB start-of-frame
    // packets. USB full speed needs ±0.25 %; a free-running HSI48 drifts with temperature.
    let mut config = embassy_stm32::Config::default();
    config.rcc.hse = None;
    config.rcc.hsi = true;
    config.rcc.pll = Some(Pll {
        source: PllSource::HSI,
        prediv: PllPreDiv::DIV4,
        mul: PllMul::MUL85,
        divp: None,
        divq: None,
        divr: Some(PllRDiv::DIV2), // 170 MHz
    });
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.boost = true; // Required for SYSCLK > 150 MHz
    config.rcc.hsi48 = Some(Hsi48Config { sync_from_usb: true });
    let p = embassy_stm32::init(config);
    info!("{}", crate::text::BANNER_RTT);

    // --- Take ownership of all pins up-front (so assignment lives in main.rs) ---
    // All pin/variable names use 1-based numbering matching PCB silkscreen labels.
    let pin_btn1 = p.PA7; // BTN_1
    let pin_btn2 = p.PB15; // BTN_2
    let pin_btn3 = p.PA6; // BTN_3
    let pin_btn4 = p.PB7; // BTN_4
    let pin_btn5 = p.PA4; // BTN_5
    let pin_btn6 = p.PB9; // BTN_6

    let pin_enc1_a = p.PB12; // ENC1_A
    let pin_enc1_b = p.PB13; // ENC1_B
    let pin_enc1_btn = p.PB11; // ENC1_BTN

    let pin_enc2_a = p.PB4; // ENC2_A
    let pin_enc2_b = p.PB5; // ENC2_B
    let pin_enc2_btn = p.PB3; // ENC2_BTN

    let pin_pcb_btn = p.PA0; // PCB_BTN (active-low; short=LED toggle, long=LED level next, repeat=100ms)

    let pin_pwm_out = p.PA1;

    // Shared flash owner + ECC-checked reads. Must precede any read of the reserved pages.
    hw::flash::init(p.FLASH);

    // --- Load config from flash (Slot A/B -> disabled) before spawning runtime tasks ---
    let (boot_st, persisted) = config::loader::load_slots();
    config::status::set_boot_status(boot_st).await;
    // Raw bytes of the running config, served to the WebHID configurator.
    config::runtime::set_running_crs(persisted.as_ref().map(|c| c.bytes)).await;

    let active_cfg = match persisted.map(|c| c.payload) {
        Some(p) => config::active::ActiveCfg::from_payload(&p),
        None => config::active::ActiveCfg::disabled_fallback(),
    };

    config::runtime::set_active_cfg(active_cfg).await;

    // Status LED + USB after config selection.
    // D16 is active-high (PC15 -> LED -> 5K1 -> GND). Fast blink = no valid config.
    let no_config = boot_st.source == crate::config::status::BootSource::DisabledFallback;
    hw::led::spawn_status_led(&spawner, p.PC15, false, no_config).unwrap();
    hw::usb_composite::spawn_usb_composite_cdc_hid(&spawner, p.USB, p.PA12, p.PA11).unwrap();

    // --- Construct GPIO Inputs based on config pull policy ---
    fn pull_from_u8(v: u8) -> Pull {
        match v {
            0 => Pull::None,
            2 => Pull::Down,
            _ => Pull::Up,
        }
    }

    let btn1 = Input::new(pin_btn1, pull_from_u8(active_cfg.panel_buttons[0].pull));
    let btn2 = Input::new(pin_btn2, pull_from_u8(active_cfg.panel_buttons[1].pull));
    let btn3 = Input::new(pin_btn3, pull_from_u8(active_cfg.panel_buttons[2].pull));
    let btn4 = Input::new(pin_btn4, pull_from_u8(active_cfg.panel_buttons[3].pull));
    let btn5 = Input::new(pin_btn5, pull_from_u8(active_cfg.panel_buttons[4].pull));
    let btn6 = Input::new(pin_btn6, pull_from_u8(active_cfg.panel_buttons[5].pull));

    let enc_ab_pull = pull_from_u8(active_cfg.globals.pin_pull_default);

    let enc1_a = Input::new(pin_enc1_a, enc_ab_pull);
    let enc1_b = Input::new(pin_enc1_b, enc_ab_pull);
    let enc1_btn = Input::new(
        pin_enc1_btn,
        pull_from_u8(active_cfg.encoder_buttons[0].pull),
    );

    let enc2_a = Input::new(pin_enc2_a, enc_ab_pull);
    let enc2_b = Input::new(pin_enc2_b, enc_ab_pull);
    let enc2_btn = Input::new(
        pin_enc2_btn,
        pull_from_u8(active_cfg.encoder_buttons[1].pull),
    );
    // PCB button: Pull::Up is hardcoded (PA0, button connects to GND = active LOW).
    let pcb_btn = Input::new(pin_pcb_btn, Pull::Up);

    if boot_st.source != crate::config::status::BootSource::DisabledFallback {
        let boot_led = hw::led_persist::scan_restore(&active_cfg);

        hw::pwm::init_from_cfg(
            boot_led.led_on,
            boot_led.led_level,
            active_cfg.globals.led_level_min,
            active_cfg.globals.led_level_step,
            active_cfg.globals.led_level_max,
        )
        .await;

        hw::led_persist::init_from_cfg_and_boot(&active_cfg, boot_led).await;
        if hw::led_persist::enabled_any().await {
            spawner
                .spawn(hw::led_persist::led_persist_task())
                .unwrap();
        }

        #[cfg(feature = "hw_pwm")]
        {
            let pwm = hw::pwm::new_hw_pwm_tim2_ch2(p.TIM2, pin_pwm_out);
            spawner.spawn(hw::pwm::pwm_hw_task(pwm)).unwrap();
        }

        #[cfg(not(feature = "hw_pwm"))]
        {
            let pwm_out = Output::new(pin_pwm_out, Level::Low, Speed::Low);
            spawner.spawn(hw::pwm::pwm_task(pwm_out)).unwrap();
        }
    } else {
        // In DisabledFallback, disable PWM functionality entirely.
        hw::pwm::init_from_cfg(false, 0, 0, 5, 100).await;
        let pwm_out = Output::new(pin_pwm_out, Level::Low, Speed::Low);
        spawner.spawn(hw::pwm::pwm_hold_low_task(pwm_out)).unwrap();
    }

    // One-time boot dump over RTT: same banner (already printed above) + same info as
    // `cfg status` and `cfg show`.
    app::diag::rtt_boot_dump().await;

    let scan_pins = input::scan::ScanPins {
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
    };

    spawner
        .spawn(input::scan::scan_task(active_cfg, scan_pins))
        .unwrap();

    spawner
        .spawn(input::mapper::mapper_task(active_cfg))
        .unwrap();

    loop {
        Timer::after_secs(1).await;
    }
}
