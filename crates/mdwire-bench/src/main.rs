//! mdwire 벤치.
//!
//! `SPEC.md` 8절이 시킨 순서 그대로다 — **서명을 고르기 전에 잰다.**
//! 지금 재는 것은 스트리밍 반환 모양 네 가지의 *바닥값*이다. 변환 자체는
//! 네 후보가 똑같은 함수를 쓰므로, 표에 남는 차이는 **서명이 강제하는 비용**뿐이다.
//!
//! ```text
//! cargo run --release -p mdwire-bench
//! ```
//!
//! 디버그 빌드 수치는 보지 않는다. 할당 횟수는 같지만 시간이 10배 가까이 다르다.

mod rig;

use mdwire::{Channel, CjkPolicy, Streamer};
use rig::{measure, report, Sample};

#[global_allocator]
static ALLOC: rig::Counting = rig::Counting;

/// 스트리밍 조각 크기. `DESIGN.md` 의 실측이 64자 단위 흘리기에서 나왔다.
const CHUNK: usize = 64;

/// 네 후보가 공유하는 변환. 내용은 중요하지 않고 **입력 길이에 비례하는 일**이면 된다.
/// 후보 간 차이가 변환이 아니라 서명에서만 나오게 하려는 것이다.
fn transform(chunk: &str, out: &mut String) {
    for c in chunk.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// 후보 A — 조각마다 `String` 을 새로 만들어 돌려준다. 지금 `SPEC.md` 에 적힌 서명.
struct ShapeString;
impl ShapeString {
    fn push(&mut self, chunk: &str) -> String {
        let mut out = String::new();
        transform(chunk, &mut out);
        out
    }
}

/// 후보 B — 내부 버퍼를 재사용하고 슬라이스를 빌려준다.
/// 다음 `push` 까지만 유효하다는 제약이 붙는다.
struct ShapeSlice {
    buf: String,
}
impl ShapeSlice {
    fn push(&mut self, chunk: &str) -> &str {
        self.buf.clear();
        transform(chunk, &mut self.buf);
        &self.buf
    }
}

/// 후보 C — 호출자의 버퍼에 직접 쓴다.
struct ShapeSink;
impl ShapeSink {
    fn push_into(&mut self, chunk: &str, out: &mut String) {
        transform(chunk, out);
    }
}

/// 후보 D — 이벤트 콜백.
struct ShapeCallback {
    buf: String,
}
impl ShapeCallback {
    fn push_with(&mut self, chunk: &str, mut f: impl FnMut(&str)) {
        self.buf.clear();
        transform(chunk, &mut self.buf);
        f(&self.buf);
    }
}

fn main() {
    let doc = synthetic_doc();
    let prose = synthetic_prose();
    let pieces = split_chunks(&doc, CHUNK);
    let input_bytes = doc.len();
    let repeats = 30;

    println!("입력 {} bytes · 조각 {} 개 (조각 크기 {CHUNK}B)", input_bytes, pieces.len());

    // 모으는 쪽의 버퍼는 후보 밖에 두고 재사용한다. 후보가 강제하는 할당만 남기려는 것이다.
    let mut collected = String::with_capacity(input_bytes * 2);

    let samples: Vec<Sample> = vec![
        measure("A: push() -> String", input_bytes, repeats, || {
            let mut s = ShapeString;
            collected.clear();
            for p in &pieces {
                collected.push_str(&s.push(p));
            }
            pieces.len()
        }),
        measure("B: push() -> &str", input_bytes, repeats, || {
            let mut s = ShapeSlice { buf: String::new() };
            collected.clear();
            for p in &pieces {
                collected.push_str(s.push(p));
            }
            pieces.len()
        }),
        measure("C: push_into(&mut String)", input_bytes, repeats, || {
            let mut s = ShapeSink;
            collected.clear();
            for p in &pieces {
                s.push_into(p, &mut collected);
            }
            pieces.len()
        }),
        measure("D: push_with(|&str|)", input_bytes, repeats, || {
            let mut s = ShapeCallback { buf: String::new() };
            collected.clear();
            for p in &pieces {
                s.push_with(p, |out| collected.push_str(out));
            }
            pieces.len()
        }),
    ];

    report("스트리밍 반환 모양 — 서명이 강제하는 바닥 비용", &samples);
    println!(
        "\n변환은 넷이 같은 함수를 쓴다. 표의 차이는 전부 서명에서 나온 것이다.\n\
         '할당/조각' 이 서명을 고르는 기준이다 — 스트리밍 경로는 조각마다 불린다."
    );

    engine(&doc, &pieces, repeats);

    let prose_pieces = split_chunks(&prose, CHUNK);
    println!("\n산문만(표·코드펜스 없음) {} bytes · 조각 {} 개", prose.len(), prose_pieces.len());
    prose_only(&prose, &prose_pieces, repeats);
}

/// 스트리밍의 뜨거운 경로는 산문이다. 표와 코드펜스는 성격이 다르다 —
/// 표는 열 너비를 재려고 어차피 버퍼링하고, 셀 수만큼 할당한다.
/// **회귀 게이트는 이 수치에 건다.**
fn prose_only(doc: &str, pieces: &[&str], repeats: usize) {
    let mut out = String::with_capacity(doc.len() * 2);
    let mut samples = Vec::new();
    for (name, channel) in [
        ("새 Streamer · telegram-html", Channel::TelegramHtml),
        ("재사용 · telegram-html", Channel::TelegramHtml),
    ] {
        let reuse = name.starts_with("재사용");
        let mut kept = Streamer::new(channel, CjkPolicy::Auto);
        if reuse {
            out.clear();
            for p in pieces {
                kept.push_into(p, &mut out);
            }
            kept.finish_into(&mut out);
        }
        samples.push(measure(name, doc.len(), repeats, || {
            let mut fresh;
            let s = if reuse {
                &mut kept
            } else {
                fresh = Streamer::new(channel, CjkPolicy::Auto);
                &mut fresh
            };
            out.clear();
            for p in pieces {
                s.push_into(p, &mut out);
            }
            s.finish_into(&mut out);
            pieces.len()
        }));
    }
    report("엔진 — 산문만", &samples);
    println!(
        "\n**재사용 줄의 할당이 0 이 아니면 회귀다.** 그 불변식은 아래 테스트가 지킨다."
    );
}

/// 실제 엔진. **순위표가 아니라 우리 수치의 절대값과 추이를 본다.**
/// 다른 구현과 비교할 때도 이 수치는 맥락일 뿐이다 — 언어가 다르면 대부분 언어 차이를 재게 된다.
fn engine(doc: &str, pieces: &[&str], repeats: usize) {
    let input_bytes = doc.len();
    let mut out = String::with_capacity(input_bytes * 2);

    let mut samples = Vec::new();
    for (name, channel) in [
        ("새 Streamer · telegram-html", Channel::TelegramHtml),
        ("새 Streamer · slack-markdown", Channel::SlackMarkdown),
        ("새 Streamer · plain", Channel::Plain),
    ] {
        samples.push(measure(name, input_bytes, repeats, || {
            let mut s = Streamer::new(channel, CjkPolicy::Auto);
            out.clear();
            for p in pieces {
                s.push_into(p, &mut out);
            }
            s.finish_into(&mut out);
            pieces.len()
        }));
    }

    // 버퍼가 다 자란 뒤의 정상 상태. **회귀 감시는 이 줄을 본다** —
    // 위의 "새 Streamer" 는 첫 문서에서 버퍼를 키우는 비용이 섞여 있다.
    for (name, channel) in [
        ("재사용 · telegram-html", Channel::TelegramHtml),
        ("재사용 · slack-markdown", Channel::SlackMarkdown),
    ] {
        let mut s = Streamer::new(channel, CjkPolicy::Auto);
        out.clear();
        for p in pieces {
            s.push_into(p, &mut out);
        }
        s.finish_into(&mut out);
        samples.push(measure(name, input_bytes, repeats, || {
            out.clear();
            for p in pieces {
                s.push_into(p, &mut out);
            }
            s.finish_into(&mut out);
            pieces.len()
        }));
    }
    samples.push(measure("render(완성본) · telegram-html", input_bytes, repeats, || {
        let parts = mdwire::render(doc, Channel::TelegramHtml, CjkPolicy::Auto);
        std::hint::black_box(&parts);
        1
    }));

    report("엔진 — 실제 변환", &samples);
    println!(
        "\n이 문서의 할당은 대부분 표에서 나온다 — 셀 하나당 하나씩이고, 표는 열 너비를\n\
         재려고 어차피 끝까지 버퍼링한다. 스트리밍의 뜨거운 경로는 아래의 산문이다.\n\
         render 는 조각을 Vec<String> 으로 돌려주므로 조각 수만큼은 반드시 할당한다."
    );
}

/// 합성 문서. **실제 에이전트 문서는 이 저장소에 넣지 않는다**(공개 저장소이고 개인 문서다).
/// 대신 `DESIGN.md` 가 실측한 모양 — 80열 wrap, 줄 넘는 강조, 불릿, 표 — 을 흉내 낸다.
/// 실제 문서로 재려면 `--dir` 를 받는 하네스 쪽을 쓴다.
fn synthetic_doc() -> String {
    let unit = "\
# 배포 점검 결과

세 환경 중 **두 곳에서 동일한 증상**이 재현됐다. 원인은 캐시 계층이 아니라
**요청 경로에서 헤더를 지우는 미들웨어**였고, 이 미들웨어는 작년에 추가됐다.

- 스테이징: 재현됨, 응답 `200` 이지만 본문이 비어 있다
- 프로덕션: 재현됨, 같은 증상
- 로컬: 재현 안 됨 — 미들웨어가 꺼져 있다

| 환경 | 재현 | 응답 시간 |
|---|---|---|
| 스테이징 | 예 | 120ms |
| 프로덕션 | 예 | 340ms |
| 로컬 | 아니오 | 15ms |

> 조치는 미들웨어를 되돌리는 것이 아니라 **헤더 허용 목록을 명시**하는 쪽으로 간다.

```rust
fn allow(header: &str) -> bool {
    matches!(header, \"x-request-id\" | \"x-trace\")
}
```

";
    unit.repeat(8)
}

/// 표도 코드펜스도 없는 산문. 스트리밍의 뜨거운 경로다.
fn synthetic_prose() -> String {
    let unit = "\
세 환경 중 **두 곳에서 동일한 증상**이 재현됐다. 원인은 캐시 계층이 아니라
**요청 경로에서 헤더를 지우는 미들웨어**였고, 이 미들웨어는 작년에 추가됐다.

- 스테이징: 재현됨, 응답은 오지만 본문이 비어 있다
- 프로덕션: 재현됨, 같은 증상
- 로컬: 재현 안 됨 — 미들웨어가 꺼져 있다

> 조치는 미들웨어를 되돌리는 것이 아니라 **허용 목록을 명시**하는 쪽으로 간다.

";
    unit.repeat(12)
}

/// 문자 경계를 지키며 대략 `n` 바이트씩 자른다.
fn split_chunks(s: &str, n: usize) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < s.len() {
        let mut end = (start + n).min(s.len());
        while !s.is_char_boundary(end) {
            end += 1;
        }
        out.push(&s[start..end]);
        start = end;
    }
    out
}

#[cfg(test)]
mod gate {
    use super::*;

    /// 회귀 게이트. `SPEC.md` 9절이 "첫 벤치 수치를 보고 정한다"고 한 그 수치다.
    ///
    /// 버퍼가 다 자란 뒤 산문을 흘리면 **할당이 한 번도 일어나지 않아야 한다.**
    /// 이 수가 0 이 아니게 되는 변경은 스트리밍 경로에 할당을 들인 것이고,
    /// 그 경로는 조각마다 불린다.
    #[test]
    fn prose_streaming_is_allocation_free_once_warm() {
        let doc = synthetic_prose();
        let pieces = split_chunks(&doc, CHUNK);
        let mut s = Streamer::new(Channel::TelegramHtml, CjkPolicy::Auto);
        let mut out = String::new();

        for p in &pieces {
            s.push_into(p, &mut out);
        }
        s.finish_into(&mut out);

        let (_, counts) = rig::count(|| {
            out.clear();
            for p in &pieces {
                s.push_into(p, &mut out);
            }
            s.finish_into(&mut out);
        });
        assert_eq!(counts.allocs, 0, "산문 스트리밍이 할당한다 — 회귀다: {counts:?}");
    }
}
