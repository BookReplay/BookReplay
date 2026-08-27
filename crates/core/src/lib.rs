use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Book {
    pub id: i64,
    pub kindle_title: String,
    pub title: String,
    pub authors: Vec<String>,
    pub open_library_key: Option<String>,
    pub cover_url: Option<String>,
    pub first_publish_year: Option<i32>,
    pub edition_count: Option<i32>,
    pub isbns: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize, sqlx::FromRow)]
pub struct Clipping {
    pub book: String,
    pub metadata: String,
    pub content: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EnrichedClipping {
    #[sqlx(flatten)]
    pub book: Book,
    pub metadata: String,
    pub content: String,
}
