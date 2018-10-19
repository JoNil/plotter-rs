use time;

#[derive(Debug)]
pub struct Timer {
    start: f64,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            start: time::precise_time_s(),
        }
    }

    pub fn reset(&mut self) -> f64 {
        let now = time::precise_time_s();
        let delta = now - self.start;
        self.start = now;
        delta
    }
}
