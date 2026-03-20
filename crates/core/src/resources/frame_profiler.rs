use std::time::Instant;

use bevy_ecs::resource::Resource;

struct Section {
    total_ms: f64,
    count: u32,
}

/// Accumulates per-frame CPU and GPU section timings and periodically logs
/// a compact single-line performance summary.
#[derive(Resource)]
pub struct FrameProfiler {
    sections: Vec<(String, Section)>,
    open_section: Option<(usize, Instant)>,
    frame_start: Instant,
    frame_total_ms: f64,
    frame_count: u32,
    report_interval_secs: f32,
    last_report: Instant,
    enabled: bool,
}

impl Default for FrameProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameProfiler {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            sections: Vec::new(),
            open_section: None,
            frame_start: now,
            frame_total_ms: 0.0,
            frame_count: 0,
            report_interval_secs: 3.0,
            last_report: now,
            enabled: true,
        }
    }

    pub fn begin_frame(&mut self) {
        if !self.enabled {
            return;
        }
        self.frame_start = Instant::now();
    }

    pub fn end_frame(&mut self) {
        if !self.enabled {
            return;
        }
        let elapsed = self.frame_start.elapsed().as_secs_f64() * 1000.0;
        self.frame_total_ms += elapsed;
        self.frame_count += 1;

        if self.last_report.elapsed().as_secs_f32() >= self.report_interval_secs {
            self.report();
        }
    }

    /// Start timing a CPU section. Non-nested — calling `begin` while another
    /// section is open silently closes the previous one.
    pub fn begin(&mut self, name: &str) {
        if !self.enabled {
            return;
        }
        // Close any open section first
        if let Some((idx, start)) = self.open_section.take() {
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            self.sections[idx].1.total_ms += ms;
            self.sections[idx].1.count += 1;
        }
        let idx = self.section_index(name);
        self.open_section = Some((idx, Instant::now()));
    }

    /// End the currently open CPU section.
    pub fn end(&mut self) {
        if !self.enabled {
            return;
        }
        if let Some((idx, start)) = self.open_section.take() {
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            self.sections[idx].1.total_ms += ms;
            self.sections[idx].1.count += 1;
        }
    }

    /// Inject a pre-measured timing (e.g. from GPU timestamp queries).
    pub fn record(&mut self, name: &str, ms: f64) {
        if !self.enabled {
            return;
        }
        let idx = self.section_index(name);
        self.sections[idx].1.total_ms += ms;
        self.sections[idx].1.count += 1;
    }

    fn section_index(&mut self, name: &str) -> usize {
        if let Some(pos) = self.sections.iter().position(|(n, _)| n == name) {
            return pos;
        }
        self.sections.push((
            name.to_string(),
            Section {
                total_ms: 0.0,
                count: 0,
            },
        ));
        self.sections.len() - 1
    }

    fn report(&mut self) {
        if self.frame_count == 0 {
            self.last_report = Instant::now();
            return;
        }

        let fps = self.frame_count as f64 / self.report_interval_secs as f64;
        let avg_frame_ms = self.frame_total_ms / self.frame_count as f64;

        let mut gpu_parts = Vec::new();
        let mut cpu_parts = Vec::new();

        for (name, section) in &self.sections {
            if section.count == 0 {
                continue;
            }
            let avg_ms = section.total_ms / section.count as f64;
            if avg_ms < 0.01 {
                continue;
            }

            let part = format!("{name}={avg_ms:.1}");
            // GPU sections are prefixed with "gpu:" by convention in record()
            if name.starts_with("gpu:") {
                gpu_parts.push(part.replacen("gpu:", "", 1));
            } else {
                cpu_parts.push(part);
            }
        }

        let mut line = format!("[perf] {fps:.0}fps {avg_frame_ms:.1}ms");

        if !gpu_parts.is_empty() {
            line.push_str(" | gpu: ");
            line.push_str(&gpu_parts.join(" "));
        }
        if !cpu_parts.is_empty() {
            line.push_str(" | cpu: ");
            line.push_str(&cpu_parts.join(" "));
        }

        tracing::info!("{line}");

        // Reset accumulators
        self.frame_total_ms = 0.0;
        self.frame_count = 0;
        for (_, section) in &mut self.sections {
            section.total_ms = 0.0;
            section.count = 0;
        }
        self.last_report = Instant::now();
    }
}
