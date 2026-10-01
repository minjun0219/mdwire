package mdwire

import (
	"bytes"
	"strings"
	"unicode"
	"unicode/utf8"
)

// 인라인 파서 — 강조의 짝을 맞춘다. 이 파일이 이 라이브러리의 이유다.
//
// 규칙은 셋이다.
//  1. 같은 종류가 열려 있고 앞이 공백이 아니면 닫는다. 줄 첫머리의 `**` 는 앞이 줄바꿈이라
//     닫기가 될 수 없다 — 정규식이 이걸 못 해서 강조 범위가 뒤집혔다(DESIGN.md).
//  2. 여는 마커가 또 오면 먼저 열린 쪽이 진다. "닫기"로 읽는 순간 범위가 뒤집힌다.
//  3. 블록이 끝나면 열린 것을 닫는다(SPEC 6절). 추측으로 연 것은 글자로 되돌린다.
//
// 스트리밍: 출력은 out 에 붙여 나가되, 열린 마커가 있으면 그 자리부터는 내보내지 않는다
// (safeLen). 여는 마크업은 짝이 맞는 순간 그 자리에 끼워 넣는다.

// noChar 는 "글자 없음"이다 — 러스트의 Option<char> 의 None. rune 은 음수가 될 수 없어 안전하다.
const noChar rune = -1

type openMark struct {
	emph emph
	// out 안에서 여는 마크업이 들어갈 바이트 위치.
	at int
	// 마커 길이. 코드 스팬은 백틱 런의 길이를 그대로 쓴다(닫을 때 같아야 한다).
	run int
	ch  rune
	// 추측으로 열었는가. 안 닫히면 글자로 되돌리거나 버린다.
	guess bool
	// 이 마커 앞이 공백(또는 블록 시작)이었는가. 추측이 빗나갔을 때 되돌릴지 버릴지를 가른다.
	afterSpace bool
	// 여는 마커 바로 앞의 원문 글자. GitHub 이 이 짝을 강조로 읽는지 가를 때 쓴다.
	before rune
	// 더 긴 런(`***`)의 첫 조각인가. 안 닫히면 버리지 않고 글자로 되돌린다 — 원래 한 덩어리의
	// 글자였다(마스킹 번호 `4***-…`).
	split bool
}

type inline struct {
	open []openMark
	// 줄을 넘어온 직전 글자. 블록 안에서 줄이 바뀌면 '\n' 이다.
	prev rune
	// 링크 텍스트를 렌더할 때만 쓰는 버퍼. 재사용해서 할당을 아낀다.
	scratch []byte
	// 지금 열려 있는 코드 스팬의 날것 내용. 블록이 끝나도록 닫는 런이 안 오면 그 백틱은
	// 글자였다는 뜻이라, 삼킨 내용을 도로 꺼내 다시 읽는다.
	codeSrc []rune
	// 정규화가 고친 것.
	repairs Repairs
	// 지금 닫는 마커 바로 뒤의 원문 글자. 짝이 맞아 닫을 때만 뜻이 있다.
	afterClose rune
	// 표 칸 안을 렌더하는가. 칸 안에서는 줄을 바꿀 수 없다 — 바꾸면 표의 행이 갈린다.
	inCell bool
	// 줄 첫머리라 벗긴 인라인 여는 태그의 수. 그 짝인 닫는 태그도 벗긴다(GitHub).
	strippedTags []uint8
	// preview 면 미리보기 복제본이다 — 블록이 끝날 때 안 닫힌 코드 스팬을 글자로 되돌리지 않고
	// 닫는다. 러스트 쪽 Inline::preview.
	preview bool
	// wrap 은 노션에서 강조를 줄마다 감쌀 때 범위를 옮겨 두는 버퍼다(wrapPerLine). 재사용한다.
	wrap []byte
}

func newInline() *inline {
	return &inline{prev: noChar, afterClose: noChar}
}

// reset 은 블록 경계다. 인라인 상태는 블록을 넘지 않는다.
func (in *inline) reset() {
	in.open = in.open[:0]
	in.prev = noChar
	in.strippedTags = in.strippedTags[:0]
}

// safeLen 은 지금 out 에서 내보내도 안전한 길이다. 열린 마커가 있으면 그 앞까지다.
func (in *inline) safeLen(outLen int) int {
	if len(in.open) > 0 {
		return in.open[0].at
	}
	return outLen
}

// shift 는 앞쪽 n 바이트를 내보냈다고 알린다. 기억하고 있던 자리를 당긴다.
func (in *inline) shift(n int) {
	for i := range in.open {
		in.open[i].at -= n
	}
}

// endLine: 줄 하나가 끝났다. 다음 줄의 첫 글자에게 앞 글자는 줄바꿈이다.
func (in *inline) endLine() { in.prev = '\n' }

// noteRaw 는 블록 층이 out 에 바로 쓴 글자를 코드 스팬 내용에도 남긴다 — 줄 사이의 구분자는
// render 를 거치지 않는데, 되돌려 다시 읽을 때 두 줄이 한 줄로 붙으면 안 된다.
func (in *inline) noteRaw(s string) {
	if n := len(in.open); n > 0 && in.open[n-1].emph == emphCode {
		in.codeSrc = appendRunes(in.codeSrc, s)
	}
}

// setPrev 는 블록 접두사(`- `, `> `)를 건너뛴 뒤의 앞 글자를 세운다.
func (in *inline) setPrev(c rune) { in.prev = c }

func (in *inline) isOpen() bool { return len(in.open) > 0 }

// render 는 인라인을 렌더한다. 한 줄을 여러 번에 나눠 넣어도 된다 — 대신 마커 런이 조각
// 끝에 걸친 채로 들어오면 안 된다. 마커는 다음 글자를 봐야 열기/닫기가 갈리기 때문이고, 그
// 판단은 호출자(블록 층)가 한다.
func (in *inline) render(line []rune, out *[]byte, v vocab) {
	i := 0
	for i < len(line) {
		// 코드 스팬 안에서는 강조 마커가 글자다.
		if n := len(in.open); n > 0 && in.open[n-1].emph == emphCode {
			top := in.open[n-1]
			run := 0
			if line[i] == '`' {
				run = runLen(line, i, '`')
			}
			if run == top.run {
				in.closeAt(n-1, out, v)
				i += run
			} else {
				// 안 맞는 백틱 런은 통째로 건너뛴다. 한 글자씩 넘기면 길이 N+1 인 런 안에
				// 길이 N 인 런이 들어 있는 꼴이 되어 그 안쪽에서 잘못 닫는다.
				step := run
				if step < 1 {
					step = 1
				}
				for k := 0; k < step; k++ {
					v.codeChar(line[i+k], out)
					in.codeSrc = append(in.codeSrc, line[i+k])
				}
				i += step
			}
			continue
		}

		c := line[i]

		// 이미지 `![alt](url)`. 대체 글은 링크 텍스트처럼 인라인으로 읽는다.
		if c == '!' && i+1 < len(line) && line[i+1] == '[' {
			if t0, t1, u0, u1, ok := findLink(line, i+1); ok {
				in.renderImage(line[t0:t1], line[u0:u1], out, v)
				i = u1 + 1
				continue
			}
		}

		if c == '[' {
			if t0, t1, u0, u1, ok := findLink(line, i); ok {
				in.renderLink(line[t0:t1], line[u0:u1], out, v)
				i = u1 + 1
				continue
			}
		}

		if c == '<' {
			if step := in.angle(line, i, out, v); step > 0 {
				i += step
				continue
			}
		}

		// 역슬래시 이스케이프. `\_` 는 밑줄 한 글자지 강조 마커가 아니다.
		if c == '\\' && i+1 < len(line) && isASCIIPunct(line[i+1]) {
			v.literal(line[i+1], out)
			in.prev = line[i+1]
			i += 2
			continue
		}

		if c == '`' {
			run := runLen(line, i, '`')
			prev := in.prevChar(line, i)
			in.codeSrc = in.codeSrc[:0]
			in.open = append(in.open, openMark{emph: emphCode, at: len(*out), run: run, ch: '`', afterSpace: prev == noChar || unicode.IsSpace(prev), before: prev})
			i += run
			continue
		}

		if c != '*' && c != '_' && c != '~' {
			in.textChar(c, out, v)
			i++
			continue
		}

		// 런은 통째로 소비한다. `***` 는 `**` 와 `*` 다 — 굵게 안에 기울임. 열 때는 굵게를
		// 먼저 열고(바깥), 닫을 때는 기울임이 열려 있으면 그것부터 닫는다(안쪽). 굵게만 열려
		// 있는데 셋이 오면 통째로 닫는 마커다 — 둘만 집으면 별표 하나가 남는다.
		take := runLen(line, i, c)
		if c != '~' && take == 3 {
			switch {
			case in.hasOpen(emphItalic):
				take = 1
			case in.hasOpen(emphBold):
				take = 3
			default:
				take = 2
			}
		}
		var e emph
		switch {
		case c == '~':
			e = emphStrike
		case take == 1:
			e = emphItalic
		default:
			e = emphBold
		}
		// 취소선은 `~~` 다. 홀로 선 `~` 는 글자다 — `~40km`, `5~6월`.
		if c == '~' && take < 2 {
			in.textChar(c, out, v)
			i++
			continue
		}

		// 쪼갠 런의 남은 조각은 런 전체의 앞 글자를 본다. 남은 `*` 가 제 앞 별표를 앞 글자로
		// 보면 열 수 있는 자리가 되어, 마스킹 번호 `4***-****-****-003*` 가 기울어졌다(실측).
		start := i
		for start > 0 && line[start-1] == c {
			start--
		}
		split := start == i && runLen(line, i, c) > take
		prev := in.prevChar(line, start)
		next := noChar
		if i+take < len(line) {
			next = line[i+take]
		}
		afterSpace := prev == noChar || unicode.IsSpace(prev)
		same := in.lastOpen(e)

		// 글자 뒤의 `_` 는 열지 못한다(CommonMark 의 단어 안 `_`). `snake_case` 도
		// `2026-04-29_제목` 도 글자로 남는다. 대신 닫는 것은 된다 — `_진료_가`.
		intraword := c == '_' && prev != noChar && isAlphanumeric(prev)
		if intraword && same < 0 {
			for k := 0; k < take; k++ {
				in.textChar(c, out, v)
			}
			i += take
			continue
		}

		left := canOpen(prev, next) && !intraword
		fresh := openMark{emph: e, run: take, ch: c, afterSpace: afterSpace, before: prev, split: split}

		switch {
		// 추측으로 연 것은 닫지 않는다. 추측은 확정되지 않는다 — 닫아 주면 여는 쪽은 사라지고
		// 닫는 쪽만 없어져 `underfront.*`·`/* 주석 */` 의 별표가 없어졌다. 앞이 글자인 마커도
		// 추측을 닫지 않고 열지도 않는다 — 열면 블록 끝까지 삼킨다. 글자다.
		case same >= 0 && in.open[same].guess && (!left || !afterSpace):
			for k := 0; k < take; k++ {
				in.textChar(c, out, v)
			}
		// 여는 자리의 마커가 왔는데 추측이 열려 있다 — 추측이 틀렸다. 되돌리고 이쪽을 연다.
		case same >= 0 && in.open[same].guess:
			in.reopenAt(same, out, v, fresh)
		// 같은 종류가 열려 있고 앞이 공백이 아니면 여기가 닫는 자리다. 규칙 1.
		case same >= 0 && !afterSpace:
			in.afterClose = next
			in.closeAt(same, out, v)
		// 앞이 공백인데 뒤로는 열 수 있다 — 여는 마커가 또 왔다. 먼저 열린 쪽이 진다. 규칙 2.
		case same >= 0 && left && same+1 == len(in.open):
			in.reopenAt(same, out, v, fresh)
		// 안쪽에 다른 종류가 열려 있으면 갈아 끼우지 못한다. 버린다.
		case same >= 0 && left:
		case same >= 0:
			in.afterClose = next
			in.closeAt(same, out, v)
		case left:
			fresh.at = len(*out)
			in.open = append(in.open, fresh)
		// 열 수도 닫을 수도 없다. 일단 열어 두고 안 닫히면 글자로 되돌린다. 규칙 3.
		default:
			fresh.at = len(*out)
			fresh.guess = true
			in.open = append(in.open, fresh)
		}
		i += take
	}
	if len(line) > 0 {
		// 다음 호출의 첫 글자에게 앞 글자를 남긴다. 한 줄을 나눠 넣어도 flanking 판정이 이어진다.
		in.prev = line[len(line)-1]
	}
}

// finishBlock: 블록이 끝났다. 열린 것을 전부 정리한다.
func (in *inline) finishBlock(out *[]byte, v vocab) {
	for len(in.open) > 0 {
		if in.open[len(in.open)-1].emph == emphCode && !in.preview {
			in.revertCodeSpan(out, v)
			continue
		}
		in.finalize(out, v, false)
	}
	in.prev = noChar
}

// revertCodeSpan 은 안 닫힌 코드 스팬을 글자로 되돌린다. 백틱만 되돌리고 끝내면 안 된다 —
// 삼킨 내용은 코드로 읽혀서 강조가 안 걸린 상태라, 도로 꺼내 다시 읽어야 그 안의 강조가 산다.
func (in *inline) revertCodeSpan(out *[]byte, v vocab) {
	n := len(in.open)
	if n == 0 {
		return
	}
	o := in.open[n-1]
	in.open = in.open[:n-1]
	in.repairs.RevertedCodeSpan++
	*out = (*out)[:o.at]
	for k := 0; k < o.run; k++ {
		*out = appendRune(*out, o.ch)
	}
	// 버퍼를 통째로 빌려 와서 다시 읽고 돌려준다. 새로 만들지 않는다.
	src := in.codeSrc
	in.codeSrc = nil
	in.prev = o.ch
	in.render(src, out, v)
	// 다시 읽는 동안 새 코드 스팬이 열렸으면 그쪽 버퍼를 지키고, 아니면 돌려준다.
	if len(in.codeSrc) == 0 {
		in.codeSrc = src[:0]
	}
}

// reopenAt 은 맨 위의 열린 마커를 물리고 이 자리에서 새로 연다 — 먼저 열린 쪽이 졌다.
// 홑마커는 글자로 되돌린다(글롭·주석·각주). `**` 는 추측이었고 앞이 공백이었을 때만
// 되돌린다(`2 ** 3`) — 진짜 여는 마커였다가 진 `**` 는 짝 잃은 마커라 버린다.
func (in *inline) reopenAt(at int, out *[]byte, v vocab, fresh openMark) {
	// at 위에 열린 것들은 먼저 정리한다 — 아래만 빼면 열린 것들의 순서가 깨진다.
	for len(in.open) > at+1 {
		in.finalize(out, v, false)
	}
	old := in.open[len(in.open)-1]
	in.open = in.open[:len(in.open)-1]
	if old.run == 1 || (old.guess && (old.afterSpace || old.split)) {
		insertMarker(out, old.at, old.ch, old.run, v, &in.repairs)
	} else {
		in.repairs.DroppedMarker++
	}
	fresh.at = len(*out)
	fresh.guess = false
	in.open = append(in.open, fresh)
}

func (in *inline) prevChar(line []rune, i int) rune {
	if i > 0 {
		return line[i-1]
	}
	return in.prev
}

func (in *inline) hasOpen(e emph) bool {
	for _, o := range in.open {
		if o.emph == e {
			return true
		}
	}
	return false
}

// lastOpen 은 같은 종류가 열린 가장 위의 자리다. 없으면 -1.
func (in *inline) lastOpen(e emph) int {
	for i := len(in.open) - 1; i >= 0; i-- {
		if in.open[i].emph == e {
			return i
		}
	}
	return -1
}

// closeAt 은 at 번째 열린 마커를 닫는다. 그 위에 열린 것들은 먼저 정리한다.
// at 은 짝이 맞아 닫히고, 그 위에 있던 것들은 짝 없이 정리된다.
func (in *inline) closeAt(at int, out *[]byte, v vocab) {
	for len(in.open) > at+1 {
		in.finalize(out, v, false)
	}
	in.finalize(out, v, true)
}

// finalize 는 맨 위 마커 하나를 확정한다 — 닫거나, 글자로 되돌리거나. matched 는 닫는 마커가
// 와서 닫는 것인가다. 아니면 저자 대신 닫아 주는 것이라 고친 것으로 센다.
func (in *inline) finalize(out *[]byte, v vocab, matched bool) {
	n := len(in.open)
	if n == 0 {
		return
	}
	o := in.open[n-1]
	in.open = in.open[:n-1]

	// 원문의 태그는 마커가 아니다 — 되돌릴 글자도, 저자 대신 고친 강조도 없다. 비었으면
	// 버리고, 아니면 여닫는다.
	if o.emph >= emphTag {
		if len(*out) > o.at {
			insertAt(out, o.at, v.open(o.emph))
			*out = append(*out, v.close(o.emph)...)
		}
		return
	}

	// 내용이 비었으면 태그를 만들지 않는다. `<b></b>` 는 아무에게도 쓸모가 없다.
	empty := len(*out) == o.at
	if o.guess || empty {
		// 추측이 빗나갔다. 홑마커는 글자로 되돌린다 — 각주·글롭·곱셈. `**` 는 앞이 공백이었을
		// 때만 되돌린다(`2 ** 3`). 앞이 글자인 `**` 가 홀로 남을 이유는 없다.
		if o.afterSpace || o.run == 1 || o.split {
			insertMarker(out, o.at, o.ch, o.run, v, &in.repairs)
		} else {
			in.repairs.DroppedMarker++
		}
		return
	}
	if !matched {
		in.repairs.ClosedEmphasis++
	}

	// 코드 스팬은 앞뒤 공백 하나를 벗긴다(CommonMark). 마커를 지우는 채널에서는 벗기지 않는다 —
	// 마커가 없으면 그 공백이 곧 낱말 경계다.
	if o.emph == emphCode && !v.isPlain() {
		body := (*out)[o.at:]
		if len(body) >= 2 && body[0] == ' ' && body[len(body)-1] == ' ' && !allSpaces(body) {
			*out = (*out)[:len(*out)-1]
			removeAt(out, o.at)
		}
	}
	// 내용에 백틱이 있으면 울타리를 늘린다. 내용 안의 가장 긴 런보다 하나 긴 울타리를 쓰고,
	// 내용이 백틱으로 시작하거나 끝나면 공백을 하나 끼워 마커와 떼어 놓는다(CommonMark).
	// 노션은 줄을 넘는 코드 스팬도 줄마다 닫는다 — 울타리는 줄마다 잡는다.
	if o.emph == emphCode && v.lineEmphasis() && bytes.IndexByte((*out)[o.at:], '\n') >= 0 {
		in.wrap = wrapPerLine(out, o.at, "`", "`", true, in.wrap)
		return
	}
	if o.emph == emphCode && v.open(emphCode) == "`" {
		body := (*out)[o.at:]
		if longest := longestRun(body, '`'); longest > 0 {
			pad := body[0] == '`' || body[len(body)-1] == '`'
			fence := string(repeatRune('`', longest+1))
			if pad {
				*out = append(*out, ' ')
			}
			*out = append(*out, fence...)
			if pad {
				fence += " "
			}
			insertAt(out, o.at, fence)
			return
		}
	}
	// GitHub 이 이 짝을 강조로 읽지 않으면 태그로 낸다. GFM 은 CommonMark 의 flanking 규칙을
	// 따라서, 닫는 `**` 앞이 구두점이고 뒤에 글자가 오면 닫지 못한다 — `**설정(config)**을` 이
	// 별표째 글자로 남고, 짝이 뒤의 `**` 와 엇갈려 범위가 뒤집힌다(실측 2026-09-30).
	after := noChar
	if matched {
		after = in.afterClose
	}
	if v.htmlEmphasis() && o.emph != emphCode && !gfmPairs(o.before, (*out)[o.at:], after) {
		in.repairs.TagEmphasis++
		insertAt(out, o.at, v.openHTML(o.emph))
		*out = append(*out, v.closeHTML(o.emph)...)
		return
	}
	// 마크다운을 내는 채널에서 원문과 다른 마커로 썼으면 센다 — 러스트 쪽과 같다.
	if !v.htmlOut() && !v.isPlain() && o.emph != emphCode {
		w := v.open(o.emph)
		if utf8.RuneCountInString(w) != o.run || strings.Trim(w, string(o.ch)) != "" {
			in.repairs.ConvertedMarker++
		}
	}
	if v.lineEmphasis() && bytes.IndexByte((*out)[o.at:], '\n') >= 0 {
		in.wrap = wrapPerLine(out, o.at, v.open(o.emph), v.close(o.emph), false, in.wrap)
		return
	}
	insertAt(out, o.at, v.open(o.emph))
	*out = append(*out, v.close(o.emph)...)
}

// wrapPerLine 은 강조 범위(at 부터 끝)를 줄마다 감싼다 — 러스트 쪽 wrap_per_line. 줄 끝에서 닫고
// 다음 줄은 블록 층이 쓴 접두사(> ·들여쓰기) 뒤에서 다시 연다. 줄 끝 공백은 닫는 마커 밖으로
// 빼고, 내용이 없는 줄은 감싸지 않는다. 노션이 줄을 넘는 마커의 짝을 못 맞춰서다. code 면 코드
// 스팬이라 울타리를 줄마다 잡는다. 범위는 buf 에 옮겨 두고 다시 쓴다 — 재사용 버퍼를 돌려준다.
func wrapPerLine(out *[]byte, at int, open, close string, code bool, buf []byte) []byte {
	buf = append(buf[:0], (*out)[at:]...)
	*out = (*out)[:at]
	rest := buf
	for i := 0; ; i++ {
		line := rest
		nl := bytes.IndexByte(rest, '\n')
		if nl >= 0 {
			line = rest[:nl]
		}
		if i > 0 {
			*out = append(*out, '\n')
		}
		lead := 0
		if i > 0 {
			lead = len(line) - len(bytes.TrimLeft(line, " \t>"))
		}
		prefix, body := line[:lead], line[lead:]
		text := bytes.TrimRight(body, " \t")
		*out = append(*out, prefix...)
		switch {
		case len(text) == 0:
			*out = append(*out, body...)
		case code:
			fence := longestRun(text, '`') + 1
			pad := text[0] == '`' || text[len(text)-1] == '`'
			for k := 0; k < fence; k++ {
				*out = append(*out, '`')
			}
			if pad {
				*out = append(*out, ' ')
			}
			*out = append(*out, text...)
			if pad {
				*out = append(*out, ' ')
			}
			for k := 0; k < fence; k++ {
				*out = append(*out, '`')
			}
			*out = append(*out, body[len(text):]...)
		default:
			*out = append(*out, open...)
			*out = append(*out, text...)
			*out = append(*out, close...)
			*out = append(*out, body[len(text):]...)
		}
		if nl < 0 {
			return buf
		}
		rest = rest[nl+1:]
	}
}

// gfmPairs 는 GFM(CommonMark)이 before + 마커 + body + 마커 + after 를 강조로 읽는가다.
// 여는 마커는 좌측 flanking, 닫는 마커는 우측 flanking 이어야 한다. 구두점은 CommonMark 0.31
// 처럼 기호까지 친다 — 글자·숫자·공백이 아니면 구두점이다.
//
// 이웃이 태그 경계면 읽는다고 보지 않는다. before·after 는 원문 글자인데, 마커에 붙은 태그나
// 주석이 벗겨지면(`**x.**<font>y`) 출력의 이웃은 그 너머 글자가 된다. 태그 너머를 보려면 조각을
// 더 붙들어야 해서, 그 자리는 판정 없이 태그로 낸다.
func gfmPairs(before rune, body []byte, after rune) bool {
	if before == '>' || after == '<' {
		return false
	}
	if len(body) == 0 {
		return true
	}
	first, _ := utf8.DecodeRune(body)
	last, _ := utf8.DecodeLastRune(body)
	punct := func(c rune) bool { return !isAlphanumeric(c) && !unicode.IsSpace(c) }
	edge := func(c rune) bool { return c == noChar || unicode.IsSpace(c) || punct(c) }
	left := !unicode.IsSpace(first) && (!punct(first) || edge(before))
	right := !unicode.IsSpace(last) && (!punct(last) || edge(after))
	return left && right
}

// angle 은 `<…>` 를 읽는다 — 오토링크, 아는 HTML 태그, 주석. 셋 중 하나면 소비한 길이를
// 돌려주고, 아니면 0 이라 `<` 는 글자로 나간다. 아는 태그만 벗긴다 — `Vec<T>` 의 `<T>` 나
// `1 < 2` 를 태그로 읽으면 글이 사라진다.
// rendersEmpty 는 이 채널의 출력에서 아무것도 안 남기는가다 — 스페이스·탭, 주석, 이 채널이
// 벗기는 태그뿐인가. 러스트 쪽 renders_empty.
func rendersEmpty(rest []rune, v vocab) bool {
	for i := 0; i < len(rest); {
		switch {
		case rest[i] == ' ' || rest[i] == '\t':
			i++
		case startsWith(rest[i:], "<!--"):
			end := findSeq(rest[i+4:], "-->")
			if end < 0 {
				return false
			}
			i += 4 + end + 3
		case rest[i] == '<':
			at := i + 1
			if at < len(rest) && rest[at] == '/' {
				at++
			}
			j := at
			for j < len(rest) && isASCIIAlnum(rest[j]) {
				j++
			}
			name := rest[at:j]
			kept := inlineTag(name) >= 0 || eqIgnoreCase(name, "br")
			if len(name) == 0 || !isKnownTag(name) || kept && v.htmlEmphasis() {
				return false
			}
			end := -1
			for k := j; k < len(rest); k++ {
				if rest[k] == '>' {
					end = k
					break
				}
			}
			if end < 0 {
				return false
			}
			i = end + 1
		default:
			return false
		}
	}
	return true
}

// textChar 는 본문 글자 하나를 적는다. 채널이 이스케이프하는 글자면 센다(Repairs.EscapedChar).
func (in *inline) textChar(c rune, out *[]byte, v vocab) {
	if v.escapes(c) {
		in.repairs.EscapedChar++
	}
	v.escapeChar(c, out)
}

func (in *inline) angle(line []rune, i int, out *[]byte, v vocab) int {
	rest := line[i:]
	if startsWith(rest, "<!--") {
		end := findSeq(rest[4:], "-->")
		if end < 0 {
			return 0
		}
		in.repairs.StrippedHTML++
		return end + 4 + 3
	}
	closeAt := indexRune(rest, '>')
	if closeAt < 0 {
		return 0
	}
	if startsWith(rest[1:], "http://") || startsWith(rest[1:], "https://") {
		body := rest[1:closeAt]
		// `<url|텍스트>` 는 슬랙 레거시 링크다. 주소와 텍스트를 가른다.
		url, label, bare := body, body, true
		if bar := indexRune(body, '|'); bar >= 0 {
			url, label, bare = body[:bar], body[bar+1:], false
		}
		for _, c := range url {
			if unicode.IsSpace(c) {
				return 0
			}
		}
		// 오토링크의 텍스트는 인라인으로 다시 읽지 않는다 — 주소 안의 `_` 가 기울임이 되면 안 된다.
		// 텍스트 없는 `<url>` 의 라벨은 주소 그대로다 — 본문 이스케이프(GitHub 의 `\~`)을 하면 주소와
		// 달라져 오토링크 대신 `[…](…)` 로 풀린다. HTML 로 가는 채널만 escape 한다.
		text := in.scratch[:0]
		for _, c := range label {
			if bare {
				v.codeChar(c, &text)
			} else {
				in.textChar(c, &text, v)
			}
		}
		if !bare && !v.htmlOut() && !v.isPlain() {
			in.repairs.ConvertedMarker++
		}
		v.link(string(text), string(url), out)
		in.scratch = text
		in.prev = '>'
		return closeAt + 1
	}
	closing := len(rest) > 1 && rest[1] == '/'
	nameAt := 1
	if closing {
		nameAt = 2
	}
	j := nameAt
	for j < closeAt && isASCIIAlnum(rest[j]) {
		j++
	}
	name := rest[nameAt:j]
	// 이름 뒤는 속성(공백)이거나 `/>` 거나 바로 `>` 다. 아니면 태그 모양이 아니다.
	if len(name) == 0 || !(rest[j] == '>' || rest[j] == '/' || unicode.IsSpace(rest[j])) {
		return 0
	}
	if !isKnownTag(name) {
		return 0
	}
	// GitHub 은 인라인 태그를 그린다 — 벗기지 않고 그대로 둔다(실측 2026-09-30). 줄 첫머리의
	// 태그는 그 줄이 태그뿐이면 GFM 이 HTML 블록을 열어 빈 줄까지 마크다운을 안 읽고, 블록
	// 태그는 자리와 무관하게 그런다. 줄이 태그뿐인지는 줄 끝까지 봐야 알아서, 스트리밍이
	// 붙들지 않도록 첫머리면 벗긴다.
	//
	// 브라우저 채널은 자리와 상관없이 살린다 — 마크다운으로 다시 읽히지 않는다.
	//
	// 살릴 때는 속성을 버리고 이름만 다시 쓰고, 강조와 같은 스택에 올린다. 출력이 innerHTML 로
	// 들어가는 채널에서 `<span onclick=…>` 을 그대로 내면 안 되고, 원문의 태그는 짝이 안
	// 맞거나(`<sub>` 만 열고 끝) 강조와 엇갈리기(`**a<sub>b**c</sub>`) 일쑤다(퍼즈가 잡았다).
	// 스택에 올리면 강조와 같은 규칙으로 닫히고 중첩이 바르다 — 짝 없는 닫는 태그는 버리고,
	// 안 닫힌 여는 태그는 블록 끝에서 닫는다.
	tag := inlineTag(name)
	br := eqIgnoreCase(name, "br")
	if (v.isHTML() || v.htmlEmphasis()) && (tag >= 0 || br) {
		p := in.prevChar(line, i)
		// 표 칸 첫머리는 `| ` 뒤라 줄 첫머리가 아니다.
		atLineStart := !in.inCell && (p == noChar || p == '\n')
		// 태그뿐인 줄만 벗긴다 — 여는 태그 뒤가 줄 끝까지 공백이면 GFM 이 HTML 블록을 연다.
		// 꼬리말 <sub>모델 · 토큰</sub> 처럼 뒤에 글이 오면 살린다. 태그뿐인지는 출력으로 본다 —
		// 뒤에 주석이나 벗기는 태그만 있으면 출력에는 이 태그 하나만 남는다. 러스트 쪽과 같다.
		tagOnlyLine := rendersEmpty(rest[closeAt+1:], v)
		githubStart := !v.isHTML() && atLineStart && tagOnlyLine
		n := len(in.strippedTags)
		switch {
		case closing && !v.isHTML() && tag >= 0 && n > 0 && in.strippedTags[n-1] == uint8(tag):
			// 여는 쪽을 벗겼다 — 닫는 쪽만 남기지 않는다. 이름이 맞을 때만이다 —
			// `<sub>a <kbd>x</kbd></sub>` 의 `</kbd>` 를 벗기면 `</sub>` 만 홀로 남는다.
			in.strippedTags = in.strippedTags[:n-1]
		case githubStart:
			if !closing && tag >= 0 {
				in.strippedTags = append(in.strippedTags, uint8(tag))
			}
		default:
			switch {
			case tag < 0:
				if !closing {
					*out = append(*out, "<br>"...)
				}
			case closing:
				if at := in.lastOpen(tagEmph(tag)); at >= 0 {
					in.afterClose = noChar
					in.closeAt(at, out, v)
				}
			default:
				in.open = append(in.open, openMark{
					emph:   tagEmph(tag),
					at:     len(*out),
					run:    1,
					ch:     '<',
					before: p,
				})
			}
			in.prev = '>'
			return closeAt + 1
		}
	}
	// 여기까지 오면 태그를 벗긴다(GitHub 칸 안의 <br> 만 그대로 둔다).
	if !(in.inCell && v.htmlEmphasis() && !closing && br) {
		in.repairs.StrippedHTML++
	}
	if !closing && br {
		// 표 칸 안의 `<br>` 은 줄바꿈으로 못 바꾼다 — 칸 안에 `\n` 이 들어가면 GFM 은 그 뒤를
		// 새 행으로 읽어 내용이 엉뚱한 열로 간다. 그걸 그리는 GitHub 에는 그대로 두고,
		// 나머지는 공백으로 편다.
		switch {
		case v.keepsBr():
			// 노션은 칸 안이든 밖이든 <br> 을 그린다 — \n 으로 바꾸면 인용이 갈리고 강조가 줄을 넘는다.
			*out = append(*out, "<br>"...)
			in.prev = ' '
		case in.inCell && v.htmlEmphasis():
			*out = append(*out, "<br>"...)
			in.prev = ' '
		case in.inCell:
			*out = append(*out, ' ')
			in.prev = ' '
		default:
			*out = append(*out, '\n')
			in.prev = '\n'
		}
	}
	return closeAt + 1
}

func (in *inline) renderLink(text, url []rune, out *[]byte, v vocab) {
	in.renderTarget(text, url, out, v, false)
}

func (in *inline) renderImage(alt, url []rune, out *[]byte, v vocab) {
	in.renderTarget(alt, url, out, v, true)
}

func (in *inline) renderTarget(text, url []rune, out *[]byte, v vocab, image bool) {
	scratch := in.scratch[:0]
	// 링크 텍스트는 자기만의 인라인 상태로 렌더한다. 바깥 강조와 섞이지 않는다.
	nested := newInline()
	nested.render(text, &scratch, v)
	nested.finishBlock(&scratch, v)
	in.repairs.add(nested.repairs)
	if image {
		v.image(string(scratch), string(url), out)
	} else {
		v.link(string(scratch), string(url), out)
	}
	in.scratch = scratch
}

// findLink 는 `[텍스트](url)` 을 찾는다. 한 줄 안에서만 본다(SPEC 8절).
// 돌려주는 것은 텍스트 구간, 주소 구간, 찾았는가.
func findLink(line []rune, at int) (t0, t1, u0, u1 int, ok bool) {
	j := at + 1
	depth := 0
	for j < len(line) {
		if line[j] == '[' {
			depth++
		} else if line[j] == ']' {
			if depth == 0 {
				break
			}
			depth--
		}
		j++
	}
	if j+1 >= len(line) || line[j+1] != '(' {
		return 0, 0, 0, 0, false
	}
	k := indexRune(line[j+2:], ')')
	if k < 0 {
		return 0, 0, 0, 0, false
	}
	return at + 1, j, j + 2, j + 2 + k, true
}

// isKnownTag 는 벗겨도 되는 HTML 태그다. 마크다운이 못 적는 표현을 LLM 이 HTML 로 메울 때
// 쓰는 것들이다. 링크(`<a>`)는 없다 — 벗기면 주소가 사라진다.
// inlineTag 는 살려 둘 인라인 태그의 번호(inlineTags)다. 없으면 -1. <br> 은 짝이 없어 여기 없다.
func inlineTag(name []rune) int {
	for k, t := range inlineTags {
		if eqIgnoreCase(name, t[0]) {
			return k
		}
	}
	return -1
}

func isKnownTag(name []rune) bool {
	for _, t := range [...]string{
		"br", "sub", "sup", "b", "strong", "i", "em", "u", "s", "strike", "del", "code", "span",
		"div", "p", "small", "mark", "kbd", "font", "center", "details", "summary", "ins",
	} {
		if eqIgnoreCase(name, t) {
			return true
		}
	}
	return false
}

// canOpen 은 이 마커가 열 수 있는가다(CommonMark 의 좌측 flanking). noChar 는 블록의 끝
// (또는 시작)이고 공백처럼 다룬다. 앞이 글자·숫자가 아니면(공백, 구두점, `①`·`🔥` 같은
// 기호) 열 수 있다 — LLM 은 항목 머리에 기호를 붙인다.
//
// 닫는 쪽은 우측 flanking 이 아니다. CommonMark 은 앞이 구두점이고 뒤가 글자면 닫지 못하게
// 하는데 한국어 출력이 거기 정면으로 걸린다(`**끝.**이라서`). 우리 목적은 스펙 준수가 아니라
// 복구라, 닫는 판정은 "같은 종류가 열려 있고 앞이 공백이 아니면 닫는다" 하나로 간다.
func canOpen(prev, next rune) bool {
	if next == noChar || unicode.IsSpace(next) {
		return false
	}
	return !isPunct(next) || prev == noChar || !isWordChar(prev)
}

// isAlphabetic 은 러스트 char::is_alphabetic — 유니코드 Alphabetic 속성이다. Go 의 IsLetter(L
// 범주)보다 넓다: 글자 수 Nl 과 Other_Alphabetic(데바나가리 모음 부호 같은 결합 문자)이
// 들어간다. 여기가 갈리면 `का_x` 의 `_` 를 러스트는 글자로 두고 Go 는 강조로 연다.
func isAlphabetic(c rune) bool {
	return unicode.IsLetter(c) || unicode.Is(unicode.Nl, c) || unicode.Is(unicode.Other_Alphabetic, c)
}

// isWordChar 는 강조 마커 앞뒤의 "글자"다. 알파벳(한글 포함)과 ASCII 숫자 — `①` 은 아니다.
func isWordChar(c rune) bool {
	return isAlphabetic(c) || (c >= '0' && c <= '9')
}

// isAlphanumeric 은 러스트 char::is_alphanumeric 의 자리다 — Alphabetic 또는 Numeric(Nd·Nl·No).
func isAlphanumeric(c rune) bool {
	return isAlphabetic(c) || unicode.IsNumber(c)
}

func isASCIIPunct(c rune) bool {
	return c < 0x80 && (unicode.IsPunct(c) || unicode.IsSymbol(c))
}

func isASCIIAlnum(c rune) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')
}

func isPunct(c rune) bool {
	if isASCIIPunct(c) {
		return true
	}
	switch c {
	case '，', '。', '、', '！', '？', '；', '：', '·', '…', '—', '～',
		'「', '」', '『', '』', '（', '）', '【', '】', '《', '》',
		'“', '”', '‘', '’':
		return true
	}
	return false
}

func allSpaces(b []byte) bool {
	for _, x := range b {
		if x != ' ' {
			return false
		}
	}
	return true
}

func repeatRune(c rune, n int) []rune {
	r := make([]rune, n)
	for i := range r {
		r[i] = c
	}
	return r
}

// insertMarker 는 짝을 못 찾은 마커를 글자로 되돌려 at 에 끼운다. 본문 글자라 채널의 이스케이프를
// 따른다 — GitHub 에서 맨몸 `~` 로 되돌리면 뒤의 `~` 와 짝지어 취소선이 된다.
func insertMarker(out *[]byte, at int, c rune, n int, v vocab, repairs *Repairs) {
	if !v.escapes(c) {
		insertRun(out, at, c, n)
		return
	}
	repairs.EscapedChar += n
	// 문자열을 만들지 않고 자리를 늘려 `\` 와 마커를 번갈아 채운다 — 조각마다 불리는 경로다.
	insertRun(out, at, c, 2*n)
	for k := 0; k < n; k++ {
		(*out)[at+2*k] = '\\'
	}
}
