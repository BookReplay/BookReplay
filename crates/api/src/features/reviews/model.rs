use std::collections::HashMap;

use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use time::OffsetDateTime;

use super::scheduler::{ReviewRating, ReviewSchedule};

#[derive(Clone, Debug, FromRow, Serialize)]
pub struct SessionHighlight {
    pub highlight_id: i64,
    pub book_title: String,
    pub authors: Vec<String>,
    pub highlight_text: String,
    #[serde(skip)]
    book_id: i64,
    #[serde(skip)]
    next_review_at: Option<OffsetDateTime>,
    #[serde(skip)]
    review_count: i32,
    #[serde(skip)]
    archived_at: Option<OffsetDateTime>,
}

#[derive(FromRow)]
pub struct ReviewState {
    pub review_count: i32,
    pub current_interval_days: i32,
    pub archived_at: Option<OffsetDateTime>,
}

#[derive(Serialize)]
pub struct ReviewResponse {
    pub highlight_id: i64,
    pub rating: ReviewRating,
    pub review_count: i32,
    pub next_interval_days: Option<i32>,
    #[serde(with = "time::serde::rfc3339")]
    pub last_reviewed_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub next_review_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

pub async fn session(
    pool: &PgPool,
    user_id: i16,
    limit: usize,
    now: OffsetDateTime,
) -> Result<Vec<SessionHighlight>, sqlx::Error> {
    let candidate_limit = i64::try_from(limit.saturating_mul(10)).unwrap_or(i64::MAX);
    let candidates = sqlx::query_as(
        "(SELECT c.id AS highlight_id, b.id AS book_id, b.title AS book_title, b.authors, \
                 c.content AS highlight_text, c.next_review_at, c.review_count, c.archived_at \
          FROM clippings c JOIN books b ON b.id = c.book_id \
          WHERE c.user_id = $1 AND c.archived_at IS NULL AND c.next_review_at <= $2 AND btrim(c.content) <> '' \
          ORDER BY c.next_review_at, c.id LIMIT $3) \
         UNION ALL \
         (SELECT c.id AS highlight_id, b.id AS book_id, b.title AS book_title, b.authors, \
                 c.content AS highlight_text, c.next_review_at, c.review_count, c.archived_at \
          FROM clippings c JOIN books b ON b.id = c.book_id \
          WHERE c.user_id = $1 AND c.archived_at IS NULL AND c.review_count = 0 AND c.next_review_at IS NULL \
                AND btrim(c.content) <> '' \
          ORDER BY c.id LIMIT $3)",
    )
    .bind(user_id)
    .bind(now)
    .bind(candidate_limit)
    .fetch_all(pool)
    .await?;

    Ok(select_session(candidates, limit, now))
}

pub async fn find_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: i16,
    highlight_id: i64,
) -> Result<Option<ReviewState>, sqlx::Error> {
    sqlx::query_as(
        "SELECT review_count, current_interval_days, archived_at \
         FROM clippings WHERE id = $1 AND user_id = $2 AND btrim(content) <> '' FOR UPDATE",
    )
    .bind(highlight_id)
    .bind(user_id)
    .fetch_optional(&mut **transaction)
    .await
}

pub async fn save_review(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: i16,
    highlight_id: i64,
    state: &ReviewState,
    rating: ReviewRating,
    schedule: &ReviewSchedule,
    now: OffsetDateTime,
) -> Result<ReviewResponse, sqlx::Error> {
    sqlx::query(
        "INSERT INTO highlight_reviews \
             (highlight_id, reviewed_at, rating, previous_interval_days, next_interval_days) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(highlight_id)
    .bind(now)
    .bind(rating.as_str())
    .bind(state.current_interval_days)
    .bind(schedule.interval_days)
    .execute(&mut **transaction)
    .await?;

    let archived_at = (rating == ReviewRating::Archive).then_some(now);
    sqlx::query(
        "UPDATE clippings SET \
             first_seen_at = COALESCE(first_seen_at, $2), last_reviewed_at = $2, \
             next_review_at = $3, review_count = review_count + 1, \
             current_interval_days = COALESCE($4, current_interval_days), archived_at = $5 \
         WHERE id = $1 AND user_id = $6",
    )
    .bind(highlight_id)
    .bind(now)
    .bind(schedule.next_review_at)
    .bind(schedule.interval_days)
    .bind(archived_at)
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;

    Ok(ReviewResponse {
        highlight_id,
        rating,
        review_count: state.review_count + 1,
        next_interval_days: schedule.interval_days,
        last_reviewed_at: now,
        next_review_at: schedule.next_review_at,
        archived_at,
    })
}

fn select_session(
    candidates: Vec<SessionHighlight>,
    limit: usize,
    now: OffsetDateTime,
) -> Vec<SessionHighlight> {
    let (mut due, mut unseen): (Vec<_>, Vec<_>) = candidates
        .into_iter()
        .filter(|highlight| highlight.archived_at.is_none())
        .filter(|highlight| {
            highlight.next_review_at.is_some_and(|date| date <= now)
                || (highlight.review_count == 0 && highlight.next_review_at.is_none())
        })
        .partition(|highlight| highlight.next_review_at.is_some());

    due.sort_by_key(|highlight| (highlight.next_review_at, highlight.highlight_id));
    unseen.sort_by_key(|highlight| highlight.highlight_id);

    let mut selected = Vec::with_capacity(limit);
    append_balanced(&mut selected, due, limit);
    append_balanced(&mut selected, unseen, limit);

    let day = now.unix_timestamp().div_euclid(86_400) as u64;
    selected.sort_by_key(|highlight| shuffle_key(highlight.highlight_id as u64, day));
    selected
}

fn append_balanced(
    selected: &mut Vec<SessionHighlight>,
    candidates: Vec<SessionHighlight>,
    limit: usize,
) {
    let mut counts = selected
        .iter()
        .fold(HashMap::new(), |mut counts, highlight| {
            *counts.entry(highlight.book_id).or_insert(0) += 1;
            counts
        });
    let mut deferred = Vec::new();

    for highlight in candidates {
        if selected.len() == limit {
            return;
        }
        let count = counts.entry(highlight.book_id).or_insert(0);
        if *count < 2 {
            *count += 1;
            selected.push(highlight);
        } else {
            deferred.push(highlight);
        }
    }

    let remaining = limit.saturating_sub(selected.len());
    selected.extend(deferred.into_iter().take(remaining));
}

fn shuffle_key(highlight_id: u64, day: u64) -> u64 {
    let mut value = highlight_id ^ day;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value.wrapping_mul(0x94d0_49bb_1331_11eb)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use time::macros::datetime;

    use super::{SessionHighlight, select_session};

    fn highlight(
        id: i64,
        book_id: i64,
        next_review_at: Option<time::OffsetDateTime>,
        archived_at: Option<time::OffsetDateTime>,
    ) -> SessionHighlight {
        SessionHighlight {
            highlight_id: id,
            book_id,
            book_title: format!("Book {book_id}"),
            authors: Vec::new(),
            highlight_text: format!("Highlight {id}"),
            next_review_at,
            review_count: i32::from(next_review_at.is_some()),
            archived_at,
        }
    }

    #[test]
    fn archived_highlights_are_never_selected() {
        let now = datetime!(2026-08-28 12:00 UTC);
        let session = select_session(
            vec![
                highlight(1, 1, Some(now), Some(now)),
                highlight(2, 2, Some(now), None),
            ],
            10,
            now,
        );

        assert_eq!(
            session
                .iter()
                .map(|item| item.highlight_id)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn session_fills_due_cards_with_never_reviewed_cards() {
        let now = datetime!(2026-08-28 12:00 UTC);
        let session = select_session(
            vec![
                highlight(1, 1, Some(now), None),
                highlight(2, 2, Some(now), None),
                highlight(3, 3, None, None),
                highlight(4, 4, None, None),
                highlight(5, 5, None, None),
            ],
            4,
            now,
        );
        let selected_ids = session
            .iter()
            .map(|highlight| highlight.highlight_id)
            .collect::<HashSet<_>>();

        assert_eq!(selected_ids, HashSet::from([1, 2, 3, 4]));
    }

    #[test]
    fn session_prefers_book_variety_when_available() {
        let now = datetime!(2026-08-28 12:00 UTC);
        let session = select_session(
            vec![
                highlight(1, 1, Some(now), None),
                highlight(2, 1, Some(now), None),
                highlight(3, 1, Some(now), None),
                highlight(4, 2, Some(now), None),
            ],
            3,
            now,
        );
        let selected_ids = session
            .iter()
            .map(|highlight| highlight.highlight_id)
            .collect::<HashSet<_>>();

        assert_eq!(selected_ids, HashSet::from([1, 2, 4]));
    }
}
