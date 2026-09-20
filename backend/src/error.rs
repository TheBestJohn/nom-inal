use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// One error type for the whole API surface. Handlers return `ApiResult<T>` and
/// use `?` freely; the conversion into an HTTP response happens in exactly one
/// place, so every error the client sees has the same JSON shape.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    /// Forbidden, with a reason worth telling the caller. A bare "forbidden"
    /// with no reason just produces a support ticket; where explaining would
    /// leak something, `Unauthorized` is usually the right answer instead.
    #[error("{0}")]
    ForbiddenReason(String),
    #[error("{0} not found")]
    NotFound(&'static str),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    UpstreamUnavailable(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorBody {
    /// Stable machine-readable code, e.g. `not_found`.
    pub error: String,
    /// Human-readable explanation.
    pub message: String,
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::ForbiddenReason(msg.into())
    }

    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::ForbiddenReason(_) => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::UpstreamUnavailable(_) => (StatusCode::BAD_GATEWAY, "upstream_unavailable"),
            Self::Database(e) => match e {
                // Surface unique-violation as a 409 rather than a 500: it is a
                // client-correctable condition (duplicate email, duplicate date).
                sqlx::Error::Database(db) if db.is_unique_violation() => {
                    (StatusCode::CONFLICT, "conflict")
                }
                sqlx::Error::Database(db) if db.is_foreign_key_violation() => {
                    (StatusCode::BAD_REQUEST, "bad_request")
                }
                sqlx::Error::RowNotFound => (StatusCode::NOT_FOUND, "not_found"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
            },
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();

        // Never leak database/internal detail to the client, but do log it.
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self, "request failed");
            "something went wrong".to_string()
        } else {
            match &self {
                ApiError::Database(sqlx::Error::Database(_)) => {
                    "that conflicts with an existing record".to_string()
                }
                other => other.to_string(),
            }
        };

        (
            status,
            Json(ErrorBody {
                error: code.to_string(),
                message,
            }),
        )
            .into_response()
    }
}

impl From<validator::ValidationErrors> for ApiError {
    fn from(e: validator::ValidationErrors) -> Self {
        let mut out = Vec::new();
        flatten(&e, "", &mut out);
        ApiError::BadRequest(out.join("; "))
    }
}

/// Flatten validation errors into `field must be …` strings.
///
/// `ValidationErrors::field_errors()` only reports errors on the struct's own
/// fields. Errors from `#[validate(nested)]` sit under `Struct`/`List` kinds,
/// so a bad value inside a collection produced an empty message. Walking the
/// tree gives a path like `targets[0].amount` instead of nothing at all.
fn flatten(errors: &validator::ValidationErrors, path: &str, out: &mut Vec<String>) {
    use validator::ValidationErrorsKind;

    let join = |field: &str| {
        if path.is_empty() {
            field.to_string()
        } else {
            format!("{path}.{field}")
        }
    };

    for (field, kind) in errors.errors() {
        match kind {
            ValidationErrorsKind::Field(errs) => {
                let reason = errs
                    .first()
                    .and_then(|v| v.message.clone())
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "is invalid".into());
                out.push(format!("{} {}", join(field), reason));
            }
            ValidationErrorsKind::Struct(inner) => flatten(inner, &join(field), out),
            ValidationErrorsKind::List(items) => {
                for (index, inner) in items {
                    flatten(inner, &format!("{}[{}]", join(field), index), out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use validator::Validate;

    #[derive(Debug, Serialize, Validate)]
    struct Item {
        #[validate(range(min = 1.0, message = "must be at least 1"))]
        amount: f64,
    }

    #[derive(Debug, Validate)]
    struct Payload {
        #[validate(nested)]
        items: Vec<Item>,
        #[validate(length(min = 1, message = "must not be empty"))]
        name: String,
    }

    #[test]
    fn reports_errors_nested_inside_a_list() {
        let payload = Payload {
            items: vec![Item { amount: 5.0 }, Item { amount: -2.0 }],
            name: "ok".into(),
        };
        let err: ApiError = payload.validate().unwrap_err().into();
        match err {
            ApiError::BadRequest(msg) => assert_eq!(msg, "items[1].amount must be at least 1"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn still_reports_top_level_field_errors() {
        let payload = Payload {
            items: vec![],
            name: String::new(),
        };
        let err: ApiError = payload.validate().unwrap_err().into();
        match err {
            ApiError::BadRequest(msg) => assert_eq!(msg, "name must not be empty"),
            other => panic!("unexpected: {other:?}"),
        }
    }
}
