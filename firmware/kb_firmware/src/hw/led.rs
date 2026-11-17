// src/hw/led.rs

use embassy_executor::{task, Spawner};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::gpio::Pin;

pub struct StatusLed {
    pin: Output<'static>,
    active_low: bool,
}

impl StatusLed {
    pub fn new<P: Pin>(pin: embassy_stm32::Peri<'static, P>, active_low: bool) -> Self {
        let level = if active_low { Level::High } else { Level::Low };
        let pin = Output::new(pin, level, Speed::Low);
        Self { pin, active_low }
    }

    pub fn on(&mut self) {
        if self.active_low { self.pin.set_low() } else { self.pin.set_high() }
    }

    pub fn off(&mut self) {
        if self.active_low { self.pin.set_high() } else { self.pin.set_low() }
    }
}

#[task]
async fn led_task(mut led: StatusLed, no_config: bool) -> ! {
    if no_config {
        crate::app::led_patterns::no_config(&mut led).await
    } else {
        crate::app::led_patterns::heartbeat(&mut led).await
    }
}

pub fn spawn_status_led<P: Pin>(
    spawner: &Spawner,
    pin: embassy_stm32::Peri<'static, P>,
    active_low: bool,
    no_config: bool,
) -> Result<(), embassy_executor::SpawnError> {
    let led = StatusLed::new(pin, active_low);
    spawner.spawn(led_task(led, no_config))?;
    Ok(())
}
