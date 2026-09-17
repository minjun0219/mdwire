//! 구현 중립 채점 — **기대 출력 없이도 재는 불변식**.
//!
//! 코퍼스 대조만으로는 자기채점이 된다. 기대 출력을 우리가 만들기 때문이다.
//! 여기 있는 규칙은 어느 구현의 출력에나 똑같이 적용된다. 그래서 다른 언어로 된 구현도
//! (`adapter::Command` 로 붙여서) 같은 잣대에 올릴 수 있고, 코퍼스에 없는 실제 문서로도
//! 잴 수 있다.
//!
//! 규칙의 우선순위는 실측이 정했다. **1순위는 강조 범위**다 — 실제 고장은 "강조가 안
//! 먹는 것"이 아니라 범위가 뒤집히는 것이었고, 그건 채널이 200 을 주기 때문에 아무도
//! 모르게 몇 달을 간다(`DESIGN.md`).

use crate::emphasis::{self, Mode, Span};
use mdwire_core::{width::str_width, Channel};

/// 불변식 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// 입력에서 강조였던 구간이 출력에서도 같은 구간인가. **가장 중요한 규칙.**
    EmphasisRange,
    /// 변환되지 않은 마크다운 마커가 출력에 남았는가.
    StrayMarker,
    /// 연 태그를 닫았는가.
    UnclosedTag,
    /// 채널이 받지 않는 태그를 썼는가.
    DisallowedTag,
    /// 이스케이프되지 않은 `<`·`&` 가 있는가.
    RawHtmlChar,
    /// 조각 하나가 채널 한도를 넘는가.
    OverLimit,
    /// 고정폭 표의 열이 표시 폭 기준으로 맞는가.
    TableMisaligned,
    /// 입력이 비지 않았는데 출력이 비었는가.
    EmptyOutput,
}

impl Rule {
    pub fn label(self) -> &'static str {
        match self {
            Rule::EmphasisRange => "강조 범위",
            Rule::StrayMarker => "남은 마커",
            Rule::UnclosedTag => "안 닫힌 태그",
            Rule::DisallowedTag => "허용 안 되는 태그",
            Rule::RawHtmlChar => "이스케이프 누락",
            Rule::OverLimit => "한도 초과",
            Rule::TableMisaligned => "표 열 어긋남",
            Rule::EmptyOutput => "빈 출력",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: Rule,
    pub detail: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.rule.label(), self.detail)
    }
}

/// 텔레그램이 받는 태그. `SPEC.md` 4절의 9개에, 다른 구현이 쓸 법한 동의어를 더했다 —
/// 우리 출력만 재는 것이 아니기 때문이다.
const TELEGRAM_TAGS: &[&str] = &[
    "b", "strong", "i", "em", "u", "ins", "s", "strike", "del", "code", "pre", "a", "blockquote",
    "tg-spoiler", "span",
];

/// 출력 조각 구분자. CLI 와 같은 규약이다(`SPEC.md` 5절).
pub const PART_SEPARATOR: char = '\0';

/// 입력과 출력을 견줘 불변식 위반을 모은다.
pub fn check(input: &str, output: &str, channel: Channel) -> Vec<Finding> {
    let mut findings = Vec::new();

    if !input.trim().is_empty() && output.trim().is_empty() {
        findings.push(Finding { rule: Rule::EmptyOutput, detail: "입력이 있는데 출력이 비었다".into() });
        return findings;
    }

    over_limit(output, channel, &mut findings);
    emphasis_range(input, output, channel, &mut findings);
    stray_markers(output, channel, &mut findings);
    if matches!(channel, Channel::TelegramHtml) {
        html_tags(output, &mut findings);
    }
    tables(output, channel, &mut findings);
    findings
}

/// 조각 하나하나가 한도 안인가. 분할이 있는 이유 자체다.
fn over_limit(output: &str, channel: Channel, out: &mut Vec<Finding>) {
    for (i, part) in output.split(PART_SEPARATOR).enumerate() {
        let n = part.chars().count();
        if n > channel.limit() {
            out.push(Finding {
                rule: Rule::OverLimit,
                detail: format!("{}번 조각이 {}자 — 한도 {}자", i + 1, n, channel.limit()),
            });
        }
    }
}

/// **핵심 규칙.** 입력에서 강조였던 구간이 출력에서도 같은 구간으로 있는가.
///
/// 입력 쪽 기준은 `emphasis` 의 참조 짝짓기다. 코어와 따로 쓴 구현이라,
/// 둘이 같은 답을 내는 것 자체가 검증이 된다.
fn emphasis_range(input: &str, output: &str, channel: Channel, out: &mut Vec<Finding>) {
    if channel == Channel::Plain {
        // 마크업을 지우는 채널이라 잴 것이 없다. 여기서 억지로 재면 규칙이 거짓말을 한다.
        return;
    }
    let want = emphasis::scan_markdown(input, Mode::Repair).spans;
    if want.is_empty() {
        return;
    }
    let plain = output.replace(PART_SEPARATOR, "\n");
    let got = match channel {
        Channel::TelegramHtml => emphasis::scan_html(&plain).spans,
        _ => emphasis::scan_markdown(&plain, Mode::Strict).spans,
    };

    let mut pool: Vec<&Span> = got.iter().collect();
    for w in &want {
        match pool.iter().position(|g| g.kind == w.kind && g.text == w.text) {
            Some(at) => {
                pool.remove(at);
            }
            None => {
                let near = pool
                    .iter()
                    .filter(|g| g.kind == w.kind)
                    .map(|g| g.text.as_str())
                    .max_by_key(|t| common_prefix(t, &w.text));
                out.push(Finding {
                    rule: Rule::EmphasisRange,
                    detail: match near {
                        Some(n) => format!(
                            "{} 범위가 다르다\n      원문: {}\n      출력: {}",
                            w.kind.label(),
                            w.text,
                            n
                        ),
                        None => format!("{} 범위가 출력에 없다: {}", w.kind.label(), w.text),
                    },
                });
            }
        }
    }
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// 변환되지 않은 마커가 남았는가.
///
/// **`**` 가 출력에 남으면 그건 실패다.** 정상이라면 전부 태그나 채널 문법으로 바뀌었어야
/// 한다. 원본 고장이 몇 달을 간 이유가 정확히 "틀려도 채널이 200 을 준다"였고,
/// 그래서 이 불변식을 테스트에 박아 둔다.
fn stray_markers(output: &str, channel: Channel, out: &mut Vec<Finding>) {
    match channel {
        Channel::SlackMarkdown | Channel::SlackMrkdwn => {
            // 마크다운을 그대로 내보내는 채널이라 마커가 남는 것이 정상이다.
            // 대신 **짝이 맞아야** 한다.
            let scan = emphasis::scan_markdown(&output.replace(PART_SEPARATOR, "\n"), Mode::Strict);
            for u in scan.unpaired {
                out.push(Finding {
                    rule: Rule::StrayMarker,
                    detail: format!("짝 없는 `{}` — …{}…", u.marker, u.context),
                });
            }
        }
        _ => {
            let text = strip_verbatim(&output.replace(PART_SEPARATOR, "\n"), channel);
            let ch: Vec<char> = text.chars().collect();
            let mut i = 0;
            while i < ch.len() {
                let c = ch[i];
                if !matches!(c, '*' | '_' | '~') {
                    i += 1;
                    continue;
                }
                let run = emphasis::run_len(&ch, i, c);
                if run >= 2 {
                    let prev = if i > 0 { Some(ch[i - 1]) } else { None };
                    let next = ch.get(i + run).copied();
                    let (left, right) = emphasis::flanking(prev, next);
                    if left || right {
                        let from = i.saturating_sub(10);
                        let to = (i + run + 10).min(ch.len());
                        out.push(Finding {
                            rule: Rule::StrayMarker,
                            detail: format!(
                                "변환되지 않은 `{}` — …{}…",
                                ch[i..i + run].iter().collect::<String>(),
                                ch[from..to].iter().collect::<String>().replace('\n', "⏎")
                            ),
                        });
                    }
                }
                i += run;
            }
        }
    }
}

/// 코드·고정폭 구간을 들어낸다. 그 안의 `*` 는 글자이지 마커가 아니다.
fn strip_verbatim(text: &str, channel: Channel) -> String {
    if channel != Channel::TelegramHtml {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<pre").or_else(|| rest.find("<code")) {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let close = if tail.starts_with("<pre") { "</pre>" } else { "</code>" };
        match tail.find(close) {
            Some(end) => rest = &tail[end + close.len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// 연 태그를 닫았는가, 채널이 받는 태그만 썼는가, 글자로서의 `<`·`&` 를 이스케이프했는가.
fn html_tags(output: &str, out: &mut Vec<Finding>) {
    let ch: Vec<char> = output.chars().collect();
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        match ch[i] {
            '<' => match emphasis::parse_tag(&ch, i) {
                Some((name, closing, end)) => {
                    if !TELEGRAM_TAGS.contains(&name.as_str()) {
                        out.push(Finding {
                            rule: Rule::DisallowedTag,
                            detail: format!("채널이 받지 않는 태그: <{name}>"),
                        });
                    }
                    if closing {
                        match stack.iter().rposition(|n| *n == name) {
                            Some(at) => {
                                for n in stack.drain(at..).skip(1) {
                                    out.push(Finding {
                                        rule: Rule::UnclosedTag,
                                        detail: format!("<{n}> 이 </{name}> 안에서 닫히지 않았다"),
                                    });
                                }
                            }
                            None => out.push(Finding {
                                rule: Rule::UnclosedTag,
                                detail: format!("열린 적 없는 </{name}>"),
                            }),
                        }
                    } else {
                        stack.push(name);
                    }
                    i = end;
                }
                None => {
                    out.push(Finding {
                        rule: Rule::RawHtmlChar,
                        detail: format!("이스케이프되지 않은 `<` — …{}…", context(&ch, i)),
                    });
                    i += 1;
                }
            },
            '&' => {
                if let Some((_, end)) = emphasis::parse_entity(&ch, i) {
                    i = end;
                } else {
                    out.push(Finding {
                        rule: Rule::RawHtmlChar,
                        detail: format!("이스케이프되지 않은 `&` — …{}…", context(&ch, i)),
                    });
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    for n in stack {
        out.push(Finding { rule: Rule::UnclosedTag, detail: format!("<{n}> 이 끝까지 안 닫혔다") });
    }
}

fn context(ch: &[char], at: usize) -> String {
    let from = at.saturating_sub(10);
    let to = (at + 12).min(ch.len());
    ch[from..to].iter().collect::<String>().replace('\n', "⏎")
}

/// 고정폭 표의 열이 **표시 폭** 기준으로 맞는가.
///
/// 문자 수로 맞춘 구현은 한글이 든 표에서 반드시 어긋난다. 이 규칙이 그것을 잡는다.
fn tables(output: &str, channel: Channel, out: &mut Vec<Finding>) {
    let text = unescape_for_width(&output.replace(PART_SEPARATOR, "\n"), channel);
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let pipes = lines[i].matches('|').count();
        if pipes == 0 {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < lines.len() && lines[j].matches('|').count() == pipes {
            j += 1;
        }
        let block = &lines[i..j];
        // 표로 보려면 구분선이 있어야 한다. 없으면 `|` 가 든 산문일 뿐이다.
        let looks_like_table = block.len() >= 3 && block[1..2].iter().any(|l| is_delimiter(l));
        if looks_like_table {
            check_columns(block, out);
        }
        i = j.max(i + 1);
    }
}

fn is_delimiter(line: &str) -> bool {
    let t = line.trim();
    t.contains('-') && t.chars().all(|c| matches!(c, '-' | ':' | '|' | '+' | ' ' | '\t'))
}

fn check_columns(block: &[&str], out: &mut Vec<Finding>) {
    let rows: Vec<Vec<usize>> = block
        .iter()
        .map(|l| l.split('|').map(str_width).collect())
        .collect();
    let cols = rows[0].len();
    for c in 0..cols {
        let first = rows[0][c];
        for (r, row) in rows.iter().enumerate().skip(1) {
            if row[c] != first {
                out.push(Finding {
                    rule: Rule::TableMisaligned,
                    detail: format!(
                        "{}번 열의 표시 폭이 다르다 — 머리글 {}칸, {}번 줄 {}칸\n      {}\n      {}",
                        c, first, r + 1, row[c], block[0].trim_end(), block[r].trim_end()
                    ),
                });
                return;
            }
        }
    }
}

/// 폭을 재기 전에 엔티티를 되돌린다. `&amp;` 는 화면에서 한 칸이지 다섯 칸이 아니다.
fn unescape_for_width(text: &str, channel: Channel) -> String {
    if channel != Channel::TelegramHtml {
        return text.to_string();
    }
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVERTED_INPUT: &str = "공개 채널**이다. 글 내용이 아니라\n**신분 공개 + 시점의 조합**이 판단 대상";

    /// 원본 고장 그대로. **채점기가 이걸 못 잡으면 나머지는 의미가 없다.**
    #[test]
    fn catches_the_inverted_emphasis_range() {
        let broken = "공개 채널<b>이다. 글 내용이 아니라\n</b>신분 공개 + 시점의 조합**이 판단 대상";
        let f = check(INVERTED_INPUT, broken, Channel::TelegramHtml);
        assert!(f.iter().any(|x| x.rule == Rule::EmphasisRange), "{f:?}");
        assert!(f.iter().any(|x| x.rule == Rule::StrayMarker), "{f:?}");
    }

    #[test]
    fn accepts_the_repaired_output() {
        let good = "공개 채널<b>이다. 글 내용이 아니라\n신분 공개 + 시점의 조합</b>이 판단 대상";
        let f = check(INVERTED_INPUT, good, Channel::TelegramHtml);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn catches_unclosed_and_disallowed_tags() {
        let f = check("가", "<b>가", Channel::TelegramHtml);
        assert!(f.iter().any(|x| x.rule == Rule::UnclosedTag), "{f:?}");
        let f = check("가", "<h1>가</h1>", Channel::TelegramHtml);
        assert!(f.iter().any(|x| x.rule == Rule::DisallowedTag), "{f:?}");
    }

    #[test]
    fn catches_raw_angle_bracket() {
        let f = check("a < b", "a < b", Channel::TelegramHtml);
        assert!(f.iter().any(|x| x.rule == Rule::RawHtmlChar), "{f:?}");
    }

    #[test]
    fn catches_over_limit() {
        let long = "가".repeat(5000);
        let f = check(&long, &long, Channel::TelegramHtml);
        assert!(f.iter().any(|x| x.rule == Rule::OverLimit), "{f:?}");
        // NUL 로 나뉘어 있으면 조각마다 잰다.
        let split = format!("{}\0{}", "가".repeat(2500), "가".repeat(2500));
        let f = check(&long, &split, Channel::TelegramHtml);
        assert!(!f.iter().any(|x| x.rule == Rule::OverLimit), "{f:?}");
    }

    /// 문자 수로 맞춘 표. 한글 열이 밀린다.
    #[test]
    fn catches_table_aligned_by_char_count() {
        let by_chars = "환경   | 재현\n------ | ----\n스테이징 | 예\n로컬   | 아니오\n";
        let f = check("", by_chars, Channel::Plain);
        assert!(f.iter().any(|x| x.rule == Rule::TableMisaligned), "{f:?}");

        let by_width = "환경       | 재현  \n---------- | ------\n스테이징   | 예    \n로컬       | 아니오\n";
        let f = check("", by_width, Channel::Plain);
        assert!(!f.iter().any(|x| x.rule == Rule::TableMisaligned), "{f:?}");
    }

    #[test]
    fn prose_with_a_pipe_is_not_a_table() {
        let f = check("", "a | b 는 표가 아니다\n다음 줄 | 도\n또 | 한", Channel::Plain);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn slack_keeps_markers_but_they_must_pair() {
        let f = check("**굵게** 다", "**굵게** 다", Channel::SlackMarkdown);
        assert!(f.is_empty(), "{f:?}");
        let f = check("**굵게** 다", "**굵게 다", Channel::SlackMarkdown);
        assert!(f.iter().any(|x| x.rule == Rule::StrayMarker), "{f:?}");
    }

    #[test]
    fn empty_output_for_nonempty_input_is_a_failure() {
        let f = check("내용이 있다", "   ", Channel::Plain);
        assert!(f.iter().any(|x| x.rule == Rule::EmptyOutput), "{f:?}");
    }
}
