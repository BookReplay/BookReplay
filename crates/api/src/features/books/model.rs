use bookreplay_openlibrary::OpenLibraryBook;
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BookSummary {
    pub id: i64,
    pub title: String,
    pub authors: Vec<String>,
    pub cover_url: Option<String>,
    pub highlight_count: i64,
}

pub async fn all(pool: &PgPool, user_id: i16) -> Result<Vec<BookSummary>, sqlx::Error> {
    sqlx::query_as(
        "SELECT books.id, books.title, books.authors, books.cover_url, \
                COUNT(clippings.id) AS highlight_count \
         FROM books JOIN clippings ON clippings.book_id = books.id \
         WHERE clippings.user_id = $1 AND btrim(clippings.content) <> '' \
         GROUP BY books.id ORDER BY books.title",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

pub async fn identify(
    pool: &PgPool,
    user_id: i16,
    book_id: i64,
    book: &OpenLibraryBook,
) -> Result<Option<BookSummary>, sqlx::Error> {
    let cover_url = book
        .cover_id
        .map(|cover_id| format!("https://covers.openlibrary.org/b/id/{cover_id}-L.jpg"));
    sqlx::query_as(
        "WITH updated AS (\
             UPDATE books SET open_library_key = $2, title = $3, authors = $4, cover_url = $5, \
                 first_publish_year = $6, edition_count = $7, isbns = $8, \
                 metadata_checked_at = NOW(), retry_count = 0, next_attempt_at = NOW(), \
                 last_error = NULL, updated_at = NOW() \
             WHERE id = $1 RETURNING id, title, authors, cover_url\
         ) \
         SELECT updated.id, updated.title, updated.authors, updated.cover_url, \
                COUNT(clippings.id) AS highlight_count \
         FROM updated LEFT JOIN clippings ON clippings.book_id = updated.id \
             AND clippings.user_id = $9 AND btrim(clippings.content) <> '' \
         GROUP BY updated.id, updated.title, updated.authors, updated.cover_url",
    )
    .bind(book_id)
    .bind(&book.open_library_key)
    .bind(book.title.trim())
    .bind(&book.authors)
    .bind(cover_url)
    .bind(book.first_publish_year)
    .bind(book.edition_count)
    .bind(&book.isbns)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::BookSummary;

    #[test]
    fn book_summary_serializes_highlight_count() {
        let book = BookSummary {
            id: 7,
            title: "A Book".into(),
            authors: vec!["An Author".into()],
            cover_url: None,
            highlight_count: 3,
        };

        assert_eq!(
            serde_json::to_value(book).expect("book summary should serialize"),
            json!({
                "id": 7,
                "title": "A Book",
                "authors": ["An Author"],
                "cover_url": null,
                "highlight_count": 3
            })
        );
    }
}
