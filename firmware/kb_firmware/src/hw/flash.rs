// src/hw/flash.rs
//
// Shared access to the reserved flash pages (LED journal + config slots A/B).
//
// - Single owner: the FLASH peripheral lives in one async mutex, so the LED journal writer and
//   the WebHID config commit never interleave erase/program operations.
// - ECC-checked reads: flash carries 8 ECC bits per 64-bit double word. A double word torn by a
//   power loss during programming raises an uncorrectable-ECC NMI when read. cortex-m-rt's default
//   NMI handler loops forever, which would turn one torn LED record or config slot into a boot
//   loop. The handler below records and clears the error so readers can treat the data as invalid.
//
// Single-bank mode has no read-while-write: every erase/program stalls instruction fetch (and
// therefore all tasks and ISRs) until it completes. Callers accept that pause.

use core::sync::atomic::{AtomicU32, Ordering};

use cortex_m_rt::exception;
use embassy_stm32::flash::{Blocking, Flash, FLASH_BASE};
use embassy_stm32::pac;
use embassy_stm32::peripherals::FLASH as FLASH_PERIPH;
use embassy_stm32::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

pub type BlockingFlash = Flash<'static, Blocking>;

static FLASH: Mutex<CriticalSectionRawMutex, Option<BlockingFlash>> = Mutex::new(None);

static ECC_FAULTS: AtomicU32 = AtomicU32::new(0);
static ECC_LAST_ADDR: AtomicU32 = AtomicU32::new(0);

/// Take ownership of the FLASH peripheral. Call once from `main()` before any flash access.
pub fn init(p: Peri<'static, FLASH_PERIPH>) {
    // Drop any stale ECC detection flags so the first checked read starts clean.
    clear_ecc_detection();

    // `Flash::new_blocking` panics if the chip is not in the bank mode the build expects
    // (single bank, DBANK=0). Never brick on that: boot without flash writes instead
    // (config commit reports FLASH_ERROR, LED persistence suspends).
    if cfg!(feature = "flash_single_bank") && pac::FLASH.optr().read().dbank() {
        defmt::error!("flash: chip is in DUAL-bank mode (DBANK=1); flash writes disabled");
        return;
    }

    // Uncontended at boot: no task has been spawned yet.
    if let Ok(mut g) = FLASH.try_lock() {
        *g = Some(Flash::new_blocking(p));
    }
}

/// Run `f` with exclusive access to the flash driver. Returns `None` before `init()`.
pub async fn with_flash<R>(f: impl FnOnce(&mut BlockingFlash) -> R) -> Option<R> {
    let mut g = FLASH.lock().await;
    g.as_mut().map(f)
}

/// Wait until no flash operation is in progress and keep it that way (used right before reset).
pub async fn quiesce() {
    let g = FLASH.lock().await;
    core::mem::forget(g);
}

/// Offset of an absolute flash address from `FLASH_BASE` (what the embassy flash API expects).
pub fn offset_of(addr: u32) -> u32 {
    addr - FLASH_BASE as u32
}

/// A read hit an uncorrectable ECC error (torn double word).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EccError;

/// Read `out.len()` bytes from absolute flash address `addr`, detecting ECC double errors.
pub fn read_checked(addr: u32, out: &mut [u8]) -> Result<(), EccError> {
    let before = ECC_FAULTS.load(Ordering::Acquire);
    let src = addr as *const u8;
    for (i, b) in out.iter_mut().enumerate() {
        // Safety: callers only pass addresses inside the reserved flash pages.
        *b = unsafe { core::ptr::read_volatile(src.add(i)) };
    }
    cortex_m::asm::dsb();
    cortex_m::asm::isb();

    // The NMI normally runs before we get here; also check the flags directly in case it is
    // still pending.
    let pending = ecc_detection_pending();
    if pending {
        clear_ecc_detection();
    }
    if pending || ECC_FAULTS.load(Ordering::Acquire) != before {
        Err(EccError)
    } else {
        Ok(())
    }
}

/// Number of uncorrectable ECC errors seen since boot, and the last reported fault address.
pub fn ecc_stats() -> (u32, u32) {
    (
        ECC_FAULTS.load(Ordering::Relaxed),
        ECC_LAST_ADDR.load(Ordering::Relaxed),
    )
}

fn ecc_detection_pending() -> bool {
    let r = pac::FLASH.eccr().read();
    r.eccd() || r.eccd2()
}

fn clear_ecc_detection() {
    // ECCD/ECCD2 (and ECCC/ECCC2) are write-1-to-clear; ECCCIE is preserved by the modify.
    pac::FLASH.eccr().modify(|w| {
        w.set_eccd(true);
        w.set_eccd2(true);
    });
}

#[exception]
unsafe fn NonMaskableInt() {
    // Must not touch critical-section-protected data (the NMI preempts critical sections):
    // only atomics and flash registers are used here.
    let r = pac::FLASH.eccr().read();
    if r.eccd() || r.eccd2() {
        ECC_LAST_ADDR.store(r.addr_ecc(), Ordering::Relaxed);
        ECC_FAULTS.fetch_add(1, Ordering::Release);
        clear_ecc_detection();
        return;
    }
    // Any other NMI source is unexpected on this board: reset rather than hang.
    cortex_m::peripheral::SCB::sys_reset();
}
