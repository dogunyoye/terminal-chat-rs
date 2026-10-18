# terminal-chat-rs

A terminal (TUI) based chat application built on WebSockets.

Host a room from one terminal and join it from others; every participant sees a
shared, live message log.

## Features

- Full-screen TUI built with [ratatui](https://ratatui.rs) + crossterm
- Host a chat server or join an existing one, from a single binary
- Nickname-based rooms with join/leave notifications
- JSON wire protocol over WebSockets (async, powered by tokio + tokio-tungstenite)
- Join by DNS name over `ws://` or TLS `wss://`; optionally host `wss://` directly
- Word-wrapped message log with scrollback, input editing, and a status bar

## Usage

```sh
cargo run
```

Use `Up`/`Down` to choose **Host a server** or **Join a server** and press
`Enter`. Fill in the port/address and a nickname, then start chatting.

To connect multiple clients, run the binary in several terminals and join
`127.0.0.1:<port>` (the default port is `8080`).

### Hosting with TLS

The **Host** form has two optional fields for a PEM certificate and private key.
Fill in both (and leave them empty for plaintext `ws://`) to serve `wss://`
directly:

- `TLS certificate (PEM, optional)` — e.g. `fullchain.pem`
- `TLS private key (PEM, optional)` — e.g. `privkey.pem`

When set, the server accepts TLS connections and the host joins its own room
over `wss://127.0.0.1:<port>`. Certificate verification is skipped only for that
loopback connection, so a self-signed certificate works for hosting.

## Connecting over the internet

The **Join** field accepts any of:

- `host:port` (assumes `ws://`), e.g. `192.168.1.20:8080`
- `ws://host[:port][/path]`
- `wss://host[:port][/path]` — encrypted, uses the system/webpki trust roots

Hostnames are resolved with DNS, so you can join by name (e.g.
`wss://chat.example.com`).

Hosting binds `0.0.0.0:<port>` and speaks plaintext `ws://` by default, which is
reachable from the internet when the machine has a public IP or a forwarded port.
Supply a certificate and key in the **Host** form to serve `wss://` directly;
otherwise put the host behind a TLS-terminating reverse proxy such as nginx or
Caddy (or a tunnel like Cloudflare Tunnel).

This is a client/server application: there is no NAT traversal, relaying, or
hole-punching, so two peers behind separate home networks cannot connect to each
other directly without one of them being reachable.

## Controls

| Context | Keys | Action |
| --- | --- | --- |
| Menu | `Up`/`Down`, `Enter` | Select an option |
| Form | `Tab`/`Shift+Tab` | Move between fields |
| Form | `Enter` / `Esc` | Submit / cancel |
| Chat | `Enter` | Send message |
| Chat | `PageUp`/`PageDown`, `Up`/`Down` | Scroll the log |
| Chat | `Esc` | Leave and return to the menu |
| Anywhere | `Ctrl-C` | Quit |

### Commands

Type a message beginning with `/` to run a command:

- `/help` – list commands
- `/users` – list participants
- `/leave` – leave the current room
- `/quit` – exit the application

## Project layout

```
src/
  main.rs        Entry point: sets up the terminal and runs the app
  app.rs         Application state machine and event loop
  event.rs       Events delivered from the networking layer
  protocol.rs    JSON chat protocol (client/server messages)
  sanitize.rs    Strips control characters from untrusted text before display
  input.rs       Single-line text input with cursor handling
  ui.rs          ratatui rendering for every screen
  net/
    server.rs    Async WebSocket chat server (rooms, broadcast, optional TLS)
    client.rs    Async WebSocket chat client (ws:// + wss://)
```

## Development

```sh
cargo build          # compile
cargo test           # run unit + networking tests
cargo clippy --all-targets
cargo fmt
```

## License

Released under the [MIT License](LICENSE).
