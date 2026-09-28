package mdwire

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"unicode/utf8"
)

// 코퍼스가 정본이다. corpus/cases/<이름>/input.md 와 채널별 기대 출력을 Rust 코어와 똑같이
// 읽는다 — 두 구현이 같은 정본을 통과해야 한다.
func corpusDir(t *testing.T) string {
	t.Helper()
	dir := filepath.Join("..", "corpus", "cases")
	if _, err := os.Stat(dir); err != nil {
		t.Skipf("코퍼스가 없다: %v", err)
	}
	return dir
}

type corpusCase struct {
	name     string
	input    string
	expected map[string]string // 채널 이름 → 기대 출력(없는 채널은 대조하지 않는다)
}

func loadCases(t *testing.T) []corpusCase {
	t.Helper()
	dir := corpusDir(t)
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var cases []corpusCase
	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		input, err := os.ReadFile(filepath.Join(dir, e.Name(), "input.md"))
		if err != nil {
			continue
		}
		c := corpusCase{name: e.Name(), input: string(input), expected: map[string]string{}}
		for _, ch := range Channels() {
			b, err := os.ReadFile(filepath.Join(dir, e.Name(), ch.Name()+".txt"))
			if err == nil {
				c.expected[ch.Name()] = string(b)
			}
		}
		cases = append(cases, c)
	}
	if len(cases) == 0 {
		t.Fatal("케이스가 없다")
	}
	return cases
}

// normalize 는 러스트 코퍼스 러너와 같다 — 끝의 줄바꿈만 뗀다.
func normalize(s string) string { return strings.TrimRight(s, "\n") }

func TestCorpus(t *testing.T) {
	for _, c := range loadCases(t) {
		for _, ch := range Channels() {
			want, ok := c.expected[ch.Name()]
			if !ok {
				continue
			}
			got := strings.Join(Render(c.input, ch), "\x00")
			if normalize(got) != normalize(want) {
				t.Errorf("%s · %s\n  got  %q\n  want %q", c.name, ch.Name(), normalize(got), normalize(want))
			}
		}
	}
}

// 스트리밍과 완성본이 같은 답을 내야 한다. 조각 크기를 바꿔 가며 같은 입력을 흘린다 — 1글자씩
// 흘리는 것은 경계가 모든 자리에 걸린다는 뜻이라 가장 가혹하다.
func TestStreamingAgreesWithBatch(t *testing.T) {
	for _, c := range loadCases(t) {
		for _, ch := range Channels() {
			parts := Render(c.input, ch)
			// 한도를 넘겨 나뉜 케이스는 건너뛴다 — 조각은 앞머리 줄바꿈을 털고 시작하므로
			// 도로 이어 붙이면 스트리밍과 달라지는 것이 정상이다. 스트리밍은 한도를 모른다.
			if len(parts) != 1 {
				continue
			}
			for _, size := range []int{1, 2, 3, 7, 64} {
				s := NewStreamer(ch)
				var got []byte
				for _, chunk := range chunksOf(c.input, size) {
					s.PushTo(chunk, &got)
				}
				s.FinishTo(&got)
				if string(got) != parts[0] {
					t.Errorf("%s · %s · 조각 %d자: 스트리밍이 완성본과 갈렸다\n  got  %q\n  want %q", c.name, ch.Name(), size, got, parts[0])
					break
				}
			}
		}
	}
}

// Push 와 PushTo 는 같은 코드를 부른다. 서명만 다르다.
func TestOwnedAndBorrowedSignaturesAgree(t *testing.T) {
	for _, c := range loadCases(t) {
		a, b := NewStreamer(TelegramHTML), NewStreamer(TelegramHTML)
		var gotA strings.Builder
		var gotB []byte
		for _, chunk := range chunksOf(c.input, 11) {
			gotA.WriteString(a.Push(chunk))
			b.PushTo(chunk, &gotB)
		}
		gotA.WriteString(a.Finish())
		b.FinishTo(&gotB)
		if gotA.String() != string(gotB) {
			t.Errorf("%s: Push 와 PushTo 가 다르다", c.name)
		}
	}
}

// chunksOf 는 글자 경계를 지키며 n 글자씩 자른다.
func chunksOf(s string, n int) []string {
	var out []string
	for len(s) > 0 {
		end := 0
		for k := 0; k < n && end < len(s); k++ {
			_, size := utf8.DecodeRuneInString(s[end:])
			end += size
		}
		out = append(out, s[:end])
		s = s[end:]
	}
	return out
}
