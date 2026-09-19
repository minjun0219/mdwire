//! 표시 폭 — 고정폭 글꼴에서 한 글자가 차지하는 칸 수.
//!
//! **표의 열은 문자 수가 아니라 표시 폭으로 맞춘다**(`SPEC.md` 7절). 한글·한자·가나·
//! 전각기호·이모지는 두 칸을 차지한다. `chars().count()` 로 맞추면 한글이 든 표는
//! 반드시 어긋난다 — 절반쯤 어긋나는 것이 아니라, 열 하나에 한글 한 글자가 들어갈 때마다
//! 한 칸씩 밀린다.
//!
//! 유니코드 East Asian Width 의 `W`·`F` 를 폭 2 로, 결합 문자와 폭 없는 제어 문자를
//! 폭 0 으로 본다. 코어에 의존성을 두지 않으므로 구간표를 직접 들고 있다
//! (`AGENTS.md`). 전각 구간은 좁아서 표로 박아도 유지가 된다.

/// 폭 0 — 결합 문자, 폭 없는 공백, 변형 선택자.
const ZERO: &[(u32, u32)] = &[
    (0x0300, 0x036F), // 결합 발음 기호
    (0x0483, 0x0489),
    (0x0591, 0x05BD),
    (0x0610, 0x061A),
    (0x064B, 0x065F),
    (0x0670, 0x0670),
    (0x06D6, 0x06DC),
    (0x0E31, 0x0E31),
    (0x0E34, 0x0E3A),
    (0x0E47, 0x0E4E),
    (0x1160, 0x11FF), // 한글 중성·종성 자모(조합용). 초성에 붙어 폭을 더하지 않는다
    (0x135D, 0x135F),
    (0x1AB0, 0x1AFF),
    (0x1DC0, 0x1DFF),
    (0x200B, 0x200F), // ZWSP·ZWNJ·ZWJ·방향 표시. CJK 정책이 끼우는 U+200B 가 여기다
    (0x2028, 0x202E),
    (0x2060, 0x2064),
    (0x20D0, 0x20F0),
    (0xFE00, 0xFE0F), // 변형 선택자
    (0xFE20, 0xFE2F),
    (0xFEFF, 0xFEFF),
    (0x1D165, 0x1D169),
    (0x1D16D, 0x1D172),
    (0xE0100, 0xE01EF),
];

/// 폭 2 — East Asian Width 가 Wide 또는 Fullwidth 인 구간.
const WIDE: &[(u32, u32)] = &[
    (0x1100, 0x115F), // 한글 초성 자모
    (0x231A, 0x231B),
    (0x2329, 0x232A),
    (0x23E9, 0x23EC),
    (0x23F0, 0x23F0),
    (0x23F3, 0x23F3),
    (0x25FD, 0x25FE),
    (0x2614, 0x2615),
    (0x2648, 0x2653),
    (0x267F, 0x267F),
    (0x2693, 0x2693),
    (0x26A1, 0x26A1),
    (0x26AA, 0x26AB),
    (0x26BD, 0x26BE),
    (0x26C4, 0x26C5),
    (0x26CE, 0x26CE),
    (0x26D4, 0x26D4),
    (0x26EA, 0x26EA),
    (0x26F2, 0x26F3),
    (0x26F5, 0x26F5),
    (0x26FA, 0x26FA),
    (0x26FD, 0x26FD),
    (0x2705, 0x2705),
    (0x270A, 0x270B),
    (0x2728, 0x2728),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2795, 0x2797),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x2E80, 0x2E99),
    (0x2E9B, 0x2EF3),
    (0x2F00, 0x2FD5),
    (0x2FF0, 0x2FFB),
    (0x3000, 0x303E), // 전각 공백 · CJK 구두점
    (0x3041, 0x3096), // 히라가나
    (0x3099, 0x30FF), // 가타카나
    (0x3105, 0x312F),
    (0x3131, 0x318E), // 한글 호환 자모
    (0x3190, 0x31E3),
    (0x31F0, 0x321E),
    (0x3220, 0x3247),
    (0x3250, 0x4DBF),
    (0x4E00, 0xA48C), // 한중일 통합 한자
    (0xA490, 0xA4C6),
    (0xA960, 0xA97C),
    (0xAC00, 0xD7A3), // 한글 음절 — 한국어 표의 거의 전부가 여기다
    (0xF900, 0xFAFF),
    (0xFE10, 0xFE19),
    (0xFE30, 0xFE52),
    (0xFE54, 0xFE66),
    (0xFE68, 0xFE6B),
    (0xFF01, 0xFF60), // 전각 영숫자·기호
    (0xFFE0, 0xFFE6),
    (0x16FE0, 0x16FE4),
    (0x17000, 0x18CD5),
    (0x1B000, 0x1B152),
    (0x1B164, 0x1B167),
    (0x1B170, 0x1B2FB),
    (0x1F004, 0x1F004),
    (0x1F0CF, 0x1F0CF),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F200, 0x1F320),
    (0x1F32D, 0x1F335),
    (0x1F337, 0x1F37C),
    (0x1F37E, 0x1F393),
    (0x1F3A0, 0x1F3CA),
    (0x1F3CF, 0x1F3D3),
    (0x1F3E0, 0x1F3F0),
    (0x1F3F4, 0x1F3F4),
    (0x1F3F8, 0x1F43E),
    (0x1F440, 0x1F440),
    (0x1F442, 0x1F4FC),
    (0x1F4FF, 0x1F53D),
    (0x1F54B, 0x1F54E),
    (0x1F550, 0x1F567),
    (0x1F57A, 0x1F57A),
    (0x1F595, 0x1F596),
    (0x1F5A4, 0x1F5A4),
    (0x1F5FB, 0x1F64F),
    (0x1F680, 0x1F6C5),
    (0x1F6CC, 0x1F6CC),
    (0x1F6D0, 0x1F6D2),
    (0x1F6D5, 0x1F6D7),
    (0x1F6EB, 0x1F6EC),
    (0x1F6F4, 0x1F6FC),
    (0x1F7E0, 0x1F7EB),
    (0x1F90C, 0x1F93A),
    (0x1F93C, 0x1F945),
    (0x1F947, 0x1F978),
    (0x1F97A, 0x1F9CB),
    (0x1F9CD, 0x1F9FF),
    (0x1FA70, 0x1FA74),
    (0x1FA78, 0x1FA7A),
    (0x1FA80, 0x1FA86),
    (0x1FA90, 0x1FAA8),
    (0x1FAB0, 0x1FAB6),
    (0x1FAC0, 0x1FAC2),
    (0x1FAD0, 0x1FAD6),
    (0x20000, 0x2FFFD),
    (0x30000, 0x3FFFD),
];

/// 한 글자의 표시 폭. 0, 1, 2 중 하나.
pub fn char_width(c: char) -> usize {
    let cp = c as u32;
    if cp == 0 {
        return 0;
    }
    if cp < 0x20 || (0x7F..0xA0).contains(&cp) {
        // 제어 문자. 표에 들어오면 폭 계산이 무너지므로 0 으로 센다.
        return 0;
    }
    if cp < 0x300 {
        // 라틴 상용 구간. 표 대부분이 여기라 먼저 끊는다.
        return 1;
    }
    if in_ranges(ZERO, cp) {
        return 0;
    }
    if in_ranges(WIDE, cp) {
        return 2;
    }
    1
}

/// CJK 인접 강조 정책이 보는 "CJK 문자"인가.
///
/// **표시 폭과는 다른 질문이다.** 둘을 한 함수로 쓰면 두 군데가 틀린다 —
/// 반각 가타카나(`ｱｲｳ`)는 폭이 1이지만 CJK 라 패딩이 필요하고, 이모지는 폭이 2지만
/// CJK 가 아니라 패딩이 필요 없다.
///
/// 판정 기준은 CommonMark 의 CJK-friendly 개정안을 따른다 — East Asian Width 가
/// `W`·`F`·`H` 이면서 이모지 표현이 아니거나, 스크립트가 Hangul 이면 CJK 다.
/// (<https://github.com/tats-u/markdown-cjk-friendly>)
pub fn is_cjk(c: char) -> bool {
    in_ranges(CJK, c as u32)
}

/// CJK 구간. 한자·가나·한글(조합형 자모 포함)·전각/반각 CJK 기호.
/// **이모지와 기호는 일부러 뺐다** — 폭이 2여도 CJK 가 아니다.
const CJK: &[(u32, u32)] = &[
    (0x1100, 0x11FF), // 한글 자모(조합형). NFD 로 분해된 한글이 여기다
    (0x2E80, 0x2EF3), // CJK 부수
    (0x2F00, 0x2FD5), // 강희 부수
    (0x3000, 0x303F), // CJK 구두점 — `。` `、` `「」` 가 여기다
    (0x3041, 0x30FF), // 히라가나 · 가타카나
    (0x3105, 0x312F),
    (0x3131, 0x318E), // 한글 호환 자모
    (0x3190, 0x31E3),
    (0x31F0, 0x321E),
    (0x3220, 0x3247),
    (0x3250, 0x4DBF),
    (0x4E00, 0x9FFF), // 한중일 통합 한자
    (0xA960, 0xA97C), // 한글 자모 확장 A
    (0xAC00, 0xD7A3), // 한글 음절
    (0xD7B0, 0xD7FB), // 한글 자모 확장 B
    (0xF900, 0xFAFF), // 호환 한자
    (0xFE10, 0xFE19),
    (0xFE30, 0xFE6B), // 세로쓰기 형태 · 전각 기호
    (0xFF01, 0xFF60), // 전각 영숫자·기호 — `（）` `，` 가 여기다
    (0xFF61, 0xFFDC), // **반각** 가타카나·한글. 폭은 1이지만 CJK 다
    (0x20000, 0x2FFFD),
    (0x30000, 0x3FFFD),
];

/// 문자열의 표시 폭.
///
/// 이모지 결합 연쇄(ZWJ 로 이어진 가족 이모지 등)는 구성 요소를 각각 세므로
/// 실제 표시보다 넓게 나올 수 있다. 표를 어긋나게 만드는 쪽이 아니라 여유를 주는
/// 방향이라 v0.1 은 이대로 둔다.
pub fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn in_ranges(table: &[(u32, u32)], cp: u32) -> bool {
    table
        .binary_search_by(|&(lo, hi)| {
            if cp < lo {
                std::cmp::Ordering::Greater
            } else if cp > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted_and_disjoint() {
        // 이진 탐색의 전제. 깨지면 조용히 틀린 폭이 나온다.
        for table in [ZERO, WIDE] {
            for w in table.windows(2) {
                assert!(w[0].1 < w[1].0, "구간이 겹치거나 순서가 틀렸다: {:?}", w);
            }
            for &(lo, hi) in table {
                assert!(lo <= hi);
            }
        }
    }

    #[test]
    fn known_widths() {
        for (c, w) in [
            ('a', 1),
            ('9', 1),
            (' ', 1),
            ('|', 1),
            ('가', 2),
            ('힣', 2),
            ('漢', 2),
            ('あ', 2),
            ('ア', 2),
            ('，', 2),
            ('　', 2), // 전각 공백
            ('Ａ', 2), // 전각 영문
            ('✅', 2),
            ('🚀', 2),
            ('\u{200b}', 0), // ZWSP — CJK 정책이 끼우는 그것
            ('\u{0301}', 0), // 결합 악센트
            ('\n', 0),
        ] {
            assert_eq!(char_width(c), w, "{c:?} 의 폭이 {w} 가 아니다");
        }
    }

    #[test]
    fn korean_text_is_twice_its_char_count() {
        let s = "한글";
        assert_eq!(s.chars().count(), 2);
        assert_eq!(str_width(s), 4, "문자 수로 맞추면 표가 어긋난다");
    }

    #[test]
    fn cjk_is_not_the_same_question_as_width() {
        // 폭 1인데 CJK — 반각 가타카나. 패딩이 필요하다
        assert_eq!(char_width('ｱ'), 1);
        assert!(is_cjk('ｱ'));
        // 폭 2인데 CJK 아님 — 이모지. 패딩이 필요 없다
        assert_eq!(char_width('🚀'), 2);
        assert!(!is_cjk('🚀'));
        assert!(!is_cjk('✅'));
        // 조합형 한글(NFD). 폭 0인 중성·종성도 CJK 다
        assert!(is_cjk('\u{1100}') && is_cjk('\u{1161}'));
        // CJK 구두점 — 개정안이 다루는 바로 그 글자들
        for c in ['。', '、', '，', '「', '」', '（', '）'] {
            assert!(is_cjk(c), "{c} 가 CJK 로 안 잡힌다");
        }
        // 라틴은 아니다
        for c in ['a', '1', ' ', '.', '-'] {
            assert!(!is_cjk(c), "{c} 가 CJK 로 잡힌다");
        }
    }

    #[test]
    fn cjk_table_is_sorted_and_disjoint() {
        for w in CJK.windows(2) {
            assert!(w[0].1 < w[1].0, "구간이 겹치거나 순서가 틀렸다: {:?}", w);
        }
    }

    #[test]
    fn mixed_width_adds_up() {
        assert_eq!(str_width("환경 env"), 2 + 2 + 1 + 3);
    }
}
