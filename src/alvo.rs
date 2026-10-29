// Ímã: descobre o botão (ou link, aba, item de menu) embaixo do mouse e o retângulo dele,
// para o ponteiro de vidro virar uma pílula do tamanho do botão, como no iPad.
// Usa a acessibilidade do Windows (UI Automation, por COM). A consulta vai a outro
// processo e pode levar alguns milissegundos, então roda numa linha de execução própria.
use crate::ffi::*;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::Duration;

pub static ATIVO: AtomicBool = AtomicBool::new(false);
/// Retângulo (na tela) do controle sob o mouse na última consulta.
static ALVO: Mutex<Option<RECT>> = Mutex::new(None);

/// O retângulo do botão sob o ponto, se o ponto ainda estiver dentro dele.
pub fn sob(x: i32, y: i32) -> Option<RECT> {
    if !ATIVO.load(Relaxed) {
        return None;
    }
    let r = (*ALVO.lock().unwrap_or_else(|e| e.into_inner()))?;
    (x >= r.left && x < r.right && y >= r.top && y < r.bottom).then_some(r)
}

const CLSID_CUIAUTOMATION: GUID = GUID { data1: 0xff48dba4, data2: 0x60ef, data3: 0x4201, data4: [0xaa, 0x87, 0x54, 0x10, 0x3e, 0xef, 0x59, 0x4e] };
const IID_IUIAUTOMATION: GUID = GUID { data1: 0x30cbe57d, data2: 0xd9d0, data3: 0x452a, data4: [0xab, 0x13, 0x7a, 0xc5, 0xac, 0x48, 0x25, 0xee] };

/// Tipos de controle que "puxam" o ponteiro.
const PUXAM: [i32; 8] = [
    50000, // botão
    50005, // link
    50011, // item de menu
    50019, // aba
    50002, // caixa de seleção
    50013, // botão de opção
    50031, // botão dividido
    50003, // caixa de combinação
];

/// Chama o método número `n` da tabela de um objeto COM.
unsafe fn metodo<F: Copy>(obj: *mut c_void, n: usize) -> F {
    let tabela = *(obj as *const *const usize);
    std::mem::transmute_copy(&*tabela.add(n))
}

unsafe fn soltar(obj: *mut c_void) {
    if !obj.is_null() {
        let f: unsafe extern "system" fn(*mut c_void) -> u32 = metodo(obj, 2);
        f(obj);
    }
}

pub fn iniciar(sair: &'static AtomicBool) {
    std::thread::spawn(move || unsafe {
        CoInitializeEx(std::ptr::null(), 0);
        let mut uia: *mut c_void = std::ptr::null_mut();
        if CoCreateInstance(&CLSID_CUIAUTOMATION, std::ptr::null_mut(), 1, &IID_IUIAUTOMATION, &mut uia) != 0 || uia.is_null() {
            crate::reg!("ímã: a acessibilidade do Windows não respondeu; sem grudar nos botões");
            return;
        }
        let elemento_em: unsafe extern "system" fn(*mut c_void, POINT, *mut *mut c_void) -> i32 = metodo(uia, 7);
        let andador: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32 = metodo(uia, 14);
        let mut arvore: *mut c_void = std::ptr::null_mut();
        andador(uia, &mut arvore);
        let meu_pid = GetCurrentProcessId();
        let mut ant = POINT { x: i32::MIN, y: 0 };
        while !sair.load(Relaxed) {
            if !ATIVO.load(Relaxed) {
                *ALVO.lock().unwrap_or_else(|e| e.into_inner()) = None;
                std::thread::sleep(Duration::from_millis(150));
                continue;
            }
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            if pt.x == ant.x && pt.y == ant.y {
                std::thread::sleep(Duration::from_millis(40));
                continue;
            }
            ant = pt;
            let achado = consultar(uia, elemento_em, arvore, pt, meu_pid);
            *ALVO.lock().unwrap_or_else(|e| e.into_inner()) = achado;
            std::thread::sleep(Duration::from_millis(25));
        }
        soltar(arvore);
        soltar(uia);
    });
}

type ElementoEm = unsafe extern "system" fn(*mut c_void, POINT, *mut *mut c_void) -> i32;

/// O controle "puxável" no ponto: o próprio elemento ou um dos três de cima (o texto de
/// um link costuma ser filho dele).
unsafe fn consultar(uia: *mut c_void, elemento_em: ElementoEm, arvore: *mut c_void, pt: POINT, meu_pid: u32) -> Option<RECT> {
    let mut el: *mut c_void = std::ptr::null_mut();
    if elemento_em(uia, pt, &mut el) != 0 || el.is_null() {
        return None;
    }
    let dpi = crate::sistema::escala_dpi(pt);
    let mut achado = None;
    for _ in 0..4 {
        let pid_de: unsafe extern "system" fn(*mut c_void, *mut i32) -> i32 = metodo(el, 20);
        let tipo_de: unsafe extern "system" fn(*mut c_void, *mut i32) -> i32 = metodo(el, 21);
        let ret_de: unsafe extern "system" fn(*mut c_void, *mut RECT) -> i32 = metodo(el, 43);
        let (mut pid, mut tipo, mut r) = (0i32, 0i32, RECT::default());
        pid_de(el, &mut pid);
        if pid as u32 == meu_pid {
            break;
        }
        tipo_de(el, &mut tipo);
        if PUXAM.contains(&tipo) && ret_de(el, &mut r) == 0 {
            let (w, h) = ((r.right - r.left) as f32 / dpi, (r.bottom - r.top) as f32 / dpi);
            // nem minúsculo nem do tamanho de um painel
            if (14.0..=420.0).contains(&w) && (12.0..=90.0).contains(&h) {
                achado = Some(r);
            }
            break;
        }
        if arvore.is_null() {
            break;
        }
        let pai_de: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32 = metodo(arvore, 3);
        let mut pai: *mut c_void = std::ptr::null_mut();
        pai_de(arvore, el, &mut pai);
        soltar(el);
        el = pai;
        if el.is_null() {
            return None;
        }
    }
    soltar(el);
    achado
}

/// Teste: o que a acessibilidade acha no ponto (x, y) da tela.
pub fn testar(x: i32, y: i32) -> String {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        CoInitializeEx(std::ptr::null(), 0);
        let mut uia: *mut c_void = std::ptr::null_mut();
        if CoCreateInstance(&CLSID_CUIAUTOMATION, std::ptr::null_mut(), 1, &IID_IUIAUTOMATION, &mut uia) != 0 || uia.is_null() {
            return "sem UI Automation".into();
        }
        let elemento_em: ElementoEm = metodo(uia, 7);
        let andador: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32 = metodo(uia, 14);
        let mut arvore: *mut c_void = std::ptr::null_mut();
        andador(uia, &mut arvore);
        let t = std::time::Instant::now();
        let r = consultar(uia, elemento_em, arvore, POINT { x, y }, GetCurrentProcessId());
        format!("({x}, {y}) -> {:?} em {:.1} ms", r.map(|r| (r.left, r.top, r.right, r.bottom)), t.elapsed().as_secs_f64() * 1000.0)
    }
}
