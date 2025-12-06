// TODO if possible, make config loading errors more granular (what we couldn't parse etc)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
pub struct RouteConfig {
    pub method: String,
    pub route: String,
    pub response: String,
    pub status: u16,
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, serde::Deserialize, Clone)]
#[serde(default)]
pub struct ConfigFile {
    pub name: String,
    pub port: u16,
    pub routes: Vec<RouteConfig>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        ConfigFile {
            name: "".to_string(),
            port: 8080,
            routes: Vec::new(),
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
// NOTE For now we're going the route of treating everything in the dir as config files, except those ignored by .fakeignore and do so recursively.
// NOTE For now we're only supporting json files.

/// The file extensions we support for configuration files.
const FILE_EXTENSIONS: [&str; 1] = ["json"];

// TODO async optimizations

impl ConfigFile {
    // TODO separate methods (load from path, load from file?)
    /// Loads configuration files from the specified directory _or_ from the current working directory if None is provided.
    ///
    /// The user can choose to ignore certain files _or_ directories by adding them to a `.fakeignore` file in the target directory.
    ///
    ///
    pub fn load(dir: Option<PathBuf>) -> ConfigResult<Vec<Self>> {
        let config_dir = dir.map_or_else(std::env::current_dir, Ok)?;
        let ignore_file = config_dir.join(".fakeignore");

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
                // TODO pretty sure this can be optimized
                // TODO only show name from parent (config.json, subdir/config.json, etc.)
                debug!("Found config file: {:?}", entry.path());
                let path = entry.path().to_path_buf();
                config_paths.push(path.to_string_lossy().to_string());
            }
        }

        let mut configs_content: Vec<ConfigFile> = Vec::new();

        for file in config_paths {
            debug!("Processing config file: {:?}", file);
            let content = std::fs::read_to_string(&file)?;
            let serialized = serde_json::from_str::<ConfigFile>(&content)?;
            let port = serialized.port;
            let mut name = serialized.name.clone();
            let routes = serialized.routes;

            if name.is_empty() {
                if let Some(fname) = Path::new(&file).file_stem() {
                    name = fname.to_string_lossy().to_string();
                } else {
                    let id = file_id();
                    name = format!("unnamed_config_{}", id);
                }
            }

            // TOOD clone needed?
            configs_content.push(ConfigFile {
                name: name.clone(),
                port,
                routes,
            });
            info!("Loaded config '{}' on port {}", name, port);
        }

        Ok(configs_content)
    }
}
