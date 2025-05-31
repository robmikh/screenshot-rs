use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    /// Capture a window who's title contains the provided input.
    #[clap(short, long, conflicts_with = "monitor", conflicts_with = "primary")]
    window: Option<String>,

    /// The index of the monitor to screenshot.
    #[clap(short, long, conflicts_with = "window", conflicts_with = "primary")]
    monitor: Option<usize>,

    /// Capture the primary monitor (default if no params are specified).
    #[clap(short, long, conflicts_with = "window", conflicts_with = "monitor")]
    primary: bool,

    /// The output file that will contain the screenshot.
    #[clap(default_value = "screenshot.png")]
    pub output_file: String,
}

pub enum CaptureMode {
    Window(String),
    Monitor(usize),
    Primary,
}

impl Args {
    pub fn parse_args() -> Self {
        Self::parse()
    }

    pub fn capture_mode(&self) -> CaptureMode {
        if let Some(window_query) = self.window.as_ref() {
            CaptureMode::Window(window_query.clone())
        } else if let Some(index) = self.monitor {
            CaptureMode::Monitor(index)
        } else {
            CaptureMode::Primary
        }
    }
}
