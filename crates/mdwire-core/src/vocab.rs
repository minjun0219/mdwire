//! 채널별 출력 어휘.
//!
//! 파서는 채널을 모른다. **무엇을 어떻게 적을지만 여기서 갈린다.** 채널을 늘릴 때
//! 손대는 곳이 이 파일이어야 한다는 뜻이고, 그게 `SPEC.md` 4절과 8절의 표가 코드에
//! 대응하는 방식이다.

use crate::width::is_cjk;
use crate::{Channel, CjkPolicy};

/// 인라인 강조의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emph {
    Bold,
    Italic,
    Strike,
    Code,
}

/// 채널 하나의 출력 어휘와 정책.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Vocab {
    pub channel: Channel,
    /// CJK 인접 강조에 폭 없는 공백(U+200B)을 끼울 것인가.
    pub pad: bool,
}

impl Vocab {
    pub fn new(channel: Channel, cjk: CjkPolicy) -> Self {
        let pad = match cjk {
            CjkPolicy::AlwaysPad => true,
            CjkPolicy::Never => false,
            // 채널별 기본값. 마크다운을 채널 파서가 다시 읽는 곳만 끼운다 —
            // 태그로 나가는 텔레그램 HTML 은 한글 옆이어도 파서가 헷갈릴 일이 없다.
            CjkPolicy::Auto => matches!(channel, Channel::SlackMarkdown | Channel::SlackMrkdwn),
        };
        Self { channel, pad }
    }

    /// 마크업 문법 자체가 없는 채널인가. 강조도 표도 글자로 내려앉는다.
    pub fn is_plain(&self) -> bool {
        self.channel == Channel::Plain
    }

    pub fn open(&self, e: Emph) -> &'static str {
        match (self.channel, e) {
            (Channel::TelegramHtml, Emph::Bold) => "<b>",
            (Channel::TelegramHtml, Emph::Italic) => "<i>",
            (Channel::TelegramHtml, Emph::Strike) => "<s>",
            (Channel::TelegramHtml, Emph::Code) => "<code>",
            (Channel::Plain, _) => "",
            // 취소선 표기는 채널마다 갈린다. 슬랙 `markdown_text` 는 표준 마크다운이라
            // `~~` 고, 레거시 `mrkdwn` 과 텔레그램 MarkdownV2 는 `~` 하나다.
            // 하나로 뭉뚱그리면 한쪽은 취소선이 안 걸리고 물결표만 보인다.
            (Channel::SlackMrkdwn | Channel::TelegramMarkdownV2, Emph::Strike) => "~",
            (_, Emph::Strike) => "~~",
            (_, Emph::Bold) => "**",
            (_, Emph::Italic) => "_",
            (_, Emph::Code) => "`",
        }
    }

    pub fn close(&self, e: Emph) -> &'static str {
        match (self.channel, e) {
            (Channel::TelegramHtml, Emph::Bold) => "</b>",
            (Channel::TelegramHtml, Emph::Italic) => "</i>",
            (Channel::TelegramHtml, Emph::Strike) => "</s>",
            (Channel::TelegramHtml, Emph::Code) => "</code>",
            _ => self.open(e),
        }
    }

    /// 글자 하나를 본문으로 적는다.
    pub fn escape_char(&self, c: char, out: &mut String) {
        match (self.channel, c) {
            (Channel::TelegramHtml, '&') => out.push_str("&amp;"),
            (Channel::TelegramHtml, '<') => out.push_str("&lt;"),
            (Channel::TelegramHtml, '>') => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }

    pub fn escape(&self, s: &str, out: &mut String) {
        if self.channel != Channel::TelegramHtml {
            out.push_str(s);
            return;
        }
        // 대부분의 줄에는 이스케이프할 글자가 없다. 있을 때만 한 글자씩 간다.
        if !s.contains(['&', '<', '>']) {
            out.push_str(s);
            return;
        }
        for c in s.chars() {
            self.escape_char(c, out);
        }
    }

    /// 링크를 적는다. 텍스트는 이미 렌더된 것을 받는다.
    pub fn link(&self, text: &str, url: &str, out: &mut String) {
        match self.channel {
            Channel::TelegramHtml => {
                // **한도를 넘는 주소는 링크로 내지 않는다.** 여는 태그 하나가 메시지를
                // 다 차지하면 조각을 아무리 나눠도 내용이 한 글자도 안 들어간다.
                // 주소는 괄호에 넣어 글로 내보낸다 — 링크는 죽어도 내용은 산다.
                let markup = escaped_len(url) + "<a href=\"\"></a>".len();
                if markup >= self.channel.limit() {
                    out.push_str(text);
                    if !url.is_empty() && !escaped_eq(text, url) {
                        out.push_str(" (");
                        self.escape(url, out);
                        out.push(')');
                    }
                    return;
                }
                out.push_str("<a href=\"");
                for c in url.chars() {
                    match c {
                        '&' => out.push_str("&amp;"),
                        '<' => out.push_str("&lt;"),
                        '>' => out.push_str("&gt;"),
                        '"' => out.push_str("&quot;"),
                        _ => out.push(c),
                    }
                }
                out.push_str("\">");
                out.push_str(text);
                out.push_str("</a>");
            }
            Channel::Plain => {
                out.push_str(text);
                if !url.is_empty() && url != text {
                    out.push_str(" (");
                    out.push_str(url);
                    out.push(')');
                }
            }
            _ => {
                out.push('[');
                out.push_str(text);
                out.push_str("](");
                out.push_str(url);
                out.push(')');
            }
        }
    }

    /// 역슬래시로 탈출된 글자를 내보낸다.
    ///
    /// **마크다운을 그대로 내보내는 채널에서는 탈출을 지키고 나간다.** 벗겨서 맨몸
    /// `*` 를 내보내면 저자가 글자로 쓴 별표가 그 채널에서 강조로 읽힌다 — 벗기는 것이
    /// 오히려 뜻을 바꾼다. HTML 로 가는 채널은 마커라는 개념이 없으니 그냥 escape 한다.
    pub fn literal(&self, c: char, out: &mut String) {
        match self.channel {
            Channel::TelegramHtml | Channel::Plain => self.escape_char(c, out),
            _ if matches!(c, '*' | '_' | '~' | '`' | '\\') => {
                out.push('\\');
                out.push(c);
            }
            _ => self.escape_char(c, out),
        }
    }

    /// 불릿 마커. `SPEC.md` 8절의 표.
    pub fn bullet(&self) -> &'static str {
        match self.channel {
            Channel::SlackMarkdown | Channel::SlackMrkdwn | Channel::TelegramMarkdownV2 => "- ",
            _ => "• ",
        }
    }

    pub fn quote_prefix(&self) -> &'static str {
        match self.channel {
            Channel::TelegramHtml => "",
            _ => "> ",
        }
    }

    pub fn quote_open(&self) -> &'static str {
        match self.channel {
            Channel::TelegramHtml => "<blockquote>",
            _ => "",
        }
    }

    pub fn quote_close(&self) -> &'static str {
        match self.channel {
            Channel::TelegramHtml => "</blockquote>",
            _ => "",
        }
    }

    /// 구분선. 텔레그램에도 Plain 에도 구문이 없어 글자로 그린다.
    pub fn rule(&self) -> &'static str {
        match self.channel {
            Channel::TelegramHtml | Channel::Plain => "──────────",
            _ => "---",
        }
    }

    /// 헤딩이 쓸 수 있는 가장 깊은 레벨. 구문이 없는 채널은 0.
    pub fn max_heading(&self) -> usize {
        match self.channel {
            // 슬랙 문서가 "모든 헤딩 레벨을 같은 크기로 그린다"고 적고 있다.
            // 그러니 더 깊이 적을 값이 없다 — 셋에서 끊는다.
            Channel::SlackMarkdown => 3,
            Channel::TelegramHtml | Channel::TelegramMarkdownV2 | Channel::SlackMrkdwn
            | Channel::Plain => 0,
        }
    }

    /// 고정폭 블록을 여닫는다. 표와 코드펜스가 같이 쓴다.
    pub fn verbatim_open(&self, info: &str, out: &mut String) {
        match self.channel {
            Channel::TelegramHtml => {
                out.push_str("<pre>");
                if !info.is_empty() {
                    out.push_str("<code class=\"language-");
                    self.escape(info, out);
                    out.push_str("\">");
                }
            }
            Channel::Plain => {}
            _ => {
                out.push_str("```");
                out.push_str(info);
            }
        }
    }

    /// 여는 마크업과 첫 내용 줄 사이에 줄바꿈이 필요한가.
    /// ` ``` ` 는 필요하고, `<pre>` 는 넣으면 빈 줄이 하나 생긴다.
    pub fn verbatim_body_newline(&self) -> bool {
        !matches!(self.channel, Channel::TelegramHtml | Channel::Plain)
    }

    pub fn verbatim_close(&self, info: &str, out: &mut String) {
        match self.channel {
            Channel::TelegramHtml => {
                if !info.is_empty() {
                    out.push_str("</code>");
                }
                out.push_str("</pre>");
            }
            Channel::Plain => {}
            _ => out.push_str("\n```"),
        }
    }
}

/// 이 글자 옆에 폭 없는 공백을 끼워야 하는가.
///
/// 이름이 정책을 가리킨다 — 판정 자체는 `width::is_cjk` 가 하고, 여기서는 그것이
/// **패딩의 기준**이라는 사실만 말한다. 전에는 `is_wide`(표시 폭 2)였는데 그건
/// 다른 질문이었다: 반각 가타카나는 폭 1이지만 끼워야 하고 이모지는 폭 2지만 아니다.
pub(crate) fn needs_cjk_padding(c: char) -> bool {
    is_cjk(c)
}

/// 폭 없는 공백. CJK 인접 강조를 살린다.
pub(crate) const ZWSP: char = '\u{200b}';

/// escape 하고 나면 몇 글자가 되는가. **재기만 하고 만들지는 않는다** — 스트리밍
/// 경로에서 링크마다 문자열을 하나씩 더 만들 수는 없다.
fn escaped_len(url: &str) -> usize {
    url.chars()
        .map(|c| match c {
            '&' => 5,
            '<' | '>' => 4,
            '"' => 6,
            _ => 1,
        })
        .sum()
}

/// escape 한 주소가 이 텍스트와 같은가. **만들지 않고 견준다.**
///
/// 맨몸 링크는 라벨이 곧 주소인데, `text` 는 이미 escape 되어 있고 `url` 은 날것이다.
/// 그냥 견주면 `&` 하나 때문에 다르다고 보고 주소를 두 번 내보낸다.
fn escaped_eq(text: &str, url: &str) -> bool {
    let mut t = text.chars();
    for c in url.chars() {
        let escaped = match c {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            _ => {
                if t.next() != Some(c) {
                    return false;
                }
                continue;
            }
        };
        for e in escaped.chars() {
            if t.next() != Some(e) {
                return false;
            }
        }
    }
    t.next().is_none()
}
