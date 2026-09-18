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
    /// Telegram `parse_mode=MarkdownV2`. 이스케이프 대상 18자. 4096자.
    TelegramMarkdownV2,
    /// Slack `markdown_text`. 표준 마크다운을 슬랙이 직접 변환한다. 12,000자.
    /// 변환이 거의 필요 없고, 남는 일은 정규화와 분할뿐이다.
    SlackMarkdown,
    /// Slack 레거시 `mrkdwn`. 헤딩·표 없음. `*굵게*` `_기울임_` `<url|text>`.
    SlackMrkdwn,
    /// 모든 마크업 제거. 폴백 경로.
    Plain,
}

impl Channel {
    /// 코퍼스 디렉토리와 CLI 인자에서 쓰는 이름. 채널을 문자열로 다루는 곳의 정본이다.
    pub fn name(self) -> &'static str {
        match self {
            Channel::TelegramHtml => "telegram-html",
            Channel::TelegramMarkdownV2 => "telegram-markdown-v2",
            Channel::SlackMarkdown => "slack-markdown",
            Channel::SlackMrkdwn => "slack-mrkdwn",
            Channel::Plain => "plain",
        }
    }

    /// v0.1 이 실제로 내보낼 수 있는 채널. 나머지는 `SPEC.md` 11절에서 미뤄 뒀다.
    pub fn v0_1() -> [Channel; 3] {
        [Channel::TelegramHtml, Channel::SlackMarkdown, Channel::Plain]
    }

    /// 이름으로 채널을 찾는다.
    pub fn parse(name: &str) -> Option<Channel> {
        [
            Channel::TelegramHtml,
            Channel::TelegramMarkdownV2,
            Channel::SlackMarkdown,
            Channel::SlackMrkdwn,
            Channel::Plain,
        ]
        .into_iter()
        .find(|c| c.name() == name)
    }

    /// 이 채널의 메시지 길이 한도(문자 수). 분할의 기준이다.
    pub fn limit(self) -> usize {
        match self {
            Channel::TelegramHtml | Channel::TelegramMarkdownV2 => 4096,
            Channel::SlackMarkdown => 12_000,
            Channel::SlackMrkdwn | Channel::Plain => 12_000,
        }
    }
}

/// CJK 인접 강조 정책.
///
/// 한글·한자·가나 옆에 붙은 `**` 를 채널 파서가 강조로 못 잡는 경우가 있다.
/// 폭 없는 공백(U+200B)을 끼워 넣으면 살아나지만, 채널마다 필요 여부가 다르다.
/// **이 정책이 채널별로 제각각인 것이 기존 변환기들의 공통 결함이다.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CjkPolicy {
    /// 채널별로 알려진 기본값을 쓴다.
    Auto,
    /// 항상 U+200B 를 끼운다.
    AlwaysPad,
    /// 끼우지 않는다.
    Never,
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
/// use mdwire::{Channel, CjkPolicy, Streamer};
///
/// let mut s = Streamer::new(Channel::TelegramHtml, CjkPolicy::Auto);
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
}

impl Streamer {
    pub fn new(channel: Channel, cjk: CjkPolicy) -> Self {
        Self { engine: Engine::new(channel, cjk), buf: String::new() }
    }

    /// 조각을 밀어 넣고, 지금 내보낼 수 있는 출력을 받는다.
    ///
    /// 돌려주는 슬라이스는 **다음 호출 전까지만** 유효하다. 할당을 아예 없애려면
    /// [`Streamer::push_into`] 를 쓴다 — 둘은 같은 코드를 부른다(`SPEC.md` 5절).
    pub fn push(&mut self, chunk: &str) -> &str {
        self.buf.clear();
        let mut sink = StringSink(&mut self.buf);
        self.engine.feed(chunk, &mut sink);
        &self.buf
    }

    /// 호출자 버퍼에 직접 쓴다. 정본 서명 — 조각당 할당이 0 이다.
    pub fn push_into(&mut self, chunk: &str, out: &mut String) {
        let mut sink = StringSink(out);
        self.engine.feed(chunk, &mut sink);
    }

    /// 입력이 끝났다. 남은 것을 전부 내보낸다(열린 마크업은 닫는다).
    pub fn finish(&mut self) -> &str {
        self.buf.clear();
        let mut sink = StringSink(&mut self.buf);
        self.engine.finish(&mut sink);
        &self.buf
    }

    /// [`Streamer::finish`] 의 무할당 판.
    pub fn finish_into(&mut self, out: &mut String) {
        let mut sink = StringSink(out);
        self.engine.finish(&mut sink);
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
/// use mdwire::{render, Channel, CjkPolicy};
///
/// let parts = render("## 제목\n\n**굵게** 있는 문단", Channel::TelegramHtml, CjkPolicy::Auto);
/// assert_eq!(parts, vec!["<b>제목</b>\n\n<b>굵게</b> 있는 문단"]);
/// ```
pub fn render(input: &str, channel: Channel, cjk: CjkPolicy) -> Vec<String> {
    let mut engine = Engine::new(channel, cjk);
    let mut sink = PartsSink::new(Vocab::new(channel, cjk));
    engine.feed(input, &mut sink);
    engine.finish(&mut sink);
    sink.into_parts()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_limits_are_channel_specific() {
        assert_eq!(Channel::TelegramHtml.limit(), 4096);
        assert_eq!(Channel::SlackMarkdown.limit(), 12_000);
    }
}
