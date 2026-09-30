use std::{
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

use bookreplay_kindle::from_kindle_title;
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

mod candidate;
pub use candidate::{BookCandidate, Provider};
use candidate::{
    GoogleResponse, author_name, clean_title, needs_more, normalize, rank, supplement,
};

impl From<SearchDocument> for BookCandidate {
    fn from(document: SearchDocument) -> Self {
        Self {
            provider: Provider::OpenLibrary,
            provider_id: document.key,
            title: document.title,
            authors: document
                .author_name
                .into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect(),
            cover_url: document
                .cover_i
                .filter(|id| *id > 0)
                .map(|id| format!("https://covers.openlibrary.org/b/id/{id}-L.jpg")),
            first_publish_year: document.first_publish_year,
            edition_count: document.edition_count,
            isbns: document.isbn,
        }
    }
}

#[derive(Debug)]
pub enum SearchError {
    Request(reqwest::Error),
    Http {
        status: StatusCode,
        retry_after: Option<Duration>,
    },
    Malformed(reqwest::Error),
}

impl fmt::Display for SearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(_) => formatter.write_str("metadata request failed"),
            Self::Http { status, .. } => {
                write!(formatter, "metadata provider returned HTTP {status}")
            }
            Self::Malformed(_) => formatter.write_str("metadata response was malformed"),
        }
    }
}

impl std::error::Error for SearchError {}

#[derive(Clone)]
pub struct OpenLibrary {
    client: Client,
    search_url: String,
    pacer: Arc<Mutex<RequestPacer>>,
    google_key: Option<String>,
    google_url: String,
}

impl OpenLibrary {
    pub fn new(client: Client) -> Self {
        Self::with_search_url(client, SEARCH_URL)
    }

    fn with_search_url(client: Client, search_url: &str) -> Self {
        Self {
            client,
            google_key: None,
            google_url: "https://www.googleapis.com/books/v1/volumes".into(),
            search_url: search_url.to_owned(),
            pacer: Arc::new(Mutex::new(RequestPacer::default())),
        }
    }

    pub fn with_google_key(mut self, key: Option<String>) -> Self {
        self.google_key = key.filter(|key| !key.trim().is_empty());
        self
    }

    pub async fn search(&self, query: &str) -> Result<Vec<BookCandidate>, SearchError> {
        self.candidates(query, &[], true).await
    }

    async fn candidates(
        &self,
        title: &str,
        authors: &[&str],
        manual: bool,
    ) -> Result<Vec<BookCandidate>, SearchError> {
        let mut books = Vec::new();
        let mut failure = None;
        for provider in [Provider::OpenLibrary, Provider::GoogleBooks] {
            if provider == Provider::GoogleBooks
                && (self.google_key.is_none() || (!manual && !needs_more(&books, title)))
            {
                continue;
            }
            let clean = clean_title(title);
            let mut queries = vec![(clean, authors)];
            if !authors.is_empty() {
                queries.push((clean, &[]));
            }
            if let Some(short) = shortened_title(clean) {
                queries.push((short, &[]));
            }
            for (query, query_authors) in queries {
                match self.request(provider, query, query_authors, manual).await {
                    Ok(found) => {
                        for book in found {
                            if book.validate()
                                && !books.iter().any(|b: &BookCandidate| {
                                    b.provider == book.provider && b.provider_id == book.provider_id
                                })
                            {
                                books.push(book);
                            }
                        }
                        rank(&mut books, title, authors);
                        if !needs_more(&books, title) {
                            break;
                        }
                    }
                    Err(error) => {
                        // Keep transient failures over terminal ones so the queue can retry.
                        if failure
                            .as_ref()
                            .is_none_or(|e: &SearchError| !e.is_transient())
                        {
                            failure = Some(error);
                        }
                        break;
                    }
                }
            }
        }
        if books.is_empty()
            && let Some(error) = failure
        {
            return Err(error);
        }
        Ok(books)
    }

    async fn request(
        &self,
        provider: Provider,
        query: &str,
        authors: &[&str],
        manual: bool,
    ) -> Result<Vec<BookCandidate>, SearchError> {
        self.wait().await;
        let title = normalize(query);
        let author = authors
            .iter()
            .map(|a| author_name(a))
            .collect::<Vec<_>>()
            .join(" ");
        let request = match provider {
            Provider::OpenLibrary => {
                let mut request = self.client.get(&self.search_url).query(&[
                    (if manual { "q" } else { "title" }, title.as_str()),
                    ("limit", "10"),
                    ("fields", SEARCH_FIELDS),
                ]);
                if !author.is_empty() {
                    request = request.query(&[("author", &author)]);
                }
                request
            }
            Provider::GoogleBooks => {
                let query = if manual {
                    title
                } else {
                    let mut query = format!("intitle:\"{title}\"");
                    if !author.is_empty() {
                        query.push_str(&format!(" inauthor:\"{author}\""));
                    }
                    query
                };
                self.client.get(&self.google_url).query(&[
                    ("q", query.as_str()),
                    ("maxResults", "10"),
                    ("key", self.google_key.as_deref().unwrap_or_default()),
                ])
            }
        };
        let response = request
            .send()
            .await
            .map_err(|error| SearchError::Request(error.without_url()))?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| parse_retry_after(value, SystemTime::now()));
            return Err(SearchError::Http {
                status,
                retry_after,
            });
        }
        match provider {
            Provider::OpenLibrary => response
                .json::<SearchResponse>()
                .await
                .map(|response| response.docs.into_iter().map(Into::into).collect())
                .map_err(|error| SearchError::Malformed(error.without_url())),
            Provider::GoogleBooks => response
                .json::<GoogleResponse>()
                .await
                .map(|response| {
                    let _ = response.total_items;
                    response.items.into_iter().map(Into::into).collect()
                })
                .map_err(|error| SearchError::Malformed(error.without_url())),
        }
    }

    async fn wait(&self) {
        let request_at = {
            let mut pacer = self.pacer.lock().unwrap_or_else(|error| error.into_inner());
            pacer.reserve(Instant::now())
        };
        tokio::time::sleep_until(request_at.into()).await;
    }
}

struct UpstreamFailure {
    message: String,
    retry_after: Option<Duration>,
}

enum SearchOutcome {
    Match(BookCandidate),
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
    fn reserve(&mut self, now: Instant) -> Instant {
        let request_at = self
            .last_request_at
            .map_or(now, |last| now.max(last + MIN_REQUEST_INTERVAL));
        self.last_request_at = Some(request_at);
        request_at
    }
}

fn shortened_title(title: &str) -> Option<&str> {
    let (prefix, _) = title.split_once(':')?;
    let prefix = prefix.trim();

    (!prefix.is_empty() && prefix != title).then_some(prefix)
}

pub fn spawn_enrichment_worker(database_url: String, open_library: OpenLibrary) {
    tokio::spawn(async move {
        run_enrichment_worker(&database_url, &open_library).await;
    });
}

async fn run_enrichment_worker(database_url: &str, open_library: &OpenLibrary) {
    loop {
        let mut connection = match PgConnection::connect(database_url).await {
            Ok(connection) => connection,
            Err(_error) => {
                error!("book enrichment worker failed to connect to database");
                tokio::time::sleep(IDLE_DELAY).await;
                continue;
            }
        };

        info!("book enrichment worker is waiting for leadership");
        loop {
            match try_acquire_leadership(&mut connection).await {
                Ok(true) => {
                    info!("book enrichment worker acquired leadership");
                    if let Err(_error) = run_as_leader(&mut connection, open_library).await {
                        error!("book enrichment worker lost its database session");
                    }
                    break;
                }
                Ok(false) => tokio::time::sleep(IDLE_DELAY).await,
                Err(_error) => {
                    error!("book enrichment worker leadership check failed");
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

async fn run_as_leader(
    connection: &mut PgConnection,
    open_library: &OpenLibrary,
) -> Result<(), sqlx::Error> {
    loop {
        match process_next_book(connection, open_library).await? {
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
    open_library: &OpenLibrary,
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
        retry_count = book.retry_count,
        "processing book enrichment backlog item"
    );

    let (title, authors) = from_kindle_title(&book.kindle_title);
    let outcome = search_with_fallback(open_library, title, &authors).await;

    match outcome {
        SearchOutcome::Match(document) => {
            save_metadata(connection, &book, &document).await?;
            Ok(WorkerStep::Processed)
        }
        SearchOutcome::NoMatch => {
            mark_checked(connection, &book, None).await?;
            warn!(
                book_id = book.id,
                "book enrichment completed without a metadata match"
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
            error!(book_id = book.id, "book enrichment failed permanently");
            Ok(WorkerStep::Processed)
        }
    }
}

async fn search_with_fallback(
    service: &OpenLibrary,
    title: &str,
    authors: &[&str],
) -> SearchOutcome {
    match service.candidates(title, authors, false).await {
        Ok(books) => supplement(&books).map_or(SearchOutcome::NoMatch, SearchOutcome::Match),
        Err(error) if error.is_transient() => {
            let retry_after = match &error {
                SearchError::Http { retry_after, .. } => *retry_after,
                _ => None,
            };
            SearchOutcome::Transient(UpstreamFailure {
                message: error.to_string(),
                retry_after,
            })
        }
        Err(error) => SearchOutcome::Terminal(error.to_string()),
    }
}

impl SearchError {
    fn is_transient(&self) -> bool {
        match self {
            Self::Http { status, .. } => is_transient_status(*status),
            _ => true,
        }
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
    document: &BookCandidate,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE books SET title = COALESCE(NULLIF($2, ''), title), authors = CASE WHEN cardinality($3::text[]) > 0 THEN $3 ELSE authors END, open_library_key = COALESCE($4, open_library_key), \
         cover_url = COALESCE($5, cover_url), first_publish_year = COALESCE($6, first_publish_year), edition_count = COALESCE($7, edition_count), isbns = CASE WHEN cardinality($8::text[]) > 0 THEN $8 ELSE isbns END, google_books_volume_id = COALESCE($9, google_books_volume_id), \
         metadata_checked_at = NOW(), last_error = NULL, updated_at = NOW() \
         WHERE id = $1 AND metadata_checked_at IS NULL",
    )
    .bind(book.id)
    .bind(&document.title)
    .bind(&document.authors)
    .bind(document.open_library_key())
    .bind(&document.cover_url)
    .bind(document.first_publish_year)
    .bind(document.edition_count)
    .bind(&document.isbns)
    .bind(document.google_books_volume_id())
    .execute(connection)
    .await
    .inspect_err(|_error| {
        error!(book_id = book.id, "failed to save enriched book metadata");
    })?;

    info!(
        book_id = book.id,

        provider_id = %document.provider_id,
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
         WHERE id = $1 AND metadata_checked_at IS NULL",
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
         last_error = $3, updated_at = NOW() WHERE id = $1 AND metadata_checked_at IS NULL",
    )
    .bind(book.id)
    .bind(delay.as_secs_f64())
    .bind(message)
    .execute(connection)
    .await?;

    warn!(
        book_id = book.id,
        retry_count = book.retry_count + 1,
        retry_in_seconds = delay.as_secs_f64(),
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
        MAX_RETRY_DELAY, OpenLibrary, SEARCH_FIELDS, SearchError, SearchOutcome, from_kindle_title,
        is_transient_status, parse_retry_after, retry_delay, search_with_fallback, shortened_title,
    };

    pub(super) async fn mock_open_library(
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
                .unwrap_or(r#"{"docs":[]}"#)
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
    async fn search_returns_multiple_candidates() {
        let (url, _, server) = mock_open_library(vec![
            r#"{"docs":[{"key":"/works/OL1W","title":"First","author_name":["One"]},{"key":"/works/OL2W","title":"Second","edition_count":4}]}"#,
        ])
        .await;
        let books = OpenLibrary::with_search_url(Client::new(), &url)
            .search("A Book")
            .await
            .expect("search should succeed");
        server.abort();

        assert_eq!(books.len(), 2);
    }

    #[tokio::test]
    async fn search_uses_general_query_limit_and_exact_fields() {
        let (url, requests, server) = mock_open_library(vec![r#"{"docs":[]}"#]).await;
        OpenLibrary::with_search_url(Client::new(), &url)
            .search("A Book: A Subtitle")
            .await
            .expect("search should succeed");
        server.abort();

        let requests = requests
            .lock()
            .expect("request capture lock should not be poisoned");
        let query = requests[0].query().expect("search should have a query");
        assert!(query.contains("q=a+book+a+subtitle"));
        assert!(query.contains("limit=10"));
        assert_eq!(
            query.split('&').find(|part| part.starts_with("fields=")),
            Some(format!("fields={}", SEARCH_FIELDS.replace(',', "%2C")).as_str())
        );
    }

    #[tokio::test]
    async fn cloned_services_share_request_pacing() {
        let (url, _, server) = mock_open_library(vec![r#"{"docs":[]}"#, r#"{"docs":[]}"#]).await;
        let open_library = OpenLibrary::with_search_url(Client::new(), &url);
        let clone = open_library.clone();

        let started_at = Instant::now();
        let (first, second) = tokio::join!(open_library.search("First"), clone.search("Second"));
        server.abort();

        assert!(first.is_ok() && second.is_ok());
        assert!(started_at.elapsed() >= Duration::from_secs(1));
    }

    #[tokio::test]
    async fn malformed_response_is_classified() {
        let (url, _, server) = mock_open_library(vec!["not JSON"]).await;

        let outcome = OpenLibrary::with_search_url(Client::new(), &url)
            .search("A Book")
            .await;
        server.abort();

        assert!(matches!(outcome, Err(SearchError::Malformed(_))));
    }

    #[tokio::test]
    async fn upstream_http_failure_is_classified() {
        let app = Router::new().fallback(|| async { StatusCode::BAD_GATEWAY });
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

        let outcome =
            OpenLibrary::with_search_url(Client::new(), &format!("http://{address}/search.json"))
                .search("A Book")
                .await;
        server.abort();

        assert!(matches!(
            outcome,
            Err(SearchError::Http {
                status: StatusCode::BAD_GATEWAY,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn empty_results_succeed() {
        let (url, _, server) = mock_open_library(vec![r#"{"docs":[]}"#]).await;

        let outcome = OpenLibrary::with_search_url(Client::new(), &url)
            .search("Unknown")
            .await;
        server.abort();

        assert!(matches!(outcome, Ok(books) if books.is_empty()));
    }

    #[tokio::test]
    async fn no_match_uses_shortened_title_once() {
        let (url, requests, server) = mock_open_library(vec![
            r#"{"docs":[]}"#,
            r#"{"docs":[{"key":"/works/OL1W","title":"A Book"}]}"#,
        ])
        .await;

        let outcome = search_with_fallback(
            &OpenLibrary::with_search_url(Client::new(), &url),
            "A Book: A Subtitle",
            &[],
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
                .contains("title=a+book")
        );
    }

    #[tokio::test]
    async fn two_no_matches_are_permanent() {
        let (url, requests, server) =
            mock_open_library(vec![r#"{"docs":[]}"#, r#"{"docs":[]}"#]).await;

        let outcome = search_with_fallback(
            &OpenLibrary::with_search_url(Client::new(), &url),
            "A Book: A Subtitle",
            &[],
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

#[cfg(test)]
mod provider_tests {
    use super::*;
    use axum::{Router, http::Uri, response::IntoResponse};
    use std::collections::VecDeque;

    const OL: &str =
        r#"{"docs":[{"key":"/works/OL1W","title":"Book","author_name":["Author"],"cover_i":1}]}"#;
    const GOOGLE: &str = r#"{"totalItems":1,"items":[{"id":"google_1","volumeInfo":{"title":"Book","authors":["Author"],"publishedDate":"2020","imageLinks":{"thumbnail":"http://books.google.com/books/content?id=google_1&img=1"}}}]}"#;

    async fn service(
        ol: Vec<(u16, &'static str)>,
        google: Vec<(u16, &'static str)>,
        delay: Duration,
    ) -> (
        OpenLibrary,
        Arc<Mutex<Vec<Uri>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let ol = Arc::new(Mutex::new(VecDeque::from(ol)));
        let google = Arc::new(Mutex::new(VecDeque::from(google)));
        let app = Router::new().fallback(move |uri: Uri| {
            captured.lock().unwrap().push(uri.clone());
            let reply = if uri.path() == "/ol" {
                ol.lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or((200, r#"{"docs":[]}"#))
            } else {
                google
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or((200, r#"{"totalItems":0}"#))
            };
            async move {
                if uri.path() == "/ol" {
                    tokio::time::sleep(delay).await;
                }
                (
                    StatusCode::from_u16(reply.0).unwrap(),
                    [("retry-after", "45")],
                    reply.1,
                )
                    .into_response()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let mut service = OpenLibrary::with_search_url(client, &format!("http://{address}/ol"))
            .with_google_key(Some("test-secret".into()));
        service.google_url = format!("http://{address}/google");
        (service, requests, server)
    }

    #[tokio::test]
    async fn exact_complete_open_library_match_skips_google_but_manual_search_queries_both() {
        let (service, requests, server) = service(
            vec![(200, OL), (200, OL)],
            vec![(200, GOOGLE)],
            Duration::ZERO,
        )
        .await;
        let automatic = service
            .candidates("Book", &["Author"], false)
            .await
            .unwrap();
        assert_eq!(automatic[0].provider, Provider::OpenLibrary);
        assert_eq!(requests.lock().unwrap().len(), 1);
        let manual = service.search("Book").await.unwrap();
        assert_eq!(manual.len(), 2);
        assert_eq!(manual[1].first_publish_year, None);
        assert!(
            manual[1]
                .cover_url
                .as_ref()
                .unwrap()
                .starts_with("https://")
        );
        server.abort();
    }

    #[tokio::test]
    async fn google_handles_empty_imperfect_or_incomplete_open_library_results() {
        for body in [
            r#"{"docs":[]}"#,
            r#"{"docs":[{"key":"/works/OL1W","title":"Different","author_name":["Author"],"cover_i":1}]}"#,
            r#"{"docs":[{"key":"/works/OL1W","title":"Book","author_name":["Author"]}]}"#,
            r#"{"docs":[{"key":"/works/OL1W","title":"Book","cover_i":1}]}"#,
        ] {
            let (service, _, server) =
                service(vec![(200, body)], vec![(200, GOOGLE)], Duration::ZERO).await;
            let books = service.candidates("Book", &[], false).await.unwrap();
            assert_eq!(books[0].provider, Provider::GoogleBooks);
            server.abort();
        }
    }

    #[tokio::test]
    async fn failures_try_other_source_and_retry_when_no_usable_results() {
        for (status, body, delay) in [
            (429, "", Duration::ZERO),
            (503, "", Duration::ZERO),
            (200, "malformed", Duration::ZERO),
            (200, OL, Duration::from_millis(200)),
        ] {
            let (service, _, server) = service(
                vec![(status, body), (status, body)],
                vec![(200, GOOGLE)],
                delay,
            )
            .await;
            assert!(matches!(
                search_with_fallback(&service, "Book", &[]).await,
                SearchOutcome::Match(_)
            ));
            assert!(matches!(
                search_with_fallback(&service, "Book", &[]).await,
                SearchOutcome::Transient(_)
            ));
            server.abort();
        }
    }

    #[tokio::test]
    async fn google_failure_keeps_available_open_library_candidate() {
        let (service, _, server) = service(
            vec![(200, r#"{"docs":[{"key":"/works/OL1W","title":"Book"}]}"#)],
            vec![(429, "")],
            Duration::ZERO,
        )
        .await;
        assert!(matches!(
            search_with_fallback(&service, "Book", &[]).await,
            SearchOutcome::Match(_)
        ));
        server.abort();
    }

    #[tokio::test]
    async fn empty_sources_are_unmatched_and_disabled_google_is_never_called() {
        let (mut service, requests, server) = service(vec![], vec![], Duration::ZERO).await;
        assert!(matches!(
            search_with_fallback(&service, "Book", &[]).await,
            SearchOutcome::NoMatch
        ));
        service.google_key = None;
        assert!(service.search("Book").await.unwrap().is_empty());
        assert_eq!(
            requests
                .lock()
                .unwrap()
                .iter()
                .filter(|uri| uri.path() == "/google")
                .count(),
            1
        );
        server.abort();
    }

    #[tokio::test]
    async fn bounded_queries_normalize_authors_then_remove_subtitle() {
        let (service, requests, server) = service(vec![], vec![], Duration::ZERO).await;
        service
            .candidates(
                "Réussir: Un guide (French Edition)",
                &["Tran, Kevin"],
                false,
            )
            .await
            .unwrap();
        let queries: Vec<_> = requests
            .lock()
            .unwrap()
            .iter()
            .map(|uri| uri.query().unwrap().to_owned())
            .collect();
        assert_eq!(queries.len(), 6);
        assert!(queries[0].contains("author=kevin+tran"));
        assert!(!queries[1].contains("author="));
        assert!(queries[2].contains("title=r%C3%A9ussir&"));
        assert!(queries.iter().all(|query| !query.contains("french")));
        server.abort();
    }

    #[tokio::test]
    async fn malformed_google_response_is_retryable_and_key_is_redacted() {
        let (service, _, server) = service(vec![], vec![(200, "{}")], Duration::ZERO).await;
        let error = service.search("Book").await.unwrap_err();
        assert!(error.is_transient());
        assert!(!format!("{error:?}").contains("test-secret"));
        server.abort();
    }
}

#[cfg(test)]
mod database_tests {
    use super::*;

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL pointing to a disposable PostgreSQL server"]
    async fn enrichment_preserves_values_and_manual_identification_wins(pool: sqlx::PgPool) {
        let mut connection = pool.acquire().await.unwrap();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO books (kindle_title, title, authors, cover_url) VALUES ('Original (Author)', 'Original', ARRAY['Author'], 'https://covers.openlibrary.org/b/id/1-L.jpg') RETURNING id"
        ).fetch_one(&mut *connection).await.unwrap();
        let book = PendingBook {
            id,
            kindle_title: "Original (Author)".into(),
            retry_count: 0,
        };
        let candidate = BookCandidate {
            provider: Provider::GoogleBooks,
            provider_id: "volume_1".into(),
            title: "Enriched".into(),
            authors: vec![],
            cover_url: None,
            first_publish_year: None,
            edition_count: None,
            isbns: vec![],
        };
        save_metadata(&mut connection, &book, &candidate)
            .await
            .unwrap();
        let row: (String, Vec<String>, Option<String>, Option<String>, Option<i32>) = sqlx::query_as(
            "SELECT title, authors, cover_url, google_books_volume_id, first_publish_year FROM books WHERE id=$1"
        ).bind(id).fetch_one(&mut *connection).await.unwrap();
        assert_eq!(
            row,
            (
                "Enriched".into(),
                vec!["Author".into()],
                Some("https://covers.openlibrary.org/b/id/1-L.jpg".into()),
                Some("volume_1".into()),
                None
            )
        );

        // Simulate a worker that already fetched this pending item before a manual save.
        sqlx::query("UPDATE books SET title='Manual', metadata_checked_at=NOW() WHERE id=$1")
            .bind(id)
            .execute(&mut *connection)
            .await
            .unwrap();
        save_metadata(&mut connection, &book, &candidate)
            .await
            .unwrap();
        mark_checked(&mut connection, &book, Some("late failure"))
            .await
            .unwrap();
        schedule_retry(
            &mut connection,
            &book,
            "late retry",
            Duration::from_secs(30),
        )
        .await
        .unwrap();
        let row: (String, i32, Option<String>) =
            sqlx::query_as("SELECT title, retry_count, last_error FROM books WHERE id=$1")
                .bind(id)
                .fetch_one(&mut *connection)
                .await
                .unwrap();
        assert_eq!(row, ("Manual".into(), 0, None));
        let service = OpenLibrary::new(Client::new());
        assert!(matches!(
            process_next_book(&mut connection, &service).await.unwrap(),
            WorkerStep::Idle
        ));
    }

    #[sqlx::test(migrations = "../../migrations")]
    #[ignore = "requires DATABASE_URL pointing to a disposable PostgreSQL server"]
    async fn pending_import_is_processed_with_ranked_metadata(pool: sqlx::PgPool) {
        let (url, _, server) = super::tests::mock_open_library(vec![
            r#"{"docs":[{"key":"/works/OL1W","title":"Wrong","author_name":["Author"],"cover_i":1},{"key":"/works/OL2W","title":"Book","author_name":["Author"],"cover_i":2}]}"#,
        ]).await;
        let mut connection = pool.acquire().await.unwrap();
        sqlx::query("INSERT INTO books (kindle_title, title) VALUES ('Book (Author)', 'Book')")
            .execute(&mut *connection)
            .await
            .unwrap();
        let service = OpenLibrary::with_search_url(Client::new(), &url);
        assert!(matches!(
            process_next_book(&mut connection, &service).await.unwrap(),
            WorkerStep::Processed
        ));
        let row: (String, String, bool) = sqlx::query_as(
            "SELECT kindle_title, open_library_key, metadata_checked_at IS NOT NULL FROM books",
        )
        .fetch_one(&mut *connection)
        .await
        .unwrap();
        assert_eq!(row, ("Book (Author)".into(), "/works/OL2W".into(), true));
        server.abort();
    }
}
