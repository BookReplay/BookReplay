use std::collections::HashMap;

use serde::Serialize;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use time::{Date, Duration, OffsetDateTime};

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
    pub next_review_at: Option<OffsetDateTime>,
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
    pub streak: i32,
    pub daily_revision_count: i32,
    pub goal_reached: bool,
}

#[derive(FromRow)]
struct StreakState {
    revision_streak: i32,
    last_revision_date: Option<Date>,
    daily_revision_count: i32,
    daily_revision_date: Option<Date>,
}

struct DailyProgress {
    streak: i32,
    daily_revision_count: i32,
    goal_reached: bool,
}

#[derive(Serialize)]
pub struct StreakResponse {
    pub streak: i32,
    pub daily_revision_count: i32,
    /// Highlights a review session could show now: due ones plus never-reviewed ones.
    pub due_count: i64,
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
        "SELECT review_count, current_interval_days, next_review_at, archived_at \
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

    let progress = record_streak(transaction, user_id, now.date()).await?;

    Ok(ReviewResponse {
        highlight_id,
        rating,
        review_count: state.review_count + 1,
        next_interval_days: schedule.interval_days,
        last_reviewed_at: now,
        next_review_at: schedule.next_review_at,
        archived_at,
        streak: progress.streak,
        daily_revision_count: progress.daily_revision_count,
        goal_reached: progress.goal_reached,
    })
}

pub async fn streak(pool: &PgPool, user_id: i16) -> Result<StreakResponse, sqlx::Error> {
    let state: StreakState = sqlx::query_as(
        "SELECT revision_streak, last_revision_date, daily_revision_count, daily_revision_date \
         FROM owner WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let now = OffsetDateTime::now_utc();
    let due_count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM clippings \
         WHERE user_id = $1 AND archived_at IS NULL AND btrim(content) <> '' \
               AND (next_review_at <= $2 OR (review_count = 0 AND next_review_at IS NULL))",
    )
    .bind(user_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(StreakResponse {
        due_count,
        ..current_progress(&state, now.date())
    })
}

fn current_progress(state: &StreakState, today: Date) -> StreakResponse {
    StreakResponse {
        streak: match state.last_revision_date {
            Some(date) if date == today || date == today - Duration::days(1) => {
                state.revision_streak
            }
            _ => 0,
        },
        daily_revision_count: if state.daily_revision_date == Some(today) {
            state.daily_revision_count
        } else {
            0
        },
        due_count: 0,
    }
}

async fn record_streak(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: i16,
    today: Date,
) -> Result<DailyProgress, sqlx::Error> {
    let state: StreakState = sqlx::query_as(
        "SELECT revision_streak, last_revision_date, daily_revision_count, daily_revision_date \
         FROM owner WHERE id = $1 FOR UPDATE",
    )
    .bind(user_id)
    .fetch_one(&mut **transaction)
    .await?;
    let progress = next_daily_progress(&state, today);
    let last_revision_date = if progress.goal_reached {
        Some(today)
    } else {
        state.last_revision_date
    };

    sqlx::query(
        "UPDATE owner SET revision_streak = $1, last_revision_date = $2, \
         daily_revision_count = $3, daily_revision_date = $4 WHERE id = $5",
    )
    .bind(progress.streak)
    .bind(last_revision_date)
    .bind(progress.daily_revision_count)
    .bind(today)
    .bind(user_id)
    .execute(&mut **transaction)
    .await?;

    Ok(progress)
}

fn next_daily_progress(state: &StreakState, today: Date) -> DailyProgress {
    const DAILY_GOAL: i32 = 5;

    let daily_revision_count = if state.daily_revision_date == Some(today) {
        state.daily_revision_count.saturating_add(1)
    } else {
        1
    };
    let goal_reached = daily_revision_count == DAILY_GOAL;

    DailyProgress {
        streak: if goal_reached {
            next_streak(state.revision_streak, state.last_revision_date, today)
        } else {
            current_progress(state, today).streak
        },
        daily_revision_count,
        goal_reached,
    }
}

fn next_streak(streak: i32, last_revision_date: Option<Date>, today: Date) -> i32 {
    match last_revision_date {
        Some(date) if date == today => streak,
        Some(date) if date == today - Duration::days(1) => streak.saturating_add(1),
        _ => 1,
    }
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

    use time::macros::{date, datetime};

    use super::{SessionHighlight, StreakState, next_daily_progress, next_streak, select_session};

    #[test]
    fn displayed_progress_resets_at_day_boundaries() {
        let state = StreakState {
            revision_streak: 4,
            last_revision_date: Some(date!(2026 - 08 - 27)),
            daily_revision_count: 7,
            daily_revision_date: Some(date!(2026 - 08 - 27)),
        };
        for (today, expected) in [
            (date!(2026 - 08 - 27), (4, 7)),
            (date!(2026 - 08 - 28), (4, 0)),
            (date!(2026 - 08 - 29), (0, 0)),
        ] {
            let progress = super::current_progress(&state, today);
            assert_eq!((progress.streak, progress.daily_revision_count), expected);
        }
    }

    #[test]
    fn streak_increments_only_after_a_consecutive_day() {
        assert_eq!(
            next_streak(4, Some(date!(2026 - 08 - 27)), date!(2026 - 08 - 28)),
            5
        );
    }

    #[test]
    fn streak_does_not_increment_twice_on_the_same_day() {
        assert_eq!(
            next_streak(4, Some(date!(2026 - 08 - 28)), date!(2026 - 08 - 28)),
            4
        );
    }

    #[test]
    fn streak_restarts_after_a_missed_day() {
        assert_eq!(
            next_streak(4, Some(date!(2026 - 08 - 26)), date!(2026 - 08 - 28)),
            1
        );
    }

    #[test]
    fn fifth_revision_advances_the_streak() {
        let progress = next_daily_progress(
            &StreakState {
                revision_streak: 4,
                last_revision_date: Some(date!(2026 - 08 - 27)),
                daily_revision_count: 4,
                daily_revision_date: Some(date!(2026 - 08 - 28)),
            },
            date!(2026 - 08 - 28),
        );

        assert!(progress.goal_reached);
        assert_eq!(progress.streak, 5);
    }

    #[test]
    fn partial_day_does_not_advance_the_streak() {
        let progress = next_daily_progress(
            &StreakState {
                revision_streak: 4,
                last_revision_date: Some(date!(2026 - 08 - 27)),
                daily_revision_count: 3,
                daily_revision_date: Some(date!(2026 - 08 - 28)),
            },
            date!(2026 - 08 - 28),
        );

        assert!(!progress.goal_reached);
        assert_eq!(progress.streak, 4);
    }

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

#[cfg(test)]
mod database_tests {
    use bookreplay_core::Clipping;

    use super::*;
    use crate::features::{clippings::model::insert, reviews::scheduler::schedule_review};

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL pointing to a disposable PostgreSQL server"]
    async fn a_reviewed_highlight_leaves_the_due_count_until_it_is_due_again(pool: PgPool) {
        let user_id: i16 = sqlx::query_scalar(
            "INSERT INTO owner (id, name, email, password_hash) VALUES (1, 'Test', 'test@example.org', 'unused') RETURNING id"
        ).fetch_one(&pool).await.unwrap();
        let clippings = ["First", "Second", ""].map(|content| Clipping {
            book: "Book (Author)".into(),
            metadata: format!("Location {content}"),
            content: content.into(),
        });
        insert(&pool, user_id, &clippings).await.unwrap();
        assert_eq!(streak(&pool, user_id).await.unwrap().due_count, 2);

        let highlight_id: i64 =
            sqlx::query_scalar("SELECT id FROM clippings WHERE content = 'First'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let now = OffsetDateTime::now_utc();
        let mut transaction = pool.begin().await.unwrap();
        let state = find_for_update(&mut transaction, user_id, highlight_id)
            .await
            .unwrap()
            .unwrap();
        assert!(state.next_review_at.is_none());
        let schedule = schedule_review(now, 0, 0, ReviewRating::Soon);
        let response = save_review(
            &mut transaction,
            user_id,
            highlight_id,
            &state,
            ReviewRating::Soon,
            &schedule,
            now,
        )
        .await
        .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(response.daily_revision_count, 1);

        let mut transaction = pool.begin().await.unwrap();
        let state = find_for_update(&mut transaction, user_id, highlight_id)
            .await
            .unwrap()
            .unwrap();
        // The controller answers 409 while this date is in the future.
        assert!(state.next_review_at.is_some_and(|date| date > now));
        drop(transaction);
        assert_eq!(streak(&pool, user_id).await.unwrap().due_count, 1);
    }
}
