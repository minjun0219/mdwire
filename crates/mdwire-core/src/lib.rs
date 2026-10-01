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
    /// 슬랙과 같은 마크다운을 내되, GFM 이 구문으로 읽는 글자 둘(`~` `<`)을 이스케이프한다.
    GithubMarkdown,
    /// 노션 페이지 본문(Notion-flavored Markdown — API `markdown` 필드·커넥터). 헤딩은 네 단계.
    /// GitHub 과 같은 마크다운을 내되, 노션이 못 그리는 인라인 HTML 은 벗기고(글자로 보인다),
    /// 오토링크 `<url>` 은 `[url](url)` 로 쓴다(꺾쇠가 글자로 남는다). 조사 앞 강조는 노션이 그대로
    /// 그려서 `<strong>` 으로 바꾸지 않는다 — 바꾸면 오히려 태그가 글자로 보인다.
    NotionMarkdown,
    /// 모든 마크업 제거. 폴백 경로.
    Plain,
    /// 브라우저에 넣을 HTML 조각. 헤딩·목록·표·코드블록을 태그로 그린다. 한도 없음.
    ///
    /// `innerHTML` 로 바로 넣는 것을 전제로 한다 — 글자는 전부 이스케이프하고, 원문의 HTML 은
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
            Channel::NotionMarkdown => "notion-markdown",
            Channel::Plain => "plain",
            Channel::Html => "html",
        }
    }

    /// 내보낼 수 있는 채널 전부. 코퍼스와 하네스가 이 목록을 돈다.
    pub fn all() -> [Channel; 6] {
        [
            Channel::TelegramHtml,
            Channel::SlackMarkdown,
            Channel::GithubMarkdown,
            Channel::NotionMarkdown,
            Channel::Plain,
            Channel::Html,
        ]
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
            // **재지 않았다.** 페이지 하나에 들어갈 본문이라 GitHub 과 같은 값을 둔다. 보내는 쪽
            // 한도가 따로 있으면 [`Options::limit`] 으로 준다.
            Channel::NotionMarkdown => 65_536,
            // 브라우저에는 메시지 한도가 없다. 나누지 않는다.
            Channel::Html => usize::MAX,
        }
    }
}

/// 입력 표기 — 에이전트가 무슨 표기로 썼는가.
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

    /// 이름으로 입력 표기를 찾는다.
    pub fn parse(name: &str) -> Option<Dialect> {
        [Dialect::Markdown, Dialect::SlackMrkdwn].into_iter().find(|d| d.name() == name)
    }
}

/// 호출자가 줄 수 있는 가장 작은 조각 한도.
///
/// **조각마다 마크업을 닫고 다시 열 자리가 있어야 한다.** 한도가 태그보다 작으면 분할기가 태그 글자
/// 사이를 가른다 — 텔레그램 `**x**` 를 한도 1 로 나누면 `<` · `b` · `></b>` 가 됐다(리뷰에서 나왔다).
/// 중첩된 여는 태그 몇 개(인용·굵게·코드·`<pre><code class="language-…">`)가 들어가고도 내용이 남는
/// 값이다. 실제 쓰임(텔레그램 4096 에서 머리글 몫을 빼는 것)과는 거리가 멀다.
pub const MIN_LIMIT: usize = 256;

/// 변환 옵션 — 입력 표기, 조각 한도, 브라우저 채널의 정책.
///
/// 필드가 늘 수 있으니 `Options { from, ..Default::default() }` 로 만든다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    pub from: Dialect,
    /// 한 조각의 한도(렌더한 출력의 글자 수). `None` 이면 [`Channel::limit`].
    ///
    /// **한도는 보내는 쪽이 정한다.** 채널이 정해 주지 못하는 경우가 있다 — plain 은 어디로
    /// 가는지 모르는 폴백이라 텔레그램으로 보내면 4096 이어야 하고(12,000 으로 나눈 7,153자
    /// 조각이 400 을 받았다), 앞에 제목을 붙여 보내는 쪽은 그만큼 덜 써야 한다.
    /// 스트리밍([`Streamer`])은 나누지 않으므로 이 값을 보지 않는다 — 노션 표만 예외다. 한도를 넘는
    /// 표는 머리글을 되풀이한 표 여럿으로 내는데, 이건 표의 모양이라 스트리밍도 같게 낸다. 브라우저 채널([`Channel::Html`])도
    /// 나누지 않는다 — 분할기가 블록 태그를 여닫지 않아 태그 한가운데서 갈린다. [`MIN_LIMIT`] 보다 작은
    /// 값은 그만큼 올린다.
    pub limit: Option<usize>,
    /// 브라우저 채널([`Channel::Html`])의 정책. 다른 채널은 보지 않는다.
    pub html: HtmlOptions,
}

/// 브라우저 채널의 정책. 기본값이 가장 보수적이다 — `<br>` 줄바꿈, 이미지는 링크로만,
/// 링크는 `http`·`https`·`mailto` 만.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HtmlOptions {
    pub line_breaks: LineBreaks,
    pub images: Images,
    /// 링크·이미지 주소로 받는 스킴(`"https"` 처럼 콜론 없이). `None` 이면 `http`·`https`·
    /// `mailto`. 목록을 주면 **그것만** 받는다 — 기본값에 더하는 것이 아니다.
    pub schemes: Option<Vec<String>>,
}

/// 블록 안의 줄바꿈을 어떻게 낼지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineBreaks {
    /// `<br>` — 채팅·메모처럼 저자의 줄바꿈이 뜻인 글. 다른 채널이 다 줄바꿈을 살린다.
    #[default]
    Br,
    /// 줄바꿈 글자만 — 브라우저가 공백으로 접는다. 80열로 wrap 된 문서를 문단으로 읽을 때.
    Space,
}

/// 이미지 `![alt](url)` 을 어떻게 낼지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Images {
    /// `<a href>alt</a>` — 누르기 전에는 아무것도 불러오지 않는다(추적 픽셀이 없다).
    #[default]
    Link,
    /// `<img src alt>` — 주소가 허용 스킴일 때만. 아니면 `Link` 처럼 낸다.
    Load,
}

/// 정규화가 고친 것과 채널에 맞춰 바꾼 것의 개수. 앞 넷(고친 것)은 **모델이 얼마나 자주 서식을
/// 깨는지**를, 뒤 여섯(바꾼 것)은 **채널을 들이기 전에 그 채널이 무엇을 바꾸는지**를 재는 데 쓴다 — 둘을 따로
/// 물으려면 [`Repairs::any`]·[`Repairs::changed`].
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
    /// 채널이 구문으로 읽을 글자를 이스케이프한 수(GitHub 의 `\~`·`\<`·`\*`).
    pub escaped_char: usize,
    /// 마커 대신 태그로 낸 강조 — GitHub 이 마커로 못 읽는 자리(`**「설정」**가`)의 `<strong>`.
    pub tag_emphasis: usize,
    /// 벗긴 원문 HTML — 그 채널이 못 그리는 태그, 주석, 줄바꿈으로 바꾼 `<br>`.
    pub stripped_html: usize,
    /// 다른 기호로 바꿔 쓴 목록 기호 — 불릿(`* `·`• ` → `- `, 텔레그램은 `- ` → `• `)과 번호(`1)` → `1.`).
    pub rewritten_bullet: usize,
    /// 원문과 다른 모양으로 다시 쓴 표(구분선·칸 공백 정규화, 고정폭으로 내림).
    pub rewritten_table: usize,
    /// 다른 표기로 바꿔 쓴 강조 마커와 링크 — mrkdwn `*굵게*` → `**굵게**`, `_기울임_` → `*기울임*`,
    /// `<url|텍스트>` → `[텍스트](url)`. 마크다운을 내는 채널에서만 센다.
    pub converted_marker: usize,
}

impl Repairs {
    pub(crate) fn add(&mut self, other: Repairs) {
        self.closed_emphasis += other.closed_emphasis;
        self.closed_fence += other.closed_fence;
        self.reverted_code_span += other.reverted_code_span;
        self.dropped_marker += other.dropped_marker;
        self.escaped_char += other.escaped_char;
        self.tag_emphasis += other.tag_emphasis;
        self.stripped_html += other.stripped_html;
        self.rewritten_bullet += other.rewritten_bullet;
        self.rewritten_table += other.rewritten_table;
        self.converted_marker += other.converted_marker;
    }

    /// 정규화가 하나라도 **고쳤는가** — 앞 넷(닫아 준 강조·펜스, 되돌린 백틱, 버린 마커). 모델이 서식을
    /// 깼는지를 묻는 값이다. 채널에 맞춰 바꾼 것(이스케이프·불릿·표 …)은 보지 않는다 — 그건 [`Repairs::changed`].
    pub fn any(&self) -> bool {
        self.closed_emphasis + self.closed_fence + self.reverted_code_span + self.dropped_marker > 0
    }

    /// 고친 것이든 채널에 맞춰 바꾼 것이든 **하나라도 했는가**. 출력이 원문과 달라질 수 있는지를 묻는다
    /// (빈 줄 접기 같은 모양 고르기는 세지 않는다 — `SPEC.md` 5.1).
    pub fn changed(&self) -> bool {
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
    /// 마지막 [`Streamer::preview`] 의 꼬리. 재사용 버퍼 — 열린 블록만큼이지 문서 전체가 아니다.
    tail: String,
    /// `tail` 이 지금 상태를 반영하지 않는다 — 미리보기를 안 했거나 그 뒤에 조각이 더 왔다.
    dirty: bool,
    /// [`Streamer::revised`] 의 답. `finish` 가 정한다.
    revised: bool,
}

impl Streamer {
    pub fn new(channel: Channel) -> Self {
        Self::with_options(channel, Options::default())
    }

    /// 옵션을 주고 만든다 — 입력 표기 따위.
    pub fn with_options(channel: Channel, options: Options) -> Self {
        Self {
            engine: Engine::new(channel, &options),
            buf: String::new(),
            started: false,
            tail: String::new(),
            dirty: true,
            revised: true,
        }
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
        self.dirty |= !chunk.is_empty();
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
        self.revised = self.dirty || self.tail != out[from..];
        // 끝난 엔진에 더 그릴 꼬리는 없다. 비워 두지 않으면 뒤이은 `preview` 가 끝난 엔진을
        // 복제해 finish 꼬리를 한 번 더 낸다.
        self.tail.clear();
        self.dirty = false;
    }

    /// **지금 입력이 끝났다면 확정분 뒤에 붙을 꼬리.** 누적본에 이걸 붙이면 그 자리에서 보낼 수
    /// 있는 모양이다 — [`Streamer::close_open`] 과 같은 자리에 들어가지만, 붙들고 있던 것까지
    /// 그린다: 열린 강조는 닫아서(`**굵` → `<b>굵</b>`), 표는 지금까지 온 행으로, 코드 스팬은
    /// 닫아서. 누적본을 통째로 다시 그리는 쪽(React, 텔레그램 `editMessageText`, 슬랙
    /// `chat.update`)의 기본값이다.
    ///
    /// 꼬리는 일괄 렌더와 같은 `finish` 경로라 문법은 늘 맞는다. 다만 **추측**이다 — 끝내 안
    /// 닫힌 코드 스팬이 글자로 되돌아가는 것처럼 뒤의 조각이 모양을 바꿀 수 있다. 끝난 뒤에
    /// 마지막 미리보기와 달라졌는지는 [`Streamer::revised`] 가 알려 준다. 누적본 자체에는
    /// 넣지 않는다.
    ///
    /// 비용은 지금 열린 블록 크기에 비례한다(엔진을 복제한다). 조각마다 부르지 말고 화면을
    /// 그릴 때 부른다.
    ///
    /// ```
    /// use mdwire::{Channel, Streamer};
    ///
    /// let mut s = Streamer::new(Channel::TelegramHtml);
    /// let mut acc = String::new();
    /// s.push_into("앞말 **굵", &mut acc);
    /// assert_eq!(acc, "앞말 ");                       // 확정분은 여기까지
    /// assert_eq!(format!("{acc}{}", s.preview()), "앞말 <b>굵</b>");
    ///
    /// s.push_into("게** 끝", &mut acc);
    /// let last = format!("{acc}{}", s.preview());
    /// s.finish_into(&mut acc);
    /// assert_eq!(acc, last);
    /// assert!(!s.revised());                          // 마지막 화면이 곧 완성본
    /// ```
    pub fn preview(&mut self) -> &str {
        // 그 뒤로 조각이 안 왔으면 같은 답이다 — 다시 그리는 쪽은 조각과 무관하게도 자주 부른다.
        if !self.dirty {
            return &self.tail;
        }
        let mut tail = std::mem::take(&mut self.tail);
        tail.clear();
        self.engine.preview(&mut StringSink(&mut tail));
        if !self.started {
            let keep = tail.len() - tail.trim_start_matches('\n').len();
            tail.drain(..keep);
        }
        self.tail = tail;
        self.dirty = false;
        &self.tail
    }

    /// [`Streamer::preview`] 를 호출자 버퍼에 덧붙인다.
    pub fn preview_into(&mut self, out: &mut String) {
        out.push_str(self.preview());
    }

    /// **완성본이 마지막 미리보기와 다른가** — `finish` 뒤에 본다. 거짓이면 마지막으로 그린
    /// 화면(`누적본 + preview`)이 곧 완성본이라 다시 그릴 필요가 없다. 텔레그램은 같은 내용으로
    /// 편집하면 400("message is not modified")을 주므로 이걸 보고 마지막 편집을 건너뛴다.
    /// 미리보기를 안 했거나 그 뒤에 조각이 더 왔으면 참이다 — **참은 "다를 수 있다"** 는 뜻이다.
    /// 편집을 솎아 보내 마지막 미리보기가 마지막 조각보다 앞서면 완성본이 같아도 참이니, 그런
    /// 쪽은 마지막으로 보낸 문자열과 직접 비교한다.
    pub fn revised(&self) -> bool {
        self.revised
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
    let mut engine = Engine::new(channel, &options);
    let mut sink = PartsSink::new(Vocab::from_options(channel, &options));
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
