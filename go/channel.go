// Package mdwire 는 에이전트가 만든 마크다운을 채팅 채널로 안전하게 내보낸다.
//
// Rust 코어(crates/mdwire-core)의 Go 이식이다. **코퍼스가 정본이다** — 두 구현은 같은
// corpus/cases 를 통과해야 하고, 규칙이 갈리면 코퍼스가 판정한다. 의존은 표준 라이브러리뿐이다.
package mdwire

import "math"

// Channel 은 내보낼 채널이다. 받는 문법이 채널마다 다르고, 출력이 좁은 쪽이 파싱 범위를 정한다.
type Channel int

const (
	// TelegramHTML 은 Telegram parse_mode=HTML. 허용 태그 9개, 표·헤딩 없음, 4096자.
	TelegramHTML Channel = iota
	// SlackMarkdown 은 Slack markdown_text. 표준 마크다운을 슬랙이 직접 변환한다. 12,000자.
	SlackMarkdown
	// Plain 은 모든 마크업 제거. 폴백 경로.
	Plain
	// GithubMarkdown 은 GitHub 코멘트·PR 본문(GFM). 65,536자. 슬랙과 같은 마크다운을 내되,
	// GFM 이 구문으로 읽는 글자 둘(`~` `<`)을 탈출한다. 값이 밀리지 않게 끝에 둔다.
	GithubMarkdown
	// HTML 은 브라우저에 넣을 HTML 조각. 헤딩·목록·표·코드블록을 태그로 그린다. 한도 없음.
	//
	// innerHTML 로 바로 넣는 것을 전제로 한다 — 글자는 전부 escape 하고, 원문의 HTML 은
	// 속성을 버린 인라인 태그만 살리며, 링크는 http(s)·mailto 만 <a> 로 낸다. 스트리밍
	// 누적본에 Streamer.CloseOpen 을 붙이면 그대로 넣어도 되는 모양이 된다. 값이 밀리지 않게 끝에 둔다.
	HTML
	// NotionMarkdown 은 노션 페이지 본문(Notion-flavored Markdown)이다. 헤딩은 네 단계. GitHub 과
	// 같은 마크다운을 내되, 노션이 못 그리는 인라인 HTML 은 벗기고 오토링크는 [url](url) 로 쓴다.
	// 조사 앞 강조는 노션이 그대로 그려서 <strong> 으로 바꾸지 않는다. 값이 밀리지 않게 끝에 둔다.
	NotionMarkdown
)

// Channels 는 내보낼 수 있는 채널 전부다. 코퍼스와 하네스가 이 목록을 돈다.
func Channels() []Channel {
	return []Channel{TelegramHTML, SlackMarkdown, GithubMarkdown, NotionMarkdown, Plain, HTML}
}

// Name 은 코퍼스 디렉토리와 CLI 인자에서 쓰는 이름이다. 채널을 문자열로 다루는 곳의 정본이다.
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

// ParseChannel 은 이름으로 채널을 찾는다. 없으면 ok 가 false 다.
func ParseChannel(name string) (Channel, bool) {
	for _, c := range Channels() {
		if c.Name() == name {
			return c, true
		}
	}
	return 0, false
}

// Limit 은 이 채널의 메시지 길이 한도(문자 수)다. 분할의 기준이다.
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
