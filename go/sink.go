package mdwire

import (
	"strings"
	"unicode"
	"unicode/utf8"
)

// 출력 싱크와 안전 분할. 엔진은 출력을 싱크에 붙여 나가면서 "여기서 잘라도 안전하다"는
// 지점을 알려 준다(boundary). 분할이 렌더 결과를 다시 훑어 자르는 것이 아니라 구조에서
// 나온다는 뜻이다 — DESIGN.md 의 두 번째 고장이 정확히 "마크업을 만든 뒤에 자르기"였다.

// bytesSink 는 스트리밍용이다. 받은 것을 그대로 이어 붙인다 — 한도 분할은 호출자 몫이다.
type bytesSink struct{ b *[]byte }

func (s bytesSink) text(b []byte) { *s.b = append(*s.b, b...) }
func (s bytesSink) boundary()     {}

// partsSink 는 완성본용이다. 한도를 넘으면 경계에서 나눈다.
type partsSink struct {
	v       vocab
	limit   int
	parts   []string
	cur     []byte
	curLen  int
	pending []byte
}

func newPartsSink(v vocab) *partsSink {
	return &partsSink{v: v, limit: v.channel.Limit()}
}

func (s *partsSink) text(b []byte) { s.pending = append(s.pending, b...) }
func (s *partsSink) boundary()     { s.commit() }

func (s *partsSink) intoParts() []string {
	s.commit()
	if len(s.cur) > 0 {
		s.parts = append(s.parts, string(s.cur))
		s.cur = nil
	}
	return s.parts
}

func (s *partsSink) commit() {
	if len(s.pending) == 0 {
		return
	}
	piece := s.pending
	if len(s.cur) == 0 {
		piece = trimLeadingNewlines(piece)
	}
	n := utf8.RuneCount(piece)
	if n == 0 {
		s.pending = s.pending[:0]
		return
	}
	if s.curLen+n <= s.limit {
		s.cur = append(s.cur, piece...)
		s.curLen += n
	} else {
		if len(s.cur) > 0 {
			s.parts = append(s.parts, string(s.cur))
			s.cur = s.cur[:0]
			s.curLen = 0
		}
		piece = trimLeadingNewlines(piece)
		n = utf8.RuneCount(piece)
		if n <= s.limit {
			s.cur = append(s.cur, piece...)
			s.curLen = n
		} else {
			// 블록 하나가 한도를 넘는다. 이때만 줄·글자 단위로 쪼갠다.
			chunks := splitHard(string(piece), s.limit, s.v)
			if len(chunks) > 0 {
				last := chunks[len(chunks)-1]
				s.parts = append(s.parts, chunks[:len(chunks)-1]...)
				s.cur = append(s.cur[:0], last...)
				s.curLen = utf8.RuneCountInString(last)
			}
		}
	}
	s.pending = s.pending[:0]
}

func trimLeadingNewlines(b []byte) []byte {
	for len(b) > 0 && b[0] == '\n' {
		b = b[1:]
	}
	return b
}

// splitHard 는 블록 하나가 한도보다 길 때의 최후 수단이다. 줄 경계를 먼저 쓰고, 한 줄이
// 통째로 넘으면 공백에서 끊는다. 어느 쪽이든 열린 마크업은 끊는 자리에서 닫고 다음 조각에서
// 다시 연다. 닫는 데 드는 글자까지 미리 빼고 재야 한다.
func splitHard(text string, limit int, v vocab) []string {
	var parts []string
	var cur []byte
	length := 0
	m := newMarkup()
	if v.channel != TelegramHTML && !v.isPlain() {
		m.spans = scanSpans(text)
	}

	for _, line := range splitInclusive(text, '\n') {
		for _, word := range splitInclusive(line, ' ') {
			rest := word
			for len(rest) > 0 {
				// 넣고 난 뒤의 마크업으로 예산을 잡는다. 넣기 전 상태로 재면 이번에 들어가는
				// 조각이 태그를 하나 더 열었을 때 그 태그를 닫을 자리가 남지 않는다.
				probe := m.clone()
				probe.feed(rest, v)
				budget := max(limit-probe.reserve(), 0)
				n := utf8.RuneCountInString(rest)
				if length+n <= budget {
					m = probe
					length += pushAfterReopen(&cur, rest, &m)
					break
				}
				if length > 0 {
					before := length
					cut(&parts, &cur, &length, &m, v, limit)
					if length < before {
						continue
					}
					// 끊어도 자리가 안 생겼다 — 다시 연 마크업이 그만큼 도로 먹은 것이다.
					// 아래 글자 단위 경로로 내려가 반드시 한 글자는 소비한다.
				}
				// 공백 하나 없는 덩어리다 — 글자로 끊는다. 예산이 0 이면 이 덩어리가 여는
				// 태그가 한도만 하다는 뜻이라, 지금 열려 있는 것 기준으로 최대한 담는다.
				room := budget
				if budget == 0 {
					room = max(limit-m.reserve(), 0)
				}
				take := max(room-length, 1)
				end := runeOffset(rest, take)
				m.feed(rest[:end], v)
				length += pushAfterReopen(&cur, rest[:end], &m)
				rest = rest[end:]
				if len(rest) > 0 {
					cut(&parts, &cur, &length, &m, v, limit)
				}
			}
		}
	}
	if len(cur) > 0 {
		parts = append(parts, string(cur))
	}
	return parts
}

// runeOffset 은 n 번째 글자가 시작하는 바이트 위치다. 글자가 모자라면 끝이다.
func runeOffset(s string, n int) int {
	i := 0
	for k := 0; k < n && i < len(s); k++ {
		_, size := utf8.DecodeRuneInString(s[i:])
		i += size
	}
	return i
}

// splitInclusive 는 구분자를 포함한 채로 나눈다 — 러스트의 split_inclusive.
func splitInclusive(s string, sep byte) []string {
	var out []string
	from := 0
	for i := 0; i < len(s); i++ {
		if s[i] == sep {
			out = append(out, s[from:i+1])
			from = i + 1
		}
	}
	if from < len(s) {
		out = append(out, s[from:])
	}
	return out
}

// pushAfterReopen 은 조각에 글을 붙이고 붙은 글자 수를 돌려준다. 방금 다시 연 마커 바로 뒤라면
// 앞 공백을 턴다 — `** 이어서` 는 열기가 아니다.
func pushAfterReopen(cur *[]byte, s string, m *markup) int {
	if m.fresh {
		s = strings.TrimLeftFunc(s, unicode.IsSpace)
	}
	if s != "" {
		m.fresh = false
	}
	*cur = append(*cur, s...)
	return utf8.RuneCountInString(s)
}

// cut 은 조각을 끊는다. 열린 것을 닫고, 다음 조각 앞머리에서 다시 연다.
func cut(parts *[]string, cur *[]byte, length *int, m *markup, v vocab, limit int) {
	if len(*cur) == 0 {
		return
	}
	part := *cur
	*cur = nil
	// 태그 한가운데서는 끊지 않는다. 여는 태그가 아직 `>` 를 못 만났으면 그만큼 도로 빼서
	// 다음 조각으로 넘긴다.
	var carry []byte
	if p := m.partial; p != "" && len(part) > len(p) && strings.HasSuffix(string(part), p) {
		at := len(part) - len(p)
		carry = append(carry, part[at:]...)
		part = part[:at]
	}
	// 닫는 마커 앞이 공백이면 닫기가 아니다. 마크다운 채널은 조각 끝 공백을 턴다.
	if v.channel != TelegramHTML {
		part = []byte(strings.TrimRightFunc(string(part), unicode.IsSpace))
	}
	m.closeAll(&part, v)
	*parts = append(*parts, string(part))
	m.reopen(cur)
	m.fresh = v.channel != TelegramHTML && len(*cur) > 0
	// 다시 열 수 없는 마크업은 버린다. 여는 태그만으로 조각이 차 버리면 내용이 한 글자도
	// 안 들어가고 같은 자리에서 같은 조각을 끝없이 찍어 낸다.
	reopened := utf8.RuneCount(*cur) + utf8.RuneCount(carry)
	if reopened >= limit {
		*cur = (*cur)[:0]
		m.forget()
		*length = 0
		return
	}
	*cur = append(*cur, carry...)
	*length = reopened
}

// markup 은 지금 열려 있는 마크업이다. 조각을 끊을 때 닫고 다시 열려고 들고 있는다.
type markup struct {
	// 열린 HTML 태그. (이름, 여는 태그 전체)
	tags []tag
	// 열린 코드펜스의 info 문자열.
	fence    string
	hasFence bool
	// 조각 경계에 걸려 아직 `>` 를 못 만난 태그의 앞부분. 여기 들어오는 텍스트는 단어 단위로
	// 잘려 있어서 `<code class="…">` 하나가 두 번에 나뉘어 들어온다.
	partial string
	// 다음에 먹일 글자가 줄 첫머리인가. 인라인 코드 스팬의 울타리와 블록 펜스를 가른다.
	atLineStart bool
	// 마크다운 채널의 인라인 스팬 — 블록 전체를 미리 훑어 짝이 맞는 것만 적어 둔다(글자
	// 위치 기준). 슬랙에서 조각이 코드 스팬 한가운데서 갈리던 것을 막는다. 단어 단위로
	// 따라가는 대신 미리 훑는 이유는 짝 없는 마커가 출력에 글자로 남기 때문이다.
	spans []span
	// 지금까지 먹인 글자 수. spans 의 위치와 맞춰 본다.
	pos int
	// 다시 열 수 없어 버린 뒤다. 그 뒤로는 스팬을 닫지도 열지도 않는다.
	dropped bool
	// 방금 마크다운 마커를 다시 열었고 아직 내용이 안 붙았다.
	fresh bool
}

type tag struct{ name, full string }

// span 은 짝이 맞는 인라인 스팬 하나다. 위치는 글자 단위. 코드 스팬은 run 이 백틱 런 길이고,
// 강조는 marker 가 마커 글자다.
type span struct {
	run    int
	marker string
	start  int // 여는 마커의 첫 글자 위치
	end    int // 닫는 마커 다음 위치
}

func (sp span) write(out *[]byte) {
	if sp.marker != "" {
		*out = append(*out, sp.marker...)
		return
	}
	for i := 0; i < sp.run; i++ {
		*out = append(*out, '`')
	}
}

func (sp span) length() int {
	if sp.marker != "" {
		return len(sp.marker)
	}
	return sp.run
}

func newMarkup() markup {
	// 블록은 줄 첫머리에서 시작한다.
	return markup{atLineStart: true}
}

// clone 은 복제다. spans 는 읽기만 하므로 배열을 공유한다 — 단어마다 복제하는 자리라 복사하면
// 조각 크기에 비례해 는다.
func (m markup) clone() markup {
	m.tags = append([]tag(nil), m.tags...)
	return m
}

func (m *markup) feed(s string, v vocab) {
	if v.channel == TelegramHTML {
		m.feedHTML(s)
	} else {
		m.feedFence(s)
		m.pos += utf8.RuneCountInString(s)
	}
}

// openSpans 는 지금 위치에서 열려 있는 스팬이다. 바깥부터 순서대로.
func (m *markup) openSpans() []span {
	if m.dropped {
		return nil
	}
	var out []span
	for _, sp := range m.spans {
		if sp.start < m.pos && m.pos < sp.end {
			out = append(out, sp)
		}
	}
	return out
}

func (m *markup) feedHTML(s string) {
	text := s
	if m.partial != "" {
		text = m.partial + s
		m.partial = ""
	}
	ch := []rune(text)
	i := 0
	for i < len(ch) {
		if ch[i] != '<' {
			i++
			continue
		}
		start := i
		closing := i+1 < len(ch) && ch[i+1] == '/'
		from := i + 1
		if closing {
			from = i + 2
		}
		j := from
		for j < len(ch) && (isASCIIAlnum(ch[j]) || ch[j] == '-') {
			j++
		}
		name := string(ch[from:j])
		for j < len(ch) && ch[j] != '>' {
			j++
		}
		if j >= len(ch) {
			// `>` 를 아직 못 봤다. 다음 조각과 이어 붙여서 다시 본다.
			m.partial = string(ch[start:])
			return
		}
		if name == "" {
			i++
			continue
		}
		if closing {
			for k := len(m.tags) - 1; k >= 0; k-- {
				if m.tags[k].name == name {
					m.tags = m.tags[:k]
					break
				}
			}
		} else {
			m.tags = append(m.tags, tag{name: name, full: string(ch[start : j+1])})
		}
		i = j + 1
	}
}

func (m *markup) feedFence(s string) {
	for i, line := range strings.Split(s, "\n") {
		// 줄 첫머리에 선 것만 펜스다. 문장 한가운데의 ``` 는 인라인 울타리다.
		if i == 0 && !m.atLineStart {
			continue
		}
		t := strings.TrimLeftFunc(line, unicode.IsSpace)
		if strings.HasPrefix(t, "```") {
			if m.hasFence {
				m.hasFence, m.fence = false, ""
			} else {
				m.hasFence, m.fence = true, strings.TrimLeft(t, "`")
			}
		}
	}
	if s != "" {
		// 공백만 먹었으면 아직 줄 첫머리다 — 들여쓴 펜스는 단어 단위로 들어올 때 공백이 먼저 온다.
		m.atLineStart = strings.HasSuffix(s, "\n") || (m.atLineStart && strings.TrimSpace(s) == "")
	}
}

// reserve 는 지금 끊으면 닫고 다시 여는 데 드는 글자 수다. 미리 빼 두지 않으면 한도를 넘긴다.
func (m *markup) reserve() int {
	n := 0
	for _, t := range m.tags {
		n += utf8.RuneCountInString(t.name) + 3 + utf8.RuneCountInString(t.full)
	}
	if m.hasFence {
		// 닫는 펜스 + 다시 여는 펜스.
		n += 4 + 4 + utf8.RuneCountInString(m.fence)
	}
	// 스팬은 닫는 마커와 다시 여는 마커, 두 번.
	for _, sp := range m.openSpans() {
		n += sp.length() * 2
	}
	return n
}

func (m *markup) closeAll(out *[]byte, v vocab) {
	// 안쪽부터 — 늦게 열린 것이 안쪽이다.
	open := m.openSpans()
	for i := len(open) - 1; i >= 0; i-- {
		open[i].write(out)
	}
	for i := len(m.tags) - 1; i >= 0; i-- {
		*out = append(*out, "</"...)
		*out = append(*out, m.tags[i].name...)
		*out = append(*out, '>')
	}
	if m.hasFence {
		v.verbatimClose("", out)
	}
}

// forget 은 열린 것을 없던 일로 한다. 이미 닫아서 내보낸 뒤에만 부른다.
func (m *markup) forget() {
	m.tags = m.tags[:0]
	m.hasFence, m.fence = false, ""
	m.partial = ""
	m.dropped = true
}

func (m *markup) reopen(out *[]byte) {
	if m.hasFence {
		*out = append(*out, "```"...)
		*out = append(*out, m.fence...)
		*out = append(*out, '\n')
	}
	for _, t := range m.tags {
		*out = append(*out, t.full...)
	}
	for _, sp := range m.openSpans() {
		sp.write(out)
	}
}

// scanSpans 는 마크다운 출력 한 블록의 인라인 스팬을 찾는다. 우리 렌더러가 낸 출력이라
// 마커는 짝이 맞고 겹침도 바르다. 글자로 남은 마커는 짝이 안 맞아 여기서 걸러진다 — 여는
// 쪽은 뒤가 글자, 닫는 쪽은 앞이 글자여야 하고, 코드 스팬은 같은 길이의 런이 뒤에 있어야
// 한다. 펜스 안은 보지 않는다.
func scanSpans(text string) []span {
	ch := []rune(text)
	// 펜스 줄과 그 안은 스팬이 아니고, 스팬이 그 너머와 짝을 맺지도 못한다.
	fenced := make([]bool, len(ch))
	{
		fence := false
		i := 0
		for i < len(ch) {
			j := i
			for j < len(ch) && (ch[j] == ' ' || ch[j] == '\t') {
				j++
			}
			lineEnd := len(ch)
			if p := indexRune(ch[i:], '\n'); p >= 0 {
				lineEnd = i + p
			}
			isFence := startsWith(ch[j:], "```")
			if isFence || fence {
				for k := i; k < lineEnd; k++ {
					fenced[k] = true
				}
			}
			if isFence {
				fence = !fence
			}
			i = lineEnd + 1
		}
	}
	var spans []span
	type openSpan struct {
		marker string
		at     int
	}
	var open []openSpan
	i := 0
	for i < len(ch) {
		c := ch[i]
		if fenced[i] {
			i++
			continue
		}
		// 줄이 바뀌면서 새 항목이 시작되면 열린 강조는 없던 일이다 — 조각 하나에 목록 항목
		// 여럿이 담기는데, 렌더러는 항목마다 인라인을 확정한다.
		if c == '\n' {
			j := i + 1
			for j < len(ch) && (ch[j] == ' ' || ch[j] == '\t') {
				j++
			}
			next := ch[j:]
			item := len(next) == 0 || next[0] == '\n'
			if !item && len(next) >= 2 && next[1] == ' ' {
				switch next[0] {
				case '-', '*', '+', '>', '#':
					item = true
				}
			}
			if !item && next[0] >= '0' && next[0] <= '9' {
				k := 1
				for k < len(next) && next[k] >= '0' && next[k] <= '9' {
					k++
				}
				item = k < len(next) && next[k] == '.'
			}
			if item {
				open = open[:0]
			}
			i++
			continue
		}
		if c == '\\' {
			i += 2
			continue
		}
		if c == '`' {
			run := runLen(ch, i, '`')
			// 셋 이상의 런은 스팬으로 보지 않는다 — 산문에 글자로 남은 ``` (한 줄에 쏟아낸
			// 가짜 펜스)가 코드블록 펜스와 짝을 맺으면 조각마다 펜스가 찍힌다.
			if run >= 3 {
				i += run
				continue
			}
			// 같은 길이의 런이 이 줄 안에 더 있어야 코드 스팬이다. 없으면 글자다. 코드 스팬은
			// 줄을 넘지 않는다 — 짝 없이 남은 백틱이 여러 항목 뒤의 백틱과 스팬을 맺는 쪽이 흔하다.
			j := i + run
			closeAt := -1
			for j < len(ch) {
				p := -1
				for k := j; k < len(ch); k++ {
					if ch[k] == '`' || ch[k] == '\n' {
						p = k
						break
					}
				}
				if p < 0 || ch[p] == '\n' || fenced[p] {
					break
				}
				n := runLen(ch, p, '`')
				if n == run {
					closeAt = p
					break
				}
				j = p + n
			}
			if closeAt >= 0 {
				spans = append(spans, span{run: run, start: i, end: closeAt + run})
				i = closeAt + run
			} else {
				i += run
			}
			continue
		}
		if c != '*' && c != '~' {
			i++
			continue
		}
		run := runLen(ch, i, c)
		prev, next := noChar, noChar
		if i > 0 {
			prev = ch[i-1]
		}
		if i+run < len(ch) {
			next = ch[i+run]
		}
		opens := next != noChar && !unicode.IsSpace(next)
		closes := prev != noChar && !unicode.IsSpace(prev)
		// `***` 는 `**` 와 `*` 다. 열 때는 굵게가 바깥, 닫을 때는 기울임이 먼저.
		var markers []string
		switch {
		case c == '~' && run == 2:
			markers = []string{"~~"}
		case c == '*' && run == 1:
			markers = []string{"*"}
		case c == '*' && run == 2:
			markers = []string{"**"}
		case c == '*' && run == 3:
			if len(open) > 0 && open[len(open)-1].marker == "*" {
				markers = []string{"*", "**"}
			} else {
				markers = []string{"**", "*"}
			}
		}
		at := i
		for _, mk := range markers {
			if closes && len(open) > 0 && open[len(open)-1].marker == mk {
				o := open[len(open)-1]
				open = open[:len(open)-1]
				spans = append(spans, span{marker: mk, start: o.at, end: at + len(mk)})
			} else if opens {
				open = append(open, openSpan{marker: mk, at: at})
			}
			at += len(mk)
		}
		i += run
	}
	sortSpans(spans)
	return spans
}

func sortSpans(s []span) {
	// 삽입 정렬 — 스팬은 많아야 수백 개고 거의 정렬돼 들어온다.
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j-1].start > s[j].start; j-- {
			s[j-1], s[j] = s[j], s[j-1]
		}
	}
}
