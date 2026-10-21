//! CLI 512³ 稀疏分块判据（ignored 诊断）：>256 自动走稀疏；density 必须给 --iso。
//! 必须用 release 跑（debug 下 512³ 会慢到不可用）：
//! `cargo test --release --test cli_sparse -- --ignored --nocapture`

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("gauss2mesh_{tag}_{}_{}", std::process::id(), stamp));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_gauss2mesh"))
        .args(args)
        .output()
        .unwrap()
}

fn json_field<'a>(json: &'a str, key: &str) -> &'a str {
    let pat = format!("\"{key}\": ");
    let start = json.find(&pat).unwrap_or_else(|| panic!("missing {key}")) + pat.len();
    let rest = &json[start..];
    let end = rest.find([',', '\n', '}']).unwrap_or(rest.len());
    rest[..end].trim()
}

#[test]
#[ignore = "diagnostic CLI 512 sparse E2E (release only)"]
fn cli_512_sparse_sphere() {
    let dir = temp_dir("cli512");
    let obj = dir.join("sphere.obj");
    let json = dir.join("sphere.json");
    let out = run(&[
        "demo",
        "sphere",
        "--grid",
        "512",
        "--no-html",
        "--out",
        obj.to_str().unwrap(),
        "--json",
        json.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&json).unwrap();
    assert_eq!(json_field(&text, "sparse"), "true");
    assert_eq!(json_field(&text, "n"), "512");
    assert_eq!(json_field(&text, "components"), "1");
    assert_eq!(json_field(&text, "chi"), "2");
    let volume: f32 = json_field(&text, "volume").parse().unwrap();
    assert!((volume - 1.767).abs() / 1.767 < 0.01, "volume {volume}");
    let grid_h: f32 = json_field(&text, "h").parse().unwrap();
    assert!(grid_h > 0.0);
    let _ = Path::new(&obj);
    fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "diagnostic CLI 512 density auto-iso (release only)"]
fn cli_512_density_auto_iso() {
    let dir = temp_dir("cli512d");
    let obj = dir.join("blobs.obj");
    let json = dir.join("blobs.json");
    let out = run(&[
        "demo",
        "blobs",
        "--grid",
        "512",
        "--mode",
        "density",
        "--no-html",
        "--out",
        obj.to_str().unwrap(),
        "--json",
        json.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&json).unwrap();
    assert_eq!(json_field(&text, "sparse"), "true");
    assert_eq!(json_field(&text, "n"), "512");
    assert_eq!(json_field(&text, "components"), "1");
    assert_eq!(json_field(&text, "chi"), "2");
    let iso: f32 = json_field(&text, "iso").parse().unwrap();
    assert!(iso > 0.0, "auto iso {iso}");
    let volume: f32 = json_field(&text, "volume").parse().unwrap();
    assert!((2.0..2.1).contains(&volume), "volume {volume}");
    fs::remove_dir_all(&dir).ok();
}
