use crate::config::AppConfig;
use crate::models::FraudScoreRequest;
use chrono::{Datelike, Timelike};

pub const DIMS: usize = 14;

pub type Vector = [f32; DIMS];

#[inline]
fn clamp01(value: f32) -> f32 {
    if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}

pub fn vectorize(req: &FraudScoreRequest, config: &AppConfig) -> Vector {
    let n = &config.normalization;

    let requested_at = chrono::DateTime::parse_from_rfc3339(&req.transaction.requested_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .ok();

    let hour = requested_at
        .as_ref()
        .map(|dt| dt.hour() as f32)
        .unwrap_or(0.0);

    let day_of_week = requested_at
        .as_ref()
        .map(|dt| dt.weekday().num_days_from_monday() as f32)
        .unwrap_or(0.0);

    let amount_vs_avg = if req.customer.avg_amount > 0.0 {
        (req.transaction.amount / req.customer.avg_amount) / n.amount_vs_avg_ratio
    } else {
        1.0
    };

    let (minutes_since_last, km_from_last) = match &req.last_transaction {
        Some(last) => {
            let minutes = match requested_at {
                Some(current_dt) => chrono::DateTime::parse_from_rfc3339(&last.timestamp)
                    .map(|last_dt| {
                        let last_utc = last_dt.with_timezone(&chrono::Utc);
                        let diff = current_dt.signed_duration_since(last_utc);
                        diff.num_minutes().max(0) as f32
                    })
                    .unwrap_or(0.0),
                None => 0.0,
            };

            (
                clamp01(minutes / n.max_minutes),
                clamp01(last.km_from_current / n.max_km),
            )
        }
        None => (-1.0, -1.0),
    };

    let unknown_merchant = if req.customer.known_merchants.iter().any(|m| m == &req.merchant.id) {
        0.0
    } else {
        1.0
    };

    let mcc_risk = config
        .mcc_risk
        .get(&req.merchant.mcc)
        .copied()
        .unwrap_or(0.5);

    [
        clamp01(req.transaction.amount / n.max_amount),
        clamp01(req.transaction.installments as f32 / n.max_installments),
        clamp01(amount_vs_avg),
        hour / 23.0,
        day_of_week / 6.0,
        minutes_since_last,
        km_from_last,
        clamp01(req.terminal.km_from_home / n.max_km),
        clamp01(req.customer.tx_count_24h as f32 / n.max_tx_count_24h),
        if req.terminal.is_online { 1.0 } else { 0.0 },
        if req.terminal.card_present { 1.0 } else { 0.0 },
        unknown_merchant,
        mcc_risk,
        clamp01(req.merchant.avg_amount / n.max_merchant_avg_amount),
    ]
}