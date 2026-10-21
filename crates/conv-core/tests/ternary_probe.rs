//! 三态场编码对比探针（ignored）：u8 vs 2-bit vs 5-trit/byte。
//! 同一份 256³ 封闭壳 + 边界泛洪，比较内存与遍历时间；验证三进制是否有性价比。

use std::time::Instant;

const N: usize = 256;
const COUNT: usize = N * N * N;

fn base_state() -> Vec<u8> {
    // 0 = unknown, 1 = barrier, 2 = outside（泛洪写入）
    let c = (N / 2) as f32;
    let r_in = 100.0f32;
    let r_out = 103.0f32;
    let mut s = vec![0u8; COUNT];
    for z in 0..N {
        for y in 0..N {
            let dz = z as f32 - c;
            let dy = y as f32 - c;
            let row = (y + z * N) * N;
            for x in 0..N {
                let dx = x as f32 - c;
                let r = (dx * dx + dy * dy + dz * dz).sqrt();
                if (r_in..=r_out).contains(&r) {
                    s[row + x] = 1;
                }
            }
        }
    }
    s
}

fn flood_u8(base: &[u8]) -> (Vec<u8>, usize) {
    let mut s = base.to_vec();
    let mut frontier: Vec<u32> = Vec::new();
    let push = |i: usize, s: &mut [u8], f: &mut Vec<u32>| {
        if s[i] == 0 {
            s[i] = 2;
            f.push(i as u32);
        }
    };
    for z in 0..N {
        for y in 0..N {
            let row = (y + z * N) * N;
            for x in 0..N {
                if x == 0 || y == 0 || z == 0 || x + 1 == N || y + 1 == N || z + 1 == N {
                    push(row + x, &mut s, &mut frontier);
                }
            }
        }
    }
    let mut visited = frontier.len();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &i in &frontier {
            let i = i as usize;
            let x = i % N;
            let y = (i / N) % N;
            let z = i / (N * N);
            if x > 0 {
                push(i - 1, &mut s, &mut next);
            }
            if x + 1 < N {
                push(i + 1, &mut s, &mut next);
            }
            if y > 0 {
                push(i - N, &mut s, &mut next);
            }
            if y + 1 < N {
                push(i + N, &mut s, &mut next);
            }
            if z > 0 {
                push(i - N * N, &mut s, &mut next);
            }
            if z + 1 < N {
                push(i + N * N, &mut s, &mut next);
            }
        }
        visited += next.len();
        frontier = next;
    }
    (s, visited)
}

#[derive(Clone)]
struct Packed2 {
    data: Vec<u8>,
}

impl Packed2 {
    fn from_u8(base: &[u8]) -> Self {
        let mut data = vec![0u8; COUNT.div_ceil(4)];
        for (i, &v) in base.iter().enumerate() {
            data[i / 4] |= (v & 3) << ((i % 4) * 2);
        }
        Self { data }
    }
    fn get(&self, i: usize) -> u8 {
        (self.data[i / 4] >> ((i % 4) * 2)) & 3
    }
    fn set(&mut self, i: usize, v: u8) {
        let sh = (i % 4) * 2;
        self.data[i / 4] = (self.data[i / 4] & !(3 << sh)) | ((v & 3) << sh);
    }
    fn to_u8(&self) -> Vec<u8> {
        (0..COUNT).map(|i| self.get(i)).collect()
    }
}

#[derive(Clone)]
struct PackedTrit {
    data: Vec<u8>,
    pow: [u8; 5],
}

impl PackedTrit {
    fn from_u8(base: &[u8]) -> Self {
        let mut data = vec![0u8; COUNT.div_ceil(5)];
        let pow = [1u8, 3, 9, 27, 81];
        for (i, &v) in base.iter().enumerate() {
            let (b, d) = (i / 5, i % 5);
            data[b] += (v % 3) * pow[d];
        }
        Self { data, pow }
    }
    fn get(&self, i: usize) -> u8 {
        let mut v = self.data[i / 5];
        let d = i % 5;
        for _ in 0..d {
            v /= 3;
        }
        v % 3
    }
    fn set(&mut self, i: usize, v: u8) {
        let b = i / 5;
        let d = i % 5;
        let old = self.get(i);
        self.data[b] = self.data[b] - old * self.pow[d] + (v % 3) * self.pow[d];
    }
    fn to_u8(&self) -> Vec<u8> {
        (0..COUNT).map(|i| self.get(i)).collect()
    }
}

fn flood_p2(base: &Packed2) -> (Packed2, usize) {
    let mut s = base.clone();
    let mut frontier: Vec<u32> = Vec::new();
    let push = |i: usize, s: &mut Packed2, f: &mut Vec<u32>| {
        if s.get(i) == 0 {
            s.set(i, 2);
            f.push(i as u32);
        }
    };
    for z in 0..N {
        for y in 0..N {
            let row = (y + z * N) * N;
            for x in 0..N {
                if x == 0 || y == 0 || z == 0 || x + 1 == N || y + 1 == N || z + 1 == N {
                    push(row + x, &mut s, &mut frontier);
                }
            }
        }
    }
    let mut visited = frontier.len();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &i in &frontier {
            let i = i as usize;
            let x = i % N;
            let y = (i / N) % N;
            let z = i / (N * N);
            if x > 0 {
                push(i - 1, &mut s, &mut next);
            }
            if x + 1 < N {
                push(i + 1, &mut s, &mut next);
            }
            if y > 0 {
                push(i - N, &mut s, &mut next);
            }
            if y + 1 < N {
                push(i + N, &mut s, &mut next);
            }
            if z > 0 {
                push(i - N * N, &mut s, &mut next);
            }
            if z + 1 < N {
                push(i + N * N, &mut s, &mut next);
            }
        }
        visited += next.len();
        frontier = next;
    }
    (s, visited)
}

fn flood_trit(base: &PackedTrit) -> (PackedTrit, usize) {
    let mut s = base.clone();
    let mut frontier: Vec<u32> = Vec::new();
    let push = |i: usize, s: &mut PackedTrit, f: &mut Vec<u32>| {
        if s.get(i) == 0 {
            s.set(i, 2);
            f.push(i as u32);
        }
    };
    for z in 0..N {
        for y in 0..N {
            let row = (y + z * N) * N;
            for x in 0..N {
                if x == 0 || y == 0 || z == 0 || x + 1 == N || y + 1 == N || z + 1 == N {
                    push(row + x, &mut s, &mut frontier);
                }
            }
        }
    }
    let mut visited = frontier.len();
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for &i in &frontier {
            let i = i as usize;
            let x = i % N;
            let y = (i / N) % N;
            let z = i / (N * N);
            if x > 0 {
                push(i - 1, &mut s, &mut next);
            }
            if x + 1 < N {
                push(i + 1, &mut s, &mut next);
            }
            if y > 0 {
                push(i - N, &mut s, &mut next);
            }
            if y + 1 < N {
                push(i + N, &mut s, &mut next);
            }
            if z > 0 {
                push(i - N * N, &mut s, &mut next);
            }
            if z + 1 < N {
                push(i + N * N, &mut s, &mut next);
            }
        }
        visited += next.len();
        frontier = next;
    }
    (s, visited)
}

#[test]
#[ignore = "diagnostic tri-state encoding cost 256^3"]
fn tri_state_encoding_probe_256() {
    let base = base_state();
    let t = Instant::now();
    let (u8s, u8v) = flood_u8(&base);
    let u8_ms = t.elapsed().as_secs_f64() * 1e3;

    let p2 = Packed2::from_u8(&base);
    let t = Instant::now();
    let (p2s, p2v) = flood_p2(&p2);
    let p2_ms = t.elapsed().as_secs_f64() * 1e3;

    let tr = PackedTrit::from_u8(&base);
    let t = Instant::now();
    let (trs, trv) = flood_trit(&tr);
    let tr_ms = t.elapsed().as_secs_f64() * 1e3;

    assert_eq!(u8v, p2v);
    assert_eq!(u8v, trv);
    assert_eq!(u8s, p2s.to_u8());
    assert_eq!(u8s, trs.to_u8());
    let u8_mb = COUNT as f64 / 1e6;
    let p2_mb = COUNT.div_ceil(4) as f64 / 1e6;
    let tr_mb = COUNT.div_ceil(5) as f64 / 1e6;
    println!(
        "cells={COUNT} visited={u8v}; u8 {u8_ms:.1} ms / {u8_mb:.2} MB; 2-bit {p2_ms:.1} ms / {p2_mb:.2} MB (x{:.2} time, {:.1}% mem); 5-trit {tr_ms:.1} ms / {tr_mb:.2} MB (x{:.2} time, {:.1}% mem)",
        p2_ms / u8_ms,
        p2_mb / u8_mb * 100.0,
        tr_ms / u8_ms,
        tr_mb / u8_mb * 100.0,
    );
}
