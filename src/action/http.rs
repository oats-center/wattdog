//! HTTP action client.

use std::time::{Duration, Instant};

use anyhow::Result;
use tracing::{info, warn};

use crate::{
    config::{ActionStepConfig, BinaryState, HttpConfig, HttpMethod},
    state::machine::ActionOutcome,
};

#[derive(Clone, Debug)]
pub struct ActionClient {
    client: reqwest::Client,
    default_method: HttpMethod,
    request_timeout: Duration,
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
        Ok(Self {
            client,
            default_method: config.method,
            request_timeout: config.timeout,
            dry_run,
        })
    }

    pub async fn execute(
        &self,
        state_name: &str,
        target: BinaryState,
        step_index: usize,
        action: &ActionStepConfig,
        deadline: Option<Instant>,
    ) -> ActionOutcome {
        match action {
            ActionStepConfig::Request { method, url } => {
                self.send(
                    state_name,
                    target,
                    step_index,
                    method.unwrap_or(self.default_method),
                    url,
                    None,
                    deadline,
                )
                .await
            }
            ActionStepConfig::WaitForJson {
                url,
                pointer,
                expected,
                ..
            } => {
                self.send(
                    state_name,
                    target,
                    step_index,
                    HttpMethod::Get,
                    url,
                    Some((pointer, expected)),
                    deadline,
                )
                .await
            }
            ActionStepConfig::Delay { .. } => ActionOutcome::Failure {
                status: None,
                error: Some("invalid_action".to_string()),
            },
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "request context is passed directly; a one-use parameter struct adds no clarity"
    )]
    async fn send(
        &self,
        state_name: &str,
        target: BinaryState,
        step_index: usize,
        method: HttpMethod,
        url: &url::Url,
        expectation: Option<(&str, &serde_json::Value)>,
        deadline: Option<Instant>,
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
                step_index,
                scheme = request_url.scheme(),
                host = request_url.host_str().unwrap_or(""),
                status = 204,
                "dry-run HTTP action"
            );
            return ActionOutcome::Success { status: 204 };
        }

        let mut request = self
            .client
            .request(reqwest_method(method), request_url.clone());
        if let Some(deadline) = deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return ActionOutcome::Failure {
                    status: None,
                    error: Some("timeout".to_string()),
                };
            }
            request = request.timeout(
                remaining
                    .min(self.request_timeout)
                    .max(Duration::from_millis(1)),
            );
        }
        if !username.is_empty() {
            request = request.basic_auth(username, password);
        }

        match request.send().await {
            Ok(response) => {
                handle_response(
                    state_name,
                    target,
                    step_index,
                    &request_url,
                    response,
                    expectation,
                )
                .await
            }
            Err(error) => {
                let category = error_category(&error);
                warn!(
                    state_name,
                    target = target.as_str(),
                    step_index,
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

async fn handle_response(
    state_name: &str,
    target: BinaryState,
    step_index: usize,
    request_url: &url::Url,
    response: reqwest::Response,
    expectation: Option<(&str, &serde_json::Value)>,
) -> ActionOutcome {
    let status = response.status().as_u16();
    if !response.status().is_success() {
        warn!(
            state_name,
            target = target.as_str(),
            step_index,
            scheme = request_url.scheme(),
            host = request_url.host_str().unwrap_or(""),
            status,
            "HTTP action failed"
        );
        return ActionOutcome::Failure {
            status: Some(status),
            error: None,
        };
    }

    if let Some((pointer, expected)) = expectation {
        let body = match response.bytes().await {
            Ok(body) => body,
            Err(error) => {
                return ActionOutcome::Failure {
                    status: Some(status),
                    error: Some(error_category(&error).to_string()),
                };
            }
        };
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) else {
            warn!(
                state_name,
                target = target.as_str(),
                step_index,
                status,
                "HTTP polling response was not valid JSON"
            );
            return ActionOutcome::Failure {
                status: Some(status),
                error: Some("invalid_json".to_string()),
            };
        };
        if json.pointer(pointer) != Some(expected) {
            info!(
                state_name,
                target = target.as_str(),
                step_index,
                status,
                "HTTP polling condition pending"
            );
            return ActionOutcome::Pending { status };
        }
    }

    info!(
        state_name,
        target = target.as_str(),
        step_index,
        scheme = request_url.scheme(),
        host = request_url.host_str().unwrap_or(""),
        status,
        "HTTP action succeeded"
    );
    ActionOutcome::Success { status }
}

fn reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
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
        config::{ActionStepConfig, BinaryState, HttpConfig, HttpMethod, WaitTimeoutPolicy},
        state::machine::ActionOutcome,
    };

    use super::ActionClient;

    type Seen = Arc<Mutex<Option<String>>>;

    #[tokio::test]
    async fn dry_run_success_does_not_hit_network() {
        let client = client(true);
        let url = Url::parse("http://192.0.2.1/action").expect("valid url");

        assert_eq!(
            client
                .execute("relay", BinaryState::On, 0, &request(url), None)
                .await,
            ActionOutcome::Success { status: 204 }
        );
    }

    #[tokio::test]
    async fn success_for_200_and_204() {
        for code in [200, 204] {
            let url = server(code, None).await;
            assert_eq!(
                client(false)
                    .execute("relay", BinaryState::On, 0, &request(url), None)
                    .await,
                ActionOutcome::Success { status: code }
            );
        }
    }

    #[tokio::test]
    async fn failure_for_500() {
        let url = server(500, None).await;

        assert_eq!(
            client(false)
                .execute("relay", BinaryState::On, 0, &request(url), None)
                .await,
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
            client(false)
                .execute("relay", BinaryState::On, 0, &request(url), None)
                .await,
            ActionOutcome::Success { status: 200 }
        ));
        assert_eq!(
            seen.lock().expect("seen").as_deref(),
            Some("POST /relay/on?token=secret|Basic dXNlcjpwYXNz")
        );
    }

    #[tokio::test]
    async fn request_step_can_override_shared_method() {
        let seen = Arc::new(Mutex::new(None));
        let url = server(204, Some(Arc::clone(&seen))).await;
        let action = ActionStepConfig::Request {
            method: Some(HttpMethod::Get),
            url,
        };

        assert!(matches!(
            client(false)
                .execute("relay", BinaryState::On, 0, &action, None)
                .await,
            ActionOutcome::Success { status: 204 }
        ));
        assert!(
            seen.lock()
                .expect("seen")
                .as_deref()
                .is_some_and(|request| request.starts_with("GET "))
        );
    }

    #[tokio::test]
    async fn json_wait_matches_exact_pointer_value() {
        let url = json_server(r#"{"result":{"leds":{"power":false}}}"#).await;
        let action = wait_for_power(url.clone(), false);

        assert_eq!(
            client(false)
                .execute("relay", BinaryState::Off, 1, &action, None)
                .await,
            ActionOutcome::Success { status: 200 }
        );

        let pending = wait_for_power(url, true);
        assert_eq!(
            client(false)
                .execute("relay", BinaryState::Off, 1, &pending, None)
                .await,
            ActionOutcome::Pending { status: 200 }
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

    fn request(url: Url) -> ActionStepConfig {
        ActionStepConfig::Request { method: None, url }
    }

    fn wait_for_power(url: Url, expected: bool) -> ActionStepConfig {
        ActionStepConfig::WaitForJson {
            url,
            pointer: "/result/leds/power".to_string(),
            expected: serde_json::Value::Bool(expected),
            poll_every: Duration::from_secs(1),
            timeout: Duration::from_secs(5),
            on_timeout: WaitTimeoutPolicy::Continue,
        }
    }

    async fn json_server(body: &'static str) -> Url {
        let app = Router::new().route(
            "/action",
            any(move || async move { (axum::http::StatusCode::OK, body) }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        Url::parse(&format!("http://{addr}/action")).expect("url")
    }

    async fn server(code: u16, seen: Option<Seen>) -> Url {
        async fn handler(
            State((code, seen)): State<(u16, Option<Seen>)>,
            method: axum::http::Method,
            uri: axum::http::Uri,
            headers: HeaderMap,
        ) -> impl IntoResponse {
            if let Some(seen) = seen {
                let auth = headers
                    .get(axum::http::header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("");
                *seen.lock().expect("seen") = Some(format!(
                    "{method} {}|{auth}",
                    uri.path_and_query().expect("path")
                ));
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
