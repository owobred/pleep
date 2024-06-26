pub struct File {
    // build_params:
    pub vector_size: u32,
    pub segments: Vec<Segment>,
}

impl File {
    pub fn write_to(&self, buffer: &mut impl std::io::Write) -> Result<(), Error> {
        buffer.write(&self.vector_size.to_le_bytes())?;
        buffer.write(&(self.segments.len() as u32).to_le_bytes())?;

        for segment in &self.segments {
            segment.write_to(buffer)?;
        }

        Ok(())
    }

    pub fn read_from(reader: &mut impl std::io::Read) -> Result<Self, Error> {
        let mut vector_size_buf = [0; 4];
        reader.read_exact(&mut vector_size_buf)?;
        let vector_size = u32::from_le_bytes(vector_size_buf);

        let mut n_segments_buf = [0; 4];
        reader.read_exact(&mut n_segments_buf)?;
        let n_segments = u32::from_le_bytes(n_segments_buf);

        let mut segments = Vec::with_capacity(n_segments as usize);

        for _ in 0..n_segments {
            let segment = Segment::read_from(reader, vector_size)?;
            segments.push(segment);
        }

        Ok(Self {
            vector_size,
            segments,
        })
    }
}

pub struct Segment {
    pub title: String,
    pub vectors: Vec<Vec<f32>>,
}

impl Segment {
    fn write_to(&self, buffer: &mut impl std::io::Write) -> Result<(), Error> {
        buffer.write(&(self.title.len() as u32).to_le_bytes())?;
        buffer.write(self.title.as_bytes())?;
        buffer.write(&(self.vectors.len() as u32).to_le_bytes())?;

        for vector in &self.vectors {
            for value in vector {
                buffer.write(&value.to_le_bytes())?;
            }
        }

        Ok(())
    }

    fn read_from(reader: &mut impl std::io::Read, vector_length: u32) -> Result<Self, Error> {
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
