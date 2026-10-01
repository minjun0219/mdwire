// mdwire CLI 의 Go 판 — stdin 을 읽어 채널 하나로 내보낸다. 러스트 CLI 와 인자·출력이 같다:
// 조각은 NUL 로 구분하고, --stream 이면 받는 대로 내보낸다. --batch jsonl 이면 문서 여럿의
// 고친 것을 한 줄씩 낸다.
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"
	"unicode/utf8"

	mdwire "github.com/minjun0219/mdwire/go"
)

func main() {
	if err := run(os.Args[1:], os.Stdin, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, "mdwire:", err)
		os.Exit(2)
	}
}

func run(args []string, in io.Reader, out io.Writer) error {
	return runWith(args, in, out, os.Stderr)
}

// runWith 는 run 에 stderr 를 따로 받는다 — --report 가 거기로 나간다.
func runWith(args []string, in io.Reader, out, errOut io.Writer) error {
	var channel string
	stream, report, batch := false, false, false
	var opts mdwire.Options
	// --이름=값 도 받는다 — 두 인자로 편다(러스트 CLI 와 같다).
	var flat []string
	for _, a := range args {
		if k, v, ok := strings.Cut(a, "="); ok && strings.HasPrefix(k, "--") {
			flat = append(flat, k, v)
		} else {
			flat = append(flat, a)
		}
	}
	args = flat
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--channel":
			if i+1 >= len(args) {
				return fmt.Errorf("--channel 에 값이 없다")
			}
			i++
			channel = args[i]
		case "--stream":
			stream = true
		case "--report":
			report = true
		case "--batch":
			if i+1 >= len(args) {
				return fmt.Errorf("--batch 에 값이 없다")
			}
			i++
			if args[i] != "jsonl" {
				return fmt.Errorf("모르는 --batch 형식: %s (jsonl 만 받는다)", args[i])
			}
			batch = true
		case "--limit":
			if i+1 >= len(args) {
				return fmt.Errorf("--limit 에 값이 없다")
			}
			i++
			n, err := strconv.Atoi(args[i])
			if err != nil || n < 1 {
				return fmt.Errorf("--limit 은 1 이상의 정수다: %s", args[i])
			}
			opts.Limit = n
		case "--html-line-breaks":
			if i+1 >= len(args) {
				return fmt.Errorf("--html-line-breaks 에 값이 없다")
			}
			i++
			switch args[i] {
			case "br":
				opts.HTML.LineBreaks = mdwire.LineBreaksBR
			case "space":
				opts.HTML.LineBreaks = mdwire.LineBreaksSpace
			default:
				return fmt.Errorf("--html-line-breaks 는 br · space 다: %s", args[i])
			}
		case "--html-images":
			if i+1 >= len(args) {
				return fmt.Errorf("--html-images 에 값이 없다")
			}
			i++
			switch args[i] {
			case "link":
				opts.HTML.Images = mdwire.ImagesLink
			case "load":
				opts.HTML.Images = mdwire.ImagesLoad
			default:
				return fmt.Errorf("--html-images 는 link · load 다: %s", args[i])
			}
		case "--html-schemes":
			if i+1 >= len(args) {
				return fmt.Errorf("--html-schemes 에 값이 없다")
			}
			i++
			// 빈 목록도 nil 이 아니게 둔다 — nil 은 기본 목록이라는 뜻이다.
			schemes := []string{}
			for _, s := range strings.Split(args[i], ",") {
				if s = strings.TrimSpace(s); s != "" {
					schemes = append(schemes, s)
				}
			}
			opts.HTML.Schemes = schemes
		case "--from":
			if i+1 >= len(args) {
				return fmt.Errorf("--from 에 값이 없다")
			}
			i++
			d, ok := mdwire.ParseDialect(args[i])
			if !ok {
				return fmt.Errorf("모르는 방언: %q (markdown · slack-mrkdwn)", args[i])
			}
			opts.From = d
		case "-h", "--help":
			fmt.Fprint(out, "사용법: mdwire --channel telegram-html|slack-markdown|github-markdown|plain|html [--from markdown|slack-mrkdwn] [--limit N] [--html-line-breaks br|space] [--html-images link|load] [--html-schemes http,https,mailto] [--stream] [--report] [--batch jsonl] < input.md\n")
			return nil
		default:
			return fmt.Errorf("모르는 인자: %s", args[i])
		}
	}
	ch, ok := mdwire.ParseChannel(channel)
	if !ok {
		return fmt.Errorf("모르는 채널: %q (telegram-html · slack-markdown · github-markdown · plain · html)", channel)
	}
	// 쓰기·비우기 실패를 삼키지 않는다 — 닫힌 파이프나 가득 찬 장치에 잘린 출력을 내고 0 으로
	// 끝나면 호출자가 배달 실패를 알 수 없다.
	w := bufio.NewWriter(out)
	if batch {
		if stream {
			return fmt.Errorf("--batch 와 --stream 은 같이 쓸 수 없다")
		}
		failed, err := batchJSONL(ch, opts, in, w)
		if err != nil {
			return err
		}
		if failed > 0 {
			return fmt.Errorf("%d줄을 읽지 못했다", failed)
		}
		return nil
	}
	if stream {
		s := mdwire.NewStreamerWith(ch, opts)
		r := bufio.NewReader(in)
		var piece []byte
		var chunk []byte
		for {
			// 64바이트씩 읽되 글자 경계에서 끊는다 — 한글 한 글자가 세 바이트라, 바이트로
			// 자르면 조각 끝에 반쪽 글자가 남아 U+FFFD 로 깨진다.
			chunk = chunk[:0]
			for len(chunk) < 64 {
				c, _, err := r.ReadRune()
				if err != nil {
					break
				}
				chunk = utf8.AppendRune(chunk, c)
			}
			if len(chunk) == 0 {
				break
			}
			piece = piece[:0]
			s.PushTo(string(chunk), &piece)
			if _, err := w.Write(piece); err != nil {
				return err
			}
			if err := w.Flush(); err != nil {
				return err
			}
		}
		piece = piece[:0]
		s.FinishTo(&piece)
		if _, err := w.Write(piece); err != nil {
			return err
		}
		if err := w.Flush(); err != nil {
			return err
		}
		if report {
			return writeReport(errOut, s.Repairs())
		}
		return nil
	}
	input, err := io.ReadAll(in)
	if err != nil {
		return err
	}
	rendered := mdwire.RenderWith(string(input), ch, opts)
	for i, part := range rendered.Parts {
		if i > 0 {
			if err := w.WriteByte(0); err != nil {
				return err
			}
		}
		if _, err := w.WriteString(part); err != nil {
			return err
		}
	}
	if err := w.Flush(); err != nil {
		return err
	}
	if report {
		return writeReport(errOut, rendered.Repairs)
	}
	return nil
}

// writeReport 는 고친 것을 JSON 한 줄로 낸다 — 러스트 CLI 와 같은 키다.
func writeReport(w io.Writer, r mdwire.Repairs) error {
	_, err := fmt.Fprintf(w, "{%s}\n", repairsFields(r))
	return err
}

func repairsFields(r mdwire.Repairs) string {
	return fmt.Sprintf("\"closedEmphasis\":%d,\"closedFence\":%d,\"revertedCodeSpan\":%d,\"droppedMarker\":%d",
		r.ClosedEmphasis, r.ClosedFence, r.RevertedCodeSpan, r.DroppedMarker)
}

// batchJSONL 은 줄마다 문서 하나를 읽어 고친 것을 한 줄씩 낸다. 못 읽은 줄 수를 돌려준다.
// 러스트 CLI 와 출력이 글자까지 같다 — 못 읽은 줄은 그 줄만 error 로 적고 계속 가고, 빈 줄은
// 건너뛰되 번호(line, 1부터)는 센다.
func batchJSONL(ch mdwire.Channel, opts mdwire.Options, in io.Reader, w *bufio.Writer) (int, error) {
	r := bufio.NewReader(in)
	failed := 0
	for n := 1; ; n++ {
		line, err := r.ReadBytes('\n')
		if len(line) == 0 && err == io.EOF {
			break
		}
		if err != nil && err != io.EOF {
			return failed, err
		}
		line = bytes.TrimSuffix(bytes.TrimSuffix(line, []byte("\n")), []byte("\r"))
		// 러스트의 is_ascii_whitespace 와 같은 글자만 빈칸으로 본다 — TrimSpace 는 NBSP 도 턴다.
		if len(bytes.Trim(line, " \t\n\f\r")) > 0 {
			id, text, msg := readDoc(line)
			if msg != "" {
				failed++
				fmt.Fprintf(w, "{\"line\":%d,\"error\":\"%s\"}\n", n, msg)
			} else {
				if id != "" {
					id = "\"id\":" + id + ","
				}
				fmt.Fprintf(w, "{\"line\":%d,%s%s}\n", n, id, repairsFields(mdwire.RenderWith(text, ch, opts).Repairs))
			}
		}
		if err == io.EOF {
			break
		}
	}
	return failed, w.Flush()
}

// readDoc 은 한 줄을 (id, text) 로 읽는다. id 는 받은 글자 그대로다 — 다시 직렬화하면
// 구현마다 이스케이프가 달라진다. 에러 문구는 러스트 CLI 와 같다.
func readDoc(line []byte) (id, text, msg string) {
	if hasLoneSurrogate(line) {
		return "", "", "짝 없는 UTF-16 서로게이트가 있다"
	}
	// encoding/json 은 잘못된 UTF-8 을 U+FFFD 로 바꿔 받는다. 러스트는 거절한다 — 같게 거절한다.
	var fields map[string]json.RawMessage
	if !utf8.Valid(line) || json.Unmarshal(line, &fields) != nil || fields == nil {
		return "", "", "JSON 객체가 아니다"
	}
	raw, ok := fields["text"]
	if !ok {
		return "", "", "text 가 없다"
	}
	if len(raw) == 0 || raw[0] != '"' || json.Unmarshal(raw, &text) != nil {
		return "", "", "text 는 문자열이어야 한다"
	}
	if raw, ok := fields["id"]; ok {
		if c := raw[0]; c != '"' && c != '-' && (c < '0' || c > '9') {
			return "", "", "id 는 문자열이나 숫자여야 한다"
		}
		id = string(raw)
	}
	return id, text, ""
}

// hasLoneSurrogate 는 줄 어디엔가 짝 없는 서로게이트 이스케이프(\ud800 홀로)가 있는지 본다.
// encoding/json 은 이걸 U+FFFD 로 바꿔 받고 serde_json 은 자리에 따라 거부한다 — 러스트 CLI 와
// 같은 규칙 하나(줄 전체 거부)로 앞에서 거른다. 역슬래시는 JSON 문자열 안에만 올 수 있다.
func hasLoneSurrogate(line []byte) bool {
	hex := func(at int) (uint64, bool) {
		if at+4 > len(line) {
			return 0, false
		}
		v, err := strconv.ParseUint(string(line[at:at+4]), 16, 32)
		return v, err == nil
	}
	for i := 0; i < len(line); {
		if line[i] != '\\' {
			i++
			continue
		}
		if i+1 >= len(line) || line[i+1] != 'u' {
			i += 2
			continue
		}
		v, ok := hex(i + 2)
		switch {
		case ok && v >= 0xD800 && v <= 0xDBFF:
			lo, ok := uint64(0), false
			if i+7 < len(line) && line[i+6] == '\\' && line[i+7] == 'u' {
				lo, ok = hex(i + 8)
			}
			if !ok || lo < 0xDC00 || lo > 0xDFFF {
				return true
			}
			i += 12
		case ok && v >= 0xDC00 && v <= 0xDFFF:
			return true
		default:
			i += 6
		}
	}
	return false
}
