use std::{error::Error, fmt, sync::Arc};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::{
    Json, Router,
    extract::{Request, State, rejection::JsonRejection},
    http::{StatusCode, Uri, header::LOCATION},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use tracing::error;

const OWNER_ID: i16 = 1;
const INVALID_LOGIN: &str = "invalid email or password";
const DUMMY_PASSWORD: &str = "bookreplay authentication timing password";

pub type AuthSession = axum_login::AuthSession<AuthBackend>;

#[derive(Clone)]
pub struct AuthBackend {
    pool: PgPool,
    dummy_hash: Arc<str>,
}

impl AuthBackend {
    pub async fn new(pool: PgPool) -> Result<Self, AuthError> {
        let dummy_hash = hash_password(DUMMY_PASSWORD.to_owned()).await?;
        Ok(Self {
            pool,
            dummy_hash: dummy_hash.into(),
        })
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct Owner {
    pub(crate) id: i16,
    password_hash: String,
}

impl AuthUser for Owner {
    type Id = i16;

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
    type User = Owner;
    type Credentials = Credentials;
    type Error = AuthError;

    async fn authenticate(
        &self,
        credentials: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let email = normalize_email(&credentials.email);
        let owner =
            sqlx::query_as::<_, Owner>("SELECT id, password_hash FROM owner WHERE email = $1")
                .bind(email)
                .fetch_optional(&self.pool)
                .await
                .map_err(AuthError::from_display)?;

        let password_hash = owner.as_ref().map_or_else(
            || self.dummy_hash.to_string(),
            |owner| owner.password_hash.clone(),
        );
        let password_matches = verify_password(credentials.password, password_hash).await?;

        Ok(if password_matches { owner } else { None })
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        sqlx::query_as::<_, Owner>("SELECT id, password_hash FROM owner WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AuthError::from_display)
    }
}

#[derive(Debug)]
pub struct AuthError(String);

impl AuthError {
    fn from_display(error: impl fmt::Display) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for AuthError {}

#[derive(Deserialize)]
struct RegisterRequest {
    email: String,
    name: String,
    password: String,
}

#[derive(Debug, PartialEq)]
struct Registration {
    email: String,
    name: String,
    password: String,
}

impl TryFrom<RegisterRequest> for Registration {
    type Error = &'static str;

    fn try_from(request: RegisterRequest) -> Result<Self, Self::Error> {
        let email = normalize_email(&request.email);
        if !valid_email(&email) {
            return Err("enter a valid email address");
        }

        let name = request.name.trim().to_owned();
        if !(1..=100).contains(&name.chars().count()) {
            return Err("name must be between 1 and 100 characters");
        }

        if !(12..=128).contains(&request.password.len()) {
            return Err("password must be between 12 and 128 bytes");
        }

        Ok(Self {
            email,
            name,
            password: request.password,
        })
    }
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
        .with_state(backend)
}

async fn register(
    State(backend): State<AuthBackend>,
    payload: Result<Json<RegisterRequest>, JsonRejection>,
) -> Response {
    let Ok(Json(request)) = payload else {
        return api_error(StatusCode::BAD_REQUEST, "invalid registration request");
    };
    let registration = match Registration::try_from(request) {
        Ok(registration) => registration,
        Err(message) => return api_error(StatusCode::BAD_REQUEST, message),
    };
    let password_hash = match hash_password(registration.password).await {
        Ok(password_hash) => password_hash,
        Err(error) => {
            error!(%error, "failed to hash owner password");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error");
        }
    };

    let result =
        sqlx::query("INSERT INTO owner (id, email, name, password_hash) VALUES ($1, $2, $3, $4)")
            .bind(OWNER_ID)
            .bind(registration.email)
            .bind(registration.name)
            .bind(password_hash)
            .execute(&backend.pool)
            .await;

    match result {
        Ok(_) => (StatusCode::CREATED, [(LOCATION, "/login/")]).into_response(),
        Err(error)
            if error
                .as_database_error()
                .and_then(|error| error.code())
                .as_deref()
                == Some("23505") =>
        {
            api_error(StatusCode::CONFLICT, "owner is already registered")
        }
        Err(error) => {
            error!(%error, "failed to register owner");
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
    let credentials = Credentials {
        email: request.email,
        password: request.password,
    };
    let owner = match auth_session.authenticate(credentials).await {
        Ok(Some(owner)) => owner,
        Ok(None) => return api_error(StatusCode::UNAUTHORIZED, INVALID_LOGIN),
        Err(error) => {
            error!(%error, "owner authentication failed");
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error");
        }
    };

    match auth_session.login(&owner).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            error!(%error, "failed to establish owner session");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error")
        }
    }
}

async fn logout(mut auth_session: AuthSession) -> Response {
    match auth_session.logout().await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => {
            error!(%error, "failed to invalidate owner session");
            api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error")
        }
    }
}

pub async fn access_guard(
    State(pool): State<PgPool>,
    auth_session: AuthSession,
    request: Request,
    next: Next,
) -> Response {
    let resource = Resource::from_uri(request.uri());
    if resource == Resource::Asset {
        return next.run(request).await;
    }

    let authenticated = auth_session.user.is_some();
    let owner_exists = if authenticated {
        false
    } else {
        match sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM owner)")
            .fetch_one(&pool)
            .await
        {
            Ok(owner_exists) => owner_exists,
            Err(error) => {
                error!(%error, "failed to determine owner registration state");
                return api_error(StatusCode::INTERNAL_SERVER_ERROR, "server error");
            }
        }
    };

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

async fn hash_password(password: String) -> Result<String, AuthError> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(AuthError::from_display)
    })
    .await
    .map_err(AuthError::from_display)?
}

async fn verify_password(password: String, password_hash: String) -> Result<bool, AuthError> {
    tokio::task::spawn_blocking(move || {
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
                Access::Register
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
