package mdwire

// Render 는 완성된 문서를 한 번에 변환한다. 한도를 넘으면 안전한 지점에서 나눈다 — 나누는
// 자리는 렌더 결과가 아니라 구조에서 고른다. 블록이 끝나 열린 마크업이 없는 지점만 경계가
// 된다. 변환 후에 문자 수로 자르면 `<code>` 가 열린 채 잘리고 채널은 400 을 준다.
func Render(input string, ch Channel) []string {
	e := newEngine(ch)
	s := newPartsSink(e.v)
	e.feed(input, s)
	e.finish(s)
	return s.intoParts()
}

// Streamer 는 스트리밍 변환기다. 조각을 넣으면 지금 안전하게 내보낼 수 있는 만큼만 돌려준다.
// 경계에 걸린 마크업(`**굵` 에서 끊긴 것)은 안에 남겨 두고 다음 조각을 기다린다.
type Streamer struct {
	e *engine
	// 줄바꿈이 아닌 글자를 하나라도 내보냈는가. 앞머리 빈 줄은 내보내지 않는다 — 완성본이
	// 조각 앞머리의 줄바꿈을 털고 시작하므로 스트리밍도 같아야 둘이 같은 답을 낸다.
	started bool
}

// NewStreamer 는 채널 하나에 묶인 변환기를 만든다.
func NewStreamer(ch Channel) *Streamer {
	return &Streamer{e: newEngine(ch)}
}

// PushTo 는 조각을 밀어 넣고 지금 내보낼 수 있는 출력을 dst 에 붙인다. 정본 서명 —
// 호출자 버퍼에 직접 쓰므로 조각당 할당이 없다.
func (s *Streamer) PushTo(chunk string, dst *[]byte) {
	from := len(*dst)
	s.e.feed(chunk, bytesSink{dst})
	s.trimLeading(dst, from)
}

// Push 는 PushTo 의 편의 서명이다. 새 문자열을 돌려준다.
func (s *Streamer) Push(chunk string) string {
	var b []byte
	s.PushTo(chunk, &b)
	return string(b)
}

// FinishTo: 입력이 끝났다. 남은 것을 전부 dst 에 내보낸다(열린 마크업은 닫는다).
func (s *Streamer) FinishTo(dst *[]byte) {
	from := len(*dst)
	s.e.finish(bytesSink{dst})
	s.trimLeading(dst, from)
}

// Finish 는 FinishTo 의 편의 서명이다.
func (s *Streamer) Finish() string {
	var b []byte
	s.FinishTo(&b)
	return string(b)
}

// CloseOpenTo 는 지금까지 받은 것을 그대로 보내도 되게 만든다. 상태는 건드리지 않으므로
// 붙인 뒤에도 스트리밍은 이어진다. 누적본을 중간에 채널로 보내는 쪽(토큰이 오는 대로 메시지를
// 편집하는 경우)은 보내기 직전에 이걸 덧붙인다 — 누적본 자체에는 넣지 않는다.
func (s *Streamer) CloseOpenTo(dst *[]byte) { s.e.closeOpen(dst) }

// CloseOpen 은 CloseOpenTo 의 편의 서명이다.
func (s *Streamer) CloseOpen() string {
	var b []byte
	s.CloseOpenTo(&b)
	return string(b)
}

// trimLeading 은 from 뒤에 새로 붙은 출력에서 앞머리 줄바꿈을 턴다. 첫 글자가 나올 때까지만이다.
func (s *Streamer) trimLeading(dst *[]byte, from int) {
	if s.started {
		return
	}
	b := *dst
	keep := from
	for keep < len(b) && b[keep] == '\n' {
		keep++
	}
	if keep > from {
		b = append(b[:from], b[keep:]...)
		*dst = b
	}
	if len(b) > from {
		s.started = true
	}
}
