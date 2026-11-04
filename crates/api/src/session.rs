use std::time::Duration;

use axum::http::{HeaderMap, header};
use conta_verde_application::AuthError;

use crate::error::ApiError;

const COOKIE_NAME: &str = "cv_session";

/// HttpOnly keeps the token away from scripts; SameSite=Lax stops browsers from attaching it to
/// cross-site POSTs, which is the CSRF defense for this JSON API.
pub(crate) fn session_cookie(token: &str, lifetime: Duration, secure: bool) -> String {
    cookie(token, lifetime.as_secs(), secure)
}

pub(crate) fn clearing_cookie(secure: bool) -> String {
    cookie("", 0, secure)
}

fn cookie(value: &str, max_age_secs: u64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{COOKIE_NAME}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age_secs}{secure}")
}

pub(crate) fn require_token(headers: &HeaderMap) -> Result<&str, ApiError> {
    token_from(headers).ok_or_else(|| ApiError::Application(AuthError::Unauthenticated.into()))
}

pub(crate) fn token_from(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, value)| *name == COOKIE_NAME && !value.is_empty())
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn finds_the_session_among_other_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("theme=dark; cv_session=abc123; other=1"),
        );
        assert_eq!(token_from(&headers), Some("abc123"));
    }

    #[test]
    fn ignores_missing_and_empty_sessions() {
        let mut headers = HeaderMap::new();
        assert_eq!(token_from(&headers), None);
        headers.insert(header::COOKIE, HeaderValue::from_static("cv_session="));
        assert_eq!(token_from(&headers), None);
    }

    #[test]
    fn cookie_flags_follow_configuration() {
        let plain = session_cookie("t", Duration::from_secs(60), false);
        assert!(plain.contains("HttpOnly") && plain.contains("SameSite=Lax"));
        assert!(!plain.contains("Secure"));
        assert!(session_cookie("t", Duration::from_secs(60), true).contains("; Secure"));
        assert!(clearing_cookie(false).contains("Max-Age=0"));
    }
}
