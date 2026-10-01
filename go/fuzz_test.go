package mdwire

import (
	"fmt"
	"math"
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
	// 브라우저 채널이 막아야 하는 것들 — 스킴, 이벤트 속성, 속성값에 드는 info, 마스킹 번호.
	"[x](javascript:alert(1))", "[m](MAILTO:a@b.c)", `<span onclick="x">`, "</span>", "<SUB>", "```x\" y=\"z\n", "4***-****-003*",
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
	// 가장 큰 한도보다 길어야 모든 채널이 나눈다. 한도가 없는 채널(html)은 나누지 않으니 뺀다.
	var splits []Channel
	longest := 0
	for _, ch := range Channels() {
		if ch.Limit() < math.MaxInt {
			splits = append(splits, ch)
			longest = max(longest, ch.Limit())
		}
	}
	input := "**" + strings.Repeat("a", longest+longest/2) + "**"
	for _, ch := range splits {
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

// 살려 둔 원문 태그도 조각 경계에서 닫고 다시 연다 — 러스트 쪽 같은 이름의 테스트.
func TestGithubKeptTagsSurviveSplitsAndMatchByName(t *testing.T) {
	input := "x <sub>" + strings.Repeat("가나 ", 30000) + "</sub> 끝"
	parts := Render(input, GithubMarkdown)
	if len(parts) < 2 {
		t.Fatal("나뉘어야 한다")
	}
	for _, p := range parts {
		if strings.Count(p, "<sub>") != strings.Count(p, "</sub>") {
			t.Fatal("조각 안에서 짝이 안 맞는다")
		}
	}
	cases := [][2]string{
		{"<sub>foo <kbd>x</kbd></sub> 뒤", "<sub>foo <kbd>x</kbd></sub> 뒤"},
		{"<sub>\nfoo <kbd>x</kbd></sub> 뒤", "foo <kbd>x</kbd> 뒤"},
		{"본문\n\n<sub>모델 · 토큰</sub>", "본문\n\n<sub>모델 · 토큰</sub>"},
		{"<br><!-- x -->\n**굵게**", "**굵게**"},
		{"<details><summary>약 ~40ms</summary>", `약 \~40ms`},
		{"<ins>새</ins> 글", "<ins>새</ins> 글"},
		{"| <sub>h</sub> | b |\n|---|---|\n| 1 | 2 |", "| <sub>h</sub> | b |\n| --- | --- |\n| 1 | 2 |"},
		{`앞 <SPAN style="x">가</SPAN>`, "앞 <span>가</span>"},
	}
	for _, c := range cases {
		if got := strings.Join(Render(c[0], GithubMarkdown), ""); got != c[1] {
			t.Errorf("%q\n  got  %q\n  want %q", c[0], got, c[1])
		}
	}
}

// 노션 채널 — 러스트 쪽 notion_escapes_what_it_eats_and_strips_tags 와 같은 기대값.
func TestNotionEscapesWhatItEatsAndStripsTags(t *testing.T) {
	cases := [][2]string{
		{"**설정(config)**을 바꾼다", "**설정(config)**을 바꾼다"},
		{"H<sub>2</sub>O 와 <kbd>C</kbd>", "H2O 와 C"},
		{"카드 1***-****-001* 끝", `카드 1\*\*\*-\*\*\*\*-001\* 끝`},
		{`백슬래시 \ 하나`, `백슬래시 \\ 하나`},
		{"약 ~40km, Vec<T>", "약 ~40km, Vec<T>"},
		{"\\*별\\* `a*b\\c`", "\\*별\\* `a*b\\c`"},
		{"<https://a.com/x_y>", `[https://a.com/x\_y](https://a.com/x_y)`},
		{"##### 다섯\n\n* 별표 목록", "#### 다섯\n\n- 별표 목록"},
		{"> **a<br>b** 끝", "> **a<br>b** 끝"},
		{"| a |\n|---|\n| 줄<br>바꿈 |", "<table header-row=\"true\">\n<tr>\n<td>a</td>\n</tr>\n<tr>\n<td>줄<br>바꿈</td>\n</tr>\n</table>"},
		{"foo\\\nbar", "foo\\\\\nbar"},
		{`path C:\`, `path C:\\`},
		{`\$x\$ 와 $5`, `\$x\$ 와 $5`},
		{"<https://a.com/*x*>", `[https://a.com/\*x\*](https://a.com/*x*)`},
		{"<https://a.com/x)>", "[https://a.com/x)](https://a.com/x%29)"},
	}
	for _, c := range cases {
		if got := strings.Join(Render(c[0], NotionMarkdown), ""); got != c[1] {
			t.Errorf("%q\n  got  %q\n  want %q", c[0], got, c[1])
		}
	}
}

// 노션은 강조를 줄마다 닫는다 — 러스트 쪽 notion_closes_emphasis_at_each_line.
func TestNotionClosesEmphasisAtEachLine(t *testing.T) {
	cases := [][2]string{
		{"**줄을\n넘는 굵게**와 끝", "**줄을**\n**넘는 굵게**와 끝"},
		{"> **인용\n> 안의** 굵게", "> **인용**\n> **안의** 굵게"},
		{"- **항목\n  이어짐** 끝\n- 둘", "- **항목**\n  **이어짐** 끝\n- 둘"},
		{"*기울임 **굵게\n이어짐** 끝*", "*기울임 **굵게***\n***이어짐** 끝*"},
		{"`코드\n이어짐`", "`코드`\n`이어짐`"},
		{"`a``\nb`", "``` a`` ```\n`b`"},
		{"**[링크\n이어](http://x.com)**", "**[링크**\n**이어](http://x.com)**"},
		{"**끝 \n다음**", "**끝**\n**다음**"},
	}
	for _, c := range cases {
		if got := strings.Join(Render(c[0], NotionMarkdown), ""); got != c[1] {
			t.Errorf("%q\n  got  %q\n  want %q", c[0], got, c[1])
		}
	}
}

// 노션에는 표를 <table> 로 낸다 — 러스트 쪽 notion_writes_tables_as_xml.
func TestNotionWritesTablesAsXML(t *testing.T) {
	got := strings.Join(Render("| 항목 | 비고 |\n|:--|--:|\n| **마통** | a \\| b |", NotionMarkdown), "")
	want := "<table header-row=\"true\">\n<tr>\n<td>항목</td>\n<td>비고</td>\n</tr>\n<tr>\n<td>**마통**</td>\n<td>a | b</td>\n</tr>\n</table>"
	if got != want {
		t.Fatalf("\n  got  %q\n  want %q", got, want)
	}
}

// 한도를 넘는 노션 표는 머리글을 되풀이한 표 여럿으로 나눈다 — 러스트 쪽 같은 이름의 테스트.
func TestNotionSplitsLongTablesIntoWholeTables(t *testing.T) {
	input := "| a | b |\n|---|---|\n"
	for i := 0; i < 40; i++ {
		input += fmt.Sprintf("| 행%d | 값%d 가나다라 |\n", i, i)
	}
	parts := RenderWith(input, NotionMarkdown, Options{Limit: 300}).Parts
	if len(parts) < 2 {
		t.Fatal("나뉘어야 한다")
	}
	for _, p := range parts {
		if utf8.RuneCountInString(p) > 300 || !strings.HasPrefix(p, "<table header-row=\"true\">\n<tr>\n<td>a</td>") || !strings.HasSuffix(p, "</table>") {
			t.Fatalf("온전한 표가 아니다: %q", p)
		}
	}
}

// 머리글과 함께 한도에 안 드는 행은 표 밖의 글로, 칸의 태그 모양은 탈출 — 러스트 쪽
// notion_splits_long_tables_into_whole_tables 의 뒷부분.
func TestNotionTableLongRowAndTagLikeCell(t *testing.T) {
	long := "| 머리 | 둘 |\n|---|---|\n| 짧음 | 가 |\n| " + strings.Repeat("긴칸", 200) + " | 나 |\n| 짧음2 | 다 |"
	parts := RenderWith(long, NotionMarkdown, Options{Limit: 300}).Parts
	for _, p := range parts {
		if utf8.RuneCountInString(p) > 300 || strings.Count(p, "<table ") != strings.Count(p, "</table>") {
			t.Fatalf("온전하지 않은 조각: %q", p)
		}
	}
	if !strings.Contains(strings.Join(parts, ""), "긴칸긴칸 | 나") {
		t.Fatal("긴 행이 글로 내려오지 않았다")
	}
	got := strings.Join(Render("| a |\n|---|\n| x </td> y <br> z |", NotionMarkdown), "")
	want := "<table header-row=\"true\">\n<tr>\n<td>a</td>\n</tr>\n<tr>\n<td>x \\<\\/td\\> y <br> z</td>\n</tr>\n</table>"
	if got != want {
		t.Fatalf("\n  got  %q\n  want %q", got, want)
	}
}

// 두 번째 자체 리뷰 — 러스트 쪽 notion_splits_long_tables_into_whole_tables 의 뒷부분과 같다.
func TestNotionTableCellAnglesAndHeaderKept(t *testing.T) {
	cells := strings.Join(Render("| a | b |\n|---|---|\n| i<n 일 때<br>반복 | `Option<T>` 와 `x</td>y` |", NotionMarkdown), "")
	if !strings.Contains(cells, "<td>i<n 일 때<br>반복</td>") || !strings.Contains(cells, "<td>`Option<T>` 와 `x\\<\\/td\\>y`</td>") {
		t.Fatalf("칸 탈출이 틀렸다: %q", cells)
	}
	only := "| 이름 | 설명 |\n|---|---|\n| # 제목처럼 | " + strings.Repeat("가 ", 200) + " |"
	out := strings.Join(RenderWith(only, NotionMarkdown, Options{Limit: 256}).Parts, "")
	if !strings.HasPrefix(out, "이름 | 설명\n\\# 제목처럼 | 가 가") {
		t.Fatalf("머리글이 사라졌거나 첫머리가 탈출되지 않았다: %q", out)
	}
}
