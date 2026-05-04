use std::time::Instant;

use rinha_fraud_rust::{
    config::AppConfig,
    dataset::Dataset,
    models::FraudScoreRequest,
    search::{count_bucket_candidates, fraud_score_bucket, fraud_score_bucket_only, fraud_score_full},
    vectorizer::vectorize,
};

fn percentile(values: &[u128], p: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }

    let index = ((values.len() as f64 - 1.0) * p).round() as usize;
    values[index]
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

    // Warmup
    let mut fallback_candidates = 0;

    for vector in &vectors {
        let bucket_only = fraud_score_bucket_only(vector, &dataset);

        if bucket_only == 0.4 || bucket_only == 0.6 {
            fallback_candidates += 1;
        }
    }

    println!("Fallback candidates in example payloads: {}/{}", fallback_candidates, vectors.len());

    let mut candidate_counts = vectors
        .iter()
        .map(|vector| count_bucket_candidates(vector, &dataset))
        .collect::<Vec<_>>();

    candidate_counts.sort_unstable();

    let total_candidates: usize = candidate_counts.iter().sum();
    let avg_candidates = total_candidates as f64 / candidate_counts.len() as f64;

    println!("--- Candidate count ---");
    println!("min candidates: {}", candidate_counts[0]);
    println!("avg candidates: {:.2}", avg_candidates);
    println!("p50 candidates: {}", candidate_counts[candidate_counts.len() / 2]);
    println!("max candidates: {}", candidate_counts[candidate_counts.len() - 1]);

    let iterations = 1_000;
    let mut durations = Vec::with_capacity(iterations);

    let start_total = Instant::now();

    for i in 0..iterations {
        let vector = &vectors[i % vectors.len()];

        let start = Instant::now();
        let _score = fraud_score_bucket(vector, &dataset);
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

    println!("--- Bucket search benchmark ---");
    println!("Iterations: {}", iterations);
    println!("Total time: {:.2?}", total_elapsed);
    println!("min: {} µs", min);
    println!("avg: {:.2} µs", avg);
    println!("p50: {} µs", p50);
    println!("p90: {} µs", p90);
    println!("p95: {} µs", p95);
    println!("p99: {} µs", p99);
    println!("max: {} µs", max);

    println!();
    println!("Running one full scan sample for comparison...");

    let full_start = Instant::now();
    let full_score = fraud_score_full(&vectors[0], &dataset);
    let full_elapsed = full_start.elapsed();

    println!("Full scan score: {}", full_score);
    println!("Full scan elapsed: {:.2?}", full_elapsed);

    Ok(())
}
