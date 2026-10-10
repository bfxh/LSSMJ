//! 规模化第一片判据：拆两道上限后的直出（compact）路径——
//! ① 192³ 分块无关性：`compact(C=32)` vs `compact(C=64)` 稠密位置**逐位** + 规范四边形集合
//!    一致（此前默认 limits 下 idx 绑定在 n≥124 拒发、散射派发在 n≥161 拒发）；
//! ② 256³ 冒烟 + 确定性：两跑 C=32 稠密位置逐位一致，且与 C=128 亦一致；
//! ③ `#[ignore]` 384³ 手动路（显存 ~1.4G，安静机按需跑）。
//!
//! 口径：稠密顶点流按 cell 序 ⇒ **位置可逐位对拍**；索引槽位 = emit 趟 atomicAdd
//! 执行序（非确定）⇒ 一律按"四顶点**有向**规范键"排序后对拍（F04：循环旋转保绕序）。

use conv_core::{
    field_to_voxels,
    gsn::{CompactMesh, surface_nets_gpu_compact},
    jfa::headless_device,
};
use std::sync::Mutex;

mod common;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const R: f32 = 0.75;

/// 有向规范四边形键：两枚三角（各自循环旋转最小化后，三角对字典序）。
type QuadKey = ([u32; 3], [u32; 3]);

fn canon_quads(indices: &[u32]) -> Vec<QuadKey> {
    common::directed_quads(indices, |v| v)
}

fn assert_positions_bitwise_equal(a: &[[f32; 3]], b: &[[f32; 3]]) {
    assert_eq!(a.len(), b.len(), "顶点数不一致");
    let diff = a
        .iter()
        .zip(b)
        .filter(|(x, y)| {
            x[0].to_bits() != y[0].to_bits()
                || x[1].to_bits() != y[1].to_bits()
                || x[2].to_bits() != y[2].to_bits()
        })
        .count();
    assert_eq!(diff, 0, "稠密位置逐位不一致顶点数={diff}");
}

fn report(tag: &str, m: &CompactMesh) {
    println!(
        "{tag}: 顶点={} 四边形={}",
        m.vertex_count,
        m.indices.len() / 6,
    );
}

#[test]
fn scale_192_chunk_invariance() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 192u32;
    let t0 = std::time::Instant::now();
    let sdf = field_to_voxels(n, R);
    let t_field = t0.elapsed();

    let t1 = std::time::Instant::now();
    let (a, sa) = surface_nets_gpu_compact(&hd, &sdf, n, 32, None);
    let t_a = t1.elapsed();
    let (b, sb) = surface_nets_gpu_compact(&hd, &sdf, n, 64, None);

    // 球面（R=0.75，192³）在带壳上应远多于 4 万表面 cell（64³ 实测 10538，(191/63)² ≈ 9×）
    assert!(a.vertex_count > 40_000, "顶点数异常少：{}", a.vertex_count);
    assert_eq!(a.vertex_count, b.vertex_count, "分块改变顶点数");
    assert_eq!(a.indices.len(), b.indices.len(), "分块改变索引数");
    assert_positions_bitwise_equal(&a.positions, &b.positions);
    assert_eq!(
        canon_quads(&a.indices),
        canon_quads(&b.indices),
        "分块改变四边形集合"
    );
    println!(
        "192³ 分块无关：C=32 活跃 {}/{} | C=64 活跃 {}/{} | 场生成 {t_field:?} C=32 跑 {t_a:?}",
        sa.active, sa.chunks, sb.active, sb.chunks
    );
    report("192³", &a);
}

#[test]
fn scale_256_smoke_and_deterministic() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 256u32;
    let t0 = std::time::Instant::now();
    let sdf = field_to_voxels(n, R);
    let t_field = t0.elapsed();

    let t1 = std::time::Instant::now();
    let (a, _) = surface_nets_gpu_compact(&hd, &sdf, n, 32, None);
    let t_a = t1.elapsed();
    let (b, _) = surface_nets_gpu_compact(&hd, &sdf, n, 128, None);
    let (c, _) = surface_nets_gpu_compact(&hd, &sdf, n, 32, None);

    assert!(a.vertex_count > 100_000, "顶点数异常少：{}", a.vertex_count);
    assert_eq!(a.vertex_count, b.vertex_count);
    assert_eq!(a.vertex_count, c.vertex_count);
    assert_positions_bitwise_equal(&a.positions, &b.positions);
    assert_positions_bitwise_equal(&a.positions, &c.positions);
    assert_eq!(
        canon_quads(&a.indices),
        canon_quads(&b.indices),
        "分块改变四边形集合"
    );
    assert_eq!(
        canon_quads(&a.indices),
        canon_quads(&c.indices),
        "重复运行不确定"
    );
    println!("256³（1670 万格点）：场生成 {t_field:?} C=32 跑 {t_a:?}");
    report("256³", &a);
}

#[test]
#[ignore = "手动路：384³ 直出（显存 ~1.4G、场生成 ~0.3s）——安静机按需跑"]
fn scale_384_manual() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let n = 384u32;
    let t0 = std::time::Instant::now();
    let sdf = field_to_voxels(n, R);
    let t_field = t0.elapsed();
    let t1 = std::time::Instant::now();
    let (m, s) = surface_nets_gpu_compact(&hd, &sdf, n, 32, None);
    let t_run = t1.elapsed();
    assert!(m.vertex_count > 200_000, "顶点数异常少：{}", m.vertex_count);
    assert!(
        m.positions.iter().all(|p| p.iter().all(|c| c.is_finite())),
        "存在非有限顶点"
    );
    println!(
        "384³（5660 万格点）：场生成 {t_field:?} 直出 {t_run:?} 活跃 {}/{}",
        s.active, s.chunks
    );
    report("384³", &m);
}
