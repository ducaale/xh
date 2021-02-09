extern crate dirs;

use crate::auth::Auth;
use crate::request_items::Parameter;
use crate::utils::ensure_session_dir_exists;
use crate::utils::session_filename;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::prelude::*;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Session {
    pub identifier: String,
    pub host: String,
    pub headers: Vec<Parameter>,
    pub auth: Option<Auth>,
    ht_version: String,
}

impl Session {
    pub fn new(identifier: String, host: String, headers: Vec<Parameter>, auth: Option<Auth>) -> Session {
        Session {
            identifier,
            host,
            headers,
            auth,
            ht_version: String::from(VERSION),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_string(&self)?;
        let mut config_dir = match ensure_session_dir_exists(&self.host) {
            Err(why) => panic!("couldn't get config directory: {}", why),
            Ok(dir) => dir,
        };
        config_dir.push(session_filename(&self.identifier));
        config_dir.set_extension("json");
        let path = config_dir.as_path();
        let display = path.display();
        // Open a file in write-only mode, returns `io::Result<File>`
        let mut file = match File::create(&path) {
            Err(why) => panic!("couldn't create {}: {}", display, why),
            Ok(file) => file,
        };

        // Write the json string to `file`, returns `io::Result<()>`
        file.write_all(json.as_bytes())?;
        Ok(())
    }

    pub fn load(identifier: &str, host: &str) -> std::io::Result<Option<Session>> {
        // Create a path to the desired file
        let mut config_dir = match ensure_session_dir_exists(host) {
            Err(why) => panic!("couldn't get config directory: {}", why),
            Ok(dir) => dir,
        };
        config_dir.push(session_filename(identifier));
        config_dir.set_extension("json");
        let path = config_dir.as_path();

        // Open the path in read-only mode, returns `io::Result<File>`
        let mut file = match File::open(&path) {
            Err(why) => {
                // No previous session found, it's ok.
                if why.kind() == std::io::ErrorKind::NotFound {
                    return Ok(None);
                } else {
                    return Err(why);
                }
            }
            Ok(file) => file,
        };

        // Read the file contents into a string, returns `io::Result<usize>`
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let deserialized: Session = serde_json::from_str(&contents).unwrap();
        Ok(Some(deserialized))
    }
}
