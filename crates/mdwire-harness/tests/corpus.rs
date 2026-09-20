//! 코퍼스가 정본이다. 이 테스트가 그 말을 강제한다.

use mdwire::{Channel, CjkPolicy, Streamer};
use mdwire_harness::adapter::Mdwire;
use mdwire_harness::corpus;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/cases")
}

/// 케이스 × 채널 전부. 기대 출력 대조와 불변식 채점을 함께 본다.
#[test]
fn corpus_passes() {
    let outcomes = corpus::run_corpus(&corpus_dir(), &Mdwire::default(), &Channel::v0_1())
        .expect("코퍼스를 읽어야 한다");
    assert!(!outcomes.is_empty(), "케이스가 없다");
    if !corpus::report(&outcomes, false) {
        panic!("코퍼스 실패 — 위 보고를 본다");
    }
}

/// **스트리밍과 완성본이 같은 답을 내야 한다.**
///
/// 조각 크기를 바꿔 가며 같은 입력을 흘린다. 조각 경계가 마크업 한가운데를 지나가도
/// 결과가 달라지면 안 된다 — 달라지는 것이 `DESIGN.md` 의 두 번째 고장이다.
/// 1바이트씩 흘리는 것은 경계가 **모든 자리**에 걸린다는 뜻이라 가장 가혹하다.
#[test]
fn streaming_agrees_with_batch() {
    let cases = corpus::load_cases(&corpus_dir()).expect("코퍼스");
    for case in &cases {
        for channel in Channel::v0_1() {
            let parts = mdwire::render(&case.input, channel, CjkPolicy::Auto);
            // **한도를 넘겨 나뉜 케이스는 건너뛴다.** 조각은 저마다 메시지 하나라 앞머리
            // 줄바꿈을 털어 내고 시작한다 — 도로 이어 붙이면 스트리밍과 달라지는 것이
            // 정상이다. 스트리밍은 한도를 모른다(`SPEC.md` 5절).
            if parts.len() > 1 {
                continue;
            }
            let batch = parts.join("");
            for size in [1usize, 2, 3, 7, 64] {
                let mut s = Streamer::new(channel, CjkPolicy::Auto);
                let mut got = String::new();
                for chunk in chunks(&case.input, size) {
                    s.push_into(chunk, &mut got);
                }
                s.finish_into(&mut got);
                assert_eq!(
                    got,
                    batch,
                    "{} · {} · 조각 {}자에서 스트리밍이 완성본과 갈렸다",
                    case.name,
                    channel.name(),
                    size
                );
            }
        }
    }
}

/// `push` 와 `push_into` 는 같은 코드를 부른다. 서명만 다르다(`SPEC.md` 5절).
#[test]
fn borrowed_and_owned_signatures_agree() {
    let cases = corpus::load_cases(&corpus_dir()).expect("코퍼스");
    for case in &cases {
        let mut a = Streamer::new(Channel::TelegramHtml, CjkPolicy::Auto);
        let mut b = Streamer::new(Channel::TelegramHtml, CjkPolicy::Auto);
        let (mut got_a, mut got_b) = (String::new(), String::new());
        for chunk in chunks(&case.input, 11) {
            got_a.push_str(a.push(chunk));
            b.push_into(chunk, &mut got_b);
        }
        got_a.push_str(a.finish());
        b.finish_into(&mut got_b);
        assert_eq!(got_a, got_b, "{}", case.name);
    }
}

/// 문자 경계를 지키며 `n` 글자씩 자른다.
fn chunks(s: &str, n: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut count = 0;
    for (i, _) in s.char_indices() {
        if count == n && i > start {
            out.push(&s[start..i]);
            start = i;
            count = 0;
        }
        count += 1;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}
