package mdwire

import "bytes"

// Render converts a complete document in one pass. Output over the limit is
// split at safe points. Split points are chosen from the document structure,
// not from the rendered output: only a point where a block has ended and no
// markup is open can be a boundary. Cutting by character count after
// conversion can leave a `<code>` open, and the channel answers with a 400.
func Render(input string, ch Channel) []string {
	return RenderWith(input, ch, Options{}).Parts
}

// MinLimit is the smallest part limit a caller can set. Each part needs room
// to close and reopen its markup. A limit smaller than a tag makes the splitter
// cut between the tag's characters (Telegram **x** split with limit 1 gives
// < · b · ></b>). Same as MIN_LIMIT in the Rust core.
const MinLimit = 256

// Options holds conversion options: the part limit and the browser channel's
// policy. The zero value is the default. There is no input-format option:
// accepting whatever an LLM writes, standard or not, is the job of the
// default reader.
type Options struct {
	// Limit is the limit for one part, in characters of rendered output.
	// Zero means the channel's Limit().
	//
	// The sender decides the limit. Plain is a fallback that does not know
	// where it goes, so when sent to Telegram it must be 4096 (a 7,153-character
	// part split at 12,000 got a 400).
	// Streaming does not split, so it ignores this value, with one exception:
	// Notion tables. A table over the limit is emitted as several tables, each
	// repeating the header row. That is the table's shape, so streaming emits
	// it the same way. The browser channel (HTML) does not split either: the
	// splitter does not close and reopen block tags, so it would cut through
	// the middle of a tag. Values below MinLimit are raised to MinLimit.
	Limit int
	// HTML is the policy for the browser channel (HTML). Other channels ignore it.
	HTML HTMLOptions
}

// HTMLOptions is the browser channel's policy. The zero value is the most
// conservative: <br> line breaks, images as links only, and only http, https
// and mailto links.
type HTMLOptions struct {
	LineBreaks LineBreaks
	Images     Images
	// Schemes lists the URL schemes accepted for links and images, without the
	// colon (as in "https"). Nil means http, https and mailto. A list replaces
	// the default; it is not added to it. An empty non-nil slice accepts no
	// scheme.
	Schemes []string
}

// LineBreaks sets how line breaks inside a block are emitted.
type LineBreaks int

const (
	// LineBreaksBR emits <br>. For text where the author's line breaks carry
	// meaning, such as chat and notes. Every other channel keeps line breaks.
	LineBreaksBR LineBreaks = iota
	// LineBreaksSpace emits only the newline character, which the browser
	// collapses into a space. For reading a document wrapped at 80 columns as
	// paragraphs.
	LineBreaksSpace
)

// Images sets how an image `![alt](url)` is emitted.
type Images int

const (
	// ImagesLink emits <a href>alt</a>. Nothing loads until the reader clicks
	// (no tracking pixels).
	ImagesLink Images = iota
	// ImagesLoad emits <img src alt>, only when the URL has an allowed scheme.
	// Otherwise it behaves like ImagesLink.
	ImagesLoad
)

// Repairs counts what normalization fixed and what was changed to fit the
// channel. The first four fields (fixes) measure how often a model breaks
// formatting. The last six (changes) measure what a channel changes, before
// you adopt it. To ask about each group separately, use Repaired and Changed.
type Repairs struct {
	// ClosedEmphasis counts emphasis still open at the end of a block, which was closed.
	ClosedEmphasis int
	// ClosedFence counts code fences still open at the end of the document, which were closed.
	ClosedFence int
	// RevertedCodeSpan counts unmatched backtick runs turned back into literal text instead of code.
	RevertedCodeSpan int
	// DroppedMarker counts unmatched `**` markers that were dropped.
	DroppedMarker int
	// EscapedChar counts characters escaped because the channel would read them
	// as syntax (GitHub's \~, \< and \*).
	EscapedChar int
	// TagEmphasis counts emphasis emitted another way because the channel cannot
	// read a marker in that position: <strong> on GitHub, a word joiner on Slack.
	TagEmphasis int
	// StrippedHTML counts raw HTML removed from the source: tags, comments, and
	// <br> turned into line breaks.
	StrippedHTML int
	// RewrittenBullet counts list bullets rewritten with a different symbol.
	RewrittenBullet int
	// RewrittenTable counts tables rewritten in a different shape from the source.
	RewrittenTable int
	// ConvertedMarker counts emphasis markers and <url|text> links rewritten in
	// another notation (markdown channels).
	ConvertedMarker int
}

// Repaired reports whether normalization fixed anything. It looks only at the
// first four fields. Repairs::any in the Rust core.
func (r Repairs) Repaired() bool {
	return r.ClosedEmphasis+r.ClosedFence+r.RevertedCodeSpan+r.DroppedMarker > 0
}

// Changed reports whether anything was fixed or changed to fit the channel.
// Repairs::changed in the Rust core.
func (r Repairs) Changed() bool { return r != Repairs{} }

func (r *Repairs) add(o Repairs) {
	r.ClosedEmphasis += o.ClosedEmphasis
	r.ClosedFence += o.ClosedFence
	r.RevertedCodeSpan += o.RevertedCodeSpan
	r.DroppedMarker += o.DroppedMarker
	r.EscapedChar += o.EscapedChar
	r.TagEmphasis += o.TagEmphasis
	r.StrippedHTML += o.StrippedHTML
	r.RewrittenBullet += o.RewrittenBullet
	r.RewrittenTable += o.RewrittenTable
	r.ConvertedMarker += o.ConvertedMarker
}

// Rendered is the result of RenderWith: the parts and the repairs.
type Rendered struct {
	Parts   []string
	Repairs Repairs
}

// RenderWith is Render with options. It also returns what normalization fixed.
func RenderWith(input string, ch Channel, o Options) Rendered {
	e := newEngine(ch, o)
	s := newPartsSink(e.v)
	e.feed(input, s)
	e.finish(s)
	return Rendered{Parts: s.intoParts(), Repairs: e.repairs()}
}

// Streamer is a streaming converter. Each chunk you push returns only the
// output that is safe to emit now. Markup cut at a chunk boundary (a chunk
// ending in `**bo`) is held back until the next chunk arrives.
type Streamer struct {
	e *engine
	// 줄바꿈이 아닌 글자를 하나라도 내보냈는가. 앞머리 빈 줄은 내보내지 않는다 — 완성본이
	// 조각 앞머리의 줄바꿈을 털고 시작하므로 스트리밍도 같아야 둘이 같은 답을 낸다.
	started bool
	// tail 은 마지막 Preview 의 꼬리다. 재사용 버퍼 — 열린 블록만큼이지 문서 전체가 아니다.
	tail []byte
	// dirty 는 tail 이 지금 상태를 반영하지 않는가다 — 미리보기를 안 했거나 그 뒤에 조각이 더 왔다.
	dirty bool
	// revised 는 Revised 의 답이다. Finish 가 정한다.
	revised bool
}

// NewStreamer creates a Streamer bound to one channel.
func NewStreamer(ch Channel) *Streamer {
	return NewStreamerWith(ch, Options{})
}

// NewStreamerWith creates a Streamer with options.
func NewStreamerWith(ch Channel, o Options) *Streamer {
	return &Streamer{e: newEngine(ch, o), dirty: true, revised: true}
}

// Repairs returns what normalization has fixed so far. After Finish it
// covers the whole document.
func (s *Streamer) Repairs() Repairs { return s.e.repairs() }

// PushTo pushes a chunk and appends the output that can be emitted now to dst.
// This is the canonical signature: it writes straight into the caller's
// buffer, so there is no allocation per chunk.
func (s *Streamer) PushTo(chunk string, dst *[]byte) {
	s.dirty = s.dirty || chunk != ""
	from := len(*dst)
	s.e.feed(chunk, bytesSink{dst})
	s.trimLeading(dst, from)
}

// Push is the convenience form of PushTo. It returns a new string.
func (s *Streamer) Push(chunk string) string {
	var b []byte
	s.PushTo(chunk, &b)
	return string(b)
}

// FinishTo marks the end of input. It appends everything left to dst,
// closing any open markup.
func (s *Streamer) FinishTo(dst *[]byte) {
	from := len(*dst)
	s.e.finish(bytesSink{dst})
	s.trimLeading(dst, from)
	s.revised = s.dirty || !bytes.Equal(s.tail, (*dst)[from:])
	// 끝난 엔진에 더 그릴 꼬리는 없다. 비워 두지 않으면 뒤이은 Preview 가 finish 꼬리를 한 번 더 낸다.
	s.tail, s.dirty = s.tail[:0], false
}

// PreviewTo appends to dst the tail that would follow the final output if
// input ended now. Streamer::preview in the Rust core. It goes in the same
// place as CloseOpenTo, but also draws what is being held back: open emphasis
// closed, a table with the rows received so far, code spans closed. It is the
// default for callers that redraw the whole accumulated output (Telegram
// editMessageText, Slack chat.update).
//
// The tail goes through the same finish path as batch rendering, so its
// syntax is always valid, but it is a guess: later chunks can change its
// shape. After Finish, Revised tells whether the result differs from the last
// preview. Cost is proportional to the size of the open block (the engine is
// cloned). Call it when you redraw the screen, not on every chunk.
func (s *Streamer) PreviewTo(dst *[]byte) {
	// 그 뒤로 조각이 안 왔으면 같은 답이다 — 다시 그리는 쪽은 조각과 무관하게도 자주 부른다.
	if s.dirty {
		s.tail = s.tail[:0]
		s.e.preview(bytesSink{&s.tail})
		if !s.started {
			// 앞을 잘라 낸 부분 슬라이스를 남기면 버퍼 앞쪽 용량을 매번 버린다 — 당겨 쓴다.
			n := len(s.tail) - len(bytes.TrimLeft(s.tail, "\n"))
			s.tail = s.tail[:copy(s.tail, s.tail[n:])]
		}
		s.dirty = false
	}
	*dst = append(*dst, s.tail...)
}

// Preview is the convenience form of PreviewTo.
func (s *Streamer) Preview() string {
	var b []byte
	s.PreviewTo(&b)
	return string(b)
}

// Revised reports whether the final output differs from the last preview.
// Check it after Finish. If false, the last screen drawn (accumulated output +
// Preview) is already the final output and needs no redraw. Telegram returns
// a 400 ("message is not modified") for an edit with identical content, so
// use this to skip the last edit. It is true if no preview was taken or more
// chunks arrived after it; true means "may differ". If you throttle edits so
// the last preview comes before the last chunk, it is true even when the
// final output is the same, so in that case compare against the string you
// sent.
func (s *Streamer) Revised() bool { return s.revised }

// Finish is the convenience form of FinishTo.
func (s *Streamer) Finish() string {
	var b []byte
	s.FinishTo(&b)
	return string(b)
}

// CloseOpenTo appends what makes the output so far safe to send as is. It
// does not touch the state, so streaming continues afterwards. Callers that
// send the accumulated output to the channel mid-stream (editing a message as
// tokens arrive) append this just before sending; do not add it to the
// accumulated output itself.
func (s *Streamer) CloseOpenTo(dst *[]byte) { s.e.closeOpen(dst) }

// CloseOpen is the convenience form of CloseOpenTo.
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
