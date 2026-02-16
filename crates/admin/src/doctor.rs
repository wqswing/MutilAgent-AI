use serde::Serialize;
use std::sync::Arc;
use axum::{extract::State, Json, http::StatusCode};
// Traits are brought in scope via AdminState if needed, or keeping them for trait bounds
use crate::AdminState;
use std::time::Instant;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<CheckResult>,
    pub overall_status: String,
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub category: String,
    pub name: String,
    pub status: String, // "pass", "fail", "warn"
    pub message: Option<String>,
    pub latency_ms: Option<u64>,
}

impl CheckResult {
    fn pass(category: &str, name: &str, latency: Option<u64>) -> Self {
        Self {
            category: category.to_string(),
            name: name.to_string(),
            status: "pass".to_string(),
            message: None,
            latency_ms: latency,
        }
    }

    fn fail(category: &str, name: &str, message: String) -> Self {
        Self {
            category: category.to_string(),
            name: name.to_string(),
            status: "fail".to_string(),
            message: Some(message),
            latency_ms: None,
        }
    }
}

pub async fn check_all(
    State(state): State<Arc<AdminState>>,
) -> Json<DoctorReport> {
    let mut checks = Vec::new();

    // 1. Check LLM Connectivity
    // Retrieve providers via read lock
    let providers = state.providers.read().await;
    if providers.is_empty() {
         checks.push(CheckResult {
            category: "LLM".to_string(),
            name: "Configuration".to_string(),
            status: "warn".to_string(),
            message: Some("No LLM providers configured".to_string()),
            latency_ms: None,
        });
    } else {
        // Ping first provider as a sample check (checking all might be slow)
        // Or check all in parallel? For now, check all (up to 3)
        for provider in providers.iter().take(3) {
            let start = Instant::now();
            let client = reqwest::Client::new();
            
            // Decrypt key
            let api_key = match state.secrets.retrieve(&provider.api_key_id).await {
                Ok(Some(k)) => k,
                _ => {
                    checks.push(CheckResult::fail("LLM", &provider.id, "Failed to retrieve API key".to_string()));
                    continue;
                }
            };

            // Simple ping to base_url (typically /v1/models or similar)
            // Heuristic: append /models if not present?
            // Providers usually store base_url like "https://api.openai.com/v1"
            let url = format!("{}/models", provider.base_url.trim_end_matches('/'));
            
            let res = client.get(&url)
                .bearer_auth(api_key)
                .timeout(std::time::Duration::from_secs(3))
                .send()
                .await;

            let latency = start.elapsed().as_millis() as u64;
            match res {
                Ok(r) if r.status().is_success() || r.status() == StatusCode::UNAUTHORIZED => {
                    // 401 is technically "reachable" so let's call it a pass for connectivity, 
                    // but maybe warn? No, doctor should be strict. 
                    // But test_provider allows 401? 
                    // Let's stick to success for "pass".
                    if r.status().is_success() {
                        checks.push(CheckResult::pass("LLM", &provider.vendor, Some(latency)));
                    } else {
                         checks.push(CheckResult::fail("LLM", &provider.vendor, format!("Status: {}", r.status())));
                    }
                }
                Ok(r) => checks.push(CheckResult::fail("LLM", &provider.vendor, format!("Status: {}", r.status()))),
                Err(e) => checks.push(CheckResult::fail("LLM", &provider.vendor, e.to_string())),
            }
        }
    }

    // 2. Check Storage (Artifacts)
    if let Some(store) = &state.artifact_store {
        let start = Instant::now();
        let test_data = axum::body::Bytes::from("doctor_check");
        match store.save(test_data).await {
            Ok(id) => {
                 // Try to delete immediately to clean up
                 let _ = store.delete(&id).await;
                 let latency = start.elapsed().as_millis() as u64;
                 checks.push(CheckResult::pass("Storage", "Artifact Store", Some(latency)));
            }
            Err(e) => checks.push(CheckResult::fail("Storage", "Artifact Store", e.to_string())),
        }
    } else {
        checks.push(CheckResult::fail("Storage", "Artifact Store", "Not initialized".to_string()));
    }

    if let Some(store) = &state.session_store {
        let start = Instant::now();
        match store.load("doctor_check").await {
            Ok(_) => {
                let latency = start.elapsed().as_millis() as u64;
                checks.push(CheckResult::pass("Storage", "Session Store", Some(latency)));
            }
            Err(e) => {
                let msg = e.to_string();
                checks.push(CheckResult::fail("Storage", "Session Store", msg));
            }
        }
    } else {
        checks.push(CheckResult::fail("Storage", "Session Store", "Not initialized".to_string()));
    }

    // 4. Check Sandbox (Docker)
    // Verify docker socket connectivity
    let docker = bollard::Docker::connect_with_socket_defaults();
    match docker {
        Ok(d) => {
             let start = Instant::now();
             match d.ping().await {
                 Ok(_) => {
                     let latency = start.elapsed().as_millis() as u64;
                     checks.push(CheckResult::pass("Infrastructure", "Docker", Some(latency)));
                 }
                 Err(e) => checks.push(CheckResult::fail("Infrastructure", "Docker", e.to_string())),
             }
        }
        Err(e) => checks.push(CheckResult::fail("Infrastructure", "Docker", e.to_string())),
    }

    // 5. Check Secrets
    // Try to store and retrieve a dummy secret
    let start = Instant::now();
    let test_key = "doctor_secret_test";
    match state.secrets.store(test_key, "test_value").await {
        Ok(_) => {
            match state.secrets.retrieve(test_key).await {
                Ok(Some(val)) if val == "test_value" => {
                    let _ = state.secrets.delete(test_key).await;
                    let latency = start.elapsed().as_millis() as u64;
                    checks.push(CheckResult::pass("Security", "Secrets Manager", Some(latency)));
                }
                Ok(_) => checks.push(CheckResult::fail("Security", "Secrets Manager", "Value mismatch".to_string())),
                Err(e) => checks.push(CheckResult::fail("Security", "Secrets Manager", e.to_string())),
            }
        }
        Err(e) => checks.push(CheckResult::fail("Security", "Secrets Manager", e.to_string())),
    }

    let overall_status = if checks.iter().any(|c| c.status == "fail") {
        "degraded".to_string()
    } else {
        "healthy".to_string()
    };

    Json(DoctorReport {
        checks,
        overall_status,
    })
}
