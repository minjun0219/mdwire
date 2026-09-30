package mdwire

import (
	"slices"
	"strconv"
	"strings"
	"unicode"
)

// 블록 스캐너 — 줄 단위로 읽고, 줄 단위로 내보낸다.
//
// 블록의 종류는 그 줄 하나로 정해지고(표만 한 줄을 더 본다), 정해지는 순간 그 줄을 열어
// 뒤따라 오는 글자를 흘려보낸다. 붙드는 것은 셋뿐이다 — 판정이 덜 된 접두사, 줄 끝에 걸친
// 마커·공백, 줄 전체가 필요한 것들(펜스 info · 표 후보 · 표 본문).

type blockState int

const (
	stateNone blockState = iota // 블록 밖. 여기서만 표가 시작될 수 있다.
	statePara
	stateHeading // 한 줄짜리 블록. 줄이 끝나면 바로 닫힌다.
	stateList
	stateQuote
	stateFence
	stateTable
)

// sink 는 엔진이 출력을 붙이는 곳이다. boundary 는 "지금까지 내보낸 것에 열린 마크업이
// 없다 — 여기가 조각 경계가 될 수 있다"는 신호다.
type sink interface {
	text(b []byte)
	boundary()
}

// lineKind 는 접두사만으로 정해지는 줄의 종류다.
type lineKind struct {
	k      lineType
	level  int // 헤딩 레벨
	indent int // 목록 들여쓰기
	n      int // 번호 목록의 번호
}

type lineType int

const (
	linePara lineType = iota
	lineHeading
	lineQuote
	lineBullet
	lineOrdered
)

// wholeKind 는 줄 전체를 봐야 정해지는 것들이다.
type wholeKind int

const (
	wholeBlank wholeKind = iota
	wholeRule
	wholeFence          // info 문자열이 줄 끝까지 이어진다
	wholeTableCandidate // 구분선이 따라오는지 다음 줄을 봐야 한다
)

type decisionKind int

const (
	needMore decisionKind = iota // 아직 못 정한다. 글자가 더 와야 한다.
	decidePrefix
	decideWhole
)

type decision struct {
	kind   decisionKind
	line   lineKind
	prefix int // 접두사의 길이(글자 수)
	whole  wholeKind
}

type fenceState struct {
	ch   rune
	n    int
	info string
	body bool // 내용을 한 줄이라도 썼는가. 여는 마크업 바로 뒤의 줄바꿈을 정하려고 본다.
}

type engine struct {
	v      vocab
	inline *inline
	// 현재 줄에서 아직 파싱하지 않은 부분.
	pending []rune
	// 이 줄의 접두사가 정해져 블록이 열렸는가.
	lineOpen bool
	kind     lineKind
	out      []byte
	state    blockState
	// 앞 헤딩의 레벨. 두 단계 이상 깊어지는 것을 막는다(SPEC 8절).
	heading int
	// 한 줄이라도 내보냈는가. 앞머리 빈 줄을 만들지 않으려고 본다.
	wrote bool
	// 빈 줄이 하나 밀려 있는가. 여러 개가 와도 하나로 접는다(정돈).
	blank   bool
	held    string
	hasHeld bool
	// hold 는 붙들어 둔 `[`·`<!--` 를 어디까지 훑었나다. safeCut 이 조각마다 처음부터 다시
	// 훑지 않게 들고 있는다. 열린 줄 안에서만 뜻이 있다.
	hold  holdMemo
	table table
	fence fenceState
	// 조각이 `\r` 로 끝났다. 다음 조각이 `\n` 으로 시작하면 CRLF 라 버리고, 아니면 글자다.
	// `\r` 를 줄에 먼저 넣으면 `---\r` 가 구분선이 아니라 문단이 된다 — 완성본은 `\r\n` 을 한
	// 번에 봐서 안 갈린다. 스트리밍이 같은 답을 내려면 다음 글자를 볼 때까지 들고 있어야 한다.
	cr bool
	// 문서 끝까지 안 닫혀서 닫아 준 코드펜스 수.
	closedFence int
	// 입력 방언. 표 셀을 읽을 때도 문서를 따른다.
	dialect Dialect
	// 열린 목록들 — 바깥부터. HTML 채널만 쓴다 — 다른 채널은 목록을 글자(`- `·`• `)로 그려서
	// 중첩을 태그로 여닫을 일이 없다. 블록이 닫혀도 슬라이스는 재사용한다.
	lists []listLevel
	// listGap 은 목록 안에서 빈 줄을 만났고, 다음 줄이 항목인지 아직 모른다는 뜻이다(HTML).
	// 항목이면 같은 목록을 잇고, 아니면 그때 닫는다.
	listGap bool
}

// listLevel 은 열린 목록 하나다 — 항목 들여쓰기와 번호 목록인가.
type listLevel struct {
	indent  int
	ordered bool
}

func newEngine(ch Channel, o Options) *engine {
	return &engine{v: newVocab(ch, o), inline: newInline(o.From), dialect: o.From}
}

// repairs 는 지금까지 정규화가 고친 것이다.
func (e *engine) repairs() Repairs {
	r := e.inline.repairs
	r.ClosedFence += e.closedFence
	return r
}

func (e *engine) feed(chunk string, s sink) {
	if chunk == "" {
		return
	}
	if e.cr {
		e.cr = false
		if chunk[0] != '\n' {
			e.pending = append(e.pending, '\r')
		}
	}
	for len(chunk) > 0 {
		nl := strings.IndexByte(chunk, '\n')
		if nl < 0 {
			if strings.HasSuffix(chunk, "\r") {
				e.cr = true
				chunk = chunk[:len(chunk)-1]
			}
			e.pending = appendRunes(e.pending, chunk)
			e.progress(false, s)
			return
		}
		rest := chunk[:nl]
		// CRLF 입력. `\r` 를 남겨 보내면 채널에 그대로 박힌다.
		rest = strings.TrimSuffix(rest, "\r")
		e.pending = appendRunes(e.pending, rest)
		e.progress(true, s)
		chunk = chunk[nl+1:]
	}
}

// preview 는 지금 입력이 끝났다면 나올 꼬리를 s 에 쓴다 — 러스트 쪽 Engine::preview. 자기 상태는
// 건드리지 않고 복제본에 finish 를 부른다. 다른 점은 하나 — 안 닫힌 코드 스팬을 글자로 되돌리지
// 않고 닫는다(inline.preview).
func (e *engine) preview(s sink) {
	c := e.clone()
	c.inline.preview = true
	c.finish(s)
}

// clone 은 깊은 복사다. 슬라이스를 나눠 쓰면 복제본의 finish 가 원본의 버퍼를 덮어쓴다.
func (e *engine) clone() *engine {
	c := *e
	in := *e.inline
	in.open = slices.Clone(in.open)
	in.scratch = slices.Clone(in.scratch)
	in.codeSrc = slices.Clone(in.codeSrc)
	in.strippedTags = slices.Clone(in.strippedTags)
	c.inline = &in
	c.pending = slices.Clone(e.pending)
	c.out = slices.Clone(e.out)
	c.lists = slices.Clone(e.lists)
	c.table.align = slices.Clone(e.table.align)
	c.table.rows = make([][]string, len(e.table.rows))
	for i, r := range e.table.rows {
		c.table.rows[i] = slices.Clone(r)
	}
	return &c
}

func (e *engine) finish(s sink) {
	// 문서가 `\r` 로 끝났다 — 뒤에 `\n` 이 없으니 글자다.
	if e.cr {
		e.cr = false
		e.pending = append(e.pending, '\r')
	}
	if len(e.pending) > 0 || e.lineOpen {
		e.progress(true, s)
	}
	if e.hasHeld {
		held := e.held
		e.held, e.hasHeld = "", false
		e.wholePara(held, s)
	}
	if e.state == stateFence {
		e.closedFence++
	}
	e.closeBlock(s)
	e.flushAll(s)
	s.boundary()
}

// progress 는 지금까지 받은 것으로 갈 수 있는 데까지 간다. eol 이면 이 줄은 여기서 끝난다 —
// 미뤄 둔 판단을 전부 확정해야 한다.
func (e *engine) progress(eol bool, s sink) {
	// 줄 전체를 봐야 하는 상태들. 여기서는 스트리밍하지 않는다.
	if e.state == stateFence || e.state == stateTable || e.hasHeld {
		if !eol {
			return
		}
		if e.consumeWholeLine(s) {
			return
		}
		// 상태가 풀렸다. 이 줄은 아래의 일반 경로로 간다.
	}

	if !e.lineOpen {
		// 새 줄이다. 앞 줄에서 훑던 자리는 이 줄과 상관없다.
		e.hold = holdMemo{}
		// 표는 `|` 로 시작하는 줄에서만 시작한다. 붙드는 값이 그 줄 하나뿐이라 그렇다.
		d := classify(e.pending, eol, true)
		switch d.kind {
		case needMore:
			return
		case decideWhole:
			e.whole(d.whole, s)
			e.pending = e.pending[:0]
			return
		case decidePrefix:
			e.openLine(d.line, d.prefix, s)
		}
	}

	// 접두사 뒤부터는 인라인이다. 끝에 걸친 마커·공백만 남기고 흘려보낸다.
	var cut int
	if eol {
		cut = trimEnd(e.pending)
	} else {
		cut = safeCut(e.pending, &e.hold)
	}
	if cut > 0 {
		e.inline.render(e.pending[:cut], &e.out, e.v)
		e.pending = append(e.pending[:0], e.pending[cut:]...)
		e.hold.shift(cut)
	}
	if eol {
		e.pending = e.pending[:0]
		e.endLine(s)
	}
	e.flushSafe(s)
}

// consumeWholeLine 은 줄 전체가 필요한 상태를 소비한다. 이 줄을 다 썼으면 true.
func (e *engine) consumeWholeLine(s sink) bool {
	// 1. 코드펜스 안은 전부 내용이다. 빈 줄도 마커도 글자다.
	if e.state == stateFence {
		line := e.takeLine()
		ch, n, _, ok := fenceMarker(strings.TrimLeftFunc(line, unicode.IsSpace))
		if ok && ch == e.fence.ch && n >= e.fence.n {
			e.closeBlock(s)
		} else {
			e.fenceBody(line)
			e.flushAll(s)
		}
		return true
	}

	// 2. 표 후보를 들고 있었다면 이 줄이 구분선인지로 판가름난다.
	if e.hasHeld {
		held := e.held
		e.held, e.hasHeld = "", false
		line := e.takeLine()
		if isDelimiterRow(line) && e.table.begin(held, line) {
			// 앞 블록이 인용문이었을 수 있다. 표를 시작하기 전에 닫는다.
			e.closeBlock(s)
			e.state = stateTable
			return true
		}
		// 표가 아니었다. 들고 있던 줄을 문단으로 내보내고, 이 줄은 아래로 흘린다.
		e.pending = append(e.pending[:0], []rune(line)...)
		e.wholePara(held, s)
		return false
	}

	// 3. 표 본문.
	if e.state == stateTable {
		line := e.takeLine()
		row := strings.TrimSpace(line) != "" && strings.ContainsRune(line, '|')
		if row {
			e.table.push(line)
		} else {
			e.pending = append(e.pending[:0], []rune(line)...)
			e.closeBlock(s)
		}
		return row
	}
	return false
}

// takeLine 은 현재 줄을 문자열로 꺼내고 pending 을 비운다.
func (e *engine) takeLine() string {
	line := string(e.pending)
	e.pending = e.pending[:0]
	return line
}

// whole 은 줄 전체로 판정되는 블록들이다.
func (e *engine) whole(k wholeKind, s sink) {
	switch k {
	case wholeBlank:
		// 빈 줄로 띄운 목록(loose list)은 한 목록이다(HTML). 여기서 닫으면 목록 스택이 비어,
		// 빈 줄 뒤 들여쓴 항목이 새 최상위 목록으로 열려 중첩이 사라진다(리뷰에서 나왔다). 항목의
		// 인라인만 확정하고 다음 줄을 기다린다. 다른 채널은 들여쓰기가 글자로 남아 중첩이 산다.
		if e.v.isHTML() && e.state == stateList {
			e.inline.finishBlock(&e.out, e.v)
			e.listGap = true
			e.blank = e.wrote
			e.flushAll(s)
			return
		}
		e.closeBlock(s)
		e.blank = e.wrote
	case wholeRule:
		e.closeBlock(s)
		e.startLine()
		e.out = append(e.out, e.v.rule()...)
		e.closeBlock(s)
	case wholeFence:
		line := e.takeLine()
		ch, n, info, _ := fenceMarker(strings.TrimLeftFunc(line, unicode.IsSpace))
		e.closeBlock(s)
		e.startLine()
		e.fence = fenceState{ch: ch, n: n, info: info}
		e.state = stateFence
		e.v.verbatimOpen(info, &e.out)
		e.flushAll(s)
	case wholeTableCandidate:
		// 구분선이 따라오는지 한 줄만 기다린다.
		e.held, e.hasHeld = string(e.pending), true
	}
}

// openLine: 접두사가 정해졌다. 블록을 열고 접두사를 내보낸다.
func (e *engine) openLine(k lineKind, prefix int, s sink) {
	// 빈 줄 뒤에 항목이 아닌 것이 왔다 — 목록이 끝났다.
	if e.listGap {
		e.listGap = false
		if k.k == linePara || k.k == lineHeading || k.k == lineQuote {
			e.closeBlock(s)
		}
	}
	switch {
	case k.k == linePara && e.state == stateList:
		// 리스트 항목이 다음 줄로 이어진다. 항목은 아직 끝나지 않았다 — 여기서 끊으면 줄을
		// 넘는 강조가 항목 안에서만 안 잡힌다. 80열 wrap 은 불릿 안에서도 똑같이 일어난다.
		e.out = append(e.out, e.v.lineBreak()...)
		e.inline.noteRaw(e.v.lineBreak())
		e.inline.endLine()
		for i := 0; i < min(prefix, 8); i++ {
			e.out = append(e.out, ' ')
			e.inline.noteRaw(" ")
		}
	case k.k == linePara:
		if e.state != statePara {
			e.closeBlock(s)
			e.state = statePara
			e.startLine()
			if e.v.isHTML() {
				e.out = append(e.out, "<p>"...)
			}
		} else {
			// 문단 안의 줄바꿈은 살린다. 강조는 이 줄바꿈을 넘어 이어진다 — 80열 wrap 된
			// 산문에서 그게 일상이고, 그것이 이 라이브러리의 첫 고장이었다.
			e.out = append(e.out, e.v.lineBreak()...)
			e.inline.noteRaw(e.v.lineBreak())
			e.inline.endLine()
		}
	case k.k == lineHeading:
		e.closeBlock(s)
		e.state = stateHeading
		e.startLine()
		e.openHeading(k.level)
	case k.k == lineQuote:
		if e.state != stateQuote {
			e.closeBlock(s)
			e.state = stateQuote
			e.startLine()
			e.out = append(e.out, e.v.quoteOpen()...)
		} else {
			e.out = append(e.out, e.v.lineBreak()...)
			e.inline.noteRaw(e.v.lineBreak())
			e.inline.endLine()
		}
		e.out = append(e.out, e.v.quotePrefix()...)
		e.inline.noteRaw(e.v.quotePrefix())
		e.inline.setPrev(noChar)
	default: // 불릿 · 번호
		if e.state != stateList {
			e.closeBlock(s)
			e.state = stateList
		} else {
			// 같은 리스트의 다음 항목. 여기서 앞 항목의 인라인을 확정한다 — 강조는 항목을 넘지 않는다.
			e.inline.finishBlock(&e.out, e.v)
		}
		e.startLine()
		if e.v.isHTML() {
			e.openItem(k.indent, k.k == lineOrdered, k.n)
		} else {
			for i := 0; i < min(k.indent, 8); i++ {
				e.out = append(e.out, ' ')
			}
			if k.k == lineOrdered {
				e.out = strconv.AppendInt(e.out, int64(k.n), 10)
				e.out = append(e.out, ". "...)
			} else {
				e.out = append(e.out, e.v.bullet()...)
			}
		}
		e.inline.setPrev(noChar)
	}
	e.pending = append(e.pending[:0], e.pending[prefix:]...)
	e.kind = k
	e.lineOpen = true
}

// endLine: 줄이 끝났다.
func (e *engine) endLine(s sink) {
	if e.kind.k == lineHeading {
		// 헤딩은 한 줄짜리 블록이다.
		e.closeBlock(s)
	} else {
		// 항목·문단·인용은 줄 하나로 끝나지 않는다. 강조는 열어 둔다.
		e.inline.endLine()
	}
	e.lineOpen = false
}

// wholePara 는 이미 완성된 줄 하나를 문단으로 내보낸다. 표가 아니었던 후보 줄이 여기로 온다.
func (e *engine) wholePara(line string, s sink) {
	saved := e.pending
	e.pending = []rune(line)
	e.lineOpen = false
	d := classify(e.pending, true, false)
	if d.kind == decidePrefix {
		e.openLine(d.line, d.prefix, s)
		if cut := trimEnd(e.pending); cut > 0 {
			e.inline.render(e.pending[:cut], &e.out, e.v)
		}
		e.endLine(s)
	} else {
		e.whole(wholeBlank, s)
	}
	e.pending = saved
	e.flushSafe(s)
}

// openItem 은 목록 항목 하나를 태그로 연다(HTML). 들여쓰기로 중첩을 가른다 — 더 깊으면 지금
// 항목 안에 목록을 새로 열고, 얕으면 그만큼 닫고, 같으면 항목만 바꾼다. 여는 태그는 붙들지
// 않는다 — 목록이 끝날 때까지 기다리면 스트리밍이 아니다. 닫는 것은 closeOpen.
func (e *engine) openItem(indent int, ordered bool, n int) {
	for len(e.lists) > 0 && e.lists[len(e.lists)-1].indent > indent {
		e.out = append(e.out, closeList(e.lists[len(e.lists)-1].ordered)...)
		e.lists = e.lists[:len(e.lists)-1]
	}
	switch last := len(e.lists) - 1; {
	case last >= 0 && e.lists[last].indent == indent && e.lists[last].ordered == ordered:
		// 같은 깊이, 같은 종류 — 항목만 바꾼다.
		e.out = append(e.out, "</li>"...)
	case last >= 0 && e.lists[last].indent == indent:
		// 같은 깊이인데 종류가 바뀌었다 — 목록을 갈아 낀다.
		e.out = append(e.out, closeList(e.lists[last].ordered)...)
		e.lists = e.lists[:last]
		e.openList(indent, ordered, n)
	default:
		e.openList(indent, ordered, n)
	}
	e.out = append(e.out, "<li>"...)
}

func (e *engine) openList(indent int, ordered bool, n int) {
	switch {
	case ordered && n != 1:
		// 1 이 아닌 번호로 시작하면 번호를 이어 간다 — 빈 줄로 끊긴 번호 목록이 그렇다.
		e.out = append(e.out, `<ol start="`...)
		e.out = strconv.AppendInt(e.out, int64(n), 10)
		e.out = append(e.out, `">`...)
	case ordered:
		e.out = append(e.out, "<ol>"...)
	default:
		e.out = append(e.out, "<ul>"...)
	}
	e.lists = append(e.lists, listLevel{indent: indent, ordered: ordered})
}

// closeList 는 목록 하나를 항목째 닫는 마크업이다.
func closeList(ordered bool) string {
	if ordered {
		return "</li></ol>"
	}
	return "</li></ul>"
}

func (e *engine) openHeading(level int) {
	if e.heading != 0 {
		level = min(level, e.heading+1)
	}
	e.heading = level
	if e.v.isHTML() {
		e.out = append(e.out, "<h"...)
		e.out = strconv.AppendInt(e.out, int64(min(level, e.v.maxHeading())), 10)
		e.out = append(e.out, '>')
	} else if max := e.v.maxHeading(); max > 0 {
		for i := 0; i < min(level, max); i++ {
			e.out = append(e.out, '#')
		}
		e.out = append(e.out, ' ')
	} else if !e.v.isPlain() {
		// 헤딩 구문이 없는 채널. 줄 전체를 굵게 내보낸다.
		e.out = append(e.out, e.v.open(emphBold)...)
	}
	e.inline.setPrev(noChar)
}

func (e *engine) fenceBody(line string) {
	if e.fence.body || e.v.verbatimBodyNewline() {
		e.out = append(e.out, '\n')
	}
	e.fence.body = true
	e.v.escape(line, &e.out)
}

// startLine 은 줄 하나를 시작한다. 블록 사이 빈 줄은 하나로 접는다(정돈).
func (e *engine) startLine() {
	if e.wrote {
		e.out = append(e.out, '\n')
		if e.blank {
			e.out = append(e.out, '\n')
		}
	}
	e.blank = false
	e.wrote = true
}

// closeOpen 은 지금까지 내보낸 것에 열려 있는 블록 마크업을 닫아 붙인다. 상태는 건드리지 않는다.
// 누적본을 중간에 그대로 채널로 보내면 열린 태그가 남는다 — 보내기 직전에 이걸 덧붙이면 된다.
func (e *engine) closeOpen(out *[]byte) { e.blockCloseMarkup(out) }

// blockCloseMarkup 은 블록의 닫는 마크업이다. 인라인 정리는 하지 않는다.
func (e *engine) blockCloseMarkup(out *[]byte) {
	switch e.state {
	case stateHeading:
		if e.v.isHTML() {
			*out = append(*out, "</h"...)
			*out = strconv.AppendInt(*out, int64(min(e.heading, e.v.maxHeading())), 10)
			*out = append(*out, '>')
		} else if e.v.maxHeading() == 0 && !e.v.isPlain() {
			*out = append(*out, e.v.close(emphBold)...)
		}
	case statePara:
		if e.v.isHTML() {
			*out = append(*out, "</p>"...)
		}
	case stateList:
		for i := len(e.lists) - 1; i >= 0; i-- {
			*out = append(*out, closeList(e.lists[i].ordered)...)
		}
	case stateQuote:
		*out = append(*out, e.v.quoteClose()...)
	case stateFence:
		e.v.verbatimClose(e.fence.info, out)
	}
}

// closeBlock 은 블록을 닫는다. 열린 인라인을 확정하고, 블록의 닫는 마크업을 붙이고, 경계를 알린다.
func (e *engine) closeBlock(s sink) {
	switch e.state {
	case stateNone:
	case statePara, stateList, stateHeading, stateQuote:
		e.inline.finishBlock(&e.out, e.v)
		e.blockCloseMarkup(&e.out)
	case stateFence:
		e.blockCloseMarkup(&e.out)
	case stateTable:
		e.startLine()
		e.table.render(e.v, e.dialect, &e.inline.repairs, &e.out)
		e.table.clear()
	}
	e.state = stateNone
	e.lists = e.lists[:0]
	e.listGap = false
	e.inline.reset()
	e.flushAll(s)
	s.boundary()
}

// flushSafe 는 열린 마크업 앞까지만 내보낸다. 나머지는 짝이 맞을 때까지 안에 남는다.
func (e *engine) flushSafe(s sink) {
	safe := e.inline.safeLen(len(e.out))
	if safe == 0 {
		return
	}
	s.text(e.out[:safe])
	e.out = append(e.out[:0], e.out[safe:]...)
	e.inline.shift(safe)
}

func (e *engine) flushAll(s sink) {
	if len(e.out) > 0 {
		s.text(e.out)
		e.out = e.out[:0]
	}
}

// markerSpace 는 마커 뒤에 올 수 있는 공백이다. 탭도 공백이다 — `*<TAB>ws` 로 쓴 목록을
// 문단으로 읽으면 줄마다 앞에 선 `*` 가 강조 마커로 짝지어져 불릿이 통째로 사라진다.
func markerSpace(c rune) bool { return c == ' ' || c == '\t' }

// classify 는 받은 데까지로 줄의 종류를 정해 본다. 못 정할 때만 기다린다 — `#` 뒤에 공백이
// 올지 글자가 올지, `--` 가 구분선이 될지 문단이 될지는 몇 글자만 더 보면 된다.
func classify(p []rune, eol, canTable bool) decision {
	indent := 0
	for indent < len(p) && (p[indent] == ' ' || p[indent] == '\t') {
		indent++
	}
	if indent == len(p) {
		if eol {
			return decision{kind: decideWhole, whole: wholeBlank}
		}
		return decision{kind: needMore}
	}
	t := p[indent:]
	para := decision{kind: decidePrefix, line: lineKind{k: linePara}, prefix: indent}
	atEnd := func(d decision) decision {
		if eol {
			return para
		}
		return d
	}
	more := decision{kind: needMore}
	get := func(i int) (rune, bool) {
		if i < len(t) {
			return t[i], true
		}
		return 0, false
	}

	switch c := t[0]; {
	case c == '#':
		n := runLen(t, 0, '#')
		if n > 6 {
			return para
		}
		next, ok := get(n)
		switch {
		case ok && markerSpace(next):
			return decision{kind: decidePrefix, line: lineKind{k: lineHeading, level: n}, prefix: indent + n + 1}
		case ok:
			return para
		default:
			return atEnd(more)
		}
	case c == '>':
		next, ok := get(1)
		switch {
		case ok && next == ' ':
			return decision{kind: decidePrefix, line: lineKind{k: lineQuote}, prefix: indent + 2}
		case ok:
			return decision{kind: decidePrefix, line: lineKind{k: lineQuote}, prefix: indent + 1}
		case eol:
			return decision{kind: decidePrefix, line: lineKind{k: lineQuote}, prefix: indent + 1}
		default:
			return more
		}
	case c == '`' || c == '~':
		n := runLen(t, 0, c)
		if n >= 3 {
			// 여는 펜스의 info 는 줄 끝까지다.
			if eol {
				return decision{kind: decideWhole, whole: wholeFence}
			}
			return more
		}
		if len(t) > n {
			return para
		}
		return atEnd(more)
	case c == '|' && canTable:
		if eol {
			return decision{kind: decideWhole, whole: wholeTableCandidate}
		}
		return more
	case c == '+' || c == '•':
		next, ok := get(1)
		switch {
		case ok && markerSpace(next):
			return decision{kind: decidePrefix, line: lineKind{k: lineBullet, indent: indent}, prefix: indent + 2}
		case ok:
			return para
		default:
			return atEnd(more)
		}
	case c == '-' || c == '*' || c == '_':
		n := runLen(t, 0, c)
		if n == 1 && c != '_' {
			next, ok := get(1)
			switch {
			case ok && markerSpace(next):
				return decision{kind: decidePrefix, line: lineKind{k: lineBullet, indent: indent}, prefix: indent + 2}
			case ok:
				return para
			default:
				return atEnd(more)
			}
		}
		// 같은 글자가 이어진다. 구분선은 그 글자와 공백만으로 된 줄이고, 그게 아니면
		// `**굵게` 처럼 강조로 시작하는 문단이다.
		for _, x := range t[n:] {
			if x != c && !markerSpace(x) {
				return para
			}
		}
		if !eol {
			return more
		}
		count := 0
		for _, x := range t {
			if x == c {
				count++
			}
		}
		if count >= 3 {
			return decision{kind: decideWhole, whole: wholeRule}
		}
		return para
	case c >= '0' && c <= '9':
		d := 0
		for d < len(t) && t[d] >= '0' && t[d] <= '9' {
			d++
		}
		if d > 9 {
			return para
		}
		mark, okMark := get(d)
		next, okNext := get(d + 1)
		switch {
		case okMark && (mark == '.' || mark == ')') && okNext && markerSpace(next):
			n := 0
			for _, x := range t[:d] {
				n = n*10 + int(x-'0')
			}
			return decision{kind: decidePrefix, line: lineKind{k: lineOrdered, indent: indent, n: n}, prefix: indent + d + 2}
		case (okMark && (mark == '.' || mark == ')') && !okNext) || !okMark:
			return atEnd(more)
		default:
			return para
		}
	default:
		return para
	}
}

// safeCut 은 아직 내보내면 안 되는 꼬리를 빼고 남은 길이다. 마커는 다음 글자를 봐야 열기/닫기가
// 갈린다. 줄 끝 공백은 지워야 한다. 닫히지 않은 `[` 는 링크가 될지 글자가 될지 모른다.
//
// 붙든 구문은 이어서 훑는다. 닫히지 않은 `[`·`<!--` 는 줄 끝까지 붙들 수 있는데, 조각마다
// 처음부터 다시 훑으면 한 줄 안에서 O(n²) 이 된다. 그 줄 안에서 pending 은 뒤에 붙기만 하고
// 앞은 cut 만큼 빠지므로, 훑던 자리를 holdMemo 에 두고 새로 온 꼬리만 본다.
func safeCut(p []rune, memo *holdMemo) int {
	k := len(p)
	// 링크는 `[` 부터 `](…)` 의 `)` 까지 통째로 봐야 한다. 텍스트 안의 `)` 로 놓으면 안 된다 —
	// `[Show GN (MAYDAY)](url)` 이 64바이트 조각으로 들어올 때 `(MAYDAY)` 의 `)` 에서 놓아
	// 링크가 글자로 나갔다(실제 문서 대조에서 나왔다).
	// 앞에서부터 본다. 마지막 `[` 만 보면 `[a](1.[b](url` 처럼 주소 안에 링크가 겹칠 때 바깥
	// `[` 가 먼저 글자로 나간다 — 완성본은 바깥을 링크로 읽는다(퍼즈에서 나왔다).
	// 앞 조각에서 여기까지는 안 닫힌 `[` 가 없었다 — 거기서 잇는다.
	i := min(memo.scanned, k)
	for i < k {
		if p[i] != '[' {
			i++
			continue
		}
		from := linkScan{phase: scanBracket, j: 1}
		if memo.hasLink && memo.linkAt == i {
			from = memo.link
		}
		end, scan, ok := linkEndFrom(p[i:k], from)
		if !ok {
			memo.hasLink, memo.linkAt, memo.link = true, i, scan
			k = i
			break
		}
		i += end
	}
	memo.scanned = i
	// 태그 모양의 `<` 도 붙든다. `<sub>` 가 `<su` / `b>` 로 갈리면 앞쪽이 글자로 나가 버린다.
	// `>` 가 오거나 태그라기엔 길어지면 놓는다. 다음 글자가 아직 안 왔으면 일단 붙든다.
	// 주석은 `-->` 까지 통째로 붙든다 — 길이를 안 잰다. 긴 주석을 80자에서 놓으면 `<` 가 글자로
	// 나가 주석이 본문에 새고, 완성본(주석을 지운다)과 갈린다. 붙드는 범위는 그 줄 안이다.
	// 주석 안의 `<b>` 에 속지 않게 앞에서부터 본다.
	for i := 0; i+4 <= k; {
		if startsWith(p[i:], "<!--") {
			body := p[i+4 : k]
			// 앞 조각에서 `-->` 가 없던 데까지는 다시 안 본다.
			from := 0
			if memo.hasComment && memo.commentAt == i {
				from = min(memo.commentFrom, len(body))
			}
			end := findSeq(body[from:], "-->")
			if end < 0 {
				// `--` 가 끝에 걸쳤을 수 있다 — 두 글자 앞에서 이어 본다.
				memo.hasComment, memo.commentAt, memo.commentFrom = true, i, max(len(body)-2, 0)
				k = i
				break
			}
			i += 4 + from + end + 3
		} else {
			i++
		}
	}
	if at := lastIndexRune(p[:k], '<'); at >= 0 && indexRune(p[at:k], '>') < 0 {
		body := p[at+1 : k]
		if prefixMatch(body, "http://") || prefixMatch(body, "https://") {
			// 오토링크는 길이를 안 잰다. 주소는 80자를 쉽게 넘고, 놓으면 `<` 가 글자로 나가
			// 링크가 죽는다(실제 문서를 64자 조각으로 흘리다 나왔다). 공백이 오면 오토링크가
			// 아니다 — 단 `<url|텍스트>` 의 텍스트에는 공백이 온다.
			ws, bar := -1, indexRune(body, '|')
			for i, c := range body {
				if unicode.IsSpace(c) {
					ws = i
					break
				}
			}
			if ws < 0 || (bar >= 0 && bar < ws) {
				k = at
			}
		} else {
			tagish := at+1 >= len(p) || isASCIIAlpha(p[at+1]) || p[at+1] == '/' || p[at+1] == '!'
			if tagish && k-at < 80 {
				k = at
			}
		}
	}
	// 마커는 맨 마지막에 붙든다 — 위에서 `[` 나 `<` 를 붙들고 나면 그 앞의 마커가 다시 끝에
	// 서기 때문이다. 역슬래시도 붙든다. 다음 글자를 봐야 탈출인지 글자인지가 갈린다.
	// `!` 도 붙든다 — 다음 글자가 `[` 면 이미지다(`![alt](url)`). 먼저 나가면 완성본과 갈린다.
	for k > 0 {
		switch p[k-1] {
		case '*', '_', '~', '`', '\\', ' ', '\t', '!':
			k--
			continue
		}
		break
	}
	// 역슬래시와 그 다음 글자 사이에서는 끊지 않는다.
	for k > 0 && p[k-1] == '\\' {
		k--
	}
	return k
}

// holdMemo 는 붙든 구문을 어디까지 훑었나다. 자리는 pending 기준이고, 앞이 cut 만큼 빠지면 당긴다.
type holdMemo struct {
	scanned     int // 여기 앞에는 안 닫힌 `[` 가 없다
	hasLink     bool
	linkAt      int // 안 닫힌 `[` 의 자리
	link        linkScan
	hasComment  bool
	commentAt   int // 안 닫힌 `<!--` 의 자리
	commentFrom int // 그 뒤에서 `-->` 를 이어 찾을 자리
}

// shift 는 앞 n 글자가 나갔을 때 자리를 당긴다. 그 안에 있던 구문은 끝난 것이라 잊는다.
func (m *holdMemo) shift(n int) {
	m.scanned = max(m.scanned-n, 0)
	if m.hasLink {
		m.linkAt -= n
		m.hasLink = m.linkAt >= 0
	}
	if m.hasComment {
		m.commentAt -= n
		m.hasComment = m.commentAt >= 0
	}
}

const (
	scanBracket = iota // 짝이 되는 `]` 를 찾는 중
	scanClosed         // `]` 는 찾았고 다음 글자를 기다린다
	scanParen          // `](` 뒤에서 `)` 를 찾는 중
)

// linkScan 은 linkEndFrom 이 멈춘 자리다. j 는 `[` 로부터의 거리다.
type linkScan struct {
	phase, j, depth int
}

// linkEndFrom 은 `[` 로 시작하는 조각이 링크로 끝나는 자리(`)` 다음)다. 링크가 아니면(`]` 뒤가
// `(` 가 아니면) `]` 다음이고, 아직 못 정하면 ok 가 false 와 멈춘 자리 — 붙들고, 다음 조각에서
// 거기서 잇는다. `]` 는 중첩 대괄호를 건너뛰어 찾는다 — 인라인 파서의 findLink 와 같은 규칙이다.
func linkEndFrom(p []rune, s linkScan) (int, linkScan, bool) {
	for {
		switch s.phase {
		case scanBracket:
			for {
				if s.j >= len(p) {
					return 0, s, false
				}
				if p[s.j] == '[' {
					s.depth++
				} else if p[s.j] == ']' {
					if s.depth == 0 {
						break
					}
					s.depth--
				}
				s.j++
			}
			s.phase = scanClosed
		case scanClosed:
			if s.j+1 >= len(p) {
				return 0, s, false
			}
			if p[s.j+1] != '(' {
				return s.j + 1, s, true
			}
			s.phase, s.j = scanParen, s.j+2
		default:
			if c := indexRune(p[min(s.j, len(p)):], ')'); c >= 0 {
				return s.j + c + 1, s, true
			}
			s.j = len(p)
			return 0, s, false
		}
	}
}

// prefixMatch 는 chars 가 s 로 시작하는가다. chars 가 더 짧으면 s 의 앞부분이어야 한다 —
// 아직 다 안 온 `<htt` 도 오토링크 후보로 붙든다.
func prefixMatch(chars []rune, s string) bool {
	i := 0
	for _, c := range s {
		if i >= len(chars) {
			return true
		}
		if chars[i] != c {
			return false
		}
		i++
	}
	return true
}

// trimEnd 는 줄 끝 공백을 뺀 길이다. 정돈의 일부다.
func trimEnd(p []rune) int {
	k := len(p)
	for k > 0 && (p[k-1] == ' ' || p[k-1] == '\t') {
		k--
	}
	return k
}

func lastIndexRune(p []rune, c rune) int {
	for i := len(p) - 1; i >= 0; i-- {
		if p[i] == c {
			return i
		}
	}
	return -1
}

func isASCIIAlpha(c rune) bool { return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') }

// fenceMarker 는 ``` 또는 ~~~ 다. info 문자열까지 돌려준다.
func fenceMarker(t string) (ch rune, n int, info string, ok bool) {
	for _, m := range []rune{'`', '~'} {
		k := 0
		for _, c := range t {
			if c != m {
				break
			}
			k++
		}
		if k >= 3 {
			return m, k, strings.TrimSpace(t[k:]), true
		}
	}
	return 0, 0, "", false
}

// isDelimiterRow 는 표의 구분선인가다. `|` 가 있어야 한다 — 없으면 그냥 구분선이다.
func isDelimiterRow(line string) bool {
	t := strings.TrimSpace(line)
	if !strings.ContainsRune(t, '-') || !strings.ContainsRune(t, '|') {
		return false
	}
	for _, c := range t {
		switch c {
		case '-', ':', '|', ' ', '\t':
		default:
			return false
		}
	}
	return true
}

type align int

const (
	alignLeft align = iota
	alignRight
	alignCenter
)

// table 은 표다. 열 너비를 알려면 끝까지 봐야 하므로 여기만 버퍼링한다.
type table struct {
	rows  [][]string
	align []align
}

// begin 은 머리글과 구분선으로 표를 시작한다. 모양이 안 맞으면 표가 아니다.
func (t *table) begin(header, delim string) bool {
	head := splitCells(header)
	d := splitCells(delim)
	if len(head) == 0 || len(head) != len(d) {
		return false
	}
	t.align = t.align[:0]
	for _, x := range d {
		x = strings.TrimSpace(x)
		switch {
		case strings.HasPrefix(x, ":") && strings.HasSuffix(x, ":"):
			t.align = append(t.align, alignCenter)
		case strings.HasSuffix(x, ":"):
			t.align = append(t.align, alignRight)
		default:
			t.align = append(t.align, alignLeft)
		}
	}
	t.rows = append(t.rows[:0], head)
	return true
}

func (t *table) push(line string) { t.rows = append(t.rows, splitCells(line)) }

func (t *table) clear() {
	t.rows = t.rows[:0]
	t.align = t.align[:0]
}

// render 는 고정폭 블록으로 그린다. 열은 표시 폭으로 맞춘다 — 문자 수로 맞추면 한글이 든
// 표는 반드시 어긋난다(SPEC 7절). 표를 직접 그리는 채널은 GFM 그대로 낸다.
func (t *table) render(v vocab, d Dialect, repairs *Repairs, out *[]byte) {
	cols := len(t.align)
	// 셀 안의 마크업은 고정폭 블록 안에서 살아남지 못한다. 글자로 내린다 — 표를 직접
	// 그리는 채널은 예외다.
	cellVocab := newVocab(Plain, Options{})
	if v.tablesNative() {
		cellVocab = v
	}
	// 셀 안의 방언은 문서를 따른다. 셀에서 고친 것도 문서의 것으로 센다 — 본문의 `**x` 를
	// 닫아 주면 세는데, 같은 것이 셀 안에 있다고 빠지면 표가 든 문서만 덜 센다.
	in := newInline(d)
	in.inCell = true
	cells := make([][]string, 0, len(t.rows))
	for _, row := range t.rows {
		line := make([]string, 0, cols)
		for c := 0; c < cols; c++ {
			var chars []rune
			// 남는 칸을 버리지 않는다. 머리글보다 칸이 많은 줄을 GFM 은 잘라 내지만, 그러면
			// 저자가 쓴 내용이 소리 없이 사라진다. 넘치는 것은 마지막 칸에 이어 붙인다.
			if c+1 == cols && len(row) > cols {
				for k, cell := range row[c:] {
					if k > 0 {
						chars = append(chars, []rune(" | ")...)
					}
					chars = append(chars, []rune(cell)...)
				}
			} else if c < len(row) {
				chars = []rune(row[c])
			}
			var cell []byte
			in.render(chars, &cell, cellVocab)
			in.finishBlock(&cell, cellVocab)
			in.reset()
			line = append(line, string(cell))
		}
		cells = append(cells, line)
	}
	repairs.add(in.repairs)

	if v.isHTML() {
		writeHTMLTable(out, cells, t.align)
		return
	}
	if v.tablesNative() {
		writeGFMTable(out, cells, t.align)
		return
	}

	widths := make([]int, cols)
	for _, row := range cells {
		for c, cell := range row {
			if w := StrWidth(cell); w > widths[c] {
				widths[c] = w
			}
		}
	}

	var body []byte
	for r, row := range cells {
		if r > 0 {
			body = append(body, '\n')
		}
		writeRow(&body, row, widths, t.align)
		if r == 0 {
			body = append(body, '\n')
			for c, w := range widths {
				if c > 0 {
					body = append(body, " | "...)
				}
				for i := 0; i < w; i++ {
					body = append(body, '-')
				}
			}
		}
	}

	v.verbatimOpen("", out)
	if v.verbatimBodyNewline() {
		*out = append(*out, '\n')
	}
	v.escape(string(body), out)
	v.verbatimClose("", out)
}

// writeHTMLTable 은 표를 <table> 로 낸다(HTML). 칸은 이미 escape·렌더된 것을 받는다.
func writeHTMLTable(out *[]byte, cells [][]string, al []align) {
	*out = append(*out, "<table>"...)
	for r, row := range cells {
		tag := "td"
		if r == 0 {
			tag = "th"
			*out = append(*out, "\n<thead>"...)
		} else if r == 1 {
			*out = append(*out, "\n<tbody>"...)
		}
		*out = append(*out, "<tr>"...)
		for c, cell := range row {
			*out = append(*out, '<')
			*out = append(*out, tag...)
			// 왼쪽은 기본값과 구분되지 않아(`---` 도 `:--` 도 왼쪽) 적지 않는다.
			if c < len(al) {
				switch al[c] {
				case alignRight:
					*out = append(*out, ` style="text-align:right"`...)
				case alignCenter:
					*out = append(*out, ` style="text-align:center"`...)
				}
			}
			*out = append(*out, '>')
			*out = append(*out, cell...)
			*out = append(*out, "</"...)
			*out = append(*out, tag...)
			*out = append(*out, '>')
		}
		*out = append(*out, "</tr>"...)
		if r == 0 {
			*out = append(*out, "</thead>"...)
		}
	}
	if len(cells) > 1 {
		*out = append(*out, "</tbody>"...)
	}
	*out = append(*out, "\n</table>"...)
}

// writeGFMTable 은 표를 GFM 그대로 낸다. 셀 안의 `|` 는 다시 `\|` 로 돌린다.
func writeGFMTable(out *[]byte, cells [][]string, al []align) {
	writeCells := func(row []string) {
		*out = append(*out, '|')
		for _, cell := range row {
			*out = append(*out, ' ')
			for _, c := range cell {
				if c == '|' {
					*out = append(*out, '\\')
				}
				*out = appendRune(*out, c)
			}
			*out = append(*out, " |"...)
		}
	}
	for r, row := range cells {
		if r > 0 {
			*out = append(*out, '\n')
		}
		writeCells(row)
		if r == 0 {
			// 구분선은 머리글 칸 수를 따른다. 정렬 정보가 모자라면 왼쪽 정렬로 채운다.
			*out = append(*out, "\n|"...)
			for i := range row {
				a := alignLeft
				if i < len(al) {
					a = al[i]
				}
				switch a {
				case alignRight:
					*out = append(*out, " ---: |"...)
				case alignCenter:
					*out = append(*out, " :---: |"...)
				default:
					*out = append(*out, " --- |"...)
				}
			}
		}
	}
}

func writeRow(out *[]byte, row []string, widths []int, al []align) {
	last := len(row) - 1
	for c, cell := range row {
		if c > 0 {
			*out = append(*out, " | "...)
		}
		pad := widths[c] - StrWidth(cell)
		if pad < 0 {
			pad = 0
		}
		a := alignLeft
		if c < len(al) {
			a = al[c]
		}
		before, after := 0, pad
		switch a {
		case alignRight:
			before, after = pad, 0
		case alignCenter:
			before, after = pad/2, pad-pad/2
		}
		for i := 0; i < before; i++ {
			*out = append(*out, ' ')
		}
		*out = append(*out, cell...)
		// 마지막 열의 오른쪽 여백은 줄 끝 공백일 뿐이라 남기지 않는다.
		if c != last {
			for i := 0; i < after; i++ {
				*out = append(*out, ' ')
			}
		}
	}
}

// splitCells 는 `| a | b |` 를 셀로 나눈다. `\|` 는 셀 안의 파이프다.
func splitCells(line string) []string {
	t := strings.TrimSpace(line)
	t = strings.TrimPrefix(t, "|")
	t = strings.TrimSuffix(t, "|")
	var cells []string
	var cur []rune
	escaped := false
	for _, c := range t {
		switch {
		case c == '\\' && !escaped:
			escaped = true
		case c == '|' && !escaped:
			cells = append(cells, strings.TrimSpace(string(cur)))
			cur = cur[:0]
		default:
			if escaped && c != '|' {
				cur = append(cur, '\\')
			}
			escaped = false
			cur = append(cur, c)
		}
	}
	return append(cells, strings.TrimSpace(string(cur)))
}
