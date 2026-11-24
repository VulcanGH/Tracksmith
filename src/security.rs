use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::Response,
};

#[derive(Clone)]
pub struct LocalSecurity {
    pub authority: String,
    pub origin: String,
    pub token: String,
}

pub async fn guard(
    State(security): State<LocalSecurity>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = request.headers();
    if headers.get(header::HOST).and_then(|v| v.to_str().ok()) != Some(security.authority.as_str())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = headers.get(header::ORIGIN)
        && origin.to_str().ok() != Some(security.origin.as_str())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if headers
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site")
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if request.uri().path().starts_with("/api/") {
        let expected = format!("Bearer {}", security.token);
        if headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            != Some(expected.as_str())
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    let mut response = next.run(request).await;
    for (name, value) in [
        (
            "content-security-policy",
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
        (
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        ),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, middleware, routing::get};
    use tower::ServiceExt;
    #[tokio::test]
    async fn privileged_api_requires_host_origin_and_token() {
        let app = Router::new()
            .route("/api/status", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(
                LocalSecurity {
                    authority: "127.0.0.1:7476".into(),
                    origin: "http://127.0.0.1:7476".into(),
                    token: "secret".into(),
                },
                guard,
            ));
        for (host, origin, token, expected) in [
            (
                "127.0.0.1:7476",
                "http://127.0.0.1:7476",
                "Bearer secret",
                StatusCode::OK,
            ),
            (
                "evil.test:7476",
                "http://127.0.0.1:7476",
                "Bearer secret",
                StatusCode::FORBIDDEN,
            ),
            (
                "127.0.0.1:7476",
                "https://evil.test",
                "Bearer secret",
                StatusCode::FORBIDDEN,
            ),
            (
                "127.0.0.1:7476",
                "null",
                "Bearer secret",
                StatusCode::FORBIDDEN,
            ),
            (
                "127.0.0.1:7476",
                "http://127.0.0.1:7476",
                "",
                StatusCode::UNAUTHORIZED,
            ),
        ] {
            let request = Request::builder()
                .uri("/api/status")
                .header("host", host)
                .header("origin", origin)
                .header("authorization", token)
                .body(Body::empty())
                .unwrap();
            assert_eq!(
                app.clone().oneshot(request).await.unwrap().status(),
                expected
            );
        }
    }
}
