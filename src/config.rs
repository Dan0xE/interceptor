// TODO if possible, make config loading errors more granular (what we couldn't parse etc)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use ignore::WalkBuilder;
use tracing::{debug, info};

use crate::utils::file_id;

// TODO allow live reloading of config files

pub type ConfigResult<T> = Result<T, ConfigError>;

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Failed to find specified path: {0}")]
    PathNotFound(#[from] std::io::Error),
    #[error("Failed to parse ignore file: {0}")]
    IgnoreError(#[from] ignore::Error),
    #[error("Failed to serialize configuration file content: {0}")]
    SerdeError(#[from] serde_json::Error),
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(from = "RouteConfigDe")]
pub struct RouteConfig {
    pub method: Arc<str>,
    pub route: Arc<str>,
    pub response: Bytes,
    pub status: u16,
    pub headers: Option<Arc<HashMap<String, String>>>,
}

#[derive(serde::Deserialize)]
struct RouteConfigDe {
    method: String,
    route: String,
    response: String,
    status: u16,
    headers: Option<HashMap<String, String>>,
}

impl From<RouteConfigDe> for RouteConfig {
    fn from(de: RouteConfigDe) -> Self {
        RouteConfig {
            method: de.method.into(),
            route: de.route.into(),
            response: Bytes::from(de.response),
            status: de.status,
            headers: de.headers.map(Arc::new),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(from = "ConfigFileDe")]
pub struct ConfigFile {
    pub name: String,
    pub port: u16,
    pub routes: Arc<Vec<RouteConfig>>,
}

#[derive(serde::Deserialize, Default)]
struct ConfigFileDe {
    #[serde(default)]
    name: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    routes: Vec<RouteConfig>,
}

fn default_port() -> u16 {
    8080
}

impl From<ConfigFileDe> for ConfigFile {
    fn from(de: ConfigFileDe) -> Self {
        ConfigFile {
            name: de.name,
            port: de.port,
            routes: Arc::new(de.routes),
        }
    }
}

impl Default for ConfigFile {
    fn default() -> Self {
        ConfigFile {
            name: "".to_string(),
            port: 8080,
            routes: Arc::new(Vec::new()),
        }
    }
}

// TODO we need to decide if:
// - We want to autoscan the dir _and_ treat specifc files (for example with -config) ending as config files.
// - We want to scan for config files recursively or not.
// - We want the user to specify an entry config file that links to other config files / dirs (like tsconfig.json)
// - We want to support different config file formats (YAML, JSON, TOML, etc.)
// - We want to allow the user to pass in multiple config dirs/files.
//
// NOTE For now we're going the route of treating everything in the dir as config files, except those ignored by .interceptorignore and do so recursively.
// NOTE For now we're only supporting json files.

/// The file extensions we support for configuration files.
const FILE_EXTENSIONS: [&str; 1] = ["json"];

impl ConfigFile {
    // TODO separate methods (load from path, load from file?)
    /// Loads configuration files from the specified directory _or_ from the current working directory if None is provided.
    ///
    /// The user can choose to ignore certain files _or_ directories by adding them to a `.interceptorignore` file in the target directory.
    ///
    ///
    pub fn load(dir: Option<PathBuf>) -> ConfigResult<Vec<Self>> {
        let config_dir = dir.map_or_else(std::env::current_dir, Ok)?;
        let ignore_file = config_dir.join(".interceptorignore");

        let ignore_builder = WalkBuilder::new(config_dir)
            .add_custom_ignore_filename(ignore_file)
            .build();

        let mut config_paths: Vec<String> = Vec::new();

        for result in ignore_builder {
            let entry = result?;
            if entry.file_type().is_some_and(|ft| ft.is_file())
                && let Some(ext) = entry.path().extension()
                && FILE_EXTENSIONS.contains(&ext.to_string_lossy().to_lowercase().as_str())
            {
                debug!("Found config file: {:?}", entry.path());
                let path = entry.path().to_path_buf();
                config_paths.push(path.to_string_lossy().to_string());
            }
        }

        let mut configs_content: Vec<ConfigFile> = Vec::new();

        for file in config_paths {
            debug!("Processing config file: {:?}", file);
            let content = std::fs::read_to_string(&file)?;
            let mut serialized = serde_json::from_str::<ConfigFile>(&content)?;

            if serialized.name.is_empty() {
                if let Some(fname) = Path::new(&file).file_stem() {
                    serialized.name = fname.to_string_lossy().to_string();
                } else {
                    let id = file_id();
                    serialized.name = format!("unnamed_config_{}", id);
                }
            }

            let name = &serialized.name;
            let port = serialized.port;
            info!("Loaded config '{}' on port {}", name, port);
            configs_content.push(serialized);
        }

        Ok(configs_content)
    }
}
