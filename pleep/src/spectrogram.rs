use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, RwLock},
};

use num_complex::Complex;
use num_traits::Zero;
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
            hanns: Arc::default(),
        }
    }

    fn get_forward_fft(&self, len: usize) -> Arc<dyn Fft<T>> {
        let mut planner = self.fft_planner.lock().unwrap();

        planner.plan_fft_forward(len)
    }

    fn get_hann(&self, size: usize) -> Arc<Vec<T>> {
        let read = self.hanns.read().unwrap();

        if read.contains_key(&size) {
            read.get(&size).unwrap().to_owned()
        } else {
            drop(read);
            self.generate_hann(size)
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

impl<T: Float> Default for Generator<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn generate_hanning_window<T: Float>(size: usize) -> Vec<T> {
    let half = T::from(0.5).unwrap();
    let tau = T::from(std::f64::consts::TAU).unwrap();

    let mut out = vec![T::zero(); size];

    for (i, item) in out.iter_mut().enumerate() {
        *item = half * (T::one() - (tau * (T::from(i).unwrap() / T::from(size).unwrap())).cos());
    }

    out
}

pub fn get_frequency_for_bin(bin: usize, sample_rate: usize, fft_len: usize) -> f64 {
    (bin * sample_rate) as f64 / fft_len as f64
}

pub fn get_bin_for_frequency(frequency: f64, sample_rate: usize, fft_len: usize) -> f64 {
    (frequency * fft_len as f64) / sample_rate as f64
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub fft_len: usize,
    pub fft_overlap: usize,
}

pub struct SpectrogramIterator<S: Float, T: Iterator<Item = S>> {
    buffer: VecDeque<S>,
    fft_scratch: Vec<Complex<S>>,
    inner: T,
    settings: Settings,
    hann: Vec<S>,
    fft: Arc<dyn Fft<S>>,
}

impl<S: Float, T: Iterator<Item = S>> SpectrogramIterator<S, T> {
    pub fn new(wraps: T, settings: Settings, generator: &Generator<S>) -> Self {
        let fft = generator.get_forward_fft(settings.fft_len);
        let hann = generator.get_hann(settings.fft_len).to_vec();

        Self {
            buffer: VecDeque::with_capacity(settings.fft_len),
            fft_scratch: vec![Complex::zero(); settings.fft_len],
            inner: wraps,
            settings,
            hann,
            fft,
        }
    }

    fn generate_spectrogram_col(
        &mut self,
        samples: impl IntoIterator<Item = Complex<S>>,
    ) -> Vec<S> {
        let mut hanned = samples
            .into_iter()
            .zip(self.hann.iter())
            .map(|(sample, hann)| sample * *hann)
            .collect::<Vec<Complex<S>>>();
        self.fft
            .process_with_scratch(&mut hanned, &mut self.fft_scratch);
        hanned
            .into_iter()
            .take(self.settings.fft_len / 2)
            .map(num_complex::Complex::norm)
            .map(|v| v / S::from(self.hann.len()).unwrap().sqrt())
            .collect::<Vec<_>>()
    }
}

impl<S: Float, T: Iterator<Item = S>> Iterator for SpectrogramIterator<S, T> {
    type Item = Vec<S>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next_sample = self.inner.next();

            match next_sample {
                Some(sample) => self.buffer.push_back(sample),
                None => {
                    if self.buffer.is_empty() {
                        return None;
                    } else {
                        self.buffer.resize(self.settings.fft_len, S::zero())
                    }
                }
            };

            if self.buffer.len() >= self.settings.fft_len {
                break;
            }
        }

        assert!(self.buffer.len() <= self.settings.fft_len);

        let samples = self
            .buffer
            .iter()
            .take(self.settings.fft_len)
            .copied()
            .map(|s| Complex::new(s, S::zero()))
            .collect::<Vec<_>>();

        self.buffer.drain(..self.settings.fft_len).for_each(drop);

        let spectrogram = self.generate_spectrogram_col(samples);

        Some(spectrogram)
    }
}
