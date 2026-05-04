use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::{
    config::AppConfig,
    dataset::Dataset,
    models::{FraudScoreRequest, FraudScoreResponse},
    search::fraud_score_bruteforce,
    vectorizer::vectorize,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub dataset: Arc<Dataset>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/ready", get(ready))
        .route("/fraud-score", post(fraud_score))
        .with_state(state)
}

async fn ready() -> StatusCode {
    StatusCode::OK
}

async fn fraud_score(
    State(state): State<AppState>,
    Json(payload): Json<FraudScoreRequest>,
) -> Json<FraudScoreResponse> {
    let vector = vectorize(&payload, &state.config);
    let fraud_score = fraud_score_bruteforce(&vector, &state.dataset);

    Json(FraudScoreResponse {
        approved: fraud_score < 0.6,
        fraud_score,
    })
}