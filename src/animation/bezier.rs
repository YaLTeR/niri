use keyframe::EasingFunction;

#[derive(Debug, Clone, Copy)]
pub struct CubicBezier {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl CubicBezier {
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn x_for_t(&self, t: f64) -> f64 {
        let omt = 1. - t;
        3. * omt * omt * t * self.x1 + 3. * omt * t * t * self.x2 + t * t * t
    }

    fn y_for_t(&self, t: f64) -> f64 {
        let omt = 1. - t;
        3. * omt * omt * t * self.y1 + 3. * omt * t * t * self.y2 + t * t * t
    }

    fn t_for_x(&self, x: f64) -> f64 {
        let mut min_t = 0.;
        let mut max_t = 1.;

        for _ in 0..=30 {
            let guess_t = (min_t + max_t) / 2.;
            let guess_x = self.x_for_t(guess_t);

            if x < guess_x {
                max_t = guess_t;
            } else {
                min_t = guess_t;
            }
        }

        (min_t + max_t) / 2.
    }
}

impl EasingFunction for CubicBezier {
    fn y(&self, x: f64) -> f64 {
        match x.clamp(0., 1.) {
            0. => 0.,
            1. => 1.,
            val => self.y_for_t(self.t_for_x(val)),
        }
    }
}
