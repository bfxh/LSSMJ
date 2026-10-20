//! 自包含 HTML 的二进制载荷判据：base64 长度与 JSON 统计一致，首顶点与 OBJ 对齐。

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a') as u32 + 26,
            b'0'..=b'9' => (c - b'0') as u32 + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for c in s.as_bytes().chunks(4) {
        let n = (val(c[0]) << 18) | (val(c[1]) << 12) | (val(c[2]) << 6) | val(c[3]);
        out.push((n >> 16) as u8);
        if c[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if c[3] != b'=' {
            out.push(n as u8);
        }
    }
    out
}

fn field<'a>(section: &'a str, key: &str) -> &'a str {
    let pat = format!("\"{key}\":\"");
    let start = section.find(&pat).expect("field start") + pat.len();
    let end = start + section[start..].find('"').expect("field end");
    &section[start..end]
}

fn count(section: &str, key: &str) -> usize {
    let pat = format!("\"{key}\":");
    let start = section.find(&pat).expect("count start") + pat.len();
    let digits: String = section[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().expect("count digits")
}

#[test]
fn html_binary_payload_matches_stats() {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("gauss2mesh_html_{}_{}", std::process::id(), stamp));
    fs::create_dir_all(&dir).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_gauss2mesh"))
        .arg("demo")
        .arg("sphere")
        .arg("--grid")
        .arg("32")
        .arg("--threads")
        .arg("1")
        .arg("--quiet")
        .arg("--out-dir")
        .arg(&dir)
        .status()
        .unwrap();
    assert!(status.success(), "gauss2mesh demo failed");

    let html = fs::read_to_string(dir.join("sphere.html")).unwrap();
    let mesh = &html[html.find("\"mesh\":{").expect("mesh section")..];
    let mesh = &mesh[..mesh.find("\"points\":{").expect("points marker")];
    let points = &html[html.find("\"points\":{").expect("points section")..];
    let pos = b64_decode(field(mesh, "pos"));
    let nrm = b64_decode(field(mesh, "nrm"));
    let idx = b64_decode(field(mesh, "idx"));
    let ppos = b64_decode(field(points, "pos"));
    let pcol = b64_decode(field(points, "col"));
    let verts = count(mesh, "vertices");
    let tris = count(mesh, "triangles");
    let npoints = count(points, "count");

    assert_eq!(pos.len(), verts * 12, "mesh positions bytes");
    assert_eq!(nrm.len(), verts * 12, "mesh normals bytes");
    assert_eq!(idx.len(), tris * 12, "mesh indices bytes");
    assert_eq!(ppos.len(), npoints * 12, "point positions bytes");
    assert_eq!(pcol.len(), npoints * 12, "point colors bytes");
    assert!(!html.contains("\"positions\":["), "old JSON array format");
    assert!(!html.contains("\"indices\":["), "old JSON array format");

    let first = [0usize, 4, 8].map(|o| f32::from_le_bytes(pos[o..o + 4].try_into().unwrap()));
    let obj = fs::read_to_string(dir.join("sphere.obj")).unwrap();
    let line = obj
        .lines()
        .find(|l| l.starts_with("v "))
        .expect("obj vertex");
    let obj_pos: Vec<f32> = line
        .split_whitespace()
        .skip(1)
        .map(|v| v.parse().unwrap())
        .collect();
    for i in 0..3 {
        assert!(
            (first[i] - obj_pos[i]).abs() < 1e-5,
            "first vertex {first:?} vs obj {obj_pos:?}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
