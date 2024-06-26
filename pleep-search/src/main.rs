use std::{collections::HashMap, path::PathBuf};

use clap::Parser;

const DEFAULT_MAX_DISTANCE: f32 = 100.0;

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

    let mut reader =
        std::io::BufReader::new(std::fs::File::open(&options.lookup_file).unwrap());

    let file = pleep_build::file::File::read_from(&mut reader).unwrap();

    let spectrogram = pleep_build::cli::file_to_log_spectrogram(
        &options.audio_file,
        &pleep_build::cli::SpectrogramSettings {
            fft_overlap: file.build_settings.fft_overlap as usize,
            fft_size: file.build_settings.fft_size as usize,
        }
        .into(),
        &pleep_build::cli::ResampleSettings {
            resample_rate: file.build_settings.resample_rate as usize,
            sub_chunks: options.resample_sub_chunks,
        }
        .into(),
        &pleep_build::cli::LogSpectrogramSettings {
            height: file.build_settings.spectrogram_height as usize,
            max_frequency: file.build_settings.spectrogram_max_frequency as usize,
        }
        .into(),
    );

    let mut best_matches = Vec::new();

    for sample in spectrogram {
        let mut segment_matches = Vec::with_capacity(file.segments.len());

        for (segment_index, segment) in file.segments.iter().enumerate() {
            let closest = segment
                .vectors
                .iter()
                .map(|vector| distance_sq(&sample, vector))
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

    for (index, (song_index, score) )in best.iter().take(options.n_results).enumerate() {
        println!(
            "{: >4}: {} [score={score}]",
            index + 1,
            &file.segments[*song_index].title
        );
    }
}

fn distance_sq(l1: &[f32], l2: &[f32]) -> f32 {
    l1.into_iter().zip(l2).map(|(l, r)| (l - r).powi(2)).sum()
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
    /// Number of sub chunks to use when resampling
    #[arg(long, default_value_t = 2048)]
    resample_sub_chunks: usize,
}