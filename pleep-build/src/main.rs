use std::path::PathBuf;

use clap::Parser;
use tracing::warn;

const DEFAULT_SAMPLE_RATE: usize = 2 << 14;
const DEFAULT_FFT_SIZE: usize = DEFAULT_SAMPLE_RATE;
const DEFAULT_FTT_OVERLAP: usize = DEFAULT_FFT_SIZE / 4;

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
    let resample_options = options.resampler.into();
    let spectrogram_settings = options.spectrogram.into();

    let files =
        pleep_build::get_files_in_directory(&options.directory).expect("failed to list directory");

    for file in files {
        let relative_path = file
            .strip_prefix(&options.directory)
            .expect("failed to strip prefix")
            .to_path_buf();
        let audio: pleep_audio::Audio<f32> = pleep_audio::load_audio(
            pleep_audio::AudioSource::from_file_path(&file).expect("failed to load audio source"),
        )
        .expect("failed to get audio samples");
        let before_sample_n = audio.samples.len();
        let resampled = pleep_audio::resample_audio(audio, &resample_options)
            .expect("failed to resample audio");
        println!(
            "resampled {relative_path:?} from {before_sample_n} to {} samples at {}hz",
            resampled.samples.len(),
            resampled.sample_rate
        );
        let spectrogram_generator = pleep::spectrogram::Generator::new();
        let spectrogram =
            spectrogram_generator.generate_spectrogram(&resampled.samples, &spectrogram_settings);
        let log_spectrogram = spectrogram
            .into_iter()
            .map(|col| pleep_build::make_log(&col, 600))
            .collect::<Vec<_>>();
        let (width, height) = (log_spectrogram.len(), log_spectrogram[0].len());
        println!("created spectrogram {width}x{height}");

        // let cutoff_frequency = resample_options.target_sample_rate / 2;
        // let cutoff_bin = pleep::spectrogram::get_bin_for_frequency(
        //     cutoff_frequency as f64,
        //     resample_options.target_sample_rate,
        //     spectrogram_settings.fft_len,
        // );

        // println!("cutting off at {cutoff_frequency}hz (bin {cutoff_bin})");

        let mut canvas: image::ImageBuffer<image::Luma<u8>, Vec<u8>> =
            image::ImageBuffer::new(width as u32, height as u32);

        for x in 0..width {
            for y in 0..height {
                // if y > cutoff_bin as usize {
                //     continue;
                // }

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
}

#[derive(Debug, clap::Args)]
struct SpectrogramSettings {
    #[arg(long, default_value_t = DEFAULT_FFT_SIZE)]
    fft_size: usize,
    #[arg(long, default_value_t = DEFAULT_FTT_OVERLAP)]
    fft_overlap: usize,
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
