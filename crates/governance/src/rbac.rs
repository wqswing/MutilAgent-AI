//! RBAC connector trait for external enterprise IAM integration.

use async_trait::async_trait;
use multi_agent_core::Result;

/// User roles returned from enterprise IAM validation.
#[derive(Debug, Clone, Default)]
pub struct UserRoles {
    /// User identifier from IAM.
    pub user_id: String,
    /// List of role names assigned to the user.
    pub roles: Vec<String>,
    /// Whether the user is an admin.
    pub is_admin: bool,
}

/// Connector for external enterprise RBAC systems (IAM/LDAP/OIDC).
#[async_trait]
pub trait RbacConnector: Send + Sync {
    /// Validate a token/session against enterprise IAM and return user roles.
    async fn validate(&self, token: &str) -> Result<UserRoles>;
    
    /// Check if a user has permission to perform an action on a resource.
    /// This is a convenience method that calls validate and checks roles.
    async fn check_permission(&self, token: &str, resource: &str, action: &str) -> Result<bool>;
}

use jsonwebtoken::{decode, decode_header, DecodingKey, Validation, Algorithm};
use serde::Deserialize;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, Duration};
use multi_agent_core::Error;

// ... existing structs ...

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
    // other fields omitted
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    #[serde(rename = "exp")]
    _exp: usize,
    realm_access: Option<RealmAccess>,
}

#[derive(Debug, Deserialize)]
struct RealmAccess {
    roles: Vec<String>,
}

/// OIDC Connector that validates JWTs against an issuer's JWKS.
pub struct OidcRbacConnector {
    issuer: String,
    jwks_url: String,
    cached_keys: Arc<RwLock<Option<(Vec<Jwk>, SystemTime)>>>,
}

impl OidcRbacConnector {
    pub fn new(issuer: &str) -> Self {
        let issuer = issuer.trim_end_matches('/').to_string();
        Self {
            jwks_url: format!("{}/protocol/openid-connect/certs", issuer), // Keycloak standard
            issuer,
            cached_keys: Arc::new(RwLock::new(None)),
        }
    }

    async fn get_decoding_key(&self, kid: &str) -> Result<DecodingKey> {
        let fetch = {
            let cache = self.cached_keys.read().unwrap();
            match &*cache {
                Some((keys, time)) if time.elapsed().unwrap_or_default() < Duration::from_secs(300) => {
                    if let Some(jwk) = keys.iter().find(|k| k.kid == kid) {
                        return DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
                            .map_err(|e| Error::SecurityViolation(format!("Invalid RSA components: {}", e)));
                    }
                    false // Key not found, force refresh
                }
                _ => true, // Creating read/write lock logic simplified here
            }
        };

        if fetch {
            // naive blocking fetch in async - should really use reqwest async
            // avoiding heavy refactor for now, trusting use of spawn_blocking or similar if needed.
            // actually, let's use reqwest::get async
            let client = reqwest::Client::new();
            let resp = client.get(&self.jwks_url).send().await
                .map_err(|e| Error::SecurityViolation(format!("Failed to fetch JWKS: {}", e)))?;
            let jwks: Jwks = resp.json().await
                .map_err(|e| Error::SecurityViolation(format!("Failed to parse JWKS: {}", e)))?;
            
            let mut cache = self.cached_keys.write().unwrap();
            *cache = Some((jwks.keys.iter().map(|k| Jwk { 
                kid: k.kid.clone(), n: k.n.clone(), e: k.e.clone() 
            }).collect(), SystemTime::now()));
            
            if let Some(jwk) = cache.as_ref().unwrap().0.iter().find(|k| k.kid == kid) {
                return DecodingKey::from_rsa_components(&jwk.n, &jwk.e)
                    .map_err(|e| Error::SecurityViolation(format!("Invalid RSA components: {}", e)));
            }
        }

        Err(Error::SecurityViolation(format!("Key ID {} not found in JWKS", kid)))
    }
}

#[async_trait]
impl RbacConnector for OidcRbacConnector {
    async fn validate(&self, token: &str) -> Result<UserRoles> {
        // 1. Decode header to get kid
        let header = decode_header(token)
            .map_err(|e| Error::SecurityViolation(format!("Invalid token header: {}", e)))?;
        let kid = header.kid.ok_or_else(|| Error::SecurityViolation("Missing kid in token header".into()))?;

        // 2. Get verification key (fetch JWKS if needed)
        let decoding_key = self.get_decoding_key(&kid).await?;

        // 3. Validate signature and claims
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        
        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|e| Error::SecurityViolation(format!("Invalid token: {}", e)))?;

        let roles = token_data.claims.realm_access
            .map(|ra| ra.roles)
            .unwrap_or_default();
            
        let is_admin = roles.contains(&"admin".to_string()) || roles.contains(&"superuser".to_string());

        Ok(UserRoles {
            user_id: token_data.claims.sub,
            roles,
            is_admin,
        })
    }

    async fn check_permission(&self, token: &str, _resource: &str, _action: &str) -> Result<bool> {
        match self.validate(token).await {
            Ok(roles) => Ok(roles.is_admin),
            Err(_) => Ok(false),
        }
    }
}

/// A no-op RBAC connector that allows all actions (for testing/development).
pub struct NoOpRbacConnector;

#[async_trait]
impl RbacConnector for NoOpRbacConnector {
    async fn validate(&self, token: &str) -> Result<UserRoles> {
        let is_admin = token == "admin";
        Ok(UserRoles {
            user_id: if is_admin { "admin" } else { "anonymous" }.to_string(),
            roles: if is_admin { vec!["admin".to_string()] } else { vec!["user".to_string()] },
            is_admin,
        })
    }
    
    async fn check_permission(&self, _token: &str, _resource: &str, _action: &str) -> Result<bool> {
        Ok(true)
    }
}

/// A RBAC connector that validates against a single static token.
pub struct StaticTokenRbacConnector {
    token: String,
}

impl StaticTokenRbacConnector {
    pub fn new(token: impl Into<String>) -> Self {
        Self { token: token.into() }
    }
}

#[async_trait]
impl RbacConnector for StaticTokenRbacConnector {
    async fn validate(&self, token: &str) -> Result<UserRoles> {
        if token == self.token {
            Ok(UserRoles {
                user_id: "admin".to_string(),
                roles: vec!["admin".to_string()],
                is_admin: true,
            })
        } else {
            Err(multi_agent_core::Error::SecurityViolation("Invalid admin token".into()))
        }
    }
    
    async fn check_permission(&self, token: &str, _resource: &str, _action: &str) -> Result<bool> {
        Ok(token == self.token)
    }
}
