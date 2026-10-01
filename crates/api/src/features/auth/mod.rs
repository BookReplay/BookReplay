use std::{error::Error, fmt, sync::Arc};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State, rejection::JsonRejection},
    http::{StatusCode, Uri, header::LOCATION},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection, PgPool};
use tokio::sync::Semaphore;
use tracing::error;

// Held inside blocking work, even if the HTTP request is cancelled.
static PASSWORD_JOBS: Semaphore = Semaphore::const_new(DEFAULT_PASSWORD_JOBS);

pub(crate) const DEFAULT_PASSWORD_JOBS: usize = 2;
/// Serializes owner setup, so racing registrations still create one account.
const SETUP_LOCK: i64 = 4_815_162_342_108_001;
const INVALID_LOGIN: &str = "invalid email or password";
const DUMMY_PASSWORD: &str = "bookreplay authentication timing password";

pub type AuthSession = axum_login::AuthSession<AuthBackend>;

#[derive(Clone)]
pub struct AuthBackend {
    pool: PgPool,
    dummy_hash: Arc<str>,
    setup_hash: Option<Arc<str>>,
}

impl AuthBackend {
    pub async fn new(pool: PgPool, setup_secret: Option<String>) -> Result<Self, AuthError> {
        let dummy_hash = hash_password(DUMMY_PASSWORD.to_owned()).await?;
        let setup_hash = match setup_secret {
            Some(secret) => Some(hash_password(secret).await?.into()),
            None => None,
        };
        Ok(Self {
            pool,
            dummy_hash: dummy_hash.into(),
            setup_hash,
        })
    }
}

#[derive(Clone, FromRow)]
pub struct User {
    pub(crate) id: i64,
    password_hash: String,
}

impl fmt::Debug for User {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("User")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

#[derive(Clone)]
pub struct Credentials {
    email: String,
    password: String,
}

impl AuthnBackend for AuthBackend {
    type User = User;
    type Credentials = Credentials;
    type Error = AuthError;

    async fn authenticate(
        &self,
        credentials: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let email = normalize_email(&credentials.email);
        let user =
            sqlx::query_as::<_, User>("SELECT id, password_hash FROM users WHERE email = $1")
                .bind(email)
                .fetch_optional(&self.pool)
                .await
                .map_err(AuthError::from_display)?;

        let password_hash = user.as_ref().map_or_else(
            || self.dummy_hash.to_string(),
            |user| user.password_hash.clone(),
        );
        let password_matches = verify_password(credentials.password, password_hash).await?;

        Ok(if password_matches { user } else { None })
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        sqlx::query_as::<_, User>("SELECT id, password_hash FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AuthError::from_display)
    }
}

#[derive(Debug)]
pub enum AuthError {
    Internal,
    Busy,
}

impl AuthError {
    fn from_display(_error: impl fmt::Display) -> Self {
        Self::Internal
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Internal => "authentication operation failed",
            Self::Busy => "password processing busy",
        })
    }
}

impl Error for AuthError {}

#[derive(Deserialize)]
struct RegisterRequest {
    #[serde(default)]
    setup_secret: String,
    email: String,
    name: String,
    password: String,
}

/// Account details that passed validation; the email is trimmed and lowercased.
#[derive(Debug, PartialEq)]
pub struct Registration {
    pub email: String,
    pub name: String,
    pub password: String,
}

impl Registration {
    pub fn new(email: &str, name: &str, password: String) -> Result<Self, &'static str> {
        let email = normalize_email(email);
        if !valid_email(&email) {
            return Err("enter a valid email address");
        }

        let name = name.trim().to_owned();
        if !(1..=100).contains(&name.chars().count()) {
            return Err("name must be between 1 and 100 characters");
        }

        if !(12..=128).contains(&password.len()) {
            return Err("password must be between 12 and 128 bytes");
        }

        Ok(Self {
            email,
            name,
            password,
        })
    }
}

impl TryFrom<RegisterRequest> for Registration {
    type Error = &'static str;

    fn try_from(request: RegisterRequest) -> Result<Self, Self::Error> {
        Self::new(&request.email, &request.name, request.password)
    }
}

/// Stores an account and returns its id. The email must come from [`Registration`];
/// a duplicate fails with SQLSTATE 23505.
pub async fn create_user(
    connection: &mut PgConnection,
    email: &str,
    name: &str,
    password_hash: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO users (email, name, password_hash) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(email)
    .bind(name)
    .bind(password_hash)
    .fetch_one(connection)
    .await
}

/// Creates the owner unless an account already exists; `None` means one does.
async fn create_owner(
    pool: &PgPool,
    registration: &Registration,
    password_hash: &str,
) -> Result<Option<i64>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SETUP_LOCK)
        .execute(&mut *transaction)
        .await?;
    if sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users)")
        .fetch_one(&mut *transaction)
        .await?
    {
        return Ok(None);
    }
    let id = create_user(
        &mut transaction,
        &registration.email,
        &registration.name,
        password_hash,
    )
    .await?;
    transaction.commit().await?;
    Ok(Some(id))
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: &'static str,
}

pub fn router(backend: AuthBackend) -> Router {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/password", post(change_password))
        .layer(DefaultBodyLimit::max(4096))
        .with_state(backend)
}

async fn register(
    State(backend): State<AuthBackend>,
    payload: Result<Json<RegisterRequest>, JsonRejection>,
) -> Response {
    let Ok(Json(request)) = payload else {
        return api_error(StatusCode::BAD_REQUEST, "invalid registration request");
    };
    let Some(setup_hash) = &backend.setup_hash else {
        return api_error(StatusCode::FORBIDDEN, "owner setup is disabled");
    };
    match verify_password(request.setup_secret.clone(), setup_hash.to_string()).await {
        Ok(true) => {}
        Ok(false) => return api_error(StatusCode::FORBIDDEN, "invalid setup secret"),
        Err(error) => return auth_error(error),
    }
    let registration = match Registration::try_from(request) {
        Ok(registration) => registration,
        Err(message) => return api_error(StatusCode::BAD_REQUEST, message),
    };
    let password_hash = match hash_password(registration.password.clone()).await {
        Ok(password_hash) => password_hash,
        Err(error) => return auth_error(error),
    };

    match create_owner(&backend.pool, &registration, &password_hash).await {
        Ok(Some(_)) => (StatusCode::CREATED, [(LOCATION, "/login/")]).into_response(),
        Ok(None) => api_error(StatusCode::CONFLICT, "owner is already registered"),
        Err(_error) => {
            error!("failed to register owner");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error")
        }
    }
}

async fn login(
    mut auth_session: AuthSession,
    payload: Result<Json<LoginRequest>, JsonRejection>,
) -> Response {
    let Ok(Json(request)) = payload else {
        return api_error(StatusCode::BAD_REQUEST, "invalid login request");
    };
    if request.email.len() > 254 || request.password.len() > 128 {
        return api_error(StatusCode::UNAUTHORIZED, INVALID_LOGIN);
    }
    let credentials = Credentials {
        email: request.email,
        password: request.password,
    };
    let user = match auth_session.authenticate(credentials).await {
        Ok(Some(user)) => user,
        Ok(None) => return api_error(StatusCode::UNAUTHORIZED, INVALID_LOGIN),
        Err(error) => {
            if matches!(error, axum_login::Error::Backend(AuthError::Busy)) {
                return auth_error(AuthError::Busy);
            }
            error!("owner authentication failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error");
        }
    };

    match auth_session.login(&user).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_error) => {
            error!("failed to establish owner session");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error")
        }
    }
}

async fn logout(mut auth_session: AuthSession) -> Response {
    match auth_session.logout().await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(_error) => {
            error!("failed to invalidate owner session");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error")
        }
    }
}

#[derive(Clone)]
pub struct AccessGuard {
    pub pool: PgPool,
    /// Paths an extension serves without a session; each also covers what lies beneath it.
    pub public_paths: &'static [&'static str],
}

fn is_public(path: &str, public_paths: &[&str]) -> bool {
    public_paths.iter().any(|public| {
        path.strip_prefix(public)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

pub async fn access_guard(
    State(AccessGuard { pool, public_paths }): State<AccessGuard>,
    auth_session: AuthSession,
    request: Request,
    next: Next,
) -> Response {
    let resource = Resource::from_uri(request.uri());
    if resource == Resource::Asset || is_public(request.uri().path(), public_paths) {
        return next.run(request).await;
    }

    let authenticated = auth_session.user.is_some();
    let owner_exists = if authenticated {
        false
    } else {
        match sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users)")
            .fetch_one(&pool)
            .await
        {
            Ok(owner_exists) => owner_exists,
            Err(_error) => {
                error!("failed to determine owner registration state");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error");
            }
        }
    };

    if request.uri().path() == "/api/auth/register" && (authenticated || owner_exists) {
        return api_error(StatusCode::CONFLICT, "owner is already registered");
    }
    match access_policy(resource, authenticated, owner_exists) {
        Access::Allow => next.run(request).await,
        Access::Register => Redirect::to("/register/").into_response(),
        Access::Login => Redirect::to("/login/").into_response(),
        Access::Home => Redirect::to("/").into_response(),
        Access::Unauthorized => api_error(StatusCode::UNAUTHORIZED, "authentication required"),
    }
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn valid_email(email: &str) -> bool {
    if email.is_empty() || email.len() > 254 || email.chars().any(char::is_whitespace) {
        return false;
    }

    email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && !domain.is_empty() && !domain.contains('@')
    })
}

/// Raises the number of password hashes computed at once above the default.
pub(crate) fn allow_password_jobs(jobs: usize) {
    PASSWORD_JOBS.add_permits(jobs.saturating_sub(DEFAULT_PASSWORD_JOBS));
}

pub async fn hash_password(password: String) -> Result<String, AuthError> {
    let permit = PASSWORD_JOBS.try_acquire().map_err(|_| AuthError::Busy)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(AuthError::from_display)
    })
    .await
    .map_err(AuthError::from_display)?
}

pub async fn verify_password(password: String, password_hash: String) -> Result<bool, AuthError> {
    let permit = PASSWORD_JOBS.try_acquire().map_err(|_| AuthError::Busy)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let parsed_hash = PasswordHash::new(&password_hash).map_err(AuthError::from_display)?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    })
    .await
    .map_err(AuthError::from_display)?
}

fn api_error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(ErrorResponse { error: message })).into_response()
}

fn auth_error(error: AuthError) -> Response {
    match error {
        AuthError::Busy => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "1")],
            Json(ErrorResponse {
                error: "password processing busy; try again",
            }),
        )
            .into_response(),
        AuthError::Internal => api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error"),
    }
}

#[derive(Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(backend): State<AuthBackend>,
    auth_session: AuthSession,
    payload: Result<Json<ChangePasswordRequest>, JsonRejection>,
) -> Response {
    let Some(user) = auth_session.user else {
        return api_error(StatusCode::UNAUTHORIZED, "authentication required");
    };
    let Ok(Json(request)) = payload else {
        return api_error(StatusCode::BAD_REQUEST, "invalid password change request");
    };
    if request.current_password.len() > 128 || !(12..=128).contains(&request.new_password.len()) {
        return api_error(
            StatusCode::BAD_REQUEST,
            "password must be between 12 and 128 bytes",
        );
    }
    match verify_password(request.current_password, user.password_hash.clone()).await {
        Ok(true) => {}
        Ok(false) => return api_error(StatusCode::UNAUTHORIZED, "incorrect current password"),
        Err(error) => return auth_error(error),
    }
    let hash = match hash_password(request.new_password).await {
        Ok(hash) => hash,
        Err(error) => return auth_error(error),
    };
    // Compare the old hash so a racing change cannot undo an operator reset.
    match sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2 AND password_hash = $3")
        .bind(hash)
        .bind(user.id)
        .bind(user.password_hash)
        .execute(&backend.pool)
        .await
    {
        Ok(result) if result.rows_affected() == 1 => StatusCode::NO_CONTENT.into_response(),
        Ok(_) => api_error(StatusCode::CONFLICT, "password changed; log in again"),
        Err(_) => api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error"),
    }
}

/// Resets the only account, or the one with `email` when it is given.
pub(crate) async fn reset_password(
    pool: &PgPool,
    email: Option<&str>,
    password: String,
) -> Result<(), Box<dyn Error>> {
    if !(12..=128).contains(&password.len()) {
        return Err("password must be between 12 and 128 bytes".into());
    }
    let hash = hash_password(password).await?;
    let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .map_err(|_| "could not reset owner password")?;
    if accounts == 0 {
        return Err("no owner exists; use initial setup".into());
    }
    if accounts > 1 && email.is_none() {
        return Err("several accounts exist; name one by email".into());
    }
    let result =
        sqlx::query("UPDATE users SET password_hash = $1 WHERE $2::text IS NULL OR email = $2")
            .bind(hash)
            .bind(email.map(normalize_email))
            .execute(pool)
            .await
            .map_err(|_| "could not reset owner password")?;
    if result.rows_affected() != 1 {
        return Err("no account has that email".into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Resource {
    Asset,
    Registration,
    Login,
    Api,
    Page,
}

impl Resource {
    fn from_uri(uri: &Uri) -> Self {
        match uri.path() {
            path if path.starts_with("/_app/") => Self::Asset,
            "/register" | "/register/" | "/register/index.html" | "/api/auth/register" => {
                Self::Registration
            }
            "/login" | "/login/" | "/login/index.html" | "/api/auth/login" => Self::Login,
            path if path == "/api" || path.starts_with("/api/") => Self::Api,
            _ => Self::Page,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Access {
    Allow,
    Register,
    Login,
    Home,
    Unauthorized,
}

fn access_policy(resource: Resource, authenticated: bool, owner_exists: bool) -> Access {
    if resource == Resource::Asset {
        return Access::Allow;
    }
    if authenticated {
        return if matches!(resource, Resource::Registration | Resource::Login) {
            Access::Home
        } else {
            Access::Allow
        };
    }
    if !owner_exists {
        return if resource == Resource::Registration {
            Access::Allow
        } else if resource == Resource::Api {
            Access::Unauthorized
        } else {
            Access::Register
        };
    }

    match resource {
        Resource::Login => Access::Allow,
        Resource::Registration => Access::Login,
        Resource::Api => Access::Unauthorized,
        Resource::Page => Access::Login,
        Resource::Asset => Access::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register_request(email: &str, name: &str, password: &str) -> RegisterRequest {
        RegisterRequest {
            setup_secret: String::new(),
            email: email.to_owned(),
            name: name.to_owned(),
            password: password.to_owned(),
        }
    }

    #[test]
    fn registration_normalizes_email_and_name_without_trimming_password() {
        let registration = Registration::try_from(register_request(
            "  OWNER@Example.COM  ",
            "  BookReplay Owner  ",
            "  password  ",
        ))
        .expect("registration should be valid");

        assert_eq!(
            registration,
            Registration {
                email: "owner@example.com".to_owned(),
                name: "BookReplay Owner".to_owned(),
                password: "  password  ".to_owned(),
            }
        );
    }

    #[test]
    fn registration_accepts_email_length_boundaries() {
        let longest_email = format!("{}@x", "a".repeat(252));

        assert!(
            Registration::try_from(register_request(&longest_email, "A", "123456789012")).is_ok()
        );
    }

    #[test]
    fn registration_rejects_email_over_254_bytes() {
        let email = format!("{}@x", "a".repeat(253));

        assert_eq!(
            Registration::try_from(register_request(&email, "A", "123456789012")),
            Err("enter a valid email address")
        );
    }

    #[test]
    fn registration_rejects_invalid_email_shapes() {
        for email in [
            "",
            "owner",
            "@example.com",
            "owner@",
            "a@@example.com",
            "a @example.com",
        ] {
            assert_eq!(
                Registration::try_from(register_request(email, "A", "123456789012")),
                Err("enter a valid email address"),
                "email {email:?} should be rejected"
            );
        }
    }

    #[test]
    fn registration_accepts_name_length_boundaries() {
        for name in ["a".to_owned(), "a".repeat(100)] {
            assert!(Registration::try_from(register_request("a@b", &name, "123456789012")).is_ok());
        }
    }

    #[test]
    fn registration_rejects_name_outside_length_boundaries() {
        for name in ["   ".to_owned(), "a".repeat(101)] {
            assert_eq!(
                Registration::try_from(register_request("a@b", &name, "123456789012")),
                Err("name must be between 1 and 100 characters")
            );
        }
    }

    #[test]
    fn registration_accepts_password_byte_length_boundaries() {
        for password in ["a".repeat(12), "a".repeat(128)] {
            assert!(Registration::try_from(register_request("a@b", "A", &password)).is_ok());
        }
    }

    #[test]
    fn registration_rejects_password_outside_byte_length_boundaries() {
        for password in ["a".repeat(11), "a".repeat(129)] {
            assert_eq!(
                Registration::try_from(register_request("a@b", "A", &password)),
                Err("password must be between 12 and 128 bytes")
            );
        }
    }

    #[tokio::test]
    async fn password_hash_verifies_only_the_correct_password() {
        let hash = hash_password("correct horse battery staple".to_owned())
            .await
            .expect("password hashing should succeed");
        let second_hash = hash_password("correct horse battery staple".to_owned())
            .await
            .expect("password hashing should succeed");

        assert!(hash.starts_with("$argon2id$v=19$"));
        assert_ne!(hash, second_hash);
        assert!(
            verify_password("correct horse battery staple".to_owned(), hash.clone())
                .await
                .expect("password verification should succeed")
        );
        assert!(
            !verify_password("wrong password".to_owned(), hash)
                .await
                .expect("wrong-password verification should complete")
        );
        let permits = PASSWORD_JOBS.try_acquire_many(2).unwrap();
        assert!(matches!(
            hash_password("cannot queue".to_owned()).await,
            Err(AuthError::Busy)
        ));
        assert!(matches!(
            verify_password("cannot queue".to_owned(), second_hash).await,
            Err(AuthError::Busy)
        ));
        drop(permits);
    }

    #[test]
    fn public_paths_cover_only_themselves_and_what_lies_beneath() {
        let public = ["/verify", "/api/account"];

        for path in [
            "/verify",
            "/verify/",
            "/verify/index.html",
            "/api/account/signup",
        ] {
            assert!(is_public(path, &public), "{path}");
        }
        for path in ["/", "/verified", "/api/accounts", "/api/books", "/x/verify"] {
            assert!(!is_public(path, &public), "{path}");
        }
        assert!(!is_public("/api/books", &[]));
    }

    #[test]
    fn access_policy_covers_every_owner_and_authentication_state() {
        let resources = [
            Resource::Registration,
            Resource::Login,
            Resource::Page,
            Resource::Api,
        ];

        assert_eq!(
            resources.map(|resource| access_policy(resource, false, false)),
            [
                Access::Allow,
                Access::Register,
                Access::Register,
                Access::Unauthorized
            ]
        );
        assert_eq!(
            resources.map(|resource| access_policy(resource, false, true)),
            [
                Access::Login,
                Access::Allow,
                Access::Login,
                Access::Unauthorized
            ]
        );
        assert_eq!(
            resources.map(|resource| access_policy(resource, true, true)),
            [Access::Home, Access::Home, Access::Allow, Access::Allow]
        );
    }
}
