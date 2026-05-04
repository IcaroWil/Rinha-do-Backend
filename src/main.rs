mod api;
mod config;
mod dataset;
mod models;
mod search;
mod vectorizer;

use std::sync::Arc;

use api::{router, AppState};
use config::AppConfig;
use dataset::Dataset;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load()?;
    let dataset = Dataset::load_index()?;

    println!("Loaded {} references", dataset.len);

    let state = AppState {
        config: Arc::new(config),
        dataset: Arc::new(dataset),
    };

    let app = router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;

    println!("API listening on 0.0.0.0:8080");

    axum::serve(listener, app).await?;

    Ok(())
}