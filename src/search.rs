use crate::dataset::Dataset;
use crate::vectorizer::{Vector, DIMS};

#[inline]
fn quantize(value: f32) -> u16 {
    if value < 0.0 {
        return 0;
    }

    let clamped = if value > 1.0 { 1.0 } else { value };

    1 + (clamped * 65534.0).round() as u16
}

#[inline]
fn distance_squared_quantized(query: &[u16; DIMS], vector: &[u16]) -> u32 {
    let mut sum = 0_u32;

    for i in 0..DIMS {
        let diff = query[i] as i32 - vector[i] as i32;
        sum += (diff * diff) as u32;
    }

    sum
}

#[inline]
fn quantize_query(query: &Vector) -> [u16; DIMS] {
    let mut result = [0_u16; DIMS];

    for i in 0..DIMS {
        result[i] = quantize(query[i]);
    }

    result
}

pub fn fraud_score_bruteforce(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);

    let mut top: [(u32, u8); 5] = [(u32::MAX, 0); 5];

    for idx in 0..dataset.len {
        let offset = idx * DIMS;
        let vector = &dataset.vectors[offset..offset + DIMS];

        let dist = distance_squared_quantized(&query_q, vector);

        let mut worst_idx = 0;
        let mut worst_dist = top[0].0;

        for i in 1..5 {
            if top[i].0 > worst_dist {
                worst_dist = top[i].0;
                worst_idx = i;
            }
        }

        if dist < worst_dist {
            top[worst_idx] = (dist, dataset.labels[idx]);
        }
    }

    let frauds = top.iter().filter(|(_, label)| *label == 1).count();

    frauds as f32 / 5.0
} 