package mdwire

import (
	"fmt"
	"os"
	"os/exec"
	"strings"
	"testing"
	"unicode/utf8"
)

// 아무 입력이나 넣어도 지켜야 하는 것 — 러스트 하네스의 퍼즈와 같은 시드·같은 조각 알파벳이다.
// 죽지 않고(패닉), 조각마다 한도를 지키고, 스트리밍이 완성본과 같고, 텔레그램 태그가 조각 안에서
// 닫힌다. 강조 범위·낱말 손실은 보지 않는다 — 무작위 마커 더미에는 "원문의 뜻"이 없다.

type xorshift uint64

func (r *xorshift) next() uint64 {
	x := uint64(*r)
	x ^= x << 13
	x ^= x >> 7
	x ^= x << 17
	*r = xorshift(x)
	return x
}

func (r *xorshift) below(n int) int { return int(r.next() % uint64(n)) }

var fuzzPieces = []string{
	"*", "**", "***", "_", "__", "~", "~~", "`", "``", "```", "```go\n", "\n```\n", "\\", "\\*", "\\_",
	"[", "]", "(", ")", "[텍스트](https://a.com/x_y)", "[괄호 (안) 텍스트](https://a.com/p)", "<", ">",
	"<https://a.com/a/very/long/path/that/keeps/going/and/going/past/eighty/characters/for/sure/index.html|긴 링크>",
	"<b>", "</b>", "<sub>", "</sub>",
	"<br>", "<!-- 주석 -->", "<!-- 이건 아주 긴 주석이라 팔십 글자를 한참 넘어간다 — 스트리밍에서 이걸 놓으면 꺾쇠가 글자로 샌다 -->", "<https://a.com/p|문서>", "<https://a.com/q>", "|", "| a | b |\n|---|---|\n",
	"#", "## ", "> ", "- ", "  - ", "1. ", "---\n", "\n", "\n\n", "\r\n", " ", "  ", "\t",
	"가", "나다", "한글 조사가", "이다.", "word", "x", "2", "का_x", "&", "😀", "①", "•", ".md", "@id", "#40",
}

func fuzzDoc(r *xorshift) string {
	n := 1 + r.below(60)
	var b strings.Builder
	for i := 0; i < n; i++ {
		b.WriteString(fuzzPieces[r.below(len(fuzzPieces))])
	}
	return b.String()
}

// telegramBalanced 는 조각 하나 안에서 태그가 열리고 닫히는지, 허용 태그만 쓰는지 본다.
func telegramBalanced(part string) bool {
	allowed := map[string]bool{"b": true, "i": true, "u": true, "s": true, "code": true, "pre": true, "a": true, "blockquote": true, "tg-spoiler": true}
	ch := []rune(part)
	var stack []string
	i := 0
	for i < len(ch) {
		if ch[i] != '<' {
			i++
			continue
		}
		closing := i+1 < len(ch) && ch[i+1] == '/'
		from := i + 1
		if closing {
			from = i + 2
		}
		j := from
		for j < len(ch) && (isASCIIAlnum(ch[j]) || ch[j] == '-') {
			j++
		}
		name := string(ch[from:j])
		for j < len(ch) && ch[j] != '>' {
			j++
		}
		if j >= len(ch) || name == "" || !allowed[name] {
			return false
		}
		if closing {
			if len(stack) == 0 || stack[len(stack)-1] != name {
				return false
			}
			stack = stack[:len(stack)-1]
		} else {
			stack = append(stack, name)
		}
		i = j + 1
	}
	return len(stack) == 0
}

type channelDialect struct {
	ch   Channel
	from Dialect
}

// channelDialects 는 채널 × 입력 방언 전부다.
func channelDialects() []channelDialect {
	var out []channelDialect
	for _, ch := range Channels() {
		for _, d := range []Dialect{Markdown, SlackMrkdwn} {
			out = append(out, channelDialect{ch, d})
		}
	}
	return out
}

func fuzzRounds() int {
	if v := os.Getenv("MDWIRE_FUZZ_ROUNDS"); v != "" {
		n := 0
		for _, c := range v {
			if c < '0' || c > '9' {
				return 3000
			}
			n = n*10 + int(c-'0')
		}
		return n
	}
	return 3000
}

func TestRandomInputNeverBreaksTheInvariants(t *testing.T) {
	r := xorshift(0x9E3779B97F4A7C15)
	failures := 0
	for round := 0; round < fuzzRounds() && failures < 10; round++ {
		input := fuzzDoc(&r)
		for _, c := range channelDialects() {
			ch, opts := c.ch, Options{From: c.from}
			rendered := RenderWith(input, ch, opts)
			parts := rendered.Parts
			for _, p := range parts {
				if utf8.RuneCountInString(p) > ch.Limit() {
					t.Errorf("#%d %s: 한도 초과\n  입력: %q", round, ch.Name(), input)
					failures++
				}
				if ch == TelegramHTML && !telegramBalanced(p) {
					t.Errorf("#%d %s: 태그가 조각 안에서 안 닫혔다\n  입력: %q\n  조각: %q", round, ch.Name(), input, p)
					failures++
				}
			}
			if len(parts) != 1 {
				continue
			}
			for _, size := range []int{1, 3, 11} {
				s := NewStreamerWith(ch, opts)
				var got []byte
				for _, piece := range chunksOf(input, size) {
					s.PushTo(piece, &got)
				}
				s.FinishTo(&got)
				if string(got) != parts[0] {
					t.Errorf("#%d %s %s 조각 %d: 스트리밍이 다르다\n  입력: %q\n  완성본: %q\n  스트리밍: %q", round, ch.Name(), c.from.Name(), size, input, parts[0], got)
					failures++
					break
				}
				// 고친 것도 조각 크기와 무관하게 같아야 한다.
				if s.Repairs() != rendered.Repairs {
					t.Errorf("#%d %s %s 조각 %d: 고친 것이 다르다 %+v ≠ %+v\n  입력: %q", round, ch.Name(), c.from.Name(), size, s.Repairs(), rendered.Repairs, input)
					failures++
					break
				}
			}
		}
	}
}

// 러스트 코어와 무작위 입력으로 대조한다. 러스트 CLI 경로를 MDWIRE_RUST 로 주면 돈다 —
// 없으면 건너뛴다. 이것이 이식의 정의를 시험으로 박은 것이다: 같은 입력, 같은 답.
func TestParityWithRustCore(t *testing.T) {
	bin := os.Getenv("MDWIRE_RUST")
	if bin == "" {
		t.Skip("MDWIRE_RUST 가 없다 — 러스트 CLI 경로를 주면 무작위 입력으로 대조한다")
	}
	r := xorshift(0xD1B54A32D192ED03)
	rounds := fuzzRounds() / 10
	failures := 0
	for round := 0; round < rounds && failures < 10; round++ {
		input := fuzzDoc(&r)
		for _, c := range channelDialects() {
			cmd := exec.Command(bin, "--channel", c.ch.Name(), "--from", c.from.Name(), "--report")
			cmd.Stdin = strings.NewReader(input)
			var stderr strings.Builder
			cmd.Stderr = &stderr
			want, err := cmd.Output()
			if err != nil {
				t.Fatalf("러스트 CLI 실행 실패: %v", err)
			}
			rendered := RenderWith(input, c.ch, Options{From: c.from})
			got := strings.Join(rendered.Parts, "\x00")
			if got != string(want) {
				t.Errorf("#%d %s %s: 러스트와 다르다\n  입력: %q\n  go   %q\n  rust %q", round, c.ch.Name(), c.from.Name(), input, got, want)
				failures++
			}
			r := rendered.Repairs
			report := fmt.Sprintf(`{"closedEmphasis":%d,"closedFence":%d,"revertedCodeSpan":%d,"droppedMarker":%d}`,
				r.ClosedEmphasis, r.ClosedFence, r.RevertedCodeSpan, r.DroppedMarker)
			if report != strings.TrimSpace(stderr.String()) {
				t.Errorf("#%d %s %s: 고친 것이 러스트와 다르다\n  입력: %q\n  go   %s\n  rust %s", round, c.ch.Name(), c.from.Name(), input, report, stderr.String())
				failures++
			}
		}
	}
}

// 공백 없는 긴 강조를 글자로 끊어도 조각이 한도를 지킨다 — 러스트 쪽 같은 이름의 테스트.
func TestLongSpaceFreeSpanIsCutWithinTheLimit(t *testing.T) {
	input := "**" + strings.Repeat("a", 20000) + "**"
	for _, ch := range Channels() {
		parts := Render(input, ch)
		if len(parts) < 2 {
			t.Errorf("%s: 나뉘어야 한다", ch.Name())
		}
		for _, p := range parts {
			if n := utf8.RuneCountInString(p); n > ch.Limit() {
				t.Errorf("%s: %d자 조각 — 한도 %d", ch.Name(), n, ch.Limit())
			}
		}
	}
}
