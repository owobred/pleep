use std::{collections::HashMap, path::PathBuf};

use clap::Parser;
use tracing::info;

const DEFAULT_MAX_DISTANCE: f32 = 0.95;

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
        }
        .into(),
    );

    // in an ideal world saving a debug image wouldn't require this
    let spectrogram = spectrogram.collect::<Vec<_>>();
    if options.debug_images {
        save_spectrogram("input.png", spectrogram.clone());
    }

    let mut best_matches = Vec::new();

    for sample in &spectrogram {
        let mut segment_matches = Vec::with_capacity(file.segments.len());

        for (segment_index, segment) in file.segments.iter().enumerate() {
            let closest = segment
                .vectors
                .iter()
                .map(|vector| 1.0 - distance_cosine(&sample, vector))
                .min_by(|l, r| l.partial_cmp(r).unwrap_or(std::cmp::Ordering::Greater))
                .unwrap_or(f32::INFINITY);

            segment_matches.push((segment_index, closest));
        }

        best_matches.push(
            segment_matches
                .into_iter()
                .min_by(|(_, left), (_, right)| {
                    left.partial_cmp(right)
                        .unwrap_or(std::cmp::Ordering::Greater)
                })
                .unwrap(),
        )
    }

    info!("completed matching samples");

    let mut out_counter = HashMap::new();

    for (best_index, value) in best_matches {
        if !out_counter.contains_key(&best_index) {
            out_counter.insert(best_index, 0.0);
        }

        let hm_value = out_counter.get_mut(&best_index).unwrap();

        *hm_value += (options.max_distance - value).max(0.0);
    }

    let mut best = out_counter.into_iter().collect::<Vec<_>>();

    best.sort_by(|(_, left), (_, right)| {
        left.partial_cmp(right)
            .unwrap_or(std::cmp::Ordering::Greater)
    });
    best.reverse();

    let mut output = CommandOutput {
        matches: Vec::with_capacity(options.n_results),
    };

    let best = best.iter().take(options.n_results).collect::<Vec<_>>();
    let scaled = scale_results(&best.iter().map(|(_, v)| *v).collect::<Vec<_>>());

    if options.debug_images {
        let best_match = &file.segments[best.first().unwrap().0];
        let best_image = save_spectrogram("best.png", best_match.vectors.clone());
        let mut difference: image::ImageBuffer<image::Luma<u8>, Vec<_>> = image::ImageBuffer::new(
            best_image.width().min(spectrogram.len() as u32 - 1),
            best_image.height().min(spectrogram[0].len() as u32 - 1),
        );
        difference.rows_mut().enumerate().for_each(|(y, best_row)| {
            best_row.into_iter().enumerate().for_each(|(x, best)| {
                *best = image::Luma([((best_match.vectors[x][y] - spectrogram[x][y]) * 10.0) as u8])
            })
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
    vectors: Vec<Vec<f32>>,
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

fn distance_sq(l1: &[f32], l2: &[f32]) -> f32 {
    l1.into_iter().zip(l2).map(|(l, r)| (l - r).powi(2)).sum()
}

fn distance_cosine(l1: &[f32], l2: &[f32]) -> f32 {
    let numer: f32 = l1.into_iter().zip(l2.into_iter()).map(|(l, r)| l * r).sum();
    let mag = distance_sq(l1, l2);

    numer / mag.sqrt()
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
    /// Number of results to display
    #[arg(long, default_value_t = 10)]
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
    let sum: f32 = values.into_iter().sum();

    values.into_iter().map(|v| *v / sum).collect()
}
