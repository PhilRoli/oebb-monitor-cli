use anyhow::Result;
use chrono::{DateTime, Local};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Wrap},
    Frame, Terminal,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::{self, Write},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

// Debug logger
struct DebugLogger {
    enabled: bool,
    file: Option<std::sync::Mutex<std::fs::File>>,
}

impl DebugLogger {
    fn new(enabled: bool) -> Self {
        let file = if enabled {
            std::fs::File::create("/tmp/oebb-debug.log")
                .ok()
                .map(std::sync::Mutex::new)
        } else {
            None
        };
        Self { enabled, file }
    }

    fn log(&self, msg: String) {
        if !self.enabled {
            return;
        }
        if let Some(ref file) = self.file {
            if let Ok(mut f) = file.lock() {
                let timestamp = Local::now().format("%H:%M:%S%.3f");
                let _ = writeln!(f, "[{}] {}", timestamp, msg);
                let _ = f.flush();
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref DEBUG: DebugLogger = {
        let enabled = std::env::args().any(|arg| arg == "--debug" || arg == "-d");
        DebugLogger::new(enabled)
    };
}

macro_rules! debug {
    ($($arg:tt)*) => {
        DEBUG.log(format!($($arg)*))
    };
}

#[derive(Debug, Clone, Deserialize)]
struct Destination {
    default: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TrainItem {
    id: String,
    train: String,
    line: Option<String>,
    product: Option<String>,
    scheduled: String,
    expected: Option<String>,
    destination: Option<Destination>,
    origin: Option<Destination>,
    track: Option<String>,
    sector: Option<String>,
    remarks: Option<Vec<Remark>>,
    via: Option<Destination>,
    #[serde(rename = "prioritizedVias")]
    prioritized_vias: Option<Vec<String>>,
    operator: Option<String>,
    formation: Option<Vec<Formation>>,
}

#[derive(Debug, Clone, Deserialize)]
struct Formation {
    #[serde(rename = "wagonNumber")]
    wagon_number: Option<String>,
    icons: Option<Vec<String>>,
    sector: Option<String>,
    destination: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Remark {
    text: Destination,
}

#[derive(Debug, Clone, Deserialize)]
struct SpecialNotice {
    text: Destination,
}

#[derive(Debug, Clone, Deserialize)]
struct TrainData {
    departures: Option<Vec<TrainItem>>,
    arrivals: Option<Vec<TrainItem>>,
    #[serde(rename = "specialNotices")]
    special_notices: Option<Vec<SpecialNotice>>,
}

#[derive(Debug, Clone, Deserialize)]
struct UpdateParams {
    data: TrainData,
}

#[derive(Debug, Clone, Deserialize)]
struct WsMessage {
    method: Option<String>,
    params: Option<UpdateParams>,
}

#[derive(Clone, PartialEq, Debug)]
enum ContentType {
    Departure,
    Arrival,
}

#[derive(Clone, PartialEq, Debug)]
enum AppMode {
    Normal,
    StationSelect,
    TrainDetail,
}

struct App {
    content_type: ContentType,
    station_id: String,
    station_name: String,
    items: Vec<TrainItem>,
    special_notices: Vec<SpecialNotice>,
    last_update: Option<DateTime<Local>>,
    mode: AppMode,
    stations: HashMap<String, String>,
    filtered_stations: Vec<(String, String)>,
    station_search: String,
    station_list_state: ListState,
    max_pages: usize,
    selected_train_index: Option<usize>,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            content_type: ContentType::Departure,
            station_id: "8101001".to_string(),
            station_name: "Wien Westbahnhof".to_string(),
            items: Vec::new(),
            special_notices: Vec::new(),
            last_update: None,
            mode: AppMode::Normal,
            stations: HashMap::new(),
            filtered_stations: Vec::new(),
            station_search: String::new(),
            station_list_state: ListState::default(),
            max_pages: 5,
            selected_train_index: None,
        };

        // Load stations from embedded JSON
        const STATIONS_JSON: &str = include_str!("../stations.json");
        
        if let Ok(stations) = serde_json::from_str::<HashMap<String, String>>(STATIONS_JSON) {
            // Trim whitespace from station IDs
            app.stations = stations.into_iter()
                .map(|(k, v)| (k.trim().to_string(), v))
                .collect();
            debug!("Loaded {} stations from embedded data", app.stations.len());
        } else {
            debug!("Failed to parse embedded stations.json");
        }

        app
    }

    fn enter_station_select(&mut self) {
        self.mode = AppMode::StationSelect;
        self.station_search.clear();
        self.update_filtered_stations();
        self.station_list_state.select(Some(0));
    }

    fn exit_station_select(&mut self) {
        self.mode = AppMode::Normal;
    }

    fn update_filtered_stations(&mut self) {
        if self.station_search.is_empty() {
            self.filtered_stations = self.stations.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        } else {
            let search_lower = self.station_search.to_lowercase();
            self.filtered_stations = self
                .stations
                .iter()
                .filter(|(_, name)| name.to_lowercase().contains(&search_lower))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        }

        self.filtered_stations.sort_by(|a, b| a.1.cmp(&b.1));
        self.filtered_stations.truncate(20);

        if !self.filtered_stations.is_empty() {
            self.station_list_state.select(Some(0));
        }
    }

    fn select_station(&mut self) -> bool {
        if let Some(selected) = self.station_list_state.selected() {
            if selected < self.filtered_stations.len() {
                let (id, name) = &self.filtered_stations[selected];
                self.station_id = id.trim().to_string();
                self.station_name = name.clone();
                self.exit_station_select();
                return true;
            }
        }
        false
    }

    fn get_ws_url(&self, page: usize) -> String {
        let content = match self.content_type {
            ContentType::Departure => "departure",
            ContentType::Arrival => "arrival",
        };
        format!(
            "wss://meine.oebb.at/abfahrtankunft/webdisplay/web_client/ws/?stationId={}&contentType={}&staticLayout=false&page={}&offset=0&ignoreIncident=false&expandAll=false",
            self.station_id, content, page
        )
    }

    fn calculate_delay(&self, item: &TrainItem) -> Option<i64> {
        if let Some(expected) = &item.expected {
            if let (Ok(scheduled), Ok(exp)) =
                (DateTime::parse_from_rfc3339(&item.scheduled), DateTime::parse_from_rfc3339(expected))
            {
                let delay = (exp - scheduled).num_minutes();
                if delay != 0 {
                    return Some(delay);
                }
            }
        }
        None
    }
}

fn format_time(iso_time: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso_time) {
        let local: DateTime<Local> = dt.into();
        local.format("%H:%M").to_string()
    } else {
        "-".to_string()
    }
}

async fn run_websocket(app: Arc<Mutex<App>>, mut reconnect_rx: mpsc::Receiver<()>) -> Result<()> {
    debug!("WebSocket handler started");
    let mut active_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let mut iteration = 0;

    loop {
        iteration += 1;
        debug!("=== WebSocket iteration {} ===", iteration);
        
        // Abort all existing tasks
        debug!("Aborting {} existing tasks", active_tasks.len());
        for task in active_tasks.drain(..) {
            task.abort();
        }

        let (max_pages, station_id, content_type) = {
            let mut app_guard = app.lock().await;
            debug!("Clearing {} items", app_guard.items.len());
            app_guard.items.clear();
            app_guard.last_update = None;
            let pages = app_guard.max_pages;
            let sid = app_guard.station_id.clone();
            let ct = app_guard.content_type.clone();
            (pages, sid, ct)
        };

        debug!("Station: {}, ContentType: {:?}, Pages: {}", station_id, content_type, max_pages);

        // Create channel for page updates
        let (page_tx, mut page_rx) = mpsc::channel(100);

        // Spawn tasks for each page
        for page in 1..=max_pages {
            let url = {
                let app = app.lock().await;
                app.get_ws_url(page)
            };
            debug!("Spawning task for page {}: {}", page, url);
            let tx = page_tx.clone();

            let task = tokio::spawn(async move {
                debug!("Page {} task started, connecting...", page);
                match connect_async(&url).await {
                    Ok((ws_stream, _)) => {
                        debug!("Page {} connected successfully", page);
                        let (_, mut read) = ws_stream.split();
                        let mut msg_count = 0;

                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(Message::Text(text)) => {
                                    msg_count += 1;
                                    debug!("Page {} received message #{} ({} bytes)", page, msg_count, text.len());
                                    
                                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                                        if ws_msg.method.as_deref() == Some("update") {
                                            debug!("Page {} got update message", page);
                                            if let Some(params) = ws_msg.params {
                                                let _ = tx.send((page, params)).await;
                                            }
                                        } else {
                                            debug!("Page {} got non-update message: {:?}", page, ws_msg.method);
                                        }
                                    } else {
                                        debug!("Page {} failed to parse message", page);
                                    }
                                }
                                Ok(Message::Close(reason)) => {
                                    debug!("Page {} WebSocket closed: {:?}", page, reason);
                                    break;
                                }
                                Err(e) => {
                                    debug!("Page {} WebSocket error: {}", page, e);
                                    break;
                                }
                                _ => {}
                            }
                        }
                        debug!("Page {} task ending after {} messages", page, msg_count);
                    }
                    Err(e) => {
                        debug!("Page {} failed to connect: {}", page, e);
                    }
                }
            });

            active_tasks.push(task);
        }

        drop(page_tx);
        debug!("Spawned {} tasks, now listening for updates", active_tasks.len());

        // Process updates or wait for reconnect signal
        let mut update_count = 0;
        loop {
            tokio::select! {
                result = page_rx.recv() => {
                    match result {
                        Some((page, params)) => {
                            update_count += 1;
                            debug!("Received update #{} from page {}", update_count, page);
                            
                            let mut app = app.lock().await;

                            if app.mode == AppMode::Normal {
                                let new_items = match app.content_type {
                                    ContentType::Departure => params.data.departures.unwrap_or_default(),
                                    ContentType::Arrival => params.data.arrivals.unwrap_or_default(),
                                };

                                debug!("Page {} has {} items", page, new_items.len());
                                
                                let before_count = app.items.len();
                                // Merge items, avoiding duplicates
                                for item in new_items {
                                    if !app.items.iter().any(|i| i.id == item.id) {
                                        app.items.push(item);
                                    }
                                }
                                let after_count = app.items.len();
                                debug!("Merged items: {} -> {} (added {})", before_count, after_count, after_count - before_count);

                                // Sort by scheduled time
                                app.items.sort_by(|a, b| a.scheduled.cmp(&b.scheduled));

                                if let Some(notices) = params.data.special_notices {
                                    debug!("Updated special notices: {}", notices.len());
                                    app.special_notices = notices;
                                }
                                app.last_update = Some(Local::now());
                            } else {
                                debug!("Skipping update, app in {:?} mode", app.mode);
                            }
                        }
                        None => {
                            debug!("All page channels closed after {} updates, will reconnect", update_count);
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            break;
                        }
                    }
                }
                _ = reconnect_rx.recv() => {
                    debug!("!!! RECONNECT SIGNAL RECEIVED after {} updates !!!", update_count);
                    break;
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    match app.mode {
        AppMode::StationSelect => render_station_select(f, app),
        AppMode::TrainDetail => render_train_detail(f, app),
        AppMode::Normal => render_main(f, app),
    }
}

fn render_main(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10), Constraint::Length(5), Constraint::Length(3)])
        .split(f.area());

    let title_text = match app.content_type {
        ContentType::Departure => format!("🚂 ABFAHRTEN - {}", app.station_name),
        ContentType::Arrival => format!("🚂 ANKÜNFTE - {}", app.station_name),
    };

    let title = Paragraph::new(title_text)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
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
        if app.content_type == ContentType::Departure { "ZIEL" } else { "VON" },
        "GLEIS",
        "SEKTOR",
        "BEMERKUNGEN",
    ]
    .iter()
    .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
    .collect();

    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.items.iter().enumerate().map(|(idx, item)| {
        let scheduled_time = format_time(&item.scheduled);

        let (actual_time, delay_str, delay_color) = if let Some(delay) = app.calculate_delay(item) {
            let actual = item.expected.as_ref().map(|e| format_time(e)).unwrap_or_else(|| "-".to_string());
            let delay_text = if delay > 0 { format!("+{}", delay) } else { delay.to_string() };
            let color = if delay > 5 { Color::Red } else if delay > 0 { Color::Yellow } else { Color::Green };
            (actual, delay_text, color)
        } else {
            ("-".to_string(), "-".to_string(), Color::Gray)
        };

        let train = item.train.clone();
        let line = item.line.clone().or_else(|| item.product.clone()).unwrap_or_default();

        let dest = match app.content_type {
            ContentType::Departure => item.destination.as_ref().map(|d| d.default.clone()).unwrap_or_else(|| "N/A".to_string()),
            ContentType::Arrival => item.origin.as_ref().map(|o| o.default.clone()).unwrap_or_else(|| "N/A".to_string()),
        };

        let track = item.track.clone().unwrap_or_else(|| "-".to_string());
        let sector = item.sector.clone().unwrap_or_else(|| "-".to_string());
        let remarks = item.remarks.as_ref().and_then(|r| r.first()).map(|r| r.text.default.clone()).unwrap_or_default();

        let number_str = if idx < 9 { format!("{}", idx + 1) } else { " ".to_string() };
        let is_selected = app.selected_train_index == Some(idx);
        
        let mut row = Row::new(vec![
            Cell::from(number_str).style(if is_selected { 
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) 
            } else { 
                Style::default().fg(Color::DarkGray) 
            }),
            Cell::from(scheduled_time),
            Cell::from(actual_time),
            Cell::from(delay_str).style(Style::default().fg(delay_color)),
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

    let notices_text: Vec<Line> = app
        .special_notices
        .iter()
        .flat_map(|notice| {
            let text = &notice.text.default;
            let words: Vec<&str> = text.split_whitespace().collect();
            let mut lines = Vec::new();
            let mut current_line = String::new();

            for word in words {
                if current_line.len() + word.len() + 1 > (chunks[2].width as usize - 4) {
                    if !current_line.is_empty() {
                        lines.push(Line::from(format!("• {}", current_line)));
                        current_line = word.to_string();
                    }
                } else {
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                }
            }

            if !current_line.is_empty() {
                lines.push(Line::from(format!("• {}", current_line)));
            }
            lines
        })
        .collect();

    let notices = Paragraph::new(notices_text)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Hinweise"))
        .wrap(Wrap { trim: true });
    f.render_widget(notices, chunks[2]);

    let status_text = vec![
        Line::from(vec![
            Span::styled("Letzte Aktualisierung: ", Style::default().fg(Color::Gray)),
            Span::styled(
                app.last_update.map(|t| t.format("%H:%M:%S").to_string()).unwrap_or_else(|| "-".to_string()),
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let status = Paragraph::new(status_text).block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(status, chunks[3]);

    // Render keyboard shortcuts right-aligned in the status bar
    let shortcuts_width = 45; // Approximate width of the shortcuts text
    let shortcuts_x = chunks[3].x + chunks[3].width.saturating_sub(shortcuts_width + 2);
    let shortcuts_y = chunks[3].y + 1;

    if shortcuts_x > chunks[3].x {
        let shortcuts = Line::from(vec![
            Span::styled("1-9", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("↑↓", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("+"),
            Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw(" Info "),
            Span::styled("[A]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("nk "),
            Span::styled("[D]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("ep "),
            Span::styled("[S]", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("tn "),
            Span::styled("[Q]", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
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

fn render_station_select(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup_area = centered_rect(60, 70, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10), Constraint::Length(3)])
        .split(popup_area);

    let block = Block::default().title("Station wählen").borders(Borders::ALL).style(Style::default().bg(Color::Black));
    f.render_widget(block, popup_area);

    let search_text = format!("Suche: {}_", app.station_search);
    let search = Paragraph::new(search_text).style(Style::default().fg(Color::Yellow)).block(Block::default().borders(Borders::ALL));
    f.render_widget(search, chunks[0]);

    let items: Vec<ListItem> = app
        .filtered_stations
        .iter()
        .enumerate()
        .map(|(i, (_, name))| ListItem::new(format!("{:2}. {}", i + 1, name)))
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!("Gefundene Stationen ({})", app.filtered_stations.len())))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, chunks[1], &mut app.station_list_state.clone());

    let help = Paragraph::new("↑↓: Navigieren | Enter: Auswählen | Esc: Abbrechen")
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[2]);
}

fn render_train_detail(f: &mut Frame, app: &App) {
    let area = f.area();
    let popup_area = centered_rect(80, 85, area);

    let train = app.selected_train_index
        .and_then(|idx| app.items.get(idx));

    if let Some(train) = train {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Title
                Constraint::Min(5),     // Content
                Constraint::Length(3),  // Help
            ])
            .split(popup_area);

        // Background block
        let block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));
        f.render_widget(block, popup_area);

        // Title
        let title_text = format!(
            "🚂 Zug {} - {} → {}",
            train.train,
            train.line.as_ref().or(train.product.as_ref()).unwrap_or(&"?".to_string()),
            train.destination.as_ref()
                .or(train.origin.as_ref())
                .map(|d| d.default.as_str())
                .unwrap_or("N/A")
        );
        let title = Paragraph::new(title_text)
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);

        // Content
        let mut content_lines = Vec::new();
        
        // Basic info
        content_lines.push(Line::from(vec![
            Span::styled("Abfahrt: ", Style::default().fg(Color::Yellow)),
            Span::raw(format_time(&train.scheduled)),
        ]));
        
        if let Some(delay) = app.calculate_delay(train) {
            let color = if delay > 5 { Color::Red } else if delay > 0 { Color::Yellow } else { Color::Green };
            content_lines.push(Line::from(vec![
                Span::styled("Verspätung: ", Style::default().fg(Color::Yellow)),
                Span::styled(format!("+{} Min", delay), Style::default().fg(color)),
            ]));
        }

        if let Some(track) = &train.track {
            content_lines.push(Line::from(vec![
                Span::styled("Gleis: ", Style::default().fg(Color::Yellow)),
                Span::styled(track.clone(), Style::default().fg(Color::Magenta)),
                Span::raw(" "),
                Span::styled(
                    train.sector.as_ref().map(|s| format!("Sektor {}", s)).unwrap_or_default(),
                    Style::default().fg(Color::Magenta)
                ),
            ]));
        }

        if let Some(operator) = &train.operator {
            content_lines.push(Line::from(vec![
                Span::styled("Betreiber: ", Style::default().fg(Color::Yellow)),
                Span::raw(operator.clone()),
            ]));
        }

        content_lines.push(Line::from(""));

        // Via stations
        if let Some(via) = &train.via {
            // Show prioritized vias if available
            if let Some(prioritized) = &train.prioritized_vias {
                if !prioritized.is_empty() {
                    content_lines.push(Line::from(Span::styled(
                        format!("Wichtige Halte: {}", prioritized.join(" ~ ")),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    )));
                }
            }

            content_lines.push(Line::from(Span::styled(
                "Alle Zwischenhalte:",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            )));
            
            // Split by ~ and wrap
            let stations: Vec<&str> = via.default.split("~").collect();
            let mut current_line = String::new();
            
            for (i, station_raw) in stations.iter().enumerate() {
                let station_cleaned = station_raw.replace("&#8203;", "");
                let station = station_cleaned.trim();
                if station.is_empty() {
                    continue;
                }
                
                let separator = if i == 0 { "" } else { " → " };
                let addition = format!("{}{}", separator, station);
                
                if current_line.len() + addition.len() > (popup_area.width as usize - 8) {
                    if !current_line.is_empty() {
                        content_lines.push(Line::from(format!("  {}", current_line)));
                        current_line = station.to_string();
                    }
                } else {
                    current_line.push_str(&addition);
                }
            }
            
            if !current_line.is_empty() {
                content_lines.push(Line::from(format!("  {}", current_line)));
            }
        }

        content_lines.push(Line::from(""));

        // Formation
        if let Some(formation) = &train.formation {
            if !formation.is_empty() {
                content_lines.push(Line::from(Span::styled(
                    "Zugformation:",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                )));
                
                for wagon in formation {
                    let mut wagon_line = vec![
                        Span::raw("  Wagen "),
                        Span::styled(
                            wagon.wagon_number.as_ref().map(|s| s.as_str()).unwrap_or("?"),
                            Style::default().fg(Color::Yellow)
                        ),
                    ];
                    
                    // Add sector and destination if available
                    if let Some(sector) = &wagon.sector {
                        wagon_line.push(Span::styled(
                            format!(" [Sektor {}]", sector),
                            Style::default().fg(Color::Magenta)
                        ));
                    }
                    if let Some(dest) = &wagon.destination {
                        wagon_line.push(Span::styled(
                            format!(" → {}", dest),
                            Style::default().fg(Color::Cyan)
                        ));
                    }
                    
                    wagon_line.push(Span::raw(": "));
                    
                    if let Some(icons) = &wagon.icons {
                        let icon_strs: Vec<String> = icons.iter().map(|icon| {
                            match icon.as_str() {
                                "wlan" => "📶 WLAN",
                                "bicycle" => "🚲 Fahrrad",
                                "disabled" => "♿ Rollstuhl",
                                "bistro" => "🍽️ Bistro",
                                _ => icon.as_str(),
                            }
                            .to_string()
                        }).collect();
                        wagon_line.push(Span::styled(icon_strs.join(" | "), Style::default().fg(Color::Green)));
                    }
                    
                    content_lines.push(Line::from(wagon_line));
                }
            }
        }

        let content = Paragraph::new(content_lines)
            .block(Block::default().borders(Borders::ALL).title("Details"))
            .wrap(Wrap { trim: true });
        f.render_widget(content, chunks[1]);

        // Help
        let help = Paragraph::new("Esc: Schließen")
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[2]);
    } else {
        // No train selected
        let msg = Paragraph::new("Kein Zug ausgewählt")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(msg, popup_area);
    }
}

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

#[tokio::main]
async fn main() -> Result<()> {
    debug!("Application starting");
    debug!("Debug mode: {}", DEBUG.enabled);
    
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Arc::new(Mutex::new(App::new()));
    let (reconnect_tx, reconnect_rx) = mpsc::channel(10);

    debug!("Spawning WebSocket handler task");
    let ws_app = app.clone();
    tokio::spawn(async move {
        let _ = run_websocket(ws_app, reconnect_rx).await;
    });

    let tick_rate = Duration::from_millis(100);
    let mut last_tick = tokio::time::Instant::now();

    debug!("Entering main event loop");
    loop {
        {
            let app = app.lock().await;
            terminal.draw(|f| ui(f, &app))?;
        }

        let timeout = tick_rate.checked_sub(last_tick.elapsed()).unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                let mut app = app.lock().await;

                match app.mode {
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            debug!("Quit key pressed");
                            break;
                        }
                        KeyCode::Char('a') | KeyCode::Char('A') => {
                            debug!("Switching to Arrivals");
                            app.content_type = ContentType::Arrival;
                            debug!("Sending reconnect signal...");
                            let result = reconnect_tx.send(()).await;
                            debug!("Reconnect signal sent: {:?}", result);
                        }
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            debug!("Switching to Departures");
                            app.content_type = ContentType::Departure;
                            debug!("Sending reconnect signal...");
                            let result = reconnect_tx.send(()).await;
                            debug!("Reconnect signal sent: {:?}", result);
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            debug!("Entering station select mode");
                            app.enter_station_select();
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() => {
                            let digit = c.to_digit(10).unwrap() as usize;
                            let index = if digit == 0 { 9 } else { digit - 1 };
                            if index < app.items.len() {
                                debug!("Selecting train at index {}", index);
                                app.selected_train_index = Some(index);
                                app.mode = AppMode::TrainDetail;
                            }
                        }
                        KeyCode::Up => {
                            if let Some(idx) = app.selected_train_index {
                                if idx > 0 {
                                    app.selected_train_index = Some(idx - 1);
                                }
                            } else if !app.items.is_empty() {
                                app.selected_train_index = Some(0);
                            }
                        }
                        KeyCode::Down => {
                            if let Some(idx) = app.selected_train_index {
                                if idx + 1 < app.items.len() {
                                    app.selected_train_index = Some(idx + 1);
                                }
                            } else if !app.items.is_empty() {
                                app.selected_train_index = Some(0);
                            }
                        }
                        KeyCode::Enter => {
                            if app.selected_train_index.is_some() {
                                debug!("Opening train detail");
                                app.mode = AppMode::TrainDetail;
                            }
                        }
                        _ => {}
                    },
                    AppMode::TrainDetail => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                            debug!("Closing train detail");
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Up => {
                            if let Some(idx) = app.selected_train_index {
                                if idx > 0 {
                                    app.selected_train_index = Some(idx - 1);
                                }
                            }
                        }
                        KeyCode::Down => {
                            if let Some(idx) = app.selected_train_index {
                                if idx + 1 < app.items.len() {
                                    app.selected_train_index = Some(idx + 1);
                                }
                            }
                        }
                        _ => {}
                    },
                    AppMode::StationSelect => match key.code {
                        KeyCode::Esc => {
                            debug!("Exiting station select");
                            app.exit_station_select();
                        }
                        KeyCode::Enter => {
                            let before_station = app.station_id.clone();
                            if app.select_station() {
                                debug!("Station changed: {} -> {}", before_station, app.station_id);
                                debug!("Sending reconnect signal...");
                                let result = reconnect_tx.send(()).await;
                                debug!("Reconnect signal sent: {:?}", result);
                            }
                        }
                        KeyCode::Up => {
                            let i = app.station_list_state.selected().unwrap_or(0);
                            if i > 0 {
                                app.station_list_state.select(Some(i - 1));
                            }
                        }
                        KeyCode::Down => {
                            let i = app.station_list_state.selected().unwrap_or(0);
                            if i < app.filtered_stations.len().saturating_sub(1) {
                                app.station_list_state.select(Some(i + 1));
                            }
                        }
                        KeyCode::Char(c) => {
                            app.station_search.push(c);
                            app.update_filtered_stations();
                        }
                        KeyCode::Backspace => {
                            app.station_search.pop();
                            app.update_filtered_stations();
                        }
                        _ => {}
                    },
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = tokio::time::Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    Ok(())
}
