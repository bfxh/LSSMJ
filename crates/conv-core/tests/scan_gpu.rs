//! GPU 排他前缀和积木判据（T-GC-05 第三片）：对拍 CPU 前缀和（逐位，含回绕口径）+
//! 块边界语料（0/1/255/256/257/1000/262144）+ 确定性。

use conv_core::{jfa::Headless, jfa::headless_device, scan::exclusive_prefix_sum_u32};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn cpu_prefix(data: &[u32]) -> (Vec<u32>, u32) {
    let mut out = Vec::with_capacity(data.len());
    let mut acc: u32 = 0;
    for &v in data {
        out.push(acc);
        acc = acc.wrapping_add(v);
    }
    (out, acc)
}

fn check(hd: &Headless, data: &[u32]) {
    let (gpu, gpu_total) = exclusive_prefix_sum_u32(hd, data, None);
    let (cpu, cpu_total) = cpu_prefix(data);
    assert_eq!(gpu, cpu, "前缀和不一致（len={}）", data.len());
    assert_eq!(gpu_total, cpu_total, "总数不一致（len={}）", data.len());
}

#[test]
fn scan_matches_cpu_prefix() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    // 空 / 单元素 / 块边界 255·256·257 / 跨块 1000
    check(&hd, &[]);
    check(&hd, &[7]);
    check(&hd, &[1; 255]);
    check(&hd, &[1; 256]);
    check(&hd, &[1; 257]);
    check(&hd, &[5; 1000]);
    // 任意 u32（含溢出回绕，与 CPU wrapping 口径一致）
    check(&hd, &[0, u32::MAX, 1, u32::MAX / 2, 42]);
    // 大语料（64³ 规模 = 真实消费方尺寸）
    let mut rng = conv_core::Lcg::new(0x5ca4);
    let big: Vec<u32> = (0..(64 * 64 * 64))
        .map(|_| if rng.next01() < 0.02 { 1 } else { 0 })
        .collect();
    check(&hd, &big);
}

#[test]
fn scan_deterministic_bitwise() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let mut rng = conv_core::Lcg::new(0x5ca5);
    let data: Vec<u32> = (0..(64 * 64 * 64))
        .map(|_| if rng.next01() < 0.02 { 1 } else { 0 })
        .collect();
    let (a, ta) = exclusive_prefix_sum_u32(&hd, &data, None);
    let (b, tb) = exclusive_prefix_sum_u32(&hd, &data, None);
    assert_eq!(a, b, "两次扫描不一致（非确定）");
    assert_eq!(ta, tb);
}
