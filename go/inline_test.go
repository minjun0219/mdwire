package mdwire

import (
	"os"
	"strings"
	"testing"
)

// renderLine 은 한 문단을 인라인 파서만으로 렌더한다. 블록 층이 없는 지금 단계의 시험
// 경로다 — 블록 접두사가 없는 문단은 완성본 파이프라인과 같은 답을 낸다. html 은 문단을
// 태그로 감싸고 줄바꿈 앞에 <br> 을 두므로 블록 층이 하는 그 둘만 여기서 흉내 낸다.
func renderLine(input string, ch Channel) string {
	v := newVocab(ch, Options{})
	in := newInline()
	var out []byte
	if v.isHTML() {
		out = append(out, "<p>"...)
	}
	for i, l := range strings.Split(input, "\n") {
		if i > 0 {
			out = append(out, v.lineBreak()...)
			in.noteRaw("\n")
			in.endLine()
		}
		in.render([]rune(l), &out, v)
	}
	in.finishBlock(&out, v)
	if v.isHTML() {
		out = append(out, "</p>"...)
	}
	return string(out)
}

// readCases 는 `----` 줄로 나뉜 항목들을 읽는다. 마지막 개행은 파일 끝의 것이라 뗀다.
func readCases(t *testing.T, name string) []string {
	t.Helper()
	b, err := os.ReadFile("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	return strings.Split(strings.TrimSuffix(string(b), "\n"), "\n----\n")
}

// 기대값은 Rust 코어의 출력에서 뽑았다(testdata/regen.sh) — 두 구현이 같은 답을 내는지가
// 곧 이식의 정의다. 표를 손으로 고치지 않는다.
func TestInlineMatchesRustCore(t *testing.T) {
	inputs := readCases(t, "inline-input.txt")
	for _, ch := range Channels() {
		want := readCases(t, "inline."+ch.Name()+".txt")
		if len(want) != len(inputs) {
			t.Fatalf("%s: 기대값 %d개, 입력 %d개 — regen.sh 를 다시 돌린다", ch.Name(), len(want), len(inputs))
		}
		for i, input := range inputs {
			if got := renderLine(input, ch); got != want[i] {
				t.Errorf("%s %q:\n  got  %q\n  want %q", ch.Name(), input, got, want[i])
			}
		}
	}
}

func TestWidths(t *testing.T) {
	for _, tc := range []struct {
		c rune
		w int
	}{
		{'a', 1}, {'9', 1}, {' ', 1}, {'|', 1}, {'가', 2}, {'힣', 2}, {'漢', 2}, {'あ', 2}, {'ア', 2},
		{'，', 2}, {'　', 2}, {'Ａ', 2}, {'✅', 2}, {'🚀', 2}, {'\u200b', 0}, {'\u0301', 0}, {'\n', 0},
	} {
		if got := CharWidth(tc.c); got != tc.w {
			t.Errorf("%q 의 폭이 %d 가 아니라 %d", tc.c, tc.w, got)
		}
	}
	if StrWidth("환경 env") != 2+2+1+3 {
		t.Error("혼합 폭이 더해지지 않는다")
	}
	// 이진 탐색의 전제 — 구간표가 정렬·비중첩이다. 깨지면 조용히 틀린 폭이 나온다.
	for _, table := range [][]cpRange{zeroWidth, wide} {
		for i := 1; i < len(table); i++ {
			if table[i-1].hi >= table[i].lo {
				t.Errorf("구간이 겹치거나 순서가 틀렸다: %v %v", table[i-1], table[i])
			}
		}
	}
}

func TestChannelNames(t *testing.T) {
	for _, c := range Channels() {
		got, ok := ParseChannel(c.Name())
		if !ok || got != c {
			t.Errorf("%s 를 이름으로 못 찾는다", c.Name())
		}
	}
	if _, ok := ParseChannel("없는채널"); ok {
		t.Error("모르는 이름을 받아들였다")
	}
}

// 블록 층까지 — 문서 하나를 완성본 파이프라인으로. 기대값은 같은 방식으로 Rust CLI 에서 뽑았다.
func TestBlocksMatchRustCore(t *testing.T) {
	inputs := readCases(t, "block-input.txt")
	for _, ch := range Channels() {
		want := readCases(t, "block."+ch.Name()+".txt")
		if len(want) != len(inputs) {
			t.Fatalf("%s: 기대값 %d개, 입력 %d개 — regen.sh 를 다시 돌린다", ch.Name(), len(want), len(inputs))
		}
		for i, input := range inputs {
			if got := strings.Join(Render(input, ch), "\x00"); got != want[i] {
				t.Errorf("%s %q:\n  got  %q\n  want %q", ch.Name(), input, got, want[i])
			}
		}
	}
}

// 슬랙의 조사 앞 강조 — 겹친 강조도 안쪽 마커에 조이너를 끼운다. 러스트 쪽 github_uses_tags_where_gfm_cannot_pair_markers.
func TestSlackJoinerForNestedEmphasis(t *testing.T) {
	cases := map[string]string{
		"**설정(config)**을":      "**설정(config)\u2060**을",
		"***중요(필수)***를":        "***중요(필수)\u2060*\u2060**를",
		"①***\"인용\"*** 끝":      "①**\u2060*\u2060\"인용\"*** 끝",
		"**마통**이 · **(중요)** 다": "**마통**이 · **(중요)** 다",
	}
	for in, want := range cases {
		if got := strings.Join(Render(in, SlackMarkdown), "\x00"); got != want {
			t.Errorf("%q:\n  got  %q\n  want %q", in, got, want)
		}
	}
}

// 여는 쪽이 막힌 강조 — 러스트 쪽 hemmed_opener_pairs_with_its_mirror_on_the_same_line.
func TestHemmedOpenerPairsWithItsMirror(t *testing.T) {
	cases := map[string]string{
		"값**(합계)**를 본다":                       "값<b>(합계)</b>를 본다",
		"이름이**\"홍길동\"**이다":                    "이름이<b>\"홍길동\"</b>이다",
		"2**(n-1) 은 거듭제곱":                     "2**(n-1) 은 거듭제곱",
		"x**(y)**z 와 2**(n-1)**2":             "x**(y)**z 와 2**(n-1)**2",
		"a**(b)**를 본다":                        "a<b>(b)</b>를 본다",
		"x**(y)**z 와 값**(합계)**를":              "x**(y)**z 와 값<b>(합계)</b>를",
		"마스킹 4***-****-****-003* 번호":          "마스킹 4***-****-****-003* 번호",
		"값**(합계)\n**다음** 줄":                   "값**(합계)\n<b>다음</b> 줄",
		"underfront.* (4개), minjunkim.* (3개)": "underfront.* (4개), minjunkim.* (3개)",
		"2**(n-1) (**주의**)":                   "2**(n-1) (<b>주의</b>)",
		"`값**(합계)\n)**를 끝":                    "`값**(합계)\n)**를 끝",
		"값**(합계) `x )**를\ny":                  "값<b>(합계) `x )</b>를\ny",
	}
	for in, want := range cases {
		if got := strings.Join(Render(in, TelegramHTML), "\x00"); got != want {
			t.Errorf("%q:\n  got  %q\n  want %q", in, got, want)
		}
	}
}

// 추측으로 짝지은 강조는 보고에서 센다 — 러스트 쪽과 같다.
func TestGuessedPairIsReported(t *testing.T) {
	r := RenderWith("값**(합계)**를 · 2**(n-1) · x**(y)**z", TelegramHTML, Options{}).Repairs
	if r.GuessedPair != 1 || r.DroppedMarker != 0 || !r.Repaired() {
		t.Errorf("%+v", r)
	}
}

// 슬랙은 글자 별표를 이스케이프한다 — 러스트 쪽 slack_escapes_literal_asterisks.
func TestSlackEscapesLiteralAsterisks(t *testing.T) {
	cases := map[string]string{
		"x**(y)**z 와 2**(n-1)**2":                 `x\*\*(y)\*\*z 와 2\*\*(n-1)\*\*2`,
		"underfront.* (4개), 2 ** 3":               `underfront.\* (4개), 2 \*\* 3`,
		"**굵게** 와 *기울임*":                          "**굵게** 와 *기울임*",
		"카드 4***-****-003* 번호, underfront.* (4개)": `카드 4\*\*\*-\*\*\*\*-003\* 번호, underfront.\* (4개)`,
	}
	for in, want := range cases {
		if got := strings.Join(Render(in, SlackMarkdown), "\x00"); got != want {
			t.Errorf("%q:\n  got  %q\n  want %q", in, got, want)
		}
	}
}
