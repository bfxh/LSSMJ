//! OBJ → UV 展开 → OBJ + `vt`。
//!
//! 用法：`uvunwrap <in.obj> <out.obj>`
//!
//! 读 `v` / `f`（忽略已有 `vt`/`vn`），按 `design::uv` 的“法线主轴分岛 + shelf 打包”
//! 生成 UV，再写回 `v` / `vt` / `f v/vt`。统计打印到 stderr/stdout。

use std::io::Write;
use std::path::Path;

use conv_core::design::eval::Mesh;
use conv_core::design::uv;

fn read_obj(path: &Path) -> Result<Mesh, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("读不了 {}：{e}", path.display()))?;
    let mut verts = Vec::new();
    let mut tris = Vec::new();
    for (ln, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let x: f32 = it
                    .next()
                    .ok_or_else(|| format!("第 {} 行 v 缺 x", ln + 1))?
                    .parse()
                    .map_err(|_| format!("第 {} 行 x 不是数字", ln + 1))?;
                let y: f32 = it
                    .next()
                    .ok_or_else(|| format!("第 {} 行 v 缺 y", ln + 1))?
                    .parse()
                    .map_err(|_| format!("第 {} 行 y 不是数字", ln + 1))?;
                let z: f32 = it
                    .next()
                    .ok_or_else(|| format!("第 {} 行 v 缺 z", ln + 1))?
                    .parse()
                    .map_err(|_| format!("第 {} 行 z 不是数字", ln + 1))?;
                verts.push([x, y, z]);
            }
            Some("f") => {
                let mut idx = Vec::new();
                for tok in it {
                    let head = tok.split('/').next().unwrap_or(tok);
                    let raw: i64 = head
                        .parse()
                        .map_err(|_| format!("第 {} 行面索引不是整数：{head}", ln + 1))?;
                    let n = verts.len() as i64;
                    let i = if raw > 0 {
                        raw - 1
                    } else if raw < 0 {
                        n + raw
                    } else {
                        return Err(format!("第 {} 行面索引 0 非法", ln + 1));
                    };
                    if i < 0 || i >= n {
                        return Err(format!("第 {} 行面索引越界：{raw}（顶点 {n}）", ln + 1));
                    }
                    idx.push(i as u32);
                }
                if idx.len() < 3 {
                    continue;
                }
                for k in 1..idx.len() - 1 {
                    tris.push([idx[0], idx[k], idx[k + 1]]);
                }
            }
            _ => {}
        }
    }
    if verts.is_empty() || tris.is_empty() {
        return Err("OBJ 没有顶点或面".into());
    }
    Ok(Mesh { verts, tris })
}

fn write_obj_uv(mesh: &Mesh, layout: &uv::UvLayout, path: &Path) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(f, "# uvunwrap (trit design uv)")?;
    for v in &mesh.verts {
        writeln!(f, "v {:.6} {:.6} {:.6}", v[0], v[1], v[2])?;
    }
    for t in &layout.uvs {
        writeln!(f, "vt {:.6} {:.6}", t[0], t[1])?;
    }
    for (ti, tri) in mesh.tris.iter().enumerate() {
        let ut = layout.uv_tris[ti];
        let b_v = 1u32;
        let b_t = 1u32;
        writeln!(
            f,
            "f {}/{} {}/{} {}/{}",
            tri[0] + b_v,
            ut[0] + b_t,
            tri[1] + b_v,
            ut[1] + b_t,
            tri[2] + b_v,
            ut[2] + b_t
        )?;
    }
    f.flush()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("用法: uvunwrap <in.obj> <out.obj>");
        std::process::exit(2);
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let mesh = read_obj(input).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let layout = uv::unwrap(&mesh, 0.001);
    write_obj_uv(&mesh, &layout, output).unwrap_or_else(|e| {
        eprintln!("写不了 {}：{e}", output.display());
        std::process::exit(1);
    });
    println!(
        "{{\"verts\":{},\"tris\":{},\"islands\":{},\"fill\":{:.4},\"overlaps\":{},\"tri_overlaps\":{},\"uvs\":{}}}",
        mesh.verts.len(),
        mesh.tris.len(),
        layout.islands.len(),
        layout.fill,
        layout.overlaps,
        layout.tri_overlaps,
        layout.uvs.len()
    );
}
