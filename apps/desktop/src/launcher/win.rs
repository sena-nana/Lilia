use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, COLORREF, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, IsWindow, RegisterClassW,
    ShowWindow, UpdateLayeredWindow, CS_HREDRAW, CS_VREDRAW, SM_CXSCREEN, SM_CYSCREEN,
    SW_SHOWNOACTIVATE, ULW_ALPHA, WM_CLOSE, WM_DESTROY, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

use super::{ICON_128, ICON_256};

pub fn create() -> Result<isize, String> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let png = if GetDpiForSystem() >= 144 {
            ICON_256
        } else {
            ICON_128
        };
        let (pixels, width, height) =
            decode_icon(png).map_err(|error| format!("cannot decode application icon: {error}"))?;
        let instance = GetModuleHandleW(None)
            .map_err(|error| format!("cannot read launcher module: {error}"))?;
        let class_name = HSTRING::from("LiliaCode.StartupIcon");
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(startup_window_proc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err("cannot register startup window".to_owned());
        }
        let window = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            &class_name,
            &HSTRING::from("LiliaCode"),
            WS_POPUP,
            (GetSystemMetrics(SM_CXSCREEN) - width as i32) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - height as i32) / 2,
            width as i32,
            height as i32,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|error| format!("cannot create startup window: {error}"))?;
        present_icon(window, &pixels, width, height)?;
        let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
        Ok(window.0 as isize)
    }
}

pub fn close(handle: isize) {
    unsafe {
        let window = HWND(handle as *mut core::ffi::c_void);
        if IsWindow(Some(window)).as_bool() {
            let _ = DestroyWindow(window);
        }
    }
}

fn decode_icon(png: &[u8]) -> windows::core::Result<(Vec<u8>, u32, u32)> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromMemory(png)?;
        let decoder = factory.CreateDecoderFromStream(
            &stream,
            std::ptr::null(),
            WICDecodeMetadataCacheOnDemand,
        )?;
        let frame = decoder.GetFrame(0)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        let mut width = 0;
        let mut height = 0;
        converter.GetSize(&mut width, &mut height)?;
        let mut pixels = vec![0u8; width.saturating_mul(height).saturating_mul(4) as usize];
        converter.CopyPixels(std::ptr::null(), width.saturating_mul(4), &mut pixels)?;
        Ok((pixels, width, height))
    }
}

unsafe fn present_icon(window: HWND, pixels: &[u8], width: u32, height: u32) -> Result<(), String> {
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                biSizeImage: pixels.len() as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|error| format!("cannot create startup bitmap: {error}"))?;
        if !bits.is_null() && !pixels.is_empty() {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
        }
        let memory = CreateCompatibleDC(None);
        let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
        let mut origin = POINT::default();
        let mut size = SIZE {
            cx: width as i32,
            cy: height as i32,
        };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let presented = UpdateLayeredWindow(
            window,
            None,
            None,
            Some(&mut size),
            Some(memory),
            Some(&mut origin),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        let _ = SelectObject(memory, previous);
        let _ = DeleteDC(memory);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        presented.map_err(|error| format!("cannot show application icon: {error}"))
    }
}

unsafe extern "system" fn startup_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CLOSE => unsafe {
            let _ = DestroyWindow(window);
            LRESULT(0)
        },
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}
