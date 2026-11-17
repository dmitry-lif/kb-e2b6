// src/input/encoder.rs

#[derive(Clone, Copy)]
pub struct Encoder {
    prev: u8,       // last stable AB (0..3)
    cand: u8,       // candidate raw AB
    cand_ticks: u8, // how long raw AB stayed the same
    stability: u8,  // ticks required to accept cand as stable
    acc: i8,        // accumulate quarter-steps, emit when |acc|==4
    swap_ab: bool,
}

impl Encoder {
    pub fn new(initial_ab: u8, stability_ticks: u8, swap_ab: bool) -> Self {
        Self {
            prev: initial_ab & 0x03,
            cand: initial_ab & 0x03,
            cand_ticks: 0,
            // Spec allows detent_guard_ms=0, which maps to stability_ticks=0.
            // In that case we accept changes immediately (no stability guard).
            stability: stability_ticks,
            acc: 0,
            swap_ab,
        }
    }

    #[inline]
    fn decode(prev: u8, curr: u8) -> i8 {
        const TBL: [i8; 16] = [0, -1, 1, 0, 1, 0, 0, -1, -1, 0, 0, 1, 0, 1, -1, 0];
        TBL[((prev << 2) | curr) as usize]
    }

    pub fn update(&mut self, a_high: bool, b_high: bool) -> Option<i8> {
        let (a_high, b_high) = if self.swap_ab {
            (b_high, a_high)
        } else {
            (a_high, b_high)
        };
        let raw = ((a_high as u8) << 1) | (b_high as u8);

        if raw == self.cand {
            if self.cand_ticks < u8::MAX {
                self.cand_ticks += 1;
            }
        } else {
            self.cand = raw;
            self.cand_ticks = 0;
        }

        if self.cand_ticks < self.stability {
            return None;
        }

        let curr = self.cand & 0x03;
        if curr == self.prev {
            return None;
        }

        let d = Self::decode(self.prev, curr);
        self.prev = curr;

        if d != 0 {
            self.acc = self.acc.saturating_add(d);
            self.acc = self.acc.clamp(-8, 8);

            if self.acc >= 4 {
                self.acc = 0;
                return Some(1);
            } else if self.acc <= -4 {
                self.acc = 0;
                return Some(-1);
            }
        }

        None
    }
}
