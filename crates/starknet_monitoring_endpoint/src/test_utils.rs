use std::net::{IpAddr, SocketAddr};

use axum::body::Body;
use axum::http::Request;
use hyper::client::HttpConnector;
use hyper::Client;
use infra_utils::run_until::run_until;
use infra_utils::tracing::{CustomLogger, TraceLevel};
use metrics_exporter_prometheus::PrometheusHandle;
use thiserror::Error;
use tracing::info;

use crate::monitoring_endpoint::{ALIVE, METRICS, MONITORING_PREFIX};

// TODO(Tsabary): rename IsAliveClient to MonitoringClient.

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum MonitoringClientError {
    // #[error(transparent)]
    // ConnectionError(#[from] hyper::Error),
    #[error("Failed to connect, error details: {}", connection_error)]
    ConnectionError { connection_error: String },
    #[error("Erroneous status: {}", status)]
    ResponseStatusError { status: String },
}

/// Client for querying 'alive' status of an http server.
pub struct IsAliveClient {
    socket: SocketAddr,
    client: Client<HttpConnector>,
}

impl IsAliveClient {
    pub fn new(socket: SocketAddr) -> Self {
        let client = Client::new();
        Self { socket, client }
    }

    /// Returns 'true' if the server is 'alive'.
    async fn query_alive(&self) -> bool {
        info!("Querying the node for aliveness.");

        self.client
            .request(build_request(&self.socket.ip(), self.socket.port(), ALIVE))
            .await
            .map_or(false, |response| response.status().is_success())
    }

    /// Blocks until 'alive', up to a maximum number of query attempts. Returns 'Ok(())' if the
    /// target is alive, otherwise 'Err(())'.
    pub async fn await_alive(&self, interval: u64, max_attempts: usize) -> Result<(), ()> {
        let condition = |node_is_alive: &bool| *node_is_alive;
        let query_alive_closure = || async move { self.query_alive().await };

        let logger =
            CustomLogger::new(TraceLevel::Info, Some("Waiting for node to be alive".to_string()));

        run_until(interval, max_attempts, query_alive_closure, condition, Some(logger))
            .await
            .ok_or(())
            .map(|_| ())
    }

    pub async fn get_metrics(&self, _metric_name: &str) -> Result<(), MonitoringClientError> {
        let response = self
            .client
            .request(build_request(&self.socket.ip(), self.socket.port(), METRICS))
            .await
            .map_err(|err| MonitoringClientError::ConnectionError {
                connection_error: err.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(MonitoringClientError::ResponseStatusError {
                status: format!("{:?}", response.status()),
            });
        }

        let body_bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let body_string = String::from_utf8(body_bytes.to_vec()).unwrap();
        info!("Metrics: {:?}", body_string);
        Ok(())
    }
}

pub(crate) fn build_request(ip: &IpAddr, port: u16, method: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("http://{ip}:{port}/{MONITORING_PREFIX}/{method}").as_str())
        .body(Body::empty())
        .unwrap()
}
