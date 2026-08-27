use std::time::{Duration, Instant, SystemTime};

use rekindle_kindle::from_kindle_title;
use reqwest::{
    Client, StatusCode,
    header::{HeaderValue, RETRY_AFTER},
};
use serde::Deserialize;
use sqlx::{Connection, PgConnection};
use tracing::{debug, error, info, warn};

const SEARCH_URL: &str = "https://openlibrary.org/search.json";
const SEARCH_FIELDS: &str = "key,title,author_name,cover_i,first_publish_year,edition_count,isbn";
const ENRICHMENT_LOCK_ID: i64 = 7_312_792_477_285_282_149;
const IDLE_DELAY: Duration = Duration::from_secs(5);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);
const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(30);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60 * 60);

#[derive(sqlx::FromRow)]
struct PendingBook {
    id: i64,
    kindle_title: String,
    retry_count: i32,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    docs: Vec<SearchDocument>,
}

#[derive(Debug, Deserialize)]
struct SearchDocument {
    key: String,
    title: String,
    #[serde(default)]
    author_name: Vec<String>,
    cover_i: Option<i64>,
    first_publish_year: Option<i32>,
    edition_count: Option<i32>,
    #[serde(default)]
    isbn: Vec<String>,
}

struct UpstreamFailure {
    message: String,
    retry_after: Option<Duration>,
}

enum SearchOutcome {
    Match(SearchDocument),
    NoMatch,
    Transient(UpstreamFailure),
    Terminal(String),
}

enum WorkerStep {
    Idle,
    Processed,
    Pause(Duration),
}

#[derive(Default)]
struct RequestPacer {
    last_request_at: Option<Instant>,
}

impl RequestPacer {
    async fn wait(&mut self) {
        if let Some(last_request_at) = self.last_request_at {
            tokio::time::sleep_until((last_request_at + MIN_REQUEST_INTERVAL).into()).await;
        }
        self.last_request_at = Some(Instant::now());
    }
}

fn shortened_title(title: &str) -> Option<&str> {
    let (prefix, _) = title.split_once(':')?;
    let prefix = prefix.trim();

    (!prefix.is_empty() && prefix != title).then_some(prefix)
}

pub fn spawn_enrichment_worker(database_url: String, client: Client) {
    tokio::spawn(async move {
        run_enrichment_worker(&database_url, &client).await;
    });
}

async fn run_enrichment_worker(database_url: &str, client: &Client) {
    loop {
        let mut connection = match PgConnection::connect(database_url).await {
            Ok(connection) => connection,
            Err(error) => {
                error!(error = %error, "book enrichment worker failed to connect to database");
                tokio::time::sleep(IDLE_DELAY).await;
                continue;
            }
        };

        info!("book enrichment worker is waiting for leadership");
        loop {
            match try_acquire_leadership(&mut connection).await {
                Ok(true) => {
                    info!("book enrichment worker acquired leadership");
                    if let Err(error) = run_as_leader(&mut connection, client).await {
                        error!(error = %error, "book enrichment worker lost its database session");
                    }
                    break;
                }
                Ok(false) => tokio::time::sleep(IDLE_DELAY).await,
                Err(error) => {
                    error!(error = %error, "book enrichment worker leadership check failed");
                    break;
                }
            }
        }

        tokio::time::sleep(IDLE_DELAY).await;
    }
}

async fn try_acquire_leadership(connection: &mut PgConnection) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(ENRICHMENT_LOCK_ID)
        .fetch_one(connection)
        .await
}

async fn run_as_leader(connection: &mut PgConnection, client: &Client) -> Result<(), sqlx::Error> {
    let mut pacer = RequestPacer::default();

    loop {
        match process_next_book(connection, client, &mut pacer).await? {
            WorkerStep::Idle => {
                debug!(
                    sleep_seconds = IDLE_DELAY.as_secs(),
                    "book enrichment backlog is idle"
                );
                tokio::time::sleep(IDLE_DELAY).await;
            }
            WorkerStep::Processed => {}
            WorkerStep::Pause(delay) => {
                warn!(
                    pause_seconds = delay.as_secs(),
                    "book enrichment worker pausing after transient upstream failure"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

async fn process_next_book(
    connection: &mut PgConnection,
    client: &Client,
    pacer: &mut RequestPacer,
) -> Result<WorkerStep, sqlx::Error> {
    let book = sqlx::query_as::<_, PendingBook>(
        "SELECT id, kindle_title, retry_count FROM books \
         WHERE metadata_checked_at IS NULL AND next_attempt_at <= NOW() \
         ORDER BY next_attempt_at, id LIMIT 1",
    )
    .fetch_optional(&mut *connection)
    .await?;
    let Some(book) = book else {
        return Ok(WorkerStep::Idle);
    };

    info!(
        book_id = book.id,
        kindle_title = %book.kindle_title,
        retry_count = book.retry_count,
        "processing book enrichment backlog item"
    );

    let (title, _) = from_kindle_title(&book.kindle_title);
    let outcome = search_with_fallback(client, SEARCH_URL, title, pacer).await;

    match outcome {
        SearchOutcome::Match(document) => {
            save_metadata(connection, &book, &document).await?;
            Ok(WorkerStep::Processed)
        }
        SearchOutcome::NoMatch => {
            mark_checked(connection, &book, None).await?;
            warn!(
                book_id = book.id,
                kindle_title = %book.kindle_title,
                "book enrichment completed without an Open Library match"
            );
            Ok(WorkerStep::Processed)
        }
        SearchOutcome::Transient(failure) => {
            let delay = retry_delay(book.retry_count, failure.retry_after);
            schedule_retry(connection, &book, &failure.message, delay).await?;
            Ok(WorkerStep::Pause(delay))
        }
        SearchOutcome::Terminal(message) => {
            mark_checked(connection, &book, Some(&message)).await?;
            error!(
                book_id = book.id,
                kindle_title = %book.kindle_title,
                error = %message,
                "book enrichment failed permanently"
            );
            Ok(WorkerStep::Processed)
        }
    }
}

async fn search_with_fallback(
    client: &Client,
    search_url: &str,
    title: &str,
    pacer: &mut RequestPacer,
) -> SearchOutcome {
    let mut outcome = search(client, search_url, title, pacer).await;
    if matches!(outcome, SearchOutcome::NoMatch)
        && let Some(fallback_title) = shortened_title(title)
    {
        warn!(
            query_title = title,
            fallback_title, "Open Library returned no match; trying shortened title"
        );
        outcome = search(client, search_url, fallback_title, pacer).await;
    }
    outcome
}

async fn search(
    client: &Client,
    search_url: &str,
    title: &str,
    pacer: &mut RequestPacer,
) -> SearchOutcome {
    pacer.wait().await;
    debug!(query_title = title, "requesting Open Library metadata");
    let started_at = Instant::now();
    let response = match client
        .get(search_url)
        .query(&[("title", title), ("limit", "1"), ("fields", SEARCH_FIELDS)])
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return SearchOutcome::Transient(UpstreamFailure {
                message: format!("Open Library request failed: {error}"),
                retry_after: None,
            });
        }
    };

    let status = response.status();
    debug!(
        status = %status,
        elapsed_ms = started_at.elapsed().as_millis(),
        "Open Library responded"
    );
    if is_transient_status(status) {
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| parse_retry_after(value, SystemTime::now()));
        return SearchOutcome::Transient(UpstreamFailure {
            message: format!("Open Library returned HTTP {status}"),
            retry_after,
        });
    }
    if status.is_client_error() {
        return SearchOutcome::Terminal(format!("Open Library returned HTTP {status}"));
    }
    if !status.is_success() {
        return SearchOutcome::Terminal(format!("Open Library returned HTTP {status}"));
    }

    match response.json::<SearchResponse>().await {
        Ok(response) => response
            .docs
            .into_iter()
            .next()
            .map_or(SearchOutcome::NoMatch, SearchOutcome::Match),
        Err(error) => SearchOutcome::Transient(UpstreamFailure {
            message: format!("Open Library response was malformed: {error}"),
            retry_after: None,
        }),
    }
}

fn is_transient_status(status: StatusCode) -> bool {
    status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn parse_retry_after(value: &HeaderValue, now: SystemTime) -> Option<Duration> {
    let value = value.to_str().ok()?;
    value
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()?
                .duration_since(now)
                .ok()
        })
}

fn retry_delay(retry_count: i32, retry_after: Option<Duration>) -> Duration {
    let exponent = retry_count.clamp(0, 7) as u32;
    let backoff = INITIAL_RETRY_DELAY
        .saturating_mul(2_u32.saturating_pow(exponent))
        .min(MAX_RETRY_DELAY);

    retry_after.map_or(backoff, |delay| delay.max(backoff))
}

async fn save_metadata(
    connection: &mut PgConnection,
    book: &PendingBook,
    document: &SearchDocument,
) -> Result<(), sqlx::Error> {
    let cover_url = document
        .cover_i
        .map(|cover_i| format!("https://covers.openlibrary.org/b/id/{cover_i}-L.jpg"));
    sqlx::query(
        "UPDATE books SET title = $2, authors = $3, open_library_key = $4, \
         cover_url = $5, first_publish_year = $6, edition_count = $7, isbns = $8, \
         metadata_checked_at = NOW(), last_error = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(book.id)
    .bind(&document.title)
    .bind(&document.author_name)
    .bind(&document.key)
    .bind(&cover_url)
    .bind(document.first_publish_year)
    .bind(document.edition_count)
    .bind(&document.isbn)
    .execute(connection)
    .await
    .inspect_err(|error| {
        error!(book_id = book.id, error = %error, "failed to save enriched book metadata");
    })?;

    info!(
        book_id = book.id,
        kindle_title = %book.kindle_title,
        open_library_key = %document.key,
        "book enrichment completed"
    );
    Ok(())
}

async fn mark_checked(
    connection: &mut PgConnection,
    book: &PendingBook,
    last_error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE books SET metadata_checked_at = NOW(), last_error = $2, updated_at = NOW() \
         WHERE id = $1",
    )
    .bind(book.id)
    .bind(last_error)
    .execute(connection)
    .await?;
    Ok(())
}

async fn schedule_retry(
    connection: &mut PgConnection,
    book: &PendingBook,
    message: &str,
    delay: Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE books SET retry_count = retry_count + 1, \
         next_attempt_at = NOW() + make_interval(secs => $2::double precision), \
         last_error = $3, updated_at = NOW() WHERE id = $1",
    )
    .bind(book.id)
    .bind(delay.as_secs_f64())
    .bind(message)
    .execute(connection)
    .await?;

    warn!(
        book_id = book.id,
        kindle_title = %book.kindle_title,
        retry_count = book.retry_count + 1,
        retry_in_seconds = delay.as_secs_f64(),
        error = %message,
        "book enrichment scheduled for retry"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
        time::{Duration, Instant, SystemTime},
    };

    use axum::{Router, http::Uri};
    use reqwest::{Client, StatusCode, header::HeaderValue};

    use super::{
        MAX_RETRY_DELAY, RequestPacer, SEARCH_FIELDS, SearchOutcome, from_kindle_title,
        is_transient_status, parse_retry_after, retry_delay, search, search_with_fallback,
        shortened_title,
    };

    async fn mock_open_library(
        bodies: Vec<&'static str>,
    ) -> (String, Arc<Mutex<Vec<Uri>>>, tokio::task::JoinHandle<()>) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured_requests = Arc::clone(&requests);
        let bodies = Arc::new(Mutex::new(VecDeque::from(bodies)));
        let app = Router::new().fallback(move |uri: Uri| async move {
            captured_requests
                .lock()
                .expect("request capture lock should not be poisoned")
                .push(uri);
            bodies
                .lock()
                .expect("response queue lock should not be poisoned")
                .pop_front()
                .expect("mock response should be available")
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock server should bind");
        let address = listener
            .local_addr()
            .expect("listener should have an address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("mock server should run");
        });

        (format!("http://{address}/search.json"), requests, server)
    }

    #[test]
    fn reported_kindle_title_has_expected_fallback() {
        let (title, _) = from_kindle_title(
            "How to Talk to Anyone: 92 Little Tricks for Big Success in Relationships (Driver, Janine)",
        );

        assert_eq!(shortened_title(title), Some("How to Talk to Anyone"));
    }

    #[test]
    fn shortened_title_returns_none_without_colon() {
        assert_eq!(shortened_title("How to Talk to Anyone"), None);
    }

    #[test]
    fn shortened_title_trims_whitespace_around_colon() {
        assert_eq!(
            shortened_title("  How to Talk to Anyone  : 92 Little Tricks"),
            Some("How to Talk to Anyone")
        );
    }

    #[test]
    fn shortened_title_returns_none_for_empty_prefix() {
        assert_eq!(shortened_title(" : A Subtitle"), None);
    }

    #[test]
    fn retry_delay_grows_exponentially_and_caps_at_one_hour() {
        let delays: Vec<_> = (0..10).map(|count| retry_delay(count, None)).collect();

        assert_eq!(
            delays,
            vec![
                Duration::from_secs(30),
                Duration::from_secs(60),
                Duration::from_secs(120),
                Duration::from_secs(240),
                Duration::from_secs(480),
                Duration::from_secs(960),
                Duration::from_secs(1920),
                MAX_RETRY_DELAY,
                MAX_RETRY_DELAY,
                MAX_RETRY_DELAY,
            ]
        );
    }

    #[test]
    fn retry_after_can_extend_the_backoff() {
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(90))),
            Duration::from_secs(90)
        );
    }

    #[test]
    fn retry_after_accepts_seconds_and_http_dates() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let date = httpdate::fmt_http_date(now + Duration::from_secs(90));

        assert_eq!(
            parse_retry_after(&HeaderValue::from_static("45"), now),
            Some(Duration::from_secs(45))
        );
        assert_eq!(
            parse_retry_after(&HeaderValue::from_str(&date).expect("valid header"), now),
            Some(Duration::from_secs(90))
        );
    }

    #[test]
    fn retry_classification_matches_upstream_policy() {
        assert!(is_transient_status(StatusCode::REQUEST_TIMEOUT));
        assert!(is_transient_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_transient_status(StatusCode::BAD_GATEWAY));
        assert!(!is_transient_status(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn search_limits_results_and_requests_only_stored_fields() {
        let (url, requests, server) =
            mock_open_library(vec![r#"{"docs":[{"key":"/works/OL1W","title":"A Book"}]}"#]).await;
        let outcome = search(
            &Client::new(),
            &url,
            "A Book: A Subtitle",
            &mut RequestPacer::default(),
        )
        .await;
        server.abort();

        assert!(matches!(outcome, SearchOutcome::Match(_)));
        let requests = requests
            .lock()
            .expect("request capture lock should not be poisoned");
        let query = requests[0].query().expect("search should have a query");
        assert!(query.contains("limit=1"));
        assert!(query.contains(&format!("fields={}", SEARCH_FIELDS.replace(',', "%2C"))));
    }

    #[tokio::test]
    async fn request_pacer_keeps_requests_at_least_one_second_apart() {
        let (url, _, server) = mock_open_library(vec![r#"{"docs":[]}"#, r#"{"docs":[]}"#]).await;
        let mut pacer = RequestPacer::default();

        let started_at = Instant::now();
        let _ = search(&Client::new(), &url, "First", &mut pacer).await;
        let _ = search(&Client::new(), &url, "Second", &mut pacer).await;
        server.abort();

        assert!(started_at.elapsed() >= Duration::from_secs(1));
    }

    #[tokio::test]
    async fn malformed_response_is_transient() {
        let (url, _, server) = mock_open_library(vec!["not JSON"]).await;

        let outcome = search(&Client::new(), &url, "A Book", &mut RequestPacer::default()).await;
        server.abort();

        assert!(matches!(outcome, SearchOutcome::Transient(_)));
    }

    #[tokio::test]
    async fn no_match_uses_shortened_title_once() {
        let (url, requests, server) = mock_open_library(vec![
            r#"{"docs":[]}"#,
            r#"{"docs":[{"key":"/works/OL1W","title":"A Book"}]}"#,
        ])
        .await;

        let outcome = search_with_fallback(
            &Client::new(),
            &url,
            "A Book: A Subtitle",
            &mut RequestPacer::default(),
        )
        .await;
        server.abort();

        assert!(matches!(outcome, SearchOutcome::Match(_)));
        let requests = requests
            .lock()
            .expect("request capture lock should not be poisoned");
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1]
                .query()
                .expect("fallback should have a query")
                .contains("title=A+Book")
        );
    }

    #[tokio::test]
    async fn two_no_matches_are_permanent() {
        let (url, requests, server) =
            mock_open_library(vec![r#"{"docs":[]}"#, r#"{"docs":[]}"#]).await;

        let outcome = search_with_fallback(
            &Client::new(),
            &url,
            "A Book: A Subtitle",
            &mut RequestPacer::default(),
        )
        .await;
        server.abort();

        assert!(matches!(outcome, SearchOutcome::NoMatch));
        assert_eq!(
            requests
                .lock()
                .expect("request capture lock should not be poisoned")
                .len(),
            2
        );
    }
}
