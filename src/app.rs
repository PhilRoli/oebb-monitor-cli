//! Application state and the pure logic that operates on it.
//!
//! [`App`] is the single source of truth shared (behind a mutex) between the UI
//! loop and the WebSocket task. This module also holds small, side-effect-free
//! helpers ([`format_time`], [`calculate_delay`], [`build_ws_url`]) that are
//! unit-tested in isolation.

use chrono::{DateTime, Local};
use ratatui::widgets::ListState;
use std::collections::HashMap;

use crate::lang::Lang;
use crate::model::{SpecialNotice, TrainItem};

/// Which board to display: departures or arrivals.
#[derive(Clone, PartialEq, Debug)]
pub enum ContentType {
    Departure,
    Arrival,
}

/// The active screen, which determines both rendering and key handling.
#[derive(Clone, PartialEq, Debug)]
pub enum AppMode {
    Normal,
    StationSelect,
    TrainDetail,
}

/// Live-connection status, surfaced in the status bar.
#[derive(Clone, PartialEq, Debug)]
pub enum ConnectionState {
    Connecting,
    Connected,
    /// All page connections failed; the UI renders a translated message.
    Failed,
}

/// The complete application state.
pub struct App {
    pub content_type: ContentType,
    pub station_id: String,
    pub station_name: String,
    pub items: Vec<TrainItem>,
    pub special_notices: Vec<SpecialNotice>,
    pub last_update: Option<DateTime<Local>>,
    pub connection: ConnectionState,
    pub mode: AppMode,
    pub stations: HashMap<String, String>,
    pub all_stations_sorted: Vec<(String, String)>,
    pub filtered_stations: Vec<(String, String)>,
    pub total_filtered_count: usize,
    pub station_search: String,
    pub station_list_state: ListState,
    pub max_pages: usize,
    pub selected_train_index: Option<usize>,
    pub selected_train_id: Option<String>,
    pub detail_scroll: u16,
    pub lang: Lang,
}

impl App {
    /// Build the initial state, loading the embedded station list and defaulting
    /// to departures at Wien Westbahnhof.
    pub fn new() -> Self {
        let mut app = Self {
            content_type: ContentType::Departure,
            station_id: "8101001".to_string(),
            station_name: "Wien Westbahnhof".to_string(),
            items: Vec::new(),
            special_notices: Vec::new(),
            last_update: None,
            connection: ConnectionState::Connecting,
            mode: AppMode::Normal,
            stations: HashMap::new(),
            all_stations_sorted: Vec::new(),
            filtered_stations: Vec::new(),
            total_filtered_count: 0,
            station_search: String::new(),
            station_list_state: ListState::default(),
            max_pages: 5,
            selected_train_index: None,
            selected_train_id: None,
            detail_scroll: 0,
            lang: Lang::initial(),
        };

        const STATIONS_JSON: &str = include_str!("../stations.json");

        if let Ok(stations) = serde_json::from_str::<HashMap<String, String>>(STATIONS_JSON) {
            app.stations = stations
                .into_iter()
                .map(|(k, v)| (k.trim().to_string(), v))
                .collect();
            debug!("Loaded {} stations from embedded data", app.stations.len());
        } else {
            debug!("Failed to parse embedded stations.json");
        }

        let mut sorted: Vec<(String, String)> = app
            .stations
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        sorted.sort_by(|a, b| a.1.cmp(&b.1));
        app.all_stations_sorted = sorted;

        app
    }

    /// Open the station picker with a cleared search field.
    pub fn enter_station_select(&mut self) {
        self.mode = AppMode::StationSelect;
        self.station_search.clear();
        self.update_filtered_stations();
        self.station_list_state.select(Some(0));
    }

    /// Close the station picker, returning to the main board.
    pub fn exit_station_select(&mut self) {
        self.mode = AppMode::Normal;
    }

    /// Recompute [`Self::filtered_stations`] from the current search string,
    /// capping the visible list at 20 entries while tracking the total match
    /// count for the header.
    pub fn update_filtered_stations(&mut self) {
        if self.station_search.is_empty() {
            self.total_filtered_count = self.all_stations_sorted.len();
            let take = self.all_stations_sorted.len().min(20);
            self.filtered_stations = self.all_stations_sorted[..take].to_vec();
        } else {
            let search_lower = self.station_search.to_lowercase();
            let mut matches: Vec<(String, String)> = self
                .stations
                .iter()
                .filter(|(_, name)| name.to_lowercase().contains(&search_lower))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            matches.sort_by(|a, b| a.1.cmp(&b.1));
            self.total_filtered_count = matches.len();
            matches.truncate(20);
            self.filtered_stations = matches;
        }

        if !self.filtered_stations.is_empty() {
            self.station_list_state.select(Some(0));
        }
    }

    /// Commit the highlighted station as the active one. Returns `true` if a
    /// selection was made (so the caller can trigger a reconnect).
    pub fn select_station(&mut self) -> bool {
        if let Some(selected) = self.station_list_state.selected() {
            if selected < self.filtered_stations.len() {
                let (id, name) = &self.filtered_stations[selected];
                self.station_id = id.clone();
                self.station_name = name.clone();
                self.exit_station_select();
                return true;
            }
        }
        false
    }

    /// Move the train selection by `delta`, clamped to the bounds of the list,
    /// keeping the tracked id in sync and resetting the detail scroll. Unifies
    /// the up/down handling for both the main list and the detail view.
    pub fn select_relative(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let new = match self.selected_train_index {
            Some(idx) => (idx as i32 + delta).clamp(0, self.items.len() as i32 - 1) as usize,
            None => 0,
        };
        self.selected_train_index = Some(new);
        self.selected_train_id = self.items.get(new).map(|t| t.id.clone());
        self.detail_scroll = 0;
    }
}

/// Build the WebSocket URL for one page of a station's board.
pub fn build_ws_url(station_id: &str, content_type: &ContentType, page: usize) -> String {
    let content = match content_type {
        ContentType::Departure => "departure",
        ContentType::Arrival => "arrival",
    };
    format!(
        "wss://meine.oebb.at/abfahrtankunft/webdisplay/web_client/ws/?stationId={}&contentType={}&staticLayout=false&page={}&offset=0&ignoreIncident=false&expandAll=false",
        station_id, content, page
    )
}

/// Format an RFC 3339 timestamp as local `HH:MM`, or `"-"` if unparseable.
pub fn format_time(iso_time: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso_time) {
        let local: DateTime<Local> = dt.into();
        local.format("%H:%M").to_string()
    } else {
        "-".to_string()
    }
}

/// Compute a train's delay in whole minutes (negative if early), returning
/// `None` when there is no expected time, the timestamps don't parse, or the
/// train is exactly on time.
pub fn calculate_delay(item: &TrainItem) -> Option<i64> {
    let expected = item.expected.as_ref()?;
    let scheduled = DateTime::parse_from_rfc3339(&item.scheduled).ok()?;
    let exp = DateTime::parse_from_rfc3339(expected).ok()?;
    let delay = (exp - scheduled).num_minutes();
    (delay != 0).then_some(delay)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(scheduled: &str, expected: Option<&str>) -> TrainItem {
        TrainItem {
            scheduled: scheduled.to_string(),
            expected: expected.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn delay_positive() {
        let i = item(
            "2024-01-01T10:00:00+01:00",
            Some("2024-01-01T10:07:00+01:00"),
        );
        assert_eq!(calculate_delay(&i), Some(7));
    }

    #[test]
    fn delay_negative() {
        let i = item(
            "2024-01-01T10:05:00+01:00",
            Some("2024-01-01T10:03:00+01:00"),
        );
        assert_eq!(calculate_delay(&i), Some(-2));
    }

    #[test]
    fn delay_none_when_on_time() {
        let i = item(
            "2024-01-01T10:00:00+01:00",
            Some("2024-01-01T10:00:00+01:00"),
        );
        assert_eq!(calculate_delay(&i), None);
    }

    #[test]
    fn delay_none_without_expected() {
        let i = item("2024-01-01T10:00:00+01:00", None);
        assert_eq!(calculate_delay(&i), None);
    }

    #[test]
    fn format_time_invalid_is_dash() {
        assert_eq!(format_time("not-a-time"), "-");
    }

    #[test]
    fn format_time_valid_is_hh_mm() {
        let formatted = format_time("2024-01-01T10:05:00+00:00");
        assert_eq!(formatted.len(), 5);
        assert_eq!(formatted.as_bytes()[2], b':');
    }

    #[test]
    fn station_filter_matches_query() {
        let mut app = App::new();
        app.station_search = "wien".to_string();
        app.update_filtered_stations();
        assert!(!app.filtered_stations.is_empty());
        assert!(app
            .filtered_stations
            .iter()
            .all(|(_, name)| name.to_lowercase().contains("wien")));
    }

    #[test]
    fn station_filter_empty_shows_some() {
        let mut app = App::new();
        app.station_search.clear();
        app.update_filtered_stations();
        assert!(!app.filtered_stations.is_empty());
        assert_eq!(app.total_filtered_count, app.all_stations_sorted.len());
    }
}
