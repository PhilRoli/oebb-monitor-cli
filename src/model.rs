//! Deserialisation types mirroring the ÖBB WebSocket JSON payloads.
//!
//! Only the fields the UI consumes are modelled; everything else in the feed is
//! ignored. Most fields are optional because the upstream data is inconsistent
//! between trains, stations, and departures vs. arrivals.

use serde::Deserialize;

/// A localisable display string. The feed nests human-readable text under a
/// `default` key (alongside other locales we don't use).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Destination {
    pub default: String,
}

/// A single departure or arrival in the board.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TrainItem {
    pub id: String,
    pub train: String,
    pub line: Option<String>,
    pub product: Option<String>,
    pub scheduled: String,
    pub expected: Option<String>,
    pub destination: Option<Destination>,
    pub origin: Option<Destination>,
    pub track: Option<String>,
    pub sector: Option<String>,
    pub remarks: Option<Vec<Remark>>,
    pub via: Option<Destination>,
    #[serde(rename = "prioritizedVias")]
    pub prioritized_vias: Option<Vec<String>>,
    pub operator: Option<String>,
    pub formation: Option<Vec<Formation>>,
}

/// One wagon (or the locomotive) in a train's physical composition, including
/// its position sector, onboard amenity icons, and car type (which matters for
/// night trains: sleeper vs. couchette vs. seated).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Formation {
    /// `None` marks the locomotive; otherwise the printed wagon number.
    #[serde(rename = "wagonNumber")]
    pub wagon_number: Option<String>,
    pub icons: Option<Vec<String>>,
    pub sector: Option<String>,
    pub destination: Option<String>,
    /// Car category, e.g. `engine`, `sleeper`, `couchette`, `passenger`,
    /// `car` (car-carrier), `restaurant`.
    #[serde(rename = "type")]
    pub car_type: Option<Vec<String>>,
    /// Whether the wagon is closed / not boardable.
    pub closed: Option<bool>,
    /// Layout code encoding the passenger class, e.g. `W_1`, `W_2`, `W_1_B`
    /// (1st + Business), `W_C_1` (Comfort + 1st), `TW_B_1`. Decoded by the UI.
    pub symbol: Option<String>,
}

/// A free-text remark attached to a train (delays, cancellations, etc.).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Remark {
    pub text: Destination,
}

/// Station-wide notices share the exact shape of a [`Remark`].
pub type SpecialNotice = Remark;

/// The payload body of an `update` message: the current board plus notices.
#[derive(Debug, Clone, Deserialize)]
pub struct TrainData {
    pub departures: Option<Vec<TrainItem>>,
    pub arrivals: Option<Vec<TrainItem>>,
    #[serde(rename = "specialNotices")]
    pub special_notices: Option<Vec<SpecialNotice>>,
}

/// Wrapper around the data carried by an `update` message.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateParams {
    pub data: TrainData,
}

/// A top-level WebSocket message. Only `method == "update"` carries board data.
#[derive(Debug, Clone, Deserialize)]
pub struct WsMessage {
    pub method: Option<String>,
    pub params: Option<UpdateParams>,
}
