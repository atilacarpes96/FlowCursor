// Fundo para os temas de vidro: uma linha de execução própria copia a área da tela em
// volta do ponteiro e guarda ela nítida e desfocada. Copiar a tela leva ~6 ms (o Windows
// espera a composição), por isso fica fora do laço de desenho; para o que aparece através
// do vidro, algumas dezenas de milissegundos de atraso não se notam.
// As janelas do FlowCursor ficam de fora da cópia (SetWindowDisplayAffinity), senão o
// vidro mostraria o próprio ponteiro.
use crate::ffi::*;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Lado da área copiada, em pixels da tela: folga para o ponteiro andar entre duas cópias.
const LADO: usize = 384;
/// O desfoque é feito em meia resolução (fica igual e custa um quarto).
const MEIO: usize = LADO / 2;
/// Com o mouse andando, ~40 cópias por segundo; parado, 8 (para vídeo ou rolagem atrás).
const INTERVALO_MOVENDO: Duration = Duration::from_millis(25);
const INTERVALO_PARADO: Duration = Duration::from_millis(125);

pub struct Fundo {
    /// Canto superior esquerdo da área na tela.
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    /// Pixels como vieram da tela (BGRA).
    nitido: Vec<u32>,
    /// Desfocado, em meia resolução, cores 0..1.
    borrado: Vec<[f32; 3]>,
}

impl Fundo {
    /// Cor na posição da tela (nítida, desfocada), se estiver dentro da área copiada.
    pub fn em(&self, x: i32, y: i32) -> Option<([f32; 3], [f32; 3])> {
        let (i, j) = (x - self.x, y - self.y);
        if i < 0 || j < 0 || i >= self.w as i32 || j >= self.h as i32 {
            return None;
        }
        let c = self.nitido[j as usize * self.w + i as usize];
        let canal = |k: u32| ((c >> k) & 255) as f32 / 255.0;
        Some(([canal(16), canal(8), canal(0)], self.borrado_em(i as f32, j as f32)))
    }

    /// Cor nítida num ponto qualquer da tela (com fração de pixel), por interpolação
    /// bicúbica (Catmull-Rom, 4x4 vizinhos). Ampliar pegando o pixel mais próximo deixa
    /// a imagem em degraus; a bicúbica mantém letras e bordas lisas e ainda definidas.
    pub fn nitido_suave(&self, x: f32, y: f32) -> Option<[f32; 3]> {
        let (u, v) = (x - self.x as f32 - 0.5, y - self.y as f32 - 0.5);
        let (u0, v0) = (u.floor(), v.floor());
        let (a, b) = (u0 as i32, v0 as i32);
        if a < 1 || b < 1 || a + 2 >= self.w as i32 || b + 2 >= self.h as i32 {
            return None;
        }
        let pesos = |t: f32| {
            let (t2, t3) = (t * t, t * t * t);
            [
                (-t3 + 2.0 * t2 - t) * 0.5,
                (3.0 * t3 - 5.0 * t2 + 2.0) * 0.5,
                (-3.0 * t3 + 4.0 * t2 + t) * 0.5,
                (t3 - t2) * 0.5,
            ]
        };
        let (wx, wy) = (pesos(u - u0), pesos(v - v0));
        let mut r = [0.0f32; 3];
        for (dj, py) in wy.iter().enumerate() {
            let linha = (b - 1 + dj as i32) as usize * self.w;
            for (di, px) in wx.iter().enumerate() {
                let c = self.nitido[linha + (a - 1 + di as i32) as usize];
                let p = px * py;
                r[0] += ((c >> 16) & 255) as f32 * p;
                r[1] += ((c >> 8) & 255) as f32 * p;
                r[2] += (c & 255) as f32 * p;
            }
        }
        Some(r.map(|c| (c / 255.0).clamp(0.0, 1.0)))
    }

    fn borrado_em(&self, i: f32, j: f32) -> [f32; 3] {
        let (u, v) = ((i + 0.5) / 2.0 - 0.5, (j + 0.5) / 2.0 - 0.5);
        let (u0, v0) = (u.floor().max(0.0), v.floor().max(0.0));
        let (fu, fv) = ((u - u0).clamp(0.0, 1.0), (v - v0).clamp(0.0, 1.0));
        let (a, b) = ((u0 as usize).min(MEIO - 1), (v0 as usize).min(MEIO - 1));
        let (a1, b1) = ((a + 1).min(MEIO - 1), (b + 1).min(MEIO - 1));
        let p = |x: usize, y: usize| self.borrado[y * MEIO + x];
        let (c00, c10, c01, c11) = (p(a, b), p(a1, b), p(a, b1), p(a1, b1));
        let mut r = [0.0; 3];
        for k in 0..3 {
            let top = c00[k] + (c10[k] - c00[k]) * fu;
            let baixo = c01[k] + (c11[k] - c01[k]) * fu;
            r[k] = top + (baixo - top) * fv;
        }
        r
    }
}

pub static ATIVA: AtomicBool = AtomicBool::new(false);
static ULTIMO: Mutex<Option<Arc<Fundo>>> = Mutex::new(None);

pub fn atual() -> Option<Arc<Fundo>> {
    if !ATIVA.load(Relaxed) {
        return None;
    }
    ULTIMO.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Começa a linha de execução (uma vez); ela só copia enquanto `ATIVA` estiver ligada.
pub fn iniciar(sair: &'static AtomicBool) {
    std::thread::spawn(move || unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: 40, biWidth: LADO as i32, biHeight: -(LADO as i32), biPlanes: 1, biBitCount: 32, ..Default::default() },
            bmiColors: [0],
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(0, &bmi, 0, &mut bits, 0, 0);
        if dib == 0 || bits.is_null() {
            return;
        }
        let dc = CreateCompatibleDC(0);
        let antigo = SelectObject(dc, dib);
        let px = std::slice::from_raw_parts(bits as *const u32, LADO * LADO);
        let mut tmp = vec![[0.0f32; 3]; MEIO * MEIO];
        let mut pt_ant = POINT { x: i32::MIN, y: 0 };
        while !sair.load(Relaxed) {
            if !ATIVA.load(Relaxed) {
                *ULTIMO.lock().unwrap_or_else(|e| e.into_inner()) = None;
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            let inicio = Instant::now();
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            let parado = pt.x == pt_ant.x && pt.y == pt_ant.y;
            pt_ant = pt;
            let (x, y) = (pt.x - LADO as i32 / 2, pt.y - LADO as i32 / 2);
            let tela = GetDC(0);
            let ok = BitBlt(dc, 0, 0, LADO as i32, LADO as i32, tela, x, y, SRCCOPY) != 0;
            ReleaseDC(0, tela);
            if ok {
                // média de 2x2 para a meia resolução, depois três passadas de caixa (~gaussiano)
                let canal = |c: u32, k: u32| ((c >> k) & 255) as f32 / 255.0;
                let mut borrado = vec![[0.0f32; 3]; MEIO * MEIO];
                for j in 0..MEIO {
                    for i in 0..MEIO {
                        let mut s = [0.0f32; 3];
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let c = px[(2 * j + dy) * LADO + 2 * i + dx];
                            s[0] += canal(c, 16);
                            s[1] += canal(c, 8);
                            s[2] += canal(c, 0);
                        }
                        borrado[j * MEIO + i] = s.map(|v| v * 0.25);
                    }
                }
                for _ in 0..3 {
                    caixa(&borrado, &mut tmp, MEIO, MEIO, 3, true);
                    caixa(&tmp, &mut borrado, MEIO, MEIO, 3, false);
                }
                let f = Fundo { x, y, w: LADO, h: LADO, nitido: px.to_vec(), borrado };
                *ULTIMO.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(f));
            }
            let intervalo = if parado { INTERVALO_PARADO } else { INTERVALO_MOVENDO };
            if let Some(resto) = intervalo.checked_sub(inicio.elapsed()) {
                std::thread::sleep(resto);
            }
        }
        SelectObject(dc, antigo);
        DeleteDC(dc);
        DeleteObject(dib);
    });
}

/// Para testes: copia a área de `LADO` x `LADO` com o canto em (x, y) da tela.
#[cfg(test)]
pub fn de_tela(x: i32, y: i32) -> Fundo {
    unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: 40, biWidth: LADO as i32, biHeight: -(LADO as i32), biPlanes: 1, biBitCount: 32, ..Default::default() },
            bmiColors: [0],
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(0, &bmi, 0, &mut bits, 0, 0);
        let dc = CreateCompatibleDC(0);
        let antigo = SelectObject(dc, dib);
        let tela = GetDC(0);
        BitBlt(dc, 0, 0, LADO as i32, LADO as i32, tela, x, y, SRCCOPY);
        ReleaseDC(0, tela);
        let nitido = std::slice::from_raw_parts(bits as *const u32, LADO * LADO).to_vec();
        SelectObject(dc, antigo);
        DeleteDC(dc);
        DeleteObject(dib);
        Fundo { x, y, w: LADO, h: LADO, nitido, borrado: vec![[0.0; 3]; MEIO * MEIO] }
    }
}

/// Média móvel de raio `r` numa direção.
fn caixa(origem: &[[f32; 3]], destino: &mut [[f32; 3]], w: usize, h: usize, r: usize, horizontal: bool) {
    let (n, linhas) = if horizontal { (w, h) } else { (h, w) };
    let idx = |linha: usize, i: usize| if horizontal { linha * w + i } else { i * w + linha };
    let k = 1.0 / (2 * r + 1) as f32;
    for linha in 0..linhas {
        let mut soma = [0.0f32; 3];
        for i in 0..=r {
            let c = origem[idx(linha, i.min(n - 1))];
            for q in 0..3 {
                soma[q] += c[q] * if i == 0 { r as f32 + 1.0 } else { 1.0 };
            }
        }
        for i in 0..n {
            destino[idx(linha, i)] = soma.map(|s| s * k);
            let entra = origem[idx(linha, (i + r + 1).min(n - 1))];
            let sai = origem[idx(linha, i.saturating_sub(r))];
            for q in 0..3 {
                soma[q] += entra[q] - sai[q];
            }
        }
    }
}

/// Liga ou desliga a exclusão das janelas do FlowCursor das cópias de tela.
/// Com ela ligada, gravações e capturas de tela também não mostram o ponteiro desenhado.
pub fn excluir_da_captura(hwnd: HWND, excluir: bool) {
    unsafe { SetWindowDisplayAffinity(hwnd, if excluir { WDA_EXCLUDEFROMCAPTURE } else { 0 }) };
}
