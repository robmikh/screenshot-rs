mod capture;
mod d3d;
mod display_info;
mod window_info;

use windows::runtime::{IInspectable, Interface, Result};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapEncoder, BitmapPixelFormat};
use windows::Storage::{CreationCollisionOption, FileAccessMode, StorageFolder};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Resource, ID3D11Texture2D, D3D11_BIND_FLAG, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ,
    D3D11_RESOURCE_MISC_FLAG, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, HMONITOR, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::System::WinRT::{
    IGraphicsCaptureItemInterop, RoInitialize, RO_INIT_MULTITHREADED,
};
use windows::Win32::UI::WindowsAndMessaging::{GetDesktopWindow, GetWindowThreadProcessId};

use capture::enumerate_capturable_windows;
use clap::{value_t, App, Arg};
use display_info::enumerate_displays;
use std::io::Write;
use std::sync::mpsc::channel;
use window_info::WindowInfo;

fn create_capture_item_for_window(window_handle: HWND) -> Result<GraphicsCaptureItem> {
    let interop = windows::runtime::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    unsafe { interop.CreateForWindow(window_handle) }
}

fn create_capture_item_for_monitor(monitor_handle: HMONITOR) -> Result<GraphicsCaptureItem> {
    let interop = windows::runtime::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    unsafe { interop.CreateForMonitor(monitor_handle) }
}

fn main() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
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
        create_capture_item_for_window(window.handle)?
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
        create_capture_item_for_monitor(display.handle)?
    } else if matches.is_present("primary") {
        let monitor_handle =
            unsafe { MonitorFromWindow(GetDesktopWindow(), MONITOR_DEFAULTTOPRIMARY) };
        create_capture_item_for_monitor(monitor_handle)?
    } else {
        std::process::exit(0);
    };

    take_screenshot(&item)?;

    Ok(())
}

fn take_screenshot(item: &GraphicsCaptureItem) -> Result<()> {
    let item_size = item.Size()?;

    let d3d_device = d3d::create_d3d_device()?;
    let d3d_context = unsafe {
        let mut d3d_context = None;
        d3d_device.GetImmediateContext(&mut d3d_context);
        d3d_context.unwrap()
    };
    let device = d3d::create_direct3d_device(&d3d_device)?;
    let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        &item_size,
    )?;
    let session = frame_pool.CreateCaptureSession(item)?;

    let (sender, receiver) = channel();
    frame_pool.FrameArrived(
        TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new({
            let d3d_device = d3d_device.clone();
            let d3d_context = d3d_context.clone();
            let session = session.clone();
            move |frame_pool, _| unsafe {
                let frame_pool = frame_pool.as_ref().unwrap();
                let frame = frame_pool.TryGetNextFrame()?;
                let source_texture: ID3D11Texture2D =
                    d3d::get_d3d_interface_from_object(&frame.Surface()?)?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                source_texture.GetDesc(&mut desc);
                desc.BindFlags = D3D11_BIND_FLAG(0);
                desc.MiscFlags = D3D11_RESOURCE_MISC_FLAG(0);
                desc.Usage = D3D11_USAGE_STAGING;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
                let copy_texture = { d3d_device.CreateTexture2D(&desc, std::ptr::null())? };

                d3d_context.CopyResource(Some(copy_texture.cast()?), Some(source_texture.cast()?));

                session.Close()?;
                frame_pool.Close()?;

                sender.send(copy_texture).unwrap();
                Ok(())
            }
        }),
    )?;
    session.StartCapture()?;

    let texture = receiver.recv().unwrap();
    let bits = unsafe {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc as *mut _);

        let resource: ID3D11Resource = texture.cast()?;
        let mapped = d3d_context.Map(Some(resource.clone()), 0, D3D11_MAP_READ, 0)?;

        // Get a slice of bytes
        let slice: &[u8] = {
            std::slice::from_raw_parts(
                mapped.pData as *const _,
                (desc.Height * mapped.RowPitch) as usize,
            )
        };

        let bytes_per_pixel = 4;
        let mut bits = vec![0u8; (desc.Width * desc.Height * bytes_per_pixel) as usize];
        for row in 0..desc.Height {
            let data_begin = (row * (desc.Width * bytes_per_pixel)) as usize;
            let data_end = ((row + 1) * (desc.Width * bytes_per_pixel)) as usize;
            let slice_begin = (row * mapped.RowPitch) as usize;
            let slice_end = slice_begin + (desc.Width * bytes_per_pixel) as usize;
            bits[data_begin..data_end].copy_from_slice(&slice[slice_begin..slice_end]);
        }

        d3d_context.Unmap(Some(resource), 0);

        bits
    };

    let path = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let folder = StorageFolder::GetFolderFromPathAsync(path.as_str())?.get()?;
    let file = folder
        .CreateFileAsync("screenshot.png", CreationCollisionOption::ReplaceExisting)?
        .get()?;

    {
        let stream = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;
        let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId()?, stream)?.get()?;
        encoder.SetPixelData(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            item_size.Width as u32,
            item_size.Height as u32,
            1.0,
            1.0,
            &bits,
        )?;

        encoder.FlushAsync()?.get()?;
    }

    Ok(())
}

fn get_window_from_query(query: &str) -> Result<WindowInfo> {
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
            unsafe { GetWindowThreadProcessId(window.handle, &mut pid) };
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
