mod capture;
mod display_info;
mod hresult;
mod window_info;

use bindings::win_rt_interop_tools::{
    desktop::CaptureItemInterop, Direct3D11CpuAccessFlag, Direct3D11Device, Direct3D11Texture2D,
};
use bindings::windows::foundation::TypedEventHandler;
use bindings::windows::graphics::capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use bindings::windows::graphics::directx::{direct3d11::Direct3DUsage, DirectXPixelFormat};
use bindings::windows::graphics::imaging::{BitmapAlphaMode, BitmapEncoder, BitmapPixelFormat};
use bindings::windows::storage::{CreationCollisionOption, FileAccessMode, StorageFolder};
use capture::enumerate_capturable_windows;
use clap::{value_t, App, Arg};
use display_info::enumerate_displays;
use hresult::AsHresult;
use std::io::Write;
use std::sync::mpsc::channel;
use window_info::WindowInfo;
//use bindings::windows::win32::winrt::{RO_INIT_TYPE, RoInitialize};
use bindings::windows::win32::base::MONITOR_DEFAULTTOPRIMARY;
use bindings::windows::win32::com::CoInitializeEx;
use bindings::windows::win32::menu_rc::{
    GetDesktopWindow, GetWindowThreadProcessId, MonitorFromWindow,
};
use bindings::windows::win32::stg::COINIT;

fn main() -> winrt::Result<()> {
    unsafe {
        //RoInitialize(RO_INIT_TYPE::RO_INIT_MULTITHREADED).as_hresult()?;
        CoInitializeEx(
            std::ptr::null_mut(),
            std::mem::transmute::<_, u32>(COINIT::COINIT_MULTITHREADED),
        )
        .as_hresult()?;
    }

    // TODO: Make input optional for window and monitor (prompt)
    let matches = App::new("screenshot")
        .version("0.1.0")
        .author("Robert Mikhayelyan <rob.mikh@outlook.com>")
        .about("A demo that saves screenshots of windows or monitors using Windows.Graphics.Capture and Rust/WinRT.")
        .arg(Arg::with_name("window")
            .short("w")
            .long("window")
            .value_name("window title query")
            .help("Capture a window who's title contains the provided input")
            .conflicts_with_all(&["monitor", "primary"])
            .takes_value(true))
        .arg(Arg::with_name("monitor")
            .short("m")
            .long("monitor")
            .value_name("monitor number")
            .help("Capture a monitor")
            .conflicts_with_all(&["window", "primary"])
            .takes_value(true))
        .arg(Arg::with_name("primary")
            .short("p")
            .long("primary")
            .help("Capture the primary monitor (default if no params are specified)")
            .conflicts_with_all(&["window", "monitor"])
            .takes_value(false))
        .get_matches();

    let item = if matches.is_present("window") {
        let query = matches.value_of("window").unwrap();
        let window = get_window_from_query(query)?;
        CaptureItemInterop::create_for_window(window.handle as u64)?
    } else if matches.is_present("monitor") {
        let id = value_t!(matches, "monitor", usize).unwrap();
        let displays = enumerate_displays();
        if id == 0 {
            println!("Invalid input, ids start with 1.");
            std::process::exit(1);
        }
        let index = (id - 1) as usize;
        if index >= displays.len() {
            println!("Invalid input, id is higher than the number of displays!");
            std::process::exit(1);
        }
        let display = &displays[index];
        CaptureItemInterop::create_for_monitor(display.handle as u64)?
    } else if matches.is_present("primary") {
        let monitor_handle =
            unsafe { MonitorFromWindow(GetDesktopWindow(), MONITOR_DEFAULTTOPRIMARY as u32) };
        CaptureItemInterop::create_for_monitor(monitor_handle as u64)?
    } else {
        std::process::exit(0);
    };

    take_screenshot(&item)?;

    Ok(())
}

fn take_screenshot(item: &GraphicsCaptureItem) -> winrt::Result<()> {
    let item_size = item.size()?;

    let device = Direct3D11Device::new()?;
    let frame_pool = Direct3D11CaptureFramePool::create_free_threaded(
        &device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        &item_size,
    )?;
    let session = frame_pool.create_capture_session(item)?;

    let (sender, receiver) = channel();
    frame_pool.frame_arrived(
        TypedEventHandler::<Direct3D11CaptureFramePool, winrt::Object>::new({
            let device = device.clone();
            let session = session.clone();
            move |frame_pool, _| {
                let frame_pool = frame_pool.as_ref().unwrap();
                let frame = frame_pool.try_get_next_frame()?;
                let source_texture =
                    Direct3D11Texture2D::create_from_direct3d_surface(frame.surface()?)?;
                let mut desc = source_texture.description2d()?;
                desc.usage = Direct3DUsage::Staging;
                desc.cpu_access_flags = Direct3D11CpuAccessFlag::AccessRead;
                unsafe {
                    use std::mem::transmute;
                    desc.bind_flags = transmute(0);
                    desc.misc_flags = transmute(0);
                }
                let copy_texture = device.create_texture2d(desc)?;

                let context = device.immediate_context()?;
                context.copy_resource(&copy_texture, source_texture)?;

                session.close()?;
                frame_pool.close()?;

                sender.send(copy_texture).unwrap();
                Ok(())
            }
        }),
    )?;
    session.start_capture()?;

    let texture = receiver.recv().unwrap();
    let bits = texture.get_bytes()?;

    let path = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let folder = StorageFolder::get_folder_from_path_async(path.as_str())?.get()?;
    let file = folder
        .create_file_async("screenshot.png", CreationCollisionOption::ReplaceExisting)?
        .get()?;

    {
        let stream = file.open_async(FileAccessMode::ReadWrite)?.get()?;
        let encoder =
            BitmapEncoder::create_async(BitmapEncoder::png_encoder_id()?, stream)?.get()?;
        encoder.set_pixel_data(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            item_size.width as u32,
            item_size.height as u32,
            1.0,
            1.0,
            &bits,
        )?;

        encoder.flush_async()?.get()?;
    }

    Ok(())
}

fn get_window_from_query(query: &str) -> winrt::Result<WindowInfo> {
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
            unsafe { GetWindowThreadProcessId(window.handle, &mut pid).as_hresult()? };
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

fn find_window(window_name: &str) -> Vec<WindowInfo> {
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
