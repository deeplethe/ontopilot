//! 模型回复落库前的**形状检查**：只看结构，不看词。
//!
//! **分工。**读懂原文里的时间、判断一段话是不是一个东西——这是语言问题，归模型，
//! 契约（提示词 3c）说清楚它该怎么写。这里只核对输出有没有照契约的形状写，判据一律
//! 是结构性的：引文里有没有这段字、值是不是只有标点、一侧是不是契约的日期格式、
//! 同一句里有没有另一条边。**不认任何一种语言的词**——第一版按英文词表认「季度」
//!「N months ended」，中文财报一条都认不出，还把四份报告的标题当成期间删了。
//!
//! 每条规则做了什么都返回给服务端记进丢弃表：违约多常见、出在哪个模型，量得出来，
//! 契约该怎么改看数说话。

use crate::{parse_time, ExtractedFact, Extraction};
use std::collections::HashSet;

/// 形状检查做了什么；服务端按条记信号
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Normalization {
    /// 值只有标点（`—`）或是空的：表里的「无」，不是一个值，不落
    NoValue { predicate: String, written: String },
    /// 值后面有一截引文里没有的字：只留引文里有的那段。模型读对了表头、却把期间
    /// 写进了值（`(6,176) for three months ended July 27, 2025`），引文只有 `(6,176)`
    ValueTrimmed {
        predicate: String,
        kept: String,
        dropped: String,
    },
    /// 没有宾语、没有值、只带边属性：每个属性落成主语上的一条值事实。
    /// 从前整条以 object_missing 丢掉，写对了的数跟着没了
    QualifiersWithoutObject { predicate: String, values: usize },
    /// 宾语是契约格式的日期（`2026-06-30`）：时间不是实体。边上的数落成值、日期进有效期；
    /// 没带数的不落
    TimeAsObject {
        predicate: String,
        written: String,
        values: usize,
    },
    /// 主语是契约格式的日期：数是某个东西在那一刻的数，那个东西是谁回复里没说。不落
    TimeAsSubject { predicate: String, written: String },
    /// 宾语的名字包住了另一个声明实体，而同一句、同主语、同谓词已有一条指向那个实体的边：
    /// 它是那个实体的描述，不落
    ObjectDescribesDeclared {
        predicate: String,
        name: String,
        head: String,
    },
    /// 上面几条去掉事实之后，没有任何事实再引用的声明：不建，否则就是一个孤点
    OrphanDeclaration { name: String },
}

/// 比对用的形态：空白折叠、小写
fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// 值后面是否挂着一截不属于它的字。返回 (保留的前缀, 去掉的尾巴)。
///
/// 两个条件同时成立才剪，都是结构，不认词：
/// - **保留的那段在引文里，而且含数字**——它是原文写的那个数；
/// - **尾巴不在引文里，而且自己含数字**——它是另一条信息（一个期间、一个日期、
///   另一个百分比），不是这个数的单位。
///
/// 第二条要数字，是因为挂在数后面、引文里又没有的，还有一类是对的：表头上的量级与
/// 单位（`53,954 million USD`，这一行引文只有 `53,954`，`million` 在表头「in millions」）、
/// 模型换了写法的单位（`10 gigawatts`）、缩写的头衔（`founder and CEO`）。它们不带数字，
/// 不剪。整个值在引文里的，一个字不动。
fn ungrounded_tail<'a>(value: &'a str, quote: &str) -> Option<(&'a str, &'a str)> {
    let q = norm(quote);
    if q.is_empty() || q.contains(&norm(value)) {
        return None;
    }
    let has_digit = |t: &str| t.chars().any(|c| c.is_numeric());
    let bounds: Vec<usize> = value
        .char_indices()
        .filter(|(i, c)| c.is_whitespace() && *i > 0)
        .map(|(i, _)| i)
        .collect();
    let (kept, rest) = bounds
        .iter()
        .rev()
        .map(|&i| {
            (
                value[..i]
                    .trim()
                    .trim_end_matches([',', ';', ':', '\u{3001}', '\u{FF0C}']),
                value[i..].trim(),
            )
        })
        .find(|(p, _)| !p.is_empty() && has_digit(p) && q.contains(&norm(p)))?;
    (!rest.is_empty() && has_digit(rest) && !q.contains(&norm(rest))).then_some((kept, rest))
}

/// 只有标点、没有字母也没有数字：表格里表示「没有」的那一格
fn is_no_value(written: &str) -> bool {
    !written.chars().any(char::is_alphanumeric)
}

/// 边属性里不是值、是单位的那几个键（与服务端同一张表）
fn is_unit_key(k: &str) -> bool {
    matches!(
        k.trim().to_lowercase().as_str(),
        "currency" | "币种" | "货币" | "unit" | "单位"
    )
}

fn qualifier_values(f: &ExtractedFact) -> Vec<(String, serde_json::Value)> {
    f.qualifiers
        .as_ref()
        .map(|q| {
            q.iter()
                .filter(|(k, v)| !v.is_null() && !is_unit_key(k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn value_fact(
    f: &ExtractedFact,
    predicate: String,
    value: serde_json::Value,
    valid_from: Option<String>,
) -> ExtractedFact {
    ExtractedFact {
        subject: f.subject.clone(),
        subject_ref: f.subject_ref.clone(),
        predicate,
        object: None,
        object_ref: None,
        value: Some(value),
        qualifiers: None,
        valid_from: valid_from.or_else(|| f.valid_from.clone()),
        valid_to: f.valid_to.clone(),
        confidence: f.confidence,
        quote: f.quote.clone(),
        subject_span: f.subject_span.clone(),
        object_span: None,
    }
}

/// 一侧写的是契约格式的时间（`YYYY` / `YYYY-MM` / `YYYY-MM-DD`，带时区的时刻）
fn is_contract_time(s: &str) -> bool {
    parse_time(s.trim()).is_some()
}

pub fn normalize_facts(x: &mut Extraction) -> Vec<Normalization> {
    let mut out = Vec::new();
    let entities = std::mem::take(&mut x.entities);

    // 一侧绑到的声明名：有句柄按句柄，没有按写出来的名字
    let handle_name = |h: Option<&String>| {
        h.and_then(|h| {
            entities
                .iter()
                .find(|e| e.local_id.as_deref().map(str::trim) == Some(h.trim()))
                .map(|e| e.name.trim().to_string())
        })
    };
    let referenced = |facts: &[ExtractedFact]| -> HashSet<String> {
        facts
            .iter()
            .flat_map(|f| {
                [
                    handle_name(f.subject_ref.as_ref())
                        .or_else(|| Some(f.subject.trim().to_string())),
                    handle_name(f.object_ref.as_ref())
                        .or_else(|| f.object.as_deref().map(|o| o.trim().to_string())),
                ]
            })
            .flatten()
            .map(|n| n.to_lowercase())
            .collect()
    };
    let before = referenced(&x.facts);

    let mut facts: Vec<ExtractedFact> = Vec::with_capacity(x.facts.len());
    for mut f in std::mem::take(&mut x.facts) {
        let quote = f.quote.clone().unwrap_or_default();

        // ---- 值 ----
        if let Some(written) = f.value.as_ref().and_then(|v| v.as_str()).map(str::to_owned) {
            if is_no_value(&written) {
                out.push(Normalization::NoValue {
                    predicate: f.predicate.clone(),
                    written,
                });
                continue;
            }
            if let Some((kept, dropped)) = ungrounded_tail(&written, &quote) {
                out.push(Normalization::ValueTrimmed {
                    predicate: f.predicate.clone(),
                    kept: kept.to_string(),
                    dropped: dropped.to_string(),
                });
                f.value = Some(serde_json::Value::String(kept.to_string()));
            }
        }

        // ---- 主语是时间 ----
        let subject =
            handle_name(f.subject_ref.as_ref()).unwrap_or_else(|| f.subject.trim().to_string());
        if is_contract_time(&subject) {
            out.push(Normalization::TimeAsSubject {
                predicate: f.predicate.clone(),
                written: subject,
            });
            continue;
        }

        let values = qualifier_values(&f);
        let object = handle_name(f.object_ref.as_ref())
            .or_else(|| f.object.as_deref().map(|o| o.trim().to_string()))
            .filter(|o| !o.is_empty());
        let has_value = f.value.as_ref().is_some_and(|v| !v.is_null());

        // ---- 只有边属性 ----
        if object.is_none() && !has_value && !values.is_empty() {
            for (key, value) in &values {
                let predicate = format!("{}.{}", f.predicate.trim(), key.trim());
                facts.push(value_fact(&f, predicate, value.clone(), None));
            }
            out.push(Normalization::QualifiersWithoutObject {
                predicate: f.predicate.clone(),
                values: values.len(),
            });
            continue;
        }

        // ---- 宾语是时间 ----
        if let Some(o) = object.as_deref().filter(|o| is_contract_time(o)) {
            let several = values.len() > 1;
            for (key, value) in &values {
                let predicate = if several {
                    format!("{}.{}", f.predicate.trim(), key.trim())
                } else {
                    f.predicate.trim().to_string()
                };
                facts.push(value_fact(
                    &f,
                    predicate,
                    value.clone(),
                    Some(o.to_string()),
                ));
            }
            out.push(Normalization::TimeAsObject {
                predicate: f.predicate.clone(),
                written: o.to_string(),
                values: values.len(),
            });
            continue;
        }

        facts.push(f);
    }

    // ---- 描述：名字包住另一个声明实体，本尊那条边同句已在 ----
    let declared: Vec<String> = entities.iter().map(|e| e.name.trim().to_string()).collect();
    let side = |r: Option<&String>, w: Option<&str>| {
        handle_name(r)
            .or_else(|| w.map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
    };
    let objects: Vec<Option<String>> = facts
        .iter()
        .map(|f| side(f.object_ref.as_ref(), f.object.as_deref()))
        .collect();
    let subjects: Vec<Option<String>> = facts
        .iter()
        .map(|f| side(f.subject_ref.as_ref(), Some(f.subject.as_str())))
        .collect();
    // 名字边界：拉丁字母数字前后不能粘着字母数字；汉字之间本来就没有空格，不设边界
    let contains_name = |outer: &str, inner: &str| {
        let (o, i) = (norm(outer), norm(inner));
        !i.is_empty()
            && o.len() > i.len()
            && o.match_indices(&i).any(|(at, _)| {
                let glued = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
                let edge_in = i.chars().next().is_some_and(|c| c.is_ascii_alphanumeric());
                let edge_out = i
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric());
                !(edge_in && glued(o[..at].chars().next_back()))
                    && !(edge_out && glued(o[at + i.len()..].chars().next()))
            })
    };
    let mut keep = vec![true; facts.len()];
    for i in 0..facts.len() {
        let Some(name) = objects[i].as_deref() else {
            continue;
        };
        let quote_i = norm(facts[i].quote.as_deref().unwrap_or(""));
        let head = declared.iter().find(|h| {
            contains_name(name, h)
                && (0..facts.len()).any(|j| {
                    j != i
                        && facts[j].predicate.eq_ignore_ascii_case(&facts[i].predicate)
                        && subjects[j] == subjects[i]
                        && objects[j]
                            .as_deref()
                            .is_some_and(|o| o.eq_ignore_ascii_case(h))
                        && {
                            let quote_j = norm(facts[j].quote.as_deref().unwrap_or(""));
                            quote_i.contains(&quote_j) || quote_j.contains(&quote_i)
                        }
                })
        });
        if let Some(head) = head {
            keep[i] = false;
            out.push(Normalization::ObjectDescribesDeclared {
                predicate: facts[i].predicate.clone(),
                name: name.to_string(),
                head: head.clone(),
            });
        }
    }
    x.facts = facts
        .into_iter()
        .zip(keep)
        .filter_map(|(f, k)| k.then_some(f))
        .collect();

    // ---- 被上面几条弄成孤点的声明 ----
    // 只去掉「原来有事实引用、现在没有了」的：模型一开始就只声明不连边的，不归这里管
    let after = referenced(&x.facts);
    let mut orphans = Vec::new();
    let mut kept_entities = entities;
    kept_entities.retain(|e| {
        let n = e.name.trim().to_lowercase();
        let orphan = before.contains(&n) && !after.contains(&n);
        if orphan {
            orphans.push(e.name.trim().to_string());
        }
        !orphan
    });
    x.entities = kept_entities;
    out.extend(
        orphans
            .into_iter()
            .map(|name| Normalization::OrphanDeclaration { name }),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExtractedEntity;

    fn fact(subject: &str, predicate: &str, object: Option<&str>, quote: &str) -> ExtractedFact {
        ExtractedFact {
            subject: subject.into(),
            subject_ref: None,
            predicate: predicate.into(),
            object: object.map(str::to_string),
            object_ref: None,
            value: None,
            qualifiers: None,
            valid_from: None,
            valid_to: None,
            confidence: Some(0.9),
            quote: Some(quote.into()),
            subject_span: Some(subject.into()),
            object_span: object.map(str::to_string),
        }
    }
    fn valued(subject: &str, predicate: &str, value: &str, quote: &str) -> ExtractedFact {
        let mut f = fact(subject, predicate, None, quote);
        f.value = Some(serde_json::Value::String(value.into()));
        f
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
    fn run(
        entities: Vec<ExtractedEntity>,
        facts: Vec<ExtractedFact>,
    ) -> (Extraction, Vec<Normalization>) {
        let mut x = Extraction {
            entities,
            facts,
            skipped_entities: 0,
            skipped_facts: 0,
            truncated: false,
        };
        let n = normalize_facts(&mut x);
        (x, n)
    }
    fn value_of(f: &ExtractedFact) -> &str {
        f.value.as_ref().unwrap().as_str().unwrap()
    }

    /// 截图里那 21 条：模型读对了表头，却把期间写进了值。引文里只有数
    #[test]
    fn a_tail_the_quote_does_not_contain_is_not_the_value() {
        let (x, n) = run(
            vec![entity("e1", "NVIDIA")],
            vec![
                valued(
                    "NVIDIA",
                    "net_cash_used",
                    "(6,176) for three months ended July 27, 2025",
                    "Net cash used in financing activities (6,176)",
                ),
                // 同样的形状，中文：不认词，照样剪
                valued(
                    "NVIDIA",
                    "经营现金流",
                    "12,345 截至2025年7月27日止三个月",
                    "经营活动产生的现金流量净额 | 12,345",
                ),
                valued(
                    "NVIDIA",
                    "revenue",
                    "$89.0 billion, up 18% from the previous quarter",
                    "Second-quarter revenue was $89.0 billion",
                ),
            ],
        );
        assert_eq!(value_of(&x.facts[0]), "(6,176)");
        assert_eq!(value_of(&x.facts[1]), "12,345");
        // 数后面接着另一个数：「up 18%」是另一条事实，不是这个数的一部分；逗号跟着前缀走
        assert_eq!(value_of(&x.facts[2]), "$89.0 billion");
        assert!(
            matches!(&n[0], Normalization::ValueTrimmed { dropped, .. } if dropped == "for three months ended July 27, 2025")
        );
    }

    #[test]
    fn a_value_grounded_in_the_quote_is_left_alone() {
        let (x, n) = run(
            vec![entity("e1", "Tench Coxe")],
            vec![
                // 整个值在引文里
                valued(
                    "NVIDIA",
                    "operating_expenses",
                    "$9.2 billion",
                    "expected to be approximately $9.2 billion and $9.0 billion",
                ),
                // 尾巴 shares 在引文别处出现过：原文的单位，不剪
                valued(
                    "Tench Coxe",
                    "votes_for",
                    "15,411,252,412 shares",
                    "Number of shares For | 15,411,252,412",
                ),
                // 规范化过的值，前缀一个词都对不上：不是这条规则管的
                valued("Vega", "amount", "$5 billion", "invested 5 billion dollars"),
                // 表头上的量级与币种：引文那一行只有数，尾巴不带数字，是它的单位，不剪
                valued(
                    "NVIDIA",
                    "net_income",
                    "53,954 million USD",
                    "Net income | $ | 53,954",
                ),
                // 模型换了写法的单位、缩写的头衔：不带数字，不剪
                valued(
                    "SB Energy",
                    "capacity",
                    "10 gigawatts",
                    "at least 10 GW of new generation",
                ),
                valued(
                    "Jensen Huang",
                    "job_title",
                    "founder and CEO",
                    "Jensen Huang, founder and chief executive officer",
                ),
            ],
        );
        let got: Vec<&str> = x.facts.iter().map(value_of).collect();
        assert_eq!(
            got,
            [
                "$9.2 billion",
                "15,411,252,412 shares",
                "$5 billion",
                "53,954 million USD",
                "10 gigawatts",
                "founder and CEO"
            ]
        );
        assert!(n.is_empty(), "{n:?}");
    }

    #[test]
    fn a_dash_is_no_value() {
        let (x, n) = run(
            vec![entity("e1", "NVIDIA")],
            vec![
                valued("NVIDIA", "dividend", "—", "Dividends | —"),
                valued("NVIDIA", "dividend", " – ", "Dividends | –"),
                valued("NVIDIA", "dividend", "$0.01", "Dividends | $0.01"),
            ],
        );
        assert_eq!(x.facts.len(), 1);
        assert_eq!(
            n.iter()
                .filter(|v| matches!(v, Normalization::NoValue { .. }))
                .count(),
            2
        );
    }

    /// Coxe 的形状：`vote_result` 带着数，没有宾语也没有值
    #[test]
    fn figures_on_an_edge_with_no_other_end_land_on_the_subject() {
        let mut f = fact(
            "Tench Coxe",
            "vote_result",
            None,
            "Number of shares For | 15,411,252,412",
        );
        f.qualifiers = Some(quals(&[
            ("for", "15,411,252,412"),
            ("against", "1,399,727,580"),
            ("unit", "shares"),
        ]));
        let (x, n) = run(vec![entity("e1", "Tench Coxe")], vec![f]);
        let mut got: Vec<(String, String)> = x
            .facts
            .iter()
            .map(|f| (f.predicate.clone(), value_of(f).to_string()))
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                (
                    "vote_result.against".to_string(),
                    "1,399,727,580".to_string()
                ),
                ("vote_result.for".to_string(), "15,411,252,412".to_string()),
            ]
        );
        assert_eq!(
            n,
            vec![Normalization::QualifiersWithoutObject {
                predicate: "vote_result".into(),
                values: 2
            }]
        );
    }

    /// 时间做了宾语：边上的数落成值，时间进有效期，那个时间节点不建
    #[test]
    fn a_time_object_becomes_the_validity_and_leaves_no_node() {
        let mut margin = fact(
            "NVIDIA",
            "gross_margin",
            Some("2026-06"),
            "Gross margin | 75.0%",
        );
        margin.object_ref = Some("e7".into());
        margin.qualifiers = Some(quals(&[("percentage", "75.0%")]));
        let mut bare = fact("NVIDIA", "reported_in", Some("2026"), "reported in 2026");
        bare.object_ref = Some("e8".into());
        let (x, n) = run(
            vec![
                entity("e1", "NVIDIA"),
                entity("e7", "2026-06"),
                entity("e8", "2026"),
            ],
            vec![margin, bare],
        );
        assert_eq!(x.facts.len(), 1);
        assert_eq!(x.facts[0].predicate, "gross_margin");
        assert_eq!(value_of(&x.facts[0]), "75.0%");
        assert_eq!(x.facts[0].valid_from.as_deref(), Some("2026-06"));
        let names: Vec<&str> = x.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["NVIDIA"]);
        assert_eq!(
            n.iter()
                .filter(|v| matches!(v, Normalization::OrphanDeclaration { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn a_time_subject_is_not_placed() {
        let f = valued(
            "2026-07-26",
            "revenue",
            "$89.0 billion",
            "revenue was $89.0 billion",
        );
        let (x, n) = run(vec![entity("e1", "NVIDIA")], vec![f]);
        assert!(x.facts.is_empty());
        assert!(matches!(&n[0], Normalization::TimeAsSubject { .. }));
    }

    /// 不是契约格式的期间名（`Q2 FY27`、`第二季度`）不归这里判：那是模型的事，契约 3c 管
    #[test]
    fn a_period_name_that_is_not_a_date_is_not_second_guessed() {
        let mut f = fact(
            "NVIDIA",
            "gross_margin_for_period",
            Some("Q2 FY27"),
            "Gross margin | 75.0 | %",
        );
        f.qualifiers = Some(quals(&[("percentage", "75.0%")]));
        let (x, n) = run(
            vec![entity("e1", "NVIDIA"), entity("e7", "Q2 FY27")],
            vec![f],
        );
        assert_eq!(x.facts.len(), 1);
        assert_eq!(x.entities.len(), 2);
        assert!(n.is_empty());
    }

    /// SB Energy 那句：名字包住了另一个声明实体，同句同主语同谓词已有一条指向它的边
    #[test]
    fn a_description_beside_its_head_goes_with_its_declaration() {
        let quote = "NVIDIA to invest $1.5B in SB Energy now to support SB Energy\u{2019}s growth and commitments to the Ohio community";
        let mut head = fact("NVIDIA", "invested_in", Some("SB Energy"), quote);
        head.object_ref = Some("e2".into());
        let mut desc = fact(
            "NVIDIA",
            "invested_in",
            Some("SB Energy's growth and commitments to the Ohio community"),
            quote,
        );
        desc.object_ref = Some("e6".into());
        let (x, n) = run(
            vec![
                entity("e1", "NVIDIA"),
                entity("e2", "SB Energy"),
                entity(
                    "e6",
                    "SB Energy's growth and commitments to the Ohio community",
                ),
            ],
            vec![head, desc],
        );
        assert_eq!(x.facts.len(), 1);
        assert_eq!(x.facts[0].object.as_deref(), Some("SB Energy"));
        assert!(x
            .entities
            .iter()
            .all(|e| e.local_id.as_deref() != Some("e6")));
        assert!(n.iter().any(
            |v| matches!(v, Normalization::ObjectDescribesDeclared { head, .. } if head == "SB Energy")
        ));
    }

    /// 同样的结构，中文：不靠 's
    #[test]
    fn a_description_is_recognised_without_a_possessive() {
        let quote = "英伟达向星辰能源投资15亿美元，支持星辰能源在俄亥俄的发展";
        let (x, _) = run(
            vec![
                entity("e1", "英伟达"),
                entity("e2", "星辰能源"),
                entity("e3", "星辰能源在俄亥俄的发展"),
            ],
            vec![
                fact("英伟达", "投资", Some("星辰能源"), quote),
                fact("英伟达", "投资", Some("星辰能源在俄亥俄的发展"), quote),
            ],
        );
        assert_eq!(x.facts.len(), 1);
        assert_eq!(x.entities.len(), 2);
    }

    /// 包住了别的名字、但旁边没有指向本尊的同一条边：可能真是一个东西，不动
    #[test]
    fn a_name_that_contains_another_without_a_sibling_edge_is_left_alone() {
        let quote = "Sam Altman was removed by OpenAI's board of directors";
        let (x, n) = run(
            vec![
                entity("e1", "Sam Altman"),
                entity("e2", "OpenAI"),
                entity("e3", "OpenAI's board of directors"),
            ],
            vec![fact(
                "Sam Altman",
                "removed_by",
                Some("OpenAI's board of directors"),
                quote,
            )],
        );
        assert_eq!(x.facts.len(), 1);
        assert_eq!(x.entities.len(), 3);
        assert!(n.is_empty());
    }
}
