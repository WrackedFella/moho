//! Renders a fixed shell frame on whatever wgpu adapter is present, so two runs, or two machines,
//! can be compared pixel for pixel.
//!
//! ```text
//! frame_probe render --out <dir> [--iterations <n>]
//! frame_probe compare <dir-a> <dir-b>
//! ```
//!
//! `render` writes `frame-<i>.png` and `probe.json` (adapter, timings, hashes, drift from frame 0)
//! into `<dir>`; `compare` prints the difference between the first frames of two such directories
//! as a Markdown table.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use egui::{Color32, vec2};
use egui_kittest::Harness;
use egui_kittest::wgpu::default_wgpu_setup;
use egui_wgpu::WgpuSetup;
use image::RgbaImage;
use moho_ui_shell::{Anchor, Console, MenuItem, MenuScreen, ScreenText, UiShell};
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const USAGE: &str = "usage: frame_probe render --out <dir> [--iterations <n>]\n       \
                     frame_probe compare <dir-a> <dir-b>";
const SIZE: [f32; 2] = [1280.0, 720.0];

#[derive(Clone, Debug)]
enum Choice {
    Play,
    Settings,
    Quit,
}

struct Delta {
    max_channel: u8,
    differing_pixels: usize,
}

struct Run {
    adapter: wgpu::AdapterInfo,
    setup_ms: f64,
    render_ms: f64,
    image: RgbaImage,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["render", flags @ ..] => {
            let (out, iterations) = parse_render_flags(flags)?;
            render(&out, iterations)
        }
        ["compare", a, b] => compare(Path::new(a), Path::new(b)),
        _ => Err(USAGE.into()),
    }
}

fn parse_render_flags(flags: &[&str]) -> Result<(PathBuf, usize)> {
    let mut out = None;
    let mut iterations = 5;
    let mut flags = flags.iter();
    while let Some(&flag) = flags.next() {
        let value = flags
            .next()
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag {
            "--out" => out = Some(PathBuf::from(value)),
            "--iterations" => iterations = value.parse()?,
            _ => return Err(format!("unknown argument {flag}\n{USAGE}").into()),
        }
    }
    Ok((
        out.ok_or_else(|| format!("--out is required\n{USAGE}"))?,
        iterations,
    ))
}

fn render(out: &Path, iterations: usize) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let mut runs = Vec::with_capacity(iterations);
    for index in 0..iterations {
        let run = render_frame()?;
        run.image.save(out.join(format!("frame-{index}.png")))?;
        runs.push(run);
    }
    let first = runs.first().ok_or("--iterations must be at least 1")?;
    if let Some(other) = runs.iter().find(|run| run.adapter != first.adapter) {
        return Err(format!(
            "the adapter changed between iterations: {:?} then {:?}",
            first.adapter.name, other.adapter.name
        )
        .into());
    }
    let json = probe_json(&runs)?;
    std::fs::write(out.join("probe.json"), &json)?;
    writeln!(std::io::stdout(), "{json}")?;
    Ok(())
}

/// Builds a fresh adapter, device and harness on every call, so runs share no GPU state.
fn render_frame() -> Result<Run> {
    let started = Instant::now();
    let (setup, chosen) = recording_setup()?;
    let mut harness = Harness::builder()
        .with_size(vec2(SIZE[0], SIZE[1]))
        .with_pixels_per_point(1.0)
        .wgpu_setup(setup)
        .build_ui_state(
            |ui, shell: &mut UiShell<(), Choice>| {
                // The shell paints on a transparent background.
                ui.painter().rect_filled(
                    ui.ctx().content_rect(),
                    0.0,
                    Color32::from_rgb(24, 26, 32),
                );
                shell.paint(ui.ctx());
            },
            fixed_shell()?,
        );
    harness.run();
    let setup_ms = started.elapsed().as_secs_f64() * 1000.0;
    let adapter = chosen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or("no adapter was selected")?;

    let started = Instant::now();
    let image = harness.render()?;
    let render_ms = started.elapsed().as_secs_f64() * 1000.0;
    Ok(Run {
        adapter,
        setup_ms,
        render_ms,
        image,
    })
}

/// The harness's default setup, plus a slot that receives the adapter it picks.
fn recording_setup() -> Result<(WgpuSetup, Arc<Mutex<Option<wgpu::AdapterInfo>>>)> {
    let WgpuSetup::CreateNew(mut create) = default_wgpu_setup() else {
        return Err("the default wgpu setup no longer creates a new instance".into());
    };
    let select = create
        .native_adapter_selector
        .take()
        .ok_or("the default wgpu setup has no adapter selector")?;
    let chosen = Arc::new(Mutex::new(None));
    let record = Arc::clone(&chosen);
    create.native_adapter_selector = Some(Arc::new(move |adapters, surface| {
        let adapter = select(adapters, surface)?;
        *record.lock().unwrap_or_else(PoisonError::into_inner) = Some(adapter.get_info());
        Ok(adapter)
    }));
    Ok((WgpuSetup::CreateNew(create), chosen))
}

fn fixed_shell() -> Result<UiShell<(), Choice>> {
    let (console, _) = Console::<()>::new(&BTreeMap::new());
    let mut shell = UiShell::new(console);
    let button = |label: &str, action| MenuItem::Button {
        label: label.to_owned(),
        action,
    };
    shell.menu_mut().push(MenuScreen {
        title: Some("Frame probe".to_owned()),
        items: vec![
            button("Play", Choice::Play),
            button("Settings", Choice::Settings),
            button("Quit", Choice::Quit),
        ],
        back: None,
        settings: None,
    });
    let text = |text: &str, anchor, offset| ScreenText {
        text: text.to_owned(),
        anchor,
        offset,
        size: 18.0,
        color: [235, 235, 235, 255],
    };
    shell.set_texts(vec![
        text("bottom right", Anchor::BottomRight, [-16.0, -16.0]),
        text("top left", Anchor::TopLeft, [16.0, 16.0]),
    ])?;
    Ok(shell)
}

fn compare(a: &Path, b: &Path) -> Result<()> {
    let frame_a = image::open(a.join("frame-0.png"))?.to_rgba8();
    let frame_b = image::open(b.join("frame-0.png"))?.to_rgba8();
    let delta = diff(&frame_a, &frame_b)?;
    let mut stdout = std::io::stdout();
    writeln!(
        stdout,
        "| Size | Max channel delta | Differing pixels |\n|---|---|---|\n| {}x{} | {} | {} |",
        frame_a.width(),
        frame_a.height(),
        delta.max_channel,
        delta.differing_pixels
    )?;
    Ok(())
}

fn diff(reference: &RgbaImage, frame: &RgbaImage) -> Result<Delta> {
    if reference.dimensions() != frame.dimensions() {
        return Err(format!(
            "frame sizes differ: {:?} and {:?}",
            reference.dimensions(),
            frame.dimensions()
        )
        .into());
    }
    let mut delta = Delta {
        max_channel: 0,
        differing_pixels: 0,
    };
    for (expected, actual) in reference.pixels().zip(frame.pixels()) {
        let channels = expected.0.iter().zip(actual.0);
        let pixel_max = channels.map(|(e, a)| e.abs_diff(a)).max().unwrap_or(0);
        if pixel_max > 0 {
            delta.max_channel = delta.max_channel.max(pixel_max);
            delta.differing_pixels += 1;
        }
    }
    Ok(delta)
}

fn probe_json(runs: &[Run]) -> Result<String> {
    let first = runs.first().ok_or("no runs")?;
    let info = &first.adapter;
    let mut json = format!(
        "{{\n  \"adapter\": {{\"name\": {}, \"backend\": {}, \"device_type\": {}, \"driver\": {}, \
         \"driver_info\": {}}},\n  \"iterations\": [",
        json_string(&info.name),
        json_string(&format!("{:?}", info.backend)),
        json_string(&format!("{:?}", info.device_type)),
        json_string(&info.driver),
        json_string(&info.driver_info),
    );
    for (index, run) in runs.iter().enumerate() {
        let delta = diff(&first.image, &run.image)?;
        write!(
            json,
            "{}\n    {{\"index\": {index}, \"setup_ms\": {:.1}, \"render_ms\": {:.1}, \
             \"sha256\": \"{}\", \"max_channel_delta\": {}, \"differing_pixels\": {}}}",
            if index == 0 { "" } else { "," },
            run.setup_ms,
            run.render_ms,
            sha256_hex(run.image.as_raw()),
            delta.max_channel,
            delta.differing_pixels,
        )?;
    }
    json.push_str("\n  ]\n}\n");
    Ok(json)
}

fn json_string(value: &str) -> String {
    let escaped: Vec<String> = value
        .chars()
        .map(|c| match c {
            '"' => "\\\"".to_owned(),
            '\\' => "\\\\".to_owned(),
            c if c.is_control() => format!("\\u{:04x}", u32::from(c)),
            c => c.to_string(),
        })
        .collect();
    format!("\"{}\"", escaped.concat())
}

fn sha256_hex(data: &[u8]) -> String {
    let bytes: Vec<String> = Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    bytes.concat()
}
