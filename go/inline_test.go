package mdwire

import (
	"os"
	"strings"
	"testing"
)

// renderLine 은 한 문단을 인라인 파서만으로 렌더한다. 블록 층이 없는 지금 단계의 시험
// 경로다 — 블록 접두사가 없는 문단은 완성본 파이프라인과 같은 답을 낸다.
func renderLine(input string, ch Channel) string {
	v := vocab{channel: ch}
	in := newInline()
	var out []byte
	for i, l := range strings.Split(input, "\n") {
		if i > 0 {
			out = append(out, '\n')
			in.noteRaw("\n")
			in.endLine()
		}
		in.render([]rune(l), &out, v)
	}
	in.finishBlock(&out, v)
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
	for _, table := range [][]span{zeroWidth, wide} {
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
