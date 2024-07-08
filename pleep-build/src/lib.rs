use std::path::PathBuf;

use tracing::{instrument, trace, warn};

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
pub fn make_log(values: &[f32], new_size: usize) -> Vec<f32> {
    let last_point_ln = (values.len() as f32).ln();
    let mut new = vec![0.0; new_size];

    for index in 0..new_size {
        let point = (index as f32).ln() / last_point_ln * values.len() as f32;
        new[index] = values[point as usize];
    }

    new
}

#[instrument(skip(samples), level = "trace")]
pub fn generate_log_spectrogram(
    samples: impl IntoIterator<Item = f32>,
    spectrogram_settings: &pleep::spectrogram::Settings,
    settings: &LogSpectrogramSettings,
) -> Vec<Vec<f32>> {
    let spectrogram_generator = pleep::spectrogram::Generator::new();
    let mut spectrogram = pleep::spectrogram::SpectrogramIterator::new(
        samples.into_iter(),
        spectrogram_settings.to_owned(),
        spectrogram_generator,
    )
    .collect::<Vec<_>>();

    let spectrogram_height = spectrogram_settings.fft_len / 2;
    let cutoff_bin = pleep::spectrogram::get_bin_for_frequency(
        settings.frequency_cutoff as f64,
        settings.input_sample_rate,
        spectrogram_settings.fft_len,
    );
    let cutoff_bin = cutoff_bin as usize;

    match cutoff_bin.cmp(&spectrogram_height) {
        std::cmp::Ordering::Greater => {
            let to_add = cutoff_bin - spectrogram_height;
            trace!(to_add, "growing spectrogram");
            let to_add = vec![0.0; to_add];
            spectrogram.iter_mut().for_each(|col| {
                col.extend(&to_add);
            });
        }
        std::cmp::Ordering::Equal => trace!("spectrogram height matched cutoff bin"),
        std::cmp::Ordering::Less => {
            trace!(
                to_remove = spectrogram_height - cutoff_bin,
                "shrinking spectrogram"
            );
            spectrogram.iter_mut().for_each(|col| {
                col.truncate(cutoff_bin);
                col.shrink_to(cutoff_bin);
            });
        }
    }

    let log_spectrogram = spectrogram
        .into_iter()
        .map(|col| make_log(&col, settings.height))
        .collect::<Vec<_>>();

    log_spectrogram
}

#[derive(Debug, Clone)]
pub struct LogSpectrogramSettings {
    pub height: usize,
    pub frequency_cutoff: usize,
    pub input_sample_rate: usize,
}
