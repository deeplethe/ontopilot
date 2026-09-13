//! 模型回复落库前的**形状归一**：与谓词、与文档无关的两条规矩，放在纯函数里，
//! 单测不用连库，服务端只负责把返回的信号记进丢弃表。
//!
//! **期间是"什么时候"，不是"什么"。**财报里一行 `Gross margin | 75.0 | %` 落在
//! `Q2 FY27` 那一列，模型常写成 `NVIDIA gross_margin_for_period Q2 FY27
//! {percentage: 75.0%}`——期间成了实体、当了宾语，数字塞进边属性。这个形状不是
//! 提示词能写死的（列头是期间的表格太多、写法太多），而账本本来就有时间轴：
//! 数字是值，期间是这条值的有效期。这里把它拆回去。
//!
//! **另一个已声明实体的所有格描述，不是新实体。**`NVIDIA invested_in SB Energy`
//! 与 `NVIDIA invested_in "SB Energy's growth and commitments to the Ohio
//! community"` 同出一句，后者是前者的描述。判据是结构性的——同一句、同主语、
//! 同谓词、本尊那条边就在旁边——而不是词面：`OpenAI's board of directors` 也是
//! 所有格开头，但它是一个东西，旁边没有一条指向 OpenAI 的同样的边。

use crate::{ExtractedFact, Extraction};
use chrono::NaiveDate;

/// 归一化做了什么；服务端按条记信号，量得出每个模型、每个库多常这么写
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Normalization {
    /// 期间做了宾语、边上带着数：拆成带有效期的值事实。`dated` = 期间解得出日期
    PeriodToValidity {
        predicate: String,
        label: String,
        dated: bool,
        values: usize,
    },
    /// 期间做了宾语、什么数都没带：这条事实只说「X 在 P 有过什么」，没有可落的值
    PeriodAsObject { predicate: String, label: String },
    /// 宾语是另一个声明实体的所有格描述，本尊那条边同句已在：丢，连同它的声明
    ObjectDescribesDeclared {
        predicate: String,
        name: String,
        head: String,
    },
}

/// 这段文字是不是一个期间标签：季度、半年、财年、年、月。
///
/// 认得出但解不出日期的（`Q2 FY27`：财年从哪天起要看公司）也算——它照样不是实体。
pub fn looks_like_period(s: &str) -> bool {
    period_shape(s).is_some()
}

/// 日历上能定位的期间的起止日（含两端）。财年的解不出，返回 None。
pub fn period_span(s: &str) -> Option<(NaiveDate, NaiveDate)> {
    match period_shape(s)? {
        Shape::Year(y) => Some((ymd(y, 1, 1)?, ymd(y, 12, 31)?)),
        Shape::Month(y, m) => Some((ymd(y, m, 1)?, month_end(y, m)?)),
        Shape::Quarter(y, q) => {
            let m = (q - 1) * 3 + 1;
            Some((ymd(y, m, 1)?, month_end(y, m + 2)?))
        }
        Shape::Half(y, h) => {
            let m = if h == 1 { 1 } else { 7 };
            Some((ymd(y, m, 1)?, month_end(y, m + 5)?))
        }
        Shape::Fiscal => None,
    }
}

enum Shape {
    Year(i32),
    Month(i32, u32),
    Quarter(i32, u32),
    Half(i32, u32),
    /// 带 FY / fiscal 的：是期间，但起止日不在标签里
    Fiscal,
}

fn period_shape(s: &str) -> Option<Shape> {
    let t = s.trim().trim_end_matches(['.', ',']);
    if t.is_empty() {
        return None;
    }
    // 2026 / 2026-06 / 2026-06-30 直接是日期：整年或整月算期间，具体到日的是时点，不算
    // 光秃秃的四位数才是整年；`FY2026`、`2026 Annual Meeting` 都不是
    if t.chars().all(|c| c.is_ascii_digit()) {
        return parse_year(t).map(Shape::Year);
    }
    if let Some((y, m)) = t.split_once('-') {
        if let (Some(y), Ok(m)) = (parse_year(y), m.parse::<u32>()) {
            if (1..=12).contains(&m) {
                return Some(Shape::Month(y, m));
            }
        }
        // 2026-Q2 / 2026-H1
        if let (Some(y), Some(part)) = (parse_year(y), qh_token(m)) {
            return Some(match part {
                ('q', n) => Shape::Quarter(y, n),
                (_, n) => Shape::Half(y, n),
            });
        }
    }
    let lower = t.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let fiscal = words.iter().any(|w| {
        *w == "fy"
            || *w == "fiscal"
            || (w.starts_with("fy") && w[2..].chars().all(|c| c.is_ascii_digit()))
    });
    // Q2 2026 / Q2 FY27 / 2Q26 / 2026 Q2 / H1 2027 / H1 FY26
    let qh = words.iter().find_map(|w| qh_token(w));
    let year = words.iter().find_map(|w| parse_year(w)).or_else(|| {
        words.iter().find_map(|w| {
            // fy27 → 27；2q26 / 1h27 → 尾上那两位
            let w = w.strip_prefix("fy").unwrap_or(w);
            let w = match w.as_bytes() {
                [d, b'q' | b'h', rest @ ..] if d.is_ascii_digit() => {
                    std::str::from_utf8(rest).unwrap_or(w)
                }
                _ => w,
            };
            (w.len() == 2 && w.chars().all(|c| c.is_ascii_digit()))
                .then(|| w.parse::<i32>().ok().map(|n| 2000 + n))
                .flatten()
        })
    });
    // 「first quarter of 2026」「second half of fiscal 2027」
    let ordinal = words.iter().position(|w| {
        matches!(
            *w,
            "first" | "second" | "third" | "fourth" | "1st" | "2nd" | "3rd" | "4th"
        )
    });
    let ordinal_part = ordinal.and_then(|i| {
        let n = match words[i] {
            "first" | "1st" => 1,
            "second" | "2nd" => 2,
            "third" | "3rd" => 3,
            _ => 4,
        };
        match words.get(i + 1).copied() {
            Some("quarter") => Some(('q', n)),
            Some("half") if n <= 2 => Some(('h', n)),
            _ => None,
        }
    });
    match (qh.or(ordinal_part), year, fiscal) {
        (Some(_), _, true) | (None, Some(_), true) => Some(Shape::Fiscal),
        (Some(('q', n)), Some(y), false) => Some(Shape::Quarter(y, n)),
        (Some(('h', n)), Some(y), false) => Some(Shape::Half(y, n)),
        (Some(_), None, _) => None,
        (None, Some(y), false) => {
            // May 2026 / 2026年6月 这类：月名 + 年
            if let Some(m) = words.iter().find_map(|w| month_name(w)) {
                return Some(Shape::Month(y, m));
            }
            // 只有一个年份词、别的都是 "year" / "calendar"：整年
            let rest: Vec<&&str> = words
                .iter()
                .filter(|w| parse_year(w).is_none() && !matches!(**w, "year" | "calendar" | "cy"))
                .collect();
            rest.is_empty().then_some(Shape::Year(y))
        }
        _ => None,
    }
}

fn qh_token(w: &str) -> Option<(char, u32)> {
    let w = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let lower = w.to_ascii_lowercase();
    let b = lower.as_bytes();
    // q2 / h1 / 2q / 1h（后面可以直接接年份：2q26）
    let (kind, n) = match b {
        [k @ (b'q' | b'h'), d, ..] if d.is_ascii_digit() => (*k as char, (d - b'0') as u32),
        [d, k @ (b'q' | b'h'), ..] if d.is_ascii_digit() => (*k as char, (d - b'0') as u32),
        _ => return None,
    };
    let ok = match kind {
        'q' => (1..=4).contains(&n),
        _ => (1..=2).contains(&n),
    };
    // 剩下的字符只能是年份数字（2q26），不能是别的字母（"quarterly"）
    let tail: String = lower.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    (ok && tail.len() == 1).then_some((kind, n))
}

fn parse_year(w: &str) -> Option<i32> {
    let w = w.trim_matches(|c: char| !c.is_ascii_digit());
    (w.len() == 4)
        .then(|| w.parse::<i32>().ok())
        .flatten()
        .filter(|y| (1900..=2100).contains(y))
}

fn month_name(w: &str) -> Option<u32> {
    Some(match w.trim_matches(|c: char| !c.is_ascii_alphabetic()) {
        "january" | "jan" => 1,
        "february" | "feb" => 2,
        "march" | "mar" => 3,
        "april" | "apr" => 4,
        "may" => 5,
        "june" | "jun" => 6,
        "july" | "jul" => 7,
        "august" | "aug" => 8,
        "september" | "sep" | "sept" => 9,
        "october" | "oct" => 10,
        "november" | "nov" => 11,
        "december" | "dec" => 12,
        _ => return None,
    })
}

fn ymd(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y, m, d)
}

fn month_end(y: i32, m: u32) -> Option<NaiveDate> {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    ymd(ny, nm, 1)?.pred_opt()
}

/// 边属性里不是值、是单位的那几个键（与服务端同一张表）
fn is_unit_key(k: &str) -> bool {
    matches!(
        k.trim().to_lowercase().as_str(),
        "currency" | "币种" | "货币" | "unit" | "单位"
    )
}

/// 谓词里把期间说了一遍的尾巴：期间进了有效期，尾巴就多余了。收得窄，只剥这几种
fn strip_period_suffix(p: &str) -> &str {
    for suf in [
        "_for_the_period",
        "_for_period",
        "_in_period",
        "_for_the_quarter",
    ] {
        if let Some(s) = p.strip_suffix(suf) {
            if !s.is_empty() {
                return s;
            }
        }
    }
    p
}

/// 两条规矩，按上面模块注释的顺序。改 `x` 本身，返回做了什么。
pub fn normalize_facts(x: &mut Extraction) -> Vec<Normalization> {
    let mut out = Vec::new();
    let mut facts: Vec<ExtractedFact> = Vec::with_capacity(x.facts.len());

    // ---- 1. 期间做宾语 ----
    for f in x.facts.drain(..) {
        let label = f
            .object
            .as_deref()
            .map(str::trim)
            .filter(|o| !o.is_empty() && looks_like_period(o))
            .map(str::to_owned);
        let Some(label) = label else {
            facts.push(f);
            continue;
        };
        let values: Vec<(String, serde_json::Value)> = f
            .qualifiers
            .as_ref()
            .map(|q| {
                q.iter()
                    .filter(|(k, v)| !v.is_null() && !is_unit_key(k))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();
        if values.is_empty() {
            out.push(Normalization::PeriodAsObject {
                predicate: f.predicate.clone(),
                label,
            });
            continue;
        }
        let span = period_span(&label);
        let base = strip_period_suffix(f.predicate.trim()).to_string();
        let several = values.len() > 1;
        for (key, value) in &values {
            let predicate = if several {
                format!("{base}.{}", key.trim())
            } else {
                base.clone()
            };
            let (valid_from, valid_to) = match span {
                Some((a, b)) => (
                    Some(a.format("%Y-%m-%d").to_string()),
                    Some(b.format("%Y-%m-%d").to_string()),
                ),
                // 财年：标签认得出、日期定不了，留模型自己写的（多半是空）
                None => (f.valid_from.clone(), f.valid_to.clone()),
            };
            facts.push(ExtractedFact {
                subject: f.subject.clone(),
                subject_ref: f.subject_ref.clone(),
                predicate,
                object: None,
                object_ref: None,
                value: Some(value.clone()),
                qualifiers: None,
                valid_from,
                valid_to,
                confidence: f.confidence,
                quote: f.quote.clone(),
                subject_span: f.subject_span.clone(),
                object_span: None,
            });
        }
        out.push(Normalization::PeriodToValidity {
            predicate: f.predicate.clone(),
            label,
            dated: span.is_some(),
            values: values.len(),
        });
    }

    // ---- 2. 所有格描述 ----
    // 声明名 → 句柄；一个句柄可能没有（旧回复），那就按名字比
    let name_of = |f: &ExtractedFact, side_ref: Option<&str>, side_name: Option<&str>| {
        side_ref
            .and_then(|h| {
                x.entities
                    .iter()
                    .find(|e| e.local_id.as_deref().map(str::trim) == Some(h.trim()))
            })
            .map(|e| e.name.trim().to_string())
            .or_else(|| side_name.map(|s| s.trim().to_string()))
            .filter(|_| f.object.is_some() || side_ref.is_some())
    };
    let declared: Vec<String> = x
        .entities
        .iter()
        .map(|e| e.name.trim().to_string())
        .collect();
    let head_of = |name: &str| -> Option<String> {
        let lower = name.to_lowercase();
        declared
            .iter()
            .filter(|m| !m.eq_ignore_ascii_case(name))
            .find(|m| {
                let ml = m.to_lowercase();
                ["'s ", "\u{2019}s "].iter().any(|p| {
                    lower
                        .strip_prefix(&ml)
                        .is_some_and(|r| r.starts_with(p) && r.len() > p.len())
                })
            })
            .cloned()
    };
    let objects: Vec<Option<String>> = facts
        .iter()
        .map(|f| name_of(f, f.object_ref.as_deref(), f.object.as_deref()))
        .collect();
    let subjects: Vec<Option<String>> = facts
        .iter()
        .map(|f| name_of(f, f.subject_ref.as_deref(), Some(f.subject.as_str())))
        .collect();
    let mut drop = vec![false; facts.len()];
    for i in 0..facts.len() {
        let Some(name) = objects[i].as_deref() else {
            continue;
        };
        let Some(head) = head_of(name) else { continue };
        let sibling = (0..facts.len()).any(|j| {
            j != i
                && facts[j].predicate.eq_ignore_ascii_case(&facts[i].predicate)
                && subjects[j] == subjects[i]
                && objects[j]
                    .as_deref()
                    .is_some_and(|o| o.eq_ignore_ascii_case(&head))
        });
        if sibling {
            drop[i] = true;
            out.push(Normalization::ObjectDescribesDeclared {
                predicate: facts[i].predicate.clone(),
                name: name.to_string(),
                head,
            });
        }
    }
    let dropped_names: Vec<String> = (0..facts.len())
        .filter(|i| drop[*i])
        .filter_map(|i| objects[i].clone())
        .collect();
    let kept: Vec<ExtractedFact> = facts
        .into_iter()
        .zip(drop)
        .filter_map(|(f, d)| (!d).then_some(f))
        .collect();
    // 被丢的描述的声明也去掉——留着它，声明循环照样会把它造成一个孤点
    if !dropped_names.is_empty() {
        let still_used = |e: &crate::ExtractedEntity| {
            kept.iter().any(|f| {
                let by_ref = |r: Option<&String>| {
                    r.map(|h| h.trim()) == e.local_id.as_deref().map(str::trim)
                        && e.local_id.is_some()
                };
                by_ref(f.subject_ref.as_ref())
                    || by_ref(f.object_ref.as_ref())
                    || f.subject.trim().eq_ignore_ascii_case(e.name.trim())
                    || f.object
                        .as_deref()
                        .is_some_and(|o| o.trim().eq_ignore_ascii_case(e.name.trim()))
            })
        };
        x.entities.retain(|e| {
            !dropped_names
                .iter()
                .any(|n| n.eq_ignore_ascii_case(e.name.trim()))
                || still_used(e)
        });
    }
    x.facts = kept;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExtractedEntity;

    fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    #[test]
    fn a_calendar_period_resolves_to_its_dates() {
        assert_eq!(
            period_span("Q2 2026"),
            Some((d(2026, 4, 1), d(2026, 6, 30)))
        );
        assert_eq!(
            period_span("2026-Q4"),
            Some((d(2026, 10, 1), d(2026, 12, 31)))
        );
        assert_eq!(period_span("2q26"), Some((d(2026, 4, 1), d(2026, 6, 30))));
        assert_eq!(
            period_span("H1 2027"),
            Some((d(2027, 1, 1), d(2027, 6, 30)))
        );
        assert_eq!(
            period_span("second half of 2026"),
            Some((d(2026, 7, 1), d(2026, 12, 31)))
        );
        assert_eq!(
            period_span("June 2026"),
            Some((d(2026, 6, 1), d(2026, 6, 30)))
        );
        assert_eq!(
            period_span("2026-02"),
            Some((d(2026, 2, 1), d(2026, 2, 28)))
        );
        assert_eq!(period_span("2026"), Some((d(2026, 1, 1), d(2026, 12, 31))));
        assert_eq!(
            period_span("calendar year 2026"),
            Some((d(2026, 1, 1), d(2026, 12, 31)))
        );
    }

    /// 财年认得出是期间，但从哪天起要看公司：不解，也不猜
    #[test]
    fn a_fiscal_period_is_a_period_but_has_no_dates_of_its_own() {
        for s in [
            "Q2 FY27",
            "FY2026",
            "fy27",
            "first quarter of fiscal 2027",
            "H2 FY26",
        ] {
            assert!(looks_like_period(s), "{s}");
            assert_eq!(period_span(s), None, "{s}");
        }
    }

    #[test]
    fn things_that_are_not_periods() {
        for s in [
            "NVIDIA",
            "Q2 results",
            "quarterly report",
            "2026 Annual Meeting of Stockholders",
            "Form 10-K",
            "2026-07-26",
            "May",
            "3M",
        ] {
            assert!(!looks_like_period(s), "{s}");
        }
    }

    fn fact(
        subject: &str,
        predicate: &str,
        object: Option<&str>,
        object_ref: Option<&str>,
    ) -> ExtractedFact {
        ExtractedFact {
            subject: subject.into(),
            subject_ref: Some("e1".into()),
            predicate: predicate.into(),
            object: object.map(str::to_string),
            object_ref: object_ref.map(str::to_string),
            value: None,
            qualifiers: None,
            valid_from: None,
            valid_to: None,
            confidence: Some(0.9),
            quote: Some("Gross margin | 75.0 | %".into()),
            subject_span: Some(subject.into()),
            object_span: object.map(str::to_string),
        }
    }
    fn entity(id: &str, name: &str) -> ExtractedEntity {
        ExtractedEntity {
            local_id: Some(id.into()),
            name: name.into(),
            type_key: "organization".into(),
            specific_type: None,
        }
    }
    fn quals(pairs: &[(&str, &str)]) -> serde_json::Map<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect()
    }

    /// 财报那一格的形状：`NVIDIA gross_margin_for_period Q2 2026 {percentage: 75.0%}`
    #[test]
    fn a_period_object_with_a_figure_becomes_a_dated_value_fact() {
        let mut f = fact(
            "NVIDIA",
            "gross_margin_for_period",
            Some("Q2 2026"),
            Some("e7"),
        );
        f.qualifiers = Some(quals(&[("percentage", "75.0%")]));
        let mut x = Extraction {
            entities: vec![entity("e1", "NVIDIA"), entity("e7", "Q2 2026")],
            facts: vec![f],
            skipped_entities: 0,
            skipped_facts: 0,
            truncated: false,
        };
        let n = normalize_facts(&mut x);
        assert_eq!(x.facts.len(), 1);
        let g = &x.facts[0];
        assert_eq!(g.predicate, "gross_margin");
        assert_eq!(g.object, None);
        assert_eq!(g.value, Some(serde_json::Value::String("75.0%".into())));
        assert_eq!(g.valid_from.as_deref(), Some("2026-04-01"));
        assert_eq!(g.valid_to.as_deref(), Some("2026-06-30"));
        assert_eq!(
            n,
            vec![Normalization::PeriodToValidity {
                predicate: "gross_margin_for_period".into(),
                label: "Q2 2026".into(),
                dated: true,
                values: 1
            }]
        );
    }

    /// 财年：数照收，日期留模型写的（这里是空），信号里说明没定出日期
    #[test]
    fn a_fiscal_period_keeps_the_figure_and_says_it_is_undated() {
        let mut f = fact(
            "NVIDIA",
            "net_income_for_period",
            Some("Q2 FY27"),
            Some("e7"),
        );
        f.valid_from = Some("2026-05".into());
        f.qualifiers = Some(quals(&[("amount", "$53,954 million"), ("currency", "USD")]));
        let mut x = Extraction {
            entities: vec![entity("e1", "NVIDIA"), entity("e7", "Q2 FY27")],
            facts: vec![f],
            skipped_entities: 0,
            skipped_facts: 0,
            truncated: false,
        };
        let n = normalize_facts(&mut x);
        assert_eq!(x.facts.len(), 1, "currency 是单位，不是第二个值");
        assert_eq!(x.facts[0].predicate, "net_income");
        assert_eq!(x.facts[0].valid_from.as_deref(), Some("2026-05"));
        assert!(matches!(
            &n[0],
            Normalization::PeriodToValidity {
                dated: false,
                values: 1,
                ..
            }
        ));
    }

    #[test]
    fn two_figures_on_one_period_keep_their_keys() {
        let mut f = fact("NVIDIA", "results_for_period", Some("Q2 2026"), None);
        f.qualifiers = Some(quals(&[
            ("revenue", "$96.2 billion"),
            ("net_income", "$53.9 billion"),
        ]));
        let mut x = Extraction {
            entities: vec![entity("e1", "NVIDIA")],
            facts: vec![f],
            skipped_entities: 0,
            skipped_facts: 0,
            truncated: false,
        };
        normalize_facts(&mut x);
        let mut preds: Vec<&str> = x.facts.iter().map(|f| f.predicate.as_str()).collect();
        preds.sort();
        assert_eq!(preds, ["results.net_income", "results.revenue"]);
    }

    #[test]
    fn a_period_object_with_nothing_on_the_edge_is_dropped() {
        let mut x = Extraction {
            entities: vec![entity("e1", "NVIDIA")],
            facts: vec![fact("NVIDIA", "reported_in", Some("Q2 FY27"), None)],
            skipped_entities: 0,
            skipped_facts: 0,
            truncated: false,
        };
        let n = normalize_facts(&mut x);
        assert!(x.facts.is_empty());
        assert!(matches!(&n[0], Normalization::PeriodAsObject { label, .. } if label == "Q2 FY27"));
    }

    /// 同一句、同主语、同谓词，本尊那条边就在旁边：所有格描述是描述，连声明一起去掉
    #[test]
    fn a_possessive_description_beside_its_head_goes_and_takes_its_declaration_along() {
        let mut x = Extraction {
            entities: vec![
                entity("e1", "NVIDIA"),
                entity("e2", "SB Energy"),
                entity(
                    "e6",
                    "SB Energy's growth and commitments to the Ohio community",
                ),
            ],
            facts: vec![
                fact("NVIDIA", "invested_in", Some("SB Energy"), Some("e2")),
                fact(
                    "NVIDIA",
                    "invested_in",
                    Some("SB Energy\u{2019}s growth and commitments to the Ohio community"),
                    Some("e6"),
                ),
            ],
            skipped_entities: 0,
            skipped_facts: 0,
            truncated: false,
        };
        let n = normalize_facts(&mut x);
        assert_eq!(x.facts.len(), 1);
        assert_eq!(x.facts[0].object.as_deref(), Some("SB Energy"));
        assert!(
            x.entities
                .iter()
                .all(|e| e.local_id.as_deref() != Some("e6")),
            "e6 不该留下成孤点"
        );
        assert!(
            matches!(&n[0], Normalization::ObjectDescribesDeclared { head, .. } if head == "SB Energy")
        );
    }

    /// 所有格开头但旁边没有本尊那条边：它可能真是一个东西（董事会），不动
    #[test]
    fn a_possessive_name_without_a_sibling_edge_is_left_alone() {
        let mut x = Extraction {
            entities: vec![
                entity("e1", "Sam Altman"),
                entity("e2", "OpenAI"),
                entity("e3", "OpenAI's board of directors"),
            ],
            facts: vec![fact(
                "Sam Altman",
                "removed_by",
                Some("OpenAI's board of directors"),
                Some("e3"),
            )],
            skipped_entities: 0,
            skipped_facts: 0,
            truncated: false,
        };
        let n = normalize_facts(&mut x);
        assert_eq!(x.facts.len(), 1);
        assert_eq!(x.entities.len(), 3);
        assert!(n.is_empty());
    }
}
