use std::path::PathBuf;

use clap::Parser;
use tracing::{instrument, trace, warn};

const DEFAULT_SAMPLE_RATE: usize = 2 << 14;
const DEFAULT_FFT_SIZE: usize = DEFAULT_SAMPLE_RATE;
const DEFAULT_FTT_OVERLAP: usize = DEFAULT_FFT_SIZE / 4;
const DEFAULT_MAX_FREQUENCY: usize = DEFAULT_SAMPLE_RATE / 2;

fn main() {
    {
        use tracing_subscriber::prelude::*;

        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_filter(tracing_subscriber::EnvFilter::from_default_env()),
            )
            .init();
    }

    let options = Options::parse();
    let resample_settings = options.resampler.into();
    let spectrogram_settings = options.spectrogram.into();

    let files =
        pleep_build::get_files_in_directory(&options.directory).expect("failed to list directory");

    for file in files {
        // let relative_path = file
        //     .strip_prefix(&options.directory)
        //     .expect("failed to strip prefix")
        //     .to_path_buf();

        let log_spectrogram = file_to_log_spectrogram(
            &file,
            &spectrogram_settings,
            &resample_settings,
            &options.log_settings,
        );
        let (width, height) = (log_spectrogram.len(), log_spectrogram[0].len());
        let mut canvas: image::ImageBuffer<image::Luma<u8>, Vec<u8>> =
            image::ImageBuffer::new(width as u32, height as u32);

        for x in 0..width {
            for y in 0..height {
                let pixel = canvas.get_pixel_mut(x as u32, y as u32);
                *pixel = image::Luma([(log_spectrogram[x][height - y - 1] * 20.0) as u8]);
            }
        }
        canvas.save("spectrogram.png").unwrap();

        break;
    }
}

#[derive(Debug, clap::Parser)]
struct Options {
    directory: PathBuf,
    #[command(flatten)]
    resampler: ResampleSettings,
    #[command(flatten)]
    spectrogram: SpectrogramSettings,
    #[command(flatten)]
    log_settings: LogSpectrogramSettings,
}

#[derive(Debug, clap::Args)]
struct SpectrogramSettings {
    /// Amount of samples per fft
    #[arg(long, default_value_t = DEFAULT_FFT_SIZE)]
    fft_size: usize,
    /// Amount of samples each fft will overlap with the previous fft
    #[arg(long, default_value_t = DEFAULT_FTT_OVERLAP)]
    fft_overlap: usize,
}

#[derive(Debug, clap::Args)]
struct LogSpectrogramSettings {
    /// Height of make the spectrogram when converting to log
    #[arg(long = "spectrogram-height", default_value_t = 200)]
    height: usize,
    /// Maximum frequency of the log spectrogram
    #[arg(long = "spectrogram-max-frequency", default_value_t = DEFAULT_MAX_FREQUENCY)]
    max_frequency: usize,
}

impl Into<pleep::spectrogram::Settings> for SpectrogramSettings {
    fn into(self) -> pleep::spectrogram::Settings {
        pleep::spectrogram::Settings {
            fft_len: self.fft_size,
            fft_overlap: self.fft_overlap,
        }
    }
}

#[derive(Debug, clap::Args)]
struct ResampleSettings {
    /// Resample audio to this before processing
    #[arg(short = 'r', long, default_value_t = DEFAULT_SAMPLE_RATE)]
    resample_rate: usize,
    /// Sub chunk size for resampler
    #[arg(long = "resample-sub-chunks", default_value_t = 2 << 10)]
    sub_chunks: usize,
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
fn file_to_log_spectrogram(
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
    let log_spectrogram = pleep_build::generate_log_spectrogram(
        &resampled.samples,
        &spectrogram_settings,
        &pleep_build::LogSpectrogramSettings {
            height: log_spectrogram_settings.height,
            frequency_cutoff: log_spectrogram_settings.max_frequency,
            input_sample_rate: resample_settings.target_sample_rate,
        },
    );
    let (width, height) = (log_spectrogram.len(), log_spectrogram[0].len());
    trace!(width, height, "created log spectrogram");

    log_spectrogram
}
