use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub content: String,
    pub id: String,
    pub role: MessageRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    Assistant,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Usage {
    pub cached_input_tokens: Option<u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamEvent {
    Delta(String),
    Done,
    Error(String),
    Interrupted,
    Start,
    Usage(Usage),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    pub compatible: bool,
    pub default: bool,
    pub hidden: bool,
    pub id: String,
    pub label: String,
    pub model_provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSelection {
    pub compatibility_warning: bool,
    pub models: Vec<ModelDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatThread {
    pub codex_thread_id: Option<String>,
    pub continued_from: Option<String>,
    pub created_at: String,
    pub id: String,
    pub messages: Vec<ChatMessage>,
    pub model_id: String,
    pub status: ThreadStatus,
    pub title: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadStatus {
    Active,
    Continued,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub current_model_id: Option<String>,
    pub current_thread_id: Option<String>,
    pub saw_compatibility_warning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub continued_from: Option<String>,
    pub id: String,
    pub model_id: String,
    pub status: ThreadStatus,
    pub title: String,
    pub updated_at: String,
}
