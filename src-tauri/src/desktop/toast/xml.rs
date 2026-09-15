//! Toast XML and the argument strings carried by toast buttons.
//!
//! Pure string code so it is unit-tested without WinRT. Arguments come back to
//! the COM activator verbatim, possibly long after the toast was shown (Action
//! Center), so every button names the item *and* the occurrence it belongs to.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub label: String,
    pub action: ToastAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Replacement key: showing a toast with the same tag and group replaces it.
    pub tag: String,
    pub group: String,
    pub title: String,
    /// Up to two further lines (Windows shows three text elements at most).
    pub lines: Vec<String>,
    /// What clicking the toast body does.
    pub launch: ToastAction,
    pub buttons: Vec<Button>,
    /// `scenario="reminder"`: stays on screen until acted on. Requires a button.
    pub reminder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastAction {
    Open {
        item: Option<String>,
    },
    Done {
        item: String,
        occ: i64,
    },
    Snooze {
        item: String,
        occ: i64,
        minutes: u32,
    },
    SnoozeAll {
        minutes: u32,
    },
}

impl ToastAction {
    pub fn to_args(&self) -> String {
        let mut pairs: Vec<(&str, String)> = Vec::new();
        match self {
            ToastAction::Open { item } => {
                pairs.push(("a", "open".into()));
                if let Some(item) = item {
                    pairs.push(("i", item.clone()));
                }
            }
            ToastAction::Done { item, occ } => {
                pairs.push(("a", "done".into()));
                pairs.push(("i", item.clone()));
                pairs.push(("o", occ.to_string()));
            }
            ToastAction::Snooze { item, occ, minutes } => {
                pairs.push(("a", "snz".into()));
                pairs.push(("i", item.clone()));
                pairs.push(("o", occ.to_string()));
                pairs.push(("m", minutes.to_string()));
            }
            ToastAction::SnoozeAll { minutes } => {
                pairs.push(("a", "snzall".into()));
                pairs.push(("m", minutes.to_string()));
            }
        }
        pairs
            .into_iter()
            .map(|(k, v)| format!("{k}={}", encode(&v)))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Parses arguments from the activator. Unknown or malformed input is an
    /// error rather than a guess: a toast must never act on the wrong item.
    pub fn parse(args: &str) -> Result<Self, String> {
        let mut map = BTreeMap::new();
        for part in args.split(';').filter(|p| !p.is_empty()) {
            let (k, v) = part
                .split_once('=')
                .ok_or_else(|| format!("malformed toast argument '{part}'"))?;
            map.insert(k, decode(v)?);
        }
        let get = |k: &str| {
            map.get(k)
                .cloned()
                .ok_or_else(|| format!("toast arguments '{args}' missing '{k}'"))
        };
        let num = |k: &str| -> Result<i64, String> {
            get(k)?
                .parse::<i64>()
                .map_err(|e| format!("toast argument '{k}': {e}"))
        };
        let minutes = || -> Result<u32, String> {
            let m = num("m")?;
            u32::try_from(m)
                .ok()
                .filter(|m| (1..=24 * 60).contains(m))
                .ok_or_else(|| format!("toast snooze minutes out of range: {m}"))
        };

        match map.get("a").map(String::as_str) {
            // An empty argument string is what Windows sends for a body click
            // on a toast without a launch attribute.
            None if map.is_empty() => Ok(ToastAction::Open { item: None }),
            Some("open") => Ok(ToastAction::Open {
                item: map.get("i").cloned(),
            }),
            Some("done") => Ok(ToastAction::Done {
                item: get("i")?,
                occ: num("o")?,
            }),
            Some("snz") => Ok(ToastAction::Snooze {
                item: get("i")?,
                occ: num("o")?,
                minutes: minutes()?,
            }),
            Some("snzall") => Ok(ToastAction::SnoozeAll {
                minutes: minutes()?,
            }),
            other => Err(format!("unknown toast action {other:?} in '{args}'")),
        }
    }
}

impl Toast {
    pub fn to_xml(&self) -> String {
        let mut x = String::new();
        let scenario = if self.reminder && !self.buttons.is_empty() {
            r#" scenario="reminder""#
        } else {
            ""
        };
        let _ = write!(
            x,
            r#"<toast{scenario} launch="{}" activationType="foreground">"#,
            attr(&self.launch.to_args())
        );
        x.push_str(r#"<visual><binding template="ToastGeneric">"#);
        let _ = write!(x, "<text>{}</text>", text(&self.title));
        for line in self.lines.iter().filter(|l| !l.is_empty()).take(2) {
            let _ = write!(x, "<text>{}</text>", text(line));
        }
        x.push_str("</binding></visual>");
        // Dun plays its own chime; the toast itself is always silent.
        x.push_str(r#"<audio silent="true"/>"#);
        if !self.buttons.is_empty() {
            x.push_str("<actions>");
            for b in self.buttons.iter().take(5) {
                let _ = write!(
                    x,
                    r#"<action content="{}" arguments="{}" activationType="background"/>"#,
                    attr(&b.label),
                    attr(&b.action.to_args())
                );
            }
            x.push_str("</actions>");
        }
        x.push_str("</toast>");
        x
    }
}

fn text(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .fold(String::with_capacity(s.len()), |mut out, c| {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                c => out.push(c),
            }
            out
        })
}

fn attr(s: &str) -> String {
    text(s).replace('"', "&quot;").replace('\'', "&apos;")
}

/// Percent-encodes the three characters that structure the argument string.
fn encode(v: &str) -> String {
    v.replace('%', "%25")
        .replace(';', "%3B")
        .replace('=', "%3D")
}

fn decode(v: &str) -> Result<String, String> {
    let mut out = String::with_capacity(v.len());
    let mut rest = v;
    while let Some(pos) = rest.find('%') {
        out.push_str(&rest[..pos]);
        let code = rest
            .get(pos + 1..pos + 3)
            .ok_or_else(|| format!("truncated escape in '{v}'"))?;
        out.push(match code {
            "25" => '%',
            "3B" => ';',
            "3D" => '=',
            _ => return Err(format!("unexpected escape %{code} in '{v}'")),
        });
        rest = &rest[pos + 3..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring() -> Toast {
        let item = "0192f0c1-aaaa-7bbb-8ccc-0123456789ab".to_string();
        let occ = 1_789_500_000_000;
        Toast {
            tag: item.clone(),
            group: "ring".into(),
            title: "Pay rent & <utilities>".into(),
            lines: vec!["Due 9:00 AM".into(), "Landlord said \"cash\"".into()],
            launch: ToastAction::Open {
                item: Some(item.clone()),
            },
            buttons: vec![
                Button {
                    label: "Done".into(),
                    action: ToastAction::Done {
                        item: item.clone(),
                        occ,
                    },
                },
                Button {
                    label: "+1m".into(),
                    action: ToastAction::Snooze {
                        item: item.clone(),
                        occ,
                        minutes: 1,
                    },
                },
            ],
            reminder: true,
        }
    }

    #[test]
    fn args_round_trip() {
        let cases = [
            ToastAction::Open { item: None },
            ToastAction::Open {
                item: Some("x;y=z%".into()),
            },
            ToastAction::Done {
                item: "abc".into(),
                occ: -5,
            },
            ToastAction::Snooze {
                item: "abc".into(),
                occ: 1_789_500_000_000,
                minutes: 15,
            },
            ToastAction::SnoozeAll { minutes: 60 },
        ];
        for a in cases {
            assert_eq!(
                ToastAction::parse(&a.to_args()).unwrap(),
                a,
                "{}",
                a.to_args()
            );
        }
    }

    #[test]
    fn empty_args_mean_open() {
        assert_eq!(
            ToastAction::parse("").unwrap(),
            ToastAction::Open { item: None }
        );
    }

    #[test]
    fn malformed_args_are_rejected() {
        for bad in [
            "a=done;i=abc",           // no occurrence
            "a=snz;i=abc;o=1;m=0",    // zero minutes
            "a=snz;i=abc;o=1;m=9999", // absurd minutes
            "a=launch",
            "a=done;i=abc;o=notanumber",
            "garbage",
            "a=open;i=%ZZ",
        ] {
            assert!(ToastAction::parse(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn xml_escapes_text_and_attributes() {
        let xml = ring().to_xml();
        assert!(
            xml.contains("<text>Pay rent &amp; &lt;utilities&gt;</text>"),
            "{xml}"
        );
        assert!(xml.contains("<text>Landlord said \"cash\"</text>"), "{xml}");
        assert!(xml.contains(r#"scenario="reminder""#));
        assert!(xml.contains(r#"<audio silent="true"/>"#));
        assert!(xml.contains(r#"<action content="Done" arguments="a=done;i=0192f0c1-aaaa-7bbb-8ccc-0123456789ab;o=1789500000000" activationType="background"/>"#), "{xml}");
    }

    #[test]
    fn reminder_scenario_needs_a_button() {
        let mut t = ring();
        t.buttons.clear();
        assert!(
            !t.to_xml().contains("scenario="),
            "Windows ignores reminder toasts without buttons"
        );
    }

    #[test]
    fn at_most_five_buttons_and_three_text_lines() {
        let mut t = ring();
        t.lines = vec!["1".into(), "2".into(), "3".into()];
        t.buttons = (0..7)
            .map(|m| Button {
                label: format!("{m}"),
                action: ToastAction::SnoozeAll { minutes: m + 1 },
            })
            .collect();
        let xml = t.to_xml();
        assert_eq!(xml.matches("<action ").count(), 5);
        assert_eq!(xml.matches("<text>").count(), 3);
    }

    #[test]
    fn control_characters_are_stripped() {
        let mut t = ring();
        t.title = "bell\u{7}ring".into();
        assert!(t.to_xml().contains("<text>bellring</text>"));
    }
}
