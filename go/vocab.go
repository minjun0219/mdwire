package mdwire

import (
	"strings"
	"unicode"
	"unicode/utf8"
)

// 채널별 출력 어휘.
//
// 파서는 채널을 모른다. 무엇을 어떻게 적을지만 여기서 갈린다. 채널을 늘릴 때 손대는 곳이
// 이 파일이어야 한다 — SPEC 4절·8절의 표가 코드에 대응하는 방식이다.

// emph 는 인라인 강조의 종류다.
type emph int

const (
	emphBold emph = iota
	emphItalic
	emphStrike
	emphCode
	// emphTag 부터는 원문에 적힌 인라인 HTML 태그다 — emphTag + inlineTags 의 번호. 태그를 그리는
	// 채널(html · GitHub)에서 강조와 같은 스택에 올려 짝과 중첩을 맞춘다.
	emphTag
)

// tagEmph 는 inlineTags 의 t 번 태그를 강조 스택에 올릴 값이다.
func tagEmph(t int) emph { return emphTag + emph(t) }

// inlineTags 는 살려 둘 수 있는 인라인 태그다 — 이름, 여는 태그, 닫는 태그. 속성은 버리고 이
// 모양으로 다시 쓴다. <br> 은 짝이 없어 따로 다룬다. GitHub 이 받는 것만 — font 는 새니타이저가 지운다.
var inlineTags = [15][3]string{
	{"sub", "<sub>", "</sub>"},
	{"sup", "<sup>", "</sup>"},
	{"b", "<b>", "</b>"},
	{"strong", "<strong>", "</strong>"},
	{"i", "<i>", "</i>"},
	{"em", "<em>", "</em>"},
	{"u", "<u>", "</u>"},
	{"s", "<s>", "</s>"},
	{"strike", "<strike>", "</strike>"},
	{"del", "<del>", "</del>"},
	{"code", "<code>", "</code>"},
	{"span", "<span>", "</span>"},
	{"small", "<small>", "</small>"},
	{"mark", "<mark>", "</mark>"},
	{"kbd", "<kbd>", "</kbd>"},
}

// vocab 은 채널 하나의 출력 어휘와 정책이다.
type vocab struct {
	channel Channel
	// limit 은 한 조각의 한도다. 채널 기본값이거나 Options.Limit 이다.
	limit int
	// 브라우저 채널의 정책. 스킴 목록은 슬라이스라 표 칸마다 어휘를 복사해도 목록은 한 벌이다.
	br         bool
	loadImages bool
	schemes    []string
}

// newVocab 은 옵션으로 만든다. 한도가 0 이면 채널 기본값, 음수는 1 로 올린다.
func newVocab(ch Channel, o Options) vocab {
	limit := o.Limit
	switch {
	// 브라우저 채널은 나누지 않는다 — 분할기가 <p>·<ul> 을 여닫지 않는다.
	case limit == 0 || ch == HTML:
		limit = ch.Limit()
	case limit < MinLimit:
		limit = MinLimit
	}
	return vocab{
		channel:    ch,
		limit:      limit,
		br:         o.HTML.LineBreaks == LineBreaksBR,
		loadImages: o.HTML.Images == ImagesLoad,
		schemes:    o.HTML.Schemes,
	}
}

// defaultSchemes 는 목록을 안 줬을 때 받는 스킴이다.
var defaultSchemes = [...]string{"http", "https", "mailto"}

// allowed 는 브라우저에서 눌러도(불러와도) 되는 주소인가다 — 허용 스킴만. 대소문자·앞 공백으로
// 숨긴 JavaScript: 도 스킴이 달라 걸러진다. 목록을 순회만 한다 — 링크마다 불리니 할당하지 않는다.
func (v vocab) allowed(url string) bool {
	u := strings.TrimLeftFunc(url, unicode.IsSpace)
	colon := strings.IndexByte(u, ':')
	if colon < 0 {
		return false
	}
	scheme := u[:colon]
	if v.schemes == nil {
		for _, s := range defaultSchemes {
			if eqFoldASCII(scheme, s) {
				return true
			}
		}
		return false
	}
	for _, s := range v.schemes {
		if eqFoldASCII(scheme, s) {
			return true
		}
	}
	return false
}

// tablesNative 는 채널이 표를 직접 그리는가다. 그리면 고정폭으로 내리는 것이 손해다 —
// 슬랙 markdown_text 는 표준 마크다운 표를 네이티브로 그린다. GitHub 은 GFM 표가 원래 문법이다.
func (v vocab) tablesNative() bool { return v.isMarkdown() || v.isHTML() }

// isHTML 은 브라우저용 HTML 채널인가다. 블록까지 태그로 그린다(<p> <h2> <ul> <table>).
func (v vocab) isHTML() bool { return v.channel == HTML }

// htmlOut 은 출력이 HTML 이라 글자를 escape 해야 하는가다. 텔레그램과 브라우저.
func (v vocab) htmlOut() bool { return v.channel == TelegramHTML || v.channel == HTML }

// lineBreak 는 블록 안의 줄바꿈이다. 브라우저는 \n 을 공백으로 접으므로 <br> 을 앞에 둔다 —
// 다른 채널이 다 줄바꿈을 살리니 같은 글이 같은 모양으로 보이게.
func (v vocab) lineBreak() string {
	if v.isHTML() && v.br {
		return "<br>\n"
	}
	return "\n"
}

// isMarkdown 은 마크다운을 그대로 내보내는 채널인가다.
func (v vocab) isMarkdown() bool {
	return v.channel == SlackMarkdown || v.channel == GithubMarkdown || v.channel == NotionMarkdown
}

// isPlain 은 마크업 문법 자체가 없는 채널인가다. 강조도 표도 글자로 내려앉는다.
func (v vocab) isPlain() bool { return v.channel == Plain }

func (v vocab) open(e emph) string {
	if e >= emphTag {
		return inlineTags[e-emphTag][1]
	}
	switch v.channel {
	case TelegramHTML:
		switch e {
		case emphBold:
			return "<b>"
		case emphItalic:
			return "<i>"
		case emphStrike:
			return "<s>"
		default:
			return "<code>"
		}
	case Plain:
		return ""
	case HTML:
		return v.openHTML(e)
	default:
		switch e {
		case emphStrike:
			return "~~"
		case emphBold:
			return "**"
		case emphItalic:
			// 기울임은 `_` 가 아니라 `*` 다. 슬랙 markdown_text 는 `_기울임_가` 를 글자 그대로
			// 두고 `*기울임*가` 는 기울인다(실측 2026-09-22, CommonMark 의 단어 안 `_` 규칙).
			return "*"
		default:
			return "`"
		}
	}
}

// htmlEmphasis 는 마크다운 마커를 채널이 못 읽는 자리에서 태그로 낼 수 있는가다. GitHub 만
// 그렇다 — 인라인 HTML 을 그리고, 마커와 달리 flanking 을 안 따진다.
func (v vocab) htmlEmphasis() bool { return v.channel == GithubMarkdown }

// keepsBr 는 원문의 <br> 을 그대로 두는가다(칸 안이든 밖이든). 노션만 — 한 블록 안의 줄바꿈으로
// 그린다. \n 으로 바꾸면 인용이 둘로 갈리고 강조가 줄을 넘는다.
func (v vocab) keepsBr() bool { return v.channel == NotionMarkdown }

// lineEmphasis 는 강조를 줄마다 닫고 다시 여는가다. 노션만 — 줄을 넘는 마커의 짝을 못 맞춘다.
func (v vocab) lineEmphasis() bool { return v.channel == NotionMarkdown }

func (v vocab) openHTML(e emph) string {
	if e >= emphTag {
		return inlineTags[e-emphTag][1]
	}
	switch e {
	case emphBold:
		return "<strong>"
	case emphItalic:
		return "<em>"
	case emphStrike:
		return "<del>"
	default:
		return "<code>"
	}
}

func (v vocab) closeHTML(e emph) string {
	if e >= emphTag {
		return inlineTags[e-emphTag][2]
	}
	switch e {
	case emphBold:
		return "</strong>"
	case emphItalic:
		return "</em>"
	case emphStrike:
		return "</del>"
	default:
		return "</code>"
	}
}

func (v vocab) close(e emph) string {
	if e >= emphTag {
		return inlineTags[e-emphTag][2]
	}
	if v.channel == HTML {
		return v.closeHTML(e)
	}
	if v.channel == TelegramHTML {
		switch e {
		case emphBold:
			return "</b>"
		case emphItalic:
			return "</i>"
		case emphStrike:
			return "</s>"
		default:
			return "</code>"
		}
	}
	return v.open(e)
}

// escapeChar 는 글자 하나를 본문으로 적는다.
//
// GitHub 에서는 `~` 와 `<` 를 탈출한다(실측 2026-09-30, POST /markdown gfm). GFM 은 홑 `~` 도
// 취소선으로 읽어서 `약 ~40km, 5~6월` 의 `40km, 5` 가 그어지고, `Vec<T>` 의 `<T>` 는 HTML
// 태그로 읽혀 새니타이저가 지운다. 슬랙 markdown_text 는 둘 다 글자로 그려서 손대지 않는다.
func (v vocab) escapeChar(c rune, out *[]byte) {
	if v.escapes(c) {
		*out = append(*out, '\\')
	}
	if v.htmlOut() {
		switch c {
		case '&':
			*out = append(*out, "&amp;"...)
			return
		case '<':
			*out = append(*out, "&lt;"...)
			return
		case '>':
			*out = append(*out, "&gt;"...)
			return
		}
	}
	*out = appendRune(*out, c)
}

// escapes 는 본문에 글자로 적을 때 역슬래시를 앞에 붙이는 글자인가다.
func (v vocab) escapes(c rune) bool {
	switch v.channel {
	case GithubMarkdown:
		return c == '~' || c == '<' || c == '*'
	case NotionMarkdown:
		// 노션은 마스킹 번호의 별표를 먹고 홀로 쓴 역슬래시를 지운다(실측 2026-10-01).
		return c == '*' || c == '\\'
	}
	return false
}

// codeChar 는 코드 안의 글자 하나를 적는다. 코드 안에서는 마크다운 탈출이 글자로 보인다 —
// 본문과 달리 `~` `<` 를 그대로 둔다. HTML 로 가는 채널만 escape 한다.
func (v vocab) codeChar(c rune, out *[]byte) {
	if v.htmlOut() {
		v.escapeChar(c, out)
		return
	}
	*out = appendRune(*out, c)
}

// escape 는 코드(펜스 본문·info·고정폭 표)를 적는다. 본문 글자는 escapeChar 다.
func (v vocab) escape(s string, out *[]byte) {
	// 대부분의 줄에는 이스케이프할 글자가 없다. 있을 때만 한 글자씩 간다.
	if !v.htmlOut() || !strings.ContainsAny(s, "&<>") {
		*out = append(*out, s...)
		return
	}
	for _, c := range s {
		v.escapeChar(c, out)
	}
}

// link 는 링크를 적는다. 텍스트는 이미 렌더된 것을 받는다.
func (v vocab) link(text, url string, out *[]byte) {
	switch v.channel {
	case TelegramHTML:
		// 한도를 넘는 주소는 링크로 내지 않는다. 여는 태그 하나가 메시지를 다 차지하면
		// 조각을 아무리 나눠도 내용이 한 글자도 안 들어간다. 주소는 괄호에 넣어 글로
		// 내보낸다 — 링크는 죽어도 내용은 산다.
		markup := escapedLen(url) + len(`<a href=""></a>`)
		if markup >= v.limit {
			*out = append(*out, text...)
			if url != "" && !escapedEq(text, url) {
				*out = append(*out, " ("...)
				v.escape(url, out)
				*out = append(*out, ')')
			}
			return
		}
		*out = append(*out, `<a href="`...)
		appendAttr(url, out)
		*out = append(*out, `">`...)
		*out = append(*out, text...)
		*out = append(*out, "</a>"...)
	case HTML:
		// innerHTML 로 들어가는 출력이라 스킴을 가린다. [x](javascript:…) 를 그대로 <a href> 로
		// 내면 누르는 순간 스크립트가 돈다. 안전한 스킴이 아니면 링크 없이 글과 주소만 낸다 —
		// 내용은 살린다.
		if !v.allowed(url) {
			*out = append(*out, text...)
			if url != "" && !escapedEq(text, url) {
				*out = append(*out, " ("...)
				v.escape(url, out)
				*out = append(*out, ')')
			}
			return
		}
		*out = append(*out, `<a href="`...)
		appendAttr(url, out)
		*out = append(*out, `">`...)
		*out = append(*out, text...)
		*out = append(*out, "</a>"...)
	case Plain:
		*out = append(*out, text...)
		if url != "" && url != text {
			*out = append(*out, " ("...)
			*out = append(*out, url...)
			*out = append(*out, ')')
		}
	default:
		// 텍스트가 주소 그대로면 오토링크다. `[url](url)` 보다 짧고 같은 뜻이다. 스킴이 있어야
		// 한다 — `<파일.md>` 는 오토링크가 아니라 꺾쇠 글자다.
		// 노션은 <url> 의 꺾쇠를 글자로 남긴다(실측) — 링크 문법으로 쓴다. 라벨은 노션이 마커로
		// 읽을 글자를 탈출하고, 주소의 괄호·공백은 퍼센트로 쓴다 — 러스트 쪽과 같다.
		if text == url && strings.Contains(url, "://") && v.channel == NotionMarkdown {
			*out = append(*out, '[')
			for _, c := range text {
				if strings.ContainsRune("\\*_~`[]$", c) {
					*out = append(*out, '\\')
				}
				*out = appendRune(*out, c)
			}
			*out = append(*out, "]("...)
			for _, c := range url {
				switch c {
				case '(':
					*out = append(*out, "%28"...)
				case ')':
					*out = append(*out, "%29"...)
				case ' ':
					*out = append(*out, "%20"...)
				default:
					*out = appendRune(*out, c)
				}
			}
			*out = append(*out, ')')
			return
		}
		if text == url && strings.Contains(url, "://") {
			*out = append(*out, '<')
			*out = append(*out, url...)
			*out = append(*out, '>')
			return
		}
		*out = append(*out, '[')
		*out = append(*out, text...)
		*out = append(*out, "]("...)
		*out = append(*out, url...)
		*out = append(*out, ')')
	}
}

// image 는 이미지 `![alt](url)` 을 적는다. 텍스트는 이미 렌더된 대체 글이다.
//
// 브라우저는 옵션이 ImagesLoad 이고 주소가 허용 스킴일 때만 <img> 로 불러온다 — 기본은
// 링크다(누르기 전에는 아무것도 안 불러온다). 텔레그램·plain 은 이미지 구문이 없어 링크로,
// 마크다운 채널은 `![alt](url)` 그대로 둔다(GitHub 은 그린다).
func (v vocab) image(alt, url string, out *[]byte) {
	switch v.channel {
	case HTML:
		if !v.loadImages || !v.allowed(url) {
			v.link(alt, url, out)
			return
		}
		*out = append(*out, `<img src="`...)
		appendAttr(url, out)
		*out = append(*out, `" alt="`...)
		// 대체 글은 이미 escape 된 본문이다. 속성값이라 `"` 만 더 막는다.
		for i := 0; i < len(alt); i++ {
			if alt[i] == '"' {
				*out = append(*out, "&quot;"...)
			} else {
				*out = append(*out, alt[i])
			}
		}
		*out = append(*out, `">`...)
	case TelegramHTML, Plain:
		v.link(alt, url, out)
	default:
		*out = append(*out, '!')
		v.link(alt, url, out)
	}
}

// literal 은 역슬래시로 탈출된 글자를 내보낸다.
//
// 마크다운을 그대로 내보내는 채널에서는 탈출을 지키고 나간다. 벗겨서 맨몸 `*` 를 내보내면
// 저자가 글자로 쓴 별표가 그 채널에서 강조로 읽힌다. HTML 로 가는 채널은 마커라는 개념이
// 없으니 그냥 escape 한다.
func (v vocab) literal(c rune, out *[]byte) {
	if v.htmlOut() || v.channel == Plain {
		v.escapeChar(c, out)
		return
	}
	// 그 채널의 마크다운이 읽는 글자면 탈출을 지킨다. 강조 마커만 지키면 `\# 제목` 이 제목이
	// 되고 `\[x\](url)` 이 링크가 된다.
	// 노션은 $…$ 를 수식으로 읽어서 \$ 도 지킨다.
	if strings.ContainsRune("*_~`\\[]()#>|-+.!", c) || (c == '$' && v.channel == NotionMarkdown) {
		*out = append(*out, '\\')
		*out = appendRune(*out, c)
		return
	}
	v.escapeChar(c, out)
}

// bullet 은 불릿 마커다. SPEC 8절의 표.
func (v vocab) bullet() string {
	if v.isMarkdown() {
		return "- "
	}
	return "• "
}

func (v vocab) quotePrefix() string {
	if v.htmlOut() {
		return ""
	}
	return "> "
}

func (v vocab) quoteOpen() string {
	if v.htmlOut() {
		return "<blockquote>"
	}
	return ""
}

func (v vocab) quoteClose() string {
	if v.htmlOut() {
		return "</blockquote>"
	}
	return ""
}

// rule 은 구분선이다. 텔레그램에도 Plain 에도 구문이 없어 글자로 그린다.
func (v vocab) rule() string {
	if v.isMarkdown() {
		return "---"
	}
	if v.isHTML() {
		return "<hr>"
	}
	return "──────────"
}

// maxHeading 은 헤딩이 쓸 수 있는 가장 깊은 레벨이다. 구문이 없는 채널은 0.
func (v vocab) maxHeading() int {
	if v.channel == SlackMarkdown {
		// 슬랙 문서가 "모든 헤딩 레벨을 같은 크기로 그린다"고 적고 있다. 셋에서 끊는다.
		return 3
	}
	if v.channel == GithubMarkdown || v.channel == HTML {
		// GitHub 은 여섯 단계를 크기를 달리해 그린다.
		return 6
	}
	if v.channel == NotionMarkdown {
		// 노션 헤딩은 네 단계다 — 다섯·여섯은 노션이 넷으로 바꾼다.
		return 4
	}
	return 0
}

// verbatimOpen 은 고정폭 블록을 연다. 표와 코드펜스가 같이 쓴다.
func (v vocab) verbatimOpen(info string, out *[]byte) {
	switch v.channel {
	case TelegramHTML, HTML:
		*out = append(*out, "<pre>"...)
		if lang := fenceLang(info); lang != "" {
			*out = append(*out, `<code class="language-`...)
			// 속성값이다 — `"` 까지 escape 한다. 안 하면 info 가 속성을 하나 더 끼워 넣는다.
			appendAttr(lang, out)
			*out = append(*out, `">`...)
		}
	case Plain:
	default:
		*out = append(*out, "```"...)
		*out = append(*out, info...)
	}
}

// fenceLang 은 코드펜스 info 에서 class 에 넣을 언어다 — 러스트 쪽 fence_lang. 첫 단어이고,
// 32자를 넘으면 언어 이름이 아니라서 버린다.
func fenceLang(info string) string {
	f := strings.Fields(info)
	if len(f) == 0 || utf8.RuneCountInString(f[0]) > 32 {
		return ""
	}
	return f[0]
}

// verbatimBodyNewline 은 여는 마크업과 첫 내용 줄 사이에 줄바꿈이 필요한가다.
// ``` 는 필요하고, <pre> 는 넣으면 빈 줄이 하나 생긴다.
func (v vocab) verbatimBodyNewline() bool {
	return v.channel != TelegramHTML && v.channel != Plain && v.channel != HTML
}

func (v vocab) verbatimClose(info string, out *[]byte) {
	switch v.channel {
	case TelegramHTML, HTML:
		if fenceLang(info) != "" {
			*out = append(*out, "</code>"...)
		}
		*out = append(*out, "</pre>"...)
	case Plain:
	default:
		*out = append(*out, "\n```"...)
	}
}

// appendAttr 는 속성값으로 escape 해서 적는다.
func appendAttr(s string, out *[]byte) {
	for _, c := range s {
		switch c {
		case '&':
			*out = append(*out, "&amp;"...)
		case '<':
			*out = append(*out, "&lt;"...)
		case '>':
			*out = append(*out, "&gt;"...)
		case '"':
			*out = append(*out, "&quot;"...)
		default:
			*out = appendRune(*out, c)
		}
	}
}

// eqFoldASCII 는 ASCII 대소문자만 무시하고 견준다. strings.EqualFold 는 유니코드 접기까지 해서
// 러스트의 eq_ignore_ascii_case 와 뜻이 다르다.
func eqFoldASCII(a, b string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := 0; i < len(a); i++ {
		x, y := a[i], b[i]
		if 'A' <= x && x <= 'Z' {
			x += 'a' - 'A'
		}
		if 'A' <= y && y <= 'Z' {
			y += 'a' - 'A'
		}
		if x != y {
			return false
		}
	}
	return true
}

// escapedLen 은 escape 하고 나면 몇 글자가 되는가다. 재기만 하고 만들지는 않는다 —
// 스트리밍 경로에서 링크마다 문자열을 하나씩 더 만들 수는 없다.
func escapedLen(url string) int {
	n := 0
	for _, c := range url {
		switch c {
		case '&':
			n += 5
		case '<', '>':
			n += 4
		case '"':
			n += 6
		default:
			n++
		}
	}
	return n
}

// escapedEq 는 escape 한 주소가 이 텍스트와 같은가다. 만들지 않고 견준다.
func escapedEq(text, url string) bool {
	t := []rune(text)
	k := 0
	for _, c := range url {
		var esc string
		switch c {
		case '&':
			esc = "&amp;"
		case '<':
			esc = "&lt;"
		case '>':
			esc = "&gt;"
		case '"':
			esc = "&quot;"
		default:
			if k >= len(t) || t[k] != c {
				return false
			}
			k++
			continue
		}
		for _, e := range esc {
			if k >= len(t) || t[k] != e {
				return false
			}
			k++
		}
	}
	return k == len(t)
}

// appendRune 은 utf8.AppendRune 이다 — 이름만 짧게.
func appendRune(b []byte, c rune) []byte {
	if c < 0x80 {
		return append(b, byte(c))
	}
	var tmp [4]byte
	n := encodeRune(tmp[:], c)
	return append(b, tmp[:n]...)
}
