use chrono::{DateTime, Utc};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap};
use ratatui::{Frame, prelude::*};

use crate::model::{
    FocusPane, Mode, PullRequestSummary, RateLimitState, WorkflowJobSummary, WorkflowRunSummary,
};

pub fn draw(frame: &mut Frame<'_>, app: &crate::app::App) {
    let size = frame.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(1)])
        .split(size);

    let body = outer[0];
    let status = outer[1];

    if body.width < minimum_width(app.config.mode) {
        let warning = Paragraph::new("Prism needs a wider terminal for this mode.")
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Resize needed"),
            );
        frame.render_widget(warning, body);
        draw_status_bar(frame, status, app);
        return;
    }

    match app.config.mode {
        Mode::Split => draw_split(frame, body, app),
        Mode::Compact => draw_compact(frame, body, app),
    }

    draw_status_bar(frame, status, app);

    if app.show_help {
        draw_overlay(frame, overlay_rect(size, 74, 60), help_paragraph());
    } else if app.detail_open {
        draw_overlay(frame, overlay_rect(size, 84, 70), detail_paragraph(app));
    }
}

fn draw_split(frame: &mut Frame<'_>, area: Rect, app: &crate::app::App) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(54), Constraint::Percentage(46)])
        .split(area);

    draw_actions_table(
        frame,
        sections[0],
        app,
        &mut app.actions_table_state.borrow_mut(),
    );
    draw_prs_table(
        frame,
        sections[1],
        app,
        &mut app.prs_table_state.borrow_mut(),
    );
}

fn draw_compact(frame: &mut Frame<'_>, area: Rect, app: &crate::app::App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    draw_actions_table(
        frame,
        sections[0],
        app,
        &mut app.actions_table_state.borrow_mut(),
    );
    draw_prs_table(
        frame,
        sections[1],
        app,
        &mut app.prs_table_state.borrow_mut(),
    );
}

fn draw_actions_table(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &crate::app::App,
    state: &mut TableState,
) {
    let block = pane_block("Actions", app.focus == FocusPane::Actions);
    let header =
        Row::new(["Repo", "Workflow", "Branch", "State", "Age", "Dur"]).style(header_style(app));
    let rows = if app.state.actions.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from("No workflow runs yet"),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        app.state
            .actions
            .iter()
            .map(|run| {
                Row::new(vec![
                    Cell::from(run.repo.slug()),
                    Cell::from(truncate(&run.workflow_name, 26)),
                    Cell::from(truncate(&run.branch, 14)),
                    Cell::from(Text::styled(
                        format_run_state(run, app),
                        state_style(
                            run.conclusion.as_deref(),
                            &run.status,
                            app.config.ui.no_color,
                        ),
                    )),
                    Cell::from(format_age(run.created_at)),
                    Cell::from(format_run_duration(run)),
                ])
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Min(20),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_prs_table(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &crate::app::App,
    state: &mut TableState,
) {
    let block = pane_block("Pull Requests", app.focus == FocusPane::PullRequests);
    let header = Row::new(["Repo", "#", "Title", "Author", "Review", "CI", "Updated"])
        .style(header_style(app));
    let rows = if app.state.pulls.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from("No pull requests yet"),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        app.state
            .pulls
            .iter()
            .map(|pr| {
                let row_style = if pr.review_requested_for_viewer {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(pr.repo.slug()),
                    Cell::from(format!("#{}", pr.number)),
                    Cell::from(truncate(&pr.title, 28)),
                    Cell::from(truncate(&pr.author, 12)),
                    Cell::from(review_state(pr)),
                    Cell::from(ci_state(pr)),
                    Cell::from(format_age(pr.updated_at)),
                ])
                .style(row_style)
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(6),
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_status_bar(frame: &mut Frame<'_>, area: Rect, app: &crate::app::App) {
    let now = Utc::now();
    let next_countdown = app
        .state
        .next_refresh_at
        .map(|next| (next - now).num_seconds().max(0))
        .map(|secs| format!("{secs}s"))
        .unwrap_or_else(|| "-".to_string());

    let rate = app
        .state
        .rate_limit
        .as_ref()
        .map(rate_limit_label)
        .unwrap_or_else(|| "rate:-".to_string());
    let refreshed = app
        .state
        .last_refresh_at
        .map(|ts| ts.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".to_string());

    let stale = if app.state.errors.is_empty() {
        ""
    } else {
        " | stale"
    };
    let detail_hint = if app.detail_open {
        " | esc:close"
    } else {
        " | l:detail"
    };

    let text = format!(
        " mode:{} | refresh:{} | next:{} | {} | host:{}{}{}",
        app.config.mode, refreshed, next_countdown, rate, app.config.host, stale, detail_hint
    );
    let widget = Paragraph::new(text).style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(widget, area);
}

fn help_paragraph() -> Paragraph<'static> {
    let lines = vec![
        Line::from("Prism keybindings"),
        Line::from(""),
        Line::from(" q       quit"),
        Line::from(" r       force refresh"),
        Line::from(" Tab     switch focus"),
        Line::from(" j / ↓   move down"),
        Line::from(" k / ↑   move up"),
        Line::from(" g / G   jump to top / bottom"),
        Line::from(" l       open Actions drill-down"),
        Line::from(" o       open selected item in browser"),
        Line::from(" Esc     close help or detail"),
        Line::from(" ?       toggle help"),
    ];
    Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: false })
}

fn detail_paragraph(app: &crate::app::App) -> Paragraph<'static> {
    let mut lines = Vec::new();
    if let Some(detail) = &app.state.detail {
        lines.push(Line::from(format!(
            "{} | {} | {} | {} | {}",
            detail.summary.repo.slug(),
            detail.summary.workflow_name,
            detail.summary.title,
            detail.summary.branch,
            detail.summary.event,
        )));
        lines.push(Line::from(format!(
            "state {}",
            format_run_state(&detail.summary, app)
        )));
        lines.push(Line::from(format!(
            "jobs {} / {} complete | running {} | failed {}",
            detail.completed_jobs, detail.total_jobs, detail.running_jobs, detail.failed_jobs
        )));
        lines.push(Line::from(progress_bar(
            detail.completed_jobs,
            detail.total_jobs,
            app.config.ui.ascii_only,
            28,
        )));
        lines.push(Line::from(""));

        for job in &detail.jobs {
            lines.push(format_job_line(job, app));
            if let Some(step) = &job.failed_step_name {
                lines.push(Line::from(Span::styled(
                    format!("    failed step: {step}"),
                    Style::default().fg(Color::Red),
                )));
            }
        }
    } else {
        lines.push(Line::from("Loading workflow detail..."));
    }

    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Workflow detail"),
        )
        .scroll((app.detail_scroll as u16, 0))
        .wrap(Wrap { trim: false })
}

fn draw_overlay(frame: &mut Frame<'_>, area: Rect, widget: impl Widget) {
    frame.render_widget(Clear, area);
    frame.render_widget(widget, area);
}

fn pane_block(title: &str, focused: bool) -> Block<'static> {
    let title = if focused {
        format!("{title} •")
    } else {
        title.to_string()
    };
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(title)
}

fn header_style(app: &crate::app::App) -> Style {
    if app.config.ui.no_color {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }
}

fn selected_style() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

fn state_style(conclusion: Option<&str>, status: &str, no_color: bool) -> Style {
    if no_color {
        return Style::default();
    }
    match (status, conclusion) {
        ("completed", Some("success")) => Style::default().fg(Color::Green),
        ("completed", Some("skipped")) => Style::default().fg(Color::DarkGray),
        ("completed", Some("failure" | "timed_out" | "cancelled")) => {
            Style::default().fg(Color::Red)
        }
        ("completed", _) => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Blue),
    }
}

fn format_run_state(run: &WorkflowRunSummary, app: &crate::app::App) -> String {
    let spinner = spinner_frame(app.spinner_index, app.config.ui.ascii_only);
    let queued = if app.config.ui.ascii_only { "." } else { "·" };
    let success = if app.config.ui.ascii_only { "+" } else { "✓" };
    let fail = if app.config.ui.ascii_only { "x" } else { "✕" };
    let skip = "-";

    match (run.status.as_str(), run.conclusion.as_deref()) {
        ("completed", Some("success")) => format!("{success} success"),
        ("completed", Some("skipped")) => format!("{skip} skipped"),
        ("completed", Some("failure")) => format!("{fail} failure"),
        ("completed", Some("cancelled")) => format!("{fail} cancelled"),
        ("completed", Some("timed_out")) => format!("{fail} timeout"),
        ("queued", _) => format!("{queued} queued"),
        _ => format!("{spinner} running"),
    }
}

fn review_state(pr: &PullRequestSummary) -> String {
    if pr.is_draft {
        return "draft".to_string();
    }

    match pr.review_decision.as_deref() {
        Some("APPROVED") => "approved".to_string(),
        Some("CHANGES_REQUESTED") => "changes".to_string(),
        Some("REVIEW_REQUIRED") => "review".to_string(),
        _ if pr.review_requested_for_viewer => "requested".to_string(),
        _ => "open".to_string(),
    }
}

fn ci_state(pr: &PullRequestSummary) -> String {
    match pr.ci_rollup.as_deref() {
        Some("SUCCESS") => "pass".to_string(),
        Some("FAILURE" | "ERROR") => "fail".to_string(),
        Some("PENDING" | "EXPECTED") => "pending".to_string(),
        Some("SKIPPED") => "skipped".to_string(),
        _ => "-".to_string(),
    }
}

fn format_job_line(job: &WorkflowJobSummary, app: &crate::app::App) -> Line<'static> {
    let duration = format_duration(job.started_at, job.completed_at);
    let prefix = if job.status == "completed" {
        match job.conclusion.as_deref() {
            Some("success") => status_symbol("success", app.config.ui.ascii_only).to_string(),
            Some("failure") => status_symbol("failure", app.config.ui.ascii_only).to_string(),
            Some("cancelled") => status_symbol("cancelled", app.config.ui.ascii_only).to_string(),
            _ => status_symbol("skipped", app.config.ui.ascii_only).to_string(),
        }
    } else {
        spinner_frame(app.spinner_index, app.config.ui.ascii_only)
    };

    let progress = if job.indeterminate_progress {
        "indeterminate".to_string()
    } else if job.total_steps > 0 {
        progress_bar(
            job.completed_steps,
            job.total_steps,
            app.config.ui.ascii_only,
            18,
        )
    } else {
        "-".to_string()
    };

    Line::from(format!(
        "{prefix} {} | {} | {}",
        truncate(&job.name, 42),
        progress,
        duration
    ))
}

fn format_run_duration(run: &WorkflowRunSummary) -> String {
    format_duration(
        run.started_at,
        run.updated_at.filter(|_| run.status == "completed"),
    )
}

pub fn format_duration(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> String {
    let Some(start) = start else {
        return "-".to_string();
    };
    let end = end.unwrap_or_else(Utc::now);
    let secs = (end - start).num_seconds().max(0);
    format_compact_duration(secs)
}

pub fn format_age(timestamp: DateTime<Utc>) -> String {
    let secs = (Utc::now() - timestamp).num_seconds().max(0);
    format_compact_duration(secs)
}

fn format_compact_duration(total_secs: i64) -> String {
    if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3_600 {
        format!("{}m", total_secs / 60)
    } else {
        format!("{}h", total_secs / 3_600)
    }
}

fn progress_bar(done: usize, total: usize, ascii_only: bool, width: usize) -> String {
    if total == 0 {
        return "-".to_string();
    }
    let filled = ((done as f64 / total as f64) * width as f64).round() as usize;
    let filled = filled.min(width);
    let full = if ascii_only { '#' } else { '█' };
    let empty = if ascii_only { '-' } else { '░' };
    format!(
        "[{}{}] {done}/{total}",
        full.to_string().repeat(filled),
        empty.to_string().repeat(width.saturating_sub(filled))
    )
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn minimum_width(mode: Mode) -> u16 {
    match mode {
        Mode::Compact => 52,
        Mode::Split => 96,
    }
}

fn overlay_rect(area: Rect, width_pct: u16, height_pct: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1]);
    horizontal[1].inner(Margin::new(0, 0))
}

fn rate_limit_label(rate_limit: &RateLimitState) -> String {
    let mut label = format!("rate:{}/{}", rate_limit.remaining, rate_limit.limit);
    if rate_limit.remaining <= 100 {
        if let Some(reset_at) = rate_limit.reset_at {
            label.push_str(&format!(" reset:{}", reset_at.format("%H:%M")));
        }
    } else if rate_limit.used > 0 {
        label.push_str(&format!(" used:{}", rate_limit.used));
    }
    if let Some(retry_after) = rate_limit.retry_after {
        label.push_str(&format!(" wait:{}s", retry_after));
    }
    label
}

fn status_symbol(kind: &str, ascii_only: bool) -> &'static str {
    if ascii_only {
        match kind {
            "success" => "+",
            "failure" => "x",
            "cancelled" => "x",
            _ => "-",
        }
    } else {
        match kind {
            "success" => "✓",
            "failure" => "✕",
            "cancelled" => "✕",
            _ => "·",
        }
    }
}

fn spinner_frame(index: usize, ascii_only: bool) -> String {
    let frames = if ascii_only {
        &["-", "\\", "|", "/"][..]
    } else {
        &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"][..]
    };
    frames[index % frames.len()].to_string()
}
