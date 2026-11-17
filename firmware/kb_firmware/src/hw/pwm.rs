use embassy_executor::task;
use embassy_stm32::gpio::Output;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};

#[cfg(feature = "hw_pwm")]
use embassy_stm32::gpio::OutputType;

#[cfg(feature = "hw_pwm")]
use embassy_stm32::{peripherals, Peri};

#[cfg(feature = "hw_pwm")]
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};

#[cfg(feature = "hw_pwm")]
use embassy_stm32::timer::low_level::CountingMode;

#[cfg(feature = "hw_pwm")]
use embassy_stm32::time::hz;

#[derive(Clone, Copy, Debug)]
pub struct PwmState {
    pub on: bool,
    pub level: u8,     // 0..=100 (percent)
    pub min_level: u8, // 0..=100 (0 disables the minimum)
    pub max_level: u8, // 1..=100
    pub step: u8,      // 1..=100 (percent)
}

impl PwmState {
    pub const fn new() -> Self {
        Self {
            on: false,
            level: 0,
            min_level: 0,
            max_level: 100,
            step: 5,
        }
    }
}

static PWM_STATE: Mutex<CriticalSectionRawMutex, PwmState> = Mutex::new(PwmState::new());

const PWM_MAX: u8 = 100;

#[cfg(feature = "hw_pwm")]
const HW_PWM_FREQ_HZ: u32 = 1_000;

pub async fn init_from_cfg(
    pwm_on_boot: bool,
    pwm_level: u8,
    pwm_min_level: u8,
    pwm_step: u8,
    pwm_max_level: u8,
) {
    let mut s = PWM_STATE.lock().await;
    s.on = pwm_on_boot;
    s.min_level = pwm_min_level.min(PWM_MAX);
    s.max_level = pwm_max_level.clamp(1, PWM_MAX);
    if s.min_level != 0 && s.min_level > s.max_level {
        s.min_level = s.max_level;
    }

    s.step = pwm_step.clamp(1, PWM_MAX);
    s.level = pwm_level.min(PWM_MAX);

    if s.level > s.max_level {
        s.level = s.max_level;
    }
    if s.min_level != 0 && s.level < s.min_level {
        s.level = s.min_level;
    }
}

#[cfg(feature = "hw_pwm")]
pub fn new_hw_pwm_tim2_ch2(
    tim2: Peri<'static, peripherals::TIM2>,
    pin_pa1: Peri<'static, peripherals::PA1>,
) -> SimplePwm<'static, peripherals::TIM2> {
    // PA1 is commonly available as TIM2_CH2 on STM32G4 via AF1.
    // If your board routes PWM elsewhere, change the timer/channel/pin here.
    SimplePwm::new(
        tim2,
        None,
        Some(PwmPin::new(pin_pa1, OutputType::PushPull)),
        None,
        None,
        hz(HW_PWM_FREQ_HZ),
        CountingMode::EdgeAlignedUp,
    )
}

#[cfg(feature = "hw_pwm")]
#[task]
pub async fn pwm_hw_task(mut pwm: SimplePwm<'static, peripherals::TIM2>) -> ! {
    // HW PWM runs continuously; we just update duty when state changes.
    // Channel 2 is used (PA1).
    let mut ch2 = pwm.ch2();

    // Keep channel enabled; duty=0 behaves like output-low.
    ch2.enable();
    let max = ch2.max_duty_cycle();

    let mut last_on: bool = false;
    let mut last_level: u8 = 0;

    loop {
        let s = *PWM_STATE.lock().await;

        let duty: u32 = if !s.on || s.level == 0 {
            0
        } else if s.level >= PWM_MAX {
            max
        } else {
            // Scale 0..=100 to 0..=max.
            (max * (s.level as u32)) / (PWM_MAX as u32)
        };

        if s.on != last_on || s.level != last_level {
            ch2.set_duty_cycle(duty);
            last_on = s.on;
            last_level = s.level;
        }

        // Polling is cheap; no need for a channel right now.
        Timer::after_millis(20).await;
    }
}

pub async fn get_state() -> PwmState {
    *PWM_STATE.lock().await
}

pub async fn toggle() {
    let mut s = PWM_STATE.lock().await;
    s.on = !s.on;
}

pub async fn set_on(on: bool) {
    let mut s = PWM_STATE.lock().await;
    s.on = on;
}

pub async fn level_next() {
    let mut s = PWM_STATE.lock().await;

    let max_level = s.max_level.clamp(1, PWM_MAX);
    let min = s.min_level.min(max_level);
    let mut cur = s.level.min(max_level);
    let step = s.step.clamp(1, max_level);

    // Degenerate case: min == max => level is fixed.
    if min >= max_level {
        s.level = max_level;
        return;
    }

    if cur < min {
        cur = min;
    }

    // "Next" level (SWING): step up; if we'd exceed max, wrap to min.
    // This matches the documented LED_SWING behavior.
    let next = cur.saturating_add(step);
    s.level = if next > max_level { min } else { next };
}

pub async fn level_inc() {
    let mut s = PWM_STATE.lock().await;

    let max_level = s.max_level.clamp(1, PWM_MAX);
    let min = s.min_level.min(max_level);
    let cur = s.level.min(max_level);
    let step = s.step.clamp(1, max_level);

    s.level = if cur < min {
        min
    } else {
        cur.saturating_add(step).min(max_level)
    };
}

pub async fn level_dec() {
    let mut s = PWM_STATE.lock().await;

    let max_level = s.max_level.clamp(1, PWM_MAX);
    let min = s.min_level.min(max_level);
    let cur = s.level.min(max_level);
    let step = s.step.clamp(1, max_level);

    s.level = if cur <= min {
        min
    } else {
        cur.saturating_sub(step).max(min)
    };
}

#[task]
pub async fn pwm_task(mut out: Output<'static>) -> ! {
    // Software PWM at 100Hz (10ms period). Good enough for a debug/aux pin.
    let period = Duration::from_millis(10);

    loop {
        let s = *PWM_STATE.lock().await;

        if !s.on || s.level == 0 {
            out.set_low();
            Timer::after(period).await;
            continue;
        }

        if s.level >= PWM_MAX {
            out.set_high();
            Timer::after(period).await;
            continue;
        }

        let on_us = (period.as_micros() as u32 * (s.level as u32)) / (PWM_MAX as u32);
        let off_us = (period.as_micros() as u32).saturating_sub(on_us);

        out.set_high();
        if on_us > 0 {
            Timer::after_micros(on_us as u64).await;
        }
        out.set_low();
        if off_us > 0 {
            Timer::after_micros(off_us as u64).await;
        }
    }
}

#[task]
pub async fn pwm_hold_low_task(mut out: Output<'static>) -> ! {
    out.set_low();
    loop {
        Timer::after_secs(3600).await;
    }
}
