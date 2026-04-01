mod theme;
mod types;
mod render;
mod event;
mod llm;
mod tools;

pub(crate) use theme::*;
pub(crate) use types::*;
pub(crate) use render::*;
pub(crate) use event::*;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tokio::time;
use anyhow::Result;
use crossterm::event as crossterm_event;

use crate::chat::ChatManager;

pub async fn run_tui(
    chat_manager: Arc<Mutex<ChatManager>>,
    debug: bool,
    always_approve: Arc<AtomicBool>,
) -> Result<()> {
    let mut terminal = init_terminal()?;

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let tx_input = tx.clone();

    let model_name = {
        let manager = chat_manager.lock().await;
        manager.get_config().model.clone()
    };

    tokio::task::spawn_blocking(move || {
        loop {
            if crossterm_event::poll(std::time::Duration::from_millis(50)).unwrap_or(false) {
                if let Ok(crossterm_event::Event::Key(key)) = crossterm_event::read() {
                    if tx_input.send(AppEvent::Key(key)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut app = App::new(model_name, always_approve);

    let mut tick_interval = time::interval(Duration::from_millis(80));

    loop {
        terminal.draw(|f| render(f, &mut app))?;

        tokio::select! {
            Some(event) = rx.recv() => {
                handle_event(&mut app, event, &chat_manager, &tx, debug).await?;
            }
            _ = tick_interval.tick() => {
                app.tick_counter = app.tick_counter.wrapping_add(1);
            }
        }

        if app.should_quit {
            break;
        }
    }

    restore_terminal(terminal)?;
    Ok(())
}
