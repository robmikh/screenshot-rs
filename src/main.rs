mod capture;
mod cli;
mod d3d;
mod display_info;
mod wic;
mod window_info;

use cli::{Args, CaptureMode};
use wic::create_wic_factory;
use windows::core::{IInspectable, Interface, Result, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, HWND};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT,
};
use windows::Win32::Graphics::Gdi::{MonitorFromWindow, HMONITOR, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::Graphics::Imaging::{
    GUID_ContainerFormatPng, GUID_ContainerFormatWmp, GUID_WICPixelFormat32bppBGRA,
    GUID_WICPixelFormat64bppRGBAHalf, IWICImagingFactory, WICBitmapEncoderNoCache,
};
use windows::Win32::System::Com::{STGM_CREATE, STGM_READWRITE};
use windows::Win32::System::WinRT::{
    Graphics::Capture::IGraphicsCaptureItemInterop, RoInitialize, RO_INIT_MULTITHREADED,
};
use windows::Win32::UI::Shell::SHCreateStreamOnFileEx;
use windows::Win32::UI::WindowsAndMessaging::{GetDesktopWindow, GetWindowThreadProcessId};

use capture::enumerate_capturable_windows;
use display_info::enumerate_displays;
use std::io::Write;
use std::path::Path;
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

    let args = Args::parse_args();
    let mode = args.capture_mode();

    // Validate path and derive pixel format
    let pixel_format = if let Some(pixel_format) = validate_path(&args.output_file) {
        pixel_format
    } else {
        println!("Invalid file extension! Expecting 'png' or 'jxr'.");
        std::process::exit(1);
    };

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

    // Initialize D3D11
    let d3d_device = d3d::create_d3d_device()?;
    let d3d_context = unsafe { d3d_device.GetImmediateContext()? };

    // Initialize WIC
    let wic_factory = create_wic_factory()?;

    let texture = take_screenshot(&item, pixel_format, &d3d_device, &d3d_context)?;
    save_texture(&d3d_context, &texture, &wic_factory, &args.output_file)?;

    Ok(())
}

fn take_screenshot(
    item: &GraphicsCaptureItem,
    pixel_format: DirectXPixelFormat,
    d3d_device: &ID3D11Device,
    d3d_context: &ID3D11DeviceContext,
) -> Result<ID3D11Texture2D> {
    let item_size = item.Size()?;

    let device = d3d::create_direct3d_device(d3d_device)?;
    let frame_pool =
        Direct3D11CaptureFramePool::CreateFreeThreaded(&device, pixel_format, 1, item_size)?;
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

    Ok(texture)
}

fn get_bytes_from_texture(
    d3d_context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
) -> Result<(Vec<u8>, u32)> {
    unsafe {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc as *mut _);

        let bytes_per_pixel = match desc.Format {
            DXGI_FORMAT_B8G8R8A8_UNORM => 4,
            DXGI_FORMAT_R16G16B16A16_FLOAT => 8,
            _ => {
                return Err(windows::core::Error::new(
                    E_INVALIDARG,
                    "Unsupported pixel format!",
                ))
            }
        };

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

        let mut bytes = vec![0u8; (desc.Width * desc.Height * bytes_per_pixel) as usize];
        for row in 0..desc.Height {
            let data_begin = (row * (desc.Width * bytes_per_pixel)) as usize;
            let data_end = ((row + 1) * (desc.Width * bytes_per_pixel)) as usize;
            let slice_begin = (row * mapped.RowPitch) as usize;
            let slice_end = slice_begin + (desc.Width * bytes_per_pixel) as usize;
            bytes[data_begin..data_end].copy_from_slice(&slice[slice_begin..slice_end]);
        }

        d3d_context.Unmap(Some(&resource), 0);

        Ok((bytes, bytes_per_pixel))
    }
}

fn save_texture(
    d3d_context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
    wic_factory: &IWICImagingFactory,
    path: &str,
) -> Result<()> {
    let (width, height, container_format, pixel_format) = unsafe {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc as *mut _);
        let (container_format, pixel_format) = match desc.Format {
            DXGI_FORMAT_B8G8R8A8_UNORM => (GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA),
            DXGI_FORMAT_R16G16B16A16_FLOAT => {
                (GUID_ContainerFormatWmp, GUID_WICPixelFormat64bppRGBAHalf)
            }
            _ => {
                return Err(windows::core::Error::new(
                    E_INVALIDARG,
                    "Unsupported pixel format!",
                ))
            }
        };
        (desc.Width, desc.Height, container_format, pixel_format)
    };
    let (bytes, bytes_per_pixel) = get_bytes_from_texture(d3d_context, texture)?;
    let stride = bytes_per_pixel * width;

    let encoder = unsafe { wic_factory.CreateEncoder(&container_format, std::ptr::null())? };

    unsafe {
        let stream = {
            let path = HSTRING::from(path);
            SHCreateStreamOnFileEx(&path, (STGM_CREATE | STGM_READWRITE).0, 0, true, None)?
        };
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let (frame, props) = {
            let mut frame = None;
            let mut props = None;
            encoder.CreateNewFrame(&mut frame, &mut props)?;
            (frame.unwrap(), props.unwrap())
        };

        frame.Initialize(&props)?;
        frame.SetSize(width, height)?;
        let mut target_format = pixel_format;
        frame.SetPixelFormat(&mut target_format)?;
        if target_format != pixel_format {
            return Err(windows::core::Error::new(
                E_FAIL,
                "Unsupported WIC pixel format!",
            ));
        }

        // TODO: Metadata

        frame.WritePixels(height, stride, &bytes)?;
        frame.Commit()?;
        encoder.Commit()?;
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

fn validate_path<P: AsRef<Path>>(path: P) -> Option<DirectXPixelFormat> {
    let path = path.as_ref();
    let mut pixel_format = None;
    if let Some(extension) = path.extension() {
        if let Some(extension) = extension.to_str() {
            match extension {
                "png" => pixel_format = Some(DirectXPixelFormat::B8G8R8A8UIntNormalized),
                "jxr" => pixel_format = Some(DirectXPixelFormat::R16G16B16A16Float),
                _ => {}
            }
        }
    }
    pixel_format
}
