// Package mdwire 는 에이전트가 만든 마크다운을 채팅 채널로 안전하게 내보낸다.
//
// Rust 코어(crates/mdwire-core)의 Go 이식이다. **코퍼스가 정본이다** — 두 구현은 같은
// corpus/cases 를 통과해야 하고, 규칙이 갈리면 코퍼스가 판정한다. 의존은 표준 라이브러리뿐이다.
package mdwire

// Channel 은 내보낼 채널이다. 받는 문법이 채널마다 다르고, 출력이 좁은 쪽이 파싱 범위를 정한다.
type Channel int

const (
	// TelegramHTML 은 Telegram parse_mode=HTML. 허용 태그 9개, 표·헤딩 없음, 4096자.
	TelegramHTML Channel = iota
	// SlackMarkdown 은 Slack markdown_text. 표준 마크다운을 슬랙이 직접 변환한다. 12,000자.
	SlackMarkdown
	// Plain 은 모든 마크업 제거. 폴백 경로.
	Plain
)

// Channels 는 내보낼 수 있는 채널 전부다. 코퍼스와 하네스가 이 목록을 돈다.
func Channels() []Channel {
	return []Channel{TelegramHTML, SlackMarkdown, Plain}
}

// Name 은 코퍼스 디렉토리와 CLI 인자에서 쓰는 이름이다. 채널을 문자열로 다루는 곳의 정본이다.
func (c Channel) Name() string {
	switch c {
	case TelegramHTML:
		return "telegram-html"
	case SlackMarkdown:
		return "slack-markdown"
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
	if c == TelegramHTML {
		return 4096
	}
	return 12_000
}
