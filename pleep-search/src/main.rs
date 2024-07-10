use std::{collections::HashMap, path::PathBuf};

use clap::Parser;
use tracing::info;

const DEFAULT_MAX_DISTANCE: f32 = 0.75;
const DEFAULT_CLOSEST_VECTORS: usize = 5;
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

    let spectrogram = pleep_build::cli::file_to_log_spectrogram(
        &options.audio_file,
        &pleep_build::cli::SpectrogramSettings {
            fft_overlap: file.build_settings.fft_overlap as usize,
            fft_size: file.build_settings.fft_size as usize,
        }
        .into(),
        &pleep_build::cli::ResampleSettings {
            resample_rate: file.build_settings.resample_rate as usize,
            chunk_size: file.build_settings.resample_chunk_size as usize,
            sub_chunks: file.build_settings.resample_sub_chunks as usize,
        }
        .into(),
        &pleep_build::cli::LogSpectrogramSettings {
            height: file.build_settings.spectrogram_height as usize,
            max_frequency: file.build_settings.spectrogram_max_frequency as usize,
        },
    );

    // in an ideal world saving a debug image wouldn't require this
    let spectrogram = spectrogram.collect::<Vec<_>>();
    if options.debug_images {
        save_spectrogram("input.png", &spectrogram);
    }

    let mut out_counter = HashMap::new();

    for sample in &spectrogram {
        let mut sample_distances = Vec::new();

        for (segment_index, segment) in file.segments.iter().enumerate() {
            for vector in &segment.vectors {
                let Some(distance) = distance_cosine(sample, vector) else {
                    continue;
                };

                if distance < options.max_distance {
                    continue;
                }

                sample_distances.push((segment_index, distance));
            }
        }

        sample_distances.sort_by(|(_, l), (_, r)| {
            l.partial_cmp(r)
                .unwrap_or(std::cmp::Ordering::Greater)
                .reverse()
        });

        for (score_index, (segment_index, distance)) in sample_distances
            .into_iter()
            .take(options.n_closest_vectors)
            .enumerate()
        {
            let entry = out_counter.entry(segment_index).or_insert(0.0);

            *entry += distance.powi(3) / (score_index + 1) as f32;
        }
    }

    info!("completed matching samples");

    let mut best = out_counter.into_iter().collect::<Vec<_>>();

    best.sort_by(|(_, left), (_, right)| {
        left.partial_cmp(right)
            .unwrap_or(std::cmp::Ordering::Greater)
            .reverse()
    });

    let mut output = CommandOutput {
        matches: Vec::with_capacity(options.n_results),
    };

    let best = best.iter().take(options.n_results).collect::<Vec<_>>();
    let scaled = scale_results(&best.iter().map(|(_, v)| *v).collect::<Vec<_>>());

    if options.debug_images {
        let best_match = &file.segments[best.first().unwrap().0];
        let best_image = save_spectrogram("best.png", &best_match.vectors);
        let mut difference: image::ImageBuffer<image::Luma<u8>, Vec<_>> = image::ImageBuffer::new(
            best_image.width().min(spectrogram.len() as u32),
            best_image.height().min(spectrogram[0].len() as u32),
        );
        difference.rows_mut().enumerate().for_each(|(y, best_row)| {
            best_row.into_iter().enumerate().for_each(|(x, best)| {
                *best =
                    image::Luma([((best_match.vectors[x][y] - spectrogram[x][y]) * 10.0) as u8]);
            });
        });
        difference
            .save("difference.png")
            .expect("failed to save difference image");
    }

    for (index, ((song_index, score), scaled_prob)) in
        best.into_iter().zip(scaled.into_iter()).enumerate()
    {
        let title = &file.segments[*song_index].title;
        output.matches.push(Match {
            title: title.to_owned(),
            score: *score,
            scaled_prob,
        });
        info!(
            "{: >4}: {} [score={score}] [scaled_prob={scaled_prob}]",
            index + 1,
            title,
        );
    }

    if options.json {
        let json = serde_json::to_string(&output).unwrap();

        print!("{json}");
    }
}

fn save_spectrogram(
    name: &str,
    vectors: &[Vec<f32>],
) -> image::ImageBuffer<image::Luma<u8>, Vec<u8>> {
    let mut canvas = image::ImageBuffer::new(vectors.len() as u32, vectors[0].len() as u32);
    for (x, column) in vectors.iter().enumerate() {
        for (y, value) in column.iter().enumerate() {
            canvas.put_pixel(x as u32, y as u32, image::Luma([(*value * 10.0) as u8]));
        }
    }
    canvas
        .save(name)
        .expect("failed to save spectrogram debug image");
    canvas
}

fn magnitude_sq(l1: &[f32]) -> f32 {
    l1.iter().map(|v| v.powi(2)).sum()
}

fn distance_cosine(l1: &[f32], l2: &[f32]) -> Option<f32> {
    let numer: f32 = l1.iter().zip(l2).map(|(l, r)| l * r).sum();
    let denom = magnitude_sq(l1) * magnitude_sq(l2);

    let result = numer / denom.sqrt();

    result.is_finite().then_some(result)
}

#[derive(Debug, clap::Parser, Clone)]
struct Options {
    /// File that contains all of the spectrograms
    lookup_file: PathBuf,
    /// File that audio should be read from
    audio_file: PathBuf,
    /// Maximum distance to consider points at
    #[arg(long, default_value_t = DEFAULT_MAX_DISTANCE)]
    max_distance: f32,
    /// Number of vectors to consider when comparing vectors
    #[arg(long, default_value_t = DEFAULT_CLOSEST_VECTORS)]
    n_closest_vectors: usize,
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
    scaled_prob: f32,
}

fn scale_results(values: &[f32]) -> Vec<f32> {
    let sum: f32 = values.iter().sum();

    values.iter().map(|v| *v / sum).collect()
}
