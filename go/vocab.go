package mdwire

import "strings"

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
)

// vocab 은 채널 하나의 출력 어휘와 정책이다.
type vocab struct {
	channel Channel
}

// tablesNative 는 채널이 표를 직접 그리는가다. 그리면 고정폭으로 내리는 것이 손해다 —
// 슬랙 markdown_text 는 표준 마크다운 표를 네이티브로 그린다. GitHub 은 GFM 표가 원래 문법이다.
func (v vocab) tablesNative() bool { return v.isMarkdown() }

// isMarkdown 은 마크다운을 그대로 내보내는 채널인가다.
func (v vocab) isMarkdown() bool { return v.channel == SlackMarkdown || v.channel == GithubMarkdown }

// isPlain 은 마크업 문법 자체가 없는 채널인가다. 강조도 표도 글자로 내려앉는다.
func (v vocab) isPlain() bool { return v.channel == Plain }

func (v vocab) open(e emph) string {
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

func (v vocab) openHTML(e emph) string {
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
	if v.channel == TelegramHTML {
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
	return v.channel == GithubMarkdown && (c == '~' || c == '<')
}

// codeChar 는 코드 안의 글자 하나를 적는다. 코드 안에서는 마크다운 탈출이 글자로 보인다 —
// 본문과 달리 `~` `<` 를 그대로 둔다. HTML 로 가는 채널만 escape 한다.
func (v vocab) codeChar(c rune, out *[]byte) {
	if v.channel == TelegramHTML {
		v.escapeChar(c, out)
		return
	}
	*out = appendRune(*out, c)
}

// escape 는 코드(펜스 본문·info·고정폭 표)를 적는다. 본문 글자는 escapeChar 다.
func (v vocab) escape(s string, out *[]byte) {
	// 대부분의 줄에는 이스케이프할 글자가 없다. 있을 때만 한 글자씩 간다.
	if v.channel != TelegramHTML || !strings.ContainsAny(s, "&<>") {
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
		if markup >= v.channel.Limit() {
			*out = append(*out, text...)
			if url != "" && !escapedEq(text, url) {
				*out = append(*out, " ("...)
				v.escape(url, out)
				*out = append(*out, ')')
			}
			return
		}
		*out = append(*out, `<a href="`...)
		for _, c := range url {
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

// literal 은 역슬래시로 탈출된 글자를 내보낸다.
//
// 마크다운을 그대로 내보내는 채널에서는 탈출을 지키고 나간다. 벗겨서 맨몸 `*` 를 내보내면
// 저자가 글자로 쓴 별표가 그 채널에서 강조로 읽힌다. HTML 로 가는 채널은 마커라는 개념이
// 없으니 그냥 escape 한다.
func (v vocab) literal(c rune, out *[]byte) {
	if v.channel == TelegramHTML || v.channel == Plain {
		v.escapeChar(c, out)
		return
	}
	// 그 채널의 마크다운이 읽는 글자면 탈출을 지킨다. 강조 마커만 지키면 `\# 제목` 이 제목이
	// 되고 `\[x\](url)` 이 링크가 된다.
	if strings.ContainsRune("*_~`\\[]()#>|-+.!", c) {
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
	if v.channel == TelegramHTML {
		return ""
	}
	return "> "
}

func (v vocab) quoteOpen() string {
	if v.channel == TelegramHTML {
		return "<blockquote>"
	}
	return ""
}

func (v vocab) quoteClose() string {
	if v.channel == TelegramHTML {
		return "</blockquote>"
	}
	return ""
}

// rule 은 구분선이다. 텔레그램에도 Plain 에도 구문이 없어 글자로 그린다.
func (v vocab) rule() string {
	if v.isMarkdown() {
		return "---"
	}
	return "──────────"
}

// maxHeading 은 헤딩이 쓸 수 있는 가장 깊은 레벨이다. 구문이 없는 채널은 0.
func (v vocab) maxHeading() int {
	if v.channel == SlackMarkdown {
		// 슬랙 문서가 "모든 헤딩 레벨을 같은 크기로 그린다"고 적고 있다. 셋에서 끊는다.
		return 3
	}
	if v.channel == GithubMarkdown {
		// GitHub 은 여섯 단계를 크기를 달리해 그린다.
		return 6
	}
	return 0
}

// verbatimOpen 은 고정폭 블록을 연다. 표와 코드펜스가 같이 쓴다.
func (v vocab) verbatimOpen(info string, out *[]byte) {
	switch v.channel {
	case TelegramHTML:
		*out = append(*out, "<pre>"...)
		if info != "" {
			*out = append(*out, `<code class="language-`...)
			v.escape(info, out)
			*out = append(*out, `">`...)
		}
	case Plain:
	default:
		*out = append(*out, "```"...)
		*out = append(*out, info...)
	}
}

// verbatimBodyNewline 은 여는 마크업과 첫 내용 줄 사이에 줄바꿈이 필요한가다.
// ``` 는 필요하고, <pre> 는 넣으면 빈 줄이 하나 생긴다.
func (v vocab) verbatimBodyNewline() bool {
	return v.channel != TelegramHTML && v.channel != Plain
}

func (v vocab) verbatimClose(info string, out *[]byte) {
	switch v.channel {
	case TelegramHTML:
		if info != "" {
			*out = append(*out, "</code>"...)
		}
		*out = append(*out, "</pre>"...)
	case Plain:
	default:
		*out = append(*out, "\n```"...)
	}
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
