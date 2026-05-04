use crate::{
    dataset::{
        bucket_key_from_parts, AMOUNT_BUCKETS, Dataset, MCC_BUCKETS,
    },
    vectorizer::{Vector, DIMS},
};

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

#[inline]
fn distance_squared_quantized(query: &[u16; DIMS], vectors: &[u16], offset: usize) -> u64 {
    let mut sum = 0_u64;

    for i in 0..DIMS {
        let diff = query[i] as i64 - vectors[offset + i] as i64;
        sum += (diff * diff) as u64;
    }

    sum
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

pub fn fraud_score_bucket(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);

    let amount_bucket = normalized_bucket(query_q[0], AMOUNT_BUCKETS);
    let has_last = if query_q[5] == 0 { 0 } else { 1 };
    let is_online = bool_bucket(query_q[9]);
    let card_present = bool_bucket(query_q[10]);
    let unknown_merchant = bool_bucket(query_q[11]);
    let mcc_bucket = normalized_bucket(query_q[12], MCC_BUCKETS);

    let amount_start = amount_bucket.saturating_sub(2);
    let amount_end = (amount_bucket + 2).min(AMOUNT_BUCKETS - 1);

    let mcc_start = mcc_bucket.saturating_sub(1);
    let mcc_end = (mcc_bucket + 1).min(MCC_BUCKETS - 1);

    let mut top: [(u64, u8); 5] = [(u64::MAX, 0); 5];
    let mut filled = 0_usize;
    let mut checked = 0_usize;

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

            let candidates = &dataset.buckets[key];

            for &idx_u32 in candidates {
                let idx = idx_u32 as usize;
                let offset = idx * DIMS;

                let dist = distance_squared_quantized(&query_q, &dataset.vectors, offset);
                let label = dataset.labels[idx];

                update_top5(&mut top, &mut filled, dist, label);
                checked += 1;
            }
        }
    }

    // Segurança: se por algum motivo o bucket vier pequeno demais,
    // fazemos fallback para busca completa para manter a qualidade.
    if checked < 5 {
        return fraud_score_full_scan_quantized(&query_q, dataset);
    }

    let frauds = top.iter().filter(|(_, label)| *label == 1).count();
    let score = frauds as f32 / 5.0;

    // A decisão só muda na fronteira 0.4 ↔ 0.6.
    // Para esses casos, confirmamos com busca exata.
    if score == 0.4 || score == 0.6 {
        return fraud_score_full_scan_quantized(&query_q, dataset);
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