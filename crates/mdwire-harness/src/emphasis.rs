//! 강조 범위 추출 — **채점의 핵심**.
//!
//! 실제로 겪은 고장은 "강조가 안 먹는 것"이 아니라 **범위가 뒤집히는 것**이었다
//! (`DESIGN.md`, 표본의 47%). 그러니 채점기는 "출력이 예쁜가"가 아니라
//! **입력에서 강조였던 구간이 출력에서도 같은 구간인가**를 물어야 한다.
//!
//! 이 모듈은 코어 파서와 **따로 쓴 참조 구현**이다. 배치형이고 느리며, 스트리밍을
//! 모른다. 일부러 그렇게 뒀다 — 같은 코드를 공유하면 같은 버그를 공유하고,
//! 그러면 채점이 자기채점이 된다. 두 경로가 독립으로 같은 답을 내는 것이 검증이다.
//!
//! 여기 적힌 짝짓기 규칙이 **정본**이다. 코어는 이 규칙을 한 번 순회로 구현한 것이다.

/// 강조의 종류. 채널마다 표기는 달라도 의미는 같다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Bold,
    Italic,
    Strike,
    Code,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Bold => "굵게",
            Kind::Italic => "기울임",
            Kind::Strike => "취소선",
            Kind::Code => "코드",
        }
    }
}

/// 강조 한 덩어리. `text` 는 마크업을 뺀 본문을 공백 정규화한 것이다.
///
/// 공백을 정규화하는 이유는 **구현 중립성**이다. 줄 넘는 강조를 두고 텔레그램은
/// 줄바꿈을 남기고 다른 구현은 공백으로 접을 수 있다. 그건 이 채점기가 따질 일이 아니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub kind: Kind,
    pub text: String,
}

/// 짝을 못 맞춘 마커. 출력을 엄격 모드로 훑을 때 나온다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpaired {
    pub marker: String,
    /// 앞뒤 맥락. 어디가 문제인지 사람이 읽으라고 남긴다.
    pub context: String,
}

#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub spans: Vec<Span>,
    pub unpaired: Vec<Unpaired>,
    /// 마커 글자별로 **글자로 남긴** 개수. 참조 모델도 강조로 읽지 않은 별표(마스킹 번호
    /// `4***-****`)는 출력에 글자로 남아도 결함이 아니다 — 남은 마커 검사가 예산으로 쓴다.
    pub literal: std::collections::HashMap<char, usize>,
    /// `spans` 와 같은 순서로, 공백을 접기 전의 범위 글. 줄바꿈이 남아 있다 — 강조를 줄마다 닫는
    /// 채널(노션)은 원문 범위를 줄에서 나눠 재야 해서 둔다.
    pub raw: Vec<String>,
}

/// 짝짓기 모드.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 입력용. 깨진 마크다운을 우리 복구 규칙대로 읽는다 — 이것이 "기대되는 강조 범위"다.
    Repair,
    /// 출력용. 복구하지 않는다. 짝이 안 맞으면 그대로 고발한다.
    Strict,
    /// 출력용 — `Strict` 에 CommonMark 의 **닫기 판정(우측 flanking)** 을 더한다. 출력을 CommonMark 로
    /// 다시 읽는 채널(슬랙 `markdown_text`)에 쓴다. 닫는 마커 앞이 구두점이고 뒤가 글자면 그 채널은 닫지
    /// 않으므로, 우리 복구 규칙으로는 짝이 맞아도 채널 화면에는 별표가 남는다.
    CommonMark,
}

/// 마크다운 문자열에서 강조 범위를 뽑는다.
pub fn scan_markdown(src: &str, mode: Mode) -> Scan {
    let mut scan = Scan::default();
    for block in blocks(src) {
        scan_block(&block, mode, &mut scan);
    }
    scan
}

/// 공백을 하나로 접고 양끝을 자른다. 비교는 언제나 이 모양으로 한다.
pub fn normalize_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        // 폭 없는 공백은 **지운다.** 공백으로 접으면 `굵은 것`+ZWSP+`이` 가
        // `굵은 것 이` 가 되어, CJK 패딩을 넣은 출력이 입력과 안 맞는 것으로 나온다.
        // 워드 조이너(U+2060)도 같다 — 슬랙에서 못 읽히는 마커 안쪽에 끼운다.
        if matches!(c, '\u{200b}' | '\u{2060}') {
            continue;
        }
        if c.is_whitespace() {
            space = !out.is_empty();
        } else {
            if space {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

/// 블록으로 쪼개고 블록 마커를 벗긴다.
///
/// 코드펜스와 표는 통째로 건너뛴다. **출력에서 강조가 사라지는 것이 정상인 자리**이기
/// 때문이다 — 코드 안은 원래 강조가 아니고, 표는 고정폭 블록으로 나가면서 마크업을 잃는다
/// (`SPEC.md` 7절). 여기를 안 걸러내면 정상 동작을 고장으로 신고하게 된다.
fn blocks(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut fence: Option<(char, usize)> = None;
    // 표는 **상태로 따라간다.** 구분선 옆줄만 보면 셋째 줄부터는 표인 줄 모르고
    // 셀 안의 강조를 산문으로 읽는다 — 그러면 정상 출력이 통째로 고장으로 신고된다.
    let mut in_table = false;
    let mut in_quote = false;
    let lines: Vec<&str> = src.split('\n').collect();

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim_start();

        if let Some((marker, len)) = fence {
            if is_fence(trimmed).is_some_and(|(m, l)| m == marker && l >= len) {
                fence = None;
            }
            continue;
        }
        if let Some((m, l)) = is_fence(trimmed) {
            flush(&mut cur, &mut out);
            fence = Some((m, l));
            continue;
        }
        if in_table {
            if !trimmed.is_empty() && line.contains('|') {
                continue;
            }
            in_table = false;
        }
        if is_delimiter_row(line) {
            flush(&mut cur, &mut out);
            in_table = true;
            continue;
        }
        // 머리글 줄. 다음 줄이 구분선이면 표의 시작이다.
        if line.contains('|') && lines.get(i + 1).copied().is_some_and(is_delimiter_row) {
            flush(&mut cur, &mut out);
            continue;
        }
        if trimmed.is_empty() || is_heading(trimmed) || is_rule(trimmed) {
            in_quote = false;
            // 헤딩을 건너뛰는 것은 **중립성 때문**이다. 헤딩 구문이 없는 채널은 줄 전체를
            // 굵게 내보내고, 그러면 헤딩 안의 강조 범위가 줄 전체로 커진다. 구현마다
            // 다른 이 선택을 고장으로 신고하지 않으려고 양쪽에서 똑같이 뺀다.
            flush(&mut cur, &mut out);
            continue;
        }
        // **리스트 항목은 각각이 한 블록이다.** 항목 여럿을 한 덩어리로 묶으면
        // 한 항목의 안 닫힌 강조가 다음 항목까지 번진 것으로 읽힌다. 코어는 항목이
        // 끝날 때 인라인을 확정하므로, 여기서도 똑같이 끊어야 같은 답이 나온다.
        if is_list_item(trimmed) {
            flush(&mut cur, &mut out);
        }
        // **인용문도 자기 블록이다.** 문단과 인용문 사이를 안 끊으면, 코어가 블록
        // 경계에서 닫은 강조를 여기서는 넘어간 것으로 읽는다.
        let quote = trimmed.starts_with('>');
        if quote != in_quote {
            flush(&mut cur, &mut out);
            in_quote = quote;
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(strip_block_marker(line));
    }
    flush(&mut cur, &mut out);
    out
}

fn flush(cur: &mut String, out: &mut Vec<String>) {
    if !cur.trim().is_empty() {
        out.push(std::mem::take(cur));
    } else {
        cur.clear();
    }
}

fn is_fence(trimmed: &str) -> Option<(char, usize)> {
    for marker in ['`', '~'] {
        let n = trimmed.chars().take_while(|&c| c == marker).count();
        if n >= 3 {
            return Some((marker, n));
        }
    }
    None
}

fn is_delimiter_row(line: &str) -> bool {
    let t = line.trim();
    t.contains('-')
        && t.contains('|')
        && t.chars().all(|c| matches!(c, '-' | ':' | '|' | ' ' | '\t'))
}

/// 블록 마커(`#`, `>`, 불릿, 번호)를 벗긴다. 인라인 짝짓기가 마커에 걸리지 않게 한다.
fn strip_block_marker(line: &str) -> &str {
    let t = line.trim_start();
    let t = t.trim_start_matches('>').trim_start();
    strip_list_marker(t).unwrap_or(t)
}

/// 리스트 마커를 벗긴 나머지. 마커가 아니면 `None`.
///
/// **마커 뒤는 공백이거나 탭이다.** `*<TAB>ws` 를 목록으로 안 보면 줄마다 앞에 선 `*`
/// 가 강조 마커로 짝지어져, 멀쩡한 출력을 고장으로 신고한다.
fn strip_list_marker(t: &str) -> Option<&str> {
    for m in ['-', '*', '+', '•'] {
        if let Some(rest) = t.strip_prefix(m) {
            if rest.starts_with([' ', '\t']) {
                return Some(&rest[1..]);
            }
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let rest = &t[digits..];
        for d in ['.', ')'] {
            if let Some(r) = rest.strip_prefix(d) {
                if r.starts_with([' ', '\t']) {
                    return Some(&r[1..]);
                }
            }
        }
    }
    None
}

/// 리스트 항목의 시작인가.
fn is_list_item(trimmed: &str) -> bool {
    strip_list_marker(trimmed).is_some()
}

/// 구분선인가. `***` 는 강조가 아니다 — 이걸 안 거르면 구분선을 굵게 만든 줄 알고
/// 정상 출력을 고장으로 신고한다.
pub(crate) fn is_rule(trimmed: &str) -> bool {
    let t = trimmed.trim_end();
    ['-', '*', '_'].iter().any(|&ch| {
        t.chars().filter(|&c| c == ch).count() >= 3
            && t.chars().all(|c| c == ch || c == ' ' || c == '\t')
    })
}

/// ATX 헤딩인가.
pub(crate) fn is_heading(trimmed: &str) -> bool {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    (1..=6).contains(&hashes) && trimmed[hashes..].starts_with([' ', '\t'])
}

struct Open {
    kind: Kind,
    marker: String,
    /// 추측으로 연 것인가. 양쪽 다 공백이라 원래는 그냥 글자인 `**` 를
    /// "줄바꿈에 걸린 강조일 것"으로 보고 연 경우다. 안 닫히면 되돌린다.
    guess: bool,
    /// 여는 쪽이 막힌 마커(`값**(합계)**를`)면 열린 줄 번호. 같은 줄의 거울 모양 마커와 짝짓는다. 규칙은 코어와 같다.
    hemmed: Option<usize>,
    buf: String,
}

fn scan_block(block: &str, mode: Mode, scan: &mut Scan) {
    let ch: Vec<char> = block.chars().collect();
    let mut stack: Vec<Open> = Vec::new();
    let mut root = String::new();
    let mut i = 0;

    let mut line = 0;
    while i < ch.len() {
        let c = ch[i];
        if c == '\n' {
            line += 1;
        }

        // **역슬래시 이스케이프가 먼저다.** `\_` 는 밑줄 한 글자지 강조 마커가 아니다.
        // 코어가 그렇게 읽으므로 참조 구현도 같이 읽어야 한다.
        if c == '\\' {
            if let Some(&next) = ch.get(i + 1) {
                if next.is_ascii_punctuation() {
                    push_char(&mut stack, &mut root, next);
                    i += 2;
                    continue;
                }
            }
        }

        // `<url>` · `<url|텍스트>` — 오토링크와 슬랙 레거시 링크. 강조 안의 본문은
        // 텍스트(없으면 주소)다. 출력은 `[텍스트](url)` 이나 `<a href>` 로 나가고 그쪽은
        // 이미 텍스트만 남기므로, 입력도 같은 모양으로 읽어야 범위가 맞는다.
        if c == '<'
            && (ch[i + 1..].starts_with(&['h', 't', 't', 'p', ':', '/', '/'])
                || ch[i + 1..].starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/']))
        {
            if let Some(close) = ch[i + 1..].iter().position(|&c| c == '>') {
                let body = &ch[i + 1..i + 1 + close];
                let (url, label) = match body.iter().position(|&c| c == '|') {
                    Some(bar) => (&body[..bar], &body[bar + 1..]),
                    None => (body, body),
                };
                // 공백은 텍스트에는 와도 주소에는 못 온다.
                if !url.iter().any(|c| c.is_whitespace()) {
                    push_text(&mut stack, &mut root, &label.iter().collect::<String>());
                    i += 1 + close + 1;
                    continue;
                }
            }
        }

        // 코드 스팬이 먼저다. 그 안의 `*` 는 강조가 아니다.
        if c == '`' {
            let run = run_len(&ch, i, '`');
            if let Some(close) = find_run(&ch, i + run, '`', run) {
                let text: String = ch[i + run..close].iter().collect();
                scan.raw.push(text.to_string());
                scan.spans.push(Span { kind: Kind::Code, text: normalize_ws(&text) });
                push_text(&mut stack, &mut root, &text);
                i = close + run;
                continue;
            }
            if mode != Mode::Repair {
                scan.unpaired.push(unpaired(&ch, i, run));
            }
            push_text(&mut stack, &mut root, &ch[i..i + run].iter().collect::<String>());
            i += run;
            continue;
        }

        // 링크의 URL 부분은 본문이 아니다. `[텍스트](url)` 에서 괄호 안을 건너뛴다.
        //
        // **대괄호를 무조건 건너뛰면 안 된다.** `[[위키링크]]` 처럼 링크가 아닌 대괄호가
        // 본문에 그대로 남는 문서가 있고, 그걸 삼키면 입력 쪽 텍스트만 짧아져서
        // 출력과 안 맞는다 — 정상 출력을 고장으로 신고하게 된다.
        if c == ']' && ch.get(i + 1) == Some(&'(') && link_end(&ch, i).is_some() {
            i = link_end(&ch, i).expect("방금 확인했다");
            continue;
        }
        if c == '[' {
            if opens_link(&ch, i) {
                i += 1;
                continue;
            }
            push_char(&mut stack, &mut root, c);
            i += 1;
            continue;
        }

        if !matches!(c, '*' | '_' | '~') {
            push_char(&mut stack, &mut root, c);
            i += 1;
            continue;
        }

        // 런은 통째로 소비한다. 규칙은 코어와 같다 — 구현만 따로다.
        // `***` 는 `**` 와 `*` 다. 열 때는 굵게 먼저, 닫을 때는 열린 기울임 먼저.
        let take = run_len(&ch, i, c);
        let take = if c != '~' && take == 3 {
            if stack.iter().any(|o| o.kind == Kind::Italic) {
                1
            } else if stack.iter().any(|o| o.kind == Kind::Bold) {
                3
            } else {
                2
            }
        } else {
            take
        };
        let kind = match (c, take) {
            ('~', _) => Kind::Strike,
            (_, 1) => Kind::Italic,
            _ => Kind::Bold,
        };
        // 취소선은 `~~` 다. 홀로 선 `~` 는 글자다(GFM · 슬랙 `markdown_text` 둘 다).
        // 규칙은 코어와 같다.
        if c == '~' && take < 2 {
            push_char(&mut stack, &mut root, c);
            i += 1;
            continue;
        }

        let marker: String = ch[i..i + take].iter().collect();
        // 쪼갠 런의 남은 조각도 런 전체의 앞 글자를 본다. 규칙은 코어와 같다.
        let start = i - ch[..i].iter().rev().take_while(|&&x| x == c).count();
        let prev = if start > 0 { Some(ch[start - 1]) } else { None };
        let next = ch.get(i + take).copied();

        let after_space = prev.is_none_or(char::is_whitespace);
        let open_same = stack.iter().rposition(|o| o.kind == kind);

        // 글자 뒤의 `_` 는 열지 못한다 — `snake_case` 도 `2026-04-29_제목` 도 글자다.
        // 열린 기울임이 있으면 닫는다(`_진료_가`). 규칙은 코어와 같다.
        let intraword = c == '_' && prev.is_some_and(char::is_alphanumeric);
        if intraword && open_same.is_none() {
            push_text(&mut stack, &mut root, &marker);
            i += take;
            continue;
        }

        let left = can_open(prev, next) && !intraword;
        // CommonMark 의 우측 flanking — `CommonMark` 모드에서만 닫기에 쓴다.
        let right = !after_space && (!prev.is_some_and(is_punct) || next.is_none_or(|n| n.is_whitespace() || is_punct(n)));
        let closes = mode != Mode::CommonMark || right;

        // 여는 쪽이 막혔다 — 앞이 글자이고 뒤가 구두점. 규칙은 코어와 같다.
        let hemmed = prev.is_some_and(|p| p.is_alphabetic() || p.is_ascii_digit())
            && next.is_some_and(|n| !n.is_whitespace() && is_punct(n));

        match open_same {
            // 막힌 여는 마커의 거울 짝 — 같은 줄, 같은 길이, 앞이 구두점. 규칙은 코어와 같다.
            Some(at)
                if mode != Mode::CommonMark
                    && stack[at].hemmed == Some(line)
                    && stack[at].marker.chars().count() == take
                    && prev.is_some_and(|p| {
                        !(p.is_alphabetic() || p.is_ascii_digit()) && !p.is_whitespace() && !is_opening_bracket(p)
                    }) =>
            {
                stack[at].guess = false;
                close_to(&mut stack, &mut root, at, scan)
            }
            // 추측으로 연 것은 닫지 않는다 — 닫는 자리의 마커는 글자다. 규칙은 코어와 같다.
            // 막힌 추측 뒤의 `(**주의` 는 여는 마커라 아래로 보낸다(`2**(n-1) (**주의**)`).
            Some(at)
                if stack[at].guess
                    && (!left || !after_space)
                    && !(left && stack[at].hemmed.is_some() && prev.is_some_and(is_opening_bracket)) =>
            {
                *scan.literal.entry(c).or_default() += take;
                push_text(&mut stack, &mut root, &marker)
            }
            // 여는 자리의 마커가 왔는데 열린 것이 추측이다 — 추측이 틀렸다. 되돌리고 연다.
            Some(at) if stack[at].guess => {
                // 위에 열린 것들을 먼저 정리한다 — 아래만 빼면 순서가 깨진다. 규칙은 코어와 같다.
                while stack.len() > at + 1 {
                    let inner = stack.pop().expect("at 보다 위에 있다");
                    if !inner.guess && !inner.buf.trim().is_empty() {
                        scan.raw.push(inner.buf.to_string());
                        scan.spans.push(Span { kind: inner.kind, text: normalize_ws(&inner.buf) });
                    }
                    let text = if inner.guess { format!("{}{}", inner.marker, inner.buf) } else { inner.buf };
                    push_text(&mut stack, &mut root, &text);
                }
                let old = stack.pop().expect("at 은 유효한 인덱스다");
                *scan.literal.entry(c).or_default() += old.marker.chars().count();
                let restored = format!("{}{}", old.marker, old.buf);
                push_text(&mut stack, &mut root, &restored);
                stack.push(Open { kind, marker, guess: false, hemmed: None, buf: String::new() });
            }
            // 같은 종류가 열려 있고 앞이 공백이 아니면 닫는 자리다.
            Some(at) if !after_space && closes => close_to(&mut stack, &mut root, at, scan),
            // 여는 자리의 마커가 또 왔다 — 먼저 열린 쪽이 진다. 마커는 버린다(엄격 모드에서는
            // 짝 없음). 규칙은 코어와 같다.
            Some(at) if left && at + 1 == stack.len() => {
                let old = stack.pop().expect("at 은 유효한 인덱스다");
                if old.guess {
                    let restored = format!("{}{}", old.marker, old.buf);
                    push_text(&mut stack, &mut root, &restored);
                } else {
                    if mode != Mode::Repair {
                        scan.unpaired.push(Unpaired { marker: old.marker.clone(), context: snippet(&old.buf) });
                    }
                    let buf = old.buf;
                    push_text(&mut stack, &mut root, &buf);
                }
                stack.push(Open { kind, marker, guess: false, hemmed: None, buf: String::new() });
            }
            // 안쪽에 다른 종류가 열려 있으면 갈아 끼우지 못한다. 버린다.
            Some(_) if left => {
                if mode != Mode::Repair {
                    scan.unpaired.push(unpaired(&ch, i, take));
                }
            }
            Some(at) if closes => close_to(&mut stack, &mut root, at, scan),
            // CommonMark 가 닫지 않는 자리다 — 채널 화면에는 마커가 글자로 남는다.
            Some(_) if !left => {
                *scan.literal.entry(c).or_default() += take;
                push_text(&mut stack, &mut root, &marker)
            }
            Some(_) => stack.push(Open { kind, marker, guess: false, hemmed: None, buf: String::new() }),
            None if left => stack.push(Open { kind, marker, guess: false, hemmed: None, buf: String::new() }),
            // 열 수도 닫을 수도 없다. 그래도 80열 wrap 이 `... **\n강조**` 를 만들어 낸다.
            // 일단 열어 두고, 안 닫히면 글자로 되돌린다.
            None if mode == Mode::Repair || (mode == Mode::Strict && hemmed) => {
                stack.push(Open { kind, marker, guess: true, hemmed: hemmed.then_some(line), buf: String::new() })
            }
            None => {
                *scan.literal.entry(c).or_default() += take;
                push_text(&mut stack, &mut root, &marker)
            }
        }
        i += take;
    }

    // 블록이 끝났다. 열린 것을 정리한다.
    while let Some(open) = stack.pop() {
        if mode != Mode::Repair {
            scan.unpaired.push(Unpaired {
                marker: open.marker.clone(),
                context: snippet(&open.buf),
            });
        }
        if open.guess {
            // 추측이 빗나갔다. 마커를 글자로 되돌린다.
            if let Some(m) = open.marker.chars().next() {
                *scan.literal.entry(m).or_default() += open.marker.chars().count();
            }
            let restored = format!("{}{}", open.marker, open.buf);
            push_text(&mut stack, &mut root, &restored);
        } else {
            // SPEC 6절 — 안 닫힌 강조는 닫는다. 범위는 블록 끝까지다.
            if !open.buf.trim().is_empty() {
                scan.raw.push(open.buf.to_string());
                scan.spans.push(Span { kind: open.kind, text: normalize_ws(&open.buf) });
            }
            let buf = open.buf;
            push_text(&mut stack, &mut root, &buf);
        }
    }
}

fn close_to(stack: &mut Vec<Open>, root: &mut String, at: usize, scan: &mut Scan) {
    // `at` 위에 다른 종류가 열려 있으면 그것들도 함께 닫는다(교차 중첩 복구).
    while stack.len() > at + 1 {
        let inner = stack.pop().expect("at 보다 위에 있다");
        if !inner.guess && !inner.buf.trim().is_empty() {
            scan.raw.push(inner.buf.to_string());
            scan.spans.push(Span { kind: inner.kind, text: normalize_ws(&inner.buf) });
        }
        let text = if inner.guess { format!("{}{}", inner.marker, inner.buf) } else { inner.buf };
        push_text(stack, root, &text);
    }
    let open = stack.pop().expect("at 은 유효한 인덱스다");
    if !open.buf.trim().is_empty() {
        scan.raw.push(open.buf.to_string());
        scan.spans.push(Span { kind: open.kind, text: normalize_ws(&open.buf) });
    }
    let buf = open.buf;
    push_text(stack, root, &buf);
}

fn push_char(stack: &mut [Open], root: &mut String, c: char) {
    match stack.last_mut() {
        Some(o) => o.buf.push(c),
        None => root.push(c),
    }
}

fn push_text(stack: &mut [Open], root: &mut String, s: &str) {
    match stack.last_mut() {
        Some(o) => o.buf.push_str(s),
        None => root.push_str(s),
    }
}


/// `](` 뒤의 닫는 괄호 다음 위치.
fn link_end(ch: &[char], at: usize) -> Option<usize> {
    ch[at + 2..].iter().position(|&x| x == ')').map(|p| at + 2 + p + 1)
}

/// 이 `[` 가 진짜 링크를 여는가 — 뒤에 `](…)` 가 있는가.
fn opens_link(ch: &[char], at: usize) -> bool {
    let mut depth = 0usize;
    let mut j = at + 1;
    while j < ch.len() {
        match ch[j] {
            '[' => depth += 1,
            ']' if depth == 0 => break,
            ']' => depth -= 1,
            _ => {}
        }
        j += 1;
    }
    ch.get(j) == Some(&']') && ch.get(j + 1) == Some(&'(') && link_end(ch, j).is_some()
}

pub(crate) fn run_len(ch: &[char], at: usize, c: char) -> usize {
    ch[at..].iter().take_while(|&&x| x == c).count()
}

fn find_run(ch: &[char], from: usize, c: char, len: usize) -> Option<usize> {
    let mut i = from;
    while i < ch.len() {
        if ch[i] == c {
            let n = run_len(ch, i, c);
            if n == len {
                return Some(i);
            }
            i += n;
        } else {
            i += 1;
        }
    }
    None
}

fn unpaired(ch: &[char], at: usize, len: usize) -> Unpaired {
    let from = at.saturating_sub(12);
    let to = (at + len + 12).min(ch.len());
    Unpaired {
        marker: ch[at..at + len].iter().collect(),
        context: ch[from..to].iter().collect::<String>().replace('\n', "⏎"),
    }
}

fn snippet(s: &str) -> String {
    let t = normalize_ws(s);
    match t.char_indices().nth(24) {
        Some((i, _)) => format!("{}…", &t[..i]),
        None => t,
    }
}

/// 이 마커가 **열 수 있는가**(CommonMark 의 좌측 flanking).
///
/// **줄 첫머리의 `**` 가 닫기가 될 수 없다**는 것이 핵심이다. 앞이 줄바꿈(= 공백)이면
/// 열기로만 읽힌다. 정규식 변환기가 이 판정을 못 해서 뒤의 `**` 와 잘못 짝지었고,
/// 강조 범위가 뒤집혔다.
///
/// 닫는 쪽은 우측 flanking 을 쓰지 않는다 — 이유는 코어의 같은 이름 함수에 적어 뒀다.
/// 규칙은 코어와 같고 구현만 따로다.
pub(crate) fn can_open(prev: Option<char>, next: Option<char>) -> bool {
    // 앞이 글자·숫자가 아니면 열 수 있다 — 기호·이모지 뒤의 `**` 도. `①` 은 유니코드
    // 숫자라 알파벳과 ASCII 숫자만 글자로 친다. 규칙은 코어와 같다.
    next.is_some_and(|n| {
        !n.is_whitespace() && (!is_punct(n) || prev.is_none_or(|p| !(p.is_alphabetic() || p.is_ascii_digit())))
    })
}

/// 여는 괄호·따옴표 — 코어의 `is_opening_bracket` 과 같다.
fn is_opening_bracket(c: char) -> bool {
    matches!(c, '(' | '[' | '{' | '「' | '『' | '（' | '【' | '《' | '“' | '‘')
}

pub(crate) fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(c,
            '，' | '。' | '、' | '！' | '？' | '；' | '：' | '·' | '…' | '—' | '～'
            | '「' | '」' | '『' | '』' | '（' | '）' | '【' | '】' | '《' | '》'
            | '“' | '”' | '‘' | '’')
}

/// 텔레그램 HTML 출력에서 강조 범위를 뽑는다.
///
/// `<pre>` 안은 건너뛴다 — 고정폭 블록이라 강조가 없는 것이 정상이다.
pub fn scan_html(src: &str) -> Scan {
    let mut scan = Scan::default();
    let mut stack: Vec<(Option<Kind>, String, String)> = Vec::new(); // (종류, 태그명, 본문)
    let mut root = String::new();
    let ch: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut in_pre = false;

    while i < ch.len() {
        if ch[i] == '<' {
            if let Some((name, closing, end)) = parse_tag(&ch, i) {
                i = end;
                if name == "pre" {
                    in_pre = !closing;
                    continue;
                }
                if in_pre {
                    continue;
                }
                // **빈 요소는 스택에 올리지 않는다.** `<br/>` 을 여는 태그로 올리면 뒤따르는
                // 글이 전부 그 안으로 들어가 강조 범위가 끊겨 보인다. `<br>` 은 줄바꿈이다.
                if matches!(name.as_str(), "br" | "hr" | "img" | "input" | "wbr") {
                    if name == "br" {
                        push_html_text(&mut stack, &mut root, "\n");
                    }
                    continue;
                }
                let kind = tag_kind(&name);
                if closing {
                    match stack.iter().rposition(|(_, n, _)| *n == name) {
                        Some(at) => {
                            while stack.len() > at + 1 {
                                let (k, n, buf) = stack.pop().expect("at 위");
                                record(&mut scan, k, &buf);
                                scan.unpaired.push(Unpaired {
                                    marker: format!("<{n}>"),
                                    context: snippet(&buf),
                                });
                                push_html_text(&mut stack, &mut root, &buf);
                            }
                            let (k, _, buf) = stack.pop().expect("at 은 유효하다");
                            record(&mut scan, k, &buf);
                            push_html_text(&mut stack, &mut root, &buf);
                        }
                        None => scan.unpaired.push(Unpaired {
                            marker: format!("</{name}>"),
                            context: "열린 적 없는 태그를 닫는다".to_string(),
                        }),
                    }
                } else {
                    stack.push((kind, name, String::new()));
                }
                continue;
            }
        }
        let c = ch[i];
        if c == '&' {
            if let Some((decoded, end)) = parse_entity(&ch, i) {
                push_html_text(&mut stack, &mut root, &decoded.to_string());
                i = end;
                continue;
            }
        }
        if !in_pre {
            match stack.last_mut() {
                Some((_, _, buf)) => buf.push(c),
                None => root.push(c),
            }
        }
        i += 1;
    }

    while let Some((k, n, buf)) = stack.pop() {
        scan.unpaired.push(Unpaired { marker: format!("<{n}>"), context: snippet(&buf) });
        record(&mut scan, k, &buf);
        push_html_text(&mut stack, &mut root, &buf);
    }
    scan
}

fn record(scan: &mut Scan, kind: Option<Kind>, buf: &str) {
    if let Some(k) = kind {
        if !buf.trim().is_empty() {
            scan.raw.push(buf.to_string());
            scan.spans.push(Span { kind: k, text: normalize_ws(buf) });
        }
    }
}

fn push_html_text(stack: &mut [(Option<Kind>, String, String)], root: &mut String, s: &str) {
    match stack.last_mut() {
        Some((_, _, buf)) => buf.push_str(s),
        None => root.push_str(s),
    }
}

/// `<b>` `</b>` 를 읽는다. 태그가 아니면 `None` — 그때 `<` 는 글자다(= 이스케이프 누락).
pub(crate) fn parse_tag(ch: &[char], at: usize) -> Option<(String, bool, usize)> {
    let mut i = at + 1;
    let closing = ch.get(i) == Some(&'/');
    if closing {
        i += 1;
    }
    let start = i;
    while i < ch.len() && (ch[i].is_ascii_alphanumeric() || ch[i] == '-') {
        i += 1;
    }
    if i == start {
        return None;
    }
    let name: String = ch[start..i].iter().collect::<String>().to_ascii_lowercase();
    while i < ch.len() && ch[i] != '>' {
        i += 1;
    }
    if i >= ch.len() {
        return None;
    }
    Some((name, closing, i + 1))
}

pub(crate) fn parse_entity(ch: &[char], at: usize) -> Option<(char, usize)> {
    for (name, c) in [("&amp;", '&'), ("&lt;", '<'), ("&gt;", '>'), ("&quot;", '"'), ("&#39;", '\'')] {
        let n = name.chars().count();
        if ch.len() >= at + n && ch[at..at + n].iter().collect::<String>() == name {
            return Some((c, at + n));
        }
    }
    // **숫자 엔티티도 읽는다** — `&#x27;` · `&#39;`. React 의 정적 렌더가 따옴표를 이렇게
    // 적는데, 못 읽으면 다른 구현의 멀쩡한 이스케이프를 누락으로 신고한다.
    if ch.get(at + 1) != Some(&'#') {
        return None;
    }
    let hex = matches!(ch.get(at + 2), Some('x' | 'X'));
    let from = at + if hex { 3 } else { 2 };
    let len = ch[from.min(ch.len())..].iter().take_while(|c| if hex { c.is_ascii_hexdigit() } else { c.is_ascii_digit() }).count();
    if len == 0 || len > 6 || ch.get(from + len) != Some(&';') {
        return None;
    }
    let digits: String = ch[from..from + len].iter().collect();
    let n = u32::from_str_radix(&digits, if hex { 16 } else { 10 }).ok()?;
    Some((char::from_u32(n)?, from + len + 1))
}

fn tag_kind(name: &str) -> Option<Kind> {
    match name {
        "b" | "strong" => Some(Kind::Bold),
        "i" | "em" => Some(Kind::Italic),
        "s" | "strike" | "del" => Some(Kind::Strike),
        "code" => Some(Kind::Code),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 다른 구현의 출력도 잰다 — React 의 정적 렌더는 따옴표를 `&#x27;` 로, 줄바꿈을
    /// `<br/>` 로 적는다. 앞엣것을 못 읽으면 escape 누락으로, 뒤엣것을 여는 태그로 올리면
    /// 뒤따르는 글이 그 안에 갇혀 강조 범위가 끊긴 것으로 잡혔다.
    #[test]
    fn html_scan_reads_numeric_entities_and_void_br() {
        let ch: Vec<char> = "&#x27;&#39;&#1;".chars().collect();
        assert_eq!(parse_entity(&ch, 0), Some(('\'', 6)));
        assert_eq!(parse_entity(&ch, 6), Some(('\'', 11)));
        let spans = scan_html("<strong>둘째 줄<br/>이어짐</strong> 뒤").spans;
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "둘째 줄 이어짐");
    }

    fn bolds(scan: &Scan) -> Vec<String> {
        scan.spans.iter().filter(|s| s.kind == Kind::Bold).map(|s| s.text.clone()).collect()
    }

    /// 코퍼스 케이스 그대로. **줄 첫머리의 `**` 는 닫기가 아니다** — 여기가 뒤집히면
    /// 그게 이 저장소가 존재하는 이유인 바로 그 고장이다. 열기로 읽고 먼저 열린 쪽을 물린다.
    #[test]
    fn emphasis_across_linebreak_pairs_outward() {
        let input = "공개 채널**이다. 글 내용이 아니라\n**신분 공개 + 시점의 조합**이 판단 대상";
        let scan = scan_markdown(input, Mode::Repair);
        assert_eq!(bolds(&scan), vec!["신분 공개 + 시점의 조합"]);
        // 실제 모양 — 여는 마커는 보통 자리, 닫는 마커는 다음 줄. 범위 하나다.
        let scan = scan_markdown("결론은 **배포를 금요일에\n하지 않는다** 이고", Mode::Repair);
        assert_eq!(bolds(&scan), vec!["배포를 금요일에 하지 않는다"]);
    }

    /// 정규식 변환기가 내놓던 뒤집힌 범위는 **입력과 다른 범위**로 잡혀야 한다.
    /// 그래야 채점기가 그 고장을 잡는다.
    #[test]
    fn inverted_range_does_not_match_the_source() {
        let input = "공개 채널**이다. 글 내용이 아니라\n**신분 공개 + 시점의 조합**이 판단 대상";
        let broken = "공개 채널<b>이다. 글 내용이 아니라\n</b>신분 공개 + 시점의 조합**이 판단 대상";
        let want = scan_markdown(input, Mode::Repair);
        let got = scan_html(broken);
        assert_ne!(bolds(&want), bolds(&got));
    }

    #[test]
    fn unclosed_emphasis_closes_at_block_end() {
        let scan = scan_markdown("앞말 **굵은 꼬리", Mode::Repair);
        assert_eq!(bolds(&scan), vec!["굵은 꼬리"]);
    }

    #[test]
    fn lone_asterisks_between_spaces_stay_literal() {
        // `a ** b` 는 강조가 아니다. 추측으로 열었더라도 안 닫히면 되돌린다.
        let scan = scan_markdown("2 ** 3 은 곱셈이 아니다", Mode::Repair);
        assert!(bolds(&scan).is_empty(), "{:?}", scan.spans);
    }

    #[test]
    fn wrapped_open_marker_is_repaired() {
        // 줄 끝에 홀로 남은 `**` 는 추측이고, 다음 줄의 `**` 가 그것을 닫지 않는다 —
        // 추측은 확정되지 않는다. 둘 다 글자로 남고 강조는 없다(CommonMark 와 같다).
        let scan = scan_markdown("글 내용이 아니라 **\n신분 공개 + 시점의 조합**이 판단", Mode::Repair);
        assert_eq!(bolds(&scan), Vec::<String>::new());
    }

    #[test]
    fn underscore_inside_identifier_is_not_emphasis() {
        let scan = scan_markdown("`snake_case` 와 plain_text_name 은 그대로다", Mode::Repair);
        assert!(scan.spans.iter().all(|s| s.kind != Kind::Italic), "{:?}", scan.spans);
    }

    #[test]
    fn code_fence_and_table_are_skipped() {
        let src = "```\n**코드 안의 별표**\n```\n\n| 열 | **표 안** |\n|---|---|\n| 1 | 2 |\n";
        let scan = scan_markdown(src, Mode::Repair);
        assert!(scan.spans.is_empty(), "{:?}", scan.spans);
    }

    #[test]
    fn strict_mode_reports_unclosed_html() {
        let scan = scan_html("앞 <b>굵게 뒤");
        assert_eq!(scan.unpaired.len(), 1);
    }

    #[test]
    fn html_entities_are_decoded_before_comparing() {
        let scan = scan_html("<b>a &amp; b</b>");
        assert_eq!(bolds(&scan), vec!["a & b"]);
    }
}

#[cfg(test)]
mod angle_tests {
    use super::*;

    #[test]
    fn angle_link_label_is_the_span_text() {
        let scan = scan_markdown("**<https://a.com/p|TS 7 RC>** 다", Mode::Repair);
        assert_eq!(scan.spans, vec![Span { kind: Kind::Bold, text: "TS 7 RC".into() }]);
    }

    /// CommonMark 모드는 슬랙이 못 닫는 짝을 고발한다 — 겹친 강조에서 바깥 조이너만 넣었을 때
    /// 안쪽 기울임이 `)` 와 조이너 사이에 끼어 못 닫혔다(실측 2026-10-02).
    #[test]
    fn commonmark_mode_reports_what_slack_cannot_close() {
        let broken = "***중요(필수)*\u{2060}**를";
        assert!(scan_markdown(broken, Mode::Strict).unpaired.is_empty());
        assert!(!scan_markdown(broken, Mode::CommonMark).unpaired.is_empty());
        let fixed = "***중요(필수)\u{2060}*\u{2060}**를";
        assert!(scan_markdown(fixed, Mode::CommonMark).unpaired.is_empty());
        assert!(!scan_markdown("**설정(config)**을", Mode::CommonMark).unpaired.is_empty());
    }

    /// 막힌 여는 마커 뒤에 괄호 안의 강조가 오면 거울이 아니다 — 규칙은 코어와 같다.
    #[test]
    fn opening_bracket_marker_is_not_a_mirror() {
        let scan = scan_markdown("2**(n-1) (**주의**)", Mode::Repair);
        assert_eq!(scan.spans, vec![Span { kind: Kind::Bold, text: "주의".into() }]);
        let scan = scan_markdown("값**(합계)**를", Mode::Repair);
        assert_eq!(scan.spans, vec![Span { kind: Kind::Bold, text: "(합계)".into() }]);
    }
}
