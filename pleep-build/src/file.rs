pub struct File {
    pub build_settings: BuildSettings,
    pub segments: Vec<Segment>,
}

impl File {
    pub fn write_to(&self, buffer: &mut impl std::io::Write) -> Result<(), Error> {
        self.build_settings.write_to(buffer)?;

        buffer.write_all(&(self.segments.len() as u32).to_le_bytes())?;

        for segment in &self.segments {
            segment.write_to(buffer)?;
        }

        Ok(())
    }

    pub fn read_from(reader: &mut impl std::io::Read) -> Result<Self, Error> {
        let build_settings = BuildSettings::read_from(reader)?;

        let mut n_segments_buf = [0; 4];
        reader.read_exact(&mut n_segments_buf)?;
        let n_segments = u32::from_le_bytes(n_segments_buf);

        let mut segments = Vec::with_capacity(n_segments as usize);

        for _ in 0..n_segments {
            let segment = Segment::read_from(reader, build_settings.spectrogram_height)?;
            segments.push(segment);
        }

        Ok(Self {
            build_settings,
            segments,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BuildSettings {
    pub fft_size: u32,
    pub fft_overlap: u32,
    pub spectrogram_height: u32,
    pub spectrogram_max_frequency: u32,
    pub resample_rate: u32,
    pub resample_chunk_size: u32,
    pub resample_sub_chunks: u32,
}

impl BuildSettings {
    pub fn write_to(&self, buffer: &mut impl std::io::Write) -> Result<(), Error> {
        buffer.write_all(&self.fft_size.to_le_bytes())?;
        buffer.write_all(&self.fft_overlap.to_le_bytes())?;
        buffer.write_all(&self.spectrogram_height.to_le_bytes())?;
        buffer.write_all(&self.spectrogram_max_frequency.to_le_bytes())?;
        buffer.write_all(&self.resample_rate.to_le_bytes())?;
        buffer.write_all(&self.resample_chunk_size.to_le_bytes())?;
        buffer.write_all(&self.resample_sub_chunks.to_le_bytes())?;

        Ok(())
    }
    pub fn read_from(reader: &mut impl std::io::Read) -> Result<Self, Error> {
        let mut fft_size_buffer = [0; 4];
        reader.read_exact(&mut fft_size_buffer)?;
        let fft_size = u32::from_le_bytes(fft_size_buffer);

        let mut fft_overlap_buffer = [0; 4];
        reader.read_exact(&mut fft_overlap_buffer)?;
        let fft_overlap = u32::from_le_bytes(fft_overlap_buffer);

        let mut spectrogram_height_buffer = [0; 4];
        reader.read_exact(&mut spectrogram_height_buffer)?;
        let spectrogram_height = u32::from_le_bytes(spectrogram_height_buffer);

        let mut spectrogram_max_frequency_buffer = [0; 4];
        reader.read_exact(&mut spectrogram_max_frequency_buffer)?;
        let spectrogram_max_frequency = u32::from_le_bytes(spectrogram_max_frequency_buffer);

        let mut resample_rate_buffer = [0; 4];
        reader.read_exact(&mut resample_rate_buffer)?;
        let resample_rate = u32::from_le_bytes(resample_rate_buffer);

        let mut resample_chunk_size_buffer = [0; 4];
        reader.read_exact(&mut resample_chunk_size_buffer)?;
        let resample_chunk_size = u32::from_le_bytes(resample_chunk_size_buffer);

        let mut resample_sub_chunks_buffer = [0; 4];
        reader.read_exact(&mut resample_sub_chunks_buffer)?;
        let resample_sub_chunks = u32::from_le_bytes(resample_sub_chunks_buffer);

        Ok(Self {
            fft_size,
            fft_overlap,
            spectrogram_height,
            spectrogram_max_frequency,
            resample_rate,
            resample_chunk_size,
            resample_sub_chunks,
        })
    }
}

impl From<crate::cli::Options> for BuildSettings {
    fn from(value: crate::cli::Options) -> Self {
        Self {
            fft_size: value.spectrogram.fft_size as u32,
            fft_overlap: value.spectrogram.fft_overlap as u32,
            spectrogram_height: value.log_settings.height as u32,
            spectrogram_max_frequency: value.log_settings.max_frequency as u32,
            resample_rate: value.resampler.resample_rate as u32,
            resample_chunk_size: value.resampler.chunk_size as u32,
            resample_sub_chunks: value.resampler.sub_chunks as u32,
        }
    }
}

pub struct Segment {
    pub title: String,
    pub vectors: Vec<Vec<f32>>,
}

impl Segment {
    pub fn write_to(&self, buffer: &mut impl std::io::Write) -> Result<(), Error> {
        buffer.write_all(&(self.title.len() as u32).to_le_bytes())?;
        buffer.write_all(self.title.as_bytes())?;
        buffer.write_all(&(self.vectors.len() as u32).to_le_bytes())?;

        for vector in &self.vectors {
            for value in vector {
                buffer.write_all(&value.to_le_bytes())?;
            }
        }

        Ok(())
    }

    pub fn read_from(reader: &mut impl std::io::Read, vector_length: u32) -> Result<Self, Error> {
        let mut title_length_buf = [0; 4];
        reader.read_exact(&mut title_length_buf)?;
        let title_length = u32::from_le_bytes(title_length_buf);

        let mut title_buf = vec![0; title_length as usize];
        reader.read_exact(&mut title_buf)?;
        let title = String::from_utf8(title_buf)?;

        let mut n_vectors_buf = [0; 4];
        reader.read_exact(&mut n_vectors_buf)?;
        let n_vectors = u32::from_le_bytes(n_vectors_buf);

        let mut vectors = Vec::with_capacity(n_vectors as usize);

        for _ in 0..n_vectors {
            let mut vector_values = Vec::with_capacity(vector_length as usize);

            for _ in 0..vector_length {
                let mut value_buf = [0; 4];
                reader.read_exact(&mut value_buf)?;
                let value = f32::from_le_bytes(value_buf);
                vector_values.push(value);
            }

            vectors.push(vector_values);
        }

        Ok(Self { title, vectors })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0:?}")]
    Io(#[from] std::io::Error),
    #[error("failed to read utf8: {0:?}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
}
