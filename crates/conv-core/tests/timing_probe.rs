//! C22 逐边 GPU 计时探针（T-GC-06 第一片）：三条转换腿各 N 次实测，
//! 记档 min/median/p95/max（ms）。先量后改——阈值（上限）按本批读数在下一片钉。
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
    let mut sample = |name: &str, run: &mut dyn FnMut(Option<&mut GpuTimer>) -> u64| {
        for _ in 0..WARMUP {
            run(None);
        }
        let mut samples: Vec<f64> = Vec::new();
        let mut hashes: Vec<u64> = Vec::new();
        for _ in 0..RUNS {
            let mut t =
                GpuTimer::try_new(&hd.device, hd.timestamp_period, 64).expect("timer unavailable");
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
        println!(
            "LEG {name}: min={min:.3} p50={med:.3} p95={p95v:.3} max={max:.3} ms (n={RUNS}, period={ts:.4}ns/tick)"
        );
        out.push(format!(
            "\"{name}\":{{\"min_ms\":{min:.4},\"p50_ms\":{med:.4},\"p95_ms\":{p95v:.4},\"max_ms\":{max:.4},\"samples\":{RUNS}}}"
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

    let json = format!(
        "{{\"adapter\":\"{}\",\"adapter_ts_period_ns\":{ts:.6},\"warmup\":{WARMUP},\"runs\":{RUNS},\"legs\":{{{}}}}}",
        hd.adapter_name,
        out.join(",")
    );
    println!("TIMINGS {json}");
    let dir = "../../scratch/w15d";
    std::fs::create_dir_all(dir).expect("mkdir");
    std::fs::write(format!("{dir}/gpu-timings.json"), json).expect("write timings");
}
