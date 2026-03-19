use std::{io, sync::Arc, time::Duration};

use anyhow::Result;
use codexchat_core::{
    auth::{AccountInfo, LoginStart},
    codex::CodexClient,
    history::ThreadStore,
    types::{
        AppConfig, ChatMessage, ChatThread, MessageRole, ModelDescriptor, StreamEvent,
        ThreadStatus, ThreadSummary,
    },
};
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
use tokio::{sync::mpsc, task::JoinHandle};
use tui_textarea::{Input, Key, TextArea};
use uuid::Uuid;

use crate::{
    app_paths, choose_model, current_account, load_models, resolve_remote_thread, thread_store,
};

enum AppEvent {
    AuthComplete(AccountInfo),
    AuthError(String),
    AuthStarted(LoginStart),
    ChatFailed(String),
    ChatFinished,
    ChatStarted {
        remote_thread_id: String,
        turn_id: String,
    },
    ChatStream(StreamEvent),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Threads,
    Chat,
    Composer,
    Models,
}

struct App {
    account: Option<AccountInfo>,
    active_remote_thread_id: Option<String>,
    active_turn_id: Option<String>,
    chat_follow: bool,
    chat_max_scroll: u16,
    chat_scroll: u16,
    compatibility_warning: bool,
    composer: TextArea<'static>,
    config: AppConfig,
    current_thread_id: Option<String>,
    focus: Focus,
    login: Option<LoginStart>,
    messages: Vec<ChatMessage>,
    model_cursor: usize,
    model_query: String,
    models: Vec<ModelDescriptor>,
    status: String,
    stream_task: Option<JoinHandle<()>>,
    thread_cursor: usize,
    thread_query: String,
    threads: Vec<ThreadSummary>,
}

impl App {
    fn new(config: AppConfig, threads: Vec<ThreadSummary>) -> Self {
        let mut composer = TextArea::default();
        composer.set_block(panel_block("Composer", true));
        Self {
            account: None,
            active_remote_thread_id: None,
            active_turn_id: None,
            chat_follow: true,
            chat_max_scroll: 0,
            chat_scroll: 0,
            compatibility_warning: false,
            composer,
            config,
            current_thread_id: None,
            focus: Focus::Composer,
            login: None,
            messages: Vec::new(),
            model_cursor: 0,
            model_query: String::new(),
            models: Vec::new(),
            status: "Ready".into(),
            stream_task: None,
            thread_cursor: 0,
            thread_query: String::new(),
            threads,
        }
    }

    fn account_label(&self) -> String {
        self.account
            .as_ref()
            .map(AccountInfo::account_label)
            .unwrap_or_else(|| "Signed out".into())
    }

    fn current_model_id(&self) -> Option<&str> {
        self.config.current_model_id.as_deref()
    }

    fn current_model_label(&self) -> String {
        self.models
            .iter()
            .find(|model| self.current_model_id() == Some(model.id.as_str()))
            .map(|model| model.label.clone())
            .unwrap_or_else(|| "No model".into())
    }

    fn current_thread_title(&self) -> String {
        self.threads
            .iter()
            .find(|thread| self.current_thread_id.as_deref() == Some(thread.id.as_str()))
            .map(|thread| thread.title.clone())
            .unwrap_or_else(|| "New chat".into())
    }

    fn filtered_models(&self) -> Vec<&ModelDescriptor> {
        let query = self.model_query.to_lowercase();
        self.models
            .iter()
            .filter(|model| {
                query.is_empty()
                    || model.id.to_lowercase().contains(&query)
                    || model.label.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn filtered_threads(&self) -> Vec<&ThreadSummary> {
        let query = self.thread_query.trim().to_lowercase();
        let mut threads = self
            .threads
            .iter()
            .filter(|thread| {
                query.is_empty()
                    || thread.title.to_lowercase().contains(&query)
                    || thread.model_id.to_lowercase().contains(&query)
            })
            .collect::<Vec<_>>();
        threads.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        threads
    }

    fn focus_label(&self) -> &'static str {
        match self.focus {
            Focus::Threads => "Threads",
            Focus::Chat => "Chat",
            Focus::Composer => "Composer",
            Focus::Models => "Models",
        }
    }

    fn has_session(&self) -> bool {
        self.account.as_ref().is_some_and(AccountInfo::is_connected)
    }

    fn is_streaming(&self) -> bool {
        self.stream_task.is_some()
    }

    fn next_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Threads => Focus::Chat,
            Focus::Chat => Focus::Composer,
            Focus::Composer => Focus::Models,
            Focus::Models => Focus::Threads,
        };
    }

    fn previous_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Threads => Focus::Models,
            Focus::Chat => Focus::Threads,
            Focus::Composer => Focus::Chat,
            Focus::Models => Focus::Composer,
        };
    }
}

pub async fn run_tui(client: Arc<CodexClient>) -> Result<()> {
    let store = thread_store().await?;
    let mut app = App::new(store.load_config().await?, store.list_threads().await?);
    app.account = current_account(&client).await.ok();
    if app.has_session() {
        let (models, compatibility_warning, model_id) = load_models(&client, &app.config).await?;
        app.models = models;
        app.compatibility_warning = compatibility_warning;
        app.config.current_model_id = Some(model_id);
    } else {
        app.status = "Press Enter to sign in with ChatGPT".into();
    }
    if let Some(thread_id) = app.config.current_thread_id.clone()
        && let Ok(thread) = store.thread(&thread_id).await
    {
        app.current_thread_id = Some(thread_id);
        app.messages = thread.messages;
    } else if let Some(first_thread) = app.threads.first().cloned() {
        app.current_thread_id = Some(first_thread.id.clone());
        app.messages = store.thread(&first_thread.id).await?.messages;
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut reader = EventStream::new();

    let result = run_loop(
        &mut terminal,
        client,
        &store,
        &mut app,
        &event_tx,
        &mut event_rx,
        &mut reader,
    )
    .await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: Arc<CodexClient>,
    store: &ThreadStore,
    app: &mut App,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
    event_rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    reader: &mut EventStream,
) -> Result<()> {
    loop {
        terminal.draw(|frame| render(frame.area(), frame, app))?;
        tokio::select! {
            maybe_event = reader.next() => {
                if let Some(Ok(Event::Key(key_event))) = maybe_event
                    && handle_key_event(key_event, client.clone(), store, app, event_tx).await?
                {
                    return Ok(());
                }
            }
            maybe_app_event = event_rx.recv() => {
                if let Some(event) = maybe_app_event {
                    handle_app_event(client.clone(), store, app, event).await?;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }
}

async fn handle_app_event(
    client: Arc<CodexClient>,
    store: &ThreadStore,
    app: &mut App,
    event: AppEvent,
) -> Result<()> {
    match event {
        AppEvent::AuthStarted(login) => {
            app.login = Some(login);
            app.status = "Browser opened. Finish ChatGPT sign-in.".into();
        }
        AppEvent::AuthComplete(account) => {
            app.account = Some(account.clone());
            app.login = None;
            let (models, compatibility_warning, model_id) =
                load_models(&client, &app.config).await?;
            app.models = models;
            app.compatibility_warning = compatibility_warning;
            app.config.current_model_id = Some(model_id);
            store.save_config(&app.config).await?;
            app.status = format!("Connected: {}", account.account_label());
        }
        AppEvent::AuthError(message) => {
            app.login = None;
            app.status = message;
        }
        AppEvent::ChatStarted {
            remote_thread_id,
            turn_id,
        } => {
            app.active_remote_thread_id = Some(remote_thread_id);
            app.active_turn_id = Some(turn_id);
        }
        AppEvent::ChatStream(StreamEvent::Delta(delta)) => {
            if let Some(last) = app.messages.last_mut() {
                last.content.push_str(&delta);
            }
            if app.chat_follow {
                app.chat_scroll = app.chat_max_scroll;
            }
            persist_current_thread(store, app).await?;
        }
        AppEvent::ChatStream(StreamEvent::Usage(usage)) => {
            app.status = format!(
                "Streaming: {} in / {} out",
                usage.input_tokens, usage.output_tokens
            );
        }
        AppEvent::ChatStream(StreamEvent::Error(message)) | AppEvent::ChatFailed(message) => {
            app.stream_task = None;
            app.active_turn_id = None;
            app.status = message;
            persist_current_thread(store, app).await?;
        }
        AppEvent::ChatStream(StreamEvent::Interrupted) => {
            app.stream_task = None;
            app.active_turn_id = None;
            app.status = "Interrupted".into();
            persist_current_thread(store, app).await?;
        }
        AppEvent::ChatFinished | AppEvent::ChatStream(StreamEvent::Done) => {
            app.stream_task = None;
            app.active_turn_id = None;
            app.status = "Reply complete".into();
            persist_current_thread(store, app).await?;
        }
        AppEvent::ChatStream(StreamEvent::Start) => {
            app.status = "Streaming...".into();
        }
    }
    Ok(())
}

async fn handle_key_event(
    key_event: KeyEvent,
    client: Arc<CodexClient>,
    store: &ThreadStore,
    app: &mut App,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
) -> Result<bool> {
    if key_event.modifiers.contains(KeyModifiers::CONTROL) && key_event.code == KeyCode::Char('c') {
        return Ok(true);
    }
    if key_event.code == KeyCode::Char('q') && !app.is_streaming() {
        return Ok(true);
    }
    if key_event.code == KeyCode::Esc && app.is_streaming() {
        if let (Some(remote_thread_id), Some(turn_id)) = (
            app.active_remote_thread_id.clone(),
            app.active_turn_id.clone(),
        ) {
            let client = client.clone();
            tokio::spawn(async move {
                let _ = client.turn_interrupt(&remote_thread_id, &turn_id).await;
            });
            app.status = "Stopping stream...".into();
        }
        return Ok(false);
    }

    if key_event.modifiers.contains(KeyModifiers::CONTROL) {
        match key_event.code {
            KeyCode::Char('n') => {
                create_new_thread(store, app).await?;
                return Ok(false);
            }
            KeyCode::Char('l') => {
                logout(client, app).await?;
                return Ok(false);
            }
            KeyCode::Char('j') => {
                app.focus = Focus::Threads;
                return Ok(false);
            }
            KeyCode::Char('k') => {
                app.focus = Focus::Models;
                return Ok(false);
            }
            KeyCode::Char('m') => {
                app.focus = Focus::Composer;
                return Ok(false);
            }
            KeyCode::Char('g') => {
                app.focus = Focus::Chat;
                return Ok(false);
            }
            _ => {}
        }
    }

    if !app.has_session() {
        if matches!(key_event.code, KeyCode::Enter | KeyCode::Char('l')) {
            start_login(client, event_tx.clone()).await?;
        }
        return Ok(false);
    }

    if key_event.code == KeyCode::Tab {
        app.next_focus();
        return Ok(false);
    }
    if key_event.code == KeyCode::BackTab {
        app.previous_focus();
        return Ok(false);
    }

    match app.focus {
        Focus::Threads => handle_thread_key(key_event, store, app).await?,
        Focus::Chat => handle_chat_key(key_event, app).await,
        Focus::Composer => handle_composer_key(key_event, client, store, app, event_tx).await?,
        Focus::Models => handle_model_key(key_event, store, app).await?,
    }
    Ok(false)
}

async fn handle_chat_key(key_event: KeyEvent, app: &mut App) {
    match key_event.code {
        KeyCode::Up => {
            app.chat_follow = false;
            app.chat_scroll = app.chat_scroll.saturating_sub(1);
        }
        KeyCode::Down => {
            app.chat_follow = false;
            app.chat_scroll = (app.chat_scroll + 1).min(app.chat_max_scroll);
        }
        KeyCode::PageUp => {
            app.chat_follow = false;
            app.chat_scroll = app.chat_scroll.saturating_sub(6);
        }
        KeyCode::PageDown => {
            app.chat_follow = false;
            app.chat_scroll = (app.chat_scroll + 6).min(app.chat_max_scroll);
        }
        KeyCode::End | KeyCode::Esc => {
            app.chat_follow = true;
            app.chat_scroll = app.chat_max_scroll;
        }
        _ => {}
    }
}

async fn handle_composer_key(
    key_event: KeyEvent,
    client: Arc<CodexClient>,
    store: &ThreadStore,
    app: &mut App,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    if key_event.code == KeyCode::Enter && !key_event.modifiers.contains(KeyModifiers::SHIFT) {
        if !app.is_streaming() {
            send_message(client, store, app, event_tx.clone()).await?;
        }
        return Ok(());
    }

    app.composer.input(key_event_to_input(key_event));
    Ok(())
}

async fn handle_model_key(key_event: KeyEvent, store: &ThreadStore, app: &mut App) -> Result<()> {
    let filtered = app.filtered_models();
    match key_event.code {
        KeyCode::Backspace => {
            app.model_query.pop();
            app.model_cursor = 0;
        }
        KeyCode::Char(value) => {
            if key_event.modifiers.is_empty() || key_event.modifiers == KeyModifiers::SHIFT {
                app.model_query.push(value);
                app.model_cursor = 0;
            }
        }
        KeyCode::Down => {
            if !filtered.is_empty() {
                app.model_cursor = (app.model_cursor + 1).min(filtered.len().saturating_sub(1));
            }
        }
        KeyCode::Up => {
            app.model_cursor = app.model_cursor.saturating_sub(1);
        }
        KeyCode::Enter => {
            if let Some(model) = filtered.get(app.model_cursor) {
                let model_id = model.id.clone();
                let model_label = model.label.clone();
                app.config.current_model_id = Some(model_id);
                store.save_config(&app.config).await?;
                app.status = format!("Model: {model_label}");
            }
        }
        KeyCode::Esc => {
            app.model_query.clear();
            app.model_cursor = 0;
        }
        _ => {}
    }
    Ok(())
}

async fn handle_thread_key(key_event: KeyEvent, store: &ThreadStore, app: &mut App) -> Result<()> {
    match key_event.code {
        KeyCode::Char('n') => create_new_thread(store, app).await?,
        KeyCode::Delete => {
            let selected = app
                .filtered_threads()
                .get(app.thread_cursor)
                .cloned()
                .cloned();
            if let Some(thread) = selected {
                store.delete_thread(&thread.id).await?;
                app.threads = store.list_threads().await?;
                if let Some(next) = app.filtered_threads().first().cloned().cloned() {
                    app.current_thread_id = Some(next.id.clone());
                    app.messages = store.thread(&next.id).await?.messages;
                } else {
                    app.current_thread_id = None;
                    app.messages.clear();
                }
                store.save_config(&app.config).await?;
                app.status = "Thread deleted".into();
            }
        }
        KeyCode::Backspace => {
            app.thread_query.pop();
            app.thread_cursor = 0;
        }
        KeyCode::Down => {
            let len = app.filtered_threads().len();
            if len > 0 {
                app.thread_cursor = (app.thread_cursor + 1).min(len.saturating_sub(1));
            }
        }
        KeyCode::Up => {
            app.thread_cursor = app.thread_cursor.saturating_sub(1);
        }
        KeyCode::Enter => {
            let selected = app
                .filtered_threads()
                .get(app.thread_cursor)
                .cloned()
                .cloned();
            if let Some(thread) = selected {
                app.current_thread_id = Some(thread.id.clone());
                app.config.current_thread_id = Some(thread.id.clone());
                app.messages = store.thread(&thread.id).await?.messages;
                store.save_config(&app.config).await?;
                app.status = format!("Thread: {}", thread.title);
            }
        }
        KeyCode::Esc => {
            app.thread_query.clear();
            app.thread_cursor = 0;
        }
        KeyCode::Char(value) => {
            if key_event.modifiers.is_empty() || key_event.modifiers == KeyModifiers::SHIFT {
                app.thread_query.push(value);
                app.thread_cursor = 0;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn create_new_thread(store: &ThreadStore, app: &mut App) -> Result<()> {
    app.current_thread_id = Some(format!("thread-{}", Uuid::new_v4()));
    app.config.current_thread_id = app.current_thread_id.clone();
    app.active_remote_thread_id = None;
    app.active_turn_id = None;
    app.messages.clear();
    app.chat_follow = true;
    app.chat_scroll = 0;
    store.save_config(&app.config).await?;
    app.status = "New chat".into();
    Ok(())
}

async fn logout(client: Arc<CodexClient>, app: &mut App) -> Result<()> {
    client.logout().await?;
    if let Some(task) = app.stream_task.take() {
        task.abort();
    }
    app.account = None;
    app.active_remote_thread_id = None;
    app.active_turn_id = None;
    app.login = None;
    app.model_cursor = 0;
    app.model_query.clear();
    app.models.clear();
    app.status = "Logged out".into();
    Ok(())
}

async fn persist_current_thread(store: &ThreadStore, app: &App) -> Result<()> {
    let Some(thread_id) = app.current_thread_id.clone() else {
        return Ok(());
    };
    let Some(model_id) = app.config.current_model_id.clone() else {
        return Ok(());
    };
    if app.messages.is_empty() {
        return Ok(());
    }

    let previous = store.thread(&thread_id).await.ok();
    let thread = ChatThread {
        codex_thread_id: app.active_remote_thread_id.clone().or_else(|| {
            previous
                .as_ref()
                .and_then(|thread| thread.codex_thread_id.clone())
        }),
        continued_from: previous
            .as_ref()
            .and_then(|thread| thread.continued_from.clone()),
        created_at: previous
            .as_ref()
            .map(|thread| thread.created_at.clone())
            .unwrap_or_else(current_timestamp),
        id: thread_id,
        messages: app.messages.clone(),
        model_id,
        status: previous
            .as_ref()
            .map(|thread| thread.status.clone())
            .unwrap_or(ThreadStatus::Active),
        title: app
            .messages
            .iter()
            .find(|message| matches!(message.role, MessageRole::User))
            .map(|message| message.content.chars().take(48).collect::<String>())
            .unwrap_or_else(|| "New chat".into()),
        updated_at: current_timestamp(),
    };
    store.save_thread(&thread).await
}

fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time after epoch")
        .as_secs()
        .to_string()
}

async fn send_message(
    client: Arc<CodexClient>,
    store: &ThreadStore,
    app: &mut App,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let prompt = app.composer.lines().join("\n").trim().to_string();
    if prompt.is_empty() {
        return Ok(());
    }

    let model_id = choose_model(&app.config, &app.models)?;
    let local_thread_id = app
        .current_thread_id
        .clone()
        .unwrap_or_else(|| format!("thread-{}", Uuid::new_v4()));
    let remote_thread_id =
        resolve_remote_thread(&client, store, &local_thread_id, &model_id).await?;

    app.current_thread_id = Some(local_thread_id.clone());
    app.config.current_thread_id = Some(local_thread_id.clone());
    app.config.current_model_id = Some(model_id.clone());
    app.active_remote_thread_id = Some(remote_thread_id.clone());
    app.composer = {
        let mut composer = TextArea::default();
        composer.set_block(panel_block("Composer", app.focus == Focus::Composer));
        composer
    };

    app.messages.push(ChatMessage {
        content: prompt.clone(),
        id: format!("msg-{}", Uuid::new_v4()),
        role: MessageRole::User,
    });
    app.messages.push(ChatMessage {
        content: String::new(),
        id: format!("msg-{}", Uuid::new_v4()),
        role: MessageRole::Assistant,
    });
    persist_current_thread(store, app).await?;
    store.save_config(&app.config).await?;
    app.status = "Sending...".into();

    let client_clone = client.clone();
    let remote_thread_id_clone = remote_thread_id.clone();
    let model_id_clone = model_id.clone();
    let event_tx_clone = event_tx.clone();
    let empty_workspace_dir = app_paths()?.empty_workspace_dir;
    let task = tokio::spawn(async move {
        match client_clone
            .stream_turn(
                &remote_thread_id_clone,
                &model_id_clone,
                &empty_workspace_dir,
                &prompt,
            )
            .await
        {
            Ok((turn, mut stream)) => {
                let _ = event_tx_clone.send(AppEvent::ChatStarted {
                    remote_thread_id: remote_thread_id_clone,
                    turn_id: turn.id,
                });
                while let Some(event) = stream.recv().await {
                    let finished = matches!(
                        event,
                        StreamEvent::Done | StreamEvent::Interrupted | StreamEvent::Error(_)
                    );
                    let _ = event_tx_clone.send(AppEvent::ChatStream(event));
                    if finished {
                        break;
                    }
                }
                let _ = event_tx_clone.send(AppEvent::ChatFinished);
            }
            Err(error) => {
                let _ = event_tx_clone.send(AppEvent::ChatFailed(error.to_string()));
            }
        }
    });
    app.stream_task = Some(task);
    Ok(())
}

async fn start_login(
    client: Arc<CodexClient>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    tokio::spawn(async move {
        match client.login_chatgpt().await {
            Ok(login) => {
                let _ = event_tx.send(AppEvent::AuthStarted(login.clone()));
                let _ = open::that(&login.auth_url);
                match client.wait_for_login(&login.login_id).await {
                    Ok(account) => {
                        let _ = event_tx.send(AppEvent::AuthComplete(account));
                    }
                    Err(error) => {
                        let _ = event_tx.send(AppEvent::AuthError(error.to_string()));
                    }
                }
            }
            Err(error) => {
                let _ = event_tx.send(AppEvent::AuthError(error.to_string()));
            }
        }
    });
    Ok(())
}

fn render(area: Rect, frame: &mut ratatui::Frame<'_>, app: &mut App) {
    if !app.has_session() {
        let block = panel_block("Sign In", true);
        let content = if let Some(login) = &app.login {
            vec![
                Line::from(Span::styled(
                    "Sign in with ChatGPT",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("Browser opened for authentication."),
                Line::from(format!("URL  {}", login.auth_url)),
                Line::from(""),
                Line::from("Waiting for login to complete..."),
                Line::from("Press q to quit."),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    "Chat with your ChatGPT subscription",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from("Press Enter to sign in with ChatGPT."),
                Line::from("Auth is handled by the official Codex bridge."),
                Line::from("Tokens stay in Codex storage, not this app."),
            ]
        };
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(content)
                .block(block)
                .wrap(Wrap { trim: true }),
            centered_rect(area, 72, 34),
        );
        return;
    }

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(12),
            Constraint::Length(2),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(34), Constraint::Min(48)])
        .split(outer[1]);
    let sidebar = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(11), Constraint::Min(10)])
        .split(body[0]);
    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(12), Constraint::Length(6)])
        .split(body[1]);

    let header_lines = vec![
        Line::from(vec![
            Span::styled(
                " codexchat ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "  {}  |  {}  |  {}  |  focus {}",
                app.current_model_label(),
                app.current_thread_title(),
                app.account_label(),
                app.focus_label()
            )),
        ]),
        Line::from(format!(
            "Status: {}{}",
            app.status,
            if app.compatibility_warning {
                "  |  fallback models"
            } else {
                ""
            }
        )),
    ];
    frame.render_widget(Paragraph::new(header_lines), outer[0]);

    frame.render_widget(
        Paragraph::new(model_panel_lines(app))
            .block(panel_block("Models", app.focus == Focus::Models))
            .wrap(Wrap { trim: false }),
        sidebar[0],
    );
    frame.render_widget(
        Paragraph::new(thread_panel_lines(app))
            .block(panel_block("Threads", app.focus == Focus::Threads))
            .wrap(Wrap { trim: false }),
        sidebar[1],
    );
    let chat_lines = chat_panel_lines(app);
    let chat_height = content[0].height.saturating_sub(2).max(1);
    app.chat_max_scroll = chat_lines.len().saturating_sub(chat_height as usize) as u16;
    if app.chat_follow {
        app.chat_scroll = app.chat_max_scroll;
    } else {
        app.chat_scroll = app.chat_scroll.min(app.chat_max_scroll);
    }
    frame.render_widget(
        Paragraph::new(chat_lines)
            .block(panel_block("Chat", app.focus == Focus::Chat))
            .scroll((app.chat_scroll, 0))
            .wrap(Wrap { trim: false }),
        content[0],
    );
    app.composer
        .set_block(panel_block("Composer", app.focus == Focus::Composer));
    frame.render_widget(&app.composer, content[1]);

    frame.render_widget(
        Paragraph::new(help_lines())
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::DarkGray)),
        outer[2],
    );
}

fn model_panel_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!("Search: {}", app.model_query))];
    for (index, model) in app.filtered_models().into_iter().enumerate() {
        let selected = index == app.model_cursor && app.focus == Focus::Models;
        let current = app.current_model_id() == Some(model.id.as_str());
        let prefix = if selected { ">" } else { " " };
        let current_marker = if current { " *" } else { "" };
        let status = if model.compatible { "" } else { " (fallback)" };
        lines.push(Line::from(Span::styled(
            format!("{prefix} {}{current_marker}{status}", model.label),
            if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            },
        )));
    }
    lines
}

fn thread_panel_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!("Search: {}", app.thread_query))];
    for (index, thread) in app.filtered_threads().into_iter().enumerate() {
        let selected = index == app.thread_cursor && app.focus == Focus::Threads;
        let current = app.current_thread_id.as_deref() == Some(thread.id.as_str());
        let prefix = if selected { ">" } else { " " };
        let current_marker = if current { " *" } else { "" };
        let suffix = if matches!(thread.status, ThreadStatus::Continued) {
            " (continued)"
        } else {
            ""
        };
        lines.push(Line::from(Span::styled(
            format!("{prefix} {}{current_marker}{suffix}", thread.title),
            if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            },
        )));
    }
    lines
}

fn chat_panel_lines(app: &App) -> Vec<Line<'static>> {
    if app.messages.is_empty() {
        return vec![
            Line::from("No messages yet."),
            Line::from("Type in the composer and press Enter."),
        ];
    }

    let mut lines = Vec::new();
    for message in &app.messages {
        let (label, style) = match message.role {
            MessageRole::Assistant => (
                "Assistant",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            MessageRole::User => (
                "You",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        };
        lines.push(Line::from(Span::styled(format!("{label}:"), style)));
        for line in message.content.lines() {
            lines.push(Line::from(format!("  {line}")));
        }
        if message.content.is_empty() {
            lines.push(Line::from("  "));
        }
        lines.push(Line::from(""));
    }
    lines
}

fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("Enter send  |  Shift+Enter newline  |  Tab move focus  |  Ctrl+N new chat"),
        Line::from(
            "Ctrl+J threads  |  Ctrl+K models  |  Ctrl+M composer  |  Ctrl+L logout  |  Esc stop stream",
        ),
    ]
}

fn key_event_to_input(key_event: KeyEvent) -> Input {
    let key = match key_event.code {
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::Tab,
        KeyCode::Char(value) => Key::Char(value),
        _ => Key::Null,
    };
    Input {
        alt: key_event.modifiers.contains(KeyModifiers::ALT),
        ctrl: key_event.modifiers.contains(KeyModifiers::CONTROL),
        key,
        shift: key_event.modifiers.contains(KeyModifiers::SHIFT),
    }
}

fn panel_block(title: &str, focused: bool) -> Block<'static> {
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(style)
        .title(Span::styled(
            format!(" {title} "),
            Style::default().add_modifier(Modifier::BOLD),
        ))
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use codexchat_core::{codex::CodexClient, config::AppPaths, history::ThreadStore};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use serde_json::Value;
    use tempfile::tempdir;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader, duplex},
        sync::{mpsc, oneshot},
    };

    use super::{App, handle_key_event};

    #[tokio::test]
    async fn esc_interrupts_active_turn() {
        let (client_io, server_io) = duplex(4096);
        let (client_reader, client_writer) = tokio::io::split(client_io);
        let (server_reader, mut server_writer) = tokio::io::split(server_io);
        let (interrupt_tx, interrupt_rx) = oneshot::channel();

        tokio::spawn(async move {
            let mut interrupt_tx = Some(interrupt_tx);
            let mut reader = BufReader::new(server_reader);
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).await.ok() == Some(0) {
                    break;
                }
                let request: Value = serde_json::from_str(&line).expect("json");
                if request.get("method").and_then(Value::as_str) != Some("turn/interrupt") {
                    continue;
                }
                if let Some(tx) = interrupt_tx.take() {
                    tx.send(request.clone()).expect("interrupt request");
                }
                let id = request
                    .get("id")
                    .and_then(Value::as_u64)
                    .expect("request id");
                server_writer
                    .write_all(
                        format!("{}\n", serde_json::json!({ "id": id, "result": {} })).as_bytes(),
                    )
                    .await
                    .expect("interrupt response");
                break;
            }
        });

        let client = Arc::new(CodexClient::from_parts(client_reader, client_writer));
        let temp = tempdir().expect("tempdir");
        let store = ThreadStore::new(AppPaths::from_root(temp.path().join(".codexchat")));
        let mut app = App::new(Default::default(), Vec::new());
        app.active_remote_thread_id = Some("thr_123".into());
        app.active_turn_id = Some("turn_123".into());
        app.stream_task = Some(tokio::spawn(async {}));
        let (event_tx, _) = mpsc::unbounded_channel();

        let should_quit = handle_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            client,
            &store,
            &mut app,
            &event_tx,
        )
        .await
        .expect("handle esc");

        assert!(!should_quit);
        assert_eq!(app.status, "Stopping stream...");

        let request: Value = tokio::time::timeout(std::time::Duration::from_secs(1), interrupt_rx)
            .await
            .expect("interrupt request received")
            .expect("interrupt payload");
        assert_eq!(
            request.pointer("/params/threadId").and_then(Value::as_str),
            Some("thr_123")
        );
        assert_eq!(
            request.pointer("/params/turnId").and_then(Value::as_str),
            Some("turn_123")
        );
    }
}
