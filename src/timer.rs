pub struct Timer {
    pub start_val: u8,
    pub current_val: u8,
    count_ticks: f64,
}

impl Timer {
    pub fn new() -> Self {
        Timer {
            start_val: 0,
            current_val: 0,
            count_ticks: 0.0,
        }
    }

    pub fn set(&mut self, val: u8) {
        self.start_val = val;
        self.current_val = val;
        self.count_ticks = 0.0;
    }

    pub fn tick(&mut self, tick_rate: u64) -> u8 {

        let ticks_per_decrement = tick_rate as f64 / 60.0; // Timer is supposed to decrement at 60Hz.
        self.count_ticks += 1.0;

        if self.current_val > 0 && self.count_ticks >= ticks_per_decrement {
            self.current_val -= 1;
            self.count_ticks = 0.0;
        }

        self.current_val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_test() {

        let mut timer = Timer::new();
        timer.set(120);

        for i in 0..400 {
            let val = timer.tick(180);

            if i == 1 {
                assert_eq!(120, val);
            }

            // 180 tick/sec means a 60Hz timer will decrement every 3 ticks
            if i == 2 {
                assert_eq!(119, val);
            }

            // At 360 ticks and 180 tick/sec, a timer with a value of 120 should be fully depleted.
            if i >= 359 {
                assert_eq!(0, val);
            }
        }
    }
}