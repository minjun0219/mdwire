//! 코퍼스가 정본이다. 이 테스트가 그 말을 강제한다.

use mdwire::{Channel, Options, Streamer};
use mdwire_harness::adapter::Mdwire;
use mdwire_harness::check::{check, Rule};
use mdwire_harness::corpus;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/cases")
}

/// 케이스 × 채널 전부. 기대 출력 대조와 불변식 채점을 함께 본다.
#[test]
fn corpus_passes() {
    let outcomes = corpus::run_corpus(&corpus_dir(), &Mdwire, &Channel::all())
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
        for channel in Channel::all() {
            let options = Options { from: case.from, ..Default::default() };
            let parts = mdwire::render_with(&case.input, channel, options.clone()).parts;
            // **한도를 넘겨 나뉜 케이스는 건너뛴다.** 조각은 저마다 메시지 하나라 앞머리
            // 줄바꿈을 털어 내고 시작한다 — 도로 이어 붙이면 스트리밍과 달라지는 것이
            // 정상이다. 스트리밍은 한도를 모른다(`SPEC.md` 5절).
            if parts.len() > 1 {
                continue;
            }
            let batch = parts.join("");
            for size in [1usize, 2, 3, 7, 64] {
                let mut s = Streamer::with_options(channel, options.clone());
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

/// **미리보기 스냅숏은 언제 보내도 채널이 받는 모양이다.** 누적본 + `preview` 를 조각마다
/// 채점한다 — 태그가 짝이 맞고, 허용 태그만 있고, escape 안 된 `<`·`&` 가 없어야 한다. 모양은
/// 뒤에서 바뀌어도 되지만(추측), 텔레그램은 문법이 틀리면 편집을 400 으로 거절해 갱신이 멈춘다.
///
/// 끝에서는 완성본이 일괄 렌더와 같고, 마지막 미리보기와 같을 때만 `revised` 가 거짓이다.
#[test]
fn preview_snapshots_are_sendable() {
    let cases = corpus::load_cases(&corpus_dir()).expect("코퍼스");
    let mut eager = 0;
    let mut compared = 0;
    for case in &cases {
        for channel in Channel::all() {
            let options = Options { from: case.from, ..Default::default() };
            let parts = mdwire::render_with(&case.input, channel, options.clone()).parts;
            if parts.len() > 1 {
                continue;
            }
            // 스냅숏마다 채점이 문서 길이만큼 들어서, 긴 케이스는 조각을 키워 스냅숏 수를 묶는다.
            let base = (case.input.chars().count() / 150).max(1);
            for size in [base, base * 3 + 2] {
                let mut s = Streamer::with_options(channel, options.clone());
                let mut acc = String::new();
                let mut fed = 0;
                let mut last = String::new();
                for chunk in chunks(&case.input, size) {
                    s.push_into(chunk, &mut acc);
                    fed += chunk.len();
                    last.clear();
                    last.push_str(&acc);
                    s.preview_into(&mut last);
                    if last.len() > acc.len() {
                        eager += 1;
                    }
                    // **미리보기는 지금까지 받은 입력의 일괄 렌더다.** 다른 점은 안 닫힌 코드 스팬을
                    // 글자로 되돌리지 않는 것뿐 — 그런 스냅숏과 한도로 나뉜 것만 뺀다. 태그 규칙은
                    // 통과해도 모양이 틀린 미리보기(닫는 백틱이 비치고, 빈 `<pre>` 가 끼는 것)를 잡는다.
                    let prefix = mdwire::render_with(&case.input[..fed], channel, options.clone());
                    if prefix.parts.len() == 1 && prefix.repairs.reverted_code_span == 0 {
                        compared += 1;
                        assert_eq!(
                            last,
                            prefix.parts[0],
                            "{} · {} · 조각 {size}자 · {fed}바이트에서 미리보기가 일괄 렌더와 다르다",
                            case.name,
                            channel.name()
                        );
                    }
                    let bad: Vec<_> = check(&case.input[..fed], &last, channel)
                        .into_iter()
                        .filter(|f| matches!(f.rule, Rule::UnclosedTag | Rule::DisallowedTag | Rule::RawHtmlChar))
                        .collect();
                    assert!(
                        bad.is_empty(),
                        "{} · {} · 조각 {size}자 · {fed}바이트에서 못 보낼 미리보기: {bad:?}\n{last}",
                        case.name,
                        channel.name()
                    );
                }
                s.finish_into(&mut acc);
                assert_eq!(acc, parts.concat(), "{} · {} · 조각 {size}자", case.name, channel.name());
                assert_eq!(s.revised(), acc != last, "{} · {} · revised", case.name, channel.name());
            }
        }
    }
    assert!(eager > 0, "미리보기가 붙든 것을 한 번도 안 그렸다면 이 시험은 아무것도 안 본다");
    assert!(compared > 1000, "일괄 렌더와 대조한 스냅숏이 너무 적다: {compared}");
}

/// `push` 와 `push_into` 는 같은 코드를 부른다. 서명만 다르다(`SPEC.md` 5절).
#[test]
fn borrowed_and_owned_signatures_agree() {
    let cases = corpus::load_cases(&corpus_dir()).expect("코퍼스");
    for case in &cases {
        let options = Options { from: case.from, ..Default::default() };
        let mut a = Streamer::with_options(Channel::TelegramHtml, options.clone());
        let mut b = Streamer::with_options(Channel::TelegramHtml, options.clone());
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
