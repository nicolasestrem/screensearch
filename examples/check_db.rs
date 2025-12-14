use sqlx::sqlite::SqlitePoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite://screensearch.db")
        .await?;

    let ids = vec![2531, 1301];
    
    for id in ids {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM frames WHERE id = ?)")
            .bind(id)
            .fetch_one(&pool)
            .await?;
            
        println!("Frame {}: {}", id, if exists { "EXISTS" } else { "MISSING" });
    }
    
    Ok(())
}
