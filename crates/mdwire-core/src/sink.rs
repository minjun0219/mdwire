//! 출력 싱크와 안전 분할.
//!
//! 엔진은 출력을 싱크에 붙여 나가면서 **여기서 잘라도 안전하다**는 지점을 알려 준다
//! ([`Sink::boundary`]). 분할이 렌더 결과를 다시 훑어 자르는 것이 아니라 **구조에서
//! 나온다**는 뜻이다 — `DESIGN.md` 의 두 번째 고장이 정확히 "마크업을 만든 뒤에 자르기"였다.

use crate::vocab::Vocab;
use crate::Channel;

pub(crate) trait Sink {
    fn text(&mut self, s: &str);
    /// 지금까지 내보낸 것에 열린 마크업이 없다. 여기가 조각 경계가 될 수 있다.
    fn boundary(&mut self) {}
}

/// 스트리밍용. 받은 것을 그대로 이어 붙인다 — 한도 분할은 호출자 몫이다.
pub(crate) struct StringSink<'a>(pub &'a mut String);

impl Sink for StringSink<'_> {
    fn text(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

/// 완성본용. 한도를 넘으면 **경계에서** 나눈다.
pub(crate) struct PartsSink {
    v: Vocab,
    limit: usize,
    parts: Vec<String>,
    cur: String,
    cur_len: usize,
    pending: String,
}

impl PartsSink {
    pub fn new(v: Vocab) -> Self {
        Self {
            limit: v.channel.limit(),
            v,
            parts: Vec::new(),
            cur: String::new(),
            cur_len: 0,
            pending: String::new(),
        }
    }

    pub fn into_parts(mut self) -> Vec<String> {
        self.commit();
        if !self.cur.is_empty() {
            self.parts.push(self.cur);
        }
        self.parts
    }

    fn commit(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        let piece = if self.cur.is_empty() { pending.trim_start_matches('\n') } else { &pending };
        let n = piece.chars().count();
        if n == 0 {
            self.pending = pending;
            self.pending.clear();
            return;
        }

        if self.cur_len + n <= self.limit {
            self.cur.push_str(piece);
            self.cur_len += n;
        } else {
            if !self.cur.is_empty() {
                self.parts.push(std::mem::take(&mut self.cur));
                self.cur_len = 0;
            }
            let piece = piece.trim_start_matches('\n');
            let n = piece.chars().count();
            if n <= self.limit {
                self.cur.push_str(piece);
                self.cur_len = n;
            } else {
                // 블록 하나가 한도를 넘는다. 이때만 줄·글자 단위로 쪼갠다.
                let mut chunks = split_hard(piece, self.limit, &self.v);
                if let Some(last) = chunks.pop() {
                    self.cur_len = last.chars().count();
                    self.cur = last;
                }
                self.parts.extend(chunks);
            }
        }
        self.pending = pending;
        self.pending.clear();
    }
}

impl Sink for PartsSink {
    fn text(&mut self, s: &str) {
        self.pending.push_str(s);
    }
    fn boundary(&mut self) {
        self.commit();
    }
}

/// 블록 하나가 한도보다 길 때의 최후 수단.
///
/// 줄 경계를 먼저 쓰고, 한 줄이 통째로 넘으면 공백에서 끊는다. 어느 쪽이든
/// **열린 마크업은 끊는 자리에서 닫고 다음 조각에서 다시 연다.** 닫는 데 드는 글자까지
/// 미리 빼고 재야 한다 — 한도에 딱 맞춰 채운 뒤 `</b></blockquote>` 를 붙이면 넘는다.
fn split_hard(text: &str, limit: usize, v: &Vocab) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut len = 0usize;
    let mut markup = Markup::default();
    if v.channel != Channel::TelegramHtml && !v.is_plain() {
        markup.spans = std::rc::Rc::from(scan_spans(text));
    }

    for line in text.split_inclusive('\n') {
        for word in line.split_inclusive(' ') {
            let mut rest = word;
            while !rest.is_empty() {
                // **넣고 난 뒤의 마크업으로 예산을 잡는다.** 넣기 전 상태로 재면,
                // 이번에 들어가는 조각이 태그를 하나 더 열었을 때 그 태그를 닫을 자리가
                // 남지 않는다 — 그러면 닫고 나서 한도를 넘는다.
                let mut probe = markup.clone();
                probe.feed(rest, v);
                let budget = limit.saturating_sub(probe.reserve(v));
                let n = rest.chars().count();
                if len + n <= budget {
                    markup = probe;
                    len += push_after_reopen(&mut cur, rest, &mut markup);
                    break;
                }
                if len > 0 {
                    let before = len;
                    cut(&mut parts, &mut cur, &mut len, &mut markup, v, limit);
                    if len < before {
                        continue;
                    }
                    // **끊어도 자리가 안 생겼다.** 다시 연 마크업이 그만큼 도로 먹은
                    // 것이라, 여기서 또 끊으면 똑같은 조각만 끝없이 찍어 낸다. 아래
                    // 글자 단위 경로로 내려가 반드시 한 글자는 소비한다.
                }
                // 공백 하나 없는 덩어리다 — 글자로 끊는다.
                //
                // 예산이 0 이면 **이 덩어리가 여는 태그가 한도만 하다는 뜻**이다. 그런
                // 태그는 이 채널에서 애초에 쓸 수 없다 — 한 글자씩 끊어 봐야 조각만
                // 쏟아진다. 지금 열려 있는 것 기준으로 최대한 담고, 닫을 자리는 `cut`
                // 이 마크업을 버려서 만든다.
                let room = if budget == 0 { limit.saturating_sub(markup.reserve(v)) } else { budget };
                let take = room.saturating_sub(len).max(1);
                let end = rest
                    .char_indices()
                    .nth(take)
                    .map_or(rest.len(), |(i, _)| i);
                markup.feed(&rest[..end], v);
                len += push_after_reopen(&mut cur, &rest[..end], &mut markup);
                rest = &rest[end..];
                if !rest.is_empty() {
                    cut(&mut parts, &mut cur, &mut len, &mut markup, v, limit);
                }
            }
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

/// 조각에 글을 붙이고 붙은 글자 수를 돌려준다. 방금 다시 연 마커 바로 뒤라면 앞 공백을
/// 턴다 — `** 이어서` 는 열기가 아니다.
fn push_after_reopen(cur: &mut String, s: &str, markup: &mut Markup) -> usize {
    let s = if markup.fresh { s.trim_start() } else { s };
    if !s.is_empty() {
        markup.fresh = false;
    }
    cur.push_str(s);
    s.chars().count()
}

/// 조각을 끊는다. 열린 것을 닫고, 다음 조각 앞머리에서 다시 연다.
fn cut(
    parts: &mut Vec<String>,
    cur: &mut String,
    len: &mut usize,
    markup: &mut Markup,
    v: &Vocab,
    limit: usize,
) {
    if cur.is_empty() {
        return;
    }
    let mut part = std::mem::take(cur);
    // **태그 한가운데서는 끊지 않는다.** 조각 하나가 `… <a ` 로 끝나면 그 조각은
    // 그 자체로 깨진 HTML 이고, 채널은 메시지를 통째로 거절한다. 여는 태그가 아직
    // `>` 를 못 만났으면 그만큼 도로 빼서 다음 조각으로 넘긴다.
    let carry = match part.len().checked_sub(markup.partial.len()) {
        Some(at) if !markup.partial.is_empty() && at > 0 && part.ends_with(&markup.partial) => {
            part.split_off(at)
        }
        _ => String::new(),
    };
    // **닫는 마커 앞이 공백이면 닫기가 아니다.** 마크다운 채널은 `**굵은 말 **` 처럼
    // 공백 뒤에 닫으면 슬랙이 별표를 글자로 보인다(CommonMark 의 flanking). 조각 끝
    // 공백은 어차피 뜻이 없다 — 턴다. 다음 조각 앞머리의 공백도 같은 이유로 `split_hard`
    // 가 다시 연 마커 뒤에서 턴다.
    if v.channel != Channel::TelegramHtml {
        let kept = part.trim_end().len();
        part.truncate(kept);
    }
    markup.close_all(&mut part, v);
    parts.push(part);
    markup.reopen(cur, v);
    markup.fresh = v.channel != Channel::TelegramHtml && !cur.is_empty();
    // **다시 열 수 없는 마크업은 버린다.** 여는 태그만으로 조각이 차 버리면 내용이 한
    // 글자도 안 들어가고, 같은 자리에서 같은 조각을 끝없이 찍어 낸다. 마크업보다
    // 내용이 먼저다 — 여기서부터는 꾸밈 없이 내보낸다.
    let reopened = cur.chars().count() + carry.chars().count();
    if reopened >= limit {
        cur.clear();
        markup.forget();
        *len = 0;
        return;
    }
    cur.push_str(&carry);
    *len = reopened;
}

/// 지금 열려 있는 마크업. 조각을 끊을 때 닫고 다시 열려고 들고 있는다.
#[derive(Clone)]
struct Markup {
    /// 열린 HTML 태그. (이름, 여는 태그 전체)
    tags: Vec<(String, String)>,
    /// 열린 코드펜스의 info 문자열.
    fence: Option<String>,
    /// 조각 경계에 걸려 아직 `>` 를 못 만난 태그의 앞부분.
    ///
    /// **이걸 안 들고 있으면 태그를 통째로 놓친다.** 여기 들어오는 텍스트는 단어 단위로
    /// 잘려 있어서 `<code class="…">` 하나가 두 번에 나뉘어 들어온다. 놓친 태그는
    /// 닫히지도 다시 열리지도 않아서, 조각 하나가 통째로 깨진 HTML 이 된다.
    partial: String,
    /// 다음에 먹일 글자가 줄 첫머리인가.
    ///
    /// **인라인 코드 스팬의 울타리와 블록 펜스를 가르는 값이다.** 백틱을 담은 코드
    /// 스팬은 ` ``` ` 로 감싸는데, 그것이 조각 단위로 들어오면 줄 첫머리의 펜스와
    /// 구분되지 않는다. 그대로 두면 분할기가 없는 코드블록을 열고 닫는다.
    at_line_start: bool,
    /// 마크다운 채널의 인라인 스팬 — 코드 스팬과 `**` `*` `~~`. 블록 전체를 미리 훑어
    /// **짝이 맞는 것만** 적어 둔다(글자 위치 기준).
    ///
    /// **슬랙에서 조각이 코드 스팬 한가운데서 갈리던 것을 막는다.** 12,000자 분할이
    /// `` `main → main` `` 의 공백에 떨어지면 앞 조각은 백틱이 열린 채 끝나고 뒤 조각은
    /// 백틱 하나로 시작한다 — 두 메시지 다 코드가 깨진다. 태그와 같은 규칙으로, 끊는
    /// 자리에서 닫고 다음 조각에서 다시 연다.
    ///
    /// 단어 단위로 먹으면서 여닫기를 따라가는 대신 미리 훑는 이유는 **짝 없는 마커가
    /// 출력에 글자로 남기 때문**이다. 렌더러가 안 닫힌 백틱 런을 글자로 되돌리므로,
    /// 따라가기만 하면 그 ``` 를 열린 코드 스팬으로 알고 조각마다 펜스를 찍어 낸다.
    spans: std::rc::Rc<[Span]>,
    /// 지금까지 먹인 글자 수. `spans` 의 위치와 맞춰 본다.
    pos: usize,
    /// 다시 열 수 없어 버린 뒤다. 그 뒤로는 스팬을 닫지도 열지도 않는다.
    dropped: bool,
    /// 방금 마크다운 마커를 다시 열었고 아직 내용이 안 붙었다. 여는 마커 뒤가 공백이면
    /// 열기가 아니라서, 다음에 붙는 글의 앞 공백을 턴다.
    fresh: bool,
}

/// 짝이 맞는 인라인 스팬 하나. 위치는 글자 단위다.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    kind: SpanKind,
    /// 여는 마커의 첫 글자 위치.
    start: usize,
    /// 닫는 마커 다음 위치.
    end: usize,
}

/// 여는 마커. 코드 스팬은 백틱 런 길이, 강조는 마커 글자다.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SpanKind {
    Code(usize),
    Emph(&'static str),
}

impl SpanKind {
    fn write(self, out: &mut String) {
        match self {
            SpanKind::Code(run) => {
                for _ in 0..run {
                    out.push('`');
                }
            }
            SpanKind::Emph(m) => out.push_str(m),
        }
    }

    fn len(self) -> usize {
        match self {
            SpanKind::Code(run) => run,
            SpanKind::Emph(m) => m.len(),
        }
    }
}

/// 마크다운 출력 한 블록의 인라인 스팬을 찾는다.
///
/// 우리 렌더러가 낸 출력이라 마커는 짝이 맞고 겹침도 바르다(굵게 바깥, 기울임 안).
/// 글자로 남은 마커(`2 ** 3`, `\*`, 되돌린 백틱)는 짝이 안 맞아 여기서 걸러진다 —
/// 여는 쪽은 뒤가 글자, 닫는 쪽은 앞이 글자여야 하고, 코드 스팬은 같은 길이의 런이
/// 뒤에 있어야 한다. 펜스 안은 보지 않는다.
fn scan_spans(text: &str) -> Vec<Span> {
    let ch: Vec<char> = text.chars().collect();
    // 펜스 줄과 그 안은 스팬이 아니고, **스팬이 그 너머와 짝을 맺지도 못한다.** 백틱
    // 런의 짝을 찾을 때 펜스를 넘어가면 글자로 남은 ``` 가 코드블록의 펜스와 짝이 된다.
    let mut fenced = vec![false; ch.len()];
    {
        let mut fence = false;
        let mut i = 0;
        while i < ch.len() {
            let mut j = i;
            while j < ch.len() && (ch[j] == ' ' || ch[j] == '\t') {
                j += 1;
            }
            let line_end = ch[i..].iter().position(|&x| x == '\n').map_or(ch.len(), |p| i + p);
            let is_fence = ch[j..].starts_with(&['`', '`', '`']);
            if is_fence || fence {
                for f in &mut fenced[i..line_end] {
                    *f = true;
                }
            }
            if is_fence {
                fence = !fence;
            }
            i = line_end + 1;
        }
    }
    let mut spans = Vec::new();
    let mut open: Vec<(SpanKind, usize)> = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if fenced[i] {
            i += 1;
            continue;
        }
        // **줄이 바뀌면서 새 항목이 시작되면 열린 강조는 없던 일이다.** 조각 하나에 목록
        // 항목 여럿이 담기는데, 렌더러는 항목마다 인라인을 확정한다. 여기서 항목을 넘어
        // 짝을 맺으면 A 항목의 글자 `*` 가 B 항목의 `*` 와 스팬이 된다.
        if c == '\n' {
            let mut j = i + 1;
            while j < ch.len() && (ch[j] == ' ' || ch[j] == '\t') {
                j += 1;
            }
            let next = &ch[j..];
            let item = next.is_empty()
                || next[0] == '\n'
                || matches!(next, ['-' | '*' | '+' | '>' | '#', ' ', ..])
                || (next[0].is_ascii_digit()
                    && next.iter().skip(1).find(|c| !c.is_ascii_digit()).is_some_and(|c| *c == '.'));
            if item {
                open.clear();
            }
            i += 1;
            continue;
        }
        if c == '\\' {
            i += 2;
            continue;
        }
        if c == '`' {
            let run = ch[i..].iter().take_while(|&&x| x == '`').count();
            // **셋 이상의 런은 스팬으로 보지 않는다.** 렌더러는 내용에 백틱이 있을 때만
            // 울타리를 늘리는데 그건 드물고, 산문에 글자로 남은 ``` (한 줄에 쏟아낸 가짜
            // 펜스)는 흔하다. 그 둘이 짝을 맺으면 조각마다 펜스가 찍힌다 — 실측이다.
            if run >= 3 {
                i += run;
                continue;
            }
            // 같은 길이의 런이 이 블록 안에 더 있어야 코드 스팬이다. 없으면 글자다.
            let mut j = i + run;
            let close = loop {
                // **코드 스팬은 줄을 넘지 않는다.** 렌더러는 넘기지만, 짝 없이 글자로
                // 남은 백틱이 여러 항목 뒤의 다른 백틱과 스팬을 맺는 쪽이 훨씬 흔하다 —
                // 실측에서 그 가짜 스팬이 항목 여럿을 덮고 조각마다 백틱을 찍었다.
                match ch[j..].iter().position(|&x| x == '`' || x == '\n') {
                    None => break None,
                    Some(p) if ch[j + p] == '\n' => break None,
                    // 펜스에 닿았다 — 짝은 없다.
                    Some(p) if fenced[j + p] => break None,
                    Some(p) => {
                        let at = j + p;
                        let n = ch[at..].iter().take_while(|&&x| x == '`').count();
                        if n == run {
                            break Some(at);
                        }
                        j = at + n;
                    }
                }
            };
            match close {
                Some(at) => {
                    spans.push(Span { kind: SpanKind::Code(run), start: i, end: at + run });
                    i = at + run;
                }
                None => i += run,
            }
            continue;
        }
        if !matches!(c, '*' | '~') {
            i += 1;
            continue;
        }
        let run = ch[i..].iter().take_while(|&&x| x == c).count();
        let prev = if i > 0 { Some(ch[i - 1]) } else { None };
        let next = ch.get(i + run).copied();
        let opens = next.is_some_and(|n| !n.is_whitespace());
        let closes = prev.is_some_and(|p| !p.is_whitespace());
        // `***` 는 `**` 와 `*` 다. 열 때는 굵게가 바깥, 닫을 때는 기울임이 먼저.
        let markers: &[&'static str] = match (c, run) {
            ('~', 2) => &["~~"],
            ('*', 1) => &["*"],
            ('*', 2) => &["**"],
            ('*', 3) => {
                if open.last().map(|o| o.0) == Some(SpanKind::Emph("*")) {
                    &["*", "**"]
                } else {
                    &["**", "*"]
                }
            }
            _ => &[],
        };
        let mut at = i;
        for m in markers {
            let kind = SpanKind::Emph(m);
            if closes && open.last().map(|o| o.0) == Some(kind) {
                let (_, start) = open.pop().expect("방금 확인했다");
                spans.push(Span { kind, start, end: at + m.len() });
            } else if opens {
                open.push((kind, at));
            }
            at += m.len();
        }
        i += run;
    }
    spans.sort_by_key(|s| s.start);
    spans
}

impl Default for Markup {
    fn default() -> Self {
        Self {
            tags: Vec::new(),
            fence: None,
            partial: String::new(),
            // 블록은 줄 첫머리에서 시작한다.
            at_line_start: true,
            spans: std::rc::Rc::from(Vec::new()),
            pos: 0,
            dropped: false,
            fresh: false,
        }
    }
}

impl Markup {
    fn feed(&mut self, s: &str, v: &Vocab) {
        if v.channel == Channel::TelegramHtml {
            self.feed_html(s);
        } else {
            self.feed_fence(s);
            self.pos += s.chars().count();
        }
    }

    /// 지금 위치에서 열려 있는 스팬. 바깥부터 순서대로.
    fn open_spans(&self) -> impl Iterator<Item = &Span> {
        let pos = self.pos;
        let dropped = self.dropped;
        self.spans.iter().filter(move |sp| !dropped && sp.start < pos && pos < sp.end)
    }

    fn feed_html(&mut self, s: &str) {
        let joined;
        let text = if self.partial.is_empty() {
            s
        } else {
            joined = format!("{}{}", self.partial, s);
            self.partial.clear();
            &joined
        };
        let ch: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < ch.len() {
            if ch[i] != '<' {
                i += 1;
                continue;
            }
            let start = i;
            let closing = ch.get(i + 1) == Some(&'/');
            let from = if closing { i + 2 } else { i + 1 };
            let mut j = from;
            while j < ch.len() && (ch[j].is_ascii_alphanumeric() || ch[j] == '-') {
                j += 1;
            }
            let name: String = ch[from..j].iter().collect();
            while j < ch.len() && ch[j] != '>' {
                j += 1;
            }
            if j >= ch.len() {
                // `>` 를 아직 못 봤다. 다음 조각과 이어 붙여서 다시 본다.
                self.partial = ch[start..].iter().collect();
                return;
            }
            if name.is_empty() {
                i += 1;
                continue;
            }
            if closing {
                if let Some(at) = self.tags.iter().rposition(|(n, _)| *n == name) {
                    self.tags.truncate(at);
                }
            } else {
                self.tags.push((name, ch[start..=j].iter().collect()));
            }
            i = j + 1;
        }
    }

    fn feed_fence(&mut self, s: &str) {
        for (i, line) in s.split('\n').enumerate() {
            // 줄 첫머리에 선 것만 펜스다. 문장 한가운데의 ` ``` ` 는 인라인 울타리다.
            if i == 0 && !self.at_line_start {
                continue;
            }
            let t = line.trim_start();
            if t.starts_with("```") {
                self.fence = match self.fence {
                    Some(_) => None,
                    None => Some(t.trim_start_matches('`').to_string()),
                };
            }
        }
        if !s.is_empty() {
            // 공백만 먹었으면 아직 줄 첫머리다. 들여쓴 펜스(`  ```json`)는 단어 단위로
            // 들어올 때 공백이 먼저 오는데, 여기서 첫머리를 잃으면 펜스를 못 알아보고
            // 스팬 추적이 그 ``` 를 코드 스팬 열기로 읽는다.
            self.at_line_start = s.ends_with('\n') || (self.at_line_start && s.trim().is_empty());
        }
    }

    /// 지금 끊으면 닫고 다시 여는 데 드는 글자 수. 미리 빼 두지 않으면 한도를 넘긴다.
    fn reserve(&self, v: &Vocab) -> usize {
        let mut n: usize = self.tags.iter().map(|(name, full)| name.chars().count() + 3 + full.chars().count()).sum();
        if let Some(info) = &self.fence {
            // 닫는 펜스 + 다시 여는 펜스.
            n += 4 + 4 + info.chars().count();
        }
        // 스팬은 닫는 마커와 다시 여는 마커, 두 번.
        n += self.open_spans().map(|sp| sp.kind.len() * 2).sum::<usize>();
        let _ = v;
        n
    }

    fn close_all(&self, out: &mut String, v: &Vocab) {
        // 안쪽부터 — 늦게 열린 것이 안쪽이다.
        let open: Vec<&Span> = self.open_spans().collect();
        for sp in open.iter().rev() {
            sp.kind.write(out);
        }
        for (name, _) in self.tags.iter().rev() {
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        if self.fence.is_some() {
            v.verbatim_close("", out);
        }
    }

    /// 열린 것을 없던 일로 한다. 이미 닫아서 내보낸 뒤에만 부른다.
    fn forget(&mut self) {
        self.tags.clear();
        self.fence = None;
        self.partial.clear();
        self.dropped = true;
    }

    fn reopen(&self, out: &mut String, _v: &Vocab) {
        if let Some(info) = &self.fence {
            out.push_str("```");
            out.push_str(info);
            out.push('\n');
        }
        for (_, full) in &self.tags {
            out.push_str(full);
        }
        for sp in self.open_spans() {
            sp.kind.write(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(limit: usize, text: &str) -> Vec<String> {
        split_hard(text, limit, &Vocab::new(Channel::TelegramHtml))
    }

    /// 조각 하나가 곧 메시지 하나다 — 태그가 그 안에서 열리고 닫혀야 한다.
    fn balanced(part: &str) -> bool {
        let ch: Vec<char> = part.chars().collect();
        let mut stack: Vec<String> = Vec::new();
        let mut i = 0;
        while i < ch.len() {
            if ch[i] != '<' {
                i += 1;
                continue;
            }
            let closing = ch.get(i + 1) == Some(&'/');
            let from = if closing { i + 2 } else { i + 1 };
            let mut j = from;
            while j < ch.len() && ch[j].is_ascii_alphanumeric() {
                j += 1;
            }
            let name: String = ch[from..j].iter().collect();
            while j < ch.len() && ch[j] != '>' {
                j += 1;
            }
            if j >= ch.len() {
                return false; // 태그가 반으로 잘렸다
            }
            if closing {
                if stack.pop().as_deref() != Some(name.as_str()) {
                    return false;
                }
            } else {
                stack.push(name);
            }
            i = j + 1;
        }
        stack.is_empty()
    }

    /// **끊어도 자리가 안 생기면 글자로 끊는다.**
    ///
    /// 마크업을 다시 여는 데 드는 글자가 끊어서 번 자리를 도로 먹으면, 끊기만 해서는
    /// 진도가 안 나간다 — 예전에는 여기서 같은 조각을 끝없이 찍어 냈다.
    #[test]
    fn splitting_makes_progress_inside_open_markup() {
        let text = format!("<blockquote>글 {} 끝</blockquote>", "y".repeat(600));
        let parts = html(200, &text);
        assert!(parts.len() < 10, "조각이 {}개 — 진도가 안 나갔다", parts.len());
        for (i, p) in parts.iter().enumerate() {
            assert!(balanced(p), "조각 {i} 의 태그가 안 맞는다: {p:?}");
        }
        assert!(parts.concat().contains("끝"), "뒤쪽 내용이 사라졌다");
    }

    /// 여는 태그 하나가 한도만 하면 다시 열 수 없다. 마크업을 버리고서라도 나아간다.
    ///
    /// 이 자리에서는 조각이 유효한 HTML 이 되지 못한다 — 애초에 담을 수 없는 태그라서다.
    /// 그래서 **이런 태그를 만들지 않는 것**이 `Vocab::link` 의 몫이고, 여기는 최후의
    /// 안전장치로 "끝나기는 한다"만 지킨다.
    #[test]
    fn splitting_gives_up_on_markup_that_cannot_reopen() {
        let url = "https://e.com/".to_string() + &"x".repeat(400);
        let text = format!("<a href=\"{url}\">아주 긴 링크</a> 뒤에 오는 글");
        let parts = html(200, &text);
        assert!(parts.len() < 10, "조각이 {}개 — 진도가 안 나갔다", parts.len());
        assert!(parts.concat().contains("뒤에 오는 글"), "뒤쪽 내용이 사라졌다");
    }
}
