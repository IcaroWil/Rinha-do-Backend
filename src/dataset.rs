use std::{
    fs::File,
    io::{BufReader, Read},
};

use crate::vectorizer::DIMS;

const MAGIC: &[u8; 8] = b"RINHAI01";

pub const AMOUNT_BUCKETS: usize = 16;
pub const MCC_BUCKETS: usize = 8;
pub const TOTAL_BUCKETS: usize = 2 * 2 * 2 * 2 * MCC_BUCKETS * AMOUNT_BUCKETS;

#[derive(Clone)]
pub struct Dataset {
    pub vectors: Vec<u16>,
    pub labels: Vec<u8>,
    pub len: usize,
    pub buckets: Vec<Vec<u32>>,
}

#[inline]
fn bool_bucket(value: u16) -> usize {
    if value > 32768 {
        1
    } else {
        0
    }
}

#[inline]
fn normalized_bucket(value: u16, buckets: usize) -> usize {
    if value == 0 {
        return 0;
    }

    let normalized = value.saturating_sub(1) as usize;
    let bucket = normalized * buckets / 65535;

    bucket.min(buckets - 1)
}

#[inline]
pub fn bucket_key_from_parts(
    has_last: usize,
    is_online: usize,
    card_present: usize,
    unknown_merchant: usize,
    mcc_bucket: usize,
    amount_bucket: usize,
) -> usize {
    amount_bucket
        + AMOUNT_BUCKETS
            * (mcc_bucket
                + MCC_BUCKETS
                    * (unknown_merchant
                        + 2 * (card_present + 2 * (is_online + 2 * has_last))))
}

#[inline]
pub fn bucket_key_from_quantized_vector(vector: &[u16]) -> usize {
    let amount_bucket = normalized_bucket(vector[0], AMOUNT_BUCKETS);

    // dim 5 = minutes_since_last_tx
    // quando last_transaction é null, o valor quantizado é 0
    let has_last = if vector[5] == 0 { 0 } else { 1 };

    let is_online = bool_bucket(vector[9]);
    let card_present = bool_bucket(vector[10]);
    let unknown_merchant = bool_bucket(vector[11]);
    let mcc_bucket = normalized_bucket(vector[12], MCC_BUCKETS);

    bucket_key_from_parts(
        has_last,
        is_online,
        card_present,
        unknown_merchant,
        mcc_bucket,
        amount_bucket,
    )
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

        let mut buckets: Vec<Vec<u32>> = (0..TOTAL_BUCKETS).map(|_| Vec::new()).collect();

        for idx in 0..len {
            let offset = idx * DIMS;
            let key = bucket_key_from_quantized_vector(&vectors[offset..offset + DIMS]);
            buckets[key].push(idx as u32);
        }

        let non_empty_buckets = buckets.iter().filter(|bucket| !bucket.is_empty()).count();

        println!(
            "Built {} buckets, {} non-empty",
            TOTAL_BUCKETS, non_empty_buckets
        );

        Ok(Self {
            vectors,
            labels,
            len,
            buckets,
        })
    }
}