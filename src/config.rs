use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Normalization {
    pub max_amount: f32,
    pub max_installments: f32,
    pub amount_vs_avg_ratio: f32,
    pub max_minutes: f32,
    pub max_km: f32,
    pub max_tx_count_24h: f32,
    pub max_merchant_avg_amount: f32,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub normalization: Normalization,
    pub mcc_risk: HashMap<String, f32>,
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let normalization_raw = std::fs::read_to_string("/app/data/normalization.json")
            .or_else(|_| std::fs::read_to_string("data/normalization.json"))?;

        let mcc_raw = std::fs::read_to_string("/app/data/mcc_risk.json")
            .or_else(|_| std::fs::read_to_string("data/mcc_risk.json"))?;

        let normalization: Normalization = serde_json::from_str(&normalization_raw)?;
        let mcc_risk: HashMap<String, f32> = serde_json::from_str(&mcc_raw)?;

        Ok(Self {
            normalization,
            mcc_risk,
        })
    }
}