// `@lezer/markdown` 확장 — 강조·취소선을 mdwire 의 규칙으로 읽는다.
//
// CommonMark 는 닫는 마커 앞이 구두점이고 뒤가 글자면 닫지 않는다. 한국어는 조사가 붙어서
// `**설정(config)**을` 이 그 자리에 정면으로 걸리고, lezer 로 그리는 화면(CodeMirror · 미리보기)에
// 별표가 그대로 보인다. 이 확장은 코어 `inline.rs` 의 읽기 규칙을 lezer 의 인라인 파서로 옮긴 것이다
// (사이트 문서 "조사 앞 강조"). 코어가 바뀌면 여기도 같이 바꾼다 — 코어와 같은 답을 내는 것이 정의다.
//
// lezer 의 기본 구분자 해석(`resolveMarkers`)은 내장 구분자 객체에만 굵게·기울임 크기와 "3의 배수"
// 규칙을 적용해서 쓸 수 없다. 대신 확장용 API(`findOpeningDelimiter` · `takeContent`)로 **닫는 마커가
// 오는 자리에서 바로 짝을 맺는다** — 코어의 규칙 1("같은 종류가 열려 있고 앞이 공백이 아니면 닫는다")과
// 같은 모양이다. 짝을 못 맺은 마커는 lezer 가 글자로 둔다. 코어가 짝 잃은 `**` 를 버리는 자리에서는
// 이 확장은 별표를 그대로 보인다 — 범위가 뒤집히지는 않는다.

/** @typedef {import("@lezer/markdown").InlineContext} InlineContext */
/** @typedef {import("@lezer/markdown").DelimiterType} DelimiterType */

// 종류마다 구분자 타입 하나. `resolve` 를 두지 않아 lezer 가 저절로 짝짓지 않는다 — 짝은 여기서 맺는다.
// 막힌 여는 마커(`값**(합계)**를` 의 첫 `**`)는 따로 둔다 — 보통 닫는 마커는 그걸 보지 못하고, 거울
// 모양의 닫는 마커만 짝이 된다.
const STRONG = { mark: "EmphasisMark" };
const EM = { mark: "EmphasisMark" };
const STRIKE = { mark: "StrikethroughMark" };
const HEMMED_STRONG = { mark: "EmphasisMark" };
const HEMMED_EM = { mark: "EmphasisMark" };
const HEMMED_STRIKE = { mark: "StrikethroughMark" };

const KINDS = {
  strong: { type: STRONG, hemmed: HEMMED_STRONG, node: "StrongEmphasis", mark: "EmphasisMark" },
  em: { type: EM, hemmed: HEMMED_EM, node: "Emphasis", mark: "EmphasisMark" },
  strike: { type: STRIKE, hemmed: HEMMED_STRIKE, node: "Strikethrough", mark: "StrikethroughMark" },
};

// 코어 `is_punct` 와 같다 — ASCII 구두점과 LLM 이 쓰는 CJK 구두점.
const CJK_PUNCT = "，。、！？；：·…—～「」『』（）【】《》“”‘’";
const ASCII_PUNCT = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";
const LETTER = /\p{L}/u;
const ALNUM = /[\p{L}\p{N}]/u;
const SPACE = /\s/u;

/** @param {number} cp */
function isWs(cp) {
  return cp >= 0 && SPACE.test(String.fromCodePoint(cp));
}
/** 코어 `is_word_char` — 알파벳(한글 포함)과 ASCII 숫자. `①` 같은 기호는 아니다. */
function isWord(cp) {
  return cp >= 0 && ((cp >= 48 && cp <= 57) || LETTER.test(String.fromCodePoint(cp)));
}
function isAlnum(cp) {
  return cp >= 0 && ALNUM.test(String.fromCodePoint(cp));
}
function isAsciiAlnum(cp) {
  return (cp >= 48 && cp <= 57) || (cp >= 65 && cp <= 90) || (cp >= 97 && cp <= 122);
}
/** 코어 `is_punct`. */
function isPunct(cp) {
  if (cp < 0) return false;
  const s = String.fromCodePoint(cp);
  return (cp < 128 && ASCII_PUNCT.includes(s)) || CJK_PUNCT.includes(s);
}

/** `pos` 바로 앞의 코드 포인트. 인라인 구간 밖이면 -1. */
function before(cx, pos) {
  if (pos <= cx.offset) return -1;
  const lo = cx.char(pos - 1);
  if (lo >= 0xdc00 && lo <= 0xdfff && pos - 2 >= cx.offset) {
    const hi = cx.char(pos - 2);
    if (hi >= 0xd800 && hi <= 0xdbff) return (hi - 0xd800) * 0x400 + (lo - 0xdc00) + 0x10000;
  }
  return lo;
}
/** `pos` 의 코드 포인트. 끝이면 -1. */
function at(cx, pos) {
  const hi = cx.char(pos);
  if (hi >= 0xd800 && hi <= 0xdbff) {
    const lo = cx.char(pos + 1);
    if (lo >= 0xdc00 && lo <= 0xdfff) return (hi - 0xd800) * 0x400 + (lo - 0xdc00) + 0x10000;
  }
  return hi;
}

/** 코어 `can_open` — 뒤가 공백이 아니고, 뒤가 구두점이면 앞이 글자가 아니어야 한다. */
function canOpen(prev, next) {
  return next >= 0 && !isWs(next) && (!isPunct(next) || !isWord(prev));
}

/** 이 파서(의 노드 집합)에 취소선 노드가 있는가 — GFM 을 안 켰으면 `~~` 는 글자다. */
const strikeKnown = new WeakMap();
function hasStrike(cx) {
  let v = strikeKnown.get(cx.parser);
  if (v === undefined) {
    v = cx.parser.nodeSet.types.some((t) => t.name === "Strikethrough");
    strikeKnown.set(cx.parser, v);
  }
  return v;
}

/** 열린 마커 `type` 을 찾아 `[열린 인덱스, 구분자]` 로. */
function open(cx, type) {
  const i = cx.findOpeningDelimiter(type);
  return i == null ? null : [i, cx.getDelimiterAt(i)];
}

/** `at` 에서 열린 것을 `[from, to)` 의 닫는 마커로 닫는다. */
function close(cx, kind, index, delim, from, to) {
  const content = cx.takeContent(index);
  cx.addElement(
    cx.elt(kind.node, delim.from, to, [cx.elt(kind.mark, delim.from, delim.to), ...content, cx.elt(kind.mark, from, to)]),
  );
  return to;
}

/**
 * 마커 런 하나를 코어 규칙으로 읽는다. `kind` 는 종류, `[from, to)` 는 이 조각, `prev` · `next` 는 런 전체의
 * 앞뒤 글자(쪼갠 조각도 런 전체의 이웃을 본다 — 코어와 같다). 돌려주는 값은 다음 위치.
 */
function marker(cx, kind, from, to, prev, next, intraword) {
  const run = to - from;
  const afterSpace = prev < 0 || isWs(prev);
  const left = canOpen(prev, next) && !intraword;
  // 여는 쪽이 막힌 자리 — 앞이 글자, 뒤가 구두점.
  const hemmedHere = isWord(prev) && next >= 0 && !isWs(next) && isPunct(next);

  const same = open(cx, kind.type);
  const hemmed = open(cx, kind.hemmed);
  // 글자·숫자 뒤의 `_` 는 열지 못한다(`snake_case`). 같은 종류가 열려 있으면 닫는 것은 된다(`_진료_가`).
  if (intraword && !same && !hemmed) return to;
  // 둘 다 열려 있으면 더 가까운 쪽이 코어의 "같은 종류 중 마지막"이다.
  const nearest = same && hemmed ? (same[0] > hemmed[0] ? "same" : "hemmed") : same ? "same" : hemmed ? "hemmed" : null;

  if (nearest === "hemmed") {
    const [i, d] = hemmed;
    // 막힌 여는 마커의 거울 짝 — 같은 줄, 같은 길이, 앞이 구두점, 뒤가 글자(조사). 양 끝이 다 ASCII
    // 영숫자면 수식(`x**(y)**z`)이라 짝짓지 않는다.
    const openBefore = before(cx, d.from);
    if (
      d.to - d.from === run &&
      !cx.slice(d.to, from).includes("\n") &&
      prev >= 0 &&
      !isWord(prev) &&
      !isWs(prev) &&
      isWord(next) &&
      !(isAsciiAlnum(openBefore) && isAsciiAlnum(next))
    ) {
      return close(cx, kind, i, d, from, to);
    }
    // 막힌 여는 마커가 또 왔다 — 앞의 것은 짝이 없었다. lezer 가 글자로 둔다. 이쪽을 새로 연다.
    if (hemmedHere) return cx.addDelimiter(kind.hemmed, from, to, true, false);
    // 추측으로 연 것은 닫지 않는다. 여는 자리면 새로 열고, 아니면 글자다.
    if (left && afterSpace) return cx.addDelimiter(kind.type, from, to, true, false);
    return to;
  }
  if (nearest === "same") {
    const [i, d] = same;
    // 같은 종류가 열려 있고 앞이 공백이 아니면 여기가 닫는 자리다. 규칙 1.
    if (!afterSpace) return close(cx, kind, i, d, from, to);
    // 앞이 공백인데 열 수 있다 — 여는 마커가 또 왔다. 먼저 열린 쪽이 진다(글자로 남는다). 규칙 2.
    if (left) return cx.addDelimiter(kind.type, from, to, true, false);
    return close(cx, kind, i, d, from, to);
  }
  if (left) return cx.addDelimiter(kind.type, from, to, true, false);
  // 열 수도 닫을 수도 없다. 막힌 여는 마커면 거울 짝을 기다리고, 아니면 글자다. 규칙 3.
  if (hemmedHere) return cx.addDelimiter(kind.hemmed, from, to, true, false);
  return to;
}

/** `*` · `_` 런 — 코어처럼 길이로 종류를 정하고, `***` 는 `**` 와 `*` 로 쪼갠다. */
function parseEmphasis(cx, next, start) {
  if (next !== 42 && next !== 95) return -1;
  let pos = start + 1;
  while (cx.char(pos) === next) pos++;
  const run = pos - start;
  // 넷 이상은 글자다 — 마스킹 번호 `4***-****-****-003*` 의 `****`.
  if (run > 3) return pos;
  const prev = before(cx, start);
  const after = at(cx, pos);
  // 코어의 `intraword` — 글자·숫자 뒤의 `_`. 판정은 `marker` 가 한다.
  const intraword = next === 95 && isAlnum(prev);

  let i = start;
  while (i < pos) {
    let take = pos - i;
    if (take === 3) {
      // `***` 는 `**` 와 `*` 다. 기울임이 열려 있으면 그것부터 닫고, 굵게만 열려 있으면 통째로 닫는
      // 마커이고, 아니면 굵게를 먼저 연다. 코어와 같다.
      if (open(cx, EM) || open(cx, HEMMED_EM)) take = 1;
      else if (open(cx, STRONG) || open(cx, HEMMED_STRONG)) take = 3;
      else take = 2;
    }
    const kind = take === 1 ? KINDS.em : KINDS.strong;
    i = marker(cx, kind, i, i + take, prev, after, intraword);
  }
  return pos;
}

/** `~~` — 취소선. 홑 `~` 와 셋 이상은 글자다. GFM 이 없으면 전부 글자다. */
function parseStrike(cx, next, start) {
  if (next !== 126 || cx.char(start + 1) !== 126 || cx.char(start + 2) === 126 || !hasStrike(cx)) return -1;
  const prev = before(cx, start);
  const after = at(cx, start + 2);
  return marker(cx, KINDS.strike, start, start + 2, prev, after, false);
}

/**
 * A `@lezer/markdown` extension that reads emphasis the way mdwire does, so `**설정(config)**을` — bold
 * followed directly by a Korean particle — closes instead of leaving the asterisks as text. It replaces the
 * built-in `Emphasis` parser and GFM's `Strikethrough` parser by name, so put it after them:
 * `parser.configure([GFM, koreanEmphasis])`. For CodeMirror:
 * `markdown({ base: markdownLanguage, extensions: [koreanEmphasis] })`.
 *
 * Markers that pair under mdwire's rules become `StrongEmphasis` / `Emphasis` / `Strikethrough` nodes; markers
 * that do not pair stay as text (mdwire drops some of those, this extension never deletes characters).
 * @type {import("@lezer/markdown").MarkdownConfig}
 */
export const koreanEmphasis = {
  parseInline: [
    { name: "Emphasis", parse: parseEmphasis },
    { name: "Strikethrough", parse: parseStrike, after: "Emphasis" },
  ],
};
