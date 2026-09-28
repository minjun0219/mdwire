package main

import (
	"errors"
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
