use ratatui::{
    backend::Backend,
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Line},
    widgets::{block::{self, Block, Title}, BorderType, Borders, Clear, Gauge, List, ListItem, Padding, Paragraph},
};
use rust_roon_api::transport::State;

use crate::app::{App, View};

const ROON_BRAND_COLOR: Color = Color::Rgb(0x75, 0x75, 0xf3);

const _LOAD: char = '\u{23f3}';
const PLAY: &str = " \u{23f5} ";
const _PAUSE: char = '\u{23f8}';
const _STOP: char = '\u{23f9}';
const _RELOAD: char = '\u{27f3}';
const _SHUFFLE: char = '\u{1f500}';
const _REPEAT: char = '\u{1f501}';
const _REPEAT_ONCE: char = '\u{1f502}';
const _SPEAKER: char = '\u{1f508}';
const _SPEAKER_ONE_WAVE: char = '\u{1f509}';
const _SPEAKER_THREE_WAVE: char = '\u{1f50a}';

pub fn draw<B>(frame: &mut Frame<B>, app: &mut App)
where
    B: Backend,
{
    let size = frame.size();

    // Surrounding block
    let title = if let Some(name) = app.core_name.as_ref() {
        format!("[ Roon TUI - {} ]", name)
    } else {
        "[ Roon TUI - No core found]".to_owned()
    };
    let color = if app.get_selected_view().is_none() {ROON_BRAND_COLOR} else {Color::Reset};
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(title, Style::default().fg(color)))
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Plain);
    frame.render_widget(block, size);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .horizontal_margin(2)
        .vertical_margin(1)
        .constraints([Constraint::Min(8), Constraint::Length(7)].as_ref())
        .split(frame.size());

    // Top two inner blocks
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[0]);

    draw_browse_view(frame, top_chunks[0], app);
    draw_queue_view(frame, top_chunks[1], app);
    draw_now_playing_view(frame, chunks[1], &app);

    if let Some(View::Zones) = &app.selected_view {
        draw_zones_view(frame, top_chunks[1], app);
    }
}

fn draw_browse_view<B>(frame: &mut Frame<B>, area: Rect, app: &mut App)
where
    B: Backend,
{
    let browse_title = format!("{}", app.browse.title.as_deref().unwrap_or("Browse"));
    let page_lines = if area.height > 2 {area.height - 2} else {0} as usize;  // Exclude border
    let view = &View::Browse;
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(&app, view))
        .title(Span::styled(
            browse_title,
            get_text_view_style(&app, view),
        ));

    app.browse.prepare_paging(page_lines, |item| if item.subtitle.is_none() {1} else {2});

    if let Some(browse_items) = &app.browse.items {
        let items: Vec<ListItem> = browse_items
            .iter()
            .map(|item| {
                let subtitle = item.subtitle.as_ref().filter(|s| !s.is_empty());
                let mut lines = vec![
                    Line::from(Span::styled(&item.title, get_text_view_style(&app, view)))
                ];

                if let Some(subtitle) = subtitle {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", subtitle),
                        Style::default().add_modifier(Modifier::ITALIC)
                    )));
                }

                ListItem::new(lines)
            })
            .collect();

        // Create a List from all list items and highlight the currently selected one
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .padding(Padding {
                        left: if app.browse.is_selected() {0} else {3},
                        right: 0,
                        top: 0,
                        bottom: 0,
                    })
            )
            .highlight_style(
                Style::default()
                    .bg(ROON_BRAND_COLOR)
                    .add_modifier(Modifier::BOLD)
            )
            .highlight_symbol(PLAY);

        // We can now render the item list
        frame.render_stateful_widget(list, area, &mut app.browse.state);

        if let Some(View::Browse) = app.selected_view.as_ref() {
            let len = browse_items.len();

            if len > 0 {
                let progress = format!(
                    "{}/{}",
                    app.browse.state.selected().unwrap() + 1,
                    len
                );
    
                block = block.title(
                    Title::from(
                        Span::styled(progress, Style::default().fg(Color::Reset))
                    ).alignment(Alignment::Right)
                );
            }
        }
    }

    frame.render_widget(block, area);
}

fn draw_queue_view<B>(frame: &mut Frame<B>, area: Rect, app: &mut App)
where
    B: Backend,
{
    let page_lines = if area.height > 2 {area.height - 2} else {0} as usize;  // Exclude border
    let view = &View::Queue;
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(&app, view))
        .title(Span::styled(
            "Queue",
            get_text_view_style(&app, view),
        ))
        .title_alignment(Alignment::Right);

    app.queue.prepare_paging(page_lines, |item| if item.two_line.line2.is_empty() {1} else {2});

    if let Some(queue_items) = &app.queue.items {
        let item_len = (area.width - 5) as usize;
        let items: Vec<ListItem> = queue_items
            .iter()
            .map(|item| {
                let duration = format!(" {}:{:02} ", item.length / 60, item.length % 60);
                let max_len = item_len - duration.len();
                let line_len = item.two_line.line1.len();
                let trim_len = if line_len < max_len {line_len} else {max_len};
                let line1 = &item.two_line.line1[0..trim_len];
                let pad_len = item_len - line1.len() - duration.len();
                let pad: String = (0..pad_len).map(|_| ' ').collect();
                let line1 = format!("{}{}{} ", line1, pad, duration);
                let mut lines = vec![
                    Line::from(Span::styled(line1, get_text_view_style(&app, view))),
                ];

                if !item.two_line.line2.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", item.two_line.line2),
                        Style::default().add_modifier(Modifier::ITALIC)
                    )));
                }

                ListItem::new(lines)
            })
            .collect();

        // Create a List from all list items and highlight the currently selected one
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .padding(Padding {
                        left: if app.queue.is_selected() {0} else {3},
                        right: 0,
                        top: 0,
                        bottom: 0,
                    })
            )
            .highlight_style(
                Style::default()
                    .bg(ROON_BRAND_COLOR)
                    .add_modifier(Modifier::BOLD)
            )
            .highlight_symbol(PLAY);

        // We can now render the item list
        frame.render_stateful_widget(list, area, &mut app.queue.state);

        if let Some(View::Queue) = app.selected_view.as_ref() {
            let len = queue_items.len();

            if len > 0 {
                let progress = format!(
                    "{}/{}",
                    app.queue.state.selected().unwrap() + 1,
                    len
                );
    
                block = block.title(
                    Title::from(
                        Span::styled(progress, Style::default().fg(Color::Reset))
                    ).alignment(Alignment::Left)
                );
            }
        }
    }

    frame.render_widget(block, area);
}

fn draw_now_playing_view<B>(frame: &mut Frame<B>, area: Rect, app: &App)
where
    B: Backend,
{
    let view = &View::NowPlaying;
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(app, view))
        .title_position(block::Position::Bottom)
        .padding(Padding {
            left: 1,
            right: 0,
            top: 0,
            bottom: 0,
        });

    if let Some(zone) = app.selected_zone.as_ref() {
        let vert_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)].as_ref())
            .split(area);

        block = block.title(
            Title::from(
                Span::styled(zone.display_name.as_str(), get_text_view_style(app, view))
            ).alignment(Alignment::Right)
        );

        if let Some(now_playing) = zone.now_playing.as_ref() {
            let metadata_block = Block::default()
                .padding(Padding {
                    left: 4,
                    right: 0,
                    top: 1,
                    bottom: 0,
                });
            let lines = vec![
                Line::from(Span::styled(
                    &now_playing.three_line.line1,
                    Style::default().add_modifier(Modifier::BOLD)
                )),
                Line::from(now_playing.three_line.line2.as_str()),
                Line::from(Span::styled(
                    &now_playing.three_line.line3,
                    Style::default().add_modifier(Modifier::ITALIC)
                )),
            ];
            let text = Paragraph::new(lines)
                .block(metadata_block);

            frame.render_widget(text, vert_chunks[0]);

            let duration = now_playing.length.unwrap_or_default();
            draw_progress_gauge(frame, vert_chunks[1], app, duration);

            let play_state_title = match zone.state {
                State::Loading => "Loading",
                State::Paused => "Paused",
                State::Playing => "Playing",
                State::Stopped => "Stopped",
            };

            block = block.title(Span::styled(
                play_state_title,
                get_text_view_style(app, view),
            ));
        }
    }

    frame.render_widget(block, area);
}

fn draw_progress_gauge<B>(frame: &mut Frame<B>, area: Rect, app: &App, duration: u32) -> Option<()>
where
    B: Backend,
{
    let elapsed = app.zone_seek.as_ref()?.seek_position? as u32;
    let progress = if duration > 0 {elapsed * 100 / duration} else {0};
    let elapsed = format!("{}:{:02}", elapsed / 60, elapsed % 60);
    let label = if duration > 0 {
        format!("{} / {}:{:02}", elapsed, duration / 60, duration % 60)
    } else {
        elapsed
    };
    let gauge = Gauge::default()
        .block(Block::default().padding(Padding {
            left: 2,
            right: 2,
            top: 0,
            bottom: 1,
        }))
        .gauge_style(get_gauge_view_style(app, &View::NowPlaying))
        .percent(progress as u16)
        .label(Span::styled(label, Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD)));

    frame.render_widget(gauge, area);

    Some(())
}

fn draw_zones_view<B>(frame: &mut Frame<B>, area: Rect, app: &mut App)
where
    B: Backend,
{
    let view = &View::Zones;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(&app, view))
        .title(Span::styled(
            "Zones",
            get_text_view_style(&app, view),
        ))
        .title_alignment(Alignment::Left);

    let area = bottom_right_rect(50, 50, area);
    let page_lines = if area.height > 2 {area.height - 2} else {0} as usize;  // Exclude border

    frame.render_widget(Clear, area);   // This clears out the background

    app.zones.prepare_paging(page_lines, |_| 1);

    if let Some(zones) = app.zones.items.as_ref() {
        let items: Vec<ListItem> = zones
            .iter()
            .map(|(_, name)| {
                let line = Span::styled(
                    name,
                    get_text_view_style(&app, view));
                ListItem::new(Line::from(line)).style(Style::default())
            })
            .collect();

        // Create a List from all list items and highlight the currently selected one
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
            )
            .highlight_style(
                Style::default()
                    .bg(ROON_BRAND_COLOR)
                    .add_modifier(Modifier::BOLD)
            )
            .highlight_symbol(PLAY);

        // We can now render the item list
        frame.render_stateful_widget(list, area, &mut app.zones.state);
    }

    frame.render_widget(block, area);
}

fn get_border_view_style(app: &App, view: &View) -> Style {
    let mut style = Style::default();

    if let Some(selected_view) = app.get_selected_view() {
        if *selected_view == *view {
            style = style.fg(ROON_BRAND_COLOR);
        }
    }

    style
}

fn get_gauge_view_style(app: &App, view: &View) -> Style {
    let mut style = Style::default().bg(Color::Rgb(0x30, 0x30, 0x30));

    if let Some(selected_view) = app.get_selected_view() {
        if *selected_view == *view {
            style = style.fg(ROON_BRAND_COLOR);
        } else {
            style = style.fg(Color::Rgb(0x80, 0x80, 0x80));
        }
    }

    style
}

fn get_text_view_style(app: &App, view: &View) -> Style {
    let mut style = Style::default();

    if let Some(selected_view) = app.get_selected_view() {
        if *selected_view == *view {
            style = style.fg(Color::Reset).add_modifier(Modifier::BOLD);
        }
    }

    style
}

fn bottom_right_rect(percent_x: u16, percent_y: u16, rect: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(100 - percent_y),
                Constraint::Percentage(percent_y),
            ]
            .as_ref(),
        )
        .split(rect);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage(100 - percent_x),
                Constraint::Percentage(percent_x),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
