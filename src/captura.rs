// Fundo para os temas de vidro: uma linha de execução própria copia a área da tela em
// volta do ponteiro e guarda ela nítida e desfocada. Copiar a tela leva ~6 ms (o Windows
// espera a composição), por isso fica fora do laço de desenho; para o que aparece através
// do vidro, algumas dezenas de milissegundos de atraso não se notam.
// As janelas do FlowCursor ficam de fora da cópia (SetWindowDisplayAffinity), senão o
// vidro mostraria o próprio ponteiro. Depois de algumas horas (visto no Windows 26300)
// essa exclusão às vezes passa a devolver preto no lugar da janela, e o vidro ficava preto:
// a cópia confere isso e pede ao laço de desenho que refaça a exclusão.
use crate::ffi::*;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Lado da área copiada, em pixels da tela: folga para o ponteiro andar entre duas cópias.
/// Sobre um botão maior que isso, a área cresce até cobrir o botão inteiro com folga.
const LADO: usize = 384;
/// Folga em volta do botão (a pílula passa um pouco dele e anda com o mouse).
const FOLGA_BOTAO: i32 = 48;
/// Maior área copiada, para um "botão" do tamanho da tela não pesar.
const MAXIMO: (i32, i32) = (2048, 1024);
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
    /// Desfocado, em meia resolução (`bw` x `bh`), cores 0..1.
    borrado: Vec<[f32; 3]>,
    bw: usize,
    bh: usize,
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
        let (a, b) = ((u0 as usize).min(self.bw - 1), (v0 as usize).min(self.bh - 1));
        let (a1, b1) = ((a + 1).min(self.bw - 1), (b + 1).min(self.bh - 1));
        let p = |x: usize, y: usize| self.borrado[y * self.bw + x];
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
/// Janela do desenho (para conferir se ela saiu preta na cópia).
static JANELA: AtomicIsize = AtomicIsize::new(0);
/// A cópia viu a janela do desenho preta: o laço de desenho refaz a exclusão.
pub static EXCLUSAO_ESTRAGOU: AtomicBool = AtomicBool::new(false);
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
        let dc = CreateCompatibleDC(0);
        let mut copia = Copia { dc, dib: 0, antigo: 0, bits: std::ptr::null_mut(), cap: (0, 0) };
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
            let (x, y, w, h) = area(pt);
            if let Some(px) = copia.copiar(x, y, w, h) {
                if janela_saiu_preta(px, x, y, w, h) {
                    // não usa esta cópia: o vidro mostraria o preto
                    EXCLUSAO_ESTRAGOU.store(true, Relaxed);
                } else {
                    let f = Fundo::novo(x, y, w, h, px.to_vec());
                    *ULTIMO.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(f));
                }
            }
            let intervalo = if parado { INTERVALO_PARADO } else { INTERVALO_MOVENDO };
            if let Some(resto) = intervalo.checked_sub(inicio.elapsed()) {
                std::thread::sleep(resto);
            }
        }
        copia.soltar();
        DeleteDC(dc);
    });
}

/// Área a copiar: `LADO` x `LADO` em volta do mouse, aumentada para cobrir o botão sob ele
/// (a pílula tem o tamanho do botão; fora da área ela ficava sem fundo nas pontas), e
/// sempre dentro do monitor: o que cai fora da tela vem preto e o desfoque espalhava esse
/// preto pelo vidro perto da borda.
fn area(pt: POINT) -> (i32, i32, usize, usize) {
    let l = LADO as i32;
    let (mut x0, mut y0, mut x1, mut y1) = (pt.x - l / 2, pt.y - l / 2, pt.x + l / 2, pt.y + l / 2);
    if let Some(b) = crate::alvo::sob(pt.x, pt.y) {
        x0 = x0.min(b.left - FOLGA_BOTAO).max(pt.x - MAXIMO.0 / 2);
        y0 = y0.min(b.top - FOLGA_BOTAO).max(pt.y - MAXIMO.1 / 2);
        x1 = x1.max(b.right + FOLGA_BOTAO).min(pt.x + MAXIMO.0 / 2);
        y1 = y1.max(b.bottom + FOLGA_BOTAO).min(pt.y + MAXIMO.1 / 2);
    }
    let (mut w, mut h) = (x1 - x0, y1 - y0);
    if let Some((m, _)) = crate::sistema::info_monitor(unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) }) {
        w = w.min(m.right - m.left);
        h = h.min(m.bottom - m.top);
        x0 = x0.min(m.right - w).max(m.left);
        y0 = y0.min(m.bottom - h).max(m.top);
    }
    (x0, y0, w.max(2) as usize, h.max(2) as usize)
}

/// Bitmap onde o BitBlt copia; é refeito quando a área muda de tamanho.
struct Copia {
    dc: HANDLE,
    dib: HANDLE,
    antigo: HANDLE,
    bits: *mut u32,
    cap: (usize, usize),
}

impl Copia {
    /// Copia a área da tela; devolve os pixels (BGRA), `w` por linha.
    unsafe fn copiar(&mut self, x: i32, y: i32, w: usize, h: usize) -> Option<&[u32]> {
        if (w, h) != self.cap {
            self.soltar();
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: 40, biWidth: w as i32, biHeight: -(h as i32), biPlanes: 1, biBitCount: 32, ..Default::default() },
                bmiColors: [0],
            };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(0, &bmi, 0, &mut bits, 0, 0);
            if dib == 0 || bits.is_null() {
                return None;
            }
            self.antigo = SelectObject(self.dc, dib);
            (self.dib, self.bits, self.cap) = (dib, bits.cast(), (w, h));
        }
        let tela = GetDC(0);
        let ok = BitBlt(self.dc, 0, 0, w as i32, h as i32, tela, x, y, SRCCOPY) != 0;
        ReleaseDC(0, tela);
        ok.then(|| std::slice::from_raw_parts(self.bits as *const u32, w * h))
    }

    unsafe fn soltar(&mut self) {
        if self.dib != 0 {
            SelectObject(self.dc, self.antigo);
            DeleteObject(self.dib);
            self.dib = 0;
            self.cap = (0, 0);
        }
    }
}

impl Fundo {
    /// Guarda a cópia nítida e faz a desfocada.
    fn novo(x: i32, y: i32, w: usize, h: usize, nitido: Vec<u32>) -> Fundo {
        // média de 2x2 para a meia resolução, depois três passadas de caixa (~gaussiano)
        let (bw, bh) = (w / 2, h / 2);
        let canal = |c: u32, k: u32| ((c >> k) & 255) as f32 / 255.0;
        let mut borrado = vec![[0.0f32; 3]; bw * bh];
        for j in 0..bh {
            for i in 0..bw {
                let mut s = [0.0f32; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let c = nitido[(2 * j + dy) * w + 2 * i + dx];
                    s[0] += canal(c, 16);
                    s[1] += canal(c, 8);
                    s[2] += canal(c, 0);
                }
                borrado[j * bw + i] = s.map(|v| v * 0.25);
            }
        }
        let mut tmp = vec![[0.0f32; 3]; bw * bh];
        for _ in 0..3 {
            caixa(&borrado, &mut tmp, bw, bh, 3, true);
            caixa(&tmp, &mut borrado, bw, bh, 3, false);
        }
        Fundo { x, y, w, h, nitido, borrado, bw, bh }
    }
}

/// A janela do desenho veio preta na cópia? Quase tudo preto puro dentro dela e não em
/// volta (um vídeo ou uma tela preta de verdade é preto dos dois lados).
fn janela_saiu_preta(px: &[u32], x: i32, y: i32, w: usize, h: usize) -> bool {
    let hwnd = JANELA.load(Relaxed);
    if hwnd == 0 || unsafe { IsWindowVisible(hwnd) } == 0 {
        return false;
    }
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) };
    preta_no_retangulo(px, x, y, w, h, r)
}

fn preta_no_retangulo(px: &[u32], x: i32, y: i32, w: usize, h: usize, r: RECT) -> bool {
    let preto = |i: i32, j: i32| -> Option<bool> {
        let (u, v) = (i - x, j - y);
        (u >= 0 && v >= 0 && (u as usize) < w && (v as usize) < h).then(|| px[v as usize * w + u as usize] & 0xff_ffff == 0)
    };
    let (mut dentro, mut pretos_dentro, mut fora, mut pretos_fora) = (0u32, 0u32, 0u32, 0u32);
    // de 2 em 2 pixels basta; "fora" é uma faixa de 4 px em volta
    for j in (r.top - 4..r.bottom + 4).step_by(2) {
        for i in (r.left - 4..r.right + 4).step_by(2) {
            let Some(p) = preto(i, j) else { continue };
            if i >= r.left && i < r.right && j >= r.top && j < r.bottom {
                dentro += 1;
                pretos_dentro += p as u32;
            } else {
                fora += 1;
                pretos_fora += p as u32;
            }
        }
    }
    dentro >= 100 && pretos_dentro * 100 >= dentro * 97 && fora >= 20 && pretos_fora * 2 < fora
}

#[cfg(test)]
impl Fundo {
    pub fn de_pixels(x: i32, y: i32, w: usize, h: usize, px: Vec<u32>) -> Fundo {
        Fundo::novo(x, y, w, h, px)
    }
}

/// Para testes: copia uma área de `w` x `h` com o canto em (x, y) da tela.
#[cfg(test)]
pub fn de_tela_area(x: i32, y: i32, w: usize, h: usize) -> Fundo {
    let mut c = Copia { dc: unsafe { CreateCompatibleDC(0) }, dib: 0, antigo: 0, bits: std::ptr::null_mut(), cap: (0, 0) };
    let px = unsafe { c.copiar(x, y, w, h) }.expect("BitBlt").to_vec();
    unsafe {
        c.soltar();
        DeleteDC(c.dc);
    }
    Fundo::novo(x, y, w, h, px)
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
        Fundo::novo(x, y, LADO, LADO, nitido)
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
pub fn excluir_da_captura(hwnd: HWND, excluir: bool) -> bool {
    JANELA.store(if excluir { hwnd } else { 0 }, Relaxed);
    unsafe { SetWindowDisplayAffinity(hwnd, if excluir { WDA_EXCLUDEFROMCAPTURE } else { 0 }) != 0 }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn reconhece_a_janela_preta_so_quando_o_resto_nao_e_preto() {
        let (w, h) = (200usize, 200usize);
        let r = RECT { left: 1050, top: 2050, right: 1100, bottom: 2100 };
        let mut px = vec![0x0030_3030u32; w * h];
        for j in 50..100 {
            for i in 50..100 {
                px[j * w + i] = 0;
            }
        }
        assert!(preta_no_retangulo(&px, 1000, 2000, w, h, r));
        // tela toda preta (vídeo): não é a exclusão
        assert!(!preta_no_retangulo(&vec![0u32; w * h], 1000, 2000, w, h, r));
        // conteúdo de verdade atrás da janela
        assert!(!preta_no_retangulo(&vec![0x0030_3030u32; w * h], 1000, 2000, w, h, r));
    }

    #[test]
    fn fundo_de_tamanho_qualquer() {
        let (w, h) = (701usize, 203usize);
        let f = Fundo::novo(10, 20, w, h, vec![0x00ff_0000u32; w * h]);
        let (n, b) = f.em(10 + 700, 20 + 202).unwrap();
        assert_eq!(n, [1.0, 0.0, 0.0]);
        assert!((b[0] - 1.0).abs() < 1e-4);
        assert!(f.em(10 + 701, 20).is_none());
    }
}
