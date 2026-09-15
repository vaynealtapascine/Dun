//! What the scheduler asks the platform shell to do.

use serde::Serialize;

use crate::model::{ChimeRef, Item};
use crate::time::{TimeZone, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertLevel {
    /// Pop the notification (fresh banner) and chime.
    Full,
    /// Put it on screen or in the notification list quietly, no chime.
    Silent,
}

/// Facts for the alert's second line; the shell formats them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertLabel {
    pub due: Timestamp,
    /// Earliest missed slot, when this ring covers time the device was away.
    pub missed_first: Option<Timestamp>,
    pub missed_count: u32,
    /// Held back by quiet hours before this alert.
    pub quiet_held: bool,
    pub snooze_count: u32,
}

impl AlertLabel {
    /// "Due 9:00 AM", "Missed Tue 9:00 PM", "Missed 3 times since Mon 9:00 AM",
    /// with " · snoozed 2×" appended when relevant.
    pub fn describe(&self, now: Timestamp, tz: &TimeZone) -> String {
        let mut s = match (self.missed_first, self.missed_count) {
            (Some(first), n) if n > 1 => format!("Missed {n} times since {}", when(first, now, tz)),
            (Some(_), _) => format!("Missed {}", when(self.due, now, tz)),
            (None, _) => format!("Due {}", when(self.due, now, tz)),
        };
        if self.quiet_held {
            s.push_str(" · held for quiet hours");
        }
        if self.snooze_count > 0 {
            s.push_str(&format!(" · snoozed {}×", self.snooze_count));
        }
        s
    }
}

/// Clock time, prefixed with the weekday (within a week) or date when it isn't today.
pub fn when(t: Timestamp, now: Timestamp, tz: &TimeZone) -> String {
    let z = t.to_zoned(tz);
    let today = now.to_zoned(tz).date();
    let hour = z.hour();
    let (h12, ampm) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    let clock = format!("{h12}:{:02} {ampm}", z.minute());
    let days = z.date().until(today).map(|s| s.get_days()).unwrap_or(0);
    match days {
        0 => clock,
        1..=6 => format!("{} {clock}", z.strftime("%a")),
        _ => format!("{} {clock}", z.strftime("%b %-d")),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Effect {
    #[serde(rename_all = "camelCase")]
    ShowAlert {
        item_id: String,
        occurrence: Timestamp,
        level: AlertLevel,
        title: String,
        notes: String,
        label: AlertLabel,
    },
    #[serde(rename_all = "camelCase")]
    ClearAlert { item_id: String },
    #[serde(rename_all = "camelCase")]
    PlayChime { chime: Option<ChimeRef> },
    #[serde(rename_all = "camelCase")]
    ScheduleWake { at: Timestamp },
    #[serde(rename_all = "camelCase")]
    TrayStatus {
        ringing: usize,
        next: Option<(String, Timestamp)>,
    },
}

impl Effect {
    pub(super) fn alert(
        item: &Item,
        occurrence: Timestamp,
        level: AlertLevel,
        label: AlertLabel,
    ) -> Self {
        Effect::ShowAlert {
            item_id: item.id.clone(),
            occurrence,
            level,
            title: item.title.clone(),
            notes: item.notes.clone(),
            label,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{resolve, DAY};
    use jiff::civil::date;

    #[test]
    fn labels_read_naturally() {
        let tz = TimeZone::UTC;
        let now = resolve(date(2026, 9, 16).at(10, 0, 0, 0), &tz); // Wednesday
        let nine = resolve(date(2026, 9, 16).at(9, 0, 0, 0), &tz);
        let mon = resolve(date(2026, 9, 14).at(21, 5, 0, 0), &tz);
        let base = AlertLabel {
            due: nine,
            missed_first: None,
            missed_count: 1,
            quiet_held: false,
            snooze_count: 0,
        };
        assert_eq!(base.describe(now, &tz), "Due 9:00 AM");
        assert_eq!(
            AlertLabel {
                missed_first: Some(nine),
                ..base
            }
            .describe(now, &tz),
            "Missed 9:00 AM"
        );
        assert_eq!(
            AlertLabel {
                missed_first: Some(mon),
                missed_count: 3,
                snooze_count: 2,
                ..base
            }
            .describe(now, &tz),
            "Missed 3 times since Mon 9:05 PM · snoozed 2×"
        );
        assert_eq!(when(nine.minus(10 * DAY), now, &tz), "Sep 6 9:00 AM");
        assert_eq!(
            when(resolve(date(2026, 9, 16).at(0, 30, 0, 0), &tz), now, &tz),
            "12:30 AM"
        );
        assert_eq!(
            when(resolve(date(2026, 9, 16).at(12, 0, 0, 0), &tz), now, &tz),
            "12:00 PM"
        );
    }
}
