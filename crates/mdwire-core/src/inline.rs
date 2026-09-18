//! 인라인 파서 — 강조의 짝을 맞춘다.
//!
//! **이 파일이 이 라이브러리의 이유다.** 정규식 변환기가 못 하는 일이 여기 있다.
//!
//! 규칙은 셋이다.
//!
//! 1. **flanking 판정으로 열기/닫기를 가른다.** 줄 첫머리의 `**` 는 앞이 줄바꿈이므로
//!    닫기가 될 수 없다. 정규식이 이걸 못 해서 뒤의 `**` 와 잘못 짝짓고, 강조 범위가
//!    뒤집혔다(`DESIGN.md`).
//! 2. **이미 열린 것과 같은 종류를 또 열려고 하면 버린다.** 겹쳐 열 이유가 없고,
//!    이 자리에서 "닫기"로 읽는 순간 범위가 뒤집힌다.
//! 3. **블록이 끝나면 열린 것을 닫는다**(`SPEC.md` 6절). 단, 양쪽이 다 공백이라
//!    추측으로 연 것은 글자로 되돌린다 — `2 ** 3` 을 굵게 만들지 않으려는 것이다.
//!
//! # 스트리밍
//!
//! 출력은 `out` 에 붙여 나가되, **열린 마커가 있으면 그 자리부터는 내보내지 않는다**
//! ([`Inline::safe_len`]). 여는 마크업은 짝이 맞는 순간 그 자리에 끼워 넣는다.
//! 그래서 조각 경계에 걸린 강조가 반쪽으로 나가는 일이 없다.

use crate::vocab::{is_wide, Emph, Vocab, ZWSP};

struct Open {
    emph: Emph,
    /// `out` 안에서 여는 마크업이 들어갈 자리.
    at: usize,
    /// 마커 길이. 코드 스팬은 백틱 런의 길이를 그대로 쓴다(닫을 때 같아야 한다).
    run: usize,
    ch: char,
    /// 추측으로 열었는가. 안 닫히면 글자로 되돌린다.
    guess: bool,
    /// 여는 쪽 CJK 패딩이 필요한가.
    pad: bool,
}

pub(crate) struct Inline {
    open: Vec<Open>,
    /// 줄을 넘어온 직전 글자. 블록 안에서 줄이 바뀌면 `'\n'` 이다.
    /// 이 값이 있어야 **줄 첫머리의 마커가 닫기가 아니라는 판정**이 선다.
    prev: Option<char>,
    /// 링크 텍스트를 렌더할 때만 쓰는 버퍼. 재사용해서 할당을 아낀다.
    scratch: String,
}

impl Inline {
    pub fn new() -> Self {
        Self { open: Vec::new(), prev: None, scratch: String::new() }
    }

    /// 블록 경계. 인라인 상태는 블록을 넘지 않는다.
    pub fn reset(&mut self) {
        self.open.clear();
        self.prev = None;
    }

    /// 지금 `out` 에서 **내보내도 안전한 길이**. 열린 마커가 있으면 그 앞까지다.
    pub fn safe_len(&self, out_len: usize) -> usize {
        self.open.first().map_or(out_len, |o| o.at)
    }

    /// 앞쪽 `n` 바이트를 내보냈다. 기억하고 있던 자리를 당긴다.
    pub fn shift(&mut self, n: usize) {
        for o in &mut self.open {
            o.at -= n;
        }
    }

    /// 줄 하나가 끝났다. 다음 줄의 첫 글자에게 앞 글자는 줄바꿈이다.
    pub fn end_line(&mut self) {
        self.prev = Some('\n');
    }

    /// 블록 접두사(`- `, `> ` 따위)를 건너뛴 뒤의 앞 글자를 세운다.
    /// 마커가 줄 첫머리에 왔다는 판정이 이 값으로 선다.
    pub fn set_prev(&mut self, c: Option<char>) {
        self.prev = c;
    }

    pub fn is_open(&self) -> bool {
        !self.open.is_empty()
    }

    /// 인라인을 렌더한다.
    ///
    /// 한 줄을 여러 번에 나눠 넣어도 된다 — 스트리밍에서는 그렇게 부른다. 대신
    /// **마커 런이 조각 끝에 걸친 채로 들어오면 안 된다.** 마커는 다음 글자를 봐야
    /// 열기/닫기가 갈리기 때문이고, 그 판단은 호출자(`block::Engine`)가 한다.
    pub fn render(&mut self, line: &[char], out: &mut String, v: &Vocab) {
        let mut i = 0;
        while i < line.len() {
            // 코드 스팬 안에서는 강조 마커가 글자다.
            if let Some(top) = self.open.last() {
                if top.emph == Emph::Code {
                    let run = if line[i] == '`' { run_len(line, i, '`') } else { 0 };
                    if run == top.run {
                        self.close_at(self.open.len() - 1, out, v, line.get(i + run).copied());
                        i += run;
                    } else {
                        v.escape_char(line[i], out);
                        i += 1;
                    }
                    continue;
                }
            }

            let c = line[i];

            if c == '[' {
                if let Some((text, url_from, url_to)) = find_link(line, i) {
                    self.render_link(&line[text.0..text.1], &line[url_from..url_to], out, v);
                    i = url_to + 1;
                    continue;
                }
            }

            if c == '`' {
                let run = run_len(line, i, '`');
                self.open.push(Open {
                    emph: Emph::Code,
                    at: out.len(),
                    run,
                    ch: '`',
                    guess: false,
                    pad: v.pad && self.prev_char(line, i).is_some_and(is_wide),
                });
                i += run;
                continue;
            }

            if !matches!(c, '*' | '_' | '~') {
                v.escape_char(c, out);
                i += 1;
                continue;
            }

            let run = run_len(line, i, c);
            let take = run.min(2);
            let emph = match (c, take) {
                ('~', _) => Emph::Strike,
                (_, 2) => Emph::Bold,
                _ => Emph::Italic,
            };
            let prev = self.prev_char(line, i);
            let next = line.get(i + take).copied();

            // `snake_case` 의 밑줄은 강조가 아니다.
            if c == '_' && prev.is_some_and(is_word) && next.is_some_and(is_word) {
                for _ in 0..take {
                    v.escape_char(c, out);
                }
                i += take;
                continue;
            }

            let (left, right) = flanking(prev, next);
            let same = self.open.iter().rposition(|o| o.emph == emph);

            match (right, same) {
                (true, Some(at)) => self.close_at(at, out, v, next),
                _ if left => {
                    if same.is_none() {
                        self.open.push(Open {
                            emph,
                            at: out.len(),
                            run: take,
                            ch: c,
                            guess: false,
                            pad: v.pad && prev.is_some_and(is_wide),
                        });
                    }
                    // 같은 종류가 이미 열려 있으면 버린다. 규칙 2 — 여기서 닫으면 범위가 뒤집힌다.
                }
                (_, Some(at)) => self.close_at(at, out, v, next),
                (_, None) => self.open.push(Open {
                    emph,
                    at: out.len(),
                    run: take,
                    ch: c,
                    guess: true,
                    pad: v.pad && prev.is_some_and(is_wide),
                }),
            }
            i += take;
        }
        if let Some(&last) = line.last() {
            // 다음 호출의 첫 글자에게 앞 글자를 남긴다. 한 줄을 나눠 넣어도
            // flanking 판정이 이어지는 이유다.
            self.prev = Some(last);
        }
    }

    /// 블록이 끝났다. 열린 것을 전부 정리한다.
    pub fn finish_block(&mut self, out: &mut String, v: &Vocab) {
        while !self.open.is_empty() {
            self.close_at(self.open.len() - 1, out, v, None);
        }
        self.prev = None;
    }

    fn prev_char(&self, line: &[char], i: usize) -> Option<char> {
        if i > 0 {
            Some(line[i - 1])
        } else {
            self.prev
        }
    }

    /// `at` 번째 열린 마커를 닫는다. 그 위에 열린 것들은 먼저 정리한다.
    fn close_at(&mut self, at: usize, out: &mut String, v: &Vocab, next: Option<char>) {
        while self.open.len() > at + 1 {
            self.finalize(out, v, None);
        }
        self.finalize(out, v, next);
    }

    /// 맨 위 마커 하나를 확정한다 — 닫거나, 글자로 되돌리거나.
    fn finalize(&mut self, out: &mut String, v: &Vocab, next: Option<char>) {
        let Some(open) = self.open.pop() else { return };

        // 내용이 비었으면 태그를 만들지 않는다. `<b></b>` 는 아무에게도 쓸모가 없다.
        let empty = out.len() == open.at;
        if open.guess || empty || (open.emph == Emph::Code && empty) {
            // 추측이 빗나갔다 — 마커를 글자로 되돌린다.
            for _ in 0..open.run {
                out.insert(open.at, open.ch);
            }
            return;
        }

        out.insert_str(open.at, v.open(open.emph));
        if open.pad {
            out.insert(open.at, ZWSP);
        }
        out.push_str(v.close(open.emph));
        if v.pad && next.is_some_and(is_wide) {
            out.push(ZWSP);
        }
    }

    fn render_link(&mut self, text: &[char], url: &[char], out: &mut String, v: &Vocab) {
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        // 링크 텍스트는 자기만의 인라인 상태로 렌더한다. 바깥 강조와 섞이지 않는다.
        let mut nested = Inline::new();
        nested.render(text, &mut scratch, v);
        nested.finish_block(&mut scratch, v);

        let mut href = String::with_capacity(url.len());
        href.extend(url.iter());
        v.link(&scratch, &href, out);

        self.scratch = scratch;
    }
}

/// `[텍스트](url)` 을 찾는다. **한 줄 안에서만** 본다(`SPEC.md` 8절).
fn find_link(line: &[char], at: usize) -> Option<((usize, usize), usize, usize)> {
    let mut j = at + 1;
    let mut depth = 0usize;
    while j < line.len() {
        match line[j] {
            '[' => depth += 1,
            ']' if depth == 0 => break,
            ']' => depth -= 1,
            _ => {}
        }
        j += 1;
    }
    if j >= line.len() || line.get(j + 1) != Some(&'(') {
        return None;
    }
    let k = line[j + 2..].iter().position(|&c| c == ')')? + j + 2;
    Some(((at + 1, j), j + 2, k))
}

fn run_len(line: &[char], at: usize, c: char) -> usize {
    line[at..].iter().take_while(|&&x| x == c).count()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// CommonMark 의 좌/우 flanking 판정.
///
/// `None` 은 블록의 끝(또는 시작)이고 공백처럼 다룬다. 줄 끝에서 다음 글자가 없는 것과
/// 다음 줄이 이어지는 것은 같은 판정을 받아야 한다 — 그래야 wrap 이 결과를 바꾸지 않는다.
fn flanking(prev: Option<char>, next: Option<char>) -> (bool, bool) {
    let left = next.is_some_and(|n| {
        !n.is_whitespace() && (!is_punct(n) || prev.is_none_or(|p| p.is_whitespace() || is_punct(p)))
    });
    let right = prev.is_some_and(|p| {
        !p.is_whitespace() && (!is_punct(p) || next.is_none_or(|n| n.is_whitespace() || is_punct(n)))
    });
    (left, right)
}

fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(c,
            '，' | '。' | '、' | '！' | '？' | '；' | '：' | '·' | '…' | '—' | '～'
            | '「' | '」' | '『' | '』' | '（' | '）' | '【' | '】' | '《' | '》'
            | '“' | '”' | '‘' | '’')
}
