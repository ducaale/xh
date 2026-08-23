#[cfg(feature = "http-message-signatures")]
mod auth_message_signature;
#[cfg(feature = "plugin-tests")]
mod auth_plugin;
mod compress_request_body;
mod download;
mod logging;
mod unix_socket;
mod xml;
