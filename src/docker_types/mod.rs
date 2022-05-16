use serde::{Deserialize, Serialize};

pub mod config;
pub mod manifest;
pub mod pusher_config;


#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
pub struct PathPair {
    pub short_path: String,
    pub path: String
}