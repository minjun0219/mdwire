package mdwire

import (
	"math"
	"strings"
	"testing"
	"unicode/utf8"
)

// 레거시 mrkdwn 의 모양 — 러스트 쪽 render.rs 의 mrkdwn_* 와 같은 입력·같은 답이다. 슬랙에서 재지는 않았다.
func TestMrkdwnMatchesRustShapes(t *testing.T) {
	for _, c := range [][2]string{
		{"**굵게** 와 *기울임* 과 ~~취소~~ 와 `코드`", "*굵게* 와 _기울임_ 과 ~취소~ 와 `코드`"},
		{"***둘 다***", "*_둘 다_*"},
		{"a & b < c > d `x<y&z`", "a &amp; b &lt; c &gt; d `x&lt;y&amp;z`"},
		{"```rust\nlet x = a && b;\n```", "```\nlet x = a &amp;&amp; b;\n```"},
		{"`` a`b ``", "`a\u02cbb`"},
		{"짝 없는 ``` 런", "짝 없는 `\u200b`\u200b` 런"},
		{"````\n```\n````", "```\n`\u200b`\u200b`\n```"},
		{"## 배포 **목록**\n\n- 하나\n\n---", "*배포 목록*\n\n• 하나\n\n──────────"},
		{"> 인용 **굵게**", "> 인용 *굵게*"},
		{"| 이름 | 값 |\n|---|---|\n| 가 | 1 |", "```\n이름 | 값\n---- | --\n가   | 1\n```"},
		{"~예전 경로~ 와 5~6월, \\*별\\* 와 2 * 3", "\u223c예전 경로~ 와 5~6월, \u2217별* 와 2 \u2217 3"},
		{"*굵게 의도*", "_굵게 의도_"},
		{"[문서](https://a.test/x?a=1&b=2)", "<https://a.test/x?a=1&amp;b=2|문서>"},
		{"<https://a.test> 와 [https://b.test](https://b.test)", "<https://a.test> 와 <https://b.test>"},
		{"[파이프](https://a.test/a|b)", "<https://a.test/a%7Cb|파이프>"},
		{"[상대](./a.md)", "상대 (./a.md)"},
		{"![그림](https://a.test/x.png)", "<https://a.test/x.png|그림>"},
		{"**마통**이 0이고 `git`로 *진료*가", "*마통*\u200b이 0이고 `git`\u200b로 _진료_\u200b가"},
		{"값**굵게**다", "값\u200b*굵게*\u200b다"},
		{"(**괄호**) 와 **공백** 옆", "(*괄호*) 와 *공백* 옆"},
		{"**첫 줄에서 열고\n둘째 줄에서 닫힘**입니다.", "*첫 줄에서 열고*\n*둘째 줄에서 닫힘*\u200b입니다."},
		{"**굵게**<span>뒤</span>", "*굵게*\u200b뒤"},
		{"앞<span>**굵게**</span>", "앞\u200b*굵게*\u200b"},
		{"**a**__b__ 와 *a*_b_", "*a*\u200b*b* 와 _a_\u200b_b_"},
		{"_**둘 다**_", "_*둘 다*_"},
	} {
		if got := strings.Join(Render(c[0], SlackMrkdwn), ""); got != c[1] {
			t.Errorf("%q:\n  got  %q\n  want %q", c[0], got, c[1])
		}
	}
	if r := RenderWith("~예전~ 2 * 3", SlackMrkdwn, Options{}).Repairs; r.EscapedChar != 2 {
		t.Errorf("닮은 글자는 이스케이프로 센다: %+v", r)
	}
	if r := RenderWith("**마통**이", SlackMrkdwn, Options{}).Repairs; r.TagEmphasis != 1 {
		t.Errorf("U+200B 를 끼운 강조를 센다: %+v", r)
	}
}

// 한도는 section 텍스트의 3,000자다. 나눈 자리에서 열린 마커는 닫고 다음 조각에서 다시 연다. 스트리밍은
// 완성본과 같다 — 러스트 쪽 mrkdwn_splits_at_section_limit_and_streams_the_same.
func TestMrkdwnSplitsAtSectionLimitAndStreamsTheSame(t *testing.T) {
	if SlackMrkdwn.Limit() != 3000 {
		t.Fatal(SlackMrkdwn.Limit())
	}
	parts := Render("**"+strings.Repeat("가나다 ", 1500)+"가나다**이다", SlackMrkdwn)
	if len(parts) < 2 {
		t.Fatal("나뉘지 않았다")
	}
	for i, p := range parts {
		if utf8.RuneCountInString(p) > 3000 || !strings.HasPrefix(p, "*") || (i < len(parts)-1 && !strings.HasSuffix(p, "*")) {
			t.Fatalf("조각 %d: %q", i, p)
		}
	}
	for _, input := range []string{"**마통**이 [문서](https://a.test?a=1&b=2) 와 `a<b`\n\n## 제목 **굵게**", "~~a~~ ***b*** _c_"} {
		want := strings.Join(Render(input, SlackMrkdwn), "")
		rs := []rune(input)
		for size := 1; size <= len(rs); size++ {
			s := NewStreamer(SlackMrkdwn)
			var got []byte
			for i := 0; i < len(rs); i += size {
				s.PushTo(string(rs[i:min(i+size, len(rs))]), &got)
			}
			s.FinishTo(&got)
			if string(got) != want {
				t.Fatalf("%q · 조각 %d:\n  got  %q\n  want %q", input, size, got, want)
			}
		}
	}
}

// mrkdwn 만 도는 경로도 할당하지 않는다 — 줄마다 닫는 강조, 붙은 글자를 떼는 U+200B, 엔티티, 닮은 글자로
// 쓰는 글자 마커. 링크는 넣지 않았다. 러스트 쪽 같은 이름의 테스트.
func TestMrkdwnProseStreamingIsAllocationFreeOnceWarm(t *testing.T) {
	unit := "**마통**이 0이고 `git`로 본다. 약 ~40km 남았다.\n" +
		"**줄을 넘는\n굵게**는 줄마다 닫는다 & a < b.\n\n"
	pieces := splitChunks(strings.Repeat(unit, 12), 64)
	s := NewStreamer(SlackMrkdwn)
	var out []byte
	for _, p := range pieces {
		s.PushTo(p, &out)
	}
	s.FinishTo(&out)
	got := string(out)
	if !strings.Contains(got, "*마통*\u200b이") || !strings.Contains(got, "\u223c40km") || !strings.Contains(got, "*줄을 넘는*\n*굵게*") {
		t.Fatalf("재는 경로를 타지 않았다: %s", out)
	}

	allocs := testing.AllocsPerRun(3, func() {
		out = out[:0]
		for _, p := range pieces {
			s.PushTo(p, &out)
		}
		s.FinishTo(&out)
	})
	if allocs != 0 {
		t.Fatalf("mrkdwn 산문 스트리밍이 할당한다 — 회귀다: 회당 %.1f 회", allocs)
	}
}

// 큰 줄에서 mrkdwn 도 글을 그대로 둔다 — 링크·엔티티 경로가 다르다. 스트리밍은 나누지 않으니 한도 없이 낸
// 완성본과 견준다. 러스트 쪽 long_unclosed_brackets_preserve_text_in_mrkdwn.
func TestLongUnclosedBracketsPreserveTextInMrkdwn(t *testing.T) {
	for _, input := range []string{strings.Repeat("[", 100_000), strings.Repeat("[a <", 5_000), strings.Repeat("*_", 50_000)} {
		want := strings.Join(RenderWith(input, SlackMrkdwn, Options{Limit: math.MaxInt}).Parts, "")
		s := NewStreamer(SlackMrkdwn)
		var acc []byte
		for i := 0; i < len(input); i += 4096 {
			s.PushTo(input[i:min(i+4096, len(input))], &acc)
		}
		s.FinishTo(&acc)
		if string(acc) != want {
			t.Fatalf("%q…: 스트리밍이 완성본과 달라졌다", input[:8])
		}
	}
}

// 한도를 넘는 링크 글은 조각마다 > 로 닫고 <주소| 로 다시 연다 — 러스트 쪽 mrkdwn_links_survive_splits.
func TestMrkdwnLinksSurviveSplits(t *testing.T) {
	for _, label := range []string{strings.Repeat("x", 3100), strings.Repeat("가나 ", 1100)} {
		parts := Render("**["+label+"](https://a.test/a_b)** 끝", SlackMrkdwn)
		if len(parts) < 2 {
			t.Fatal("나뉘지 않았다")
		}
		for i, p := range parts {
			if utf8.RuneCountInString(p) > 3000 || !strings.HasPrefix(p, "*<https://a.test/a_b|") || strings.Count(p, "<") != strings.Count(p, ">") {
				t.Fatalf("조각 %d: %q … %q", i, p[:40], p[len(p)-20:])
			}
		}
	}
}
