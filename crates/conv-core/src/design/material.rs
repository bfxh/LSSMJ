//! 物料引擎（第一版）：材质表 + 零件绑定 + MTL 导出。
//!
//! 现状缺口：`Op::Texture { name }` 之前只把 `State.textured` 置真，**没有材质表**，
//! 名字也不校验，导出的 OBJ 没有 `usemtl`/`.mtl`——也就是"贴图"在产物里根本看不见。
//!
//! 这一版只补数据面与导出，不改几何：
//! - [`MaterialLib`]：按声明顺序保存材质，名字唯一，查表 O(n)（材质个数是个位数，别上 HashMap，
//!   否则导出顺序会随哈希漂移，破坏"同一份设计 → 同一份文件"）；
//! - [`bindings`]：按零件路径收集它绑定过的材质名（`Op::Texture` 的最后一个生效，与建造态一致）；
//! - [`validate`]：引用了未声明的材质 → 带零件路径的诊断，**不静默降级成默认材质**；
//! - [`write_obj_mtl`]：写 `.obj`（带 `mtllib`/`usemtl`）+ `.mtl`，浮点定点输出，确定性。

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;

use super::eval::Mesh;
use super::ir::{Design, Node, Op, Part};
use super::plan::Diag;

/// 一种材质。只放"能进 MTL 且能确定复现"的字段；贴图文件不在这里（那是资产管线的事）。
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    /// 线性 sRGB 基色，0..=1。
    pub base_color: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
}

impl Material {
    pub fn new(
        name: &str,
        base_color: [f32; 3],
        metallic: f32,
        roughness: f32,
    ) -> Result<Self, String> {
        let name = name.trim();
        if name.is_empty() || name.len() > 64 {
            return Err(format!("材质名非法 {name:?}：长度 1..=64"));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Err(format!(
                "材质名 {name:?}：只允许字母数字与 _-.（要进 MTL 名字）"
            ));
        }
        for (i, c) in base_color.iter().enumerate() {
            if !(c.is_finite() && (0.0..=1.0).contains(c)) {
                return Err(format!("base_color[{i}]={c} 必须在 0..=1"));
            }
        }
        for (label, v) in [("metallic", metallic), ("roughness", roughness)] {
            if !(v.is_finite() && (0.0..=1.0).contains(&v)) {
                return Err(format!("{label}={v} 必须在 0..=1"));
            }
        }
        Ok(Self {
            name: name.to_string(),
            base_color,
            metallic,
            roughness,
        })
    }

    /// MTL 的 `Kd` 行（基色）。
    pub fn kd(&self) -> [f32; 3] {
        self.base_color
    }
}

/// 有序材质表：顺序即导出顺序。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialLib {
    items: Vec<Material>,
}

impl MaterialLib {
    pub fn new(items: Vec<Material>) -> Result<Self, String> {
        let mut seen = BTreeSet::new();
        for m in &items {
            if !seen.insert(m.name.clone()) {
                return Err(format!("材质名重复：{}", m.name));
            }
        }
        Ok(Self { items })
    }

    pub fn get(&self, name: &str) -> Option<&Material> {
        self.items.iter().find(|m| m.name == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Material> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// 零件路径 → 它最后绑定到的材质名。
pub type Bindings = Vec<(String, String)>;

/// 按声明顺序收集每个零件最后绑定的材质（`Op::Texture` 后写覆盖前写）。
pub fn bindings(d: &Design) -> Bindings {
    let mut out = Vec::new();
    for s in &d.systems {
        for a in &s.assemblies {
            walk_asm(a, &format!("{}/{}", s.id, a.id), &mut out);
        }
    }
    out
}

fn walk_asm(a: &super::ir::Assembly, path: &str, out: &mut Bindings) {
    for child in &a.children {
        match child {
            Node::Assembly(sub) => walk_asm(sub, &format!("{path}/{}", sub.id), out),
            Node::Part(p) => {
                if let Some(name) = last_texture(p) {
                    out.push((format!("{path}/{}", p.id), name));
                }
            }
        }
    }
}

fn last_texture(p: &Part) -> Option<String> {
    p.ops.iter().rev().find_map(|op| match op {
        Op::Texture { name } => Some(name.clone()),
        _ => None,
    })
}

/// 校验：引用了材质表里没有的名字 ⇒ 诊断（带零件路径）。空表 + 有引用也算红。
pub fn validate(d: &Design, lib: &MaterialLib) -> Vec<Diag> {
    let mut diags = Vec::new();
    for (path, name) in bindings(d) {
        if lib.get(&name).is_none() {
            diags.push(Diag {
                path,
                msg: format!("引用了未声明的材质 {name:?}（先放进材质表，或改名字）"),
            });
        }
    }
    diags
}

/// 写 OBJ + MTL：OBJ 带 `mtllib` 与每个 `o` 前的 `usemtl`；MTL 按材质表顺序。
///
/// `parts` 里每项是 `(零件路径, 实体列表)`；没绑材质的零件不写 `usemtl`（用查看器默认材质），
/// 这样"没材质"和"材质叫 default"不会混为一谈。
pub fn write_obj_mtl(
    parts: &[(String, Vec<Mesh>)],
    binds: &Bindings,
    lib: &MaterialLib,
    obj_path: &Path,
    mtl_path: &Path,
) -> std::io::Result<()> {
    let mtl_name = mtl_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "materials.mtl".to_string());

    // MTL
    {
        let mut m = std::io::BufWriter::new(std::fs::File::create(mtl_path)?);
        writeln!(m, "# trit material library")?;
        for mat in lib.iter() {
            let [r, g, b] = mat.kd();
            writeln!(m, "newmtl {}", mat.name)?;
            writeln!(m, "Kd {r:.6} {g:.6} {b:.6}")?;
            writeln!(m, "Ks 0.000000 0.000000 0.000000")?;
            writeln!(m, "Ns {:.6}", 1.0 + 255.0 * (1.0 - mat.roughness))?;
            writeln!(m, "d 1.000000")?;
            writeln!(m, "illum 2")?;
            // metallic 没有标准 MTL 字段；写成注释保住信息，不冒充 Pm 之类的扩展。
            writeln!(m, "# metallic {:.6}", mat.metallic)?;
        }
        m.flush()?;
    }

    // OBJ
    let mut f = std::io::BufWriter::new(std::fs::File::create(obj_path)?);
    writeln!(f, "# trit design export")?;
    writeln!(f, "mtllib {mtl_name}")?;
    let mut base = 1u32;
    for (name, bodies) in parts {
        let mat = binds
            .iter()
            .find(|(p, _)| p == name)
            .map(|(_, m)| m.as_str());
        for (bi, m) in bodies.iter().enumerate() {
            writeln!(f, "o {}#{}", name, bi)?;
            if let Some(mat) = mat {
                writeln!(f, "usemtl {mat}")?;
            }
            for v in &m.verts {
                writeln!(f, "v {:.6} {:.6} {:.6}", v[0], v[1], v[2])?;
            }
            for t in &m.tris {
                writeln!(f, "f {} {} {}", t[0] + base, t[1] + base, t[2] + base)?;
            }
            base += m.verts.len() as u32;
        }
    }
    f.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::ir::{Assembly, Attach, Design, Id, Node, Op, Part, Place, System};

    fn part(id: &str, ops: Vec<Op>) -> Part {
        Part {
            id: Id::new(id).unwrap(),
            attach: Attach::Base(Place::IDENTITY),
            ops,
        }
    }

    fn design_with(ops: Vec<Op>) -> Design {
        Design {
            name: "t".into(),
            systems: vec![System {
                id: Id::new("sys").unwrap(),
                assemblies: vec![Assembly {
                    id: Id::new("asm").unwrap(),
                    place: Place::IDENTITY,
                    children: vec![Node::Part(part("p1", ops))],
                }],
            }],
        }
    }

    fn lib() -> MaterialLib {
        MaterialLib::new(vec![
            Material::new("steel", [0.6, 0.6, 0.65], 1.0, 0.35).unwrap(),
            Material::new("wood", [0.45, 0.3, 0.18], 0.0, 0.8).unwrap(),
        ])
        .unwrap()
    }

    #[test]
    fn rejects_bad_material_params() {
        assert!(Material::new("", [0.5; 3], 0.0, 0.5).is_err(), "空名要红");
        assert!(
            Material::new("a b", [0.5; 3], 0.0, 0.5).is_err(),
            "空格进不了 MTL"
        );
        assert!(
            Material::new("m", [1.5, 0.0, 0.0], 0.0, 0.5).is_err(),
            "基色越界要红"
        );
        assert!(
            Material::new("m", [0.5; 3], 0.0, f32::NAN).is_err(),
            "NaN 要红"
        );
        assert!(Material::new("m", [0.5; 3], 0.0, 0.5).is_ok());
    }

    #[test]
    fn duplicate_names_rejected() {
        let m = Material::new("steel", [0.5; 3], 0.0, 0.5).unwrap();
        assert!(MaterialLib::new(vec![m.clone(), m]).is_err());
    }

    #[test]
    fn last_texture_wins() {
        let d = design_with(vec![
            Op::Texture {
                name: "steel".into(),
            },
            Op::Texture {
                name: "wood".into(),
            },
        ]);
        let b = bindings(&d);
        assert_eq!(b, vec![("sys/asm/p1".to_string(), "wood".to_string())]);
    }

    #[test]
    fn unknown_material_is_an_error_with_path() {
        let d = design_with(vec![Op::Texture {
            name: "unobtainium".into(),
        }]);
        let diags = validate(&d, &lib());
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].path, "sys/asm/p1");
        assert!(diags[0].msg.contains("unobtainium"));
    }

    #[test]
    fn no_texture_means_no_binding_and_no_diag() {
        let d = design_with(vec![]);
        assert!(bindings(&d).is_empty());
        assert!(validate(&d, &lib()).is_empty());
    }

    #[test]
    fn obj_mtl_export_is_deterministic_and_tagged() {
        let d = design_with(vec![Op::Texture {
            name: "steel".into(),
        }]);
        let binds = bindings(&d);
        let mesh = Mesh {
            verts: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            tris: vec![[0, 1, 2]],
        };
        let parts = vec![("sys/asm/p1".to_string(), vec![mesh])];
        let td = std::env::temp_dir().join(format!("lssmj_mat_{}", std::process::id()));
        std::fs::create_dir_all(&td).unwrap();
        let (obj, mtl) = (td.join("a.obj"), td.join("a.mtl"));
        write_obj_mtl(&parts, &binds, &lib(), &obj, &mtl).unwrap();

        let o = std::fs::read_to_string(&obj).unwrap();
        let m = std::fs::read_to_string(&mtl).unwrap();
        assert!(o.contains("mtllib a.mtl"));
        assert!(o.contains("usemtl steel"));
        assert!(o.contains("o sys/asm/p1#0"));
        assert!(m.contains("newmtl steel") && m.contains("newmtl wood"));
        assert!(m.contains("Kd 0.600000 0.600000 0.650000"));

        // 同输入两次 ⇒ 字节相同（同目录同名才算同输入：OBJ 里嵌了 MTL 文件名）
        let td2 = td.join("again");
        std::fs::create_dir_all(&td2).unwrap();
        let (obj2, mtl2) = (td2.join("a.obj"), td2.join("a.mtl"));
        write_obj_mtl(&parts, &binds, &lib(), &obj2, &mtl2).unwrap();
        assert_eq!(o, std::fs::read_to_string(&obj2).unwrap());
        assert_eq!(m, std::fs::read_to_string(&mtl2).unwrap());
        let _ = std::fs::remove_dir_all(&td);
    }
}
