//! 인자 표기. `--이름=값` 도 `--이름 값` 과 같게 받는다.

use std::io::Write;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mdwire"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("mdwire 를 띄운다");
    child.stdin.take().expect("stdin").write_all(input.as_bytes()).expect("쓴다");
    let out = child.wait_with_output().expect("끝난다");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).expect("UTF-8")
}

#[test]
fn equals_form_is_the_same_as_two_args() {
    let spaced = run(&["--channel", "slack-markdown", "--limit", "300"], "_기울임_ __굵게__");
    let equals = run(&["--channel=slack-markdown", "--limit=300"], "_기울임_ __굵게__");
    assert_eq!(spaced, "*기울임* **굵게**");
    assert_eq!(equals, spaced);
}
