// Janela transparente onde o ponteiro é desenhado: sempre na frente, não recebe
// cliques, não aparece na barra de tarefas nem no Alt+Tab.
use crate::ffi::*;
use crate::motor::Quadro;
use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::{null, null_mut};

const CLASSE: &str = "FlowCursorCamada";

unsafe extern "system" fn procedimento(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        return HTTRANSPARENT;
    }
    DefWindowProcW(h, msg, wp, lp)
}

pub struct Overlay {
    hwnd: HWND,
    dc: HANDLE,
    dib: HANDLE,
    dib_antigo: HANDLE,
    bits: *mut u32,
    cap_w: usize,
    cap_h: usize,
    visivel: bool,
    barreiras: HashSet<HWND>,
}

impl Overlay {
    pub fn criar() -> Option<Self> {
        unsafe {
            let inst = GetModuleHandleW(null());
            let nome = w(CLASSE);
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
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                nome.as_ptr(),
                w("FlowCursor").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                0,
                0,
                inst,
                null(),
            );
            if hwnd == 0 {
                return None;
            }
            // Sem a animação de abrir/fechar janela do Windows: o ponteiro tem que aparecer na hora.
            let sim: i32 = 1;
            DwmSetWindowAttribute(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, (&sim as *const i32).cast(), 4);
            let dc = CreateCompatibleDC(0);
            let mut o = Overlay { hwnd, dc, dib: 0, dib_antigo: 0, bits: null_mut(), cap_w: 0, cap_h: 0, visivel: false, barreiras: HashSet::new() };
            if !o.garantir(256, 256) {
                return None;
            }
            Some(o)
        }
    }

    /// Garante um bitmap de pelo menos w x h (cresce em blocos de 128 px).
    fn garantir(&mut self, w: usize, h: usize) -> bool {
        if w <= self.cap_w && h <= self.cap_h {
            return true;
        }
        let nw = w.max(self.cap_w).div_ceil(128) * 128;
        let nh = h.max(self.cap_h).div_ceil(128) * 128;
        unsafe {
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: 40,
                    biWidth: nw as i32,
                    biHeight: -(nh as i32), // de cima para baixo
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                bmiColors: [0],
            };
            let mut bits: *mut c_void = null_mut();
            let dib = CreateDIBSection(self.dc, &bmi, 0, &mut bits, 0, 0);
            if dib == 0 || bits.is_null() {
                return false;
            }
            let antigo = SelectObject(self.dc, dib);
            if self.dib != 0 {
                DeleteObject(self.dib);
            } else {
                self.dib_antigo = antigo;
            }
            self.dib = dib;
            self.bits = bits.cast();
            self.cap_w = nw;
            self.cap_h = nh;
        }
        true
    }

    pub fn apresentar(&mut self, q: &Quadro) -> bool {
        if !self.garantir(q.w, q.h) {
            return false;
        }
        unsafe {
            let destino = std::slice::from_raw_parts_mut(self.bits, self.cap_w * self.cap_h);
            for y in 0..q.h {
                destino[y * self.cap_w..y * self.cap_w + q.w].copy_from_slice(&q.px[y * q.w..(y + 1) * q.w]);
            }
            let mistura = BLENDFUNCTION { BlendOp: 0, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA };
            let pos = POINT { x: q.x, y: q.y };
            let tam = SIZE { cx: q.w as i32, cy: q.h as i32 };
            let origem = POINT { x: 0, y: 0 };
            let ok = UpdateLayeredWindow(self.hwnd, 0, &pos, &tam, self.dc, &origem, 0, &mistura, ULW_ALPHA) != 0;
            if !self.visivel {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.trazer_para_frente();
                self.visivel = true;
            }
            ok
        }
    }

    pub fn esconder(&mut self) {
        if self.visivel {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.visivel = false;
        }
    }

    fn trazer_para_frente(&self) {
        unsafe {
            SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSENDCHANGING,
            );
        }
    }

    /// Menus de contexto, dicas e a barra de tarefas também são "sempre visíveis"
    /// e passam na frente quando abrem. Se houver janela visível acima, volta ao topo.
    /// Acima do topo possível ficam as camadas do próprio Windows, que não dá para
    /// passar: a primeira janela que continua acima depois de subir vira a "barreira",
    /// e dali para cima nada é disputado. Devolve a janela que estava acima e a nova
    /// barreira, se houver.
    pub fn manter_no_topo(&mut self) -> Option<(HWND, Option<HWND>)> {
        if !self.visivel {
            return None;
        }
        unsafe {
            let mut acima = GetWindow(self.hwnd, GW_HWNDPREV);
            let mut passos = 0;
            while acima != 0 && passos < 64 {
                if self.barreiras.contains(&acima) {
                    return None;
                }
                if IsWindowVisible(acima) != 0 {
                    self.trazer_para_frente();
                    let novo = GetWindow(self.hwnd, GW_HWNDPREV);
                    let barreira = (novo != 0 && self.barreiras.insert(novo)).then_some(novo);
                    return Some((acima, barreira));
                }
                acima = GetWindow(acima, GW_HWNDPREV);
                passos += 1;
            }
        }
        None
    }

    /// Atende as mensagens da janela (o Windows espera que toda janela responda).
    pub fn bombear(&self) {
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
            if self.dib != 0 {
                SelectObject(self.dc, self.dib_antigo);
                DeleteObject(self.dib);
            }
            DeleteDC(self.dc);
        }
    }
}
