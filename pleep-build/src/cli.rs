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
    /// Sub chunk size for resampler
    #[arg(long = "resample-sub-chunks", default_value_t = 2 << 10)]
    pub sub_chunks: usize,
}

impl Into<pleep_audio::ResampleSettings> for ResampleSettings {
    fn into(self) -> pleep_audio::ResampleSettings {
        pleep_audio::ResampleSettings {
            target_sample_rate: self.resample_rate,
            sub_chunks: self.sub_chunks,
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
    let audio: pleep_audio::Audio<f32> = pleep_audio::load_audio(
        pleep_audio::AudioSource::from_file_path(&path).expect("failed to load audio source"),
    )
    .expect("failed to get audio samples");
    trace!(
        sample_rate = audio.sample_rate,
        n_samples = audio.samples.len(),
        "loaded audio"
    );
    let resampled =
        pleep_audio::resample_audio(audio, &resample_settings).expect("failed to resample audio");
    trace!(
        sample_rate = resampled.sample_rate,
        n_samples = resampled.samples.len(),
        "completed resample"
    );
    let log_spectrogram = crate::generate_log_spectrogram(
        &resampled.samples,
        &spectrogram_settings,
        &crate::LogSpectrogramSettings {
            height: log_spectrogram_settings.height,
            frequency_cutoff: log_spectrogram_settings.max_frequency,
            input_sample_rate: resample_settings.target_sample_rate,
        },
    );
    let (width, height) = (log_spectrogram.len(), log_spectrogram[0].len());
    trace!(width, height, "created log spectrogram");

    log_spectrogram
}
