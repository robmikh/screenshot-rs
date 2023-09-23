mod capture;
mod cli;
mod d3d;
mod display_info;
mod window_info;

use cli::CaptureMode;
use windows::core::{ComInterface, IInspectable, Result, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapEncoder, BitmapPixelFormat};
use windows::Storage::{CreationCollisionOption, FileAccessMode, StorageFolder};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Resource, ID3D11Texture2D, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, HMONITOR, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::System::WinRT::{
    Graphics::Capture::IGraphicsCaptureItemInterop, RoInitialize, RO_INIT_MULTITHREADED,
};
use windows::Win32::UI::WindowsAndMessaging::{GetDesktopWindow, GetWindowThreadProcessId};

use capture::enumerate_capturable_windows;
use display_info::enumerate_displays;
use std::io::Write;
use std::sync::mpsc::channel;
use window_info::WindowInfo;

fn create_capture_item_for_window(window_handle: HWND) -> Result<GraphicsCaptureItem> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    unsafe { interop.CreateForWindow(window_handle) }
}

fn create_capture_item_for_monitor(monitor_handle: HMONITOR) -> Result<GraphicsCaptureItem> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    unsafe { interop.CreateForMonitor(monitor_handle) }
}

fn main() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }

    let mode = CaptureMode::from_args();

    let item = match mode {
        CaptureMode::Window(query) => {
            let window = get_window_from_query(&query)?;
            create_capture_item_for_window(window.handle)?
        }
        CaptureMode::Monitor(id) => {
            let displays = enumerate_displays()?;
            if id == 0 {
                println!("Invalid input, ids start with 1.");
                std::process::exit(1);
            }
            let index = id - 1;
            if index >= displays.len() {
                println!("Invalid input, id is higher than the number of displays!");
                std::process::exit(1);
            }
            let display = &displays[index];
            create_capture_item_for_monitor(display.handle)?
        }
        CaptureMode::Primary => {
            let monitor_handle =
                unsafe { MonitorFromWindow(GetDesktopWindow(), MONITOR_DEFAULTTOPRIMARY) };
            create_capture_item_for_monitor(monitor_handle)?
        }
    };

    take_screenshot(&item)?;

    Ok(())
}

fn take_screenshot(item: &GraphicsCaptureItem) -> Result<()> {
    let item_size = item.Size()?;

    let d3d_device = d3d::create_d3d_device()?;
    let d3d_context = unsafe { d3d_device.GetImmediateContext()? };
    let device = d3d::create_direct3d_device(&d3d_device)?;
    let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        item_size,
    )?;
    let session = frame_pool.CreateCaptureSession(item)?;

    let (sender, receiver) = channel();
    frame_pool.FrameArrived(
        &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new({
            move |frame_pool, _| {
                let frame_pool = frame_pool.as_ref().unwrap();
                let frame = frame_pool.TryGetNextFrame()?;
                sender.send(frame).unwrap();
                Ok(())
            }
        }),
    )?;
    session.StartCapture()?;

    let texture = unsafe {
        let frame = receiver.recv().unwrap();

        let source_texture: ID3D11Texture2D =
            d3d::get_d3d_interface_from_object(&frame.Surface()?)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        source_texture.GetDesc(&mut desc);
        desc.BindFlags = 0;
        desc.MiscFlags = 0;
        desc.Usage = D3D11_USAGE_STAGING;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        let copy_texture = {
            let mut texture = None;
            d3d_device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            texture.unwrap()
        };

        d3d_context.CopyResource(Some(&copy_texture.cast()?), Some(&source_texture.cast()?));

        session.Close()?;
        frame_pool.Close()?;

        copy_texture
    };

    let bits = unsafe {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc as *mut _);

        let resource: ID3D11Resource = texture.cast()?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        d3d_context.Map(
            Some(&resource.clone()),
            0,
            D3D11_MAP_READ,
            0,
            Some(&mut mapped),
        )?;

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

        d3d_context.Unmap(Some(&resource), 0);

        bits
    };

    let path = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let folder = StorageFolder::GetFolderFromPathAsync(&HSTRING::from(&path))?.get()?;
    let file = folder
        .CreateFileAsync(
            &HSTRING::from("screenshot.png"),
            CreationCollisionOption::ReplaceExisting,
        )?
        .get()?;

    {
        let stream = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;
        let encoder = BitmapEncoder::CreateAsync(BitmapEncoder::PngEncoderId()?, &stream)?.get()?;
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
    let window = if windows.is_empty() {
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
            unsafe { GetWindowThreadProcessId(window.handle, Some(&mut pid)) };
            println!("    {:>3}    {:>6}    {}", i, pid, window.title);
        }
        let index: usize;
        loop {
            print!("Please make a selection (q to quit): ");
            std::io::stdout().flush().unwrap();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            if input.to_lowercase().contains('q') {
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
