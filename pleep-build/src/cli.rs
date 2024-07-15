use std::{path::PathBuf, time::Duration};

use tracing::instrument;

use crate::LogSpectrogramIterator;

const DEFAULT_SAMPLE_RATE: usize = 2 << 14;
const DEFAULT_FFT_SIZE: usize = DEFAULT_SAMPLE_RATE;
const DEFAULT_FTT_OVERLAP: usize = DEFAULT_FFT_SIZE / 4;
const DEFAULT_MAX_FREQUENCY: usize = DEFAULT_SAMPLE_RATE / 2;

#[derive(Debug, clap::Parser, Clone)]
pub struct Options {
    /// The folders to look for songs in
    #[arg(long = "search")]
    pub search_directories: Vec<PathBuf>,
    /// The name of the file to output data to
    pub out_file: PathBuf,
    /// Files to be ignored in the directory
    #[arg(long = "ignore")]
    pub ignore_paths: Vec<PathBuf>,
    #[command(flatten)]
    pub resampler: ResampleSettings,
    #[command(flatten)]
    pub spectrogram: SpectrogramSettings,
    #[command(flatten)]
    pub log_settings: LogSpectrogramSettings,
}

#[derive(Debug, clap::Args, Clone)]
pub struct SpectrogramSettings {
    /// Amount of samples per fft
    #[arg(long, default_value_t = DEFAULT_FFT_SIZE)]
    pub fft_size: usize,
    /// Amount of samples each fft will overlap with the previous fft
    #[arg(long, default_value_t = DEFAULT_FTT_OVERLAP)]
    pub fft_overlap: usize,
}

#[derive(Debug, clap::Args, Clone)]
pub struct LogSpectrogramSettings {
    /// Height of make the spectrogram when converting to log
    #[arg(long = "spectrogram-height", default_value_t = 200)]
    pub height: usize,
    /// Maximum frequency of the log spectrogram
    #[arg(long = "spectrogram-max-frequency", default_value_t = DEFAULT_MAX_FREQUENCY, value_parser = parse_frequency)]
    pub max_frequency: usize,
    /// The base the use when transforming to a log graph
    #[arg(long, default_value_t = 9.5)]
    pub log_base: f32,
}

impl From<SpectrogramSettings> for pleep::spectrogram::Settings {
    fn from(val: SpectrogramSettings) -> Self {
        pleep::spectrogram::Settings {
            fft_len: val.fft_size,
            fft_overlap: val.fft_overlap,
        }
    }
}

#[derive(Debug, clap::Args, Clone)]
pub struct ResampleSettings {
    /// Resample audio to this before processing
    #[arg(short = 'r', long, default_value_t = DEFAULT_SAMPLE_RATE, value_parser = parse_frequency)]
    pub resample_rate: usize,
    /// Number of sub chunks used in resampler
    #[arg(long = "resample-sub-chunks", default_value_t = 1)]
    pub sub_chunks: usize,
    /// Sub chunk size for resampler
    #[arg(long = "resample-chunk-size", default_value_t = 2 << 16)]
    pub chunk_size: usize,
}

impl From<ResampleSettings> for pleep_audio::ResampleSettings {
    fn from(val: ResampleSettings) -> Self {
        pleep_audio::ResampleSettings {
            target_sample_rate: val.resample_rate,
            sub_chunks: val.sub_chunks,
            chunk_size: val.chunk_size,
        }
    }
}

#[instrument(level = "trace")]
pub fn file_to_log_spectrogram(
    path: &PathBuf,
    spectrogram_settings: &pleep::spectrogram::Settings,
    resample_settings: &pleep_audio::ResampleSettings,
    log_spectrogram_settings: &LogSpectrogramSettings,
) -> (
    Duration,
    LogSpectrogramIterator<f32, std::vec::IntoIter<f32>>,
) {
    let audio = pleep_audio::ConvertingAudioIterator::new(
        pleep_audio::AudioSource::from_file_path(path).expect("failed to get audio source"),
    )
    .expect("failed to load file");

    let resampled = pleep_audio::ResamplingChunksIterator::new_from_audio_iterator(
        audio,
        resample_settings.to_owned(),
    )
    .expect("failed to create resampler")
    .flatten()
    .collect::<Vec<f32>>();

    (
        Duration::from_secs_f64(
            resampled.len() as f64 / resample_settings.target_sample_rate as f64,
        ),
        crate::generate_log_spectrogram(
            resampled,
            spectrogram_settings,
            &crate::LogSpectrogramSettings {
                height: log_spectrogram_settings.height,
                frequency_cutoff: log_spectrogram_settings.max_frequency,
                input_sample_rate: resample_settings.target_sample_rate,
                base: log_spectrogram_settings.log_base,
            },
        ),
    )
}

pub fn parse_frequency(input: &str) -> Result<usize, ParseFrequencyError> {
    let lower = input.trim().to_lowercase();
    let lower = input.strip_suffix("hz").unwrap_or(&lower);

    let multiplier = if lower.ends_with("k") {
        1000
    } else if lower.chars().map(|c| c.is_numeric()).all(|v| v) {
        1
    } else {
        return Err(ParseFrequencyError::InvalidText(input.to_string()));
    };

    let numbers = lower.chars().filter(|c| c.is_numeric()).collect::<String>();

    let freq: usize = numbers
        .parse()
        .map_err(|error| ParseFrequencyError::ParseInt {
            text: numbers,
            original: error,
        })?;

    Ok(freq * multiplier)
}

#[derive(Debug, thiserror::Error)]
pub enum ParseFrequencyError {
    #[error("invalid text: {0}")]
    InvalidText(String),
    #[error("failed to parse string to integer: {text:?} {original:?}")]
    ParseInt {
        text: String,
        original: std::num::ParseIntError,
    },
}
