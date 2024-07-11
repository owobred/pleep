use clap::Parser;
use pleep_build::cli::{file_to_log_spectrogram, Options};
use tracing::{debug, info};

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
    let resample_settings: pleep_audio::ResampleSettings = options.clone().resampler.into();
    let spectrogram_settings: pleep::spectrogram::Settings = options.clone().spectrogram.into();

    let files = options
        .search_directories
        .iter()
        .flat_map(|dir| pleep_build::get_files_in_directory(dir).expect("failed to list directory"))
        .collect::<Vec<_>>();

    let mut out_file = std::io::BufWriter::new(
        std::fs::File::create(&options.out_file).expect("failed to open output file for writing"),
    );

    let mut out_file_values = pleep_build::file::File {
        build_settings: options.clone().into(),
        segments: Vec::new(),
    };

    let (send, recv) = crossbeam::channel::unbounded();

    let canonicalized_ignore_files = options
        .ignore_paths
        .into_iter()
        .map(|file| file.canonicalize().unwrap())
        .collect::<Vec<_>>();

    rayon::scope(move |s| {
        for file in files {
            if canonicalized_ignore_files.contains(&file.canonicalize().unwrap()) {
                debug!(?file, "skipping file as it is ignored");
                continue;
            }

            let spectrogram_settings = spectrogram_settings.clone();
            let resample_settings = resample_settings.clone();
            let log_settings = options.log_settings.clone();
            let sender = send.clone();

            s.spawn(move |_s| {
                info!(path=?file, "processing file");
                let (audio_duration, log_spectrogram) = file_to_log_spectrogram(
                    &file,
                    &spectrogram_settings,
                    &resample_settings,
                    &log_settings,
                );

                let segment = pleep_build::file::Segment {
                    title: file.to_string_lossy().to_string(),
                    vectors: log_spectrogram.collect(),
                    duration: audio_duration,
                };

                sender.send(segment).expect("failed to send to mpsc");
            });
        }
    });

    info!("all subtasks finished");

    while let Ok(segment) = recv.recv() {
        out_file_values.segments.push(segment);
    }

    info!("sorting segments");

    out_file_values
        .segments
        .sort_by_key(|segment| segment.title.clone());

    info!("saving file");

    out_file_values
        .write_to(&mut out_file)
        .expect("failed to write file");
}
