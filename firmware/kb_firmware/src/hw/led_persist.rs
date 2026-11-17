use embassy_executor::task;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant, Timer};

use crate::config::active::ActiveCfg;
use crate::config::fw::{LED_STATE_FLASH_PAGE_BASE, LED_STATE_FLASH_PAGE_SIZE};
use crate::hw::flash::{self, BlockingFlash};

const REC_MAGIC: u16 = 0x4C45; // "EL" little-endian
const REC_VERSION: u8 = 1;
const REC_SIZE: usize = 16;

const MAX_FLASH_ATTEMPTS: u8 = 10;

const DIRTY_ON: u8 = 1 << 0;
const DIRTY_LEVEL: u8 = 1 << 1;

#[derive(Clone, Copy, Debug)]
struct Rec {
    led_on: bool,
    level: u8,
    seq: u32,
}

fn is_seq_newer(a: u32, b: u32) -> bool {
    // Standard wrapping sequence comparison.
    a.wrapping_sub(b) < 0x8000_0000
}

fn parse_rec_v1(bytes: &[u8; REC_SIZE]) -> Result<Rec, ()> {
    let magic = u16::from_le_bytes([bytes[0], bytes[1]]);
    if magic == 0xFFFF {
        return Err(());
    }
    if magic != REC_MAGIC {
        return Err(());
    }
    let version = bytes[2];
    if version != REC_VERSION {
        return Err(());
    }

    let flags = bytes[3];
    let level = bytes[4];
    let reserved0 = bytes[5];
    if reserved0 != 0 {
        return Err(());
    }

    let reserved1 = u16::from_le_bytes([bytes[6], bytes[7]]);
    if reserved1 != 0 {
        return Err(());
    }

    if !(1..=100).contains(&level) {
        return Err(());
    }

    let seq = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let crc32 = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

    let computed = crs_format::crc32_bytes(&bytes[..12]);
    if computed != crc32 {
        return Err(());
    }

    Ok(Rec {
        led_on: (flags & 0x01) != 0,
        level,
        seq,
    })
}

fn make_rec_bytes_v1(led_on: bool, level: u8, seq: u32) -> [u8; REC_SIZE] {
    let mut out = [0xFFu8; REC_SIZE];

    out[0..2].copy_from_slice(&REC_MAGIC.to_le_bytes());
    out[2] = REC_VERSION;
    out[3] = if led_on { 0x01 } else { 0x00 };
    out[4] = level;
    out[5] = 0;
    out[6..8].copy_from_slice(&0u16.to_le_bytes());
    out[8..12].copy_from_slice(&seq.to_le_bytes());

    let crc = crs_format::crc32_bytes(&out[..12]);
    out[12..16].copy_from_slice(&crc.to_le_bytes());
    out
}

#[derive(Clone, Copy, Debug)]
pub struct BootRestore {
    pub found_record: bool,
    pub led_on: bool,
    pub led_level: u8,

    pub write_index: usize,
    pub next_seq: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct LedPersistStatus {
    pub enabled: bool,
    pub page_base: u32,
    pub page_size: u32,
    pub rec_size: usize,
    pub cap: usize,
    pub write_index: usize,
    pub next_seq: u32,
    pub last_seq: Option<u32>,
    pub dirty_mask: u8,
    pub pending_commit: bool,

    // Boot-time snapshot (effective values after applying policy + clamp).
    pub boot_found_record: bool,
    pub boot_on: bool,
    pub boot_level: u8,

    // Flash operation health (last attempt result).
    pub last_read_ok: Option<bool>,
    pub last_erase_ok: Option<bool>,
    pub last_write_ok: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
struct FlashOpHealth {
    last_ok: Option<bool>,
}

impl FlashOpHealth {
    const fn new() -> Self {
        Self { last_ok: None }
    }

    fn record(&mut self, _attempts_used: u8, ok: bool) {
        self.last_ok = Some(ok);
    }
}

pub fn scan_restore(cfg: &ActiveCfg) -> BootRestore {
    let state_persist = cfg.globals.led_state_persist;
    let level_persist = cfg.globals.led_level_persist;
    let enabled_any = state_persist || level_persist;
    let restore_on = cfg.globals.led_state_on_boot == 2 && state_persist;
    let restore_level = cfg.globals.led_level_on_boot == 0 && level_persist;

    let mut best: Option<Rec> = None;

    let page_base = LED_STATE_FLASH_PAGE_BASE;
    let page_size = LED_STATE_FLASH_PAGE_SIZE as usize;
    let cap = page_size / REC_SIZE;
    let mut write_index = cap;
    let mut saw_nonempty_before_empty = false;
    let mut saw_ecc_error = false;

    if enabled_any {
        for i in 0..cap {
            let mut b = [0u8; REC_SIZE];
            if flash::read_checked(page_base + (i as u32) * (REC_SIZE as u32), &mut b).is_err() {
                // Torn record (power loss mid-write): unusable, and the page must be erased
                // before further appends.
                saw_ecc_error = true;
                saw_nonempty_before_empty = true;
                continue;
            }

            let magic = u16::from_le_bytes([b[0], b[1]]);
            if magic == 0xFFFF {
                write_index = i;
                break;
            }

            // If the page isn't clean-erased and we haven't found any valid record,
            // start from a clean slate on first commit.
            saw_nonempty_before_empty = true;

            if let Ok(rec) = parse_rec_v1(&b) {
                best = match best {
                    None => Some(rec),
                    Some(cur) => {
                        if is_seq_newer(rec.seq, cur.seq) {
                            Some(rec)
                        } else {
                            Some(cur)
                        }
                    }
                };
            }
        }
    }

    // If we saw non-empty flash bytes but no valid record, force an erase on first commit.
    // This guarantees we don't keep a "partially junk" journal that reduces capacity.
    if enabled_any && best.is_none() && saw_nonempty_before_empty {
        write_index = cap;
    }
    // A torn record also forces an erase (the latest valid record is re-written as a snapshot).
    if enabled_any && saw_ecc_error {
        write_index = cap;
    }

    let found_record = best.is_some();

    // Boot policy: LED on/off.
    let led_on = if restore_on {
        best.map(|r| r.led_on).unwrap_or(false)
    } else {
        match cfg.globals.led_state_on_boot {
            1 => true,
            _ => false,
        }
    };

    // Boot policy: LED level.
    let mut level = if restore_level {
        best.map(|r| r.level)
            .unwrap_or(cfg.globals.led_level_default)
    } else {
        match cfg.globals.led_level_on_boot {
            1..=100 => cfg.globals.led_level_on_boot,
            _ => cfg.globals.led_level_default,
        }
    };

    // Clamp per spec.
    let min = cfg.globals.led_level_min.max(1);
    let mut max = cfg.globals.led_level_max.max(1);
    if min > max {
        max = min;
    }
    level = level.clamp(min, max);
    if level == 0 {
        level = 1;
    }

    let next_seq = match best {
        Some(r) => r.seq.wrapping_add(1),
        None => 0,
    };

    if enabled_any {
        defmt::info!(
            "led_persist: restore scan found={} write_idx={} seq_next={} on={} lvl={}",
            found_record as u8,
            write_index,
            next_seq,
            led_on as u8,
            level
        );
    }

    BootRestore {
        found_record,
        led_on,
        led_level: level,
        write_index,
        next_seq,
    }
}

#[derive(Clone, Copy, Debug)]
struct PersistCfg {
    state_persist: bool,
    level_persist: bool,
    autosave_delay_ms: u16,
    level_min: u8,
    level_max: u8,
}

#[derive(Clone, Copy, Debug)]
struct PersistState {
    cfg: PersistCfg,
    boot_found_record: bool,
    boot_on: bool,
    boot_level: u8,
    current_on: bool,
    current_level: u8,
    dirty_mask: u8,
    commit_deadline: Option<Instant>,
    next_seq: u32,
    last_seq: Option<u32>,
    write_index: usize,
    cap: usize,

    // Flash operation tracking + failure clamp.
    flash_read: FlashOpHealth,
    flash_erase: FlashOpHealth,
    flash_write: FlashOpHealth,
    flash_suspended: bool,
}

impl PersistState {
    const fn empty() -> Self {
        Self {
            cfg: PersistCfg {
                state_persist: false,
                level_persist: false,
                autosave_delay_ms: 0,
                level_min: 1,
                level_max: 100,
            },
            boot_found_record: false,
            boot_on: false,
            boot_level: 1,
            current_on: false,
            current_level: 1,
            dirty_mask: 0,
            commit_deadline: None,
            next_seq: 0,
            last_seq: None,
            write_index: 0,
            cap: 0,

            flash_read: FlashOpHealth::new(),
            flash_erase: FlashOpHealth::new(),
            flash_write: FlashOpHealth::new(),
            flash_suspended: false,
        }
    }
}

static STATE: Mutex<CriticalSectionRawMutex, PersistState> = Mutex::new(PersistState::empty());
static KICK: Channel<CriticalSectionRawMutex, (), 1> = Channel::new();

pub async fn init_from_cfg_and_boot(cfg: &ActiveCfg, boot: BootRestore) {
    let state_persist = cfg.globals.led_state_persist;
    let level_persist = cfg.globals.led_level_persist;

    let page_size = LED_STATE_FLASH_PAGE_SIZE as usize;
    let cap = page_size / REC_SIZE;

    let mut s = STATE.lock().await;
    s.cfg = PersistCfg {
        state_persist,
        level_persist,
        autosave_delay_ms: cfg.globals.led_autosave_delay_ms,
        level_min: cfg.globals.led_level_min.max(1),
        level_max: cfg.globals.led_level_max.max(1),
    };
    if s.cfg.level_min > s.cfg.level_max {
        s.cfg.level_max = s.cfg.level_min;
    }

    s.boot_found_record = boot.found_record;
    s.boot_on = boot.led_on;
    s.boot_level = boot.led_level;
    s.current_on = boot.led_on;
    s.current_level = boot.led_level;
    s.dirty_mask = 0;
    s.commit_deadline = None;
    s.next_seq = boot.next_seq;
    s.last_seq = if boot.found_record {
        Some(boot.next_seq.wrapping_sub(1))
    } else {
        None
    };
    s.write_index = boot.write_index.min(cap);
    s.cap = cap;

    s.flash_suspended = false;
    if state_persist || level_persist {
        // Treat "no valid record found" as a read failure for operator visibility.
        s.flash_read.record(1, boot.found_record);
    } else {
        s.flash_read = FlashOpHealth::new();
        s.flash_erase = FlashOpHealth::new();
        s.flash_write = FlashOpHealth::new();
    }

    defmt::info!(
        "led_persist: init en_s={} en_l={} autosave_ms={} min={} max={} on={} lvl={} idx={} seq_next={}",
        state_persist as u8,
        level_persist as u8,
        s.cfg.autosave_delay_ms,
        s.cfg.level_min,
        s.cfg.level_max,
        s.current_on as u8,
        s.current_level,
        s.write_index,
        s.next_seq
    );
}

pub async fn get_status() -> LedPersistStatus {
    let s = *STATE.lock().await;
    LedPersistStatus {
        enabled: s.cfg.state_persist || s.cfg.level_persist,
        page_base: LED_STATE_FLASH_PAGE_BASE,
        page_size: LED_STATE_FLASH_PAGE_SIZE,
        rec_size: REC_SIZE,
        cap: s.cap,
        write_index: s.write_index,
        next_seq: s.next_seq,
        last_seq: s.last_seq,
        dirty_mask: s.dirty_mask,
        pending_commit: s.commit_deadline.is_some(),
        boot_found_record: s.boot_found_record,
        boot_on: s.boot_on,
        boot_level: s.boot_level,

        last_read_ok: s.flash_read.last_ok,
        last_erase_ok: s.flash_erase.last_ok,
        last_write_ok: s.flash_write.last_ok,
    }
}

pub async fn notify_pwm_state(on: bool, level: u8) {
    let mut s = STATE.lock().await;

    let old_on = s.current_on;
    let old_level = s.current_level;

    // Clamp level before comparing/storing.
    let mut lvl = level;
    if lvl == 0 {
        lvl = 1;
    }
    lvl = lvl.clamp(s.cfg.level_min.max(1), s.cfg.level_max.max(1));

    s.current_on = on;
    s.current_level = lvl;

    if s.flash_suspended {
        // Keep runtime state updated, but stop attempting flash I/O after repeated failures.
        return;
    }

    let mut changed_mask = 0;
    if on != old_on {
        changed_mask |= DIRTY_ON;
    }
    if lvl != old_level {
        changed_mask |= DIRTY_LEVEL;
    }

    let mut dirty = 0;
    if (changed_mask & DIRTY_ON) != 0 && s.cfg.state_persist {
        dirty |= DIRTY_ON;
    }
    if (changed_mask & DIRTY_LEVEL) != 0 && s.cfg.level_persist {
        dirty |= DIRTY_LEVEL;
    }

    if dirty == 0 {
        return;
    }

    s.dirty_mask |= dirty;

    let delay = Duration::from_millis(s.cfg.autosave_delay_ms as u64);
    let now = Instant::now();
    let deadline = now + delay;
    s.commit_deadline = Some(match s.commit_deadline {
        None => deadline,
        Some(cur) => {
            if deadline > cur {
                deadline
            } else {
                cur
            }
        }
    });

    let _ = KICK.try_send(());
}

fn page_offset() -> u32 {
    flash::offset_of(LED_STATE_FLASH_PAGE_BASE)
}

fn append_record(
    flash: &mut BlockingFlash,
    write_index: usize,
    seq: u32,
    led_on: bool,
    level: u8,
) -> Result<(), embassy_stm32::flash::Error> {
    let off = page_offset() + (write_index as u32) * (REC_SIZE as u32);
    let rec = make_rec_bytes_v1(led_on, level, seq);
    flash.blocking_write(off, &rec)
}

fn erase_page(flash: &mut BlockingFlash) -> Result<(), embassy_stm32::flash::Error> {
    let off = page_offset();
    flash.blocking_erase(off, off + LED_STATE_FLASH_PAGE_SIZE)
}

/// Single writer of the LED journal page. The flash driver itself is shared (`hw::flash`) with
/// the WebHID config commit; it is locked per operation, never across a retry delay.
#[task]
pub async fn led_persist_task() -> ! {
    loop {
        // Wait until there's something to do.
        let (dirty, deadline) = {
            let s = *STATE.lock().await;
            (s.dirty_mask, s.commit_deadline)
        };

        if dirty == 0 {
            let _ = KICK.receive().await;
            continue;
        }

        if let Some(dl) = deadline {
            let now = Instant::now();
            if now < dl {
                let _ = embassy_futures::select::select(async { KICK.receive().await }, async {
                    Timer::at(dl).await;
                })
                .await;
                continue;
            }
        }

        // Snapshot state to commit.
        let (led_on, level, seq, mut idx, cap, committed_dirty) = {
            let mut s = STATE.lock().await;

            if s.flash_suspended {
                // Drop pending persistence attempts when suspended.
                s.dirty_mask = 0;
                s.commit_deadline = None;
                continue;
            }

            // Take ownership of current dirty bits so we don't lose updates that arrive during commit.
            let committed_dirty = s.dirty_mask;
            s.dirty_mask = 0;

            // Clear deadline now; if we get new changes during commit, notify() will set it again.
            s.commit_deadline = None;
            (
                s.current_on,
                s.current_level,
                s.next_seq,
                s.write_index,
                s.cap,
                committed_dirty,
            )
        };

        if committed_dirty == 0 {
            // Nothing to commit (can happen if another loop iteration committed just before we got here).
            continue;
        }

        // Ensure there's space.
        if idx >= cap {
            defmt::info!("led_persist: erasing journal page");

            let mut ok = false;
            for attempt in 1..=MAX_FLASH_ATTEMPTS {
                let res = flash::with_flash(erase_page)
                    .await
                    .unwrap_or(Err(embassy_stm32::flash::Error::Seq));
                {
                    let mut s = STATE.lock().await;
                    s.flash_erase.record(attempt, res.is_ok());
                }
                if res.is_ok() {
                    ok = true;
                    break;
                }
                defmt::warn!(
                    "led_persist: erase failed; attempt {}/{}",
                    attempt,
                    MAX_FLASH_ATTEMPTS
                );
                Timer::after_millis(200).await;
            }

            if !ok {
                defmt::error!(
                    "led_persist: erase failed; retry limit reached, suspending persistence"
                );
                {
                    let mut s = STATE.lock().await;
                    s.flash_suspended = true;
                    s.dirty_mask = 0;
                    s.commit_deadline = None;
                }
                continue;
            }
            idx = 0;
        }

        let mut write_ok = false;
        for attempt in 1..=MAX_FLASH_ATTEMPTS {
            let res = flash::with_flash(|f| append_record(f, idx, seq, led_on, level))
                .await
                .unwrap_or(Err(embassy_stm32::flash::Error::Seq));
            {
                let mut s = STATE.lock().await;
                s.flash_write.record(attempt, res.is_ok());
            }
            if res.is_ok() {
                write_ok = true;
                break;
            }
            defmt::warn!(
                "led_persist: write failed; attempt {}/{}",
                attempt,
                MAX_FLASH_ATTEMPTS
            );
            Timer::after_millis(200).await;
        }

        if !write_ok {
            defmt::error!("led_persist: write failed; retry limit reached, suspending persistence");
            {
                let mut s = STATE.lock().await;
                s.flash_suspended = true;
                s.dirty_mask = 0;
                s.commit_deadline = None;
            }
            continue;
        }

        defmt::info!(
            "led_persist: saved seq={} idx={} on={} lvl={}",
            seq,
            idx,
            led_on as u8,
            level
        );

        // Commit success: advance pointers and clear dirty.
        {
            let mut s = STATE.lock().await;
            // Advance pointers even if another task updated current state.
            s.last_seq = Some(seq);
            s.next_seq = seq.wrapping_add(1);
            s.write_index = (idx + 1).min(cap);
            // IMPORTANT: do not clear s.dirty_mask here.
            // We cleared it when taking the snapshot, but new changes may have arrived while
            // we were writing to flash.
        }
    }
}

pub async fn enabled_any() -> bool {
    let s = STATE.lock().await;
    s.cfg.state_persist || s.cfg.level_persist
}
