//! 공개 API 의 행동. 코퍼스가 "실제로 겪은 고장"을 지킨다면, 여기는 규칙이 규칙대로
//! 도는지를 본다.

use mdwire::{render, Channel, CjkPolicy, Streamer};

fn one(input: &str, channel: Channel) -> String {
    let parts = render(input, channel, CjkPolicy::Auto);
    assert!(parts.len() <= 1, "이 입력은 나뉠 길이가 아니다: {parts:?}");
    parts.into_iter().next().unwrap_or_default()
}

fn tg(input: &str) -> String {
    one(input, Channel::TelegramHtml)
}

// ── 복구 ────────────────────────────────────────────────────────────────

#[test]
fn emphasis_spanning_a_line_break_is_one_range() {
    assert_eq!(
        tg("공개 채널**이다. 글 내용이 아니라\n**신분 공개 + 시점의 조합**이 판단 대상"),
        "공개 채널<b>이다. 글 내용이 아니라\n신분 공개 + 시점의 조합</b>이 판단 대상"
    );
}

#[test]
fn unclosed_emphasis_closes_at_block_end() {
    assert_eq!(tg("앞말 **굵은 꼬리"), "앞말 <b>굵은 꼬리</b>");
    // 블록을 넘지는 않는다. 다음 문단까지 굵어지면 뒤의 무관한 텍스트를 삼킨 것이다.
    assert_eq!(tg("**열고 끝\n\n다음 문단"), "<b>열고 끝</b>\n\n다음 문단");
}

#[test]
fn unclosed_code_fence_closes() {
    assert_eq!(tg("```\n코드 한 줄"), "<pre>코드 한 줄</pre>");
}

#[test]
fn lone_asterisks_stay_literal() {
    // `2 ** 3` 은 강조가 아니다. 추측으로 열었더라도 안 닫히면 되돌린다.
    assert_eq!(tg("2 ** 3 은 곱셈이 아니다"), "2 ** 3 은 곱셈이 아니다");
}

#[test]
fn underscores_inside_identifiers_survive() {
    assert_eq!(tg("plain_text_name 은 그대로다"), "plain_text_name 은 그대로다");
}

#[test]
fn markers_inside_code_are_text() {
    assert_eq!(tg("`**굵지 않다**` 그리고 밖"), "<code>**굵지 않다**</code> 그리고 밖");
}

/// **`**` 가 태그 채널의 출력에 남으면 그건 실패다.** 정상이라면 전부 변환됐어야 한다.
/// 이 고장이 몇 달을 간 이유가 "틀려도 채널이 200 을 준다" 였다.
#[test]
fn no_emphasis_marker_survives_into_tag_channels() {
    for input in [
        "공개 채널**이다. 글 내용이 아니라\n**신분 공개 + 시점의 조합**이 판단 대상",
        "**열기만** 하고 **닫지 않으면",
        "- **항목** 하나\n- 항목 **둘",
        "> 인용 안의 **강조**도",
        "# 헤딩의 **강조**",
    ] {
        for channel in [Channel::TelegramHtml, Channel::Plain] {
            let out = one(input, channel);
            assert!(!out.contains("**"), "{}: {out}", channel.name());
        }
    }
}

#[test]
fn a_run_of_three_markers_leaves_nothing_behind() {
    // `***x***` 에서 두 개만 집으면 별표 하나가 출력에 남는다. 남은 마커는 실패다.
    assert_eq!(tg("***세 개 별표***"), "<b>세 개 별표</b>");
    assert_eq!(tg("***여는 쪽만 셋**"), "<b>여는 쪽만 셋</b>");
    assert_eq!(tg("**닫는 쪽만 셋***"), "<b>닫는 쪽만 셋</b>");
}

#[test]
fn crlf_input_does_not_leak_carriage_returns() {
    let out = tg("윈도우 줄바꿈\r\n**굵게**\r\n");
    assert!(!out.contains('\r'), "{out:?}");
    assert_eq!(out, "윈도우 줄바꿈\n<b>굵게</b>");
}

/// `SPEC.md` 6절 — 각색은 하지 않는다. 체크박스를 ✅/⬜ 로 바꾸지 않는다.
#[test]
fn checkboxes_are_not_dressed_up() {
    assert_eq!(tg("- [ ] 안 한 일\n- [x] 한 일"), "• [ ] 안 한 일\n• [x] 한 일");
}

// ── 블록 매핑 (SPEC 8절) ────────────────────────────────────────────────

#[test]
fn headings_map_per_channel() {
    assert_eq!(tg("## 제목"), "<b>제목</b>");
    assert_eq!(one("## 제목", Channel::SlackMarkdown), "## 제목");
    assert_eq!(one("## 제목", Channel::Plain), "제목");
}

#[test]
fn heading_levels_do_not_jump() {
    // 두 단계 이상 깊어지면 한 단계로 당긴다.
    assert_eq!(
        one("# 하나\n\n#### 넷", Channel::SlackMarkdown),
        "# 하나\n\n## 넷"
    );
}

#[test]
fn list_markers_are_unified() {
    assert_eq!(one("* 하나\n+ 둘\n- 셋", Channel::SlackMarkdown), "- 하나\n- 둘\n- 셋");
    assert_eq!(tg("* 하나\n+ 둘"), "• 하나\n• 둘");
    assert_eq!(one("3. 셋\n4. 넷", Channel::Plain), "3. 셋\n4. 넷");
}

#[test]
fn quotes_wrap_once_not_per_line() {
    assert_eq!(tg("> 한 줄\n> 두 줄"), "<blockquote>한 줄\n두 줄</blockquote>");
    assert_eq!(one("> 한 줄\n> 두 줄", Channel::Plain), "> 한 줄\n> 두 줄");
}

#[test]
fn fence_info_becomes_a_language_class() {
    assert_eq!(
        tg("```rust\nlet a = 1 < 2;\n```"),
        "<pre><code class=\"language-rust\">let a = 1 &lt; 2;</code></pre>"
    );
}

#[test]
fn links_render_per_channel() {
    let input = "[문서](https://example.com/a?b=1&c=2) 참고";
    assert_eq!(tg(input), "<a href=\"https://example.com/a?b=1&amp;c=2\">문서</a> 참고");
    assert_eq!(one(input, Channel::SlackMarkdown), input);
    assert_eq!(one(input, Channel::Plain), "문서 (https://example.com/a?b=1&c=2) 참고");
}

#[test]
fn html_special_characters_are_escaped() {
    assert_eq!(tg("a < b & c > d"), "a &lt; b &amp; c &gt; d");
}

// ── 정돈 ────────────────────────────────────────────────────────────────

#[test]
fn extra_blank_lines_collapse_to_one() {
    assert_eq!(tg("첫 문단\n\n\n\n\n둘째 문단"), "첫 문단\n\n둘째 문단");
}

#[test]
fn leading_and_trailing_blank_lines_go_away() {
    assert_eq!(tg("\n\n본문\n\n\n"), "본문");
}

// ── CJK 정책 ────────────────────────────────────────────────────────────

#[test]
fn cjk_policy_is_per_channel_with_override() {
    let input = "채널**이다";
    // 슬랙은 마크다운을 채널 파서가 다시 읽는다 — 끼운다.
    assert_eq!(
        render(input, Channel::SlackMarkdown, CjkPolicy::Auto)[0],
        "채널\u{200b}**이다**"
    );
    // 텔레그램 HTML 은 태그로 나가니 끼울 이유가 없다.
    assert_eq!(render(input, Channel::TelegramHtml, CjkPolicy::Auto)[0], "채널<b>이다</b>");
    // 오버라이드가 먹는다. **이게 기존 변환기에 없던 것이다.**
    assert_eq!(render(input, Channel::SlackMarkdown, CjkPolicy::Never)[0], "채널**이다**");
    assert_eq!(
        render(input, Channel::TelegramHtml, CjkPolicy::AlwaysPad)[0],
        "채널\u{200b}<b>이다</b>"
    );
    // 영문 옆에는 끼우지 않는다. 필요가 없고, 넣으면 복사할 때 딸려간다.
    assert_eq!(render("ab**cd", Channel::SlackMarkdown, CjkPolicy::Auto)[0], "ab**cd**");
}

// ── 분할 ────────────────────────────────────────────────────────────────

#[test]
fn parts_stay_within_the_limit_and_keep_markup_whole() {
    let block = "**굵은 문단**이 하나 있고 `코드`도 있다. 이 문단은 길이 한도를 넘기려고 반복된다.\n\n";
    let input = block.repeat(80);
    let parts = render(&input, Channel::TelegramHtml, CjkPolicy::Auto);
    assert!(parts.len() > 1, "한도를 넘겼는데 안 나뉘었다");
    for part in &parts {
        assert!(part.chars().count() <= Channel::TelegramHtml.limit(), "조각이 한도를 넘는다");
        assert!(!part.contains("**"), "마커가 남았다");
        assert_eq!(part.matches("<b>").count(), part.matches("</b>").count(), "태그 짝이 안 맞는다");
        assert_eq!(part.matches("<code>").count(), part.matches("</code>").count());
        assert!(!part.starts_with('\n') && !part.ends_with('\n'), "조각 끝에 빈 줄이 남았다");
    }
}

#[test]
fn a_single_oversized_block_is_cut_with_tags_closed_and_reopened() {
    // 한 블록이 통째로 한도를 넘는 경우. 자르는 자리에서 닫고 다음 조각에서 다시 연다.
    let input = format!("> **{}**", "한글 ".repeat(3000));
    let parts = render(&input, Channel::TelegramHtml, CjkPolicy::Auto);
    assert!(parts.len() > 1);
    for part in &parts {
        assert!(part.chars().count() <= Channel::TelegramHtml.limit());
        assert_eq!(part.matches("<blockquote>").count(), part.matches("</blockquote>").count());
        assert_eq!(part.matches("<b>").count(), part.matches("</b>").count());
    }
}

// ── 스트리밍 ────────────────────────────────────────────────────────────

#[test]
fn chunk_boundary_inside_markup_does_not_leak() {
    let mut s = Streamer::new(Channel::TelegramHtml, CjkPolicy::Auto);
    let mut out = String::new();
    out.push_str(s.push("앞말 **굵"));
    // 강조가 아직 안 닫혔다. 여는 태그도 내용도 나가면 안 된다 —
    // 나가 버리면 뒤에 오는 무관한 텍스트를 삼킨다.
    assert_eq!(out, "앞말 ");
    out.push_str(s.push("게** 뒷말"));
    out.push_str(s.finish());
    assert_eq!(out, "앞말 <b>굵게</b> 뒷말");
}

#[test]
fn streaming_never_emits_half_a_tag() {
    let input = "# 제목\n\n**굵게** 있는 문단과 `코드` 그리고\n이어지는 **줄 넘는 강조**다.\n";
    for size in 1..=12 {
        let mut s = Streamer::new(Channel::TelegramHtml, CjkPolicy::Auto);
        let mut seen = String::new();
        for chunk in input.as_bytes().chunks(size) {
            // 바이트로 자르면 UTF-8 이 깨지므로 문자 단위로 다시 맞춘다.
            let _ = chunk;
        }
        let mut buf = String::new();
        for c in input.chars() {
            buf.push(c);
            if buf.chars().count() == size {
                let piece = s.push(&buf);
                assert!(!piece.contains('<') || piece.matches('<').count() == piece.matches('>').count());
                seen.push_str(piece);
                buf.clear();
            }
        }
        seen.push_str(s.push(&buf));
        seen.push_str(s.finish());
        assert_eq!(seen, render(input, Channel::TelegramHtml, CjkPolicy::Auto).join(""));
    }
}

#[test]
fn empty_input_produces_nothing() {
    assert!(render("", Channel::TelegramHtml, CjkPolicy::Auto).is_empty());
    assert!(render("   \n\n  \n", Channel::TelegramHtml, CjkPolicy::Auto).is_empty());
}
