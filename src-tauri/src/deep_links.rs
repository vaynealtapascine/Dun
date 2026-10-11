//! Untrusted links only navigate or prepare drafts. They never mutate storage.
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use dun_core::{actions::ItemDraft, model::Schedule, time::Timestamp};
use serde::Serialize;
use tauri::{Emitter, Manager, Runtime};
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Link {
    Open,
    Item { id: String },
    New { draft: Box<ItemDraft> },
    Error { message: String },
}

/// Bounded queue: cold launches and links received during editing wait for the UI.
#[derive(Default)]
pub struct Pending(Mutex<VecDeque<Link>>);

impl Pending {
    fn push(&self, link: Link) {
        let mut queue = self.0.lock().unwrap();
        if queue.len() < 16 {
            queue.push_back(link);
        }
    }
}

#[tauri::command]
pub fn take_deep_link(pending: tauri::State<'_, Pending>) -> Option<Link> {
    pending.0.lock().unwrap().pop_front()
}

pub fn setup<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        for url in event.urls() {
            receive(&handle, url.as_str());
        }
    });
    if let Some(urls) = app.deep_link().get_current()? {
        for url in urls {
            receive(app, url.as_str());
        }
    }
    Ok(())
}

fn receive<R: Runtime>(app: &tauri::AppHandle<R>, raw: &str) {
    let link = parse(raw).unwrap_or_else(|message| Link::Error { message });
    app.state::<Pending>().push(link);
    #[cfg(desktop)]
    crate::desktop::window::show_main(app);
    let _ = app.emit_to("main", "deep-link-pending", ());
}

pub fn parse(raw: &str) -> Result<Link, String> {
    let invalid = || "This Dun link is invalid or unsupported.".to_string();
    if raw.len() > 8192 || raw.chars().any(char::is_control) {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    if url.scheme() != "dun"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(invalid());
    }
    // Reject malformed percent escapes and invalid UTF-8 instead of replacing text.
    let query = url.query().unwrap_or("");
    let bytes = query.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(invalid());
        }
    }
    let mut params = BTreeMap::new();
    for (key, value) in url.query_pairs() {
        if value.contains('\u{fffd}')
            || key.contains('\u{fffd}')
            || params
                .insert(key.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(invalid());
        }
    }
    let link = match url.host_str() {
        Some("open") => Link::Open,
        Some("item") => {
            let id = required(&mut params, "id", 512)?;
            Link::Item { id }
        }
        Some(kind @ ("reminder" | "timer")) => {
            let title = required(&mut params, "title", 300)?;
            let notes = params.remove("notes").unwrap_or_default();
            if notes.len() > 4000 {
                return Err("Link notes must be at most 4,000 bytes.".into());
            }
            let schedule = if kind == "reminder" {
                let due = required(&mut params, "dueAt", 64)?
                    .parse::<jiff::Timestamp>()
                    .map_err(|_| "dueAt must include a timezone offset or Z.".to_string())?;
                Schedule::OneOff {
                    due: Timestamp::from_jiff(due),
                }
            } else {
                let seconds = required(&mut params, "durationSeconds", 10)?
                    .parse::<i64>()
                    .ok()
                    .filter(|s| (1..=31_536_000).contains(s))
                    .ok_or_else(|| "durationSeconds must be 1–31536000.".to_string())?;
                Schedule::Timer {
                    duration_ms: seconds * 1000,
                }
            };
            Link::New {
                draft: Box::new(ItemDraft {
                    title,
                    notes,
                    schedule,
                    tag: None,
                    nag: None,
                    chime: None,
                    quiet_exempt: None,
                    start_timer: kind == "timer",
                }),
            }
        }
        _ => return Err(invalid()),
    };
    if !params.is_empty() {
        return Err("This Dun link contains unsupported parameters.".into());
    }
    Ok(link)
}

fn required(
    params: &mut BTreeMap<String, String>,
    key: &str,
    max: usize,
) -> Result<String, String> {
    params
        .remove(key)
        .filter(|s| !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control))
        .ok_or_else(|| format!("{key} is missing or invalid."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_drafts_without_creating_items() {
        let Link::New { draft } = parse("dun://reminder?title=Caf%C3%A9+%26+tea&dueAt=2026-10-12T09%3A00%3A00%2B08%3A00&notes=Line+1%0ALine+2").unwrap() else { panic!() };
        assert_eq!(draft.title, "Café & tea");
        assert_eq!(draft.notes, "Line 1\nLine 2");
        assert_eq!(
            draft.schedule,
            Schedule::OneOff {
                due: Timestamp::from_jiff("2026-10-12T01:00:00Z".parse().unwrap())
            }
        );
        assert!(!draft.start_timer);
        let Link::New { draft } = parse("dun://timer?title=Tea&durationSeconds=60").unwrap() else {
            panic!()
        };
        assert!(draft.start_timer);
        assert_eq!(
            draft.schedule,
            Schedule::Timer {
                duration_ms: 60_000
            }
        );
        assert_eq!(
            parse("dun://item?id=api-123").unwrap(),
            Link::Item {
                id: "api-123".into()
            }
        );
        assert_eq!(parse("dun://open").unwrap(), Link::Open);
    }

    #[test]
    fn rejects_ambiguous_or_unsafe_links() {
        for raw in [
            "https://open",
            "dun://open/path",
            "dun://user@open",
            "dun://open:90",
            "dun://open#fragment",
            "dun://open?token=secret",
            "dun://done?id=1",
            "dun://item?id=1&id=2",
            "dun://item?id=%FF",
            "dun://item?id=%xx",
            "dun://item?id=%00",
            "dun://timer?title=Tea&durationSeconds=0",
            "dun://timer?title=Tea&durationSeconds=1.5",
            "dun://timer?title=Tea&durationSeconds=31536001",
            "dun://reminder?title=Tea&dueAt=2026-10-12T09:00:00",
            "dun://reminder?title=Tea&dueAt=2026-10-12T09:00:00Z&durationSeconds=3",
        ] {
            assert!(parse(raw).is_err(), "{raw}");
        }
        assert!(parse(&format!("dun://item?id={}", "x".repeat(9000))).is_err());
    }

    #[test]
    fn queue_preserves_order_and_is_bounded() {
        let pending = Pending::default();
        for i in 0..20 {
            pending.push(Link::Item { id: i.to_string() });
        }
        let mut queue = pending.0.lock().unwrap();
        assert_eq!(queue.len(), 16);
        assert_eq!(queue.pop_front(), Some(Link::Item { id: "0".into() }));
    }
}
