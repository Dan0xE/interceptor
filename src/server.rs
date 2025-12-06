use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use axum::{
    Router,
    body::Body,
    http::{Error as HttpError, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{MethodFilter, on},
};
use tracing::{debug, info, warn};

use crate::config::ConfigFile;

// TODO port collision detection?
// TODO if yes, granually? (meaning yes, two configs can share the same port, _but_ not the same routes)

#[derive(thiserror::Error, Debug)]
pub enum MockServerError {
    #[error("No instances are currently running.")]
    NoInstancesRunning,
    #[error("No matching instance found for the given input: {0}")]
    NoMatchingInstance(String), // String is okay here as we can also use it for ints
    #[error("Instance reported an error: {0}")]
    InstanceError(#[from] ServerInstanceError),
    #[error("Axum error occurred: {0}")]
    AxumError(#[from] axum::Error),
}

pub type MockServerResult<T> = Result<T, MockServerError>;

#[derive(Debug)]
pub struct MockServer {
    configs: Vec<ConfigFile>,
    pub instances: Option<Vec<ServerInstance>>,
}

#[derive(thiserror::Error, Debug)]
pub enum ServerInstanceError {
    #[error("Failed to bind to the specified port: {0}")]
    PortBindError(#[from] std::io::Error),
    #[error("Failed to build a response: {0}")]
    ResponseBuildError(#[from] HttpError),
    #[error("Invalid header name: {0}")]
    InvalidHeaderName(#[from] header::InvalidHeaderName),
    #[error("Invalid header value: {0}")]
    InvalidHeaderValue(#[from] header::InvalidHeaderValue),
}

impl IntoResponse for ServerInstanceError {
    fn into_response(self) -> Response {
        let status = match &self {
            ServerInstanceError::PortBindError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ServerInstanceError::ResponseBuildError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ServerInstanceError::InvalidHeaderName(_) => StatusCode::BAD_REQUEST,
            ServerInstanceError::InvalidHeaderValue(_) => StatusCode::BAD_REQUEST,
        };
        (status, self.to_string()).into_response()
    }
}

pub type ServerInstanceResult<T> = Result<T, ServerInstanceError>;

#[derive(Debug)]
pub struct ServerInstance {
    id: usize,
    config: ConfigFile,
    t_handle: Option<tokio::task::JoinHandle<ServerInstanceResult<()>>>,
}

impl ServerInstance {
    pub fn create(id: usize, config: ConfigFile) -> Self {
        ServerInstance {
            id,
            config,
            t_handle: None,
        }
    }

    // TODO do we really want to give back &mut Self here?
    pub async fn start(&mut self) -> ServerInstanceResult<&mut Self> {
        // This should basically bind to the port and start
        let port = self.config.port;
        let config = self.config.routes.clone(); // TODO clone needed?
        let handle: JoinHandle<ServerInstanceResult<()>> = tokio::spawn(async move {
            let mut router = Router::new();

            // TODO we should allow specifying the IP itself
            let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

            for route in config.iter() {
                let method = route.method.to_uppercase();
                let route_path = route.route.clone();
                let response_body = route.response.clone();
                let status_code = route.status;
                let headers = route.headers.clone();

                // TODO this is getting clunky, refactor
                let path = route_path.clone();

                let handler = move || {
                    let body = response_body.clone();
                    let headers = headers.clone();
                    async move {
                        let mut response = Response::builder()
                            .status(status_code)
                            .body(Body::from(body))?;

                        if let Some(hdrs) = headers {
                            for (key, value) in hdrs.iter() {
                                response.headers_mut().insert(
                                    HeaderName::from_bytes(key.as_bytes())?,
                                    HeaderValue::from_str(value)?,
                                );
                            }
                        }

                        info!(
                            "Handled request for route '{}' with status {}",
                            path, status_code
                        );

                        Ok::<_, ServerInstanceError>(response)
                    }
                };

                let method_filter = match method.as_str() {
                    "GET" => MethodFilter::GET,
                    "POST" => MethodFilter::POST,
                    "PUT" => MethodFilter::PUT,
                    "DELETE" => MethodFilter::DELETE,
                    "PATCH" => MethodFilter::PATCH,
                    "HEAD" => MethodFilter::HEAD,
                    "OPTIONS" => MethodFilter::OPTIONS,
                    _ => {
                        warn!(
                            "Unsupported HTTP method '{}' for route '{}', skipping.",
                            method, route_path
                        );
                        continue;
                    }
                };

                router = router.route(&route_path, on(method_filter, handler));
            }

            axum::serve(listener, router).await?;

            // TODO what do we do if the spawn fails? more clarity
            Ok(())
        });

        self.t_handle = Some(handle);

        Ok(self)
    }

    pub async fn stop(&self) -> ServerInstanceResult<()> {
        info!(
            "Stopping server instance {} on port {}",
            self.config.name, self.config.port
        );

        if let Some(h) = self.t_handle.as_ref() {
            debug!("Aborting task handle for instance {}", self.id);
            h.abort()
        }

        Ok(())
    }
}

impl MockServer {
    pub fn new(configs: Vec<ConfigFile>) -> Self {
        MockServer {
            configs,
            instances: None,
        }
    }

    pub async fn start(&mut self, id: usize, config: &ConfigFile) -> MockServerResult<()> {
        let mut instance = ServerInstance::create(id, config.clone());
        instance.start().await?;

        self.instances.get_or_insert(Vec::new()).push(instance);

        info!(
            "Started instance '{}' on port {} with ID {}",
            config.name, config.port, id
        );

        Ok(())
    }

    pub async fn stop(&mut self, id: usize) -> MockServerResult<()> {
        let Some(instances) = &mut self.instances else {
            return Err(MockServerError::NoInstancesRunning);
        };

        // TODO could also search by config.name
        if let Some(pos) = instances.iter().position(|inst| inst.id == id) {
            let instance = &instances[pos];
            instance.stop().await?;
            instances.remove(pos);

            Ok(())
        } else {
            Err(MockServerError::NoMatchingInstance(format!("ID: {id}",)))
        }
    }

    pub async fn start_all(&mut self) -> MockServerResult<()> {
        let configs: Vec<_> = self.configs.iter().cloned().enumerate().collect(); // TODO clone needed?
        for (id, config) in configs {
            self.start(id, &config).await?;
        }

        Ok(())
    }

    pub async fn stop_all(&mut self) -> MockServerResult<()> {
        // TODO can be optimized
        let Some(ref instances) = self.instances else {
            return Err(MockServerError::NoInstancesRunning);
        };

        let ids: Vec<_> = instances.iter().map(|inst| inst.id).collect::<Vec<usize>>();

        for id in ids {
            self.stop(id).await?;
        }

        Ok(())
    }

    pub async fn health(&mut self) -> MockServerResult<()> {
        let Some(instances) = &mut self.instances else {
            warn!("No instances to watch.");
            return Err(MockServerError::NoInstancesRunning);
        };

        instances.retain(|instance| {
            let id = instance.id;
            let Some(ref handle) = instance.t_handle else {
                warn!("Handle returned None for Instance ({id}), removing from list");
                return false;
            };

            if handle.is_finished() {
                warn!("Instance ({id}) has died, removing from list");
                return false;
            }

            true
        });

        if instances.is_empty() {
            warn!("No instances to watch");
            return Err(MockServerError::NoInstancesRunning);
        };

        Ok(())
    }
}
