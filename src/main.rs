use std::{collections::{HashMap}, sync::{Arc, Mutex}, time::Duration};
use axum::{Router, routing::get, response::{Html}};
use tokio::{net::TcpListener};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};
use sqlx::{postgres::{PgPoolOptions}};
use ws_handler::{ws_handler, AppState};

mod queries;
mod ws_handler;
mod websocket;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
    .with(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "example_chat=trace".into()),
    )
    .with(tracing_subscriber::fmt::layer())
    .init();

    let rooms = Mutex::new(HashMap::new());
    let db_url = "postgres://postgres:password@localhost:5432/axum_chat".to_string();
    
    let db_pool = PgPoolOptions::new()
    .max_connections(5)
    .acquire_timeout(Duration::from_secs(5))
    .connect(&db_url)
    .await
    .expect("Could not conect to database");

let app_state = Arc::new(AppState{rooms,db_pool});
    let app:Router = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .with_state(app_state);
    let listner = TcpListener::bind("127.0.0.1:5001").await.unwrap();
    println!("Server is running...||Database is connected...");
    axum::serve(listner, app).await.unwrap();
}

async fn index() -> Html<&'static str>{
    Html(std::include_str!("../chat.html"))
}
