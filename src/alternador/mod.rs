// Alternador de janelas: substitui o Alt+Tab do Windows por um painel com
// miniaturas ao vivo, agrupadas por aplicativo, busca digitando e animações.
//
// Não precisa ficar segurando o Alt: se o painel já apareceu e você solta o Alt
// sem ter escolhido pelo Tab, ele fica aberto ("fixo") para usar o mouse, as
// setas ou digitar a busca; Enter ou clique abre, Esc ou clique fora fecha.
// O Alt+Tab rápido continua trocando direto para a janela anterior, e
// "Alt+Tab, Tab, Tab, solta" continua escolhendo, como no Windows.
//
// Um gancho de teclado de baixo nível só "segura" as teclas enquanto o painel
// está aberto; o Alt em si nunca é engolido, para o Windows não achar que ele
// ficou preso. Se esta linha de execução travar, o Windows desliga o gancho
// sozinho e o Alt+Tab normal volta.
pub mod janelas;
mod painel;

use crate::ffi::*;
use crate::reg;
use painel::{Clique, Painel};
use std::cell::{Cell, RefCell};
use std::ptr::null;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering::Relaxed};
use std::time::{Duration, Instant};

/// Marca das teclas que o próprio FlowCursor envia (o gancho deixa passar).
pub const MARCA: usize = 0x464C_4F57;

pub static HABILITADO: AtomicBool = AtomicBool::new(true);
static ABERTO: AtomicBool = AtomicBool::new(false);
static FIXO: AtomicBool = AtomicBool::new(false);
static THREAD: AtomicU32 = AtomicU32::new(0);
static GANCHO: AtomicIsize = AtomicIsize::new(0);
static GANCHO_MOUSE: AtomicIsize = AtomicIsize::new(0);
/// Retângulo do painel na tela, para o gancho do mouse saber se o clique foi fora.
static RET_PAINEL: [AtomicI32; 4] = [AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0)];

const M_ABRIR: u32 = WM_APP + 20;
const M_AVANCAR: u32 = WM_APP + 21;
const M_TECLA: u32 = WM_APP + 22;
const M_SOLTOU_ALT: u32 = WM_APP + 23;
const M_SAIR: u32 = WM_APP + 24;
const M_DEMO: u32 = WM_APP + 25;
const M_CLIQUE_FORA: u32 = WM_APP + 26;
const PM_NOREMOVE: u32 = 0;
const WH_MOUSE_LL: i32 = 14;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_MBUTTONDOWN: u32 = 0x0207;
/// Painel fixo sem nenhum uso por este tempo fecha sozinho.
const OCIOSO_FIXO: Duration = Duration::from_secs(30);

#[repr(C)]
struct MSLLHOOKSTRUCT {
    pt: POINT,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

/// Demonstração (--demo-alttab): o painel abre sem teclado e fecha sozinho, sem trocar de janela.
static DEMO_ATE: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

thread_local! {
    static PAINEL: RefCell<Option<Painel>> = const { RefCell::new(None) };
    /// escolheu com Tab ou setas enquanto segurava o Alt
    static NAVEGOU: Cell<bool> = const { Cell::new(false) };
    /// digitou, apagou ou fechou janela (aí soltar o Alt não deve escolher)
    static INTERAGIU: Cell<bool> = const { Cell::new(false) };
    static FRENTE_NA_ABERTURA: Cell<HWND> = const { Cell::new(0) };
    static ULTIMO_USO: Cell<Option<Instant>> = const { Cell::new(None) };
}

fn com_painel<R>(f: impl FnOnce(&mut Painel) -> R) -> Option<R> {
    PAINEL.with(|p| p.try_borrow_mut().ok().and_then(|mut p| p.as_mut().map(f)))
}

fn usou() {
    ULTIMO_USO.with(|u| u.set(Some(Instant::now())));
}

fn tecla_alt(vk: u32) -> bool {
    vk == VK_MENU || vk == VK_LMENU || vk == VK_RMENU
}

unsafe extern "system" fn gancho(codigo: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if codigo >= 0 && HABILITADO.load(Relaxed) {
        let k = &*(lp as *const KBDLLHOOKSTRUCT);
        if k.dwExtraInfo != MARCA {
            let m = wp as u32;
            let desce = m == WM_KEYDOWN || m == WM_SYSKEYDOWN;
            let aberto = ABERTO.load(Relaxed);
            let thread = THREAD.load(Relaxed);
            if k.vkCode == VK_TAB && desce && (aberto || k.flags & LLKHF_ALTDOWN != 0) {
                // Ctrl+Alt+Tab e Win+Tab continuam do Windows; em jogo ou vídeo em tela cheia também
                // (o painel não aparece por cima de tela cheia exclusiva).
                let ctrl_win = GetAsyncKeyState(VK_CONTROL) < 0 || GetAsyncKeyState(VK_LWIN) < 0 || GetAsyncKeyState(VK_RWIN) < 0;
                let tela_cheia = crate::sistema::SITUACAO.load(Relaxed) == crate::sistema::SITUACAO_TELA_CHEIA;
                if (!ctrl_win && !tela_cheia) || aberto {
                    let shift = (GetAsyncKeyState(VK_SHIFT) < 0) as usize;
                    ABERTO.store(true, Relaxed);
                    PostThreadMessageW(thread, if aberto { M_AVANCAR } else { M_ABRIR }, shift, 0);
                    return 1;
                }
            }
            if aberto {
                if tecla_alt(k.vkCode) {
                    if !desce {
                        PostThreadMessageW(thread, M_SOLTOU_ALT, 0, 0);
                    }
                    return CallNextHookEx(0, codigo, wp, lp);
                }
                // Shift passa (é o Shift+Tab); a tecla Windows passa e fecha o painel.
                if matches!(k.vkCode, 0x10 | 0xA0 | 0xA1) {
                    return CallNextHookEx(0, codigo, wp, lp);
                }
                if matches!(k.vkCode, 0x5B | 0x5C) {
                    if desce {
                        PostThreadMessageW(thread, M_TECLA, k.vkCode as usize, 0);
                    }
                    return CallNextHookEx(0, codigo, wp, lp);
                }
                if desce {
                    PostThreadMessageW(thread, M_TECLA, k.vkCode as usize, 0);
                }
                return 1;
            }
        }
    }
    CallNextHookEx(0, codigo, wp, lp)
}

/// Com o painel fixo, um clique fora dele fecha o painel (o clique segue para onde foi).
unsafe extern "system" fn gancho_mouse(codigo: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if codigo >= 0 && FIXO.load(Relaxed) {
        let m = wp as u32;
        if m == WM_LBUTTONDOWN || m == WM_RBUTTONDOWN || m == WM_MBUTTONDOWN {
            let k = &*(lp as *const MSLLHOOKSTRUCT);
            let r: Vec<i32> = RET_PAINEL.iter().map(|a| a.load(Relaxed)).collect();
            if !(k.pt.x >= r[0] && k.pt.x < r[2] && k.pt.y >= r[1] && k.pt.y < r[3]) {
                PostThreadMessageW(THREAD.load(Relaxed), M_CLIQUE_FORA, 0, 0);
            }
        }
    }
    CallNextHookEx(0, codigo, wp, lp)
}

fn encerrar_sessao() {
    FIXO.store(false, Relaxed);
    let m = GANCHO_MOUSE.swap(0, Relaxed);
    if m != 0 {
        unsafe { UnhookWindowsHookEx(m) };
    }
    ABERTO.store(false, Relaxed);
}

fn fechar_sem_trocar() {
    com_painel(|p| p.fechar());
    encerrar_sessao();
}

fn confirmar() {
    let alvo = com_painel(|p| {
        let h = p.selecionada();
        p.fechar();
        h
    })
    .flatten();
    encerrar_sessao();
    if let Some(h) = alvo {
        janelas::ativar(h);
    }
}

/// Soltou o Alt: troca de janela (Alt+Tab rápido, ou escolheu pelo Tab) ou deixa o painel fixo.
fn soltou_alt() {
    if !ABERTO.load(Relaxed) || FIXO.load(Relaxed) {
        return;
    }
    let (mostrado, mexeu) = com_painel(|p| (p.mostrado(), p.mouse_mexeu())).unwrap_or((false, false));
    if !mostrado || (NAVEGOU.with(Cell::get) && !INTERAGIU.with(Cell::get) && !mexeu) {
        confirmar();
        return;
    }
    FIXO.store(true, Relaxed);
    if let Some(r) = com_painel(|p| {
        p.fixar();
        p.na_tela()
    }) {
        for (a, v) in RET_PAINEL.iter().zip([r.left, r.top, r.right, r.bottom]) {
            a.store(v, Relaxed);
        }
    }
    unsafe {
        let m = SetWindowsHookExW(WH_MOUSE_LL, Some(gancho_mouse), GetModuleHandleW(null()), 0);
        GANCHO_MOUSE.store(m, Relaxed);
    }
    usou();
}

fn abrir(shift: bool) {
    abrir_com(shift, true);
}

fn abrir_com(shift: bool, mascarar: bool) {
    let inicio = Instant::now();
    NAVEGOU.with(|c| c.set(false));
    INTERAGIU.with(|c| c.set(false));
    FRENTE_NA_ABERTURA.with(|c| c.set(unsafe { GetForegroundWindow() }));
    usou();
    let lista = janelas::listar();
    if lista.is_empty() {
        reg!("Alt+Tab: nenhuma janela para mostrar");
        encerrar_sessao();
        return;
    }
    // Sem isto, soltar o Alt depois abriria o menu do programa em primeiro plano.
    if mascarar {
        janelas::mascarar_alt();
    }
    let n = lista.len();
    let inicial = if n > 1 { if shift { n - 1 } else { 1 } } else { 0 };
    let ok = com_painel(|p| p.abrir(lista, inicial));
    if ok != Some(true) {
        reg!("Alt+Tab: o painel não abriu ({})", if ok.is_none() { "painel ocupado" } else { "falha ao preparar" });
        encerrar_sessao();
        return;
    }
    reg!("Alt+Tab: {n} janelas, preparado em {:.0} ms", inicio.elapsed().as_secs_f64() * 1000.0);
}

/// Pede para a janela selecionada fechar (como o "X" dela) e tira do painel; sem
/// nenhuma janela sobrando, o painel fecha.
fn fechar_selecionada() {
    INTERAGIU.with(|c| c.set(true));
    com_painel(|p| {
        if let Some(h) = p.selecionada() {
            janelas::fechar(h);
            p.remover_selecionada();
        }
    });
    if com_painel(|p| p.vazio()).unwrap_or(true) {
        fechar_sem_trocar();
    }
}

fn tecla(vk: u32) {
    usou();
    match vk {
        VK_ESCAPE | 0x5B | 0x5C => fechar_sem_trocar(),
        VK_RETURN => confirmar(),
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN => {
            NAVEGOU.with(|c| c.set(true));
            let (dx, dy) = match vk {
                VK_LEFT => (-1.0, 0.0),
                VK_RIGHT => (1.0, 0.0),
                VK_UP => (0.0, -1.0),
                _ => (0.0, 1.0),
            };
            com_painel(|p| p.mover(dx, dy));
        }
        VK_DELETE => {
            INTERAGIU.with(|c| c.set(true));
            fechar_selecionada();
        }
        VK_BACK => {
            INTERAGIU.with(|c| c.set(true));
            com_painel(|p| {
                let mut f = p.filtro.clone();
                f.pop();
                p.filtrar(f);
            });
        }
        _ => {
            let c = match vk {
                0x41..=0x5A => Some((vk as u8 - 0x41 + b'a') as char),
                0x30..=0x39 => Some((vk as u8) as char),
                0x60..=0x69 => Some((vk as u8 - 0x60 + b'0') as char),
                VK_SPACE => Some(' '),
                _ => None,
            };
            if let Some(c) = c {
                INTERAGIU.with(|c| c.set(true));
                com_painel(|p| {
                    let mut f = p.filtro.clone();
                    f.push(c);
                    p.filtrar(f);
                });
            }
        }
    }
}

/// Mensagens que o gancho manda para esta linha de execução. Devolve false para sair.
fn tratar(msg: &MSG) -> bool {
    if msg.hwnd != 0 {
        unsafe {
            TranslateMessage(msg);
            DispatchMessageW(msg);
        }
        return true;
    }
    match msg.message {
        M_SAIR => return false,
        M_ABRIR => abrir(msg.wParam != 0),
        M_AVANCAR => {
            usou();
            NAVEGOU.with(|c| c.set(true));
            com_painel(|p| p.avancar(if msg.wParam != 0 { -1 } else { 1 }));
        }
        M_TECLA => tecla(msg.wParam as u32),
        M_DEMO => {
            *DEMO_ATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now() + Duration::from_millis(msg.wParam as u64));
            ABERTO.store(true, Relaxed);
            abrir_com(false, false);
            com_painel(|p| {
                let pontos: Vec<String> = (0..p.itens.len()).filter_map(|i| p.centro_na_tela(i)).map(|c| format!("{},{}", c.x, c.y)).collect();
                reg!("demonstração do Alt+Tab: miniaturas em {}", pontos.join(" "));
            });
        }
        M_SOLTOU_ALT => soltou_alt(),
        M_CLIQUE_FORA => {
            if FIXO.load(Relaxed) {
                fechar_sem_trocar();
            }
        }
        _ => {}
    }
    true
}

unsafe extern "system" fn procedimento(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ponto = || ((lp & 0xFFFF) as i16 as f32, ((lp >> 16) & 0xFFFF) as i16 as f32);
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_MOUSEMOVE => {
            let (x, y) = ponto();
            com_painel(|p| p.mouse(x, y));
            usou();
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = ponto();
            match com_painel(|p| p.clique(x, y)) {
                Some(Clique::Item) => confirmar(),
                Some(Clique::Fechar) => fechar_selecionada(),
                _ => {}
            }
            0
        }
        WM_MBUTTONUP => {
            // botão do meio fecha a janela, como na barra de tarefas
            let (x, y) = ponto();
            INTERAGIU.with(|c| c.set(true));
            let sob = com_painel(|p| p.indice_sob(x, y).map(|i| p.sel = i)).flatten();
            if sob.is_some() {
                fechar_selecionada();
            }
            0
        }
        WM_PAINT => {
            // o bitmap guarda sempre o último quadro inteiro
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(h, &mut ps);
            com_painel(|p| p.repintar(dc));
            EndPaint(h, &ps);
            0
        }
        0x0014 => 1, // WM_ERASEBKGND: o fundo é todo nosso
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

fn executar() {
    unsafe {
        THREAD.store(GetCurrentThreadId(), Relaxed);
        // cria a fila de mensagens antes do gancho começar a postar nela
        let mut msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut msg, 0, 0, 0, PM_NOREMOVE);
        let Some(painel) = Painel::criar(procedimento) else {
            reg!("Alt+Tab: não consegui criar o painel; fica o do Windows");
            return;
        };
        PAINEL.with(|p| *p.borrow_mut() = Some(painel));
        let h = SetWindowsHookExW(WH_KEYBOARD_LL, Some(gancho), GetModuleHandleW(null()), 0);
        if h == 0 {
            reg!("Alt+Tab: o Windows não aceitou o gancho de teclado; fica o do Windows");
            return;
        }
        GANCHO.store(h, Relaxed);
        reg!("Alt+Tab do FlowCursor pronto");
        let mut vblank = crate::sistema::Vblank::abrir();
        'laco: loop {
            if ABERTO.load(Relaxed) {
                while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
                    if !tratar(&msg) {
                        break 'laco;
                    }
                }
                let demo = *DEMO_ATE.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(ate) = demo {
                    if Instant::now() >= ate {
                        *DEMO_ATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
                        fechar_sem_trocar();
                    } else {
                        com_painel(|p| p.quadro());
                        vblank.esperar();
                    }
                    continue;
                }
                if FIXO.load(Relaxed) {
                    // Painel fixo: fecha se outra janela veio para a frente ou se ficou esquecido.
                    let mudou_frente = GetForegroundWindow() != FRENTE_NA_ABERTURA.with(Cell::get);
                    let ocioso = ULTIMO_USO.with(Cell::get).is_some_and(|u| u.elapsed() > OCIOSO_FIXO);
                    if mudou_frente || ocioso {
                        fechar_sem_trocar();
                        continue;
                    }
                } else if ABERTO.load(Relaxed) && GetAsyncKeyState(VK_MENU as i32) >= 0 {
                    // Segurança: a soltura do Alt se perdeu (outra área de trabalho, por exemplo).
                    std::thread::sleep(Duration::from_millis(30));
                    if GetAsyncKeyState(VK_MENU as i32) >= 0 {
                        soltou_alt();
                    }
                }
                com_painel(|p| p.quadro());
                vblank.esperar();
            } else {
                if GetMessageW(&mut msg, 0, 0, 0) <= 0 {
                    break;
                }
                if !tratar(&msg) {
                    break;
                }
            }
        }
        encerrar_sessao();
        UnhookWindowsHookEx(h);
        PAINEL.with(|p| *p.borrow_mut() = None);
    }
}

pub fn iniciar() -> std::thread::JoinHandle<()> {
    std::thread::spawn(executar)
}

pub fn demonstrar(ms: u32) {
    let t = THREAD.load(Relaxed);
    if t != 0 {
        unsafe {
            PostThreadMessageW(t, M_DEMO, ms as usize, 0);
        }
    }
}

pub fn encerrar() {
    let t = THREAD.load(Relaxed);
    if t != 0 {
        unsafe {
            PostThreadMessageW(t, M_SAIR, 0, 0);
        }
    }
}
