use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    SignedOut,
    SigningIn,
    Connected,
    Expired,
    RateLimited,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AccountInfo {
    pub auth_state: Option<AuthState>,
    pub auth_mode: Option<String>,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub requires_openai_auth: bool,
}

impl AccountInfo {
    pub fn account_label(&self) -> String {
        match (&self.email, &self.plan_type) {
            (Some(email), Some(plan_type)) => format!("{email} ({plan_type})"),
            (Some(email), None) => email.clone(),
            (None, Some(plan_type)) => format!("ChatGPT ({plan_type})"),
            (None, None) => "Signed out".to_string(),
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.auth_state, Some(AuthState::Connected))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginStart {
    pub auth_url: String,
    pub login_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RateLimitSnapshot {
    pub message: Option<String>,
    pub raw_json: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{AccountInfo, AuthState};

    #[test]
    fn account_label_prefers_email_and_plan() {
        let info = AccountInfo {
            auth_state: Some(AuthState::Connected),
            auth_mode: Some("chatgpt".into()),
            email: Some("user@example.com".into()),
            plan_type: Some("plus".into()),
            requires_openai_auth: true,
        };

        assert_eq!(info.account_label(), "user@example.com (plus)");
        assert!(info.is_connected());
    }
}
