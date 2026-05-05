use crate::{
    dataset::{
        bucket_key_from_parts, AMOUNT_BUCKETS, Dataset, MCC_BUCKETS,
    },
    vectorizer::{Vector, DIMS},
};

const MAX_CANDIDATES_PER_QUERY: usize = 50_000;

#[inline]
fn quantize(value: f32) -> u16 {
    if value < 0.0 {
        return 0;
    }

    let clamped = if value > 1.0 { 1.0 } else { value };

    1 + (clamped * 65534.0).round() as u16
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
fn bool_bucket(value: u16) -> usize {
    if value > 32768 {
        1
    } else {
        0
    }
}

#[inline]
fn quantize_query(query: &Vector) -> [u16; DIMS] {
    let mut result = [0_u16; DIMS];

    for i in 0..DIMS {
        result[i] = quantize(query[i]);
    }

    result
}

#[inline(always)]
fn sq_diff(a: u16, b: u16) -> u64 {
    let diff = a as i64 - b as i64;
    (diff * diff) as u64
}

#[inline(always)]
fn distance_squared_quantized(query: &[u16; DIMS], vectors: &[u16], offset: usize) -> u64 {
    sq_diff(query[0], vectors[offset])
        + sq_diff(query[1], vectors[offset + 1])
        + sq_diff(query[2], vectors[offset + 2])
        + sq_diff(query[3], vectors[offset + 3])
        + sq_diff(query[4], vectors[offset + 4])
        + sq_diff(query[5], vectors[offset + 5])
        + sq_diff(query[6], vectors[offset + 6])
        + sq_diff(query[7], vectors[offset + 7])
        + sq_diff(query[8], vectors[offset + 8])
        + sq_diff(query[9], vectors[offset + 9])
        + sq_diff(query[10], vectors[offset + 10])
        + sq_diff(query[11], vectors[offset + 11])
        + sq_diff(query[12], vectors[offset + 12])
        + sq_diff(query[13], vectors[offset + 13])
}

#[inline]
fn find_worst_idx(top: &[(u64, u8); 5]) -> usize {
    let mut worst_idx = 0;
    let mut worst_dist = top[0].0;

    for i in 1..5 {
        if top[i].0 > worst_dist {
            worst_dist = top[i].0;
            worst_idx = i;
        }
    }

    worst_idx
}

#[inline]
fn update_top5(top: &mut [(u64, u8); 5], filled: &mut usize, dist: u64, label: u8) {
    if *filled < 5 {
        top[*filled] = (dist, label);
        *filled += 1;
        return;
    }

    let worst_idx = find_worst_idx(top);
    let worst_dist = top[worst_idx].0;

    if dist < worst_dist {
        top[worst_idx] = (dist, label);
    }
}

fn fraud_score_bucket_range(
    query_q: &[u16; DIMS],
    dataset: &Dataset,
    amount_radius: usize,
    mcc_radius: usize,
) -> Option<f32> {
    let amount_bucket = normalized_bucket(query_q[0], AMOUNT_BUCKETS);
    let has_last = if query_q[5] == 0 { 0 } else { 1 };
    let is_online = bool_bucket(query_q[9]);
    let card_present = bool_bucket(query_q[10]);
    let unknown_merchant = bool_bucket(query_q[11]);
    let mcc_bucket = normalized_bucket(query_q[12], MCC_BUCKETS);

    let amount_start = amount_bucket.saturating_sub(amount_radius);
    let amount_end = (amount_bucket + amount_radius).min(AMOUNT_BUCKETS - 1);

    let mcc_start = mcc_bucket.saturating_sub(mcc_radius);
    let mcc_end = (mcc_bucket + mcc_radius).min(MCC_BUCKETS - 1);

    let mut bucket_keys = Vec::with_capacity(16);
    let mut total_candidates = 0_usize;

    for amount in amount_start..=amount_end {
        for mcc in mcc_start..=mcc_end {
            let key = bucket_key_from_parts(
                has_last,
                is_online,
                card_present,
                unknown_merchant,
                mcc,
                amount,
            );

            total_candidates += dataset.buckets[key].len();
            bucket_keys.push(key);
        }
    }

    if total_candidates < 5 {
        return None;
    }

    let step = if total_candidates > MAX_CANDIDATES_PER_QUERY {
        (total_candidates / MAX_CANDIDATES_PER_QUERY).max(1)
    } else {
        1
    };

    let mut top: [(u64, u8); 5] = [(u64::MAX, 0); 5];
    let mut filled = 0_usize;
    let mut checked = 0_usize;
    let mut global_pos = 0_usize;

    for key in bucket_keys {
        let candidates = &dataset.buckets[key];

        for &idx_u32 in candidates {
            if step > 1 && global_pos % step != 0 {
                global_pos += 1;
                continue;
            }

            let idx = idx_u32 as usize;
            let offset = idx * DIMS;

            let dist = distance_squared_quantized(query_q, &dataset.vectors, offset);
            let label = dataset.labels[idx];

            update_top5(&mut top, &mut filled, dist, label);

            checked += 1;
            global_pos += 1;

            if checked >= MAX_CANDIDATES_PER_QUERY {
                break;
            }
        }

        if checked >= MAX_CANDIDATES_PER_QUERY {
            break;
        }
    }

    if filled < 5 {
        return None;
    }

    let frauds = top.iter().filter(|(_, label)| *label == 1).count();

    Some(frauds as f32 / 5.0)
}

pub fn fraud_score_bucket(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);

    let score = match fraud_score_bucket_range(&query_q, dataset, 0, 1) {
        Some(score) => score,
        None => return fraud_score_full_scan_quantized(&query_q, dataset),
    };

    if score == 0.4 || score == 0.6 {
        return match fraud_score_bucket_range(&query_q, dataset, 1, 1) {
            Some(expanded_score) => expanded_score,
            None => fraud_score_full_scan_quantized(&query_q, dataset),
        };
    }

    score
}

fn fraud_score_full_scan_quantized(query_q: &[u16; DIMS], dataset: &Dataset) -> f32 {
    let mut top: [(u64, u8); 5] = [(u64::MAX, 0); 5];
    let mut filled = 0_usize;

    for idx in 0..dataset.len {
        let offset = idx * DIMS;

        let dist = distance_squared_quantized(query_q, &dataset.vectors, offset);
        let label = dataset.labels[idx];

        update_top5(&mut top, &mut filled, dist, label);
    }

    let frauds = top.iter().filter(|(_, label)| *label == 1).count();

    frauds as f32 / 5.0
}

pub fn fraud_score_full(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);
    fraud_score_full_scan_quantized(&query_q, dataset)
}

pub fn count_bucket_candidates(query: &Vector, dataset: &Dataset) -> usize {
    let query_q = quantize_query(query);

    let amount_bucket = normalized_bucket(query_q[0], AMOUNT_BUCKETS);
    let has_last = if query_q[5] == 0 { 0 } else { 1 };
    let is_online = bool_bucket(query_q[9]);
    let card_present = bool_bucket(query_q[10]);
    let unknown_merchant = bool_bucket(query_q[11]);
    let mcc_bucket = normalized_bucket(query_q[12], MCC_BUCKETS);

    let amount_start = amount_bucket;
    let amount_end = amount_bucket;

    let mcc_start = mcc_bucket.saturating_sub(1);
    let mcc_end = (mcc_bucket + 1).min(MCC_BUCKETS - 1);

    let mut total = 0_usize;

    for amount in amount_start..=amount_end {
        for mcc in mcc_start..=mcc_end {
            let key = bucket_key_from_parts(
                has_last,
                is_online,
                card_present,
                unknown_merchant,
                mcc,
                amount,
            );

            total += dataset.buckets[key].len();
        }
    }

    total
}