//! 계측 리그 — 할당 횟수와 시간을 센다.
//!
//! `SPEC.md` 8절: **벤치를 먼저 세우고 API 를 확정한다.** 그래서 이 모듈이 먼저 선다.
//! 재는 것은 둘이다.
//!
//! - **조각당 할당 횟수** — 스트리밍 경로는 조각마다 불린다. 여기가 비면 서명이 잘못됐다.
//! - **처리량** — 회귀 감시용. 순위표가 아니라 우리 수치의 추이를 본다.
//!
//! 계수기는 **스레드 로컬**이다. 전역 원자 카운터로 두면 테스트가 병렬로 돌 때
//! 다른 스레드의 할당이 섞여 들어와 수치가 무의미해진다.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::time::{Duration, Instant};

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

/// 시스템 할당기를 감싸 호출 횟수를 센다. `#[global_allocator]` 로 꽂아 쓴다.
pub struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump(layout.size());
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump(layout.size());
        System.alloc_zeroed(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // 버퍼가 자라는 것도 할당이다. `String::push_str` 의 재할당이 여기 잡힌다 —
        // 조각마다 버퍼를 새로 만드는 서명과 재사용하는 서명의 차이가 이 숫자로 갈린다.
        bump(new_size.saturating_sub(layout.size()));
        System.realloc(ptr, layout, new_size)
    }
}

fn bump(bytes: usize) {
    // TLS 파괴 이후에도 할당은 일어날 수 있다. 그때는 그냥 세지 않는다.
    let _ = ALLOCS.try_with(|c| c.set(c.get().wrapping_add(1)));
    let _ = BYTES.try_with(|c| c.set(c.get().wrapping_add(bytes)));
}

/// 한 구간에서 일어난 할당.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// `alloc`/`realloc` 호출 횟수.
    pub allocs: usize,
    /// 새로 요청된 바이트 수(realloc 은 증가분만).
    pub bytes: usize,
}

/// `f` 를 돌리는 동안의 할당을 센다. 반환값은 호출자가 붙들어 둔다 —
/// 구간 안에서 drop 되면 그 해제까지 함께 재게 된다.
pub fn count<T>(f: impl FnOnce() -> T) -> (T, Counts) {
    let (a0, b0) = (ALLOCS.get(), BYTES.get());
    let out = black_box(f());
    let counts = Counts {
        allocs: ALLOCS.get().wrapping_sub(a0),
        bytes: BYTES.get().wrapping_sub(b0),
    };
    (out, counts)
}

/// 한 후보에 대한 측정 결과.
#[derive(Debug, Clone)]
pub struct Sample {
    pub name: &'static str,
    /// 1회(= 문서 하나를 끝까지 처리) 소요 시간. 여러 번 중 최소값 — 잡음은 위로만 붙는다.
    pub per_run: Duration,
    /// 1회당 할당 횟수.
    pub allocs: usize,
    /// 1회당 할당 바이트.
    pub bytes: usize,
    /// 1회가 처리한 입력 바이트. 처리량 계산에 쓴다.
    pub input_bytes: usize,
    /// 1회가 밀어 넣은 조각 수. 조각당 할당을 내려면 필요하다.
    pub chunks: usize,
}

impl Sample {
    /// 조각당 할당 횟수. 스트리밍 서명을 고르는 기준이 되는 수치다.
    pub fn allocs_per_chunk(&self) -> f64 {
        if self.chunks == 0 {
            return 0.0;
        }
        self.allocs as f64 / self.chunks as f64
    }

    /// MiB/s.
    pub fn throughput_mib(&self) -> f64 {
        let secs = self.per_run.as_secs_f64();
        if secs <= 0.0 {
            return f64::INFINITY;
        }
        (self.input_bytes as f64 / (1024.0 * 1024.0)) / secs
    }
}

/// 후보 하나를 측정한다.
///
/// `run` 은 "문서 하나를 끝까지 처리"하고 처리한 조각 수를 돌려준다.
/// 시간은 `repeats` 회 중 최소값을, 할당은 워밍업 뒤 한 번을 쓴다 —
/// 할당 횟수는 결정적이라 평균낼 이유가 없다.
pub fn measure(name: &'static str, input_bytes: usize, repeats: usize, mut run: impl FnMut() -> usize) -> Sample {
    // 워밍업. 첫 회는 버퍼가 비어 있어 재할당이 더 잡힌다.
    let mut chunks = black_box(run());

    let (c, counts) = count(&mut run);
    chunks = chunks.max(c);

    let mut per_run = Duration::MAX;
    for _ in 0..repeats.max(1) {
        let start = Instant::now();
        black_box(run());
        per_run = per_run.min(start.elapsed());
    }

    Sample {
        name,
        per_run,
        allocs: counts.allocs,
        bytes: counts.bytes,
        input_bytes,
        chunks,
    }
}

/// 측정 결과를 표로 찍는다. 열 맞춤은 ASCII 라 문자 수로 충분하다 —
/// 표시 폭 문제는 라이브러리 안쪽(`mdwire::width`)의 일이다.
pub fn report(title: &str, samples: &[Sample]) {
    println!("\n== {title} ==");
    println!(
        "{:<28} {:>12} {:>12} {:>14} {:>12}",
        "후보", "할당/문서", "할당/조각", "바이트/문서", "MiB/s"
    );
    for s in samples {
        println!(
            "{:<28} {:>12} {:>12.2} {:>14} {:>12.1}",
            s.name,
            s.allocs,
            s.allocs_per_chunk(),
            s.bytes,
            s.throughput_mib()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 리그 자체의 검증. **계수기가 맞다는 근거가 없으면 이후 수치는 전부 무의미하다.**
    /// 계수기가 스레드 로컬이라 테스트가 병렬로 돌아도 구간이 섞이지 않는다.
    #[test]
    fn rig_counts_what_it_claims_to_count() {
        // 정확히 N 번 할당하는 코드: 미리 용량을 잡은 Vec 에 String 을 N 개 담는다.
        let n = 32;
        let (v, counts) = count(|| {
            let mut v: Vec<String> = Vec::with_capacity(n);
            for i in 0..n {
                v.push(i.to_string());
            }
            v
        });
        assert_eq!(v.len(), n);
        // Vec 하나 + String N 개. 재할당이 없으니 정확히 N+1 이다.
        assert_eq!(counts.allocs, n + 1, "할당 횟수가 예상과 다르다: {counts:?}");
        assert!(counts.bytes >= n, "바이트 계수가 0 이면 realloc/alloc 훅이 빠진 것이다");

        // 아무것도 할당하지 않는 구간은 0 이어야 한다. 0 이 안 나오면
        // 계측 구간 자체가 할당하고 있다는 뜻이고, 그러면 작은 수치를 못 읽는다.
        let (sum, counts) = count(|| (0..100u64).sum::<u64>());
        assert_eq!(sum, 4950);
        assert_eq!(counts.allocs, 0, "계측 구간이 스스로 할당한다: {counts:?}");
    }
}
