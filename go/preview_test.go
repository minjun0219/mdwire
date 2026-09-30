package mdwire

import (
	"strings"
	"testing"
)

// 미리보기는 붙든 것을 먼저 그린다 — 러스트 쪽 preview_draws_what_push_holds 와 같은 기대값.
func TestPreviewDrawsWhatPushHolds(t *testing.T) {
	cases := []struct {
		ch         Channel
		input      string
		acc, tail  string
		tailPrefix bool
	}{
		{TelegramHTML, "앞말 **굵", "앞말 ", "<b>굵</b>", false},
		{TelegramHTML, "앞 `코드", "앞 ", "<code>코드</code>", false},
		{HTML, "| a | b |\n|---|---|\n| 1 | 2 |\n| 3", "", "<table>", true},
	}
	for _, c := range cases {
		s := NewStreamer(c.ch)
		acc := s.Push(c.input)
		tail := s.Preview()
		if acc != c.acc || (c.tailPrefix && !strings.HasPrefix(tail, c.tail)) || (!c.tailPrefix && tail != c.tail) {
			t.Errorf("%q\n  got  %q + %q\n  want %q + %q", c.input, acc, tail, c.acc, c.tail)
		}
	}
	// 미리보기는 상태를 바꾸지 않는다.
	// 닫는 백틱이 조각 끝에 붙들려 있어도, 줄을 넘어 열려 있어도 코드로 닫는다.
	for _, c := range []struct {
		ch    Channel
		input string
	}{
		{TelegramHTML, "앞 `코드`"}, {TelegramHTML, "앞 `코드` "},
		{HTML, "> a ```b\n"}, {HTML, "> a `b\n"}, {HTML, "- a `b\n"},
	} {
		s := NewStreamer(c.ch)
		got := s.Push(c.input) + s.Preview()
		if strings.Contains(got, "`") || strings.Contains(got, "<pre>") {
			t.Errorf("%q → %q", c.input, got)
		}
	}
	s := NewStreamer(TelegramHTML)
	var acc []byte
	for _, c := range []string{"앞 **굵", "게** `코", "드` 끝"} {
		s.PushTo(c, &acc)
		s.Preview()
	}
	s.FinishTo(&acc)
	if want := strings.Join(Render("앞 **굵게** `코드` 끝", TelegramHTML), ""); string(acc) != want {
		t.Fatalf("미리보기가 상태를 바꿨다\n  got  %q\n  want %q", acc, want)
	}
}

// Revised — 러스트 쪽 revised_tells_whether_the_last_preview_was_final.
func TestRevisedTellsWhetherTheLastPreviewWasFinal(t *testing.T) {
	run := func(chunks []string, previewLast bool) bool {
		s := NewStreamer(TelegramHTML)
		for _, c := range chunks {
			s.Push(c)
		}
		if previewLast {
			s.Preview()
		}
		s.Finish()
		return s.Revised()
	}
	if run([]string{"앞 **굵게** 끝"}, true) {
		t.Error("마지막 미리보기가 곧 완성본")
	}
	if !run([]string{"앞 `안 닫힌 코드"}, true) {
		t.Error("미리보기는 코드로, 완성본은 글자로")
	}
	if !run([]string{"앞 **굵게** 끝"}, false) {
		t.Error("미리보기를 안 했으면 다시 그린다")
	}
	s := NewStreamer(TelegramHTML)
	s.Push("앞")
	s.Preview()
	s.Push(" 뒤")
	s.Finish()
	if !s.Revised() {
		t.Error("미리보기 뒤에 조각이 더 왔다")
	}
	if got := s.Preview(); got != "" {
		t.Errorf("끝난 뒤에는 더 그릴 꼬리가 없다: %q", got)
	}
	s = NewStreamer(TelegramHTML)
	s.Push("앞 **굵")
	s.Finish()
	if got := s.Preview(); got != "" {
		t.Errorf("미리보기 없이 끝났어도 마찬가지: %q", got)
	}
}

// 미리보기 스냅숏은 언제 보내도 태그가 균형이 맞는다 — 러스트 쪽 preview_snapshots_are_sendable.
// 끝에서는 완성본이 일괄 렌더와 같고, 마지막 미리보기와 같을 때만 Revised 가 거짓이다.
func TestPreviewSnapshotsAreBalanced(t *testing.T) {
	for _, c := range loadCases(t) {
		for _, ch := range []Channel{TelegramHTML, HTML} {
			parts := RenderWith(c.input, ch, c.opts).Parts
			if len(parts) != 1 {
				continue
			}
			base := max(len([]rune(c.input))/150, 1)
			for _, size := range []int{base, base*3 + 2} {
				s := NewStreamerWith(ch, c.opts)
				var acc, last []byte
				fed := 0
				for _, chunk := range chunksOf(c.input, size) {
					s.PushTo(chunk, &acc)
					last = append(last[:0], acc...)
					s.PreviewTo(&last)
					// 미리보기는 지금까지 받은 입력의 일괄 렌더다 — 안 닫힌 코드 스팬만 다르다.
					prefix := RenderWith(string(fedInput(c.input, chunk, &fed)), ch, c.opts)
					if len(prefix.Parts) == 1 && prefix.Repairs.RevertedCodeSpan == 0 && string(last) != prefix.Parts[0] {
						t.Fatalf("%s · %s · 조각 %d자: 미리보기가 일괄 렌더와 다르다\n  got  %q\n  want %q", c.name, ch.Name(), size, last, prefix.Parts[0])
					}
					if !balancedHTML(string(last)) {
						t.Fatalf("%s · %s · 조각 %d자: 균형이 깨진 미리보기\n%s", c.name, ch.Name(), size, last)
					}
				}
				s.FinishTo(&acc)
				if string(acc) != parts[0] {
					t.Fatalf("%s · %s: 완성본이 일괄 렌더와 다르다", c.name, ch.Name())
				}
				if s.Revised() != (string(acc) != string(last)) {
					t.Fatalf("%s · %s: Revised 가 틀렸다", c.name, ch.Name())
				}
			}
		}
	}
}

// fedInput 은 지금까지 흘린 입력(앞 조각들 + 이번 조각)이다. fed 는 흘린 바이트 수를 들고 간다.
func fedInput(input, chunk string, fed *int) string {
	*fed += len(chunk)
	return input[:*fed]
}
