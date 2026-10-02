//! mdwire — safely deliver agent-generated Markdown to chat channels.
//!
//! It does three things in one pipeline. The order is the design.
//!
//! 1. **Normalize** — LLM output is not valid CommonMark. Unbalanced emphasis,
//!    emphasis that spans lines, and unclosed code fences are routine. Repair them first.
//! 2. **Render for the channel** — translate into the syntax the target accepts. Syntax the
//!    target cannot accept is not worth parsing in the first place (see the `Channel` docs below).
//! 3. **Split safely** — at channel limits and streaming boundaries, never cut through markup.
//!
//! # Why no dependencies
//!
//! Existing parsers are all **batch** parsers — they take the whole document, build an AST,
//! then render. That does not fit a problem where output must go out as tokens stream in.
//! And the CJK-adjacent emphasis policy is baked into the parser, so with someone else's
//! parser you cannot change it. That is exactly the problem this library sets out to fix.

#![forbid(unsafe_code)]

mod block;
mod inline;
mod sink;
mod vocab;
pub mod width;

use block::Engine;
use sink::{PartsSink, StringSink};
use vocab::Vocab;

/// Target channel. Each channel accepts a different syntax, and **the narrower output decides the
/// parsing scope**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    /// Telegram `parse_mode=HTML`. Nine allowed tags:
    /// `b i u s code pre a blockquote tg-spoiler`. No tables or headings. 4096 characters.
    TelegramHtml,
    /// Slack `markdown_text`. Slack converts standard Markdown itself. 12,000 characters.
    /// Almost no conversion is needed; what is left is normalization and splitting.
    SlackMarkdown,
    /// GitHub comments and PR bodies (GFM). Renders tables, headings, and strikethrough.
    /// 65,536 characters. Emits the same Markdown as Slack, but escapes the two characters GFM
    /// reads as syntax (`~` `<`).
    GithubMarkdown,
    /// Notion page body (Notion-flavored Markdown — the API `markdown` field and connectors).
    /// Four heading levels. Emits the same Markdown as GitHub, but strips inline HTML Notion
    /// cannot render (it would show as text), and writes autolinks `<url>` as `[url](url)` (the
    /// angle brackets would remain as text). Emphasis before a Korean particle is rendered by
    /// Notion as is, so it is not turned into `<strong>` — doing so would make the tag show as text.
    NotionMarkdown,
    /// Strips all markup. The fallback path.
    Plain,
    /// An HTML fragment for the browser. Renders headings, lists, tables, and code blocks as tags.
    /// No limit.
    ///
    /// Assumes the output goes straight into `innerHTML` — all text is escaped, HTML from the
    /// source survives only as inline tags with their attributes dropped, and only `http(s)` and
    /// `mailto` links become `<a>`. Appending [`Streamer::close_open`] to the streaming
    /// accumulated output gives a shape that can be inserted as is.
    Html,
}

impl Channel {
    /// The name used for corpus directories and CLI arguments. The source of truth wherever a
    /// channel is handled as a string.
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

    /// Every supported channel. The corpus and the harness iterate over this list.
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

    /// Looks up a channel by name.
    pub fn parse(name: &str) -> Option<Channel> {
        Self::all().into_iter().find(|c| c.name() == name)
    }

    /// This channel's message length limit, in characters. Splitting is based on it.
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

/// The smallest part limit a caller can set.
///
/// **Each part needs room to close and reopen its markup.** If the limit is smaller than a tag,
/// the splitter cuts between the tag's characters — splitting Telegram `**x**` with a limit of 1
/// produced `<` · `b` · `></b>` (found in review). This value fits several nested opening tags
/// (quote, bold, code, `<pre><code class="language-…">`) and still leaves room for content. It is
/// far from real use (subtracting a header's share from Telegram's 4096).
pub const MIN_LIMIT: usize = 256;

/// Conversion options — the part limit and the browser channel's policy.
///
/// **There is no input-syntax choice.** Accepting LLM output even when it strays from the standard
/// is the job of the default reader.
///
/// Fields may be added, so build it as `Options { limit, ..Default::default() }`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    /// The limit for one part (characters of rendered output). `None` means [`Channel::limit`].
    ///
    /// **The sender decides the limit.** Sometimes the channel cannot — plain is a fallback with
    /// no known destination, so sending it to Telegram needs 4096 (a 7,153-character part split at
    /// 12,000 got a 400), and a sender that prepends a title must use that much less.
    /// Streaming ([`Streamer`]) does not split, so it ignores this value — except for Notion
    /// tables. A table over the limit comes out as several tables that repeat the header row; that
    /// is the table's shape, so streaming emits it the same way. The browser channel
    /// ([`Channel::Html`]) does not split either — the splitter does not close and reopen block
    /// tags, so it would cut through the middle of a tag. Values below [`MIN_LIMIT`] are raised
    /// to it.
    pub limit: Option<usize>,
    /// The browser channel's ([`Channel::Html`]) policy. Other channels ignore it.
    pub html: HtmlOptions,
}

/// The browser channel's policy. The defaults are the most conservative — `<br>` line breaks,
/// images as links only, and only `http`, `https`, and `mailto` links.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HtmlOptions {
    pub line_breaks: LineBreaks,
    pub images: Images,
    /// Schemes accepted for link and image URLs (without the colon, like `"https"`). `None`
    /// means `http`, `https`, and `mailto`. A list accepts **only** those — it does not add to the
    /// defaults.
    pub schemes: Option<Vec<String>>,
}

/// How to emit line breaks inside a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineBreaks {
    /// `<br>` — for text where the author's line breaks carry meaning, like chat or notes. Every
    /// other channel keeps line breaks.
    #[default]
    Br,
    /// The newline character only — the browser folds it into a space. For reading a document
    /// wrapped at 80 columns as paragraphs.
    Space,
}

/// How to emit an image `![alt](url)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Images {
    /// `<a href>alt</a>` — nothing loads until it is clicked (no tracking pixels).
    #[default]
    Link,
    /// `<img src alt>` — only when the URL has an allowed scheme. Otherwise emitted like `Link`.
    Load,
}

/// Counts of what normalization repaired and what was changed to fit the channel. The first five
/// (repairs) measure **how often the model breaks formatting**; the last six (changes) measure
/// **what a channel changes, before adopting it** — to ask about each separately, use
/// [`Repairs::any`] and [`Repairs::changed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Repairs {
    /// Emphasis left unclosed at the end of a block and closed for it (like `**영향 범위`).
    pub closed_emphasis: usize,
    /// Code fences left unclosed at the end of the document and closed for it.
    pub closed_fence: usize,
    /// Unmatched backtick runs turned back into text instead of code.
    pub reverted_code_span: usize,
    /// Orphaned `**` dropped (preceded by text, like `꼬리**`).
    pub dropped_marker: usize,
    /// Emphasis paired by a guess rather than by the rules — an opener CommonMark would not open
    /// (`값**(합계)**를`, a letter before it and punctuation after) matched with a mirror-shaped closer
    /// on the same line. Math is left out — ASCII letters or digits on both ends (`x**(y)**z`) never
    /// pair — so what remains is the rare shape that is bold or math only by context. A non-zero
    /// count marks an answer worth a look.
    pub guessed_pair: usize,
    /// Characters escaped because the channel would read them as syntax (GitHub's `\~`, `\<`,
    /// `\*`; Slack's and Notion's `\*`).
    pub escaped_char: usize,
    /// Emphasis emitted differently because the channel cannot read the markers in that position
    /// (`**「설정」**가`) — `<strong>` on GitHub, U+2060 inserted inside the markers on Slack.
    pub tag_emphasis: usize,
    /// Source HTML stripped — tags the channel cannot render, comments, and `<br>` turned into
    /// line breaks.
    pub stripped_html: usize,
    /// List markers rewritten with a different symbol — bullets (`* `, `• ` → `- `; on Telegram
    /// `- ` → `• `) and numbers (`1)` → `1.`).
    pub rewritten_bullet: usize,
    /// Tables rewritten into a different shape from the source (delimiter row and cell padding
    /// normalized, or lowered to fixed width).
    pub rewritten_table: usize,
    /// Emphasis markers and links rewritten in a different notation — `_기울임_` → `*기울임*`,
    /// `__굵게__` → `**굵게**`, `<url|텍스트>` → `[텍스트](url)`. Counted only on channels that
    /// emit Markdown.
    pub converted_marker: usize,
}

impl Repairs {
    pub(crate) fn add(&mut self, other: Repairs) {
        self.closed_emphasis += other.closed_emphasis;
        self.closed_fence += other.closed_fence;
        self.reverted_code_span += other.reverted_code_span;
        self.dropped_marker += other.dropped_marker;
        self.guessed_pair += other.guessed_pair;
        self.escaped_char += other.escaped_char;
        self.tag_emphasis += other.tag_emphasis;
        self.stripped_html += other.stripped_html;
        self.rewritten_bullet += other.rewritten_bullet;
        self.rewritten_table += other.rewritten_table;
        self.converted_marker += other.converted_marker;
    }

    /// Whether normalization **repaired** anything — the first five (closed emphasis and fences,
    /// reverted backticks, dropped markers, guessed pairs). It asks whether the model broke formatting. Changes
    /// made to fit the channel (escapes, bullets, tables …) are not counted — that is
    /// [`Repairs::changed`].
    pub fn any(&self) -> bool {
        self.closed_emphasis + self.closed_fence + self.reverted_code_span + self.dropped_marker + self.guessed_pair > 0
    }

    /// Whether **anything at all** was done, repair or channel change. It asks whether the output
    /// may differ from the source (tidying such as collapsing blank lines is not counted —
    /// `SPEC.md` 5.1).
    pub fn changed(&self) -> bool {
        *self != Repairs::default()
    }
}

/// The result of [`render_with`] — the parts and the repairs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub parts: Vec<String>,
    pub repairs: Repairs,
}

/// Streaming converter.
///
/// Feed it chunks and it returns **only as much as is safe to emit right now**.
/// Markup caught on a boundary (cut off at `**굵`) stays inside until the next chunk arrives.
/// This is the core of the library — converting a finished document is a problem others have
/// already solved, while the boundary problem shows up on every channel as long as you stream.
///
/// # Example
///
/// ```
/// use mdwire::{Channel, Streamer};
///
/// let mut s = Streamer::new(Channel::TelegramHtml);
/// let mut out = String::new();
/// // A chunk boundary inside `**` never sends half a marker.
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

    /// Builds one with options.
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

    /// What normalization has repaired so far. After `finish`, it covers the whole document.
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

    /// Pushes a chunk and returns the output that can be emitted now.
    ///
    /// The returned slice is valid **only until the next call**. To avoid allocation entirely,
    /// use [`Streamer::push_into`] — both call the same code (`SPEC.md` section 5).
    pub fn push(&mut self, chunk: &str) -> &str {
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        self.push_into(chunk, &mut buf);
        self.buf = buf;
        &self.buf
    }

    /// Writes directly into the caller's buffer. The canonical signature — zero allocations per
    /// chunk.
    pub fn push_into(&mut self, chunk: &str, out: &mut String) {
        self.dirty |= !chunk.is_empty();
        let from = out.len();
        let mut sink = StringSink(out);
        self.engine.feed(chunk, &mut sink);
        self.trim_leading(out, from);
    }

    /// Signals the end of input. Emits everything left (open markup is closed).
    pub fn finish(&mut self) -> &str {
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        self.finish_into(&mut buf);
        self.buf = buf;
        &self.buf
    }

    /// The allocation-free version of [`Streamer::finish`].
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

    /// **The tail that would follow the final output if input ended now.** Appending it to the
    /// accumulated output gives a shape that can be sent on the spot — it goes where
    /// [`Streamer::close_open`] goes, but also renders what is being held back: open emphasis
    /// closed (`**굵` → `<b>굵</b>`), tables with the rows received so far, code spans closed.
    /// It is the default for callers that redraw the whole accumulated output (React, Telegram
    /// `editMessageText`, Slack `chat.update`).
    ///
    /// The tail takes the same `finish` path as batch rendering, so its syntax is always valid. But
    /// it is a **guess** — later chunks can change the shape, for example a code span that never
    /// closes turns back into text. After finishing, [`Streamer::revised`] tells you whether the
    /// result differs from the last preview. Do not put it into the accumulated output itself.
    ///
    /// The cost is proportional to the size of the currently open block (the engine is cloned).
    /// Call it when drawing the screen, not on every chunk.
    ///
    /// ```
    /// use mdwire::{Channel, Streamer};
    ///
    /// let mut s = Streamer::new(Channel::TelegramHtml);
    /// let mut acc = String::new();
    /// s.push_into("앞말 **굵", &mut acc);
    /// assert_eq!(acc, "앞말 ");                       // final output so far
    /// assert_eq!(format!("{acc}{}", s.preview()), "앞말 <b>굵</b>");
    ///
    /// s.push_into("게** 끝", &mut acc);
    /// let last = format!("{acc}{}", s.preview());
    /// s.finish_into(&mut acc);
    /// assert_eq!(acc, last);
    /// assert!(!s.revised());                          // the last frame is already the result
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

    /// Appends [`Streamer::preview`] to the caller's buffer.
    pub fn preview_into(&mut self, out: &mut String) {
        out.push_str(self.preview());
    }

    /// **Whether the final output differs from the last preview** — check it after `finish`. If
    /// false, the last screen drawn (accumulated output + `preview`) already is the final output, so no
    /// redraw is needed. Telegram returns 400 ("message is not modified") for an edit with the same
    /// content, so use this to skip the last edit. It is true if there was no preview or more
    /// chunks arrived after it — **true means "may differ"**. If you throttle edits so the last
    /// preview comes before the last chunk, it is true even when the final output is the same;
    /// in that case compare against the last string you sent.
    pub fn revised(&self) -> bool {
        self.revised
    }

    /// **Makes what has been received so far safe to send as is.** It does not touch the state,
    /// so streaming continues after appending it.
    ///
    /// Emphasis is held inside until its pair arrives, so it is already balanced, but a block's
    /// opening markup (`<blockquote>`, `<pre>`, a heading's `<b>`) goes out before the block
    /// ends — holding output until a code block ends would not be streaming. Callers that send
    /// the accumulated output to a channel midway (editing a message as tokens arrive) append this
    /// right before sending. **Do not put it into the accumulated output itself** — the next chunk
    /// continues from there.
    ///
    /// ```
    /// use mdwire::{Channel, Streamer};
    ///
    /// let mut s = Streamer::new(Channel::TelegramHtml);
    /// let mut acc = String::new();
    /// s.push_into("> 인용이 시작되고", &mut acc);
    ///
    /// let mut snapshot = acc.clone();
    /// s.close_open(&mut snapshot);          // safe to send now
    /// assert_eq!(snapshot, "<blockquote>인용이 시작되고</blockquote>");
    ///
    /// s.push_into("\n> 이어진다\n", &mut acc);  // the accumulated output just continues
    /// s.finish_into(&mut acc);
    /// assert_eq!(acc, "<blockquote>인용이 시작되고\n이어진다</blockquote>");
    /// ```
    pub fn close_open(&self, out: &mut String) {
        self.engine.close_open(out);
    }
}

/// Converts a finished document in one go. Splits at safe points when it exceeds the limit.
///
/// Split points are chosen **from the structure, not the rendered output** — only points where a
/// block has ended and no markup is open become boundaries. Cutting by character count after
/// conversion leaves `<code>` open across the cut, and the channel returns 400 (`DESIGN.md`).
///
/// # Example
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

/// [`render`] with options; also returns what normalization repaired.
///
/// ```
/// use mdwire::{render_with, Channel, Options};
///
/// let options = Options { limit: Some(4096), ..Default::default() };
/// let out = render_with("**굵게** 는 **영향 범위", Channel::SlackMarkdown, options);
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
