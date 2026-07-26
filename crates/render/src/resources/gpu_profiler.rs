use capy_core::FrameProfiler;

const MAX_QUERIES: u32 = 16; // 8 pass pairs (begin + end)

struct ProfilerFrame {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    next_query: u32,
    labels: Vec<(String, u32, u32)>, // (name, begin_idx, end_idx)
    submission: Option<wgpu::SubmissionIndex>,
}

impl ProfilerFrame {
    fn new(device: &wgpu::Device) -> Self {
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("GPU Profiler QuerySet"),
            ty: wgpu::QueryType::Timestamp,
            count: MAX_QUERIES,
        });
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Profiler Resolve"),
            size: (MAX_QUERIES as u64) * 8,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU Profiler Readback"),
            size: (MAX_QUERIES as u64) * 8,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            query_set,
            resolve_buffer,
            readback_buffer,
            next_query: 0,
            labels: Vec::new(),
            submission: None,
        }
    }

    fn reset(&mut self) {
        self.next_query = 0;
        self.labels.clear();
        self.submission = None;
    }
}

/// Double-buffered GPU timestamp query profiler.
///
/// Writes timestamp queries into the current frame's query set during render
/// passes, then reads back the *previous* frame's results (guaranteed complete
/// after `queue.submit`).
pub(crate) struct GpuProfiler {
    frames: [ProfilerFrame; 2],
    current_frame: usize,
    timestamp_period: f32, // nanoseconds per tick
    pub(crate) supported: bool,
}

impl GpuProfiler {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, supported: bool) -> Self {
        let timestamp_period = queue.get_timestamp_period();
        Self {
            frames: [ProfilerFrame::new(device), ProfilerFrame::new(device)],
            current_frame: 0,
            timestamp_period,
            supported,
        }
    }

    /// Allocate a begin/end query index pair for a named pass.
    /// Returns `None` if timestamps are unsupported or we've run out of query slots.
    pub(crate) fn pass_indices(&mut self, name: &str) -> Option<(u32, u32)> {
        if !self.supported {
            return None;
        }
        let frame = &mut self.frames[self.current_frame];
        if frame.next_query + 2 > MAX_QUERIES {
            return None;
        }
        let begin = frame.next_query;
        let end = begin + 1;
        frame.next_query += 2;
        frame.labels.push((name.to_string(), begin, end));
        Some((begin, end))
    }

    /// Borrow the current frame's query set for use in pass descriptors.
    pub(crate) fn query_set(&self) -> &wgpu::QuerySet {
        &self.frames[self.current_frame].query_set
    }

    /// Resolve all written queries into the resolve buffer, then copy to readback.
    /// Call this after the last timed pass, before any encoder split (e.g. DLSS).
    pub(crate) fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.supported {
            return;
        }
        let frame = &self.frames[self.current_frame];
        if frame.next_query == 0 {
            return;
        }
        encoder.resolve_query_set(
            &frame.query_set,
            0..frame.next_query,
            &frame.resolve_buffer,
            0,
        );
        encoder.copy_buffer_to_buffer(
            &frame.resolve_buffer,
            0,
            &frame.readback_buffer,
            0,
            (frame.next_query as u64) * 8,
        );
    }

    /// Map the *previous* frame's readback buffer and inject timings into FrameProfiler.
    /// The previous frame's GPU work is guaranteed complete after `queue.submit`.
    pub(crate) fn read_back(&self, device: &wgpu::Device, profiler: &mut FrameProfiler) {
        if !self.supported {
            return;
        }
        let prev = 1 - self.current_frame;
        let frame = &self.frames[prev];
        if frame.labels.is_empty() {
            return;
        }

        let slice = frame.readback_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        // Wait only for the previous frame's submission (already complete by
        // now) — waiting on the latest submission would block on the frame
        // just submitted and serialize CPU and GPU.
        device
            .poll(wgpu::PollType::Wait {
                submission_index: frame.submission.clone(),
                timeout: None,
            })
            .ok();

        {
            let data = slice.get_mapped_range();
            let timestamps: &[u64] = bytemuck::cast_slice(&data[..frame.next_query as usize * 8]);

            let ns_per_tick = self.timestamp_period as f64;
            for (name, begin_idx, end_idx) in &frame.labels {
                let begin_tick = timestamps[*begin_idx as usize];
                let end_tick = timestamps[*end_idx as usize];
                if end_tick > begin_tick {
                    let ms = (end_tick - begin_tick) as f64 * ns_per_tick / 1_000_000.0;
                    profiler.record(&format!("gpu:{name}"), ms);
                }
            }
        }

        frame.readback_buffer.unmap();
    }

    /// Record the submission that contains this frame's resolve/readback copy.
    pub(crate) fn set_submission(&mut self, submission: wgpu::SubmissionIndex) {
        self.frames[self.current_frame].submission = Some(submission);
    }

    /// Swap double buffer and reset the new current frame.
    pub(crate) fn end_frame(&mut self) {
        self.current_frame = 1 - self.current_frame;
        self.frames[self.current_frame].reset();
    }
}
