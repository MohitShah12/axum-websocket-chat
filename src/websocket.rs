use std::sync::Arc;

use axum::{ extract::{ ws::{WebSocket, Message}}};
use futures::{ StreamExt, SinkExt};
use serde::Deserialize;
use tokio::{sync::broadcast};

use crate::{AppState, queries::{insert_user, get_admin, get_all, insert_msg}, ws_handler::RoomState};




pub async fn websocket(stream:WebSocket,state:Arc<AppState>){
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
            "--------------{} created a new channel: {}",
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
        let msg = format!("--------------{} joined the {} chat created by {}", username, channel,admin[0].name);
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

    let msg = format!("--------------{} left the chat",username);

    
    let _ = tx.send(msg);
    
    let _ = sqlx::query("DELETE FROM chatuser WHERE name = $1")
    .bind(username.clone())
    .execute(&state.db_pool)
    .await;
    let mut rooms = state.rooms.lock().unwrap();

    rooms.get_mut(&channel).unwrap().user_set.remove(&username);
}