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
//!
//! `--batch jsonl` 은 문서 여럿을 한 프로세스로 잰다 — 답변 수백 개의 품질을 점검할 때
//! 문서마다 프로세스를 띄우지 않게.

use mdwire::{Channel, Images, LineBreaks, Options, Repairs, Streamer};
use serde_json::value::RawValue;
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};
use std::process::ExitCode;

const USAGE: &str = "\
mdwire — 에이전트 마크다운을 채팅 채널로 안전하게 내보낸다

사용법:
  mdwire --channel <채널> [--stream] [--report]
  mdwire --channel <채널> --batch jsonl

채널:
  telegram-html · slack-markdown · github-markdown · notion-markdown · plain · html

옵션:
  --channel <이름>      필수
  --limit <글자 수>     조각 한도. 기본은 채널의 한도 — plain 을 텔레그램에 보내면 4096
  --html-line-breaks <br|space>   html: 블록 안 줄바꿈. 기본 br
  --html-images <link|load>       html: 이미지를 링크로만(기본) · <img> 로 불러오기
  --html-schemes <목록>           html: 링크·이미지 주소로 받는 스킴, 쉼표로. 기본 http,https,mailto
  --stream              stdin 을 읽는 대로 내보낸다. 한도 분할은 하지 않는다
  --report              정규화가 고친 것을 stderr 에 JSON 한 줄로 낸다
  --batch jsonl         문서 여럿을 JSON lines 로 받아 문서마다 고친 것을 stdout 에
                        한 줄씩 낸다. 렌더 결과는 내지 않는다
  -h, --help            이 도움말
  -V, --version         버전

  값을 받는 옵션은 --이름=값 으로도 쓸 수 있다(--channel=slack-markdown).

출력:
  조각이 여럿이면 NUL(\\0) 로 구분한다. --stream 은 한 덩어리로 흘린다.

--batch jsonl:
  입력 한 줄  {\"id\": \"a1\", \"text\": \"마크다운\"}   (id 는 문자열·숫자, 없어도 된다)
  출력 한 줄  {\"line\":1,\"id\":\"a1\",\"closedEmphasis\":0,…}
  못 읽은 줄  {\"line\":2,\"error\":\"…\"} — 나머지는 계속 읽고, 하나라도 있으면 실패로 끝난다
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
    let mut batch = false;
    let mut options = Options::default();

    // `--이름=값` 도 받는다 — 두 인자로 편다. 흔한 표기라 `모르는 인자` 로 막으면 쓰는 쪽이 헷갈린다.
    let mut args = std::env::args().skip(1).flat_map(|a| match a.split_once('=') {
        Some((k, v)) if k.starts_with("--") => vec![k.to_string(), v.to_string()],
        _ => vec![a],
    });
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
            "--batch" => {
                let v = value()?;
                if v != "jsonl" {
                    return Err(format!("모르는 --batch 형식: {v} (jsonl 만 받는다)"));
                }
                batch = true;
            }
            "--html-line-breaks" => {
                options.html.line_breaks = match value()?.as_str() {
                    "br" => LineBreaks::Br,
                    "space" => LineBreaks::Space,
                    v => return Err(format!("--html-line-breaks 는 br 또는 space 여야 한다: {v}")),
                };
            }
            "--html-images" => {
                options.html.images = match value()?.as_str() {
                    "link" => Images::Link,
                    "load" => Images::Load,
                    v => return Err(format!("--html-images 는 link 또는 load 여야 한다: {v}")),
                };
            }
            "--html-schemes" => {
                let v = value()?;
                options.html.schemes = Some(v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect());
            }
            "--limit" => {
                let v = value()?;
                let n: usize = v.parse().ok().filter(|&n| n > 0).ok_or_else(|| format!("--limit 은 1 이상의 정수여야 한다: {v}"))?;
                options.limit = Some(n);
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

    if batch {
        if stream {
            return Err("--batch 와 --stream 은 같이 쓸 수 없다".to_string());
        }
        let failed = batch_jsonl(channel, options, io::stdin().lock(), &mut out).map_err(|e| e.to_string())?;
        return match failed {
            0 => Ok(()),
            n => Err(format!("{n}줄을 읽지 못했다")),
        };
    }

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

/// 고친 것을 JSON 한 줄로. 키는 npm 바인딩과 같다.
fn repairs_json(r: &Repairs) -> String {
    format!("{{{}}}", repairs_fields(r))
}

fn repairs_fields(r: &Repairs) -> String {
    format!(
        "\"closedEmphasis\":{},\"closedFence\":{},\"revertedCodeSpan\":{},\"droppedMarker\":{},\"guessedPair\":{},\
         \"escapedChar\":{},\"tagEmphasis\":{},\"strippedHtml\":{},\"rewrittenBullet\":{},\
         \"rewrittenTable\":{},\"convertedMarker\":{}",
        r.closed_emphasis,
        r.closed_fence,
        r.reverted_code_span,
        r.dropped_marker,
        r.guessed_pair,
        r.escaped_char,
        r.tag_emphasis,
        r.stripped_html,
        r.rewritten_bullet,
        r.rewritten_table,
        r.converted_marker
    )
}

/// 줄마다 문서 하나를 읽어 고친 것을 한 줄씩 낸다. 못 읽은 줄 수를 돌려준다.
///
/// **못 읽은 줄에서 멈추지 않는다.** 수백 줄짜리 점검에서 한 줄 때문에 나머지를 버리면 다시
/// 돌려야 한다 — 그 줄만 `error` 로 적고 계속 간다. 줄 번호(`line`, 1부터)는 늘 붙인다.
/// 빈 줄은 건너뛰되 번호는 센다 — 편집기에서 찾아가는 번호와 맞게.
fn batch_jsonl(channel: Channel, options: Options, input: impl BufRead, out: &mut impl Write) -> io::Result<usize> {
    let mut failed = 0;
    for (i, line) in input.split(b'\n').enumerate() {
        let line = line?;
        let n = i + 1;
        let line = line.strip_suffix(b"\r").unwrap_or(&line);
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match read_doc(line) {
            Ok((id, text)) => {
                let repairs = mdwire::render_with(&text, channel, options.clone()).repairs;
                let id = id.map(|id| format!("\"id\":{id},")).unwrap_or_default();
                writeln!(out, "{{\"line\":{n},{id}{}}}", repairs_fields(&repairs))?;
            }
            Err(e) => {
                failed += 1;
                writeln!(out, "{{\"line\":{n},\"error\":\"{e}\"}}")?;
            }
        }
    }
    out.flush()?;
    Ok(failed)
}

/// 한 줄을 `(id, text)` 로 읽는다. `id` 는 받은 글자 그대로 — 다시 직렬화하면 구현마다
/// 이스케이프가 달라진다. 에러 문구는 파서의 것이 아니라 이쪽 것이다 — Go 판과 같게.
fn read_doc(line: &[u8]) -> Result<(Option<String>, String), &'static str> {
    if has_lone_surrogate(line) {
        return Err("짝 없는 UTF-16 서로게이트가 있다");
    }
    let fields: HashMap<String, Box<RawValue>> =
        serde_json::from_slice(line).map_err(|_| "JSON 객체가 아니다")?;
    let text = fields.get("text").ok_or("text 가 없다")?;
    let text: String = serde_json::from_str(text.get()).map_err(|_| "text 는 문자열이어야 한다")?;
    let id = match fields.get("id").map(|v| v.get()) {
        None => None,
        Some(raw) if raw.starts_with('"') || raw.starts_with(|c: char| c == '-' || c.is_ascii_digit()) => {
            Some(raw.to_string())
        }
        Some(_) => return Err("id 는 문자열이나 숫자여야 한다"),
    };
    Ok((id, text))
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

/// 줄 어디엔가 짝 없는 서로게이트 이스케이프(`\ud800` 홀로)가 있는가.
///
/// **줄 전체를 거부하는 규칙 하나로 둔다.** serde_json 은 문자열로 푸는 자리(키·`text`)에서만
/// 거부하고 그냥 건너뛰는 값(`id`·모르는 필드)에서는 받는다. Go 의 encoding/json 은 어디서든
/// U+FFFD 로 바꿔 받는다. 파서마다 다른 자리를 맞추느니 앞에서 한 번 거른다. 역슬래시는
/// JSON 에서 문자열 안에만 올 수 있으니 문자열 경계를 따라가지 않아도 된다.
fn has_lone_surrogate(line: &[u8]) -> bool {
    let hex = |at: usize| -> Option<u32> {
        let s = std::str::from_utf8(line.get(at..at + 4)?).ok()?;
        u32::from_str_radix(s, 16).ok()
    };
    let mut i = 0;
    while i < line.len() {
        if line[i] != b'\\' {
            i += 1;
            continue;
        }
        if line.get(i + 1) != Some(&b'u') {
            i += 2;
            continue;
        }
        match hex(i + 2) {
            Some(0xD800..=0xDBFF) => {
                let low = (line.get(i + 6) == Some(&b'\\') && line.get(i + 7) == Some(&b'u')).then(|| hex(i + 8)).flatten();
                if !matches!(low, Some(0xDC00..=0xDFFF)) {
                    return true;
                }
                i += 12;
            }
            Some(0xDC00..=0xDFFF) => return true,
            _ => i += 6,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(input: &str) -> (String, usize) {
        let mut out = Vec::new();
        let failed = batch_jsonl(Channel::SlackMarkdown, Options::default(), input.as_bytes(), &mut out).expect("쓰기");
        (String::from_utf8(out).expect("UTF-8"), failed)
    }

    /// 문서마다 한 줄. `id` 는 받은 글자 그대로, 줄 번호는 빈 줄까지 센다.
    #[test]
    fn batch_reports_each_document_on_its_own_line() {
        let (out, failed) = batch("{\"id\":\"a\\\"1\",\"text\":\"_첫 줄\\n둘째_\"}\n\n{\"id\":7,\"text\":\"**안 닫힘\"}\r\n");
        assert_eq!(failed, 0);
        assert_eq!(
            out,
            "{\"line\":1,\"id\":\"a\\\"1\",\"closedEmphasis\":0,\"closedFence\":0,\"revertedCodeSpan\":0,\"droppedMarker\":0,\"guessedPair\":0,\
             \"escapedChar\":0,\"tagEmphasis\":0,\"strippedHtml\":0,\"rewrittenBullet\":0,\"rewrittenTable\":0,\
             \"convertedMarker\":1}\n\
             {\"line\":3,\"id\":7,\"closedEmphasis\":1,\"closedFence\":0,\"revertedCodeSpan\":0,\"droppedMarker\":0,\"guessedPair\":0,\
             \"escapedChar\":0,\"tagEmphasis\":0,\"strippedHtml\":0,\"rewrittenBullet\":0,\"rewrittenTable\":0,\
             \"convertedMarker\":0}\n"
        );
    }

    /// 못 읽은 줄은 그 줄만 `error` 로 적고 계속 간다.
    #[test]
    fn batch_keeps_going_past_bad_lines() {
        let (out, failed) = batch("not json\n{\"id\":[1],\"text\":\"x\"}\n{\"text\":3}\n{\"id\":\"x\"}\n{\"text\":\"끝\"}\n");
        assert_eq!(failed, 4);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "{\"line\":1,\"error\":\"JSON 객체가 아니다\"}");
        assert_eq!(lines[1], "{\"line\":2,\"error\":\"id 는 문자열이나 숫자여야 한다\"}");
        assert_eq!(lines[2], "{\"line\":3,\"error\":\"text 는 문자열이어야 한다\"}");
        assert_eq!(lines[3], "{\"line\":4,\"error\":\"text 가 없다\"}");
        assert!(lines[4].starts_with("{\"line\":5,\"closedEmphasis\":0"), "{}", lines[4]);
    }

    /// 짝 없는 서로게이트는 자리와 상관없이 줄을 거부한다. 짝이 맞으면(이모지) 받는다.
    #[test]
    fn lone_surrogates_are_rejected_anywhere() {
        for line in [r#"{"text":"\ud800"}"#, r#"{"id":"\udc00","text":"x"}"#, r#"{"a":"\ud800\u0041","text":"x"}"#] {
            assert!(has_lone_surrogate(line.as_bytes()), "{line}");
        }
        for line in [r#"{"text":"\ud83d\ude00"}"#, r#"{"text":"\\ud800"}"#, r#"{"text":"\u0041"}"#] {
            assert!(!has_lone_surrogate(line.as_bytes()), "{line}");
        }
    }
}
