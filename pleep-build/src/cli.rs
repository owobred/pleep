use std::path::PathBuf;

use tracing::{instrument, trace};

const DEFAULT_SAMPLE_RATE: usize = 2 << 14;
const DEFAULT_FFT_SIZE: usize = DEFAULT_SAMPLE_RATE;
const DEFAULT_FTT_OVERLAP: usize = DEFAULT_FFT_SIZE / 4;
const DEFAULT_MAX_FREQUENCY: usize = DEFAULT_SAMPLE_RATE / 2;

#[derive(Debug, clap::Parser, Clone)]
pub struct Options {
    /// The folder to look for songs in
    pub directory: PathBuf,
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
    #[arg(long = "spectrogram-max-frequency", default_value_t = DEFAULT_MAX_FREQUENCY)]
    pub max_frequency: usize,
}

impl Into<pleep::spectrogram::Settings> for SpectrogramSettings {
    fn into(self) -> pleep::spectrogram::Settings {
        pleep::spectrogram::Settings {
            fft_len: self.fft_size,
            fft_overlap: self.fft_overlap,
        }
    }
}

#[derive(Debug, clap::Args, Clone)]
pub struct ResampleSettings {
    /// Resample audio to this before processing
    #[arg(short = 'r', long, default_value_t = DEFAULT_SAMPLE_RATE)]
    pub resample_rate: usize,
    /// Number of sub chunks used in resampler
    #[arg(long = "resample-sub-chunks", default_value_t = 1)]
    pub sub_chunks: usize,
    /// Sub chunk size for resampler
    #[arg(long = "resample-chunk-size", default_value_t = 2 << 14)]
    pub chunk_size: usize,
}

impl Into<pleep_audio::ResampleSettings> for ResampleSettings {
    fn into(self) -> pleep_audio::ResampleSettings {
        pleep_audio::ResampleSettings {
            target_sample_rate: self.resample_rate,
            sub_chunks: self.sub_chunks,
            chunk_size: self.chunk_size,
        }
    }
}

#[instrument(level = "trace")]
pub fn file_to_log_spectrogram(
    path: &PathBuf,
    spectrogram_settings: &pleep::spectrogram::Settings,
    resample_settings: &pleep_audio::ResampleSettings,
    log_spectrogram_settings: &LogSpectrogramSettings,
) -> Vec<Vec<f32>> {
    let audio = pleep_audio::ConvertingAudioIterator::new(
        pleep_audio::AudioSource::from_file_path(&path).expect("failed to get audio source"),
    )
    .expect("failed to load file");

    let resampled = pleep_audio::ResamplingChunksIterator::new_from_audio_iterator(
        audio,
        resample_settings.to_owned(),
    )
    .expect("failed to create resampler")
    .flatten()
    .collect::<Vec<f32>>();

    let log_spectrogram = crate::generate_log_spectrogram(
        resampled,
        &spectrogram_settings,
        &crate::LogSpectrogramSettings {
            height: log_spectrogram_settings.height,
            frequency_cutoff: log_spectrogram_settings.max_frequency,
            input_sample_rate: resample_settings.target_sample_rate,
        },
    );
    let height = log_spectrogram.height();
    let log_spectrogram = log_spectrogram.collect::<Vec<_>>();
    let width = log_spectrogram.len();
    trace!(width, height, "created log spectrogram");

    log_spectrogram
}
