use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use crate::event::AppEvent;
use crate::input::InputState;
use crate::net::{self, ClientHandle, ServerHandle};
use crate::protocol::{ClientMessage, ServerMessage};
use crate::sanitize;
use crate::ui;

pub const HOME_ITEMS: [&str; 3] = ["Host a server", "Join a server", "Quit"];
const MAX_NICKNAME_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Host,
    Join,
}

#[derive(Debug)]
pub struct Field {
    pub label: &'static str,
    pub input: InputState,
}

impl Field {
    fn new(label: &'static str, value: &str) -> Self {
        Field {
            label,
            input: InputState::new(value),
        }
    }
}

#[derive(Debug)]
pub struct Form {
    pub kind: FormKind,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEntry {
    Message {
        nickname: String,
        content: String,
        timestamp: String,
    },
    System(String),
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Form,
    Chat,
}

struct Session {
    client: ClientHandle,
    server: Option<ServerHandle>,
}

pub struct App {
    events: mpsc::UnboundedSender<AppEvent>,
    pub screen: Screen,
    pub selected: usize,
    pub form: Option<Form>,
    pub entries: Vec<ChatEntry>,
    pub input: InputState,
    pub users: Vec<String>,
    pub nickname: String,
    pub connection_label: String,
    pub connected: bool,
    pub scroll_back: usize,
    pub should_quit: bool,
    session: Option<Session>,
}

impl App {
    pub fn new(events: mpsc::UnboundedSender<AppEvent>) -> Self {
        App {
            events,
            screen: Screen::Home,
            selected: 0,
            form: None,
            entries: Vec::new(),
            input: InputState::default(),
            users: Vec::new(),
            nickname: String::new(),
            connection_label: String::new(),
            connected: false,
            scroll_back: 0,
            should_quit: false,
            session: None,
        }
    }

    /// Run the event loop until the user quits.
    pub async fn run(
        &mut self,
        terminal: &mut ratatui::DefaultTerminal,
        mut events_rx: mpsc::UnboundedReceiver<AppEvent>,
    ) -> Result<()> {
        let mut input_stream = EventStream::new();

        loop {
            terminal.draw(|frame| ui::draw(frame, self))?;

            if self.should_quit {
                break;
            }

            tokio::select! {
                maybe_event = input_stream.next() => match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        self.handle_key(key).await;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => break,
                },
                Some(event) = events_rx.recv() => self.handle_event(event),
            }
        }

        Ok(())
    }

    async fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        match self.screen {
            Screen::Home => self.handle_home_key(key),
            Screen::Form => self.handle_form_key(key).await,
            Screen::Chat => self.handle_chat_key(key),
        }
    }

    fn handle_home_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                self.selected = self.selected.checked_sub(1).unwrap_or(HOME_ITEMS.len() - 1);
            }
            KeyCode::Down => {
                self.selected = (self.selected + 1) % HOME_ITEMS.len();
            }
            KeyCode::Enter => match self.selected {
                0 => self.open_form(FormKind::Host),
                1 => self.open_form(FormKind::Join),
                _ => self.should_quit = true,
            },
            KeyCode::Char('q') => self.should_quit = true,
            _ => {}
        }
    }

    async fn handle_form_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.form = None;
                self.screen = Screen::Home;
            }
            KeyCode::Enter => self.submit_form().await,
            KeyCode::Tab | KeyCode::Down => self.move_focus(1),
            KeyCode::BackTab | KeyCode::Up => self.move_focus(-1),
            _ => {
                if let Some(form) = self.form.as_mut() {
                    let input = &mut form.fields[form.focus].input;
                    edit_input(input, key);
                }
            }
        }
    }

    fn handle_chat_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.leave_to_home(),
            KeyCode::Enter => {
                let text = self.input.value().trim().to_string();
                self.input.clear();

                if !text.is_empty() {
                    self.submit_input(text);
                }
            }
            KeyCode::PageUp => self.scroll_back = self.scroll_back.saturating_add(5),
            KeyCode::PageDown => self.scroll_back = self.scroll_back.saturating_sub(5),
            KeyCode::Up => self.scroll_back = self.scroll_back.saturating_add(1),
            KeyCode::Down => self.scroll_back = self.scroll_back.saturating_sub(1),
            _ => edit_input(&mut self.input, key),
        }
    }

    fn open_form(&mut self, kind: FormKind) {
        let nickname = default_nickname();

        let fields = match kind {
            FormKind::Host => vec![
                Field::new("Port", "8080"),
                Field::new("Nickname", &nickname),
                Field::new("TLS certificate (PEM, optional)", ""),
                Field::new("TLS private key (PEM, optional)", ""),
            ],
            FormKind::Join => vec![
                Field::new("Server (host:port or wss://url)", "127.0.0.1:8080"),
                Field::new("Nickname", &nickname),
            ],
        };

        self.form = Some(Form {
            kind,
            fields,
            focus: 0,
            error: None,
        });
        self.screen = Screen::Form;
    }

    fn move_focus(&mut self, delta: i32) {
        if let Some(form) = self.form.as_mut() {
            let len = form.fields.len() as i32;
            form.focus = (form.focus as i32 + delta).rem_euclid(len) as usize;
        }
    }

    async fn submit_form(&mut self) {
        let Some(form) = self.form.as_ref() else {
            return;
        };

        let kind = form.kind;
        let values: Vec<String> = form
            .fields
            .iter()
            .map(|field| field.input.value().trim().to_string())
            .collect();

        let outcome = match kind {
            FormKind::Host => {
                self.start_host(&values[0], &values[1], &values[2], &values[3])
                    .await
            }
            FormKind::Join => self.start_join(&values[0], &values[1]).await,
        };

        match outcome {
            Ok(()) => {
                self.form = None;
                self.screen = Screen::Chat;
            }
            Err(error) => {
                if let Some(form) = self.form.as_mut() {
                    form.error = Some(error);
                }
            }
        }
    }

    async fn start_host(
        &mut self,
        port: &str,
        nickname: &str,
        cert: &str,
        key: &str,
    ) -> Result<(), String> {
        let port: u16 = port
            .parse()
            .map_err(|_| "port must be a number between 0 and 65535".to_string())?;
        let nickname = validate_nickname(nickname)?;

        let cert = cert.trim();
        let key = key.trim();

        if cert.is_empty() != key.is_empty() {
            return Err("provide both a TLS certificate and private key, or neither".to_string());
        }

        let tls = !cert.is_empty();

        let server = if tls {
            net::server::listen_tls(port, net::server::TlsConfig::new(cert, key)).await
        } else {
            net::server::listen(port).await
        }
        .map_err(|error| format!("{:#}", error))?;

        let listen_addr = server.addr;
        let port = listen_addr.port();

        let client = if server.tls {
            let target = format!("wss://127.0.0.1:{}", port);
            net::client::connect_untrusted(&target, nickname.clone(), self.events.clone())
                .await
                .map_err(|error| format!("{:#}", error))?
        } else {
            let target = format!("ws://127.0.0.1:{}", port);
            net::client::connect(&target, nickname.clone(), self.events.clone())
                .await
                .map_err(|error| format!("{:#}", error))?
        };

        let scheme = if server.tls { "wss" } else { "ws" };

        self.begin_session(
            Session {
                client,
                server: Some(server),
            },
            nickname,
            format!("Hosting {}://{}", scheme, listen_addr),
        );

        Ok(())
    }

    async fn start_join(&mut self, target: &str, nickname: &str) -> Result<(), String> {
        let target = target.trim();
        let nickname = validate_nickname(nickname)?;

        let client = net::client::connect(target, nickname.clone(), self.events.clone())
            .await
            .map_err(|error| format!("{:#}", error))?;

        self.begin_session(
            Session {
                client,
                server: None,
            },
            nickname,
            format!("Connected to {}", target),
        );

        Ok(())
    }

    fn begin_session(&mut self, session: Session, nickname: String, label: String) {
        self.session = Some(session);
        self.nickname = nickname;
        self.connection_label = label;
        self.connected = true;
        self.entries.clear();
        self.users.clear();
        self.input.clear();
        self.scroll_back = 0;
        self.screen = Screen::Chat;
    }

    fn submit_input(&mut self, text: String) {
        if let Some(command) = text.strip_prefix('/') {
            self.run_command(command);
        } else {
            self.send_chat(text);
        }
    }

    fn run_command(&mut self, command: &str) {
        match command {
            "help" => self.entries.push(ChatEntry::System(
                "Commands: /help /users /leave /quit".to_string(),
            )),
            "users" => {
                let names = if self.users.is_empty() {
                    "no other users".to_string()
                } else {
                    self.users.join(", ")
                };
                self.entries
                    .push(ChatEntry::System(format!("Users: {}", names)));
            }
            "leave" => self.leave_to_home(),
            "quit" | "exit" => self.should_quit = true,
            other => self
                .entries
                .push(ChatEntry::Error(format!("unknown command: /{}", other))),
        }
    }

    fn send_chat(&mut self, content: String) {
        let Some(session) = self.session.as_ref() else {
            self.entries
                .push(ChatEntry::Error("not connected".to_string()));
            return;
        };

        if let Err(error) = session.client.send(ClientMessage::Chat { content }) {
            self.entries.push(ChatEntry::Error(error.to_string()));
        }
    }

    fn leave_to_home(&mut self) {
        if let Some(session) = self.session.take() {
            if let Some(server) = session.server {
                server.shutdown();
            }
        }

        self.connected = false;
        self.users.clear();
        self.input.clear();
        self.scroll_back = 0;
        self.selected = 0;
        self.screen = Screen::Home;
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Server(message) => self.handle_server_message(message),
            AppEvent::Disconnected { reason } => {
                self.connected = false;
                self.connection_label = format!("Disconnected: {}", reason);
                self.entries
                    .push(ChatEntry::System(format!("Connection closed: {}", reason)));
            }
            AppEvent::Error(message) => self
                .entries
                .push(ChatEntry::Error(sanitize::sanitize(&message))),
        }
    }

    fn handle_server_message(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome { nickname, users } => {
                let nickname = sanitize::sanitize(&nickname);
                self.nickname = nickname.clone();
                self.users = users.iter().map(|user| sanitize::sanitize(user)).collect();
                self.entries
                    .push(ChatEntry::System(format!("You joined as {}", nickname)));
            }
            ServerMessage::Joined { nickname } => {
                let nickname = sanitize::sanitize(&nickname);
                if !self.users.contains(&nickname) {
                    self.users.push(nickname.clone());
                }
                self.entries
                    .push(ChatEntry::System(format!("{} joined", nickname)));
            }
            ServerMessage::Left { nickname } => {
                let nickname = sanitize::sanitize(&nickname);
                self.users.retain(|user| user != &nickname);
                self.entries
                    .push(ChatEntry::System(format!("{} left", nickname)));
            }
            ServerMessage::Chat {
                nickname,
                content,
                timestamp,
            } => {
                self.entries.push(ChatEntry::Message {
                    nickname: sanitize::sanitize(&nickname),
                    content: sanitize::sanitize(&content),
                    timestamp: sanitize::sanitize(&timestamp),
                });
            }
            ServerMessage::Error { message } => {
                self.entries
                    .push(ChatEntry::Error(sanitize::sanitize(&message)));
            }
        }
    }
}

fn edit_input(input: &mut InputState, key: KeyEvent) {
    match key.code {
        KeyCode::Backspace => input.backspace(),
        KeyCode::Delete => input.delete(),
        KeyCode::Left => input.move_left(),
        KeyCode::Right => input.move_right(),
        KeyCode::Home => input.move_home(),
        KeyCode::End => input.move_end(),
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            input.insert_char(c);
        }
        _ => {}
    }
}

fn validate_nickname(nickname: &str) -> Result<String, String> {
    let nickname = nickname.trim();

    if nickname.is_empty() {
        return Err("nickname cannot be empty".to_string());
    }

    if nickname.chars().count() > MAX_NICKNAME_LEN {
        return Err(format!(
            "nickname must be {} characters or fewer",
            MAX_NICKNAME_LEN
        ));
    }

    if sanitize::sanitize(nickname) != nickname {
        return Err("nickname contains invalid characters".to_string());
    }

    Ok(nickname.to_string())
}

fn default_nickname() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "anonymous".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_nickname() {
        assert_eq!(validate_nickname(" alice ").unwrap(), "alice");
        assert!(validate_nickname("   ").is_err());
        assert!(validate_nickname(&"a".repeat(MAX_NICKNAME_LEN + 1)).is_err());
    }

    #[test]
    fn handles_chat_messages() {
        let (events, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(events);

        app.handle_server_message(ServerMessage::Welcome {
            nickname: "alice".to_string(),
            users: vec!["alice".to_string(), "bob".to_string()],
        });
        assert_eq!(app.users.len(), 2);
        assert_eq!(app.nickname, "alice");

        app.handle_server_message(ServerMessage::Joined {
            nickname: "carol".to_string(),
        });
        assert_eq!(app.users.len(), 3);

        app.handle_server_message(ServerMessage::Left {
            nickname: "bob".to_string(),
        });
        assert_eq!(app.users, vec!["alice".to_string(), "carol".to_string()]);
    }

    #[test]
    fn routes_commands() {
        let (events, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(events);

        app.run_command("help");
        assert!(matches!(app.entries.last(), Some(ChatEntry::System(_))));

        app.run_command("nope");
        assert!(matches!(app.entries.last(), Some(ChatEntry::Error(_))));
    }

    #[test]
    fn edits_input() {
        let mut input = InputState::new("ab");

        edit_input(
            &mut input,
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE),
        );
        assert_eq!(input.value(), "abc");

        edit_input(
            &mut input,
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
        );
        assert_eq!(input.value(), "abc");
    }

    #[test]
    fn sanitizes_text_from_the_server() {
        let (events, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(events);

        app.handle_server_message(ServerMessage::Chat {
            nickname: "ev\u{1b}]0;spoof".to_string(),
            content: "hi\u{1b}]0;pwned\u{7}".to_string(),
            timestamp: "12:00:00".to_string(),
        });

        match app.entries.last() {
            Some(ChatEntry::Message {
                nickname, content, ..
            }) => {
                assert_eq!(nickname, "ev]0;spoof");
                assert_eq!(content, "hi]0;pwned");
            }
            other => panic!("unexpected entry: {other:?}"),
        }
    }

    #[tokio::test]
    async fn hosting_over_tls_reaches_the_chat_screen() {
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let params = rcgen::CertificateParams::new(vec!["127.0.0.1".to_string()]).unwrap();
        let cert = params.self_signed(&key_pair).unwrap();

        let dir =
            std::env::temp_dir().join(format!("terminal-chat-app-tls-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        std::fs::write(&cert_path, cert.pem()).unwrap();
        std::fs::write(&key_path, key_pair.serialize_pem()).unwrap();

        let (events, _rx) = mpsc::unbounded_channel();
        let mut app = App::new(events);

        app.form = Some(Form {
            kind: FormKind::Host,
            fields: vec![
                Field::new("Port", "0"),
                Field::new("Nickname", "alice"),
                Field::new(
                    "TLS certificate (PEM, optional)",
                    cert_path.to_str().unwrap(),
                ),
                Field::new(
                    "TLS private key (PEM, optional)",
                    key_path.to_str().unwrap(),
                ),
            ],
            focus: 0,
            error: None,
        });

        app.submit_form().await;

        assert_eq!(app.form.as_ref().and_then(|form| form.error.clone()), None);
        assert_eq!(app.screen, Screen::Chat);
        assert!(
            app.connection_label.starts_with("Hosting wss://"),
            "unexpected label: {}",
            app.connection_label
        );
    }
}
