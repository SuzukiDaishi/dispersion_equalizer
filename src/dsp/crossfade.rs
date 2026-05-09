#[derive(Clone, Copy, Debug)]
pub struct Crossfade {
    pos: u32,
    len: u32,
    active: bool,
}

impl Default for Crossfade {
    fn default() -> Self {
        Self {
            pos: 0,
            len: 1,
            active: false,
        }
    }
}

impl Crossfade {
    pub fn start(&mut self, sample_rate: f32, time_ms: f32) {
        self.len = ((sample_rate * time_ms / 1000.0).round() as u32).max(1);
        self.pos = 0;
        self.active = time_ms > 0.0;
    }

    pub fn reset(&mut self) {
        self.pos = 0;
        self.active = false;
    }

    pub fn next(&mut self) -> f32 {
        if !self.active {
            return 1.0;
        }

        let t = (self.pos as f32 / self.len as f32).clamp(0.0, 1.0);
        self.pos = self.pos.saturating_add(1);
        if self.pos >= self.len {
            self.active = false;
        }
        t
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}
