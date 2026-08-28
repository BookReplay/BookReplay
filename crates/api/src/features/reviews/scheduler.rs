use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

const MAX_INTERVAL_DAYS: i32 = 3_650;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewRating {
    Soon,
    Later,
    MuchLater,
    Archive,
}

impl ReviewRating {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Soon => "SOON",
            Self::Later => "LATER",
            Self::MuchLater => "MUCH_LATER",
            Self::Archive => "ARCHIVE",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct ReviewSchedule {
    pub interval_days: Option<i32>,
    pub next_review_at: Option<OffsetDateTime>,
}

pub fn schedule_review(
    now: OffsetDateTime,
    current_interval_days: i32,
    review_count: i32,
    rating: ReviewRating,
) -> ReviewSchedule {
    let review_count = review_count.max(i32::from(current_interval_days > 0));
    let interval_days = match rating {
        ReviewRating::Soon => Some(growing_interval(3, review_count)),
        ReviewRating::Later => Some(later_interval(review_count)),
        ReviewRating::MuchLater => Some(growing_interval(60, review_count)),
        ReviewRating::Archive => None,
    };

    ReviewSchedule {
        interval_days,
        next_review_at: interval_days.map(|days| now + Duration::days(i64::from(days))),
    }
}

fn growing_interval(base_days: i32, review_count: i32) -> i32 {
    base_days
        .saturating_mul(2_i32.saturating_pow(review_count.clamp(0, 30) as u32))
        .min(MAX_INTERVAL_DAYS)
}

fn later_interval(review_count: i32) -> i32 {
    if review_count <= 0 {
        14
    } else {
        growing_interval(30, review_count - 1)
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::{ReviewRating, schedule_review};

    #[test]
    fn later_schedule_uses_growing_intervals() {
        let now = datetime!(2026-08-28 12:00 UTC);
        let intervals = (0..4)
            .map(|count| {
                schedule_review(now, 0, count, ReviewRating::Later)
                    .interval_days
                    .expect("later should have an interval")
            })
            .collect::<Vec<_>>();

        assert_eq!(intervals, vec![14, 30, 60, 120]);
    }

    #[test]
    fn archive_has_no_future_review() {
        let schedule = schedule_review(
            datetime!(2026-08-28 12:00 UTC),
            14,
            1,
            ReviewRating::Archive,
        );

        assert_eq!(
            schedule,
            super::ReviewSchedule {
                interval_days: None,
                next_review_at: None,
            }
        );
    }

    #[test]
    fn first_review_uses_rating_baselines() {
        let now = datetime!(2026-08-28 12:00 UTC);
        let intervals = [
            ReviewRating::Soon,
            ReviewRating::Later,
            ReviewRating::MuchLater,
        ]
        .map(|rating| schedule_review(now, 0, 0, rating).interval_days);

        assert_eq!(intervals, [Some(3), Some(14), Some(60)]);
    }
}
