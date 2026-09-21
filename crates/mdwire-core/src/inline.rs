//! 인라인 파서 — 강조의 짝을 맞춘다.
//!
//! **이 파일이 이 라이브러리의 이유다.** 정규식 변환기가 못 하는 일이 여기 있다.
//!
//! 규칙은 셋이다.
//!
//! 1. **같은 종류가 열려 있고 앞이 공백이 아니면 닫는다.** 줄 첫머리의 `**` 는 앞이
//!    줄바꿈이라 닫기가 될 수 없다. 정규식이 이걸 못 해서 뒤의 `**` 와 잘못 짝짓고,
//!    강조 범위가 뒤집혔다(`DESIGN.md`).
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

use crate::vocab::{needs_cjk_padding, Emph, Vocab, ZWSP};

struct Open {
    emph: Emph,
    /// `out` 안에서 여는 마크업이 들어갈 자리.
    at: usize,
    /// 마커 길이. 코드 스팬은 백틱 런의 길이를 그대로 쓴다(닫을 때 같아야 한다).
    run: usize,
    ch: char,
    /// 추측으로 열었는가. 안 닫히면 글자로 되돌리거나 버린다.
    guess: bool,
    /// 이 마커 앞이 공백(또는 블록 시작)이었는가.
    ///
    /// 추측이 빗나갔을 때 **되돌릴지 버릴지**를 이 값이 가른다. `2 ** 3` 처럼 앞이
    /// 공백이면 원래 글자였으므로 되돌리고, `…온다*` 처럼 앞이 글자면 짝 잃은 닫는
    /// 마커이므로 버린다 — 되돌려 놓으면 출력에 마커가 남는다.
    after_space: bool,
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
    /// 지금 열려 있는 코드 스팬의 **날것 내용**.
    ///
    /// 블록이 끝나도록 닫는 런이 안 오면 그 백틱은 코드가 아니라 글자였다는 뜻이다.
    /// 그때 삼킨 내용을 도로 꺼내 다시 읽으려고 들고 있는다. 코드 스팬은 겹쳐 열리지
    /// 않아서 하나면 충분하고, 비우기만 하고 버리지 않아 할당이 다시 들지 않는다.
    ///
    /// `Vec<char>` 인 것은 다시 읽을 때 `render` 에 그대로 넘기기 위해서다 — `String`
    /// 으로 두면 되돌릴 때마다 글자 벡터를 새로 만들어야 한다.
    code_src: Vec<char>,
}

impl Inline {
    pub fn new() -> Self {
        Self {
            open: Vec::new(),
            prev: None,
            scratch: String::new(),
            code_src: Vec::new(),
        }
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

    /// 블록 층이 `out` 에 바로 쓴 글자를 코드 스팬 내용에도 남긴다.
    ///
    /// **줄 사이의 구분자는 `render` 를 거치지 않는다.** 줄바꿈·인용 접두사·이어지는
    /// 항목의 들여쓰기는 블록 층이 `out` 에 직접 쓴다. 코드 스팬이 열려 있는 동안
    /// 그것을 안 남겨 두면, 되돌려 다시 읽을 때 두 줄이 한 줄로 붙는다.
    pub fn note_raw(&mut self, s: &str) {
        if self.open.last().is_some_and(|o| o.emph == Emph::Code) {
            self.code_src.extend(s.chars());
        }
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
                        // **안 맞는 백틱 런은 통째로 건너뛴다.** 한 글자씩 넘기면 길이 N+1 인
                        // 런 안에 길이 N 인 런이 들어 있는 꼴이 되어, 그 안쪽에서 잘못 닫는다.
                        // `` ` foo `` bar ` `` 의 내용이 "foo `" 로 잘리던 것이 이것이다.
                        let step = run.max(1);
                        for k in 0..step {
                            v.escape_char(line[i + k], out);
                            self.code_src.push(line[i + k]);
                        }
                        i += step;
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

            // **역슬래시 탈출.** `\_` 는 밑줄 한 글자지 강조 마커가 아니다. 안 보면
            // `TEST\_VCLEFT\_FRONT` 가 기울임이 되고 역슬래시까지 출력에 남는다 —
            // 실제 문서에서 그러고 있었다.
            if c == '\\' {
                if let Some(&next) = line.get(i + 1) {
                    if next.is_ascii_punctuation() {
                        v.literal(next, out);
                        self.prev = Some(next);
                        i += 2;
                        continue;
                    }
                }
            }

            if c == '`' {
                let run = run_len(line, i, '`');
                let prev = self.prev_char(line, i);
                self.code_src.clear();
                self.open.push(Open {
                    emph: Emph::Code,
                    at: out.len(),
                    run,
                    ch: '`',
                    guess: false,
                    after_space: prev.is_none_or(char::is_whitespace),
                    pad: v.pad && prev.is_some_and(needs_cjk_padding),
                });
                i += run;
                continue;
            }

            if !matches!(c, '*' | '_' | '~') {
                v.escape_char(c, out);
                i += 1;
                continue;
            }

            // 런은 통째로 소비한다. `***강조***` 에서 2 개만 집으면 별표 하나가
            // 출력에 남고, 남은 마커는 곧 실패다. 중첩 강조를 살리는 것보다
            // 마커를 안 남기는 것이 먼저다 — 실측에서 중첩 강조는 나오지 않았다.
            let take = run_len(line, i, c);
            let emph = match (c, take) {
                ('~', _) => Emph::Strike,
                (_, 1) => Emph::Italic,
                _ => Emph::Bold,
            };
            // 취소선은 `~~` 다. **홀로 선 `~` 는 글자다** — `~40km`, `5~6월` 처럼
            // 한국어에서 물결표는 근사값과 범위에 늘 쓰인다. 이걸 취소선으로 읽으면
            // 물결표가 사라지고 멀쩡한 문장이 통째로 그어진다.
            //
            // `~~` 로 한정해도 잃는 것이 없는 이유는 **그게 문법이기 때문**이다 —
            // GFM 도 슬랙 `markdown_text` 도 취소선을 `~~` 로 적는다. 홀로 선 `~` 를
            // 취소선으로 읽는 것은 레거시 `mrkdwn` 과 MarkdownV2 의 *출력* 규칙이지
            // 입력 문법이 아니다.
            if c == '~' && take < 2 {
                v.escape_char(c, out);
                i += 1;
                continue;
            }

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

            let left = can_open(prev, next);
            let after_space = prev.is_none_or(char::is_whitespace);
            let same = self.open.iter().rposition(|o| o.emph == emph);
            let pad = v.pad && prev.is_some_and(needs_cjk_padding);

            match same {
                // 같은 종류가 열려 있고 앞이 공백이 아니면 여기가 닫는 자리다. 규칙 1.
                Some(at) if !after_space => self.close_at(at, out, v, next),
                // 앞이 공백인데 뒤로는 열 수 있다 — 줄 첫머리에 온 여는 마커다.
                // **여기서 닫으면 강조 범위가 뒤집힌다.** 겹쳐 열지도 않고 버린다. 규칙 2.
                Some(_) if left => {}
                Some(at) => self.close_at(at, out, v, next),
                None if left => self.open.push(Open {
                    emph,
                    at: out.len(),
                    run: take,
                    ch: c,
                    guess: false,
                    after_space,
                    pad,
                }),
                // 열 수도 닫을 수도 없다. 일단 열어 두고 안 닫히면 글자로 되돌린다. 규칙 3.
                None => self.open.push(Open {
                    emph,
                    at: out.len(),
                    run: take,
                    ch: c,
                    guess: true,
                    after_space,
                    pad,
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
        while let Some(top) = self.open.last() {
            if top.emph == Emph::Code {
                self.revert_code_span(out, v);
                continue;
            }
            self.close_at(self.open.len() - 1, out, v, None);
        }
        self.prev = None;
    }

    /// 안 닫힌 코드 스팬을 글자로 되돌린다.
    ///
    /// **블록이 끝나도록 닫는 런이 안 왔으면 그 백틱은 코드가 아니었다.** 그대로
    /// `<code>` 로 닫아 버리면 뒤에 오던 강조가 통째로 코드 안에 갇힌다 — 실제 문서에서
    /// `앞 ``` 뒤에 **굵게**` 의 굵게가 사라지고 있었다.
    ///
    /// 백틱만 되돌리고 끝내면 안 된다. 삼킨 내용은 코드로 읽혀서 강조가 안 걸린
    /// 상태다 — **도로 꺼내 다시 읽어야** 그 안의 강조가 산다.
    fn revert_code_span(&mut self, out: &mut String, v: &Vocab) {
        let Some(open) = self.open.pop() else { return };
        out.truncate(open.at);
        for _ in 0..open.run {
            out.push(open.ch);
        }
        // 버퍼를 통째로 빌려 와서 다시 읽고 돌려준다. 새로 만들지 않는다.
        let src = std::mem::take(&mut self.code_src);
        // 다시 읽는 내용은 백틱 바로 뒤에서 시작한다.
        self.prev = Some(open.ch);
        self.render(&src, out, v);
        // 다시 읽는 동안 새 코드 스팬이 열렸으면 그쪽 버퍼를 지키고, 아니면 돌려준다.
        if self.code_src.is_empty() {
            self.code_src = src;
            self.code_src.clear();
        }
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
        if open.guess || empty {
            // 추측이 빗나갔다. 앞이 공백이었으면 원래 글자였던 것이니 되돌리고,
            // 앞이 글자였으면 짝 잃은 닫는 마커이니 버린다 — 되돌리면 출력에 남는다.
            // 내용이 빈 홑마커(`참고*` 의 꼬리 같은 것)는 글자로 남긴다. `**` 는 버린다 —
            // 홑마커는 각주나 곱셈으로 쓰이지만 `**` 가 홀로 남을 이유는 없다.
            if open.after_space || (empty && open.run == 1) {
                for _ in 0..open.run {
                    out.insert(open.at, open.ch);
                }
            }
            return;
        }

        // **코드 스팬은 앞뒤 공백 하나를 벗긴다.** `` ` `` 처럼 내용이 백틱으로
        // 시작하거나 끝날 때 마커와 붙지 않게 끼워 넣는 공백이라, 내용이 아니다
        // (CommonMark). 안 벗기면 `<code> ` </code>` 처럼 없던 공백이 남는다.
        // 마커를 지우는 채널에서는 벗기지 않는다. 마커가 없으면 그 공백이 곧 낱말
        // 경계라, 벗기면 앞뒤 글자가 붙어 버린다.
        if open.emph == Emph::Code && !v.is_plain() {
            let body = &out[open.at..];
            if body.len() >= 2
                && body.starts_with(' ')
                && body.ends_with(' ')
                && !body.trim().is_empty()
            {
                out.truncate(out.len() - 1);
                out.remove(open.at);
            }
        }
        // **내용에 백틱이 있으면 울타리를 늘린다.** 마크다운을 그대로 내보내는
        // 채널에서 백틱 하나로 감싸면 ``` ``` ``` 가 되어 코드 블록으로 읽힌다.
        // 내용 안의 가장 긴 런보다 하나 긴 울타리를 쓰고, 내용이 백틱으로 시작하거나
        // 끝나면 공백을 하나 끼워 마커와 떼어 놓는다(CommonMark).
        if open.emph == Emph::Code && v.open(Emph::Code) == "`" {
            let body = &out[open.at..];
            let longest = longest_run(body, '`');
            if longest > 0 {
                let pad = body.starts_with('`') || body.ends_with('`');
                if pad {
                    out.push(' ');
                }
                for _ in 0..longest + 1 {
                    out.push('`');
                }
                if pad {
                    out.insert(open.at, ' ');
                }
                for _ in 0..longest + 1 {
                    out.insert(open.at, '`');
                }
                return;
            }
        }
        out.insert_str(open.at, v.open(open.emph));
        if open.pad {
            out.insert(open.at, ZWSP);
        }
        out.push_str(v.close(open.emph));
        if v.pad && next.is_some_and(needs_cjk_padding) {
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

/// 같은 글자가 이어진 가장 긴 길이.
fn longest_run(s: &str, c: char) -> usize {
    let mut best = 0;
    let mut cur = 0;
    for x in s.chars() {
        if x == c {
            cur += 1;
            best = best.max(cur);
        } else {
            cur = 0;
        }
    }
    best
}

fn run_len(line: &[char], at: usize, c: char) -> usize {
    line[at..].iter().take_while(|&&x| x == c).count()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric()
}

/// 이 마커가 **열 수 있는가**(CommonMark 의 좌측 flanking).
///
/// `None` 은 블록의 끝(또는 시작)이고 공백처럼 다룬다. 줄 끝에서 다음 글자가 없는 것과
/// 다음 줄이 이어지는 것은 같은 판정을 받아야 한다 — 그래야 wrap 이 결과를 바꾸지 않는다.
///
/// # 닫는 쪽은 왜 우측 flanking 이 아닌가
///
/// CommonMark 은 **앞이 구두점이고 뒤가 글자면 닫지 못하게** 한다. 한국어 출력이 거기
/// 정면으로 걸린다 — 강조 끝에 조사가 붙고 그 앞이 구두점인 모양이 흔하기 때문이다.
///
/// ```text
/// **끝.**이라서     **(중요)**이다     **`코드`**였다
/// ```
///
/// 이 규칙을 그대로 따르면 셋 다 닫히지 못하고 **강조가 블록 끝까지 번져 뒤의 무관한
/// 텍스트를 삼킨다.** 우리 목적은 스펙 준수가 아니라 복구다. 그래서 닫는 판정은
/// "같은 종류가 열려 있고 앞이 공백이 아니면 닫는다" 하나로 간다. 앞이 공백인 경우를
/// 빼는 것이 핵심인데, 그게 줄 첫머리로 밀려난 **여는** 마커이고 거기서 닫으면
/// 범위가 뒤집히기 때문이다(`DESIGN.md`).
fn can_open(prev: Option<char>, next: Option<char>) -> bool {
    next.is_some_and(|n| {
        !n.is_whitespace() && (!is_punct(n) || prev.is_none_or(|p| p.is_whitespace() || is_punct(p)))
    })
}

fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(c,
            '，' | '。' | '、' | '！' | '？' | '；' | '：' | '·' | '…' | '—' | '～'
            | '「' | '」' | '『' | '』' | '（' | '）' | '【' | '】' | '《' | '》'
            | '“' | '”' | '‘' | '’')
}
