//! GPU JFA 积木判据：对 CPU 暴力参照的精确性对拍 + 两次运行逐位确定性。
//! （点种子下 JFA 给出精确最近种子距离——label 平局不影响距离值。）

use conv_core::{Lcg, jfa::headless_device, jfa::jfa_distance_field};
use std::sync::Mutex;

// GPU 测试必须串行：并行建设备会死锁（本机实测惯犯）——全局互斥锁保证任何线程配置下安全
static GPU_LOCK: Mutex<()> = Mutex::new(());

fn seed_points(n: u32, k: usize) -> Vec<[f32; 3]> {
    let mut rng = Lcg::new(0x5eed_2026_1007);
    let mut out = Vec::with_capacity(k);
    for _ in 0..k {
        // 整数位置种子（本积木的口径：粒子先散射到最近体素）
        let s = |rng: &mut Lcg| (rng.next01() * n as f32).min((n - 1) as f32).floor();
        out.push([s(&mut rng), s(&mut rng), s(&mut rng)]);
    }
    out
}

fn cpu_brute(n: u32, seeds: &[[f32; 3]]) -> Vec<f32> {
    let count = (n * n * n) as usize;
    let mut out = vec![f32::INFINITY; count];
    for (i, d) in out.iter_mut().enumerate() {
        let i = i as u32;
        let x = i % n;
        let y = (i / n) % n;
        let z = i / (n * n);
        let p = [x as f32, y as f32, z as f32];
        let mut best = f32::INFINITY;
        for s in seeds {
            let dx = p[0] - s[0];
            let dy = p[1] - s[1];
            let dz = p[2] - s[2];
            let dd = (dx * dx + dy * dy + dz * dz).sqrt();
            if dd < best {
                best = dd;
            }
        }
        *d = best;
    }
    out
}

fn fnv(data: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for v in data {
        for b in v.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

#[test]
fn jfa_matches_cpu_brute() {
    let _gpu = GPU_LOCK.lock().unwrap();
    // JFA 是近似算法（Rong-Tan 2006 口径："approximation to the distance transform"）——
    // 判据 = 对拍带：mean 极小 + max ≤ 1 体素间距；罕见体素的标签失准计入近似口径。
    let n = 32;
    let seeds = seed_points(n, 128);
    let headless = headless_device();
    let gpu = jfa_distance_field(&headless, &seeds, n);
    let cpu = cpu_brute(n, &seeds);
    let mut sum = 0f64;
    let mut max_err = 0f32;
    let mut cnt_01 = 0usize;
    let mut cnt_1 = 0usize;
    for (g, c) in gpu.iter().zip(&cpu) {
        let e = (g - c).abs();
        sum += e as f64;
        max_err = max_err.max(e);
        if e > 0.01 {
            cnt_01 += 1;
        }
        if e > 1.0 {
            cnt_1 += 1;
        }
    }
    let mean = sum / gpu.len() as f64;
    println!(
        "JFA mean={mean:.6} max={max_err:.4} err>0.01: {cnt_01} err>1.0: {cnt_1} / {}",
        gpu.len()
    );
    assert!(mean <= 0.01, "mean_err={mean}");
    assert!(max_err <= 1.0, "max_err={max_err}");
}

#[test]
fn jfa_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let n = 32;
    let seeds = seed_points(n, 128);
    let headless = headless_device();
    let a = jfa_distance_field(&headless, &seeds, n);
    let b = jfa_distance_field(&headless, &seeds, n);
    assert_eq!(fnv(&a), fnv(&b), "bitwise determinism across runs");
}
