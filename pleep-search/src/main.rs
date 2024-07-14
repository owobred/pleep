use std::{collections::HashMap, path::PathBuf};

use clap::Parser;
use tracing::{debug, info, warn};

const DEFAULT_MAX_ERROR: f32 = 5.0;
// const DEFAULT_MAX_ERROR: f32 = 1e-2;
const DEFAULT_NUM_RESULTS: usize = 10;

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

    let mut reader = std::io::BufReader::new(std::fs::File::open(&options.lookup_file).unwrap());
    let file = pleep_build::file::File::read_from(&mut reader).unwrap();
    info!(build_settings=?file.build_settings, "read search file");

    let audio: pleep_audio::Audio<f32> = pleep_audio::ConvertingAudioIterator::new(
        pleep_audio::AudioSource::from_file_path(&options.audio_file)
            .expect("failed to get audio source"),
    )
    .expect("failed to load file")
    .remaining_to_audio();

    let num_extra_offsets = 50;

    let threadpool = rayon::ThreadPoolBuilder::new().build().unwrap();
    let (send, recv) = crossbeam::channel::unbounded();

    let mut errors = HashMap::new();
    threadpool.scope(|s| {
        let mut slices = Vec::new();
        for index in 0..=num_extra_offsets {
            let offset = index * audio.sample_rate / num_extra_offsets;
            slices.push((offset, &audio.samples[offset..]));
        }

        for (offset, slice) in slices {
            let file = &file;
            let options = &options;
            let send = send.clone();

            s.spawn(move |_s| {
                debug!(offset, "starting offset");
    
                let offset_errors =
                    get_error(slice, audio.sample_rate, file, options);
    
                send.send(offset_errors).unwrap();
            });
        }
    });
    drop(send);

    while let Ok(offset_errors) = recv.recv() {
        for (index, mse) in offset_errors {
            errors
                .entry(index)
                .and_modify(|v| *v = mse.min(*v))
                .or_insert(f32::INFINITY);
        }
    }

    let mut best = errors.into_iter().collect::<Vec<_>>();

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

    for (index, (segment_index, mse)) in top_n.iter().enumerate() {
        info!(
            mse,
            neg_scaled_mse = 1.0 - mse / max_observed_mse,
            confidence = (options.max_error - mse) / options.max_error,
            "{index: >4}: {}",
            file.segments[*segment_index].title
        );
    }

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
    file: &pleep_build::file::File,
    options: &Options,
) -> HashMap<usize, f32> {
    let resample = pleep_audio::ResamplingChunksIterator::new(
        samples.iter().copied(),
        sample_rate,
        pleep_build::cli::ResampleSettings {
            resample_rate: file.build_settings.resample_rate as usize,
            chunk_size: file.build_settings.resample_chunk_size as usize,
            sub_chunks: file.build_settings.resample_sub_chunks as usize,
        }
        .into(),
    )
    .unwrap();

    let mut spectrogram = pleep_build::generate_log_spectrogram(
        resample.flatten().collect::<Vec<_>>(),
        &pleep_build::cli::SpectrogramSettings {
            fft_overlap: file.build_settings.fft_overlap as usize,
            fft_size: file.build_settings.fft_size as usize,
        }
        .into(),
        &pleep_build::LogSpectrogramSettings {
            height: file.build_settings.spectrogram_height as usize,
            frequency_cutoff: file.build_settings.spectrogram_max_frequency as usize,
            input_sample_rate: file.build_settings.resample_rate as usize,
        },
    )
    .collect::<Vec<_>>();

    // TODO: make this only happen on one iteration
    // if options.debug_images {
    //     save_spectrogram("input.png", &spectrogram);
    // }

    let empty_vec = vec![0.0; spectrogram[0].len()];
    spectrogram.resize(spectrogram.len() + 3, empty_vec.clone());
    spectrogram.reverse();
    spectrogram.resize(spectrogram.len() + 3, empty_vec.clone());
    spectrogram.reverse();

    let before_len = file.segments.len();
    let filtered_segments = file
        .segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| segment.vectors.len() <= spectrogram.len())
        .collect::<Vec<_>>();
    debug!(
        before_len,
        after_len = filtered_segments.len(),
        "trimmed segments"
    );

    let mut scores = HashMap::new();

    for (segment_index, segment) in &filtered_segments {
        let mut min_error = f32::INFINITY;
        for spectrogram_window in spectrogram.windows(segment.vectors.len()) {
            let error = spectrogram_window
                .iter()
                .zip(segment.vectors.iter())
                .map(|(spect_vect, segmenmt_vect)| distance_sq(&spect_vect, &segmenmt_vect))
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
