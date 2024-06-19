use std::sync::{Arc, Mutex};

use num_complex::Complex;
use rustfft::{Fft, FftPlanner};

pub trait Float: rustfft::FftNum + num_traits::Float {}
impl Float for f64 {}
impl Float for f32 {}

#[derive(Clone)]
pub struct Generator<T: Float> {
    fft_planner: Arc<Mutex<FftPlanner<T>>>,
}

impl<T: Float> Generator<T> {
    pub fn new() -> Self {
        Self {
            fft_planner: Arc::new(Mutex::new(rustfft::FftPlanner::new())),
        }
    }

    pub fn generate_spectrogram(&self, samples: &[T], settings: &Settings) -> Vec<Vec<T>> {
        let fft = self.get_forward_fft(settings.fft_len);
        let scratch_size = fft.get_outofplace_scratch_len();
        let mut scratch = vec![Complex::new(T::zero(), T::zero()); scratch_size];

        let spectrogram = samples
            .into_iter()
            .map(|sample| Complex::new(*sample, T::zero()))
            .collect::<Vec<_>>()
            .windows(settings.fft_len)
            .step_by(settings.fft_len - settings.fft_overlap)
            .map(|sample_group| {
                let mut group = sample_group.to_vec();
                fft.process_with_scratch(&mut group, &mut scratch);
                group
            })
            .map(|group| {
                group
                    .into_iter()
                    .take(settings.fft_len / 2)
                    .map(|v| v.norm())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        spectrogram
    }

    fn get_forward_fft(&self, len: usize) -> Arc<dyn Fft<T>> {
        let mut planner = self.fft_planner.lock().unwrap();

        planner.plan_fft_forward(len)
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    fft_len: usize,
    fft_overlap: usize,
}
