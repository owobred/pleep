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
pub fn make_log<S: pleep::spectrogram::Float>(values: &[S], log_indexes: &[S]) -> Vec<S> {
    let last_point_ln = S::from(values.len()).unwrap().ln();
    let mut new = vec![S::zero(); log_indexes.len()];

    for (index, log_index) in log_indexes.into_iter().enumerate() {
        let point = *log_index / last_point_ln * S::from(values.len()).unwrap();
        new[index] = values[point.to_usize().unwrap_or(0)];
    }

    new
}

pub fn gen_log_indexes<S: pleep::spectrogram::Float>(start_at: usize, end_at: usize) -> Vec<S> {
    (start_at..=end_at)
        .into_iter()
        .map(|index| S::from(index).unwrap().ln())
        .collect()
}

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
        spectrogram_generator,
    );

    let cutoff_bin = pleep::spectrogram::get_bin_for_frequency(
        settings.frequency_cutoff as f64,
        settings.input_sample_rate,
        spectrogram_settings.fft_len,
    );
    let cutoff_bin = cutoff_bin as usize;

    LogSpectrogramIterator::new(spectrogram, settings.to_owned(), cutoff_bin)
}

pub struct LogSpectrogramIterator<S: pleep::spectrogram::Float, I: Iterator<Item = S>> {
    inner: SpectrogramIterator<S, I>,
    cutoff_bin: usize,
    log_indexes: Vec<S>,
}

impl<S: pleep::spectrogram::Float, I: Iterator<Item = S>> LogSpectrogramIterator<S, I> {
    pub fn new(
        spectrogram: SpectrogramIterator<S, I>,
        settings: LogSpectrogramSettings,
        cutoff_bin: usize,
    ) -> Self {
        let log_indexes = gen_log_indexes(0, settings.height - 1);

        Self {
            inner: spectrogram,
            log_indexes,
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

            make_log(&col, &self.log_indexes)
        })
    }
}

#[derive(Debug, Clone)]
pub struct LogSpectrogramSettings {
    pub height: usize,
    pub frequency_cutoff: usize,
    pub input_sample_rate: usize,
}
