//! `mdwire-check` — 코퍼스를 돌리고 불변식을 잰다.
//!
//! ```text
//! mdwire-check                              코퍼스 전부, 우리 구현
//! mdwire-check --scan ~/docs                실제 문서 더미에 불변식만
//! mdwire-check --cmd "node conv.js --to {channel}"   다른 구현을 같은 잣대로
//! ```
//!
//! 실패가 있으면 1 로 끝난다. CI 에 그대로 걸 수 있다.

use mdwire::Channel;
use mdwire_harness::{adapter, corpus};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
mdwire-check — 코퍼스 대조 + 구현 중립 불변식 채점

사용법:
  mdwire-check [옵션]

옵션:
  --corpus <디렉토리>   코퍼스 케이스 디렉토리 (기본: corpus/cases)
  --scan <디렉토리>     .md 를 훑어 불변식만 잰다. 기대 출력은 보지 않는다
  --cmd \"<명령>\"        외부 구현을 채점한다. {channel} 이 채널 이름으로 치환된다
  --channel <목록>      쉼표로 구분 (기본: telegram-html,slack-markdown,plain)
  -v, --verbose         통과한 것도 전부 찍는다
  -h, --help            이 도움말
";

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("mdwire-check: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<bool, String> {
    let mut corpus_dir: Option<PathBuf> = None;
    let mut scan_dir: Option<PathBuf> = None;
    let mut cmd: Option<String> = None;
    let mut channels: Vec<Channel> = Channel::all().to_vec();
    let mut verbose = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} 에 값이 없다"));
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(true);
            }
            "-v" | "--verbose" => verbose = true,
            "--corpus" => corpus_dir = Some(PathBuf::from(value()?)),
            "--scan" => scan_dir = Some(PathBuf::from(value()?)),
            "--cmd" => cmd = Some(value()?),
            "--channel" => {
                let v = value()?;
                channels = v
                    .split(',')
                    .map(|n| Channel::parse(n.trim()).ok_or_else(|| format!("모르는 채널: {n}")))
                    .collect::<Result<_, _>>()?;
            }
            other => return Err(format!("모르는 인자: {other}\n\n{USAGE}")),
        }
    }

    let renderer: Box<dyn adapter::Renderer> = match &cmd {
        Some(spec) => Box::new(adapter::Command::parse(spec)?),
        None => Box::new(adapter::Mdwire),
    };

    let outcomes = match scan_dir {
        Some(dir) => {
            println!("훑기: {} · 대상 {}", dir.display(), renderer.name());
            corpus::scan_dir(&dir, renderer.as_ref(), &channels)
                .map_err(|e| format!("{}: {e}", dir.display()))?
        }
        None => {
            let dir = corpus_dir.unwrap_or_else(default_corpus_dir);
            println!("코퍼스: {} · 대상 {}", dir.display(), renderer.name());
            corpus::run_corpus(&dir, renderer.as_ref(), &channels)
                .map_err(|e| format!("{}: {e}", dir.display()))?
        }
    };

    Ok(corpus::report(&outcomes, verbose))
}

/// 작업 디렉토리 기준으로 먼저 찾고, 없으면 이 crate 위치 기준으로 찾는다.
fn default_corpus_dir() -> PathBuf {
    let cwd = PathBuf::from("corpus/cases");
    if cwd.is_dir() {
        return cwd;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus/cases")
}
