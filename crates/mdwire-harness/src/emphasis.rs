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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

/// 짝짓기 모드.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 입력용. 깨진 마크다운을 우리 복구 규칙대로 읽는다 — 이것이 "기대되는 강조 범위"다.
    Repair,
    /// 출력용. 복구하지 않는다. 짝이 안 맞으면 그대로 고발한다.
    Strict,
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
        if c == '\u{200b}' {
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
    for m in ["- ", "* ", "+ ", "• "] {
        if let Some(rest) = t.strip_prefix(m) {
            return rest;
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return r;
        }
    }
    t
}

/// 리스트 항목의 시작인가.
fn is_list_item(trimmed: &str) -> bool {
    for m in ["- ", "* ", "+ ", "• "] {
        if trimmed.starts_with(m) {
            return true;
        }
    }
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && {
        let rest = &trimmed[digits..];
        rest.starts_with(". ") || rest.starts_with(") ")
    }
}

/// 구분선인가. `***` 는 강조가 아니다 — 이걸 안 거르면 구분선을 굵게 만든 줄 알고
/// 정상 출력을 고장으로 신고한다.
pub(crate) fn is_rule(trimmed: &str) -> bool {
    let t = trimmed.trim_end();
    ['-', '*', '_'].iter().any(|&ch| {
        t.chars().filter(|&c| c == ch).count() >= 3 && t.chars().all(|c| c == ch || c == ' ')
    })
}

/// ATX 헤딩인가.
pub(crate) fn is_heading(trimmed: &str) -> bool {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ')
}

struct Open {
    kind: Kind,
    marker: String,
    /// 추측으로 연 것인가. 양쪽 다 공백이라 원래는 그냥 글자인 `**` 를
    /// "줄바꿈에 걸린 강조일 것"으로 보고 연 경우다. 안 닫히면 되돌린다.
    guess: bool,
    buf: String,
}

fn scan_block(block: &str, mode: Mode, scan: &mut Scan) {
    let ch: Vec<char> = block.chars().collect();
    let mut stack: Vec<Open> = Vec::new();
    let mut root = String::new();
    let mut i = 0;

    while i < ch.len() {
        let c = ch[i];

        // 코드 스팬이 먼저다. 그 안의 `*` 는 강조가 아니다.
        if c == '`' {
            let run = run_len(&ch, i, '`');
            if let Some(close) = find_run(&ch, i + run, '`', run) {
                let text: String = ch[i + run..close].iter().collect();
                scan.spans.push(Span { kind: Kind::Code, text: normalize_ws(&text) });
                push_text(&mut stack, &mut root, &text);
                i = close + run;
                continue;
            }
            if mode == Mode::Strict {
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
        let take = run_len(&ch, i, c);
        let kind = match (c, take) {
            ('~', _) => Kind::Strike,
            (_, 1) => Kind::Italic,
            _ => Kind::Bold,
        };
        // 취소선은 `~~` 다. 홀로 선 `~` 는 글자다 — 규칙은 코어와 같다.
        if c == '~' && take < 2 {
            push_char(&mut stack, &mut root, c);
            i += 1;
            continue;
        }

        let marker: String = ch[i..i + take].iter().collect();
        let prev = if i > 0 { Some(ch[i - 1]) } else { None };
        let next = ch.get(i + take).copied();

        // `snake_case` 의 밑줄은 강조가 아니다.
        if c == '_' && prev.is_some_and(is_word) && next.is_some_and(is_word) {
            push_text(&mut stack, &mut root, &marker);
            i += take;
            continue;
        }

        let left = can_open(prev, next);
        let after_space = prev.is_none_or(char::is_whitespace);
        let open_same = stack.iter().rposition(|o| o.kind == kind);

        match open_same {
            // 같은 종류가 열려 있고 앞이 공백이 아니면 닫는 자리다.
            Some(at) if !after_space => close_to(&mut stack, &mut root, at, scan),
            // 이미 같은 종류가 열려 있는데 또 여는 마커가 왔다. 줄바꿈에 걸린 강조에서
            // 실제로 나오는 모양이다 — 겹쳐 열지 않고 버린다. 여기서 "닫기"로 읽으면
            // 강조 범위가 뒤집힌다. 그 고장이 원본이다.
            Some(_) if left => {
                if mode == Mode::Strict {
                    scan.unpaired.push(unpaired(&ch, i, take));
                }
            }
            Some(at) => close_to(&mut stack, &mut root, at, scan),
            None if left => stack.push(Open { kind, marker, guess: false, buf: String::new() }),
            // 열 수도 닫을 수도 없다. 그래도 80열 wrap 이 `... **\n강조**` 를 만들어 낸다.
            // 일단 열어 두고, 안 닫히면 글자로 되돌린다.
            None if mode == Mode::Repair => {
                stack.push(Open { kind, marker, guess: true, buf: String::new() })
            }
            None => push_text(&mut stack, &mut root, &marker),
        }
        i += take;
    }

    // 블록이 끝났다. 열린 것을 정리한다.
    while let Some(open) = stack.pop() {
        if mode == Mode::Strict {
            scan.unpaired.push(Unpaired {
                marker: open.marker.clone(),
                context: snippet(&open.buf),
            });
        }
        if open.guess {
            // 추측이 빗나갔다. 마커를 글자로 되돌린다.
            let restored = format!("{}{}", open.marker, open.buf);
            push_text(&mut stack, &mut root, &restored);
        } else {
            // SPEC 6절 — 안 닫힌 강조는 닫는다. 범위는 블록 끝까지다.
            if !open.buf.trim().is_empty() {
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
            scan.spans.push(Span { kind: inner.kind, text: normalize_ws(&inner.buf) });
        }
        let text = if inner.guess { format!("{}{}", inner.marker, inner.buf) } else { inner.buf };
        push_text(stack, root, &text);
    }
    let open = stack.pop().expect("at 은 유효한 인덱스다");
    if !open.buf.trim().is_empty() {
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

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
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
    next.is_some_and(|n| {
        !n.is_whitespace() && (!is_punct(n) || prev.is_none_or(|p| p.is_whitespace() || is_punct(p)))
    })
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
    None
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

    fn bolds(scan: &Scan) -> Vec<String> {
        scan.spans.iter().filter(|s| s.kind == Kind::Bold).map(|s| s.text.clone()).collect()
    }

    /// 코퍼스 케이스 그대로. **이 한 줄이 이 저장소의 존재 이유다.**
    #[test]
    fn emphasis_across_linebreak_pairs_outward() {
        let input = "공개 채널**이다. 글 내용이 아니라\n**신분 공개 + 시점의 조합**이 판단 대상";
        let scan = scan_markdown(input, Mode::Repair);
        assert_eq!(bolds(&scan), vec!["이다. 글 내용이 아니라 신분 공개 + 시점의 조합"]);
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
        // 80열 wrap 이 만드는 모양: 여는 마커가 줄 끝에 남는다.
        let scan = scan_markdown("글 내용이 아니라 **\n신분 공개 + 시점의 조합**이 판단", Mode::Repair);
        assert_eq!(bolds(&scan), vec!["신분 공개 + 시점의 조합"]);
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
