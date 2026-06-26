//! All terminal rendering.
//!
//! [`ui`] dispatches to one of three screen renderers based on the current
//! [`AppMode`]. Rendering is a pure function of [`App`] state; nothing here
//! mutates application data beyond the list widget's own scroll/selection.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::app::{calculate_delay, format_time, App, AppMode, ConnectionState, ContentType};

/// Human-readable label for a notable wagon car type (e.g. night-train cars),
/// or `None` for ordinary/unknown types.
fn car_type_label(types: &[String]) -> Option<&'static str> {
    types.iter().find_map(|t| match t.as_str() {
        // Bed and fork/plate need a U+FE0F variation selector so they render
        // full-width; the others are already emoji-presentation.
        "sleeper" => Some("🛏️ Schlafwagen"),
        "couchette" => Some("🛌 Liegewagen"),
        "passenger" => Some("🪑 Sitzwagen"),
        "car" => Some("🚗 Autotransport"),
        "restaurant" => Some("🍽️ Restaurant"),
        _ => None,
    })
}

/// Decode the passenger class(es) from a wagon's `symbol` code, e.g.
/// `W_1` → "1. Klasse", `W_1_B` → "1. Klasse + Business",
/// `W_C_1` → "Comfort + 1. Klasse". Returns `None` for symbols that carry no
/// class (locomotives, sleepers, couchettes, car-carriers — covered by `type`).
fn class_label(symbol: &str) -> Option<String> {
    let parts: Vec<&str> = symbol
        .split('_')
        .skip(1) // drop the vehicle prefix (W / TW / L)
        .filter_map(|seg| match seg {
            "1" => Some("1. Klasse"),
            "2" => Some("2. Klasse"),
            "B" => Some("Business"),
            "C" => Some("Comfort"),
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" + "))
}

/// Map a delay (minutes) to its colour: green on time, yellow up to 5 min late,
/// red beyond that.
fn delay_color(delay: i64) -> Color {
    if delay > 5 {
        Color::Red
    } else if delay > 0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// Render the current frame, dispatching on the active mode.
pub fn ui(f: &mut Frame, app: &mut App) {
    match app.mode {
        AppMode::StationSelect => render_station_select(f, app),
        AppMode::TrainDetail => render_train_detail(f, app),
        AppMode::Normal => render_main(f, app),
    }
}

/// The main board: title, the departures/arrivals table, station notices,
/// and a status line with connection state and key hints.
fn render_main(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(5),
            Constraint::Length(3),
        ])
        .split(f.area());

    let title_text = match app.content_type {
        ContentType::Departure => format!("🚂 ABFAHRTEN - {}", app.station_name),
        ContentType::Arrival => format!("🚂 ANKÜNFTE - {}", app.station_name),
    };

    let title = Paragraph::new(title_text)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    let header_cells: Vec<Cell> = [
        "#",
        "ZEIT",
        "IST",
        "VERSP.",
        "ZUG",
        "LINIE",
        if app.content_type == ContentType::Departure {
            "ZIEL"
        } else {
            "VON"
        },
        "GLEIS",
        "SEKTOR",
        "BEMERKUNGEN",
    ]
    .iter()
    .map(|h| {
        Cell::from(*h).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    })
    .collect();

    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.items.iter().enumerate().map(|(idx, item)| {
        let scheduled_time = format_time(&item.scheduled);

        let (actual_time, delay_str, delay_col) = if let Some(delay) = calculate_delay(item) {
            let actual = item
                .expected
                .as_ref()
                .map(|e| format_time(e))
                .unwrap_or_else(|| "-".to_string());
            let delay_text = if delay > 0 {
                format!("+{}", delay)
            } else {
                delay.to_string()
            };
            (actual, delay_text, delay_color(delay))
        } else {
            ("-".to_string(), "-".to_string(), Color::Gray)
        };

        let train = item.train.clone();
        let line = item
            .line
            .clone()
            .or_else(|| item.product.clone())
            .unwrap_or_default();

        let dest = match app.content_type {
            ContentType::Departure => item
                .destination
                .as_ref()
                .map(|d| d.default.clone())
                .unwrap_or_else(|| "N/A".to_string()),
            ContentType::Arrival => item
                .origin
                .as_ref()
                .map(|o| o.default.clone())
                .unwrap_or_else(|| "N/A".to_string()),
        };

        let track = item.track.clone().unwrap_or_else(|| "-".to_string());
        let sector = item.sector.clone().unwrap_or_else(|| "-".to_string());
        let remarks = item
            .remarks
            .as_ref()
            .and_then(|r| r.first())
            // Remark text can contain embedded newlines; flatten to a space so
            // it stays on the single-height table row.
            .map(|r| r.text.default.replace('\n', " "))
            .unwrap_or_default();

        let number_str = match idx {
            0..=8 => format!("{}", idx + 1),
            9 => "0".to_string(),
            _ => " ".to_string(),
        };
        let is_selected = app.selected_train_index == Some(idx);

        let mut row = Row::new(vec![
            Cell::from(number_str).style(if is_selected {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            }),
            Cell::from(scheduled_time),
            Cell::from(actual_time),
            Cell::from(delay_str).style(Style::default().fg(delay_col)),
            Cell::from(train),
            Cell::from(line).style(Style::default().fg(Color::Cyan)),
            Cell::from(dest),
            Cell::from(track).style(Style::default().fg(Color::Magenta)),
            Cell::from(sector),
            Cell::from(remarks),
        ]);

        if is_selected {
            row = row.style(Style::default().bg(Color::DarkGray));
        }

        row
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(25),
            Constraint::Length(7),
            Constraint::Length(8),
            Constraint::Min(15),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Züge"));

    f.render_widget(table, chunks[1]);

    // Ratatui's Paragraph wraps for us (correctly accounting for character
    // width), so we just emit one bullet line per notice and let it flow.
    let notices_text: Vec<Line> = app
        .special_notices
        .iter()
        .map(|notice| Line::from(format!("• {}", notice.text.default)))
        .collect();

    let notices = Paragraph::new(notices_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Hinweise"))
        .wrap(Wrap { trim: true });
    f.render_widget(notices, chunks[2]);

    let (update_text, update_color) = match &app.connection {
        ConnectionState::Connected => match app.last_update {
            Some(t) => (t.format("%H:%M:%S").to_string(), Color::White),
            None => ("Verbunden".to_string(), Color::Green),
        },
        ConnectionState::Connecting => ("Verbinde...".to_string(), Color::Yellow),
        ConnectionState::Failed(reason) => (format!("Fehler: {}", reason), Color::Red),
    };

    let status_text = vec![Line::from(vec![
        Span::styled("Letzte Aktualisierung: ", Style::default().fg(Color::Gray)),
        Span::styled(update_text, Style::default().fg(update_color)),
    ])];

    let status =
        Paragraph::new(status_text).block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(status, chunks[3]);

    let shortcuts_width = 56u16;
    let shortcuts_x = chunks[3].x + chunks[3].width.saturating_sub(shortcuts_width + 2);
    let shortcuts_y = chunks[3].y + 1;

    if shortcuts_x > chunks[3].x {
        let shortcuts = Line::from(vec![
            Span::styled(
                "1-9,0",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("/"),
            Span::styled(
                "↑↓",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("+"),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Detail "),
            Span::styled(
                "[A]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("nk "),
            Span::styled(
                "[D]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("ep "),
            Span::styled(
                "[S]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("tn "),
            Span::styled(
                "[R]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("ef "),
            Span::styled(
                "[Q]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("uit"),
        ]);

        let shortcuts_area = Rect {
            x: shortcuts_x,
            y: shortcuts_y,
            width: shortcuts_width,
            height: 1,
        };

        let shortcuts_widget = Paragraph::new(shortcuts).alignment(Alignment::Right);
        f.render_widget(shortcuts_widget, shortcuts_area);
    }
}

/// The centred station-picker popup: search field, filtered list, key hints.
fn render_station_select(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let popup_area = centered_rect(60, 70, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(popup_area);

    let block = Block::default()
        .title("Station wählen")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    f.render_widget(block, popup_area);

    let search_text = format!("Suche: {}_", app.station_search);
    let search = Paragraph::new(search_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(search, chunks[0]);

    let items: Vec<ListItem> = app
        .filtered_stations
        .iter()
        .enumerate()
        .map(|(i, (_, name))| ListItem::new(format!("{:2}. {}", i + 1, name)))
        .collect();

    let list_title = if app.total_filtered_count > app.filtered_stations.len() {
        format!(
            "Stationen ({} von {} angezeigt)",
            app.filtered_stations.len(),
            app.total_filtered_count
        )
    } else {
        format!("Stationen ({})", app.filtered_stations.len())
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(list_title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, chunks[1], &mut app.station_list_state);

    let help = Paragraph::new("↑↓: Navigieren | Enter: Auswählen | Esc: Abbrechen")
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);
}

/// The centred detail popup for the selected train: timing, platform, operator,
/// remarks, intermediate stops, and physical formation with amenities.
fn render_train_detail(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let popup_area = centered_rect(80, 85, area);

    let train = app.selected_train_index.and_then(|idx| app.items.get(idx));

    if let Some(train) = train {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));
        f.render_widget(block, popup_area);

        let line = train
            .line
            .as_deref()
            .or(train.product.as_deref())
            .unwrap_or("?");
        let dest = train
            .destination
            .as_ref()
            .or(train.origin.as_ref())
            .map(|d| d.default.as_str())
            .unwrap_or("N/A");
        let title_text = format!("🚂 Zug {} - {} → {}", train.train, line, dest);
        let title = Paragraph::new(title_text)
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);

        let mut content_lines = Vec::new();

        content_lines.push(Line::from(vec![
            Span::styled("Abfahrt: ", Style::default().fg(Color::Yellow)),
            Span::raw(format_time(&train.scheduled)),
        ]));

        if let Some(delay) = calculate_delay(train) {
            let delay_text = if delay > 0 {
                format!("+{} Min", delay)
            } else {
                format!("{} Min", delay)
            };
            content_lines.push(Line::from(vec![
                Span::styled("Verspätung: ", Style::default().fg(Color::Yellow)),
                Span::styled(delay_text, Style::default().fg(delay_color(delay))),
            ]));
        }

        if let Some(track) = &train.track {
            content_lines.push(Line::from(vec![
                Span::styled("Gleis: ", Style::default().fg(Color::Yellow)),
                Span::styled(track.clone(), Style::default().fg(Color::Magenta)),
                Span::raw(" "),
                Span::styled(
                    train
                        .sector
                        .as_ref()
                        .map(|s| format!("Sektor {}", s))
                        .unwrap_or_default(),
                    Style::default().fg(Color::Magenta),
                ),
            ]));
        }

        if let Some(operator) = &train.operator {
            content_lines.push(Line::from(vec![
                Span::styled("Betreiber: ", Style::default().fg(Color::Yellow)),
                Span::raw(operator.clone()),
            ]));
        }

        if let Some(remarks) = &train.remarks {
            if !remarks.is_empty() {
                content_lines.push(Line::from(Span::styled(
                    "Bemerkungen:",
                    Style::default().fg(Color::Yellow),
                )));
                for remark in remarks {
                    // A single remark may carry embedded newlines (e.g.
                    // "Kurzzug\nSektor B einsteigen"); render each as its own
                    // line since ratatui does not split them within a Line.
                    for line in remark.text.default.split('\n') {
                        content_lines.push(Line::from(Span::styled(
                            line.to_string(),
                            Style::default().fg(Color::Red),
                        )));
                    }
                }
            }
        }

        content_lines.push(Line::from(""));

        // Prioritized stops render independently of the full via list, so they
        // still show when the detailed via string is missing.
        if let Some(prioritized) = &train.prioritized_vias {
            if !prioritized.is_empty() {
                content_lines.push(Line::from(Span::styled(
                    format!("Wichtige Halte: {}", prioritized.join(" ~ ")),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )));
            }
        }

        if let Some(via) = &train.via {
            let stops: Vec<String> = via
                .default
                .split('~')
                .map(|s| s.replace("&#8203;", "").trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if !stops.is_empty() {
                content_lines.push(Line::from(Span::styled(
                    "Alle Zwischenhalte:",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )));
                content_lines.push(Line::from(format!("  {}", stops.join(" → "))));
            }
        }

        content_lines.push(Line::from(""));

        if let Some(formation) = &train.formation {
            if !formation.is_empty() {
                content_lines.push(Line::from(Span::styled(
                    "Zugformation:",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )));

                for wagon in formation {
                    let is_locomotive = wagon
                        .car_type
                        .as_deref()
                        .map(|t| t.iter().any(|c| c == "engine"))
                        .unwrap_or(false)
                        || wagon.wagon_number.is_none();
                    let mut wagon_line = if is_locomotive {
                        vec![Span::styled(
                            "  🚂 Lokomotive",
                            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                        )]
                    } else {
                        vec![
                            Span::raw("  Wagen "),
                            Span::styled(
                                wagon.wagon_number.as_deref().unwrap_or("?"),
                                Style::default().fg(Color::Yellow),
                            ),
                        ]
                    };

                    // Car type (sleeper / couchette / seated / restaurant) and
                    // passenger class (1./2. Klasse, Business, Comfort) — most
                    // relevant for night and long-distance trains.
                    if !is_locomotive {
                        if let Some(label) = wagon.car_type.as_deref().and_then(car_type_label) {
                            wagon_line.push(Span::styled(
                                format!(" {}", label),
                                Style::default().fg(Color::Blue),
                            ));
                        }
                        if let Some(class) = wagon.symbol.as_deref().and_then(class_label) {
                            wagon_line.push(Span::styled(
                                format!(" · {}", class),
                                Style::default().fg(Color::Green),
                            ));
                        }
                    }

                    if wagon.closed == Some(true) {
                        wagon_line.push(Span::styled(
                            " (geschlossen)",
                            Style::default().fg(Color::DarkGray),
                        ));
                    }

                    if let Some(sector) = &wagon.sector {
                        wagon_line.push(Span::styled(
                            format!(" [Sektor {}]", sector),
                            Style::default().fg(Color::Magenta),
                        ));
                    }
                    if let Some(dest) = &wagon.destination {
                        if !dest.is_empty() {
                            wagon_line.push(Span::styled(
                                format!(" → {}", dest),
                                Style::default().fg(Color::Cyan),
                            ));
                        }
                    }

                    if !is_locomotive {
                        wagon_line.push(Span::raw(": "));
                    }

                    if let Some(icons) = &wagon.icons {
                        let icon_strs: Vec<String> = icons
                            .iter()
                            .map(|icon| {
                                match icon.as_str() {
                                    "wlan" => "📶 WLAN",
                                    "bicycle" => "🚲 Fahrrad",
                                    "disabled" => "♿ Rollstuhl",
                                    "bistro" => "🍽️ Bistro",
                                    "motherchild" => "👪 Familie",
                                    "silence" => "🔇 Ruhe",
                                    _ => icon.as_str(),
                                }
                                .to_string()
                            })
                            .collect();
                        wagon_line.push(Span::styled(
                            icon_strs.join(" | "),
                            Style::default().fg(Color::Green),
                        ));
                    }

                    content_lines.push(Line::from(wagon_line));
                }
            }
        }

        let content = Paragraph::new(content_lines)
            .block(Block::default().borders(Borders::ALL).title("Details"))
            .wrap(Wrap { trim: true })
            .scroll((app.detail_scroll, 0));
        f.render_widget(content, chunks[1]);

        let help = Paragraph::new("↑↓: Züge | PgUp/PgDn: Scrollen | Esc/Q: Schließen")
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[2]);
    } else {
        let msg = Paragraph::new("Kein Zug ausgewählt")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(msg, popup_area);
    }
}

/// Compute a rectangle centred within `r`, sized as the given percentages of
/// its width and height.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_colors() {
        assert_eq!(delay_color(0), Color::Green);
        assert_eq!(delay_color(-3), Color::Green);
        assert_eq!(delay_color(3), Color::Yellow);
        assert_eq!(delay_color(5), Color::Yellow);
        assert_eq!(delay_color(6), Color::Red);
    }

    #[test]
    fn car_types() {
        let t = |s: &str| car_type_label(&[s.to_string()]);
        assert_eq!(t("sleeper"), Some("🛏️ Schlafwagen"));
        assert_eq!(t("restaurant"), Some("🍽️ Restaurant"));
        assert_eq!(t("car"), Some("🚗 Autotransport"));
        assert_eq!(t("unknown"), None);
    }

    #[test]
    fn class_labels() {
        assert_eq!(class_label("W_1").as_deref(), Some("1. Klasse"));
        assert_eq!(class_label("W_2").as_deref(), Some("2. Klasse"));
        assert_eq!(class_label("W_1_B").as_deref(), Some("1. Klasse + Business"));
        assert_eq!(class_label("W_C_1").as_deref(), Some("Comfort + 1. Klasse"));
        assert_eq!(class_label("TW_B_1").as_deref(), Some("Business + 1. Klasse"));
        // Restaurant section (S) carries no passenger class of its own.
        assert_eq!(class_label("W_1_S").as_deref(), Some("1. Klasse"));
        // Sleepers/couchettes/locos have no class code.
        assert_eq!(class_label("W_Schlaf"), None);
        assert_eq!(class_label("L_L"), None);
    }
}
