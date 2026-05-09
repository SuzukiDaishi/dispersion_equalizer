use arrayvec::ArrayVec;

#[derive(Clone, Copy, Debug, Default)]
pub enum SectionDescriptor {
    /// RBJ biquad all-pass: freq_hz + Q.  Matches preview.html coefficients.
    SecondOrder { freq_hz: f32, q: f32 },
    #[default]
    Bypass,
}

#[derive(Clone, Debug)]
pub struct RuntimeChainDescriptor {
    pub global_delay_ms: f32,
    pub sections: ArrayVec<SectionDescriptor, { crate::dsp::MAX_RUNTIME_SECTIONS }>,
}

impl Default for RuntimeChainDescriptor {
    fn default() -> Self {
        Self {
            global_delay_ms: 0.0,
            sections: ArrayVec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PreviewCurve {
    pub target_points: Vec<[f32; 2]>,
    pub actual_points: Vec<[f32; 2]>,
    pub fit_error_ms: f32,
    pub section_count: usize,
    pub pure_delay_ms: f32,
}
