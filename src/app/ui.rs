use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Line},
    widgets::{block::{self, Block, Position, Title}, BorderType, Borders, Clear, Gauge, HighlightSpacing, List, ListItem, Padding, Paragraph},
};
use roon_api::transport::{State, Zone, Repeat, volume::Scale};

use crate::{app::{App, View}, io::EndPoint};

const ROON_BRAND_COLOR: Color = Color::Rgb(0x75, 0x75, 0xf3);
const CUSTOM_GRAY: Color = Color::Rgb(0x80, 0x80, 0x80);
const UNI_HIGHLIGHT_SYMBOL: &str = " \u{23f5} ";
const UNI_CHECKED_SYMBOL: &str = "\u{1F5F9}";
const UNI_UNCHECKED_SYMBOL: &str = "\u{2610}";
const HIGHLIGHT_SYMBOL: &str = " > ";
const CHECKED_SYMBOL: &str = "+";
const UNCHECKED_SYMBOL: &str = "-";

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.size();

    // Surrounding block
    let title = format!(" Roon TUI v{} ", env!("CARGO_PKG_VERSION"));
    let subtitle = if let Some(name) = app.core_name.as_ref() {
        format!(" {} ", name)
    } else {
        app.select_view(None);
        " No Roon Server paired/found ".to_owned()
    };
    let hint = Title::from(
            Span::styled(" Ctrl-h for Help ", Style::default().fg(Color::Reset))
        )
        .position(Position::Bottom)
        .alignment(Alignment::Center);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(app, None))
        .title(Span::styled(title, get_text_view_style(app, None)))
        .title(Span::styled(subtitle, get_text_view_style(app, None)))
        .title(hint)
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Plain);

    frame.render_widget(block, size);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .horizontal_margin(2)
        .vertical_margin(1)
        .constraints([Constraint::Min(8), Constraint::Length(7)].as_ref())
        .split(size);

    // Top two inner blocks
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(chunks[0]);

    draw_browse_view(frame, top_chunks[0], app);
    draw_queue_view(frame, top_chunks[1], app);
    draw_now_playing_view(frame, chunks[1], app);

    match app.selected_view {
        Some(View::Prompt) => draw_prompt_view(frame, top_chunks[0], app),
        Some(View::Zones) => draw_zones_view(frame, top_chunks[1], app),
        Some(View::Grouping) | Some(View::GroupingPreset) => {
            draw_grouping_view(frame, top_chunks[1], app);
        }
        Some(View::Help) => draw_help_view(frame, size, app),
        _ => (),
    }
}

fn draw_browse_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let browse_title = app.browse.title.as_deref().unwrap_or("Browse").to_owned();
    let page_lines = area.height.saturating_sub(2) as usize;  // Exclude border
    let view = Some(&View::Browse);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(app, view))
        .title(Span::styled(
            browse_title,
            get_text_view_style(app, view),
        ));

    app.browse.prepare_paging(page_lines, |item| if item.subtitle.is_none() {1} else {2});

    if let Some(browse_items) = &app.browse.items {
        let secondary_style = if app.get_selected_view().is_some() {
            Style::default().add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(CUSTOM_GRAY).add_modifier(Modifier::ITALIC)
        };
        let items: Vec<ListItem> = browse_items
            .iter()
            .map(|item| {
                let subtitle = item.subtitle.as_ref().filter(|s| !s.is_empty());
                let mut lines = vec![
                    Line::from(Span::styled(&item.title, get_text_view_style(app, view)))
                ];

                if let Some(subtitle) = subtitle {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", subtitle),
                        secondary_style,
                    )));
                }

                ListItem::new(lines)
            })
            .collect();

        // Create a List from all list items and highlight the currently selected one
        let highlight_symbol = if app.no_unicode_symbols {HIGHLIGHT_SYMBOL} else {UNI_HIGHLIGHT_SYMBOL};
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .bg(ROON_BRAND_COLOR)
                    .add_modifier(Modifier::BOLD)
            )
            .highlight_symbol(highlight_symbol)
            .highlight_spacing(HighlightSpacing::Always);

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

                if !app.input.is_empty() {
                    block = block.title(
                        Title::from(
                            Span::styled(app.input.as_str(), Style::default().fg(Color::Reset))
                        ).position(Position::Bottom)
                    );
                }
            }
        }
    }

    frame.render_widget(block, area);
}

fn draw_queue_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let page_lines = area.height.saturating_sub(2) as usize;  // Exclude border
    let view = Some(&View::Queue);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(app, view))
        .title(Span::styled(
            "Queue",
            get_text_view_style(app, view),
        ))
        .title_alignment(Alignment::Right);

    if let Some(queue_mode) = app.queue_mode {
        block = block.title(
            Title::from(
                Span::styled(queue_mode, Style::default().fg(Color::Reset))
            ).position(Position::Bottom)
        );
    }

    app.queue.prepare_paging(page_lines, |item| if item.two_line.line2.is_empty() {1} else {2});

    if let Some(queue_items) = &app.queue.items {
        let item_len = area.width.saturating_sub(6) as usize;
        let secondary_style = if app.get_selected_view().is_some() {
            Style::default().add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(CUSTOM_GRAY).add_modifier(Modifier::ITALIC)
        };
        let items: Vec<ListItem> = queue_items
            .iter()
            .map(|item| {
                let duration = get_time_string(item.length);
                let max_len = item_len.saturating_sub(duration.len() + 1);
                let (line1_len, line1) = trim_string(&item.two_line.line1, max_len);
                let pad_len = item_len.saturating_sub(line1_len + duration.len());
                let pad: String = (0..pad_len).map(|_| ' ').collect();
                let line1 = format!("{}{}{}", line1, pad, duration);
                let mut lines = vec![
                    Line::from(Span::styled(line1, get_text_view_style(app, view))),
                ];

                if !item.two_line.line2.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", item.two_line.line2),
                        secondary_style,
                    )));
                }

                ListItem::new(lines)
            })
            .collect();

        // Create a List from all list items and highlight the currently selected one
        let highlight_symbol = if app.no_unicode_symbols {HIGHLIGHT_SYMBOL} else {UNI_HIGHLIGHT_SYMBOL};
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .bg(ROON_BRAND_COLOR)
                    .add_modifier(Modifier::BOLD)
            )
            .highlight_symbol(highlight_symbol)
            .highlight_spacing(HighlightSpacing::Always);

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
        } else if let Some(queue_time_remaining) = get_queue_time_remaining(app) {
            block = block.title(
                Title::from(
                    Span::styled(queue_time_remaining, Style::default().fg(Color::Reset))
                ).alignment(Alignment::Left)
            );
        }
    }

    frame.render_widget(block, area);
}

fn draw_now_playing_view(frame: &mut Frame, area: Rect, app: &App) {
    let view = Some(&View::NowPlaying);
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
        let hor_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(14)].as_ref())
            .split(vert_chunks[0]);
        let style = if app.get_selected_view().is_some() {
            Style::default().fg(Color::Reset)
        } else {
            Style::default().fg(CUSTOM_GRAY)
        };

        let display_name = match app.matched_preset.as_ref() {
            Some(preset) => format!("{} ({})", preset.as_str(), zone.display_name),
            None => zone.display_name.to_owned(),
        };

        block = block.title(
            Title::from(Span::styled(
                display_name,
                get_text_view_style(app, view),
            )).alignment(Alignment::Right)
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
                    style.add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    &now_playing.three_line.line2,
                    style,
                )),
                Line::from(Span::styled(
                    &now_playing.three_line.line3,
                    style.add_modifier(Modifier::ITALIC),
                )),
            ];
            let text = Paragraph::new(lines)
                .block(metadata_block);

            frame.render_widget(text, hor_chunks[0]);

            let duration = now_playing.length.unwrap_or_default();
            let seek_position = if let Some(zone_seek) = app.zone_seek.as_ref() {
                if zone_seek.seek_position.is_some() {
                    zone_seek.seek_position
                } else {
                    now_playing.seek_position
                }
            } else {
                now_playing.seek_position
            };

            draw_progress_gauge(frame, vert_chunks[1], app, view, duration, seek_position);

            let play_state_title = match zone.state {
                State::Loading => "Loading",
                State::Paused => "Paused",
                State::Playing => if app.pause_on_track_end {
                    "Pause at End of Track"
                } else {
                    "Playing"
                },
                State::Stopped => "Stopped",
            };

            block = block.title(Span::styled(
                play_state_title,
                get_text_view_style(app, view),
            ));
        } else if app.core_name.is_some() {
            let msg_block = Block::default()
                .padding(Padding {
                    left: 0,
                    right: 0,
                    top: 1,
                    bottom: 0,
                });
            let text = Paragraph::new("Go find something to play!")
                .block(msg_block).alignment(Alignment::Center);

            frame.render_widget(text, hor_chunks[0]);
        }

        let status_block = Block::default()
            .padding(Padding {
                left: 1,
                right: 2,
                top: 1,
                bottom: 0,
            });
        let text = Paragraph::new(get_status_lines(zone, style))
            .block(status_block).alignment(Alignment::Right);

        frame.render_widget(text, hor_chunks[1]);
    } else {
        let msg_block = Block::default()
            .padding(Padding {
                left: 0,
                right: 0,
                top: 1,
                bottom: 0,
            });
        let msg = if app.core_name.is_some()  {
            "No zone selected, use Ctrl-z to select one"
        } else {
            "Not paired to a Roon Server (or no server found)\n\
            Use a Roon Remote and go to Settings->Extensions to enable Roon TUI"
        };
        let text = Paragraph::new(msg)
            .block(msg_block).alignment(Alignment::Center);

        frame.render_widget(text, area);
    }

    frame.render_widget(block, area);
}

fn draw_progress_gauge(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: Option<&View>,
    duration: u32,
    seek_position: Option<i64>,
) -> Option<()> {
    let elapsed = seek_position? as u32;
    let progress = if duration > 0 {elapsed * 100 / duration} else {0};
    let elapsed = get_time_string(elapsed);
    let label = if duration > 0 {
        format!("{} / {}", elapsed, get_time_string(duration))
    } else {
        elapsed
    };
    let style = if app.get_selected_view().is_some() {
        Style::default().fg(Color::Reset)
    } else {
        Style::default().fg(CUSTOM_GRAY)
    };
    let gauge = Gauge::default()
        .block(Block::default().padding(Padding {
            left: 2,
            right: 2,
            top: 0,
            bottom: 1,
        }))
        .gauge_style(get_gauge_view_style(app, view))
        .percent(progress as u16)
        .label(Span::styled(label, style.add_modifier(Modifier::BOLD)));

    frame.render_widget(gauge, area);

    Some(())
}

fn get_time_string(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

fn get_queue_time_remaining(app: &App) -> Option<String> {
    let zone = app.selected_zone.as_ref()?;
    let now_playing = zone.now_playing.as_ref()?;
    let queue_time_remaining = match app.zone_seek.as_ref() {
        Some(zone_seek) => zone_seek.queue_time_remaining,
        None => zone.queue_time_remaining,
    };

    if queue_time_remaining > 0 && now_playing.length.is_some() {
        Some(get_time_string(queue_time_remaining as u32))
    } else {
        None
    }
}

fn trim_string(string: &str, trim_len: usize) -> (usize, &str) {
    let trim = match string.char_indices().nth(trim_len) {
        None => string,
        Some((index, _)) => &string[..index],
    };

    (trim.chars().count(), trim)
}

fn get_status_lines(zone: &Zone, style: Style) -> Vec<Line> {
    let volume = if let Some(output) = zone.outputs.get(0) {
        if let Some(volume) = output.volume.as_ref() {
            match volume.scale {
                Scale::Incremental => "Vol Incrmnt".to_owned(),
                _ => {
                    let is_muted = volume.is_muted.unwrap_or_default();

                    if is_muted {
                        "Vol   Muted".to_owned()
                    } else {
                        let volume_level = volume.value.unwrap();

                        match volume.scale {
                            Scale::Decibel => {
                                if volume.step.unwrap() < 1.0 {
                                    format!("Vol {:5.1}dB", volume_level)
                                } else {
                                    format!("Vol {:5}dB", volume_level)
                                }
                            }
                            Scale::Number => format!("Vol {:7}", volume_level),
                            _ => String::new(),
                        }
                    }
                }
            }
        } else {
            "Vol   Fixed".to_owned()
        }
    } else {
        String::new()
    };
    let settings = &zone.settings;
    let repeat_icon = match settings.repeat {
        Repeat::All => "Repeat  All",
        Repeat::One => "Repeat  One",
        _ => "Repeat  Off",
    };

    vec![
        Line::from(Span::styled(volume, style)),
        Line::from(Span::styled(repeat_icon, style)),
        Line::from(Span::styled(
            if settings.shuffle {"Shuffle  On"} else {"Shuffle Off"},
            style
        )),
    ]
}

fn draw_prompt_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let view = Some(&View::Prompt);
    let area = upper_bar(area);
    let max_len = area.width.saturating_sub(3) as usize;
    app.set_max_input_len(max_len);

    let prompt = app.prompt.as_str();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(app, view))
        .title(Span::styled(
            prompt,
            get_text_view_style(app, view),
        ))
        .title_alignment(Alignment::Left);

    frame.render_widget(Clear, area);   // This clears out the background

    let input = Line::from(Span::styled(app.input.as_str(), Style::default().fg(Color::Reset)));
    let input = Paragraph::new(input)
        .style(Style::default().fg(ROON_BRAND_COLOR))
        .block(block);

    frame.render_widget(input, area);

    // Make the cursor visible and ask ratatui to put it at the specified coordinates after
    // rendering
    frame.set_cursor(
        // Draw the cursor at the current position in the input field.
        // This position can be controlled via the left and right arrow key
        area.x + app.cursor_position.clamp(0, max_len) as u16 + 1,
        // Move one line down, from the border to the input line
        area.y + 1,
    );
}

fn draw_zones_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let view = Some(&View::Zones);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(app, view))
        .title(Span::styled(
            "Zones",
            get_text_view_style(app, view),
        ))
        .title_alignment(Alignment::Left);

    let area = bottom_right_rect(50, 50, area);
    let page_lines = area.height.saturating_sub(2) as usize;  // Exclude border

    frame.render_widget(Clear, area);   // This clears out the background

    app.zones.prepare_paging(page_lines, |_| 1);

    if let Some(zones) = app.zones.items.as_ref() {
        let items: Vec<ListItem> = zones
            .iter()
            .map(|(end_point, name)| {
                let name = match end_point {
                    EndPoint::Preset(_) => format!("[{}]", name),
                    EndPoint::Output(_) => format!("<{}>", name),
                    EndPoint::Zone(_) => name.to_owned(),
                    EndPoint::MatchedPreset((_, preset)) => preset.to_owned(),
                };
                let line = Span::styled(
                    name,
                    get_text_view_style(app, view));
                ListItem::new(Line::from(line)).style(Style::default())
            })
            .collect();

        // Create a List from all list items and highlight the currently selected one
        let highlight_symbol = if app.no_unicode_symbols {HIGHLIGHT_SYMBOL} else {UNI_HIGHLIGHT_SYMBOL};
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL))
            .highlight_style(
                Style::default()
                    .bg(ROON_BRAND_COLOR)
                    .add_modifier(Modifier::BOLD)
            )
            .highlight_symbol(highlight_symbol);

        // We can now render the item list
        frame.render_stateful_widget(list, area, &mut app.zones.state);
    }

    frame.render_widget(block, area);
}

fn draw_grouping_view(frame: &mut Frame, area: Rect, app: &mut App) -> Option<()> {
    let view = if app.selected_view == Some(View::GroupingPreset) {
        View::GroupingPreset
    } else {
        View::Grouping
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(app, Some(&view)));
    let area = bottom_right_rect(50, 50, area);
    let vchunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Min(2),
                Constraint::Min(5),
            ]
            .as_ref(),
        )
        .horizontal_margin(1)
        .split(area);

    let list_area = Rect::new(
        vchunks[1].x,
        vchunks[1].y,
        vchunks[1].width,
        vchunks[1].height.saturating_sub(1)
    );

    frame.render_widget(Clear, area);   // This clears out the background

    if view == View::GroupingPreset {
        let max_len = vchunks[0].width.saturating_sub(1) as usize;
        app.set_max_input_len(max_len);

        let input = vec![
            Line::from(""),                 // Hidden underneath border
            Line::from(Span::styled(app.input.as_str(), Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD)))
        ];
        let input = Paragraph::new(input)
            .style(Style::default().fg(ROON_BRAND_COLOR));

        frame.render_widget(input, vchunks[0]);

        // Make the cursor visible and ask ratatui to put it at the specified coordinates after
        // rendering
        frame.set_cursor(
            // Draw the cursor at the current position in the input field.
            // This position can be controlled via the left and right arrow key
            vchunks[0].x + app.cursor_position.clamp(0, max_len) as u16,
            // Move one line down, from the border to the input line
            vchunks[0].y + 1,
        );
    } else {
        let ouput_ids = app.get_included_output_ids(app.grouping.items.as_ref()?);
        let zone_name = if ouput_ids.len() <= 1 {
            app.selected_zone.as_ref()?.display_name.as_str()
        } else if !app.input.is_empty() {
            app.input.as_str()
        } else if app.matched_draft_preset.is_some() {
            app.matched_draft_preset.as_deref()?
        } else {
            app.selected_zone.as_ref()?.display_name.as_str()
        };
        let zone_name = vec![
            Line::from(""),                 // Hidden underneath border
            Line::from(Span::styled(zone_name, Style::default().fg(Color::Reset).add_modifier(Modifier::BOLD))),
        ];
        let page_lines = list_area.height as usize;

        app.grouping.prepare_paging(page_lines, |_| 1);

        frame.render_widget(Paragraph::new(zone_name), vchunks[0]);
    }

    let grouping = app.grouping.items.as_ref()?;
    let checked_symbol = if app.no_unicode_symbols {CHECKED_SYMBOL} else {UNI_CHECKED_SYMBOL};
    let unchecked_symbol = if app.no_unicode_symbols {UNCHECKED_SYMBOL} else {UNI_UNCHECKED_SYMBOL};
    let items: Vec<ListItem> = grouping
        .iter()
        .map(|(_, name, included)| {
            let state = if *included {checked_symbol} else {unchecked_symbol};
            let line = Span::styled(
                format!("{}  {}", state, name),
                get_text_view_style(app, Some(&View::Grouping)));

            ListItem::new(Line::from(line)).style(Style::default())
        })
        .collect();

    // Create a List from all list items and highlight the currently selected one
    let list = List::new(items)
        .block(Block::default())
        .highlight_style(
            Style::default()
                .bg(ROON_BRAND_COLOR)
                .add_modifier(Modifier::BOLD)
        );

    // We can now render the widgets
    frame.render_stateful_widget(list, list_area, &mut app.grouping.state);
    frame.render_widget(block, area);

    Some(())
}

fn draw_help_view(frame: &mut Frame, area: Rect, app: &mut App) {
    let view = Some(&View::Help);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(get_border_view_style(app, view))
        .title(Span::styled(
            "Help",
            get_text_view_style(app, view),
        ))
        .title_alignment(Alignment::Left);
    let chunk = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(2)
        .vertical_margin(1)
        .constraints([Constraint::Percentage(100)])
        .split(area);
    let hor_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .horizontal_margin(2)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(33)].as_ref())
        .split(chunk[0]);
    let max_entries: usize = (hor_chunks[0].height as usize).saturating_sub(2);
    let text = [
        "__Global__",
        "Tab     Next view",
        "Sh-Tab  Previous view",
        "Ctrl-z  Select zone",
        "Ctrl-g  Group zones",
        "Ctrl-Sp Play/Pause",
        "Ctrl-p  Play/Pause",
        "Ctrl-e  Pause at end",
        "Ctrl-Up Volume up",
        "Ctrl-Dn Volume down",
        "Ctrl-Ri Next track",
        "Ctrl-Le Previous track",
        "Ctrl-q  Queue mode",
        "Ctrl-a  Append queue",
        "Ctrl-h  This help page",
        "Ctrl-c  Quit",
        "",
        "__List Controls__",
        "Up      Move up",
        "Down    Move down",
        "Home    Move to top",
        "End     Move to bottom",
        "Page-Up Move page up",
        "Page-Dn Move page down",
        "",
        "__Browse View__",
        "Enter   Select",
        "Esc     Move level up",
        "Ctrl-Hm Browse home",
        "F5      Refresh",
        "a..z    Char jump",
        "Backsp  Prev char jump",
        "",
        "__Queue View__",
        "Enter   Play from here",
        "",
        "__Now Playing View__",
        "m       Mute",
        "u       Unmute",
        "+       Volume up",
        "-       Volume down",
        "r       Toggle repeat",
        "s       Toggle shuffle",
        "",
        "__Zone Select Popup__",
        "Enter   Select zone",
        "Esc     Back to view",
        "Delete  Delete preset",
        "",
        "__Zone Grouping Popup__",
        "Space   Toggle output",
        "Enter   Activate group",
        "s       Save as preset",
        "Esc     Back to view",
        "",
        "__Text Input__",
        "Enter   Confirm input",
        "Esc     Cancel input",
    ];

    frame.render_widget(Clear, chunk[0]);   // This clears out the background

    for column in 0..hor_chunks.len() {
        let start = column * max_entries;
        let end = (start + max_entries).clamp(start, text.len());

        frame.render_widget(create_paragraph(&text[start..end]), hor_chunks[column]);

        if end == text.len() {
            break;
        }
    }

    frame.render_widget(block, chunk[0]);
}

fn create_paragraph<'a>(text: &'a[&str]) -> Paragraph<'a> {
    let block = Block::default()
        .padding(Padding {
            left: 1,
            right: 1,
            top: 1,
            bottom: 0,
        });
    let style = Style::default().fg(Color::Reset);
    let mut lines = Vec::new();

    for line in text {
        if line.starts_with("__") && line.ends_with("__") {
            let bold_line = &line[2..line.len().saturating_sub(2)];
            let bold_style = style.add_modifier(Modifier::BOLD);

            lines.push(Line::from(Span::styled(bold_line, bold_style)))
        } else {
            lines.push(Line::from(Span::styled(*line, style)))
        }
    }

    Paragraph::new(lines).block(block)
}

fn get_border_view_style(app: &App, view: Option<&View>) -> Style {
    let mut style = Style::default();

    if let Some(selected_view) = app.get_selected_view() {
        if let Some(view) = view {
            if *selected_view == *view {
                style = style.fg(ROON_BRAND_COLOR);
            }
        }
    } else if view.is_none() {
        style = style.fg(ROON_BRAND_COLOR);
    } else {
        style = style.fg(CUSTOM_GRAY);
    }

    style
}

fn get_text_view_style(app: &App, view: Option<&View>) -> Style {
    let mut style = Style::default();

    if let Some(selected_view) = app.get_selected_view() {
        if let Some(view) = view {
            if *selected_view == *view {
                style = style.fg(Color::Reset).add_modifier(Modifier::BOLD);
            }
        }
    } else if view.is_none() {
        style = style.fg(Color::Reset).add_modifier(Modifier::BOLD);
    } else {
        style = style.fg(CUSTOM_GRAY);
    }

    style
}

fn get_gauge_view_style(app: &App, view: Option<&View>) -> Style {
    let mut style = Style::default().bg(Color::Rgb(0x30, 0x30, 0x30));

    if let Some(selected_view) = app.get_selected_view() {
        if let Some(view) = view {
            if *selected_view == *view {
                style = style.fg(ROON_BRAND_COLOR);
            } else {
                style = style.fg(CUSTOM_GRAY);
            }
        }
    } else if view.is_some() {
        style = style.fg(Color::Rgb(0x30, 0x30, 0x30));
    }

    style
}

fn upper_bar(rect: Rect) -> Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(3),
            ]
            .as_ref(),
        )
        .split(rect)[0]
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
