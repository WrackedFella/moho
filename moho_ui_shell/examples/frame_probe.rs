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
use std::time::Instant;

use egui::{Color32, vec2};
use egui_kittest::Harness;
use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};
use image::RgbaImage;
use moho_ui_shell::{Anchor, Console, MenuItem, MenuScreen, ScreenText, UiShell};

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

/// How far a frame is from a reference frame of the same size.
struct Delta {
    max_channel: u8,
    differing_pixels: usize,
}

/// One rendered frame and what it cost.
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

/// Builds a fresh adapter, device and harness, then paints the fixed content once.
fn render_frame() -> Result<Run> {
    let started = Instant::now();
    let state = create_render_state(
        default_wgpu_setup(),
        renderer_options(create_render_state, |options| {
            options.dithering = false;
            options.predictable_texture_filtering = true;
        }),
    );
    let adapter = state.adapter.get_info();
    let mut harness = Harness::builder()
        .with_size(vec2(SIZE[0], SIZE[1]))
        .with_pixels_per_point(1.0)
        .renderer(WgpuTestRenderer::from_render_state(state))
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

/// The renderer options `create` takes, from their default with `adjust` applied. egui-wgpu is
/// not a dependency here, so its `RendererOptions` cannot be named; `create` supplies the type.
fn renderer_options<Setup, Options: Default, State>(
    _create: fn(Setup, Options) -> State,
    adjust: impl FnOnce(&mut Options),
) -> Options {
    let mut options = Options::default();
    adjust(&mut options);
    options
}

/// The main menu with its first row focused, and a text in two opposite corners.
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

/// `probe.json`: the adapter of the first run, then each run's timings, hash and drift from the
/// first run's frame.
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

/// The first `N` fractional parts of `root` of the primes, scaled to 32 bits: how SHA-256 defines
/// its initial state (square roots) and round constants (cube roots).
fn fractional_roots<const N: usize>(root: fn(f64) -> f64) -> [u32; N] {
    let is_prime = |n: &u32| {
        (2..*n)
            .take_while(|d| d * d <= *n)
            .all(|d| !n.is_multiple_of(d))
    };
    let mut constants = [0; N];
    for (constant, prime) in constants.iter_mut().zip((2..).filter(is_prime)) {
        *constant = (root(f64::from(prime)).fract() * 4_294_967_296.0) as u32;
    }
    constants
}

/// SHA-256 of `data` as lower-case hex, so a reader can check two frames with `sha256sum` alike.
fn sha256_hex(data: &[u8]) -> String {
    let rounds: [u32; 64] = fractional_roots(f64::cbrt);
    let mut state: [u32; 8] = fractional_roots(f64::sqrt);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&(data.len() as u64 * 8).to_be_bytes());
    for block in padded.as_chunks::<64>().0 {
        let mut schedule = [0u32; 64];
        for (word, bytes) in schedule.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for i in 16..64 {
            let (w15, w2) = (schedule[i - 15], schedule[i - 2]);
            let small0 = w15.rotate_right(7) ^ w15.rotate_right(18) ^ (w15 >> 3);
            let small1 = w2.rotate_right(17) ^ w2.rotate_right(19) ^ (w2 >> 10);
            schedule[i] = schedule[i - 16]
                .wrapping_add(small0)
                .wrapping_add(schedule[i - 7])
                .wrapping_add(small1);
        }
        let mut v = state;
        for (round, word) in rounds.iter().zip(schedule) {
            let big1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let choose = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let temp1 = v[7]
                .wrapping_add(big1)
                .wrapping_add(choose)
                .wrapping_add(*round)
                .wrapping_add(word);
            let big0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let majority = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let temp2 = big0.wrapping_add(majority);
            v = [
                temp1.wrapping_add(temp2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(temp1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (slot, value) in state.iter_mut().zip(v) {
            *slot = slot.wrapping_add(value);
        }
    }
    state.map(|word| format!("{word:08x}")).concat()
}
