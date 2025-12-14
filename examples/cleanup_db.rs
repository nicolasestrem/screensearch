use sqlx::sqlite::SqlitePoolOptions;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite://screensearch.db")
        .await?;

    println!("Cleaning up orphaned OCR records...");
    
    let result = sqlx::query("DELETE FROM ocr_text WHERE frame_id NOT IN (SELECT id FROM frames)")
        .execute(&pool)
        .await?;
        
    println!("Deleted {} orphaned OCR records.", result.rows_affected());
    
    // Also clean up embeddings if any
    let result_emb = sqlx::query("DELETE FROM embeddings WHERE frame_id NOT IN (SELECT id FROM frames)")
        .execute(&pool)
        .await?;
        
    println!("Deleted {} orphaned embedding records.", result_emb.rows_affected());
    
    Ok(())
}
