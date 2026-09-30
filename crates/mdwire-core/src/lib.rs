//! mdwire — 에이전트가 만든 마크다운을 채팅 채널로 안전하게 내보낸다.
//!
//! 세 가지를 한 파이프라인에서 한다. 순서가 곧 설계다.
//!
//! 1. **정규화** — LLM 출력은 올바른 CommonMark 가 아니다. 짝이 안 맞는 강조,
//!    줄을 넘는 강조, 안 닫힌 코드펜스가 일상이다. 먼저 복구한다.
//! 2. **채널 렌더링** — 타깃이 받는 문법으로 옮긴다. 타깃이 못 받는 구문은
//!    파싱할 이유도 없다 (아래 `Channel` 주석 참고).
//! 3. **안전 분할** — 채널 한도와 스트리밍 경계에서, 마크업 한가운데를 자르지 않는다.
//!
//! # 왜 의존성이 없나
//!
//! 기성 파서는 전부 **배치형**이다 — 문서 전체를 받아 AST 를 만든 뒤 렌더한다.
//! 토큰이 흘러들어오는 대로 내보내야 하는 이 문제에는 처음부터 맞지 않는다.
//! 그리고 CJK 인접 강조 정책은 파서 안에 박혀 있어서, 남의 것을 쓰면 못 바꾼다.
//! 그게 이 라이브러리가 고치려는 바로 그 문제다.

#![forbid(unsafe_code)]

mod block;
mod inline;
mod sink;
mod vocab;
pub mod width;

use block::Engine;
use sink::{PartsSink, StringSink};
use vocab::Vocab;

/// 내보낼 채널. 받는 문법이 채널마다 다르고, **출력이 좁은 쪽이 파싱 범위를 정한다**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Telegram `parse_mode=HTML`. 허용 태그 9개:
    /// `b i u s code pre a blockquote tg-spoiler`. 표·헤딩 없음. 4096자.
    TelegramHtml,
    /// Slack `markdown_text`. 표준 마크다운을 슬랙이 직접 변환한다. 12,000자.
    /// 변환이 거의 필요 없고, 남는 일은 정규화와 분할뿐이다.
    SlackMarkdown,
    /// GitHub 코멘트·PR 본문(GFM). 표·헤딩·취소선을 다 그린다. 65,536자.
    /// 슬랙과 같은 마크다운을 내되, GFM 이 구문으로 읽는 글자 둘(`~` `<`)을 탈출한다.
    GithubMarkdown,
    /// 모든 마크업 제거. 폴백 경로.
    Plain,
    /// 브라우저에 넣을 HTML 조각. 헤딩·목록·표·코드블록을 태그로 그린다. 한도 없음.
    ///
    /// `innerHTML` 로 바로 넣는 것을 전제로 한다 — 글자는 전부 escape 하고, 원문의 HTML 은
    /// 속성을 버린 인라인 태그만 살리며, 링크는 `http(s)`·`mailto` 만 `<a>` 로 낸다.
    /// 스트리밍 누적본에 [`Streamer::close_open`] 을 붙이면 그대로 넣어도 되는 모양이 된다.
    Html,
}

impl Channel {
    /// 코퍼스 디렉토리와 CLI 인자에서 쓰는 이름. 채널을 문자열로 다루는 곳의 정본이다.
    pub fn name(self) -> &'static str {
        match self {
            Channel::TelegramHtml => "telegram-html",
            Channel::SlackMarkdown => "slack-markdown",
            Channel::GithubMarkdown => "github-markdown",
            Channel::Plain => "plain",
            Channel::Html => "html",
        }
    }

    /// 내보낼 수 있는 채널 전부. 코퍼스와 하네스가 이 목록을 돈다.
    pub fn all() -> [Channel; 5] {
        [Channel::TelegramHtml, Channel::SlackMarkdown, Channel::GithubMarkdown, Channel::Plain, Channel::Html]
    }

    /// 이름으로 채널을 찾는다.
    pub fn parse(name: &str) -> Option<Channel> {
        Self::all().into_iter().find(|c| c.name() == name)
    }

    /// 이 채널의 메시지 길이 한도(문자 수). 분할의 기준이다.
    pub fn limit(self) -> usize {
        match self {
            Channel::TelegramHtml => 4096,
            Channel::SlackMarkdown | Channel::Plain => 12_000,
            // 코멘트 본문의 한도다. 넘기면 API 가 422 로 거절한다("Body is too long").
            Channel::GithubMarkdown => 65_536,
            // 브라우저에는 메시지 한도가 없다. 나누지 않는다.
            Channel::Html => usize::MAX,
        }
    }
}

/// 입력 방언 — 에이전트가 무슨 표기로 썼는가.
///
/// 기본은 표준 마크다운이다. 슬랙에 답하는 에이전트는 흔히 **레거시 `mrkdwn`** 으로 쓴다
/// (슬랙 문서가 그렇게 가르친다) — `*굵게*` · `_기울임_` · `~취소~`. 표준으로 읽으면
/// `*굵게*` 가 기울임이 되고 `~취소~` 는 글자로 남는다. 출력 채널과는 따로 정한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dialect {
    /// 표준 마크다운(CommonMark · GFM).
    #[default]
    Markdown,
    /// 슬랙 레거시 `mrkdwn`. 별표는 몇 개든 굵게, 물결은 하나든 둘이든 취소선이다.
    /// 표준 표기(`**굵게**` · `~~취소~~` · `[텍스트](url)`)가 섞여도 같은 뜻으로 읽는다.
    SlackMrkdwn,
}

impl Dialect {
    /// CLI 인자와 바인딩에서 쓰는 이름.
    pub fn name(self) -> &'static str {
        match self {
            Dialect::Markdown => "markdown",
            Dialect::SlackMrkdwn => "slack-mrkdwn",
        }
    }

    /// 이름으로 방언을 찾는다.
    pub fn parse(name: &str) -> Option<Dialect> {
        [Dialect::Markdown, Dialect::SlackMrkdwn].into_iter().find(|d| d.name() == name)
    }
}

/// 변환 옵션 — 입력 방언과 조각 한도.
///
/// 필드가 늘 수 있으니 `Options { from, ..Default::default() }` 로 만든다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    pub from: Dialect,
    /// 한 조각의 한도(렌더한 출력의 글자 수). `None` 이면 [`Channel::limit`].
    ///
    /// **한도는 보내는 쪽이 정한다.** 채널이 정해 주지 못하는 경우가 있다 — plain 은 어디로
    /// 가는지 모르는 폴백이라 텔레그램으로 보내면 4096 이어야 하고(12,000 으로 나눈 7,153자
    /// 조각이 400 을 받았다), 앞에 제목을 붙여 보내는 쪽은 그만큼 덜 써야 한다.
    /// 스트리밍([`Streamer`])은 나누지 않으므로 이 값을 보지 않는다.
    pub limit: Option<usize>,
}

/// 정규화가 고친 것의 개수. **모델이 얼마나 자주 서식을 깨는지**를 재는 데 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Repairs {
    /// 블록이 끝나도록 안 닫혀서 닫아 준 강조(`**영향 범위` 처럼).
    pub closed_emphasis: usize,
    /// 문서 끝까지 안 닫혀서 닫아 준 코드펜스.
    pub closed_fence: usize,
    /// 짝이 없어 코드가 아니라 글자로 되돌린 백틱 런.
    pub reverted_code_span: usize,
    /// 짝 잃은 채 버린 `**` (`꼬리**` 처럼 앞이 글자인 것).
    pub dropped_marker: usize,
}

impl Repairs {
    pub(crate) fn add(&mut self, other: Repairs) {
        self.closed_emphasis += other.closed_emphasis;
        self.closed_fence += other.closed_fence;
        self.reverted_code_span += other.reverted_code_span;
        self.dropped_marker += other.dropped_marker;
    }

    /// 하나라도 고쳤는가.
    pub fn any(&self) -> bool {
        *self != Repairs::default()
    }
}

/// [`render_with`] 의 결과 — 조각과 고친 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub parts: Vec<String>,
    pub repairs: Repairs,
}

/// 스트리밍 변환기.
///
/// 조각을 넣으면 **지금 안전하게 내보낼 수 있는 만큼만** 돌려준다.
/// 경계에 걸린 마크업(`**굵` 에서 끊긴 것)은 안에 남겨 두고 다음 조각을 기다린다.
/// 이것이 이 라이브러리의 핵심이다 — 완성본 변환은 이미 남들이 푼 문제고,
/// 경계 문제는 스트리밍을 하는 한 채널과 무관하게 생긴다.
///
/// # 예
///
/// ```
/// use mdwire::{Channel, Streamer};
///
/// let mut s = Streamer::new(Channel::TelegramHtml);
/// let mut out = String::new();
/// // 조각 경계가 `**` 한가운데를 지나가도 반쪽으로 나가지 않는다.
/// out.push_str(s.push("앞말 **굵"));
/// out.push_str(s.push("게** 뒷말"));
/// out.push_str(s.finish());
/// assert_eq!(out, "앞말 <b>굵게</b> 뒷말");
/// ```
pub struct Streamer {
    engine: Engine,
    /// [`Streamer::push`] 가 빌려주는 버퍼. 재사용하므로 조각마다 할당하지 않는다.
    buf: String,
    /// 줄바꿈이 아닌 글자를 하나라도 내보냈는가.
    ///
    /// **앞머리 빈 줄은 내보내지 않는다.** 문서가 주석이나 `<br>` 로 시작하면 첫 블록이
    /// 비고 그 뒤의 줄바꿈만 남는데, 완성본은 조각 앞머리의 줄바꿈을 털고 시작한다
    /// (`sink::PartsSink`). 스트리밍도 같아야 한다 — 그래야 둘이 같은 답을 낸다.
    started: bool,
}

impl Streamer {
    pub fn new(channel: Channel) -> Self {
        Self::with_options(channel, Options::default())
    }

    /// 옵션을 주고 만든다 — 입력 방언 따위.
    pub fn with_options(channel: Channel, options: Options) -> Self {
        Self { engine: Engine::new(channel, options), buf: String::new(), started: false }
    }

    /// 지금까지 정규화가 고친 것. `finish` 뒤에 보면 문서 전체의 값이다.
    pub fn repairs(&self) -> Repairs {
        self.engine.repairs()
    }

    /// `from` 뒤에 새로 붙은 출력에서 앞머리 줄바꿈을 턴다. 첫 글자가 나올 때까지만이다.
    fn trim_leading(&mut self, out: &mut String, from: usize) {
        if self.started {
            return;
        }
        let fresh = &out[from..];
        let keep = fresh.len() - fresh.trim_start_matches('\n').len();
        if keep > 0 {
            out.drain(from..from + keep);
        }
        if out.len() > from {
            self.started = true;
        }
    }

    /// 조각을 밀어 넣고, 지금 내보낼 수 있는 출력을 받는다.
    ///
    /// 돌려주는 슬라이스는 **다음 호출 전까지만** 유효하다. 할당을 아예 없애려면
    /// [`Streamer::push_into`] 를 쓴다 — 둘은 같은 코드를 부른다(`SPEC.md` 5절).
    pub fn push(&mut self, chunk: &str) -> &str {
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        self.push_into(chunk, &mut buf);
        self.buf = buf;
        &self.buf
    }

    /// 호출자 버퍼에 직접 쓴다. 정본 서명 — 조각당 할당이 0 이다.
    pub fn push_into(&mut self, chunk: &str, out: &mut String) {
        let from = out.len();
        let mut sink = StringSink(out);
        self.engine.feed(chunk, &mut sink);
        self.trim_leading(out, from);
    }

    /// 입력이 끝났다. 남은 것을 전부 내보낸다(열린 마크업은 닫는다).
    pub fn finish(&mut self) -> &str {
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        self.finish_into(&mut buf);
        self.buf = buf;
        &self.buf
    }

    /// [`Streamer::finish`] 의 무할당 판.
    pub fn finish_into(&mut self, out: &mut String) {
        let from = out.len();
        let mut sink = StringSink(out);
        self.engine.finish(&mut sink);
        self.trim_leading(out, from);
    }

    /// **지금까지 받은 것을 그대로 보내도 되게 만든다.** 상태는 건드리지 않으므로
    /// 붙인 뒤에도 스트리밍은 이어진다.
    ///
    /// 강조는 짝이 맞을 때까지 안에 붙들려 있어 이미 균형이 맞지만, 블록의 여는
    /// 마크업(`<blockquote>`·`<pre>`·헤딩의 `<b>`)은 블록이 끝나기 전에 나간다 —
    /// 코드블록이 끝날 때까지 출력을 멈추면 스트리밍이 아니기 때문이다. 누적본을
    /// 중간에 채널로 보내는 쪽(토큰이 오는 대로 메시지를 편집하는 경우)은 보내기
    /// 직전에 이걸 덧붙인다. **누적본 자체에는 넣지 않는다** — 다음 조각이 이어진다.
    ///
    /// ```
    /// use mdwire::{Channel, Streamer};
    ///
    /// let mut s = Streamer::new(Channel::TelegramHtml);
    /// let mut acc = String::new();
    /// s.push_into("> 인용이 시작되고", &mut acc);
    ///
    /// let mut snapshot = acc.clone();
    /// s.close_open(&mut snapshot);          // 지금 보내도 되는 모양
    /// assert_eq!(snapshot, "<blockquote>인용이 시작되고</blockquote>");
    ///
    /// s.push_into("\n> 이어진다\n", &mut acc);  // 누적본은 그대로 이어진다
    /// s.finish_into(&mut acc);
    /// assert_eq!(acc, "<blockquote>인용이 시작되고\n이어진다</blockquote>");
    /// ```
    pub fn close_open(&self, out: &mut String) {
        self.engine.close_open(out);
    }
}

/// 완성된 문서를 한 번에 변환한다. 한도를 넘으면 안전한 지점에서 나눈다.
///
/// 나누는 자리는 **렌더 결과가 아니라 구조에서** 고른다 — 블록이 끝나 열린 마크업이
/// 없는 지점만 경계가 된다. 변환 후에 문자 수로 자르면 `<code>` 가 열린 채 잘리고,
/// 채널은 400 을 준다(`DESIGN.md`).
///
/// # 예
///
/// ```
/// use mdwire::{render, Channel};
///
/// let parts = render("## 제목\n\n**굵게** 있는 문단", Channel::TelegramHtml);
/// assert_eq!(parts, vec!["<b>제목</b>\n\n<b>굵게</b> 있는 문단"]);
/// ```
pub fn render(input: &str, channel: Channel) -> Vec<String> {
    render_with(input, channel, Options::default()).parts
}

/// [`render`] 에 옵션을 주고, 정규화가 고친 것도 같이 받는다.
///
/// ```
/// use mdwire::{render_with, Channel, Dialect, Options};
///
/// let options = Options { from: Dialect::SlackMrkdwn, ..Default::default() };
/// let out = render_with("*굵게* 는 **영향 범위", Channel::SlackMarkdown, options);
/// assert_eq!(out.parts, vec!["**굵게** 는 **영향 범위**"]);
/// assert_eq!(out.repairs.closed_emphasis, 1);
/// ```
pub fn render_with(input: &str, channel: Channel, options: Options) -> Rendered {
    let mut engine = Engine::new(channel, options);
    let mut sink = PartsSink::new(Vocab::with_limit(channel, options.limit));
    engine.feed(input, &mut sink);
    engine.finish(&mut sink);
    Rendered { repairs: engine.repairs(), parts: sink.into_parts() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_limits_are_channel_specific() {
        assert_eq!(Channel::TelegramHtml.limit(), 4096);
        assert_eq!(Channel::SlackMarkdown.limit(), 12_000);
        assert_eq!(Channel::GithubMarkdown.limit(), 65_536);
    }

    /// **조각 하나가 곧 메시지 하나다.** 태그 한가운데서 끊으면 조각이 `… <a ` 로
    /// 끝나고, 채널은 그 메시지를 통째로 거절한다. 실제 문서(링크가 달린 긴 목록)에서
    /// 나온 고장이다.
    #[test]
    fn parts_never_end_inside_a_tag() {
        let mut input = String::new();
        for i in 0..40 {
            input.push_str(&format!(
                "- `method{i}(options?: SomeLongOptionsType{i}): string` — 주소의 뒤집힌 꼴을 \
                 돌려준다 [src](https://example.com/owner/repo/blob/master/src/mod{i}.ts#L{i}28)\n"
            ));
        }
        let parts = render(&input, Channel::TelegramHtml);
        assert!(parts.len() > 1, "한도를 넘겨서 나뉘어야 하는 입력이다");
        for (i, p) in parts.iter().enumerate() {
            assert_eq!(
                p.matches('<').count(),
                p.matches('>').count(),
                "조각 {i} 가 태그 한가운데서 끊겼다: …{}",
                &p[p.len().saturating_sub(40)..]
            );
            assert!(p.chars().count() <= Channel::TelegramHtml.limit(), "조각 {i} 가 한도를 넘었다");
        }
    }
}
