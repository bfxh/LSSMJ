//! API 边界契约（v5 阶段 0 / F12 附加约束）：n<2、n³ 溢出、资源上限都在入口边界拒绝。
//! 守卫是显式 assert 且先于任何 u32 乘法 ⇒ **debug/release 同判**（不靠算术溢出的 profile 差异）。
//! 边界锚：n=2 最小合法格；n=1625 是 n³ ≤ u32::MAX 的上界（1625³=4,293,671,875；
//! 1626³=4,299,081,976 已溢出）。

use std::sync::Mutex;

use conv_core::{
    field_to_voxels, gsn::surface_nets_gpu, jfa::headless_device, jfa::jfa_distance_field,
    mesh_to_sdf_band, require_device_buffer, require_grid,
};

// GPU 测试串行（并行建设备死锁，本机实测惯犯）。
// should_panic 测试会持锁 panic ⇒ 锁中毒；本锁只做互斥、无共享状态 ⇒ 容忍 poison 取回内部门。
static GPU_LOCK: Mutex<()> = Mutex::new(());

fn gpu_lock() -> std::sync::MutexGuard<'static, ()> {
    GPU_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

// ---- require_grid：尺寸契约的直接边界 ----

#[test]
#[should_panic(expected = "< 2")]
fn grid_n_zero_rejected() {
    require_grid(0, "注入");
}

#[test]
#[should_panic(expected = "< 2")]
fn grid_n_one_rejected() {
    require_grid(1, "注入");
}

#[test]
fn grid_boundary_values_accepted() {
    require_grid(2, "注入");
    require_grid(64, "注入");
    require_grid(1625, "注入"); // n³ = 4,293,671,875 ≤ u32::MAX
}

#[test]
#[should_panic(expected = "溢出 u32")]
fn grid_n_1626_rejected() {
    require_grid(1626, "注入"); // n³ = 4,299,081,976 > u32::MAX
}

// ---- CPU 入口：分配发生前拒绝 ----

#[test]
#[should_panic(expected = "field_to_voxels")]
fn cpu_field_n_one_rejected() {
    let _ = field_to_voxels(1, 0.75);
}

#[test]
#[should_panic(expected = "溢出 u32")]
fn cpu_band_overflow_rejected_before_alloc() {
    // 守卫先于 n³ 乘法与缓冲分配 ⇒ 空网格入参也安全红
    let _ = mesh_to_sdf_band(1626, 0.75, 0.1, &[], &[]);
}

// ---- GPU 入口：同契约 + 资源上限预检 ----

#[test]
#[should_panic(expected = "< 2")]
fn gpu_jfa_n_one_rejected_before_alloc() {
    let _gpu = gpu_lock();
    let hd = headless_device();
    let _ = jfa_distance_field(&hd, &[], 1, None);
}

#[test]
#[should_panic(expected = "< 2")]
fn gpu_gsn_n_one_rejected_before_dispatch() {
    let _gpu = gpu_lock();
    let hd = headless_device();
    let _ = surface_nets_gpu(&hd, &[], 1, None);
}

#[test]
#[should_panic(expected = "超适配器上限")]
fn device_buffer_cap_rejects_any_adapter() {
    // 与适配器无关的确定性红：u64::MAX 必超任何 max_buffer_size——
    // 不写依赖具体显存容量的测试（适配器而异 ⇒ flaky）；各入口的接线由本判据 + 代码审覆盖。
    let _gpu = gpu_lock();
    let hd = headless_device();
    require_device_buffer(&hd.device, u64::MAX, "注入");
}
