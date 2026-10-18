mod app;
mod event;
mod input;
mod net;
mod protocol;
mod ui;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::app::App;

#[tokio::main]
async fn main() -> Result<()> {
    // Install the process-wide rustls crypto provider so `wss://` URLs work.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let mut app = App::new(events_tx);

    let mut terminal = ratatui::try_init()?;
    let result = app.run(&mut terminal, events_rx).await;
    ratatui::try_restore()?;

    result
}
