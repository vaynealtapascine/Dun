//! Properties of sync merge:
//!
//! 1. Replicas that exchange changes with skewed clocks and partial syncs end
//!    up identical, and identical to an independent oracle of what should win.
//! 2. Delivering the same writes in any order, with duplicates, gives the
//!    oracle's result.
//!
//! The oracle is written separately from `sync::merge` on purpose: plain
//! convergence also holds for wrong rules (e.g. "always take incoming"), so
//! the tests pin down *which* value wins, not just that replicas agree.

use std::collections::BTreeMap;

use dun_core::model::Completion;
use dun_core::store::{NewHistory, NewWrite, Store};
use dun_core::sync::{field, Entity, HistoryKind, Reg};
use dun_core::time::{Timestamp, MINUTE};
use proptest::prelude::*;

const T0: i64 = 1_789_500_000_000;
const PAGE: usize = 3;

#[derive(Debug, Clone)]
enum Op {
    Title {
        replica: usize,
        item: u8,
        text: u8,
    },
    Created {
        replica: usize,
        item: u8,
    },
    Complete {
        replica: usize,
        item: u8,
        through: u8,
        gen: u8,
    },
    Delete {
        replica: usize,
        item: u8,
        deleted: bool,
    },
    Done {
        replica: usize,
        item: u8,
    },
    Advance {
        replica: usize,
        ms: i64,
    },
    Sync {
        from: usize,
        to: usize,
    },
}

fn op() -> impl Strategy<Value = Op> {
    let replica = 0usize..2;
    prop_oneof![
        (replica.clone(), 0u8..3, 0u8..4).prop_map(|(replica, item, text)| Op::Title {
            replica,
            item,
            text
        }),
        (replica.clone(), 0u8..3).prop_map(|(replica, item)| Op::Created { replica, item }),
        (replica.clone(), 0u8..3, 0u8..6, 0u8..3).prop_map(|(replica, item, through, gen)| {
            Op::Complete {
                replica,
                item,
                through,
                gen,
            }
        }),
        (replica.clone(), 0u8..3, any::<bool>()).prop_map(|(replica, item, deleted)| Op::Delete {
            replica,
            item,
            deleted
        }),
        (replica.clone(), 0u8..3).prop_map(|(replica, item)| Op::Done { replica, item }),
        // Clocks drift apart by up to an hour and sometimes step backwards.
        (replica.clone(), -5_000i64..(10 * MINUTE))
            .prop_map(|(replica, ms)| Op::Advance { replica, ms }),
        (0usize..2).prop_map(|from| Op::Sync { from, to: 1 - from }),
    ]
}

struct Replica {
    store: Store,
    now: Timestamp,
}

struct Sim {
    replicas: [Replica; 2],
    /// cursor[from][to]: seq of `from` already delivered to `to`.
    cursor: [[i64; 2]; 2],
    /// Every register any replica ever wrote locally.
    produced: Vec<Reg>,
}

impl Sim {
    fn new() -> Self {
        Sim {
            replicas: [
                Replica {
                    store: Store::open_in_memory_as("pc").unwrap(),
                    now: Timestamp(T0),
                },
                Replica {
                    store: Store::open_in_memory_as("phone").unwrap(),
                    now: Timestamp(T0 + 30 * MINUTE),
                },
            ],
            cursor: [[0; 2]; 2],
            produced: Vec::new(),
        }
    }

    fn write(&mut self, replica: usize, w: NewWrite) {
        let r = &mut self.replicas[replica];
        let applied = r.store.write(r.now, vec![w]).unwrap();
        self.produced.extend(applied);
    }

    fn apply(&mut self, op: &Op) {
        match *op {
            Op::Title {
                replica,
                item,
                text,
            } => self.write(
                replica,
                NewWrite::new(
                    Entity::Item,
                    format!("i{item}"),
                    field::TITLE,
                    format!("t{text}"),
                ),
            ),
            Op::Created { replica, item } => {
                let now = self.replicas[replica].now;
                self.write(
                    replica,
                    NewWrite::new(
                        Entity::Item,
                        format!("i{item}"),
                        field::CREATED,
                        serde_json::json!({ "at": now, "by": replica }),
                    ),
                )
            }
            Op::Complete {
                replica,
                item,
                through,
                gen,
            } => self.write(
                replica,
                NewWrite::new(
                    Entity::Item,
                    format!("i{item}"),
                    field::COMPLETION,
                    Completion {
                        through: Some(Timestamp(T0 + i64::from(through) * MINUTE)),
                        done_at: Some(self.replicas[replica].now),
                        gen: u32::from(gen),
                    },
                ),
            ),
            Op::Delete {
                replica,
                item,
                deleted,
            } => self.write(
                replica,
                NewWrite::new(Entity::Item, format!("i{item}"), field::DELETED, deleted),
            ),
            Op::Done { replica, item } => {
                let r = &mut self.replicas[replica];
                r.store
                    .append_history(
                        r.now,
                        NewHistory {
                            item_id: format!("i{item}"),
                            kind: HistoryKind::Done,
                            occurrence: Some(r.now),
                            snooze_count: 0,
                            title: String::new(),
                            ref_id: None,
                            prev_completion: None,
                        },
                    )
                    .unwrap();
            }
            Op::Advance { replica, ms } => {
                let r = &mut self.replicas[replica];
                r.now = r.now.plus(ms);
            }
            Op::Sync { from, to } => self.sync(from, to, 1),
        }
    }

    /// Delivers up to `pages` pages from one replica to the other.
    fn sync(&mut self, from: usize, to: usize, pages: usize) {
        for _ in 0..pages {
            let changes = self.replicas[from]
                .store
                .changes_since(self.cursor[from][to], PAGE)
                .unwrap();
            let now = self.replicas[to].now;
            self.replicas[to]
                .store
                .merge(now, &changes.regs, &changes.history)
                .unwrap();
            self.cursor[from][to] = changes.max_seq;
            if !changes.more {
                break;
            }
        }
    }

    fn settle(&mut self) {
        // Two rounds each way: the second delivers rows the first merge re-issued.
        for _ in 0..2 {
            self.sync(0, 1, usize::MAX);
            self.sync(1, 0, usize::MAX);
        }
    }
}

type Key = (Entity, String, String);

/// Independent statement of the merge rules.
fn oracle(regs: &[Reg]) -> Vec<Reg> {
    fn beats(a: &Reg, b: &Reg) -> bool {
        let stamp = |r: &Reg| (r.hlc, r.device.clone());
        match (a.entity, a.field.as_str()) {
            (Entity::Item, "created") => stamp(a) < stamp(b),
            (Entity::Item, "completion") => {
                let key = |r: &Reg| {
                    let c: Completion = serde_json::from_value(r.value.clone()).unwrap();
                    (c.gen, c.through, r.hlc, r.device.clone())
                };
                key(a) > key(b)
            }
            _ => stamp(a) > stamp(b),
        }
    }
    let mut best: BTreeMap<Key, Reg> = BTreeMap::new();
    for r in regs {
        let key = (r.entity, r.id.clone(), r.field.clone());
        match best.get(&key) {
            Some(cur) if !beats(r, cur) => {}
            _ => {
                best.insert(key, r.clone());
            }
        }
    }
    best.into_values().collect()
}

fn history_ids(store: &Store) -> Vec<String> {
    let mut ids: Vec<String> = store
        .history(None, None, 10_000)
        .unwrap()
        .into_iter()
        .map(|h| h.id)
        .collect();
    ids.sort();
    ids
}

fn shuffle(regs: &mut [Reg], seed: u64) {
    let mut state = seed | 1;
    for i in (1..regs.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        regs.swap(i, (state % (i as u64 + 1)) as usize);
    }
}

fn merged_one_by_one(regs: &[Reg]) -> Vec<Reg> {
    let now = Timestamp(T0 + 24 * 60 * MINUTE);
    let mut s = Store::open_in_memory_as("observer").unwrap();
    for r in regs {
        s.merge(now, std::slice::from_ref(r), &[]).unwrap();
    }
    s.registers().unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn replicas_converge_on_the_oracle(ops in prop::collection::vec(op(), 1..60)) {
        let mut sim = Sim::new();
        for op in &ops {
            sim.apply(op);
        }
        sim.settle();

        let a = sim.replicas[0].store.registers().unwrap();
        let b = sim.replicas[1].store.registers().unwrap();
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(&a, &oracle(&sim.produced));
        prop_assert_eq!(history_ids(&sim.replicas[0].store), history_ids(&sim.replicas[1].store));

        // Once settled, another sync is a no-op in both directions.
        let (seq_a, seq_b) = (sim.replicas[0].store.seq(), sim.replicas[1].store.seq());
        sim.settle();
        prop_assert_eq!(seq_a, sim.replicas[0].store.seq());
        prop_assert_eq!(seq_b, sim.replicas[1].store.seq());
    }

    #[test]
    fn any_delivery_order_gives_the_oracle(
        ops in prop::collection::vec(op(), 1..40),
        seed_a in any::<u64>(),
        seed_b in any::<u64>(),
    ) {
        let mut sim = Sim::new();
        for op in ops.iter().filter(|o| !matches!(o, Op::Sync { .. })) {
            sim.apply(op);
        }
        let expected = oracle(&sim.produced);

        let forward = sim.produced.clone();
        let mut reversed = sim.produced.clone();
        reversed.reverse();
        let mut shuffled_a = sim.produced.clone();
        shuffle(&mut shuffled_a, seed_a);
        let mut shuffled_b = sim.produced.clone();
        shuffle(&mut shuffled_b, seed_b);
        let with_duplicates: Vec<Reg> = shuffled_a.iter().chain(shuffled_b.iter()).cloned().collect();

        prop_assert_eq!(merged_one_by_one(&forward), expected.clone());
        prop_assert_eq!(merged_one_by_one(&reversed), expected.clone());
        prop_assert_eq!(merged_one_by_one(&shuffled_a), expected.clone());
        prop_assert_eq!(merged_one_by_one(&with_duplicates), expected);
    }
}
