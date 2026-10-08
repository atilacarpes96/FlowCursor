// Declarações das funções do Windows usadas pelo FlowCursor.
// Ligadas com raw-dylib: não precisam das .lib do Windows SDK nem de crates.
#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms, dead_code)]

use std::ffi::c_void;

pub type BOOL = i32;
pub type HANDLE = isize;
pub type HWND = isize;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type WNDPROC = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;
pub type MONITORENUMPROC = unsafe extern "system" fn(HANDLE, HANDLE, *mut RECT, LPARAM) -> BOOL;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SIZE {
    pub cx: i32,
    pub cy: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CURSORINFO {
    pub cbSize: u32,
    pub flags: u32,
    pub hCursor: HANDLE,
    pub ptScreenPos: POINT,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ICONINFO {
    pub fIcon: BOOL,
    pub xHotspot: u32,
    pub yHotspot: u32,
    pub hbmMask: HANDLE,
    pub hbmColor: HANDLE,
}

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: Option<WNDPROC>,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HANDLE,
    pub hIcon: HANDLE,
    pub hCursor: HANDLE,
    pub hbrBackground: HANDLE,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HANDLE,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
    pub lPrivate: u32,
}

#[repr(C)]
pub struct BLENDFUNCTION {
    pub BlendOp: u8,
    pub BlendFlags: u8,
    pub SourceConstantAlpha: u8,
    pub AlphaFormat: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BITMAPINFOHEADER {
    pub biSize: u32,
    pub biWidth: i32,
    pub biHeight: i32,
    pub biPlanes: u16,
    pub biBitCount: u16,
    pub biCompression: u32,
    pub biSizeImage: u32,
    pub biXPelsPerMeter: i32,
    pub biYPelsPerMeter: i32,
    pub biClrUsed: u32,
    pub biClrImportant: u32,
}

#[repr(C)]
pub struct BITMAPINFO {
    pub bmiHeader: BITMAPINFOHEADER,
    pub bmiColors: [u32; 1],
}

#[repr(C)]
pub struct BITMAP {
    pub bmType: i32,
    pub bmWidth: i32,
    pub bmHeight: i32,
    pub bmWidthBytes: i32,
    pub bmPlanes: u16,
    pub bmBitsPixel: u16,
    pub bmBits: *mut c_void,
}

#[repr(C)]
pub struct MONITORINFOEXW {
    pub cbSize: u32,
    pub rcMonitor: RECT,
    pub rcWork: RECT,
    pub dwFlags: u32,
    pub szDevice: [u16; 32],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: u32,
    pub hWnd: HWND,
    pub uID: u32,
    pub uFlags: u32,
    pub uCallbackMessage: u32,
    pub hIcon: HANDLE,
    pub szTip: [u16; 128],
    pub dwState: u32,
    pub dwStateMask: u32,
    pub szInfo: [u16; 256],
    pub uVersion: u32,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: u32,
    pub guidItem: GUID,
    pub hBalloonIcon: HANDLE,
}

#[repr(C)]
pub struct PROCESS_POWER_THROTTLING_STATE {
    pub Version: u32,
    pub ControlMask: u32,
    pub StateMask: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct LUID {
    pub LowPart: u32,
    pub HighPart: i32,
}

#[repr(C)]
#[derive(Default)]
pub struct D3DKMT_OPENADAPTERFROMHDC {
    pub hDc: HANDLE,
    pub hAdapter: u32,
    pub AdapterLuid: LUID,
    pub VidPnSourceId: u32,
}

#[repr(C)]
pub struct D3DKMT_WAITFORVERTICALBLANKEVENT {
    pub hAdapter: u32,
    pub hDevice: u32,
    pub VidPnSourceId: u32,
}

#[repr(C)]
pub struct D3DKMT_CLOSEADAPTER {
    pub hAdapter: u32,
}

// Conferência dos tamanhos em 64 bits: um erro aqui quebra a chamada em silêncio.
const _: () = assert!(std::mem::size_of::<CURSORINFO>() == 24);
const _: () = assert!(std::mem::size_of::<ICONINFO>() == 32);
const _: () = assert!(std::mem::size_of::<WNDCLASSEXW>() == 80);
const _: () = assert!(std::mem::size_of::<MSG>() == 48);
const _: () = assert!(std::mem::size_of::<BITMAPINFOHEADER>() == 40);
const _: () = assert!(std::mem::size_of::<BITMAP>() == 32);
const _: () = assert!(std::mem::size_of::<MONITORINFOEXW>() == 104);
const _: () = assert!(std::mem::size_of::<NOTIFYICONDATAW>() == 976);
const _: () = assert!(std::mem::size_of::<D3DKMT_OPENADAPTERFROMHDC>() == 24);

// Mensagens
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_QUERYENDSESSION: u32 = 0x0011;
pub const WM_ENDSESSION: u32 = 0x0016;
pub const WM_SETTINGCHANGE: u32 = 0x001A;
pub const WM_DISPLAYCHANGE: u32 = 0x007E;
pub const WM_NCHITTEST: u32 = 0x0084;
pub const WM_COMMAND: u32 = 0x0111;
pub const WM_TIMER: u32 = 0x0113;
pub const WM_LBUTTONDBLCLK: u32 = 0x0203;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_HOTKEY: u32 = 0x0312;
pub const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
pub const WM_APP: u32 = 0x8000;
pub const HTTRANSPARENT: LRESULT = -1;

// Estilos de janela
pub const WS_POPUP: u32 = 0x8000_0000;
pub const WS_EX_TOPMOST: u32 = 0x0000_0008;
pub const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
pub const HWND_TOPMOST: HWND = -1;
pub const SWP_NOSIZE: u32 = 0x0001;
pub const SWP_NOMOVE: u32 = 0x0002;
pub const SWP_NOACTIVATE: u32 = 0x0010;
pub const SWP_NOOWNERZORDER: u32 = 0x0200;
pub const SWP_NOSENDCHANGING: u32 = 0x0400;
pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNORMAL: i32 = 1;
pub const SW_SHOWNOACTIVATE: i32 = 4;
pub const GW_HWNDPREV: u32 = 3;
pub const GA_ROOT: u32 = 2;
pub const PM_REMOVE: u32 = 0x0001;
pub const ULW_ALPHA: u32 = 0x0000_0002;
pub const AC_SRC_ALPHA: u8 = 0x01;
pub const DWMWA_TRANSITIONS_FORCEDISABLED: u32 = 3;
pub const DWMWA_CLOAKED: u32 = 14;

// Cursores do sistema (os IDC_ têm os mesmos números)
pub const OCR_NORMAL: u32 = 32512;
pub const OCR_IBEAM: u32 = 32513;
pub const OCR_WAIT: u32 = 32514;
pub const OCR_CROSS: u32 = 32515;
pub const OCR_UP: u32 = 32516;
pub const OCR_SIZENWSE: u32 = 32642;
pub const OCR_SIZENESW: u32 = 32643;
pub const OCR_SIZEWE: u32 = 32644;
pub const OCR_SIZENS: u32 = 32645;
pub const OCR_SIZEALL: u32 = 32646;
pub const OCR_NO: u32 = 32648;
pub const OCR_HAND: u32 = 32649;
pub const OCR_APPSTARTING: u32 = 32650;
pub const OCR_HELP: u32 = 32651;
pub const IDI_APPLICATION: usize = 32512;
pub const CURSOR_SHOWING: u32 = 0x0000_0001;
pub const SPI_SETCURSORS: u32 = 0x0057;

// Monitores e DPI
pub const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
pub const MONITOR_DEFAULTTONEAREST: u32 = 2;
pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
pub const MDT_EFFECTIVE_DPI: i32 = 0;

// Teclado e mouse
pub const VK_LBUTTON: i32 = 0x01;
pub const VK_RBUTTON: i32 = 0x02;
pub const VK_F9: u32 = 0x78;
pub const VK_F10: u32 = 0x79;
pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_NOREPEAT: u32 = 0x4000;

// Bandeja e menu
pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIF_MESSAGE: u32 = 0x01;
pub const NIF_ICON: u32 = 0x02;
pub const NIF_TIP: u32 = 0x04;
pub const NIF_INFO: u32 = 0x10;
pub const NIIF_INFO: u32 = 0x01;
pub const MF_STRING: u32 = 0x0000;
pub const MF_SEPARATOR: u32 = 0x0800;
pub const MF_CHECKED: u32 = 0x0008;
pub const TPM_RIGHTBUTTON: u32 = 0x0002;
pub const TPM_NONOTIFY: u32 = 0x0080;
pub const TPM_RETURNCMD: u32 = 0x0100;
pub const SM_CXSMICON: i32 = 49;

// Processos, sessão e energia
pub const ERROR_ALREADY_EXISTS: u32 = 183;
pub const SYNCHRONIZE: u32 = 0x0010_0000;
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const INFINITE: u32 = 0xFFFF_FFFF;
pub const WAIT_OBJECT_0: u32 = 0;
pub const WAIT_ABANDONED: u32 = 0x80;
pub const THREAD_PRIORITY_HIGHEST: i32 = 2;
pub const PROCESS_POWER_THROTTLING_CURRENT_VERSION: u32 = 1;
pub const PROCESS_POWER_THROTTLING_EXECUTION_SPEED: u32 = 0x1;
pub const PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION: u32 = 0x4;
pub const PROCESS_POWER_THROTTLING_CLASS: i32 = 4; // ProcessPowerThrottling
pub const NOTIFY_FOR_THIS_SESSION: u32 = 0;
pub const WTS_SESSION_LOCK: usize = 0x7;
pub const WTS_SESSION_UNLOCK: usize = 0x8;
pub const DESKTOP_SWITCHDESKTOP: u32 = 0x0100;
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    pub fn GetCursorPos(p: *mut POINT) -> BOOL;
    pub fn GetCursorInfo(pci: *mut CURSORINFO) -> BOOL;
    pub fn LoadCursorW(inst: HANDLE, name: *const u16) -> HANDLE;
    pub fn LoadIconW(inst: HANDLE, name: *const u16) -> HANDLE;
    pub fn CreateCursor(
        inst: HANDLE,
        x_hotspot: i32,
        y_hotspot: i32,
        w: i32,
        h: i32,
        and_plane: *const c_void,
        xor_plane: *const c_void,
    ) -> HANDLE;
    pub fn DestroyCursor(h: HANDLE) -> BOOL;
    pub fn SetSystemCursor(h: HANDLE, id: u32) -> BOOL;
    pub fn SystemParametersInfoW(action: u32, uiparam: u32, pv: *mut c_void, winini: u32) -> BOOL;
    pub fn GetIconInfo(h: HANDLE, info: *mut ICONINFO) -> BOOL;
    pub fn CreateIconIndirect(info: *const ICONINFO) -> HANDLE;
    pub fn DestroyIcon(h: HANDLE) -> BOOL;
    pub fn RegisterClassExW(wc: *const WNDCLASSEXW) -> u16;
    pub fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HANDLE,
        inst: HANDLE,
        param: *const c_void,
    ) -> HWND;
    pub fn DestroyWindow(h: HWND) -> BOOL;
    pub fn DefWindowProcW(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    pub fn GetMessageW(msg: *mut MSG, h: HWND, min: u32, max: u32) -> BOOL;
    pub fn PeekMessageW(msg: *mut MSG, h: HWND, min: u32, max: u32, remove: u32) -> BOOL;
    pub fn TranslateMessage(msg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(msg: *const MSG) -> LRESULT;
    pub fn PostMessageW(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> BOOL;
    pub fn PostQuitMessage(code: i32);
    pub fn UpdateLayeredWindow(
        h: HWND,
        hdc_dst: HANDLE,
        ppt_dst: *const POINT,
        psize: *const SIZE,
        hdc_src: HANDLE,
        ppt_src: *const POINT,
        cr_key: u32,
        pblend: *const BLENDFUNCTION,
        flags: u32,
    ) -> BOOL;
    pub fn SetWindowPos(h: HWND, after: HWND, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> BOOL;
    pub fn ShowWindow(h: HWND, cmd: i32) -> BOOL;
    pub fn IsWindowVisible(h: HWND) -> BOOL;
    pub fn GetWindow(h: HWND, cmd: u32) -> HWND;
    pub fn GetAncestor(h: HWND, flags: u32) -> HWND;
    pub fn WindowFromPoint(pt: POINT) -> HWND;
    pub fn GetForegroundWindow() -> HWND;
    pub fn SetForegroundWindow(h: HWND) -> BOOL;
    pub fn GetWindowRect(h: HWND, r: *mut RECT) -> BOOL;
    pub fn GetClassNameW(h: HWND, buf: *mut u16, max: i32) -> i32;
    pub fn GetWindowThreadProcessId(h: HWND, pid: *mut u32) -> u32;
    pub fn FindWindowW(class: *const u16, name: *const u16) -> HWND;
    pub fn MonitorFromWindow(h: HWND, flags: u32) -> HANDLE;
    pub fn MonitorFromPoint(pt: POINT, flags: u32) -> HANDLE;
    pub fn GetMonitorInfoW(mon: HANDLE, mi: *mut MONITORINFOEXW) -> BOOL;
    pub fn EnumDisplayMonitors(hdc: HANDLE, clip: *const RECT, cb: Option<MONITORENUMPROC>, data: LPARAM) -> BOOL;
    pub fn RegisterHotKey(h: HWND, id: i32, mods: u32, vk: u32) -> BOOL;
    pub fn UnregisterHotKey(h: HWND, id: i32) -> BOOL;
    pub fn GetAsyncKeyState(vk: i32) -> i16;
    pub fn GetSystemMetrics(i: i32) -> i32;
    pub fn SetProcessDpiAwarenessContext(ctx: isize) -> BOOL;
    pub fn OpenInputDesktop(flags: u32, inherit: BOOL, access: u32) -> HANDLE;
    pub fn CloseDesktop(h: HANDLE) -> BOOL;
    pub fn CreatePopupMenu() -> HANDLE;
    pub fn AppendMenuW(menu: HANDLE, flags: u32, id: usize, text: *const u16) -> BOOL;
    pub fn TrackPopupMenu(menu: HANDLE, flags: u32, x: i32, y: i32, reserved: i32, h: HWND, r: *const RECT) -> BOOL;
    pub fn DestroyMenu(menu: HANDLE) -> BOOL;
    pub fn RegisterWindowMessageW(s: *const u16) -> u32;
    pub fn SetTimer(h: HWND, id: usize, ms: u32, f: *const c_void) -> usize;
    pub fn MessageBoxW(h: HWND, text: *const u16, caption: *const u16, kind: u32) -> i32;
    pub fn GetDC(h: HWND) -> HANDLE;
    pub fn ReleaseDC(h: HWND, dc: HANDLE) -> i32;
}

#[link(name = "gdi32", kind = "raw-dylib")]
extern "system" {
    pub fn CreateCompatibleDC(dc: HANDLE) -> HANDLE;
    pub fn CreateDCW(driver: *const u16, device: *const u16, port: *const u16, dm: *const c_void) -> HANDLE;
    pub fn DeleteDC(dc: HANDLE) -> BOOL;
    pub fn CreateDIBSection(
        dc: HANDLE,
        bmi: *const BITMAPINFO,
        usage: u32,
        bits: *mut *mut c_void,
        section: HANDLE,
        offset: u32,
    ) -> HANDLE;
    pub fn CreateBitmap(w: i32, h: i32, planes: u32, bpp: u32, bits: *const c_void) -> HANDLE;
    pub fn SelectObject(dc: HANDLE, obj: HANDLE) -> HANDLE;
    pub fn DeleteObject(obj: HANDLE) -> BOOL;
    pub fn GetObjectW(h: HANDLE, c: i32, pv: *mut c_void) -> i32;
    pub fn GetBitmapBits(h: HANDLE, cb: i32, bits: *mut c_void) -> i32;
    pub fn D3DKMTOpenAdapterFromHdc(p: *mut D3DKMT_OPENADAPTERFROMHDC) -> i32;
    pub fn D3DKMTWaitForVerticalBlankEvent(p: *const D3DKMT_WAITFORVERTICALBLANKEVENT) -> i32;
    pub fn D3DKMTCloseAdapter(p: *const D3DKMT_CLOSEADAPTER) -> i32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    pub fn GetModuleHandleW(name: *const u16) -> HANDLE;
    pub fn CreateMutexW(attr: *const c_void, owner: BOOL, name: *const u16) -> HANDLE;
    pub fn ReleaseMutex(h: HANDLE) -> BOOL;
    pub fn GetLastError() -> u32;
    pub fn OpenProcess(access: u32, inherit: BOOL, pid: u32) -> HANDLE;
    pub fn WaitForSingleObject(h: HANDLE, ms: u32) -> u32;
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn QueryFullProcessImageNameW(h: HANDLE, flags: u32, buf: *mut u16, size: *mut u32) -> BOOL;
    pub fn GetCurrentThread() -> HANDLE;
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn SetThreadPriority(h: HANDLE, p: i32) -> BOOL;
    pub fn SetProcessInformation(h: HANDLE, class: i32, info: *const c_void, size: u32) -> BOOL;
}

#[link(name = "dwmapi", kind = "raw-dylib")]
extern "system" {
    pub fn DwmFlush() -> i32;
    pub fn DwmSetWindowAttribute(h: HWND, attr: u32, pv: *const c_void, cb: u32) -> i32;
    pub fn DwmGetWindowAttribute(h: HWND, attr: u32, pv: *mut c_void, cb: u32) -> i32;
}

#[link(name = "shcore", kind = "raw-dylib")]
extern "system" {
    pub fn GetDpiForMonitor(mon: HANDLE, kind: i32, x: *mut u32, y: *mut u32) -> i32;
}

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    pub fn Shell_NotifyIconW(msg: u32, data: *const NOTIFYICONDATAW) -> BOOL;
    pub fn ShellExecuteW(
        h: HWND,
        op: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> HANDLE;
}

#[link(name = "wtsapi32", kind = "raw-dylib")]
extern "system" {
    pub fn WTSRegisterSessionNotification(h: HWND, flags: u32) -> BOOL;
    pub fn WTSUnRegisterSessionNotification(h: HWND) -> BOOL;
}

pub const HKEY_CURRENT_USER: HANDLE = 0x8000_0001u32 as i32 as isize; // estendido com sinal, como no SDK
pub const KEY_QUERY_VALUE: u32 = 0x0001;
pub const KEY_SET_VALUE: u32 = 0x0002;
pub const REG_SZ: u32 = 1;
pub const SW_RESTORE: i32 = 9;

#[link(name = "advapi32", kind = "raw-dylib")]
extern "system" {
    pub fn RegOpenKeyExW(chave: HANDLE, sub: *const u16, opcoes: u32, acesso: u32, saida: *mut HANDLE) -> i32;
    pub fn RegSetValueExW(chave: HANDLE, nome: *const u16, reservado: u32, tipo: u32, dados: *const u8, tam: u32) -> i32;
    pub fn RegQueryValueExW(chave: HANDLE, nome: *const u16, reservado: *mut u32, tipo: *mut u32, dados: *mut u8, tam: *mut u32) -> i32;
    pub fn RegDeleteValueW(chave: HANDLE, nome: *const u16) -> i32;
    pub fn RegCloseKey(chave: HANDLE) -> i32;
}

pub const GWL_STYLE: i32 = -16;
pub const GWL_EXSTYLE: i32 = -20;
pub const WS_CAPTION: u32 = 0x00C0_0000;

#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    pub fn IsZoomed(h: HWND) -> BOOL;
    pub fn GetWindowLongW(h: HWND, indice: i32) -> i32;
    pub fn IsIconic(h: HWND) -> BOOL;
    pub fn LoadImageW(inst: HANDLE, nome: *const u16, tipo: u32, cx: i32, cy: i32, flags: u32) -> HANDLE;
}

// ---------- Instalador ----------

pub const REG_DWORD: u32 = 4;
pub const KEY_ALL_ACCESS: u32 = 0xF003F;
pub const IMAGE_ICON: u32 = 1;
pub const SM_CXICON: i32 = 11;
pub const COINIT_APARTMENTTHREADED: u32 = 0x2;
pub const CLSCTX_INPROC_SERVER: u32 = 0x1;
pub const MB_OK: u32 = 0x0;
pub const MB_OKCANCEL: u32 = 0x1;
pub const MB_ICONINFORMATION: u32 = 0x40;
pub const IDOK: i32 = 1;
pub const IDCANCEL: i32 = 2;

// Diálogo de tarefa (TaskDialogIndirect). As duas estruturas são empacotadas
// em 1 byte no commctrl.h, por isso o `packed`.
pub const TDF_ENABLE_HYPERLINKS: u32 = 0x0001;
pub const TDF_USE_HICON_MAIN: u32 = 0x0002;
pub const TDF_ALLOW_DIALOG_CANCELLATION: u32 = 0x0008;
pub const TDF_USE_COMMAND_LINKS: u32 = 0x0010;
pub const TDF_VERIFICATION_FLAG_CHECKED: u32 = 0x0100;
pub const TDF_SIZE_TO_CONTENT: u32 = 0x0100_0000;
pub const TDCBF_OK_BUTTON: u32 = 0x0001;
pub const TDCBF_CANCEL_BUTTON: u32 = 0x0008;
pub const TDCBF_CLOSE_BUTTON: u32 = 0x0020;
pub const TD_INFORMATION_ICON: isize = -3;
pub const TD_ERROR_ICON: isize = -2;

#[repr(C, packed(1))]
pub struct TASKDIALOG_BUTTON {
    pub nButtonID: i32,
    pub pszButtonText: *const u16,
}

#[repr(C, packed(1))]
pub struct TASKDIALOGCONFIG {
    pub cbSize: u32,
    pub hwndParent: HWND,
    pub hInstance: HANDLE,
    pub dwFlags: u32,
    pub dwCommonButtons: u32,
    pub pszWindowTitle: *const u16,
    pub hMainIcon: isize,
    pub pszMainInstruction: *const u16,
    pub pszContent: *const u16,
    pub cButtons: u32,
    pub pButtons: *const TASKDIALOG_BUTTON,
    pub nDefaultButton: i32,
    pub cRadioButtons: u32,
    pub pRadioButtons: *const TASKDIALOG_BUTTON,
    pub nDefaultRadioButton: i32,
    pub pszVerificationText: *const u16,
    pub pszExpandedInformation: *const u16,
    pub pszExpandedControlText: *const u16,
    pub pszCollapsedControlText: *const u16,
    pub hFooterIcon: isize,
    pub pszFooter: *const u16,
    pub pfCallback: isize,
    pub lpCallbackData: isize,
    pub cxWidth: u32,
}
const _: () = assert!(std::mem::size_of::<TASKDIALOGCONFIG>() == 160);
const _: () = assert!(std::mem::size_of::<TASKDIALOG_BUTTON>() == 12);

pub type TaskDialogIndirectFn = unsafe extern "system" fn(*const TASKDIALOGCONFIG, *mut i32, *mut i32, *mut BOOL) -> i32;

pub const CLSID_SHELL_LINK: GUID = GUID { data1: 0x00021401, data2: 0, data3: 0, data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46] };
pub const IID_ISHELL_LINK_W: GUID = GUID { data1: 0x000214F9, data2: 0, data3: 0, data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46] };
pub const IID_IPERSIST_FILE: GUID = GUID { data1: 0x0000010B, data2: 0, data3: 0, data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46] };
pub const FOLDERID_DESKTOP: GUID = GUID { data1: 0xB4BFCC3A, data2: 0xDB2C, data3: 0x424C, data4: [0xB0, 0x29, 0x7F, 0xE9, 0x9A, 0x87, 0xC6, 0x41] };
pub const FOLDERID_PROGRAMS: GUID = GUID { data1: 0xA77F5D77, data2: 0x2E2B, data3: 0x44C3, data4: [0xA6, 0xA2, 0xAB, 0xA6, 0x01, 0x05, 0x4A, 0x51] };
pub const FOLDERID_USER_PROGRAM_FILES: GUID = GUID { data1: 0x5CD7AEE2, data2: 0x2219, data3: 0x4A67, data4: [0xB8, 0x5D, 0x6C, 0x9C, 0xE1, 0x56, 0x60, 0xCB] };

#[link(name = "ole32", kind = "raw-dylib")]
extern "system" {
    pub fn CoInitializeEx(reservado: *const c_void, modo: u32) -> i32;
    pub fn CoUninitialize();
    pub fn CoCreateInstance(clsid: *const GUID, externo: *mut c_void, contexto: u32, iid: *const GUID, saida: *mut *mut c_void) -> i32;
    pub fn CoTaskMemFree(p: *mut c_void);
}

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    pub fn SHGetKnownFolderPath(id: *const GUID, flags: u32, token: HANDLE, caminho: *mut *mut u16) -> i32;
}

#[link(name = "advapi32", kind = "raw-dylib")]
extern "system" {
    pub fn RegCreateKeyExW(
        chave: HANDLE,
        sub: *const u16,
        reservado: u32,
        classe: *const u16,
        opcoes: u32,
        acesso: u32,
        seguranca: *const c_void,
        saida: *mut HANDLE,
        disposicao: *mut u32,
    ) -> i32;
    pub fn RegDeleteTreeW(chave: HANDLE, sub: *const u16) -> i32;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    pub fn LoadLibraryW(nome: *const u16) -> HANDLE;
    pub fn GetProcAddress(modulo: HANDLE, nome: *const u8) -> *const c_void;
    pub fn IsProcessInJob(processo: HANDLE, job: HANDLE, resultado: *mut BOOL) -> BOOL;
    pub fn CreateEventW(attr: *const c_void, manual: BOOL, inicial: BOOL, nome: *const u16) -> HANDLE;
    pub fn OpenEventW(acesso: u32, herdar: BOOL, nome: *const u16) -> HANDLE;
    pub fn SetEvent(h: HANDLE) -> BOOL;
    pub fn WaitForMultipleObjects(n: u32, handles: *const HANDLE, todos: BOOL, ms: u32) -> u32;
}

/// Cria o processo fora do "job" (grupo de processos) de quem o abriu: se o
/// programa que abriu o FlowCursor fechar o grupo, ele não morre junto.
pub const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

#[link(name = "winmm", kind = "raw-dylib")]
extern "system" {
    pub fn timeBeginPeriod(ms: u32) -> u32;
    pub fn timeEndPeriod(ms: u32) -> u32;
}

/// Texto em UTF-16 terminado em zero, para passar às funções W.
pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copia um texto para um campo UTF-16 de tamanho fixo (corta se faltar espaço).
pub fn copiar_w(dest: &mut [u16], s: &str) {
    let mut i = 0;
    for c in s.encode_utf16() {
        if i + 1 >= dest.len() {
            break;
        }
        dest[i] = c;
        i += 1;
    }
    dest[i] = 0;
}

/// Lê um texto UTF-16 terminado em zero de um buffer.
pub fn ler_w(buf: &[u16]) -> String {
    let fim = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..fim])
}

// ---------- Alternador de janelas (Alt+Tab) ----------

pub const WH_KEYBOARD_LL: i32 = 13;
pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_KEYUP: u32 = 0x0101;
pub const WM_SYSKEYDOWN: u32 = 0x0104;
pub const WM_SYSKEYUP: u32 = 0x0105;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_MBUTTONUP: u32 = 0x0208;
pub const WM_MOUSEACTIVATE: u32 = 0x0021;
pub const WM_GETICON: u32 = 0x007F;
pub const MA_NOACTIVATE: LRESULT = 3;
pub const LLKHF_ALTDOWN: u32 = 0x20;
pub const VK_TAB: u32 = 0x09;
pub const VK_RETURN: u32 = 0x0D;
pub const VK_SHIFT: i32 = 0x10;
pub const VK_CONTROL: i32 = 0x11;
pub const VK_MENU: u32 = 0x12;
pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_SPACE: u32 = 0x20;
pub const VK_LEFT: u32 = 0x25;
pub const VK_UP: u32 = 0x26;
pub const VK_RIGHT: u32 = 0x27;
pub const VK_DOWN: u32 = 0x28;
pub const VK_DELETE: u32 = 0x2E;
pub const VK_BACK: u32 = 0x08;
pub const VK_LWIN: i32 = 0x5B;
pub const VK_RWIN: i32 = 0x5C;
pub const VK_LMENU: u32 = 0xA4;
pub const VK_RMENU: u32 = 0xA5;
pub const INPUT_KEYBOARD: u32 = 1;
pub const KEYEVENTF_KEYUP: u32 = 0x0002;
pub const ICON_SMALL: usize = 0;
pub const ICON_BIG: usize = 1;
pub const ICON_SMALL2: usize = 2;
pub const GCLP_HICON: i32 = -14;
pub const GCLP_HICONSM: i32 = -34;
pub const SMTO_ABORTIFHUNG: u32 = 0x0002;
pub const SMTO_BLOCK: u32 = 0x0001;
pub const GW_OWNER: u32 = 4;
pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;
pub const SW_SHOWNA: i32 = 8;
pub const SRCCOPY: u32 = 0x00CC_0020;
pub const DI_NORMAL: u32 = 0x0003;
pub const LWA_ALPHA: u32 = 0x0000_0002;
pub const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
pub const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
pub const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
pub const DWMWCP_ROUND: i32 = 2;
pub const DWMSBT_TRANSIENTWINDOW: i32 = 3;
pub const DWM_TNP_RECTDESTINATION: u32 = 0x01;
pub const DWM_TNP_RECTSOURCE: u32 = 0x02;
pub const DWM_TNP_OPACITY: u32 = 0x04;
pub const DWM_TNP_VISIBLE: u32 = 0x08;
pub const DWM_TNP_SOURCECLIENTAREAONLY: u32 = 0x10;
pub const SHGFI_ICON: u32 = 0x0100;
pub const SHGFI_LARGEICON: u32 = 0x0000;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct KBDLLHOOKSTRUCT {
    pub vkCode: u32,
    pub scanCode: u32,
    pub flags: u32,
    pub time: u32,
    pub dwExtraInfo: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KEYBDINPUT {
    pub wVk: u16,
    pub wScan: u16,
    pub dwFlags: u32,
    pub time: u32,
    pub dwExtraInfo: usize,
}

/// INPUT com o lado do teclado da união; o resto completa os 40 bytes da união.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct INPUT {
    pub tipo: u32,
    pub _alinhamento: u32,
    pub ki: KEYBDINPUT,
    pub _resto: [u8; 8],
}
const _: () = assert!(std::mem::size_of::<INPUT>() == 40);

#[repr(C)]
pub struct MARGINS {
    pub cxLeftWidth: i32,
    pub cxRightWidth: i32,
    pub cyTopHeight: i32,
    pub cyBottomHeight: i32,
}

/// Empacotada em 1 byte no dwmapi.h.
#[repr(C, packed(1))]
pub struct DWM_THUMBNAIL_PROPERTIES {
    pub dwFlags: u32,
    pub rcDestination: RECT,
    pub rcSource: RECT,
    pub opacity: u8,
    pub fVisible: BOOL,
    pub fSourceClientAreaOnly: BOOL,
}
const _: () = assert!(std::mem::size_of::<DWM_THUMBNAIL_PROPERTIES>() == 45);

#[repr(C)]
pub struct PAINTSTRUCT {
    pub hdc: HANDLE,
    pub fErase: BOOL,
    pub rcPaint: RECT,
    pub fRestore: BOOL,
    pub fIncUpdate: BOOL,
    pub rgbReserved: [u8; 32],
}

#[repr(C)]
pub struct SHFILEINFOW {
    pub hIcon: HANDLE,
    pub iIcon: i32,
    pub dwAttributes: u32,
    pub szDisplayName: [u16; 260],
    pub szTypeName: [u16; 80],
}

pub type HOOKPROC = unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT;
pub type WNDENUMPROC = unsafe extern "system" fn(HWND, LPARAM) -> BOOL;

#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    pub fn SetWindowsHookExW(tipo: i32, f: Option<HOOKPROC>, modulo: HANDLE, thread: u32) -> HANDLE;
    pub fn UnhookWindowsHookEx(h: HANDLE) -> BOOL;
    pub fn CallNextHookEx(h: HANDLE, codigo: i32, wp: WPARAM, lp: LPARAM) -> LRESULT;
    pub fn PostThreadMessageW(thread: u32, msg: u32, wp: WPARAM, lp: LPARAM) -> BOOL;
    pub fn SendInput(n: u32, entradas: *const INPUT, tam: i32) -> u32;
    pub fn EnumWindows(f: Option<WNDENUMPROC>, dados: LPARAM) -> BOOL;
    pub fn GetWindowTextW(h: HWND, buf: *mut u16, max: i32) -> i32;
    pub fn SendMessageTimeoutW(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, flags: u32, ms: u32, resultado: *mut usize) -> LRESULT;
    pub fn GetClassLongPtrW(h: HWND, indice: i32) -> usize;
    pub fn BringWindowToTop(h: HWND) -> BOOL;
    pub fn AttachThreadInput(de: u32, para: u32, ligar: BOOL) -> BOOL;
    pub fn InvalidateRect(h: HWND, r: *const RECT, apagar: BOOL) -> BOOL;
    pub fn BeginPaint(h: HWND, ps: *mut PAINTSTRUCT) -> HANDLE;
    pub fn EndPaint(h: HWND, ps: *const PAINTSTRUCT) -> BOOL;
    pub fn SetLayeredWindowAttributes(h: HWND, cor: u32, alfa: u8, flags: u32) -> BOOL;
    pub fn DrawIconEx(dc: HANDLE, x: i32, y: i32, icone: HANDLE, cx: i32, cy: i32, passo: u32, fundo: HANDLE, flags: u32) -> BOOL;
    pub fn FindWindowExW(pai: HWND, depois: HWND, classe: *const u16, nome: *const u16) -> HWND;
    pub fn IsWindow(h: HWND) -> BOOL;
    pub fn ScreenToClient(h: HWND, p: *mut POINT) -> BOOL;
    pub fn PrivateExtractIconsW(arquivo: *const u16, indice: i32, cx: i32, cy: i32, icones: *mut HANDLE, ids: *mut u32, n: u32, flags: u32) -> u32;
    pub fn SendMessageW(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT;
}

pub const WM_NCACTIVATE: u32 = 0x0086;

#[link(name = "dwmapi", kind = "raw-dylib")]
extern "system" {
    pub fn DwmGetColorizationColor(cor: *mut u32, opaca: *mut BOOL) -> i32;
}

/// Vidro acrílico com a cor e a transparência escolhidas (função antiga do Windows,
/// não documentada, usada por muitos apps para esse efeito).
#[repr(C)]
pub struct ACCENT_POLICY {
    pub AccentState: u32,
    pub AccentFlags: u32,
    pub GradientColor: u32,
    pub AnimationId: u32,
}

#[repr(C)]
pub struct WINDOWCOMPOSITIONATTRIBDATA {
    pub Attrib: u32,
    pub pvData: *mut c_void,
    pub cbData: usize,
}

pub const WCA_ACCENT_POLICY: u32 = 19;
pub const ACCENT_ENABLE_TRANSPARENTGRADIENT: u32 = 2;
pub const ACCENT_ENABLE_BLURBEHIND: u32 = 3;
pub const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;
pub const STRING_FORMAT_LINELIMIT: i32 = 0x2000;
pub const STRING_ALIGN_FAR: i32 = 2;
pub const HTTRANSPARENTE: LRESULT = -1;

#[link(name = "user32", kind = "raw-dylib")]
extern "system" {
    pub fn SetWindowCompositionAttribute(h: HWND, dados: *mut WINDOWCOMPOSITIONATTRIBDATA) -> BOOL;
}

#[link(name = "gdi32", kind = "raw-dylib")]
extern "system" {
    pub fn BitBlt(dst: HANDLE, x: i32, y: i32, w: i32, h: i32, src: HANDLE, x1: i32, y1: i32, rop: u32) -> BOOL;
}

#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    pub fn GetCurrentThreadId() -> u32;
    pub fn GetCurrentProcessId() -> u32;
}

#[link(name = "dwmapi", kind = "raw-dylib")]
extern "system" {
    pub fn DwmRegisterThumbnail(destino: HWND, origem: HWND, saida: *mut isize) -> i32;
    pub fn DwmUnregisterThumbnail(h: isize) -> i32;
    pub fn DwmUpdateThumbnailProperties(h: isize, p: *const DWM_THUMBNAIL_PROPERTIES) -> i32;
    pub fn DwmQueryThumbnailSourceSize(h: isize, tam: *mut SIZE) -> i32;
    pub fn DwmExtendFrameIntoClientArea(h: HWND, m: *const MARGINS) -> i32;
}

#[link(name = "shell32", kind = "raw-dylib")]
extern "system" {
    pub fn SHGetFileInfoW(caminho: *const u16, atributos: u32, info: *mut SHFILEINFOW, tam: u32, flags: u32) -> usize;
}

#[link(name = "version", kind = "raw-dylib")]
extern "system" {
    pub fn GetFileVersionInfoSizeW(arquivo: *const u16, h: *mut u32) -> u32;
    pub fn GetFileVersionInfoW(arquivo: *const u16, h: u32, tam: u32, dados: *mut c_void) -> BOOL;
    pub fn VerQueryValueW(bloco: *const c_void, sub: *const u16, saida: *mut *mut c_void, tam: *mut u32) -> BOOL;
}

// GDI+ (API em C plana): texto suavizado no painel do alternador.
#[repr(C)]
pub struct GdiplusStartupInput {
    pub GdiplusVersion: u32,
    pub DebugEventCallback: *const c_void,
    pub SuppressBackgroundThread: BOOL,
    pub SuppressExternalCodecs: BOOL,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;
pub const UNIT_PIXEL: i32 = 2;
pub const TEXT_RENDERING_ANTIALIAS_GRIDFIT: i32 = 3;
pub const STRING_TRIMMING_ELLIPSIS: i32 = 3;
pub const STRING_FORMAT_NOWRAP: i32 = 0x1000;
pub const STRING_ALIGN_NEAR: i32 = 0;
pub const STRING_ALIGN_CENTER: i32 = 1;
pub const FONT_REGULAR: i32 = 0;
pub const FONT_BOLD: i32 = 1;

#[link(name = "gdiplus", kind = "raw-dylib")]
extern "system" {
    pub fn GdiplusStartup(token: *mut usize, entrada: *const GdiplusStartupInput, saida: *mut c_void) -> i32;
    pub fn GdipCreateBitmapFromScan0(w: i32, h: i32, passo: i32, formato: i32, dados: *mut u8, bmp: *mut *mut c_void) -> i32;
    pub fn GdipGetImageGraphicsContext(img: *mut c_void, g: *mut *mut c_void) -> i32;
    pub fn GdipDeleteGraphics(g: *mut c_void) -> i32;
    pub fn GdipDisposeImage(img: *mut c_void) -> i32;
    pub fn GdipSetTextRenderingHint(g: *mut c_void, modo: i32) -> i32;
    pub fn GdipCreateFontFamilyFromName(nome: *const u16, colecao: *mut c_void, familia: *mut *mut c_void) -> i32;
    pub fn GdipDeleteFontFamily(f: *mut c_void) -> i32;
    pub fn GdipCreateFont(familia: *mut c_void, tam: f32, estilo: i32, unidade: i32, fonte: *mut *mut c_void) -> i32;
    pub fn GdipDeleteFont(f: *mut c_void) -> i32;
    pub fn GdipCreateSolidFill(argb: u32, pincel: *mut *mut c_void) -> i32;
    pub fn GdipDeleteBrush(p: *mut c_void) -> i32;
    pub fn GdipCreateStringFormat(flags: i32, idioma: u16, formato: *mut *mut c_void) -> i32;
    pub fn GdipDeleteStringFormat(f: *mut c_void) -> i32;
    pub fn GdipSetStringFormatTrimming(f: *mut c_void, corte: i32) -> i32;
    pub fn GdipSetStringFormatAlign(f: *mut c_void, alinhamento: i32) -> i32;
    pub fn GdipSetStringFormatLineAlign(f: *mut c_void, alinhamento: i32) -> i32;
    pub fn GdipDrawString(g: *mut c_void, texto: *const u16, n: i32, fonte: *mut c_void, r: *const RectF, f: *mut c_void, pincel: *mut c_void) -> i32;
    pub fn GdipMeasureString(
        g: *mut c_void,
        texto: *const u16,
        n: i32,
        fonte: *mut c_void,
        caixa: *const RectF,
        f: *mut c_void,
        medida: *mut RectF,
        cabem: *mut i32,
        linhas: *mut i32,
    ) -> i32;
}

// ---------- WinHTTP (procurar e baixar atualizações) ----------

pub const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;
pub const WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY: u32 = 4;
pub const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
pub const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
pub const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;

#[link(name = "winhttp", kind = "raw-dylib")]
extern "system" {
    pub fn WinHttpOpen(agente: *const u16, acesso: u32, proxy: *const u16, excecoes: *const u16, flags: u32) -> HANDLE;
    pub fn WinHttpSetTimeouts(h: HANDLE, resolver: i32, conectar: i32, enviar: i32, receber: i32) -> BOOL;
    pub fn WinHttpConnect(h: HANDLE, servidor: *const u16, porta: u16, reservado: u32) -> HANDLE;
    pub fn WinHttpOpenRequest(
        h: HANDLE,
        verbo: *const u16,
        caminho: *const u16,
        versao: *const u16,
        referencia: *const u16,
        tipos: *const *const u16,
        flags: u32,
    ) -> HANDLE;
    pub fn WinHttpSendRequest(h: HANDLE, cabecalhos: *const u16, tam: u32, opcional: *const c_void, tam_opcional: u32, total: u32, contexto: usize) -> BOOL;
    pub fn WinHttpReceiveResponse(h: HANDLE, reservado: *mut c_void) -> BOOL;
    pub fn WinHttpQueryHeaders(h: HANDLE, nivel: u32, nome: *const u16, buf: *mut c_void, tam: *mut u32, indice: *mut u32) -> BOOL;
    pub fn WinHttpReadData(h: HANDLE, buf: *mut c_void, ler: u32, lidos: *mut u32) -> BOOL;
    pub fn WinHttpCloseHandle(h: HANDLE) -> BOOL;
}
