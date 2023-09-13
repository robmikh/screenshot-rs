use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Capture a window who's title contains the provided input.
    #[clap(short, long, conflicts_with = "monitor", conflicts_with = "primary")]
    window: Option<String>,

    /// The index of the monitor to screenshot.
    #[clap(short, long, conflicts_with = "window", conflicts_with = "primary")]
    monitor: Option<usize>,

    /// Capture the primary monitor (default if no params are specified).
    #[clap(short, long, conflicts_with = "window", conflicts_with = "monitor")]
    primary: bool,
}

pub enum CaptureMode {
    Window(String),
    Monitor(usize),
    Primary,
}

impl CaptureMode {
    pub fn from_args() -> Self {
        let args = Args::parse();

        if let Some(window_query) = args.window {
            CaptureMode::Window(window_query)
        } else if let Some(index) = args.monitor {
            CaptureMode::Monitor(index)
        } else {
            CaptureMode::Primary
        }
    }
}
