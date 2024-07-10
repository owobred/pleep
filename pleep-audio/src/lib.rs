use std::{collections::VecDeque, path::PathBuf};

use rubato::Resampler;
use symphonia::core::{
    codecs::{Decoder, DecoderOptions},
    conv::FromSample,
    formats::{FormatOptions, FormatReader},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
    sample::Sample,
};
use thiserror::Error;
use tracing::{error, instrument};

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

#[derive(Debug)]
pub struct Audio<T: AnySample> {
    pub samples: Vec<T>,
    pub sample_rate: usize,
}

pub struct ConvertingAudioIterator<T: ExtendedAnySample> {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    discovered_sample_rate: u32,
    track_id: u32,
    buffer: VecDeque<T>,
}

impl<T: ExtendedAnySample> ConvertingAudioIterator<T> {
    pub fn new(
        AudioSource { media_source }: AudioSource,
    ) -> Result<Self, symphonia::core::errors::Error> {
        let registry = symphonia::default::get_codecs();
        let probe = symphonia::default::get_probe();
        let format = probe.format(
            &Hint::new(),
            media_source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;

        let default_track = format.format.default_track().expect("no default track");
        let default_track_id = default_track.id;
        let default_track_params = default_track.codec_params.clone();

        let decoder = registry.make(&default_track_params, &DecoderOptions::default())?;

        Ok(Self {
            discovered_sample_rate: default_track_params.sample_rate.unwrap(),
            format: format.format,
            decoder,
            track_id: default_track_id,
            buffer: VecDeque::new(),
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.discovered_sample_rate
    }

    pub fn remaining_to_audio(self) -> Audio<T> {
        let sample_rate = self.sample_rate() as usize;
        let samples = self.collect::<Vec<_>>();

        Audio {
            samples,
            sample_rate,
        }
    }
}

impl<T: ExtendedAnySample> Iterator for ConvertingAudioIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(sample) = self.buffer.pop_front() {
            return Some(sample);
        }

        while let Ok(packet) = self.format.next_packet() {
            if packet.track_id() != self.track_id {
                continue;
            }

            let audio_buffer = match self.decoder.decode(&packet) {
                Ok(packet) => packet,
                Err(error) => {
                    error!(?error, "failed to decode packet");
                    return None;
                }
            };

            let mut float_converted = audio_buffer.make_equivalent();
            audio_buffer.convert(&mut float_converted);
            drop(audio_buffer);

            let planes = float_converted.planes();
            let planes_slice = planes.planes();
            let main_channel = planes_slice[0];

            self.buffer.extend(main_channel);

            return self.buffer.pop_front();
        }

        None
    }
}

pub struct ResamplingChunksIterator<T: ExtendedAnySample, I: Iterator<Item = T>> {
    inner_iterator: I,
    resampler: rubato::FftFixedIn<T>,
    settings: ResampleSettings,
}

impl<T: ExtendedAnySample, I: Iterator<Item = T>> ResamplingChunksIterator<T, I> {
    pub fn new(
        wraps: I,
        original_sample_rate: usize,
        settings: ResampleSettings,
    ) -> Result<Self, rubato::ResamplerConstructionError> {
        let resampler = rubato::FftFixedIn::new(
            original_sample_rate,
            settings.target_sample_rate,
            settings.chunk_size,
            settings.sub_chunks,
            1,
        )?;

        Ok(Self {
            inner_iterator: wraps,
            resampler,
            settings,
        })
    }
}

impl<T: ExtendedAnySample> ResamplingChunksIterator<T, ConvertingAudioIterator<T>> {
    pub fn new_from_audio_iterator(
        iterator: ConvertingAudioIterator<T>,
        settings: ResampleSettings,
    ) -> Result<Self, rubato::ResamplerConstructionError> {
        let sample_rate = iterator.sample_rate() as usize;

        Self::new(iterator, sample_rate, settings)
    }

    pub fn remaining_to_audio(self) -> Audio<T> {
        let sample_rate = self.settings.target_sample_rate;
        let samples = self.flatten().collect::<Vec<_>>();

        Audio {
            samples,
            sample_rate,
        }
    }
}

impl<T: ExtendedAnySample, I: Iterator<Item = T>> Iterator for ResamplingChunksIterator<T, I> {
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut samples = Vec::with_capacity(self.settings.chunk_size);

        #[allow(clippy::while_let_on_iterator, reason = "type contraints of `I` do not allow calling `I::by_ref(&self)`")]
        while let Some(sample) = self.inner_iterator.next() {
            samples.push(sample);

            if samples.len() >= self.settings.chunk_size {
                break;
            }
        }

        if samples.is_empty() {
            return None;
        }

        samples.resize(self.settings.chunk_size, T::zero());

        let resampled = self
            .resampler
            .process(&[samples], None)
            .expect("failed to resample");

        Some(resampled.into_iter().next().unwrap())
    }
}

#[derive(Debug, Clone)]
pub struct ResampleSettings {
    pub target_sample_rate: usize,
    pub sub_chunks: usize,
    pub chunk_size: usize,
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
