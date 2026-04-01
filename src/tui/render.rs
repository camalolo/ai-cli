use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use super::theme::{spinner_frame, Theme};
use super::types::{App, AppState, ChatMessage, TuiTerminal};

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max_chars).collect::<String>())
    }
}

// ── Root render ──────────────────────────────────────────────────────────────

pub(crate) fn render(f: &mut Frame, app: &mut App) {
    let theme = Theme::default();
    let size = f.area();

    // Fill root background
    f.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        size,
    );

    // Optional sidebar when terminal is wide enough
    let show_sidebar = size.width > 120;
    let (main_area, sidebar_area) = if show_sidebar {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(80), Constraint::Length(42)])
            .split(size);
        (chunks[0], Some(chunks[1]))
    } else {
        (size, None)
    };

    // Horizontal padding (2 px each side)
    let padded_main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(main_area);
    let content_area = padded_main[1];

    // Vertical split: chat · input · status
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),     // chat
            Constraint::Length(12), // input
            Constraint::Length(1),  // status bar
        ])
        .split(content_area);

    render_chat(f, app, &theme, vertical[0]);
    render_input(f, app, &theme, vertical[1]);
    render_status_bar(f, app, &theme, vertical[2]);

    if let Some(sidebar) = sidebar_area {
        render_sidebar(f, app, &theme, sidebar);
    }
}

// ── Chat area ────────────────────────────────────────────────────────────────

fn render_chat(f: &mut Frame, app: &mut App, theme: &Theme, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let width = area.width.saturating_sub(2) as usize;

    if app.messages.is_empty() {
        // Welcome screen
        let mid = area.height / 2;
        for _ in 0..mid.saturating_sub(4) {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(Span::styled(
            "Welcome to AI CLI",
            theme.primary_style().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            app.model.clone(),
            theme.text_style(),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Enter a message to begin  \u{2022}  Ctrl+C to cancel  \u{2022}  Ctrl+D to quit",
            theme.muted_style(),
        )));
    } else {
        for msg in app.messages.iter().filter(|msg| {
            !matches!(msg, ChatMessage::ToolCall { .. } | ChatMessage::ToolResult { .. })
        }) {
            match msg {
                ChatMessage::User { content } => {
                    lines.push(Line::raw(""));
                    let wrapped =
                        word_wrap_line(content, width.saturating_sub(4), theme.text_style());
                    for wl in &wrapped {
                        let mut spans = vec![Span::styled(
                            "\u{2503} ",
                            Style::default().fg(theme.primary),
                        )];
                        spans.extend(wl.spans.iter().cloned());
                        lines.push(Line::from(spans));
                    }
                    lines.push(Line::raw(""));
                }
                ChatMessage::Assistant {
                    content,
                    is_streaming,
                } => {
                    lines.push(Line::raw(""));
                    let styled = render_markdown(content, width.saturating_sub(3), theme);
                    for sl in styled {
                        let mut spans = vec![Span::raw("   ")];
                        spans.extend(sl.spans.iter().cloned());
                        lines.push(Line::from(spans));
                    }
                    if *is_streaming {
                        let frame = spinner_frame(app.tick_counter);
                        lines.push(Line::from(vec![
                            Span::raw("   "),
                            Span::styled(frame, theme.accent_style()),
                        ]));
                    }
                    lines.push(Line::raw(""));
                }
                ChatMessage::ToolCall { name, args } => {
                    let frame = spinner_frame(app.tick_counter);
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {frame} "), theme.accent_style()),
                        Span::styled(format!("calling {name}"), theme.muted_style()),
                    ]));
                    if !args.is_empty() {
                        let preview = truncate_str(args, 80);
                        lines.push(Line::from(Span::styled(
                            format!("    {preview}"),
                            Style::default().fg(theme.text_muted),
                        )));
                    }
                }
                ChatMessage::ToolResult { name, result } => {
                    let preview = truncate_str(result, 120);
                    lines.push(Line::from(vec![
                        Span::styled("  \u{2713} ", theme.success_style()),
                        Span::styled(name.to_string(), theme.muted_style()),
                    ]));
                    for rl in preview.lines().take(3) {
                        lines.push(Line::from(Span::styled(
                            format!("    {rl}"),
                            Style::default().fg(theme.text_muted),
                        )));
                    }
                }
                ChatMessage::Error { message } => {
                    lines.push(Line::raw(""));
                    let wrapped =
                        word_wrap_line(message, width.saturating_sub(4), theme.error_style());
                    for wl in &wrapped {
                        let mut spans =
                            vec![Span::styled("\u{2503} ", Style::default().fg(theme.error))];
                        spans.extend(wl.spans.iter().cloned());
                        lines.push(Line::from(spans));
                    }
                    lines.push(Line::raw(""));
                }
                ChatMessage::Info { message } => {
                    lines.push(Line::from(Span::styled(
                        format!("  {message}"),
                        theme.info_style(),
                    )));
                }
            }
        }
    }

    // Scroll handling
    let total_lines = lines.len() as u16;
    let visible_height = area.height;
    let max_scroll = total_lines.saturating_sub(visible_height);

    if app.auto_scroll {
        app.scroll_offset = max_scroll;
    }
    app.scroll_offset = app.scroll_offset.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .style(Style::default().bg(theme.background))
        .scroll((app.scroll_offset, 0));

    f.render_widget(paragraph, area);

    // Themed scrollbar
    if total_lines > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(total_lines as usize).position(app.scroll_offset as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(theme.muted_style()),
            area,
            &mut scrollbar_state,
        );
    }
}

// ── Markdown helpers ─────────────────────────────────────────────────────────

fn render_markdown(content: &str, _max_width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;

    for raw_line in content.lines() {
        if raw_line.starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                theme.success_style(),
            )));
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                theme.success_style(),
            )));
            continue;
        }

        if raw_line.starts_with("### ") {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                theme.accent_style().add_modifier(Modifier::BOLD),
            )));
        } else if raw_line.starts_with("## ") {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                theme.accent_style().add_modifier(Modifier::BOLD),
            )));
        } else if raw_line.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                theme.accent_style().add_modifier(Modifier::BOLD),
            )));
        } else if raw_line.contains('`') {
            lines.push(render_inline_code(raw_line, theme));
        } else {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                theme.text_style(),
            )));
        }
    }

    if lines.is_empty() {
        lines.push(Line::raw(""));
    }

    lines
}

fn render_inline_code(line: &str, theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut parts = line.split('`');
    let mut in_code = false;

    while let Some(part) = parts.next() {
        if in_code {
            spans.push(Span::styled(
                format!("`{part}`"),
                Style::default()
                    .fg(theme.success)
                    .bg(theme.background_element),
            ));
        } else if !part.is_empty() {
            spans.push(Span::styled(part.to_string(), theme.text_style()));
        }
        in_code = !in_code;
    }

    if spans.is_empty() {
        Line::raw("")
    } else {
        Line::from(spans)
    }
}

// ── Input area ───────────────────────────────────────────────────────────────

fn render_input(f: &mut Frame, app: &mut App, theme: &Theme, area: Rect) {
    let border_color = match &app.state {
        AppState::Idle => theme.primary,
        AppState::Streaming => theme.accent,
        AppState::WaitingConfirmation { .. } => theme.warning,
        AppState::ProcessingTools => theme.accent,
    };

    match &app.state {
        AppState::WaitingConfirmation { prompt, .. } => {
            let lines = vec![
                Line::from(Span::styled(
                    format!("\u{2503} {prompt}"),
                    Style::default().fg(theme.warning),
                )),
                Line::raw(""),
                Line::from(vec![
                    Span::styled("  [y] Yes  ", theme.success_style()),
                    Span::styled("  [n] No  ", theme.error_style()),
                    Span::styled("  [a] Always approve", theme.muted_style()),
                ]),
            ];
            let block = Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(theme.background_element));
            let paragraph = Paragraph::new(lines).block(block);
            f.render_widget(paragraph, area);
        }
        _ => {
            let block = Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(theme.background_element));

            app.input.set_block(block);

            let input_area = Rect {
                height: area.height.saturating_sub(1),
                ..area
            };
            let info_area = Rect {
                y: area.y + area.height.saturating_sub(1),
                height: 1,
                ..area
            };

            f.render_widget(&app.input, input_area);

            // Set cursor position inside textarea only when idle
            if matches!(app.state, AppState::Idle) {
                let (cursor_y, cursor_x) = app.input.cursor();
                let safe_y = (cursor_y as u16).min(input_area.height.saturating_sub(1));
                let safe_x = (cursor_x as u16).min(input_area.width.saturating_sub(1));
                f.set_cursor_position((
                    input_area.x.saturating_add(safe_x),
                    input_area.y.saturating_add(safe_y),
                ));
            }

            // Model info line at the bottom
            let model_info = Line::from(vec![Span::styled(
                format!("  {} ", app.model),
                theme.muted_style(),
            )]);
            f.render_widget(
                Paragraph::new(model_info).style(Style::default().bg(theme.background)),
                info_area,
            );
        }
    }
}

// ── Status bar ───────────────────────────────────────────────────────────────

fn render_status_bar(f: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    let max_cwd_len = (area.width as usize).saturating_sub(30);
    let cwd_display = if cwd.len() > max_cwd_len {
        format!("{}...", truncate_str(&cwd, max_cwd_len - 3))
    } else {
        cwd
    };

    let left = Span::styled(format!(" {cwd_display}"), theme.muted_style());

    let right: Vec<Span<'static>> = match &app.state {
        AppState::Streaming => vec![
            Span::styled(
                spinner_frame(app.tick_counter).to_string(),
                theme.accent_style(),
            ),
            Span::styled(" Streaming", theme.accent_style()),
        ],
        AppState::ProcessingTools => vec![
            Span::styled(
                spinner_frame(app.tick_counter).to_string(),
                theme.accent_style(),
            ),
            Span::styled(" Processing", theme.accent_style()),
        ],
        AppState::Idle => {
            let msg_count = app.messages.len();
            vec![
                Span::styled(format!("{msg_count} messages"), theme.muted_style()),
                Span::styled(
                    "  \u{2022}  Enter to send  \u{2022}  Ctrl+D quit",
                    Style::default().fg(theme.text_muted),
                ),
            ]
        }
        AppState::WaitingConfirmation { .. } => {
            vec![Span::styled("Awaiting confirmation", theme.warning_style())]
        }
    };

    let right_text: String = right.iter().map(|s| s.content.as_ref()).collect();
    let right_len = right_text.len() as u16;
    let padding = area
        .width
        .saturating_sub(cwd_display.len() as u16)
        .saturating_sub(right_len)
        .saturating_sub(2);

    let mut spans: Vec<Span<'static>> = vec![left];
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding as usize)));
    }
    spans.extend(right);

    let status =
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.background_panel));

    f.render_widget(status, area);
}

// ── Sidebar ──────────────────────────────────────────────────────────────────

fn render_sidebar(f: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let block = Block::default()
        .style(Style::default().bg(theme.background_panel).fg(theme.text))
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme.border));

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "  Session",
            theme.primary_style().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            format!("  Model: {}", app.model),
            theme.muted_style(),
        )),
        Line::from(Span::styled(
            format!("  Messages: {}", app.messages.len()),
            theme.muted_style(),
        )),
        Line::raw(""),
    ];

    // State indicator
    match &app.state {
        AppState::Streaming => {
            lines.push(Line::from(vec![
                Span::styled("  \u{25cf} ", theme.accent_style()),
                Span::styled("Streaming", theme.accent_style()),
            ]));
        }
        AppState::ProcessingTools => {
            lines.push(Line::from(vec![
                Span::styled("  \u{25cf} ", theme.warning_style()),
                Span::styled("Processing", theme.warning_style()),
            ]));
        }
        AppState::Idle => {
            lines.push(Line::from(vec![
                Span::styled("  \u{25cf} ", theme.success_style()),
                Span::styled("Ready", theme.success_style()),
            ]));
        }
        AppState::WaitingConfirmation { .. } => {
            lines.push(Line::from(vec![
                Span::styled("  \u{25cf} ", theme.warning_style()),
                Span::styled("Confirming", theme.warning_style()),
            ]));
        }
    }

    // Fill remaining space, then add version footer
    let remaining = area.height as usize - lines.len();
    for _ in 0..remaining.saturating_sub(1) {
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(vec![
        Span::styled("  \u{25cf} ", theme.success_style()),
        Span::styled("AI CLI", theme.muted_style()),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

// ── Word-wrap helper (unchanged) ─────────────────────────────────────────────

pub(crate) fn word_wrap_line(text: &str, max_width: usize, style: Style) -> Vec<Line<'static>> {
    let mut result = Vec::new();
    let max_width = max_width.max(10);
    for paragraph in text.split('\n') {
        if paragraph.chars().count() <= max_width {
            result.push(Line::from(Span::styled(paragraph.to_string(), style)));
            continue;
        }
        let mut current_line = String::new();
        let mut current_len: usize = 0;
        for word in paragraph.split(' ') {
            let word_len = word.chars().count();
            if word_len > max_width {
                if !current_line.is_empty() {
                    result.push(Line::from(Span::styled(current_line.clone(), style)));
                    current_line.clear();
                    current_len = 0;
                }
                let mut chunk_start = 0;
                for (i, _ch) in word.char_indices() {
                    if i >= chunk_start && current_len == max_width {
                        let chunk: String = word[chunk_start..i].to_string();
                        result.push(Line::from(Span::styled(chunk, style)));
                        chunk_start = i;
                        current_len = 0;
                    }
                    current_len += 1;
                }
                if chunk_start < word.len() {
                    let chunk = word[chunk_start..].to_string();
                    current_line = chunk.clone();
                    current_len = chunk.chars().count();
                }
            } else if current_line.is_empty() {
                current_line = word.to_string();
                current_len = word_len;
            } else if current_len + 1 + word_len <= max_width {
                current_line.push(' ');
                current_line.push_str(word);
                current_len += 1 + word_len;
            } else {
                result.push(Line::from(Span::styled(current_line.clone(), style)));
                current_line = word.to_string();
                current_len = word_len;
            }
        }
        if !current_line.is_empty() {
            result.push(Line::from(Span::styled(current_line, style)));
        }
    }
    if result.is_empty() {
        result.push(Line::from(Span::styled(String::new(), style)));
    }
    result
}

// ── Terminal lifecycle (unchanged) ───────────────────────────────────────────

pub(crate) fn init_terminal() -> Result<TuiTerminal> {
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let terminal = ratatui::Terminal::new(backend)?;
    Ok(terminal)
}

pub(crate) fn restore_terminal(mut terminal: TuiTerminal) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
