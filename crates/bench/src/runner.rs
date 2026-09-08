use crate::{
    cli::Options,
    report::{Distribution, Report, Run},
    scenario::{Kind, STEP, Scenario},
};
use anyhow::{Context, Result, ensure};
use engine::ScenarioRenderer;
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn measure(gpu: &mut ScenarioRenderer, kind: Kind, run: u32, options: &Options) -> Result<Run> {
    let size = [options.width, options.height];

    let mut warmup = Scenario::new(kind)?;

    for frame in 0..options.warmup {
        warmup.update(frame)?;

        gpu.submit(warmup.view(), &warmup.canvas(frame, size))?;
    }

    gpu.finish()?;

    drop(warmup);

    let setup = Instant::now();

    let mut scenario = Scenario::new(kind)?;

    gpu.submit(scenario.view(), &scenario.canvas(0, size))?;

    gpu.finish()?;

    let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;

    let capacity = options.frames as usize;

    let mut cpu = Vec::with_capacity(capacity);

    let mut updates = Vec::with_capacity(capacity);

    let mut prepares = Vec::with_capacity(capacity);

    let mut encodes = Vec::with_capacity(capacity);

    let mut waits = Vec::with_capacity(capacity);

    let start = Instant::now();

    for frame in 0..options.frames {
        let frame_start = Instant::now();

        scenario.update(frame)?;

        let canvas = scenario.canvas(frame, size);

        let update = frame_start.elapsed();

        let timing = gpu.submit(scenario.view(), &canvas)?;

        drop(canvas);

        cpu.push(frame_start.elapsed().saturating_sub(timing.backpressure));

        updates.push(update);

        prepares.push(timing.prepare);

        encodes.push(timing.encode);

        waits.push(timing.backpressure);
    }

    let gpu_times = gpu.finish()?;

    let elapsed_seconds = start.elapsed().as_secs_f64();

    ensure!(
        !gpu.timestamps_supported() || gpu_times.len() == capacity,
        "incomplete GPU timing results"
    );

    Ok(Run {
        scenario: kind,
        run,
        frames: options.frames,
        setup_ms,
        elapsed_seconds,
        completed_frames_per_second: f64::from(options.frames) / elapsed_seconds,
        cpu_frame_ms: Distribution::milliseconds(&cpu).context("missing CPU samples")?,
        update_and_ui_ms: Distribution::milliseconds(&updates).context("missing update samples")?,
        render_prepare_ms: Distribution::milliseconds(&prepares)
            .context("missing preparation samples")?,
        encode_and_submit_ms: Distribution::milliseconds(&encodes)
            .context("missing encoding samples")?,
        backpressure_ms: Distribution::milliseconds(&waits)
            .context("missing backpressure samples")?,
        gpu_render_ms: Distribution::milliseconds(&gpu_times),
    })
}

pub fn run(options: Options) -> Result<()> {
    let mut output = options
        .output
        .as_ref()
        .map(|path| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .with_context(|| {
                    format!(
                        "create report {}; choose a new path to avoid overwriting a previous run",
                        path.display()
                    )
                })
                .map(BufWriter::new)
        })
        .transpose()?;

    let mut gpu = ScenarioRenderer::new([options.width, options.height])?;

    let info = gpu.adapter();

    let mut report = Report {
        schema_version: 1,
        workload_version: 1,
        version: env!("CARGO_PKG_VERSION"),
        mode: "offscreen-throughput",
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        debug_build: cfg!(debug_assertions),
        unix_timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        resolution: [options.width, options.height],
        simulation_step_ns: STEP.as_nanos(),
        warmup_frames: options.warmup,
        frames_in_flight: gpu.frames_in_flight(),
        adapter: info.name.clone(),
        backend: format!("{:?}", info.backend),
        driver: info.driver.clone(),
        driver_info: info.driver_info.clone(),
        device_type: format!("{:?}", info.device_type),
        gpu_timestamps: gpu.timestamps_supported(),
        runs: Vec::new(),
    };

    println!(
        "{} / {} | {}x{} | offscreen throughput",
        report.adapter, report.backend, options.width, options.height
    );

    println!(
        "Fixed 60 Hz simulation, {} frames in flight; not display FPS or a compatibility rating.",
        report.frames_in_flight
    );

    if report.debug_build {
        println!("Debug build: use --release for meaningful performance results.");
    }

    if !report.gpu_timestamps {
        println!("GPU timestamps unavailable; GPU render timings will be omitted.");
    }

    println!(
        "{:<14} {:>3} {:>12} {:>12} {:>12}",
        "Scenario", "Run", "frames/s", "CPU p95 ms", "GPU p95 ms"
    );

    let scenarios = if options.scenario.is_empty() {
        vec![Kind::TownWalk, Kind::Destruction, Kind::UiHeavy]
    } else {
        options.scenario.clone()
    };

    for kind in scenarios {
        for index in 1..=options.runs {
            let result = measure(&mut gpu, kind, index, &options)
                .with_context(|| format!("{} run {index}", kind.name()))?;

            let gpu_ms = result
                .gpu_render_ms
                .as_ref()
                .map_or_else(|| "n/a".to_owned(), |value| format!("{:.3}", value.p95));

            println!(
                "{:<14} {:>3} {:>12.1} {:>12.3} {:>12}",
                kind.name(),
                index,
                result.completed_frames_per_second,
                result.cpu_frame_ms.p95,
                gpu_ms
            );

            report.runs.push(result);
        }
    }

    if let Some(output) = &mut output {
        serde_json::to_writer_pretty(&mut *output, &report)?;

        output.write_all(b"\n")?;

        output.flush()?;
    }

    Ok(())
}
