//! 아무 입력이나 넣어도 지켜야 하는 것.
//!
//! 코퍼스는 "실제로 겪은 고장"이고, 여기는 **아직 안 겪은 고장**이다 — 마커·백틱·꺾쇠·
//! 대괄호·줄바꿈을 무작위로 섞은 입력을 채널마다 돌려서, 죽지 않고(패닉), 조각마다
//! 한도를 지키고, 스트리밍이 완성본과 같고, 텔레그램 태그가 조각 안에서 닫히는지 본다.
//! 리뷰에서 잡힌 `a*** **x` 패닉이 정확히 이 부류였다.
//!
//! 난수는 xorshift 다 — 의존을 두지 않고, 시드가 고정이라 실패가 재현된다.

use mdwire::{Channel, Options, Streamer};
use mdwire_harness::check::{check, Rule};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// LLM 이 실제로 쏟아내는 부품들. 문법 조각과 글자를 같은 확률로 섞는다.
const PIECES: &[&str] = &[
    "*", "**", "***", "_", "__", "~", "~~", "`", "``", "```", "```go\n", "\n```\n", "\\", "\\*", "\\_",
    "[", "]", "(", ")", "[텍스트](https://a.com/x_y)", "[괄호 (안) 텍스트](https://a.com/p)", "<", ">",
    "<https://a.com/a/very/long/path/that/keeps/going/and/going/past/eighty/characters/for/sure/index.html|긴 링크>", 
    "<b>", "</b>", "<sub>", "</sub>",
    "<br>", "<!-- 주석 -->", "<!-- 이건 아주 긴 주석이라 팔십 글자를 한참 넘어간다 — 스트리밍에서 이걸 놓으면 꺾쇠가 글자로 샌다 -->",  "<https://a.com/p|문서>", "<https://a.com/q>", "|", "| a | b |\n|---|---|\n",
    "#", "## ", "> ", "- ", "  - ", "1. ", "---\n", "\n", "\n\n", "\r\n", " ", "  ", "\t",
    "가", "나다", "한글 조사가", "이다.", "word", "x", "2", "का_x",  "&", "😀", "①", "•", ".md", "@id", "#40",
    // 브라우저 채널이 막아야 하는 것들 — 스킴, 이벤트 속성, 속성값에 드는 info, 마스킹 번호.
    "[x](javascript:alert(1))", "[m](MAILTO:a@b.c)", "<span onclick=\"x\">", "</span>", "<SUB>", "```x\" y=\"z\n", "4***-****-003*",
];

fn doc(rng: &mut Rng) -> String {
    let n = 1 + rng.below(60);
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(PIECES[rng.below(PIECES.len())]);
    }
    s
}

fn chunks(s: &str, n: usize) -> Vec<&str> {
    let idx: Vec<usize> = s.char_indices().map(|(i, _)| i).collect();
    let mut out = Vec::new();
    let mut from = 0;
    for k in (n..idx.len()).step_by(n) {
        out.push(&s[from..idx[k]]);
        from = idx[k];
    }
    out.push(&s[from..]);
    out
}

#[test]
fn random_input_never_breaks_the_invariants() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut failures = Vec::new();
    // 게이트에서는 3,000회. 더 파고 싶으면 `MDWIRE_FUZZ_ROUNDS` 로 늘린다.
    let rounds: usize = std::env::var("MDWIRE_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(3000);
    for round in 0..rounds {
        let input = doc(&mut rng);
        for channel in Channel::all() {
            let options = Options::default();
            let parts = mdwire::render_with(&input, channel, options.clone()).parts;
            let joined = parts.join("\0");
            // 한도 · 태그 · 이스케이프. 강조 범위와 낱말 손실은 여기서 보지 않는다 — 무작위
            // 마커 더미에는 "원문의 강조"라는 것이 없다.
            let bad: Vec<_> = check(&input, &joined, channel)
                .into_iter()
                .filter(|f| {
                    matches!(f.rule, Rule::OverLimit | Rule::UnclosedTag | Rule::DisallowedTag | Rule::RawHtmlChar)
                })
                .collect();
            if !bad.is_empty() {
                failures.push(format!("#{round} {}: {bad:?}\n  입력: {input:?}", channel.name()));
            }
            // 스트리밍은 완성본과 같다 — 한도를 넘겨 나뉜 것은 건너뛴다(코퍼스 테스트와 같은 이유).
            if parts.len() == 1 {
                for size in [1usize, 3, 11] {
                    let mut s = Streamer::with_options(channel, options.clone());
                    let mut got = String::new();
                    for c in chunks(&input, size) {
                        s.push_into(c, &mut got);
                    }
                    s.finish_into(&mut got);
                    if got != parts[0] {
                        failures.push(format!(
                            "#{round} {} 조각 {size}: 스트리밍이 다르다\n  입력: {input:?}\n  완성본: {:?}\n  스트리밍: {got:?}",
                            channel.name(),
                            parts[0]
                        ));
                        break;
                    }
                }
            }
        }
        if failures.len() >= 10 {
            break;
        }
    }
    assert!(failures.is_empty(), "{}건 실패:\n{}", failures.len(), failures.join("\n"));
}

/// 한도를 넘는 무작위 입력 — 분할이 조각마다 한도를 지키고 태그를 닫는가.
#[test]
fn random_long_input_splits_cleanly() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let mut failures = Vec::new();
    for round in 0..150 {
        let mut input = String::new();
        // 짧은 문서 여러 개를 이어서 **모든 채널의** 한도를 확실히 넘긴다 — 가장 큰
        // 한도의 두 배. 텔레그램만 넘기면 나머지 채널의 분할 경로는 한 번도 안 돈다.
        // **GitHub 한도(65,536)는 열 번에 한 번만 넘긴다.** 매번 채우면 퍼즈가 여덟 배
        // 느려지는데(8초 → 67초) 분할 경로는 슬랙과 같은 코드다.
        let wide = round % 10 == 0;
        // 한도가 없는 채널(html)은 나누지 않으니 이 시험 밖이다.
        let reach = |c: &Channel| c.limit() < usize::MAX && (wide || c.limit() <= Channel::SlackMarkdown.limit());
        let target = Channel::all().iter().filter(|c| reach(c)).map(|c| c.limit()).max().expect("채널이 있다") * 2;
        while input.chars().count() < target {
            input.push_str(&doc(&mut rng));
        }
        for channel in Channel::all().into_iter().filter(reach) {
            let parts = mdwire::render(&input, channel);
            if parts.len() < 2 {
                failures.push(format!("#{round} {}: 한도를 넘겼는데 안 나뉘었다", channel.name()));
            }
            let joined = parts.join("\0");
            let bad: Vec<_> = check(&input, &joined, channel)
                .into_iter()
                .filter(|f| {
                    matches!(f.rule, Rule::OverLimit | Rule::UnclosedTag | Rule::DisallowedTag | Rule::RawHtmlChar)
                })
                .collect();
            if !bad.is_empty() {
                failures.push(format!("#{round} {} ({}조각): {bad:?}", channel.name(), parts.len()));
            }
            if parts.iter().any(|p| p.is_empty()) {
                failures.push(format!("#{round} {}: 빈 조각", channel.name()));
            }
        }
        if failures.len() >= 10 {
            break;
        }
    }
    assert!(failures.is_empty(), "{}건 실패:\n{}", failures.len(), failures.join("\n"));
}

/// 호출자가 준 가장 작은 한도(`MIN_LIMIT`)에서도 조각마다 한도를 지키고 태그를 닫는가. 한도가 작을수록
/// 마크업을 닫고 다시 여는 몫이 커서 분할기의 가장자리가 드러난다.
#[test]
fn smallest_caller_limit_splits_cleanly() {
    let mut rng = Rng(0x5EED_0000_0000_0256);
    let mut failures = Vec::new();
    for round in 0..200 {
        let mut input = String::new();
        while input.chars().count() < mdwire::MIN_LIMIT * 4 {
            input.push_str(&doc(&mut rng));
        }
        for channel in [
            Channel::TelegramHtml,
            Channel::SlackMarkdown,
            Channel::GithubMarkdown,
            Channel::NotionMarkdown,
            Channel::Plain,
        ] {
            let options = Options { limit: Some(mdwire::MIN_LIMIT), ..Default::default() };
            let parts = mdwire::render_with(&input, channel, options).parts;
            // 노션 표는 조각마다 온전해야 한다 — 분할기는 태그를 모르고 줄로 끊는다.
            if channel == Channel::NotionMarkdown
                && parts.iter().any(|p| p.matches("<table ").count() != p.matches("</table>").count())
            {
                failures.push(format!("#{round} notion-markdown: 조각 안에서 표가 갈렸다"));
            }
            if parts.iter().any(|p| p.chars().count() > mdwire::MIN_LIMIT) {
                failures.push(format!("#{round} {}: 한도를 넘은 조각", channel.name()));
            }
            if channel == Channel::TelegramHtml {
                let bad: Vec<_> = check(&input, &parts.join("\0"), channel)
                    .into_iter()
                    .filter(|f| matches!(f.rule, Rule::UnclosedTag | Rule::DisallowedTag | Rule::RawHtmlChar))
                    .collect();
                if !bad.is_empty() {
                    failures.push(format!("#{round} telegram-html: {bad:?}"));
                }
            }
        }
        if failures.len() >= 10 {
            break;
        }
    }
    assert!(failures.is_empty(), "{}건 실패:\n{}", failures.len(), failures.join("\n"));
}
