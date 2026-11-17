use embassy_time::Timer;
use crate::hw::led::StatusLed;

/// Running with a valid config: two short flashes once per second.
pub async fn heartbeat(led: &mut StatusLed) -> ! {
    loop {
        led.on();  Timer::after_millis(60).await;
        led.off(); Timer::after_millis(140).await;

        led.on();  Timer::after_millis(60).await;
        led.off(); Timer::after_millis(740).await;
    }
}

/// Running without a valid config (DisabledFallback): fast even blink, 5 per second.
pub async fn no_config(led: &mut StatusLed) -> ! {
    loop {
        led.on();  Timer::after_millis(100).await;
        led.off(); Timer::after_millis(100).await;
    }
}