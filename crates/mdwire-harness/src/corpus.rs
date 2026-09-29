//! 코퍼스 실행기.
//!
//! 두 모드가 있다.
//!
//! - **코퍼스 모드** — `cases/<이름>/input.md` 를 돌려 `<채널>.txt` 와 대조한다.
//!   입력이 표준 마크다운이 아니면 `from` 파일에 방언 이름(`slack-mrkdwn`)을 적는다.
//!   기대 출력 파일이 없는 채널은 그 케이스에서 대조하지 않는다(불변식은 그래도 잰다).
//!   **한도를 넘겨 조각으로 나뉘는 케이스는 기대 출력을 두지 않는다** — 조각 구분자가
//!   NUL 이라 파일이 바이너리가 되고, 그러면 정본이 읽히지 않는다. 그런 케이스의 고장은
//!   "조각이 저마다 유효한가"라서 불변식 쪽이 정확히 그것을 잰다.
//! - **훑기 모드** — 아무 디렉토리나 받아 `.md` 를 찾아 불변식만 잰다.
//!   실제 에이전트 문서로 잴 때 쓴다. **그 문서들은 이 저장소에 넣지 않는다** —
//!   공개 저장소이고 남의 글이다.

use crate::adapter::Renderer;
use crate::check::{self, Finding, Rule};
use mdwire::{Channel, Dialect};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 코퍼스 케이스 하나.
#[derive(Debug, Clone)]
pub struct Case {
    pub name: String,
    pub input: String,
    /// 입력 방언. `from` 파일이 없으면 표준 마크다운.
    pub from: Dialect,
    /// 채널 이름 → 기대 출력.
    pub expected: BTreeMap<String, String>,
}

/// 한 케이스 × 한 채널의 결과.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub source: String,
    pub channel: Channel,
    /// 기대 출력과 대조했다면 그 결과. 기대 파일이 없으면 `None`.
    pub compared: Option<Compare>,
    pub findings: Vec<Finding>,
    /// 구현이 에러를 냈다면.
    pub error: Option<String>,
    /// 구현이 이 케이스의 입력 방언을 받지 않아 돌리지 않았다.
    pub skipped: bool,
}

#[derive(Debug, Clone)]
pub struct Compare {
    pub matched: bool,
    pub expected: String,
    pub actual: String,
}

impl Outcome {
    pub fn ok(&self) -> bool {
        self.error.is_none()
            && self.findings.is_empty()
            && self.compared.as_ref().is_none_or(|c| c.matched)
    }
}

/// 코퍼스 디렉토리(`corpus/cases`)를 읽는다.
pub fn load_cases(dir: &Path) -> io::Result<Vec<Case>> {
    let mut cases = Vec::new();
    let mut entries: Vec<PathBuf> =
        fs::read_dir(dir)?.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();

    for path in entries {
        if !path.is_dir() {
            continue;
        }
        let input_path = path.join("input.md");
        if !input_path.exists() {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let mut expected = BTreeMap::new();
        for file in fs::read_dir(&path)?.filter_map(Result::ok) {
            let p = file.path();
            if p.extension().is_some_and(|e| e == "txt") {
                let channel = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                expected.insert(channel, fs::read_to_string(&p)?);
            }
        }
        let from = match fs::read_to_string(path.join("from")) {
            Ok(name) => Dialect::parse(name.trim()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, format!("{name:?}: 모르는 입력 방언 ({})", path.display()))
            })?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Dialect::Markdown,
            Err(e) => return Err(e),
        };
        cases.push(Case { name, input: fs::read_to_string(&input_path)?, from, expected });
    }
    Ok(cases)
}

/// 기대 출력 대조는 **끝의 줄바꿈을 무시**한다. 파일 끝 줄바꿈은 편집기가 붙였다 뗐다 하는
/// 것이지 이 라이브러리의 동작이 아니다.
fn normalize(s: &str) -> &str {
    s.trim_end_matches('\n')
}

/// 입력을 **표준 마크다운으로 읽어야** 재는 규칙인가. 다른 방언으로 쓴 입력에는 이 규칙들이
/// 틀린 답을 낸다 — mrkdwn 의 `*굵게*` 를 기울임으로 읽고 범위가 다르다고 한다. 그런 케이스는
/// 기대 출력 대조와 입력을 안 보는 규칙만으로 잰다(퍼즈와 같은 선택).
fn reads_input_as_markdown(rule: Rule) -> bool {
    matches!(rule, Rule::EmphasisRange | Rule::StrayMarker | Rule::TextLoss | Rule::TableMisaligned)
}

pub fn run_case(case: &Case, renderer: &dyn Renderer, channel: Channel) -> Outcome {
    let mut outcome = Outcome {
        source: case.name.clone(),
        channel,
        compared: None,
        findings: Vec::new(),
        error: None,
        skipped: false,
    };
    if !renderer.supports(case.from) {
        outcome.skipped = true;
        return outcome;
    }
    match renderer.render(&case.input, channel, case.from) {
        Ok(actual) => {
            outcome.findings = check::check(&case.input, &actual, channel);
            if case.from != Dialect::Markdown {
                outcome.findings.retain(|f| !reads_input_as_markdown(f.rule));
            }
            stream_findings(renderer, &case.input, channel, case.from, &actual, &mut outcome.findings);
            if let Some(expected) = case.expected.get(channel.name()) {
                outcome.compared = Some(Compare {
                    matched: normalize(expected) == normalize(&actual),
                    expected: expected.clone(),
                    actual: actual.clone(),
                });
            }
        }
        Err(e) => outcome.error = Some(e),
    }
    outcome
}

pub fn run_corpus(
    dir: &Path,
    renderer: &dyn Renderer,
    channels: &[Channel],
) -> io::Result<Vec<Outcome>> {
    let cases = load_cases(dir)?;
    let mut out = Vec::new();
    for case in &cases {
        for &channel in channels {
            out.push(run_case(case, renderer, channel));
        }
    }
    Ok(out)
}

/// 디렉토리를 훑어 `.md` 하나하나에 불변식을 잰다. 기대 출력은 없다.
pub fn scan_dir(dir: &Path, renderer: &dyn Renderer, channels: &[Channel]) -> io::Result<Vec<Outcome>> {
    let mut files = Vec::new();
    collect_md(dir, &mut files)?;
    files.sort();

    let mut out = Vec::new();
    for path in files {
        let input = fs::read_to_string(&path)?;
        let name = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().to_string();
        for &channel in channels {
            let mut outcome = Outcome {
                source: name.clone(),
                channel,
                compared: None,
                findings: Vec::new(),
                error: None,
                skipped: false,
            };
            match renderer.render(&input, channel, Dialect::Markdown) {
                Ok(actual) => {
                    outcome.findings = check::check(&input, &actual, channel);
                    stream_findings(renderer, &input, channel, Dialect::Markdown, &actual, &mut outcome.findings);
                }
                Err(e) => outcome.error = Some(e),
            }
            out.push(outcome);
        }
    }
    Ok(out)
}

/// 스트리밍으로 받은 것을 이어 붙인 결과가 완성본과 같은가 — append-only 계약.
///
/// **한 글자씩**과 **64글자씩** 두 번 흘린다. 한 글자씩은 경계가 모든 자리에 걸리는 가장
/// 가혹한 경우고, 64글자는 실제 토큰 흐름에 가깝다. 한도를 넘겨 나뉜 출력은 건너뛴다 —
/// 조각은 저마다 메시지 하나라 앞머리 줄바꿈을 털고 시작하고, 스트리밍은 한도를 모른다.
fn stream_findings(
    renderer: &dyn Renderer,
    input: &str,
    channel: Channel,
    from: Dialect,
    batch: &str,
    findings: &mut Vec<check::Finding>,
) {
    if batch.contains('\0') {
        return;
    }
    for chunk in [1, 64] {
        let Some(streamed) = renderer.stream(input, channel, from, chunk) else { return };
        if streamed != batch {
            let at = streamed.chars().zip(batch.chars()).take_while(|(a, b)| a == b).count();
            let around = |s: &str| s.chars().skip(at.saturating_sub(15)).take(40).collect::<String>();
            findings.push(check::Finding {
                rule: check::Rule::StreamDiverged,
                detail: format!(
                    "{chunk}글자씩 흘린 결과가 {at}번째 글자부터 다르다\n      완성본: …{}…\n      스트림: …{}…",
                    around(batch).replace('\n', "⏎"),
                    around(&streamed).replace('\n', "⏎")
                ),
            });
            return;
        }
    }
}

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)?.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_md(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "md" || e == "markdown" || e == "txt") {
            out.push(path);
        }
    }
    Ok(())
}

/// 결과 요약.
#[derive(Debug, Clone, Copy, Default)]
pub struct Summary {
    pub runs: usize,
    pub compared: usize,
    pub mismatched: usize,
    pub findings: usize,
    pub errors: usize,
    /// 입력 방언을 못 받아 건너뛴 실행.
    pub skipped: usize,
}

impl Summary {
    pub fn ok(self) -> bool {
        self.mismatched == 0 && self.findings == 0 && self.errors == 0
    }
}

pub fn summarize(outcomes: &[Outcome]) -> Summary {
    let mut s = Summary::default();
    for o in outcomes {
        s.runs += 1;
        s.skipped += usize::from(o.skipped);
        s.errors += usize::from(o.error.is_some());
        s.findings += o.findings.len();
        if let Some(c) = &o.compared {
            s.compared += 1;
            s.mismatched += usize::from(!c.matched);
        }
    }
    s
}

/// 사람이 읽을 보고서. 통과면 `true`.
pub fn report(outcomes: &[Outcome], verbose: bool) -> bool {
    for o in outcomes {
        if o.skipped && verbose {
            println!("\n[건너뜀] {} · {} — 입력 방언을 받지 않는 구현이다(`--cmd` 에 `{{from}}` 이 없다)", o.source, o.channel.name());
            continue;
        }
        if o.ok() && !verbose {
            continue;
        }
        let mark = if o.ok() { "ok" } else { "실패" };
        println!("\n[{mark}] {} · {}", o.source, o.channel.name());
        if let Some(e) = &o.error {
            println!("  에러: {e}");
        }
        for f in &o.findings {
            println!("  {f}");
        }
        if let Some(c) = &o.compared {
            if !c.matched {
                println!("  기대 출력과 다르다");
                println!("{}", diff(normalize(&c.expected), normalize(&c.actual)));
            }
        }
    }
    let s = summarize(outcomes);
    println!(
        "\n{} 회 실행 · 대조 {} (불일치 {}) · 불변식 위반 {} · 에러 {}{}",
        s.runs,
        s.compared,
        s.mismatched,
        s.findings,
        s.errors,
        if s.skipped > 0 { format!(" · 방언 때문에 건너뜀 {}", s.skipped) } else { String::new() }
    );
    s.ok()
}

/// 줄 단위 차이. 첫 다른 줄만 보여 준다 — 어디서 갈렸는지만 알면 된다.
fn diff(expected: &str, actual: &str) -> String {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    for i in 0..e.len().max(a.len()) {
        let (le, la) = (e.get(i).copied(), a.get(i).copied());
        if le != la {
            out.push_str(&format!("    {}번 줄\n", i + 1));
            out.push_str(&format!("      기대: {}\n", le.unwrap_or("<없음>")));
            out.push_str(&format!("      실제: {}\n", la.unwrap_or("<없음>")));
            return out;
        }
    }
    out.push_str("    줄은 같은데 끝이 다르다(공백 또는 줄바꿈)\n");
    out
}
