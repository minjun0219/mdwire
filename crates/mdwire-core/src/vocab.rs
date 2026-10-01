//! 채널별 출력 어휘.
//!
//! 파서는 채널을 모른다. **무엇을 어떻게 적을지만 여기서 갈린다.** 채널을 늘릴 때
//! 손대는 곳이 이 파일이어야 한다는 뜻이고, 그게 `SPEC.md` 4절과 8절의 표가 코드에
//! 대응하는 방식이다.

use crate::Channel;

/// 인라인 강조의 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Emph {
    Bold,
    Italic,
    Strike,
    Code,
    /// 원문에 적힌 인라인 HTML 태그 — [`INLINE_TAGS`] 의 번호. 태그를 그리는 채널(html ·
    /// GitHub)에서 강조와 같은 스택에 올려 짝과 중첩을 맞춘다.
    Tag(u8),
}

/// 살려 둘 수 있는 인라인 태그 — (이름, 여는 태그, 닫는 태그). 속성은 버리고 이 모양으로
/// 다시 쓴다. `<br>` 은 짝이 없어 따로 다룬다. GitHub 이 받는 것만 — `font` 는 새니타이저가 지운다.
pub(crate) const INLINE_TAGS: [(&str, &str, &str); 16] = [
    ("sub", "<sub>", "</sub>"),
    ("sup", "<sup>", "</sup>"),
    ("b", "<b>", "</b>"),
    ("strong", "<strong>", "</strong>"),
    ("i", "<i>", "</i>"),
    ("em", "<em>", "</em>"),
    ("u", "<u>", "</u>"),
    ("s", "<s>", "</s>"),
    ("strike", "<strike>", "</strike>"),
    ("del", "<del>", "</del>"),
    ("code", "<code>", "</code>"),
    ("span", "<span>", "</span>"),
    ("small", "<small>", "</small>"),
    ("mark", "<mark>", "</mark>"),
    ("kbd", "<kbd>", "</kbd>"),
    ("ins", "<ins>", "</ins>"),
];

/// 채널 하나의 출력 어휘와 정책.
#[derive(Debug, Clone)]
pub(crate) struct Vocab {
    pub channel: Channel,
    /// 한 조각의 한도. 채널 기본값이거나 호출자가 [`crate::Options::limit`] 로 준 값이다.
    pub limit: usize,
    /// 브라우저 채널의 정책. 스킴 목록은 공유한다 — 표 칸마다 어휘를 복제해도 목록은 한 벌.
    br: bool,
    load_images: bool,
    schemes: Option<std::rc::Rc<[String]>>,
}

impl Vocab {
    pub fn new(channel: Channel) -> Self {
        Self::from_options(channel, &crate::Options::default())
    }

    /// 옵션으로 만든다. 한도가 `None` 이면 채널 기본값, [`crate::MIN_LIMIT`] 아래는 올린다.
    pub fn from_options(channel: Channel, o: &crate::Options) -> Self {
        let limit = match o.limit {
            // 브라우저 채널은 나누지 않는다 — 분할기가 `<p>`·`<ul>` 을 여닫지 않는다.
            Some(n) if channel != Channel::Html => n.max(crate::MIN_LIMIT),
            _ => channel.limit(),
        };
        Self {
            channel,
            limit,
            br: o.html.line_breaks == crate::LineBreaks::Br,
            load_images: o.html.images == crate::Images::Load,
            schemes: o.html.schemes.as_ref().map(|s| s.iter().map(|x| x.to_ascii_lowercase()).collect()),
        }
    }

    /// 브라우저에서 눌러도(불러와도) 되는 주소인가 — 허용 스킴만. 대소문자·앞 공백으로 숨긴
    /// `JavaScript:` 도 스킴이 달라 걸러진다.
    fn allowed(&self, url: &str) -> bool {
        let u = url.trim_start();
        let Some(colon) = u.find(':') else { return false };
        let scheme = &u[..colon];
        match &self.schemes {
            None => ["http", "https", "mailto"].iter().any(|s| scheme.eq_ignore_ascii_case(s)),
            Some(list) => list.iter().any(|s| scheme.eq_ignore_ascii_case(s)),
        }
    }

    /// 채널이 표를 직접 그리는가. 그리면 고정폭으로 내리는 것이 손해다.
    ///
    /// 슬랙 `markdown_text` 는 표준 마크다운 표를 네이티브로 그린다(Slack markdown block
    /// 문서). 고정폭 코드블록으로 바꾸면 화면에서 표가 아니라 코드로 보인다. GitHub 은
    /// GFM 표가 원래 문법이다.
    pub fn tables_native(&self) -> bool {
        matches!(
            self.channel,
            Channel::SlackMarkdown | Channel::GithubMarkdown | Channel::NotionMarkdown | Channel::Html
        )
    }

    /// 표를 노션 `<table>` 로 내는가. 파이프 표로 내면 칸 안의 `|` 가 칸을 가른다(실측).
    pub fn xml_tables(&self) -> bool {
        self.channel == Channel::NotionMarkdown
    }

    /// 브라우저용 HTML 채널인가. 블록까지 태그로 그린다(`<p>` `<h2>` `<ul>` `<table>`).
    pub fn is_html(&self) -> bool {
        self.channel == Channel::Html
    }

    /// 출력이 HTML 이라 글자를 escape 해야 하는가. 텔레그램과 브라우저.
    pub(crate) fn html_out(&self) -> bool {
        matches!(self.channel, Channel::TelegramHtml | Channel::Html)
    }

    /// 블록 안의 줄바꿈. 브라우저는 `\n` 을 공백으로 접으므로 `<br>` 을 앞에 둔다 — 다른
    /// 채널이 다 줄바꿈을 살리니 같은 글이 같은 모양으로 보이게.
    pub fn line_break(&self) -> &'static str {
        if self.is_html() && self.br { "<br>\n" } else { "\n" }
    }

    /// 마크업 문법 자체가 없는 채널인가. 강조도 표도 글자로 내려앉는다.
    pub fn is_plain(&self) -> bool {
        self.channel == Channel::Plain
    }

    pub fn open(&self, e: Emph) -> &'static str {
        match (self.channel, e) {
            (_, Emph::Tag(t)) => INLINE_TAGS[t as usize].1,
            (Channel::TelegramHtml, Emph::Bold) => "<b>",
            (Channel::TelegramHtml, Emph::Italic) => "<i>",
            (Channel::TelegramHtml, Emph::Strike) => "<s>",
            (Channel::TelegramHtml, Emph::Code) => "<code>",
            (Channel::Plain, _) => "",
            (Channel::Html, _) => self.open_html(e),
            (_, Emph::Strike) => "~~",
            (_, Emph::Bold) => "**",
            // 기울임은 `_` 가 아니라 `*` 다. 슬랙 `markdown_text` 는 `_기울임_가` 를 글자
            // 그대로 두고 `*기울임*가` 는 기울인다(실측 2026-09-22, CommonMark 의 단어 안
            // `_` 규칙). 한글은 조사가 붙는 것이 기본이라 `_` 로 내면 기울임이 자주 죽는다.
            (_, Emph::Italic) => "*",
            (_, Emph::Code) => "`",
        }
    }

    /// 원문의 `<br>` 을 그대로 두는가(칸 안이든 밖이든). 노션만 — 한 블록 안의 줄바꿈으로 그린다.
    /// `\n` 으로 바꾸면 인용이 둘로 갈리고 강조가 줄을 넘는다(실측 2026-10-01).
    pub fn keeps_br(&self) -> bool {
        self.channel == Channel::NotionMarkdown
    }

    /// 강조를 줄마다 닫고 다시 여는가. 노션만 그렇다 — 줄을 넘는 마커의 짝을 못 맞춘다.
    pub fn line_emphasis(&self) -> bool {
        self.channel == Channel::NotionMarkdown
    }

    /// 마크다운 마커를 채널이 못 읽는 자리에서 태그로 낼 수 있는가. GitHub 만 그렇다 —
    /// 인라인 HTML 을 그리고, 마커와 달리 flanking 을 안 따진다.
    pub fn html_emphasis(&self) -> bool {
        self.channel == Channel::GithubMarkdown
    }

    pub fn open_html(&self, e: Emph) -> &'static str {
        match e {
            Emph::Bold => "<strong>",
            Emph::Italic => "<em>",
            Emph::Strike => "<del>",
            Emph::Code => "<code>",
            Emph::Tag(t) => INLINE_TAGS[t as usize].1,
        }
    }

    pub fn close_html(&self, e: Emph) -> &'static str {
        match e {
            Emph::Bold => "</strong>",
            Emph::Italic => "</em>",
            Emph::Strike => "</del>",
            Emph::Code => "</code>",
            Emph::Tag(t) => INLINE_TAGS[t as usize].2,
        }
    }

    pub fn close(&self, e: Emph) -> &'static str {
        match (self.channel, e) {
            (_, Emph::Tag(t)) => INLINE_TAGS[t as usize].2,
            (Channel::TelegramHtml, Emph::Bold) => "</b>",
            (Channel::TelegramHtml, Emph::Italic) => "</i>",
            (Channel::TelegramHtml, Emph::Strike) => "</s>",
            (Channel::TelegramHtml, Emph::Code) => "</code>",
            (Channel::Html, _) => self.close_html(e),
            _ => self.open(e),
        }
    }

    /// 글자 하나를 본문으로 적는다.
    ///
    /// **GitHub 에서는 `~` 와 `<` 를 이스케이프한다**(실측 2026-09-30, `POST /markdown` gfm).
    /// GFM 은 홑 `~` 도 취소선으로 읽어서 `약 ~40km, 5~6월` 의 `40km, 5` 가 그어지고,
    /// `Vec<T>` 의 `<T>` 는 HTML 태그로 읽혀 새니타이저가 지운다. 둘 다 저자가 글자로
    /// 쓴 것이고, `\~` · `\<` 는 화면에 `~` · `<` 로 보인다. 슬랙 `markdown_text` 는
    /// 둘 다 글자로 그려서 손대지 않는다.
    pub fn escape_char(&self, c: char, out: &mut String) {
        match c {
            '&' if self.html_out() => out.push_str("&amp;"),
            '<' if self.html_out() => out.push_str("&lt;"),
            '>' if self.html_out() => out.push_str("&gt;"),
            _ if self.escapes(c) => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }

    /// 본문에 글자로 적을 때 역슬래시를 앞에 붙이는 글자인가.
    ///
    /// `*` 도 GitHub 에서는 이스케이프한다 — mdwire 가 글자로 판정한 별표를 GFM 이 다시 읽는다.
    /// 마스킹 번호 `1***-****-****-001*` 의 `-****-` 가 `-<strong>-</strong>-` 로 먹혔다(실측
    /// 2026-09-30). 강조 마커는 `open`/`close` 로 따로 나가니 여기 오는 별표는 전부 글자다.
    ///
    /// **노션은 `*` 와 `\` 를 이스케이프한다**(실측 2026-10-01, 커넥터). 마스킹 번호가 GitHub 처럼
    /// 뭉개지고(`1***-****-001*` → `1***-**--001*`), 홀로 쓴 `\` 는 사라진다. `~` 와 `<` 는
    /// 노션이 글자로 그려 손대지 않는다.
    pub fn escapes(&self, c: char) -> bool {
        match self.channel {
            Channel::GithubMarkdown => matches!(c, '~' | '<' | '*'),
            Channel::NotionMarkdown => matches!(c, '*' | '\\'),
            _ => false,
        }
    }

    /// 코드 안의 글자 하나를 적는다. **코드 안에서는 마크다운 이스케이프가 글자로 보인다** —
    /// 본문과 달리 `~` `<` 를 그대로 둔다. HTML 로 가는 채널만 escape 한다.
    pub fn code_char(&self, c: char, out: &mut String) {
        if self.html_out() {
            self.escape_char(c, out);
        } else {
            out.push(c);
        }
    }

    /// 코드(펜스 본문·info·고정폭 표)를 적는다. 본문 글자는 [`Vocab::escape_char`] 다.
    pub fn escape(&self, s: &str, out: &mut String) {
        if !self.html_out() {
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
                if markup >= self.limit {
                    out.push_str(text);
                    if !url.is_empty() && !escaped_eq(text, url) {
                        out.push_str(" (");
                        self.escape(url, out);
                        out.push(')');
                    }
                    return;
                }
                out.push_str("<a href=\"");
                push_attr(url, out);
                out.push_str("\">");
                out.push_str(text);
                out.push_str("</a>");
            }
            Channel::Html => {
                // **`innerHTML` 로 들어가는 출력이라 스킴을 가린다.** `[x](javascript:…)` 를
                // 그대로 `<a href>` 로 내면 누르는 순간 스크립트가 돈다. 안전한 스킴이 아니면
                // 링크 없이 글과 주소만 낸다 — 내용은 살린다.
                if !self.allowed(url) {
                    out.push_str(text);
                    if !url.is_empty() && !escaped_eq(text, url) {
                        out.push_str(" (");
                        self.escape(url, out);
                        out.push(')');
                    }
                    return;
                }
                out.push_str("<a href=\"");
                push_attr(url, out);
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
                // 텍스트가 주소 그대로면 오토링크다. `[url](url)` 보다 짧고 같은 뜻이다.
                // 스킴이 있어야 한다 — `<파일.md>` 는 오토링크가 아니라 꺾쇠 글자다.
                // 노션은 `<url>` 의 꺾쇠를 글자로 남긴다(실측) — 링크 문법으로 쓴다. 라벨은 날것의
                // 주소라 노션이 마커로 읽을 글자를 이스케이프하고, 주소의 괄호·공백은 퍼센트로 쓴다 —
                // 안 그러면 `a]b` 에서 라벨이, `x)` 에서 주소가 끝난다.
                if text == url && url.contains("://") && self.channel == Channel::NotionMarkdown {
                    out.push('[');
                    for c in text.chars() {
                        if matches!(c, '\\' | '*' | '_' | '~' | '`' | '[' | ']' | '$') {
                            out.push('\\');
                        }
                        out.push(c);
                    }
                    out.push_str("](");
                    for c in url.chars() {
                        match c {
                            '(' => out.push_str("%28"),
                            ')' => out.push_str("%29"),
                            ' ' => out.push_str("%20"),
                            _ => out.push(c),
                        }
                    }
                    out.push(')');
                    return;
                }
                if text == url && url.contains("://") {
                    out.push('<');
                    out.push_str(url);
                    out.push('>');
                    return;
                }
                out.push('[');
                out.push_str(text);
                out.push_str("](");
                out.push_str(url);
                out.push(')');
            }
        }
    }

    /// 이미지 `![alt](url)` 을 적는다. 텍스트는 이미 렌더된 대체 글이다.
    ///
    /// 브라우저는 옵션이 `Load` 이고 주소가 허용 스킴일 때만 `<img>` 로 불러온다 — 기본은
    /// 링크다(누르기 전에는 아무것도 안 불러온다). 텔레그램·plain 은 이미지 구문이 없어 링크로,
    /// 마크다운 채널은 `![alt](url)` 그대로 둔다(GitHub 은 그린다).
    pub fn image(&self, alt: &str, url: &str, out: &mut String) {
        match self.channel {
            Channel::Html if self.load_images && self.allowed(url) => {
                out.push_str("<img src=\"");
                push_attr(url, out);
                out.push_str("\" alt=\"");
                // 대체 글은 이미 이스케이프된 본문이다. 속성값이라 `"` 만 더 막는다.
                for c in alt.chars() {
                    if c == '"' {
                        out.push_str("&quot;");
                    } else {
                        out.push(c);
                    }
                }
                out.push_str("\">");
            }
            Channel::Html | Channel::TelegramHtml | Channel::Plain => self.link(alt, url, out),
            Channel::SlackMarkdown | Channel::GithubMarkdown | Channel::NotionMarkdown => {
                out.push('!');
                self.link(alt, url, out);
            }
        }
    }

    /// 역슬래시로 이스케이프된 글자를 내보낸다.
    ///
    /// **마크다운을 그대로 내보내는 채널에서는 이스케이프를 지키고 나간다.** 벗겨서 맨몸
    /// `*` 를 내보내면 저자가 글자로 쓴 별표가 그 채널에서 강조로 읽힌다 — 벗기는 것이
    /// 오히려 뜻을 바꾼다. HTML 로 가는 채널은 마커라는 개념이 없으니 그냥 escape 한다.
    pub fn literal(&self, c: char, out: &mut String) {
        match self.channel {
            Channel::TelegramHtml | Channel::Plain | Channel::Html => self.escape_char(c, out),
            // **그 채널의 마크다운이 읽는 글자면 이스케이프를 지킨다.** 강조 마커만 지키면
            // `\# 제목` 이 제목이 되고 `\[x\](url)` 이 링크가 된다 — 저자가 글자로
            // 쓴 것을 채널이 구문으로 읽어 버린다.
            //
            // 노션은 `$…$` 를 수식으로 읽어서 `\$` 도 지킨다(실측 — 벗기면 `\$x\$` 가 수식이 된다).
            _ if matches!(
                c,
                '*' | '_' | '~' | '`' | '\\' | '[' | ']' | '(' | ')' | '#' | '>' | '|' | '-'
                    | '+' | '.' | '!'
            ) || (c == '$' && self.channel == Channel::NotionMarkdown) =>
            {
                out.push('\\');
                out.push(c);
            }
            _ => self.escape_char(c, out),
        }
    }

    /// 불릿 마커. `SPEC.md` 8절의 표.
    pub fn bullet(&self) -> &'static str {
        match self.channel {
            Channel::SlackMarkdown | Channel::GithubMarkdown | Channel::NotionMarkdown => "- ",
            _ => "• ",
        }
    }

    pub fn quote_prefix(&self) -> &'static str {
        if self.html_out() { "" } else { "> " }
    }

    pub fn quote_open(&self) -> &'static str {
        if self.html_out() { "<blockquote>" } else { "" }
    }

    pub fn quote_close(&self) -> &'static str {
        if self.html_out() { "</blockquote>" } else { "" }
    }

    /// 구분선. 텔레그램에도 Plain 에도 구문이 없어 글자로 그린다.
    pub fn rule(&self) -> &'static str {
        match self.channel {
            Channel::TelegramHtml | Channel::Plain => "──────────",
            Channel::Html => "<hr>",
            _ => "---",
        }
    }

    /// 헤딩이 쓸 수 있는 가장 깊은 레벨. 구문이 없는 채널은 0.
    pub fn max_heading(&self) -> usize {
        match self.channel {
            // 슬랙 문서가 "모든 헤딩 레벨을 같은 크기로 그린다"고 적고 있다.
            // 그러니 더 깊이 적을 값이 없다 — 셋에서 끊는다.
            Channel::SlackMarkdown => 3,
            // GitHub 은 여섯 단계를 크기를 달리해 그린다.
            Channel::GithubMarkdown | Channel::Html => 6,
            // 노션 헤딩은 네 단계다 — 다섯·여섯은 노션이 넷으로 바꾼다(명세, 실측도 같다).
            Channel::NotionMarkdown => 4,
            Channel::TelegramHtml | Channel::Plain => 0,
        }
    }

    /// 고정폭 블록을 여닫는다. 표와 코드펜스가 같이 쓴다.
    pub fn verbatim_open(&self, info: &str, out: &mut String) {
        match self.channel {
            Channel::TelegramHtml | Channel::Html => {
                out.push_str("<pre>");
                let lang = fence_lang(info);
                if !lang.is_empty() {
                    out.push_str("<code class=\"language-");
                    // 속성값이다 — `"` 까지 escape 한다. 안 하면 ```` ```x" onmouseover="… ````
                    // 가 속성을 하나 더 끼워 넣는다(innerHTML 로 들어가는 채널에서 스크립트).
                    push_attr(lang, out);
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
        !matches!(self.channel, Channel::TelegramHtml | Channel::Plain | Channel::Html)
    }

    pub fn verbatim_close(&self, info: &str, out: &mut String) {
        match self.channel {
            Channel::TelegramHtml | Channel::Html => {
                if !fence_lang(info).is_empty() {
                    out.push_str("</code>");
                }
                out.push_str("</pre>");
            }
            Channel::Plain => {}
            _ => out.push_str("\n```"),
        }
    }
}

/// 코드펜스 info 에서 `<code class="language-…">` 에 넣을 언어. info 의 첫 단어다(CommonMark).
/// 32자를 넘으면 언어 이름이 아니라서 버린다 — 여는 태그가 한도만큼 길어지면 분할기가 태그
/// 한가운데를 가른다(최소 한도 256 퍼즈에서 나왔다).
fn fence_lang(info: &str) -> &str {
    let lang = info.split_whitespace().next().unwrap_or("");
    if lang.chars().count() <= 32 { lang } else { "" }
}

/// 속성값으로 escape 해서 적는다.
fn push_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// 이스케이프하고 나면 몇 글자가 되는가. **재기만 하고 만들지는 않는다** — 스트리밍
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
