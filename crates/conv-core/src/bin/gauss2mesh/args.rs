//! gauss2mesh 的参数面：命令/选项/默认值。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub(crate) const HELP: &str = "\
LSSMJ gauss2mesh · 高斯体素 → 网格

用法：
  gauss2mesh demo [sphere|torus|blobs]     合成场景演示（默认 sphere）
  gauss2mesh convert <input.ply>           读 3DGS PLY 并转换
  gauss2mesh batch <input.ply...>          批量转换（同一进程复用一次 GPU 设备）
  gauss2mesh inspect <input.ply>           只读入并报告统计

常用选项：
  --grid <N>             体素分辨率 8..512（默认 64；>256 无 --gpu 走稀疏分块，density 自动半分辨率探针求 iso）
  --threads <N>          splat 线程数（默认 0=自动；1=单线程参照）
  --gpu                  强制 GPU splat（自研 wgpu 内核；512 走 dense GPU，约 3 GB 显存；失败回退 CPU/稀疏）
  --no-gpu               强制 CPU（默认 auto：批量 ≥2 文件或 grid ≥512 走 GPU，其余 CPU）
  --gpu-mesh             GPU Surface Nets（实验档：通常慢于 CPU；>256 需配合 --gpu；失败回退）
  --bins3d               CPU splat 走 3D tile 分桶（默认 z-slab；GPU 忽略）
  --simd-prep            可选 SIMD/cyclic eigen prep（默认标量逐位；opt-in 改哈希）
  --largest-component    只保留最大连通分量（去 floaters/碎片）
  --mode <auto|density|surfel>（默认 auto）
  --iso <auto|数值>      密度阈值；auto = 0.65 × 正密度 99.9 分位
  --kernel-clamp-world <σ>  核标准差下限用世界单位（默认按 0.75 格）
  --kernel-clamp-ref <N>    按参考网格 N 折算核下限（= 0.75 × h_N）：跨 grid 复用同一核宽，
                        等价于把核宽冻结在 N³；opt-in，不改默认行为
  --density-norm <N>    把 splat 权重乘 (h_N/h)²，让密度跨分辨率可比（opt-in，仅 CPU）
                        注意：它只修密度幅度、不修核宽，512³ 拓扑仍不闭合；
                        要跨分辨率同面请用 --kernel-clamp-ref <N> + 显式 --iso
  --quant16              走 u16 定点密度 splat（需 --iso）：同面、内存 1/2
  --quant-scale <S>      指定 u16 定点 scale，跳过标定（批量复用时省一趟 splat）
  --prune-opacity <f>    不透明度下限（默认 0.05）
  --min-radius <f>       3σ 半径下限（默认 0，关）
  --padding <n>          域边界留白格（默认 3；覆盖 3h 采样带宽）
  --hollow               不做未知区封闭（输出壳而不是实心体）
  --raw-scale            PLY scale_* 已是标准差（不取 exp）
  --raw-opacity          PLY opacity 已激活（不做 sigmoid）

输出：
  --out <path>           OBJ 路径
  --html <path>          HTML 预览路径（默认与输入同名 .html）
  --no-html              不写 HTML 预览
  --json <path>          机器可读报告
  --out-dir <dir>        demo/batch 输出目录（默认 out）
  --seed <u64>           blobs 场景种子（默认 7）
  --quiet                只打印最终三行

例：
  cargo run --release --bin gauss2mesh -- demo sphere
  cargo run --release --bin gauss2mesh -- convert scene.ply --grid 128 --mode auto
  cargo run --release --bin gauss2mesh -- batch a.ply b.ply --gpu --out-dir out/batch
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModeSel {
    Auto,
    Density,
    Surfel,
}

impl ModeSel {
    pub(crate) fn name(self) -> &'static str {
        match self {
            ModeSel::Auto => "auto",
            ModeSel::Density => "density",
            ModeSel::Surfel => "surfel",
        }
    }
}

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) grid: u32,
    pub(crate) threads: usize,
    pub(crate) gpu: bool,
    pub(crate) gpu_mesh: bool,
    pub(crate) bins3d: bool,
    pub(crate) simd_prep: bool,
    pub(crate) mode: ModeSel,
    pub(crate) iso: Option<f32>,
    pub(crate) prune_opacity: f32,
    pub(crate) min_radius: f32,
    pub(crate) max_radius: f32,
    pub(crate) padding: u32,
    pub(crate) seal: bool,
    pub(crate) largest_component: bool,
    pub(crate) kernel_clamp_world: Option<f32>,
    pub(crate) kernel_clamp_ref: Option<u32>,
    pub(crate) density_norm_ref: Option<u32>,
    pub(crate) quant16: bool,
    pub(crate) no_gpu: bool,
    pub(crate) gpu_auto: bool,
    pub(crate) quant_scale: Option<f32>,
    pub(crate) seed: u64,
    pub(crate) raw_scale: bool,
    pub(crate) raw_opacity: bool,
    pub(crate) quiet: bool,
    pub(crate) out: Option<PathBuf>,
    pub(crate) out_dir: PathBuf,
    pub(crate) html: Option<PathBuf>,
    pub(crate) no_html: bool,
    pub(crate) json: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            grid: 64,
            threads: 0,
            gpu: false,
            gpu_mesh: false,
            bins3d: false,
            simd_prep: false,
            mode: ModeSel::Auto,
            iso: None,
            prune_opacity: 0.05,
            min_radius: 0.0,
            max_radius: 0.0,
            padding: 3,
            seal: true,
            largest_component: false,
            kernel_clamp_world: None,
            kernel_clamp_ref: None,
            density_norm_ref: None,
            quant16: false,
            no_gpu: false,
            gpu_auto: true,
            quant_scale: None,
            seed: 7,
            raw_scale: false,
            raw_opacity: false,
            quiet: false,
            out: None,
            out_dir: PathBuf::from("out"),
            html: None,
            no_html: false,
            json: None,
        }
    }
}

pub(crate) fn is_bool_flag(name: &str) -> bool {
    matches!(
        name,
        "no-html"
            | "hollow"
            | "raw-scale"
            | "raw-opacity"
            | "quiet"
            | "help"
            | "quant16"
            | "no-gpu"
            | "gpu"
            | "gpu-mesh"
            | "bins3d"
            | "simd-prep"
            | "largest-component"
    )
}

pub(crate) fn parse_config(
    flags: &HashMap<String, String>,
    bools: &HashSet<String>,
) -> Result<Config, String> {
    let known = [
        "grid",
        "threads",
        "mode",
        "iso",
        "quant-scale",
        "kernel-clamp-world",
        "kernel-clamp-ref",
        "density-norm",
        "prune-opacity",
        "min-radius",
        "max-radius",
        "padding",
        "out",
        "out-dir",
        "html",
        "json",
        "seed",
        "scene",
    ];
    if let Some(k) = flags.keys().find(|k| !known.contains(&k.as_str())) {
        return Err(format!("未知选项 --{k}"));
    }
    let mut cfg = Config::default();
    if let Some(v) = flags.get("grid") {
        cfg.grid = v.parse().map_err(|_| format!("--grid 不是整数：{v}"))?;
        if !(8..=512).contains(&cfg.grid) {
            return Err("--grid 必须在 8..=512".into());
        }
    }
    if let Some(v) = flags.get("threads") {
        cfg.threads = v.parse().map_err(|_| format!("--threads 不是整数：{v}"))?;
        if cfg.threads > 256 {
            return Err("--threads 必须在 0..=256".into());
        }
    }
    if let Some(v) = flags.get("mode") {
        cfg.mode = match v.as_str() {
            "auto" => ModeSel::Auto,
            "density" => ModeSel::Density,
            "surfel" => ModeSel::Surfel,
            _ => return Err(format!("--mode 只能是 auto|density|surfel：{v}")),
        };
    }
    if let Some(v) = flags.get("kernel-clamp-ref") {
        let x: u32 = v
            .parse()
            .map_err(|_| format!("--kernel-clamp-ref 不是整数：{v}"))?;
        if x < 8 {
            return Err("--kernel-clamp-ref 必须 ≥ 8".into());
        }
        cfg.kernel_clamp_ref = Some(x);
    }
    if let Some(v) = flags.get("density-norm") {
        let x: u32 = v
            .parse()
            .map_err(|_| format!("--density-norm 不是整数：{v}"))?;
        if x < 8 {
            return Err("--density-norm 必须 ≥ 8".into());
        }
        cfg.density_norm_ref = Some(x);
    }
    if let Some(v) = flags.get("kernel-clamp-world") {
        let x: f32 = v
            .parse()
            .map_err(|_| format!("--kernel-clamp-world 不是数值：{v}"))?;
        if x <= 0.0 || !x.is_finite() {
            return Err("--kernel-clamp-world 必须 > 0".into());
        }
        cfg.kernel_clamp_world = Some(x);
    }
    if let Some(v) = flags.get("quant-scale") {
        let x: f32 = v.parse().map_err(|_| format!("--quant-scale 不是数值：{v}"))?;
        if x <= 0.0 || !x.is_finite() {
            return Err("--quant-scale 必须 > 0".into());
        }
        cfg.quant_scale = Some(x);
    }
    if let Some(v) = flags.get("iso").filter(|v| *v != "auto") {
        let x: f32 = v.parse().map_err(|_| format!("--iso 不是数值：{v}"))?;
        if x <= 0.0 || !x.is_finite() {
            return Err("--iso 必须 > 0".into());
        }
        cfg.iso = Some(x);
    }
    if let Some(v) = flags.get("prune-opacity") {
        cfg.prune_opacity = v
            .parse()
            .map_err(|_| format!("--prune-opacity 不是数值：{v}"))?;
        if !(0.0..=1.0).contains(&cfg.prune_opacity) {
            return Err("--prune-opacity 必须在 0..=1".into());
        }
    }
    if let Some(v) = flags.get("min-radius") {
        cfg.min_radius = v
            .parse()
            .map_err(|_| format!("--min-radius 不是数值：{v}"))?;
    }
    if let Some(v) = flags.get("max-radius") {
        cfg.max_radius = v
            .parse()
            .map_err(|_| format!("--max-radius 不是数值：{v}"))?;
    }
    if let Some(v) = flags.get("padding") {
        cfg.padding = v.parse().map_err(|_| format!("--padding 不是整数：{v}"))?;
        if cfg.padding >= cfg.grid / 2 {
            return Err("--padding 必须小于 grid/2".into());
        }
    }
    if let Some(v) = flags.get("seed") {
        cfg.seed = v.parse().map_err(|_| format!("--seed 不是整数：{v}"))?;
    }
    cfg.seal = !bools.contains("hollow");
    cfg.largest_component = bools.contains("largest-component");
    cfg.quant16 = bools.contains("quant16");
    cfg.no_gpu = bools.contains("no-gpu");
    cfg.gpu = bools.contains("gpu");
    if cfg.gpu && cfg.no_gpu {
        return Err("--gpu 与 --no-gpu 不能同时用".into());
    }
    // 默认 auto：没显式指定就由规模决定（批量/大网格走自研 GPU 内核）
    cfg.gpu_auto = !cfg.gpu && !cfg.no_gpu;
    cfg.gpu_mesh = bools.contains("gpu-mesh");
    cfg.bins3d = bools.contains("bins3d");
    cfg.simd_prep = bools.contains("simd-prep");
    cfg.raw_scale = bools.contains("raw-scale");
    cfg.raw_opacity = bools.contains("raw-opacity");
    cfg.quiet = bools.contains("quiet");
    cfg.no_html = bools.contains("no-html");
    cfg.out = flags.get("out").map(PathBuf::from);
    cfg.out_dir = flags
        .get("out-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("out"));
    cfg.html = flags.get("html").map(PathBuf::from);
    cfg.json = flags.get("json").map(PathBuf::from);
    Ok(cfg)
}
