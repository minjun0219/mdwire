//! 채점 대상을 붙이는 자리.
//!
//! 요구하는 것은 **문자열 in → 문자열 out** 하나뿐이다. 그래서 다른 언어로 쓰인 구현도
//! [`Command`] 로 프로세스 경계에서 붙여 같은 잣대에 올릴 수 있다. 채점기가 언어를
//! 알 필요가 없어야 비교가 성립한다.
//!
//! 분할 결과는 NUL 로 이어 붙인 한 문자열로 돌려준다 — CLI 와 같은 규약이다
//! (`SPEC.md` 5절).

use mdwire::{Channel, CjkPolicy};
use std::io::Write;
use std::process::{Command as Proc, Stdio};

/// 채점 대상 하나.
pub trait Renderer {
    /// 표에 찍힐 이름.
    fn name(&self) -> String;
    /// 입력을 채널용으로 변환한다. 조각이 여럿이면 NUL 로 잇는다.
    fn render(&self, input: &str, channel: Channel) -> Result<String, String>;
}

/// 이 저장소의 구현.
pub struct Mdwire {
    pub cjk: CjkPolicy,
}

impl Default for Mdwire {
    fn default() -> Self {
        Self { cjk: CjkPolicy::Auto }
    }
}

impl Renderer for Mdwire {
    fn name(&self) -> String {
        "mdwire".to_string()
    }

    fn render(&self, input: &str, channel: Channel) -> Result<String, String> {
        Ok(mdwire::render(input, channel, self.cjk).join("\0"))
    }
}

/// 외부 구현. stdin 으로 넣고 stdout 으로 받는다.
///
/// `argv` 안의 `{channel}` 은 채널 이름으로 치환된다. 예:
/// `--cmd "node tools/convert.js --target {channel}"`.
pub struct Command {
    pub label: String,
    pub argv: Vec<String>,
}

impl Command {
    /// 공백으로 끊어 argv 를 만든다. 따옴표는 다루지 않는다 —
    /// 복잡한 호출은 셸 스크립트 하나를 만들어 그걸 가리키는 편이 낫다.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let argv: Vec<String> = spec.split_whitespace().map(str::to_string).collect();
        if argv.is_empty() {
            return Err("--cmd 가 비었다".to_string());
        }
        Ok(Self { label: argv[0].clone(), argv })
    }
}

impl Renderer for Command {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn render(&self, input: &str, channel: Channel) -> Result<String, String> {
        let args: Vec<String> =
            self.argv.iter().map(|a| a.replace("{channel}", channel.name())).collect();
        let mut child = Proc::new(&args[0])
            .args(&args[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{} 실행 실패: {e}", args[0]))?;

        // 입력을 다 쓰기 전에 자식이 출력을 뱉으면 파이프가 막힌다. 쓰기는 따로 돌린다.
        let mut stdin = child.stdin.take().ok_or("stdin 을 열지 못했다")?;
        let payload = input.to_string();
        let writer = std::thread::spawn(move || stdin.write_all(payload.as_bytes()));

        let out = child.wait_with_output().map_err(|e| format!("대기 실패: {e}"))?;
        writer.join().map_err(|_| "입력 쓰기 스레드가 죽었다".to_string())?
            .map_err(|e| format!("입력 쓰기 실패: {e}"))?;

        if !out.status.success() {
            return Err(format!(
                "{} 가 {} 로 끝났다: {}",
                args[0],
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        String::from_utf8(out.stdout).map_err(|e| format!("출력이 UTF-8 이 아니다: {e}"))
    }
}

/// 클로저를 채점 대상으로 쓴다. 테스트에서 가짜 구현을 끼울 때 쓴다.
pub struct FnRenderer<F> {
    pub label: String,
    pub f: F,
}

impl<F> Renderer for FnRenderer<F>
where
    F: Fn(&str, Channel) -> Result<String, String>,
{
    fn name(&self) -> String {
        self.label.clone()
    }
    fn render(&self, input: &str, channel: Channel) -> Result<String, String> {
        (self.f)(input, channel)
    }
}
