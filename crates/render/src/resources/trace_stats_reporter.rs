use std::time::Instant;

use bevy_ecs::resource::Resource;

use crate::resources::trace::TraceStatsSnapshot;

#[derive(Default)]
struct TraceStatsAccum {
    primary_chunk_steps: u64,
    primary_node_steps: u64,
    primary_descents: u64,
    primary_occupied_chunks: u64,
    primary_empty_chunks: u64,
    shadow_chunk_steps: u64,
    shadow_node_steps: u64,
    shadow_descents: u64,
    hit_pixels: u64,
    miss_pixels: u64,
    shadow_rays: u64,
    shadow_blocked: u64,
    lod_hits: u64,
    material_hits: u64,
    grass_trace_calls: u64,
    grass_run_visits: u64,
    grass_steps: u64,
    grass_candidates: u64,
    grass_tile_rejects: u64,
    grass_heightmap_reads: u64,
    grass_column_misses: u64,
    grass_y_checks: u64,
    grass_y_rejects: u64,
    grass_trace_hits: u64,
    grass_visible_pixels: u64,
    grass_shadow_rays: u64,
    water_pixels: u64,
    water_top_face_pixels: u64,
    water_side_face_pixels: u64,
    water_shadow_rays: u64,
    water_absorb_evals: u64,
    water_underwater_sky: u64,
    water_dda_chunks_behind: u64,
    water_deep_no_hit: u64,
    water_normal_evals: u64,
    water_sky_evals: u64,
    water_normal_lod: u64,
    water_shadow_skipped: u64,
}

/// Periodically logs averaged traversal counters collected from the trace pass.
#[derive(Resource)]
pub(crate) struct TraceStatsReporter {
    accum: TraceStatsAccum,
    frame_count: u32,
    report_interval_secs: f32,
    last_report: Instant,
    enabled: bool,
}

impl Default for TraceStatsReporter {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            accum: TraceStatsAccum::default(),
            frame_count: 0,
            report_interval_secs: 3.0,
            last_report: now,
            enabled: true,
        }
    }
}

impl TraceStatsReporter {
    pub(crate) fn record(&mut self, snapshot: TraceStatsSnapshot) {
        if !self.enabled {
            return;
        }

        self.accum.primary_chunk_steps += u64::from(snapshot.primary_chunk_steps);
        self.accum.primary_node_steps += u64::from(snapshot.primary_node_steps);
        self.accum.primary_descents += u64::from(snapshot.primary_descents);
        self.accum.primary_occupied_chunks += u64::from(snapshot.primary_occupied_chunks);
        self.accum.primary_empty_chunks += u64::from(snapshot.primary_empty_chunks);
        self.accum.shadow_chunk_steps += u64::from(snapshot.shadow_chunk_steps);
        self.accum.shadow_node_steps += u64::from(snapshot.shadow_node_steps);
        self.accum.shadow_descents += u64::from(snapshot.shadow_descents);
        self.accum.hit_pixels += u64::from(snapshot.hit_pixels);
        self.accum.miss_pixels += u64::from(snapshot.miss_pixels);
        self.accum.shadow_rays += u64::from(snapshot.shadow_rays);
        self.accum.shadow_blocked += u64::from(snapshot.shadow_blocked);
        self.accum.lod_hits += u64::from(snapshot.lod_hits);
        self.accum.material_hits += u64::from(snapshot.material_hits);
        self.accum.grass_trace_calls += u64::from(snapshot.grass_trace_calls);
        self.accum.grass_run_visits += u64::from(snapshot.grass_run_visits);
        self.accum.grass_steps += u64::from(snapshot.grass_steps);
        self.accum.grass_candidates += u64::from(snapshot.grass_candidates);
        self.accum.grass_tile_rejects += u64::from(snapshot.grass_tile_rejects);
        self.accum.grass_heightmap_reads += u64::from(snapshot.grass_heightmap_reads);
        self.accum.grass_column_misses += u64::from(snapshot.grass_column_misses);
        self.accum.grass_y_checks += u64::from(snapshot.grass_y_checks);
        self.accum.grass_y_rejects += u64::from(snapshot.grass_y_rejects);
        self.accum.grass_trace_hits += u64::from(snapshot.grass_trace_hits);
        self.accum.grass_visible_pixels += u64::from(snapshot.grass_visible_pixels);
        self.accum.grass_shadow_rays += u64::from(snapshot.grass_shadow_rays);
        self.accum.water_pixels += u64::from(snapshot.water_pixels);
        self.accum.water_top_face_pixels += u64::from(snapshot.water_top_face_pixels);
        self.accum.water_side_face_pixels += u64::from(snapshot.water_side_face_pixels);
        self.accum.water_shadow_rays += u64::from(snapshot.water_shadow_rays);
        self.accum.water_absorb_evals += u64::from(snapshot.water_absorb_evals);
        self.accum.water_underwater_sky += u64::from(snapshot.water_underwater_sky);
        self.accum.water_dda_chunks_behind += u64::from(snapshot.water_dda_chunks_behind);
        self.accum.water_deep_no_hit += u64::from(snapshot.water_deep_no_hit);
        self.accum.water_normal_evals += u64::from(snapshot.water_normal_evals);
        self.accum.water_sky_evals += u64::from(snapshot.water_sky_evals);
        self.accum.water_normal_lod += u64::from(snapshot.water_normal_lod);
        self.accum.water_shadow_skipped += u64::from(snapshot.water_shadow_skipped);
        self.frame_count += 1;

        if self.last_report.elapsed().as_secs_f32() >= self.report_interval_secs {
            self.report();
        }
    }

    fn report(&mut self) {
        if self.frame_count == 0 {
            self.last_report = Instant::now();
            return;
        }

        let total_pixels = (self.accum.hit_pixels + self.accum.miss_pixels).max(1);
        let total_chunk_steps = self.accum.primary_chunk_steps.max(1);
        let total_hits = self.accum.hit_pixels.max(1);
        let total_shadow_rays = self.accum.shadow_rays.max(1);
        let grass_calls = self.accum.grass_trace_calls.max(1);
        let grass_steps = self.accum.grass_steps.max(1);
        let grass_candidates = self.accum.grass_candidates.max(1);
        let grass_heightmap_reads = self.accum.grass_heightmap_reads.max(1);
        let grass_y_checks = self.accum.grass_y_checks.max(1);
        let grass_visible_pixels = self.accum.grass_visible_pixels.max(1);
        let hit_ratio = self.accum.hit_pixels as f64 / total_pixels as f64 * 100.0;
        let occupied_chunk_ratio =
            self.accum.primary_occupied_chunks as f64 / total_chunk_steps as f64 * 100.0;
        let empty_chunk_ratio =
            self.accum.primary_empty_chunks as f64 / total_chunk_steps as f64 * 100.0;
        let lod_ratio = self.accum.lod_hits as f64 / total_hits as f64 * 100.0;
        let shadow_blocked_ratio =
            self.accum.shadow_blocked as f64 / total_shadow_rays as f64 * 100.0;
        let grass_visible_ratio =
            self.accum.grass_visible_pixels as f64 / total_pixels as f64 * 100.0;
        let grass_tile_reject_ratio =
            self.accum.grass_tile_rejects as f64 / grass_candidates as f64 * 100.0;
        let grass_heightmap_ratio =
            self.accum.grass_heightmap_reads as f64 / grass_candidates as f64 * 100.0;
        let grass_column_miss_ratio =
            self.accum.grass_column_misses as f64 / grass_heightmap_reads as f64 * 100.0;
        let grass_y_reject_ratio =
            self.accum.grass_y_rejects as f64 / grass_y_checks as f64 * 100.0;
        let grass_hit_ratio = self.accum.grass_trace_hits as f64 / grass_calls as f64 * 100.0;

        let water_pixels = self.accum.water_pixels.max(1);
        let water_visible_ratio = self.accum.water_pixels as f64 / total_pixels as f64 * 100.0;
        let water_top_ratio = self.accum.water_top_face_pixels as f64 / water_pixels as f64 * 100.0;
        let water_shadow_ratio = self.accum.water_shadow_rays as f64 / water_pixels as f64 * 100.0;
        let water_absorb_ratio = self.accum.water_absorb_evals as f64 / water_pixels as f64 * 100.0;
        let water_uw_sky_ratio =
            self.accum.water_underwater_sky as f64 / total_pixels as f64 * 100.0;
        let water_deep_no_hit_ratio =
            self.accum.water_deep_no_hit as f64 / water_pixels as f64 * 100.0;
        let water_chunks_behind_per_pix =
            self.accum.water_dda_chunks_behind as f64 / water_pixels as f64;
        let water_normal_ratio = self.accum.water_normal_evals as f64 / water_pixels as f64 * 100.0;
        let water_sky_ratio = self.accum.water_sky_evals as f64 / water_pixels as f64 * 100.0;
        let water_normal_lod_ratio =
            self.accum.water_normal_lod as f64 / water_pixels as f64 * 100.0;
        let water_shadow_skip_ratio =
            self.accum.water_shadow_skipped as f64 / water_pixels as f64 * 100.0;

        tracing::info!(
            "[trace-stats] hit={hit_ratio:.0}% lod={lod_ratio:.0}% \
             | primary: chunks/pix={:.1} nodes/pix={:.1} desc/pix={:.1} \
             occ-chunks/pix={:.1} empty-chunks/pix={:.1} occ={occupied_chunk_ratio:.0}% empty={empty_chunk_ratio:.0}% \
             | grass: vis={grass_visible_ratio:.0}% calls/pix={:.2} tiles/call={:.1} steps/call={:.1} cand/step={:.1} tile-rej={grass_tile_reject_ratio:.0}% hm/cand={:.0}% col-miss/read={grass_column_miss_ratio:.0}% y-rej/check={grass_y_reject_ratio:.0}% hit/call={grass_hit_ratio:.0}% shadow/vis={:.1} \
             | water: vis={water_visible_ratio:.0}% top={water_top_ratio:.0}% shadow={water_shadow_ratio:.0}% absorb={water_absorb_ratio:.0}% uw-sky={water_uw_sky_ratio:.0}% deep-miss={water_deep_no_hit_ratio:.0}% chunks-behind/pix={water_chunks_behind_per_pix:.1} normals={water_normal_ratio:.0}% sky={water_sky_ratio:.0}% n-lod={water_normal_lod_ratio:.0}% shad-skip={water_shadow_skip_ratio:.0}% \
             | shadow: rays/hit={:.1} blocked={shadow_blocked_ratio:.0}% chunks/ray={:.1} nodes/ray={:.1} desc/ray={:.1}",
            self.accum.primary_chunk_steps as f64 / total_pixels as f64,
            self.accum.primary_node_steps as f64 / total_pixels as f64,
            self.accum.primary_descents as f64 / total_pixels as f64,
            self.accum.primary_occupied_chunks as f64 / total_pixels as f64,
            self.accum.primary_empty_chunks as f64 / total_pixels as f64,
            self.accum.grass_trace_calls as f64 / total_pixels as f64,
            self.accum.grass_run_visits as f64 / grass_calls as f64,
            self.accum.grass_steps as f64 / grass_calls as f64,
            self.accum.grass_candidates as f64 / grass_steps as f64,
            grass_heightmap_ratio,
            self.accum.grass_shadow_rays as f64 / grass_visible_pixels as f64,
            self.accum.shadow_rays as f64 / total_hits as f64,
            self.accum.shadow_chunk_steps as f64 / total_shadow_rays as f64,
            self.accum.shadow_node_steps as f64 / total_shadow_rays as f64,
            self.accum.shadow_descents as f64 / total_shadow_rays as f64,
        );

        self.accum = TraceStatsAccum::default();
        self.frame_count = 0;
        self.last_report = Instant::now();
    }
}
