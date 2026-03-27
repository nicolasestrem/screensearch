//! Database manager implementation
//!
//! Provides the main DatabaseManager struct with connection pooling,
//! performance optimizations, and migration handling.

use crate::{DatabaseConfig, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::str::FromStr;
use std::time::Duration;

/// Main database manager with connection pooling and query execution
pub struct DatabaseManager {
    pool: Pool<Sqlite>,
    config: DatabaseConfig,
}

impl DatabaseManager {
    /// Create a new database manager with default configuration and run migrations
    pub async fn new(path: impl Into<String>) -> Result<Self> {
        let config = DatabaseConfig::new(path);
        Self::with_config(config).await
    }

    /// Create database manager with custom configuration
    ///
    /// # Arguments
    /// * `config` - DatabaseConfig with pool settings, path, and performance options
    ///
    /// # Returns
    /// Result containing initialized DatabaseManager or error
    pub async fn with_config(config: DatabaseConfig) -> Result<Self> {
        tracing::info!(
            "Initializing database at: {} (max_connections: {}, min_connections: {})",
            config.path,
            config.max_connections,
            config.min_connections
        );

        // Create SQLite connection options
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", config.path))
            .map_err(|e| {
                crate::DatabaseError::InitializationError(format!(
                    "Failed to parse database URL: {}",
                    e
                ))
            })?
            .create_if_missing(true);

        // Create connection pool with configured limits
        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
            .connect_with(options)
            .await
            .map_err(|e| {
                crate::DatabaseError::InitializationError(format!(
                    "Failed to create connection pool: {}",
                    e
                ))
            })?;

        let manager = Self { pool, config };

        // Apply performance optimizations
        manager.apply_pragmas().await?;

        tracing::info!("Database optimizations applied successfully");

        // Run database migrations to ensure schema is current
        manager.run_migrations().await?;

        tracing::info!("Database initialization complete");

        Ok(manager)
    }

    /// Apply SQLite performance optimizations
    ///
    /// Configures:
    /// - WAL mode for concurrent read/write access
    /// - Cache size for memory optimization
    /// - Temporary tables in memory
    /// - Synchronous mode for performance
    async fn apply_pragmas(&self) -> Result<()> {
        tracing::debug!("Applying SQLite pragmas");

        if self.config.enable_wal {
            sqlx::query("PRAGMA journal_mode = WAL")
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to enable WAL mode: {}", e);
                    crate::DatabaseError::InitializationError(format!(
                        "Failed to enable WAL mode: {}",
                        e
                    ))
                })?;
            tracing::debug!("WAL mode enabled");
        }

        sqlx::query(&format!(
            "PRAGMA cache_size = {}",
            self.config.cache_size_kb
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to set cache size: {}", e);
            crate::DatabaseError::InitializationError(format!("Failed to set cache size: {}", e))
        })?;

        sqlx::query("PRAGMA temp_store = MEMORY")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to set temp_store: {}", e);
                crate::DatabaseError::InitializationError(format!(
                    "Failed to set temp_store: {}",
                    e
                ))
            })?;

        sqlx::query("PRAGMA synchronous = NORMAL")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to set synchronous mode: {}", e);
                crate::DatabaseError::InitializationError(format!(
                    "Failed to set synchronous mode: {}",
                    e
                ))
            })?;

        tracing::debug!("All pragmas applied successfully");
        Ok(())
    }

    /// Run database migrations to ensure schema is current
    async fn run_migrations(&self) -> Result<()> {
        tracing::info!("Running database migrations");
        crate::migrations::run_migrations(&self.pool).await?;
        Ok(())
    }

    /// Get a reference to the connection pool for advanced operations
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    /// Get connection pool statistics (num_idle)
    pub fn pool_stats(&self) -> usize {
        self.pool.num_idle()
    }

    /// Close the database connection pool and release resources
    pub async fn close(self) {
        tracing::info!("Closing database connection pool");
        self.pool.close().await;
        tracing::info!("Database closed");
    }
}
