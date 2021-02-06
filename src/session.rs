extern crate dirs;

use crate::auth::Auth;
use crate::request_items::Parameter;
use crate::utils::ensure_config_dir_exists;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::prelude::*;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Session {
    pub identifier: String,
    pub headers: Vec<Parameter>,
    pub auth: Option<Auth>,
    ht_version: String,
}

impl Session {
    pub fn new(identifier: String, headers: Vec<Parameter>, auth: Option<Auth>) -> Session {
        Session {
            identifier,
            headers,
            auth,
            ht_version: String::from(VERSION),
        }
    }

    pub fn save(&self) -> std::io::Result<String> {
        let json = serde_json::to_string(&self)?;
        let identifier = &self.identifier;
        let mut home_dir = match ensure_config_dir_exists() {
            Err(why) => panic!("couldn't get config directory: {}", why),
            Ok(dir) => dir,
        };
        home_dir.push(identifier);
        home_dir.set_extension("json");
        let path = home_dir.as_path();
        let display = path.display();
        // Open a file in write-only mode, returns `io::Result<File>`
        let mut file = match File::create(&path) {
            Err(why) => panic!("couldn't create {}: {}", display, why),
            Ok(file) => file,
        };

        // Write the json string to `file`, returns `io::Result<()>`
        file.write_all(json.as_bytes())?;
        Ok(String::from(identifier))
    }

    pub fn load(identifier: &String) -> std::io::Result<Option<Session>> {
        // Create a path to the desired file
        let mut home_dir = match ensure_config_dir_exists() {
            Err(why) => panic!("couldn't get config directory: {}", why),
            Ok(dir) => dir,
        };
        home_dir.push(identifier);
        home_dir.set_extension("json");
        let path = home_dir.as_path();

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
