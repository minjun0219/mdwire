package mdwire

import "bytes"

// Render 는 완성된 문서를 한 번에 변환한다. 한도를 넘으면 안전한 지점에서 나눈다 — 나누는
// 자리는 렌더 결과가 아니라 구조에서 고른다. 블록이 끝나 열린 마크업이 없는 지점만 경계가
// 된다. 변환 후에 문자 수로 자르면 `<code>` 가 열린 채 잘리고 채널은 400 을 준다.
func Render(input string, ch Channel) []string {
	return RenderWith(input, ch, Options{}).Parts
}

// Dialect 는 입력 방언 — 에이전트가 무슨 표기로 썼는가다. 기본은 표준 마크다운이다. 슬랙에
// 답하는 에이전트는 흔히 레거시 mrkdwn(`*굵게*` · `_기울임_` · `~취소~`)으로 쓴다.
type Dialect int

const (
	// Markdown 은 표준 마크다운(CommonMark · GFM)이다.
	Markdown Dialect = iota
	// SlackMrkdwn 은 슬랙 레거시 mrkdwn 이다. 별표는 몇 개든 굵게, 물결은 하나든 둘이든
	// 취소선이다. 표준 표기가 섞여도 같은 뜻으로 읽는다.
	SlackMrkdwn
)

// Name 은 CLI 인자와 바인딩에서 쓰는 이름이다.
func (d Dialect) Name() string {
	if d == SlackMrkdwn {
		return "slack-mrkdwn"
	}
	return "markdown"
}

// ParseDialect 는 이름으로 방언을 찾는다.
func ParseDialect(name string) (Dialect, bool) {
	for _, d := range []Dialect{Markdown, SlackMrkdwn} {
		if d.Name() == name {
			return d, true
		}
	}
	return 0, false
}

// MinLimit 은 호출자가 줄 수 있는 가장 작은 조각 한도다. 조각마다 마크업을 닫고 다시 열 자리가 있어야
// 한다 — 한도가 태그보다 작으면 분할기가 태그 글자 사이를 가른다(텔레그램 **x** 를 한도 1 로 나누면
// < · b · ></b>). 러스트 쪽 MIN_LIMIT 과 같다.
const MinLimit = 256

// Options 는 변환 옵션이다 — 입력 방언, 조각 한도, 브라우저 채널의 정책. 영값이 기본값이다.
type Options struct {
	From Dialect
	// Limit 은 한 조각의 한도(렌더한 출력의 글자 수)다. 0 이면 채널의 Limit().
	//
	// 한도는 보내는 쪽이 정한다 — plain 은 어디로 가는지 모르는 폴백이라 텔레그램으로 보내면
	// 4096 이어야 한다(12,000 으로 나눈 7,153자 조각이 400 을 받았다).
	// 스트리밍은 나누지 않으므로 이 값을 보지 않는다. 브라우저 채널(HTML)도 나누지 않는다 — 분할기가
	// 블록 태그를 여닫지 않아 태그 한가운데서 갈린다. MinLimit 보다 작은 값은 그만큼 올린다.
	Limit int
	// HTML 은 브라우저 채널(HTML)의 정책이다. 다른 채널은 보지 않는다.
	HTML HTMLOptions
}

// HTMLOptions 는 브라우저 채널의 정책이다. 영값이 가장 보수적이다 — <br> 줄바꿈, 이미지는
// 링크로만, 링크는 http·https·mailto 만.
type HTMLOptions struct {
	LineBreaks LineBreaks
	Images     Images
	// Schemes 는 링크·이미지 주소로 받는 스킴이다("https" 처럼 콜론 없이). nil 이면 http·https·
	// mailto. 목록을 주면 그것만 받는다 — 기본값에 더하는 것이 아니다. 빈 슬라이스(nil 이 아닌)는
	// 아무 스킴도 받지 않는다.
	Schemes []string
}

// LineBreaks 는 블록 안의 줄바꿈을 어떻게 낼지다.
type LineBreaks int

const (
	// LineBreaksBR 은 <br> 이다 — 채팅·메모처럼 저자의 줄바꿈이 뜻인 글. 다른 채널이 다
	// 줄바꿈을 살린다.
	LineBreaksBR LineBreaks = iota
	// LineBreaksSpace 는 줄바꿈 글자만 낸다 — 브라우저가 공백으로 접는다. 80열로 wrap 된 문서를
	// 문단으로 읽을 때.
	LineBreaksSpace
)

// Images 는 이미지 `![alt](url)` 을 어떻게 낼지다.
type Images int

const (
	// ImagesLink 는 <a href>alt</a> 다 — 누르기 전에는 아무것도 불러오지 않는다(추적 픽셀이 없다).
	ImagesLink Images = iota
	// ImagesLoad 는 <img src alt> 다 — 주소가 허용 스킴일 때만. 아니면 ImagesLink 처럼 낸다.
	ImagesLoad
)

// Repairs 는 정규화가 고친 것의 개수다. 모델이 얼마나 자주 서식을 깨는지 재는 데 쓴다.
type Repairs struct {
	// ClosedEmphasis 는 블록이 끝나도록 안 닫혀서 닫아 준 강조다.
	ClosedEmphasis int
	// ClosedFence 는 문서 끝까지 안 닫혀서 닫아 준 코드펜스다.
	ClosedFence int
	// RevertedCodeSpan 은 짝이 없어 코드가 아니라 글자로 되돌린 백틱 런이다.
	RevertedCodeSpan int
	// DroppedMarker 는 짝 잃은 채 버린 `**` 다.
	DroppedMarker int
}

func (r *Repairs) add(o Repairs) {
	r.ClosedEmphasis += o.ClosedEmphasis
	r.ClosedFence += o.ClosedFence
	r.RevertedCodeSpan += o.RevertedCodeSpan
	r.DroppedMarker += o.DroppedMarker
}

// Rendered 는 RenderWith 의 결과 — 조각과 고친 것이다.
type Rendered struct {
	Parts   []string
	Repairs Repairs
}

// RenderWith 는 Render 에 옵션을 주고, 정규화가 고친 것도 같이 돌려준다.
func RenderWith(input string, ch Channel, o Options) Rendered {
	e := newEngine(ch, o)
	s := newPartsSink(e.v)
	e.feed(input, s)
	e.finish(s)
	return Rendered{Parts: s.intoParts(), Repairs: e.repairs()}
}

// Streamer 는 스트리밍 변환기다. 조각을 넣으면 지금 안전하게 내보낼 수 있는 만큼만 돌려준다.
// 경계에 걸린 마크업(`**굵` 에서 끊긴 것)은 안에 남겨 두고 다음 조각을 기다린다.
type Streamer struct {
	e *engine
	// 줄바꿈이 아닌 글자를 하나라도 내보냈는가. 앞머리 빈 줄은 내보내지 않는다 — 완성본이
	// 조각 앞머리의 줄바꿈을 털고 시작하므로 스트리밍도 같아야 둘이 같은 답을 낸다.
	started bool
	// tail 은 마지막 Preview 의 꼬리다. 재사용 버퍼 — 열린 블록만큼이지 문서 전체가 아니다.
	tail []byte
	// previewed 는 미리보기를 한 번이라도 했는가, dirty 는 그 뒤에 조각이 더 들어왔는가다.
	previewed, dirty bool
	// revised 는 Revised 의 답이다. Finish 가 정한다.
	revised bool
}

// NewStreamer 는 채널 하나에 묶인 변환기를 만든다.
func NewStreamer(ch Channel) *Streamer {
	return NewStreamerWith(ch, Options{})
}

// NewStreamerWith 는 옵션을 주고 만든다 — 입력 방언 따위.
func NewStreamerWith(ch Channel, o Options) *Streamer {
	return &Streamer{e: newEngine(ch, o), revised: true}
}

// Repairs 는 지금까지 정규화가 고친 것이다. Finish 뒤에 보면 문서 전체의 값이다.
func (s *Streamer) Repairs() Repairs { return s.e.repairs() }

// PushTo 는 조각을 밀어 넣고 지금 내보낼 수 있는 출력을 dst 에 붙인다. 정본 서명 —
// 호출자 버퍼에 직접 쓰므로 조각당 할당이 없다.
func (s *Streamer) PushTo(chunk string, dst *[]byte) {
	s.dirty = s.dirty || chunk != ""
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
	s.revised = !s.previewed || s.dirty || !bytes.Equal(s.tail, (*dst)[from:])
}

// PreviewTo 는 지금 입력이 끝났다면 확정분 뒤에 붙을 꼬리를 dst 에 붙인다 — 러스트 쪽
// Streamer::preview. CloseOpenTo 와 같은 자리에 들어가지만 붙들고 있던 것까지 그린다: 열린
// 강조는 닫아서, 표는 지금까지 온 행으로, 코드 스팬은 닫아서. 누적본을 통째로 다시 그리는 쪽
// (텔레그램 editMessageText, 슬랙 chat.update)의 기본값이다.
//
// 꼬리는 일괄 렌더와 같은 finish 경로라 문법은 늘 맞지만 추측이다 — 뒤의 조각이 모양을 바꿀 수
// 있다. 끝난 뒤 마지막 미리보기와 달라졌는지는 Revised 가 알려 준다. 비용은 열린 블록 크기에
// 비례한다(엔진을 복제한다). 조각마다 말고 화면을 그릴 때 부른다.
func (s *Streamer) PreviewTo(dst *[]byte) {
	s.tail = s.tail[:0]
	s.e.preview(bytesSink{&s.tail})
	if !s.started {
		s.tail = bytes.TrimLeft(s.tail, "\n")
	}
	s.previewed, s.dirty = true, false
	*dst = append(*dst, s.tail...)
}

// Preview 는 PreviewTo 의 편의 서명이다.
func (s *Streamer) Preview() string {
	var b []byte
	s.PreviewTo(&b)
	return string(b)
}

// Revised 는 완성본이 마지막 미리보기와 다른가다 — Finish 뒤에 본다. 거짓이면 마지막으로 그린
// 화면(누적본 + Preview)이 곧 완성본이라 다시 그릴 필요가 없다. 텔레그램은 같은 내용으로 편집하면
// 400("message is not modified")을 주므로 이걸 보고 마지막 편집을 건너뛴다. 미리보기를 안 했거나
// 그 뒤에 조각이 더 왔으면 참이다.
func (s *Streamer) Revised() bool { return s.revised }

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
