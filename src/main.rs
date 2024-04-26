use std::{collections::{HashSet, HashMap}, sync::{Arc, Mutex}};
use axum::{Router, routing::get, extract::{WebSocketUpgrade, State, ws::{WebSocket, Message}}, response::{IntoResponse, Html}};
use futures::{ StreamExt, SinkExt};
use serde::Deserialize;
use tokio::{sync::broadcast, net::TcpListener};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

struct AppState{
    rooms: Mutex<HashMap<String, RoomState>>
}

struct RoomState{
    user_set:HashSet<String>,
    tx:broadcast::Sender<String>
}

impl RoomState{
    fn new() -> Self{
        Self {  
            user_set: HashSet::new(),
            tx:broadcast::channel(100).0
        }
    }
}

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

    let app_state = Arc::new(AppState{rooms});

    let app:Router = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .with_state(app_state);

    let listner = TcpListener::bind("127.0.0.1:5001").await.unwrap();

    println!("Server is running...");

    axum::serve(listner, app).await.unwrap();

}

async fn ws_handler(
    ws:WebSocketUpgrade,
    State(state):State<Arc<AppState>>
) -> impl IntoResponse{
    let state_clone = state.clone(); 
    ws.on_upgrade(|socket| websocket(socket, state_clone))
}

async fn websocket(stream:WebSocket,state:Arc<AppState>){
    let (mut sender, mut receiver) = stream.split();

    let mut tx = None::<broadcast::Sender<String>>;
    let mut username = String::new();
    let mut channel = String::new();

    //Loop until a text msg is found
    while let Some(Ok(message)) = receiver.next().await{
        if let Message::Text(name) = message {
            #[derive(Deserialize)]
            struct Connect{
                username: String,
                channel: String
            }

            let connect: Connect = match serde_json::from_str(&name){
                Ok(connect) => connect,
                Err(error) => {
                    tracing::error!(%error);
                    let _ = sender.send(Message::Text(String::from("Failed to parse connect message",)))
                    .await;
                    break;
                }
            };

            {
                let mut rooms = state.rooms.lock().unwrap();

                channel = connect.channel.clone();
                let room = rooms.entry(connect.channel).or_insert_with(RoomState::new);

                tx = Some(room.tx.clone());

                if !room.user_set.contains(&connect.username){
                    room.user_set.insert(connect.username.to_owned());
                    username = connect.username.clone();
                }
            }

            if tx.is_some() && !username.is_empty(){
                break;
            }else {
                let _ = sender.send(
                    Message::Text(String::from("Username already taken."))
                ).await;

                return ;
            }
        }
    }

    let tx = tx.unwrap();

    let mut rx = tx.subscribe();

    let msg = format!("--------------{} joined the chat--------------",username);
    let _ = tx.send(msg);

    let mut send_task = tokio::spawn(
        async move{
            while let Ok(msg) = rx.recv().await {
                if sender.send(Message::Text(msg)).await.is_err(){
                    break;
                }
            }
        });

    let mut recv_task ={
        let tx = tx.clone();
        let name = username.clone();

        tokio::spawn(async move{
            while let Some(Ok(Message::Text(text))) = receiver.next().await{
                let _ = tx.send(format!("{}: {}",name,text));
            }
        })
    };

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }

    let msg = format!("--------------{} left the chat--------------",username);

    let _ = tx.send(msg);

    let mut rooms = state.rooms.lock().unwrap();

    rooms.get_mut(&channel).unwrap().user_set.remove(&username);
}

async fn index() -> Html<&'static str>{
    Html(std::include_str!("../chat.html"))
}
