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
  --grid <N>             体素分辨率 8..256（默认 64）
  --threads <N>          splat 线程数（默认 0=自动；1=单线程参照）
  --gpu                  GPU splat（定点整数原子；失败回退 CPU；忽略 --threads）
  --gpu-mesh             GPU Surface Nets 网格化（实验档：读回开销大，通常慢于 CPU；失败回退）
  --bins3d               CPU splat 强制 3D tile 分桶（默认：n≥64 且 ≤50k 高斯自动开）
  --no-bins3d            关闭自动 3D tile，回到 z-slab
  --simd-prep            可选 SIMD/cyclic eigen prep（默认标量逐位；opt-in 改哈希）
  --mode <auto|density|surfel>（默认 auto）
  --iso <auto|数值>      密度阈值；auto = 0.65 × 正密度 99.9 分位
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
    pub(crate) bins3d_off: bool,
    pub(crate) simd_prep: bool,
    pub(crate) mode: ModeSel,
    pub(crate) iso: Option<f32>,
    pub(crate) prune_opacity: f32,
    pub(crate) min_radius: f32,
    pub(crate) max_radius: f32,
    pub(crate) padding: u32,
    pub(crate) seal: bool,
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
            bins3d_off: false,
            simd_prep: false,
            mode: ModeSel::Auto,
            iso: None,
            prune_opacity: 0.05,
            min_radius: 0.0,
            max_radius: 0.0,
            padding: 3,
            seal: true,
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

/// 小规模 CPU 默认走 3D tile：12–20k 实测快 15–34%，100k 收益降到 3–5%；
/// 超过 50k、单线程或 n<64 时保持 z-slab。显式 `--bins3d`/`--no-bins3d` 优先。
pub(crate) fn use_bins3d(cfg: &Config, grid: u32, threads: usize, kept: usize) -> bool {
    if cfg.bins3d_off {
        return false;
    }
    cfg.bins3d || (threads > 1 && grid >= 64 && kept <= 50_000)
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
            | "gpu"
            | "gpu-mesh"
            | "bins3d"
            | "no-bins3d"
            | "simd-prep"
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
        if !(8..=256).contains(&cfg.grid) {
            return Err("--grid 必须在 8..=256".into());
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
    cfg.gpu = bools.contains("gpu");
    cfg.gpu_mesh = bools.contains("gpu-mesh");
    cfg.bins3d = bools.contains("bins3d");
    cfg.bins3d_off = bools.contains("no-bins3d");
    if cfg.bins3d && cfg.bins3d_off {
        return Err("--bins3d 与 --no-bins3d 不能同时使用".into());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_bins3d_rules() {
        let mut c = Config::default();
        assert!(use_bins3d(&c, 64, 24, 20_000));
        assert!(use_bins3d(&c, 128, 24, 50_000));
        assert!(!use_bins3d(&c, 64, 1, 20_000));
        assert!(!use_bins3d(&c, 32, 24, 20_000));
        assert!(!use_bins3d(&c, 128, 24, 50_001));
        c.bins3d = true;
        assert!(use_bins3d(&c, 32, 1, 100_000));
        c.bins3d = false;
        c.bins3d_off = true;
        assert!(!use_bins3d(&c, 128, 24, 20_000));
    }
}
