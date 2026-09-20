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
                    cur.push_str(rest);
                    len += n;
                    break;
                }
                if len > 0 {
                    cut(&mut parts, &mut cur, &mut len, &markup, v);
                    continue;
                }
                // 조각이 비었는데도 안 들어간다. 공백 하나 없는 덩어리다 — 글자로 끊는다.
                let take = budget.max(1);
                let end = rest
                    .char_indices()
                    .nth(take)
                    .map_or(rest.len(), |(i, _)| i);
                markup.feed(&rest[..end], v);
                cur.push_str(&rest[..end]);
                len += take;
                rest = &rest[end..];
                if !rest.is_empty() {
                    cut(&mut parts, &mut cur, &mut len, &markup, v);
                }
            }
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

/// 조각을 끊는다. 열린 것을 닫고, 다음 조각 앞머리에서 다시 연다.
fn cut(parts: &mut Vec<String>, cur: &mut String, len: &mut usize, markup: &Markup, v: &Vocab) {
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
    markup.close_all(&mut part, v);
    parts.push(part);
    markup.reopen(cur, v);
    cur.push_str(&carry);
    *len = cur.chars().count();
}

/// 지금 열려 있는 마크업. 조각을 끊을 때 닫고 다시 열려고 들고 있는다.
#[derive(Default, Clone)]
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
}

impl Markup {
    fn feed(&mut self, s: &str, v: &Vocab) {
        if v.channel == Channel::TelegramHtml {
            self.feed_html(s);
        } else {
            self.feed_fence(s);
        }
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
        for line in s.split('\n') {
            let t = line.trim_start();
            if t.starts_with("```") {
                self.fence = match self.fence {
                    Some(_) => None,
                    None => Some(t.trim_start_matches('`').to_string()),
                };
            }
        }
    }

    /// 지금 끊으면 닫고 다시 여는 데 드는 글자 수. 미리 빼 두지 않으면 한도를 넘긴다.
    fn reserve(&self, v: &Vocab) -> usize {
        let mut n: usize = self.tags.iter().map(|(name, full)| name.chars().count() + 3 + full.chars().count()).sum();
        if let Some(info) = &self.fence {
            // 닫는 펜스 + 다시 여는 펜스.
            n += 4 + 4 + info.chars().count();
        }
        let _ = v;
        n
    }

    fn close_all(&self, out: &mut String, v: &Vocab) {
        for (name, _) in self.tags.iter().rev() {
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        if self.fence.is_some() {
            v.verbatim_close("", out);
        }
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
    }
}
