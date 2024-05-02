use std::{sync::{Arc, Mutex}, collections::{HashMap, HashSet}};

use axum::{extract::{WebSocketUpgrade, State}, response::IntoResponse};
use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::websocket::websocket;

pub struct AppState{
    pub rooms: Mutex<HashMap<String, RoomState>>,
    pub db_pool:PgPool
}

pub struct RoomState{
    pub user_set:HashSet<String>,
    pub tx:broadcast::Sender<String>
}

impl RoomState{
    pub fn new() -> Self{
        Self {  
            user_set: HashSet::new(),
            tx:broadcast::channel(100).0
        }
    }
}

pub async fn ws_handler(
    ws:WebSocketUpgrade,
    State(state):State<Arc<AppState>>
) -> impl IntoResponse{
    let state_clone = state.clone(); 
    ws.on_upgrade(|socket| websocket(socket, state_clone))
}