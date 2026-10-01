package main

import (
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// failWriter 는 가득 찬 장치나 닫힌 파이프처럼 쓰기를 거절한다.
type failWriter struct{}

func (failWriter) Write([]byte) (int, error) { return 0, errors.New("no space left on device") }

// 쓰기 실패를 삼키면 잘린 출력을 내고도 0 으로 끝나 호출자가 배달 실패를 모른다.
func TestWriteFailuresAreReported(t *testing.T) {
	for _, args := range [][]string{{"--channel", "plain"}, {"--channel", "plain", "--stream"}} {
		if err := run(args, strings.NewReader("본문 **굵게**\n"), failWriter{}); err == nil {
			t.Errorf("%v: 쓰기 실패가 에러로 안 돌아왔다", args)
		}
	}
}

// batchInput 은 정상 줄과 못 읽는 줄을 섞는다. 러스트 CLI 와 글자까지 같아야 한다.
var batchInput = strings.Join([]string{
	`{"id":"a\"1<&>","text":"*첫 줄\n둘째*입니다"}`,
	``,
	`{"id": 7, "text": "**안 닫힘"}` + "\r",
	`{"text":"` + "```" + `\n안 닫힌 펜스"}`,
	`not json`,
	`null`,
	`{"id":[1],"text":"x"}`,
	`{"text":3}`,
	`{"id":"x"}`,
	" ",
	"{\"text\":\"\xff\"}",
	`{"id":-1.5e3,"text":"앞 ` + "`" + ` 뒤"}`,
	"\v",
	`{"text":"**\ud800"}`,
	`{"id":"\udc00","text":"x"}`,
	`{"\ud800":1,"text":"x"}`,
	`{"text":"\ud83d\ude00 \\ud800 ok"}`,
}, "\n")

func TestBatchKeepsGoingAndCountsFailures(t *testing.T) {
	var out, errOut strings.Builder
	err := runWith([]string{"--channel", "slack-markdown", "--from", "slack-mrkdwn", "--batch", "jsonl"}, strings.NewReader(batchInput), &out, &errOut)
	if err == nil || err.Error() != "11줄을 읽지 못했다" {
		t.Fatalf("에러가 %v — 못 읽은 11줄을 세야 한다\n%s", err, out.String())
	}
	lines := strings.Split(strings.TrimSuffix(out.String(), "\n"), "\n")
	if got := lines[0]; got != `{"line":1,"id":"a\"1<&>","closedEmphasis":0,"closedFence":0,"revertedCodeSpan":0,"droppedMarker":0}` {
		t.Errorf("1줄: %s", got)
	}
	if got := lines[1]; !strings.HasPrefix(got, `{"line":3,"id":7,"closedEmphasis":1`) {
		t.Errorf("3줄: %s", got)
	}
}

// 러스트 CLI 와 대조한다. MDWIRE_RUST 로 러스트 CLI 경로를 주면 돈다 — 상대 경로는 go/
// 기준이다(`go test ./...` 를 go/ 에서 돌리는 CI 와 같게). 이 시험은 go/cmd/mdwire 에서 돈다.
func TestBatchMatchesRust(t *testing.T) {
	bin := os.Getenv("MDWIRE_RUST")
	if bin == "" {
		t.Skip("MDWIRE_RUST 가 없다")
	}
	if !filepath.IsAbs(bin) {
		bin = filepath.Join("..", "..", bin)
	}
	args := []string{"--channel", "slack-markdown", "--from", "slack-mrkdwn", "--batch", "jsonl"}
	cmd := exec.Command(bin, args...)
	cmd.Stdin = strings.NewReader(batchInput)
	// 못 읽은 줄이 있어 실패로 끝나는 것이 정상이다. 실행 자체가 안 된 것만 막는다.
	want, err := cmd.Output()
	var exit *exec.ExitError
	if err != nil && !errors.As(err, &exit) {
		t.Fatalf("러스트 CLI 실행 실패: %v", err)
	}
	var got strings.Builder
	_ = runWith(args, strings.NewReader(batchInput), &got, io.Discard)
	if got.String() != string(want) {
		t.Errorf("러스트와 다르다\n  go   %s\n  rust %s", got.String(), want)
	}
}

// --이름=값 도 --이름 값 과 같게 받는다 — 러스트 CLI 의 equals_form_is_the_same_as_two_args.
func TestEqualsFormIsTheSameAsTwoArgs(t *testing.T) {
	render := func(args ...string) string {
		var out strings.Builder
		if err := run(args, strings.NewReader("*굵게* ~취소~"), &out); err != nil {
			t.Fatal(err)
		}
		return out.String()
	}
	spaced := render("--channel", "slack-markdown", "--from", "slack-mrkdwn")
	equals := render("--channel=slack-markdown", "--from=slack-mrkdwn")
	if spaced != "**굵게** ~~취소~~" || equals != spaced {
		t.Fatalf("spaced %q equals %q", spaced, equals)
	}
}
