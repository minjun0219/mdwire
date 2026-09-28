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
	var channel string
	stream := false
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
		case "-h", "--help":
			fmt.Fprint(out, "사용법: mdwire --channel telegram-html|slack-markdown|plain [--stream] < input.md\n")
			return nil
		default:
			return fmt.Errorf("모르는 인자: %s", args[i])
		}
	}
	ch, ok := mdwire.ParseChannel(channel)
	if !ok {
		return fmt.Errorf("모르는 채널: %q (telegram-html · slack-markdown · plain)", channel)
	}
	w := bufio.NewWriter(out)
	defer w.Flush()
	if stream {
		s := mdwire.NewStreamer(ch)
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
			w.Write(piece)
			w.Flush()
		}
		piece = piece[:0]
		s.FinishTo(&piece)
		w.Write(piece)
		return nil
	}
	input, err := io.ReadAll(in)
	if err != nil {
		return err
	}
	for i, part := range mdwire.Render(string(input), ch) {
		if i > 0 {
			w.WriteByte(0)
		}
		w.WriteString(part)
	}
	return nil
}
