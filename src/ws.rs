use anyhow::Result;
use chrono::Local;
use futures_util::StreamExt;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, Mutex, Notify};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::app::{build_ws_url, App, ConnectionState, ContentType};
use crate::model::{TrainItem, UpdateParams, WsMessage};

/// Events emitted by a single per-page WebSocket worker task back to the
/// coordinating loop in [`run_websocket`].
enum PageEvent {
    /// The page's socket connected successfully.
    Connected,
    /// The page failed to connect at all.
    Failed,
    /// The page delivered a fresh `update` payload.
    Update(usize, UpdateParams),
}

/// Drives the live data connection for the lifetime of the program.
///
/// Each iteration opens `max_pages` parallel WebSocket connections for the
/// currently selected station and content type, merges their incremental
/// updates into [`App::items`], and keeps reconnecting. A reconnect is
/// triggered either by a signal on `reconnect_rx` (station change, A/D switch,
/// manual refresh) or by the sockets closing on their own.
///
/// `notify` is pulsed whenever the rendered state changes so the UI loop can
/// redraw without busy-polling.
pub async fn run_websocket(
    app: Arc<Mutex<App>>,
    mut reconnect_rx: mpsc::Receiver<()>,
    notify: Arc<Notify>,
) -> Result<()> {
    debug!("WebSocket handler started");
    let mut active_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let mut prev: Option<(String, ContentType)> = None;
    // Exponential backoff between self-initiated reconnects; reset on success.
    let mut backoff = Duration::from_secs(1);
    let mut iteration = 0;

    loop {
        iteration += 1;
        debug!("=== WebSocket iteration {} ===", iteration);

        for task in active_tasks.drain(..) {
            task.abort();
        }

        let (max_pages, station_id, content_type) = {
            let app_guard = app.lock().await;
            (
                app_guard.max_pages,
                app_guard.station_id.clone(),
                app_guard.content_type.clone(),
            )
        };

        // Only wipe the visible board when the station or direction actually
        // changed. For a plain refresh/reconnect we keep the stale rows on
        // screen until fresh data lands, avoiding a jarring blank flash.
        let target = (station_id.clone(), content_type.clone());
        let params_changed = prev.as_ref() != Some(&target);
        {
            let mut app_guard = app.lock().await;
            if params_changed {
                debug!(
                    "Station/content changed, clearing {} items",
                    app_guard.items.len()
                );
                app_guard.items.clear();
                app_guard.selected_train_index = None;
                app_guard.selected_train_id = None;
            }
            app_guard.connection = ConnectionState::Connecting;
            app_guard.last_update = None;
        }
        prev = Some(target);
        notify.notify_one();

        debug!(
            "Station: {}, ContentType: {:?}, Pages: {}",
            station_id, content_type, max_pages
        );

        let (page_tx, mut page_rx) = mpsc::channel(100);

        for page in 1..=max_pages {
            let url = build_ws_url(&station_id, &content_type, page);
            debug!("Spawning task for page {}: {}", page, url);
            let tx = page_tx.clone();

            let task = tokio::spawn(async move {
                debug!("Page {} task started, connecting...", page);
                match connect_async(&url).await {
                    Ok((ws_stream, _)) => {
                        debug!("Page {} connected successfully", page);
                        let _ = tx.send(PageEvent::Connected).await;
                        let (_, mut read) = ws_stream.split();
                        let mut msg_count = 0;

                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(Message::Text(text)) => {
                                    msg_count += 1;
                                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                                        if ws_msg.method.as_deref() == Some("update") {
                                            if let Some(params) = ws_msg.params {
                                                let _ =
                                                    tx.send(PageEvent::Update(page, params)).await;
                                            }
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
                        let _ = tx.send(PageEvent::Failed).await;
                    }
                }
            });

            active_tasks.push(task);
        }

        drop(page_tx);
        debug!("Spawned {} tasks, now listening for updates", max_pages);

        // Items are accumulated fresh each iteration so trains that have since
        // departed drop off, then published to the shared state on every batch.
        let mut collected: Vec<TrainItem> = Vec::new();
        let mut update_count = 0;
        let mut failed_count = 0;
        let mut connected_any = false;

        loop {
            tokio::select! {
                event = page_rx.recv() => {
                    match event {
                        Some(PageEvent::Connected) => {
                            connected_any = true;
                            app.lock().await.connection = ConnectionState::Connected;
                            notify.notify_one();
                        }
                        Some(PageEvent::Failed) => {
                            failed_count += 1;
                            if failed_count >= max_pages && !connected_any {
                                debug!("All {} pages failed to connect", max_pages);
                                app.lock().await.connection =
                                    ConnectionState::Failed("Verbindung fehlgeschlagen".to_string());
                                notify.notify_one();
                            }
                        }
                        Some(PageEvent::Update(page, params)) => {
                            update_count += 1;
                            debug!("Received update #{} from page {}", update_count, page);

                            let mut app = app.lock().await;

                            let new_items = match app.content_type {
                                ContentType::Departure => params.data.departures.unwrap_or_default(),
                                ContentType::Arrival => params.data.arrivals.unwrap_or_default(),
                            };

                            let before = collected.len();
                            for item in new_items {
                                if !collected.iter().any(|i| i.id == item.id) {
                                    collected.push(item);
                                }
                            }
                            collected.sort_by(|a, b| a.scheduled.cmp(&b.scheduled));
                            debug!("Merged items: {} -> {}", before, collected.len());

                            app.items = collected.clone();

                            // Re-sync the selected index from its id after the
                            // sort, so the detail view keeps tracking the right
                            // train as rows shift around.
                            if let Some(id) = app.selected_train_id.clone() {
                                app.selected_train_index =
                                    app.items.iter().position(|i| i.id == id);
                            }

                            if let Some(notices) = params.data.special_notices {
                                app.special_notices = notices;
                            }
                            app.last_update = Some(Local::now());
                            app.connection = ConnectionState::Connected;
                            notify.notify_one();
                        }
                        None => {
                            // Every page socket closed; back off and reconnect.
                            if update_count > 0 {
                                backoff = Duration::from_secs(1);
                            }
                            debug!(
                                "All page channels closed after {} updates, reconnecting in {:?}",
                                update_count, backoff
                            );
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(30));
                            break;
                        }
                    }
                }
                _ = reconnect_rx.recv() => {
                    debug!("!!! RECONNECT SIGNAL RECEIVED after {} updates !!!", update_count);
                    backoff = Duration::from_secs(1);
                    break;
                }
            }
        }
    }
}
