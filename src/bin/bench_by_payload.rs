use std::time::Instant;

use rinha_fraud_rust::{
    config::AppConfig,
    dataset::Dataset,
    models::FraudScoreRequest,
    search::{
        count_bool_slice_candidates, count_bucket_candidates, fraud_score_bucket_bounded_v1,
        fraud_score_bucket_bounded_v2, fraud_score_bucket_legacy, fraud_score_full,
    },
    vectorizer::vectorize,
};

#[derive(Debug)]
struct PayloadBench {
    id: String,
    full_score: f32,
    legacy_score: f32,
    current_bounded_score: f32,
    improved_bounded_score: f32,
    primary_candidates: usize,
    bool_slice_candidates: usize,
    legacy_avg_us: f64,
    current_bounded_avg_us: f64,
    improved_bounded_avg_us: f64,
    legacy_max_us: u128,
    current_bounded_max_us: u128,
    improved_bounded_max_us: u128,
}

fn bench_payload(
    vector: &[f32; 14],
    dataset: &Dataset,
    search_fn: fn(&[f32; 14], &Dataset) -> f32,
    iterations: usize,
) -> (f32, f64, u128) {
    for _ in 0..10 {
        let _ = search_fn(vector, dataset);
    }

    let mut durations = Vec::with_capacity(iterations);
    let mut score = 0.0;

    for _ in 0..iterations {
        let start = Instant::now();
        score = search_fn(vector, dataset);
        durations.push(start.elapsed().as_micros());
    }

    durations.sort_unstable();

    let avg_us = durations.iter().sum::<u128>() as f64 / durations.len() as f64;
    let max_us = durations[durations.len() - 1];

    (score, avg_us, max_us)
}

fn main() -> anyhow::Result<()> {
    let config = AppConfig::load()?;
    let dataset = Dataset::load_index()?;

    let raw = std::fs::read_to_string("data/example-payloads.json")?;
    let payloads: Vec<FraudScoreRequest> = serde_json::from_str(&raw)?;

    let iterations_per_payload = 100;
    let mut results = Vec::new();

    for payload in payloads {
        let vector = vectorize(&payload, &config);
        let full_score = fraud_score_full(&vector, &dataset);
        let (legacy_score, legacy_avg_us, legacy_max_us) =
            bench_payload(&vector, &dataset, fraud_score_bucket_legacy, iterations_per_payload);
        let (current_bounded_score, current_bounded_avg_us, current_bounded_max_us) =
            bench_payload(
                &vector,
                &dataset,
                fraud_score_bucket_bounded_v1,
                iterations_per_payload,
            );
        let (improved_bounded_score, improved_bounded_avg_us, improved_bounded_max_us) =
            bench_payload(
                &vector,
                &dataset,
                fraud_score_bucket_bounded_v2,
                iterations_per_payload,
            );

        results.push(PayloadBench {
            id: payload.id,
            full_score,
            legacy_score,
            current_bounded_score,
            improved_bounded_score,
            primary_candidates: count_bucket_candidates(&vector, &dataset),
            bool_slice_candidates: count_bool_slice_candidates(&vector, &dataset),
            legacy_avg_us,
            current_bounded_avg_us,
            improved_bounded_avg_us,
            legacy_max_us,
            current_bounded_max_us,
            improved_bounded_max_us,
        });
    }

    results.sort_by(|a, b| {
        b.primary_candidates
            .cmp(&a.primary_candidates)
            .then_with(|| b.legacy_max_us.cmp(&a.legacy_max_us))
    });

    println!("--- Payload comparison ---");

    for result in &results {
        println!(
            "{} | full={} | legacy={} | current_bounded={} | improved_bounded={} | primary_candidates={} | bool_slice_candidates={} | legacy_avg={:.2}us | current_bounded_avg={:.2}us | improved_bounded_avg={:.2}us | legacy_max={}us | current_bounded_max={}us | improved_bounded_max={}us",
            result.id,
            result.full_score,
            result.legacy_score,
            result.current_bounded_score,
            result.improved_bounded_score,
            result.primary_candidates,
            result.bool_slice_candidates,
            result.legacy_avg_us,
            result.current_bounded_avg_us,
            result.improved_bounded_avg_us,
            result.legacy_max_us,
            result.current_bounded_max_us,
            result.improved_bounded_max_us
        );
    }

    Ok(())
}
