//! 하네스 자체의 검증.
//!
//! **채점기가 고장을 못 잡으면 이후 작업은 전부 검증 없이 가는 것이다.** 그래서
//! 일부러 틀린 구현(입력을 그대로 돌려주는 것)을 붙여, 실행기가 그걸 실패로 잡는지 본다.
//! 통과를 확인하는 테스트는 코어가 선 뒤 `corpus.rs` 가 맡는다.

use mdwire_core::Channel;
use mdwire_harness::adapter::FnRenderer;
use mdwire_harness::corpus;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/cases")
}

/// 입력을 그대로 내보내는 구현. 마크다운을 하나도 변환하지 않는다.
fn passthrough() -> FnRenderer<impl Fn(&str, Channel) -> Result<String, String>> {
    FnRenderer { label: "passthrough".into(), f: |input: &str, _| Ok(input.to_string()) }
}

#[test]
fn corpus_cases_load() {
    let cases = corpus::load_cases(&corpus_dir()).expect("코퍼스를 읽어야 한다");
    assert!(!cases.is_empty(), "케이스가 하나도 없다");
    for c in &cases {
        assert!(!c.input.trim().is_empty(), "{} 의 input.md 가 비었다", c.name);
    }
}

#[test]
fn passthrough_fails_the_corpus() {
    let outcomes =
        corpus::run_corpus(&corpus_dir(), &passthrough(), &[Channel::TelegramHtml]).expect("실행");
    assert!(!outcomes.is_empty());
    let s = corpus::summarize(&outcomes);
    assert!(!s.ok(), "변환하지 않는 구현이 코퍼스를 통과하면 채점이 무의미하다");
    assert!(s.mismatched > 0, "기대 출력 대조가 안 걸렸다");
    assert!(s.findings > 0, "불변식 채점이 안 걸렸다 — 남은 `**` 를 잡아야 한다");
}

#[test]
fn scan_mode_needs_no_expected_output() {
    // 코퍼스 밖 입력. 실제 비교에는 이 저장소에 없는 문서 더미를 쓴다.
    let dir = std::env::temp_dir().join(format!("mdwire-scan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("임시 디렉토리");
    std::fs::write(dir.join("a.md"), "앞말 **굵게\n이어지는 줄**이 있다\n").expect("쓰기");

    let outcomes = corpus::scan_dir(&dir, &passthrough(), &[Channel::TelegramHtml]).expect("훑기");
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].compared.is_none(), "훑기 모드는 기대 출력을 보지 않는다");
    assert!(!outcomes[0].findings.is_empty(), "불변식은 그래도 재야 한다");
}
