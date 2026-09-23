//! 브라우저·npm 바인딩.
//!
//! **`wasm-bindgen` 은 이 crate 에만 있다**(`SPEC.md` 3절). 코어를 쓰는 쪽은
//! 의존 없이 남는다.
//!
//! 올리는 것은 **bundler 타깃**이다(`scripts/build-npm.sh`). 번들러가 wasm 초기화를
//! 맡으므로 `init()` 을 부르지 않는다 — `web` 타깃으로 직접 쓸 때만 필요하다.
//!
//! ```js
//! import { render, Streamer } from "mdwire";
//!
//! const parts = render(markdown, "telegram-html");
//!
//! const s = new Streamer("slack-markdown");
//! let out = "";
//! for await (const chunk of stream) {
//!   out += s.push(chunk);
//!   await edit(out + s.closeOpen());   // 중간에 보낼 때만 닫아 붙인다
//! }
//! out += s.finish();
//! ```
//!
//! 경계를 넘을 때는 어차피 복사가 일어난다. 그래서 여기서는 코어가 고른
//! `push_into` 대신 `String` 을 돌려주는 모양이 자연스럽다 — 복사를 한 번 더
//! 하는 것이 아니라, 그 한 번이 경계 자체다.

#![forbid(unsafe_code)]

use mdwire::Channel;
use wasm_bindgen::prelude::*;

/// 완성된 문서를 변환한다. 한도를 넘으면 조각 배열로 돌아온다.
#[wasm_bindgen]
pub fn render(input: &str, channel: &str) -> Result<Vec<String>, JsError> {
    Ok(mdwire::render(input, parse_channel(channel)?))
}

/// 채널의 길이 한도(문자 수). 조각을 직접 다루려는 호출자를 위해 열어 둔다.
#[wasm_bindgen]
pub fn limit(channel: &str) -> Result<usize, JsError> {
    Ok(parse_channel(channel)?.limit())
}

/// 스트리밍 변환기.
///
/// 조각을 넣으면 지금 안전하게 내보낼 수 있는 만큼만 돌려준다. 경계에 걸린 마크업은
/// 안에 남는다 — 토큰이 흘러들어오는 대로 화면에 붙이는 쪽이 이것 때문에 쓴다.
#[wasm_bindgen]
pub struct Streamer {
    inner: mdwire::Streamer,
    buf: String,
}

#[wasm_bindgen]
impl Streamer {
    #[wasm_bindgen(constructor)]
    pub fn new(channel: &str) -> Result<Streamer, JsError> {
        Ok(Streamer {
            inner: mdwire::Streamer::new(parse_channel(channel)?),
            buf: String::new(),
        })
    }

    pub fn push(&mut self, chunk: &str) -> String {
        self.buf.clear();
        self.inner.push_into(chunk, &mut self.buf);
        self.buf.clone()
    }

    /// **지금까지 받은 것을 그대로 보내려면 이걸 뒤에 붙인다.**
    ///
    /// 상태는 건드리지 않으므로 붙인 뒤에도 스트리밍은 이어진다. 누적본 자체에는
    /// 넣지 말고, 보내기 직전에만 붙인다. 토큰이 오는 대로 메시지를 편집하는 쪽이 쓴다.
    ///
    /// ```js
    /// acc += s.push(chunk);
    /// await edit(acc + s.closeOpen());   // 누적본은 그대로 둔다
    /// ```
    #[wasm_bindgen(js_name = closeOpen)]
    pub fn close_open(&self) -> String {
        let mut out = String::new();
        self.inner.close_open(&mut out);
        out
    }

    /// 입력이 끝났다. 남은 것을 내보내고 열린 마크업을 닫는다.
    pub fn finish(&mut self) -> String {
        self.buf.clear();
        self.inner.finish_into(&mut self.buf);
        self.buf.clone()
    }
}

// 이름 해석은 `JsError` 를 모르는 순수 함수로 둔다. `JsError::new` 는 wasm 밖에서
// 부를 수 없어서, 섞어 두면 호스트 타깃에서 테스트할 수 없게 된다.

fn channel_of(name: &str) -> Result<Channel, String> {
    Channel::parse(name).ok_or_else(|| format!("모르는 채널: {name}"))
}

fn parse_channel(name: &str) -> Result<Channel, JsError> {
    channel_of(name).map_err(|e| JsError::new(&e))
}

#[cfg(test)]
mod tests {
    //! 호스트 타깃에서도 도는 것만 둔다. wasm 경계 자체는 여기서 못 잰다.
    use super::*;

    #[test]
    fn channel_and_policy_names_round_trip() {
        assert!(channel_of("telegram-html").is_ok());
        assert!(channel_of("없는채널").is_err());
    }

    #[test]
    fn render_goes_through_the_core() {
        let parts = render("**굵게**", "telegram-html").expect("변환");
        assert_eq!(parts, vec!["<b>굵게</b>"]);
    }
}
