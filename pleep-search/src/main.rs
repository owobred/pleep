use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};

use clap::Parser;
use tracing::{debug, info, warn};

const DEFAULT_MAX_ERROR: f32 = 10.0;
const DEFAULT_NUM_RESULTS: usize = 10;
const DEFAULT_EXTRA_OFFSETS: usize = 10;
const DEFAULT_SEGMENT_TRIM_SIZE: usize = 20;
const DEFAULT_SEGMENT_TRIM_STEP: usize = 3;
const DEFAULT_MIN_VECTORS: usize = 6;
const DEFAULT_SPECTROGRAM_PADDING: usize = 3;

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
    let start = std::time::Instant::now();

    let mut reader = std::io::BufReader::new(std::fs::File::open(&options.lookup_file).unwrap());
    let file = pleep_build::file::File::read_from(&mut reader).unwrap();
    info!(build_settings=?file.build_settings, "read search file");

    let audio: pleep_audio::Audio<f32> = pleep_audio::ConvertingAudioIterator::new(
        pleep_audio::AudioSource::from_file_path(&options.audio_file)
            .expect("failed to get audio source"),
    )
    .expect("failed to load file")
    .remaining_to_audio();

    let threadpool = rayon::ThreadPoolBuilder::new().build().unwrap();
    let (send, recv) = crossbeam::channel::unbounded();

    let mut errors = vec![f32::INFINITY; file.segments.len()];
    let mut trimmed_segments = Vec::new();

    for remove_pre in (0..=options.segment_trim_size).step_by(options.segment_trim_step) {
        let trimmed = file
            .segments
            .iter()
            .map(|segment| &segment.vectors[(remove_pre.min(segment.vectors.len()))..])
            .collect::<Vec<_>>();

        trimmed_segments.push(trimmed);
    }

    threadpool.scope(|s| {
        for trimmed in &trimmed_segments {
            let mut slices = Vec::new();
            for index in 0..=options.extra_offsets {
                let offset = (index * audio.sample_rate * file.build_settings.fft_size as usize
                    / file.build_settings.resample_rate as usize)
                    / options.extra_offsets;
                slices.push((offset, &audio.samples[offset..]));
            }

            for (offset, slice) in slices {
                let build_settings = &file.build_settings;
                let options = &options;
                let send = send.clone();

                s.spawn(move |_s| {
                    debug!(offset, "starting offset");

                    let offset_errors = get_error(
                        slice,
                        audio.sample_rate,
                        build_settings,
                        options,
                        options.min_vectors,
                        &trimmed,
                        options.spectrogram_padding,
                    );

                    send.send(offset_errors).unwrap();
                });
            }
        }
    });
    drop(send);

    debug!("merging errors");
    while let Ok(offset_errors) = recv.recv() {
        for (index, mse) in offset_errors {
            errors[index] = errors[index].min(mse)
        }
    }

    let mut best = errors
        .into_iter()
        .enumerate()
        .filter(|(_, mse)| mse.is_finite())
        .collect::<Vec<_>>();

    best.sort_by(|(_, l), (_, r)| l.partial_cmp(r).unwrap_or(std::cmp::Ordering::Less));

    if options.debug_images {
        if best.len() > 0 {
            let best_section = &file.segments[best[0].0];
            save_spectrogram("best.png", &best_section.vectors);
        } else {
            warn!("no best segment, not creating best.png");
        }
    }

    let top_n = best.into_iter().take(options.n_results).collect::<Vec<_>>();

    let max_observed_mse = top_n
        .iter()
        .map(|(_, mse)| *mse)
        .max_by(|l, r| l.partial_cmp(r).unwrap_or(std::cmp::Ordering::Less))
        .unwrap_or(f32::INFINITY);

    let elapsed_time = start.elapsed();

    for (index, (segment_index, mse)) in top_n.iter().enumerate() {
        info!(
            mse,
            neg_scaled_mse = 1.0 - mse / max_observed_mse,
            confidence = (options.max_error - mse) / options.max_error,
            "{index: >4}: {}",
            file.segments[*segment_index].title
        );
    }
    debug!(?elapsed_time, "done");

    if options.json {
        print!(
            "{}",
            serde_json::to_string(&CommandOutput {
                matches: top_n
                    .into_iter()
                    .map(|(segment_index, score)| Match {
                        title: file.segments[segment_index].title.clone(),
                        score
                    })
                    .collect()
            })
            .unwrap()
        );
    }
}

fn save_spectrogram(
    name: &str,
    vectors: &[Vec<f32>],
) -> image::ImageBuffer<image::Luma<u8>, Vec<u8>> {
    let min = *vectors
        .iter()
        .flatten()
        .min_by(|l, r| l.partial_cmp(r).unwrap_or(std::cmp::Ordering::Less))
        .unwrap();
    let max = *vectors
        .iter()
        .flatten()
        .max_by(|l, r| l.partial_cmp(r).unwrap_or(std::cmp::Ordering::Less))
        .unwrap();
    let difference = max - min;

    let mut canvas = image::ImageBuffer::new(vectors.len() as u32, vectors[0].len() as u32);
    for (x, column) in vectors.iter().enumerate() {
        for (y, value) in column.iter().enumerate() {
            canvas.put_pixel(
                x as u32,
                y as u32,
                image::Luma([((*value * 255.0 - min) / difference) as u8]),
            );
        }
    }
    canvas
        .save(name)
        .expect("failed to save spectrogram debug image");
    canvas
}

fn distance_sq(l1: &[f32], l2: &[f32]) -> f32 {
    l1.iter().zip(l2).map(|(l, r)| (l - r).powi(2)).sum()
}

#[derive(Debug, clap::Parser, Clone)]
struct Options {
    /// File that contains all of the spectrograms
    lookup_file: PathBuf,
    /// File that audio should be read from
    audio_file: PathBuf,
    /// Maximum mse to consider windows at
    #[arg(long, default_value_t = DEFAULT_MAX_ERROR)]
    max_error: f32,
    /// Number of results to display
    #[arg(long, default_value_t = DEFAULT_NUM_RESULTS)]
    n_results: usize,
    /// Output a json object detailing the outputs to stdout
    #[arg(long, action = clap::ArgAction::SetTrue)]
    json: bool,
    /// Generate some debug spectrograms for the fun of it
    #[arg(long, action = clap::ArgAction::SetTrue)]
    debug_images: bool,
    /// The number of different offsets to try when creating shifted spectrograms
    #[arg(long, default_value_t = DEFAULT_EXTRA_OFFSETS)]
    extra_offsets: usize,
    /// Maximum number of vectors to remove from the beginning of segments when matching vectors
    #[arg(long, default_value_t = DEFAULT_SEGMENT_TRIM_SIZE)]
    segment_trim_size: usize,
    /// Step size when testing segments with reduced samples
    #[arg(long, default_value_t = DEFAULT_SEGMENT_TRIM_STEP)]
    segment_trim_step: usize,
    /// Minimum number of vectors a segment must have to be considered
    #[arg(long, default_value_t = DEFAULT_MIN_VECTORS)]
    min_vectors: usize,
    /// Padding to apply to spectrograms
    #[arg(long, default_value_t = DEFAULT_SPECTROGRAM_PADDING)]
    spectrogram_padding: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct CommandOutput {
    matches: Vec<Match>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct Match {
    title: String,
    score: f32,
}

fn get_error(
    samples: &[f32],
    sample_rate: usize,
    build_settings: &pleep_build::file::BuildSettings,
    options: &Options,
    skip_less_than: usize,
    segments: &[&[Vec<f32>]],
    spectrogram_padding: usize,
) -> HashMap<usize, f32> {
    let resample = pleep_audio::ResamplingChunksIterator::new(
        samples.iter().copied(),
        sample_rate,
        pleep_build::cli::ResampleSettings {
            resample_rate: build_settings.resample_rate as usize,
            chunk_size: build_settings.resample_chunk_size as usize,
            sub_chunks: build_settings.resample_sub_chunks as usize,
        }
        .into(),
    )
    .unwrap();

    let mut spectrogram = pleep_build::generate_log_spectrogram(
        resample.flatten().collect::<Vec<_>>(),
        &pleep_build::cli::SpectrogramSettings {
            fft_overlap: build_settings.fft_overlap as usize,
            fft_size: build_settings.fft_size as usize,
        }
        .into(),
        &pleep_build::LogSpectrogramSettings {
            height: build_settings.spectrogram_height as usize,
            frequency_cutoff: build_settings.spectrogram_max_frequency as usize,
            input_sample_rate: build_settings.resample_rate as usize,
            base: build_settings.log_base,
        },
    )
    .collect::<VecDeque<_>>();

    debug!(len = spectrogram.len(), "created spectrogram");

    // TODO: make this only happen on one iteration
    // if options.debug_images {
    //     save_spectrogram("input.png", &spectrogram);
    // }

    let empty_vec = vec![0.0; spectrogram[0].len()];
    for _ in 0..spectrogram_padding {
        spectrogram.push_front(empty_vec.clone());
        spectrogram.push_back(empty_vec.clone());
    }
    let spectrogram = spectrogram.make_contiguous();

    let before_len = segments.len();
    let filtered_segments = segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| segment.len() <= spectrogram.len())
        .filter(|(_, segment)| segment.len() >= skip_less_than)
        .collect::<Vec<_>>();
    debug!(
        before_len,
        after_len = filtered_segments.len(),
        "trimmed segments"
    );

    let mut scores = HashMap::new();

    for (segment_index, segment) in &filtered_segments {
        let mut min_error = f32::INFINITY;
        for spectrogram_window in spectrogram.windows(segment.len()) {
            let error = spectrogram_window
                .iter()
                .zip(segment.iter())
                .map(|(spect_vect, segment_vect)| distance_sq(&spect_vect, &segment_vect))
                .sum::<f32>()
                / spectrogram_window.len() as f32;
            min_error = min_error.min(error);
        }

        if min_error > options.max_error {
            continue;
        }

        scores.insert(*segment_index, min_error);
    }

    scores
}
