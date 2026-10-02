// Package mdwire safely delivers agent-written markdown to chat channels.
//
// It is a Go port of the Rust core (crates/mdwire-core). **The corpus is the
// source of truth**: both implementations must pass the same corpus/cases, and
// when their rules disagree, the corpus decides. It depends only on the
// standard library.
package mdwire

import "math"

// Channel is an output channel. Each channel accepts a different syntax, and
// the narrower output decides the parsing scope.
type Channel int

const (
	// TelegramHTML is Telegram parse_mode=HTML. 9 allowed tags, no tables or
	// headings, 4096 characters.
	TelegramHTML Channel = iota
	// SlackMarkdown is Slack markdown_text. Slack converts standard markdown
	// itself. 12,000 characters.
	SlackMarkdown
	// Plain strips all markup. The fallback path.
	Plain
	// GithubMarkdown is a GitHub comment or PR body (GFM). 65,536 characters.
	// It emits the same markdown as Slack, but escapes the two characters GFM
	// reads as syntax (`~` `<`). Placed last so existing values do not shift.
	GithubMarkdown
	// HTML is an HTML fragment for the browser. Headings, lists, tables and
	// code blocks are drawn as tags. No limit.
	//
	// It assumes the output goes straight into innerHTML: all text is escaped,
	// raw HTML from the source survives only as inline tags with attributes
	// dropped, and only http(s) and mailto links become <a>. Appending
	// Streamer.CloseOpen to the accumulated stream output makes it safe to
	// insert as is. Placed last so existing values do not shift.
	HTML
	// NotionMarkdown is a Notion page body (Notion-flavored Markdown). Four
	// heading levels. It emits the same markdown as GitHub, but strips inline
	// HTML that Notion cannot render and writes autolinks as [url](url).
	// Notion renders emphasis directly before a Korean particle as is, so it
	// is not turned into <strong>. Placed last so existing values do not shift.
	NotionMarkdown
)

// Channels returns every output channel. The corpus and the harness iterate
// over this list.
func Channels() []Channel {
	return []Channel{TelegramHTML, SlackMarkdown, GithubMarkdown, NotionMarkdown, Plain, HTML}
}

// Name returns the name used for corpus directories and CLI arguments. It is
// the canonical string form of a channel.
func (c Channel) Name() string {
	switch c {
	case TelegramHTML:
		return "telegram-html"
	case SlackMarkdown:
		return "slack-markdown"
	case GithubMarkdown:
		return "github-markdown"
	case NotionMarkdown:
		return "notion-markdown"
	case HTML:
		return "html"
	default:
		return "plain"
	}
}

// ParseChannel looks up a channel by name. The second result is false if
// there is no such channel.
func ParseChannel(name string) (Channel, bool) {
	for _, c := range Channels() {
		if c.Name() == name {
			return c, true
		}
	}
	return 0, false
}

// Limit returns the channel's message length limit in characters. Splitting
// is based on it.
func (c Channel) Limit() int {
	switch c {
	case TelegramHTML:
		return 4096
	case GithubMarkdown:
		// 코멘트 본문의 한도다. 넘기면 API 가 422 로 거절한다("Body is too long").
		return 65_536
	case NotionMarkdown:
		// 재지 않았다. 페이지 하나의 본문이라 GitHub 과 같은 값을 둔다 — 러스트 쪽과 같다.
		return 65_536
	case HTML:
		// 브라우저에는 메시지 한도가 없다. 나누지 않는다.
		return math.MaxInt
	}
	return 12_000
}
