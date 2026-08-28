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

pub async fn all(pool: &PgPool) -> Result<Vec<BookSummary>, sqlx::Error> {
    sqlx::query_as(
        "SELECT books.id, books.title, books.authors, books.cover_url, \
                COUNT(clippings.id) AS highlight_count \
         FROM books JOIN clippings ON clippings.book_id = books.id \
         WHERE btrim(clippings.content) <> '' \
         GROUP BY books.id ORDER BY books.title",
    )
    .fetch_all(pool)
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
