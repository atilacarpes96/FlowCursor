// Contato com o Windows: esconder e devolver o ponteiro, guardião, sincronia
// com o monitor, DPI e a vigia que decide quando usar o ponteiro normal.
use crate::ffi::*;
use crate::formas::Tipo;
use crate::reg;
use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Cursores do sistema que o FlowCursor desenha. Os outros (ajuda, caneta,
/// arrastar, cursores próprios de programas) continuam com o desenho real.
pub const SUBSTITUIDOS: [(u32, Tipo); 12] = [
    (OCR_NORMAL, Tipo::Seta),
    (OCR_IBEAM, Tipo::Texto),
    (OCR_HAND, Tipo::Mao),
    (OCR_WAIT, Tipo::Espera),
    (OCR_APPSTARTING, Tipo::SetaEspera),
    (OCR_CROSS, Tipo::Cruz),
    (OCR_NO, Tipo::Proibido),
    (OCR_SIZEWE, Tipo::RedimH),
    (OCR_SIZENS, Tipo::RedimV),
    (OCR_SIZENWSE, Tipo::RedimNwse),
    (OCR_SIZENESW, Tipo::RedimNesw),
    (OCR_SIZEALL, Tipo::Mover),
];

/// Troca os cursores substituídos por um cursor transparente.
pub fn esconder_cursores() -> bool {
    let e = [0xFFu8; 128]; // máscara AND: tudo transparente (32x32, 1 bit por pixel)
    let ou = [0u8; 128]; // máscara XOR: nada desenhado
    let mut ok = true;
    unsafe {
        let inst = GetModuleHandleW(null());
        for (id, _) in SUBSTITUIDOS {
            let h = CreateCursor(inst, 0, 0, 32, 32, e.as_ptr().cast(), ou.as_ptr().cast());
            if h == 0 {
                ok = false;
                continue;
            }
            // Em caso de sucesso o Windows passa a ser dono do cursor e o destrói.
            if SetSystemCursor(h, id) == 0 {
                DestroyCursor(h);
                ok = false;
            }
        }
    }
    ok
}

/// Recarrega o esquema de cursores do usuário (o que está no registro do Windows).
pub fn restaurar_cursores() {
    unsafe {
        SystemParametersInfoW(SPI_SETCURSORS, 0, null_mut(), 0);
    }
}

/// Confere se a seta do sistema é hoje o nosso cursor transparente.
pub fn seta_em_branco() -> bool {
    unsafe {
        let h = LoadCursorW(0, OCR_NORMAL as usize as *const u16);
        let mut ii: ICONINFO = std::mem::zeroed();
        if GetIconInfo(h, &mut ii) == 0 {
            return false;
        }
        let le = |bmp: HANDLE| -> Option<(BITMAP, Vec<u8>)> {
            let mut b: BITMAP = std::mem::zeroed();
            if GetObjectW(bmp, std::mem::size_of::<BITMAP>() as i32, (&mut b as *mut BITMAP).cast()) == 0 {
                return None;
            }
            let n = (b.bmWidthBytes * b.bmHeight) as usize;
            let mut v = vec![0u8; n];
            if GetBitmapBits(bmp, n as i32, v.as_mut_ptr().cast()) == 0 {
                return None;
            }
            Some((b, v))
        };
        let mascara = le(ii.hbmMask);
        let cor = if ii.hbmColor != 0 { le(ii.hbmColor) } else { None };
        DeleteObject(ii.hbmMask);
        if ii.hbmColor != 0 {
            DeleteObject(ii.hbmColor);
        }
        let Some((bm, bits)) = mascara else { return false };
        // Cursor monocromático: metade de cima é AND, metade de baixo é XOR.
        let (and, xor): (&[u8], &[u8]) = if ii.hbmColor == 0 {
            let meio = (bm.bmWidthBytes * bm.bmHeight / 2) as usize;
            (&bits[..meio], &bits[meio..])
        } else {
            (&bits[..], &[])
        };
        let cor_vazia = cor.map_or(true, |(_, c)| c.iter().all(|&x| x == 0));
        and.iter().all(|&x| x == 0xFF) && xor.iter().all(|&x| x == 0) && cor_vazia
    }
}

/// Identifica o tipo pelo identificador do cursor. Os cursores do sistema
/// mantêm o mesmo identificador depois de SetSystemCursor.
pub struct MapaCursores {
    pares: Vec<(isize, Tipo)>,
}

impl MapaCursores {
    pub fn novo() -> Self {
        let pares = SUBSTITUIDOS
            .iter()
            .map(|&(id, t)| (unsafe { LoadCursorW(0, id as usize as *const u16) }, t))
            .collect();
        MapaCursores { pares }
    }
    pub fn tipo(&self, h: isize) -> Option<Tipo> {
        self.pares.iter().find(|(x, _)| *x == h).map(|(_, t)| *t)
    }
    pub fn identificadores(&self) -> Vec<(isize, Tipo)> {
        self.pares.clone()
    }
}

/// Nome do sinal que o FlowCursor dá ao guardião quando fecha normalmente.
pub fn nome_sinal_fim(pid: u32) -> String {
    format!("Local\\FlowCursorFim{pid}")
}

/// Processo separado que devolve o ponteiro normal se o principal morrer
/// sem conseguir (travamento, Gerenciador de Tarefas, o programa que o abriu
/// encerrando tudo). Se o principal fechar normalmente, ele avisa pelo sinal
/// e o guardião sai sem mexer em nada (uma nova instância pode estar abrindo).
pub fn guardiao(pid: u32) {
    unsafe {
        let processo = OpenProcess(SYNCHRONIZE, 0, pid);
        if processo == 0 {
            restaurar_cursores();
            return;
        }
        let sinal = OpenEventW(SYNCHRONIZE, 0, w(&nome_sinal_fim(pid)).as_ptr());
        let r = if sinal != 0 {
            WaitForMultipleObjects(2, [processo, sinal].as_ptr(), 0, INFINITE)
        } else {
            WaitForSingleObject(processo, INFINITE)
        };
        if r == WAIT_OBJECT_0 {
            restaurar_cursores();
        }
        CloseHandle(processo);
        if sinal != 0 {
            CloseHandle(sinal);
        }
    }
}

/// Abre o guardião por um intermediário que fecha na hora: assim o guardião
/// não fica pendurado no FlowCursor e "Finalizar árvore de processos" não o leva junto.
pub fn lancar_guardiao(pid: u32) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut c = std::process::Command::new(exe);
    c.arg("--guardiao").arg(pid.to_string());
    use std::os::windows::process::CommandExt;
    let fora = c.creation_flags(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB).spawn();
    if fora.is_err() {
        let _ = c.creation_flags(CREATE_NO_WINDOW).spawn();
    }
}

pub fn em_job() -> bool {
    let mut r: BOOL = 0;
    unsafe { IsProcessInJob(GetCurrentProcess(), 0, &mut r) };
    r != 0
}

/// Classe da janela principal (invisível): serve para achar uma instância aberta.
pub const CLASSE_PRINCIPAL: &str = "FlowCursorPrincipal";

/// Pede para a instância aberta fechar e espera até 5 s. Devolve se havia uma.
pub fn fechar_instancia() -> bool {
    unsafe {
        let h = FindWindowW(w(CLASSE_PRINCIPAL).as_ptr(), null());
        if h == 0 {
            return false;
        }
        PostMessageW(h, WM_CLOSE, 0, 0);
        for _ in 0..100 {
            if FindWindowW(w(CLASSE_PRINCIPAL).as_ptr(), null()) == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }
}

const CHAVE_INICIAR: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const NOME_INICIAR: &str = "FlowCursor";

/// Se o FlowCursor está na lista de programas que abrem com o Windows (só do usuário).
pub fn inicia_com_windows() -> bool {
    unsafe {
        let mut chave: HANDLE = 0;
        if RegOpenKeyExW(HKEY_CURRENT_USER, w(CHAVE_INICIAR).as_ptr(), 0, KEY_QUERY_VALUE, &mut chave) != 0 {
            return false;
        }
        let mut tam = 0u32;
        let existe = RegQueryValueExW(chave, w(NOME_INICIAR).as_ptr(), null_mut(), null_mut(), null_mut(), &mut tam) == 0;
        RegCloseKey(chave);
        existe
    }
}

/// Liga ou desliga o início com o Windows para o programa em `exe`.
/// O argumento --inicio evita o aviso de "FlowCursor ativo" a cada vez que o PC liga.
pub fn definir_inicio_com_windows(ligar: bool, exe: &std::path::Path) -> bool {
    unsafe {
        let mut chave: HANDLE = 0;
        if RegOpenKeyExW(HKEY_CURRENT_USER, w(CHAVE_INICIAR).as_ptr(), 0, KEY_SET_VALUE, &mut chave) != 0 {
            return false;
        }
        let ok = if ligar {
            let valor = w(&format!("\"{}\" --inicio", exe.display()));
            RegSetValueExW(chave, w(NOME_INICIAR).as_ptr(), 0, REG_SZ, valor.as_ptr().cast(), (valor.len() * 2) as u32) == 0
        } else {
            let r = RegDeleteValueW(chave, w(NOME_INICIAR).as_ptr());
            r == 0 || r == 2 // 2: já não existia
        };
        RegCloseKey(chave);
        ok
    }
}

/// Pede ao Windows para não colocar o processo em modo de economia,
/// que atrasaria os quadros quando o FlowCursor está em segundo plano.
pub fn sem_economia_de_energia() {
    let estado = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        StateMask: 0,
    };
    unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            PROCESS_POWER_THROTTLING_CLASS,
            (&estado as *const PROCESS_POWER_THROTTLING_STATE).cast(),
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );
    }
}

/// Escala do monitor sob o ponto (1.0 = 100%).
pub fn escala_dpi(pt: POINT) -> f32 {
    unsafe {
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let (mut x, mut y) = (96u32, 96u32);
        if GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut x, &mut y) != 0 {
            return 1.0;
        }
        x as f32 / 96.0
    }
}

pub fn info_monitor(mon: HANDLE) -> Option<(RECT, String)> {
    unsafe {
        let mut mi: MONITORINFOEXW = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return None;
        }
        Some((mi.rcMonitor, ler_w(&mi.szDevice)))
    }
}

pub fn listar_monitores() -> Vec<(RECT, String, f32)> {
    unsafe extern "system" fn cb(mon: HANDLE, _: HANDLE, _: *mut RECT, dados: LPARAM) -> BOOL {
        let v = &mut *(dados as *mut Vec<HANDLE>);
        v.push(mon);
        1
    }
    let mut mons: Vec<HANDLE> = Vec::new();
    unsafe {
        EnumDisplayMonitors(0, null(), Some(cb), &mut mons as *mut Vec<HANDLE> as LPARAM);
    }
    mons.into_iter()
        .filter_map(|m| {
            let (r, nome) = info_monitor(m)?;
            let (mut x, mut y) = (96u32, 96u32);
            unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
            Some((r, nome, x as f32 / 96.0))
        })
        .collect()
}

// ---------- Sincronia com o monitor ----------

/// Espera o próximo retraço vertical do monitor principal (164 Hz aqui),
/// para desenhar exatamente uma vez por atualização da tela.
pub struct Vblank {
    adaptador: u32,
    fonte: u32,
    ativo: bool,
    falhas: u32,
    ultimo: Instant,
    pub metodo: &'static str,
}

impl Vblank {
    pub fn abrir() -> Self {
        let mut v = Vblank { adaptador: 0, fonte: 0, ativo: false, falhas: 0, ultimo: Instant::now(), metodo: "DwmFlush" };
        unsafe {
            let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            if let Some((_, nome)) = info_monitor(mon) {
                let dc = CreateDCW(null(), w(&nome).as_ptr(), null(), null());
                if dc != 0 {
                    let mut o = D3DKMT_OPENADAPTERFROMHDC { hDc: dc, ..Default::default() };
                    if D3DKMTOpenAdapterFromHdc(&mut o) == 0 {
                        v.adaptador = o.hAdapter;
                        v.fonte = o.VidPnSourceId;
                        v.ativo = true;
                        v.metodo = "D3DKMT";
                    }
                    DeleteDC(dc);
                }
            }
        }
        v
    }

    pub fn esperar(&mut self) {
        unsafe {
            if self.ativo {
                let e = D3DKMT_WAITFORVERTICALBLANKEVENT { hAdapter: self.adaptador, hDevice: 0, VidPnSourceId: self.fonte };
                if D3DKMTWaitForVerticalBlankEvent(&e) != 0 {
                    self.falhas += 1;
                    if self.falhas > 30 {
                        reg!("sincronia D3DKMT falhou {} vezes; passando para DwmFlush", self.falhas);
                        self.ativo = false;
                        self.metodo = "DwmFlush";
                    }
                    std::thread::sleep(Duration::from_millis(6));
                }
            } else {
                DwmFlush();
            }
            // Proteção: se a espera voltar na hora (monitor desligado, por exemplo),
            // segura o laço em no máximo ~250 quadros por segundo.
            let decorrido = self.ultimo.elapsed();
            if decorrido < Duration::from_millis(4) {
                std::thread::sleep(Duration::from_millis(4) - decorrido);
            }
            self.ultimo = Instant::now();
        }
    }
}

impl Drop for Vblank {
    fn drop(&mut self) {
        if self.ativo {
            unsafe {
                D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: self.adaptador });
            }
        }
    }
}

// ---------- Vigia: quando usar o ponteiro normal ----------

pub const SITUACAO_NORMAL: u8 = 0;
pub const SITUACAO_SISTEMA: u8 = 1;
pub const SITUACAO_TELA_CHEIA: u8 = 2;
pub const SITUACAO_AREA_SEGURA: u8 = 3;
pub static SITUACAO: AtomicU8 = AtomicU8::new(SITUACAO_NORMAL);

/// Janelas do Windows que ficam numa camada acima de qualquer "sempre visível"
/// (menu Iniciar, pesquisa, Alt+Tab, central de notificações...). Sobre elas o
/// desenho do FlowCursor ficaria escondido, então o ponteiro real volta.
const PROCESSOS_SISTEMA: [&str; 6] = [
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "searchapp.exe",
    "shellexperiencehost.exe",
    "shellhost.exe",
    "lockapp.exe",
];
const CLASSES_SISTEMA: [&str; 4] = ["XamlExplorerHostIslandWindow", "MultitaskingViewFrame", "ForegroundStaging", "TaskSwitcherWnd"];

pub fn classe(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), 256) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn processo(h: HWND) -> String {
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
        let caminho = String::from_utf16_lossy(&buf[..n as usize]);
        caminho.rsplit('\\').next().unwrap_or("").to_lowercase()
    }
}

fn oculta_pelo_dwm(h: HWND) -> bool {
    let mut v = 0u32;
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED, (&mut v as *mut u32).cast::<c_void>(), 4) == 0 && v != 0 }
}

fn e_do_sistema(classe: &str, processo: &str) -> bool {
    PROCESSOS_SISTEMA.contains(&processo) || CLASSES_SISTEMA.contains(&classe)
}

/// Camada ("banda") da janela na ordem do Windows. As janelas comuns, inclusive as
/// "sempre visíveis" como o desenho do FlowCursor, ficam na camada 1. Algumas ficam
/// numa camada acima, que programa comum nenhum consegue passar: o Gerenciador de
/// Tarefas com "Sempre visível" ligado fica na 16. `GetWindowBand` não é documentada,
/// então é buscada na hora; se faltar, toda janela conta como camada 1.
fn camada(h: HWND) -> u32 {
    type GetWindowBandFn = unsafe extern "system" fn(HWND, *mut u32) -> BOOL;
    static FUNCAO: OnceLock<Option<GetWindowBandFn>> = OnceLock::new();
    let funcao = FUNCAO.get_or_init(|| unsafe {
        let user32 = GetModuleHandleW(w("user32.dll").as_ptr());
        let f = if user32 != 0 { GetProcAddress(user32, c"GetWindowBand".as_ptr().cast()) } else { null() };
        (!f.is_null()).then(|| std::mem::transmute::<*const c_void, GetWindowBandFn>(f))
    });
    let mut banda = 1u32;
    if let Some(f) = funcao {
        if unsafe { f(h, &mut banda) } == 0 {
            banda = 1;
        }
    }
    banda
}

fn tela_cheia(h: HWND, classe: &str) -> bool {
    if h == 0 || classe == "Progman" || classe == "WorkerW" || classe == "Shell_TrayWnd" {
        return false;
    }
    unsafe {
        // Janela maximizada num monitor sem barra de tarefas também cobre a tela toda,
        // mas não é jogo nem vídeo. Tela cheia de verdade não fica "maximizada" nem tem barra de título.
        if IsZoomed(h) != 0 || (GetWindowLongW(h, GWL_STYLE) as u32 & WS_CAPTION) == WS_CAPTION {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r) == 0 {
            return false;
        }
        let Some((m, _)) = info_monitor(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST)) else { return false };
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

fn area_segura() -> bool {
    // Tela de bloqueio, UAC e Ctrl+Alt+Del ficam em outra área de trabalho.
    unsafe {
        let d = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if d == 0 {
            return true;
        }
        CloseDesktop(d);
        false
    }
}

/// Laço da vigia (thread própria, a cada 30 ms). Fica fora do laço de desenho
/// porque algumas consultas a outras janelas podem demorar.
pub fn vigiar(sair: &std::sync::atomic::AtomicBool, desativar_tela_cheia: &std::sync::atomic::AtomicBool) {
    let mut fg_ant: HWND = -1;
    let mut fg_info = (String::new(), String::new());
    let mut vistos: HashSet<(String, String)> = HashSet::new();
    let mut sit_ant = SITUACAO_NORMAL;
    let mut motivo_ant = String::new();
    while !sair.load(Ordering::Relaxed) {
        let (sit, motivo) = unsafe {
            let fg = GetForegroundWindow();
            if fg != fg_ant {
                fg_ant = fg;
                fg_info = (classe(fg), processo(fg));
                if vistos.insert(fg_info.clone()) {
                    reg!("janela em primeiro plano: {} ({})", fg_info.1, fg_info.0);
                }
            }
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            let sob = GetAncestor(WindowFromPoint(pt), GA_ROOT);
            let sob_info = if sob != 0 && !oculta_pelo_dwm(sob) { (classe(sob), processo(sob)) } else { Default::default() };
            if area_segura() {
                (SITUACAO_AREA_SEGURA, "área segura do Windows".to_string())
            } else if e_do_sistema(&fg_info.0, &fg_info.1) {
                (SITUACAO_SISTEMA, format!("{} em primeiro plano", fg_info.1))
            } else if e_do_sistema(&sob_info.0, &sob_info.1) {
                (SITUACAO_SISTEMA, format!("ponteiro sobre {} ({})", sob_info.1, sob_info.0))
            } else if !sob_info.0.is_empty() && camada(sob) > 1 {
                // Sobre ela o desenho ficaria por baixo, cortado ou invisível.
                (SITUACAO_SISTEMA, format!("ponteiro sobre {} ({}), numa camada acima do FlowCursor", sob_info.1, sob_info.0))
            } else if desativar_tela_cheia.load(Ordering::Relaxed) && !oculta_pelo_dwm(fg) && tela_cheia(fg, &fg_info.0) {
                (SITUACAO_TELA_CHEIA, format!("tela cheia: {} ({})", fg_info.1, fg_info.0))
            } else {
                (SITUACAO_NORMAL, String::new())
            }
        };
        if sit != sit_ant || motivo != motivo_ant {
            if sit != SITUACAO_NORMAL {
                reg!("ponteiro normal: {motivo}");
            } else {
                reg!("FlowCursor de volta");
            }
            sit_ant = sit;
            motivo_ant = motivo;
        }
        SITUACAO.store(sit, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(30));
    }
}
