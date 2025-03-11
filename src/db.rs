// src/db.rs
use crate::config::CONFIG;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbErr, Schema};

pub type DbPool = DatabaseConnection;

pub async fn init_db() -> Result<DbPool, DbErr> {
    let database_url = &CONFIG.database_url;
    println!("Connecting to database at: {}", database_url);

    let db = Database::connect(database_url).await?;

    // Create the schema builder
    let schema = Schema::new(db.get_database_backend());

    // Create tables if they don't exist
    db.execute(
        db.get_database_backend().build(
            schema
                .create_table_from_entity(crate::entities::containers::Entity)
                .if_not_exists(),
        ),
    )
    .await?;
    Ok(db)
}
