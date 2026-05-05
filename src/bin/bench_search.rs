use std::time::Instant;

use rinha_fraud_rust::{
    config::AppConfig,
    dataset::Dataset,
    models::FraudScoreRequest,
    search::{
        fraud_score_bucket_bounded_v1, fraud_score_bucket_bounded_v2,
        fraud_score_bucket_legacy, fraud_score_full,
    },
    vectorizer::vectorize,
};

fn percentile(values: &[u128], p: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }

    let index = ((values.len() as f64 - 1.0) * p).round() as usize;
    values[index]
}

fn run_benchmark(
    name: &str,
    vectors: &[[f32; 14]],
    dataset: &Dataset,
    search_fn: fn(&[f32; 14], &Dataset) -> f32,
) {
    for vector in vectors {
        let _ = search_fn(vector, dataset);
    }

    let iterations = 1_000;
    let mut durations = Vec::with_capacity(iterations);

    let start_total = Instant::now();

    for i in 0..iterations {
        let vector = &vectors[i % vectors.len()];

        let start = Instant::now();
        let _score = search_fn(vector, dataset);
        let elapsed = start.elapsed().as_micros();

        durations.push(elapsed);
    }

    let total_elapsed = start_total.elapsed();

    durations.sort_unstable();

    let min = durations[0];
    let max = durations[durations.len() - 1];
    let p50 = percentile(&durations, 0.50);
    let p90 = percentile(&durations, 0.90);
    let p95 = percentile(&durations, 0.95);
    let p99 = percentile(&durations, 0.99);
    let avg = durations.iter().sum::<u128>() as f64 / durations.len() as f64;

    println!("--- {} ---", name);
    println!("Iterations: {}", iterations);
    println!("Total time: {:.2?}", total_elapsed);
    println!("min: {} us", min);
    println!("avg: {:.2} us", avg);
    println!("p50: {} us", p50);
    println!("p90: {} us", p90);
    println!("p95: {} us", p95);
    println!("p99: {} us", p99);
    println!("max: {} us", max);
    println!();
}

fn main() -> anyhow::Result<()> {
    let config = AppConfig::load()?;
    let dataset = Dataset::load_index()?;

    let raw = std::fs::read_to_string("data/example-payloads.json")?;
    let payloads: Vec<FraudScoreRequest> = serde_json::from_str(&raw)?;

    let vectors = payloads
        .iter()
        .map(|payload| vectorize(payload, &config))
        .collect::<Vec<_>>();

    println!("Payloads loaded: {}", vectors.len());
    println!("Dataset references: {}", dataset.len);
    println!();

    run_benchmark("Legacy bucket search", &vectors, &dataset, fraud_score_bucket_legacy);
    run_benchmark(
        "Current bounded search",
        &vectors,
        &dataset,
        fraud_score_bucket_bounded_v1,
    );
    run_benchmark(
        "Improved bounded search",
        &vectors,
        &dataset,
        fraud_score_bucket_bounded_v2,
    );

    println!("Running one full scan sample for comparison...");

    let full_start = Instant::now();
    let full_score = fraud_score_full(&vectors[0], &dataset);
    let full_elapsed = full_start.elapsed();

    println!("Full scan score: {}", full_score);
    println!("Full scan elapsed: {:.2?}", full_elapsed);

    Ok(())
}
