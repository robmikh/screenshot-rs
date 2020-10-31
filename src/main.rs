winrt::include_bindings!();

mod capture;
mod dwmapi;
mod hresult;
mod user;
mod window_info;

use capture::enumerate_capturable_windows;
use hresult::AsHresult;
use std::io::Write;
use win_rt_interop_tools::Direct3D11Device;
use window_info::WindowInfo;

fn main() -> winrt::Result<()> {
    unsafe {
        win32::RoInitialize(win32::RO_INIT_TYPE::RO_INIT_MULTITHREADED).as_hresult()?;
    }

    if let Some(query) = std::env::args().nth(1) {
        let _window = get_window_from_query(&query)?;
        let _device = Direct3D11Device::new()?;
    } else {
        println!("No window query given!");
    }

    Ok(())
}

pub fn get_window_from_query(query: &str) -> winrt::Result<WindowInfo> {
    let windows = find_window(query);
    let window = if windows.len() == 0 {
        println!("No window matching '{}' found!", query);
        std::process::exit(1);
    } else if windows.len() == 1 {
        &windows[0]
    } else {
        println!(
            "{} windows found matching '{}', please select one:",
            windows.len(),
            query
        );
        println!("    Num       PID    Window Title");
        for (i, window) in windows.iter().enumerate() {
            let mut pid = 0;
            unsafe { win32::GetWindowThreadProcessId(window.handle, &mut pid).as_hresult()? };
            println!("    {:>3}    {:>6}    {}", i, pid, window.title);
        }
        let index: usize;
        loop {
            print!("Please make a selection (q to quit): ");
            std::io::stdout().flush().unwrap();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            if input.to_lowercase().contains("q") {
                std::process::exit(0);
            }
            let input = input.trim();
            let selection: Option<usize> = match input.parse::<usize>() {
                Ok(selection) => {
                    if selection < windows.len() {
                        Some(selection)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(selection) = selection {
                index = selection;
                break;
            } else {
                println!("Invalid input, '{}'!", input);
                continue;
            };
        }
        &windows[index]
    };

    Ok(window.clone())
}

pub fn find_window(window_name: &str) -> Vec<WindowInfo> {
    let window_list = enumerate_capturable_windows();
    let mut windows: Vec<WindowInfo> = Vec::new();
    for window_info in window_list.into_iter() {
        let title = window_info.title.to_lowercase();
        if title.contains(&window_name.to_string().to_lowercase()) {
            windows.push(window_info.clone());
        }
    }
    windows
}
