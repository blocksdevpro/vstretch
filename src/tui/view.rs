//! Draws the TUI from its state. Nothing here changes settings or the display.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use super::app::{App, HOME_ACTIONS, Screen, UpdateState};
use crate::{
    config::presets::POPULAR_STRETCH,
    display::{self, Mode, ModeKind},
};

pub(super) fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, chunks[0]);
    draw_status_panel(f, app, chunks[1]);

    match app.screen {
        Screen::Home => draw_home_menu(f, app, chunks[2]),
        Screen::Presets => draw_presets(f, app, chunks[2]),
        Screen::Native => draw_native_modes(f, app, chunks[2]),
    }

    draw_message(f, app, chunks[3]);
    draw_update(f, app, chunks[4]);
    draw_help(f, app, chunks[5]);
    if app.confirm_update {
        draw_update_confirmation(f, app);
    }
}

fn draw_header(f: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " vstretch ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            concat!("v", env!("CARGO_PKG_VERSION"), "  "),
            Style::default().fg(Color::White),
        ),
        Span::styled("native ↔ stretch", Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta)),
    );
    f.render_widget(title, area);
}

fn draw_update(f: &mut Frame, app: &App, area: Rect) {
    let (text, color) = match &app.update {
        UpdateState::Checking(_) => ("Checking for updates...".into(), Color::DarkGray),
        UpdateState::Current => ("Up to date.  u  check again".into(), Color::DarkGray),
        UpdateState::Available(release) => (
            format!("v{} is available!  u  update", release.version),
            Color::Yellow,
        ),
        UpdateState::Failed(error) => (format!("{error}  |  u  retry"), Color::Red),
        UpdateState::Installing { version, .. } => {
            (format!("Installing v{version}..."), Color::Cyan)
        }
        UpdateState::Installed(version) => (
            format!("Updated to v{version}. Quit and reopen vstretch."),
            Color::Green,
        ),
    };
    f.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(color))
            .block(
                Block::default()
                    .title(" updates ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(color)),
            ),
        area,
    );
}

fn draw_update_confirmation(f: &mut Frame, app: &App) {
    let UpdateState::Available(release) = &app.update else {
        return;
    };
    let area = f.area();
    let width = area.width.min(62);
    let height = area.height.min(8);
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    f.render_widget(Clear, popup);
    let message = format!(
        "Install vstretch v{}?\n\n\
         Download and replace this executable. Your display config stays saved. \
         Reopen vstretch after updating.",
        release.version
    );
    let confirmation = Paragraph::new(message).wrap(Wrap { trim: true }).block(
        Block::default()
            .title(" Update available ")
            .title_bottom(" Enter  install    Esc  later ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );
    f.render_widget(confirmation, popup);
}

fn draw_status_panel(f: &mut Frame, app: &App, area: Rect) {
    let (mode_name, mode_color) = mode_badge(app);
    let current = app
        .display
        .current
        .map(Mode::label)
        .unwrap_or_else(|| "—".into());
    let native = match app.display.native {
        Some(n) => {
            let src = if app.config.native.is_some() {
                "set"
            } else {
                "auto"
            };
            format!("{}  ({})", n.label(), src)
        }
        None => "—".into(),
    };
    let default = app.stretch_label();

    let lines = vec![
        Line::from(vec![
            Span::styled("  Mode      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                mode_name,
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Current   ", Style::default().fg(Color::DarkGray)),
            Span::styled(current, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Native    ", Style::default().fg(Color::DarkGray)),
            Span::styled(native, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Stretch   ", Style::default().fg(Color::DarkGray)),
            Span::styled(default, Style::default().fg(Color::Yellow)),
        ]),
    ];

    let panel = Paragraph::new(lines).block(
        Block::default()
            .title(" display ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(panel, area);
}

fn draw_home_menu(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = HOME_ACTIONS
        .iter()
        .enumerate()
        .map(|(i, (_, label))| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {}. ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(*label),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" actions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" › ");

    f.render_stateful_widget(list, area, &mut app.home_state);
}

fn draw_presets(f: &mut Frame, app: &mut App, area: Rect) {
    let default = (app.config.stretch.width, app.config.stretch.height);

    let items: Vec<ListItem> = POPULAR_STRETCH
        .iter()
        .map(|p| {
            let is_default = default == (p.width, p.height);
            let mut spans = vec![
                Span::styled(
                    p.resolution_label(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("    {:5}", p.aspect),
                    Style::default().fg(Color::DarkGray),
                ),
            ];
            // Reserve the tag column so the saved-preset star lines up on every row.
            let (tag, tag_color) = match p.tag {
                Some("popular") => ("popular", Color::Magenta),
                Some("common") => ("common ", Color::Cyan),
                _ => ("       ", Color::DarkGray),
            };
            spans.push(Span::styled(
                format!("    {tag}"),
                Style::default().fg(tag_color),
            ));
            if is_default {
                spans.push(Span::styled(
                    "  ★",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Select default stretch resolution ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" › ");

    f.render_stateful_widget(list, area, &mut app.preset_state);
}

fn draw_native_modes(f: &mut Frame, app: &mut App, area: Rect) {
    let mut items = Vec::with_capacity(app.native_choice_len());

    let auto_is_default = app.config.native.is_none();
    let starred =
        app.config.native.as_ref().and_then(|n| {
            display::preferred_override_mode(&app.native_modes, n, app.display.panel)
        });
    let mut auto_spans = vec![Span::styled(
        "Auto (panel native)",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(panel) = app.display.panel {
        auto_spans.push(Span::styled(
            format!("    {}", panel.label()),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if auto_is_default {
        auto_spans.push(Span::styled(
            "  ★",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }
    items.push(ListItem::new(Line::from(auto_spans)));

    for mode in &app.native_modes {
        let is_default = starred == Some(*mode);
        let mut spans = vec![
            Span::styled(
                format!("{:>4} × {:<4}", mode.width, mode.height),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("    {:>3}Hz", mode.refresh),
                Style::default().fg(Color::DarkGray),
            ),
        ];
        if app
            .display
            .panel
            .is_some_and(|p| p.size_eq(*mode) && p.refresh == mode.refresh)
        {
            spans.push(Span::styled("    panel", Style::default().fg(Color::Cyan)));
        } else if app.display.current.is_some_and(|c| c == *mode) {
            spans.push(Span::styled("    now", Style::default().fg(Color::Magenta)));
        }
        if is_default {
            spans.push(Span::styled(
                "  ★",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        items.push(ListItem::new(Line::from(spans)));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Select default native resolution ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(" › ");

    f.render_stateful_widget(list, area, &mut app.native_state);
}

fn draw_message(f: &mut Frame, app: &App, area: Rect) {
    let color = if app.status_ok {
        Color::Green
    } else {
        Color::Red
    };
    let msg = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(&app.status, Style::default().fg(color)),
    ]))
    .block(
        Block::default()
            .title(" status ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    f.render_widget(msg, area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let text = match app.screen {
        Screen::Home => "↑↓/jk  move   Enter  select   1–5  quick   r  refresh   q  quit",
        Screen::Presets => "↑↓/jk  move   Enter  set default   Esc  back",
        Screen::Native => "↑↓/jk  move   Enter  set default   r  refresh   Esc  back",
    };
    let help = Paragraph::new(Span::styled(
        format!("  {text}"),
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(help, area);
}

/// The short mode name and colour shown in the status panel.
fn mode_badge(app: &App) -> (&'static str, Color) {
    match app.display.kind(&app.config.stretch) {
        ModeKind::Native => ("NATIVE", Color::Cyan),
        ModeKind::Stretch => ("STRETCH", Color::Magenta),
        ModeKind::Panel => ("PANEL", Color::Blue),
        ModeKind::Other => ("OTHER", Color::DarkGray),
        ModeKind::Unknown => ("UNKNOWN", Color::DarkGray),
    }
}
