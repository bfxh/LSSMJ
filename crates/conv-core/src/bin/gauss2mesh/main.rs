//! `gauss2mesh`：把 3DGS 高斯（或合成场景）体素化成标量场，再用 Surface Nets 提网格。
//!
//! 这个二进制是"转换腿"的用户面：阶段进度、auto 选路、三旋钮、显式回退记账、
//! 自包含 WebGL 预览。库侧 API 在 `conv_core::gauss`。

mod args;
mod html;
mod report;

use args::{Config, HELP, ModeSel, is_bool_flag, parse_config};
use conv_core::gauss::mesh::{Mesh, field_to_mesh, write_obj};
use conv_core::gauss::ply::{PlyReadOptions, parse_ply_with};
use conv_core::gauss::voxel::{
    Backend, Field, VoxelMode, VoxelizeOptions, voxelize, voxelize_gpu_with,
};
use conv_core::gauss::{
    Gaussian, PruneOptions, add_floaters, blob_sphere_gaussians, bounds_of, median_flatness, prune,
    sphere_gaussians, torus_gaussians,
};
use conv_core::jfa::{Headless, try_headless_device};
use html::write_html;
use report::{ReportData, Timings, jstr, report_json};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;

static GPU_CACHE: OnceLock<Headless> = OnceLock::new();

/// 同进程内复用同一 wgpu 设备；batch 的首个文件付 init，其余复用。
fn cached_gpu() -> Result<&'static Headless, String> {
    if let Some(hd) = GPU_CACHE.get() {
        return Ok(hd);
    }
    let hd = try_headless_device()?;
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
    let (mut mode, auto_reason) = choose_mode(cfg.mode, loaded.has_normals, flatness);
    let threads = if cfg.threads == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    } else {
        cfg.threads
    };
    let mut gpu_fallback = None;
    let gpu_device = if cfg.gpu {
        let t = Instant::now();
        match cached_gpu() {
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
    let gpu_ref = gpu_device;
    let mut voxel_opts = VoxelizeOptions {
        mode,
        bins3d: cfg.bins3d,
        backend: if gpu_device.is_some() {
            Backend::Gpu
        } else {
            Backend::Cpu
        },
        n: cfg.grid,
        padding: cfg.padding,
        iso_frac: 0.65,
        iso_abs: cfg.iso,
        seal: cfg.seal,
        threads,
        ..VoxelizeOptions::default()
    };
    let mut result = run_leg(&kept, &voxel_opts, gpu_ref);
    let mut acc_vox = 0.0;
    let mut acc_mesh = 0.0;
    if let Ok((_, _, v, m)) = &result {
        acc_vox += *v;
        acc_mesh += *m;
    }
    if gpu_device.is_some() {
        let bad = match &result {
            Err(_) => true,
            Ok((_, m, _, _)) => m.stats.triangles == 0 || m.stats.volume <= 0.0,
        };
        if bad {
            let reason = match &result {
                Err(e) => e.clone(),
                Ok(_) => "GPU 输出空网格".to_string(),
            };
            voxel_opts.backend = Backend::Cpu;
            result = run_leg(&kept, &voxel_opts, gpu_ref);
            if let Ok((_, _, v, m)) = &result {
                acc_vox += *v;
                acc_mesh += *m;
            }
            gpu_fallback = Some(format!("GPU 失败（{reason}）→ CPU"));
        }
    }
    let mut fallback = None;
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
    let (mut field, mut mesh, _, _) = result?;
    timings.voxel = acc_vox;
    timings.mesh = acc_mesh;
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
    if cfg.mode == ModeSel::Auto && mode == VoxelMode::Density && cfg.iso.is_none() {
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
    if !cfg.quiet {
        let exec = match field.backend {
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
        stage(
            3,
            5,
            "体素化",
            &format!(
                "{} · {} · {}³ · h={:.5} · iso={:.5} · 观测 {} · 封闭内部 {} · {:.1} ms · {}",
                field.mode.name(),
                exec,
                cfg.grid,
                field.grid.h,
                field.iso,
                fmt_int(field.observed),
                fmt_int(field.sealed_interior),
                timings.voxel,
                auto_reason
            ),
        );
        stage(
            4,
            5,
            "网格",
            &format!(
                "Surface Nets · {} 顶点 · {} 三角形 · χ={} · {} 分量 · {:.1} ms",
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
    let field = match (opts.backend, gpu) {
        (Backend::Gpu, Some(hd)) => voxelize_gpu_with(hd, gaussians, opts)?,
        _ => voxelize(gaussians, opts)?,
    };
    let voxel_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let mesh = field_to_mesh(&field);
    let mesh_ms = t.elapsed().as_secs_f64() * 1e3;
    Ok((field, mesh, voxel_ms, mesh_ms))
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
