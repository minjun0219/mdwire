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
use mdwire::{width::str_width, Channel};

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

/// 닫는 짝이 없는 HTML 요소. 열린 채로 두는 것이 정상이다.
///
/// 텔레그램은 이것들도 안 받으므로 "허용 안 되는 태그"로는 걸린다. 다만 그걸 다시
/// "안 닫혔다"로 세면 같은 사실을 두 번 신고하는 것이고, `<br>` 을 내보내는 구현이
/// 실제보다 나빠 보인다.
const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
    "source", "track", "wbr",
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
    stray_markers(input, output, channel, &mut findings);
    if matches!(channel, Channel::TelegramHtml) {
        html_tags(output, &mut findings);
    }
    tables(input, output, channel, &mut findings);
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
                // **어느 쪽으로 어긋났는지**가 진단의 전부다. 비슷한 것을 보여 주는 것으로는
                // 출력이 더 넓게 잡았는지 좁게 잡았는지 알 수 없고, 그 둘은 원인이 다르다.
                let wider = pool.iter().find(|g| g.kind == w.kind && g.text.contains(&w.text));
                let narrower = pool.iter().find(|g| g.kind == w.kind && w.text.contains(&g.text));
                let detail = match (wider, narrower) {
                    (Some(g), _) => format!(
                        "{} 범위가 원문보다 넓다\n      원문: {}\n      출력: {}",
                        w.kind.label(),
                        w.text,
                        g.text
                    ),
                    (None, Some(g)) => format!(
                        "{} 범위가 원문보다 좁다\n      원문: {}\n      출력: {}",
                        w.kind.label(),
                        w.text,
                        g.text
                    ),
                    (None, None) => {
                        format!("{} 범위가 출력에 아예 없다: {}", w.kind.label(), w.text)
                    }
                };
                out.push(Finding { rule: Rule::EmphasisRange, detail });
            }
        }
    }
}

/// 변환되지 않은 마커가 남았는가.
///
/// **`**` 가 출력에 남으면 그건 실패다.** 정상이라면 전부 태그나 채널 문법으로 바뀌었어야
/// 한다. 원본 고장이 몇 달을 간 이유가 정확히 "틀려도 채널이 200 을 준다"였고,
/// 그래서 이 불변식을 테스트에 박아 둔다.
fn stray_markers(input: &str, output: &str, channel: Channel, out: &mut Vec<Finding>) {
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
            let text = strip_verbatim(&output.replace(PART_SEPARATOR, "\n"), channel, input);
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
                    // 식별자 안의 밑줄은 강조가 아니다 — `GUID-1234__GUID-5678` 처럼
                    // 글자 사이에 낀 `__` 는 파서도 글자로 둔다. 같은 예외를 여기도 둔다.
                    if c == '_'
                        && prev.is_some_and(char::is_alphanumeric)
                        && next.is_some_and(char::is_alphanumeric)
                    {
                        i += run;
                        continue;
                    }
                    // 강조가 될 수 있었던 자리인가 — 열 수 있거나, 앞이 공백이 아니어서
                    // 닫는 자리일 수 있거나. `2 ** 3` 처럼 양쪽이 공백인 것은 글자다.
                    let could_be_emphasis = emphasis::can_open(prev, next)
                        || !prev.is_none_or(char::is_whitespace);
                    if could_be_emphasis {
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
///
/// Plain 채널은 코드 표시가 출력에 안 남으므로(마크업을 지우는 것이 일이다)
/// **입력에서 코드였던 덩어리를 찾아 지운다.** `wiki/**/*.md` 같은 glob 이
/// 코드 스팬 안에 들어 있는 문서가 실제로 있다.
fn strip_verbatim(text: &str, channel: Channel, input: &str) -> String {
    if channel == Channel::Plain {
        let mut out = text.to_string();
        for chunk in verbatim_chunks(input) {
            if chunk.contains('*') || chunk.contains('_') || chunk.contains('~') {
                out = out.replace(&chunk, " ");
            }
        }
        return out;
    }
    if channel != Channel::TelegramHtml {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // **먼저 나오는 쪽을 집는다.** `<pre` 를 우선하면 그 앞에 있던 `<code>` 구간이
    // 통째로 살아남는다 — 그 안의 `packages/**` 같은 글로브가 남은 마커로 신고된다.
    while let Some(start) = [rest.find("<pre"), rest.find("<code")].into_iter().flatten().min() {
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

/// 입력에서 코드였던 덩어리들. 코드펜스 본문과 인라인 코드 스팬.
fn verbatim_chunks(input: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut fence = false;
    for line in input.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            chunks.push(line.to_string());
            continue;
        }
        let mut rest = line;
        while let Some(start) = rest.find('`') {
            let after = &rest[start + 1..];
            match after.find('`') {
                Some(end) => {
                    chunks.push(after[..end].to_string());
                    rest = &after[end + 1..];
                }
                None => break,
            }
        }
    }
    chunks
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
                    } else if !VOID_TAGS.contains(&name.as_str()) {
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
fn tables(input: &str, output: &str, channel: Channel, out: &mut Vec<Finding>) {
    // **코드 블록 안의 표는 표가 아니다.** 마크다운 표를 코드로 보여 주는 문서가 있고,
    // 그건 원문 그대로 나가는 것이 맞다. 여기를 안 걸러내면 정상 통과를 고장으로 신고한다.
    // 입력에서 코드였던 줄을 기억해 두었다가, 그 줄로 시작하는 덩어리는 건너뛴다.
    let verbatim: std::collections::HashSet<String> =
        verbatim_chunks(input).into_iter().map(|c| c.trim().to_string()).collect();
    // **조각마다 따로 본다.** 조각은 각각 독립된 메시지다. 이어 붙여 놓고 보면
    // 조각 경계에서 만난 표 둘이 한 덩어리가 되어, 서로 다른 표의 열 너비를 견주게 된다.
    for part in output.split(PART_SEPARATOR) {
        tables_in_part(part, channel, &verbatim, out);
    }
}

fn tables_in_part(
    part: &str,
    channel: Channel,
    verbatim: &std::collections::HashSet<String>,
    out: &mut Vec<Finding>,
) {
    let text = unescape_for_width(part, channel);
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
        let looks_like_table = block.len() >= 3
            && block[1..2].iter().any(|l| is_delimiter(l))
            && !verbatim.contains(block[0].trim());
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
    // 줄 끝 공백은 재지 않는다. 마지막 열의 오른쪽 여백은 화면에 아무 일도 하지 않으므로
    // 구현마다 붙이기도 안 붙이기도 한다 — 그걸 어긋남으로 볼 이유가 없다.
    let rows: Vec<Vec<usize>> = block
        .iter()
        .map(|l| l.trim_end().split('|').map(str_width).collect())
        .collect();
    let cols = rows.iter().map(Vec::len).min().unwrap_or(0);
    for c in 0..cols.saturating_sub(1) {
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

/// 폭을 재기 전에 화면에 안 보이는 것을 걷어낸다.
///
/// `&amp;` 는 화면에서 한 칸이지 다섯 칸이 아니고, `<pre>` 는 아예 0 칸이다.
/// 이걸 안 하면 표 첫 줄만 태그 길이만큼 넓게 재서, 맞는 표를 어긋났다고 신고한다.
fn unescape_for_width(text: &str, channel: Channel) -> String {
    if channel != Channel::TelegramHtml {
        return text.to_string();
    }
    let ch: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '<' {
            if let Some((_, _, end)) = emphasis::parse_tag(&ch, i) {
                i = end;
                continue;
            }
        }
        if ch[i] == '&' {
            if let Some((c, end)) = emphasis::parse_entity(&ch, i) {
                out.push(c);
                i = end;
                continue;
            }
        }
        out.push(ch[i]);
        i += 1;
    }
    out
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
