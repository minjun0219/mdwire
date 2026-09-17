//! stdin → stdout. 한 번에 채널 하나.
//!
//! CLI 가 있으면 어떤 언어의 에이전트든 파이프 하나로 쓴다 — FFI 도 바인딩도 없이.

use std::io::{self, Read, Write};

fn main() -> io::Result<()> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    // TODO: 인자 파싱(--channel, --cjk, --stream) 후 mdwire_core 호출
    io::stdout().write_all(input.as_bytes())
}
