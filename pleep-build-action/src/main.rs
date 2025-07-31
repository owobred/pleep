use std::{collections::VecDeque, io::Read};

use serde::Deserialize;
use tracing::{debug, info, warn};

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

    let mut handle = std::process::Command::new("rclone")
        .arg("sync")
        .arg("arg_gd:/audio")
        .arg("arg_local:audio")
        .arg("--files-from")
        .arg("files_to_fetch.txt")
        .arg("--use-json-log")
        .arg("-v")
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let mut stderr = handle.stderr.take().unwrap();

    let mut line_ringbuffer = VecDeque::new();
    let mut buffer = vec![0u8; 4096];

    rayon::scope(move |s| {
        'outer: loop {
            let read = match stderr.read(&mut buffer) {
                Ok(read) => read,
                Err(error) => match error.kind() {
                    std::io::ErrorKind::BrokenPipe => {
                        debug!("broken pipe :3");
                        if !line_ringbuffer.contains(&('\n' as u8)) {
                            break 'outer;
                        } else {
                            0
                        }
                    }
                    _ => {
                        warn!("paniced");
                        panic!("got unexpected error from rclone: {error:?}");
                    }
                },
            };

            if read == 0 && line_ringbuffer.len() == 0 {
                break 'outer;
            }

            let slice = &buffer[..read];

            for value in slice {
                line_ringbuffer.push_back(*value);
            }

            'inner: loop {
                if let Some(newline_index) = line_ringbuffer
                    .iter()
                    .enumerate()
                    .find_map(|(index, value)| (*value == '\n' as u8).then(|| index))
                {
                    let line = line_ringbuffer.drain(..=newline_index).collect::<Vec<_>>();
                    // TODO: this should be allowed to fail
                    let decoded = match serde_json::from_slice::<LogLine>(&line) {
                        Ok(decoded) => decoded,
                        Err(_) => {
                            warn!(?line, "failed to parse json log");
                            continue 'inner;
                        }
                    };

                    if decoded.msg == "Copied (new)" {
                        s.spawn(move |_s| handle_file_sync(decoded));
                    } else {
                        debug!(?decoded.msg, "unexpected log message");
                    }
                } else {
                    break 'inner;
                }
            }
        }
    });

    info!("building output file");

    let mut segments = Vec::new();

    for file in std::fs::read_dir("segments/").unwrap() {
        let file = file.unwrap();
        let file_name = file.file_name().to_str().unwrap().to_string();
        if !file_name.ends_with(".segment.bin") {
            continue;
        }

        let mut file = std::fs::File::open(format!("segments/{}", file_name)).unwrap();
        let segment = pleep_build::file::Segment::read_from(&mut file, 140).unwrap();
        segments.push(segment);
    }

    segments.sort_by_key(|segment| segment.title.clone());

    info!("writing output file");

    let mut output = std::io::BufWriter::new(std::fs::File::create("out.bin").unwrap());
    pleep_build::file::File {
        build_settings: pleep_build::file::BuildSettings {
            fft_size: 65536,
            fft_overlap: 16384,
            spectrogram_height: 140,
            spectrogram_max_frequency: 13000,
            resample_rate: 26000,
            resample_chunk_size: 131072,
            resample_sub_chunks: 1,
            log_base: 9.5,
        },
        segments,
    }
    .write_to(&mut output)
    .unwrap();

    info!("done");
}

fn handle_file_sync(decoded: LogLine) {
    let mut file = std::path::PathBuf::from("audio/".to_string());
    file.push(decoded.object);
    info!(?file, "processing file");
    let (audio_duration, log_spectrogram) = pleep_build::cli::file_to_log_spectrogram(
        &file,
        &pleep::spectrogram::Settings {
            fft_len: 65536,
            fft_overlap: 16384,
        },
        &pleep_audio::ResampleSettings {
            target_sample_rate: 26000,
            sub_chunks: 1,
            chunk_size: 131072,
        },
        &pleep_build::cli::LogSpectrogramSettings {
            height: 140,
            max_frequency: 13000,
            log_base: 9.5,
        },
    );

    let segment = pleep_build::file::Segment {
        title: file.to_string_lossy().to_string().replacen("audio/", "", 1),
        vectors: log_spectrogram.collect(),
        duration: audio_duration,
    };

    let sha256sum_output = std::process::Command::new("sha256sum")
        .arg(file.to_str().unwrap())
        .output()
        .unwrap();

    std::fs::remove_file(&file).unwrap();

    let fname = &format!(
        "segments/{}.segment.bin",
        String::from_utf8_lossy(&sha256sum_output.stdout)
            .split_once("  ")
            .unwrap()
            .0
    );

    debug!(?fname, "creating segment file");

    let mut file = std::fs::File::create(fname).unwrap();
    segment.write_to(&mut file).unwrap();
    debug!(?fname, "finished segment");
}

#[derive(Debug, Deserialize)]
struct LogLine {
    msg: String,
    object: String,
}
