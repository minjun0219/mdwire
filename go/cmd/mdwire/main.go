// mdwire CLI 의 Go 판 — stdin 을 읽어 채널 하나로 내보낸다. 러스트 CLI 와 인자·출력이 같다:
// 조각은 NUL 로 구분하고, --stream 이면 받는 대로 내보낸다.
package main

import (
	"bufio"
	"fmt"
	"io"
	"os"
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
	stream, report := false, false
	var opts mdwire.Options
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
			fmt.Fprint(out, "사용법: mdwire --channel telegram-html|slack-markdown|plain [--from markdown|slack-mrkdwn] [--stream] [--report] < input.md\n")
			return nil
		default:
			return fmt.Errorf("모르는 인자: %s", args[i])
		}
	}
	ch, ok := mdwire.ParseChannel(channel)
	if !ok {
		return fmt.Errorf("모르는 채널: %q (telegram-html · slack-markdown · plain)", channel)
	}
	// 쓰기·비우기 실패를 삼키지 않는다 — 닫힌 파이프나 가득 찬 장치에 잘린 출력을 내고 0 으로
	// 끝나면 호출자가 배달 실패를 알 수 없다.
	w := bufio.NewWriter(out)
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
	_, err := fmt.Fprintf(w, "{\"closedEmphasis\":%d,\"closedFence\":%d,\"revertedCodeSpan\":%d,\"droppedMarker\":%d}\n",
		r.ClosedEmphasis, r.ClosedFence, r.RevertedCodeSpan, r.DroppedMarker)
	return err
}
