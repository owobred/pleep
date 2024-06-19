use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};

use num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use tracing::instrument;

pub trait Float: rustfft::FftNum + num_traits::Float {}
impl Float for f64 {}
impl Float for f32 {}

#[derive(Clone)]
pub struct Generator<T: Float> {
    fft_planner: Arc<Mutex<FftPlanner<T>>>,
    hanns: Arc<RwLock<HashMap<usize, Arc<Vec<T>>>>>,
}

impl<T: Float> Generator<T> {
    pub fn new() -> Self {
        Self {
            fft_planner: Arc::new(Mutex::new(rustfft::FftPlanner::new())),
            hanns: Default::default(),
        }
    }

    pub fn generate_spectrogram(&self, samples: &[T], settings: &Settings) -> Vec<Vec<T>> {
        let fft = self.get_forward_fft(settings.fft_len);
        let scratch_size = fft.get_outofplace_scratch_len();
        let mut scratch = vec![Complex::new(T::zero(), T::zero()); scratch_size];
        let hann = self.get_hann(settings.fft_len);

        let spectrogram = samples
            .into_iter()
            .map(|sample| Complex::new(*sample, T::zero()))
            .collect::<Vec<_>>()
            .windows(settings.fft_len)
            .step_by(settings.fft_len - settings.fft_overlap)
            .map(|sample_group| {
                let mut group = sample_group.to_vec();
                group
                    .iter_mut()
                    .zip(hann.iter())
                    .for_each(|(sample, &frac)| sample.re = sample.re * frac);
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

    fn get_hann(&self, size: usize) -> Arc<Vec<T>> {
        let read = self.hanns.read().unwrap();

        match read.contains_key(&size) {
            true => read.get(&size).unwrap().to_owned(),
            false => {
                drop(read);
                self.generate_hann(size)
            }
        }
    }

    #[instrument(skip(self), level = "trace")]
    fn generate_hann(&self, size: usize) -> Arc<Vec<T>> {
        let hann = generate_hanning_window(size);
        let hann = Arc::new(hann);
        let mut write = self.hanns.write().unwrap();
        write.insert(size, hann.clone());
        hann
    }
}

fn generate_hanning_window<T: Float>(size: usize) -> Vec<T> {
    let half = T::from(0.5).unwrap();
    let tau = T::from(std::f64::consts::TAU).unwrap();

    let mut out = vec![T::zero(); size];

    for i in 0..size {
        out[i] = half * (T::one() - (tau * (T::from(i).unwrap() / T::from(size).unwrap())).cos());
    }

    out
}

#[derive(Debug, Clone)]
pub struct Settings {
    fft_len: usize,
    fft_overlap: usize,
}
