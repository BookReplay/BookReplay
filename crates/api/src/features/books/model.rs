use bookreplay_openlibrary::BookCandidate;
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

pub async fn all(pool: &PgPool, user_id: i64) -> Result<Vec<BookSummary>, sqlx::Error> {
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

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BookClipping {
    pub id: i64,
    pub metadata: String,
    pub content: String,
}

pub async fn clippings(
    pool: &PgPool,
    user_id: i64,
    book_id: i64,
) -> Result<Vec<BookClipping>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, metadata, content FROM clippings \
         WHERE user_id = $1 AND book_id = $2 AND btrim(content) <> '' ORDER BY id",
    )
    .bind(user_id)
    .bind(book_id)
    .fetch_all(pool)
    .await
}

pub async fn identify(
    pool: &PgPool,
    user_id: i64,
    book_id: i64,
    book: &BookCandidate,
) -> Result<Option<BookSummary>, sqlx::Error> {
    sqlx::query_as(
        "WITH updated AS (\
             UPDATE books SET open_library_key = COALESCE($2, open_library_key), title = $3, authors = CASE WHEN cardinality($4::text[]) > 0 THEN $4 ELSE authors END, cover_url = COALESCE($5, cover_url), \
                 first_publish_year = COALESCE($6, first_publish_year), edition_count = COALESCE($7, edition_count), isbns = CASE WHEN cardinality($8::text[]) > 0 THEN $8 ELSE isbns END, google_books_volume_id = COALESCE($10, google_books_volume_id), \
                 metadata_checked_at = NOW(), retry_count = 0, next_attempt_at = NOW(), \
                 last_error = NULL, updated_at = NOW() \
             WHERE id = $1 AND user_id = $9 RETURNING id, title, authors, cover_url\
         ) \
         SELECT updated.id, updated.title, updated.authors, updated.cover_url, \
                COUNT(clippings.id) AS highlight_count \
         FROM updated LEFT JOIN clippings ON clippings.book_id = updated.id \
             AND clippings.user_id = $9 AND btrim(clippings.content) <> '' \
         GROUP BY updated.id, updated.title, updated.authors, updated.cover_url",
    )
    .bind(book_id)
    .bind(book.open_library_key())
    .bind(book.title.trim())
    .bind(&book.authors)
    .bind(&book.cover_url)
    .bind(book.first_publish_year)
    .bind(book.edition_count)
    .bind(&book.isbns)
    .bind(user_id)
    .bind(book.google_books_volume_id())
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

#[cfg(test)]
mod database_tests {
    use super::*;
    use crate::features::clippings::model::insert;
    use bookreplay_core::Clipping;
    use bookreplay_openlibrary::Provider;

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL pointing to a disposable PostgreSQL server"]
    async fn identification_supports_both_sources_and_duplicate_import_keeps_ids(pool: PgPool) {
        let user_id: i64 = sqlx::query_scalar(
            "INSERT INTO users (name, email, password_hash) VALUES ('Test', 'test@example.org', 'unused') RETURNING id"
        ).fetch_one(&pool).await.unwrap();
        // Advance the sequence so the clipping id cannot equal the book id.
        sqlx::query("SELECT setval('clippings_id_seq', 40)")
            .execute(&pool)
            .await
            .unwrap();
        let imported = vec![Clipping {
            book: "Book (Author)".into(),
            metadata: "Location 1".into(),
            content: "Highlight".into(),
        }];
        assert_eq!(insert(&pool, user_id, &imported).await.unwrap().books, 1);
        let (book_id, clipping_id): (i64, i64) =
            sqlx::query_as("SELECT book_id, id FROM clippings")
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut candidate = BookCandidate {
            provider: Provider::OpenLibrary,
            provider_id: "/works/OL1W".into(),
            title: "Identified".into(),
            authors: vec!["Author".into()],
            cover_url: Some("https://covers.openlibrary.org/b/id/1-L.jpg".into()),
            first_publish_year: Some(1990),
            edition_count: Some(2),
            isbns: vec![],
        };
        assert_eq!(
            identify(&pool, user_id, book_id, &candidate)
                .await
                .unwrap()
                .unwrap()
                .title,
            "Identified"
        );
        candidate.provider = Provider::GoogleBooks;
        candidate.provider_id = "google_1".into();
        candidate.title = "Google title".into();
        candidate.authors.clear();
        candidate.cover_url = None;
        candidate.first_publish_year = None;
        let updated = identify(&pool, user_id, book_id, &candidate)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.authors, ["Author"]);
        assert!(updated.cover_url.is_some());
        let result = insert(&pool, user_id, &imported).await.unwrap();
        assert_eq!((result.books, result.clippings), (0, 0));
        let listed = clippings(&pool, user_id, book_id).await.unwrap();
        assert_eq!(
            listed.iter().map(|row| row.id).collect::<Vec<_>>(),
            [clipping_id]
        );
        assert!(
            clippings(&pool, user_id, clipping_id)
                .await
                .unwrap()
                .is_empty()
        );
        let ids: (i64, i64) = sqlx::query_as("SELECT book_id, id FROM clippings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(ids, (book_id, clipping_id));
        let row: (String, String, String, bool, i32) = sqlx::query_as(
            "SELECT title, open_library_key, google_books_volume_id, metadata_checked_at IS NOT NULL, first_publish_year FROM books"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(
            row,
            (
                "Google title".into(),
                "/works/OL1W".into(),
                "google_1".into(),
                true,
                1990
            )
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL pointing to a disposable PostgreSQL server"]
    async fn each_user_has_a_separate_library(pool: PgPool) {
        let [first, second]: [i64; 2] = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (name, email, password_hash) VALUES ('First', 'first@example.org', 'unused'), ('Second', 'second@example.org', 'unused') RETURNING id"
        ).fetch_all(&pool).await.unwrap().try_into().unwrap();
        let imported = vec![Clipping {
            book: "Book (Author)".into(),
            metadata: "Location 1".into(),
            content: "Highlight".into(),
        }];
        // The same Kindle title is a separate book for each user.
        for user_id in [first, second] {
            let result = insert(&pool, user_id, &imported).await.unwrap();
            assert_eq!((result.books, result.clippings), (1, 1));
        }
        let first_book = all(&pool, first).await.unwrap().remove(0);
        let second_book = all(&pool, second).await.unwrap().remove(0);
        assert_ne!(first_book.id, second_book.id);
        assert_eq!(
            (first_book.highlight_count, second_book.highlight_count),
            (1, 1)
        );

        assert!(
            clippings(&pool, second, first_book.id)
                .await
                .unwrap()
                .is_empty()
        );
        let candidate = BookCandidate {
            provider: Provider::OpenLibrary,
            provider_id: "/works/OL1W".into(),
            title: "Rewritten".into(),
            authors: vec![],
            cover_url: None,
            first_publish_year: None,
            edition_count: None,
            isbns: vec![],
        };
        assert!(
            identify(&pool, second, first_book.id, &candidate)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(all(&pool, first).await.unwrap()[0].title, "Book");

        // The schema itself refuses a highlight filed under another user's book.
        let misfiled = sqlx::query(
            "INSERT INTO clippings (user_id, book_id, metadata, content, import_hash) VALUES ($1, $2, 'm', 'c', 'h')",
        )
        .bind(second)
        .bind(first_book.id)
        .execute(&pool)
        .await;
        assert!(misfiled.is_err());

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(first)
            .execute(&pool)
            .await
            .unwrap();
        let remaining: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM books")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, [second]);
        assert_eq!(
            clippings(&pool, second, second_book.id)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
