// FlowCursor: troca o desenho do ponteiro do Windows por um ponteiro com mola,
// borrão de movimento e rastro. O ponteiro real continua fazendo os cliques;
// só o desenho muda.
#![windows_subsystem = "windows"]

mod alternador;
mod config;
mod ffi;
mod formas;
mod galeria;
mod instalador;
mod laco;
mod motor;
mod overlay;
mod png;
mod registro;
mod sistema;
mod ui;

use ffi::*;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering::Relaxed};
use std::sync::OnceLock;

use sistema::CLASSE_PRINCIPAL;
const WM_BANDEJA: u32 = WM_APP + 1;
const WM_ABRIR_AJUSTES: u32 = WM_APP + 2;
const WM_DEMO_ALTTAB: u32 = WM_APP + 3;
const CMD_AJUSTES: usize = 1;
const CMD_ATIVO: usize = 2;
const CMD_CONFIG: usize = 3;
const CMD_REGISTRO: usize = 4;
const CMD_SAIR: usize = 5;
const ATALHO_PAUSAR: i32 = 1;
const ATALHO_SAIR: i32 = 2;
const RELOGIO_CONFIG: usize = 1;
const RELOGIO_TESTE: usize = 2;

static CAMINHO_REGISTRO: OnceLock<PathBuf> = OnceLock::new();
static ICONE: AtomicIsize = AtomicIsize::new(0);
static MSG_BARRA_CRIADA: AtomicU32 = AtomicU32::new(0);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--guardiao") => {
            if let Some(pid) = args.get(2).and_then(|s| s.parse().ok()) {
                sistema::guardiao(pid);
            }
            return;
        }
        Some("--guardiao-lancar") => {
            if let Some(pid) = args.get(2).and_then(|s| s.parse().ok()) {
                sistema::lancar_guardiao(pid);
            }
            return;
        }
        Some("--restaurar") => {
            sistema::restaurar_cursores();
            return;
        }
        Some("--desinstalar") => {
            instalador::desinstalar(&args);
            return;
        }
        Some("--demo-alttab") => {
            // Mostra o painel do Alt+Tab da instância aberta por alguns ms, sem teclado (para conferir o visual).
            let ms: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2500);
            unsafe {
                let h = FindWindowW(w(CLASSE_PRINCIPAL).as_ptr(), null());
                PostMessageW(h, WM_DEMO_ALTTAB, ms, 0);
            }
            return;
        }
        Some("--sair") => {
            // Fecha a instância aberta e espera ela terminar (usado pelo compilar.ps1).
            sistema::fechar_instancia();
            return;
        }
        Some("--galeria") => {
            galeria::gerar(Path::new(args.get(2).map_or("galeria", String::as_str)));
            return;
        }
        Some("--diagnostico") => {
            galeria::diagnostico(Path::new(args.get(2).map_or("diagnostico.txt", String::as_str)));
            return;
        }
        Some("--icone") => {
            let imagens: Vec<(usize, Vec<u8>)> = [16, 24, 32, 48, 64, 256].iter().map(|&l| (l, formas::icone_app(l))).collect();
            let _ = png::salvar_ico(Path::new(args.get(2).map_or("flowcursor.ico", String::as_str)), &imagens);
            return;
        }
        _ => {}
    }
    if instalador::e_instalador(&args) {
        instalador::instalar(&args);
        return;
    }
    // --teste N: roda por N segundos e fecha sozinho (e substitui uma instância aberta).
    let teste: Option<u32> = args.iter().position(|a| a == "--teste").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok());
    // --ajustes: abre a tela de ajustes (é o que o atalho da área de trabalho usa).
    let abrir_ajustes = args.iter().any(|a| a == "--ajustes");

    // Quem abriu o FlowCursor (um terminal, um app) pode encerrar de uma vez todos os
    // processos que abriu, guardião junto. Fora desse grupo, o FlowCursor fica independente.
    if teste.is_none() && !args.iter().any(|a| a == "--independente") && sistema::em_job() {
        if let Ok(exe) = std::env::current_exe() {
            let solto = std::process::Command::new(&exe)
                .args(&args[1..])
                .arg("--independente")
                .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
                .spawn();
            if solto.is_ok() {
                return;
            }
            // O grupo não deixa sair (o app do Claude é assim): o Explorer abre o programa
            // fora dele, só que sem argumentos. A marca evita relançar em círculo.
            let marca = std::env::temp_dir().join("flowcursor-relancado");
            let recente = std::fs::metadata(&marca)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|d| d < std::time::Duration::from_secs(15));
            if !recente {
                let _ = std::fs::write(&marca, b"");
                if std::process::Command::new("explorer.exe").arg(&exe).spawn().is_ok() {
                    return;
                }
            }
        }
    }

    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    sistema::sem_economia_de_energia();

    // Uma instância só. Abrir de novo com ele já rodando abre a tela de ajustes;
    // só o modo de teste fecha a instância aberta e assume o lugar.
    let mutex = unsafe {
        let anterior = FindWindowW(w(CLASSE_PRINCIPAL).as_ptr(), null());
        if anterior != 0 {
            if teste.is_none() {
                PostMessageW(anterior, WM_ABRIR_AJUSTES, 0, 0);
                return;
            }
            PostMessageW(anterior, WM_CLOSE, 0, 0);
        }
        let m = CreateMutexW(null(), 1, w("Local\\FlowCursorInstancia").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let r = WaitForSingleObject(m, 5000);
            if r != WAIT_OBJECT_0 && r != WAIT_ABANDONED {
                return;
            }
        }
        m
    };

    let caminho_config = config::localizar();
    let caminho_registro = caminho_config.with_file_name("flowcursor.log");
    registro::iniciar(&caminho_registro);
    reg!("FlowCursor {} iniciado (pid {})", env!("CARGO_PKG_VERSION"), std::process::id());
    reg!("configuração: {}", caminho_config.display());
    for (r, nome, esc) in sistema::listar_monitores() {
        reg!("monitor {nome}: {}x{} em ({}, {}), escala {:.0}%", r.right - r.left, r.bottom - r.top, r.left, r.top, esc * 100.0);
    }
    let (cfg, avisos) = config::carregar(&caminho_config);
    for a in avisos {
        reg!("configuração: {a}");
    }
    reg!("{cfg:?}");
    laco::publicar_config(cfg);
    *config::DATA.lock().unwrap() = std::fs::metadata(&caminho_config).and_then(|m| m.modified()).ok();
    let _ = config::CAMINHO.set(caminho_config);
    let _ = CAMINHO_REGISTRO.set(caminho_registro);

    // Começa limpo, caso uma execução anterior tenha caído com o ponteiro escondido.
    sistema::restaurar_cursores();
    std::panic::set_hook(Box::new(|info| {
        sistema::restaurar_cursores();
        reg!("erro interno: {info}");
        laco::SAIR.store(true, Relaxed);
        unsafe {
            let h = FindWindowW(w(CLASSE_PRINCIPAL).as_ptr(), null());
            PostMessageW(h, WM_CLOSE, 0, 0);
        }
    }));

    if sistema::em_job() {
        reg!("aviso: rodando dentro do grupo de processos de quem o abriu (o Windows não deixou sair)");
    }
    // O sinal de fim é criado antes do guardião, que o abre para saber se o fechamento foi normal.
    let sinal_fim = unsafe { CreateEventW(null(), 1, 0, w(&sistema::nome_sinal_fim(std::process::id())).as_ptr()) };
    let lancador = std::env::current_exe().and_then(|exe| {
        let mut c = std::process::Command::new(exe);
        c.arg("--guardiao-lancar").arg(std::process::id().to_string());
        c.creation_flags(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB).spawn().or_else(|_| c.creation_flags(CREATE_NO_WINDOW).spawn())
    });
    match lancador {
        Ok(mut l) => {
            let _ = l.wait();
        }
        Err(e) => reg!("guardião não iniciou: {e}"),
    }

    let hwnd = criar_janela_principal();
    if hwnd == 0 {
        reg!("não consegui criar a janela principal");
        sistema::restaurar_cursores();
        return;
    }
    unsafe {
        ICONE.store(criar_icone(), Relaxed);
        MSG_BARRA_CRIADA.store(RegisterWindowMessageW(w("TaskbarCreated").as_ptr()), Relaxed);
        adicionar_bandeja(hwnd);
        if RegisterHotKey(hwnd, ATALHO_PAUSAR, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_F9) == 0 {
            reg!("atalho Ctrl+Alt+F9 já está em uso por outro programa");
        }
        if RegisterHotKey(hwnd, ATALHO_SAIR, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_F10) == 0 {
            reg!("atalho Ctrl+Alt+F10 já está em uso por outro programa");
        }
        WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        SetTimer(hwnd, RELOGIO_CONFIG, 500, null());
        if let Some(s) = teste {
            reg!("modo de teste: fecha sozinho em {s} s");
            SetTimer(hwnd, RELOGIO_TESTE, s * 1000, null());
        } else if !abrir_ajustes && !args.iter().any(|a| a == "--inicio") {
            aviso_bandeja(hwnd, "FlowCursor ativo", "Clique duplo no ícone abre os ajustes · Ctrl+Alt+F10 fecha");
        }
        if abrir_ajustes {
            ui::abrir_janela();
        }
    }

    let vigia = std::thread::spawn(|| sistema::vigiar(&laco::SAIR, &laco::DESATIVAR_TELA_CHEIA));
    let desenho = std::thread::spawn(laco::executar);
    let alt_tab = alternador::iniciar();

    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    laco::SAIR.store(true, Relaxed);
    let _ = desenho.join();
    let _ = vigia.join();
    alternador::encerrar();
    let _ = alt_tab.join();
    ui::fechar_janela();
    unsafe {
        remover_bandeja(hwnd);
        UnregisterHotKey(hwnd, ATALHO_PAUSAR);
        UnregisterHotKey(hwnd, ATALHO_SAIR);
        WTSUnRegisterSessionNotification(hwnd);
        DestroyWindow(hwnd);
        DestroyIcon(ICONE.load(Relaxed));
    }
    // Fechamento normal: o guardião sai sem mexer nos cursores (e não pode
    // devolvê-los por cima de uma nova instância que esteja abrindo).
    unsafe {
        SetEvent(sinal_fim);
    }
    sistema::restaurar_cursores();
    reg!("encerrado");
    unsafe {
        ReleaseMutex(mutex);
        CloseHandle(mutex);
    }
}

fn criar_janela_principal() -> HWND {
    unsafe {
        let inst = GetModuleHandleW(null());
        let nome = w(CLASSE_PRINCIPAL);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(procedimento),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: 0,
            hCursor: 0,
            hbrBackground: 0,
            lpszMenuName: null(),
            lpszClassName: nome.as_ptr(),
            hIconSm: 0,
        };
        RegisterClassExW(&wc);
        // Janela comum e invisível (não "só mensagens"), para receber os avisos
        // gerais do Windows, como mudança de configurações e de monitores.
        CreateWindowExW(WS_EX_TOOLWINDOW, nome.as_ptr(), w("FlowCursor").as_ptr(), WS_POPUP, 0, 0, 0, 0, 0, 0, inst, null())
    }
}

unsafe extern "system" fn procedimento(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_BANDEJA => {
            match lp as u32 {
                WM_RBUTTONUP => mostrar_menu(h),
                WM_LBUTTONDBLCLK => ui::abrir_janela(),
                _ => {}
            }
            0
        }
        WM_ABRIR_AJUSTES => {
            ui::abrir_janela();
            0
        }
        WM_DEMO_ALTTAB => {
            alternador::demonstrar(wp as u32);
            0
        }
        WM_HOTKEY => {
            match wp as i32 {
                ATALHO_PAUSAR => alternar_pausa(h),
                ATALHO_SAIR => {
                    reg!("Ctrl+Alt+F10: fechando");
                    PostQuitMessage(0);
                }
                _ => {}
            }
            0
        }
        WM_TIMER => {
            if wp == RELOGIO_CONFIG {
                conferir_config();
            } else if wp == RELOGIO_TESTE {
                reg!("fim do tempo de teste");
                PostQuitMessage(0);
            }
            0
        }
        WM_WTSSESSION_CHANGE => {
            if wp == WTS_SESSION_LOCK {
                laco::BLOQUEADO.store(true, Relaxed);
                reg!("sessão bloqueada");
            } else if wp == WTS_SESSION_UNLOCK {
                laco::BLOQUEADO.store(false, Relaxed);
                reg!("sessão desbloqueada");
            }
            0
        }
        WM_SETTINGCHANGE => {
            if wp as u32 == SPI_SETCURSORS {
                laco::REAPLICAR.store(true, Relaxed);
            }
            DefWindowProcW(h, msg, wp, lp)
        }
        WM_DISPLAYCHANGE => {
            reg!("monitores mudaram");
            laco::REABRIR_VBLANK.store(true, Relaxed);
            0
        }
        WM_ENDSESSION => {
            if wp != 0 {
                sistema::restaurar_cursores();
            }
            0
        }
        WM_CLOSE => {
            reg!("pedido para fechar");
            PostQuitMessage(0);
            0
        }
        m if m != 0 && m == MSG_BARRA_CRIADA.load(Relaxed) => {
            // O Explorer reiniciou e a bandeja foi recriada vazia.
            adicionar_bandeja(h);
            0
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

fn conferir_config() {
    let Some(caminho) = config::CAMINHO.get() else { return };
    let data = std::fs::metadata(caminho).and_then(|m| m.modified()).ok();
    let mut guarda = config::DATA.lock().unwrap_or_else(|e| e.into_inner());
    if data == *guarda {
        return;
    }
    *guarda = data;
    let (cfg, avisos) = config::carregar(caminho);
    for a in avisos {
        reg!("configuração: {a}");
    }
    reg!("configuração recarregada: {cfg:?}");
    laco::publicar_config(cfg);
}

fn alternar_pausa(h: HWND) {
    let pausado = !laco::PAUSADO.load(Relaxed);
    laco::PAUSADO.store(pausado, Relaxed);
    reg!("{}", if pausado { "pausado" } else { "retomado" });
    unsafe {
        let mut d = dados_bandeja(h);
        d.uFlags = NIF_TIP;
        copiar_w(&mut d.szTip, dica());
        Shell_NotifyIconW(NIM_MODIFY, &d);
    }
}

fn dica() -> &'static str {
    if laco::PAUSADO.load(Relaxed) {
        "FlowCursor (pausado)"
    } else {
        "FlowCursor"
    }
}

fn dados_bandeja(h: HWND) -> NOTIFYICONDATAW {
    let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = h;
    d.uID = 1;
    d
}

unsafe fn adicionar_bandeja(h: HWND) {
    let mut d = dados_bandeja(h);
    d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    d.uCallbackMessage = WM_BANDEJA;
    d.hIcon = ICONE.load(Relaxed);
    copiar_w(&mut d.szTip, dica());
    Shell_NotifyIconW(NIM_ADD, &d);
}

unsafe fn aviso_bandeja(h: HWND, titulo: &str, texto: &str) {
    let mut d = dados_bandeja(h);
    d.uFlags = NIF_INFO;
    d.dwInfoFlags = NIIF_INFO;
    copiar_w(&mut d.szInfoTitle, titulo);
    copiar_w(&mut d.szInfo, texto);
    Shell_NotifyIconW(NIM_MODIFY, &d);
}

unsafe fn remover_bandeja(h: HWND) {
    let d = dados_bandeja(h);
    Shell_NotifyIconW(NIM_DELETE, &d);
}

unsafe fn mostrar_menu(h: HWND) {
    let menu = CreatePopupMenu();
    let ativo = if laco::PAUSADO.load(Relaxed) { 0 } else { MF_CHECKED };
    AppendMenuW(menu, MF_STRING, CMD_AJUSTES, w("Ajustes…").as_ptr());
    AppendMenuW(menu, MF_STRING | ativo, CMD_ATIVO, w("Ativo\tCtrl+Alt+F9").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, CMD_CONFIG, w("Abrir flowcursor.ini").as_ptr());
    AppendMenuW(menu, MF_STRING, CMD_REGISTRO, w("Abrir registro").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, CMD_SAIR, w("Sair\tCtrl+Alt+F10").as_ptr());
    let mut pt = POINT::default();
    GetCursorPos(&mut pt);
    SetForegroundWindow(h);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, pt.x, pt.y, 0, h, null()) as usize;
    DestroyMenu(menu);
    match cmd {
        CMD_AJUSTES => ui::abrir_janela(),
        CMD_ATIVO => alternar_pausa(h),
        CMD_CONFIG => abrir(config::CAMINHO.get()),
        CMD_REGISTRO => abrir(CAMINHO_REGISTRO.get()),
        CMD_SAIR => PostQuitMessage(0),
        _ => {}
    }
}

fn abrir(caminho: Option<&PathBuf>) {
    if let Some(c) = caminho {
        ui::abrir_arquivo(c);
    }
}

/// Ícone da bandeja desenhado pelo próprio motor de formas.
unsafe fn criar_icone() -> HANDLE {
    let lado = GetSystemMetrics(SM_CXSMICON).max(16) as usize;
    let px = formas::icone(lado);
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: 40,
            biWidth: lado as i32,
            biHeight: -(lado as i32),
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        bmiColors: [0],
    };
    let mut bits: *mut std::ffi::c_void = null_mut();
    let cor = CreateDIBSection(0, &bmi, 0, &mut bits, 0, 0);
    if cor == 0 || bits.is_null() {
        return LoadIconW(0, IDI_APPLICATION as *const u16);
    }
    std::ptr::copy_nonoverlapping(px.as_ptr(), bits.cast::<u32>(), lado * lado);
    let zeros = vec![0u8; lado.div_ceil(16) * 2 * lado];
    let mascara = CreateBitmap(lado as i32, lado as i32, 1, 1, zeros.as_ptr().cast());
    let ii = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mascara, hbmColor: cor };
    let h = CreateIconIndirect(&ii);
    DeleteObject(cor);
    DeleteObject(mascara);
    if h == 0 {
        LoadIconW(0, IDI_APPLICATION as *const u16)
    } else {
        h
    }
}
