// src/input/debounce.rs

pub struct Debounce {
    stable: bool,
    last_raw: bool,
    cnt: u8,
    threshold: u8,
}

impl Debounce {
    pub const fn new(initial: bool, threshold: u8) -> Self {
        Self {
            stable: initial,
            last_raw: initial,
            cnt: 0,
            threshold,
        }
    }

    pub fn update(&mut self, raw: bool) -> Option<bool> {
        if raw == self.last_raw {
            if self.cnt < self.threshold {
                self.cnt += 1;
            }
        } else {
            self.last_raw = raw;
            self.cnt = 0;
        }

        if self.cnt == self.threshold && self.stable != self.last_raw {
            self.stable = self.last_raw;
            return Some(self.stable);
        }

        None
    }
}
