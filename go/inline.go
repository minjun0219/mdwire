package mdwire

import (
	"bytes"
	"slices"
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
	// hemmed 는 여는 쪽이 막힌 마커인가다 — 앞이 글자이고 뒤가 구두점(`값**(합계)**를`). 열린 줄 번호에
	// 1 을 더해 든다(0 은 아님). 같은 줄에서 거울 모양으로 닫는 마커가 오면 짝을 맺는다. 러스트 쪽과 같다.
	hemmed uint32
	// line 은 열린 줄 번호다. 안 닫힌 코드 스팬을 되돌려 다시 읽을 때 줄 번호를 여기로 되감는다.
	line uint32
}

type inline struct {
	// 링크·이미지 라벨의 재귀 깊이. 러스트와 같은 한도로 호출 스택을 보호한다.
	targetDepth uint8
	// 실제로 내보낼 <a> 안의 라벨을 렌더하는가. <a> 안에 <a>를 만들 수 없다.
	inAnchor bool
	// `[`가 든 줄의 대괄호·주소 색인(indexLinks). 비우기만 하고 재사용한다.
	links []linkBounds
	open  []openMark
	// 지금 몇 번째 줄인가. 막힌 여는 마커는 같은 줄에서만 짝을 맺는다.
	line uint32
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
	// boldLine 은 블록 층이 줄 전체를 굵게 감쌌는가다(헤딩 구문이 없는 채널의 헤딩). 블록이 끝나면 내린다.
	boldLine bool
	// lastClose 는 mrkdwn 에서 마지막으로 닫은 강조의 닫는 마커 글자다(mrkdwnBounds).
	lastClose rune
	// enclosing 은 링크 라벨을 그리는 복제본에게 바깥에서 열려 있던 mrkdwn 마커다(* _ ~ 비트, markerBit).
	// 라벨은 자기 인라인 상태로 그려서, 이게 없으면 바깥 굵게를 모른다. 러스트 쪽과 같다.
	enclosing uint8
}

func newInline() *inline {
	return &inline{prev: noChar, afterClose: noChar, lastClose: noChar}
}

// reset 은 블록 경계다. 인라인 상태는 블록을 넘지 않는다.
func (in *inline) reset() {
	in.open = in.open[:0]
	in.prev = noChar
	in.strippedTags = in.strippedTags[:0]
	in.boldLine = false
	in.lastClose = noChar
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
func (in *inline) endLine() {
	in.line++
	in.prev = '\n'
}

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
	// 대괄호가 없는 산문은 색인하지 않는다. 버퍼는 빌려 와 재사용한다 — 되돌린 코드 스팬을 다시
	// 읽는 render 가 같은 배열을 덮어쓰지 않게 비워 둔다.
	links := in.links[:0]
	in.links = nil
	indexed := false
	// 위키링크가 닫히지 않는다고 이미 본 자리. 그 앞에서 시작하는 `[[` 는 같은 꼬리를 다시 훑지 않는다.
	unclosedUntil := 0
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
				// 닫는 백틱 뒤 글자다 — mrkdwn 은 코드 스팬도 글자에 붙으면 뗀다(mrkdwnBounds).
				in.afterClose = noChar
				if i+run < len(line) {
					in.afterClose = line[i+run]
				}
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
		// 되돌린 코드 스팬을 다시 읽을 때만 줄바꿈이 여기 온다 — 되감은 줄을 도로 센다.
		if c == '\n' {
			in.line++
		}

		// 이미지 `![alt](url)`. 대체 글은 링크 텍스트처럼 인라인으로 읽는다.
		if c == '!' && i+1 < len(line) && line[i+1] == '[' {
			if !indexed {
				links = indexLinks(line, links)
				indexed = true
			}
			if t0, t1, u0, u1, ok := findLink(line, i+1, links); ok {
				in.renderImage(line[t0:t1], line[u0:u1], out, v)
				i = u1 + 1
				continue
			}
		}

		// 옵시디언의 위키링크는 통째로 글자 그대로 낸다(vocab.wikilinks). 첨부를 붙이는 `![[…]]` 는 위의
		// 이미지 자리에서 `!` 가 글자로 나가고 여기로 온다 — 러스트 쪽과 같다.
		if c == '[' && v.wikilinks() && i >= unclosedUntil && i+1 < len(line) && line[i+1] == '[' {
			n, stop := wikilinkLen(line[i:])
			if n > 0 {
				for _, r := range line[i : i+n] {
					*out = appendRune(*out, r)
				}
				i += n
				continue
			}
			unclosedUntil = i + stop
		}

		if c == '[' {
			if !indexed {
				links = indexLinks(line, links)
				indexed = true
			}
			if t0, t1, u0, u1, ok := findLink(line, i, links); ok {
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
			if v.isMrkdwn() {
				prev := in.prevChar(line, i)
				// 줄 첫머리의 `\>` 는 인용이 아니다. 슬랙은 인용을 `&gt;` 로 시작하는 줄로 적어서, 그대로 내면
				// 저자가 막은 인용이 다시 열린다. U+200B 를 앞에 두어 줄 첫머리에서 뗀다. 러스트 쪽과 같다.
				if line[i+1] == '>' && (prev == noChar || prev == '\n') {
					*out = appendRune(*out, zeroWidthSpace)
				}
				next := noChar
				if i+2 < len(line) {
					next = line[i+2]
				}
				in.textCharAfter(line[i+1], prev, next, out, v)
				in.prev = line[i+1]
				i += 2
				continue
			}
			v.literal(line[i+1], out)
			in.prev = line[i+1]
			i += 2
			continue
		}

		if c == '`' {
			run := runLen(line, i, '`')
			prev := in.prevChar(line, i)
			in.codeSrc = in.codeSrc[:0]
			in.open = append(in.open, openMark{emph: emphCode, at: len(*out), run: run, ch: '`', afterSpace: prev == noChar || unicode.IsSpace(prev), before: prev, line: in.line})
			i += run
			continue
		}

		if c != '*' && c != '_' && c != '~' {
			// 앞뒤 글자는 mrkdwn 만 본다 — 글자마다 부르는 길이라 다른 채널은 안 꺼낸다.
			in.textCharAfter(c, noChar, noChar, out, v)
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
			next := noChar
			if i+1 < len(line) {
				next = line[i+1]
			}
			in.textCharAfter(c, in.prevChar(line, i), next, out, v)
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
		// `2026-04-29_제목` 도 글자로 남는다. 대신 닫는 것은 된다 — `_진료_가`. 닫는 것은 `_` 로
		// 연 것뿐이다 — `2*3 = 6, snake_case` 의 밑줄이 `*` 로 연 기울임을 닫으면 `snake*case` 가 된다.
		intraword := c == '_' && prev != noChar && isAlphanumeric(prev)
		if intraword && (same < 0 || in.open[same].ch != '_') {
			in.textRun(c, take, prev, next, out, v)
			i += take
			continue
		}

		left := canOpen(prev, next) && !intraword
		// 여는 쪽이 막힌 자리인가 — 앞이 글자, 뒤가 구두점. 러스트 쪽 hemmed_here.
		hemmedHere := prev != noChar && isWordChar(prev) && next != noChar && !unicode.IsSpace(next) && isPunct(next)
		fresh := openMark{emph: e, run: take, ch: c, afterSpace: afterSpace, before: prev, split: split}

		switch {
		// 추측으로 연 것은 닫지 않는다. 추측은 확정되지 않는다 — 닫아 주면 여는 쪽은 사라지고
		// 닫는 쪽만 없어져 `underfront.*`·`/* 주석 */` 의 별표가 없어졌다. 앞이 글자인 마커도
		// 추측을 닫지 않고 열지도 않는다 — 열면 블록 끝까지 삼킨다. 글자다.
		// 막힌 여는 마커의 거울 짝 — 러스트 쪽과 같다(`값**(합계)**를`).
		// 앞이 여는 괄호·따옴표면 거울이 아니라 다음 강조의 여는 마커다(`2**(n-1) (**주의**)`).
		// 양 끝이 다 ASCII 영숫자면 수식이라 짝짓지 않는다(`x**(y)**z`).
		case same >= 0 && in.open[same].hemmed == in.line+1 && in.open[same].run == take &&
			prev != noChar && !isWordChar(prev) && !unicode.IsSpace(prev) && !isOpeningBracket(prev) &&
			next != noChar && isWordChar(next) &&
			!(isASCIIAlnum(in.open[same].before) && next != noChar && isASCIIAlnum(next)):
			in.open[same].guess = false
			in.repairs.GuessedPair++
			in.afterClose = next
			in.closeAt(same, out, v)
		// 막힌 여는 마커가 또 왔다 — 앞의 것은 글자로 되돌리고 이쪽을 새 막힌 마커로 연다. 러스트 쪽과 같다.
		case same >= 0 && in.open[same].hemmed != 0 && hemmedHere:
			in.reopenAt(same, out, v, fresh)
			top := &in.open[len(in.open)-1]
			top.guess = true
			top.hemmed = in.line + 1
		case same >= 0 && in.open[same].guess && (!left || !afterSpace) &&
			!(left && in.open[same].hemmed != 0 && isOpeningBracket(prev)):
			in.textRun(c, take, prev, next, out, v)
		// 여는 자리의 마커가 왔는데 추측이 열려 있다 — 추측이 틀렸다. 되돌리고 이쪽을 연다.
		// 막힌 추측 뒤의 `(**주의` 도 여기다 — 막힌 마커는 글자로 돌아간다.
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
			if hemmedHere {
				fresh.hemmed = in.line + 1
			}
			in.open = append(in.open, fresh)
		}
		i += take
	}
	in.links = links[:0]
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
	if v.isMrkdwn() {
		// 글자로 되돌린 런이다 — mrkdwn 에서는 백틱끼리 뗀다(insertMarker).
		var discard Repairs
		insertMarker(out, o.at, o.ch, o.run, o.before, false, v, &discard)
	} else {
		for k := 0; k < o.run; k++ {
			*out = appendRune(*out, o.ch)
		}
	}
	// 버퍼를 통째로 빌려 와서 다시 읽고 돌려준다. 새로 만들지 않는다.
	src := in.codeSrc
	in.codeSrc = nil
	// 줄 번호는 코드 스팬이 열린 줄로 되감고 다시 읽으며 센다 — 안 그러면 막힌 마커의 "같은 줄"
	// 검사가 줄을 넘어 통과한다. 다 읽으면 블록 층이 세어 둔 값으로 돌려놓는다. 러스트 쪽과 같다.
	line := in.line
	in.line = o.line
	in.prev = o.ch
	in.render(src, out, v)
	in.line = line
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
	if old.run == 1 || (old.guess && (old.afterSpace || old.split || old.hemmed != 0)) {
		insertMarker(out, old.at, old.ch, old.run, old.before, in.inside(old.ch, v), v, &in.repairs)
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
		// 막힌 여는 마커도 되돌린다 — 짝 없는 `2**(n-1)` 의 `**` 는 거듭제곱이다.
		if o.afterSpace || o.run == 1 || o.split || o.hemmed != 0 {
			insertMarker(out, o.at, o.ch, o.run, o.before, in.inside(o.ch, v), v, &in.repairs)
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
		in.mrkdwnBounds(out, o.at, o.before, afterOf(matched, in.afterClose), v)
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
			in.mrkdwnBounds(out, o.at, o.before, afterOf(matched, in.afterClose), v)
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
	// mrkdwn 은 굵게 안의 굵게를 못 쓴다 — 마커가 같은 * 라 `*배포 *목록**` 이 바깥에서 닫힌다. 이미 굵은
	// 자리라 안쪽 마커는 버려도 모양이 같다. 헤딩을 굵은 줄로 낼 때 흔하다(`## 배포 **목록**`). 링크 라벨
	// 안의 굵게도 바깥이 굵으면 버린다(enclosing) — `*제목 <주소|*링크*>*` 가 된다.
	if v.isMrkdwn() && o.emph == emphBold && in.inside('*', v) {
		return
	}
	if v.htmlEmphasis() && o.emph != emphCode && !gfmPairs(o.before, (*out)[o.at:], after) {
		in.repairs.TagEmphasis++
		insertAt(out, o.at, v.openHTML(o.emph))
		*out = append(*out, v.closeHTML(o.emph)...)
		return
	}
	// 슬랙은 태그가 없어서 마커 안쪽에 U+2060(워드 조이너)을 끼운다 — 러스트 쪽과 같다(실측
	// 2026-10-01). 못 읽는 쪽에만 넣는다.
	if v.joinerEmphasis() && o.emph != emphCode {
		if left, right := gfmSides(o.before, (*out)[o.at:], after); !(left && right) {
			in.repairs.TagEmphasis++
			// 안쪽 강조의 마커가 맞붙어 있으면 그쪽에도 끼운다 — 러스트 쪽과 같다(`***중요(필수)***를`).
			punct := func(c rune) bool { return !isAlphanumeric(c) && !unicode.IsSpace(c) && c != '\u2060' }
			isMarker := func(c byte) bool { return c == '*' || c == '_' || c == '~' }
			if !right {
				body := (*out)[o.at:]
				run := 0
				for run < len(body) && isMarker(body[len(body)-1-run]) {
					run++
				}
				if run > 0 {
					if c, _ := utf8.DecodeLastRune(body[:len(body)-run]); c != utf8.RuneError && punct(c) {
						insertAt(out, len(*out)-run, wordJoiner)
					}
				}
				*out = append(*out, wordJoiner...)
			}
			if !left {
				body := (*out)[o.at:]
				run := 0
				for run < len(body) && isMarker(body[run]) {
					run++
				}
				if run > 0 {
					if c, _ := utf8.DecodeRune(body[run:]); c != utf8.RuneError && punct(c) {
						insertAt(out, o.at+run, wordJoiner)
					}
				}
				insertAt(out, o.at, wordJoiner)
			}
		}
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
		in.mrkdwnBounds(out, o.at, o.before, after, v)
		return
	}
	insertAt(out, o.at, v.open(o.emph))
	*out = append(*out, v.close(o.emph)...)
	in.mrkdwnBounds(out, o.at, o.before, after, v)
}

// mrkdwnBounds: mrkdwn 은 단어 경계에서만 마커를 읽는다 — 마커 바깥에 붙은 글자가 있으면 U+200B 로 뗀다.
// 조사가 붙는 한국어(`*마통*이`)는 이 모양이 기본이다. 슬랙에서 재지는 않았다. 앞은 원문에서 마커 바로
// 앞 글자, 뒤는 닫는 마커 다음 글자로 본다 — 러스트 쪽 mrkdwn_bounds.
func (in *inline) mrkdwnBounds(out *[]byte, at int, before, after rune, v vocab) {
	if !v.isMrkdwn() {
		return
	}
	// 원문의 이웃이 태그 꺾쇠면 그 태그는 벗겨져서 너머의 글자가 붙을 수 있다 — 늘 뗀다. 앞이 마커 글자이고
	// 바로 앞 강조가 같은 마커로 닫혔으면 둘이 붙어 런이 된다(`**a**__b__`). out 은 스트리밍 중에 비어 있을 수
	// 있어서 닫은 마커를 따로 기억해 둔다(lastClose) — 러스트 쪽과 같다.
	openC, _ := utf8.DecodeRune((*out)[at:])
	markerBefore := (before == '*' || before == '_' || before == '~') && in.lastClose == openC
	left := glued(before) || before == '>' || markerBefore
	right := glued(after) || after == '<'
	in.lastClose = noChar
	if r, n := utf8.DecodeLastRune(*out); n > 0 {
		in.lastClose = r
	}
	if left || right {
		in.repairs.TagEmphasis++
	}
	if right {
		*out = append(*out, zeroWidthSpaceStr...)
	}
	if left {
		insertAt(out, at, zeroWidthSpaceStr)
	}
}

// glued 는 마커 바깥에 붙은 글자·숫자인가다.
func glued(c rune) bool { return c != noChar && isAlphanumeric(c) }

// afterOf 는 닫는 마커 다음 글자다. 저자 대신 닫은 것이면 없다.
func afterOf(matched bool, afterClose rune) rune {
	if matched {
		return afterClose
	}
	return noChar
}

// runPrev 는 런의 k 번째 글자에게 앞 글자다 — 첫 글자는 원문의 앞 글자, 나머지는 런의 앞 글자.
func runPrev(k int, prev, c rune) rune {
	if k == 0 {
		return prev
	}
	return c
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
	left, right := gfmSides(before, body, after)
	return left && right
}

// gfmSides 는 gfmPairs 를 양쪽으로 나눠 본다 — 여는 마커가 열리는가, 닫는 마커가 닫히는가.
func gfmSides(before rune, body []byte, after rune) (bool, bool) {
	if len(body) == 0 {
		return true, true
	}
	first, _ := utf8.DecodeRune(body)
	last, _ := utf8.DecodeLastRune(body)
	punct := func(c rune) bool { return !isAlphanumeric(c) && !unicode.IsSpace(c) }
	edge := func(c rune) bool { return c == noChar || unicode.IsSpace(c) || punct(c) }
	left := before != '>' && !unicode.IsSpace(first) && (!punct(first) || edge(before))
	right := after != '<' && !unicode.IsSpace(last) && (!punct(last) || edge(after))
	return left, right
}

// wordJoiner 는 슬랙에서 마커가 못 읽히는 자리에 끼우는 글자다. 보이지 않고 줄도 가르지 않는다.
const wordJoiner = "\u2060"

// zeroWidthSpace 는 mrkdwn 에서 마커와 붙은 글자를 떼는 글자다(mrkdwnBounds).
const (
	zeroWidthSpace    = '\u200B'
	zeroWidthSpaceStr = "\u200B"
)

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

// textChars 는 본문 글자 여럿을 적는다. 앞뒤 글자는 이 글 안에서 본다 — 오토링크 라벨처럼 따로 쓰는 글이다.
func (in *inline) textChars(s []rune, out *[]byte, v vocab) {
	for k, c := range s {
		prev, next := noChar, noChar
		if v.isMrkdwn() {
			if k > 0 {
				prev = s[k-1]
			} else if r, n := utf8.DecodeLastRune(*out); n > 0 {
				prev = r
			}
			if k+1 < len(s) {
				next = s[k+1]
			}
		}
		in.textCharAfter(c, prev, next, out, v)
	}
}

// textRun 은 마커 글자 런을 본문 글자로 적는다. 글자마다 앞뒤를 따로 본다 — 런 안의 둘째 글자는 앞이
// 마커라 여는 자리일 수 있다(`MY__VAR` 의 둘째 `_`).
func (in *inline) textRun(c rune, n int, prev, next rune, out *[]byte, v vocab) {
	for k := 0; k < n; k++ {
		nx := next
		if k+1 < n {
			nx = c
		}
		in.textCharAfter(c, runPrev(k, prev, c), nx, out, v)
	}
}

// inside 는 지금 mrkdwn 마커 c 로 그리는 강조 안인가다 — 바깥 라벨, 굵은 줄, 열린 마커 어디서든. 그
// 안에서는 글자로 쓴 같은 마커가 닫는 쪽으로 읽힐 수 있다(mrkdwnAlike).
func (in *inline) inside(c rune, v vocab) bool {
	if in.enclosing&markerBit(c) != 0 || (c == '*' && in.boldLine) {
		return true
	}
	for _, o := range in.open {
		if m := v.open(o.emph); m != "" && rune(m[0]) == c {
			return true
		}
	}
	return false
}

// textCharAfter 는 글자 하나를 본문으로 적는다. prev·next 는 원문에서 바로 앞뒤 글자다 — out 은 스트리밍
// 중에 앞이 내보내져 비어 있을 수 있어서, 앞뒤 글자로 갈리는 판단은 원문으로 한다. mrkdwn 만 본다.
//
// mrkdwn 에는 이스케이프가 없다. 글자로 쓴 * _ ~ 가 마커로 읽힐 수 있는 자리에서는 모양이 닮은 글자로
// 쓴다(mrkdwnAlike). 글자 백틱은 앞이 백틱이면 U+200B 로 뗀다 — 런이 되면 코드 블록을 연다. 슬랙에서
// 재지는 않았다. 러스트 쪽과 같다.
func (in *inline) textCharAfter(c, prev, next rune, out *[]byte, v vocab) {
	if v.isMrkdwn() {
		inside := (c == '*' || c == '_' || c == '~') && in.inside(c, v)
		if a := mrkdwnAlike(c, prev, next, inside); a != noChar {
			in.repairs.EscapedChar++
			*out = appendRune(*out, a)
			return
		}
		if c == '`' {
			if prev == '`' || (len(*out) > 0 && (*out)[len(*out)-1] == '`') {
				*out = appendRune(*out, zeroWidthSpace)
			}
			*out = append(*out, '`')
			return
		}
	}
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
		if bare {
			for _, c := range label {
				v.codeChar(c, &text)
			}
		} else {
			in.textChars(label, &text, v)
		}
		if !bare && !v.htmlOut() && !v.isPlain() {
			in.repairs.ConvertedMarker++
		}
		if in.inAnchor {
			v.unlinked(string(text), string(url), out)
		} else {
			v.link(string(text), string(url), out)
		}
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
	href := string(url)
	loads := image && v.loadsImage(href)
	labelVocab := v
	if loads {
		labelVocab = newVocab(Plain, Options{})
	}
	// 링크 텍스트는 자기만의 인라인 상태로 렌더한다. 바깥 강조와 섞이지 않는다.
	if in.targetDepth < maxTargetDepth {
		nested := newInline()
		nested.targetDepth = in.targetDepth + 1
		// 라벨이 들어갈 자리가 실제 <a>일 때만 안쪽 링크를 글자로 내린다.
		nested.inAnchor = in.inAnchor || (!loads && v.makesAnchor(href))
		// 표 칸 안의 라벨도 줄을 바꿀 수 없다.
		nested.inCell = in.inCell
		// 바깥에서 열린 mrkdwn 마커를 넘긴다 — 라벨 안의 같은 마커가 바깥 강조와 엉키지 않게.
		if v.isMrkdwn() {
			for _, m := range []rune{'*', '_', '~'} {
				if in.inside(m, v) {
					nested.enclosing |= markerBit(m)
				}
			}
		}
		nested.render(text, &scratch, labelVocab)
		nested.finishBlock(&scratch, labelVocab)
		in.repairs.add(nested.repairs)
	} else {
		// 한도 아래와 같은 글자 escape를 쓰되, 다시 파싱하지 않는다.
		in.textChars(text, &scratch, labelVocab)
	}
	// <a> 안의 링크와 이미지 링크 폴백은 주소를 글자로 보존한다.
	if in.inAnchor && !loads {
		v.unlinked(string(scratch), href, out)
	} else if image {
		v.image(string(scratch), href, out)
	} else {
		v.link(string(scratch), href, out)
	}
	in.scratch = scratch
}

const maxTargetDepth uint8 = 32

type linkBounds struct{ bracket, paren int }

// 대괄호 짝과 주소 끝을 역순 한 번으로 links 에 색인한다. 짝 없는 `[`마다 다시 훑지 않는다.
// `]`의 bracket으로 다음 `]`를 이어 별도 스택 없이 짝을 맞춘다.
func indexLinks(line []rune, links []linkBounds) []linkBounds {
	missing := len(line)
	links = slices.Grow(links[:0], len(line))[:len(line)]
	closeAt, paren := missing, missing
	for i := len(line) - 1; i >= 0; i-- {
		links[i] = linkBounds{missing, missing}
		switch line[i] {
		case ')':
			paren = i
		case ']':
			links[i] = linkBounds{closeAt, paren}
			closeAt = i
		case '[':
			if closeAt < missing {
				end := closeAt
				closeAt = links[end].bracket
				if end+1 < len(line) && line[end+1] == '(' && links[end].paren < missing {
					links[i] = linkBounds{end, links[end].paren}
				}
			}
		}
	}
	return links
}

// findLink 는 한 줄 안의 `[텍스트](url)`을 색인에서 찾는다(SPEC 8절).
func findLink(line []rune, at int, links []linkBounds) (t0, t1, u0, u1 int, ok bool) {
	b := links[at]
	if b.bracket >= len(line) {
		return 0, 0, 0, 0, false
	}
	return at + 1, b.bracket, b.bracket + 2, b.paren, true
}

// wikilinkLen 은 `[[` 로 시작하는 s 에서 위키링크의 길이(`]]` 까지)다. 같은 줄 안에서 닫혀야 하고,
// 비어 있으면 위키링크가 아니다(0) — 러스트 쪽 wikilink_len. 아니면 stop 은 훑다 멈춘 자리다. 그 앞에서
// 시작하는 `[[` 도 같은 꼬리를 보니 닫히지 않는다 — 그 자리를 잊고 `[[` 마다 다시 훑으면 `[[[[…`에서
// O(n²)이 된다.
func wikilinkLen(s []rune) (n, stop int) {
	for j := 2; j+1 < len(s); j++ {
		if s[j] == '\n' {
			return 0, j
		}
		if s[j] == ']' && s[j+1] == ']' {
			if j > 2 {
				return j + 2, 0
			}
			return 0, j
		}
	}
	return 0, len(s)
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

// isOpeningBracket 은 여는 괄호·따옴표다 — 바로 뒤의 마커는 닫는 자리가 아니라 여는 자리다.
// ASCII `"`·`'` 는 여닫는 모양이 같아 가르지 못하므로 넣지 않는다.
func isOpeningBracket(c rune) bool {
	switch c {
	case '(', '[', '{', '「', '『', '（', '【', '《', '“', '‘':
		return true
	}
	return false
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
//
// before 는 원문에서 마커 바로 앞 글자, inside 는 같은 마커로 그리는 강조 안인가다(mrkdwn 의 닮은 글자
// 판단, mrkdwnAlike). 뒤 글자는 out 의 at 뒤에서 본다 — 되돌리는 마커의 내용이 거기 있다.
func insertMarker(out *[]byte, at int, c rune, n int, before rune, inside bool, v vocab, repairs *Repairs) {
	// mrkdwn 의 글자 마커는 본문 글자와 같은 규칙이다 — 글자마다 앞뒤를 보고 읽힐 자리면 닮은 글자로.
	if v.isMrkdwn() && c != '`' {
		after := noChar
		if r, size := utf8.DecodeRune((*out)[at:]); size > 0 {
			after = r
		}
		s := make([]byte, 0, n*3)
		for k := 0; k < n; k++ {
			nx := after
			if k+1 < n {
				nx = c
			}
			if a := mrkdwnAlike(c, runPrev(k, before, c), nx, inside); a != noChar {
				repairs.EscapedChar++
				s = appendRune(s, a)
			} else {
				s = appendRune(s, c)
			}
		}
		insertAt(out, at, string(s))
		return
	}
	// mrkdwn 은 글자 백틱끼리 U+200B 로 뗀다(vocab.escapeChar) — 되돌린 런도, 그 앞뒤 백틱과도.
	if c == '`' && v.isMrkdwn() {
		prev := before
		if prev == noChar {
			if r, size := utf8.DecodeLastRune((*out)[:at]); size > 0 {
				prev = r
			}
		}
		s := make([]byte, 0, n*4+3)
		for k := 0; k < n; k++ {
			if prev == '`' {
				s = appendRune(s, zeroWidthSpace)
			}
			s = append(s, '`')
			prev = '`'
		}
		if at < len(*out) && (*out)[at] == '`' {
			s = appendRune(s, zeroWidthSpace)
		}
		insertAt(out, at, string(s))
		return
	}
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

// markerBit 는 mrkdwn 마커 글자의 비트다(inline.enclosing).
func markerBit(c rune) uint8 {
	switch c {
	case '*':
		return 1
	case '_':
		return 2
	case '~':
		return 4
	}
	return 0
}

// mrkdwnAlike 는 mrkdwn 에서 글자로 쓴 c 를 닮은 글자로 바꿀 자리면 그 글자다 — ∗(U+2217) ＿(U+FF3F)
// ∼(U+223C). 아니면 noChar.
//
// 여는 자리는 앞이 글자·숫자가 아니고 뒤가 공백이 아닌 곳이다 — `SELECT * FROM` 의 `*` 는 둔다. 닫는 자리는
// 앞이 공백이 아니고 뒤가 글자·숫자가 아닌 곳인데, 같은 마커로 그리는 강조 안(inside)에서만 짝이 있다.
// 런은 글자마다 본다. 슬랙에서 재지는 않았다. 러스트 쪽 mrkdwn_alike 와 같다.
func mrkdwnAlike(c, prev, next rune, inside bool) rune {
	var alike rune
	switch c {
	case '*':
		alike = '\u2217'
	case '_':
		alike = '\uFF3F'
	case '~':
		alike = '\u223C'
	default:
		return noChar
	}
	opens := !glued(prev) && next != noChar && !unicode.IsSpace(next)
	closes := inside && prev != noChar && !unicode.IsSpace(prev) && !glued(next)
	if opens || closes {
		return alike
	}
	return noChar
}
