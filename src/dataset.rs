use std::{
    fs::File,
    io::{BufReader, Read},
};

use crate::vectorizer::DIMS;

const MAGIC: &[u8; 8] = b"RINHAI01";

#[derive(Clone)]
pub struct Dataset {
    pub vectors: Vec<u16>,
    pub labels: Vec<u8>,
    pub len: usize,
}

impl Dataset {
    pub fn load_index() -> anyhow::Result<Self> {
        let file = File::open("/app/data/index.bin")
            .or_else(|_| File::open("data/index.bin"))?;

        let mut reader = BufReader::new(file);

        let mut magic = [0_u8; 8];
        reader.read_exact(&mut magic)?;

        if &magic != MAGIC {
            anyhow::bail!("invalid index magic");
        }

        let mut len_bytes = [0_u8; 4];
        reader.read_exact(&mut len_bytes)?;
        let len = u32::from_le_bytes(len_bytes) as usize;

        let mut dims_byte = [0_u8; 1];
        reader.read_exact(&mut dims_byte)?;
        let dims = dims_byte[0] as usize;

        if dims != DIMS {
            anyhow::bail!("invalid index dims: expected {}, got {}", DIMS, dims);
        }

        let vector_values = len * DIMS;
        let mut vector_bytes = vec![0_u8; vector_values * 2];
        reader.read_exact(&mut vector_bytes)?;

        let mut vectors = Vec::with_capacity(vector_values);

        for chunk in vector_bytes.chunks_exact(2) {
            vectors.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }

        let mut labels = vec![0_u8; len];
        reader.read_exact(&mut labels)?;

        Ok(Self {
            vectors,
            labels,
            len,
        })
    }
}