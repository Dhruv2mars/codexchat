use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::{Mutex, broadcast, mpsc, oneshot},
};

use crate::{
    auth::{AccountInfo, AuthState, LoginStart, RateLimitSnapshot},
    types::{ModelDescriptor, StreamEvent, Usage},
};

const DEFAULT_DEVELOPER_INSTRUCTIONS: &str = "Answer conversationally. Do not assume a coding task. Do not propose file edits, shell commands, tools, or agent actions unless the user explicitly asks.";
pub const PINNED_CODEX_APP_SERVER_TAG: &str = "rust-v0.115.0";
pub const SUPPORTED_RPC_METHODS: &[&str] = &[
    "initialize",
    "account/read",
    "account/login/start",
    "account/login/cancel",
    "account/logout",
    "account/rateLimits/read",
    "model/list",
    "thread/start",
    "thread/resume",
    "thread/list",
    "thread/read",
    "turn/start",
    "turn/interrupt",
];
pub const STREAM_NOTIFICATION_METHODS: &[&str] = &[
    "account/login/completed",
    "account/updated",
    "account/rateLimits/updated",
    "thread/started",
    "turn/started",
    "item/agentMessage/delta",
    "thread/tokenUsage/updated",
    "turn/completed",
];
pub const THREAD_OVERRIDE_FIELDS: &[&str] = &[
    "approvalPolicy",
    "cwd",
    "developerInstructions",
    "model",
    "personality",
    "sandbox",
];
pub const TURN_OVERRIDE_FIELDS: &[&str] = &[
    "approvalPolicy",
    "cwd",
    "input",
    "model",
    "personality",
    "sandboxPolicy",
    "summary",
    "threadId",
];

trait AsyncWriteTarget: AsyncWrite + Send + Unpin {}
impl<T> AsyncWriteTarget for T where T: AsyncWrite + Send + Unpin {}
type PendingResponseMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadHandle {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnHandle {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcNotification {
    pub method: String,
    pub params: Value,
}

#[derive(Clone)]
pub struct CodexClient {
    child: Arc<Mutex<Option<Child>>>,
    rpc: JsonRpcClient,
}

#[derive(Debug, Clone)]
pub struct CodexClientOptions {
    pub client_name: String,
    pub client_title: String,
    pub client_version: String,
    pub codex_bin: PathBuf,
}

impl CodexClientOptions {
    pub fn new(codex_bin: PathBuf) -> Self {
        Self {
            client_name: "codexchat_tui".to_string(),
            client_title: "codexchat".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            codex_bin,
        }
    }
}

#[derive(Clone)]
struct JsonRpcClient {
    next_id: Arc<AtomicU64>,
    notifications: broadcast::Sender<RpcNotification>,
    pending: PendingResponseMap,
    writer: Arc<Mutex<Box<dyn AsyncWriteTarget>>>,
}

impl CodexClient {
    pub async fn spawn(options: CodexClientOptions) -> Result<Self> {
        let mut child = Command::new(&options.codex_bin)
            .arg("app-server")
            .arg("--listen")
            .arg("stdio://")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawn {}", options.codex_bin.display()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("codex_stdin_unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("codex_stdout_unavailable"))?;
        let rpc = JsonRpcClient::new(stdout, stdin);
        let client = Self {
            child: Arc::new(Mutex::new(Some(child))),
            rpc,
        };
        client
            .initialize(
                &options.client_name,
                &options.client_title,
                &options.client_version,
            )
            .await?;
        Ok(client)
    }

    pub fn from_parts<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        Self {
            child: Arc::new(Mutex::new(None)),
            rpc: JsonRpcClient::new(reader, writer),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RpcNotification> {
        self.rpc.subscribe()
    }

    pub async fn initialize(&self, name: &str, title: &str, version: &str) -> Result<()> {
        let _: Value = self
            .rpc
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": name,
                        "title": title,
                        "version": version,
                    }
                }),
            )
            .await?;
        self.rpc.notify("initialized", json!({})).await
    }

    pub async fn account_read(&self) -> Result<AccountInfo> {
        let response: AccountReadResponse = self
            .rpc
            .request("account/read", json!({ "refreshToken": false }))
            .await?;
        Ok(AccountInfo {
            auth_state: auth_state_from_response(&response),
            auth_mode: response
                .account
                .as_ref()
                .map(|value| value.account_type.clone()),
            email: response
                .account
                .as_ref()
                .and_then(|value| value.email.clone()),
            plan_type: response
                .account
                .as_ref()
                .and_then(|value| value.plan_type.clone()),
            requires_openai_auth: response.requires_openai_auth,
        })
    }

    pub async fn login_chatgpt(&self) -> Result<LoginStart> {
        let response: LoginStartResponse = self
            .rpc
            .request("account/login/start", json!({ "type": "chatgpt" }))
            .await?;
        Ok(LoginStart {
            auth_url: response.auth_url,
            login_id: response.login_id,
        })
    }

    pub async fn wait_for_login(&self, login_id: &str) -> Result<AccountInfo> {
        let mut notifications = self.subscribe();
        loop {
            let notification = notifications.recv().await?;
            if notification.method != "account/login/completed" {
                continue;
            }
            let payload: LoginCompletedNotification = serde_json::from_value(notification.params)?;
            if payload.login_id.as_deref() != Some(login_id) {
                continue;
            }
            if !payload.success {
                return Err(anyhow!(
                    payload
                        .error
                        .unwrap_or_else(|| "chatgpt_login_failed".to_string())
                ));
            }
            return self.account_read().await;
        }
    }

    pub async fn logout(&self) -> Result<()> {
        let _: Value = self.rpc.request("account/logout", json!({})).await?;
        Ok(())
    }

    pub async fn rate_limits(&self) -> Result<RateLimitSnapshot> {
        let raw: Value = self
            .rpc
            .request("account/rateLimits/read", json!({}))
            .await?;
        Ok(RateLimitSnapshot {
            message: raw
                .pointer("/rateLimits/message")
                .and_then(Value::as_str)
                .map(str::to_string),
            raw_json: Some(raw.to_string()),
        })
    }

    pub async fn list_models(&self) -> Result<Vec<ModelDescriptor>> {
        let response: ModelListResponse = self
            .rpc
            .request("model/list", json!({ "includeHidden": false }))
            .await?;
        Ok(response
            .data
            .into_iter()
            .map(|model| ModelDescriptor {
                compatible: false,
                default: model.default.unwrap_or(false),
                hidden: model.hidden.unwrap_or(false),
                id: model.id,
                label: model.label,
                model_provider: model.model_provider,
            })
            .collect())
    }

    pub async fn thread_read(&self, thread_id: &str) -> Result<ThreadHandle> {
        let response: ThreadResponse = self
            .rpc
            .request("thread/read", json!({ "threadId": thread_id }))
            .await?;
        Ok(ThreadHandle {
            id: response.thread.id,
        })
    }

    pub async fn thread_resume(
        &self,
        thread_id: &str,
        model: &str,
        cwd: &Path,
    ) -> Result<ThreadHandle> {
        let response: ThreadResponse = self
            .rpc
            .request(
                "thread/resume",
                json!({
                    "approvalPolicy": "never",
                    "cwd": cwd,
                    "developerInstructions": DEFAULT_DEVELOPER_INSTRUCTIONS,
                    "model": model,
                    "personality": "friendly",
                    "sandbox": "readOnly",
                    "threadId": thread_id,
                }),
            )
            .await?;
        Ok(ThreadHandle {
            id: response.thread.id,
        })
    }

    pub async fn thread_start(&self, model: &str, cwd: &Path) -> Result<ThreadHandle> {
        let response: ThreadResponse = self
            .rpc
            .request(
                "thread/start",
                json!({
                    "approvalPolicy": "never",
                    "cwd": cwd,
                    "developerInstructions": DEFAULT_DEVELOPER_INSTRUCTIONS,
                    "model": model,
                    "personality": "friendly",
                    "sandbox": "readOnly",
                    "serviceName": "codexchat",
                }),
            )
            .await?;
        Ok(ThreadHandle {
            id: response.thread.id,
        })
    }

    pub async fn turn_interrupt(&self, thread_id: &str, turn_id: &str) -> Result<()> {
        let _: Value = self
            .rpc
            .request(
                "turn/interrupt",
                json!({
                    "threadId": thread_id,
                    "turnId": turn_id,
                }),
            )
            .await?;
        Ok(())
    }

    pub async fn turn_start(
        &self,
        thread_id: &str,
        model: &str,
        cwd: &Path,
        prompt: &str,
    ) -> Result<TurnHandle> {
        let response: TurnResponse = self
            .rpc
            .request(
                "turn/start",
                json!({
                    "approvalPolicy": "never",
                    "cwd": cwd,
                    "input": [{ "type": "text", "text": prompt }],
                    "model": model,
                    "personality": "friendly",
                    "sandboxPolicy": { "type": "readOnly", "networkAccess": false },
                    "summary": "concise",
                    "threadId": thread_id,
                }),
            )
            .await?;
        Ok(TurnHandle {
            id: response.turn.id,
        })
    }

    pub async fn stream_turn(
        &self,
        thread_id: &str,
        model: &str,
        cwd: &Path,
        prompt: &str,
    ) -> Result<(TurnHandle, mpsc::UnboundedReceiver<StreamEvent>)> {
        let mut notifications = self.subscribe();
        let turn = self.turn_start(thread_id, model, cwd, prompt).await?;
        let (tx, rx) = mpsc::unbounded_channel();
        let thread_id = thread_id.to_string();
        let turn_id = turn.id.clone();

        tokio::spawn(async move {
            while let Ok(notification) = notifications.recv().await {
                if !matches_thread_turn(
                    &notification.method,
                    &notification.params,
                    &thread_id,
                    &turn_id,
                ) {
                    continue;
                }

                match notification.method.as_str() {
                    "turn/started" => {
                        let _ = tx.send(StreamEvent::Start);
                    }
                    "item/agentMessage/delta" => {
                        if let Some(delta) =
                            notification.params.get("delta").and_then(Value::as_str)
                        {
                            let _ = tx.send(StreamEvent::Delta(delta.to_string()));
                        }
                    }
                    "thread/tokenUsage/updated" => {
                        if let Ok(payload) = serde_json::from_value::<TokenUsageUpdatedNotification>(
                            notification.params.clone(),
                        ) && let Some(last) = payload.token_usage.last
                        {
                            let _ = tx.send(StreamEvent::Usage(Usage {
                                cached_input_tokens: last.cached_input_tokens,
                                input_tokens: last.input_tokens,
                                output_tokens: last.output_tokens,
                                reasoning_output_tokens: last.reasoning_output_tokens,
                                total_tokens: last.total_tokens,
                            }));
                        }
                    }
                    "error" => {
                        let message = notification
                            .params
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or("codex_turn_failed");
                        let _ = tx.send(StreamEvent::Error(message.to_string()));
                    }
                    "turn/completed" => {
                        let status = notification
                            .params
                            .pointer("/turn/status")
                            .and_then(Value::as_str)
                            .unwrap_or("completed");
                        match status {
                            "interrupted" => {
                                let _ = tx.send(StreamEvent::Interrupted);
                            }
                            "failed" => {
                                let message = notification
                                    .params
                                    .pointer("/turn/error/message")
                                    .and_then(Value::as_str)
                                    .unwrap_or("codex_turn_failed");
                                let _ = tx.send(StreamEvent::Error(message.to_string()));
                            }
                            _ => {}
                        }
                        let _ = tx.send(StreamEvent::Done);
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok((turn, rx))
    }

    pub fn has_child(&self) -> bool {
        self.child
            .try_lock()
            .map(|guard| guard.is_some())
            .unwrap_or(true)
    }
}

impl JsonRpcClient {
    fn new<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let notifications = broadcast::channel(256).0;
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let pending_reader = pending.clone();
        let notifications_reader = notifications.clone();
        tokio::spawn(read_loop(
            BufReader::new(reader),
            pending_reader,
            notifications_reader,
        ));

        Self {
            next_id: Arc::new(AtomicU64::new(1)),
            notifications,
            pending,
            writer: Arc::new(Mutex::new(Box::new(writer))),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<RpcNotification> {
        self.notifications.subscribe()
    }

    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.write_message(&json!({ "method": method, "params": params }))
            .await
    }

    async fn request<T>(&self, method: &str, params: Value) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write_message(&json!({ "id": id, "method": method, "params": params }))
            .await?;
        let response = rx
            .await
            .map_err(|_| anyhow!("rpc_response_dropped"))?
            .map_err(|error| anyhow!(error))?;
        Ok(serde_json::from_value(response)?)
    }

    async fn write_message(&self, message: &Value) -> Result<()> {
        let mut writer = self.writer.lock().await;
        let mut bytes = serde_json::to_vec(message)?;
        bytes.push(b'\n');
        writer.write_all(&bytes).await?;
        writer.flush().await?;
        Ok(())
    }
}

async fn read_loop<R>(
    mut reader: BufReader<R>,
    pending: PendingResponseMap,
    notifications: broadcast::Sender<RpcNotification>,
) where
    R: AsyncRead + Send + Unpin + 'static,
{
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }

        if line.trim().is_empty() {
            continue;
        }

        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if let Some(method) = message.get("method").and_then(Value::as_str) {
            let _ = notifications.send(RpcNotification {
                method: method.to_string(),
                params: message.get("params").cloned().unwrap_or_else(|| json!({})),
            });
            continue;
        }

        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            continue;
        };

        let sender = pending.lock().await.remove(&id);
        if let Some(sender) = sender {
            if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
                let _ = sender.send(Err(error.to_string()));
            } else {
                let _ = sender.send(Ok(message.get("result").cloned().unwrap_or(Value::Null)));
            }
        }
    }
}

fn auth_state_from_response(response: &AccountReadResponse) -> Option<AuthState> {
    match response
        .account
        .as_ref()
        .map(|value| value.account_type.as_str())
    {
        Some("chatgpt") | Some("apiKey") => Some(AuthState::Connected),
        Some(_) => Some(AuthState::Error),
        None if response.requires_openai_auth => Some(AuthState::SignedOut),
        None => None,
    }
}

fn matches_thread_turn(method: &str, params: &Value, thread_id: &str, turn_id: &str) -> bool {
    let matches_thread = params
        .get("threadId")
        .and_then(Value::as_str)
        .map(|value| value == thread_id)
        .unwrap_or_else(|| {
            params
                .pointer("/turn/threadId")
                .and_then(Value::as_str)
                .map(|value| value == thread_id)
                .unwrap_or(false)
        });

    if method == "thread/tokenUsage/updated" {
        return matches_thread;
    }

    let matches_turn = params
        .get("turnId")
        .and_then(Value::as_str)
        .map(|value| value == turn_id)
        .unwrap_or_else(|| {
            params
                .pointer("/turn/id")
                .and_then(Value::as_str)
                .map(|value| value == turn_id)
                .unwrap_or(false)
        });

    matches_thread && matches_turn
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountReadResponse {
    account: Option<AccountPayload>,
    requires_openai_auth: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountPayload {
    #[serde(rename = "type")]
    account_type: String,
    email: Option<String>,
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginStartResponse {
    auth_url: String,
    login_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginCompletedNotification {
    error: Option<String>,
    login_id: Option<String>,
    success: bool,
}

#[derive(Debug, Deserialize)]
struct ThreadResponse {
    thread: RemoteThread,
}

#[derive(Debug, Deserialize)]
struct RemoteThread {
    id: String,
}

#[derive(Debug, Deserialize)]
struct TurnResponse {
    turn: RemoteTurn,
}

#[derive(Debug, Deserialize)]
struct RemoteTurn {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<RemoteModel>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteModel {
    default: Option<bool>,
    hidden: Option<bool>,
    id: String,
    label: String,
    model_provider: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenUsageUpdatedNotification {
    token_usage: TokenUsage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenUsage {
    last: Option<TokenUsageCounts>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenUsageCounts {
    cached_input_tokens: Option<u64>,
    input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use anyhow::Result;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, duplex};
    use tokio::time::{Duration, sleep, timeout};

    use crate::{auth::AuthState, types::StreamEvent};

    use super::{
        CodexClient, PINNED_CODEX_APP_SERVER_TAG, STREAM_NOTIFICATION_METHODS,
        SUPPORTED_RPC_METHODS, THREAD_OVERRIDE_FIELDS, TURN_OVERRIDE_FIELDS,
    };

    #[derive(Debug, Deserialize)]
    struct ProtocolFixture {
        pinned_tag: String,
        rpc_methods: Vec<String>,
        stream_notifications: Vec<String>,
        thread_override_fields: Vec<String>,
        turn_override_fields: Vec<String>,
    }

    #[test]
    fn checked_in_protocol_fixture_matches_supported_subset() -> Result<()> {
        let fixture_path = format!(
            "{}/../fixtures/codex-app-server-rust-v0.115.0.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let fixture: ProtocolFixture = serde_json::from_str(&fs::read_to_string(fixture_path)?)?;

        assert_eq!(fixture.pinned_tag, PINNED_CODEX_APP_SERVER_TAG);
        assert_eq!(
            fixture.rpc_methods,
            SUPPORTED_RPC_METHODS
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            fixture.stream_notifications,
            STREAM_NOTIFICATION_METHODS
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            fixture.thread_override_fields,
            THREAD_OVERRIDE_FIELDS
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            fixture.turn_override_fields,
            TURN_OVERRIDE_FIELDS
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[tokio::test]
    async fn reads_account_models_and_turn_stream_from_json_rpc_server() -> Result<()> {
        let (client_io, server_io) = duplex(8192);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, mut server_writer) = tokio::io::split(server_io);

        tokio::spawn(async move {
            let mut reader = BufReader::new(server_reader);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.ok() == Some(0) {
                    break;
                }
                let request: Value = serde_json::from_str(&line).expect("json");
                let id = request.get("id").and_then(Value::as_u64);
                let method = request.get("method").and_then(Value::as_str).unwrap_or("");
                match method {
                    "initialize" => {
                        server_writer
                            .write_all(
                                format!("{}\n", json!({ "id": id, "result": {} })).as_bytes(),
                            )
                            .await
                            .expect("initialize response");
                    }
                    "initialized" => {}
                    "account/read" => {
                        server_writer
                            .write_all(
                                format!(
                                    "{}\n",
                                    json!({
                                        "id": id,
                                        "result": {
                                            "account": {
                                                "type": "chatgpt",
                                                "email": "user@example.com",
                                                "planType": "plus"
                                            },
                                            "requiresOpenaiAuth": true
                                        }
                                    })
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("account response");
                    }
                    "model/list" => {
                        server_writer
                            .write_all(
                                format!(
                                    "{}\n",
                                    json!({
                                        "id": id,
                                        "result": {
                                            "data": [
                                                { "id": "gpt-5.4", "label": "GPT-5.4", "default": true, "hidden": false, "modelProvider": "openai" }
                                            ]
                                        }
                                    })
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("models response");
                    }
                    "thread/start" => {
                        server_writer
                            .write_all(
                                format!(
                                    "{}\n",
                                    json!({ "id": id, "result": { "thread": { "id": "thr_123" } } })
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("thread response");
                    }
                    "turn/start" => {
                        server_writer
                            .write_all(
                                format!(
                                    "{}\n",
                                    json!({ "id": id, "result": { "turn": { "id": "turn_123" } } })
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("turn response");
                        for notification in [
                            json!({ "method": "turn/started", "params": { "threadId": "thr_123", "turnId": "turn_123", "turn": { "id": "turn_123", "threadId": "thr_123" } } }),
                            json!({ "method": "item/agentMessage/delta", "params": { "threadId": "thr_123", "turnId": "turn_123", "delta": "Hello" } }),
                            json!({ "method": "thread/tokenUsage/updated", "params": { "threadId": "thr_123", "tokenUsage": { "last": { "inputTokens": 12, "outputTokens": 8, "reasoningOutputTokens": 3, "totalTokens": 20, "cachedInputTokens": 1 } } } }),
                            json!({ "method": "turn/completed", "params": { "threadId": "thr_123", "turnId": "turn_123", "turn": { "id": "turn_123", "threadId": "thr_123", "status": "completed" } } }),
                        ] {
                            server_writer
                                .write_all(format!("{notification}\n").as_bytes())
                                .await
                                .expect("notification");
                        }
                    }
                    _ => {}
                }
            }
        });

        let client = CodexClient::from_parts(client_reader, client_writer);
        client
            .initialize("codexchat_tui", "codexchat", "0.1.0")
            .await?;

        let account = client.account_read().await?;
        assert_eq!(account.auth_state, Some(AuthState::Connected));
        assert_eq!(account.plan_type.as_deref(), Some("plus"));

        let models = client.list_models().await?;
        assert_eq!(models[0].id, "gpt-5.4");

        let thread = client.thread_start("gpt-5.4", Path::new("/tmp")).await?;
        assert_eq!(thread.id, "thr_123");

        let (_, mut stream) = client
            .stream_turn("thr_123", "gpt-5.4", Path::new("/tmp"), "hello")
            .await?;

        assert_eq!(stream.recv().await, Some(StreamEvent::Start));
        assert_eq!(
            stream.recv().await,
            Some(StreamEvent::Delta("Hello".into()))
        );
        match stream.recv().await {
            Some(StreamEvent::Usage(usage)) => {
                assert_eq!(usage.input_tokens, 12);
                assert_eq!(usage.output_tokens, 8);
            }
            other => panic!("unexpected stream event: {other:?}"),
        }
        assert_eq!(stream.recv().await, Some(StreamEvent::Done));

        Ok(())
    }

    #[tokio::test]
    async fn wait_for_login_returns_error_when_login_fails() -> Result<()> {
        let (client_io, server_io) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, mut server_writer) = tokio::io::split(server_io);

        tokio::spawn(async move {
            let mut reader = BufReader::new(server_reader);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.ok() == Some(0) {
                    break;
                }
                let request: Value = serde_json::from_str(&line).expect("json");
                let id = request.get("id").and_then(Value::as_u64);
                let method = request.get("method").and_then(Value::as_str).unwrap_or("");
                match method {
                    "initialize" => {
                        server_writer
                            .write_all(
                                format!("{}\n", json!({ "id": id, "result": {} })).as_bytes(),
                            )
                            .await
                            .expect("initialize response");
                    }
                    "initialized" => {}
                    "account/login/start" => {
                        server_writer
                            .write_all(
                                format!(
                                    "{}\n",
                                    json!({
                                        "id": id,
                                        "result": {
                                            "authUrl": "https://chatgpt.com/login",
                                            "loginId": "login_123"
                                        }
                                    })
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("login response");
                        sleep(Duration::from_millis(25)).await;
                        server_writer
                            .write_all(
                                format!(
                                    "{}\n",
                                    json!({
                                        "method": "account/login/completed",
                                        "params": {
                                            "loginId": "login_123",
                                            "success": false,
                                            "error": "cancelled"
                                        }
                                    })
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("login completed");
                    }
                    _ => {}
                }
            }
        });

        let client = CodexClient::from_parts(client_reader, client_writer);
        client
            .initialize("codexchat_tui", "codexchat", "0.1.0")
            .await?;
        let login = client.login_chatgpt().await?;
        let error = timeout(
            Duration::from_secs(1),
            client.wait_for_login(&login.login_id),
        )
        .await?
        .expect_err("failed login");
        assert_eq!(error.to_string(), "cancelled");

        Ok(())
    }
}
