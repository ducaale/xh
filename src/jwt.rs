use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};

use crate::utils::config_dir;

/// JWT token information with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtToken {
    pub token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub issued_at: u64,
    pub token_type: String,
    pub scope: Option<String>,
}

impl JwtToken {
    pub fn new(token: String, token_type: Option<String>) -> Self {
        Self {
            token,
            refresh_token: None,
            expires_at: None,
            issued_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            token_type: token_type.unwrap_or_else(|| "Bearer".to_string()),
            scope: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            // Add 30 second buffer to account for clock skew
            now + 30 >= expires_at
        } else {
            false
        }
    }

    pub fn expires_within(&self, duration_seconds: u64) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            // Check if token expires within the specified duration
            expires_at <= now + duration_seconds
        } else {
            false
        }
    }
}

/// JWT token store for managing multiple tokens by name/endpoint
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct JwtTokenStore {
    tokens: HashMap<String, JwtToken>,
    #[serde(rename = "__meta__")]
    meta: JwtTokenMeta,
}

#[derive(Debug, Serialize, Deserialize)]
struct JwtTokenMeta {
    about: String,
    xh: String,
}

impl Default for JwtTokenMeta {
    fn default() -> Self {
        Self {
            about: "xh JWT token store".to_string(),
            xh: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl JwtTokenStore {
    /// Load the JWT token store from disk
    pub fn load() -> Result<Self> {
        let path = Self::get_store_path()?;
        
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read JWT token store from {:?}", path))?;
        
        let store: JwtTokenStore = serde_json::from_str(&contents)
            .with_context(|| "Failed to parse JWT token store")?;
        
        Ok(store)
    }

    /// Save the JWT token store to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::get_store_path()?;
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }

        let contents = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize JWT token store")?;
        
        fs::write(&path, contents)
            .with_context(|| format!("Failed to write JWT token store to {:?}", path))?;
        
        Ok(())
    }

    /// Get the path to the JWT token store file
    fn get_store_path() -> Result<PathBuf> {
        let config_dir = config_dir()
            .context("Could not determine config directory")?;
        Ok(config_dir.join("jwt-tokens.json"))
    }

    /// Store a JWT token with a given name
    pub fn store_token(&mut self, name: &str, token: JwtToken) {
        self.tokens.insert(name.to_string(), token);
    }

    /// Get a JWT token by name
    pub fn get_token(&self, name: &str) -> Option<&JwtToken> {
        self.tokens.get(name)
    }

    /// Get a mutable reference to a JWT token by name
    pub fn get_token_mut(&mut self, name: &str) -> Option<&mut JwtToken> {
        self.tokens.get_mut(name)
    }

    /// Remove a JWT token by name
    pub fn remove_token(&mut self, name: &str) -> Option<JwtToken> {
        self.tokens.remove(name)
    }

    /// List all stored token names
    pub fn list_tokens(&self) -> Vec<&String> {
        self.tokens.keys().collect()
    }

    /// Attempt to refresh a token using a default refresh URL pattern
    /// This is a simple implementation that assumes refresh URL is the same as original URL
    pub fn refresh_token_if_needed(&mut self, name: &str, client: &Client, refresh_url: Option<&str>) -> Result<bool> {
        let needs_refresh = if let Some(token) = self.get_token(name) {
            token.is_expired() || token.expires_within(300) // 5 minutes
        } else {
            return Ok(false);
        };

        if !needs_refresh {
            return Ok(false);
        }

        let token = self.get_token(name).ok_or_else(|| anyhow!("Token not found"))?;
        
        if token.refresh_token.is_none() {
            return Ok(false); // Can't refresh without refresh token
        }

        if let Some(refresh_url) = refresh_url {
            let url: reqwest::Url = refresh_url.parse()
                .context("Invalid refresh URL")?;
            
            let token_mut = self.get_token_mut(name).unwrap();
            match refresh_token(client, token_mut, &url, None, None) {
                Ok(()) => {
                    log::info!("Successfully refreshed JWT token '{}'", name);
                    Ok(true)
                }
                Err(e) => {
                    log::warn!("Failed to refresh JWT token '{}': {}", name, e);
                    Ok(false)
                }
            }
        } else {
            Ok(false)
        }
    }
}

/// JWT token request configuration
#[derive(Debug, Clone)]
pub struct JwtTokenRequest {
    pub url: Url,
    pub method: Method,
    pub username: Option<String>,
    pub password: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub scope: Option<String>,
    pub grant_type: String,
    pub additional_params: HashMap<String, String>,
}

impl Default for JwtTokenRequest {
    fn default() -> Self {
        Self {
            url: "http://localhost/oauth/token".parse().unwrap(),
            method: Method::POST,
            username: None,
            password: None,
            client_id: None,
            client_secret: None,
            scope: None,
            grant_type: "password".to_string(),
            additional_params: HashMap::new(),
        }
    }
}

impl JwtTokenRequest {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            ..Default::default()
        }
    }

    pub fn with_credentials(mut self, username: String, password: String) -> Self {
        self.username = Some(username);
        self.password = Some(password);
        self
    }

    pub fn with_client_credentials(mut self, client_id: String, client_secret: String) -> Self {
        self.client_id = Some(client_id);
        self.client_secret = Some(client_secret);
        self.grant_type = "client_credentials".to_string();
        self
    }

    pub fn with_scope(mut self, scope: String) -> Self {
        self.scope = Some(scope);
        self
    }

    pub fn with_grant_type(mut self, grant_type: String) -> Self {
        self.grant_type = grant_type;
        self
    }

    #[cfg(test)]
    pub fn with_param(mut self, key: String, value: String) -> Self {
        self.additional_params.insert(key, value);
        self
    }

    /// Request a JWT token synchronously using blocking client
    pub fn request_token_sync(&self, client: &Client) -> Result<JwtToken> {
        // Check if we should use JSON format (for password grant with username/password)
        let use_json = self.grant_type == "password" && self.username.is_some() && self.password.is_some();
        
        let response = if use_json {
            // Use JSON format for login endpoints that expect email/password
            let mut json_data = serde_json::Map::new();
            
            if let (Some(username), Some(password)) = (&self.username, &self.password) {
                json_data.insert("email".to_string(), serde_json::Value::String(username.clone()));
                json_data.insert("password".to_string(), serde_json::Value::String(password.clone()));
            }
            
            // Add any additional parameters to JSON
            for (key, value) in &self.additional_params {
                json_data.insert(key.clone(), serde_json::Value::String(value.clone()));
            }
            
            client
                .request(self.method.clone(), self.url.clone())
                .header(CONTENT_TYPE, "application/json")
                .json(&json_data)
                .send()?
        } else {
            // Use form format for OAuth2 token endpoints
            let mut form_data = HashMap::new();
            
            form_data.insert("grant_type".to_string(), self.grant_type.clone());
            
            if let (Some(username), Some(password)) = (&self.username, &self.password) {
                form_data.insert("username".to_string(), username.clone());
                form_data.insert("password".to_string(), password.clone());
            }
            
            if let (Some(client_id), Some(client_secret)) = (&self.client_id, &self.client_secret) {
                form_data.insert("client_id".to_string(), client_id.clone());
                form_data.insert("client_secret".to_string(), client_secret.clone());
            }
            
            if let Some(scope) = &self.scope {
                form_data.insert("scope".to_string(), scope.clone());
            }
            
            // Add any additional parameters
            for (key, value) in &self.additional_params {
                form_data.insert(key.clone(), value.clone());
            }

            client
                .request(self.method.clone(), self.url.clone())
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .form(&form_data)
                .send()?
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow!(
                "Failed to get JWT token: {} - {}",
                status,
                body
            ));
        }

        let token_response: TokenResponse = response.json()
            .context("Failed to parse token response as JSON")?;

        let mut token = JwtToken::new(
            token_response.access_token,
            Some(token_response.token_type.unwrap_or_else(|| "Bearer".to_string())),
        );
        
        token.refresh_token = token_response.refresh_token;
        token.scope = token_response.scope.or_else(|| self.scope.clone());
        
        if let Some(expires_in) = token_response.expires_in {
            token.expires_at = Some(token.issued_at + expires_in as u64);
        }

        Ok(token)
    }
}

/// OAuth2/JWT token response structure
#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(alias = "token")]
    access_token: String,
    token_type: Option<String>,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
    scope: Option<String>,
}

/// Refresh a JWT token using its refresh token
pub fn refresh_token(client: &Client, token: &mut JwtToken, refresh_url: &Url, client_id: Option<&str>, client_secret: Option<&str>) -> Result<()> {
    let refresh_token = token.refresh_token.as_ref()
        .ok_or_else(|| anyhow!("No refresh token available"))?;
    
    let mut form_data = HashMap::new();
    form_data.insert("grant_type".to_string(), "refresh_token".to_string());
    form_data.insert("refresh_token".to_string(), refresh_token.clone());
    
    if let (Some(client_id), Some(client_secret)) = (client_id, client_secret) {
        form_data.insert("client_id".to_string(), client_id.to_string());
        form_data.insert("client_secret".to_string(), client_secret.to_string());
    }

    let response = client
        .post(refresh_url.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .form(&form_data)
        .send()?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(anyhow!(
            "Failed to refresh JWT token: {} - {}",
            status,
            body
        ));
    }

    let token_response: TokenResponse = response.json()
        .context("Failed to parse refresh token response as JSON")?;

    // Update the token with new values
    token.token = token_response.access_token;
    token.issued_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    if let Some(new_refresh_token) = token_response.refresh_token {
        token.refresh_token = Some(new_refresh_token);
    }
    
    if let Some(expires_in) = token_response.expires_in {
        token.expires_at = Some(token.issued_at + expires_in as u64);
    }
    
    if let Some(token_type) = token_response.token_type {
        token.token_type = token_type;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_token_creation() {
        let token = JwtToken::new("test-token".to_string(), Some("Bearer".to_string()));
        assert_eq!(token.token, "test-token");
        assert_eq!(token.token_type, "Bearer");
        assert!(!token.is_expired());
    }

    #[test]
    fn test_jwt_token_expiry() {
        let mut token = JwtToken::new("test-token".to_string(), Some("Bearer".to_string()));
        token.expires_at = Some(1); // Expired timestamp
        assert!(token.is_expired());
    }

    #[test]
    fn test_jwt_token_expires_within() {
        let mut token = JwtToken::new("test-token".to_string(), Some("Bearer".to_string()));
        
        // Set expiry to 10 minutes from now
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        token.expires_at = Some(now + 600); // 10 minutes
        
        // Should expire within 15 minutes (900 seconds)
        assert!(token.expires_within(900));
        
        // Should not expire within 5 minutes (300 seconds)
        assert!(!token.expires_within(300));
        
        // Should not be expired yet
        assert!(!token.is_expired());
    }

    #[test]
    fn test_jwt_token_store() {
        let mut store = JwtTokenStore::default();
        let token = JwtToken::new("test-token".to_string(), Some("Bearer".to_string()));
        
        store.store_token("test", token);
        assert!(store.get_token("test").is_some());
        assert_eq!(store.get_token("test").unwrap().token, "test-token");
        
        let removed = store.remove_token("test");
        assert!(removed.is_some());
        assert!(store.get_token("test").is_none());
    }

    #[test]
    fn test_jwt_token_request_builder() {
        let url: Url = "https://example.com/oauth/token".parse().unwrap();
        let request = JwtTokenRequest::new(url.clone())
            .with_credentials("user".to_string(), "pass".to_string())
            .with_scope("read write".to_string())
            .with_param("custom".to_string(), "value".to_string());
        
        assert_eq!(request.url, url);
        assert_eq!(request.username, Some("user".to_string()));
        assert_eq!(request.password, Some("pass".to_string()));
        assert_eq!(request.scope, Some("read write".to_string()));
        assert_eq!(request.additional_params.get("custom"), Some(&"value".to_string()));
    }
}
