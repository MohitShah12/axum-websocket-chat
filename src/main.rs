use std::{collections::{HashSet, HashMap}, sync::{Arc, Mutex}, time::Duration};
use axum::{Router, routing::get, extract::{WebSocketUpgrade, State, ws::{WebSocket, Message}}, response::{IntoResponse, Html}};
use futures::{ StreamExt, SinkExt};
use serde::Deserialize;
use tokio::{sync::broadcast, net::TcpListener};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};
use sqlx::{postgres::{PgPoolOptions}, PgPool};

#[derive(Debug,sqlx::FromRow)]
struct Users{
    name:String,
}
struct AppState{
    rooms: Mutex<HashMap<String, RoomState>>,
    db_pool:PgPool
}

#[derive(Debug)]
#[derive(sqlx::FromRow, Clone)]
struct AdminUser{
    name:String,
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

async fn insert_user(pool:&PgPool, username:&str, channel:&str) -> Result<(),sqlx::Error>{
    sqlx::query("INSERT INTO chatuser (name,channel) VALUES ($1,$2)")
        .bind(username)
        .bind(channel)
        .execute(pool)
        .await?;
    Ok(())
}

async fn insert_msg(pool:&PgPool,message:String,channel:&String) -> Result<(),sqlx::Error>{
    println!("Inside query:{}",message);
    sqlx::query("INSERT INTO messages (chat, channel) VALUES ($1,$2)")
        .bind(message)
        .bind(channel)
        .execute(pool)
        .await?;
    Ok(())
}

async fn get_admin(pool:&PgPool, channel:&String) -> Vec<AdminUser>{
    let user = sqlx::query_as::<_,AdminUser>("SELECT name FROM chatuser WHERE channel = $1 LIMIT 1")
        .bind(channel)
        .fetch_all(pool)
    .await.unwrap();
    
    user
}

async fn get_all(pool:&PgPool, channel:&String)-> Vec<Users>{
    let user = sqlx::query_as::<_,Users>("SELECT name FROM chatuser WHERE channel = $1")
        .bind(channel)
        .fetch_all(pool)
    .await.unwrap();
    
    user
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

            if let Err(err) = insert_user(&state.db_pool, &connect.username, &connect.channel).await{
                tracing::error!("Failed to insert user to database {:?}",err);
                let _ = sender.send(Message::Text(String::from("Failed to insert use to database")))
                        .await;
                return ;
            }

            {
                let mut rooms = state.rooms.lock().unwrap();

                channel = connect.channel.clone();

                let room = rooms.entry(connect.channel).or_insert_with(RoomState::new);
                tx = Some(room.tx.clone());

                println!("hey {}",connect.username);

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

    if state.rooms.lock().unwrap().get(&channel).unwrap().user_set.len() == 1{
        let msg = format!(
            "--------------{} created a new channel: {}--------------",
            username, channel
        );
        let _ = tx.send(msg);
    } else{
        let user_set = state.rooms.lock().unwrap().get(&channel).unwrap().user_set.clone();
        println!("{:?}",user_set);
        if let Some(first_user) = user_set.iter().next() {
            println!("{:?}", first_user);
        } else {
            println!("User set is empty");
        }
        let admin = get_admin(&state.db_pool, &channel).await;
        // let ad_min = admin.get(1);
        println!("{:?}",admin[0].name);
        let msg = format!("--------------{} joined the {} chat created by {}--------------", username, channel,admin[0].name);
        let _ = tx.send(msg);
    };

    let user = get_all(&state.db_pool, &channel).await;
    for i in user{
        println!("{:?}",i.name);
        let _ = tx.send(i.name);
    }

    let state_clone = Arc::clone(&state);
    let channel_clone = channel.clone();
    let mut send_task = tokio::spawn(
        async move{
            while let Ok(msg) = rx.recv().await {
                if sender.send(Message::Text(msg.clone())).await.is_err(){
                    break;
                }
                if !msg.starts_with("-------------"){
                    // let message_content = msg.splitn(2, ": ").nth(1).unwrap_or(&msg);
                    // println!("{:?}",message_content);
                    if let Err(err) = insert_msg(&state_clone.db_pool, msg,&channel_clone).await {
                        tracing::error!("Failed to insert message to database {:?}", err);
                    }
                }
            }
        });

    let mut recv_task ={
        let tx = tx.clone();
        let name = username.clone();

        tokio::spawn(async move{
            while let Some(Ok(Message::Text(text))) = receiver.next().await{
                println!("{}",text);
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
