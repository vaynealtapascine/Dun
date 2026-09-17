//! The contract between Rust and Android's PlanExecutor.
//!
//! Kotlin makes no scheduling decisions: it posts and cancels exactly what a
//! [`Plan`] says and keeps one alarm for `next_wake_at`. Field names are
//! camelCase on the wire because Kotlin reads them with `org.json`.

use serde::{Deserialize, Serialize};

/// Notification buttons Android knows how to render (max three).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Button {
    Done,
    Snooze5,
    Snooze15,
}

impl Button {
    pub const RING: [Button; 3] = [Button::Done, Button::Snooze5, Button::Snooze15];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub notif_id: i32,
    pub item_id: String,
    /// Occurrence (due instant, ms) the notification belongs to; echoed back by
    /// buttons so a stale notification can't act on a newer occurrence.
    pub occ: i64,
    pub channel: String,
    pub silent: bool,
    pub title: String,
    pub text: String,
    pub notes: Option<String>,
    pub when: Option<i64>,
    pub actions: Vec<Button>,
    /// Stays on screen and can't be swiped away: a timer still counting down.
    #[serde(default)]
    pub ongoing: bool,
    /// Instant to count down to. Android ticks it on its own, so a running
    /// timer needs no alarm to keep its notification honest.
    #[serde(default)]
    pub countdown_to: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub post: Vec<Post>,
    pub cancel: Vec<i32>,
    pub next_wake_at: Option<i64>,
    pub pending_push: bool,
    /// Free text for logcat; not shown to the user.
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub log: String,
}

/// What Kotlin sends in. Unknown fields are ignored so Kotlin can add
/// diagnostics without a Rust change.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Alarm {
        #[serde(default)]
        late_by_ms: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    Action {
        item_id: String,
        occ: i64,
        button: Button,
    },
    #[serde(rename_all = "camelCase")]
    Dismissed {
        item_id: String,
        occ: i64,
    },
    Reschedule {
        reason: String,
    },
    AppStarted,
    SyncOnly,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub handler_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_parse_from_kotlin_json() {
        let e: Event = serde_json::from_str(r#"{"type":"alarm","lateByMs":1200}"#).unwrap();
        assert_eq!(
            e,
            Event::Alarm {
                late_by_ms: Some(1200)
            }
        );

        let e: Event = serde_json::from_str(
            r#"{"type":"action","itemId":"spike","occ":42,"button":"snooze15"}"#,
        )
        .unwrap();
        assert_eq!(
            e,
            Event::Action {
                item_id: "spike".into(),
                occ: 42,
                button: Button::Snooze15
            }
        );

        let e: Event = serde_json::from_str(r#"{"type":"reschedule","reason":"boot"}"#).unwrap();
        assert_eq!(
            e,
            Event::Reschedule {
                reason: "boot".into()
            }
        );

        let e: Event = serde_json::from_str(r#"{"type":"appStarted"}"#).unwrap();
        assert_eq!(e, Event::AppStarted);
    }

    #[test]
    fn plan_serializes_with_kotlin_field_names() {
        let plan = Plan {
            post: vec![Post {
                notif_id: 1,
                item_id: "a".into(),
                occ: 5,
                channel: "ring_default".into(),
                silent: false,
                title: "T".into(),
                text: "x".into(),
                notes: None,
                when: Some(5),
                actions: Button::RING.to_vec(),
                ongoing: true,
                countdown_to: Some(9),
            }],
            cancel: vec![2],
            next_wake_at: None,
            pending_push: false,
            log: String::new(),
        };
        let v = serde_json::to_value(&plan).unwrap();
        assert_eq!(v["post"][0]["notifId"], 1);
        assert_eq!(v["post"][0]["ongoing"], true);
        assert_eq!(v["post"][0]["countdownTo"], 9);
        assert_eq!(
            v["post"][0]["actions"],
            serde_json::json!(["done", "snooze5", "snooze15"])
        );
        assert!(
            v["nextWakeAt"].is_null(),
            "Kotlin checks isNull(\"nextWakeAt\")"
        );
        assert_eq!(v["pendingPush"], false);
        assert!(v.get("log").is_none());
    }
}
