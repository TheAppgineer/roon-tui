use ratatui::{
    backend::Backend,
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Line},
    widgets::{block::{self, Block, Title}, BorderType, Borders, Clear, List, ListItem, Padding, Paragraph},
};

use crate::app::{App, View};

const ROON_BRAND_COLOR: Color = Color::Rgb(0x75, 0x75, 0xf3);

const PLAY: &str = " \u{23f5} ";
const _PAUSE: char = '\u{23f8}';
const _STOP: char = '\u{23f9}';
const _RELOAD: char = '\u{27f3}';
const _SHUFFLE: char = '\u{1f500}';
const _REPEAT: char = '\u{1f501}';
const _REPEAT_ONCE: char = '\u{1f502}';
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
        .constraints([Constraint::Percentage(80), Constraint::Percentage(20)].as_ref())
        .split(frame.size());

    // Top two inner blocks
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[0]);

    // Browse view
    let browse_title = format!("{}", app.browse.title.as_deref().unwrap_or("Browse"));
    let page_lines = (top_chunks[0].height - 2) as usize;  // Exclude border
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(&app, View::Browse))
        .title(Span::styled(
            browse_title,
            get_text_view_style(&app, View::Browse),
        ));

    app.browse.prepare_paging(page_lines, |item| if item.subtitle.is_none() {1} else {2});

    if let Some(browse_items) = &app.browse.items {
        let items: Vec<ListItem> = browse_items
            .iter()
            .map(|item| {
                let subtitle = item.subtitle.as_ref().filter(|s| !s.is_empty());
                let mut lines = vec![
                    Line::from(Span::styled(&item.title, get_text_view_style(&app, View::Browse)))
                ];

                if let Some(subtitle) = subtitle {
                    lines.push(Line::from(format!("  ({})", subtitle)));
                }

                ListItem::new(lines).style(Style::default())
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
        frame.render_stateful_widget(list, top_chunks[0], &mut app.browse.state);

        if let Some(selected_view) = app.selected_view.as_ref() {
            if *selected_view == View::Browse {
                let progress = format!(
                    "{}/{}",
                    app.browse.state.selected().unwrap() + 1,
                    browse_items.len()
                );

                block = block.title(
                    Title::from(
                        Span::styled(progress, Style::default().fg(Color::Reset))
                    ).alignment(Alignment::Right));
            }
        }
    }

    frame.render_widget(block, top_chunks[0]);

    // [ Queue ] view
    let page_lines = (top_chunks[1].height - 2) as usize;  // Exclude border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(&app, View::Queue))
        .title(Span::styled(
            "Queue",
            get_text_view_style(&app, View::Queue),
        ))
        .title_alignment(Alignment::Right);

    app.queue.prepare_paging(page_lines, |item| if item.two_line.line2.is_empty() {1} else {2});

    if let Some(queue_items) = &app.queue.items {
        let items: Vec<ListItem> = queue_items
            .iter()
            .map(|item| {
                let mut lines = vec![
                    Line::from(Span::styled(&item.two_line.line1, get_text_view_style(&app, View::Queue))),
                ];

                if !item.two_line.line2.is_empty() {
                    lines.push(Line::from(format!("  ({})", item.two_line.line2)));
                }

                ListItem::new(lines).style(Style::default())
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
        frame.render_stateful_widget(list, top_chunks[1], &mut app.queue.state);
    }

    frame.render_widget(block, top_chunks[1]);

    // [ Now Playing] view
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(&app, View::NowPlaying))
        .title(Span::styled(
            "Now Playing",
            get_text_view_style(&app, View::NowPlaying),
        ))
        .title_position(block::Position::Bottom)
        .padding(Padding {
            left: 2,
            right: 2,
            top: 0,
            bottom: 0,
        });

    let text = Paragraph::new("Track\nArtist\nAlbum")
        .style(Style::default().add_modifier(Modifier::DIM))
        .block(block);
    frame.render_widget(text, chunks[1]);

    if let Some(View::Zones) = &app.selected_view {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(get_border_view_style(&app, View::Zones))
            .title("[ Playback Zones ]")
            .title_alignment(Alignment::Center);

        let area = bottom_right_rect(50, 50, top_chunks[1]);
        let page_lines = (area.height - 2) as usize;  // Exclude border

        frame.render_widget(Clear, area); //this clears out the background

        app.zones.prepare_paging(page_lines, |_| 1);

        if let Some(zones) = app.zones.items.as_ref() {
            let items: Vec<ListItem> = zones
                .iter()
                .map(|(_, name)| {
                    let line = Span::styled(
                        name,
                        get_text_view_style(&app, View::Zones));
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
}

fn get_border_view_style(app: &App, view: View) -> Style {
    let mut style = Style::default();

    if let Some(selected_view) = app.get_selected_view() {
        if *selected_view == view {
            style = style.fg(ROON_BRAND_COLOR);
        }
    }

    style
}

fn get_text_view_style(app: &App, view: View) -> Style {
    let mut style = Style::default();

    if let Some(selected_view) = app.get_selected_view() {
        if *selected_view == view {
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
