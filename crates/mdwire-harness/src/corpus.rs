//! 코퍼스 실행기.
//!
//! 두 모드가 있다.
//!
//! - **코퍼스 모드** — `cases/<이름>/input.md` 를 돌려 `<채널>.txt` 와 대조한다.
//!   기대 출력 파일이 없는 채널은 그 케이스에서 대조하지 않는다(불변식은 그래도 잰다).
//!   **한도를 넘겨 조각으로 나뉘는 케이스는 기대 출력을 두지 않는다** — 조각 구분자가
//!   NUL 이라 파일이 바이너리가 되고, 그러면 정본이 읽히지 않는다. 그런 케이스의 고장은
//!   "조각이 저마다 유효한가"라서 불변식 쪽이 정확히 그것을 잰다.
//! - **훑기 모드** — 아무 디렉토리나 받아 `.md` 를 찾아 불변식만 잰다.
//!   실제 에이전트 문서로 잴 때 쓴다. **그 문서들은 이 저장소에 넣지 않는다** —
//!   공개 저장소이고 남의 글이다.

use crate::adapter::Renderer;
use crate::check::{self, Finding};
use mdwire::Channel;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 코퍼스 케이스 하나.
#[derive(Debug, Clone)]
pub struct Case {
    pub name: String,
    pub input: String,
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
        cases.push(Case { name, input: fs::read_to_string(&input_path)?, expected });
    }
    Ok(cases)
}

/// 기대 출력 대조는 **끝의 줄바꿈을 무시**한다. 파일 끝 줄바꿈은 편집기가 붙였다 뗐다 하는
/// 것이지 이 라이브러리의 동작이 아니다.
fn normalize(s: &str) -> &str {
    s.trim_end_matches('\n')
}

pub fn run_case(case: &Case, renderer: &dyn Renderer, channel: Channel) -> Outcome {
    let mut outcome = Outcome {
        source: case.name.clone(),
        channel,
        compared: None,
        findings: Vec::new(),
        error: None,
    };
    match renderer.render(&case.input, channel) {
        Ok(actual) => {
            outcome.findings = check::check(&case.input, &actual, channel);
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
            };
            match renderer.render(&input, channel) {
                Ok(actual) => outcome.findings = check::check(&input, &actual, channel),
                Err(e) => outcome.error = Some(e),
            }
            out.push(outcome);
        }
    }
    Ok(out)
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
        "\n{} 회 실행 · 대조 {} (불일치 {}) · 불변식 위반 {} · 에러 {}",
        s.runs, s.compared, s.mismatched, s.findings, s.errors
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
