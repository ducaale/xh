use std::fs;
use tempfile::TempDir;

// Basic JWT integration test to verify CLI integration
#[test]
fn test_jwt_list_empty() {
    // Create a temporary directory for JWT token storage
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("xh");
    fs::create_dir_all(&config_path).unwrap();
    
    // Set XH_CONFIG_DIR environment variable to use our temporary directory
    std::env::set_var("XH_CONFIG_DIR", config_path.to_str().unwrap());
    
    // Test that listing JWT tokens with no stored tokens works
    let output = std::process::Command::new("cargo")
        .args(["run", "--", "--jwt-list"])
        .current_dir("/home/nelly/Projects/xh")
        .output();
    
    // The command should succeed and output "No JWT tokens stored."
    if let Ok(output) = output {
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("No JWT tokens stored."));
    }
    
    // Clean up environment variable
    std::env::remove_var("XH_CONFIG_DIR");
}

#[test]
fn test_jwt_show_not_found() {
    // Test showing a non-existent JWT token
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("xh");
    fs::create_dir_all(&config_path).unwrap();
    
    std::env::set_var("XH_CONFIG_DIR", config_path.to_str().unwrap());
    
    let output = std::process::Command::new("cargo")
        .args(["run", "--", "--jwt-show", "nonexistent"])
        .current_dir("/home/nelly/Projects/xh")
        .output();
    
    if let Ok(output) = output {
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("JWT token 'nonexistent' not found"));
    }
    
    std::env::remove_var("XH_CONFIG_DIR");
}
