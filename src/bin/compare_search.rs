use std::sync::Arc;

use rinha_fraud_rust::{
    config::AppConfig,
    dataset::Dataset,
    models::FraudScoreRequest,
    search::{fraud_score_bucket, fraud_score_full},
    vectorizer::vectorize,
};

fn main() -> anyhow::Result<()> {
    let config = Arc::new(AppConfig::load()?);
    let dataset = Dataset::load_index()?;

    let raw = std::fs::read_to_string("data/example-payloads.json")?;
    let payloads: Vec<FraudScoreRequest> = serde_json::from_str(&raw)?;

    let mut total = 0;
    let mut same_score = 0;
    let mut same_decision = 0;

    for payload in &payloads {
        let vector = vectorize(payload, &config);

        let full = fraud_score_full(&vector, &dataset);
        let bucket = fraud_score_bucket(&vector, &dataset);

        let full_approved = full < 0.6;
        let bucket_approved = bucket < 0.6;

        total += 1;

        if (full - bucket).abs() < f32::EPSILON {
            same_score += 1;
        }

        if full_approved == bucket_approved {
            same_decision += 1;
        }

        if full_approved != bucket_approved {
            println!(
                "DIFF id={} full_score={} bucket_score={} full_approved={} bucket_approved={}",
                payload.id, full, bucket, full_approved, bucket_approved
            );
        }
    }

    println!("Total payloads: {}", total);
    println!("Same score: {}/{}", same_score, total);
    println!("Same decision: {}/{}", same_decision, total);

    Ok(())
}
