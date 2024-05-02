use sqlx::PgPool;

#[derive(Debug)]
#[derive(sqlx::FromRow, Clone)]
pub struct AdminUser{
    pub name:String,
}

#[derive(Debug,sqlx::FromRow)]
pub struct Users{
    pub name:String,
}

pub async fn insert_user(pool:&PgPool, username:&str, channel:&str) -> Result<(),sqlx::Error>{
    sqlx::query("INSERT INTO chatuser (name,channel) VALUES ($1,$2)")
        .bind(username)
        .bind(channel)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_msg(pool:&PgPool,message:String,channel:&String) -> Result<(),sqlx::Error>{
    println!("Inside query:{}",message);
    sqlx::query("INSERT INTO messages (chat, channel) VALUES ($1,$2)")
        .bind(message)
        .bind(channel)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_admin(pool:&PgPool, channel:&String) -> Vec<AdminUser>{
    let user = sqlx::query_as::<_,AdminUser>("SELECT name FROM chatuser WHERE channel = $1 LIMIT 1")
        .bind(channel)
        .fetch_all(pool)
    .await.unwrap();
    
    user
}

pub async fn get_all(pool:&PgPool, channel:&String)-> Vec<Users>{
    let user = sqlx::query_as::<_,Users>("SELECT name FROM chatuser WHERE channel = $1")
        .bind(channel)
        .fetch_all(pool)
    .await.unwrap();
    
    user
}