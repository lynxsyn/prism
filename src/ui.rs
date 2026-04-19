use chrono::{DateTime, Utc};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap,
};
use ratatui::{Frame, prelude::*};

use crate::app::App;
use crate::model::{
    FocusPane, Mode, PullRequestSummary, RateLimitState, WorkflowJobSummary, WorkflowRunSummary,
};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
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
                    .title("Resize needed")
                    .padding(Padding::horizontal(1)),
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
        draw_overlay(frame, overlay_rect(size, 72, 64), help_paragraph());
    }
}

fn draw_split(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    for pane_index in 0..2 {
        draw_split_pane(frame, sections[pane_index], app, pane_index);
    }
}

fn draw_split_pane(frame: &mut Frame<'_>, area: Rect, app: &App, pane_index: usize) {
    let Some(repo) = app.split_repo(pane_index) else {
        let block = pane_block("Empty pane".to_string(), false);
        let placeholder = Paragraph::new("No repo configured for this pane.").block(block);
        frame.render_widget(placeholder, area);
        return;
    };

    if app.split_detail_open(pane_index) {
        let target = app.current_split_action(pane_index);
        draw_detail_pane(
            frame,
            area,
            app,
            target,
            format!("{}  ·  Workflow detail", repo.slug()),
            pane_index == app.split_focus,
        );
        return;
    }

    let title = format!(
        "{}  ·  {}",
        repo.slug(),
        view_label(app.split_panes[pane_index].content)
    );
    let mut table_state = app.split_panes[pane_index].table_state.borrow_mut();
    match app.split_panes[pane_index].content {
        FocusPane::Actions => draw_repo_actions_table(
            frame,
            area,
            app,
            &app.split_actions(pane_index),
            &title,
            pane_index == app.split_focus,
            &mut table_state,
        ),
        FocusPane::PullRequests => draw_repo_prs_table(
            frame,
            area,
            app,
            &app.split_pulls(pane_index),
            &title,
            pane_index == app.split_focus,
            &mut table_state,
        ),
    }
}

fn draw_compact(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    if app.detail_open() {
        draw_detail_pane(
            frame,
            sections[0],
            app,
            app.current_action(),
            "Workflow detail".to_string(),
            app.compact_focus == FocusPane::Actions,
        );
    } else {
        draw_actions_table(
            frame,
            sections[0],
            app,
            &mut app.actions_table_state.borrow_mut(),
        );
    }

    draw_prs_table(
        frame,
        sections[1],
        app,
        &mut app.prs_table_state.borrow_mut(),
    );
}

fn draw_actions_table(frame: &mut Frame<'_>, area: Rect, app: &App, state: &mut TableState) {
    let header = Row::new([
        " Workflow ",
        " Repo ",
        " Branch ",
        " State ",
        " Age ",
        " Dur ",
    ])
    .style(header_style(app));
    let rows = if app.state.actions.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(" No workflow runs yet "),
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
                    Cell::from(format!(" {} ", truncate(&run.workflow_name, 22))),
                    Cell::from(format!(" {} ", truncate(&run.repo.slug(), 18))),
                    Cell::from(format!(" {} ", truncate(&run.branch, 12))),
                    Cell::from(Text::styled(
                        format!(" {} ", format_run_state(run, app)),
                        state_style(
                            run.conclusion.as_deref(),
                            &run.status,
                            app.config.ui.no_color,
                        ),
                    )),
                    Cell::from(format!(" {} ", format_age(run.created_at))),
                    Cell::from(format!(" {} ", format_run_duration(run))),
                ])
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Min(18),
            Constraint::Length(20),
            Constraint::Length(16),
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(pane_block(
        "Actions".to_string(),
        app.compact_focus == FocusPane::Actions,
    ))
    .column_spacing(1)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_prs_table(frame: &mut Frame<'_>, area: Rect, app: &App, state: &mut TableState) {
    let header = Row::new([
        " Repo ",
        " # ",
        " Title ",
        " Author ",
        " Review ",
        " CI ",
        " Updated ",
    ])
    .style(header_style(app));
    let rows = if app.state.pulls.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(" No pull requests yet "),
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
                    Cell::from(format!(" {} ", truncate(&pr.repo.slug(), 18))),
                    Cell::from(format!(" #{} ", pr.number)),
                    Cell::from(format!(" {} ", truncate(&pr.title, 24))),
                    Cell::from(format!(" {} ", truncate(&pr.author, 10))),
                    Cell::from(format!(" {} ", review_state(pr))),
                    Cell::from(format!(" {} ", ci_state(pr))),
                    Cell::from(format!(" {} ", format_age(pr.updated_at))),
                ])
                .style(row_style)
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(20),
            Constraint::Length(6),
            Constraint::Min(18),
            Constraint::Length(12),
            Constraint::Length(13),
            Constraint::Length(10),
            Constraint::Length(9),
        ],
    )
    .header(header)
    .block(pane_block(
        "Pull requests".to_string(),
        app.compact_focus == FocusPane::PullRequests,
    ))
    .column_spacing(1)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_repo_actions_table(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    rows_for_repo: &[&WorkflowRunSummary],
    title: &str,
    focused: bool,
    state: &mut TableState,
) {
    let header =
        Row::new([" Workflow ", " Branch ", " State ", " Age ", " Dur "]).style(header_style(app));
    let rows = if rows_for_repo.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(" No workflow runs yet "),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        rows_for_repo
            .iter()
            .map(|run| {
                Row::new(vec![
                    Cell::from(format!(" {} ", truncate(&run.workflow_name, 24))),
                    Cell::from(format!(" {} ", truncate(&run.branch, 14))),
                    Cell::from(Text::styled(
                        format!(" {} ", format_run_state(run, app)),
                        state_style(
                            run.conclusion.as_deref(),
                            &run.status,
                            app.config.ui.no_color,
                        ),
                    )),
                    Cell::from(format!(" {} ", format_age(run.created_at))),
                    Cell::from(format!(" {} ", format_run_duration(run))),
                ])
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(16),
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(pane_block(title.to_string(), focused))
    .column_spacing(1)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_repo_prs_table(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    rows_for_repo: &[&PullRequestSummary],
    title: &str,
    focused: bool,
    state: &mut TableState,
) {
    let header = Row::new([
        " # ",
        " Title ",
        " Author ",
        " Review ",
        " CI ",
        " Updated ",
    ])
    .style(header_style(app));
    let rows = if rows_for_repo.is_empty() {
        vec![Row::new(vec![
            Cell::from(""),
            Cell::from(" No pull requests yet "),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])]
    } else {
        rows_for_repo
            .iter()
            .map(|pr| {
                let row_style = if pr.review_requested_for_viewer {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(format!(" #{} ", pr.number)),
                    Cell::from(format!(" {} ", truncate(&pr.title, 28))),
                    Cell::from(format!(" {} ", truncate(&pr.author, 12))),
                    Cell::from(format!(" {} ", review_state(pr))),
                    Cell::from(format!(" {} ", ci_state(pr))),
                    Cell::from(format!(" {} ", format_age(pr.updated_at))),
                ])
                .style(row_style)
            })
            .collect()
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Min(24),
            Constraint::Length(14),
            Constraint::Length(13),
            Constraint::Length(10),
            Constraint::Length(9),
        ],
    )
    .header(header)
    .block(pane_block(title.to_string(), focused))
    .column_spacing(1)
    .row_highlight_style(selected_style())
    .highlight_symbol("› ");

    frame.render_stateful_widget(table, area, state);
}

fn draw_detail_pane(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    target: Option<&WorkflowRunSummary>,
    title: String,
    focused: bool,
) {
    let paragraph = Paragraph::new(detail_lines(app, target))
        .block(pane_block(title, focused))
        .scroll((app.detail_scroll as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn draw_status_bar(frame: &mut Frame<'_>, area: Rect, app: &App) {
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

    let focus = match app.config.mode {
        Mode::Compact => format!("focus:{}", view_label(app.compact_focus)),
        Mode::Split => format!(
            "focus:{}/{}",
            app.current_repo_label().unwrap_or_else(|| "-".to_string()),
            if app.detail_pane == Some(app.split_focus) {
                "detail".to_string()
            } else {
                view_label(app.focused_view()).to_string()
            }
        ),
    };
    let stale = if app.state.errors.is_empty() {
        ""
    } else {
        " | stale"
    };
    let controls = match app.config.mode {
        Mode::Compact => {
            if app.detail_open() {
                " | esc:back | l:link"
            } else {
                " | enter:detail | l:link"
            }
        }
        Mode::Split => {
            if app.detail_pane == Some(app.split_focus) {
                " | ←→:repo | esc:back | l:link"
            } else {
                " | ←→:repo | tab:view | enter:detail | l:link"
            }
        }
    };

    let text = format!(
        " mode:{} | {} | refresh:{} | next:{} | {} | host:{}{}{}",
        app.config.mode, focus, refreshed, next_countdown, rate, app.config.host, stale, controls
    );
    let widget = Paragraph::new(text).style(Style::default().add_modifier(Modifier::DIM));
    frame.render_widget(widget, area);
}

fn help_paragraph() -> Paragraph<'static> {
    let lines = vec![
        Line::from("Prism keybindings"),
        Line::from(""),
        Line::from(" q         quit"),
        Line::from(" r         force refresh"),
        Line::from(" Tab       toggle Actions / Pull requests in the focused pane"),
        Line::from(" ← / →     move focus between repo panes in split mode"),
        Line::from(" j / ↓     move down"),
        Line::from(" k / ↑     move up"),
        Line::from(" g / G     jump to top / bottom"),
        Line::from(" Enter     open workflow detail in the current pane"),
        Line::from(" l / o     open the selected item in the browser"),
        Line::from(" Esc       close help or return from detail"),
        Line::from(" ?         toggle help"),
    ];
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Help")
                .padding(Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: false })
}

fn detail_lines(app: &App, target: Option<&WorkflowRunSummary>) -> Vec<Line<'static>> {
    let Some(target) = target else {
        return vec![Line::from(" No workflow run selected. ")];
    };

    let Some(detail) = app.state.detail.as_ref().filter(|detail| {
        detail.summary.id == target.id && detail.summary.repo.slug() == target.repo.slug()
    }) else {
        return vec![
            Line::from(format!(
                " {}  {}",
                target.repo.slug(),
                truncate(&target.workflow_name, 36)
            )),
            Line::from(""),
            Line::from(" Loading workflow detail... "),
        ];
    };

    let ascii_only = app.config.ui.ascii_only;
    let mut lines = vec![
        Line::from(format!(
            " {}  {}",
            detail.summary.workflow_name,
            truncate(&detail.summary.title, 48)
        )),
        Line::from(""),
        Line::from(format!(
            " {} branch     {}",
            tree_branch(false, ascii_only),
            detail.summary.branch
        )),
        Line::from(format!(
            " {} event      {}",
            tree_branch(false, ascii_only),
            detail.summary.event
        )),
        Line::from(format!(
            " {} state      {}",
            tree_branch(false, ascii_only),
            format_run_state(&detail.summary, app)
        )),
        Line::from(format!(
            " {} jobs       {} complete  ·  {} running  ·  {} failed",
            tree_branch(false, ascii_only),
            detail.completed_jobs,
            detail.running_jobs,
            detail.failed_jobs
        )),
        Line::from(format!(
            " {} progress   {}",
            tree_branch(false, ascii_only),
            progress_bar(detail.completed_jobs, detail.total_jobs, ascii_only, 16)
        )),
        Line::from(format!(
            " {} job tree",
            tree_branch(detail.jobs.is_empty(), ascii_only)
        )),
    ];

    for (index, job) in detail.jobs.iter().enumerate() {
        lines.extend(detail_job_lines(
            job,
            index == detail.jobs.len().saturating_sub(1),
            ascii_only,
            app,
        ));
    }

    lines
}

fn detail_job_lines(
    job: &WorkflowJobSummary,
    last_job: bool,
    ascii_only: bool,
    app: &App,
) -> Vec<Line<'static>> {
    let job_progress = if job.indeterminate_progress {
        "indeterminate".to_string()
    } else if job.total_steps > 0 {
        progress_bar(job.completed_steps, job.total_steps, ascii_only, 10)
    } else {
        "-".to_string()
    };

    let mut lines = vec![Line::from(format!(
        " {} {}  {}  {}",
        nested_branch(last_job, ascii_only),
        truncate(&job.name, 24),
        job_progress,
        format_duration(job.started_at, job.completed_at)
    ))];

    if let Some(step) = &job.failed_step_name {
        lines.push(Line::from(Span::styled(
            format!(
                " {} {} failed step  {}",
                nested_child_prefix(last_job, ascii_only),
                tree_branch(true, ascii_only),
                step
            ),
            Style::default().fg(Color::Red),
        )));
    }

    if job.status != "completed" {
        lines.push(Line::from(format!(
            " {} {} state        {}",
            nested_child_prefix(last_job, ascii_only),
            tree_branch(true, ascii_only),
            format_job_state(job, app)
        )));
    }

    lines
}

fn draw_overlay(frame: &mut Frame<'_>, area: Rect, widget: impl Widget) {
    frame.render_widget(Clear, area);
    frame.render_widget(widget, area);
}

fn pane_block(title: String, focused: bool) -> Block<'static> {
    let title = if focused {
        format!("{title}  •")
    } else {
        title
    };
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .padding(Padding::horizontal(1))
        .title(title)
}

fn header_style(app: &App) -> Style {
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

fn format_run_state(run: &WorkflowRunSummary, app: &App) -> String {
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

fn format_job_state(job: &WorkflowJobSummary, app: &App) -> String {
    if job.status == "completed" {
        match job.conclusion.as_deref() {
            Some("success") => "✓ success".to_string(),
            Some("failure") => "✕ failure".to_string(),
            Some("cancelled") => "✕ cancelled".to_string(),
            _ => "- done".to_string(),
        }
    } else {
        format!(
            "{} running",
            spinner_frame(app.spinner_index, app.config.ui.ascii_only)
        )
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
        Mode::Compact => 56,
        Mode::Split => 98,
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
    horizontal[1]
}

fn rate_limit_label(rate_limit: &RateLimitState) -> String {
    let mut label = format!("rate:{}/{}", rate_limit.remaining, rate_limit.limit);
    if let Some(retry_after) = rate_limit.retry_after {
        label.push_str(&format!(" wait:{retry_after}s"));
    } else if rate_limit.remaining <= 100 {
        if let Some(reset_at) = rate_limit.reset_at {
            label.push_str(&format!(" reset:{}", reset_at.format("%H:%M")));
        }
    } else if rate_limit.used > 0 {
        label.push_str(&format!(" used:{}", rate_limit.used));
    }
    label
}

fn view_label(kind: FocusPane) -> &'static str {
    match kind {
        FocusPane::Actions => "Actions",
        FocusPane::PullRequests => "Pull requests",
    }
}

fn tree_branch(last: bool, ascii_only: bool) -> &'static str {
    match (last, ascii_only) {
        (true, true) => "\\-",
        (false, true) => "+-",
        (true, false) => "└─",
        (false, false) => "├─",
    }
}

fn nested_branch(last: bool, ascii_only: bool) -> &'static str {
    match (last, ascii_only) {
        (true, true) => "   \\-",
        (false, true) => "   +-",
        (true, false) => "   └─",
        (false, false) => "   ├─",
    }
}

fn nested_child_prefix(last: bool, ascii_only: bool) -> &'static str {
    match (last, ascii_only) {
        (true, true) => "      ",
        (false, true) => "   |  ",
        (true, false) => "      ",
        (false, false) => "   │  ",
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
