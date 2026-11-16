//! Terminal controls for profiles, manual display changes, and verified updates.
//!
//! - [`app`]: state, key handling, and the display and update actions.
//! - [`view`]: drawing that state with ratatui.

mod app;
mod view;

use std::{
    io::{self, stdout},
    time::Duration,
};

use anyhow::{Context, Result};
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;
use view::ui;

struct TerminalSession;

impl Drop for TerminalSession {
    fn drop(&mut self) {
        // Cleanup must also run when entering the screen or creating the backend
        // fails. Ignore cleanup errors so the original failure stays visible.
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
    }
}

pub fn run() -> Result<()> {
    let mut app = App::new()?;
    enable_raw_mode().context("enable raw mode")?;
    let _session = TerminalSession;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;

    run_loop(&mut terminal, &mut app)
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        app.poll_update();
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key.code);
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, display::DisplayState, update};
    use app::{HOME_ACTIONS, Screen, UpdateState};
    use crossterm::event::KeyCode;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::ListState;
    use std::sync::mpsc;

    fn app(update: UpdateState) -> App {
        App {
            screen: Screen::Home,
            home_state: ListState::default(),
            preset_state: ListState::default(),
            native_state: ListState::default(),
            native_modes: Vec::new(),
            config: Config::default(),
            display: DisplayState::default(),
            status: "Ready".into(),
            status_ok: true,
            should_quit: false,
            update,
            confirm_update: false,
        }
    }

    fn render(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| ui(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn available_update_renders_banner_and_confirmation_can_be_dismissed() {
        let mut app = app(UpdateState::Available(update::tests::available_release()));
        assert!(render(&mut app, 90, 30).contains("v9.0.0 is available!"));
        app.on_key(KeyCode::Char('u'));
        assert!(app.confirm_update);
        let popup = render(&mut app, 90, 30);
        assert!(popup.contains("Install vstretch v9.0.0?"));
        assert!(popup.contains("Enter  install    Esc  later"));
        app.on_key(KeyCode::Esc);
        assert!(!app.confirm_update);
        assert!(matches!(app.update, UpdateState::Available(_)));
        assert!(!app.should_quit);
    }

    #[test]
    fn check_results_and_offline_errors_reach_the_banner() {
        let (sender, receiver) = mpsc::channel();
        let mut app = app(UpdateState::Checking(receiver));
        sender
            .send(Ok(Some(update::tests::available_release())))
            .unwrap();
        app.poll_update();
        assert!(matches!(app.update, UpdateState::Available(_)));
        app.update = UpdateState::Failed("Offline".into());
        assert!(render(&mut app, 90, 30).contains("Offline  |  u  retry"));
        app.on_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn installation_blocks_quit_then_shows_restart_notice() {
        let (sender, receiver) = mpsc::channel();
        let mut app = app(UpdateState::Installing {
            version: "9.0.0".into(),
            result: receiver,
        });
        app.on_key(KeyCode::Char('q'));
        assert!(!app.should_quit);
        app.home_state.select(Some(HOME_ACTIONS.len() - 1));
        app.on_key(KeyCode::Enter);
        assert!(!app.should_quit);
        sender.send(Ok(())).unwrap();
        app.poll_update();
        assert!(render(&mut app, 90, 30).contains("Updated to v9.0.0. Quit and reopen vstretch."));
        app.on_key(KeyCode::Char('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn installation_allows_leaving_resolution_pickers() {
        let (_sender, receiver) = mpsc::channel();
        let mut app = app(UpdateState::Installing {
            version: "9.0.0".into(),
            result: receiver,
        });
        for screen in [Screen::Presets, Screen::Native] {
            for key in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Backspace] {
                app.screen = screen;
                app.on_key(key);
                assert!(matches!(app.screen, Screen::Home));
                assert!(!app.should_quit);
                assert!(matches!(app.update, UpdateState::Installing { .. }));
            }
        }
    }

    #[test]
    fn update_rendering_handles_small_terminals() {
        let mut app = app(UpdateState::Available(update::tests::available_release()));
        app.confirm_update = true;
        let popup = render(&mut app, 35, 15);
        assert!(popup.contains("Install vstretch v9.0.0?"));
        assert!(popup.contains("Enter  install    Esc  later"));
    }
}
