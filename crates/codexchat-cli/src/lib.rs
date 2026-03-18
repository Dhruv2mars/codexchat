use std::{
    env,
    io::{self, Write},
    path::PathBuf,
    sync::Arc,
};

use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use codexchat_core::{
    auth::AccountInfo,
    codex::{CodexClient, CodexClientOptions},
    config::AppPaths,
    history::ThreadStore,
    models::select_models,
    types::{AppConfig, ChatMessage, MessageRole, ModelDescriptor, StreamEvent},
};
use uuid::Uuid;

mod tui;

#[derive(Debug, Parser)]
#[command(name = "codexchat")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    Chat {
        prompt: String,
    },
    Models,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum AuthCommand {
    Login,
    Logout,
    Status,
}

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Command::Auth { command }) => run_auth_command(command).await,
        Some(Command::Chat { prompt }) => run_chat_command(prompt).await,
        Some(Command::Models) => run_models_command().await,
        None => tui::run_tui(Arc::new(make_client().await?)).await,
    }
}

pub fn app_paths() -> Result<AppPaths> {
    AppPaths::detect()
}

pub fn choose_model(config: &AppConfig, models: &[ModelDescriptor]) -> Result<String> {
    if let Some(current_model_id) = &config.current_model_id
        && models.iter().any(|model| model.id == *current_model_id)
    {
        return Ok(current_model_id.clone());
    }

    models
        .first()
        .map(|model| model.id.clone())
        .ok_or_else(|| anyhow!("no_available_models"))
}

pub async fn current_account(client: &CodexClient) -> Result<AccountInfo> {
    client.account_read().await
}

pub async fn make_client() -> Result<CodexClient> {
    let paths = app_paths()?;
    let codex_bin = env::var_os("CODEXCHAT_CODEX_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.codex_bin_path());
    let options = CodexClientOptions::new(codex_bin);
    CodexClient::spawn(options).await
}

pub async fn load_models(
    client: &CodexClient,
    config: &AppConfig,
) -> Result<(Vec<ModelDescriptor>, bool, String)> {
    let selection = select_models(client.list_models().await?);
    let model_id = choose_model(config, &selection.models)?;
    Ok((selection.models, selection.compatibility_warning, model_id))
}

pub async fn thread_store() -> Result<ThreadStore> {
    Ok(ThreadStore::new(app_paths()?))
}

pub async fn resolve_remote_thread(
    client: &CodexClient,
    store: &ThreadStore,
    local_thread_id: &str,
    model_id: &str,
) -> Result<String> {
    let paths = app_paths()?;
    let existing = store.thread(local_thread_id).await.ok();
    if let Some(thread) = existing
        && let Some(remote_thread_id) = thread.codex_thread_id.clone()
    {
        return match client
            .thread_resume(&remote_thread_id, model_id, &paths.empty_workspace_dir)
            .await
        {
            Ok(handle) => Ok(handle.id),
            Err(_) => {
                let handle = client
                    .thread_start(model_id, &paths.empty_workspace_dir)
                    .await?;
                store
                    .remap_codex_thread(local_thread_id, Some(&remote_thread_id), &handle.id)
                    .await?;
                Ok(handle.id)
            }
        };
    }

    Ok(client
        .thread_start(model_id, &paths.empty_workspace_dir)
        .await?
        .id)
}

async fn require_connected_account(client: &CodexClient) -> Result<AccountInfo> {
    let account = current_account(client).await?;
    if account.is_connected() {
        return Ok(account);
    }
    Err(anyhow!("auth_required"))
}

async fn run_auth_command(command: AuthCommand) -> Result<()> {
    let client = make_client().await?;
    match command {
        AuthCommand::Login => {
            let account = current_account(&client).await?;
            if account.is_connected() {
                println!("Connected: {}", account.account_label());
                return Ok(());
            }

            let login = client.login_chatgpt().await?;
            println!("Open: {}", login.auth_url);
            let _ = open::that(&login.auth_url);
            let account = client.wait_for_login(&login.login_id).await?;
            println!("Connected: {}", account.account_label());
            Ok(())
        }
        AuthCommand::Logout => {
            client.logout().await?;
            println!("Logged out");
            Ok(())
        }
        AuthCommand::Status => {
            let account = current_account(&client).await?;
            if account.is_connected() {
                println!("Connected: {}", account.account_label());
            } else {
                println!("Disconnected");
            }
            Ok(())
        }
    }
}

async fn run_chat_command(prompt: String) -> Result<()> {
    let client = make_client().await?;
    require_connected_account(&client).await?;
    let store = thread_store().await?;
    let mut config = store.load_config().await?;
    let (_, compatibility_warning, model_id) = load_models(&client, &config).await?;
    let local_thread_id = config
        .current_thread_id
        .clone()
        .unwrap_or_else(|| format!("thread-{}", Uuid::new_v4()));
    let remote_thread_id =
        resolve_remote_thread(&client, &store, &local_thread_id, &model_id).await?;

    let user_message = ChatMessage {
        content: prompt.clone(),
        id: format!("msg-{}", Uuid::new_v4()),
        role: MessageRole::User,
    };
    store
        .append_message(
            &local_thread_id,
            &model_id,
            Some(&remote_thread_id),
            user_message,
        )
        .await?;

    config.current_model_id = Some(model_id.clone());
    config.current_thread_id = Some(local_thread_id.clone());
    config.saw_compatibility_warning = config.saw_compatibility_warning || compatibility_warning;
    store.save_config(&config).await?;

    let paths = app_paths()?;
    let (_, mut stream) = client
        .stream_turn(
            &remote_thread_id,
            &model_id,
            &paths.empty_workspace_dir,
            &prompt,
        )
        .await?;

    let mut assistant = String::new();
    let mut failure: Option<String> = None;
    while let Some(event) = stream.recv().await {
        match event {
            StreamEvent::Delta(delta) => {
                print!("{delta}");
                io::stdout().flush()?;
                assistant.push_str(&delta);
            }
            StreamEvent::Error(message) => {
                failure = Some(message);
            }
            StreamEvent::Interrupted => {
                failure = Some("interrupted".into());
            }
            StreamEvent::Done => break,
            StreamEvent::Start | StreamEvent::Usage(_) => {}
        }
    }
    println!();

    store
        .append_message(
            &local_thread_id,
            &model_id,
            Some(&remote_thread_id),
            ChatMessage {
                content: assistant,
                id: format!("msg-{}", Uuid::new_v4()),
                role: MessageRole::Assistant,
            },
        )
        .await?;

    if let Some(message) = failure {
        return Err(anyhow!(message));
    }

    Ok(())
}

async fn run_models_command() -> Result<()> {
    let client = make_client().await?;
    require_connected_account(&client).await?;
    let config = thread_store().await?.load_config().await?;
    let (models, compatibility_warning, _) = load_models(&client, &config).await?;

    if compatibility_warning && !config.saw_compatibility_warning {
        println!("warning\tNo GPT-family model exposed by Codex. Showing fallback OpenAI models.");
    }

    for model in models {
        let status = if model.compatible {
            "compatible"
        } else {
            "fallback"
        };
        println!("{status}\t{}\t{}", model.id, model.label);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::{AuthCommand, Cli, Command, choose_model};
    use codexchat_core::types::{AppConfig, ModelDescriptor};

    #[test]
    fn parses_chat_and_auth_commands() {
        let chat = Cli::parse_from(["codexchat", "chat", "indian capital city?"]);
        assert_eq!(
            chat.command,
            Some(Command::Chat {
                prompt: "indian capital city?".into()
            })
        );

        let auth = Cli::parse_from(["codexchat", "auth", "status"]);
        assert_eq!(
            auth.command,
            Some(Command::Auth {
                command: AuthCommand::Status
            })
        );
    }

    #[test]
    fn chooses_current_model_or_first_available_model() {
        let models = vec![
            ModelDescriptor {
                compatible: true,
                default: false,
                hidden: false,
                id: "gpt-5.4".into(),
                label: "GPT-5.4".into(),
                model_provider: Some("openai".into()),
            },
            ModelDescriptor {
                compatible: true,
                default: false,
                hidden: false,
                id: "gpt-5.2".into(),
                label: "GPT-5.2".into(),
                model_provider: Some("openai".into()),
            },
        ];

        assert_eq!(
            choose_model(
                &AppConfig {
                    current_model_id: Some("gpt-5.2".into()),
                    current_thread_id: None,
                    saw_compatibility_warning: false,
                },
                &models
            )
            .expect("choose current"),
            "gpt-5.2"
        );

        assert_eq!(
            choose_model(
                &AppConfig {
                    current_model_id: Some("missing".into()),
                    current_thread_id: None,
                    saw_compatibility_warning: false,
                },
                &models
            )
            .expect("choose fallback"),
            "gpt-5.4"
        );
    }
}
