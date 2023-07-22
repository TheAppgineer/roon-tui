use ratatui::{
    backend::Backend,
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Line},
    widgets::{block::{self, Block}, BorderType, Borders, List, ListItem, Padding, Paragraph},
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
    // Wrapping block for a group
    // Just draw the block and the group on the same area and build the group
    // with at least a margin of 1
    let size = frame.size();

    // Surrounding block
    let color = if app.get_selected_view().is_none() {ROON_BRAND_COLOR} else {Color::Reset};
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled("[ Roon TUI ]", Style::default().fg(color)))
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Plain);
    frame.render_widget(block, size);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .horizontal_margin(2)
        .vertical_margin(1)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)].as_ref())
        .split(frame.size());

    // Top two inner blocks
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[0]);

    // Iterate through all elements in the `items` app and append some debug text to it.
    if let Some(browse_items) = &app.browse.items {
        let items: Vec<ListItem> = browse_items
        .iter()
        .map(|item| {
            let subtitle = item.subtitle.as_ref().filter(|s| !s.is_empty());
            let mut lines = vec![
                Line::from(Span::styled(&item.title, Style::default().add_modifier(Modifier::BOLD)))
            ];

            if let Some(subtitle) = subtitle {
                lines.push(Line::from(Span::styled(subtitle, Style::default())));
            }
            ListItem::new(lines).style(Style::default())
        })
        .collect();

        // Create a List from all list items and highlight the currently selected one
        let items = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("List")
            )
            .highlight_style(
                Style::default()
                    .bg(ROON_BRAND_COLOR)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(PLAY);

        // We can now render the item list
        frame.render_stateful_widget(items, top_chunks[0], &mut app.browse.state);
    }

    // [ Browse ] view
    let browse_title = format!("[ {} ]", app.browse.title.as_deref().unwrap_or("Browse"));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_view_style(app, View::Browse))
        .title(
            Span::styled(
                browse_title, 
                get_view_style(app, View::Browse)
            )
        );
    frame.render_widget(block, top_chunks[0]);

    // [ Queue ] view
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_view_style(app, View::Queue))
        .title(
            Span::styled(
                "[ Queue ]",
                get_view_style(app, View::Queue),
            )
        )
        .title_alignment(Alignment::Right);
    frame.render_widget(block, top_chunks[1]);

    // [ Now Playing] view
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_view_style(app, View::NowPlaying))
        .title(
            Span::styled(
                "[ Now Playing ]",
                get_view_style(app, View::NowPlaying),
            )
        )
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
}

fn get_view_style(app: &App, view: View) -> Style {
    let mut style = Style::default();

    if let Some(selected_view) = app.get_selected_view() {
        if *selected_view == view {
            style = style.fg(ROON_BRAND_COLOR);
        }
    }

    style
}
