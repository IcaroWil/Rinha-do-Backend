use crate::{
    dataset::{bucket_key_from_parts, Dataset, AMOUNT_BUCKETS, MCC_BUCKETS},
    vectorizer::{Vector, DIMS},
};

const LEGACY_MAX_CANDIDATES_PER_QUERY: usize = 50_000;
const PRIMARY_MAX_CANDIDATES_PER_QUERY: usize = 8_192;
const EXPANDED_MAX_CANDIDATES_PER_QUERY: usize = 16_384;
const BOOL_SLICE_MAX_CANDIDATES_PER_QUERY: usize = 12_288;

#[derive(Clone, Copy)]
struct ProbeResult {
    top: [(u64, u8); 5],
    filled: usize,
}

impl ProbeResult {
    #[inline]
    fn empty() -> Self {
        Self {
            top: [(u64::MAX, 0); 5],
            filled: 0,
        }
    }

    #[inline]
    fn score(self) -> Option<f32> {
        if self.filled < 5 {
            return None;
        }

        let frauds = self.top.iter().filter(|(_, label)| *label == 1).count();
        Some(frauds as f32 / 5.0)
    }
}

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

#[inline]
fn merge_probe_results(mut left: ProbeResult, right: ProbeResult) -> ProbeResult {
    for i in 0..right.filled {
        let (dist, label) = right.top[i];
        update_top5(&mut left.top, &mut left.filled, dist, label);
    }

    left
}

#[inline]
fn query_seed(query_q: &[u16; DIMS]) -> usize {
    let mut seed = 0_usize;

    for &value in query_q {
        seed = seed.wrapping_mul(131).wrapping_add(value as usize);
    }

    seed
}

#[inline]
fn query_shape(query_q: &[u16; DIMS]) -> (usize, usize, usize, usize, usize, usize) {
    (
        normalized_bucket(query_q[0], AMOUNT_BUCKETS),
        if query_q[5] == 0 { 0 } else { 1 },
        bool_bucket(query_q[9]),
        bool_bucket(query_q[10]),
        bool_bucket(query_q[11]),
        normalized_bucket(query_q[12], MCC_BUCKETS),
    )
}

fn collect_bucket_keys(
    amount_start: usize,
    amount_end: usize,
    mcc_start: usize,
    mcc_end: usize,
    has_last: usize,
    is_online: usize,
    card_present: usize,
    unknown_merchant: usize,
) -> Vec<usize> {
    let mut bucket_keys =
        Vec::with_capacity((amount_end - amount_start + 1) * (mcc_end - mcc_start + 1));

    for amount in amount_start..=amount_end {
        for mcc in mcc_start..=mcc_end {
            bucket_keys.push(bucket_key_from_parts(
                has_last,
                is_online,
                card_present,
                unknown_merchant,
                mcc,
                amount,
            ));
        }
    }

    bucket_keys
}

fn probe_bucket_keys_with_phase(
    query_q: &[u16; DIMS],
    dataset: &Dataset,
    bucket_keys: &[usize],
    max_candidates: usize,
    phase: usize,
) -> ProbeResult {
    let total_candidates = bucket_keys
        .iter()
        .map(|&key| dataset.buckets[key].len())
        .sum::<usize>();

    if total_candidates == 0 {
        return ProbeResult::empty();
    }

    let step = if total_candidates > max_candidates {
        (total_candidates / max_candidates).max(1)
    } else {
        1
    };

    let seed = query_seed(query_q).wrapping_add(phase.wrapping_mul(1_000_003));
    let mut top = [(u64::MAX, 0); 5];
    let mut filled = 0_usize;
    let mut checked = 0_usize;

    for (bucket_idx, key) in bucket_keys.iter().enumerate() {
        let candidates = &dataset.buckets[*key];

        if candidates.is_empty() {
            continue;
        }

        let mut pos = if step == 1 {
            0
        } else if candidates.len() <= step {
            seed % candidates.len()
        } else {
            (seed + bucket_idx) % step
        };

        while pos < candidates.len() && checked < max_candidates {
            let idx_u32 = candidates[pos];
            let idx = idx_u32 as usize;
            let offset = idx * DIMS;
            let dist = distance_squared_quantized(query_q, &dataset.vectors, offset);
            let label = dataset.labels[idx];

            update_top5(&mut top, &mut filled, dist, label);

            checked += 1;
            pos += step;
        }
    }

    ProbeResult { top, filled }
}

fn probe_bucket_range_with_phase(
    query_q: &[u16; DIMS],
    dataset: &Dataset,
    amount_radius: usize,
    mcc_radius: usize,
    max_candidates: usize,
    phase: usize,
) -> ProbeResult {
    let (amount_bucket, has_last, is_online, card_present, unknown_merchant, mcc_bucket) =
        query_shape(query_q);

    let amount_start = amount_bucket.saturating_sub(amount_radius);
    let amount_end = (amount_bucket + amount_radius).min(AMOUNT_BUCKETS - 1);
    let mcc_start = mcc_bucket.saturating_sub(mcc_radius);
    let mcc_end = (mcc_bucket + mcc_radius).min(MCC_BUCKETS - 1);

    let bucket_keys = collect_bucket_keys(
        amount_start,
        amount_end,
        mcc_start,
        mcc_end,
        has_last,
        is_online,
        card_present,
        unknown_merchant,
    );

    probe_bucket_keys_with_phase(query_q, dataset, &bucket_keys, max_candidates, phase)
}

fn probe_bool_slice_with_phase(
    query_q: &[u16; DIMS],
    dataset: &Dataset,
    max_candidates: usize,
    phase: usize,
) -> ProbeResult {
    let (_, has_last, is_online, card_present, unknown_merchant, _) = query_shape(query_q);
    let bucket_keys = collect_bucket_keys(
        0,
        AMOUNT_BUCKETS - 1,
        0,
        MCC_BUCKETS - 1,
        has_last,
        is_online,
        card_present,
        unknown_merchant,
    );

    probe_bucket_keys_with_phase(query_q, dataset, &bucket_keys, max_candidates, phase)
}

fn probe_global_sample(query_q: &[u16; DIMS], dataset: &Dataset) -> ProbeResult {
    let mut top = [(u64::MAX, 0); 5];
    let mut filled = 0_usize;

    for &idx_u32 in dataset.global_sample.as_slice() {
        let idx = idx_u32 as usize;
        let offset = idx * DIMS;
        let dist = distance_squared_quantized(query_q, &dataset.vectors, offset);
        let label = dataset.labels[idx];

        update_top5(&mut top, &mut filled, dist, label);
    }

    ProbeResult { top, filled }
}

fn score_with_bounded_search_v1(query_q: &[u16; DIMS], dataset: &Dataset) -> f32 {
    let primary = probe_bucket_range_with_phase(
        query_q,
        dataset,
        0,
        1,
        PRIMARY_MAX_CANDIDATES_PER_QUERY,
        0,
    );

    if let Some(score) = primary.score() {
        if score != 0.4 && score != 0.6 {
            return score;
        }
    }

    if primary.filled >= 5 {
        return probe_bucket_range_with_phase(
            query_q,
            dataset,
            1,
            1,
            EXPANDED_MAX_CANDIDATES_PER_QUERY,
            0,
        )
        .score()
        .unwrap_or(primary.score().unwrap_or(0.0));
    }

    if let Some(score) = probe_bool_slice_with_phase(
        query_q,
        dataset,
        BOOL_SLICE_MAX_CANDIDATES_PER_QUERY,
        0,
    )
    .score()
    {
        return score;
    }

    probe_global_sample(query_q, dataset).score().unwrap_or(0.0)
}

fn score_with_bounded_search_v2(query_q: &[u16; DIMS], dataset: &Dataset) -> f32 {
    let primary = probe_bucket_range_with_phase(
        query_q,
        dataset,
        0,
        1,
        PRIMARY_MAX_CANDIDATES_PER_QUERY,
        0,
    );

    if let Some(score) = primary.score() {
        if score != 0.4 && score != 0.6 {
            return score;
        }
    }

    if primary.filled >= 5 {
        let expanded = probe_bucket_range_with_phase(
            query_q,
            dataset,
            1,
            1,
            EXPANDED_MAX_CANDIDATES_PER_QUERY,
            0,
        );

        if expanded.score() == Some(0.6) {
            return merge_probe_results(
                expanded,
                probe_bucket_range_with_phase(
                    query_q,
                    dataset,
                    1,
                    1,
                    EXPANDED_MAX_CANDIDATES_PER_QUERY,
                    1,
                ),
            )
            .score()
            .unwrap_or(primary.score().unwrap_or(0.0));
        }

        return expanded.score().unwrap_or(primary.score().unwrap_or(0.0));
    }

    let bool_slice =
        probe_bool_slice_with_phase(query_q, dataset, BOOL_SLICE_MAX_CANDIDATES_PER_QUERY, 0);

    if bool_slice.score() == Some(0.6) {
        return merge_probe_results(
            bool_slice,
            probe_bool_slice_with_phase(
                query_q,
                dataset,
                BOOL_SLICE_MAX_CANDIDATES_PER_QUERY,
                1,
            ),
        )
        .score()
        .unwrap_or(0.0);
    }

    if let Some(score) = bool_slice.score() {
        return score;
    }

    probe_global_sample(query_q, dataset).score().unwrap_or(0.0)
}

fn fraud_score_bucket_range_legacy(
    query_q: &[u16; DIMS],
    dataset: &Dataset,
    amount_radius: usize,
    mcc_radius: usize,
) -> Option<f32> {
    let (amount_bucket, has_last, is_online, card_present, unknown_merchant, mcc_bucket) =
        query_shape(query_q);

    let amount_start = amount_bucket.saturating_sub(amount_radius);
    let amount_end = (amount_bucket + amount_radius).min(AMOUNT_BUCKETS - 1);
    let mcc_start = mcc_bucket.saturating_sub(mcc_radius);
    let mcc_end = (mcc_bucket + mcc_radius).min(MCC_BUCKETS - 1);

    let bucket_keys = collect_bucket_keys(
        amount_start,
        amount_end,
        mcc_start,
        mcc_end,
        has_last,
        is_online,
        card_present,
        unknown_merchant,
    );

    let total_candidates = bucket_keys
        .iter()
        .map(|&key| dataset.buckets[key].len())
        .sum::<usize>();

    if total_candidates < 5 {
        return None;
    }

    let step = if total_candidates > LEGACY_MAX_CANDIDATES_PER_QUERY {
        (total_candidates / LEGACY_MAX_CANDIDATES_PER_QUERY).max(1)
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

            if checked >= LEGACY_MAX_CANDIDATES_PER_QUERY {
                break;
            }
        }

        if checked >= LEGACY_MAX_CANDIDATES_PER_QUERY {
            break;
        }
    }

    ProbeResult { top, filled }.score()
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

    ProbeResult { top, filled }.score().unwrap_or(0.0)
}

pub fn fraud_score_bucket(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);
    score_with_bounded_search_v1(&query_q, dataset)
}

pub fn fraud_score_bucket_bounded_v1(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);
    score_with_bounded_search_v1(&query_q, dataset)
}

pub fn fraud_score_bucket_bounded_v2(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);
    score_with_bounded_search_v2(&query_q, dataset)
}

pub fn fraud_score_bucket_legacy(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);

    let score = match fraud_score_bucket_range_legacy(&query_q, dataset, 0, 1) {
        Some(score) => score,
        None => return fraud_score_full_scan_quantized(&query_q, dataset),
    };

    if score == 0.4 || score == 0.6 {
        return match fraud_score_bucket_range_legacy(&query_q, dataset, 1, 1) {
            Some(expanded_score) => expanded_score,
            None => fraud_score_full_scan_quantized(&query_q, dataset),
        };
    }

    score
}

pub fn fraud_score_full(query: &Vector, dataset: &Dataset) -> f32 {
    let query_q = quantize_query(query);
    fraud_score_full_scan_quantized(&query_q, dataset)
}

pub fn count_bucket_candidates(query: &Vector, dataset: &Dataset) -> usize {
    let query_q = quantize_query(query);
    let (amount_bucket, has_last, is_online, card_present, unknown_merchant, mcc_bucket) =
        query_shape(&query_q);

    let bucket_keys = collect_bucket_keys(
        amount_bucket,
        amount_bucket,
        mcc_bucket.saturating_sub(1),
        (mcc_bucket + 1).min(MCC_BUCKETS - 1),
        has_last,
        is_online,
        card_present,
        unknown_merchant,
    );

    bucket_keys
        .iter()
        .map(|&key| dataset.buckets[key].len())
        .sum::<usize>()
}

pub fn count_bool_slice_candidates(query: &Vector, dataset: &Dataset) -> usize {
    let query_q = quantize_query(query);
    let (_, has_last, is_online, card_present, unknown_merchant, _) = query_shape(&query_q);

    collect_bucket_keys(
        0,
        AMOUNT_BUCKETS - 1,
        0,
        MCC_BUCKETS - 1,
        has_last,
        is_online,
        card_present,
        unknown_merchant,
    )
    .iter()
    .map(|&key| dataset.buckets[key].len())
    .sum::<usize>()
}
