//! How two versions of the same register combine.
//!
//! Every rule is a total order on `(value, stamp)`, so replicas that see the
//! same writes in any order, any number of times, end up identical.
//!
//! | Register            | Winner                                               |
//! |---------------------|------------------------------------------------------|
//! | `item.created`      | the first write (lowest stamp)                       |
//! | `item.completion`   | highest `(gen, through)`, then highest stamp         |
//! | everything else     | highest stamp (last writer wins)                     |

use std::cmp::Ordering;

use crate::model::Completion;

use super::rows::{field, Entity, Reg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    KeepLocal,
    TakeIncoming,
}

/// Decides between the stored register and an incoming one with the same key.
pub fn decide(local: &Reg, incoming: &Reg) -> Decision {
    debug_assert_eq!(local.key(), incoming.key());
    match order(local, incoming) {
        Ordering::Less => Decision::TakeIncoming,
        Ordering::Equal | Ordering::Greater => Decision::KeepLocal,
    }
}

/// `Greater` means `a` should win.
fn order(a: &Reg, b: &Reg) -> Ordering {
    match (a.entity, a.field.as_str()) {
        (Entity::Item, field::CREATED) => b.stamp().cmp(&a.stamp()),
        (Entity::Item, field::COMPLETION) => completion_key(a)
            .cmp(&completion_key(b))
            .then_with(|| a.stamp().cmp(&b.stamp())),
        _ => a.stamp().cmp(&b.stamp()),
    }
}

/// `(gen, through)`; an unreadable value sorts lowest so a corrupt write from
/// a peer can't clobber a real completion.
fn completion_key(r: &Reg) -> (bool, u32, Option<i64>) {
    match serde_json::from_value::<Completion>(r.value.clone()) {
        Ok(c) => (true, c.gen, c.through.map(|t| t.0)),
        Err(_) => (false, 0, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::Hlc;
    use crate::time::Timestamp;
    use serde_json::json;

    fn reg(field: &str, value: serde_json::Value, ms: i64, device: &str) -> Reg {
        Reg {
            entity: Entity::Item,
            id: "i1".into(),
            field: field.into(),
            value,
            hlc: Hlc::new(ms, 0),
            device: device.into(),
        }
    }

    fn completion(through: i64, gen: u32) -> serde_json::Value {
        serde_json::to_value(Completion {
            through: Some(Timestamp(through)),
            done_at: Some(Timestamp(through + 5)),
            gen,
        })
        .unwrap()
    }

    #[test]
    fn last_writer_wins_with_device_tie_break() {
        let a = reg(field::TITLE, json!("old"), 10, "pc");
        let b = reg(field::TITLE, json!("new"), 20, "phone");
        assert_eq!(decide(&a, &b), Decision::TakeIncoming);
        assert_eq!(decide(&b, &a), Decision::KeepLocal);

        let tie_pc = reg(field::TITLE, json!("x"), 10, "pc");
        let tie_phone = reg(field::TITLE, json!("y"), 10, "phone");
        // "phone" > "pc" lexicographically, so phone wins wherever it's evaluated.
        assert_eq!(decide(&tie_pc, &tie_phone), Decision::TakeIncoming);
        assert_eq!(decide(&tie_phone, &tie_pc), Decision::KeepLocal);
    }

    #[test]
    fn identical_register_is_a_no_op() {
        let a = reg(field::TITLE, json!("same"), 10, "pc");
        assert_eq!(decide(&a, &a.clone()), Decision::KeepLocal);
    }

    #[test]
    fn created_keeps_the_first_writer() {
        let first = reg(field::CREATED, json!({"at": 1}), 10, "phone");
        let later = reg(field::CREATED, json!({"at": 2}), 20, "pc");
        assert_eq!(decide(&first, &later), Decision::KeepLocal);
        assert_eq!(decide(&later, &first), Decision::TakeIncoming);
    }

    #[test]
    fn completion_is_a_max_register_on_gen_then_through() {
        let done_early_stamp_late = reg(field::COMPLETION, completion(1_000, 0), 99, "phone");
        let done_later_occ_early_stamp = reg(field::COMPLETION, completion(2_000, 0), 10, "pc");
        // Later occurrence wins even though its write is older.
        assert_eq!(
            decide(&done_early_stamp_late, &done_later_occ_early_stamp),
            Decision::TakeIncoming
        );

        // Undo (gen+1) beats a stale Done of a later occurrence.
        let undo = reg(field::COMPLETION, completion(500, 1), 5, "pc");
        assert_eq!(
            decide(&done_later_occ_early_stamp, &undo),
            Decision::TakeIncoming
        );
        assert_eq!(
            decide(&undo, &done_later_occ_early_stamp),
            Decision::KeepLocal
        );
    }

    #[test]
    fn corrupt_completion_never_wins() {
        let real = reg(field::COMPLETION, completion(1_000, 0), 10, "pc");
        let junk = reg(field::COMPLETION, json!("garbage"), 99, "phone");
        assert_eq!(decide(&real, &junk), Decision::KeepLocal);
        assert_eq!(decide(&junk, &real), Decision::TakeIncoming);
    }
}
