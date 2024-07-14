#![feature(array_windows)]

use std::path::PathBuf;

use pleep::spectrogram::SpectrogramIterator;
use tracing::{debug, instrument, warn};

pub mod cli;
pub mod file;

#[instrument(level = "trace", err(level = "debug"))]
pub fn get_files_in_directory(directory: &PathBuf) -> Result<Vec<PathBuf>, std::io::Error> {
    get_files_recursive(directory, directory)
}

#[instrument(skip(base), err(level = "debug"), level = "trace")]
fn get_files_recursive(
    directory: &PathBuf,
    base: &PathBuf,
) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut paths = Vec::new();

    for file in std::fs::read_dir(directory)? {
        let file = match file {
            Ok(file) => file,
            Err(error) => {
                warn!(?error, "skipping file");
                continue;
            }
        };

        let file_path = file.path();
        let file_type = file.file_type()?;

        if file_path.ends_with(".gitignore") {
            debug!(?file_path, "skipped gitignore file");
            continue;
        }

        if file_type.is_dir() {
            let mut sub_files = get_files_recursive(&directory.join(file.file_name()), base)?;
            paths.append(&mut sub_files);
        } else if file_type.is_file() {
            paths.push(file_path);
        }
    }

    Ok(paths)
}

#[instrument(skip(values), level = "trace")]
pub fn make_log<S: pleep::spectrogram::Float>(values: &[S], out_height: usize) -> Vec<S> {
    let mut new = vec![S::zero(); out_height];

    // TODO: put this value in the build file
    let a = 10.0f64;

    for (index, [last_index, next_index]) in (0..=out_height)
        .map(|index| {
            let frac = index as f64 / out_height as f64;
            ((a.powf(frac) - 1.0) / (a - 1.0) * values.len() as f64) as usize
        })
        .collect::<Vec<_>>()
        .array_windows()
        .enumerate()
    {
        // TODO: decide on the best way to find a value for a pixel
        let to_average = &values[*last_index..*next_index];
        // let average = average(to_average);

        // new[index] = average;
        new[index] = *to_average
            .iter()
            .max_by(|l, r| l.partial_cmp(r).unwrap_or(std::cmp::Ordering::Less))
            .unwrap();
        // new[index] = values[*last_index];
    }

    new
}

// TODO: remove if unused in ^^^
// fn average<S: pleep::spectrogram::Float>(values: &[S]) -> S {
//     let count = values.len();
//
//     S::from(values.iter().map(|v| v.to_f64().unwrap()).sum::<f64>() / count as f64).unwrap()
// }

#[instrument(skip(samples), level = "trace")]
pub fn generate_log_spectrogram<S: pleep::spectrogram::Float, I: Iterator<Item = S>>(
    samples: impl IntoIterator<Item = S, IntoIter = I>,
    spectrogram_settings: &pleep::spectrogram::Settings,
    settings: &LogSpectrogramSettings,
) -> LogSpectrogramIterator<S, I> {
    let spectrogram_generator = pleep::spectrogram::Generator::new();
    let spectrogram = pleep::spectrogram::SpectrogramIterator::new(
        samples.into_iter(),
        spectrogram_settings.to_owned(),
        &spectrogram_generator,
    );

    let cutoff_bin = pleep::spectrogram::get_bin_for_frequency(
        settings.frequency_cutoff as f64,
        settings.input_sample_rate,
        spectrogram_settings.fft_len,
    );
    let cutoff_bin = cutoff_bin as usize;

    LogSpectrogramIterator::new(spectrogram, settings.height, cutoff_bin)
}

pub struct LogSpectrogramIterator<S: pleep::spectrogram::Float, I: Iterator<Item = S>> {
    inner: SpectrogramIterator<S, I>,
    cutoff_bin: usize,
    height: usize,
}

impl<S: pleep::spectrogram::Float, I: Iterator<Item = S>> LogSpectrogramIterator<S, I> {
    pub fn new(spectrogram: SpectrogramIterator<S, I>, height: usize, cutoff_bin: usize) -> Self {
        Self {
            inner: spectrogram,
            height,
            cutoff_bin,
        }
    }

    pub fn height(&self) -> usize {
        self.cutoff_bin
    }
}

impl<S: pleep::spectrogram::Float, I: Iterator<Item = S>> Iterator
    for LogSpectrogramIterator<S, I>
{
    type Item = Vec<S>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|mut col| {
            col.resize(self.cutoff_bin, S::zero());

            make_log(&col, self.height)
        })
    }
}

#[derive(Debug, Clone)]
pub struct LogSpectrogramSettings {
    pub height: usize,
    pub frequency_cutoff: usize,
    pub input_sample_rate: usize,
}
