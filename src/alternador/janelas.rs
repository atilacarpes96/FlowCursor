// Lista de janelas do alternador: as mesmas que o Alt+Tab do Windows mostraria,
// na ordem de uso (a primeira é a janela atual), com nome do app e ícone.
use crate::ffi::*;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::Mutex;

pub struct Janela {
    pub hwnd: HWND,
    pub titulo: String,
    /// Caminho do executável em minúsculas: é a chave para agrupar por aplicativo.
    pub exe: String,
    pub app: String,
    pub icone: HANDLE,
    pub minimizada: bool,
}

/// Nome e ícone por executável, guardados enquanto o FlowCursor roda.
static APPS: Mutex<Option<HashMap<String, (String, isize)>>> = Mutex::new(None);

unsafe extern "system" fn coletar(h: HWND, dados: LPARAM) -> BOOL {
    (*(dados as *mut Vec<HWND>)).push(h);
    1
}

fn texto_janela(h: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn caminho_processo(h: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        let hp = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if hp == 0 {
            return String::new();
        }
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(hp, 0, buf.as_mut_ptr(), &mut n);
        CloseHandle(hp);
        if ok == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn oculta(h: HWND) -> bool {
    let mut v = 0u32;
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, (&mut v as *mut u32).cast::<c_void>(), 4) == 0 && v != 0 }
}

/// Regras do Alt+Tab: visível, não escondida pelo Windows (outra área de trabalho
/// virtual, app suspenso), com título, e janela "de aplicativo" (não ferramenta nem filha).
fn elegivel(h: HWND, meu_pid: u32) -> bool {
    unsafe {
        if IsWindowVisible(h) == 0 || oculta(h) {
            return false;
        }
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        let de_app = ex & WS_EX_APPWINDOW != 0;
        if !de_app && (ex & WS_EX_TOOLWINDOW != 0 || ex & WS_EX_NOACTIVATE != 0 || GetWindow(h, GW_OWNER) != 0) {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        if pid == meu_pid {
            return false;
        }
        let classe = crate::sistema::classe(h);
        if matches!(classe.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Windows.UI.Core.CoreWindow") {
            return false;
        }
        !texto_janela(h).trim().is_empty()
    }
}

/// "Descrição do arquivo" do executável (ex.: "Google Chrome"), que é o nome que as pessoas reconhecem.
fn descricao_exe(caminho: &str) -> Option<String> {
    unsafe {
        let cw = w(caminho);
        let mut h = 0u32;
        let tam = GetFileVersionInfoSizeW(cw.as_ptr(), &mut h);
        if tam == 0 {
            return None;
        }
        let mut dados = vec![0u8; tam as usize];
        if GetFileVersionInfoW(cw.as_ptr(), 0, tam, dados.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut p: *mut c_void = null_mut();
        let mut n = 0u32;
        let mut idiomas = Vec::new();
        if VerQueryValueW(dados.as_ptr().cast(), w("\\VarFileInfo\\Translation").as_ptr(), &mut p, &mut n) != 0 && n >= 4 {
            let pares = std::slice::from_raw_parts(p as *const u16, (n / 2) as usize);
            for c in pares.chunks(2).filter(|c| c.len() == 2) {
                idiomas.push(format!("{:04x}{:04x}", c[0], c[1]));
            }
        }
        idiomas.extend(["041604b0".to_string(), "040904b0".to_string(), "040904e4".to_string()]);
        for id in idiomas {
            let sub = w(&format!("\\StringFileInfo\\{id}\\FileDescription"));
            if VerQueryValueW(dados.as_ptr().cast(), sub.as_ptr(), &mut p, &mut n) != 0 && n > 1 {
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p as *const u16, n as usize - 1));
                let s = s.trim().trim_end_matches('\0').to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }
}

/// Ícone grande do executável (128 px, para reduzir com qualidade no painel), extraído dos recursos dele.
fn icone_exe(caminho: &str) -> HANDLE {
    unsafe {
        let mut icone: HANDLE = 0;
        let mut id = 0u32;
        let n = PrivateExtractIconsW(w(caminho).as_ptr(), 0, 128, 128, &mut icone, &mut id, 1, 0);
        if n == 1 && icone != 0 {
            return icone;
        }
        let mut info: SHFILEINFOW = std::mem::zeroed();
        if SHGetFileInfoW(w(caminho).as_ptr(), 0, &mut info, std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON) != 0 {
            return info.hIcon;
        }
        0
    }
}

/// Ícone que a própria janela informa (apps que trocam de ícone, como o Explorer).
fn icone_janela(h: HWND) -> HANDLE {
    unsafe {
        let mut r = 0usize;
        for tipo in [ICON_BIG, ICON_SMALL2, ICON_SMALL] {
            if SendMessageTimeoutW(h, WM_GETICON, tipo, 0, SMTO_ABORTIFHUNG | SMTO_BLOCK, 40, &mut r) != 0 && r != 0 {
                return r as HANDLE;
            }
        }
        let c = GetClassLongPtrW(h, GCLP_HICON);
        if c != 0 {
            return c as HANDLE;
        }
        GetClassLongPtrW(h, GCLP_HICONSM) as HANDLE
    }
}

/// Nome amigável quando o executável não tem descrição: "chrome.exe" vira "Chrome".
fn nome_pelo_arquivo(caminho: &str) -> String {
    let base = caminho.rsplit('\\').next().unwrap_or(caminho);
    let base = base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base);
    let mut c = base.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::from("Aplicativo"),
    }
}

pub fn listar() -> Vec<Janela> {
    let mut todas: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(Some(coletar), &mut todas as *mut Vec<HWND> as LPARAM);
    }
    let meu_pid = unsafe { GetCurrentProcessId() };
    let mut apps = APPS.lock().unwrap_or_else(|e| e.into_inner());
    let apps = apps.get_or_insert_with(HashMap::new);
    let mut saida = Vec::new();
    for h in todas {
        if !elegivel(h, meu_pid) {
            continue;
        }
        // Apps da Loja ficam dentro de uma moldura (ApplicationFrameHost); o app de verdade é a janela de dentro.
        let mut fonte = h;
        if crate::sistema::classe(h) == "ApplicationFrameWindow" {
            let dentro = unsafe { FindWindowExW(h, 0, w("Windows.UI.Core.CoreWindow").as_ptr(), null()) };
            if dentro != 0 {
                fonte = dentro;
            }
        }
        let caminho = caminho_processo(fonte);
        let chave = caminho.to_lowercase();
        let (app, icone_app) = apps
            .entry(chave.clone())
            .or_insert_with(|| {
                let nome = descricao_exe(&caminho).unwrap_or_else(|| nome_pelo_arquivo(&caminho));
                (nome, icone_exe(&caminho))
            })
            .clone();
        let icone = if icone_app != 0 { icone_app } else { icone_janela(h) };
        saida.push(Janela {
            hwnd: h,
            titulo: texto_janela(h),
            exe: chave,
            app,
            icone,
            minimizada: unsafe { IsIconic(h) != 0 },
        });
    }
    saida
}

/// Envia uma tecla sem uso (0xE8) com a marca do FlowCursor: soltar o Alt
/// depois dela não abre o menu do programa em primeiro plano.
pub fn mascarar_alt() {
    let mut teclas = [INPUT::default(); 2];
    for (i, t) in teclas.iter_mut().enumerate() {
        t.tipo = INPUT_KEYBOARD;
        t.ki.wVk = 0xE8;
        t.ki.dwFlags = if i == 1 { KEYEVENTF_KEYUP } else { 0 };
        t.ki.dwExtraInfo = super::MARCA;
    }
    unsafe {
        SendInput(2, teclas.as_ptr(), std::mem::size_of::<INPUT>() as i32);
    }
}

/// Traz a janela para a frente (restaura se estiver minimizada).
pub fn ativar(h: HWND) {
    unsafe {
        if IsWindow(h) == 0 {
            return;
        }
        mascarar_alt();
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        if SetForegroundWindow(h) != 0 && GetForegroundWindow() == h {
            return;
        }
        // O Windows às vezes recusa trocar o primeiro plano; ligando a entrada de
        // teclado à da janela atual por um instante ele aceita.
        let atual = GetForegroundWindow();
        let thread_atual = GetWindowThreadProcessId(atual, null_mut());
        let minha = GetCurrentThreadId();
        AttachThreadInput(minha, thread_atual, 1);
        BringWindowToTop(h);
        SetForegroundWindow(h);
        AttachThreadInput(minha, thread_atual, 0);
    }
}

pub fn fechar(h: HWND) {
    unsafe {
        PostMessageW(h, WM_CLOSE, 0, 0);
    }
}
