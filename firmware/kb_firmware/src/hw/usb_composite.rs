// src/hw/usb_composite.rs

use embassy_executor::{task, Spawner};
use embassy_stm32::usb::Driver;
use embassy_stm32::Peri;
use embassy_stm32::{bind_interrupts, peripherals};
#[cfg(feature = "usb_cdc")]
use embassy_time::Timer;

#[cfg(feature = "usb_cdc")]
use embassy_usb::class::cdc_acm::{CdcAcmClass, State as CdcState};
use embassy_usb::class::hid::{Config as HidCfg, HidReaderWriter, HidWriter, State as HidState};
use embassy_usb::{Builder, Config};

use static_cell::StaticCell;

#[cfg(feature = "usb_cdc")]
use crate::app::console::{Response, PROMPT};
use crate::hid::reports::*;

pub(crate) type UsbDriver = Driver<'static, peripherals::USB>;
type UsbDevice = embassy_usb::UsbDevice<'static, UsbDriver>;
#[cfg(feature = "usb_cdc")]
type Cdc = CdcAcmClass<'static, UsbDriver>;

bind_interrupts!(struct Irqs {
    USB_LP => embassy_stm32::usb::InterruptHandler<peripherals::USB>;
});

static USB_DEVICE_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static USB_BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
static USB_CTRL_BUF: StaticCell<[u8; 128]> = StaticCell::new();

#[cfg(feature = "usb_cdc")]
static CDC_STATE: StaticCell<CdcState<'static>> = StaticCell::new();
static HID_STATE: StaticCell<HidState<'static>> = StaticCell::new();
static CFG_HID_STATE: StaticCell<HidState<'static>> = StaticCell::new();

#[cfg(feature = "usb_cdc")]
const CDC_MAX_PACKET: usize = 64;

#[task]
async fn usb_task(mut dev: UsbDevice) -> ! {
    dev.run().await
}

#[cfg(feature = "usb_cdc")]
#[task]
async fn cdc_task(mut cdc: Cdc) -> ! {
    let mut rx = [0u8; 64];
    let mut line: heapless::String<128> = heapless::String::new();

    loop {
        while !cdc.dtr() {
            Timer::after_millis(50).await;
        }

        write_all(&mut cdc, b"\r\n").await;
        write_all(&mut cdc, crate::text::BANNER_CDC).await;

        write_all(&mut cdc, PROMPT).await;

        let mut last_was_cr = false;

        loop {
            match cdc.read_packet(&mut rx).await {
                Ok(n) => {
                    for &b in &rx[..n] {
                        match b {
                            b'\n' if last_was_cr => {
                                last_was_cr = false;
                                continue;
                            }
                            b'\r' | b'\n' => {
                                last_was_cr = b == b'\r';

                                let mut cmdline: heapless::String<128> = heapless::String::new();
                                let _ = cmdline.push_str(line.as_str());
                                line.clear();

                                let cmd = cmdline.trim();

                                write_all(&mut cdc, b"\r\n").await;

                                if !cmd.is_empty() {
                                    match crate::app::console::handle_line(cmd).await {
                                        Response::None => {}
                                        Response::Static(buf) => write_all(&mut cdc, buf).await,
                                        Response::Owned(s) => {
                                            write_all(&mut cdc, s.as_bytes()).await
                                        }
                                        Response::Ok => write_all(&mut cdc, b"ok\r\n").await,
                                        Response::Err => {
                                            write_all(&mut cdc, b"ERR (try: help)\r\n").await
                                        }
                                    }
                                }

                                write_all(&mut cdc, PROMPT).await;
                            }

                            0x08 | 0x7F => {
                                last_was_cr = false;
                                if !line.is_empty() {
                                    let _ = line.pop();
                                    write_all(&mut cdc, b"\x08 \x08").await;
                                }
                            }

                            0x20..=0x7E => {
                                last_was_cr = false;
                                if line.push(char::from(b)).is_ok() {
                                    let one = [b];
                                    write_all(&mut cdc, &one).await;
                                } else {
                                    line.clear();
                                    write_all(&mut cdc, b"\r\nERR: line too long\r\n").await;
                                    write_all(&mut cdc, PROMPT).await;
                                }
                            }

                            _ => {
                                last_was_cr = false;
                            }
                        }
                    }
                }
                Err(_) => {
                    line.clear();
                    break;
                }
            }
        }
    }
}

#[cfg(feature = "usb_cdc")]
async fn write_all(cdc: &mut Cdc, mut buf: &[u8]) {
    while !buf.is_empty() {
        let n = core::cmp::min(buf.len(), CDC_MAX_PACKET);
        let chunk = &buf[..n];
        let _ = cdc.write_packet(chunk).await;
        buf = &buf[n..];
    }
}

/// Read current PWM state and forward to led_persist after any LED action.
async fn notify_led() {
    let s = crate::hw::pwm::get_state().await;
    crate::hw::led_persist::notify_pwm_state(s.on, s.level).await;
}

#[task]
async fn hid_task(mut hid: HidWriter<'static, UsbDriver, 64>) -> ! {
    use crate::events::bus::ACTION_CH;
    use crate::events::types::HidEvent;

    let boot = crate::config::status::get_boot_status().await;
    let pwm_allowed = boot.source != crate::config::status::BootSource::DisabledFallback;

    loop {
        let ev = ACTION_CH.receive().await;

        match ev {
            HidEvent::KbdPress { mods, usage } => {
                let rep = keyboard_press(mods, usage);
                let _ = hid.write(&rep).await;
            }
            HidEvent::KbdRelease => {
                let _ = hid.write(&keyboard_release()).await;
            }
            HidEvent::CcPress { usage } => {
                let rep = consumer_press(usage);
                let _ = hid.write(&rep).await;
            }
            HidEvent::CcRelease => {
                let _ = hid.write(&consumer_release()).await;
            }
            HidEvent::Internal { id } => match id {
                crate::config::action::INTERNAL_LED_TOGGLE => {
                    if pwm_allowed {
                        crate::hw::pwm::toggle().await;
                        notify_led().await;
                    }
                }
                crate::config::action::INTERNAL_LED_SWING => {
                    if pwm_allowed {
                        crate::hw::pwm::level_next().await;
                        notify_led().await;
                    }
                }
                crate::config::action::INTERNAL_LED_ON => {
                    if pwm_allowed {
                        crate::hw::pwm::set_on(true).await;
                        notify_led().await;
                    }
                }
                crate::config::action::INTERNAL_LED_OFF => {
                    if pwm_allowed {
                        crate::hw::pwm::set_on(false).await;
                        notify_led().await;
                    }
                }
                crate::config::action::INTERNAL_LED_UP => {
                    if pwm_allowed {
                        crate::hw::pwm::level_inc().await;
                        notify_led().await;
                    }
                }
                crate::config::action::INTERNAL_LED_DOWN => {
                    if pwm_allowed {
                        crate::hw::pwm::level_dec().await;
                        notify_led().await;
                    }
                }
                _ => {}
            },
        }
    }
}

pub fn spawn_usb_composite_cdc_hid(
    spawner: &Spawner,
    usb: Peri<'static, peripherals::USB>,
    dp: Peri<'static, peripherals::PA12>,
    dm: Peri<'static, peripherals::PA11>,
) -> Result<(), embassy_executor::SpawnError> {
    let driver: UsbDriver = Driver::new(usb, Irqs, dp, dm);

    let mut cfg = Config::new(crate::text::USB_VID, crate::text::USB_PID);
    cfg.manufacturer = None; // no manufacturer string, see text.rs
    cfg.product = Some(crate::text::USB_PRODUCT);
    cfg.serial_number = Some(crate::text::usb_serial());
    // bcdDevice 2.00: the interface set changed (vendor HID config interface added). A new
    // release number keeps hosts (notably Windows) from reusing descriptor data cached for 1.x.
    cfg.device_release = 0x0200;
    // Worst-case draw: the LED rail up to its eFuse limit (~135 mA nominal, ~158 mA max) plus the
    // MCU and logic (~40 mA). The embassy-usb default of 100 mA would under-report it.
    cfg.max_power = 250;

    let dev_desc = USB_DEVICE_DESC.init([0u8; 256]);
    let cfg_desc = USB_CONFIG_DESC.init([0u8; 256]);
    let bos_desc = USB_BOS_DESC.init([0u8; 256]);
    let ctrl_buf = USB_CTRL_BUF.init([0u8; 128]);

    let mut builder = Builder::new(driver, cfg, dev_desc, cfg_desc, bos_desc, ctrl_buf);

    // CDC ACM (debug channel)
    #[cfg(feature = "usb_cdc")]
    let cdc = {
        let cdc_state = CDC_STATE.init(CdcState::new());
        CdcAcmClass::new(&mut builder, cdc_state, 64)
    };

    // HID writer
    let hid_state = HID_STATE.init(HidState::new());
    let hid_cfg = HidCfg {
        report_descriptor: REPORT_DESCRIPTOR,
        request_handler: None,
        poll_ms: 4,
        max_packet_size: 64,
    };
    let hid = HidWriter::new(&mut builder, hid_state, hid_cfg);

    // Vendor-defined HID interface for the WebHID configurator (separate top-level collection,
    // so browsers do not treat it as a protected keyboard/consumer collection).
    let cfg_hid_state = CFG_HID_STATE.init(HidState::new());
    let cfg_hid_cfg = HidCfg {
        report_descriptor: crate::webhid::REPORT_DESCRIPTOR,
        request_handler: None,
        poll_ms: crate::webhid::POLL_MS,
        max_packet_size: 64,
    };
    let cfg_hid = HidReaderWriter::<_, 64, 64>::new(&mut builder, cfg_hid_state, cfg_hid_cfg);
    let (cfg_reader, cfg_writer) = cfg_hid.split();

    let usb = builder.build();

    spawner.spawn(usb_task(usb))?;
    #[cfg(feature = "usb_cdc")]
    spawner.spawn(cdc_task(cdc))?;
    spawner.spawn(hid_task(hid))?;
    spawner.spawn(crate::webhid::config_hid_task(cfg_reader, cfg_writer))?;
    Ok(())
}
