//! HTTP action client.

use anyhow::Result;
use tracing::{info, warn};

use crate::{
    config::{BinaryState, HttpConfig, HttpMethod},
    state::machine::ActionOutcome,
};

#[derive(Clone, Debug)]
pub struct ActionClient {
    client: reqwest::Client,
    method: reqwest::Method,
    dry_run: bool,
}

impl ActionClient {
    /// Builds a reusable HTTP action client.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying reqwest client cannot be built.
    pub fn new(config: &HttpConfig, dry_run: bool) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .danger_accept_invalid_certs(config.allow_invalid_certs)
            .build()?;
        let method = match config.method {
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
        };
        Ok(Self {
            client,
            method,
            dry_run,
        })
    }

    pub async fn send(
        &self,
        state_name: &str,
        target: BinaryState,
        url: &url::Url,
    ) -> ActionOutcome {
        let username = url.username().to_string();
        let password = url.password().map(str::to_string);
        let mut request_url = url.clone();
        if request_url.set_username("").is_err() || request_url.set_password(None).is_err() {
            return ActionOutcome::Failure {
                status: None,
                error: Some("invalid_url".to_string()),
            };
        }

        if self.dry_run {
            info!(
                state_name,
                target = target.as_str(),
                scheme = request_url.scheme(),
                host = request_url.host_str().unwrap_or(""),
                status = 204,
                "dry-run HTTP action"
            );
            return ActionOutcome::Success { status: 204 };
        }

        let mut request = self
            .client
            .request(self.method.clone(), request_url.clone());
        if !username.is_empty() {
            request = request.basic_auth(username, password);
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                if response.status().is_success() {
                    info!(
                        state_name,
                        target = target.as_str(),
                        scheme = request_url.scheme(),
                        host = request_url.host_str().unwrap_or(""),
                        status,
                        "HTTP action succeeded"
                    );
                    ActionOutcome::Success { status }
                } else {
                    warn!(
                        state_name,
                        target = target.as_str(),
                        scheme = request_url.scheme(),
                        host = request_url.host_str().unwrap_or(""),
                        status,
                        "HTTP action failed"
                    );
                    ActionOutcome::Failure {
                        status: Some(status),
                        error: None,
                    }
                }
            }
            Err(error) => {
                let category = error_category(&error);
                warn!(
                    state_name,
                    target = target.as_str(),
                    scheme = request_url.scheme(),
                    host = request_url.host_str().unwrap_or(""),
                    error = category,
                    "HTTP action failed"
                );
                ActionOutcome::Failure {
                    status: None,
                    error: Some(category.to_string()),
                }
            }
        }
    }
}

fn error_category(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timeout"
    } else if error.is_connect() {
        "connect"
    } else if error.is_request() {
        "request"
    } else {
        "network"
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use axum::{Router, extract::State, http::HeaderMap, response::IntoResponse, routing::any};
    use tokio::net::TcpListener;
    use url::Url;

    use crate::{
        config::{BinaryState, HttpConfig, HttpMethod},
        state::machine::ActionOutcome,
    };

    use super::ActionClient;

    type Seen = Arc<Mutex<Option<String>>>;

    #[tokio::test]
    async fn dry_run_success_does_not_hit_network() {
        let client = client(true);
        let url = Url::parse("http://192.0.2.1/action").expect("valid url");

        assert_eq!(
            client.send("relay", BinaryState::On, &url).await,
            ActionOutcome::Success { status: 204 }
        );
    }

    #[tokio::test]
    async fn success_for_200_and_204() {
        for code in [200, 204] {
            let url = server(code, None).await;
            assert_eq!(
                client(false).send("relay", BinaryState::On, &url).await,
                ActionOutcome::Success { status: code }
            );
        }
    }

    #[tokio::test]
    async fn failure_for_500() {
        let url = server(500, None).await;

        assert_eq!(
            client(false).send("relay", BinaryState::On, &url).await,
            ActionOutcome::Failure {
                status: Some(500),
                error: None,
            }
        );
    }

    #[tokio::test]
    async fn userinfo_becomes_basic_auth_and_path_query_are_preserved() {
        let seen = Arc::new(Mutex::new(None));
        let mut url = server(200, Some(Arc::clone(&seen))).await;
        url.set_path("/relay/on");
        url.set_query(Some("token=secret"));
        url.set_username("user").expect("set username");
        url.set_password(Some("pass")).expect("set password");

        assert!(matches!(
            client(false).send("relay", BinaryState::On, &url).await,
            ActionOutcome::Success { status: 200 }
        ));
        assert_eq!(
            seen.lock().expect("seen").as_deref(),
            Some("/relay/on?token=secret|Basic dXNlcjpwYXNz")
        );
    }

    fn client(dry_run: bool) -> ActionClient {
        ActionClient::new(
            &HttpConfig {
                method: HttpMethod::Post,
                timeout: Duration::from_secs(1),
                retry_initial: Duration::from_millis(1),
                retry_max: Duration::from_millis(1),
                require_https: false,
                allow_invalid_certs: false,
            },
            dry_run,
        )
        .expect("client")
    }

    async fn server(code: u16, seen: Option<Seen>) -> Url {
        async fn handler(
            State((code, seen)): State<(u16, Option<Seen>)>,
            uri: axum::http::Uri,
            headers: HeaderMap,
        ) -> impl IntoResponse {
            if let Some(seen) = seen {
                let auth = headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("");
                *seen.lock().expect("seen") =
                    Some(format!("{}|{auth}", uri.path_and_query().expect("path")));
            }
            axum::http::StatusCode::from_u16(code).expect("status")
        }

        let app = Router::new()
            .route("/{*path}", any(handler))
            .with_state((code, seen));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        Url::parse(&format!("http://{addr}/action")).expect("url")
    }
}
