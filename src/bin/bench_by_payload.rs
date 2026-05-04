use std::time::Instant;

use rinha_fraud_rust::{
    config::AppConfig,
    dataset::Dataset,
    models::FraudScoreRequest,
    search::{count_bucket_candidates, fraud_score_bucket, fraud_score_full},
    vectorizer::vectorize,
};

#[derive(Debug)]
struct PayloadBench {
    id: String,
    approved: bool,
    score: f32,
    avg_us: f64,
    min_us: u128,
    max_us: u128,
    full_score: f32,
    candidates: usize,
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

        // Warmup
        for _ in 0..10 {
            let _ = fraud_score_bucket(&vector, &dataset);
        }

        let mut durations = Vec::with_capacity(iterations_per_payload);

        let mut score = 0.0;

        for _ in 0..iterations_per_payload {
            let start = Instant::now();
            score = fraud_score_bucket(&vector, &dataset);
            durations.push(start.elapsed().as_micros());
        }

        durations.sort_unstable();

        let min_us = durations[0];
        let max_us = durations[durations.len() - 1];
        let avg_us = durations.iter().sum::<u128>() as f64 / durations.len() as f64;

        let full_score = fraud_score_full(&vector, &dataset);
        let candidates = count_bucket_candidates(&vector, &dataset);

        results.push(PayloadBench {
            id: payload.id,
            approved: score < 0.6,
            score,
            avg_us,
            min_us,
            max_us,
            full_score,
            candidates,
        });
    }

    results.sort_by(|a, b| b.avg_us.partial_cmp(&a.avg_us).unwrap());

    println!("--- Slowest payloads ---");

    for result in &results {
        println!(
            "{} | approved={} | score={} | full_score={} | candidates={} | avg={:.2}µs | min={}µs | max={}µs",
            result.id,
            result.approved,
            result.score,
            result.full_score,
            result.candidates,
            result.avg_us,
            result.min_us,
            result.max_us
        );
    }

    Ok(())
}
