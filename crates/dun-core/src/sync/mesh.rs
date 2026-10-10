//! What PCs tell each other besides rows: who is attended, and which rings a
//! phone has said it is covering.
//!
//! Neither is stored. Both travel with every sync between PCs and are merged
//! by "latest wins", so they reach PCs that aren't paired directly, by way of
//! the ones in between. Presence carries the moment the PC itself last saw
//! someone at it, never the moment it was passed on, so a PC nobody is at
//! drops out everywhere once [`PRESENCE_FRESH_MS`] has passed instead of
//! being kept alive by PCs echoing it back and forth.

use std::collections::BTreeMap;

use crate::scheduler::{Ack, PRESENCE_FRESH_MS};
use crate::sync::protocol::{DueItem, Presence};
use crate::time::{Timestamp, DAY};

/// Acknowledgements older than this are for rings long finished.
const ACK_KEEP_MS: i64 = DAY;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mesh {
    /// Handoff acknowledgements by item, from a phone or passed on by a PC.
    pub acks: BTreeMap<String, Ack>,
    /// Other PCs, by device id: the last moment someone was at each.
    presence: BTreeMap<String, Timestamp>,
}

impl Mesh {
    /// Takes in acknowledgements. Ones without a time come straight from a
    /// phone and are stamped `now`; for each item the latest occurrence, then
    /// the latest acknowledgement of it, is kept.
    pub fn merge_acks(&mut self, items: &[DueItem], now: Timestamp) {
        for due in items {
            let ack = Ack {
                occurrence: due.occurrence,
                at: due.at.unwrap_or(now),
            };
            let newer = self
                .acks
                .get(&due.item)
                .is_none_or(|old| (ack.occurrence, ack.at) > (old.occurrence, old.at));
            if newer {
                self.acks.insert(due.item.clone(), ack);
            }
        }
        self.acks.retain(|_, a| now.since(a.at) < ACK_KEEP_MS);
    }

    /// The acknowledgements to pass on to another PC.
    pub fn acks_out(&self) -> Vec<DueItem> {
        self.acks
            .iter()
            .map(|(item, ack)| DueItem {
                item: item.clone(),
                occurrence: ack.occurrence,
                at: Some(ack.at),
            })
            .collect()
    }

    /// Takes in what another PC knows. Entries about this PC are ignored: it
    /// knows better than anyone whether someone is at it.
    pub fn merge_presence(&mut self, own_id: &str, list: &[Presence]) {
        for p in list.iter().filter(|p| p.device_id != own_id) {
            let at = self
                .presence
                .entry(p.device_id.clone())
                .or_insert(p.attended_at);
            *at = (*at).max(p.attended_at);
        }
    }

    /// What to tell `recipient`: this PC if someone is at it, and every other
    /// PC still fresh, leaving out the recipient itself.
    pub fn presence_for(
        &self,
        own_id: &str,
        attended: bool,
        now: Timestamp,
        recipient: &str,
    ) -> Vec<Presence> {
        let own = attended.then(|| Presence {
            device_id: own_id.to_string(),
            attended_at: now,
        });
        own.into_iter()
            .chain(
                self.presence
                    .iter()
                    .filter(|(id, at)| *id != recipient && fresh(**at, now))
                    .map(|(id, at)| Presence {
                        device_id: id.clone(),
                        attended_at: *at,
                    }),
            )
            .collect()
    }

    /// The last moment someone was at another PC, if any PC has said so.
    pub fn peer_attended_at(&self) -> Option<Timestamp> {
        self.presence.values().copied().max()
    }

    /// Whether someone is at this PC or, recently, any other.
    pub fn anyone_attended(&self, attended: bool, now: Timestamp) -> bool {
        attended || self.peer_attended_at().is_some_and(|at| fresh(at, now))
    }

    /// Forgets a PC that is no longer paired.
    pub fn forget(&mut self, device_id: &str) {
        self.presence.remove(device_id);
    }
}

fn fresh(at: Timestamp, now: Timestamp) -> bool {
    now.since(at) <= PRESENCE_FRESH_MS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::SECOND;

    const T: Timestamp = Timestamp(1_789_500_000_000);

    fn due(item: &str, occurrence: Timestamp, at: Option<Timestamp>) -> DueItem {
        DueItem {
            item: item.into(),
            occurrence,
            at,
        }
    }

    #[test]
    fn a_phone_ack_is_stamped_on_arrival_and_passed_on_with_that_time() {
        let mut mesh = Mesh::default();
        mesh.merge_acks(&[due("a", T, None)], T.plus(5 * SECOND));
        assert_eq!(mesh.acks_out(), [due("a", T, Some(T.plus(5 * SECOND)))]);
    }

    #[test]
    fn a_relayed_ack_never_replaces_a_newer_one() {
        let mut mesh = Mesh::default();
        mesh.merge_acks(&[due("a", T, Some(T.plus(30 * SECOND)))], T);
        mesh.merge_acks(&[due("a", T, Some(T.plus(10 * SECOND)))], T);
        assert_eq!(mesh.acks["a"].at, T.plus(30 * SECOND));

        let next = T.plus(60 * SECOND);
        mesh.merge_acks(&[due("a", next, Some(next))], next);
        assert_eq!(mesh.acks["a"].occurrence, next);
    }

    #[test]
    fn presence_expires_rather_than_echoing_forever() {
        // A and B are paired; someone was at C, which only B can see.
        let (mut a, mut b) = (Mesh::default(), Mesh::default());
        b.merge_presence(
            "b",
            &[Presence {
                device_id: "c".into(),
                attended_at: T,
            }],
        );

        // B tells A, and A tells B straight back, again and again.
        let mut now = T;
        for _ in 0..10 {
            now = now.plus(10 * SECOND);
            a.merge_presence("a", &b.presence_for("b", false, now, "a"));
            b.merge_presence("b", &a.presence_for("a", false, now, "b"));
        }
        assert!(!a.anyone_attended(false, now), "C left long ago");
        assert!(!b.anyone_attended(false, now));
        assert_eq!(
            a.peer_attended_at(),
            Some(T),
            "the time passed on is C's own"
        );
    }

    #[test]
    fn a_pc_ignores_what_others_say_about_it_and_leaves_the_recipient_out() {
        let mut mesh = Mesh::default();
        mesh.merge_presence(
            "a",
            &[
                Presence {
                    device_id: "a".into(),
                    attended_at: T,
                },
                Presence {
                    device_id: "b".into(),
                    attended_at: T,
                },
            ],
        );
        assert_eq!(mesh.presence.len(), 1);
        let out = mesh.presence_for("a", true, T, "b");
        assert_eq!(
            out,
            [Presence {
                device_id: "a".into(),
                attended_at: T
            }]
        );
    }
}
