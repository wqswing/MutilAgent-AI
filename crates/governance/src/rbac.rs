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
