//! Browser and npm bindings.
//!
//! **`wasm-bindgen` lives only in this crate** (`SPEC.md` section 3). Users of the core
//! stay dependency-free.
//!
//! The published package is the **bundler target** (`scripts/build-npm.sh`). The bundler
//! initializes wasm, so you do not call `init()`. You only need it when you use the `web`
//! target directly.
//!
//! ```js
//! import { render, Streamer } from "@minjun0219/mdwire";
//!
//! const parts = render(markdown, "telegram-html");
//! const { parts: p, repairs } = renderWithReport(markdown, "slack-markdown");
//!
//! const s = new Streamer("slack-markdown");
//! let out = "";
//! for await (const chunk of stream) {
//!   out += s.push(chunk);
//!   await edit(out + s.closeOpen());   // close and append only when sending mid-stream
//! }
//! out += s.finish();
//! ```
//!
//! Crossing the JS boundary copies anyway. So instead of the core's `push_into`,
//! returning a `String` is the natural shape here. It is not an extra copy:
//! that one copy is the boundary itself.

#![forbid(unsafe_code)]

use mdwire::{Channel, Options};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const OPTIONS_TS: &str = r#"
/** Conversion options. Anything omitted uses the channel's default. */
export interface RenderOptions {
  /** Part size limit (in characters). Defaults to the channel's limit; for example, 4096 when sending plain to Telegram. Streaming does not split. */
  limit?: number;
  /** Policy for the browser channel ("html"). The defaults are the most conservative. */
  html?: {
    /** Line breaks inside a block. "br" (default) emits `<br>`; "space" lets the browser collapse them into spaces. */
    lineBreaks?: "br" | "space";
    /** Images. "link" (default) loads nothing until clicked; "load" emits `<img>`. */
    images?: "link" | "load";
    /** Schemes allowed in link and image URLs (without the colon). If given, only these are allowed. Default ["http","https","mailto"]. */
    schemes?: string[];
  };
}
"#;

#[wasm_bindgen]
extern "C" {
    /// The options object from JS. Fields are read as properties, without `js-sys`.
    #[wasm_bindgen(typescript_type = "RenderOptions")]
    pub type RenderOptions;

    #[wasm_bindgen(method, getter)]
    fn limit(this: &RenderOptions) -> Option<f64>;

    #[wasm_bindgen(method, getter)]
    fn html(this: &RenderOptions) -> Option<HtmlOptionsJs>;

    /// The `RenderOptions.html` object.
    pub type HtmlOptionsJs;

    #[wasm_bindgen(method, getter, js_name = lineBreaks)]
    fn line_breaks(this: &HtmlOptionsJs) -> Option<String>;

    #[wasm_bindgen(method, getter)]
    fn images(this: &HtmlOptionsJs) -> Option<String>;

    #[wasm_bindgen(method, getter)]
    fn schemes(this: &HtmlOptionsJs) -> Option<Vec<String>>;
}

/// Converts a complete document. Returns an array of parts; it has more than one when the text exceeds the limit.
#[wasm_bindgen]
pub fn render(input: &str, channel: &str, options: Option<RenderOptions>) -> Result<Vec<String>, JsError> {
    Ok(mdwire::render_with(input, parse_channel(channel)?, parse_options(options)?).parts)
}

/// Like [`render`], but also returns **what normalization repaired**. Use it to log
/// how often the model breaks formatting.
#[wasm_bindgen(js_name = renderWithReport)]
pub fn render_with_report(
    input: &str,
    channel: &str,
    options: Option<RenderOptions>,
) -> Result<Rendered, JsError> {
    let out = mdwire::render_with(input, parse_channel(channel)?, parse_options(options)?);
    Ok(Rendered { parts: out.parts, repairs: out.repairs.into() })
}

/// The result of `renderWithReport`.
#[wasm_bindgen]
pub struct Rendered {
    parts: Vec<String>,
    repairs: Repairs,
}

#[wasm_bindgen]
impl Rendered {
    /// The parts. Just one if the text fit within the limit.
    #[wasm_bindgen(getter)]
    pub fn parts(&self) -> Vec<String> {
        self.parts.clone()
    }

    /// What normalization repaired.
    #[wasm_bindgen(getter)]
    pub fn repairs(&self) -> Repairs {
        self.repairs
    }
}

/// Counts of what normalization repaired and what was changed to fit the channel.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Repairs {
    /// Emphasis left open at the end of a block, closed for you.
    #[wasm_bindgen(js_name = closedEmphasis)]
    pub closed_emphasis: usize,
    /// Code fences left open at the end of the document, closed for you.
    #[wasm_bindgen(js_name = closedFence)]
    pub closed_fence: usize,
    /// Unmatched backtick runs, turned back into literal text.
    #[wasm_bindgen(js_name = revertedCodeSpan)]
    pub reverted_code_span: usize,
    /// Unmatched `**` markers that were dropped.
    #[wasm_bindgen(js_name = droppedMarker)]
    pub dropped_marker: usize,
    /// Characters escaped because the channel would read them as syntax (GitHub's `\~`, `\<`, `\*`).
    #[wasm_bindgen(js_name = escapedChar)]
    pub escaped_char: usize,
    /// Emphasis emitted differently because the channel would not read the marker in that position (GitHub's `<strong>`, Slack's U+2060).
    #[wasm_bindgen(js_name = tagEmphasis)]
    pub tag_emphasis: usize,
    /// Raw HTML stripped from the source (tags, comments, and `<br>` turned into line breaks).
    #[wasm_bindgen(js_name = strippedHtml)]
    pub stripped_html: usize,
    /// List bullets rewritten with a different symbol.
    #[wasm_bindgen(js_name = rewrittenBullet)]
    pub rewritten_bullet: usize,
    /// Tables rewritten in a different shape from the source.
    #[wasm_bindgen(js_name = rewrittenTable)]
    pub rewritten_table: usize,
    /// Emphasis markers and `<url|text>` links rewritten in a different notation.
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

/// The channel's length limit (in characters). Exposed for callers that handle parts themselves.
#[wasm_bindgen]
pub fn limit(channel: &str) -> Result<usize, JsError> {
    Ok(parse_channel(channel)?.limit())
}

/// Streaming converter.
///
/// Push a chunk and it returns only what is safe to emit now. Markup cut off at the
/// chunk boundary stays inside. This is what you want when appending tokens to the
/// screen as they arrive.
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

    /// What normalization has repaired so far. After `finish`, this covers the whole document.
    pub fn repairs(&self) -> Repairs {
        self.inner.repairs().into()
    }

    pub fn push(&mut self, chunk: &str) -> String {
        self.buf.clear();
        self.inner.push_into(chunk, &mut self.buf);
        self.buf.clone()
    }

    /// **To send what you have received so far as is, append this after it.**
    ///
    /// It does not touch the state, so streaming continues afterwards. Do not add it
    /// to the accumulated output itself; append it only right before sending. Use it
    /// when editing a message as tokens arrive.
    ///
    /// ```js
    /// acc += s.push(chunk);
    /// await edit(acc + s.closeOpen());   // leave acc unchanged
    /// ```
    #[wasm_bindgen(js_name = closeOpen)]
    pub fn close_open(&self) -> String {
        let mut out = String::new();
        self.inner.close_open(&mut out);
        out
    }

    /// **The tail that would follow the final output if input ended now.** It goes in the
    /// same place as `closeOpen`, but also draws what is being held back (open emphasis,
    /// table rows, code spans). This is the default when you redraw the whole message.
    /// It is a guess, so later chunks may change the shape. Check `revised()` after the end.
    ///
    /// ```js
    /// acc += s.push(chunk);
    /// await edit(acc + s.preview());      // call only when drawing (costs as much as the open block)
    /// acc += s.finish();
    /// if (s.revised()) await edit(acc);   // skip if the last screen already is the final output
    /// ```
    pub fn preview(&mut self) -> String {
        self.inner.preview().to_string()
    }

    /// Whether the final output differs from the last `preview()`. Check it after `finish`.
    pub fn revised(&self) -> bool {
        self.inner.revised()
    }

    /// Input has ended. Emits what is left and closes open markup.
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

fn parse_options(options: Option<RenderOptions>) -> Result<Options, JsError> {
    let mut out = Options::default();
    let Some(options) = options else { return Ok(out) };
    if let Some(n) = options.limit() {
        out.limit = Some(limit_of(n).map_err(|e| JsError::new(&e))?);
    }
    if let Some(html) = options.html() {
        out.html.line_breaks = match html.line_breaks().as_deref() {
            None | Some("br") => mdwire::LineBreaks::Br,
            Some("space") => mdwire::LineBreaks::Space,
            Some(v) => return Err(JsError::new(&format!("html.lineBreaks 는 br 또는 space 여야 한다: {v}"))),
        };
        out.html.images = match html.images().as_deref() {
            None | Some("link") => mdwire::Images::Link,
            Some("load") => mdwire::Images::Load,
            Some(v) => return Err(JsError::new(&format!("html.images 는 link 또는 load 여야 한다: {v}"))),
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
        Err(format!("limit 은 1 이상의 정수여야 한다: {n}"))
    }
}

#[cfg(test)]
mod tests {
    //! 호스트 타깃에서도 도는 것만 둔다. wasm 경계 자체는 여기서 못 잰다.
    use super::*;

    #[test]
    fn channel_names_and_limits_round_trip() {
        assert!(channel_of("telegram-html").is_ok());
        assert!(channel_of("없는채널").is_err());
        assert_eq!(limit_of(4096.0), Ok(4096));
        assert!(limit_of(0.0).is_err() && limit_of(1.5).is_err() && limit_of(f64::NAN).is_err());
    }

    #[test]
    fn render_goes_through_the_core() {
        let parts = render("**굵게**", "telegram-html", None).expect("변환");
        assert_eq!(parts, vec!["<b>굵게</b>"]);
    }
}
