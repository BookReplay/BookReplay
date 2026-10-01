use serde::Serialize;

#[derive(Debug, PartialEq, Serialize, sqlx::FromRow)]
pub struct Clipping {
    pub book: String,
    pub metadata: String,
    pub content: String,
}
