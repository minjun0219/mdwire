//! 블록 스캐너 — 줄 단위로 읽고, 줄 단위로 내보낸다.
//!
//! 스트리밍이 가능한 이유가 여기 있다. 블록의 종류는 **그 줄 하나**로 정해지고
//! (표만 한 줄을 더 본다), 결정되면 바로 렌더해서 내보낸다. 문서 전체를 받아
//! AST 를 만드는 배치형 파서와 갈리는 지점이다.
//!
//! # 줄이 끝나기를 기다리지 않는다
//!
//! 줄 단위로만 내보내면, 줄바꿈 없이 한 문단을 쏟는 에이전트 앞에서 이 라이브러리는
//! 사실상 배치형이 된다. 그래서 **블록 종류가 접두사로 정해지는 순간** 그 줄을 열고,
//! 뒤따라 오는 글자를 그대로 흘려보낸다.
//!
//! 붙들고 있는 것은 셋뿐이다.
//!
//! - **판정이 덜 된 접두사** — `#` 뒤에 공백이 올지 글자가 올지 봐야 한다. 몇 글자다
//! - **줄 끝에 걸친 마커·공백** — `**` 는 다음 글자를 봐야 열기/닫기가 갈리고,
//!   줄 끝 공백은 지워야 한다
//! - **줄 전체가 필요한 것들** — 코드펜스의 info, 표 후보 한 줄, 열 너비를 재야 하는 표 본문
//!
//! 나머지는 전부 흘려보낸다.

use crate::inline::Inline;
use crate::sink::Sink;
use crate::vocab::{Emph, Vocab};
use crate::width::str_width;
use crate::{Channel, Dialect, Options, Repairs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// 블록 밖. 여기서만 표가 시작될 수 있다.
    None,
    Para,
    /// 한 줄짜리 블록. 줄이 끝나면 바로 닫힌다.
    Heading,
    List,
    Quote,
    Fence,
    Table,
}

#[derive(Clone)]
pub(crate) struct Engine {
    pub v: Vocab,
    inline: Inline,
    /// 현재 줄에서 아직 파싱하지 않은 부분.
    pending: Vec<char>,
    /// 이 줄의 접두사가 정해져 블록이 열렸는가.
    line_open: bool,
    /// 열린 줄의 종류. `line_open` 일 때만 뜻이 있다.
    kind: LineKind,
    out: String,
    state: State,
    /// 앞 헤딩의 레벨. 두 단계 이상 깊어지는 것을 막는다(`SPEC.md` 8절).
    heading: usize,
    /// 한 줄이라도 내보냈는가. 앞머리 빈 줄을 만들지 않으려고 본다.
    wrote: bool,
    /// 빈 줄이 하나 밀려 있는가. 여러 개가 와도 하나로 접는다(정돈).
    blank: bool,
    held: Option<String>,
    /// 붙들어 둔 `[`·`<!--` 를 어디까지 훑었나. [`safe_cut`] 이 조각마다 처음부터 다시 훑지
    /// 않게 들고 있는다. 열린 줄 안에서만 뜻이 있다.
    hold: HoldMemo,
    /// 줄 전체가 필요한 경로에서만 쓰는 버퍼. 재사용해서 줄마다 할당하지 않는다.
    scratch: String,
    table: Table,
    fence: FenceState,
    /// 조각이 `\r` 로 끝났다. 다음 조각이 `\n` 으로 시작하면 CRLF 라 버리고, 아니면 글자다.
    ///
    /// **`\r` 를 줄에 먼저 넣으면 안 된다.** 넣는 순간 그 줄은 `\r` 로 끝나는 줄로 판정된다
    /// — `---\r` 는 구분선이 아니라 문단이 되고, `\r` 하나뿐인 줄은 빈 줄이 아니라 문단이
    /// 된다. 완성본은 `\r\n` 을 한 번에 봐서 안 갈린다. 스트리밍이 같은 답을 내려면
    /// 다음 글자를 볼 때까지 들고 있어야 한다.
    cr: bool,
    /// 문서 끝까지 안 닫혀서 닫아 준 코드펜스 수.
    closed_fence: usize,
    /// 입력 방언. 표 셀을 읽을 때도 문서를 따른다.
    dialect: Dialect,
    /// 열린 목록들 — (항목 들여쓰기, 번호 목록인가). 바깥부터. **HTML 채널만 쓴다** — 다른
    /// 채널은 목록을 글자(`- `·`• `)로 그려서 중첩을 태그로 여닫을 일이 없다.
    lists: Vec<(usize, bool)>,
    /// 목록 안에서 빈 줄을 만났고, 다음 줄이 항목인지 아직 모른다(HTML). 항목이면 같은 목록을
    /// 잇고, 아니면 그때 닫는다.
    list_gap: bool,
}

#[derive(Default, Clone)]
struct FenceState {
    ch: char,
    len: usize,
    info: String,
    /// 내용을 한 줄이라도 썼는가. 여는 마크업 바로 뒤의 줄바꿈을 정하려고 본다.
    body: bool,
}

impl Engine {
    pub fn new(channel: Channel, options: &Options) -> Self {
        Self {
            v: Vocab::from_options(channel, options),
            inline: Inline::new(options.from),
            pending: Vec::new(),
            line_open: false,
            kind: LineKind::Para,
            out: String::new(),
            state: State::None,
            heading: 0,
            wrote: false,
            blank: false,
            held: None,
            hold: HoldMemo::default(),
            scratch: String::new(),
            table: Table::default(),
            fence: FenceState::default(),
            cr: false,
            closed_fence: 0,
            dialect: options.from,
            lists: Vec::new(),
            list_gap: false,
        }
    }

    /// 지금까지 정규화가 고친 것.
    pub fn repairs(&self) -> Repairs {
        let mut r = self.inline.repairs;
        r.closed_fence += self.closed_fence;
        r
    }

    pub fn feed<S: Sink>(&mut self, chunk: &str, sink: &mut S) {
        if chunk.is_empty() {
            return;
        }
        if std::mem::take(&mut self.cr) && !chunk.starts_with('\n') {
            self.pending.push('\r');
        }
        for seg in chunk.split_inclusive('\n') {
            match seg.strip_suffix('\n') {
                Some(rest) => {
                    // CRLF 입력. `\r` 를 남겨 보내면 채널에 그대로 박힌다.
                    let rest = rest.strip_suffix('\r').unwrap_or(rest);
                    self.pending.extend(rest.chars());
                    self.progress(true, sink);
                }
                None => {
                    let seg = match seg.strip_suffix('\r') {
                        Some(s) => {
                            self.cr = true;
                            s
                        }
                        None => seg,
                    };
                    self.pending.extend(seg.chars());
                    self.progress(false, sink);
                }
            }
        }
    }

    pub fn finish<S: Sink>(&mut self, sink: &mut S) {
        // 문서가 `\r` 로 끝났다 — 뒤에 `\n` 이 없으니 글자다.
        if std::mem::take(&mut self.cr) {
            self.pending.push('\r');
        }
        if !self.pending.is_empty() || self.line_open {
            self.progress(true, sink);
        }
        if let Some(held) = self.held.take() {
            self.whole_para(&held, sink);
        }
        if self.state == State::Fence {
            self.closed_fence += 1;
        }
        self.close_block(sink);
        self.flush_all(sink);
        sink.boundary();
    }

    /// **지금 입력이 끝났다면** 나올 꼬리를 `out` 에 쓴다. 자기 상태는 건드리지 않는다 —
    /// 복제본에 `finish` 를 부른다. 그래서 꼬리는 일괄 렌더와 같은 경로를 타고, 붙들고 있던
    /// 것(열린 강조, 표 행, 펜스, 판정 전 접두사)이 일괄 렌더가 지키는 모양 그대로 나온다.
    ///
    /// 다른 점은 하나 — 안 닫힌 코드 스팬을 글자로 되돌리지 않고 닫는다([`Inline::preview`]).
    pub fn preview<S: Sink>(&self, sink: &mut S) {
        let mut e = self.clone();
        e.inline.preview = true;
        e.finish(sink);
    }

    /// 지금까지 받은 것으로 갈 수 있는 데까지 간다.
    ///
    /// `eol` 이면 이 줄은 여기서 끝난다 — 미뤄 둔 판단을 전부 확정해야 한다.
    fn progress<S: Sink>(&mut self, eol: bool, sink: &mut S) {
        // 줄 전체를 봐야 하는 상태들. 여기서는 스트리밍하지 않는다.
        if self.state == State::Fence || self.state == State::Table || self.held.is_some() {
            if !eol {
                return;
            }
            if self.consume_whole_line(sink) {
                return;
            }
            // 상태가 풀렸다. 이 줄은 아래의 일반 경로로 간다.
        }

        if !self.line_open {
            // 새 줄이다. 앞 줄에서 훑던 자리는 이 줄과 상관없다.
            self.hold = HoldMemo::default();
            // 표는 **`|` 로 시작하는 줄에서만** 시작한다. 앞 블록이 무엇이든 상관없다.
            //
            // 붙드는 값이 그 줄 하나뿐이라 그렇다 — 구분선이 따라오는지 보려면 한 줄을
            // 더 기다려야 하는데, 대상이 `|` 로 시작하는 줄뿐이면 산문은 한 번도 안 걸린다.
            // 반대로 "줄 어디에든 `|` 가 있으면 표일 수 있다"로 넓히면 모든 줄을 끝까지
            // 붙들어야 하고, 그러면 스트리밍이 죽는다. 그 경계가 여기다.
            match classify(&self.pending, eol, true) {
                Decision::NeedMore => return,
                Decision::Whole(kind) => {
                    self.whole(kind, sink);
                    self.pending.clear();
                    return;
                }
                Decision::Prefix(kind, len) => {
                    self.open_line(kind, len, sink);
                }
            }
        }

        // 접두사 뒤부터는 인라인이다. 끝에 걸친 마커·공백만 남기고 흘려보낸다.
        let cut = if eol { trim_end(&self.pending) } else { safe_cut(&self.pending, &mut self.hold) };
        if cut > 0 {
            self.inline.render(&self.pending[..cut], &mut self.out, &self.v);
            self.pending.drain(..cut);
            self.hold.shift(cut);
        }
        if eol {
            self.pending.clear();
            self.end_line(sink);
        }
        self.flush_safe(sink);
    }

    /// 줄 전체가 필요한 상태를 소비한다. 이 줄을 다 썼으면 `true`.
    fn consume_whole_line<S: Sink>(&mut self, sink: &mut S) -> bool {
        // 1. 코드펜스 안은 전부 내용이다. 빈 줄도 마커도 글자다.
        if self.state == State::Fence {
            let line = self.take_line();
            let closing = fence_marker(line.trim_start())
                .is_some_and(|(ch, len, _)| ch == self.fence.ch && len >= self.fence.len);
            if closing {
                self.close_block(sink);
            } else {
                self.fence_body(&line);
                self.flush_all(sink);
            }
            self.scratch = line;
            return true;
        }

        // 2. 표 후보를 들고 있었다면 이 줄이 구분선인지로 판가름난다.
        if let Some(held) = self.held.take() {
            let line = self.take_line();
            if is_delimiter_row(&line) && self.table.begin(&held, &line) {
                self.pending.clear();
                self.scratch = line;
                // 앞 블록이 인용문이었을 수 있다. 표를 시작하기 전에 닫는다 —
                // 안 닫으면 `<blockquote>` 가 열린 채로 표가 들어간다.
                self.close_block(sink);
                self.state = State::Table;
                return true;
            }
            // 표가 아니었다. 들고 있던 줄을 문단으로 내보내고, 이 줄은 아래로 흘린다.
            self.pending.clear();
            self.pending.extend(line.chars());
            self.scratch = line;
            self.whole_para(&held, sink);
            return false;
        }

        // 3. 표 본문.
        if self.state == State::Table {
            let line = self.take_line();
            let row = !line.trim().is_empty() && line.contains('|');
            if row {
                self.pending.clear();
                self.table.push(&line);
            } else {
                self.pending.clear();
                self.pending.extend(line.chars());
                self.close_block(sink);
            }
            self.scratch = line;
            return row;
        }
        false
    }

    /// 현재 줄을 문자열로 꺼낸다. 버퍼는 `scratch` 에서 빌리고 쓴 뒤 돌려준다.
    /// 돌려주기 전까지 `self.scratch` 는 비어 있다.
    fn take_line(&mut self) -> String {
        let mut line = std::mem::take(&mut self.scratch);
        line.clear();
        line.extend(self.pending.iter());
        self.pending.clear();
        line
    }

    /// 줄 전체로 판정되는 블록들.
    fn whole<S: Sink>(&mut self, kind: WholeKind, sink: &mut S) {
        match kind {
            WholeKind::Blank => {
                // **빈 줄로 띄운 목록(loose list)은 한 목록이다**(HTML). 여기서 닫으면 목록 스택이
                // 비어, 빈 줄 뒤 들여쓴 항목이 새 최상위 목록으로 열려 중첩이 사라진다 —
                // `- a⏎⏎  - b` 의 b 가 a 밖으로 빠졌다(리뷰에서 나왔다). 항목의 인라인만 확정하고
                // 다음 줄을 기다린다. 다른 채널은 들여쓰기가 글자로 남아 이미 중첩이 산다.
                if self.v.is_html() && self.state == State::List {
                    self.inline.finish_block(&mut self.out, &self.v);
                    self.list_gap = true;
                    self.blank = self.wrote;
                    self.flush_all(sink);
                    return;
                }
                self.close_block(sink);
                self.blank = self.wrote;
            }
            WholeKind::Rule => {
                self.close_block(sink);
                self.start_line();
                self.out.push_str(self.v.rule());
                self.close_block(sink);
            }
            WholeKind::Fence => {
                let line = self.take_line();
                let t = line.trim_start();
                let (ch, len, info) = fence_marker(t).expect("분류가 펜스라고 했다");
                self.close_block(sink);
                self.start_line();
                self.fence = FenceState { ch, len, info: info.to_string(), body: false };
                self.state = State::Fence;
                let info = std::mem::take(&mut self.fence.info);
                self.v.verbatim_open(&info, &mut self.out);
                self.fence.info = info;
                self.flush_all(sink);
                self.scratch = line;
            }
            WholeKind::TableCandidate => {
                // 구분선이 따라오는지 한 줄만 기다린다.
                self.held = Some(self.pending.iter().collect());
            }
        }
    }

    /// 접두사가 정해졌다. 블록을 열고 접두사를 내보낸다.
    fn open_line<S: Sink>(&mut self, kind: LineKind, prefix: usize, sink: &mut S) {
        // 빈 줄 뒤에 항목이 아닌 것이 왔다 — 목록이 끝났다.
        if std::mem::take(&mut self.list_gap) && !matches!(kind, LineKind::Bullet(_) | LineKind::Ordered(..)) {
            self.close_block(sink);
        }
        match kind {
            LineKind::Para if self.state == State::List => {
                // **리스트 항목이 다음 줄로 이어진다.** 항목은 아직 끝나지 않았다.
                // 여기서 항목을 끊으면 줄을 넘는 강조가 항목 안에서만 안 잡힌다 —
                // 80열 wrap 은 불릿 안에서도 똑같이 일어나므로 그건 고장이다.
                self.out.push_str(self.v.line_break());
                self.inline.note_raw(self.v.line_break());
                self.inline.end_line();
                for _ in 0..prefix.min(8) {
                    self.out.push(' ');
                    self.inline.note_raw(" ");
                }
            }
            LineKind::Para => {
                if self.state != State::Para {
                    self.close_block(sink);
                    self.state = State::Para;
                    self.start_line();
                    if self.v.is_html() {
                        self.out.push_str("<p>");
                    }
                } else {
                    // 문단 안의 줄바꿈은 살린다. **강조는 이 줄바꿈을 넘어 이어진다** —
                    // 80열 wrap 된 산문에서 그게 일상이고, 그것이 이 라이브러리의 첫 고장이었다.
                    self.out.push_str(self.v.line_break());
                    self.inline.note_raw(self.v.line_break());
                    self.inline.end_line();
                }
            }
            LineKind::Heading(level) => {
                self.close_block(sink);
                self.state = State::Heading;
                self.start_line();
                self.open_heading(level);
            }
            LineKind::Quote => {
                if self.state != State::Quote {
                    self.close_block(sink);
                    self.state = State::Quote;
                    self.start_line();
                    self.out.push_str(self.v.quote_open());
                } else {
                    self.out.push_str(self.v.line_break());
                    self.inline.note_raw(self.v.line_break());
                    self.inline.end_line();
                }
                self.out.push_str(self.v.quote_prefix());
                self.inline.note_raw(self.v.quote_prefix());
                self.inline.set_prev(None);
            }
            LineKind::Bullet(indent) | LineKind::Ordered(indent, _) => {
                if self.state != State::List {
                    self.close_block(sink);
                    self.state = State::List;
                } else {
                    // 같은 리스트의 다음 항목. 여기서 앞 항목의 인라인을 확정한다 —
                    // 강조는 항목을 넘지 않는다.
                    self.inline.finish_block(&mut self.out, &self.v);
                }
                self.start_line();
                if self.v.is_html() {
                    let n = match kind {
                        LineKind::Ordered(_, n) => Some(n),
                        _ => None,
                    };
                    self.open_item(indent, n);
                } else {
                    for _ in 0..indent.min(8) {
                        self.out.push(' ');
                    }
                    match kind {
                        LineKind::Ordered(_, n) => {
                            push_usize(&mut self.out, n);
                            self.out.push_str(". ");
                        }
                        _ => self.out.push_str(self.v.bullet()),
                    }
                }
                self.inline.set_prev(None);
            }
        }
        self.pending.drain(..prefix);
        self.kind = kind;
        self.line_open = true;
    }

    /// 줄이 끝났다.
    fn end_line<S: Sink>(&mut self, sink: &mut S) {
        match self.kind {
            // 헤딩은 한 줄짜리 블록이다.
            LineKind::Heading(_) => self.close_block(sink),
            // 항목은 줄 하나로 끝나지 않는다. 이어지는 줄이 올 수 있으니 강조는 열어 둔다.
            // 확정은 다음 항목이 시작되거나 리스트가 끝날 때다.
            LineKind::Bullet(_) | LineKind::Ordered(..) | LineKind::Para | LineKind::Quote => {
                self.inline.end_line()
            }
        }
        self.line_open = false;
    }

    /// 이미 완성된 줄 하나를 문단으로 내보낸다. 표가 아니었던 후보 줄이 여기로 온다.
    fn whole_para<S: Sink>(&mut self, line: &str, sink: &mut S) {
        let saved = std::mem::take(&mut self.pending);
        self.pending.extend(line.chars());
        self.line_open = false;
        match classify(&self.pending, true, false) {
            Decision::Prefix(kind, len) => {
                self.open_line(kind, len, sink);
                let cut = trim_end(&self.pending);
                if cut > 0 {
                    self.inline.render(&self.pending[..cut], &mut self.out, &self.v);
                }
                self.end_line(sink);
            }
            _ => self.whole(WholeKind::Blank, sink),
        }
        self.pending = saved;
        self.flush_safe(sink);
    }

    /// 목록 항목 하나를 태그로 연다(HTML). 들여쓰기로 중첩을 가른다 — 더 깊으면 지금 항목
    /// 안에 목록을 새로 열고, 얕으면 그만큼 닫고, 같으면 항목만 바꾼다. **여는 태그는 붙들지
    /// 않는다** — 목록이 끝날 때까지 기다리면 스트리밍이 아니다. 닫는 것은 `close_open`.
    fn open_item(&mut self, indent: usize, number: Option<usize>) {
        let ordered = number.is_some();
        while self.lists.last().is_some_and(|&(i, _)| i > indent) {
            let (_, o) = self.lists.pop().expect("방금 봤다");
            self.out.push_str(if o { "</li></ol>" } else { "</li></ul>" });
        }
        match self.lists.last().copied() {
            // 같은 깊이, 같은 종류 — 항목만 바꾼다.
            Some((i, o)) if i == indent && o == ordered => self.out.push_str("</li>"),
            // 같은 깊이인데 종류가 바뀌었다 — 목록을 갈아 낀다.
            Some((i, o)) if i == indent => {
                self.lists.pop();
                self.out.push_str(if o { "</li></ol>" } else { "</li></ul>" });
                self.open_list(indent, number);
            }
            _ => self.open_list(indent, number),
        }
        self.out.push_str("<li>");
    }

    fn open_list(&mut self, indent: usize, number: Option<usize>) {
        match number {
            // 1 이 아닌 번호로 시작하면 번호를 이어 간다 — 빈 줄로 끊긴 번호 목록이 그렇다.
            Some(n) if n != 1 => {
                self.out.push_str("<ol start=\"");
                push_usize(&mut self.out, n);
                self.out.push_str("\">");
            }
            Some(_) => self.out.push_str("<ol>"),
            None => self.out.push_str("<ul>"),
        }
        self.lists.push((indent, number.is_some()));
    }

    fn open_heading(&mut self, level: usize) {
        let level = if self.heading == 0 { level } else { level.min(self.heading + 1) };
        self.heading = level;
        let max = self.v.max_heading();
        if self.v.is_html() {
            self.out.push_str("<h");
            push_usize(&mut self.out, level.min(max));
            self.out.push('>');
        } else if max > 0 {
            for _ in 0..level.min(max) {
                self.out.push('#');
            }
            self.out.push(' ');
        } else if !self.v.is_plain() {
            // 헤딩 구문이 없는 채널. 줄 전체를 굵게 내보낸다.
            self.out.push_str(self.v.open(Emph::Bold));
        }
        self.inline.set_prev(None);
    }

    fn fence_body(&mut self, line: &str) {
        if self.fence.body || self.v.verbatim_body_newline() {
            self.out.push('\n');
        }
        self.fence.body = true;
        self.v.escape(line, &mut self.out);
    }

    /// 줄 하나를 시작한다. 블록 사이 빈 줄은 하나로 접는다(정돈).
    fn start_line(&mut self) {
        if self.wrote {
            self.out.push('\n');
            if self.blank {
                self.out.push('\n');
            }
        }
        self.blank = false;
        self.wrote = true;
    }

    /// **지금까지 내보낸 것에 열려 있는 블록 마크업을 닫아 붙인다.** 상태는 건드리지 않는다.
    ///
    /// 인라인 강조는 짝이 맞을 때까지 안에 붙들어 두므로 이미 균형이 맞다. 하지만 블록의
    /// 여는 마크업(`<blockquote>`·`<pre>`·헤딩의 `<b>`)은 그 블록이 끝나기 전에 나간다 —
    /// 코드블록 하나가 끝날 때까지 출력을 멈출 수는 없기 때문이다. 그래서 **누적본을
    /// 중간에 그대로 채널로 보내면 열린 태그가 남는다.** 보내기 직전에 이걸 덧붙이면 된다.
    pub fn close_open(&self, out: &mut String) {
        self.block_close_markup(out);
    }

    /// 블록의 닫는 마크업. 인라인 정리는 하지 않는다.
    fn block_close_markup(&self, out: &mut String) {
        match self.state {
            State::Heading if self.v.is_html() => {
                out.push_str("</h");
                push_usize(out, self.heading.min(self.v.max_heading()));
                out.push('>');
            }
            State::Heading => {
                if self.v.max_heading() == 0 && !self.v.is_plain() {
                    out.push_str(self.v.close(Emph::Bold));
                }
            }
            State::Para if self.v.is_html() => out.push_str("</p>"),
            State::List => {
                for &(_, ordered) in self.lists.iter().rev() {
                    out.push_str(if ordered { "</li></ol>" } else { "</li></ul>" });
                }
            }
            State::Quote => out.push_str(self.v.quote_close()),
            State::Fence => self.v.verbatim_close(&self.fence.info, out),
            State::None | State::Para | State::Table => {}
        }
    }

    /// 블록을 닫는다. 열린 인라인을 확정하고, 블록의 닫는 마크업을 붙이고, 경계를 알린다.
    fn close_block<S: Sink>(&mut self, sink: &mut S) {
        match self.state {
            State::None => {}
            State::Para | State::List | State::Heading | State::Quote => {
                self.inline.finish_block(&mut self.out, &self.v);
                let mut out = std::mem::take(&mut self.out);
                self.block_close_markup(&mut out);
                self.out = out;
            }
            State::Fence => {
                let mut out = std::mem::take(&mut self.out);
                self.block_close_markup(&mut out);
                self.out = out;
            }
            State::Table => {
                let mut table = std::mem::take(&mut self.table);
                self.start_line();
                let breaks = table.render(&self.v, self.dialect, &mut self.inline.repairs, &mut self.out);
                table.clear();
                self.table = table;
                // 한도를 넘어 여러 표로 나눴다 — 사이를 블록 경계로 내보내 분할기가 표 한가운데를
                // 자르지 않게 한다(노션).
                if !breaks.is_empty() {
                    let mut from = 0;
                    for at in breaks {
                        sink.text(&self.out[from..at]);
                        sink.boundary();
                        from = at;
                    }
                    self.out.drain(..from);
                }
            }
        }
        self.state = State::None;
        self.lists.clear();
        self.list_gap = false;
        self.inline.reset();
        self.flush_all(sink);
        sink.boundary();
    }

    /// 열린 마크업 앞까지만 내보낸다. 나머지는 짝이 맞을 때까지 안에 남는다.
    fn flush_safe<S: Sink>(&mut self, sink: &mut S) {
        let safe = self.inline.safe_len(self.out.len());
        if safe == 0 {
            return;
        }
        sink.text(&self.out[..safe]);
        self.out.drain(..safe);
        self.inline.shift(safe);
    }

    fn flush_all<S: Sink>(&mut self, sink: &mut S) {
        debug_assert!(!self.inline.is_open(), "열린 마크업이 있는 채로 전부 내보낼 수 없다");
        if !self.out.is_empty() {
            sink.text(&self.out);
            self.out.clear();
        }
    }
}

/// 접두사만으로 정해지는 줄의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Para,
    Heading(usize),
    Quote,
    Bullet(usize),
    Ordered(usize, usize),
}

/// 줄 전체를 봐야 정해지는 것들.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WholeKind {
    Blank,
    Rule,
    /// info 문자열이 줄 끝까지 이어진다.
    Fence,
    /// 구분선이 따라오는지 다음 줄을 봐야 한다.
    TableCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    /// 아직 못 정한다. 글자가 더 와야 한다.
    NeedMore,
    /// 정해졌다. 두 번째 값은 접두사의 길이(글자 수)다.
    Prefix(LineKind, usize),
    Whole(WholeKind),
}

/// 마커 뒤에 올 수 있는 공백. **탭도 공백이다.**
///
/// 실제 문서에서 나왔다 — `*<TAB>ws` 로 쓴 목록을 문단으로 읽으면, 줄마다 앞에 선 `*`
/// 가 강조 마커로 짝지어져 **불릿이 통째로 사라진다.**
fn marker_space(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// 받은 데까지로 줄의 종류를 정해 본다.
///
/// **못 정할 때만 기다린다.** `#` 뒤에 공백이 올지 글자가 올지, `--` 가 구분선이 될지
/// 문단이 될지는 몇 글자만 더 보면 된다. 그 몇 글자가 이 라이브러리가 붙드는 전부다.
fn classify(p: &[char], eol: bool, can_table: bool) -> Decision {
    let indent = p.iter().take_while(|&&c| c == ' ' || c == '\t').count();
    if indent == p.len() {
        return if eol { Decision::Whole(WholeKind::Blank) } else { Decision::NeedMore };
    }
    let t = &p[indent..];
    let para = Decision::Prefix(LineKind::Para, indent);
    let at_end = |d: Decision| if eol { para } else { d };

    match t[0] {
        '#' => {
            let n = run(t, '#');
            if n > 6 {
                return para;
            }
            match t.get(n) {
                Some(c) if marker_space(*c) => {
                    Decision::Prefix(LineKind::Heading(n), indent + n + 1)
                }
                Some(_) => para,
                None => at_end(Decision::NeedMore),
            }
        }
        '>' => match t.get(1) {
            Some(' ') => Decision::Prefix(LineKind::Quote, indent + 2),
            Some(_) => Decision::Prefix(LineKind::Quote, indent + 1),
            None => {
                if eol {
                    Decision::Prefix(LineKind::Quote, indent + 1)
                } else {
                    Decision::NeedMore
                }
            }
        },
        c @ ('`' | '~') => {
            let n = run(t, c);
            if n >= 3 {
                // 여는 펜스의 info 는 줄 끝까지다.
                return if eol { Decision::Whole(WholeKind::Fence) } else { Decision::NeedMore };
            }
            if t.len() > n {
                para
            } else {
                at_end(Decision::NeedMore)
            }
        }
        '|' if can_table => {
            if eol {
                Decision::Whole(WholeKind::TableCandidate)
            } else {
                Decision::NeedMore
            }
        }
        '+' | '•' => match t.get(1) {
            Some(c) if marker_space(*c) => Decision::Prefix(LineKind::Bullet(indent), indent + 2),
            Some(_) => para,
            None => at_end(Decision::NeedMore),
        },
        c @ ('-' | '*' | '_') => {
            let n = run(t, c);
            if n == 1 && c != '_' {
                return match t.get(1) {
                    Some(c) if marker_space(*c) => {
                        Decision::Prefix(LineKind::Bullet(indent), indent + 2)
                    }
                    Some(_) => para,
                    None => at_end(Decision::NeedMore),
                };
            }
            // 같은 글자가 이어진다. 구분선은 그 글자와 공백만으로 된 줄이고,
            // 그게 아니면 `**굵게` 처럼 강조로 시작하는 문단이다.
            if t[n..].iter().any(|&x| x != c && !marker_space(x)) {
                return para;
            }
            if eol {
                if t.iter().filter(|&&x| x == c).count() >= 3 {
                    Decision::Whole(WholeKind::Rule)
                } else {
                    para
                }
            } else {
                Decision::NeedMore
            }
        }
        '0'..='9' => {
            let d = t.iter().take_while(|c| c.is_ascii_digit()).count();
            if d > 9 {
                return para;
            }
            match (t.get(d), t.get(d + 1)) {
                (Some('.' | ')'), Some(c)) if marker_space(*c) => {
                    let mut n = 0usize;
                    for c in &t[..d] {
                        n = n * 10 + (*c as usize - '0' as usize);
                    }
                    Decision::Prefix(LineKind::Ordered(indent, n), indent + d + 2)
                }
                (Some('.' | ')'), None) | (None, _) => at_end(Decision::NeedMore),
                (Some(_), _) => para,
            }
        }
        _ => para,
    }
}

/// 아직 내보내면 안 되는 꼬리를 빼고 남은 길이.
///
/// 마커는 **다음 글자를 봐야 열기/닫기가 갈린다**. 줄 끝 공백은 지워야 한다.
/// 닫히지 않은 `[` 는 링크가 될지 글자가 될지 모른다.
///
/// **붙든 구문은 이어서 훑는다.** 닫히지 않은 `[`·`<!--` 는 줄 끝까지 붙들 수 있는데, 조각마다
/// 처음부터 다시 훑으면 한 줄 안에서 O(n²) 이 된다 — 한 글자씩 흘린 4만 자에서 수백 ms 였다.
/// 붙든 그 줄 안에서 `pending` 은 뒤에 붙기만 하고 앞은 `cut` 만큼 빠지므로, 훑던 자리를
/// [`HoldMemo`] 에 두고 새로 온 꼬리만 본다.
fn safe_cut(p: &[char], memo: &mut HoldMemo) -> usize {
    let mut k = p.len();
    // 링크는 `[` 부터 `](…)` 의 `)` 까지 통째로 봐야 한다. **텍스트 안의 `)` 로 놓으면 안
    // 된다** — `[Show GN (MAYDAY)](url)` 이 64바이트 조각으로 들어올 때 `(MAYDAY)` 의 `)` 에서
    // 놓아 링크가 글자로 나갔다(Go 이식과 실제 문서를 대조하다 나왔다).
    // **앞에서부터 본다.** 마지막 `[` 만 보면 `[a](1.[b](url` 처럼 주소 안에 링크가 겹칠 때
    // 바깥 `[` 가 먼저 글자로 나간다 — 완성본은 바깥을 링크로 읽는다(퍼즈에서 나왔다).
    // 앞 조각에서 여기까지는 안 닫힌 `[` 가 없었다 — 거기서 잇는다.
    let mut i = memo.scanned.min(k);
    while i < k {
        if p[i] != '[' {
            i += 1;
            continue;
        }
        let from = match memo.link {
            Some((at, scan)) if at == i => scan,
            _ => LinkScan::Bracket { j: 1, depth: 0 },
        };
        match link_end_from(&p[i..k], from) {
            Ok(end) => i += end,
            Err(scan) => {
                memo.link = Some((i, scan));
                k = i;
                break;
            }
        }
    }
    memo.scanned = i;
    // **태그 모양의 `<` 도 붙든다.** `<sub>` 가 `<su` / `b>` 로 갈리면 앞쪽이 글자로
    // 나가 버린다. `>` 가 오거나 태그라기엔 길어지면 놓는다 — `1 < 2` 처럼 뒤가
    // 공백이면 애초에 안 붙든다. 다음 글자가 아직 안 왔으면(`<` 가 마지막) 일단 붙든다.
    // **주석은 `-->` 까지 통째로 붙든다 — 길이를 안 잰다.** 긴 주석을 80자에서 놓으면 `<` 가
    // 글자로 나가 주석이 본문에 새고, 완성본(주석을 지운다)과 갈린다. 붙드는 범위는 그 줄
    // 안이라 안 닫힌 주석도 줄 끝에서 풀린다. 주석 안의 `<b>` 에 속지 않게 앞에서부터 본다.
    let mut i = 0;
    while i + 4 <= k {
        if p[i..].starts_with(&['<', '!', '-', '-']) {
            let body = &p[i + 4..k];
            // 앞 조각에서 `-->` 가 없던 데까지는 다시 안 본다.
            let from = match memo.comment {
                Some((at, from)) if at == i => from.min(body.len()),
                _ => 0,
            };
            match body[from..].windows(3).position(|w| w == ['-', '-', '>']) {
                Some(end) => i += 4 + from + end + 3,
                None => {
                    // `--` 가 끝에 걸쳤을 수 있다 — 두 글자 앞에서 이어 본다.
                    memo.comment = Some((i, body.len().saturating_sub(2)));
                    k = i;
                    break;
                }
            }
        } else {
            i += 1;
        }
    }
    if let Some(at) = p[..k].iter().rposition(|&c| c == '<') {
        let body = &p[at + 1..k];
        if !p[at..k].contains(&'>') {
            let autolink = starts_with_chars(body, "http://") || starts_with_chars(body, "https://");
            if autolink {
                // **오토링크는 길이를 안 잰다.** 주소는 80자를 쉽게 넘고, 놓으면 `<` 가 글자로
                // 나가 링크가 죽는다(실제 문서를 64자 조각으로 흘리다 나왔다). 공백이 오면
                // 오토링크가 아니다 — 단 `<url|텍스트>` 의 텍스트에는 공백이 온다.
                let ws = body.iter().position(|c| c.is_whitespace());
                let bar = body.iter().position(|&c| c == '|');
                if ws.is_none_or(|w| bar.is_some_and(|b| b < w)) {
                    k = k.min(at);
                }
            } else {
                let tagish = p.get(at + 1).is_none_or(|&c| c.is_ascii_alphabetic() || c == '/' || c == '!');
                if tagish && k - at < 80 {
                    k = k.min(at);
                }
            }
        }
    }
    // **마커는 맨 마지막에 붙든다.** 다음 글자를 봐야 열기/닫기가 갈리는데, 위에서 `[` 나
    // `<` 를 붙들고 나면 그 앞의 마커가 다시 끝에 서기 때문이다 — `***[텍스트](…)` 의
    // `***` 가 `[` 를 못 본 채 나가면 스트리밍이 완성본과 갈린다(퍼즈에서 나왔다).
    // 역슬래시도 붙든다. 다음 글자를 봐야 **탈출인지 글자인지**가 갈린다.
    // `!` 도 붙든다 — 다음 글자가 `[` 면 이미지다(`![alt](url)`). 먼저 나가면 완성본과 갈린다.
    while k > 0 && matches!(p[k - 1], '*' | '_' | '~' | '`' | '\\' | ' ' | '\t' | '!') {
        k -= 1;
    }
    // **역슬래시와 그 다음 글자 사이에서는 끊지 않는다.** `[` 규칙이 `\[` 한가운데를
    // 가르면, 역슬래시만 먼저 나가서 탈출이 풀리지 않는다.
    while k > 0 && p[k - 1] == '\\' {
        k -= 1;
    }
    k
}

/// 붙든 구문을 어디까지 훑었나. 자리는 `pending` 기준이고, 앞이 `cut` 만큼 빠지면 당긴다.
#[derive(Default, Clone, Copy)]
pub(crate) struct HoldMemo {
    /// 여기 앞에는 안 닫힌 `[` 가 없다. 링크를 찾는 훑기가 여기서 잇는다.
    scanned: usize,
    /// 안 닫힌 `[` 의 자리와 그 안에서 훑던 상태.
    link: Option<(usize, LinkScan)>,
    /// 안 닫힌 `<!--` 의 자리와, 그 뒤에서 `-->` 를 이어 찾을 자리.
    comment: Option<(usize, usize)>,
}

impl HoldMemo {
    /// 앞 `n` 글자가 나갔다. 그 안에 있던 구문은 끝난 것이라 잊는다.
    fn shift(&mut self, n: usize) {
        self.scanned = self.scanned.saturating_sub(n);
        self.link = self.link.and_then(|(at, s)| at.checked_sub(n).map(|at| (at, s)));
        self.comment = self.comment.and_then(|(at, f)| at.checked_sub(n).map(|at| (at, f)));
    }
}

/// [`link_end_from`] 이 멈춘 자리. 자리는 `[` 로부터의 거리다.
#[derive(Clone, Copy)]
enum LinkScan {
    /// 짝이 되는 `]` 를 찾는 중. `j` 부터 본다.
    Bracket { j: usize, depth: usize },
    /// `]` 는 찾았고 다음 글자를 기다린다.
    Closed { at: usize },
    /// `](` 뒤에서 `)` 를 찾는 중.
    Paren { j: usize },
}

/// `[` 로 시작하는 조각이 링크로 끝나는 자리(`)` 다음). 링크가 아니면(`]` 뒤가 `(` 가 아니면)
/// `]` 다음이고, 아직 못 정하면 멈춘 자리를 `Err` 로 돌려준다 — 붙들고, 다음 조각에서 거기서
/// 잇는다. `]` 는 중첩 대괄호를 건너뛰어 찾는다 — 인라인 파서의 `find_link` 와 같은 규칙이다.
fn link_end_from(p: &[char], mut scan: LinkScan) -> Result<usize, LinkScan> {
    loop {
        scan = match scan {
            LinkScan::Bracket { mut j, mut depth } => {
                loop {
                    match p.get(j) {
                        None => return Err(LinkScan::Bracket { j, depth }),
                        Some('[') => depth += 1,
                        Some(']') if depth == 0 => break,
                        Some(']') => depth -= 1,
                        Some(_) => {}
                    }
                    j += 1;
                }
                LinkScan::Closed { at: j }
            }
            LinkScan::Closed { at } => match p.get(at + 1) {
                None => return Err(LinkScan::Closed { at }),
                Some('(') => LinkScan::Paren { j: at + 2 },
                Some(_) => return Ok(at + 1),
            },
            LinkScan::Paren { j } => {
                return match p[j.min(p.len())..].iter().position(|&c| c == ')') {
                    Some(c) => Ok(j + c + 1),
                    None => Err(LinkScan::Paren { j: p.len() }),
                };
            }
        }
    }
}

/// `chars` 가 `s` 로 시작하는가. `s` 가 더 길면 `chars` 전체가 `s` 의 앞부분이어야 한다 —
/// 아직 다 안 온 `<htt` 도 오토링크 후보로 붙든다.
fn starts_with_chars(chars: &[char], s: &str) -> bool {
    let mut it = chars.iter();
    for c in s.chars() {
        match it.next() {
            Some(&x) if x == c => {}
            Some(_) => return false,
            None => return true,
        }
    }
    true
}

/// 줄 끝 공백을 뺀 길이. 정돈의 일부다.
fn trim_end(p: &[char]) -> usize {
    let mut k = p.len();
    while k > 0 && (p[k - 1] == ' ' || p[k - 1] == '\t') {
        k -= 1;
    }
    k
}

fn run(t: &[char], c: char) -> usize {
    t.iter().take_while(|&&x| x == c).count()
}

fn push_usize(out: &mut String, mut n: usize) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    out.push_str(std::str::from_utf8(&buf[i..]).expect("숫자는 ASCII 다"));
}

/// ` ``` ` 또는 `~~~`. info 문자열까지 돌려준다.
fn fence_marker(t: &str) -> Option<(char, usize, &str)> {
    for ch in ['`', '~'] {
        let n = t.chars().take_while(|&c| c == ch).count();
        if n >= 3 {
            return Some((ch, n, t[n..].trim()));
        }
    }
    None
}

/// 표의 구분선인가. `|` 가 있어야 한다 — 없으면 그냥 구분선이다.
fn is_delimiter_row(line: &str) -> bool {
    let t = line.trim();
    t.contains('-')
        && t.contains('|')
        && t.chars().all(|c| matches!(c, '-' | ':' | '|' | ' ' | '\t'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Align {
    #[default]
    Left,
    Right,
    Center,
}

/// 표. **열 너비를 알려면 끝까지 봐야 하므로 여기만 버퍼링한다.**
#[derive(Default, Clone)]
pub(crate) struct Table {
    rows: Vec<Vec<String>>,
    align: Vec<Align>,
    cell: String,
}

impl Table {
    /// 머리글과 구분선으로 표를 시작한다. 모양이 안 맞으면 표가 아니다.
    fn begin(&mut self, header: &str, delim: &str) -> bool {
        let head = split_cells(header);
        let delim = split_cells(delim);
        if head.is_empty() || head.len() != delim.len() {
            return false;
        }
        self.align = delim
            .iter()
            .map(|d| {
                let d = d.trim();
                match (d.starts_with(':'), d.ends_with(':')) {
                    (true, true) => Align::Center,
                    (false, true) => Align::Right,
                    _ => Align::Left,
                }
            })
            .collect();
        self.rows.clear();
        self.rows.push(head);
        true
    }

    fn push(&mut self, line: &str) {
        self.rows.push(split_cells(line));
    }

    fn clear(&mut self) {
        self.rows.clear();
        self.align.clear();
    }

    /// 고정폭 블록으로 그린다. **열은 표시 폭으로 맞춘다** — 문자 수로 맞추면
    /// 한글이 든 표는 반드시 어긋난다(`SPEC.md` 7절).
    ///
    /// 돌려주는 것은 표를 여럿으로 나눈 자리(`out` 의 바이트 위치)다 — 노션 표가 한도를 넘을 때만 있다.
    fn render(&mut self, v: &Vocab, dialect: Dialect, repairs: &mut Repairs, out: &mut String) -> Vec<usize> {
        let cols = self.align.len();
        // 셀 안의 마크업은 고정폭 블록 안에서 살아남지 못한다. 글자로 내린다.
        // **표를 직접 그리는 채널은 예외다** — 거기서는 셀도 그 채널 표기로 낸다.
        let plain = Vocab::new(Channel::Plain);
        let cell_vocab = if v.tables_native() { v } else { &plain };
        let rows = std::mem::take(&mut self.rows);
        let mut cells: Vec<Vec<String>> = Vec::with_capacity(rows.len());
        // 셀 안의 방언은 문서를 따른다. 셀에서 고친 것도 문서의 것으로 센다 — 본문의
        // `**x` 를 닫아 주면 세는데, 같은 것이 셀 안에 있다고 빠지면 표가 든 문서만 덜 센다.
        let mut inline = Inline::new(dialect);
        inline.in_cell = true;
        let mut chars: Vec<char> = Vec::new();
        for row in &rows {
            let mut line = Vec::with_capacity(cols);
            for c in 0..cols {
                self.cell.clear();
                chars.clear();
                // **남는 칸을 버리지 않는다.** 머리글보다 칸이 많은 줄을 GFM 은 잘라
                // 내지만, 그러면 저자가 쓴 내용이 소리 없이 사라진다 — 코드 스팬이나
                // 위키링크 안의 `|` 가 칸을 갈라 놓는 것이 실제로 그랬다. 넘치는 것은
                // 마지막 칸에 이어 붙인다.
                if c + 1 == cols && row.len() > cols {
                    // 재사용 버퍼에 바로 이어 붙인다. `join` 으로 중간 문자열을
                    // 만들면 넘치는 줄마다 할당이 하나씩 더 든다.
                    for (k, cell) in row[c..].iter().enumerate() {
                        if k > 0 {
                            chars.extend(" | ".chars());
                        }
                        chars.extend(cell.chars());
                    }
                } else {
                    chars.extend(row.get(c).map(String::as_str).unwrap_or("").chars());
                }
                inline.render(&chars, &mut self.cell, cell_vocab);
                inline.finish_block(&mut self.cell, cell_vocab);
                inline.reset();
                line.push(std::mem::take(&mut self.cell));
            }
            cells.push(line);
        }
        self.rows = rows;
        repairs.add(inline.repairs);

        if v.is_html() {
            write_html_table(out, &cells, &self.align);
            return Vec::new();
        }
        if v.xml_tables() {
            return write_notion_table(out, &cells, v.limit);
        }
        if v.tables_native() {
            write_gfm_table(out, &cells, &self.align);
            return Vec::new();
        }

        let mut widths = vec![0usize; cols];
        for row in &cells {
            for (c, cell) in row.iter().enumerate() {
                widths[c] = widths[c].max(str_width(cell));
            }
        }

        let mut body = String::new();
        for (r, row) in cells.iter().enumerate() {
            if r > 0 {
                body.push('\n');
            }
            write_row(&mut body, row, &widths, &self.align);
            if r == 0 {
                body.push('\n');
                for (c, w) in widths.iter().enumerate() {
                    if c > 0 {
                        body.push_str(" | ");
                    }
                    for _ in 0..*w {
                        body.push('-');
                    }
                }
            }
        }

        v.verbatim_open("", out);
        if v.verbatim_body_newline() {
            out.push('\n');
        }
        v.escape(&body, out);
        v.verbatim_close("", out);
        Vec::new()
    }
}

/// 표를 `<table>` 로 낸다(HTML). 칸은 이미 escape·렌더된 것을 받는다.
fn write_html_table(out: &mut String, cells: &[Vec<String>], align: &[Align]) {
    out.push_str("<table>");
    for (r, row) in cells.iter().enumerate() {
        let tag = if r == 0 { "th" } else { "td" };
        if r == 0 {
            out.push_str("\n<thead>");
        } else if r == 1 {
            out.push_str("\n<tbody>");
        }
        out.push_str("<tr>");
        for (c, cell) in row.iter().enumerate() {
            out.push('<');
            out.push_str(tag);
            // 왼쪽은 기본값과 구분되지 않아(`---` 도 `:--` 도 Left) 적지 않는다.
            match align.get(c) {
                Some(Align::Right) => out.push_str(" style=\"text-align:right\""),
                Some(Align::Center) => out.push_str(" style=\"text-align:center\""),
                _ => {}
            }
            out.push('>');
            out.push_str(cell);
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        out.push_str("</tr>");
        if r == 0 {
            out.push_str("</thead>");
        }
    }
    if cells.len() > 1 {
        out.push_str("</tbody>");
    }
    out.push_str("\n</table>");
}

/// 표를 노션 `<table>` 로 낸다. 칸은 노션 마크다운(굵게 `**` 따위)으로 렌더된 것을 받는다.
///
/// **GFM 파이프 표로 내면 칸 안의 `|` 가 칸을 가른다**(실측 2026-10-01). 노션은 코드 스팬 안의
/// `` `a|b` `` 도, 탈출한 `a \| b` 도 칸 경계로 읽어 뒤의 내용을 버렸다. `<table>` 로 쓰면 `|` 는
/// 그냥 글자다. 정렬은 노션 표에 없어 버린다. 칸에 글자로 적힌 태그 모양(`</td>`)은 표를 깨므로
/// 탈출한다([`push_notion_cell`]).
///
/// **한도를 넘으면 머리글을 되풀이한 표 여럿으로 낸다.** 분할기는 태그를 모르고 줄로 끊어서, 한
/// 표를 가르면 앞 조각은 `</table>` 없이 끝나고 뒤 조각은 `<tr>` 로 시작한다. 머리글과 함께 한도에
/// 안 드는 행은 표 밖의 글(칸을 ` | ` 로 이은 줄)로 내린다 — 표는 깨지지 않고 내용은 남는다. 머리글
/// 하나가 한도를 넘으면 표 전체를 그렇게 내린다. 나눈 자리(다음 조각을 여는 빈 줄의 위치)를
/// 돌려주면 엔진이 그 사이를 블록 경계로 내보낸다.
fn write_notion_table(out: &mut String, cells: &[Vec<String>], limit: usize) -> Vec<usize> {
    const OPEN: &str = "<table header-row=\"true\">";
    const CLOSE: &str = "\n</table>";
    let cells: Vec<Vec<String>> = cells
        .iter()
        .map(|row| {
            row.iter()
                .map(|c| {
                    let mut e = String::with_capacity(c.len());
                    push_notion_cell(c, &mut e);
                    e
                })
                .collect()
        })
        .collect();
    let row_len = |row: &[String]| -> usize {
        "\n<tr>\n</tr>".len() + row.iter().map(|c| "\n<td></td>".len() + c.chars().count()).sum::<usize>()
    };
    let write_row = |out: &mut String, row: &[String]| {
        out.push_str("\n<tr>");
        for cell in row {
            out.push_str("\n<td>");
            out.push_str(cell);
            out.push_str("</td>");
        }
        out.push_str("\n</tr>");
    };
    let write_text = |out: &mut String, row: &[String]| {
        for (k, cell) in row.iter().enumerate() {
            if k > 0 {
                out.push_str(" | ");
            }
            out.push_str(cell);
        }
    };
    let Some((head, body)) = cells.split_first() else { return Vec::new() };
    let base = OPEN.len() + CLOSE.len() + row_len(head);
    if base > limit {
        for (r, row) in cells.iter().enumerate() {
            if r > 0 {
                out.push('\n');
            }
            write_text(out, row);
        }
        return Vec::new();
    }
    let mut breaks = Vec::new();
    // 이 표 블록에 무엇이든 썼는가 — 다음 조각 앞에 경계와 빈 줄을 둔다.
    let mut wrote = false;
    let mut open = false;
    let mut used = 0;
    let mut piece = |out: &mut String, wrote: &mut bool| {
        if *wrote {
            breaks.push(out.len());
            out.push_str("\n\n");
        }
        *wrote = true;
    };
    for row in body {
        let n = row_len(row);
        if base + n > limit {
            if open {
                out.push_str(CLOSE);
                open = false;
            }
            piece(out, &mut wrote);
            write_text(out, row);
            continue;
        }
        if open && used + n > limit {
            out.push_str(CLOSE);
            open = false;
        }
        if !open {
            piece(out, &mut wrote);
            out.push_str(OPEN);
            write_row(out, head);
            open = true;
            used = base;
        }
        write_row(out, row);
        used += n;
    }
    if open {
        out.push_str(CLOSE);
    } else if !wrote {
        // 머리글뿐인 표.
        out.push_str(OPEN);
        write_row(out, head);
        out.push_str(CLOSE);
    }
    breaks
}

/// 노션 표 칸 하나를 적는다. **글자로 적힌 태그 모양은 탈출한다** — 칸 안의 `</td>` 는 노션이 칸을
/// 닫는 태그로 읽어 뒤의 내용을 버렸다(실측). `\<` 만으로는 안 되고 `\<\/td\>` 로 `/` 와 `>` 까지
/// 막아야 글자로 남았다. `<br>` 은 노션이 그리는 줄바꿈이라 둔다.
fn push_notion_cell(cell: &str, out: &mut String) {
    let b = cell.as_bytes();
    let mut i = 0;
    let mut in_tag = false;
    while i < b.len() {
        let c = b[i];
        if in_tag && c == b'>' {
            out.push_str("\\>");
            in_tag = false;
            i += 1;
            continue;
        }
        if c == b'<' && !in_tag {
            let slash = b.get(i + 1) == Some(&b'/');
            let name_at = i + 1 + usize::from(slash);
            let is_br = !slash && cell[i..].get(..4).is_some_and(|t| t.eq_ignore_ascii_case("<br>"));
            if !is_br && b.get(name_at).is_some_and(u8::is_ascii_alphabetic) {
                out.push_str(if slash { "\\<\\/" } else { "\\<" });
                in_tag = true;
                i = name_at;
                continue;
            }
        }
        // 한 바이트씩 옮기되 글자를 가르지 않는다 — 여러 바이트 글자는 통째로.
        let len = utf8_len(c);
        out.push_str(&cell[i..i + len]);
        i += len;
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// 표를 GFM 그대로 낸다. 채널이 직접 그리는 곳용이라 열 너비를 맞추지 않는다.
///
/// 셀 안의 `|` 는 다시 `\|` 로 돌린다 — 안 그러면 읽는 쪽에서 칸이 갈린다.
fn write_gfm_table(out: &mut String, cells: &[Vec<String>], align: &[Align]) {
    let write_cells = |out: &mut String, row: &[String]| {
        out.push('|');
        for cell in row {
            out.push(' ');
            for c in cell.chars() {
                if c == '|' {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push_str(" |");
        }
    };
    for (r, row) in cells.iter().enumerate() {
        if r > 0 {
            out.push('\n');
        }
        write_cells(out, row);
        if r == 0 {
            // 구분선은 머리글 칸 수를 따른다. 정렬 정보가 모자라면 왼쪽 정렬로 채운다 —
            // 칸 수가 어긋난 구분선은 읽는 쪽이 표로 안 받는다.
            out.push_str("\n|");
            for i in 0..row.len() {
                out.push_str(match align.get(i).unwrap_or(&Align::Left) {
                    Align::Left => " --- |",
                    Align::Right => " ---: |",
                    Align::Center => " :---: |",
                });
            }
        }
    }
}

fn write_row(out: &mut String, row: &[String], widths: &[usize], align: &[Align]) {
    let last = row.len().saturating_sub(1);
    for (c, cell) in row.iter().enumerate() {
        if c > 0 {
            out.push_str(" | ");
        }
        let pad = widths[c].saturating_sub(str_width(cell));
        let (before, after) = match align.get(c).copied().unwrap_or_default() {
            Align::Left => (0, pad),
            Align::Right => (pad, 0),
            Align::Center => (pad / 2, pad - pad / 2),
        };
        for _ in 0..before {
            out.push(' ');
        }
        out.push_str(cell);
        // 마지막 열의 오른쪽 여백은 줄 끝 공백일 뿐이라 남기지 않는다.
        if c != last {
            for _ in 0..after {
                out.push(' ');
            }
        }
    }
}

/// `| a | b |` 를 셀로 나눈다. `\|` 는 셀 안의 파이프다.
fn split_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut escaped = false;
    for c in t.chars() {
        match c {
            '\\' if !escaped => escaped = true,
            '|' if !escaped => {
                cells.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => {
                if escaped && c != '|' {
                    cur.push('\\');
                }
                escaped = false;
                cur.push(c);
            }
        }
    }
    cells.push(cur.trim().to_string());
    cells
}
