use std::sync::Arc;

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

fn main() -> anyhow::Result<()> {
    let config = Arc::new(AppConfig::load()?);
    let dataset = Dataset::load_index()?;

    let raw = std::fs::read_to_string("data/example-payloads.json")?;
    let payloads: Vec<FraudScoreRequest> = serde_json::from_str(&raw)?;

    let mut total = 0;
    let mut legacy_same_score = 0;
    let mut legacy_same_decision = 0;
    let mut current_bounded_same_score = 0;
    let mut current_bounded_same_decision = 0;
    let mut improved_bounded_same_score = 0;
    let mut improved_bounded_same_decision = 0;

    for payload in &payloads {
        let vector = vectorize(payload, &config);

        let full = fraud_score_full(&vector, &dataset);
        let legacy = fraud_score_bucket_legacy(&vector, &dataset);
        let current_bounded = fraud_score_bucket_bounded_v1(&vector, &dataset);
        let improved_bounded = fraud_score_bucket_bounded_v2(&vector, &dataset);

        let full_approved = full < 0.6;
        let legacy_approved = legacy < 0.6;
        let current_bounded_approved = current_bounded < 0.6;
        let improved_bounded_approved = improved_bounded < 0.6;

        total += 1;

        if (full - legacy).abs() < f32::EPSILON {
            legacy_same_score += 1;
        }

        if full_approved == legacy_approved {
            legacy_same_decision += 1;
        }

        if (full - current_bounded).abs() < f32::EPSILON {
            current_bounded_same_score += 1;
        }

        if full_approved == current_bounded_approved {
            current_bounded_same_decision += 1;
        }

        if (full - improved_bounded).abs() < f32::EPSILON {
            improved_bounded_same_score += 1;
        }

        if full_approved == improved_bounded_approved {
            improved_bounded_same_decision += 1;
        }

        if full_approved != legacy_approved
            || full_approved != current_bounded_approved
            || full_approved != improved_bounded_approved
        {
            println!(
                "DIFF id={} full_score={} legacy_score={} current_bounded_score={} improved_bounded_score={} full_approved={} legacy_approved={} current_bounded_approved={} improved_bounded_approved={}",
                payload.id,
                full,
                legacy,
                current_bounded,
                improved_bounded,
                full_approved,
                legacy_approved,
                current_bounded_approved,
                improved_bounded_approved
            );
        }
    }

    println!("Total payloads: {}", total);
    println!("Legacy same score: {}/{}", legacy_same_score, total);
    println!("Legacy same decision: {}/{}", legacy_same_decision, total);
    println!(
        "Current bounded same score: {}/{}",
        current_bounded_same_score, total
    );
    println!(
        "Current bounded same decision: {}/{}",
        current_bounded_same_decision, total
    );
    println!(
        "Improved bounded same score: {}/{}",
        improved_bounded_same_score, total
    );
    println!(
        "Improved bounded same decision: {}/{}",
        improved_bounded_same_decision, total
    );

    Ok(())
}
