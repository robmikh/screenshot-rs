mod capture;
mod dwmapi;
mod user;
mod window_info;

use capture::enumerate_capturable_windows;

fn main() {
    let windows = enumerate_capturable_windows();

    for window in windows.into_iter() {
        println!("{}", window.title);
    }
}
