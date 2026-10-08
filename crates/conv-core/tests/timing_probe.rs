//! C22 逐边 GPU 计时探针（T-GC-06）：三条转换腿各 N 次实测，记档 min/median/p95/max（ms）。
//! 第一片只记档（先量后改）；本片（第二片）钉阈值——P95 棘轮上限 = w15d 记档 p95 × 2（红档），
//! 超 1.5× 记档为黄档（只告警）；档位口径沿 gate_all 计时软门先例（W3H-051/052）。
//! `budget_gate_canary` 独立构造越界样本先验红（门必须会红）；
//! `budget_degrade_canary` 钉降级阶梯的序（先降旋钮再降腿）与实效（降档必须落回合成预算）。
//! 性能类门：安静机独占跑（04-ci-and-gates §B.3）。
//! 产出：`scratch/w15d/gpu-timings.json`。

use conv_core::{
    GRID, field_to_voxels, gsn::surface_nets_gpu, icosphere, jfa::headless_device,
    jfa::jfa_distance_field, mesh_sdf_gpu::mesh_to_sdf_gpu, timer::GpuTimer,
};
use std::sync::Mutex;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const R: f32 = 0.75;
const WARMUP: u32 = 3;
const RUNS: u32 = 15;

/// P95 棘轮上限（ms）= w15d 记档 p95 × 2，与场景绑定（64³）——棘轮只准减，
/// 上调必须在此注记"为什么可以涨"。
const P95_BOUND_MS: &[(&str, f64)] = &[
    ("jfa_64_4096seeds", 0.60), // 记档 p95=0.2994（d239d7b，RTX 4060 Ti Vulkan，n=15）
    ("mesh_to_sdf_64", 1.71),   // 记档 p95=0.8565
    // 上调留档（为什么可以涨）：T-GC-05 第五片把分块遍历搬上 GPU（occupancy + 块 scan +
    // build_active + indirect 参数 + indirect 派发，共 +7 个 dispatch ≈ +40µs 固定开销 @64³）
    // ——换取大网格/稀疏场景的口径可扩展性；新基线 = 实测 p95 0.064ms（×2 得上限）。
    // 规模第一片两趟发射（count 趟 + 4B 回读同步，换 idx O(72·n³)→O(24B/quad)）⇒ 实测
    // p95 0.076ms——上限不变（仍在 0.13 内，未触黄档）。
    ("gsn_64", 0.13),
];

/// 预算判定：超上限 ⇒ Some(红因)；超 1.5× 记档（上限×0.75）⇒ 打印黄档告警。
fn budget_check(leg: &str, p95_ms: f64, bound_ms: f64) -> Option<String> {
    if p95_ms > bound_ms {
        return Some(format!(
            "{leg}: p95 {p95_ms:.4}ms > 上限 {bound_ms:.4}ms（2× 记档，红档）"
        ));
    }
    if p95_ms > bound_ms * 0.75 {
        println!("BUDGET-WARN {leg}: p95 {p95_ms:.4}ms > 1.5× 记档（黄档；上限 {bound_ms:.4}ms）");
    }
    None
}

fn fnv_f32(data: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for v in data {
        for b in v.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn fnv_mesh(positions: &[[f32; 3]], indices: &[u32]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for p in positions {
        for c in p {
            for b in c.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
    }
    for i in indices {
        for b in i.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

fn stats(samples: &mut [f64]) -> (f64, f64, f64, f64) {
    samples.sort_by(|x, y| x.total_cmp(y));
    let min = samples[0];
    let median = samples[samples.len() / 2];
    let p95v = samples[((samples.len() as f64 * 0.95).ceil() as usize).max(1) - 1];
    let max = samples[samples.len() - 1];
    (min, median, p95v, max)
}

#[test]
fn timing_probe_all_legs() {
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let ts = hd
        .timestamp_period
        .expect("本机适配器不支持 TIMESTAMP_QUERY——C22 计时判据无法执行");
    println!("ADAPTER {}", hd.adapter_name);

    // 腿 A：JFA（粒子→SDF，64³ / 4096 种子）
    let seeds: Vec<[f32; 3]> = {
        let mut rng = conv_core::Lcg::new(0x7061);
        (0..4096)
            .map(|_| {
                let s = |r: &mut conv_core::Lcg| (r.next01() * 64.0).min(63.0).floor();
                [s(&mut rng), s(&mut rng), s(&mut rng)]
            })
            .collect()
    };
    // 腿 B：mesh→SDF（icosphere 320 面，64³）
    let (verts, faces) = icosphere(2, R);
    // 腿 C：GSN（SDF→mesh，同一场）
    let sdf = field_to_voxels(GRID, R);

    let mut out: Vec<String> = Vec::new();
    let mut violations: Vec<String> = Vec::new();
    {
        let mut sample = |name: &str, run: &mut dyn FnMut(Option<&mut GpuTimer>) -> u64| {
            for _ in 0..WARMUP {
                run(None);
            }
            let mut samples: Vec<f64> = Vec::new();
            let mut hashes: Vec<u64> = Vec::new();
            for _ in 0..RUNS {
                let mut t = GpuTimer::try_new(&hd.device, hd.timestamp_period, 64)
                    .expect("timer unavailable");
                let h = run(Some(&mut t));
                let ms = t.resolve_ms(&hd);
                samples.push(ms.iter().sum());
                hashes.push(h);
            }
            assert!(
                samples.iter().all(|s| *s > 0.0),
                "{name}: 计时读数为零——仪器失效（时间戳未落）"
            );
            assert!(
                hashes.iter().all(|h| *h == hashes[0]),
                "{name}: 计时路径下输出非逐位确定"
            );
            let (min, med, p95v, max) = stats(&mut samples);
            let bound = P95_BOUND_MS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, b)| *b)
                .expect("该腿无 P95 上限登记（P95_BOUND_MS 缺失）");
            if let Some(v) = budget_check(name, p95v, bound) {
                violations.push(v);
            }
            println!(
                "LEG {name}: min={min:.3} p50={med:.3} p95={p95v:.3} max={max:.3} ms (n={RUNS}, period={ts:.4}ns/tick)"
            );
            out.push(format!(
                "\"{name}\":{{\"min_ms\":{min:.4},\"p50_ms\":{med:.4},\"p95_ms\":{p95v:.4},\"max_ms\":{max:.4},\"p95_bound_ms\":{bound},\"samples\":{RUNS}}}"
            ));
        };

        sample("jfa_64_4096seeds", &mut |t| {
            fnv_f32(&jfa_distance_field(&hd, &seeds, 64, t))
        });
        sample("mesh_to_sdf_64", &mut |t| {
            fnv_f32(&mesh_to_sdf_gpu(&hd, &verts, &faces, GRID, t))
        });
        sample("gsn_64", &mut |t| {
            // 顶点流逐位（T-GC-02 判据口径）；索引槽位执行序非确定——四边形集合确定性由 gsn_gpu 判据覆盖
            let m = surface_nets_gpu(&hd, &sdf, GRID, t);
            fnv_mesh(&m.positions, &[])
        });
    }

    let json = format!(
        "{{\"adapter\":\"{}\",\"adapter_ts_period_ns\":{ts:.6},\"warmup\":{WARMUP},\"runs\":{RUNS},\"legs\":{{{}}}}}",
        hd.adapter_name,
        out.join(",")
    );
    println!("TIMINGS {json}");
    let dir = "../../scratch/w15d";
    std::fs::create_dir_all(dir).expect("mkdir");
    std::fs::write(format!("{dir}/gpu-timings.json"), json).expect("write timings");

    assert!(
        violations.is_empty(),
        "预算门红（p95 超 2× 记档上限）：\n{}",
        violations.join("\n")
    );
}

/// 预算门金丝雀（先验红）：独立量级构造证明门会红；边界（恰=上限）不判红。
#[test]
fn budget_gate_canary() {
    assert!(budget_check("canary", 1.0, 2.0).is_none(), "好样本被判红");
    assert!(
        budget_check("canary", 2.0, 2.0).is_none(),
        "边界样本（恰=上限）不应判红"
    );
    let red = budget_check("canary", 200.0, 2.0);
    assert!(
        red.is_some(),
        "金丝雀失败：100× 越界样本未判红 ⇒ 预算门失效"
    );
}

/// 预算降级金丝雀（先验红 + 实效锚到真实测量）：规则侧钉死"先降旋钮再降腿"的序；
/// 观测侧在 GPU 上实跑全档 vs 降档分辨率——合成预算 = 全档实测的一半，
/// 降档读数必须落回预算内（否则降级是纸面动作，此测试判红）。
#[test]
fn budget_degrade_canary() {
    use conv_core::budget::{BudgetPlan, BudgetResponse, respond};

    // ---- 规则侧（独立构造样本，不依赖 GPU）----
    let nominal = BudgetPlan::nominal();
    assert_eq!(
        respond(false, nominal),
        BudgetResponse::Keep,
        "预算内不得动旋钮"
    );
    let step1 = match respond(true, nominal) {
        BudgetResponse::Degrade(p) => p,
        other => panic!("超阈未降旋钮 ⇒ 阶梯失效：{other:?}"),
    };
    assert!(
        step1.voxel_resolution < nominal.voxel_resolution,
        "降档必须严减旋钮（{} 未小于 {}）",
        step1.voxel_resolution,
        nominal.voxel_resolution
    );
    // 一路到旋钮尽 ⇒ 必须降腿（"先降旋钮再降腿"的序）
    let mut plan = step1;
    let mut dropped = false;
    for _ in 0..8 {
        match respond(true, plan) {
            BudgetResponse::Degrade(next) => plan = next,
            BudgetResponse::DropLeg => {
                dropped = true;
                break;
            }
            BudgetResponse::Keep => panic!("超阈路径出现 Keep ⇒ 阶梯失效"),
        }
    }
    assert!(dropped, "旋钮耗尽后必须降腿");

    // ---- 观测侧（真实 GPU 测量）----
    let _gpu = GPU_LOCK.lock().unwrap();
    let hd = headless_device();
    let (verts, faces) = icosphere(2, R);
    let measure = |n: u32| {
        let mut best = f64::INFINITY;
        for _ in 0..3 {
            let mut t =
                GpuTimer::try_new(&hd.device, hd.timestamp_period, 64).expect("timer unavailable");
            mesh_to_sdf_gpu(&hd, &verts, &faces, n, Some(&mut t));
            best = best.min(t.resolve_ms(&hd).iter().sum::<f64>());
        }
        best
    };
    let t_full = measure(nominal.voxel_resolution);
    // 合成预算：全档实测的一半——构造自观测，不是硬编码阈值
    let budget = t_full * 0.5;
    let t_degraded = measure(step1.voxel_resolution);
    assert!(
        t_degraded <= budget,
        "降档后未落回合成预算：full={t_full:.4}ms budget={budget:.4}ms degraded={t_degraded:.4}ms"
    );
    println!(
        "DEGRADE-CANARY full={t_full:.4}ms -> n={} {t_degraded:.4}ms (合成预算 {budget:.4}ms)",
        step1.voxel_resolution
    );
}
