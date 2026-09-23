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

use crate::vocab::{Emph, Vocab};

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
                        self.close_at(self.open.len() - 1, out, v);
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

            if c == '<' {
                if let Some(step) = self.angle(line, i, out, v) {
                    i += step;
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
            // **`***` 는 `**` 와 `*` 다** — 굵게 안에 기울임. 통째로 굵게로 읽으면 기울임을
            // 잃고, `_**x**_` 처럼 따로 적은 것과 답이 달라진다. 열 때는 굵게를 먼저
            // 열고(바깥), 닫을 때는 기울임이 열려 있으면 그것부터 닫는다(안쪽). 나머지
            // 마커는 다음 바퀴에서 자기 자리로 읽힌다.
            let take = if c != '~' && take == 3 {
                if self.open.iter().any(|o| o.emph == Emph::Italic) {
                    1
                } else if self.open.iter().any(|o| o.emph == Emph::Bold) {
                    // 굵게만 열려 있는데 셋이 왔다 — `**닫는 쪽만 셋***`. 통째로 닫는
                    // 마커다. 둘만 집으면 별표 하나가 남는다.
                    3
                } else {
                    2
                }
            } else {
                take
            };
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

            let after_space = prev.is_none_or(char::is_whitespace);
            let same = self.open.iter().rposition(|o| o.emph == emph);

            // **글자 뒤의 `_` 는 열지 못한다**(CommonMark 의 단어 안 `_`). `snake_case` 도
            // `2026-04-29_제목` 도 여기서 글자로 남는다 — 스크립트를 가리지 않는다. 한글 뒤
            // `_` 를 열어 주면 날짜 붙은 파일 이름이 기울임을 열고 블록 끝까지 삼킨다.
            // 대신 **닫는 것은 된다.** `_진료_가` 의 둘째 `_` 는 열린 기울임을 닫는다 —
            // CommonMark 는 못 닫지만, 조사가 붙는 한국어에서는 그게 저자의 뜻이다.
            let intraword = c == '_' && prev.is_some_and(char::is_alphanumeric);
            if intraword && same.is_none() {
                for _ in 0..take {
                    v.escape_char(c, out);
                }
                i += take;
                continue;
            }

            let left = can_open(prev, next) && !intraword;

            match same {
                // **추측으로 연 것은 닫지 않는다.** 추측은 확정되지 않는다 — 닫아 주면
                // 여는 쪽은 글자로 되돌아가거나 버려지고 닫는 쪽만 사라진다. `underfront.*
                // (4개), minjunkim.*` 의 글롭 별표 둘과 `/* 주석 */` 이 그렇게 사라졌다.
                // 실측(LLM 산출물 430건)에서 추측이 맞아떨어지는 모양은 없었고, 글자로 남은
                // 별표가 사라지는 쪽만 있었다. 닫는 자리의 마커는 글자고, 추측은 블록
                // 끝에서 되돌린다.
                // 앞이 글자인 마커(`조합**이`)도 추측을 닫지 않고, 열지도 않는다 — 열면
                // 블록 끝까지 삼킨다. 글자다.
                Some(at) if self.open[at].guess && (!left || !after_space) => {
                    for _ in 0..take {
                        v.escape_char(c, out);
                    }
                }
                // 여는 자리의 마커가 왔는데 추측이 열려 있다 — 추측이 틀렸다. 되돌리고
                // 이쪽을 연다. `/* a */ 다음 *z*` 의 `*z` 가 여기다.
                Some(at) if self.open[at].guess => self.reopen_at(at, out, take, c, emph, after_space),
                // 같은 종류가 열려 있고 앞이 공백이 아니면 여기가 닫는 자리다. 규칙 1.
                Some(at) if !after_space => self.close_at(at, out, v),
                // 앞이 공백인데 뒤로는 열 수 있다 — 여는 마커가 또 왔다. **먼저 열린 쪽이
                // 진다.** `채널**이다. …⏎**신분 공개**이` 에서 첫 `**` 는 짝 잃은 마커고
                // 둘째 줄이 온전한 굵게다 — CommonMark 도 슬랙도 그렇게 읽는다(실측). 먼저
                // 열린 마커는 앞이 공백이었으면 글자로 되돌리고, 글자였으면 버린다. 여기서
                // "닫기"로 읽으면 강조 범위가 뒤집힌다 — 그 고장이 원본이다. 규칙 2.
                Some(at) if left && at + 1 == self.open.len() => {
                    self.reopen_at(at, out, take, c, emph, after_space)
                }
                // 안쪽에 다른 종류가 열려 있으면 갈아 끼우지 못한다. 버린다.
                Some(_) if left => {}
                Some(at) => self.close_at(at, out, v),
                None if left => self.open.push(Open {
                    emph,
                    at: out.len(),
                    run: take,
                    ch: c,
                    guess: false,
                    after_space,
                }),
                // 열 수도 닫을 수도 없다. 일단 열어 두고 안 닫히면 글자로 되돌린다. 규칙 3.
                None => self.open.push(Open {
                    emph,
                    at: out.len(),
                    run: take,
                    ch: c,
                    guess: true,
                    after_space,
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
            self.close_at(self.open.len() - 1, out, v);
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

    /// 맨 위의 열린 마커를 물리고 이 자리에서 새로 연다 — 먼저 열린 쪽이 졌다.
    ///
    /// 홑마커는 글자로 되돌린다(글롭·주석·각주). `**` 는 추측이었고 앞이 공백이었을 때만
    /// 되돌린다(`2 ** 3`) — 진짜 여는 마커였다가 진 `**` 는 짝 잃은 마커라 버린다. 되돌리면
    /// 텔레그램 화면에 `**` 가 글자로 남는다.
    fn reopen_at(&mut self, at: usize, out: &mut String, take: usize, c: char, emph: Emph, after_space: bool) {
        debug_assert_eq!(at + 1, self.open.len());
        let old = self.open.pop().expect("at 은 유효한 인덱스다");
        if old.run == 1 || (old.guess && old.after_space) {
            for _ in 0..old.run {
                out.insert(old.at, old.ch);
            }
        }
        self.open.push(Open { emph, at: out.len(), run: take, ch: c, guess: false, after_space });
    }

    fn prev_char(&self, line: &[char], i: usize) -> Option<char> {
        if i > 0 {
            Some(line[i - 1])
        } else {
            self.prev
        }
    }

    /// `at` 번째 열린 마커를 닫는다. 그 위에 열린 것들은 먼저 정리한다.
    fn close_at(&mut self, at: usize, out: &mut String, v: &Vocab) {
        while self.open.len() > at + 1 {
            self.finalize(out, v);
        }
        self.finalize(out, v);
    }

    /// 맨 위 마커 하나를 확정한다 — 닫거나, 글자로 되돌리거나.
    fn finalize(&mut self, out: &mut String, v: &Vocab) {
        let Some(open) = self.open.pop() else { return };

        // 내용이 비었으면 태그를 만들지 않는다. `<b></b>` 는 아무에게도 쓸모가 없다.
        let empty = out.len() == open.at;
        if open.guess || empty {
            // 추측이 빗나갔다. 홑마커는 글자로 되돌린다 — 각주(`참고*`), 글롭
            // (`underfront.*`), 곱셈(`2 * 3`)으로 쓰이는 글자라 버리면 내용 손실이다.
            // `**` 는 앞이 공백이었을 때만 되돌린다(`2 ** 3`). 앞이 글자인 `**` 가 홀로
            // 남을 이유는 없다 — 짝 잃은 닫는 마커고, 되돌리면 출력에 마커가 남는다.
            if open.after_space || open.run == 1 {
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
                // `trim` 은 탭도 턴다. `<code> \t </code>` 의 바깥 공백은 벗겨야
                // 하므로, "전부 공백인가"는 ASCII 공백만으로 본다.
                && !body.chars().all(|c| c == ' ')
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
                // **한 번에 끼워 넣는다.** 한 글자씩 앞에 넣으면 넣을 때마다 내용
                // 전체가 밀려서 런이 길수록 제곱으로 는다.
                let mut fence = String::with_capacity(longest + 2);
                for _ in 0..longest + 1 {
                    fence.push('`');
                }
                if pad {
                    out.push(' ');
                }
                out.push_str(&fence);
                if pad {
                    fence.push(' ');
                }
                out.insert_str(open.at, &fence);
                return;
            }
        }
        out.insert_str(open.at, v.open(open.emph));
        out.push_str(v.close(open.emph));
    }

    /// `<…>` 를 읽는다 — 오토링크, 아는 HTML 태그, 주석. 셋 중 하나면 소비한 길이를
    /// 돌려주고, 아니면 `None` 이라 `<` 는 글자로 나간다.
    ///
    /// **LLM 산출물에서 실측된 셋이다.** `<https://…>` 는 mrkdwn 습관이 남은 링크
    /// (한 표본에서 124건)고, `<sub>`·`<br>` 은 마크다운에 없는 표현을 HTML 로 메운
    /// 것(74건)이다. 텔레그램은 모르는 태그를 받으면 400 이라 escape 해 왔는데, 그러면
    /// 화면에 `&lt;sub&gt;` 가 글자로 보인다. 태그는 벗기고 내용은 둔다 — 각색이 아니라
    /// 표처럼 **타깃에 그 구문이 없어서**다. `<br>` 만 줄바꿈으로 남긴다.
    ///
    /// 아는 태그만 벗긴다. `Vec<T>` 의 `<T>` 나 `1 < 2` 를 태그로 읽으면 글이 사라진다.
    fn angle(&mut self, line: &[char], i: usize, out: &mut String, v: &Vocab) -> Option<usize> {
        let rest = &line[i..];
        if starts_with(rest, "<!--") {
            let end = find_seq(&rest[4..], "-->")? + 4;
            return Some(end + 3);
        }
        let close = rest.iter().position(|&c| c == '>')?;
        if starts_with(&rest[1..], "http://") || starts_with(&rest[1..], "https://") {
            let body = &rest[1..close];
            // `<url|텍스트>` 는 슬랙 레거시 링크다. 슬랙에서 긁어 온 글에 그대로 남는다 —
            // 한 표본의 124건이 전부 이 모양이었다. 주소와 텍스트를 가른다.
            let (url, label) = match body.iter().position(|&c| c == '|') {
                Some(bar) => (&body[..bar], &body[bar + 1..]),
                None => (body, body),
            };
            if url.iter().any(|c| c.is_whitespace()) {
                return None;
            }
            // 오토링크의 텍스트는 인라인으로 다시 읽지 않는다 — 주소 안의 `_` 가
            // 기울임이 되면 안 된다.
            let mut href = String::with_capacity(url.len());
            href.extend(url.iter());
            let mut text = std::mem::take(&mut self.scratch);
            text.clear();
            for &c in label {
                v.escape_char(c, &mut text);
            }
            v.link(&text, &href, out);
            self.scratch = text;
            self.prev = Some('>');
            return Some(close + 1);
        }
        let closing = rest.get(1) == Some(&'/');
        let name_at = if closing { 2 } else { 1 };
        let mut j = name_at;
        while j < close && rest[j].is_ascii_alphanumeric() {
            j += 1;
        }
        let name = &rest[name_at..j];
        // 이름 뒤는 속성(공백)이거나 `/>` 거나 바로 `>` 다. 아니면 태그 모양이 아니다.
        if name.is_empty() || !(rest[j] == '>' || rest[j] == '/' || rest[j].is_whitespace()) {
            return None;
        }
        if !is_known_tag(name) {
            return None;
        }
        if !closing && eq_ignore_case(name, "br") {
            out.push('\n');
            self.prev = Some('\n');
        }
        Some(close + 1)
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

fn starts_with(chars: &[char], s: &str) -> bool {
    let mut it = chars.iter();
    s.chars().all(|c| it.next() == Some(&c))
}

fn find_seq(chars: &[char], s: &str) -> Option<usize> {
    (0..chars.len()).find(|&k| starts_with(&chars[k..], s))
}

fn eq_ignore_case(chars: &[char], s: &str) -> bool {
    chars.len() == s.len() && chars.iter().zip(s.chars()).all(|(a, b)| a.eq_ignore_ascii_case(&b))
}

/// 벗겨도 되는 HTML 태그. 마크다운이 못 적는 표현을 LLM 이 HTML 로 메울 때 쓰는 것들이다.
/// 링크(`<a>`)는 없다 — 벗기면 주소가 사라진다.
fn is_known_tag(name: &[char]) -> bool {
    const KNOWN: [&str; 21] = [
        "br", "sub", "sup", "b", "strong", "i", "em", "u", "s", "strike", "del", "code", "span",
        "div", "p", "small", "mark", "kbd", "font", "center", "details",
    ];
    KNOWN.iter().any(|t| eq_ignore_case(name, t)) || eq_ignore_case(name, "summary")
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
    // 앞이 글자·숫자가 아니면(공백, 구두점, 그리고 `①`·`🔥` 같은 기호) 열 수 있다.
    // CommonMark 는 유니코드 구두점만 치지만, LLM 은 항목 머리에 기호를 붙인다 —
    // `①**"주간 졸림"**` 을 못 열면 닫는 쪽만 글자로 남는다(실측). `①` 은 유니코드로는
    // 숫자(No)라 `is_alphanumeric` 에 걸린다 — 글자는 알파벳과 ASCII 숫자만 친다.
    next.is_some_and(|n| {
        !n.is_whitespace() && (!is_punct(n) || prev.is_none_or(|p| !is_word_char(p)))
    })
}

/// 강조 마커 앞뒤의 "글자". 알파벳(한글 포함)과 ASCII 숫자다 — `①` 같은 기호는 아니다.
fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c.is_ascii_digit()
}

fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(c,
            '，' | '。' | '、' | '！' | '？' | '；' | '：' | '·' | '…' | '—' | '～'
            | '「' | '」' | '『' | '』' | '（' | '）' | '【' | '】' | '《' | '》'
            | '“' | '”' | '‘' | '’')
}
