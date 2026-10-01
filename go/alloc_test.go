package mdwire

import (
	"strings"
	"testing"
)

// 러스트 벤치 게이트와 같은 입력 — 산문·굵게·목록·인용을 12번 반복한 문서를 64바이트쯤씩 흘린다.
func syntheticProse() string {
	unit := "세 환경 중 **두 곳에서 동일한 증상**이 재현됐다. 원인은 캐시 계층이 아니라\n" +
		"**요청 경로에서 헤더를 지우는 미들웨어**였고, 이 미들웨어는 작년에 추가됐다.\n" +
		"\n" +
		"- 스테이징: 재현됨, 응답은 오지만 본문이 비어 있다\n" +
		"- 프로덕션: 재현됨, 같은 증상\n" +
		"- 로컬: 재현 안 됨 — 미들웨어가 꺼져 있다\n" +
		"\n" +
		"> 조치는 미들웨어를 되돌리는 것이 아니라 **허용 목록을 명시**하는 쪽으로 간다.\n" +
		"\n"
	return strings.Repeat(unit, 12)
}

// 글자 경계를 지키며 대략 n 바이트씩 자른다 — 러스트의 split_chunks.
func splitChunks(s string, n int) []string {
	var out []string
	for start := 0; start < len(s); {
		end := min(start+n, len(s))
		for end < len(s) && !utf8RuneStart(s[end]) {
			end++
		}
		out = append(out, s[start:end])
		start = end
	}
	return out
}

func utf8RuneStart(b byte) bool { return b&0xC0 != 0x80 }

// 회귀 게이트 — 러스트 쪽 `prose_streaming_is_allocation_free_once_warm` 의 자리다.
//
// 버퍼가 다 자란 뒤 산문을 흘리면 할당이 한 번도 일어나지 않아야 한다. 이 수가 0 이 아니게
// 되는 변경은 스트리밍 경로에 할당을 들인 것이고, 그 경로는 조각마다 불린다.
func TestProseStreamingIsAllocationFreeOnceWarm(t *testing.T) {
	pieces := splitChunks(syntheticProse(), 64)
	s := NewStreamer(TelegramHTML)
	var out []byte
	for _, p := range pieces {
		s.PushTo(p, &out)
	}
	s.FinishTo(&out)

	allocs := testing.AllocsPerRun(3, func() {
		out = out[:0]
		for _, p := range pieces {
			s.PushTo(p, &out)
		}
		s.FinishTo(&out)
	})
	if allocs != 0 {
		t.Fatalf("산문 스트리밍이 할당한다 — 회귀다: 회당 %.1f 회", allocs)
	}
}

// GitHub 만 도는 경로도 할당하지 않는다 — `~`·`<` 이스케이프, 글자로 되돌린 `~~`, 마커 대신 내는
// `<strong>`. 위 게이트의 산문에는 이 셋이 없다. 러스트 쪽 같은 이름의 테스트.
func TestGithubProseStreamingIsAllocationFreeOnceWarm(t *testing.T) {
	unit := "주행 거리는 약 ~40km 남았고 5~6월에 충전한다. `Vec<T>` 가 아니라 Vec<T> 다.\n" +
		"값은 2 ~~ 3 사이이고, 마감 전에 **설정(config)**을 바꾼다.\n\n"
	pieces := splitChunks(strings.Repeat(unit, 12), 64)
	s := NewStreamer(GithubMarkdown)
	var out []byte
	for _, p := range pieces {
		s.PushTo(p, &out)
	}
	s.FinishTo(&out)
	if !strings.Contains(string(out), `\~40km`) || !strings.Contains(string(out), "<strong>") {
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
		t.Fatalf("GitHub 산문 스트리밍이 할당한다 — 회귀다: 회당 %.1f 회", allocs)
	}
}

// 노션만 도는 경로도 할당하지 않는다 — `*`·`\` 이스케이프, 줄마다 닫는 강조(인용·목록 안 포함), 살려
// 둔 `<br>`. 러스트 쪽 같은 이름의 테스트.
func TestNotionProseStreamingIsAllocationFreeOnceWarm(t *testing.T) {
	unit := "카드 1***-001* 과 백슬래시 \\ 하나. **배포를 금요일에\n하지 않는다** 이고\n" +
		"> **인용\n> 안의** 굵게와 줄<br>바꿈.\n\n- **항목\n  이어짐** 끝, `a``\nb` 코드\n\n"
	pieces := splitChunks(strings.Repeat(unit, 12), 64)
	s := NewStreamer(NotionMarkdown)
	var out []byte
	for _, p := range pieces {
		s.PushTo(p, &out)
	}
	s.FinishTo(&out)
	if !strings.Contains(string(out), `\*\*\*`) || !strings.Contains(string(out), "금요일에**\n**하지") {
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
		t.Fatalf("노션 산문 스트리밍이 할당한다 — 회귀다: 회당 %.1f 회", allocs)
	}
}

// 브라우저 채널만 도는 경로도 할당하지 않는다 — 문단·헤딩 태그, 목록 스택, `<br>`, 원문
// 인라인 태그. 러스트 쪽 같은 이름의 테스트.
func TestHTMLProseStreamingIsAllocationFreeOnceWarm(t *testing.T) {
	unit := "## 제목\n\n세 환경 중 **두 곳에서**\n재현됐다. H<sub>2</sub>O 와 줄<br>바꿈.\n\n" +
		"- 스테이징: 재현됨\n  - 본문이 비어 있다\n- 로컬: 재현 안 됨\n\n> 조치는 **허용 목록을 명시**한다.\n\n"
	pieces := splitChunks(strings.Repeat(unit, 12), 64)
	s := NewStreamer(HTML)
	var out []byte
	for _, p := range pieces {
		s.PushTo(p, &out)
	}
	s.FinishTo(&out)
	if !strings.Contains(string(out), "<ul><li>") || !strings.Contains(string(out), "<sub>") {
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
		t.Fatalf("html 산문 스트리밍이 할당한다 — 회귀다: 회당 %.1f 회", allocs)
	}
}

// 회귀 감시용 벤치. 절대값보다 이전 회차와의 비교에 뜻이 있다 — 러스트와 나란한 순위표는
// 만들지 않는다(SPEC 9절).
func BenchmarkProseStreaming(b *testing.B) {
	pieces := splitChunks(syntheticProse(), 64)
	s := NewStreamer(TelegramHTML)
	var out []byte
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		out = out[:0]
		for _, p := range pieces {
			s.PushTo(p, &out)
		}
		s.FinishTo(&out)
	}
}

func BenchmarkRender(b *testing.B) {
	doc := syntheticProse()
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		Render(doc, TelegramHTML)
	}
}
