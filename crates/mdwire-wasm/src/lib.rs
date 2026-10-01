//! 브라우저·npm 바인딩.
//!
//! **`wasm-bindgen` 은 이 crate 에만 있다**(`SPEC.md` 3절). 코어를 쓰는 쪽은
//! 의존 없이 남는다.
//!
//! 올리는 것은 **bundler 타깃**이다(`scripts/build-npm.sh`). 번들러가 wasm 초기화를
//! 맡으므로 `init()` 을 부르지 않는다 — `web` 타깃으로 직접 쓸 때만 필요하다.
//!
//! ```js
//! import { render, Streamer } from "@minjun0219/mdwire";
//!
//! const parts = render(markdown, "telegram-html");
//! const { parts: p, repairs } = renderWithReport(markdown, "slack-markdown", { from: "slack-mrkdwn" });
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

use mdwire::{Channel, Dialect, Options};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const OPTIONS_TS: &str = r#"
/** 변환 옵션. 생략하면 표준 마크다운 입력이다. */
export interface RenderOptions {
  /** 입력 표기. 슬랙 레거시 mrkdwn(`*굵게*` `~취소~`)으로 쓴 에이전트 출력이면 "slack-mrkdwn". */
  from?: "markdown" | "slack-mrkdwn";
  /** 조각 한도(글자 수). 생략하면 채널의 한도다 — plain 을 텔레그램에 보내면 4096. 스트리밍은 나누지 않는다. */
  limit?: number;
  /** 브라우저 채널("html")의 정책. 기본값이 가장 보수적이다. */
  html?: {
    /** 블록 안 줄바꿈. "br"(기본) 은 `<br>`, "space" 는 브라우저가 공백으로 접게 둔다. */
    lineBreaks?: "br" | "space";
    /** 이미지. "link"(기본) 는 누르기 전에 아무것도 안 불러온다, "load" 는 `<img>`. */
    images?: "link" | "load";
    /** 링크·이미지 주소로 받는 스킴(콜론 없이). 주면 그것만 받는다. 기본 ["http","https","mailto"]. */
    schemes?: string[];
  };
}
"#;

#[wasm_bindgen]
extern "C" {
    /// JS 쪽 옵션 객체. 필드를 속성으로 읽는다 — `js-sys` 없이.
    #[wasm_bindgen(typescript_type = "RenderOptions")]
    pub type RenderOptions;

    #[wasm_bindgen(method, getter)]
    fn from(this: &RenderOptions) -> Option<String>;

    #[wasm_bindgen(method, getter)]
    fn limit(this: &RenderOptions) -> Option<f64>;

    #[wasm_bindgen(method, getter)]
    fn html(this: &RenderOptions) -> Option<HtmlOptionsJs>;

    /// `RenderOptions.html` 객체.
    pub type HtmlOptionsJs;

    #[wasm_bindgen(method, getter, js_name = lineBreaks)]
    fn line_breaks(this: &HtmlOptionsJs) -> Option<String>;

    #[wasm_bindgen(method, getter)]
    fn images(this: &HtmlOptionsJs) -> Option<String>;

    #[wasm_bindgen(method, getter)]
    fn schemes(this: &HtmlOptionsJs) -> Option<Vec<String>>;
}

/// 완성된 문서를 변환한다. 한도를 넘으면 조각 배열로 돌아온다.
#[wasm_bindgen]
pub fn render(input: &str, channel: &str, options: Option<RenderOptions>) -> Result<Vec<String>, JsError> {
    Ok(mdwire::render_with(input, parse_channel(channel)?, parse_options(options)?).parts)
}

/// [`render`] 에 **정규화가 고친 것**을 같이 돌려준다. 모델이 얼마나 자주 서식을 깨는지
/// 로그로 남기려는 쪽이 쓴다.
#[wasm_bindgen(js_name = renderWithReport)]
pub fn render_with_report(
    input: &str,
    channel: &str,
    options: Option<RenderOptions>,
) -> Result<Rendered, JsError> {
    let out = mdwire::render_with(input, parse_channel(channel)?, parse_options(options)?);
    Ok(Rendered { parts: out.parts, repairs: out.repairs.into() })
}

/// `renderWithReport` 의 결과.
#[wasm_bindgen]
pub struct Rendered {
    parts: Vec<String>,
    repairs: Repairs,
}

#[wasm_bindgen]
impl Rendered {
    /// 조각들. 한도를 넘지 않았으면 하나다.
    #[wasm_bindgen(getter)]
    pub fn parts(&self) -> Vec<String> {
        self.parts.clone()
    }

    /// 정규화가 고친 것.
    #[wasm_bindgen(getter)]
    pub fn repairs(&self) -> Repairs {
        self.repairs
    }
}

/// 정규화가 고친 것과 채널에 맞춰 바꾼 것의 개수.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Repairs {
    /// 블록이 끝나도록 안 닫혀서 닫아 준 강조.
    #[wasm_bindgen(js_name = closedEmphasis)]
    pub closed_emphasis: usize,
    /// 문서 끝까지 안 닫혀서 닫아 준 코드펜스.
    #[wasm_bindgen(js_name = closedFence)]
    pub closed_fence: usize,
    /// 짝이 없어 글자로 되돌린 백틱 런.
    #[wasm_bindgen(js_name = revertedCodeSpan)]
    pub reverted_code_span: usize,
    /// 짝 잃은 채 버린 `**`.
    #[wasm_bindgen(js_name = droppedMarker)]
    pub dropped_marker: usize,
    /// 채널이 구문으로 읽을 글자를 탈출한 수(GitHub 의 `\~`·`\<`·`\*`).
    #[wasm_bindgen(js_name = escapedChar)]
    pub escaped_char: usize,
    /// 마커 대신 태그로 낸 강조(GitHub 의 `<strong>`).
    #[wasm_bindgen(js_name = tagEmphasis)]
    pub tag_emphasis: usize,
    /// 벗긴 원문 HTML(태그·주석·줄바꿈으로 바꾼 `<br>`).
    #[wasm_bindgen(js_name = strippedHtml)]
    pub stripped_html: usize,
    /// 다른 기호로 바꿔 쓴 목록 불릿.
    #[wasm_bindgen(js_name = rewrittenBullet)]
    pub rewritten_bullet: usize,
    /// 원문과 다른 모양으로 다시 쓴 표.
    #[wasm_bindgen(js_name = rewrittenTable)]
    pub rewritten_table: usize,
    /// 다른 표기로 바꿔 쓴 강조 마커와 `<url|텍스트>` 링크.
    #[wasm_bindgen(js_name = convertedMarker)]
    pub converted_marker: usize,
}

impl From<mdwire::Repairs> for Repairs {
    fn from(r: mdwire::Repairs) -> Self {
        Repairs {
            closed_emphasis: r.closed_emphasis,
            closed_fence: r.closed_fence,
            reverted_code_span: r.reverted_code_span,
            dropped_marker: r.dropped_marker,
            escaped_char: r.escaped_char,
            tag_emphasis: r.tag_emphasis,
            stripped_html: r.stripped_html,
            rewritten_bullet: r.rewritten_bullet,
            rewritten_table: r.rewritten_table,
            converted_marker: r.converted_marker,
        }
    }
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
    pub fn new(channel: &str, options: Option<RenderOptions>) -> Result<Streamer, JsError> {
        Ok(Streamer {
            inner: mdwire::Streamer::with_options(parse_channel(channel)?, parse_options(options)?),
            buf: String::new(),
        })
    }

    /// 지금까지 정규화가 고친 것. `finish` 뒤에 보면 문서 전체의 값이다.
    pub fn repairs(&self) -> Repairs {
        self.inner.repairs().into()
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

    /// **지금 입력이 끝났다면 확정분 뒤에 붙을 꼬리.** `closeOpen` 과 같은 자리에 들어가지만
    /// 붙들고 있던 것(열린 강조, 표 행, 코드 스팬)까지 그린다. 누적본을 통째로 다시 그리는 쪽의
    /// 기본값이다. 추측이라 뒤 조각이 모양을 바꿀 수 있다 — 끝난 뒤 `revised()` 로 본다.
    ///
    /// ```js
    /// acc += s.push(chunk);
    /// await edit(acc + s.preview());      // 화면을 그릴 때만 부른다(열린 블록만큼 든다)
    /// acc += s.finish();
    /// if (s.revised()) await edit(acc);   // 마지막 화면이 곧 완성본이면 건너뛴다
    /// ```
    pub fn preview(&mut self) -> String {
        self.inner.preview().to_string()
    }

    /// 완성본이 마지막 `preview()` 와 다른가 — `finish` 뒤에 본다.
    pub fn revised(&self) -> bool {
        self.inner.revised()
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

fn dialect_of(name: &str) -> Result<Dialect, String> {
    Dialect::parse(name).ok_or_else(|| format!("모르는 방언: {name}"))
}

fn parse_options(options: Option<RenderOptions>) -> Result<Options, JsError> {
    let mut out = Options::default();
    let Some(options) = options else { return Ok(out) };
    if let Some(from) = options.from() {
        out.from = dialect_of(&from).map_err(|e| JsError::new(&e))?;
    }
    if let Some(n) = options.limit() {
        out.limit = Some(limit_of(n).map_err(|e| JsError::new(&e))?);
    }
    if let Some(html) = options.html() {
        out.html.line_breaks = match html.line_breaks().as_deref() {
            None | Some("br") => mdwire::LineBreaks::Br,
            Some("space") => mdwire::LineBreaks::Space,
            Some(v) => return Err(JsError::new(&format!("html.lineBreaks 는 br · space 다: {v}"))),
        };
        out.html.images = match html.images().as_deref() {
            None | Some("link") => mdwire::Images::Link,
            Some("load") => mdwire::Images::Load,
            Some(v) => return Err(JsError::new(&format!("html.images 는 link · load 다: {v}"))),
        };
        out.html.schemes = html.schemes();
    }
    Ok(out)
}

/// JS 숫자를 한도로 읽는다. 1 이상의 정수만 받는다 — `NaN`·소수·음수를 조용히 깎으면 호출자
/// 실수가 한 글자짜리 조각 폭탄이 된다.
fn limit_of(n: f64) -> Result<usize, String> {
    if n.is_finite() && n >= 1.0 && n.fract() == 0.0 && n <= u32::MAX as f64 {
        Ok(n as usize)
    } else {
        Err(format!("limit 은 1 이상의 정수다: {n}"))
    }
}

#[cfg(test)]
mod tests {
    //! 호스트 타깃에서도 도는 것만 둔다. wasm 경계 자체는 여기서 못 잰다.
    use super::*;

    #[test]
    fn channel_and_dialect_names_round_trip() {
        assert!(channel_of("telegram-html").is_ok());
        assert!(channel_of("없는채널").is_err());
        assert_eq!(limit_of(4096.0), Ok(4096));
        assert!(limit_of(0.0).is_err() && limit_of(1.5).is_err() && limit_of(f64::NAN).is_err());
        assert!(dialect_of("slack-mrkdwn").is_ok());
        assert!(dialect_of("markdown").is_ok());
        assert!(dialect_of("mrkdwn").is_err());
    }

    #[test]
    fn render_goes_through_the_core() {
        let parts = render("**굵게**", "telegram-html", None).expect("변환");
        assert_eq!(parts, vec!["<b>굵게</b>"]);
    }
}
