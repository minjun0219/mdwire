//! stdin → stdout. 한 번에 채널 하나.
//!
//! CLI 가 있으면 어떤 언어의 에이전트든 파이프 하나로 쓴다 — FFI 도 바인딩도 없이.
//! 이게 이 저장소에서 가장 널리 쓰일 표면이다.
//!
//! ```text
//! cat out.md | mdwire --channel telegram-html
//! cat out.md | mdwire --channel slack-markdown --stream
//! ```
//!
//! 분할 결과는 **NUL 로 구분**한다(`SPEC.md` 5절). 셸에서 다루기 가장 쉽고,
//! 마크다운 본문에 안 나오는 바이트다.

use mdwire::{Channel, Dialect, Options, Repairs, Streamer};
use std::io::{self, BufRead, Read, Write};
use std::process::ExitCode;

const USAGE: &str = "\
mdwire — 에이전트 마크다운을 채팅 채널로 안전하게 내보낸다

사용법:
  mdwire --channel <채널> [--from <방언>] [--stream] [--report]

채널:
  telegram-html · slack-markdown · plain

옵션:
  --channel <이름>      필수
  --from <방언>         입력 표기. markdown(기본) · slack-mrkdwn
  --stream              stdin 을 읽는 대로 내보낸다. 한도 분할은 하지 않는다
  --report              정규화가 고친 것을 stderr 에 JSON 한 줄로 낸다
  -h, --help            이 도움말
  -V, --version         버전

출력:
  조각이 여럿이면 NUL(\\0) 로 구분한다. --stream 은 한 덩어리로 흘린다.
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mdwire: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut channel: Option<Channel> = None;
    let mut stream = false;
    let mut report = false;
    let mut options = Options::default();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} 에 값이 없다"));
        match arg.as_str() {
            "-V" | "--version" => {
                println!("mdwire {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "--stream" => stream = true,
            "--report" => report = true,
            "--from" => {
                let v = value()?;
                options.from =
                    Dialect::parse(&v).ok_or_else(|| format!("모르는 방언: {v}\n\n{USAGE}"))?;
            }
            "--channel" => {
                let v = value()?;
                channel = Some(
                    Channel::parse(&v).ok_or_else(|| format!("모르는 채널: {v}\n\n{USAGE}"))?,
                );
            }
            other => return Err(format!("모르는 인자: {other}\n\n{USAGE}")),
        }
    }

    let channel = channel.ok_or_else(|| format!("--channel 이 필요하다\n\n{USAGE}"))?;
    let stdout = io::stdout();
    let mut out = stdout.lock();

    if stream {
        let repairs = stream_stdin(channel, options, &mut out).map_err(|e| e.to_string())?;
        if report {
            eprintln!("{}", repairs_json(&repairs));
        }
        return Ok(());
    }

    let mut input = String::new();
    io::stdin().read_to_string(&mut input).map_err(|e| e.to_string())?;
    let rendered = mdwire::render_with(&input, channel, options);
    for (i, part) in rendered.parts.iter().enumerate() {
        if i > 0 {
            out.write_all(b"\0").map_err(|e| e.to_string())?;
        }
        out.write_all(part.as_bytes()).map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())?;
    if report {
        eprintln!("{}", repairs_json(&rendered.repairs));
    }
    Ok(())
}

/// 고친 것을 JSON 한 줄로. 키는 npm 바인딩과 같다 — 의존 없이 손으로 적는다.
fn repairs_json(r: &Repairs) -> String {
    format!(
        "{{\"closedEmphasis\":{},\"closedFence\":{},\"revertedCodeSpan\":{},\"droppedMarker\":{}}}",
        r.closed_emphasis, r.closed_fence, r.reverted_code_span, r.dropped_marker
    )
}

/// 읽는 대로 내보낸다. 버퍼는 재사용한다 — 조각마다 할당하지 않는 것이
/// 이 라이브러리가 서명을 그렇게 고른 이유다(`SPEC.md` 5절).
fn stream_stdin(channel: Channel, options: Options, out: &mut impl Write) -> io::Result<Repairs> {
    let mut streamer = Streamer::with_options(channel, options);
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut rendered = String::new();
    // 글자 경계에 걸려 남은 바이트. `fill_buf` 는 버퍼가 비기 전에는 다시 읽지 않으므로,
    // 버퍼 끝의 반쪽 글자는 **다음 회차에 홀로 돌아온다** — 그때 "잘못된 UTF-8" 로 죽던
    // 것을 여기 들고 있다가 뒤에 오는 바이트와 이어 붙인다.
    let mut carry: Vec<u8> = Vec::new();

    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            break;
        }
        let n = chunk.len();
        carry.extend_from_slice(chunk);
        reader.consume(n);
        // UTF-8 경계를 넘지 않는 만큼만 넘긴다. 남은 바이트는 다음 회차에 이어 읽는다.
        let valid = match std::str::from_utf8(&carry) {
            Ok(s) => s.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(e) => return Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        };
        if valid == 0 {
            continue;
        }
        let text = std::str::from_utf8(&carry[..valid]).expect("valid_up_to 까지는 UTF-8 이다");
        rendered.clear();
        streamer.push_into(text, &mut rendered);
        out.write_all(rendered.as_bytes())?;
        out.flush()?;
        carry.drain(..valid);
    }
    if !carry.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "입력이 글자 한가운데서 끝났다"));
    }

    rendered.clear();
    streamer.finish_into(&mut rendered);
    out.write_all(rendered.as_bytes())?;
    out.flush()?;
    Ok(streamer.repairs())
}
