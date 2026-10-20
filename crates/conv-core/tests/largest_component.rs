//! `--largest-component` 判据：两个分离球的场 → 默认 2 分量，过滤后 1 分量且体积约半。

use conv_core::gauss::sphere_gaussians;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const NAMES: [&str; 17] = [
    "x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1",
    "scale_2", "rot_0", "rot_1", "rot_2", "rot_3",
];

fn write_ply(path: &Path, gs: &[conv_core::gauss::Gaussian]) {
    let mut header = format!(
        "ply\nformat binary_little_endian 1.0\nelement vertex {}\n",
        gs.len()
    );
    for n in NAMES {
        header.push_str(&format!("property float {n}\n"));
    }
    header.push_str("end_header\n");
    let mut bytes = header.into_bytes();
    for g in gs {
        let mut row = Vec::with_capacity(17 * 4);
        let push = |row: &mut Vec<u8>, v: f32| row.extend_from_slice(&v.to_le_bytes());
        push(&mut row, g.mean[0]);
        push(&mut row, g.mean[1]);
        push(&mut row, g.mean[2]);
        let n = g.normal.unwrap_or([0.0, 0.0, 0.0]);
        push(&mut row, n[0]);
        push(&mut row, n[1]);
        push(&mut row, n[2]);
        for c in g.color {
            push(&mut row, (c - 0.5) / 0.282_094_8);
        }
        push(&mut row, (g.opacity / (1.0 - g.opacity)).ln());
        for s in g.scale {
            push(&mut row, s.ln());
        }
        for q in g.rot {
            push(&mut row, q);
        }
        bytes.extend_from_slice(&row);
    }
    fs::write(path, bytes).unwrap();
}

fn num(json: &str, key: &str) -> f64 {
    let pat = format!("\"{key}\":");
    let start = json.find(&pat).expect("field") + pat.len();
    let rest = json[start..].trim_start();
    let end = rest.find([',', '}']).expect("field end");
    rest[..end].trim().parse().expect("number")
}

fn run(bin: &Path, ply: &Path, out: &Path, filtered: bool) -> String {
    let json = out.join(if filtered {
        "filtered.json"
    } else {
        "base.json"
    });
    let obj = out.join(if filtered { "filtered.obj" } else { "base.obj" });
    let mut cmd = Command::new(bin);
    cmd.arg("convert")
        .arg(ply)
        .arg("--grid")
        .arg("64")
        .arg("--mode")
        .arg("density")
        .arg("--prune-opacity")
        .arg("0")
        .arg("--no-html")
        .arg("--quiet")
        .arg("--out")
        .arg(&obj)
        .arg("--json")
        .arg(&json);
    if filtered {
        cmd.arg("--largest-component");
    }
    let status = cmd.status().unwrap();
    assert!(status.success(), "convert failed (filtered={filtered})");
    fs::read_to_string(&json).unwrap()
}

#[test]
fn largest_component_keeps_one_of_two_spheres() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("gauss2mesh_lc_{}_{}", std::process::id(), stamp));
    fs::create_dir_all(&dir).unwrap();
    let mut gs = sphere_gaussians(2_000, 1.0, 0.99);
    let mut right = gs.clone();
    for g in &mut gs {
        g.mean[0] -= 1.6;
    }
    for g in &mut right {
        g.mean[0] += 1.6;
    }
    gs.extend(right);
    let ply = dir.join("two.ply");
    write_ply(&ply, &gs);

    let base = run(
        Path::new(env!("CARGO_BIN_EXE_gauss2mesh")),
        &ply,
        &dir,
        false,
    );
    let filtered = run(
        Path::new(env!("CARGO_BIN_EXE_gauss2mesh")),
        &ply,
        &dir,
        true,
    );
    let base_c = num(&base, "components");
    let filt_c = num(&filtered, "components");
    let base_t = num(&base, "triangles");
    let filt_t = num(&filtered, "triangles");
    let ratio = num(&filtered, "volume") / num(&base, "volume");
    println!(
        "components {base_c}->{filt_c}, triangles {base_t}->{filt_t}, volume ratio {ratio:.3}"
    );
    assert!(base_c >= 2.0, "expected >=2 components, got {base_c}");
    assert_eq!(filt_c, 1.0, "filtered components");
    assert!(filt_t < base_t, "filtered triangles");
    assert!((0.3..0.7).contains(&ratio), "volume ratio {ratio}");
    let _ = fs::remove_dir_all(&dir);
}
