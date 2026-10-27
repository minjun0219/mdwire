//! 슬랙 레거시 `mrkdwn` 출력을 텔레그램 HTML 로 다시 쓴다 — 채점은 그 모양으로 한다.
//!
//! 두 채널은 구조가 같다. 헤딩은 굵은 줄, 표는 고정폭 블록, 불릿은 `•`, 글자는 엔티티다. 다른 것은
//! 강조와 링크를 적는 문법뿐이라, 그 둘만 태그로 바꾸면 텔레그램의 불변식(태그 균형, 강조 범위, 내용
//! 손실, 표 폭)을 그대로 쓸 수 있다.
//!
//! **코어와 따로 쓴 읽기다.** 마커는 단어 경계에서만 연다(앞이 공백·구두점·U+200B 이거나 줄 첫머리,
//! 뒤가 공백이 아님) — `mrkdwn` 이 읽는다고 알려진 규칙이고, 코어가 끼운 U+200B 가 그 경계를 만드는지가
//! 여기서 드러난다. 짝은 줄 안에서만 찾는다. 짝 없는 마커와 둘 이상 붙은 마커(`**`)는 글자로 남는다.
//! 슬랙에서 잰 규칙이 아니다 — 재면 이 읽기를 거기에 맞춘다.

use crate::check::PART_SEPARATOR;

const ZERO_WIDTH_SPACE: char = '\u{200B}';

/// 출력 전체를 바꾼다. 조각 구분자는 그대로 둔다.
pub fn as_telegram_html(output: &str) -> String {
    let mut out = String::with_capacity(output.len() + 64);
    for (i, part) in output.split(PART_SEPARATOR).enumerate() {
        if i > 0 {
            out.push(PART_SEPARATOR);
        }
        part_as_html(part, &mut out);
    }
    out
}

fn part_as_html(part: &str, out: &mut String) {
    let mut fence = false;
    let mut quote = false;
    let lines: Vec<&str> = part.split('\n').collect();
    for (n, line) in lines.iter().enumerate() {
        if n > 0 {
            out.push('\n');
        }
        if line.trim_start().starts_with("```") {
            out.push_str(if fence { "</pre>" } else { "<pre>" });
            fence = !fence;
            out.push_str(&line.trim_start()[3..]);
            continue;
        }
        if fence {
            out.push_str(line);
            continue;
        }
        let body = match line.strip_prefix("> ").or_else(|| (*line == ">").then_some("")) {
            Some(rest) => {
                if !quote {
                    out.push_str("<blockquote>");
                    quote = true;
                }
                rest
            }
            None => {
                if quote {
                    // 앞 줄 끝에 닫는다 — 줄바꿈은 이미 들어갔다.
                    out.pop();
                    out.push_str("</blockquote>\n");
                    quote = false;
                }
                line
            }
        };
        inline_as_html(body, out);
    }
    if fence {
        out.push_str("</pre>");
    }
    if quote {
        out.push_str("</blockquote>");
    }
}

/// 한 줄의 강조·코드·링크를 태그로 바꾼다.
fn inline_as_html(line: &str, out: &mut String) {
    let ch: Vec<char> = line.chars().collect();
    // 먼저 짝을 정한다 — 여는 자리마다 같은 줄의 닫는 자리. 코드 스팬 안은 보지 않는다.
    let mut tag: Vec<Option<&'static str>> = vec![None; ch.len()];
    let mut open: Vec<(char, usize)> = Vec::new();
    // 링크(`<…>`)는 한 덩어리다 — 밖의 백틱이 링크 글 안의 백틱과 코드 스팬을 맺지 않는다.
    let mut in_link = vec![false; ch.len()];
    let mut k = 0;
    while k < ch.len() {
        if ch[k] == '<' {
            if let Some(end) = ch[k..].iter().position(|&x| x == '>') {
                for f in &mut in_link[k..=k + end] {
                    *f = true;
                }
                k += end + 1;
                continue;
            }
        }
        k += 1;
    }
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if c == '`' {
            let run = ch[i..].iter().take_while(|&&x| x == '`').count();
            let close = (i + run..ch.len()).take_while(|&j| !in_link[j]).find(|&j| {
                ch[j] == '`'
                    && ch[j..].iter().take_while(|&&x| x == '`').count() == run
                    && (j == 0 || ch[j - 1] != '`')
            });
            match close {
                Some(j) => {
                    tag[i] = Some("<code>");
                    for t in &mut tag[i + 1..i + run] {
                        *t = Some("");
                    }
                    tag[j] = Some("</code>");
                    for t in &mut tag[j + 1..j + run] {
                        *t = Some("");
                    }
                    i = j + run;
                }
                None => i += run,
            }
            continue;
        }
        if c == '<' {
            if let Some(end) = ch[i..].iter().position(|&x| x == '>') {
                i += end + 1;
                continue;
            }
        }
        let name = match c {
            '*' => "b",
            '_' => "i",
            '~' => "s",
            _ => {
                i += 1;
                continue;
            }
        };
        // 같은 마커가 둘 이상 붙은 런(`2**(n-1)`)은 글자로 본다. `mrkdwn` 의 마커는 홑이라 코어는 런을
        // 내지 않는다 — 출력에 남은 런은 저자가 글자로 쓴 것이다.
        let run = ch[i..].iter().take_while(|&&x| x == c).count();
        if run > 1 {
            i += run;
            continue;
        }
        let prev = if i > 0 { Some(ch[i - 1]) } else { None };
        let next = ch.get(i + 1).copied();
        let edge = |x: Option<char>| x.is_none_or(|x| !x.is_alphanumeric() || x == ZERO_WIDTH_SPACE);
        let closes = prev.is_some_and(|p| !p.is_whitespace()) && edge(next);
        let opens = edge(prev) && next.is_some_and(|n| !n.is_whitespace() && n != c);
        if closes {
            if let Some(at) = open.iter().rposition(|&(m, _)| m == c) {
                let (_, start) = open[at];
                open.truncate(at);
                tag[start] = Some(match name {
                    "b" => "<b>",
                    "i" => "<i>",
                    _ => "<s>",
                });
                tag[i] = Some(match name {
                    "b" => "</b>",
                    "i" => "</i>",
                    _ => "</s>",
                });
                i += 1;
                continue;
            }
        }
        if opens {
            open.push((c, i));
        }
        i += 1;
    }

    let mut i = 0;
    let mut code = false;
    while i < ch.len() {
        if let Some(t) = tag[i] {
            out.push_str(t);
            if t == "<code>" {
                code = true;
            } else if t == "</code>" {
                code = false;
            }
            i += 1;
            continue;
        }
        let c = ch[i];
        if c == ZERO_WIDTH_SPACE {
            i += 1;
            continue;
        }
        // 코드 스팬 안의 백틱은 코어가 U+02CB 로 쓴다 — 원문 글자로 돌려 견준다.
        if code && c == '\u{02CB}' {
            out.push('`');
            i += 1;
            continue;
        }
        if c == '<' && !code {
            if let Some(end) = ch[i..].iter().position(|&x| x == '>') {
                let body: String = ch[i + 1..i + end].iter().collect();
                let (url, text) = body.split_once('|').unwrap_or((&body, &body));
                out.push_str("<a href=\"");
                out.push_str(&url.replace('"', "&quot;"));
                out.push_str("\">");
                // 글 안의 강조와 코드도 읽는다 — 코어는 링크 글을 본문처럼 렌더한다.
                inline_as_html(text, out);
                out.push_str("</a>");
                i += end + 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_markers_at_word_boundaries() {
        assert_eq!(as_telegram_html("*굵게* _기울임_ ~취소~"), "<b>굵게</b> <i>기울임</i> <s>취소</s>");
        assert_eq!(as_telegram_html("*마통*\u{200b}이"), "<b>마통</b>이");
        // 붙은 글자가 있으면 마커가 아니다 — 코어가 U+200B 를 빠뜨리면 여기서 글자로 남는다.
        assert_eq!(as_telegram_html("*마통*이 snake_case_name 5~6월"), "*마통*이 snake_case_name 5~6월");
        assert_eq!(as_telegram_html("`a*b*c` <https://a.test|문서>"), "<code>a*b*c</code> <a href=\"https://a.test\">문서</a>");
    }

    #[test]
    fn quotes_and_fences_become_blocks() {
        assert_eq!(as_telegram_html("> 인용\n> 둘\n본문"), "<blockquote>인용\n둘</blockquote>\n본문");
        assert_eq!(as_telegram_html("```\n*a*\n```"), "<pre>\n*a*\n</pre>");
    }
}
