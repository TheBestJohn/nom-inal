use argon2::password_hash::{
    rand_core::{OsRng, RngCore},
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: the user id.
    pub sub: Uuid,
    pub email: String,
    pub exp: i64,
    pub iat: i64,
}

/// How the caller proved who they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential {
    /// A JWT from signing in with a password.
    Session,
    /// A long-lived API key.
    ApiKey,
}

/// Every API key token starts with this, which is what lets one `Authorization:
/// Bearer` header carry either kind of credential without the client having to
/// say which it is holding.
pub const API_KEY_PREFIX: &str = "nomi_";

/// An authenticated caller. Any handler that takes this as an argument is
/// automatically protected: the extractor rejects the request with 401 before
/// the handler body ever runs, so authorization can't be forgotten.
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser {
    pub id: Uuid,
    pub is_admin: bool,
    pub credential: Credential,
    /// False for a read-only API key. Always true for a session.
    pub can_write: bool,
}

/// Resolve whoever the request headers say is calling, without yet asking
/// what they are allowed to do.
///
/// Split from the extractor so the MCP endpoint can share it: an MCP call is
/// always an HTTP POST, so the method-based scope rule below would refuse a
/// read-only key before it had listed a single tool. The scope is still
/// enforced — every tool call is dispatched back through the API with the
/// same header, where the extractor applies it to the real method.
pub async fn authenticate_headers(
    state: &AppState,
    headers: &axum::http::HeaderMap,
) -> Result<CurrentUser, ApiError> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| {
            h.strip_prefix("Bearer ")
                .or_else(|| h.strip_prefix("bearer "))
        })
        // `X-API-Key` is accepted as well, because that is the header most
        // scripting clients and dashboards expect for a static key.
        .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()))
        .ok_or(ApiError::Unauthorized)?;

    let token = header.trim();

    if token.starts_with(API_KEY_PREFIX) {
        authenticate_api_key(state, token).await
    } else {
        authenticate_session(state, token).await
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = authenticate_headers(state, &parts.headers).await?;

        // One choke point for scope enforcement, rather than a check repeated
        // in every mutating handler and forgotten in the next one added. Safe
        // methods are the read surface by definition, so the mapping from
        // scope to permission needs no route table to stay correct.
        if !user.can_write && !parts.method.is_safe() {
            return Err(ApiError::forbidden(
                "this API key is read-only; create a key with the 'write' scope to make changes",
            ));
        }

        Ok(user)
    }
}

/// Resolve a JWT, then confirm the account still exists and is still enabled.
///
/// The token alone would be enough to identify the user, and checking it
/// against the database costs a primary-key lookup on every request. That is
/// the price of two properties worth having: disabling an account takes effect
/// at once rather than whenever the holder's token happens to expire, and a
/// promotion to administrator applies without making them sign in again.
async fn authenticate_session(state: &AppState, token: &str) -> Result<CurrentUser, ApiError> {
    let claims = verify_token(token, &state.config.jwt_secret)?;

    let row: Option<(bool, Option<chrono::DateTime<Utc>>)> =
        sqlx::query_as("SELECT is_admin, disabled_at FROM users WHERE id = $1")
            .bind(claims.sub)
            .fetch_optional(&state.db)
            .await?;
    let (is_admin, disabled_at) = row.ok_or(ApiError::Unauthorized)?;
    if disabled_at.is_some() {
        return Err(ApiError::forbidden("this account has been disabled"));
    }

    Ok(CurrentUser {
        id: claims.sub,
        is_admin,
        credential: Credential::Session,
        can_write: true,
    })
}

#[derive(sqlx::FromRow)]
struct ApiKeyPrincipal {
    key_id: Uuid,
    user_id: Uuid,
    scopes: Vec<String>,
    is_admin: bool,
    disabled_at: Option<chrono::DateTime<Utc>>,
}

/// Resolve an API key by the digest of the presented token.
async fn authenticate_api_key(state: &AppState, token: &str) -> Result<CurrentUser, ApiError> {
    let digest = hash_api_key(token);

    // Expiry and revocation are filtered in SQL rather than compared in Rust so
    // that a retired key is indistinguishable from one that never existed: both
    // return no row and produce the same 401.
    let row: Option<ApiKeyPrincipal> = sqlx::query_as(
        "SELECT k.id AS key_id, k.user_id, k.scopes, u.is_admin, u.disabled_at
         FROM api_keys k
         JOIN users u ON u.id = k.user_id
         WHERE k.token_hash = $1
           AND k.revoked_at IS NULL
           AND (k.expires_at IS NULL OR k.expires_at > now())",
    )
    .bind(&digest)
    .fetch_optional(&state.db)
    .await?;

    let ApiKeyPrincipal {
        key_id,
        user_id,
        scopes,
        is_admin,
        disabled_at,
    } = row.ok_or(ApiError::Unauthorized)?;
    if disabled_at.is_some() {
        return Err(ApiError::forbidden("this account has been disabled"));
    }

    // "Last used" is for spotting a key nobody needs any more, so minute-level
    // accuracy is pointless and a write on every single request is not. The
    // predicate keeps a busy key to one update every five minutes.
    let _ = sqlx::query(
        "UPDATE api_keys SET last_used_at = now()
         WHERE id = $1
           AND (last_used_at IS NULL OR last_used_at < now() - interval '5 minutes')",
    )
    .bind(key_id)
    .execute(&state.db)
    .await;

    Ok(CurrentUser {
        id: user_id,
        is_admin,
        credential: Credential::ApiKey,
        can_write: scopes.iter().any(|s| s == "write"),
    })
}

/// A caller who signed in with a password, not an API key.
///
/// Guards the endpoints that manage credentials and the instance itself. A key
/// that leaks is bad; a key that can mint more keys, or promote its holder to
/// administrator, turns a leak into a permanent foothold. Requiring the
/// interactive credential for those actions keeps the blast radius of a stolen
/// key to the data it was scoped for.
#[derive(Debug, Clone, Copy)]
pub struct SessionUser(pub CurrentUser);

impl std::ops::Deref for SessionUser {
    type Target = CurrentUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for SessionUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = CurrentUser::from_request_parts(parts, state).await?;
        if user.credential != Credential::Session {
            return Err(ApiError::forbidden(
                "this action requires signing in; API keys cannot manage credentials",
            ));
        }
        Ok(SessionUser(user))
    }
}

/// An administrator, signed in interactively.
#[derive(Debug, Clone, Copy)]
pub struct AdminUser(pub CurrentUser);

impl std::ops::Deref for AdminUser {
    type Target = CurrentUser;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = SessionUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(ApiError::forbidden("administrator access required"));
        }
        Ok(AdminUser(user.0))
    }
}

/// Mint a new API key: the token to show the caller once, and the digest and
/// display prefix to store.
pub fn generate_api_key() -> (String, String, String) {
    use base64::Engine;

    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = format!(
        "{API_KEY_PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    );

    let digest = hash_api_key(&token);
    // Enough of the token to tell two keys apart in a list, and far too little
    // to be worth anything: 8 of 43 random characters.
    let prefix: String = token.chars().take(API_KEY_PREFIX.len() + 8).collect();
    (token, digest, prefix)
}

/// SHA-256, hex. See the note on `api_keys.token_hash` for why this is not
/// Argon2 even though passwords in the same schema are.
pub fn hash_api_key(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write;
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

pub fn hash_password(plain: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("hashing failed: {e}")))
}

pub fn verify_password(plain: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

pub fn issue_token(
    user_id: Uuid,
    email: &str,
    secret: &str,
    ttl_hours: i64,
) -> Result<(String, i64), ApiError> {
    let now = Utc::now();
    let exp = (now + Duration::hours(ttl_hours)).timestamp();
    let claims = Claims {
        sub: user_id,
        email: email.to_string(),
        exp,
        iat: now.timestamp(),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("token encoding failed: {e}")))?;
    Ok((token, ttl_hours * 3600))
}

pub fn verify_token(token: &str, secret: &str) -> Result<Claims, ApiError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| ApiError::Unauthorized)
}
