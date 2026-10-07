//! GPU timestamp 计时（T-GC-06 第一片，判据 C22"逐边 GPU 时间上限"的测量仪器）。
//!
//! 用法：腿函数收 `Option<&mut GpuTimer>`，每个 compute pass 调 `writes()` 取
//! begin/end 时间戳对；腿结束后 `resolve_ms()` 读回全部时长（ms，按提交序）。
//! 适配器不支持 TIMESTAMP_QUERY 时 `try_new` 返回 None（腿以 None 计时器照常运行）。

use wgpu::util::DeviceExt;

pub struct GpuTimer {
    query_set: wgpu::QuerySet,
    /// `Queue::get_timestamp_period()` 口径：每 tick 的纳秒数（不是毫秒！）
    period_ns: f32,
    next: u32,
    capacity: u32,
}

impl GpuTimer {
    /// 容量 = 时间戳槽位数（每 pass 消耗 2）。适配器不支持时返回 None。
    pub fn try_new(
        device: &wgpu::Device,
        timestamp_period_ns: Option<f32>,
        capacity: u32,
    ) -> Option<Self> {
        let period_ns = timestamp_period_ns?;
        Some(Self {
            query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("conv-timer"),
                ty: wgpu::QueryType::Timestamp,
                count: capacity,
            }),
            period_ns,
            next: 0,
            capacity,
        })
    }

    /// 为下一个 compute pass 分配 begin/end 槽位。
    pub fn writes(&mut self) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        if self.next + 2 > self.capacity {
            return None;
        }
        let b = self.next;
        self.next += 2;
        Some(wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(b),
            end_of_pass_write_index: Some(b + 1),
        })
    }

    /// 读回全部 pass 时长（ms，提交序）。消费自身（resolve 后缓冲作废）。
    pub fn resolve_ms(self, hd: &crate::jfa::Headless) -> Vec<f64> {
        let device = &hd.device;
        let count = self.next;
        if count == 0 {
            return Vec::new();
        }
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("timer-resolve"),
            size: (count as u64) * 8,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let download = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("timer-download"),
            contents: &vec![0u8; (count as usize) * 8],
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        });
        let mut enc =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.resolve_query_set(&self.query_set, 0..count, &resolve, 0);
        enc.copy_buffer_to_buffer(&resolve, 0, &download, 0, (count as u64) * 8);
        hd.queue.submit([enc.finish()]);
        let slice = download.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = slice.get_mapped_range().unwrap();
        let ticks: Vec<u64> = bytemuck::allocation::pod_collect_to_vec(&data);
        ticks
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| {
                let d = p[1].saturating_sub(p[0]);
                d as f64 * self.period_ns as f64 / 1e6
            })
            .collect()
    }
}
