use std::collections::HashMap;

use rekindle_core::{Clipping, EnrichedClipping};
use rekindle_kindle::from_kindle_title;
use sqlx::PgPool;

pub async fn all(pool: &PgPool) -> Result<Vec<EnrichedClipping>, sqlx::Error> {
    sqlx::query_as(
        "SELECT books.id, books.kindle_title, books.title, books.authors, \
                books.open_library_key, books.cover_url, books.first_publish_year, \
                books.edition_count, books.isbns, clippings.metadata, clippings.content \
         FROM clippings JOIN books ON books.id = clippings.book_id ORDER BY clippings.id",
    )
    .fetch_all(pool)
    .await
}

pub struct ImportResult {
    pub clippings: usize,
    pub books: usize,
}

pub async fn insert(pool: &PgPool, clippings: &[Clipping]) -> Result<ImportResult, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let mut book_ids = HashMap::new();
    let mut inserted_books = 0;
    let mut inserted_clippings = 0;

    for clipping in clippings {
        let book_id = if let Some(book_id) = book_ids.get(clipping.book.as_str()) {
            *book_id
        } else {
            let (title, authors) = from_kindle_title(&clipping.book);
            inserted_books += sqlx::query(
                "INSERT INTO books (kindle_title, title, authors) VALUES ($1, $2, $3) \
                 ON CONFLICT (kindle_title) DO NOTHING",
            )
            .bind(&clipping.book)
            .bind(title)
            .bind(authors)
            .execute(&mut *transaction)
            .await?
            .rows_affected() as usize;

            let book_id: i64 = sqlx::query_scalar("SELECT id FROM books WHERE kindle_title = $1")
                .bind(&clipping.book)
                .fetch_one(&mut *transaction)
                .await?;
            book_ids.insert(clipping.book.as_str(), book_id);
            book_id
        };

        inserted_clippings += sqlx::query(
            "INSERT INTO clippings (book_id, metadata, content) VALUES ($1, $2, $3) \
             ON CONFLICT (book_id, metadata, content) DO NOTHING",
        )
        .bind(book_id)
        .bind(&clipping.metadata)
        .bind(&clipping.content)
        .execute(&mut *transaction)
        .await?
        .rows_affected() as usize;
    }

    transaction.commit().await?;
    Ok(ImportResult {
        clippings: inserted_clippings,
        books: inserted_books,
    })
}
