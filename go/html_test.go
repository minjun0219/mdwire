package mdwire

import (
	"strings"
	"testing"
)

// innerHTML 로 들어가는 출력이다 — 러스트 쪽 html_output_is_safe_for_inner_html 과 같은 단언.
// 글자는 escape 하고, javascript: 링크는 글로 떨어뜨리고, 원문 태그는 속성을 버린 이름만 살린다.
func TestHTMLOutputIsSafeForInnerHTML(t *testing.T) {
	cases := [][2]string{
		{"1 < 2 & `a<b>`", "<p>1 &lt; 2 &amp; <code>a&lt;b&gt;</code></p>"},
		{"[나쁨](javascript:alert) [좋음](https://a.com)", `<p>나쁨 (javascript:alert) <a href="https://a.com">좋음</a></p>`},
		{"[메일](MAILTO:a@b.c)", `<p><a href="MAILTO:a@b.c">메일</a></p>`},
		{"[a](  JaVaScRiPt:x)", "<p>a (  JaVaScRiPt:x)</p>"},
		{`H<sub onclick="x()">2</sub>O`, "<p>H<sub>2</sub>O</p>"},
		{"<div>블록</div> <script>x</script>", "<p>블록 &lt;script&gt;x&lt;/script&gt;</p>"},
		{"**a<sub>b**c</sub>", "<p><strong>a<sub>b</sub></strong>c</p>"},
		{"```x\"onmouseover=\"alert(1)\ncode\n```", `<pre><code class="language-x&quot;onmouseover=&quot;alert(1)">code</code></pre>`},
	}
	for _, c := range cases {
		if got := strings.Join(Render(c[0], HTML), ""); got != c[1] {
			t.Errorf("%q\n  got  %q\n  want %q", c[0], got, c[1])
		}
	}
}

// 빈 줄로 띄운 목록(loose list)도 한 목록이다 — 러스트 쪽 html_nests_lists_by_indent.
func TestHTMLLooseListKeepsNesting(t *testing.T) {
	loose := strings.Join(Render("- a\n\n  - b\n\n- c", HTML), "")
	if !strings.HasPrefix(loose, "<ul><li>a") || !strings.Contains(loose, "<ul><li>b") ||
		!strings.HasSuffix(loose, "</li><li>c</li></ul>") || strings.Count(loose, "<ul>") != 2 {
		t.Fatalf("중첩이 빠졌다: %q", loose)
	}
	if got := strings.Join(Render("- a\n\n문단", HTML), ""); got != "<ul><li>a</li></ul>\n\n<p>문단</p>" {
		t.Fatalf("목록 뒤 문단: %q", got)
	}
}

// 스트리밍 누적본에 CloseOpen 을 붙이면 언제나 균형 잡힌 HTML 이다 — 러스트 쪽
// html_streaming_snapshot_is_always_balanced. 한 글자씩 흘리며 매번 잰다.
func TestHTMLStreamingSnapshotIsAlwaysBalanced(t *testing.T) {
	input := "## 제목\n\n문단 **굵게** 와 `코드`\n\n- 하나\n  - 둘 <sub>x</sub>\n- 셋\n\n> 인용\n\n```\nfence\n```\n| a | b |\n|---|---|\n| 1 | 2 |\n"
	s := NewStreamer(HTML)
	var acc []byte
	for _, r := range input {
		s.PushTo(string(r), &acc)
		snap := append([]byte(nil), acc...)
		s.CloseOpenTo(&snap)
		if !balancedHTML(string(snap)) {
			t.Fatalf("균형이 깨진 스냅숏: %s", snap)
		}
	}
	s.FinishTo(&acc)
	if want := strings.Join(Render(input, HTML), ""); string(acc) != want {
		t.Fatalf("스트리밍이 완성본과 다르다\n  got  %q\n  want %q", acc, want)
	}
}

func balancedHTML(s string) bool {
	var stack []string
	for {
		at := strings.IndexByte(s, '<')
		if at < 0 {
			return len(stack) == 0
		}
		end := strings.IndexByte(s[at:], '>')
		if end < 0 {
			return false
		}
		tag := s[at+1 : at+end]
		s = s[at+end+1:]
		name := strings.TrimPrefix(tag, "/")
		if i := strings.IndexFunc(name, func(r rune) bool { return !(r >= 'a' && r <= 'z' || r >= '0' && r <= '9') }); i >= 0 {
			name = name[:i]
		}
		if name == "br" || name == "hr" {
			continue
		}
		if strings.HasPrefix(tag, "/") {
			if len(stack) == 0 || stack[len(stack)-1] != name {
				return false
			}
			stack = stack[:len(stack)-1]
		} else {
			stack = append(stack, name)
		}
	}
}

// 한도는 호출자가 정한다 — 러스트 쪽 caller_limit_overrides_the_channel_limit.
func TestCallerLimitOverridesTheChannelLimit(t *testing.T) {
	long := strings.Repeat("가나다 ", 3000)
	if n := len(Render(long, Plain)); n != 1 {
		t.Fatalf("plain 기본 한도는 12,000 이다: %d 조각", n)
	}
	parts := RenderWith(long, Plain, Options{Limit: 4096}).Parts
	if len(parts) < 2 {
		t.Fatalf("나뉘어야 한다")
	}
	for _, p := range parts {
		if n := len([]rune(p)); n > 4096 {
			t.Fatalf("%d자 조각 — 한도 4096", n)
		}
	}
	input := "[문서](https://a.com/" + strings.Repeat("x", 300) + ")"
	if out := strings.Join(RenderWith(input, TelegramHTML, Options{Limit: MinLimit}).Parts, ""); strings.Contains(out, "<a href") {
		t.Fatalf("한도 256 에 300자 주소 링크: %s", out)
	}
}
