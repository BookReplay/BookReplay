use std::collections::HashMap;

use bookreplay_core::Clipping;
use bookreplay_kindle::from_kindle_title;
use sqlx::PgPool;

pub async fn update_content(
    pool: &PgPool,
    user_id: i64,
    clipping_id: i64,
    content: &str,
) -> Result<Option<String>, sqlx::Error> {
    // import_hash keeps the imported text's hash so a later import still matches this row.
    sqlx::query_scalar(
        "UPDATE clippings SET content = $1 WHERE id = $2 AND user_id = $3 RETURNING content",
    )
    .bind(content)
    .bind(clipping_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

pub struct ImportResult {
    pub clippings: usize,
    pub books: usize,
}

pub async fn insert(
    pool: &PgPool,
    user_id: i64,
    clippings: &[Clipping],
) -> Result<ImportResult, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let mut book_ids = HashMap::new();
    let mut inserted_books = 0;

    for clipping in clippings {
        if book_ids.contains_key(clipping.book.as_str()) {
            continue;
        }
        let (title, authors) = from_kindle_title(&clipping.book);
        inserted_books += sqlx::query(
            "INSERT INTO books (user_id, kindle_title, title, authors) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (user_id, kindle_title) DO NOTHING",
        )
        .bind(user_id)
        .bind(&clipping.book)
        .bind(title)
        .bind(authors)
        .execute(&mut *transaction)
        .await?
        .rows_affected() as usize;

        let book_id: i64 =
            sqlx::query_scalar("SELECT id FROM books WHERE user_id = $1 AND kindle_title = $2")
                .bind(user_id)
                .bind(&clipping.book)
                .fetch_one(&mut *transaction)
                .await?;
        book_ids.insert(clipping.book.as_str(), book_id);
    }

    let clipping_book_ids: Vec<i64> = clippings
        .iter()
        .map(|clipping| book_ids[clipping.book.as_str()])
        .collect();
    let metadata: Vec<&str> = clippings
        .iter()
        .map(|clipping| clipping.metadata.as_str())
        .collect();
    let contents: Vec<&str> = clippings
        .iter()
        .map(|clipping| clipping.content.as_str())
        .collect();
    let inserted_clippings = sqlx::query(
        "INSERT INTO clippings (user_id, book_id, metadata, content, import_hash) \
         SELECT $1, book_id, metadata, content, md5(content) \
         FROM UNNEST($2::bigint[], $3::text[], $4::text[]) AS imported (book_id, metadata, content) \
         ON CONFLICT (user_id, book_id, metadata, import_hash) DO NOTHING",
    )
    .bind(user_id)
    .bind(clipping_book_ids)
    .bind(metadata)
    .bind(contents)
    .execute(&mut *transaction)
    .await?
    .rows_affected() as usize;

    transaction.commit().await?;
    Ok(ImportResult {
        clippings: inserted_clippings,
        books: inserted_books,
    })
}

#[cfg(test)]
mod database_tests {
    use super::*;

    fn clipping(metadata: &str, content: &str) -> Clipping {
        Clipping {
            book: "Book (Author)".into(),
            metadata: metadata.into(),
            content: content.into(),
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL pointing to a disposable PostgreSQL server"]
    async fn reimport_after_an_edit_does_not_duplicate_the_highlight(pool: PgPool) {
        let user_id: i64 = sqlx::query_scalar(
            "INSERT INTO users (name, email, password_hash) VALUES ('Test', 'test@example.org', 'unused') RETURNING id"
        ).fetch_one(&pool).await.unwrap();
        // Longer than a btree index row can hold when stored as text.
        let long: String = (0..4_000).map(|n| format!("{n:x}")).collect();
        let clippings = vec![
            clipping("Location 1", "Original"),
            clipping("Location 1", "Original"),
            clipping("Location 2", &long),
        ];

        let result = insert(&pool, user_id, &clippings).await.unwrap();
        assert_eq!((result.books, result.clippings), (1, 2));

        let id: i64 = sqlx::query_scalar("SELECT id FROM clippings WHERE content = 'Original'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            update_content(&pool, user_id, id, "Edited")
                .await
                .unwrap()
                .as_deref(),
            Some("Edited")
        );

        let result = insert(&pool, user_id, &clippings).await.unwrap();
        assert_eq!((result.books, result.clippings), (0, 0));
        let stored: Vec<String> =
            sqlx::query_scalar("SELECT content FROM clippings WHERE metadata = 'Location 1'")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(stored, ["Edited"]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL pointing to a disposable PostgreSQL server"]
    async fn a_user_cannot_edit_another_users_highlight(pool: PgPool) {
        let [first, second]: [i64; 2] = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (name, email, password_hash) VALUES ('First', 'first@example.org', 'unused'), ('Second', 'second@example.org', 'unused') RETURNING id"
        ).fetch_all(&pool).await.unwrap().try_into().unwrap();
        insert(&pool, first, &[clipping("Location 1", "Original")])
            .await
            .unwrap();
        let id: i64 = sqlx::query_scalar("SELECT id FROM clippings")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(
            update_content(&pool, second, id, "Edited").await.unwrap(),
            None
        );
        let stored: String = sqlx::query_scalar("SELECT content FROM clippings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, "Original");
    }
}
