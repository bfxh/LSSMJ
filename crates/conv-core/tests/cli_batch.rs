//! CLI 批处理判据：一次进程跑多文件、坏文件不阻断好文件、最终非零退出。

use conv_core::gauss::sphere_gaussians;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const NAMES: [&str; 17] = [
    "x", "y", "z", "nx", "ny", "nz", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1",
    "scale_2", "rot_0", "rot_1", "rot_2", "rot_3",
];

fn write_ply(path: &Path, count: usize) {
    let gs = sphere_gaussians(count, 0.75, 0.99);
    let mut header = format!(
        "ply\nformat binary_little_endian 1.0\nelement vertex {}\n",
        gs.len()
    );
    for n in NAMES {
        header.push_str(&format!("property float {n}\n"));
    }
    header.push_str("end_header\n");
    let mut bytes = header.into_bytes();
    for g in &gs {
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

#[test]
fn batch_continues_after_bad_input_and_exits_nonzero() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("gauss2mesh_batch_{}_{}", std::process::id(), stamp));
    let out = dir.join("out");
    fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.ply");
    let b = dir.join("b.ply");
    let missing = dir.join("missing.ply");
    write_ply(&a, 400);
    write_ply(&b, 600);

    let status = Command::new(env!("CARGO_BIN_EXE_gauss2mesh"))
        .arg("batch")
        .arg(&a)
        .arg(&missing)
        .arg(&b)
        .arg("--out-dir")
        .arg(&out)
        .arg("--grid")
        .arg("32")
        .arg("--no-html")
        .arg("--quiet")
        .status()
        .unwrap();
    assert!(
        !status.success(),
        "batch with a bad input must exit nonzero"
    );
    for name in ["a", "b"] {
        let obj = out.join(format!("{name}.obj"));
        let json = out.join(format!("{name}.json"));
        assert!(obj.exists(), "missing {name}.obj");
        assert!(json.exists(), "missing {name}.json");
        assert!(fs::metadata(&obj).unwrap().len() > 0);
        let text = fs::read_to_string(&json).unwrap();
        assert!(text.contains("\"backend\": \"cpu\""), "backend cpu");
        assert!(text.contains("\"triangles\":"), "triangles field");
        assert!(!text.contains("\"triangles\": 0"), "empty mesh");
    }
    assert!(!out.join("missing.obj").exists());
    let _ = fs::remove_dir_all(&dir);
}
