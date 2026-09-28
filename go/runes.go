package mdwire

import "unicode/utf8"

// 러스트 코어는 `Vec<char>` 위에서 인덱스로 파싱한다. Go 에서는 `[]rune` 이 그 자리다 —
// `string` 은 바이트 인덱스라 `s[i]` 가 글자가 아니다. 출력은 `[]byte` 에 붙인다: 여는
// 마크업을 나중에 끼워 넣어야 해서(`insertAt`) `strings.Builder` 로는 안 된다.

func encodeRune(b []byte, c rune) int { return utf8.EncodeRune(b, c) }

// insertAt 은 out[at:] 앞에 s 를 끼워 넣는다. at 은 바이트 위치다.
func insertAt(out *[]byte, at int, s string) {
	b := *out
	b = append(b, s...) // 자리를 늘린다
	copy(b[at+len(s):], b[at:len(b)-len(s)])
	copy(b[at:], s)
	*out = b
}

// removeAt 은 out[at] 한 바이트를 지운다. ASCII 공백처럼 한 바이트인 것만 지운다.
func removeAt(out *[]byte, at int) {
	b := *out
	copy(b[at:], b[at+1:])
	*out = b[:len(b)-1]
}

func runLen(line []rune, at int, c rune) int {
	n := 0
	for at+n < len(line) && line[at+n] == c {
		n++
	}
	return n
}

func startsWith(chars []rune, s string) bool {
	i := 0
	for _, c := range s {
		if i >= len(chars) || chars[i] != c {
			return false
		}
		i++
	}
	return true
}

func findSeq(chars []rune, s string) int {
	for k := range chars {
		if startsWith(chars[k:], s) {
			return k
		}
	}
	return -1
}

func indexRune(chars []rune, c rune) int {
	for i, x := range chars {
		if x == c {
			return i
		}
	}
	return -1
}

func eqIgnoreCase(chars []rune, s string) bool {
	if len(chars) != len(s) {
		return false
	}
	for i, c := range s {
		a, b := chars[i], c
		if a >= 'A' && a <= 'Z' {
			a += 'a' - 'A'
		}
		if b >= 'A' && b <= 'Z' {
			b += 'a' - 'A'
		}
		if a != b {
			return false
		}
	}
	return true
}

// longestRun 은 같은 글자가 이어진 가장 긴 길이다.
func longestRun(s []byte, c byte) int {
	best, cur := 0, 0
	for _, x := range s {
		if x == c {
			cur++
			if cur > best {
				best = cur
			}
		} else {
			cur = 0
		}
	}
	return best
}
