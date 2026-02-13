use clap::{Parser, ValueEnum};
use electron_beam::{BeamMode, RenderOptions, create_gif_from_image};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Generate an ElectronBeam-style GIF from a static image"
)]
struct Cli {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,

    #[arg(long, value_enum, default_value_t = ModeArg::CoolDown)]
    mode: ModeArg,

    #[arg(long, default_value_t = 48)]
    frames: usize,

    #[arg(long, default_value_t = 50)]
    fps: u16,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ModeArg {
    WarmUp,
    CoolDown,
    Fade,
    ScaleDown,
}

impl From<ModeArg> for BeamMode {
    fn from(value: ModeArg) -> Self {
        match value {
            ModeArg::WarmUp => BeamMode::WarmUp,
            ModeArg::CoolDown => BeamMode::CoolDown,
            ModeArg::Fade => BeamMode::Fade,
            ModeArg::ScaleDown => BeamMode::ScaleDown,
        }
    }
}

fn main() {
    let cli = Cli::parse();

    let frame_delay_cs = ((100.0 / cli.fps.max(1) as f32).round() as u16).max(1);
    let options = RenderOptions {
        mode: cli.mode.into(),
        frames: cli.frames,
        frame_delay_cs,
    };

    if let Err(err) = create_gif_from_image(&cli.input, &cli.output, &options) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }

    println!(
        "Wrote {} frames to {} (mode: {:?}, fps: {})",
        options.frames,
        cli.output.display(),
        options.mode,
        cli.fps
    );
}
