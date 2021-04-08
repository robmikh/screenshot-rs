use bindings::Windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use bindings::Windows::Win32::WinRT::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess, IInspectable,
};
use bindings::Windows::Win32::{
    Direct3D11::{
        D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
        D3D_DRIVER_TYPE,
    },
    Dxgi::{IDXGIDevice, DXGI_ERROR_UNSUPPORTED},
};
use windows::{Abi, ErrorCode, Interface};

fn create_d3d_device_with_type(
    driver_type: D3D_DRIVER_TYPE,
    flags: D3D11_CREATE_DEVICE_FLAG,
    device: *mut Option<ID3D11Device>,
) -> ErrorCode {
    unsafe {
        D3D11CreateDevice(
            None,
            driver_type,
            0,
            flags,
            std::ptr::null(),
            0,
            D3D11_SDK_VERSION as u32,
            device,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    }
}

pub fn create_d3d_device() -> windows::Result<ID3D11Device> {
    let mut device = None;
    let mut hresult = create_d3d_device_with_type(
        D3D_DRIVER_TYPE::D3D_DRIVER_TYPE_HARDWARE,
        D3D11_CREATE_DEVICE_FLAG::D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        &mut device,
    );
    if hresult.0 == DXGI_ERROR_UNSUPPORTED.0 as u32 {
        hresult = create_d3d_device_with_type(
            D3D_DRIVER_TYPE::D3D_DRIVER_TYPE_WARP,
            D3D11_CREATE_DEVICE_FLAG::D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            &mut device,
        );
    }
    hresult.ok()?;
    Ok(device.unwrap())
}

pub fn create_direct3d_device(d3d_device: &ID3D11Device) -> windows::Result<IDirect3DDevice> {
    let dxgi_device: IDXGIDevice = d3d_device.cast()?;
    let mut inspectable: Option<IInspectable> = None;
    unsafe {
        CreateDirect3D11DeviceFromDXGIDevice(Some(dxgi_device), &mut inspectable as *mut _).ok()?;
    }
    inspectable.unwrap().cast()
}

pub fn get_d3d_interface_from_object<S: Interface, R: Interface + Abi>(
    object: &S,
) -> windows::Result<R> {
    let access: IDirect3DDxgiInterfaceAccess = object.cast()?;
    let mut result: Option<R> = None;
    unsafe {
        access
            .GetInterface(&R::IID as *const _, result.set_abi())
            .ok()?;
    }
    Ok(result.unwrap())
}
