//! `gauss2mesh`：把 3DGS 高斯（或合成场景）体素化成标量场，再用 Surface Nets 提网格。
//!
//! 这个二进制是"转换腿"的用户面：阶段进度、auto 选路、三旋钮、显式回退记账、
//! 自包含 WebGL 预览。库侧 API 在 `conv_core::gauss`。

mod args;
mod html;
mod report;

use args::{Config, HELP, ModeSel, is_bool_flag, parse_config};
use conv_core::gauss::mesh::{
    Mesh, field_to_mesh, field_to_mesh_gpu, keep_largest_component, write_obj,
};
use conv_core::gauss::ply::{PlyReadOptions, parse_ply_with};
use conv_core::gauss::voxel::{
    Backend, Field, Grid, VoxelMode, VoxelizeOptions, sparse_density::SparseDensity,
    sparse_surfel::SparseSurfel, surface_blocks::SurfaceBlocks, voxelize, voxelize_gpu_with,
};
use conv_core::gauss::{
    Gaussian, PruneOptions, add_floaters, blob_sphere_gaussians, bounds_of, median_flatness, prune,
    sphere_gaussians, torus_gaussians,
};
use conv_core::jfa::{Headless, try_headless_device_sized};
use html::write_html;
use report::{ReportData, Timings, jstr, report_json};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

static GPU_CACHE: OnceLock<Headless> = OnceLock::new();

/// >256³ 的稀疏分块边长。512³ 实测 B=16 比 B=32 少 35% 角点、build 快 20%。
const SPARSE_BLOCK: u32 = 16;

/// 同进程内复用同一 wgpu 设备；batch 的首个文件付 init，其余复用。
fn cached_gpu(need_bytes: u64) -> Result<&'static Headless, String> {
    if let Some(hd) = GPU_CACHE.get() {
        return Ok(hd);
    }
    let hd = try_headless_device_sized(need_bytes)?;
    Ok(GPU_CACHE.get_or_init(|| hd))
}

struct Loaded {
    gaussians: Vec<Gaussian>,
    subject: String,
    format: &'static str,
    has_normals: bool,
    source_bytes: usize,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("\n错误：{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print!("{HELP}");
        return Ok(());
    }
    if matches!(args[1].as_str(), "--help" | "-h" | "help") {
        print!("{HELP}");
        return Ok(());
    }
    let mut flags: HashMap<String, String> = HashMap::new();
    let mut bools: HashSet<String> = HashSet::new();
    let mut positional: Vec<String> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        let a = &args[i];
        if a == "--help" || a == "-h" {
            bools.insert("help".into());
            i += 1;
        } else if let Some(rest) = a.strip_prefix("--") {
            if let Some(eq) = rest.find('=') {
                flags.insert(rest[..eq].to_string(), rest[eq + 1..].to_string());
                i += 1;
            } else if is_bool_flag(rest) {
                bools.insert(rest.to_string());
                i += 1;
            } else {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| format!("--{rest} 缺参数值"))?;
                flags.insert(rest.to_string(), v.clone());
                i += 2;
            }
        } else {
            positional.push(a.clone());
            i += 1;
        }
    }
    if bools.contains("help") {
        print!("{HELP}");
        return Ok(());
    }
    let cfg = parse_config(&flags, &bools)?;
    match args[1].as_str() {
        "demo" => {
            let scene = positional
                .first()
                .map(String::as_str)
                .or_else(|| flags.get("scene").map(String::as_str))
                .unwrap_or("sphere");
            run_demo(&cfg, scene)
        }
        "convert" => {
            let input = positional
                .first()
                .ok_or("convert 需要输入 PLY：gauss2mesh convert <input.ply>")?;
            // auto：单文件只有大网格 + surfel 才走 GPU。
            // 512³ density 的 GPU job 表会溢出 u32（探针已断言），必须留在稀疏 CPU。
            let mut cfg = cfg;
            if cfg.gpu_auto && cfg.grid >= 512 && cfg.mode == ModeSel::Surfel {
                cfg.gpu = true;
            }
            run_convert(&cfg, Path::new(input))
        }
        "batch" => run_batch(&cfg, &positional),
        "inspect" => {
            let input = positional
                .first()
                .ok_or("inspect 需要输入 PLY：gauss2mesh inspect <input.ply>")?;
            run_inspect(&cfg, Path::new(input))
        }
        other => Err(format!("未知命令：{other}\n\n{HELP}")),
    }
}

fn run_demo(cfg: &Config, scene: &str) -> Result<(), String> {
    let t0 = Instant::now();
    let mut gaussians = match scene {
        "sphere" => sphere_gaussians(20_000, 0.75, 0.99),
        "torus" => torus_gaussians(180, 110, 0.55, 0.22, 0.99),
        "blobs" => blob_sphere_gaussians(12_000, 0.75, 0.05, 0.95, cfg.seed),
        other => {
            return Err(format!(
                "未知 demo 场景：{other}（可选 sphere|torus|blobs）"
            ));
        }
    };
    let has_normals = gaussians.iter().any(|g| g.normal.is_some());
    if scene == "blobs" {
        add_floaters(&mut gaussians, 300, cfg.seed ^ 0x9e37_79b9);
    }
    let reference = match scene {
        "sphere" => Some(("球", 4.0 / 3.0 * std::f32::consts::PI * 0.75f32.powi(3))),
        "torus" => Some((
            "环面",
            2.0 * std::f32::consts::PI.powi(2) * 0.55 * 0.22 * 0.22,
        )),
        _ => None,
    };
    let loaded = Loaded {
        gaussians,
        subject: format!("demo:{scene}"),
        format: "synthetic",
        has_normals,
        source_bytes: 0,
    };
    let load_ms = t0.elapsed().as_secs_f64() * 1e3;

    let stem = scene.to_string();
    let out_dir = &cfg.out_dir;
    fs::create_dir_all(out_dir).map_err(|e| format!("建目录 {} 失败：{e}", out_dir.display()))?;
    let obj = cfg
        .out
        .clone()
        .unwrap_or_else(|| out_dir.join(format!("{stem}.obj")));
    let html = if cfg.no_html {
        None
    } else {
        Some(
            cfg.html
                .clone()
                .unwrap_or_else(|| obj.with_extension("html")),
        )
    };
    let json = Some(
        cfg.json
            .clone()
            .unwrap_or_else(|| obj.with_extension("json")),
    );
    let title = format!("高斯体素 → 网格 · {stem}");
    convert_and_write(
        cfg,
        &loaded,
        load_ms,
        &title,
        &obj,
        html.as_deref(),
        json.as_deref(),
        reference,
    )
}

fn run_convert(cfg: &Config, input: &Path) -> Result<(), String> {
    let t0 = Instant::now();
    let bytes = fs::read(input).map_err(|e| format!("读不了 {}：{e}", input.display()))?;
    let data = parse_ply_with(
        &bytes,
        &PlyReadOptions {
            scales_are_log: !cfg.raw_scale,
            opacity_is_logit: !cfg.raw_opacity,
        },
    )?;
    let load_ms = t0.elapsed().as_secs_f64() * 1e3;
    let loaded = Loaded {
        gaussians: data.gaussians,
        subject: input.display().to_string(),
        format: data.format,
        has_normals: data.has_normals,
        source_bytes: bytes.len(),
    };
    let obj = cfg
        .out
        .clone()
        .unwrap_or_else(|| input.with_extension("obj"));
    let html = if cfg.no_html {
        None
    } else {
        Some(
            cfg.html
                .clone()
                .unwrap_or_else(|| input.with_extension("html")),
        )
    };
    let title = input
        .file_name()
        .map(|s| format!("{} · 高斯体素 → 网格", s.to_string_lossy()))
        .unwrap_or_else(|| "高斯体素 → 网格".into());
    convert_and_write(
        cfg,
        &loaded,
        load_ms,
        &title,
        &obj,
        html.as_deref(),
        cfg.json.as_deref(),
        None,
    )
}

fn run_batch(cfg: &Config, inputs: &[String]) -> Result<(), String> {
    if inputs.is_empty() {
        return Err("batch 需要至少一个输入 PLY：gauss2mesh batch <a.ply> [b.ply ...]".into());
    }
    if cfg.out.is_some() || cfg.html.is_some() || cfg.json.is_some() {
        return Err("batch 用 --out-dir；--out/--html/--json 是单文件选项".into());
    }
    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("建目录 {} 失败：{e}", cfg.out_dir.display()))?;
    // auto：批量 ≥2 文件时 GPU 初始化可摊平（实测 4×256³ 为 2810 vs 3344 ms）
    let auto_gpu = cfg.gpu_auto && inputs.len() >= 2;
    if auto_gpu {
        eprintln!(
            "auto: 批量 {} 文件 → 自研 GPU 内核（初始化可摊平）",
            inputs.len()
        );
    }
    let mut used: HashSet<std::path::PathBuf> = HashSet::new();
    let mut ok = 0usize;
    let mut failed = 0usize;
    let batch_start = Instant::now();
    for (i, input) in inputs.iter().enumerate() {
        let input = Path::new(input);
        let stem = input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "mesh".to_string());
        let mut obj = cfg.out_dir.join(format!("{stem}.obj"));
        let mut n = 2;
        while used.contains(&obj) {
            obj = cfg.out_dir.join(format!("{stem}_{n}.obj"));
            n += 1;
        }
        used.insert(obj.clone());
        let mut c = cfg.clone();
        if auto_gpu {
            c.gpu = true;
            c.gpu_auto = false;
        }
        c.out = Some(obj.clone());
        c.html = if cfg.no_html {
            None
        } else {
            Some(obj.with_extension("html"))
        };
        c.json = Some(obj.with_extension("json"));
        println!(
            "\n===== [{}/{}] {} =====",
            i + 1,
            inputs.len(),
            input.display()
        );
        match run_convert(&c, input) {
            Ok(()) => ok += 1,
            Err(e) => {
                failed += 1;
                eprintln!("✗ {}：{e}", input.display());
            }
        }
    }
    println!(
        "\n批处理完成：{ok} 成功 / {failed} 失败 · {:.1} ms",
        batch_start.elapsed().as_secs_f64() * 1e3
    );
    if failed > 0 {
        Err(format!("{failed} 个输入失败"))
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn convert_and_write(
    cfg: &Config,
    loaded: &Loaded,
    load_ms: f64,
    title: &str,
    obj: &Path,
    html: Option<&Path>,
    json: Option<&Path>,
    reference: Option<(&str, f32)>,
) -> Result<(), String> {
    let mut timings = Timings::new();
    timings.load = load_ms;
    if !cfg.quiet {
        let size = if loaded.source_bytes > 0 {
            format!(" · {:.1} KB", loaded.source_bytes as f64 / 1024.0)
        } else {
            String::new()
        };
        stage(
            1,
            5,
            "读入",
            &format!(
                "{} · {} 高斯{} · {:.1} ms",
                loaded.format,
                fmt_int(loaded.gaussians.len()),
                size,
                timings.load
            ),
        );
    }

    let t = Instant::now();
    let (kept, prune_stats) = prune(
        &loaded.gaussians,
        &PruneOptions {
            min_opacity: cfg.prune_opacity,
            min_radius: cfg.min_radius,
            max_radius: cfg.max_radius,
        },
    );
    timings.prune = t.elapsed().as_secs_f64() * 1e3;
    if kept.is_empty() {
        return Err("剪枝后没有高斯：降低 --prune-opacity 或 --min-radius".into());
    }
    if !cfg.quiet {
        stage(
            2,
            5,
            "剪枝",
            &format!(
                "{} 保留 · 丢 {}（低不透明度 {} / 半径 {} / 非法 {}）· {:.1} ms",
                fmt_int(kept.len()),
                fmt_int(prune_stats.input - prune_stats.kept),
                fmt_int(prune_stats.dropped_opacity),
                fmt_int(prune_stats.dropped_radius),
                fmt_int(prune_stats.dropped_invalid),
                timings.prune
            ),
        );
    }

    let flatness = median_flatness(&kept);
    let (mut mode, mut auto_reason) = choose_mode(cfg.mode, loaded.has_normals, flatness);
    let threads = if cfg.threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    } else {
        cfg.threads
    };
    let gpu_dense_512 = cfg.grid > 256 && cfg.gpu && mode == VoxelMode::Surfel;
    let mut sparse = cfg.grid > 256 && !gpu_dense_512;
    let mut gpu_fallback = None;
    let want_gpu = if cfg.grid > 256 && !gpu_dense_512 {
        if cfg.gpu || cfg.gpu_mesh {
            gpu_fallback = Some(if mode == VoxelMode::Density {
                "512³ density 的 GPU job 表会溢出 u32；已走稀疏 CPU（auto-iso）".into()
            } else {
                ">256³ 的 --gpu-mesh 需要配合 --gpu（dense GPU）；已走稀疏 CPU".into()
            });
        }
        false
    } else {
        cfg.gpu || cfg.gpu_mesh
    };
    // --density-norm 只实现了 CPU 内核；显式/自动选 GPU 时降级到 CPU 并说明。
    let want_gpu = if cfg.density_norm_ref.is_some() && want_gpu {
        gpu_fallback = Some("--density-norm 仅有 CPU 实现 → CPU".into());
        false
    } else {
        want_gpu
    };
    let gpu_device = if want_gpu {
        let t = Instant::now();
        // 只按本次实际需要的缓冲申请 limits（小网格不要求 512 MB 绑定）
        let need = (cfg.grid as u64).pow(3) * if mode == VoxelMode::Surfel { 16 } else { 4 };
        match cached_gpu(need) {
            Ok(hd) => {
                timings.gpu_init = t.elapsed().as_secs_f64() * 1e3;
                Some(hd)
            }
            Err(e) => {
                gpu_fallback = Some(format!("GPU 初始化失败（{e}）→ CPU"));
                None
            }
        }
    } else {
        None
    };
    if want_gpu && cfg.grid > 256 && cfg.gpu && gpu_device.is_none() {
        sparse = true;
        gpu_fallback = Some("GPU 初始化失败 → 512 走稀疏 CPU".into());
    }
    let gpu_ref = gpu_device;
    // >256³ 的稀疏 density：数值核宽下限 `0.75×h` 在细网格上远小于数据的作者尺度，
    // 于是等值面被抽成薄壳，默认 `seal=true` 承诺的「实心体」拿不到（实测 512³
    // 0.25505 / χ=−39 / 25 分量）。默认把核宽钉在参考 256³ 的世界尺度，512³ 得到
    // 0.72455 / χ=5 / 5 分量。≤256³ 与 surfel 不受影响；显式 --kernel-clamp-* 优先。
    let mut clamp_default_note = None;
    let kernel_clamp_ref = if cfg.kernel_clamp_ref.is_none()
        && cfg.kernel_clamp_world.is_none()
        && !cfg.splat_integrate
        && cfg.cross_resolution.is_none()
        && sparse
        && mode == VoxelMode::Density
        && cfg.grid > 256
    {
        clamp_default_note = Some(
            ">256³ density 默认把核宽钉在参考 256³ 世界尺度（否则等值面被抽成薄壳）".to_string(),
        );
        Some(256u32)
    } else {
        cfg.kernel_clamp_ref
    };
    let mut voxel_opts = VoxelizeOptions {
        mode,
        bins3d: cfg.bins3d,
        simd_prep: cfg.simd_prep,
        fast_exp: cfg.fast_exp,
        backend: if !sparse && cfg.gpu && gpu_device.is_some() {
            Backend::Gpu
        } else {
            Backend::Cpu
        },
        n: cfg.grid,
        padding: cfg.padding,
        iso_frac: 0.65,
        iso_abs: cfg.iso,
        seal: cfg.seal,
        kernel_clamp_world: cfg.kernel_clamp_world,
        kernel_clamp_ref,
        density_norm_ref: cfg.density_norm_ref,
        splat_integrate: cfg.splat_integrate,
        quant16: cfg.quant16,
        quant_scale: cfg.quant_scale,
        threads,
        ..VoxelizeOptions::default()
    };
    // --cross-resolution <ref>：一键统一口径——核宽钉在参考网格，iso 也用参考网格的绝对阈值。
    if let Some(r) = cfg.cross_resolution {
        if voxel_opts.kernel_clamp_ref.is_none() && voxel_opts.kernel_clamp_world.is_none() {
            voxel_opts.kernel_clamp_ref = Some(r);
        }
        if voxel_opts.iso_abs.is_none() {
            let mut probe = voxel_opts.clone();
            probe.n = r;
            probe.backend = Backend::Cpu;
            probe.bins3d = false;
            probe.seal = false;
            let iso = voxelize(&kept, &probe)
                .map_err(|e| format!("--cross-resolution {r} 的参考 iso 探针失败：{e}"))?
                .iso;
            voxel_opts.iso_abs = Some(iso);
            let inner_cur = (cfg.grid as f32 - 1.0 - 2.0 * cfg.padding as f32).max(1.0);
            let inner_ref = (r as f32 - 1.0 - 2.0 * cfg.padding as f32).max(1.0);
            let ratio = inner_cur / inner_ref;
            let mut note = format!(
                "--cross-resolution {r}：核宽钉在 {r}³ 世界尺度，iso={iso:.5}（{r}³ 探针，所有 grid 复用）"
            );
            if ratio < 0.667 {
                note.push_str(&format!(
                    "；⚠ 本 grid 的核宽下限只有 {:.2} 格，<0.5 格会采样成空场（64³ 实测塌到 0.014），建议 --grid ≥ {r}",
                    0.75 * ratio
                ));
            }
            clamp_default_note = Some(match clamp_default_note {
                Some(mut s) => {
                    s.push('；');
                    s.push_str(&note);
                    s
                }
                None => note,
            });
        }
    }
    let mut fallback = None;
    let mut sparse_note = None;
    let mut acc_vox = 0.0;
    let mut acc_mesh = 0.0;
    let (mut field, mut mesh) = if sparse {
        let (f, m, v, mm, note) = run_sparse_leg(&kept, &voxel_opts)?;
        acc_vox = v;
        acc_mesh = mm;
        sparse_note = note;
        (f, m)
    } else {
        let mut result = run_leg(&kept, &voxel_opts, gpu_ref);
        if let Ok((_, _, v, m)) = &result {
            acc_vox += *v;
            acc_mesh += *m;
        }
        if cfg.gpu && gpu_device.is_some() {
            let bad = match &result {
                Err(_) => true,
                Ok((_, m, _, _)) => m.stats.triangles == 0 || m.stats.volume <= 0.0,
            };
            if bad {
                let reason = match &result {
                    Err(e) => e.clone(),
                    Ok(_) => "GPU 输出空网格".to_string(),
                };
                if cfg.grid > 256 && result.is_err() {
                    return Err(format!(
                        "512³ GPU 失败（{reason}）；去掉 --gpu 可走稀疏 CPU"
                    ));
                }
                voxel_opts.backend = Backend::Cpu;
                result = run_leg(&kept, &voxel_opts, gpu_ref);
                if let Ok((_, _, v, m)) = &result {
                    acc_vox += *v;
                    acc_mesh += *m;
                }
                gpu_fallback = Some(format!("GPU 失败（{reason}）→ CPU"));
            }
        }
        if cfg.mode == ModeSel::Auto && mode == VoxelMode::Surfel {
            let bad = match &result {
                Err(_) => true,
                Ok((_, m, _, _)) => m.stats.triangles == 0 || m.stats.volume <= 0.0,
            };
            if bad {
                let reason = match &result {
                    Err(e) => e.clone(),
                    Ok(_) => "surfel 输出空网格".to_string(),
                };
                voxel_opts.mode = VoxelMode::Density;
                mode = VoxelMode::Density;
                result = run_leg(&kept, &voxel_opts, gpu_ref);
                if let Ok((_, _, v, m)) = &result {
                    acc_vox += *v;
                    acc_mesh += *m;
                }
                fallback = Some(format!("surfel 失败（{reason}）→ density"));
            }
        }
        let (f, m, _, _) = result?;
        (f, m)
    };
    timings.voxel = acc_vox;
    timings.mesh = acc_mesh;
    if let Some(note) = &clamp_default_note {
        auto_reason.push('；');
        auto_reason.push_str(note);
    }
    if let Some(note) = &sparse_note {
        auto_reason.push('；');
        auto_reason.push_str(note);
    }
    if mesh.stats.triangles == 0 {
        return Err(format!(
            "没有生成三角形（mode={}，grid={}）。试：提高 --grid、降低 --iso、或 --mode density",
            mode.name(),
            cfg.grid
        ));
    }
    // 密度场的全局阈值在稀疏处会破壳：auto 模式下检测到 χ<0 且无封闭内部时，
    // 自动降阈值重试一次/两次；显式 --iso 不覆盖用户选择。
    let mut iso_retry = None;
    if !sparse && cfg.mode == ModeSel::Auto && mode == VoxelMode::Density && cfg.iso.is_none() {
        let suspicious = |f: &Field, m: &Mesh| {
            m.stats.components > 8 || (f.sealed_interior == 0 && m.stats.chi < 0)
        };
        if suspicious(&field, &mesh) {
            let base_frac = voxel_opts.iso_frac;
            for factor in [0.75f32, 0.5] {
                let mut o = voxel_opts.clone();
                o.iso_frac = base_frac * factor;
                if let Ok((f2, m2, v2, mm2)) = run_leg(&kept, &o, gpu_ref) {
                    acc_vox += v2;
                    acc_mesh += mm2;
                    if m2.stats.triangles > 0 && f2.sealed_interior > 0 && m2.stats.chi >= 2 {
                        field = f2;
                        mesh = m2;
                        iso_retry = Some(format!("iso ×{factor:.2}（原阈值破壳）"));
                        break;
                    }
                }
            }
            timings.voxel = acc_vox;
            timings.mesh = acc_mesh;
        }
    }
    // 只对最终选中的场重做一次网格；重试期间仍用 CPU 网格做质量判据。
    let mut mesher_gpu = false;
    let mut mesh_fallback = None;
    if !sparse && cfg.gpu_mesh {
        if let Some(hd) = gpu_ref {
            let t = Instant::now();
            match field_to_mesh_gpu(hd, &field) {
                Ok(m) => {
                    timings.mesh = t.elapsed().as_secs_f64() * 1e3;
                    mesh = m;
                    mesher_gpu = true;
                }
                Err(e) => mesh_fallback = Some(format!("GPU 网格失败（{e}）→ CPU Surface Nets")),
            }
        } else {
            mesh_fallback = Some("--gpu-mesh 无可用 GPU → CPU Surface Nets".into());
        }
    }
    let mut component_note = None;
    if cfg.largest_component && mesh.stats.components > 1 {
        let before_comps = mesh.stats.components;
        let before_tris = mesh.stats.triangles;
        let t = Instant::now();
        mesh = keep_largest_component(&mesh, &field.grid);
        timings.mesh += t.elapsed().as_secs_f64() * 1e3;
        component_note = Some(format!(
            "只保留最大连通分量：{}→{} 分量，{}→{} 三角形",
            before_comps,
            mesh.stats.components,
            fmt_int(before_tris),
            fmt_int(mesh.stats.triangles)
        ));
    }
    if !cfg.quiet {
        let mut exec = match field.backend {
            Backend::Gpu => {
                if timings.gpu_init >= 0.5 {
                    format!("gpu (init {:.0} ms)", timings.gpu_init)
                } else {
                    "gpu (cached)".to_string()
                }
            }
            Backend::Cpu => format!(
                "cpu/{}t{}",
                field.workers,
                if field.bins3d { " +3dtile" } else { "" }
            ),
        };
        if sparse {
            exec.push_str(" +sparse");
        }
        if field.simd_prep {
            exec.push_str(" +simdprep");
        }
        stage(
            3,
            5,
            "体素化",
            &format!(
                "{} · {} · {}³ · h={:.5} · iso={:.5} · 观测 {} · 封闭内部 {}{} · {:.1} ms · {}",
                field.mode.name(),
                exec,
                cfg.grid,
                field.grid.h,
                field.iso,
                fmt_int(field.observed),
                fmt_int(field.sealed_interior),
                if sparse { "（活跃块内）" } else { "" },
                timings.voxel,
                auto_reason
            ),
        );
        stage(
            4,
            5,
            "网格",
            &format!(
                "Surface Nets · {} · {} 顶点 · {} 三角形 · χ={} · {} 分量 · {:.1} ms",
                if mesher_gpu { "gpu" } else { "cpu" },
                fmt_int(mesh.stats.vertices),
                fmt_int(mesh.stats.triangles),
                mesh.stats.chi,
                mesh.stats.components,
                timings.mesh
            ),
        );
    }

    let mut warnings = Vec::new();
    if let Some(g) = &gpu_fallback {
        warnings.push(g.clone());
    }
    if let Some(f) = &fallback {
        warnings.push(f.clone());
    }
    if let Some(r) = &iso_retry {
        warnings.push(format!("密度阈值自动重试：{r}，已选更闭合的网格"));
    }
    if let Some(m) = &mesh_fallback {
        warnings.push(m.clone());
    }
    if let Some(n) = &component_note {
        warnings.push(n.clone());
    }
    if mesher_gpu {
        warnings.push(
            "GPU 网格为实验档：热态仍慢于 CPU（64³ 13–15 ms vs 4 ms；128³ 96–183 ms vs 21 ms），首跑还含管线编译"
                .into(),
        );
    }
    if field.backend == Backend::Gpu && cfg.bins3d {
        warnings.push("--gpu 忽略 --bins3d（GPU 用自己的 job 表）".into());
    }
    if mesh.stats.boundary_touch {
        warnings.push("网格触及域边界（可能被截断）：加大 --grid 或 --padding".into());
    }
    if mesh.stats.components > 1 {
        warnings.push(format!(
            "{} 个连通分量：floaters、开放面、真实多物体，或 --hollow 的内外双面；提高 --prune-opacity 可减碎片",
            mesh.stats.components
        ));
    }
    if cfg.seal && field.sealed_interior == 0 && mesh.stats.chi < 0 {
        warnings.push(
            "封闭填充为 0 且 χ<0：场可能破壳/有洞；试降低 --iso、提高 --grid 或改用 --mode surfel"
                .into(),
        );
    } else if mode == VoxelMode::Surfel && cfg.seal && field.sealed_interior == 0 {
        warnings.push("封闭填充为 0：surfel 面可能开放/有洞（需要闭合表面）".into());
    }
    if mesh.stats.volume <= 0.0 {
        warnings.push("网格体积为 0：输出可能是开放面".into());
    }
    for w in &warnings {
        eprintln!("⚠ {w}");
    }

    let t = Instant::now();
    if let Some(parent) = obj.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
    }
    write_obj(&mesh, obj).map_err(|e| format!("写 OBJ 失败：{e}"))?;
    let mut outputs = vec![obj.to_path_buf()];
    let mut data = ReportData {
        title,
        subject: &loaded.subject,
        format: loaded.format,
        mode_requested: cfg.mode.name(),
        mode_used: mode.name(),
        auto_reason: &auto_reason,
        fallback: fallback.as_deref(),
        gpu_fallback: gpu_fallback.as_deref(),
        iso_retry: iso_retry.as_deref(),
        timings,
        prune_stats,
        field: &field,
        mesh: &mesh,
        warnings: &warnings,
        has_normals: loaded.has_normals,
        flatness,
        hollow: !cfg.seal,
        sparse,
        mesher: if mesher_gpu { "gpu" } else { "cpu" },
    };
    if let Some(path) = html {
        if mesh.stats.vertices > 250_000 {
            eprintln!(
                "⚠ 网格顶点 {} 过大，跳过默认 HTML 预览（显式 --html 仍会写）",
                fmt_int(mesh.stats.vertices)
            );
        } else {
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
            }
            write_html(path, &data, &kept).map_err(|e| format!("写 HTML 失败：{e}"))?;
            outputs.push(path.to_path_buf());
        }
    }
    timings.write = t.elapsed().as_secs_f64() * 1e3;
    data.timings = timings;
    if let Some(path) = json {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
        }
        fs::write(path, report_json(&data)).map_err(|e| format!("写 JSON 失败：{e}"))?;
        outputs.push(path.to_path_buf());
    }
    timings.write = t.elapsed().as_secs_f64() * 1e3;
    if !cfg.quiet {
        stage(
            5,
            5,
            "写出",
            &format!(
                "{} · {:.1} ms",
                outputs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" · "),
                timings.write
            ),
        );
    }
    println!(
        "\n体积 {:.5} · χ={} · 分量 {} · {}³ · {} · {:.1} ms 总计",
        mesh.stats.volume,
        mesh.stats.chi,
        mesh.stats.components,
        field.grid.n,
        mode.name(),
        timings.load
            + timings.prune
            + timings.voxel
            + timings.mesh
            + timings.write
            + timings.gpu_init
    );
    if let Some((name, v)) = reference {
        let rel = ((mesh.stats.volume - v) / v).abs() * 100.0;
        if cfg.seal {
            println!("解析{name}体积 {v:.5} · 相对误差 {rel:.2}%");
        } else {
            println!("解析{name}体积 {v:.5} · 相对误差 {rel:.2}%（--hollow 壳体，参考体积不适用）");
        }
    }
    println!(
        "哈希 {:016x} · 导出 {}",
        mesh.stats.hash,
        outputs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" · ")
    );
    Ok(())
}

fn run_leg(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
    gpu: Option<&Headless>,
) -> Result<(Field, Mesh, f64, f64), String> {
    let t = Instant::now();
    let field = if opts.quant16 {
        // 显式 u16 定点路径：与 f32 同面（IoU 0.996–1.000），1.1×，内存 1/2
        let (f, _scale) = conv_core::gauss::voxel::quantize_field_auto(gaussians, opts)?;
        f
    } else {
        match (opts.backend, gpu) {
            (Backend::Gpu, Some(hd)) => voxelize_gpu_with(hd, gaussians, opts)?,
            _ => voxelize(gaussians, opts)?,
        }
    };
    let voxel_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let mesh = field_to_mesh(&field);
    let mesh_ms = t.elapsed().as_secs_f64() * 1e3;
    Ok((field, mesh, voxel_ms, mesh_ms))
}

/// 核宽/密度是否已钉在世界单位（此时密度近似分辨率无关，探针 iso 不再按 n⁻² 换算）。
fn iso_pinned(opts: &VoxelizeOptions) -> bool {
    opts.kernel_clamp_ref.is_some()
        || opts.kernel_clamp_world.is_some()
        || opts.density_norm_ref.is_some()
        || opts.splat_integrate
}

/// \>256³ 的 density iso：显式 `--iso` 直接用；否则跑半分辨率探针。
/// 默认核宽随 h 漂移，实测 iso ∝ n⁻²（scene01：128→117.52、256→29.386、512→7.469，
/// 比值 4.00 / 3.94），探针 iso 必须按 (probe_n/n)² 换算；但核宽/密度钉住后密度已
/// 近似分辨率无关，再乘会把 512 的阈值压低 4 倍。
fn resolve_sparse_iso(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
) -> Result<(f32, f64, Option<String>), String> {
    if let Some(v) = opts.iso_abs {
        return Ok((v, 0.0, None));
    }
    let probe_n = (opts.n / 2).clamp(8, 256);
    let mut probe = opts.clone();
    probe.n = probe_n;
    probe.backend = Backend::Cpu;
    probe.bins3d = false;
    probe.seal = false;
    let t = Instant::now();
    let iso = voxelize(gaussians, &probe)
        .map_err(|e| format!(">256³ density 自动 iso 失败（{probe_n}³ 探针）：{e}"))?
        .iso;
    let probe_ms = t.elapsed().as_secs_f64() * 1e3;
    let pinned = iso_pinned(opts) || opts.splat_integrate;
    let k = if pinned {
        1.0
    } else {
        (probe_n as f32 / opts.n as f32).powi(2)
    };
    let iso = iso * k;
    let note = if pinned {
        format!("auto iso {iso:.8}（{probe_n}³ 探针，核宽/密度已钉在世界单位 ⇒ 不换算分辨率）")
    } else {
        format!(
            "auto iso {iso:.4}（{probe_n}³ 探针 ×{k:.3} 分辨率换算；                         p99.9 本身随分辨率漂移，512³ 体积约为 256³ 的 1/2，需同面请显式 --iso）"
        )
    };
    Ok((iso, probe_ms, Some(note)))
}

fn run_sparse_leg(
    gaussians: &[Gaussian],
    opts: &VoxelizeOptions,
) -> Result<(Field, Mesh, f64, f64, Option<String>), String> {
    let bounds = opts.aabb.unwrap_or_else(|| bounds_of(gaussians));
    let grid = Grid::fit(bounds, opts.n, opts.padding)?;
    let mut probe_ms = 0.0f64;
    let mut iso_note = None;
    let iso_fixed = if opts.mode == VoxelMode::Density {
        let (iso, ms, note) = resolve_sparse_iso(gaussians, opts)?;
        probe_ms = ms;
        iso_note = note;
        Some(iso)
    } else {
        None
    };
    // 候选块检测内部会跑一遍粗分辨率 density；若核宽已钉住但 iso 还是 auto，粗场会
    // 用自己的（随分辨率漂移的）auto-iso，导致 auto 与显式 --iso 选出不同的块集。
    // 钉住时把已解析的 iso 交给检测，两条路径才一致；默认路径不动。
    let mut det_opts = opts.clone();
    if let (Some(iso), true, true) = (iso_fixed, iso_pinned(opts), opts.iso_abs.is_none()) {
        det_opts.iso_abs = Some(iso);
    }
    let t = Instant::now();
    let blocks = SurfaceBlocks::detect(gaussians, &det_opts, grid, SPARSE_BLOCK, 0, 4)
        .map_err(|e| format!("稀疏候选块失败：{e}"))?;
    let detect_ms = t.elapsed().as_secs_f64() * 1e3;
    let build_ms;
    let mesh_ms;
    let (mesh, observed, sealed, iso) = match opts.mode {
        VoxelMode::Surfel => {
            let t = Instant::now();
            let s = SparseSurfel::build(gaussians, opts, grid, &blocks, opts.seal)?;
            build_ms = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let m = s.mesh()?;
            mesh_ms = t.elapsed().as_secs_f64() * 1e3;
            (m, s.observed, s.sealed_interior, 0.0)
        }
        VoxelMode::Density => {
            let iso = iso_fixed.ok_or("density 路径缺少 iso")?;
            let t = Instant::now();
            let s = SparseDensity::build(gaussians, opts, grid, &blocks, iso, opts.seal)?;
            build_ms = t.elapsed().as_secs_f64() * 1e3;
            let t = Instant::now();
            let m = s.mesh()?;
            mesh_ms = t.elapsed().as_secs_f64() * 1e3;
            (m, s.observed, s.sealed_interior, s.iso)
        }
    };
    let field = Field {
        values: Vec::new(),
        grid,
        mode: opts.mode,
        backend: Backend::Cpu,
        bins3d: false,
        simd_prep: opts.simd_prep,
        iso,
        observed,
        sealed_interior: sealed,
        workers: opts.threads,
        bounds,
    };
    let mut note = format!(
        "稀疏分块 {:.1}%（{} 块，B={SPARSE_BLOCK}）",
        blocks.fraction() * 100.0,
        blocks.active_count()
    );
    if let Some(iso) = iso_note {
        note = format!("{iso}；{note}");
    }
    Ok((
        field,
        mesh,
        detect_ms + probe_ms + build_ms,
        mesh_ms,
        Some(note),
    ))
}

fn choose_mode(sel: ModeSel, has_normals: bool, flatness: f32) -> (VoxelMode, String) {
    match sel {
        ModeSel::Density => (VoxelMode::Density, "显式 density".into()),
        ModeSel::Surfel => (
            VoxelMode::Surfel,
            if has_normals {
                "显式 surfel".into()
            } else {
                format!(
                    "显式 surfel（无法线：最薄轴 + 质心定向，可能翻面；各向异性中位数 {flatness:.2}）"
                )
            },
        ),
        ModeSel::Auto => {
            if has_normals {
                (VoxelMode::Surfel, "auto→surfel：资产带法线".into())
            } else {
                (
                    VoxelMode::Density,
                    format!(
                        "auto→density：无法线（各向异性中位数 {flatness:.2}）；surfel 需显式 --mode surfel"
                    ),
                )
            }
        }
    }
}

fn run_inspect(cfg: &Config, input: &Path) -> Result<(), String> {
    let bytes = fs::read(input).map_err(|e| format!("读不了 {}：{e}", input.display()))?;
    let data = parse_ply_with(
        &bytes,
        &PlyReadOptions {
            scales_are_log: !cfg.raw_scale,
            opacity_is_logit: !cfg.raw_opacity,
        },
    )?;
    let b = bounds_of(&data.gaussians);
    let mut radii: Vec<f32> = data
        .gaussians
        .iter()
        .map(|g| g.axis_radius().into_iter().fold(0f32, f32::max))
        .collect();
    let mut ops: Vec<f32> = data.gaussians.iter().map(|g| g.opacity).collect();
    radii.sort_by(f32::total_cmp);
    ops.sort_by(f32::total_cmp);
    let flat = median_flatness(&data.gaussians);
    let (_, stats) = prune(
        &data.gaussians,
        &PruneOptions {
            min_opacity: cfg.prune_opacity,
            min_radius: cfg.min_radius,
            max_radius: cfg.max_radius,
        },
    );
    let (suggest, why) = choose_mode(cfg.mode, data.has_normals, flat);
    println!("格式        {}", data.format);
    println!(
        "高斯        {}（{} KB）",
        fmt_int(data.gaussians.len()),
        bytes.len() / 1024
    );
    println!("法线        {}", if data.has_normals { "有" } else { "无" });
    println!(
        "包围盒      min=({:.4}, {:.4}, {:.4})  max=({:.4}, {:.4}, {:.4})",
        b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]
    );
    let e = b.extent();
    println!("尺寸        ({:.4}, {:.4}, {:.4})", e[0], e[1], e[2]);
    println!(
        "3σ 半径     p50={:.5}  p95={:.5}  max={:.5}",
        pct(&radii, 0.50),
        pct(&radii, 0.95),
        pct(&radii, 1.0)
    );
    println!(
        "不透明度    p50={:.3}  p95={:.3}  min={:.3}",
        pct(&ops, 0.50),
        pct(&ops, 0.95),
        pct(&ops, 0.0)
    );
    println!("各向异性    p50（中间/最薄）= {flat:.2}");
    println!(
        "剪枝预演    {} 保留 / {} 丢（阈值 opacity≥{:.3}）",
        fmt_int(stats.kept),
        fmt_int(stats.input - stats.kept),
        cfg.prune_opacity
    );
    println!("auto 建议   {}（{}）", suggest.name(), why);
    if let Some(path) = &cfg.json {
        let mut s = String::new();
        let _ = write!(
            s,
            "{{\n  \"subject\": {},\n  \"format\": {},\n  \"gaussians\": {},\n  \"has_normals\": {},\n  \"flatness_p50\": {:.5},\n  \"radius_p50\": {:.6},\n  \"radius_p95\": {:.6},\n  \"opacity_p50\": {:.5},\n  \"prune_kept\": {},\n  \"suggest\": {}\n}}\n",
            jstr(&input.display().to_string()),
            jstr(data.format),
            data.gaussians.len(),
            data.has_normals,
            flat,
            pct(&radii, 0.50),
            pct(&radii, 0.95),
            pct(&ops, 0.50),
            stats.kept,
            jstr(suggest.name())
        );
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|e| format!("建目录失败：{e}"))?;
        }
        fs::write(path, s).map_err(|e| format!("写 JSON 失败：{e}"))?;
        println!("报告        {}", path.display());
    }
    Ok(())
}

fn pct(v: &[f32], q: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let i = ((v.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    v[i.min(v.len() - 1)]
}

fn stage(i: u32, n: u32, label: &str, detail: &str) {
    println!("[{i}/{n}] {label:<6} {detail}");
}

fn fmt_int(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
