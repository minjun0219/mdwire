package mdwire

import (
	"strconv"
	"strings"
	"testing"
)

// 링크 라벨의 안쪽 주소는 글자로 보존하고 <a>를 겹치지 않는다.
func TestHTMLLinkLabelsNeverNestAnchors(t *testing.T) {
	for _, c := range [][2]string{
		{"[[**cat**](https://inner.test)](https://outer.test)", "<strong>cat</strong> (https://inner.test)"},
		{"[<https://inner.test>](https://outer.test)", "https://inner.test"},
		{"[![**cat**](https://inner.test/p.png)](https://outer.test)", "<strong>cat</strong> (https://inner.test/p.png)"},
	} {
		want := `<p><a href="https://outer.test">` + c[1] + `</a></p>`
		if got := strings.Join(Render(c[0], HTML), ""); got != want {
			t.Fatalf("%q: got %q, want %q", c[0], got, want)
		}
		s := NewStreamer(HTML)
		var acc []byte
		for _, r := range c[0] {
			s.PushTo(string(r), &acc)
		}
		s.FinishTo(&acc)
		if string(acc) != want {
			t.Fatal(string(acc))
		}
		if got := strings.Join(Render(c[0], TelegramHTML), ""); strings.Count(got, "<a ") != 1 {
			t.Fatal(got)
		}
	}
	o := Options{HTML: HTMLOptions{Images: ImagesLoad}}
	got := strings.Join(RenderWith("[![**cat**](https://inner.test/p.png)](https://outer.test)", HTML, o).Parts, "")
	if got != `<p><a href="https://outer.test"><img src="https://inner.test/p.png" alt="cat"></a></p>` {
		t.Fatal(got)
	}
}

// 바깥 링크가 <a>가 되지 못하면 안쪽 링크는 산다 — 러스트 inner_links_survive_when_the_outer_link_is_not_an_anchor.
func TestInnerLinksSurviveWhenTheOuterLinkIsNotAnAnchor(t *testing.T) {
	for _, c := range [][2]string{
		{"[[docs](https://ok.test)](javascript:x)", `<p><a href="https://ok.test">docs</a> (javascript:x)</p>`},
		{"[<https://in.test>](javascript:x)", `<p><a href="https://in.test">https://in.test</a> (javascript:x)</p>`},
	} {
		if got := strings.Join(Render(c[0], HTML), ""); got != c[1] {
			t.Fatalf("%q: got %q, want %q", c[0], got, c[1])
		}
	}
	long := "https://e.test/" + strings.Repeat("x", 5000)
	got := strings.Join(Render("[[docs](https://ok.test)]("+long+")", TelegramHTML), "")
	if !strings.HasPrefix(got, `<a href="https://ok.test">docs</a> (`) {
		t.Fatal(got[:80])
	}
}

// 표 칸 안 링크 라벨의 <br>은 줄을 바꾸지 않는다.
func TestLinkLabelsInCellsKeepTheRow(t *testing.T) {
	input := "| a | b |\n|---|---|\n| [x<br>y](https://e.test) | z |"
	if got := strings.Join(Render(input, SlackMarkdown), ""); !strings.HasSuffix(got, "| [x y](https://e.test) | z |") {
		t.Fatal(got)
	}
	if got := strings.Join(Render(input, Plain), ""); !strings.HasSuffix(got, "x y (https://e.test) | z") {
		t.Fatal(got)
	}
}

// 큰 대괄호 줄은 내용을 보존하고 완성본·스트리밍이 같은 답을 낸다.
func TestLongUnclosedBracketsPreserveText(t *testing.T) {
	input := strings.Repeat("[", 100_000)
	want := "<p>" + input + "</p>"
	if got := strings.Join(Render(input, HTML), ""); got != want {
		t.Fatal("대괄호 내용이 달라졌다")
	}
	s := NewStreamer(HTML)
	var acc []byte
	for i := 0; i < len(input); i += 4096 {
		s.PushTo(input[i:min(i+4096, len(input))], &acc)
	}
	s.FinishTo(&acc)
	if string(acc) != want {
		t.Fatal("스트리밍이 완성본과 달라졌다")
	}
	if got := strings.Join(Render("[[broken [**ok**](https://e.test)", HTML), ""); got != `<p>[[broken <a href="https://e.test"><strong>ok</strong></a></p>` {
		t.Fatal(got)
	}
}

// 라벨 재귀는 제한하지만 한도 뒤의 내용과 HTML escape는 보존한다.
func TestDeeplyNestedTargetsDoNotExhaustTheStack(t *testing.T) {
	input := strings.Repeat("[", 10_000) + "x <script> &" + strings.Repeat("](https://e.test)", 10_000)
	got := strings.Join(Render(input, HTML), "")
	if !strings.Contains(got, "x &lt;script&gt; &amp;") || !strings.HasSuffix(got, "</a></p>") {
		t.Fatal("내용이나 escape가 빠졌다")
	}
	s := NewStreamer(HTML)
	var acc []byte
	for i := 0; i < len(input); i += 4096 {
		s.PushTo(input[i:min(i+4096, len(input))], &acc)
	}
	s.FinishTo(&acc)
	if string(acc) != got {
		t.Fatal("스트리밍이 완성본과 달라졌다")
	}
}

// 반복 탐색이 돌아오면 입력 크기에 따른 비용 증가가 보이도록 둔다.
func BenchmarkUnclosedBrackets(b *testing.B) {
	for _, n := range []int{10_000, 20_000, 40_000} {
		b.Run(strconv.Itoa(n), func(b *testing.B) {
			input := strings.Repeat("[", n)
			b.ResetTimer()
			for i := 0; i < b.N; i++ {
				Render(input, HTML)
			}
		})
	}
}
