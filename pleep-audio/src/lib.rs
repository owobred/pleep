use std::path::PathBuf;

use rubato::Resampler;
use symphonia::core::{
    audio::AudioBuffer,
    codecs::DecoderOptions,
    conv::FromSample,
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
    sample::Sample,
};
use thiserror::Error;
use tracing::instrument;

pub trait AnySample:
    Sample
    + FromSample<u8>
    + FromSample<u16>
    + FromSample<symphonia::core::sample::u24>
    + FromSample<u32>
    + FromSample<i8>
    + FromSample<i16>
    + FromSample<symphonia::core::sample::i24>
    + FromSample<i32>
    + FromSample<f32>
    + FromSample<f64>
{
}

impl<
        T: Sample
            + FromSample<u8>
            + FromSample<u16>
            + FromSample<symphonia::core::sample::u24>
            + FromSample<u32>
            + FromSample<i8>
            + FromSample<i16>
            + FromSample<symphonia::core::sample::i24>
            + FromSample<i32>
            + FromSample<f32>
            + FromSample<f64>,
    > AnySample for T
{
}

pub trait ExtendedAnySample: AnySample + rubato::Sample {}
impl<T: AnySample + rubato::Sample> ExtendedAnySample for T {}

pub struct AudioSource {
    media_source: MediaSourceStream,
}

impl AudioSource {
    pub fn new(media_source: MediaSourceStream) -> Self {
        Self { media_source }
    }

    #[instrument(err(level = "debug"), level = "trace")]
    pub fn from_file_path(path: &PathBuf) -> Result<Self, std::io::Error> {
        let file = std::fs::File::open(path)?;
        let media_source_stream =
            MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

        Ok(Self {
            media_source: media_source_stream,
        })
    }

    #[instrument(skip(buffer), level = "trace")]
    pub fn from_memory_buffer(buffer: Vec<u8>) -> Self {
        let media_source_stream = MediaSourceStream::new(
            Box::new(std::io::Cursor::new(buffer)),
            MediaSourceStreamOptions::default(),
        );

        Self {
            media_source: media_source_stream,
        }
    }
}

#[instrument(skip(media_source), err(level = "debug"), level = "trace")]
pub fn load_audio<T: ExtendedAnySample>(
    AudioSource { media_source }: AudioSource,
) -> Result<Audio<T>, Error> {
    let registry = symphonia::default::get_codecs();
    let probe = symphonia::default::get_probe();
    let mut format = probe.format(
        &Hint::new(),
        media_source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let default_track = format.format.default_track().expect("no default track");
    let default_track_id = default_track.id;
    let default_track_params = default_track.codec_params.to_owned();

    let mut decoder = registry.make(&default_track_params, &DecoderOptions::default())?;

    let mut samples = Vec::new();

    while let Ok(packet) = format.format.next_packet() {
        if packet.track_id() != default_track_id {
            continue;
        }

        let audio_buffer = decoder.decode(&packet)?;
        let mut float_converted = audio_buffer.make_equivalent();
        audio_buffer.convert(&mut float_converted);
        drop(audio_buffer);
        let planes = float_converted.planes();
        let planes_slice = planes.planes();
        let main_channel = planes_slice[0];

        samples.extend(main_channel);
    }

    Ok(Audio {
        sample_rate: default_track_params.sample_rate.unwrap() as usize,
        samples,
    })
}

#[derive(Debug)]
pub struct Audio<T: AnySample> {
    pub samples: Vec<T>,
    pub sample_rate: usize,
}

#[instrument(skip(audio), err(level = "debug"), level = "trace")]
pub fn resample_audio<T: ExtendedAnySample>(
    audio: Audio<T>,
    settings: &ResampleSettings,
) -> Result<Audio<T>, Error> {
    let mut resampler = rubato::FftFixedIn::new(
        audio.sample_rate,
        settings.target_sample_rate,
        audio.samples.len(),
        settings.sub_chunks,
        1,
    )?;
    let resampled = resampler.process(&[&audio.samples], None)?;

    Ok(Audio {
        samples: resampled.into_iter().next().unwrap(),
        sample_rate: settings.target_sample_rate,
    })
}

#[derive(Debug, Clone)]
pub struct ResampleSettings {
    pub target_sample_rate: usize,
    pub sub_chunks: usize,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("error passed from symphonia: {0:?}")]
    Symphonia(#[from] symphonia::core::errors::Error),
    #[error("audio did not have a default track")]
    NoDefaultTrack,
    #[error("error constructing resampler: {0:?}")]
    ResamplerConstruction(#[from] rubato::ResamplerConstructionError),
    #[error("error resampling: {0:?}")]
    Resampler(#[from] rubato::ResampleError),
}
