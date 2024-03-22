pub struct WheelTracker {
    last: f64,
    acc: f64,
}

impl WheelTracker {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { last: 0., acc: 0. }
    }

    pub fn accumulate(&mut self, amount_v120: f64) -> i8 {
        let changed_direction =
            (self.last > 0. && amount_v120 < 0.) || (self.last < 0. && amount_v120 > 0.);
        if changed_direction {
            self.acc = 0.
        }

        self.last = amount_v120;
        self.acc += amount_v120;

        let mut ticks = 0;
        if self.acc.abs() >= 120. {
            let clamped = self.acc.clamp(-127. * 120., 127. * 120.);
            ticks = (clamped as i16 / 120) as i8;
            self.acc %= 120.;
        }

        ticks
    }

    pub fn reset(&mut self) {
        self.last = 0.;
        self.acc = 0.;
    }
}
